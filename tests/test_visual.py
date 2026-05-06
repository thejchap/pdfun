"""Visual regression tests — render pdfun, diff against committed WeasyPrint refs.

Each fixture under ``tests/visual/`` (any depth) is rendered with pdfun,
rasterized per page at 100 DPI, and compared against
``tests/visual/ref/<name>.page<N>.png`` — committed PNGs produced by
``tools/bless_visual_refs.py`` running WeasyPrint inside a pinned Docker
image. Per-fixture tolerances live in ``tests/visual/tolerances.toml``.

Fixtures are grouped into four category-level tests so ``tryke``'s static
discovery sees stable test names while the actual fixture list grows over
time. Failures use soft assertions, so one bad fixture doesn't hide the
rest. The harness always emits per-page artifacts plus a JSON line into
``target/visual-report/results.jsonl``; an ``atexit`` hook then renders
``target/visual-report/index.html`` (auto-refreshing every 2 s) so a
left-open browser tab tracks progress as you iterate. Open it with::

    open target/visual-report/index.html
"""

from __future__ import annotations

import atexit
from pathlib import Path

from tryke import describe, expect, test

from tests._visual_diff import (
    VISUAL_DIR,
    discover_fixtures,
    ensure_report_dirs,
    fixture_path,
    ref_path,
    render_pdfun_pages,
    report_actual_path,
    tolerance_for,
    tolerant_pixel_diff,
)
from tests._visual_report import append_result, reset_results, write_report

ensure_report_dirs()
reset_results()
# tryke runs tests inside a long-lived worker process whose atexit hooks
# may not fire when it's torn down by the runner; flush the report after
# every category test so the on-disk report is always current.
atexit.register(write_report)


def _record(
    name: str,
    page: int,
    *,
    status: str,
    diff_pct: float | None,
    tolerance: float,
    actual: Path | None,
    reference: Path | None,
) -> None:
    append_result(
        {
            "name": name,
            "page": page,
            "status": status,
            "diff_pct": diff_pct,
            "tolerance": tolerance,
            "actual_path": str(actual) if actual else None,
            "ref_path": str(reference) if reference else None,
        }
    )


def _check_one(name: str) -> list[str]:
    """Render and diff a single fixture; return human-readable failure lines."""
    html = fixture_path(name).read_text()
    pages = render_pdfun_pages(html)
    tol = tolerance_for(name)
    failures: list[str] = []
    for index, png_bytes in enumerate(pages, start=1):
        actual_out = report_actual_path(name, index)
        actual_out.parent.mkdir(parents=True, exist_ok=True)
        actual_out.write_bytes(png_bytes)

        ref = ref_path(name, index)
        if not ref.exists():
            _record(
                name,
                index,
                status="MISSING-REF",
                diff_pct=None,
                tolerance=tol,
                actual=actual_out,
                reference=None,
            )
            continue

        diff = tolerant_pixel_diff(png_bytes, ref.read_bytes())
        status = "PASS" if diff <= tol else "FAIL"
        _record(
            name,
            index,
            status=status,
            diff_pct=diff,
            tolerance=tol,
            actual=actual_out,
            reference=ref,
        )
        if status == "FAIL":
            failures.append(f"{name} page {index}: diff={diff:.4f} > tol={tol:.4f}")
    return failures


def _check_category(prefix: str | None) -> None:
    """Run every fixture matching `prefix` (None means top-level fixtures only).

    Soft-assertions accumulate so all failing fixtures show up in one go.
    """
    matched: list[str] = []
    for name in discover_fixtures():
        if prefix is None:
            if "/" not in name:
                matched.append(name)
        elif name.startswith(prefix + "/"):
            matched.append(name)

    failures: list[str] = []
    for name in matched:
        failures.extend(_check_one(name))

    write_report()
    summary = "; ".join(failures) if failures else ""
    expect(summary, name="visual_diffs").to_equal("")


# Eagerly create the category subdirectories under tests/visual/ so that
# discover_fixtures() returns a stable shape even when fixtures haven't
# been authored yet — keeps the test names stable for tryke discovery.
for sub in ("progressive", "wpt", "realworld"):
    (VISUAL_DIR / sub).mkdir(parents=True, exist_ok=True)


with describe("visual regression"):

    @test
    def visual_legacy():
        """Top-level fixtures shipped before the WeasyPrint loop existed."""
        _check_category(None)

    @test
    def visual_progressive():
        """Hand-rolled corpus that introduces one feature at a time."""
        _check_category("progressive")

    @test
    def visual_wpt():
        """Curated subset of W3C / WPT reference tests."""
        _check_category("wpt")

    @test
    def visual_realworld():
        """End-to-end stress tests: real documents, loose tolerances."""
        _check_category("realworld")
