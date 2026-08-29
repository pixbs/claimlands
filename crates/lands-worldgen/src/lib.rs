//! Procedural planet topology and terrain (M3).
//!
//! The planet is a Goldberg polyhedron: the dual of a geodesic icosahedron.
//! Take the twenty triangular faces of an icosahedron, cut each into `n²`
//! smaller triangles, push every point out onto the unit sphere, and then turn
//! each vertex into a tile. A frequency-`n` planet is always `10n² + 2` tiles,
//! of which exactly twelve — the icosahedron's own corners — are pentagons and
//! the rest hexagons.
//!
//! [`icosahedron`] and [`geodesic`] build the triangle mesh; [`goldberg`]
//! turns it inside out into the tiles the game is played on, and [`terrain`]
//! decides which of those tiles are land. Cover comes later and sits on top of
//! that.
//!
//! # What this crate owes the rest of the game
//!
//! One thing: [`Goldberg::topology`], a `lands_core::Topology`. `lands-core`
//! never sees a coordinate — it asks spatial questions in graph hops
//! (`lands_core::topology`), so the planet's geometry can change without
//! touching a line of game logic. What it may not change is **vertex order**:
//! tile ids in saved levels and in the golden replay corpus are indices into
//! the order [`geodesic`] produces, so reordering it renumbers every tile on
//! every stored planet. [`Geodesic::structure_hash`] and
//! [`Goldberg::adjacency_hash`] pin that order in a test so an accidental
//! change is caught here rather than in a save file.
//!
//! Floating point is allowed in this crate and forbidden in `lands-core`. That
//! is not an inconsistency: the mesh is built once per match from an integer
//! frequency, and nothing downstream of it compares a coordinate to decide a
//! rule. Anything that must be identical on every target — the dedup, the
//! ordering, the fingerprint — is computed from integers here too. See
//! `docs/determinism.md`.
//!
//! [`terrain`] is the one place that turns geometry into something the rules
//! do read, since land and water decide where a unit may stand. It stays
//! reproducible by using only arithmetic IEEE-754 specifies exactly — which
//! `sin` is not, so the crate brings its own (`trig`).

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

mod digest;
mod trig;

pub mod geodesic;
pub mod goldberg;
pub mod icosahedron;
pub mod terrain;
pub mod vec3;

pub use geodesic::{Geodesic, LatticeKey, geodesic, triangle_count, vertex_count};
pub use goldberg::{Cell, Goldberg, dual, goldberg};
pub use icosahedron::{Icosahedron, icosahedron};
pub use terrain::{LAND_PERCENT, TerrainMap, target_land, terrain};
pub use vec3::Vec3;
