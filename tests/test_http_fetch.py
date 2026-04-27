"""End-to-end tests for the pluggable URL fetcher and the opt-in HTTP backend.

These exercise the public Python API: a `url_fetcher` callable on
`HtmlDocument`, plus the no-feature-flag fallback path. The tests do not
require the Rust `http-fetch` feature to be compiled in — when it is
not, the renderer's `DefaultFetcher` returns a "feature not enabled"
warning and we assert that the document still renders.
"""

import struct
import zlib

from tryke import describe, expect, test

from pdfun import HtmlDocument


def _make_png(width: int, height: int, rgb_bytes: bytes) -> bytes:
    """Build a minimal 8-bit RGB PNG."""
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(ctype: bytes, data: bytes) -> bytes:
        crc = zlib.crc32(ctype + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + ctype + data + struct.pack(">I", crc)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    raw = b""
    for y in range(height):
        raw += b"\x00" + rgb_bytes[y * width * 3 : (y + 1) * width * 3]
    idat = zlib.compress(raw)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


with describe("pluggable URL fetcher"):

    @test("a custom url_fetcher feeds bytes for an http:// <img> src")
    def custom_fetcher_loads_remote_image():
        # Tiny 1x1 red PNG.
        png = _make_png(1, 1, b"\xff\x00\x00")
        seen = []

        def fetcher(url: str) -> bytes | None:
            seen.append(url)
            if url == "http://example.test/red.png":
                return png
            return None

        html = '<html><body><img src="http://example.test/red.png"></body></html>'
        doc = HtmlDocument(string=html, url_fetcher=fetcher)
        pdf_bytes = doc.to_bytes()

        # The fetcher must have been invoked for the remote image, and
        # the resulting PDF must embed something — we don't deep-parse
        # here, just confirm the render didn't crash and produced bytes.
        expect(seen).to_contain("http://example.test/red.png")
        expect(len(pdf_bytes)).to_be_greater_than(100)
        expect(pdf_bytes.startswith(b"%PDF-")).to_be(True)
        expect(doc.warnings()).to_equal([])

    @test("a returning-None url_fetcher surfaces a render warning")
    def fetcher_none_becomes_warning():
        def fetcher(url: str) -> bytes | None:  # noqa: ARG001
            return None

        html = '<html><body><img src="http://example.test/missing.png"></body></html>'
        doc = HtmlDocument(string=html, url_fetcher=fetcher)
        # The render still completes; the missing image becomes a warning.
        pdf_bytes = doc.to_bytes()
        expect(pdf_bytes.startswith(b"%PDF-")).to_be(True)
        warnings = doc.warnings()
        expect(any("missing.png" in w for w in warnings)).to_be(True)

    @test("a raising url_fetcher does not crash the renderer")
    def fetcher_exception_becomes_warning():
        def fetcher(url: str) -> bytes | None:  # noqa: ARG001
            msg = "synthetic failure"
            raise RuntimeError(msg)

        html = '<html><body><img src="http://example.test/x.png"></body></html>'
        doc = HtmlDocument(string=html, url_fetcher=fetcher)
        pdf_bytes = doc.to_bytes()
        expect(pdf_bytes.startswith(b"%PDF-")).to_be(True)
        warnings = doc.warnings()
        expect(any("x.png" in w for w in warnings)).to_be(True)

    @test("without a custom fetcher http:// <img> falls through gracefully")
    def http_img_without_feature_or_fetcher_warns():
        # With neither the Rust http-fetch feature nor a Python
        # callable, the DefaultFetcher returns "feature not enabled"
        # and the renderer surfaces a warning. The document still
        # renders.
        html = '<html><body><img src="http://example.test/y.png"></body></html>'
        doc = HtmlDocument(string=html)
        pdf_bytes = doc.to_bytes()
        expect(pdf_bytes.startswith(b"%PDF-")).to_be(True)
        warnings = doc.warnings()
        expect(any("y.png" in w for w in warnings)).to_be(True)

    @test("file-only fetcher resolves bare relative paths the same as before")
    def relative_path_still_works():
        # Smoke-test that wiring the fetcher through doesn't break the
        # bare-path code path. We don't bother creating a real image —
        # the warning text is enough to confirm the lookup happened.
        html = '<html><body><img src="missing.png"></body></html>'
        doc = HtmlDocument(string=html)
        warnings = doc.warnings()
        expect(any("missing.png" in w for w in warnings)).to_be(True)
