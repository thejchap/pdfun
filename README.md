# pdfun

<p align="center">
  <a href="https://github.com/thejchap/pdfun/actions/workflows/ci.yml">
    <img src="https://github.com/thejchap/pdfun/actions/workflows/ci.yml/badge.svg" alt="CI" />
  </a>
  <a href="https://github.com/astral-sh/ruff">
    <img src="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/astral-sh/ruff/main/assets/badge/v2.json" alt="ruff" />
  </a>
  <a href="https://python.org">
    <img src="https://img.shields.io/badge/python-3.12%20%7C%203.13%20%7C%203.14%20%7C%203.15-blue.svg" alt="python" />
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

## Feature parity

See [`docs/PARITY.md`](docs/PARITY.md) for a per-behavior matrix of CSS spec coverage and WeasyPrint comparison. It is auto-generated from [`tools/parity/catalog.toml`](tools/parity/catalog.toml) and inline `spec:` markers in tests; CI enforces freshness via `uv run python tools/parity/generate.py --check`.
