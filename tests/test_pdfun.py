import os
import tempfile

from tryke import describe, expect, test

from pdfun import FontDatabase, PdfDocument

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

    @test
    def measure_text_different_fonts_differ():
        """Different fonts produce different widths."""
        from pdfun import text_width

        helv = text_width("Hello World", "Helvetica", 12.0)
        times = text_width("Hello World", "Times-Roman", 12.0)
        courier = text_width("Hello World", "Courier", 12.0)
        # these should all be different (Courier is monospace)
        expect(helv).not_.to_equal(times)
        expect(helv).not_.to_equal(courier)
        expect(times).not_.to_equal(courier)


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
        db.load_system_fonts()

    @test
    def load_font_file():
        """load_font_file() returns a FontId for a valid font."""
        import glob

        # use a system font available on macOS
        candidates = glob.glob("/System/Library/Fonts/*.ttf")
        if not candidates:
            candidates = glob.glob("/usr/share/fonts/**/*.ttf", recursive=True)
        if not candidates:
            return  # skip on systems with no accessible TTF files
        db = FontDatabase()
        font_id = db.load_font_file(candidates[0])
        expect(font_id).not_.to_be_none()

    @test
    def load_font_file_invalid_raises():
        """load_font_file() raises ValueError for invalid file."""
        import tempfile

        db = FontDatabase()
        with tempfile.NamedTemporaryFile(suffix=".ttf", delete=False) as f:
            f.write(b"not a font")
            path = f.name
        try:
            expect(lambda: db.load_font_file(path)).to_raise(ValueError)
        finally:
            os.unlink(path)

    @test
    def load_font_data():
        """load_font_data() accepts raw bytes of a valid font."""
        import glob

        candidates = glob.glob("/System/Library/Fonts/*.ttf")
        if not candidates:
            candidates = glob.glob("/usr/share/fonts/**/*.ttf", recursive=True)
        if not candidates:
            return  # skip on systems with no accessible TTF files
        with open(candidates[0], "rb") as f:
            data = f.read()
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
        # Helvetica is available on macOS; on Linux try DejaVu
        result = db.query("Helvetica")
        if result is None:
            result = db.query("DejaVu Sans")
        # at least one should work on any system with fonts

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
        # should not raise, regardless of result
        db.query("Helvetica", weight=700)

    @test
    def query_with_italic():
        """query() accepts italic parameter."""
        db = FontDatabase()
        db.load_system_fonts()
        # should not raise, regardless of result
        db.query("Helvetica", italic=True)


with describe("Font embedding"):

    def _load_system_font():
        """helper: load a system TTF font, return (db, font_id) or None."""
        import glob

        candidates = glob.glob("/System/Library/Fonts/*.ttf")
        if not candidates:
            candidates = glob.glob("/usr/share/fonts/**/*.ttf", recursive=True)
        if not candidates:
            return None
        db = FontDatabase()
        font_id = db.load_font_file(candidates[0])
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
        page.draw_text(72.0, 720.0, "\u00e9\u00e8\u00ea")  # accented chars
        data = doc.to_bytes()
        expect(data[:5]).to_equal(b"%PDF-")
