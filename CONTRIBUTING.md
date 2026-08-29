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
cargo run -p lands-cli -- play --seed 42 --stats        # one headless match
cargo run --release -p lands-cli -- fuzz --matches 2000 # invariant fuzzer
cargo run -p lands-cli -- golden verify                 # the regression net
PROPTEST_CASES=3000 cargo test -p lands-core --test properties
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

Nothing that records the authorship of a change may name an AI assistant. That
means all of:

- the commit message, body, trailers, co-authors and author field
- the **branch name** — `claude/fix-the-thing` is refused; branches are named
  `<scope>/<issue>-<slug>`, for the work rather than the tool
- the **pull request body**, including a "Generated with ..." footer

Write the change as its author.

Enforced in four places against one shared list of names
(`BANNED_ATTRIBUTION` in `xtask/src/text.rs`): the local `commit-msg` hook,
`cargo xtask check-commits` over the whole pull request range, `cargo xtask
check-pr` over the body and branch, and branch protection requiring both jobs.
`--no-verify` will not get past it.

One consequence worth knowing: the ban is on the string, so a pull request
body cannot discuss the rule by naming a tool either. Say "an assistant".

## Pull requests

- One issue per pull request. The issue names the crate; stay in it.
- **The body must start with `Closes #<issue>`.** See below.
- Each branch in a stack must pass CI on its own.
- Fill in the template, especially the behaviour section.
- If a golden replay hash moved, say which and why. Re-record in a separate
  commit.

See `docs/agent-workflow.md` for worktrees, stacking and the merge queue.

### Closing the issue

GitHub closes an issue on merge only when the pull request **body** carries a
closing keyword:

```
Closes #42
```

The `(#42)` in the commit subject is not enough. It is a link, not an
instruction, and a pull request that only has that merges green and leaves its
issue open — which is how #2 and #34 sat closed-in-fact and open-on-GitHub until
someone noticed.

So **do not use `gh pr create --fill`**: it replaces the body with the commit
message and discards the template that carries the keyword. Write the body:

```bash
gh pr create --base master --title "<commit subject>" --body-file <body>
```

`cargo xtask check-pr` enforces this on every pull request. To try it:

```bash
PR_BODY='Closes #42' cargo xtask check-pr
```

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
cargo run --release -p lands-cli -- fuzz --matches 5000
```
