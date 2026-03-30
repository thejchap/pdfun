"""CLI commands for pdfun."""

from pathlib import Path

import click

from pdfun._core import Layout, PdfDocument


@click.group()
@click.version_option(package_name="pdfun")
def main() -> None:
    """Pure-Rust PDF generation from Python."""


@main.command()
@click.argument("input_path", type=click.Path(exists=True))
@click.option(
    "-o",
    "--output",
    required=True,
    type=click.Path(),
    help="Output PDF path.",
)
@click.option("--font", default="Helvetica", help="Font name.")
@click.option("--font-size", default=12.0, type=float, help="Font size in points.")
@click.option(
    "--page-width",
    default=612.0,
    type=float,
    help="Page width in points.",
)
@click.option(
    "--page-height",
    default=792.0,
    type=float,
    help="Page height in points.",
)
def text(  # noqa: PLR0913
    input_path: str,
    output: str,
    font: str,
    font_size: float,
    page_width: float,
    page_height: float,
) -> None:
    """Convert a text file to PDF."""
    content = Path(input_path).read_text()
    doc = PdfDocument()
    layout = Layout(doc, page_width=page_width, page_height=page_height)
    for paragraph in content.split("\n\n"):
        stripped = paragraph.strip()
        if stripped:
            layout.add_text(
                stripped,
                font=font,
                font_size=font_size,
                spacing_after=font_size,
            )
    layout.finish()
    doc.save(output)


@main.command()
@click.argument("input_path", type=click.Path(exists=True))
@click.option(
    "-o",
    "--output",
    required=True,
    type=click.Path(),
    help="Output PDF path.",
)
def render(input_path: str, output: str) -> None:
    """Render HTML to PDF."""
    from pdfun.html import HtmlDocument  # noqa: PLC0415

    doc = HtmlDocument(string=Path(input_path).read_text())
    doc.write_pdf(output)


@main.command()
@click.argument("input_path", type=click.Path(exists=True))
def info(input_path: str) -> None:
    """Inspect a PDF and print metadata."""
    msg = "info command not yet implemented"
    raise NotImplementedError(msg)
