# Economy

Every rule below is resolved **per territory**, in ascending territory id, at
the start of its owner's turn. Costs and counts are always measured *within one
territory*, never across a faction.

Numbers live in `assets/rules/default.ron`; this file says what they mean.

---

## Income

### ECON-001 — Tile yields
Each owned tile contributes an unconditional yield each turn.

| Sub-rule | Tile | Wheat | Gold |
|---|---|---|---|
| **ECON-001a** | Empty | +1 | 0 |
| **ECON-001b** | Capital | +1 | +1 |
| **ECON-001c** | Field | +2 | 0 |
| **ECON-001d** | Forest | 0 | 0 |

Towns are deliberately absent: their output is conditional (ECON-004). The
ruleset validator rejects a `Town` entry in `economy.tile_yields`.

### ECON-020 — Starting treasury
A capital founded at the start of a match begins with `starting_wheat` and
`starting_gold`. A capital created by a territory split instead inherits a
proportional share of its parent's treasury (TERR-030).

---

## Upkeep

### ECON-002 — Unit upkeep
Each unit consumes resources every turn from the territory **it is standing
in**, not the one that built it. Walking a unit across a border moves its cost
with it.

| Sub-rule | Unit | Wheat | Gold |
|---|---|---|---|
| **ECON-002a** | Pawn | 1 | 0 |
| **ECON-002b** | Warrior | 2 | 0 |
| **ECON-002c** | Knight | 2 | 1 |

Knights are the only unit with a gold upkeep. That makes a knight army a
standing drain competing with construction, rather than merely an expensive
purchase — which is what stops the endgame collapsing into "whoever banked the
most gold fields the most knights".

### ECON-003 — Order of operations
Within one territory, in this order:

1. Collect tile yields (ECON-001).
2. Feed units (ECON-002), starving them if necessary (ECON-005).
3. Feed towns from whatever wheat remains (ECON-004).

Wheat goes to units before towns. A territory that cannot feed its army will
therefore produce no gold from towns at all.

### ECON-004 — Town production
Towns convert wheat into gold, and only **whole** towns are fed:

```
towns_fed = min(town_count, floor(wheat_remaining / town.wheat_cost))
wheat_spent = towns_fed * town.wheat_cost
gold_made   = towns_fed * town.gold_yield
```

With the default numbers (`wheat_cost: 3`, `gold_yield: 2`), the brief's worked
example holds: three towns with seven wheat remaining feed
`floor(7/3) = 2` towns, spending 6 wheat and producing 4 gold. The third town
goes hungry and produces nothing — towns are never fed in fractions.

### ECON-005 — Famine
If a territory cannot pay its unit upkeep, units are destroyed one at a time
until it can. It sheds the **minimum** number that restores solvency.

Order of loss is `units.starvation_priority`, default `[Knight, Warrior, Pawn]`
— the most expensive units are shed first. Within one kind, the **oldest** unit
(lowest creation sequence) dies first.

Upgrading a unit does not reset its age, so a player cannot dodge famine by
promoting.

---

## Prices

Every price has the form `base + per_existing × count`, where `count` is
measured within the spending territory only.

### ECON-010 — Recruit a pawn
`1 + 1 × (units of ANY kind in the territory)`.

#### ECON-010b — Recruits cannot act immediately
A unit bought this turn has already used its action, so gold cannot be turned
straight into a capture. Without this, a stockpiled treasury converts into a
surprise attack the defender had no way to read; with it, an army massing is
visible for a turn before it can strike.

### ECON-011 — Upgrade pawn → warrior
`1 + 1 × (warriors + knights in the territory)`. Pawns do **not** count toward
this price.

### ECON-012 — Upgrade warrior → knight
`2 + 2 × (knights in the territory)`.

### ECON-013 — Build a town
`2 + 1 × (towns in the territory)`. Requires an owned, empty tile.

### ECON-014 — Build a field
`1 + 1 × (fields in the territory)`. Requires an owned, empty tile.
