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
— reload it after each run to see updated diffs:

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

### iterating on a fixture (agent workflow)

After each `tryke test` run, every FAIL row gets a side-by-side composite
PNG at `target/visual-report/composite/<name>.page<N>.png` showing
`pdfun | WeasyPrint reference | diff overlay`. Red in the overlay marks
pixels where pdfun diverges from the reference; faded grayscale is
matching content. **Read that single image** to see what's wrong — it
replaces having to read actual + ref + manually mentally-diff.

Suggested loop when fixing a regression:

1. `uv run tryke test` to refresh the report.
2. Pick a target. The JSONL is in execution order, so sort by diff first:

   ```bash
   uv run python -c "import json; \
   rows=[json.loads(l) for l in open('target/visual-report/results.jsonl') if l.strip()]; \
   rows=[r for r in rows if r.get('status')=='FAIL']; \
   rows.sort(key=lambda r: -(r.get('diff_pct') or 0)); \
   [print(f\"{r['diff_pct']:.4f}  {r['name']} page{r['page']}\") for r in rows[:10]]"
   ```

3. `Read target/visual-report/composite/<name>.page<N>.png` to see
   actual / reference / diff at a glance. The red regions tell you
   where to look in the rendering pipeline.
4. Edit the layout / rasterization code.
5. `uv run tryke test` again, then re-Read the same composite to confirm
   the red shrank rather than just moved. The `diff_pct` field is the
   numeric proxy — verify it dropped before moving on.

Composites are only generated for `FAIL` rows (skipped for PASS and
MISSING-REF). They're rebuilt every run, so don't edit them by hand.

### ratcheting tolerances down

`tests/visual/tolerances.toml` is pinned to each fixture's observed
diff plus ~1% headroom (rounded up to 0.01). Each rendering fix should
be paired with a tolerance drop in the same commit so the ceiling
doesn't silently stay loose. To recompute after a run:

```bash
uv run python -c "import json, math; \
rows=[json.loads(l) for l in open('target/visual-report/results.jsonl') if l.strip()]; \
obs={}; \
[obs.update({r['name']: max(obs.get(r['name'],0), r.get('diff_pct') or 0)}) for r in rows if r['status']!='MISSING-REF']; \
[print(f'{n} = {min(1.0, math.ceil((d+0.01)*100)/100):.2f}') for n,d in sorted(obs.items())]"
```

Known-bad fixtures worth tackling: `columns` renders 1 column instead
of 3; `wpt/cascade_specificity` renders all-red text; `rgba_overlap`
sits at the 1.00 ceiling because its two boxes are positioned
differently enough that the structural shrink can't align them.
