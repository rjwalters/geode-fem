//! Semi-analytic **parallel-plate-waveguide array** oracle (issue #1513,
//! Epic #1252 Phase 2). Test-only; pulled in with
//! `#[path = "common/ppw_array.rs"] mod ppw_array;`.
//!
//! An infinite array of parallel-plate waveguides (zero-thickness PEC
//! plates, period / guide width `d`, uniform along `y`) opens into free
//! space and is scanned in the **H-plane**: `E ∥ y` is parallel to the
//! plates, the guide mode is `TE₁₀` (`sin(πx/d)`, cutoff `d = λ/2`) and the
//! scan plane is `xz`. This is the modal-integral (mode-matching) solution
//! of the classical guide-array problem (Amitay–Galindo–Wu, *Theory and
//! Analysis of Phased Array Antennas*, ch. 4): the aperture field is
//! expanded in guide modes, the free-space field in Floquet harmonics
//! `e^{−j k_xn x}`, `k_xn = k sin θ + 2πn/d`, and `∂_z E_y` is projected on
//! each guide mode at the aperture plane.
//!
//! With `a_m` the aperture amplitude of guide mode `m` and
//! `I_nm = ∫₀^d sin(mπx/d) e^{j k_xn x} dx`:
//!
//! ```text
//! [ (d/2) β_p δ_pm + (1/d) Σ_n k_zn conj(I_np) I_nm ] a_m = d β₁ δ_p1,
//! Γ = a₁ − 1   (reference plane at the aperture).
//! ```
//!
//! Self-checks live in `floquet_active_reflection.rs`: power balance
//! (`β₁ d/2 (1 − |Γ|²) = d Σ_prop Re(k_zn)|c_n|²`), truncation
//! convergence, and the closed-form `Γ` limit of a vanishing guide
//! (`d → 0` is not used; the physical limit used is energy).
#![allow(dead_code)]

use faer::{Mat, c64};
use std::f64::consts::PI;

/// Result of one scan angle.
pub struct PpwResult {
    /// Active reflection of guide mode 1 at the aperture plane.
    pub gamma: c64,
    /// `Σ_prop Re(k_zn)|c_n|² d / (β₁ d/2)`: radiated fraction by
    /// Floquet-harmonic power (should equal `1 − |Γ|²`).
    pub radiated: f64,
    /// Number of propagating free-space harmonics.
    pub n_prop: usize,
}

fn outgoing(arg: f64) -> c64 {
    // √(arg) on the outgoing branch: real positive if propagating, −j α if
    // evanescent.
    if arg >= 0.0 {
        c64::new(arg.sqrt(), 0.0)
    } else {
        c64::new(0.0, -(-arg).sqrt())
    }
}

/// `I_nm = ∫₀^d sin(mπx/d) e^{jbx} dx`, `b = k_xn`.
fn integral(m: usize, b: f64, d: f64) -> c64 {
    let a = m as f64 * PI / d;
    if (a * a - b * b).abs() < 1e-9 * a * a {
        // b = ±a: ∫ sin² = d/2 for the imaginary part, the cos·sin part
        // integrates to zero over whole periods.
        return c64::new(0.0, d / 2.0 * b.signum());
    }
    let sign = if m.is_multiple_of(2) { 1.0 } else { -1.0 };
    let e = c64::new((b * d).cos(), (b * d).sin());
    (c64::new(1.0, 0.0) - e * sign) * (a / (a * a - b * b))
}

/// Solve at free-space wavenumber `k`, period `d`, scan angle `theta`
/// (radians), with `n_modes` guide modes and harmonics `|n| ≤ n_harm`.
pub fn solve(k: f64, d: f64, theta: f64, n_modes: usize, n_harm: i32) -> PpwResult {
    use faer::linalg::solvers::Solve;
    let kx0 = k * theta.sin();
    let ns: Vec<i32> = (-n_harm..=n_harm).collect();
    let kxn: Vec<f64> = ns
        .iter()
        .map(|&n| kx0 + 2.0 * PI * f64::from(n) / d)
        .collect();
    let kzn: Vec<c64> = kxn
        .iter()
        .map(|&kx| outgoing((k - kx) * (k + kx)))
        .collect();
    let beta: Vec<c64> = (1..=n_modes)
        .map(|m| {
            let kc = m as f64 * PI / d;
            outgoing((k - kc) * (k + kc))
        })
        .collect();
    // I[n][m]
    let i_nm: Vec<Vec<c64>> = kxn
        .iter()
        .map(|&b| (1..=n_modes).map(|m| integral(m, b, d)).collect())
        .collect();
    let mut a_mat = Mat::<c64>::zeros(n_modes, n_modes);
    for p in 0..n_modes {
        for m in 0..n_modes {
            let mut acc = c64::new(0.0, 0.0);
            for n in 0..ns.len() {
                acc += kzn[n] * i_nm[n][p].conj() * i_nm[n][m];
            }
            a_mat[(p, m)] = acc * (1.0 / d);
        }
        a_mat[(p, p)] += beta[p] * (d / 2.0);
    }
    let mut rhs = Mat::<c64>::zeros(n_modes, 1);
    rhs[(0, 0)] = beta[0] * d;
    let sol = a_mat.partial_piv_lu().solve(&rhs);
    let gamma = sol[(0, 0)] - c64::new(1.0, 0.0);
    let mut p_rad = 0.0;
    let mut n_prop = 0;
    for n in 0..ns.len() {
        let mut c = c64::new(0.0, 0.0);
        for m in 0..n_modes {
            c += sol[(m, 0)] * i_nm[n][m];
        }
        c *= 1.0 / d;
        if kzn[n].im == 0.0 {
            n_prop += 1;
            p_rad += kzn[n].re * c.norm_sqr() * d;
        }
    }
    PpwResult {
        gamma,
        radiated: p_rad / (beta[0].re * d / 2.0),
        n_prop,
    }
}
