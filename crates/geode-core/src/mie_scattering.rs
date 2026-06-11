//! Analytic Mie scattering efficiencies `Q_ext` / `Q_sca` for a
//! homogeneous dielectric sphere in vacuum (issue #195, Epic #193).
//!
//! Where [`crate::mie`] tabulates PEC-cavity resonance *positions* and
//! [`crate::mie_open`] the open-space resonance *poles*, this module
//! evaluates the full Mie scattering **series** for a plane wave
//! incident on a sphere of (real) refractive index `m` and size
//! parameter `x = k·a`:
//!
//! ```text
//! a_l = [ m ψ_l(mx) ψ_l'(x) − ψ_l(x) ψ_l'(mx) ]
//!     / [ m ψ_l(mx) ξ_l'(x) − ξ_l(x) ψ_l'(mx) ]
//!
//! b_l = [ ψ_l(mx) ψ_l'(x) − m ψ_l(x) ψ_l'(mx) ]
//!     / [ ψ_l(mx) ξ_l'(x) − m ξ_l(x) ψ_l'(mx) ]
//!
//! Q_ext = (2/x²) Σ_l (2l+1) Re(a_l + b_l)
//! Q_sca = (2/x²) Σ_l (2l+1) (|a_l|² + |b_l|²)
//! ```
//!
//! with the Riccati-Bessel functions `ψ_l(z) = z·j_l(z)`,
//! `χ_l(z) = −z·y_l(z)`, and `ξ_l(z) = ψ_l(z) − i·χ_l(z) = z·h_l⁽¹⁾(z)`
//! (Bohren & Huffman, *Absorption and Scattering of Light by Small
//! Particles*, §4.4, `exp(−iωt)` convention). The efficiencies are
//! real scalars, so they are convention-independent and can be compared
//! directly against the FEM driven-scattering benchmark, which uses the
//! codebase's `exp(+jωt)` convention.
//!
//! # Relation to the open-space WGM catalogue
//!
//! The denominators above are exactly the open-space TE/TM
//! characteristic functions of [`crate::mie_open`] (up to the constant
//! factor `m` on the `a_l` denominator): the scattering coefficients'
//! complex poles **are** the open-space Mie resonances. A unit test
//! below pins this identity on a real-axis grid so the scattering
//! series and the resonance catalogue cannot silently drift apart.
//!
//! # Numerical notes
//!
//! All special functions come from [`crate::mie`]'s real-argument
//! ladders ([`crate::mie::spherical_j`] is Miller-stabilized for
//! `l > x + 1`, `y_l` is upward-stable), so the series is accurate to
//! ~1e-12 relative over the benchmark range `x ∈ (0, 10]`. The series
//! is truncated at the Wiscombe criterion
//! `l_max = ⌈x + 4·x^{1/3} + 2⌉` (Wiscombe, *Appl. Opt.* 19, 1505
//! (1980)).
//!
//! The independent NumPy sidecar
//! (`reference/numpy/mie_efficiencies.py`) implements the same physics
//! via the **logarithmic-derivative downward recurrence** (BHMIE
//! algorithm) instead of direct `ψ_l(mx)` evaluation, so cross-IR
//! agreement (`crates/geode-validation/tests/`
//! `mie_efficiencies_numpy_reference.rs`) pins the mathematics rather
//! than a shared algorithm.

use faer::c64;

use crate::mie::{chi, chi_prime, psi, psi_prime};

/// A single pair of Mie scattering coefficients `(a_l, b_l)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MieCoefficients {
    /// Angular order `l ≥ 1`.
    pub l: usize,
    /// Electric (TM-type, B&H `a_n`) scattering coefficient.
    pub a: c64,
    /// Magnetic (TE-type, B&H `b_n`) scattering coefficient.
    pub b: c64,
}

/// Mie scattering efficiencies at a single size parameter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MieEfficiencies {
    /// Size parameter `x = k·a`.
    pub x: f64,
    /// Extinction efficiency `Q_ext = C_ext / (π a²)`.
    pub q_ext: f64,
    /// Scattering efficiency `Q_sca = C_sca / (π a²)`.
    pub q_sca: f64,
    /// Number of series terms used (Wiscombe truncation).
    pub n_terms: usize,
}

/// Wiscombe series-truncation order `l_max = ⌈x + 4·x^{1/3} + 2⌉`.
///
/// Valid (and conservative) for the benchmark range `x ≤ 10`; clamped
/// below at 3 so the small-`x` Rayleigh regime still carries the dipole
/// and quadrupole terms.
pub fn mie_series_order(x: f64) -> usize {
    assert!(x > 0.0, "size parameter must be positive");
    ((x + 4.0 * x.cbrt() + 2.0).ceil() as usize).max(3)
}

/// Riccati-Hankel `ξ_l(x) = ψ_l(x) − i·χ_l(x) = x·h_l⁽¹⁾(x)` for real
/// `x` (B&H `exp(−iωt)` convention).
fn xi(l: usize, x: f64) -> c64 {
    c64::new(psi(l, x), -chi(l, x))
}

/// `ξ_l'(x) = ψ_l'(x) − i·χ_l'(x)`.
fn xi_prime(l: usize, x: f64) -> c64 {
    c64::new(psi_prime(l, x), -chi_prime(l, x))
}

/// Mie scattering coefficients `(a_l, b_l)` for a nonmagnetic sphere of
/// real refractive index `m` at size parameter `x = k·a`, for a single
/// angular order `l ≥ 1` (Bohren & Huffman eq. 4.53).
pub fn mie_a_b(m: f64, x: f64, l: usize) -> MieCoefficients {
    assert!(l >= 1, "Mie series starts at l = 1");
    assert!(x > 0.0, "size parameter must be positive");
    assert!(m > 0.0, "refractive index must be positive");
    let mx = m * x;

    let psi_mx = psi(l, mx);
    let psi_p_mx = psi_prime(l, mx);
    let psi_x = psi(l, x);
    let psi_p_x = psi_prime(l, x);
    let xi_x = xi(l, x);
    let xi_p_x = xi_prime(l, x);

    // a_l: electric multipole. Numerator/denominator are the same
    // expression with ψ_l(x) → ξ_l(x) swapped in the second slot.
    let a_num = c64::new(m * psi_mx * psi_p_x - psi_x * psi_p_mx, 0.0);
    let a_den = xi_p_x * (m * psi_mx) - xi_x * psi_p_mx;
    // b_l: magnetic multipole.
    let b_num = c64::new(psi_mx * psi_p_x - m * psi_x * psi_p_mx, 0.0);
    let b_den = xi_p_x * psi_mx - xi_x * (m * psi_p_mx);

    MieCoefficients {
        l,
        a: a_num / a_den,
        b: b_num / b_den,
    }
}

/// Full Mie coefficient ladder `l = 1..=mie_series_order(x)`.
pub fn mie_coefficients(m: f64, x: f64) -> Vec<MieCoefficients> {
    (1..=mie_series_order(x))
        .map(|l| mie_a_b(m, x, l))
        .collect()
}

/// Extinction and scattering efficiencies `Q_ext`, `Q_sca` for a
/// nonmagnetic sphere of real refractive index `m` at size parameter
/// `x = k·a`.
///
/// For a lossless (real-`m`) sphere `Q_ext = Q_sca` analytically (no
/// absorption); the two are still computed through their independent
/// series (`Re(a+b)` vs `|a|² + |b|²`) so the identity doubles as an
/// internal consistency check (see `tests::lossless_qext_equals_qsca`).
pub fn mie_efficiencies(m: f64, x: f64) -> MieEfficiencies {
    let coeffs = mie_coefficients(m, x);
    let mut q_ext = 0.0_f64;
    let mut q_sca = 0.0_f64;
    for c in &coeffs {
        let w = (2 * c.l + 1) as f64;
        q_ext += w * (c.a.re + c.b.re);
        q_sca += w * (c.a.norm_sqr() + c.b.norm_sqr());
    }
    let scale = 2.0 / (x * x);
    MieEfficiencies {
        x,
        q_ext: scale * q_ext,
        q_sca: scale * q_sca,
        n_terms: coeffs.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mie_open::{characteristic_te_open, characteristic_tm_open};

    /// Rayleigh limit: `Q_sca → (8/3) x⁴ |(m²−1)/(m²+2)|²` as `x → 0`.
    #[test]
    fn rayleigh_limit_small_x() {
        let m = 1.5_f64;
        let x = 0.05_f64;
        let polarizability = (m * m - 1.0) / (m * m + 2.0);
        let q_rayleigh = (8.0 / 3.0) * x.powi(4) * polarizability * polarizability;
        let eff = mie_efficiencies(m, x);
        let rel = (eff.q_sca - q_rayleigh).abs() / q_rayleigh;
        assert!(
            rel < 0.01,
            "Q_sca({x}) = {} vs Rayleigh {} (rel err {rel:.2e})",
            eff.q_sca,
            q_rayleigh
        );
    }

    /// Lossless sphere: `Q_ext = Q_sca` exactly; the two independent
    /// series must agree to recurrence accuracy.
    #[test]
    fn lossless_qext_equals_qsca() {
        for &x in &[0.3_f64, 1.0, 1.9, 2.6, 3.0, 5.0, 8.0] {
            let eff = mie_efficiencies(1.5, x);
            let rel = (eff.q_ext - eff.q_sca).abs() / eff.q_ext.abs();
            assert!(
                rel < 1e-10,
                "x = {x}: Q_ext = {} vs Q_sca = {} (rel {rel:.3e})",
                eff.q_ext,
                eff.q_sca
            );
        }
    }

    /// For a nonabsorbing sphere each coefficient lies on the unitary
    /// circle `|c − ½| = ½`, equivalently `Re(c) = |c|²` — a sharp
    /// per-coefficient correctness identity (B&H §4.4.3).
    #[test]
    fn coefficients_are_unitary_for_real_m() {
        for &x in &[0.5_f64, 1.26, 1.88, 3.0, 6.0] {
            for c in mie_coefficients(1.5, x) {
                let da = (c.a.re - c.a.norm_sqr()).abs();
                let db = (c.b.re - c.b.norm_sqr()).abs();
                assert!(
                    da < 1e-10 && db < 1e-10,
                    "x = {x}, l = {}: Re(a)−|a|² = {da:.3e}, Re(b)−|b|² = {db:.3e}",
                    c.l
                );
            }
        }
    }

    /// The scattering-coefficient denominators are the open-space
    /// resonance characteristic functions of `mie_open` (the `a_l`
    /// denominator carries an extra constant factor `m`). Pin the
    /// identity on a real-axis grid so the scattering series and the
    /// WGM catalogue cannot drift apart.
    #[test]
    fn denominators_match_open_space_characteristics() {
        let m = 1.5_f64;
        for &x in &[0.7_f64, 1.3, 2.1, 3.4, 5.5] {
            for l in 1..=5_usize {
                let mx = m * x;
                let a_den = xi_prime(l, x) * (m * psi(l, mx)) - xi(l, x) * psi_prime(l, mx);
                let b_den = xi_prime(l, x) * psi(l, mx) - xi(l, x) * (m * psi_prime(l, mx));

                let z = c64::new(x, 0.0);
                // mie_open's "TE" condition is ψ ξ' − (1/m) ψ' ξ = a_den / m;
                // its "TM" condition is ψ ξ' − m ψ' ξ = b_den.
                let te = characteristic_te_open(m, l, 1.0, z) * m;
                let tm = characteristic_tm_open(m, l, 1.0, z);

                let ea = (a_den - te).norm() / a_den.norm().max(1e-30);
                let eb = (b_den - tm).norm() / b_den.norm().max(1e-30);
                assert!(
                    ea < 1e-9 && eb < 1e-9,
                    "x = {x}, l = {l}: a_den vs m·TE rel {ea:.3e}, b_den vs TM rel {eb:.3e}"
                );
            }
        }
    }

    /// Series truncation: the Wiscombe order is converged — doubling
    /// the number of terms moves the efficiencies by < 1e-12 relative.
    #[test]
    fn wiscombe_truncation_is_converged() {
        let m = 1.5_f64;
        for &x in &[1.0_f64, 2.5, 5.0] {
            let n = mie_series_order(x);
            let q: f64 = (1..=n)
                .map(|l| {
                    let c = mie_a_b(m, x, l);
                    (2 * l + 1) as f64 * (c.a.re + c.b.re)
                })
                .sum();
            let q2: f64 = (1..=2 * n)
                .map(|l| {
                    let c = mie_a_b(m, x, l);
                    (2 * l + 1) as f64 * (c.a.re + c.b.re)
                })
                .sum();
            let rel = (q - q2).abs() / q.abs();
            assert!(rel < 1e-12, "x = {x}: truncation rel err {rel:.3e}");
        }
    }
}
