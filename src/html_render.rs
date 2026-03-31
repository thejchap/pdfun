use markup5ever_rcdom::{Handle, NodeData};

use crate::css::{self, ComputedStyle, ElementInfo, FontStyle, Stylesheet};
use crate::layout::{BlockStyle, LayoutInner, Paragraph, TextRun};

// ── Constants ────────────────────────────────────────────────

const BLOCK_ELEMENTS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6", "p", "div", "blockquote", "pre"];
const SKIP_ELEMENTS: &[&str] = &["head", "title", "style", "script", "meta", "link"];
const INLINE_BOLD: &[&str] = &["b", "strong"];
const INLINE_ITALIC: &[&str] = &["i", "em"];
const INLINE_CODE: &[&str] = &["code", "kbd", "samp"];
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
        "pre" => UaStyle { font: "Courier", font_size: 12.0, spacing_after: 12.0 },
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

// ── Font family helpers ──────────────────────────────────────

/// Get the root font family (strip Bold/Italic/Oblique variant).
/// Used when CSS explicitly overrides font-weight or font-style.
fn font_family_root(font: &'static str) -> &'static str {
    if font.starts_with("Helvetica") {
        return "Helvetica";
    }
    if font.starts_with("Times") {
        return "Times-Roman";
    }
    if font.starts_with("Courier") {
        return "Courier";
    }
    font
}

fn map_css_font_family(family: &str) -> Option<&'static str> {
    for name in family.split(',') {
        let lower = name.trim().trim_matches(|c| c == '\'' || c == '"').to_ascii_lowercase();
        match lower.as_str() {
            "serif" | "times" | "times new roman" | "times-roman" => return Some("Times-Roman"),
            "sans-serif" | "helvetica" | "arial" => return Some("Helvetica"),
            "monospace" | "courier" | "courier new" => return Some("Courier"),
            _ => continue,
        }
    }
    None
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

// ── Style block extraction ──────────────────────────────────

/// Recursively extract text content from all `<style>` elements in the DOM.
fn extract_style_blocks(handle: &Handle) -> String {
    let mut css = String::new();
    extract_style_blocks_inner(handle, &mut css);
    css
}

fn extract_style_blocks_inner(handle: &Handle, css: &mut String) {
    match &handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                extract_style_blocks_inner(child, css);
            }
        }
        NodeData::Element { name, .. } => {
            if name.local.as_ref() == "style" {
                // Collect text children
                for child in handle.children.borrow().iter() {
                    if let NodeData::Text { contents } = &child.data {
                        css.push_str(&contents.borrow());
                        css.push('\n');
                    }
                }
            } else {
                for child in handle.children.borrow().iter() {
                    extract_style_blocks_inner(child, css);
                }
            }
        }
        _ => {}
    }
}

/// Extract class list and id from an element handle.
fn extract_element_attrs(handle: &Handle) -> (Vec<String>, Option<String>) {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let attrs = attrs.borrow();
        let mut classes = Vec::new();
        let mut id = None;
        for attr in attrs.iter() {
            let name = attr.name.local.as_ref();
            if name == "class" {
                classes = attr.value.split_whitespace().map(String::from).collect();
            } else if name == "id" {
                id = Some(attr.value.to_string());
            }
        }
        (classes, id)
    } else {
        (Vec::new(), None)
    }
}

/// Build ancestor chain from current element walking up the DOM tree.
fn build_ancestors(handle: &Handle) -> Vec<(String, Vec<String>, Option<String>)> {
    let mut ancestors = Vec::new();
    let mut current = handle.parent.take();
    // Put back the parent (Cell::take leaves None)
    handle.parent.set(current.clone());

    while let Some(weak) = current {
        let Some(node) = weak.upgrade() else { break };
        match &node.data {
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.as_ref().to_string();
                let attrs_ref = attrs.borrow();
                let mut classes = Vec::new();
                let mut id = None;
                for attr in attrs_ref.iter() {
                    let attr_name = attr.name.local.as_ref();
                    if attr_name == "class" {
                        classes = attr.value.split_whitespace().map(String::from).collect();
                    } else if attr_name == "id" {
                        id = Some(attr.value.to_string());
                    }
                }
                drop(attrs_ref);
                ancestors.push((tag, classes, id));
                let parent = node.parent.take();
                node.parent.set(parent.clone());
                current = parent;
            }
            NodeData::Document => break,
            _ => {
                let parent = node.parent.take();
                node.parent.set(parent.clone());
                current = parent;
            }
        }
    }
    ancestors
}

// ── Style attribute extraction ──────────────────────────────

fn extract_style_attr(handle: &Handle) -> Option<ComputedStyle> {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let attrs = attrs.borrow();
        let style_value = attrs.iter().find_map(|attr| {
            if attr.name.local.as_ref() == "style" {
                Some(attr.value.to_string())
            } else {
                None
            }
        })?;
        Some(crate::css::parse_inline_style(&style_value))
    } else {
        None
    }
}

// ── HTML renderer ────────────────────────────────────────────

struct HtmlRenderer<'a> {
    layout: &'a mut LayoutInner,
    stylesheet: Stylesheet,
    runs: Vec<TextRun>,
    current_text: String,
    current_tag: Option<String>,
    skip_depth: usize,
    bold_depth: usize,
    italic_depth: usize,
    code_depth: usize,
    pre_depth: usize,
    list_stack: Vec<ListEntry>,
    block_style: Option<ComputedStyle>,
    inline_styles: Vec<ComputedStyle>,
    /// Track whether each bold/italic/code/span tag pushed a style onto `inline_styles`.
    /// True = this tag pushed a style; False = no style attribute.
    bold_had_style: Vec<bool>,
    italic_had_style: Vec<bool>,
    code_had_style: Vec<bool>,
    span_had_style: Vec<bool>,
    /// Inherited CSS style from body/html (font-family, font-size, line-height).
    inherited_style: Option<ComputedStyle>,
}

impl<'a> HtmlRenderer<'a> {
    fn new(layout: &'a mut LayoutInner, stylesheet: Stylesheet) -> Self {
        Self {
            layout,
            stylesheet,
            runs: Vec::new(),
            current_text: String::new(),
            current_tag: None,
            skip_depth: 0,
            bold_depth: 0,
            italic_depth: 0,
            code_depth: 0,
            pre_depth: 0,
            list_stack: Vec::new(),
            block_style: None,
            inline_styles: Vec::new(),
            bold_had_style: Vec::new(),
            italic_had_style: Vec::new(),
            code_had_style: Vec::new(),
            span_had_style: Vec::new(),
            inherited_style: None,
        }
    }

    /// Get the effective inline style by checking the `inline_styles` stack,
    /// then falling back to `block_style`.
    fn effective_style(&self) -> Option<&ComputedStyle> {
        self.inline_styles.last().or(self.block_style.as_ref())
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
                let inline_style = extract_style_attr(handle);

                // Match stylesheet rules and merge with inline style
                let merged_style = if self.stylesheet.rules.is_empty() {
                    inline_style
                } else {
                    let (classes, id) = extract_element_attrs(handle);
                    let ancestors_owned = build_ancestors(handle);
                    let ancestors: Vec<(&str, Vec<&str>, Option<&str>)> = ancestors_owned
                        .iter()
                        .map(|(t, c, i)| {
                            (
                                t.as_str(),
                                c.iter().map(String::as_str).collect(),
                                i.as_deref(),
                            )
                        })
                        .collect();
                    let elem = ElementInfo {
                        tag,
                        classes: classes.iter().map(String::as_str).collect(),
                        id: id.as_deref(),
                        ancestors,
                    };
                    let mut matched = css::match_rules(&elem, &self.stylesheet);
                    if let Some(inline) = &inline_style {
                        css::merge_style(&mut matched, inline);
                    }
                    if matched.has_any_property() {
                        Some(matched)
                    } else {
                        inline_style
                    }
                };

                self.handle_start_tag(tag, merged_style);
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

    fn handle_start_tag(&mut self, tag: &str, inline_style: Option<ComputedStyle>) {
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
            self.block_style = inline_style;
        } else if tag == "body" || tag == "html" {
            if let Some(ref style) = inline_style {
                if let Some(count) = style.column_count {
                    self.layout.column_count = count;
                }
                if let Some(gap) = style.column_gap {
                    self.layout.column_gap = gap.resolve(12.0);
                }
                if let Some(width) = style.column_rule_width {
                    self.layout.column_rule_width = width.resolve(12.0);
                }
                if let Some(color) = style.column_rule_color {
                    self.layout.column_rule_color = Some(color);
                }
            }
            if let Some(style) = inline_style {
                self.inherited_style = Some(style);
            }
        } else if BLOCK_ELEMENTS.contains(&tag) {
            self.flush();
            self.current_tag = Some(tag.to_string());
            self.block_style = inline_style;
            if tag == "pre" {
                self.pre_depth += 1;
            }
        } else if tag == "br" {
            self.flush();
        } else if tag == "hr" {
            self.flush();
            self.layout.push_hr();
        } else if tag == "span" {
            let has_style = inline_style.is_some();
            if let Some(style) = inline_style {
                self.flush_run();
                self.inline_styles.push(style);
            }
            self.span_had_style.push(has_style);
        } else if INLINE_BOLD.contains(&tag) {
            self.flush_run();
            self.bold_depth += 1;
            let has_style = inline_style.is_some();
            if let Some(style) = inline_style {
                self.inline_styles.push(style);
            }
            self.bold_had_style.push(has_style);
        } else if INLINE_ITALIC.contains(&tag) {
            self.flush_run();
            self.italic_depth += 1;
            let has_style = inline_style.is_some();
            if let Some(style) = inline_style {
                self.inline_styles.push(style);
            }
            self.italic_had_style.push(has_style);
        } else if INLINE_CODE.contains(&tag) {
            self.flush_run();
            self.code_depth += 1;
            let has_style = inline_style.is_some();
            if let Some(style) = inline_style {
                self.inline_styles.push(style);
            }
            self.code_had_style.push(has_style);
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
            if tag == "pre" && self.pre_depth > 0 {
                self.pre_depth -= 1;
            }
            self.current_tag = None;
            self.block_style = None;
        } else if tag == "span" && !self.span_had_style.is_empty() {
            self.flush_run();
            if self.span_had_style.pop() == Some(true) {
                self.inline_styles.pop();
            }
        } else if INLINE_BOLD.contains(&tag) && self.bold_depth > 0 {
            self.flush_run();
            self.bold_depth -= 1;
            if self.bold_had_style.pop() == Some(true) {
                self.inline_styles.pop();
            }
        } else if INLINE_ITALIC.contains(&tag) && self.italic_depth > 0 {
            self.flush_run();
            self.italic_depth -= 1;
            if self.italic_had_style.pop() == Some(true) {
                self.inline_styles.pop();
            }
        } else if INLINE_CODE.contains(&tag) && self.code_depth > 0 {
            self.flush_run();
            self.code_depth -= 1;
            if self.code_had_style.pop() == Some(true) {
                self.inline_styles.pop();
            }
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
        let ua = ua_style(tag);
        let effective = self.effective_style();

        // Resolve font size: CSS override → inherited → UA default
        let font_size = effective
            .and_then(|s| s.font_size)
            .or_else(|| self.inherited_style.as_ref().and_then(|s| s.font_size))
            .map_or(ua.font_size, |len| len.resolve(ua.font_size));

        // Check if CSS explicitly sets font-weight or font-style
        let css_weight = effective.and_then(|s| s.font_weight);
        let css_style = effective.and_then(|s| s.font_style);
        let css_overrides_variant = css_weight.is_some() || css_style.is_some();

        // Resolve bold: CSS font-weight overrides tag/depth-implied bold
        let bold = if let Some(fw) = css_weight {
            fw.is_bold()
        } else {
            self.bold_depth > 0
        };

        // Resolve italic: CSS font-style overrides tag/depth-implied italic
        let italic = if let Some(fs) = css_style {
            matches!(fs, FontStyle::Italic)
        } else {
            self.italic_depth > 0
        };

        // Resolve font family: CSS → code depth → map generic names → UA default
        // When CSS overrides font-weight/style, strip variant from UA font
        // so resolve_font builds the correct variant from scratch.
        let ua_font = if css_overrides_variant {
            font_family_root(ua.font)
        } else {
            ua.font
        };
        let code_font = if self.code_depth > 0 { Some("Courier") } else { None };
        let base_font = effective
            .and_then(|s| s.font_family.as_deref())
            .and_then(map_css_font_family)
            .or(code_font)
            .or_else(|| {
                self.inherited_style
                    .as_ref()
                    .and_then(|s| s.font_family.as_deref())
                    .and_then(map_css_font_family)
            })
            .unwrap_or(ua_font);

        let resolved_font = resolve_font(base_font, bold, italic);

        // Resolve text color from inline style (run-level)
        let color = effective.and_then(|s| s.color);

        self.runs.push(TextRun {
            text,
            font_name: resolved_font.to_string(),
            font_size,
            color,
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

                let mut list_block_style = BlockStyle {
                    padding_left,
                    ..BlockStyle::default()
                };

                // Apply CSS block properties from li's inline style
                self.apply_block_css(&mut list_block_style);

                self.layout.push_paragraph(Paragraph {
                    runs,
                    line_height: self.resolve_line_height(),
                    spacing_after: self.resolve_spacing_after(LIST_ITEM_SPACING),
                    style: list_block_style,
                    marker: Some(marker),
                    is_hr: false,
                    preserve_whitespace: false,
                });
                return;
            }
            // Bare <li> outside a list — fall through to plain paragraph
        }

        let ua = ua_style(tag.unwrap_or(""));
        let mut block_style = BlockStyle::default();

        if tag == Some("blockquote") {
            block_style.padding_left = 20.0;
        }

        // Apply CSS block properties
        self.apply_block_css(&mut block_style);

        self.layout.push_paragraph(Paragraph {
            runs,
            line_height: self.resolve_line_height(),
            spacing_after: self.resolve_spacing_after(ua.spacing_after),
            style: block_style,
            marker: None,
            is_hr: false,
            preserve_whitespace: self.pre_depth > 0,
        });
    }

    // ── CSS application helpers ─────────────────────────────

    fn resolve_em_base(&self) -> f32 {
        let tag = self.current_tag.as_deref().unwrap_or("");
        ua_style(tag).font_size
    }

    fn resolve_line_height(&self) -> Option<f32> {
        let em = self.resolve_em_base();
        self.block_style
            .as_ref()
            .and_then(|s| s.line_height)
            .or_else(|| self.inherited_style.as_ref().and_then(|s| s.line_height))
            .map(|len| len.resolve(em))
    }

    fn resolve_spacing_after(&self, default: f32) -> f32 {
        let em = self.resolve_em_base();
        self.block_style
            .as_ref()
            .and_then(|s| s.margin_bottom)
            .map_or(default, |len| len.resolve(em))
    }

    fn apply_block_css(&self, block_style: &mut BlockStyle) {
        let Some(style) = self.block_style.as_ref() else {
            return;
        };
        let em = self.resolve_em_base();

        if let Some(c) = style.color {
            block_style.color = Some(c);
        }
        if let Some(c) = style.background_color {
            block_style.background_color = Some(c);
        }
        if let Some(a) = &style.text_align {
            block_style.text_align = a.clone();
        }
        if let Some(len) = style.padding_top {
            block_style.padding_top = len.resolve(em);
        }
        if let Some(len) = style.padding_right {
            block_style.padding_right = len.resolve(em);
        }
        if let Some(len) = style.padding_bottom {
            block_style.padding_bottom = len.resolve(em);
        }
        if let Some(len) = style.padding_left {
            block_style.padding_left = len.resolve(em);
        }
        if let Some(len) = style.border_width {
            block_style.border_width = len.resolve(em);
        }
        if let Some(c) = style.border_color {
            block_style.border_color = Some(c);
        }
    }
}

/// Walk an html5ever DOM and produce paragraphs into a `LayoutInner`.
/// Returns the `PageStyle` extracted from any `@page` CSS rule.
pub fn render_dom_to_layout(document: &Handle, layout: &mut LayoutInner) -> css::PageStyle {
    let css_text = extract_style_blocks(document);
    let stylesheet = css::parse_stylesheet(&css_text);
    let page_style = stylesheet.page_style.clone();
    let mut renderer = HtmlRenderer::new(layout, stylesheet);
    renderer.walk_node(document, 0);
    renderer.flush();
    page_style
}
