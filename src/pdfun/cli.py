import click


@click.group()
@click.version_option(package_name="pdfun")
def main():
    """pdfun -- pure-Rust PDF generation from Python."""


@main.command()
@click.argument("input", type=click.Path(exists=True))
@click.option(
    "-o", "--output", required=True, type=click.Path(), help="Output PDF path."
)
@click.option("--font", default="Helvetica", help="Font name.")
@click.option("--font-size", default=12.0, type=float, help="Font size in points.")
@click.option("--page-width", default=612.0, type=float, help="Page width in points.")
@click.option("--page-height", default=792.0, type=float, help="Page height in points.")
def text(input, output, font, font_size, page_width, page_height):
    """Convert a text file to PDF."""
    raise NotImplementedError("text command not yet implemented")


@main.command()
@click.argument("input", type=click.Path(exists=True))
@click.option(
    "-o", "--output", required=True, type=click.Path(), help="Output PDF path."
)
def render(input, output):
    """Render HTML/CSS to PDF."""
    raise NotImplementedError("render command not yet implemented")


@main.command()
@click.argument("input", type=click.Path(exists=True))
def info(input):
    """Inspect a PDF and print metadata."""
    raise NotImplementedError("info command not yet implemented")
