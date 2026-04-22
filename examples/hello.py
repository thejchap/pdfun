"""Minimal HTML to PDF. Run with ``uv run python examples/hello.py``."""

from pdfun import HtmlDocument

HtmlDocument(string="<h1>Hello, pdfun!</h1>").write_pdf("hello.pdf")
print("wrote hello.pdf")
