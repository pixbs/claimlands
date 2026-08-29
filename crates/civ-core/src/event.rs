//! Events — everything that happened, in the order it happened.
//!
//! # The wall between simulation and presentation
//!
//! The renderer reacts to these and **never reads simulation state to decide
//! what to animate**. That is the boundary that stops a rendering change from
//! being able to affect game logic, and it is why a rendering agent and a
//! rules agent can work on the same feature without coordinating.
//!
//! Events are also a readable audit trail: a failing golden test prints the
//! event stream, which usually identifies the broken rule immediately.

use crate::ids::{Faction, TerritoryId, TileId, UnitId};
use crate::state::Outcome;
use civ_rules::{TileKind, UnitKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    // ---- turn structure --------------------------------------------------
    TurnBegan {
        faction: Faction,
        round: u32,
    },
    TurnEnded {
        faction: Faction,
    },
    RoundBegan {
        round: u32,
    },

    // ---- economy ---------------------------------------------------------
    /// Tile yields collected, before towns are fed.
    ResourcesProduced {
        territory: TerritoryId,
        wheat: i32,
        gold: i32,
    },
    /// ECON-004. `fed` of `of` towns produced gold this turn.
    TownsFed {
        territory: TerritoryId,
        fed: u32,
        of: u32,
    },
    /// ECON-005. A unit was lost to famine, not to combat.
    UnitStarved {
        unit: UnitId,
        kind: UnitKind,
        faction: Faction,
        at: TileId,
    },

    // ---- units -----------------------------------------------------------
    UnitRecruited {
        unit: UnitId,
        kind: UnitKind,
        faction: Faction,
        at: TileId,
        territory: TerritoryId,
        cost: i32,
    },
    UnitUpgraded {
        unit: UnitId,
        from: UnitKind,
        to: UnitKind,
        at: TileId,
        cost: i32,
    },
    UnitMoved {
        unit: UnitId,
        from: TileId,
        to: TileId,
    },
    /// Destroyed by an enemy unit.
    UnitKilled {
        unit: UnitId,
        kind: UnitKind,
        faction: Faction,
        at: TileId,
        by: UnitId,
    },
    /// Destroyed because the territory it stood in ceased to exist
    /// (TERR-013: a forest-only remnant is disbanded).
    UnitDisbanded {
        unit: UnitId,
        kind: UnitKind,
        faction: Faction,
        at: TileId,
    },

    // ---- tiles -----------------------------------------------------------
    TileBuilt {
        tile: TileId,
        kind: TileKind,
        territory: TerritoryId,
        cost: i32,
    },
    /// Ownership changed hands. `previous_kind` is what stood there before the
    /// capture razed it, which the renderer needs in order to play the right
    /// destruction effect.
    TileCaptured {
        tile: TileId,
        from: Option<Faction>,
        to: Faction,
        previous_kind: TileKind,
    },
    /// TERR-020. A capital fell and the captor took a share of its treasury.
    CapitalRazed {
        tile: TileId,
        faction: Faction,
        gold_looted: i32,
    },
    /// GROW-001.
    ForestSpread {
        from: TileId,
        to: TileId,
    },

    // ---- territories -----------------------------------------------------
    TerritoryCreated {
        territory: TerritoryId,
        faction: Faction,
        capital: TileId,
        tiles: u32,
        wheat: i32,
        gold: i32,
    },
    TerritoryDissolved {
        territory: TerritoryId,
        faction: Faction,
    },
    /// TERR-011. The capital moved because the old one was destroyed or
    /// because two territories merged and one capital had to go.
    CapitalRelocated {
        territory: TerritoryId,
        from: Option<TileId>,
        to: TileId,
    },
    /// TERR-030. One territory became several.
    TerritorySplit {
        parent: TerritoryId,
        into: Vec<TerritoryId>,
    },
    /// TERR-040. Several territories became one; treasuries were summed.
    TerritoriesMerged {
        kept: TerritoryId,
        absorbed: Vec<TerritoryId>,
    },
    /// TERR-013. A remnant with no valid capital site lost its owner entirely.
    TerritoryDisbanded {
        territory: TerritoryId,
        faction: Faction,
        tiles: u32,
    },

    // ---- match -----------------------------------------------------------
    PlayerEliminated {
        faction: Faction,
    },
    GameOver {
        outcome: Outcome,
    },
}

/// Collects events during command application.
///
/// A plain `Vec` behind a named type so that call sites read clearly and so
/// that a future buffered/streaming sink is a non-breaking change.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EventSink {
    events: Vec<Event>,
}

impl EventSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn into_events(self) -> Vec<Event> {
        self.events
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
}

impl Extend<Event> for EventSink {
    fn extend<T: IntoIterator<Item = Event>>(&mut self, iter: T) {
        self.events.extend(iter);
    }
}
