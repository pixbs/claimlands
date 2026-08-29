//! Deterministic randomness, split into independent streams.
//!
//! # Why this exists
//!
//! The obvious design — one RNG for the whole game — is a regression trap. Any
//! feature that draws a new random number shifts every subsequent draw, which
//! invalidates every stored replay and every golden test at once. That is
//! exactly the cross-agent breakage this project is built to avoid.
//!
//! Instead, every consumer derives its own stream from
//! `(world_seed, domain, turn, entity)`. Adding a new [`SeedDomain`] **cannot**
//! perturb an existing one, so a new feature can never invalidate an old
//! replay. This is the single most load-bearing property in `civ-core`.
//!
//! # Why the algorithms are hand-written
//!
//! splitmix64 and xoshiro256** are short, fully specified, integer-only and
//! public domain. Depending on a crate for them would mean a semver bump could
//! silently change every saved game.

use serde::{Deserialize, Serialize};

/// An independent random stream. Append new variants freely; never renumber
/// or remove an existing one, because that would change historical streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum SeedDomain {
    Terrain = 1,
    CoverSeeding = 2,
    ForestSpread = 3,
    CapitalRelocation = 4,
    AiJitter = 5,
    UnitPlacement = 6,
    // Append here. Do NOT renumber.
}

impl SeedDomain {
    #[inline]
    pub const fn code(self) -> u64 {
        self as u32 as u64
    }
}

/// xoshiro256\*\* — small, fast, and identical on every platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    s: [u64; 4],
}

/// Derive an independent stream.
///
/// `entity` distinguishes streams that would otherwise collide within one
/// domain and turn — a tile id, a unit id, a faction ordinal. Pass 0 when
/// there is only one consumer.
pub fn stream(world_seed: u64, domain: SeedDomain, turn: u32, entity: u32) -> Rng {
    // Fold the four coordinates through splitmix64 so that neighbouring
    // inputs (turn N vs N+1) produce completely unrelated streams.
    let mut mixer = SplitMix64::new(world_seed);
    mixer.absorb(domain.code());
    mixer.absorb(turn as u64);
    mixer.absorb(entity as u64);
    Rng::from_splitmix(&mut mixer)
}

impl Rng {
    /// Seed directly from a single value. Prefer [`stream`] in simulation code.
    pub fn seed_from_u64(seed: u64) -> Self {
        let mut mixer = SplitMix64::new(seed);
        Self::from_splitmix(&mut mixer)
    }

    fn from_splitmix(mixer: &mut SplitMix64) -> Self {
        let s = [mixer.next(), mixer.next(), mixer.next(), mixer.next()];
        // An all-zero state is the one degenerate case for xoshiro; splitmix64
        // makes it astronomically unlikely, but the guard costs nothing.
        if s == [0; 4] {
            return Self { s: [1, 2, 3, 4] };
        }
        Self { s }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        (self.next_u64() >> 32) as u32
    }

    /// Uniform in `0..n`. Returns 0 when `n == 0`.
    ///
    /// Rejection sampling rather than modulo, so the distribution is exactly
    /// uniform and the result does not depend on the machine's integer width.
    pub fn below(&mut self, n: u32) -> u32 {
        if n <= 1 {
            return 0;
        }
        let zone = u32::MAX - (u32::MAX % n) - 1;
        loop {
            let v = self.next_u32();
            if v <= zone {
                return v % n;
            }
        }
    }

    /// True with probability `percent/100`.
    pub fn chance_percent(&mut self, percent: u32) -> bool {
        if percent == 0 {
            return false;
        }
        if percent >= 100 {
            return true;
        }
        self.below(100) < percent
    }

    /// Pick one element uniformly. `None` for an empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.below(items.len() as u32) as usize)
    }
}

/// splitmix64, used only to expand a seed into xoshiro's 256-bit state.
struct SplitMix64 {
    x: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { x: seed }
    }

    /// Fold another value into the state before expanding.
    fn absorb(&mut self, v: u64) {
        self.x = self.x.wrapping_add(v.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        self.x ^= self.x >> 29;
    }

    fn next(&mut self) -> u64 {
        self.x = self.x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.x;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitmix64_matches_reference_vectors() {
        // Reference output for seed 0. Pinned: changing these changes every
        // world ever generated.
        let mut m = SplitMix64::new(0);
        assert_eq!(m.next(), 0xe220_a839_7b1d_cdaf);
        assert_eq!(m.next(), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(m.next(), 0x06c4_5d18_8009_454f);
    }

    #[test]
    fn same_inputs_give_the_same_stream() {
        let a: Vec<u64> = (0..8)
            .scan(stream(42, SeedDomain::Terrain, 3, 7), |r, _| {
                Some(r.next_u64())
            })
            .collect();
        let b: Vec<u64> = (0..8)
            .scan(stream(42, SeedDomain::Terrain, 3, 7), |r, _| {
                Some(r.next_u64())
            })
            .collect();
        assert_eq!(a, b);
    }

    /// The property the whole design rests on: streams are independent, so
    /// adding a new domain cannot disturb an existing one.
    #[test]
    fn domains_are_independent() {
        let mut terrain = stream(42, SeedDomain::Terrain, 0, 0);
        let mut forest = stream(42, SeedDomain::ForestSpread, 0, 0);
        let mut ai = stream(42, SeedDomain::AiJitter, 0, 0);
        let t: Vec<u64> = (0..4).map(|_| terrain.next_u64()).collect();
        let f: Vec<u64> = (0..4).map(|_| forest.next_u64()).collect();
        let a: Vec<u64> = (0..4).map(|_| ai.next_u64()).collect();
        assert_ne!(t, f);
        assert_ne!(f, a);
        assert_ne!(t, a);
    }

    #[test]
    fn adjacent_turns_do_not_correlate() {
        let mut a = stream(1, SeedDomain::ForestSpread, 100, 0);
        let mut b = stream(1, SeedDomain::ForestSpread, 101, 0);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut r = Rng::seed_from_u64(9);
        let mut seen = [false; 6];
        for _ in 0..2_000 {
            let v = r.below(6);
            assert!(v < 6);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "every outcome should appear");
    }

    #[test]
    fn chance_percent_honours_the_extremes() {
        let mut r = Rng::seed_from_u64(3);
        assert!(!r.chance_percent(0));
        assert!(r.chance_percent(100));
        assert!(r.chance_percent(200));
    }

    #[test]
    fn chance_percent_is_roughly_calibrated() {
        let mut r = Rng::seed_from_u64(11);
        let hits = (0..10_000).filter(|_| r.chance_percent(25)).count();
        assert!(
            (2_200..2_800).contains(&hits),
            "got {hits} hits, expected ~2500"
        );
    }
}
