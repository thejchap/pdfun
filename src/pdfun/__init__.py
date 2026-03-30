"""pdfun -- pure-Rust PDF generation from Python."""

from pdfun._core import (
    FontDatabase,
    FontId,
    Layout,
    Page,
    PdfDocument,
    TextRun,
    text_width,
    wrap_text,
)
from pdfun.cli import main
from pdfun.html import HtmlDocument

__all__ = [
    "FontDatabase",
    "FontId",
    "HtmlDocument",
    "Layout",
    "Page",
    "PdfDocument",
    "TextRun",
    "main",
    "text_width",
    "wrap_text",
]
