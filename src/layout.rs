use std::sync::{Arc, Mutex};

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

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

#[derive(Clone, Default, PartialEq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

// ── BlockStyle ────────────────────────────────────────────────

#[derive(Clone, Default)]
pub struct BlockStyle {
    pub color: Option<(f32, f32, f32)>,
    pub background_color: Option<(f32, f32, f32)>,
    pub padding_top: f32,
    pub padding_right: f32,
    pub padding_bottom: f32,
    pub padding_left: f32,
    pub border_width: f32,
    pub border_color: Option<(f32, f32, f32)>,
    pub text_align: TextAlign,
}

impl BlockStyle {
    fn has_any_styling(&self) -> bool {
        self.color.is_some() || self.background_color.is_some() || self.border_width > 0.0
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
}

// ── Wrapping internals ──────────────────────────────────────

struct StyledWord {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
}

struct LineSegment {
    text: String,
    font_name: String,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
    width: f32,
}

struct WrappedLine {
    segments: Vec<LineSegment>,
    total_width: f32,
    max_font_size: f32,
}

/// Wrap a sequence of `TextRun`s into lines that fit within `max_width`.
fn wrap_runs_impl(runs: &[TextRun], max_width: f32) -> Result<Vec<WrappedLine>, String> {
    // Phase 1: flatten runs into styled words
    let mut words: Vec<StyledWord> = Vec::new();
    for run in runs {
        for word in run.text.split_whitespace() {
            words.push(StyledWord {
                text: word.to_string(),
                font_name: run.font_name.clone(),
                font_size: run.font_size,
                color: run.color,
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
    blocks: Vec<Paragraph>,
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
        }
    }

    pub fn push_paragraph(&mut self, para: Paragraph) {
        let has_content = para.runs.iter().any(|r| !r.text.trim().is_empty());
        if !has_content {
            return;
        }
        self.blocks.push(para);
    }

    #[allow(clippy::too_many_lines)]
    pub fn finish(&mut self, doc: &mut PdfDocument) -> Result<(), String> {
        let content_width = self.page_width - self.margin_left - self.margin_right;
        let content_top = self.page_height - self.margin_top;
        let blocks = std::mem::take(&mut self.blocks);

        if blocks.is_empty() {
            new_page(doc, self.page_width, self.page_height);
            return Ok(());
        }

        let mut current_page = new_page(doc, self.page_width, self.page_height);
        let mut cursor_y = content_top;

        for block in &blocks {
            let text_area_width =
                content_width - block.style.padding_left - block.style.padding_right;

            let wrapped_lines = wrap_runs_impl(&block.runs, text_area_width)?;

            let max_font_size = wrapped_lines
                .iter()
                .map(|l| l.max_font_size)
                .fold(0.0_f32, f32::max);
            let line_height = block.line_height.unwrap_or(max_font_size * 1.2);

            let block_text_height = wrapped_lines.len() as f32 * line_height;
            let block_total_height =
                block.style.padding_top + block_text_height + block.style.padding_bottom;

            if cursor_y - block_total_height < self.margin_bottom && cursor_y < content_top {
                current_page = new_page(doc, self.page_width, self.page_height);
                cursor_y = content_top;
            }

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
                    x: self.margin_left,
                    y: cursor_y - block_total_height,
                    width: content_width,
                    height: block_total_height,
                });
                page.operations.push(PdfOp::Fill);
            }

            if block.style.border_width > 0.0 {
                let (r, g, b) = block.style.border_color.unwrap_or((0.0, 0.0, 0.0));
                page.operations.push(PdfOp::SetStrokeColor { r, g, b });
                page.operations
                    .push(PdfOp::SetLineWidth(block.style.border_width));
                page.operations.push(PdfOp::Rectangle {
                    x: self.margin_left,
                    y: cursor_y - block_total_height,
                    width: content_width,
                    height: block_total_height,
                });
                page.operations.push(PdfOp::Stroke);
            }

            if let Some((r, g, b)) = block.style.color {
                page.operations.push(PdfOp::SetFillColor { r, g, b });
            }

            let text_x_base = self.margin_left + block.style.padding_left;
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

                let align_offset = match block.style.text_align {
                    TextAlign::Left => 0.0,
                    TextAlign::Center => (text_area_width - line.total_width) / 2.0,
                    TextAlign::Right => text_area_width - line.total_width,
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

                line_y -= line_height;
            }

            if needs_state_wrap {
                page.operations.push(PdfOp::RestoreState);
            }

            drop(page);
            cursor_y -= block_total_height + block.spacing_after;
        }

        Ok(())
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
        };

        self.inner.blocks.push(Paragraph {
            runs: vec![run],
            line_height: Some(line_height.map_or(fs * 1.2, |h| h as f32)),
            spacing_after: spacing_after as f32,
            style: BlockStyle {
                color: None,
                background_color: background_color.map(rgb_to_f32),
                padding_top: pad_t,
                padding_right: pad_r,
                padding_bottom: pad_b,
                padding_left: pad_l,
                border_width: border_width as f32,
                border_color: border_color.map(rgb_to_f32),
                text_align: align,
            },
            marker: None,
        });
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
            },
            marker,
        });
        Ok(())
    }

    fn finish(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.doc.borrow_mut(py);
        self.inner.finish(&mut doc).map_err(PyValueError::new_err)
    }
}
