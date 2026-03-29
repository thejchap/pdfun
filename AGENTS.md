# agents

## guidelines

- this is intended to be an alternative to weasyprint (<https://github.com/Kozea/WeasyPrint>). where relevant, reference the weasyprint test suite and functionality
- limit dependencies on external libraries where possible - specifically, no external system dependencies

## verification

run the following:

```bash
uv run ty check # type checker
uv run tryke test # tests
uv run ruff check # linter
uv run ruff format --check # formatter
```
