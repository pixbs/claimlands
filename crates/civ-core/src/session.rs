//! A playable match: the world, the command log, and undo.
//!
//! # Undo (the brief: "undo any move within the turn until the turn is played")
//!
//! Undo works by restoring a snapshot taken at the start of the current turn
//! and replaying every command except the last. That is far simpler than
//! writing an inverse for each command — and, crucially, it cannot drift:
//! there is no second implementation of the rules to keep in sync. It is
//! affordable because turn-based state is small and a turn holds a handful of
//! commands.
//!
//! Ending a turn takes a fresh snapshot, which is exactly why undo stops at the
//! turn boundary.
//!
//! # Replay
//!
//! The command log plus the starting world reproduces the match exactly. A save
//! file is therefore `(ruleset_hash, level_id, Vec<Command>)` — a few hundred
//! bytes — and the same bytes are what a future multiplayer transport would
//! carry.

use crate::apply;
use crate::command::{Command, Rejection};
use crate::event::{Event, EventSink};
use crate::hash::world_hash;
use crate::state::World;
use crate::turn;
use civ_rules::Ruleset;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error(
        "this replay was recorded against ruleset {recorded:#018x}, but this build has \
         {current:#018x}; the balance has changed since it was saved"
    )]
    RulesetMismatch { recorded: u64, current: u64 },

    #[error("command {index} of the replay was rejected: {source}")]
    Rejected {
        index: usize,
        #[source]
        source: Rejection,
    },
}

/// One match in progress.
#[derive(Debug)]
pub struct Session {
    rules: Ruleset,
    world: World,
    /// The world before any command ran, for full replay and restart.
    initial: World,
    /// Every command applied, in order. This is the save file.
    log: Vec<Command>,
    /// Snapshot taken when the current turn began.
    turn_start: World,
    /// Where in `log` the current turn's commands begin.
    turn_start_index: usize,
    /// Events produced by the most recent call, for the renderer to drain.
    last_events: Vec<Event>,
}

impl Session {
    /// Begin a match from a prepared world.
    ///
    /// The world should already have its terrain, players and starting
    /// capitals in place; `civ-levels` builds it.
    pub fn start(mut world: World, rules: Ruleset) -> Self {
        let initial = world.clone();
        let mut sink = EventSink::new();
        turn::start_match(&mut world, &rules, &mut sink);

        Self {
            turn_start: world.clone(),
            initial,
            rules,
            world,
            log: Vec::new(),
            turn_start_index: 0,
            last_events: sink.into_events(),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn rules(&self) -> &Ruleset {
        &self.rules
    }

    pub fn log(&self) -> &[Command] {
        &self.log
    }

    /// Events from the most recent [`start`](Self::start), [`execute`] or
    /// [`undo`] call.
    ///
    /// [`execute`]: Self::execute
    /// [`undo`]: Self::undo
    pub fn last_events(&self) -> &[Event] {
        &self.last_events
    }

    /// Fingerprint of the current state. The assertion in golden replay tests.
    pub fn state_hash(&self) -> u64 {
        world_hash(&self.world)
    }

    /// Whether a command would be accepted, without running it.
    pub fn can(&self, cmd: &Command) -> Result<(), Rejection> {
        apply::validate(&self.world, &self.rules, cmd)
    }

    /// Run a command, recording it in the log.
    pub fn execute(&mut self, cmd: Command) -> Result<&[Event], Rejection> {
        let events = apply::apply(&mut self.world, &self.rules, &cmd)?;

        let ends_turn = matches!(cmd, Command::EndTurn);
        self.log.push(cmd);

        if ends_turn {
            // A new turn begins, so undo may no longer reach behind it.
            self.turn_start = self.world.clone();
            self.turn_start_index = self.log.len();
        }

        self.last_events = events;
        Ok(&self.last_events)
    }

    /// Whether there is a command in the current turn that can be taken back.
    pub fn can_undo(&self) -> bool {
        self.log.len() > self.turn_start_index
    }

    /// Take back the most recent command of the current turn.
    ///
    /// Returns the events of the *rebuilt* turn, so the renderer can resync to
    /// the restored state rather than trying to reverse animations.
    pub fn undo(&mut self) -> Option<&[Event]> {
        if !self.can_undo() {
            return None;
        }

        self.log.pop();
        self.world = self.turn_start.clone();

        let mut sink = EventSink::new();
        for cmd in &self.log[self.turn_start_index..] {
            // These commands were legal when first run against this exact
            // state, so they must still be legal now.
            let events = apply::apply(&mut self.world, &self.rules, cmd)
                .expect("replaying a turn's own commands cannot fail");
            sink.extend(events);
        }

        self.last_events = sink.into_events();
        Some(&self.last_events)
    }

    /// Rebuild a match by replaying a command log against a starting world.
    pub fn replay(
        initial: World,
        rules: Ruleset,
        recorded_ruleset_hash: Option<u64>,
        commands: &[Command],
    ) -> Result<Self, ReplayError> {
        if let Some(recorded) = recorded_ruleset_hash {
            let current = rules.hash();
            if recorded != current {
                return Err(ReplayError::RulesetMismatch { recorded, current });
            }
        }

        let mut session = Self::start(initial, rules);
        for (index, cmd) in commands.iter().enumerate() {
            session
                .execute(cmd.clone())
                .map_err(|source| ReplayError::Rejected { index, source })?;
        }
        Ok(session)
    }

    /// The world as it was before any command ran.
    pub fn initial_world(&self) -> &World {
        &self.initial
    }
}
