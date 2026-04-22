# Architecture

One-page orientation for contributors. Details live in the source; this page is the map.

## Pipeline

```
HTML string ─▶ html5ever parse ─▶ DOM ─▶ CSS cascade ─▶ box tree ─▶ layout ─▶ paint ops ─▶ PDF writer ─▶ bytes
               (scraper)                  (src/css.rs)   (src/box_  (src/     (src/          (pdf-writer
                                                          tree.rs)   layout.   lib.rs)        crate)
                                                                     rs)
```

Each arrow is a function call, not a process boundary. The Python side is a thin wrapper (`src/pdfun/*.py`) that mostly calls into the Rust extension `pdfun._core`.

## Crates we depend on

- [`pdf-writer`](https://crates.io/crates/pdf-writer) — low-level PDF writer.
- [`scraper`](https://crates.io/crates/scraper) — HTML parsing (wraps html5ever).
- [`cssparser`](https://crates.io/crates/cssparser) — CSS tokenizer.
- [`image`](https://crates.io/crates/image) — PNG / JPEG decode for `<img>` and `background-image`.
- [`ttf-parser`](https://crates.io/crates/ttf-parser) — font metrics for text measurement.
- [`pyo3`](https://crates.io/crates/pyo3) — Python bindings.

All of these are pure Rust. No system libraries are linked.

## Where things live

| Area | File | What it does |
|---|---|---|
| Python entry | `src/pdfun/__init__.py` | Re-exports the public API |
| HTML wrapper | `src/pdfun/html.py` | `HtmlDocument` class, ToC prepending |
| ToC builder | `src/pdfun/toc.py` | Heading scrape → `<ul>` markup |
| CLI | `src/pdfun/cli.py` | `click`-based `pdfun render` |
| Python ↔ Rust | `src/lib.rs` | PyO3 `#[pymodule]`, page/document/font types |
| HTML → box tree | `src/html_render.rs` | DOM walk, pseudo-element insertion, style dispatch |
| CSS parser | `src/css.rs` | Property parsing, inheritance, `calc()`, @page |
| Box tree | `src/box_tree.rs` | Intermediate tree shape |
| Layout | `src/layout.rs` | Line breaking, page breaking, float placement, background/border paint |
| DOM helpers | `src/dom.rs` | Shared node-walking utilities |
| Fonts | `src/font_metrics.rs` | Per-font width tables for the 14 built-ins |
| Images | `src/image.rs` | PNG/JPEG decode, XObject registration |

## Data flow per page

1. **Parse** HTML into a `scraper` tree.
2. **Cascade** — walk each element, collect matching rules from inline `<style>`, `<link>`, and the author stylesheet; compute an inherited `ComputedStyle`.
3. **Build** a box tree: one box per generator node, with pseudo-elements (`::before`/`::after`) spliced in.
4. **Layout** — flow children through lines, break into pages, compute final `(x, y, w, h)` for every box.
5. **Paint** — walk laid-out boxes emitting PDF operators (`q`/`Q` save/restore, `cm` transforms, `W n` clips, `Do` for XObjects, `Tj` for text).
6. **Emit** — `pdf-writer` serializes the object graph into the final byte stream; content streams are FlateDecode-compressed.

## Font story

Only the 14 built-in PDF fonts are understood today: Helvetica (×4), Times (×4), Courier (×4), Symbol, ZapfDingbats. Each has a hardcoded AFM metrics table in `src/font_metrics.rs`. `@font-face` and system-font discovery are **not implemented** — they're on the roadmap but gated on picking a pure-Rust font shaping story (likely `rustybuzz`).

## Tests

- `tests/test_pdfun.py` — the low-level API (PdfDocument, Layout, Page).
- `tests/test_html.py` — HTML/CSS end-to-end, assertions against decompressed content streams (see `tests/_pdf_helpers.py`).
- `tests/test_text_runs.py` — multi-run paragraphs.
- `tests/test_visual.py` — `visual snapshot` style comparisons.

Parity tracking lives in `tools/parity/catalog.toml` plus inline `# spec:` markers in tests. `tools/parity/generate.py --check` regenerates `docs/PARITY.md` and fails CI if it drifts.
