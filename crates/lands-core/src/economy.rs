//! Per-turn income, upkeep and famine.
//!
//! # Order of operations (ECON-003)
//!
//! Resolved independently for each territory, in ascending id order:
//!
//! 1. **Income.** Every owned tile contributes its unconditional yield.
//!    Towns contribute nothing here — their output depends on wheat.
//! 2. **Units eat.** Wheat, and for knights also gold. If the territory cannot
//!    feed them, units die one at a time until it can (ECON-005).
//! 3. **Towns eat what is left.** Only *whole* towns are fed:
//!    `towns_fed = floor(wheat_remaining / wheat_cost)`, each producing gold.
//!
//! The brief's worked example falls straight out of step 3: three towns with
//! seven wheat remaining feed `floor(7/3) = 2` towns, spending 6 wheat and
//! producing 4 gold.
//!
//! # Why famine is a loop
//!
//! Killing units one at a time and re-checking is what makes "in proportion to
//! the lack of wheat" exact: the territory sheds the *minimum* number of units
//! that restores solvency, no more.

use crate::event::{Event, EventSink};
use crate::ids::{Faction, TerritoryId, UnitId};
use crate::state::World;
use lands_rules::{Ruleset, TileKind};

/// Run income, upkeep and town production for every territory of a faction.
pub fn resolve_faction(world: &mut World, rules: &Ruleset, faction: Faction, sink: &mut EventSink) {
    let ids: Vec<TerritoryId> = world
        .territories
        .values()
        .filter(|t| t.faction == faction)
        .map(|t| t.id)
        .collect();

    for id in ids {
        resolve_territory(world, rules, id, sink);
    }
}

/// One territory's full economic turn.
pub fn resolve_territory(
    world: &mut World,
    rules: &Ruleset,
    id: TerritoryId,
    sink: &mut EventSink,
) {
    let Some(territory) = world.territories.get(&id) else {
        return;
    };
    let faction = territory.faction;

    // ---- 1. Income ------------------------------------------------------
    let mut income_wheat = 0;
    let mut income_gold = 0;
    for &tile in &territory.tiles {
        let kind = world.tiles[tile.index()].kind;
        if kind == TileKind::Town {
            continue; // Conditional; handled in step 3.
        }
        let y = rules.tile_yield(kind);
        income_wheat += y.wheat;
        income_gold += y.gold;
    }

    {
        let t = world.territories.get_mut(&id).expect("checked above");
        t.wheat += income_wheat;
        t.gold += income_gold;
    }
    {
        let s = world.stats_mut(faction);
        s.wheat_earned += income_wheat.max(0) as u64;
        s.gold_earned += income_gold.max(0) as u64;
    }
    sink.push(Event::ResourcesProduced {
        territory: id,
        wheat: income_wheat,
        gold: income_gold,
    });

    // ---- 2. Units eat, and starve if they cannot ------------------------
    loop {
        let (need_wheat, need_gold) = upkeep_of(world, rules, id);
        let t = &world.territories[&id];
        if need_wheat <= t.wheat && need_gold <= t.gold {
            let t = world.territories.get_mut(&id).expect("checked above");
            t.wheat -= need_wheat;
            t.gold -= need_gold;
            break;
        }
        match starvation_victim(world, rules, id) {
            Some(victim) => starve(world, victim, sink),
            // Nothing left to shed. Clamp rather than going negative, which
            // the invariant checker would (rightly) flag.
            None => {
                let t = world.territories.get_mut(&id).expect("checked above");
                t.wheat = t.wheat.max(0);
                t.gold = t.gold.max(0);
                break;
            }
        }
    }

    // ---- 3. Towns eat what is left -------------------------------------
    let towns = world.count_kind_in(id, TileKind::Town);
    if towns > 0 {
        let cost = rules.economy.town.wheat_cost;
        let t = &world.territories[&id];
        let affordable = if cost > 0 {
            t.wheat / cost
        } else {
            towns as i32
        };
        let fed = towns.min(affordable.max(0) as u32);
        let gold_made = fed as i32 * rules.economy.town.gold_yield;

        let t = world.territories.get_mut(&id).expect("checked above");
        t.wheat -= fed as i32 * cost;
        t.gold += gold_made;

        world.stats_mut(faction).gold_earned += gold_made.max(0) as u64;
        sink.push(Event::TownsFed {
            territory: id,
            fed,
            of: towns,
        });
    }
}

/// Total upkeep of every unit currently standing in a territory.
///
/// Units belong to whichever territory they stand in, so walking a unit across
/// a border moves its cost with it.
pub fn upkeep_of(world: &World, rules: &Ruleset, id: TerritoryId) -> (i32, i32) {
    let mut wheat = 0;
    let mut gold = 0;
    for unit_id in world.units_in(id) {
        if let Some(unit) = world.unit(unit_id) {
            let up = rules.unit(unit.kind).upkeep;
            wheat += up.wheat;
            gold += up.gold;
        }
    }
    (wheat, gold)
}

/// ECON-005. Which unit dies next when a territory cannot feed itself.
///
/// The ruleset's `starvation_priority` decides which *kind* goes first — the
/// brief sheds the most expensive units first, so knights die before warriors
/// and warriors before pawns. Within a kind, the oldest unit dies first.
fn starvation_victim(world: &World, rules: &Ruleset, id: TerritoryId) -> Option<UnitId> {
    let present = world.units_in(id);
    for &kind in &rules.units.starvation_priority {
        let oldest = present
            .iter()
            .filter_map(|&u| world.unit(u))
            .filter(|u| u.kind == kind)
            .min_by_key(|u| (u.born, u.id));
        if let Some(unit) = oldest {
            return Some(unit.id);
        }
    }
    None
}

fn starve(world: &mut World, unit_id: UnitId, sink: &mut EventSink) {
    let Some(unit) = world.units.remove(&unit_id) else {
        return;
    };
    world.tile_mut(unit.tile).unit = None;
    world.stats_mut(unit.faction).units_starved += 1;
    sink.push(Event::UnitStarved {
        unit: unit_id,
        kind: unit.kind,
        faction: unit.faction,
        at: unit.tile,
    });
}

/// Whether a territory can afford anything at all this turn.
///
/// Drives the rotating star the brief asks for on a capital that has something
/// worth spending on.
pub fn can_afford_anything(world: &World, rules: &Ruleset, id: TerritoryId) -> bool {
    use crate::command::cost_filters;

    let Some(t) = world.territory(id) else {
        return false;
    };

    let mut has_empty = false;
    let mut has_free_empty = false;
    for &tile in &t.tiles {
        let x = world.tile(tile);
        if x.kind == TileKind::Empty {
            has_empty = true;
            has_free_empty |= x.unit.is_none();
        }
    }
    if !has_empty {
        return false;
    }

    let pawn = rules
        .costs
        .recruit_pawn
        .at(world.count_units_in(id, cost_filters::any_unit));
    let town = rules
        .costs
        .build_town
        .at(world.count_kind_in(id, TileKind::Town));
    let field = rules
        .costs
        .build_field
        .at(world.count_kind_in(id, TileKind::Field));

    // A pawn additionally needs somewhere unoccupied to stand.
    (has_free_empty && t.gold >= pawn) || t.gold >= town.min(field)
}
