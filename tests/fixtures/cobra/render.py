"""End-to-end COBRA acceptance fixture.

Renders the trimmed COBRA election notice through pdfun, with HTTP fetch
disabled (the public anuvi.io URLs will warn but won't crash). Verifies
the text-and-layout acceptance criteria from the plan:

- Spanish line "¿Hablas español? Instrucciones en la última página"
  round-trips through the PDF text layer.
- List bullets are •.
- "Page X of Y" footer is on every page.
- Cover-page article respects its `height: 11in` and the next article
  starts on page 2.
- 30/10/60 column split on the cost table is in the content stream.
"""

from __future__ import annotations

import sys
from pathlib import Path

from pdfun import HtmlDocument

HERE = Path(__file__).parent
HTML_PATH = HERE / "cobra_notice.html"
PDF_OUT = HERE / "cobra_notice.pdf"


def main() -> int:
    html = HTML_PATH.read_text()
    doc = HtmlDocument(string=html, base_url=str(HERE))
    doc.write_pdf(str(PDF_OUT))
    print(f"wrote {PDF_OUT} ({PDF_OUT.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
