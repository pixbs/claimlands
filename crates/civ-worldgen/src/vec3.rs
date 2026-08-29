//! A minimal 3-vector, for planet geometry only.
//!
//! Hand-rolled rather than pulled from a linear-algebra crate: the planet
//! builder needs six operations, and every dependency added this close to
//! `civ-core` is one more thing a reviewer has to audit before trusting a
//! replay. Six operations are cheaper to read than a crate is to vet.
//!
//! `f64` rather than `f32` because the barycentric mix compounds rounding
//! across the lattice and the precision is free here — the sphere is built
//! once, off any hot path, and `civ-procgen` narrows to `f32` when it fills a
//! vertex buffer.
//!
//! Nothing in this module is used by `civ-core`, which does no floating-point
//! arithmetic at all. See `docs/determinism.md`.

use std::ops::{Add, Mul, Sub};

/// A point or direction in world space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(self, other: Self) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    pub fn length(self) -> f64 {
        self.dot(self).sqrt()
    }

    /// The same direction, scaled to unit length.
    ///
    /// A zero vector comes back unchanged rather than as `NaN`. The geodesic
    /// builder never produces one — a barycentric mix of three points on a
    /// sphere always sits inside their triangle, well away from the centre —
    /// but a silent `NaN` would propagate into every triangle touching it, and
    /// returning the input keeps the failure local enough to see.
    pub fn normalize(self) -> Self {
        let length = self.length();
        if length == 0.0 {
            self
        } else {
            self * (1.0 / length)
        }
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, other: Self) -> Self {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Mul<f64> for Vec3 {
    type Output = Self;
    fn mul(self, scalar: f64) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cross_is_right_handed() {
        let x = Vec3::new(1.0, 0.0, 0.0);
        let y = Vec3::new(0.0, 1.0, 0.0);
        assert_eq!(x.cross(y), Vec3::new(0.0, 0.0, 1.0));
    }

    #[test]
    fn normalize_gives_unit_length() {
        let v = Vec3::new(3.0, 4.0, 0.0);
        assert_eq!(v.length(), 5.0);
        let n = v.normalize();
        assert!((n.length() - 1.0).abs() < 1e-15);
        assert!((n - Vec3::new(0.6, 0.8, 0.0)).length() < 1e-15);
    }

    #[test]
    fn normalizing_zero_does_not_produce_nan() {
        assert_eq!(Vec3::ZERO.normalize(), Vec3::ZERO);
    }

    #[test]
    fn arithmetic_is_componentwise() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        assert_eq!(a + b, Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(b - a, Vec3::new(3.0, 3.0, 3.0));
        assert_eq!(a * 2.0, Vec3::new(2.0, 4.0, 6.0));
        assert_eq!(a.dot(b), 32.0);
    }
}
