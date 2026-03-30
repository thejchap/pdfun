"""HTML-to-PDF rendering via Rust html5ever parser."""

from __future__ import annotations

from pdfun._core import html_to_pdf


class HtmlDocument:
    """Parse HTML and render to PDF."""

    def __init__(self, *, string: str) -> None:
        """Create an HtmlDocument from an HTML string."""
        self._html = string

    def write_pdf(self, path: str) -> None:
        """Write the rendered PDF to a file."""
        doc = html_to_pdf(self._html)
        doc.save(path)

    def to_bytes(self) -> bytes:
        """Return the rendered PDF as bytes."""
        doc = html_to_pdf(self._html)
        return doc.to_bytes()
