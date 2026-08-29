# ADR 0005 — No floating point in the simulation

**Status:** accepted · 2026-08-29

## Context

Replays, golden tests and future lockstep multiplayer all require the
simulation to produce bit-identical results on every target — at minimum on
aarch64 (both phones) and x86-64 (CI and desktop).

IEEE-754 arithmetic is well defined, but compilers are free to contract
`a * b + c` into a fused multiply-add, vectorise a reduction in a different
order, or keep intermediates at higher precision. Those choices vary by target
and optimisation level. One differing bit, one comparison later, and two devices
disagree about who owns a tile.

## Decision

`#![deny(clippy::float_arithmetic)]` in `civ-core` and `civ-rules`.

Resources are `i32`. Percentages are integer comparisons:

```rust
let dominant = u64::from(held) * 100 >= u64::from(land) * u64::from(threshold);
```

Proportional splits use integer division with the remainder explicitly assigned,
so nothing is created or destroyed (TERR-030).

## Why

The alternative — floats plus an epsilon discipline — requires every author
forever to get it right. A compiler lint requires it once.

Nothing in the game design needs fractions. Gold, wheat, tiles and hops are all
naturally whole.

## Consequences

- Rounding must be decided explicitly at each site rather than falling out of
  the arithmetic. That is a feature: the town-feeding rule (ECON-004) rounds
  *down* and it matters, so it is written down and tested.
- Rendering, which is downstream of `civ-core`, uses floats freely. The lint is
  scoped to the crates where determinism is a requirement.
