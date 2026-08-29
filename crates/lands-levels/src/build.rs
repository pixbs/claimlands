//! Turning a [`Level`] into a [`World`] that `Session::start` can play.
//!
//! # Why the planet is passed in
//!
//! A level names a `freq` and a `seed`; the sphere and the terrain those
//! describe are `lands-worldgen`'s work. That crate sits on the **same layer**
//! as this one, so neither may depend on the other (`cargo xtask check-deps`,
//! docs/architecture.md §3) — and that is the right shape rather than an
//! obstacle. Assembling a world out of a planet is a different job from
//! growing the planet, the two change for different reasons, and keeping them
//! apart means a level can be loaded onto a hand-built topology in a test with
//! no mesh anywhere near it.
//!
//! So the caller — `lands-app`, `lands-cli`, the level editor, all of which sit
//! above both crates — supplies a [`PlanetSource`], and this module owns
//! everything after that: terrain, overrides, players, territories, treasuries.

use crate::format::{Level, PlayerKind, tile_count};
use crate::validate::LevelError;
use lands_core::event::EventSink;
use lands_core::prelude::{Faction, Terrain, Tile, TileKind, Topology, World};
use lands_core::state::{Controller, Player};
use lands_core::territory;
use lands_rules::Ruleset;
use std::collections::BTreeSet;
use std::sync::Arc;

/// Where the planet named by a level's `freq` and `seed` comes from.
///
/// One implementation wraps `lands-worldgen` and is what ships; tests
/// implement it with a topology small enough to draw on paper.
pub trait PlanetSource {
    /// Tile adjacency of a frequency-`freq` planet, which must have exactly
    /// `10·freq²+2` tiles.
    ///
    /// The shape of the sphere does not depend on the seed — only what grows
    /// on it does — so an ocean world needs this and nothing else.
    fn topology(&self, freq: u32) -> Result<Topology, String>;

    /// Which of those tiles are land, in tile order.
    ///
    /// Never called with `seed == 0`: that is the authoring seed and its ocean
    /// is made here, so an implementation may treat 0 as unreachable.
    fn terrain(&self, freq: u32, seed: u64) -> Result<Vec<Terrain>, String>;
}

impl Level {
    /// Build the world this level describes.
    ///
    /// The result is ready for `Session::start`: terrain, players, capitals,
    /// territories and starting treasuries are all in place.
    ///
    /// Deterministic given the same level, planet source and ruleset — the
    /// world is assembled in tile order and territories are derived per
    /// faction in [`Faction`] order, so loading twice produces identical
    /// bytes.
    pub fn build<P: PlanetSource + ?Sized>(
        &self,
        planet: &P,
        rules: &Ruleset,
    ) -> Result<World, LevelError> {
        self.validate()?;

        let freq = self.freq;
        let expected = tile_count(freq);
        let unavailable = |message| LevelError::Planet { freq, message };
        // A source that miscounts would renumber every override, so the size
        // is checked rather than trusted.
        let wrong_size = |what, found| LevelError::PlanetSize {
            what,
            freq,
            expected,
            found,
        };

        let topology = planet.topology(freq).map_err(unavailable)?;
        if topology.tile_count() != expected as usize {
            return Err(wrong_size("tiles", topology.tile_count()));
        }

        let mut world = World::empty(self.seed, Arc::new(topology));

        // Seed 0 is the authoring seed: no terrain is grown at all, and the
        // author raises the land they want with overrides.
        if !self.is_ocean() {
            let terrain = planet.terrain(freq, self.seed).map_err(unavailable)?;
            if terrain.len() != expected as usize {
                return Err(wrong_size("terrain entries", terrain.len()));
            }
            for (tile, &terrain) in world.tiles.iter_mut().zip(terrain.iter()) {
                if terrain == Terrain::Land {
                    *tile = Tile::land(TileKind::Empty);
                }
            }
        }

        // The sparse part: whatever the author changed wins over the seed.
        for entry in &self.overrides {
            let tile = &mut world.tiles[entry.id as usize];
            *tile = match entry.terrain {
                Terrain::Water => Tile::water(),
                Terrain::Land => Tile::land(entry.kind),
            };
            tile.owner = entry.owner;
        }

        world.players = self
            .players
            .iter()
            .map(|p| Player {
                faction: p.faction,
                controller: match &p.kind {
                    PlayerKind::Human => Controller::Human,
                    PlayerKind::Ai { profile } => Controller::Ai(profile.clone()),
                },
                eliminated: false,
            })
            .collect();

        // Derive territories with the real implementation, so a level can only
        // ever describe a world the game itself could have produced.
        let mut sink = EventSink::new();
        let factions: BTreeSet<Faction> = world.tiles.iter().filter_map(|t| t.owner).collect();
        for faction in factions {
            territory::retopologize(&mut world, rules, faction, None, &mut sink);
        }

        // ECON-020. `retopologize` only ever redistributes a treasury that
        // already exists, so a territory present from the first turn has
        // nothing to inherit and is endowed from the ruleset here.
        let ids: Vec<_> = world.territories.keys().copied().collect();
        for id in ids {
            let t = world.territory_mut(id).expect("just enumerated");
            t.wheat = rules.economy.starting_wheat;
            t.gold = rules.economy.starting_gold;
        }

        Ok(world)
    }
}
