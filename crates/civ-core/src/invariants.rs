//! Structural invariants of the world.
//!
//! These are the statements the rest of the codebase is allowed to assume, and
//! the ones that any territory bug will violate first. [`check`] is called
//! after every command in debug builds and by every property test, so a
//! violation is caught at the command that caused it rather than ten turns
//! later when something finally panics.
//!
//! The important one is **exactly one capital per territory**. Almost every
//! subtle rule in the game — splitting, merging, relocation, disbanding —
//! exists to maintain it.

use crate::ids::{Faction, TerritoryId, TileId, UnitId};
use crate::state::World;
use civ_rules::TileKind;
use std::collections::BTreeSet;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    /// A tile has an owner but belongs to no territory.
    OwnedTileWithoutTerritory {
        tile: TileId,
        owner: Faction,
    },
    /// A tile points at a territory that does not exist.
    DanglingTerritoryRef {
        tile: TileId,
        territory: TerritoryId,
    },
    /// A tile and its territory disagree about who owns it.
    OwnerMismatch {
        tile: TileId,
        tile_owner: Option<Faction>,
        territory_owner: Faction,
    },
    /// A territory claims a tile that does not point back at it.
    TerritoryTileMismatch {
        territory: TerritoryId,
        tile: TileId,
    },
    /// A territory's tiles are not all reachable from one another.
    TerritoryNotConnected {
        territory: TerritoryId,
        pieces: usize,
    },
    /// A territory does not contain the tile it calls its capital.
    CapitalOutsideTerritory {
        territory: TerritoryId,
        capital: TileId,
    },
    /// The capital tile is not marked as a capital.
    CapitalTileWrongKind {
        territory: TerritoryId,
        capital: TileId,
        found: TileKind,
    },
    /// A territory holds more than one capital tile.
    MultipleCapitals {
        territory: TerritoryId,
        count: usize,
    },
    /// A capital tile exists outside any territory.
    OrphanCapital {
        tile: TileId,
    },
    /// Two territories claim the same tile.
    OverlappingTerritories {
        tile: TileId,
        a: TerritoryId,
        b: TerritoryId,
    },
    NegativeTreasury {
        territory: TerritoryId,
        wheat: i32,
        gold: i32,
    },
    /// A unit's tile does not point back at the unit.
    UnitTileMismatch {
        unit: UnitId,
        tile: TileId,
    },
    /// A tile points at a unit that no longer exists.
    DanglingUnitRef {
        tile: TileId,
        unit: UnitId,
    },
    UnitOnWater {
        unit: UnitId,
        tile: TileId,
    },
    /// Water is never owned, built on, or occupied.
    WaterTileNotNeutral {
        tile: TileId,
    },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Every invariant violation currently present. Empty means the world is sound.
pub fn check(world: &World) -> Vec<Violation> {
    let mut out = Vec::new();
    check_tiles(world, &mut out);
    check_territories(world, &mut out);
    check_units(world, &mut out);
    out
}

fn check_tiles(world: &World, out: &mut Vec<Violation>) {
    for tile_id in world.tile_ids() {
        let tile = world.tile(tile_id);

        if !tile.is_land() {
            if tile.owner.is_some()
                || tile.territory.is_some()
                || tile.unit.is_some()
                || tile.kind != TileKind::Empty
            {
                out.push(Violation::WaterTileNotNeutral { tile: tile_id });
            }
            continue;
        }

        match (tile.owner, tile.territory) {
            (Some(owner), None) => {
                out.push(Violation::OwnedTileWithoutTerritory {
                    tile: tile_id,
                    owner,
                });
            }
            (_, Some(territory)) => match world.territory(territory) {
                None => out.push(Violation::DanglingTerritoryRef {
                    tile: tile_id,
                    territory,
                }),
                Some(t) => {
                    if tile.owner != Some(t.faction) {
                        out.push(Violation::OwnerMismatch {
                            tile: tile_id,
                            tile_owner: tile.owner,
                            territory_owner: t.faction,
                        });
                    }
                    if !t.tiles.contains(&tile_id) {
                        out.push(Violation::TerritoryTileMismatch {
                            territory,
                            tile: tile_id,
                        });
                    }
                }
            },
            (None, None) => {
                // Unowned land. A capital may not stand on it.
                if tile.kind == TileKind::Capital {
                    out.push(Violation::OrphanCapital { tile: tile_id });
                }
            }
        }
    }
}

fn check_territories(world: &World, out: &mut Vec<Violation>) {
    let mut claimed: std::collections::BTreeMap<TileId, TerritoryId> =
        std::collections::BTreeMap::new();

    for (&id, t) in &world.territories {
        if t.wheat < 0 || t.gold < 0 {
            out.push(Violation::NegativeTreasury {
                territory: id,
                wheat: t.wheat,
                gold: t.gold,
            });
        }

        for &tile in &t.tiles {
            if let Some(&other) = claimed.get(&tile) {
                out.push(Violation::OverlappingTerritories {
                    tile,
                    a: other,
                    b: id,
                });
            } else {
                claimed.insert(tile, id);
            }
            if world.tile(tile).territory != Some(id) {
                out.push(Violation::TerritoryTileMismatch {
                    territory: id,
                    tile,
                });
            }
        }

        let pieces = world.topology.components(&t.tiles);
        if pieces.len() > 1 {
            out.push(Violation::TerritoryNotConnected {
                territory: id,
                pieces: pieces.len(),
            });
        }

        if !t.tiles.contains(&t.capital) {
            out.push(Violation::CapitalOutsideTerritory {
                territory: id,
                capital: t.capital,
            });
        } else {
            let kind = world.tile(t.capital).kind;
            if kind != TileKind::Capital {
                out.push(Violation::CapitalTileWrongKind {
                    territory: id,
                    capital: t.capital,
                    found: kind,
                });
            }
        }

        let capitals: BTreeSet<TileId> = t
            .tiles
            .iter()
            .copied()
            .filter(|&tile| world.tile(tile).kind == TileKind::Capital)
            .collect();
        if capitals.len() > 1 {
            out.push(Violation::MultipleCapitals {
                territory: id,
                count: capitals.len(),
            });
        }
    }
}

fn check_units(world: &World, out: &mut Vec<Violation>) {
    for (&id, unit) in &world.units {
        let tile = world.tile(unit.tile);
        if tile.unit != Some(id) {
            out.push(Violation::UnitTileMismatch {
                unit: id,
                tile: unit.tile,
            });
        }
        if !tile.is_land() {
            out.push(Violation::UnitOnWater {
                unit: id,
                tile: unit.tile,
            });
        }
    }

    for tile_id in world.tile_ids() {
        if let Some(unit) = world.tile(tile_id).unit
            && !world.units.contains_key(&unit)
        {
            out.push(Violation::DanglingUnitRef {
                tile: tile_id,
                unit,
            });
        }
    }
}

/// Panic with the full list if the world is unsound.
///
/// Used by tests and by debug builds of the CLI fuzzer.
#[track_caller]
pub fn assert_sound(world: &World) {
    let violations = check(world);
    assert!(
        violations.is_empty(),
        "world invariants violated ({}):\n{}",
        violations.len(),
        violations
            .iter()
            .map(|v| format!("  - {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
