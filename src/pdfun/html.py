"""HTML-to-PDF rendering via Rust html5ever parser."""

from __future__ import annotations

from pdfun._core import html_to_pdf
from pdfun.toc import build_toc


class HtmlDocument:
    """Parse HTML and render to PDF."""

    def __init__(
        self,
        *,
        string: str,
        base_url: str | None = None,
        toc: bool | str = False,
    ) -> None:
        """Create an HtmlDocument from an HTML string.

        ``base_url`` is used to resolve relative paths for images in ``<img>``
        tags. If ``None``, paths are resolved relative to the current
        working directory.

        ``toc`` enables an auto-generated Table of Contents prepended to
        the document. Pass ``True`` for the default heading, or a custom
        title string. The ToC is built from ``<h1>``..``<h6>`` tags and
        produces clickable internal links; missing ``id`` attributes on
        headings are auto-assigned.
        """
        if toc:
            title = toc if isinstance(toc, str) else "Table of Contents"
            modified, toc_html = build_toc(string, title=title)
            string = toc_html + modified
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
