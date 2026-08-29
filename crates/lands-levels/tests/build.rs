//! Loading a level into a world the simulation can play.

use lands_core::invariants;
use lands_core::prelude::*;
use lands_levels::{Level, LevelError, PlanetSource, tile_count};
use lands_testkit::covers;

// ---- planets ------------------------------------------------------------
//
// `lands-worldgen` builds the real sphere and sits beside `lands-levels` in the
// crate graph, so these tests bring their own. That is what the seam is for: a
// level can be loaded onto any legal topology, with no mesh in sight.

/// A ring of `10·freq²+2` tiles, each adjacent to its two neighbours.
///
/// Nothing like a Goldberg polyhedron geometrically, and identical to one in
/// every way this crate cares about: the right tile count, a closed surface
/// with no edge, and a legal adjacency graph.
#[derive(Debug)]
struct Ring;

impl PlanetSource for Ring {
    fn topology(&self, freq: u32) -> Result<Topology, String> {
        let n = tile_count(freq);
        let neighbors = (0..n)
            .map(|i| vec![TileId((i + n - 1) % n), TileId((i + 1) % n)])
            .collect();
        Topology::new(neighbors).map_err(|e| e.to_string())
    }

    /// Water wherever `tile ^ seed` is a multiple of three: a stand-in for
    /// procedural terrain that is deterministic, moves with the seed, and
    /// leaves enough sea to tell an ocean world apart from a grown one.
    fn terrain(&self, freq: u32, seed: u64) -> Result<Vec<Terrain>, String> {
        assert_ne!(
            seed, 0,
            "seed 0 is the authoring ocean; lands-levels owns it"
        );
        Ok((0..u64::from(tile_count(freq)))
            .map(|i| {
                if (i ^ seed).is_multiple_of(3) {
                    Terrain::Water
                } else {
                    Terrain::Land
                }
            })
            .collect())
    }
}

/// A source that lies about how big the planet is.
#[derive(Debug)]
struct WrongSize;

impl PlanetSource for WrongSize {
    fn topology(&self, _freq: u32) -> Result<Topology, String> {
        Topology::new(vec![vec![TileId(1)], vec![TileId(0)]]).map_err(|e| e.to_string())
    }

    fn terrain(&self, _freq: u32, _seed: u64) -> Result<Vec<Terrain>, String> {
        Ok(vec![Terrain::Land; 2])
    }
}

/// A source that cannot produce the planet at all.
#[derive(Debug)]
struct Broken;

impl PlanetSource for Broken {
    fn topology(&self, freq: u32) -> Result<Topology, String> {
        Err(format!("no mesh cached for frequency {freq}"))
    }

    fn terrain(&self, _freq: u32, _seed: u64) -> Result<Vec<Terrain>, String> {
        unreachable!("topology fails first")
    }
}

// ---- levels -------------------------------------------------------------

/// Red and Blue at opposite ends of a frequency-2 ring: 42 tiles, tile 0 and
/// tile 21 as far apart as the planet allows.
fn duel(seed: u64) -> Level {
    Level::from_ron(&format!(
        r#"Level(
            id: "test/duel",
            freq: 2,
            seed: {seed},
            players: [
                Player(faction: Red,  kind: Human),
                Player(faction: Blue, kind: Ai(profile: "aggressive-2")),
            ],
            overrides: [
                Tile(id: 0,  terrain: Land, kind: Capital, owner: Some(Red)),
                Tile(id: 1,  terrain: Land, kind: Field,   owner: Some(Red)),
                Tile(id: 21, terrain: Land, kind: Capital, owner: Some(Blue)),
                Tile(id: 27, terrain: Land, kind: Forest,  owner: None),
            ],
        )"#
    ))
    .expect("the duel fixture must be a valid level")
}

// ---- the world a level produces -----------------------------------------

#[test]
fn a_level_becomes_a_world_the_rules_accept() {
    let world = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();

    assert_eq!(world.seed, 9);
    assert_eq!(world.tiles.len(), 42);
    assert_eq!(world.players.len(), 2);
    assert!(
        invariants::check(&world).is_empty(),
        "{:?}",
        invariants::check(&world)
    );
}

#[test]
fn a_loaded_world_starts_a_session() {
    let level = duel(9);
    let rules = Ruleset::bundled();
    let world = level.build(&Ring, &rules).unwrap();

    let session = Session::start(world, rules);
    assert_eq!(session.world().current_faction(), Some(Faction::Red));
    assert!(!session.world().is_over());
}

#[test]
fn turn_order_follows_the_player_list() {
    let world = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();
    let order: Vec<Faction> = world.players.iter().map(|p| p.faction).collect();
    assert_eq!(order, vec![Faction::Red, Faction::Blue]);

    assert_eq!(
        world.player(Faction::Blue).map(|p| p.controller.clone()),
        Some(Controller::Ai("aggressive-2".to_owned()))
    );
}

#[test]
fn each_faction_gets_a_territory_around_its_capital() {
    let world = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();

    assert_eq!(world.territories.len(), 2);
    let red = world
        .territory_at(TileId(0))
        .expect("Red's capital is owned");
    assert_eq!(red.faction, Faction::Red);
    assert_eq!(red.capital, TileId(0));
    assert_eq!(red.tiles.len(), 2, "Red was given tiles 0 and 1");
}

#[test]
fn overrides_win_over_the_seed() {
    // Tile 27 is water under this seed; the author made it a forest.
    assert_eq!(
        Ring.terrain(2, 9).unwrap()[27],
        Terrain::Water,
        "fixture assumption: the seed grows sea at tile 27"
    );

    let world = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();
    assert_eq!(world.tile(TileId(27)).terrain, Terrain::Land);
    assert_eq!(world.tile(TileId(27)).kind, TileKind::Forest);
}

#[test]
fn the_seed_grows_the_tiles_no_override_mentions() {
    let world = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();
    let grown = Ring.terrain(2, 9).unwrap();

    for id in world.tile_ids() {
        if matches!(id.0, 0 | 1 | 21 | 27) {
            continue; // the author's four tiles
        }
        assert_eq!(
            world.tile(id).terrain,
            grown[id.index()],
            "tile {id} should be whatever the seed grew"
        );
    }
}

#[test]
fn a_different_seed_is_a_different_planet() {
    let a = duel(9).build(&Ring, &Ruleset::bundled()).unwrap();
    let b = duel(10).build(&Ring, &Ruleset::bundled()).unwrap();
    assert_ne!(world_hash(&a), world_hash(&b));
}

// ---- the authoring seed -------------------------------------------------

#[test]
fn seed_zero_is_an_empty_ocean() {
    let world = duel(0).build(&Ring, &Ruleset::bundled()).unwrap();

    // Only the author's own four tiles are anything at all.
    assert_eq!(world.land_tile_count(), 4);
    for id in world.tile_ids() {
        if matches!(id.0, 0 | 1 | 21 | 27) {
            continue;
        }
        assert_eq!(
            world.tile(id).terrain,
            Terrain::Water,
            "tile {id} should still be sea"
        );
    }
    assert!(invariants::check(&world).is_empty());
}

#[test]
fn an_ocean_world_never_asks_for_terrain() {
    // `Ring::terrain` asserts it is not called with seed 0. Reaching the end of
    // this test is the assertion.
    let world = duel(0).build(&Ring, &Ruleset::bundled()).unwrap();
    assert_eq!(world.seed, 0);
}

// ---- determinism --------------------------------------------------------

#[test]
fn loading_a_level_twice_gives_byte_identical_worlds() {
    let level = duel(194_837);
    let rules = Ruleset::bundled();

    let a = level.build(&Ring, &rules).unwrap();
    let b = level.build(&Ring, &rules).unwrap();

    assert_eq!(
        ron::to_string(&a).unwrap(),
        ron::to_string(&b).unwrap(),
        "two loads of one level must serialise to the same bytes"
    );
    assert_eq!(world_hash(&a), world_hash(&b));
    assert_eq!(a, b);
}

#[test]
fn a_level_round_tripped_through_ron_loads_the_same_world() {
    // The file is the level, so writing it out and reading it back must not
    // move the world it describes by a single byte.
    let level = duel(194_837);
    let rules = Ruleset::bundled();
    let reparsed = Level::from_ron(&level.to_ron().unwrap()).unwrap();

    let a = level.build(&Ring, &rules).unwrap();
    let b = reparsed.build(&Ring, &rules).unwrap();
    assert_eq!(ron::to_string(&a).unwrap(), ron::to_string(&b).unwrap());
}

// ---- economy ------------------------------------------------------------

#[test]
fn starting_treasuries_come_from_the_ruleset() {
    covers!("ECON-020");

    let rules = Ruleset::bundled();
    let world = duel(9).build(&Ring, &rules).unwrap();

    assert!(!world.territories.is_empty());
    for territory in world.territories.values() {
        assert_eq!(territory.wheat, rules.economy.starting_wheat);
        assert_eq!(territory.gold, rules.economy.starting_gold);
    }
}

#[test]
fn starting_treasuries_follow_an_alternate_ruleset() {
    // ECON-020 is a statement about `Ruleset::economy`, not about the numbers
    // that happen to be in the bundled file today.
    let mut rules = Ruleset::bundled();
    rules.economy.starting_wheat += 7;
    rules.economy.starting_gold += 11;

    let world = duel(9).build(&Ring, &rules).unwrap();
    for territory in world.territories.values() {
        assert_eq!(territory.wheat, rules.economy.starting_wheat);
        assert_eq!(territory.gold, rules.economy.starting_gold);
    }
}

// ---- what a bad planet does ---------------------------------------------

#[test]
fn a_level_is_validated_before_the_planet_is_touched() {
    let mut level = duel(9);
    level.players.truncate(1);

    assert_eq!(
        level.build(&Broken, &Ruleset::bundled()),
        Err(LevelError::PlayerCount {
            min: 2,
            max: 4,
            found: 1,
        })
    );
}

#[test]
fn reports_a_planet_source_that_fails() {
    let err = duel(9).build(&Broken, &Ruleset::bundled()).unwrap_err();
    assert_eq!(
        err,
        LevelError::Planet {
            freq: 2,
            message: "no mesh cached for frequency 2".to_owned(),
        }
    );
    assert!(err.to_string().contains("freq 2"), "got: {err}");
}

#[test]
fn reports_a_planet_of_the_wrong_size() {
    // Tile ids in a level index a planet of exactly `10n^2+2` tiles. A source
    // that returns anything else would silently renumber every override.
    let err = duel(9).build(&WrongSize, &Ruleset::bundled()).unwrap_err();
    assert_eq!(
        err,
        LevelError::PlanetSize {
            what: "tiles",
            freq: 2,
            expected: 42,
            found: 2,
        }
    );
}
