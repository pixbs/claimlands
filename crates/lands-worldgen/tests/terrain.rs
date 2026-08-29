//! The planet's coastlines, pinned and stress-tested.
//!
//! Two things are checked here, and they fail for different reasons.
//!
//! The **snapshots** pin a seed to a land mask. A level stores a seed rather
//! than a map, so a seed *is* the planet: if one of these hashes moves, every
//! level ever authored is now set on a different world, with its capitals
//! possibly in the sea. That is the same class of failure `snapshot.rs` guards
//! for the tile numbering, and it is caught here for the same reason — far from
//! where it would otherwise surface.
//!
//! The **properties** check the two things the generator promises for *every*
//! seed rather than for the pinned ones: that the land share is what it claims
//! to be, and that no tile is left alone in the water or alone in the land. A
//! snapshot cannot see either; it only knows that today's answer equals
//! yesterday's, including when both are wrong.

use lands_core::prelude::TileId;
use lands_core::rng::Rng;
use lands_worldgen::goldberg::{Goldberg, goldberg};
use lands_worldgen::terrain::{TerrainMap, target_land, terrain};

/// Frequency 8 — 642 tiles, the size `docs/architecture.md` uses for a level.
const FREQUENCY: u32 = 8;

/// How many seeds each property is asked to hold for.
const SEEDS: usize = 200;

/// Frequency, and how far the land count may miss its target on a planet that
/// size.
///
/// The miss is granularity rather than error: a tile is land or it is not, and
/// moving the cut past one tile's elevation can move the smoothed count by more
/// than one, so an exact hit is not always on offer at all — which is why it
/// shrinks as the planet grows. `the_cut_is_the_best_available_one` in
/// `src/terrain.rs` is what checks the miss is the smallest one available;
/// these are only its size.
const TOLERANCES: &[(u32, usize)] = &[(4, 3), (6, 2), (8, 2), (10, 2), (12, 2)];

/// Frequency, world seed, the land count they produce, and the fingerprint of
/// the mask.
///
/// Small, middling and large planets, with seeds that share no bits worth
/// speaking of — a generator that had lost its dependence on the seed, or on
/// the frequency, would still pass a single row.
const PINNED: &[(u32, u64, usize, u64)] = &[
    (2, 0, 19, 0x8aca_a4aa_21cb_e920),
    (4, 1, 68, 0x5b40_9245_9370_bcd2),
    (8, 0, 270, 0x617a_eb5e_4096_637d),
    (8, 0xc0ff_ee00_1234_5678, 270, 0xf298_8e9e_f343_dd96),
    (12, 0xffff_ffff_ffff_ffff, 606, 0x6ec0_2fa3_30d3_5dab),
];

fn planet_and_terrain(frequency: u32, seed: u64) -> (Goldberg, TerrainMap) {
    let planet = goldberg(frequency);
    let terrain = terrain(&planet, seed);
    (planet, terrain)
}

/// The seeds every property below is asked to hold for.
///
/// Drawn from the crate's own generator rather than counted up from zero, so
/// they are spread across the whole of `u64` — and reproducible, so a failure
/// names a seed that can be put straight back in.
fn seeds() -> Vec<u64> {
    let mut rng = Rng::seed_from_u64(0x5EED_5EED_5EED_5EED);
    (0..SEEDS).map(|_| rng.next_u64()).collect()
}

/// Land neighbours of a tile, counted from the finished terrain.
fn land_neighbors(planet: &Goldberg, terrain: &TerrainMap, tile: TileId) -> usize {
    planet
        .cell(tile)
        .neighbors()
        .iter()
        .filter(|&&n| terrain.is_land(TileId(n)))
        .count()
}

#[test]
fn the_pinned_planets_have_not_moved() {
    for &(frequency, seed, _, hash) in PINNED {
        let (_, terrain) = planet_and_terrain(frequency, seed);
        assert_eq!(
            terrain.terrain_hash(),
            hash,
            "the terrain at n={frequency}, seed {seed:#x} changed; every level \
             authored on that seed is now set on a different planet"
        );
    }
}

#[test]
fn the_snapshot_describes_the_planet_it_claims_to() {
    // A hash alone cannot say what it hashed. The land count beside it pins the
    // shape the number is a fingerprint *of*, so a reader can tell whether a
    // moved hash means "a different coastline" or "no land at all".
    for &(frequency, seed, land, _) in PINNED {
        let (planet, terrain) = planet_and_terrain(frequency, seed);
        assert_eq!(terrain.tile_count(), planet.tile_count());
        assert_eq!(
            terrain.land_count(),
            land,
            "at n={frequency}, seed {seed:#x}"
        );
        assert_eq!(terrain.water_count(), planet.tile_count() - land);
        // And near enough the target that the row is not pinning a bug.
        assert!(land.abs_diff(target_land(planet.tile_count())) <= 3);
    }
}

#[test]
fn regenerating_reproduces_the_snapshot() {
    // Guards against a result that depends on allocator addresses or a hash
    // seed rather than on the world seed — the class of bug a single run
    // cannot see.
    let planet = goldberg(FREQUENCY);
    let first = terrain(&planet, 12345);
    for _ in 0..4 {
        assert_eq!(terrain(&planet, 12345), first);
    }
}

#[test]
fn the_land_share_holds_for_every_seed() {
    // The reason the cut is a quantile at all: with a fixed threshold the sum
    // of seven waves would give one seed an ocean world and the next a
    // supercontinent, and a level author choosing a seed would be rolling for a
    // playable amount of land.
    for &(frequency, tolerance) in TOLERANCES {
        let planet = goldberg(frequency);
        let target = target_land(planet.tile_count());

        for seed in seeds() {
            let land = terrain(&planet, seed).land_count();
            assert!(
                land.abs_diff(target) <= tolerance,
                "n={frequency}, seed {seed:#x}: {land} land tiles of {}, wanted {target}",
                planet.tile_count()
            );
        }
    }
}

#[test]
fn no_tile_is_left_alone_in_the_water_or_in_the_land() {
    // A one-tile island is unreachable and a one-tile lake is unusable: both
    // are tiles the player can see and can do nothing with, and both are what
    // a threshold through a wave field produces in quantity.
    for &(frequency, _) in TOLERANCES {
        let planet = goldberg(frequency);

        for seed in seeds() {
            let terrain = terrain(&planet, seed);
            for tile in (0..planet.tile_count() as u32).map(TileId) {
                let land = land_neighbors(&planet, &terrain, tile);
                let sides = planet.cell(tile).sides();
                if terrain.is_land(tile) {
                    assert!(
                        land > 0,
                        "n={frequency}, seed {seed:#x}: {tile:?} is an island of one"
                    );
                } else {
                    assert!(
                        land < sides,
                        "n={frequency}, seed {seed:#x}: {tile:?} is a lake of one"
                    );
                }
            }
        }
    }
}

#[test]
fn different_seeds_give_different_planets() {
    // Two planets of the same size and different seeds must differ, or the
    // waves are not being drawn from the seed at all — which a snapshot test
    // would happily pin.
    let planet = goldberg(FREQUENCY);
    let mut hashes: Vec<u64> = seeds()
        .into_iter()
        .map(|seed| terrain(&planet, seed).terrain_hash())
        .collect();
    hashes.sort_unstable();
    let before = hashes.len();
    hashes.dedup();
    assert_eq!(hashes.len(), before, "two seeds produced the same planet");
}

#[test]
fn every_tile_is_land_or_water_and_the_counts_agree() {
    let (planet, terrain) = planet_and_terrain(FREQUENCY, 99);
    assert_eq!(terrain.tiles().len(), planet.tile_count());
    let counted = terrain
        .tiles()
        .iter()
        .filter(|&&t| t == lands_core::prelude::Terrain::Land)
        .count();
    assert_eq!(counted, terrain.land_count());
    assert_eq!(
        terrain.land_count() + terrain.water_count(),
        planet.tile_count()
    );
    for tile in (0..planet.tile_count() as u32).map(TileId) {
        assert_eq!(
            terrain.is_land(tile),
            terrain.get(tile) == lands_core::prelude::Terrain::Land
        );
    }
}
