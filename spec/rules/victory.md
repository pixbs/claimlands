# Victory

## VICT-001 — Elimination
A faction that holds no tiles at all is eliminated and skipped in the turn
order. When only one faction remains, it wins.

If every faction is eliminated in the same resolution step, the match is a draw.

## VICT-002 — Dominance
A faction holding at least `victory.dominance_threshold_percent` (default 70%)
of the planet's **land** tiles has an unbeatable advantage and wins.

This exists so that a decided match ends rather than dragging on while the
winner mops up single tiles. 70% is past the point where the remaining factions
could realistically combine to reverse it; a literal reading of "majority" at
50% would end matches that are still genuinely contested.

The brief asked only for "occupation of the majority of the territory", so the
number is a judgement call. It is the rule most likely to move after
playtesting, and the cheapest to change — two numbers in
`assets/rules/default.ron`, no code.

The comparison is integer: `held × 100 >= land × threshold`. No division, no
floating point.

## VICT-003 — Dominance must be held
The threshold must be met for `victory.dominance_turns` consecutive **rounds**
(default 1).

Measured once per round, not once per turn, so a four-player match cannot
accumulate a streak four times as fast as a two-player one.

## VICT-010 — The record
The victory screen reports, per faction:

- rounds elapsed ("how many steps it took")
- units recruited, upgraded, killed, lost, starved
- all-time gold and wheat earned, and gold looted
- tiles captured and lost
- towns and fields built, capitals razed
- peak tiles and peak territory count held at once
