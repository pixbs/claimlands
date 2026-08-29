# Determinism

Read this before changing anything in `civ-core`.

## Why it matters

Three features depend on the simulation producing bit-identical results every
time, on every machine:

- **Replays.** A save file is a command log. If replaying it diverges, the save
  is worthless.
- **Golden tests.** The regression net is a set of state hashes. If the hash is
  not stable, the net catches nothing.
- **Multiplayer** (post-MVP). Lockstep means both devices run the same
  simulation and must agree. An iPhone and an Android phone that disagree by
  one gold desync the match.

Determinism is not something to add later. Every one of those features assumes
it from the first line of code.

---

## The four rules

### 1. No floating point in `civ-core`

`#![deny(clippy::float_arithmetic)]` is on and must stay on.

Floats are not the problem in themselves — IEEE-754 arithmetic is well
defined — but compilers are free to contract `a * b + c` into an FMA, vectorise
a sum in a different order, or evaluate at higher precision, and they make
different choices per target and per optimisation level. Two devices then
disagree in the last bit, and one comparison later they disagree about who owns
a tile.

Resources are integers. Percentages are integer comparisons:

```rust
// Not: held as f32 / land as f32 >= threshold as f32 / 100.0
let dominant = u64::from(held) * 100 >= u64::from(land) * u64::from(threshold);
```

Distances are **graph hops**, not Euclidean (see below).

### 2. No hash-based iteration

`HashMap` and `HashSet` iterate in an order that depends on the hasher's random
seed, which changes per process. Iterating one to pick "the first valid
candidate" gives a different answer on every run.

Use `BTreeMap`, `BTreeSet` or `Vec`. Every collection in `civ-core` already
does. `Topology::new` even sorts the neighbour lists it is given, because BFS
tie-breaks depend on that order.

### 3. Randomness comes only from `rng::stream`

```rust
pub fn stream(world_seed: u64, domain: SeedDomain, turn: u32, entity: u32) -> Rng
```

Every consumer draws from its own stream. This is the single most load-bearing
decision in the crate, and the reason is worth stating plainly:

> With one shared RNG, **adding any new random draw shifts every subsequent
> draw**. Every stored replay and every golden hash breaks at once — because
> someone added a feature that had nothing to do with them.

With split streams, adding a new `SeedDomain` cannot perturb an existing one.
A new feature is genuinely additive.

**When you add randomness:** append a variant to `SeedDomain`. Never renumber
or remove an existing one — the numeric value is baked into every stream that
was ever drawn.

The algorithms (splitmix64 for seeding, xoshiro256\*\* for the stream) are
hand-written rather than pulled from a crate, because a semver bump in an RNG
dependency would silently change every saved game. They are pinned by test
vectors in `rng.rs`.

### 4. No clock, no filesystem, no threads

`civ-core` has no notion of wall time and does no I/O. `chrono` and `instant`
are banned in `deny.toml`. Anything that varies between runs varies between
players.

---

## Why the simulation has no coordinates

The planet is a Goldberg polyhedron, so tiles have 3D positions — but
`civ-core` does not know them.

Every spatial question the rules ask is answered in **graph hops**:

| Rule | Question | Answered by |
|---|---|---|
| UNIT-030 | "within four tiles" | BFS depth |
| TERR-011 | "approximately the centre" | smallest total hop distance |
| TERR-040 | "closest to the captured tile" | BFS distance |

This buys three things at once: it is integer (rule 1), it is closer to what a
hex-game player actually perceives than Euclidean distance, and it means
`civ-worldgen` can change the planet's geometry without touching a line of game
logic.

---

## How it is checked

| Gate | What it proves |
|---|---|
| Golden replays (`tests/replays/`) | Known scenarios still land on the same state |
| `the_same_seed_always_plays_the_same_match` | Two runs in one process agree |
| `replaying_a_random_match_reproduces_it_exactly` | A command log reproduces its own match |
| CI `determinism` job | Linux, macOS and Windows all agree |
| `civ-cli fuzz` | Hundreds of thousands of commands hold the invariants |

The CI matrix is the one that matters for multiplayer: it is the only check
that would catch an architecture-dependent difference.

---

## If you need to break a rule

You do not. But if you are convinced otherwise, write an ADR in `docs/adr/`
explaining what breaks and how replays stay valid. Do not do it in a pull
request that is about something else.
