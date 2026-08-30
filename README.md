# Claimlands

A turn-based strategy game on a procedurally generated hexagonal planet. Up to
four factions expand across it, spending wheat and gold, splitting and merging
territories as borders shift.

Everything you see is generated at runtime — terrain, textures, trees, houses,
clouds. Nothing is loaded from an asset file.

Targets iOS and Android from one Rust codebase.

---

## Status

| Milestone | State |
|---|---|
| M0 Foundation — workspace, gates, docs | **done** |
| M1 Spec — rules with stable ids | **done** |
| M2 Simulation core | **done** |
| M3 Worldgen — the hex sphere in Rust | in progress |
| M3.5 Preview — WebGPU planet viewer, deployed per PR | **next** |
| M4 Levels and CLI | in progress |
| M5 AI | |
| M6 Renderer — `lands-procgen` + `lands-render` | |
| M7 Platform shells | |
| M8 Game feel — HUD, camera, victory screen | |

**M3.5 is the one that matters most**, and it is not about graphics. Until it
lands, a pull request can only be checked by reading its tests — and tests prove
the numbers are right, not that the planet is right. Cover scattered evenly and
cover properly clumped produce identical share statistics. See
[ADR 0007](docs/adr/0007-preview-as-verification-surface.md).

The simulation is complete and tested: territories split and merge, capitals
relocate and fall, units starve, forests spread, matches are won. It runs
headless with no renderer.

## Quick start

```bash
git config core.hooksPath .githooks
cargo test --workspace
```

Play a full four-faction match with random agents:

```bash
cargo run -p lands-cli -- play --seed 42 --stats
```

Fuzz the invariants:

```bash
cargo run --release -p lands-cli -- fuzz --matches 2000
```

## Where to look

| Path | What |
|---|---|
| [`AGENTS.md`](AGENTS.md) | **Read this first** if you are going to change anything. |
| [`spec/rules/`](spec/rules/) | Source of truth for game rules, with stable ids. |
| [`docs/architecture.md`](docs/architecture.md) | The crate graph and why it is shaped this way. |
| [`docs/determinism.md`](docs/determinism.md) | The rules that keep replays reproducible. |
| [`docs/adr/`](docs/adr/) | Decisions already made, with their reasoning. |
| [`assets/rules/default.ron`](assets/rules/default.ron) | Every balance number. |
| [`reference/prototype/`](reference/prototype/) | The original Three.js prototype — the visual oracle. |

## Design in one paragraph

`lands-core` holds the entire game and knows nothing about rendering, files, time
or platforms. Every change to the world goes through a `Command`, which makes
undo, replay, AI and future multiplayer the same mechanism. Every random draw
comes from its own stream, so adding a feature cannot invalidate a saved game.
Every balance number is data, and every rule has an id that a test must declare
it covers. Those four choices are what let many people work on this at once
without breaking each other's work.

## Stack

Rust · [wgpu](https://wgpu.rs) · thin Swift and Kotlin shells. No engine
framework — see [ADR 0001](docs/adr/0001-rust-wgpu-thin-engine.md).
