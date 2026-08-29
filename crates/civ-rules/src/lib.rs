//! Balance data for Hex Planet.
//!
//! Every tunable number in the game lives here and is loaded from
//! `assets/rules/*.ron`. Nothing in this crate makes a decision; it only
//! describes what the numbers are. All *behaviour* lives in `civ-core`.
//!
//! Why this crate exists: a balance change should be a data diff that any
//! reviewer can read at a glance, not a code change that could carry a
//! regression with it. It also makes alternate rulesets (tutorial, hardcore,
//! future science-tree variants) free.
//!
//! See `spec/rules/` for the prose each field implements; rule ids are quoted
//! on the fields they govern.

#![forbid(unsafe_code)]
#![deny(clippy::float_arithmetic)]

mod hash;
mod types;
mod validate;

pub use hash::ruleset_hash;
pub use types::*;
pub use validate::RulesetError;

/// The default ruleset, compiled into the binary.
///
/// Embedded rather than read from disk so that `civ-core` tests, the mobile
/// shells and the CLI all agree without any filesystem setup. Loading an
/// alternate ruleset from disk is still possible via [`Ruleset::from_ron`].
pub const DEFAULT_RULES_RON: &str = include_str!("../../../assets/rules/default.ron");

impl Ruleset {
    /// Parse a ruleset from RON text, then validate it.
    pub fn from_ron(text: &str) -> Result<Self, RulesetError> {
        let rules: Ruleset = ron::from_str(text).map_err(|e| RulesetError::Parse(e.to_string()))?;
        rules.validate()?;
        Ok(rules)
    }

    /// The bundled default ruleset.
    ///
    /// Panics only if the compiled-in RON is malformed, which the
    /// `default_rules_are_valid` test makes impossible to land.
    pub fn bundled() -> Self {
        Self::from_ron(DEFAULT_RULES_RON).expect("bundled ruleset must parse and validate")
    }

    /// Stable fingerprint of this ruleset.
    ///
    /// Stored in every replay so that a balance change which would invalidate
    /// a saved game is detected loudly instead of silently replaying wrong.
    pub fn hash(&self) -> u64 {
        ruleset_hash(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_are_valid() {
        let r = Ruleset::bundled();
        assert!(r.validate().is_ok());
    }

    #[test]
    fn ruleset_hash_is_stable_across_parses() {
        assert_eq!(Ruleset::bundled().hash(), Ruleset::bundled().hash());
    }

    #[test]
    fn ruleset_hash_changes_when_a_number_changes() {
        let a = Ruleset::bundled();
        let mut b = Ruleset::bundled();
        b.economy.town.wheat_cost += 1;
        assert_ne!(a.hash(), b.hash());
    }
}
