//! Tile adjacency — the only spatial knowledge `civ-core` has.
//!
//! # Why there are no coordinates here
//!
//! The planet is a Goldberg polyhedron, so tiles have 3D positions. The
//! simulation deliberately does not know them. Every spatial question the rules
//! ask — "approximately the centre of the territory", "the capital closest to
//! the captured tile", "within four steps" — is answered with **graph distance
//! in hops**, which is integer, platform-independent, and closer to what a
//! hex-game player actually perceives than Euclidean distance would be.
//!
//! This keeps `civ-core` free of floating point entirely (see
//! docs/determinism.md) and means `civ-worldgen` can change the planet's
//! geometry without touching a line of game logic.

use crate::ids::TileId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Who counts as adjacent to whom.
///
/// Produced by `civ-worldgen` from the hex sphere, or by hand in tests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topology {
    /// Neighbours of each tile, sorted ascending and deduplicated.
    neighbors: Vec<Vec<TileId>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TopologyError {
    #[error("tile {tile} lists neighbour {neighbor}, which is out of range (tile count {count})")]
    OutOfRange {
        tile: TileId,
        neighbor: TileId,
        count: usize,
    },
    #[error("adjacency is not symmetric: {a} lists {b}, but {b} does not list {a}")]
    Asymmetric { a: TileId, b: TileId },
    #[error("tile {0} lists itself as a neighbour")]
    SelfLoop(TileId),
}

impl Topology {
    /// Build from raw adjacency lists, sorting and validating them.
    ///
    /// Sorting is not cosmetic: iteration order over neighbours must be
    /// identical everywhere, or BFS tie-breaks would differ between machines.
    pub fn new(mut neighbors: Vec<Vec<TileId>>) -> Result<Self, TopologyError> {
        let count = neighbors.len();

        for list in neighbors.iter_mut() {
            list.sort_unstable();
            list.dedup();
        }

        for (i, list) in neighbors.iter().enumerate() {
            let tile = TileId(i as u32);
            for &n in list {
                if n.index() >= count {
                    return Err(TopologyError::OutOfRange {
                        tile,
                        neighbor: n,
                        count,
                    });
                }
                if n == tile {
                    return Err(TopologyError::SelfLoop(tile));
                }
                if !neighbors[n.index()].contains(&tile) {
                    return Err(TopologyError::Asymmetric { a: tile, b: n });
                }
            }
        }

        Ok(Self { neighbors })
    }

    #[inline]
    pub fn tile_count(&self) -> usize {
        self.neighbors.len()
    }

    #[inline]
    pub fn neighbors(&self, tile: TileId) -> &[TileId] {
        &self.neighbors[tile.index()]
    }

    /// All tile ids, ascending.
    pub fn tiles(&self) -> impl Iterator<Item = TileId> + '_ {
        (0..self.neighbors.len() as u32).map(TileId)
    }

    /// Hop distance from `from` to every tile reachable through `passable`.
    ///
    /// `from` itself is always distance 0, even if it is not passable, so a
    /// unit standing on a tile can measure distances outward from it.
    pub fn distances_from(
        &self,
        from: TileId,
        passable: impl Fn(TileId) -> bool,
    ) -> Vec<Option<u32>> {
        let mut dist = vec![None; self.tile_count()];
        dist[from.index()] = Some(0);
        let mut frontier = vec![from];
        let mut next = Vec::new();
        let mut depth = 0;

        while !frontier.is_empty() {
            depth += 1;
            for &tile in &frontier {
                for &n in self.neighbors(tile) {
                    if dist[n.index()].is_none() && passable(n) {
                        dist[n.index()] = Some(depth);
                        next.push(n);
                    }
                }
            }
            frontier.clear();
            core::mem::swap(&mut frontier, &mut next);
        }

        dist
    }

    /// Hop distance from `from` to `to`, travelling only through `passable`.
    pub fn distance(
        &self,
        from: TileId,
        to: TileId,
        passable: impl Fn(TileId) -> bool,
    ) -> Option<u32> {
        if from == to {
            return Some(0);
        }
        self.distances_from(from, passable)[to.index()]
    }

    /// Split a tile set into connected components, using only edges that stay
    /// inside the set.
    ///
    /// Components come back sorted by their lowest member, so the result is
    /// stable regardless of the input's insertion history.
    pub fn components(&self, members: &BTreeSet<TileId>) -> Vec<BTreeSet<TileId>> {
        let mut seen = BTreeSet::new();
        let mut out: Vec<BTreeSet<TileId>> = Vec::new();

        // Iterating a BTreeSet is ascending, so components are discovered in
        // ascending order of their lowest member already.
        for &start in members {
            if seen.contains(&start) {
                continue;
            }
            let mut component = BTreeSet::new();
            let mut stack = vec![start];
            seen.insert(start);
            component.insert(start);

            while let Some(tile) = stack.pop() {
                for &n in self.neighbors(tile) {
                    if members.contains(&n) && seen.insert(n) {
                        component.insert(n);
                        stack.push(n);
                    }
                }
            }
            out.push(component);
        }

        out
    }

    /// The tile in `candidates` closest to the graph centre of `members`,
    /// measured as the smallest total hop distance to every member.
    ///
    /// Returns every tile tied for best, ascending, so the caller can break the
    /// tie however the rules require (see `territory::relocate_capital`, which
    /// breaks it randomly to honour the brief's "randomly ... approximately the
    /// center").
    pub fn most_central(&self, members: &BTreeSet<TileId>, candidates: &[TileId]) -> Vec<TileId> {
        let mut best = u64::MAX;
        let mut winners = Vec::new();

        for &candidate in candidates {
            let dist = self.distances_from(candidate, |t| members.contains(&t));
            // Unreachable members should not happen inside one component, but
            // if they do, penalise heavily rather than panicking.
            let total: u64 = members
                .iter()
                .map(|m| dist[m.index()].map_or(u64::from(u32::MAX), u64::from))
                .sum();

            if total < best {
                best = total;
                winners.clear();
                winners.push(candidate);
            } else if total == best {
                winners.push(candidate);
            }
        }

        winners.sort_unstable();
        winners
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A line of `n` tiles: 0-1-2-...-(n-1).
    pub(crate) fn line(n: u32) -> Topology {
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
        Topology::new(neighbors).unwrap()
    }

    fn set(ids: &[u32]) -> BTreeSet<TileId> {
        ids.iter().copied().map(TileId).collect()
    }

    #[test]
    fn rejects_asymmetric_adjacency() {
        let err = Topology::new(vec![vec![TileId(1)], vec![]]).unwrap_err();
        assert!(matches!(err, TopologyError::Asymmetric { .. }));
    }

    #[test]
    fn rejects_self_loops() {
        let err = Topology::new(vec![vec![TileId(0)]]).unwrap_err();
        assert_eq!(err, TopologyError::SelfLoop(TileId(0)));
    }

    #[test]
    fn rejects_out_of_range_neighbours() {
        let err = Topology::new(vec![vec![TileId(5)]]).unwrap_err();
        assert!(matches!(err, TopologyError::OutOfRange { .. }));
    }

    #[test]
    fn measures_hop_distance_along_a_line() {
        let t = line(5);
        assert_eq!(t.distance(TileId(0), TileId(4), |_| true), Some(4));
        assert_eq!(t.distance(TileId(0), TileId(0), |_| true), Some(0));
    }

    #[test]
    fn impassable_tiles_block_the_path() {
        let t = line(5);
        // Tile 2 is a wall, so 0 cannot reach 4.
        assert_eq!(t.distance(TileId(0), TileId(4), |x| x != TileId(2)), None);
    }

    #[test]
    fn splits_a_broken_line_into_two_components() {
        let t = line(5);
        let comps = t.components(&set(&[0, 1, 3, 4]));
        assert_eq!(comps, vec![set(&[0, 1]), set(&[3, 4])]);
    }

    #[test]
    fn components_are_ordered_by_lowest_member() {
        let t = line(7);
        let comps = t.components(&set(&[5, 6, 0, 1]));
        assert_eq!(comps[0], set(&[0, 1]));
        assert_eq!(comps[1], set(&[5, 6]));
    }

    #[test]
    fn most_central_finds_the_middle_of_a_line() {
        let t = line(5);
        let members = set(&[0, 1, 2, 3, 4]);
        let candidates: Vec<TileId> = (0..5).map(TileId).collect();
        assert_eq!(t.most_central(&members, &candidates), vec![TileId(2)]);
    }

    #[test]
    fn most_central_returns_every_tie() {
        let t = line(4);
        let members = set(&[0, 1, 2, 3]);
        let candidates: Vec<TileId> = (0..4).map(TileId).collect();
        // Both middle tiles have total distance 1+0+1+2 = 4.
        assert_eq!(
            t.most_central(&members, &candidates),
            vec![TileId(1), TileId(2)]
        );
    }
}
