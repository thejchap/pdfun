use std::sync::{Mutex, OnceLock};

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
    /// CSS Values 3 `calc()` expression. The `u32` indexes into a
    /// process-wide intern arena (`CALC_ARENA`). This keeps `CssLength`
    /// `Copy`, so the 56 existing `resolve_ctx` call sites and the
    /// `ComputedStyle` merge macro don't need to change. Arena entries
    /// are never freed — bounded by stylesheet size per process.
    Calc(u32),
}

/// Nodes of a parsed `calc()` expression tree. Stored in `CALC_ARENA`
/// and referenced by index from `CssLength::Calc`. Operands are
/// themselves `CssLength` values (including nested `Calc` indices), so
/// the tree can recurse arbitrarily.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CalcOp {
    /// `a + b` where both are lengths (or nested calcs).
    Add(CssLength, CssLength),
    /// `a - b` where both are lengths (or nested calcs).
    Sub(CssLength, CssLength),
    /// `a * n` where `a` is a length and `n` is a unitless number.
    Mul(CssLength, f32),
    /// `a / n` where `a` is a length and `n` is a unitless number.
    Div(CssLength, f32),
}

static CALC_ARENA: OnceLock<Mutex<Vec<CalcOp>>> = OnceLock::new();

fn calc_arena() -> &'static Mutex<Vec<CalcOp>> {
    CALC_ARENA.get_or_init(|| Mutex::new(Vec::new()))
}

/// Intern a calc node and return its arena index.
fn intern_calc(op: CalcOp) -> u32 {
    let mut arena = calc_arena().lock().unwrap();
    let idx = u32::try_from(arena.len()).expect("calc arena overflow");
    arena.push(op);
    idx
}

/// Look up a calc node by index. Clones out so the mutex is released
/// before recursive resolution.
fn get_calc(idx: u32) -> CalcOp {
    calc_arena().lock().unwrap()[idx as usize]
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
            CssLength::Calc(idx) => match get_calc(idx) {
                CalcOp::Add(a, b) => a.resolve_ctx(ctx) + b.resolve_ctx(ctx),
                CalcOp::Sub(a, b) => a.resolve_ctx(ctx) - b.resolve_ctx(ctx),
                CalcOp::Mul(a, n) => a.resolve_ctx(ctx) * n,
                CalcOp::Div(a, n) => a.resolve_ctx(ctx) / n,
            },
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
    InlineBlock,
    None,
}

/// CSS `float` values. `None` is the initial state (not floated); `Left`
/// and `Right` take the element out of the normal flow and push it to the
/// corresponding edge of its containing block.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum FloatValue {
    #[default]
    None,
    Left,
    Right,
}

/// CSS `clear` values. Controls whether a block-level element is moved
/// below preceding floats on the matching side(s).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ClearValue {
    #[default]
    None,
    Left,
    Right,
    Both,
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

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PageBreakInside {
    Auto,
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

/// CSS `overflow` property (CSS 2.1 §11.1). Controls whether content
/// that exceeds the box's padding edge is clipped.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Overflow {
    /// Default: content is not clipped.
    #[default]
    Visible,
    /// Content is clipped at the padding edge.
    Hidden,
    /// In a paged medium (PDF), scroll/auto behave as hidden — there is
    /// no interactive scroll surface to render.
    Scroll,
    /// Same as Scroll in a paged medium.
    Auto,
}

impl Overflow {
    /// Whether this overflow value requires clipping to the padding box.
    pub fn clips(self) -> bool {
        !matches!(self, Overflow::Visible)
    }
}

/// CSS `white-space` property. Controls whitespace collapsing and line
/// wrapping for inline content.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WhiteSpace {
    /// Default: collapse runs of whitespace, wrap at word boundaries.
    #[default]
    Normal,
    /// Preserve all whitespace and newlines; no wrapping (`<pre>` default).
    Pre,
    /// Collapse whitespace, no wrapping — emit one long line.
    Nowrap,
    /// Preserve whitespace and newlines; wrap at word boundaries.
    PreWrap,
    /// Collapse runs of whitespace but preserve newlines; wrap at word
    /// boundaries. V1 treats this as `Normal` (newlines collapsed).
    PreLine,
}

impl WhiteSpace {
    /// Whether runs of whitespace should be preserved verbatim.
    pub fn preserves_whitespace(self) -> bool {
        matches!(self, WhiteSpace::Pre | WhiteSpace::PreWrap)
    }

    /// Whether line wrapping is allowed at word boundaries.
    pub fn allows_wrap(self) -> bool {
        matches!(
            self,
            WhiteSpace::Normal | WhiteSpace::PreWrap | WhiteSpace::PreLine
        )
    }
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

/// CSS `vertical-align`. Only the three block-level keywords we support on
/// table cells — baseline/sub/super/text-top/text-bottom are not modeled here
/// (super/sub flow through the run-level `baseline_shift` instead).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VerticalAlignValue {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// CSS `border-collapse` for tables.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BorderCollapseValue {
    /// Each cell draws its own border; default CSS behavior.
    #[default]
    Separate,
    /// Adjacent borders share a single stroke and the outer border is drawn
    /// as one rectangle with internal gridlines.
    Collapse,
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
            (page_break_inside, copy,  no),
            (orphans,           copy,  yes),
            (widows,            copy,  yes),
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
            (white_space,       copy,  yes),
            (text_indent,       copy,  yes),
            (border_radius,     copy,  no),
            (opacity,           copy,  no),
            (float,             copy,  no),
            (clear,             copy,  no),
            (vertical_align,    copy,  no),
            (border_collapse,   copy,  no),
            (overflow,          copy,  no),
            (pseudo_content,    clone, no),
        }
    };
}

macro_rules! merge_one {
    (copy, $t:ident, $s:ident, $f:ident) => {
        if $s.$f.is_some() {
            $t.$f = $s.$f;
        }
    };
    (clone, $t:ident, $s:ident, $f:ident) => {
        if $s.$f.is_some() {
            $t.$f.clone_from(&$s.$f);
        }
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
            pub fn inherit_into(&self, child: Option<&ComputedStyle>) -> ComputedStyle {
                let mut result = ComputedStyle::default();
                $( inherit_one!($kind, $inh, result, child, self, $name); )*
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
    pub page_break_inside: Option<PageBreakInside>,
    pub orphans: Option<u32>,
    pub widows: Option<u32>,
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
    pub white_space: Option<WhiteSpace>,
    pub text_indent: Option<CssLength>,
    pub border_radius: Option<[CssLength; 4]>,
    pub opacity: Option<f32>,
    pub float: Option<FloatValue>,
    pub clear: Option<ClearValue>,
    pub vertical_align: Option<VerticalAlignValue>,
    pub border_collapse: Option<BorderCollapseValue>,
    pub overflow: Option<Overflow>,
    /// String value of the CSS `content` property, used for `::before` /
    /// `::after` pseudo-elements. Lives here so `merge_style` can cascade
    /// it like any other property. Only the literal-string form is parsed
    /// today; `counter()`, `attr()`, and `url()` are not.
    pub pseudo_content: Option<String>,
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
                // Optional alpha: ", <number>" — parsed and ignored.
                let _ = input.try_parse(|i| -> Result<(), ParseError<'i, ()>> {
                    i.expect_comma()?;
                    let _ = i.expect_number()?;
                    Ok(())
                });
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
        Token::Function(name) if name.eq_ignore_ascii_case("calc") => {
            input.parse_nested_block(|i| parse_calc_expression(i))
        }
        _ => Err(location.new_custom_error(())),
    }
}

/// Parse the body of a `calc()` expression (the tokens inside the
/// parentheses). Handles `+ - * /` with standard precedence and
/// parenthesised sub-expressions. Returns a `CssLength::Calc(idx)` into
/// the process-wide arena unless the result collapses to a literal —
/// we keep the literal when possible to avoid arena churn on trivial
/// expressions like `calc(10px)`.
fn parse_calc_expression<'i>(input: &mut Parser<'i, '_>) -> Result<CssLength, ParseError<'i, ()>> {
    parse_calc_sum(input)
}

/// `sum := product ( ('+' | '-') product )*`
///
/// CSS Values §10: `+` and `-` in a calc require whitespace on both
/// sides. cssparser tokenises `a - b` differently from `a-b` — the
/// latter becomes a single identifier/number — so we just consume the
/// operator token. `a-b` simply won't parse as a subtraction.
fn parse_calc_sum<'i>(input: &mut Parser<'i, '_>) -> Result<CssLength, ParseError<'i, ()>> {
    let mut lhs = parse_calc_product(input)?;
    loop {
        let state = input.state();
        let op = match input.next() {
            Ok(Token::Delim('+')) => '+',
            Ok(Token::Delim('-')) => '-',
            _ => {
                input.reset(&state);
                break;
            }
        };
        let rhs = parse_calc_product(input)?;
        let node = match op {
            '+' => CalcOp::Add(lhs, rhs),
            '-' => CalcOp::Sub(lhs, rhs),
            _ => unreachable!(),
        };
        lhs = CssLength::Calc(intern_calc(node));
    }
    Ok(lhs)
}

/// `product := unit ( ('*' | '/') unit )*`
///
/// `*` and `/` have stricter operand rules per CSS Values §10: at least
/// one side must be a unitless number. We accept `<length> * <number>`,
/// `<number> * <length>`, and `<length> / <number>`. Division by a
/// length is not permitted and returns a parse error.
fn parse_calc_product<'i>(input: &mut Parser<'i, '_>) -> Result<CssLength, ParseError<'i, ()>> {
    let location = input.current_source_location();
    let mut lhs = parse_calc_atom(input)?;
    loop {
        let state = input.state();
        let op = match input.next() {
            Ok(Token::Delim('*')) => '*',
            Ok(Token::Delim('/')) => '/',
            _ => {
                input.reset(&state);
                break;
            }
        };
        // Peek for a numeric operand. If the next token is a number we
        // parse it directly; otherwise the operand must be a unit and
        // `lhs` must already be a number — in that case we swap.
        let after_op = input.state();
        let rhs_number = if let Ok(Token::Number { value, .. }) = input.next() {
            Some(*value)
        } else {
            input.reset(&after_op);
            None
        };
        let (operand, factor) = if let Some(n) = rhs_number {
            (lhs, n)
        } else if let CssLength::Pt(n_guess) = lhs {
            // `lhs` was produced from a bare number (see parse_calc_atom
            // which encodes bare numbers as Pt — only valid here as a
            // multiplier, not as a real length).
            let rhs = parse_calc_atom(input)?;
            (rhs, n_guess)
        } else {
            return Err(location.new_custom_error(()));
        };
        let node = match op {
            '*' => CalcOp::Mul(operand, factor),
            '/' if op == '/' => {
                if rhs_number.is_none() {
                    return Err(location.new_custom_error(()));
                }
                CalcOp::Div(operand, factor)
            }
            _ => unreachable!(),
        };
        lhs = CssLength::Calc(intern_calc(node));
    }
    Ok(lhs)
}

/// `atom := length | percentage | number | '(' sum ')' | 'calc' '(' sum ')'`
///
/// Bare numbers are temporarily encoded as `CssLength::Pt(n)` so they
/// can flow through the same pipeline as lengths; `parse_calc_product`
/// recognises them by shape when combining with `*` or `/`.
fn parse_calc_atom<'i>(input: &mut Parser<'i, '_>) -> Result<CssLength, ParseError<'i, ()>> {
    let location = input.current_source_location();
    let state = input.state();
    match input.next()?.clone() {
        Token::ParenthesisBlock => input.parse_nested_block(|i| parse_calc_sum(i)),
        Token::Function(name) if name.eq_ignore_ascii_case("calc") => {
            input.parse_nested_block(|i| parse_calc_sum(i))
        }
        Token::Number { value, .. } => Ok(CssLength::Pt(value)),
        _ => {
            input.reset(&state);
            parse_css_length(input).map_err(|_| location.new_custom_error(()))
        }
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
        // `calc()` can produce any sign at resolve time — accept and let
        // downstream layout clamp if needed. This matches CSS Values 3:
        // negative `calc()` results for border-width are resolved to 0 at
        // used-value time, not rejected at parse time.
        CssLength::Calc(_) => 0.0,
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

/// Parse the `border-radius` shorthand into a four-corner array in CSS
/// order `[top-left, top-right, bottom-right, bottom-left]`.
///
/// - `border-radius: a` → `[a, a, a, a]`
/// - `border-radius: a b` → `[a, b, a, b]` (tl=br, tr=bl)
/// - `border-radius: a b c` → `[a, b, c, b]`
/// - `border-radius: a b c d` → `[a, b, c, d]`
///
/// The `/` elliptical form (`border-radius: a b / c d`) is not supported —
/// everything after the first `/` is consumed and ignored, and the corner
/// values use the circular interpretation of the first length list only.
fn parse_border_radius_shorthand<'i>(
    input: &mut Parser<'i, '_>,
) -> Result<[CssLength; 4], ParseError<'i, ()>> {
    let first = parse_non_negative_length(input)?;
    let second = input.try_parse(parse_non_negative_length).ok();
    let third = input.try_parse(parse_non_negative_length).ok();
    let fourth = input.try_parse(parse_non_negative_length).ok();
    // Consume and ignore any elliptical second set (`/ …`).
    let _ = input.try_parse(|i: &mut Parser<'i, '_>| -> Result<(), ParseError<'i, ()>> {
        i.expect_delim('/')?;
        while i.try_parse(parse_non_negative_length).is_ok() {}
        Ok(())
    });

    // CSS spec corner order: [top-left, top-right, bottom-right, bottom-left].
    Ok(match (second, third, fourth) {
        (None, _, _) => [first, first, first, first],
        (Some(s), None, _) => [first, s, first, s],
        (Some(s), Some(t), None) => [first, s, t, s],
        (Some(s), Some(t), Some(f)) => [first, s, t, f],
    })
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
                    if input.try_parse(cssparser::Parser::expect_comma).is_err() {
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
                match u32::try_from(n) {
                    Ok(n) if n >= 1 => self.style.column_count = Some(n),
                    _ => return Err(input.new_custom_error(())),
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
                self.style.text_decoration = Some(TextDecoration {
                    underline,
                    line_through,
                });
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
            "page-break-inside" | "break-inside" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let pbi = match ident.to_ascii_lowercase().as_str() {
                    "auto" => PageBreakInside::Auto,
                    "avoid" | "avoid-page" | "avoid-column" => PageBreakInside::Avoid,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.page_break_inside = Some(pbi);
            }
            "orphans" => {
                let n = input.expect_integer()?;
                if n < 1 {
                    return Err(input.new_custom_error(()));
                }
                self.style.orphans = Some(n.cast_unsigned());
            }
            "widows" => {
                let n = input.expect_integer()?;
                if n < 1 {
                    return Err(input.new_custom_error(()));
                }
                self.style.widows = Some(n.cast_unsigned());
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
            "border-radius" => {
                self.style.border_radius = Some(parse_border_radius_shorthand(input)?);
            }
            "border-top-left-radius" => {
                let r = parse_non_negative_length(input)?;
                let mut arr = self.style.border_radius.unwrap_or([CssLength::Px(0.0); 4]);
                arr[0] = r;
                self.style.border_radius = Some(arr);
            }
            "border-top-right-radius" => {
                let r = parse_non_negative_length(input)?;
                let mut arr = self.style.border_radius.unwrap_or([CssLength::Px(0.0); 4]);
                arr[1] = r;
                self.style.border_radius = Some(arr);
            }
            "border-bottom-right-radius" => {
                let r = parse_non_negative_length(input)?;
                let mut arr = self.style.border_radius.unwrap_or([CssLength::Px(0.0); 4]);
                arr[2] = r;
                self.style.border_radius = Some(arr);
            }
            "border-bottom-left-radius" => {
                let r = parse_non_negative_length(input)?;
                let mut arr = self.style.border_radius.unwrap_or([CssLength::Px(0.0); 4]);
                arr[3] = r;
                self.style.border_radius = Some(arr);
            }
            "opacity" => {
                let location = input.current_source_location();
                let token = input.next()?.clone();
                let alpha = match &token {
                    Token::Number { value, .. } => *value,
                    Token::Percentage { unit_value, .. } => *unit_value,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.opacity = Some(alpha.clamp(0.0, 1.0));
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
            "white-space" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let ws = match ident.to_ascii_lowercase().as_str() {
                    "normal" => WhiteSpace::Normal,
                    "pre" => WhiteSpace::Pre,
                    "nowrap" => WhiteSpace::Nowrap,
                    "pre-wrap" => WhiteSpace::PreWrap,
                    "pre-line" => WhiteSpace::PreLine,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.white_space = Some(ws);
            }
            "overflow" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let ov = match ident.to_ascii_lowercase().as_str() {
                    "visible" => Overflow::Visible,
                    "hidden" | "clip" => Overflow::Hidden,
                    "scroll" => Overflow::Scroll,
                    "auto" => Overflow::Auto,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.overflow = Some(ov);
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
                    "inline-block" => self.style.display = Some(DisplayValue::InlineBlock),
                    _ => return Err(location.new_custom_error(())),
                }
            }
            "float" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                match ident.to_ascii_lowercase().as_str() {
                    "none" => self.style.float = Some(FloatValue::None),
                    "left" => self.style.float = Some(FloatValue::Left),
                    "right" => self.style.float = Some(FloatValue::Right),
                    _ => return Err(location.new_custom_error(())),
                }
            }
            "clear" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                match ident.to_ascii_lowercase().as_str() {
                    "none" => self.style.clear = Some(ClearValue::None),
                    "left" => self.style.clear = Some(ClearValue::Left),
                    "right" => self.style.clear = Some(ClearValue::Right),
                    "both" => self.style.clear = Some(ClearValue::Both),
                    _ => return Err(location.new_custom_error(())),
                }
            }
            "vertical-align" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let va = match ident.to_ascii_lowercase().as_str() {
                    "top" => VerticalAlignValue::Top,
                    "middle" => VerticalAlignValue::Middle,
                    "bottom" => VerticalAlignValue::Bottom,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.vertical_align = Some(va);
            }
            "border-collapse" => {
                let location = input.current_source_location();
                let ident = input.expect_ident()?.clone();
                let bc = match ident.to_ascii_lowercase().as_str() {
                    "separate" => BorderCollapseValue::Separate,
                    "collapse" => BorderCollapseValue::Collapse,
                    _ => return Err(location.new_custom_error(())),
                };
                self.style.border_collapse = Some(bc);
            }
            "content" => {
                // Only the literal-string form of `content`. `none` and the
                // CSS-wide idents like `normal`/`inherit` are treated as
                // "no content" (which is also the default).
                if let Ok(s) =
                    input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_string()))
                {
                    self.style.pseudo_content = Some(s);
                } else if let Ok(ident) =
                    input.try_parse(|i| i.expect_ident().map(|s| s.as_ref().to_ascii_lowercase()))
                {
                    if ident != "none" && ident != "normal" {
                        return Err(input.new_custom_error(()));
                    }
                } else {
                    return Err(input.new_custom_error(()));
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
    /// Pseudo-class such as `:first-child`, `:nth-child(2n+1)`, `:not(.foo)`.
    PseudoClass(PseudoClass),
}

/// CSS pseudo-elements. Unlike pseudo-classes, these don't select an
/// existing DOM element — they attach a style (and optionally generated
/// content) to a synthetic position before/after the element's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PseudoElement {
    Before,
    After,
}

/// CSS pseudo-classes supported by the selector matcher.
#[derive(Clone, Debug, PartialEq)]
pub enum PseudoClass {
    FirstChild,
    LastChild,
    OnlyChild,
    NthChild(AnB),
    /// `:not(compound)` — single compound-selector argument (no selector lists).
    Not(Box<CompoundSelector>),
}

/// `An+B` formula for `:nth-child()`.
#[derive(Clone, Debug, PartialEq, Copy)]
pub struct AnB {
    pub a: i32,
    pub b: i32,
}

impl AnB {
    /// Does this formula match a 1-based position `n`?
    pub fn matches(self, n: i32) -> bool {
        if n < 1 {
            return false;
        }
        if self.a == 0 {
            return n == self.b;
        }
        // n = a*k + b  =>  k = (n - b) / a
        let diff = n - self.b;
        if diff == 0 {
            return true;
        }
        if self.a > 0 {
            diff >= 0 && diff % self.a == 0
        } else {
            diff <= 0 && diff % self.a == 0
        }
    }
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
    /// Adjacent sibling combinator: `h1 + p`.
    AdjacentSibling,
    /// General sibling combinator: `h1 ~ p`.
    GeneralSibling,
}

/// A compound selector is a sequence of simple selectors that all match one element.
/// For example, `p.note#main` = [Type("p"), Class("note"), Id("main")].
#[derive(Clone, Debug, PartialEq)]
pub struct CompoundSelector {
    pub parts: Vec<SimpleSelector>,
}

impl CompoundSelector {
    /// Full match including optional sibling-position data used for pseudo-classes.
    /// `sibling_pos` is the 0-based element-sibling index; `sibling_count` is the
    /// number of element siblings on the shared parent (including self).
    #[allow(clippy::too_many_arguments)]
    fn matches_full(
        &self,
        tag: &str,
        classes: &[&str],
        id: Option<&str>,
        attributes: &[(&str, &str)],
        sibling_pos: Option<usize>,
        sibling_count: Option<usize>,
    ) -> bool {
        self.parts.iter().all(|part| match part {
            SimpleSelector::Type(t) => t.eq_ignore_ascii_case(tag),
            SimpleSelector::Class(c) => classes.iter().any(|cl| cl.eq_ignore_ascii_case(c)),
            SimpleSelector::Id(i) => id.is_some_and(|elem_id| elem_id.eq_ignore_ascii_case(i)),
            SimpleSelector::Universal => true,
            SimpleSelector::PseudoClass(pc) => match pc {
                PseudoClass::FirstChild => sibling_pos == Some(0),
                PseudoClass::LastChild => match (sibling_pos, sibling_count) {
                    (Some(i), Some(n)) => i + 1 == n,
                    _ => false,
                },
                PseudoClass::OnlyChild => match (sibling_pos, sibling_count) {
                    (Some(i), Some(n)) => i == 0 && n == 1,
                    _ => false,
                },
                PseudoClass::NthChild(anb) => match sibling_pos {
                    Some(i) => anb.matches(i as i32 + 1),
                    None => false,
                },
                PseudoClass::Not(inner) => {
                    !inner.matches_full(tag, classes, id, attributes, sibling_pos, sibling_count)
                }
            },
            SimpleSelector::Attribute { name, op, value } => {
                let attr_value = attributes
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(name))
                    .map(|(_, v)| *v);
                match (op, value) {
                    (AttrOp::Exists, _) | (_, None) => attr_value.is_some(),
                    (AttrOp::Equals, Some(v)) => attr_value == Some(v.as_str()),
                    (AttrOp::Includes, Some(v)) => {
                        if v.is_empty() || v.contains(char::is_whitespace) {
                            return false;
                        }
                        attr_value.is_some_and(|av| av.split_whitespace().any(|tok| tok == v))
                    }
                    (AttrOp::DashMatch, Some(v)) => {
                        attr_value.is_some_and(|av| av == v || av.starts_with(&format!("{v}-")))
                    }
                    (AttrOp::Prefix, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.is_some_and(|av| av.starts_with(v.as_str()))
                    }
                    (AttrOp::Suffix, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.is_some_and(|av| av.ends_with(v.as_str()))
                    }
                    (AttrOp::Substring, Some(v)) => {
                        if v.is_empty() {
                            return false;
                        }
                        attr_value.is_some_and(|av| av.contains(v.as_str()))
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
    /// Trailing pseudo-element (`::before` / `::after`). When `None`, the
    /// chain targets real DOM elements; otherwise it targets a synthetic
    /// slot attached to the matched element.
    pub pseudo_element: Option<PseudoElement>,
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
/// Splits on whitespace and recognises `>`, `+`, `~` combinators.
fn parse_one_selector_from_text(text: &str) -> Option<SelectorChain> {
    let mut compounds: Vec<CompoundSelector> = Vec::new();
    let mut combinators: Vec<Combinator> = Vec::new();
    let mut id_count: u16 = 0;
    let mut class_count: u16 = 0;
    let mut type_count: u16 = 0;
    let mut pseudo_element: Option<PseudoElement> = None;

    // Tokenize into a stream where compound selectors and combinator symbols
    // are separate tokens. We walk character-by-character so combinators
    // attached to the previous compound (e.g. `h1+p`) are split properly.
    let tokens = tokenize_selector(text);

    let mut pending_combinator: Option<Combinator> = None;
    let mut first = true;
    for tok in tokens {
        match tok {
            SelectorToken::Combinator(c) => {
                // A combinator symbol overrides any pending descendant combinator.
                pending_combinator = Some(c);
            }
            SelectorToken::Whitespace => {
                if !first && pending_combinator.is_none() {
                    pending_combinator = Some(Combinator::Descendant);
                }
            }
            SelectorToken::Compound(text) => {
                let compound = parse_compound_from_text(
                    &text,
                    &mut id_count,
                    &mut class_count,
                    &mut type_count,
                    &mut pseudo_element,
                );
                if compound.parts.is_empty() {
                    continue;
                }
                if !first {
                    combinators.push(pending_combinator.unwrap_or(Combinator::Descendant));
                }
                pending_combinator = None;
                compounds.push(compound);
                first = false;
            }
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
        pseudo_element,
    })
}

enum SelectorToken {
    Compound(String),
    Combinator(Combinator),
    Whitespace,
}

/// Walk the selector text and split it into compound-selector tokens,
/// whitespace runs, and explicit combinator symbols (`>`, `+`, `~`).
/// Respects `[...]` brackets and `(...)` pseudo-class argument parens.
fn tokenize_selector(text: &str) -> Vec<SelectorToken> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            out.push(SelectorToken::Whitespace);
            continue;
        }
        if c == '>' {
            out.push(SelectorToken::Combinator(Combinator::Child));
            i += 1;
            continue;
        }
        if c == '+' {
            out.push(SelectorToken::Combinator(Combinator::AdjacentSibling));
            i += 1;
            continue;
        }
        if c == '~' {
            // Could be an attribute `~=` inside `[...]`, but we're outside of
            // brackets here (brackets consumed as part of compound).
            out.push(SelectorToken::Combinator(Combinator::GeneralSibling));
            i += 1;
            continue;
        }
        // Start of a compound: consume until we hit whitespace or a top-level
        // combinator symbol, respecting `[...]` and `(...)` nesting.
        let start = i;
        let mut depth_brack: i32 = 0;
        let mut depth_paren: i32 = 0;
        while i < chars.len() {
            let ch = chars[i];
            if depth_brack == 0
                && depth_paren == 0
                && (ch.is_whitespace() || ch == '>' || ch == '+' || ch == '~')
            {
                break;
            }
            if ch == '[' {
                depth_brack += 1;
            } else if ch == ']' {
                depth_brack -= 1;
            } else if ch == '(' {
                depth_paren += 1;
            } else if ch == ')' {
                depth_paren -= 1;
            }
            i += 1;
        }
        if start < i {
            out.push(SelectorToken::Compound(chars[start..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    out
}

/// Parse a compound selector from a text token like `p.note#main[data-x="y"]`.
fn parse_compound_from_text(
    text: &str,
    id_count: &mut u16,
    class_count: &mut u16,
    type_count: &mut u16,
    pseudo_element: &mut Option<PseudoElement>,
) -> CompoundSelector {
    // Stop characters for ident-like runs.
    fn is_boundary(c: char) -> bool {
        c == '.' || c == '#' || c == '[' || c == ':' || c == '('
    }

    let mut parts: Vec<SimpleSelector> = Vec::new();
    let mut chars = text.char_indices().peekable();

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
            ':' => {
                chars.next();
                // `::x` is a pseudo-element; `:x` is usually a pseudo-class,
                // except for the legacy CSS 2.1 forms `:before` / `:after`
                // which aliased today's `::before` / `::after`.
                let double_colon = chars.peek().map(|&(_, c)| c) == Some(':');
                if double_colon {
                    chars.next();
                }
                let name_start = chars.peek().map_or(text.len(), |&(i, _)| i);
                while let Some(&(_, c)) = chars.peek() {
                    if c.is_ascii_alphanumeric() || c == '-' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                let name_end = chars.peek().map_or(text.len(), |&(i, _)| i);
                let name = text[name_start..name_end].to_ascii_lowercase();

                if double_colon || matches!(name.as_str(), "before" | "after") {
                    // Pseudo-element. Record it on the chain and skip the
                    // compound slot — the selector subject stays the real
                    // element; the pseudo is a tag on the whole chain.
                    match name.as_str() {
                        "before" => *pseudo_element = Some(PseudoElement::Before),
                        "after" => *pseudo_element = Some(PseudoElement::After),
                        _ => {} // unknown pseudo-element: silently skip
                    }
                    continue;
                }

                // Optional parenthesised argument.
                let mut arg: Option<String> = None;
                if chars.peek().map(|&(_, c)| c) == Some('(') {
                    chars.next();
                    let arg_start = chars.peek().map_or(text.len(), |&(i, _)| i);
                    let mut depth = 1;
                    let mut arg_end = text.len();
                    while let Some(&(i, c)) = chars.peek() {
                        if c == '(' {
                            depth += 1;
                            chars.next();
                        } else if c == ')' {
                            depth -= 1;
                            if depth == 0 {
                                arg_end = i;
                                chars.next();
                                break;
                            }
                            chars.next();
                        } else {
                            chars.next();
                        }
                    }
                    arg = Some(text[arg_start..arg_end].to_string());
                }

                if let Some((pseudo, spec_delta)) = parse_pseudo_class(&name, arg.as_deref()) {
                    parts.push(SimpleSelector::PseudoClass(pseudo));
                    // `:not(x)` contributes the specificity of its argument,
                    // other pseudo-classes add (0,1,0) like classes.
                    *id_count += spec_delta.0;
                    *class_count += spec_delta.1;
                    *type_count += spec_delta.2;
                }
                // Unknown pseudo-class: silently skip.
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

/// Parse a pseudo-class by name (lowercased) and optional argument text.
/// Returns `(pseudo, specificity_delta)` on success or `None` to skip.
fn parse_pseudo_class(name: &str, arg: Option<&str>) -> Option<(PseudoClass, (u16, u16, u16))> {
    match name {
        "first-child" => Some((PseudoClass::FirstChild, (0, 1, 0))),
        "last-child" => Some((PseudoClass::LastChild, (0, 1, 0))),
        "only-child" => Some((PseudoClass::OnlyChild, (0, 1, 0))),
        "nth-child" => {
            let arg = arg?.trim();
            let anb = parse_an_b(arg)?;
            Some((PseudoClass::NthChild(anb), (0, 1, 0)))
        }
        "not" => {
            let arg = arg?.trim();
            // Parse the argument as a single compound selector. `:not(::before)`
            // is semantically nonsense and ignored — we discard the pseudo.
            let mut id_c = 0u16;
            let mut cls_c = 0u16;
            let mut typ_c = 0u16;
            let mut discard: Option<PseudoElement> = None;
            let compound =
                parse_compound_from_text(arg, &mut id_c, &mut cls_c, &mut typ_c, &mut discard);
            if compound.parts.is_empty() {
                return None;
            }
            Some((PseudoClass::Not(Box::new(compound)), (id_c, cls_c, typ_c)))
        }
        _ => None,
    }
}

/// Parse an `An+B` microsyntax argument to `:nth-child()`:
/// `odd`, `even`, `3`, `2n`, `2n+1`, `-n+3`, `n`, ` +3n - 2 `, etc.
pub(crate) fn parse_an_b(arg: &str) -> Option<AnB> {
    let s: String = arg.chars().filter(|c| !c.is_whitespace()).collect();
    let lower = s.to_ascii_lowercase();
    if lower == "odd" {
        return Some(AnB { a: 2, b: 1 });
    }
    if lower == "even" {
        return Some(AnB { a: 2, b: 0 });
    }
    if lower.is_empty() {
        return None;
    }

    // Split into (a-part, b-part) around an `n` if present.
    if let Some(n_pos) = lower.find('n') {
        let a_str = &lower[..n_pos];
        let b_str = &lower[n_pos + 1..];
        let a: i32 = match a_str {
            "" | "+" => 1,
            "-" => -1,
            s => s.parse().ok()?,
        };
        let b: i32 = if b_str.is_empty() {
            0
        } else {
            // Must start with + or -
            let first = b_str.as_bytes()[0] as char;
            if first != '+' && first != '-' {
                return None;
            }
            b_str.parse().ok()?
        };
        Some(AnB { a, b })
    } else {
        // Just a literal integer B.
        let b: i32 = lower.parse().ok()?;
        Some(AnB { a: 0, b })
    }
}

/// Parse the contents inside `[` ... `]` into an attribute selector.
fn parse_attribute_selector(inner: &str) -> Option<SimpleSelector> {
    fn is_name_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '-' || c == '_'
    }

    let inner = inner.trim();
    if inner.is_empty() {
        return None;
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
                    page_style.margin_right = Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                "margin-bottom" => {
                    page_style.margin_bottom =
                        Some(parse_non_negative_length(input)?.resolve(12.0));
                }
                "margin-left" => {
                    page_style.margin_left = Some(parse_non_negative_length(input)?.resolve(12.0));
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

/// Does an `@media` prelude apply in a print context?
///
/// Returns true unless the query explicitly targets a non-print medium
/// (e.g. `screen` only). Complex media features like `(max-width: 600px)`
/// are not evaluated — we err toward inclusion, matching browser
/// behavior when a query type is unrecognized.
fn media_query_applies_to_print(prelude: &str) -> bool {
    let trimmed = prelude.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Split on `,` to get the comma-separated list of queries. A rule
    // applies if ANY clause matches print.
    for clause in trimmed.split(',') {
        let c = clause.trim().to_ascii_lowercase();
        // Strip a leading `not`/`only` keyword. `not` inverts the match.
        let (negate, rest) = if let Some(r) = c.strip_prefix("not ") {
            (true, r.trim_start())
        } else if let Some(r) = c.strip_prefix("only ") {
            (false, r.trim_start())
        } else {
            (false, c.as_str())
        };
        // First token is the media type (or `(feature: value)` expression).
        let first_token = rest.split_ascii_whitespace().next().unwrap_or("");
        let ty_matches = match first_token {
            "print" | "all" | "" => true,
            t if t.starts_with('(') => true, // feature query w/o explicit type
            _ => false,
        };
        if ty_matches != negate {
            return true;
        }
    }
    false
}

/// Consume leading whitespace and any `@import ...;` / `@charset ...;`
/// statements from the front of `remaining`. Returns the rest of the
/// stylesheet after those have been skipped.
///
/// `@import` is recognized and discarded without fetching the referenced
/// stylesheet — pdfun has no file-resolver plumbed into the CSS layer
/// yet, and silently ignoring the directive is preferable to hard-failing
/// the whole sheet. Consumers that need imports can inline the CSS
/// themselves.
fn skip_leading_at_statements(mut remaining: &str) -> &str {
    loop {
        let trimmed = remaining.trim_start();
        if trimmed.starts_with("@import") || trimmed.starts_with("@charset") {
            if let Some(end) = trimmed.find(';') {
                remaining = &trimmed[end + 1..];
                continue;
            }
            // Malformed (no semicolon): drop the remainder.
            return "";
        }
        return trimmed;
    }
}

/// Parse stylesheet by manually scanning for `{ }` blocks.
fn parse_stylesheet_manual(css: &str, rules: &mut Vec<CssRule>, page_style: &mut PageStyle) {
    let mut remaining = css;

    while !remaining.trim().is_empty() {
        remaining = skip_leading_at_statements(remaining);
        if remaining.is_empty() {
            break;
        }

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

        // Handle @media <query> { ...rules... }. The body holds full
        // CSS rules that should be applied only when the query matches
        // the print output context. We recurse into `parse_stylesheet_manual`
        // for matching blocks so nested at-rules (e.g. another @media)
        // behave consistently.
        if let Some(prelude) = selector_text.strip_prefix("@media") {
            let Some(brace_close) = find_matching_close(after_open) else {
                break;
            };
            let body = &after_open[..brace_close];
            remaining = &after_open[brace_close + 1..];
            if media_query_applies_to_print(prelude) {
                parse_stylesheet_manual(body, rules, page_style);
            }
            continue;
        }

        // Other unknown at-rules: skip the block entirely.
        if selector_text.starts_with('@') {
            let Some(brace_close) = find_matching_close(after_open) else {
                break;
            };
            remaining = &after_open[brace_close + 1..];
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
                    while input.try_parse(cssparser::Parser::expect_comma).is_ok() {
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
        if let Ok(s) = input.try_parse(|i| i.expect_string().map(|s| s.as_ref().to_string())) {
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

/// A minimal record for preceding element siblings used by sibling combinators.
#[derive(Clone, Debug)]
pub struct SiblingRecord<'a> {
    pub tag: &'a str,
    pub classes: Vec<&'a str>,
    pub id: Option<&'a str>,
    pub attributes: Vec<(&'a str, &'a str)>,
    /// 0-based element-sibling index on the shared parent.
    pub sibling_index: usize,
    /// Total element-sibling count on the shared parent.
    pub sibling_count: usize,
}

/// Ancestor record for selector matching: (tag, classes, id, attributes).
pub type AncestorInfo<'a> = (
    &'a str,
    Vec<&'a str>,
    Option<&'a str>,
    Vec<(&'a str, &'a str)>,
);

/// Information about an element needed for selector matching.
pub struct ElementInfo<'a> {
    pub tag: &'a str,
    pub classes: Vec<&'a str>,
    pub id: Option<&'a str>,
    /// All attributes on the element (name, value).
    pub attributes: Vec<(&'a str, &'a str)>,
    /// Ancestor chain from parent to root.
    pub ancestors: Vec<AncestorInfo<'a>>,
    /// 0-based index of this element among its element siblings.
    pub sibling_index: usize,
    /// Total number of element siblings on the shared parent (including self).
    pub sibling_count: usize,
    /// Preceding element siblings in document order.
    pub preceding_siblings: Vec<SiblingRecord<'a>>,
}

/// Match all rules in a stylesheet against an element, returning the
/// merged `ComputedStyle` from all matching rules (respecting specificity).
/// Rules whose selector carries a pseudo-element (`::before`/`::after`)
/// are skipped — those are surfaced via `match_pseudo_rules`.
pub fn match_rules(element: &ElementInfo<'_>, stylesheet: &Stylesheet) -> ComputedStyle {
    match_rules_for(element, stylesheet, None)
}

/// Match rules with a specific pseudo-element tag. Used to resolve the
/// computed style for a synthetic `::before` / `::after` slot.
pub fn match_pseudo_rules(
    element: &ElementInfo<'_>,
    stylesheet: &Stylesheet,
    pseudo: PseudoElement,
) -> ComputedStyle {
    match_rules_for(element, stylesheet, Some(pseudo))
}

fn match_rules_for(
    element: &ElementInfo<'_>,
    stylesheet: &Stylesheet,
    pseudo: Option<PseudoElement>,
) -> ComputedStyle {
    let mut matches: Vec<(u16, u16, u16, usize, &ComputedStyle)> = Vec::new();

    for (rule_idx, rule) in stylesheet.rules.iter().enumerate() {
        for selector in &rule.selectors {
            if selector.pseudo_element != pseudo {
                continue;
            }
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

fn compound_matches_sibling(compound: &CompoundSelector, sib: &SiblingRecord<'_>) -> bool {
    compound.matches_full(
        sib.tag,
        &sib.classes,
        sib.id,
        &sib.attributes,
        Some(sib.sibling_index),
        Some(sib.sibling_count),
    )
}

fn selector_matches(selector: &SelectorChain, element: &ElementInfo<'_>) -> bool {
    // The subject (first compound after reversal) must match the element
    let subject = &selector.compounds[0];
    if !subject.matches_full(
        element.tag,
        &element.classes,
        element.id,
        &element.attributes,
        Some(element.sibling_index),
        Some(element.sibling_count),
    ) {
        return false;
    }

    // Match ancestor/sibling chain
    if selector.compounds.len() == 1 {
        return true;
    }

    // Walk the combinator chain. We track the current "cursor" position in the
    // tree: either walking ancestors (for descendant/child combinators) or
    // walking preceding siblings (for sibling combinators).
    //
    // For sibling combinators, the cursor stays at the same parent level, so
    // subsequent ancestor combinators resume from the element's own ancestor
    // list. This is a simplification that covers the common cases used in
    // the brief (e.g. `div > p:first-child`, `h1 + p`, `h1 ~ p`).
    let mut ancestor_idx: usize = 0;
    // Current siblings view: at the start this is the subject element's own
    // preceding_siblings, but after walking an ancestor combinator we lose
    // the sibling context, which is fine because subsequent sibling
    // combinators on a different level are not expressible without selector
    // lists anyway.
    let mut sibling_cursor: Option<usize> = Some(element.preceding_siblings.len());
    // When Some(k), the next sibling combinator looks at
    // element.preceding_siblings[..k]. When None, sibling combinators cannot
    // be applied (we've walked past sibling scope).

    for i in 1..selector.compounds.len() {
        let compound = &selector.compounds[i];
        let combinator = selector.combinators[i - 1];

        match combinator {
            Combinator::Child => {
                if ancestor_idx >= element.ancestors.len() {
                    return false;
                }
                let (tag, ref classes, id, ref attrs) = element.ancestors[ancestor_idx];
                // Ancestors don't carry sibling info — pseudo-classes referring
                // to sibling position will fail to match here. That's
                // acceptable for the scope of this work.
                if !compound.matches_full(tag, classes, id, attrs, None, None) {
                    return false;
                }
                ancestor_idx += 1;
                sibling_cursor = None;
            }
            Combinator::Descendant => {
                let mut found = false;
                while ancestor_idx < element.ancestors.len() {
                    let (tag, ref classes, id, ref attrs) = element.ancestors[ancestor_idx];
                    ancestor_idx += 1;
                    if compound.matches_full(tag, classes, id, attrs, None, None) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return false;
                }
                sibling_cursor = None;
            }
            Combinator::AdjacentSibling => {
                let Some(k) = sibling_cursor else {
                    return false;
                };
                if k == 0 {
                    return false;
                }
                let sib = &element.preceding_siblings[k - 1];
                if !compound_matches_sibling(compound, sib) {
                    return false;
                }
                sibling_cursor = Some(k - 1);
            }
            Combinator::GeneralSibling => {
                let Some(k) = sibling_cursor else {
                    return false;
                };
                let mut found_at: Option<usize> = None;
                for j in (0..k).rev() {
                    if compound_matches_sibling(compound, &element.preceding_siblings[j]) {
                        found_at = Some(j);
                        break;
                    }
                }
                match found_at {
                    Some(j) => sibling_cursor = Some(j),
                    None => return false,
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
        assert_eq!(
            selectors[0].compounds[0].parts,
            vec![SimpleSelector::Type("p".into())]
        );
    }

    #[test]
    fn selector_class() {
        let selectors = parse_selector_list(".highlight");
        assert_eq!(selectors.len(), 1);
        assert_eq!(
            selectors[0].compounds[0].parts,
            vec![SimpleSelector::Class("highlight".into())]
        );
    }

    #[test]
    fn selector_id() {
        let selectors = parse_selector_list("#header");
        assert_eq!(selectors.len(), 1);
        assert_eq!(
            selectors[0].compounds[0].parts,
            vec![SimpleSelector::Id("header".into())]
        );
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

    fn test_elem<'a>(
        tag: &'a str,
        classes: Vec<&'a str>,
        id: Option<&'a str>,
        attributes: Vec<(&'a str, &'a str)>,
        ancestors: Vec<AncestorInfo<'a>>,
    ) -> ElementInfo<'a> {
        ElementInfo {
            tag,
            classes,
            id,
            attributes,
            ancestors,
            sibling_index: 0,
            sibling_count: 1,
            preceding_siblings: Vec::new(),
        }
    }

    #[test]
    fn match_type_selector() {
        let sheet = parse_stylesheet("p { color: red }");
        let elem = test_elem("p", vec![], None, vec![], vec![]);
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_class_selector() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = test_elem("p", vec!["red"], None, vec![], vec![]);
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn no_match_wrong_class() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = test_elem("p", vec!["blue"], None, vec![], vec![]);
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn match_descendant_selector() {
        let sheet = parse_stylesheet("div p { color: red }");
        let elem = test_elem(
            "p",
            vec![],
            None,
            vec![],
            vec![("div", vec![], None, vec![])],
        );
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_child_selector() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = test_elem(
            "p",
            vec![],
            None,
            vec![],
            vec![("div", vec![], None, vec![])],
        );
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn child_no_match_grandchild() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = test_elem(
            "p",
            vec![],
            None,
            vec![],
            vec![
                ("blockquote", vec![], None, vec![]),
                ("div", vec![], None, vec![]),
            ],
        );
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn specificity_class_beats_type() {
        let sheet = parse_stylesheet("p { color: red } .blue { color: blue }");
        let elem = test_elem("p", vec!["blue"], None, vec![], vec![]);
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((0.0, 0.0, 1.0)));
    }

    // ── Attribute selector tests ──────────────────────────────

    #[test]
    fn match_attr_exists() {
        let sheet = parse_stylesheet("[data-x] { color: red }");
        let elem = test_elem("p", vec![], None, vec![("data-x", "")], vec![]);
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_equals() {
        let sheet = parse_stylesheet("[data-role=\"primary\"] { color: red }");
        let elem = test_elem("p", vec![], None, vec![("data-role", "primary")], vec![]);
        assert_eq!(match_rules(&elem, &sheet).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_includes() {
        let sheet = parse_stylesheet("[class~=\"note\"] { color: red }");
        let elem_match = test_elem(
            "p",
            vec!["intro", "note", "main"],
            None,
            vec![("class", "intro note main")],
            vec![],
        );
        assert_eq!(
            match_rules(&elem_match, &sheet).color,
            Some((1.0, 0.0, 0.0))
        );

        let elem_no = test_elem(
            "p",
            vec!["notepad"],
            None,
            vec![("class", "notepad")],
            vec![],
        );
        assert!(match_rules(&elem_no, &sheet).color.is_none());
    }

    #[test]
    fn match_attr_dashmatch() {
        let sheet = parse_stylesheet("[lang|=\"en\"] { color: red }");
        let a = test_elem("p", vec![], None, vec![("lang", "en")], vec![]);
        let b = test_elem("p", vec![], None, vec![("lang", "en-US")], vec![]);
        let c = test_elem("p", vec![], None, vec![("lang", "english")], vec![]);
        assert_eq!(match_rules(&a, &sheet).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&b, &sheet).color, Some((1.0, 0.0, 0.0)));
        assert!(match_rules(&c, &sheet).color.is_none());
    }

    #[test]
    fn match_attr_prefix_suffix_substring() {
        let sheet_prefix = parse_stylesheet("[href^=\"http\"] { color: red }");
        let sheet_suffix = parse_stylesheet("[src$=\".png\"] { color: red }");
        let sheet_substr = parse_stylesheet("[class*=\"big\"] { color: red }");
        let a = test_elem(
            "a",
            vec![],
            None,
            vec![("href", "https://example.com")],
            vec![],
        );
        let b = test_elem("img", vec![], None, vec![("src", "pic.png")], vec![]);
        let c = test_elem(
            "div",
            vec!["bigbox"],
            None,
            vec![("class", "bigbox")],
            vec![],
        );
        assert_eq!(match_rules(&a, &sheet_prefix).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&b, &sheet_suffix).color, Some((1.0, 0.0, 0.0)));
        assert_eq!(match_rules(&c, &sheet_substr).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_attr_compound_with_type() {
        let sheet = parse_stylesheet("p[class=\"foo\"] { color: red }");
        let elem = test_elem("p", vec!["foo"], None, vec![("class", "foo")], vec![]);
        assert_eq!(match_rules(&elem, &sheet).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn attr_selector_specificity() {
        let selectors = parse_selector_list("[data-x]");
        assert_eq!(selectors[0].specificity, (0, 1, 0));
    }

    // ── Pseudo-class and sibling combinator tests ─────────────

    #[test]
    fn parse_an_b_keywords() {
        assert_eq!(parse_an_b("odd"), Some(AnB { a: 2, b: 1 }));
        assert_eq!(parse_an_b("even"), Some(AnB { a: 2, b: 0 }));
        assert_eq!(parse_an_b("ODD"), Some(AnB { a: 2, b: 1 }));
    }

    #[test]
    fn parse_an_b_integer() {
        assert_eq!(parse_an_b("3"), Some(AnB { a: 0, b: 3 }));
    }

    #[test]
    fn parse_an_b_n_forms() {
        assert_eq!(parse_an_b("n"), Some(AnB { a: 1, b: 0 }));
        assert_eq!(parse_an_b("2n"), Some(AnB { a: 2, b: 0 }));
        assert_eq!(parse_an_b("2n+1"), Some(AnB { a: 2, b: 1 }));
        assert_eq!(parse_an_b(" 2n + 1 "), Some(AnB { a: 2, b: 1 }));
        assert_eq!(parse_an_b("-n+3"), Some(AnB { a: -1, b: 3 }));
    }

    #[test]
    fn an_b_matches_arithmetic() {
        let even = AnB { a: 2, b: 0 };
        assert!(even.matches(2));
        assert!(even.matches(4));
        assert!(!even.matches(1));
        assert!(!even.matches(3));

        let odd = AnB { a: 2, b: 1 };
        assert!(odd.matches(1));
        assert!(odd.matches(3));
        assert!(!odd.matches(2));

        let literal = AnB { a: 0, b: 3 };
        assert!(literal.matches(3));
        assert!(!literal.matches(2));

        let neg = AnB { a: -1, b: 3 };
        // matches n = 1, 2, 3
        assert!(neg.matches(1));
        assert!(neg.matches(2));
        assert!(neg.matches(3));
        assert!(!neg.matches(4));
    }

    #[test]
    fn parse_first_child_pseudo() {
        let selectors = parse_selector_list("p:first-child");
        assert_eq!(selectors.len(), 1);
        let parts = &selectors[0].compounds[0].parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], SimpleSelector::Type("p".into()));
        assert_eq!(
            parts[1],
            SimpleSelector::PseudoClass(PseudoClass::FirstChild)
        );
        // Specificity: (0, 1, 1) — type + pseudo-class
        assert_eq!(selectors[0].specificity, (0, 1, 1));
    }

    #[test]
    fn parse_nth_child_pseudo() {
        let selectors = parse_selector_list("li:nth-child(2n+1)");
        let parts = &selectors[0].compounds[0].parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(
            parts[1],
            SimpleSelector::PseudoClass(PseudoClass::NthChild(AnB { a: 2, b: 1 }))
        );
    }

    #[test]
    fn parse_not_pseudo_specificity() {
        let selectors = parse_selector_list("p:not(.foo)");
        // type p (0,0,1) + :not(.foo) adds (0,1,0) = (0,1,1)
        assert_eq!(selectors[0].specificity, (0, 1, 1));
        let parts = &selectors[0].compounds[0].parts;
        assert!(matches!(
            &parts[1],
            SimpleSelector::PseudoClass(PseudoClass::Not(_))
        ));
    }

    #[test]
    fn parse_adjacent_sibling_combinator() {
        let selectors = parse_selector_list("h1 + p");
        assert_eq!(selectors[0].compounds.len(), 2);
        assert_eq!(selectors[0].combinators, vec![Combinator::AdjacentSibling]);
    }

    #[test]
    fn parse_general_sibling_combinator() {
        let selectors = parse_selector_list("h1 ~ p");
        assert_eq!(selectors[0].compounds.len(), 2);
        assert_eq!(selectors[0].combinators, vec![Combinator::GeneralSibling]);
    }

    #[test]
    fn match_first_child() {
        let sheet = parse_stylesheet("p:first-child { color: red }");
        let mut first = test_elem("p", vec![], None, vec![], vec![]);
        first.sibling_index = 0;
        first.sibling_count = 3;
        assert_eq!(match_rules(&first, &sheet).color, Some((1.0, 0.0, 0.0)));

        let mut second = test_elem("p", vec![], None, vec![], vec![]);
        second.sibling_index = 1;
        second.sibling_count = 3;
        assert!(match_rules(&second, &sheet).color.is_none());
    }

    #[test]
    fn match_nth_child_even() {
        let sheet = parse_stylesheet("li:nth-child(2n) { color: red }");
        let mut li2 = test_elem("li", vec![], None, vec![], vec![]);
        li2.sibling_index = 1; // 2nd
        li2.sibling_count = 4;
        assert_eq!(match_rules(&li2, &sheet).color, Some((1.0, 0.0, 0.0)));

        let mut li1 = test_elem("li", vec![], None, vec![], vec![]);
        li1.sibling_index = 0;
        li1.sibling_count = 4;
        assert!(match_rules(&li1, &sheet).color.is_none());
    }

    #[test]
    fn match_not_pseudo() {
        let sheet = parse_stylesheet("p:not(.skip) { color: red }");
        let plain = test_elem("p", vec![], None, vec![], vec![]);
        assert_eq!(match_rules(&plain, &sheet).color, Some((1.0, 0.0, 0.0)));

        let skipped = test_elem("p", vec!["skip"], None, vec![], vec![]);
        assert!(match_rules(&skipped, &sheet).color.is_none());
    }

    #[test]
    fn match_adjacent_sibling() {
        let sheet = parse_stylesheet("h1 + p { color: red }");
        let mut p = test_elem("p", vec![], None, vec![], vec![]);
        p.sibling_index = 1;
        p.sibling_count = 2;
        p.preceding_siblings = vec![SiblingRecord {
            tag: "h1",
            classes: vec![],
            id: None,
            attributes: vec![],
            sibling_index: 0,
            sibling_count: 2,
        }];
        assert_eq!(match_rules(&p, &sheet).color, Some((1.0, 0.0, 0.0)));

        // A p following a div should NOT match h1+p.
        let mut p2 = test_elem("p", vec![], None, vec![], vec![]);
        p2.sibling_index = 1;
        p2.sibling_count = 2;
        p2.preceding_siblings = vec![SiblingRecord {
            tag: "div",
            classes: vec![],
            id: None,
            attributes: vec![],
            sibling_index: 0,
            sibling_count: 2,
        }];
        assert!(match_rules(&p2, &sheet).color.is_none());
    }

    #[test]
    fn match_general_sibling() {
        let sheet = parse_stylesheet("h1 ~ p { color: red }");
        // Structure: h1, div, p. The p comes after the h1 but not adjacently.
        let mut p = test_elem("p", vec![], None, vec![], vec![]);
        p.sibling_index = 2;
        p.sibling_count = 3;
        p.preceding_siblings = vec![
            SiblingRecord {
                tag: "h1",
                classes: vec![],
                id: None,
                attributes: vec![],
                sibling_index: 0,
                sibling_count: 3,
            },
            SiblingRecord {
                tag: "div",
                classes: vec![],
                id: None,
                attributes: vec![],
                sibling_index: 1,
                sibling_count: 3,
            },
        ];
        assert_eq!(match_rules(&p, &sheet).color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_child_with_first_child_pseudo() {
        // `div > p:first-child` - compose pseudo-classes with combinators.
        let sheet = parse_stylesheet("div > p:first-child { color: red }");
        let mut first = test_elem(
            "p",
            vec![],
            None,
            vec![],
            vec![("div", vec![], None, vec![])],
        );
        first.sibling_index = 0;
        first.sibling_count = 2;
        assert_eq!(match_rules(&first, &sheet).color, Some((1.0, 0.0, 0.0)));

        let mut second = test_elem(
            "p",
            vec![],
            None,
            vec![],
            vec![("div", vec![], None, vec![])],
        );
        second.sibling_index = 1;
        second.sibling_count = 2;
        assert!(match_rules(&second, &sheet).color.is_none());
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
        let css = r"@page { @bottom-center { content: counter(page); } }";
        let sheet = parse_stylesheet(css);
        let mb = sheet
            .page_style
            .margin_boxes
            .bottom_center
            .as_ref()
            .unwrap();
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
        let mut target = ComputedStyle {
            color: Some((1.0, 0.0, 0.0)),
            ..ComputedStyle::default()
        };
        let source = ComputedStyle {
            font_size: Some(CssLength::Pt(18.0)),
            ..ComputedStyle::default()
        };
        merge_style(&mut target, &source);
        assert_eq!(target.color, Some((1.0, 0.0, 0.0)));
        assert!(matches!(target.font_size, Some(CssLength::Pt(v)) if (v - 18.0).abs() < 0.001));
    }
}
