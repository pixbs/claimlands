//! The turn pipeline.
//!
//! # Structure
//!
//! A **round** is one turn for every surviving player. A **turn** belongs to a
//! player, not to a territory: a player with four territories plays all of
//! them within their single turn, exactly as the brief specifies.
//!
//! ```text
//! round start ──▶ forest growth (once, for the whole planet)
//!      │
//!      ├─▶ player turn ──▶ reset moves ──▶ income ──▶ upkeep ──▶ towns ──▶ commands
//!      ├─▶ player turn ──▶ ...
//!      └─▶ round end   ──▶ eliminations ──▶ dominance check
//! ```
//!
//! # Adding a phase
//!
//! New systems (a science tree, weather, disease) are added as a new call in
//! [`begin_turn`] or [`end_turn`], not by editing an existing phase. Give the
//! new system its own [`SeedDomain`](crate::rng::SeedDomain) and it cannot
//! disturb the randomness of anything that already exists — which is what
//! keeps the golden replay corpus valid across feature work.

use crate::economy;
use crate::event::{Event, EventSink};
use crate::growth;
use crate::state::World;
use crate::victory;
use civ_rules::Ruleset;

/// Put the match into its first turn. Call once, after the level is loaded.
pub fn start_match(world: &mut World, rules: &Ruleset, sink: &mut EventSink) {
    world.round = 1;
    world.turn_index = world
        .players
        .iter()
        .position(|p| !p.eliminated)
        .unwrap_or(0);

    sink.push(Event::RoundBegan { round: world.round });
    begin_turn(world, rules, sink);
}

/// Hand control to the next surviving player, advancing the round if the
/// turn order wraps.
pub fn end_turn(world: &mut World, rules: &Ruleset, sink: &mut EventSink) {
    let Some(faction) = world.current_faction() else {
        return;
    };
    sink.push(Event::TurnEnded { faction });

    // A player may have been knocked out by the turn that just finished.
    victory::evaluate(world, rules, false, sink);
    if world.is_over() {
        return;
    }

    let Some((next, wrapped)) = next_player(world) else {
        // No one is left standing; let victory resolve it as a draw.
        victory::evaluate(world, rules, true, sink);
        return;
    };
    world.turn_index = next;

    if wrapped {
        world.round += 1;
        victory::evaluate(world, rules, true, sink);
        if world.is_over() {
            return;
        }
        sink.push(Event::RoundBegan { round: world.round });
        growth::spread_forests(world, rules, sink);
    }

    begin_turn(world, rules, sink);
}

/// Start the active player's turn: refresh their units, then run their economy.
///
/// Income is collected at the *start* of the owner's turn rather than at the
/// end, so that what the HUD shows as affordable is what the player can
/// actually spend right now.
fn begin_turn(world: &mut World, rules: &Ruleset, sink: &mut EventSink) {
    let Some(faction) = world.current_faction() else {
        return;
    };
    sink.push(Event::TurnBegan {
        faction,
        round: world.round,
    });

    for unit in world.units.values_mut() {
        if unit.faction == faction {
            unit.moved = false;
        }
    }

    economy::resolve_faction(world, rules, faction, sink);
    record_peaks(world, faction);
}

/// Index of the next player who is still in the game, and whether the search
/// wrapped past the end of the turn order.
fn next_player(world: &World) -> Option<(usize, bool)> {
    let count = world.players.len();
    if count == 0 {
        return None;
    }
    for step in 1..=count {
        let index = (world.turn_index + step) % count;
        if !world.players[index].eliminated {
            return Some((index, world.turn_index + step >= count));
        }
    }
    None
}

/// Track high-water marks for the victory screen.
fn record_peaks(world: &mut World, faction: crate::ids::Faction) {
    let tiles = world.tile_count_of(faction);
    let territories = world.territories_of(faction).count() as u32;
    let stats = world.stats_mut(faction);
    stats.peak_tiles = stats.peak_tiles.max(tiles);
    stats.peak_territories = stats.peak_territories.max(territories);
}
