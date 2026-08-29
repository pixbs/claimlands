//! Typed identifiers.
//!
//! These are newtypes rather than bare `u32` so that a tile id can never be
//! passed where a unit id was meant. Every one derives `Ord` because the
//! simulation must iterate collections in a fixed order on every machine
//! (see docs/determinism.md).

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            #[inline]
            pub const fn index(self) -> usize {
                self.0 as usize
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

id_type! {
    /// Index of a tile on the planet. Dense: `0..topology.tile_count()`.
    TileId
}

id_type! {
    /// A connected, singly-capitalised block of tiles owned by one faction.
    ///
    /// Territories are created and destroyed constantly as borders shift, so
    /// ids are never reused within a game.
    TerritoryId
}

id_type! {
    /// A unit. Ids are never reused, so a dead unit's id stays dangling-safe.
    UnitId
}

/// The four playable colours.
///
/// Fixed at four by the game design; a wider set would change the level format
/// and the HUD, so it is deliberately not a `u8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Faction {
    Red,
    Yellow,
    Green,
    Blue,
}

impl Faction {
    pub const ALL: [Faction; 4] = [Faction::Red, Faction::Yellow, Faction::Green, Faction::Blue];

    /// Stable small integer, used for seeding and for the wire format.
    pub const fn ordinal(self) -> u32 {
        match self {
            Faction::Red => 0,
            Faction::Yellow => 1,
            Faction::Green => 2,
            Faction::Blue => 3,
        }
    }
}

impl core::fmt::Display for Faction {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Faction::Red => "Red",
            Faction::Yellow => "Yellow",
            Faction::Green => "Green",
            Faction::Blue => "Blue",
        };
        f.write_str(s)
    }
}
