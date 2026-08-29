//! `lands-core`'s closed-surface tests, run on the planet the game ships.
//!
//! `crates/lands-core/tests/closed_surface.rs` proves the rules need no
//! wrap-around special cases, using `topo::icosahedron()` — the genuine `n = 1`
//! planet, but only twelve tiles — and `topo::torus`, which is closed but is
//! not a sphere. Both are stand-ins, and a stand-in cannot show that the *real*
//! tiling has no seam. `lands-core` cannot run these against the real thing,
//! because depending on this crate would point the graph back at itself
//! (`cargo xtask check-deps`), so the same scenarios live here instead, on a
//! 642-tile Goldberg sphere straight out of [`goldberg`].
//!
//! Nothing in `lands-core` changed to make them pass. That is the whole point:
//! a sphere is just a graph with no boundary, so movement BFS, connected
//! components and capital relocation are correct on one for the same reason
//! they are correct on a line — they never knew the difference. See
//! `docs/adr/0004-graph-distance-not-geometry.md`.
//!
//! Tile ids here are the real planet's, not the testkit icosahedron's, and the
//! two number their tiles differently. So no id is written down: every fixture
//! below is derived from the adjacency graph, which also means these tests keep
//! describing the same *situation* if the numbering ever moves.

use lands_core::apply::legal_commands;
use lands_core::invariants;
use lands_core::movement::reachable;
use lands_core::rng::Rng;
use lands_testkit::covers;
use lands_testkit::prelude::*;
use lands_worldgen::goldberg;
use std::collections::BTreeSet;

/// Frequency 8 — 642 tiles, the size `docs/architecture.md` uses for a level.
const FREQUENCY: u32 = 8;

/// The tile every ring below is measured from. Any tile would do; a sphere has
/// no privileged one, which is rather the point.
const POLE: TileId = TileId(0);

/// How far out the equator sits: the ring of 40 tiles furthest from `POLE`.
const EQUATOR: u32 = 12;

fn planet() -> Topology {
    goldberg(FREQUENCY).topology()
}

/// The tiles exactly `hops` from `from`, in the order they run around the
/// planet.
///
/// This is a torus's "wrapping row" without the modulo arithmetic. On a closed
/// surface a breadth-first ring is a genuine cycle — every tile in it has
/// exactly two neighbours inside it, and the last one is adjacent to the first
/// — and that falls out of the adjacency graph rather than being constructed.
/// Both facts are asserted here, so a fixture built on this cannot quietly
/// stop being a ring.
fn ring(planet: &Topology, from: TileId, hops: u32) -> Vec<u32> {
    let dist = planet.distances_from(from, |_| true);
    let members: BTreeSet<TileId> = planet
        .tiles()
        .filter(|t| dist[t.index()] == Some(hops))
        .collect();
    assert!(
        members.len() > 3,
        "{hops} hops out is too small to be a ring"
    );

    let start = *members.iter().next().expect("just checked it is non-empty");
    let mut cycle = vec![start];
    let mut walked: BTreeSet<TileId> = [start].into_iter().collect();

    while cycle.len() < members.len() {
        let here = *cycle.last().expect("the cycle always has a last tile");
        assert_eq!(
            planet
                .neighbors(here)
                .iter()
                .filter(|n| members.contains(n))
                .count(),
            2,
            "{here} has more than two neighbours in the ring, so it is a band"
        );
        let next = *planet
            .neighbors(here)
            .iter()
            .find(|n| members.contains(n) && !walked.contains(n))
            .expect("a ring on a closed surface never dead-ends");
        walked.insert(next);
        cycle.push(next);
    }

    assert!(
        planet
            .neighbors(*cycle.last().expect("non-empty"))
            .contains(&start),
        "the ring did not close back on itself"
    );
    cycle.into_iter().map(|t| t.0).collect()
}

/// A tile next to `on_ring` but one step nearer `from` — somewhere for an
/// attacker to stand while it punches through the ring.
fn just_inside(planet: &Topology, from: TileId, hops: u32, on_ring: u32) -> u32 {
    let dist = planet.distances_from(from, |_| true);
    planet
        .neighbors(TileId(on_ring))
        .iter()
        .find(|n| dist[n.index()] == Some(hops - 1))
        .expect("every ring tile has a neighbour one step nearer")
        .0
}

/// The planet has no edge anywhere, and exactly twelve places where the tiling
/// bends.
///
/// Not a game rule — the evidence that everything below is running on the shape
/// it claims to be.
#[test]
fn the_planet_the_game_ships_is_a_closed_surface() {
    let planet = planet();
    assert_eq!(planet.tile_count(), 642, "10n^2+2 at n=8");

    let degrees: BTreeSet<usize> = planet.tiles().map(|t| planet.neighbors(t).len()).collect();
    assert_eq!(degrees, [5, 6].into_iter().collect());
    assert_eq!(
        planet
            .tiles()
            .filter(|&t| planet.neighbors(t).len() == 5)
            .count(),
        12,
        "twelve pentagons, and nowhere the world stops"
    );

    // One component: a seam would show up here as an unreachable tile.
    let dist = planet.distances_from(POLE, |_| true);
    assert!(dist.iter().all(Option::is_some));

    // And the distance profile is symmetric about the equator, which a sphere
    // with a tear in it would not be.
    let count = |d: u32| dist.iter().filter(|x| **x == Some(d)).count();
    assert_eq!(count(1), 5, "the pole is a pentagon");
    for d in 1..EQUATOR {
        assert_eq!(count(d), count(24 - d), "ring {d} against ring {}", 24 - d);
    }
}

/// A territory that wraps the whole way around is still **one** territory.
#[test]
fn a_territory_can_encircle_the_planet() {
    covers!("TERR-010");

    let planet = planet();
    let equator = ring(&planet, POLE, EQUATOR);
    assert_eq!(equator.len(), 40);

    let (world, _) = WorldBuilder::new(planet)
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &equator)
        .capital(equator[0])
        .build();

    assert_sound(&world);
    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(
        territories.len(),
        1,
        "the ring closes on itself, so it is a single territory"
    );
    assert_eq!(territories[0].size(), 40);
}

/// Cutting a ring **once** does not split it — the other way round is still
/// open.
///
/// The identical capture on a line splits the territory in two
/// (`lands-core/tests/territory.rs`). Nothing special-cases the difference:
/// connected components over the adjacency graph simply give the right answer
/// on a real planet as readily as on a torus.
#[test]
fn cutting_an_encircling_territory_once_does_not_split_it() {
    covers!("TERR-030");

    let planet = planet();
    let equator = ring(&planet, POLE, EQUATOR);
    // Cut far from the capital, so the capital cannot defend the tile and no
    // relocation is expected either.
    let (cut, seat) = (equator[0], equator[20]);
    let attacker = just_inside(&planet, POLE, EQUATOR, cut);

    let mut session = WorldBuilder::new(planet)
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &equator)
        .capital(seat)
        .own(Faction::Blue, &[attacker])
        .capital(attacker)
        .unit(UnitKind::Pawn, attacker)
        .session();

    let pawn = session.world().tile(TileId(attacker)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(cut),
        })
        .expect("a pawn may take empty enemy ground");

    let world = session.world();
    assert_sound(world);

    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(
        territories.len(),
        1,
        "the remaining 39 tiles still reach each other the long way round"
    );
    assert_eq!(territories[0].size(), 39);
    assert_eq!(territories[0].capital, TileId(seat), "no relocation needed");
}

/// A second cut on the far side finally does split it.
#[test]
fn cutting_an_encircling_territory_twice_splits_it() {
    covers!("TERR-030");

    let planet = planet();
    let equator = ring(&planet, POLE, EQUATOR);
    // Opposite sides of a forty-tile ring, with the capital in one of the two
    // arcs the cuts will leave behind.
    let (first_cut, second_cut, seat) = (equator[0], equator[20], equator[10]);
    let west = just_inside(&planet, POLE, EQUATOR, first_cut);
    let east = just_inside(&planet, POLE, EQUATOR, second_cut);

    let mut session = WorldBuilder::new(planet)
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &equator)
        .capital(seat)
        .treasury(seat, 20, 20)
        .own(Faction::Blue, &[west])
        .capital(west)
        .unit(UnitKind::Pawn, west)
        .own(Faction::Blue, &[east])
        .capital(east)
        .unit(UnitKind::Pawn, east)
        .session();

    for cut in [first_cut, second_cut] {
        let pawn = session
            .world()
            .tile(TileId(just_inside(
                session.world().topology.as_ref(),
                POLE,
                EQUATOR,
                cut,
            )))
            .unit
            .unwrap();
        session
            .execute(Command::MoveUnit {
                unit: pawn,
                to: TileId(cut),
            })
            .expect("a pawn may take empty enemy ground");
    }

    let world = session.world();
    assert_sound(world);

    let mut sizes: Vec<u32> = world
        .territories_of(Faction::Red)
        .map(|t| t.size())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![19, 19], "the ring became two arcs of nineteen");

    // The treasury still divides in proportion and conserves exactly.
    let wheat: i32 = world.territories_of(Faction::Red).map(|t| t.wheat).sum();
    let gold: i32 = world.territories_of(Faction::Red).map(|t| t.gold).sum();
    assert_eq!(wheat, 20);
    assert_eq!(gold, 20);
}

/// Movement goes either way round rather than walking to a map edge.
#[test]
fn movement_goes_either_way_round_the_planet() {
    covers!("UNIT-030");

    let planet = planet();
    let equator = ring(&planet, POLE, EQUATOR);
    let start = equator[0];

    let session = WorldBuilder::new(planet)
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &equator)
        .capital(equator[20])
        .unit(UnitKind::Pawn, start)
        .session();

    let pawn = session.world().tile(TileId(start)).unit.unwrap();
    let can_reach = reachable(session.world(), session.rules(), pawn);
    let steps = session.rules().units.own_territory_steps as usize;

    // Four steps forward around the ring, and four steps *backward* — which on
    // a bounded map would be walking off the edge.
    for k in 1..=steps {
        assert!(
            can_reach.contains(&TileId(equator[k])),
            "{k} steps forward should be reachable"
        );
        assert!(
            can_reach.contains(&TileId(equator[equator.len() - k])),
            "{k} steps backward should be reachable"
        );
    }

    // And the budget is a budget in both directions, not a map edge.
    assert!(!can_reach.contains(&TileId(equator[steps + 1])));
    assert!(!can_reach.contains(&TileId(equator[equator.len() - steps - 1])));
}

/// A capital rehouses correctly when "the centre" has no edges to anchor it.
///
/// On a ring every tile is equally central — the degenerate case a Euclidean
/// centroid would get wrong, since the average of points on a sphere lies
/// *inside* it rather than on the surface. The graph median always has an
/// answer, and the tie is broken deterministically.
#[test]
fn a_capital_rehouses_on_a_surface_with_no_centre() {
    covers!("TERR-011");

    let planet = planet();
    let equator = ring(&planet, POLE, EQUATOR);
    let seat = equator[0];
    let attacker = just_inside(&planet, POLE, EQUATOR, seat);

    let mut session = WorldBuilder::new(planet)
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &equator)
        .capital(seat)
        .own(Faction::Blue, &[attacker])
        .capital(attacker)
        .treasury(attacker, 20, 0)
        .unit(UnitKind::Warrior, attacker)
        .session();

    let warrior = session.world().tile(TileId(attacker)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(seat),
        })
        .expect("a warrior may take a capital");

    let world = session.world();
    assert_sound(world);

    let territories: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(
        territories.len(),
        1,
        "an arc of thirty-nine, still connected"
    );
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

/// The invariants hold on the planet the game actually ships, played at random
/// until it is over.
///
/// `lands-core` runs this on twelve tiles and on a torus. Neither has an
/// icosahedral defect in the middle of open ground, which is where a hop count
/// stops being locally uniform and where a bug that assumed six neighbours
/// would show up.
#[test]
fn the_invariants_hold_on_the_planet_the_game_ships() {
    covers!("INV-001", "INV-003");

    let sphere = goldberg(FREQUENCY);
    let planet = sphere.topology();

    // The four factions start on four of the twelve pentagons — the corners of
    // the icosahedron underneath, so they are as far from each other as the
    // planet allows.
    let pentagons: Vec<u32> = sphere.pentagons().map(|t| t.0).collect();
    let starts = [pentagons[0], pentagons[3], pentagons[6], pentagons[9]];
    for (i, &a) in starts.iter().enumerate() {
        for &b in &starts[i + 1..] {
            let apart = planet.distance(TileId(a), TileId(b), |_| true).unwrap();
            assert!(apart >= 8, "{a} and {b} start only {apart} hops apart");
        }
    }

    let mut commands = 0;
    let mut claimed = 0;

    for seed in 0..30u64 {
        let mut session = WorldBuilder::new(planet.clone())
            .seed(seed)
            .all_land()
            .player(Faction::Red)
            .player(Faction::Yellow)
            .player(Faction::Green)
            .player(Faction::Blue)
            .own(Faction::Red, &[starts[0]])
            .capital(starts[0])
            .own(Faction::Yellow, &[starts[1]])
            .capital(starts[1])
            .own(Faction::Green, &[starts[2]])
            .capital(starts[2])
            .own(Faction::Blue, &[starts[3]])
            .capital(starts[3])
            .session();

        let mut rng = Rng::seed_from_u64(seed ^ 0x601D_BE46);
        for step in 0..800 {
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
            commands += 1;

            let violations = invariants::check(session.world());
            assert!(
                violations.is_empty(),
                "seed {seed} step {step} ({cmd:?}) broke a 642-tile planet:\n{}",
                violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        claimed = session
            .world()
            .tiles
            .iter()
            .filter(|t| t.owner.is_some())
            .count();
    }

    // A fuzzer that stalls on turn one passes every assertion above without
    // testing anything, so say out loud how much of the planet it covered.
    // Borders have to actually meet for territories to split and merge.
    assert!(commands > 20_000, "only {commands} commands were played");
    assert!(
        claimed > 200,
        "the last run claimed only {claimed} of 642 tiles, so the factions \
         never met and nothing interesting was exercised"
    );
}
