"""Generate docs/PARITY.md from tools/parity/catalog.toml + inline test markers.

Usage:
    python tools/parity/generate.py            regenerate docs/PARITY.md
    python tools/parity/generate.py --check    verify docs/PARITY.md is up to date

Marker syntax (inline in tests):
    Visual (HTML): <!-- spec: CSS 2.1 §8.3.1; behaviors: box-collapse-siblings -->
    Python:        # spec: CSS 2.1 §8.3.1; behaviors: box-collapse-siblings

Multiple ids are comma-separated. Tests without markers are ignored.
Markers referencing unknown behavior ids fail the generator.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
CATALOG_PATH = REPO_ROOT / "tools" / "parity" / "catalog.toml"
OUTPUT_PATH = REPO_ROOT / "docs" / "PARITY.md"
TESTS_ROOT = REPO_ROOT / "tests"

_WP_EMOJI = {"full": "✅", "partial": "🟡", "none": "❌"}
_DEF_RE = re.compile(r"def\s+(\w+)\s*\(")


@dataclass
class Behavior:
    """One row in the matrix — a single spec behavior we track."""

    id: str
    title: str
    spec: str
    section: str
    weasyprint: str
    implemented: bool


@dataclass
class Section:
    """A group of related behaviors, usually a spec chapter or module."""

    spec: str
    section: str
    title: str
    behaviors: list[Behavior] = field(default_factory=list)


@dataclass
class TestRef:
    """Reference to a test file or test function."""

    path: Path
    name: str

    def display(self) -> str:
        """Render a human-readable repo-relative reference string."""
        rel = self.path.relative_to(REPO_ROOT).as_posix()
        if self.name and self.name != self.path.stem:
            return f"{rel}::{self.name}"
        return rel


def load_catalog(path: Path) -> list[Section]:
    """Parse catalog.toml into Section objects."""
    data = tomllib.loads(path.read_text())
    sections: list[Section] = []
    for s in data.get("section", []):
        sec = Section(spec=s["spec"], section=s.get("section", ""), title=s["title"])
        for b in s.get("behavior", []):
            sec.behaviors.append(
                Behavior(
                    id=b["id"],
                    title=b["title"],
                    spec=sec.spec,
                    section=b.get("section", sec.section),
                    weasyprint=b["weasyprint"],
                    implemented=b["implemented"],
                )
            )
        sections.append(sec)
    return sections


def parse_marker(raw: str) -> list[str] | None:
    """Extract behavior ids from a single marker line, or None if not a marker."""
    text = raw.strip().replace("<!--", "").replace("-->", "").strip()
    if text.startswith("#"):
        text = text.lstrip("#").strip()
    if not text.startswith("spec:"):
        return None
    if ";" not in text:
        return None
    _, _, after = text.partition(";")
    after = after.strip()
    if after.startswith("behaviors:"):
        ids_str = after[len("behaviors:") :]
    elif after.startswith("behavior:"):
        ids_str = after[len("behavior:") :]
    else:
        return None
    return [s.strip() for s in ids_str.split(",") if s.strip()]


def scan_html_tests(root: Path) -> list[tuple[TestRef, list[str]]]:
    """Find spec markers in the first non-blank line of each *.html under root."""
    results: list[tuple[TestRef, list[str]]] = []
    for html_path in sorted(root.rglob("*.html")):
        with html_path.open() as f:
            for line in f:
                if line.strip():
                    ids = parse_marker(line)
                    if ids:
                        results.append((TestRef(html_path, html_path.stem), ids))
                    break
    return results


def scan_python_tests(root: Path) -> list[tuple[TestRef, list[str]]]:
    """Find `# spec:` markers immediately preceding `@test` or `def test_*`."""
    results: list[tuple[TestRef, list[str]]] = []
    for py_path in sorted(root.rglob("*.py")):
        if py_path.name == "__init__.py":
            continue
        lines = py_path.read_text().splitlines()
        for i, line in enumerate(lines):
            ids = parse_marker(line)
            if ids is None:
                continue
            name = _find_test_name(lines, i + 1)
            if name is not None:
                results.append((TestRef(py_path, name), ids))
    return results


def _find_test_name(lines: list[str], start: int) -> str | None:
    """Find the first `def <name>(` after `start`, skipping comments/decorators."""
    j = start
    while j < len(lines):
        stripped = lines[j].strip()
        if not stripped or stripped.startswith("#"):
            j += 1
            continue
        if stripped.startswith("@test"):
            j += 1
            continue
        if stripped.startswith("@"):
            j += 1
            continue
        m = _DEF_RE.match(stripped)
        if m and m.group(1).startswith(("test_", "test")):
            return m.group(1)
        if m:
            return m.group(1)
        return None
    return None


def build_coverage(
    catalog: list[Section], tests: list[tuple[TestRef, list[str]]]
) -> tuple[dict[str, list[TestRef]], list[str]]:
    """Map behavior id -> tests. Return (coverage, unknown_references)."""
    valid = {b.id for s in catalog for b in s.behaviors}
    coverage: dict[str, list[TestRef]] = {bid: [] for bid in valid}
    unknown: list[str] = []
    for ref, ids in tests:
        for bid in ids:
            if bid in valid:
                coverage[bid].append(ref)
            else:
                unknown.append(f"{ref.display()} references unknown behavior '{bid}'")
    return coverage, unknown


def _section_heading(sec: Section) -> str:
    """Render a top-level section heading string."""
    if sec.section:
        return f"{sec.spec} §{sec.section} — {sec.title}"
    return f"{sec.spec} — {sec.title}"


def render_matrix(catalog: list[Section], coverage: dict[str, list[TestRef]]) -> str:
    """Render the matrix as a markdown document."""
    total = sum(len(s.behaviors) for s in catalog)
    implemented = sum(1 for s in catalog for b in s.behaviors if b.implemented)
    tested = sum(1 for s in catalog for b in s.behaviors if coverage.get(b.id))

    tested_legend = (
        "| `Tested` | ✅ (N) tests referencing this behavior · "
        "⚠️ implemented but untested · — not applicable |"
    )
    generated_note = (
        "Auto-generated from "
        "[`tools/parity/catalog.toml`](../tools/parity/catalog.toml) "
        "plus inline `spec:` markers in tests. "
        "Run `uv run python tools/parity/generate.py` to regenerate."
    )

    out: list[str] = [
        "# Feature Parity Matrix",
        "",
        generated_note,
        "",
        (
            f"**Summary:** {implemented}/{total} behaviors implemented · "
            f"{tested}/{total} tested · "
            "WeasyPrint comparison hand-curated in catalog."
        ),
        "",
        "## Legend",
        "",
        "| Column | Meaning |",
        "|--------|---------|",
        "| `Spec §` | Sub-section within the spec, if applicable |",
        "| `WeasyPrint` | ✅ full · 🟡 partial · ❌ none |",
        "| `pdfun` | ✅ implemented · ❌ not implemented |",
        tested_legend,
        "",
    ]

    for sec in catalog:
        out.append(f"## {_section_heading(sec)}")
        out.append("")
        out.append("| Behavior | Spec § | WeasyPrint | pdfun | Tested |")
        out.append("|----------|:------:|:----------:|:-----:|:-------|")
        for b in sec.behaviors:
            refs = coverage.get(b.id, [])
            wp = _WP_EMOJI[b.weasyprint]
            impl = "✅" if b.implemented else "❌"
            if refs:
                links = ", ".join(f"`{r.display()}`" for r in refs)
                tested_cell = f"✅ ({len(refs)}) {links}"
            elif b.implemented:
                tested_cell = "⚠️ untested"
            else:
                tested_cell = "—"
            section_cell = b.section or "—"
            title = b.title.replace("|", "\\|")
            out.append(f"| {title} | {section_cell} | {wp} | {impl} | {tested_cell} |")
        out.append("")
    return "\n".join(out) + "\n"


def main(argv: list[str] | None = None) -> int:
    """CLI entry point."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="exit non-zero if docs/PARITY.md is out of date or markers are invalid",
    )
    args = parser.parse_args(argv)

    catalog = load_catalog(CATALOG_PATH)
    all_tests = scan_html_tests(TESTS_ROOT) + scan_python_tests(TESTS_ROOT)
    coverage, unknown = build_coverage(catalog, all_tests)

    if unknown:
        print("ERROR: unknown behavior references in test markers:", file=sys.stderr)
        for msg in unknown:
            print(f"  {msg}", file=sys.stderr)
        print("Update tools/parity/catalog.toml or fix the marker.", file=sys.stderr)
        return 2

    content = render_matrix(catalog, coverage)
    rel = OUTPUT_PATH.relative_to(REPO_ROOT).as_posix()

    if args.check:
        if not OUTPUT_PATH.exists():
            print(
                f"ERROR: {rel} is missing. Run: uv run python tools/parity/generate.py",
                file=sys.stderr,
            )
            return 1
        current = OUTPUT_PATH.read_text()
        if current != content:
            print(f"ERROR: {rel} is out of date.", file=sys.stderr)
            print("Run: uv run python tools/parity/generate.py", file=sys.stderr)
            return 1
        return 0

    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(content)
    print(f"wrote {rel}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
