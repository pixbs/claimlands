//! Who has won, and when (VICT-001..VICT-003).
//!
//! Two ways to win:
//!
//! * **Elimination** — every rival holds no tiles at all.
//! * **Dominance** — hold at least `dominance_threshold_percent` of the
//!   planet's land for `dominance_turns` consecutive rounds. This is the
//!   brief's "unbeatable advantage": without it, a decided match drags on while
//!   the winner mops up single tiles.
//!
//! Dominance is measured **once per round**, not once per turn, so a four-player
//! match cannot accumulate a streak four times as fast as a two-player one.

use crate::event::{Event, EventSink};
use crate::ids::Faction;
use crate::state::{Outcome, VictoryReason, World};
use civ_rules::Ruleset;

/// Update eliminations, dominance streaks and the outcome.
///
/// `round_ended` gates the dominance check; pass `false` for the mid-round
/// call that only needs to notice eliminations.
pub fn evaluate(world: &mut World, rules: &Ruleset, round_ended: bool, sink: &mut EventSink) {
    if world.is_over() {
        return;
    }

    mark_eliminations(world, sink);

    let alive: Vec<Faction> = world
        .players
        .iter()
        .filter(|p| !p.eliminated)
        .map(|p| p.faction)
        .collect();

    match alive.len() {
        0 => {
            let outcome = Outcome::Draw { round: world.round };
            world.outcome = Some(outcome);
            sink.push(Event::GameOver { outcome });
            return;
        }
        1 => {
            let outcome = Outcome::Victory {
                faction: alive[0],
                reason: VictoryReason::Elimination,
                round: world.round,
            };
            world.outcome = Some(outcome);
            sink.push(Event::GameOver { outcome });
            return;
        }
        _ => {}
    }

    if round_ended {
        check_dominance(world, rules, &alive, sink);
    }
}

/// A faction with no tiles left is out. Its units, if any survive on tiles it
/// no longer owns, are irrelevant: with no capital it can never rebuild.
fn mark_eliminations(world: &mut World, sink: &mut EventSink) {
    let newly_out: Vec<Faction> = world
        .players
        .iter()
        .filter(|p| !p.eliminated && world.tile_count_of(p.faction) == 0)
        .map(|p| p.faction)
        .collect();

    for faction in newly_out {
        if let Some(p) = world.players.iter_mut().find(|p| p.faction == faction) {
            p.eliminated = true;
        }
        sink.push(Event::PlayerEliminated { faction });
    }
}

fn check_dominance(world: &mut World, rules: &Ruleset, alive: &[Faction], sink: &mut EventSink) {
    let land = world.land_tile_count();
    if land == 0 {
        return;
    }
    let threshold = rules.victory.dominance_threshold_percent;

    for &faction in alive {
        let held = world.tile_count_of(faction);
        // Integer comparison rather than a percentage division, so no rounding
        // and no floating point.
        let dominant = u64::from(held) * 100 >= u64::from(land) * u64::from(threshold);

        let streak = world.dominance_streak.entry(faction).or_insert(0);
        *streak = if dominant { *streak + 1 } else { 0 };

        if *streak >= rules.victory.dominance_turns {
            let outcome = Outcome::Victory {
                faction,
                reason: VictoryReason::Dominance,
                round: world.round,
            };
            world.outcome = Some(outcome);
            sink.push(Event::GameOver { outcome });
            return;
        }
    }
}

/// Share of the planet's land a faction holds, in whole percent.
///
/// Provided for the HUD; the victory test itself avoids the division.
pub fn land_share_percent(world: &World, faction: Faction) -> u32 {
    let land = world.land_tile_count();
    if land == 0 {
        return 0;
    }
    world.tile_count_of(faction) * 100 / land
}
