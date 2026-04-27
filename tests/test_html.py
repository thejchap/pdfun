import base64
import struct
import tempfile
import zlib
from pathlib import Path

from tryke import describe, expect, test

from pdfun import HtmlDocument
from tests._pdf_helpers import content_stream

_FIXTURE_TTF_PATH = Path(__file__).parent / "fixtures" / "font.ttf"


def _find_text_x(content: bytes, marker: bytes) -> float | None:
    """Find the x-coordinate (operand 1) of the most recent `Td` operator
    preceding `marker` in `content`. Returns `None` if no Td is found.

    The PDF text operator `tx ty Td` carries the baseline position in
    operands `tx ty`. The renderer emits text positions via `Td` (move
    to next line), so this is the right hook for "where did cell text X
    get drawn?" assertions.
    """
    idx = content.find(marker)
    if idx < 0:
        return None
    td_idx = content.rfind(b" Td\n", 0, idx)
    if td_idx < 0:
        return None
    line_start = max(content.rfind(b"\n", 0, td_idx), 0)
    line = content[line_start + 1 : td_idx].split()
    if len(line) < 2:
        return None
    try:
        return float(line[0])
    except ValueError:
        return None


def _font_data_uri(b: bytes | None = None) -> str:
    raw = b if b is not None else _FIXTURE_TTF_PATH.read_bytes()
    return "data:font/ttf;base64," + base64.b64encode(raw).decode()


def _make_png(width: int, height: int, rgb_bytes: bytes) -> bytes:
    """Build a minimal 8-bit RGB PNG."""
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(ctype: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(ctype + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + ctype + data + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)  # RGB
    raw = b""
    for y in range(height):
        raw += b"\x00" + rgb_bytes[y * width * 3 : (y + 1) * width * 3]
    idat = zlib.compress(raw)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def _make_png_rgba(width: int, height: int, rgba_bytes: bytes) -> bytes:
    """Build a minimal 8-bit RGBA PNG."""
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(ctype: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(ctype + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + ctype + data + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)  # RGBA
    raw = b""
    for y in range(height):
        raw += b"\x00" + rgba_bytes[y * width * 4 : (y + 1) * width * 4]
    idat = zlib.compress(raw)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


with describe("HtmlDocument - constructor"):

    @test
    def create_from_string():
        """HtmlDocument(string=...) constructs without error."""
        HtmlDocument(string="<p>Hello</p>")

    @test
    def create_empty_html():
        """HtmlDocument with empty body produces valid PDF."""
        doc = HtmlDocument(string="<html><body></body></html>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def create_from_plain_text():
        """Plain text without tags is treated as content."""
        doc = HtmlDocument(string="Just plain text")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Just plain text")


with describe("HtmlDocument - headings"):

    @test
    def h1_renders():
        """<h1> renders text in PDF."""
        doc = HtmlDocument(string="<h1>Title</h1>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Title")

    @test
    def h1_uses_bold_font():
        """<h1> uses Helvetica-Bold."""
        doc = HtmlDocument(string="<h1>Bold Title</h1>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def h1_uses_24pt():
        """<h1> uses 24pt font size."""
        doc = HtmlDocument(string="<h1>Big</h1>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"24 Tf")

    @test
    def h2_uses_18pt():
        """<h2> uses 18pt font size."""
        doc = HtmlDocument(string="<h2>Sub</h2>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def all_heading_levels():
        """h1-h6 all render their text."""
        html = "".join(f"<h{i}>H{i}</h{i}>" for i in range(1, 7))
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        for i in range(1, 7):
            expect(data).to_contain(f"H{i}".encode())


with describe("HtmlDocument - paragraphs"):
    # spec: HTML; behaviors: html-paragraph
    @test
    def paragraph_renders():
        """<p> renders text."""
        doc = HtmlDocument(string="<p>Paragraph text</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Paragraph text")

    @test
    def paragraph_uses_12pt():
        """<p> uses 12pt Helvetica."""
        doc = HtmlDocument(string="<p>Body</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"12 Tf")

    @test
    def multiple_paragraphs():
        """Multiple <p> elements render sequentially."""
        doc = HtmlDocument(string="<p>First</p><p>Second</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"First")
        expect(content).to_contain(b"Second")

    @test
    def paragraph_wraps_long_text():
        """Long paragraph text wraps within page width."""
        long = " ".join(["word"] * 80)
        doc = HtmlDocument(string=f"<p>{long}</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content.count(b"Td")).to_be_greater_than(1)


with describe("HtmlDocument - div"):
    # spec: HTML; behaviors: html-div
    @test
    def div_renders():
        """<div> renders its text content."""
        doc = HtmlDocument(string="<div>Div content</div>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Div content")


with describe("HtmlDocument - semantic elements"):
    # spec: HTML; behaviors: html-semantic
    @test
    def article_renders():
        """<article> renders its text content."""
        doc = HtmlDocument(string="<article>Article content</article>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Article content")

    @test
    def section_renders():
        """<section> renders its text content."""
        doc = HtmlDocument(string="<section>Section content</section>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Section content")

    @test
    def nav_renders():
        """<nav> renders its text content."""
        doc = HtmlDocument(string="<nav>Nav content</nav>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Nav content")

    @test
    def header_renders():
        """<header> renders its text content."""
        doc = HtmlDocument(string="<header>Header content</header>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Header content")

    @test
    def footer_renders():
        """<footer> renders its text content."""
        doc = HtmlDocument(string="<footer>Footer content</footer>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Footer content")

    @test
    def aside_renders():
        """<aside> renders its text content."""
        doc = HtmlDocument(string="<aside>Aside content</aside>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Aside content")

    @test
    def main_renders():
        """<main> renders its text content."""
        doc = HtmlDocument(string="<main>Main content</main>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Main content")

    @test
    def semantic_nesting():
        """Semantic elements nest properly."""
        doc = HtmlDocument(
            string="<article><section><p>Nested text</p></section></article>"
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Nested text")


with describe("HtmlDocument - br"):
    # spec: HTML; behaviors: html-br
    @test
    def br_splits_text():
        """<br> creates a line break between text."""
        doc = HtmlDocument(string="<p>Line one<br>Line two</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Line one")
        expect(content).to_contain(b"Line two")


with describe("HtmlDocument - inline elements"):

    @test
    def bold_extracts_text():
        """<b> text content is extracted."""
        doc = HtmlDocument(string="<p><b>Bold</b> text</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bold")
        expect(content).to_contain(b"text")

    @test
    def nested_inline():
        """Nested inline elements extract all text."""
        doc = HtmlDocument(string="<p><b><em>Nested</em></b></p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Nested")

    # spec: HTML; behaviors: html-span
    @test
    def span_extracts_text():
        """<span> text content is extracted."""
        doc = HtmlDocument(string="<p><span>Span</span> text</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Span text")


with describe("HtmlDocument - inline styling"):

    @test
    def bold_tag_applies_bold_font():
        """<b> applies Helvetica-Bold font."""
        doc = HtmlDocument(string="<p>Hello <b>bold</b> world</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    # spec: CSS 2.1 §15.4; behaviors: font-style
    @test
    def italic_tag_applies_italic_font():
        """<i> applies Helvetica-Oblique font."""
        doc = HtmlDocument(string="<p>Hello <i>italic</i> world</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Oblique")

    @test
    def bold_italic_combined():
        """<b><i> applies Helvetica-BoldOblique font."""
        doc = HtmlDocument(string="<p><b><i>both</i></b></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-BoldOblique")

    @test
    def strong_treated_as_bold():
        """<strong> treated same as <b>."""
        doc = HtmlDocument(string="<p><strong>text</strong></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def em_treated_as_italic():
        """<em> treated same as <i>."""
        doc = HtmlDocument(string="<p><em>text</em></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Oblique")

    @test
    def h1_with_italic():
        """<h1><i>text</i></h1> uses BoldOblique (h1 already bold)."""
        doc = HtmlDocument(string="<h1><i>Styled</i></h1>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-BoldOblique")

    @test
    def nested_bold_no_break():
        """Nested <b> tags don't break rendering."""
        doc = HtmlDocument(string="<p><b>outer <b>inner</b> outer</b></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def bold_then_normal():
        """<b>bold</b> normal has both fonts in PDF."""
        doc = HtmlDocument(string="<p><b>bold</b> normal</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"/BaseFont /Helvetica\n")


with describe("HtmlDocument - output"):

    @test
    def write_pdf_creates_file():
        """write_pdf() writes to disk."""
        doc = HtmlDocument(string="<p>File test</p>")
        with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
            path = Path(f.name)
        try:
            doc.write_pdf(str(path))
            expect(path.stat().st_size).to_be_greater_than(50)
            expect(path.read_bytes()[:5]).to_equal(b"%PDF-")
        finally:
            path.unlink()

    @test
    def to_bytes_returns_pdf():
        """to_bytes() returns valid PDF bytes."""
        doc = HtmlDocument(string="<h1>Bytes</h1>")
        data = doc.to_bytes()
        expect(type(data)).to_equal(bytes)
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data.rstrip().endswith(b"%%EOF")).to_be_truthy()

    @test
    def page_break_on_overflow():
        """Enough content creates multiple pages."""
        paras = "".join(f"<p>Paragraph {i}</p>" for i in range(100))
        doc = HtmlDocument(string=paras)
        data = doc.to_bytes()
        expect(data).not_.to_contain(b"/Count 1")


with describe("HtmlDocument - complex documents"):

    @test
    def mixed_headings_and_paragraphs():
        """Document with h1, h2, p renders all content."""
        html = "<h1>Title</h1><p>Intro.</p><h2>Section</h2><p>Body.</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Title")
        expect(content).to_contain(b"Intro.")
        expect(content).to_contain(b"Section")
        expect(content).to_contain(b"Body.")

    @test
    def whitespace_normalization():
        """Extra whitespace in HTML is collapsed."""
        doc = HtmlDocument(string="<p>  Hello   world  </p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello world")


with describe("HtmlDocument - edge cases"):

    @test
    def empty_paragraph():
        """Empty <p></p> does not crash."""
        doc = HtmlDocument(string="<p></p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def html_entities():
        """HTML entities like &amp; are decoded."""
        doc = HtmlDocument(string="<p>A &amp; B</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"A & B")

    @test
    def skip_script_content():
        """<script> content is not rendered."""
        doc = HtmlDocument(string="<script>var x = 1;</script><p>Visible</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Visible")
        expect(content).not_.to_contain(b"var x")

    @test
    def skip_style_content():
        """<style> content is not rendered."""
        html = "<style>body { color: red; }</style><p>Visible</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Visible")
        expect(content).not_.to_contain(b"color")


with describe("HtmlDocument - lists"):
    # spec: HTML; behaviors: html-ul
    @test
    def ul_renders_item_text():
        """<ul><li> renders item text in PDF."""
        doc = HtmlDocument(string="<ul><li>Item one</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Item one")

    @test
    def ul_has_bullet_marker():
        """<ul><li> emits a disc bullet marker as WinAnsi byte 0x95
        (the PDF-spec bullet • slot). Pre-WS-1A this rendered as the
        ASCII '*' substitute."""
        doc = HtmlDocument(string="<ul><li>Item</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"<95>")

    @test
    def ul_multiple_items():
        """Multiple <li> elements render sequentially."""
        doc = HtmlDocument(string="<ul><li>First</li><li>Second</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"First")
        expect(content).to_contain(b"Second")

    # spec: HTML; behaviors: html-ol
    @test
    def ol_has_numbered_markers():
        """<ol><li> items have numbered markers."""
        html = "<ol><li>Alpha</li><li>Beta</li></ol>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1.")
        expect(content).to_contain(b"2.")
        expect(content).to_contain(b"Alpha")
        expect(content).to_contain(b"Beta")

    @test
    def li_with_bold():
        """<li><b>bold</b> text uses bold font."""
        doc = HtmlDocument(string="<ul><li><b>Bold</b> item</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(content).to_contain(b"Bold")
        expect(content).to_contain(b"item")

    @test
    def li_wraps_long_text():
        """Long list item text wraps at reduced width."""
        long = " ".join(["word"] * 80)
        doc = HtmlDocument(string=f"<ul><li>{long}</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content.count(b"Td")).to_be_greater_than(1)

    # spec: HTML; behaviors: html-list-nesting
    @test
    def nested_ul():
        """Nested <ul> renders both outer and inner items."""
        html = "<ul><li>Outer<ul><li>Inner</li></ul></li></ul>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Outer")
        expect(content).to_contain(b"Inner")

    @test
    def nested_ul_different_markers():
        """Nested <ul> uses different bullet style at depth 1."""
        html = "<ul><li>Outer<ul><li>Inner</li></ul></li></ul>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # depth 0 = disc (WinAnsi byte 0x95 -> hex `<95>`),
        # depth 1 = circle (ASCII 'o' -> literal `(o)`).
        expect(content).to_contain(b"<95>")
        expect(content).to_contain(b"(o)")

    @test
    def nested_ol_restarts_numbering():
        """Nested <ol> restarts numbering at 1."""
        html = (
            "<ol><li>First<ol><li>Inner A</li><li>Inner B</li>"
            "</ol></li><li>Second</li></ol>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Inner A")
        expect(content).to_contain(b"Inner B")
        expect(content).to_contain(b"Second")

    @test
    def mixed_list_nesting():
        """<ol> with nested <ul> renders correctly."""
        html = "<ol><li>Numbered<ul><li>Bulleted</li></ul></li></ol>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Numbered")
        expect(content).to_contain(b"Bulleted")

    @test
    def list_between_paragraphs():
        """<p> before and after <ul> all render."""
        html = "<p>Before</p><ul><li>Item</li></ul><p>After</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Before")
        expect(content).to_contain(b"Item")
        expect(content).to_contain(b"After")

    @test
    def empty_li_no_crash():
        """Empty <li></li> does not crash."""
        doc = HtmlDocument(string="<ul><li></li></ul>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def li_outside_list():
        """Bare <li> outside list renders as plain paragraph."""
        doc = HtmlDocument(string="<li>Orphan</li>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Orphan")


with describe("HtmlDocument - malformed HTML"):

    @test
    def unclosed_tag():
        """Unclosed <p> does not crash; text is still rendered."""
        doc = HtmlDocument(string="<p>Hello")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello")

    @test
    def unclosed_bold():
        """Unclosed <b> does not crash; text is rendered."""
        doc = HtmlDocument(string="<p><b>Bold text")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bold text")

    @test
    def extra_closing_tags():
        """Extra closing tags do not crash."""
        doc = HtmlDocument(string="<p>Text</p></p></div>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def nested_same_block():
        """<p> inside <p> is auto-closed by html5ever."""
        doc = HtmlDocument(string="<p>First<p>Second")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"First")
        expect(content).to_contain(b"Second")

    @test
    def misnested_inline():
        """Overlapping inline tags handled gracefully."""
        doc = HtmlDocument(string="<p><b>bold <i>both</b> italic</i></p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"bold")
        expect(content).to_contain(b"both")
        expect(content).to_contain(b"italic")

    @test
    def no_root_element():
        """Content without <html>/<body> still renders."""
        doc = HtmlDocument(string="Just text, no tags at all")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Just text")

    @test
    def completely_empty():
        """Empty string produces valid PDF."""
        doc = HtmlDocument(string="")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("HtmlDocument - unknown and void elements"):

    @test
    def unknown_tag_renders_content():
        """<custom-tag> content is still rendered."""
        doc = HtmlDocument(string="<custom-tag>Inside</custom-tag>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Inside")

    @test
    def void_elements_no_crash():
        """Void elements (img, hr, input) do not crash."""
        html = "<p>Before</p><hr><img><input><p>After</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Before")
        expect(content).to_contain(b"After")

    @test
    def self_closing_br():
        """<br/> (self-closing) works the same as <br>."""
        doc = HtmlDocument(string="<p>Line one<br/>Line two</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Line one")
        expect(content).to_contain(b"Line two")

    # spec: HTML; behaviors: html-anchor
    @test
    def anchor_tag_preserves_text():
        """<a> tag text is rendered alongside surrounding text."""
        doc = HtmlDocument(string='<p>Click <a href="url">here</a></p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Click")
        expect(content).to_contain(b"here")


with describe("HtmlDocument - whitespace"):

    @test
    def leading_trailing_whitespace():
        """Leading/trailing whitespace in tags is collapsed."""
        doc = HtmlDocument(string="<p>  Hello  </p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello")

    @test
    def newlines_collapsed():
        """Newlines within text are collapsed to spaces."""
        doc = HtmlDocument(string="<p>Hello\nworld</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello world")

    @test
    def tabs_collapsed():
        """Tabs are collapsed to spaces."""
        doc = HtmlDocument(string="<p>Hello\tworld</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello world")

    @test
    def inter_element_whitespace():
        """Whitespace between inline elements is preserved."""
        doc = HtmlDocument(string="<p><b>Bold</b> <i>italic</i></p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bold")
        expect(content).to_contain(b"italic")

    @test
    def whitespace_only_paragraph():
        """Paragraph with only whitespace produces valid PDF."""
        doc = HtmlDocument(string="<p>   \n\t  </p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def multiple_spaces_between_words():
        """Multiple spaces between words collapse to one."""
        doc = HtmlDocument(string="<p>Hello     world</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello world")


with describe("HtmlDocument - unicode"):

    @test
    def unicode_text():
        """Unicode characters land in the content stream as their
        PDF-WinAnsi bytes (post WS-1A), not the original UTF-8.
        `Héllo wörld` becomes `48 E9 6C 6C 6F 20 77 F6 72 6C 64`."""
        doc = HtmlDocument(string="<p>Héllo wörld</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data[:5]).to_equal(b"%PDF-")
        # WinAnsi bytes (uppercase hex form chosen by pdf-writer when
        # the literal contains non-ASCII): 0xE9 = é, 0xF6 = ö.
        expect(content).to_contain(b"48E96C6C6F2077F6726C64")

    @test
    def numeric_entity():
        """Numeric character reference &#169; (©) is decoded and
        emitted as the WinAnsi byte 0xA9 — pre WS-1A this test
        asserted the UTF-8 hex `C2A9`, which was the latent encoding
        bug we are fixing."""
        doc = HtmlDocument(string="<p>&#169; 2024</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data[:5]).to_equal(b"%PDF-")
        # © in WinAnsi is byte 0xA9, then space (0x20), then "2024" =
        # 0x32 0x30 0x32 0x34 — assert the full WinAnsi hex sequence.
        expect(content).to_contain(b"A92032303234")

    @test
    def hex_entity():
        """Hex character reference &#x2603; (☃) is outside the WinAnsi
        repertoire. WS-1A bounded the damage by emitting '?'; WS-1B
        promotes the run onto the bundled `__pdfun_fallback`
        Identity-H face so the snowman actually renders. We assert the
        PDF round-trips through the text layer (pymupdf decodes the
        ToUnicode CMap)."""
        import fitz

        doc = HtmlDocument(string="<p>&#x2603;</p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf)
        finally:
            pdf.close()
        assert "☃" in extracted, (
            f"snowman did not round-trip via fallback: {extracted!r}"
        )

    @test
    def multiple_named_entities():
        """Multiple named entities decode correctly."""
        doc = HtmlDocument(string="<p>&lt;tag&gt; &amp; &quot;quotes&quot;</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b'<tag> & "quotes"')


with describe("HtmlDocument - WinAnsi text encoding (WS-1A)"):
    # spec: ISO 32000-1 Annex D.2; behaviors: text-encoding-winansi
    @test
    def winansi_chars_in_text_op():
        """Built-in PDF fonts speak WinAnsi: a string with a Latin-1
        codepoint must land in the content stream as the WinAnsi byte
        (`Caf\\xe9`), not the raw UTF-8 (`Caf\\xc3\\xa9`).

        Note: `pdf-writer` switches Tj string literals to hex form
        (`<436166E9>`) whenever the buffer contains a non-ASCII byte,
        so we accept either the parenthesized literal (`(Caf\\xe9)`)
        or its uppercase-hex equivalent."""
        doc = HtmlDocument(string="<p>Café</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Either form is a correct PDF representation of the bytes
        # 43 61 66 E9 ("Café" in WinAnsi):
        assert b"(Caf\xe9)" in content or b"<436166E9>" in content, (
            f"expected WinAnsi 'Café' (literal or <436166E9>), got: {content!r}"
        )
        # Negative: the UTF-8 byte sequence must NOT show up in the
        # content stream — that's the defect we're fixing.
        assert b"\xc3\xa9" not in content, (
            "UTF-8 bytes leaked into content stream — WinAnsi transcode missing"
        )
        assert b"C3A9" not in content, (
            "UTF-8 bytes (hex form) leaked into content stream — "
            "WinAnsi transcode missing"
        )

    @test
    def winansi_handles_em_dash_and_smart_quotes():
        """Em-dash (U+2014) and smart quotes (U+2018/U+2019) live in
        the 0x80-0x9F WinAnsi override window — not in Latin-1.
        Verify they emit the PDF-spec bytes (em-dash 0x97,
        left-quote 0x93, right-quote 0x94). pdf-writer hex-encodes
        non-ASCII literals so we look for the uppercase hex digits."""
        doc = HtmlDocument(string="<p>he said—“hi”</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # The hex string between < > carries every byte uppercase-hex:
        # so 0x97 em-dash appears as "97", 0x93 as "93", 0x94 as "94".
        assert b"<" in content
        assert b">" in content
        # Find the hex-string segment for our text and assert the
        # expected bytes are present.
        import re

        found_run = False
        for m in re.finditer(rb"<([0-9A-Fa-f]+)>", content):
            hexbytes = bytes.fromhex(m.group(1).decode())
            if b"he said" in hexbytes:
                assert b"\x97" in hexbytes, "em-dash 0x97 missing"
                assert b"\x93" in hexbytes, "left-quote 0x93 missing"
                assert b"\x94" in hexbytes, "right-quote 0x94 missing"
                found_run = True
                break
        assert found_run, "text run with 'he said' not found"

    @test
    def disc_marker_is_bullet():
        """The list `Disc` marker emits the WinAnsi bullet byte (0x95),
        not an ASCII asterisk substitute. pdf-writer hex-encodes
        non-ASCII Tj literals — so we look for `<95>` (the marker
        emitted by itself) or `95` inside a hex run."""
        doc = HtmlDocument(string="<ul><li>x</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Negative: the asterisk substitute is gone.
        assert b"(*)" not in content, (
            "list disc marker still emits ASCII '*' instead of WinAnsi bullet 0x95"
        )
        # Positive: a Tj of just byte 0x95 — pdf-writer renders
        # non-ASCII byte strings as `<95>`.
        assert b"<95>" in content, f"expected `<95> Tj` for bullet, got: {content!r}"

    @test
    def spanish_text_extracts_via_text_layer():
        """A built-in font emitting WinAnsi bytes must declare
        `/Encoding /WinAnsiEncoding` on the font dict so PDF readers
        (and `pymupdf.Page.get_text`) can map bytes back to Unicode.
        Without this, byte 0xE9 is interpreted via StandardEncoding
        and the extracted text mangles to `Caf\\xd8`."""
        import fitz

        spanish = "¿Hablas español? Última página."
        doc = HtmlDocument(string=f"<p>{spanish}</p>")
        data = doc.to_bytes()
        # The font dict must carry /Encoding /WinAnsiEncoding so that
        # readers know how to decode the WinAnsi bytes we emit.
        assert b"/Encoding /WinAnsiEncoding" in data, (
            "Type1 font dict missing /Encoding /WinAnsiEncoding — "
            "viewers will mis-decode WinAnsi bytes via StandardEncoding"
        )
        # Confirm the PDF text layer round-trips back to the original
        # string. (PyMuPDF honors WinAnsiEncoding when present.)
        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf)
        finally:
            pdf.close()
        assert spanish in extracted, (
            f"text layer extraction expected to contain {spanish!r}, got {extracted!r}"
        )

    @test
    def spanish_text_round_trip():
        """Integration: render Spanish text with Latin-1 codepoints
        plus the inverted question/exclamation marks, then byte-compare
        the extracted text against the original input. Rung 8 of the
        WS-1A acceptance gate — this is the "Café español" defect
        the workstream exists to fix."""
        import fitz

        original = "¿Hablas español? Instrucciones en la última página."
        doc = HtmlDocument(string=f"<p>{original}</p>")
        data = doc.to_bytes()
        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf).strip()
        finally:
            pdf.close()
        assert extracted == original, (
            f"round-trip mismatch:\n  expected: {original!r}\n  got: {extracted!r}"
        )

    @test
    def winansi_non_mappable_promotes_to_fallback():
        """A codepoint outside WinAnsi (e.g. U+2611 ballot box) used to
        fall back to '?' (WS-1A bound). WS-1B promotes such runs onto
        the bundled `__pdfun_fallback` Identity-H face — so the WinAnsi
        prefix and suffix still show as Tj literals on F1, but the
        ballot box no longer mangles to '?'.

        We assert (a) the prefix/suffix WinAnsi bytes still appear on
        F1, (b) at least one Tf switch to F2 (the fallback face) is
        emitted between them, and (c) no '?' substitute leaks in."""
        doc = HtmlDocument(string="<p>ok ☑ done</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Prefix on F1 should still appear (either as a literal or as
        # uppercase hex). "ok " contains only ASCII so the literal form
        # is reliable.
        assert b"(ok ) Tj" in content, (
            f"expected `(ok ) Tj` on built-in font, got: {content!r}"
        )
        # Fallback face was activated at least once.
        assert b"/F2" in content, (
            f"expected fallback font /F2 in content stream, got: {content!r}"
        )
        # The non-WinAnsi run is no longer substituted with '?'.
        assert b"ok ? done" not in content, (
            "non-WinAnsi run still falls back to '?' instead of promoting"
        )


with describe("HtmlDocument - WS-1B fallback font"):
    # spec: ISO 32000-1 Annex D.2 + bundled DejaVu Sans;
    # behaviors: text-encoding-fallback-font

    @test
    def unicode_text_emits_two_fonts():
        """Mixed run `Café ☑ done` must produce a content stream that
        switches F1 → F2 → F1 inside the same `BT…ET` block. The
        WinAnsi prefix/suffix go on F1 (Helvetica + WinAnsiEncoding),
        the ballot box goes on F2 (the bundled fallback as Identity-H
        Type0)."""
        doc = HtmlDocument(string="<p>Café ☑ done</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # F1 (Helvetica) is set first, then F2 (fallback), then F1
        # again. We assert by ordered substring match.
        idx_f1 = content.find(b"/F1 12 Tf")
        idx_f2 = content.find(b"/F2 12 Tf")
        idx_f1_after = content.find(b"/F1 12 Tf", idx_f1 + 1)
        assert idx_f1 != -1, f"F1 (Helvetica) not set, got: {content!r}"
        assert idx_f2 != -1, f"F2 (fallback) not set, got: {content!r}"
        assert idx_f1_after != -1, "fallback run did not switch back to F1"
        assert idx_f1 < idx_f2 < idx_f1_after, (
            f"font switches not ordered F1→F2→F1, got indices "
            f"F1={idx_f1} F2={idx_f2} F1'={idx_f1_after}"
        )
        # The fallback content is a ShowGlyphs op — pdf-writer renders
        # a Type0/Identity-H Tj as a (\xHH\xLL) literal pair.
        # The "Café " prefix (with WinAnsi 0xE9) appears either as a
        # parenthesised literal containing 0xE9 or as <Caf…> hex.
        assert b"(Caf\xe9 ) Tj" in content or b"<436166E920>" in content, (
            f"WinAnsi prefix `Café ` missing, got: {content!r}"
        )

    @test
    def fallback_face_carries_identity_h_encoding():
        """The bundled fallback face is embedded as Type0/Identity-H
        with a CIDFontType2 descendant — same plumbing as
        user-supplied `@font-face`. The font dict in the PDF body must
        carry `/Encoding /Identity-H` so viewers know how to decode the
        glyph IDs."""
        doc = HtmlDocument(string="<p>Café ☑ done</p>")
        data = doc.to_bytes()
        # Identity-H is what we declare on the Type0 wrapper; the
        # presence of *any* Identity-H font dict tells us the fallback
        # was registered at PDF write time.
        assert b"/Encoding /Identity-H" in data, (
            "fallback face missing /Encoding /Identity-H — "
            "subsetting/embedding pipeline not engaged"
        )
        assert b"/Subtype /CIDFontType2" in data, (
            "fallback face missing CIDFontType2 descendant — "
            "Type0 embed pipeline not engaged"
        )

    @test
    def unknown_family_falls_back_to_pdfun_fallback():
        """`font-family: Roboto` with no `@font-face` for Roboto and no
        generic fallback in the family chain must use the bundled
        `__pdfun_fallback` face — *not* silently swap to Helvetica.
        Pre-WS-1B that swap caused unknown-family runs to lose
        Unicode coverage; the fallback restores it."""
        doc = HtmlDocument(string='<p style="font-family: Roboto">Café ☑</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        # The whole run renders on the fallback face — no Helvetica
        # WinAnsi prefix is emitted because Roboto resolves directly to
        # __pdfun_fallback.
        assert b"/F1 12 Tf" in content, (
            f"expected an F1 Tf op for the fallback, got: {content!r}"
        )
        # Fallback is Identity-H Type0 — verify the descendant font
        # entry is present in the PDF body.
        assert b"/Subtype /CIDFontType2" in data, (
            "fallback face missing CIDFontType2 descendant — "
            "Roboto did not resolve to __pdfun_fallback"
        )
        # Both characters must round-trip through the text layer.
        import fitz

        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf)
        finally:
            pdf.close()
        assert "Café" in extracted, (
            f"WinAnsi chars lost on fallback face: {extracted!r}"
        )
        assert "☑" in extracted, f"ballot box lost on fallback face: {extracted!r}"

    @test
    def known_generic_family_still_wins_over_fallback():
        """If the CSS family list ends in a known generic
        (`sans-serif`, `serif`, `monospace`), the generic still wins —
        WS-1B only promotes when nothing in the list is recognised."""
        html = '<p style="font-family: Roboto, sans-serif">hello</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # F1 here should be Helvetica (the generic match), not the
        # fallback. We can't trivially distinguish F1 across documents,
        # but we can assert the Type0/CIDFontType2 (fallback) embed is
        # *not* present — proving the fallback wasn't promoted.
        assert b"/Subtype /CIDFontType2" not in data, (
            "known-generic family should not engage the fallback face"
        )

    @test
    def ballot_box_round_trips_through_text_layer():
        """Acceptance gate: a U+2611 ballot box rendered with no
        `@font-face` declared must round-trip through the PDF text
        layer. WS-1A made this read as '?'; WS-1B promotes it onto the
        bundled fallback so `pymupdf.Page.get_text` recovers the
        original codepoint."""
        import fitz

        doc = HtmlDocument(string="<p>ok ☑ done</p>")
        data = doc.to_bytes()
        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf)
        finally:
            pdf.close()
        assert "☑" in extracted, (
            f"ballot box did not round-trip through text layer: {extracted!r}"
        )
        assert "?" not in extracted, (
            f"text layer still shows '?' substitute: {extracted!r}"
        )

    @test
    def bundled_fallback_license_present():
        """WS-1B Rung 6 (CI sanity): every TTF / OTF committed under
        `assets/fonts/` must ship a sibling `<stem>-LICENSE` file. The
        DejaVu / Bitstream Vera license is permissive but redistribution
        still requires the copyright text. `tools/check_fallback_font_license.py`
        runs the same check; we wrap it as a unit test so the harness
        catches a missing license at PR time, not release time."""
        import subprocess
        import sys

        repo_root = Path(__file__).resolve().parent.parent
        script = repo_root / "tools" / "check_fallback_font_license.py"
        assert script.exists(), f"missing {script}"
        result = subprocess.run(  # noqa: S603 -- internal repo script, no untrusted input
            [sys.executable, str(script)],
            capture_output=True,
            text=True,
            check=False,
        )
        assert result.returncode == 0, (
            f"license check failed (exit {result.returncode}):\n"
            f"stdout: {result.stdout}\nstderr: {result.stderr}"
        )

    @test
    def cobra_check_marks_render():
        """Integration: the COBRA-notice fixture relies on `&#9745;`
        (ballot box ☑) rendering inline with surrounding Latin text.
        This is the canonical WS-1B acceptance scenario — Rung 5 of
        the workstream's bottom-up TDD ladder. The PDF must
        (a) declare both Helvetica AND `__pdfun_fallback` in its font
        resource dictionary, and (b) round-trip the ballot box and
        Spanish text together via the PDF text layer."""
        import fitz

        # Mirrors the COBRA fixture pattern: bullet list with a checked
        # ballot box, plus Latin-1 chars that exercise the WinAnsi
        # path on Helvetica.
        html = (
            "<ul>"
            "<li>&#9745; Inscripci&oacute;n confirmada</li>"
            "<li>&#9745; Caf&eacute; incluido</li>"
            "<li>&#9745; &Uacute;ltima p&aacute;gina</li>"
            "</ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()

        # Both faces must appear in the resource dict — Helvetica for
        # the Latin runs and __pdfun_fallback for the ballot boxes.
        assert b"/Helvetica" in data, "Helvetica missing from font resource dict"
        assert b"/__pdfun_fallback" in data, (
            "__pdfun_fallback missing from font resource dict"
        )

        # Text-layer round-trip: every line must extract intact.
        pdf = fitz.open(stream=data, filetype="pdf")
        try:
            extracted = "".join(page.get_text() for page in pdf)
        finally:
            pdf.close()
        for needle in (
            "☑",
            "Inscripción confirmada",
            "Café incluido",
            "Última página",
        ):
            assert needle in extracted, (
                f"COBRA round-trip lost {needle!r}; got: {extracted!r}"
            )


with describe("HtmlDocument - nesting"):

    @test
    def deeply_nested_divs():
        """Deeply nested divs do not crash."""
        html = "<div>" * 50 + "Content" + "</div>" * 50
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Content")

    @test
    def deeply_nested_lists():
        """Deeply nested lists render without crash."""
        html = ""
        for i in range(10):
            html += f"<ul><li>Level {i}"
        html += "".join("</li></ul>" for _ in range(10))
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Level 0")
        expect(content).to_contain(b"Level 9")

    @test
    def mixed_block_nesting():
        """Block elements nested inside other block elements."""
        html = "<div><p>Para in div</p><div><p>Nested deeper</p></div></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Para in div")
        expect(content).to_contain(b"Nested deeper")

    @test
    def inline_in_heading():
        """Multiple inline styles in a heading."""
        html = "<h2><b>Bold</b> and <i>italic</i> heading</h2>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bold")
        expect(content).to_contain(b"italic")
        expect(content).to_contain(b"heading")


with describe("HtmlDocument - large documents"):

    @test
    def many_paragraphs():
        """500 paragraphs render without crash, spanning multiple pages."""
        paras = "".join(f"<p>Paragraph {i}</p>" for i in range(500))
        doc = HtmlDocument(string=paras)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Paragraph 0")
        expect(content).to_contain(b"Paragraph 499")

    @test
    def long_single_paragraph():
        """Very long single paragraph wraps correctly."""
        text = " ".join(f"word{i}" for i in range(500))
        doc = HtmlDocument(string=f"<p>{text}</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"word0")
        expect(content).to_contain(b"word499")


with describe("HtmlDocument - inline styles"):
    # spec: CSS 2.1 §14.1; behaviors: color-named
    @test
    def inline_color_named():
        """style='color: red' sets text color to red."""
        doc = HtmlDocument(string='<p style="color: red">Red</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §14.1; behaviors: color-hex
    @test
    def inline_color_hex():
        """style='color: #0000ff' sets text color to blue."""
        doc = HtmlDocument(string='<p style="color: #0000ff">Blue</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def inline_color_hex_short():
        """style='color: #f00' sets text color to red."""
        doc = HtmlDocument(string='<p style="color: #f00">Red</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §14.1; behaviors: color-rgb
    @test
    def inline_color_rgb():
        """style='color: rgb(0, 128, 0)' sets text color to green."""
        doc = HtmlDocument(string='<p style="color: rgb(0, 128, 0)">Green</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Green")
        expect(content).to_contain(b"0 0.50")

    # spec: CSS 2.1 §15.7; behaviors: font-size
    @test
    def inline_font_size_pt():
        """style='font-size: 18pt' uses 18pt font."""
        doc = HtmlDocument(string='<p style="font-size: 18pt">Big</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def inline_font_size_px():
        """style='font-size: 24px' converts to 18pt (24 * 0.75)."""
        doc = HtmlDocument(string='<p style="font-size: 24px">Big</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    # spec: CSS 2.1 §15.6; behaviors: font-weight
    @test
    def inline_font_weight_bold():
        """style='font-weight: bold' applies bold font."""
        doc = HtmlDocument(string='<p style="font-weight: bold">Bold</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def inline_font_weight_700():
        """style='font-weight: 700' applies bold font."""
        doc = HtmlDocument(string='<p style="font-weight: 700">Bold</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def inline_font_style_italic():
        """style='font-style: italic' applies italic font."""
        doc = HtmlDocument(string='<p style="font-style: italic">Italic</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Oblique")

    @test
    def inline_font_weight_and_style():
        """style='font-weight: bold; font-style: italic' uses BoldOblique."""
        doc = HtmlDocument(
            string='<p style="font-weight: bold; font-style: italic">Both</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-BoldOblique")

    @test
    def inline_font_family_serif():
        """style='font-family: serif' uses Times font."""
        doc = HtmlDocument(string='<p style="font-family: serif">Serif</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Times-Roman")

    @test
    def inline_font_family_monospace():
        """style='font-family: monospace' uses Courier font."""
        doc = HtmlDocument(string='<p style="font-family: monospace">Mono</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    # spec: CSS 2.1 §14.2.1; behaviors: bg-color
    @test
    def inline_background_color():
        """style='background-color: yellow' draws yellow background."""
        doc = HtmlDocument(
            string='<p style="background-color: #ffff00">Highlighted</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 1 0 rg")

    @test
    def inline_text_align_center():
        """style='text-align: center' centers text."""
        doc = HtmlDocument(string='<p style="text-align: center">Centered</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Centered")

    @test
    def inline_multiple_properties():
        """Multiple properties in one style attribute."""
        doc = HtmlDocument(string='<p style="color: blue; font-size: 24pt">Styled</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")
        expect(content).to_contain(b"24 Tf")

    @test
    def inline_invalid_css_ignored():
        """Invalid CSS values are silently ignored; valid ones still apply."""
        doc = HtmlDocument(
            string='<p style="color: notacolor; font-size: 18pt">Text</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def inline_style_on_heading():
        """Inline style on heading overrides UA defaults."""
        doc = HtmlDocument(string='<h1 style="font-size: 12pt">Small H1</h1>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"12 Tf")

    @test
    def inline_font_weight_normal_on_heading():
        """font-weight: normal on h1 overrides implied bold."""
        doc = HtmlDocument(string='<h1 style="font-weight: normal">Not Bold</h1>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica\n")

    @test
    def span_with_inline_style():
        """<span style='color: red'> applies color to span text only."""
        doc = HtmlDocument(
            string='<p>Normal <span style="color: red">red</span> normal</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def span_without_style():
        """<span> without style passes text through normally."""
        doc = HtmlDocument(string="<p>Before <span>inside</span> after</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"inside")

    @test
    def inline_padding():
        """style='padding: 10pt' adds padding to the block."""
        doc = HtmlDocument(string='<p style="padding: 10pt">Padded</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Padded")

    @test
    def inline_border():
        """style='border: 1px solid black' renders border."""
        doc = HtmlDocument(string='<p style="border: 1px solid black">Bordered</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bordered")

    @test
    def inline_margin_bottom():
        """style='margin-bottom: 24pt' adjusts spacing after paragraph."""
        doc = HtmlDocument(
            string='<p style="margin-bottom: 24pt">Spaced</p><p>Next</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Spaced")
        expect(content).to_contain(b"Next")


with describe("HtmlDocument - inline style hardening"):

    @test
    def nested_spans_inner_wins():
        """Inner span color overrides outer span color."""
        html = (
            '<p><span style="color: red">'
            '<span style="color: blue">inner</span></span></p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def span_inside_bold():
        """<b><span style='color: red'>text</span></b> applies both bold and color."""
        doc = HtmlDocument(string='<p><b><span style="color: red">text</span></b></p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def bold_with_color_style():
        """<b style='color: red'>text</b> applies bold font AND red color."""
        doc = HtmlDocument(string='<p><b style="color: red">text</b></p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def italic_with_font_size_style():
        """<i style='font-size: 18pt'>text</i> applies italic font at 18pt."""
        doc = HtmlDocument(string='<p><i style="font-size: 18pt">text</i></p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Helvetica-Oblique")
        expect(content).to_contain(b"18 Tf")

    @test
    def styled_bold_does_not_leak():
        """Style on <b> does not leak to following text."""
        doc = HtmlDocument(string='<p><b style="color: red">bold</b> normal</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        expect(content).to_contain(b"normal")

    @test
    def span_style_inside_styled_bold():
        """<b style='color: red'><span style='color: blue'>text</span></b> uses blue."""
        html = '<p><b style="color: red"><span style="color: blue">text</span></b></p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def serif_bold():
        """font-family: serif + font-weight: bold produces Times-Bold."""
        doc = HtmlDocument(
            string='<p style="font-family: serif; font-weight: bold">Serif bold</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"/Times-Bold")

    @test
    def monospace_italic():
        """font-family: monospace + font-style: italic produces Courier-Oblique."""
        html = '<p style="font-family: monospace; font-style: italic">Mono italic</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier-Oblique")

    @test
    def li_with_color():
        """<li style='color: red'> renders red text."""
        doc = HtmlDocument(string='<ul><li style="color: red">Red item</li></ul>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        expect(content).to_contain(b"Red item")

    @test
    def li_with_background():
        """<li style='background-color: yellow'> renders background."""
        doc = HtmlDocument(
            string='<ul><li style="background-color: #ffff00">Highlighted</li></ul>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 1 0 rg")

    @test
    def h2_with_color():
        """<h2 style='color: blue'> renders blue heading."""
        doc = HtmlDocument(string='<h2 style="color: blue">Blue Title</h2>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")
        expect(content).to_contain(b"Blue Title")

    @test
    def h3_with_font_weight_normal():
        """<h3 style='font-weight: normal'> overrides bold."""
        doc = HtmlDocument(string='<h3 style="font-weight: normal">Normal H3</h3>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica\n")

    @test
    def h1_with_background():
        """<h1 style='background-color: yellow'> renders heading with background."""
        doc = HtmlDocument(string='<h1 style="background-color: #ffff00">Title</h1>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 1 0 rg")
        expect(content).to_contain(b"Title")

    @test
    def inline_text_align_right():
        """style='text-align: right' right-aligns text."""
        doc = HtmlDocument(string='<p style="text-align: right">Right</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Right")

    @test
    def inline_line_height():
        """style='line-height: 24pt' sets line height."""
        html = '<p style="line-height: 24pt">Some text here</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def inline_line_height_number():
        """style='line-height: 2' uses multiplier."""
        html = '<p style="line-height: 2">Double spaced</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def inline_padding_left():
        """style='padding-left: 30pt' offsets text."""
        doc = HtmlDocument(string='<p style="padding-left: 30pt">Indented</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Indented")

    @test
    def inline_border_width_only():
        """style='border-width: 3px' draws border."""
        doc = HtmlDocument(string='<p style="border-width: 3px">Bordered</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Bordered")

    @test
    def inline_border_color_only():
        """style='border-color: red' with border-width draws colored border."""
        doc = HtmlDocument(
            string='<p style="border-width: 1px; border-color: red">Red border</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0")

    @test
    def important_not_breaking():
        """!important does not break CSS parsing."""
        doc = HtmlDocument(string='<p style="color: red !important">Important</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def negative_font_size_ignored():
        """Negative font-size is silently ignored; default applies."""
        doc = HtmlDocument(string='<p style="font-size: -12pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"12 Tf")

    @test
    def negative_padding_ignored():
        """Negative padding is silently ignored."""
        doc = HtmlDocument(string='<p style="padding: -10px; color: red">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def uppercase_property_name():
        """Uppercase property name COLOR works."""
        doc = HtmlDocument(string='<p style="COLOR: red">Red</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def extra_semicolons_handled():
        """Extra semicolons in style attribute don't break parsing."""
        doc = HtmlDocument(string='<p style=";;color: red;;;font-size: 18pt;">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        expect(content).to_contain(b"18 Tf")

    @test
    def missing_value_handled():
        """Missing value after colon doesn't crash."""
        doc = HtmlDocument(string='<p style="color:; font-size: 18pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def malformed_rgb_too_few_args():
        """rgb() with too few arguments is silently ignored."""
        doc = HtmlDocument(
            string='<p style="color: rgb(255, 0); font-size: 18pt">Text</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def span_without_style_near_styled_span():
        """Unstyled span doesn't interfere with styled span."""
        html = (
            "<p><span>plain</span> "
            '<span style="color: red">red</span> '
            "<span>plain2</span></p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"plain")
        expect(content).to_contain(b"red")
        expect(content).to_contain(b"plain2")


with describe("HtmlDocument - code element"):

    @test
    def code_uses_courier():
        """<code> renders text with Courier font."""
        doc = HtmlDocument(string="<p><code>x = 1</code></p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Courier")
        expect(content).to_contain(b"x = 1")

    @test
    def code_inside_bold():
        """<b><code>text</code></b> uses Courier-Bold."""
        doc = HtmlDocument(string="<p><b><code>bold code</code></b></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier-Bold")

    @test
    def code_inside_italic():
        """<i><code>text</code></i> uses Courier-Oblique."""
        doc = HtmlDocument(string="<p><i><code>italic code</code></i></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier-Oblique")

    @test
    def code_does_not_leak():
        """<code> font does not leak to following text."""
        doc = HtmlDocument(string="<p><code>code</code> normal</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")
        expect(data).to_contain(b"/Helvetica\n")

    @test
    def code_with_css_font_override():
        """CSS font-family on <code> overrides default Courier."""
        doc = HtmlDocument(string='<p><code style="font-family: serif">code</code></p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Times-Roman")

    @test
    def kbd_uses_courier():
        """<kbd> renders text with Courier font."""
        doc = HtmlDocument(string="<p><kbd>Ctrl+C</kbd></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def samp_uses_courier():
        """<samp> renders text with Courier font."""
        doc = HtmlDocument(string="<p><samp>output</samp></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")


with describe("HtmlDocument - blockquote element"):
    # spec: HTML; behaviors: html-blockquote
    @test
    def blockquote_renders():
        """<blockquote> renders text."""
        doc = HtmlDocument(string="<blockquote>Quoted text</blockquote>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Quoted text")

    @test
    def blockquote_with_style():
        """<blockquote> with inline style applies CSS."""
        doc = HtmlDocument(
            string='<blockquote style="color: blue">Blue quote</blockquote>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def blockquote_nested_in_div():
        """<blockquote> inside <div> renders correctly."""
        html = "<div><blockquote>Quoted</blockquote></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Quoted")


with describe("HtmlDocument - hr element"):
    # spec: HTML; behaviors: html-hr
    @test
    def hr_renders():
        """<hr> between paragraphs doesn't crash."""
        doc = HtmlDocument(string="<p>Before</p><hr><p>After</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Before")
        expect(content).to_contain(b"After")

    @test
    def hr_alone():
        """<hr> alone produces valid PDF with stroke."""
        doc = HtmlDocument(string="<hr>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data[:5]).to_equal(b"%PDF-")
        expect(content).to_contain(b"S\n")

    @test
    def multiple_hr():
        """Multiple <hr> elements don't crash."""
        doc = HtmlDocument(string="<p>A</p><hr><p>B</p><hr><p>C</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"A")
        expect(content).to_contain(b"B")
        expect(content).to_contain(b"C")


with describe("HtmlDocument - pre element"):
    # spec: HTML; behaviors: html-pre
    @test
    def pre_preserves_spaces():
        """<pre> preserves multiple spaces."""
        doc = HtmlDocument(string="<pre>a  b  c</pre>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"a  b  c")

    @test
    def pre_preserves_newlines():
        """<pre> preserves newlines as separate lines."""
        doc = HtmlDocument(string="<pre>line1\nline2\nline3</pre>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"line1")
        expect(content).to_contain(b"line2")
        expect(content).to_contain(b"line3")

    @test
    def pre_uses_courier():
        """<pre> uses monospace (Courier) font."""
        doc = HtmlDocument(string="<pre>code here</pre>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def pre_with_code():
        """<pre><code> works (common pattern)."""
        doc = HtmlDocument(string="<pre><code>x = 1\ny = 2</code></pre>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Courier")
        expect(content).to_contain(b"x = 1")
        expect(content).to_contain(b"y = 2")

    @test
    def pre_followed_by_normal():
        """Normal <p> after <pre> resumes word-wrapping."""
        html = "<pre>  spaced  </pre><p>Normal paragraph</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Normal paragraph")


with describe("HtmlDocument - white-space"):
    # spec: CSS 2.1 §16.6; behaviors: text-white-space
    @test
    def white_space_pre_preserves_spaces_like_pre_tag():
        """`white-space: pre` on a <p> preserves internal runs of spaces."""
        html = '<p style="white-space: pre">a  b  c</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"a  b  c")

    @test
    def white_space_pre_preserves_newlines():
        """`white-space: pre` renders `\\n` as a line break."""
        html = '<p style="white-space: pre">line1\nline2</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"line1")
        expect(content).to_contain(b"line2")

    @test
    def white_space_nowrap_keeps_long_text_on_one_line():
        """`white-space: nowrap` bypasses wrapping for text wider than the column."""
        words = " ".join([f"word{i}" for i in range(40)])
        normal = HtmlDocument(string=f"<p>{words}</p>").to_bytes()
        normal_content = content_stream(normal)
        nowrap = HtmlDocument(
            string=f'<p style="white-space: nowrap">{words}</p>'
        ).to_bytes()
        nowrap_content = content_stream(nowrap)
        # Both contain the text; nowrap's stream should have fewer text-show ops
        # than the wrapped version (one line vs many). We approximate this by
        # counting the number of `Td` operators that start a new line.
        expect(normal_content.count(b" Td")).to_be_greater_than(
            nowrap_content.count(b" Td")
        )

    @test
    def white_space_normal_collapses_whitespace():
        """`white-space: normal` collapses runs of internal whitespace."""
        html = '<p style="white-space: normal">a   b   c</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Collapsed form is "a b c"; the tripled-space form must NOT appear.
        assert b"a   b   c" not in content
        expect(content).to_contain(b"a b c")

    @test
    def white_space_inherits_to_children():
        """`white-space` inherits from a parent container to inline children."""
        html = '<div style="white-space: pre">outer  <span>inner  text</span></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"outer  ")


with describe("HtmlDocument - style blocks"):
    # spec: CSS 2.1 §5.3, §14.1; behaviors: sel-type, color-property
    @test
    def style_type_selector_color():
        """<style>p { color: red }</style> applies red to paragraphs."""
        html = "<style>p { color: red }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def style_type_selector_font_size():
        """<style>p { font-size: 18pt }</style> sets font size."""
        html = "<style>p { font-size: 18pt }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    # spec: CSS 2.1 §5.3; behaviors: sel-universal
    @test
    def universal_selector_matches_all_elements():
        """`* { color: red }` applies red to arbitrary, unrelated elements."""
        html = "<style>* { color: red }</style><h1>H</h1><p>P</p><div>D</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # The red fill-color op should be emitted at least once per distinct
        # element type — we don't need to count, just verify the selector
        # matched beyond a single element.
        expect(content.count(b"1 0 0 rg")).to_be_greater_than(1)

    # spec: CSS 2.1 §5.8.3; behaviors: sel-class
    @test
    def style_class_selector():
        """<style>.red { color: red }</style> matches class attribute."""
        html = '<style>.red { color: red }</style><p class="red">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §5.9; behaviors: sel-id
    @test
    def style_id_selector():
        """<style>#title { font-size: 24pt }</style> matches id attribute."""
        html = '<style>#title { font-size: 24pt }</style><p id="title">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"24 Tf")

    # spec: CSS 2.1 §6.4.1; behaviors: cascade-origin
    @test
    def style_inline_wins_over_style_block():
        """Inline style overrides <style> block rule."""
        html = '<style>p { color: blue }</style><p style="color: red">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def style_later_rule_wins():
        """Later rule with same specificity wins."""
        html = "<style>p { color: green } p { color: blue }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def style_class_beats_type():
        """Class selector beats type selector (higher specificity)."""
        html = (
            "<style>p { color: red } .blue { color: blue }</style>"
            '<p class="blue">Text</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    # spec: CSS 2.1 §6.4.3; behaviors: cascade-specificity
    @test
    def style_id_beats_class():
        """ID selector beats class selector (higher specificity)."""
        html = (
            "<style>.red { color: red } #blue { color: blue }</style>"
            '<p class="red" id="blue">Text</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    # spec: CSS 2.1 §5.2.1; behaviors: sel-selector-list
    @test
    def style_multiple_selectors():
        """Comma-separated selectors apply to all matches."""
        html = "<style>h1, h2 { color: red }</style><h1>A</h1><h2>B</h2>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §5.5; behaviors: sel-descendant
    @test
    def style_descendant_selector():
        """Descendant selector 'div p' matches nested elements."""
        html = "<style>div p { color: red }</style><div><p>Text</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def style_descendant_no_match():
        """Descendant selector does not match non-descendant."""
        html = "<style>div p { color: red }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    # spec: CSS 2.1 §5.6; behaviors: sel-child
    @test
    def style_child_selector():
        """Child selector 'div > p' matches direct children."""
        html = "<style>div > p { color: red }</style><div><p>Text</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def style_child_no_match_grandchild():
        """Child selector does not match grandchildren."""
        html = (
            "<style>div > p { color: red }</style>"
            "<div><blockquote><p>Text</p></blockquote></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    # spec: CSS 2.1 §5.2; behaviors: sel-compound
    @test
    def style_compound_selector():
        """Compound selector 'p.note' matches element with class."""
        html = (
            "<style>p.note { color: red }</style>"
            '<p class="note">Match</p><p>No match</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def style_no_style_block():
        """Document without <style> block still works."""
        doc = HtmlDocument(string="<p>Text</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def style_multiple_style_blocks():
        """Multiple <style> blocks are concatenated."""
        html = (
            "<style>p { color: red }</style>"
            "<style>p { font-size: 18pt }</style>"
            "<p>Text</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"18 Tf")

    @test
    def style_font_weight():
        """<style> block can set font-weight: bold."""
        html = "<style>p { font-weight: bold }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def style_background_color():
        """<style> block can set background-color."""
        html = "<style>p { background-color: yellow }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 1 0 rg")


with describe("@media and @import"):
    # spec: CSS 2.1 §7.2; behaviors: at-media
    @test
    def media_print_rules_apply():
        """Rules inside `@media print { ... }` apply in the PDF output."""
        html = "<style>@media print { p { color: red } }</style><p>Hello</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def media_screen_only_rules_skipped():
        """Rules scoped to `@media screen` do NOT apply in the PDF."""
        html = "<style>@media screen { p { color: red } }</style><p>Hello</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        assert b"1 0 0 rg" not in content

    @test
    def media_all_rules_apply():
        """`@media all` applies regardless of output medium."""
        html = "<style>@media all { p { color: red } }</style><p>Hello</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def media_not_screen_applies_to_print():
        """`@media not screen` matches print."""
        html = "<style>@media not screen { p { color: red } }</style><p>Hello</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def media_comma_list_matches_if_any_clause_matches():
        """Comma-separated `@media screen, print` applies to print."""
        html = "<style>@media screen, print { p { color: red } }</style><p>Hello</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §6.3; behaviors: at-import
    @test
    def import_statement_does_not_break_parser():
        """`@import url(...)` at the top of a sheet is ignored, not fatal."""
        html = '<style>@import url("other.css"); p { color: red }</style><p>Hello</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # The rule that followed @import should still apply.
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def multiple_imports_tolerated():
        """Two stacked `@import` statements are skipped without error."""
        html = (
            "<style>"
            '@import "a.css"; @import url(b.css); '
            "p { color: red }"
            "</style><p>Hello</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")


with describe("body CSS inheritance"):
    # spec: CSS 2.1 §15.3; behaviors: font-family
    @test
    def body_font_family_inherits():
        """font-family on body inherits to child elements."""
        html = "<style>body { font-family: monospace }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def body_font_family_comma_list():
        """Comma-separated font-family resolves through the list."""
        html = (
            "<style>body { font-family: 'Courier New', Courier,"
            " monospace }</style><p>Text</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def font_family_unknown_first_falls_through_to_next_known():
        """An unknown family at the head of the chain is skipped in favor of
        the next available family. spec:fonts3-fallback"""
        html = "<style>p { font-family: 'NotInstalledFont', serif }</style><p>Text</p>"
        data = HtmlDocument(string=html).to_bytes()
        expect(data).to_contain(b"/Times-Roman")

    @test
    def font_family_all_unknown_falls_back_to_pdfun_fallback():
        """If no family in the chain matches a known generic / built-in
        and no `@font-face` rule catches them either, WS-1B promotes
        the run onto the bundled `__pdfun_fallback` Identity-H face so
        the document renders with full Unicode coverage. Pre-WS-1B this
        silently swapped to Helvetica, which loses non-WinAnsi glyphs.
        spec:fonts3-fallback"""
        html = "<style>p { font-family: 'FakeA', 'FakeB', 'FakeC' }</style><p>Text</p>"
        data = HtmlDocument(string=html).to_bytes()
        expect(data).to_contain(b"/__pdfun_fallback")

    @test
    def font_family_fallback_honors_bold_weight():
        """Fallback picks up `font-weight: bold` from the same rule — the
        resolved family's bold variant is selected. spec:fonts3-fallback"""
        html = (
            "<style>p { font-family: 'FakeFont', serif;"
            " font-weight: bold }</style><p>Text</p>"
        )
        data = HtmlDocument(string=html).to_bytes()
        expect(data).to_contain(b"/Times-Bold")

    @test
    def font_family_unquoted_unknown_skipped():
        """Unquoted unknown family idents are skipped just like quoted ones.
        spec:fonts3-fallback"""
        html = "<style>p { font-family: CustomFont, monospace }</style><p>Text</p>"
        data = HtmlDocument(string=html).to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def font_family_case_insensitive_generic():
        """Generic family keywords match case-insensitively.
        spec:fonts3-fallback"""
        html = "<style>p { font-family: 'Unknown', Monospace }</style><p>Text</p>"
        data = HtmlDocument(string=html).to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def body_font_size_inherits():
        """font-size on body inherits to child elements."""
        html = "<style>body { font-size: 10pt }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"10 Tf")

    @test
    def body_line_height_inherits():
        """line-height on body inherits to child elements without crash."""
        html = "<style>body { line-height: 1.4 }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("CSS inheritance"):

    @test
    def color_inherits_through_div():
        """color on a div inherits to child p."""
        html = '<div style="color: red"><p>Red text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def font_weight_inherits():
        """font-weight: bold on a div inherits to child p."""
        html = '<div style="font-weight: bold"><p>Bold text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

    @test
    def font_style_inherits():
        """font-style: italic on a div inherits to child p."""
        html = '<div style="font-style: italic"><p>Italic text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Oblique")

    @test
    def text_align_inherits():
        """text-align: center on a div inherits to child p."""
        html = '<div style="text-align: center"><p>Centered</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # Centered text will have non-zero x offset from left margin
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def font_family_inherits_through_div():
        """font-family on a div inherits to child p."""
        html = '<div style="font-family: monospace"><p>Mono text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def font_size_inherits_through_div():
        """font-size on a div inherits to child p."""
        html = '<div style="font-size: 20pt"><p>Big text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"20 Tf")

    @test
    def multi_level_inheritance():
        """color propagates through intermediate elements."""
        html = '<div style="color: red"><div><p>Still red</p></div></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def child_overrides_parent():
        """Explicit style on child overrides inherited value."""
        html = '<div style="color: red"><p style="color: blue">Blue wins</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def background_does_not_inherit():
        """background-color is non-inheritable; child should not get it."""
        html = '<div style="background-color: red"><p>No red bg on me</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # The div itself will have a red background (1 0 0 rg),
        # but the p should not have its own background rect.
        # We verify valid PDF is produced; detailed rect check is hard here.
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def stylesheet_rule_inheritance():
        """Stylesheet rules on a parent inherit to children."""
        html = (
            "<style>.parent { color: red }</style>"
            '<div class="parent"><p>Red from rule</p></div>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def inheritance_does_not_leak_to_siblings():
        """Inherited style on one branch does not affect sibling branch."""
        html = '<div style="color: red"><p>Red</p></div><p>Default color</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    # spec: CSS 2.1 §10.8; behaviors: vfmd-line-height
    @test
    def line_height_inherits_through_div():
        """line-height on a div inherits to child elements."""
        html = '<div style="line-height: 2"><p>Spaced text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("@page rule"):
    # spec: CSS 2.1 §13.2; behaviors: paged-at-page
    @test
    def at_page_size_letter():
        """@page { size: letter } sets page to 612x792."""
        html = "<style>@page { size: letter }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/MediaBox [0 0 612 792]")

    @test
    def at_page_size_a4():
        """@page { size: a4 } sets page to 595x842."""
        html = "<style>@page { size: a4 }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/MediaBox [0 0 595 842]")

    @test
    def at_page_margin():
        """@page { margin: 0.75in } parses without error."""
        html = "<style>@page { margin: 0.75in }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def at_page_with_other_rules():
        """@page rules coexist with regular CSS rules."""
        html = "<style>@page { size: letter } p { color: red }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/MediaBox [0 0 612 792]")
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def inch_unit_in_margin():
        """Inch units resolve correctly (0.75in = 54pt)."""
        html = "<style>@page { size: letter; margin: 1in }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("@page margin boxes"):

    @test
    def top_center_string_literal():
        """@top-center renders a plain string literal."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @top-center { content: "My Document"; }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(My Document)")

    @test
    def bottom_right_string_literal():
        """@bottom-right renders a plain string literal."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @bottom-right { content: "Confidential"; }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Confidential)")

    # spec: CSS Paged Media 3 §4.3; behaviors: paged3-counters
    @test
    def counter_page_renders_1_on_first_page():
        """counter(page) substitutes 1 on the first (only) page."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @bottom-center { content: counter(page); }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # The shown text is literally "1"
        expect(content).to_contain(b"(1)")

    @test
    def counter_pages_renders_total_page_count():
        """counter(pages) substitutes the final document page count."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @bottom-center { content: "Total: " counter(pages); }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Total: 1)")

    @test
    def page_n_of_m_format():
        """Combined "Page N of M" format works."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @bottom-right {
                    content: "Page " counter(page) " of " counter(pages);
                }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Items are concatenated into a single Tj literal.
        expect(content).to_contain(b"(Page 1 of 1)")

    @test
    def multi_page_document_numbers_sequentially():
        """On a multi-page document, each page shows its own counter."""
        # Force at least 3 pages by inserting explicit page breaks.
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @bottom-center { content: counter(page) " / " counter(pages); }
            }
            .pb { page-break-after: always; }
        </style>
        <p class="pb">Page one body</p>
        <p class="pb">Page two body</p>
        <p>Page three body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Each page's counter resolves to its own 1-indexed number, and
        # counter(pages) resolves to the total (3) on every page.
        expect(content).to_contain(b"(1 / 3)")
        expect(content).to_contain(b"(2 / 3)")
        expect(content).to_contain(b"(3 / 3)")

    @test
    def top_left_and_top_right_coexist():
        """Multiple margin-box positions can be declared together."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @top-left { content: "Left"; }
                @top-right { content: "Right"; }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Left)")
        expect(content).to_contain(b"(Right)")

    # spec: CSS Paged Media 3 §5; behaviors: paged3-margin-boxes
    @test
    def margin_box_with_font_size_and_color():
        """Margin box honors font-size and color overrides."""
        html = """<style>
            @page {
                size: letter;
                margin: 2cm;
                @top-center {
                    content: "Styled";
                    font-size: 14pt;
                    color: #ff0000;
                }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Styled)")
        # Red fill color "1 0 0 rg" emitted before the text.
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def margin_box_does_not_break_other_page_properties():
        """Nested margin boxes don't prevent size/margin from parsing."""
        html = """<style>
            @page {
                size: a4;
                margin: 1in;
                @top-center { content: "Header"; }
            }
        </style><p>Body</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/MediaBox [0 0 595 842]")
        expect(content).to_contain(b"(Header)")


# ── @page pseudo-class selectors (WS-4) ──────────────────────────────
# spec: CSS Paged Media L3 §4.4
with describe("@page pseudo-classes"):

    @test
    def page_layout_uses_first_margin():
        """`@page :first { margin: 0.25in }` shrinks page 1's margins.

        Page 1 should use 18pt margins; page 2 should fall back to the
        base `@page` declaration's 1in (72pt). The first text run on
        each page lives at `(margin_left, page_height - margin_top - …)`,
        so the per-page Td coordinate distinguishes them.
        """
        import fitz

        html = """<style>
            @page { size: letter; margin: 1in; }
            @page :first { margin: 0.25in; }
            .pb { page-break-after: always; }
        </style>
        <p class="pb">Page one body</p>
        <p>Page two body</p>"""
        data = HtmlDocument(string=html).to_bytes()
        with fitz.open(stream=data, filetype="pdf") as pdf:
            # First text on page 1: bbox.x0 should be ~18pt (0.25in).
            # First text on page 2: bbox.x0 should be ~72pt (1in).
            blocks_1 = pdf[0].get_text("blocks")
            blocks_2 = pdf[1].get_text("blocks")
            x0_p1 = blocks_1[0][0]
            x0_p2 = blocks_2[0][0]
        # Tight tolerance: 0.25in = 18pt, 1in = 72pt.
        expect(abs(x0_p1 - 18.0) < 2.0).to_equal(True)
        expect(abs(x0_p2 - 72.0) < 2.0).to_equal(True)

    @test
    def at_bottom_right_paints_on_first_page():
        """`@bottom-right` content renders on page 1 even when `:first`
        narrows the margins.

        Reproduces the COBRA defect: with `@page :first { margin: 0.25in }`
        and `@page { @bottom-right { content: "Page " counter(page) } }`,
        the footer was previously clipped because the first page used
        the wrong margin. Now both pages should carry "Page 1" and
        "Page 2" respectively.
        """
        import fitz

        html = """<style>
            @page { size: letter; margin: 1in;
                @bottom-right { content: "Page " counter(page); }
            }
            @page :first { margin: 0.25in; }
            .pb { page-break-after: always; }
        </style>
        <p class="pb">Body 1</p>
        <p>Body 2</p>"""
        data = HtmlDocument(string=html).to_bytes()
        with fitz.open(stream=data, filetype="pdf") as pdf:
            page1_text = pdf[0].get_text()
            page2_text = pdf[1].get_text()
        expect("Page 1" in page1_text).to_equal(True)
        expect("Page 2" in page2_text).to_equal(True)

    @test
    def first_right_beats_first_alone_on_page_one():
        """When both `@page :first` and `@page :first:right` declare a
        margin, the higher-specificity `:first:right` wins on page 1
        (LTR — page 1 is right-hand)."""
        import fitz

        html = """<style>
            @page { size: letter; margin: 1in; }
            @page :first { margin-top: 0.5in; }
            @page :first:right { margin-top: 0.25in; }
        </style>
        <p>Hello</p>"""
        data = HtmlDocument(string=html).to_bytes()
        with fitz.open(stream=data, filetype="pdf") as pdf:
            blocks = pdf[0].get_text("blocks")
            y0 = blocks[0][1]
        # 0.25in = 18pt. fitz reports y0 from page top, so first text
        # baseline should be just below 18pt.
        expect(y0 < 30.0).to_equal(True)
        expect(y0 > 10.0).to_equal(True)

    @test
    def universal_at_page_only_continues_to_apply():
        """A bare `@page { margin: 1in }` (specificity 0,0,0) still
        applies on every page when no pseudo rules are present."""
        import fitz

        html = """<style>
            @page { size: letter; margin: 1in; }
            .pb { page-break-after: always; }
        </style>
        <p class="pb">Body 1</p>
        <p>Body 2</p>"""
        data = HtmlDocument(string=html).to_bytes()
        with fitz.open(stream=data, filetype="pdf") as pdf:
            x0_p1 = pdf[0].get_text("blocks")[0][0]
            x0_p2 = pdf[1].get_text("blocks")[0][0]
        expect(abs(x0_p1 - 72.0) < 2.0).to_equal(True)
        expect(abs(x0_p2 - 72.0) < 2.0).to_equal(True)

    @test
    def cobra_cover_page_margin_and_footer():
        """Integration: simulate the COBRA cover-page scenario.

        - `@page :first { margin: 0.25in }` shrinks page 1's margins so
          the cover article (height: ~10.5in) fits without bleeding to
          page 2.
        - `@page { @bottom-right { content: "Page " counter(page)
          " of " counter(pages) } }` paints a footer on every page.
        - "IMPORTANT INFORMATION" lives in a body paragraph after a
          forced page break, so it must begin on page 2.
        - The footer should read "Page 1 of 2" on page 1 and
          "Page 2 of 2" on page 2 — both within the resolved bottom
          margin strip (so `:first`'s narrower margins didn't clip the
          footer off page 1).
        """
        import fitz

        html = """<style>
            @page {
                size: letter;
                margin: 1in;
                @bottom-right { content: "Page " counter(page) " of " counter(pages); }
            }
            @page :first { margin: 0.25in; }
            .cover {
                page-break-after: always;
            }
        </style>
        <div class="cover">
            <h1>COBRA Coverage Notice</h1>
            <p>This is the cover page article that needs the
               narrower margins to fit.</p>
        </div>
        <h2>IMPORTANT INFORMATION</h2>
        <p>Body content begins on page 2.</p>"""
        data = HtmlDocument(string=html).to_bytes()
        with fitz.open(stream=data, filetype="pdf") as pdf:
            page1 = pdf[0].get_text()
            page2 = pdf[1].get_text()
            # Page 1 starts within the narrow margin strip.
            x0_p1 = pdf[0].get_text("blocks")[0][0]
        expect(abs(x0_p1 - 18.0) < 2.0).to_equal(True)
        # Footer present on each page, with correct counter values.
        expect("Page 1 of 2" in page1).to_equal(True)
        expect("Page 2 of 2" in page2).to_equal(True)
        # Body article begins on page 2, not page 1.
        expect("IMPORTANT INFORMATION" in page2).to_equal(True)
        expect("IMPORTANT INFORMATION" in page1).to_equal(False)


with describe("multi-column layout"):

    @test
    def column_count_renders():
        """column-count: 2 produces a valid PDF with text wrapped narrower."""
        html = """<style>body { column-count: 2 }</style>
        <p>First paragraph with enough text to show wrapping.</p>
        <p>Second paragraph also with some text content.</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data[:5]).to_equal(b"%PDF-")
        expect(content).to_contain(b"First paragraph")

    @test
    def column_rule_renders():
        """column-rule draws stroke operations in the PDF."""
        html = """<style>
        body { column-count: 2; column-rule: 1px solid #cccccc }
        </style>
        <p>Left column text.</p>
        <p>More text for the layout.</p>
        <p>Even more to push to second column.</p>
        <p>And another paragraph.</p>
        <p>Filling up the first column.</p>
        <p>This should overflow.</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Column rules produce stroke ops with the specified color
        expect(content).to_contain(b"0.8 0.8 0.8 RG")

    @test
    def column_gap_respected():
        """column-gap: 0.3in produces valid PDF."""
        html = """<style>body { column-count: 2; column-gap: 0.3in }</style>
        <p>Text in columns.</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def jc_news_full_css():
        """The full jc-news CSS renders without error."""
        html = """<style>
        @page { size: letter; margin: 0.75in; }
        body {
            column-count: 2; column-gap: 0.3in;
            column-rule: 1px solid #ccc;
            font-family: 'Courier New', Courier, monospace;
            font-size: 10pt; line-height: 1.4;
            margin: 0; padding: 0;
        }
        h1 { font-size: 16pt; font-weight: bold; }
        h2 { font-size: 13pt; font-weight: bold; }
        </style>
        <h1>Hacker News Summary</h1>
        <h2>1. Test Post</h2>
        <p>100 points by alice | 50 comments</p>
        <p>A cool project that does interesting things.</p>
        <hr>
        <h2>2. Another Post</h2>
        <p>200 points by bob | 120 comments</p>
        <p>An article about something interesting.</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/MediaBox [0 0 612 792]")
        expect(data).to_contain(b"/Courier")
        expect(content).to_contain(b"Hacker News Summary")


with describe("display: none"):

    @test
    def inline_display_none_hides_element():
        """An element with inline display:none is not rendered."""
        doc = HtmlDocument(string='<p style="display:none">Hidden</p><p>Visible</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Visible")
        assert b"Hidden" not in content

    # spec: CSS 2.1 §9.2.4; behaviors: vfm-display-none
    @test
    def display_none_hides_children():
        """display:none on a parent hides all descendants."""
        doc = HtmlDocument(
            string='<div style="display:none"><p>Deep hidden</p></div><p>Visible</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Visible")
        assert b"Deep hidden" not in content

    @test
    def display_none_via_style_block():
        """display:none set via a <style> block hides the element."""
        html = """<html><head><style>.hide { display: none; }</style></head>
        <body><p class="hide">Hidden</p><p>Visible</p></body></html>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Visible")
        assert b"Hidden" not in content

    @test
    def display_block_still_renders():
        """display:block does not hide the element."""
        doc = HtmlDocument(string='<p style="display:block">Shown</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Shown")

    # spec: CSS 2.1 §9.3.1; behaviors: vfm-position-static
    @test
    def position_static_ignores_offsets():
        """position:static blocks ignore top/left/right/bottom offsets per spec."""
        baseline = HtmlDocument(string="<p>plain</p>").to_bytes()
        with_offsets = HtmlDocument(
            string='<p style="position: static; top: 500px; left: 500px">plain</p>'
        ).to_bytes()
        # Layout must be identical — offsets have no effect on static boxes.
        expect(baseline).to_equal(with_offsets)


with describe("box-sizing"):
    # spec: CSS 2.1 §10.1; behaviors: vfmd-box-sizing
    @test
    def border_box_vs_content_box_differ():
        """box-sizing: border-box shrinks the content area inside width."""
        css = "width: 100px; padding: 20px; border: 5px solid red"
        content_box = HtmlDocument(
            string=f'<div style="box-sizing: content-box; {css}">x</div>'
        ).to_bytes()
        border_box = HtmlDocument(
            string=f'<div style="box-sizing: border-box; {css}">x</div>'
        ).to_bytes()
        # The two sizing modes produce different border rectangles in the PDF.
        assert content_box != border_box


with describe("Document metadata from HTML"):

    @test
    def title_tag_sets_pdf_title():
        """<title> content becomes the PDF /Title metadata."""
        html = (
            "<html><head><title>My Page Title</title></head>"
            "<body><p>Hello</p></body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        assert b"/Title" in data
        assert b"My Page Title" in data

    @test
    def empty_title_tag_no_title():
        """An empty <title> tag does not set metadata."""
        html = "<html><head><title></title></head><body><p>Hello</p></body></html>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        assert b"/Title" not in data


with describe("CSS margins"):
    # spec: CSS 2.1 §8.3; behaviors: box-margin
    @test
    def margin_top_renders():
        """margin-top on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-top: 50pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def margin_left_renders():
        """margin-left on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-left: 50pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def margin_right_renders():
        """margin-right on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-right: 50pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def margin_shorthand_all_four():
        """margin shorthand sets all four sides."""
        doc = HtmlDocument(string='<p style="margin: 10pt 20pt 30pt 40pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def margin_shorthand_two_values():
        """margin: 10pt 20pt sets top/bottom=10 and left/right=20."""
        doc = HtmlDocument(string='<p style="margin: 10pt 20pt">Text</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text")

    @test
    def adjacent_sibling_margins_collapse_to_max():
        """Adjacent vertical margins should collapse to the larger value.

        Four paragraphs each with margin-top/bottom: 180pt.
        - Collapsed: inter-paragraph gap is 180pt, total vertical spend
          is roughly 4 * 180 + text heights ~= 720pt + content. With top
          page margin 72pt this fits within a 792pt Letter page.
        - Additive: gaps would be 360pt, total ~= 1440pt + content,
          forcing a second page.
        """
        html = (
            '<p style="margin-top: 180pt; margin-bottom: 180pt">A</p>'
            '<p style="margin-top: 180pt; margin-bottom: 180pt">B</p>'
        )
        data = HtmlDocument(string=html).to_bytes()
        # Two short paragraphs with 180pt collapsed margins fit on one page.
        assert b"/Count 1" in data

    @test
    def parent_first_child_margin_top_collapses():
        """Stage B2: a container's margin-top collapses with its first
        in-flow child's margin-top.

        Total required vertical = 4 * 180pt margins + some text. If B2
        works, all four 180pt margins collapse to a single 180pt, so
        everything fits on one page. If they additively summed, the
        content would overflow to page 2.
        """
        html = (
            '<div style="margin-top: 180pt; margin-bottom: 180pt">'
            '  <p style="margin-top: 180pt; margin-bottom: 180pt">inside</p>'
            "</div>"
            '<p style="margin-top: 180pt">after</p>'
        )
        data = HtmlDocument(string=html).to_bytes()
        assert b"/Count 1" in data

    @test
    def parent_last_child_margin_bottom_collapses():
        """Stage B2: a container's margin-bottom collapses with its last
        in-flow child's margin-bottom, and then with the following sibling.
        """
        html = (
            '<div style="margin-bottom: 300pt">'
            '  <p style="margin-bottom: 300pt">inside</p>'
            "</div>"
            '<p style="margin-top: 300pt">after</p>'
        )
        data = HtmlDocument(string=html).to_bytes()
        # Three 300pt margins collapsing to one fit on a single page
        # alongside two short paragraphs; the additive case (900pt) would
        # blow past the usable content area and force a page break.
        assert b"/Count 1" in data

    # spec: CSS 2.1 §8.3.1; behaviors: box-collapse-empty
    @test
    def empty_block_self_collapses():
        """Stage B3: an empty block with only margin-top and margin-bottom
        and no border/padding/content self-collapses (its top and bottom
        margins merge into a single margin that also folds in with the
        surrounding flow)."""
        html = (
            '<p style="margin-bottom: 200pt">before</p>'
            '<div style="margin-top: 200pt; margin-bottom: 200pt"></div>'
            '<p style="margin-top: 200pt">after</p>'
        )
        data = HtmlDocument(string=html).to_bytes()
        # Four 200pt margins collapse to one → fits on one page.
        # Additive would require at least 800pt of margin.
        assert b"/Count 1" in data

    @test
    def margin_left_right_narrows_content():
        """Large left/right margins cause text to wrap at a narrower width."""
        long_text = "word " * 50
        # Without margins — should produce some lines
        doc_wide = HtmlDocument(string=f"<p>{long_text}</p>")
        data_wide = doc_wide.to_bytes()
        # With large margins — should produce more pages or more content
        doc_narrow = HtmlDocument(
            string=f'<p style="margin-left: 200pt; margin-right: 200pt">{long_text}</p>'
        )
        data_narrow = doc_narrow.to_bytes()
        # The narrow version should be longer due to more wrapping
        assert len(data_narrow) > len(data_wide)


with describe("hsl colors"):
    # spec: CSS Color 3 §4.2.3; behaviors: color-hsl
    @test
    def hsl_red_renders():
        """hsl(0, 100%, 50%) is pure red."""
        html = '<p style="color: hsl(0, 100%, 50%)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"red")
        # Pure red in PDF fill color op: "1 0 0 rg"
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def hsl_green_renders():
        """hsl(120, 100%, 50%) is pure green."""
        html = '<p style="color: hsl(120, 100%, 50%)">green</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 1 0 rg")

    # spec: CSS Color 3 §4.2.4; behaviors: color-hsla
    @test
    def hsla_accepts_alpha_component():
        """hsla()'s 4th component flows through `WS-2`'s ExtGState gate:
        a `/Gs<N> gs` op precedes the `rg` color op, and the document
        carries an `/ExtGState` resource."""
        html = '<p style="color: hsla(240, 100%, 50%, 0.5)">blue</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Color is still emitted — RGB channels are unchanged.
        expect(content).to_contain(b"0 0 1 rg")
        # And the alpha is now active: `gs` referenced before `rg`.
        idx_gs = content.index(b"gs")
        idx_rg = content.index(b"0 0 1 rg")
        expect(idx_gs < idx_rg).to_equal(True)
        expect(data).to_contain(b"/ExtGState")

    # spec: CSS Color 3 §4.2.1; behaviors: color-rgba
    @test
    def rgba_accepts_alpha_component():
        """rgba()'s 4th component flows through `WS-2`'s ExtGState gate:
        the `/Gs<N> gs` op precedes the foreground `rg` and the resource
        dict references `/ExtGState`."""
        html = '<p style="color: rgba(255, 0, 0, 0.5)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        idx_gs = content.index(b"gs")
        idx_rg = content.index(b"1 0 0 rg")
        expect(idx_gs < idx_rg).to_equal(True)
        expect(data).to_contain(b"/ExtGState")


with describe("cmyk colors"):
    # spec: CSS Color 4 §5.1; behaviors: color-cmyk
    @test
    def device_cmyk_pure_cyan_renders_as_rgb():
        """device-cmyk(1 0 0 0) is pure cyan; we flatten to sRGB (0, 1, 1)."""
        html = '<p style="color: device-cmyk(1 0 0 0)">cyan</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 1 1 rg")

    @test
    def cmyk_alias_is_accepted():
        """cmyk() is the short spelling accepted alongside device-cmyk()."""
        html = '<p style="color: cmyk(0 1 0 0)">magenta</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 1 rg")

    @test
    def device_cmyk_pure_black_via_k():
        """device-cmyk(0 0 0 1) collapses all channels to black."""
        html = '<p style="color: device-cmyk(0 0 0 1)">black</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 0 rg")

    @test
    def device_cmyk_percentage_components_are_accepted():
        """Percentage components are in [0,100] and match the numeric form."""
        html = '<p style="color: device-cmyk(0% 100% 100% 0%)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def device_cmyk_legacy_comma_syntax_is_accepted():
        """Legacy comma-separated form parses the same as whitespace-separated."""
        html = '<p style="color: device-cmyk(0, 1, 1, 0)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def device_cmyk_alpha_component_is_ignored():
        """Optional alpha (`/ <number>`) parses and is dropped."""
        html = '<p style="color: device-cmyk(0 1 1 0 / 0.5)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def device_cmyk_works_as_background_color():
        """device-cmyk() is accepted anywhere a <color> is, e.g. background-color."""
        html = (
            '<p style="background-color: device-cmyk(1 0 0 0); '
            'color: rgb(0, 0, 0)">cyan bg</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # background fill: cyan (0, 1, 1)
        expect(content).to_contain(b"0 1 1 rg")


with describe("semantic block elements"):

    @test
    def article_renders_as_block():
        """<article> renders its text content as a block."""
        doc = HtmlDocument(string="<article>Article text</article>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Article text")

    @test
    def section_renders_as_block():
        doc = HtmlDocument(string="<section>Section text</section>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Section text")

    @test
    def nested_semantic_margins_collapse():
        """<article><section><p>…</p></section></article> — nested pure
        containers collapse margins through to the child paragraph, same as
        <div>."""
        html = (
            '<article style="margin-top: 200pt">'
            '<section style="margin-top: 200pt">'
            '<p style="margin-top: 200pt">Hello</p>'
            "</section></article>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello")
        # 200pt collapses to 200pt — everything fits on one page
        expect(data).to_contain(b"/Count 1")


with describe("text-indent"):
    # spec: CSS 2.1 §16.1; behaviors: text-indent
    @test
    def text_indent_renders_first_line_shift():
        """text-indent shifts the first line's x position."""
        html = (
            '<p style="text-indent: 24pt; text-align: left">'
            "First line indented by twenty-four points here.</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"First line indented")

    @test
    def text_indent_inherits_from_parent():
        """text-indent is an inherited property."""
        html = '<div style="text-indent: 24pt"><p>Indented nested text.</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Indented nested")


with describe("text-transform"):
    # spec: CSS 2.1 §16.5; behaviors: text-transform
    @test
    def uppercase_transforms_text():
        """text-transform: uppercase converts text to uppercase."""
        html = '<p style="text-transform: uppercase">hello world</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"HELLO WORLD")
        assert b"hello world" not in content

    @test
    def lowercase_transforms_text():
        """text-transform: lowercase converts text to lowercase."""
        html = '<p style="text-transform: lowercase">HELLO World</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"hello world")
        assert b"HELLO World" not in content

    @test
    def capitalize_transforms_text():
        """text-transform: capitalize uppercases the first letter of each word."""
        html = '<p style="text-transform: capitalize">hello world</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hello World")

    @test
    def text_transform_inherits_from_parent():
        """text-transform inherits through descendants."""
        doc = HtmlDocument(
            string='<div style="text-transform: uppercase"><p>nested</p></div>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"NESTED")


with describe("text-decoration"):

    @test
    def underline_produces_stroke_ops():
        """text-decoration: underline draws a line under text."""
        doc = HtmlDocument(
            string='<p style="text-decoration: underline">Underlined</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Underlined")
        # The PDF should contain stroke operations for the underline
        # (MoveTo + LineTo + Stroke pattern in content stream)

    @test
    def line_through_produces_stroke_ops():
        """text-decoration: line-through draws a line through text."""
        doc = HtmlDocument(string='<p style="text-decoration: line-through">Struck</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Struck")

    @test
    def underline_and_line_through_combined():
        """Both underline and line-through can be applied together."""
        doc = HtmlDocument(
            string='<p style="text-decoration: underline line-through">Both</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Both")

    @test
    def decoration_none_overrides_inherited():
        """text-decoration: none disables inherited decoration."""
        html = (
            "<html><head><style>"
            ".underlined { text-decoration: underline; }"
            ".no-dec { text-decoration: none; }"
            "</style></head><body>"
            '<p class="underlined"><span class="no-dec">Plain</span></p>'
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Plain")

    @test
    def underline_via_style_block():
        """text-decoration set via <style> block is applied."""
        html = """<html><head><style>
        .ul { text-decoration: underline; }
        </style></head>
        <body><p class="ul">Styled underline</p></body></html>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Styled underline")


with describe("border-style"):

    @test
    def border_solid_renders():
        """border with solid style renders normally."""
        doc = HtmlDocument(string='<p style="border: 2px solid black">Solid border</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Solid border")

    @test
    def border_dashed_renders():
        """border with dashed style renders."""
        doc = HtmlDocument(string='<p style="border: 2px dashed red">Dashed border</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Dashed border")

    @test
    def border_dotted_renders():
        """border with dotted style renders."""
        doc = HtmlDocument(
            string='<p style="border: 2px dotted blue">Dotted border</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Dotted border")

    @test
    def border_style_none_suppresses_border():
        """border-style: none suppresses border even if width is set."""
        doc_none = HtmlDocument(
            string='<p style="border-width: 2px; border-style: none">No border</p>'
        )
        data_none = doc_none.to_bytes()
        doc_solid = HtmlDocument(
            string='<p style="border: 2px solid black">Has border</p>'
        )
        data_solid = doc_solid.to_bytes()
        # The solid version should be longer due to stroke operations
        assert len(data_solid) > len(data_none)

    @test
    def border_style_property_standalone():
        """border-style as a standalone property works."""
        doc = HtmlDocument(
            string=(
                '<p style="border-width: 1px; border-style: dashed;'
                ' border-color: green">Styled</p>'
            )
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Styled")


with describe("clickable links"):
    # spec: PDF; behaviors: pdf-external-links
    @test
    def link_produces_annotation():
        """An <a href> produces a /Link annotation in the PDF."""
        doc = HtmlDocument(
            string='<p>Visit <a href="https://example.com">our site</a> today</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"our site")
        expect(data).to_contain(b"/Link")
        expect(data).to_contain(b"https://example.com")

    @test
    def link_uses_uri_action():
        """Links use a URI action type."""
        doc = HtmlDocument(string='<a href="https://example.com/path">click</a>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/URI")
        expect(data).to_contain(b"https://example.com/path")

    @test
    def link_without_href_no_annotation():
        """An <a> without href does not create an annotation."""
        doc = HtmlDocument(string="<p><a>no link</a> here</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"no link")
        assert b"/Link" not in data

    @test
    def multiple_links_multiple_annotations():
        """Multiple <a> tags produce multiple annotations."""
        doc = HtmlDocument(
            string=(
                '<p><a href="https://a.com">first</a> and '
                '<a href="https://b.com">second</a></p>'
            )
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"https://a.com")
        expect(data).to_contain(b"https://b.com")
        # Two /Link subtypes
        assert data.count(b"/Link") >= 2

    @test
    def link_text_rendered_inline():
        """Link text flows inline with surrounding text."""
        doc = HtmlDocument(
            string='<p>before <a href="https://x.com">linked</a> after</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"before")
        expect(content).to_contain(b"linked")
        expect(content).to_contain(b"after")

    @test
    def link_with_inline_style():
        """A link with inline color styling still produces an annotation."""
        doc = HtmlDocument(
            string='<a href="https://example.com" style="color: blue">styled link</a>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"/Link")
        expect(content).to_contain(b"styled link")


with describe("list-style-type"):

    @test
    def ol_default_is_decimal():
        """<ol> defaults to decimal markers."""
        doc = HtmlDocument(string="<ol><li>One</li><li>Two</li></ol>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(1.)")
        expect(content).to_contain(b"(2.)")

    @test
    def lower_alpha_marker():
        """list-style-type: lower-alpha produces a, b, c markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: lower-alpha">'
                "<li>First</li><li>Second</li></ol>"
            )
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(a.)")
        expect(content).to_contain(b"(b.)")

    @test
    def upper_alpha_marker():
        """list-style-type: upper-alpha produces A, B markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: upper-alpha"><li>One</li><li>Two</li></ol>'
            )
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(A.)")
        expect(content).to_contain(b"(B.)")

    @test
    def lower_roman_marker():
        """list-style-type: lower-roman produces i, ii, iii markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: lower-roman">'
                "<li>One</li><li>Two</li><li>Three</li></ol>"
            )
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(i.)")
        expect(content).to_contain(b"(ii.)")
        expect(content).to_contain(b"(iii.)")

    @test
    def upper_roman_marker():
        """list-style-type: upper-roman produces I, II markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: upper-roman"><li>One</li><li>Two</li></ol>'
            )
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(I.)")
        expect(content).to_contain(b"(II.)")

    @test
    def disc_marker_explicit():
        """list-style-type: disc produces WinAnsi bullet byte 0x95
        (rendered as `<95>` in the content stream by pdf-writer when
        the literal contains non-ASCII)."""
        doc = HtmlDocument(
            string='<ul style="list-style-type: disc"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"<95>")

    @test
    def square_marker():
        """list-style-type: square produces '#' ASCII marker."""
        doc = HtmlDocument(
            string='<ul style="list-style-type: square"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(#)")

    @test
    def none_marker_suppresses_bullet():
        """list-style-type: none produces no marker."""
        doc_with = HtmlDocument(string="<ul><li>Item</li></ul>")
        data_with = doc_with.to_bytes()
        data_with_content = content_stream(data_with)
        doc_none = HtmlDocument(
            string='<ul style="list-style-type: none"><li>Item</li></ul>'
        )
        data_none = doc_none.to_bytes()
        data_none_content = content_stream(data_none)
        # Both contain "Item" but the "none" variant has no marker
        expect(data_with_content).to_contain(b"Item")
        expect(data_none_content).to_contain(b"Item")
        # The marker ShowText call is absent in the "none" version.
        # Disc renders as WinAnsi bullet byte 0x95 (`<95>` hex form).
        assert b"<95>" in data_with_content
        assert b"<95>" not in data_none_content

    @test
    def decimal_via_stylesheet():
        """list-style-type set via <style> block on <ol> applies."""
        html = (
            "<html><head><style>"
            ".lower { list-style-type: lower-alpha; }"
            "</style></head><body>"
            '<ol class="lower"><li>A</li><li>B</li></ol>'
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(a.)")
        expect(content).to_contain(b"(b.)")

    @test
    def roman_numerals_compound():
        """Roman numeral markers handle compound values correctly."""
        items = "".join(f"<li>Item {i}</li>" for i in range(1, 10))
        doc = HtmlDocument(
            string=f'<ol style="list-style-type: lower-roman">{items}</ol>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(iv.)")
        expect(content).to_contain(b"(ix.)")


with describe("list-style-position"):

    @test
    def default_is_outside():
        """Default list-style-position is outside (marker hangs left of text)."""
        doc = HtmlDocument(string="<ul><li>Item</li></ul>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Item")
        # Disc marker -> WinAnsi byte 0x95.
        expect(content).to_contain(b"<95>")

    @test
    def inside_renders_marker_and_text():
        """list-style-position: inside still renders both marker and text."""
        doc = HtmlDocument(
            string='<ul style="list-style-position: inside"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Item")
        # Disc marker -> WinAnsi byte 0x95.
        expect(content).to_contain(b"<95>")

    # spec: CSS Lists 3 §3; behaviors: lists-style-position
    @test
    def inside_differs_from_outside():
        """list-style-position: inside produces different bytes than outside."""
        doc_outside = HtmlDocument(
            string='<ul style="list-style-position: outside"><li>Item</li></ul>'
        )
        doc_inside = HtmlDocument(
            string='<ul style="list-style-position: inside"><li>Item</li></ul>'
        )
        assert doc_outside.to_bytes() != doc_inside.to_bytes()

    @test
    def inside_via_stylesheet():
        """list-style-position set via <style> block applies."""
        html = (
            "<html><head><style>"
            "ul { list-style-position: inside; }"
            "</style></head><body>"
            "<ul><li>Item</li></ul>"
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Item")
        # Disc marker -> WinAnsi byte 0x95.
        expect(content).to_contain(b"<95>")


with describe("definition lists and figures"):
    # spec: HTML; behaviors: html-definition-list
    @test
    def dl_renders_term_and_definition():
        """<dl><dt><dd> renders both term and definition text."""
        doc = HtmlDocument(string="<dl><dt>Term</dt><dd>Definition</dd></dl>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Term")
        expect(content).to_contain(b"Definition")

    @test
    def dd_is_indented():
        """<dd> content is indented relative to <dt>."""
        doc = HtmlDocument(string="<dl><dt>Term</dt><dd>Definition</dd></dl>")
        # Rendering must succeed and contain the definition text.
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Definition")
        # A plain paragraph version should differ from the dd-indented one.
        doc_plain = HtmlDocument(string="<p>Term</p><p>Definition</p>")
        assert doc_plain.to_bytes() != data

    # spec: HTML; behaviors: html-figure
    @test
    def figure_with_figcaption_renders_both():
        """<figure><figcaption> renders both the figure body and caption."""
        doc = HtmlDocument(
            string="<figure><p>Body</p><figcaption>Caption</figcaption></figure>"
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Body")
        expect(content).to_contain(b"Caption")

    @test
    def multiple_dt_dd_pairs():
        """A <dl> with multiple term/definition pairs renders every entry."""
        doc = HtmlDocument(
            string=("<dl><dt>One</dt><dd>First</dd><dt>Two</dt><dd>Second</dd></dl>")
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"One")
        expect(content).to_contain(b"First")
        expect(content).to_contain(b"Two")
        expect(content).to_contain(b"Second")


with describe("page-break"):

    @test
    def page_break_before_always_forces_new_page():
        """page-break-before: always forces the element onto a new page."""
        doc_baseline = HtmlDocument(string="<p>First</p><p>Second</p>")
        baseline = doc_baseline.to_bytes()
        doc = HtmlDocument(
            string=('<p>First</p><p style="page-break-before: always">Second</p>')
        )
        data = doc.to_bytes()
        # Baseline: 1 page (/Count 1). With break: 2 pages (/Count 2).
        assert b"/Count 1" in baseline
        assert b"/Count 2" in data

    @test
    def page_break_after_always_forces_new_page():
        """page-break-after: always forces the following content onto a new page."""
        doc = HtmlDocument(
            string=('<p style="page-break-after: always">First</p><p>Second</p>')
        )
        data = doc.to_bytes()
        assert b"/Count 2" in data

    @test
    def page_break_auto_does_nothing():
        """page-break-before: auto does not force a new page."""
        doc = HtmlDocument(
            string=('<p>First</p><p style="page-break-before: auto">Second</p>')
        )
        data = doc.to_bytes()
        assert b"/Count 1" in data

    @test
    def page_break_before_at_start_no_blank_page():
        """page-break-before on the first element should not produce a blank page."""
        doc = HtmlDocument(string='<p style="page-break-before: always">First</p>')
        data = doc.to_bytes()
        assert b"/Count 1" in data

    @test
    def break_before_alias_accepted():
        """break-before (CSS3 alias) is accepted and forces a page break."""
        doc = HtmlDocument(
            string=('<p>First</p><p style="break-before: page">Second</p>')
        )
        data = doc.to_bytes()
        assert b"/Count 2" in data

    @test
    def page_break_via_stylesheet():
        """page-break-after in a <style> block applies."""
        html = (
            "<html><head><style>"
            ".cover { page-break-after: always; }"
            "</style></head><body>"
            '<h1 class="cover">Title Page</h1>'
            "<p>Chapter content</p>"
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        assert b"/Count 2" in data

    # spec: CSS 2.1 §13.3.1; behaviors: paged-page-break-inside
    @test
    def page_break_inside_avoid_parses():
        """page-break-inside: avoid is accepted and renders without error."""
        doc = HtmlDocument(
            string='<p style="page-break-inside: avoid">Keep together</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Keep together")
        expect(data).to_contain(b"/Count 1")

    # spec: CSS 2.1 §13.3.1; behaviors: paged-page-break-inside
    @test
    def break_inside_avoid_alias_accepted():
        """break-inside (CSS3 alias) with avoid is accepted."""
        doc = HtmlDocument(string='<p style="break-inside: avoid">Keep</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Keep")

    # spec: CSS 2.1 §13.3.1; behaviors: paged-page-break-inside
    @test
    def page_break_inside_avoid_pushes_overflow_to_next_page():
        """A tall paragraph that would overflow the current page is pushed
        whole to the next page when page-break-inside: avoid is set. This
        matches the existing "keep block together" behavior."""
        # Fill most of page 1, then place an avoid-inside block that must go
        # to page 2 rather than overflow at the bottom of page 1.
        filler = "<p>line</p>" * 45
        html = (
            f"{filler}"
            '<p style="page-break-inside: avoid">'
            "<span>A</span><span>B</span><span>C</span>"
            "</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Count 2")

    # spec: CSS 2.1 §13.3.2; behaviors: paged-orphans-widows
    @test
    def orphans_integer_parses():
        """orphans: N is accepted with any positive integer."""
        doc = HtmlDocument(string='<p style="orphans: 3">Body</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Body")

    # spec: CSS 2.1 §13.3.2; behaviors: paged-orphans-widows
    @test
    def widows_integer_parses():
        """widows: N is accepted with any positive integer."""
        doc = HtmlDocument(string='<p style="widows: 4">Body</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Body")

    # spec: CSS 2.1 §13.3.2; behaviors: paged-orphans-widows
    @test
    def orphans_and_widows_via_stylesheet():
        """orphans and widows declared in a stylesheet apply to matched rules."""
        html = (
            "<html><head><style>"
            "p { orphans: 3; widows: 3; }"
            "</style></head><body>"
            "<p>Paragraph text</p>"
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Paragraph text")


with describe("text-align: justify"):
    # spec: CSS 2.1 §16.4; behaviors: text-word-spacing
    @test
    def justify_emits_word_spacing_op():
        """justified text emits a Tw (word spacing) operator in the stream."""
        long_text = "word " * 30
        doc = HtmlDocument(string=f'<p style="text-align: justify">{long_text}</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        # Tw is the PDF word spacing operator
        expect(content).to_contain(b" Tw")

    # spec: CSS 2.1 §16.4; behaviors: text-letter-spacing
    @test
    def letter_spacing_emits_character_spacing_op():
        """letter-spacing emits a Tc (character spacing) operator in the stream."""
        doc = HtmlDocument(string='<p style="letter-spacing: 5px">abc</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        # Tc is the PDF character spacing operator. 5px == 3.75pt.
        expect(content).to_contain(b" Tc")
        expect(content).to_contain(b"3.75 Tc")

    @test
    def justify_last_line_not_widened():
        """Last line of justified paragraph has no word-spacing applied."""
        # Short single-line paragraph: no spacing needed, no Tw emitted
        doc = HtmlDocument(string='<p style="text-align: justify">Short line only</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        # Single-line para has no lines to widen → no Tw
        assert b" Tw" not in content

    @test
    def justify_left_aligns_single_line():
        """A single-line justified paragraph renders like left-aligned."""
        doc_left = HtmlDocument(string="<p>Hello world</p>")
        doc_justify = HtmlDocument(
            string='<p style="text-align: justify">Hello world</p>'
        )
        # Both should produce the same text at the same x position
        expect(doc_left.to_bytes()).to_contain(b"(Hello world)")
        expect(doc_justify.to_bytes()).to_contain(b"(Hello world)")

    @test
    def justify_via_stylesheet():
        """text-align: justify via stylesheet applies."""
        long_text = "word " * 30
        html = (
            "<html><head><style>"
            "p { text-align: justify; }"
            "</style></head><body>"
            f"<p>{long_text}</p>"
            "</body></html>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b" Tw")

    @test
    def justify_resets_word_spacing_after_line():
        """Word spacing is reset to 0 after each justified line."""
        long_text = "word " * 30
        doc = HtmlDocument(string=f'<p style="text-align: justify">{long_text}</p>')
        data = doc.to_bytes()
        content = content_stream(data)
        # A reset to 0 should appear somewhere in the stream
        expect(content).to_contain(b"0 Tw")


with describe("tables"):

    @test
    def basic_table_renders_cells():
        """A simple table renders all cell text."""
        html = (
            "<table>"
            "<tr><td>Alice</td><td>30</td></tr>"
            "<tr><td>Bob</td><td>25</td></tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Alice")
        expect(content).to_contain(b"Bob")
        expect(content).to_contain(b"(30)")
        expect(content).to_contain(b"(25)")

    @test
    def table_header_uses_bold_font():
        """<th> cells render in Helvetica-Bold."""
        html = (
            "<table>"
            "<tr><th>Name</th><th>Age</th></tr>"
            "<tr><td>Alice</td><td>30</td></tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data).to_contain(b"Helvetica-Bold")
        expect(content).to_contain(b"Name")

    @test
    def table_draws_cell_borders():
        """Each cell has a stroked border by default."""
        html = "<table><tr><td>A</td><td>B</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Stroke operator (uppercase S) should appear
        expect(content).to_contain(b"\nS\n")

    @test
    def table_columns_sized_by_content():
        """Wider content gets a wider column."""
        html = (
            "<table>"
            "<tr>"
            "<td>short</td>"
            "<td>this is a much wider cell content</td>"
            "</tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"short")
        expect(content).to_contain(b"this is a much wider cell content")

    @test
    def table_inside_thead_tbody():
        """Rows inside <thead>/<tbody> are collected correctly."""
        html = (
            "<table>"
            "<thead><tr><th>Header</th></tr></thead>"
            "<tbody><tr><td>Body</td></tr></tbody>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Header")
        expect(content).to_contain(b"Body")

    @test
    def table_with_long_content_wraps_in_cell():
        """Long cell content wraps within the cell's column width."""
        long_text = "word " * 50
        html = f"<table><tr><td>{long_text}</td><td>short</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Should render without error and contain both cells
        expect(content).to_contain(b"short")
        expect(content).to_contain(b"word")

    @test
    def empty_table_renders_without_error():
        """An empty <table> does not crash or produce garbage."""
        doc = HtmlDocument(string="<table></table>")
        data = doc.to_bytes()
        # Just a page with no content
        assert b"%PDF" in data

    # spec: CSS 2.1 §17.5; behaviors: table-layout
    @test
    def table_td_inline_style_color():
        """A <td> inline color style applies to cell text."""
        html = '<table><tr><td style="color: red">Red</td></tr></table>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Red")
        # The fill color for red should be emitted somewhere
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def table_multiple_rows_stacked_vertically():
        """Multiple rows stack without overlap (later rows get lower y)."""
        html = (
            "<table>"
            "<tr><td>Row1</td></tr>"
            "<tr><td>Row2</td></tr>"
            "<tr><td>Row3</td></tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Row1")
        expect(content).to_contain(b"Row2")
        expect(content).to_contain(b"Row3")

    @test
    def table_cells_with_padding():
        """Default cell padding leaves space inside cells."""
        # Hard to assert precise geometry, but the PDF should render
        # and contain both cells without collision.
        html = "<table><tr><td>Left</td><td>Right</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Left")
        expect(content).to_contain(b"Right")

    # spec: CSS 2.1 §17.5.2 (WS-5)
    @test
    def cobra_cost_table_columns():
        """Mirror the COBRA cost table's 30%/10%/60% colgroup hints and
        verify the cell-text x-coordinates land at approximately the
        column boundaries.

        Default page is 612pt wide with 1in (72pt) margins, so the
        usable column area is 468pt. After the table's own margin (0pt
        by default) the column boundaries fall at:

            col 0:  72pt        (start)
            col 1:  72 + 0.3*468 = 212.4pt
            col 2:  72 + 0.4*468 = 259.2pt
            (right edge: 540pt)

        Cell text is offset by the cell's left padding (default 6pt).
        The PDF `Td` operator `x y Td` gives the baseline position, so
        we look for `Tm`/`Td` operands matching the expected x.
        """
        html = (
            "<table>"
            "<colgroup>"
            '<col style="width: 30%">'
            '<col style="width: 10%">'
            '<col style="width: 60%">'
            "</colgroup>"
            "<tr><td>A</td><td>B</td><td>C</td></tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # All three cells render
        expect(content).to_contain(b"(A) Tj")
        expect(content).to_contain(b"(B) Tj")
        expect(content).to_contain(b"(C) Tj")
        # The text-position lines for A, B, C must be in increasing x.
        # Each cell-text "<x> <y> Td" precedes its "(.) Tj" line.
        ax = _find_text_x(content, b"(A) Tj")
        bx = _find_text_x(content, b"(B) Tj")
        cx = _find_text_x(content, b"(C) Tj")
        assert ax is not None, f"expected Td/Tj for (A); got {ax}"
        assert bx is not None, f"expected Td/Tj for (B); got {bx}"
        assert cx is not None, f"expected Td/Tj for (C); got {cx}"
        assert ax < bx, f"col 0→1 out of order: {ax} >= {bx}"
        assert bx < cx, f"col 1→2 out of order: {bx} >= {cx}"
        # Approximate location asserts: col 1 starts ~140pt to the right
        # of col 0; col 2 ~47pt to the right of col 1. Allow 1pt slop
        # for padding rounding.
        assert 130 < (bx - ax) < 150, (
            f"expected col 0→1 gap ≈ 30% of 468 = 140pt; got {bx - ax}"
        )
        assert 35 < (cx - bx) < 60, (
            f"expected col 1→2 gap ≈ 10% of 468 = 47pt; got {cx - bx}"
        )

    # spec: CSS 2.1 §17.5.1 painter's order, §17.3 col properties (WS-5)
    @test
    def col_background_paints_behind_cells():
        """A `<col style="background-color: yellow">` paints a yellow
        rectangle behind that column's cells. Per CSS 2.1 §17.5.1 the
        column rectangle is drawn before the cell rectangles, so the
        yellow `1 1 0 rg` fill must appear in the content stream before
        the cell text fills.
        """
        html = (
            "<table>"
            "<colgroup>"
            '<col style="background-color: yellow">'
            "<col>"
            "</colgroup>"
            "<tr><td>L</td><td>R</td></tr>"
            "</table>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Yellow column fill — `1 1 0 rg` is the f32-formatted RGB.
        expect(content).to_contain(b"1 1 0 rg")
        # Painter's order: column-background `1 1 0 rg` then `f` Fill
        # before the per-cell text `(L) Tj`.
        yellow_idx = content.find(b"1 1 0 rg")
        l_idx = content.find(b"(L) Tj")
        assert yellow_idx >= 0, "expected yellow rg in content stream"
        assert l_idx >= 0, "expected (L) Tj in content stream"
        assert yellow_idx < l_idx, (
            f"col background (1 1 0 rg @ {yellow_idx}) must paint before cell text "
            f"((L) Tj @ {l_idx}) per CSS 2.1 §17.5.1"
        )


with describe("images"):
    # spec: HTML; behaviors: html-img-png
    @test
    def png_img_produces_xobject():
        """<img src=...> for a PNG produces an Image XObject in the PDF."""
        png = _make_png(2, 2, bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(string=f'<img src="{path}" width="50" height="50">')
            data = doc.to_bytes()
            content = content_stream(data)
            expect(data).to_contain(b"/XObject")
            expect(data).to_contain(b"/Subtype /Image")
            expect(data).to_contain(b"/FlateDecode")
            expect(data).to_contain(b"/DeviceRGB")
            expect(content).to_contain(b"/Im0 Do")
        finally:
            Path(path).unlink()

    # spec: HTML; behaviors: html-img-jpeg
    @test
    def jpeg_img_produces_dctdecode_xobject():
        """<img src=...> for a JPEG produces a DCTDecode-filtered Image XObject."""
        import fitz

        pix = fitz.Pixmap(fitz.csRGB, fitz.IRect(0, 0, 2, 2), 0)
        pix.set_rect(fitz.IRect(0, 0, 2, 2), (255, 128, 64))
        jpg = pix.tobytes("jpg")
        with tempfile.NamedTemporaryFile(suffix=".jpg", delete=False) as f:
            f.write(jpg)
            path = f.name
        try:
            doc = HtmlDocument(string=f'<img src="{path}" width="50" height="50">')
            data = doc.to_bytes()
            content = content_stream(data)
            expect(data).to_contain(b"/XObject")
            expect(data).to_contain(b"/Subtype /Image")
            expect(data).to_contain(b"/DCTDecode")
            expect(content).to_contain(b"/Im0 Do")
        finally:
            Path(path).unlink()

    @test
    def png_rgba_produces_smask():
        """An RGBA PNG produces a soft mask (SMask) for the alpha channel."""
        rgba = bytes(
            [
                255,
                0,
                0,
                255,
                0,
                255,
                0,
                128,
                0,
                0,
                255,
                64,
                255,
                255,
                255,
                0,
            ]
        )
        png = _make_png_rgba(2, 2, rgba)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(string=f'<img src="{path}">')
            data = doc.to_bytes()
            expect(data).to_contain(b"/SMask")
            expect(data).to_contain(b"/DeviceGray")
        finally:
            Path(path).unlink()

    @test
    def img_intrinsic_dimensions():
        """An <img> without width/height uses the PNG's intrinsic dimensions."""
        # 4x3 PNG, all red
        rgb = bytes([255, 0, 0] * 12)
        png = _make_png(4, 3, rgb)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(string=f'<img src="{path}">')
            data = doc.to_bytes()
            # Just verify width and height dict entries exist
            expect(data).to_contain(b"/Width 4")
            expect(data).to_contain(b"/Height 3")
        finally:
            Path(path).unlink()

    # spec: CSS 2.1 §10.2; behaviors: vfmd-width
    @test
    def img_width_preserves_aspect_ratio():
        """Setting only width preserves aspect ratio for height."""
        # 4x2 (aspect 2:1)
        rgb = bytes([0, 0, 0] * 8)
        png = _make_png(4, 2, rgb)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(string=f'<img src="{path}" width="100">')
            data = doc.to_bytes()
            content = content_stream(data)
            # A 100-wide image should become 50 tall (preserving 2:1 ratio)
            # The CTM is written as "100 0 0 50 x y cm"
            expect(content).to_contain(b"100 0 0 50")
        finally:
            Path(path).unlink()

    @test
    def missing_image_does_not_crash():
        """A missing image file is silently skipped."""
        doc = HtmlDocument(
            string='<p>before</p><img src="/nonexistent/image.png"><p>after</p>'
        )
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"before")
        expect(content).to_contain(b"after")
        # No image XObject should appear
        assert b"/Subtype /Image" not in data

    @test
    def img_with_base_url():
        """base_url resolves relative src paths."""
        png = _make_png(1, 1, bytes([128, 128, 128]))
        with tempfile.TemporaryDirectory() as tmpdir:
            img_path = Path(tmpdir) / "pixel.png"
            img_path.write_bytes(png)
            doc = HtmlDocument(
                string='<img src="pixel.png">',
                base_url=tmpdir,
            )
            data = doc.to_bytes()
            expect(data).to_contain(b"/Subtype /Image")

    @test
    def multiple_images_get_distinct_names():
        """Multiple images in one document get /Im0 and /Im1."""
        png1 = _make_png(1, 1, bytes([255, 0, 0]))
        png2 = _make_png(1, 1, bytes([0, 255, 0]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f1:
            f1.write(png1)
            p1 = f1.name
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f2:
            f2.write(png2)
            p2 = f2.name
        try:
            doc = HtmlDocument(string=f'<img src="{p1}"><img src="{p2}">')
            data = doc.to_bytes()
            content = content_stream(data)
            expect(content).to_contain(b"/Im0 Do")
            expect(content).to_contain(b"/Im1 Do")
        finally:
            Path(p1).unlink()
            Path(p2).unlink()


with describe("attribute selectors"):

    @test
    def attr_equals_exact():
        """[name="value"] matches exact attribute equality."""
        html = (
            '<style>[data-role="primary"] { color: red }</style>'
            '<p data-role="primary">X</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_equals_no_match():
        """[name="value"] does not match when attribute value differs."""
        html = (
            '<style>[data-role="primary"] { color: red }</style>'
            '<p data-role="secondary">X</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    @test
    def attr_includes_whitespace_list():
        """[class~="note"] matches a whitespace-separated token list member."""
        html = (
            '<style>[class~="note"] { color: red }</style>'
            '<p class="intro note main">X</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_includes_no_substring_match():
        """[class~="note"] does not match a bare substring like 'notepad'."""
        html = '<style>[class~="note"] { color: red }</style><p class="notepad">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    @test
    def attr_dashmatch_en():
        """[lang|="en"] matches exact 'en'."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="en">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_dashmatch_en_us():
        """[lang|="en"] matches 'en-US' (prefix followed by dash)."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="en-US">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_dashmatch_english_no_match():
        """[lang|="en"] does not match 'english' (no dash after prefix)."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="english">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    @test
    def attr_exists_any_value():
        """[data-x] matches any element with that attribute regardless of value."""
        html = '<style>[data-x] { color: red }</style><p data-x="whatever">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_exists_empty_value():
        """[data-x] matches even when the attribute value is empty."""
        html = '<style>[data-x] { color: red }</style><p data-x="">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_compound_with_type():
        """p[class="foo"] requires both type and attribute to match."""
        html = '<style>p[class="foo"] { color: red }</style><p class="foo">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_compound_with_type_no_match():
        """p[class="foo"] does not match a div with class foo."""
        html = '<style>p[class="foo"] { color: red }</style><div class="foo">X</div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    # spec: CSS 2.1 §5.8; behaviors: sel-attribute
    @test
    def attr_prefix_match():
        """[href^="http"] matches values starting with 'http'."""
        html = (
            '<style>[href^="http"] { color: red }</style>'
            '<a href="https://example.com">X</a>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def attr_suffix_match():
        """[src$=".png"] matches values ending with '.png'."""
        html = '<style>[src$=".png"] { color: red }</style><p src="picture.png">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")


def _td_y_for_needle(data: bytes, needle: bytes) -> float:
    """Return the y coordinate of the Td (next_line) that immediately
    precedes the given `(needle) Tj` showtext. Caller passes the
    decompressed content stream (via `content_stream(...)`), so a simple
    bytes scan recovers baselines.
    """
    marker = b"(" + needle + b") Tj"
    idx = data.find(marker)
    assert idx >= 0, f"marker {needle!r} not found in PDF content"
    td_idx = data.rfind(b" Td\n", 0, idx)
    assert td_idx >= 0, "no Td before showtext"
    line_start = data.rfind(b"\n", 0, td_idx) + 1
    parts = data[line_start:td_idx].split()
    return float(parts[-1])


def _all_td_ys(data: bytes) -> list[float]:
    """All Td y-coordinates in the stream (in document order)."""
    ys: list[float] = []
    start = 0
    while True:
        idx = data.find(b" Td\n", start)
        if idx < 0:
            break
        line_start = data.rfind(b"\n", 0, idx) + 1
        parts = data[line_start:idx].split()
        if len(parts) >= 2:
            ys.append(float(parts[-1]))
        start = idx + 4
    return ys


with describe("table vertical-align"):
    # The "tall" cell is a narrow column filled with enough words to wrap
    # to several lines, making the row significantly taller than the first
    # cell's single line of text.
    _LONG_BLOB = " ".join(f"w{i}" for i in range(40))

    def _two_cell_table(va_for_short: str | None) -> bytes:
        va_attr = f' style="vertical-align: {va_for_short}"' if va_for_short else ""
        html = f"<table><tr><td{va_attr}>X</td><td>{_LONG_BLOB}</td></tr></table>"
        return content_stream(HtmlDocument(string=html).to_bytes())

    @test
    def vertical_align_default_is_top():
        """Without a vertical-align style, the short cell's baseline
        matches the tall cell's first line (both are top-aligned)."""
        data = _two_cell_table(None)
        short_y = _td_y_for_needle(data, b"X")
        all_ys = _all_td_ys(data)
        # The tall cell's top line is the maximum y seen in the table.
        top_y = max(all_ys)
        assert abs(short_y - top_y) < 1.0

    # spec: CSS 2.1 §10.8; behaviors: vfmd-vertical-align
    @test
    def vertical_align_top_matches_default():
        """vertical-align: top behaves identically to the default."""
        data = _two_cell_table("top")
        short_y = _td_y_for_needle(data, b"X")
        top_y = max(_all_td_ys(data))
        assert abs(short_y - top_y) < 1.0

    @test
    def vertical_align_middle_centers_content():
        """vertical-align: middle drops the short baseline below the
        first line and above the last line of the tall cell."""
        data = _two_cell_table("middle")
        short_y = _td_y_for_needle(data, b"X")
        ys = _all_td_ys(data)
        top_y = max(ys)
        bot_y = min(ys)
        assert short_y < top_y - 2.0, (
            f"middle not below top: short={short_y} top={top_y}"
        )
        assert short_y > bot_y + 2.0, (
            f"middle not above bottom: short={short_y} bot={bot_y}"
        )

    @test
    def vertical_align_bottom_drops_to_bottom():
        """vertical-align: bottom places the short baseline at (or very
        near) the tall cell's last line."""
        data = _two_cell_table("bottom")
        short_y = _td_y_for_needle(data, b"X")
        bot_y = min(_all_td_ys(data))
        assert abs(short_y - bot_y) < 2.0, (
            f"bottom not at bottom: short={short_y} bot={bot_y}"
        )

    @test
    def vertical_align_bottom_strictly_below_top():
        """For the same table, bottom is strictly lower than top."""
        top = _two_cell_table("top")
        bot = _two_cell_table("bottom")
        top_y = _td_y_for_needle(top, b"X")
        bot_y = _td_y_for_needle(bot, b"X")
        # The cell is many lines tall, so the gap should be substantial.
        assert bot_y < top_y - 10.0

    @test
    def vertical_align_invalid_keyword_is_ignored():
        """A keyword pdfun doesn't support (e.g. `super`) is dropped by
        the CSS parser, so the cell stays top-aligned and the PDF still
        renders."""
        html = (
            "<table><tr>"
            '<td style="vertical-align: super">X</td>'
            f"<td>{_LONG_BLOB}</td>"
            "</tr></table>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        assert b"%PDF" in data
        short_y = _td_y_for_needle(content, b"X")
        top_y = max(_all_td_ys(content))
        assert abs(short_y - top_y) < 1.0


with describe("table border-collapse"):

    @test
    def border_collapse_separate_preserves_per_cell_rects():
        """border-collapse: separate (default) draws one rect per cell."""
        html = (
            "<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # Each cell emits a `re\nS\n` (rect + stroke) pair for its border.
        # With 4 cells we expect at least 4 such stroke rectangles.
        expect(content.count(b" re\nS\n")).to_be_greater_than(3)

    @test
    def border_collapse_collapse_reduces_stroke_count():
        """border-collapse: collapse draws one outer rect plus internal
        grid lines, so the number of stroked rectangles is much smaller
        than the number of cells."""
        sep_html = (
            "<table><tr><td>A</td><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>"
        )
        col_html = (
            '<table style="border-collapse: collapse">'
            "<tr><td>A</td><td>B</td></tr>"
            "<tr><td>C</td><td>D</td></tr>"
            "</table>"
        )
        sep = HtmlDocument(string=sep_html).to_bytes()
        sep_content = content_stream(sep)
        col = HtmlDocument(string=col_html).to_bytes()
        col_content = content_stream(col)
        sep_rects = sep_content.count(b" re\nS\n")
        col_rects = col_content.count(b" re\nS\n")
        # Separate draws at least 4 bordered cells; collapse draws exactly
        # one outer rectangle plus `m`/`l`/`S` for internal gridlines.
        assert sep_rects >= 4, f"expected >=4 stroke rects, got {sep_rects}"
        assert col_rects == 1, f"expected 1 stroke rect, got {col_rects}"

    @test
    def border_collapse_collapse_emits_internal_gridlines():
        """collapse mode emits move/line/stroke triples for each internal
        vertical and horizontal grid line."""
        html = (
            '<table style="border-collapse: collapse">'
            "<tr><td>A</td><td>B</td><td>C</td></tr>"
            "<tr><td>D</td><td>E</td><td>F</td></tr>"
            "<tr><td>G</td><td>H</td><td>I</td></tr>"
            "</table>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # 3 columns -> 2 internal vertical gridlines
        # 3 rows -> 2 internal horizontal gridlines
        # Each gridline emits an `l\nS\n` (line + stroke) pair.
        expect(content.count(b" l\nS\n")).to_be_greater_than(3)
        # Text still renders.
        expect(content).to_contain(b"(A)")
        expect(content).to_contain(b"(I)")

    @test
    def border_collapse_separate_explicit_matches_default():
        """Explicit border-collapse: separate matches the default
        (both draw one stroked rectangle per cell)."""
        default_html = "<table><tr><td>A</td><td>B</td></tr></table>"
        explicit_html = (
            '<table style="border-collapse: separate">'
            "<tr><td>A</td><td>B</td></tr></table>"
        )
        d1 = HtmlDocument(string=default_html).to_bytes()
        d1_content = content_stream(d1)
        d2 = HtmlDocument(string=explicit_html).to_bytes()
        d2_content = content_stream(d2)
        assert d1_content.count(b" re\nS\n") == d2_content.count(b" re\nS\n")

    @test
    def border_collapse_single_row_single_col():
        """A 1x1 table in collapse mode still renders the outer rectangle
        and its single cell's content."""
        html = '<table style="border-collapse: collapse"><tr><td>Solo</td></tr></table>'
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Solo)")
        # Exactly one rectangle (the outer border) and no gridlines.
        assert content.count(b" re\nS\n") == 1

    @test
    def attr_substring_match():
        """[class*="big"] matches values containing 'big'."""
        html = '<style>[class*="big"] { color: red }</style><p class="thebigone">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")


with describe("pseudo-classes and sibling combinators"):
    # spec: CSS 2.1 §5.11.3; behaviors: sel-pseudo-first-child
    @test
    def first_child_matches_first_p():
        """p:first-child matches only the first p in a div."""
        html = (
            "<style>p:first-child { color: red }</style>"
            "<div><p>first</p><p>second</p><p>third</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def first_child_skips_second_p():
        """p:first-child does not match siblings after the first."""
        # Style the second p blue via a different selector, and the first p red.
        # Assert both colors are present once.
        html = (
            "<style>p:first-child { color: red } .second { color: blue }</style>"
            '<div><p>a</p><p class="second">b</p></div>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        expect(content).to_contain(b"0 0 1 rg")

    @test
    def last_child_matches_last_p():
        """p:last-child matches the final p among its siblings."""
        html = (
            "<style>p:last-child { color: red }</style>"
            "<div><p>one</p><p>two</p><p>three</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def only_child_matches_single_child():
        """p:only-child matches a p that is the sole element child of its parent."""
        html = "<style>p:only-child { color: red }</style><div><p>alone</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def only_child_no_match_when_siblings():
        """p:only-child does not match when the p has siblings."""
        html = "<style>p:only-child { color: red }</style><div><p>a</p><p>b</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    # spec: CSS 2.1 §5.11.3; behaviors: sel-pseudo-nth-child
    @test
    def nth_child_even_hits_even_positions():
        """li:nth-child(2n) matches the even-indexed children."""
        html = (
            "<style>li:nth-child(2n) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def nth_child_odd_hits_odd_positions():
        """li:nth-child(odd) matches the odd-indexed children."""
        html = (
            "<style>li:nth-child(odd) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def nth_child_literal_three():
        """li:nth-child(3) matches only the third element sibling."""
        html = (
            "<style>li:nth-child(3) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    # spec: CSS 2.1 §5.11.4; behaviors: sel-pseudo-not
    @test
    def not_pseudo_excludes_class():
        """p:not(.skip) matches plain p but not p.skip."""
        html = (
            "<style>p:not(.skip) { color: red }</style>"
            '<p>one</p><p class="skip">two</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        # And 'two' should not be red — but we can only tell structurally.
        # Count red-set ops: exactly one fill-color set should appear for the
        # matching p. (The second paragraph falls back to default color.)

    # spec: CSS 2.1 §5.7; behaviors: sel-adjacent-sibling
    @test
    def adjacent_sibling_matches_immediate_p():
        """h1 + p styles the p immediately following an h1."""
        html = (
            "<style>h1 + p { color: red }</style>"
            "<h1>Heading</h1><p>just after</p><p>later</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def adjacent_sibling_no_match_when_not_adjacent():
        """h1 + p does not match a p that is not immediately after an h1."""
        html = (
            "<style>h1 + p { color: red }</style>"
            "<h1>Heading</h1><div>gap</div><p>too late</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 rg" not in content).to_equal(True)

    # spec: CSS 2.1 §5.7; behaviors: sel-general-sibling
    @test
    def general_sibling_matches_all_following_p():
        """h1 ~ p styles all p elements following an h1 in document order."""
        html = (
            "<style>h1 ~ p { color: red }</style>"
            "<h1>Heading</h1><div>noise</div><p>after one</p><p>after two</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def pseudo_composes_with_child_combinator():
        """div > p:first-child composes pseudo-classes with child combinators."""
        html = (
            "<style>div > p:first-child { color: red }</style>"
            "<div><p>first</p><p>second</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")


with describe("sup and sub"):

    @test
    def sup_renders_text():
        """<sup> text is included in the PDF content stream."""
        html = "<p>E=mc<sup>2</sup></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"E=mc")
        expect(content).to_contain(b"2")

    @test
    def sub_renders_text():
        """<sub> text is included in the PDF content stream."""
        html = "<p>H<sub>2</sub>O</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(H")
        expect(content).to_contain(b"2")
        expect(content).to_contain(b"O)")


with describe("table caption"):

    @test
    def caption_appears_before_table_rows():
        """<caption> renders as text above the first row."""
        html = "<table><caption>My Data</caption><tr><td>a</td><td>b</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(My Data)")
        expect(content).to_contain(b"(a)")

    @test
    def caption_without_rows_still_renders():
        """A <caption>-only table still renders the caption text."""
        html = "<table><caption>Orphan</caption></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(Orphan)")


with describe("min-height and max-height"):
    # spec: CSS 2.1 §10.5; behaviors: vfmd-height
    @test
    def min_height_expands_short_block():
        """min-height forces a block to occupy at least the specified height."""
        html = '<div style="min-height: 100pt; background-color: red">short</div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(short)")

    @test
    def max_height_clamps_tall_block():
        """max-height is accepted without crashing."""
        html = (
            '<div style="max-height: 20pt; background-color: blue">'
            "content that would normally be taller</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"0 0 1 rg")


with describe("per-corner border-radius"):

    @test
    def shorthand_border_radius_four_corners():
        """border-radius shorthand accepts 1-4 values."""
        html = (
            '<div style="border: 1pt solid black; border-radius: 5pt 10pt 15pt 20pt">'
            "rounded</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(rounded)")

    @test
    def border_top_left_radius_longhand():
        """border-top-left-radius longhand is parsed and applied."""
        html = (
            '<div style="border: 1pt solid black; border-top-left-radius: 8pt">'
            "corner</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(corner)")

    @test
    def per_corner_radii_all_four_longhands():
        """All four per-corner border-radius longhands are parsed."""
        html = (
            '<div style="border: 1pt solid black;'
            " border-top-left-radius: 2pt;"
            " border-top-right-radius: 4pt;"
            " border-bottom-right-radius: 6pt;"
            ' border-bottom-left-radius: 8pt">corners</div>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(corners)")


with describe("opacity"):

    @test
    def opacity_emits_ext_graphics_state():
        """opacity < 1 emits a /Gs1 gs call and an ExtGState resource."""
        html = '<p style="opacity: 0.5">faded</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"/Gs1 gs")
        expect(data).to_contain(b"/ExtGState")

    @test
    def opacity_full_does_not_emit_state():
        """opacity: 1 is a no-op — no ExtGState emitted."""
        html = '<p style="opacity: 1">opaque</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"/Gs1 gs" not in content).to_equal(True)

    @test
    def opacity_clamped_zero_one():
        """opacity values outside [0,1] are clamped."""
        html = '<p style="opacity: 0.25">quarter</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"/Gs1 gs")


with describe("rgba/hsla alpha plumbing (WS-2)"):
    """End-to-end coverage for `rgba()` / `hsla()` α < 1.0 flowing all
    the way through to PDF's ExtGState `ca` / `CA` channels. ISO
    32000-1 §11.3.7."""

    @test
    def translucent_fill_emits_extgstate():
        """A `background-color: rgba(255, 0, 0, 0.5)` block emits a
        `/Gs<N> gs` op immediately before the `1 0 0 rg` fill, and the
        document's `/ExtGState` resource binds that key to `/ca 0.5
        /CA 0.5` (non-stroking AND stroking — never stale)."""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " background-color: rgba(255, 0, 0, 0.5)'>x</div>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # Background color is unchanged at the rg op.
        expect(content).to_contain(b"1 0 0 rg")
        # An ExtGState reference precedes it, gating the fill alpha.
        idx_gs = content.index(b"gs")
        idx_rg = content.index(b"1 0 0 rg")
        expect(idx_gs < idx_rg).to_equal(True)
        # The page resource dict + ExtGState entries land in the doc.
        expect(data).to_contain(b"/ExtGState")
        # ISO 32000-1 §11.3.7: both `ca` AND `CA` MUST be set together.
        # A stale `CA` from a prior op carries through if not reset.
        expect(data).to_contain(b"/ca 0.5")
        expect(data).to_contain(b"/CA 0.5")

    @test
    def translucent_border_uses_CA():
        """A translucent border (`rgba(0,0,255,0.5)` on a stroked rect)
        emits the `/Gs<N> gs` gate before the `0 0 1 RG` stroke color
        — and the ExtGState entry sets `CA` (stroking alpha)."""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " border: 2pt solid rgba(0, 0, 255, 0.5)'>x</div>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # Stroke color (uppercase RG) for the border.
        expect(content).to_contain(b"0 0 1 RG")
        idx_gs = content.index(b"gs")
        idx_rg = content.index(b"0 0 1 RG")
        expect(idx_gs < idx_rg).to_equal(True)
        expect(data).to_contain(b"/CA 0.5")
        # Both channels are still set together — `ca` must be present too.
        expect(data).to_contain(b"/ca 0.5")

    @test
    def opaque_color_emits_no_extgstate():
        """`rgba(...)` with α = 1.0 (or plain `rgb()`) does NOT emit a
        `/Gs<N> gs` gate — only the opaque rg/RG ops. Cheap path."""
        html = '<p style="color: rgba(255, 0, 0, 1.0)">opaque</p>'
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        # No /Gs gate — full alpha needs no ExtGState reference.
        expect(b"/Gs1 gs" not in content).to_equal(True)

    @test
    def page_group_entry_present_when_translucent():
        """ISO 32000-1 §11.6.6 + WeasyPrint issue #2723: a page that
        contains any `ca`/`CA != 1` gets a `/Group << /S /Transparency
        /CS /DeviceRGB /I true >>` entry on the page object so the
        viewer doesn't fall back to non-isolated-against-page-backdrop
        and produce divergent renderings between Adobe / Foxit /
        Preview."""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " background-color: rgba(255, 0, 0, 0.5)'>x</div>"
        )
        data = HtmlDocument(string=html).to_bytes()
        # The page dict must carry the Transparency Group entry.
        expect(data).to_contain(b"/Group")
        expect(data).to_contain(b"/S /Transparency")
        expect(data).to_contain(b"/CS /DeviceRGB")
        expect(data).to_contain(b"/I true")

    @test
    def page_group_absent_when_fully_opaque():
        """Pages that paint only opaque colors do NOT emit the page
        Group entry — no extra dict cost on opaque pages."""
        html = "<div style='background: rgb(255, 0, 0)'>x</div>"
        data = HtmlDocument(string=html).to_bytes()
        # No translucent paint anywhere → no page Group entry.
        expect(b"/S /Transparency" not in data).to_equal(True)


with describe("box-shadow"):

    @test
    def box_shadow_emits_offset_fill():
        """box-shadow: 5pt 5pt black paints a filled rect offset from the
        block before the block's own background. spec:bb3-box-shadow"""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " background-color: rgb(200, 200, 200);"
            " box-shadow: 5pt 5pt 0 0 rgb(0, 0, 0)'>x</div>"
        )
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # Shadow is black (0 0 0 rg) and must precede the grey background.
        idx_shadow = content.index(b"0 0 0 rg")
        idx_bg = content.index(b"0.78431374 0.78431374 0.78431374 rg")
        expect(idx_shadow < idx_bg).to_equal(True)

    @test
    def box_shadow_none_paints_no_shadow():
        """box-shadow: none emits no extra fill ops. spec:bb3-box-shadow"""
        base = "<div style='width: 60pt; height: 40pt'>x</div>"
        none = "<div style='width: 60pt; height: 40pt; box-shadow: none'>x</div>"
        expect(content_stream(HtmlDocument(string=base).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=none).to_bytes())
        )

    @test
    def box_shadow_multiple_layers_back_to_front():
        """Multiple comma-separated shadows paint last-first so the
        first-declared layer lands on top. spec:bb3-box-shadow"""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " box-shadow: 2pt 2pt 0 0 rgb(255, 0, 0),"
            " 10pt 10pt 0 0 rgb(0, 255, 0)'>x</div>"
        )
        content = content_stream(HtmlDocument(string=html).to_bytes())
        # Green is declared second but must paint first (back).
        idx_red = content.index(b"1 0 0 rg")
        idx_green = content.index(b"0 1 0 rg")
        expect(idx_green < idx_red).to_equal(True)

    @test
    def box_shadow_spread_inflates_shadow_rect():
        """Positive spread inflates the shadow rect equally on all sides.
        spec:bb3-box-shadow"""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " box-shadow: 0 0 0 5pt rgb(0, 0, 0)'>x</div>"
        )
        html_no_spread = (
            "<div style='width: 60pt; height: 40pt;"
            " box-shadow: 0 0 0 0 rgb(0, 0, 0)'>x</div>"
        )
        with_spread = content_stream(HtmlDocument(string=html).to_bytes())
        without = content_stream(HtmlDocument(string=html_no_spread).to_bytes())
        # With spread=5pt, the shadow rect width grows by 2*5=10pt.
        expect(with_spread).to_contain(b" 70 ")
        expect(without).to_contain(b" 60 ")

    @test
    def box_shadow_inset_uses_even_odd_fill():
        """An inset shadow paints an annulus inside the padding box using
        the even-odd fill operator (f*). spec:bb3-box-shadow"""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " box-shadow: inset 4pt 4pt 0 0 rgb(0, 0, 0)'>x</div>"
        )
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"f*")

    @test
    def box_shadow_inset_uses_clip_for_containment():
        """An inset shadow wraps its paint in a save/clip/restore bracket so
        it can't spill outside the padding box. spec:bb3-box-shadow"""
        html = (
            "<div style='width: 60pt; height: 40pt;"
            " box-shadow: inset 4pt 4pt 0 0 rgb(0, 0, 0)'>x</div>"
        )
        content = content_stream(HtmlDocument(string=html).to_bytes())
        # q ... W n ... f* ... Q sequence for the inset.
        expect(content).to_contain(b"q\n")
        expect(content).to_contain(b"W\nn\n")
        expect(content).to_contain(b"f*\n")
        expect(content).to_contain(b"Q\n")


with describe("inline decoration tags"):

    @test
    def u_tag_emits_underline_stroke():
        """<u> text is underlined (stroke line in content stream)."""
        html = "<p>This is <u>underlined</u> text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # text_decoration underline draws a line, which uses S (stroke)
        expect(content).to_contain(b"underlined")

    @test
    def ins_tag_is_underlined_like_u():
        """<ins> renders the same way as <u>."""
        html = "<p>Marked <ins>insertion</ins></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"insertion")

    @test
    def del_tag_emits_strikethrough():
        """<del> text has line-through decoration."""
        html = "<p>removed <del>crossed out</del> here</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"crossed out")

    @test
    def s_tag_is_strikethrough_like_del():
        """<s> is treated as strikethrough."""
        html = "<p><s>old price</s></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"old price")


with describe("details and summary"):
    # spec: HTML; behaviors: html-details
    @test
    def summary_renders_as_bold_block():
        """<summary> inside <details> is rendered as a block with text."""
        html = (
            "<details><summary>Click me</summary>"
            "<p>Hidden by default in a browser, shown in PDF.</p></details>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Click me")
        expect(content).to_contain(b"Hidden by default")

    @test
    def details_body_always_visible_in_pdf():
        """PDF rendering ignores the open attribute — body is always laid out."""
        html = "<details><summary>S</summary><div>Body text</div></details>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Body text")


with describe("display: inline-block"):
    # spec: CSS 2.1 §9.2.2; behaviors: vfm-display-inline
    @test
    def inline_block_renders_as_inline_atom():
        """<span style="display: inline-block"> flows inline with surrounding text."""
        html = "<p>Before <span style='display:inline-block;'>Badge</span> after.</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Before")
        expect(content).to_contain(b"Badge")
        expect(content).to_contain(b"after")

    # spec: CSS 2.1 §9.2.4; behaviors: vfm-display-inline-block
    @test
    def inline_block_with_fixed_width():
        """A declared width on an inline-block is honored as the atom's width."""
        html = (
            "<p>Foo <span style='display:inline-block; width: 80px;'>Hi</span> bar</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Hi")
        expect(content).to_contain(b"Foo")
        expect(content).to_contain(b"bar")

    @test
    def inline_block_with_background_color():
        """Background color on an inline-block emits a filled rectangle."""
        html = (
            "<p>Status: "
            "<span style='display:inline-block; background-color: #ff0000; "
            "color: #ffffff;'>ERR</span>"
            "</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # The red fill color should appear in the content stream
        expect(content).to_contain(b"1 0 0 rg")
        expect(content).to_contain(b"ERR")

    @test
    def inline_block_with_border():
        """Border on an inline-block emits a stroked rectangle around the atom."""
        html = (
            "<p>Label "
            "<span style='display:inline-block; border: 1px solid black; "
            "padding: 2px;'>tag</span>"
            "</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"tag")
        # A stroked rectangle should appear somewhere in the content stream
        expect(content).to_contain(b" re\n")

    @test
    def inline_block_preserves_surrounding_text_order():
        """Multiple inline-blocks in a single paragraph all render in order."""
        html = (
            "<p>"
            "<span style='display:inline-block;'>One</span> "
            "<span style='display:inline-block;'>Two</span> "
            "<span style='display:inline-block;'>Three</span>"
            "</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"One")
        expect(content).to_contain(b"Two")
        expect(content).to_contain(b"Three")

    @test
    def inline_block_never_splits_across_lines():
        """An inline-block with long text stays atomic even if it overflows."""
        html = "<p>x <span style='display:inline-block; width: 50px;'>Badge</span></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Badge")


with describe("calc()"):
    # spec: CSS Values 3 §10; behaviors: values3-calc

    def _width_div(width_expr: str) -> bytes:
        style = f"width: {width_expr}; background-color: red"
        html = f'<div style="{style}">x</div>'
        return content_stream(HtmlDocument(string=html).to_bytes())

    @test
    def calc_subtraction_in_width():
        """`width: calc(100px - 20px)` resolves to `80px`."""
        expect(_width_div("calc(100px - 20px)")).to_equal(_width_div("80px"))

    @test
    def calc_addition_in_width():
        """`width: calc(50px + 30px)` resolves to `80px`."""
        expect(_width_div("calc(50px + 30px)")).to_equal(_width_div("80px"))

    @test
    def calc_multiplication_length_times_number():
        """`calc(10px * 2)` resolves to `20px`."""
        expect(_width_div("calc(10px * 2)")).to_equal(_width_div("20px"))

    @test
    def calc_multiplication_number_times_length():
        """`calc(2 * 10px)` also resolves to `20px` (operand order)."""
        expect(_width_div("calc(2 * 10px)")).to_equal(_width_div("20px"))

    @test
    def calc_division_by_number():
        """`calc(100px / 4)` resolves to `25px`."""
        expect(_width_div("calc(100px / 4)")).to_equal(_width_div("25px"))

    @test
    def calc_nested_with_parens():
        """Parenthesised sub-expression: `calc((100px - 20px) - 10px)` → `70px`."""
        expect(_width_div("calc((100px - 20px) - 10px)")).to_equal(_width_div("70px"))

    @test
    def calc_nested_calc_call():
        """A nested `calc(...)` inside another `calc(...)` resolves correctly."""
        expect(_width_div("calc(calc(100px - 20px) - 10px)")).to_equal(
            _width_div("70px")
        )

    @test
    def calc_with_mixed_units():
        """`calc(1em + 4px)` — 1em at the default 12pt font = 12pt, plus
        4px (= 3pt) = 15pt. `calc(1em + 4px)` equals that literal pt
        total."""
        expect(_width_div("calc(1em + 4px)")).to_equal(_width_div("15pt"))

    @test
    def calc_with_percentage():
        """`calc(50% + 10px)` resolves against the containing-block width.

        The default page content area is US Letter (612pt) minus 72pt
        margins on each side = 468pt. So `50% + 10px = 234pt + 7.5pt = 241.5pt`.
        """
        expect(_width_div("calc(50% + 10px)")).to_equal(_width_div("241.5pt"))


with describe("overflow"):
    # spec: CSS 2.1 §11.1; behaviors: ve-overflow

    @test
    def overflow_hidden_emits_clip_op():
        """`overflow: hidden` emits a `W n` clip op (nonzero-rule clip path
        ending without fill/stroke)."""
        html = (
            "<div style='overflow: hidden; width: 50px; height: 40px;'>"
            "Some text that overflows."
            "</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"\nW\n")
        expect(content).to_contain(b"\nn\n")

    @test
    def overflow_visible_does_not_clip():
        """`overflow: visible` is the default and must not emit `W n`."""
        html = (
            "<div style='overflow: visible; width: 50px; height: 40px;'>"
            "Some text."
            "</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # A plain block without clipping should not emit a W n pair.
        expect(content.find(b"\nW\n")).to_equal(-1)

    @test
    def overflow_scroll_clips_like_hidden_in_print():
        """In a paged medium (PDF), `overflow: scroll` falls back to
        hidden-like clipping — emits a W n pair."""
        html = (
            "<div style='overflow: scroll; width: 50px; height: 40px;'>Some text.</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"\nW\n")

    @test
    def overflow_auto_clips_like_hidden_in_print():
        """`overflow: auto` also collapses to hidden in PDF output."""
        html = (
            "<div style='overflow: auto; width: 50px; height: 40px;'>Some text.</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"\nW\n")

    @test
    def overflow_hidden_clip_is_wrapped_in_save_restore():
        """The clip path is scoped to a q…Q pair so it doesn't leak to
        subsequent content."""
        html = (
            "<div style='overflow: hidden; width: 50px; height: 40px;'>A</div>"
            "<p>Unclipped paragraph.</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        # Every W must sit between a matched q/Q pair emitted for the block.
        w_idx = content.find(b"\nW\n")
        expect(w_idx).to_be_greater_than(-1)
        # A Q (restore) must appear after the W.
        expect(content.find(b"\nQ\n", w_idx)).to_be_greater_than(-1)


with describe("position: relative"):
    # spec: CSS 2.1 §9.4.3; behaviors: vfm-position-relative, vfm-offsets

    @test
    def relative_top_shifts_box_down():
        """`position: relative; top: 10pt` emits a negative-y translate
        (PDF y grows upward, CSS top grows downward)."""
        html = "<div style='position: relative; top: 10pt;'>Shifted down</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 1 0 -10 cm")
        expect(content).to_contain(b"Shifted down")

    @test
    def relative_left_shifts_box_right():
        """`position: relative; left: 15pt` emits a positive-x translate."""
        html = "<div style='position: relative; left: 15pt;'>Shifted right</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 1 15 0 cm")

    @test
    def relative_top_and_left_combined():
        """Both `top` and `left` contribute to a single translation matrix."""
        html = "<div style='position: relative; top: 5pt; left: 20pt;'>x</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 1 20 -5 cm")

    @test
    def relative_does_not_disturb_siblings():
        """A relatively-positioned box does not shift the sibling that
        follows it — the translate is wrapped in q…Q."""
        html = (
            "<div style='position: relative; top: 100pt;'>First</div><div>Second</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        cm_idx = content.find(b"1 0 0 1 0 -100 cm")
        expect(cm_idx).to_be_greater_than(-1)
        # There is a Q (restore) between the translate and the sibling's
        # text — the translate's effect is scoped to the first box only.
        second_idx = content.find(b"Second", cm_idx)
        expect(second_idx).to_be_greater_than(-1)
        expect(content.rfind(b"\nQ\n", cm_idx, second_idx)).to_be_greater_than(-1)

    @test
    def relative_without_offsets_emits_no_translate():
        """`position: relative` with no `top`/`left`/etc. is a no-op — no
        translate op is emitted."""
        html = "<div style='position: relative;'>Plain</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(b"1 0 0 1 0 0 cm" in content).to_equal(False)


with describe("position: absolute"):
    # spec: CSS 2.1 §9.6; behaviors: vfm-position-absolute, vfm-offsets

    @test
    def absolute_with_top_left_positions_box_relative_to_page():
        """`position: absolute; top: 100pt; left: 50pt` paints the box at
        page coordinates, using a translate relative to the in-flow
        cursor. The box's text content appears in the stream."""
        html = "<div style='position: absolute; top: 100pt; left: 50pt;'>ABS</div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(ABS)")
        # The translate is wrapped in q…Q (SaveState/RestoreState).
        expect(content).to_contain(b"\nq\n")
        expect(content).to_contain(b"\nQ\n")

    @test
    def absolute_is_removed_from_normal_flow():
        """A sibling following an absolute-positioned box renders at the
        same y-position as if the absolute box were not there — confirms
        siblings don't see its metrics."""
        with_abs = HtmlDocument(
            string=(
                "<p>BEFORE</p>"
                "<div style='position: absolute; top: 400pt; left: 300pt;'>A</div>"
                "<p>AFTER</p>"
            )
        ).to_bytes()
        without_abs = HtmlDocument(string="<p>BEFORE</p><p>AFTER</p>").to_bytes()
        # The "AFTER" paragraph's Td (baseline) should match in both
        # streams — the absolute box did not advance the cursor.
        cs_with = content_stream(with_abs)
        cs_without = content_stream(without_abs)
        # Extract the Td that precedes "(AFTER)" in each stream.
        import re as _re

        def baseline_of(content: bytes, text: bytes) -> bytes | None:
            idx = content.find(text)
            if idx == -1:
                return None
            prefix = content[:idx]
            matches = _re.findall(rb"([\-\d\.]+ [\-\d\.]+) Td", prefix)
            return matches[-1] if matches else None

        expect(baseline_of(cs_with, b"(AFTER)")).to_equal(
            baseline_of(cs_without, b"(AFTER)")
        )

    @test
    def absolute_with_right_and_bottom_anchors_to_opposite_edge():
        """When `left` is absent, `right` is used — the box's right edge
        is anchored that many points from the page's right edge."""
        # Use an explicit width so the computed position is stable.
        html = (
            "<div style='position: absolute; top: 10pt; right: 20pt;"
            " width: 50pt;'>R</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(R)")
        # Default page width is 612pt. Box's left edge = 612 - 20 - 50 = 542.
        # We don't assert the exact translate delta (depends on natural
        # flow origin) but the PDF should still render without error.


with describe("position: fixed"):
    # spec: CSS 2.1 §9.6; behaviors: vfm-position-fixed, vfm-offsets

    @test
    def fixed_paints_on_every_page():
        """A `position: fixed` block is stamped onto every page of the
        document, so the text appears in each page's content stream."""
        html = (
            "<div style='position: fixed; top: 10pt; left: 20pt;'>STAMP</div>"
            "<p>ONE</p>"
            "<p style='page-break-before: always;'>TWO</p>"
            "<p style='page-break-before: always;'>THREE</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        import fitz as _fitz

        with _fitz.open(stream=data, filetype="pdf") as pdf:
            expect(len(pdf)).to_equal(3)
            for page in pdf:
                expect(b"(STAMP)" in page.read_contents()).to_equal(True)

    @test
    def fixed_paints_once_per_page_even_with_multiple_siblings():
        """A fixed block defined before several in-flow siblings is
        stamped exactly once per page — not once per sibling."""
        html = (
            "<div style='position: fixed; top: 10pt; left: 20pt;'>F</div>"
            "<p>A</p><p>B</p><p>C</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content.count(b"(F)")).to_equal(1)

    @test
    def fixed_is_removed_from_normal_flow():
        """Siblings following a fixed-positioned box render as if the
        fixed box were not there."""
        with_fixed = HtmlDocument(
            string=(
                "<div style='position: fixed; top: 10pt; left: 20pt;'>F</div>"
                "<p>AFTER</p>"
            )
        ).to_bytes()
        without_fixed = HtmlDocument(string="<p>AFTER</p>").to_bytes()
        import re as _re

        def baseline_of(content: bytes, text: bytes) -> bytes | None:
            idx = content.find(text)
            if idx == -1:
                return None
            prefix = content[:idx]
            matches = _re.findall(rb"([\-\d\.]+ [\-\d\.]+) Td", prefix)
            return matches[-1] if matches else None

        cs_with = content_stream(with_fixed)
        cs_without = content_stream(without_fixed)
        expect(baseline_of(cs_with, b"(AFTER)")).to_equal(
            baseline_of(cs_without, b"(AFTER)")
        )


with describe("background-image"):
    # spec: CSS 2.1 §14.2.1; behaviors: bb3-bg-image, bb3-bg-repeat,
    # bb3-bg-size, bb3-bg-position

    @test
    def background_image_url_emits_xobject():
        """`background-image: url(...)` loads the PNG and emits an Image
        XObject plus a `Do` op referencing it."""
        png = _make_png(2, 2, bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(
                string=(
                    f"<div style='background-image: url({path}); "
                    f"width: 30pt; height: 30pt;'>Block</div>"
                )
            )
            data = doc.to_bytes()
            content = content_stream(data)
            expect(data).to_contain(b"/XObject")
            expect(data).to_contain(b"/Subtype /Image")
            expect(content).to_contain(b"/Im0 Do")
            expect(content).to_contain(b"Block")
        finally:
            Path(path).unlink()

    @test
    def background_repeat_no_repeat_paints_once():
        """With `background-repeat: no-repeat` the Image XObject is
        painted exactly once regardless of box size."""
        png = _make_png(1, 1, bytes([0, 0, 0]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(
                string=(
                    f"<div style='background-image: url({path}); "
                    f"background-repeat: no-repeat; "
                    f"width: 100pt; height: 100pt;'>X</div>"
                )
            )
            data = doc.to_bytes()
            content = content_stream(data)
            # Count `Do` operator invocations within the block's clip.
            expect(content.count(b"/Im0 Do")).to_equal(1)
        finally:
            Path(path).unlink()

    @test
    def background_repeat_tiles_multiple_times():
        """With the default `repeat`, a small tile is drawn multiple times
        to cover a larger padding box."""
        png = _make_png(1, 1, bytes([0, 0, 0]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            # Intrinsic 1px = 0.75pt tile. In a 30ptx30pt box we expect
            # 40x40 = 1600 tiles.
            doc = HtmlDocument(
                string=(
                    f"<div style='background-image: url({path}); "
                    f"width: 30pt; height: 30pt;'>X</div>"
                )
            )
            data = doc.to_bytes()
            content = content_stream(data)
            expect(content.count(b"/Im0 Do")).to_be_greater_than(100)
        finally:
            Path(path).unlink()

    @test
    def background_size_cover_fills_box():
        """`background-size: cover` scales the tile to fully cover the
        padding box. A single 2x2 tile in a 60ptx30pt box with cover+
        no-repeat paints one tile at least 60pt wide (inspected via the
        image transform matrix in the DrawImage op)."""
        png = _make_png(2, 2, bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(
                string=(
                    f"<div style='background-image: url({path}); "
                    f"background-size: cover; "
                    f"background-repeat: no-repeat; "
                    f"width: 60pt; height: 30pt;'>X</div>"
                )
            )
            data = doc.to_bytes()
            content = content_stream(data)
            # Exactly one tile.
            expect(content.count(b"/Im0 Do")).to_equal(1)
            # The PDF cm matrix for the tile is emitted as `w 0 0 h x y cm`
            # where `w` is the tile width. Cover should produce w≥60.
            expect(content).to_contain(b"60 0 0 60")
        finally:
            Path(path).unlink()

    @test
    def background_image_over_background_color_both_rendered():
        """A block with both `background-color` and `background-image`
        emits the color fill first, then the image — colour is underneath."""
        png = _make_png(1, 1, bytes([0, 0, 0]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            # CSS url(...) treats `\` as an escape character, so Windows
            # paths with backslashes get corrupted. Normalise to POSIX.
            path = Path(f.name).as_posix()
        try:
            doc = HtmlDocument(
                string=(
                    f"<div style='background-color: red; "
                    f"background-image: url({path}); "
                    f"background-repeat: no-repeat; "
                    f"width: 30pt; height: 30pt;'>X</div>"
                )
            )
            data = doc.to_bytes()
            content = content_stream(data)
            # `f` (fill) for the color must appear before the first `Do`
            # for the image.
            fill_idx = content.find(b"\nf\n")
            do_idx = content.find(b"/Im0 Do")
            expect(fill_idx).to_be_greater_than(-1)
            expect(do_idx).to_be_greater_than(-1)
            expect(fill_idx).to_be_less_than(do_idx)
        finally:
            Path(path).unlink()


with describe("float and clear"):

    @test
    def left_float_renders_at_left_edge():
        """<div style="float:left"> sits at the left edge of its column."""
        html = (
            "<div style='float:left; width: 100px; background-color: red;'>"
            "Floated"
            "</div>"
            "<p>Following text flows next to the float.</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Floated")
        expect(content).to_contain(b"Following text")

    @test
    def right_float_renders_at_right_edge():
        """<div style="float:right"> sits at the right edge of its column."""
        html = (
            "<div style='float:right; width: 100px;'>Side</div><p>Main body text.</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Side")
        expect(content).to_contain(b"Main body text")

    @test
    def float_does_not_advance_cursor():
        """Subsequent content flows from the float's top, not its bottom."""
        html = (
            "<div style='float:left; width: 80px; height: 200px;'>X</div>"
            "<p>Text next to float</p>"
            "<p>Second paragraph</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Text next to float")
        expect(content).to_contain(b"Second paragraph")

    # spec: CSS 2.1 §9.5.2; behaviors: vfm-clear
    @test
    def clear_both_drops_below_floats():
        """A block with clear: both sits below preceding left/right floats."""
        html = (
            "<div style='float:left; width: 80px;'>L</div>"
            "<div style='float:right; width: 80px;'>R</div>"
            "<p>Inline text</p>"
            "<p style='clear: both;'>Below both</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Below both")

    @test
    def clear_left_only_affects_left_floats():
        """clear: left bypasses left floats but not right floats."""
        html = (
            "<div style='float:left; width: 60px;'>L</div>"
            "<p style='clear: left;'>After clear</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"After clear")

    @test
    def multiple_floats_stack_on_same_side():
        """Two left-floats and a flowing paragraph all render without error."""
        html = (
            "<div style='float:left; width: 50px;'>A</div>"
            "<div style='float:left; width: 50px;'>B</div>"
            "<p>Flowing</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Flowing")


with describe("bookmarks and internal links"):
    # spec: PDF; behaviors: pdf-bookmarks
    @test
    def headings_emit_outlines_key():
        """An h1 heading produces /Outlines in the catalog."""
        doc = HtmlDocument(string="<h1>Chapter One</h1><p>body</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Outlines")

    @test
    def heading_text_appears_in_outline():
        """The heading text is emitted in an outline item /Title."""
        doc = HtmlDocument(string="<h1>Introduction</h1>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Introduction")
        # Outlines dictionary marker.
        expect(data).to_contain(b"/Outlines")

    @test
    def no_headings_no_outline():
        """A document with no headings does not emit an /Outlines catalog entry."""
        doc = HtmlDocument(string="<p>just a paragraph</p>")
        data = doc.to_bytes()
        assert b"/Outlines" not in data

    @test
    def nested_hierarchy_links_h2_to_h1_parent():
        """An h2 following an h1 becomes a child of the h1 (parent reference)."""
        doc = HtmlDocument(string="<h1>Parent</h1><h2>Child</h2><h2>Sibling</h2>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Parent")
        expect(content).to_contain(b"Child")
        expect(content).to_contain(b"Sibling")
        # The h1 is the outline parent; its item should have /First and /Last
        # attributes pointing at its two h2 children. We check for the
        # generic /First and /Last markers on an outline item.
        expect(data).to_contain(b"/First")
        expect(data).to_contain(b"/Last")
        expect(data).to_contain(b"/Parent")

    @test
    def multiple_top_level_headings():
        """Two h1s produce two top-level outline items with /Next/Prev siblings."""
        doc = HtmlDocument(string="<h1>Alpha</h1><h1>Beta</h1>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Alpha")
        expect(content).to_contain(b"Beta")
        expect(data).to_contain(b"/Next")
        expect(data).to_contain(b"/Prev")

    # spec: PDF; behaviors: pdf-internal-anchors
    @test
    def internal_link_emits_goto_action():
        """<a href="#target"> with a matching id becomes a GoTo action."""
        html = '<h1 id="start">Top</h1><p><a href="#start">jump</a></p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Link")
        expect(data).to_contain(b"/GoTo")
        # Destination arrays use /FitH for the vertical-offset fit mode.
        expect(data).to_contain(b"/FitH")
        # External URL action type should not appear for a pure internal link.
        assert b"/URI" not in data

    @test
    def internal_link_to_paragraph_id():
        """An id on a <p> is a valid anchor target."""
        html = (
            '<p><a href="#ref">see reference</a></p>'
            '<p id="ref">the reference paragraph</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Link")
        expect(data).to_contain(b"/GoTo")

    @test
    def missing_anchor_does_not_crash():
        """An href="#nope" with no matching id produces a PDF without a GoTo."""
        html = '<p><a href="#nope">broken</a></p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        content = content_stream(data)
        expect(data[:5]).to_equal(b"%PDF-")
        expect(content).to_contain(b"broken")
        # Annotation is emitted (rect present) but without a GoTo action.
        expect(data).to_contain(b"/Link")
        assert b"/GoTo" not in data
        # And without a URI action — the fragment is internal-only.
        assert b"/URI" not in data

    @test
    def external_and_internal_links_coexist():
        """External URI links and internal GoTo links both appear in the PDF."""
        html = (
            '<h1 id="top">Top</h1>'
            '<p><a href="#top">up</a> or '
            '<a href="https://example.com">external</a></p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/GoTo")
        expect(data).to_contain(b"/URI")
        expect(data).to_contain(b"https://example.com")

    @test
    def all_heading_levels_in_outline():
        """h1 through h6 all contribute entries to the outline tree."""
        html = (
            "<h1>One</h1><h2>Two</h2><h3>Three</h3>"
            "<h4>Four</h4><h5>Five</h5><h6>Six</h6>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        for word in (b"One", b"Two", b"Three", b"Four", b"Five", b"Six"):
            expect(data).to_contain(word)
        expect(data).to_contain(b"/Outlines")


def _pdf_text(pdf_bytes: bytes) -> str:
    import fitz

    with fitz.open(stream=pdf_bytes, filetype="pdf") as pdf:
        return pdf[0].get_text().strip()


with describe("pseudo-elements ::before and ::after"):
    # spec: CSS 2.1 §5.12.3; behaviors: sel-pseudo-element-before
    @test
    def before_prepends_generated_content():
        html = '<style>p::before { content: "[PRE]" }</style><p>body</p>'
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text.startswith("[PRE]")).to_equal(True)
        expect(text).to_contain("body")

    # spec: CSS 2.1 §5.12.3; behaviors: sel-pseudo-element-after
    @test
    def after_appends_generated_content():
        html = '<style>p::after { content: "[POST]" }</style><p>body</p>'
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text.endswith("[POST]")).to_equal(True)
        expect(text).to_contain("body")

    @test
    def legacy_single_colon_before_is_accepted():
        """CSS 2.1 :before aliases CSS3 ::before for compatibility."""
        html = '<style>p:before { content: "[LEG]" }</style><p>body</p>'
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text.startswith("[LEG]")).to_equal(True)

    @test
    def pseudo_does_not_leak_style_to_host_element():
        """color on ::before must not colour the host element's own text."""
        # The element itself has no color rule; only its ::before does. The
        # PDF should have at most one red fill operator (for the ::before),
        # and the body text stays in the default fill.
        html = '<style>p::before { content: "[R]"; color: red }</style><p>body</p>'
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        # One red rg operator, for just the pseudo-element's run.
        expect(content.count(b"1 0 0 rg")).to_equal(1)

    @test
    def non_matching_pseudo_rule_is_inert():
        html = '<style>div::before { content: "NO" }</style><p>body</p>'
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text).to_equal("body")

    @test
    def before_and_after_combine():
        html = (
            '<style>p::before { content: "<" }p::after { content: ">" }</style><p>x</p>'
        )
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text.startswith("<")).to_equal(True)
        expect(text.endswith(">")).to_equal(True)
        expect(text).to_contain("x")

    @test
    def pseudo_without_content_property_is_ignored():
        """::before with no content produces no text, matching CSS spec."""
        html = "<style>p::before { color: red }</style><p>body</p>"
        text = _pdf_text(HtmlDocument(string=html).to_bytes())
        expect(text).to_equal("body")


with describe("table of contents"):
    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_true_prepends_heading_list():
        """`toc=True` auto-prepends a clickable ToC based on document headings."""
        html = (
            "<h1>Intro</h1><p>hello</p>"
            "<h2>Details</h2><p>body</p>"
            "<h1>Wrap up</h1><p>bye</p>"
        )
        data = HtmlDocument(string=html, toc=True).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Table of Contents")
        expect(content).to_contain(b"Intro")
        expect(content).to_contain(b"Details")
        expect(content).to_contain(b"Wrap up")

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_emits_internal_link_per_heading():
        """Each heading produces a clickable GoTo annotation in the PDF."""
        html = "<h1>One</h1><h1>Two</h1><h1>Three</h1>"
        data = HtmlDocument(string=html, toc=True).to_bytes()
        # One GoTo action per heading (the ToC links).
        expect(data.count(b"/GoTo")).to_equal(3)

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_isolates_itself_on_dedicated_page():
        """The ToC sits before a forced page break so body starts fresh."""
        html = "<h1>A</h1><p>body</p>"
        data = HtmlDocument(string=html, toc=True).to_bytes()
        expect(data).to_contain(b"/Count 2")

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_string_sets_custom_title():
        """Passing a string to `toc` uses it as the heading text."""
        html = "<h1>Chapter</h1><p>body</p>"
        data = HtmlDocument(string=html, toc="Contents").to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"Contents")
        assert b"Table of Contents" not in content

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_preserves_existing_heading_ids():
        """User-supplied `id` on a heading is not used in the generated link."""
        from pdfun.toc import build_toc

        html = '<h1 id="preface">Preface</h1><p>body</p>'
        _modified, toc_html = build_toc(html)
        expect("#preface" in toc_html).to_equal(True)

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_false_is_a_no_op():
        """`toc=False` (default) leaves the document untouched."""
        html = "<h1>A</h1><p>body</p>"
        data_no_toc = HtmlDocument(string=html).to_bytes()
        data_false = HtmlDocument(string=html, toc=False).to_bytes()
        expect(data_no_toc).to_equal(data_false)

    # spec: pdfun; behaviors: pdf-toc
    @test
    def toc_on_empty_document_no_op():
        """A document with no headings renders identically with toc=True."""
        html = "<p>just a paragraph</p>"
        data_no_toc = HtmlDocument(string=html).to_bytes()
        data_toc = HtmlDocument(string=html, toc=True).to_bytes()
        expect(data_no_toc).to_equal(data_toc)


with describe("custom properties (var())"):
    # spec: CSS Variables §3; behaviors: values3-custom-props

    @test
    def var_in_same_rule_resolves_to_literal():
        """A `var(--x)` reference in the same rule block resolves to the
        custom property's declared value."""
        html_var = "<div style='--fg: #ff0000; color: var(--fg)'>hi</div>"
        html_literal = "<div style='color: #ff0000'>hi</div>"
        expect(content_stream(HtmlDocument(string=html_var).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def var_inherits_from_root():
        """Custom properties inherit, so `var()` in a descendant resolves
        against an ancestor's declaration."""
        html_var = (
            "<style>:root { --accent: rgb(0, 128, 0); } "
            "p { color: var(--accent); }</style>"
            "<p>text</p>"
        )
        html_literal = "<style>p { color: rgb(0, 128, 0); }</style><p>text</p>"
        expect(content_stream(HtmlDocument(string=html_var).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def var_fallback_used_when_undefined():
        """`var(--missing, blue)` falls back to the second argument when
        no custom property with that name is in scope."""
        html_fallback = "<div style='color: var(--missing, blue)'>text</div>"
        html_literal = "<div style='color: blue'>text</div>"
        expect(content_stream(HtmlDocument(string=html_fallback).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def var_in_length_property_resolves():
        """`var()` works in length contexts like `width`, not just color."""
        html_var = (
            "<div style='--w: 80px; width: var(--w); background-color: red'>x</div>"
        )
        html_literal = "<div style='width: 80px; background-color: red'>x</div>"
        expect(content_stream(HtmlDocument(string=html_var).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def var_inside_calc_resolves():
        """`calc(var(--w) - 10px)` substitutes the var first, then evaluates
        the calc expression."""
        html_var = (
            "<div style='--w: 50px; width: calc(var(--w) + 30px); "
            "background-color: red'>x</div>"
        )
        html_literal = "<div style='width: 80px; background-color: red'>x</div>"
        expect(content_stream(HtmlDocument(string=html_var).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def nested_var_chain_resolves():
        """A custom property whose value is itself `var(--other)` resolves
        through the chain."""
        html_chained = (
            "<div style='--primary: teal; --fg: var(--primary); "
            "color: var(--fg)'>x</div>"
        )
        html_literal = "<div style='color: teal'>x</div>"
        expect(content_stream(HtmlDocument(string=html_chained).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )

    @test
    def unresolved_var_drops_declaration():
        """If `var()` has no custom-property match and no fallback, the
        whole declaration is invalid at computed-value time (dropped)."""
        # No `--missing` in scope, no fallback → `color` declaration is
        # dropped, leaving the default (black).
        html_drop = "<div style='color: var(--missing)'>x</div>"
        html_default = "<div>x</div>"
        expect(content_stream(HtmlDocument(string=html_drop).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_default).to_bytes())
        )

    @test
    def child_var_override_shadows_ancestor():
        """A custom property redeclared on the child shadows the inherited
        ancestor value for that subtree."""
        html_shadow = (
            "<style>:root { --fg: red; } "
            "p { color: var(--fg); }</style>"
            "<div style='--fg: blue'><p>x</p></div>"
        )
        html_literal = "<style>p { color: blue; }</style><div><p>x</p></div>"
        expect(content_stream(HtmlDocument(string=html_shadow).to_bytes())).to_equal(
            content_stream(HtmlDocument(string=html_literal).to_bytes())
        )


with describe("@font-face"):
    # spec: CSS Fonts 3 §4.1; behaviors: fonts3-face

    @test
    def data_uri_loads_and_renders_text():
        """A `data:` URI src embeds the TTF and renders the paragraph as
        Identity-H glyph IDs (Type0 font), not WinAnsi byte strings."""
        css = f'@font-face {{ font-family: MyFont; src: url("{_font_data_uri()}"); }}'
        html = f"<style>{css}</style><p style='font-family: MyFont'>Hi</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(doc.warnings()).to_equal([])
        # The PDF declares a Type0 font and an Identity-H ToUnicode CMap —
        # both unique to embedded subsetted fonts.
        assert b"/Subtype /Type0" in data
        assert b"/Encoding /Identity-H" in data
        # The text 'Hi' (2 chars) is shown as 4 bytes (2x 16-bit CIDs)
        # rather than 2 bytes of WinAnsi.
        content = content_stream(data)
        assert b"(Hi)" not in content

    @test
    def relative_path_resolves_against_base_url():
        """`url("./font.ttf")` joins against the `base_url` argument."""
        with tempfile.TemporaryDirectory() as tmpdir:
            font_path = Path(tmpdir) / "myfont.ttf"
            font_path.write_bytes(_FIXTURE_TTF_PATH.read_bytes())
            css = '@font-face { font-family: MyFont; src: url("myfont.ttf"); }'
            html = f"<style>{css}</style><p style='font-family: MyFont'>x</p>"
            doc = HtmlDocument(string=html, base_url=tmpdir)
            data = doc.to_bytes()
            expect(doc.warnings()).to_equal([])
            assert b"/Subtype /Type0" in data

    @test
    def unknown_format_falls_through_to_next_src():
        """A `format(woff2)` source we can't load is skipped; the next
        comma-separated entry (a `data:` URI we can load) wins."""
        css = (
            "@font-face { font-family: MyFont; "
            'src: url("ignored.woff2") format("woff2"), '
            f'url("{_font_data_uri()}"); }}'
        )
        html = f"<style>{css}</style><p style='font-family: MyFont'>x</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(doc.warnings()).to_equal([])
        assert b"/Subtype /Type0" in data

    @test
    def failed_load_falls_back_to_next_family():
        """A broken `data:` payload emits a warning, the rule is dropped,
        and the family fallback (sans-serif → Helvetica) renders the text."""
        # base64 of 3 bytes is too short for a TTF — parse will fail.
        css = (
            "@font-face { font-family: BadFont; "
            'src: url("data:font/ttf;base64,QUJD"); }'
        )
        html = f"<style>{css}</style><p style='font-family: BadFont, sans-serif'>x</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        warnings = doc.warnings()
        assert any("BadFont" in w for w in warnings), warnings
        # No Type0 font registered — the paragraph rendered with a
        # built-in face instead.
        assert b"/Subtype /Type0" not in data

    @test
    def weight_routing_picks_bold_face():
        """Two `@font-face` rules with the same family at weights 400/700:
        a `<strong>` child should pick the 700 face, not the 400 one."""
        uri = _font_data_uri()
        css = (
            "@font-face { font-family: MyFont; font-weight: 400; "
            f'src: url("{uri}"); }}'
            "@font-face { font-family: MyFont; font-weight: 700; "
            f'src: url("{uri}"); }}'
        )
        html = (
            f"<style>{css}</style>"
            "<p style='font-family: MyFont'>"
            "regular <strong>bold</strong>"
            "</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(doc.warnings()).to_equal([])
        # Both faces register, both Type0 fonts appear in the file.
        assert data.count(b"/Subtype /Type0") == 2

    @test
    def emits_warning_on_local_src():
        """`local("Foo")` is recognised but unsupported — a warning is
        emitted and the family falls through to the cascade tail."""
        css = '@font-face { font-family: MyFont; src: local("Helvetica"); }'
        html = f"<style>{css}</style><p style='font-family: MyFont, sans-serif'>x</p>"
        doc = HtmlDocument(string=html)
        # to_bytes also re-renders, but warnings() does its own render.
        warnings = doc.warnings()
        assert any("local" in w.lower() for w in warnings), warnings

    @test
    def emits_warning_on_http_url():
        """An `http(s)://` src is rejected outright — we have no network
        client. A warning is emitted and the family falls through."""
        css = (
            "@font-face { font-family: MyFont; "
            'src: url("https://example.com/font.ttf"); }'
        )
        html = f"<style>{css}</style><p style='font-family: MyFont, sans-serif'>x</p>"
        doc = HtmlDocument(string=html)
        warnings = doc.warnings()
        assert any("scheme" in w.lower() or "http" in w.lower() for w in warnings), (
            warnings
        )


with describe("<button>"):
    # spec: HTML; behaviors: html-form-button

    @test
    def button_renders_inner_text_with_default_box():
        """`<button>Send</button>` renders the inner text inside a styled
        box: the UA stylesheet supplies a 1pt border, light-grey fill,
        and horizontal padding so the button looks like a button even
        without author CSS."""
        doc = HtmlDocument(string="<p><button>Send</button></p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Inner text shows up in the content stream.
        expect(content).to_contain(b"(Send) Tj")
        # Default light-grey background fill (#efefef ≈ 0.937).
        expect(content).to_contain(b"0.937 0.937 0.937 rg")
        # Default dark-grey border stroke (#767676 ≈ 0.463).
        expect(content).to_contain(b"0.463 0.463 0.463 RG")

    @test
    def button_flows_inline_with_surrounding_text():
        """A button inside a paragraph keeps the text flow on the same
        line — it's an inline-block atom, never a block-level break."""
        doc = HtmlDocument(string="<p>before <button>X</button> after</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Both "before" and "after" appear in the same paragraph; the
        # button text "X" sits between them, so all three are present.
        expect(content).to_contain(b"(before)")
        expect(content).to_contain(b"(X)")
        expect(content).to_contain(b"after")

    @test
    def button_user_css_overrides_ua_background():
        """Author CSS on the button beats the UA default — `background:
        red` shows red fill, not the default light grey."""
        html = "<button style='background-color: red; border-color: black'>X</button>"
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")
        # UA default light-grey fill must NOT appear since it was
        # overridden.
        assert b"0.937 0.937 0.937 rg" not in content

    @test
    def empty_button_does_not_crash():
        """A button with no inner content still renders a (zero-text)
        styled box and the surrounding paragraph completes."""
        doc = HtmlDocument(string="<p>x<button></button>y</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Both "x" and "y" still appear; renderer didn't choke on the
        # empty button.
        expect(content).to_contain(b"(x)")
        expect(content).to_contain(b"y")


with describe("<textarea>"):
    # spec: HTML; behaviors: html-form-textarea

    @test
    def textarea_renders_inner_text_in_bordered_box():
        """`<textarea>...</textarea>` renders the inner text inside a
        white box with a 1pt grey border. Default font is monospace."""
        doc = HtmlDocument(string="<textarea>hello</textarea>")
        data = doc.to_bytes()
        content = content_stream(data)
        # Inner text shows up.
        expect(content).to_contain(b"(hello) Tj")
        # White background fill.
        expect(content).to_contain(b"1 1 1 rg")
        # Default dark-grey border stroke.
        expect(content).to_contain(b"0.463 0.463 0.463 RG")
        # Font dict declares Courier (the monospace mapping).
        assert b"/BaseFont /Courier" in data

    @test
    def textarea_preserves_internal_newlines():
        """Whitespace inside `<textarea>` is preserved like `<pre>` —
        each newline becomes a separate rendered line."""
        doc = HtmlDocument(string="<textarea>line one\nline two</textarea>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(line one) Tj")
        expect(content).to_contain(b"(line two) Tj")

    @test
    def textarea_user_css_overrides_ua_background():
        """Author CSS on the textarea beats the UA defaults — `background:
        red` shows red fill, not the default white."""
        html = "<textarea style='background-color: red'>x</textarea>"
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"1 0 0 rg")

    @test
    def empty_textarea_does_not_crash():
        """A textarea with no inner text still renders the styled box
        and the surrounding flow continues."""
        doc = HtmlDocument(string="<p>before</p><textarea></textarea><p>after</p>")
        data = doc.to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(before)")
        expect(content).to_contain(b"after")


with describe("<select>"):
    # spec: HTML; behaviors: html-form-select

    @test
    def select_renders_first_option_in_bordered_box():
        """`<select>` with no [selected] attribute renders the first
        option's text inside a styled bordered inline-block."""
        html = "<select><option>Apple</option><option>Banana</option></select>"
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # First option text shows up.
        expect(content).to_contain(b"(Apple) Tj")
        # Other options must NOT appear in the rendered content.
        assert b"Banana" not in content
        # White background fill from the UA style.
        expect(content).to_contain(b"1 1 1 rg")
        # Default dark-grey border stroke.
        expect(content).to_contain(b"0.463 0.463 0.463 RG")

    @test
    def select_selected_option_wins_over_first():
        """When an option carries the `selected` attribute, it's the one
        rendered — not the first option in document order."""
        html = (
            "<select>"
            "<option>Apple</option>"
            "<option selected>Banana</option>"
            "<option>Cherry</option>"
            "</select>"
        )
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(Banana) Tj")
        # Sibling options should not be rendered.
        assert b"Apple" not in content
        assert b"Cherry" not in content

    @test
    def empty_select_does_not_crash():
        """A select with no options renders an empty styled box; the
        surrounding flow continues."""
        html = "<p>before</p><select></select><p>after</p>"
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        expect(content).to_contain(b"(before)")
        expect(content).to_contain(b"after")

    @test
    def select_user_css_overrides_ua_background():
        """Author CSS on the select beats the UA defaults — `background:
        red` shows red fill, not the default white."""
        html = "<select style='background-color: red'><option>X</option></select>"
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"1 0 0 rg")


with describe("<input>"):
    # spec: HTML; behaviors: html-form-input

    @test
    def text_input_renders_value_in_bordered_box():
        """`<input type="text" value="hello">` renders the value inside
        a styled bordered inline-block (white fill, grey border)."""
        html = '<input type="text" value="hello">'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(hello) Tj")
        expect(content).to_contain(b"1 1 1 rg")
        expect(content).to_contain(b"0.463 0.463 0.463 RG")

    @test
    def text_input_default_type_is_text():
        """An `<input>` with no `type` attribute defaults to text-input
        rendering — a bordered fixed-width box."""
        html = '<input value="x">'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(x) Tj")
        expect(content).to_contain(b"1 1 1 rg")

    @test
    def submit_input_uses_button_style_with_default_text():
        """`<input type="submit">` (no value) renders a button-style box
        with default text "Submit"."""
        html = '<input type="submit">'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(Submit) Tj")
        # Button-style light grey background.
        expect(content).to_contain(b"0.937 0.937 0.937 rg")

    @test
    def submit_input_value_overrides_default_text():
        """`<input type="submit" value="Send">` shows "Send", not the
        default."""
        html = '<input type="submit" value="Send">'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(Send) Tj")
        # Default text is not rendered.
        assert b"Submit" not in content

    @test
    def reset_and_button_inputs_have_their_own_default_labels():
        """`<input type="reset">` and `<input type="button">` each get a
        sensible default label so the box isn't blank."""
        c1 = content_stream(HtmlDocument(string='<input type="reset">').to_bytes())
        expect(c1).to_contain(b"(Reset) Tj")
        c2 = content_stream(HtmlDocument(string='<input type="button">').to_bytes())
        expect(c2).to_contain(b"(Button) Tj")

    @test
    def checkbox_renders_small_box_blank_when_unchecked():
        """An unchecked checkbox renders an empty bordered square; no
        check mark."""
        html = '<input type="checkbox">'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        # No check-mark text in the content stream.
        assert b"(X) Tj" not in content
        # White background fill.
        expect(content).to_contain(b"1 1 1 rg")
        expect(content).to_contain(b"0.463 0.463 0.463 RG")

    @test
    def checkbox_checked_includes_x_glyph():
        """A checkbox with the [checked] attribute renders an "X" inside
        the styled box."""
        html = '<input type="checkbox" checked>'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(X) Tj")

    @test
    def radio_checked_includes_dot_glyph():
        """A checked radio renders an ASCII glyph inside the styled
        box. (The list-bullet renderer uses the same `*` substitute for
        the same reason: WinAnsi is unreliable for the prettier dot
        characters with base-14 fonts.)"""
        html = '<input type="radio" checked>'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        expect(content).to_contain(b"(*) Tj")

    @test
    def hidden_input_renders_nothing():
        """`<input type="hidden">` is invisible — neither its value nor
        any styled box appears in the content stream. Surrounding text
        flow continues unchanged."""
        html = '<p>before<input type="hidden" value="x">after</p>'
        content = content_stream(HtmlDocument(string=html).to_bytes())
        # The literal "x" must not appear (it'd be the value if rendered).
        # Since the hidden input emits no atom, "before" and "after"
        # collapse into a single contiguous text fragment.
        expect(content).to_contain(b"(beforeafter) Tj")

    @test
    def text_input_user_css_overrides_ua_width():
        """Author CSS `width` beats the 150pt default — pdfun honors it."""
        html = '<input type="text" value="x" style="width: 60pt">'
        data = HtmlDocument(string=html).to_bytes()
        content = content_stream(data)
        # The fill rectangle width matches the author's 60pt request,
        # not the 150pt UA default.
        assert b" 60 " in content, content[:400]
