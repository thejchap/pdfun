"""HTML-to-PDF rendering using Python's stdlib html.parser."""

from __future__ import annotations

from html import unescape
from html.parser import HTMLParser
from typing import TypedDict

from pdfun._core import Layout, TextRun, html_to_pdf


class _Style(TypedDict):
    font: str
    font_size: float
    spacing_after: float


UA_STYLES: dict[str, _Style] = {
    "h1": {"font": "Helvetica-Bold", "font_size": 24.0, "spacing_after": 16.0},
    "h2": {"font": "Helvetica-Bold", "font_size": 18.0, "spacing_after": 14.0},
    "h3": {"font": "Helvetica-Bold", "font_size": 14.0, "spacing_after": 12.0},
    "h4": {"font": "Helvetica-Bold", "font_size": 12.0, "spacing_after": 10.0},
    "h5": {"font": "Helvetica-Bold", "font_size": 10.0, "spacing_after": 8.0},
    "h6": {"font": "Helvetica-Bold", "font_size": 8.0, "spacing_after": 8.0},
}
DEFAULT_STYLE: _Style = {
    "font": "Helvetica",
    "font_size": 12.0,
    "spacing_after": 12.0,
}

BLOCK_ELEMENTS = {"h1", "h2", "h3", "h4", "h5", "h6", "p", "div"}
SKIP_ELEMENTS = {"head", "title", "style", "script", "meta", "link"}
INLINE_BOLD = {"b", "strong"}
INLINE_ITALIC = {"i", "em"}
LIST_ELEMENTS = {"ul", "ol"}
UL_MARKERS = ["-", "o", "-"]
LIST_INDENT = 36.0
LIST_ITEM_SPACING = 4.0

FONT_VARIANTS: dict[str, dict[tuple[bool, bool], str]] = {
    "Helvetica": {
        (False, False): "Helvetica",
        (True, False): "Helvetica-Bold",
        (False, True): "Helvetica-Oblique",
        (True, True): "Helvetica-BoldOblique",
    },
    "Times": {
        (False, False): "Times-Roman",
        (True, False): "Times-Bold",
        (False, True): "Times-Italic",
        (True, True): "Times-BoldItalic",
    },
    "Courier": {
        (False, False): "Courier",
        (True, False): "Courier-Bold",
        (False, True): "Courier-Oblique",
        (True, True): "Courier-BoldOblique",
    },
}


def _resolve_font(base_font: str, bold: bool, italic: bool) -> str:
    """Resolve a font variant given bold/italic flags."""
    for family, variants in FONT_VARIANTS.items():
        if base_font.startswith(family):
            eff_bold = bold or "Bold" in base_font
            eff_italic = italic or "Italic" in base_font or "Oblique" in base_font
            return variants[(eff_bold, eff_italic)]
    return base_font


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


class _BlockExtractor(HTMLParser):
    def __init__(self, layout: Layout) -> None:
        super().__init__()
        self._layout = layout
        self._runs: list[TextRun] = []
        self._current_text: list[str] = []
        self._current_tag: str | None = None
        self._skip_depth = 0
        self._bold_depth = 0
        self._italic_depth = 0
        self._list_stack: list[dict[str, str | int]] = []

    def handle_starttag(
        self,
        tag: str,
        attrs: list[tuple[str, str | None]],  # noqa: ARG002
    ) -> None:
        if tag in SKIP_ELEMENTS:
            self._skip_depth += 1
        elif tag in LIST_ELEMENTS:
            self._flush()
            self._list_stack.append({"type": tag, "counter": 0})
        elif tag == "li":
            self._flush()
            if self._list_stack:
                entry = self._list_stack[-1]
                entry["counter"] = int(entry["counter"]) + 1
            self._current_tag = "li"
        elif tag in BLOCK_ELEMENTS:
            self._flush()
            self._current_tag = tag
        elif tag == "br":
            self._flush()
        elif tag in INLINE_BOLD:
            self._flush_run()
            self._bold_depth += 1
        elif tag in INLINE_ITALIC:
            self._flush_run()
            self._italic_depth += 1

    def handle_endtag(self, tag: str) -> None:
        if tag in SKIP_ELEMENTS and self._skip_depth > 0:
            self._skip_depth -= 1
        elif tag in LIST_ELEMENTS and self._list_stack:
            self._flush()
            self._list_stack.pop()
        elif tag in {*BLOCK_ELEMENTS, "li"}:
            self._flush()
            self._current_tag = None
        elif tag in INLINE_BOLD and self._bold_depth > 0:
            self._flush_run()
            self._bold_depth -= 1
        elif tag in INLINE_ITALIC and self._italic_depth > 0:
            self._flush_run()
            self._italic_depth -= 1

    def handle_data(self, data: str) -> None:
        if self._skip_depth > 0:
            return
        self._current_text.append(data)

    def handle_entityref(self, name: str) -> None:
        self._current_text.append(unescape(f"&{name};"))

    def handle_charref(self, name: str) -> None:
        self._current_text.append(unescape(f"&#{name};"))

    def _flush_run(self) -> None:
        """Save current text as a TextRun with current inline styling."""
        text = "".join(self._current_text)
        self._current_text = []
        if not text:
            return
        tag = self._current_tag
        style = UA_STYLES.get(tag, DEFAULT_STYLE) if tag else DEFAULT_STYLE
        base_font = style["font"]
        font_size = style["font_size"]

        bold = self._bold_depth > 0
        italic = self._italic_depth > 0
        resolved_font = _resolve_font(base_font, bold, italic)

        self._runs.append(
            TextRun(text, font_name=resolved_font, font_size=font_size),
        )

    def _flush(self) -> None:
        """Flush all accumulated runs as a paragraph."""
        self._flush_run()
        if not self._runs:
            return
        runs = self._runs
        self._runs = []

        tag = self._current_tag

        # List item: add marker and indentation
        if tag == "li" and self._list_stack:
            entry = self._list_stack[-1]
            depth = len(self._list_stack) - 1
            padding_left = (depth + 1) * LIST_INDENT

            if entry["type"] == "ol":
                marker_text = f"{entry['counter']}."
            else:
                marker_text = UL_MARKERS[depth % len(UL_MARKERS)]

            marker = TextRun(marker_text)

            self._layout.add_paragraph(
                runs=runs,
                spacing_after=LIST_ITEM_SPACING,
                padding=(0.0, 0.0, 0.0, padding_left),
                marker=marker,
            )
            return

        style = UA_STYLES.get(tag, DEFAULT_STYLE) if tag else DEFAULT_STYLE

        self._layout.add_paragraph(
            runs=runs,
            spacing_after=style["spacing_after"],
        )


def _parse_into_layout(html: str, layout: Layout) -> None:
    parser = _BlockExtractor(layout)
    parser.feed(html)
    parser._flush()  # noqa: SLF001
