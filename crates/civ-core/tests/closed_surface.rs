//! The planet is a sphere, so the playfield has **no edges**.
//!
//! `civ-core` handles this by knowing nothing about geometry at all: it sees
//! only an adjacency graph (see `docs/adr/0004-graph-distance-not-geometry.md`).
//! A sphere is simply a graph with no boundary, so every algorithm — movement
//! BFS, territory connected-components, capital relocation — is correct on one
//! without a single wrap-around special case. There is no row, no column, and
//! no modulo arithmetic anywhere in the crate.
//!
//! This file is the evidence for that claim. The other test files use `line`,
//! `grid` and `hex_grid`, which all have boundaries; these use `icosahedron`
//! (the genuine `n = 1` planet, twelve pentagons) and `torus` (a closed surface
//! big enough for territories to encircle).

use civ_core::apply::legal_commands;
use civ_core::invariants;
use civ_core::movement::reachable;
use civ_core::rng::Rng;
use civ_testkit::covers;
use civ_testkit::prelude::*;

/// A territory that wraps the whole way around is still **one** territory.
///
/// On a bounded map, tiles 0 and 5 of a six-wide row are five steps apart. On a
/// closed one they are adjacent, and the code must see that without being told.
#[test]
fn a_territory_can_encircle_the_planet() {
    covers!("TERR-010");

    // Red owns an entire wrapping row of a 6x4 torus.
    let (world, _) = WorldBuilder::new(topo::torus(6, 4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4, 5])
        .capital(0)
        .build();

    assert_sound(&world);
    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(
        territories.len(),
        1,
        "the ring closes on itself, so it is a single territory"
    );
    assert_eq!(territories[0].size(), 6);
}

/// Cutting a ring **once** does not split it — because the other way round is
/// still open.
///
/// This is the sharpest demonstration that the topology is real. The identical
/// capture on a straight line splits the territory in two
/// (`territory.rs::splitting_divides_the_treasury_in_proportion`); on a closed
/// surface it must not. Nothing in `civ-core` special-cases this: connected
/// components over the adjacency graph simply give the right answer.
#[test]
fn cutting_an_encircling_territory_once_does_not_split_it() {
    covers!("TERR-030");

    let mut session = WorldBuilder::new(topo::torus(6, 4))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4, 5])
        .capital(0)
        // Blue sits one row below tile 2, ready to punch through the ring.
        .own(Faction::Blue, &[8])
        .capital(8)
        .unit(UnitKind::Pawn, 8)
        .session();

    let pawn = session.world().tile(TileId(8)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(2),
        })
        .expect("a pawn may take empty enemy ground");

    let world = session.world();
    assert_sound(world);

    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(
        territories.len(),
        1,
        "tiles 3-4-5 still reach 0-1 by wrapping past tile 5, so the ring holds"
    );
    assert_eq!(territories[0].size(), 5);
    assert_eq!(
        territories[0].capital,
        TileId(0),
        "no relocation was needed"
    );
}

/// A second cut on the opposite side finally does split it.
#[test]
fn cutting_an_encircling_territory_twice_splits_it() {
    covers!("TERR-030");

    let mut session = WorldBuilder::new(topo::torus(6, 4))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4, 5])
        .capital(0)
        .treasury(0, 20, 20)
        .own(Faction::Blue, &[8, 11])
        .capital(8)
        .unit(UnitKind::Pawn, 8)
        .unit(UnitKind::Pawn, 11)
        .session();

    // Cut at tile 2 and at tile 5, on opposite sides of the ring.
    let first = session.world().tile(TileId(8)).unit.unwrap();
    let second = session.world().tile(TileId(11)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: first,
            to: TileId(2),
        })
        .unwrap();
    session
        .execute(Command::MoveUnit {
            unit: second,
            to: TileId(5),
        })
        .unwrap();

    let world = session.world();
    assert_sound(world);

    let mut sizes: Vec<u32> = world
        .territories_of(Faction::Red)
        .map(|t| t.size())
        .collect();
    sizes.sort_unstable();
    assert_eq!(
        sizes,
        vec![2, 2],
        "the ring became two arcs: {{0,1}} and {{3,4}}"
    );

    // The treasury still divides in proportion and conserves exactly.
    let wheat: i32 = world.territories_of(Faction::Red).map(|t| t.wheat).sum();
    let gold: i32 = world.territories_of(Faction::Red).map(|t| t.gold).sum();
    assert_eq!(wheat, 20);
    assert_eq!(gold, 20);
}

/// Movement takes the short way round rather than walking to a map edge.
#[test]
fn movement_wraps_around_the_planet() {
    covers!("UNIT-030");

    let session = WorldBuilder::new(topo::torus(6, 4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4, 5])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 0)
        .session();

    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    let can_reach = reachable(session.world(), session.rules(), pawn);

    // Tile 5 is one step *backwards* around the ring, not five forwards.
    assert!(
        can_reach.contains(&TileId(5)),
        "the pawn should see tile 5 as adjacent"
    );
    // Every other tile of the six-tile ring is within the four-step budget in
    // one direction or the other.
    for t in [1u32, 2, 3, 4, 5] {
        assert!(
            can_reach.contains(&TileId(t)),
            "tile {t} should be reachable"
        );
    }
}

/// A capital rehouses correctly when "the centre" has no edges to anchor it.
///
/// On a ring every tile is equally central, which is exactly the degenerate
/// case a Euclidean centroid would get wrong — the average of points on a
/// sphere lies *inside* it, not on the surface. Graph distance has no such
/// problem, and the tie is broken deterministically.
#[test]
fn a_capital_rehouses_on_a_surface_with_no_centre() {
    covers!("TERR-011");

    let mut session = WorldBuilder::new(topo::torus(6, 4))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4, 5])
        .capital(3)
        .own(Faction::Blue, &[9])
        .capital(9)
        .treasury(9, 20, 0)
        .unit(UnitKind::Warrior, 9)
        .session();

    let warrior = session.world().tile(TileId(9)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(3),
        })
        .expect("a warrior may take a capital");

    let world = session.world();
    assert_sound(world);

    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(territories.len(), 1, "an arc of five, still connected");
    assert!(
        territories[0].tiles.contains(&territories[0].capital),
        "the rehoused capital is inside its own territory"
    );
    assert_eq!(
        world.tile(territories[0].capital).kind,
        TileKind::Capital,
        "and the tile is marked as one"
    );
}

/// The invariants hold on the genuine minimal planet, where every tile is a
/// pentagon and nothing has a boundary to lean on.
#[test]
fn the_invariants_hold_on_a_real_planet() {
    covers!("INV-001");

    for seed in 0..60u64 {
        let mut session = WorldBuilder::new(topo::icosahedron())
            .seed(seed)
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .own(Faction::Blue, &[3])
            .capital(3)
            .session();

        let mut rng = Rng::seed_from_u64(seed);
        for step in 0..200 {
            if session.world().is_over() {
                break;
            }
            let options = legal_commands(session.world(), session.rules());
            let Some(cmd) = rng.pick(&options).cloned() else {
                break;
            };
            session
                .execute(cmd.clone())
                .unwrap_or_else(|e| panic!("seed {seed} step {step}: {cmd:?} was refused: {e}"));

            let violations = invariants::check(session.world());
            assert!(
                violations.is_empty(),
                "seed {seed} step {step} ({cmd:?}) broke a 12-tile planet:\n{}",
                violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}

/// The same, on a closed surface large enough for territories to wrap, split
/// and merge repeatedly.
#[test]
fn the_invariants_hold_on_a_wrapping_world() {
    covers!("INV-001", "INV-003");

    for seed in 0..40u64 {
        let mut session = WorldBuilder::new(topo::torus(7, 6))
            .seed(seed)
            .all_land()
            .player(Faction::Red)
            .player(Faction::Yellow)
            .player(Faction::Green)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .own(Faction::Yellow, &[3])
            .capital(3)
            .own(Faction::Green, &[24])
            .capital(24)
            .own(Faction::Blue, &[38])
            .capital(38)
            .session();

        let mut rng = Rng::seed_from_u64(seed ^ 0xABCD);
        for step in 0..300 {
            if session.world().is_over() {
                break;
            }
            let options = legal_commands(session.world(), session.rules());
            let Some(cmd) = rng.pick(&options).cloned() else {
                break;
            };
            session
                .execute(cmd.clone())
                .unwrap_or_else(|e| panic!("seed {seed} step {step}: {cmd:?}: {e}"));

            let violations = invariants::check(session.world());
            assert!(
                violations.is_empty(),
                "seed {seed} step {step} ({cmd:?}) broke a wrapping world:\n{}",
                violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}
