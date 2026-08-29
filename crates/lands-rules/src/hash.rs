//! Stable fingerprint of a ruleset.
//!
//! Replays store this alongside the command log. If the balance changes in a
//! way that would make a stored replay diverge, loading it fails loudly rather
//! than silently producing a different game.
//!
//! FNV-1a is hand-rolled rather than pulled from a crate on purpose: the hash
//! must never change value between releases, and a dependency bump could do
//! exactly that. This is a change detector, not a security primitive.

use crate::Ruleset;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over a byte slice.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

/// Fingerprint a ruleset by hashing its canonical RON encoding.
///
/// Canonical because every map in [`Ruleset`] is a `BTreeMap`, so field and
/// key order are identical on every platform.
pub fn ruleset_hash(rules: &Ruleset) -> u64 {
    let canonical = ron::ser::to_string(rules).expect("ruleset is always serialisable");
    fnv1a(canonical.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_matches_reference_vectors() {
        // Published FNV-1a 64 test vectors. If these ever change, every stored
        // replay silently breaks, so they are pinned here deliberately.
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x85944171f73967e8);
    }
}
