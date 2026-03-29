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
        expect(result.output).to_contain("pdfun")

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
    def text_stub_raises():
        """pdfun text raises NotImplementedError."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            with open("input.txt", "w") as f:
                f.write("hello")
            result = runner.invoke(main, ["text", "input.txt", "-o", "out.pdf"])
            expect(result.exit_code).not_.to_equal(0)
            expect(type(result.exception)).to_equal(NotImplementedError)


with describe("CLI - render command"):

    @test
    def render_help():
        """pdfun render --help shows options."""
        runner = CliRunner()
        result = runner.invoke(main, ["render", "--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("--output")

    @test
    def render_stub_raises():
        """pdfun render raises NotImplementedError."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            with open("page.html", "w") as f:
                f.write("<h1>hi</h1>")
            result = runner.invoke(main, ["render", "page.html", "-o", "out.pdf"])
            expect(result.exit_code).not_.to_equal(0)
            expect(type(result.exception)).to_equal(NotImplementedError)


with describe("CLI - info command"):

    @test
    def info_help():
        """pdfun info --help shows usage."""
        runner = CliRunner()
        result = runner.invoke(main, ["info", "--help"])
        expect(result.exit_code).to_equal(0)
        expect(result.output).to_contain("INPUT")

    @test
    def info_stub_raises():
        """pdfun info raises NotImplementedError."""
        runner = CliRunner()
        with runner.isolated_filesystem():
            with open("test.pdf", "w") as f:
                f.write("fake")
            result = runner.invoke(main, ["info", "test.pdf"])
            expect(result.exit_code).not_.to_equal(0)
            expect(type(result.exception)).to_equal(NotImplementedError)
