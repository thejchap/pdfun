use markup5ever_rcdom::{Handle, NodeData};

use crate::layout::{BlockStyle, LayoutInner, Paragraph, TextRun};

// ── Constants ────────────────────────────────────────────────

const BLOCK_ELEMENTS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "p", "div"];
const SKIP_ELEMENTS: &[&str] = &["head", "title", "style", "script", "meta", "link"];
const INLINE_BOLD: &[&str] = &["b", "strong"];
const INLINE_ITALIC: &[&str] = &["i", "em"];
const LIST_ELEMENTS: &[&str] = &["ul", "ol"];
const UL_MARKERS: &[&str] = &["-", "o", "-"];
const LIST_INDENT: f32 = 36.0;
const LIST_ITEM_SPACING: f32 = 4.0;
const MAX_NESTING_DEPTH: usize = 256;

struct UaStyle {
    font: &'static str,
    font_size: f32,
    spacing_after: f32,
}

fn ua_style(tag: &str) -> UaStyle {
    match tag {
        "h1" => UaStyle { font: "Helvetica-Bold", font_size: 24.0, spacing_after: 16.0 },
        "h2" => UaStyle { font: "Helvetica-Bold", font_size: 18.0, spacing_after: 14.0 },
        "h3" => UaStyle { font: "Helvetica-Bold", font_size: 14.0, spacing_after: 12.0 },
        "h4" => UaStyle { font: "Helvetica-Bold", font_size: 12.0, spacing_after: 10.0 },
        "h5" => UaStyle { font: "Helvetica-Bold", font_size: 10.0, spacing_after: 8.0 },
        "h6" => UaStyle { font: "Helvetica-Bold", font_size: 8.0, spacing_after: 8.0 },
        _ => UaStyle { font: "Helvetica", font_size: 12.0, spacing_after: 12.0 },
    }
}

// ── Font variant resolution ──────────────────────────────────

fn resolve_font(base_font: &'static str, bold: bool, italic: bool) -> &'static str {
    let eff_bold = bold || base_font.contains("Bold");
    let eff_italic = italic || base_font.contains("Italic") || base_font.contains("Oblique");

    if base_font.starts_with("Helvetica") {
        return match (eff_bold, eff_italic) {
            (false, false) => "Helvetica",
            (true, false) => "Helvetica-Bold",
            (false, true) => "Helvetica-Oblique",
            (true, true) => "Helvetica-BoldOblique",
        };
    }
    if base_font.starts_with("Times") {
        return match (eff_bold, eff_italic) {
            (false, false) => "Times-Roman",
            (true, false) => "Times-Bold",
            (false, true) => "Times-Italic",
            (true, true) => "Times-BoldItalic",
        };
    }
    if base_font.starts_with("Courier") {
        return match (eff_bold, eff_italic) {
            (false, false) => "Courier",
            (true, false) => "Courier-Bold",
            (false, true) => "Courier-Oblique",
            (true, true) => "Courier-BoldOblique",
        };
    }
    base_font
}

// ── List tracking ────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListType {
    Ordered,
    Unordered,
}

struct ListEntry {
    list_type: ListType,
    counter: usize,
}

// ── HTML renderer ────────────────────────────────────────────

struct HtmlRenderer<'a> {
    layout: &'a mut LayoutInner,
    runs: Vec<TextRun>,
    current_text: String,
    current_tag: Option<String>,
    skip_depth: usize,
    bold_depth: usize,
    italic_depth: usize,
    list_stack: Vec<ListEntry>,
}

impl<'a> HtmlRenderer<'a> {
    fn new(layout: &'a mut LayoutInner) -> Self {
        Self {
            layout,
            runs: Vec::new(),
            current_text: String::new(),
            current_tag: None,
            skip_depth: 0,
            bold_depth: 0,
            italic_depth: 0,
            list_stack: Vec::new(),
        }
    }

    // ── Tree walk ───────────────────────────────────────────

    fn walk_node(&mut self, handle: &Handle, depth: usize) {
        if depth > MAX_NESTING_DEPTH {
            return;
        }
        match &handle.data {
            NodeData::Document => {
                for child in handle.children.borrow().iter() {
                    self.walk_node(child, depth + 1);
                }
            }
            NodeData::Element { name, .. } => {
                let tag = name.local.as_ref();
                self.handle_start_tag(tag);
                for child in handle.children.borrow().iter() {
                    self.walk_node(child, depth + 1);
                }
                self.handle_end_tag(tag);
            }
            NodeData::Text { contents } => {
                let text = contents.borrow().to_string();
                self.handle_data(&text);
            }
            _ => {}
        }
    }

    // ── Event handlers ──────────────────────────────────────

    fn handle_start_tag(&mut self, tag: &str) {
        if SKIP_ELEMENTS.contains(&tag) {
            self.skip_depth += 1;
        } else if LIST_ELEMENTS.contains(&tag) {
            self.flush();
            let list_type = if tag == "ol" {
                ListType::Ordered
            } else {
                ListType::Unordered
            };
            self.list_stack.push(ListEntry {
                list_type,
                counter: 0,
            });
        } else if tag == "li" {
            self.flush();
            if let Some(entry) = self.list_stack.last_mut() {
                entry.counter += 1;
            }
            self.current_tag = Some("li".to_string());
        } else if BLOCK_ELEMENTS.contains(&tag) {
            self.flush();
            self.current_tag = Some(tag.to_string());
        } else if tag == "br" {
            self.flush();
        } else if INLINE_BOLD.contains(&tag) {
            self.flush_run();
            self.bold_depth += 1;
        } else if INLINE_ITALIC.contains(&tag) {
            self.flush_run();
            self.italic_depth += 1;
        }
    }

    fn handle_end_tag(&mut self, tag: &str) {
        if SKIP_ELEMENTS.contains(&tag) && self.skip_depth > 0 {
            self.skip_depth -= 1;
        } else if LIST_ELEMENTS.contains(&tag) && !self.list_stack.is_empty() {
            self.flush();
            self.list_stack.pop();
        } else if BLOCK_ELEMENTS.contains(&tag) || tag == "li" {
            self.flush();
            self.current_tag = None;
        } else if INLINE_BOLD.contains(&tag) && self.bold_depth > 0 {
            self.flush_run();
            self.bold_depth -= 1;
        } else if INLINE_ITALIC.contains(&tag) && self.italic_depth > 0 {
            self.flush_run();
            self.italic_depth -= 1;
        }
    }

    fn handle_data(&mut self, data: &str) {
        if self.skip_depth > 0 {
            return;
        }
        self.current_text.push_str(data);
    }

    // ── Flush logic ─────────────────────────────────────────

    fn flush_run(&mut self) {
        let text = std::mem::take(&mut self.current_text);
        if text.is_empty() {
            return;
        }

        let tag = self.current_tag.as_deref().unwrap_or("");
        let style = ua_style(tag);
        let bold = self.bold_depth > 0;
        let italic = self.italic_depth > 0;
        let resolved_font = resolve_font(style.font, bold, italic);

        self.runs.push(TextRun {
            text,
            font_name: resolved_font.to_string(),
            font_size: style.font_size,
            color: None,
        });
    }

    fn flush(&mut self) {
        self.flush_run();
        if self.runs.is_empty() {
            return;
        }

        let runs = std::mem::take(&mut self.runs);
        let tag = self.current_tag.as_deref();

        // List item: add marker and indentation
        // Not collapsed: bare <li> outside a list falls through to plain paragraph
        #[allow(clippy::collapsible_if)]
        if tag == Some("li") {
            if let Some(entry) = self.list_stack.last() {
                let depth = self.list_stack.len() - 1;
                let padding_left = (depth as f32 + 1.0) * LIST_INDENT;

                let marker_text = if entry.list_type == ListType::Ordered {
                    format!("{}.", entry.counter)
                } else {
                    UL_MARKERS[depth % UL_MARKERS.len()].to_string()
                };

                let marker = TextRun {
                    text: marker_text,
                    font_name: "Helvetica".to_string(),
                    font_size: 12.0,
                    color: None,
                };

                self.layout.push_paragraph(Paragraph {
                    runs,
                    line_height: None,
                    spacing_after: LIST_ITEM_SPACING,
                    style: BlockStyle {
                        padding_left,
                        ..BlockStyle::default()
                    },
                    marker: Some(marker),
                });
                return;
            }
            // Bare <li> outside a list — fall through to plain paragraph
        }

        let style = ua_style(tag.unwrap_or(""));
        self.layout.push_paragraph(Paragraph {
            runs,
            line_height: None,
            spacing_after: style.spacing_after,
            style: BlockStyle::default(),
            marker: None,
        });
    }
}

/// Walk an html5ever DOM and produce paragraphs into a `LayoutInner`.
pub fn render_dom_to_layout(document: &Handle, layout: &mut LayoutInner) {
    let mut renderer = HtmlRenderer::new(layout);
    renderer.walk_node(document, 0);
    renderer.flush();
}
