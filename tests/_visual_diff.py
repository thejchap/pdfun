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


def ensure_report_dirs() -> None:
    (REPORT_DIR / "actual").mkdir(parents=True, exist_ok=True)
