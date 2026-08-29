//! The shape of a level file.
//!
//! Terrain is procedural, so a level is a seed plus the handful of tiles an
//! author changed. Everything here is plain data: nothing in this module makes
//! a decision, exactly as `lands-rules` describes balance without applying it.
//!
//! # Why the Rust names and the RON names differ
//!
//! The format in `docs/architecture.md` §5 writes players as `Player(...)` and
//! overrides as `Tile(...)`, which are the words an author thinks in. Both
//! names are already taken in `lands_core::state` by different types, so the
//! Rust types are [`LevelPlayer`] and [`TileOverride`] and serde renames them
//! at the file boundary. The file format is the contract; the Rust name is not.

use lands_core::prelude::{Faction, Terrain, TileKind};
use serde::{Deserialize, Serialize};

/// Lowest planet frequency a level may name: `10·1²+2` = 12 tiles, the dual of
/// a bare icosahedron.
pub const MIN_FREQUENCY: u32 = 1;

/// Highest planet frequency a level may name: `10·12²+2` = 1442 tiles.
///
/// `lands-worldgen` will subdivide far beyond this; the cap is the game's, not
/// the mesh's. A larger planet is a design change, not a bigger number.
pub const MAX_FREQUENCY: u32 = 12;

/// Fewest players a match can be played with.
pub const MIN_PLAYERS: usize = 2;

/// Most players a match can be played with — one per [`Faction`].
pub const MAX_PLAYERS: usize = 4;

/// The seed that means "no terrain at all", for authoring.
pub const OCEAN_SEED: u64 = 0;

/// Tiles on a frequency-`freq` planet: `10n² + 2`.
///
/// The same formula as `lands_worldgen::vertex_count`, restated here because
/// that crate sits beside this one in the graph and cannot be called from it.
/// It is the definition of a Goldberg polyhedron rather than an implementation
/// detail, so the duplication cannot drift.
pub const fn tile_count(freq: u32) -> u32 {
    10 * freq * freq + 2
}

/// One authored level.
///
/// ```
/// use lands_levels::Level;
///
/// let level = Level::from_ron(
///     r#"Level(
///         id: "campaign/01-first-light",
///         freq: 2,
///         seed: 194837,
///         players: [
///             Player(faction: Red,  kind: Human),
///             Player(faction: Blue, kind: Ai(profile: "aggressive-2")),
///         ],
///         overrides: [
///             Tile(id: 3,  terrain: Land, kind: Capital, owner: Some(Red)),
///             Tile(id: 30, terrain: Land, kind: Capital, owner: Some(Blue)),
///         ],
///     )"#,
/// )
/// .unwrap();
///
/// assert_eq!(level.players.len(), 2);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Level {
    /// Stable name, and the key a save file records so a replay knows which
    /// map it was played on. By convention `campaign/03-the-narrows`.
    pub id: String,
    /// Planet frequency. The tile ids in `overrides` index a planet of exactly
    /// this size, so changing it renumbers every override.
    pub freq: u32,
    /// Root of the terrain and of every random stream drawn during play.
    /// [`OCEAN_SEED`] means an empty ocean world, for authoring.
    pub seed: u64,
    /// Turn order, first to last.
    pub players: Vec<LevelPlayer>,
    /// The tiles the author changed, in no required order.
    pub overrides: Vec<TileOverride>,
}

/// A seat at the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "Player")]
pub struct LevelPlayer {
    pub faction: Faction,
    pub kind: PlayerKind,
}

/// Who plays a seat.
///
/// Mirrors `lands_core::state::Controller`, but names the AI profile as a
/// field so the file reads `Ai(profile: "aggressive-2")` rather than
/// `Ai("aggressive-2")` — an author should not have to remember what the
/// string means.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerKind {
    Human,
    /// Names a profile in `assets/ai/`.
    Ai {
        profile: String,
    },
}

/// One tile the author set by hand, overriding whatever the seed grew there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "Tile")]
pub struct TileOverride {
    /// Index into the planet's tiles, `0..tile_count(freq)`.
    pub id: u32,
    pub terrain: Terrain,
    /// Meaningful only on land; water must carry [`TileKind::Empty`].
    pub kind: TileKind,
    pub owner: Option<Faction>,
}

impl Level {
    /// Tiles on the planet this level is played on.
    pub const fn tile_count(&self) -> u32 {
        tile_count(self.freq)
    }

    /// Whether this is an authoring level with no procedural terrain.
    pub const fn is_ocean(&self) -> bool {
        self.seed == OCEAN_SEED
    }

    /// The player holding a faction, if any.
    pub fn player(&self, faction: Faction) -> Option<&LevelPlayer> {
        self.players.iter().find(|p| p.faction == faction)
    }
}
