//! The regular icosahedron the planet is built from.
//!
//! Twenty triangular faces over twelve vertices, wound counter-clockwise seen
//! from outside. Everything else in this crate is a subdivision of it, so this
//! is where the planet's global symmetry — and its twelve pentagons — comes
//! from: the twelve vertices here are the only points of the finished sphere
//! with five neighbours instead of six.
//!
//! Ported from `reference/prototype/hex-planet.html`. The vertex order and the
//! face list are kept byte-for-byte identical to the prototype's, because the
//! prototype is the visual oracle: a screenshot of the two must be comparable
//! tile for tile, and reordering either table would silently renumber every
//! tile on the planet.

use crate::vec3::Vec3;

/// How many vertices an icosahedron has.
pub const VERTEX_COUNT: usize = 12;

/// How many faces an icosahedron has.
pub const FACE_COUNT: usize = 20;

/// The twenty faces, as triples of vertex indices.
///
/// Every triple is wound counter-clockwise seen from outside the sphere, so
/// `(b - a) × (c - a)` points away from the centre. The subdivision inherits
/// that winding, and the dual's corner fan depends on it.
pub const FACES: [[u8; 3]; FACE_COUNT] = [
    [0, 11, 5],
    [0, 5, 1],
    [0, 1, 7],
    [0, 7, 10],
    [0, 10, 11],
    [1, 5, 9],
    [5, 11, 4],
    [11, 10, 2],
    [10, 7, 6],
    [7, 1, 8],
    [3, 9, 4],
    [3, 4, 2],
    [3, 2, 6],
    [3, 6, 8],
    [3, 8, 9],
    [4, 9, 5],
    [2, 4, 11],
    [6, 2, 10],
    [8, 6, 7],
    [9, 8, 1],
];

/// A unit icosahedron: twelve vertices on the sphere, and the faces over them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Icosahedron {
    /// Unit-length vertex positions, in the prototype's order.
    pub vertices: [Vec3; VERTEX_COUNT],
    /// The faces, exactly [`FACES`].
    pub faces: [[u8; 3]; FACE_COUNT],
}

/// Build the unit icosahedron.
///
/// The twelve vertices are the corners of three mutually perpendicular golden
/// rectangles — `(±1, ±t, 0)`, `(0, ±1, ±t)`, `(±t, 0, ±1)` with `t` the golden
/// ratio. All twelve are the same distance from the origin, so normalising
/// them lands every one on the unit sphere without distorting the shape.
pub fn icosahedron() -> Icosahedron {
    let t = (1.0 + 5.0_f64.sqrt()) / 2.0;

    let vertices = [
        Vec3::new(-1.0, t, 0.0),
        Vec3::new(1.0, t, 0.0),
        Vec3::new(-1.0, -t, 0.0),
        Vec3::new(1.0, -t, 0.0),
        Vec3::new(0.0, -1.0, t),
        Vec3::new(0.0, 1.0, t),
        Vec3::new(0.0, -1.0, -t),
        Vec3::new(0.0, 1.0, -t),
        Vec3::new(t, 0.0, -1.0),
        Vec3::new(t, 0.0, 1.0),
        Vec3::new(-t, 0.0, -1.0),
        Vec3::new(-t, 0.0, 1.0),
    ]
    .map(Vec3::normalize);

    Icosahedron {
        vertices,
        faces: FACES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn every_vertex_sits_on_the_unit_sphere() {
        for (i, v) in icosahedron().vertices.iter().enumerate() {
            assert!(
                (v.length() - 1.0).abs() < 1e-15,
                "vertex {i} has length {}",
                v.length()
            );
        }
    }

    #[test]
    fn every_vertex_belongs_to_exactly_five_faces() {
        // This is what makes the twelve pentagons of the finished planet: a
        // tile of the dual has one side per face around its vertex.
        let mut uses = [0u32; VERTEX_COUNT];
        for face in FACES {
            for v in face {
                uses[v as usize] += 1;
            }
        }
        assert_eq!(uses, [5; VERTEX_COUNT]);
    }

    #[test]
    fn the_surface_is_closed_and_consistently_wound() {
        // Each undirected edge is shared by exactly two faces, and each
        // *directed* edge is used exactly once — which is precisely the
        // statement that all twenty faces wind the same way round.
        let mut directed = BTreeSet::new();
        let mut undirected: BTreeMap<(u8, u8), u32> = BTreeMap::new();

        for face in FACES {
            for k in 0..3 {
                let (a, b) = (face[k], face[(k + 1) % 3]);
                assert!(directed.insert((a, b)), "directed edge {a}->{b} reused");
                *undirected.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }

        assert_eq!(undirected.len(), 30, "an icosahedron has 30 edges");
        assert!(undirected.values().all(|&n| n == 2));
        // Euler's formula, as a cross-check on all three counts at once.
        assert_eq!(VERTEX_COUNT + FACE_COUNT - undirected.len(), 2);
    }

    #[test]
    fn every_face_points_outward() {
        let ico = icosahedron();
        for (f, face) in FACES.iter().enumerate() {
            let [a, b, c] = face.map(|v| ico.vertices[v as usize]);
            let normal = (b - a).cross(c - a);
            let centre = a + b + c;
            assert!(
                normal.dot(centre) > 0.0,
                "face {f} is wound the wrong way round"
            );
        }
    }
}
