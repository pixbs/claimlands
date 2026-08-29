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

If `tests/snapshot.rs` fails, the numbering moved. The question to answer is
not what the new hash should be — it is which saved levels and golden replays
now describe a different planet than the one they were recorded on.

It pins three values, and which one moved says how bad it is:

| Constant | What moved | Cost |
|---|---|---|
| `MESH` | the geodesic vertex order | every stored tile id means a different tile |
| `ADJACENCY` | the tile graph `lands-core` is given | same, and the rules now play on a different board |
| `STRUCTURE` | the dual's corner fans as well | only `lands-procgen` renumbers; saved games are fine |

`tests/terrain.rs` pins a second kind of snapshot: a seed to a land mask. A
level stores a seed rather than a map, so a seed *is* a planet, and a moved
hash there means every level ever authored is now set on a different world.
Unlike the three above it is a hash over something derived from coordinates,
which only holds because every step from the seed to the mask is arithmetic
IEEE-754 specifies exactly — `sin` is not, so `src/trig.rs` supplies one that
is. Anything added here that needs a transcendental function needs to go the
same way rather than reach for `f64::sin`.

`tests/closed_surface.rs` runs `lands-core`'s own closed-surface scenarios on a
real 642-tile sphere. `lands-core` cannot: depending on this crate would point
the graph back at itself. If one of those fails, worldgen has produced a planet
with a seam in it — the rules themselves are not in question.
