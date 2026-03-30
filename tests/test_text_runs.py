from tryke import describe, expect, test

from pdfun import Layout, PdfDocument, TextRun

with describe("TextRun"):

    @test
    def create_with_defaults():
        """TextRun("hello") has default font and size."""
        run = TextRun("hello")
        expect(run.text).to_equal("hello")
        expect(run.font_name).to_equal("Helvetica")
        expect(run.font_size).to_equal(12.0)
        expect(run.color).to_be_none()

    @test
    def create_with_all_params():
        """TextRun with explicit font, size, and color."""
        run = TextRun("hi", font_name="Courier", font_size=24.0, color=(1.0, 0.0, 0.0))
        expect(run.text).to_equal("hi")
        expect(run.font_name).to_equal("Courier")
        expect(run.font_size).to_equal(24.0)
        expect(run.color).to_equal((1.0, 0.0, 0.0))


with describe("Layout.add_paragraph - single run"):

    @test
    def single_run_renders():
        """Paragraph with one run renders text in PDF."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(runs=[TextRun("Hello world")])
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello world")

    @test
    def single_run_bold():
        """TextRun with Helvetica-Bold appears in PDF."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(runs=[TextRun("Bold text", font_name="Helvetica-Bold")])
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"Bold text")


with describe("Layout.add_paragraph - multiple runs"):

    @test
    def two_runs_same_line():
        """Short bold + normal text both appear."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[
                TextRun("Hello ", font_name="Helvetica-Bold"),
                TextRun("world"),
            ],
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello")
        expect(data).to_contain(b"world")

    @test
    def bold_and_normal_fonts_in_pdf():
        """PDF contains both /Helvetica and /Helvetica-Bold font references."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[
                TextRun("Bold ", font_name="Helvetica-Bold"),
                TextRun("normal", font_name="Helvetica"),
            ],
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
        expect(data).to_contain(b"/BaseFont /Helvetica\n")

    @test
    def runs_wrap_across_lines():
        """Long mixed-font text wraps correctly."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[
                TextRun(" ".join(["bold"] * 30), font_name="Helvetica-Bold"),
                TextRun(" ".join(["normal"] * 30)),
            ],
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data.count(b"Td")).to_be_greater_than(1)

    @test
    def different_font_sizes():
        """Runs with different font sizes both render."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[
                TextRun("Big ", font_size=24.0),
                TextRun("small", font_size=10.0),
            ],
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"24 Tf")
        expect(data).to_contain(b"10 Tf")


with describe("Layout.add_paragraph - block styling"):

    @test
    def paragraph_with_spacing_after():
        """spacing_after creates gap between paragraphs."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(runs=[TextRun("First")], spacing_after=20.0)
        layout.add_paragraph(runs=[TextRun("Second")])
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Second")

    @test
    def paragraph_with_background():
        """Background rectangle drawn behind paragraph."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[TextRun("Styled")],
            background_color=(0.9, 0.9, 0.9),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"0.9 0.9 0.9 rg")

    @test
    def paragraph_with_padding():
        """Padding insets text from block edges."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[TextRun("Padded")],
            padding=10.0,
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Padded")

    @test
    def paragraph_with_text_align_center():
        """Centered mixed-font text."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[TextRun("Centered")],
            text_align="center",
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Centered")


with describe("Layout.add_paragraph - backward compat"):

    @test
    def add_text_still_works():
        """Existing add_text API still works."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Hello world", font="Helvetica", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello world")

    @test
    def mixed_add_text_and_add_paragraph():
        """Both add_text and add_paragraph on same Layout."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("First", font="Helvetica", font_size=12.0)
        layout.add_paragraph(runs=[TextRun("Second", font_name="Helvetica-Bold")])
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"First")
        expect(data).to_contain(b"Second")


with describe("Layout.add_paragraph - colors"):

    @test
    def run_color_overrides_block():
        """Run-level color takes precedence over block color."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[TextRun("Red", color=(1.0, 0.0, 0.0))],
            color=(0.0, 0.0, 1.0),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def different_runs_different_colors():
        """Each run can have its own color."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[
                TextRun("Red ", color=(1.0, 0.0, 0.0)),
                TextRun("Blue", color=(0.0, 0.0, 1.0)),
            ],
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"0 0 1 rg")


with describe("Layout.add_paragraph - marker"):

    @test
    def paragraph_with_marker():
        """Paragraph with marker TextRun renders marker and body text."""
        doc = PdfDocument()
        layout = Layout(doc)
        marker = TextRun("-")
        layout.add_paragraph(
            runs=[TextRun("Item text")],
            padding=(0.0, 0.0, 0.0, 36.0),
            marker=marker,
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Item text")

    @test
    def marker_font_registered():
        """Marker's font is registered in the PDF."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_paragraph(
            runs=[TextRun("Item")],
            padding=(0.0, 0.0, 0.0, 36.0),
            marker=TextRun("1.", font_name="Helvetica-Bold"),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica-Bold")
