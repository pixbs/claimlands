# Turns, undo and replay

## Turn order

### SESS-020 — Rounds and turns
A **turn** belongs to a player; a **round** is one turn for every surviving
player. A player with four territories plays all of them within their single
turn.

At the start of a player's turn their units refresh and their economy resolves
(ECON-003). At the end of a round, forests spread (GROW-001) and dominance is
checked (VICT-002).

### SESS-021 — Ownership of commands
A player may only command units and territories of their own faction. Anything
else is refused with `NotYourUnit` or `NotYourTerritory`.

### SESS-022 — A finished match accepts nothing
Once an outcome is recorded, every command is refused with `GameOver`.

---

## Undo

### SESS-001 — Taking a move back
Any command in the current turn may be undone, most recent first. Undo restores
the state exactly, including treasuries, unit ages and statistics.

It works by restoring a snapshot taken at the start of the turn and replaying
every command except the last, so there is no second implementation of the rules
that could drift out of sync with the first.

### SESS-002 — Undo stops at the turn boundary
Ending a turn is the commit point: it takes a fresh snapshot, so a played turn
can never be taken back. This is the brief's "undo any move within the turn
until the turn is played".

---

## Replay

### SESS-010 — A log reproduces the match
The starting world plus the command log reproduces the match exactly. A save
file is therefore `(ruleset_hash, level_id, Vec<Command>)` — a few hundred bytes
for a whole game — and the same bytes are what a future multiplayer transport
would carry.

### SESS-011 — Balance changes invalidate replays loudly
Every replay records the ruleset hash it was made against. Loading one recorded
against different balance fails with `RulesetMismatch` rather than silently
playing out differently.
