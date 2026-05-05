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

### what to do next (visual loop, WIP)

The diff metric was changed mid-iteration to a structural one (shrink +
inked-union; see `tolerant_pixel_diff` in `tests/_visual_diff.py`) and
**`tests/visual/tolerances.toml` is not yet recalibrated for it**. The
old tolerances assume `differing-pixels / total-pixels` (whitespace
dominates), the new metric is `differing-blocks / inked-blocks` after
8x downsample. Almost every fixture now reads in the 50–100% range
under the new metric — these aren't 41 new regressions, they're honest
parity gaps that the old metric was hiding.

To finish the migration:

1. **Pin tolerances to current state.** Run the suite, write each
   fixture's observed diff (+ ~2% headroom) into `tolerances.toml` as
   the new ceiling. Sample script:

   ```bash
   uv run python -c "import json, math; \
   rows=[json.loads(l) for l in open('target/visual-report/results.jsonl') if l.strip()]; \
   obs={}; \
   [obs.update({r['name']: max(obs.get(r['name'],0), r.get('diff_pct') or 0)}) for r in rows if r['status']!='MISSING-REF']; \
   [print(f'{n} = {min(1.0, math.ceil((d+0.02)*50)/50):.2f}') for n,d in sorted(obs.items())]"
   ```

2. **Ratchet down per fixture as parity improves.** Each rendering fix
   should be paired with a tolerance drop in the same commit so the
   ceiling doesn't silently stay loose.
3. **Flip `@test.todo` back to `@test` in `tests/test_visual.py`** once
   tolerances reflect a state we want CI to defend.
4. **Re-bless `rgba_overlap`.** Master added it as
   `tests/visual/ref/rgba_overlap.png`, but the multi-page convention
   we standardised on is `<name>.page<N>.png`. Run
   `uv run python tools/bless_visual_refs.py rgba_overlap` to refresh.

The "obvious wrong" fixtures the user flagged (`columns` rendering 1
column instead of 3, `wpt/cascade_specificity` rendering all-red text)
are real pdfun bugs that the new metric correctly catches; they're
worth tackling once tolerances are in a state where the metric guards
against future drift.
