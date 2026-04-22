# pdfun

Pure-Rust HTML/CSS to PDF renderer with Python bindings. A [WeasyPrint](https://github.com/Kozea/WeasyPrint)-compatible subset, with **zero system dependencies** — no Pango, Cairo, GObject, or GTK to install.

## Why

WeasyPrint is excellent and we target a compatible subset of its feature set, but it drags in a large C/GObject stack that makes install painful in containers, serverless, and locked-down environments. pdfun is a pure-Rust alternative — `pip install pdfun` and nothing else.

## Where to start

- **[Getting Started](getting-started.md)** — install, your first PDF, and how to style it.
- **[CSS Support](css-support.md)** — which CSS properties work, which don't, and the pitfalls that bite.
- **[Architecture](architecture.md)** — how the HTML → layout → PDF pipeline is wired.
- **[Parity Matrix](PARITY.md)** — auto-generated per-behavior status against the CSS spec and WeasyPrint.

## Status

Roughly 114 of 133 tracked behaviors are implemented. The [parity matrix](PARITY.md) is the authoritative list. Non-goals for now: flexbox, grid, `@font-face`, `position: absolute`/`fixed`, SVG, and PDF encryption.

## Source

Code and issues live on [GitHub](https://github.com/thejchap/pdfun). Contributions welcome — see [CONTRIBUTING.md](https://github.com/thejchap/pdfun/blob/master/CONTRIBUTING.md).
