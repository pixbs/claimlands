//! Forest spread (GROW-001, GROW-002).
//!
//! The default 10% chance would make these assertions look flaky even though
//! the simulation is deterministic, so they raise it to 100% and check the
//! *shape* of the rule. The randomness itself is exercised by
//! [`spread_is_seeded_and_reproducible`].
//!
//! Every fixture has two players. Growth runs when the turn order wraps, and a
//! lone player wins by elimination before that can ever happen.

use civ_testkit::covers;
use civ_testkit::prelude::*;

/// The bundled rules with forests guaranteed to spread, so a single round is
/// enough to observe the behaviour.
fn always_spreading() -> Ruleset {
    let mut rules = Ruleset::bundled();
    rules.growth.forest_spread_percent = 100;
    rules
}

/// Advance a full round, so that growth runs.
fn end_round(session: &mut Session) {
    session.execute(Command::EndTurn).unwrap();
    session.execute(Command::EndTurn).unwrap();
}

#[test]
fn a_forest_seeds_a_neighbouring_empty_tile_each_round() {
    covers!("GROW-001");

    // Red and Blue sit at opposite ends; the forest at 3 is on neutral ground
    // with empty tiles either side, so ownership is not a confounder.
    let mut session = WorldBuilder::new(topo::line(7))
        .rules(always_spreading())
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0])
        .capital(0)
        .own(Faction::Blue, &[6])
        .capital(6)
        .kind(3, TileKind::Forest)
        .session();

    assert_eq!(session.world().tile(TileId(2)).kind, TileKind::Empty);
    assert_eq!(session.world().tile(TileId(4)).kind, TileKind::Empty);

    end_round(&mut session);

    let world = session.world();
    let grown = [2u32, 4]
        .iter()
        .filter(|&&t| world.tile(TileId(t)).kind == TileKind::Forest)
        .count();
    assert_eq!(grown, 1, "exactly one neighbour should have been seeded");
    assert_eq!(
        world.tile(TileId(3)).kind,
        TileKind::Forest,
        "the parent forest stays put"
    );
}

#[test]
fn forests_never_take_developed_ground() {
    covers!("GROW-002");

    // The forest at 3 is hemmed in by Red's capital and a town.
    let mut session = WorldBuilder::new(topo::line(7))
        .rules(always_spreading())
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[2, 3, 4])
        .capital(2)
        .kind(3, TileKind::Forest)
        .kind(4, TileKind::Town)
        .own(Faction::Blue, &[6])
        .capital(6)
        .session();

    end_round(&mut session);

    let world = session.world();
    assert_eq!(world.tile(TileId(2)).kind, TileKind::Capital);
    assert_eq!(world.tile(TileId(4)).kind, TileKind::Town);
}

#[test]
fn forests_do_take_owned_empty_ground() {
    covers!("GROW-002");

    // Deliberate: neglected land degrades. An owned empty tile is fair game,
    // which is the main pressure to develop rather than merely hold.
    // See GROW-002.
    let mut session = WorldBuilder::new(topo::line(7))
        .rules(always_spreading())
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .kind(3, TileKind::Forest)
        .own(Faction::Blue, &[6])
        .capital(6)
        .session();

    end_round(&mut session);

    let world = session.world();
    let grown = [2u32, 4]
        .iter()
        .filter(|&&t| world.tile(TileId(t)).kind == TileKind::Forest)
        .count();
    assert_eq!(grown, 1);
    // Tile 2 belongs to Red; if it is the one that grew, Red still owns it —
    // growth changes what stands on a tile, never who holds it.
    if world.tile(TileId(2)).kind == TileKind::Forest {
        assert_eq!(world.tile(TileId(2)).owner, Some(Faction::Red));
    }
}

/// Growth is random but seeded, so the same match always grows the same trees.
#[test]
fn spread_is_seeded_and_reproducible() {
    covers!("GROW-001");

    let run = |seed: u64| {
        let mut session = WorldBuilder::new(topo::hex_grid(5, 5))
            .seed(seed)
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .own(Faction::Blue, &[24])
            .capital(24)
            .kinds(&[7, 12, 17], TileKind::Forest)
            .session();

        for _ in 0..20 {
            end_round(&mut session);
        }
        session.state_hash()
    };

    assert_eq!(run(4242), run(4242), "the same seed grows the same forest");
    assert_ne!(
        run(4242),
        run(9999),
        "different seeds should diverge over twenty rounds"
    );
}
