# pdfun

A pure-Rust HTML/CSS to PDF renderer with Python bindings. An alternative to [WeasyPrint](https://github.com/Kozea/WeasyPrint) with zero system dependencies.

## Roadmap

Current burndown, ordered by the work items in flight:

**Stage A — relative units, spacing, sizing** (done)
- [x] A1: `em` / `rem` resolution through a threaded `LengthContext`
- [x] A2: `vw` / `vh` viewport units
- [x] A3: `@page` size & margins
- [x] A4: `%` percentages (font-size, width, padding, margin)
- [x] A5: `letter-spacing`, `word-spacing` (Tc/Tw, negative values)
- [x] A6: `box-sizing: border-box`

**Stage B — margin collapsing** (done)
- [x] B1: adjacent siblings collapse to max (positive/positive), min (negative/negative), sum (mixed)
- [x] B2: parent / first-child and parent / last-child collapse
- [x] B3: empty-block self-collapse

**Stage C — floats & inline-block** (planned)
- [ ] C1: `float: left` / `float: right` with line-box intrusion
- [ ] C2: `clear: left` / `right` / `both`
- [ ] C3: `display: inline-block`

**Later / unscoped**
- Positioning (`position: absolute` / `relative` / `fixed`)
- Flexbox and grid
- `@page` margin boxes (headers / footers)
- Bookmarks, internal links, PDF/A, compression, encryption

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
- [ ] Table `caption`
- [x] Semantic elements (`article`, `section`, `nav`, `header`, `footer`, `aside`, `main`)
- [x] Definition lists (`dl`, `dt`, `dd`)
- [ ] Superscript/subscript (`sup`, `sub`)
- [x] Figure (`figure`, `figcaption`)
- [ ] Details/summary (`details`, `summary`)
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
- [ ] Adjacent sibling combinator (`h1 + p`)
- [ ] General sibling combinator (`h1 ~ p`)
- [ ] Pseudo-classes (`:first-child`, `:nth-child()`, `:not()`)
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
- [ ] `min-height`, `max-height`
- [ ] `display` (block, inline, inline-block, none, flex, grid)
- [ ] `position` (static, relative, absolute, fixed)
- [ ] `top`, `right`, `bottom`, `left`
- [ ] `float`, `clear`
- [ ] `overflow`
- [ ] `opacity`
- [ ] `border-radius`
- [x] `text-transform` (uppercase, lowercase, capitalize)
- [x] `text-indent`
- [ ] `white-space` (via `<pre>` only, not as CSS property)
- [ ] `vertical-align`
- [ ] `box-shadow`
- [ ] `background-image`, `background-repeat`, `background-size`, `background-position`
- [x] `list-style-position`
- [ ] CSS custom properties (`var()`)
- [ ] `calc()`

### CSS At-Rules

- [x] `@page` (page size, margins)
- [ ] `@page` margin boxes (headers/footers)
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
- [ ] Floats
- [ ] Absolute/relative positioning
- [ ] Flexbox
- [ ] Grid
- [ ] Inline-block
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
- [ ] Bookmarks/outline (from headings)
- [ ] Internal link anchors
- [ ] Table of contents
- [ ] Page numbers / counters
- [ ] Headers and footers (`@page` margin boxes)
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
