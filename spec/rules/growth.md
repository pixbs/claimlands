# Growth

## GROW-001 — Forest spread
Once per round, each Forest tile has a `growth.forest_spread_percent` chance
(default 10%) to seed one neighbouring tile.

The candidate list is snapshotted at the start of the round, so a tile that
becomes a forest cannot spread again in the same round.

Each source tile draws from its own random stream keyed on
`(seed, ForestSpread, round, tile)`, so the outcome does not depend on
iteration order.

## GROW-002 — Where forests may spread
Only onto tiles in `growth.forest_spread_targets`, default `[Empty]`. Forests
never appear on Fields, Towns or Capitals.

Forests **will** take an *owned* empty tile. This is deliberate: empty tiles
yield wheat and forests yield nothing, so neglected land quietly degrades while
developed land is immune. It is the main pressure pushing players to build
rather than merely hold, and it is what stops forests from being scenery.
