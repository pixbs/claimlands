//! The golden replay harness — gate 3, the primary regression net.
//!
//! A golden replay is a self-contained RON file: how to build the world, the
//! commands to run, and the state hash the run must end on. Because the hash
//! covers every simulation-visible field ([`lands_core::hash::world_hash`]), a
//! change to *any* rule moves it, including rules the author never intended to
//! touch. That is the point: it catches the regression nobody predicted.
//!
//! # Adding one
//!
//! Write the scenario, run `cargo run -p lands-cli -- golden record <file>`, and
//! commit the file with the hash it prints. Review it like a test, because it
//! is one — a hash that changes in a PR means the PR changed behaviour, and the
//! author must say why in the description.
//!
//! # When a hash legitimately changes
//!
//! A deliberate balance or rule change invalidates every replay at once, which
//! looks alarming and is meant to. Re-record them in a *separate commit* from
//! the change itself, so the diff shows exactly which scenarios moved.

use lands_core::prelude::*;
use lands_core::state::Controller;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{WorldBuilder, topo};

#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not parse {path}: {message}")]
    Parse { path: PathBuf, message: String },

    #[error(
        "replay `{name}` was recorded against ruleset {recorded:#018x} but this build has \
         {current:#018x}; the balance changed, so re-record the replay in its own commit"
    )]
    RulesetChanged {
        name: String,
        recorded: u64,
        current: u64,
    },

    #[error("replay `{name}`: command {index} ({command}) was rejected: {reason}")]
    CommandRejected {
        name: String,
        index: usize,
        command: String,
        reason: Rejection,
    },

    #[error(
        "replay `{name}` ended on state {actual:#018x}, expected {expected:#018x}.\n\
         Some rule changed behaviour. If that was intended, re-record; if not, this is the \
         regression."
    )]
    HashMismatch {
        name: String,
        expected: u64,
        actual: u64,
    },

    #[error("replay `{name}` left the world unsound:\n{report}")]
    Unsound { name: String, report: String },
}

/// Which test topology to build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TopologySpec {
    Line(u32),
    Grid(u32, u32),
    HexGrid(u32, u32),
}

impl TopologySpec {
    pub fn build(&self) -> Topology {
        match *self {
            TopologySpec::Line(n) => topo::line(n),
            TopologySpec::Grid(w, h) => topo::grid(w, h),
            TopologySpec::HexGrid(w, h) => topo::hex_grid(w, h),
        }
    }
}

/// Everything needed to reconstruct the starting world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Setup {
    pub topology: TopologySpec,
    #[serde(default)]
    pub seed: u64,
    /// Tiles that are land. `None` means the whole planet is land.
    #[serde(default)]
    pub land: Option<Vec<u32>>,
    pub players: Vec<PlayerSpec>,
    #[serde(default)]
    pub ownership: Vec<(Faction, Vec<u32>)>,
    #[serde(default)]
    pub capitals: Vec<u32>,
    #[serde(default)]
    pub kinds: Vec<(u32, TileKind)>,
    #[serde(default)]
    pub units: Vec<(Faction, UnitKind, u32)>,
    /// `(capital tile, wheat, gold)` — overrides the derived territory's purse.
    #[serde(default)]
    pub treasuries: Vec<(u32, i32, i32)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSpec {
    pub faction: Faction,
    #[serde(default)]
    pub ai: Option<String>,
}

impl Setup {
    /// Reconstruct the world this replay starts from.
    pub fn build(&self) -> (World, Ruleset) {
        let mut b = WorldBuilder::new(self.topology.build()).seed(self.seed);

        b = match &self.land {
            None => b.all_land(),
            Some(tiles) => b.land(tiles),
        };

        for p in &self.players {
            b = match &p.ai {
                None => b.player(p.faction),
                Some(profile) => b.ai_player(p.faction, profile),
            };
        }
        for (faction, tiles) in &self.ownership {
            b = b.own(*faction, tiles);
        }
        for &(tile, kind) in &self.kinds {
            b = b.kind(tile, kind);
        }
        for &tile in &self.capitals {
            b = b.capital(tile);
        }
        for &(faction, kind, tile) in &self.units {
            b = b.unit_of(faction, kind, tile);
        }
        for &(tile, wheat, gold) in &self.treasuries {
            b = b.treasury(tile, wheat, gold);
        }

        let _ = Controller::Human; // keep the import meaningful for readers
        b.build()
    }
}

/// One committed regression scenario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenReplay {
    pub name: String,
    /// What this scenario is protecting. Write a sentence, not a label.
    pub description: String,
    /// Spec rule ids this scenario exercises, for gate 12.
    #[serde(default)]
    pub rules: Vec<String>,
    pub ruleset_hash: u64,
    pub setup: Setup,
    pub commands: Vec<Command>,
    pub expected_state_hash: u64,
}

impl GoldenReplay {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, GoldenError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| GoldenError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        ron::from_str(&text).map_err(|e| GoldenError::Parse {
            path: path.to_path_buf(),
            message: e.to_string(),
        })
    }

    /// Rewrite only the two hash fields, leaving the rest of the file — and
    /// crucially its comments — exactly as the author wrote them.
    ///
    /// Re-serialising the whole struct would be simpler but would strip every
    /// comment, and the comment explaining *why* a scenario exists is the most
    /// valuable part of a golden replay.
    pub fn save_hashes(&self, path: impl AsRef<Path>) -> Result<(), GoldenError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| GoldenError::Io {
            path: path.to_path_buf(),
            source,
        })?;

        let patched = text
            .lines()
            .map(|line| match hash_field(line) {
                Some(("ruleset_hash", indent)) => {
                    format!("{indent}ruleset_hash: {},", self.ruleset_hash)
                }
                Some(("expected_state_hash", indent)) => {
                    format!("{indent}expected_state_hash: {},", self.expected_state_hash)
                }
                _ => line.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n");

        std::fs::write(path, patched + "\n").map_err(|source| GoldenError::Io {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Run the scenario, returning the finished session.
    pub fn run(&self) -> Result<Session, GoldenError> {
        let (world, rules) = self.setup.build();

        let current = rules.hash();
        if self.ruleset_hash != current {
            return Err(GoldenError::RulesetChanged {
                name: self.name.clone(),
                recorded: self.ruleset_hash,
                current,
            });
        }

        let mut session = Session::start(world, rules);
        for (index, cmd) in self.commands.iter().enumerate() {
            session
                .execute(cmd.clone())
                .map_err(|reason| GoldenError::CommandRejected {
                    name: self.name.clone(),
                    index,
                    command: format!("{cmd:?}"),
                    reason,
                })?;
        }
        Ok(session)
    }

    /// Run and assert both the invariants and the recorded hash.
    pub fn verify(&self) -> Result<(), GoldenError> {
        let session = self.run()?;

        let violations = lands_core::invariants::check(session.world());
        if !violations.is_empty() {
            return Err(GoldenError::Unsound {
                name: self.name.clone(),
                report: violations
                    .iter()
                    .map(|v| format!("  - {v}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            });
        }

        let actual = session.state_hash();
        if actual != self.expected_state_hash {
            return Err(GoldenError::HashMismatch {
                name: self.name.clone(),
                expected: self.expected_state_hash,
                actual,
            });
        }
        Ok(())
    }

    /// Re-run and rewrite the recorded hashes. Used by
    /// `lands-cli golden record`; never call this from a test.
    pub fn rerecord(&mut self) -> Result<(), GoldenError> {
        let (_, rules) = self.setup.build();
        self.ruleset_hash = rules.hash();
        let session = self.run()?;
        self.expected_state_hash = session.state_hash();
        Ok(())
    }
}

/// If a line assigns one of the hash fields, its name and leading whitespace.
fn hash_field(line: &str) -> Option<(&'static str, &str)> {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    for name in ["ruleset_hash", "expected_state_hash"] {
        if let Some(after) = rest.strip_prefix(name)
            && after.trim_start().starts_with(':')
        {
            return Some((name, indent));
        }
    }
    None
}

/// Every `.ron` replay in a directory, sorted by filename for stable output.
pub fn load_dir(dir: impl AsRef<Path>) -> Result<Vec<(PathBuf, GoldenReplay)>, GoldenError> {
    let dir = dir.as_ref();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| GoldenError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "ron"))
        .collect();
    paths.sort();

    paths
        .into_iter()
        .map(|p| GoldenReplay::load(&p).map(|r| (p, r)))
        .collect()
}
