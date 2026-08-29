//! Headless tooling for the simulation.
//!
//! This is how the game is exercised without a renderer: playing whole matches,
//! fuzzing for invariant violations, and recording or verifying the golden
//! replays that guard every rule.
//!
//! ```text
//! cargo run -p lands-cli -- fuzz --matches 5000
//! cargo run -p lands-cli -- play --seed 42 --stats
//! cargo run -p lands-cli -- golden verify
//! cargo run -p lands-cli -- golden record tests/replays/capital-split.ron
//! ```

use clap::{Parser, Subcommand};
use lands_core::apply::legal_commands;
use lands_core::invariants;
use lands_core::prelude::*;
use lands_core::rng::Rng;
use lands_testkit::golden::{GoldenReplay, load_dir};
use lands_testkit::{WorldBuilder, topo};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(name = "lands", about = "Headless tools for the Claimlands simulation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Play one match with random legal moves and report the outcome.
    Play {
        #[arg(long, default_value_t = 1)]
        seed: u64,
        /// Maximum commands before giving up on a stalemate.
        #[arg(long, default_value_t = 5_000)]
        steps: usize,
        /// Print the per-faction record at the end.
        #[arg(long)]
        stats: bool,
    },

    /// Play many matches, asserting the invariants after every command.
    Fuzz {
        #[arg(long, default_value_t = 500)]
        matches: usize,
        #[arg(long, default_value_t = 400)]
        steps: usize,
        #[arg(long, default_value_t = 0)]
        seed: u64,
        /// Check invariants only at the end of each match. Much faster; use
        /// for long soak runs where a violation will still be caught.
        #[arg(long)]
        fast: bool,
    },

    /// Work with the golden replay corpus.
    Golden {
        #[command(subcommand)]
        action: GoldenAction,
    },

    /// Print the hash of the bundled ruleset.
    RulesHash,
}

#[derive(Subcommand)]
enum GoldenAction {
    /// Verify every replay in a directory.
    Verify {
        #[arg(default_value = "tests/replays")]
        dir: PathBuf,
    },
    /// Re-run a replay and rewrite its recorded hashes.
    ///
    /// Do this in a commit of its own, so a reviewer can see exactly which
    /// scenarios a rule change moved.
    Record { file: PathBuf },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("\x1b[31merror\x1b[0m: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<(), String> {
    match cli.command {
        Command::Play { seed, steps, stats } => play(seed, steps, stats),
        Command::Fuzz {
            matches,
            steps,
            seed,
            fast,
        } => fuzz(matches, steps, seed, !fast),
        Command::Golden { action } => match action {
            GoldenAction::Verify { dir } => verify_golden(&dir),
            GoldenAction::Record { file } => record_golden(&file),
        },
        Command::RulesHash => {
            println!("{:#018x}", Ruleset::bundled().hash());
            Ok(())
        }
    }
}

/// Four factions on an 8x8 board, one capital each in a corner.
///
/// The arena alternates between a **bounded** hex grid and a **wrapping**
/// torus, by seed parity. The real planet is a closed surface with no edges, so
/// fuzzing only the bounded shape would miss anything that accidentally relies
/// on the world stopping somewhere — a territory that encircles the planet, a
/// path that takes the short way round. Alternating means a single `fuzz` run
/// covers both without CI needing to know.
///
/// The bounded arena matches the property tests, so a failure on an even seed
/// reproduces there directly.
fn arena(seed: u64) -> Session {
    let topology = if seed.is_multiple_of(2) {
        topo::hex_grid(8, 8)
    } else {
        topo::torus(8, 8)
    };

    WorldBuilder::new(topology)
        .seed(seed)
        .all_land()
        .player(Faction::Red)
        .player(Faction::Yellow)
        .player(Faction::Green)
        .player(Faction::Blue)
        .own(Faction::Red, &[0])
        .capital(0)
        .own(Faction::Yellow, &[7])
        .capital(7)
        .own(Faction::Green, &[56])
        .capital(56)
        .own(Faction::Blue, &[63])
        .capital(63)
        .kinds(&[18, 19, 27, 36, 44, 45], TileKind::Forest)
        .session()
}

/// Play randomly until the match ends or the step budget runs out.
fn play_out(seed: u64, steps: usize, check_each_step: bool) -> Result<Session, String> {
    let mut session = arena(seed);
    let mut rng = Rng::seed_from_u64(seed ^ 0x9e37_79b9);

    for step in 0..steps {
        if session.world().is_over() {
            break;
        }
        let options = legal_commands(session.world(), session.rules());
        let Some(cmd) = rng.pick(&options).cloned() else {
            break;
        };

        session
            .execute(cmd.clone())
            .map_err(|e| format!("seed {seed} step {step}: {cmd:?} was refused: {e}"))?;

        if check_each_step {
            let violations = invariants::check(session.world());
            if !violations.is_empty() {
                return Err(format!(
                    "seed {seed} step {step}: {cmd:?} broke the world:\n{}",
                    violations
                        .iter()
                        .map(|v| format!("  - {v}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
    }

    Ok(session)
}

fn play(seed: u64, steps: usize, show_stats: bool) -> Result<(), String> {
    let session = play_out(seed, steps, false)?;
    let world = session.world();

    println!("seed        {seed}");
    println!("rounds      {}", world.round);
    println!("commands    {}", session.log().len());
    match world.outcome {
        Some(Outcome::Victory {
            faction,
            reason,
            round,
        }) => println!("outcome     {faction} wins by {reason:?} on round {round}"),
        Some(Outcome::Draw { round }) => println!("outcome     draw on round {round}"),
        None => println!("outcome     unresolved after {steps} commands"),
    }
    println!("state hash  {:#018x}", session.state_hash());

    if show_stats {
        println!();
        println!(
            "{:<8} {:>5} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6}",
            "faction", "tiles", "built", "killed", "starved", "gold", "wheat", "loot"
        );
        for faction in Faction::ALL {
            if world.player(faction).is_none() {
                continue;
            }
            let s = world.stats_of(faction);
            println!(
                "{:<8} {:>5} {:>6} {:>6} {:>7} {:>6} {:>6} {:>6}",
                faction.to_string(),
                world.tile_count_of(faction),
                s.towns_built + s.fields_built,
                s.units_killed,
                s.units_starved,
                s.gold_earned,
                s.wheat_earned,
                s.gold_looted,
            );
        }
    }

    Ok(())
}

fn fuzz(matches: usize, steps: usize, seed: u64, check_each_step: bool) -> Result<(), String> {
    let mut decided = 0usize;
    let mut commands = 0usize;

    for i in 0..matches {
        let session = play_out(seed.wrapping_add(i as u64), steps, check_each_step)?;
        commands += session.log().len();
        if session.world().is_over() {
            decided += 1;
        }
        // Always check the final state, even in fast mode.
        let violations = invariants::check(session.world());
        if !violations.is_empty() {
            return Err(format!(
                "match {i} ended unsound:\n{}",
                violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }

    println!("{matches} matches, {commands} commands, {decided} decided — no invariant violations");
    Ok(())
}

fn verify_golden(dir: &PathBuf) -> Result<(), String> {
    let replays = load_dir(dir).map_err(|e| e.to_string())?;
    if replays.is_empty() {
        return Err(format!("no replays found in {}", dir.display()));
    }

    let mut failures = Vec::new();
    for (path, replay) in &replays {
        match replay.verify() {
            Ok(()) => println!("  \x1b[32mok\x1b[0m   {}", path.display()),
            Err(e) => {
                println!("  \x1b[31mFAIL\x1b[0m {}", path.display());
                failures.push(e.to_string());
            }
        }
    }

    if failures.is_empty() {
        println!("\n{} replay(s) verified", replays.len());
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

fn record_golden(file: &PathBuf) -> Result<(), String> {
    let mut replay = GoldenReplay::load(file).map_err(|e| e.to_string())?;
    let before = replay.expected_state_hash;
    replay.rerecord().map_err(|e| e.to_string())?;
    replay.save_hashes(file).map_err(|e| e.to_string())?;

    if before == replay.expected_state_hash {
        println!("{}: unchanged ({:#018x})", replay.name, before);
    } else {
        println!(
            "{}: {:#018x} -> {:#018x}\n\nBehaviour changed. Commit this on its own and say why.",
            replay.name, before, replay.expected_state_hash
        );
    }
    Ok(())
}
