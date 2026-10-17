//! Cancellation-free principal complex square root `k = √λ` (issue #830).
//!
//! Every path that turns a complex eigenvalue `λ = k²` into a wavenumber
//! `k` (and from there a quality factor `Q = Re k / (2|Im k|)`) shares
//! [`principal_sqrt`]. The textbook form
//!
//! ```text
//! Re k = √(½(|λ| + Re λ)),   Im k = ±√(½(|λ| − Re λ))
//! ```
//!
//! cancels catastrophically in the second formula when `|Im λ| ≪ Re λ`,
//! i.e. at high `Q`: `|λ| − Re λ ≈ (Im λ)² / (2 Re λ)` is formed as the
//! difference of two numbers that agree to `≈ 2 log₁₀(2Q)` digits, so
//! `Im k` carries a relative error of about `ε·Q²` and is **exactly 0**
//! once `(1/Q)² < ε` (`Q ≳ 6.7e7`), whatever the mesh length unit. A
//! high-Q resonator was then reported as lossless (`Q = ∞`).
//!
//! [`principal_sqrt`] instead evaluates only the non-cancelling half and
//! recovers the other from `Im λ = 2 Re k · Im k`:
//!
//! - `Re λ ≥ 0`: `Re k = √(½(|λ| + Re λ))`, `Im k = Im λ / (2 Re k)`;
//! - `Re λ < 0` (evanescent / overdamped): `|Im k| = √(½(|λ| − Re λ))`,
//!   `Re k = |Im λ| / (2|Im k|)`.
//!
//! Both components are then accurate to a few ulps for every `λ` (no
//! subtraction of nearly equal quantities anywhere), so `Q` is accurate
//! to a few ulps right up to the `Q ≈ 1.4e14` ceiling of
//! [`crate::eigen::self_consistent::Q_LOSSLESS_REL_TOL`].

use faer::c64;

/// Principal complex square root `k = √z`, computed without cancellation.
///
/// Branch and sign conventions (identical to the previous per-site
/// copies it replaces):
///
/// - `Re k ≥ 0` (principal branch);
/// - `sign(Im k) = sign(Im z)`, with `Im z = ±0` treated as non-negative,
///   so a real negative `z` maps to `+i√|z|` and every `Im k = 0` result
///   is `+0.0` (never `-0.0`);
/// - `z = 0` maps to `0`; an exactly real non-negative `z` maps to the
///   real `√z` with `Im k = +0.0` exactly (so
///   [`crate::eigen::self_consistent::q_factor`] still reports a lossless
///   mode as `Q = ∞`).
///
/// `Re z ≥ 0` uses `Re k = √(½(|z| + Re z))`, `Im k = Im z / (2 Re k)`;
/// `Re z < 0` mirrors it (`|Im k| = √(½(|z| − Re z))`,
/// `Re k = |Im z| / (2|Im k|)`). Neither half subtracts nearly equal
/// numbers, so both components are accurate to a few ulps at any `Q` (the
/// textbook `Im k = √(½(|z| − Re z))` loses `≈ log₁₀(2Q²)` digits and is
/// exactly 0 from `Q ≈ 6.7e7`; issue #830). `|z|` is formed with `hypot`.
pub fn principal_sqrt(z: c64) -> c64 {
    if z.re == 0.0 && z.im == 0.0 {
        return c64::new(0.0, 0.0);
    }
    let r = z.re.hypot(z.im);
    if z.re >= 0.0 {
        // r > 0 here, so re_k > 0 and the division is safe.
        let re_k = (0.5 * (r + z.re)).sqrt();
        // `+ 0.0` normalizes a `-0.0` (from `Im z = -0.0`) to `+0.0`.
        let im_k = z.im / (2.0 * re_k) + 0.0;
        c64::new(re_k, im_k)
    } else {
        // Re z < 0, so r - Re z = r + |Re z| > 0: no cancellation.
        let im_mag = (0.5 * (r - z.re)).sqrt();
        let re_k = z.im.abs() / (2.0 * im_mag);
        let im_k = if z.im >= 0.0 { im_mag } else { -im_mag };
        c64::new(re_k, im_k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eigen::self_consistent::q_factor;

    /// The textbook cancelling form the helper replaced, kept here only
    /// to demonstrate the failure mode it fixes.
    fn textbook_sqrt(z: c64) -> c64 {
        let r = (z.re * z.re + z.im * z.im).sqrt();
        let re = (0.5 * (r + z.re)).max(0.0).sqrt();
        let im_mag = (0.5 * (r - z.re)).max(0.0).sqrt();
        c64::new(re, if z.im >= 0.0 { im_mag } else { -im_mag })
    }

    /// `(Re λ, Im λ, Re k, Im k, Q)` with `k = √λ` and
    /// `Q = Re k / (2|Im k|)` evaluated by Python `mpmath` at
    /// `mp.dps = 50` on the **exact** binary64 `λ` (each `λ` literal is
    /// the shortest round-trip repr, so it parses to the same `f64` in
    /// Rust), printed to 20 significant digits.
    #[allow(clippy::excessive_precision)]
    const MPMATH_REF: &[(f64, f64, f64, f64, f64)] = &[
        // Re λ ≈ 2.3e3 (a mm-unit GHz cavity), Q = 1e2 … 1e14.
        (
            2345.6789,
            23.456789,
            48.43281470765996964,
            0.24215801973914758249,
            100.00249993750311753,
        ),
        (
            2345.6789,
            0.0023456789,
            48.432209323967312742,
            0.000024216104661977601319,
            1000000.0000002500424,
        ),
        (
            2345.6789,
            2.3456789e-05,
            48.432209323961259322,
            2.4216104661980628869e-7,
            100000000.00000000327,
        ),
        (
            2345.6789,
            2.3456788999999998e-07,
            48.432209323961258716,
            2.4216104661980628953e-9,
            10000000000.000000167,
        ),
        (
            2345.6789,
            2.3456789e-11,
            48.432209323961258716,
            2.4216104661980630634e-13,
            99999999999999.994731,
        ),
        // Re λ ≈ 3.7e-9 (a metre-unit mesh at ~20 MHz): same Q ladder.
        (
            3.7e-09,
            3.7e-11,
            0.00006082838562453894076,
            3.0413432495464529566e-7,
            100.00249993750312201,
        ),
        (
            3.7e-09,
            3.7e-15,
            0.000060827625302989800708,
            3.0413812651487297927e-11,
            1000000.0000002499663,
        ),
        (
            3.7e-09,
            3.7000000000000003e-17,
            0.000060827625302982198015,
            3.0413812651491100692e-13,
            99999999.999999994462,
        ),
        (
            3.7e-09,
            3.7e-19,
            0.000060827625302982197254,
            3.0413812651491098697e-15,
            9999999999.999999977,
        ),
        (
            3.7e-09,
            3.7e-23,
            0.000060827625302982197254,
            3.0413812651491100321e-19,
            99999999999999.994432,
        ),
        // Im λ < 0 (a growing mode): sign(Im k) follows Im λ.
        (
            2345.6789,
            -2.3456788999999998e-07,
            48.432209323961258716,
            -2.4216104661980628953e-9,
            10000000000.000000167,
        ),
        // Re λ < 0 (evanescent / overdamped), both signs of Im λ: the
        // mirrored branch, where the *real* part used to cancel.
        (
            -2345.6789,
            2.3456788999999998e-07,
            2.4216104661980628953e-9,
            48.432209323961258716,
            2.4999999999999999582e-11,
        ),
        (
            -2345.6789,
            -2.3456788999999998e-07,
            2.4216104661980628953e-9,
            -48.432209323961258716,
            2.4999999999999999582e-11,
        ),
        // Generic O(1) points and the Re λ = 0 boundary.
        (
            -1.0,
            0.5,
            0.24293413587832283909,
            1.0290855136357461252,
            0.1180339887498948482,
        ),
        (
            1.0,
            0.5,
            1.0290855136357461252,
            0.24293413587832283909,
            2.1180339887498948482,
        ),
        (0.0, 2.0, 1.0, 1.0, 0.5),
    ];

    fn rel(a: f64, b: f64) -> f64 {
        (a - b).abs() / b.abs()
    }

    #[test]
    fn principal_sqrt_matches_extended_precision_at_every_q() {
        // A few ulps: hypot (≤1 ulp), sqrt (½ ulp), the ½(r + Re) sum and
        // the Im/(2 Re) division each add ≤ 1 ulp.
        let tol = 4.0 * f64::EPSILON;
        for &(lre, lim, kre, kim, q) in MPMATH_REF {
            let k = principal_sqrt(c64::new(lre, lim));
            let old = textbook_sqrt(c64::new(lre, lim));
            // `--nocapture` prints the old-vs-new accuracy table (PR #830).
            eprintln!(
                "λ = {lre:e}{lim:+e}i  Q_ref = {q:.3e}  | new rel err Re {:.1e} Im {:.1e} Q {:.1e} \
                 | old rel err Re {:.1e} Im {:.1e} Q {:.1e}",
                rel(k.re, kre),
                rel(k.im, kim),
                rel(q_factor(k), q),
                rel(old.re, kre),
                rel(old.im, kim),
                rel(q_factor(old), q),
            );
            assert!(
                rel(k.re, kre) <= tol,
                "λ = {lre:e}{lim:+e}i: Re k = {:e} vs mpmath {kre:e} (rel {:e})",
                k.re,
                rel(k.re, kre)
            );
            assert!(
                rel(k.im, kim) <= tol,
                "λ = {lre:e}{lim:+e}i: Im k = {:e} vs mpmath {kim:e} (rel {:e})",
                k.im,
                rel(k.im, kim)
            );
            // Q is a ratio of the two: ≤ 2·tol, plus one rounding.
            let got_q = q_factor(k);
            assert!(
                rel(got_q, q) <= 3.0 * tol,
                "λ = {lre:e}{lim:+e}i: Q = {got_q:e} vs mpmath {q:e} (rel {:e})",
                rel(got_q, q)
            );
        }
    }

    #[test]
    fn textbook_form_fails_where_the_stable_form_does_not() {
        // Guards the test itself: on the same λ the replaced formula is
        // badly wrong at Q = 1e6 and exactly lossless at Q ≥ 1e8, so the
        // accuracy test above would catch a regression to it.
        let (lre, lim, _, kim, _) = MPMATH_REF[1]; // Q = 1e6
        let old = textbook_sqrt(c64::new(lre, lim));
        assert!(
            rel(old.im, kim) > 1e-6,
            "Q = 1e6 old Im k rel err {:e}",
            rel(old.im, kim)
        );
        for &(lre, lim, _, _, _) in &MPMATH_REF[2..5] {
            // Q = 1e8, 1e10, 1e14.
            let old = textbook_sqrt(c64::new(lre, lim));
            assert_eq!(old.im, 0.0, "old form at λ = {lre:e}{lim:+e}i");
            assert!(q_factor(old).is_infinite());
            assert!(q_factor(principal_sqrt(c64::new(lre, lim))).is_finite());
        }
    }

    #[test]
    fn exact_real_lambda_stays_lossless() {
        // Im λ = 0 exactly (a lossless pencil): Im k = +0 exactly and
        // `q_factor` keeps returning ∞ (#829's relative lossless test).
        for re in [1.0, 2.0, 2345.6789, 3.7e-9, 1e300, 5e-324] {
            for im in [0.0, -0.0] {
                let k = principal_sqrt(c64::new(re, im));
                assert_eq!(k.re, re.sqrt(), "√{re}");
                assert_eq!(
                    k.im.to_bits(),
                    0.0_f64.to_bits(),
                    "Im √({re}{im:+}i) must be +0"
                );
                assert!(q_factor(k).is_infinite(), "λ = {re} must stay lossless");
            }
        }
        // Real negative λ: purely imaginary k = +i√|λ| (old convention).
        for im in [0.0, -0.0] {
            let k = principal_sqrt(c64::new(-4.0, im));
            assert_eq!((k.re, k.im), (0.0, 2.0));
            assert_eq!(k.re.to_bits(), 0.0_f64.to_bits());
        }
        assert_eq!(principal_sqrt(c64::new(0.0, 0.0)), c64::new(0.0, 0.0));
        assert_eq!(
            principal_sqrt(c64::new(-0.0, -0.0)).im.to_bits(),
            0.0_f64.to_bits()
        );
    }

    #[test]
    fn principal_sqrt_agrees_with_old_form_on_its_branch_conventions() {
        // Same branch and signs as the per-site copies it replaced (they
        // differed only in precision): Re k ≥ 0, sign(Im k) = sign(Im λ),
        // and k² = λ. Swept over all four quadrants at moderate Q, where
        // the old form is accurate to ~ε·Q².
        for &re in &[-3.0, -1e-3, 0.0, 1e-3, 3.0] {
            for &im in &[-2.0, -1e-2, 1e-2, 2.0] {
                let z = c64::new(re, im);
                let k = principal_sqrt(z);
                let old = textbook_sqrt(z);
                assert!(k.re >= 0.0);
                assert_eq!(k.im.is_sign_negative(), im < 0.0, "√({re}{im:+}i)");
                assert!(
                    (k - old).norm() <= 1e-10 * old.norm(),
                    "√({re}{im:+}i): {k} vs {old}"
                );
                assert!(
                    (k * k - z).norm() <= 4.0 * f64::EPSILON * z.norm(),
                    "({k})² vs {z}"
                );
            }
        }
    }
}
