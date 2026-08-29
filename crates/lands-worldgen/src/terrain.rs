//! Continents: seven directional waves over the sphere, cut at a quantile.
//!
//! Ported from `generateTerrain` in `reference/prototype/hex-planet.html`.
//!
//! Seven plane waves, each with a random direction, wavelength and phase, are
//! summed at every tile centre. Amplitudes fall as `1 / frequency`, so the slow
//! waves lay down continents and the fast ones only roughen their edges. The
//! field is then cut into land and water, and the coastline is smoothed —
//! without that pass a threshold through a wave field leaves confetti, not
//! coasts.
//!
//! # The quantile is the whole trick
//!
//! The obvious way to turn a height field into land is to keep everything above
//! a fixed height. It does not work: the sum of seven waves has a different
//! spread for every seed, so one seed comes out an ocean world and the next a
//! supercontinent, and a level author choosing a seed is really rolling for a
//! playable amount of land.
//!
//! So the cut is a **quantile**, not a height: sort the elevations and cut at
//! an index. The waves then decide only the *shape* of the continents, never
//! how much of them there is. `docs/visual-language.md` lists `LAND_FRACTION`
//! as pinned "by quantile, not threshold" for this reason.
//!
//! # Which quantile, and why it is searched for rather than computed
//!
//! The prototype cuts at `floor(n * (1 - LAND_FRACTION))` and stops there. That
//! pins the land share *before* smoothing, which is not where anybody wants it
//! pinned: the pass drowns far more than it fills — a coastline has many more
//! one-tile spits than one-tile bays — so the planet that comes out misses the
//! target by up to fifteen tiles in 642, usually short, and by a different
//! amount for every seed. That is the variance the quantile was chosen to
//! remove, turning up again one step later.
//!
//! So the cut index is chosen so that the land share is right *after* the
//! smoothing, by binary search over the sorted elevations. That search is exact
//! rather than approximate, because the pipeline is monotone: raising the cut
//! can only take land away, and both smoothing passes preserve that — if one
//! land set contains another before a pass, it still contains it after (see
//! [`smooth_coastline`]). So the final land count is non-increasing in the cut
//! index, ten probes find the best index there is, and no threshold anywhere
//! would have landed closer.
//!
//! What is left over is granularity, not error: a tile is land or it is not, so
//! a planet of 642 tiles cannot be 42% land to better than half a tile, and
//! moving the cut past one tile's elevation can move the smoothed count by more
//! than one. Measured over thousands of seeds, the miss is at most two tiles
//! from 252 tiles upward and at most three on the two smallest planets, where
//! three tiles is under two percent of the whole.
//!
//! # Why the share is a percentage rather than `0.42`
//!
//! `LAND_PERCENT` is an integer because it is the numerator of a tile count,
//! and `round(n * 0.42)` in floating point is a different function from
//! `(n * 42 + 50) / 100` at every `n` where the product lands on a half. There
//! is no reason to introduce a rounding boundary into a number that never
//! needed to leave the integers.
//!
//! # Determinism
//!
//! Land or water is a fact the simulation plays on, so it must be the same
//! everywhere. Randomness comes from `lands_core::rng::stream` on
//! [`SeedDomain::Terrain`] — the domain reserved for exactly this since the
//! enum was written — and the field is built only from arithmetic IEEE-754
//! pins down, [`crate::trig::sin_pi`] included. See `docs/determinism.md`.

use crate::digest::Digest;
use crate::goldberg::{Cell, Goldberg};
use crate::trig::sin_pi;
use crate::vec3::Vec3;
use lands_core::prelude::{Terrain, TileId};
use lands_core::rng::{Rng, SeedDomain, stream};

/// The share of the planet that comes out as land, as a percentage.
///
/// `LAND_FRACTION` in the prototype and in `docs/visual-language.md`, where
/// 0.42 is the value the look was tuned at: enough ocean for the continents to
/// read as continents, enough land for four factions to have somewhere to go.
pub const LAND_PERCENT: usize = 42;

/// How many waves are summed into the elevation field.
///
/// Fewer and the continents come out as stripes; many more and the `1 / f`
/// falloff has the fast ones cancelling into noise. Seven is the prototype's.
const WAVES: usize = 7;

/// The slowest wave: a little over one crest across the planet.
const MIN_FREQUENCY: f64 = 1.2;

/// How much faster than the slowest wave the fastest may be.
const FREQUENCY_SPAN: f64 = 4.2;

/// A land tile keeps its land only if this many neighbours are land too.
///
/// One is a spit sticking out of nowhere and zero is a lone islet; the
/// prototype drowns both, and the coastline reads as a coastline for it.
const MIN_LAND_NEIGHBORS: usize = 2;

/// Which tiles of a planet are land and which are water.
///
/// Indexed by [`TileId`], parallel to [`Goldberg::cells`]: tile `i` here is
/// tile `i` there, which is geodesic vertex `i`, which is the id a saved level
/// and a golden replay both mean. Build one with [`terrain`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainMap {
    tiles: Vec<Terrain>,
    land: usize,
}

impl TerrainMap {
    /// How many tiles the planet has.
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// Land or water, by tile id.
    pub fn get(&self, tile: TileId) -> Terrain {
        self.tiles[tile.index()]
    }

    /// Whether a tile is land.
    pub fn is_land(&self, tile: TileId) -> bool {
        self.get(tile) == Terrain::Land
    }

    /// Every tile, indexed by tile id — what a level builder wants in one go.
    pub fn tiles(&self) -> &[Terrain] {
        &self.tiles
    }

    /// How many tiles are land: [`target_land`] for the planet's size, to
    /// within the granularity of a single tile's elevation.
    pub fn land_count(&self) -> usize {
        self.land
    }

    /// How many tiles are water.
    pub fn water_count(&self) -> usize {
        self.tiles.len() - self.land
    }

    /// A fingerprint of the land mask: the tile count, then the mask packed
    /// thirty-two tiles to a word, lowest id in the lowest bit.
    ///
    /// Integers only, for the reason [`crate::digest`] gives — and this is a
    /// stronger claim than the crate's other snapshots, because the mask is
    /// derived from coordinates whereas theirs come from the lattice. It
    /// holds because every step from the seed to the mask is arithmetic
    /// IEEE-754 specifies exactly. If it ever stops holding on one target and
    /// not another, [`crate::trig`] is where to look first.
    pub fn terrain_hash(&self) -> u64 {
        let mut d = Digest::new();
        d.u32(self.tiles.len() as u32);
        for word in self.tiles.chunks(32) {
            let mut bits = 0u32;
            for (bit, &tile) in word.iter().enumerate() {
                if tile == Terrain::Land {
                    bits |= 1 << bit;
                }
            }
            d.u32(bits);
        }
        d.finish()
    }
}

/// How many tiles of a planet of `tile_count` tiles should be land: the nearest
/// whole tile to [`LAND_PERCENT`] of it.
pub const fn target_land(tile_count: usize) -> usize {
    (tile_count * LAND_PERCENT + 50) / 100
}

/// Generate the terrain of a planet from a world seed.
///
/// The planet's *shape* comes from its frequency and is the same for every
/// seed; what the seed picks is where the continents sit on it.
pub fn terrain(planet: &Goldberg, world_seed: u64) -> TerrainMap {
    let waves = waves(world_seed);
    let elevation: Vec<f64> = planet
        .cells()
        .iter()
        .map(|cell| elevation(cell.center, &waves))
        .collect();

    let land = continents(planet, &elevation, quantile_cut(planet, &elevation));

    let count = land.iter().filter(|&&land| land).count();
    let tiles = land
        .iter()
        .map(|&land| if land { Terrain::Land } else { Terrain::Water })
        .collect();

    TerrainMap { tiles, land: count }
}

/// One plane wave over the sphere.
#[derive(Debug, Clone, Copy)]
struct Wave {
    /// Unit-length direction the wave travels in.
    direction: Vec3,
    /// Crests per unit of distance along `direction`, in half turns — the
    /// prototype's `f`, whose `Math.PI` is folded into [`sin_pi`].
    frequency: f64,
    /// Where the wave starts, in half turns: `0 .. 2` is a whole period.
    phase: f64,
}

/// Draw the waves for a seed.
///
/// The draw order is the prototype's, and it is load-bearing: it is what ties a
/// seed to a planet, so reordering these lines gives every existing seed a
/// different world.
fn waves(world_seed: u64) -> Vec<Wave> {
    // Terrain is generated once, before the first turn, and is the only
    // consumer of its domain — hence turn 0 and entity 0.
    let mut rng = stream(world_seed, SeedDomain::Terrain, 0, 0);

    (0..WAVES)
        .map(|_| {
            let x = signed_unit(&mut rng);
            let y = signed_unit(&mut rng);
            let z = signed_unit(&mut rng);
            let frequency = MIN_FREQUENCY + FREQUENCY_SPAN * unit(&mut rng);
            let phase = 2.0 * unit(&mut rng);
            Wave {
                // A uniform point in the cube, normalised. That favours the
                // eight corner directions slightly over the six face ones. It
                // is the prototype's bias and it is kept: the waves are a look
                // rather than a distribution, and correcting it would move
                // every existing seed's planet for nothing anyone can see.
                direction: Vec3::new(x, y, z).normalize(),
                frequency,
                phase,
            }
        })
        .collect()
}

/// The height field at one tile centre: the waves summed, each damped by its
/// own frequency so the slow ones shape the continents.
fn elevation(center: Vec3, waves: &[Wave]) -> f64 {
    let mut sum = 0.0;
    for wave in waves {
        sum += sin_pi(wave.frequency * center.dot(wave.direction) + wave.phase) / wave.frequency;
    }
    sum
}

/// Everything above `cut`, with its coastline smoothed.
fn continents(planet: &Goldberg, elevation: &[f64], cut: f64) -> Vec<bool> {
    let mut land: Vec<bool> = elevation.iter().map(|&e| e > cut).collect();
    smooth_coastline(planet, &mut land);
    remove_lone_tiles(planet, &mut land);
    land
}

/// The elevation to cut at, chosen so that the *finished* planet is as close to
/// [`target_land`] as any cut could make it.
///
/// The count of land after [`continents`] is non-increasing in the cut index,
/// so this is a binary search for the first index that does not overshoot,
/// followed by one comparison against the index before it. Ten probes at the
/// size the game plays at.
fn quantile_cut(planet: &Goldberg, elevation: &[f64]) -> f64 {
    let mut sorted = elevation.to_vec();
    // `total_cmp` rather than `partial_cmp` and an unwrap: it is a total order
    // on every `f64` there is, so the sort cannot depend on whether a NaN
    // reached it.
    sorted.sort_unstable_by(f64::total_cmp);

    let target = target_land(sorted.len());
    let land_at = |index: usize| {
        continents(planet, elevation, sorted[index])
            .iter()
            .filter(|&&land| land)
            .count()
    };

    // Cutting at the highest elevation leaves nothing above it, so the search
    // always has somewhere to land; cutting at the lowest leaves all but one
    // tile, which no target can exceed.
    let (mut low, mut high) = (0, sorted.len() - 1);
    while low < high {
        let middle = low + (high - low) / 2;
        if land_at(middle) <= target {
            high = middle;
        } else {
            low = middle + 1;
        }
    }

    // `low` is the first index at or under the target; the one before it is the
    // last one over. Whichever misses by less wins, and a tie keeps the drier
    // planet, so that the choice does not turn on which way a `>=` is written.
    if low > 0 && land_at(low - 1).abs_diff(target) < land_at(low).abs_diff(target) {
        sorted[low - 1]
    } else {
        sorted[low]
    }
}

/// The prototype's pass: drown land with almost no land around it, fill water
/// with almost nothing but land around it.
///
/// Every tile reads the state the pass started in, so the result does not
/// depend on the order tiles are visited in. A sequential pass would give a
/// different planet if the tiles were ever renumbered; this one gives the same
/// answer whatever order it is walked.
///
/// It is also monotone, which is what makes [`quantile_cut`] exact: if one land
/// set contains another going in, it still contains it coming out. A land tile
/// in the smaller set has no more land neighbours than the same tile in the
/// larger, so if it survives the smaller it survives the larger; and a water
/// tile that fills in the smaller set has every neighbour but one as land
/// there, so in the larger set it is either already land with well over
/// [`MIN_LAND_NEIGHBORS`] neighbours, or fills for the same reason.
fn smooth_coastline(planet: &Goldberg, land: &mut [bool]) {
    let before = land.to_vec();

    for (tile, cell) in planet.cells().iter().enumerate() {
        let neighbors = land_neighbors(cell, &before);
        land[tile] = if before[tile] {
            // A lone islet, or a one-tile spit off the end of a coast.
            neighbors >= MIN_LAND_NEIGHBORS
        } else {
            // A puddle: water with land all the way round bar one tile.
            neighbors + 1 >= cell.sides()
        };
    }
}

/// Drown every single-tile island and fill every single-tile lake.
///
/// [`smooth_coastline`] removes most of these and makes a few of its own, which
/// is why it cannot be the last word. A tile with two land neighbours survives
/// it; if those two are not adjacent to each other, each has only that tile for
/// company and both drown, and the one they hung off is left alone in the
/// water. About one seed in eight leaves at least one such tile at the size the
/// game plays at.
///
/// Repeating the pass until it settles would not obviously terminate —
/// simultaneous threshold rules can oscillate with period two — so the strict
/// rule is applied instead, which needs exactly one pass and can never need
/// another:
///
/// * A single-tile island has no land neighbour, so no land tile is adjacent to
///   it and drowning it lowers no land tile's count. It cannot strand another.
/// * A single-tile lake has no water neighbour, so filling it lowers no water
///   tile's count, by the same argument.
/// * The two cannot undo each other either. A land tile with exactly one land
///   neighbour keeps it, because that neighbour is adjacent to land and so is
///   not a lone island; the mirror holds for water.
///
/// So one pass leaves none of either, which is what the planet is required to
/// come out with.
fn remove_lone_tiles(planet: &Goldberg, land: &mut [bool]) {
    let before = land.to_vec();

    for (tile, cell) in planet.cells().iter().enumerate() {
        let neighbors = land_neighbors(cell, &before);
        if before[tile] && neighbors == 0 {
            land[tile] = false;
        }
        if !before[tile] && neighbors == cell.sides() {
            land[tile] = true;
        }
    }
}

/// How many of a tile's neighbours are land.
fn land_neighbors(cell: &Cell, land: &[bool]) -> usize {
    cell.neighbors()
        .iter()
        .filter(|&&neighbor| land[neighbor as usize])
        .count()
}

/// A float in `[0, 1)` from one draw.
///
/// The top 53 bits scaled by `2^-53`: every bit of the mantissa carries, and
/// the scale is a power of two, so the conversion is exact rather than rounded.
fn unit(rng: &mut Rng) -> f64 {
    const SCALE: f64 = 1.0 / 9_007_199_254_740_992.0;
    (rng.next_u64() >> 11) as f64 * SCALE
}

/// A float in `[-1, 1)` from one draw.
fn signed_unit(rng: &mut Rng) -> f64 {
    2.0 * unit(rng) - 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goldberg::goldberg;

    /// Small enough to walk every cut index for several seeds, big enough to
    /// have a coastline worth smoothing.
    const FREQUENCY: u32 = 4;

    fn field(planet: &Goldberg, world_seed: u64) -> Vec<f64> {
        let waves = waves(world_seed);
        planet
            .cells()
            .iter()
            .map(|cell| elevation(cell.center, &waves))
            .collect()
    }

    fn sorted(field: &[f64]) -> Vec<f64> {
        let mut out = field.to_vec();
        out.sort_unstable_by(f64::total_cmp);
        out
    }

    fn land_count(mask: &[bool]) -> usize {
        mask.iter().filter(|&&land| land).count()
    }

    #[test]
    fn the_share_is_the_nearest_whole_tile() {
        assert_eq!(target_land(642), 270); // 269.64
        assert_eq!(target_land(1442), 606); // 605.64
        assert_eq!(target_land(100), 42);
        assert_eq!(target_land(0), 0);
        // The case floating point gets wrong: `floor(50 * 0.58)` is 28, not 29,
        // because 0.58 is not representable and the product lands under.
        assert_eq!(target_land(50), 21);
    }

    #[test]
    fn a_draw_stays_in_its_range() {
        let mut rng = Rng::seed_from_u64(4);
        for _ in 0..10_000 {
            let u = unit(&mut rng);
            assert!((0.0..1.0).contains(&u), "{u} is outside [0, 1)");
            let s = signed_unit(&mut rng);
            assert!((-1.0..1.0).contains(&s), "{s} is outside [-1, 1)");
        }
    }

    #[test]
    fn the_waves_come_from_the_terrain_domain_at_turn_zero() {
        // Which stream the waves are drawn from is not an implementation
        // detail: it is what ties a seed to a planet. Moving the domain, the
        // turn or the entity would give every level ever authored a different
        // world, and nothing downstream could tell that it had happened.
        let mut expected = stream(7, SeedDomain::Terrain, 0, 0);
        let x = signed_unit(&mut expected);
        let y = signed_unit(&mut expected);
        let z = signed_unit(&mut expected);
        let frequency = MIN_FREQUENCY + FREQUENCY_SPAN * unit(&mut expected);
        let phase = 2.0 * unit(&mut expected);

        let first = waves(7)[0];
        assert_eq!(first.direction, Vec3::new(x, y, z).normalize());
        assert_eq!(first.frequency, frequency);
        assert_eq!(first.phase, phase);

        // And no other domain would do. That is the whole point of splitting
        // the streams: a feature that draws from its own cannot move this one.
        let mut elsewhere = stream(7, SeedDomain::CoverSeeding, 0, 0);
        assert_ne!(signed_unit(&mut elsewhere), x);
    }

    #[test]
    fn the_waves_span_the_frequency_range() {
        for seed in 0..200 {
            for wave in waves(seed) {
                assert!(
                    (MIN_FREQUENCY..MIN_FREQUENCY + FREQUENCY_SPAN).contains(&wave.frequency),
                    "{} is outside the wave range",
                    wave.frequency
                );
                assert!((0.0..2.0).contains(&wave.phase));
                assert!((wave.direction.length() - 1.0).abs() < 1e-15);
            }
        }
    }

    /// The property [`quantile_cut`]'s binary search rests on: raising the cut
    /// can only take land away, smoothing and all.
    #[test]
    fn a_higher_cut_never_makes_more_land() {
        let planet = goldberg(FREQUENCY);
        for seed in 0..20 {
            let field = field(&planet, seed);
            let sorted = sorted(&field);
            let mut previous = usize::MAX;
            for &cut in &sorted {
                let count = land_count(&continents(&planet, &field, cut));
                assert!(
                    count <= previous,
                    "seed {seed}: cutting at {cut} gave {count} land after {previous}"
                );
                previous = count;
            }
        }
    }

    /// The search does not merely land near the target — it lands as near as
    /// any cut could. What is left over is the granularity of the field.
    #[test]
    fn the_cut_is_the_best_available_one() {
        let planet = goldberg(FREQUENCY);
        let target = target_land(planet.tile_count());

        for seed in 0..20 {
            let field = field(&planet, seed);
            let chosen = land_count(&continents(&planet, &field, quantile_cut(&planet, &field)));

            for &cut in &sorted(&field) {
                let count = land_count(&continents(&planet, &field, cut));
                assert!(
                    chosen.abs_diff(target) <= count.abs_diff(target),
                    "seed {seed}: the search took {chosen} land when {count} was on offer, \
                     wanting {target}"
                );
            }
        }
    }

    /// The claim the pass's doc comment makes: it needs one application and can
    /// never need another.
    #[test]
    fn removing_lone_tiles_settles_in_one_pass() {
        let planet = goldberg(FREQUENCY);
        let mut rng = Rng::seed_from_u64(0xB0A7);

        // Uniform noise rather than a wave field: it is nothing but lone tiles,
        // which is the input that would expose a rule needing a second pass.
        for _ in 0..200 {
            let mut land: Vec<bool> = (0..planet.tile_count())
                .map(|_| rng.chance_percent(42))
                .collect();
            remove_lone_tiles(&planet, &mut land);

            let settled = land.clone();
            remove_lone_tiles(&planet, &mut land);
            assert_eq!(land, settled, "a second pass moved tiles the first left");

            for (tile, cell) in planet.cells().iter().enumerate() {
                let neighbors = land_neighbors(cell, &land);
                if land[tile] {
                    assert!(neighbors > 0, "tile {tile} is an island of one");
                } else {
                    assert!(neighbors < cell.sides(), "tile {tile} is a lake of one");
                }
            }
        }
    }

    #[test]
    fn smoothing_reads_the_state_the_pass_started_in() {
        // A sequential pass would give a different answer walked backwards; a
        // simultaneous one gives the same answer whatever order it is walked,
        // which is what keeps the planet independent of the tile numbering.
        let planet = goldberg(FREQUENCY);
        let field = field(&planet, 3);
        let cut = quantile_cut(&planet, &field);

        let mut forwards: Vec<bool> = field.iter().map(|&e| e > cut).collect();
        let before = forwards.clone();
        smooth_coastline(&planet, &mut forwards);

        // The same rule, applied by hand from the untouched snapshot.
        let by_hand: Vec<bool> = planet
            .cells()
            .iter()
            .enumerate()
            .map(|(tile, cell)| {
                let neighbors = land_neighbors(cell, &before);
                if before[tile] {
                    neighbors >= MIN_LAND_NEIGHBORS
                } else {
                    neighbors + 1 >= cell.sides()
                }
            })
            .collect();
        assert_eq!(forwards, by_hand);
    }

    #[test]
    fn the_fingerprint_notices_a_single_tile() {
        let planet = goldberg(FREQUENCY);
        let map = terrain(&planet, 11);
        let mut moved = map.clone();
        let tile = moved
            .tiles
            .iter()
            .position(|&t| t == Terrain::Land)
            .expect("a planet has land");
        moved.tiles[tile] = Terrain::Water;
        moved.land -= 1;
        assert_ne!(map.terrain_hash(), moved.terrain_hash());
    }
}
