# Feature Parity Matrix

Auto-generated from [`tools/parity/catalog.toml`](../tools/parity/catalog.toml) plus inline `spec:` markers in tests. Run `uv run python tools/parity/generate.py` to regenerate.

**Summary:** 105/133 behaviors implemented · 105/133 tested · WeasyPrint comparison hand-curated in catalog.

## Legend

| Column | Meaning |
|--------|---------|
| `Spec §` | Sub-section within the spec, if applicable |
| `WeasyPrint` | ✅ full · 🟡 partial · ❌ none |
| `pdfun` | ✅ implemented · ❌ not implemented |
| `Tested` | ✅ (N) tests referencing this behavior · ⚠️ implemented but untested · — not applicable |

## HTML — Block-level elements

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Headings (h1–h6) with scaled default sizes | — | ✅ | ✅ | ✅ (1) `tests/visual/heading_sizes.html` |
| Paragraph (p) default margins | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::paragraph_renders` |
| Generic block container (div) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::div_renders` |
| Block quote (blockquote) with indent | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::blockquote_renders` |
| Preformatted text (pre) whitespace preservation | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::pre_preserves_spaces` |
| Horizontal rule (hr) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::hr_renders` |
| Semantic elements (article/section/nav/header/footer/aside/main) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::article_renders` |
| Figure / figcaption | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::figure_with_figcaption_renders_both` |
| Details / summary (expanded rendering) | — | 🟡 | ✅ | ✅ (1) `tests/test_html.py::summary_renders_as_bold_block` |

## HTML — Inline elements

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Bold (b, strong) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Italic (i, em) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Inline span | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::span_extracts_text` |
| Line break (br) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::br_splits_text` |
| Inline code (code, kbd, samp) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Superscript / subscript (sup, sub) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Links (a) with external PDF annotations | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::anchor_tag_preserves_text` |

## HTML — Lists

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Unordered list (ul, li) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::ul_renders_item_text` |
| Ordered list (ol, li) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::ol_has_numbered_markers` |
| Nested lists | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::nested_ul` |
| Definition list (dl, dt, dd) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::dl_renders_term_and_definition` |

## HTML — Embedded content

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| PNG image embedding (img) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::png_img_produces_xobject` |
| JPEG image embedding (img) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::jpeg_img_produces_dctdecode_xobject` |
| SVG rendering | — | ✅ | ❌ | — |

## HTML — Tables

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Table with tr/td/th | — | ✅ | ✅ | ✅ (1) `tests/visual/table_layout.html` |
| Table row groups (thead, tbody, tfoot) | — | ✅ | ✅ | ✅ (1) `tests/visual/table_layout.html` |
| Table caption | — | ✅ | ✅ | ✅ (1) `tests/visual/table_layout.html` |

## HTML — Forms

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| input elements | — | 🟡 | ❌ | — |
| select elements | — | 🟡 | ❌ | — |
| textarea elements | — | 🟡 | ❌ | — |
| button elements | — | 🟡 | ❌ | — |

## CSS 2.1 §5 — Selectors

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Type selectors (p, h1) | 5.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_type_selector_color` |
| Class selectors (.foo) | 5.8.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_class_selector` |
| ID selectors (#foo) | 5.9 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_id_selector` |
| Universal selector (*) | 5.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::universal_selector_matches_all_elements` |
| Compound selectors (p.note#main) | 5.2 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_compound_selector` |
| Descendant combinator (div p) | 5.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_descendant_selector` |
| Child combinator (div > p) | 5.6 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_child_selector` |
| Adjacent sibling combinator (h1 + p) | 5.7 | ✅ | ✅ | ✅ (1) `tests/test_html.py::adjacent_sibling_matches_immediate_p` |
| General sibling combinator (h1 ~ p) | 5.7 | ✅ | ✅ | ✅ (1) `tests/test_html.py::general_sibling_matches_all_following_p` |
| Selector lists (h1, h2, h3) | 5.2.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_multiple_selectors` |
| Attribute selectors ([type="text"]) | 5.8 | ✅ | ✅ | ✅ (1) `tests/test_html.py::attr_prefix_match` |
| :first-child pseudo-class | 5.11.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::first_child_matches_first_p` |
| :nth-child() pseudo-class | 5.11.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::nth_child_even_hits_even_positions` |
| :not() pseudo-class | 5.11.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::not_pseudo_excludes_class` |
| ::before pseudo-element | 5.12.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::before_prepends_generated_content` |
| ::after pseudo-element | 5.12.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::after_appends_generated_content` |
| ::first-line pseudo-element | 5.12.1 | ❌ | ❌ | — |

## CSS 2.1 §6 — Cascade and inheritance

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Specificity ordering | 6.4.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_id_beats_class` |
| Cascade: UA defaults < <style> < inline | 6.4.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_inline_wins_over_style_block` |
| Property inheritance parent → child | 6.2 | ✅ | ✅ | ✅ (1) `tests/visual/nested_containers.html` |

## CSS 2.1 §8 — Box model

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| margin (shorthand + four sides) | 8.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::margin_top_renders` |
| padding (shorthand + four sides) | 8.4 | ✅ | ✅ | ✅ (1) `tests/visual/padding_border.html` |
| border / border-width / border-color / border-style | 8.5 | ✅ | ✅ | ✅ (1) `tests/visual/padding_border.html` |
| Margin collapse: adjacent siblings | 8.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/margin_collapse_siblings.html` |
| Margin collapse: parent / first child | 8.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/margin_collapse_parent_child.html` |
| Margin collapse: empty blocks | 8.3.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::empty_block_self_collapses` |

## CSS 2.1 §9 — Visual formatting model

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| display: block | 9.2.1 | ✅ | ✅ | ✅ (1) `tests/visual/nested_containers.html` |
| display: inline | 9.2.2 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_block_renders_as_inline_atom` |
| display: inline-block | 9.2.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_block_with_fixed_width` |
| display: none | 9.2.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::display_none_hides_children` |
| float: left with text wrap | 9.5.1 | ✅ | ✅ | ✅ (1) `tests/visual/float_left.html` |
| float: right with text wrap | 9.5.1 | ✅ | ✅ | ✅ (1) `tests/visual/float_right.html` |
| clear property | 9.5.2 | ✅ | ✅ | ✅ (1) `tests/test_html.py::clear_both_drops_below_floats` |
| position: static | 9.3.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::position_static_ignores_offsets` |
| position: relative | 9.3.1 | ✅ | ❌ | — |
| position: absolute | 9.3.1 | ✅ | ❌ | — |
| position: fixed | 9.3.1 | ✅ | ❌ | — |
| top / right / bottom / left offsets | 9.3.2 | ✅ | ❌ | — |

## CSS 2.1 §10 — Visual formatting model details

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| width / min-width / max-width | 10.2 | ✅ | ✅ | ✅ (1) `tests/test_html.py::img_width_preserves_aspect_ratio` |
| height / min-height / max-height | 10.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::min_height_expands_short_block` |
| box-sizing (content-box, border-box) | 10.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::border_box_vs_content_box_differ` |
| line-height | 10.8 | ✅ | ✅ | ✅ (1) `tests/test_html.py::line_height_inherits_through_div` |
| vertical-align (table cells: top, middle, bottom) | 10.8 | ✅ | ✅ | ✅ (1) `tests/test_html.py::vertical_align_top_matches_default` |

## CSS 2.1 §11 — Visual effects

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| overflow (visible, hidden, scroll, auto) | 11.1.1 | ✅ | ❌ | — |

## CSS 2.1 §13 — Paged media

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @page (size, margins) | 13.2 | ✅ | ✅ | ✅ (1) `tests/test_html.py::at_page_size_letter` |
| page-break-before / page-break-after | 13.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/page_break.html` |
| page-break-inside | 13.3.1 | ✅ | ✅ | ✅ (3) `tests/test_html.py::page_break_inside_avoid_parses`, `tests/test_html.py::break_inside_avoid_alias_accepted`, `tests/test_html.py::page_break_inside_avoid_pushes_overflow_to_next_page` |
| orphans / widows | 13.3.2 | ✅ | ✅ | ✅ (3) `tests/test_html.py::orphans_integer_parses`, `tests/test_html.py::widows_integer_parses`, `tests/test_html.py::orphans_and_widows_via_stylesheet` |

## CSS 2.1 §14 — Colors and backgrounds

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Named colors (red, blue, ...) | 14.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_color_named` |
| Hex colors (#rgb, #rrggbb) | 14.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_color_hex` |
| rgb() function | 14.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_color_rgb` |
| color property | 14.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::style_type_selector_color` |
| background-color | 14.2.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_background_color` |

## CSS 2.1 §15 — Fonts

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| font-family (generic: serif, sans-serif, monospace) | 15.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::body_font_family_inherits` |
| font-style (normal, italic) | 15.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::italic_tag_applies_italic_font` |
| font-weight (normal, bold, 100–900) | 15.6 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_font_weight_bold` |
| font-size (px, pt, em, rem, %, vw/vh) | 15.7 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inline_font_size_pt` |

## CSS 2.1 §16 — Text

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| text-indent | 16.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::text_indent_renders_first_line_shift` |
| text-align (left, center, right, justify) | 16.2 | ✅ | ✅ | ✅ (1) `tests/visual/text_align.html` |
| text-decoration (underline, line-through) | 16.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| letter-spacing | 16.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::letter_spacing_emits_character_spacing_op` |
| word-spacing | 16.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::justify_emits_word_spacing_op` |
| text-transform (uppercase, lowercase, capitalize) | 16.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::uppercase_transforms_text` |
| white-space CSS property | 16.6 | ✅ | ✅ | ✅ (1) `tests/test_html.py::white_space_pre_preserves_spaces_like_pre_tag` |

## CSS 2.1 §17 — Tables

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Table layout (auto width) | 17.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::table_td_inline_style_color` |
| border-collapse (separate, collapse) | 17.6.2 | ✅ | ✅ | ✅ (1) `tests/visual/table_layout.html` |

## CSS Backgrounds & Borders 3 — Backgrounds and borders

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| border-radius | 5.1 | ✅ | ✅ | ✅ (1) `tests/visual/border_radius.html` |
| box-shadow | 7.1 | ✅ | ❌ | — |
| background-image | 3.3 | ✅ | ❌ | — |
| background-repeat | 3.5 | ✅ | ❌ | — |
| background-size | 3.9 | ✅ | ❌ | — |
| background-position | 3.6 | ✅ | ❌ | — |

## CSS Color 3 — Color spaces and alpha

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| rgba() function | 4.2.1 | ✅ | ✅ | ✅ (1) `tests/test_html.py::rgba_accepts_alpha_component` |
| hsl() function | 4.2.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::hsl_red_renders` |
| hsla() function | 4.2.4 | ✅ | ✅ | ✅ (1) `tests/test_html.py::hsla_accepts_alpha_component` |
| opacity property | 3.2 | ✅ | ✅ | ✅ (1) `tests/visual/opacity.html` |
| device-cmyk() / CMYK colors | — | ❌ | ❌ | — |

## CSS Fonts 3 — Fonts Level 3

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @font-face (web fonts) | 4.1 | ✅ | ❌ | — |
| Variable fonts | — | 🟡 | ❌ | — |
| OpenType features (ligatures, alternates) | 6 | ✅ | ❌ | — |
| Font fallback chains | 4.2 | ✅ | ❌ | — |

## CSS Multi-column 1 — Multi-column layout

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Multi-column layout (column-count, column-gap) | 2 | ✅ | ✅ | ✅ (1) `tests/visual/columns.html` |

## CSS Paged Media 3 — Paged media extensions

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @page margin boxes (headers/footers) | 5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::margin_box_with_font_size_and_color` |
| Page counters (counter(page), counter(pages)) | 4.3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::counter_page_renders_1_on_first_page` |

## CSS Values 3 — Values and units

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Custom properties (var()) | — | ✅ | ❌ | — |
| calc() expressions | 8.1 | ✅ | ❌ | — |

## CSS Lists 3 — Lists and counters

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| list-style-type (disc, decimal, lower/upper-alpha, lower/upper-roman) | 3 | ✅ | ✅ | ✅ (1) `tests/visual/list_styles.html` |
| list-style-position | 3 | ✅ | ✅ | ✅ (1) `tests/test_html.py::inside_differs_from_outside` |

## CSS 2.1 §4.1.5 — At-rules

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @media | 4.1.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::media_print_rules_apply` |
| @import | 4.1.5 | ✅ | ✅ | ✅ (1) `tests/test_html.py::import_statement_does_not_break_parser` |

## CSS Flexbox 1 — Flexbox layout

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Flex container (display: flex) | 3 | ✅ | ❌ | — |

## CSS Grid 1 — Grid layout

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Grid container (display: grid) | 6 | ❌ | ❌ | — |

## PDF — PDF output features

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Multi-page documents | — | ✅ | ✅ | ✅ (1) `tests/test_pdfun.py::multi_page_document` |
| Document metadata (title, author, keywords) | — | ✅ | ✅ | ✅ (1) `tests/test_pdfun.py::set_title_appears_in_pdf` |
| Clickable external links | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::link_produces_annotation` |
| Internal link anchors | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::internal_link_emits_goto_action` |
| Bookmarks / outline (from headings) | — | ✅ | ✅ | ✅ (1) `tests/test_html.py::headings_emit_outlines_key` |
| Table of contents | — | ✅ | ✅ | ✅ (7) `tests/test_html.py::toc_true_prepends_heading_list`, `tests/test_html.py::toc_emits_internal_link_per_heading`, `tests/test_html.py::toc_isolates_itself_on_dedicated_page`, `tests/test_html.py::toc_string_sets_custom_title`, `tests/test_html.py::toc_preserves_existing_heading_ids`, `tests/test_html.py::toc_false_is_a_no_op`, `tests/test_html.py::toc_on_empty_document_no_op` |
| Custom font embedding (CIDFont + ToUnicode) | — | ✅ | ✅ | ✅ (1) `tests/test_pdfun.py::register_font_returns_name` |
| Font subsetting (only used glyphs embedded) | — | ✅ | ✅ | ✅ (1) `tests/test_pdfun.py::embedded_font_has_widths` |
| Stream compression | — | ✅ | ❌ | — |
| PDF encryption | — | 🟡 | ❌ | — |
| PDF/A compliance | — | ✅ | ❌ | — |

