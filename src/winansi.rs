//! PDF `WinAnsiEncoding` transcoder (ISO 32000-1 Annex D.2).
//!
//! `WinAnsiEncoding` is the PDF-defined byte → glyph table for built-in
//! Type 1 fonts. It is similar to Windows-1252 but not identical: per the
//! PDF spec, byte values that have no other glyph assignment in the
//! table — 0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D, and 0xAD — all render as
//! `•` (bullet). A transcoder built blindly from `iconv WINDOWS-1252`
//! would emit those bytes for codepoints they represent in Windows-1252
//! (DEL at 0x7F, soft hyphen at 0xAD), which would render as bullets in
//! a PDF reader rather than the intended glyphs. We always emit
//! 0x95 (the canonical bullet slot) for both U+2022 BULLET and
//! U+00AD SOFT HYPHEN, and never emit the seven shadow-bullet bytes.

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
/// Returns `None` if the codepoint is not in the `WinAnsi` repertoire.
fn char_to_winansi_byte(ch: char) -> Option<u8> {
    let cp = ch as u32;
    // ASCII (0x00..=0x7E) passes through unchanged. U+007F is the DEL
    // control character — PDF's WinAnsi puts the bullet glyph at byte
    // 0x7F instead, so we have no canonical byte to emit for U+007F.
    // Treat it as out-of-range so the caller can fallback or substitute.
    if cp < 0x7F {
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
        // U+2022 BULLET and U+00AD SOFT HYPHEN both map to byte 0x95
        // per PDF's WinAnsiEncoding. (Latin-1 puts soft hyphen at 0xAD;
        // PDF spec doesn't.)
        '\u{2022}' | '\u{00AD}' => Some(0x95),
        '\u{2013}' => Some(0x96), // –
        '\u{2014}' => Some(0x97), // —
        '\u{02DC}' => Some(0x98), // ˜
        '\u{2122}' => Some(0x99), // ™
        '\u{0161}' => Some(0x9A), // š
        '\u{203A}' => Some(0x9B), // ›
        '\u{0153}' => Some(0x9C), // œ
        '\u{017E}' => Some(0x9E), // ž
        '\u{0178}' => Some(0x9F), // Ÿ
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

    /// Codepoints outside the `WinAnsi` repertoire surface as `Err(c)`
    /// carrying the offending character. WS-1B uses the carried char to
    /// decide whether to split the run onto a Unicode-capable fallback.
    #[test]
    fn winansi_errs_on_non_win1252() {
        // U+2611 BALLOT BOX WITH CHECK — definitely not in WinAnsi.
        assert_eq!(transcode_to_pdf_winansi("\u{2611}"), Err('\u{2611}'));
        // The error must carry the *first* non-mappable char, not the
        // last, so the splitter can preserve the WinAnsi-safe prefix.
        assert_eq!(transcode_to_pdf_winansi("ok ☑ done"), Err('\u{2611}'));
    }

    /// ISO 32000-1 Annex D.2 — PDF's `WinAnsiEncoding` defines the seven
    /// slots that Windows-1252 leaves undefined (0x7F, 0x81, 0x8D, 0x8F,
    /// 0x90, 0x9D, 0xAD) as `bullet`. A transcoder built from `iconv
    /// WINDOWS-1252` would either error on those slots or assign them
    /// to other glyphs — this test guards the invariants that follow:
    ///
    /// 1. The Unicode bullet (U+2022) must round-trip to byte 0x95
    ///    (the canonical bullet slot).
    /// 2. Soft hyphen (U+00AD) — which Latin-1 puts at byte 0xAD — must
    ///    map to bullet (0x95) per the PDF spec, not to its Latin-1
    ///    byte. (Iconv would emit 0xAD; PDF would render that as
    ///    bullet too, but we normalize on 0x95 to keep the encoding
    ///    consistent with the spec's documented bullet slot.)
    /// 3. None of the seven "undefined-in-Win-1252" output bytes
    ///    (0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D, 0xAD) is ever produced
    ///    for a non-bullet input — i.e. our table never accidentally
    ///    maps some other glyph there.
    #[test]
    fn pdf_winansi_undefined_slots_map_to_bullet() {
        const UNDEFINED_SLOTS: [u8; 7] = [0x7F, 0x81, 0x8D, 0x8F, 0x90, 0x9D, 0xAD];

        // (1) Canonical bullet round-trips to 0x95.
        assert_eq!(transcode_to_pdf_winansi("\u{2022}"), Ok(vec![0x95]));
        // (2) Soft hyphen -> bullet per PDF spec.
        assert_eq!(transcode_to_pdf_winansi("\u{00AD}"), Ok(vec![0x95]));

        // (3) Sweep every Unicode codepoint that *can* map (the table
        // is finite) and verify none of the undefined slots show up.
        // Scan a generous range covering ASCII, Latin-1, the 0x80-9F
        // override window, and the small set of higher codepoints in
        // the WinAnsi repertoire (Œ, ž, etc. — all <= U+20AC).
        for cp in 0x00u32..=0x20ACu32 {
            let Some(ch) = char::from_u32(cp) else {
                continue;
            };
            let Ok(bytes) = transcode_to_pdf_winansi(&ch.to_string()) else {
                continue;
            };
            for b in bytes {
                assert!(
                    !UNDEFINED_SLOTS.contains(&b) || b == 0x95,
                    "codepoint U+{cp:04X} produced byte 0x{b:02X} which is a Win-1252 undefined slot \
                     that PDF maps to bullet — only U+2022/U+00AD should reach a bullet byte (0x95)"
                );
            }
        }
    }
}
