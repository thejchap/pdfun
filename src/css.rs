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
}

impl CssLength {
    /// Resolve to points. `em_base` is the current font size in points.
    pub fn resolve(self, em_base: f32) -> f32 {
        match self {
            CssLength::Px(v) => v * 0.75, // 96 dpi CSS px → 72 dpi PDF pt
            CssLength::Pt(v) => v,
            CssLength::Em(v) => v * em_base,
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
    pub margin_bottom: Option<CssLength>,
    pub padding_top: Option<CssLength>,
    pub padding_right: Option<CssLength>,
    pub padding_bottom: Option<CssLength>,
    pub padding_left: Option<CssLength>,
    pub border_width: Option<CssLength>,
    pub border_color: Option<(f32, f32, f32)>,
}

impl ComputedStyle {
    /// Returns true if any property is set (non-None).
    pub fn has_any_property(&self) -> bool {
        self.color.is_some()
            || self.background_color.is_some()
            || self.font_size.is_some()
            || self.font_weight.is_some()
            || self.font_style.is_some()
            || self.font_family.is_some()
            || self.text_align.is_some()
            || self.line_height.is_some()
            || self.margin_bottom.is_some()
            || self.padding_top.is_some()
            || self.padding_right.is_some()
            || self.padding_bottom.is_some()
            || self.padding_left.is_some()
            || self.border_width.is_some()
            || self.border_color.is_some()
    }
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
        _ => Err(location.new_custom_error(())),
    }
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
            } else {
                Err(location.new_custom_error(()))
            }
        }
        Token::Number { value, .. } if *value == 0.0 => Ok(CssLength::Px(0.0)),
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
        CssLength::Px(v) | CssLength::Pt(v) | CssLength::Em(v) => v,
    };
    if value < 0.0 {
        return Err(location.new_custom_error(()));
    }
    Ok(len)
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
                let location = input.current_source_location();
                let token = input.next()?.clone();
                match &token {
                    Token::Ident(ident) => {
                        self.style.font_family = Some(ident.to_string());
                    }
                    Token::QuotedString(s) => {
                        self.style.font_family = Some(s.to_string());
                    }
                    _ => return Err(location.new_custom_error(())),
                }
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
                // Only margin-bottom maps to spacing_after for now
                let _ = (top, right, left);
                self.style.margin_bottom = Some(bottom);
            }
            "margin-bottom" => {
                self.style.margin_bottom = Some(parse_css_length(input)?);
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
            "border" => {
                // border shorthand: parse width and/or color in any order
                // (ignore border-style, always solid)
                let mut found = false;
                for _ in 0..3 {
                    if let Ok(len) = input.try_parse(parse_non_negative_length) {
                        self.style.border_width = Some(len);
                        found = true;
                    } else if let Ok(color) = input.try_parse(parse_css_color) {
                        self.style.border_color = Some(color);
                        found = true;
                    } else if input
                        .try_parse(|i: &mut Parser<'i, '_>| {
                            let ident = i.expect_ident()?;
                            if matches!(
                                ident.to_ascii_lowercase().as_str(),
                                "solid"
                                    | "dashed"
                                    | "dotted"
                                    | "none"
                                    | "hidden"
                                    | "double"
                                    | "groove"
                                    | "ridge"
                                    | "inset"
                                    | "outset"
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
    fn matches(&self, tag: &str, classes: &[&str], id: Option<&str>) -> bool {
        self.parts.iter().all(|part| match part {
            SimpleSelector::Type(t) => t.eq_ignore_ascii_case(tag),
            SimpleSelector::Class(c) => classes.iter().any(|cl| cl.eq_ignore_ascii_case(c)),
            SimpleSelector::Id(i) => id.is_some_and(|elem_id| elem_id.eq_ignore_ascii_case(i)),
            SimpleSelector::Universal => true,
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

/// Parse a compound selector from a text token like `p.note#main`.
fn parse_compound_from_text(
    text: &str,
    id_count: &mut u16,
    class_count: &mut u16,
    type_count: &mut u16,
) -> CompoundSelector {
    let mut parts: Vec<SimpleSelector> = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some(&(pos, ch)) = chars.peek() {
        match ch {
            '#' => {
                chars.next();
                let start = pos + 1;
                while chars.peek().is_some_and(|&(_, c)| c != '.' && c != '#') {
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
                while chars.peek().is_some_and(|&(_, c)| c != '.' && c != '#') {
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
            _ => {
                let start = pos;
                while chars.peek().is_some_and(|&(_, c)| c != '.' && c != '#') {
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

// ── Stylesheet parsing ────────────────────────────────────

/// Parse a CSS stylesheet (from `<style>` blocks) into rules.
pub fn parse_stylesheet(css: &str) -> Stylesheet {
    let mut rules = Vec::new();
    parse_stylesheet_manual(css, &mut rules);
    Stylesheet { rules }
}

/// Parse stylesheet by manually scanning for `{ }` blocks.
fn parse_stylesheet_manual(css: &str, rules: &mut Vec<CssRule>) {
    let mut remaining = css;

    while !remaining.trim().is_empty() {
        // Find the next `{`
        let Some(brace_open) = remaining.find('{') else {
            break;
        };
        let selector_text = remaining[..brace_open].trim();
        let after_open = &remaining[brace_open + 1..];

        // Find matching `}` (simple: no nesting support)
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

// ── Selector matching ─────────────────────────────────────

/// Information about an element needed for selector matching.
pub struct ElementInfo<'a> {
    pub tag: &'a str,
    pub classes: Vec<&'a str>,
    pub id: Option<&'a str>,
    /// Ancestor chain from parent to root.
    /// Each entry: (tag, classes, id).
    pub ancestors: Vec<(&'a str, Vec<&'a str>, Option<&'a str>)>,
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
    if !subject.matches(element.tag, &element.classes, element.id) {
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
                let (tag, ref classes, id) = element.ancestors[ancestor_idx];
                if !compound.matches(tag, classes, id) {
                    return false;
                }
                ancestor_idx += 1;
            }
            Combinator::Descendant => {
                // Must match some ancestor
                let mut found = false;
                while ancestor_idx < element.ancestors.len() {
                    let (tag, ref classes, id) = element.ancestors[ancestor_idx];
                    ancestor_idx += 1;
                    if compound.matches(tag, classes, id) {
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

/// Merge non-None fields from `source` into `target`.
pub fn merge_style(target: &mut ComputedStyle, source: &ComputedStyle) {
    if source.color.is_some() {
        target.color = source.color;
    }
    if source.background_color.is_some() {
        target.background_color = source.background_color;
    }
    if source.font_size.is_some() {
        target.font_size = source.font_size;
    }
    if source.font_weight.is_some() {
        target.font_weight = source.font_weight;
    }
    if source.font_style.is_some() {
        target.font_style = source.font_style;
    }
    if source.font_family.is_some() {
        target.font_family.clone_from(&source.font_family);
    }
    if source.text_align.is_some() {
        target.text_align.clone_from(&source.text_align);
    }
    if source.line_height.is_some() {
        target.line_height = source.line_height;
    }
    if source.margin_bottom.is_some() {
        target.margin_bottom = source.margin_bottom;
    }
    if source.padding_top.is_some() {
        target.padding_top = source.padding_top;
    }
    if source.padding_right.is_some() {
        target.padding_right = source.padding_right;
    }
    if source.padding_bottom.is_some() {
        target.padding_bottom = source.padding_bottom;
    }
    if source.padding_left.is_some() {
        target.padding_left = source.padding_left;
    }
    if source.border_width.is_some() {
        target.border_width = source.border_width;
    }
    if source.border_color.is_some() {
        target.border_color = source.border_color;
    }
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
        let elem = ElementInfo { tag: "p", classes: vec![], id: None, ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_class_selector() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = ElementInfo { tag: "p", classes: vec!["red"], id: None, ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn no_match_wrong_class() {
        let sheet = parse_stylesheet(".red { color: red }");
        let elem = ElementInfo { tag: "p", classes: vec!["blue"], id: None, ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn match_descendant_selector() {
        let sheet = parse_stylesheet("div p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None,
            ancestors: vec![("div", vec![], None)],
        };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn match_child_selector() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None,
            ancestors: vec![("div", vec![], None)],
        };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((1.0, 0.0, 0.0)));
    }

    #[test]
    fn child_no_match_grandchild() {
        let sheet = parse_stylesheet("div > p { color: red }");
        let elem = ElementInfo {
            tag: "p", classes: vec![], id: None,
            ancestors: vec![("blockquote", vec![], None), ("div", vec![], None)],
        };
        let style = match_rules(&elem, &sheet);
        assert!(style.color.is_none());
    }

    #[test]
    fn specificity_class_beats_type() {
        let sheet = parse_stylesheet("p { color: red } .blue { color: blue }");
        let elem = ElementInfo { tag: "p", classes: vec!["blue"], id: None, ancestors: vec![] };
        let style = match_rules(&elem, &sheet);
        assert_eq!(style.color, Some((0.0, 0.0, 1.0)));
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
