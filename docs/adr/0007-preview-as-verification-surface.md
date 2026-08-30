# ADR 0007 — The preview is the verification surface

**Status:** accepted · 2026-08-30

## Context

Every gate in this repository checks something mechanical: formatting, lints,
tests, golden state hashes, the dependency graph, spec coverage. Together they
prove a change is *self-consistent*.

None of them prove it is *right*.

The gap is sharpest in exactly the work the project is made of:

- Cover scattered evenly across the planet and cover properly clumped into
  villages produce **identical** share statistics. Every assertion in GROW and
  the cover-seeding tests passes either way.
- Continents banded around the equator, or clinging to one face of the
  icosahedron, satisfy every assertion about a land-fraction quantile.
- A territory split is only ever observed as a 64-bit hash. `0xb9e0cb1f3f8c7990`
  is either right or wrong and no human can tell which by reading it.
- A seam along an icosahedral edge is invisible to a pentagon count.

Each of those is a five-second observation if anyone can look, and an unbounded
debugging session if nobody can. Until now nobody could: `lands-render`,
`lands-procgen` and `lands-app` were stubs, and the first thing able to draw a
planet sat four milestones away behind the native shells.

The interim answer was #34, which exports the planet as OBJ or JSON for an
offline viewer. That was the right call and it helped, but it is a file you
remember to open, not something a reviewer is handed.

## Decision

**A live WebGPU preview is the verification surface for the project**, and it
comes before the milestones that depend on it.

1. **It lives in `lands-app`**, started in a debug mode — not a separate tool.
   The camera, input and picking it needs are the same code the shipping app
   needs, so a separate harness would mean writing them twice and letting the
   two drift. A green preview has to be evidence about the game, not about a
   lookalike.

2. **It boots straight to a planet** at a fixed default frequency and seed, so
   two previews from two pull requests are directly comparable.

3. **The first renderer is deliberately minimal** — flat-shaded coloured
   polygons, no textures, no pixel-art pass. The point is to exist while
   worldgen is still being written, which is when looking matters most. The real
   pipeline replaces it later.

4. **It plays golden replays** rather than being interactive. Playback is
   deterministic, reuses a corpus that already exists, and ties directly to the
   regression net: if a replay looks wrong on screen, its hash is wrong too.
   Interactivity is a later question.

5. **Every visual issue adds one file** under `crates/lands-app/src/debug/` plus
   one appended line in a registry — the same append-only discipline as
   `SeedDomain`, for the same reason. Agents touch their own file; the shared
   line conflicts trivially rather than semantically.

6. **Every pull request gets a deployed preview URL**, on Cloudflare Pages. A
   preview you have to fetch a branch and build to see is friction paid on every
   review, and friction paid every time is friction eventually skipped.

7. **The preview build is a required check.** A change that breaks it cannot
   merge, like any other regression.

## Consequences

- The renderer milestone effectively splits: a minimal path now, the pixel-art
  pipeline later. #18 changes from "build the renderer" to "replace the flat
  path", and stops being a bottleneck.
- The wasm half of #23 is absorbed. What remains there is the desktop harness,
  which may not be worth building at all once a preview URL exists.
- Deployment needs a Cloudflare account and two repository secrets, which cannot
  be set from inside the repository. The workflow fails with a message naming
  them rather than a generic authentication error.
- WebGPU on desktop Chrome and Edge only. A browser without it gets an explicit
  explanation, never a black screen. A WebGL2 fallback was considered and
  rejected: it would constrain the pixel-art pipeline to what WebGL2 allows, and
  two rendering paths that look different are worse than one that some browsers
  cannot open.
- Non-visual work — traits, formats, CI, tooling — is exempt. Requiring a panel
  for a file format would produce contrived panels nobody reads, and a rule
  satisfied by ceremony stops being a rule.
