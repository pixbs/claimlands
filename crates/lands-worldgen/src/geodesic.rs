//! Geodesic subdivision: the icosahedron, cut finer, pushed onto the sphere.
//!
//! Every one of the twenty icosahedral faces is cut into `n²` triangles on a
//! barycentric lattice, and each lattice point is projected onto the unit
//! sphere. Faces share their edges, so the same point is reached from two
//! different faces and has to be recognised as one vertex — that dedup is the
//! whole difficulty of this module, and it is the reason the vertex count
//! comes out at exactly `10n² + 2` instead of somewhere near it.
//!
//! Ported from `reference/prototype/hex-planet.html`.
//!
//! # Why the dedup is combinatorial and not geometric
//!
//! The prototype keys vertices on their coordinates rounded to six decimals.
//! That works in a browser demo and is a trap here. Two faces compute a shared
//! edge point from different corners in a different order, so the two results
//! differ in the last bits; rounding hides the difference *most* of the time,
//! but a point that lands near a rounding boundary can round two ways and
//! silently become two vertices — a seam in the mesh and a hole in the tile
//! graph. Widening the tolerance only swaps that failure for the opposite one,
//! where two genuinely distinct vertices merge.
//!
//! So no coordinate is ever compared. A lattice point's identity is where it
//! sits on the icosahedron — a corner, a point some whole number of steps
//! along an edge, or a point strictly inside one face — and that is an integer
//! ([`LatticeKey`]). Dedup is then exact by construction: no tolerance, no
//! near-misses, and the same answer on every target.
//!
//! # Ordering
//!
//! Vertices come out in the order they are first reached, walking the faces in
//! [`crate::icosahedron::FACES`] order and each face's lattice in `i`-then-`j`
//! order. The order is part of this crate's contract, not an implementation
//! detail: it is what numbers the planet's tiles, so changing it renumbers
//! every tile in every saved level. [`Geodesic::structure_hash`] exists to
//! make an accidental change of it fail a test rather than a save file.

use crate::digest::Digest;
use crate::icosahedron::{Icosahedron, icosahedron};
use crate::vec3::Vec3;
use std::collections::BTreeMap;

/// The largest frequency [`geodesic`] accepts.
///
/// Far above anything the game uses — the level format tops out at 12, which
/// is 1442 tiles — and present only so that a nonsensical frequency panics
/// with a clear message instead of overflowing the `u32` vertex indices.
pub const MAX_FREQUENCY: u32 = 512;

/// Vertices in a frequency-`n` geodesic: `10n² + 2`.
///
/// This is also the tile count of its dual Goldberg polyhedron, since the dual
/// turns each geodesic vertex into one tile: twelve pentagons at the original
/// icosahedron's corners, hexagons everywhere else.
pub const fn vertex_count(frequency: u32) -> u32 {
    10 * frequency * frequency + 2
}

/// Triangles in a frequency-`n` geodesic: `20n²`, twenty faces cut into `n²`.
pub const fn triangle_count(frequency: u32) -> u32 {
    20 * frequency * frequency
}

/// Where a lattice point sits on the icosahedron — its exact identity, with no
/// coordinates involved.
///
/// Two faces that reach the same point produce the same key, and two distinct
/// points can never produce the same one, so comparing keys deduplicates the
/// subdivision exactly. Edge keys are canonicalised to run from the
/// lower-numbered corner to the higher, so the two faces sharing an edge agree
/// on how far along it a point is even though they walk it in opposite
/// directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LatticeKey {
    /// One of the icosahedron's own twelve corners. These become the planet's
    /// twelve pentagons.
    Corner(u8),
    /// `step` of `frequency` steps along the edge from corner `from` to corner
    /// `to`, where `from < to` and `0 < step < frequency`.
    Edge { from: u8, to: u8, step: u32 },
    /// Strictly inside face `face`, at barycentric lattice coordinates
    /// `(i, j)` — all three weights non-zero, so no other face contains it.
    Interior { face: u8, i: u32, j: u32 },
}

/// A subdivided icosahedron projected onto the unit sphere.
///
/// Build one with [`geodesic`]. The dual of this — one tile per vertex — is
/// the Goldberg polyhedron the game is played on.
#[derive(Debug, Clone, PartialEq)]
pub struct Geodesic {
    frequency: u32,
    vertices: Vec<Vec3>,
    keys: Vec<LatticeKey>,
    triangles: Vec<[u32; 3]>,
}

impl Geodesic {
    /// The subdivision frequency this was built at.
    pub fn frequency(&self) -> u32 {
        self.frequency
    }

    /// Unit-length vertex positions, in first-reached order.
    pub fn vertices(&self) -> &[Vec3] {
        &self.vertices
    }

    /// The identity of each vertex, parallel to [`Self::vertices`].
    ///
    /// Exposed because it is the only description of the mesh that carries no
    /// floating point: a caller that needs to compare, cache or fingerprint a
    /// planet should compare these rather than coordinates.
    pub fn keys(&self) -> &[LatticeKey] {
        &self.keys
    }

    /// Triangles, as triples of vertex indices, wound counter-clockwise seen
    /// from outside.
    pub fn triangles(&self) -> &[[u32; 3]] {
        &self.triangles
    }

    /// A fingerprint of the vertex order and the triangle list.
    ///
    /// Coordinates are deliberately **not** hashed. A compiler is free to
    /// contract or reorder floating-point arithmetic differently per target,
    /// so a hash over coordinates would disagree between an x86-64 CI runner
    /// and an ARM phone while describing the very same planet — a test that
    /// fails for a reason nobody can act on. What must never drift is the
    /// *numbering*: tile ids in saved levels and golden replays are indices
    /// into this order. That is integer, and it is what this hashes.
    ///
    /// Taken with [`crate::digest`], which explains the choice of hash.
    pub fn structure_hash(&self) -> u64 {
        let mut d = Digest::new();
        d.u32(self.frequency);

        d.u32(self.vertices.len() as u32);
        for key in &self.keys {
            match *key {
                LatticeKey::Corner(v) => {
                    d.byte(0);
                    d.byte(v);
                }
                LatticeKey::Edge { from, to, step } => {
                    d.byte(1);
                    d.byte(from);
                    d.byte(to);
                    d.u32(step);
                }
                LatticeKey::Interior { face, i, j } => {
                    d.byte(2);
                    d.byte(face);
                    d.u32(i);
                    d.u32(j);
                }
            }
        }

        d.u32(self.triangles.len() as u32);
        for triangle in &self.triangles {
            for index in triangle {
                d.u32(*index);
            }
        }

        d.finish()
    }
}

/// Subdivide the icosahedron `frequency` times and project onto the sphere.
///
/// The result has [`vertex_count`] vertices and [`triangle_count`] triangles,
/// exactly — see the module docs for why "exactly" is the interesting part.
///
/// # Panics
///
/// If `frequency` is zero or above [`MAX_FREQUENCY`]. Frequency is a level
/// parameter chosen by an author, not player input, so a bad one is a bug in
/// the caller rather than a condition to handle.
pub fn geodesic(frequency: u32) -> Geodesic {
    assert!(
        (1..=MAX_FREQUENCY).contains(&frequency),
        "geodesic frequency must be 1..={MAX_FREQUENCY}, got {frequency}"
    );

    let n = frequency;
    let base: Icosahedron = icosahedron();

    let mut vertices = Vec::with_capacity(vertex_count(n) as usize);
    let mut keys = Vec::with_capacity(vertex_count(n) as usize);
    let mut triangles = Vec::with_capacity(triangle_count(n) as usize);
    let mut index: BTreeMap<LatticeKey, u32> = BTreeMap::new();

    // One face's lattice, held as a square grid with the `i + j > n` half
    // unused. Reused across faces so the allocation happens once.
    let row = (n + 1) as usize;
    let mut lattice = vec![0u32; row * row];
    let at = |i: u32, j: u32| i as usize * row + j as usize;

    for (f, corners) in base.faces.iter().enumerate() {
        let [a, b, c] = corners.map(|v| base.vertices[v as usize]);

        // Barycentric lattice: weight `w` on A, `i` on B, `j` on C, summing to
        // n. `i` walks toward B and `j` toward C, so the lattice inherits the
        // face's winding.
        for i in 0..=n {
            for j in 0..=(n - i) {
                let w = n - i - j;
                let key = lattice_key(f as u8, *corners, n, i, j);

                let next = vertices.len() as u32;
                let id = *index.entry(key).or_insert(next);
                if id == next {
                    // The prototype divides the mix by n before normalising.
                    // Normalising discards the scale, so the division only
                    // adds a rounding step; skipping it leaves a corner's
                    // position bit-identical to the base vertex it came from.
                    let mix = a * f64::from(w) + b * f64::from(i) + c * f64::from(j);
                    vertices.push(mix.normalize());
                    keys.push(key);
                }
                lattice[at(i, j)] = id;
            }
        }

        // Each lattice cell contributes an upward triangle, and every cell but
        // the last of its row a downward one as well: n² per face.
        for i in 0..n {
            for j in 0..(n - i) {
                triangles.push([
                    lattice[at(i, j)],
                    lattice[at(i + 1, j)],
                    lattice[at(i, j + 1)],
                ]);
                if j + 1 < n - i {
                    triangles.push([
                        lattice[at(i + 1, j)],
                        lattice[at(i + 1, j + 1)],
                        lattice[at(i, j + 1)],
                    ]);
                }
            }
        }
    }

    debug_assert_eq!(vertices.len(), vertex_count(n) as usize);
    debug_assert_eq!(triangles.len(), triangle_count(n) as usize);

    Geodesic {
        frequency,
        vertices,
        keys,
        triangles,
    }
}

/// The identity of lattice point `(i, j)` on face `face` of `corners`.
///
/// A weight of zero means the point lies on the far side of the triangle from
/// that corner, so counting the zeros says whether it is a corner (two zeros),
/// on an edge (one) or interior (none).
fn lattice_key(face: u8, [a, b, c]: [u8; 3], n: u32, i: u32, j: u32) -> LatticeKey {
    let w = n - i - j;

    match (w == 0, i == 0, j == 0) {
        // Two weights zero: an icosahedron corner, shared by five faces.
        (_, true, true) => LatticeKey::Corner(a),
        (true, _, true) => LatticeKey::Corner(b),
        (true, true, _) => LatticeKey::Corner(c),
        // One weight zero: on the edge between the other two corners, shared
        // by two faces. The step is counted from the first named corner and
        // canonicalised by `edge_key`.
        (_, _, true) => edge_key(a, b, n, i),
        (_, true, _) => edge_key(a, c, n, j),
        (true, _, _) => edge_key(b, c, n, j),
        // No weight zero: strictly inside this face, and reached only here.
        _ => LatticeKey::Interior { face, i, j },
    }
}

/// An edge key that does not depend on which direction the edge is walked.
///
/// `step` arrives counted from `from`. The two faces sharing an edge walk it
/// in opposite directions, so both are rewritten to count from the
/// lower-numbered corner — without which the same point would get two keys and
/// the mesh would tear along every one of the thirty icosahedral edges.
fn edge_key(from: u8, to: u8, n: u32, step: u32) -> LatticeKey {
    if from < to {
        LatticeKey::Edge { from, to, step }
    } else {
        LatticeKey::Edge {
            from: to,
            to: from,
            step: n - step,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icosahedron::FACES;
    use std::collections::{BTreeMap, BTreeSet};

    /// Every undirected edge of the mesh, with how many triangles use it.
    fn edge_uses(g: &Geodesic) -> BTreeMap<(u32, u32), u32> {
        let mut out: BTreeMap<(u32, u32), u32> = BTreeMap::new();
        for t in g.triangles() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                *out.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        out
    }

    #[test]
    fn counts_match_the_closed_form_at_the_sizes_the_game_uses() {
        for n in [1, 2, 8, 12] {
            let g = geodesic(n);
            assert_eq!(
                g.vertices().len(),
                vertex_count(n) as usize,
                "vertex count at n={n}"
            );
            assert_eq!(
                g.triangles().len(),
                triangle_count(n) as usize,
                "triangle count at n={n}"
            );
        }

        // Spot values, spelled out so a wrong formula cannot agree with itself.
        assert_eq!(geodesic(1).vertices().len(), 12);
        assert_eq!(geodesic(2).vertices().len(), 42);
        assert_eq!(geodesic(8).vertices().len(), 642);
        assert_eq!(geodesic(12).vertices().len(), 1442);
    }

    #[test]
    fn the_dual_has_ten_n_squared_plus_two_tiles_at_every_playable_frequency() {
        // The dual turns one geodesic vertex into one tile, so the vertex
        // count *is* the tile count. This is the acceptance criterion for the
        // whole subdivision: too few means two distinct points were merged,
        // too many means a shared point was counted twice.
        for n in 2..=12 {
            assert_eq!(
                geodesic(n).vertices().len() as u32,
                10 * n * n + 2,
                "tile count at n={n}"
            );
        }
    }

    #[test]
    fn frequency_one_reproduces_the_icosahedron_itself() {
        let g = geodesic(1);
        let corner = |v: u32| match g.keys()[v as usize] {
            LatticeKey::Corner(c) => c,
            other => panic!("n=1 should be corners only, got {other:?}"),
        };
        let faces: Vec<[u8; 3]> = g.triangles().iter().map(|t| t.map(corner)).collect();
        assert_eq!(faces, FACES.to_vec());
    }

    #[test]
    fn every_vertex_lands_on_the_unit_sphere() {
        for n in [1, 2, 8, 12] {
            for (i, v) in geodesic(n).vertices().iter().enumerate() {
                assert!(
                    (v.length() - 1.0).abs() < 1e-12,
                    "n={n} vertex {i} has length {}",
                    v.length()
                );
            }
        }
    }

    #[test]
    fn no_two_vertices_are_near_duplicates() {
        // The failure a float-keyed dedup produces is a *pair* of vertices a
        // few ulps apart where there should be one. The counts above would
        // catch that; this catches it with a diagnosis, and also proves the
        // opposite mistake — a tolerance wide enough to merge real neighbours
        // — was not made, since the closest genuine pair is still far apart.
        let g = geodesic(8);
        let v = g.vertices();

        let mut closest = f64::MAX;
        for (i, a) in v.iter().enumerate() {
            for b in &v[i + 1..] {
                closest = closest.min((*a - *b).length());
            }
        }

        // Neighbouring vertices at n=8 sit roughly 0.078 apart; anything an
        // order of magnitude below that is a duplicate, not a neighbour.
        assert!(
            closest > 0.05,
            "closest pair of the 642 vertices is {closest} apart"
        );
    }

    #[test]
    fn the_mesh_is_a_closed_surface() {
        for n in [1, 2, 8] {
            let g = geodesic(n);
            let edges = edge_uses(&g);

            assert!(
                edges.values().all(|&uses| uses == 2),
                "n={n} has an edge that is not shared by exactly two triangles"
            );
            // Euler's formula. It only comes out at 2 if the dedup was exact:
            // a missed merge adds vertices and edges without adding faces.
            let (v, e, f) = (g.vertices().len(), edges.len(), g.triangles().len());
            assert_eq!(v + f - e, 2, "n={n}: V={v} E={e} F={f}");
            assert_eq!(e as u32, 30 * n * n);
        }
    }

    #[test]
    fn every_triangle_is_wound_outward() {
        let g = geodesic(8);
        for (t, tri) in g.triangles().iter().enumerate() {
            let [a, b, c] = tri.map(|i| g.vertices()[i as usize]);
            let normal = (b - a).cross(c - a);
            assert!(
                normal.dot(a + b + c) > 0.0,
                "triangle {t} faces inward, which would flip the dual's corner fan"
            );
        }
    }

    #[test]
    fn every_triangle_indexes_three_distinct_vertices_in_range() {
        let g = geodesic(8);
        for tri in g.triangles() {
            for &i in tri {
                assert!((i as usize) < g.vertices().len());
            }
            let distinct: BTreeSet<u32> = tri.iter().copied().collect();
            assert_eq!(distinct.len(), 3, "degenerate triangle {tri:?}");
        }
    }

    #[test]
    fn each_vertex_has_exactly_one_identity() {
        let g = geodesic(8);
        let distinct: BTreeSet<LatticeKey> = g.keys().iter().copied().collect();
        assert_eq!(distinct.len(), g.vertices().len());
        assert_eq!(g.keys().len(), g.vertices().len());

        // Exactly twelve corners survive, which is where the twelve pentagons
        // of the dual come from.
        let corners = g
            .keys()
            .iter()
            .filter(|k| matches!(k, LatticeKey::Corner(_)))
            .count();
        assert_eq!(corners, 12);
    }

    #[test]
    fn the_two_faces_sharing_an_edge_agree_on_its_points() {
        // Directly the property `edge_key` exists for: walking an edge from
        // either end names the same points. Without the canonicalisation each
        // of the thirty icosahedral edges would carry a duplicate set.
        let n = 5;
        assert_eq!(edge_key(2, 7, n, 1), edge_key(7, 2, n, n - 1));
        assert_eq!(edge_key(9, 4, n, 3), edge_key(4, 9, n, n - 3));
        // Distinct steps stay distinct.
        assert_ne!(edge_key(2, 7, n, 1), edge_key(2, 7, n, 2));
    }

    #[test]
    fn the_same_frequency_always_builds_the_same_sphere() {
        // Bit-for-bit, including coordinates: within one target the build is
        // reproducible, which is what lets a level store a tile id.
        for n in [2, 8] {
            let (a, b) = (geodesic(n), geodesic(n));
            assert_eq!(a, b);
            assert_eq!(a.structure_hash(), b.structure_hash());
        }
    }

    #[test]
    fn the_fingerprint_tells_frequencies_apart() {
        let hashes: BTreeSet<u64> = (1..=12).map(|n| geodesic(n).structure_hash()).collect();
        assert_eq!(hashes.len(), 12);
    }

    #[test]
    #[should_panic(expected = "geodesic frequency must be")]
    fn frequency_zero_is_rejected() {
        let _ = geodesic(0);
    }

    #[test]
    #[should_panic(expected = "geodesic frequency must be")]
    fn an_absurd_frequency_is_rejected_before_it_overflows() {
        let _ = geodesic(MAX_FREQUENCY + 1);
    }
}
