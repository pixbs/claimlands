# Visual language

Everything the renderer needs to know that was worked out in the prototype,
`reference/prototype/hex-planet.html`. That file is the **visual oracle**: when
the Rust renderer and the prototype disagree, the prototype is right until
someone decides otherwise on purpose.

These numbers belong in `assets/visual/style.ron` (M6), not in code — same
argument as ADR 0006. This document explains *why* each one is what it is,
which a data file cannot.

---

## The look

Pixel art on a rotating sphere. The whole effect comes from two decisions:

1. **Render at `1 / pixelScale` resolution** (default 3) into a small
   backbuffer, then upscale with nearest-neighbour filtering. This is also the
   single biggest fill-rate win, which matters on a phone.
2. **Every texture is `NearestFilter` with no mipmaps.** The one exception is
   the planet's halo, which is a glow rather than pixel art.

The camera is fixed on +Z looking at the origin; **the planet rotates, not the
camera**. That is why the halo needs no billboarding.

---

## Geometry

The planet is a Goldberg polyhedron — the dual of a subdivided icosahedron.
Every geodesic vertex becomes a tile; every triangle centroid becomes a tile
corner.

| Constant | Value | Meaning |
|---|---|---|
| tile count | `10n² + 2` | always exactly 12 pentagons, the rest hexagons |
| frequency range | 2 … 12 | 42 … 1442 tiles; default 8 → 642 |
| `TILE_PX` | 24 | texture pixels across one tile |
| `RADIUS` | 1 | unit sphere |
| `LEVEL_PX` | 4 | height step between land and sea |
| `LAND_FRACTION` | 0.42 | land is pinned to this share by quantile, not threshold |

### The seamlessness trick

Any value that is a property of a **corner** — coast proximity, built-up
fraction, cliff push direction — is computed from the three tiles meeting at
that corner, so both sides of a shared edge agree exactly. This is what stops
visible cracks along tile borders, and it is the single most important
structural idea to carry across to Rust.

Each tile has a stable tangent frame (`e1` toward corner 0, `e2 = centre × e1`)
so texture orientation is deterministic.

---

## Palettes

Straight from the prototype. Colours are sRGB hex.

| Name | Values |
|---|---|
| `GRASS_BANDS` | `#3f7d34` `#5aa444` `#7cc255` |
| `MUD_BANDS` | `#a18a5b` `#c9ac72` `#ddbd7d` |
| `SEA_BANDS` | `#123659` `#1a4d78` `#246698` `#3184b0` `#46a2c2` |
| `CLIFF_ROWS` | `#5a4a2e` `#7d6a45` `#a08a5f` `#bda677` |
| `FOAM_ROWS` | `#ffffff` `#eaf6ff` `#d3e8f7` `#bcd9ee` |
| `CANOPY` | body `#3a7a26`, zones `#6a8d25` `#78a226`, rim `#225d1d`, side `#1d4f18` |
| `HOUSE_WALLS` | `#e7ddc8` `#dcd0b8` `#cfc2a8` `#e2d5bd` (one tone per village) |
| `HOUSE_ROOF` | `#a24b32` / `#823a26` |
| `HOUSE_OPEN` | `#453227` (doors and windows) |
| `AIR_COLOR` | `#c3e3f6` |
| `SKY_CORE` / `SKY_RIM` | `#0f0c26` / `#030209` |
| `CLOUD_DECKS` | `#b9d4e8` @ 38%, `#dceaf5` @ 20%, `#ffffff` @ 9% cover |

### Faction colours — the gap to fill

**The prototype has no faction colours.** It has one territory colour,
`#f2b45c` (gold), and a single owner. The Rust renderer needs a proper
`Faction → colour` map for RED, YELLOW, GREEN and BLUE, per-owner border
meshes (the prototype merges them all into one), and a per-owner tint in the
ground atlas.

Note that `FIELD_CROPS` contains entries *named* `green` and `yellow`. Those are
crop tones, not factions — do not reuse them.

UI tokens from the prototype's CSS, for reference: `--deep #0e0b16`,
`--line #2f2740`, `--text #ece4d4`, `--muted #8b809c`, `--gold #f2b45c`,
`--teal #58c2b0` (the hover ring).

---

## Procedural content

### Textures (all generated at runtime; none loaded)

| Texture | Size | Notes |
|---|---|---|
| Cliff strip | 24 × 4 | 4 rows, per-texel row-swap jitter, repeats |
| Foam sheet | 24 × 56 | 8-frame vertical sprite sheet, animated at ~7 fps by offsetting V |
| Field strip | 5 × 8 | one column per furrow cycle, two rows per crop |
| **Ground atlas** | up to 912 × 912 | one 24×24 cell per tile; each texel inverse-mapped and sampled from a continuous 3D fBm, so the pattern crosses tile borders unbroken |
| Cloud decks ×3 | 928 × 296 RGBA | equal-area cylindrical mapping; alpha holds an ordered 4×4 Bayer dither as reciprocal rank |
| Halo | 128 × 128 | radial alpha ramp; the only linearly-filtered texture |

### Meshes

- **Fields** are built per *zone* (a connected run of field tiles), not per
  tile: a gnomonic projection, a recursive randomised binary split into
  parcels, then clipping to individual hexes carrying per-edge tags so field
  edges get a sloped skirt while hex seams stay flush.
- **Forests** are planted on a jittered grid with a pitch *smaller* than the
  crown radius, so crowns interpenetrate and read as one canopy mass. Crowns
  are deliberately **not** clipped to the hex — the treeline spills over the
  border. A seven-sided floor disc under each tree (odd, so it does not align
  with the hex grid) merges into a scalloped shadow.
- **Houses** are built from primitives per building: two long walls, two
  five-sided gable ends, striped roof pitches, a ridge cap, a mitred eave, decal
  doors and windows floated slightly off the surface, and an optional chimney
  poking through at the correct local roof height.
- **Coasts** are a wedge, not a square cliff: the bottom edge is pushed out over
  the water by a corner-derived amount, so coastlines cannot crack.

---

## Performance notes carried forward

What the prototype does, and what to do differently in Rust.

**Keep:**
- Low-resolution render target plus nearest upscale.
- Partial atlas repaint — repainting one tile plus its coast falloff rather than
  the whole atlas.
- Animation without geometry rebuilds: foam by texture offset, cloud drift by
  whole-texel offset, see-through by uniform writes.

**Fix:**
- The atlas painter is a **synchronous main-thread loop over ~830k texels** at
  the largest size. In Rust this should be a compute shader or a worker.
- The cloud sky is 274,688 texels of three-octave fBm, also synchronous.
- Flipping one tile rebuilds the **entire** world mesh, and changing one cover
  tile rebuilds all three cover meshes. Acceptable in a prototype at ~2 ms;
  not acceptable as the game grows.
- **No instancing anywhere.** Trees and houses are CPU-merged into large
  buffers. Thousands of trees at ~165 vertices each is the obvious first thing
  to move to `InstancedBuffer`.
- Materials are not disposed in the mesh-drop path, which leaks slowly across
  regenerations.
