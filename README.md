# pdfun

<p align="center">
  <a href="https://github.com/thejchap/pdfun/actions/workflows/ci.yml">
    <img src="https://github.com/thejchap/pdfun/actions/workflows/ci.yml/badge.svg" alt="CI" />
  </a>
  <a href="https://github.com/astral-sh/ruff">
    <img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json" alt="ruff" />
  </a>
  <a href="https://python.org">
    <img src="https://img.shields.io/badge/python-3.12%20%7C%203.13%20%7C%203.14-blue.svg" alt="python" />
  </a>
</p>

A pure-Rust HTML/CSS to PDF renderer with Python bindings. An alternative to [WeasyPrint](https://github.com/Kozea/WeasyPrint) with zero system dependencies.

## Quick Start

### HTML to PDF

```python
from pdfun import HtmlDocument

doc = HtmlDocument(string="""
<style>
  body { font-family: sans-serif; margin: 2cm; }
  h1 { color: navy; }
</style>
<h1>Hello, pdfun!</h1>
<p>A pure-Rust HTML/CSS to PDF renderer.</p>
""")
doc.write_pdf("output.pdf")
```

### Layout API

```python
from pdfun import PdfDocument, Layout, TextRun

doc = PdfDocument()
layout = Layout(doc)
layout.add_text("Hello, world!", font_size=24.0, spacing_after=12.0)
layout.add_paragraph([
    TextRun("Bold text", font_name="Helvetica-Bold"),
    TextRun(" and normal text."),
])
layout.finish()
doc.save("output.pdf")
```

### CLI

```bash
pdfun render input.html -o output.pdf
```

## Feature Parity Checklist (vs WeasyPrint)

### HTML Elements

- [x] Headings (`h1`-`h6`)
- [x] Paragraphs (`p`)
- [x] Divisions (`div`)
- [x] Lists (`ul`, `ol`, `li`) with nesting
- [x] Bold/strong (`b`, `strong`)
- [x] Italic/emphasis (`i`, `em`)
- [x] Inline span (`span`)
- [x] Line break (`br`)
- [x] Preformatted text (`pre`)
- [x] Inline code (`code`, `kbd`, `samp`)
- [x] Block quote (`blockquote`)
- [x] Horizontal rule (`hr`)
- [x] Links (`a`) with clickable annotations
- [x] Images (`img`) — PNG and JPEG embedding
- [x] Tables (`table`, `tr`, `td`, `th`, `thead`, `tbody`, `tfoot`)
- [x] Table `caption`
- [x] Semantic elements (`article`, `section`, `nav`, `header`, `footer`, `aside`, `main`)
- [x] Definition lists (`dl`, `dt`, `dd`)
- [x] Superscript/subscript (`sup`, `sub`)
- [x] Figure (`figure`, `figcaption`)
- [x] Details/summary (`details`, `summary`)
- [ ] Form elements (`input`, `select`, `textarea`, `button`)

### CSS Selectors

- [x] Type selectors (`p`, `h1`)
- [x] Class selectors (`.highlight`)
- [x] ID selectors (`#header`)
- [x] Universal selector (`*`)
- [x] Compound selectors (`p.note#main`)
- [x] Descendant combinator (`div p`)
- [x] Child combinator (`div > p`)
- [x] Selector lists (`h1, h2, h3`)
- [x] Specificity ordering
- [x] Cascade: UA defaults < `<style>` < inline `style=""`
- [x] Attribute selectors (`[type="text"]`)
- [x] Adjacent sibling combinator (`h1 + p`)
- [x] General sibling combinator (`h1 ~ p`)
- [x] Pseudo-classes (`:first-child`, `:nth-child()`, `:not()`)
- [ ] Pseudo-elements (`::before`, `::after`, `::first-line`)

### CSS Properties

- [x] `color` (named, hex, `rgb()`)
- [x] `background-color`
- [x] `font-size` (px, pt, em, rem, %, vw/vh)
- [x] `font-weight` (normal, bold, 100-900)
- [x] `font-style` (normal, italic)
- [x] `font-family` (generic families: serif, sans-serif, monospace)
- [x] `text-align` (left, center, right, justify)
- [x] `line-height`
- [x] `margin` (shorthand + all four sides)
- [x] `padding` (shorthand + all four sides)
- [x] `border` (shorthand), `border-width`, `border-color`, `border-style`
- [x] `width`, `height`, `min-width`, `max-width`
- [x] `box-sizing` (content-box, border-box)
- [x] `text-decoration` (underline, line-through)
- [x] `list-style-type` (disc, circle, square, decimal, lower/upper-alpha, lower/upper-roman)
- [x] `page-break-before`, `page-break-after`
- [x] `letter-spacing`, `word-spacing`
- [x] Margin collapsing (adjacent siblings)
- [x] Margin collapsing (parent/child, empty blocks)
- [x] `min-height`, `max-height`
- [x] `display` (block, inline, inline-block, none) — flex/grid not supported
- [ ] `position` (static, relative, absolute, fixed)
- [ ] `top`, `right`, `bottom`, `left`
- [x] `float`, `clear`
- [ ] `overflow`
- [x] `opacity`
- [x] `border-radius`
- [x] `text-transform` (uppercase, lowercase, capitalize)
- [x] `text-indent`
- [ ] `white-space` (via `<pre>` only, not as CSS property)
- [x] `vertical-align` (table cells: top, middle, bottom)
- [x] `border-collapse` (separate, collapse)
- [ ] `box-shadow`
- [ ] `background-image`, `background-repeat`, `background-size`, `background-position`
- [x] `list-style-position`
- [ ] CSS custom properties (`var()`)
- [ ] `calc()`

### CSS At-Rules

- [x] `@page` (page size, margins)
- [x] `@page` margin boxes (headers/footers)
- [ ] `@font-face`
- [ ] `@media`
- [ ] `@import`

### Layout

- [x] Block flow (vertical stacking)
- [x] Inline text with word wrapping
- [x] Mixed fonts/sizes/colors within a paragraph
- [x] Automatic page breaks
- [x] Configurable page size and margins
- [x] Text alignment (left, center, right)
- [x] Padding and borders on blocks
- [x] List markers (bullets and numbers)
- [x] Whitespace preservation (`<pre>`)
- [x] Justify alignment
- [x] Multi-column layout
- [x] Table layout
- [x] CSS style inheritance (parent to child)
- [x] Margin collapsing (adjacent siblings)
- [x] Margin collapsing (parent/child, empty blocks)
- [x] Floats
- [ ] Absolute/relative positioning
- [ ] Flexbox
- [ ] Grid
- [x] Inline-block
- [ ] Orphans/widows control

### Fonts

- [x] 14 standard PDF fonts (Helvetica, Times, Courier + variants, Symbol, ZapfDingbats)
- [x] TrueType/OpenType font loading (file, bytes, system fonts)
- [x] Font subsetting (only used glyphs embedded)
- [x] Font database with family/weight/italic querying
- [ ] `@font-face` web fonts
- [ ] Variable fonts
- [ ] OpenType features (ligatures, alternates)
- [ ] Font fallback chains

### PDF Features

- [x] Multi-page documents
- [x] Configurable page dimensions
- [x] Text rendering with proper encoding
- [x] Graphics (rectangles, lines, fill, stroke)
- [x] Custom font embedding (CIDFont + ToUnicode)
- [x] Clickable links (external)
- [x] Document metadata (title, author, keywords)
- [x] Image embedding (PNG, JPEG)
- [x] Bookmarks/outline (from headings)
- [x] Internal link anchors
- [ ] Table of contents
- [x] Page numbers / counters
- [x] Headers and footers (`@page` margin boxes)
- [ ] SVG rendering
- [ ] PDF/A compliance
- [ ] Compression
- [ ] Encryption

### Colors

- [x] Named colors (red, blue, green, etc.)
- [x] Hex colors (`#rgb`, `#rrggbb`)
- [x] `rgb()` / `rgba()` functions
- [x] `hsl()` / `hsla()` functions
- [ ] CMYK colors
- [ ] Transparency/alpha
- [ ] Gradients (linear, radial, conic)
