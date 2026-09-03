//! What the seeder puts on a planet, pinned and stress-tested.
//!
//! Three kinds of check live here, and they fail for different reasons.
//!
//! The **snapshots** pin a seed to a cover layout. A level stores a seed rather
//! than a map, so a moved hash means every level authored on that seed now
//! starts with its villages and farmland somewhere else — the same class of
//! failure `terrain.rs` guards for the coastline.
//!
//! The **properties** check what the seeder promises for *every* seed rather
//! than for the pinned ones: cover only on land, each kind getting exactly the
//! share the data asks for, and the passes running in the order the algorithm
//! depends on.
//!
//! And one check exists because neither of the above can see it. Cover
//! scattered evenly and cover properly clumped produce **identical** share
//! statistics and are equally snapshot-able; the difference is the whole point
//! of the algorithm. `cover_arrives_in_clumps_rather_than_confetti` measures it
//! against a scattered control, which is the closest a test can get to looking
//! at the planet (ADR 0007 is about why looking is still needed).

use lands_core::prelude::TileId;
use lands_core::rng::Rng;
use lands_worldgen::cover::{Cover, CoverMap, CoverRules, cover};
use lands_worldgen::goldberg::{Goldberg, goldberg};
use lands_worldgen::terrain::{TerrainMap, terrain};

/// Frequency 8 — 642 tiles, the size `docs/architecture.md` uses for a level.
const FREQUENCY: u32 = 8;

/// How many seeds each property is asked to hold for.
const SEEDS: usize = 100;

/// The frequencies the properties are checked across: the smallest planet the
/// game allows, the one it plays at, and one larger.
const FREQUENCIES: &[u32] = &[2, 4, 8, 12];

/// Frequency, world seed, the tiles each kind claimed in seeding order, and the
/// fingerprint of the layout.
///
/// Small, middling and large planets, with seeds sharing no bits worth speaking
/// of — a seeder that had lost its dependence on the seed, or on the frequency,
/// would still pass a single row.
const PINNED: &[(u32, u64, [usize; 3], u64)] = &[
    (2, 0, [2, 4, 3], 0xccae_1ada_b9bd_90a6),
    (4, 1, [5, 14, 11], 0xd18b_1a9f_65a8_e753),
    (8, 0, [22, 54, 43], 0x6048_da58_d8eb_54a8),
    (
        8,
        0xc0ff_ee00_1234_5678,
        [22, 54, 43],
        0x113d_3021_7f25_924a,
    ),
    (
        12,
        0xffff_ffff_ffff_ffff,
        [48, 121, 97],
        0x9f12_8be9_c7d6_68b9,
    ),
];

fn planet_terrain_cover(frequency: u32, seed: u64) -> (Goldberg, TerrainMap, CoverMap) {
    let planet = goldberg(frequency);
    let land = terrain(&planet, seed);
    let map = cover(&planet, &land, seed, &CoverRules::bundled());
    (planet, land, map)
}

/// The seeds every property below is asked to hold for.
///
/// Drawn from the crate's own generator rather than counted up from zero, so
/// they are spread across the whole of `u64` — and reproducible, so a failure
/// names a seed that can be put straight back in.
fn seeds() -> Vec<u64> {
    let mut rng = Rng::seed_from_u64(0xC0FE_C0FE_C0FE_C0FE);
    (0..SEEDS).map(|_| rng.next_u64()).collect()
}

/// How many tiles of a share of `percent` of `land` come to. The same rounding
/// the seeder uses, restated here so the test is not checking the code against
/// itself.
fn share_of(land: usize, percent: usize) -> usize {
    (land * percent + 50) / 100
}

/// The configured share of each kind, in seeding order.
fn configured_shares() -> [usize; 3] {
    let rules = CoverRules::bundled();
    Cover::ALL.map(|kind| rules.kinds[&kind].share_percent as usize)
}

/// Every tile carrying `kind`, as ids.
fn tiles_of(map: &CoverMap, kind: Cover) -> Vec<TileId> {
    (0..map.tile_count() as u32)
        .map(TileId)
        .filter(|&t| map.get(t) == Some(kind))
        .collect()
}

/// How many neighbours of its own kind the average covered tile has, times a
/// hundred so the measure stays an integer.
///
/// Around six for cover grown in one solid mass, and around six times the
/// covered share of the planet for cover sprinkled tile by tile. A strictly
/// stronger measure than "touches at least one of its own kind", which a
/// planet this densely covered satisfies by accident half the time.
fn kinship(planet: &Goldberg, covered: &[Option<Cover>]) -> usize {
    let (mut tiles, mut same) = (0, 0);
    for (tile, &kind) in covered.iter().enumerate() {
        let Some(kind) = kind else { continue };
        tiles += 1;
        same += planet
            .cell(TileId(tile as u32))
            .neighbors()
            .iter()
            .filter(|&&n| covered[n as usize] == Some(kind))
            .count();
    }
    if tiles == 0 { 0 } else { same * 100 / tiles }
}

/// The same tile counts, scattered uniformly over the land instead of grown.
///
/// The control the clumping is measured against: it has identical share
/// statistics and a completely different planet.
fn scattered(
    planet: &Goldberg,
    land: &TerrainMap,
    map: &CoverMap,
    seed: u64,
) -> Vec<Option<Cover>> {
    let mut free: Vec<u32> = (0..planet.tile_count() as u32)
        .filter(|&t| land.is_land(TileId(t)))
        .collect();
    let mut out = vec![None; planet.tile_count()];
    let mut rng = Rng::seed_from_u64(seed);

    for kind in Cover::ALL {
        for _ in 0..map.count(kind) {
            if free.is_empty() {
                break;
            }
            let at = rng.below(free.len() as u32) as usize;
            out[free.swap_remove(at) as usize] = Some(kind);
        }
    }
    out
}

#[test]
fn the_pinned_planets_have_not_moved() {
    for &(frequency, seed, _, hash) in PINNED {
        let (_, _, map) = planet_terrain_cover(frequency, seed);
        assert_eq!(
            map.cover_hash(),
            hash,
            "the cover at n={frequency}, seed {seed:#x} changed; every level \
             authored on that seed now starts with different villages, woods \
             and farmland"
        );
    }
}

#[test]
fn the_snapshot_describes_the_planet_it_claims_to() {
    // A hash alone cannot say what it hashed. The counts beside it pin the
    // shape the number is a fingerprint *of*, so a reader can tell whether a
    // moved hash means "the clumps sit elsewhere" or "there is no cover left".
    for &(frequency, seed, counts, _) in PINNED {
        let (planet, land, map) = planet_terrain_cover(frequency, seed);
        assert_eq!(map.tile_count(), planet.tile_count());

        for (kind, &expected) in Cover::ALL.iter().zip(counts.iter()) {
            assert_eq!(
                map.count(*kind),
                expected,
                "{kind:?} at n={frequency}, seed {seed:#x}"
            );
        }
        assert_eq!(map.covered_count(), counts.iter().sum::<usize>());

        // And the counts are the configured shares of the land rather than a
        // pinned bug.
        for (kind, percent) in Cover::ALL.iter().zip(configured_shares()) {
            assert_eq!(map.count(*kind), share_of(land.land_count(), percent));
        }
    }
}

#[test]
fn regenerating_reproduces_the_snapshot() {
    // Guards against a result that depends on allocator addresses or a hash
    // seed rather than on the world seed — the class of bug a single run
    // cannot see.
    let planet = goldberg(FREQUENCY);
    let land = terrain(&planet, 12345);
    let rules = CoverRules::bundled();
    let first = cover(&planet, &land, 12345, &rules);
    for _ in 0..4 {
        assert_eq!(cover(&planet, &land, 12345, &rules), first);
    }
}

#[test]
fn cover_only_ever_lands_on_land() {
    // A village in the sea is not a rendering glitch: cover becomes a
    // `TileKind`, so it would be a town the simulation lets a unit walk to
    // across open water.
    for &frequency in FREQUENCIES {
        let planet = goldberg(frequency);
        for seed in seeds() {
            let land = terrain(&planet, seed);
            let map = cover(&planet, &land, seed, &CoverRules::bundled());
            for tile in (0..planet.tile_count() as u32).map(TileId) {
                if map.get(tile).is_some() {
                    assert!(
                        land.is_land(tile),
                        "n={frequency}, seed {seed:#x}: {tile:?} carries \
                         {:?} on water",
                        map.get(tile)
                    );
                }
            }
        }
    }
}

#[test]
fn every_kind_gets_exactly_the_share_the_data_asks_for() {
    // Exactly, not approximately. The prototype gives up after 900 attempts at
    // finding a free tile, so its shares drift; drawing from the free tiles
    // directly means the budget is met unless the land genuinely runs out,
    // which 44% of it between three kinds leaves no room for.
    let shares = configured_shares();

    for &frequency in FREQUENCIES {
        let planet = goldberg(frequency);
        for seed in seeds() {
            let land = terrain(&planet, seed);
            let map = cover(&planet, &land, seed, &CoverRules::bundled());

            for (kind, percent) in Cover::ALL.iter().zip(shares) {
                assert_eq!(
                    map.count(*kind),
                    share_of(land.land_count(), percent),
                    "{kind:?} at n={frequency}, seed {seed:#x}, on \
                     {} land tiles",
                    land.land_count()
                );
            }
            assert!(map.covered_count() < land.land_count());
        }
    }
}

#[test]
fn the_shares_are_within_a_tile_of_the_configured_fractions() {
    // The same claim stated as the issue states it: the resulting share of the
    // land is the configured one, to within the granularity of a single tile.
    // A tile is covered or it is not, so half a tile is the best any share can
    // do and the miss shrinks as the planet grows.
    for &frequency in FREQUENCIES {
        let planet = goldberg(frequency);
        for seed in seeds() {
            let land = terrain(&planet, seed);
            let map = cover(&planet, &land, seed, &CoverRules::bundled());

            for (kind, percent) in Cover::ALL.iter().zip(configured_shares()) {
                // Both sides scaled by a hundred, and the difference doubled,
                // so "within half a tile" is checked without leaving the
                // integers: |count - land * percent / 100| <= 1/2.
                let wanted = land.land_count() * percent;
                let got = map.count(*kind) * 100;
                assert!(
                    got.abs_diff(wanted) * 2 <= 100,
                    "{kind:?} at n={frequency}, seed {seed:#x}: {} tiles of \
                     {} land is not {percent}%",
                    map.count(*kind),
                    land.land_count()
                );
            }
        }
    }
}

#[test]
fn villages_are_seeded_before_woods_and_fields() {
    // The order is load-bearing, and this is what proves the passes actually
    // run in it rather than merely being declared in it.
    //
    // Each pass draws from one stream and grows around what the passes before
    // it claimed. So turning a later kind's share down to nothing must leave
    // every earlier kind exactly where it was — no draw it made was ever seen
    // by them. If woods went first, adding them would move the villages.
    let planet = goldberg(FREQUENCY);

    for seed in seeds().into_iter().take(20) {
        let land = terrain(&planet, seed);
        let full = cover(&planet, &land, seed, &CoverRules::bundled());

        let villages_only = cover(&planet, &land, seed, &only(&[Cover::Village]));
        assert_eq!(
            tiles_of(&villages_only, Cover::Village),
            tiles_of(&full, Cover::Village),
            "seed {seed:#x}: the woods and fields moved the villages, so they \
             were seeded first"
        );

        let no_fields = cover(
            &planet,
            &land,
            seed,
            &only(&[Cover::Village, Cover::Forest]),
        );
        assert_eq!(
            tiles_of(&no_fields, Cover::Forest),
            tiles_of(&full, Cover::Forest),
            "seed {seed:#x}: the fields moved the woods, so they were seeded \
             first"
        );
        assert_eq!(
            tiles_of(&no_fields, Cover::Village),
            tiles_of(&full, Cover::Village)
        );

        // And the other direction, so the two assertions above cannot be
        // passing because the shares are simply being ignored: fields seeded
        // on their own land somewhere else entirely, because in a full run
        // they are dealt what the first two passes left.
        let fields_only = cover(&planet, &land, seed, &only(&[Cover::Field]));
        assert_ne!(
            tiles_of(&fields_only, Cover::Field),
            tiles_of(&full, Cover::Field),
            "seed {seed:#x}: the fields ignored the villages and woods"
        );
    }
}

/// The bundled rules with every kind but `keep` turned down to nothing.
///
/// A share of zero draws nothing at all, so the kinds left in place see exactly
/// the stream they would have seen in a full run.
fn only(keep: &[Cover]) -> CoverRules {
    let mut rules = CoverRules::bundled();
    for kind in Cover::ALL {
        if !keep.contains(&kind) {
            rules.kinds.get_mut(&kind).expect("bundled").share_percent = 0;
        }
    }
    rules
}

#[test]
fn cover_arrives_in_clumps_rather_than_confetti() {
    // The check no snapshot and no share statistic can make. One lone hex of
    // wheat is not a farm — the renderer merges neighbouring tiles of a kind
    // into one parcelled zone, and that only shows when tiles come in groups.
    //
    // Measured as the neighbours of its own kind the average covered tile has,
    // against the same tile counts scattered uniformly over the same land. The
    // two are indistinguishable by every other assertion in this file, which is
    // the whole reason ADR 0007 wants somebody to look at a planet as well.
    let planet = goldberg(FREQUENCY);
    let (mut grown, mut sprinkled) = (0, 0);
    let trials = 20;

    for seed in seeds().into_iter().take(trials) {
        let land = terrain(&planet, seed);
        let map = cover(&planet, &land, seed, &CoverRules::bundled());

        let clumped = kinship(&planet, map.tiles());
        assert!(
            clumped >= 250,
            "seed {seed:#x}: the average covered tile has only {} neighbours \
             of its own kind; the cover is not clumping",
            clumped as f64 / 100.0
        );
        grown += clumped;
        sprinkled += kinship(&planet, &scattered(&planet, &land, &map, seed));
    }

    // Comfortably more than double, and measured at nearly five times: grown
    // cover averages about three and a half neighbours of its own kind and
    // scattered cover under one, because scattering gives a tile only the
    // covered share of its six neighbours.
    assert!(
        grown > sprinkled * 2,
        "grown cover averages {} neighbours of its own kind and scattered \
         cover {}; the clumping is not doing anything",
        grown as f64 / (trials * 100) as f64,
        sprinkled as f64 / (trials * 100) as f64
    );
}

#[test]
fn different_seeds_give_different_cover() {
    // Two planets of the same size and different seeds must differ, or the
    // cover is not being drawn from the seed at all — which a snapshot test
    // would happily pin.
    let planet = goldberg(FREQUENCY);
    let rules = CoverRules::bundled();
    let mut hashes: Vec<u64> = seeds()
        .into_iter()
        .map(|seed| {
            let land = terrain(&planet, seed);
            cover(&planet, &land, seed, &rules).cover_hash()
        })
        .collect();
    hashes.sort_unstable();
    let before = hashes.len();
    hashes.dedup();
    assert_eq!(hashes.len(), before, "two seeds produced the same cover");
}

#[test]
fn the_shares_follow_the_data() {
    // The acceptance criterion the whole data file exists for: tuning a share
    // is a one-line diff, and nothing in the code has to move with it.
    let planet = goldberg(FREQUENCY);
    let land = terrain(&planet, 21);

    let mut rules = CoverRules::bundled();
    rules
        .kinds
        .get_mut(&Cover::Field)
        .expect("bundled")
        .share_percent = 30;
    rules
        .validate()
        .expect("30% of the land is still under the whole");

    let map = cover(&planet, &land, 21, &rules);
    assert_eq!(map.count(Cover::Field), share_of(land.land_count(), 30));
    // And only that kind moved: villages are seeded before fields, so their
    // draws are untouched.
    let bundled = cover(&planet, &land, 21, &CoverRules::bundled());
    assert_eq!(
        tiles_of(&map, Cover::Village),
        tiles_of(&bundled, Cover::Village)
    );
}

#[test]
#[should_panic(expected = "the terrain describes a different planet")]
fn a_terrain_from_another_planet_is_refused() {
    // Cover reads the terrain to decide what is land, so a mismatched pair
    // would index one planet's tiles with another's ids: silently wrong cover
    // rather than a crash, on a map nothing downstream can check.
    let planet = goldberg(FREQUENCY);
    let elsewhere = terrain(&goldberg(FREQUENCY - 2), 1);
    let _ = cover(&planet, &elsewhere, 1, &CoverRules::bundled());
}

#[test]
fn every_covered_tile_agrees_with_the_kind_the_simulation_is_told() {
    let (planet, _, map) = planet_terrain_cover(FREQUENCY, 99);
    assert_eq!(map.tiles().len(), planet.tile_count());

    let mut counted = [0; 3];
    for tile in (0..planet.tile_count() as u32).map(TileId) {
        match map.get(tile) {
            None => assert_eq!(map.tile_kind(tile), None),
            Some(kind) => {
                assert_eq!(map.tile_kind(tile), Some(kind.tile_kind()));
                counted[Cover::ALL.iter().position(|k| *k == kind).unwrap()] += 1;
            }
        }
    }
    for (kind, &count) in Cover::ALL.iter().zip(counted.iter()) {
        assert_eq!(map.count(*kind), count);
    }
    assert_eq!(map.covered_count(), counted.iter().sum::<usize>());
}
