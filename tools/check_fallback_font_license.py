#!/usr/bin/env python3
"""WS-1B license sanity: every bundled font asset must ship its license.

The Bitstream Vera / DejaVu license is permissive but redistribution
still requires the copyright + license text. This script asserts that
for each `assets/fonts/<name>.ttf` (or `.otf`) committed in-tree, a
sibling `<name>-LICENSE` file exists and is non-empty.

Run via `python tools/check_fallback_font_license.py` (CI hook) or
indirectly through the `test_bundled_fallback_license` test in
`tests/test_html.py`.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
ASSETS_FONT_DIR = REPO_ROOT / "assets" / "fonts"


def check() -> list[str]:
    """Return a list of human-readable error messages; empty list = OK."""
    errors: list[str] = []
    if not ASSETS_FONT_DIR.is_dir():
        # No bundled fonts at all — nothing to validate.
        return errors

    font_files = sorted(
        p for p in ASSETS_FONT_DIR.iterdir() if p.suffix.lower() in {".ttf", ".otf"}
    )
    if not font_files:
        errors.append(f"no font files found under {ASSETS_FONT_DIR}")
        return errors

    for font in font_files:
        license_path = font.with_name(f"{font.stem}-LICENSE")
        if not license_path.exists():
            errors.append(
                f"missing license file: {license_path} (required alongside {font.name})"
            )
            continue
        try:
            size = license_path.stat().st_size
        except OSError as e:
            errors.append(f"cannot stat {license_path}: {e}")
            continue
        if size == 0:
            errors.append(f"license file is empty: {license_path}")

    return errors


def main() -> int:
    """Entry point: run `check()` and surface any errors on stderr."""
    errors = check()
    if errors:
        for e in errors:
            print(f"check_fallback_font_license: ERROR: {e}", file=sys.stderr)
        return 1
    print("check_fallback_font_license: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
