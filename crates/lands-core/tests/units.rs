//! Unit rules: movement, capture, combat and upgrades (UNIT-002 … UNIT-031).

use lands_core::movement::reachable;
use lands_testkit::covers;
use lands_testkit::prelude::*;
use std::collections::BTreeSet;

fn tiles(ids: &[u32]) -> BTreeSet<TileId> {
    ids.iter().copied().map(TileId).collect()
}

/// Red owns `0..=4` of a ten-tile line with its capital at 0; everything east
/// is neutral. Enough wheat that nothing starves mid-test.
fn corridor() -> Session {
    WorldBuilder::new(topo::line(10))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 0)
        .session()
}

#[test]
fn a_unit_moves_four_tiles_through_its_own_territory() {
    covers!("UNIT-030");

    let session = corridor();
    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    let can_reach = reachable(session.world(), session.rules(), pawn);

    // Four steps of home ground, plus one step out onto neutral tile 5.
    assert_eq!(can_reach, tiles(&[1, 2, 3, 4, 5]));
    assert!(
        !can_reach.contains(&TileId(6)),
        "tile 6 would need a second foreign step"
    );
}

#[test]
fn a_unit_takes_only_one_step_beyond_its_own_border() {
    covers!("UNIT-031");

    let session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 0)
        .session();

    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    assert_eq!(
        reachable(session.world(), session.rules(), pawn),
        tiles(&[1])
    );
}

#[test]
fn units_block_the_path_rather_than_being_walked_through() {
    covers!("UNIT-032");

    let session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1, 2, 3, 4])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 0)
        .unit(UnitKind::Pawn, 2)
        .session();

    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    // Tile 2 is blocked, so 3 and 4 are unreachable even though they are ours
    // and well within the four-step budget.
    assert_eq!(
        reachable(session.world(), session.rules(), pawn),
        tiles(&[1])
    );
}

#[test]
fn a_pawn_may_not_take_a_capital_but_a_warrior_may() {
    covers!("UNIT-010a", "UNIT-010b");

    let build = |kind: UnitKind| {
        WorldBuilder::new(topo::line(4))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(kind, 0)
            .own(Faction::Blue, &[1])
            .capital(1)
            .session()
    };

    let session = build(UnitKind::Pawn);
    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    assert!(
        !reachable(session.world(), session.rules(), pawn).contains(&TileId(1)),
        "a pawn cannot storm a capital"
    );

    let session = build(UnitKind::Warrior);
    let warrior = session.world().tile(TileId(0)).unit.unwrap();
    assert!(reachable(session.world(), session.rules(), warrior).contains(&TileId(1)));
}

#[test]
fn a_pawn_may_not_take_a_town_but_a_warrior_may() {
    covers!("UNIT-010a", "UNIT-010b");

    let build = |kind: UnitKind| {
        WorldBuilder::new(topo::line(5))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(kind, 0)
            .own(Faction::Blue, &[1, 2])
            .capital(2)
            .kind(1, TileKind::Town)
            .session()
    };

    let session = build(UnitKind::Pawn);
    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    assert!(!reachable(session.world(), session.rules(), pawn).contains(&TileId(1)));

    let session = build(UnitKind::Warrior);
    let warrior = session.world().tile(TileId(0)).unit.unwrap();
    assert!(reachable(session.world(), session.rules(), warrior).contains(&TileId(1)));
}

#[test]
fn a_warrior_beats_a_pawn_but_not_another_warrior() {
    covers!("UNIT-011b");

    let build = |defender: UnitKind| {
        WorldBuilder::new(topo::line(4))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(UnitKind::Warrior, 0)
            .own(Faction::Blue, &[1, 2])
            .capital(2)
            .treasury(2, 50, 50)
            .unit_of(Faction::Blue, defender, 1)
            .session()
    };

    let session = build(UnitKind::Pawn);
    let attacker = session.world().tile(TileId(0)).unit.unwrap();
    assert!(reachable(session.world(), session.rules(), attacker).contains(&TileId(1)));

    let session = build(UnitKind::Warrior);
    let attacker = session.world().tile(TileId(0)).unit.unwrap();
    assert!(
        !reachable(session.world(), session.rules(), attacker).contains(&TileId(1)),
        "a warrior cannot dislodge another warrior"
    );
}

#[test]
fn a_knight_beats_anything_including_another_knight() {
    covers!("UNIT-011c", "UNIT-010c");

    for defender in [UnitKind::Pawn, UnitKind::Warrior, UnitKind::Knight] {
        let session = WorldBuilder::new(topo::line(4))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(UnitKind::Knight, 0)
            .own(Faction::Blue, &[1, 2])
            .capital(2)
            .treasury(2, 50, 50)
            .unit_of(Faction::Blue, defender, 1)
            .session();

        let knight = session.world().tile(TileId(0)).unit.unwrap();
        assert!(
            reachable(session.world(), session.rules(), knight).contains(&TileId(1)),
            "a knight should be able to defeat a {defender:?}"
        );
    }
}

/// A pawn is an expansion tool, not a weapon: it takes open ground but cannot
/// remove even the weakest defender.
#[test]
fn a_pawn_cannot_fight_at_all() {
    covers!("UNIT-011a");

    for defender in [UnitKind::Pawn, UnitKind::Warrior, UnitKind::Knight] {
        let session = WorldBuilder::new(topo::line(4))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(UnitKind::Pawn, 0)
            .own(Faction::Blue, &[1, 2])
            .capital(2)
            .treasury(2, 50, 50)
            .unit_of(Faction::Blue, defender, 1)
            .session();

        let pawn = session.world().tile(TileId(0)).unit.unwrap();
        assert!(
            !reachable(session.world(), session.rules(), pawn).contains(&TileId(1)),
            "a pawn should not be able to defeat a {defender:?}"
        );
    }
}

#[test]
fn a_unit_never_moves_onto_a_friend() {
    covers!("UNIT-012");

    let session = corridor();
    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    // Tile 0 holds the mover itself, and it is not offered as a destination.
    assert!(!reachable(session.world(), session.rules(), pawn).contains(&TileId(0)));
}

#[test]
fn capturing_razes_whatever_stood_on_the_tile() {
    covers!("UNIT-013");

    for razed in [TileKind::Town, TileKind::Field, TileKind::Forest] {
        let mut session = WorldBuilder::new(topo::line(5))
            .all_land()
            .player(Faction::Red)
            .player(Faction::Blue)
            .own(Faction::Red, &[0])
            .capital(0)
            .treasury(0, 50, 50)
            .unit(UnitKind::Warrior, 0)
            .own(Faction::Blue, &[1, 2])
            .capital(2)
            .kind(1, razed)
            .session();

        let warrior = session.world().tile(TileId(0)).unit.unwrap();
        session
            .execute(Command::MoveUnit {
                unit: warrior,
                to: TileId(1),
            })
            .unwrap_or_else(|e| panic!("capturing a {razed:?} tile should be legal: {e}"));

        assert_eq!(
            session.world().tile(TileId(1)).kind,
            TileKind::Empty,
            "a captured {razed:?} should be razed to empty ground"
        );
    }
}

#[test]
fn upgrading_uses_the_units_action_for_the_turn() {
    covers!("UNIT-021");

    let mut session = corridor();
    let pawn = session.world().tile(TileId(0)).unit.unwrap();

    session
        .execute(Command::UpgradeUnit { unit: pawn })
        .expect("a pawn with gold and an unused move may be promoted");

    assert_eq!(session.world().unit(pawn).unwrap().kind, UnitKind::Warrior);
    assert!(session.world().unit(pawn).unwrap().moved);
    assert!(
        reachable(session.world(), session.rules(), pawn).is_empty(),
        "the upgrade consumed the move"
    );
}

#[test]
fn upgrading_refuses_a_unit_that_already_acted() {
    covers!("UNIT-021");

    let mut session = corridor();
    let pawn = session.world().tile(TileId(0)).unit.unwrap();

    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(1),
        })
        .unwrap();

    assert_eq!(
        session.execute(Command::UpgradeUnit { unit: pawn }),
        Err(Rejection::AlreadyMoved(pawn))
    );
}

#[test]
fn a_knight_is_the_top_of_the_chain() {
    covers!("UNIT-020c");

    let mut session = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Knight, 1)
        .session();

    let knight = session.world().tile(TileId(1)).unit.unwrap();
    assert_eq!(
        session.execute(Command::UpgradeUnit { unit: knight }),
        Err(Rejection::NoUpgradeAvailable(UnitKind::Knight))
    );
}

#[test]
fn a_fresh_recruit_cannot_act_on_the_turn_it_is_bought() {
    covers!("ECON-010b");

    let mut session = corridor();
    let territory = session.world().territory_at(TileId(0)).unwrap().id;

    session
        .execute(Command::RecruitUnit {
            territory,
            at: TileId(2),
        })
        .expect("an owned, empty, unoccupied tile can take a recruit");

    let recruit = session.world().tile(TileId(2)).unit.unwrap();
    assert!(session.world().unit(recruit).unwrap().moved);
    assert!(reachable(session.world(), session.rules(), recruit).is_empty());
}

#[test]
fn a_unit_that_has_acted_has_no_moves_left() {
    covers!("UNIT-030");

    let mut session = corridor();
    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(2),
        })
        .unwrap();

    assert!(reachable(session.world(), session.rules(), pawn).is_empty());
    assert_eq!(
        session.execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(3)
        }),
        Err(Rejection::AlreadyMoved(pawn))
    );
}

#[test]
fn moves_refresh_at_the_start_of_the_owners_next_turn() {
    covers!("UNIT-030");

    let mut session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 0)
        .own(Faction::Blue, &[5])
        .capital(5)
        .treasury(5, 50, 50)
        .session();

    let pawn = session.world().tile(TileId(0)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(1),
        })
        .unwrap();
    assert!(session.world().unit(pawn).unwrap().moved);

    session.execute(Command::EndTurn).unwrap(); // Red -> Blue
    session.execute(Command::EndTurn).unwrap(); // Blue -> Red, new round

    assert!(!session.world().unit(pawn).unwrap().moved);
}
