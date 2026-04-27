"""HTML-to-PDF rendering via Rust html5ever parser."""

from __future__ import annotations

from collections.abc import Callable

from pdfun._core import html_to_pdf
from pdfun.toc import build_toc

# Mirrors WeasyPrint's ``url_fetcher=`` parameter: ``(url) -> bytes``.
# Returning ``None`` (or raising) signals a fetch failure that becomes a
# render warning rather than aborting the whole document.
UrlFetcher = Callable[[str], bytes | None]


class HtmlDocument:
    """Parse HTML and render to PDF."""

    def __init__(
        self,
        *,
        string: str,
        base_url: str | None = None,
        toc: bool | str = False,
        url_fetcher: UrlFetcher | None = None,
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

        ``url_fetcher`` is a callable ``(url) -> bytes`` that the renderer
        uses to load remote ``<img>``, ``background-image``, and
        ``@font-face`` URLs. If omitted, pdfun's default fetcher handles
        ``file://`` and bare paths only; HTTP(S) URLs require either the
        ``http-fetch`` Cargo feature or a custom callable. Mirrors
        WeasyPrint's ``url_fetcher=`` parameter.
        """
        if toc:
            title = toc if isinstance(toc, str) else "Table of Contents"
            modified, toc_html = build_toc(string, title=title)
            string = toc_html + modified
        self._html = string
        self._base_url = base_url
        self._url_fetcher = url_fetcher

    def write_pdf(self, path: str) -> None:
        """Write the rendered PDF to a file."""
        doc = html_to_pdf(
            self._html,
            base_url=self._base_url,
            url_fetcher=self._url_fetcher,
        )
        doc.save(path)

    def to_bytes(self) -> bytes:
        """Return the rendered PDF as bytes."""
        doc = html_to_pdf(
            self._html,
            base_url=self._base_url,
            url_fetcher=self._url_fetcher,
        )
        return doc.to_bytes()

    def warnings(self) -> list[str]:
        """Non-fatal diagnostics from rendering.

        Image load failures, ``@font-face`` sources we can't load, etc. Each
        call re-renders, so it's safe alongside ``to_bytes`` but inefficient
        for large documents — cache the document if you need both.
        """
        doc = html_to_pdf(
            self._html,
            base_url=self._base_url,
            url_fetcher=self._url_fetcher,
        )
        return list(doc.warnings)
