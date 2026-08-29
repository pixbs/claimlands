//! The level format: a seed plus the handful of tiles an author changed.
//!
//! Terrain is procedural, so almost nothing about a level needs storing. What
//! does is a planet frequency, the seed it grew from, who is playing, and the
//! sparse set of tiles the author placed by hand:
//!
//! ```ron
//! Level(
//!     id: "campaign/03-the-narrows",
//!     freq: 8,        // 10n²+2 = 642 tiles
//!     seed: 194837,   // 0 means an empty ocean world, for authoring
//!     players: [
//!         Player(faction: Red,  kind: Human),
//!         Player(faction: Blue, kind: Ai(profile: "aggressive-2")),
//!     ],
//!     overrides: [
//!         Tile(id: 214, terrain: Land, kind: Capital, owner: Some(Red)),
//!         Tile(id: 297, terrain: Land, kind: Forest,  owner: None),
//!     ],
//! )
//! ```
//!
//! # Why RON and not a compact string
//!
//! A one-line encoding is unreviewable in a pull request, and with many agents
//! editing levels an invisible one-character diff is a live regression risk.
//! The compact form still exists for player sharing — base64url of the binary
//! encoding — but it is an export, never the source of truth.
//!
//! # What this crate does not do
//!
//! It does not generate planets. `lands-worldgen` does, and it sits on the same
//! layer, so the planet arrives through [`PlanetSource`]. See [`build`] for why
//! that separation is the point rather than a workaround.
//!
//! # Loading
//!
//! ```
//! # use lands_levels::{Level, PlanetSource};
//! # use lands_core::prelude::{Ruleset, Terrain, TileId, Topology};
//! # struct Ring;
//! # impl PlanetSource for Ring {
//! #     fn topology(&self, freq: u32) -> Result<Topology, String> {
//! #         let n = 10 * freq * freq + 2;
//! #         let neighbors = (0..n)
//! #             .map(|i| vec![TileId((i + n - 1) % n), TileId((i + 1) % n)])
//! #             .collect();
//! #         Topology::new(neighbors).map_err(|e| e.to_string())
//! #     }
//! #     fn terrain(&self, freq: u32, _seed: u64) -> Result<Vec<Terrain>, String> {
//! #         Ok(vec![Terrain::Land; (10 * freq * freq + 2) as usize])
//! #     }
//! # }
//! let level = Level::from_ron(
//!     r#"Level(
//!         id: "campaign/01-first-light",
//!         freq: 1,
//!         seed: 7,
//!         players: [
//!             Player(faction: Red,  kind: Human),
//!             Player(faction: Blue, kind: Ai(profile: "aggressive-2")),
//!         ],
//!         overrides: [
//!             Tile(id: 0, terrain: Land, kind: Capital, owner: Some(Red)),
//!             Tile(id: 6, terrain: Land, kind: Capital, owner: Some(Blue)),
//!         ],
//!     )"#,
//! )?;
//!
//! let world = level.build(&Ring, &Ruleset::bundled())?;
//! assert_eq!(world.players.len(), 2);
//! assert_eq!(world.territories.len(), 2);
//! # Ok::<(), lands_levels::LevelError>(())
//! ```

#![forbid(unsafe_code)]
// A level builds `lands-core` state directly, so it inherits the simulation's
// determinism rules. See docs/determinism.md.
#![deny(clippy::float_arithmetic)]
#![warn(missing_debug_implementations)]

pub mod build;
pub mod format;
pub mod validate;

pub use build::PlanetSource;
pub use format::{
    Level, LevelPlayer, MAX_FREQUENCY, MAX_PLAYERS, MIN_FREQUENCY, MIN_PLAYERS, OCEAN_SEED,
    PlayerKind, TileOverride, tile_count,
};
pub use validate::LevelError;

use ron::ser::PrettyConfig;

impl Level {
    /// Parse a level from RON text, then validate it.
    ///
    /// Validation is not optional here for the same reason it is not optional
    /// in `Ruleset::from_ron`: a level that parses but does not hold together
    /// would otherwise reach the simulation and fail somewhere far from the
    /// typo that caused it.
    pub fn from_ron(text: &str) -> Result<Self, LevelError> {
        let level: Level = ron::from_str(text).map_err(|e| LevelError::Parse(e.to_string()))?;
        level.validate()?;
        Ok(level)
    }

    /// Render this level back to RON, in the reviewable form authors write.
    ///
    /// Struct names are emitted (`Level(...)`, `Player(...)`, `Tile(...)`)
    /// because they are what makes a level diff readable — the whole reason
    /// this format is not one compact line.
    pub fn to_ron(&self) -> Result<String, LevelError> {
        let config = PrettyConfig::new().struct_names(true);
        ron::ser::to_string_pretty(self, config).map_err(|e| LevelError::Serialize(e.to_string()))
    }
}
