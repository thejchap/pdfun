"""Re-bless WeasyPrint reference PNGs for the visual regression suite.

Renders each fixture under ``tests/visual/`` to PDF inside a pinned
WeasyPrint Docker image, then rasterizes every page on the host with
PyMuPDF and writes ``tests/visual/ref/<name>.page<N>.png``. Refs are
committed to the repo so CI doesn't need WeasyPrint installed.

Usage::

    uv run python tools/bless_visual_refs.py                # all fixtures
    uv run python tools/bless_visual_refs.py progressive    # one subdirectory
    uv run python tools/bless_visual_refs.py progressive/02_block_model

The first run builds the Docker image (``pdfun-weasyprint:<digest>``),
caching it locally; subsequent runs just spin up containers. Requires
Docker on the host. Nothing else.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import fitz

REPO_ROOT = Path(__file__).resolve().parents[1]
TESTS_VISUAL = REPO_ROOT / "tests" / "visual"
REF_DIR = TESTS_VISUAL / "ref"
DOCKERFILE = REPO_ROOT / "tools" / "weasyprint.Dockerfile"
CACHE_DIR = REPO_ROOT / ".cache" / "weasyprint"
DPI = 100  # must match tests/_visual_diff.DEFAULT_DPI


def main() -> int:
    """Render fixtures via WeasyPrint in Docker, write reference PNGs."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "selectors",
        nargs="*",
        help="Optional fixture selectors (path under tests/visual/, sans .html). "
        "Matches as a prefix; empty means everything.",
    )
    parser.add_argument(
        "--dpi", type=int, default=DPI, help="Rasterization DPI (default: 100)"
    )
    args = parser.parse_args()

    if shutil.which("docker") is None:
        sys.stderr.write("error: docker is required to bless references\n")
        return 1

    fixtures = _discover_fixtures(args.selectors)
    if not fixtures:
        sys.stderr.write("no fixtures matched\n")
        return 1

    image_tag = _ensure_image()
    CACHE_DIR.mkdir(parents=True, exist_ok=True)

    failures: list[str] = []
    for name in fixtures:
        try:
            pages = _render_fixture(name, image_tag=image_tag, dpi=args.dpi)
        except subprocess.CalledProcessError as exc:
            failures.append(f"{name}: docker run failed ({exc.returncode})")
            continue
        except Exception as exc:  # noqa: BLE001
            failures.append(f"{name}: {type(exc).__name__}: {exc}")
            continue

        # Wipe any older page PNGs for this fixture so removed pages don't linger.
        for stale in REF_DIR.glob(f"{name}.page*.png"):
            stale.unlink()
        for index, png_bytes in enumerate(pages, start=1):
            out = REF_DIR / f"{name}.page{index}.png"
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_bytes(png_bytes)
        print(f"  blessed {name} ({len(pages)} page{'s' if len(pages) != 1 else ''})")

    if failures:
        sys.stderr.write("\nfailed fixtures:\n")
        for line in failures:
            sys.stderr.write(f"  {line}\n")
        return 1
    return 0


def _discover_fixtures(selectors: list[str]) -> list[str]:
    all_names: list[str] = []
    for path in sorted(TESTS_VISUAL.rglob("*.html")):
        rel = path.relative_to(TESTS_VISUAL)
        if rel.parts and rel.parts[0] == "ref":
            continue
        all_names.append(rel.with_suffix("").as_posix())
    if not selectors:
        return all_names
    matched: list[str] = []
    for selector in selectors:
        sel = selector.removesuffix(".html")
        for name in all_names:
            if (name == sel or name.startswith(sel + "/")) and name not in matched:
                matched.append(name)
    return matched


def _ensure_image() -> str:
    digest = hashlib.sha256(DOCKERFILE.read_bytes()).hexdigest()[:12]
    tag = f"pdfun-weasyprint:{digest}"
    inspect = subprocess.run(
        ["docker", "image", "inspect", tag],
        capture_output=True,
        check=False,
    )
    if inspect.returncode == 0:
        return tag
    print(f"building {tag} from {DOCKERFILE.name} (one-time)…")
    subprocess.run(
        [
            "docker",
            "build",
            "-t",
            tag,
            "-f",
            str(DOCKERFILE),
            str(DOCKERFILE.parent),
        ],
        check=True,
    )
    return tag


def _render_fixture(name: str, *, image_tag: str, dpi: int) -> list[bytes]:
    html_path = TESTS_VISUAL / f"{name}.html"
    if not html_path.exists():
        msg = f"fixture not found: {html_path}"
        raise FileNotFoundError(msg)

    pdf_dir = CACHE_DIR / Path(name).parent
    pdf_dir.mkdir(parents=True, exist_ok=True)
    pdf_path = CACHE_DIR / f"{name}.pdf"

    rel_html = html_path.relative_to(REPO_ROOT).as_posix()
    rel_pdf = pdf_path.relative_to(REPO_ROOT).as_posix()

    with tempfile.TemporaryDirectory():
        subprocess.run(
            [
                "docker",
                "run",
                "--rm",
                "-v",
                f"{REPO_ROOT}:/work",
                image_tag,
                f"/work/{rel_html}",
                f"/work/{rel_pdf}",
            ],
            check=True,
            capture_output=True,
        )

    return _rasterize(pdf_path.read_bytes(), dpi=dpi)


def _rasterize(pdf_bytes: bytes, *, dpi: int) -> list[bytes]:
    scale = dpi / 72.0
    matrix = fitz.Matrix(scale, scale)
    doc = fitz.open(stream=pdf_bytes, filetype="pdf")
    try:
        return [page.get_pixmap(matrix=matrix).tobytes("png") for page in doc]
    finally:
        doc.close()


if __name__ == "__main__":
    sys.exit(main())
