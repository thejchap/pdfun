"""Table-of-contents generation from heading structure.

Scans HTML for heading elements (``<h1>``..``<h6>``), auto-assigns ``id``
attributes to any that lack one, and builds a ToC HTML snippet with
internal links to each heading. The renderer resolves those links into
PDF GoTo actions via the existing anchor registry, so entries are
clickable in the output PDF.

Page numbers are intentionally not emitted in this first version —
mirroring WeasyPrint's ``target-counter()``/``target-text()`` semantics
is a follow-up that requires multi-pass layout. Entries are rendered as
a nested ``<ol>`` with one ``<li>`` per heading, indented by level.
"""

from __future__ import annotations

from html import escape
from html.parser import HTMLParser

_DEFAULT_TITLE = "Table of Contents"
_HEADING_TAG_LEN = 2  # "h1".."h6"
_MAX_HEADING_LEVEL = 6


class _HeadingScanner(HTMLParser):
    """Walks HTML and records (level, text, id) for every heading.

    Collects heading IDs when present; we generate missing ones in a
    separate rewrite pass keyed on the same document offsets.
    """

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.headings: list[tuple[int, str, str | None, int]] = []
        self._current_level: int | None = None
        self._current_id: str | None = None
        self._current_start: int | None = None
        self._current_text: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if len(tag) == _HEADING_TAG_LEN and tag[0] == "h" and tag[1].isdigit():
            level = int(tag[1])
            if 1 <= level <= _MAX_HEADING_LEVEL:
                self._current_level = level
                self._current_id = None
                self._current_text = []
                offset = self.getpos()
                self._current_start = offset[0]
                for name, value in attrs:
                    if name == "id" and value:
                        self._current_id = value

    def handle_endtag(self, tag: str) -> None:
        if (
            self._current_level is not None
            and len(tag) == _HEADING_TAG_LEN
            and tag[0] == "h"
            and tag[1].isdigit()
            and int(tag[1]) == self._current_level
        ):
            text = "".join(self._current_text).strip()
            if text:
                self.headings.append(
                    (
                        self._current_level,
                        text,
                        self._current_id,
                        self._current_start or 0,
                    )
                )
            self._current_level = None
            self._current_id = None
            self._current_text = []
            self._current_start = None

    def handle_data(self, data: str) -> None:
        if self._current_level is not None:
            self._current_text.append(data)


def _inject_ids(
    html: str,
    headings: list[tuple[int, str, str | None, int]],
) -> tuple[str, list[tuple[int, str, str]]]:
    """Return ``(modified_html, resolved_headings)`` with IDs on every heading.

    The resolver walks the original source, generating stable
    ``pdfun-toc-h-<n>`` IDs for any heading missing one. IDs the user
    supplied are preserved verbatim.
    """
    resolved: list[tuple[int, str, str]] = []
    auto_idx = 0
    out: list[str] = []
    cursor = 0
    heading_iter = iter(headings)
    pending = next(heading_iter, None)

    lines = html.splitlines(keepends=True)
    line_starts = [0]
    for line in lines:
        line_starts.append(line_starts[-1] + len(line))

    while pending is not None:
        level, text, existing_id, line_no = pending
        if existing_id is not None:
            resolved.append((level, text, existing_id))
            pending = next(heading_iter, None)
            continue

        heading_id = f"pdfun-toc-h-{auto_idx}"
        auto_idx += 1
        resolved.append((level, text, heading_id))

        if line_no > 0:
            line_idx = min(line_no - 1, len(line_starts) - 1)
            search_start = line_starts[line_idx]
        else:
            search_start = 0
        search_start = max(search_start, cursor)
        tag_open = html.find(f"<h{level}", search_start)
        if tag_open < 0:
            pending = next(heading_iter, None)
            continue
        tag_close = html.find(">", tag_open)
        if tag_close < 0:
            pending = next(heading_iter, None)
            continue
        out.append(html[cursor:tag_close])
        out.append(f' id="{heading_id}"')
        cursor = tag_close
        pending = next(heading_iter, None)

    out.append(html[cursor:])
    return "".join(out), resolved


def build_toc(
    html: str,
    *,
    title: str = _DEFAULT_TITLE,
) -> tuple[str, str]:
    """Scan ``html`` for headings and return ``(modified_html, toc_html)``.

    ``modified_html`` has an auto-generated ``id`` attribute on any
    heading that lacked one. ``toc_html`` is a standalone snippet the
    caller can prepend to the document body; it contains a heading with
    ``title`` followed by an ordered list of clickable links. A trailing
    ``page-break-after: always`` spacer isolates the ToC on its own
    page(s).
    """
    scanner = _HeadingScanner()
    scanner.feed(html)
    modified, resolved = _inject_ids(html, scanner.headings)

    if not resolved:
        return modified, ""

    entries = []
    for level, text, heading_id in resolved:
        indent_px = (level - 1) * 20
        entries.append(
            f'<li style="margin-left:{indent_px}px">'
            f'<a href="#{escape(heading_id, quote=True)}">{escape(text)}</a>'
            f"</li>"
        )
    body = "".join(entries)
    toc_html = (
        f'<nav class="pdfun-toc">'
        f"<h1>{escape(title)}</h1>"
        f"<ol>{body}</ol>"
        f"</nav>"
        f'<div style="page-break-after:always"></div>'
    )
    return modified, toc_html
