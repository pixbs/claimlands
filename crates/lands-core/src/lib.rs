//! The rules of Claimlands, and nothing else.
//!
//! # What this crate is
//!
//! The complete simulation: tiles, factions, territories, treasuries, units,
//! turn resolution, victory. It knows nothing about rendering, files, time,
//! threads or platforms, and it never will — the dependency graph is enforced
//! by `cargo xtask check-deps` in CI.
//!
//! That isolation is what makes the game testable. A whole match resolves
//! headlessly in microseconds, so the golden-replay corpus that guards every
//! rule can be enormous and still run in seconds.
//!
//! # The three properties everything rests on
//!
//! 1. **No floating point.** `#![deny(clippy::float_arithmetic)]` is enforced
//!    below. Gold and wheat are integers, and every spatial question is
//!    answered in graph hops (see [`topology`]), so results are bit-identical
//!    on x86-64 and aarch64. Future multiplayer depends on this.
//!
//! 2. **No hash iteration.** Every collection is a `Vec`, `BTreeMap` or
//!    `BTreeSet`. `HashMap` order varies between runs and would make replays
//!    irreproducible.
//!
//! 3. **Split random streams.** Each consumer draws from its own stream keyed
//!    on `(seed, domain, turn, entity)` (see [`rng`]), so adding a feature that
//!    needs randomness cannot disturb the randomness of anything that already
//!    exists — and therefore cannot invalidate a stored replay.
//!
//! # Where to start reading
//!
//! * [`command`] — the only way the world ever changes, and why.
//! * [`territory`] — splitting, merging and rehousing capitals. The subtlest
//!   module; read it before changing anything near borders.
//! * [`turn`] — the turn pipeline, and how to add a phase without touching an
//!   existing one.
//! * [`invariants`] — what is always true, and what to assert in a new test.
//!
//! # Example
//!
//! ```
//! use lands_core::prelude::*;
//! # use std::sync::Arc;
//! # let topology = Arc::new(Topology::new(vec![vec![TileId(1)], vec![TileId(0)]]).unwrap());
//! let rules = Ruleset::bundled();
//! let world = World::empty(1234, topology);
//! let mut session = Session::start(world, rules);
//! assert!(session.state_hash() != 0);
//! ```

#![forbid(unsafe_code)]
// Determinism: the simulation must produce identical results on every target,
// so it does no floating-point arithmetic at all. See docs/determinism.md.
#![deny(clippy::float_arithmetic)]
#![warn(missing_debug_implementations)]

pub mod apply;
pub mod command;
pub mod economy;
pub mod event;
pub mod growth;
pub mod hash;
pub mod ids;
pub mod invariants;
pub mod movement;
pub mod rng;
pub mod session;
pub mod state;
pub mod territory;
pub mod topology;
pub mod turn;
pub mod victory;

/// Everything a caller normally needs, in one import.
pub mod prelude {
    pub use crate::command::{Command, Rejection};
    pub use crate::event::Event;
    pub use crate::hash::world_hash;
    pub use crate::ids::{Faction, TerritoryId, TileId, UnitId};
    pub use crate::session::{ReplayError, Session};
    pub use crate::state::{
        Controller, Outcome, Player, Terrain, Territory, Tile, Unit, VictoryReason, World,
    };
    pub use crate::topology::{Topology, TopologyError};
    pub use lands_rules::{Ruleset, TileKind, UnitKind};
}

pub use lands_rules;
