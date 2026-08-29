//! Ruleset sanity checks.
//!
//! These run on every load, including the bundled default. They exist so that
//! an agent editing `assets/rules/default.ron` gets a precise error at load
//! time instead of a confusing panic ten turns into a simulation.

use crate::types::{RULESET_VERSION, Ruleset, TileKind, UnitKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RulesetError {
    #[error("failed to parse ruleset: {0}")]
    Parse(String),

    #[error("ruleset version {found} is not supported (this build expects {expected})")]
    Version { found: u32, expected: u32 },

    #[error("no yield defined for tile kind {0:?}")]
    MissingTileYield(TileKind),

    #[error(
        "tile kind Town must not appear in economy.tile_yields; its output is conditional on \
         wheat and is configured under economy.town"
    )]
    TownYieldMisplaced,

    #[error("no profile defined for unit kind {0:?}")]
    MissingUnitProfile(UnitKind),

    #[error("units.starvation_priority must list every unit kind exactly once (found {found:?})")]
    StarvationPriority { found: Vec<UnitKind> },

    #[error("{field} must be within {min}..={max}, found {found}")]
    OutOfRange {
        field: String,
        min: i64,
        max: i64,
        found: i64,
    },

    #[error("{field} must not be negative, found {found}")]
    Negative { field: String, found: i32 },

    #[error("territory.capital_relocation_preference must not contain Capital")]
    RelocateOntoCapital,

    #[error("upgrade chain is not acyclic: {0:?} upgrades to itself")]
    SelfUpgrade(UnitKind),
}

impl Ruleset {
    /// Check every invariant the rest of the codebase is allowed to assume.
    pub fn validate(&self) -> Result<(), RulesetError> {
        if self.version != RULESET_VERSION {
            return Err(RulesetError::Version {
                found: self.version,
                expected: RULESET_VERSION,
            });
        }

        // Every unconditionally-yielding tile kind must be present, so that
        // `tile_yield` never silently returns zero for a kind someone forgot.
        if self.economy.tile_yields.contains_key(&TileKind::Town) {
            return Err(RulesetError::TownYieldMisplaced);
        }
        for kind in TileKind::ALL {
            if kind != TileKind::Town && !self.economy.tile_yields.contains_key(&kind) {
                return Err(RulesetError::MissingTileYield(kind));
            }
        }

        range(
            "economy.town.wheat_cost",
            self.economy.town.wheat_cost as i64,
            1,
            1_000,
        )?;
        non_negative("economy.town.gold_yield", self.economy.town.gold_yield)?;
        non_negative("economy.starting_wheat", self.economy.starting_wheat)?;
        non_negative("economy.starting_gold", self.economy.starting_gold)?;

        for (name, cost) in [
            ("costs.recruit_pawn", self.costs.recruit_pawn),
            ("costs.upgrade_warrior", self.costs.upgrade_warrior),
            ("costs.upgrade_knight", self.costs.upgrade_knight),
            ("costs.build_town", self.costs.build_town),
            ("costs.build_field", self.costs.build_field),
        ] {
            non_negative(&format!("{name}.base"), cost.base)?;
            non_negative(&format!("{name}.per_existing"), cost.per_existing)?;
        }

        for kind in UnitKind::ALL {
            let profile = self
                .units
                .profiles
                .get(&kind)
                .ok_or(RulesetError::MissingUnitProfile(kind))?;
            if profile.upgrades_to == Some(kind) {
                return Err(RulesetError::SelfUpgrade(kind));
            }
        }

        // Starvation order must be a permutation, or some unit kind would be
        // immortal under famine and another would be considered twice.
        let mut sorted = self.units.starvation_priority.clone();
        sorted.sort();
        sorted.dedup();
        if sorted.len() != UnitKind::ALL.len()
            || self.units.starvation_priority.len() != UnitKind::ALL.len()
        {
            return Err(RulesetError::StarvationPriority {
                found: self.units.starvation_priority.clone(),
            });
        }

        range(
            "territory.capital_loot_percent",
            self.territory.capital_loot_percent as i64,
            0,
            100,
        )?;
        if self
            .territory
            .capital_relocation_preference
            .contains(&TileKind::Capital)
        {
            return Err(RulesetError::RelocateOntoCapital);
        }

        range(
            "growth.forest_spread_percent",
            self.growth.forest_spread_percent as i64,
            0,
            100,
        )?;
        range(
            "victory.dominance_threshold_percent",
            self.victory.dominance_threshold_percent as i64,
            1,
            100,
        )?;
        range(
            "victory.dominance_turns",
            self.victory.dominance_turns as i64,
            1,
            10_000,
        )?;

        Ok(())
    }
}

fn range(field: &str, found: i64, min: i64, max: i64) -> Result<(), RulesetError> {
    if found < min || found > max {
        return Err(RulesetError::OutOfRange {
            field: field.to_owned(),
            min,
            max,
            found,
        });
    }
    Ok(())
}

fn non_negative(field: &str, found: i32) -> Result<(), RulesetError> {
    if found < 0 {
        return Err(RulesetError::Negative {
            field: field.to_owned(),
            found,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_town_yield_in_the_wrong_place() {
        let mut r = Ruleset::bundled();
        r.economy
            .tile_yields
            .insert(TileKind::Town, crate::Yield { wheat: 0, gold: 2 });
        assert_eq!(r.validate(), Err(RulesetError::TownYieldMisplaced));
    }

    #[test]
    fn rejects_incomplete_starvation_priority() {
        let mut r = Ruleset::bundled();
        r.units.starvation_priority = vec![UnitKind::Pawn, UnitKind::Pawn, UnitKind::Knight];
        assert!(matches!(
            r.validate(),
            Err(RulesetError::StarvationPriority { .. })
        ));
    }

    #[test]
    fn rejects_out_of_range_loot_percent() {
        let mut r = Ruleset::bundled();
        r.territory.capital_loot_percent = 250;
        assert!(matches!(r.validate(), Err(RulesetError::OutOfRange { .. })));
    }

    #[test]
    fn rejects_unsupported_version() {
        let mut r = Ruleset::bundled();
        r.version = 99;
        assert!(matches!(r.validate(), Err(RulesetError::Version { .. })));
    }
}
