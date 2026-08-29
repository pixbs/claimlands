# lands-core — local rules

This crate is the whole game. It is also the one place where a careless change
breaks every saved game at once, so it has the strictest rules in the
repository.

## Hard constraints (all enforced by CI)

- **No floating point.** `#![deny(clippy::float_arithmetic)]`. Resources are
  integers; distances are graph hops. See ADR 0005.
- **No `HashMap` / `HashSet`.** `BTreeMap`, `BTreeSet`, `Vec` only. Hash order
  varies per process and silently breaks replays.
- **No I/O, no clock, no threads, no platform APIs.**
- **No dependency on any crate downstream of this one.** `cargo xtask
  check-deps` fails the build if you add one.
- **Randomness only through `rng::stream`,** with a new `SeedDomain` for a new
  consumer. Never renumber an existing variant. Read `docs/determinism.md`
  first — this is the rule that keeps old replays valid.

## Where to make a change

| You want to | Change |
|---|---|
| Tune a number | `assets/rules/default.ron` — not this crate |
| Add a turn phase | `turn.rs`, as a new call. Do not edit an existing phase. |
| Add a player action | `command.rs` (variant) + `apply.rs` (validate and apply) |
| Change what a unit may do | `assets/rules/default.ron`; `movement.rs` reads it |
| Touch borders | `territory.rs` — read the module docs first, all of them |

## `territory.rs` deserves a warning

Splitting, merging, relocation and disbanding interact, and a capture can
trigger several at once. `retopologize` recomputes a faction's territories from
scratch rather than patching them, which is what makes compound cases correct
by construction. Resist the urge to "optimise" it into an incremental update.

## Before you push

```bash
cargo test -p lands-core
PROPTEST_CASES=2000 cargo test -p lands-core --test properties
cargo run --release -p lands-cli -- fuzz --matches 1000
```

The fuzzer is the fastest way to find out whether a territory change is sound.
