//! Semi-analytic **thin dipole over a ground plane** infinite-array oracle
//! (issue #1513 AC2, Epic #1252 Phase 2). Test-only; pulled in with
//! `#[path = "common/dipole_array.rs"] mod dipole_array;`.
//!
//! One `x`-directed centre-fed dipole of half-length `L` per `dx × dy`
//! cell, at height `h` over a PEC ground plane, with the sinusoidal current
//! `I(x) = sin(k(L − |x|))/sin(kL)` (unit feed current) and the Floquet
//! phase of a beam scanned to `(θ, φ)`. The element is expanded in Floquet
//! harmonics `k_x = k_x0 + 2πm/dx`, `k_y = k_y0 + 2πn/dy`,
//! `k_z = √(k² − k_x² − k_y²)`; the sheet-current field with the ground
//! image `(1 − e^{−2 j k_z h})` gives the scan impedance
//!
//! ```text
//! Z = (η / (2 k A)) Σ_mn (k² − k_x²)/k_z · (1 − e^{−2 j k_z h}) |Ĩ(k_x)|²,
//! Ĩ(k_x) = ∫ I(x) e^{j k_x x} dx = 2k (cos k_x L − cos kL) / ((k² − k_x²) sin kL).
//! ```
//!
//! Only propagating harmonics are real: the scan **resistance** is
//! `R = (η/(2kA)) Σ_prop (k² − k_x²)/k_z · 2 sin²(k_z h) |Ĩ|²`, with `η = 1`
//! in natural units. (The wire radius enters only the reactance, so `R` is
//! the quantity the lumped-feed FEM is compared on.)
#![allow(dead_code)]

use std::f64::consts::PI;

/// Scan resistance of the dipole array at `(theta, phi)` (radians).
/// `k` is the free-space wavenumber (natural units, `η = 1`).
#[allow(clippy::too_many_arguments)]
pub fn scan_resistance(
    k: f64,
    dx: f64,
    dy: f64,
    half_len: f64,
    h: f64,
    theta: f64,
    phi: f64,
    n_orders: i32,
) -> f64 {
    let kx0 = k * theta.sin() * phi.cos();
    let ky0 = k * theta.sin() * phi.sin();
    let sk = (k * half_len).sin();
    let ck = (k * half_len).cos();
    let mut sum = 0.0;
    for m in -n_orders..=n_orders {
        let kx = kx0 + 2.0 * PI * f64::from(m) / dx;
        for n in -n_orders..=n_orders {
            let ky = ky0 + 2.0 * PI * f64::from(n) / dy;
            let kz2 = k * k - kx * kx - ky * ky;
            if kz2 <= 1e-12 {
                continue;
            }
            let kz = kz2.sqrt();
            let a = k * k - kx * kx;
            // (k² − kx²)|Ĩ|² with the removable kx → k limit handled.
            let num = if a.abs() < 1e-9 {
                // Ĩ → L² k/ (sin kL) · sin(kL)... use the limit
                // 2k·(L sin kL /(2k))·... evaluated numerically below.
                let e = 1e-5;
                let kxe = k - e;
                let ae = k * k - kxe * kxe;
                let it = 2.0 * k * ((kxe * half_len).cos() - ck) / (ae * sk);
                ae * it * it
            } else {
                let it = 2.0 * k * ((kx * half_len).cos() - ck) / (a * sk);
                a * it * it
            };
            sum += num / kz * 2.0 * (kz * h).sin().powi(2);
        }
    }
    sum / (2.0 * k * dx * dy)
}
