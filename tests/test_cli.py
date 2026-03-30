from pathlib import Path

from click.testing import CliRunner
from tryke import describe, expect, test

from pdfun.cli import main

with describe("CLI - group"):

    @test
    def cli_help():
        """pdfun --help shows usage."""
        runner = CliRunner()
        result = runner.invoke(main, ["--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("PDF generation")

    @test
    def cli_version():
        """pdfun --version prints version."""
        runner = CliRunner()
        result = runner.invoke(main, ["--version"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("0.1.0")


with describe("CLI - text command"):

    @test
    def text_help():
        """pdfun text --help shows options."""
        runner = CliRunner()
        result = runner.invoke(main, ["text", "--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("--font")
        expect(result.output).to_contain("--output")

    @test
    def text_converts_file():
        """pdfun text converts a text file to PDF."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            Path("input.txt").write_text("Hello world")
            result = runner.invoke(main, ["text", "input.txt", "-o", "out.pdf"])
            expect(result.exit_code).to_equal(0)
            data = Path("out.pdf").read_bytes()
            expect(data[:5]).to_equal(b"%PDF-")
            expect(data).to_contain(b"Hello world")

    @test
    def text_wraps_long_lines():
        """pdfun text wraps long lines across the page."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            Path("input.txt").write_text(" ".join(["word"] * 100))
            result = runner.invoke(main, ["text", "input.txt", "-o", "out.pdf"])
            expect(result.exit_code).to_equal(0)
            data = Path("out.pdf").read_bytes()
            expect(data[:5]).to_equal(b"%PDF-")


with describe("CLI - render command"):

    @test
    def render_help():
        """pdfun render --help shows options."""
        runner = CliRunner()
        result = runner.invoke(main, ["render", "--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("--output")

    @test
    def render_html_file():
        """pdfun render converts HTML file to PDF."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            Path("page.html").write_text("<h1>Test</h1><p>Body text</p>")
            result = runner.invoke(main, ["render", "page.html", "-o", "out.pdf"])
            expect(result.exit_code).to_equal(0)
            data = Path("out.pdf").read_bytes()
            expect(data[:5]).to_equal(b"%PDF-")
            expect(data).to_contain(b"Test")


with describe("CLI - info command"):

    @test
    def info_help():
        """pdfun info --help shows usage."""
        runner = CliRunner()
        result = runner.invoke(main, ["info", "--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("INPUT_PATH")

    @test
    def info_stub_raises():
        """pdfun info raises NotImplementedError."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            Path("test.pdf").write_text("fake")
            result = runner.invoke(main, ["info", "test.pdf"])
            expect(result.exit_code).not_.to_equal(0)
            expect(type(result.exception)).to_equal(NotImplementedError)
