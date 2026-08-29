//! Session behaviour: undo, replay and turn order (SESS-001 … SESS-020).

use lands_testkit::covers;
use lands_testkit::prelude::*;

/// Red and Blue face each other across a five-tile line, with enough in the
/// bank that nothing starves during a short test.
fn duel() -> Session {
    WorldBuilder::new(topo::line(6))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1, 2])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Pawn, 2)
        .own(Faction::Blue, &[5])
        .capital(5)
        .treasury(5, 50, 50)
        .session()
}

#[test]
fn undo_puts_the_world_back_exactly() {
    covers!("SESS-001");

    let mut session = duel();
    let before = session.state_hash();
    let pawn = session.world().tile(TileId(2)).unit.unwrap();

    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(3),
        })
        .unwrap();
    assert_ne!(session.state_hash(), before, "the move changed something");

    session
        .undo()
        .expect("a move inside the turn can be taken back");
    assert_eq!(
        session.state_hash(),
        before,
        "undo restores the state bit for bit"
    );
}

#[test]
fn several_moves_unwind_one_at_a_time() {
    covers!("SESS-001");

    let mut session = duel();
    let territory = session.world().territory_at(TileId(0)).unwrap().id;
    let start = session.state_hash();

    session
        .execute(Command::BuildField {
            territory,
            at: TileId(1),
        })
        .unwrap();
    let after_first = session.state_hash();

    let pawn = session.world().tile(TileId(2)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(3),
        })
        .unwrap();

    session.undo().unwrap();
    assert_eq!(session.state_hash(), after_first);

    session.undo().unwrap();
    assert_eq!(session.state_hash(), start);

    assert!(!session.can_undo(), "nothing left to undo this turn");
    assert!(session.undo().is_none());
}

/// The brief allows undo "within the turn until the turn is played" — ending
/// the turn is the commit point.
#[test]
fn undo_cannot_reach_back_past_the_end_of_a_turn() {
    covers!("SESS-002");

    let mut session = duel();
    let pawn = session.world().tile(TileId(2)).unit.unwrap();

    session
        .execute(Command::MoveUnit {
            unit: pawn,
            to: TileId(3),
        })
        .unwrap();
    session.execute(Command::EndTurn).unwrap();

    assert!(!session.can_undo());
    assert!(session.undo().is_none());
}

#[test]
fn undo_becomes_available_again_after_acting_in_the_new_turn() {
    covers!("SESS-002");

    let mut session = duel();
    session.execute(Command::EndTurn).unwrap();
    assert!(!session.can_undo());

    // Blue's turn: spend something, then take it back.
    let territory = session.world().territory_at(TileId(5)).unwrap().id;
    session
        .execute(Command::RecruitUnit {
            territory,
            at: TileId(5),
        })
        .unwrap();
    assert!(session.can_undo());
    session.undo().unwrap();
    assert!(!session.can_undo());
}

#[test]
fn replaying_a_command_log_reproduces_the_match_exactly() {
    covers!("SESS-010");

    let mut session = duel();
    let territory = session.world().territory_at(TileId(0)).unwrap().id;
    let pawn = session.world().tile(TileId(2)).unit.unwrap();

    let script = [
        Command::BuildField {
            territory,
            at: TileId(1),
        },
        Command::MoveUnit {
            unit: pawn,
            to: TileId(3),
        },
        Command::EndTurn,
        Command::EndTurn,
    ];
    for cmd in &script {
        session.execute(cmd.clone()).unwrap();
    }

    let expected = session.state_hash();

    let replayed = Session::replay(
        session.initial_world().clone(),
        session.rules().clone(),
        Some(session.rules().hash()),
        session.log(),
    )
    .expect("a log recorded by this build always replays");

    assert_eq!(replayed.state_hash(), expected);
}

#[test]
fn a_replay_recorded_against_different_balance_is_refused() {
    covers!("SESS-011");

    let session = duel();
    let stale = session.rules().hash() ^ 0xdead_beef;

    let err = Session::replay(
        session.initial_world().clone(),
        session.rules().clone(),
        Some(stale),
        &[],
    )
    .unwrap_err();

    assert!(matches!(err, ReplayError::RulesetMismatch { .. }));
}

#[test]
fn turns_rotate_between_players_and_advance_the_round() {
    covers!("SESS-020");

    let mut session = duel();
    assert_eq!(session.world().current_faction(), Some(Faction::Red));
    assert_eq!(session.world().round, 1);

    session.execute(Command::EndTurn).unwrap();
    assert_eq!(session.world().current_faction(), Some(Faction::Blue));
    assert_eq!(session.world().round, 1, "the round is not over yet");

    session.execute(Command::EndTurn).unwrap();
    assert_eq!(session.world().current_faction(), Some(Faction::Red));
    assert_eq!(session.world().round, 2);
}

#[test]
fn a_player_may_not_touch_another_players_units() {
    covers!("SESS-021");

    let mut session = duel();
    session.execute(Command::EndTurn).unwrap(); // now Blue's turn

    let red_pawn = session.world().tile(TileId(2)).unit.unwrap();
    assert_eq!(
        session.execute(Command::MoveUnit {
            unit: red_pawn,
            to: TileId(3)
        }),
        Err(Rejection::NotYourUnit {
            unit: red_pawn,
            owner: Faction::Red,
            active: Faction::Blue,
        })
    );
}

#[test]
fn eliminating_the_last_rival_ends_the_match() {
    covers!("VICT-001");

    // Blue holds a single capital tile with a Red warrior next door.
    let mut session = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Warrior, 1)
        .own(Faction::Blue, &[2])
        .capital(2)
        .session();

    let warrior = session.world().tile(TileId(1)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(2),
        })
        .unwrap();
    session.execute(Command::EndTurn).unwrap();

    match session.world().outcome {
        Some(Outcome::Victory {
            faction, reason, ..
        }) => {
            assert_eq!(faction, Faction::Red);
            assert_eq!(reason, VictoryReason::Elimination);
        }
        other => panic!("expected Red to win by elimination, got {other:?}"),
    }
}

#[test]
fn commands_are_refused_once_the_match_is_over() {
    covers!("SESS-022");

    let mut session = WorldBuilder::new(topo::line(4))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Blue)
        .own(Faction::Red, &[0, 1])
        .capital(0)
        .treasury(0, 50, 50)
        .unit(UnitKind::Warrior, 1)
        .own(Faction::Blue, &[2])
        .capital(2)
        .session();

    let warrior = session.world().tile(TileId(1)).unit.unwrap();
    session
        .execute(Command::MoveUnit {
            unit: warrior,
            to: TileId(2),
        })
        .unwrap();
    session.execute(Command::EndTurn).unwrap();

    assert_eq!(session.execute(Command::EndTurn), Err(Rejection::GameOver));
}
