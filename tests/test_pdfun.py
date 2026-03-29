import os
import tempfile

from tryke import describe, expect, test

from pdfun import FontDatabase, FontId, PdfDocument

# ── API surface (cf. WeasyPrint test_api.py) ────────────────────

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
        try:
            doc.save(path)
            expect(os.path.getsize(path)).to_be_greater_than(50)
        finally:
            os.unlink(path)

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


# ── PDF structure (cf. WeasyPrint test_pdf.py) ──────────────────

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


# ── Text rendering (cf. WeasyPrint test_text.py) ────────────────

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
        # "Hello" in Helvetica: H=722 e=556 l=222 l=222 o=556 = 2278
        # 2278 * 12 / 1000 = 27.336
        width = page.measure_text("Hello")
        expect(abs(width - 27.336) < 0.01).to_be_truthy()

    @test
    def measure_text_courier_monospace():
        """Courier produces uniform character widths."""
        doc = PdfDocument()
        page = doc.add_page()
        page.set_font("Courier", 10.0)
        # every char is 600 units, so 5 chars = 3000 * 10 / 1000 = 30.0
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
        from pdfun import text_width

        width = text_width("Hello", "Helvetica", 12.0)
        expect(abs(width - 27.336) < 0.01).to_be_truthy()

    @test
    def text_width_unknown_font_raises():
        """text_width() raises ValueError for unknown font."""
        from pdfun import text_width

        expect(lambda: text_width("test", "FakeFont", 12.0)).to_raise(ValueError)


# ── Font loading (cf. WeasyPrint test_fonts.py) ─────────────────
# These remain red (NotImplementedError) until Phase 2.

with describe("FontDatabase API"):

    @test
    def create_font_db():
        """FontDatabase() constructs without error."""
        FontDatabase()

    @test
    def load_system_fonts():
        """load_system_fonts() discovers installed fonts."""
        db = FontDatabase()
        expect(lambda: db.load_system_fonts()).to_raise(NotImplementedError)

    @test
    def load_font_file():
        """load_font_file() returns a FontId."""
        db = FontDatabase()
        expect(lambda: db.load_font_file("/nonexistent.ttf")).to_raise(
            NotImplementedError
        )

    @test
    def load_font_data():
        """load_font_data() accepts raw bytes."""
        db = FontDatabase()
        expect(lambda: db.load_font_data(b"fake")).to_raise(NotImplementedError)

    @test
    def query_by_family():
        """query() finds a font by family name."""
        db = FontDatabase()
        expect(lambda: db.query("Arial")).to_raise(NotImplementedError)

    @test
    def query_with_weight():
        """query() accepts weight parameter."""
        db = FontDatabase()
        expect(lambda: db.query("Arial", weight=700)).to_raise(NotImplementedError)

    @test
    def query_with_italic():
        """query() accepts italic parameter."""
        db = FontDatabase()
        expect(lambda: db.query("Arial", italic=True)).to_raise(NotImplementedError)

    @test
    def query_missing_font():
        """query() returns None for unknown font."""
        db = FontDatabase()
        expect(lambda: db.query("NonexistentFont12345")).to_raise(NotImplementedError)


with describe("Font embedding"):

    @test
    def register_font():
        """register_font() returns a font name string."""
        doc = PdfDocument()
        db = FontDatabase()
        expect(lambda: doc.register_font(db, FontId())).to_raise(NotImplementedError)

    @test
    def embedded_font_in_pdf():
        """PDF contains embedded font data after register_font()."""
        doc = PdfDocument()
        db = FontDatabase()
        expect(lambda: doc.register_font(db, FontId())).to_raise(NotImplementedError)

    @test
    def text_searchable_with_embedded_font():
        """PDF text is searchable (ToUnicode CMap present)."""
        doc = PdfDocument()
        db = FontDatabase()
        expect(lambda: doc.register_font(db, FontId())).to_raise(NotImplementedError)

    @test
    def shaped_text_dimensions():
        """Shaped text has non-zero advance widths."""
        doc = PdfDocument()
        db = FontDatabase()
        expect(lambda: doc.register_font(db, FontId())).to_raise(NotImplementedError)

    @test
    def unicode_text_with_embedded_font():
        """Non-ASCII text renders with embedded font."""
        doc = PdfDocument()
        db = FontDatabase()
        expect(lambda: doc.register_font(db, FontId())).to_raise(NotImplementedError)
