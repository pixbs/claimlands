# Agent contract

Read this before touching anything. It is short on purpose.

This repository is built to be worked on by many agents in parallel without
them breaking each other's work. Almost every rule below exists because
ignoring it would cause a regression somebody else has to find.

---

## 1. Stay inside your crate

Your issue names a crate. Change files in that crate. If the work seems to need
a change in another crate, say so on the issue rather than reaching across —
that is how two agents end up editing the same file from opposite directions.

The dependency graph is the isolation guarantee, and it is enforced:

```
civ-rules → civ-core → { civ-worldgen, civ-ai, civ-levels } → civ-procgen
          → civ-render → civ-app → civ-ffi → platforms/
```

**No dependency may ever point back toward `civ-core`.** `cargo xtask check-deps`
fails the build if one does. See `docs/architecture.md` §3.

## 2. `civ-core` is sacred

The simulation must produce identical results on every machine, forever.
Inside `civ-core`:

- **No floating point.** `#![deny(clippy::float_arithmetic)]` is on. Resources
  are integers; distances are graph hops.
- **No `HashMap` or `HashSet`.** Use `BTreeMap` / `BTreeSet` / `Vec`. Hash
  iteration order varies between runs and silently breaks replays.
- **No clock, no filesystem, no threads, no randomness except through
  `rng::stream`.**
- **New randomness gets a new `SeedDomain`.** Append to the enum, never
  renumber. This is what stops your feature from invalidating every stored
  replay. Read `docs/determinism.md` before you write a single `rng` call.

## 3. Change balance in data, not code

Every number the game balances on lives in `assets/rules/default.ron`. If you
find yourself writing a literal `2` or `0.25` in a rule, it belongs there
instead.

## 4. Every rule has an id, and every id has a test

Game rules are specified in `spec/rules/` with stable ids like `ECON-004`.
Tests declare what they cover:

```rust
#[test]
fn towns_are_fed_whole() {
    covers!("ECON-004");
    // ...
}
```

`cargo xtask spec-coverage` fails if a documented rule has no test, or if a
test cites an id that does not exist. Adding a rule means adding it to the spec
**and** covering it.

## 5. The golden replays are the regression net

`tests/replays/*.ron` pin the behaviour of whole scenarios by state hash. If
your change moves a hash, it changed behaviour.

- If that was **intended**, re-record with
  `cargo run -p civ-cli -- golden record <file>` **in a separate commit**, and
  say in the PR why each scenario moved.
- If it was **not** intended, you have found your regression. Do not re-record.

## 6. Before you open a PR

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask ci
```

All four must pass. CI runs the same commands plus the mobile builds.

## 7. Commits

Conventional Commits, scope = crate:

```
feat(core): territory split conserves the treasury (#42)
fix(render): stop the cloud shader banding at the poles (#71)
```

**Never mention an AI assistant** — not in the message, not in the body, not in
a trailer, not as a co-author, not in the author field. This is checked by a
local hook *and* by CI over the whole commit range, so `--no-verify` will not
get past it. Install the hook once with:

```bash
git config core.hooksPath .githooks
```

## 8. `TODO` must name an issue

`// TODO(#123): ...`, never a bare `TODO`. Enforced by `cargo xtask check-todos`.

---

## Where things are

| Path | What |
|---|---|
| `spec/rules/` | **Source of truth for game rules.** Prose, with stable ids. Every rule is settled and carries its own reasoning — where the design brief was vague, the spec says what was chosen and why. Do not treat a rule as provisional. |
| `docs/architecture.md` | The crate graph and why it is shaped this way. |
| `docs/determinism.md` | The rules that keep replays reproducible. Read before touching `civ-core`. |
| `docs/adr/` | Decisions already made. Do not re-litigate; write a new ADR instead. |
| `assets/rules/default.ron` | Every balance number. |
| `reference/prototype/` | The original Three.js prototype — the visual oracle. |

Each crate has its own `AGENTS.md` with local rules. Read that too.
