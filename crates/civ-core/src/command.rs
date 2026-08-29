//! Commands — the only way the world ever changes.
//!
//! # Why everything funnels through here
//!
//! Making `Command` the single mutation channel buys five things at once:
//!
//! * **Undo** — restore the turn-start snapshot and replay all but the last
//!   command (see [`crate::session::Session`]).
//! * **Replay** — a save file *is* `(ruleset_hash, level, Vec<Command>)`.
//! * **AI** — a brain emits the same commands a human does, so it physically
//!   cannot cheat or corrupt state.
//! * **Multiplayer** — ship `Command` over a transport; nothing in `civ-core`
//!   has to change.
//! * **Regression testing** — a command log plus an expected state hash is the
//!   golden test format that guards every rule in the game.
//!
//! Validation is separated from application: [`validate`] never mutates, so
//! the UI can ask "may I?" to grey out buttons, and [`apply`] can assume the
//! command is legal.
//!
//! [`validate`]: crate::apply::validate
//! [`apply`]: crate::apply::apply

use crate::ids::{Faction, TerritoryId, TileId, UnitId};
use civ_rules::{TileKind, UnitKind};
use serde::{Deserialize, Serialize};

/// A single player action.
///
/// Adding a variant is additive: existing replays keep deserialising, because
/// they contain no instances of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Move a unit, capturing the destination if it is not already ours
    /// (UNIT-030, UNIT-031).
    MoveUnit { unit: UnitId, to: TileId },

    /// Promote a unit to the next kind up (UNIT-020). Costs gold and the
    /// unit's action for the turn.
    UpgradeUnit { unit: UnitId },

    /// Buy a new pawn and place it on an owned tile (ECON-010).
    RecruitUnit { territory: TerritoryId, at: TileId },

    /// Build a town on an owned empty tile (ECON-013).
    BuildTown { territory: TerritoryId, at: TileId },

    /// Build a field on an owned empty tile (ECON-014).
    BuildField { territory: TerritoryId, at: TileId },

    /// Pass control to the next surviving player.
    EndTurn,
}

/// Why a command was refused.
///
/// These are user-facing: the HUD shows them, so they name the specific rule
/// that was broken rather than saying "invalid move".
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum Rejection {
    #[error("the match is already over")]
    GameOver,

    #[error("no player is currently active")]
    NoActivePlayer,

    #[error("unit {0} does not exist")]
    NoSuchUnit(UnitId),

    #[error("territory {0} does not exist")]
    NoSuchTerritory(TerritoryId),

    #[error("tile {0} is out of range")]
    NoSuchTile(TileId),

    #[error("unit {unit} belongs to {owner}, but it is {active}'s turn")]
    NotYourUnit {
        unit: UnitId,
        owner: Faction,
        active: Faction,
    },

    #[error("territory {territory} belongs to {owner}, but it is {active}'s turn")]
    NotYourTerritory {
        territory: TerritoryId,
        owner: Faction,
        active: Faction,
    },

    #[error("unit {0} has already acted this turn")]
    AlreadyMoved(UnitId),

    #[error("tile {0} is water")]
    NotLand(TileId),

    #[error("tile {tile} is not inside territory {territory}")]
    NotInTerritory {
        tile: TileId,
        territory: TerritoryId,
    },

    #[error("tile {0} is already occupied by a unit")]
    TileOccupied(TileId),

    #[error("tile {tile} is {found:?}; this action needs an empty tile")]
    TileNotEmpty { tile: TileId, found: TileKind },

    #[error("tile {to} is not reachable from {from} this turn")]
    Unreachable { from: TileId, to: TileId },

    #[error("a {mover:?} may not capture a {target:?} tile")]
    CannotCaptureTile { mover: UnitKind, target: TileKind },

    #[error("a {mover:?} may not defeat a {target:?}")]
    CannotDefeatUnit { mover: UnitKind, target: UnitKind },

    #[error("tile {0} holds a friendly unit")]
    BlockedByAlly(TileId),

    #[error("a {0:?} cannot be upgraded any further")]
    NoUpgradeAvailable(UnitKind),

    #[error("territory {territory} has {available} gold but needs {needed}")]
    NotEnoughGold {
        territory: TerritoryId,
        needed: i32,
        available: i32,
    },
}

/// Which units count toward each scaling price.
///
/// Kept next to `Command` because it is part of the cost contract the UI needs
/// in order to preview prices before committing.
pub mod cost_filters {
    use civ_rules::UnitKind;

    /// ECON-010: recruiting counts every unit in the territory.
    pub fn any_unit(_: UnitKind) -> bool {
        true
    }

    /// ECON-011: upgrading to warrior counts warriors and knights, not pawns.
    pub fn warrior_or_knight(kind: UnitKind) -> bool {
        matches!(kind, UnitKind::Warrior | UnitKind::Knight)
    }

    /// ECON-012: upgrading to knight counts knights only.
    pub fn knight_only(kind: UnitKind) -> bool {
        kind == UnitKind::Knight
    }
}
