import tempfile
from pathlib import Path

from tryke import describe, expect, test

from pdfun import FontDatabase, FontId, Layout, PdfDocument, text_width, wrap_text

with describe("PdfDocument API"):

    @test
    def create_document():
        """PdfDocument() constructs without error."""
        PdfDocument()

    @test
    def add_page_returns_page():
        """add_page() returns a Page with default US Letter dimensions."""
        doc = PdfDocument()
        page = doc.add_page()
        expect(page.width).to_equal(612.0)
        expect(page.height).to_equal(792.0)

    @test
    def add_page_custom_size():
        """add_page(w, h) accepts custom dimensions."""
        doc = PdfDocument()
        page = doc.add_page(595.28, 841.89)
        expect(page.width).to_equal(595.28)
        expect(page.height).to_equal(841.89)

    @test
    def to_bytes_returns_bytes():
        """to_bytes() returns PDF as bytes."""
        doc = PdfDocument()
        doc.add_page()
        data = doc.to_bytes()
        expect(type(data)).to_equal(bytes)
        expect(len(data)).to_be_greater_than(50)

    @test
    def save_writes_file():
        """save(path) writes PDF to disk."""
        doc = PdfDocument()
        doc.add_page()
        with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
            path = f.name
        path = Path(path)
        try:
            doc.save(str(path))
            expect(path.stat().st_size).to_be_greater_than(50)
        finally:
            path.unlink()

    @test
    def multi_page_document():
        """Multiple pages can be added to one document."""
        doc = PdfDocument()
        p1 = doc.add_page(100.0, 200.0)
        p2 = doc.add_page(300.0, 400.0)
        expect(p1.width).to_equal(100.0)
        expect(p2.width).to_equal(300.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"/Count 2")


with describe("Page API"):

    @test
    def page_dimensions():
        """Page exposes width/height properties."""
        doc = PdfDocument()
        page = doc.add_page(500.0, 700.0)
        expect(page.width).to_equal(500.0)
        expect(page.height).to_equal(700.0)

    @test
    def page_set_font():
        """set_font() accepts name and size without error."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)

    @test
    def page_draw_text():
        """draw_text() places text at (x, y)."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "Hello")
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello")


with describe("PDF output format"):

    @test
    def pdf_magic_bytes():
        """Output starts with %PDF header."""
        doc = PdfDocument()
        doc.add_page()
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def pdf_has_eof_marker():
        """Output ends with %%EOF trailer."""
        doc = PdfDocument()
        doc.add_page()
        data = doc.to_bytes()
        expect(data.rstrip().endswith(b"%%EOF")).to_be_truthy()

    @test
    def pdf_page_media_box():
        """MediaBox matches requested page dimensions."""
        doc = PdfDocument()
        doc.add_page(200.0, 300.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"/MediaBox [0 0 200 300]")

    @test
    def pdf_contains_font_reference():
        """PDF references a font resource when text is drawn."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "test")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica")

    @test
    def pdf_content_stream_has_text_operators():
        """Content stream contains BT/ET text operators."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "test")
        data = doc.to_bytes()
        expect(data).to_contain(b"BT")
        expect(data).to_contain(b"ET")


with describe("Text - built-in fonts"):

    @test
    def text_with_helvetica():
        """Helvetica text produces valid PDF."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "Hello")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Helvetica")
        expect(data).to_contain(b"(Hello) Tj")

    @test
    def text_with_times_roman():
        """Times-Roman is a valid built-in font."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Times-Roman", 12.0)
        page.draw_text(72.0, 720.0, "Hello")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Times-Roman")

    @test
    def text_with_courier():
        """Courier is a valid built-in font."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Courier", 12.0)
        page.draw_text(72.0, 720.0, "Hello")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Courier")

    @test
    def text_at_specific_position():
        """draw_text(x, y, ...) positions text via Td operator."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(100.0, 500.0, "Positioned")
        data = doc.to_bytes()
        expect(data).to_contain(b"100 500 Td")

    @test
    def text_multiple_lines():
        """Multiple draw_text() calls on one page."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "Line one")
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 700.0, "Line two")
        data = doc.to_bytes()
        expect(data).to_contain(b"Line one")
        expect(data).to_contain(b"Line two")

    @test
    def text_empty_string():
        """draw_text with empty string doesn't break PDF."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def text_special_chars():
        """Parentheses and backslashes are escaped in PDF strings."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "test (parens) and \\backslash")
        data = doc.to_bytes()
        expect(data).to_contain(b"\\(")
        expect(data).to_contain(b"\\)")
        expect(data).to_contain(b"\\\\")

    @test
    def text_font_size():
        """Font size is set via Tf operator."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 24.0)
        page.draw_text(72.0, 720.0, "Big")
        data = doc.to_bytes()
        expect(data).to_contain(b"24 Tf")


with describe("Text measurement"):

    @test
    def measure_text_helvetica():
        """measure_text() returns width in points for current font."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        width = page.measure_text("Hello")
        expect(abs(width - 27.336) < 0.01).to_be_truthy()

    @test
    def measure_text_courier_monospace():
        """Courier produces uniform character widths."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Courier", 10.0)
        width = page.measure_text("Hello")
        expect(abs(width - 30.0) < 0.01).to_be_truthy()

    @test
    def measure_text_empty_string():
        """measure_text() returns 0.0 for empty string."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Helvetica", 12.0)
        width = page.measure_text("")
        expect(width).to_equal(0.0)

    @test
    def measure_text_no_font_raises():
        """measure_text() raises ValueError if no font is set."""
        doc = PdfDocument()
        page = doc.add_page()
        expect(lambda: page.measure_text("test")).to_raise(ValueError)

    @test
    def text_width_standalone():
        """text_width() measures text without a page."""
        width = text_width("Hello", "Helvetica", 12.0)
        expect(abs(width - 27.336) < 0.01).to_be_truthy()

    @test
    def text_width_unknown_font_raises():
        """text_width() raises ValueError for unknown font."""
        expect(lambda: text_width("test", "FakeFont", 12.0)).to_raise(ValueError)

    @test
    def measure_text_different_fonts_differ():
        """Different fonts produce different widths."""
        helv = text_width("Hello World", "Helvetica", 12.0)
        times = text_width("Hello World", "Times-Roman", 12.0)
        courier = text_width("Hello World", "Courier", 12.0)
        expect(helv).not_.to_equal(times)
        expect(helv).not_.to_equal(courier)
        expect(times).not_.to_equal(courier)


with describe("FontDatabase API"):

    @test
    def create_font_db():
        """FontDatabase() constructs without error."""
        FontDatabase()

    @test
    def load_system_fonts():
        """load_system_fonts() discovers installed fonts."""
        db = FontDatabase()
        db.load_system_fonts()

    @test
    def load_font_file():
        """load_font_file() returns a FontId for a valid font."""
        candidates = list(Path("/System/Library/Fonts").glob("*.ttf"))
        if not candidates:
            candidates = list(Path("/usr/share/fonts").rglob("*.ttf"))
        if not candidates:
            return
        db = FontDatabase()
        font_id = db.load_font_file(str(candidates[0]))
        expect(font_id).not_.to_be_none()

    @test
    def load_font_file_invalid_raises():
        """load_font_file() raises ValueError for invalid file."""
        db = FontDatabase()
        with tempfile.NamedTemporaryFile(suffix=".ttf", delete=False) as f:
            f.write(b"not a font")
            path = Path(f.name)
        try:
            expect(lambda: db.load_font_file(str(path))).to_raise(ValueError)
        finally:
            path.unlink()

    @test
    def load_font_data():
        """load_font_data() accepts raw bytes of a valid font."""
        candidates = list(Path("/System/Library/Fonts").glob("*.ttf"))
        if not candidates:
            candidates = list(Path("/usr/share/fonts").rglob("*.ttf"))
        if not candidates:
            return
        data = candidates[0].read_bytes()
        db = FontDatabase()
        font_id = db.load_font_data(data)
        expect(font_id).not_.to_be_none()

    @test
    def load_font_data_invalid_raises():
        """load_font_data() raises ValueError for invalid data."""
        db = FontDatabase()
        expect(lambda: db.load_font_data(b"fake")).to_raise(ValueError)

    @test
    def query_by_family():
        """query() finds a font by family name."""
        db = FontDatabase()
        db.load_system_fonts()
        result = db.query("Helvetica")
        if result is None:
            result = db.query("DejaVu Sans")

    @test
    def query_missing_font():
        """query() returns None for unknown font."""
        db = FontDatabase()
        db.load_system_fonts()
        result = db.query("NonexistentFont12345XYZ")
        expect(result).to_be_none()

    @test
    def query_with_weight():
        """query() accepts weight parameter."""
        db = FontDatabase()
        db.load_system_fonts()
        db.query("Helvetica", weight=700)

    @test
    def query_with_italic():
        """query() accepts italic parameter."""
        db = FontDatabase()
        db.load_system_fonts()
        db.query("Helvetica", italic=True)


with describe("Font embedding"):

    def _load_system_font() -> tuple[FontDatabase, FontId] | None:
        """Load a system TTF font, return (db, font_id) or None."""
        candidates = list(Path("/System/Library/Fonts").glob("*.ttf"))
        if not candidates:
            candidates = list(Path("/usr/share/fonts").rglob("*.ttf"))
        if not candidates:
            return None
        db = FontDatabase()
        font_id = db.load_font_file(str(candidates[0]))
        return db, font_id

    @test
    def register_font_returns_name():
        """register_font() returns a font name string."""
        result = _load_system_font()
        if result is None:
            return
        db, font_id = result
        doc = PdfDocument()
        name = doc.register_font(db, font_id)
        expect(type(name)).to_equal(str)
        expect(len(name)).to_be_greater_than(0)

    @test
    def embedded_font_in_pdf():
        """PDF contains embedded font data after register_font()."""
        result = _load_system_font()
        if result is None:
            return
        db, font_id = result
        doc = PdfDocument()
        name = doc.register_font(db, font_id)
        page = doc.add_page()
        page.set_font(name, 12.0)
        page.draw_text(72.0, 720.0, "Hello embedded")
        data = doc.to_bytes()
        expect(data).to_contain(b"/Type0")

    @test
    def text_searchable_with_embedded_font():
        """PDF text is searchable (ToUnicode CMap present)."""
        result = _load_system_font()
        if result is None:
            return
        db, font_id = result
        doc = PdfDocument()
        name = doc.register_font(db, font_id)
        page = doc.add_page()
        page.set_font(name, 12.0)
        page.draw_text(72.0, 720.0, "Searchable")
        data = doc.to_bytes()
        expect(data).to_contain(b"/ToUnicode")

    @test
    def embedded_font_has_widths():
        """Embedded font CIDFont has a /W widths entry."""
        result = _load_system_font()
        if result is None:
            return
        db, font_id = result
        doc = PdfDocument()
        name = doc.register_font(db, font_id)
        page = doc.add_page()
        page.set_font(name, 12.0)
        page.draw_text(72.0, 720.0, "Widths")
        data = doc.to_bytes()
        expect(data).to_contain(b"/W ")

    @test
    def unicode_text_with_embedded_font():
        """Non-ASCII text renders with embedded font."""
        result = _load_system_font()
        if result is None:
            return
        db, font_id = result
        doc = PdfDocument()
        name = doc.register_font(db, font_id)
        page = doc.add_page()
        page.set_font(name, 12.0)
        page.draw_text(72.0, 720.0, "\u00e9\u00e8\u00ea")
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("Graphics - rectangles"):

    @test
    def draw_filled_rect():
        """draw_rect() produces re and f operators in content stream."""
        doc = PdfDocument()
        page = doc.add_page()
        page.draw_rect(100.0, 200.0, 50.0, 30.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"100 200 50 30 re")
        expect(data).to_contain(b"\nf\n")

    @test
    def stroke_rect():
        """stroke_rect() produces re and S operators."""
        doc = PdfDocument()
        page = doc.add_page()
        page.stroke_rect(10.0, 20.0, 100.0, 50.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"10 20 100 50 re")
        expect(data).to_contain(b"\nS\n")

    @test
    def fill_and_stroke_rect():
        """fill_and_stroke_rect() produces re and B operators."""
        doc = PdfDocument()
        page = doc.add_page()
        page.fill_and_stroke_rect(10.0, 20.0, 100.0, 50.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"10 20 100 50 re")
        expect(data).to_contain(b"\nB\n")


with describe("Graphics - colors"):

    @test
    def set_fill_color_rgb():
        """set_fill_color() produces rg operator."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_fill_color(1.0, 0.0, 0.0)
        page.draw_rect(10.0, 10.0, 50.0, 50.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")

    @test
    def set_stroke_color_rgb():
        """set_stroke_color() produces RG operator."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_stroke_color(0.0, 0.0, 1.0)
        page.stroke_rect(10.0, 10.0, 50.0, 50.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 1 RG")

    @test
    def fill_color_then_text():
        """Color changes affect subsequent text (colored text)."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_fill_color(1.0, 0.0, 0.0)
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 720.0, "Red text")
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"Red text")


with describe("Graphics - lines"):

    @test
    def draw_line():
        """draw_line() produces m, l, S operators."""
        doc = PdfDocument()
        page = doc.add_page()
        page.draw_line(0.0, 0.0, 100.0, 100.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 m")
        expect(data).to_contain(b"100 100 l")
        expect(data).to_contain(b"\nS\n")

    @test
    def set_line_width():
        """set_line_width() produces w operator."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_line_width(2.5)
        page.draw_line(0.0, 0.0, 100.0, 0.0)
        data = doc.to_bytes()
        expect(data).to_contain(b"2.5 w")

    @test
    def line_width_negative_raises():
        """set_line_width() raises ValueError for negative width."""
        doc = PdfDocument()
        page = doc.add_page()
        expect(lambda: page.set_line_width(-1.0)).to_raise(ValueError)


with describe("Graphics - state management"):

    @test
    def save_restore_state():
        """save_state()/restore_state() produce q/Q operators."""
        doc = PdfDocument()
        page = doc.add_page()
        page.save_state()
        page.set_fill_color(1.0, 0.0, 0.0)
        page.draw_rect(10.0, 10.0, 50.0, 50.0)
        page.restore_state()
        data = doc.to_bytes()
        expect(data).to_contain(b"q\n")
        expect(data).to_contain(b"Q\n")

    @test
    def state_isolates_color():
        """Colors set within save/restore do not leak."""
        doc = PdfDocument()
        page = doc.add_page()
        page.save_state()
        page.set_fill_color(1.0, 0.0, 0.0)
        page.draw_rect(10.0, 10.0, 50.0, 50.0)
        page.restore_state()
        page.draw_rect(100.0, 100.0, 50.0, 50.0)
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("Graphics - combined text and shapes"):

    @test
    def text_and_rect_on_same_page():
        """Text and rectangles coexist on a single page."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_fill_color(0.9, 0.9, 0.9)
        page.draw_rect(70.0, 710.0, 200.0, 20.0)
        page.set_fill_color(0.0, 0.0, 0.0)
        page.set_font("Helvetica", 12.0)
        page.draw_text(72.0, 715.0, "Hello on background")
        data = doc.to_bytes()
        expect(data).to_contain(b"re")
        expect(data).to_contain(b"Hello on background")


with describe("Text wrapping"):

    @test
    def wrap_single_word_fits():
        """Single word shorter than max_width returns one line."""
        lines = wrap_text("Hello", 100.0, "Courier", 10.0)
        expect(lines).to_equal(["Hello"])

    @test
    def wrap_empty_string():
        """Empty string returns empty list."""
        lines = wrap_text("", 100.0, "Courier", 10.0)
        expect(lines).to_equal([])

    @test
    def wrap_at_word_boundary():
        """Text wraps at word boundaries when line exceeds max_width."""
        lines = wrap_text("AA BB CC", 20.0, "Courier", 10.0)
        expect(lines).to_equal(["AA", "BB", "CC"])

    @test
    def wrap_long_word_overflows():
        """Word wider than max_width is placed alone, not split."""
        lines = wrap_text("ABCDEFGHIJ", 20.0, "Courier", 10.0)
        expect(lines).to_equal(["ABCDEFGHIJ"])

    @test
    def wrap_multiple_spaces_collapsed():
        """Consecutive spaces are collapsed to single space."""
        lines = wrap_text("A  B", 100.0, "Courier", 10.0)
        expect(lines).to_equal(["A B"])

    @test
    def wrap_trailing_whitespace_ignored():
        """Trailing whitespace does not produce empty trailing line."""
        lines = wrap_text("Hello   ", 100.0, "Courier", 10.0)
        expect(lines).to_equal(["Hello"])

    @test
    def wrap_proportional_font():
        """Proportional font wraps differently based on character widths."""
        narrow = wrap_text("ii ii", 20.0, "Helvetica", 10.0)
        wide = wrap_text("WW WW", 20.0, "Helvetica", 10.0)
        expect(len(narrow)).to_equal(1)
        expect(len(wide)).to_equal(2)

    @test
    def wrap_unknown_font_raises():
        """Unknown font raises ValueError."""
        expect(lambda: wrap_text("x", 100.0, "FakeFont", 10.0)).to_raise(ValueError)


with describe("Layout"):

    @test
    def layout_single_paragraph():
        """Single paragraph renders text in PDF."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Hello")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Hello")

    @test
    def layout_respects_margins():
        """Default margins position text at (72, page_height - 72 - font_size)."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Margin test", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"72 708 Td")

    @test
    def layout_wraps_long_text():
        """Long paragraph wraps within content area."""
        doc = PdfDocument()
        layout = Layout(doc)
        long_text = " ".join(["word"] * 50)
        layout.add_text(long_text)
        layout.finish()
        data = doc.to_bytes()
        expect(data.count(b"Td")).to_be_greater_than(1)

    @test
    def layout_multiple_paragraphs():
        """Two add_text calls produce two blocks of text."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("First paragraph")
        layout.add_text("Second paragraph")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"First paragraph")
        expect(data).to_contain(b"Second paragraph")

    @test
    def layout_page_break():
        """Enough text creates multiple pages."""
        doc = PdfDocument()
        layout = Layout(doc)
        for _ in range(100):
            layout.add_text("Paragraph that takes space.", spacing_after=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).not_.to_contain(b"/Count 1")

    @test
    def layout_custom_margins():
        """Non-default margins shift text position."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=100.0, margin_top=100.0)
        layout.add_text("Custom margins", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"100 680 Td")

    @test
    def layout_custom_line_height():
        """Custom line_height affects vertical spacing between lines."""
        doc = PdfDocument()
        layout = Layout(doc)
        long_text = " ".join(["word"] * 50)
        layout.add_text(long_text, line_height=20.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"72 688 Td")

    @test
    def layout_spacing_after():
        """spacing_after adds gap between paragraphs."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Para one", font_size=12.0, spacing_after=24.0)
        layout.add_text("Para two", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Para one")
        expect(data).to_contain(b"Para two")

    @test
    def layout_different_fonts():
        """Paragraphs with different fonts render correctly."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Helvetica text", font="Helvetica", font_size=12.0)
        layout.add_text("Times text", font="Times-Roman", font_size=14.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Helvetica text")
        expect(data).to_contain(b"Times text")

    @test
    def layout_empty_text():
        """add_text with empty string is a no-op."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("")
        layout.add_text("Actual text")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Actual text")

    @test
    def layout_finish_no_blocks():
        """finish() with no blocks produces valid PDF."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.finish()
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")


with describe("Layout - text color"):

    @test
    def layout_text_color():
        """add_text with color renders text in specified color."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Red text", color=(1.0, 0.0, 0.0))
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 rg")
        expect(data).to_contain(b"Red text")

    @test
    def layout_text_color_isolation():
        """Color from one block does not leak to the next."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Red", color=(1.0, 0.0, 0.0))
        layout.add_text("Default")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"q\n")
        expect(data).to_contain(b"Q\n")


with describe("Layout - background color"):

    @test
    def layout_background_color():
        """add_text with background_color draws filled rect behind text."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Highlighted", background_color=(1.0, 1.0, 0.0))
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 1 0 rg")
        expect(data).to_contain(b"re")
        expect(data).to_contain(b"Highlighted")

    @test
    def layout_background_behind_text():
        """Background rect appears before text in content stream."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Over bg", background_color=(0.9, 0.9, 0.9))
        layout.finish()
        data = doc.to_bytes()
        fill_pos = data.find(b"\nf\n")
        bt_pos = data.find(b"BT")
        expect(fill_pos).to_be_less_than(bt_pos)

    @test
    def layout_no_background_by_default():
        """add_text without background_color does not draw rectangles."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Plain text")
        layout.finish()
        data = doc.to_bytes()
        expect(data).not_.to_contain(b" re\n")


with describe("Layout - padding"):

    @test
    def layout_padding_uniform():
        """Uniform padding insets text from block edges."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=72.0, margin_top=72.0)
        layout.add_text(
            "Padded",
            font_size=12.0,
            padding=10.0,
            background_color=(0.9, 0.9, 0.9),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"82 698 Td")

    @test
    def layout_padding_four_sides():
        """Tuple padding (top, right, bottom, left) works."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=72.0, margin_top=72.0)
        layout.add_text(
            "Padded",
            font_size=12.0,
            padding=(20.0, 10.0, 20.0, 30.0),
            background_color=(0.9, 0.9, 0.9),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"102 688 Td")

    @test
    def layout_padding_narrows_wrap_width():
        """Padding reduces available width for text wrapping."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=10.0, margin_right=10.0, page_width=200.0)
        long_text = " ".join(["word"] * 20)
        layout.add_text(long_text, padding=50.0)
        layout.finish()
        data = doc.to_bytes()
        no_pad_doc = PdfDocument()
        no_pad_layout = Layout(
            no_pad_doc,
            margin_left=10.0,
            margin_right=10.0,
            page_width=200.0,
        )
        no_pad_layout.add_text(long_text)
        no_pad_layout.finish()
        no_pad_data = no_pad_doc.to_bytes()
        expect(data.count(b"Td")).to_be_greater_than(no_pad_data.count(b"Td"))

    @test
    def layout_padding_zero_default():
        """Default padding=0 behaves like no padding."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("No padding", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"72 708 Td")


with describe("Layout - borders"):

    @test
    def layout_border():
        """add_text with border_width draws stroked rect around block."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Bordered", border_width=1.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 w")
        expect(data).to_contain(b"re")
        expect(data).to_contain(b"\nS\n")

    @test
    def layout_border_color():
        """border_color controls stroke color of border."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Red border", border_width=2.0, border_color=(1.0, 0.0, 0.0))
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"1 0 0 RG")
        expect(data).to_contain(b"2 w")

    @test
    def layout_border_default_color_black():
        """Border without border_color defaults to black."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Default border", border_width=1.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"0 0 0 RG")

    @test
    def layout_no_border_by_default():
        """No border drawn when border_width is 0."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("No border")
        layout.finish()
        data = doc.to_bytes()
        expect(data).not_.to_contain(b"RG")

    @test
    def layout_border_with_background():
        """Border and background can coexist; fill appears before stroke."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text(
            "Both",
            background_color=(0.9, 0.9, 0.9),
            border_width=1.0,
            border_color=(0.0, 0.0, 0.0),
        )
        layout.finish()
        data = doc.to_bytes()
        fill_pos = data.find(b"\nf\n")
        stroke_pos = data.find(b"\nS\n")
        expect(fill_pos).to_be_less_than(stroke_pos)


with describe("Layout - text alignment"):

    @test
    def layout_align_left_default():
        """Default alignment is left (text at margin_left)."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text("Left aligned", font_size=12.0)
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"72 708 Td")

    @test
    def layout_align_center():
        """center alignment centers each line within content area."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=72.0, margin_right=72.0)
        layout.add_text("Hi", font="Courier", font_size=10.0, text_align="center")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"300 710 Td")

    @test
    def layout_align_right():
        """right alignment right-aligns each line within content area."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=72.0, margin_right=72.0)
        layout.add_text("Hi", font="Courier", font_size=10.0, text_align="right")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"528 710 Td")

    @test
    def layout_align_invalid_raises():
        """Invalid text_align value raises ValueError."""
        doc = PdfDocument()
        layout = Layout(doc)
        expect(lambda: layout.add_text("x", text_align="bogus")).to_raise(ValueError)

    @test
    def layout_align_center_with_padding():
        """center alignment accounts for padding."""
        doc = PdfDocument()
        layout = Layout(doc, margin_left=72.0, margin_right=72.0)
        layout.add_text(
            "Hi",
            font="Courier",
            font_size=10.0,
            text_align="center",
            padding=(0.0, 20.0, 0.0, 20.0),
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"300 710 Td")


with describe("Layout - combined styling"):

    @test
    def layout_full_box_model():
        """Block with all styling properties produces correct PDF ops."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text(
            "Styled block",
            font_size=12.0,
            color=(1.0, 1.0, 1.0),
            background_color=(0.0, 0.0, 0.5),
            padding=10.0,
            border_width=1.0,
            border_color=(0.0, 0.0, 0.0),
            text_align="center",
        )
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Styled block")
        expect(data).to_contain(b"q\n")
        expect(data).to_contain(b"Q\n")
        expect(data).to_contain(b"re")
        expect(data[:5]).to_equal(b"%PDF-")

    @test
    def layout_styled_blocks_page_break():
        """Padded blocks correctly trigger page breaks."""
        doc = PdfDocument()
        layout = Layout(doc)
        for _ in range(20):
            layout.add_text(
                "Block with padding",
                font_size=12.0,
                padding=20.0,
                spacing_after=10.0,
                background_color=(0.95, 0.95, 0.95),
            )
        layout.finish()
        data = doc.to_bytes()
        expect(data).not_.to_contain(b"/Count 1")

    @test
    def layout_styled_then_unstyled():
        """Styled block followed by unstyled block does not leak styling."""
        doc = PdfDocument()
        layout = Layout(doc)
        layout.add_text(
            "Styled",
            color=(1.0, 0.0, 0.0),
            background_color=(1.0, 1.0, 0.0),
        )
        layout.add_text("Plain")
        layout.finish()
        data = doc.to_bytes()
        expect(data).to_contain(b"Styled")
        expect(data).to_contain(b"Plain")


with describe("Document metadata"):

    @test
    def set_title_appears_in_pdf():
        """set_title() writes a /Title entry in the PDF info dictionary."""
        doc = PdfDocument()
        doc.set_title("My Document")
        doc.add_page()
        data = doc.to_bytes()
        assert b"/Title" in data
        assert b"My Document" in data

    @test
    def set_author_appears_in_pdf():
        """set_author() writes an /Author entry."""
        doc = PdfDocument()
        doc.set_author("Jane Doe")
        doc.add_page()
        data = doc.to_bytes()
        assert b"/Author" in data
        assert b"Jane Doe" in data

    @test
    def set_keywords_appears_in_pdf():
        """set_keywords() writes a /Keywords entry."""
        doc = PdfDocument()
        doc.set_keywords("pdf, test")
        doc.add_page()
        data = doc.to_bytes()
        assert b"/Keywords" in data
        assert b"pdf, test" in data

    @test
    def default_creator_is_pdfun():
        """The default creator is 'pdfun'."""
        doc = PdfDocument()
        doc.add_page()
        data = doc.to_bytes()
        assert b"/Creator" in data
        assert b"pdfun" in data
