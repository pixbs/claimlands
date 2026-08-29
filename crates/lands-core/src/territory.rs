//! Territory topology: splitting, merging, and rehousing capitals.
//!
//! This is the subtlest module in the game and the one most worth reading
//! before changing anything. Every border change funnels through
//! [`retopologize`], which recomputes one faction's territories from scratch
//! rather than trying to patch them incrementally. Recomputing costs a few
//! microseconds and is *correct by construction* for compound cases — a
//! capture that simultaneously splits the victim and merges two of the
//! captor's territories is not a special case here, it just falls out.
//!
//! # The rules it implements
//!
//! * **TERR-030 Split.** When a capture disconnects a territory, the component
//!   holding the old capital keeps it, and each other component is given a new
//!   one. Treasuries divide *in proportion to tile count*, floored, with the
//!   remainder going to the component that kept the capital, so a split never
//!   creates or destroys value.
//!
//! * **TERR-040 Merge.** When a capture joins two territories of one faction,
//!   treasuries sum and the capital *closest to the newly captured tile*
//!   survives; the others become empty tiles.
//!
//! * **TERR-011 Relocation.** A territory that has lost its capital rehouses it
//!   on the most central tile of the preferred kind (Empty, then Town, then
//!   Field).
//!
//! * **TERR-013 Disband.** A territory that holds nothing but forest cannot
//!   rehouse its capital, so it loses its owner entirely and its units die.
//!
//! # Invariant
//!
//! On return, every territory of `faction` is connected and has exactly one
//! capital, and every tile the faction owns belongs to exactly one of them.
//! [`crate::invariants`] re-checks this after every command.

use crate::event::{Event, EventSink};
use crate::ids::{Faction, TerritoryId, TileId};
use crate::rng::{SeedDomain, stream};
use crate::state::{Territory, World};
use crate::topology::Topology;
use lands_rules::{Ruleset, TileKind};
use std::collections::{BTreeMap, BTreeSet};

/// One connected piece of a former territory, carrying its share of the
/// treasury and its claim on the old identity.
#[derive(Debug, Clone)]
struct Fragment {
    parent: TerritoryId,
    tiles: BTreeSet<TileId>,
    wheat: i32,
    gold: i32,
    /// The parent's capital, if it survived inside this fragment.
    capital: Option<TileId>,
    /// Whether this fragment may reuse the parent's [`TerritoryId`]. Exactly
    /// one fragment per parent is the heir, so ids stay stable for the UI
    /// across ordinary border nudges.
    heir: bool,
}

/// A territory as it will exist after the rebuild, plus what to say about it.
struct Rebuilt {
    territory: Territory,
    is_new: bool,
    previous_capital: Option<TileId>,
    absorbed: Vec<TerritoryId>,
    /// Capitals that lost the merge contest and must be razed to empty.
    retired_capitals: Vec<TileId>,
}

/// Recompute every territory belonging to `faction`.
///
/// Call this after *any* change to which tiles the faction owns. `focus` is the
/// tile that triggered the change (the newly captured tile), which TERR-040
/// uses to decide which capital survives a merge; pass `None` when there is no
/// such tile, as during world setup.
pub fn retopologize(
    world: &mut World,
    rules: &Ruleset,
    faction: Faction,
    focus: Option<TileId>,
    sink: &mut EventSink,
) {
    let topo = world.topology.clone();
    let owned = world.tiles_of(faction);

    let old_ids: Vec<TerritoryId> = world
        .territories
        .values()
        .filter(|t| t.faction == faction)
        .map(|t| t.id)
        .collect();

    let fragments = fragment(world, &topo, &owned, &old_ids);
    let components = topo.components(&owned);

    // A fragment is connected and entirely faction-owned, so it lies wholly
    // inside exactly one component.
    let mut by_component: BTreeMap<usize, Vec<Fragment>> = BTreeMap::new();
    for f in fragments {
        let probe = *f.tiles.iter().next().expect("fragments are non-empty");
        let index = components
            .iter()
            .position(|c| c.contains(&probe))
            .expect("a faction-owned tile is always in one of the faction's components");
        by_component.entry(index).or_default().push(f);
    }

    let mut rebuilt: Vec<Rebuilt> = Vec::new();
    let mut disbanded: Vec<(TerritoryId, BTreeSet<TileId>)> = Vec::new();
    let mut claimed: BTreeSet<TerritoryId> = BTreeSet::new();

    for (index, component) in components.iter().enumerate() {
        let mut contributors = by_component.remove(&index).unwrap_or_default();
        // Largest first, lowest parent id breaking ties: the order every
        // decision below depends on, so it must not vary between machines.
        contributors.sort_by_key(|f| (std::cmp::Reverse(f.tiles.len()), f.parent));

        let candidates: Vec<TileId> = contributors.iter().filter_map(|f| f.capital).collect();

        // A component may already contain capital tiles that no fragment
        // reported — during level setup there are no prior territories at all.
        // Trust the map before inventing a new site.
        let candidates: Vec<TileId> = if candidates.is_empty() {
            component
                .iter()
                .copied()
                .filter(|&t| world.tile(t).kind == TileKind::Capital)
                .collect()
        } else {
            candidates
        };

        let (capital, retired_capitals) = match candidates.len() {
            0 => match choose_capital_site(world, rules, component) {
                Some(site) => (site, Vec::new()),
                None => {
                    // TERR-013: forest-only remnant. It loses its owner.
                    let id = contributors
                        .first()
                        .map(|f| f.parent)
                        .unwrap_or_else(|| world.alloc_territory_id());
                    disbanded.push((id, component.clone()));
                    continue;
                }
            },
            1 => (candidates[0], Vec::new()),
            _ => {
                // TERR-040: several capitals met. The nearest to the tile that
                // caused the merge survives; the rest are razed.
                let keep = nearest_capital(&topo, component, &candidates, focus);
                let retired = candidates.iter().copied().filter(|&c| c != keep).collect();
                (keep, retired)
            }
        };

        // Reuse the identity of the largest contributing heir, so an ordinary
        // border nudge does not renumber the territory under the player's HUD.
        let inherited = contributors
            .iter()
            .find(|f| f.heir && !claimed.contains(&f.parent))
            .map(|f| f.parent);
        let (id, is_new) = match inherited {
            Some(id) => {
                claimed.insert(id);
                (id, false)
            }
            None => (world.alloc_territory_id(), true),
        };

        let previous_capital = contributors
            .iter()
            .find(|f| f.parent == id)
            .and_then(|f| f.capital);

        rebuilt.push(Rebuilt {
            territory: Territory {
                id,
                faction,
                capital,
                wheat: contributors.iter().map(|f| f.wheat).sum(),
                gold: contributors.iter().map(|f| f.gold).sum(),
                tiles: component.clone(),
            },
            is_new,
            previous_capital,
            absorbed: contributors
                .iter()
                .map(|f| f.parent)
                .filter(|&p| p != id)
                .collect(),
            retired_capitals,
        });
    }

    commit(world, faction, &old_ids, rebuilt, disbanded, sink);
}

/// Phase 1: break each old territory into the connected pieces that survive.
fn fragment(
    world: &World,
    topo: &Topology,
    owned: &BTreeSet<TileId>,
    old_ids: &[TerritoryId],
) -> Vec<Fragment> {
    let mut out = Vec::new();

    for id in old_ids {
        let old = &world.territories[id];
        let surviving: BTreeSet<TileId> = old
            .tiles
            .iter()
            .copied()
            .filter(|t| owned.contains(t))
            .collect();

        if surviving.is_empty() {
            continue; // Nothing left; the id is retired in `commit`.
        }

        let pieces = topo.components(&surviving);
        if pieces.len() == 1 {
            let tiles = pieces.into_iter().next().expect("length checked");
            let capital = tiles.contains(&old.capital).then_some(old.capital);
            out.push(Fragment {
                parent: old.id,
                tiles,
                wheat: old.wheat,
                gold: old.gold,
                capital,
                heir: true,
            });
        } else {
            out.extend(split_treasury(old, pieces));
        }
    }

    out
}

/// TERR-030. Divide a territory's treasury between its disconnected pieces in
/// proportion to tile count.
///
/// Floor division, with the remainder going to the piece that kept the capital
/// (or, if the capital is gone, the largest piece). This is what makes the
/// brief's worked example come out exactly: 15 tiles and 15 gold splitting
/// 10/5 yields 10 and 5, with nothing lost.
fn split_treasury(old: &Territory, pieces: Vec<BTreeSet<TileId>>) -> Vec<Fragment> {
    let total: i64 = pieces.iter().map(|p| p.len() as i64).sum();

    let heir_index = pieces
        .iter()
        .position(|p| p.contains(&old.capital))
        .unwrap_or_else(|| {
            pieces
                .iter()
                .enumerate()
                .max_by_key(|(_, p)| {
                    (
                        p.len(),
                        std::cmp::Reverse(*p.iter().next().expect("non-empty")),
                    )
                })
                .map(|(i, _)| i)
                .expect("a split always has at least one piece")
        });

    let share = |amount: i32, size: i64| -> i32 {
        if total == 0 {
            0
        } else {
            ((amount as i64 * size) / total) as i32
        }
    };

    let mut out: Vec<Fragment> = pieces
        .into_iter()
        .map(|tiles| {
            let size = tiles.len() as i64;
            let capital = tiles.contains(&old.capital).then_some(old.capital);
            Fragment {
                parent: old.id,
                wheat: share(old.wheat, size),
                gold: share(old.gold, size),
                capital,
                tiles,
                heir: false,
            }
        })
        .collect();

    // Floor division loses at most `pieces - 1` of each resource; the heir
    // takes the remainder so that a split conserves the treasury exactly.
    let distributed_wheat: i32 = out.iter().map(|f| f.wheat).sum();
    let distributed_gold: i32 = out.iter().map(|f| f.gold).sum();
    out[heir_index].wheat += old.wheat - distributed_wheat;
    out[heir_index].gold += old.gold - distributed_gold;
    out[heir_index].heir = true;

    out
}

/// TERR-011. Where to rehouse a capital that no longer exists.
///
/// Walks the preference order from the ruleset (Empty, then Town, then Field),
/// taking the first kind that appears anywhere in the territory, and within
/// that kind the tile closest to the territory's graph centre.
///
/// The brief asks for a capital placed "randomly ... approximately the center",
/// which reads as a contradiction. It is resolved by being central first and
/// random only among exact ties, so the result is both centred and
/// reproducible. Returns `None` when no preferred kind exists at all, which
/// means the territory is forest-only and must be disbanded (TERR-013).
fn choose_capital_site(
    world: &World,
    rules: &Ruleset,
    component: &BTreeSet<TileId>,
) -> Option<TileId> {
    for &kind in &rules.territory.capital_relocation_preference {
        let candidates: Vec<TileId> = component
            .iter()
            .copied()
            .filter(|&t| world.tile(t).kind == kind)
            .collect();
        if candidates.is_empty() {
            continue;
        }

        let central = world.topology.most_central(component, &candidates);
        if central.len() == 1 {
            return Some(central[0]);
        }

        // Seeded on the territory's lowest tile so the choice is stable across
        // replays of the same match.
        let anchor = *component.iter().next().expect("non-empty component");
        let mut rng = stream(
            world.seed,
            SeedDomain::CapitalRelocation,
            world.round,
            anchor.0,
        );
        return rng.pick(&central).copied();
    }
    None
}

/// TERR-040. Of several capitals meeting in one territory, which survives.
///
/// The nearest to the tile that caused the merge, measured in hops through the
/// territory. Ties — including the brief's unfinished "if both capitals were
/// created at the same time" case — fall to the lowest tile id, which is
/// arbitrary but reproducible (TERR-041).
fn nearest_capital(
    topo: &Topology,
    component: &BTreeSet<TileId>,
    candidates: &[TileId],
    focus: Option<TileId>,
) -> TileId {
    let mut sorted = candidates.to_vec();
    sorted.sort_unstable();

    let Some(focus) = focus else {
        return sorted[0];
    };

    let dist = topo.distances_from(focus, |t| component.contains(&t));
    sorted
        .into_iter()
        .min_by_key(|c| (dist[c.index()].unwrap_or(u32::MAX), *c))
        .expect("at least one capital candidate")
}

/// Swap the rebuilt territories into the world and emit the difference.
fn commit(
    world: &mut World,
    faction: Faction,
    old_ids: &[TerritoryId],
    rebuilt: Vec<Rebuilt>,
    disbanded: Vec<(TerritoryId, BTreeSet<TileId>)>,
    sink: &mut EventSink,
) {
    for &id in old_ids {
        world.territories.remove(&id);
    }

    // TERR-013 first: disbanding removes tiles from the faction, and the
    // remaining territories were computed on the assumption that it had.
    for (id, tiles) in &disbanded {
        for &tile in tiles {
            if let Some(unit_id) = world.tile(tile).unit
                && let Some(unit) = world.units.remove(&unit_id)
            {
                sink.push(Event::UnitDisbanded {
                    unit: unit_id,
                    kind: unit.kind,
                    faction: unit.faction,
                    at: tile,
                });
            }
            let t = world.tile_mut(tile);
            t.unit = None;
            t.owner = None;
            t.territory = None;
            if t.kind == TileKind::Capital {
                t.kind = TileKind::Empty;
            }
        }
        sink.push(Event::TerritoryDisbanded {
            territory: *id,
            faction,
            tiles: tiles.len() as u32,
        });
    }

    // Clear stale back-pointers before writing new ones, so a tile cannot keep
    // pointing at a territory that no longer owns it.
    for tile in world.tile_ids().collect::<Vec<_>>() {
        if world.tile(tile).owner == Some(faction) {
            world.tile_mut(tile).territory = None;
        }
    }

    let surviving: BTreeSet<TerritoryId> = rebuilt.iter().map(|r| r.territory.id).collect();
    // Which final territories each old id fed into, so a one-to-many
    // relationship can be reported as a split.
    let mut descendants: BTreeMap<TerritoryId, Vec<TerritoryId>> = BTreeMap::new();

    for r in rebuilt {
        let Rebuilt {
            territory,
            is_new,
            previous_capital,
            absorbed,
            retired_capitals,
        } = r;

        for tile in retired_capitals {
            world.tile_mut(tile).kind = TileKind::Empty;
        }
        for &tile in &territory.tiles {
            world.tile_mut(tile).territory = Some(territory.id);
        }
        world.tile_mut(territory.capital).kind = TileKind::Capital;

        descendants
            .entry(territory.id)
            .or_default()
            .push(territory.id);
        for &parent in &absorbed {
            descendants.entry(parent).or_default().push(territory.id);
        }

        if is_new {
            sink.push(Event::TerritoryCreated {
                territory: territory.id,
                faction,
                capital: territory.capital,
                tiles: territory.size(),
                wheat: territory.wheat,
                gold: territory.gold,
            });
        } else if previous_capital != Some(territory.capital) {
            sink.push(Event::CapitalRelocated {
                territory: territory.id,
                from: previous_capital,
                to: territory.capital,
            });
        }

        if !absorbed.is_empty() {
            sink.push(Event::TerritoriesMerged {
                kept: territory.id,
                absorbed,
            });
        }

        world.territories.insert(territory.id, territory);
    }

    for (parent, children) in descendants {
        if children.len() > 1 && old_ids.contains(&parent) {
            sink.push(Event::TerritorySplit {
                parent,
                into: children,
            });
        }
    }

    for &id in old_ids {
        if !surviving.contains(&id) {
            sink.push(Event::TerritoryDissolved {
                territory: id,
                faction,
            });
        }
    }
}

/// Found a brand-new territory around a single capital tile.
///
/// Used by level loading to seed starting positions; ordinary play never calls
/// this, because territories are only ever produced by [`retopologize`].
pub fn found(world: &mut World, rules: &Ruleset, faction: Faction, capital: TileId) -> TerritoryId {
    let id = world.alloc_territory_id();
    let tile = world.tile_mut(capital);
    tile.owner = Some(faction);
    tile.territory = Some(id);
    tile.kind = TileKind::Capital;

    world.territories.insert(
        id,
        Territory {
            id,
            faction,
            capital,
            wheat: rules.economy.starting_wheat,
            gold: rules.economy.starting_gold,
            tiles: BTreeSet::from([capital]),
        },
    );
    id
}
