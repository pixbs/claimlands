//! The vertex order of the planet, pinned.
//!
//! `lands_core::hash` deliberately leaves the topology out of the world hash,
//! because it is fixed for a whole match — "covered by the worldgen snapshot
//! tests", it says. This is that test.
//!
//! What it guards is not the geometry but the **numbering**. A level stores
//! `Tile(id: 214, ...)`; a golden replay stores a command log full of tile
//! ids. Both are indices into the order `geodesic` emits vertices in. Any
//! change to the face order, the lattice walk or the dedup renumbers them, and
//! every stored level and replay quietly starts describing a different planet.
//! The failure would surface far from its cause, so it is caught here.
//!
//! If this value moves, the question to answer is not "what is the new hash"
//! but "which saved planets did I just invalidate".
//!
//! Three fingerprints, deliberately separate, because they fail for different
//! reasons and cost different amounts to fix:
//!
//! * [`MESH`] — the geodesic's vertex order and triangle list.
//! * [`ADJACENCY`] — the tile graph handed to `lands-core`. This is the one
//!   that invalidates saved levels and golden replays.
//! * [`STRUCTURE`] — the whole dual, corner fans included. It can move while
//!   `ADJACENCY` holds still, which means the tiling is unchanged and only the
//!   geometry was renumbered: `lands-procgen` cares, saved games do not.

use lands_core::prelude::TileId;
use lands_worldgen::geodesic::{LatticeKey, geodesic, vertex_count};
use lands_worldgen::goldberg::goldberg;

/// Frequency 8 — 642 tiles, the size `docs/architecture.md` uses for a level.
const FREQUENCY: u32 = 8;

/// The fingerprint of the frequency-8 vertex order and triangle list.
const MESH: u64 = 0xea54_c44e_0454_7e1d;

/// The fingerprint of the tile graph the simulation is given.
const ADJACENCY: u64 = 0x4396_81c1_06a3_3e7e;

/// The fingerprint of the whole dual: adjacency, corner fans and corner tiles.
const STRUCTURE: u64 = 0x6a48_f634_d146_3e2e;

#[test]
fn the_frequency_eight_vertex_order_has_not_moved() {
    let planet = geodesic(FREQUENCY);
    assert_eq!(
        planet.structure_hash(),
        MESH,
        "the vertex order at n={FREQUENCY} changed; every stored tile id now \
         refers to a different tile"
    );
}

#[test]
fn the_snapshot_describes_the_planet_it_claims_to() {
    // A hash alone cannot say what it hashed. These pin the shape the number
    // above is a fingerprint *of*, so a reader can tell at a glance whether a
    // moved hash means "renumbered" or "rebuilt entirely".
    let planet = geodesic(FREQUENCY);
    assert_eq!(planet.frequency(), FREQUENCY);
    assert_eq!(planet.vertices().len(), vertex_count(FREQUENCY) as usize);
    assert_eq!(planet.vertices().len(), 642);
    assert_eq!(planet.triangles().len(), 1280);

    // The first vertex is always the icosahedron corner that opens face 0,
    // and the last of the first 12 is a corner too — the twelve pentagons are
    // not clustered at the front, they are found as the walk reaches them.
    assert_eq!(planet.keys()[0], LatticeKey::Corner(0));
    assert_eq!(planet.keys()[8], LatticeKey::Corner(5));

    // Face 0's first triangle: its A corner, one step toward B, one toward C.
    // The middle index is 9 rather than 1 because the walk lays down the whole
    // A-C edge (ids 1..=8) before it starts the row that leaves A toward B.
    assert_eq!(planet.triangles()[0], [0, 9, 1]);
}

#[test]
fn rebuilding_the_planet_reproduces_the_snapshot() {
    // Guards against an ordering that depends on allocator addresses or hash
    // seeds rather than on the frequency — the class of bug a single run of
    // the test above cannot see.
    let first = geodesic(FREQUENCY);
    for _ in 0..4 {
        let again = geodesic(FREQUENCY);
        assert_eq!(again.structure_hash(), first.structure_hash());
        assert_eq!(again.keys(), first.keys());
        assert_eq!(again.triangles(), first.triangles());
    }
}

#[test]
fn the_adjacency_handed_to_the_simulation_has_not_moved() {
    // The seam between worldgen and the simulation. A level stores tile ids and
    // a golden replay stores a command log full of them; both are meaningless
    // if a tile's neighbours change underneath them. This is the value to guard
    // hardest — it is the one whose movement invalidates stored games.
    let planet = goldberg(FREQUENCY);
    assert_eq!(
        planet.adjacency_hash(),
        ADJACENCY,
        "the tile graph at n={FREQUENCY} changed; every stored level and \
         replay now describes a planet whose tiles have different neighbours"
    );
}

#[test]
fn the_dual_structure_has_not_moved() {
    let planet = goldberg(FREQUENCY);
    assert_eq!(
        planet.structure_hash(),
        STRUCTURE,
        "the dual's ordering at n={FREQUENCY} changed; if `ADJACENCY` still \
         holds, only the corner numbering moved and no saved game is affected"
    );
}

#[test]
fn the_dual_snapshot_describes_the_planet_it_claims_to() {
    // As above: a hash cannot say what it hashed, so pin the shape too.
    let planet = goldberg(FREQUENCY);
    assert_eq!(planet.frequency(), FREQUENCY);
    assert_eq!(planet.tile_count(), 642);
    assert_eq!(planet.corners().len(), 1280, "one corner per mesh triangle");
    assert_eq!(planet.pentagons().count(), 12);

    // Tile 0 is icosahedron corner 0, so it is a pentagon, and its five corners
    // are the first triangle of each of the five faces meeting there — face `f`
    // opens at triangle `f * 64` at this frequency.
    let first = planet.cell(TileId(0));
    assert!(first.is_pentagon());
    assert_eq!(first.corners(), [0, 64, 128, 192, 256]);
    // Its neighbours run round the fan, not in ascending order; `topology()` is
    // what sorts them.
    assert_eq!(first.neighbors(), [1, 45, 81, 117, 9]);
    assert_eq!(
        planet.topology().neighbors(TileId(0)),
        [1, 9, 45, 81, 117].map(TileId)
    );

    // Corner 0 is the centroid of mesh triangle 0, so the three tiles meeting
    // there are that triangle's vertices — the same `[0, 9, 1]` the mesh
    // snapshot above pins.
    assert_eq!(planet.corner_cells()[0], [0, 9, 1]);
}

#[test]
fn rebuilding_the_dual_reproduces_the_snapshot() {
    let first = goldberg(FREQUENCY);
    for _ in 0..4 {
        let again = goldberg(FREQUENCY);
        assert_eq!(again.adjacency_hash(), first.adjacency_hash());
        assert_eq!(again.structure_hash(), first.structure_hash());
        assert_eq!(again.cells(), first.cells());
    }
}
