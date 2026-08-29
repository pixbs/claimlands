//! Territory rules (TERR-011 … TERR-040).
//!
//! The centrepiece is [`splitting_divides_the_treasury_in_proportion`], the
//! worked example from the design brief. These tests are the reason the
//! territory module can be refactored with any confidence at all.

use civ_testkit::covers;
use civ_testkit::prelude::*;

/// Territory containing `tile`, panicking with a useful message if there is none.
fn terr(world: &World, tile: u32) -> &Territory {
    world
        .territory_at(TileId(tile))
        .unwrap_or_else(|| panic!("tile {tile} should belong to a territory"))
}

/// A 16-wide, 2-tall grid. Red holds the whole top row; Blue sits underneath at
/// `blue_col` ready to punch through.
///
/// Blue plays first so that Red's treasury is exactly what the fixture sets —
/// Red's income never runs before the capture.
fn strip_world(blue_col: u32, wheat: i32, gold: i32) -> Session {
    let width = 16;
    let blue_tile = width + blue_col;

    WorldBuilder::new(topo::grid(width, 2))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &(0..width).collect::<Vec<_>>())
        .capital(0)
        .treasury(0, wheat, gold)
        .own(Faction::Blue, &[blue_tile])
        .capital(blue_tile)
        .unit(UnitKind::Pawn, blue_tile)
        .session()
}

/// The brief's worked example, verbatim:
///
/// > the player has 16 tiles, 15 gold, and 15 wheat. One tile gets occupied by
/// > the enemy player, in that case, there is 2 territories: one with the
/// > remaining capital and 10 tiles, the other with the newly generated capital
/// > and 5 tiles. The new capital will receive 5 gold and 5 wheat; the old one
/// > will receive -5 gold and -5 wheat, resulting in a total of 10 gold and
/// > 10 wheat.
#[test]
fn splitting_divides_the_treasury_in_proportion() {
    covers!("TERR-030");

    let mut session = strip_world(10, 15, 15);
    let pawn = session.world().tile(TileId(26)).unit.unwrap();

    // Blue punches through the middle of Red's strip at column 10.
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(10),
        })
        .expect("a pawn may take an empty enemy tile");

    let world = session.world();
    assert_sound(world);

    let red: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(red.len(), 2, "the strip was cut in two");

    // Tiles 0..9 keep the original capital; tiles 11..15 get a fresh one.
    let kept = terr(world, 0);
    let fresh = terr(world, 15);

    assert_eq!(kept.size(), 10);
    assert_eq!(fresh.size(), 5);
    assert_eq!(
        kept.capital,
        TileId(0),
        "the old capital survives untouched"
    );

    assert_eq!((kept.wheat, kept.gold), (10, 10));
    assert_eq!((fresh.wheat, fresh.gold), (5, 5));

    // The brief's arithmetic only works if nothing is created or destroyed.
    assert_eq!(kept.wheat + fresh.wheat, 15);
    assert_eq!(kept.gold + fresh.gold, 15);
}

#[test]
fn a_split_never_loses_resources_to_rounding() {
    covers!("TERR-030");

    // 15 remaining tiles splitting 10/5 with a treasury of 7 divides as 4 and 2,
    // leaving 1 that must go somewhere rather than evaporating.
    let mut session = strip_world(10, 7, 7);
    let pawn = session.world().tile(TileId(26)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(10),
        })
        .unwrap();

    let world = session.world();
    let total_wheat: i32 = world.territories_of(Faction::Red).map(|t| t.wheat).sum();
    let total_gold: i32 = world.territories_of(Faction::Red).map(|t| t.gold).sum();
    assert_eq!(total_wheat, 7);
    assert_eq!(total_gold, 7);

    // The remainder goes to the piece that kept the capital.
    assert_eq!(terr(world, 0).wheat, 5);
    assert_eq!(terr(world, 15).wheat, 2);
}

#[test]
fn the_new_capital_appears_near_the_middle_of_the_orphaned_piece() {
    covers!("TERR-011");

    let mut session = strip_world(10, 15, 15);
    let pawn = session.world().tile(TileId(26)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(10),
        })
        .unwrap();

    // The orphan is tiles 11..15; its graph centre is 13.
    assert_eq!(terr(session.world(), 15).capital, TileId(13));
    assert_eq!(session.world().tile(TileId(13)).kind, TileKind::Capital);
}

#[test]
fn capturing_a_capital_transfers_a_quarter_of_its_gold() {
    covers!("TERR-020");

    // Red holds two tiles with its capital at 1, adjacent to Blue's warrior.
    let mut session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &[1, 2])
        .capital(1)
        .treasury(1, 0, 40)
        .own(Faction::Blue, &[0])
        .capital(0)
        // Enough wheat that Blue's warrior survives to act: a one-tile
        // territory yields 1 wheat a turn and a warrior eats 2.
        .treasury(0, 10, 0)
        .unit(UnitKind::Warrior, 0)
        .session();

    let warrior = session.world().tile(TileId(0)).unit.unwrap();
    let blue_gold_before = terr(session.world(), 0).gold;

    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(1),
        })
        .expect("a warrior may take a capital");

    let world = session.world();
    assert_sound(world);

    // 25% of 40 is 10.
    assert_eq!(terr(world, 0).gold, blue_gold_before + 10);
    assert_eq!(world.stats_of(Faction::Blue).gold_looted, 10);
    // Red keeps the rest and rehouses its capital on its one remaining tile.
    assert_eq!(terr(world, 2).gold, 30);
    assert_eq!(terr(world, 2).capital, TileId(2));
}

#[test]
fn a_razed_capital_becomes_empty_ground() {
    covers!("TERR-020", "UNIT-010b");

    let mut session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        .own(Faction::Red, &[1, 2])
        .capital(1)
        .own(Faction::Blue, &[0])
        .capital(0)
        .treasury(0, 10, 0)
        .unit(UnitKind::Warrior, 0)
        .session();

    let warrior = session.world().tile(TileId(0)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(1),
        })
        .unwrap();

    assert_eq!(session.world().tile(TileId(1)).kind, TileKind::Empty);
    assert_eq!(session.world().tile(TileId(1)).owner, Some(Faction::Blue));
}

/// TERR-013: a territory reduced to nothing but forest cannot rehouse its
/// capital, so it stops belonging to anyone.
#[test]
fn a_forest_only_remnant_is_disbanded() {
    covers!("TERR-013");

    let mut session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Blue)
        .player(Faction::Red)
        // Red holds its capital at 1 and nothing but forest beyond it.
        .own(Faction::Red, &[1, 2, 3])
        .capital(1)
        .kinds(&[2, 3], TileKind::Forest)
        .own(Faction::Blue, &[0])
        .capital(0)
        .treasury(0, 10, 0)
        .unit(UnitKind::Warrior, 0)
        .session();

    let warrior = session.world().tile(TileId(0)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(1),
        })
        .unwrap();

    let world = session.world();
    assert_sound(world);

    assert_eq!(
        world.territories_of(Faction::Red).count(),
        0,
        "with only forest left, the zone becomes no one's land"
    );
    assert_eq!(world.tile(TileId(2)).owner, None);
    assert_eq!(world.tile(TileId(3)).owner, None);
    assert_eq!(
        world.tile(TileId(3)).kind,
        TileKind::Forest,
        "disbanding removes the owner, not the trees"
    );
}

/// TERR-040: reconnecting two of your own territories sums their treasuries and
/// keeps the capital nearest the tile that joined them.
#[test]
fn merging_sums_treasuries_and_keeps_the_nearer_capital() {
    covers!("TERR-040");

    // Red holds 0-1 and 3-4 on a line; tile 2 is neutral and separates them.
    // The capitals sit at different distances from tile 2 — one hop from 3,
    // two from 0 — so the rule has something to choose between.
    let mut session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .treasury(0, 3, 4)
        .own(Faction::Red, &[3, 4])
        .capital(3)
        .treasury(3, 5, 6)
        .unit(UnitKind::Pawn, 4)
        .session();

    assert_eq!(session.world().territories_of(Faction::Red).count(), 2);

    let pawn = session.world().tile(TileId(4)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(2),
        })
        .expect("a pawn may take neutral empty ground");

    let world = session.world();
    assert_sound(world);

    let merged: Vec<&Territory> = world.territories_of(Faction::Red).collect();
    assert_eq!(merged.len(), 1, "the two halves became one");
    let merged = merged[0];
    assert_eq!(merged.size(), 5);

    // Treasuries add, plus the income each half collected before they joined.
    // Each yields 2 wheat (capital + one empty tile) and 1 gold, and the pawn
    // standing in the eastern half eats 1 wheat.
    assert_eq!(merged.wheat, (3 + 2) + (5 + 2 - 1));
    assert_eq!(merged.gold, (4 + 1) + (6 + 1));

    // Tile 2 is one hop from capital 3 and two hops from capital 0.
    assert_eq!(merged.capital, TileId(3), "the nearer capital survives");
    assert_eq!(
        world.tile(TileId(0)).kind,
        TileKind::Empty,
        "the losing capital is razed to empty ground"
    );
}

/// The brief's sentence "If both capitals were created at the same time" is
/// unfinished, so the tie-break is ours to choose. It is the lowest tile id:
/// arbitrary, but reproducible on every machine and in every replay.
/// See TERR-041.
#[test]
fn equidistant_capitals_are_broken_by_lowest_tile_id() {
    covers!("TERR-041");

    // Symmetric this time: tile 2 is two hops from both capitals.
    let mut session = WorldBuilder::new(topo::line(5))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .own(Faction::Red, &[3, 4])
        .capital(4)
        .unit(UnitKind::Pawn, 3)
        .session();

    let pawn = session.world().tile(TileId(3)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(2),
        })
        .unwrap();

    let merged: Vec<&Territory> = session.world().territories_of(Faction::Red).collect();
    assert_eq!(merged.len(), 1);
    assert_eq!(merged[0].capital, TileId(0));
}

#[test]
fn territory_identity_survives_an_ordinary_border_nudge() {
    covers!("TERR-031");

    // Losing an edge tile should not renumber the territory under the HUD.
    let mut session = strip_world(15, 10, 10);
    let pawn = session.world().tile(TileId(31)).unit.unwrap();
    let before = terr(session.world(), 0).id;

    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(15),
        })
        .unwrap();

    assert_eq!(terr(session.world(), 0).id, before);
    assert_eq!(terr(session.world(), 0).size(), 15);
}

#[test]
fn every_territory_always_has_exactly_one_capital() {
    covers!("TERR-010");

    let mut session = strip_world(10, 15, 15);
    let pawn = session.world().tile(TileId(26)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(10),
        })
        .unwrap();

    let world = session.world();
    for t in world.territories.values() {
        let capitals = t
            .tiles
            .iter()
            .filter(|&&tile| world.tile(tile).kind == TileKind::Capital)
            .count();
        assert_eq!(capitals, 1, "territory {} has {capitals} capitals", t.id);
        assert!(t.tiles.contains(&t.capital));
    }
}
