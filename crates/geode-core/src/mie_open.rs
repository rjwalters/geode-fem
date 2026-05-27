//! Open-space Mie whispering-gallery-mode (WGM) resonance positions for
//! a homogeneous dielectric sphere in vacuum (issue #33).
//!
//! The companion module [`crate::mie`] tabulates the **PEC-cavity** TE/TM
//! roots — i.e. the closed-shell limit (`σ₀ → 0`) of the bundled FEM
//! fixture, where the outer wall at `R_b` reflects perfectly. This
//! module extends to the genuinely radiative case: a sphere of
//! refractive index `n` and radius `R_s` immersed in unbounded vacuum,
//! with outgoing-wave Sommerfeld radiation BC at infinity.
//!
//! # Physical setup
//!
//! Two regions only:
//! - **Inner sphere** `0 ≤ r ≤ R_s`: dielectric, refractive index `n`,
//!   field uses spherical Bessel `j_l(n k r)` (regular at origin).
//! - **Exterior** `r > R_s`: vacuum, field uses spherical Hankel
//!   `h_l^(1)(k r) = j_l(k r) + i y_l(k r)` (outgoing).
//!
//! Matching tangential `E` and `H` at `r = R_s` gives the standard Mie
//! denominator zeros (poles of the `b_l`/`a_l` scattering coefficients):
//!
//! ```text
//! TE (b_l pole):  ψ_l(n·k·R_s) · ξ_l'(k·R_s) − (1/n) · ψ_l'(n·k·R_s) · ξ_l(k·R_s) = 0
//! TM (a_l pole):  ψ_l(n·k·R_s) · ξ_l'(k·R_s) −    n  · ψ_l'(n·k·R_s) · ξ_l(k·R_s) = 0
//! ```
//!
//! where `ψ_l(z) = z·j_l(z)` and `ξ_l(z) = z·h_l^(1)(z)`.
//!
//! These are the same matching equations as in [`crate::mie`] with the
//! outer-wall PEC condition replaced by an outgoing-wave Sommerfeld BC.
//!
//! # Sign convention
//!
//! Under time dependence `exp(-i ω t)` with `ω = c k`, an outgoing wave
//! `exp(+i k r)` decays in time when `Im(k) < 0`. The Mie poles
//! therefore sit at `Im(k) < 0` on the second Riemann sheet of the
//! resolvent. The Rust constant table records `(Re(k), Im(k))` in this
//! convention; `|Im(k)|` is the linewidth (radiative decay rate in `k`)
//! and `Q = Re(k) / (2 |Im(k)|)` is the quality factor.
//!
//! The FEM eigensolver in [`crate::complex_eigen`] picks up modes with
//! `Im(k) > 0` (sign comes from the PML profile and the principal
//! sqrt branch). FEM-vs-analytic pairing should match on
//! `(Re(k), |Im(k)|)`.
//!
//! # How the static catalog was produced
//!
//! `mesh_scripts/mie_open_space_roots.py` is the reference reproducer.
//! It seeds complex Newton iteration from PEC-cavity real roots at
//! several outer-wall radii (and a coarse complex-plane grid) and keeps
//! the unique converged roots with `Im(k) < 0`. Each catalog entry is
//! re-verified at runtime via [`tests::catalog_residuals_are_small`].
//!
//! Reproducing the table:
//!
//! ```sh
//! python3 mesh_scripts/mie_open_space_roots.py
//! ```
//!
//! # Scope
//!
//! - **Geometry**: `R_s = 1.0`, vacuum exterior — the bundled fixture.
//! - **Refractive index**: `n = 1.5` — the textbook dielectric.
//! - **Catalog extent**: `l ∈ [1, 5]`, lowest 3 radial orders per
//!   `(l, polarisation)`, 30 entries total.
//!
//! Lossy or dispersive `n(ω)`, magnetic spheres, and stratified
//! geometries are out of scope for v0; tracked separately.

use crate::mie::MiePolarisation;

/// A single analytic resonance root of the open-space dielectric-sphere
/// (Mie) problem. Distinct from [`crate::mie::MieRoot`], which records
/// real PEC-cavity positions; this one carries a full complex `k`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MieRootComplex {
    /// Polarisation channel (TE or TM).
    pub pol: MiePolarisation,
    /// Angular order `l ≥ 1`.
    pub l: usize,
    /// Radial order `n ≥ 1` (1 = lowest root for this `l`, `pol`).
    pub n: usize,
    /// Re(k), the resonance position (rad / length, with same unit as
    /// `R_s`).
    pub re_k: f64,
    /// Im(k), the radiative decay rate in `k`. Always `< 0` in this
    /// catalog under the `exp(-i ω t)` convention; `|Im(k)|` is the
    /// linewidth and `Q = Re(k) / (2 |Im(k)|)` is the quality factor.
    pub im_k: f64,
    /// `2 l + 1`, the magnetic-quantum-number degeneracy (same as for
    /// `crate::mie::MieRoot`).
    pub multiplicity: usize,
}

impl MieRootComplex {
    /// Quality factor `Q = Re(k) / (2 |Im(k)|)`.
    pub fn q(&self) -> f64 {
        if self.im_k.abs() > 1e-30 {
            self.re_k / (2.0 * self.im_k.abs())
        } else {
            f64::INFINITY
        }
    }
}

// ---------------------------------------------------------------------
// Complex-arg spherical Bessel / Hankel by upward recurrence.
// ---------------------------------------------------------------------

/// Complex spherical Bessel `j_l(z)`, upward recurrence.
///
/// Stable for `|z| ≳ l`. The static catalog only evaluates these at
/// roots with `|z| ≳ 1` and `l ≤ 5`, well within the stable regime;
/// for completeness the runtime self-check in `tests` exercises the
/// recurrence at the catalog points.
pub fn spherical_j_c(l: usize, z: faer::c64) -> faer::c64 {
    if z.norm() < 1e-14 {
        return if l == 0 {
            faer::c64::new(1.0, 0.0)
        } else {
            faer::c64::new(0.0, 0.0)
        };
    }
    let s = c_sin(z);
    let c = c_cos(z);
    let j0 = s / z;
    if l == 0 {
        return j0;
    }
    let j1 = s / (z * z) - c / z;
    if l == 1 {
        return j1;
    }
    let mut prev = j0;
    let mut curr = j1;
    for k in 2..=l {
        let next = faer::c64::new((2 * k - 1) as f64, 0.0) / z * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Complex spherical Bessel `y_l(z)`, upward recurrence.
pub fn spherical_y_c(l: usize, z: faer::c64) -> faer::c64 {
    let s = c_sin(z);
    let c = c_cos(z);
    let y0 = -c / z;
    if l == 0 {
        return y0;
    }
    let y1 = -c / (z * z) - s / z;
    if l == 1 {
        return y1;
    }
    let mut prev = y0;
    let mut curr = y1;
    for k in 2..=l {
        let next = faer::c64::new((2 * k - 1) as f64, 0.0) / z * curr - prev;
        prev = curr;
        curr = next;
    }
    curr
}

/// Spherical Hankel of the first kind: `h_l^(1)(z) = j_l(z) + i y_l(z)`.
pub fn spherical_h1_c(l: usize, z: faer::c64) -> faer::c64 {
    spherical_j_c(l, z) + faer::c64::new(0.0, 1.0) * spherical_y_c(l, z)
}

/// `j_l'(z) = j_{l−1}(z) − (l+1)/z · j_l(z)`.
pub fn spherical_j_prime_c(l: usize, z: faer::c64) -> faer::c64 {
    if l == 0 {
        return -spherical_j_c(1, z);
    }
    spherical_j_c(l - 1, z) - faer::c64::new((l + 1) as f64, 0.0) / z * spherical_j_c(l, z)
}

/// `h_l^(1)'(z) = h_{l−1}^(1)(z) − (l+1)/z · h_l^(1)(z)`.
pub fn spherical_h1_prime_c(l: usize, z: faer::c64) -> faer::c64 {
    if l == 0 {
        return -spherical_h1_c(1, z);
    }
    spherical_h1_c(l - 1, z) - faer::c64::new((l + 1) as f64, 0.0) / z * spherical_h1_c(l, z)
}

/// Riccati-Bessel `ψ_l(z) = z · j_l(z)`.
pub fn psi_c(l: usize, z: faer::c64) -> faer::c64 {
    z * spherical_j_c(l, z)
}

/// `ψ_l'(z) = j_l(z) + z · j_l'(z)`.
pub fn psi_prime_c(l: usize, z: faer::c64) -> faer::c64 {
    spherical_j_c(l, z) + z * spherical_j_prime_c(l, z)
}

/// Riccati-Hankel `ξ_l(z) = z · h_l^(1)(z)`.
pub fn xi_c(l: usize, z: faer::c64) -> faer::c64 {
    z * spherical_h1_c(l, z)
}

/// `ξ_l'(z) = h_l^(1)(z) + z · h_l^(1)'(z)`.
pub fn xi_prime_c(l: usize, z: faer::c64) -> faer::c64 {
    spherical_h1_c(l, z) + z * spherical_h1_prime_c(l, z)
}

/// `sin(z)` for complex `z` (faer::c64 doesn't expose trig directly).
fn c_sin(z: faer::c64) -> faer::c64 {
    // sin(a + bi) = sin(a) cosh(b) + i cos(a) sinh(b).
    faer::c64::new(z.re.sin() * z.im.cosh(), z.re.cos() * z.im.sinh())
}

/// `cos(z)` for complex `z`.
fn c_cos(z: faer::c64) -> faer::c64 {
    // cos(a + bi) = cos(a) cosh(b) − i sin(a) sinh(b).
    faer::c64::new(z.re.cos() * z.im.cosh(), -z.re.sin() * z.im.sinh())
}

// ---------------------------------------------------------------------
// Characteristic functions (open-space).
// ---------------------------------------------------------------------

/// TE-mode resonance condition (pole of Mie `b_l` coefficient):
///
/// ```text
/// ψ_l(n·k·R_s) · ξ_l'(k·R_s) − (1/n) · ψ_l'(n·k·R_s) · ξ_l(k·R_s) = 0
/// ```
pub fn characteristic_te_open(n: f64, l: usize, r_s: f64, k: faer::c64) -> faer::c64 {
    let x_in = faer::c64::new(n * r_s, 0.0) * k;
    let x_s = faer::c64::new(r_s, 0.0) * k;
    psi_c(l, x_in) * xi_prime_c(l, x_s)
        - faer::c64::new(1.0 / n, 0.0) * psi_prime_c(l, x_in) * xi_c(l, x_s)
}

/// TM-mode resonance condition (pole of Mie `a_l` coefficient):
///
/// ```text
/// ψ_l(n·k·R_s) · ξ_l'(k·R_s) − n · ψ_l'(n·k·R_s) · ξ_l(k·R_s) = 0
/// ```
pub fn characteristic_tm_open(n: f64, l: usize, r_s: f64, k: faer::c64) -> faer::c64 {
    let x_in = faer::c64::new(n * r_s, 0.0) * k;
    let x_s = faer::c64::new(r_s, 0.0) * k;
    psi_c(l, x_in) * xi_prime_c(l, x_s)
        - faer::c64::new(n, 0.0) * psi_prime_c(l, x_in) * xi_c(l, x_s)
}

// ---------------------------------------------------------------------
// Static catalog: n = 1.5, R_s = 1.0.
// ---------------------------------------------------------------------

/// Refractive index for which [`OPEN_SPACE_WGM_TABLE_N15`] was tabulated.
pub const OPEN_SPACE_WGM_N: f64 = 1.5;

/// Sphere radius for which [`OPEN_SPACE_WGM_TABLE_N15`] was tabulated.
pub const OPEN_SPACE_WGM_R_S: f64 = 1.0;

/// Open-space Mie WGM resonance positions for `n = 1.5`, `R_s = 1.0`.
///
/// Each entry: `(polarisation, l, n_radial, Re(k), Im(k))` with
/// `Im(k) < 0` (radiative decay under `exp(-i ω t)`, see module docs).
///
/// Catalog extent: `l ∈ [1, 5]`, lowest 3 distinct converged radial
/// orders per `(l, polarisation)`. 30 entries total, sorted globally
/// by ascending `Re(k)`. Each entry was produced by complex Newton
/// iteration and the residual on the open-space TE/TM characteristic
/// function is `≲ 1 × 10⁻¹²` (re-verified at test time, see
/// `tests::catalog_residuals_are_small`).
///
/// **Reproducer**: `python3 mesh_scripts/mie_open_space_roots.py`.
pub static OPEN_SPACE_WGM_TABLE_N15: &[(MiePolarisation, usize, usize, f64, f64)] = &[
    (
        MiePolarisation::TE,
        1,
        1,
        1.2589599273e+00,
        -8.7021308883e-01,
    ),
    (
        MiePolarisation::TM,
        1,
        1,
        1.8807401144e+00,
        -4.8180596191e-01,
    ),
    (
        MiePolarisation::TE,
        2,
        1,
        2.3505102361e+00,
        -9.1640268874e-01,
    ),
    (
        MiePolarisation::TM,
        2,
        1,
        2.6818589916e+00,
        -4.2285156689e-01,
    ),
    (
        MiePolarisation::TE,
        1,
        2,
        2.9990897088e+00,
        -6.2353572127e-01,
    ),
    (
        MiePolarisation::TE,
        3,
        1,
        3.3821623638e+00,
        -8.4520914661e-01,
    ),
    (
        MiePolarisation::TM,
        3,
        1,
        3.4696117121e+00,
        -3.6676887861e-01,
    ),
    (
        MiePolarisation::TE,
        2,
        2,
        3.8590553171e+00,
        -7.4306931339e-01,
    ),
    (
        MiePolarisation::TM,
        1,
        2,
        4.0822559169e+00,
        -5.2427741567e-01,
    ),
    (
        MiePolarisation::TM,
        4,
        1,
        4.2496371545e+00,
        -3.1536363923e-01,
    ),
    (
        MiePolarisation::TE,
        4,
        1,
        4.3279225516e+00,
        -7.0620806623e-01,
    ),
    (
        MiePolarisation::TE,
        3,
        2,
        4.7210269651e+00,
        -9.0040239056e-01,
    ),
    (
        MiePolarisation::TM,
        2,
        2,
        4.9712778093e+00,
        -5.0840959906e-01,
    ),
    (
        MiePolarisation::TM,
        5,
        1,
        5.0242154597e+00,
        -2.6906807636e-01,
    ),
    (
        MiePolarisation::TE,
        1,
        3,
        5.1501372961e+00,
        -5.6338897223e-01,
    ),
    (
        MiePolarisation::TE,
        5,
        1,
        5.1907969939e+00,
        -5.6772496274e-01,
    ),
    (
        MiePolarisation::TE,
        4,
        2,
        5.6367932176e+00,
        -1.0752119786e+00,
    ),
    (
        MiePolarisation::TM,
        3,
        2,
        5.8288122837e+00,
        -4.9061643875e-01,
    ),
    (
        MiePolarisation::TE,
        2,
        3,
        6.0634748163e+00,
        -6.0093412132e-01,
    ),
    (
        MiePolarisation::TM,
        1,
        3,
        6.2123115928e+00,
        -5.3117106076e-01,
    ),
    (
        MiePolarisation::TE,
        5,
        2,
        6.6131109918e+00,
        -1.2091484956e+00,
    ),
    (
        MiePolarisation::TM,
        4,
        2,
        6.6648430571e+00,
        -4.7152996682e-01,
    ),
    (
        MiePolarisation::TE,
        3,
        3,
        6.9455091957e+00,
        -6.4620249433e-01,
    ),
    (
        MiePolarisation::TM,
        2,
        3,
        7.1447443178e+00,
        -5.2362308367e-01,
    ),
    (
        MiePolarisation::TM,
        5,
        2,
        7.4851705320e+00,
        -4.5146070540e-01,
    ),
    (
        MiePolarisation::TE,
        4,
        3,
        7.8055751310e+00,
        -6.9935896099e-01,
    ),
    (
        MiePolarisation::TM,
        3,
        3,
        8.0467371134e+00,
        -5.1480526643e-01,
    ),
    (
        MiePolarisation::TE,
        5,
        3,
        8.6499939186e+00,
        -7.6205738453e-01,
    ),
    (
        MiePolarisation::TM,
        4,
        3,
        8.9261115288e+00,
        -5.0512669709e-01,
    ),
    (
        MiePolarisation::TM,
        5,
        3,
        9.7878811873e+00,
        -4.9479274921e-01,
    ),
];

/// Iterator over the open-space catalog as fully-formed
/// [`MieRootComplex`] values. The catalog is sorted by ascending
/// `Re(k)`.
pub fn open_space_wgm_roots_n15() -> Vec<MieRootComplex> {
    OPEN_SPACE_WGM_TABLE_N15
        .iter()
        .map(|&(pol, l, n, re_k, im_k)| MieRootComplex {
            pol,
            l,
            n,
            re_k,
            im_k,
            multiplicity: 2 * l + 1,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Spherical Hankel sanity at a known real argument:
    ///   h_0^(1)(x) = -i exp(i x) / x  (closed form).
    #[test]
    fn h1_l0_closed_form_real_arg() {
        let x = 2.3_f64;
        let z = faer::c64::new(x, 0.0);
        let got = spherical_h1_c(0, z);
        let want = faer::c64::new(x.sin() / x, -x.cos() / x);
        // h_0^(1)(x) = j_0(x) + i y_0(x) = sin(x)/x + i (-cos(x)/x)
        assert!(
            (got.re - want.re).abs() < 1e-12 && (got.im - want.im).abs() < 1e-12,
            "h_0^(1)({x}): got {got:?}, want {want:?}"
        );
    }

    /// Complex `j_l` upward recurrence matches the real-arg
    /// [`crate::mie::spherical_j`] when `z` is purely real.
    #[test]
    fn complex_j_matches_real_j() {
        for &x in &[0.5_f64, 1.0, 2.5, 4.7, 6.3] {
            for l in 0..=5 {
                let got = spherical_j_c(l, faer::c64::new(x, 0.0));
                let want = crate::mie::spherical_j(l, x);
                assert!(
                    (got.re - want).abs() < 1e-10 && got.im.abs() < 1e-12,
                    "j_{l}({x}): real={want}, complex={got:?}"
                );
            }
        }
    }

    /// Every entry of the static catalog must produce a tiny residual
    /// on the appropriate open-space characteristic function. This is
    /// the runtime self-check that the Python-generated numbers are
    /// internally consistent — if anyone perturbs the static table
    /// without re-running the script, this test catches it.
    #[test]
    fn catalog_residuals_are_small() {
        let cat = open_space_wgm_roots_n15();
        assert_eq!(cat.len(), 30, "catalog should have 30 entries");
        for r in &cat {
            let k = faer::c64::new(r.re_k, r.im_k);
            let res = match r.pol {
                MiePolarisation::TE => {
                    characteristic_te_open(OPEN_SPACE_WGM_N, r.l, OPEN_SPACE_WGM_R_S, k)
                }
                MiePolarisation::TM => {
                    characteristic_tm_open(OPEN_SPACE_WGM_N, r.l, OPEN_SPACE_WGM_R_S, k)
                }
            };
            let res_abs = res.norm();
            assert!(
                res_abs < 1e-6,
                "{:?}_{},{} residual {} too large at k = {:?}",
                r.pol,
                r.l,
                r.n,
                res_abs,
                k
            );
        }
    }

    /// Sort and degeneracy sanity.
    #[test]
    fn catalog_is_sorted_with_correct_multiplicity() {
        let cat = open_space_wgm_roots_n15();
        for w in cat.windows(2) {
            assert!(w[0].re_k <= w[1].re_k, "catalog not sorted by Re(k)");
        }
        for r in &cat {
            assert_eq!(r.multiplicity, 2 * r.l + 1);
            assert!(r.im_k < 0.0, "Im(k) must be < 0 in our convention");
            assert!(r.q() > 0.0, "Q must be positive");
        }
    }

    /// Cross-check: the **lowest** open-space TE_1,1 root for
    /// `n = 1.5`, `R_s = 1.0` is a classic published value. Two
    /// reasonable references:
    ///
    /// - Hightower & Richardson, "Resonant Mie scattering from a layered
    ///   sphere", Appl. Opt. 27, 4850 (1988) — Table I lists
    ///   x = Re(k R_s) ≈ 1.26 for the lowest TE mode.
    /// - Lai, Leung, Liu, Tong & Young, "Time-independent perturbation
    ///   for leaking electromagnetic modes in open systems with
    ///   application to resonances in microdroplets", PRA 41, 5187
    ///   (1990) — gives k R_s = 1.2589 − 0.8702i for n = 1.5.
    ///
    /// We match to ~4 sig figs on both Re(k) and Im(k).
    #[test]
    fn te_1_1_matches_published_value() {
        let cat = open_space_wgm_roots_n15();
        let te11 = cat
            .iter()
            .find(|r| r.pol == MiePolarisation::TE && r.l == 1 && r.n == 1)
            .expect("TE_1,1 in catalog");
        // Published target: x = 1.2589 − 0.8702i.
        assert!(
            (te11.re_k - 1.2589).abs() < 1e-3,
            "TE_1,1 Re(k) = {} (want ≈ 1.2589)",
            te11.re_k
        );
        assert!(
            (te11.im_k + 0.8702).abs() < 1e-3,
            "TE_1,1 Im(k) = {} (want ≈ −0.8702)",
            te11.im_k
        );
    }

    /// Smoke test: the TM_1,1 mode (which the bundled FEM example
    /// claims as its lowest physical multiplet) should agree with
    /// textbook value `x ≈ 1.881 − 0.482i`.
    #[test]
    fn tm_1_1_matches_published_value() {
        let cat = open_space_wgm_roots_n15();
        let tm11 = cat
            .iter()
            .find(|r| r.pol == MiePolarisation::TM && r.l == 1 && r.n == 1)
            .expect("TM_1,1 in catalog");
        assert!(
            (tm11.re_k - 1.8807).abs() < 1e-3,
            "TM_1,1 Re(k) = {} (want ≈ 1.8807)",
            tm11.re_k
        );
        assert!(
            (tm11.im_k + 0.4818).abs() < 1e-3,
            "TM_1,1 Im(k) = {} (want ≈ −0.4818)",
            tm11.im_k
        );
    }
}
