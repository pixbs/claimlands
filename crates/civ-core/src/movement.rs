//! Where a unit may go, and what it may do when it gets there.
//!
//! # The move budget (UNIT-030, UNIT-031)
//!
//! One action per unit per turn. A unit may travel up to
//! `own_territory_steps` hops through tiles its own faction owns, and may then
//! take one further step into a tile the faction does not own, which captures
//! it. The two parts compose into a single action (see
//! UNIT-031).
//!
//! Units block each other: a path may not pass *through* any occupied tile,
//! friendly or hostile. Only the final tile is contested.
//!
//! [`reachable`] is the single source of truth. The HUD calls it to highlight
//! legal destinations and [`crate::apply`] calls it to validate, so what the
//! player is shown and what the rules permit cannot drift apart.

use crate::command::Rejection;
use crate::ids::{TileId, UnitId};
use crate::state::World;
use civ_rules::Ruleset;
use std::collections::BTreeSet;

/// Every tile the unit may legally move to this turn.
///
/// Empty when the unit has already acted, or when it is hemmed in.
pub fn reachable(world: &World, rules: &Ruleset, unit_id: UnitId) -> BTreeSet<TileId> {
    let Some(unit) = world.unit(unit_id) else {
        return BTreeSet::new();
    };
    if unit.moved {
        return BTreeSet::new();
    }

    let topo = &world.topology;
    let faction = unit.faction;
    let origin = unit.tile;

    // Interior movement: own land, and not blocked by another unit. The unit's
    // own tile is passable so the BFS can leave it.
    let passable = |t: TileId| {
        let tile = world.tile(t);
        tile.is_land() && tile.owner == Some(faction) && (tile.unit.is_none() || t == origin)
    };

    let dist = topo.distances_from(origin, passable);
    let budget = rules.units.own_territory_steps;

    let interior: Vec<TileId> = topo
        .tiles()
        .filter(|t| dist[t.index()].is_some_and(|d| d <= budget))
        .collect();

    let mut out = BTreeSet::new();

    // Repositioning inside our own borders.
    for &tile in &interior {
        if tile != origin && world.tile(tile).unit.is_none() {
            out.insert(tile);
        }
    }

    // One step outward, from anywhere we could have reached.
    if rules.units.foreign_steps >= 1 {
        for &tile in &interior {
            for &neighbor in topo.neighbors(tile) {
                if world.tile(neighbor).owner != Some(faction)
                    && can_enter(world, rules, unit_id, neighbor).is_ok()
                {
                    out.insert(neighbor);
                }
            }
        }
    }

    out
}

/// Whether a unit is permitted to end its move on `target`, ignoring distance.
///
/// Splitting this out from [`reachable`] means the HUD can explain *why* a tap
/// was refused ("a pawn cannot take a capital") rather than silently doing
/// nothing.
pub fn can_enter(
    world: &World,
    rules: &Ruleset,
    unit_id: UnitId,
    target: TileId,
) -> Result<(), Rejection> {
    let Some(unit) = world.unit(unit_id) else {
        return Err(Rejection::NoSuchUnit(unit_id));
    };
    let tile = world.tile(target);

    if !tile.is_land() {
        return Err(Rejection::NotLand(target));
    }

    // An occupied tile: either an ally in the way, or a fight.
    if let Some(other_id) = tile.unit {
        let other = world
            .unit(other_id)
            .expect("a tile's unit reference is always live");
        if other.faction == unit.faction {
            return Err(Rejection::BlockedByAlly(target));
        }
        if !rules.unit(unit.kind).defeats.contains(&other.kind) {
            return Err(Rejection::CannotDefeatUnit {
                mover: unit.kind,
                target: other.kind,
            });
        }
    }

    // Moving inside our own borders needs no capture permission.
    if tile.owner == Some(unit.faction) {
        return Ok(());
    }

    // UNIT-010: taking someone else's ground, or neutral ground.
    if !rules.unit(unit.kind).captures.contains(&tile.kind) {
        return Err(Rejection::CannotCaptureTile {
            mover: unit.kind,
            target: tile.kind,
        });
    }

    Ok(())
}

/// Whether the unit has any legal action at all this turn.
///
/// Drives the rotating star the brief asks for above units that can still act.
pub fn has_available_move(world: &World, rules: &Ruleset, unit_id: UnitId) -> bool {
    !reachable(world, rules, unit_id).is_empty()
}
