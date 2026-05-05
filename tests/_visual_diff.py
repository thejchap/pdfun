"""Visual-regression diff helpers.

Render pdfun output to per-page PNGs, tolerantly compare them against
committed WeasyPrint references, and write report artifacts. The static
HTML report (see ``_visual_report.py``) handles side-by-side display via
CSS, so we don't compose pixmaps here — keeps the diff loop fast in pure
Python.
"""

from __future__ import annotations

import tomllib
from pathlib import Path

import fitz

from pdfun import HtmlDocument

DEFAULT_DPI = 100
CHANNEL_TOL = 12
SUBSAMPLE_STEP = 4
VISUAL_DIR = Path(__file__).parent / "visual"
REF_DIR = VISUAL_DIR / "ref"
TOLERANCES_PATH = VISUAL_DIR / "tolerances.toml"
REPORT_DIR = Path(__file__).resolve().parents[1] / "target" / "visual-report"


def render_pdfun_pages(html: str, *, dpi: int = DEFAULT_DPI) -> list[bytes]:
    """Render HTML through pdfun and rasterize every page to PNG bytes."""
    pdf_bytes = HtmlDocument(string=html).to_bytes()
    return rasterize_pdf_bytes(pdf_bytes, dpi=dpi)


def rasterize_pdf_bytes(pdf_bytes: bytes, *, dpi: int = DEFAULT_DPI) -> list[bytes]:
    """Rasterize every page of a PDF to PNG bytes at the requested DPI."""
    scale = dpi / 72.0
    matrix = fitz.Matrix(scale, scale)
    doc = fitz.open(stream=pdf_bytes, filetype="pdf")
    try:
        return [page.get_pixmap(matrix=matrix).tobytes("png") for page in doc]
    finally:
        doc.close()


def tolerant_pixel_diff(
    actual_png: bytes,
    expected_png: bytes,
    *,
    channel_tol: int = CHANNEL_TOL,
    subsample: int = SUBSAMPLE_STEP,
) -> float:
    """Fraction of (subsampled) pixels whose RGB channels differ by > tol.

    Subsampling is essential for keeping the diff cheap in pure Python — a
    100 DPI A4 page is still ~900 K pixels. Sampling every Nth pixel gives
    a fine-enough estimate for tolerance gating while running in well under
    a second per fixture. The actual and reference PNGs are still written
    at full resolution for the report.
    """
    pix_a = fitz.Pixmap(actual_png)
    pix_b = fitz.Pixmap(expected_png)
    if pix_a.width != pix_b.width or pix_a.height != pix_b.height:
        return 1.0
    samples_a = pix_a.samples
    samples_b = pix_b.samples
    stride = pix_a.n
    pixel_count = pix_a.width * pix_a.height
    if pixel_count == 0:
        return 0.0
    sampled = 0
    different = 0
    for i in range(0, len(samples_a), stride * subsample):
        sampled += 1
        if (
            abs(samples_a[i] - samples_b[i]) > channel_tol
            or abs(samples_a[i + 1] - samples_b[i + 1]) > channel_tol
            or abs(samples_a[i + 2] - samples_b[i + 2]) > channel_tol
        ):
            different += 1
    return different / sampled if sampled else 0.0


def composite_diff_png(
    actual_png: bytes,
    ref_png: bytes,
    *,
    channel_tol: int = CHANNEL_TOL,
    gap: int = 8,
) -> bytes:
    """Build a side-by-side composite (actual | reference | diff overlay).

    The diff overlay is a faded grayscale of the actual page with red
    markers wherever the channels diverge by more than ``channel_tol``.
    A single image captures the whole "what changed and where" picture,
    which makes it useful both as a CI artifact and for AI-assisted
    iteration where reading three separate PNGs is clunky.

    Padding handles the common case where pdfun and WeasyPrint render
    the page at slightly different sizes — both inputs are padded to
    the bounding canvas with white before diffing.
    """
    pix_a = fitz.Pixmap(actual_png)
    pix_b = fitz.Pixmap(ref_png)
    if pix_a.alpha:
        pix_a = fitz.Pixmap(pix_a, 0)
    if pix_b.alpha:
        pix_b = fitz.Pixmap(pix_b, 0)
    canvas_w = max(pix_a.width, pix_b.width)
    canvas_h = max(pix_a.height, pix_b.height)
    sa = _pad_white(pix_a.samples, pix_a.width, pix_a.height, canvas_w, canvas_h)
    sb = _pad_white(pix_b.samples, pix_b.width, pix_b.height, canvas_w, canvas_h)

    diff = bytearray(canvas_w * canvas_h * 3)
    for i in range(0, canvas_w * canvas_h * 3, 3):
        ar, ag, ab = sa[i], sa[i + 1], sa[i + 2]
        br, bg, bb = sb[i], sb[i + 1], sb[i + 2]
        if (
            abs(ar - br) > channel_tol
            or abs(ag - bg) > channel_tol
            or abs(ab - bb) > channel_tol
        ):
            diff[i], diff[i + 1], diff[i + 2] = 220, 30, 30
        else:
            gray = (ar + ag + ab) // 3
            faded = min(255, 200 + gray // 5)
            diff[i] = diff[i + 1] = diff[i + 2] = faded

    comp_w = canvas_w * 3 + gap * 2
    comp = bytearray(b"\xff" * comp_w * canvas_h * 3)
    src_stride = canvas_w * 3
    dst_stride = comp_w * 3
    col2 = (canvas_w + gap) * 3
    col3 = (canvas_w * 2 + gap * 2) * 3
    for y in range(canvas_h):
        dst = y * dst_stride
        src = y * src_stride
        comp[dst : dst + src_stride] = sa[src : src + src_stride]
        comp[dst + col2 : dst + col2 + src_stride] = sb[src : src + src_stride]
        comp[dst + col3 : dst + col3 + src_stride] = diff[src : src + src_stride]

    out = fitz.Pixmap(fitz.csRGB, comp_w, canvas_h, bytes(comp), 0)
    return out.tobytes("png")


def _pad_white(samples: bytes, sw: int, sh: int, tw: int, th: int) -> bytearray:
    out = bytearray(b"\xff" * tw * th * 3)
    src_row = sw * 3
    dst_row = tw * 3
    for y in range(sh):
        out[y * dst_row : y * dst_row + src_row] = samples[
            y * src_row : y * src_row + src_row
        ]
    return out


def load_tolerances() -> tuple[float, dict[str, float]]:
    """Return (default_pct, per_fixture_pct) from tolerances.toml."""
    if not TOLERANCES_PATH.exists():
        return 0.03, {}
    data = tomllib.loads(TOLERANCES_PATH.read_text())
    default = float(data.get("defaults", {}).get("pixel_diff_pct", 0.03))
    fixtures = {name: float(value) for name, value in data.get("fixtures", {}).items()}
    return default, fixtures


def tolerance_for(name: str) -> float:
    default, fixtures = load_tolerances()
    return fixtures.get(name, default)


def discover_fixtures(root: Path = VISUAL_DIR) -> list[str]:
    """Return fixture names (path under tests/visual/, sans .html, posix-style)."""
    names: list[str] = []
    for path in sorted(root.rglob("*.html")):
        rel = path.relative_to(root)
        if rel.parts and rel.parts[0] == "ref":
            continue
        names.append(rel.with_suffix("").as_posix())
    return names


def fixture_path(name: str) -> Path:
    return VISUAL_DIR / f"{name}.html"


def ref_path(name: str, page: int) -> Path:
    return REF_DIR / f"{name}.page{page}.png"


def report_actual_path(name: str, page: int) -> Path:
    return REPORT_DIR / "actual" / f"{name}.page{page}.png"


def report_composite_path(name: str, page: int) -> Path:
    return REPORT_DIR / "composite" / f"{name}.page{page}.png"


def ensure_report_dirs() -> None:
    (REPORT_DIR / "actual").mkdir(parents=True, exist_ok=True)
    (REPORT_DIR / "composite").mkdir(parents=True, exist_ok=True)
