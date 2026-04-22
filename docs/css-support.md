# CSS Support

pdfun implements a WeasyPrint-compatible subset of CSS 2.1 and a handful of CSS 3 modules. The [parity matrix](PARITY.md) is the machine-readable list; this page is the prose overview — what works, what doesn't, and the things that commonly trip people up.

## What works

### Selectors
Type, class, id, descendant, child, adjacent-sibling, universal, and the pseudo-elements `::before` / `::after` with `content:`. Attribute selectors (`[href]`, `[href="/"]`, `[class~="x"]`).

### Box model
`margin`, `padding`, `border`, `width`, `height`, `min-width` / `min-height` / `max-width` / `max-height`, `box-sizing`, `border-radius`, `border-style` (solid/dashed/dotted), individual `border-*-width`/`-color`/`-style`.

### Positioning & flow
`display: block | inline | inline-block | none | list-item`, `float: left | right | none`, `clear`, `position: static | relative` with `top`/`right`/`bottom`/`left`, `overflow: visible | hidden | scroll | auto` (the last two clip like `hidden` in paged media).

### Typography
`font-family`, `font-size`, `font-weight`, `font-style`, `line-height`, `letter-spacing`, `word-spacing`, `text-align`, `text-decoration`, `text-indent`, `text-transform`, `white-space`, `vertical-align`.

### Colors & backgrounds
Named colors, `#rgb` / `#rrggbb` / `#rrggbbaa`, `rgb()` / `rgba()`, `hsl()` / `hsla()`. `background-color`, `background-image: url(...)`, `background-repeat`, `background-size`, `background-position`.

### Lengths
`px`, `pt`, `pc`, `in`, `cm`, `mm`, `em`, `rem`, `%`, plus `calc()` with `+ - * /`.

### Paged media
`@page { size: A4; margin: ...; }` (named sizes and explicit dimensions), `page-break-before`/`-after`/`-inside`, `orphans`, `widows`, `@media print`, `@import`.

### Lists
`ul`, `ol`, `list-style-type` (disc/circle/square/decimal/lower-alpha/upper-alpha/lower-roman/upper-roman/none), `list-style-position`.

### Tables
`<table>`, `<thead>`, `<tbody>`, `<tr>`, `<td>`, `<th>`, `border-collapse`, `border-spacing`, column widths.

### Auto-generated ToC
Pass `toc=True` to `HtmlDocument(...)` (see [Getting Started](getting-started.md)).

## What doesn't

These are **not** implemented — attempts to use them won't crash, but the result will ignore the property:

- `flexbox`, `grid`
- `position: absolute`, `position: fixed`
- `@font-face` (only built-in PDF fonts work: Helvetica, Times, Courier, Symbol, ZapfDingbats)
- SVG (`<svg>` elements are ignored)
- Transforms, transitions, animations
- `filter`, `backdrop-filter`, `clip-path`
- CSS custom properties (`var(--x)`)
- `min()` / `max()` / `clamp()` (but `calc()` works)
- Multi-column layout
- `@counter-style`, `@supports`, `@keyframes`
- PDF encryption / password protection

See the full [parity matrix](PARITY.md) for the exhaustive list.

## Pitfalls

**`px` vs `pt`.** 1pt = 1/72in. 1px = 1/96in (CSS's anchored pixel). pdfun honors both but internally everything is pt, so rounding in px can surprise you: `border: 1px solid` renders as 0.75pt — usually what you want, but if you're comparing `×1.0pt` to `1px` they won't match.

**`@page` margins vs `body { margin }`.** `@page margin` is the printable area; `body { margin }` is additional space inside that. If your content looks "double-indented," you've probably set both.

**Built-in fonts only.** `font-family: Arial` silently falls back to Helvetica; `font-family: 'My Custom Font'` falls back too. There is no `@font-face` yet — register only the 14 PDF base fonts.

**Relative images.** `<img src="logo.png">` resolves against `base_url=` (kwarg to `HtmlDocument`), not against the CSS file's location. If your CSS imports have relative `url(...)` paths, they use `base_url` too.

**Overflow in paged media.** `overflow: hidden` clips to the padding box. `overflow: scroll` and `overflow: auto` clip the same way in print — there are no scrollbars in a PDF. This matches WeasyPrint.

**`position: relative` does not affect page breaks.** Shifting a block with `top: 100pt` moves the paint but not the layout; siblings don't reflow and the box still counts where it originally was for pagination.
