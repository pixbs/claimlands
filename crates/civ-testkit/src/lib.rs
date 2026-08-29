//! Shared test scaffolding: hand-built worlds, topologies, and the golden
//! replay harness.
//!
//! Lives in its own crate rather than in `civ-core`'s `#[cfg(test)]` so that
//! integration tests, the CLI fuzzer and future crates can all build the same
//! fixtures. When a rule needs a new kind of situation to test, add a builder
//! here rather than hand-rolling one in a test file — that is what keeps a
//! thousand tests readable.

#![forbid(unsafe_code)]

pub mod builder;
pub mod golden;
pub mod topo;

pub use builder::WorldBuilder;
pub use golden::{GoldenError, GoldenReplay};

/// Re-exported so a test file needs one `use`.
pub mod prelude {
    pub use crate::builder::WorldBuilder;
    pub use crate::golden::GoldenReplay;
    pub use crate::topo;
    pub use civ_core::invariants::{assert_sound, check};
    pub use civ_core::prelude::*;
}

/// Declare which spec rule ids a test covers (gate 12).
///
/// `cargo xtask spec-coverage` cross-references these against the ids defined
/// in `spec/rules/`, and fails CI if a documented rule has no test or a test
/// cites a rule that does not exist. Compiles to nothing.
///
/// ```
/// # fn main() {}
/// # use civ_testkit::covers;
/// #[test]
/// fn towns_are_fed_whole() {
///     covers!("ECON-004");
///     // ...
/// }
/// ```
#[macro_export]
macro_rules! covers {
    ($($id:literal),+ $(,)?) => {
        const _COVERED_RULES: &[&str] = &[$($id),+];
    };
}
