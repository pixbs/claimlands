//! The vertex order of the planet, pinned.
//!
//! `civ_core::hash` deliberately leaves the topology out of the world hash,
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

use civ_worldgen::geodesic::{LatticeKey, geodesic, vertex_count};

/// Frequency 8 — 642 tiles, the size `docs/architecture.md` uses for a level.
const FREQUENCY: u32 = 8;

/// The fingerprint of the frequency-8 vertex order and triangle list.
const SNAPSHOT: u64 = 0xea54_c44e_0454_7e1d;

#[test]
fn the_frequency_eight_vertex_order_has_not_moved() {
    let planet = geodesic(FREQUENCY);
    assert_eq!(
        planet.structure_hash(),
        SNAPSHOT,
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
