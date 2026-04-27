//! Image decoding and loading for embedding in PDFs.
//!
//! Supports two formats:
//!
//! - **JPEG**: passed through to the PDF as a `DCTDecode` stream. We only
//!   parse the SOF marker to extract width/height and component count.
//! - **PNG**: decoded to raw pixels using the `png` crate, optionally
//!   compressed with `miniz_oxide`, and written as a `FlateDecode` stream.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFilter {
    /// Raw JPEG bytes (PDF's `DCTDecode`).
    Dct,
    /// Deflate-compressed raw pixels (PDF's `FlateDecode`).
    Flate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageColorSpace {
    DeviceGray,
    DeviceRgb,
}

#[derive(Clone)]
pub struct ImageData {
    pub width: u32,
    pub height: u32,
    pub color_space: ImageColorSpace,
    pub bits_per_component: u8,
    pub filter: ImageFilter,
    /// Raw bytes of the stream (already in target format).
    pub data: Vec<u8>,
    /// Optional soft mask (alpha channel) as a separate grayscale image.
    pub alpha_mask: Option<Box<ImageData>>,
}

/// Load an image from a local file. Selects the decoder based on the
/// magic bytes of the file contents, not the extension. Retained for
/// callers that have a `Path` already; new code should prefer
/// `load_from_source` so it goes through the URL fetcher.
#[allow(dead_code)]
pub fn load_from_path(path: &Path) -> Result<ImageData, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read image {}: {e}", path.display()))?;
    decode_bytes(&bytes)
}

/// Load an image from any source the renderer might encounter — a bare
/// path, a `file://` URL, or (with the `http-fetch` feature) an HTTP/S
/// URL. Routes through the supplied `UrlFetcher` so server-side callers
/// can plug in their own connection pool / auth / cache.
#[allow(dead_code)] // Wired into html_render call sites by a later rung.
pub fn load_from_source(
    src: &str,
    base_dir: Option<&std::path::Path>,
    fetcher: &dyn crate::url_fetcher::UrlFetcher,
) -> Result<ImageData, String> {
    let resource = fetcher.fetch(src, base_dir)?;
    decode_bytes(&resource.bytes)
}

/// Decode an image from its raw bytes based on magic-byte sniffing.
pub fn decode_bytes(bytes: &[u8]) -> Result<ImageData, String> {
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        decode_jpeg(bytes)
    } else if bytes.len() >= 8 && bytes[0..8] == [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
        decode_png(bytes)
    } else {
        Err("unsupported image format (expected JPEG or PNG)".to_string())
    }
}

/// Parse a JPEG's SOF marker to get dimensions, then return the image
/// data as a `DCTDecode` stream (passthrough, no re-encoding).
fn decode_jpeg(bytes: &[u8]) -> Result<ImageData, String> {
    let mut i = 0;
    // Require SOI
    if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return Err("not a valid JPEG (missing SOI)".to_string());
    }
    i += 2;

    while i + 3 < bytes.len() {
        // Skip any padding 0xFF bytes
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;

        // Standalone markers (no length): RST0-7 (0xD0-D7), SOI (0xD8), EOI (0xD9)
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            continue;
        }

        if i + 1 >= bytes.len() {
            return Err("truncated JPEG".to_string());
        }
        let seg_len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;

        // SOF markers (Start Of Frame): 0xC0 = baseline, 0xC1 = extended seq,
        // 0xC2 = progressive, and more in range 0xC0-0xCF except 0xC4, 0xC8, 0xCC.
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;

        if is_sof {
            if i + 7 >= bytes.len() {
                return Err("truncated SOF marker".to_string());
            }
            let precision = bytes[i + 2];
            let height = u32::from(u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]));
            let width = u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]));
            let components = bytes[i + 7];
            let color_space = match components {
                1 => ImageColorSpace::DeviceGray,
                3 => ImageColorSpace::DeviceRgb,
                _ => {
                    return Err(format!("unsupported JPEG component count: {components}"));
                }
            };
            return Ok(ImageData {
                width,
                height,
                color_space,
                bits_per_component: precision,
                filter: ImageFilter::Dct,
                data: bytes.to_vec(),
                alpha_mask: None,
            });
        }

        // Skip past this segment
        i += seg_len;
    }
    Err("no SOF marker found in JPEG".to_string())
}

/// Decode a PNG using the `png` crate and return an `ImageData` with
/// deflate-compressed raw pixels.
fn decode_png(bytes: &[u8]) -> Result<ImageData, String> {
    use png::ColorType;

    let decoder = png::Decoder::new(bytes);
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG decode failed: {e}"))?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let bit_depth = info.bit_depth as u8;
    let color_type = info.color_type;

    // The png crate gives us the frame in (color_type, bit_depth) — we need
    // to normalize to DeviceRGB or DeviceGray, 8bpc, without alpha inline.
    let mut buf = vec![0; reader.output_buffer_size()];
    let frame = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG decode failed: {e}"))?;
    buf.truncate(frame.buffer_size());

    let out_bpc = 8; // png crate expands to 8bpc for most inputs; see below
    let _ = bit_depth;

    // Produce (rgb_bytes, optional alpha_bytes)
    let (rgb_bytes, alpha_bytes, color_space) = match color_type {
        ColorType::Grayscale => (buf, None, ImageColorSpace::DeviceGray),
        ColorType::GrayscaleAlpha => {
            // Split interleaved G,A pairs
            let mut g = Vec::with_capacity(buf.len() / 2);
            let mut a = Vec::with_capacity(buf.len() / 2);
            for pair in buf.chunks_exact(2) {
                g.push(pair[0]);
                a.push(pair[1]);
            }
            (g, Some(a), ImageColorSpace::DeviceGray)
        }
        ColorType::Rgb => (buf, None, ImageColorSpace::DeviceRgb),
        ColorType::Rgba => {
            // Split interleaved RGBA into RGB + A
            let mut rgb = Vec::with_capacity((buf.len() / 4) * 3);
            let mut a = Vec::with_capacity(buf.len() / 4);
            for px in buf.chunks_exact(4) {
                rgb.push(px[0]);
                rgb.push(px[1]);
                rgb.push(px[2]);
                a.push(px[3]);
            }
            (rgb, Some(a), ImageColorSpace::DeviceRgb)
        }
        ColorType::Indexed => {
            return Err("indexed PNGs are not yet supported".to_string());
        }
    };

    let compressed = compress(&rgb_bytes);
    let alpha_mask = alpha_bytes.map(|a| {
        let alpha_compressed = compress(&a);
        Box::new(ImageData {
            width,
            height,
            color_space: ImageColorSpace::DeviceGray,
            bits_per_component: 8,
            filter: ImageFilter::Flate,
            data: alpha_compressed,
            alpha_mask: None,
        })
    });

    Ok(ImageData {
        width,
        height,
        color_space,
        bits_per_component: out_bpc,
        filter: ImageFilter::Flate,
        data: compressed,
        alpha_mask,
    })
}

pub(crate) fn compress(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(data, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reject_unknown_format() {
        match decode_bytes(b"not an image") {
            Err(e) => assert!(e.contains("unsupported")),
            Ok(_) => panic!("expected an error"),
        }
    }

    #[test]
    fn reject_truncated_jpeg() {
        // Starts with the JPEG magic but has no SOF.
        match decode_bytes(&[0xFF, 0xD8, 0xFF, 0xE0]) {
            Err(e) => assert!(!e.is_empty()),
            Ok(_) => panic!("expected an error"),
        }
    }

    /// Build a minimal 2x1 8-bit RGB PNG on the fly and round-trip it.
    fn tiny_rgb_png() -> Vec<u8> {
        let pixels = vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00]; // red, green
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        out
    }

    /// Build a 2x1 RGBA PNG so we can verify alpha-mask extraction.
    fn tiny_rgba_png() -> Vec<u8> {
        let pixels = vec![0xFF, 0x00, 0x00, 0x80, 0x00, 0xFF, 0x00, 0xFF];
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
        }
        out
    }

    #[test]
    fn decode_rgb_png() {
        let bytes = tiny_rgb_png();
        let img = decode_bytes(&bytes).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        assert_eq!(img.color_space, ImageColorSpace::DeviceRgb);
        assert_eq!(img.bits_per_component, 8);
        assert_eq!(img.filter, ImageFilter::Flate);
        assert!(img.alpha_mask.is_none());
        assert!(!img.data.is_empty());
    }

    #[test]
    fn load_from_source_routes_through_fetcher() {
        // Write a tiny PNG to a temp file, then ask `load_from_source`
        // to read it through the DefaultFetcher (file:// path). Same
        // bytes back as `load_from_path`.
        let bytes = tiny_rgb_png();
        let mut path = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!(
            "pdfun-img-src-{}-{}.png",
            std::process::id(),
            nanos
        ));
        std::fs::write(&path, &bytes).unwrap();
        let img = load_from_source(
            path.to_str().unwrap(),
            None,
            &crate::url_fetcher::DefaultFetcher,
        )
        .expect("loads via fetcher");
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    #[cfg(not(feature = "http-fetch"))]
    fn load_from_source_errors_on_http_without_feature() {
        // Without the feature: the DefaultFetcher returns the canonical
        // "feature not enabled" error and `load_from_source` propagates
        // it unchanged.
        let res = load_from_source(
            "http://example.invalid/x.png",
            None,
            &crate::url_fetcher::DefaultFetcher,
        );
        let Err(err) = res else {
            panic!("expected an error, got Ok");
        };
        assert!(
            err.contains("http-fetch feature not enabled"),
            "got {err:?}"
        );
    }

    #[test]
    fn decode_rgba_png_splits_alpha_mask() {
        let bytes = tiny_rgba_png();
        let img = decode_bytes(&bytes).unwrap();
        assert_eq!(img.width, 2);
        assert_eq!(img.color_space, ImageColorSpace::DeviceRgb);
        let mask = img.alpha_mask.expect("RGBA PNG should produce an SMask");
        assert_eq!(mask.width, 2);
        assert_eq!(mask.height, 1);
        assert_eq!(mask.color_space, ImageColorSpace::DeviceGray);
        assert_eq!(mask.filter, ImageFilter::Flate);
        assert!(mask.alpha_mask.is_none());
    }
}
