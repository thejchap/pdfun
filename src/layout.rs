use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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
}

// ── Wrapping internals ──────────────────────────────────────

struct StyledWord {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
    text_decoration: Option<css::TextDecoration>,
    link_url: Option<String>,
}

struct LineSegment {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
    width: f32,
    text_decoration: Option<css::TextDecoration>,
    link_url: Option<String>,
}

struct WrappedLine {
    segments: Vec<LineSegment>,
    total_width: f32,
    max_font_size: f32,
}

/// Wrap a sequence of `TextRun`s into lines that fit within `max_width`.
fn wrap_runs_impl(
    runs: &[TextRun],
    max_width: f32,
    preserve_whitespace: bool,
) -> Result<Vec<WrappedLine>, String> {
    if preserve_whitespace {
        return wrap_runs_preformatted(runs);
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
        let word_width = font_metrics::measure_str(&word.text, &word.font_name, word.font_size)
            .ok_or_else(|| format!("unknown font: {}", word.font_name))?;

        if current.is_empty() {
            current_width = word_width;
            current.push(word);
        } else {
            let space_width =
                font_metrics::measure_str(" ", &word.font_name, word.font_size).unwrap_or(0.0);
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
            });

            if can_merge {
                let last = segments.last_mut().unwrap();
                last.text.push(' ');
                last.text.push_str(&word.text);
                last.width =
                    font_metrics::measure_str(&last.text, &last.font_name, last.font_size)
                        .unwrap_or(0.0);
            } else {
                // Prepend space if not the first segment on the line
                let text = if segments.is_empty() {
                    word.text.clone()
                } else {
                    format!(" {}", word.text)
                };
                let width =
                    font_metrics::measure_str(&text, &word.font_name, word.font_size)
                        .unwrap_or(0.0);
                segments.push(LineSegment {
                    text,
                    font_name: word.font_name.clone(),
                    font_size: word.font_size,
                    color: word.color,
                    width,
                    text_decoration: word.text_decoration,
                    link_url: word.link_url.clone(),
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
fn wrap_runs_preformatted(runs: &[TextRun]) -> Result<Vec<WrappedLine>, String> {
    let mut full_text = String::new();
    let mut font_name = String::new();
    let mut font_size = 12.0_f32;
    let mut color = None;
    let mut text_decoration = None;
    let mut link_url = None;

    for run in runs {
        full_text.push_str(&run.text);
        font_name.clone_from(&run.font_name);
        font_size = run.font_size;
        color = run.color;
        text_decoration = run.text_decoration;
        link_url = run.link_url.clone();
    }

    let mut result: Vec<WrappedLine> = Vec::new();
    for line in full_text.split('\n') {
        let width = font_metrics::measure_str(line, &font_name, font_size)
            .ok_or_else(|| format!("unknown font: {font_name}"))?;
        let segment = LineSegment {
            text: line.to_string(),
            font_name: font_name.clone(),
            font_size,
            color,
            width,
            text_decoration,
            link_url: link_url.clone(),
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

    #[allow(clippy::too_many_lines)]
    pub fn finish(&mut self, doc: &mut PdfDocument) -> Result<(), String> {
        let content_width = self.page_width - self.margin_left - self.margin_right;
        let content_top = self.page_height - self.margin_top;
        let blocks = std::mem::take(&mut self.blocks);

        // Column calculations
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

        let mut current_page = new_page(doc, self.page_width, self.page_height);
        let mut cursor_y = content_top;
        let mut current_col: u32 = 0;

        // Helper: x-offset for a given column
        let margin_left = self.margin_left;
        let col_x = |col: u32| -> f32 {
            margin_left + col as f32 * (col_width + col_gap)
        };

        for block_enum in &blocks {
            let block = match block_enum {
                Block::Paragraph(p) => p,
                Block::Table(t) => {
                    self.render_table(
                        &mut current_page,
                        doc,
                        t,
                        &mut cursor_y,
                        &mut current_col,
                        col_count,
                        col_width,
                        col_gap,
                        content_top,
                    )?;
                    continue;
                }
                Block::Image(img) => {
                    self.render_image(
                        &mut current_page,
                        doc,
                        img,
                        &mut cursor_y,
                        &mut current_col,
                        col_count,
                        col_width,
                        col_gap,
                        content_top,
                    );
                    continue;
                }
            };
            if block.is_hr {
                if cursor_y - 12.0 < self.margin_bottom && cursor_y < content_top {
                    self.advance_column_or_page(
                        &mut current_page,
                        doc,
                        &mut cursor_y,
                        &mut current_col,
                        col_count,
                        col_width,
                        col_gap,
                        content_top,
                    );
                }
                let cx = col_x(current_col);
                let mut page = current_page.lock().unwrap();
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
                cursor_y -= block.spacing_after;
                continue;
            }

            // page-break-before: force advance before rendering (unless already at top)
            if matches!(block.style.page_break_before, Some(css::PageBreak::Always))
                && cursor_y < content_top
            {
                self.advance_column_or_page(
                    &mut current_page,
                    doc,
                    &mut cursor_y,
                    &mut current_col,
                    col_count,
                    col_width,
                    col_gap,
                    content_top,
                );
            }

            // Available width after margins
            let box_width =
                col_width - block.style.margin_left - block.style.margin_right;
            let text_area_width =
                box_width - block.style.padding_left - block.style.padding_right;

            let wrapped_lines =
                wrap_runs_impl(&block.runs, text_area_width, block.preserve_whitespace)?;

            let max_font_size = wrapped_lines
                .iter()
                .map(|l| l.max_font_size)
                .fold(0.0_f32, f32::max);
            let line_height = block.line_height.unwrap_or(max_font_size * 1.2);

            let block_text_height = wrapped_lines.len() as f32 * line_height;
            let box_height =
                block.style.padding_top + block_text_height + block.style.padding_bottom;
            let block_total_height =
                block.style.margin_top + box_height + block.style.margin_bottom;

            if cursor_y - block_total_height < self.margin_bottom && cursor_y < content_top {
                self.advance_column_or_page(
                    &mut current_page,
                    doc,
                    &mut cursor_y,
                    &mut current_col,
                    col_count,
                    col_width,
                    col_gap,
                    content_top,
                );
            }

            let cx = col_x(current_col);
            // Block box starts after left margin
            let box_x = cx + block.style.margin_left;
            // Cursor moves past top margin for box content
            cursor_y -= block.style.margin_top;

            let needs_state_wrap =
                block.style.has_any_styling() || block.runs.iter().any(|r| r.color.is_some());
            let mut page = current_page.lock().unwrap();

            for line in &wrapped_lines {
                for seg in &line.segments {
                    if !page.fonts_used.contains(&seg.font_name) {
                        page.fonts_used.push(seg.font_name.clone());
                    }
                }
            }

            if let Some(marker) = &block.marker
                && !page.fonts_used.contains(&marker.font_name)
            {
                page.fonts_used.push(marker.font_name.clone());
            }

            if needs_state_wrap {
                page.operations.push(PdfOp::SaveState);
            }

            if let Some((r, g, b)) = block.style.background_color {
                page.operations.push(PdfOp::SetFillColor { r, g, b });
                page.operations.push(PdfOp::Rectangle {
                    x: box_x,
                    y: cursor_y - box_height,
                    width: box_width,
                    height: box_height,
                });
                page.operations.push(PdfOp::Fill);
            }

            if block.style.border_width > 0.0
                && !matches!(block.style.border_style, Some(css::BorderStyle::None))
            {
                let (r, g, b) = block.style.border_color.unwrap_or((0.0, 0.0, 0.0));
                page.operations.push(PdfOp::SetStrokeColor { r, g, b });
                page.operations
                    .push(PdfOp::SetLineWidth(block.style.border_width));

                // Apply dash pattern based on border-style
                match block.style.border_style {
                    Some(css::BorderStyle::Dashed) => {
                        let dash = block.style.border_width * 3.0;
                        let gap = block.style.border_width * 2.0;
                        page.operations.push(PdfOp::SetDashPattern {
                            array: vec![dash, gap],
                            phase: 0.0,
                        });
                    }
                    Some(css::BorderStyle::Dotted) => {
                        let dot = block.style.border_width;
                        page.operations.push(PdfOp::SetDashPattern {
                            array: vec![dot, dot * 2.0],
                            phase: 0.0,
                        });
                    }
                    _ => {} // Solid or unset — no dash pattern needed
                }

                page.operations.push(PdfOp::Rectangle {
                    x: box_x,
                    y: cursor_y - box_height,
                    width: box_width,
                    height: box_height,
                });
                page.operations.push(PdfOp::Stroke);
            }

            if let Some((r, g, b)) = block.style.color {
                page.operations.push(PdfOp::SetFillColor { r, g, b });
            }

            let text_x_base = box_x + block.style.padding_left;
            let mut line_y = cursor_y - block.style.padding_top;
            let marker_gap = 6.0_f32;

            for (line_idx, line) in wrapped_lines.iter().enumerate() {
                let baseline_y = line_y - line.max_font_size;

                if line_idx == 0
                    && let Some(marker) = &block.marker
                {
                    let marker_width = font_metrics::measure_str(
                        &marker.text,
                        &marker.font_name,
                        marker.font_size,
                    )
                    .unwrap_or(0.0);
                    let marker_x = text_x_base - marker_gap - marker_width;

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

                // For justify: all but the last line get word-spacing widening
                let is_justified_line = matches!(block.style.text_align, TextAlign::Justify)
                    && line_idx + 1 < wrapped_lines.len();

                let word_spacing = if is_justified_line {
                    // Count spaces across all segments on this line
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

                let align_offset = match block.style.text_align {
                    TextAlign::Left | TextAlign::Justify => 0.0,
                    TextAlign::Center => (text_area_width - line.total_width) / 2.0,
                    TextAlign::Right => text_area_width - line.total_width,
                };

                // Apply/reset word spacing for justified lines
                if is_justified_line {
                    page.operations.push(PdfOp::SetWordSpacing(word_spacing));
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

                    // Record link annotation rect if this segment is part of an <a>
                    if let Some(url) = &segment.link_url {
                        page.links.push(crate::LinkAnnotation {
                            x,
                            y: baseline_y - segment.font_size * 0.2,
                            width: segment.width,
                            height: segment.font_size * 1.1,
                            url: url.clone(),
                        });
                    }

                    // Draw text-decoration lines (underline and/or line-through)
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

                    // Advance x, including extra word-spacing (Tw) for justified lines
                    let spaces_in_segment = segment.text.matches(' ').count() as f32;
                    x += segment.width + spaces_in_segment * word_spacing;
                }

                // Reset word spacing after a justified line
                if is_justified_line {
                    page.operations.push(PdfOp::SetWordSpacing(0.0));
                }

                line_y -= line_height;
            }

            if needs_state_wrap {
                page.operations.push(PdfOp::RestoreState);
            }

            drop(page);
            cursor_y -= box_height + block.style.margin_bottom + block.spacing_after;

            // page-break-after: force advance after rendering
            if matches!(block.style.page_break_after, Some(css::PageBreak::Always)) {
                self.advance_column_or_page(
                    &mut current_page,
                    doc,
                    &mut cursor_y,
                    &mut current_col,
                    col_count,
                    col_width,
                    col_gap,
                    content_top,
                );
            }
        }

        // Draw column rules on the last page
        self.draw_column_rules(&current_page, content_top, col_width, col_gap, col_count);

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

        *cursor_y -= table.style.margin_top;

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
                let wrapped = wrap_runs_impl(&cell.runs, text_w, false)?;
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

        *cursor_y -= table.style.margin_bottom + table.spacing_after;
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

        let total_height = img.style.margin_top + draw_h + img.style.margin_bottom;

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
        }

        *cursor_y -= img.style.margin_top;
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
}
