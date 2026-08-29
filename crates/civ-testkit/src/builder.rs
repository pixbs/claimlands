//! Fluent construction of hand-made worlds.
//!
//! A test should read as a description of the situation it is testing:
//!
//! ```
//! use civ_testkit::prelude::*;
//!
//! let (world, rules) = WorldBuilder::new(topo::line(5))
//!     .all_land()
//!     .player(Faction::Red)
//!     .player(Faction::Blue)
//!     .own(Faction::Red, &[0, 1, 2])
//!     .capital(2)
//!     .own(Faction::Blue, &[4])
//!     .capital(4)
//!     .build();
//!
//! assert_eq!(world.territories.len(), 2);
//! ```
//!
//! Ownership and capitals are declared as plain data; the builder then runs the
//! real [`territory::retopologize`] to derive territories, so a fixture can
//! never describe a world the game itself could not produce.

use civ_core::event::EventSink;
use civ_core::prelude::*;
use civ_core::state::{Controller, Player, Terrain, Unit};
use civ_core::territory;
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Debug)]
pub struct WorldBuilder {
    seed: u64,
    topology: Arc<Topology>,
    tiles: Vec<Tile>,
    players: Vec<Player>,
    units: Vec<(Faction, UnitKind, TileId)>,
    treasuries: Vec<(TileId, i32, i32)>,
    rules: Ruleset,
    /// The faction most recently named by `own`, so `capital` and `unit` can
    /// be written without repeating it.
    current: Option<Faction>,
}

impl WorldBuilder {
    pub fn new(topology: Topology) -> Self {
        let count = topology.tile_count();
        Self {
            seed: 0xC100_0000_0000_0001,
            topology: Arc::new(topology),
            tiles: vec![Tile::water(); count],
            players: Vec::new(),
            units: Vec::new(),
            treasuries: Vec::new(),
            rules: Ruleset::bundled(),
            current: None,
        }
    }

    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Use a ruleset other than the bundled default.
    pub fn rules(mut self, rules: Ruleset) -> Self {
        self.rules = rules;
        self
    }

    /// Make every tile dry land. Most rule tests do not care about water.
    pub fn all_land(mut self) -> Self {
        for tile in &mut self.tiles {
            *tile = Tile::land(TileKind::Empty);
        }
        self
    }

    /// Make specific tiles land, leaving the rest water.
    pub fn land(mut self, tiles: &[u32]) -> Self {
        for &t in tiles {
            self.tiles[t as usize] = Tile::land(TileKind::Empty);
        }
        self
    }

    pub fn player(self, faction: Faction) -> Self {
        self.controlled_player(faction, Controller::Human)
    }

    pub fn ai_player(self, faction: Faction, profile: &str) -> Self {
        self.controlled_player(faction, Controller::Ai(profile.to_owned()))
    }

    fn controlled_player(mut self, faction: Faction, controller: Controller) -> Self {
        self.players.push(Player {
            faction,
            controller,
            eliminated: false,
        });
        self
    }

    /// Give a faction these tiles, and make them the target of the next
    /// `capital` / `unit` call.
    pub fn own(mut self, faction: Faction, tiles: &[u32]) -> Self {
        for &t in tiles {
            let tile = &mut self.tiles[t as usize];
            if tile.terrain != Terrain::Land {
                *tile = Tile::land(TileKind::Empty);
            }
            tile.owner = Some(faction);
        }
        self.current = Some(faction);
        self
    }

    /// Mark a tile as the capital of the faction named by the last `own`.
    pub fn capital(mut self, tile: u32) -> Self {
        self.tiles[tile as usize].kind = TileKind::Capital;
        self
    }

    /// Set what stands on a tile.
    pub fn kind(mut self, tile: u32, kind: TileKind) -> Self {
        self.tiles[tile as usize].kind = kind;
        self
    }

    pub fn kinds(mut self, tiles: &[u32], kind: TileKind) -> Self {
        for &t in tiles {
            self.tiles[t as usize].kind = kind;
        }
        self
    }

    /// Place a unit for the faction named by the last `own`.
    pub fn unit(mut self, kind: UnitKind, tile: u32) -> Self {
        let faction = self
            .current
            .expect("call `own` before `unit` so the builder knows whose unit it is");
        self.units.push((faction, kind, TileId(tile)));
        self
    }

    /// Place a unit for an explicit faction.
    pub fn unit_of(mut self, faction: Faction, kind: UnitKind, tile: u32) -> Self {
        self.units.push((faction, kind, TileId(tile)));
        self
    }

    /// Override the treasury of whichever territory ends up containing `tile`.
    ///
    /// Applied after territories are derived, because the builder does not know
    /// their ids until then.
    pub fn treasury(mut self, capital_tile: u32, wheat: i32, gold: i32) -> Self {
        self.treasuries.push((TileId(capital_tile), wheat, gold));
        self
    }

    /// Produce the world and the ruleset it was built against.
    pub fn build(self) -> (World, Ruleset) {
        let mut world = World::empty(self.seed, self.topology);
        world.tiles = self.tiles;
        world.players = self.players;

        // Derive territories using the real implementation, so a fixture can
        // only ever describe a legal world.
        let mut sink = EventSink::new();
        let factions: BTreeSet<Faction> = world.tiles.iter().filter_map(|t| t.owner).collect();
        for faction in factions {
            territory::retopologize(&mut world, &self.rules, faction, None, &mut sink);
        }

        // ECON-020. `retopologize` only ever redistributes an existing
        // treasury, so a territory that exists from the very start has nothing
        // to inherit and must be endowed here.
        let ids: Vec<TerritoryId> = world.territories.keys().copied().collect();
        for id in ids {
            let t = world.territory_mut(id).expect("just enumerated");
            t.wheat = self.rules.economy.starting_wheat;
            t.gold = self.rules.economy.starting_gold;
        }

        for (faction, kind, tile) in self.units {
            let id = world.alloc_unit_id();
            let born = world.alloc_born();
            world.units.insert(
                id,
                Unit {
                    id,
                    kind,
                    faction,
                    tile,
                    born,
                    moved: false,
                },
            );
            world.tile_mut(tile).unit = Some(id);
        }

        for (capital_tile, wheat, gold) in self.treasuries {
            let id = world
                .tile(capital_tile)
                .territory
                .expect("treasury() needs a tile inside a territory");
            let t = world.territory_mut(id).expect("territory just resolved");
            t.wheat = wheat;
            t.gold = gold;
        }

        (world, self.rules)
    }

    /// Build and immediately start a session, which runs the first turn's
    /// income. Most rule tests want this.
    pub fn session(self) -> Session {
        let (world, rules) = self.build();
        Session::start(world, rules)
    }
}
