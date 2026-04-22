# Contributing

## Local development

```bash
uv sync
uv run maturin develop --release
uv run tryke test
```

The full verification pass CI runs:

```bash
uv run ty check
uv run tryke test
uv run ruff check
uv run ruff format --check
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
uv run python tools/parity/generate.py --check
```

## Commit messages

Conventional commits (`feat:`, `fix:`, `perf:`, `ci:`, `chore:`, `docs:`). Tests carry `# spec:` / `behaviors:` markers; `tools/parity/catalog.toml` tracks coverage.

## Stacked PRs

Shipping a feature in several reviewable pieces? Stack the PRs.

```bash
git checkout -b feat/a master
# ...work, commit, push...
gh pr create --base master --title "feat: a"

git checkout -b feat/b feat/a
# ...work, commit, push...
gh pr create --base feat/a --title "feat: b (stacked on #<A>)"
```

When **PR A** merges:

1. GitHub deletes the `feat/a` branch (this repo has **Automatically delete head branches** on).
2. GitHub retargets any open PR whose base was `feat/a` — so **PR B**'s base flips to `master` automatically.

No manual `gh pr edit --base master` needed. This works for merge-commit, squash, and rebase merges — retargeting triggers on branch deletion, not on the merge strategy.

### When auto-retarget doesn't fire

- The auto-delete setting was turned off on the repo (check: `gh api repos/thejchap/pdfun --jq '.delete_branch_on_merge'`).
- A reviewer used the PR-level "Delete branch" toggle to opt the merge out of auto-delete.

Fallback: `gh pr edit <num> --base master`.
