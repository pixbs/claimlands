//! Small, legible topologies for tests.
//!
//! The real planet is a Goldberg polyhedron with 42 to 1442 tiles, which is
//! useless for reasoning about a failing rule. These give a test a shape you
//! can draw on paper: a line to reason about distance, a hex grid to reason
//! about territories splitting.

use lands_core::prelude::{TileId, Topology};

/// `n` tiles in a row: `0 — 1 — 2 — … — (n-1)`.
///
/// The clearest shape for anything about distance, reachability, or a
/// territory being cut in half.
pub fn line(n: u32) -> Topology {
    let neighbors = (0..n)
        .map(|i| {
            let mut v = Vec::new();
            if i > 0 {
                v.push(TileId(i - 1));
            }
            if i + 1 < n {
                v.push(TileId(i + 1));
            }
            v
        })
        .collect();
    Topology::new(neighbors).expect("a line is always a valid topology")
}

/// A `width` × `height` grid with four-way adjacency.
///
/// Tile ids run left to right, top to bottom: `id = row * width + col`.
pub fn grid(width: u32, height: u32) -> Topology {
    let id = |c: u32, r: u32| TileId(r * width + c);
    let mut neighbors = vec![Vec::new(); (width * height) as usize];

    for r in 0..height {
        for c in 0..width {
            let mut v = Vec::new();
            if c > 0 {
                v.push(id(c - 1, r));
            }
            if c + 1 < width {
                v.push(id(c + 1, r));
            }
            if r > 0 {
                v.push(id(c, r - 1));
            }
            if r + 1 < height {
                v.push(id(c, r + 1));
            }
            neighbors[id(c, r).index()] = v;
        }
    }
    Topology::new(neighbors).expect("a grid is always a valid topology")
}

/// A `width` × `height` hex grid in odd-row offset layout, six-way adjacency.
///
/// Closer to the real planet than [`grid`] — a hex tile has six neighbours and
/// the offset rows make territory shapes behave realistically — while still
/// being small enough to reason about. Ids run `row * width + col`.
pub fn hex_grid(width: u32, height: u32) -> Topology {
    let id = |c: u32, r: u32| TileId(r * width + c);
    let mut neighbors = vec![Vec::new(); (width * height) as usize];

    for r in 0..height {
        for c in 0..width {
            // Odd rows are shifted half a tile right, so the diagonal
            // neighbours sit at different column offsets.
            let diagonals: [(i64, i64); 4] = if r % 2 == 0 {
                [(-1, -1), (0, -1), (-1, 1), (0, 1)]
            } else {
                [(0, -1), (1, -1), (0, 1), (1, 1)]
            };

            let mut v = Vec::new();
            for (dc, dr) in [(-1i64, 0i64), (1, 0)].into_iter().chain(diagonals) {
                let nc = c as i64 + dc;
                let nr = r as i64 + dr;
                if nc >= 0 && nc < width as i64 && nr >= 0 && nr < height as i64 {
                    v.push(id(nc as u32, nr as u32));
                }
            }
            neighbors[id(c, r).index()] = v;
        }
    }
    Topology::new(neighbors).expect("a hex grid is always a valid topology")
}

/// The smallest real planet: the dual of an icosahedron.
///
/// Twelve tiles, every one a pentagon with exactly five neighbours. This is
/// literally the `n = 1` case of the game's Goldberg polyhedron
/// (`10n² + 2 = 12`), so it is the cheapest possible **closed** surface that is
/// also the genuine article.
///
/// Use it whenever a test needs to prove something holds on a world with no
/// edges. [`line`], [`grid`] and [`hex_grid`] all have boundaries, which can
/// hide a bug that only appears where a map wraps around.
pub fn icosahedron() -> Topology {
    // The twenty faces of an icosahedron, as vertex triples. Tiles are the
    // *vertices* (the dual), so two tiles are adjacent when they share an edge
    // of some face.
    const FACES: [[u32; 3]; 20] = [
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

    let mut neighbors = vec![Vec::new(); 12];
    for face in FACES {
        for i in 0..3 {
            let (a, b) = (face[i], face[(i + 1) % 3]);
            neighbors[a as usize].push(TileId(b));
            neighbors[b as usize].push(TileId(a));
        }
    }
    // `Topology::new` sorts and dedups, so the duplicates from shared edges
    // are harmless.
    Topology::new(neighbors).expect("the icosahedron is a valid topology")
}

/// A `width` × `height` grid that wraps in **both** directions — a closed
/// surface with no boundary, four neighbours per tile.
///
/// A sphere is awkward to build at arbitrary sizes; a torus is not, and for
/// the property that matters here — *there is no edge of the map* — the two are
/// equivalent. Use this to fuzz territory logic on a world where a region can
/// encircle the planet.
///
/// Both dimensions must be at least 3, or a tile would be its own neighbour.
pub fn torus(width: u32, height: u32) -> Topology {
    assert!(
        width >= 3 && height >= 3,
        "a torus smaller than 3x3 would make a tile adjacent to itself"
    );

    let id = |c: u32, r: u32| TileId(r * width + c);
    let mut neighbors = vec![Vec::new(); (width * height) as usize];

    for r in 0..height {
        for c in 0..width {
            neighbors[id(c, r).index()] = vec![
                id((c + width - 1) % width, r),
                id((c + 1) % width, r),
                id(c, (r + height - 1) % height),
                id(c, (r + 1) % height),
            ];
        }
    }
    Topology::new(neighbors).expect("a torus is always a valid topology")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// A surface is closed when no tile has fewer neighbours than any other:
    /// there is nowhere that the world stops.
    fn is_closed(t: &Topology) -> bool {
        let degrees: BTreeSet<usize> = t.tiles().map(|id| t.neighbors(id).len()).collect();
        degrees.len() == 1
    }

    #[test]
    fn line_ends_have_one_neighbour() {
        let t = line(5);
        assert_eq!(t.neighbors(TileId(0)).len(), 1);
        assert_eq!(t.neighbors(TileId(4)).len(), 1);
        assert_eq!(t.neighbors(TileId(2)).len(), 2);
    }

    #[test]
    fn grid_interior_has_four_neighbours() {
        let t = grid(4, 4);
        assert_eq!(t.neighbors(TileId(5)).len(), 4);
        assert_eq!(t.neighbors(TileId(0)).len(), 2);
    }

    #[test]
    fn hex_grid_interior_has_six_neighbours() {
        let t = hex_grid(5, 5);
        // Tile (2,2) is fully surrounded.
        assert_eq!(t.neighbors(TileId(12)).len(), 6);
    }

    #[test]
    fn hex_grid_adjacency_is_symmetric_for_both_row_parities() {
        // Topology::new validates symmetry, so construction succeeding is the
        // assertion. Both parities are exercised by an odd height.
        let _ = hex_grid(6, 5);
        let _ = hex_grid(5, 6);
    }

    #[test]
    fn the_icosahedron_is_twelve_pentagons() {
        let t = icosahedron();
        assert_eq!(t.tile_count(), 12, "10n^2+2 at n=1");
        for id in t.tiles() {
            assert_eq!(
                t.neighbors(id).len(),
                5,
                "{id} should be a pentagon like every other tile"
            );
        }
    }

    #[test]
    fn flat_topologies_have_edges_and_closed_ones_do_not() {
        // This is the distinction that matters: the game is played on a closed
        // surface, so anything that only works because the world stops
        // somewhere is a bug waiting to happen.
        assert!(!is_closed(&line(5)));
        assert!(!is_closed(&grid(4, 4)));
        assert!(!is_closed(&hex_grid(5, 5)));

        assert!(is_closed(&icosahedron()));
        assert!(is_closed(&torus(6, 4)));
    }

    #[test]
    fn every_tile_of_a_closed_surface_reaches_every_other() {
        for t in [icosahedron(), torus(5, 4)] {
            let dist = t.distances_from(TileId(0), |_| true);
            assert!(
                dist.iter().all(Option::is_some),
                "a closed surface has no unreachable tile"
            );
        }
    }

    #[test]
    fn a_torus_wraps_in_both_directions() {
        let t = torus(6, 4);
        // Column 0 and column 5 are neighbours; row 0 and row 3 likewise.
        assert!(t.neighbors(TileId(0)).contains(&TileId(5)));
        assert!(t.neighbors(TileId(0)).contains(&TileId(18)));

        // Going the short way round beats going the long way.
        assert_eq!(t.distance(TileId(0), TileId(5), |_| true), Some(1));
    }
}
