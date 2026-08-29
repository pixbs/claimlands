# ADR 0002 — Commands are the only way the world changes

**Status:** accepted · 2026-08-29

## Context

The design brief asks for four things that are usually built separately: undo
within a turn, session replay, AI opponents, and (later) multiplayer. Built
separately they drift apart — the AI gets a shortcut the human does not have,
undo forgets to reverse one field, replay diverges.

## Decision

Every mutation of `World` goes through `Command`. `validate` never mutates;
`apply` assumes legality and emits `Event`. The renderer reacts only to events
and never reads simulation state to decide what to animate.

## Why

One pattern satisfies all four requirements, plus testing:

| Requirement | How it falls out |
|---|---|
| Undo | Restore the turn-start snapshot, replay all but the last command |
| Replay | A save file *is* `(ruleset_hash, level, Vec<Command>)` |
| AI | A brain emits the same commands a human does, so it cannot cheat |
| Multiplayer | Ship `Command` over a transport; `lands-core` does not change |
| Regression tests | A command log plus an expected state hash |

Undo deserves a note: implementing it as "snapshot and replay" rather than as
an inverse operation per command means there is no second implementation of the
rules to keep in sync. It is affordable because turn-based state is small.

## Consequences

- Nothing may write to `World` outside `apply.rs`. This is a real constraint
  and occasionally an inconvenient one.
- Commands are part of the save format, so a variant's shape is a compatibility
  surface. Adding variants is safe; changing existing ones is not.
- `legal_commands` must stay exhaustive, because the AI and the fuzzer both
  choose from it.
