# ADR 0004 — The simulation measures distance in hops, not metres

**Status:** accepted · 2026-08-29

## Context

The planet is a Goldberg polyhedron: the dual of a subdivided icosahedron,
`10n²+2` tiles, twelve of them pentagons. Tiles have real 3D positions.

Several rules ask spatial questions. "A unit moves four tiles." "A new capital
appears approximately at the centre of the territory." "The capital closest to
the captured tile survives."

## Decision

`lands-core` knows only adjacency. Every spatial question is answered by
breadth-first search in hops. `Topology` holds neighbour lists and nothing else
— no coordinates at all.

"Approximately the centre" is defined as the tile with the smallest total hop
distance to every other tile in the territory.

## Why

- **It is integer**, so it satisfies the no-floating-point rule (ADR 0005) with
  nothing to argue about. Comparing Euclidean distances would mean comparing
  floats, which is exactly where architectures diverge.
- **It matches what a player perceives.** On a hex board, "three tiles away" is
  what the player counts. Great-circle distance would occasionally disagree with
  the obvious answer, especially near the twelve pentagons.
- **It decouples the rules from the geometry.** `lands-worldgen` can change tile
  count, projection or subdivision without touching a line of game logic. Tests
  can use a five-tile line instead of a 642-tile sphere.

## The consequence that matters most: the planet has no edges

A sphere is a **closed surface**. A territory can encircle it; a path can go
either way round; there is no row zero and no last column.

On a coordinate-based design this is a notorious source of bugs — wrap-around
needs modulo arithmetic at every site that touches position, and the ones that
get forgotten fail only at the seam, which is exactly where nobody tests.

Because `lands-core` holds only an adjacency graph, **a sphere is just a graph
with no boundary**, and there is nothing to special-case. Movement BFS,
territory connected-components and capital relocation are all correct on a
closed surface for the same reason they are correct on a bounded one: they
never knew the difference. There is no modulo arithmetic anywhere in the crate.

Two consequences are worth stating because they would be easy to get wrong
otherwise:

- **A ring cut once does not split.** Removing one tile from a territory that
  encircles the planet leaves it connected the other way round. On a line the
  same capture splits it in two. Connected-components gives both answers with
  no branch.
- **A Euclidean "centre" would be actively wrong.** The average of points on a
  sphere lies *inside* the sphere, not on its surface, so TERR-011's
  "approximately the centre" has no metric answer at all. The graph median has
  one, always, and on a perfectly symmetric ring it degrades to a
  deterministic tie-break rather than to nonsense.

`crates/lands-core/tests/closed_surface.rs` is the evidence: it runs the rules on
the dual of an icosahedron (the genuine `n = 1` planet — twelve pentagons) and
on a wrapping torus. Those tests passed the first time they were run, with no
change to `lands-core`.

## Other consequences

- Any future rule genuinely needing metric distance (a projectile arc, an area
  effect) must either be expressed in hops or live outside `lands-core`.
- Pentagon tiles have five neighbours instead of six, so hop distance is
  slightly anisotropic near them. This is a property of the board that players
  can see and reason about, not a bug.
