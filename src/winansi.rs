//! PDF `WinAnsiEncoding` transcoder (ISO 32000-1 Annex D.2).
//!
//! NB: PDF's `WinAnsiEncoding` is **not** identical to Windows-1252. The
//! seven slots Windows-1252 leaves undefined (0x7F, 0x81, 0x8D, 0x8F,
//! 0x90, 0x9D, 0xAD) are all mapped to the bullet glyph `•` in the PDF
//! spec — a transcoder built from `iconv WINDOWS-1252` would mis-encode
//! those slots.

/// Transcode a `&str` into PDF `WinAnsiEncoding` bytes. Returns `Err(c)`
/// carrying the first non-mappable character so a downstream caller (e.g.
/// the WS-1B fallback-font promoter) can decide whether to split the run
/// onto a Unicode-capable face.
pub fn transcode_to_pdf_winansi(s: &str) -> Result<Vec<u8>, char> {
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        match char_to_winansi_byte(ch) {
            Some(b) => out.push(b),
            None => return Err(ch),
        }
    }
    Ok(out)
}

/// Map a single Unicode codepoint to its PDF `WinAnsiEncoding` byte.
/// Returns `None` if the codepoint is not in the WinAnsi repertoire.
fn char_to_winansi_byte(ch: char) -> Option<u8> {
    let cp = ch as u32;
    // ASCII passes through unchanged.
    if cp < 0x80 {
        return Some(cp as u8);
    }
    // 0x80..=0x9F: PDF defines specific glyphs in this window — these
    // are *not* the C1 control codes that Latin-1 puts there. Match
    // explicitly. The seven "undefined-in-Win-1252" slots (0x81, 0x8D,
    // 0x8F, 0x90, 0x9D, plus 0x7F and 0xAD) are intentionally absent
    // here; they're produced only by the bullet → 0x95 path.
    match ch {
        '\u{20AC}' => Some(0x80), // €
        '\u{201A}' => Some(0x82), // ‚
        '\u{0192}' => Some(0x83), // ƒ
        '\u{201E}' => Some(0x84), // „
        '\u{2026}' => Some(0x85), // …
        '\u{2020}' => Some(0x86), // †
        '\u{2021}' => Some(0x87), // ‡
        '\u{02C6}' => Some(0x88), // ˆ
        '\u{2030}' => Some(0x89), // ‰
        '\u{0160}' => Some(0x8A), // Š
        '\u{2039}' => Some(0x8B), // ‹
        '\u{0152}' => Some(0x8C), // Œ
        '\u{017D}' => Some(0x8E), // Ž
        '\u{2018}' => Some(0x91), // '
        '\u{2019}' => Some(0x92), // '
        '\u{201C}' => Some(0x93), // "
        '\u{201D}' => Some(0x94), // "
        '\u{2022}' => Some(0x95), // •
        '\u{2013}' => Some(0x96), // –
        '\u{2014}' => Some(0x97), // —
        '\u{02DC}' => Some(0x98), // ˜
        '\u{2122}' => Some(0x99), // ™
        '\u{0161}' => Some(0x9A), // š
        '\u{203A}' => Some(0x9B), // ›
        '\u{0153}' => Some(0x9C), // œ
        '\u{017E}' => Some(0x9E), // ž
        '\u{0178}' => Some(0x9F), // Ÿ
        // 0xAD soft hyphen — PDF maps to bullet, not to 0xAD.
        '\u{00AD}' => Some(0x95),
        // 0xA0..=0xFF Latin-1 supplement passes through 1:1 (modulo the
        // soft-hyphen override above).
        _ if (0xA0..=0xFF).contains(&cp) => Some(cp as u8),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_winansi_round_trips_latin1() {
        assert_eq!(transcode_to_pdf_winansi("é"), Ok(vec![0xE9]));
        assert_eq!(transcode_to_pdf_winansi("¿"), Ok(vec![0xBF]));
        assert_eq!(transcode_to_pdf_winansi("—"), Ok(vec![0x97]));
        assert_eq!(transcode_to_pdf_winansi("•"), Ok(vec![0x95]));
        assert_eq!(transcode_to_pdf_winansi("Œ"), Ok(vec![0x8C]));
        assert_eq!(transcode_to_pdf_winansi("€"), Ok(vec![0x80]));
    }
}
