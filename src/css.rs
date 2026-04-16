use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, Token,
    color::{parse_hash_color, parse_named_color},
};

use crate::layout::TextAlign;

// ── CSS value types ─────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CssLength {
    Px(f32),
    Pt(f32),
    Em(f32),
    Rem(f32),
    In(f32),
    Vw(f32),
    Vh(f32),
    /// CSS percentage (e.g. `50%`). Resolves against `LengthContext::container`
    /// — the caller is responsible for setting `container` to the correct
    /// reference value for the property (content width for horizontal
    /// margins/paddings/widths, content height for heights, etc.).
    Pct(f32),
}

/// Per-element context needed to resolve relative CSS lengths to points.
#[derive(Clone, Copy, Debug)]
pub struct LengthContext {
    /// Current element's font size in points (for `em`).
    pub em: f32,
    /// Root element's font size in points (for `rem`).
    pub rem: f32,
    /// Viewport (page) width in points (for `vw`).
    pub vw: f32,
    /// Viewport (page) height in points (for `vh`).
    pub vh: f32,
    /// Containing-block reference value for percentages. Default is the
    /// containing block's content width (the usual choice — margins,
    /// paddings, and widths all resolve against width per CSS spec).
    /// Callers resolving height-related percents should build a second
    /// context with `container` set to the containing height.
    pub container: f32,
}

impl LengthContext {
    pub const DEFAULT_EM: f32 = 12.0;
    pub const DEFAULT_VW: f32 = 612.0;
    pub const DEFAULT_VH: f32 = 792.0;

    /// Sensible default context: 12pt body font, US-Letter viewport. Used in
    /// tests and code paths that don't yet thread a real renderer context.
    pub fn fallback() -> Self {
        Self {
            em: Self::DEFAULT_EM,
            rem: Self::DEFAULT_EM,
            vw: Self::DEFAULT_VW,
            vh: Self::DEFAULT_VH,
            container: Self::DEFAULT_VW,
        }
    }
}

impl CssLength {
    /// Resolve to points. `em_base` is the current font size; `rem`/`vw`/`vh`
    /// fall back to sensible defaults when not supplied. Prefer
    /// [`resolve_ctx`] when you already have a `LengthContext`.
    pub fn resolve(self, em_base: f32) -> f32 {
        self.resolve_ctx(&LengthContext {
            em: em_base,
            rem: em_base,
            vw: LengthContext::DEFAULT_VW,
            vh: LengthContext::DEFAULT_VH,
            container: LengthContext::DEFAULT_VW,
        })
    }

    /// Resolve to points using the full context. Required for `rem`, `vw`,
    /// `vh`, and percentages to produce correct values.
    pub fn resolve_ctx(self, ctx: &LengthContext) -> f32 {
        match self {
            CssLength::Px(v) => v * 0.75, // 96 dpi CSS px → 72 dpi PDF pt
            CssLength::Pt(v) => v,
            CssLength::Em(v) => v * ctx.em,
            CssLength::Rem(v) => v * ctx.rem,
            CssLength::In(v) => v * 72.0, // 1 inch = 72 PDF points
            CssLength::Vw(v) => v * ctx.vw / 100.0,
            CssLength::Vh(v) => v * ctx.vh / 100.0,
            CssLength::Pct(v) => v * ctx.container / 100.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontWeight {
    Normal,
    Bold,
    Numeric(u16),
}

impl FontWeight {
    pub fn is_bold(self) -> bool {
        match self {
            FontWeight::Normal => false,
            FontWeight::Bold => true,
            FontWeight::Numeric(n) => n >= 700,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FontStyle {
    Normal,
    Italic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DisplayValue {
    Block,
    Inline,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextDecoration {
    pub underline: bool,
    pub line_through: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BorderStyle {
    None,
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageBreak {
    Auto,
    Always,
    Avoid,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BoxSizing {
    /// CSS default: `width`/`height` refer to the content area.
    #[default]
    ContentBox,
    /// `width`/`height` include padding and border.
    BorderBox,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListStyleType {
    None,
    Disc,
    Circle,
    Square,
    Decimal,
    LowerAlpha,
    UpperAlpha,
    LowerRoman,
    UpperRoman,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
}

// ── PageStyle (@page rule) ──────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct PageStyle {
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_boxes: MarginBoxes,
}

/// A single `content:` value item inside a `@page` margin box.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentItem {
    /// A plain string literal (e.g. `"Page "`).
    String(String),
    /// The `counter(page)` function — replaced at render time with the
    /// 1-indexed current page number.
    CounterPage,
    /// The `counter(pages)` function — replaced at render time with the
    /// total page count.
    CounterPages,
}

/// Position of a `@page` margin box. Only the six most common positions
/// are supported; the four corner / side boxes from the CSS spec are out
/// of scope for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarginBoxPosition {
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// A single margin box declared inside `@page { ... }`.
#[derive(Clone, Debug, Default)]
pub struct MarginBox {
    /// Parsed `content:` value. Empty = no content (box is effectively
    /// inert, matching the spec which says the box generates no content).
    pub content: Vec<ContentItem>,
    /// Optional font family override. `None` = inherit / Helvetica.
    pub font_family: Option<String>,
    /// Optional font-size override in points. `None` = 10pt default.
    pub font_size: Option<f32>,
    /// Optional color override. `None` = black.
    pub color: Option<(f32, f32, f32)>,
    /// Optional text-align override. `None` = position default
    /// (left/center/right based on `MarginBoxPosition`).
    pub text_align: Option<TextAlign>,
}

/// Holder for the six supported margin-box positions.
#[derive(Clone, Debug, Default)]
pub struct MarginBoxes {
    pub top_left: Option<MarginBox>,
    pub top_center: Option<MarginBox>,
    pub top_right: Option<MarginBox>,
    pub bottom_left: Option<MarginBox>,
    pub bottom_center: Option<MarginBox>,
    pub bottom_right: Option<MarginBox>,
}

impl MarginBoxes {
    pub fn set(&mut self, pos: MarginBoxPosition, margin_box: MarginBox) {
        match pos {
            MarginBoxPosition::TopLeft => self.top_left = Some(margin_box),
            MarginBoxPosition::TopCenter => self.top_center = Some(margin_box),
            MarginBoxPosition::TopRight => self.top_right = Some(margin_box),
            MarginBoxPosition::BottomLeft => self.bottom_left = Some(margin_box),
            MarginBoxPosition::BottomCenter => self.bottom_center = Some(margin_box),
            MarginBoxPosition::BottomRight => self.bottom_right = Some(margin_box),
        }
    }

    /// Returns true if any margin box is set.
    pub fn any(&self) -> bool {
        self.top_left.is_some()
            || self.top_center.is_some()
            || self.top_right.is_some()
            || self.bottom_left.is_some()
            || self.bottom_center.is_some()
            || self.bottom_right.is_some()
    }
}

fn margin_box_position_for(name: &str) -> Option<MarginBoxPosition> {
    match name {
        "top-left" => Some(MarginBoxPosition::TopLeft),
        "top-center" => Some(MarginBoxPosition::TopCenter),
        "top-right" => Some(MarginBoxPosition::TopRight),
        "bottom-left" => Some(MarginBoxPosition::BottomLeft),
        "bottom-center" => Some(MarginBoxPosition::BottomCenter),
        "bottom-right" => Some(MarginBoxPosition::BottomRight),
        _ => None,
    }
}

// ── ComputedStyle field table ───────────────────────────────
//
// `with_style_fields!` is the single source of truth for every
// `ComputedStyle` field. Each entry is `(name, kind, inherited)`:
//   - kind: `copy` for `Option<impl Copy>`, `clone` for `Option<impl Clone>`
//   - inherited: `yes` if CSS-inheritable, `no` otherwise
//
// The generator macros below (`gen_merge`, `gen_has_any`, `gen_inherit`)
// are invoked via this table, so adding a new CSS property means editing
// the field list here + adding one match arm to the declaration parser,
// and the merge / inherit / has_any plumbing is generated automatically.

macro_rules! with_style_fields {
    ($m:ident) => {
        $m! {
            (color,             copy,  yes),
            (background_color,  copy,  no),
            (font_size,         copy,  yes),
            (font_weight,       copy,  yes),
            (font_style,        copy,  yes),
            (font_family,       clone, yes),
            (text_align,        clone, yes),
            (line_height,       copy,  yes),
            (margin_top,        copy,  no),
            (margin_right,      copy,  no),
            (margin_bottom,     copy,  no),
            (margin_left,       copy,  no),
            (padding_top,       copy,  no),
            (padding_right,     copy,  no),
            (padding_bottom,    copy,  no),
            (padding_left,      copy,  no),
            (border_width,      copy,  no),
            (border_color,      copy,  no),
            (border_style,      copy,  no),
            (column_count,      copy,  no),
            (column_gap,        copy,  no),
            (column_rule_width, copy,  no),
            (column_rule_color, copy,  no),
            (display,           copy,  no),
            (text_decoration,   copy,  yes),
            (list_style_type,   copy,  yes),
            (list_style_position, copy, yes),
            (page_break_before, copy,  no),
            (page_break_after,  copy,  no),
            (width,             copy,  no),
            (height,            copy,  no),
            (min_width,         copy,  no),
            (min_height,        copy,  no),
            (max_width,         copy,  no),
            (max_height,        copy,  no),
            (letter_spacing,    copy,  yes),
            (word_spacing,      copy,  yes),
            (box_sizing,        copy,  no),
            (text_transform,    copy,  yes),
            (text_indent,       copy,  yes),
        }
    };
}

macro_rules! merge_one {
    (copy, $t:ident, $s:ident, $f:ident) => {
        if $s.$f.is_some() { $t.$f = $s.$f; }
    };
    (clone, $t:ident, $s:ident, $f:ident) => {
        if $s.$f.is_some() { $t.$f.clone_from(&$s.$f); }
    };
}

macro_rules! inherit_one {
    (copy, yes, $r:ident, $c:ident, $p:ident, $f:ident) => {
        $r.$f = $c.and_then(|c| c.$f).or($p.$f);
    };
    (clone, yes, $r:ident, $c:ident, $p:ident, $f:ident) => {
        $r.$f = $c.and_then(|c| c.$f.clone()).or_else(|| $p.$f.clone());
    };
    ($kind:ident, no, $r:ident, $c:ident, $p:ident, $f:ident) => {};
}

macro_rules! gen_merge {
    ($( ($name:ident, $kind:ident, $inh:ident) ),* $(,)?) => {
        /// Merge non-None fields from `source` into `target`.
        pub fn merge_style(target: &mut ComputedStyle, source: &ComputedStyle) {
            $( merge_one!($kind, target, source, $name); )*
        }
    };
}

macro_rules! gen_style_impl {
    ($( ($name:ident, $kind:ident, $inh:ident) ),* $(,)?) => {
        impl ComputedStyle {
            /// Returns true if any property is set (non-None).
            pub fn has_any_property(&self) -> bool {
                false $( || self.$name.is_some() )*
            }

            /// Build inherited style for a child element. Inheritable
            /// properties fall back to parent's value when the child
            /// leaves them unset; non-inheritable properties reset.
            pub fn inherit_into(&self, child: &Option<ComputedStyle>) -> ComputedStyle {
                let mut result = ComputedStyle::default();
                let c = child.as_ref();
                $( inherit_one!($kind, $inh, result, c, self, $name); )*
                result
            }
        }
    };
}

with_style_fields!(gen_style_impl);
with_style_fields!(gen_merge);

// ── ComputedStyle ───────────────────────────────────────────

#[derive(Clone, Debug, Default)]
pub struct ComputedStyle {
    pub color: Option<(f32, f32, f32)>,
    pub background_color: Option<(f32, f32, f32)>,
    pub font_size: Option<CssLength>,
    pub font_weight: Option<FontWeight>,
    pub font_style: Option<FontStyle>,
    pub font_family: Option<String>,
    pub text_align: Option<TextAlign>,
    pub line_height: Option<CssLength>,
    pub margin_top: Option<CssLength>,
    pub margin_right: Option<CssLength>,
    pub margin_bottom: Option<CssLength>,
    pub margin_left: Option<CssLength>,
    pub padding_top: Option<CssLength>,
    pub padding_right: Option<CssLength>,
    pub padding_bottom: Option<CssLength>,
    pub padding_left: Option<CssLength>,
    pub border_width: Option<CssLength>,
    pub border_color: Option<(f32, f32, f32)>,
    pub border_style: Option<BorderStyle>,
    pub column_count: Option<u32>,
    pub column_gap: Option<CssLength>,
    pub column_rule_width: Option<CssLength>,
    pub column_rule_color: Option<(f32, f32, f32)>,
    pub display: Option<DisplayValue>,
    pub text_decoration: Option<TextDecoration>,
    pub list_style_type: Option<ListStyleType>,
    pub list_style_position: Option<ListStylePosition>,
    pub page_break_before: Option<PageBreak>,
    pub page_break_after: Option<PageBreak>,
    pub width: Option<CssLength>,
    pub height: Option<CssLength>,
    pub min_width: Option<CssLength>,
    pub min_height: Option<CssLength>,
    pub max_width: Option<CssLength>,
    pub max_height: Option<CssLength>,
    pub letter_spacing: Option<CssLength>,
    pub word_spacing: Option<CssLength>,
    pub box_sizing: Option<BoxSizing>,
    pub text_transform: Option<TextTransform>,
    pub text_indent: Option<CssLength>,
}


// ── Color parsing ───────────────────────────────────────────

fn u8_to_f32(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    )
}

fn parse_css_color<'i>(input: &mut Parser<'i, '_>) -> Result<(f32, f32, f32), ParseError<'i, ()>> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    match &token {
        Token::Hash(value) | Token::IDHash(value) => {
            let (r, g, b, _a) =
                parse_hash_color(value.as_bytes()).map_err(|()| location.new_custom_error(()))?;
            Ok(u8_to_f32(r, g, b))
        }
        Token::Ident(ident) => {
            let (r, g, b) = parse_named_color(ident).map_err(|()| location.new_custom_error(()))?;
            Ok(u8_to_f32(r, g, b))
        }
        Token::Function(name)
            if name.eq_ignore_ascii_case("rgb") || name.eq_ignore_ascii_case("rgba") =>
        {
            input.parse_nested_block(|input| {
                let r = input.expect_number()?.clamp(0.0, 255.0);
                input.expect_comma()?;
                let g = input.expect_number()?.clamp(0.0, 255.0);
                input.expect_comma()?;
                let b = input.expect_number()?.clamp(0.0, 255.0);
                // Ignore optional alpha
                Ok((r / 255.0, g / 255.0, b / 255.0))
            })
        }
        Token::Function(name)
            if name.eq_ignore_ascii_case("hsl") || name.eq_ignore_ascii_case("hsla") =>
        {
            input.parse_nested_block(|input| {
                let h = expect_hue(input)?;
                input.expect_comma()?;
                let s = input.expect_percentage()?.clamp(0.0, 1.0);
                input.expect_comma()?;
                let l = input.expect_percentage()?.clamp(0.0, 1.0);
                // Optional alpha: ", <number>" — parsed and ignored.
                let _ = input.try_parse(|i| -> Result<(), ParseError<'i, ()>> {
                    i.expect_comma()?;
                    let _ = i.expect_number()?;
                    Ok(())
                });
                let (r, g, b) = hsl_to_rgb(h, s, l);
                Ok((r, g, b))
            })
        }
        _ => Err(location.new_custom_error(())),
    }
}

fn expect_hue<'i>(input: &mut Parser<'i, '_>) -> Result<f32, ParseError<'i, ()>> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    match &token {
        Token::Number { value, .. } => Ok(*value),
        Token::Dimension { value, unit, .. } => {
            let unit = unit.to_ascii_lowercase();
            let degrees = match unit.as_str() {
                "deg" => *value,
                "grad" => *value * 0.9,
                "rad" => value.to_degrees(),
                "turn" => *value * 360.0,
                _ => return Err(location.new_custom_error(())),
            };
            Ok(degrees)
        }
        _ => Err(location.new_custom_error(())),
    }
}

fn hsl_to_rgb(hue_deg: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let h = hue_deg.rem_euclid(360.0) / 60.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = if h < 1.0 {
        (c, x, 0.0)
    } else if h < 2.0 {
        (x, c, 0.0)
    } else if h < 3.0 {
        (0.0, c, x)
    } else if h < 4.0 {
        (0.0, x, c)
    } else if h < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    (r1 + m, g1 + m, b1 + m)
}

// ── Length parsing ──────────────────────────────────────────

fn parse_css_length<'i>(input: &mut Parser<'i, '_>) -> Result<CssLength, ParseError<'i, ()>> {
    let location = input.current_source_location();
    let token = input.next()?.clone();
    match &token {
        Token::Dimension { value, unit, .. } => {
            if unit.eq_ignore_ascii_case("px") {
                Ok(CssLength::Px(*value))
            } else if unit.eq_ignore_ascii_case("pt") {
                Ok(CssLength::Pt(*value))
            } else if unit.eq_ignore_ascii_case("em") {
                Ok(CssLength::Em(*value))
            } else if unit.eq_ignore_ascii_case("rem") {
                Ok(CssLength::Rem(*value))
            } else if unit.eq_ignore_ascii_case("in") {
                Ok(CssLength::In(*value))
            } else if unit.eq_ignore_ascii_case("vw") {
                Ok(CssLength::Vw(*value))
            } else if unit.eq_ignore_ascii_case("vh") {
                Ok(CssLength::Vh(*value))
            } else {
                Err(location.new_custom_error(()))
            }
        }
        Token::Number { value, .. } if *value == 0.0 => Ok(CssLength::Px(0.0)),
        Token::Percentage { unit_value, .. } => Ok(CssLength::Pct(*unit_value * 100.0)),
        _ => Err(location.new_custom_error(())),
    }
}

/// Parse a CSS length that must be non-negative (for font-size, padding, border-width).
fn parse_non_negative_length<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<CssLength, ParseError<'i, ()>> {
    let location = input.current_source_location();
    let len = parse_css_length(input)?;
    let value = match len {
        CssLength::Px(v)
        | CssLength::Pt(v)
        | CssLength::Em(v)
        | CssLength::Rem(v)
        | CssLength::In(v)
        | CssLength::Vw(v)
        | CssLength::Vh(v)
        | CssLength::Pct(v) => v,
    };
    if value < 0.0 {
        return Err(location.new_custom_error(()));
    }
    Ok(len)
}

/// Parse a non-negative length, or accept the given keyword as "not set"
/// (returning Ok(None)). Used for `width: auto`, `max-width: none`, etc. —
/// where the keyword is semantically the default, so we can drop the field.
fn parse_length_or_keyword<'i>(
    input: &mut Parser<'i, '_>,
    keyword: &'static str,
) -> Result<Option<CssLength>, ParseError<'i, ()>> {
    if input
        .try_parse(|i| {
            let ident = i.expect_ident()?;
            if ident.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(i.new_custom_error::<_, ()>(()))
            }
        })
        .is_ok()
    {
        return Ok(None);
    }
    parse_non_negative_length(input).map(Some)
}

/// Parse a CSS length (possibly negative) or accept the given keyword as
/// "not set" (returning `Ok(None)`). Used for `letter-spacing: normal`,
/// `word-spacing: normal`, etc. — properties that allow negative values.
fn parse_signed_length_or_keyword<'i>(
    input: &mut Parser<'i, '_>,
    keyword: &'static str,
) -> Result<Option<CssLength>, ParseError<'i, ()>> {
    if input
        .try_parse(|i| {
            let ident = i.expect_ident()?;
            if ident.eq_ignore_ascii_case(keyword) {
                Ok(())
            } else {
                Err(i.new_custom_error::<_, ()>(()))
            }
        })
        .is_ok()
    {
        return Ok(None);
    }
    parse_css_length(input).map(Some)
}

// ── Shorthand expansion ────────────────────────────────────

/// Parse 1-4 length values for margin/padding shorthand.
/// Returns (top, right, bottom, left).
fn parse_box_shorthand<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<(CssLength, CssLength, CssLength, CssLength), ParseError<'i, ()>> {
    let first = parse_css_length(input)?;
    let second = input.try_parse(parse_css_length).ok();
    let third = input.try_parse(parse_css_length).ok();
    let fourth = input.try_parse(parse_css_length).ok();

    match (second, third, fourth) {
        (None, _, _) => Ok((first, first, first, first)),
        (Some(s), None, _) => Ok((first, s, first, s)),
        (Some(s), Some(t), None) => Ok((first, s, t, s)),
        (Some(s), Some(t), Some(f)) => Ok((first, s, t, f)),
    }
}

/// Non-negative variant of `parse_box_shorthand` for padding.
fn parse_non_negative_box_shorthand<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<(CssLength, CssLength, CssLength, CssLength), ParseError<'i, ()>> {
    let first = parse_non_negative_length(input)?;
    let second = input.try_parse(parse_non_negative_length).ok();
    let third = input.try_parse(parse_non_negative_length).ok();
    let fourth = input.try_parse(parse_non_negative_length).ok();

    match (second, third, fourth) {
        (None, _, _) => Ok((first, first, first, first)),
        (Some(s), None, _) => Ok((first, s, first, s)),
        (Some(s), Some(t), None) => Ok((first, s, t, s)),
        (Some(s), Some(t), Some(f)) => Ok((first, s, t, f)),
    }
}

// ── Declaration parser ─────────────────────────────────────

struct StyleDeclarationParser<'s> {
    style: &'s mut ComputedStyle,
}

#[allow(clippy::too_many_lines)]
impl<'i> DeclarationParser<'i> for StyleDeclarationParser<'_> {
    type Declaration = ();
    type Error = ();

    // NOTE: `!important` is currently parsed-and-ignored (cssparser consumes
    // the `!important` marker after this method returns). A full CSS-spec
    // implementation would give important declarations higher cascade
    // priority than their specificity would otherwise suggest; we don't do
    // that yet. Until we do, a `color: red !important` rule can still be
    // overridden by a later `color: blue`. If that matters to you, move the
    // target rule later in the stylesheet or increase its specificity.
    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _start: &cssparser::ParserState,
    ) -> Result<(), ParseError<'i, ()>> {
        let name_lower = name.to_ascii_lowercase();
        match name_lower.as_str() {
            "color" => {
                self.style.color = Some(parse_css_color(input)?);
            }
            "background-color" => {
                self.style.background_color = Some(parse_css_color(input)?);
            }
            "font-size" => {
                self.style.font_size = Some(parse_non_negative_length(input)?);
            }
            "font-weight" => {
                let location = input.current_source_location();
                let token = input.next()?.clone();
                match &token {
                    Token::Ident(ident) => {
                        if ident.eq_ignore_ascii_case("bold") {
                            self.style.font_weight = Some(FontWeight::Bold);
                        } else if ident.eq_ignore_ascii_case("normal") {
                            self.style.font_weight = Some(FontWeight::Normal);
                        } else {
                            return Err(location.new_custom_error(()));
                        }
                    }
                    Token::Number {
                        int_value: Some(n), ..
                    } => {
                        let w = u16::try_from(*n).map_err(|_| location.new_custom_error(()))?;
                        if (100..=900).contains(&w) && w % 100 == 0 {
                            self.style.font_weight = Some(FontWeight::Numeric(w));
                        } else {
                            return Err(location.new_custom_error(()));
                        }
                    }
                    _ => return Err(location.new_custom_error(())),
                }
            }
            "font-style" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                if ident.eq_ignore_ascii_case("italic") {
                    self.style.font_style = Some(FontStyle::Italic);
                } else if ident.eq_ignore_ascii_case("normal") {
                    self.style.font_style = Some(FontStyle::Normal);
                } else {
                    return Err(location.new_custom_error(()));
                }
            }
            "font-family" => {
                let mut families = Vec::new();
                loop {
                    let Ok(token) = input.next().cloned() else {
                        break;
                    };
                    match &token {
                        Token::Ident(ident) => families.push(ident.to_string()),
                        Token::QuotedString(s) => families.push(s.to_string()),
                        _ => break,
                    }
                    if input.try_parse(|i| i.expect_comma()).is_err() {
                        break;
                    }
                }
                if families.is_empty() {
                    return Err(input.current_source_location().new_custom_error(()));
                }
                self.style.font_family = Some(families.join(","));
            }
            "text-align" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                if ident.eq_ignore_ascii_case("left") {
                    self.style.text_align = Some(TextAlign::Left);
                } else if ident.eq_ignore_ascii_case("center") {
                    self.style.text_align = Some(TextAlign::Center);
                } else if ident.eq_ignore_ascii_case("right") {
                    self.style.text_align = Some(TextAlign::Right);
                } else if ident.eq_ignore_ascii_case("justify") {
                    self.style.text_align = Some(TextAlign::Justify);
                } else {
                    return Err(location.new_custom_error(()));
                }
            }
            "line-height" => {
                // Try length first, then bare number (treated as em)
                if let Ok(len) = input.try_parse(parse_css_length) {
                    self.style.line_height = Some(len);
                } else {
                    let n = input.expect_number()?;
                    self.style.line_height = Some(CssLength::Em(n));
                }
            }
            "margin" => {
                let (top, right, bottom, left) = parse_box_shorthand(input)?;
                self.style.margin_top = Some(top);
                self.style.margin_right = Some(right);
                self.style.margin_bottom = Some(bottom);
                self.style.margin_left = Some(left);
            }
            "margin-top" => {
                self.style.margin_top = Some(parse_css_length(input)?);
            }
            "margin-right" => {
                self.style.margin_right = Some(parse_css_length(input)?);
            }
            "margin-bottom" => {
                self.style.margin_bottom = Some(parse_css_length(input)?);
            }
            "margin-left" => {
                self.style.margin_left = Some(parse_css_length(input)?);
            }
            "padding" => {
                let (top, right, bottom, left) = parse_non_negative_box_shorthand(input)?;
                self.style.padding_top = Some(top);
                self.style.padding_right = Some(right);
                self.style.padding_bottom = Some(bottom);
                self.style.padding_left = Some(left);
            }
            "padding-top" => {
                self.style.padding_top = Some(parse_non_negative_length(input)?);
            }
            "padding-right" => {
                self.style.padding_right = Some(parse_non_negative_length(input)?);
            }
            "padding-bottom" => {
                self.style.padding_bottom = Some(parse_non_negative_length(input)?);
            }
            "padding-left" => {
                self.style.padding_left = Some(parse_non_negative_length(input)?);
            }
            "border-width" => {
                self.style.border_width = Some(parse_non_negative_length(input)?);
            }
            "border-color" => {
                self.style.border_color = Some(parse_css_color(input)?);
            }
            "border-style" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                match ident.to_ascii_lowercase().as_str() {
                    "none" | "hidden" => {
                        self.style.border_style = Some(BorderStyle::None);
                    }
                    "solid" => self.style.border_style = Some(BorderStyle::Solid),
                    "dashed" => self.style.border_style = Some(BorderStyle::Dashed),
                    "dotted" => self.style.border_style = Some(BorderStyle::Dotted),
                    _ => return Err(location.new_custom_error(())),
                }
            }
            "border" => {
                // border shorthand: parse width, style, and/or color in any order
                let mut found = false;
                for _ in 0..3 {
                    if let Ok(len) = input.try_parse(parse_non_negative_length) {
                        self.style.border_width = Some(len);
                        found = true;
                    } else if let Ok(color) = input.try_parse(parse_css_color) {
                        self.style.border_color = Some(color);
                        found = true;
                    } else if let Ok(style) = input.try_parse(|i: &mut Parser<'i, '_>| {
                        let ident = i.expect_ident()?.clone();
                        match ident.to_ascii_lowercase().as_str() {
                            "solid" => Ok::<_, ParseError<'i, ()>>(BorderStyle::Solid),
                            "dashed" => Ok(BorderStyle::Dashed),
                            "dotted" => Ok(BorderStyle::Dotted),
                            "none" | "hidden" => Ok(BorderStyle::None),
                            _ => Err(i.new_custom_error(())),
                        }
                    }) {
                        self.style.border_style = Some(style);
                        found = true;
                    } else {
                        break;
                    }
                }
                if !found {
                    return Err(input.new_custom_error(()));
                }
            }
            "column-count" => {
                let n = input.expect_integer()?;
                if n >= 1 {
                    self.style.column_count = Some(n as u32);
                } else {
                    return Err(input.new_custom_error(()));
                }
            }
            "column-gap" => {
                self.style.column_gap = Some(parse_non_negative_length(input)?);
            }
            "column-rule-width" => {
                self.style.column_rule_width = Some(parse_non_negative_length(input)?);
            }
            "column-rule-color" => {
                self.style.column_rule_color = Some(parse_css_color(input)?);
            }
            "column-rule" => {
                // shorthand: <width> <style> <color> in any order
                let mut found = false;
                for _ in 0..3 {
                    if let Ok(len) = input.try_parse(parse_non_negative_length) {
                        self.style.column_rule_width = Some(len);
                        found = true;
                    } else if let Ok(color) = input.try_parse(parse_css_color) {
                        self.style.column_rule_color = Some(color);
                        found = true;
                    } else if input
                        .try_parse(|i: &mut Parser<'i, '_>| {
                            let ident = i.expect_ident()?;
                            if matches!(
                                ident.to_ascii_lowercase().as_str(),
                                "solid" | "dashed" | "dotted" | "none"
                            ) {
                                Ok::<(), ParseError<'i, ()>>(())
                            } else {
                                Err(i.new_custom_error(()))
                            }
                        })
                        .is_ok()
                    {
                        found = true;
                    } else {
                        break;
                    }
                }
                if !found {
                    return Err(input.new_custom_error(()));
                }
            }
            "text-decoration" | "text-decoration-line" => {
                let mut underline = false;
                let mut line_through = false;
                loop {
                    let Ok(ident) = input.try_parse(|i| i.expect_ident().cloned()) else {
                        break;
                    };
                    match ident.to_ascii_lowercase().as_str() {
                        "underline" => underline = true,
                        "line-through" => line_through = true,
                        "none" => {
                            underline = false;
                            line_through = false;
                            break;
                        }
                        _ => {} // ignore overline, etc.
                    }
                }
                self.style.text_decoration =
                    Some(TextDecoration { underline, line_through });
            }
            "page-break-before" | "break-before" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let pb = match ident.to_ascii_lowercase().as_str() {
                    "auto" => PageBreak::Auto,
                    "always" | "page" | "left" | "right" => PageBreak::Always,
                    "avoid" | "avoid-page" => PageBreak::Avoid,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.page_break_before = Some(pb);
            }
            "page-break-after" | "break-after" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let pb = match ident.to_ascii_lowercase().as_str() {
                    "auto" => PageBreak::Auto,
                    "always" | "page" | "left" | "right" => PageBreak::Always,
                    "avoid" | "avoid-page" => PageBreak::Avoid,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.page_break_after = Some(pb);
            }
            "width" => {
                self.style.width = parse_length_or_keyword(input, "auto")?;
            }
            "height" => {
                self.style.height = parse_length_or_keyword(input, "auto")?;
            }
            "min-width" => {
                self.style.min_width = parse_length_or_keyword(input, "auto")?;
            }
            "min-height" => {
                self.style.min_height = parse_length_or_keyword(input, "auto")?;
            }
            "max-width" => {
                self.style.max_width = parse_length_or_keyword(input, "none")?;
            }
            "max-height" => {
                self.style.max_height = parse_length_or_keyword(input, "none")?;
            }
            "letter-spacing" => {
                // CSS allows `normal` to mean "no extra spacing".
                self.style.letter_spacing = parse_signed_length_or_keyword(input, "normal")?;
            }
            "word-spacing" => {
                self.style.word_spacing = parse_signed_length_or_keyword(input, "normal")?;
            }
            "box-sizing" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let bs = match ident.to_ascii_lowercase().as_str() {
                    "content-box" => BoxSizing::ContentBox,
                    "border-box" => BoxSizing::BorderBox,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.box_sizing = Some(bs);
            }
            "text-indent" => {
                self.style.text_indent = Some(parse_css_length(input)?);
            }
            "text-transform" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let tt = match ident.to_ascii_lowercase().as_str() {
                    "none" => TextTransform::None,
                    "uppercase" => TextTransform::Uppercase,
                    "lowercase" => TextTransform::Lowercase,
                    "capitalize" => TextTransform::Capitalize,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.text_transform = Some(tt);
            }
            "list-style-type" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let lst = match ident.to_ascii_lowercase().as_str() {
                    "none" => ListStyleType::None,
                    "disc" => ListStyleType::Disc,
                    "circle" => ListStyleType::Circle,
                    "square" => ListStyleType::Square,
                    "decimal" => ListStyleType::Decimal,
                    "lower-alpha" | "lower-latin" => ListStyleType::LowerAlpha,
                    "upper-alpha" | "upper-latin" => ListStyleType::UpperAlpha,
                    "lower-roman" => ListStyleType::LowerRoman,
                    "upper-roman" => ListStyleType::UpperRoman,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.list_style_type = Some(lst);
            }
            "list-style-position" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let pos = match ident.to_ascii_lowercase().as_str() {
                    "outside" => ListStylePosition::Outside,
                    "inside" => ListStylePosition::Inside,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.list_style_position = Some(pos);
            }
            "list-style" => {
                // Shorthand: only capture the list-style-type component.
                // Attempt to parse an ident; if it matches a known type, use it.
                if let Ok(lst) = input.try_parse(|i: &mut Parser<'i, '_>| {
                    let ident = i.expect_ident()?.clone();
                    match ident.to_ascii_lowercase().as_str() {
                        "none" => Ok(ListStyleType::None),
                        "disc" => Ok::<_, ParseError<'i, ()>>(ListStyleType::Disc),
                        "circle" => Ok(ListStyleType::Circle),
                        "square" => Ok(ListStyleType::Square),
                        "decimal" => Ok(ListStyleType::Decimal),
                        "lower-alpha" | "lower-latin" => Ok(ListStyleType::LowerAlpha),
                        "upper-alpha" | "upper-latin" => Ok(ListStyleType::UpperAlpha),
                        "lower-roman" => Ok(ListStyleType::LowerRoman),
                        "upper-roman" => Ok(ListStyleType::UpperRoman),
                        _ => Err(i.new_custom_error(())),
                    }
                }) {
                    self.style.list_style_type = Some(lst);
                }
                // Consume and ignore remaining tokens (position, image)
                while input.next().is_ok() {}
            }
            "display" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                match ident.to_ascii_lowercase().as_str() {
                    "none" => self.style.display = Some(DisplayValue::None),
                    "block" => self.style.display = Some(DisplayValue::Block),
                    "inline" => self.style.display = Some(DisplayValue::Inline),
                    _ => return Err(location.new_custom_error(())),
                }
            }
            _ => return Err(input.new_custom_error(())),
        }
        Ok(())
    }
}

impl AtRuleParser<'_> for StyleDeclarationParser<'_> {
    type Prelude = ();
    type AtRule = ();
    type Error = ();
}

impl QualifiedRuleParser<'_> for StyleDeclarationParser<'_> {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = ();
}

impl RuleBodyItemParser<'_, (), ()> for StyleDeclarationParser<'_> {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        false
    }
}

// ── Public API ─────────────────────────────────────────────

/// Parse declarations from a `cssparser::Parser` into a `ComputedStyle`.
fn parse_declarations(input: &mut Parser<'_, '_>) -> ComputedStyle {
    let mut style = ComputedStyle::default();
    let mut decl_parser = StyleDeclarationParser { style: &mut style };
    let rule_body = RuleBodyParser::new(input, &mut decl_parser);
    for result in rule_body {
        let _ = result;
    }
    style
}

/// Parse a CSS inline style attribute value into a `ComputedStyle`.
/// Invalid or unsupported declarations are silently ignored (per CSS spec).
pub fn parse_inline_style(style_str: &str) -> ComputedStyle {
    let mut parser_input = ParserInput::new(style_str);
    let mut parser = Parser::new(&mut parser_input);
    parse_declarations(&mut parser)
}

// ── Selector types ────────────────────────────────────────

/// A single simple selector component.
#[derive(Clone, Debug, PartialEq)]
pub enum SimpleSelector {
    /// Matches element by tag name (e.g., `p`, `h1`).
    Type(String),
    /// Matches element by class (e.g., `.highlight`).
    Class(String),
    /// Matches element by id (e.g., `#header`).
    Id(String),
    /// Matches any element (`*`).
    Universal,
    /// Matches element by attribute (e.g., `[name]`, `[name="value"]`).
    Attribute {
        name: String,
        op: AttrOp,
        value: Option<String>,
    },
}

/// Attribute selector operator.
#[derive(Clone, Debug, PartialEq)]
pub enum AttrOp {
    /// `[name]` — attribute exists.
    Exists,
    /// `[name="value"]` — exact equality.
    Equals,
    /// `[name~="value"]` — whitespace-separated list contains value.
    Includes,
    /// `[name|="value"]` — equals value or starts with `value-`.
    DashMatch,
    /// `[name^="value"]` — starts with value.
    Prefix,
    /// `[name$="value"]` — ends with value.
    Suffix,
    /// `[name*="value"]` — contains value.
    Substring,
}

/// How two compound selectors are combined.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Combinator {
    /// Descendant combinator (whitespace): `div p`.
    Descendant,
    /// Child combinator: `div > p`.
    Child,
}

/// A compound selector is a sequence of simple selectors that all match one element.
/// For example, `p.note#main` = [Type("p"), Class("note"), Id("main")].
#[derive(Clone, Debug)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

impl CompoundSelector {
    fn matches(
        &self,
        tag: &str,
        classes: &[&str],
        id: Option<&str>,
        attributes: &[(&str, &str)],
    ) -> bool {
        self.parts.iter().all(|part| match part {
            SimpleSelector::Type(t) => t.eq_ignore_ascii_case(tag),
            SimpleSelector::Class(c) => classes.iter().any(|cl| cl.eq_ignore_ascii_case(c)),
            SimpleSelector::Id(i) => id.is_some_and(|elem_id| elem_id.eq_ignore_ascii_case(i)),
            SimpleSelector::Universal => true,
            SimpleSelector::Attribute { name, op, value } => {
                let attr_value = attributes
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, v)| *v);
                match (op, value) {
                    (AttrOp::Exists, _) => attr_value.is_some(),
                    (_, None) => attr_value.is_some(),
                    (AttrOp::Equals, Some(v)) => attr_value == Some(v.as_str()),
                    (AttrOp::Includes, Some(v)) => {
                        if v.is_empty() || v.contains(char::is_whitespace) {
                            return false;
                        }
                        attr_value
                            .map(|av| av.split_whitespace().any(|tok| tok == v))
                            .unwrap_or(false)
                    }
                    (AttrOp::DashMatch, Some(v)) => attr_value
                        .map(|av| av == v || av.starts_with(&format!("{}-", v)))
                        .unwrap_or(false),
                    (AttrOp::Prefix, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.map(|av| av.starts_with(v.as_str())).unwrap_or(false)
                    }
                    (AttrOp::Suffix, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.map(|av| av.ends_with(v.as_str())).unwrap_or(false)
                    }
                    (AttrOp::Substring, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.map(|av| av.contains(v.as_str())).unwrap_or(false)
                    }
                }
            }
        })
    }
}

/// A full selector chain: compound selectors linked by combinators.
/// Stored right-to-left: the subject (rightmost) is first.
#[derive(Clone, Debug)]
pub struct SelectorChain {
    /// Compound selectors from right to left.
    pub compounds: Vec<CompoundSelector>,
    /// Combinators between compounds (`compounds.len() - 1`).
    pub combinators: Vec<Combinator>,
    /// Specificity as (`id_count`, `class_count`, `type_count`).
    pub specificity: (u16, u16, u16),
}

/// A CSS rule: one or more selectors with a declaration block.
#[derive(Clone, Debug)]
pub struct CssRule {
    pub selectors: Vec<SelectorChain>,
    pub style: ComputedStyle,
}

/// A parsed stylesheet: a list of CSS rules.
#[derive(Clone, Debug, Default)]
pub struct Stylesheet {
    pub rules: Vec<CssRule>,
    pub page_style: PageStyle,
}

// ── Selector parsing ──────────────────────────────────────

/// Parse a selector list from CSS text. Returns selector chains.
/// Supports: type, class, id, universal, descendant, child, and comma-separated lists.
///
/// Since cssparser auto-skips whitespace (which is the CSS descendant combinator),
/// we parse selectors from the raw text using a simple state machine.
fn parse_selector_list(selector_text: &str) -> Vec<SelectorChain> {
    // Split on commas first (selector list)
    let mut result = Vec::new();
    for part in selector_text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(chain) = parse_one_selector_from_text(part) {
            result.push(chain);
        }
    }
    result
}

/// Parse a single selector (no commas) from raw text.
/// Splits on whitespace and `>` to find compound selectors and combinators.
fn parse_one_selector_from_text(text: &str) -> Option<SelectorChain> {
    // Tokenize by splitting on whitespace, but keep `>` as a separator
    let mut compounds: Vec<CompoundSelector> = Vec::new();
    let mut combinators: Vec<Combinator> = Vec::new();
    let mut id_count: u16 = 0;
    let mut class_count: u16 = 0;
    let mut type_count: u16 = 0;
    let mut pending_child = false;

    for token in text.split_whitespace() {
        if token == ">" {
            pending_child = true;
            continue;
        }

        // Handle `>` attached to other tokens (e.g., "div>p")
        let sub_parts: Vec<&str> = if token.contains('>') {
            token.split('>').collect()
        } else {
            vec![token]
        };

        for (i, sub) in sub_parts.iter().enumerate() {
            let sub = sub.trim();
            if sub.is_empty() {
                if i > 0 {
                    pending_child = true;
                }
                continue;
            }

            let compound = parse_compound_from_text(sub, &mut id_count, &mut class_count, &mut type_count);
            if compound.parts.is_empty() {
                continue;
            }

            if !compounds.is_empty() {
                if pending_child || (i > 0) {
                    combinators.push(Combinator::Child);
                } else {
                    combinators.push(Combinator::Descendant);
                }
            }
            pending_child = false;
            compounds.push(compound);
        }
    }

    if compounds.is_empty() {
        return None;
    }

    // Reverse so the subject (rightmost) is first
    compounds.reverse();
    combinators.reverse();

    Some(SelectorChain {
        compounds,
        combinators,
        specificity: (id_count, class_count, type_count),
    })
}

/// Parse a compound selector from a text token like `p.note#main[data-x="y"]`.
fn parse_compound_from_text(
    text: &str,
    id_count: &mut u16,
    class_count: &mut u16,
    type_count: &mut u16,
) -> CompoundSelector {
    let mut parts: Vec<SimpleSelector> = Vec::new();
    let mut chars = text.char_indices().peekable();

    // Stop characters for ident-like runs.
    fn is_boundary(c: char) -> bool {
        c == '.' || c == '#' || c == '['
    }

    while let Some(&(pos, ch)) = chars.peek() {
        match ch {
            '#' => {
                chars.next();
                let start = pos + 1;
                while chars.peek().is_some_and(|&(_, c)| !is_boundary(c)) {
                    chars.next();
                }
                let end = chars.peek().map_or(text.len(), |&(i, _)| i);
                let id = &text[start..end];
                if !id.is_empty() {
                    parts.push(SimpleSelector::Id(id.to_string()));
                    *id_count += 1;
                }
            }
            '.' => {
                chars.next();
                let start = pos + 1;
                while chars.peek().is_some_and(|&(_, c)| !is_boundary(c)) {
                    chars.next();
                }
                let end = chars.peek().map_or(text.len(), |&(i, _)| i);
                let class = &text[start..end];
                if !class.is_empty() {
                    parts.push(SimpleSelector::Class(class.to_string()));
                    *class_count += 1;
                }
            }
            '*' => {
                chars.next();
                parts.push(SimpleSelector::Universal);
            }
            '[' => {
                chars.next(); // consume `[`
                // Find the matching `]`, respecting quoted strings.
                let content_start = chars.peek().map_or(text.len(), |&(i, _)| i);
                let mut content_end = text.len();
                let mut in_quote: Option<char> = None;
                let mut escaped = false;
                while let Some(&(i, c)) = chars.peek() {
                    if escaped {
                        escaped = false;
                        chars.next();
                        continue;
                    }
                    if c == '\\' {
                        escaped = true;
                        chars.next();
                        continue;
                    }
                    if let Some(q) = in_quote {
                        if c == q {
                            in_quote = None;
                        }
                        chars.next();
                        continue;
                    }
                    if c == '"' || c == '\'' {
                        in_quote = Some(c);
                        chars.next();
                        continue;
                    }
                    if c == ']' {
                        content_end = i;
                        chars.next(); // consume `]`
                        break;
                    }
                    chars.next();
                }
                let inner = &text[content_start..content_end];
                if let Some(attr_sel) = parse_attribute_selector(inner) {
                    parts.push(attr_sel);
                    // Attribute selectors count as class-level specificity.
                    *class_count += 1;
                }
            }
            _ => {
                let start = pos;
                while chars.peek().is_some_and(|&(_, c)| !is_boundary(c)) {
                    chars.next();
                }
                let end = chars.peek().map_or(text.len(), |&(i, _)| i);
                let tag = &text[start..end];
                if !tag.is_empty() {
                    parts.push(SimpleSelector::Type(tag.to_string()));
                    *type_count += 1;
                }
            }
        }
    }

    CompoundSelector { parts }
}

/// Parse the contents inside `[` ... `]` into an attribute selector.
fn parse_attribute_selector(inner: &str) -> Option<SimpleSelector> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }

    fn is_name_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }

    // Scan for end of attribute name.
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() && is_name_char(bytes[i] as char) {
        i += 1;
    }
    let name = inner[..i].trim();
    if name.is_empty() {
        return None;
    }

    let rest = inner[i..].trim_start();
    if rest.is_empty() {
        return Some(SimpleSelector::Attribute {
            name: name.to_string(),
            op: AttrOp::Exists,
            value: None,
        });
    }

    let (op, value_str) = if let Some(stripped) = rest.strip_prefix("~=") {
        (AttrOp::Includes, stripped)
    } else if let Some(stripped) = rest.strip_prefix("|=") {
        (AttrOp::DashMatch, stripped)
    } else if let Some(stripped) = rest.strip_prefix("^=") {
        (AttrOp::Prefix, stripped)
    } else if let Some(stripped) = rest.strip_prefix("$=") {
        (AttrOp::Suffix, stripped)
    } else if let Some(stripped) = rest.strip_prefix("*=") {
        (AttrOp::Substring, stripped)
    } else if let Some(stripped) = rest.strip_prefix('=') {
        (AttrOp::Equals, stripped)
    } else {
        // Unrecognized — treat as Exists.
        return Some(SimpleSelector::Attribute {
            name: name.to_string(),
            op: AttrOp::Exists,
            value: None,
        });
    };

    let value_str = value_str.trim();
    // Strip surrounding quotes if present.
    let value = if (value_str.starts_with('"') && value_str.ends_with('"') && value_str.len() >= 2)
        || (value_str.starts_with('\'') && value_str.ends_with('\'') && value_str.len() >= 2)
    {
        value_str[1..value_str.len() - 1].to_string()
    } else {
        value_str.to_string()
    };

    Some(SimpleSelector::Attribute {
        name: name.to_string(),
        op,
        value: Some(value),
    })
}

// ── @page rule parsing ───────────────────────────────────

fn parse_page_declarations(declarations: &str, page_style: &mut PageStyle) {
    // Split on top-level `;` and parse each declaration in isolation.
    // This is more forgiving than feeding the whole block to cssparser
    // and avoids getting stuck on malformed / unknown declarations
    // (previous versions of this function used a while-loop around
    // `try_parse` that could infinite-loop on trailing whitespace).
    for decl in split_top_level_semicolons(declarations) {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut parser_input = ParserInput::new(decl);
        let mut parser = Parser::new(&mut parser_input);
        let _ = parser.parse_entirely(|input| -> Result<(), ParseError<'_, ()>> {
            let name = input.expect_ident()?.clone();
            input.expect_colon()?;

            match name.to_ascii_lowercase().as_str() {
                "size" => {
                    let ident = input.try_parse(|i| i.expect_ident().cloned());
                    if let Ok(ref id) = ident {
                        match id.to_ascii_lowercase().as_str() {
                            "letter" => {
                                page_style.width = Some(612.0);
                                page_style.height = Some(792.0);
                            }
                            "a4" => {
                                page_style.width = Some(595.0);
                                page_style.height = Some(842.0);
                            }
                            _ => return Err(input.new_custom_error(())),
                        }
                    } else {
                        let w = parse_non_negative_length(input)?;
                        let h = input.try_parse(parse_non_negative_length).unwrap_or(w);
                        page_style.width = Some(w.resolve(12.0));
                        page_style.height = Some(h.resolve(12.0));
                    }
                }
                "margin" => {
                    let (top, right, bottom, left) = parse_non_negative_box_shorthand(input)?;
                    let em = 12.0;
                    page_style.margin_top = Some(top.resolve(em));
                    page_style.margin_right = Some(right.resolve(em));
                    page_style.margin_bottom = Some(bottom.resolve(em));
                    page_style.margin_left = Some(left.resolve(em));
                }
                "margin-top" => {
                    page_style.margin_top = Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                "margin-right" => {
                    page_style.margin_right =
                        Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                "margin-bottom" => {
                    page_style.margin_bottom =
                        Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                "margin-left" => {
                    page_style.margin_left =
                        Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                _ => return Err(input.new_custom_error(())),
            }
            // Drain any stray trailing tokens (e.g. `!important`) so
            // parse_entirely's exhaustion check succeeds.
            while !input.is_exhausted() {
                let _ = input.next();
            }
            Ok(())
        });
    }
}

// ── Stylesheet parsing ────────────────────────────────────

/// Parse a CSS stylesheet (from `<style>` blocks) into rules.
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    let mut rules = Vec::new();
    let mut page_style = PageStyle::default();
    parse_stylesheet_manual(css, &mut rules, &mut page_style);
    Stylesheet { rules, page_style }
}

/// Find the byte index of the `}` that matches the `{` at position
/// `after_open_start` in `src`, honoring nesting. Returns the index into
/// `src` of the closing brace. If unbalanced, returns `None`.
fn find_matching_close(src: &str) -> Option<usize> {
    // `src` starts immediately after the opening `{`. We walk forward,
    // counting nested `{` / `}` pairs. Strings inside CSS declarations
    // ("..." / '...') are skipped so that braces inside a string literal
    // don't confuse the counter.
    let bytes = src.as_bytes();
    let mut depth: usize = 0;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' => {
                // Skip a CSS string literal. Honors `\` escapes.
                let quote = b;
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if c == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Parse stylesheet by manually scanning for `{ }` blocks.
fn parse_stylesheet_manual(css: &str, rules: &mut Vec<CssRule>, page_style: &mut PageStyle) {
    let mut remaining = css;

    while !remaining.trim().is_empty() {
        // Find the next `{`
        let Some(brace_open) = remaining.find('{') else {
            break;
        };
        let selector_text = remaining[..brace_open].trim();
        let after_open = &remaining[brace_open + 1..];

        // Handle @page rule — may contain nested `@top-center { ... }`
        // blocks, so we need to find the matching close brace honoring
        // nesting.
        if selector_text.starts_with("@page") {
            let Some(brace_close) = find_matching_close(after_open) else {
                break;
            };
            let body = &after_open[..brace_close];
            remaining = &after_open[brace_close + 1..];
            parse_page_body(body, page_style);
            continue;
        }

        // Non-@page rules: simple scan for the next `}` (we don't yet
        // support nesting in regular rules).
        let Some(brace_close) = after_open.find('}') else {
            break;
        };
        let declarations_text = &after_open[..brace_close];
        remaining = &after_open[brace_close + 1..];

        if selector_text.is_empty() {
            continue;
        }

        let selectors = parse_selector_list(selector_text);
        if selectors.is_empty() {
            continue;
        }

        let style = parse_inline_style(declarations_text);
        rules.push(CssRule { selectors, style });
    }
}

/// Parse the body of an `@page` rule: a mix of plain declarations and
/// nested margin-box at-rules (`@top-center { ... }` etc.). Declarations
/// outside any nested block are routed to `parse_page_declarations`;
/// nested blocks are routed to `parse_margin_box_declarations`.
fn parse_page_body(body: &str, page_style: &mut PageStyle) {
    // We walk `body` character-by-character, accumulating the "flat"
    // declaration text (everything outside nested `@xxx { }` blocks) into
    // `flat`. When we hit an `@name { ... }`, we dispatch it to the
    // margin-box parser and then continue scanning after the close brace.
    let mut flat = String::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'@' {
            // Read at-rule name up to whitespace or `{`.
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                let c = bytes[j];
                if c.is_ascii_whitespace() || c == b'{' {
                    break;
                }
                j += 1;
            }
            let name = &body[start..j];
            // Skip whitespace up to `{`.
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'{' {
                let after = &body[j + 1..];
                if let Some(close_rel) = find_matching_close(after) {
                    let inner = &after[..close_rel];
                    if let Some(pos) = margin_box_position_for(&name.to_ascii_lowercase()) {
                        let mb = parse_margin_box_declarations(inner);
                        page_style.margin_boxes.set(pos, mb);
                    }
                    // advance `i` past the closing brace
                    i = j + 1 + close_rel + 1;
                    continue;
                }
                // Unbalanced: bail on this at-rule.
                break;
            }
            // No `{` found; treat as a stray `@` and fall through.
            flat.push('@');
            i += 1;
            continue;
        }
        flat.push(b as char);
        i += 1;
    }
    parse_page_declarations(&flat, page_style);
}

/// Parse the declaration list of a `@page` margin box (e.g. the body of
/// `@top-center { ... }`). Supported properties: `content`, `font-size`,
/// `font-family`, `color`, `text-align`. Unknown declarations are
/// silently ignored.
///
/// Implementation note: we split the body on `;` ourselves (honoring
/// string literals) instead of relying on cssparser's declaration list
/// iterator. This keeps every declaration self-contained and avoids
/// any risk of a malformed value leaving the parser in a state where
/// it can't make forward progress.
fn parse_margin_box_declarations(declarations: &str) -> MarginBox {
    let mut mb = MarginBox::default();
    for decl in split_top_level_semicolons(declarations) {
        let decl = decl.trim();
        if decl.is_empty() {
            continue;
        }
        let mut input = ParserInput::new(decl);
        let mut parser = Parser::new(&mut input);
        let _ = parser.parse_entirely(|input| -> Result<(), ParseError<'_, ()>> {
            let name = input.expect_ident()?.clone();
            input.expect_colon()?;

            match name.to_ascii_lowercase().as_str() {
                "content" => {
                    mb.content = parse_margin_box_content(input)?;
                }
                "font-size" => {
                    let len = parse_non_negative_length(input)?;
                    mb.font_size = Some(len.resolve(10.0));
                }
                "font-family" => {
                    // Accept a single ident or string as the family name;
                    // only the first value is honored. Further comma-
                    // separated fallbacks are consumed but ignored.
                    let first = if let Ok(s) =
                        input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_string()))
                    {
                        s
                    } else {
                        input.expect_ident()?.as_ref().to_string()
                    };
                    mb.font_family = Some(first);
                    while input.try_parse(|i| i.expect_comma()).is_ok() {
                        let _ = input
                            .try_parse(|i| i.expect_string().map(|_| ()))
                            .or_else(|_| input.try_parse(|i| i.expect_ident().map(|_| ())));
                    }
                }
                "color" => {
                    if let Ok(c) = parse_css_color(input) {
                        mb.color = Some(c);
                    }
                }
                "text-align" => {
                    let id = input.expect_ident()?.clone();
                    mb.text_align = Some(match id.to_ascii_lowercase().as_str() {
                        "left" => TextAlign::Left,
                        "center" => TextAlign::Center,
                        "right" => TextAlign::Right,
                        "justify" => TextAlign::Justify,
                        _ => return Err(input.new_custom_error(())),
                    });
                }
                _ => return Err(input.new_custom_error(())),
            }
            // Drain any leftover tokens inside this declaration — e.g.
            // trailing `!important`, which we parse but ignore.
            while !input.is_exhausted() {
                let _ = input.next();
            }
            Ok(())
        });
    }
    mb
}

/// Split a CSS declaration-list string on top-level `;` characters.
/// Honors string literals (`"..."` / `'...'`) and parenthesized groups
/// so that semicolons inside them are not treated as delimiters.
fn split_top_level_semicolons(src: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut start = 0;
    let mut i = 0;
    let mut paren_depth = 0i32;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'"' | b'\'' => {
                let quote = b;
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    if c == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if c == quote {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            b'(' => {
                paren_depth += 1;
                i += 1;
            }
            b')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                i += 1;
            }
            b';' if paren_depth == 0 => {
                out.push(&src[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        out.push(&src[start..]);
    }
    out
}

/// Parse a `content:` value for a margin box: a sequence of string
/// literals and `counter(page)` / `counter(pages)` function calls,
/// separated by whitespace (the CSS spec uses whitespace juxtaposition).
///
/// `input` is the regular (whitespace-skipping) declaration-body parser;
/// this function uses only the high-level `try_parse` entry points so
/// that parser state is always recoverable and we can't accidentally
/// infinite-loop on stray whitespace.
fn parse_margin_box_content<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<Vec<ContentItem>, ParseError<'i, ()>> {
    let mut items = Vec::new();
    loop {
        if input.is_exhausted() {
            break;
        }
        // Try a string literal.
        if let Ok(s) =
            input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_string()))
        {
            items.push(ContentItem::String(s));
            continue;
        }
        // Try `counter(ident)`.
        let counter_result = input.try_parse(|i| -> Result<ContentItem, ParseError<'_, ()>> {
            let name = i.expect_function()?.clone();
            if !name.eq_ignore_ascii_case("counter") {
                return Err(i.new_custom_error(()));
            }
            i.parse_nested_block(|nested| -> Result<ContentItem, ParseError<'_, ()>> {
                let id = nested.expect_ident()?.clone();
                match id.to_ascii_lowercase().as_str() {
                    "page" => Ok(ContentItem::CounterPage),
                    "pages" => Ok(ContentItem::CounterPages),
                    _ => Err(nested.new_custom_error(())),
                }
            })
        });
        if let Ok(item) = counter_result {
            items.push(item);
            continue;
        }
        // Anything else terminates the content value. We intentionally
        // don't advance `input` here — the outer declaration loop will
        // see the remaining tokens (typically a semicolon) and recover.
        break;
    }
    Ok(items)
}


// ── Selector matching ─────────────────────────────────────

/// Information about an element needed for selector matching.
pub struct ElementInfo<'a> {
    pub tag: &'a str,
    pub classes: Vec<&'a str>,
    pub id: Option<&'a str>,
    /// All attributes on the element (name, value).
    pub attributes: Vec<(&'a str, &'a str)>,
    /// Ancestor chain from parent to root.
    /// Each entry: (tag, classes, id, attributes).
    pub ancestors: Vec<(
        &'a str,
        Vec<&'a str>,
        Option<&'a str>,
        Vec<(&'a str, &'a str)>,
    )>,
}

/// Match all rules in a stylesheet against an element, returning the
/// merged `ComputedStyle` from all matching rules (respecting specificity).
pub fn match_rules(element: &ElementInfo<'_>, stylesheet: &Stylesheet) -> ComputedStyle {
    let mut matches: Vec<(u16, u16, u16, usize, &ComputedStyle)> = Vec::new();

    for (rule_idx, rule) in stylesheet.rules.iter().enumerate() {
        for selector in &rule.selectors {
            if selector_matches(selector, element) {
                matches.push((
                    selector.specificity.0,
                    selector.specificity.1,
                    selector.specificity.2,
                    rule_idx,
                    &rule.style,
                ));
                break; // One match per rule is enough
            }
        }
    }

    // Sort by specificity (id, class, type), then by source order
    matches.sort_by_key(|&(id, cls, typ, idx, _)| (id, cls, typ, idx));

    let mut result = ComputedStyle::default();
    for (_, _, _, _, style) in &matches {
        merge_style(&mut result, style);
    }
    result
}

fn selector_matches(selector: &SelectorChain, element: &ElementInfo<'_>) -> bool {
    // The subject (first compound after reversal) must match the element
    let subject = &selector.compounds[0];
    if !subject.matches(element.tag, &element.classes, element.id, &element.attributes) {
        return false;
    }

    // Match ancestor chain
    if selector.compounds.len() == 1 {
        return true;
    }

    // Walk the combinator chain
    let mut ancestor_idx = 0;
    for i in 1..selector.compounds.len() {
        let compound = &selector.compounds[i];
        let combinator = selector.combinators[i - 1];

        match combinator {
            Combinator::Child => {
                // Must match the immediate parent
                if ancestor_idx >= element.ancestors.len() {
                    return false;
                }
                let (tag, ref classes, id, ref attrs) = element.ancestors[ancestor_idx];
                if !compound.matches(tag, classes, id, attrs) {
                    return false;
                }
                ancestor_idx += 1;
            }
            Combinator::Descendant => {
                // Must match some ancestor
                let mut found = false;
                while ancestor_idx < element.ancestors.len() {
                    let (tag, ref classes, id, ref attrs) = element.ancestors[ancestor_idx];
                    ancestor_idx += 1;
                    if compound.matches(tag, classes, id, attrs) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
            }
        }
    }
    true
}


// ── Tests ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_named_red() {
        let s = parse_inline_style("color: red");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn color_named_blue() {
        let s = parse_inline_style("color: blue");
        assert_eq!(s.color, Some((0.0, 0.0, 1.0)));
    }

    #[test]
    fn color_named_black() {
        let s = parse_inline_style("color: black");
        assert_eq!(s.color, Some((0.0, 0.0, 0.0)));
    }

    #[test]
    fn color_named_white() {
        let s = parse_inline_style("color: white");
        assert_eq!(s.color, Some((1.0, 1.0, 1.0)));
    }

    #[test]
    fn color_hex_6() {
        let s = parse_inline_style("color: #ff0000");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn color_hex_3() {
        let s = parse_inline_style("color: #00f");
        assert_eq!(s.color, Some((0.0, 0.0, 1.0)));
    }

    #[test]
    fn color_rgb_function() {
        let s = parse_inline_style("color: rgb(255, 128, 0)");
        let (r, g, b) = s.color.unwrap();
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 128.0 / 255.0).abs() < 0.01);
        assert!(b.abs() < 0.01);
    }

    #[test]
    fn background_color() {
        let s = parse_inline_style("background-color: yellow");
        assert_eq!(s.background_color, Some((1.0, 1.0, 0.0)));
    }

    #[test]
    fn font_size_px() {
        let s = parse_inline_style("font-size: 16px");
        assert!(matches!(s.font_size, Some(CssLength::Px(v)) if (v - 16.0).abs() < 0.001));
    }

    #[test]
    fn font_size_pt() {
        let s = parse_inline_style("font-size: 18pt");
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn font_size_em() {
        let s = parse_inline_style("font-size: 1.5em");
        assert!(matches!(s.font_size, Some(CssLength::Em(v)) if (v - 1.5).abs() < 0.001));
    }

    #[test]
    fn font_weight_bold() {
        let s = parse_inline_style("font-weight: bold");
        assert_eq!(s.font_weight, Some(FontWeight::Bold));
    }

    #[test]
    fn font_weight_normal() {
        let s = parse_inline_style("font-weight: normal");
        assert_eq!(s.font_weight, Some(FontWeight::Normal));
    }

    #[test]
    fn font_weight_numeric_700() {
        let s = parse_inline_style("font-weight: 700");
        assert_eq!(s.font_weight, Some(FontWeight::Numeric(700)));
        assert!(s.font_weight.unwrap().is_bold());
    }

    #[test]
    fn font_weight_numeric_400() {
        let s = parse_inline_style("font-weight: 400");
        assert_eq!(s.font_weight, Some(FontWeight::Numeric(400)));
        assert!(!s.font_weight.unwrap().is_bold());
    }

    #[test]
    fn font_style_italic() {
        let s = parse_inline_style("font-style: italic");
        assert_eq!(s.font_style, Some(FontStyle::Italic));
    }

    #[test]
    fn font_style_normal() {
        let s = parse_inline_style("font-style: normal");
        assert_eq!(s.font_style, Some(FontStyle::Normal));
    }

    #[test]
    fn font_family_ident() {
        let s = parse_inline_style("font-family: serif");
        assert_eq!(s.font_family.as_deref(), Some("serif"));
    }

    #[test]
    fn font_family_quoted() {
        let s = parse_inline_style("font-family: \"Times New Roman\"");
        assert_eq!(s.font_family.as_deref(), Some("Times New Roman"));
    }

    #[test]
    fn text_align_center() {
        let s = parse_inline_style("text-align: center");
        assert_eq!(s.text_align, Some(TextAlign::Center));
    }

    #[test]
    fn text_align_right() {
        let s = parse_inline_style("text-align: right");
        assert_eq!(s.text_align, Some(TextAlign::Right));
    }

    #[test]
    fn line_height_pt() {
        let s = parse_inline_style("line-height: 18pt");
        assert!(matches!(s.line_height, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn line_height_bare_number() {
        let s = parse_inline_style("line-height: 1.5");
        assert!(matches!(s.line_height, Some(CssLength::Em(v)) if (v - 1.5).abs() < 0.001));
    }

    #[test]
    fn margin_shorthand_one_value() {
        let s = parse_inline_style("margin: 10px");
        assert!(matches!(s.margin_bottom, Some(CssLength::Px(v)) if (v - 10.0).abs() < 0.001));
    }

    #[test]
    fn margin_bottom_individual() {
        let s = parse_inline_style("margin-bottom: 20pt");
        assert!(matches!(s.margin_bottom, Some(CssLength::Pt(v)) if (v - 20.0).abs() < 0.001));
    }

    #[test]
    fn padding_shorthand_four_values() {
        let s = parse_inline_style("padding: 10px 20px 30px 40px");
        assert!(matches!(s.padding_top, Some(CssLength::Px(v)) if (v - 10.0).abs() < 0.001));
        assert!(matches!(s.padding_right, Some(CssLength::Px(v)) if (v - 20.0).abs() < 0.001));
        assert!(matches!(s.padding_bottom, Some(CssLength::Px(v)) if (v - 30.0).abs() < 0.001));
        assert!(matches!(s.padding_left, Some(CssLength::Px(v)) if (v - 40.0).abs() < 0.001));
    }

    #[test]
    fn padding_shorthand_two_values() {
        let s = parse_inline_style("padding: 10pt 20pt");
        assert!(matches!(s.padding_top, Some(CssLength::Pt(v)) if (v - 10.0).abs() < 0.001));
        assert!(matches!(s.padding_right, Some(CssLength::Pt(v)) if (v - 20.0).abs() < 0.001));
        assert!(matches!(s.padding_bottom, Some(CssLength::Pt(v)) if (v - 10.0).abs() < 0.001));
        assert!(matches!(s.padding_left, Some(CssLength::Pt(v)) if (v - 20.0).abs() < 0.001));
    }

    #[test]
    fn border_width() {
        let s = parse_inline_style("border-width: 2px");
        assert!(matches!(s.border_width, Some(CssLength::Px(v)) if (v - 2.0).abs() < 0.001));
    }

    #[test]
    fn border_color() {
        let s = parse_inline_style("border-color: red");
        assert_eq!(s.border_color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn border_shorthand() {
        let s = parse_inline_style("border: 1px solid black");
        assert!(matches!(s.border_width, Some(CssLength::Px(v)) if (v - 1.0).abs() < 0.001));
        assert_eq!(s.border_color, Some((0.0, 0.0, 0.0)));
    }

    #[test]
    fn invalid_property_ignored() {
        let s = parse_inline_style("invalid-prop: value; color: blue");
        assert_eq!(s.color, Some((0.0, 0.0, 1.0)));
    }

    #[test]
    fn invalid_value_ignored() {
        let s = parse_inline_style("color: notacolor; font-size: 18pt");
        assert!(s.color.is_none());
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn empty_string() {
        let s = parse_inline_style("");
        assert!(s.color.is_none());
        assert!(s.font_size.is_none());
    }

    #[test]
    fn multiple_properties() {
        let s = parse_inline_style("color: red; font-size: 24pt; font-weight: bold");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 24.0).abs() < 0.001));
        assert_eq!(s.font_weight, Some(FontWeight::Bold));
    }

    #[test]
    fn width_height_length_parsed() {
        let s = parse_inline_style("width: 300pt; height: 200px");
        assert!(matches!(s.width, Some(CssLength::Pt(v)) if (v - 300.0).abs() < 0.001));
        assert!(matches!(s.height, Some(CssLength::Px(v)) if (v - 200.0).abs() < 0.001));
    }

    #[test]
    fn width_auto_leaves_field_none() {
        let s = parse_inline_style("width: auto");
        assert!(s.width.is_none());
    }

    #[test]
    fn max_width_none_leaves_field_none() {
        let s = parse_inline_style("max-width: none");
        assert!(s.max_width.is_none());
    }

    #[test]
    fn rem_vw_vh_units_parse() {
        let s = parse_inline_style("width: 10rem; height: 50vh; max-width: 80vw");
        assert!(matches!(s.width, Some(CssLength::Rem(v)) if (v - 10.0).abs() < 0.001));
        assert!(matches!(s.height, Some(CssLength::Vh(v)) if (v - 50.0).abs() < 0.001));
        assert!(matches!(s.max_width, Some(CssLength::Vw(v)) if (v - 80.0).abs() < 0.001));
    }

    #[test]
    fn rem_resolves_against_root_font_size() {
        let ctx = LengthContext {
            em: 24.0, // inside a header
            rem: 12.0,
            vw: 612.0,
            vh: 792.0,
            container: 500.0,
        };
        assert!((CssLength::Rem(2.0).resolve_ctx(&ctx) - 24.0).abs() < 0.001);
        assert!((CssLength::Em(2.0).resolve_ctx(&ctx) - 48.0).abs() < 0.001);
    }

    #[test]
    fn vw_vh_resolve_against_viewport() {
        let ctx = LengthContext {
            em: 12.0,
            rem: 12.0,
            vw: 600.0,
            vh: 800.0,
            container: 500.0,
        };
        assert!((CssLength::Vw(50.0).resolve_ctx(&ctx) - 300.0).abs() < 0.001);
        assert!((CssLength::Vh(25.0).resolve_ctx(&ctx) - 200.0).abs() < 0.001);
    }

    #[test]
    fn percentage_parses_and_resolves() {
        let s = parse_inline_style("width: 50%; margin-left: 10%");
        assert!(matches!(s.width, Some(CssLength::Pct(v)) if (v - 50.0).abs() < 0.001));
        assert!(matches!(s.margin_left, Some(CssLength::Pct(v)) if (v - 10.0).abs() < 0.001));
        let ctx = LengthContext {
            em: 12.0,
            rem: 12.0,
            vw: 612.0,
            vh: 792.0,
            container: 400.0,
        };
        assert!((CssLength::Pct(50.0).resolve_ctx(&ctx) - 200.0).abs() < 0.001);
    }

    #[test]
    fn box_sizing_parse() {
        let s = parse_inline_style("box-sizing: border-box");
        assert_eq!(s.box_sizing, Some(BoxSizing::BorderBox));
        let s = parse_inline_style("box-sizing: content-box");
        assert_eq!(s.box_sizing, Some(BoxSizing::ContentBox));
    }

    #[test]
    fn letter_and_word_spacing_parse() {
        let s = parse_inline_style("letter-spacing: 2pt; word-spacing: 0.5em");
        assert!(matches!(s.letter_spacing, Some(CssLength::Pt(v)) if (v - 2.0).abs() < 0.001));
        assert!(matches!(s.word_spacing, Some(CssLength::Em(v)) if (v - 0.5).abs() < 0.001));
    }

    #[test]
    fn letter_spacing_normal_leaves_field_none() {
        let s = parse_inline_style("letter-spacing: normal");
        assert!(s.letter_spacing.is_none());
    }

    #[test]
    fn letter_spacing_accepts_negative() {
        let s = parse_inline_style("letter-spacing: -1pt");
        assert!(matches!(s.letter_spacing, Some(CssLength::Pt(v)) if (v + 1.0).abs() < 0.001));
    }

    #[test]
    fn min_max_width_height_lengths() {
        let s = parse_inline_style(
            "min-width: 100pt; max-width: 600pt; min-height: 50pt; max-height: 400pt",
        );
        assert!(matches!(s.min_width, Some(CssLength::Pt(v)) if (v - 100.0).abs() < 0.001));
        assert!(matches!(s.max_width, Some(CssLength::Pt(v)) if (v - 600.0).abs() < 0.001));
        assert!(matches!(s.min_height, Some(CssLength::Pt(v)) if (v - 50.0).abs() < 0.001));
        assert!(matches!(s.max_height, Some(CssLength::Pt(v)) if (v - 400.0).abs() < 0.001));
    }

    #[test]
    fn later_declaration_wins() {
        let s = parse_inline_style("color: red; color: blue");
        assert_eq!(s.color, Some((0.0, 0.0, 1.0)));
    }

    #[test]
    fn zero_without_units() {
        let s = parse_inline_style("padding: 0");
        assert!(matches!(s.padding_top, Some(CssLength::Px(v)) if v.abs() < 0.001));
    }

    #[test]
    fn important_not_breaking() {
        // !important should not prevent the value from being parsed
        let s = parse_inline_style("color: red !important");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn important_with_other_declarations() {
        let s = parse_inline_style("color: red !important; font-size: 18pt");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn negative_font_size_rejected() {
        let s = parse_inline_style("font-size: -12pt");
        assert!(s.font_size.is_none());
    }

    #[test]
    fn negative_padding_rejected() {
        let s = parse_inline_style("padding: -10px");
        assert!(s.padding_top.is_none());
    }

    #[test]
    fn negative_border_width_rejected() {
        let s = parse_inline_style("border-width: -2px");
        assert!(s.border_width.is_none());
    }

    #[test]
    fn negative_margin_allowed() {
        // CSS spec allows negative margins
        let s = parse_inline_style("margin-bottom: -10px");
        assert!(matches!(s.margin_bottom, Some(CssLength::Px(v)) if (v - -10.0).abs() < 0.001));
    }

    #[test]
    fn uppercase_property_name() {
        let s = parse_inline_style("COLOR: red");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn mixed_case_property_name() {
        let s = parse_inline_style("Font-Size: 18pt");
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn uppercase_hex_color() {
        let s = parse_inline_style("color: #FF0000");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn uppercase_named_color() {
        let s = parse_inline_style("color: RED");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn extra_whitespace_around_values() {
        let s = parse_inline_style("  color :  red  ;  font-size :  18pt  ");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn extra_semicolons() {
        let s = parse_inline_style(";;color: red;;;font-size: 18pt;;");
        assert_eq!(s.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }

    #[test]
    fn fractional_length() {
        let s = parse_inline_style("font-size: 12.5pt");
        assert!(matches!(s.font_size, Some(CssLength::Pt(v)) if (v - 12.5).abs() < 0.001));
    }

    #[test]
    fn padding_three_values() {
        let s = parse_inline_style("padding: 10px 20px 30px");
        assert!(matches!(s.padding_top, Some(CssLength::Px(v)) if (v - 10.0).abs() < 0.001));
        assert!(matches!(s.padding_right, Some(CssLength::Px(v)) if (v - 20.0).abs() < 0.001));
        assert!(matches!(s.padding_bottom, Some(CssLength::Px(v)) if (v - 30.0).abs() < 0.001));
        assert!(matches!(s.padding_left, Some(CssLength::Px(v)) if (v - 20.0).abs() < 0.001));
    }

    #[test]
    fn font_weight_500_not_bold() {
        let s = parse_inline_style("font-weight: 500");
        assert_eq!(s.font_weight, Some(FontWeight::Numeric(500)));
        assert!(!s.font_weight.unwrap().is_bold());
    }

    #[test]
    fn font_weight_invalid_150_rejected() {
        let s = parse_inline_style("font-weight: 150");
        assert!(s.font_weight.is_none());
    }

    #[test]
    fn font_weight_1000_rejected() {
        let s = parse_inline_style("font-weight: 1000");
        assert!(s.font_weight.is_none());
    }

    #[test]
    fn line_height_zero() {
        let s = parse_inline_style("line-height: 0");
        // Zero is valid (though unusual)
        assert!(s.line_height.is_some());
    }

    #[test]
    fn border_color_first() {
        let s = parse_inline_style("border: red 2px solid");
        assert_eq!(s.border_color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(s.border_width, Some(CssLength::Px(v)) if (v - 2.0).abs() < 0.001));
    }

    #[test]
    fn length_resolve_px() {
        assert!((CssLength::Px(16.0).resolve(12.0) - 12.0).abs() < 0.001);
    }

    #[test]
    fn length_resolve_pt() {
        assert!((CssLength::Pt(18.0).resolve(12.0) - 18.0).abs() < 0.001);
    }

    #[test]
    fn length_resolve_em() {
        assert!((CssLength::Em(1.5).resolve(12.0) - 18.0).abs() < 0.001);
    }

    // ── Selector parsing tests ────────────────────────────────

    #[test]
    fn selector_type() {
        let selectors = parse_selector_list("p");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].compounds.len(), 1);
        assert_eq!(selectors[0].compounds[0].parts, vec![SimpleSelector::Type("p".into())]);
    }

    #[test]
    fn selector_class() {
        let selectors = parse_selector_list(".highlight");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].compounds[0].parts, vec![SimpleSelector::Class("highlight".into())]);
    }

    #[test]
    fn selector_id() {
        let selectors = parse_selector_list("#header");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].compounds[0].parts, vec![SimpleSelector::Id("header".into())]);
    }

    #[test]
    fn selector_compound() {
        let selectors = parse_selector_list("p.note");
        assert_eq!(selectors.len(), 1);
        let parts = &selectors[0].compounds[0].parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], SimpleSelector::Type("p".into()));
        assert_eq!(parts[1], SimpleSelector::Class("note".into()));
    }

    #[test]
    fn selector_comma_list() {
        let selectors = parse_selector_list("h1, h2");
        assert_eq!(selectors.len(), 2);
    }

    #[test]
    fn selector_descendant() {
        let selectors = parse_selector_list("div p");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].compounds.len(), 2);
        assert_eq!(selectors[0].combinators, vec![Combinator::Descendant]);
    }

    #[test]
    fn selector_child() {
        let selectors = parse_selector_list("div > p");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].compounds.len(), 2);
        assert_eq!(selectors[0].combinators, vec![Combinator::Child]);
    }

    #[test]
    fn selector_specificity_type() {
        let selectors = parse_selector_list("p");
        assert_eq!(selectors[0].specificity, (0, 0, 1));
    }

    #[test]
    fn selector_specificity_class() {
        let selectors = parse_selector_list(".foo");
        assert_eq!(selectors[0].specificity, (0, 1, 0));
    }

    #[test]
    fn selector_specificity_id() {
        let selectors = parse_selector_list("#bar");
        assert_eq!(selectors[0].specificity, (1, 0, 0));
    }

    #[test]
    fn selector_specificity_compound() {
        let selectors = parse_selector_list("p.note#main");
        assert_eq!(selectors[0].specificity, (1, 1, 1));
    }

    // ── Stylesheet parsing tests ──────────────────────────────

    #[test]
    fn stylesheet_single_rule() {
        let sheet = parse_stylesheet("p { color: red }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn stylesheet_multiple_rules() {
        let sheet = parse_stylesheet("p { color: red } h1 { font-size: 24pt }");
        assert_eq!(sheet.rules.len(), 2);
    }

    #[test]
    fn stylesheet_empty() {
        let sheet = parse_stylesheet("");
        assert!(sheet.rules.is_empty());
    }

    // ── Selector matching tests ───────────────────────────────

    #[test]
    fn match_type_selector() {
        let sheet = parse_stylesheet("p { color: red }");
        let elem = ElementInfo { tag: "p", classes: vec![], id: None, attributes: vec![], ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_class_selector() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = ElementInfo { tag: "p", classes: vec!["red"], id: None, attributes: vec![], ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn no_match_wrong_class() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = ElementInfo { tag: "p", classes: vec!["blue"], id: None, attributes: vec![], ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn match_descendant_selector() {
        let sheet = parse_stylesheet("div p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None, attributes: vec![],
            ancestors: vec![("div", vec![], None, vec![])],
        };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_child_selector() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None, attributes: vec![],
            ancestors: vec![("div", vec![], None, vec![])],
        };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn child_no_match_grandchild() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None, attributes: vec![],
            ancestors: vec![("blockquote", vec![], None, vec![]), ("div", vec![], None, vec![])],
        };
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn specificity_class_beats_type() {
        let sheet = parse_stylesheet("p { color: red } .blue { color: blue }");
        let elem = ElementInfo { tag: "p", classes: vec!["blue"], id: None, attributes: vec![], ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((0.0, 0.0, 1.0)));
    }

    // ── Attribute selector tests ──────────────────────────────

    #[test]
    fn match_attr_exists() {
        let sheet = parse_stylesheet("[data-x] { color: red }");
        let elem = ElementInfo {
            tag: "p",
            classes: vec![],
            id: None,
            attributes: vec![("data-x", "")],
            ancestors: vec![],
        };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_equals() {
        let sheet = parse_stylesheet("[data-role=\"primary\"] { color: red }");
        let elem = ElementInfo {
            tag: "p",
            classes: vec![],
            id: None,
            attributes: vec![("data-role", "primary")],
            ancestors: vec![],
        };
        assert_eq!(match_rules(&elem, &sheet).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_includes() {
        let sheet = parse_stylesheet("[class~=\"note\"] { color: red }");
        let elem_match = ElementInfo {
            tag: "p",
            classes: vec!["intro", "note", "main"],
            id: None,
            attributes: vec![("class", "intro note main")],
            ancestors: vec![],
        };
        assert_eq!(match_rules(&elem_match, &sheet).color, Some((1.0, 0.0, 0.0)));

        let elem_no = ElementInfo {
            tag: "p",
            classes: vec!["notepad"],
            id: None,
            attributes: vec![("class", "notepad")],
            ancestors: vec![],
        };
        assert!(match_rules(&elem_no, &sheet).color.is_none());
    }

    #[test]
    fn match_attr_dashmatch() {
        let sheet = parse_stylesheet("[lang|=\"en\"] { color: red }");
        let a = ElementInfo {
            tag: "p",
            classes: vec![],
            id: None,
            attributes: vec![("lang", "en")],
            ancestors: vec![],
        };
        let b = ElementInfo {
            tag: "p",
            classes: vec![],
            id: None,
            attributes: vec![("lang", "en-US")],
            ancestors: vec![],
        };
        let c = ElementInfo {
            tag: "p",
            classes: vec![],
            id: None,
            attributes: vec![("lang", "english")],
            ancestors: vec![],
        };
        assert_eq!(match_rules(&a, &sheet).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&b, &sheet).color, Some((1.0, 0.0, 0.0)));
        assert!(match_rules(&c, &sheet).color.is_none());
    }

    #[test]
    fn match_attr_prefix_suffix_substring() {
        let sheet_prefix = parse_stylesheet("[href^=\"http\"] { color: red }");
        let sheet_suffix = parse_stylesheet("[src$=\".png\"] { color: red }");
        let sheet_substr = parse_stylesheet("[class*=\"big\"] { color: red }");
        let a = ElementInfo {
            tag: "a",
            classes: vec![],
            id: None,
            attributes: vec![("href", "https://example.com")],
            ancestors: vec![],
        };
        let b = ElementInfo {
            tag: "img",
            classes: vec![],
            id: None,
            attributes: vec![("src", "pic.png")],
            ancestors: vec![],
        };
        let c = ElementInfo {
            tag: "div",
            classes: vec!["bigbox"],
            id: None,
            attributes: vec![("class", "bigbox")],
            ancestors: vec![],
        };
        assert_eq!(match_rules(&a, &sheet_prefix).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&b, &sheet_suffix).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&c, &sheet_substr).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_compound_with_type() {
        let sheet = parse_stylesheet("p[class=\"foo\"] { color: red }");
        let elem = ElementInfo {
            tag: "p",
            classes: vec!["foo"],
            id: None,
            attributes: vec![("class", "foo")],
            ancestors: vec![],
        };
        assert_eq!(match_rules(&elem, &sheet).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn attr_selector_specificity() {
        let selectors = parse_selector_list("[data-x]");
        assert_eq!(selectors[0].specificity, (0, 1, 0));
    }

    #[test]
    fn page_margin_box_top_center_string() {
        let css = r#"@page { @top-center { content: "Hello"; } }"#;
        let sheet = parse_stylesheet(css);
        let mb = sheet.page_style.margin_boxes.top_center.as_ref().unwrap();
        assert_eq!(mb.content, vec![ContentItem::String("Hello".to_string())]);
    }

    #[test]
    fn page_margin_box_counter_page() {
        let css = r#"@page { @bottom-center { content: counter(page); } }"#;
        let sheet = parse_stylesheet(css);
        let mb = sheet.page_style.margin_boxes.bottom_center.as_ref().unwrap();
        assert_eq!(mb.content, vec![ContentItem::CounterPage]);
    }

    #[test]
    fn page_decls_with_trailing_whitespace_do_not_hang() {
        // Regression: this used to infinite-loop because the old parser
        // used `while !is_exhausted` around `try_parse` which never
        // advanced on stray whitespace.
        let mut ps = PageStyle::default();
        parse_page_declarations("  size: a4;  margin: 36pt;   ", &mut ps);
        assert_eq!(ps.width, Some(595.0));
        assert_eq!(ps.margin_top, Some(36.0));
    }

    #[test]
    fn page_margin_box_page_n_of_m() {
        let css = r#"@page {
            size: a4;
            margin: 2cm;
            @bottom-right {
                content: "Page " counter(page) " of " counter(pages);
            }
        }"#;
        let sheet = parse_stylesheet(css);
        assert_eq!(sheet.page_style.width, Some(595.0));
        let mb = sheet.page_style.margin_boxes.bottom_right.as_ref().unwrap();
        assert_eq!(
            mb.content,
            vec![
                ContentItem::String("Page ".to_string()),
                ContentItem::CounterPage,
                ContentItem::String(" of ".to_string()),
                ContentItem::CounterPages,
            ]
        );
    }

    #[test]
    fn merge_style_non_none_wins() {
        let mut target = ComputedStyle::default();
        target.color = Some((1.0, 0.0, 0.0));
        let source = ComputedStyle {
            font_size: Some(CssLength::Pt(18.0)),
            ..ComputedStyle::default()
        };
        merge_style(&mut target, &source);
        assert_eq!(target.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(target.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }
}
