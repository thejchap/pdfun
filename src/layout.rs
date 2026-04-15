use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use crate::box_tree::{AnonymousBox, BlockBox, Node};
use crate::css;
use crate::font_metrics;
use crate::{PageContent, PdfDocument, PdfOp, BUILTIN_FONTS};

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

#[derive(Clone, Default)]
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
    /// emission in SaveState → SetAlpha → … → RestoreState.
    pub opacity: Option<f32>,
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
#[derive(Clone)]
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
}

#[pymethods]
impl TextRun {
    #[new]
    #[pyo3(signature = (text, font_name="Helvetica", font_size=12.0, color=None))]
    fn new(
        text: String,
        font_name: &str,
        font_size: f64,
        color: Option<(f64, f64, f64)>,
    ) -> Self {
        TextRun {
            text,
            font_name: font_name.to_string(),
            font_size: font_size as f32,
            color: color.map(rgb_to_f32),
            text_decoration: None,
            link_url: None,
            baseline_shift: 0.0,
        }
    }
}

// ── Paragraph ────────────────────────────────────────────────

pub struct Paragraph {
    pub runs: Vec<TextRun>,
    pub line_height: Option<f32>,
    pub spacing_after: f32,
    pub style: BlockStyle,
    pub marker: Option<TextRun>,
    pub is_hr: bool,
    pub preserve_whitespace: bool,
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
    /// Optional `<caption>` laid out above the first row (caption-side: top
    /// is assumed; `caption-side: bottom` is out of scope for this batch).
    /// Rendered centered by default; the caller may override via the
    /// caption paragraph's `style.text_align`.
    pub caption: Option<Box<Paragraph>>,
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
    preserve_whitespace: bool,
    spacing: SpacingOpts,
) -> Result<Vec<WrappedLine>, String> {
    if preserve_whitespace {
        return wrap_runs_preformatted(runs, spacing);
    }

    // Phase 1: flatten runs into styled words
    let mut words: Vec<StyledWord> = Vec::new();
    for run in runs {
        for word in run.text.split_whitespace() {
            words.push(StyledWord {
                text: word.to_string(),
                font_name: run.font_name.clone(),
                font_size: run.font_size,
                color: run.color,
                text_decoration: run.text_decoration,
                link_url: run.link_url.clone(),
                baseline_shift: run.baseline_shift,
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
        let base_word_width =
            font_metrics::measure_str(&word.text, &word.font_name, word.font_size)
                .ok_or_else(|| format!("unknown font: {}", word.font_name))?;
        let word_width =
            base_word_width + spacing.letter_spacing * word.text.chars().count() as f32;

        if current.is_empty() {
            current_width = word_width;
            current.push(word);
        } else {
            let base_space =
                font_metrics::measure_str(" ", &word.font_name, word.font_size).unwrap_or(0.0);
            // The joining space is itself a glyph, so one letter-spacing
            // advances after it as well as word-spacing.
            let space_width = base_space + spacing.letter_spacing + spacing.word_spacing;
            if current_width + space_width + word_width <= max_width {
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
            let can_merge = segments.last().is_some_and(|last: &LineSegment| {
                last.font_name == word.font_name
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
                last.width = base
                    + spacing.letter_spacing * n_chars
                    + spacing.word_spacing * n_spaces;
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
                });
            }
        }

        let total_width: f32 = segments.iter().map(|s| s.width).sum();
        let max_font_size = segments
            .iter()
            .map(|s| s.font_size)
            .fold(0.0_f32, f32::max);
        result.push(WrappedLine {
            segments,
            total_width,
            max_font_size,
        });
    }

    Ok(result)
}

/// Wrap preformatted text: preserve whitespace, split on newlines only.
fn wrap_runs_preformatted(
    runs: &[TextRun],
    spacing: SpacingOpts,
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
        link_url = run.link_url.clone();
        baseline_shift = run.baseline_shift;
    }

    let mut result: Vec<WrappedLine> = Vec::new();
    for line in full_text.split('\n') {
        let base = font_metrics::measure_str(line, &font_name, font_size)
            .ok_or_else(|| format!("unknown font: {font_name}"))?;
        let width = base
            + spacing.letter_spacing * line.chars().count() as f32
            + spacing.word_spacing * line.matches(' ').count() as f32;
        let segment = LineSegment {
            text: line.to_string(),
            font_name: font_name.clone(),
            font_size,
            color,
            width,
            text_decoration,
            link_url: link_url.clone(),
            baseline_shift,
        };
        result.push(WrappedLine {
            segments: vec![segment],
            total_width: width,
            max_font_size: font_size,
        });
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

fn rgb_to_f32(c: (f64, f64, f64)) -> (f32, f32, f32) {
    (c.0 as f32, c.1 as f32, c.2 as f32)
}

fn new_page(doc: &mut PdfDocument, width: f32, height: f32) -> Arc<Mutex<PageContent>> {
    let page = Arc::new(Mutex::new(PageContent::new(f64::from(width), f64::from(height))));
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
        }
    }

    pub fn push_paragraph(&mut self, para: Paragraph) {
        if !para.preserve_whitespace {
            let has_content = para.runs.iter().any(|r| !r.text.trim().is_empty());
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
            preserve_whitespace: false,
        }));
    }

    pub fn finish(&mut self, doc: &mut PdfDocument) -> Result<(), String> {
        let content_width = self.page_width - self.margin_left - self.margin_right;
        let content_top = self.page_height - self.margin_top;
        let blocks = std::mem::take(&mut self.blocks);

        let col_count = self.column_count.max(1);
        let (col_width, col_gap) = if col_count > 1 {
            let gap_total = (col_count - 1) as f32 * self.column_gap;
            ((content_width - gap_total) / col_count as f32, self.column_gap)
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
        };

        let tree = crate::box_tree::unflatten_blocks(blocks);
        self.render_nodes(doc, &tree, &mut state)?;

        self.draw_column_rules(&state.current_page, content_top, col_width, col_gap, col_count);

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
        self.render_table(
            &mut state.current_page,
            doc,
            table,
            &mut state.cursor_y,
            &mut state.current_col,
            state.col_count,
            state.col_width,
            state.col_gap,
            state.content_top,
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
            state.col_width,
            state.col_gap,
            state.content_top,
            &mut state.pending_bottom,
        );
    }

    fn render_hr_node(
        &mut self,
        doc: &mut PdfDocument,
        bb: &BlockBox,
        state: &mut RenderState,
    ) {
        Self::fold_container_top(state);
        if state.cursor_y - 12.0 < self.margin_bottom && state.cursor_y < state.content_top {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                state.col_width,
                state.col_gap,
                state.content_top,
            );
        }
        let cx = self.col_x(state.current_col, state);
        let cursor_y = state.cursor_y;
        let col_width = state.col_width;
        let mut page = state.current_page.lock().unwrap();
        page.operations.push(PdfOp::SaveState);
        page.operations
            .push(PdfOp::SetStrokeColor { r: 0.75, g: 0.75, b: 0.75 });
        page.operations.push(PdfOp::SetLineWidth(0.5));
        page.operations.push(PdfOp::MoveTo {
            x: cx,
            y: cursor_y,
        });
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
                state.col_width,
                state.col_gap,
                state.content_top,
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
                state.col_width,
                state.col_gap,
                state.content_top,
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
                state.col_width,
                state.col_gap,
                state.content_top,
            );
            state.pending_container_top = 0.0;
        }

        Self::fold_container_top(state);

        let h_padding = style.padding_left + style.padding_right;
        let h_border = style.border_width * 2.0;
        let content_adjust = match style.box_sizing {
            css::BoxSizing::ContentBox => 0.0,
            css::BoxSizing::BorderBox => h_padding + h_border,
        };
        let available_text_width =
            state.col_width - style.margin_left - style.margin_right - h_padding;
        let mut text_area_width = match style.width {
            Some(w) => (w - content_adjust).max(0.0),
            None => available_text_width,
        };
        if let Some(max_w) = style.max_width {
            text_area_width = text_area_width.min((max_w - content_adjust).max(0.0));
        }
        if let Some(min_w) = style.min_width {
            text_area_width = text_area_width.max((min_w - content_adjust).max(0.0));
        }
        text_area_width = text_area_width.min(available_text_width).max(0.0);
        let box_width = text_area_width + h_padding;

        let spacing = SpacingOpts {
            letter_spacing: style.letter_spacing,
            word_spacing: style.word_spacing,
        };
        let text_indent = style.text_indent.max(0.0);
        let marker_gap = 6.0_f32;
        let marker_width_outside = marker
            .map(|m| {
                font_metrics::measure_str(&m.text, &m.font_name, m.font_size).unwrap_or(0.0)
            })
            .unwrap_or(0.0);
        let inside_indent = if matches!(
            style.list_style_position,
            css::ListStylePosition::Inside
        ) && marker.is_some()
        {
            marker_width_outside + marker_gap
        } else {
            0.0
        };
        let wrap_width = (text_area_width - text_indent - inside_indent).max(0.0);
        let wrapped_lines = wrap_runs_impl(
            &anon.runs,
            wrap_width,
            anon.preserve_whitespace,
            spacing,
        )?;

        let max_font_size = wrapped_lines
            .iter()
            .map(|l| l.max_font_size)
            .fold(0.0_f32, f32::max);
        let line_height = anon.line_height.unwrap_or(max_font_size * 1.2);

        let block_text_height = wrapped_lines.len() as f32 * line_height;
        let natural_box_height =
            style.padding_top + block_text_height + style.padding_bottom;
        // Clamp to min/max-height. The content still top-aligns inside the
        // padded box — min-height extends the background/border rect down
        // but does not shift the text. (CSS actually lets height: auto
        // grow to content, so only min-height ever extends downward; we
        // still honor max-height by clipping the drawn rect — any text
        // that would have hung below the clamped edge just spills out.)
        let mut box_height = natural_box_height;
        if let Some(min_h) = style.min_height {
            if box_height < min_h {
                box_height = min_h;
            }
        }
        if let Some(max_h) = style.max_height {
            if box_height > max_h {
                box_height = max_h;
            }
        }
        let top_delta = collapsed_top_delta(state.pending_bottom, style.margin_top);
        let block_total_height = top_delta + box_height + style.margin_bottom;

        if state.cursor_y - block_total_height < self.margin_bottom
            && state.cursor_y < state.content_top
        {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                state.col_width,
                state.col_gap,
                state.content_top,
            );
            state.pending_bottom = 0.0;
        }

        let cx = self.col_x(state.current_col, state);
        let box_x = cx + style.margin_left;
        state.cursor_y -= collapsed_top_delta(state.pending_bottom, style.margin_top);

        let needs_state_wrap =
            style.has_any_styling() || anon.runs.iter().any(|r| r.color.is_some());
        let cursor_y = state.cursor_y;
        let mut page = state.current_page.lock().unwrap();

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
        }

        // Helper for emitting a rectangle path that is either sharp or
        // rounded depending on `style.border_radius`.
        let emit_rect_path = |ops: &mut Vec<PdfOp>, x: f32, y: f32, w: f32, h: f32| {
            match style.border_radius {
                Some(radii) => ops.push(PdfOp::RoundedRectangle {
                    x,
                    y,
                    width: w,
                    height: h,
                    radii,
                }),
                None => ops.push(PdfOp::Rectangle { x, y, width: w, height: h }),
            }
        };

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

        if style.border_width > 0.0
            && !matches!(style.border_style, Some(css::BorderStyle::None))
        {
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

        if let Some((r, g, b)) = style.color {
            page.operations.push(PdfOp::SetFillColor { r, g, b });
        }

        let text_x_base = box_x + style.padding_left;
        let mut line_y = cursor_y - style.padding_top;
        let is_inside_marker = matches!(
            style.list_style_position,
            css::ListStylePosition::Inside
        ) && marker.is_some();

        for (line_idx, line) in wrapped_lines.iter().enumerate() {
            let baseline_y = line_y - line.max_font_size;

            if line_idx == 0
                && let Some(marker) = marker
            {
                let marker_width = font_metrics::measure_str(
                    &marker.text,
                    &marker.font_name,
                    marker.font_size,
                )
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
                    .push(PdfOp::ShowText(marker.text.clone()));
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
                    indent_for_line + (text_area_width - indent_for_line - line.total_width)
                        / 2.0
                }
                TextAlign::Right => text_area_width - line.total_width,
            };

            let needs_tw = total_word_spacing != 0.0;
            let needs_tc = style.letter_spacing != 0.0;
            if needs_tw {
                page.operations.push(PdfOp::SetWordSpacing(total_word_spacing));
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

                page.operations.push(PdfOp::BeginText);
                page.operations.push(PdfOp::SetFont {
                    name: segment.font_name.clone(),
                    size: segment.font_size,
                });
                page.operations
                    .push(PdfOp::SetTextPosition { x, y: baseline_y });
                page.operations
                    .push(PdfOp::ShowText(segment.text.clone()));
                page.operations.push(PdfOp::EndText);

                if let Some(url) = &segment.link_url {
                    page.links.push(crate::LinkAnnotation {
                        x,
                        y: baseline_y - segment.font_size * 0.2,
                        width: segment.width,
                        height: segment.font_size * 1.1,
                        url: url.clone(),
                    });
                }

                if let Some(td) = segment.text_decoration {
                    if td.underline || td.line_through {
                        let metrics =
                            font_metrics::get_builtin_metrics(&segment.font_name);
                        let scale = segment.font_size / 1000.0;
                        let stroke_width = (segment.font_size * 0.05).max(0.5);

                        page.operations.push(PdfOp::SaveState);
                        if let Some((r, g, b)) = segment.color {
                            page.operations
                                .push(PdfOp::SetStrokeColor { r, g, b });
                        }
                        page.operations.push(PdfOp::SetLineWidth(stroke_width));

                        if td.underline {
                            let descent = metrics
                                .map_or(-207.0, |m| m.descent as f32);
                            let underline_y = baseline_y + descent * scale / 3.0;
                            page.operations.push(PdfOp::MoveTo {
                                x,
                                y: underline_y,
                            });
                            page.operations.push(PdfOp::LineTo {
                                x: x + segment.width,
                                y: underline_y,
                            });
                            page.operations.push(PdfOp::Stroke);
                        }

                        if td.line_through {
                            let ascent =
                                metrics.map_or(718.0, |m| m.ascent as f32);
                            let strike_y = baseline_y + ascent * scale / 3.0;
                            page.operations.push(PdfOp::MoveTo {
                                x,
                                y: strike_y,
                            });
                            page.operations.push(PdfOp::LineTo {
                                x: x + segment.width,
                                y: strike_y,
                            });
                            page.operations.push(PdfOp::Stroke);
                        }

                        page.operations.push(PdfOp::RestoreState);
                    }
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
        state.cursor_y -= box_height + style.margin_bottom + anon.spacing_after;
        state.pending_bottom = style.margin_bottom;

        if matches!(style.page_break_after, Some(css::PageBreak::Always)) {
            self.advance_column_or_page(
                &mut state.current_page,
                doc,
                &mut state.cursor_y,
                &mut state.current_col,
                state.col_count,
                state.col_width,
                state.col_gap,
                state.content_top,
            );
            state.pending_bottom = 0.0;
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn render_table(
        &mut self,
        current_page: &mut Arc<Mutex<PageContent>>,
        doc: &mut PdfDocument,
        table: &Table,
        cursor_y: &mut f32,
        current_col: &mut u32,
        col_count: u32,
        col_width: f32,
        col_gap: f32,
        content_top: f32,
        pending_bottom: &mut f32,
    ) -> Result<(), String> {
        if table.rows.is_empty() {
            return Ok(());
        }
        let column_count = table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
        if column_count == 0 {
            return Ok(());
        }

        // column_width helper (reused)
        let col_x_for = |col: u32| -> f32 {
            self.margin_left + col as f32 * (col_width + col_gap)
        };

        // Total available width for the table within the current column
        let table_area_width =
            col_width - table.style.margin_left - table.style.margin_right;

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
                col_max
                    .iter()
                    .map(|&w| w + extra * (w / sum_max))
                    .collect()
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
                let wrapped = wrap_runs_impl(&cell.runs, text_w, false, cell_spacing)?;
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
            if *cursor_y - row_height < self.margin_bottom && *cursor_y < content_top {
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

                // Cell border
                if cell.style.border_width > 0.0
                    && !matches!(cell.style.border_style, Some(css::BorderStyle::None))
                {
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
                    page.operations
                        .push(PdfOp::SetFillColor { r: 0.0, g: 0.0, b: 0.0 });
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
                            .push(PdfOp::ShowText(segment.text.clone()));
                        page.operations.push(PdfOp::EndText);
                        x += segment.width;
                    }
                    line_y -= line_h;
                }

                cell_x += cwidth;
            }
            drop(page);

            *cursor_y = row_bottom;
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
        col_width: f32,
        col_gap: f32,
        content_top: f32,
        pending_bottom: &mut f32,
    ) {
        let col_x_for = |col: u32| -> f32 {
            self.margin_left + col as f32 * (col_width + col_gap)
        };

        // Scale down if wider than column content width
        let box_width = col_width
            - img.style.margin_left
            - img.style.margin_right;
        let (draw_w, draw_h) = if img.width > box_width && img.width > 0.0 {
            let scale = box_width / img.width;
            (box_width, img.height * scale)
        } else {
            (img.width, img.height)
        };

        let top_delta = collapsed_top_delta(*pending_bottom, img.style.margin_top);
        let total_height = top_delta + draw_h + img.style.margin_bottom;

        // Page-break check
        if *cursor_y - total_height < self.margin_bottom && *cursor_y < content_top {
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
    fn advance_column_or_page(
        &self,
        current_page: &mut Arc<Mutex<PageContent>>,
        doc: &mut PdfDocument,
        cursor_y: &mut f32,
        current_col: &mut u32,
        col_count: u32,
        col_width: f32,
        col_gap: f32,
        content_top: f32,
    ) {
        if *current_col < col_count - 1 {
            *current_col += 1;
            *cursor_y = content_top;
        } else {
            self.draw_column_rules(current_page, content_top, col_width, col_gap, col_count);
            *current_page = new_page(doc, self.page_width, self.page_height);
            *cursor_y = content_top;
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
            page.operations
                .push(PdfOp::MoveTo { x: rule_x, y: content_top });
            page.operations
                .push(PdfOp::LineTo { x: rule_x, y: self.margin_bottom });
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
            text_decoration: None,
            link_url: None,
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
            preserve_whitespace: false,
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
            preserve_whitespace: false,
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
