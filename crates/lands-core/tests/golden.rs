//! Gate 3: the golden replay corpus.
//!
//! Runs every scenario in `tests/replays/` and compares the resulting state
//! hash against the committed value. A hash that moves means behaviour changed
//! — deliberately or otherwise.
//!
//! See `tests/replays/README.md` for how to add one and what to do when a hash
//! changes legitimately.

use lands_testkit::golden::load_dir;
use std::path::PathBuf;

fn replay_dir() -> PathBuf {
    // Tests run with the crate root as the working directory.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/replays")
        .canonicalize()
        .expect("tests/replays should exist at the repository root")
}

#[test]
fn every_golden_replay_still_holds() {
    let replays = load_dir(replay_dir()).expect("the replay directory should be readable");
    assert!(
        !replays.is_empty(),
        "no golden replays found — the primary regression net is empty"
    );

    let mut failures = Vec::new();
    for (path, replay) in &replays {
        if let Err(e) = replay.verify() {
            failures.push(format!("{}\n{e}", path.display()));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} golden replay(s) failed:\n\n{}",
        failures.len(),
        replays.len(),
        failures.join("\n\n")
    );
}

/// Every replay should say which spec rules it exercises, so gate 12 and a
/// human reviewer can both see what it is protecting.
#[test]
fn every_golden_replay_declares_what_it_protects() {
    let replays = load_dir(replay_dir()).expect("the replay directory should be readable");

    for (path, replay) in &replays {
        assert!(
            !replay.description.trim().is_empty(),
            "{}: needs a description saying what it protects",
            path.display()
        );
        assert!(
            !replay.rules.is_empty(),
            "{}: needs at least one spec rule id in `rules`",
            path.display()
        );
    }
}
