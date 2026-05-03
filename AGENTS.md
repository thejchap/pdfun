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

## visual regression loop

`tryke test` rasterizes every fixture under `tests/visual/` and pixel-diffs
the result against committed WeasyPrint reference PNGs. Failures and
per-fixture diff thumbnails land in `target/visual-report/index.html`
(auto-refreshing every 2 s) — open it once and keep the tab open while
you iterate:

```bash
uv run tryke test                           # pdfun vs committed refs
open target/visual-report/index.html        # browse diffs
```

When you add or change a fixture, re-bless the reference PNGs. WeasyPrint
runs inside a pinned Docker image so the host needs only Docker:

```bash
uv run python tools/bless_visual_refs.py                    # all fixtures
uv run python tools/bless_visual_refs.py progressive/02_block_model
```

Per-fixture tolerances live in `tests/visual/tolerances.toml`. Tighten
them as parity improves.
