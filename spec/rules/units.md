# Units

Three kinds, in a strict upgrade chain: **Pawn → Warrior → Knight**. Each unit
gets one action per turn — moving and upgrading both spend it.

Costs and upkeep are in `spec/rules/economy.md` (ECON-002, ECON-010..012).

---

## Capture

### UNIT-010 — What a unit may take
A unit ending its move on a tile another faction owns, or on neutral ground,
captures it. Which tiles it may take depends on its kind.

| Sub-rule | Unit | May capture |
|---|---|---|
| **UNIT-010a** | Pawn | Empty, Forest, Field |
| **UNIT-010b** | Warrior | Empty, Forest, Field, **Town, Capital** |
| **UNIT-010c** | Knight | everything a warrior can |

A pawn is therefore an expansion tool, not a weapon: it can take open ground
but cannot break a defended settlement.

### UNIT-011 — What a unit may defeat
Moving onto an enemy unit destroys it, if permitted.

| Sub-rule | Unit | May defeat |
|---|---|---|
| **UNIT-011a** | Pawn | nothing |
| **UNIT-011b** | Warrior | Pawn |
| **UNIT-011c** | Knight | Pawn, Warrior, Knight |

A warrior cannot dislodge another warrior, so an equal front line is a
stalemate until someone fields a knight.

### UNIT-012 — Friendly units block
A unit may never move onto a tile held by one of its own faction's units,
whatever its kind.

### UNIT-013 — Capture razes
Whatever stood on a captured tile is destroyed and the tile becomes Empty. This
applies to Towns, Fields, Forests and Capitals alike. Sacking a capital
additionally transfers gold (TERR-020).

---

## Upgrades

### UNIT-020 — The chain
| Sub-rule | Unit | Upgrades to |
|---|---|---|
| **UNIT-020a** | Pawn | Warrior |
| **UNIT-020b** | Warrior | Knight |
| **UNIT-020c** | Knight | *nothing — top of the chain* |

### UNIT-021 — An upgrade costs the turn
Upgrading requires the unit to have its action available, and consumes it. A
unit therefore cannot both promote and move in the same turn.

---

## Movement

### UNIT-030 — The move budget
One action per unit per turn. A unit may travel up to
`units.own_territory_steps` (default 4) hops through tiles **its own faction
owns**.

### UNIT-031 — The capturing step
After its interior movement, a unit may take up to `units.foreign_steps`
(default 1) further steps onto ground its faction does not own, which captures
it (UNIT-010).

The two parts **compose into one action**: a unit may cross four of its own
tiles *and then* capture. This is what makes a developed interior useful for
redeploying to a threatened front. Were they exclusive, every attack would take
two turns — one to walk to the border, one to cross it — which telegraphs an
offensive the defender can always answer.

### UNIT-032 — Units block paths
A path may not pass **through** any occupied tile, friendly or hostile. Only the
final tile of a move is contested. A unit standing in a corridor therefore seals
it.
