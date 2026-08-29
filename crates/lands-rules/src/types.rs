//! The shape of the balance data.
//!
//! Field docs quote the spec rule id they implement (see `spec/rules/`).
//! Gate 12 cross-references those ids against the test suite.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What occupies a land tile. Water tiles have no kind.
///
/// `BTreeMap` (never `HashMap`) is used for any collection keyed by this,
/// because iteration order must be identical on every machine and every run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TileKind {
    Empty,
    Capital,
    Town,
    Field,
    Forest,
}

impl TileKind {
    /// Every variant, in a fixed order. Used for exhaustive validation.
    pub const ALL: [TileKind; 5] = [
        TileKind::Empty,
        TileKind::Capital,
        TileKind::Town,
        TileKind::Field,
        TileKind::Forest,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum UnitKind {
    Pawn,
    Warrior,
    Knight,
}

impl UnitKind {
    pub const ALL: [UnitKind; 3] = [UnitKind::Pawn, UnitKind::Warrior, UnitKind::Knight];
}

/// Per-turn resource change contributed by one tile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Yield {
    pub wheat: i32,
    pub gold: i32,
}

/// A cost that scales with how many of a thing the territory already holds.
///
/// Every build and upgrade price in the game has this shape:
/// `base + per_existing * count`, counted **within the territory only**,
/// never across the faction. See `spec/rules/economy.md` ECON-010..ECON-014.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScalingCost {
    pub base: i32,
    pub per_existing: i32,
}

impl ScalingCost {
    /// Price given how many qualifying things the territory already holds.
    pub fn at(&self, existing: u32) -> i32 {
        self.base + self.per_existing * existing as i32
    }
}

/// Towns are the only conditional producer: they convert wheat into gold, and
/// only whole towns are fed. See ECON-004.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TownRules {
    /// Wheat one town consumes per turn.
    pub wheat_cost: i32,
    /// Gold one *fed* town produces per turn.
    pub gold_yield: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Economy {
    /// Unconditional per-turn yield of each tile kind. Towns are absent here
    /// because their yield depends on wheat availability (see [`TownRules`]).
    pub tile_yields: BTreeMap<TileKind, Yield>,
    pub town: TownRules,
    /// Starting treasury of a freshly founded capital.
    pub starting_wheat: i32,
    pub starting_gold: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Costs {
    /// ECON-010. Scales with units of *any* kind in the territory.
    pub recruit_pawn: ScalingCost,
    /// ECON-011. Scales with warriors + knights only — never with pawns.
    pub upgrade_warrior: ScalingCost,
    /// ECON-012. Scales with knights only.
    pub upgrade_knight: ScalingCost,
    /// ECON-013. Scales with towns in the territory.
    pub build_town: ScalingCost,
    /// ECON-014. Scales with fields in the territory.
    pub build_field: ScalingCost,
}

/// What one unit kind costs to keep and what it may do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitProfile {
    /// UNIT-002. Wheat (and, for knights, gold) consumed per turn.
    pub upkeep: Yield,
    /// UNIT-010. Tile kinds this unit may capture from another faction.
    pub captures: Vec<TileKind>,
    /// UNIT-011. Enemy unit kinds this unit may defeat by moving onto them.
    pub defeats: Vec<UnitKind>,
    /// UNIT-020. What this unit may be upgraded into, if anything.
    pub upgrades_to: Option<UnitKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRules {
    pub profiles: BTreeMap<UnitKind, UnitProfile>,
    /// UNIT-030. Steps a unit may take through tiles its own faction owns.
    pub own_territory_steps: u32,
    /// UNIT-031. Additional steps into tiles the faction does not own. The
    /// capturing step is taken *after* the in-territory movement, in the same
    /// action (UNIT-031).
    pub foreign_steps: u32,
    /// ECON-005. Order in which units starve when wheat runs short. Earlier
    /// entries die first; within one kind the oldest unit dies first.
    pub starvation_priority: Vec<UnitKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerritoryRules {
    /// TERR-020. Percentage of the victim territory's gold the captor gains
    /// when taking a capital. Integer percent; the result is floored.
    pub capital_loot_percent: i32,
    /// TERR-011. Preference order when a destroyed capital must be rehoused.
    /// If none of these kinds exist in the territory, it is disbanded.
    pub capital_relocation_preference: Vec<TileKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrowthRules {
    /// GROW-001. Percent chance per forest tile per turn to spread.
    pub forest_spread_percent: u32,
    /// GROW-002. Tile kinds a forest may spread onto.
    pub forest_spread_targets: Vec<TileKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VictoryRules {
    /// VICT-002. Share of all land tiles that counts as an unbeatable lead.
    pub dominance_threshold_percent: u32,
    /// VICT-003. Consecutive turns the lead must be held.
    pub dominance_turns: u32,
}

/// The complete balance of one game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruleset {
    /// Bumped when the *shape* of this struct changes, so old files fail loudly.
    pub version: u32,
    pub economy: Economy,
    pub costs: Costs,
    pub units: UnitRules,
    pub territory: TerritoryRules,
    pub growth: GrowthRules,
    pub victory: VictoryRules,
}

/// The schema version this build understands.
pub const RULESET_VERSION: u32 = 1;

impl Ruleset {
    /// Unconditional yield of a tile kind. Towns yield nothing here; their
    /// output is computed during upkeep because it depends on wheat.
    pub fn tile_yield(&self, kind: TileKind) -> Yield {
        self.economy
            .tile_yields
            .get(&kind)
            .copied()
            .unwrap_or_default()
    }

    /// Profile of a unit kind. Validation guarantees every kind is present.
    pub fn unit(&self, kind: UnitKind) -> &UnitProfile {
        self.units
            .profiles
            .get(&kind)
            .expect("validated ruleset has a profile for every unit kind")
    }
}
