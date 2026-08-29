//! A fingerprint of the entire world.
//!
//! This is the assertion at the end of every golden replay test: run a command
//! log, hash the result, compare against the committed value. If any rule
//! changes behaviour — even one nobody thought the change would touch — the
//! hash moves and CI says so.
//!
//! The encoding is written by hand rather than derived from `serde` on
//! purpose. A serialisation format is allowed to change how it renders a
//! struct between versions; this must not, or every stored golden value would
//! break at once for no real reason.
//!
//! Only *simulation-visible* state is hashed. Statistics are included (they are
//! part of the outcome the player sees) but the topology is not, since it is
//! fixed for the whole match and covered by the worldgen snapshot tests.

use crate::state::{Outcome, Terrain, VictoryReason, World};
use civ_rules::{TileKind, UnitKind};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Streaming FNV-1a writer.
struct Digest(u64);

impl Digest {
    fn new() -> Self {
        Self(FNV_OFFSET)
    }

    fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.byte(b);
        }
    }

    fn i32(&mut self, v: i32) {
        self.u32(v as u32);
    }

    fn u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.byte(b);
        }
    }

    /// Encode an optional id as a sentinel-prefixed value, so `None` and
    /// `Some(0)` cannot collide.
    fn opt_u32(&mut self, v: Option<u32>) {
        match v {
            None => self.byte(0),
            Some(x) => {
                self.byte(1);
                self.u32(x);
            }
        }
    }
}

fn tile_kind_code(k: TileKind) -> u8 {
    match k {
        TileKind::Empty => 0,
        TileKind::Capital => 1,
        TileKind::Town => 2,
        TileKind::Field => 3,
        TileKind::Forest => 4,
    }
}

fn unit_kind_code(k: UnitKind) -> u8 {
    match k {
        UnitKind::Pawn => 0,
        UnitKind::Warrior => 1,
        UnitKind::Knight => 2,
    }
}

/// Hash every piece of simulation state that a rule could affect.
pub fn world_hash(world: &World) -> u64 {
    let mut d = Digest::new();

    d.u64(world.seed);
    d.u32(world.round);
    d.u32(world.turn_index as u32);

    match world.outcome {
        None => d.byte(0),
        Some(Outcome::Draw { round }) => {
            d.byte(1);
            d.u32(round);
        }
        Some(Outcome::Victory {
            faction,
            reason,
            round,
        }) => {
            d.byte(2);
            d.u32(faction.ordinal());
            d.byte(match reason {
                VictoryReason::Elimination => 0,
                VictoryReason::Dominance => 1,
            });
            d.u32(round);
        }
    }

    // Tiles, in index order.
    for tile in &world.tiles {
        d.byte(match tile.terrain {
            Terrain::Water => 0,
            Terrain::Land => 1,
        });
        d.byte(tile_kind_code(tile.kind));
        d.opt_u32(tile.owner.map(|f| f.ordinal()));
        d.opt_u32(tile.territory.map(|t| t.0));
        d.opt_u32(tile.unit.map(|u| u.0));
    }

    // Units and territories, in id order (BTreeMap iterates ascending).
    for (id, unit) in &world.units {
        d.u32(id.0);
        d.byte(unit_kind_code(unit.kind));
        d.u32(unit.faction.ordinal());
        d.u32(unit.tile.0);
        d.u32(unit.born);
        d.byte(unit.moved as u8);
    }

    for (id, t) in &world.territories {
        d.u32(id.0);
        d.u32(t.faction.ordinal());
        d.u32(t.capital.0);
        d.i32(t.wheat);
        d.i32(t.gold);
        d.u32(t.tiles.len() as u32);
        for tile in &t.tiles {
            d.u32(tile.0);
        }
    }

    for player in &world.players {
        d.u32(player.faction.ordinal());
        d.byte(player.eliminated as u8);
    }

    for (faction, s) in &world.stats {
        d.u32(faction.ordinal());
        for v in [
            s.units_recruited,
            s.units_upgraded,
            s.units_lost,
            s.units_killed,
            s.units_starved,
            s.tiles_captured,
            s.tiles_lost,
            s.towns_built,
            s.fields_built,
            s.capitals_razed,
            s.peak_tiles,
            s.peak_territories,
        ] {
            d.u32(v);
        }
        for v in [s.gold_earned, s.wheat_earned, s.gold_looted] {
            d.u64(v);
        }
    }

    d.0
}
