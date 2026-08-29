# ADR 0003 — One random stream per purpose

**Status:** accepted · 2026-08-29

## Context

The simulation needs randomness: terrain, cover seeding, forest spread, capital
relocation tie-breaks, AI jitter. The obvious design is one seeded RNG threaded
through the world.

That design has a failure mode that would undermine the entire repository.
**Adding any new random draw shifts every subsequent draw.** A pull request
adding weather would invalidate every stored replay and every golden hash —
because of a feature that has nothing to do with them. Agents would learn to
re-record hashes reflexively, and the regression net would stop meaning
anything.

## Decision

Every consumer derives its own stream:

```rust
fn stream(world_seed: u64, domain: SeedDomain, turn: u32, entity: u32) -> Rng
```

`SeedDomain` is an explicitly-numbered enum. New variants are appended; existing
ones are never renumbered or removed.

The algorithms — splitmix64 for seeding, xoshiro256\*\* for the stream — are
written by hand and pinned by published test vectors.

## Why

- Adding a `SeedDomain` **cannot** perturb an existing one, so a new feature is
  genuinely additive and old replays keep passing.
- Keying on `(turn, entity)` as well means the result does not depend on
  iteration order, so reordering a loop is safe.
- Hand-writing the PRNG removes the risk that a semver bump in a dependency
  silently changes every saved game. Both algorithms are short, public domain
  and exactly specified.

## Consequences

- Slightly more ceremony at each call site: you must name a domain.
- `SeedDomain`'s numbering is a permanent compatibility surface, comparable to a
  wire format.
- A test asserts the streams are independent, so the property cannot quietly
  regress.
