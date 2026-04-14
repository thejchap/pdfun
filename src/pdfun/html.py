"""HTML-to-PDF rendering via Rust html5ever parser."""

from __future__ import annotations

from pdfun._core import html_to_pdf


class HtmlDocument:
    """Parse HTML and render to PDF."""

    def __init__(self, *, string: str, base_url: str | None = None) -> None:
        """Create an HtmlDocument from an HTML string.

        ``base_url`` is used to resolve relative paths for images in ``<img>``
        tags. If ``None``, paths are resolved relative to the current
        working directory.
        """
        self._html = string
        self._base_url = base_url

    def write_pdf(self, path: str) -> None:
        """Write the rendered PDF to a file."""
        doc = html_to_pdf(self._html, base_url=self._base_url)
        doc.save(path)

    def to_bytes(self) -> bytes:
        """Return the rendered PDF as bytes."""
        doc = html_to_pdf(self._html, base_url=self._base_url)
        return doc.to_bytes()
