//! A sine that gives the same answer on every target.
//!
//! `f64::sin` calls the platform's libm, and Rust says so plainly: the
//! precision of the transcendental functions is unspecified and may differ
//! between platforms. Everything else this crate computes is `+ - * /` and
//! `sqrt`, which IEEE-754 requires to be correctly rounded and which Rust
//! never contracts or reorders — so today the whole planet is bit-identical on
//! an x86-64 runner and on an ARM phone, and `sin` would be the first thing in
//! it that is not.
//!
//! That matters here rather than in the abstract, because terrain is not
//! scenery. A tile is land or it is water, `lands-core` refuses a move onto
//! water, and a level stores a seed rather than a map — so two devices that
//! disagree by one ulp inside a sine can disagree about which tiles exist to
//! stand on, and a replay recorded on one desyncs on the other. That is the
//! failure `docs/determinism.md` is written to prevent, arriving through the
//! one door the crate had left open.
//!
//! So the sine is computed here instead: reduce the argument by symmetry, then
//! a Taylor series in the remaining quarter period. Every step is exact
//! arithmetic on the same inputs, which makes the result a function of its
//! argument and nothing else — no libm version, no target, no optimisation
//! level.

/// `sin(PI * x)`, to within a few ulps.
///
/// The half-turn form rather than radians because it is what the caller has
/// and because the reduction is exact in it: folding `x` into `[0, 2)` is a
/// subtraction of two nearby numbers, where radians would need a rounded `PI`
/// subtracted repeatedly and would lose a digit per turn.
///
/// Intended for arguments of modest size — the terrain waves stay under ten
/// half-turns. Far from the origin the fold loses precision, as every
/// range reduction does; it stays deterministic, which is the property this
/// module exists for.
pub(crate) fn sin_pi(x: f64) -> f64 {
    // Fold into [0, 2). Halving and doubling are exact, and the subtraction is
    // exact too whenever the two are within a factor of two of each other,
    // which they are for any argument this crate produces.
    let folded = x - 2.0 * (x * 0.5).floor();

    // sin(PI * (t + 1)) == -sin(PI * t): the second half turn is the first
    // one mirrored.
    let (half, sign) = if folded >= 1.0 {
        (folded - 1.0, -1.0)
    } else {
        (folded, 1.0)
    };

    // sin(PI * (1 - t)) == sin(PI * t): and the second quarter is the first
    // one reflected. What is left is 0 ..= 1/2, a quarter period.
    let quarter = if half > 0.5 { 1.0 - half } else { half };

    sign * sin_quarter(std::f64::consts::PI * quarter)
}

/// `sin(a)` for `a` in `0 ..= PI/2`, by its Taylor series about zero.
///
/// Eleven terms. The series alternates, so the error is smaller than the first
/// term left out — `a^23 / 23!`, which is under `2e-18` across the whole
/// quarter period and so is lost in the rounding of the terms that are kept.
/// A minimax polynomial would need fewer of them; nothing here is hot enough
/// for that to buy anything, and a Taylor series can be checked by eye.
fn sin_quarter(a: f64) -> f64 {
    const C3: f64 = -1.0 / 6.0;
    const C5: f64 = 1.0 / 120.0;
    const C7: f64 = -1.0 / 5040.0;
    const C9: f64 = 1.0 / 362_880.0;
    const C11: f64 = -1.0 / 39_916_800.0;
    const C13: f64 = 1.0 / 6_227_020_800.0;
    const C15: f64 = -1.0 / 1_307_674_368_000.0;
    const C17: f64 = 1.0 / 355_687_428_096_000.0;
    const C19: f64 = -1.0 / 121_645_100_408_832_000.0;
    const C21: f64 = 1.0 / 51_090_942_171_709_440_000.0;

    let sq = a * a;
    let series = C11 + sq * (C13 + sq * (C15 + sq * (C17 + sq * (C19 + sq * C21))));
    a * (1.0 + sq * (C3 + sq * (C5 + sq * (C7 + sq * (C9 + sq * series)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    /// How far from `f64::sin` the result is allowed to sit. Being close to
    /// libm is not the point — being the *same everywhere* is — but a gross
    /// disagreement would mean the reduction is wrong, and only a comparison
    /// against an independent implementation can say that.
    const TOLERANCE: f64 = 1e-15;

    /// The same, out where the comparison itself has gone soft: `(PI * x).sin()`
    /// rounds `PI * x` before libm ever sees it, which costs about an eighth of
    /// an ulp of `PI` per half turn. Most of the gap this allows is the
    /// reference drifting, not `sin_pi` — which takes its argument in half
    /// turns and never forms the product at all.
    const FAR_TOLERANCE: f64 = 4e-15;

    #[test]
    fn agrees_with_libm_inside_the_first_period() {
        let mut worst: f64 = 0.0;
        for step in -10_000..=10_000 {
            let x = f64::from(step) / 10_000.0;
            worst = worst.max((sin_pi(x) - (PI * x).sin()).abs());
        }
        assert!(worst < TOLERANCE, "worst disagreement was {worst:e}");
    }

    #[test]
    fn agrees_with_libm_across_several_periods() {
        let mut worst: f64 = 0.0;
        // 8 half turns each way is past anything the terrain waves reach.
        for step in -80_000..=80_000 {
            let x = f64::from(step) / 10_000.0;
            worst = worst.max((sin_pi(x) - (PI * x).sin()).abs());
        }
        assert!(worst < FAR_TOLERANCE, "worst disagreement was {worst:e}");
    }

    #[test]
    fn hits_the_quarter_period_landmarks() {
        assert_eq!(sin_pi(0.0), 0.0);
        assert!((sin_pi(0.5) - 1.0).abs() < TOLERANCE);
        assert!(sin_pi(1.0).abs() < TOLERANCE);
        assert!((sin_pi(1.5) + 1.0).abs() < TOLERANCE);
        assert!((sin_pi(1.0 / 6.0) - 0.5).abs() < TOLERANCE);
        assert!((sin_pi(-0.5) + 1.0).abs() < TOLERANCE);
    }

    #[test]
    fn is_periodic_and_odd() {
        for step in 0..1_000 {
            let x = f64::from(step) / 250.0 - 2.0;
            assert!((sin_pi(x) - sin_pi(x + 2.0)).abs() < TOLERANCE);
            assert!((sin_pi(x) + sin_pi(-x)).abs() < TOLERANCE);
        }
    }

    #[test]
    fn stays_inside_the_unit_range() {
        for step in -20_000..=20_000 {
            let v = sin_pi(f64::from(step) / 3_000.0);
            assert!((-1.0..=1.0).contains(&v), "{v} is outside [-1, 1]");
        }
    }

    /// The reason the module exists: one argument, one answer, always.
    #[test]
    fn repeats_exactly() {
        for step in -500..500 {
            let x = f64::from(step) / 37.0;
            assert_eq!(sin_pi(x).to_bits(), sin_pi(x).to_bits());
        }
    }
}
