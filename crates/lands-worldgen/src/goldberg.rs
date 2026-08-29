//! The Goldberg dual: every geodesic vertex becomes a tile.
//!
//! Turn the triangle mesh [`crate::geodesic`] builds inside out. Each of its
//! vertices becomes one tile of the playing surface, each of its triangles
//! becomes one tile *corner* (the triangle's centroid, pushed back out onto the
//! sphere), and two tiles are neighbours exactly when their vertices shared a
//! mesh edge. A frequency-`n` planet is `10n² + 2` tiles, twelve of them
//! pentagons — the icosahedron's own corners, the only vertices with five
//! triangles around them instead of six.
//!
//! Ported from `reference/prototype/hex-planet.html`.
//!
//! # This is the seam between worldgen and the simulation
//!
//! [`Goldberg::topology`] emits a [`Topology`], and that is the entire contract
//! with `lands-core`: neighbour lists, nothing else. No coordinate crosses that
//! line, because the simulation answers every spatial question in graph hops
//! (see `docs/adr/0004-graph-distance-not-geometry.md`). The geometry below —
//! centres, corners, tangent frames — stays here for `lands-procgen` to build
//! meshes from later, and can change without touching a line of game logic.
//!
//! What may **not** change is the numbering. Tile id `i` is geodesic vertex
//! `i`, so the dual inherits the vertex order that saved levels and golden
//! replays index into. [`Goldberg::adjacency_hash`] pins the graph the
//! simulation receives; [`Goldberg::structure_hash`] pins the geometry's
//! ordering as well.
//!
//! # Why the corner fan is walked, not sorted by angle
//!
//! The prototype orders the triangles around a vertex by `atan2` in a tangent
//! plane. That is a float comparison standing in for a combinatorial fact, and
//! this crate does not do that (see `AGENTS.md`): the triangles around a vertex
//! already form a cycle, and stepping from one to the next across a shared edge
//! walks it exactly. The walk is integer arithmetic, needs no tolerance, and
//! gives the same answer on every target — and it turns out to be simpler than
//! the sort it replaces.

use crate::digest::Digest;
use crate::geodesic::{Geodesic, geodesic};
use crate::vec3::Vec3;
use lands_core::prelude::{TileId, Topology};

/// The most sides a tile can have.
///
/// A geodesic vertex belongs to six triangles, or to five if it is one of the
/// icosahedron's own corners. There is no third case at any frequency, so a
/// tile's corners and neighbours fit in a fixed array and the dual allocates
/// nothing per tile.
pub const MAX_SIDES: usize = 6;

/// One cell of the Goldberg polyhedron: a pentagon or a hexagon.
///
/// Called a cell rather than a tile because `lands_core::state::Tile` is the
/// same tile seen as game state — who owns it, what stands on it. This is the
/// same tile seen as geometry, and the two never meet: the only thing that
/// crosses from here into the simulation is [`Goldberg::topology`].
///
/// Corners and neighbours are aligned and both run counter-clockwise seen from
/// outside the sphere: `neighbors()[k]` is the tile across the edge from
/// `corners()[k]` to `corners()[k + 1]`, wrapping at the end. That alignment is
/// what lets a territory be drawn as one outline instead of six tile borders.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// Unit-length tile centre — the geodesic vertex this tile came from.
    pub center: Vec3,
    /// First tangent axis, aimed at `corners()[0]`, so a tile's texture keeps
    /// a stable orientation relative to its own geometry.
    pub e1: Vec3,
    /// Second tangent axis. `(e1, e2, center)` is right-handed.
    pub e2: Vec3,
    corners: [u32; MAX_SIDES],
    neighbors: [u32; MAX_SIDES],
    sides: u8,
}

impl Cell {
    /// Five for a pentagon, six for a hexagon.
    #[inline]
    pub fn sides(&self) -> usize {
        self.sides as usize
    }

    /// Corner ids, counter-clockwise seen from outside.
    ///
    /// Index [`Goldberg::corners`] with these for positions, or
    /// [`Goldberg::corner_cells`] for the three tiles that meet there.
    #[inline]
    pub fn corners(&self) -> &[u32] {
        &self.corners[..self.sides()]
    }

    /// Neighbouring tile ids, aligned with [`Self::corners`] — one per edge,
    /// counter-clockwise seen from outside.
    ///
    /// This is geometric order, not ascending order:
    /// [`Goldberg::topology`] sorts before handing the graph to `lands-core`.
    #[inline]
    pub fn neighbors(&self) -> &[u32] {
        &self.neighbors[..self.sides()]
    }

    /// Whether this is one of the twelve pentagons.
    #[inline]
    pub fn is_pentagon(&self) -> bool {
        self.sides == 5
    }

    /// The neighbours, ascending — what `lands-core` wants.
    fn sorted_neighbors(&self) -> ([u32; MAX_SIDES], usize) {
        let mut out = self.neighbors;
        let sides = self.sides();
        out[..sides].sort_unstable();
        (out, sides)
    }
}

/// A planet: the dual of a geodesic sphere, one tile per geodesic vertex.
///
/// Build one with [`goldberg`], or from an existing mesh with [`dual`].
#[derive(Debug, Clone, PartialEq)]
pub struct Goldberg {
    frequency: u32,
    cells: Vec<Cell>,
    corners: Vec<Vec3>,
    corner_cells: Vec<[u32; 3]>,
}

impl Goldberg {
    /// The subdivision frequency the underlying geodesic was built at.
    pub fn frequency(&self) -> u32 {
        self.frequency
    }

    /// Every tile, indexed by tile id: `10n² + 2` of them.
    pub fn cells(&self) -> &[Cell] {
        &self.cells
    }

    /// One tile, by id.
    pub fn cell(&self, tile: TileId) -> &Cell {
        &self.cells[tile.index()]
    }

    /// How many tiles the planet has.
    pub fn tile_count(&self) -> usize {
        self.cells.len()
    }

    /// Unit-length corner positions, indexed by corner id: `20n²` of them, one
    /// per geodesic triangle.
    pub fn corners(&self) -> &[Vec3] {
        &self.corners
    }

    /// The three tiles meeting at each corner, parallel to [`Self::corners`].
    ///
    /// A corner is the centroid of one geodesic triangle, and that triangle's
    /// three vertices *are* those three tiles.
    pub fn corner_cells(&self) -> &[[u32; 3]] {
        &self.corner_cells
    }

    /// The twelve pentagons, ascending.
    ///
    /// They sit at the icosahedron's own corners and nowhere else, at every
    /// frequency. Worth knowing about downstream: hop distance is slightly
    /// anisotropic around them, and a mesh builder may want to special-case
    /// their fan.
    pub fn pentagons(&self) -> impl Iterator<Item = TileId> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.is_pentagon())
            .map(|(i, _)| TileId(i as u32))
    }

    /// The adjacency graph, which is everything `lands-core` is told about the
    /// planet.
    ///
    /// Neighbour lists are sorted before they are handed over. `Topology::new`
    /// would sort them anyway, but a caller that compares or fingerprints this
    /// input should see a stable order rather than one that depends on where
    /// each tile's corner fan happened to start.
    pub fn topology(&self) -> Topology {
        let neighbors = self
            .cells
            .iter()
            .map(|cell| {
                let (sorted, sides) = cell.sorted_neighbors();
                sorted[..sides].iter().map(|&n| TileId(n)).collect()
            })
            .collect();

        Topology::new(neighbors)
            .expect("the dual of a closed, consistently wound mesh is always a valid topology")
    }

    /// A fingerprint of the adjacency alone — the half of this crate's output
    /// that reaches the simulation.
    ///
    /// Deliberately independent of the corner fan, so that a change to the
    /// geometry and a change to the graph fail different assertions. This one
    /// moving means stored levels and golden replays now describe a planet
    /// whose tiles have different neighbours.
    ///
    /// Taken with [`crate::digest`], which explains the choice of hash.
    pub fn adjacency_hash(&self) -> u64 {
        let mut d = Digest::new();
        d.u32(self.cells.len() as u32);
        for cell in &self.cells {
            let (sorted, sides) = cell.sorted_neighbors();
            d.byte(sides as u8);
            for &n in &sorted[..sides] {
                d.u32(n);
            }
        }
        d.finish()
    }

    /// A fingerprint of the whole combinatorial structure: the adjacency, plus
    /// the corner fans and which tiles meet at each corner.
    ///
    /// Coordinates are not hashed, for the reason [`crate::digest`] gives.
    /// This moving while [`Self::adjacency_hash`] holds still means the tiling
    /// is the same and only its ordering moved — which renumbers corners for
    /// `lands-procgen` but leaves every saved game valid.
    pub fn structure_hash(&self) -> u64 {
        let mut d = Digest::new();
        d.u32(self.frequency);

        d.u32(self.cells.len() as u32);
        for cell in &self.cells {
            d.byte(cell.sides);
            for &corner in cell.corners() {
                d.u32(corner);
            }
            for &neighbor in cell.neighbors() {
                d.u32(neighbor);
            }
        }

        d.u32(self.corner_cells.len() as u32);
        for corner in &self.corner_cells {
            for &tile in corner {
                d.u32(tile);
            }
        }

        d.finish()
    }
}

/// Build a frequency-`n` planet: subdivide, project, then take the dual.
///
/// # Panics
///
/// If `frequency` is out of range, with the message [`geodesic`] gives.
pub fn goldberg(frequency: u32) -> Goldberg {
    dual(&geodesic(frequency))
}

/// Take the dual of an already-built geodesic mesh.
///
/// Tile `i` is mesh vertex `i` and corner `t` is mesh triangle `t`, so both
/// numberings are inherited rather than invented.
pub fn dual(mesh: &Geodesic) -> Goldberg {
    let vertices = mesh.vertices();
    let triangles = mesh.triangles();

    // A tile corner is a triangle's centroid pushed back out onto the sphere.
    // Summing the three vertices and normalising is that, without the divide
    // by three: normalising discards the scale, so the division would only add
    // a rounding step.
    let corners: Vec<Vec3> = triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| vertices[v as usize]);
            (a + b + c).normalize()
        })
        .collect();

    // The triangles touching each vertex, in triangle order. Unordered so far
    // — the fan walk below puts them in a ring.
    let mut rings = vec![Ring::EMPTY; vertices.len()];
    for (t, triangle) in triangles.iter().enumerate() {
        for &v in triangle {
            rings[v as usize].push(t as u32);
        }
    }

    let cells = (0..vertices.len())
        .map(|v| build_cell(v as u32, &rings[v], vertices, triangles, &corners))
        .collect();

    Goldberg {
        frequency: mesh.frequency(),
        cells,
        corners,
        corner_cells: triangles.to_vec(),
    }
}

/// One tile: its corner fan in order, the neighbour across each edge, and the
/// tangent frame the fan anchors.
fn build_cell(
    v: u32,
    ring: &Ring,
    vertices: &[Vec3],
    triangles: &[[u32; 3]],
    corners: &[Vec3],
) -> Cell {
    let sides = ring.len();
    assert!(
        sides == 5 || sides == 6,
        "vertex {v} has {sides} triangles around it; a geodesic vertex has five or six"
    );

    let mut fan = [0u32; MAX_SIDES];
    let mut neighbors = [0u32; MAX_SIDES];
    let mut walked = [false; MAX_SIDES];

    // Start at the lowest-numbered triangle around the vertex. Any starting
    // point gives the same ring; this one is the only choice that does not
    // depend on a coordinate, so it is the one that is the same everywhere.
    let mut slot = 0;

    for k in 0..sides {
        let t = ring.triangles[slot];
        walked[slot] = true;
        fan[k] = t;

        // The vertex before `v` in this triangle's winding. Leaving across the
        // edge `(v, before)` turns counter-clockwise around `v` seen from
        // outside — the triangles are wound counter-clockwise, so `before` is
        // the more counter-clockwise of the two other corners. That makes
        // `before` the tile on the far side of the edge between this corner
        // and the next, which is exactly what a neighbour list wants.
        let before = preceding(triangles[t as usize], v);
        neighbors[k] = before;

        if k + 1 < sides {
            slot = (0..sides)
                .find(|&s| !walked[s] && triangles[ring.triangles[s] as usize].contains(&before))
                .expect("the mesh is closed, so the edge leaving this triangle has a second one");
        }
    }

    debug_assert!(
        triangles[fan[0] as usize].contains(&neighbors[sides - 1]),
        "the fan around vertex {v} did not close back onto its first triangle"
    );

    // Tangent frame, anchored at the first corner. `e1` is that corner with
    // its radial component removed, so it lies in the tangent plane.
    let center = vertices[v as usize];
    let first = corners[fan[0] as usize];
    let e1 = (first - center * first.dot(center)).normalize();
    let e2 = center.cross(e1);

    Cell {
        center,
        e1,
        e2,
        corners: fan,
        neighbors,
        sides: sides as u8,
    }
}

/// The vertex before `v` in a triangle's winding.
fn preceding(triangle: [u32; 3], v: u32) -> u32 {
    let k = triangle
        .iter()
        .position(|&x| x == v)
        .expect("the triangle came from this vertex's own ring");
    triangle[(k + 2) % 3]
}

/// The triangles around one vertex: five or six, never more.
#[derive(Debug, Clone, Copy)]
struct Ring {
    triangles: [u32; MAX_SIDES],
    len: u8,
}

impl Ring {
    const EMPTY: Self = Self {
        triangles: [0; MAX_SIDES],
        len: 0,
    };

    fn len(&self) -> usize {
        self.len as usize
    }

    fn push(&mut self, triangle: u32) {
        assert!(
            self.len() < MAX_SIDES,
            "a geodesic vertex has at most {MAX_SIDES} triangles around it"
        );
        self.triangles[self.len()] = triangle;
        self.len += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geodesic::{LatticeKey, triangle_count, vertex_count};
    use std::collections::{BTreeMap, BTreeSet};

    /// The sizes the game actually uses, plus the two degenerate ends.
    const SIZES: [u32; 4] = [1, 2, 8, 12];

    #[test]
    fn a_planet_has_ten_n_squared_plus_two_tiles() {
        for n in 1..=12 {
            let planet = goldberg(n);
            assert_eq!(
                planet.tile_count() as u32,
                10 * n * n + 2,
                "tile count at n={n}"
            );
            assert_eq!(planet.tile_count(), vertex_count(n) as usize);
            assert_eq!(planet.corners().len(), triangle_count(n) as usize);
        }

        // Spot values, spelled out so a wrong formula cannot agree with itself.
        assert_eq!(goldberg(1).tile_count(), 12);
        assert_eq!(goldberg(2).tile_count(), 42);
        assert_eq!(goldberg(8).tile_count(), 642);
        assert_eq!(goldberg(12).tile_count(), 1442);
    }

    #[test]
    fn exactly_twelve_tiles_are_pentagons_at_every_frequency() {
        for n in 1..=12 {
            let planet = goldberg(n);
            assert_eq!(planet.pentagons().count(), 12, "pentagon count at n={n}");
            assert!(
                planet
                    .cells()
                    .iter()
                    .all(|c| c.sides() == 5 || c.sides() == 6),
                "n={n} has a tile that is neither a pentagon nor a hexagon"
            );
        }
    }

    #[test]
    fn the_pentagons_are_the_icosahedron_corners_and_nothing_else() {
        // A pentagon count of twelve is cheap to satisfy by accident — a mesh
        // with a hole in it can produce twelve low-degree tiles too. This says
        // *which* tiles they are: exactly the vertices the subdivision
        // identified as icosahedron corners, which is the only place five
        // triangles can meet.
        for n in SIZES {
            let mesh = geodesic(n);
            let planet = dual(&mesh);

            let from_geometry: BTreeSet<u32> = planet.pentagons().map(|t| t.0).collect();
            let from_identity: BTreeSet<u32> = mesh
                .keys()
                .iter()
                .enumerate()
                .filter(|(_, key)| matches!(key, LatticeKey::Corner(_)))
                .map(|(i, _)| i as u32)
                .collect();

            assert_eq!(from_geometry, from_identity, "n={n}");
            assert_eq!(from_geometry.len(), 12);
        }
    }

    #[test]
    fn tile_ids_are_geodesic_vertex_ids() {
        // The numbering is the contract (see `AGENTS.md`): a saved level's
        // `Tile(id: 214)` is geodesic vertex 214, so the dual must not permute
        // anything on its way through.
        let mesh = geodesic(8);
        let planet = dual(&mesh);
        for (i, cell) in planet.cells().iter().enumerate() {
            assert_eq!(cell.center, mesh.vertices()[i], "tile {i} moved");
        }
    }

    #[test]
    fn adjacency_is_symmetric_with_no_self_loops_or_repeats() {
        for n in SIZES {
            let planet = goldberg(n);
            for (i, cell) in planet.cells().iter().enumerate() {
                let i = i as u32;
                let distinct: BTreeSet<u32> = cell.neighbors().iter().copied().collect();
                assert_eq!(
                    distinct.len(),
                    cell.sides(),
                    "n={n} tile {i} repeats a neighbour"
                );
                assert!(
                    !distinct.contains(&i),
                    "n={n} tile {i} is its own neighbour"
                );

                for &j in cell.neighbors() {
                    assert!(
                        planet.cells()[j as usize].neighbors().contains(&i),
                        "n={n}: {i} lists {j} but {j} does not list {i}"
                    );
                }
            }
        }
    }

    #[test]
    fn neighbours_are_exactly_the_tiles_sharing_a_mesh_edge() {
        // The dual's edges must be the mesh's edges and no others — the fan
        // walk could otherwise skip a triangle and still close up.
        let mesh = geodesic(8);
        let planet = dual(&mesh);

        let mut from_mesh: Vec<BTreeSet<u32>> = vec![BTreeSet::new(); mesh.vertices().len()];
        for t in mesh.triangles() {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                from_mesh[a as usize].insert(b);
                from_mesh[b as usize].insert(a);
            }
        }

        for (i, cell) in planet.cells().iter().enumerate() {
            let from_dual: BTreeSet<u32> = cell.neighbors().iter().copied().collect();
            assert_eq!(from_dual, from_mesh[i], "tile {i}");
        }
    }

    #[test]
    fn every_corner_is_shared_by_exactly_three_tiles() {
        // Three tiles meet at a corner on any Goldberg polyhedron. A corner
        // used twice would be a crack in the surface; four would be a fold.
        for n in SIZES {
            let mesh = geodesic(n);
            let planet = dual(&mesh);

            let mut uses: BTreeMap<u32, BTreeSet<u32>> = BTreeMap::new();
            for (i, cell) in planet.cells().iter().enumerate() {
                for &corner in cell.corners() {
                    uses.entry(corner).or_default().insert(i as u32);
                }
            }

            assert_eq!(uses.len(), planet.corners().len(), "n={n}: unused corner");
            for (corner, tiles) in &uses {
                let expected: BTreeSet<u32> = planet.corner_cells()[*corner as usize]
                    .iter()
                    .copied()
                    .collect();
                assert_eq!(tiles, &expected, "n={n} corner {corner}");
                assert_eq!(tiles.len(), 3);
            }
        }
    }

    #[test]
    fn the_tiling_closes_the_sphere() {
        // Euler's formula over the dual itself. It comes out at 2 only if
        // every tile is a closed ring of corners and every edge is shared by
        // exactly two tiles — that is, only if there is no seam.
        for n in SIZES {
            let planet = goldberg(n);

            let mut edges: BTreeMap<(u32, u32), u32> = BTreeMap::new();
            for (i, cell) in planet.cells().iter().enumerate() {
                for &j in cell.neighbors() {
                    let (a, b) = (i as u32, j);
                    *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            assert!(
                edges.values().all(|&uses| uses == 2),
                "n={n} has an edge that is not shared by exactly two tiles"
            );

            let (f, e, v) = (planet.tile_count(), edges.len(), planet.corners().len());
            assert_eq!(v + f - e, 2, "n={n}: V={v} E={e} F={f}");
            assert_eq!(e as u32, 30 * n * n);

            // Every corner belongs to three tiles, so the sides sum to three
            // corners each: 12 pentagons and 10n²-10 hexagons give 60n².
            let sides: usize = planet.cells().iter().map(Cell::sides).sum();
            assert_eq!(sides, 3 * planet.corners().len());
            assert_eq!(sides as u32, 60 * n * n);
        }
    }

    #[test]
    fn every_tile_winds_counter_clockwise_seen_from_outside() {
        // The fan walk claims a direction; this is the claim. `lands-procgen`
        // will triangulate these fans, and a tile wound the wrong way would be
        // back-face culled — one hole in the planet per tile.
        let planet = goldberg(8);
        for (i, cell) in planet.cells().iter().enumerate() {
            let sides = cell.sides();
            for k in 0..sides {
                let a = planet.corners()[cell.corners()[k] as usize] - cell.center;
                let b = planet.corners()[cell.corners()[(k + 1) % sides] as usize] - cell.center;
                assert!(
                    a.cross(b).dot(cell.center) > 0.0,
                    "tile {i} turns the wrong way between corners {k} and {}",
                    (k + 1) % sides
                );
            }
        }
    }

    #[test]
    fn the_neighbour_across_an_edge_is_the_tile_the_two_corners_agree_on() {
        // `neighbors()[k]` must be the tile across the edge joining corner `k`
        // to corner `k+1`. Those corners are the centroids of two triangles
        // sharing an edge of the mesh, and the neighbour is the vertex they
        // have in common besides this tile.
        let planet = goldberg(8);
        for (i, cell) in planet.cells().iter().enumerate() {
            let sides = cell.sides();
            for k in 0..sides {
                let here: BTreeSet<u32> = planet.corner_cells()[cell.corners()[k] as usize]
                    .iter()
                    .copied()
                    .collect();
                let next: BTreeSet<u32> = planet.corner_cells()
                    [cell.corners()[(k + 1) % sides] as usize]
                    .iter()
                    .copied()
                    .collect();

                let shared: Vec<u32> = here
                    .intersection(&next)
                    .copied()
                    .filter(|&t| t != i as u32)
                    .collect();
                assert_eq!(shared, vec![cell.neighbors()[k]], "tile {i} edge {k}");
            }
        }
    }

    #[test]
    fn every_corner_sits_on_the_unit_sphere() {
        for n in SIZES {
            for (t, corner) in goldberg(n).corners().iter().enumerate() {
                assert!(
                    (corner.length() - 1.0).abs() < 1e-12,
                    "n={n} corner {t} has length {}",
                    corner.length()
                );
            }
        }
    }

    #[test]
    fn the_tangent_frame_is_orthonormal_and_right_handed() {
        let planet = goldberg(8);
        for (i, cell) in planet.cells().iter().enumerate() {
            assert!((cell.e1.length() - 1.0).abs() < 1e-12, "tile {i} e1");
            assert!((cell.e2.length() - 1.0).abs() < 1e-12, "tile {i} e2");
            assert!(cell.e1.dot(cell.center).abs() < 1e-12, "tile {i} e1 tilts");
            assert!(cell.e2.dot(cell.center).abs() < 1e-12, "tile {i} e2 tilts");
            assert!(cell.e1.dot(cell.e2).abs() < 1e-12, "tile {i} frame is skew");
            // (e1, e2, center) right-handed: e1 x e2 points straight out.
            assert!((cell.e1.cross(cell.e2) - cell.center).length() < 1e-12);
        }
    }

    #[test]
    fn the_first_corner_anchors_the_frame() {
        // `e1` aims at corner 0, which is what gives a tile's texture a stable
        // orientation relative to its own geometry rather than to the world.
        let planet = goldberg(8);
        for cell in planet.cells() {
            let first = planet.corners()[cell.corners()[0] as usize];
            let tangent = (first - cell.center * first.dot(cell.center)).normalize();
            assert!((tangent - cell.e1).length() < 1e-12);
        }
    }

    #[test]
    fn the_topology_is_accepted_and_matches_the_tiling() {
        for n in SIZES {
            let planet = goldberg(n);
            let topology = planet.topology();

            assert_eq!(topology.tile_count(), planet.tile_count(), "n={n}");
            for (i, cell) in planet.cells().iter().enumerate() {
                let listed = topology.neighbors(TileId(i as u32));
                assert_eq!(listed.len(), cell.sides(), "n={n} tile {i} degree");

                let mut expected: Vec<TileId> =
                    cell.neighbors().iter().map(|&j| TileId(j)).collect();
                expected.sort_unstable();
                assert_eq!(listed, expected, "n={n} tile {i}");
            }
        }
    }

    #[test]
    fn the_neighbour_lists_handed_over_are_already_sorted() {
        // `Topology::new` sorts, but the input should not need it: anything
        // that compares or hashes this graph before it is built should see a
        // stable order rather than wherever each corner fan started.
        for n in SIZES {
            for cell in goldberg(n).cells() {
                let (sorted, sides) = cell.sorted_neighbors();
                assert!(
                    sorted[..sides].windows(2).all(|w| w[0] < w[1]),
                    "n={n}: neighbours out of order"
                );
            }
        }
    }

    #[test]
    fn the_smallest_planet_is_the_icosahedron_the_testkit_hand_wrote() {
        // `topo::icosahedron()` is the twelve-pentagon world every closed-
        // surface test in `lands-core` runs on, written out by hand straight
        // from the face table. The n=1 dual must be the same graph — the
        // cheapest possible proof that this code builds the shape it claims.
        //
        // The same graph, but **not** the same numbering. Tile ids here are
        // geodesic vertex ids, handed out in the order the face walk first
        // reaches each point, and that is a permutation of the icosahedron's
        // own corner numbering rather than the identity. So a tile id means a
        // different tile in the two, and the hardcoded ids in
        // `lands-core/tests/closed_surface.rs` are the testkit's, not a real
        // planet's. Map through the permutation and they agree edge for edge.
        let mesh = geodesic(1);
        let topology = dual(&mesh).topology();
        let hand_written = lands_testkit::topo::icosahedron();

        let corner_of = |tile: TileId| match mesh.keys()[tile.index()] {
            LatticeKey::Corner(c) => TileId(u32::from(c)),
            other => panic!("n=1 is corners only, got {other:?}"),
        };

        assert_eq!(topology.tile_count(), hand_written.tile_count());
        for tile in topology.tiles() {
            let mut mapped: Vec<TileId> = topology
                .neighbors(tile)
                .iter()
                .map(|&n| corner_of(n))
                .collect();
            mapped.sort_unstable();
            assert_eq!(
                mapped,
                hand_written.neighbors(corner_of(tile)),
                "tile {tile} is icosahedron corner {}",
                corner_of(tile)
            );
        }
    }

    #[test]
    fn a_planet_is_the_same_planet_every_time_it_is_built() {
        for n in [2, 8] {
            let (a, b) = (goldberg(n), goldberg(n));
            assert_eq!(a, b);
            assert_eq!(a.structure_hash(), b.structure_hash());
            assert_eq!(a.adjacency_hash(), b.adjacency_hash());
        }
    }

    #[test]
    fn the_fingerprints_tell_frequencies_apart() {
        let structures: BTreeSet<u64> = (1..=12).map(|n| goldberg(n).structure_hash()).collect();
        let graphs: BTreeSet<u64> = (1..=12).map(|n| goldberg(n).adjacency_hash()).collect();
        assert_eq!(structures.len(), 12);
        assert_eq!(graphs.len(), 12);
    }

    #[test]
    #[should_panic(expected = "geodesic frequency must be")]
    fn an_out_of_range_frequency_is_rejected_by_the_mesh_it_asks_for() {
        let _ = goldberg(0);
    }
}
