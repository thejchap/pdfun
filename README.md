# pdfun

A pure-Rust HTML/CSS to PDF renderer with Python bindings. An alternative to [WeasyPrint](https://github.com/Kozea/WeasyPrint) with zero system dependencies.

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
- [ ] Links (`a`) — text renders, but no clickable annotations or styling
- [ ] Images (`img`)
- [ ] Tables (`table`, `tr`, `td`, `th`, `thead`, `tbody`, `tfoot`, `caption`)
- [ ] Semantic elements (`article`, `section`, `nav`, `header`, `footer`, `aside`, `main`)
- [ ] Definition lists (`dl`, `dt`, `dd`)
- [ ] Superscript/subscript (`sup`, `sub`)
- [ ] Figure (`figure`, `figcaption`)
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
- [ ] Attribute selectors (`[type="text"]`)
- [ ] Adjacent sibling combinator (`h1 + p`)
- [ ] General sibling combinator (`h1 ~ p`)
- [ ] Pseudo-classes (`:first-child`, `:nth-child()`, `:not()`)
- [ ] Pseudo-elements (`::before`, `::after`, `::first-line`)

### CSS Properties

- [x] `color` (named, hex, `rgb()`)
- [x] `background-color`
- [x] `font-size` (px, pt, em)
- [x] `font-weight` (normal, bold, 100-900)
- [x] `font-style` (normal, italic)
- [x] `font-family` (generic families: serif, sans-serif, monospace)
- [x] `text-align` (left, center, right)
- [x] `line-height`
- [x] `margin` (shorthand and `margin-bottom`)
- [x] `padding` (shorthand and individual sides)
- [x] `border` (shorthand), `border-width`, `border-color`
- [ ] `margin-top`, `margin-left`, `margin-right`
- [ ] `width`, `height`, `min-width`, `max-width`, `min-height`, `max-height`
- [ ] `display` (block, inline, inline-block, none, flex, grid)
- [ ] `position` (static, relative, absolute, fixed)
- [ ] `top`, `right`, `bottom`, `left`
- [ ] `float`, `clear`
- [ ] `overflow`
- [ ] `opacity`
- [ ] `border-radius`
- [ ] `border-style`
- [ ] `text-decoration` (underline, line-through)
- [ ] `text-transform` (uppercase, lowercase, capitalize)
- [ ] `text-indent`
- [ ] `white-space` (via `<pre>` only, not as CSS property)
- [ ] `vertical-align`
- [ ] `letter-spacing`, `word-spacing`
- [ ] `box-shadow`
- [ ] `background-image`, `background-repeat`, `background-size`, `background-position`
- [ ] `list-style-type`, `list-style-position`
- [ ] CSS custom properties (`var()`)
- [ ] `calc()`

### CSS At-Rules

- [ ] `@page` (page size, margins, margin boxes)
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
- [ ] Justify alignment
- [ ] Floats
- [ ] Absolute/relative positioning
- [ ] Flexbox
- [ ] Grid
- [ ] Multi-column layout
- [ ] Table layout
- [ ] Inline-block
- [x] CSS style inheritance (parent to child)
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
- [ ] Bookmarks/outline (from headings)
- [ ] Clickable links (internal and external)
- [ ] Document metadata (title, author, keywords)
- [ ] Table of contents
- [ ] Page numbers / counters
- [ ] Headers and footers (`@page` margin boxes)
- [ ] Image embedding (PNG, JPEG)
- [ ] SVG rendering
- [ ] PDF/A compliance
- [ ] Compression
- [ ] Encryption

### Colors

- [x] Named colors (red, blue, green, etc.)
- [x] Hex colors (`#rgb`, `#rrggbb`)
- [x] `rgb()` / `rgba()` functions
- [ ] `hsl()` / `hsla()` functions
- [ ] CMYK colors
- [ ] Transparency/alpha
- [ ] Gradients (linear, radial, conic)
