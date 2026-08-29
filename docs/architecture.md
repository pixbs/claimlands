# Architecture

## The problem this shape solves

Many agents will work on this game in parallel, over many iterations, on
stacked pull requests. The failure mode to avoid is the one where adding the
science tree breaks combat, and fixing combat breaks the AI.

Everything below optimises for one property: **the blast radius of any change
is knowable and small.**

---

## 1. Stack

| Layer | Choice | Why |
|---|---|---|
| Language | Rust | Deterministic by construction, no GC pauses, headless-testable at millions of turns per second, one codebase for both platforms. |
| Graphics | `wgpu` | Metal on iOS, Vulkan on Android, **WebGPU in a browser** — which is what makes automated visual regression testing possible without a device farm. |
| Engine | none — a thin custom one | Bevy's own documentation still describes mobile as "possible, not easy", and it ships breaking releases roughly quarterly. That churn is exactly what causes cross-agent regressions. The prototype hand-rolls its renderer anyway, and procedural generation needs no asset pipeline. |
| Shells | Swift (iOS) + Kotlin (Android) over one FFI surface | Owns lifecycle, surface, safe-area insets, haptics — and later IAP, Game Center / Play Games, push. Retrofitting this is painful; doing it now is cheap. |

---

## 2. The three ideas everything rests on

### A pure simulation core

`civ-core` contains the entire game and knows nothing about rendering, files,
time, threads or platforms. It cannot import anything downstream, and that is
enforced mechanically rather than by convention (`cargo xtask check-deps`).

The payoff is testability: a whole match resolves headlessly in microseconds,
so the regression corpus can be enormous and still run in seconds. The
invariant fuzzer plays over a million commands in about half a minute.

### Command → Event sourcing

One pattern satisfies five separate requirements:

| Requirement | How |
|---|---|
| Undo | Restore the turn-start snapshot, replay all but the last command |
| Replay | A save file *is* `(ruleset_hash, level, Vec<Command>)` |
| AI | A brain emits the same `Command` a human does, so it cannot cheat |
| Multiplayer (later) | Ship `Command` over a transport; `civ-core` does not change |
| Regression testing | A command log plus an expected state hash is the golden test format |

Validation is separate from application: `validate` never mutates, so the HUD
can grey out an illegal action, and `apply` can assume legality.

**The renderer reacts only to `Event`.** It never reads simulation state to
decide what to animate. That wall is why a rendering agent and a rules agent
can work on the same feature without coordinating.

### Split random streams

Every consumer draws from its own stream keyed on
`(seed, domain, turn, entity)`. Adding a new `SeedDomain` cannot perturb an
existing one, so a feature that needs randomness cannot invalidate a stored
replay. See `docs/determinism.md` — this is the detail that makes "additive"
literally true rather than aspirational.

---

## 3. The crate graph

**The dependency direction is the isolation guarantee.**

```
civ-rules ──┐
            ├─→ civ-core ──┬─→ civ-worldgen ──→ civ-procgen ──→ civ-render ──┐
            │              ├─→ civ-ai                                        ├─→ civ-app ──→ civ-ffi ──→ platforms/
            │              └─→ civ-levels ────────────────────────────────────┘
civ-testkit ──→ (dev-dependency of everything)
```

| Crate | Layer | Owns | Must not contain |
|---|---|---|---|
| `civ-rules` | 0 | `Ruleset` types, RON loading, validation, `ruleset_hash()` | Any game logic |
| `civ-core` | 1 | World, Territory, Unit, turn pipeline, commands, events, invariants | Floats, I/O, rendering, time, hash iteration |
| `civ-worldgen` | 2 | Hex sphere topology, terrain, cover seeding | Rendering types |
| `civ-ai` | 2 | `Brain` trait, difficulty profiles | Mutating `World` directly |
| `civ-levels` | 2 | Level format, share codes, campaign | Rendering |
| `civ-procgen` | 3 | CPU mesh and texture builders → plain buffers | Any `wgpu` type |
| `civ-render` | 4 | wgpu device, pipelines, the low-res pixel-art pass | Game rules |
| `civ-app` | 5 | State machine, input, camera, HUD, `Event` → visual | Game rules |
| `civ-ffi` | 6 | `extern "C"` surface for the shells | Logic of any kind |
| `civ-testkit` | — | Fixtures, golden harness, topologies | Production code |

A crate may depend only on **strictly lower** layers. `cargo xtask check-deps`
enforces it; the table lives in `xtask/src/deps.rs`.

Two constraints are worth their own sentence:

- **`civ-procgen` may not touch `wgpu`.** That makes every mesh builder a pure
  function testable by vertex count and bounding box, with no GPU in CI.
- **`civ-testkit` may not be a normal dependency of shipped code.** Dev tools
  (`civ-cli`, `level-editor`) are exempt because they never reach a device.

---

## 4. Repository layout

```
spec/           SOURCE OF TRUTH FOR GAME RULES. Prose with stable ids.
docs/           Architecture, determinism, agent workflow, ADRs.
crates/         The crate graph above.
platforms/      iOS shell, Android shell, desktop dev harness, wasm for CI.
tools/          civ-cli (headless), level-editor.
xtask/          The quality gates cargo cannot express.
assets/rules/   Every balance number. Data, not code.
assets/visual/  Every palette and tunable from the prototype.
tests/replays/  The golden regression corpus.
reference/      The original Three.js prototype — the visual oracle.
```

Two directories deserve emphasis.

**`assets/rules/default.ron`** holds every number the game balances on. A
balance change is then a data diff any reviewer can read at a glance, and
alternate rulesets (tutorial, hardcore, a future science-tree variant) are free.

**`spec/` is the source of truth for rules; GitHub Issues are the source of
truth for work.** Keeping them separate is what stops rule discussions from
being buried in closed issues.

---

## 5. Level format

Because terrain is procedural, a level is mostly a seed plus the handful of
tiles an author changed:

```ron
Level(
    id: "campaign/03-the-narrows",
    freq: 8,        // 10n²+2 = 642 tiles
    seed: 194837,   // 0 means an empty ocean world, for authoring
    players: [
        Player(faction: Red,   kind: Human),
        Player(faction: Blue,  kind: Ai(profile: "aggressive-2")),
    ],
    overrides: [
        Tile(id: 214, terrain: Land, kind: Capital, owner: Some(Red)),
        Tile(id: 297, terrain: Land, kind: Forest,  owner: None),
    ],
)
```

RON rather than a single compact string because **a one-line encoding is
unreviewable in a pull request** — with many agents editing levels, an
invisible one-character diff is a live regression risk. The compact form still
exists: `civ-cli level export --share` emits base64url of the binary encoding,
which is what the future player-sharing feature will use.

---

## 6. Adding a feature without breaking one

The pattern is the same every time:

1. **Write the rule in `spec/rules/`** with a new id.
2. **Add its numbers to `assets/rules/default.ron`**, not to code.
3. **Add a turn phase** in `turn.rs` if it needs one — do not edit an existing
   phase.
4. **Give it a new `SeedDomain`** if it needs randomness.
5. **Write a test declaring `covers!("YOUR-ID")`**, and a golden replay if it
   is a scenario rather than a unit of arithmetic.
6. Run `cargo xtask ci`.

Worked examples of what this means for the roadmap:

- **Science tree** — a new turn phase, new `Ruleset` fields, new commands. No
  existing phase changes.
- **Multiplayer** — a transport that ships `Command`. `civ-core` does not
  change at all.
- **Player-facing level editor** — a UI over the format `civ-levels` already
  defines.
