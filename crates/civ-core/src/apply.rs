//! Validating and applying commands.
//!
//! [`validate`] never mutates, so the HUD can ask "may I?" to grey out an
//! illegal action, and [`apply`] can trust that the command is legal by the
//! time it starts changing state. Every mutation in the game passes through
//! here — there is no other write path into [`World`].
//!
//! # Capture ordering
//!
//! The one genuinely delicate sequence is capturing a capital, because the
//! loot has to move *before* the victim's territory splits and *before* the
//! captor's territories merge:
//!
//! 1. Take the loot from the victim territory and give it to the captor's.
//! 2. Flip ownership and raze whatever stood on the tile.
//! 3. Move the unit.
//! 4. Recompute the victim's territories — which may split them.
//! 5. Recompute the captor's — which may merge them, summing treasuries and
//!    so preserving the loot regardless of which territory it landed in.
//!
//! Doing steps 4 and 5 in the other order would let a merge consume a
//! territory that the split was about to divide.

use crate::command::{Command, Rejection, cost_filters};
use crate::event::{Event, EventSink};
use crate::ids::{Faction, TerritoryId, TileId, UnitId};
use crate::movement;
use crate::state::{Unit, World};
use crate::territory;
use crate::turn;
use civ_rules::{Ruleset, TileKind, UnitKind};

/// Check a command without changing anything.
pub fn validate(world: &World, rules: &Ruleset, cmd: &Command) -> Result<(), Rejection> {
    if world.is_over() {
        return Err(Rejection::GameOver);
    }
    let active = world.current_faction().ok_or(Rejection::NoActivePlayer)?;

    match cmd {
        Command::EndTurn => Ok(()),

        Command::MoveUnit { unit, to } => {
            let u = own_unit(world, *unit, active)?;
            if u.moved {
                return Err(Rejection::AlreadyMoved(*unit));
            }
            tile_in_range(world, *to)?;
            // `can_enter` first, so the player is told *why* rather than just
            // "unreachable" when the destination is legal but too far.
            movement::can_enter(world, rules, *unit, *to)?;
            if !movement::reachable(world, rules, *unit).contains(to) {
                return Err(Rejection::Unreachable {
                    from: u.tile,
                    to: *to,
                });
            }
            Ok(())
        }

        Command::UpgradeUnit { unit } => {
            let u = own_unit(world, *unit, active)?;
            if u.moved {
                return Err(Rejection::AlreadyMoved(*unit));
            }
            let (_, cost, territory) =
                upgrade_plan(world, rules, *unit).ok_or(Rejection::NoUpgradeAvailable(u.kind))?;
            afford(world, territory, cost)
        }

        Command::RecruitUnit { territory, at } => {
            let t = own_territory(world, *territory, active)?;
            if !t.tiles.contains(at) {
                return Err(Rejection::NotInTerritory {
                    tile: *at,
                    territory: *territory,
                });
            }
            if world.tile(*at).unit.is_some() {
                return Err(Rejection::TileOccupied(*at));
            }
            afford(world, *territory, recruit_cost(world, rules, *territory))
        }

        Command::BuildTown { territory, at } => {
            validate_build(world, *territory, *at, active)?;
            afford(world, *territory, build_town_cost(world, rules, *territory))
        }

        Command::BuildField { territory, at } => {
            validate_build(world, *territory, *at, active)?;
            afford(
                world,
                *territory,
                build_field_cost(world, rules, *territory),
            )
        }
    }
}

/// Validate, then apply, returning everything that happened.
pub fn apply(world: &mut World, rules: &Ruleset, cmd: &Command) -> Result<Vec<Event>, Rejection> {
    validate(world, rules, cmd)?;
    let mut sink = EventSink::new();

    match cmd {
        Command::EndTurn => turn::end_turn(world, rules, &mut sink),
        Command::MoveUnit { unit, to } => move_unit(world, rules, *unit, *to, &mut sink),
        Command::UpgradeUnit { unit } => upgrade_unit(world, rules, *unit, &mut sink),
        Command::RecruitUnit { territory, at } => recruit(world, rules, *territory, *at, &mut sink),
        Command::BuildTown { territory, at } => {
            build(world, rules, *territory, *at, TileKind::Town, &mut sink)
        }
        Command::BuildField { territory, at } => {
            build(world, rules, *territory, *at, TileKind::Field, &mut sink)
        }
    }

    Ok(sink.into_events())
}

/// What a command would cost in gold, for previewing prices in the HUD.
///
/// `None` for commands that cost nothing.
pub fn cost_of(world: &World, rules: &Ruleset, cmd: &Command) -> Option<i32> {
    match cmd {
        Command::RecruitUnit { territory, .. } => Some(recruit_cost(world, rules, *territory)),
        Command::BuildTown { territory, .. } => Some(build_town_cost(world, rules, *territory)),
        Command::BuildField { territory, .. } => Some(build_field_cost(world, rules, *territory)),
        Command::UpgradeUnit { unit } => upgrade_plan(world, rules, *unit).map(|(_, c, _)| c),
        Command::MoveUnit { .. } | Command::EndTurn => None,
    }
}

/// Every command the active player could legally issue right now.
///
/// Used by the AI (a brain picks from this list, so it physically cannot cheat),
/// by the CLI fuzzer, and by property tests. The order is deterministic, so a
/// seeded chooser replays identically.
///
/// This is deliberately exhaustive rather than clever: at MVP board sizes it is
/// a few thousand entries and costs microseconds, and being exhaustive means a
/// brain can never miss a legal option that a human would spot.
pub fn legal_commands(world: &World, rules: &Ruleset) -> Vec<Command> {
    let mut out = Vec::new();
    let Some(active) = world.current_faction() else {
        return out;
    };
    if world.is_over() {
        return out;
    }

    // Units, in id order.
    for (&unit, u) in &world.units {
        if u.faction != active || u.moved {
            continue;
        }
        for to in movement::reachable(world, rules, unit) {
            out.push(Command::MoveUnit { unit, to });
        }
        if let Some((_, cost, territory)) = upgrade_plan(world, rules, unit)
            && afford(world, territory, cost).is_ok()
        {
            out.push(Command::UpgradeUnit { unit });
        }
    }

    // Territories, in id order.
    let territories: Vec<TerritoryId> = world
        .territories
        .values()
        .filter(|t| t.faction == active)
        .map(|t| t.id)
        .collect();

    for territory in territories {
        let recruit = recruit_cost(world, rules, territory);
        let town = build_town_cost(world, rules, territory);
        let field = build_field_cost(world, rules, territory);
        let gold = world.territory(territory).map_or(0, |t| t.gold);
        let tiles: Vec<TileId> = world
            .territory(territory)
            .map(|t| t.tiles.iter().copied().collect())
            .unwrap_or_default();

        for at in tiles {
            let tile = world.tile(at);
            if gold >= recruit && tile.unit.is_none() {
                out.push(Command::RecruitUnit { territory, at });
            }
            if tile.kind == TileKind::Empty {
                if gold >= town {
                    out.push(Command::BuildTown { territory, at });
                }
                if gold >= field {
                    out.push(Command::BuildField { territory, at });
                }
            }
        }
    }

    out.push(Command::EndTurn);
    out
}

// ---------------------------------------------------------------------------
// Costs. Each is `base + per_existing * count`, counted within the territory.
// ---------------------------------------------------------------------------

/// ECON-010. Counts units of every kind.
pub fn recruit_cost(world: &World, rules: &Ruleset, territory: TerritoryId) -> i32 {
    rules
        .costs
        .recruit_pawn
        .at(world.count_units_in(territory, cost_filters::any_unit))
}

/// ECON-013.
pub fn build_town_cost(world: &World, rules: &Ruleset, territory: TerritoryId) -> i32 {
    rules
        .costs
        .build_town
        .at(world.count_kind_in(territory, TileKind::Town))
}

/// ECON-014.
pub fn build_field_cost(world: &World, rules: &Ruleset, territory: TerritoryId) -> i32 {
    rules
        .costs
        .build_field
        .at(world.count_kind_in(territory, TileKind::Field))
}

/// What upgrading this unit would produce, cost, and which treasury pays.
///
/// `None` when the unit is already at the top of its chain or stands outside
/// any territory.
pub fn upgrade_plan(
    world: &World,
    rules: &Ruleset,
    unit: UnitId,
) -> Option<(UnitKind, i32, TerritoryId)> {
    let u = world.unit(unit)?;
    let next = rules.unit(u.kind).upgrades_to?;
    let territory = world.tile(u.tile).territory?;

    // ECON-011 counts warriors and knights; ECON-012 counts knights only. The
    // filter is chosen by what the unit is becoming.
    let cost = match next {
        UnitKind::Knight => rules
            .costs
            .upgrade_knight
            .at(world.count_units_in(territory, cost_filters::knight_only)),
        _ => rules
            .costs
            .upgrade_warrior
            .at(world.count_units_in(territory, cost_filters::warrior_or_knight)),
    };
    Some((next, cost, territory))
}

// ---------------------------------------------------------------------------
// Application
// ---------------------------------------------------------------------------

fn move_unit(
    world: &mut World,
    rules: &Ruleset,
    unit_id: UnitId,
    to: TileId,
    sink: &mut EventSink,
) {
    let unit = world.units[&unit_id].clone();
    let from = unit.tile;
    let faction = unit.faction;
    let captured = world.tile(to).owner != Some(faction);

    let victim_faction = world.tile(to).owner;
    let victim_territory = world.tile(to).territory;
    let captor_territory = world.tile(from).territory;

    if captured {
        // 1. Defender, if any, is destroyed. `can_enter` has already proved
        //    this unit is allowed to beat it.
        if let Some(defender_id) = world.tile(to).unit {
            if let Some(defender) = world.units.remove(&defender_id) {
                world.stats_mut(defender.faction).units_lost += 1;
                world.stats_mut(faction).units_killed += 1;
                sink.push(Event::UnitKilled {
                    unit: defender_id,
                    kind: defender.kind,
                    faction: defender.faction,
                    at: to,
                    by: unit_id,
                });
            }
            world.tile_mut(to).unit = None;
        }

        let previous_kind = world.tile(to).kind;

        // 2. TERR-020: sacking a capital transfers a share of its treasury.
        //    This must happen while the victim territory is still whole.
        if previous_kind == TileKind::Capital {
            let looted = victim_territory
                .and_then(|id| world.territory(id))
                .map(|t| {
                    let pct = rules.territory.capital_loot_percent as i64;
                    ((t.gold.max(0) as i64 * pct) / 100) as i32
                })
                .unwrap_or(0);

            if let Some(id) = victim_territory
                && let Some(t) = world.territory_mut(id)
            {
                t.gold -= looted;
            }
            if let Some(id) = captor_territory
                && let Some(t) = world.territory_mut(id)
            {
                t.gold += looted;
            }
            world.stats_mut(faction).gold_looted += looted.max(0) as u64;
            world.stats_mut(faction).capitals_razed += 1;
            sink.push(Event::CapitalRazed {
                tile: to,
                faction: victim_faction.unwrap_or(faction),
                gold_looted: looted,
            });
        }

        // 3. Whatever stood there is razed; the ground changes hands.
        let tile = world.tile_mut(to);
        tile.kind = TileKind::Empty;
        tile.owner = Some(faction);
        tile.territory = None;

        world.stats_mut(faction).tiles_captured += 1;
        if let Some(v) = victim_faction {
            world.stats_mut(v).tiles_lost += 1;
        }
        sink.push(Event::TileCaptured {
            tile: to,
            from: victim_faction,
            to: faction,
            previous_kind,
        });
    }

    // 4. The unit itself moves.
    world.tile_mut(from).unit = None;
    world.tile_mut(to).unit = Some(unit_id);
    if let Some(u) = world.unit_mut(unit_id) {
        u.tile = to;
        u.moved = true;
    }
    sink.push(Event::UnitMoved {
        unit: unit_id,
        from,
        to,
    });

    // 5. Borders moved, so both sides' territories are recomputed. Victim
    //    first: a merge on the captor's side must not run before the split.
    if captured {
        if let Some(v) = victim_faction {
            territory::retopologize(world, rules, v, None, sink);
        }
        territory::retopologize(world, rules, faction, Some(to), sink);
    }
}

fn upgrade_unit(world: &mut World, rules: &Ruleset, unit_id: UnitId, sink: &mut EventSink) {
    let Some((next, cost, territory)) = upgrade_plan(world, rules, unit_id) else {
        return;
    };
    if let Some(t) = world.territory_mut(territory) {
        t.gold -= cost;
    }

    let (from_kind, at, faction) = {
        let u = world.unit_mut(unit_id).expect("validated");
        let previous = u.kind;
        // `born` is deliberately not refreshed: upgrading must not make a unit
        // younger, or players could dodge famine by promoting (ECON-005).
        u.kind = next;
        u.moved = true;
        (previous, u.tile, u.faction)
    };

    world.stats_mut(faction).units_upgraded += 1;
    sink.push(Event::UnitUpgraded {
        unit: unit_id,
        from: from_kind,
        to: next,
        at,
        cost,
    });
}

fn recruit(
    world: &mut World,
    rules: &Ruleset,
    territory: TerritoryId,
    at: TileId,
    sink: &mut EventSink,
) {
    let cost = recruit_cost(world, rules, territory);
    let faction = world.territories[&territory].faction;

    if let Some(t) = world.territory_mut(territory) {
        t.gold -= cost;
    }

    let id = world.alloc_unit_id();
    let born = world.alloc_born();
    world.units.insert(
        id,
        Unit {
            id,
            kind: UnitKind::Pawn,
            faction,
            tile: at,
            born,
            // A fresh recruit cannot act on the turn it is bought, so gold
            // cannot be converted straight into a capture (open question Q5).
            moved: true,
        },
    );
    world.tile_mut(at).unit = Some(id);
    world.stats_mut(faction).units_recruited += 1;

    sink.push(Event::UnitRecruited {
        unit: id,
        kind: UnitKind::Pawn,
        faction,
        at,
        territory,
        cost,
    });
}

fn build(
    world: &mut World,
    rules: &Ruleset,
    territory: TerritoryId,
    at: TileId,
    kind: TileKind,
    sink: &mut EventSink,
) {
    let cost = match kind {
        TileKind::Town => build_town_cost(world, rules, territory),
        _ => build_field_cost(world, rules, territory),
    };
    let faction = world.territories[&territory].faction;

    if let Some(t) = world.territory_mut(territory) {
        t.gold -= cost;
    }
    world.tile_mut(at).kind = kind;

    let stats = world.stats_mut(faction);
    match kind {
        TileKind::Town => stats.towns_built += 1,
        _ => stats.fields_built += 1,
    }

    sink.push(Event::TileBuilt {
        tile: at,
        kind,
        territory,
        cost,
    });
}

// ---------------------------------------------------------------------------
// Shared validation helpers
// ---------------------------------------------------------------------------

fn own_unit(world: &World, unit: UnitId, active: Faction) -> Result<&Unit, Rejection> {
    let u = world.unit(unit).ok_or(Rejection::NoSuchUnit(unit))?;
    if u.faction != active {
        return Err(Rejection::NotYourUnit {
            unit,
            owner: u.faction,
            active,
        });
    }
    Ok(u)
}

fn own_territory(
    world: &World,
    territory: TerritoryId,
    active: Faction,
) -> Result<&crate::state::Territory, Rejection> {
    let t = world
        .territory(territory)
        .ok_or(Rejection::NoSuchTerritory(territory))?;
    if t.faction != active {
        return Err(Rejection::NotYourTerritory {
            territory,
            owner: t.faction,
            active,
        });
    }
    Ok(t)
}

fn tile_in_range(world: &World, tile: TileId) -> Result<(), Rejection> {
    if tile.index() >= world.tiles.len() {
        return Err(Rejection::NoSuchTile(tile));
    }
    Ok(())
}

/// Towns and fields share every precondition except the price.
fn validate_build(
    world: &World,
    territory: TerritoryId,
    at: TileId,
    active: Faction,
) -> Result<(), Rejection> {
    let t = own_territory(world, territory, active)?;
    if !t.tiles.contains(&at) {
        return Err(Rejection::NotInTerritory {
            tile: at,
            territory,
        });
    }
    let kind = world.tile(at).kind;
    if kind != TileKind::Empty {
        return Err(Rejection::TileNotEmpty {
            tile: at,
            found: kind,
        });
    }
    Ok(())
}

fn afford(world: &World, territory: TerritoryId, needed: i32) -> Result<(), Rejection> {
    let available = world
        .territory(territory)
        .ok_or(Rejection::NoSuchTerritory(territory))?
        .gold;
    if available < needed {
        return Err(Rejection::NotEnoughGold {
            territory,
            needed,
            available,
        });
    }
    Ok(())
}
