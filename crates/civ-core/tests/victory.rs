//! Victory conditions and the match record (VICT-002, VICT-003, VICT-010).
//!
//! Elimination (VICT-001) is covered in `session.rs`, where the capture that
//! causes it belongs.

use civ_testkit::covers;
use civ_testkit::prelude::*;

/// Rules with a dominance threshold that a small fixture can actually reach.
fn dominance_at(percent: u32, rounds: u32) -> Ruleset {
    let mut rules = Ruleset::bundled();
    rules.victory.dominance_threshold_percent = percent;
    rules.victory.dominance_turns = rounds;
    rules
}

/// Red holds four of five land tiles — 80% — with Blue clinging to one.
fn lopsided(rules: Ruleset) -> Session {
    WorldBuilder::new(topo::line(5))
        .rules(rules)
        .land(&[0, 1, 2, 3, 4])
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2, 3])
        .capital(0)
        .own(Faction::Blue, &[4])
        .capital(4)
        .session()
}

#[test]
fn holding_most_of_the_planet_wins_the_match() {
    covers!("VICT-002");

    let mut session = lopsided(dominance_at(70, 1));
    assert!(session.world().outcome.is_none());

    session.execute(Command::EndTurn).unwrap(); // Red -> Blue
    session.execute(Command::EndTurn).unwrap(); // Blue -> round ends

    match session.world().outcome {
        Some(Outcome::Victory {
            faction, reason, ..
        }) => {
            assert_eq!(faction, Faction::Red);
            assert_eq!(reason, VictoryReason::Dominance);
        }
        other => panic!("expected Red to win on dominance, got {other:?}"),
    }
}

#[test]
fn a_lead_below_the_threshold_wins_nothing() {
    covers!("VICT-002");

    // 80% held, 90% required.
    let mut session = lopsided(dominance_at(90, 1));
    for _ in 0..6 {
        session.execute(Command::EndTurn).unwrap();
    }
    assert!(
        session.world().outcome.is_none(),
        "80% should not win when 90% is required"
    );
}

#[test]
fn dominance_must_be_held_for_the_required_number_of_rounds() {
    covers!("VICT-003");

    let mut session = lopsided(dominance_at(70, 3));

    // Two full rounds is not yet enough.
    for _ in 0..4 {
        session.execute(Command::EndTurn).unwrap();
    }
    assert!(
        session.world().outcome.is_none(),
        "two rounds is short of three"
    );

    session.execute(Command::EndTurn).unwrap();
    session.execute(Command::EndTurn).unwrap();
    assert!(
        session.world().outcome.is_some(),
        "the third consecutive round should decide it"
    );
}

#[test]
fn dominance_is_counted_once_per_round_not_once_per_turn() {
    covers!("VICT-003");

    // Four players would accumulate a streak four times as fast if the check
    // ran per turn. Red holds 5 of 8 land tiles: dominant at 60%, not at 70%.
    let mut session = WorldBuilder::new(topo::line(8))
        .rules(dominance_at(60, 2))
        .land(&[0, 1, 2, 3, 4, 5, 6, 7])
        .player(Faction::Red)
        .player(Faction::Yellow)
        .player(Faction::Green)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2, 3, 4])
        .capital(2)
        .own(Faction::Yellow, &[5])
        .capital(5)
        .own(Faction::Green, &[6])
        .capital(6)
        .own(Faction::Blue, &[7])
        .capital(7)
        .session();

    // One full round of four turns: streak reaches 1, not 4.
    for _ in 0..4 {
        session.execute(Command::EndTurn).unwrap();
    }
    assert!(
        session.world().outcome.is_none(),
        "one round should not satisfy a two-round requirement"
    );

    for _ in 0..4 {
        session.execute(Command::EndTurn).unwrap();
    }
    assert!(
        session.world().outcome.is_some(),
        "the second round decides it"
    );
}

#[test]
fn the_match_record_accumulates_for_the_victory_screen() {
    covers!("VICT-010");

    let mut session = WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .treasury(0, 60, 60)
        .unit(UnitKind::Warrior, 2)
        .own(Faction::Blue, &[3, 4])
        .capital(4)
        .treasury(4, 60, 60)
        .unit_of(Faction::Blue, UnitKind::Pawn, 3)
        .session();

    let territory = session.world().territory_at(TileId(0)).unwrap().id;
    session
        .execute(Command::BuildTown {
            territory,
            at: TileId(1),
        })
        .unwrap();

    let warrior = session.world().tile(TileId(2)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(3),
        })
        .expect("a warrior may defeat a pawn and take the ground");

    let red = session.world().stats_of(Faction::Red);
    assert_eq!(red.towns_built, 1);
    assert_eq!(red.units_killed, 1);
    assert_eq!(red.tiles_captured, 1);
    assert!(red.gold_earned > 0, "income should be recorded");
    assert!(red.peak_tiles >= 3);

    let blue = session.world().stats_of(Faction::Blue);
    assert_eq!(blue.units_lost, 1);
    assert_eq!(blue.tiles_lost, 1);
}
