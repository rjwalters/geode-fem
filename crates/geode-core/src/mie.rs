//! Analytic resonance roots for a dielectric sphere inside a PEC
//! cavity (issue #4 — Mie benchmark v0).
//!
//! This module provides the **analytic ground truth** that the FEM
//! sphere benchmark in `examples/mie_sphere.rs` compares against.
//!
//! # Physical setup
//!
//! Two-shell concentric geometry that exactly matches the bundled
//! sphere fixture:
//!
//! - **Inner sphere** `0 ≤ r ≤ R_s`: dielectric with refractive index
//!   `n` (relative permittivity `ε_r = n²`, `μ_r = 1`).
//! - **Vacuum buffer** `R_s ≤ r ≤ R_b`: vacuum (`ε_r = 1`, `μ_r = 1`).
//! - **Outer wall** at `r = R_b`: perfect electric conductor (PEC).
//!
//! This is **not** the open-space Mie scattering problem (which has
//! complex resonance frequencies due to radiative leakage). The PEC
//! outer wall closes the cavity, making the spectrum purely real. It
//! is precisely the limit the FEM solver hits when the scalar
//! isotropic PML absorption strength `σ₀ → 0`: the buffer becomes
//! transparent and the PEC wall at `r = R_b` sets the boundary
//! condition.
//!
//! v0 of the Mie benchmark cross-checks FEM modes against **these**
//! analytic roots. The true open-space Mie WGM positions (which need
//! Hankel functions and complex Newton iteration) are a v1 extension —
//! see the README cross-link with `strata-fdtd`, which computes the
//! same physical problem in the time domain.
//!
//! # Characteristic equations
//!
//! For each angular order `l ≥ 1` we have a `2 × 2` matching system at
//! `r = R_s` (continuity of tangential `E` and `H`) plus a single PEC
//! condition at `r = R_b`. Eliminating the buffer coefficients gives a
//! scalar characteristic function whose real roots `k` are the
//! resonances. See `characteristic_te` / `characteristic_tm` below.
//!
//! Closed forms for `l = 1, 2` and the upward recurrence
//! `j_l(x) = (2l-1)/x · j_{l-1}(x) - j_{l-2}(x)` (and the analogous
//! recurrence for `y_l`) for higher `l`. Roots are extracted by dense
//! sampling + sign-change bracketing + bisection refinement.

/// Polarisation of an electromagnetic resonance in the spherical
/// cavity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiePolarisation {
    /// Transverse-electric. `E` has no radial component; the matching
    /// condition at `r = R_s` reduces to continuity of the
    /// Riccati-Bessel `ψ` and `ψ'`.
    TE,
    /// Transverse-magnetic. `H` has no radial component; the matching
    /// condition at `r = R_s` mixes in the `1/n²` permittivity jump.
    TM,
}

/// A single analytic resonance root of the PEC-cavity dielectric
/// sphere problem.
#[derive(Debug, Clone, Copy)]
pub struct MieRoot {
    /// Polarisation channel (TE or TM).
    pub pol: MiePolarisation,
    /// Angular order `l ≥ 1`.
    pub l: usize,
    /// Radial order `n ≥ 1` (1 = lowest root for this `l`, `pol`).
    pub n: usize,
    /// Resonance position `k` (units: inverse length, same units as
    /// `R_s`, `R_b`).
    pub k: f64,
}

/// Spherical Bessel function `j_l(x)` of the first kind, real arg.
///
/// Closed forms for `l = 0, 1`; upward recurrence for `l ≥ 2`. Stable
/// for `x ≳ l` (the regime we evaluate in).
pub fn spherical_j(l: usize, x: f64) -> f64 {
    if x.abs() < 1e-14 {
        return if l == 0 { 1.0 } else { 0.0 };
    }
    let s = x.sin();
    let c = x.cos();
    let j0 = s / x;
    if l == 0 {
        return j0;
    }
    let j1 = s / (x * x) - c / x;
    if l == 1 {
        return j1;
    }
    let mut prev = j0;
    let mut curr = j1;
    for k in 2..=l {
        let next = ((2 * k - 1) as f64) / x * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Spherical Bessel function `y_l(x)` of the second kind, real arg.
///
/// Singular at the origin (`y_l(0) = -∞`); only evaluated at `x > 0`
/// in the buffer region. Closed forms for `l = 0, 1`; upward
/// recurrence (same as `j_l`) for `l ≥ 2`.
pub fn spherical_y(l: usize, x: f64) -> f64 {
    assert!(x > 0.0, "spherical_y is singular at x=0");
    let s = x.sin();
    let c = x.cos();
    let y0 = -c / x;
    if l == 0 {
        return y0;
    }
    let y1 = -c / (x * x) - s / x;
    if l == 1 {
        return y1;
    }
    let mut prev = y0;
    let mut curr = y1;
    for k in 2..=l {
        let next = ((2 * k - 1) as f64) / x * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Derivative `j_l'(x) = j_{l-1}(x) - (l+1)/x · j_l(x)`.
pub fn spherical_j_prime(l: usize, x: f64) -> f64 {
    if x.abs() < 1e-14 {
        return if l == 1 { 1.0 / 3.0 } else { 0.0 };
    }
    if l == 0 {
        return -spherical_j(1, x);
    }
    spherical_j(l - 1, x) - ((l + 1) as f64) / x * spherical_j(l, x)
}

/// Derivative `y_l'(x) = y_{l-1}(x) - (l+1)/x · y_l(x)`.
pub fn spherical_y_prime(l: usize, x: f64) -> f64 {
    assert!(x > 0.0, "y_l' is singular at x=0");
    if l == 0 {
        return -spherical_y(1, x);
    }
    spherical_y(l - 1, x) - ((l + 1) as f64) / x * spherical_y(l, x)
}

/// Riccati-Bessel functions `ψ_l(x) = x · j_l(x)` and
/// `χ_l(x) = -x · y_l(x)`.
pub fn psi(l: usize, x: f64) -> f64 {
    x * spherical_j(l, x)
}

/// Derivative `ψ_l'(x) = j_l(x) + x · j_l'(x)`.
pub fn psi_prime(l: usize, x: f64) -> f64 {
    spherical_j(l, x) + x * spherical_j_prime(l, x)
}

/// Riccati-Bessel `χ_l(x) = -x · y_l(x)` (Bohren-Huffman convention).
pub fn chi(l: usize, x: f64) -> f64 {
    -x * spherical_y(l, x)
}

/// Derivative `χ_l'(x) = -y_l(x) - x · y_l'(x)`.
pub fn chi_prime(l: usize, x: f64) -> f64 {
    -spherical_y(l, x) - x * spherical_y_prime(l, x)
}

/// TE characteristic function for the dielectric-sphere-in-PEC-cavity
/// resonance.
///
/// The system in the unknowns `(A, B)` (buffer-region coefficients of
/// `j_l` and `y_l` for the tangential electric field) is
///
/// ```text
/// (1) ψ_l(n·k·R_s) = A · ψ_l(k·R_s) − B · χ_l(k·R_s)
/// (2) (1/n) · ψ_l'(n·k·R_s) = A · ψ_l'(k·R_s) − B · χ_l'(k·R_s)
/// (3) A · ψ_l(k·R_b) − B · χ_l(k·R_b) = 0   (PEC: E_θ = 0)
/// ```
///
/// Equation (3) defines `B/A`. Substituting into the determinant of
/// the matching pair (1, 2) at `r = R_s` yields the scalar
/// characteristic function returned here. A zero of this function in
/// `k` is a resonance.
pub fn characteristic_te(n: f64, l: usize, r_s: f64, r_b: f64, k: f64) -> f64 {
    let x_in = n * k * r_s; // dielectric argument at the interface
    let x_s = k * r_s; // vacuum argument at the interface
    let x_b = k * r_b; // vacuum argument at the outer wall

    // Buffer coefficients up to overall scale: A = χ(x_b), B = ψ(x_b),
    // so that A·ψ(x_b) - B·χ(x_b) = 0 (PEC). This form has no spurious
    // pole when χ(x_b) → 0.
    let big_a = chi(l, x_b);
    let big_b = psi(l, x_b);

    let buf = big_a * psi(l, x_s) - big_b * chi(l, x_s);
    let buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s);

    // Matching at r = R_s:
    //   ψ(x_in)      = buf
    //   ψ'(x_in) / n = buf_prime
    // → resonance condition: ψ(x_in) * buf_prime - ψ'(x_in)/n * buf = 0.
    psi(l, x_in) * buf_prime - (psi_prime(l, x_in) / n) * buf
}

/// TM characteristic function for the dielectric-sphere-in-PEC-cavity
/// resonance.
///
/// The matching system differs from TE in two places: the magnetic
/// continuity equation picks up a `1/n²` permittivity factor (from
/// `∂E_r / ∂r` mismatch) and the PEC condition becomes
/// `∂_r (r · E_r) = 0` ≡ `ψ_l'(k·R_b) = 0` in Riccati-Bessel form,
/// i.e., the derivative of `ψ` (not `ψ` itself) vanishes.
pub fn characteristic_tm(n: f64, l: usize, r_s: f64, r_b: f64, k: f64) -> f64 {
    let x_in = n * k * r_s;
    let x_s = k * r_s;
    let x_b = k * r_b;

    // TM PEC condition: ∂_r(r E_r) = 0 ↔ ψ'(x_b) = 0, so use
    // A = χ'(x_b), B = ψ'(x_b).
    let big_a = chi_prime(l, x_b);
    let big_b = psi_prime(l, x_b);

    let buf = big_a * psi(l, x_s) - big_b * chi(l, x_s);
    let buf_prime = big_a * psi_prime(l, x_s) - big_b * chi_prime(l, x_s);

    // Matching at r = R_s for TM:
    //   ψ(x_in)        = buf
    //   n · ψ'(x_in)   = buf_prime         (ε ratio factor)
    // → ψ(x_in) * buf_prime - n · ψ'(x_in) * buf = 0.
    psi(l, x_in) * buf_prime - n * psi_prime(l, x_in) * buf
}

/// Find real roots of `f` on `[k_min, k_max]` via dense sampling +
/// sign-change bracketing + bisection refinement to ~12 sig figs.
fn find_roots<F: Fn(f64) -> f64>(f: F, k_min: f64, k_max: f64, n_samples: usize) -> Vec<f64> {
    assert!(k_max > k_min);
    assert!(n_samples >= 3);

    let dk = (k_max - k_min) / (n_samples as f64);
    let ks: Vec<f64> = (0..=n_samples).map(|i| k_min + (i as f64) * dk).collect();
    let fs: Vec<f64> = ks.iter().map(|&k| f(k)).collect();

    let mut roots = Vec::new();
    for i in 0..n_samples {
        let (a, b) = (ks[i], ks[i + 1]);
        let (fa, fb) = (fs[i], fs[i + 1]);
        if !fa.is_finite() || !fb.is_finite() {
            continue;
        }
        if fa == 0.0 && fb == 0.0 {
            continue;
        }
        if fa * fb > 0.0 {
            continue;
        }
        // Reject brackets where the *magnitude* on both sides is
        // enormous — those are spurious sign flips across a pole of
        // the characteristic function (e.g., zero of `χ_l(k·R_b)`).
        let scale = fa.abs().min(fb.abs());
        if scale > 1e8 {
            continue;
        }

        let mut lo = a;
        let mut hi = b;
        let mut f_lo = fa;
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            let f_mid = f(mid);
            if !f_mid.is_finite() {
                break;
            }
            if f_mid == 0.0 || (hi - lo) < 1e-12 * mid.abs().max(1.0) {
                lo = mid;
                hi = mid;
                break;
            }
            if f_lo * f_mid < 0.0 {
                hi = mid;
            } else {
                lo = mid;
                f_lo = f_mid;
            }
        }
        roots.push(0.5 * (lo + hi));
    }
    roots
}

/// Lowest `n_max` analytic resonance positions `k` for the
/// PEC-cavity dielectric-sphere problem with refractive-index contrast
/// `n` and geometry `(R_s, R_b)`.
///
/// Searches `k ∈ (0.1, k_max]` with `k_max = 20` chosen to include
/// the lowest several roots for `l ≤ 5` and `n ∈ [1.0, 4.0]`.
pub fn resonance_roots(
    pol: MiePolarisation,
    n: f64,
    l: usize,
    r_s: f64,
    r_b: f64,
    n_max: usize,
) -> Vec<MieRoot> {
    assert!(n > 0.0);
    assert!(l >= 1);
    assert!(r_b > r_s);

    let k_min = 0.1;
    let k_max = 20.0;
    let n_samples = 30_000;

    let f = |k: f64| -> f64 {
        match pol {
            MiePolarisation::TE => characteristic_te(n, l, r_s, r_b, k),
            MiePolarisation::TM => characteristic_tm(n, l, r_s, r_b, k),
        }
    };

    let mut raw = find_roots(f, k_min, k_max, n_samples);
    raw.dedup_by(|a, b| (*a - *b).abs() < 1e-5);

    raw.into_iter()
        .take(n_max)
        .enumerate()
        .map(|(idx, k)| MieRoot {
            pol,
            l,
            n: idx + 1,
            k,
        })
        .collect()
}

/// Convenience: lowest `n_max` TE and TM roots for `l ∈ l_set`,
/// merged and sorted by ascending `k`.
pub fn merged_roots(n: f64, l_set: &[usize], r_s: f64, r_b: f64, n_max: usize) -> Vec<MieRoot> {
    let mut all = Vec::new();
    for &l in l_set {
        all.extend(resonance_roots(MiePolarisation::TE, n, l, r_s, r_b, n_max));
        all.extend(resonance_roots(MiePolarisation::TM, n, l, r_s, r_b, n_max));
    }
    all.sort_by(|a, b| a.k.partial_cmp(&b.k).unwrap());
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spherical_j_closed_forms() {
        let pi = std::f64::consts::PI;
        assert!((spherical_j(0, pi)).abs() < 1e-12);
        assert!((spherical_j(0, pi / 2.0) - 2.0 / pi).abs() < 1e-12);
        assert!((spherical_j(1, pi) - 1.0 / pi).abs() < 1e-12);
    }

    #[test]
    fn spherical_j_recurrence_matches_closed_form_l2() {
        for &x in &[1.0_f64, 2.5, 4.7, 6.3, 8.1] {
            let closed = (3.0 / (x * x) - 1.0) * x.sin() / x - 3.0 * x.cos() / (x * x);
            let recur = spherical_j(2, x);
            assert!(
                (closed - recur).abs() < 1e-12,
                "j_2({x}): closed = {closed}, recur = {recur}"
            );
        }
    }

    #[test]
    fn spherical_y_closed_forms() {
        // y_0(x) = -cos(x)/x ; y_1(x) = -cos(x)/x^2 - sin(x)/x.
        let x = 2.3_f64;
        let y0_closed = -x.cos() / x;
        let y1_closed = -x.cos() / (x * x) - x.sin() / x;
        assert!((spherical_y(0, x) - y0_closed).abs() < 1e-12);
        assert!((spherical_y(1, x) - y1_closed).abs() < 1e-12);
    }

    #[test]
    fn psi_zero_at_origin() {
        for l in 0..=4 {
            assert_eq!(psi(l, 0.0), 0.0, "ψ_{l}(0) must be 0");
        }
    }

    #[test]
    fn pec_vacuum_limit_te_first_root_is_j_l_zero() {
        // Limit n → 1: the two regions merge into one vacuum cavity
        // with PEC at r = R_b. The TE characteristic should reduce to
        // ψ_l(k·R_b) = 0  ↔  j_l(k·R_b) = 0. For l = 1, R_b = 1, the
        // first zero of j_1 is at k·R_b ≈ 4.4934.
        // Put R_s strictly inside so the matching is well-defined.
        let roots = resonance_roots(MiePolarisation::TE, 1.0, 1, 0.5, 1.0, 1);
        assert!(!roots.is_empty());
        let k = roots[0].k;
        assert!(
            (k - 4.4934).abs() < 1e-2,
            "n=1 vacuum TE_1 ground k = {k}, expected ≈ 4.4934 (first zero of j_1)"
        );
    }

    #[test]
    fn lowest_roots_are_finite_and_sorted_for_fixture_geometry() {
        // The bundled fixture has R_SPHERE = 1.0, R_BUFFER = 2.0 and
        // we use n = 1.5 in the benchmark. The lowest few roots must
        // be finite and sorted; we don't pin specific numerical
        // values here (those are recorded in the benchmark output).
        let roots = merged_roots(1.5, &[1, 2], 1.0, 2.0, 3);
        assert!(roots.len() >= 4, "expected ≥ 4 roots, got {}", roots.len());
        for w in roots.windows(2) {
            assert!(w[0].k <= w[1].k, "merged_roots must be sorted by k");
        }
        for r in &roots {
            assert!(r.k > 0.0 && r.k.is_finite());
        }
    }
}
