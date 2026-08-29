//! The world: everything the simulation knows.
//!
//! # The central entity is the Territory, not the Faction
//!
//! A [`Territory`] is a *connected component* of tiles owned by one faction.
//! Treasury, capital, and every build price are per-territory. A faction may
//! hold several at once, and they are created and destroyed constantly as
//! borders shift. Almost every subtle rule in the game is a statement about
//! territories rather than about players.
//!
//! # Determinism
//!
//! Every collection here is a `Vec` or a `BTreeMap`/`BTreeSet`, never a
//! `HashMap`. Hash iteration order varies between runs and would make replays
//! irreproducible. See docs/determinism.md.

use crate::ids::{Faction, TerritoryId, TileId, UnitId};
use crate::topology::Topology;
use lands_rules::{TileKind, UnitKind};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Water,
    Land,
}

/// One tile of the planet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    /// Meaningful only on land. Water tiles carry [`TileKind::Empty`] and are
    /// never owned, built on, or entered.
    pub kind: TileKind,
    pub owner: Option<Faction>,
    /// Which territory this tile belongs to. Always `Some` when `owner` is
    /// `Some` — the invariant checker enforces it.
    pub territory: Option<TerritoryId>,
    pub unit: Option<UnitId>,
}

impl Tile {
    pub fn water() -> Self {
        Self {
            terrain: Terrain::Water,
            kind: TileKind::Empty,
            owner: None,
            territory: None,
            unit: None,
        }
    }

    pub fn land(kind: TileKind) -> Self {
        Self {
            terrain: Terrain::Land,
            kind,
            owner: None,
            territory: None,
            unit: None,
        }
    }

    #[inline]
    pub fn is_land(&self) -> bool {
        self.terrain == Terrain::Land
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unit {
    pub id: UnitId,
    pub kind: UnitKind,
    pub faction: Faction,
    pub tile: TileId,
    /// Monotonic creation sequence, used to resolve "the oldest unit dies
    /// first" under famine (ECON-005). Survives upgrades, so upgrading a unit
    /// does not make it younger and therefore safer.
    pub born: u32,
    /// Whether this unit has already acted this turn. Moving and upgrading
    /// both consume it (UNIT-030, UNIT-021).
    pub moved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Territory {
    pub id: TerritoryId,
    pub faction: Faction,
    pub capital: TileId,
    pub wheat: i32,
    pub gold: i32,
    pub tiles: BTreeSet<TileId>,
}

impl Territory {
    #[inline]
    pub fn size(&self) -> u32 {
        self.tiles.len() as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Controller {
    Human,
    /// Names a profile in `assets/ai/`.
    Ai(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Player {
    pub faction: Faction,
    pub controller: Controller,
    /// Set once a faction holds no tiles at all. Eliminated players are skipped
    /// in the turn order but stay in the list so stats survive.
    pub eliminated: bool,
}

/// Per-faction running totals, reported on the victory screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionStats {
    pub units_recruited: u32,
    pub units_upgraded: u32,
    /// Units of this faction destroyed by an enemy.
    pub units_lost: u32,
    /// Enemy units this faction destroyed.
    pub units_killed: u32,
    /// Units of this faction lost to famine.
    pub units_starved: u32,
    pub gold_earned: u64,
    pub wheat_earned: u64,
    pub gold_looted: u64,
    pub tiles_captured: u32,
    pub tiles_lost: u32,
    pub towns_built: u32,
    pub fields_built: u32,
    pub capitals_razed: u32,
    pub peak_tiles: u32,
    pub peak_territories: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VictoryReason {
    /// Every rival faction holds no tiles (VICT-001).
    Elimination,
    /// Held at least the dominance threshold for long enough (VICT-002).
    Dominance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Victory {
        faction: Faction,
        reason: VictoryReason,
        /// Rounds elapsed — "how many steps it took to achieve the victory".
        round: u32,
    },
    /// Every faction was eliminated in the same resolution step.
    Draw { round: u32 },
}

/// The complete state of one match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    /// The seed the planet was generated from. Also the root of every random
    /// stream drawn during play (see [`crate::rng`]).
    pub seed: u64,
    /// Shared because [`World`] is cloned once per turn for undo, and the
    /// topology never changes during a match.
    pub topology: Arc<Topology>,

    /// Completed rounds. One round is one turn for every surviving player.
    pub round: u32,
    /// Index into `players` of whoever is acting.
    pub turn_index: usize,

    pub tiles: Vec<Tile>,
    pub units: BTreeMap<UnitId, Unit>,
    pub territories: BTreeMap<TerritoryId, Territory>,
    pub players: Vec<Player>,

    pub stats: BTreeMap<Faction, FactionStats>,
    /// Consecutive rounds each faction has met the dominance threshold.
    pub dominance_streak: BTreeMap<Faction, u32>,
    pub outcome: Option<Outcome>,

    next_unit: u32,
    next_territory: u32,
    next_born: u32,
}

impl World {
    /// An all-water planet with no players. Levels build on top of this.
    pub fn empty(seed: u64, topology: Arc<Topology>) -> Self {
        let tiles = vec![Tile::water(); topology.tile_count()];
        Self {
            seed,
            topology,
            round: 0,
            turn_index: 0,
            tiles,
            units: BTreeMap::new(),
            territories: BTreeMap::new(),
            players: Vec::new(),
            stats: BTreeMap::new(),
            dominance_streak: BTreeMap::new(),
            outcome: None,
            next_unit: 0,
            next_territory: 0,
            next_born: 0,
        }
    }

    // ---- tile access -----------------------------------------------------

    #[inline]
    pub fn tile(&self, id: TileId) -> &Tile {
        &self.tiles[id.index()]
    }

    #[inline]
    pub fn tile_mut(&mut self, id: TileId) -> &mut Tile {
        &mut self.tiles[id.index()]
    }

    pub fn tile_ids(&self) -> impl Iterator<Item = TileId> + '_ {
        (0..self.tiles.len() as u32).map(TileId)
    }

    pub fn land_tile_count(&self) -> u32 {
        self.tiles.iter().filter(|t| t.is_land()).count() as u32
    }

    /// Tiles owned by a faction, ascending.
    pub fn tiles_of(&self, faction: Faction) -> BTreeSet<TileId> {
        self.tile_ids()
            .filter(|&t| self.tile(t).owner == Some(faction))
            .collect()
    }

    pub fn tile_count_of(&self, faction: Faction) -> u32 {
        self.tiles
            .iter()
            .filter(|t| t.owner == Some(faction))
            .count() as u32
    }

    // ---- territory access ------------------------------------------------

    pub fn territory(&self, id: TerritoryId) -> Option<&Territory> {
        self.territories.get(&id)
    }

    pub fn territory_mut(&mut self, id: TerritoryId) -> Option<&mut Territory> {
        self.territories.get_mut(&id)
    }

    /// The territory a tile belongs to, if it is owned.
    pub fn territory_at(&self, tile: TileId) -> Option<&Territory> {
        self.tile(tile)
            .territory
            .and_then(|id| self.territories.get(&id))
    }

    pub fn territories_of(&self, faction: Faction) -> impl Iterator<Item = &Territory> {
        self.territories
            .values()
            .filter(move |t| t.faction == faction)
    }

    /// How many tiles of a given kind a territory holds. Drives every scaling
    /// build cost (ECON-013, ECON-014).
    pub fn count_kind_in(&self, territory: TerritoryId, kind: TileKind) -> u32 {
        let Some(t) = self.territories.get(&territory) else {
            return 0;
        };
        t.tiles
            .iter()
            .filter(|&&id| self.tile(id).kind == kind)
            .count() as u32
    }

    // ---- unit access -----------------------------------------------------

    pub fn unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.get(&id)
    }

    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.units.get_mut(&id)
    }

    /// Units standing inside a territory, ascending by id.
    ///
    /// Units belong to territories only by where they stand: moving a unit
    /// across a border changes which territory pays for it.
    pub fn units_in(&self, territory: TerritoryId) -> Vec<UnitId> {
        let Some(t) = self.territories.get(&territory) else {
            return Vec::new();
        };
        let mut ids: Vec<UnitId> = t
            .tiles
            .iter()
            .filter_map(|&tile| self.tile(tile).unit)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Count of units in a territory whose kind passes `filter`.
    ///
    /// The three upgrade and recruit prices differ only in this filter
    /// (ECON-010 counts everything, ECON-011 counts warriors and knights,
    /// ECON-012 counts knights), so they share one implementation.
    pub fn count_units_in(&self, territory: TerritoryId, filter: impl Fn(UnitKind) -> bool) -> u32 {
        self.units_in(territory)
            .iter()
            .filter_map(|id| self.units.get(id))
            .filter(|u| filter(u.kind))
            .count() as u32
    }

    // ---- players ---------------------------------------------------------

    pub fn current_player(&self) -> Option<&Player> {
        self.players.get(self.turn_index)
    }

    pub fn current_faction(&self) -> Option<Faction> {
        self.current_player().map(|p| p.faction)
    }

    pub fn player(&self, faction: Faction) -> Option<&Player> {
        self.players.iter().find(|p| p.faction == faction)
    }

    pub fn is_over(&self) -> bool {
        self.outcome.is_some()
    }

    pub fn stats_mut(&mut self, faction: Faction) -> &mut FactionStats {
        self.stats.entry(faction).or_default()
    }

    pub fn stats_of(&self, faction: Faction) -> FactionStats {
        self.stats.get(&faction).cloned().unwrap_or_default()
    }

    // ---- id allocation ---------------------------------------------------

    /// Ids are never reused within a match, so a stale id is always detectably
    /// stale rather than silently pointing at a different object.
    pub fn alloc_unit_id(&mut self) -> UnitId {
        let id = UnitId(self.next_unit);
        self.next_unit += 1;
        id
    }

    pub fn alloc_territory_id(&mut self) -> TerritoryId {
        let id = TerritoryId(self.next_territory);
        self.next_territory += 1;
        id
    }

    /// Next creation-order stamp, for famine ordering.
    pub fn alloc_born(&mut self) -> u32 {
        let n = self.next_born;
        self.next_born += 1;
        n
    }
}
