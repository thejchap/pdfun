"""Low-level Layout API (no HTML). Run with ``uv run python examples/layout_api.py``."""

from pdfun import Layout, PdfDocument, TextRun

doc = PdfDocument()
doc.set_title("Invoice #1042")

layout = Layout(doc)
layout.add_text("Invoice #1042", font_size=24.0, spacing_after=12.0)
layout.add_paragraph(
    runs=[
        TextRun("Bill to: ", font_name="Helvetica-Bold"),
        TextRun("Acme Corp."),
    ],
)
layout.add_paragraph(
    runs=[
        TextRun("Amount due: ", font_name="Helvetica-Bold"),
        TextRun("$1,234.56"),
    ],
)
layout.finish()

doc.save("layout_api.pdf")
print("wrote layout_api.pdf")
