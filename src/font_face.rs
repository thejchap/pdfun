//! Resolution and registration of `@font-face` rules.
//!
//! Layer 5 (`resolve_font_face_src`) walks a comma-separated `src:` list
//! and returns the first source whose bytes we can actually obtain. Data
//! URIs are returned directly; `url(...)` is interpreted as a path on
//! disk (relative to `base_url` when given). `local()` and HTTP(S) URLs
//! are recognized but unsupported — they emit a warning so the caller
//! can fall through to the next entry in the list.
//!
//! Layer 6 (`build_font_face_registry`) walks every parsed `FontFaceRule`
//! and turns successful loads into `RegisteredFont` entries plus a
//! `FontFaceLookup` keyed on (family-lower, weight, style). Failures
//! contribute a warning to the doc but never abort the build.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::RegisteredFont;
use crate::css::{FontFaceRule, FontFaceSrc, FontStyle, FontWeight, SrcFormat, SrcKind};

/// One reason a single `src:` entry failed to load. Carried through so
/// the caller can decide whether to fall through to the next entry, the
/// next family, or surface a `doc.warnings` line.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FontFaceLoadWarning {
    /// `format(...)` hint names a wrapper we don't handle (woff/woff2/...).
    UnsupportedFormat(String),
    /// `url(...)` uses a scheme other than `data:` / file path (e.g. `http`).
    UnsupportedScheme(String),
    /// `local(name)` source — we don't query the system font DB yet.
    UnsupportedLocal(String),
    /// I/O failure when reading a relative/absolute path src.
    Io(String),
    /// No `src:` entries — defensively guarded; parser already rejects this.
    NoSources,
}

impl FontFaceLoadWarning {
    pub fn user_message(&self, family: &str) -> String {
        match self {
            Self::UnsupportedFormat(fmt) => {
                format!("@font-face for {family:?}: format({fmt:?}) is not supported, skipping")
            }
            Self::UnsupportedScheme(url) => {
                format!("@font-face for {family:?}: url scheme not supported ({url:?}), skipping")
            }
            Self::UnsupportedLocal(name) => {
                format!("@font-face for {family:?}: local({name:?}) is not supported, skipping")
            }
            Self::Io(err) => format!("@font-face for {family:?}: failed to read src — {err}"),
            Self::NoSources => format!("@font-face for {family:?}: no src entries"),
        }
    }
}

/// Lookup from `(family-lowercase, weight, style)` to the registered
/// font name (`Custom-N`). Weight/style come from the `@font-face` rule
/// itself; cascade-time matching tolerates `Normal`/`Numeric(400)` etc.
/// being treated as equivalent.
pub type FontFaceLookup = HashMap<(String, FontWeight, FontStyle), String>;

/// Decide whether a given `format(...)` hint is one we can handle. Empty
/// (no hint) is always accepted — many real stylesheets omit it.
fn format_is_supported(format: Option<&SrcFormat>) -> bool {
    matches!(
        format,
        None | Some(SrcFormat::Truetype | SrcFormat::Opentype)
    )
}

/// Convert a `format(...)` hint into the string we surface in warnings.
fn format_label(format: &SrcFormat) -> String {
    match format {
        SrcFormat::Truetype => "truetype".to_string(),
        SrcFormat::Opentype => "opentype".to_string(),
        SrcFormat::Woff => "woff".to_string(),
        SrcFormat::Woff2 => "woff2".to_string(),
        SrcFormat::Other(s) => s.clone(),
    }
}

/// Resolve a single `url(...)` value to bytes, honoring `base_url` for
/// relative paths. HTTP/HTTPS schemes are explicitly rejected — we have
/// no network client and don't intend to grow one in this layer.
fn load_url_bytes(url: &str, base_url: Option<&Path>) -> Result<Vec<u8>, FontFaceLoadWarning> {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Err(FontFaceLoadWarning::UnsupportedScheme(url.to_string()));
    }
    let path_str = url.strip_prefix("file://").unwrap_or(url);
    let path = Path::new(path_str);
    let resolved: PathBuf = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(base) = base_url {
        base.join(path)
    } else {
        path.to_path_buf()
    };
    std::fs::read(&resolved).map_err(|e| FontFaceLoadWarning::Io(e.to_string()))
}

/// Walk `srcs` in order, returning the bytes from the first source we
/// can actually load. Any source we can't handle (unsupported format,
/// HTTP url, `local()`) contributes a warning that we keep around so the
/// caller can surface the *last* failure if every entry falls through.
#[allow(dead_code)]
pub fn resolve_font_face_src(
    srcs: &[FontFaceSrc],
    base_url: Option<&Path>,
) -> Result<Vec<u8>, FontFaceLoadWarning> {
    if srcs.is_empty() {
        return Err(FontFaceLoadWarning::NoSources);
    }
    let mut last_warning = FontFaceLoadWarning::NoSources;
    for src in srcs {
        if !format_is_supported(src.format.as_ref()) {
            last_warning =
                FontFaceLoadWarning::UnsupportedFormat(format_label(src.format.as_ref().unwrap()));
            continue;
        }
        match &src.kind {
            SrcKind::DataUri(bytes) => return Ok(bytes.clone()),
            SrcKind::Url(url) => match load_url_bytes(url, base_url) {
                Ok(b) => return Ok(b),
                Err(w) => last_warning = w,
            },
            SrcKind::Local(name) => {
                last_warning = FontFaceLoadWarning::UnsupportedLocal(name.clone());
            }
        }
    }
    Err(last_warning)
}

/// Map a `FontWeight` to its numeric value for distance comparison
/// during the cascade. `Normal` ≡ 400, `Bold` ≡ 700.
fn weight_to_num(w: FontWeight) -> u16 {
    match w {
        FontWeight::Normal => 400,
        FontWeight::Bold => 700,
        FontWeight::Numeric(n) => n,
    }
}

/// Resolve `(family, weight, style)` to a registered `Custom-N` name
/// using a parsed `@font-face` lookup. Falls back along the CSS Fonts
/// §5.2 priority: exact (family, weight, style) → same-family + same-
/// style closest weight → same-family any-style closest weight. Returns
/// `None` if `family_lower` isn't declared by any face.
#[allow(dead_code)]
pub fn resolve_font_face_for_cascade(
    family_lower: &str,
    weight: FontWeight,
    style: FontStyle,
    lookup: &FontFaceLookup,
) -> Option<String> {
    if let Some(name) = lookup.get(&(family_lower.to_string(), weight, style)) {
        return Some(name.clone());
    }
    let target = weight_to_num(weight);
    let mut best: Option<(u16, &String)> = None;
    for ((fam, w, st), name) in lookup {
        if fam != family_lower || *st != style {
            continue;
        }
        let dist = weight_to_num(*w).abs_diff(target);
        if best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, name));
        }
    }
    if let Some((_, n)) = best {
        return Some(n.clone());
    }
    let mut best: Option<(u16, &String)> = None;
    for ((fam, w, _), name) in lookup {
        if fam != family_lower {
            continue;
        }
        let dist = weight_to_num(*w).abs_diff(target);
        if best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, name));
        }
    }
    best.map(|(_, n)| n.clone())
}

/// Walk every parsed `@font-face` rule and produce a flat
/// `Vec<RegisteredFont>` (so the existing custom-font subsetting
/// pipeline can pick them up unchanged) plus a `FontFaceLookup` for
/// cascade matching. Rules whose `src:` resolves to nothing are dropped
/// with a warning — never panic.
#[allow(dead_code)]
pub fn build_font_face_registry(
    face_rules: &[FontFaceRule],
    base_url: Option<&Path>,
) -> (Vec<RegisteredFont>, FontFaceLookup, Vec<String>) {
    let mut registered: Vec<RegisteredFont> = Vec::new();
    let mut lookup: FontFaceLookup = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    for rule in face_rules {
        match resolve_font_face_src(&rule.src, base_url) {
            Ok(data) => {
                let name = format!("Custom-{}", registered.len());
                let weight = rule.weight.unwrap_or(FontWeight::Normal);
                let style = rule.style.unwrap_or(FontStyle::Normal);
                lookup.insert(
                    (rule.family.to_ascii_lowercase(), weight, style),
                    name.clone(),
                );
                registered.push(RegisteredFont {
                    data,
                    family: rule.family.clone(),
                    name,
                });
            }
            Err(w) => {
                warnings.push(w.user_message(&rule.family));
            }
        }
    }

    (registered, lookup, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::{FontFaceSrc, SrcFormat, SrcKind};
    use std::io::Write;

    fn data_uri(bytes: &[u8]) -> FontFaceSrc {
        FontFaceSrc {
            kind: SrcKind::DataUri(bytes.to_vec()),
            format: None,
        }
    }

    #[test]
    fn resolve_picks_first_data_uri() {
        let srcs = vec![data_uri(b"hello")];
        let bytes = resolve_font_face_src(&srcs, None).expect("loads");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn resolve_skips_format_woff2_and_falls_to_next() {
        let srcs = vec![
            FontFaceSrc {
                kind: SrcKind::Url("ignored.woff2".to_string()),
                format: Some(SrcFormat::Woff2),
            },
            data_uri(b"abc"),
        ];
        let bytes = resolve_font_face_src(&srcs, None).expect("loads second");
        assert_eq!(bytes, b"abc");
    }

    #[test]
    fn resolve_rejects_http_url_with_warning() {
        let srcs = vec![FontFaceSrc {
            kind: SrcKind::Url("https://example.com/font.ttf".to_string()),
            format: None,
        }];
        let err = resolve_font_face_src(&srcs, None).expect_err("rejects");
        assert!(matches!(err, FontFaceLoadWarning::UnsupportedScheme(_)));
    }

    #[test]
    fn resolve_emits_warning_on_local() {
        let srcs = vec![FontFaceSrc {
            kind: SrcKind::Local("Helvetica".to_string()),
            format: None,
        }];
        let err = resolve_font_face_src(&srcs, None).expect_err("rejects");
        assert!(matches!(err, FontFaceLoadWarning::UnsupportedLocal(_)));
    }

    #[test]
    fn resolve_relative_path_uses_base_url() {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        dir.push(format!("pdfun-test-{}-{}", std::process::id(), nanos));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let path = dir.join("font.bin");
        let payload = b"abcdef";
        let mut f = std::fs::File::create(&path).expect("create");
        f.write_all(payload).expect("write");

        let srcs = vec![FontFaceSrc {
            kind: SrcKind::Url("font.bin".to_string()),
            format: None,
        }];
        let bytes = resolve_font_face_src(&srcs, Some(&dir)).expect("loads");
        assert_eq!(bytes, payload);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    // Layer 6 tests

    fn rule_with(srcs: Vec<FontFaceSrc>, family: &str) -> FontFaceRule {
        FontFaceRule {
            family: family.to_string(),
            src: srcs,
            weight: None,
            style: None,
        }
    }

    #[test]
    fn single_rule_registers_as_custom_0() {
        let rules = vec![rule_with(vec![data_uri(b"AAA")], "MyFont")];
        let (registered, lookup, warnings) = build_font_face_registry(&rules, None);
        assert!(warnings.is_empty());
        assert_eq!(registered.len(), 1);
        assert_eq!(registered[0].name, "Custom-0");
        assert_eq!(registered[0].family, "MyFont");
        assert_eq!(registered[0].data, b"AAA");
        let key = ("myfont".to_string(), FontWeight::Normal, FontStyle::Normal);
        assert_eq!(lookup.get(&key), Some(&"Custom-0".to_string()));
    }

    #[test]
    fn two_rules_same_family_different_weight() {
        let mut regular = rule_with(vec![data_uri(b"R")], "F");
        regular.weight = Some(FontWeight::Numeric(400));
        let mut bold = rule_with(vec![data_uri(b"B")], "F");
        bold.weight = Some(FontWeight::Numeric(700));
        let (registered, lookup, warnings) = build_font_face_registry(&[regular, bold], None);
        assert!(warnings.is_empty());
        assert_eq!(registered.len(), 2);
        assert_eq!(registered[0].name, "Custom-0");
        assert_eq!(registered[1].name, "Custom-1");
        assert_eq!(
            lookup.get(&("f".to_string(), FontWeight::Numeric(400), FontStyle::Normal)),
            Some(&"Custom-0".to_string())
        );
        assert_eq!(
            lookup.get(&("f".to_string(), FontWeight::Numeric(700), FontStyle::Normal)),
            Some(&"Custom-1".to_string())
        );
    }

    #[test]
    fn cascade_resolves_font_face_match_to_custom_name() {
        let mut lookup: FontFaceLookup = HashMap::new();
        lookup.insert(
            ("myfont".to_string(), FontWeight::Normal, FontStyle::Normal),
            "Custom-0".to_string(),
        );
        let resolved =
            resolve_font_face_for_cascade("myfont", FontWeight::Normal, FontStyle::Normal, &lookup);
        assert_eq!(resolved, Some("Custom-0".to_string()));
    }

    #[test]
    fn cascade_picks_closest_weight_when_exact_missing() {
        let mut lookup: FontFaceLookup = HashMap::new();
        lookup.insert(
            ("f".to_string(), FontWeight::Numeric(400), FontStyle::Normal),
            "Custom-0".to_string(),
        );
        lookup.insert(
            ("f".to_string(), FontWeight::Numeric(700), FontStyle::Normal),
            "Custom-1".to_string(),
        );
        // weight 600 is not exactly registered. |400-600|=200, |700-600|=100,
        // so 700 wins.
        let resolved = resolve_font_face_for_cascade(
            "f",
            FontWeight::Numeric(600),
            FontStyle::Normal,
            &lookup,
        );
        assert_eq!(resolved, Some("Custom-1".to_string()));
    }

    #[test]
    fn cascade_unknown_family_returns_none() {
        let lookup: FontFaceLookup = HashMap::new();
        assert!(
            resolve_font_face_for_cascade(
                "nothere",
                FontWeight::Normal,
                FontStyle::Normal,
                &lookup
            )
            .is_none()
        );
    }

    #[test]
    fn failed_rule_does_not_register_but_emits_warning() {
        let bad = rule_with(
            vec![FontFaceSrc {
                kind: SrcKind::Local("Helvetica".to_string()),
                format: None,
            }],
            "F",
        );
        let (registered, lookup, warnings) = build_font_face_registry(&[bad], None);
        assert!(registered.is_empty());
        assert!(lookup.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("local"));
    }
}
