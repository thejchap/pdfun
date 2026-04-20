# Feature Parity Matrix

Auto-generated from [`tools/parity/catalog.toml`](../tools/parity/catalog.toml) plus inline `spec:` markers in tests. Run `uv run python tools/parity/generate.py` to regenerate.

**Summary:** 97/133 behaviors implemented · 24/133 tested · WeasyPrint comparison hand-curated in catalog.

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
| Paragraph (p) default margins | — | ✅ | ✅ | ⚠️ untested |
| Generic block container (div) | — | ✅ | ✅ | ⚠️ untested |
| Block quote (blockquote) with indent | — | ✅ | ✅ | ⚠️ untested |
| Preformatted text (pre) whitespace preservation | — | ✅ | ✅ | ⚠️ untested |
| Horizontal rule (hr) | — | ✅ | ✅ | ⚠️ untested |
| Semantic elements (article/section/nav/header/footer/aside/main) | — | ✅ | ✅ | ⚠️ untested |
| Figure / figcaption | — | ✅ | ✅ | ⚠️ untested |
| Details / summary (expanded rendering) | — | 🟡 | ✅ | ⚠️ untested |

## HTML — Inline elements

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Bold (b, strong) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Italic (i, em) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Inline span | — | ✅ | ✅ | ⚠️ untested |
| Line break (br) | — | ✅ | ✅ | ⚠️ untested |
| Inline code (code, kbd, samp) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Superscript / subscript (sup, sub) | — | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| Links (a) with external PDF annotations | — | ✅ | ✅ | ⚠️ untested |

## HTML — Lists

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Unordered list (ul, li) | — | ✅ | ✅ | ⚠️ untested |
| Ordered list (ol, li) | — | ✅ | ✅ | ⚠️ untested |
| Nested lists | — | ✅ | ✅ | ⚠️ untested |
| Definition list (dl, dt, dd) | — | ✅ | ✅ | ⚠️ untested |

## HTML — Embedded content

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| PNG image embedding (img) | — | ✅ | ✅ | ⚠️ untested |
| JPEG image embedding (img) | — | ✅ | ✅ | ⚠️ untested |
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
| Type selectors (p, h1) | 5.3 | ✅ | ✅ | ⚠️ untested |
| Class selectors (.foo) | 5.8.3 | ✅ | ✅ | ⚠️ untested |
| ID selectors (#foo) | 5.9 | ✅ | ✅ | ⚠️ untested |
| Universal selector (*) | 5.3 | ✅ | ✅ | ⚠️ untested |
| Compound selectors (p.note#main) | 5.2 | ✅ | ✅ | ⚠️ untested |
| Descendant combinator (div p) | 5.5 | ✅ | ✅ | ⚠️ untested |
| Child combinator (div > p) | 5.6 | ✅ | ✅ | ⚠️ untested |
| Adjacent sibling combinator (h1 + p) | 5.7 | ✅ | ✅ | ⚠️ untested |
| General sibling combinator (h1 ~ p) | 5.7 | ✅ | ✅ | ⚠️ untested |
| Selector lists (h1, h2, h3) | 5.2.1 | ✅ | ✅ | ⚠️ untested |
| Attribute selectors ([type="text"]) | 5.8 | ✅ | ✅ | ⚠️ untested |
| :first-child pseudo-class | 5.11.3 | ✅ | ✅ | ⚠️ untested |
| :nth-child() pseudo-class | 5.11.3 | ✅ | ✅ | ⚠️ untested |
| :not() pseudo-class | 5.11.4 | ✅ | ✅ | ⚠️ untested |
| ::before pseudo-element | 5.12.3 | ✅ | ❌ | — |
| ::after pseudo-element | 5.12.3 | ✅ | ❌ | — |
| ::first-line pseudo-element | 5.12.1 | ❌ | ❌ | — |

## CSS 2.1 §6 — Cascade and inheritance

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Specificity ordering | 6.4.3 | ✅ | ✅ | ⚠️ untested |
| Cascade: UA defaults < <style> < inline | 6.4.1 | ✅ | ✅ | ⚠️ untested |
| Property inheritance parent → child | 6.2 | ✅ | ✅ | ✅ (1) `tests/visual/nested_containers.html` |

## CSS 2.1 §8 — Box model

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| margin (shorthand + four sides) | 8.3 | ✅ | ✅ | ⚠️ untested |
| padding (shorthand + four sides) | 8.4 | ✅ | ✅ | ✅ (1) `tests/visual/padding_border.html` |
| border / border-width / border-color / border-style | 8.5 | ✅ | ✅ | ✅ (1) `tests/visual/padding_border.html` |
| Margin collapse: adjacent siblings | 8.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/margin_collapse_siblings.html` |
| Margin collapse: parent / first child | 8.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/margin_collapse_parent_child.html` |
| Margin collapse: empty blocks | 8.3.1 | ✅ | ✅ | ⚠️ untested |

## CSS 2.1 §9 — Visual formatting model

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| display: block | 9.2.1 | ✅ | ✅ | ✅ (1) `tests/visual/nested_containers.html` |
| display: inline | 9.2.2 | ✅ | ✅ | ⚠️ untested |
| display: inline-block | 9.2.4 | ✅ | ✅ | ⚠️ untested |
| display: none | 9.2.4 | ✅ | ✅ | ⚠️ untested |
| float: left with text wrap | 9.5.1 | ✅ | ✅ | ✅ (1) `tests/visual/float_left.html` |
| float: right with text wrap | 9.5.1 | ✅ | ✅ | ✅ (1) `tests/visual/float_right.html` |
| clear property | 9.5.2 | ✅ | ✅ | ⚠️ untested |
| position: static | 9.3.1 | ✅ | ✅ | ⚠️ untested |
| position: relative | 9.3.1 | ✅ | ❌ | — |
| position: absolute | 9.3.1 | ✅ | ❌ | — |
| position: fixed | 9.3.1 | ✅ | ❌ | — |
| top / right / bottom / left offsets | 9.3.2 | ✅ | ❌ | — |

## CSS 2.1 §10 — Visual formatting model details

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| width / min-width / max-width | 10.2 | ✅ | ✅ | ⚠️ untested |
| height / min-height / max-height | 10.5 | ✅ | ✅ | ⚠️ untested |
| box-sizing (content-box, border-box) | 10.1 | ✅ | ✅ | ⚠️ untested |
| line-height | 10.8 | ✅ | ✅ | ⚠️ untested |
| vertical-align (table cells: top, middle, bottom) | 10.8 | ✅ | ✅ | ⚠️ untested |

## CSS 2.1 §11 — Visual effects

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| overflow (visible, hidden, scroll, auto) | 11.1.1 | ✅ | ❌ | — |

## CSS 2.1 §13 — Paged media

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @page (size, margins) | 13.2 | ✅ | ✅ | ⚠️ untested |
| page-break-before / page-break-after | 13.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/page_break.html` |
| page-break-inside | 13.3.1 | ✅ | ❌ | — |
| orphans / widows | 13.3.2 | ✅ | ❌ | — |

## CSS 2.1 §14 — Colors and backgrounds

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Named colors (red, blue, ...) | 14.1 | ✅ | ✅ | ⚠️ untested |
| Hex colors (#rgb, #rrggbb) | 14.1 | ✅ | ✅ | ⚠️ untested |
| rgb() function | 14.1 | ✅ | ✅ | ⚠️ untested |
| color property | 14.1 | ✅ | ✅ | ⚠️ untested |
| background-color | 14.2.1 | ✅ | ✅ | ⚠️ untested |

## CSS 2.1 §15 — Fonts

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| font-family (generic: serif, sans-serif, monospace) | 15.3 | ✅ | ✅ | ⚠️ untested |
| font-style (normal, italic) | 15.4 | ✅ | ✅ | ⚠️ untested |
| font-weight (normal, bold, 100–900) | 15.6 | ✅ | ✅ | ⚠️ untested |
| font-size (px, pt, em, rem, %, vw/vh) | 15.7 | ✅ | ✅ | ⚠️ untested |

## CSS 2.1 §16 — Text

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| text-indent | 16.1 | ✅ | ✅ | ⚠️ untested |
| text-align (left, center, right, justify) | 16.2 | ✅ | ✅ | ✅ (1) `tests/visual/text_align.html` |
| text-decoration (underline, line-through) | 16.3.1 | ✅ | ✅ | ✅ (1) `tests/visual/inline_styles.html` |
| letter-spacing | 16.4 | ✅ | ✅ | ⚠️ untested |
| word-spacing | 16.4 | ✅ | ✅ | ⚠️ untested |
| text-transform (uppercase, lowercase, capitalize) | 16.5 | ✅ | ✅ | ⚠️ untested |
| white-space CSS property | 16.6 | ✅ | ❌ | — |

## CSS 2.1 §17 — Tables

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Table layout (auto width) | 17.5 | ✅ | ✅ | ⚠️ untested |
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
| rgba() function | 4.2.1 | ✅ | ✅ | ⚠️ untested |
| hsl() function | 4.2.3 | ✅ | ✅ | ⚠️ untested |
| hsla() function | 4.2.4 | ✅ | ✅ | ⚠️ untested |
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
| @page margin boxes (headers/footers) | 5 | ✅ | ✅ | ⚠️ untested |
| Page counters (counter(page), counter(pages)) | 4.3 | ✅ | ✅ | ⚠️ untested |

## CSS Values 3 — Values and units

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| Custom properties (var()) | — | ✅ | ❌ | — |
| calc() expressions | 8.1 | ✅ | ❌ | — |

## CSS Lists 3 — Lists and counters

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| list-style-type (disc, decimal, lower/upper-alpha, lower/upper-roman) | 3 | ✅ | ✅ | ✅ (1) `tests/visual/list_styles.html` |
| list-style-position | 3 | ✅ | ✅ | ⚠️ untested |

## CSS 2.1 §4.1.5 — At-rules

| Behavior | Spec § | WeasyPrint | pdfun | Tested |
|----------|:------:|:----------:|:-----:|:-------|
| @media | 4.1.5 | ✅ | ❌ | — |
| @import | 4.1.5 | ✅ | ❌ | — |

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
| Multi-page documents | — | ✅ | ✅ | ⚠️ untested |
| Document metadata (title, author, keywords) | — | ✅ | ✅ | ⚠️ untested |
| Clickable external links | — | ✅ | ✅ | ⚠️ untested |
| Internal link anchors | — | ✅ | ✅ | ⚠️ untested |
| Bookmarks / outline (from headings) | — | ✅ | ✅ | ⚠️ untested |
| Table of contents | — | ✅ | ❌ | — |
| Custom font embedding (CIDFont + ToUnicode) | — | ✅ | ✅ | ⚠️ untested |
| Font subsetting (only used glyphs embedded) | — | ✅ | ✅ | ⚠️ untested |
| Stream compression | — | ✅ | ❌ | — |
| PDF encryption | — | 🟡 | ❌ | — |
| PDF/A compliance | — | ✅ | ❌ | — |

