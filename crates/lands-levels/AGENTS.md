# lands-levels — local rules

This crate owns the level format: a planet frequency, the seed its terrain grew
from, who is playing, and the sparse set of tiles an author placed by hand. It
turns that into a `lands_core::World` ready for `Session::start`. Share codes
and campaign bundles land here later.

## Hard constraints

- **The layering rule applies.** Layer 2, so only `lands-core` and
  `lands-rules` may be reached from here, and `cargo xtask check-deps` fails
  the build otherwise.
- **`lands-worldgen` is a sibling, not a dependency.** It is on the same layer,
  so the planet a level names cannot be generated here. It arrives through the
  `PlanetSource` trait, supplied by whoever is above both crates. Resist the
  urge to grow terrain here to avoid the seam — that would duplicate the one
  place tile numbering is defined.
- **A level is data, and the world it builds is `lands-core` state.** No
  floating point, no `HashMap`/`HashSet`, no clock. See `docs/determinism.md`.

## Tile ids are the format's contract

An override names a tile by its index into the planet `lands-worldgen` emits.
The count is `10·freq²+2` and that is checked here, but the *order* is
`lands-worldgen`'s and nothing in this crate can detect that it moved. If
`crates/lands-worldgen/tests/snapshot.rs` fails, every saved level describes a
different planet than the one it was authored on.

## Validation is not optional

`Level::from_ron` validates as well as parses, for the same reason
`Ruleset::from_ron` does: a level that parses but does not hold together fails
somewhere far away from the typo that caused it. New checks belong in
`validate.rs`, report the **first** problem in file order, and name the
offending field — `overrides[3].id`, not "a tile".

## Before you push

```bash
cargo test -p lands-levels
```
