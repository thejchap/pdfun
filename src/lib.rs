//! pdfun — pure-Rust HTML/CSS to PDF renderer, exposed to Python via `PyO3`.
//!
//! Pipeline: HTML (`scraper`) → CSS cascade (`css`) → box tree (`box_tree`) →
//! layout (`layout`) → paint ops → PDF bytes (`pdf-writer`). See the
//! architecture page at <https://thejchap.github.io/pdfun/architecture/>.

#![allow(clippy::cast_possible_truncation)] // f64->f32 at PyO3 boundary is intentional
#![allow(clippy::cast_precision_loss)] // u32/usize->f32 precision loss is acceptable
#![allow(clippy::cast_possible_wrap)] // usize->i32 for page count is fine
#![allow(clippy::unnecessary_wraps)] // PyO3 #[pymethods] requires PyResult signatures
#![allow(clippy::too_many_lines)] // layout/emit pipelines thread state; splitting hurts readability
#![allow(clippy::similar_names)] // layout code uses paired x/y, fg/bg, etc.
#![allow(clippy::many_single_char_names)] // color math reads better with r, g, b, h, s, l

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pdf_writer::types::{
    ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap,
};
use pdf_writer::{Content, Filter, Name, Pdf, Rect, Ref, Str};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

mod base64;
mod box_tree;
mod css;
mod dom;
mod font_face;
mod font_metrics;
mod html_render;
mod image;
mod layout;
mod winansi;

// ── Built-in PDF fonts ─────────────────────────────────────────

pub(crate) const BUILTIN_FONTS: &[&str] = &[
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

// ── Internal types ─────────────────────────────────────────────

#[derive(Clone)]
pub(crate) enum PdfOp {
    BeginText,
    EndText,
    SetFont {
        name: String,
        size: f32,
    },
    SetTextPosition {
        x: f32,
        y: f32,
    },
    /// Pre-transcoded PDF-WinAnsi byte string for the built-in font path.
    /// Built by `show_text_for` via `winansi::transcode_to_pdf_winansi`;
    /// emitted into the content stream wrapped in a PDF literal string.
    ShowText(Vec<u8>),
    /// glyph ids are resolved lazily in `to_bytes()`; stores (char,) pairs
    ShowGlyphs(Vec<char>),
    // Graphics primitives
    SetFillColor {
        r: f32,
        g: f32,
        b: f32,
    },
    SetStrokeColor {
        r: f32,
        g: f32,
        b: f32,
    },
    SetLineWidth(f32),
    Rectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// Rounded rectangle path. Radii are [top-left, top-right, bottom-right, bottom-left].
    /// Each corner is independently clamped to half of the shorter box edge.
    RoundedRectangle {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radii: [f32; 4],
    },
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    Stroke,
    Fill,
    /// Fill the current path with the even-odd rule (`f*`). Used for
    /// annular paths like inset `box-shadow`, where one rectangle is
    /// nested inside another and only the region between them should fill.
    FillEvenOdd,
    FillAndStroke,
    /// `W n`: intersect the current clipping path with the previously-built
    /// path using the nonzero winding rule, then discard the path (no
    /// fill or stroke). Used by `overflow: hidden` to clip children to
    /// the padding box.
    ClipNonzero,
    SaveState,
    RestoreState,
    SetDashPattern {
        array: Vec<f32>,
        phase: f32,
    },
    SetWordSpacing(f32),
    SetCharacterSpacing(f32),
    /// Draw image #index at (x, y) with given width/height (points).
    DrawImage {
        index: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    /// Apply a constant opacity (via `ExtGState`). The emitter allocates an
    /// `ExtGState` resource named `/GsN` that sets both `CA` (stroking alpha)
    /// and `ca` (non-stroking alpha) to `alpha` and references it with
    /// `/GsN gs`. Same `alpha` reuses the same resource on a given page.
    SetAlpha {
        alpha: f32,
    },
    /// Concat a pure translation `1 0 0 1 dx dy cm` onto the current
    /// transformation matrix. Used to implement `position: relative` offsets.
    Translate {
        dx: f32,
        dy: f32,
    },
}

pub(crate) struct LinkAnnotation {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) url: String,
}

/// A heading captured during layout for the PDF outline (bookmarks).
/// `level` is 1–6 matching `h1`–`h6`.
pub(crate) struct HeadingEntry {
    pub(crate) level: u8,
    pub(crate) text: String,
    pub(crate) page_index: usize,
    /// PDF-space y coordinate of the heading's top edge (post-margin),
    /// suitable for passing to `Destination::fit_horizontal`.
    pub(crate) y: f32,
}

pub(crate) struct PageContent {
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) operations: Vec<PdfOp>,
    pub(crate) fonts_used: Vec<String>,
    pub(crate) current_font: Option<String>,
    pub(crate) current_font_size: Option<f32>,
    pub(crate) links: Vec<LinkAnnotation>,
    /// Indices into `PdfDocument::images` of images used on this page.
    pub(crate) images_used: Vec<usize>,
}

impl PageContent {
    fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            operations: Vec::new(),
            fonts_used: Vec::new(),
            current_font: None,
            current_font_size: None,
            links: Vec::new(),
            images_used: Vec::new(),
        }
    }
}

/// Transcode `text` into PDF `WinAnsiEncoding` bytes, substituting `?`
/// per-char for codepoints outside the WinAnsi repertoire. WS-1A treats
/// non-mappable codepoints as unrenderable on built-in fonts; WS-1B
/// will replace this fallback with a split-and-promote onto a Unicode
/// face. The substitution is per-char so a single rogue codepoint in
/// the middle of an otherwise-Latin string only loses that one glyph.
pub(crate) fn transcode_with_fallback(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        match winansi::transcode_to_pdf_winansi(s) {
            Ok(mut bytes) => out.append(&mut bytes),
            Err(_) => out.push(b'?'),
        }
    }
    out
}

/// Escape a byte buffer for PDF literal string encoding (`(...)`).
/// Parentheses and backslashes must be escaped per ISO 32000-1 §7.3.4.2.
/// Accepts arbitrary bytes — used after WinAnsi transcoding so the input
/// is already in the encoding the built-in font expects.
fn pdf_escape(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'(' => out.extend_from_slice(b"\\("),
            b')' => out.extend_from_slice(b"\\)"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            _ => out.push(b),
        }
    }
    out
}

// ── Registered (embedded) fonts ────────────────────────────────

pub(crate) struct RegisteredFont {
    pub(crate) data: Vec<u8>,
    pub(crate) family: String,
    pub(crate) name: String, // e.g. "Custom-0"
}

// ── Indirect-ref allocator ─────────────────────────────────────

struct RefAllocator(i32);

impl RefAllocator {
    fn new() -> Self {
        Self(1)
    }
    fn alloc(&mut self) -> Ref {
        let r = Ref::new(self.0);
        self.0 += 1;
        r
    }
}

// ── Font bookkeeping for a single to_bytes() call ─────────────

struct FontRefs {
    name: String,
    type0_ref: Ref,
    cid_font_ref: Option<Ref>,
    font_descriptor_ref: Option<Ref>,
    font_file_ref: Option<Ref>,
    tounicode_ref: Option<Ref>,
}

struct CustomFontData {
    subset_data: Vec<u8>,
    char_to_new_gid: BTreeMap<char, u16>,
    gid_widths: BTreeMap<u16, f32>,
    ascent: f32,
    descent: f32,
    cap_height: f32,
    bbox: Rect,
    family: String,
}

/// Walk every page's ops and collect (custom font name → chars used).
fn collect_font_chars(pages: &[Arc<Mutex<PageContent>>]) -> BTreeMap<String, Vec<char>> {
    let mut out: BTreeMap<String, Vec<char>> = BTreeMap::new();
    for page_arc in pages {
        let page = page_arc.lock().unwrap();
        let mut current: Option<String> = None;
        for op in &page.operations {
            match op {
                PdfOp::SetFont { name, .. } => current = Some(name.clone()),
                PdfOp::ShowGlyphs(chars) => {
                    if let Some(ref fname) = current {
                        out.entry(fname.clone()).or_default().extend(chars);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

/// For each registered custom font used on some page, parse the TTF,
/// subset it down to the required glyphs, and extract metrics.
fn build_custom_font_data(
    registered: &[RegisteredFont],
    font_chars: &BTreeMap<String, Vec<char>>,
) -> PyResult<BTreeMap<String, CustomFontData>> {
    let mut out: BTreeMap<String, CustomFontData> = BTreeMap::new();

    for rf in registered {
        let Some(chars) = font_chars.get(&rf.name) else {
            continue;
        };

        let face = ttf_parser::Face::parse(&rf.data, 0).map_err(|e| {
            PyValueError::new_err(format!("failed to parse font for embedding: {e}"))
        })?;

        let units_per_em = f32::from(face.units_per_em());

        let mut remapper = subsetter::GlyphRemapper::new();
        let mut char_to_old_gid: BTreeMap<char, u16> = BTreeMap::new();

        for &ch in chars {
            if let Some(gid) = face.glyph_index(ch) {
                let old_gid = gid.0;
                remapper.remap(old_gid);
                char_to_old_gid.insert(ch, old_gid);
            }
        }

        let subset_data =
            subsetter::subset(&rf.data, 0, &remapper).unwrap_or_else(|_| rf.data.clone());

        let mut char_to_new_gid: BTreeMap<char, u16> = BTreeMap::new();
        let mut gid_widths: BTreeMap<u16, f32> = BTreeMap::new();

        for (&ch, &old_gid) in &char_to_old_gid {
            if let Some(new_gid) = remapper.get(old_gid) {
                char_to_new_gid.insert(ch, new_gid);
                let width = f32::from(
                    face.glyph_hor_advance(ttf_parser::GlyphId(old_gid))
                        .unwrap_or(0),
                );
                let scaled_width = width * 1000.0 / units_per_em;
                gid_widths.insert(new_gid, scaled_width);
            }
        }

        let ascent = f32::from(face.ascender()) * 1000.0 / units_per_em;
        let descent = f32::from(face.descender()) * 1000.0 / units_per_em;
        let cap_height =
            f32::from(face.capital_height().unwrap_or(face.ascender())) * 1000.0 / units_per_em;
        let global_bbox = face.global_bounding_box();
        let bbox = Rect::new(
            f32::from(global_bbox.x_min) * 1000.0 / units_per_em,
            f32::from(global_bbox.y_min) * 1000.0 / units_per_em,
            f32::from(global_bbox.x_max) * 1000.0 / units_per_em,
            f32::from(global_bbox.y_max) * 1000.0 / units_per_em,
        );

        out.insert(
            rf.name.clone(),
            CustomFontData {
                subset_data,
                char_to_new_gid,
                gid_widths,
                ascent,
                descent,
                cap_height,
                bbox,
                family: rf.family.clone(),
            },
        );
    }

    Ok(out)
}

/// Collect the unique opacities referenced by `SetAlpha` ops on this page,
/// returning them in first-seen order. The index in the returned `Vec` plus 1
/// becomes the suffix of the `/GsN` `ExtGState` resource name.
fn collect_page_alphas(page: &PageContent) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for op in &page.operations {
        if let PdfOp::SetAlpha { alpha } = op
            && !out.iter().any(|a| (a - alpha).abs() < 1e-6)
        {
            out.push(*alpha);
        }
    }
    out
}

/// Format a rounded-rectangle path into `content`. `radii` is
/// `[top-left, top-right, bottom-right, bottom-left]`. Each corner is
/// clamped to half the shorter edge so adjacent corners never overlap.
fn emit_rounded_rect_path(content: &mut Content, x: f32, y: f32, w: f32, h: f32, radii: [f32; 4]) {
    // Kappa constant for quarter-circle Bezier approximation.
    const K: f32 = 0.552_284_8;

    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let max_r = (w.min(h)) / 2.0;
    let r_tl = radii[0].max(0.0).min(max_r);
    let r_tr = radii[1].max(0.0).min(max_r);
    let r_br = radii[2].max(0.0).min(max_r);
    let r_bl = radii[3].max(0.0).min(max_r);
    // In PDF user space y grows upward. We trace clockwise starting at the
    // top-left corner's horizontal tangent point, i.e. (x + r_tl, y + h).
    let left = x;
    let right = x + w;
    let bottom = y;
    let top = y + h;

    // Start: top edge, just past the top-left corner.
    content.move_to(left + r_tl, top);
    // Top edge to the start of the top-right corner.
    content.line_to(right - r_tr, top);
    // Top-right corner: curve to (right, top - r_tr).
    if r_tr > 0.0 {
        content.cubic_to(
            right - r_tr + r_tr * K,
            top,
            right,
            top - r_tr + r_tr * K,
            right,
            top - r_tr,
        );
    }
    // Right edge down to bottom-right corner.
    content.line_to(right, bottom + r_br);
    if r_br > 0.0 {
        content.cubic_to(
            right,
            bottom + r_br - r_br * K,
            right - r_br + r_br * K,
            bottom,
            right - r_br,
            bottom,
        );
    }
    // Bottom edge to bottom-left corner.
    content.line_to(left + r_bl, bottom);
    if r_bl > 0.0 {
        content.cubic_to(
            left + r_bl - r_bl * K,
            bottom,
            left,
            bottom + r_bl - r_bl * K,
            left,
            bottom + r_bl,
        );
    }
    // Left edge up to top-left corner.
    content.line_to(left, top - r_tl);
    if r_tl > 0.0 {
        content.cubic_to(
            left,
            top - r_tl + r_tl * K,
            left + r_tl - r_tl * K,
            top,
            left + r_tl,
            top,
        );
    }
}

/// Emit the PDF content stream for a single page, walking its `PdfOp`s.
fn write_page_content_stream(
    page: &PageContent,
    font_refs: &[FontRefs],
    custom_data: &BTreeMap<String, CustomFontData>,
    page_alphas: &[f32],
) -> Vec<u8> {
    let mut content = Content::new();
    let mut current_font_name: Option<String> = None;

    for op in &page.operations {
        match op {
            PdfOp::BeginText => {
                content.begin_text();
            }
            PdfOp::EndText => {
                content.end_text();
            }
            PdfOp::SetFont { name, size } => {
                current_font_name = Some(name.clone());
                let idx = font_refs
                    .iter()
                    .position(|fr| fr.name == *name)
                    .unwrap_or(0);
                let resource_name = format!("F{}", idx + 1);
                content.set_font(Name(resource_name.as_bytes()), *size);
            }
            PdfOp::SetTextPosition { x, y } => {
                content.next_line(*x, *y);
            }
            PdfOp::ShowText(bytes) => {
                let escaped = pdf_escape(bytes);
                content.show(Str(&escaped));
            }
            PdfOp::ShowGlyphs(chars) => {
                if let Some(ref fname) = current_font_name
                    && let Some(cfd) = custom_data.get(fname)
                {
                    let mut bytes = Vec::with_capacity(chars.len() * 2);
                    for &ch in chars {
                        let gid = cfd.char_to_new_gid.get(&ch).copied().unwrap_or(0);
                        bytes.push((gid >> 8) as u8);
                        bytes.push((gid & 0xff) as u8);
                    }
                    content.show(Str(&bytes));
                }
            }
            PdfOp::SetFillColor { r, g, b } => {
                content.set_fill_rgb(*r, *g, *b);
            }
            PdfOp::SetStrokeColor { r, g, b } => {
                content.set_stroke_rgb(*r, *g, *b);
            }
            PdfOp::SetLineWidth(w) => {
                content.set_line_width(*w);
            }
            PdfOp::Rectangle {
                x,
                y,
                width,
                height,
            } => {
                content.rect(*x, *y, *width, *height);
            }
            PdfOp::RoundedRectangle {
                x,
                y,
                width,
                height,
                radii,
            } => {
                emit_rounded_rect_path(&mut content, *x, *y, *width, *height, *radii);
            }
            PdfOp::MoveTo { x, y } => {
                content.move_to(*x, *y);
            }
            PdfOp::LineTo { x, y } => {
                content.line_to(*x, *y);
            }
            PdfOp::Stroke => {
                content.stroke();
            }
            PdfOp::Fill => {
                content.fill_nonzero();
            }
            PdfOp::FillEvenOdd => {
                content.fill_even_odd();
            }
            PdfOp::FillAndStroke => {
                content.fill_nonzero_and_stroke();
            }
            PdfOp::ClipNonzero => {
                content.clip_nonzero();
                content.end_path();
            }
            PdfOp::SaveState => {
                content.save_state();
            }
            PdfOp::RestoreState => {
                content.restore_state();
            }
            PdfOp::SetDashPattern { array, phase } => {
                content.set_dash_pattern(array.iter().copied(), *phase);
            }
            PdfOp::SetWordSpacing(spacing) => {
                content.set_word_spacing(*spacing);
            }
            PdfOp::SetCharacterSpacing(spacing) => {
                content.set_char_spacing(*spacing);
            }
            PdfOp::DrawImage {
                index,
                x,
                y,
                width,
                height,
            } => {
                let resource_name = format!("Im{index}");
                content.save_state();
                content.transform([*width, 0.0, 0.0, *height, *x, *y]);
                content.x_object(Name(resource_name.as_bytes()));
                content.restore_state();
            }
            PdfOp::SetAlpha { alpha } => {
                let idx = page_alphas
                    .iter()
                    .position(|a| (a - alpha).abs() < 1e-6)
                    .unwrap_or(0);
                let resource_name = format!("Gs{}", idx + 1);
                content.set_parameters(Name(resource_name.as_bytes()));
            }
            PdfOp::Translate { dx, dy } => {
                content.transform([1.0, 0.0, 0.0, 1.0, *dx, *dy]);
            }
        }
    }

    content.finish().to_vec()
}

/// Build the parent/sibling relationships for a flat list of headings and
/// write the matching `/Outlines` dictionary plus one `OutlineItem` per
/// heading.
///
/// Hierarchy rule: an `h_N` becomes a child of the nearest preceding heading
/// with level < N. If there is no such heading, it becomes a top-level
/// entry. This is the same "stack of open parents" algorithm Markdown
/// renderers use for TOCs and matches what a reader's sidebar expects.
///
/// Destinations use `/FitH` with the heading's recorded y so clicking a
/// bookmark scrolls to the heading's top edge at the viewer's current zoom.
fn write_outline(
    pdf: &mut Pdf,
    root_ref: Ref,
    item_refs: &[Ref],
    headings: &[HeadingEntry],
    page_refs: &[(Ref, Ref)],
) {
    debug_assert_eq!(item_refs.len(), headings.len());

    // Precompute each item's parent / siblings / first child / last child
    // without writing PDF yet. `parent[i]` is the index into `headings` of
    // this item's parent, or `None` if it is a top-level item.
    let n = headings.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut top_level: Vec<usize> = Vec::new();
    // Stack of (item index, level) tracking currently-open parents.
    let mut stack: Vec<(usize, u8)> = Vec::new();
    for (i, h) in headings.iter().enumerate() {
        while let Some(&(_, lvl)) = stack.last() {
            if lvl < h.level {
                break;
            }
            stack.pop();
        }
        if let Some(&(p_idx, _)) = stack.last() {
            parent[i] = Some(p_idx);
            children[p_idx].push(i);
        } else {
            top_level.push(i);
        }
        stack.push((i, h.level));
    }

    // Write the outline dictionary.
    {
        let mut outline = pdf.outline(root_ref);
        if let (Some(&first), Some(&last)) = (top_level.first(), top_level.last()) {
            outline.first(item_refs[first]);
            outline.last(item_refs[last]);
            // Top-level visible count. A negative value would mean
            // "collapsed" — we always emit the tree expanded.
            outline.count(top_level.len() as i32);
        }
    }

    // Write each OutlineItem.
    for (i, h) in headings.iter().enumerate() {
        let siblings = match parent[i] {
            Some(p) => &children[p][..],
            None => &top_level[..],
        };
        let pos = siblings.iter().position(|&x| x == i).unwrap();
        let prev = if pos > 0 {
            Some(siblings[pos - 1])
        } else {
            None
        };
        let next = if pos + 1 < siblings.len() {
            Some(siblings[pos + 1])
        } else {
            None
        };

        let mut item = pdf.outline_item(item_refs[i]);
        item.title(pdf_writer::TextStr(&h.text));
        item.parent(match parent[i] {
            Some(p) => item_refs[p],
            None => root_ref,
        });
        if let Some(p) = prev {
            item.prev(item_refs[p]);
        }
        if let Some(nx) = next {
            item.next(item_refs[nx]);
        }
        if !children[i].is_empty() {
            item.first(item_refs[*children[i].first().unwrap()]);
            item.last(item_refs[*children[i].last().unwrap()]);
            // Always emit child count as positive (tree starts expanded).
            item.count(children[i].len() as i32);
        }
        if h.page_index < page_refs.len() {
            let page_id = page_refs[h.page_index].0;
            item.dest().page(page_id).fit_horizontal(h.y);
        }
    }
}

/// Write PDF image `XObjects` for every image and its optional alpha `SMask`.
fn write_image_xobjects(
    pdf: &mut Pdf,
    images: &[image::ImageData],
    image_refs: &[(Ref, Option<Ref>)],
) {
    for (idx, img) in images.iter().enumerate() {
        let (main_ref, smask_ref) = image_refs[idx];

        {
            let mut xobj = pdf.image_xobject(main_ref, &img.data);
            xobj.width(img.width as i32);
            xobj.height(img.height as i32);
            xobj.bits_per_component(i32::from(img.bits_per_component));
            match img.color_space {
                image::ImageColorSpace::DeviceGray => {
                    xobj.color_space().device_gray();
                }
                image::ImageColorSpace::DeviceRgb => {
                    xobj.color_space().device_rgb();
                }
            }
            match img.filter {
                image::ImageFilter::Dct => {
                    xobj.filter(Filter::DctDecode);
                }
                image::ImageFilter::Flate => {
                    xobj.filter(Filter::FlateDecode);
                }
            }
            if let Some(smask) = smask_ref {
                xobj.s_mask(smask);
            }
        }

        if let (Some(smask), Some(alpha)) = (smask_ref, img.alpha_mask.as_ref()) {
            let mut smask_obj = pdf.image_xobject(smask, &alpha.data);
            smask_obj.width(alpha.width as i32);
            smask_obj.height(alpha.height as i32);
            smask_obj.bits_per_component(i32::from(alpha.bits_per_component));
            smask_obj.color_space().device_gray();
            if matches!(alpha.filter, image::ImageFilter::Flate) {
                smask_obj.filter(Filter::FlateDecode);
            }
        }
    }
}

/// Write all font objects: Type1 for the 14 built-ins, Type0/CIDFont2
/// for every embedded custom font (with descriptor, font file, `ToUnicode`).
fn write_font_objects(
    pdf: &mut Pdf,
    font_refs: &[FontRefs],
    custom_data: &BTreeMap<String, CustomFontData>,
) {
    for fr in font_refs {
        if fr.name.starts_with("Custom-") {
            let Some(cfd) = custom_data.get(&fr.name) else {
                continue;
            };
            let cid_font_ref = fr.cid_font_ref.unwrap();
            let font_descriptor_ref = fr.font_descriptor_ref.unwrap();
            let font_file_ref = fr.font_file_ref.unwrap();
            let tounicode_ref = fr.tounicode_ref.unwrap();

            let base_font_name = cfd.family.replace(' ', "-");

            pdf.type0_font(fr.type0_ref)
                .base_font(Name(base_font_name.as_bytes()))
                .encoding_predefined(Name(b"Identity-H"))
                .descendant_font(cid_font_ref)
                .to_unicode(tounicode_ref);

            {
                let mut cid = pdf.cid_font(cid_font_ref);
                cid.subtype(CidFontType::Type2)
                    .base_font(Name(base_font_name.as_bytes()))
                    .system_info(SystemInfo {
                        registry: Str(b"Adobe"),
                        ordering: Str(b"Identity"),
                        supplement: 0,
                    })
                    .font_descriptor(font_descriptor_ref)
                    .cid_to_gid_map_predefined(Name(b"Identity"));

                let mut widths = cid.widths();
                for (&gid, &w) in &cfd.gid_widths {
                    widths.consecutive(gid, [w]);
                }
            }

            pdf.font_descriptor(font_descriptor_ref)
                .name(Name(base_font_name.as_bytes()))
                .flags(FontFlags::NON_SYMBOLIC)
                .bbox(cfd.bbox)
                .italic_angle(0.0)
                .ascent(cfd.ascent)
                .descent(cfd.descent)
                .cap_height(cfd.cap_height)
                .stem_v(80.0)
                .font_file2(font_file_ref);

            let compressed_font = image::compress(&cfd.subset_data);
            pdf.stream(font_file_ref, &compressed_font)
                .filter(Filter::FlateDecode);

            let sys_info = SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            };
            let mut cmap = UnicodeCmap::new(Name(b"Custom"), sys_info);
            for (&ch, &new_gid) in &cfd.char_to_new_gid {
                cmap.pair(new_gid, ch);
            }
            let cmap_data = cmap.finish();
            let compressed_cmap = image::compress(&cmap_data);
            pdf.stream(tounicode_ref, &compressed_cmap)
                .filter(Filter::FlateDecode);
        } else {
            pdf.type1_font(fr.type0_ref)
                .base_font(Name(fr.name.as_bytes()));
        }
    }
}

// ── PdfDocument ────────────────────────────────────────────────

#[pyclass]
pub(crate) struct PdfDocument {
    pub(crate) pages: Vec<Arc<Mutex<PageContent>>>,
    registered_fonts: Vec<RegisteredFont>,
    pub(crate) images: Vec<image::ImageData>,
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) subject: Option<String>,
    pub(crate) keywords: Option<String>,
    pub(crate) creator: Option<String>,
    /// Non-fatal diagnostics collected during rendering — e.g. image
    /// load failures. Exposed to Python so callers can decide whether
    /// to surface them to end users.
    pub(crate) warnings: Vec<String>,
    /// Headings collected during layout, in document order. Used to
    /// build the PDF `/Outlines` tree at `to_bytes()` time.
    pub(crate) headings: Vec<HeadingEntry>,
    /// Map from element `id` to `(page_index, y)` for every anchor the
    /// layout registered. Internal `<a href="#id">` links are resolved
    /// against this map at `to_bytes()` time.
    pub(crate) anchors: std::collections::HashMap<String, (usize, f32)>,
}

#[pymethods]
impl PdfDocument {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(PdfDocument {
            pages: Vec::new(),
            registered_fonts: Vec::new(),
            images: Vec::new(),
            title: None,
            author: None,
            subject: None,
            keywords: None,
            creator: Some("pdfun".to_string()),
            warnings: Vec::new(),
            headings: Vec::new(),
            anchors: std::collections::HashMap::new(),
        })
    }

    /// Non-fatal diagnostics from rendering (e.g. image load failures).
    #[getter]
    fn warnings(&self) -> Vec<String> {
        self.warnings.clone()
    }

    fn set_title(&mut self, title: &str) -> PyResult<()> {
        self.title = Some(title.to_string());
        Ok(())
    }

    fn set_author(&mut self, author: &str) -> PyResult<()> {
        self.author = Some(author.to_string());
        Ok(())
    }

    fn set_subject(&mut self, subject: &str) -> PyResult<()> {
        self.subject = Some(subject.to_string());
        Ok(())
    }

    fn set_keywords(&mut self, keywords: &str) -> PyResult<()> {
        self.keywords = Some(keywords.to_string());
        Ok(())
    }

    #[pyo3(signature = (width=612.0, height=792.0))]
    fn add_page(&mut self, width: f64, height: f64) -> PyResult<Page> {
        let content = Arc::new(Mutex::new(PageContent::new(width, height)));
        self.pages.push(Arc::clone(&content));
        Ok(Page { content })
    }

    fn to_bytes(&self) -> PyResult<Vec<u8>> {
        let mut pdf = Pdf::new();
        let mut allocator = RefAllocator::new();

        let catalog_id = allocator.alloc();
        let page_tree_id = allocator.alloc();

        let page_refs: Vec<(Ref, Ref)> = self
            .pages
            .iter()
            .map(|_| (allocator.alloc(), allocator.alloc()))
            .collect();

        // Collect unique fonts used across all pages and allocate per-font refs.
        let mut all_fonts: Vec<String> = Vec::new();
        for page_arc in &self.pages {
            let page = page_arc.lock().unwrap();
            for font_name in &page.fonts_used {
                if !all_fonts.contains(font_name) {
                    all_fonts.push(font_name.clone());
                }
            }
        }

        let font_refs: Vec<FontRefs> = all_fonts
            .iter()
            .map(|name| {
                let is_custom = name.starts_with("Custom-");
                FontRefs {
                    name: name.clone(),
                    type0_ref: allocator.alloc(),
                    cid_font_ref: is_custom.then(|| allocator.alloc()),
                    font_descriptor_ref: is_custom.then(|| allocator.alloc()),
                    font_file_ref: is_custom.then(|| allocator.alloc()),
                    tounicode_ref: is_custom.then(|| allocator.alloc()),
                }
            })
            .collect();

        let image_refs: Vec<(Ref, Option<Ref>)> = self
            .images
            .iter()
            .map(|img| {
                let main = allocator.alloc();
                let smask = img.alpha_mask.as_ref().map(|_| allocator.alloc());
                (main, smask)
            })
            .collect();

        // Subset custom fonts based on glyphs used across all pages.
        let custom_font_chars = collect_font_chars(&self.pages);
        let custom_data = build_custom_font_data(&self.registered_fonts, &custom_font_chars)?;

        // Allocate outline refs up front so the catalog can reference the
        // outline dictionary. Skip allocation entirely when there are no
        // headings — the outline is optional.
        let outline_refs: Option<(Ref, Vec<Ref>)> = if self.headings.is_empty() {
            None
        } else {
            let root = allocator.alloc();
            let items = self.headings.iter().map(|_| allocator.alloc()).collect();
            Some((root, items))
        };

        // Catalog and document metadata.
        {
            let mut catalog = pdf.catalog(catalog_id);
            catalog.pages(page_tree_id);
            if let Some((root, _)) = &outline_refs {
                catalog.outlines(*root);
            }
        }

        let has_info = self.title.is_some()
            || self.author.is_some()
            || self.subject.is_some()
            || self.keywords.is_some()
            || self.creator.is_some();
        if has_info {
            let info_id = allocator.alloc();
            let mut info = pdf.document_info(info_id);
            if let Some(ref title) = self.title {
                info.title(pdf_writer::TextStr(title));
            }
            if let Some(ref author) = self.author {
                info.author(pdf_writer::TextStr(author));
            }
            if let Some(ref subject) = self.subject {
                info.subject(pdf_writer::TextStr(subject));
            }
            if let Some(ref keywords) = self.keywords {
                info.keywords(pdf_writer::TextStr(keywords));
            }
            if let Some(ref creator) = self.creator {
                info.creator(pdf_writer::TextStr(creator));
            }
        }

        // Page tree.
        let page_ids: Vec<Ref> = page_refs.iter().map(|(pid, _)| *pid).collect();
        pdf.pages(page_tree_id)
            .kids(page_ids)
            .count(self.pages.len() as i32);

        // Per-page content streams, resource dicts, and annotation objects.
        for (i, page_arc) in self.pages.iter().enumerate() {
            let page = page_arc.lock().unwrap();
            let (page_id, content_id) = page_refs[i];

            let page_alphas = collect_page_alphas(&page);
            // Allocate a Ref per unique alpha on this page.
            let alpha_refs: Vec<Ref> = page_alphas.iter().map(|_| allocator.alloc()).collect();

            let content_bytes =
                write_page_content_stream(&page, &font_refs, &custom_data, &page_alphas);
            let annot_refs: Vec<Ref> = page.links.iter().map(|_| allocator.alloc()).collect();

            {
                let mut page_writer = pdf.page(page_id);
                page_writer
                    .parent(page_tree_id)
                    .media_box(Rect::new(0.0, 0.0, page.width as f32, page.height as f32))
                    .contents(content_id);

                if !annot_refs.is_empty() {
                    page_writer.annotations(annot_refs.iter().copied());
                }

                let mut resources = page_writer.resources();
                {
                    let mut fonts_dict = resources.fonts();
                    for (idx, fr) in font_refs.iter().enumerate() {
                        let resource_name = format!("F{}", idx + 1);
                        fonts_dict.pair(Name(resource_name.as_bytes()), fr.type0_ref);
                    }
                }
                if !page.images_used.is_empty() {
                    let mut xobjects = resources.x_objects();
                    for &img_idx in &page.images_used {
                        let resource_name = format!("Im{img_idx}");
                        xobjects.pair(Name(resource_name.as_bytes()), image_refs[img_idx].0);
                    }
                }
                if !alpha_refs.is_empty() {
                    let mut ext_g_states = resources.ext_g_states();
                    for (idx, r) in alpha_refs.iter().enumerate() {
                        let resource_name = format!("Gs{}", idx + 1);
                        ext_g_states.pair(Name(resource_name.as_bytes()), *r);
                    }
                }
            }

            // Emit the per-alpha ExtGState dicts for this page.
            for (alpha, alpha_ref) in page_alphas.iter().zip(alpha_refs.iter()) {
                pdf.ext_graphics(*alpha_ref)
                    .non_stroking_alpha(*alpha)
                    .stroking_alpha(*alpha);
            }

            for (annot_ref, link) in annot_refs.iter().zip(page.links.iter()) {
                let rect = Rect::new(link.x, link.y, link.x + link.width, link.y + link.height);
                let mut annot = pdf.annotation(*annot_ref);
                annot
                    .subtype(AnnotationType::Link)
                    .rect(rect)
                    .border(0.0, 0.0, 0.0, None);

                // Internal fragment links (`<a href="#id">`) become GoTo
                // actions targeting a FitH destination at the anchor's
                // recorded y. External URLs keep the original URI action.
                // Unresolved fragment links degrade to a plain Link
                // annotation with no action — the rect still renders but
                // clicks are inert, which matches "do not crash" from the
                // implementation plan.
                if let Some(fragment) = link.url.strip_prefix('#') {
                    if let Some((target_page_index, target_y)) = self.anchors.get(fragment).copied()
                        && target_page_index < page_refs.len()
                    {
                        let target_page_id = page_refs[target_page_index].0;
                        let mut action = annot.action();
                        action.action_type(ActionType::GoTo);
                        action
                            .destination()
                            .page(target_page_id)
                            .fit_horizontal(target_y);
                    }
                    // Unresolved: drop the action silently.
                } else {
                    annot
                        .action()
                        .action_type(ActionType::Uri)
                        .uri(Str(link.url.as_bytes()));
                }
            }

            let compressed_content = image::compress(&content_bytes);
            pdf.stream(content_id, &compressed_content)
                .filter(Filter::FlateDecode);
        }

        // Emit the outline tree if we collected any headings.
        if let Some((root_ref, item_refs)) = &outline_refs {
            write_outline(&mut pdf, *root_ref, item_refs, &self.headings, &page_refs);
        }

        write_image_xobjects(&mut pdf, &self.images, &image_refs);
        write_font_objects(&mut pdf, &font_refs, &custom_data);

        Ok(pdf.finish())
    }

    fn save(&self, path: &str) -> PyResult<()> {
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes)
            .map_err(|e| PyValueError::new_err(format!("failed to write file: {e}")))?;
        Ok(())
    }

    fn register_font(&mut self, font_db: &FontDatabase, font_id: &FontId) -> PyResult<String> {
        let entry = font_db
            .fonts
            .get(font_id.index)
            .ok_or_else(|| PyValueError::new_err("invalid font id"))?;
        let name = format!("Custom-{}", self.registered_fonts.len());
        self.registered_fonts.push(RegisteredFont {
            data: entry.data.clone(),
            family: entry.family.clone(),
            name: name.clone(),
        });
        Ok(name)
    }
}

// ── Page ───────────────────────────────────────────────────────

#[pyclass]
struct Page {
    content: Arc<Mutex<PageContent>>,
}

#[pymethods]
impl Page {
    #[getter]
    fn width(&self) -> PyResult<f64> {
        Ok(self.content.lock().unwrap().width)
    }

    #[getter]
    fn height(&self) -> PyResult<f64> {
        Ok(self.content.lock().unwrap().height)
    }

    fn set_font(&mut self, name: &str, size: f64) -> PyResult<()> {
        let is_builtin = BUILTIN_FONTS.contains(&name);
        let is_custom = name.starts_with("Custom-");
        if !is_builtin && !is_custom {
            return Err(PyValueError::new_err(format!("unknown font: {name}")));
        }
        let mut page = self.content.lock().unwrap();
        // track that this page uses this font
        if !page.fonts_used.contains(&name.to_string()) {
            page.fonts_used.push(name.to_string());
        }
        page.current_font = Some(name.to_string());
        page.current_font_size = Some(size as f32);
        page.operations.push(PdfOp::BeginText);
        page.operations.push(PdfOp::SetFont {
            name: name.to_string(),
            size: size as f32,
        });
        Ok(())
    }

    fn measure_text(&self, text: &str) -> PyResult<f64> {
        let page = self.content.lock().unwrap();
        let font_name = page
            .current_font
            .as_deref()
            .ok_or_else(|| PyValueError::new_err("no font set; call set_font() first"))?;
        let font_size = page.current_font_size.unwrap();
        let width = font_metrics::measure_str(text, font_name, font_size)
            .ok_or_else(|| PyValueError::new_err(format!("no metrics for font: {font_name}")))?;
        Ok(f64::from(width))
    }

    fn draw_text(&mut self, x: f64, y: f64, text: &str) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        let is_custom = page
            .current_font
            .as_deref()
            .is_some_and(|f| f.starts_with("Custom-"));
        page.operations.push(PdfOp::SetTextPosition {
            x: x as f32,
            y: y as f32,
        });
        if is_custom {
            page.operations
                .push(PdfOp::ShowGlyphs(text.chars().collect()));
        } else {
            // Built-in PDF fonts speak WinAnsi: transcode the text now
            // so the content-stream literal carries the bytes the
            // viewer expects. Non-mappable codepoints fall back to '?'
            // per char (the same defect today, but bounded — WS-1B
            // will promote those runs to a Unicode-capable face).
            let bytes = transcode_with_fallback(text);
            page.operations.push(PdfOp::ShowText(bytes));
        }
        page.operations.push(PdfOp::EndText);
        Ok(())
    }

    fn set_fill_color(&mut self, r: f64, g: f64, b: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetFillColor {
            r: r as f32,
            g: g as f32,
            b: b as f32,
        });
        Ok(())
    }

    fn set_stroke_color(&mut self, r: f64, g: f64, b: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetStrokeColor {
            r: r as f32,
            g: g as f32,
            b: b as f32,
        });
        Ok(())
    }

    fn set_line_width(&mut self, width: f64) -> PyResult<()> {
        if width < 0.0 {
            return Err(PyValueError::new_err("line width must be non-negative"));
        }
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SetLineWidth(width as f32));
        Ok(())
    }

    fn draw_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::Fill);
        Ok(())
    }

    fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::Stroke);
        Ok(())
    }

    fn fill_and_stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::Rectangle {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        });
        page.operations.push(PdfOp::FillAndStroke);
        Ok(())
    }

    fn draw_line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::MoveTo {
            x: x1 as f32,
            y: y1 as f32,
        });
        page.operations.push(PdfOp::LineTo {
            x: x2 as f32,
            y: y2 as f32,
        });
        page.operations.push(PdfOp::Stroke);
        Ok(())
    }

    fn save_state(&mut self) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::SaveState);
        Ok(())
    }

    fn restore_state(&mut self) -> PyResult<()> {
        let mut page = self.content.lock().unwrap();
        page.operations.push(PdfOp::RestoreState);
        Ok(())
    }
}

// ── FontId ────────────────────────────────────────────────────

#[pyclass]
#[derive(Clone)]
struct FontId {
    index: usize,
}

#[pymethods]
impl FontId {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontId { index: 0 })
    }
}

// ── FontDatabase ──────────────────────────────────────────────

struct FontEntry {
    data: Vec<u8>,
    family: String,
    weight: u16,
    italic: bool,
}

#[pyclass]
struct FontDatabase {
    fonts: Vec<FontEntry>,
}

#[pymethods]
impl FontDatabase {
    #[new]
    fn new() -> PyResult<Self> {
        Ok(FontDatabase { fonts: Vec::new() })
    }

    fn load_system_fonts(&mut self) -> PyResult<()> {
        let dirs = if cfg!(target_os = "macos") {
            vec![
                "/System/Library/Fonts".to_string(),
                "/Library/Fonts".to_string(),
            ]
        } else if cfg!(target_os = "linux") {
            vec![
                "/usr/share/fonts".to_string(),
                "/usr/local/share/fonts".to_string(),
            ]
        } else if cfg!(target_os = "windows") {
            vec!["C:\\Windows\\Fonts".to_string()]
        } else {
            vec![]
        };

        for dir in dirs {
            self.scan_font_dir(&dir);
        }
        Ok(())
    }

    fn load_font_file(&mut self, path: &str) -> PyResult<FontId> {
        let data = std::fs::read(path)
            .map_err(|e| PyValueError::new_err(format!("failed to read font file: {e}")))?;
        self.load_font_data_impl(&data)
    }

    fn load_font_data(&mut self, data: &[u8]) -> PyResult<FontId> {
        self.load_font_data_impl(data)
    }

    #[pyo3(signature = (family, weight=None, italic=None))]
    fn query(
        &self,
        family: &str,
        weight: Option<u16>,
        italic: Option<bool>,
    ) -> PyResult<Option<FontId>> {
        let family_lower = family.to_lowercase();
        let mut best: Option<(usize, i32)> = None;

        for (i, entry) in self.fonts.iter().enumerate() {
            if entry.family.to_lowercase() != family_lower {
                continue;
            }
            let mut score: i32 = 0;
            if let Some(w) = weight {
                score -= (i32::from(entry.weight) - i32::from(w)).abs();
            }
            if let Some(it) = italic
                && entry.italic != it
            {
                score -= 1000;
            }
            if best.is_none() || score > best.unwrap().1 {
                best = Some((i, score));
            }
        }

        Ok(best.map(|(index, _)| FontId { index }))
    }
}

impl FontDatabase {
    fn load_font_data_impl(&mut self, data: &[u8]) -> PyResult<FontId> {
        let face = ttf_parser::Face::parse(data, 0)
            .map_err(|e| PyValueError::new_err(format!("invalid font data: {e}")))?;

        let family = face
            .names()
            .into_iter()
            .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
            .and_then(|n| n.to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let weight = face.weight().to_number();
        let italic = face.is_italic();

        let index = self.fonts.len();
        self.fonts.push(FontEntry {
            data: data.to_vec(),
            family,
            weight,
            italic,
        });

        Ok(FontId { index })
    }

    fn scan_font_dir(&mut self, dir: &str) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.scan_font_dir(&path.to_string_lossy());
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if matches!(ext.as_str(), "ttf" | "otf" | "ttc")
                && let Ok(data) = std::fs::read(&path)
            {
                let _ = self.load_font_data_impl(&data);
            }
        }
    }
}

/// RAII guard that clears the per-thread `@font-face` measurement
/// metrics installed by `html_render::render_dom_to_layout`. Ensures a
/// panic during render still wipes the thread-local so a follow-on
/// caller on the same thread doesn't see stale state.
struct FontFaceMetricsGuard;

impl Drop for FontFaceMetricsGuard {
    fn drop(&mut self) {
        font_metrics::clear_font_face_metrics();
    }
}

/// Render HTML to a PDF document (called from Python `HtmlDocument`).
#[pyfunction]
#[pyo3(signature = (html, margin_top=72.0, margin_right=72.0, margin_bottom=72.0, margin_left=72.0, page_width=612.0, page_height=792.0, base_url=None))]
#[allow(clippy::too_many_arguments)]
fn html_to_pdf(
    html: &str,
    margin_top: f64,
    margin_right: f64,
    margin_bottom: f64,
    margin_left: f64,
    page_width: f64,
    page_height: f64,
    base_url: Option<&str>,
) -> PyResult<PdfDocument> {
    let _font_face_guard = FontFaceMetricsGuard;
    let parsed = dom::parse_html(html);
    let title = html_render::extract_title(&parsed.document);
    let mut doc = PdfDocument {
        pages: Vec::new(),
        registered_fonts: Vec::new(),
        images: Vec::new(),
        title,
        author: None,
        subject: None,
        keywords: None,
        creator: Some("pdfun".to_string()),
        warnings: Vec::new(),
        headings: Vec::new(),
        anchors: std::collections::HashMap::new(),
    };
    let mut inner = layout::LayoutInner::new(
        margin_top as f32,
        margin_right as f32,
        margin_bottom as f32,
        margin_left as f32,
        page_width as f32,
        page_height as f32,
    );
    let base_path = base_url.map(std::path::PathBuf::from);
    let outcome =
        html_render::render_dom_to_layout(&parsed.document, &mut inner, base_path.as_deref());
    let page_style = outcome.page_style;
    doc.warnings.extend(outcome.warnings);

    // Apply @page CSS overrides
    if let Some(w) = page_style.width {
        inner.page_width = w;
    }
    if let Some(h) = page_style.height {
        inner.page_height = h;
    }
    if let Some(m) = page_style.margin_top {
        inner.margin_top = m;
    }
    if let Some(m) = page_style.margin_right {
        inner.margin_right = m;
    }
    if let Some(m) = page_style.margin_bottom {
        inner.margin_bottom = m;
    }
    if let Some(m) = page_style.margin_left {
        inner.margin_left = m;
    }
    inner.margin_boxes = page_style.margin_boxes;

    inner.finish(&mut doc).map_err(PyValueError::new_err)?;
    // Transfer any images collected during rendering to the document
    doc.images.append(&mut inner.images);
    Ok(doc)
}

/// measure text width without a page context.
#[pyfunction]
fn text_width(text: &str, font: &str, size: f64) -> PyResult<f64> {
    let width = font_metrics::measure_str(text, font, size as f32)
        .ok_or_else(|| PyValueError::new_err(format!("unknown font: {font}")))?;
    Ok(f64::from(width))
}

/// word-wrap text into lines that fit within `max_width` points.
#[pyfunction]
fn wrap_text(text: &str, max_width: f64, font: &str, font_size: f64) -> PyResult<Vec<String>> {
    layout::wrap_text_impl(text, max_width as f32, font, font_size as f32)
        .map_err(PyValueError::new_err)
}

// ── Module ─────────────────────────────────────────────────────

#[pymodule]
mod _core {
    use pyo3::prelude::*;

    use super::{
        FontDatabase, FontId, Page, PdfDocument, html_to_pdf, layout, text_width, wrap_text,
    };

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add_class::<PdfDocument>()?;
        m.add_class::<Page>()?;
        m.add_class::<FontDatabase>()?;
        m.add_class::<FontId>()?;
        m.add_class::<layout::Layout>()?;
        m.add_class::<layout::TextRun>()?;
        m.add_function(wrap_pyfunction!(text_width, m)?)?;
        m.add_function(wrap_pyfunction!(wrap_text, m)?)?;
        m.add_function(wrap_pyfunction!(html_to_pdf, m)?)?;
        Ok(())
    }
}
