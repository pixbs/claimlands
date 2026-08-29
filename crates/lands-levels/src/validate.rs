//! Level sanity checks.
//!
//! These run on every load, so an author who mistypes a tile id gets a precise
//! error naming the field rather than a world that quietly plays wrong — the
//! same bargain `lands_rules::validate` makes for balance data.
//!
//! Everything here is decided from the file alone. Nothing needs the planet,
//! because the only thing a tile id has to agree with is `10·freq²+2`, and that
//! is arithmetic.

use crate::format::{
    Level, MAX_FREQUENCY, MAX_PLAYERS, MIN_FREQUENCY, MIN_PLAYERS, PlayerKind, tile_count,
};
use lands_core::prelude::{Faction, Terrain, TileKind};
use std::collections::BTreeMap;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LevelError {
    #[error("failed to parse level: {0}")]
    Parse(String),

    #[error("failed to write level: {0}")]
    Serialize(String),

    #[error("id must not be empty")]
    MissingId,

    #[error("freq must be within {min}..={max}, found {found}")]
    Frequency { min: u32, max: u32, found: u32 },

    #[error("players must hold {min} to {max} entries, found {found}")]
    PlayerCount {
        min: usize,
        max: usize,
        found: usize,
    },

    #[error("players[{index}].faction is {faction}, which players[{first}] already holds")]
    DuplicateFaction {
        index: usize,
        first: usize,
        faction: Faction,
    },

    #[error("players[{index}].kind is Ai with an empty profile name")]
    EmptyAiProfile { index: usize },

    #[error(
        "overrides[{index}].id is {tile}, which is out of range \
         (freq {freq} has {count} tiles, 0..{count})"
    )]
    OverrideOutOfRange {
        index: usize,
        tile: u32,
        freq: u32,
        count: u32,
    },

    #[error("overrides[{index}].id is {tile}, which overrides[{first}] already set")]
    DuplicateOverride {
        index: usize,
        first: usize,
        tile: u32,
    },

    #[error("overrides[{index}].owner is {faction}, which is not one of the players")]
    UnknownOwner { index: usize, faction: Faction },

    #[error(
        "overrides[{index}].terrain is Water, so its kind must be Empty and its owner None \
         (found kind {kind:?}, owner {owner:?})"
    )]
    WaterNotNeutral {
        index: usize,
        kind: TileKind,
        owner: Option<Faction>,
    },

    #[error("overrides[{index}].kind is Capital but its owner is None; a capital is somebody's")]
    UnownedCapital { index: usize },

    #[error(
        "players[{index}] holds {faction}, which has no capital: no entry in overrides gives \
         {faction} a tile of kind Capital"
    )]
    FactionWithoutCapital { index: usize, faction: Faction },

    #[error("the planet source could not build the freq {freq} planet: {message}")]
    Planet { freq: u32, message: String },

    #[error(
        "the planet source returned {found} {what} for freq {freq}, which has {expected} tiles"
    )]
    PlanetSize {
        what: &'static str,
        freq: u32,
        expected: u32,
        found: usize,
    },
}

impl Level {
    /// Check everything [`Level::build`] and the rest of the game are allowed
    /// to assume.
    ///
    /// Reports the first problem it finds, in file order: id, then freq, then
    /// players, then overrides. An author fixes them one at a time anyway, and
    /// a later check is often only failing because of an earlier one.
    pub fn validate(&self) -> Result<(), LevelError> {
        if self.id.trim().is_empty() {
            return Err(LevelError::MissingId);
        }

        if self.freq < MIN_FREQUENCY || self.freq > MAX_FREQUENCY {
            return Err(LevelError::Frequency {
                min: MIN_FREQUENCY,
                max: MAX_FREQUENCY,
                found: self.freq,
            });
        }

        // Two players is a game; five is impossible, because there are four
        // factions and a faction is a colour on the planet, not a slot.
        if self.players.len() < MIN_PLAYERS || self.players.len() > MAX_PLAYERS {
            return Err(LevelError::PlayerCount {
                min: MIN_PLAYERS,
                max: MAX_PLAYERS,
                found: self.players.len(),
            });
        }

        let mut seats: BTreeMap<Faction, usize> = BTreeMap::new();
        for (index, player) in self.players.iter().enumerate() {
            if let Some(&first) = seats.get(&player.faction) {
                return Err(LevelError::DuplicateFaction {
                    index,
                    first,
                    faction: player.faction,
                });
            }
            seats.insert(player.faction, index);

            if let PlayerKind::Ai { profile } = &player.kind
                && profile.trim().is_empty()
            {
                return Err(LevelError::EmptyAiProfile { index });
            }
        }

        let count = tile_count(self.freq);
        let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
        let mut capitals: BTreeMap<Faction, usize> = BTreeMap::new();

        for (index, tile) in self.overrides.iter().enumerate() {
            if tile.id >= count {
                return Err(LevelError::OverrideOutOfRange {
                    index,
                    tile: tile.id,
                    freq: self.freq,
                    count,
                });
            }
            if let Some(&first) = seen.get(&tile.id) {
                return Err(LevelError::DuplicateOverride {
                    index,
                    first,
                    tile: tile.id,
                });
            }
            seen.insert(tile.id, index);

            if let Some(faction) = tile.owner
                && !seats.contains_key(&faction)
            {
                return Err(LevelError::UnknownOwner { index, faction });
            }

            // Water is never owned, built on, or occupied — the same statement
            // `lands_core::invariants` makes about a live world, made here so
            // the world is never built wrong in the first place.
            if tile.terrain == Terrain::Water
                && (tile.kind != TileKind::Empty || tile.owner.is_some())
            {
                return Err(LevelError::WaterNotNeutral {
                    index,
                    kind: tile.kind,
                    owner: tile.owner,
                });
            }

            if tile.kind == TileKind::Capital {
                match tile.owner {
                    None => return Err(LevelError::UnownedCapital { index }),
                    Some(faction) => {
                        capitals.entry(faction).or_insert(index);
                    }
                }
            }
        }

        // Every faction needs somewhere to start. Terrain is procedural, so a
        // capital can only come from an override; a faction without one would
        // hold no tiles and be eliminated before its first turn.
        for (index, player) in self.players.iter().enumerate() {
            if !capitals.contains_key(&player.faction) {
                return Err(LevelError::FactionWithoutCapital {
                    index,
                    faction: player.faction,
                });
            }
        }

        Ok(())
    }
}
