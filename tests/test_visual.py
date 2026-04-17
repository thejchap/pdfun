"""Visual regression tests — render HTML to PDF to PNG, compare against references."""

from __future__ import annotations

import os
from pathlib import Path

import fitz
from tryke import describe, expect, test

from pdfun import HtmlDocument

VISUAL_DIR = Path(__file__).parent / "visual"
REF_DIR = VISUAL_DIR / "ref"
BLESS = os.environ.get("BLESS", "").strip() == "1"
DPI = 200
THRESHOLD = 0.001


def render_to_png(html: str, *, dpi: int = DPI) -> bytes:
    """HTML -> PDF -> PNG bytes (first page at given DPI)."""
    doc = HtmlDocument(string=html)
    pdf_bytes = doc.to_bytes()
    fitz_doc = fitz.open(stream=pdf_bytes, filetype="pdf")
    page = fitz_doc[0]
    scale = dpi / 72.0
    pix = page.get_pixmap(matrix=fitz.Matrix(scale, scale))
    return pix.tobytes("png")


def pixel_diff(img_a: bytes, img_b: bytes) -> float:
    """Return fraction of pixels that differ between two PNGs."""
    pix_a = fitz.Pixmap(img_a)
    pix_b = fitz.Pixmap(img_b)
    if pix_a.width != pix_b.width or pix_a.height != pix_b.height:
        return 1.0
    samples_a = pix_a.samples
    samples_b = pix_b.samples
    total = len(samples_a)
    if total == 0:
        return 0.0
    diff_count = sum(1 for a, b in zip(samples_a, samples_b, strict=True) if a != b)
    return diff_count / total


def _check_visual(name: str) -> None:
    html_path = VISUAL_DIR / f"{name}.html"
    html = html_path.read_text()
    actual = render_to_png(html)

    ref_path = REF_DIR / f"{name}.png"
    if not ref_path.exists() or BLESS:
        ref_path.parent.mkdir(parents=True, exist_ok=True)
        ref_path.write_bytes(actual)
        return

    expected = ref_path.read_bytes()
    diff = pixel_diff(actual, expected)
    expect(diff).to_be_less_than(THRESHOLD)


with describe("visual regression"):

    @test
    def border_radius():
        _check_visual("border_radius")

    @test
    def columns():
        _check_visual("columns")

    @test
    def float_left():
        _check_visual("float_left")

    @test
    def float_right():
        _check_visual("float_right")

    @test
    def heading_sizes():
        _check_visual("heading_sizes")

    @test
    def inline_styles():
        _check_visual("inline_styles")

    @test
    def list_styles():
        _check_visual("list_styles")

    @test
    def margin_collapse_parent_child():
        _check_visual("margin_collapse_parent_child")

    @test
    def margin_collapse_siblings():
        _check_visual("margin_collapse_siblings")

    @test
    def nested_containers():
        _check_visual("nested_containers")

    @test
    def opacity():
        _check_visual("opacity")

    @test
    def padding_border():
        _check_visual("padding_border")

    @test
    def page_break():
        _check_visual("page_break")

    @test
    def table_layout():
        _check_visual("table_layout")

    @test
    def text_align():
        _check_visual("text_align")
