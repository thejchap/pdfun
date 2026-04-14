use markup5ever_rcdom::{Handle, NodeData};

use crate::css::{self, ComputedStyle, ElementInfo, FontStyle, Stylesheet};
use crate::layout::{BlockStyle, LayoutInner, Paragraph, TextRun};

// ── Constants ────────────────────────────────────────────────

const BLOCK_ELEMENTS: &[&str] = &[
    "h1", "h2", "h3", "h4", "h5", "h6", "p", "div", "blockquote", "pre",
    "article", "section", "nav", "header", "footer", "aside", "main",
];
const SKIP_ELEMENTS: &[&str] = &["head", "title", "style", "script", "meta", "link"];
const LIST_ELEMENTS: &[&str] = &["ul", "ol"];

/// Inline tag categories — determines which depth counter and had-style
/// stack a tag uses during inline-context tracking.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineKind {
    Bold,
    Italic,
    Code,
    Span,
    Link,
}

/// Single source of truth for inline tag dispatch. Adding a new inline tag
/// (e.g. `<mark>`) means one row here, not parallel edits to start/end
/// handlers and `flush_run`.
static INLINE_TAGS: &[(&str, InlineKind)] = &[
    ("b", InlineKind::Bold),
    ("strong", InlineKind::Bold),
    ("i", InlineKind::Italic),
    ("em", InlineKind::Italic),
    ("code", InlineKind::Code),
    ("kbd", InlineKind::Code),
    ("samp", InlineKind::Code),
    ("span", InlineKind::Span),
    ("a", InlineKind::Link),
];

fn lookup_inline(tag: &str) -> Option<InlineKind> {
    INLINE_TAGS
        .iter()
        .find(|(t, _)| *t == tag)
        .map(|(_, k)| *k)
}
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
    style_override: Option<css::ListStyleType>,
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

/// Convert 1-based counter to alphabetic marker (a, b, ..., z, aa, ab, ...).
fn to_alpha(n: usize, upper: bool) -> String {
    if n == 0 {
        return String::new();
    }
    let base = if upper { b'A' } else { b'a' };
    let mut n = n;
    let mut buf = Vec::new();
    while n > 0 {
        n -= 1;
        buf.push(base + (n % 26) as u8);
        n /= 26;
    }
    buf.reverse();
    String::from_utf8(buf).unwrap()
}

/// Convert 1-based counter to roman numeral.
fn to_roman(n: usize, upper: bool) -> String {
    if n == 0 || n > 3999 {
        return n.to_string();
    }
    const PAIRS: &[(usize, &str, &str)] = &[
        (1000, "M", "m"),
        (900, "CM", "cm"),
        (500, "D", "d"),
        (400, "CD", "cd"),
        (100, "C", "c"),
        (90, "XC", "xc"),
        (50, "L", "l"),
        (40, "XL", "xl"),
        (10, "X", "x"),
        (9, "IX", "ix"),
        (5, "V", "v"),
        (4, "IV", "iv"),
        (1, "I", "i"),
    ];
    let mut n = n;
    let mut out = String::new();
    for &(val, up, lo) in PAIRS {
        while n >= val {
            out.push_str(if upper { up } else { lo });
            n -= val;
        }
    }
    out
}

/// Extract src, width, height attributes from an `<img>` element.
fn extract_img_attrs(handle: &Handle) -> (Option<String>, Option<f32>, Option<f32>) {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let attrs = attrs.borrow();
        let mut src = None;
        let mut width = None;
        let mut height = None;
        for attr in attrs.iter() {
            match attr.name.local.as_ref() {
                "src" => {
                    let v = attr.value.to_string();
                    if !v.is_empty() {
                        src = Some(v);
                    }
                }
                "width" => {
                    if let Ok(v) = attr.value.parse::<f32>() {
                        width = Some(v);
                    }
                }
                "height" => {
                    if let Ok(v) = attr.value.parse::<f32>() {
                        height = Some(v);
                    }
                }
                _ => {}
            }
        }
        (src, width, height)
    } else {
        (None, None, None)
    }
}

fn extract_href_attr(handle: &Handle) -> Option<String> {
    if let NodeData::Element { attrs, .. } = &handle.data {
        let attrs = attrs.borrow();
        attrs.iter().find_map(|attr| {
            if attr.name.local.as_ref() == "href" {
                let href = attr.value.to_string();
                if href.is_empty() { None } else { Some(href) }
            } else {
                None
            }
        })
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
    /// Stack of link hrefs, pushed/popped as we enter/leave `<a>` elements.
    link_stack: Vec<String>,
    /// Temporary storage for href extracted in walk_node, consumed by handle_start_tag.
    pending_href: Option<String>,
    /// Stack of had-style flags for `<a>` tags.
    link_had_style: Vec<bool>,
    /// Stack of inherited CSS styles, pushed/popped as we enter/leave DOM elements.
    inherit_stack: Vec<ComputedStyle>,
    /// Base directory for resolving relative image paths.
    base_dir: Option<std::path::PathBuf>,
    /// Non-fatal issues collected during rendering (image load failures, etc.).
    warnings: Vec<String>,
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
            link_stack: Vec::new(),
            pending_href: None,
            link_had_style: Vec::new(),
            inherit_stack: Vec::new(),
            base_dir: None,
            warnings: Vec::new(),
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

                // Skip elements with display: none (and their entire subtree)
                if let Some(ref style) = merged_style {
                    if style.display == Some(css::DisplayValue::None) {
                        return;
                    }
                }

                // Push inherited style for this element's subtree
                let parent = self.inherit_stack.last().cloned().unwrap_or_default();
                let inherited = parent.inherit_into(&merged_style);
                self.inherit_stack.push(inherited);

                // Tables are processed as a self-contained subtree.
                if tag == "table" {
                    self.flush();
                    self.build_and_push_table(handle, merged_style);
                    self.inherit_stack.pop();
                    return;
                }

                // Images
                if tag == "img" {
                    self.flush();
                    self.build_and_push_image(handle, merged_style.as_ref());
                    self.inherit_stack.pop();
                    return;
                }

                // Extract href for <a> tags before handling
                if tag == "a" {
                    self.pending_href = extract_href_attr(handle);
                }

                self.handle_start_tag(tag, merged_style);
                for child in handle.children.borrow().iter() {
                    self.walk_node(child, depth + 1);
                }
                self.handle_end_tag(tag);

                self.inherit_stack.pop();
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
            let style_override = inline_style
                .as_ref()
                .and_then(|s| s.list_style_type);
            self.list_stack.push(ListEntry {
                list_type,
                counter: 0,
                style_override,
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
        } else if let Some(kind) = lookup_inline(tag) {
            self.flush_run();
            match kind {
                InlineKind::Bold => self.bold_depth += 1,
                InlineKind::Italic => self.italic_depth += 1,
                InlineKind::Code => self.code_depth += 1,
                InlineKind::Link => {
                    if let Some(href) = self.pending_href.take() {
                        self.link_stack.push(href);
                    }
                }
                InlineKind::Span => {}
            }
            let has_style = inline_style.is_some();
            if let Some(style) = inline_style {
                self.inline_styles.push(style);
            }
            self.had_style_stack(kind).push(has_style);
        }
    }

    /// Return the had-style stack for a given inline kind.
    fn had_style_stack(&mut self, kind: InlineKind) -> &mut Vec<bool> {
        match kind {
            InlineKind::Bold => &mut self.bold_had_style,
            InlineKind::Italic => &mut self.italic_had_style,
            InlineKind::Code => &mut self.code_had_style,
            InlineKind::Span => &mut self.span_had_style,
            InlineKind::Link => &mut self.link_had_style,
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
        } else if let Some(kind) = lookup_inline(tag) {
            // Only act if the matching stack is non-empty (defensive against
            // mismatched/extra closing tags).
            if self.had_style_stack(kind).is_empty() {
                return;
            }
            match kind {
                InlineKind::Bold if self.bold_depth == 0 => return,
                InlineKind::Italic if self.italic_depth == 0 => return,
                InlineKind::Code if self.code_depth == 0 => return,
                _ => {}
            }
            self.flush_run();
            match kind {
                InlineKind::Bold => self.bold_depth -= 1,
                InlineKind::Italic => self.italic_depth -= 1,
                InlineKind::Code => self.code_depth -= 1,
                InlineKind::Link => {
                    self.link_stack.pop();
                }
                InlineKind::Span => {}
            }
            if self.had_style_stack(kind).pop() == Some(true) {
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

        let inherited = self.inherit_stack.last();

        // Resolve font size: CSS override → inherited → UA default
        let font_size = effective
            .and_then(|s| s.font_size)
            .or_else(|| inherited.and_then(|s| s.font_size))
            .map_or(ua.font_size, |len| len.resolve(ua.font_size));

        // Resolve bold: CSS → inherited → tag/depth-implied bold
        let css_weight = effective
            .and_then(|s| s.font_weight)
            .or_else(|| inherited.and_then(|s| s.font_weight));
        // Resolve italic: CSS → inherited → tag/depth-implied italic
        let css_style = effective
            .and_then(|s| s.font_style)
            .or_else(|| inherited.and_then(|s| s.font_style));
        let css_overrides_variant = css_weight.is_some() || css_style.is_some();

        let bold = if let Some(fw) = css_weight {
            fw.is_bold()
        } else {
            self.bold_depth > 0
        };

        let italic = if let Some(fs) = css_style {
            matches!(fs, FontStyle::Italic)
        } else {
            self.italic_depth > 0
        };

        // Resolve font family: CSS → code depth → inherited → UA default
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
                inherited
                    .and_then(|s| s.font_family.as_deref())
                    .and_then(map_css_font_family)
            })
            .unwrap_or(ua_font);

        let resolved_font = resolve_font(base_font, bold, italic);

        // Resolve text color: CSS → inherited
        let color = effective
            .and_then(|s| s.color)
            .or_else(|| inherited.and_then(|s| s.color));

        // Resolve text-decoration: CSS → inherited
        let text_decoration = effective
            .and_then(|s| s.text_decoration)
            .or_else(|| inherited.and_then(|s| s.text_decoration))
            .filter(|td| td.underline || td.line_through);

        let link_url = self.link_stack.last().cloned();

        self.runs.push(TextRun {
            text,
            font_name: resolved_font.to_string(),
            font_size,
            color,
            text_decoration,
            link_url,
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

                // Resolve list-style-type: entry override → inherited → default
                let inherited_lst = self
                    .inherit_stack
                    .last()
                    .and_then(|s| s.list_style_type);
                let resolved_lst = entry.style_override.or(inherited_lst).unwrap_or_else(|| {
                    if entry.list_type == ListType::Ordered {
                        css::ListStyleType::Decimal
                    } else {
                        match depth % 3 {
                            0 => css::ListStyleType::Disc,
                            1 => css::ListStyleType::Circle,
                            _ => css::ListStyleType::Square,
                        }
                    }
                });

                // Built-in PDF fonts use WinAnsi encoding, which has limited
                // Unicode support. Use ASCII-safe marker glyphs.
                let marker_text = match resolved_lst {
                    css::ListStyleType::None => String::new(),
                    css::ListStyleType::Disc => "*".to_string(),
                    css::ListStyleType::Circle => "o".to_string(),
                    css::ListStyleType::Square => "#".to_string(),
                    css::ListStyleType::Decimal => format!("{}.", entry.counter),
                    css::ListStyleType::LowerAlpha => {
                        format!("{}.", to_alpha(entry.counter, false))
                    }
                    css::ListStyleType::UpperAlpha => {
                        format!("{}.", to_alpha(entry.counter, true))
                    }
                    css::ListStyleType::LowerRoman => {
                        format!("{}.", to_roman(entry.counter, false))
                    }
                    css::ListStyleType::UpperRoman => {
                        format!("{}.", to_roman(entry.counter, true))
                    }
                };

                let marker = TextRun {
                    text: marker_text,
                    font_name: "Helvetica".to_string(),
                    font_size: 12.0,
                    color: None,
                    text_decoration: None,
                    link_url: None,
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

    // ── Table building ──────────────────────────────────────

    fn build_and_push_table(
        &mut self,
        table_handle: &Handle,
        table_style: Option<ComputedStyle>,
    ) {
        use crate::layout::{Table, TableRow};

        let em = 12.0_f32;
        let inherited = self.inherit_stack.last().cloned().unwrap_or_default();
        let inherited_with_table = inherited.inherit_into(&table_style);

        let mut table_block_style = BlockStyle::default();
        let spacing_after = 12.0_f32;

        if let Some(style) = table_style.as_ref() {
            if let Some(len) = style.margin_top {
                table_block_style.margin_top = len.resolve(em);
            }
            if let Some(len) = style.margin_right {
                table_block_style.margin_right = len.resolve(em);
            }
            if let Some(len) = style.margin_bottom {
                table_block_style.margin_bottom = len.resolve(em);
            }
            if let Some(len) = style.margin_left {
                table_block_style.margin_left = len.resolve(em);
            }
        }

        let default_line_height = inherited_with_table
            .line_height
            .map(|len| len.resolve(em));

        let mut rows: Vec<TableRow> = Vec::new();
        collect_table_rows(
            table_handle,
            &inherited_with_table,
            &mut rows,
            &self.stylesheet,
            &[],
        );

        if rows.is_empty() {
            return;
        }

        let table = Table {
            rows,
            style: table_block_style,
            spacing_after,
            default_line_height,
        };
        self.layout.push_table(table);
    }

    fn build_and_push_image(
        &mut self,
        img_handle: &Handle,
        inline_style: Option<&ComputedStyle>,
    ) {
        use crate::layout::ImageBlock;

        // Extract src and optional width/height attributes
        let (src, attr_w, attr_h) = extract_img_attrs(img_handle);
        let Some(src) = src else {
            return;
        };

        // Resolve path relative to base_dir if given, else CWD
        let path = if let Some(base) = &self.base_dir {
            base.join(&src)
        } else {
            std::path::PathBuf::from(&src)
        };

        let img_data = match crate::image::load_from_path(&path) {
            Ok(data) => data,
            Err(e) => {
                self.warnings.push(format!("image {src}: {e}"));
                return;
            }
        };

        let intrinsic_w = img_data.width as f32;
        let intrinsic_h = img_data.height as f32;

        // Resolve display width/height: CSS → attribute → intrinsic
        let em = 12.0_f32;
        let css_w = inline_style.and_then(|s| s.padding_left.map(|_| 0.0));
        let _ = css_w;

        let css_width = inline_style
            .and_then(|s| {
                // We don't have dedicated width/height fields on ComputedStyle
                // yet — layout width is not CSS-driven for images. Use attrs.
                let _ = s;
                None::<f32>
            });

        let width = css_width
            .or(attr_w)
            .unwrap_or(intrinsic_w);

        let height = if let Some(h) = attr_h {
            h
        } else if let Some(w) = attr_w {
            // Preserve aspect ratio
            intrinsic_h * (w / intrinsic_w)
        } else {
            intrinsic_h
        };

        // Store in layout's image vec and get an index
        let image_index = self.layout.images.len();
        self.layout.images.push(img_data);

        let mut style = BlockStyle::default();
        if let Some(s) = inline_style {
            if let Some(len) = s.margin_top {
                style.margin_top = len.resolve(em);
            }
            if let Some(len) = s.margin_right {
                style.margin_right = len.resolve(em);
            }
            if let Some(len) = s.margin_bottom {
                style.margin_bottom = len.resolve(em);
            }
            if let Some(len) = s.margin_left {
                style.margin_left = len.resolve(em);
            }
        }

        self.layout.push_image(ImageBlock {
            image_index,
            width,
            height,
            spacing_after: 6.0,
            style,
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
            .or_else(|| self.inherit_stack.last().and_then(|s| s.line_height))
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
        let inherited = self.inherit_stack.last();
        let em = self.resolve_em_base();

        if let Some(style) = self.block_style.as_ref() {
            if let Some(c) = style.color {
                block_style.color = Some(c);
            }
            if let Some(c) = style.background_color {
                block_style.background_color = Some(c);
            }
            if let Some(a) = &style.text_align {
                block_style.text_align = a.clone();
            }
            if let Some(len) = style.margin_top {
                block_style.margin_top = len.resolve(em);
            }
            if let Some(len) = style.margin_right {
                block_style.margin_right = len.resolve(em);
            }
            if let Some(len) = style.margin_bottom {
                block_style.margin_bottom = len.resolve(em);
            }
            if let Some(len) = style.margin_left {
                block_style.margin_left = len.resolve(em);
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
            if let Some(bs) = style.border_style {
                block_style.border_style = Some(bs);
            }
            if let Some(pb) = style.page_break_before {
                block_style.page_break_before = Some(pb);
            }
            if let Some(pb) = style.page_break_after {
                block_style.page_break_after = Some(pb);
            }
        }

        // Inherit color and text-align from ancestor if not explicitly set
        if let Some(inh) = inherited {
            let block_has_color = self.block_style.as_ref().is_some_and(|s| s.color.is_some());
            if !block_has_color && block_style.color.is_none() {
                if let Some(c) = inh.color {
                    block_style.color = Some(c);
                }
            }
            let block_has_align = self.block_style.as_ref().is_some_and(|s| s.text_align.is_some());
            if !block_has_align {
                if let Some(a) = &inh.text_align {
                    block_style.text_align = a.clone();
                }
            }
        }
    }
}

/// Recursively walk a `<table>` subtree and collect `TableRow`s from any
/// `<tr>` descendants. Does not interpret `<thead>`/`<tbody>`/`<tfoot>`
/// specially — their `<tr>` children are collected in document order.
fn collect_table_rows(
    handle: &Handle,
    inherited: &ComputedStyle,
    rows: &mut Vec<crate::layout::TableRow>,
    _stylesheet: &Stylesheet,
    _ancestors: &[()],
) {
    if let NodeData::Element { name, .. } = &handle.data {
        let tag = name.local.as_ref();

        if tag == "tr" {
            let row_inherited = inherited.clone();
            let mut cells: Vec<crate::layout::TableCell> = Vec::new();
            for child in handle.children.borrow().iter() {
                if let NodeData::Element { name: cname, .. } = &child.data {
                    let ctag = cname.local.as_ref();
                    if ctag == "td" || ctag == "th" {
                        cells.push(build_table_cell(
                            child,
                            &row_inherited,
                            ctag == "th",
                        ));
                    }
                }
            }
            if !cells.is_empty() {
                rows.push(crate::layout::TableRow { cells });
            }
            return;
        }

        // Descend into table/thead/tbody/tfoot/etc.
        for child in handle.children.borrow().iter() {
            collect_table_rows(child, inherited, rows, _stylesheet, &[]);
        }
    }
}

fn build_table_cell(
    handle: &Handle,
    inherited: &ComputedStyle,
    is_header: bool,
) -> crate::layout::TableCell {
    use crate::layout::{TableCell, VerticalAlign};

    // Cell's inline style only (no stylesheet matching for MVP)
    let cell_style: Option<ComputedStyle> = extract_style_attr(handle);

    let merged_inherited = inherited.inherit_into(&cell_style);
    let em = 12.0_f32;

    // Build BlockStyle for the cell
    let mut block_style = BlockStyle {
        // Default cell padding
        padding_top: 4.0,
        padding_right: 6.0,
        padding_bottom: 4.0,
        padding_left: 6.0,
        // Default cell border so tables look like tables
        border_width: 1.0,
        border_color: Some((0.6, 0.6, 0.6)),
        border_style: Some(css::BorderStyle::Solid),
        text_align: if is_header {
            crate::layout::TextAlign::Center
        } else {
            crate::layout::TextAlign::Left
        },
        ..BlockStyle::default()
    };

    if let Some(style) = cell_style.as_ref() {
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
        if let Some(bs) = style.border_style {
            block_style.border_style = Some(bs);
        }
    }

    // Collect text runs from cell content
    let mut runs: Vec<TextRun> = Vec::new();
    let font_size = merged_inherited
        .font_size
        .map_or(12.0, |len| len.resolve(12.0));
    let color = merged_inherited.color;
    let font_name = if is_header {
        "Helvetica-Bold".to_string()
    } else {
        "Helvetica".to_string()
    };
    collect_cell_text(handle, &mut runs, &font_name, font_size, color);

    TableCell {
        runs,
        line_height: merged_inherited.line_height.map(|len| len.resolve(em)),
        style: block_style,
        vertical_align: VerticalAlign::Top,
    }
}

/// Recursively flatten inline text content from a cell into `TextRun`s,
/// collapsing whitespace. Does not handle nested block elements.
fn collect_cell_text(
    handle: &Handle,
    runs: &mut Vec<TextRun>,
    font_name: &str,
    font_size: f32,
    color: Option<(f32, f32, f32)>,
) {
    match &handle.data {
        NodeData::Text { contents } => {
            let text = contents.borrow().to_string();
            // Collapse internal whitespace
            let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !collapsed.is_empty() {
                // Add leading/trailing spaces if the original text had them
                let mut out = String::new();
                if text.starts_with(char::is_whitespace) && !runs.is_empty() {
                    out.push(' ');
                }
                out.push_str(&collapsed);
                if text.ends_with(char::is_whitespace) {
                    out.push(' ');
                }
                runs.push(TextRun {
                    text: out,
                    font_name: font_name.to_string(),
                    font_size,
                    color,
                    text_decoration: None,
                    link_url: None,
                });
            }
        }
        NodeData::Element { name, .. } => {
            let tag = name.local.as_ref();
            if tag == "br" {
                runs.push(TextRun {
                    text: "\n".to_string(),
                    font_name: font_name.to_string(),
                    font_size,
                    color,
                    text_decoration: None,
                    link_url: None,
                });
                return;
            }
            // Inline formatting inside cells is not yet supported — children
            // render in the cell's base font.
            for child in handle.children.borrow().iter() {
                collect_cell_text(child, runs, font_name, font_size, color);
            }
        }
        _ => {}
    }
}

/// Result of rendering a DOM: page style from `@page`, plus any non-fatal
/// warnings produced along the way (e.g. image load failures).
pub struct RenderOutcome {
    pub page_style: css::PageStyle,
    pub warnings: Vec<String>,
}

/// Walk an html5ever DOM and produce paragraphs into a `LayoutInner`.
pub fn render_dom_to_layout(
    document: &Handle,
    layout: &mut LayoutInner,
    base_dir: Option<&std::path::Path>,
) -> RenderOutcome {
    let css_text = extract_style_blocks(document);
    let stylesheet = css::parse_stylesheet(&css_text);
    let page_style = stylesheet.page_style.clone();
    let mut renderer = HtmlRenderer::new(layout, stylesheet);
    renderer.base_dir = base_dir.map(std::path::Path::to_path_buf);
    renderer.walk_node(document, 0);
    renderer.flush();
    RenderOutcome {
        page_style,
        warnings: renderer.warnings,
    }
}

/// Extract the text content of the first `<title>` element in the DOM.
pub fn extract_title(handle: &Handle) -> Option<String> {
    match &handle.data {
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                if let Some(t) = extract_title(child) {
                    return Some(t);
                }
            }
        }
        NodeData::Element { name, .. } => {
            if name.local.as_ref() == "title" {
                let mut text = String::new();
                for child in handle.children.borrow().iter() {
                    if let NodeData::Text { contents } = &child.data {
                        text.push_str(&contents.borrow());
                    }
                }
                let trimmed = text.trim().to_string();
                if !trimmed.is_empty() {
                    return Some(trimmed);
                }
            }
            for child in handle.children.borrow().iter() {
                if let Some(t) = extract_title(child) {
                    return Some(t);
                }
            }
        }
        _ => {}
    }
    None
}
