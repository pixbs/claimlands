# Invariants

Statements that must hold after **every** command. Checked by
`civ_core::invariants::check`, asserted by the property tests (gate 4) and by
`civ-cli fuzz`.

If you are adding a rule, ask which of these it could break. If it could break
one, the property test that covers it is not optional.

## INV-001 — The world is always sound
After any sequence of legal commands:

- Every territory is connected and holds exactly one capital (TERR-010).
- Every owned tile belongs to a territory whose faction matches the tile's.
- No Capital tile exists on unowned ground.
- No two territories claim the same tile.
- No treasury is negative.
- Every unit stands on land, on a tile that points back at it.
- No two units share a tile.
- Water is never owned, built on, or occupied.

## INV-002 — Play is reproducible
The same starting world and the same command log always produce a
bit-identical result — on the same machine, on a different run, and on a
different architecture.

This is what future multiplayer rests on, and it is why `civ-core` denies
floating-point arithmetic and uses no hash-map iteration.

## INV-003 — Territories partition ownership
For each faction, the union of its territories' tile sets equals exactly the set
of tiles it owns — no gaps, no overlaps, nothing claimed twice.
