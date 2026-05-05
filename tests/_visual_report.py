"""Static HTML report for the visual regression suite.

Each test appends a JSON line to ``target/visual-report/results.jsonl``;
on process exit we read it back and emit a single ``index.html`` showing
each fixture (sorted worst-diff first) with pdfun output, the WeasyPrint
reference, and the diff number side-by-side.

A small inline script handles the dark-mode toggle and status/name
filters, persisting both to localStorage so the choices survive across
manual reloads.
"""

from __future__ import annotations

import html as html_lib
import json
import shutil
from pathlib import Path

from tests._visual_diff import (
    REF_DIR,
    REPORT_DIR,
    composite_diff_png,
    report_composite_path,
)

RESULTS_PATH = REPORT_DIR / "results.jsonl"
INDEX_PATH = REPORT_DIR / "index.html"
VENDORED_REF_DIR = REPORT_DIR / "ref"

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
    for row in rows:
        _vendor_ref(row)
        _build_composite(row)
    INDEX_PATH.write_text(_render_html(rows), encoding="utf-8")


def _build_composite(row: dict[str, object]) -> None:
    """For FAIL rows, emit a single PNG showing actual | ref | diff overlay.

    The composite is the same image humans see in the report's third
    column and that an automated reviewer (Claude) reads as a single
    artifact instead of three separate PNGs. PASS rows skip it (no
    interesting diff) and MISSING-REF rows can't generate one.
    """
    if str(row.get("status")) != "FAIL":
        return
    actual = row.get("actual_path")
    ref = row.get("ref_path")
    if not actual or not ref:
        return
    actual_p = Path(str(actual))
    ref_p = Path(str(ref))
    if not actual_p.exists() or not ref_p.exists():
        return
    name = str(row.get("name", ""))
    page = _as_int(row.get("page"), default=1)
    dest = report_composite_path(name, page)
    dest.parent.mkdir(parents=True, exist_ok=True)
    png = composite_diff_png(actual_p.read_bytes(), ref_p.read_bytes())
    dest.write_bytes(png)
    row["composite_path"] = str(dest)


def _vendor_ref(row: dict[str, object]) -> None:
    """Copy the row's ref PNG inside REPORT_DIR so the report is portable.

    Refs live under ``tests/visual/ref/``, which is fine when viewing the
    report on the machine that generated it but breaks the moment the
    report is downloaded as a CI artifact. Copy each referenced file into
    ``target/visual-report/ref/`` and rewrite ``ref_path`` so the
    relative-path branch in ``_maybe_rel`` kicks in.
    """
    ref = row.get("ref_path")
    if not ref:
        return
    src = Path(str(ref))
    if not src.exists():
        return
    src_resolved = src.resolve()
    report_resolved = REPORT_DIR.resolve()
    try:
        src_resolved.relative_to(report_resolved)
    except ValueError:
        pass
    else:
        return  # already inside the report dir
    try:
        rel = src_resolved.relative_to(REF_DIR.resolve())
    except ValueError:
        rel = Path(src.name)
    dest = VENDORED_REF_DIR / rel
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dest)
    row["ref_path"] = str(dest)


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
  <title>pdfun visual regression</title>
  <style>
    :root {{
      --bg: #ffffff;
      --fg: #222222;
      --muted: #555555;
      --border: #e0e0e0;
      --border-soft: #dddddd;
      --th-bg: #fafafa;
      --placeholder-bg: #f5f5f5;
      --placeholder-fg: #888888;
      --placeholder-border: #cccccc;
      --input-bg: #ffffff;
      --input-fg: #222222;
      --pass-bg: #d4edda; --pass-fg: #155724;
      --fail-bg: #f8d7da; --fail-fg: #721c24;
      --missing-bg: #fff3cd; --missing-fg: #856404;
    }}
    @media (prefers-color-scheme: dark) {{
      :root {{
        --bg: #15171a;
        --fg: #e6e6e6;
        --muted: #9aa0a6;
        --border: #2a2d31;
        --border-soft: #2a2d31;
        --th-bg: #1c1f23;
        --placeholder-bg: #1c1f23;
        --placeholder-fg: #888888;
        --placeholder-border: #34383d;
        --input-bg: #1c1f23;
        --input-fg: #e6e6e6;
        --pass-bg: #143a1f; --pass-fg: #8fd19e;
        --fail-bg: #4a1d22; --fail-fg: #f1a7ad;
        --missing-bg: #4a3a14; --missing-fg: #f0d27a;
      }}
    }}
    html[data-theme="light"] {{
      --bg: #ffffff; --fg: #222222; --muted: #555555;
      --border: #e0e0e0; --border-soft: #dddddd; --th-bg: #fafafa;
      --placeholder-bg: #f5f5f5; --placeholder-fg: #888888;
      --placeholder-border: #cccccc;
      --input-bg: #ffffff; --input-fg: #222222;
      --pass-bg: #d4edda; --pass-fg: #155724;
      --fail-bg: #f8d7da; --fail-fg: #721c24;
      --missing-bg: #fff3cd; --missing-fg: #856404;
    }}
    html[data-theme="dark"] {{
      --bg: #15171a; --fg: #e6e6e6; --muted: #9aa0a6;
      --border: #2a2d31; --border-soft: #2a2d31; --th-bg: #1c1f23;
      --placeholder-bg: #1c1f23; --placeholder-fg: #888888;
      --placeholder-border: #34383d;
      --input-bg: #1c1f23; --input-fg: #e6e6e6;
      --pass-bg: #143a1f; --pass-fg: #8fd19e;
      --fail-bg: #4a1d22; --fail-fg: #f1a7ad;
      --missing-bg: #4a3a14; --missing-fg: #f0d27a;
    }}
    body {{
      font-family: -apple-system, system-ui, sans-serif;
      margin: 24px; color: var(--fg); background: var(--bg);
    }}
    h1 {{ margin: 0 0 4px; font-size: 20px; }}
    .summary {{ color: var(--muted); margin-bottom: 12px; font-size: 13px; }}
    .controls {{
      display: flex; flex-wrap: wrap; align-items: center; gap: 12px;
      margin-bottom: 16px; font-size: 12px; color: var(--muted);
    }}
    .controls label {{ display: inline-flex; align-items: center; gap: 4px; }}
    .controls input[type="text"], .controls select {{
      background: var(--input-bg); color: var(--input-fg);
      border: 1px solid var(--border); border-radius: 3px;
      padding: 4px 6px; font: inherit;
    }}
    .controls input[type="text"] {{ width: 200px; }}
    .controls .group {{
      display: inline-flex; align-items: center; gap: 8px;
      padding: 4px 8px; border: 1px solid var(--border); border-radius: 3px;
    }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{
      border: 1px solid var(--border); padding: 8px;
      vertical-align: top; font-size: 12px;
    }}
    th {{ background: var(--th-bg); text-align: left; }}
    tr.hidden {{ display: none; }}
    .images {{ display: flex; gap: 8px; }}
    .images figure {{ margin: 0; flex: 1; min-width: 0; }}
    .images img {{
      max-width: 100%; height: auto; border: 1px solid var(--border-soft);
    }}
    .images figcaption {{
      font-size: 10px; color: var(--muted); margin-top: 2px;
    }}
    .composite {{ margin: 0; min-width: 0; }}
    .composite img {{
      max-width: 100%; height: auto; border: 1px solid var(--border-soft);
    }}
    .composite figcaption {{
      font-size: 10px; color: var(--muted); margin-top: 2px;
    }}
    .badge {{
      display: inline-block; padding: 2px 6px; border-radius: 3px;
      font-weight: bold; font-size: 11px;
    }}
    .pass {{ background: var(--pass-bg); color: var(--pass-fg); }}
    .fail {{ background: var(--fail-bg); color: var(--fail-fg); }}
    .missing {{ background: var(--missing-bg); color: var(--missing-fg); }}
    .name {{ font-family: ui-monospace, monospace; }}
    .num {{
      font-variant-numeric: tabular-nums; text-align: right;
      white-space: nowrap;
    }}
    .placeholder {{
      display: flex; align-items: center; justify-content: center;
      width: 100%; min-height: 200px;
      background: var(--placeholder-bg); color: var(--placeholder-fg);
      font-size: 11px;
      border: 1px dashed var(--placeholder-border);
    }}
  </style>
  <script>
    // applied as early as possible so the page never flashes the wrong theme
    (function () {{
      try {{
        var t = localStorage.getItem('pdfun-vr-theme');
        if (t === 'light' || t === 'dark') {{
          document.documentElement.setAttribute('data-theme', t);
        }}
      }} catch (e) {{}}
    }})();
  </script>
</head>
<body>
  <h1>pdfun visual regression</h1>
  <div class="summary">
    <span id="summary-counts">{html_lib.escape(summary)}</span>
  </div>
  <div class="controls">
    <span class="group">
      <span>show:</span>
      <label><input type="checkbox" data-status="FAIL" checked> FAIL</label>
      <label>
        <input type="checkbox" data-status="MISSING-REF" checked> MISSING-REF
      </label>
      <label><input type="checkbox" data-status="PASS" checked> PASS</label>
    </span>
    <label>
      fixture:
      <input type="text" id="filter-name" placeholder="substring match…">
    </label>
    <label>
      theme:
      <select id="theme-select">
        <option value="auto">auto</option>
        <option value="light">light</option>
        <option value="dark">dark</option>
      </select>
    </label>
  </div>
  <table>
    <thead>
      <tr>
        <th>Status</th>
        <th>Fixture</th>
        <th>Page</th>
        <th class="num">Diff %</th>
        <th class="num">Tolerance</th>
        <th>pdfun &middot; WeasyPrint reference</th>
        <th>diff overlay</th>
      </tr>
    </thead>
    <tbody id="rows">
{body_rows}
    </tbody>
  </table>
  <script>
    (function () {{
      var KEY_STATUS = 'pdfun-vr-status';
      var KEY_NAME = 'pdfun-vr-name';
      var KEY_THEME = 'pdfun-vr-theme';
      var statusBoxes = document.querySelectorAll('input[data-status]');
      var nameInput = document.getElementById('filter-name');
      var themeSelect = document.getElementById('theme-select');
      var rows = document.querySelectorAll('#rows tr');
      var summary = document.getElementById('summary-counts');

      function load(key, fallback) {{
        try {{
          var v = localStorage.getItem(key);
          return v === null ? fallback : v;
        }} catch (e) {{ return fallback; }}
      }}
      function save(key, value) {{
        try {{ localStorage.setItem(key, value); }} catch (e) {{}}
      }}

      // restore status filter (comma-separated list of allowed statuses)
      var savedStatus = load(KEY_STATUS, null);
      if (savedStatus !== null) {{
        var allowed = savedStatus.split(',').filter(Boolean);
        statusBoxes.forEach(function (cb) {{
          cb.checked = allowed.indexOf(cb.dataset.status) !== -1;
        }});
      }}
      nameInput.value = load(KEY_NAME, '');
      themeSelect.value = load(KEY_THEME, 'auto');

      function applyTheme() {{
        var v = themeSelect.value;
        if (v === 'auto') {{
          document.documentElement.removeAttribute('data-theme');
        }} else {{
          document.documentElement.setAttribute('data-theme', v);
        }}
      }}
      applyTheme();

      function applyFilters() {{
        var allowed = {{}};
        statusBoxes.forEach(function (cb) {{
          if (cb.checked) allowed[cb.dataset.status] = true;
        }});
        var needle = nameInput.value.trim().toLowerCase();
        var shown = 0, total = rows.length;
        var counts = {{ 'PASS': 0, 'FAIL': 0, 'MISSING-REF': 0 }};
        rows.forEach(function (tr) {{
          var status = tr.dataset.status || '';
          var name = (tr.dataset.name || '').toLowerCase();
          var ok = (allowed[status] === true) &&
                   (needle === '' || name.indexOf(needle) !== -1);
          tr.classList.toggle('hidden', !ok);
          if (ok) {{
            shown++;
            if (counts.hasOwnProperty(status)) counts[status]++;
          }}
        }});
        summary.textContent =
          'shown ' + shown + ' / ' + total + ' · ' +
          'PASS: ' + counts.PASS + ' · ' +
          'FAIL: ' + counts.FAIL + ' · ' +
          'MISSING-REF: ' + counts['MISSING-REF'];
      }}

      statusBoxes.forEach(function (cb) {{
        cb.addEventListener('change', function () {{
          var on = [];
          statusBoxes.forEach(function (b) {{
            if (b.checked) on.push(b.dataset.status);
          }});
          save(KEY_STATUS, on.join(','));
          applyFilters();
        }});
      }});
      nameInput.addEventListener('input', function () {{
        save(KEY_NAME, nameInput.value);
        applyFilters();
      }});
      themeSelect.addEventListener('change', function () {{
        save(KEY_THEME, themeSelect.value);
        applyTheme();
      }});

      applyFilters();
    }})();
  </script>
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
    composite_rel = _maybe_rel(row.get("composite_path"))
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
    composite_block = (
        f"""
        <td>
          <figure class="composite">
            <img src="{composite_rel}" alt="diff overlay page {page}">
            <figcaption>actual &middot; reference &middot; diff overlay</figcaption>
          </figure>
        </td>"""
        if composite_rel
        else "<td></td>"
    )
    return f"""<tr data-status="{html_lib.escape(status)}" data-name="{name}">
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
        </td>{composite_block}
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
