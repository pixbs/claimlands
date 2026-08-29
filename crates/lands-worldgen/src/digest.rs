//! The fingerprint every snapshot in this crate is taken with.
//!
//! FNV-1a, written out by hand rather than pulled from a crate, for the same
//! reason `lands_core::hash` is: a committed snapshot value must not move
//! because a dependency changed its mind about how to serialise a struct.
//!
//! Only integers are ever fed to it. Coordinates are deliberately never
//! hashed — a compiler may contract or reorder floating-point arithmetic
//! differently per target, so a hash over coordinates would disagree between
//! an x86-64 CI runner and an ARM phone while describing the very same planet.
//! What must never drift is the *numbering*, and that is integer.

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Streaming FNV-1a writer, the same one `lands_core::hash` uses.
#[derive(Debug)]
pub(crate) struct Digest(u64);

impl Digest {
    pub(crate) fn new() -> Self {
        Self(FNV_OFFSET)
    }

    pub(crate) fn byte(&mut self, b: u8) {
        self.0 ^= u64::from(b);
        self.0 = self.0.wrapping_mul(FNV_PRIME);
    }

    pub(crate) fn u32(&mut self, v: u32) {
        for b in v.to_le_bytes() {
            self.byte(b);
        }
    }

    pub(crate) fn finish(self) -> u64 {
        self.0
    }
}
