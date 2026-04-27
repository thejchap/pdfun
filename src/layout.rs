use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::box_tree::{AnonymousBox, BlockBox, Node};
use crate::css;
use crate::font_metrics;
use crate::{BUILTIN_FONTS, PageContent, PdfDocument, PdfOp, RegisteredFont};

/// Emit the right "show text" operator for a font: built-in fonts use
/// `Tj` with a `(WinAnsi)` literal string, but `@font-face` faces are
/// embedded as Type0 / `CIDFontType2` / Identity-H — for those we have
/// to emit a `Vec<char>` that the page writer remaps into 16-bit glyph
/// IDs.
///
/// For the built-in path, transcode the input UTF-8 text into PDF's
/// `WinAnsiEncoding` byte string here so the content stream carries
/// the bytes the viewer expects (`Caf\xe9`, not the raw UTF-8 `Caf\xc3\xa9`).
/// Non-mappable codepoints fall back to `?` per char — WS-1B will
/// replace that fallback with a split-and-promote onto a Unicode face.
fn show_text_for(font_name: &str, text: String) -> PdfOp {
    if font_name.starts_with("Custom-") {
        PdfOp::ShowGlyphs(text.chars().collect())
    } else {
        PdfOp::ShowText(crate::transcode_with_fallback(&text))
    }
}

/// Word-wrap text into lines that fit within `max_width` points.
pub fn wrap_text_impl(
    text: &str,
    max_width: f32,
    font_name: &str,
    font_size: f32,
) -> Result<Vec<String>, String> {
    if font_metrics::get_builtin_metrics(font_name).is_none() {
        return Err(format!("unknown font: {font_name}"));
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }

    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    for word in &words {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else {
            let candidate = format!("{current_line} {word}");
            let width = font_metrics::measure_str(&candidate, font_name, font_size).unwrap();
            if width <= max_width {
                current_line = candidate;
            } else {
                lines.push(current_line);
                current_line = word.to_string();
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    Ok(lines)
}

// ── TextAlign ─────────────────────────────────────────────────

#[derive(Clone, Debug, Default, PartialEq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

// ── BlockStyle ────────────────────────────────────────────────

#[derive(Clone)]
pub struct BlockStyle {
    pub color: Option<(f32, f32, f32)>,
    pub background_color: Option<(f32, f32, f32)>,
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub border_width: f32,
    pub border_color: Option<(f32, f32, f32)>,
    pub border_style: Option<css::BorderStyle>,
    pub text_align: TextAlign,
    pub page_break_before: Option<css::PageBreak>,
    pub page_break_after: Option<css::PageBreak>,
    pub page_break_inside: Option<css::PageBreakInside>,
    /// CSS `orphans`: minimum lines of a paragraph that must be left at the
    /// bottom of a page before a split. Default 2.
    pub orphans: u32,
    /// CSS `widows`: minimum lines of a paragraph that must be carried to
    /// the top of the next page after a split. Default 2.
    pub widows: u32,
    /// Resolved content-box sizing constraints in PDF points. `None` means
    /// "auto" / "no constraint". These are applied when computing the
    /// content width of a block in `render_paragraph_block`.
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    /// Extra per-character space in points (CSS `letter-spacing`). Added
    /// after every glyph, including the last — matches PDF `Tc` semantics.
    pub letter_spacing: f32,
    /// Extra space between words in points (CSS `word-spacing`). Applied
    /// on top of the space character's natural width, and on top of any
    /// justification widening.
    pub word_spacing: f32,
    /// CSS `box-sizing`. Default is `ContentBox`, where `width`/`height`
    /// refer to the content area; `BorderBox` makes them include padding
    /// and border.
    pub box_sizing: css::BoxSizing,
    /// CSS `text-indent`: horizontal indent of the first line of text in a
    /// block, in points. Positive values shift the first line to the right.
    pub text_indent: f32,
    /// CSS `list-style-position`: whether the marker hangs outside the
    /// principal block (default) or flows inline as the first inline box
    /// on the first line.
    pub list_style_position: css::ListStylePosition,
    /// Per-corner `border-radius` resolved to points. CSS order
    /// `[top-left, top-right, bottom-right, bottom-left]`. `None` means
    /// draw sharp rectangles (the fast path — no rounded ops emitted).
    pub border_radius: Option<[f32; 4]>,
    /// CSS `opacity`. `None` or `Some(1.0)` means fully opaque (no state
    /// change emitted). Values in `[0.0, 1.0)` wrap the block's content
    /// emission in `SaveState` → `SetAlpha` → … → `RestoreState`.
    pub opacity: Option<f32>,
    /// CSS `float`. A floated block is taken out of the normal flow and
    /// shifted to the left or right edge of its containing block.
    /// Subsequent in-flow blocks have their text area narrowed by the
    /// float's width until `cursor_y` descends past its bottom.
    pub float: css::FloatValue,
    /// CSS `clear`. Before rendering, `cursor_y` is advanced past the
    /// bottom of any matching in-flight floats.
    pub clear: css::ClearValue,
    /// CSS `overflow`. When not `Visible`, content is clipped to the
    /// padding box via a `W n` path after the normal border/background
    /// draw. In a paged medium, `scroll`/`auto` collapse to `hidden`.
    pub overflow: css::Overflow,
    /// CSS `position`. `Static` (default, no-op) and `Relative` keep
    /// the box in normal flow — `Relative` shifts the painted box by
    /// `(position_left, -position_top)` without disturbing siblings.
    /// `Absolute` and `Fixed` take the box out of flow and paint it
    /// relative to the page (the initial containing block). `Fixed`
    /// additionally replays onto every page.
    pub position: css::Position,
    /// CSS `top` offset in points. For `Relative`, shifts the painted
    /// box downward. For `Absolute` / `Fixed`, anchors the box's top
    /// edge this far below the page's top.
    pub position_top: Option<f32>,
    /// CSS `left` offset in points. For `Relative`, shifts the painted
    /// box rightward. For `Absolute` / `Fixed`, anchors the box's left
    /// edge this far from the page's left.
    pub position_left: Option<f32>,
    /// CSS `right` offset in points. Used by `Absolute` / `Fixed` when
    /// `left` is absent — anchors the box's right edge this far from
    /// the page's right. Ignored in normal flow / `Relative`.
    pub position_right: Option<f32>,
    /// CSS `bottom` offset in points. Used by `Absolute` / `Fixed`
    /// when `top` is absent — anchors the box's bottom edge this far
    /// from the page's bottom. Ignored in normal flow / `Relative`.
    pub position_bottom: Option<f32>,
    /// Resolved background image parameters. `None` means no background
    /// image was specified (or loading failed). Present value carries the
    /// arena index of the loaded image plus the repeat/size/position
    /// parameters needed to emit the tile grid at paint time.
    pub background_image: Option<BackgroundImageParams>,
    /// Resolved `box-shadow` layers in CSS declaration order. Paint order
    /// is back-to-front (last layer paints first, so the first-declared
    /// shadow sits on top). Empty = no shadows.
    pub box_shadow: Vec<ResolvedBoxShadow>,
}

/// Paint-time `box-shadow` layer with all lengths resolved to points.
/// Blur is stored but currently painted as sharp — we have no soft-mask
/// emitter yet.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedBoxShadow {
    pub offset_x: f32,
    pub offset_y: f32,
    #[allow(dead_code)]
    pub blur: f32,
    pub spread: f32,
    pub color: (f32, f32, f32),
    pub inset: bool,
}

/// Paint-time parameters for a block's `background-image`. The image has
/// already been decoded and registered into `Layout::images`; `index`
/// references that slot.
#[derive(Clone, Copy, Debug)]
pub struct BackgroundImageParams {
    pub index: usize,
    pub intrinsic_width: f32,
    pub intrinsic_height: f32,
    pub repeat: css::BackgroundRepeat,
    pub size: css::BackgroundSize,
    pub position: css::BackgroundPosition,
}

impl Default for BlockStyle {
    fn default() -> Self {
        Self {
            color: None,
            background_color: None,
            margin_top: 0.0,
            margin_right: 0.0,
            margin_bottom: 0.0,
            margin_left: 0.0,
            padding_top: 0.0,
            padding_right: 0.0,
            padding_bottom: 0.0,
            padding_left: 0.0,
            border_width: 0.0,
            border_color: None,
            border_style: None,
            text_align: TextAlign::default(),
            page_break_before: None,
            page_break_after: None,
            page_break_inside: None,
            orphans: 2,
            widows: 2,
            width: None,
            height: None,
            min_width: None,
            min_height: None,
            max_width: None,
            max_height: None,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            box_sizing: css::BoxSizing::default(),
            text_indent: 0.0,
            list_style_position: css::ListStylePosition::default(),
            border_radius: None,
            opacity: None,
            float: css::FloatValue::default(),
            clear: css::ClearValue::default(),
            overflow: css::Overflow::default(),
            position: css::Position::default(),
            position_top: None,
            position_left: None,
            position_right: None,
            position_bottom: None,
            background_image: None,
            box_shadow: Vec::new(),
        }
    }
}

impl BlockStyle {
    fn has_any_styling(&self) -> bool {
        self.color.is_some()
            || self.background_color.is_some()
            || self.border_width > 0.0
            || self.margin_top > 0.0
            || self.margin_right > 0.0
            || self.margin_bottom > 0.0
            || self.margin_left > 0.0
            || self.border_radius.is_some()
            || self.background_image.is_some()
            || self.needs_alpha()
    }

    /// Does this block need a non-default alpha state (opacity < 1)?
    pub fn needs_alpha(&self) -> bool {
        matches!(self.opacity, Some(a) if a < 1.0)
    }
}

// ── TextRun ──────────────────────────────────────────────────

/// A text segment with its own font, size, and optional color.
#[pyclass]
#[derive(Clone, Default)]
pub struct TextRun {
    #[pyo3(get)]
    pub text: String,
    #[pyo3(get)]
    pub font_name: String,
    #[pyo3(get)]
    pub font_size: f32,
    #[pyo3(get)]
    pub color: Option<(f32, f32, f32)>,
    pub text_decoration: Option<css::TextDecoration>,
    pub link_url: Option<String>,
    /// Baseline offset in points. Positive raises the glyphs (superscript),
    /// negative lowers them (subscript). Default 0. Does not affect line
    /// height — shifted runs ride along the enclosing line box.
    pub baseline_shift: f32,
    /// If `Some`, this run is an inline-block atom with a fixed total
    /// width (in points). The wrapper treats it as a single unbreakable
    /// glyph whose width is `inline_block_width` instead of measuring the
    /// text with font metrics. The text is still painted inside, centered
    /// within the available interior area.
    pub inline_block_width: Option<f32>,
    /// Background color for an inline-block atom — draws a filled rect
    /// behind the text.
    pub inline_block_bg: Option<(f32, f32, f32)>,
    /// Stroked border `(width_pt, (r, g, b))` for an inline-block atom.
    pub inline_block_border: Option<(f32, (f32, f32, f32))>,
    /// Horizontal padding (in points) added inside an inline-block atom
    /// — used when centering the text inside its fixed width.
    pub inline_block_padding_x: f32,
}

#[pymethods]
impl TextRun {
    #[new]
    #[pyo3(signature = (text, font_name="Helvetica", font_size=12.0, color=None))]
    fn new(text: String, font_name: &str, font_size: f64, color: Option<(f64, f64, f64)>) -> Self {
        TextRun {
            text,
            font_name: font_name.to_string(),
            font_size: font_size as f32,
            color: color.map(rgb_to_f32),
            ..Default::default()
        }
    }
}

// ── Paragraph ────────────────────────────────────────────────

#[derive(Clone)]
pub struct Paragraph {
    pub runs: Vec<TextRun>,
    pub line_height: Option<f32>,
    pub spacing_after: f32,
    pub style: BlockStyle,
    pub marker: Option<TextRun>,
    pub is_hr: bool,
    pub white_space: css::WhiteSpace,
    /// Source tag name (e.g. "h1", "p"). Only set by `html_render` for
    /// tags that need to participate in document-level features like the
    /// heading outline. `None` means "doesn't matter" (list items, etc.).
    pub tag: Option<&'static str>,
    /// An id="…" attribute captured during DOM walk. Registered as a
    /// `GoTo` destination at render time so `<a href="#id">` links can
    /// resolve to this block's page and y-position.
    pub anchor_id: Option<String>,
}

// ── Tables ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

pub struct TableCell {
    pub runs: Vec<TextRun>,
    pub line_height: Option<f32>,
    pub style: BlockStyle,
    pub vertical_align: VerticalAlign,
}

pub struct TableRow {
    pub cells: Vec<TableCell>,
}

pub struct Table {
    pub rows: Vec<TableRow>,
    pub style: BlockStyle,
    pub spacing_after: f32,
    /// Default line-height for cells (resolved from CSS).
    pub default_line_height: Option<f32>,
    pub caption: Option<Box<Paragraph>>,
    pub border_collapse: css::BorderCollapseValue,
    /// Per-column style records built from `<colgroup>` / `<col>` (CSS
    /// 2.1 §17.3). `None` when no `<colgroup>` was declared, in which
    /// case the layout falls through to its intrinsic algorithm.
    /// `Some(empty)` means the table had a colgroup but no columns
    /// (treated the same as `None`).
    pub col_styles: Option<Vec<css::ColStyle>>,
    /// CSS `table-layout` (CSS 2.1 §17.5.2). `Auto` (default) treats
    /// `<col width>` as a preferred-width hint bounded below by the
    /// content's min-width; `Fixed` pins column widths to their hints.
    pub table_layout: css::TableLayoutValue,
}

pub struct ImageBlock {
    pub image_index: usize,
    pub width: f32,
    pub height: f32,
    pub spacing_after: f32,
    pub style: BlockStyle,
}

pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    Image(ImageBlock),
    /// Marks the start of a CSS block container (e.g. a `<div>` wrapping
    /// block children). Emitted by `html_render` so that the container's
    /// margins can participate in parent/child collapsing (CSS 2.1 § 8.3.1).
    /// The container itself does not paint — its background/border/padding
    /// are currently dropped. Paired LIFO with a matching `ContainerEnd`.
    ContainerStart(BlockStyle),
    /// Marks the end of a container opened by `ContainerStart`.
    ContainerEnd(BlockStyle),
}

/// Mutable state threaded through the recursive box-tree walker in
/// `LayoutInner::finish()`. Holds the cursor position, current page, the
/// pending collapsed margins (parent/child and sibling/sibling), and the
/// column geometry. Recursion into a child block shares the same
/// `RenderState` so margin collapsing and page breaks remain correct
/// across nesting levels.
struct RenderState {
    col_count: u32,
    col_width: f32,
    col_gap: f32,
    content_top: f32,
    current_page: Arc<Mutex<PageContent>>,
    cursor_y: f32,
    current_col: u32,
    /// Pending bottom margin from the previously-rendered sibling. When
    /// the next block renders, its effective `margin_top` is collapsed
    /// against this value (CSS 2.1 § 8.3.1).
    pending_bottom: f32,
    /// Pending top margin from enclosing containers that have not yet
    /// committed any child content. Folds into `pending_bottom` when the
    /// first child leaf renders, so parent/child collapsing falls out
    /// naturally without needing a separate stack.
    pending_container_top: f32,
    /// Active floats in the current block formatting context. Each float
    /// occupies a rectangle from `top_y` down to `bottom_y`, pinned to
    /// either the left or right edge of its column. Subsequent in-flow
    /// blocks whose `cursor_y` falls within that range have their text
    /// area shifted and narrowed to avoid the float. Floats drop out of
    /// this list when the cursor advances past their bottom.
    active_floats: Vec<ActiveFloat>,
}

/// A float currently affecting the layout in a block formatting context.
/// Y-coordinates use the same down-is-negative convention as `cursor_y`:
/// `top_y > bottom_y`.
#[derive(Clone, Copy, Debug)]
struct ActiveFloat {
    side: css::FloatValue,
    left: f32,
    right: f32,
    top_y: f32,
    bottom_y: f32,
    /// Column this float belongs to. A float only affects subsequent
    /// blocks in the same column.
    column: u32,
}

// ── Wrapping internals ──────────────────────────────────────

struct StyledWord {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
    text_decoration: Option<css::TextDecoration>,
    link_url: Option<String>,
    baseline_shift: f32,
    inline_block_width: Option<f32>,
    inline_block_bg: Option<(f32, f32, f32)>,
    inline_block_border: Option<(f32, (f32, f32, f32))>,
    inline_block_padding_x: f32,
}

struct LineSegment {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
    width: f32,
    text_decoration: Option<css::TextDecoration>,
    link_url: Option<String>,
    baseline_shift: f32,
    inline_block_width: Option<f32>,
    inline_block_bg: Option<(f32, f32, f32)>,
    inline_block_border: Option<(f32, (f32, f32, f32))>,
    inline_block_padding_x: f32,
}

struct WrappedLine {
    segments: Vec<LineSegment>,
    total_width: f32,
    max_font_size: f32,
}

/// Collapse two adjoining vertical margins per CSS 2.1 § 8.3.1.
///
/// - Both non-negative: max of the two.
/// - Both negative: min of the two (i.e. the most-negative).
/// - Mixed sign: their sum (positive + negative).
fn collapse_margins(a: f32, b: f32) -> f32 {
    if a >= 0.0 && b >= 0.0 {
        a.max(b)
    } else if a < 0.0 && b < 0.0 {
        a.min(b)
    } else {
        a + b
    }
}

/// Amount to subtract from `cursor_y` to advance into a block whose
/// top margin is `curr_top`, given that `pending` of bottom margin from
/// the previous block has already been consumed. Returns the delta such
/// that the resulting collapsed advance equals `collapse(pending, curr_top)`.
fn collapsed_top_delta(pending: f32, curr_top: f32) -> f32 {
    collapse_margins(pending, curr_top) - pending
}

/// Extra per-line spacing applied during wrapping. `letter_spacing` is
/// added after every glyph, `word_spacing` after every space.
#[derive(Clone, Copy, Default)]
struct SpacingOpts {
    letter_spacing: f32,
    word_spacing: f32,
}

/// Wrap a sequence of `TextRun`s into lines that fit within `max_width`.
fn wrap_runs_impl(
    runs: &[TextRun],
    max_width: f32,
    ws: css::WhiteSpace,
    spacing: SpacingOpts,
) -> Result<Vec<WrappedLine>, String> {
    if ws.preserves_whitespace() {
        // Pre / PreWrap: explicit newlines split lines. PreWrap additionally
        // wraps overly-long lines at word boundaries; Pre lets them overflow.
        let wrap_limit = if ws.allows_wrap() {
            max_width
        } else {
            f32::INFINITY
        };
        return wrap_runs_preformatted(runs, spacing, wrap_limit);
    }

    // Collapse-whitespace path: Normal, Nowrap, and PreLine (V1 treats
    // PreLine as Normal — newlines collapse). Nowrap disables wrapping by
    // giving the greedy algorithm an effectively infinite line budget.
    let effective_max = if ws.allows_wrap() {
        max_width
    } else {
        f32::INFINITY
    };

    // Phase 1: flatten runs into styled words. Inline-block runs are
    // treated as a single unbreakable atom regardless of their text
    // content — they keep their fixed width and are not split on
    // whitespace.
    let mut words: Vec<StyledWord> = Vec::new();
    for run in runs {
        if run.inline_block_width.is_some() {
            words.push(StyledWord {
                text: run.text.clone(),
                font_name: run.font_name.clone(),
                font_size: run.font_size,
                color: run.color,
                text_decoration: run.text_decoration,
                link_url: run.link_url.clone(),
                baseline_shift: run.baseline_shift,
                inline_block_width: run.inline_block_width,
                inline_block_bg: run.inline_block_bg,
                inline_block_border: run.inline_block_border,
                inline_block_padding_x: run.inline_block_padding_x,
            });
            continue;
        }
        for word in run.text.split_whitespace() {
            words.push(StyledWord {
                text: word.to_string(),
                font_name: run.font_name.clone(),
                font_size: run.font_size,
                color: run.color,
                text_decoration: run.text_decoration,
                link_url: run.link_url.clone(),
                baseline_shift: run.baseline_shift,
                inline_block_width: None,
                inline_block_bg: None,
                inline_block_border: None,
                inline_block_padding_x: 0.0,
            });
        }
    }

    if words.is_empty() {
        return Ok(Vec::new());
    }

    // Phase 2: greedy line-breaking
    let mut line_words: Vec<Vec<StyledWord>> = Vec::new();
    let mut current: Vec<StyledWord> = Vec::new();
    let mut current_width: f32 = 0.0;

    for word in words {
        let word_width = if let Some(w) = word.inline_block_width {
            w
        } else {
            let base_word_width =
                font_metrics::measure_str(&word.text, &word.font_name, word.font_size)
                    .ok_or_else(|| format!("unknown font: {}", word.font_name))?;
            base_word_width + spacing.letter_spacing * word.text.chars().count() as f32
        };

        if current.is_empty() {
            current_width = word_width;
            current.push(word);
        } else {
            let base_space =
                font_metrics::measure_str(" ", &word.font_name, word.font_size).unwrap_or(0.0);
            // The joining space is itself a glyph, so one letter-spacing
            // advances after it as well as word-spacing.
            let space_width = base_space + spacing.letter_spacing + spacing.word_spacing;
            if current_width + space_width + word_width <= effective_max {
                current_width += space_width + word_width;
                current.push(word);
            } else {
                line_words.push(std::mem::take(&mut current));
                current_width = word_width;
                current.push(word);
            }
        }
    }
    if !current.is_empty() {
        line_words.push(current);
    }

    // Phase 3: merge adjacent same-style words into segments
    let mut result: Vec<WrappedLine> = Vec::new();
    for words in &line_words {
        let mut segments: Vec<LineSegment> = Vec::new();

        for word in words {
            #[allow(clippy::float_cmp)] // font sizes are user-set, not computed
            let can_merge = word.inline_block_width.is_none()
                && segments.last().is_some_and(|last: &LineSegment| {
                    last.inline_block_width.is_none()
                        && last.font_name == word.font_name
                        && last.font_size == word.font_size
                        && last.color == word.color
                        && last.text_decoration == word.text_decoration
                        && last.link_url == word.link_url
                        && last.baseline_shift == word.baseline_shift
                });

            if can_merge {
                let last = segments.last_mut().unwrap();
                last.text.push(' ');
                last.text.push_str(&word.text);
                let base = font_metrics::measure_str(&last.text, &last.font_name, last.font_size)
                    .unwrap_or(0.0);
                let n_chars = last.text.chars().count() as f32;
                let n_spaces = last.text.matches(' ').count() as f32;
                last.width =
                    base + spacing.letter_spacing * n_chars + spacing.word_spacing * n_spaces;
            } else if let Some(ib_width) = word.inline_block_width {
                // Inline-block atom — fixed width, never merges.
                let leading_space = if segments.is_empty() {
                    0.0
                } else {
                    font_metrics::measure_str(" ", &word.font_name, word.font_size).unwrap_or(0.0)
                        + spacing.letter_spacing
                        + spacing.word_spacing
                };
                segments.push(LineSegment {
                    text: word.text.clone(),
                    font_name: word.font_name.clone(),
                    font_size: word.font_size,
                    color: word.color,
                    width: ib_width + leading_space,
                    text_decoration: word.text_decoration,
                    link_url: word.link_url.clone(),
                    baseline_shift: word.baseline_shift,
                    inline_block_width: Some(ib_width),
                    inline_block_bg: word.inline_block_bg,
                    inline_block_border: word.inline_block_border,
                    inline_block_padding_x: word.inline_block_padding_x,
                });
            } else {
                // Prepend space if not the first segment on the line
                let text = if segments.is_empty() {
                    word.text.clone()
                } else {
                    format!(" {}", word.text)
                };
                let base = font_metrics::measure_str(&text, &word.font_name, word.font_size)
                    .unwrap_or(0.0);
                let n_chars = text.chars().count() as f32;
                let n_spaces = text.matches(' ').count() as f32;
                let width =
                    base + spacing.letter_spacing * n_chars + spacing.word_spacing * n_spaces;
                segments.push(LineSegment {
                    text,
                    font_name: word.font_name.clone(),
                    font_size: word.font_size,
                    color: word.color,
                    width,
                    text_decoration: word.text_decoration,
                    link_url: word.link_url.clone(),
                    baseline_shift: word.baseline_shift,
                    inline_block_width: None,
                    inline_block_bg: None,
                    inline_block_border: None,
                    inline_block_padding_x: 0.0,
                });
            }
        }

        let total_width: f32 = segments.iter().map(|s| s.width).sum();
        let max_font_size = segments.iter().map(|s| s.font_size).fold(0.0_f32, f32::max);
        result.push(WrappedLine {
            segments,
            total_width,
            max_font_size,
        });
    }

    Ok(result)
}

/// Wrap preformatted text: preserve whitespace, split on newlines.
///
/// When `wrap_limit` is finite (`PreWrap`), lines that exceed the limit are
/// additionally soft-wrapped at the last space that still fits. The
/// leading whitespace on each hard-newline segment is preserved.
fn wrap_runs_preformatted(
    runs: &[TextRun],
    spacing: SpacingOpts,
    wrap_limit: f32,
) -> Result<Vec<WrappedLine>, String> {
    let mut full_text = String::new();
    let mut font_name = String::new();
    let mut font_size = 12.0_f32;
    let mut color = None;
    let mut text_decoration = None;
    let mut link_url = None;
    let mut baseline_shift = 0.0_f32;

    for run in runs {
        full_text.push_str(&run.text);
        font_name.clone_from(&run.font_name);
        font_size = run.font_size;
        color = run.color;
        text_decoration = run.text_decoration;
        link_url.clone_from(&run.link_url);
        baseline_shift = run.baseline_shift;
    }

    let measure = |s: &str| -> Result<f32, String> {
        let base = font_metrics::measure_str(s, &font_name, font_size)
            .ok_or_else(|| format!("unknown font: {font_name}"))?;
        Ok(base
            + spacing.letter_spacing * s.chars().count() as f32
            + spacing.word_spacing * s.matches(' ').count() as f32)
    };

    let emit = |text: String, width: f32| WrappedLine {
        segments: vec![LineSegment {
            text,
            font_name: font_name.clone(),
            font_size,
            color,
            width,
            text_decoration,
            link_url: link_url.clone(),
            baseline_shift,
            inline_block_width: None,
            inline_block_bg: None,
            inline_block_border: None,
            inline_block_padding_x: 0.0,
        }],
        total_width: width,
        max_font_size: font_size,
    };

    let mut result: Vec<WrappedLine> = Vec::new();
    for line in full_text.split('\n') {
        let width = measure(line)?;
        if !wrap_limit.is_finite() || width <= wrap_limit || line.is_empty() {
            result.push(emit(line.to_string(), width));
            continue;
        }

        // PreWrap soft-wrap: walk the line, breaking at whitespace where
        // possible. We keep whitespace characters attached to the segment
        // they follow so the preserved indentation survives.
        let mut remainder = line;
        while !remainder.is_empty() {
            let total_w = measure(remainder)?;
            if total_w <= wrap_limit {
                result.push(emit(remainder.to_string(), total_w));
                break;
            }
            let mut break_idx: Option<usize> = None;
            let mut cursor = 0usize;
            for (idx, ch) in remainder.char_indices() {
                if ch == ' ' && idx > 0 {
                    let w = measure(&remainder[..idx])?;
                    if w <= wrap_limit {
                        break_idx = Some(idx);
                    } else {
                        break;
                    }
                }
                cursor = idx + ch.len_utf8();
            }
            let split = break_idx.unwrap_or(cursor);
            let (head, tail) = remainder.split_at(split);
            let head_w = measure(head)?;
            result.push(emit(head.to_string(), head_w));
            // Skip exactly one space (the break point) if present; otherwise
            // continue verbatim to avoid losing content when no break exists.
            remainder = tail.strip_prefix(' ').unwrap_or(tail);
            if remainder == line {
                break;
            }
        }
    }

    Ok(result)
}

/// Measure the intrinsic min and max content widths of a table cell.
/// Min width = width of the widest single word. Max width = width of the
/// content rendered on a single line.
fn measure_cell_intrinsic(cell: &TableCell) -> Result<(f32, f32), String> {
    let mut min_width: f32 = 0.0;
    let mut max_width: f32 = 0.0;
    for run in &cell.runs {
        let mut first_word = true;
        for word in run.text.split_whitespace() {
            let w = font_metrics::measure_str(word, &run.font_name, run.font_size)
                .ok_or_else(|| format!("unknown font: {}", run.font_name))?;
            if w > min_width {
                min_width = w;
            }
            if !first_word {
                let space_w =
                    font_metrics::measure_str(" ", &run.font_name, run.font_size).unwrap_or(0.0);
                max_width += space_w;
            }
            max_width += w;
            first_word = false;
        }
    }
    Ok((min_width, max_width))
}

/// Resolve a `<col>` width hint to a point value in the context of the
/// table's containing area. Percent hints resolve against `container`.
/// Other length units use a fallback `LengthContext` (rem/vw/vh inside
/// table column hints aren't yet contextual — see TODO).
fn resolve_col_hint(hint: Option<css::CssLength>, container: f32) -> Option<f32> {
    let len = hint?;
    let ctx = css::LengthContext {
        em: 12.0,
        rem: 12.0,
        vw: css::LengthContext::DEFAULT_VW,
        vh: css::LengthContext::DEFAULT_VH,
        container,
    };
    Some(len.resolve_ctx(&ctx))
}

/// Resolve `<col>` width hints to per-column point values (or `None`
/// where no hint was given). Percent hints use `container_width` as the
/// reference.
pub(crate) fn resolve_col_hints(
    col_styles: &[css::ColStyle],
    container_width: f32,
) -> Vec<Option<f32>> {
    col_styles
        .iter()
        .map(|cs| resolve_col_hint(cs.width, container_width))
        .collect()
}

/// CSS 2.1 §17.5.2 column-width resolution.
///
/// Two modes:
///
/// - **`auto`** (default): explicit column hints raise the column's
///   *preferred* width, but `col_min` is **left at the content
///   minimum**. This way a long unbreakable word (a URL, a hash) in a
///   column hinted to `width: 10%` still claims its real intrinsic
///   minimum and never clips. Per CSS 2.1 §17.5.2.2, also preserves the
///   invariant `col_max[i] >= col_preferred[i]`.
/// - **`fixed`**: column widths are pinned to their hints. Cells cannot
///   widen the column (overflow or wrap). Hint-less columns split the
///   leftover space evenly. CSS 2.1 §17.5.2.1.
///
/// Percent hints exceeding 100% in `auto` mode are scaled proportionally
/// down to fit so a malformed `[60%, 60%]` still uses the table's full
/// area instead of overflowing — matches Blink and WeasyPrint.
pub(crate) fn apply_col_hints_auto(
    col_min: &[f32],
    col_max: &mut [f32],
    col_preferred: &mut [f32],
    hints: &[Option<f32>],
) {
    debug_assert_eq!(col_min.len(), col_max.len());
    debug_assert_eq!(col_min.len(), col_preferred.len());
    debug_assert_eq!(col_min.len(), hints.len());
    for i in 0..col_min.len() {
        // `col_preferred[i]` starts at the intrinsic max-content; an
        // explicit hint overrides only if it's *larger* than the
        // content's min-width — content-min is the floor either way.
        if let Some(h) = hints[i] {
            let pref = h.max(col_min[i]);
            col_preferred[i] = pref;
            if col_max[i] < pref {
                col_max[i] = pref;
            }
        }
    }
}

/// `table-layout: fixed` — pin column widths to hints, even-split
/// leftover area among hint-less columns. Returns the resolved widths.
pub(crate) fn apply_col_hints_fixed(
    column_count: usize,
    hints: &[Option<f32>],
    container_width: f32,
) -> Vec<f32> {
    let mut widths = vec![0.0_f32; column_count];
    let mut hinted_total = 0.0_f32;
    let mut unhinted = 0_usize;
    for i in 0..column_count {
        match hints.get(i).copied().flatten() {
            Some(h) => {
                widths[i] = h;
                hinted_total += h;
            }
            None => unhinted += 1,
        }
    }
    if unhinted > 0 {
        let leftover = (container_width - hinted_total).max(0.0);
        let share = leftover / unhinted as f32;
        for i in 0..column_count {
            if hints.get(i).copied().flatten().is_none() {
                widths[i] = share;
            }
        }
    } else if hinted_total > container_width && hinted_total > 0.0 {
        // Total hints overflow the container — scale proportionally
        // so the table still fits. (CSS 2.1 §17.5.2.1 doesn't prescribe
        // this but every UA does it.)
        let scale = container_width / hinted_total;
        for w in &mut widths {
            *w *= scale;
        }
    }
    widths
}

/// Pre-process auto-mode percent hints so their sum doesn't exceed
/// `container_width`. Per Blink/WeasyPrint, `[60%, 60%]` resolves to
/// `[300pt, 300pt]` of a 600pt area, not `[360, 360]` overflowing.
/// Acts in-place on the resolved hint values.
pub(crate) fn scale_overlong_percent_hints(hints: &mut [Option<f32>], container_width: f32) {
    let total: f32 = hints.iter().filter_map(|h| *h).sum();
    if total > container_width && total > 0.0 {
        let scale = container_width / total;
        for h in hints.iter_mut() {
            if let Some(v) = h {
                *v *= scale;
            }
        }
    }
}

fn rgb_to_f32(c: (f64, f64, f64)) -> (f32, f32, f32) {
    (c.0 as f32, c.1 as f32, c.2 as f32)
}

fn resolve_margin_box_content(
    items: &[css::ContentItem],
    page_num: usize,
    total_pages: usize,
) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            css::ContentItem::String(s) => out.push_str(s),
            css::ContentItem::CounterPage => {
                out.push_str(&page_num.to_string());
            }
            css::ContentItem::CounterPages => {
                out.push_str(&total_pages.to_string());
            }
        }
    }
    out
}

fn resolve_margin_box_font(family: Option<&str>) -> String {
    if let Some(fam) = family {
        let first = fam.split(',').next().unwrap_or(fam).trim();
        let first = first.trim_matches(|c| c == '"' || c == '\'');
        let lower = first.to_ascii_lowercase();
        let resolved = match lower.as_str() {
            "helvetica" | "arial" | "sans-serif" => "Helvetica",
            "times" | "times new roman" | "serif" => "Times-Roman",
            "courier" | "courier new" | "monospace" => "Courier",
            _ => {
                if BUILTIN_FONTS.iter().any(|f| f.eq_ignore_ascii_case(first)) {
                    return first.to_string();
                }
                "Helvetica"
            }
        };
        return resolved.to_string();
    }
    "Helvetica".to_string()
}

fn heading_level(tag: &str) -> Option<u8> {
    match tag {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Paint `background-image` tiles into `page`, clipped to the padding box
/// `(pad_x, pad_y_bottom)` with size `(pad_w, pad_h)` (PDF coords: origin
/// bottom-left of the clip rect). Handles `background-size`,
/// `background-repeat`, and `background-position`.
fn emit_background_image_tiles(
    page: &mut PageContent,
    bg: &BackgroundImageParams,
    pad_x: f32,
    pad_y_bottom: f32,
    pad_w: f32,
    pad_h: f32,
) {
    // Resolve tile size.
    let intr_w = bg.intrinsic_width.max(1.0);
    let intr_h = bg.intrinsic_height.max(1.0);
    let (tile_w, tile_h) = match bg.size {
        css::BackgroundSize::Auto => (intr_w, intr_h),
        css::BackgroundSize::Cover => {
            let s = (pad_w / intr_w).max(pad_h / intr_h);
            (intr_w * s, intr_h * s)
        }
        css::BackgroundSize::Contain => {
            let s = (pad_w / intr_w).min(pad_h / intr_h);
            (intr_w * s, intr_h * s)
        }
        css::BackgroundSize::Length(w, h) => {
            // Percentages resolve against the padding box; `auto` on
            // either axis preserves the image's aspect ratio.
            let resolve_w = |l: css::CssLength| match l {
                css::CssLength::Pct(v) => v * pad_w / 100.0,
                other => other.resolve(12.0),
            };
            let resolve_h = |l: css::CssLength| match l {
                css::CssLength::Pct(v) => v * pad_h / 100.0,
                other => other.resolve(12.0),
            };
            let rw = w.map(resolve_w);
            let rh = h.map(resolve_h);
            match (rw, rh) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (w, intr_h * (w / intr_w)),
                (None, Some(h)) => (intr_w * (h / intr_h), h),
                (None, None) => (intr_w, intr_h),
            }
        }
    };
    if tile_w <= 0.0 || tile_h <= 0.0 {
        return;
    }

    // Resolve position. Per CSS 2.1 §14.2.1 a percentage value `p` places
    // the point `p` of the image at the point `p` of the padding box —
    // i.e. the tile's origin-x is `(pad_w - tile_w) * p`.
    let pos_x = match bg.position.x {
        css::CssLength::Pct(p) => (pad_w - tile_w) * p / 100.0,
        other => other.resolve(12.0),
    };
    let pos_y_from_top = match bg.position.y {
        css::CssLength::Pct(p) => (pad_h - tile_h) * p / 100.0,
        other => other.resolve(12.0),
    };
    // Convert CSS-top-anchored y to PDF-bottom-anchored y for the tile's
    // lower-left corner.
    let origin_x = pad_x + pos_x;
    let origin_y_bottom = pad_y_bottom + pad_h - pos_y_from_top - tile_h;

    // Tile counts per repeat mode.
    let (repeat_x, repeat_y) = match bg.repeat {
        css::BackgroundRepeat::Repeat => (true, true),
        css::BackgroundRepeat::NoRepeat => (false, false),
        css::BackgroundRepeat::RepeatX => (true, false),
        css::BackgroundRepeat::RepeatY => (false, true),
    };

    page.operations.push(PdfOp::SaveState);
    page.operations.push(PdfOp::Rectangle {
        x: pad_x,
        y: pad_y_bottom,
        width: pad_w,
        height: pad_h,
    });
    page.operations.push(PdfOp::ClipNonzero);

    if !page.images_used.contains(&bg.index) {
        page.images_used.push(bg.index);
    }

    // Compute how many tiles to emit on each axis, and the starting offset.
    let (x_start, x_count) = if repeat_x {
        // Find the leftmost x ≤ pad_x by stepping origin_x backward by
        // tile_w, then count forward until off the right edge.
        let steps_back = ((origin_x - pad_x) / tile_w).ceil().max(0.0);
        let start = origin_x - steps_back * tile_w;
        let count = (((pad_x + pad_w) - start) / tile_w).ceil() as i32;
        (start, count.max(0))
    } else {
        (origin_x, 1)
    };
    let (y_start, y_count) = if repeat_y {
        let tile_top_bottom = origin_y_bottom;
        let steps_back = ((tile_top_bottom - pad_y_bottom) / tile_h).ceil().max(0.0);
        let start = tile_top_bottom - steps_back * tile_h;
        let count = (((pad_y_bottom + pad_h) - start) / tile_h).ceil() as i32;
        (start, count.max(0))
    } else {
        (origin_y_bottom, 1)
    };

    for iy in 0..y_count {
        for ix in 0..x_count {
            let tx = x_start + ix as f32 * tile_w;
            let ty = y_start + iy as f32 * tile_h;
            page.operations.push(PdfOp::DrawImage {
                index: bg.index,
                x: tx,
                y: ty,
                width: tile_w,
                height: tile_h,
            });
        }
    }

    page.operations.push(PdfOp::RestoreState);
}

fn new_page(doc: &mut PdfDocument, width: f32, height: f32) -> Arc<Mutex<PageContent>> {
    let page = Arc::new(Mutex::new(PageContent::new(
        f64::from(width),
        f64::from(height),
    )));
    doc.pages.push(Arc::clone(&page));
    page
}

fn parse_text_align(text_align: &str) -> Result<TextAlign, PyErr> {
    match text_align {
        "left" => Ok(TextAlign::Left),
        "center" => Ok(TextAlign::Center),
        "right" => Ok(TextAlign::Right),
        "justify" => Ok(TextAlign::Justify),
        _ => Err(PyValueError::new_err(format!(
            "invalid text_align: {text_align} (expected left, center, or right)"
        ))),
    }
}

fn parse_padding(padding: Option<&Bound<'_, PyAny>>) -> Result<(f32, f32, f32, f32), PyErr> {
    match padding {
        None => Ok((0.0, 0.0, 0.0, 0.0)),
        Some(obj) => {
            if let Ok(val) = obj.extract::<f64>() {
                let v = val as f32;
                Ok((v, v, v, v))
            } else if let Ok((t, r, b, l)) = obj.extract::<(f64, f64, f64, f64)>() {
                Ok((t as f32, r as f32, b as f32, l as f32))
            } else {
                Err(PyValueError::new_err(
                    "padding must be a float or (top, right, bottom, left) tuple",
                ))
            }
        }
    }
}

// ── LayoutInner (non-PyO3) ────────────────────────────────────

/// A captured `position: fixed` block. Collected during the main
/// layout walk; the stored `ops` are stamped onto every page after the
/// normal flow finishes. `fonts` lists the font names the ops reference
/// so each page can register them before serialisation.
struct FixedBlock {
    ops: Vec<PdfOp>,
    fonts: Vec<String>,
}

pub struct LayoutInner {
    pub margin_top: f32,
    pub margin_right: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub page_width: f32,
    pub page_height: f32,
    blocks: Vec<Block>,
    pub column_count: u32,
    pub column_gap: f32,
    pub column_rule_width: f32,
    pub column_rule_color: Option<(f32, f32, f32)>,
    pub images: Vec<crate::image::ImageData>,
    /// `@page` margin boxes parsed from the stylesheet. Empty by default.
    /// Stamped onto every page after the main render loop completes so
    /// that `counter(pages)` can resolve to the final page count.
    pub margin_boxes: css::MarginBoxes,
    /// All `@page` rules parsed from the stylesheet (in source order).
    /// Used by `resolve_page_style_for` to compute the per-page width,
    /// height, margin, and margin-box state. CSS Paged Media L3 §4.4.
    pub page_rules: Vec<css::PageRule>,
    /// Collected `position: fixed` blocks. Stamped onto every page in
    /// `finish()` so the box appears across the full document.
    fixed_blocks: Vec<FixedBlock>,
    /// Embedded `@font-face` payloads collected during render. Drained
    /// into `PdfDocument.registered_fonts` at `finish()` time so the
    /// PDF emission pipeline subsets and embeds them like any other
    /// `register_font()`-registered font.
    pub font_face_registered: Vec<RegisteredFont>,
}

impl LayoutInner {
    pub fn new(
        margin_top: f32,
        margin_right: f32,
        margin_bottom: f32,
        margin_left: f32,
        page_width: f32,
        page_height: f32,
    ) -> Self {
        LayoutInner {
            margin_top,
            margin_right,
            margin_bottom,
            margin_left,
            page_width,
            page_height,
            blocks: Vec::new(),
            column_count: 1,
            column_gap: 0.0,
            column_rule_width: 0.0,
            column_rule_color: None,
            images: Vec::new(),
            margin_boxes: css::MarginBoxes::default(),
            page_rules: Vec::new(),
            fixed_blocks: Vec::new(),
            font_face_registered: Vec::new(),
        }
    }

    /// Resolve the effective `@page` style for `page_num` (1-indexed).
    /// Walks every `@page` rule that matches the page (per CSS Paged
    /// Media L3 §4.4) and merges declarations in ascending specificity
    /// order. `total_pages` is unused by every v1 pseudo (`:first`,
    /// `:left`, `:right`, `:nth(N)`, `:blank`) — it's accepted for
    /// future-compat with selectors that depend on the document's total
    /// length.
    pub fn resolve_page_style_for(&self, page_num: usize, total_pages: usize) -> css::PageStyle {
        let mut indexed: Vec<(usize, &css::PageRule)> = self
            .page_rules
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selector.matches(page_num, total_pages))
            .collect();
        indexed.sort_by_key(|(idx, r)| (r.selector.specificity(), *idx));
        let mut merged = css::PageStyle::default();
        for (_, rule) in indexed {
            css::merge_page_style_pub(&mut merged, &rule.decls);
        }
        merged
    }

    /// Apply the resolved per-page style to `self.margin_*` and
    /// `self.page_{width,height}`. Called immediately before opening a
    /// new page so the new page-frame is sized correctly. Returns the
    /// resolved style so callers (e.g. `stamp_margin_boxes`) can also
    /// pull the per-page margin-box declarations.
    pub fn apply_page_style_for(
        &mut self,
        page_num: usize,
        total_pages: usize,
    ) -> css::PageStyle {
        let resolved = self.resolve_page_style_for(page_num, total_pages);
        if let Some(w) = resolved.width {
            self.page_width = w;
        }
        if let Some(h) = resolved.height {
            self.page_height = h;
        }
        if let Some(m) = resolved.margin_top {
            self.margin_top = m;
        }
        if let Some(m) = resolved.margin_right {
            self.margin_right = m;
        }
        if let Some(m) = resolved.margin_bottom {
            self.margin_bottom = m;
        }
        if let Some(m) = resolved.margin_left {
            self.margin_left = m;
        }
        resolved
    }

    pub fn push_paragraph(&mut self, para: Paragraph) {
        if !para.white_space.preserves_whitespace() {
            let has_content = para
                .runs
                .iter()
                .any(|r| !r.text.trim().is_empty() || r.inline_block_width.is_some());
            if !has_content {
                return;
            }
        }
        self.blocks.push(Block::Paragraph(para));
    }

    pub fn push_table(&mut self, table: Table) {
        self.blocks.push(Block::Table(table));
    }

    pub fn push_image(&mut self, img: ImageBlock) {
        self.blocks.push(Block::Image(img));
    }

    /// Emit a container-start sentinel. Paired LIFO with `push_container_end`
    /// by `html_render` — the box tree refactor Stage 1 uses this to record
    /// a `<div>`'s margins so they can participate in parent/child collapse
    /// (CSS 2.1 § 8.3.1). The container itself does not paint; its padding,
    /// border, and background are currently dropped per the Stage 1 scope.
    pub fn push_container_start(&mut self, style: BlockStyle) {
        self.blocks.push(Block::ContainerStart(style));
    }

    pub fn push_container_end(&mut self, style: BlockStyle) {
        self.blocks.push(Block::ContainerEnd(style));
    }

    pub fn push_hr(&mut self) {
        self.blocks.push(Block::Paragraph(Paragraph {
            runs: vec![],
            line_height: None,
            spacing_after: 12.0,
            style: BlockStyle::default(),
            marker: None,
            is_hr: true,
            white_space: css::WhiteSpace::Normal,
            tag: None,
            anchor_id: None,
        }));
    }

    pub fn finish(&mut self, doc: &mut PdfDocument) -> Result<(), String> {
        if !self.font_face_registered.is_empty() {
            doc.registered_fonts.append(&mut self.font_face_registered);
        }

        // Apply per-page CSS resolution for page 1 before opening the
        // first page. Subsequent pages re-resolve at each break in
        // `advance_column_or_page`. CSS Paged Media L3 §4.4.
        if !self.page_rules.is_empty() {
            // `total_pages` is unused by every v1 pseudo-class.
            let _ = self.apply_page_style_for(1, 1);
        }

        let content_width = self.page_width - self.margin_left - self.margin_right;
        let content_top = self.page_height - self.margin_top;
        let blocks = std::mem::take(&mut self.blocks);

        let col_count = self.column_count.max(1);
        let (col_width, col_gap) = if col_count > 1 {
            let gap_total = (col_count - 1) as f32 * self.column_gap;
            (
                (content_width - gap_total) / col_count as f32,
                self.column_gap,
            )
        } else {
            (content_width, 0.0)
        };

        if blocks.is_empty() {
            new_page(doc, self.page_width, self.page_height);
            return Ok(());
        }

        let current_page = new_page(doc, self.page_width, self.page_height);
        let mut state = RenderState {
            col_count,
            col_width,
            col_gap,
            content_top,
            current_page,
            cursor_y: content_top,
            current_col: 0,
            pending_bottom: 0.0,
            pending_container_top: 0.0,
            active_floats: Vec::new(),
        };

        let tree = crate::box_tree::unflatten_blocks(blocks);
        self.render_nodes(doc, &tree, &mut state)?;

        self.draw_column_rules(
            &state.current_page,
            content_top,
            col_width,
            col_gap,
            col_count,
        );

        // Stamp @page margin boxes (headers/footers) onto every page.
        // Done after the main render so `counter(pages)` resolves to the
        // final page count.
        self.stamp_margin_boxes(doc);

        // Stamp `position: fixed` blocks onto every page. Captured ops
        // already include their SaveState/Translate/RestoreState wrapper,
        // so no additional scoping is needed.
        self.stamp_fixed_blocks(doc);

        Ok(())
    }

    fn col_x(&self, col: u32, state: &RenderState) -> f32 {
        self.margin_left + col as f32 * (state.col_width + state.col_gap)
    }

    /// Fold any pending container top margin into `pending_bottom` so leaf
    /// render paths see a single collapsed margin value. Advances the
    /// cursor by the delta between the old `pending_bottom` and the new
    /// collapsed value — matching the "already-spent" invariant
    /// `pending_bottom` represents.
    fn fold_container_top(state: &mut RenderState) {
        if state.pending_container_top == 0.0 {
            return;
        }
        let combined = collapse_margins(state.pending_bottom, state.pending_container_top);
        let delta = combined - state.pending_bottom;
        if delta > 0.0 {
            state.cursor_y -= delta;
        }
        state.pending_bottom = combined;
        state.pending_container_top = 0.0;
    }

    /// Recursive box-tree walker. Returns `true` if any leaf content was
    /// rendered for this sibling list — used by container close logic to
    /// decide between self-collapse (empty container) and parent/child
    /// margin-bottom collapsing (CSS 2.1 § 8.3.1).
    /// Width insets applied to a block at `cursor_y` because of active
    /// floats in the current column. Returns `(left_inset, right_inset)`
    /// — each in points. A block's usable text area shrinks by the sum
    /// and shifts right by `left_inset`.
    fn float_insets(state: &RenderState) -> (f32, f32) {
        let mut left: f32 = 0.0;
        let mut right: f32 = 0.0;
        for f in &state.active_floats {
            if f.column != state.current_col {
                continue;
            }
            // Active only while cursor is between top and bottom.
            if state.cursor_y <= f.top_y && state.cursor_y > f.bottom_y {
                match f.side {
                    css::FloatValue::Left => {
                        let w = f.right - f.left;
                        if w > left {
                            left = w;
                        }
                    }
                    css::FloatValue::Right => {
                        let w = f.right - f.left;
                        if w > right {
                            right = w;
                        }
                    }
                    css::FloatValue::None => {}
                }
            }
        }
        (left, right)
    }

    /// Honor `clear`: advance cursor past the bottom of matching active
    /// floats before laying out the block.
    fn apply_clear(state: &mut RenderState, clear: css::ClearValue) {
        let match_side = |side: css::FloatValue| match clear {
            css::ClearValue::Left => side == css::FloatValue::Left,
            css::ClearValue::Right => side == css::FloatValue::Right,
            css::ClearValue::Both => {
                side == css::FloatValue::Left || side == css::FloatValue::Right
            }
            css::ClearValue::None => false,
        };
        let mut target = state.cursor_y;
        for f in &state.active_floats {
            if f.column != state.current_col {
                continue;
            }
            if match_side(f.side) && f.bottom_y < target {
                target = f.bottom_y;
            }
        }
        if target < state.cursor_y {
            state.cursor_y = target;
        }
    }

    fn render_nodes(
        &mut self,
        doc: &mut PdfDocument,
        nodes: &[Node],
        state: &mut RenderState,
    ) -> Result<bool, String> {
        let mut any_content = false;
        for node in nodes {
            match node {
                Node::Block(bb) => {
                    if bb.is_hr {
                        self.render_hr_node(doc, bb, state);
                        any_content = true;
                        continue;
                    }
                    if let Some(anon) = crate::box_tree::paragraph_shape(bb) {
                        self.render_paragraph_node(doc, bb, anon, state)?;
                        any_content = true;
                        continue;
                    }
                    // Real container — recurse into children.
                    self.enter_container_node(doc, bb, state);
                    let child_rendered = self.render_nodes(doc, &bb.children, state)?;
                    self.exit_container_node(doc, bb, child_rendered, state);
                    if child_rendered {
                        any_content = true;
                    }
                }
                Node::Anonymous(_) => {
                    // Top-level anonymous boxes shouldn't exist with today's
                    // tree construction (paragraph_leaf always wraps them in
                    // a BlockBox). Nothing to do.
                }
                Node::Table(t) => {
                    Self::fold_container_top(state);
                    self.render_table_to_state(doc, &t.table, state)?;
                    any_content = true;
                }
                Node::Image(i) => {
                    Self::fold_container_top(state);
                    self.render_image_to_state(doc, &i.image, state);
                    any_content = true;
                }
            }
        }
        Ok(any_content)
    }

    /// Thin adapter from `RenderState` to the legacy `render_table` args.
    fn render_table_to_state(
        &mut self,
        doc: &mut PdfDocument,
        table: &Table,
        state: &mut RenderState,
    ) -> Result<(), String> {
        if let Some(cap) = &table.caption {
            let caption_node = crate::box_tree::Node::paragraph_leaf((**cap).clone());
            if let crate::box_tree::Node::Block(bb) = &caption_node
                && let Some(anon) = crate::box_tree::paragraph_shape(bb)
            {
                self.render_paragraph_node(doc, bb, anon, state)?;
            }
        }
        self.render_table(
            &mut state.current_page,
            doc,
            table,
            &mut state.cursor_y,
            &mut state.current_col,
            state.col_count,
            &mut state.col_width,
            &mut state.col_gap,
            &mut state.content_top,
            &mut state.pending_bottom,
        )
    }

    /// Thin adapter from `RenderState` to the legacy `render_image` args.
    fn render_image_to_state(
        &mut self,
        doc: &mut PdfDocument,
        img: &ImageBlock,
        state: &mut RenderState,
    ) {
        self.render_image(
            &mut state.current_page,
            doc,
            img,
            &mut state.cursor_y,
            &mut state.current_col,
            state.col_count,
            &mut state.col_width,
            &mut state.col_gap,
            &mut state.content_top,
            &mut state.pending_bottom,
        );
    }

    fn render_hr_node(&mut self, doc: &mut PdfDocument, bb: &BlockBox, state: &mut RenderState) {
        Self::fold_container_top(state);
        if state.cursor_y - 12.0 < self.margin_bottom && state.cursor_y < state.content_top {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
        }
        let cx = self.col_x(state.current_col, state);
        let cursor_y = state.cursor_y;
        let col_width = state.col_width;
        let mut page = state.current_page.lock().unwrap();
        page.operations.push(PdfOp::SaveState);
        page.operations.push(PdfOp::SetStrokeColor {
            r: 0.75,
            g: 0.75,
            b: 0.75,
        });
        page.operations.push(PdfOp::SetLineWidth(0.5));
        page.operations.push(PdfOp::MoveTo { x: cx, y: cursor_y });
        page.operations.push(PdfOp::LineTo {
            x: cx + col_width,
            y: cursor_y,
        });
        page.operations.push(PdfOp::Stroke);
        page.operations.push(PdfOp::RestoreState);
        drop(page);
        state.cursor_y -= bb.spacing_after;
    }

    fn enter_container_node(
        &mut self,
        doc: &mut PdfDocument,
        bb: &BlockBox,
        state: &mut RenderState,
    ) {
        if matches!(bb.page_break_before, Some(css::PageBreak::Always))
            && state.cursor_y < state.content_top
        {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
            state.pending_bottom = 0.0;
            state.pending_container_top = 0.0;
        }
        state.pending_container_top =
            collapse_margins(state.pending_container_top, bb.style.margin_top);
    }

    fn exit_container_node(
        &mut self,
        doc: &mut PdfDocument,
        bb: &BlockBox,
        child_rendered: bool,
        state: &mut RenderState,
    ) {
        // Non-empty containers: the child/parent margin_bottom collapse is
        // already partially accounted for by pending_bottom (the child's
        // trailing margin). Empty containers self-collapse top+bottom into
        // a single value that also folds with the surrounding flow.
        let contribution = if child_rendered {
            bb.style.margin_bottom
        } else {
            let c = collapse_margins(state.pending_container_top, bb.style.margin_bottom);
            state.pending_container_top = 0.0;
            c
        };
        let new_pending = collapse_margins(state.pending_bottom, contribution);
        let delta = new_pending - state.pending_bottom;
        if delta > 0.0 {
            state.cursor_y -= delta;
        }
        state.pending_bottom = new_pending;

        if matches!(bb.page_break_after, Some(css::PageBreak::Always)) {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
            state.pending_bottom = 0.0;
            state.pending_container_top = 0.0;
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_paragraph_node(
        &mut self,
        doc: &mut PdfDocument,
        bb: &BlockBox,
        anon: &AnonymousBox,
        state: &mut RenderState,
    ) -> Result<(), String> {
        let style = &bb.style;
        let marker = bb.marker.as_ref();

        if matches!(style.page_break_before, Some(css::PageBreak::Always))
            && state.cursor_y < state.content_top
        {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
            state.pending_container_top = 0.0;
        }

        let is_float = !matches!(style.float, css::FloatValue::None);
        let saved_cursor_y = state.cursor_y;
        let saved_pending_bottom = state.pending_bottom;

        Self::fold_container_top(state);

        if style.clear != css::ClearValue::None {
            Self::apply_clear(state, style.clear);
        }

        if !is_float && !doc.pages.is_empty() {
            let page_index = doc.pages.len() - 1;
            if let Some(tag) = bb.tag
                && let Some(level) = heading_level(tag)
            {
                let text: String = anon
                    .runs
                    .iter()
                    .map(|r| r.text.as_str())
                    .collect::<Vec<_>>()
                    .join("");
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    doc.headings.push(crate::HeadingEntry {
                        level,
                        text: trimmed,
                        page_index,
                        y: state.cursor_y,
                    });
                }
            }
            if let Some(id) = &bb.anchor_id {
                doc.anchors
                    .entry(id.clone())
                    .or_insert((page_index, state.cursor_y));
            }
        }

        // Compute left/right insets from floats active at this `cursor_y`
        // so in-flow blocks' text areas avoid intruding into them. Floats
        // themselves bypass this — they sit at the column edge.
        let (float_left, float_right) = if is_float {
            (0.0, 0.0)
        } else {
            Self::float_insets(state)
        };

        let h_padding = style.padding_left + style.padding_right;
        let h_border = style.border_width * 2.0;
        let content_adjust = match style.box_sizing {
            css::BoxSizing::ContentBox => 0.0,
            css::BoxSizing::BorderBox => h_padding + h_border,
        };
        let available_text_width = state.col_width
            - style.margin_left
            - style.margin_right
            - h_padding
            - float_left
            - float_right;
        let mut text_area_width = match style.width {
            Some(w) => (w - content_adjust).max(0.0),
            None if is_float => {
                // Shrink-to-fit — we don't actually have a proper
                // min-content measurement pass yet, so use a third of
                // the column as a reasonable default for floats.
                ((state.col_width / 3.0) - h_padding).max(0.0)
            }
            None => available_text_width,
        };
        if let Some(max_w) = style.max_width {
            text_area_width = text_area_width.min((max_w - content_adjust).max(0.0));
        }
        if let Some(min_w) = style.min_width {
            text_area_width = text_area_width.max((min_w - content_adjust).max(0.0));
        }
        if is_float {
            text_area_width = text_area_width.max(0.0);
        } else {
            text_area_width = text_area_width.min(available_text_width).max(0.0);
        }
        let box_width = text_area_width + h_padding;

        let spacing = SpacingOpts {
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
        };
        let text_indent = style.text_indent.max(0.0);
        let marker_gap = 6.0_f32;
        let marker_width_outside = marker.map_or(0.0, |m| {
            font_metrics::measure_str(&m.text, &m.font_name, m.font_size).unwrap_or(0.0)
        });
        let inside_indent = if matches!(style.list_style_position, css::ListStylePosition::Inside)
            && marker.is_some()
        {
            marker_width_outside + marker_gap
        } else {
            0.0
        };
        let wrap_width = (text_area_width - text_indent - inside_indent).max(0.0);
        let wrapped_lines = wrap_runs_impl(&anon.runs, wrap_width, anon.white_space, spacing)?;

        let max_font_size = wrapped_lines
            .iter()
            .map(|l| l.max_font_size)
            .fold(0.0_f32, f32::max);
        let line_height = anon.line_height.unwrap_or(max_font_size * 1.2);

        let block_text_height = wrapped_lines.len() as f32 * line_height;
        let natural_box_height = style.padding_top + block_text_height + style.padding_bottom;
        // Clamp to min/max-height. The content still top-aligns inside the
        // padded box — min-height extends the background/border rect down
        // but does not shift the text. (CSS actually lets height: auto
        // grow to content, so only min-height ever extends downward; we
        // still honor max-height by clipping the drawn rect — any text
        // that would have hung below the clamped edge just spills out.)
        let mut box_height = natural_box_height;
        if let Some(min_h) = style.min_height
            && box_height < min_h
        {
            box_height = min_h;
        }
        if let Some(max_h) = style.max_height
            && box_height > max_h
        {
            box_height = max_h;
        }
        let top_delta = collapsed_top_delta(state.pending_bottom, style.margin_top);
        let block_total_height = top_delta + box_height + style.margin_bottom;

        // Keep-together: by default (or with explicit `page-break-inside:
        // avoid`) push the whole block to the next page when it won't fit
        // in the remaining space. `page-break-inside: auto` opts out — the
        // block is laid out at the current cursor and may run off the
        // bottom (a known limitation until mid-paragraph splitting lands).
        // `orphans` and `widows` are carried on the style and trivially
        // honored: since we never split a paragraph across pages, all lines
        // always stay together on one page.
        let avoid_split = !matches!(style.page_break_inside, Some(css::PageBreakInside::Auto));
        if avoid_split
            && state.cursor_y - block_total_height < self.margin_bottom
            && state.cursor_y < state.content_top
        {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
            state.pending_bottom = 0.0;
        }
        // Silence dead-code warnings — these are parsed, propagated into
        // `BlockStyle`, and trivially honored (no splitting). They'll gain
        // real enforcement when mid-paragraph pagination is implemented.
        let _ = (style.orphans, style.widows);

        let cx = self.col_x(state.current_col, state);
        let box_x = if is_float {
            match style.float {
                css::FloatValue::Right => cx + state.col_width - box_width - style.margin_right,
                _ => cx + style.margin_left,
            }
        } else {
            cx + style.margin_left + float_left
        };
        if !is_float {
            state.cursor_y -= collapsed_top_delta(state.pending_bottom, style.margin_top);
        }

        let is_relative_positioned = style.position == css::Position::Relative
            && (style.position_top.is_some() || style.position_left.is_some());
        let is_absolute_or_fixed = matches!(
            style.position,
            css::Position::Absolute | css::Position::Fixed
        );
        // Absolute / fixed paint relative to the page (the initial
        // containing block). Resolve target page coords and derive the
        // translate delta from the box's natural (in-flow) position.
        let abs_translate = if is_absolute_or_fixed {
            let target_left_x = if let Some(l) = style.position_left {
                l
            } else if let Some(r) = style.position_right {
                self.page_width - r - box_width
            } else {
                box_x
            };
            let target_top_y = if let Some(t) = style.position_top {
                self.page_height - t
            } else if let Some(b) = style.position_bottom {
                b + box_height
            } else {
                state.cursor_y
            };
            Some((target_left_x - box_x, target_top_y - state.cursor_y))
        } else {
            None
        };
        let needs_state_wrap = style.has_any_styling()
            || style.overflow.clips()
            || is_relative_positioned
            || is_absolute_or_fixed
            || anon.runs.iter().any(|r| r.color.is_some());
        let cursor_y = state.cursor_y;
        let is_fixed = style.position == css::Position::Fixed;
        let mut page = state.current_page.lock().unwrap();
        let ops_capture_start = page.operations.len();
        let fonts_capture_start = page.fonts_used.len();

        for line in &wrapped_lines {
            for seg in &line.segments {
                if !page.fonts_used.contains(&seg.font_name) {
                    page.fonts_used.push(seg.font_name.clone());
                }
            }
        }

        if let Some(marker) = marker
            && !page.fonts_used.contains(&marker.font_name)
        {
            page.fonts_used.push(marker.font_name.clone());
        }

        if needs_state_wrap {
            page.operations.push(PdfOp::SaveState);
            // Apply opacity inside the save/restore so the alpha state is
            // reverted when the block finishes. Opacity is block-level for
            // now (no per-run inline opacity), and 1.0 is a no-op.
            if let Some(alpha) = style.opacity
                && alpha < 1.0
            {
                page.operations.push(PdfOp::SetAlpha { alpha });
            }
            // CSS 2.1 §9.4.3: `position: relative` shifts the painted box
            // by (+left, -top) without disturbing flow. Emit the translation
            // inside the save/restore so siblings are unaffected.
            if is_relative_positioned {
                let dx = style.position_left.unwrap_or(0.0);
                let dy = -style.position_top.unwrap_or(0.0);
                if dx != 0.0 || dy != 0.0 {
                    page.operations.push(PdfOp::Translate { dx, dy });
                }
            }
            // CSS 2.1 §9.6: `position: absolute` / `fixed` paint at a
            // page-relative target computed above. The box retains its
            // natural layout metrics — only the paint origin shifts.
            if let Some((dx, dy)) = abs_translate
                && (dx != 0.0 || dy != 0.0)
            {
                page.operations.push(PdfOp::Translate { dx, dy });
            }
        }

        // Helper for emitting a rectangle path that is either sharp or
        // rounded depending on `style.border_radius`.
        let emit_rect_path =
            |ops: &mut Vec<PdfOp>, x: f32, y: f32, w: f32, h: f32| match style.border_radius {
                Some(radii) => ops.push(PdfOp::RoundedRectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                    radii,
                }),
                None => ops.push(PdfOp::Rectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                }),
            };

        // CSS Backgrounds & Borders 3 §7: outset `box-shadow` layers paint
        // behind the box (below its background). Spec paint order is back-
        // to-front so the first-declared layer lands on top — iterate the
        // list in reverse. Blur is parsed but not rendered yet: shadows
        // come out sharp (documented limitation; a soft-mask emitter would
        // be required for a true gaussian blur).
        for sh in style.box_shadow.iter().rev().filter(|s| !s.inset) {
            let (r, g, b) = sh.color;
            let shadow_w = box_width + 2.0 * sh.spread;
            let shadow_h = box_height + 2.0 * sh.spread;
            if shadow_w <= 0.0 || shadow_h <= 0.0 {
                continue;
            }
            let shadow_x = box_x + sh.offset_x - sh.spread;
            let shadow_y = cursor_y - box_height - sh.offset_y - sh.spread;
            page.operations.push(PdfOp::SetFillColor { r, g, b });
            emit_rect_path(&mut page.operations, shadow_x, shadow_y, shadow_w, shadow_h);
            page.operations.push(PdfOp::Fill);
        }

        if let Some((r, g, b)) = style.background_color {
            page.operations.push(PdfOp::SetFillColor { r, g, b });
            emit_rect_path(
                &mut page.operations,
                box_x,
                cursor_y - box_height,
                box_width,
                box_height,
            );
            page.operations.push(PdfOp::Fill);
        }

        if style.border_width > 0.0 && !matches!(style.border_style, Some(css::BorderStyle::None)) {
            let (r, g, b) = style.border_color.unwrap_or((0.0, 0.0, 0.0));
            page.operations.push(PdfOp::SetStrokeColor { r, g, b });
            page.operations
                .push(PdfOp::SetLineWidth(style.border_width));

            match style.border_style {
                Some(css::BorderStyle::Dashed) => {
                    let dash = style.border_width * 3.0;
                    let gap = style.border_width * 2.0;
                    page.operations.push(PdfOp::SetDashPattern {
                        array: vec![dash, gap],
                        phase: 0.0,
                    });
                }
                Some(css::BorderStyle::Dotted) => {
                    let dot = style.border_width;
                    page.operations.push(PdfOp::SetDashPattern {
                        array: vec![dot, dot * 2.0],
                        phase: 0.0,
                    });
                }
                _ => {}
            }

            emit_rect_path(
                &mut page.operations,
                box_x,
                cursor_y - box_height,
                box_width,
                box_height,
            );
            page.operations.push(PdfOp::Stroke);
        }

        // CSS Backgrounds & Borders 3 §7: inset shadows paint inside the
        // padding box, above the background and border but below content.
        // The filled region is an annulus between the padding box and a
        // cutout rectangle shifted by (offset_x, offset_y) and shrunk by
        // `spread`. Emitted as two rectangle subpaths filled with the
        // even-odd rule. Clipped to the padding box so the shadow never
        // paints outside the element.
        let inset_shadows_present = style.box_shadow.iter().any(|s| s.inset);
        if inset_shadows_present {
            let pad_x = box_x + style.border_width;
            let pad_y = cursor_y - box_height + style.border_width;
            let pad_w = (box_width - 2.0 * style.border_width).max(0.0);
            let pad_h = (box_height - 2.0 * style.border_width).max(0.0);
            if pad_w > 0.0 && pad_h > 0.0 {
                page.operations.push(PdfOp::SaveState);
                page.operations.push(PdfOp::Rectangle {
                    x: pad_x,
                    y: pad_y,
                    width: pad_w,
                    height: pad_h,
                });
                page.operations.push(PdfOp::ClipNonzero);
                for sh in style.box_shadow.iter().rev().filter(|s| s.inset) {
                    let (r, g, b) = sh.color;
                    // Cutout rectangle (the unfilled hole). CSS y grows
                    // down, so a positive offset_y shifts the cutout down
                    // which in PDF coordinates means subtracting from y.
                    let cutout_w = (pad_w - 2.0 * sh.spread).max(0.0);
                    let cutout_h = (pad_h - 2.0 * sh.spread).max(0.0);
                    let cutout_x = pad_x + sh.offset_x + sh.spread;
                    let cutout_y = pad_y - sh.offset_y + sh.spread;
                    page.operations.push(PdfOp::SetFillColor { r, g, b });
                    page.operations.push(PdfOp::Rectangle {
                        x: pad_x,
                        y: pad_y,
                        width: pad_w,
                        height: pad_h,
                    });
                    page.operations.push(PdfOp::Rectangle {
                        x: cutout_x,
                        y: cutout_y,
                        width: cutout_w,
                        height: cutout_h,
                    });
                    page.operations.push(PdfOp::FillEvenOdd);
                }
                page.operations.push(PdfOp::RestoreState);
            }
        }

        // CSS 2.1 §14.2.1: paint `background-image` tiles over the
        // padding box. Clipped to the padding box via its own save/clip/
        // restore bracket so tiles never leak out of the block even when
        // `overflow: visible`, and so later content (text, borders of
        // siblings) is unaffected by the clip.
        if let Some(bg_img) = &style.background_image {
            let pad_x = box_x + style.border_width;
            let pad_y_bottom = cursor_y - box_height + style.border_width;
            let pad_w = (box_width - 2.0 * style.border_width).max(0.0);
            let pad_h = (box_height - 2.0 * style.border_width).max(0.0);
            if pad_w > 0.0 && pad_h > 0.0 {
                emit_background_image_tiles(&mut page, bg_img, pad_x, pad_y_bottom, pad_w, pad_h);
            }
        }

        // CSS 2.1 §11.1: `overflow: hidden` (and scroll/auto in a paged
        // medium) clip content to the padding edge. Emit the clip path
        // after drawing the box's own background + border so those stay
        // visible, but before any children. The clip is scoped to the
        // SaveState/RestoreState pair (`needs_state_wrap` already set).
        if style.overflow.clips() {
            let pad_x = box_x + style.border_width;
            let pad_y = cursor_y - box_height + style.border_width;
            let pad_w = (box_width - 2.0 * style.border_width).max(0.0);
            let pad_h = (box_height - 2.0 * style.border_width).max(0.0);
            page.operations.push(PdfOp::Rectangle {
                x: pad_x,
                y: pad_y,
                width: pad_w,
                height: pad_h,
            });
            page.operations.push(PdfOp::ClipNonzero);
        }

        if let Some((r, g, b)) = style.color {
            page.operations.push(PdfOp::SetFillColor { r, g, b });
        }

        let text_x_base = box_x + style.padding_left;
        let mut line_y = cursor_y - style.padding_top;
        let is_inside_marker =
            matches!(style.list_style_position, css::ListStylePosition::Inside) && marker.is_some();

        for (line_idx, line) in wrapped_lines.iter().enumerate() {
            let baseline_y = line_y - line.max_font_size;

            if line_idx == 0
                && let Some(marker) = marker
            {
                let marker_width =
                    font_metrics::measure_str(&marker.text, &marker.font_name, marker.font_size)
                        .unwrap_or(0.0);
                let marker_x = if is_inside_marker {
                    text_x_base
                } else {
                    text_x_base - marker_gap - marker_width
                };

                if let Some((r, g, b)) = marker.color {
                    page.operations.push(PdfOp::SetFillColor { r, g, b });
                }
                page.operations.push(PdfOp::BeginText);
                page.operations.push(PdfOp::SetFont {
                    name: marker.font_name.clone(),
                    size: marker.font_size,
                });
                page.operations.push(PdfOp::SetTextPosition {
                    x: marker_x,
                    y: baseline_y,
                });
                page.operations
                    .push(show_text_for(&marker.font_name, marker.text.clone()));
                page.operations.push(PdfOp::EndText);
            }

            let is_justified_line = matches!(style.text_align, TextAlign::Justify)
                && line_idx + 1 < wrapped_lines.len();

            let justify_word_spacing = if is_justified_line {
                let space_count: usize = line
                    .segments
                    .iter()
                    .map(|s| s.text.matches(' ').count())
                    .sum();
                if space_count > 0 {
                    (text_area_width - line.total_width) / space_count as f32
                } else {
                    0.0
                }
            } else {
                0.0
            };
            let total_word_spacing = style.word_spacing + justify_word_spacing;

            let inside_indent_for_line = if line_idx == 0 { inside_indent } else { 0.0 };
            let indent_for_line =
                if line_idx == 0 { text_indent } else { 0.0 } + inside_indent_for_line;
            let align_offset = match style.text_align {
                TextAlign::Left | TextAlign::Justify => indent_for_line,
                TextAlign::Center => {
                    indent_for_line + (text_area_width - indent_for_line - line.total_width) / 2.0
                }
                TextAlign::Right => text_area_width - line.total_width,
            };

            let needs_tw = total_word_spacing != 0.0;
            let needs_tc = style.letter_spacing != 0.0;
            if needs_tw {
                page.operations
                    .push(PdfOp::SetWordSpacing(total_word_spacing));
            }
            if needs_tc {
                page.operations
                    .push(PdfOp::SetCharacterSpacing(style.letter_spacing));
            }

            let mut x = text_x_base + align_offset;

            for segment in &line.segments {
                if let Some((r, g, b)) = segment.color {
                    page.operations.push(PdfOp::SetFillColor { r, g, b });
                }

                let seg_y = baseline_y + segment.baseline_shift;

                // Inline-block atoms paint a background rect and/or border
                // first, then the text is drawn at the atom's left edge
                // (plus padding). The leading space width is already folded
                // into `segment.width`, so the atom box starts at
                // `x + leading_space`. We recover the atom width from
                // `inline_block_width` directly.
                let (atom_x, text_offset_x) = if let Some(atom_w) = segment.inline_block_width {
                    let atom_x = x + (segment.width - atom_w);
                    // Paint background.
                    if let Some((r, g, b)) = segment.inline_block_bg {
                        page.operations.push(PdfOp::SaveState);
                        page.operations.push(PdfOp::SetFillColor { r, g, b });
                        page.operations.push(PdfOp::Rectangle {
                            x: atom_x,
                            y: seg_y - segment.font_size * 0.2,
                            width: atom_w,
                            height: segment.font_size * 1.2,
                        });
                        page.operations.push(PdfOp::Fill);
                        page.operations.push(PdfOp::RestoreState);
                        // Re-apply text color since SaveState did not
                        // preserve the previously set fill color in our
                        // op stream model (SaveState does preserve it
                        // in PDF, but we re-set to be safe).
                        if let Some((r, g, b)) = segment.color {
                            page.operations.push(PdfOp::SetFillColor { r, g, b });
                        }
                    }
                    if let Some((bw, (br, bg_, bb))) = segment.inline_block_border {
                        page.operations.push(PdfOp::SaveState);
                        page.operations.push(PdfOp::SetStrokeColor {
                            r: br,
                            g: bg_,
                            b: bb,
                        });
                        page.operations.push(PdfOp::SetLineWidth(bw));
                        page.operations.push(PdfOp::Rectangle {
                            x: atom_x,
                            y: seg_y - segment.font_size * 0.2,
                            width: atom_w,
                            height: segment.font_size * 1.2,
                        });
                        page.operations.push(PdfOp::Stroke);
                        page.operations.push(PdfOp::RestoreState);
                        if let Some((r, g, b)) = segment.color {
                            page.operations.push(PdfOp::SetFillColor { r, g, b });
                        }
                    }
                    (atom_x, segment.inline_block_padding_x)
                } else {
                    (x, 0.0)
                };

                page.operations.push(PdfOp::BeginText);
                page.operations.push(PdfOp::SetFont {
                    name: segment.font_name.clone(),
                    size: segment.font_size,
                });
                page.operations.push(PdfOp::SetTextPosition {
                    x: atom_x + text_offset_x,
                    y: seg_y,
                });
                page.operations
                    .push(show_text_for(&segment.font_name, segment.text.clone()));
                page.operations.push(PdfOp::EndText);

                if let Some(url) = &segment.link_url {
                    page.links.push(crate::LinkAnnotation {
                        x,
                        y: seg_y - segment.font_size * 0.2,
                        width: segment.width,
                        height: segment.font_size * 1.1,
                        url: url.clone(),
                    });
                }

                if let Some(td) = segment.text_decoration
                    && (td.underline || td.line_through)
                {
                    let metrics = font_metrics::get_builtin_metrics(&segment.font_name);
                    let scale = segment.font_size / 1000.0;
                    let stroke_width = (segment.font_size * 0.05).max(0.5);

                    page.operations.push(PdfOp::SaveState);
                    if let Some((r, g, b)) = segment.color {
                        page.operations.push(PdfOp::SetStrokeColor { r, g, b });
                    }
                    page.operations.push(PdfOp::SetLineWidth(stroke_width));

                    if td.underline {
                        let descent = metrics.map_or(-207.0, |m| f32::from(m.descent));
                        let underline_y = baseline_y + descent * scale / 3.0;
                        page.operations.push(PdfOp::MoveTo { x, y: underline_y });
                        page.operations.push(PdfOp::LineTo {
                            x: x + segment.width,
                            y: underline_y,
                        });
                        page.operations.push(PdfOp::Stroke);
                    }

                    if td.line_through {
                        let ascent = metrics.map_or(718.0, |m| f32::from(m.ascent));
                        let strike_y = baseline_y + ascent * scale / 3.0;
                        page.operations.push(PdfOp::MoveTo { x, y: strike_y });
                        page.operations.push(PdfOp::LineTo {
                            x: x + segment.width,
                            y: strike_y,
                        });
                        page.operations.push(PdfOp::Stroke);
                    }

                    page.operations.push(PdfOp::RestoreState);
                }

                let spaces_in_segment = segment.text.matches(' ').count() as f32;
                x += segment.width + spaces_in_segment * justify_word_spacing;
            }

            if needs_tw {
                page.operations.push(PdfOp::SetWordSpacing(0.0));
            }
            if needs_tc {
                page.operations.push(PdfOp::SetCharacterSpacing(0.0));
            }

            line_y -= line_height;
        }

        if needs_state_wrap {
            page.operations.push(PdfOp::RestoreState);
        }

        drop(page);
        if is_float {
            // Record the float and restore the cursor so subsequent
            // in-flow content flows past it rather than below it.
            let top_y = state.cursor_y;
            let bottom_y = top_y - box_height - style.margin_bottom;
            state.active_floats.push(ActiveFloat {
                side: style.float,
                left: box_x - style.padding_left,
                right: box_x + box_width + style.padding_right,
                top_y,
                bottom_y,
                column: state.current_col,
            });
            state.cursor_y = saved_cursor_y;
            state.pending_bottom = saved_pending_bottom;
            return Ok(());
        }
        if is_absolute_or_fixed {
            // Out of flow: siblings must see the cursor/pending_bottom
            // exactly as it was on entry. For `fixed`, pull the captured
            // ops off the current page and stash them for per-page replay
            // in `finish()`.
            if is_fixed {
                let mut page = state.current_page.lock().unwrap();
                let captured_ops = page.operations.split_off(ops_capture_start);
                let captured_fonts: Vec<String> = page.fonts_used[fonts_capture_start..].to_vec();
                drop(page);
                self.fixed_blocks.push(FixedBlock {
                    ops: captured_ops,
                    fonts: captured_fonts,
                });
            }
            state.cursor_y = saved_cursor_y;
            state.pending_bottom = saved_pending_bottom;
            return Ok(());
        }
        state.cursor_y -= box_height + style.margin_bottom + anon.spacing_after;
        state.pending_bottom = style.margin_bottom;

        if matches!(style.page_break_after, Some(css::PageBreak::Always)) {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                &mut state.col_width,
                &mut state.col_gap,
                &mut state.content_top,
            );
            state.pending_bottom = 0.0;
        }

        Ok(())
    }

    fn stamp_fixed_blocks(&self, doc: &mut PdfDocument) {
        if self.fixed_blocks.is_empty() {
            return;
        }
        for page_arc in &doc.pages {
            let mut page = page_arc.lock().unwrap();
            for fb in &self.fixed_blocks {
                for font in &fb.fonts {
                    if !page.fonts_used.contains(font) {
                        page.fonts_used.push(font.clone());
                    }
                }
                page.operations.extend(fb.ops.iter().cloned());
            }
        }
    }

    /// Iterate `doc.pages` and render any `@page` margin boxes onto each
    /// one. Uses the 1-indexed page number for `counter(page)` and the
    /// total page count for `counter(pages)`. Each of the six supported
    /// positions (top/bottom × left/center/right) gets a single baseline
    /// of text inside the printable margin strip.
    ///
    /// Per CSS Paged Media L3 §4.4 the margin-box content cascades
    /// per-property across `@page` rules — `@page :first { @top-left { … } }`
    /// only overlays the properties it sets, leaving the rest of
    /// `@top-left`'s properties from the base `@page` intact. This is
    /// achieved by calling `resolve_page_style_for(page_num)` per page
    /// and reading the merged margin boxes from the result.
    fn stamp_margin_boxes(&mut self, doc: &mut PdfDocument) {
        // If there are no `@page` rules, we may still have a top-level
        // `margin_boxes` field set by a non-CSS API consumer — fall back
        // to the universal slot in that case.
        let has_any_rule_box = self
            .page_rules
            .iter()
            .any(|r| r.decls.margin_boxes.any());
        if !has_any_rule_box && !self.margin_boxes.any() {
            return;
        }
        let total_pages = doc.pages.len();
        // Snapshot pages from `doc.pages` length so we don't borrow
        // `doc` mutably and immutably at once. We resolve the per-page
        // boxes ahead of the loop because `resolve_page_style_for`
        // reads `self.page_rules` which can't overlap a mut borrow on
        // `self.margin_boxes`/`self.render_margin_box`.
        let resolved: Vec<css::MarginBoxes> = (1..=total_pages)
            .map(|page_num| {
                if self.page_rules.is_empty() {
                    self.margin_boxes.clone()
                } else {
                    self.resolve_page_style_for(page_num, total_pages)
                        .margin_boxes
                }
            })
            .collect();
        for (idx, page_arc) in doc.pages.iter().enumerate() {
            let page_num = idx + 1;
            let boxes = &resolved[idx];
            if !boxes.any() {
                continue;
            }
            // The page-frame size and margins for *this* page are what
            // determine where the margin-strip lives; refresh
            // `self.{page_width,page_height,margin_*}` for this page
            // before rendering so the strip math lines up. (No-op if
            // `page_rules` is empty.)
            if !self.page_rules.is_empty() {
                let _ = self.apply_page_style_for(page_num, total_pages);
            }
            let mut page = page_arc.lock().unwrap();
            let positions = [
                (&boxes.top_left, css::MarginBoxPosition::TopLeft),
                (&boxes.top_center, css::MarginBoxPosition::TopCenter),
                (&boxes.top_right, css::MarginBoxPosition::TopRight),
                (&boxes.bottom_left, css::MarginBoxPosition::BottomLeft),
                (&boxes.bottom_center, css::MarginBoxPosition::BottomCenter),
                (&boxes.bottom_right, css::MarginBoxPosition::BottomRight),
            ];
            for (slot, pos) in positions {
                if let Some(mb) = slot.as_ref() {
                    self.render_margin_box(&mut page, mb, pos, page_num, total_pages);
                }
            }
        }
    }

    /// Render a single margin box onto `page`. `page_num` is 1-indexed;
    /// `total_pages` is the total page count. Text is single-line and
    /// baseline-positioned; overflow is allowed (see the scope note in
    /// the plan).
    fn render_margin_box(
        &self,
        page: &mut PageContent,
        mb: &css::MarginBox,
        pos: css::MarginBoxPosition,
        page_num: usize,
        total_pages: usize,
    ) {
        // Resolve the content string.
        let text = resolve_margin_box_content(&mb.content, page_num, total_pages);
        if text.is_empty() {
            return;
        }
        // Resolve font / size / color with defaults.
        let font_size = mb.font_size.unwrap_or(10.0);
        let font_name = resolve_margin_box_font(mb.font_family.as_deref());
        let color = mb.color.unwrap_or((0.0, 0.0, 0.0));

        // Compute the box's horizontal strip inside the printable area.
        let printable_left = self.margin_left;
        let printable_right = self.page_width - self.margin_right;
        let printable_width = (printable_right - printable_left).max(0.0);
        let col_w = printable_width / 3.0;
        let (box_left, default_align) = match pos {
            css::MarginBoxPosition::TopLeft | css::MarginBoxPosition::BottomLeft => {
                (printable_left, TextAlign::Left)
            }
            css::MarginBoxPosition::TopCenter | css::MarginBoxPosition::BottomCenter => {
                (printable_left + col_w, TextAlign::Center)
            }
            css::MarginBoxPosition::TopRight | css::MarginBoxPosition::BottomRight => {
                (printable_left + 2.0 * col_w, TextAlign::Right)
            }
        };
        let align = mb.text_align.clone().unwrap_or(default_align);

        // Horizontal position based on alignment.
        let text_width = font_metrics::measure_str(&text, &font_name, font_size).unwrap_or(0.0);
        let x = match align {
            TextAlign::Left | TextAlign::Justify => box_left,
            TextAlign::Center => box_left + (col_w - text_width) / 2.0,
            TextAlign::Right => box_left + col_w - text_width,
        };

        // Vertical baseline. Top boxes sit in the top margin strip with
        // their baseline at (page_height - margin_top/2 - font_size/3);
        // this is a rough "centered within the margin" placement that
        // matches WeasyPrint's default for small headers. Bottom boxes
        // mirror it from zero.
        let y = match pos {
            css::MarginBoxPosition::TopLeft
            | css::MarginBoxPosition::TopCenter
            | css::MarginBoxPosition::TopRight => {
                let strip_center = self.page_height - (self.margin_top / 2.0);
                strip_center - font_size / 3.0
            }
            css::MarginBoxPosition::BottomLeft
            | css::MarginBoxPosition::BottomCenter
            | css::MarginBoxPosition::BottomRight => {
                let strip_center = self.margin_bottom / 2.0;
                strip_center - font_size / 3.0
            }
        };

        // Register the font so the page writer emits the right resource
        // dictionary entry.
        if !page.fonts_used.contains(&font_name) {
            page.fonts_used.push(font_name.clone());
        }

        page.operations.push(PdfOp::SetFillColor {
            r: color.0,
            g: color.1,
            b: color.2,
        });
        page.operations.push(PdfOp::BeginText);
        page.operations.push(PdfOp::SetFont {
            name: font_name.clone(),
            size: font_size,
        });
        page.operations.push(PdfOp::SetTextPosition { x, y });
        page.operations.push(show_text_for(&font_name, text));
        page.operations.push(PdfOp::EndText);
        // Restore default fill color to black so later ops (if any) see
        // a predictable state. Since margin boxes are stamped last, this
        // is mostly defensive.
        page.operations.push(PdfOp::SetFillColor {
            r: 0.0,
            g: 0.0,
            b: 0.0,
        });
    }

    #[allow(clippy::too_many_arguments, clippy::items_after_statements)]
    fn render_table(
        &mut self,
        current_page: &mut Arc<Mutex<PageContent>>,
        doc: &mut PdfDocument,
        table: &Table,
        cursor_y: &mut f32,
        current_col: &mut u32,
        col_count: u32,
        col_width: &mut f32,
        col_gap: &mut f32,
        content_top: &mut f32,
        pending_bottom: &mut f32,
    ) -> Result<(), String> {
        if table.rows.is_empty() {
            return Ok(());
        }
        let column_count = table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        if column_count == 0 {
            return Ok(());
        }

        // Snapshot the column geometry at the start of the table — if a
        // page-break advances us mid-table, `advance_column_or_page`
        // will have rewritten the caller's geometry to the new page's,
        // but the table's intrinsic-width math wants the geometry it
        // started with. (Spanning a table across pages with mismatched
        // margins is a v1 cut; tracked separately.)
        let col_width_v: f32 = *col_width;
        let col_gap_v: f32 = *col_gap;
        let content_top_v: f32 = *content_top;
        // Snapshot `margin_left` so the col-position closure doesn't
        // hold an immutable borrow of `self` across the mutable borrow
        // we need when calling `advance_column_or_page` mid-table.
        let margin_left_v: f32 = self.margin_left;

        // column_width helper (reused)
        let col_x_for =
            |col: u32| -> f32 { margin_left_v + col as f32 * (col_width_v + col_gap_v) };

        // Total available width for the table within the current column
        let table_area_width = col_width_v - table.style.margin_left - table.style.margin_right;

        // Measure intrinsic min and max widths per column
        let mut col_min = vec![0.0_f32; column_count];
        let mut col_max = vec![0.0_f32; column_count];
        for row in &table.rows {
            for (idx, cell) in row.cells.iter().enumerate() {
                if idx >= column_count {
                    break;
                }
                let (cmin, cmax) = measure_cell_intrinsic(cell)?;
                let h_pad = cell.style.padding_left + cell.style.padding_right;
                if cmin + h_pad > col_min[idx] {
                    col_min[idx] = cmin + h_pad;
                }
                if cmax + h_pad > col_max[idx] {
                    col_max[idx] = cmax + h_pad;
                }
            }
        }

        // Distribute widths: use max if they fit; else scale proportionally down to min
        let sum_max: f32 = col_max.iter().sum();
        let sum_min: f32 = col_min.iter().sum();
        let col_widths: Vec<f32> = if sum_max <= table_area_width {
            // Space to spare: distribute the extra proportionally from max
            let extra = table_area_width - sum_max;
            if sum_max > 0.0 {
                col_max.iter().map(|&w| w + extra * (w / sum_max)).collect()
            } else {
                vec![table_area_width / column_count as f32; column_count]
            }
        } else if sum_min >= table_area_width {
            col_min.clone()
        } else {
            // Interpolate between min and max
            let slack = table_area_width - sum_min;
            let range = sum_max - sum_min;
            col_min
                .iter()
                .zip(col_max.iter())
                .map(|(&lo, &hi)| lo + slack * ((hi - lo) / range))
                .collect()
        };

        let table_actual_width: f32 = col_widths.iter().sum();
        let _ = table_actual_width;

        *cursor_y -= collapsed_top_delta(*pending_bottom, table.style.margin_top);
        *pending_bottom = 0.0;

        let is_collapse = matches!(table.border_collapse, css::BorderCollapseValue::Collapse);

        // In collapse mode we batch internal + outer borders per-band
        // (a "band" is a contiguous stretch of rows on the same page/column)
        // and flush them when the band ends — either on a page break or at
        // the end of the table.
        struct CollapseBand {
            x0: f32,
            top_y: f32,
            row_bottoms: Vec<f32>,
            // Max border width and first non-None color encountered in the band.
            max_border_width: f32,
            border_color: Option<(f32, f32, f32)>,
            has_border: bool,
        }
        let mut collapse_band: Option<CollapseBand> = None;

        // Flushes the currently accumulated band's collapse borders to `page`.
        // Draws the outer rectangle once, internal vertical gridlines at each
        // column boundary, and internal horizontal gridlines at each row
        // boundary.
        fn flush_collapse_band(page: &mut PageContent, band: &CollapseBand, col_widths: &[f32]) {
            if !band.has_border || band.row_bottoms.is_empty() {
                return;
            }
            let (r, g, b) = band.border_color.unwrap_or((0.0, 0.0, 0.0));
            let w = band.max_border_width;
            let total_w: f32 = col_widths.iter().sum();
            let bottom_y = *band
                .row_bottoms
                .last()
                .expect("row_bottoms non-empty (checked above)");
            let height = band.top_y - bottom_y;
            page.operations.push(PdfOp::SetStrokeColor { r, g, b });
            page.operations.push(PdfOp::SetLineWidth(w));
            // Outer rectangle
            page.operations.push(PdfOp::Rectangle {
                x: band.x0,
                y: bottom_y,
                width: total_w,
                height,
            });
            page.operations.push(PdfOp::Stroke);
            // Internal vertical gridlines (skip the last boundary — that's
            // the right edge of the outer rectangle)
            let mut gx = band.x0;
            for cw in col_widths.iter().take(col_widths.len().saturating_sub(1)) {
                gx += cw;
                page.operations.push(PdfOp::MoveTo {
                    x: gx,
                    y: band.top_y,
                });
                page.operations.push(PdfOp::LineTo { x: gx, y: bottom_y });
                page.operations.push(PdfOp::Stroke);
            }
            // Internal horizontal gridlines — every row boundary except
            // the band's final bottom edge (already drawn as part of the
            // outer rectangle).
            for &rb in &band.row_bottoms[..band.row_bottoms.len() - 1] {
                page.operations.push(PdfOp::MoveTo { x: band.x0, y: rb });
                page.operations.push(PdfOp::LineTo {
                    x: band.x0 + total_w,
                    y: rb,
                });
                page.operations.push(PdfOp::Stroke);
            }
        }

        for row in &table.rows {
            // Wrap each cell's content at its column width and compute text heights
            let mut cell_text_heights: Vec<f32> = Vec::with_capacity(row.cells.len());
            let mut wrapped_cells: Vec<Vec<WrappedLine>> = Vec::with_capacity(row.cells.len());
            let mut cell_line_heights: Vec<f32> = Vec::with_capacity(row.cells.len());

            for (idx, cell) in row.cells.iter().enumerate() {
                if idx >= column_count {
                    break;
                }
                let cwidth = col_widths[idx];
                let text_w = (cwidth - cell.style.padding_left - cell.style.padding_right).max(1.0);
                let cell_spacing = SpacingOpts {
                    letter_spacing: cell.style.letter_spacing,
                    word_spacing: cell.style.word_spacing,
                };
                let wrapped =
                    wrap_runs_impl(&cell.runs, text_w, css::WhiteSpace::Normal, cell_spacing)?;
                let max_fs = wrapped
                    .iter()
                    .map(|l| l.max_font_size)
                    .fold(0.0_f32, f32::max);
                let lh = cell
                    .line_height
                    .or(table.default_line_height)
                    .unwrap_or(max_fs * 1.2);
                let text_h = wrapped.len() as f32 * lh;
                cell_text_heights.push(text_h);
                cell_line_heights.push(lh);
                wrapped_cells.push(wrapped);
            }

            // Row height = tallest cell's padded content
            let row_height = row
                .cells
                .iter()
                .zip(cell_text_heights.iter())
                .map(|(cell, &th)| th + cell.style.padding_top + cell.style.padding_bottom)
                .fold(0.0_f32, f32::max);

            // Page-break check: move the entire row to next column/page if it doesn't fit
            if *cursor_y - row_height < self.margin_bottom && *cursor_y < content_top_v {
                // Flush any pending collapse band before moving on.
                if is_collapse && let Some(band) = collapse_band.take() {
                    let mut page = current_page.lock().unwrap();
                    flush_collapse_band(&mut page, &band, &col_widths);
                }
                self.advance_column_or_page(
                    current_page,
                    doc,
                    cursor_y,
                    current_col,
                    col_count,
                    col_width,
                    col_gap,
                    content_top,
                );
            }

            // Draw cells in this row
            let row_top = *cursor_y;
            let row_bottom = row_top - row_height;
            let mut cell_x = col_x_for(*current_col) + table.style.margin_left;
            let band_x0 = cell_x;
            if table_actual_width < table_area_width {
                // tables default to left-aligned at the margin; no centering
            }

            let mut page = current_page.lock().unwrap();
            // Register fonts used by cells
            for wrapped in &wrapped_cells {
                for line in wrapped {
                    for seg in &line.segments {
                        if !page.fonts_used.contains(&seg.font_name) {
                            page.fonts_used.push(seg.font_name.clone());
                        }
                    }
                }
            }

            for (idx, cell) in row.cells.iter().enumerate() {
                if idx >= column_count {
                    break;
                }
                let cwidth = col_widths[idx];

                // Cell background
                if let Some((r, g, b)) = cell.style.background_color {
                    page.operations.push(PdfOp::SetFillColor { r, g, b });
                    page.operations.push(PdfOp::Rectangle {
                        x: cell_x,
                        y: row_bottom,
                        width: cwidth,
                        height: row_height,
                    });
                    page.operations.push(PdfOp::Fill);
                }

                // Cell border — in separate mode each cell strokes its own
                // rectangle; in collapse mode we accumulate into the band and
                // draw the outer + internal gridlines once when the band ends.
                let cell_has_border = cell.style.border_width > 0.0
                    && !matches!(cell.style.border_style, Some(css::BorderStyle::None));
                if !is_collapse && cell_has_border {
                    let (r, g, b) = cell.style.border_color.unwrap_or((0.0, 0.0, 0.0));
                    page.operations.push(PdfOp::SetStrokeColor { r, g, b });
                    page.operations
                        .push(PdfOp::SetLineWidth(cell.style.border_width));
                    page.operations.push(PdfOp::Rectangle {
                        x: cell_x,
                        y: row_bottom,
                        width: cwidth,
                        height: row_height,
                    });
                    page.operations.push(PdfOp::Stroke);
                }

                // Set text color
                if let Some((r, g, b)) = cell.style.color {
                    page.operations.push(PdfOp::SetFillColor { r, g, b });
                } else {
                    page.operations.push(PdfOp::SetFillColor {
                        r: 0.0,
                        g: 0.0,
                        b: 0.0,
                    });
                }

                // Determine text start y based on vertical_align
                let text_height = cell_text_heights[idx];
                let avail_text_h = row_height - cell.style.padding_top - cell.style.padding_bottom;
                let v_offset = match cell.vertical_align {
                    VerticalAlign::Top => 0.0,
                    VerticalAlign::Middle => (avail_text_h - text_height) / 2.0,
                    VerticalAlign::Bottom => avail_text_h - text_height,
                };

                let text_x_base = cell_x + cell.style.padding_left;
                let text_area_w = cwidth - cell.style.padding_left - cell.style.padding_right;
                let line_h = cell_line_heights[idx];
                let mut line_y = row_top - cell.style.padding_top - v_offset;

                for line in &wrapped_cells[idx] {
                    let baseline_y = line_y - line.max_font_size;
                    let align_offset = match cell.style.text_align {
                        TextAlign::Left | TextAlign::Justify => 0.0,
                        TextAlign::Center => (text_area_w - line.total_width) / 2.0,
                        TextAlign::Right => text_area_w - line.total_width,
                    };
                    let mut x = text_x_base + align_offset;
                    for segment in &line.segments {
                        if let Some((r, g, b)) = segment.color {
                            page.operations.push(PdfOp::SetFillColor { r, g, b });
                        }
                        page.operations.push(PdfOp::BeginText);
                        page.operations.push(PdfOp::SetFont {
                            name: segment.font_name.clone(),
                            size: segment.font_size,
                        });
                        page.operations
                            .push(PdfOp::SetTextPosition { x, y: baseline_y });
                        page.operations
                            .push(show_text_for(&segment.font_name, segment.text.clone()));
                        page.operations.push(PdfOp::EndText);
                        x += segment.width;
                    }
                    line_y -= line_h;
                }

                cell_x += cwidth;
            }
            drop(page);

            if is_collapse {
                let band = collapse_band.get_or_insert_with(|| CollapseBand {
                    x0: band_x0,
                    top_y: row_top,
                    row_bottoms: Vec::new(),
                    max_border_width: 0.0,
                    border_color: None,
                    has_border: false,
                });
                band.row_bottoms.push(row_bottom);
                for cell in &row.cells {
                    let cell_has_border = cell.style.border_width > 0.0
                        && !matches!(cell.style.border_style, Some(css::BorderStyle::None));
                    if cell_has_border {
                        band.has_border = true;
                        if cell.style.border_width > band.max_border_width {
                            band.max_border_width = cell.style.border_width;
                        }
                        if band.border_color.is_none() {
                            band.border_color = cell.style.border_color;
                        }
                    }
                }
            }

            *cursor_y = row_bottom;
        }

        // Flush any remaining collapse band at the end of the table.
        if is_collapse && let Some(band) = collapse_band.take() {
            let mut page = current_page.lock().unwrap();
            flush_collapse_band(&mut page, &band, &col_widths);
        }

        // Record table bottom margin as pending so the following block's
        // top margin collapses against it. `spacing_after` is applied now
        // (it is not a CSS margin).
        *cursor_y -= table.style.margin_bottom + table.spacing_after;
        *pending_bottom = table.style.margin_bottom;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_image(
        &mut self,
        current_page: &mut Arc<Mutex<PageContent>>,
        doc: &mut PdfDocument,
        img: &ImageBlock,
        cursor_y: &mut f32,
        current_col: &mut u32,
        col_count: u32,
        col_width: &mut f32,
        col_gap: &mut f32,
        content_top: &mut f32,
        pending_bottom: &mut f32,
    ) {
        // Snapshot the column geometry — see `render_table` for the
        // rationale (mid-block page breaks may rewrite the caller's
        // refs to the new page's geometry).
        let col_width_v: f32 = *col_width;
        let col_gap_v: f32 = *col_gap;
        let content_top_v: f32 = *content_top;
        let margin_left_v: f32 = self.margin_left;
        let col_x_for =
            |col: u32| -> f32 { margin_left_v + col as f32 * (col_width_v + col_gap_v) };

        // Scale down if wider than column content width
        let box_width = col_width_v - img.style.margin_left - img.style.margin_right;
        let (draw_w, draw_h) = if img.width > box_width && img.width > 0.0 {
            let scale = box_width / img.width;
            (box_width, img.height * scale)
        } else {
            (img.width, img.height)
        };

        let top_delta = collapsed_top_delta(*pending_bottom, img.style.margin_top);
        let total_height = top_delta + draw_h + img.style.margin_bottom;

        // Page-break check
        if *cursor_y - total_height < self.margin_bottom && *cursor_y < content_top_v {
            self.advance_column_or_page(
                current_page,
                doc,
                cursor_y,
                current_col,
                col_count,
                col_width,
                col_gap,
                content_top,
            );
            *pending_bottom = 0.0;
        }

        *cursor_y -= collapsed_top_delta(*pending_bottom, img.style.margin_top);
        let x = col_x_for(*current_col) + img.style.margin_left;
        let y_bottom = *cursor_y - draw_h;

        let mut page = current_page.lock().unwrap();
        if !page.images_used.contains(&img.image_index) {
            page.images_used.push(img.image_index);
        }
        page.operations.push(PdfOp::DrawImage {
            index: img.image_index,
            x,
            y: y_bottom,
            width: draw_w,
            height: draw_h,
        });
        drop(page);

        *cursor_y = y_bottom - img.style.margin_bottom - img.spacing_after;
        *pending_bottom = img.style.margin_bottom;
    }

    /// Advance to the next column, or if already in the last column,
    /// draw rules on the current page and start a new one.
    ///
    /// When opening a new page, this re-resolves the per-page `@page`
    /// style (CSS Paged Media L3 §4.4) so margins, page size, and
    /// margin-box content can vary across pages (e.g. `:first` shrinks
    /// page 1's margins, `:left`/`:right` flip mirror gutters). The
    /// caller's column-relative cursor / `col_width` state is updated
    /// in place to reflect the freshly resolved geometry.
    #[allow(clippy::too_many_arguments)]
    fn advance_column_or_page(
        &mut self,
        current_page: &mut Arc<Mutex<PageContent>>,
        doc: &mut PdfDocument,
        cursor_y: &mut f32,
        current_col: &mut u32,
        col_count: u32,
        col_width: &mut f32,
        col_gap: &mut f32,
        content_top: &mut f32,
    ) {
        if *current_col < col_count - 1 {
            *current_col += 1;
            *cursor_y = *content_top;
        } else {
            self.draw_column_rules(current_page, *content_top, *col_width, *col_gap, col_count);
            // Re-resolve `@page` for the page we're about to open. The
            // page index is `doc.pages.len() + 1` because the new page
            // hasn't been pushed yet.
            if !self.page_rules.is_empty() {
                let next_page_num = doc.pages.len() + 1;
                let _ = self.apply_page_style_for(next_page_num, next_page_num);
            }
            *current_page = new_page(doc, self.page_width, self.page_height);
            // Recompute column / content geometry so blocks on the new
            // page measure against the freshly resolved margins.
            let new_content_width = self.page_width - self.margin_left - self.margin_right;
            let (new_col_w, new_col_g) = if col_count > 1 {
                let gap_total = (col_count - 1) as f32 * self.column_gap;
                (
                    (new_content_width - gap_total) / col_count as f32,
                    self.column_gap,
                )
            } else {
                (new_content_width, 0.0)
            };
            *col_width = new_col_w;
            *col_gap = new_col_g;
            *content_top = self.page_height - self.margin_top;
            *cursor_y = *content_top;
            *current_col = 0;
        }
    }

    fn draw_column_rules(
        &self,
        page: &Arc<Mutex<PageContent>>,
        content_top: f32,
        col_width: f32,
        col_gap: f32,
        col_count: u32,
    ) {
        if col_count <= 1 || self.column_rule_width <= 0.0 {
            return;
        }
        let mut page = page.lock().unwrap();
        page.operations.push(PdfOp::SaveState);
        let (r, g, b) = self.column_rule_color.unwrap_or((0.75, 0.75, 0.75));
        page.operations.push(PdfOp::SetStrokeColor { r, g, b });
        page.operations
            .push(PdfOp::SetLineWidth(self.column_rule_width));

        for col in 0..(col_count - 1) {
            let rule_x = self.margin_left
                + (col + 1) as f32 * col_width
                + col as f32 * col_gap
                + col_gap / 2.0;
            page.operations.push(PdfOp::MoveTo {
                x: rule_x,
                y: content_top,
            });
            page.operations.push(PdfOp::LineTo {
                x: rule_x,
                y: self.margin_bottom,
            });
            page.operations.push(PdfOp::Stroke);
        }

        page.operations.push(PdfOp::RestoreState);
    }
}

// ── Layout (PyO3 wrapper) ────────────────────────────────────

#[pyclass]
pub struct Layout {
    inner: LayoutInner,
    doc: Py<PdfDocument>,
}

#[pymethods]
impl Layout {
    #[new]
    #[allow(clippy::unnecessary_wraps)]
    #[pyo3(signature = (doc, margin_top=72.0, margin_right=72.0, margin_bottom=72.0, margin_left=72.0, page_width=612.0, page_height=792.0))]
    fn new(
        doc: Py<PdfDocument>,
        margin_top: f64,
        margin_right: f64,
        margin_bottom: f64,
        margin_left: f64,
        page_width: f64,
        page_height: f64,
    ) -> PyResult<Self> {
        Ok(Layout {
            inner: LayoutInner::new(
                margin_top as f32,
                margin_right as f32,
                margin_bottom as f32,
                margin_left as f32,
                page_width as f32,
                page_height as f32,
            ),
            doc,
        })
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (text, font="Helvetica", font_size=12.0, line_height=None, spacing_after=0.0, color=None, background_color=None, padding=None, border_width=0.0, border_color=None, text_align="left"))]
    fn add_text(
        &mut self,
        text: &str,
        font: &str,
        font_size: f64,
        line_height: Option<f64>,
        spacing_after: f64,
        color: Option<(f64, f64, f64)>,
        background_color: Option<(f64, f64, f64)>,
        padding: Option<&Bound<'_, PyAny>>,
        border_width: f64,
        border_color: Option<(f64, f64, f64)>,
        text_align: &str,
    ) -> PyResult<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        if !BUILTIN_FONTS.contains(&font) {
            return Err(PyValueError::new_err(format!("unknown font: {font}")));
        }

        let align = parse_text_align(text_align)?;
        let (pad_t, pad_r, pad_b, pad_l) = parse_padding(padding)?;
        let fs = font_size as f32;
        let run = TextRun {
            text: text.to_string(),
            font_name: font.to_string(),
            font_size: fs,
            color: color.map(rgb_to_f32),
            ..Default::default()
        };

        self.inner.blocks.push(Block::Paragraph(Paragraph {
            runs: vec![run],
            line_height: Some(line_height.map_or(fs * 1.2, |h| h as f32)),
            spacing_after: spacing_after as f32,
            style: BlockStyle {
                background_color: background_color.map(rgb_to_f32),
                padding_top: pad_t,
                padding_right: pad_r,
                padding_bottom: pad_b,
                padding_left: pad_l,
                border_width: border_width as f32,
                border_color: border_color.map(rgb_to_f32),
                text_align: align,
                ..BlockStyle::default()
            },
            marker: None,
            is_hr: false,
            white_space: css::WhiteSpace::Normal,
            tag: None,
            anchor_id: None,
        }));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (runs, line_height=None, spacing_after=0.0, color=None, background_color=None, padding=None, border_width=0.0, border_color=None, text_align="left", marker=None))]
    fn add_paragraph(
        &mut self,
        runs: Vec<TextRun>,
        line_height: Option<f64>,
        spacing_after: f64,
        color: Option<(f64, f64, f64)>,
        background_color: Option<(f64, f64, f64)>,
        padding: Option<&Bound<'_, PyAny>>,
        border_width: f64,
        border_color: Option<(f64, f64, f64)>,
        text_align: &str,
        marker: Option<TextRun>,
    ) -> PyResult<()> {
        let align = parse_text_align(text_align)?;
        let (pad_t, pad_r, pad_b, pad_l) = parse_padding(padding)?;

        self.inner.push_paragraph(Paragraph {
            runs,
            line_height: line_height.map(|h| h as f32),
            spacing_after: spacing_after as f32,
            style: BlockStyle {
                color: color.map(rgb_to_f32),
                background_color: background_color.map(rgb_to_f32),
                padding_top: pad_t,
                padding_right: pad_r,
                padding_bottom: pad_b,
                padding_left: pad_l,
                border_width: border_width as f32,
                border_color: border_color.map(rgb_to_f32),
                text_align: align,
                ..BlockStyle::default()
            },
            marker,
            is_hr: false,
            white_space: css::WhiteSpace::Normal,
            tag: None,
            anchor_id: None,
        });
        Ok(())
    }

    fn finish(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.doc.borrow_mut(py);
        self.inner.finish(&mut doc).map_err(PyValueError::new_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_text_empty_input_returns_empty() {
        let lines = wrap_text_impl("", 100.0, "Helvetica", 12.0).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn wrap_text_whitespace_only_returns_empty() {
        let lines = wrap_text_impl("   \t\n  ", 100.0, "Helvetica", 12.0).unwrap();
        assert!(lines.is_empty());
    }

    #[test]
    fn wrap_text_short_fits_on_one_line() {
        let lines = wrap_text_impl("hello world", 200.0, "Helvetica", 12.0).unwrap();
        assert_eq!(lines, vec!["hello world".to_string()]);
    }

    #[test]
    fn wrap_text_wraps_at_word_boundary() {
        let lines = wrap_text_impl(
            "one two three four five six seven eight nine ten",
            60.0,
            "Helvetica",
            12.0,
        )
        .unwrap();
        assert!(lines.len() > 1);
        for line in &lines {
            assert!(!line.starts_with(' '));
            assert!(!line.ends_with(' '));
        }
    }

    #[test]
    fn wrap_text_single_long_word_kept_on_own_line() {
        let lines = wrap_text_impl(
            "short verylongunbreakableword more",
            40.0,
            "Helvetica",
            12.0,
        )
        .unwrap();
        // the long word overflows but should still produce a line for it
        // rather than getting lost.
        assert!(lines.iter().any(|l| l.contains("verylongunbreakableword")));
    }

    #[test]
    fn wrap_text_collapses_multiple_spaces() {
        let lines = wrap_text_impl("hello    world", 200.0, "Helvetica", 12.0).unwrap();
        assert_eq!(lines, vec!["hello world".to_string()]);
    }

    #[test]
    fn wrap_text_rejects_unknown_font() {
        let err = wrap_text_impl("hello", 100.0, "NotAFont", 12.0).unwrap_err();
        assert!(err.contains("unknown font"));
    }

    // ── <col> width hint resolution (CSS 2.1 §17.5.2) ────────

    #[test]
    fn auto_layout_explicit_widths_set_preferred_not_min() {
        // Container = 600pt, hints = 30%/10%/60%, intrinsic mins = 10pt.
        // Auto layout must:
        //   * raise col_preferred to the resolved hint when the hint is
        //     larger than the content min-width
        //   * leave col_min UNCHANGED (this is the bug-prevention assert
        //     — the naive `col_min = col_max = hint` plan would clip a
        //     long unbreakable word in a hinted-narrow column).
        //   * keep col_max >= col_preferred.
        let col_min = vec![10.0_f32; 3];
        let mut col_max = vec![20.0_f32, 20.0, 20.0];
        let mut col_preferred = col_max.clone();
        let hints = vec![Some(180.0), Some(60.0), Some(360.0)]; // 30/10/60% of 600
        let col_min_before = col_min.clone();
        apply_col_hints_auto(&col_min, &mut col_max, &mut col_preferred, &hints);
        assert_eq!(col_preferred, vec![180.0, 60.0, 360.0]);
        // col_min was not touched — even after applying hints. This is
        // what the long-URL test below relies on.
        assert_eq!(col_min, col_min_before);
        for i in 0..3 {
            assert!(
                col_max[i] >= col_preferred[i],
                "col_max[{i}]={} should be >= col_preferred[{i}]={}",
                col_max[i],
                col_preferred[i]
            );
        }

        // Bug-prevention assert: a long word with min-width 80 in
        // column 1 (hinted to 60) must not be clipped to 60.
        // col_min is the floor for col_preferred, so the column gets
        // its real intrinsic minimum (80) regardless of the smaller hint.
        let col_min2 = vec![10.0_f32, 80.0, 10.0];
        let mut col_max2 = vec![20.0_f32, 80.0, 20.0];
        let mut col_preferred2 = col_max2.clone();
        let hints2 = vec![Some(180.0), Some(60.0), Some(360.0)];
        apply_col_hints_auto(&col_min2, &mut col_max2, &mut col_preferred2, &hints2);
        assert!(
            col_preferred2[1] >= 80.0,
            "long word in column 1 (min 80, hint 60) was clipped to {}",
            col_preferred2[1]
        );
    }

    // ── margin collapse (Stage B1) ──────────────────────────
    #[test]
    fn collapse_two_positives_picks_the_larger() {
        assert!((collapse_margins(10.0, 20.0) - 20.0).abs() < 1e-6);
        assert!((collapse_margins(20.0, 10.0) - 20.0).abs() < 1e-6);
    }

    #[test]
    fn collapse_two_negatives_picks_the_most_negative() {
        assert!((collapse_margins(-5.0, -10.0) + 10.0).abs() < 1e-6);
        assert!((collapse_margins(-10.0, -5.0) + 10.0).abs() < 1e-6);
    }

    #[test]
    fn collapse_mixed_sign_sums() {
        assert!((collapse_margins(10.0, -4.0) - 6.0).abs() < 1e-6);
        assert!((collapse_margins(-5.0, 3.0) + 2.0).abs() < 1e-6);
    }

    #[test]
    fn collapsed_top_delta_zero_when_pending_dominates() {
        // A prior bottom of 20 has already been spent. A new top of 10
        // collapses to max(20,10)=20 — so no additional advance.
        assert!(collapsed_top_delta(20.0, 10.0).abs() < 1e-6);
    }

    #[test]
    fn collapsed_top_delta_is_difference_when_new_top_dominates() {
        // Prior 5 already spent, new top 12 -> effective 12, need to add 7.
        assert!((collapsed_top_delta(5.0, 12.0) - 7.0).abs() < 1e-6);
    }

    #[test]
    fn collapsed_top_delta_handles_negatives() {
        // Prior -5 spent, new top 3 -> effective -2, need -2-(-5)=+3 more.
        assert!((collapsed_top_delta(-5.0, 3.0) - 3.0).abs() < 1e-6);
    }
}
