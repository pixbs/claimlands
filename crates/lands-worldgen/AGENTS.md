# lands-worldgen — local rules

This crate builds the planet: the geodesic sphere, its dual tile graph, and
later the terrain and cover on it. `lands-core` plays the game on whatever comes
out of here, so the geometry is free to change — the numbering is not.

## Hard constraints

- **The layering rule applies from the first line of code.** This crate may
  depend only on strictly lower layers, and `cargo xtask check-deps` fails the
  build otherwise. Nothing above `lands-core` may be reached from here.
- **Floating point is allowed here and only here on this path.** The sphere is
  built once per match from an integer frequency, and no rule ever compares a
  coordinate. That permission does not extend to anything crossing into
  `lands-core`, which does no floating-point arithmetic at all.
- **Vertex order is the API, not an implementation detail.** Tile ids in
  `assets/`, in saved levels and in `tests/replays/` are indices into the order
  `geodesic()` emits vertices in. Reordering it renumbers every tile on every
  stored planet, and nothing downstream can detect that it happened.

## Identity is combinatorial, never geometric

Two icosahedral faces reach a shared edge point by different arithmetic and
land a few ulps apart. The prototype papers over that by rounding coordinates
to six decimals and comparing strings; do not copy it. Rounding is a tolerance,
and every tolerance is wrong in one of two directions — too tight and one
vertex becomes two (a seam in the mesh, a hole in the tile graph), too loose
and two vertices become one.

So no coordinate is ever compared for identity. A point is identified by
*where it sits on the icosahedron* — a corner, a whole number of steps along a
named edge, or an interior point of one face — which is an integer, exact, and
the same on every target. See `LatticeKey`. Anything added to this crate that
needs to know whether two points are the same point must extend that idea
rather than reach for an epsilon.

Vertex counts are the cheapest check that it held: a frequency-`n` sphere has
exactly `10n² + 2` vertices, so both failure directions show up as a wrong
count, and `V - E + F = 2` catches the rest.

## Before you push

```bash
cargo test -p lands-worldgen
```

If `tests/snapshot.rs` fails, the vertex order moved. The question to answer is
not what the new hash should be — it is which saved levels and golden replays
now describe a different planet than the one they were recorded on.
