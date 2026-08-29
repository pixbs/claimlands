# Contributing

## Setup

```bash
git config core.hooksPath .githooks
cargo test --workspace
cargo xtask ci
```

The toolchain is pinned in `rust-toolchain.toml`; rustup will fetch it
automatically.

## The loop

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask ci
```

Useful extras:

```bash
cargo run -p civ-cli -- play --seed 42 --stats        # one headless match
cargo run --release -p civ-cli -- fuzz --matches 2000 # invariant fuzzer
cargo run -p civ-cli -- golden verify                 # the regression net
PROPTEST_CASES=3000 cargo test -p civ-core --test properties
```

## Commit messages

Conventional Commits, scope = crate:

```
feat(core): territory split conserves the treasury (#42)
fix(render): stop cloud banding at the poles (#71)
docs(spec): pin the tie-break for equidistant capitals (#88)
```

Types: `feat` `fix` `docs` `style` `refactor` `perf` `test` `build` `ci`
`chore` `revert`.

### No AI attribution

Commit messages, bodies, trailers, co-authors and the author field must **not**
name an AI assistant — not Claude, not Codex, not Kiwi, not any other. Write
the commit as the author of the change.

This is enforced in three places: the local `commit-msg` hook, a CI job that
scans the whole pull request range, and branch protection requiring that job.
`--no-verify` will not get past it.

## Pull requests

- One issue per pull request. The issue names the crate; stay in it.
- Each branch in a stack must pass CI on its own.
- Fill in the template, especially the behaviour section.
- If a golden replay hash moved, say which and why. Re-record in a separate
  commit.

See `docs/agent-workflow.md` for worktrees, stacking and the merge queue.

## Adding a game rule

1. Write it in `spec/rules/` with a new id.
2. Put its numbers in `assets/rules/default.ron`.
3. Implement it — as a new turn phase if it needs one, never by editing an
   existing phase.
4. Give it a new `SeedDomain` if it needs randomness.
5. Write a test declaring `covers!("YOUR-ID")`.
6. Add a golden replay if it is a scenario rather than a unit of arithmetic.

`cargo xtask spec-coverage` fails if a documented rule has no test, or if a test
cites an id that does not exist.

## Reporting a bug

The simulation is deterministic, so almost every bug is exactly reproducible.
Find the seed — it turns a report into a test. The fuzzer prints one:

```bash
cargo run --release -p civ-cli -- fuzz --matches 5000
```
