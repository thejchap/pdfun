# Getting Started

## Install

```bash
pip install pdfun
```

Python 3.12 or newer. pdfun ships as a compiled Rust extension; wheels are published for Linux, macOS, and Windows — no system dependencies to install.

For local development:

```bash
git clone https://github.com/thejchap/pdfun
cd pdfun
uv sync
uv run maturin develop --release
```

## Your first PDF

```python
from pdfun import HtmlDocument

HtmlDocument(string="<h1>Hello, pdfun!</h1>").write_pdf("hello.pdf")
```

That's it. `hello.pdf` is a one-page document with the heading rendered in the default font.

## Styling with CSS

Put a `<style>` block inline or a `<link rel="stylesheet">`. Most of CSS 2.1 layout works — see [CSS Support](css-support.md) for the full list.

```python
from pdfun import HtmlDocument

html = """
<style>
  @page { size: A4; margin: 2cm; }
  body { font-family: Helvetica; font-size: 11pt; line-height: 1.4; }
  h1 { color: #1a237e; border-bottom: 2pt solid #1a237e; padding-bottom: 4pt; }
  .note { background: #fff3e0; padding: 8pt; border-left: 3pt solid #ff9800; }
</style>
<h1>Quarterly Report</h1>
<p class="note">Figures are preliminary and subject to revision.</p>
"""

HtmlDocument(string=html).write_pdf("report.pdf")
```

## Table of contents

Pass `toc=True` (or a custom title) to prepend an auto-generated ToC built from `<h1>`–`<h6>`:

```python
HtmlDocument(string=html, toc="Contents").write_pdf("report.pdf")
```

Headings without `id` attributes get them auto-assigned, and the ToC entries are clickable internal links in the PDF.

## Images

Relative paths are resolved against `base_url` (defaults to the current working directory):

```python
HtmlDocument(
    string='<img src="logo.png" style="width: 200pt">',
    base_url="/path/to/assets",
).write_pdf("out.pdf")
```

PNG and JPEG are supported.

## Low-level Layout API

When you need to place text and shapes without HTML, use the layout API directly:

```python
from pdfun import PdfDocument, Layout, TextRun

doc = PdfDocument()
layout = Layout(doc)
layout.add_text("Invoice", font_size=24.0, spacing_after=12.0)
layout.add_paragraph(runs=[
    TextRun("Amount due: ", font_name="Helvetica-Bold"),
    TextRun("$1,234.56"),
])
layout.finish()
doc.save("invoice.pdf")
```

The `Layout` takes care of page breaks and line wrapping; `PdfDocument` owns the page list and font table. For raw placement (no layout), grab a `Page` directly via `doc.add_page()` and call `page.draw_text(x, y, ...)`, `page.draw_rect(...)`, etc.

## CLI

```bash
pdfun render input.html -o output.pdf
```

See `pdfun render --help` for flags.

## Examples

Runnable versions of the above live in the repo's [`examples/`](https://github.com/thejchap/pdfun/tree/master/examples) directory.
