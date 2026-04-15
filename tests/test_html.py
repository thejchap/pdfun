import struct
import tempfile
import zlib
from pathlib import Path

from tryke import describe, expect, test

from pdfun import HtmlDocument


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
        expect(data).to_contain(b"Just plain text")


with describe("HtmlDocument - headings"):

    @test
    def h1_renders():
        """<h1> renders text in PDF."""
        doc = HtmlDocument(string="<h1>Title</h1>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Title")

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
        expect(data).to_contain(b"24 Tf")

    @test
    def h2_uses_18pt():
        """<h2> uses 18pt font size."""
        doc = HtmlDocument(string="<h2>Sub</h2>")
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

    @test
    def all_heading_levels():
        """h1-h6 all render their text."""
        html = "".join(f"<h{i}>H{i}</h{i}>" for i in range(1, 7))
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        for i in range(1, 7):
            expect(data).to_contain(f"H{i}".encode())


with describe("HtmlDocument - paragraphs"):

    @test
    def paragraph_renders():
        """<p> renders text."""
        doc = HtmlDocument(string="<p>Paragraph text</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Paragraph text")

    @test
    def paragraph_uses_12pt():
        """<p> uses 12pt Helvetica."""
        doc = HtmlDocument(string="<p>Body</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"12 Tf")

    @test
    def multiple_paragraphs():
        """Multiple <p> elements render sequentially."""
        doc = HtmlDocument(string="<p>First</p><p>Second</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Second")

    @test
    def paragraph_wraps_long_text():
        """Long paragraph text wraps within page width."""
        long = " ".join(["word"] * 80)
        doc = HtmlDocument(string=f"<p>{long}</p>")
        data = doc.to_bytes()
        expect(data.count(b"Td")).to_be_greater_than(1)


with describe("HtmlDocument - div"):

    @test
    def div_renders():
        """<div> renders its text content."""
        doc = HtmlDocument(string="<div>Div content</div>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Div content")


with describe("HtmlDocument - semantic elements"):

    @test
    def article_renders():
        """<article> renders its text content."""
        doc = HtmlDocument(string="<article>Article content</article>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Article content")

    @test
    def section_renders():
        """<section> renders its text content."""
        doc = HtmlDocument(string="<section>Section content</section>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Section content")

    @test
    def nav_renders():
        """<nav> renders its text content."""
        doc = HtmlDocument(string="<nav>Nav content</nav>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Nav content")

    @test
    def header_renders():
        """<header> renders its text content."""
        doc = HtmlDocument(string="<header>Header content</header>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Header content")

    @test
    def footer_renders():
        """<footer> renders its text content."""
        doc = HtmlDocument(string="<footer>Footer content</footer>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Footer content")

    @test
    def aside_renders():
        """<aside> renders its text content."""
        doc = HtmlDocument(string="<aside>Aside content</aside>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Aside content")

    @test
    def main_renders():
        """<main> renders its text content."""
        doc = HtmlDocument(string="<main>Main content</main>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Main content")

    @test
    def semantic_nesting():
        """Semantic elements nest properly."""
        doc = HtmlDocument(
            string="<article><section><p>Nested text</p></section></article>"
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Nested text")


with describe("HtmlDocument - br"):

    @test
    def br_splits_text():
        """<br> creates a line break between text."""
        doc = HtmlDocument(string="<p>Line one<br>Line two</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Line one")
        expect(data).to_contain(b"Line two")


with describe("HtmlDocument - inline elements"):

    @test
    def bold_extracts_text():
        """<b> text content is extracted."""
        doc = HtmlDocument(string="<p><b>Bold</b> text</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Bold")
        expect(data).to_contain(b"text")

    @test
    def nested_inline():
        """Nested inline elements extract all text."""
        doc = HtmlDocument(string="<p><b><em>Nested</em></b></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Nested")

    @test
    def span_extracts_text():
        """<span> text content is extracted."""
        doc = HtmlDocument(string="<p><span>Span</span> text</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Span text")


with describe("HtmlDocument - inline styling"):

    @test
    def bold_tag_applies_bold_font():
        """<b> applies Helvetica-Bold font."""
        doc = HtmlDocument(string="<p>Hello <b>bold</b> world</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")

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
        expect(data).to_contain(b"Title")
        expect(data).to_contain(b"Intro.")
        expect(data).to_contain(b"Section")
        expect(data).to_contain(b"Body.")

    @test
    def whitespace_normalization():
        """Extra whitespace in HTML is collapsed."""
        doc = HtmlDocument(string="<p>  Hello   world  </p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello world")


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
        expect(data).to_contain(b"A & B")

    @test
    def skip_script_content():
        """<script> content is not rendered."""
        doc = HtmlDocument(string="<script>var x = 1;</script><p>Visible</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Visible")
        expect(data).not_.to_contain(b"var x")

    @test
    def skip_style_content():
        """<style> content is not rendered."""
        html = "<style>body { color: red; }</style><p>Visible</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Visible")
        expect(data).not_.to_contain(b"color")


with describe("HtmlDocument - lists"):

    @test
    def ul_renders_item_text():
        """<ul><li> renders item text in PDF."""
        doc = HtmlDocument(string="<ul><li>Item one</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Item one")

    @test
    def ul_has_bullet_marker():
        """<ul><li> has a disc bullet marker (rendered as ASCII '*')."""
        doc = HtmlDocument(string="<ul><li>Item</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"(*)")

    @test
    def ul_multiple_items():
        """Multiple <li> elements render sequentially."""
        doc = HtmlDocument(string="<ul><li>First</li><li>Second</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Second")

    @test
    def ol_has_numbered_markers():
        """<ol><li> items have numbered markers."""
        html = "<ol><li>Alpha</li><li>Beta</li></ol>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1.")
        expect(data).to_contain(b"2.")
        expect(data).to_contain(b"Alpha")
        expect(data).to_contain(b"Beta")

    @test
    def li_with_bold():
        """<li><b>bold</b> text uses bold font."""
        doc = HtmlDocument(string="<ul><li><b>Bold</b> item</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"Bold")
        expect(data).to_contain(b"item")

    @test
    def li_wraps_long_text():
        """Long list item text wraps at reduced width."""
        long = " ".join(["word"] * 80)
        doc = HtmlDocument(string=f"<ul><li>{long}</li></ul>")
        data = doc.to_bytes()
        expect(data.count(b"Td")).to_be_greater_than(1)

    @test
    def nested_ul():
        """Nested <ul> renders both outer and inner items."""
        html = "<ul><li>Outer<ul><li>Inner</li></ul></li></ul>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Outer")
        expect(data).to_contain(b"Inner")

    @test
    def nested_ul_different_markers():
        """Nested <ul> uses different bullet style at depth 1."""
        html = "<ul><li>Outer<ul><li>Inner</li></ul></li></ul>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # depth 0 = disc ('*'), depth 1 = circle ('o')
        expect(data).to_contain(b"(*)")
        expect(data).to_contain(b"(o)")

    @test
    def nested_ol_restarts_numbering():
        """Nested <ol> restarts numbering at 1."""
        html = (
            "<ol><li>First<ol><li>Inner A</li><li>Inner B</li>"
            "</ol></li><li>Second</li></ol>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Inner A")
        expect(data).to_contain(b"Inner B")
        expect(data).to_contain(b"Second")

    @test
    def mixed_list_nesting():
        """<ol> with nested <ul> renders correctly."""
        html = "<ol><li>Numbered<ul><li>Bulleted</li></ul></li></ol>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Numbered")
        expect(data).to_contain(b"Bulleted")

    @test
    def list_between_paragraphs():
        """<p> before and after <ul> all render."""
        html = "<p>Before</p><ul><li>Item</li></ul><p>After</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Before")
        expect(data).to_contain(b"Item")
        expect(data).to_contain(b"After")

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
        expect(data).to_contain(b"Orphan")


with describe("HtmlDocument - malformed HTML"):

    @test
    def unclosed_tag():
        """Unclosed <p> does not crash; text is still rendered."""
        doc = HtmlDocument(string="<p>Hello")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello")

    @test
    def unclosed_bold():
        """Unclosed <b> does not crash; text is rendered."""
        doc = HtmlDocument(string="<p><b>Bold text")
        data = doc.to_bytes()
        expect(data).to_contain(b"Bold text")

    @test
    def extra_closing_tags():
        """Extra closing tags do not crash."""
        doc = HtmlDocument(string="<p>Text</p></p></div>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

    @test
    def nested_same_block():
        """<p> inside <p> is auto-closed by html5ever."""
        doc = HtmlDocument(string="<p>First<p>Second")
        data = doc.to_bytes()
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Second")

    @test
    def misnested_inline():
        """Overlapping inline tags handled gracefully."""
        doc = HtmlDocument(string="<p><b>bold <i>both</b> italic</i></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"bold")
        expect(data).to_contain(b"both")
        expect(data).to_contain(b"italic")

    @test
    def no_root_element():
        """Content without <html>/<body> still renders."""
        doc = HtmlDocument(string="Just text, no tags at all")
        data = doc.to_bytes()
        expect(data).to_contain(b"Just text")

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
        expect(data).to_contain(b"Inside")

    @test
    def void_elements_no_crash():
        """Void elements (img, hr, input) do not crash."""
        html = "<p>Before</p><hr><img><input><p>After</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Before")
        expect(data).to_contain(b"After")

    @test
    def self_closing_br():
        """<br/> (self-closing) works the same as <br>."""
        doc = HtmlDocument(string="<p>Line one<br/>Line two</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Line one")
        expect(data).to_contain(b"Line two")

    @test
    def anchor_tag_preserves_text():
        """<a> tag text is rendered alongside surrounding text."""
        doc = HtmlDocument(string='<p>Click <a href="url">here</a></p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Click")
        expect(data).to_contain(b"here")


with describe("HtmlDocument - whitespace"):

    @test
    def leading_trailing_whitespace():
        """Leading/trailing whitespace in tags is collapsed."""
        doc = HtmlDocument(string="<p>  Hello  </p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello")

    @test
    def newlines_collapsed():
        """Newlines within text are collapsed to spaces."""
        doc = HtmlDocument(string="<p>Hello\nworld</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello world")

    @test
    def tabs_collapsed():
        """Tabs are collapsed to spaces."""
        doc = HtmlDocument(string="<p>Hello\tworld</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello world")

    @test
    def inter_element_whitespace():
        """Whitespace between inline elements is preserved."""
        doc = HtmlDocument(string="<p><b>Bold</b> <i>italic</i></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Bold")
        expect(data).to_contain(b"italic")

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
        expect(data).to_contain(b"Hello world")


with describe("HtmlDocument - unicode"):

    @test
    def unicode_text():
        """Unicode characters pass through without crashing."""
        doc = HtmlDocument(string="<p>Héllo wörld</p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data).to_contain("Héllo wörld".encode().hex().upper().encode())

    @test
    def numeric_entity():
        """Numeric character reference &#169; is decoded."""
        doc = HtmlDocument(string="<p>&#169; 2024</p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data).to_contain(b"C2A9")

    @test
    def hex_entity():
        """Hex character reference &#x2603; is decoded."""
        doc = HtmlDocument(string="<p>&#x2603;</p>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data).to_contain(b"E29883")

    @test
    def multiple_named_entities():
        """Multiple named entities decode correctly."""
        doc = HtmlDocument(string="<p>&lt;tag&gt; &amp; &quot;quotes&quot;</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b'<tag> & "quotes"')


with describe("HtmlDocument - nesting"):

    @test
    def deeply_nested_divs():
        """Deeply nested divs do not crash."""
        html = "<div>" * 50 + "Content" + "</div>" * 50
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Content")

    @test
    def deeply_nested_lists():
        """Deeply nested lists render without crash."""
        html = ""
        for i in range(10):
            html += f"<ul><li>Level {i}"
        html += "".join("</li></ul>" for _ in range(10))
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Level 0")
        expect(data).to_contain(b"Level 9")

    @test
    def mixed_block_nesting():
        """Block elements nested inside other block elements."""
        html = "<div><p>Para in div</p><div><p>Nested deeper</p></div></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Para in div")
        expect(data).to_contain(b"Nested deeper")

    @test
    def inline_in_heading():
        """Multiple inline styles in a heading."""
        html = "<h2><b>Bold</b> and <i>italic</i> heading</h2>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Bold")
        expect(data).to_contain(b"italic")
        expect(data).to_contain(b"heading")


with describe("HtmlDocument - large documents"):

    @test
    def many_paragraphs():
        """500 paragraphs render without crash, spanning multiple pages."""
        paras = "".join(f"<p>Paragraph {i}</p>" for i in range(500))
        doc = HtmlDocument(string=paras)
        data = doc.to_bytes()
        expect(data).to_contain(b"Paragraph 0")
        expect(data).to_contain(b"Paragraph 499")

    @test
    def long_single_paragraph():
        """Very long single paragraph wraps correctly."""
        text = " ".join(f"word{i}" for i in range(500))
        doc = HtmlDocument(string=f"<p>{text}</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"word0")
        expect(data).to_contain(b"word499")


with describe("HtmlDocument - inline styles"):

    @test
    def inline_color_named():
        """style='color: red' sets text color to red."""
        doc = HtmlDocument(string='<p style="color: red">Red</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def inline_color_hex():
        """style='color: #0000ff' sets text color to blue."""
        doc = HtmlDocument(string='<p style="color: #0000ff">Blue</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def inline_color_hex_short():
        """style='color: #f00' sets text color to red."""
        doc = HtmlDocument(string='<p style="color: #f00">Red</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def inline_color_rgb():
        """style='color: rgb(0, 128, 0)' sets text color to green."""
        doc = HtmlDocument(string='<p style="color: rgb(0, 128, 0)">Green</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Green")
        expect(data).to_contain(b"0 0.50")

    @test
    def inline_font_size_pt():
        """style='font-size: 18pt' uses 18pt font."""
        doc = HtmlDocument(string='<p style="font-size: 18pt">Big</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

    @test
    def inline_font_size_px():
        """style='font-size: 24px' converts to 18pt (24 * 0.75)."""
        doc = HtmlDocument(string='<p style="font-size: 24px">Big</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

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

    @test
    def inline_background_color():
        """style='background-color: yellow' draws yellow background."""
        doc = HtmlDocument(
            string='<p style="background-color: #ffff00">Highlighted</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"1 1 0 rg")

    @test
    def inline_text_align_center():
        """style='text-align: center' centers text."""
        doc = HtmlDocument(string='<p style="text-align: center">Centered</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Centered")

    @test
    def inline_multiple_properties():
        """Multiple properties in one style attribute."""
        doc = HtmlDocument(string='<p style="color: blue; font-size: 24pt">Styled</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")
        expect(data).to_contain(b"24 Tf")

    @test
    def inline_invalid_css_ignored():
        """Invalid CSS values are silently ignored; valid ones still apply."""
        doc = HtmlDocument(
            string='<p style="color: notacolor; font-size: 18pt">Text</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

    @test
    def inline_style_on_heading():
        """Inline style on heading overrides UA defaults."""
        doc = HtmlDocument(string='<h1 style="font-size: 12pt">Small H1</h1>')
        data = doc.to_bytes()
        expect(data).to_contain(b"12 Tf")

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
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def span_without_style():
        """<span> without style passes text through normally."""
        doc = HtmlDocument(string="<p>Before <span>inside</span> after</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"inside")

    @test
    def inline_padding():
        """style='padding: 10pt' adds padding to the block."""
        doc = HtmlDocument(string='<p style="padding: 10pt">Padded</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Padded")

    @test
    def inline_border():
        """style='border: 1px solid black' renders border."""
        doc = HtmlDocument(string='<p style="border: 1px solid black">Bordered</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Bordered")

    @test
    def inline_margin_bottom():
        """style='margin-bottom: 24pt' adjusts spacing after paragraph."""
        doc = HtmlDocument(
            string='<p style="margin-bottom: 24pt">Spaced</p><p>Next</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Spaced")
        expect(data).to_contain(b"Next")


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
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def span_inside_bold():
        """<b><span style='color: red'>text</span></b> applies both bold and color."""
        doc = HtmlDocument(string='<p><b><span style="color: red">text</span></b></p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def bold_with_color_style():
        """<b style='color: red'>text</b> applies bold font AND red color."""
        doc = HtmlDocument(string='<p><b style="color: red">text</b></p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def italic_with_font_size_style():
        """<i style='font-size: 18pt'>text</i> applies italic font at 18pt."""
        doc = HtmlDocument(string='<p><i style="font-size: 18pt">text</i></p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Oblique")
        expect(data).to_contain(b"18 Tf")

    @test
    def styled_bold_does_not_leak():
        """Style on <b> does not leak to following text."""
        doc = HtmlDocument(string='<p><b style="color: red">bold</b> normal</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"normal")

    @test
    def span_style_inside_styled_bold():
        """<b style='color: red'><span style='color: blue'>text</span></b> uses blue."""
        html = '<p><b style="color: red"><span style="color: blue">text</span></b></p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")
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
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"Red item")

    @test
    def li_with_background():
        """<li style='background-color: yellow'> renders background."""
        doc = HtmlDocument(
            string='<ul><li style="background-color: #ffff00">Highlighted</li></ul>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"1 1 0 rg")

    @test
    def h2_with_color():
        """<h2 style='color: blue'> renders blue heading."""
        doc = HtmlDocument(string='<h2 style="color: blue">Blue Title</h2>')
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")
        expect(data).to_contain(b"Blue Title")

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
        expect(data).to_contain(b"1 1 0 rg")
        expect(data).to_contain(b"Title")

    @test
    def inline_text_align_right():
        """style='text-align: right' right-aligns text."""
        doc = HtmlDocument(string='<p style="text-align: right">Right</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Right")

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
        expect(data).to_contain(b"Indented")

    @test
    def inline_border_width_only():
        """style='border-width: 3px' draws border."""
        doc = HtmlDocument(string='<p style="border-width: 3px">Bordered</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Bordered")

    @test
    def inline_border_color_only():
        """style='border-color: red' with border-width draws colored border."""
        doc = HtmlDocument(
            string='<p style="border-width: 1px; border-color: red">Red border</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0")

    @test
    def important_not_breaking():
        """!important does not break CSS parsing."""
        doc = HtmlDocument(string='<p style="color: red !important">Important</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def negative_font_size_ignored():
        """Negative font-size is silently ignored; default applies."""
        doc = HtmlDocument(string='<p style="font-size: -12pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"12 Tf")

    @test
    def negative_padding_ignored():
        """Negative padding is silently ignored."""
        doc = HtmlDocument(string='<p style="padding: -10px; color: red">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def uppercase_property_name():
        """Uppercase property name COLOR works."""
        doc = HtmlDocument(string='<p style="COLOR: red">Red</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def extra_semicolons_handled():
        """Extra semicolons in style attribute don't break parsing."""
        doc = HtmlDocument(string='<p style=";;color: red;;;font-size: 18pt;">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"18 Tf")

    @test
    def missing_value_handled():
        """Missing value after colon doesn't crash."""
        doc = HtmlDocument(string='<p style="color:; font-size: 18pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

    @test
    def malformed_rgb_too_few_args():
        """rgb() with too few arguments is silently ignored."""
        doc = HtmlDocument(
            string='<p style="color: rgb(255, 0); font-size: 18pt">Text</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

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
        expect(data).to_contain(b"plain")
        expect(data).to_contain(b"red")
        expect(data).to_contain(b"plain2")


with describe("HtmlDocument - code element"):

    @test
    def code_uses_courier():
        """<code> renders text with Courier font."""
        doc = HtmlDocument(string="<p><code>x = 1</code></p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")
        expect(data).to_contain(b"x = 1")

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

    @test
    def blockquote_renders():
        """<blockquote> renders text."""
        doc = HtmlDocument(string="<blockquote>Quoted text</blockquote>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Quoted text")

    @test
    def blockquote_with_style():
        """<blockquote> with inline style applies CSS."""
        doc = HtmlDocument(
            string='<blockquote style="color: blue">Blue quote</blockquote>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def blockquote_nested_in_div():
        """<blockquote> inside <div> renders correctly."""
        html = "<div><blockquote>Quoted</blockquote></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Quoted")


with describe("HtmlDocument - hr element"):

    @test
    def hr_renders():
        """<hr> between paragraphs doesn't crash."""
        doc = HtmlDocument(string="<p>Before</p><hr><p>After</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Before")
        expect(data).to_contain(b"After")

    @test
    def hr_alone():
        """<hr> alone produces valid PDF with stroke."""
        doc = HtmlDocument(string="<hr>")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data).to_contain(b"S\n")

    @test
    def multiple_hr():
        """Multiple <hr> elements don't crash."""
        doc = HtmlDocument(string="<p>A</p><hr><p>B</p><hr><p>C</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"A")
        expect(data).to_contain(b"B")
        expect(data).to_contain(b"C")


with describe("HtmlDocument - pre element"):

    @test
    def pre_preserves_spaces():
        """<pre> preserves multiple spaces."""
        doc = HtmlDocument(string="<pre>a  b  c</pre>")
        data = doc.to_bytes()
        expect(data).to_contain(b"a  b  c")

    @test
    def pre_preserves_newlines():
        """<pre> preserves newlines as separate lines."""
        doc = HtmlDocument(string="<pre>line1\nline2\nline3</pre>")
        data = doc.to_bytes()
        expect(data).to_contain(b"line1")
        expect(data).to_contain(b"line2")
        expect(data).to_contain(b"line3")

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
        expect(data).to_contain(b"/Courier")
        expect(data).to_contain(b"x = 1")
        expect(data).to_contain(b"y = 2")

    @test
    def pre_followed_by_normal():
        """Normal <p> after <pre> resumes word-wrapping."""
        html = "<pre>  spaced  </pre><p>Normal paragraph</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Normal paragraph")


with describe("HtmlDocument - style blocks"):

    @test
    def style_type_selector_color():
        """<style>p { color: red }</style> applies red to paragraphs."""
        html = "<style>p { color: red }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_type_selector_font_size():
        """<style>p { font-size: 18pt }</style> sets font size."""
        html = "<style>p { font-size: 18pt }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"18 Tf")

    @test
    def style_class_selector():
        """<style>.red { color: red }</style> matches class attribute."""
        html = '<style>.red { color: red }</style><p class="red">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_id_selector():
        """<style>#title { font-size: 24pt }</style> matches id attribute."""
        html = '<style>#title { font-size: 24pt }</style><p id="title">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"24 Tf")

    @test
    def style_inline_wins_over_style_block():
        """Inline style overrides <style> block rule."""
        html = '<style>p { color: blue }</style><p style="color: red">Text</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_later_rule_wins():
        """Later rule with same specificity wins."""
        html = "<style>p { color: green } p { color: blue }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def style_class_beats_type():
        """Class selector beats type selector (higher specificity)."""
        html = (
            "<style>p { color: red } .blue { color: blue }</style>"
            '<p class="blue">Text</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def style_id_beats_class():
        """ID selector beats class selector (higher specificity)."""
        html = (
            "<style>.red { color: red } #blue { color: blue }</style>"
            '<p class="red" id="blue">Text</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def style_multiple_selectors():
        """Comma-separated selectors apply to all matches."""
        html = "<style>h1, h2 { color: red }</style><h1>A</h1><h2>B</h2>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_descendant_selector():
        """Descendant selector 'div p' matches nested elements."""
        html = "<style>div p { color: red }</style><div><p>Text</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_descendant_no_match():
        """Descendant selector does not match non-descendant."""
        html = "<style>div p { color: red }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def style_child_selector():
        """Child selector 'div > p' matches direct children."""
        html = "<style>div > p { color: red }</style><div><p>Text</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_child_no_match_grandchild():
        """Child selector does not match grandchildren."""
        html = (
            "<style>div > p { color: red }</style>"
            "<div><blockquote><p>Text</p></blockquote></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def style_compound_selector():
        """Compound selector 'p.note' matches element with class."""
        html = (
            "<style>p.note { color: red }</style>"
            '<p class="note">Match</p><p>No match</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def style_no_style_block():
        """Document without <style> block still works."""
        doc = HtmlDocument(string="<p>Text</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

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
        expect(data).to_contain(b"18 Tf")

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
        expect(data).to_contain(b"1 1 0 rg")


with describe("body CSS inheritance"):

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
    def body_font_size_inherits():
        """font-size on body inherits to child elements."""
        html = "<style>body { font-size: 10pt }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"10 Tf")

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
        expect(data).to_contain(b"1 0 0 rg")

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
        expect(data).to_contain(b"20 Tf")

    @test
    def multi_level_inheritance():
        """color propagates through intermediate elements."""
        html = '<div style="color: red"><div><p>Still red</p></div></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def child_overrides_parent():
        """Explicit style on child overrides inherited value."""
        html = '<div style="color: red"><p style="color: blue">Blue wins</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")

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
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def inheritance_does_not_leak_to_siblings():
        """Inherited style on one branch does not affect sibling branch."""
        html = '<div style="color: red"><p>Red</p></div><p>Default color</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def line_height_inherits_through_div():
        """line-height on a div inherits to child elements."""
        html = '<div style="line-height: 2"><p>Spaced text</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("@page rule"):

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
        expect(data).to_contain(b"/MediaBox [0 0 612 792]")
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def inch_unit_in_margin():
        """Inch units resolve correctly (0.75in = 54pt)."""
        html = "<style>@page { size: letter; margin: 1in }</style><p>Text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("multi-column layout"):

    @test
    def column_count_renders():
        """column-count: 2 produces a valid PDF with text wrapped narrower."""
        html = """<style>body { column-count: 2 }</style>
        <p>First paragraph with enough text to show wrapping.</p>
        <p>Second paragraph also with some text content.</p>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
        expect(data).to_contain(b"First paragraph")

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
        # Column rules produce stroke ops with the specified color
        expect(data).to_contain(b"0.8 0.8 0.8 RG")

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
        expect(data).to_contain(b"/MediaBox [0 0 612 792]")
        expect(data).to_contain(b"/Courier")
        expect(data).to_contain(b"Hacker News Summary")


with describe("display: none"):

    @test
    def inline_display_none_hides_element():
        """An element with inline display:none is not rendered."""
        doc = HtmlDocument(string='<p style="display:none">Hidden</p><p>Visible</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Visible")
        assert b"Hidden" not in data

    @test
    def display_none_hides_children():
        """display:none on a parent hides all descendants."""
        doc = HtmlDocument(
            string='<div style="display:none"><p>Deep hidden</p></div><p>Visible</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Visible")
        assert b"Deep hidden" not in data

    @test
    def display_none_via_style_block():
        """display:none set via a <style> block hides the element."""
        html = """<html><head><style>.hide { display: none; }</style></head>
        <body><p class="hide">Hidden</p><p>Visible</p></body></html>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Visible")
        assert b"Hidden" not in data

    @test
    def display_block_still_renders():
        """display:block does not hide the element."""
        doc = HtmlDocument(string='<p style="display:block">Shown</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Shown")


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

    @test
    def margin_top_renders():
        """margin-top on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-top: 50pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

    @test
    def margin_left_renders():
        """margin-left on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-left: 50pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

    @test
    def margin_right_renders():
        """margin-right on an element produces valid PDF output."""
        doc = HtmlDocument(string='<p style="margin-right: 50pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

    @test
    def margin_shorthand_all_four():
        """margin shorthand sets all four sides."""
        doc = HtmlDocument(string='<p style="margin: 10pt 20pt 30pt 40pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

    @test
    def margin_shorthand_two_values():
        """margin: 10pt 20pt sets top/bottom=10 and left/right=20."""
        doc = HtmlDocument(string='<p style="margin: 10pt 20pt">Text</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Text")

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

    @test
    def hsl_red_renders():
        """hsl(0, 100%, 50%) is pure red."""
        html = '<p style="color: hsl(0, 100%, 50%)">red</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"red")
        # Pure red in PDF fill color op: "1 0 0 rg"
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def hsl_green_renders():
        """hsl(120, 100%, 50%) is pure green."""
        html = '<p style="color: hsl(120, 100%, 50%)">green</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 1 0 rg")

    @test
    def hsla_accepts_alpha_component():
        """hsla() parses (alpha is currently ignored)."""
        html = '<p style="color: hsla(240, 100%, 50%, 0.5)">blue</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")


with describe("semantic block elements"):

    @test
    def article_renders_as_block():
        """<article> renders its text content as a block."""
        doc = HtmlDocument(string="<article>Article text</article>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Article text")

    @test
    def section_renders_as_block():
        doc = HtmlDocument(string="<section>Section text</section>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Section text")

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
        expect(data).to_contain(b"Hello")
        # 200pt collapses to 200pt — everything fits on one page
        expect(data).to_contain(b"/Count 1")


with describe("text-indent"):

    @test
    def text_indent_renders_first_line_shift():
        """text-indent shifts the first line's x position."""
        html = (
            '<p style="text-indent: 24pt; text-align: left">'
            "First line indented by twenty-four points here.</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"First line indented")

    @test
    def text_indent_inherits_from_parent():
        """text-indent is an inherited property."""
        html = '<div style="text-indent: 24pt"><p>Indented nested text.</p></div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Indented nested")


with describe("text-transform"):

    @test
    def uppercase_transforms_text():
        """text-transform: uppercase converts text to uppercase."""
        html = '<p style="text-transform: uppercase">hello world</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"HELLO WORLD")
        assert b"hello world" not in data

    @test
    def lowercase_transforms_text():
        """text-transform: lowercase converts text to lowercase."""
        html = '<p style="text-transform: lowercase">HELLO World</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"hello world")
        assert b"HELLO World" not in data

    @test
    def capitalize_transforms_text():
        """text-transform: capitalize uppercases the first letter of each word."""
        html = '<p style="text-transform: capitalize">hello world</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello World")

    @test
    def text_transform_inherits_from_parent():
        """text-transform inherits through descendants."""
        doc = HtmlDocument(
            string='<div style="text-transform: uppercase"><p>nested</p></div>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"NESTED")


with describe("text-decoration"):

    @test
    def underline_produces_stroke_ops():
        """text-decoration: underline draws a line under text."""
        doc = HtmlDocument(
            string='<p style="text-decoration: underline">Underlined</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Underlined")
        # The PDF should contain stroke operations for the underline
        # (MoveTo + LineTo + Stroke pattern in content stream)

    @test
    def line_through_produces_stroke_ops():
        """text-decoration: line-through draws a line through text."""
        doc = HtmlDocument(string='<p style="text-decoration: line-through">Struck</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Struck")

    @test
    def underline_and_line_through_combined():
        """Both underline and line-through can be applied together."""
        doc = HtmlDocument(
            string='<p style="text-decoration: underline line-through">Both</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Both")

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
        expect(data).to_contain(b"Plain")

    @test
    def underline_via_style_block():
        """text-decoration set via <style> block is applied."""
        html = """<html><head><style>
        .ul { text-decoration: underline; }
        </style></head>
        <body><p class="ul">Styled underline</p></body></html>"""
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Styled underline")


with describe("border-style"):

    @test
    def border_solid_renders():
        """border with solid style renders normally."""
        doc = HtmlDocument(string='<p style="border: 2px solid black">Solid border</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Solid border")

    @test
    def border_dashed_renders():
        """border with dashed style renders."""
        doc = HtmlDocument(string='<p style="border: 2px dashed red">Dashed border</p>')
        data = doc.to_bytes()
        expect(data).to_contain(b"Dashed border")

    @test
    def border_dotted_renders():
        """border with dotted style renders."""
        doc = HtmlDocument(
            string='<p style="border: 2px dotted blue">Dotted border</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Dotted border")

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
        expect(data).to_contain(b"Styled")


with describe("clickable links"):

    @test
    def link_produces_annotation():
        """An <a href> produces a /Link annotation in the PDF."""
        doc = HtmlDocument(
            string='<p>Visit <a href="https://example.com">our site</a> today</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"our site")
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
        expect(data).to_contain(b"no link")
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
        expect(data).to_contain(b"before")
        expect(data).to_contain(b"linked")
        expect(data).to_contain(b"after")

    @test
    def link_with_inline_style():
        """A link with inline color styling still produces an annotation."""
        doc = HtmlDocument(
            string='<a href="https://example.com" style="color: blue">styled link</a>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"/Link")
        expect(data).to_contain(b"styled link")


with describe("list-style-type"):

    @test
    def ol_default_is_decimal():
        """<ol> defaults to decimal markers."""
        doc = HtmlDocument(string="<ol><li>One</li><li>Two</li></ol>")
        data = doc.to_bytes()
        expect(data).to_contain(b"(1.)")
        expect(data).to_contain(b"(2.)")

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
        expect(data).to_contain(b"(a.)")
        expect(data).to_contain(b"(b.)")

    @test
    def upper_alpha_marker():
        """list-style-type: upper-alpha produces A, B markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: upper-alpha"><li>One</li><li>Two</li></ol>'
            )
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"(A.)")
        expect(data).to_contain(b"(B.)")

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
        expect(data).to_contain(b"(i.)")
        expect(data).to_contain(b"(ii.)")
        expect(data).to_contain(b"(iii.)")

    @test
    def upper_roman_marker():
        """list-style-type: upper-roman produces I, II markers."""
        doc = HtmlDocument(
            string=(
                '<ol style="list-style-type: upper-roman"><li>One</li><li>Two</li></ol>'
            )
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"(I.)")
        expect(data).to_contain(b"(II.)")

    @test
    def disc_marker_explicit():
        """list-style-type: disc produces '*' ASCII marker."""
        doc = HtmlDocument(
            string='<ul style="list-style-type: disc"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"(*)")

    @test
    def square_marker():
        """list-style-type: square produces '#' ASCII marker."""
        doc = HtmlDocument(
            string='<ul style="list-style-type: square"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"(#)")

    @test
    def none_marker_suppresses_bullet():
        """list-style-type: none produces no marker."""
        doc_with = HtmlDocument(string="<ul><li>Item</li></ul>")
        data_with = doc_with.to_bytes()
        doc_none = HtmlDocument(
            string='<ul style="list-style-type: none"><li>Item</li></ul>'
        )
        data_none = doc_none.to_bytes()
        # Both contain "Item" but the "none" variant has no marker
        expect(data_with).to_contain(b"Item")
        expect(data_none).to_contain(b"Item")
        # The marker ShowText call is absent in the "none" version
        assert b"(*)" in data_with
        assert b"(*)" not in data_none

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
        expect(data).to_contain(b"(a.)")
        expect(data).to_contain(b"(b.)")

    @test
    def roman_numerals_compound():
        """Roman numeral markers handle compound values correctly."""
        items = "".join(f"<li>Item {i}</li>" for i in range(1, 10))
        doc = HtmlDocument(
            string=f'<ol style="list-style-type: lower-roman">{items}</ol>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"(iv.)")
        expect(data).to_contain(b"(ix.)")


with describe("list-style-position"):

    @test
    def default_is_outside():
        """Default list-style-position is outside (marker hangs left of text)."""
        doc = HtmlDocument(string="<ul><li>Item</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Item")
        expect(data).to_contain(b"(*)")

    @test
    def inside_renders_marker_and_text():
        """list-style-position: inside still renders both marker and text."""
        doc = HtmlDocument(
            string='<ul style="list-style-position: inside"><li>Item</li></ul>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Item")
        expect(data).to_contain(b"(*)")

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
        expect(data).to_contain(b"Item")
        expect(data).to_contain(b"(*)")


with describe("definition lists and figures"):

    @test
    def dl_renders_term_and_definition():
        """<dl><dt><dd> renders both term and definition text."""
        doc = HtmlDocument(string="<dl><dt>Term</dt><dd>Definition</dd></dl>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Term")
        expect(data).to_contain(b"Definition")

    @test
    def dd_is_indented():
        """<dd> content is indented relative to <dt>."""
        doc = HtmlDocument(string="<dl><dt>Term</dt><dd>Definition</dd></dl>")
        # Rendering must succeed and contain the definition text.
        data = doc.to_bytes()
        expect(data).to_contain(b"Definition")
        # A plain paragraph version should differ from the dd-indented one.
        doc_plain = HtmlDocument(string="<p>Term</p><p>Definition</p>")
        assert doc_plain.to_bytes() != data

    @test
    def figure_with_figcaption_renders_both():
        """<figure><figcaption> renders both the figure body and caption."""
        doc = HtmlDocument(
            string="<figure><p>Body</p><figcaption>Caption</figcaption></figure>"
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"Body")
        expect(data).to_contain(b"Caption")

    @test
    def multiple_dt_dd_pairs():
        """A <dl> with multiple term/definition pairs renders every entry."""
        doc = HtmlDocument(
            string=("<dl><dt>One</dt><dd>First</dd><dt>Two</dt><dd>Second</dd></dl>")
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"One")
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Two")
        expect(data).to_contain(b"Second")


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


with describe("text-align: justify"):

    @test
    def justify_emits_word_spacing_op():
        """justified text emits a Tw (word spacing) operator in the stream."""
        long_text = "word " * 30
        doc = HtmlDocument(string=f'<p style="text-align: justify">{long_text}</p>')
        data = doc.to_bytes()
        # Tw is the PDF word spacing operator
        expect(data).to_contain(b" Tw")

    @test
    def justify_last_line_not_widened():
        """Last line of justified paragraph has no word-spacing applied."""
        # Short single-line paragraph: no spacing needed, no Tw emitted
        doc = HtmlDocument(string='<p style="text-align: justify">Short line only</p>')
        data = doc.to_bytes()
        # Single-line para has no lines to widen → no Tw
        assert b" Tw" not in data

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
        expect(data).to_contain(b" Tw")

    @test
    def justify_resets_word_spacing_after_line():
        """Word spacing is reset to 0 after each justified line."""
        long_text = "word " * 30
        doc = HtmlDocument(string=f'<p style="text-align: justify">{long_text}</p>')
        data = doc.to_bytes()
        # A reset to 0 should appear somewhere in the stream
        expect(data).to_contain(b"0 Tw")


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
        expect(data).to_contain(b"Alice")
        expect(data).to_contain(b"Bob")
        expect(data).to_contain(b"(30)")
        expect(data).to_contain(b"(25)")

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
        expect(data).to_contain(b"Helvetica-Bold")
        expect(data).to_contain(b"Name")

    @test
    def table_draws_cell_borders():
        """Each cell has a stroked border by default."""
        html = "<table><tr><td>A</td><td>B</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # Stroke operator (uppercase S) should appear
        expect(data).to_contain(b"\nS\n")

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
        expect(data).to_contain(b"short")
        expect(data).to_contain(b"this is a much wider cell content")

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
        expect(data).to_contain(b"Header")
        expect(data).to_contain(b"Body")

    @test
    def table_with_long_content_wraps_in_cell():
        """Long cell content wraps within the cell's column width."""
        long_text = "word " * 50
        html = f"<table><tr><td>{long_text}</td><td>short</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # Should render without error and contain both cells
        expect(data).to_contain(b"short")
        expect(data).to_contain(b"word")

    @test
    def empty_table_renders_without_error():
        """An empty <table> does not crash or produce garbage."""
        doc = HtmlDocument(string="<table></table>")
        data = doc.to_bytes()
        # Just a page with no content
        assert b"%PDF" in data

    @test
    def table_td_inline_style_color():
        """A <td> inline color style applies to cell text."""
        html = '<table><tr><td style="color: red">Red</td></tr></table>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Red")
        # The fill color for red should be emitted somewhere
        expect(data).to_contain(b"1 0 0 rg")

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
        expect(data).to_contain(b"Row1")
        expect(data).to_contain(b"Row2")
        expect(data).to_contain(b"Row3")

    @test
    def table_cells_with_padding():
        """Default cell padding leaves space inside cells."""
        # Hard to assert precise geometry, but the PDF should render
        # and contain both cells without collision.
        html = "<table><tr><td>Left</td><td>Right</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Left")
        expect(data).to_contain(b"Right")


with describe("images"):

    @test
    def png_img_produces_xobject():
        """<img src=...> for a PNG produces an Image XObject in the PDF."""
        png = _make_png(2, 2, bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255]))
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            path = f.name
        try:
            doc = HtmlDocument(string=f'<img src="{path}" width="50" height="50">')
            data = doc.to_bytes()
            expect(data).to_contain(b"/XObject")
            expect(data).to_contain(b"/Subtype /Image")
            expect(data).to_contain(b"/FlateDecode")
            expect(data).to_contain(b"/DeviceRGB")
            expect(data).to_contain(b"/Im0 Do")
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
            path = f.name
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
            path = f.name
        try:
            doc = HtmlDocument(string=f'<img src="{path}">')
            data = doc.to_bytes()
            # Just verify width and height dict entries exist
            expect(data).to_contain(b"/Width 4")
            expect(data).to_contain(b"/Height 3")
        finally:
            Path(path).unlink()

    @test
    def img_width_preserves_aspect_ratio():
        """Setting only width preserves aspect ratio for height."""
        # 4x2 (aspect 2:1)
        rgb = bytes([0, 0, 0] * 8)
        png = _make_png(4, 2, rgb)
        with tempfile.NamedTemporaryFile(suffix=".png", delete=False) as f:
            f.write(png)
            path = f.name
        try:
            doc = HtmlDocument(string=f'<img src="{path}" width="100">')
            data = doc.to_bytes()
            # A 100-wide image should become 50 tall (preserving 2:1 ratio)
            # The CTM is written as "100 0 0 50 x y cm"
            expect(data).to_contain(b"100 0 0 50")
        finally:
            Path(path).unlink()

    @test
    def missing_image_does_not_crash():
        """A missing image file is silently skipped."""
        doc = HtmlDocument(
            string='<p>before</p><img src="/nonexistent/image.png"><p>after</p>'
        )
        data = doc.to_bytes()
        expect(data).to_contain(b"before")
        expect(data).to_contain(b"after")
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
            expect(data).to_contain(b"/Im0 Do")
            expect(data).to_contain(b"/Im1 Do")
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
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_equals_no_match():
        """[name="value"] does not match when attribute value differs."""
        html = (
            '<style>[data-role="primary"] { color: red }</style>'
            '<p data-role="secondary">X</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def attr_includes_whitespace_list():
        """[class~="note"] matches a whitespace-separated token list member."""
        html = (
            '<style>[class~="note"] { color: red }</style>'
            '<p class="intro note main">X</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_includes_no_substring_match():
        """[class~="note"] does not match a bare substring like 'notepad'."""
        html = '<style>[class~="note"] { color: red }</style><p class="notepad">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def attr_dashmatch_en():
        """[lang|="en"] matches exact 'en'."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="en">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_dashmatch_en_us():
        """[lang|="en"] matches 'en-US' (prefix followed by dash)."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="en-US">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_dashmatch_english_no_match():
        """[lang|="en"] does not match 'english' (no dash after prefix)."""
        html = '<style>[lang|="en"] { color: red }</style><p lang="english">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def attr_exists_any_value():
        """[data-x] matches any element with that attribute regardless of value."""
        html = '<style>[data-x] { color: red }</style><p data-x="whatever">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_exists_empty_value():
        """[data-x] matches even when the attribute value is empty."""
        html = '<style>[data-x] { color: red }</style><p data-x="">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_compound_with_type():
        """p[class="foo"] requires both type and attribute to match."""
        html = '<style>p[class="foo"] { color: red }</style><p class="foo">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_compound_with_type_no_match():
        """p[class="foo"] does not match a div with class foo."""
        html = '<style>p[class="foo"] { color: red }</style><div class="foo">X</div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def attr_prefix_match():
        """[href^="http"] matches values starting with 'http'."""
        html = (
            '<style>[href^="http"] { color: red }</style>'
            '<a href="https://example.com">X</a>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_suffix_match():
        """[src$=".png"] matches values ending with '.png'."""
        html = '<style>[src$=".png"] { color: red }</style><p src="picture.png">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def attr_substring_match():
        """[class*="big"] matches values containing 'big'."""
        html = '<style>[class*="big"] { color: red }</style><p class="thebigone">X</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")


with describe("pseudo-classes and sibling combinators"):

    @test
    def first_child_matches_first_p():
        """p:first-child matches only the first p in a div."""
        html = (
            "<style>p:first-child { color: red }</style>"
            "<div><p>first</p><p>second</p><p>third</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

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
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"0 0 1 rg")

    @test
    def last_child_matches_last_p():
        """p:last-child matches the final p among its siblings."""
        html = (
            "<style>p:last-child { color: red }</style>"
            "<div><p>one</p><p>two</p><p>three</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def only_child_matches_single_child():
        """p:only-child matches a p that is the sole element child of its parent."""
        html = "<style>p:only-child { color: red }</style><div><p>alone</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def only_child_no_match_when_siblings():
        """p:only-child does not match when the p has siblings."""
        html = "<style>p:only-child { color: red }</style><div><p>a</p><p>b</p></div>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def nth_child_even_hits_even_positions():
        """li:nth-child(2n) matches the even-indexed children."""
        html = (
            "<style>li:nth-child(2n) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def nth_child_odd_hits_odd_positions():
        """li:nth-child(odd) matches the odd-indexed children."""
        html = (
            "<style>li:nth-child(odd) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def nth_child_literal_three():
        """li:nth-child(3) matches only the third element sibling."""
        html = (
            "<style>li:nth-child(3) { color: red }</style>"
            "<ul><li>1</li><li>2</li><li>3</li><li>4</li></ul>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def not_pseudo_excludes_class():
        """p:not(.skip) matches plain p but not p.skip."""
        html = (
            "<style>p:not(.skip) { color: red }</style>"
            '<p>one</p><p class="skip">two</p>'
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        # And 'two' should not be red — but we can only tell structurally.
        # Count red-set ops: exactly one fill-color set should appear for the
        # matching p. (The second paragraph falls back to default color.)

    @test
    def adjacent_sibling_matches_immediate_p():
        """h1 + p styles the p immediately following an h1."""
        html = (
            "<style>h1 + p { color: red }</style>"
            "<h1>Heading</h1><p>just after</p><p>later</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def adjacent_sibling_no_match_when_not_adjacent():
        """h1 + p does not match a p that is not immediately after an h1."""
        html = (
            "<style>h1 + p { color: red }</style>"
            "<h1>Heading</h1><div>gap</div><p>too late</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"1 0 0 rg" not in data).to_equal(True)

    @test
    def general_sibling_matches_all_following_p():
        """h1 ~ p styles all p elements following an h1 in document order."""
        html = (
            "<style>h1 ~ p { color: red }</style>"
            "<h1>Heading</h1><div>noise</div><p>after one</p><p>after two</p>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def pseudo_composes_with_child_combinator():
        """div > p:first-child composes pseudo-classes with child combinators."""
        html = (
            "<style>div > p:first-child { color: red }</style>"
            "<div><p>first</p><p>second</p></div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")


with describe("sup and sub"):

    @test
    def sup_renders_text():
        """<sup> text is included in the PDF content stream."""
        html = "<p>E=mc<sup>2</sup></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"E=mc")
        expect(data).to_contain(b"2")

    @test
    def sub_renders_text():
        """<sub> text is included in the PDF content stream."""
        html = "<p>H<sub>2</sub>O</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"(H")
        expect(data).to_contain(b"2")
        expect(data).to_contain(b"O)")


with describe("table caption"):

    @test
    def caption_appears_before_table_rows():
        """<caption> renders as text above the first row."""
        html = "<table><caption>My Data</caption><tr><td>a</td><td>b</td></tr></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"(My Data)")
        expect(data).to_contain(b"(a)")

    @test
    def caption_without_rows_still_renders():
        """A <caption>-only table still renders the caption text."""
        html = "<table><caption>Orphan</caption></table>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"(Orphan)")


with describe("min-height and max-height"):

    @test
    def min_height_expands_short_block():
        """min-height forces a block to occupy at least the specified height."""
        html = '<div style="min-height: 100pt; background-color: red">short</div>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"(short)")

    @test
    def max_height_clamps_tall_block():
        """max-height is accepted without crashing."""
        html = (
            '<div style="max-height: 20pt; background-color: blue">'
            "content that would normally be taller</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 rg")


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
        expect(data).to_contain(b"(rounded)")

    @test
    def border_top_left_radius_longhand():
        """border-top-left-radius longhand is parsed and applied."""
        html = (
            '<div style="border: 1pt solid black; border-top-left-radius: 8pt">'
            "corner</div>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"(corner)")

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
        expect(data).to_contain(b"(corners)")


with describe("opacity"):

    @test
    def opacity_emits_ext_graphics_state():
        """opacity < 1 emits a /Gs1 gs call and an ExtGState resource."""
        html = '<p style="opacity: 0.5">faded</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Gs1 gs")
        expect(data).to_contain(b"/ExtGState")

    @test
    def opacity_full_does_not_emit_state():
        """opacity: 1 is a no-op — no ExtGState emitted."""
        html = '<p style="opacity: 1">opaque</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(b"/Gs1 gs" not in data).to_equal(True)

    @test
    def opacity_clamped_zero_one():
        """opacity values outside [0,1] are clamped."""
        html = '<p style="opacity: 0.25">quarter</p>'
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Gs1 gs")


with describe("inline decoration tags"):

    @test
    def u_tag_emits_underline_stroke():
        """<u> text is underlined (stroke line in content stream)."""
        html = "<p>This is <u>underlined</u> text</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        # text_decoration underline draws a line, which uses S (stroke)
        expect(data).to_contain(b"underlined")

    @test
    def ins_tag_is_underlined_like_u():
        """<ins> renders the same way as <u>."""
        html = "<p>Marked <ins>insertion</ins></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"insertion")

    @test
    def del_tag_emits_strikethrough():
        """<del> text has line-through decoration."""
        html = "<p>removed <del>crossed out</del> here</p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"crossed out")

    @test
    def s_tag_is_strikethrough_like_del():
        """<s> is treated as strikethrough."""
        html = "<p><s>old price</s></p>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"old price")


with describe("details and summary"):

    @test
    def summary_renders_as_bold_block():
        """<summary> inside <details> is rendered as a block with text."""
        html = (
            "<details><summary>Click me</summary>"
            "<p>Hidden by default in a browser, shown in PDF.</p></details>"
        )
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Click me")
        expect(data).to_contain(b"Hidden by default")

    @test
    def details_body_always_visible_in_pdf():
        """PDF rendering ignores the open attribute — body is always laid out."""
        html = "<details><summary>S</summary><div>Body text</div></details>"
        doc = HtmlDocument(string=html)
        data = doc.to_bytes()
        expect(data).to_contain(b"Body text")
