//! Cover: villages, woods and farmland, grown outward in clumps.
//!
//! Ported from `seedCover` in `reference/prototype/hex-planet.html`.
//!
//! Each kind picks a bare land tile at random, claims it, and then spreads to
//! its neighbours with a probability that decays every ring — so cover arrives
//! in blobs rather than as confetti. One lone hex of wheat is not a farm: the
//! renderer merges neighbouring tiles of the same kind into a single parcelled
//! zone, and that only shows when tiles come in groups.
//!
//! # The order is the algorithm
//!
//! Villages first, then woods, then fields. The prototype says why, and it is
//! worth repeating because it is the one thing here that cannot be recovered
//! from the output: **clearings cut out of forest read as settled land, whereas
//! forest dropped into the gaps between fields reads as leftovers.** Each pass
//! sees what the passes before it claimed and grows around them, so reversing
//! two kinds does not merely relabel tiles — it produces a differently shaped
//! planet. [`Cover::ALL`] is that order, and `tests/cover.rs` pins it.
//!
//! # Integers only
//!
//! Nothing here is geometric. A clump is grown over the tile *graph*, so the
//! whole pass is counting and comparison, and it is written without a single
//! floating-point operation even though this crate is allowed them. That is
//! not decoration: cover decides a tile's [`TileKind`], which the simulation
//! plays on, so it has to come out identical on a phone and on a CI runner. See
//! `docs/determinism.md`.
//!
//! Probabilities are therefore carried as parts per million rather than as
//! percentages, because a percentage is too coarse to decay: `0.85 * 0.62³` is
//! `0.2026`, a hair above the floor the growth stops at, and truncating to
//! whole percents at every step loses that ring. Parts per million reproduce
//! the prototype's ring counts exactly.
//!
//! # The graph, not the fan
//!
//! Neighbours come from [`Goldberg::topology`] — the sorted adjacency the
//! simulation is given — and not from `Cell::neighbors`, which runs round the
//! corner fan. The two hold the same tiles in different orders, and the order
//! decides which neighbour is offered a clump first. Reading the sorted graph
//! means a change that renumbers corners without changing the tiling (see
//! `tests/snapshot.rs`) leaves every planet's cover exactly where it was.
//!
//! # Where the numbers live
//!
//! In `assets/worldgen/cover.ron`, loaded as [`CoverRules`]. That file explains
//! why it is separate from `assets/rules/default.ron`.

#![deny(clippy::float_arithmetic)]

use crate::digest::Digest;
use crate::goldberg::Goldberg;
use crate::terrain::TerrainMap;
use lands_core::prelude::{TileId, TileKind, Topology};
use lands_core::rng::{Rng, SeedDomain, stream};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the seeder puts on a land tile.
///
/// A worldgen vocabulary rather than a subset of [`TileKind`], because the
/// declaration order below *is* the seeding order and a type is the right place
/// to keep something load-bearing. [`Self::tile_kind`] is the seam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Cover {
    /// The prototype's `houses`: a cluster of dwellings, which the game plays
    /// as a town.
    Village,
    Forest,
    Field,
}

impl Cover {
    /// Every kind, **in the order they are seeded in**.
    ///
    /// Villages claim first, woods fill in around them, fields take what is
    /// left. Reordering this is a change to the planet, not a refactor — see
    /// the module docs, and `villages_are_seeded_before_woods_and_fields` in
    /// `tests/cover.rs`, which fails if the passes ever run in another order.
    pub const ALL: [Cover; 3] = [Cover::Village, Cover::Forest, Cover::Field];

    /// What the simulation calls this.
    pub const fn tile_kind(self) -> TileKind {
        match self {
            Cover::Village => TileKind::Town,
            Cover::Forest => TileKind::Forest,
            Cover::Field => TileKind::Field,
        }
    }

    /// Position in [`Self::ALL`], used to index the per-kind tallies.
    const fn ordinal(self) -> usize {
        match self {
            Cover::Village => 0,
            Cover::Forest => 1,
            Cover::Field => 2,
        }
    }
}

/// The two bits a tile is fingerprinted as. Zero means bare.
const fn cover_code(cover: Option<Cover>) -> u32 {
    match cover {
        None => 0,
        Some(Cover::Village) => 1,
        Some(Cover::Forest) => 2,
        Some(Cover::Field) => 3,
    }
}

/// How much of a planet one kind takes, and how eagerly its clumps spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverKindRules {
    /// Share of the planet's **land** this kind claims, as a percentage.
    pub share_percent: u32,
    /// How likely the first ring around a seed tile is to join it, as a
    /// percentage. Every ring after that is
    /// [`CoverRules::ring_decay_percent`] of the one before.
    pub cling_percent: u32,
}

/// The contents of `assets/worldgen/cover.ron`.
///
/// Fields are public so a test can vary one number without touching code —
/// which is half the point of keeping them in data at all (ADR 0006). A
/// ruleset built by hand is not validated; [`Self::from_ron`] and
/// [`Self::validate`] are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverRules {
    /// Format version. Bumped only when the *shape* of this file changes, so
    /// that an old file fails loudly instead of deserialising into defaults.
    pub version: u32,
    /// What a ring's cling probability is multiplied by to get the next
    /// ring's, as a percentage.
    pub ring_decay_percent: u32,
    /// A clump stops growing once its cling probability falls to this.
    pub ring_floor_percent: u32,
    /// The numbers for each kind. Order here is irrelevant: the seeding order
    /// is [`Cover::ALL`].
    pub kinds: BTreeMap<Cover, CoverKindRules>,
}

/// The version of the file layout this build understands.
const VERSION: u32 = 1;

/// The cover rules, compiled into the binary.
///
/// Embedded rather than read from disk for the reason `lands-rules` embeds
/// its own: the tests, the CLI and the mobile shells must all agree without
/// any filesystem setup.
pub const DEFAULT_COVER_RON: &str = include_str!("../../../assets/worldgen/cover.ron");

/// Why a set of cover rules was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoverRulesError {
    #[error("the cover rules could not be parsed: {0}")]
    Parse(String),

    #[error(
        "the cover rules are version {found}; this build understands version {expected}. \
         The file's shape changed, so its numbers cannot be read safely."
    )]
    Version { found: u32, expected: u32 },

    #[error(
        "no entry for {0:?}. Every kind in `Cover::ALL` needs one, or that kind \
         would silently never appear on a planet."
    )]
    Missing(Cover),

    #[error("{field} is {value}, which is outside {min}..={max}")]
    OutOfRange {
        field: &'static str,
        value: u32,
        min: u32,
        max: u32,
    },

    #[error(
        "the shares add up to {total}% of the land, so the last kind seeded \
         would run out of room before it met its budget"
    )]
    Oversubscribed { total: u32 },
}

impl CoverRules {
    /// Parse cover rules from RON text, then validate them.
    pub fn from_ron(text: &str) -> Result<Self, CoverRulesError> {
        let rules: CoverRules =
            ron::from_str(text).map_err(|e| CoverRulesError::Parse(e.to_string()))?;
        rules.validate()?;
        Ok(rules)
    }

    /// The bundled cover rules.
    ///
    /// Panics only if the compiled-in RON is malformed, which the
    /// `the_bundled_rules_are_valid` test makes impossible to land.
    pub fn bundled() -> Self {
        Self::from_ron(DEFAULT_COVER_RON).expect("bundled cover rules must parse and validate")
    }

    /// Everything that has to hold before a planet is seeded from these.
    ///
    /// The first problem found stops the check rather than every problem being
    /// collected, because this file is a dozen lines long and whoever broke it
    /// is looking straight at it — unlike a pull request body, where each
    /// failure costs a CI round trip to discover.
    pub fn validate(&self) -> Result<(), CoverRulesError> {
        if self.version != VERSION {
            return Err(CoverRulesError::Version {
                found: self.version,
                expected: VERSION,
            });
        }

        // A decay of 100% never shrinks, so a clump would grow until it ran out
        // of budget and swallow the planet whole; 0% is a floor by another
        // name and is said properly with `ring_floor_percent`.
        in_range("ring_decay_percent", self.ring_decay_percent, 1, 99)?;
        in_range("ring_floor_percent", self.ring_floor_percent, 0, 100)?;

        let mut total = 0;
        for kind in Cover::ALL {
            let rules = self
                .kinds
                .get(&kind)
                .ok_or(CoverRulesError::Missing(kind))?;
            in_range("share_percent", rules.share_percent, 0, 100)?;
            in_range("cling_percent", rules.cling_percent, 0, 100)?;
            total += rules.share_percent;
        }

        // Strictly under the whole planet. At exactly 100% the rounding of
        // three separate shares can ask for more tiles than there are, and the
        // kind that happens to be seeded last is the one that silently comes up
        // short — a bug that looks like a tuning decision.
        if total >= 100 {
            return Err(CoverRulesError::Oversubscribed { total });
        }

        Ok(())
    }
}

/// One bounds check, phrased the way the error reads.
fn in_range(field: &'static str, value: u32, min: u32, max: u32) -> Result<(), CoverRulesError> {
    if (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(CoverRulesError::OutOfRange {
            field,
            value,
            min,
            max,
        })
    }
}

/// What sits on each tile of a planet.
///
/// Indexed by [`TileId`], parallel to [`Goldberg::cells`] and to
/// [`TerrainMap`]. Water tiles and bare land both read `None`; which of the two
/// a tile is, is the terrain's business, not this map's. Build one with
/// [`cover`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverMap {
    tiles: Vec<Option<Cover>>,
    counts: [usize; Cover::ALL.len()],
}

impl CoverMap {
    /// How many tiles the planet has.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// What is on a tile, if anything.
    pub fn get(&self, tile: TileId) -> Option<Cover> {
        self.tiles[tile.index()]
    }

    /// What the simulation should call this tile, if the seeder claimed it.
    pub fn tile_kind(&self, tile: TileId) -> Option<TileKind> {
        self.get(tile).map(Cover::tile_kind)
    }

    /// Every tile, indexed by tile id — what a level builder wants in one go.
    pub fn tiles(&self) -> &[Option<Cover>] {
        &self.tiles
    }

    /// How many tiles one kind claimed.
    pub fn count(&self, cover: Cover) -> usize {
        self.counts[cover.ordinal()]
    }

    /// How many tiles carry any cover at all.
    pub fn covered_count(&self) -> usize {
        self.counts.iter().sum()
    }

    /// A fingerprint of the layout: the tile count, then two bits per tile
    /// packed sixteen to a word, lowest id in the lowest bits.
    ///
    /// Integers only, for the reason [`crate::digest`] gives — and here that
    /// costs nothing at all, because no step from the seed to this map ever
    /// touches a float.
    pub fn cover_hash(&self) -> u64 {
        let mut d = Digest::new();
        d.u32(self.tiles.len() as u32);
        for word in self.tiles.chunks(16) {
            let mut bits = 0u32;
            for (slot, &cover) in word.iter().enumerate() {
                bits |= cover_code(cover) << (slot * 2);
            }
            d.u32(bits);
        }
        d.finish()
    }
}

/// How many tiles a share of `percent` of `land` comes to: the nearest whole
/// tile, in integers for the reason [`crate::terrain::target_land`] gives.
fn tiles_for_share(land: usize, percent: u32) -> usize {
    (land * percent as usize + 50) / 100
}

/// Probabilities are carried in parts per million. See the module docs.
const PPM: u32 = 1_000_000;

/// A percentage as parts per million.
const fn ppm(percent: u32) -> u32 {
    percent * (PPM / 100)
}

/// How one kind's clumps spread: what the first ring joins at, how fast that
/// thins, and where it stops.
///
/// The three travel together rather than as three bare `u32` arguments, which
/// would be three chances to pass two of them the wrong way round and no
/// compiler error for doing it.
#[derive(Debug, Clone, Copy)]
struct Clump {
    /// Parts per million that a neighbour of the current ring joins with.
    cling: u32,
    /// Percent of this ring's probability the next ring gets.
    decay: u32,
    /// Growth stops once `cling` falls to this, in parts per million.
    floor: u32,
}

impl Clump {
    fn new(kind: &CoverKindRules, rules: &CoverRules) -> Self {
        Self {
            cling: ppm(kind.cling_percent),
            decay: rules.ring_decay_percent,
            floor: ppm(rules.ring_floor_percent),
        }
    }

    /// Whether a ring at this probability is still worth growing.
    fn spreading(self) -> bool {
        self.cling > self.floor
    }

    /// The next ring out.
    fn thinned(self) -> Self {
        Self {
            cling: self.cling * self.decay / 100,
            ..self
        }
    }
}

/// Seed the cover of a planet from a world seed.
///
/// The terrain must be the one generated for this planet: cover only ever
/// lands on tiles it says are land.
///
/// # Panics
///
/// If `terrain` describes a different number of tiles than `planet` does.
pub fn cover(
    planet: &Goldberg,
    terrain: &TerrainMap,
    world_seed: u64,
    rules: &CoverRules,
) -> CoverMap {
    assert_eq!(
        planet.tile_count(),
        terrain.tile_count(),
        "the terrain describes a different planet than the one being covered"
    );

    let topology = planet.topology();
    let land: Vec<TileId> = topology.tiles().filter(|&t| terrain.is_land(t)).collect();

    // Cover is seeded once, before the first turn, and is the only consumer of
    // its domain — hence turn 0 and entity 0. One stream serves all three
    // passes, because they are not independent: each grows around what the
    // ones before it claimed, so a separate stream per kind would suggest a
    // separation that does not exist.
    let mut seeder = Seeder {
        topology: &topology,
        terrain,
        tiles: vec![None; planet.tile_count()],
        rng: stream(world_seed, SeedDomain::CoverSeeding, 0, 0),
    };

    let mut counts = [0; Cover::ALL.len()];

    for kind in Cover::ALL {
        // A kind with no entry claims nothing rather than panicking. Validation
        // is what makes that loud; this only decides what an unvalidated
        // ruleset does, and quietly missing cover beats a crash in a level
        // loader.
        let Some(kind_rules) = rules.kinds.get(&kind) else {
            continue;
        };
        let budget = tiles_for_share(land.len(), kind_rules.share_percent);
        counts[kind.ordinal()] = seeder.seed(kind, &land, budget, Clump::new(kind_rules, rules));
    }

    CoverMap {
        tiles: seeder.tiles,
        counts,
    }
}

/// One planet's worth of half-finished cover, and the stream growing it.
#[derive(Debug)]
struct Seeder<'a> {
    topology: &'a Topology,
    terrain: &'a TerrainMap,
    tiles: Vec<Option<Cover>>,
    rng: Rng,
}

impl Seeder<'_> {
    /// Claim `budget` tiles for one kind, in clumps. Returns how many it got.
    ///
    /// Fewer than asked only when the land genuinely ran out, which the bundled
    /// shares cannot cause: they claim 44% of it between all three kinds.
    fn seed(&mut self, kind: Cover, land: &[TileId], budget: usize, clump: Clump) -> usize {
        let mut left = budget;
        while left > 0 {
            let Some(start) = self.draw_bare(land) else {
                break; // Every land tile is spoken for.
            };
            self.tiles[start.index()] = Some(kind);
            left -= 1;
            self.grow(kind, start, &mut left, clump);
        }
        budget - left
    }

    /// Spread out from `start`, one ring at a time, thinning as it goes.
    ///
    /// A ring is every tile that joined in the pass before it. Each of their
    /// bare land neighbours is offered the clump once, at the ring's cling
    /// probability, and the ones that take it become the next ring. Growth
    /// stops when the budget is spent, when a ring recruits nobody, or when the
    /// probability has decayed to the floor.
    fn grow(&mut self, kind: Cover, start: TileId, left: &mut usize, clump: Clump) {
        let mut ring = vec![start];
        let mut clump = clump;

        while *left > 0 && !ring.is_empty() && clump.spreading() {
            let mut next = Vec::new();
            'ring: for &tile in &ring {
                for &neighbor in self.topology.neighbors(tile) {
                    if *left == 0 {
                        break 'ring;
                    }
                    // The draw comes last, and only for a tile that could
                    // actually take the cover. Spending one on a tile that is
                    // water, or already claimed, would make the layout depend
                    // on how much of the planet happened to be sea.
                    if self.tiles[neighbor.index()].is_some() || !self.terrain.is_land(neighbor) {
                        continue;
                    }
                    if !self.clings(clump.cling) {
                        continue;
                    }
                    self.tiles[neighbor.index()] = Some(kind);
                    *left -= 1;
                    next.push(neighbor);
                }
            }
            ring = next;
            clump = clump.thinned();
        }
    }

    /// A land tile with nothing on it yet, drawn uniformly.
    ///
    /// The prototype draws from all land and retries when it lands on an
    /// occupied tile, giving up after 900 attempts. Rejection sampling over the
    /// whole set *is* a uniform draw over the free part of it, so this is the
    /// same distribution with the arbitrary cap taken out: the budget is either
    /// met or the land genuinely ran out, never "the guard expired". That is
    /// what lets `tests/cover.rs` assert an exact share rather than a range.
    fn draw_bare(&mut self, land: &[TileId]) -> Option<TileId> {
        let bare = land
            .iter()
            .filter(|&&tile| self.tiles[tile.index()].is_none())
            .count();
        if bare == 0 {
            return None;
        }

        let mut skip = self.rng.below(bare as u32) as usize;
        for &tile in land {
            if self.tiles[tile.index()].is_some() {
                continue;
            }
            if skip == 0 {
                return Some(tile);
            }
            skip -= 1;
        }
        unreachable!("the bare tiles were counted from this same slice a moment ago")
    }

    /// True with probability `cling` in a million.
    fn clings(&mut self, cling: u32) -> bool {
        self.rng.below(PPM) < cling
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goldberg::goldberg;
    use crate::terrain::terrain;

    /// Small enough to run a few hundred planets, big enough to have clumps.
    const FREQUENCY: u32 = 4;

    fn bundled() -> CoverRules {
        CoverRules::bundled()
    }

    #[test]
    fn the_bundled_rules_are_valid() {
        let rules = bundled();
        assert!(rules.validate().is_ok());
        assert_eq!(rules.version, VERSION);
    }

    #[test]
    fn the_bundled_rules_are_the_prototypes_numbers() {
        // The three shares and the decay the issue names, read back out of the
        // data file rather than restated in code. If somebody tunes the file
        // this test is meant to fail: it is the record of what was ported.
        let rules = bundled();
        assert_eq!(rules.ring_decay_percent, 62);
        assert_eq!(rules.kinds[&Cover::Village].share_percent, 8);
        assert_eq!(rules.kinds[&Cover::Forest].share_percent, 20);
        assert_eq!(rules.kinds[&Cover::Field].share_percent, 16);
        assert_eq!(rules.kinds[&Cover::Village].cling_percent, 92);
        assert_eq!(rules.kinds[&Cover::Forest].cling_percent, 90);
        assert_eq!(rules.kinds[&Cover::Field].cling_percent, 85);
    }

    #[test]
    fn the_seeding_order_is_villages_then_woods_then_fields() {
        // The order is the algorithm; see the module docs. This pins the
        // constant, and `tests/cover.rs` pins that the passes actually run in
        // it.
        assert_eq!(Cover::ALL, [Cover::Village, Cover::Forest, Cover::Field]);
    }

    #[test]
    fn every_kind_maps_onto_a_tile_the_simulation_knows() {
        assert_eq!(Cover::Village.tile_kind(), TileKind::Town);
        assert_eq!(Cover::Forest.tile_kind(), TileKind::Forest);
        assert_eq!(Cover::Field.tile_kind(), TileKind::Field);
        // Never `Empty`, which is what a bare tile is, and never `Capital`,
        // which a level places rather than the seeder.
        for kind in Cover::ALL {
            assert_ne!(kind.tile_kind(), TileKind::Empty);
            assert_ne!(kind.tile_kind(), TileKind::Capital);
        }
    }

    #[test]
    fn a_share_is_the_nearest_whole_tile() {
        assert_eq!(tiles_for_share(270, 8), 22); // 21.6
        assert_eq!(tiles_for_share(270, 20), 54);
        assert_eq!(tiles_for_share(270, 16), 43); // 43.2
        assert_eq!(tiles_for_share(0, 20), 0);
        assert_eq!(tiles_for_share(100, 0), 0);
        // The case floating point gets wrong: 50 * 0.16 is 8.000000000000002,
        // so `round` and `floor` disagree about a product that is exactly 8.
        assert_eq!(tiles_for_share(50, 16), 8);
    }

    #[test]
    fn a_clump_gets_the_rings_the_prototype_gives_it() {
        // The prototype multiplies by 0.62 and stops at 0.2. Whole percents
        // would lose the fourth ring of an 85% clump — 0.85 * 0.62^3 is 0.2026,
        // and 85 * 62 / 100 three times over is 19. Parts per million keep it.
        let rules = bundled();
        let floor = ppm(rules.ring_floor_percent);

        for (kind, expected) in [(Cover::Village, 4), (Cover::Forest, 4), (Cover::Field, 4)] {
            let mut cling = ppm(rules.kinds[&kind].cling_percent);
            let mut rings = 0;
            while cling > floor {
                rings += 1;
                cling = cling * rules.ring_decay_percent / 100;
            }
            assert_eq!(rings, expected, "{kind:?} grew {rings} rings");
        }
    }

    #[test]
    fn validation_rejects_rules_that_cannot_produce_a_planet() {
        let over = |mutate: fn(&mut CoverRules)| {
            let mut rules = bundled();
            mutate(&mut rules);
            rules.validate().expect_err("should have been rejected")
        };

        assert_eq!(
            over(|r| r.version = 2),
            CoverRulesError::Version {
                found: 2,
                expected: 1
            }
        );
        // 100% never shrinks, so one clump would eat the planet.
        assert!(matches!(
            over(|r| r.ring_decay_percent = 100),
            CoverRulesError::OutOfRange { .. }
        ));
        assert!(matches!(
            over(|r| r.kinds.get_mut(&Cover::Field).unwrap().share_percent = 101),
            CoverRulesError::OutOfRange { .. }
        ));
        assert_eq!(
            over(|r| {
                r.kinds.remove(&Cover::Forest);
            }),
            CoverRulesError::Missing(Cover::Forest)
        );
        assert_eq!(
            over(|r| r.kinds.get_mut(&Cover::Forest).unwrap().share_percent = 80),
            CoverRulesError::Oversubscribed { total: 104 }
        );
    }

    #[test]
    fn malformed_rules_are_a_parse_error_rather_than_a_panic() {
        assert!(matches!(
            CoverRules::from_ron("(this is not ron"),
            Err(CoverRulesError::Parse(_))
        ));
    }

    #[test]
    fn the_cover_comes_from_the_cover_seeding_domain_at_turn_zero() {
        // Which stream the cover is drawn from is not an implementation
        // detail: it is what ties a seed to a planet's villages. It also has to
        // be a stream of its own, because that is what stops this feature from
        // moving the terrain of every level ever authored.
        let planet = goldberg(FREQUENCY);
        let terrain = terrain(&planet, 7);
        let map = cover(&planet, &terrain, 7, &bundled());

        let mut elsewhere = Seeder {
            topology: &planet.topology(),
            terrain: &terrain,
            tiles: vec![None; planet.tile_count()],
            // The terrain domain, which must not produce this planet's cover.
            rng: stream(7, SeedDomain::Terrain, 0, 0),
        };
        let land: Vec<TileId> = planet
            .topology()
            .tiles()
            .filter(|&t| terrain.is_land(t))
            .collect();
        let rules = bundled();
        for kind in Cover::ALL {
            let kind_rules = rules.kinds[&kind];
            let budget = tiles_for_share(land.len(), kind_rules.share_percent);
            elsewhere.seed(kind, &land, budget, Clump::new(&kind_rules, &rules));
        }

        assert_ne!(
            map.tiles(),
            elsewhere.tiles.as_slice(),
            "the cover is not being drawn from its own domain"
        );
    }

    #[test]
    fn the_fingerprint_notices_a_single_tile() {
        let planet = goldberg(FREQUENCY);
        let terrain = terrain(&planet, 11);
        let map = cover(&planet, &terrain, 11, &bundled());

        let mut moved = map.clone();
        let tile = moved
            .tiles
            .iter()
            .position(Option::is_some)
            .expect("a planet has cover");
        moved.tiles[tile] = None;
        assert_ne!(map.cover_hash(), moved.cover_hash());

        // And notices a tile changing kind, not merely appearing.
        let mut relabelled = map.clone();
        relabelled.tiles[tile] = Some(match map.tiles[tile] {
            Some(Cover::Village) => Cover::Forest,
            _ => Cover::Village,
        });
        assert_ne!(map.cover_hash(), relabelled.cover_hash());
    }

    #[test]
    fn a_planet_with_no_land_gets_no_cover() {
        // Not reachable through `terrain`, which always produces land, but the
        // seeder is handed a land list and must stop rather than spin when it
        // is empty.
        let planet = goldberg(2);
        let ocean = terrain(&planet, 0);
        let mut seeder = Seeder {
            topology: &planet.topology(),
            terrain: &ocean,
            tiles: vec![None; planet.tile_count()],
            rng: stream(1, SeedDomain::CoverSeeding, 0, 0),
        };

        let clump = Clump::new(&bundled().kinds[&Cover::Forest], &bundled());
        assert_eq!(seeder.seed(Cover::Forest, &[], 5, clump), 0);
        assert!(seeder.tiles.iter().all(Option::is_none));
    }

    #[test]
    fn a_kind_missing_from_the_rules_claims_nothing() {
        let planet = goldberg(FREQUENCY);
        let terrain = terrain(&planet, 5);
        let mut rules = bundled();
        rules.kinds.remove(&Cover::Forest);

        let map = cover(&planet, &terrain, 5, &rules);
        assert_eq!(map.count(Cover::Forest), 0);
        assert!(map.count(Cover::Village) > 0);
        assert!(map.count(Cover::Field) > 0);
    }
}
