# Territories

A **territory** is a connected component of tiles owned by one faction. It is
the central entity of the game: treasury, capital and every build price belong
to a territory, not to a player. One faction may hold several at once.

These are the subtlest rules in the game. Read `crates/lands-core/src/territory.rs`
alongside this file before changing anything here.

---

## The core invariant

### TERR-010 — One capital, always
Every territory contains exactly one Capital tile, and it is the tile the
territory names as its capital. No Capital tile exists outside a territory.

Every rule below exists to maintain TERR-010 through border changes.

---

## Losing a capital

### TERR-011 — Relocation
When a territory loses its capital, it rehouses it on a tile of the first kind
available from `territory.capital_relocation_preference`, default
`[Empty, Town, Field]`. Within that kind it picks the tile closest to the
territory's **graph centre** — the tile with the smallest total hop distance to
every other tile in the territory.

The brief asks for a capital placed "randomly … approximately the center",
which reads as a contradiction. It is resolved by being central first and random
only among exact ties, so the result is both centred and reproducible.

### TERR-013 — Disbanding
A territory holding nothing but Forest has nowhere to rehouse its capital. The
entire zone loses its owner and becomes no one's land. Its units are destroyed
and its treasury is lost. The trees remain.

---

## Sacking a capital

### TERR-020 — Loot
Capturing a Capital tile transfers `floor(victim_gold × capital_loot_percent /
100)` — default 25% — from the victim's territory to the capturing unit's
territory. The tile is then razed to Empty (UNIT-013), which triggers TERR-011
or TERR-013 for the victim.

The transfer happens **before** the victim's territory splits, so the loot is
taken from the whole treasury rather than from one fragment.

---

## Splitting

### TERR-030 — Proportional division
When a capture disconnects a territory, it becomes several. Each connected
component becomes its own territory, and the treasury divides **in proportion to
tile count**:

```
share_i = floor(treasury × size_i / total_size)
```

The remainder from floor division goes to the component that kept the original
capital (or, if the capital was lost, to the largest component). A split
therefore never creates or destroys wheat or gold.

The component holding the original capital keeps it. Every other component gets
a new one by TERR-011, or is disbanded by TERR-013.

**Worked example (from the brief).** A player holds 16 tiles with 15 gold and
15 wheat. One tile is captured, leaving 15 tiles split 10 / 5. The 10-tile piece
keeps the capital and 10 of each resource; the 5-tile piece gets a new capital
and 5 of each.

### TERR-031 — Identity is stable
An ordinary border nudge does not renumber a territory. The largest surviving
piece inherits the parent's id, so the HUD does not flicker and a queued command
referring to a territory stays valid.

---

## Merging

### TERR-040 — Joining two of your own
When a capture connects two territories of the same faction, they become one.
Treasuries **sum**. The capital closest in hops to the newly captured tile
survives; every other capital in the merged territory is razed to Empty.

### TERR-041 — Tie-break
Capitals equidistant from the captured tile are resolved by **lowest tile id**.

The brief's sentence *"If both capitals were created at the same time"* is
unfinished, so this rule was ours to choose. Keeping the older capital would
need a creation timestamp on territories, and two capitals founded by the same
split are genuinely the same age — so that tie-break would itself need a
tie-break. Tile id is arbitrary but total and reproducible everywhere, which
matters more here than the tie-break feeling principled.
