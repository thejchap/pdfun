"""Static HTML report for the visual regression suite.

Each test appends a JSON line to ``target/visual-report/results.jsonl``;
on process exit we read it back and emit a single ``index.html`` showing
each fixture (sorted worst-diff first) with pdfun output, the WeasyPrint
reference, and the diff number side-by-side.

No JavaScript. Auto-refreshes every 2 s so a left-open browser tab picks
up the next test run.
"""

from __future__ import annotations

import html as html_lib
import json
from pathlib import Path

from tests._visual_diff import REPORT_DIR

RESULTS_PATH = REPORT_DIR / "results.jsonl"
INDEX_PATH = REPORT_DIR / "index.html"

STATUS_ORDER = {"FAIL": 0, "MISSING-REF": 1, "PASS": 2}


def append_result(record: dict[str, object]) -> None:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    with RESULTS_PATH.open("a", encoding="utf-8") as fh:
        fh.write(json.dumps(record) + "\n")


def reset_results() -> None:
    REPORT_DIR.mkdir(parents=True, exist_ok=True)
    RESULTS_PATH.write_text("", encoding="utf-8")


def write_report() -> None:
    if not RESULTS_PATH.exists():
        return
    rows: list[dict[str, object]] = []
    for raw in RESULTS_PATH.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line:
            continue
        rows.append(json.loads(line))
    if not rows:
        return

    def _sort_key(r: dict[str, object]) -> tuple[int, float, str, int]:
        diff = r.get("diff_pct")
        diff_val = float(diff) if isinstance(diff, (int, float)) else 0.0
        return (
            STATUS_ORDER.get(str(r.get("status", "PASS")), 99),
            -diff_val,
            str(r.get("name", "")),
            _as_int(r.get("page"), default=1),
        )

    rows.sort(key=_sort_key)
    INDEX_PATH.write_text(_render_html(rows), encoding="utf-8")


def _render_html(rows: list[dict[str, object]]) -> str:
    counts: dict[str, int] = {"PASS": 0, "FAIL": 0, "MISSING-REF": 0}
    for row in rows:
        counts[str(row.get("status", "PASS"))] = (
            counts.get(str(row.get("status", "PASS")), 0) + 1
        )
    body_rows = "\n".join(_render_row(row) for row in rows)
    summary = (
        f"PASS: {counts.get('PASS', 0)} · "
        f"FAIL: {counts.get('FAIL', 0)} · "
        f"MISSING-REF: {counts.get('MISSING-REF', 0)}"
    )
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta http-equiv="refresh" content="2">
  <title>pdfun visual regression</title>
  <style>
    body {{
      font-family: -apple-system, system-ui, sans-serif;
      margin: 24px; color: #222;
    }}
    h1 {{ margin: 0 0 4px; font-size: 20px; }}
    .summary {{ color: #555; margin-bottom: 16px; font-size: 13px; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{
      border: 1px solid #e0e0e0; padding: 8px;
      vertical-align: top; font-size: 12px;
    }}
    th {{ background: #fafafa; text-align: left; }}
    .images {{ display: flex; gap: 8px; }}
    .images figure {{ margin: 0; flex: 1; min-width: 0; }}
    .images img {{ max-width: 100%; height: auto; border: 1px solid #ddd; }}
    .images figcaption {{ font-size: 10px; color: #666; margin-top: 2px; }}
    .badge {{
      display: inline-block; padding: 2px 6px; border-radius: 3px;
      font-weight: bold; font-size: 11px;
    }}
    .pass {{ background: #d4edda; color: #155724; }}
    .fail {{ background: #f8d7da; color: #721c24; }}
    .missing {{ background: #fff3cd; color: #856404; }}
    .name {{ font-family: ui-monospace, monospace; }}
    .num {{
      font-variant-numeric: tabular-nums; text-align: right;
      white-space: nowrap;
    }}
    .placeholder {{
      display: flex; align-items: center; justify-content: center;
      width: 100%; min-height: 200px;
      background: #f5f5f5; color: #888; font-size: 11px;
      border: 1px dashed #ccc;
    }}
  </style>
</head>
<body>
  <h1>pdfun visual regression</h1>
  <div class="summary">{html_lib.escape(summary)}. Page auto-refreshes every 2 s.</div>
  <table>
    <thead>
      <tr>
        <th>Status</th>
        <th>Fixture</th>
        <th>Page</th>
        <th class="num">Diff %</th>
        <th class="num">Tolerance</th>
        <th>pdfun &middot; WeasyPrint reference</th>
      </tr>
    </thead>
    <tbody>
{body_rows}
    </tbody>
  </table>
</body>
</html>
"""


def _render_row(row: dict[str, object]) -> str:
    status = str(row.get("status", "PASS"))
    badge_class = {"PASS": "pass", "FAIL": "fail", "MISSING-REF": "missing"}.get(
        status, "missing"
    )
    name = html_lib.escape(str(row.get("name", "")))
    page = _as_int(row.get("page"), default=1)
    diff_str = _percent(row.get("diff_pct"))
    tol_str = _percent(row.get("tolerance"))
    actual_rel = _maybe_rel(row.get("actual_path"))
    ref_rel = _maybe_rel(row.get("ref_path"))
    actual_img = (
        f'<img src="{actual_rel}" alt="pdfun page {page}">'
        if actual_rel
        else '<div class="placeholder">no pdfun output</div>'
    )
    ref_img = (
        f'<img src="{ref_rel}" alt="reference page {page}">'
        if ref_rel
        else (
            '<div class="placeholder">no reference yet — run bless_visual_refs.py</div>'
        )
    )
    return f"""<tr>
        <td><span class="badge {badge_class}">{status}</span></td>
        <td class="name">{name}</td>
        <td class="num">{page}</td>
        <td class="num">{diff_str}</td>
        <td class="num">{tol_str}</td>
        <td>
          <div class="images">
            <figure>{actual_img}<figcaption>pdfun</figcaption></figure>
            <figure>{ref_img}<figcaption>WeasyPrint</figcaption></figure>
          </div>
        </td>
      </tr>"""


def _as_int(value: object, *, default: int) -> int:
    if isinstance(value, bool):
        return int(value)
    if isinstance(value, int):
        return value
    if isinstance(value, float):
        return int(value)
    return default


def _percent(value: object) -> str:
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return f"{float(value) * 100:.2f}%"
    return "—"


def _maybe_rel(path_value: object) -> str | None:
    if not path_value:
        return None
    path = Path(str(path_value))
    if not path.exists():
        return None
    try:
        return path.resolve().relative_to(REPORT_DIR.resolve()).as_posix()
    except ValueError:
        return path.resolve().as_posix()
