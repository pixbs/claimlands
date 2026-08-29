//! Forest spread (GROW-001, GROW-002).
//!
//! Once per round, every forest tile has a chance to seed a neighbouring empty
//! tile. Forests never appear on fields, towns or capitals — only on empty
//! ground — so a developed territory is naturally immune, while neglected land
//! slowly reverts.
//!
//! Forests are unowned ground as far as growth is concerned: a forest will
//! happily take an *owned* empty tile, which is a real strategic pressure
//! (empty tiles yield wheat, forests yield nothing) and is why players are
//! pushed to develop rather than merely hold.
//!
//! # Determinism
//!
//! The candidate list is snapshotted before the loop, so a tile that becomes a
//! forest this round cannot spread again in the same round. Each source tile
//! draws from its own stream keyed on `(round, tile)`, so the outcome does not
//! depend on iteration order and adding a new random consumer elsewhere in the
//! game cannot shift it.

use crate::event::{Event, EventSink};
use crate::ids::TileId;
use crate::rng::{SeedDomain, stream};
use crate::state::World;
use lands_rules::{Ruleset, TileKind};

/// Advance forest growth by one round.
pub fn spread_forests(world: &mut World, rules: &Ruleset, sink: &mut EventSink) {
    if rules.growth.forest_spread_percent == 0 {
        return;
    }

    let sources: Vec<TileId> = world
        .tile_ids()
        .filter(|&t| {
            let tile = world.tile(t);
            tile.is_land() && tile.kind == TileKind::Forest
        })
        .collect();

    let topo = world.topology.clone();

    for source in sources {
        let mut rng = stream(world.seed, SeedDomain::ForestSpread, world.round, source.0);
        if !rng.chance_percent(rules.growth.forest_spread_percent) {
            continue;
        }

        let targets: Vec<TileId> = topo
            .neighbors(source)
            .iter()
            .copied()
            .filter(|&n| {
                let tile = world.tile(n);
                tile.is_land() && rules.growth.forest_spread_targets.contains(&tile.kind)
            })
            .collect();

        let Some(&target) = rng.pick(&targets) else {
            continue;
        };
        world.tile_mut(target).kind = TileKind::Forest;
        sink.push(Event::ForestSpread {
            from: source,
            to: target,
        });
    }
}
