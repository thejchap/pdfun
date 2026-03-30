import tempfile
from pathlib import Path

from tryke import describe, expect, test

from pdfun import HtmlDocument

# ── Constructor ────────────────────────────────────────────────

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


# ── Headings ───────────────────────────────────────────────────

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


# ── Paragraphs ─────────────────────────────────────────────────

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


# ── Div ────────────────────────────────────────────────────────

with describe("HtmlDocument - div"):

    @test
    def div_renders():
        """<div> renders its text content."""
        doc = HtmlDocument(string="<div>Div content</div>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Div content")


# ── Line breaks ───────────────────────────────────────────────

with describe("HtmlDocument - br"):

    @test
    def br_splits_text():
        """<br> creates a line break between text."""
        doc = HtmlDocument(string="<p>Line one<br>Line two</p>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Line one")
        expect(data).to_contain(b"Line two")


# ── Inline elements ───────────────────────────────────────────

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


# ── Inline styling ─────────────────────────────────────────────

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


# ── Output ─────────────────────────────────────────────────────

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


# ── Complex documents ─────────────────────────────────────────

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


# ── Edge cases ─────────────────────────────────────────────────

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


# ── Lists ─────────────────────────────────────────────────────

with describe("HtmlDocument - lists"):

    @test
    def ul_renders_item_text():
        """<ul><li> renders item text in PDF."""
        doc = HtmlDocument(string="<ul><li>Item one</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"Item one")

    @test
    def ul_has_bullet_marker():
        """<ul><li> has a dash bullet marker."""
        doc = HtmlDocument(string="<ul><li>Item</li></ul>")
        data = doc.to_bytes()
        expect(data).to_contain(b"(-)")  # PDF string encoding

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
        # Depth 0 uses "-", depth 1 uses "o"
        expect(data).to_contain(b"(-)")
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
