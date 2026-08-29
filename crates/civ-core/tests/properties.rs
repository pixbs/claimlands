//! Property tests — gate 4.
//!
//! These play thousands of randomised matches and assert, after every single
//! command, that the world is still structurally sound. Where the golden
//! replays pin down *known* scenarios, this finds the ones nobody thought of:
//! the compound capture that splits one territory while merging two others,
//! the famine that empties a territory on the same turn its capital falls.
//!
//! Every command comes from [`legal_commands`], so the fuzzer only ever plays
//! moves a real player could have made.

use civ_core::apply::legal_commands;
use civ_core::invariants;
use civ_core::rng::Rng;
use civ_testkit::covers;
use civ_testkit::prelude::*;
use proptest::prelude::*;

/// Four factions on a 6×6 hex grid, one capital each in a corner.
///
/// Small enough that thousands of matches run in seconds, big enough that
/// territories genuinely split, merge and collapse.
fn arena() -> Session {
    WorldBuilder::new(topo::hex_grid(6, 6))
        .all_land()
        .player(Faction::Red)
        .player(Faction::Yellow)
        .player(Faction::Green)
        .player(Faction::Blue)
        .own(Faction::Red, &[0])
        .capital(0)
        .own(Faction::Yellow, &[5])
        .capital(5)
        .own(Faction::Green, &[30])
        .capital(30)
        .own(Faction::Blue, &[35])
        .capital(35)
        // Some scenery to capture and raze.
        .kinds(&[8, 9, 14], TileKind::Forest)
        .session()
}

/// Play randomly, checking the world after every command.
fn play_random(seed: u64, steps: usize, check_every_step: bool) -> Session {
    let mut session = arena();
    let mut rng = Rng::seed_from_u64(seed);

    for step in 0..steps {
        if session.world().is_over() {
            break;
        }
        let options = legal_commands(session.world(), session.rules());
        let Some(cmd) = rng.pick(&options).cloned() else {
            break;
        };

        session.execute(cmd.clone()).unwrap_or_else(|e| {
            panic!("legal_commands offered {cmd:?} at step {step}, but it was refused: {e}")
        });

        if check_every_step {
            let violations = invariants::check(session.world());
            assert!(
                violations.is_empty(),
                "after step {step} ({cmd:?}) the world broke:\n{}",
                violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    session
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The headline property: no sequence of legal moves can produce an
    /// unsound world. Every territory stays connected with exactly one
    /// capital, no treasury goes negative, no two units share a tile.
    #[test]
    fn random_play_never_breaks_the_invariants(seed in any::<u64>()) {
        covers!("INV-001");
        play_random(seed, 300, true);
    }

    /// Determinism: the same command log against the same starting world
    /// always lands on the same state. Future multiplayer depends on this
    /// holding across machines, not just across runs.
    #[test]
    fn replaying_a_random_match_reproduces_it_exactly(seed in any::<u64>()) {
        covers!("INV-002");

        let session = play_random(seed, 200, false);
        let expected = session.state_hash();

        let replayed = Session::replay(
            session.initial_world().clone(),
            session.rules().clone(),
            Some(session.rules().hash()),
            session.log(),
        ).expect("a log this build produced always replays");

        prop_assert_eq!(replayed.state_hash(), expected);
    }

    /// Running the same seed twice produces identical play. Guards against
    /// anything sneaking in that reads the clock, the address space, or a
    /// hash map's iteration order.
    #[test]
    fn the_same_seed_always_plays_the_same_match(seed in any::<u64>()) {
        covers!("INV-002");

        let a = play_random(seed, 150, false);
        let b = play_random(seed, 150, false);
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.log(), b.log());
    }
}

/// Undoing the last command and replaying it must land exactly where it was.
#[test]
fn undo_then_redo_is_the_identity() {
    covers!("SESS-001");

    for seed in 0..40u64 {
        let mut session = play_random(seed, 60, false);
        if !session.can_undo() {
            continue;
        }

        let before = session.state_hash();
        let last = session
            .log()
            .last()
            .cloned()
            .expect("can_undo implies a command");

        session.undo().expect("can_undo was true");
        session
            .execute(last.clone())
            .unwrap_or_else(|e| panic!("seed {seed}: redoing {last:?} failed: {e}"));

        assert_eq!(
            session.state_hash(),
            before,
            "seed {seed}: undo followed by redo changed the world"
        );
    }
}

/// A split must not create or destroy wheat or gold. Checked directly rather
/// than through random play, because income makes the global total move.
#[test]
fn splitting_a_territory_conserves_its_treasury() {
    covers!("TERR-030");

    // Sweep widths and break points so the floor-division remainder lands in
    // every possible place.
    for width in [7u32, 8, 11, 16] {
        for cut in 2..width - 2 {
            for purse in [0i32, 1, 7, 15, 100, 101] {
                let mut session = WorldBuilder::new(topo::grid(width, 2))
                    .all_land()
                    .player(Faction::Blue)
                    .player(Faction::Red)
                    .own(Faction::Red, &(0..width).collect::<Vec<_>>())
                    .capital(0)
                    .treasury(0, purse, purse)
                    .own(Faction::Blue, &[width + cut])
                    .capital(width + cut)
                    .treasury(width + cut, 20, 0)
                    .unit(UnitKind::Pawn, width + cut)
                    .session();

                let pawn = session.world().tile(TileId(width + cut)).unit.unwrap();
                session
                    .execute(Command::MoveUnit {
                        unit: pawn,
                        to: TileId(cut),
                    })
                    .unwrap_or_else(|e| panic!("w={width} cut={cut}: {e}"));

                let world = session.world();
                assert_sound(world);

                let wheat: i32 = world.territories_of(Faction::Red).map(|t| t.wheat).sum();
                let gold: i32 = world.territories_of(Faction::Red).map(|t| t.gold).sum();
                assert_eq!(
                    wheat, purse,
                    "w={width} cut={cut} purse={purse}: wheat was not conserved"
                );
                assert_eq!(
                    gold, purse,
                    "w={width} cut={cut} purse={purse}: gold was not conserved"
                );
            }
        }
    }
}

/// Territories are a partition: every owned tile belongs to exactly one, and
/// no territory claims a tile it does not own.
#[test]
fn territories_always_partition_the_tiles_a_faction_owns() {
    covers!("INV-003");

    for seed in 0..30u64 {
        let session = play_random(seed, 120, false);
        let world = session.world();

        for faction in Faction::ALL {
            let owned = world.tiles_of(faction);
            let claimed: std::collections::BTreeSet<TileId> = world
                .territories_of(faction)
                .flat_map(|t| t.tiles.iter().copied())
                .collect();
            assert_eq!(
                owned, claimed,
                "seed {seed}: {faction}'s territories do not cover exactly its tiles"
            );
        }
    }
}
