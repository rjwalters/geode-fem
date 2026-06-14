//! Near-to-far-field (NTFF) transform via Love's surface-equivalence
//! theorem (Epic #226 Phase 3, issue #229).
//!
//! The project's first **far-field** capability. Phases 1/2 deliver the
//! driven near-field solution (`e_edges`) on the patch fixture plus a
//! scalar radiated-power integrator ([`crate::scattering::flux_power_box`]);
//! this module turns the tangential `(E, H)` on the closed Huygens box
//! into the angular far field `E(θ, φ)`, the directivity `D(θ, φ)`, and
//! (with the Phase-2 efficiency) the gain `G = D · η`.
//!
//! # Love surface equivalence
//!
//! On the closed Huygens box surface `S` (just inside the matched UPML,
//! enclosing the entire radiator) the tangential fields are replaced by
//! equivalent electric / magnetic surface currents
//!
//! ```text
//! J_s = n̂ × H,    M_s = −n̂ × E       (n̂ outward),
//! ```
//!
//! which radiate the same exterior field as the original source. The
//! far-field radiation vectors (Balanis, *Antenna Theory* 4e, 12-12 /
//! 12-27, natural units `c = μ₀ = ε₀ = 1`, `η₀ = 1`, `ω ≡ k`) are
//!
//! ```text
//! N(θ, φ) = ∮_S J_s · e^{ +j k r̂·r' } dS',
//! L(θ, φ) = ∮_S M_s · e^{ +j k r̂·r' } dS',
//! ```
//!
//! and the radiated far field (dropping the common
//! `j k e^{−jkr}/(4πr)` prefactor, which cancels in directivity) is
//!
//! ```text
//! E_θ ∝ −( L_φ + η₀ N_θ ),    E_φ ∝ +( L_θ − η₀ N_φ ).
//! ```
//!
//! # Convention
//!
//! The codebase uses `exp(+jωt)`: the incident plane wave
//! ([`crate::scattering::plane_wave_e_inc`]) is `x̂·e^{−jωz}`, so an
//! **outgoing** spherical wave carries `e^{−jkr}`. Consistency then
//! forces the radiation-integral phase to be `e^{+j k r̂·r'}` (the
//! retarded path-length difference `k(r − r̂·r')` enters the total
//! phase as `e^{−jk(r − r̂·r')}`). The `dipole_translation_invariance`
//! unit test pins this sign: shifting the source must leave
//! `|E(θ, φ)|` unchanged (only the common phase moves).
//!
//! `H = (i/ω) ∇×E` (`∇×E = −iωH`) is recovered per box face from the
//! piecewise-constant Whitney curl
//! ([`crate::scattering`] reuses the same evaluators), so no new field
//! machinery is introduced — only the Love integral.

use std::f64::consts::PI;

use faer::c64;

use crate::scattering::{box_surface_samples, BoxFaceSample};
use crate::TetMesh;

/// Free-space impedance in the codebase's natural units (`η₀ = 1`).
const ETA_0_NATURAL: f64 = 1.0;

/// A computed far field on a `(θ, φ)` grid.
///
/// `theta` has `n_theta` samples in `[0, π]`, `phi` has `n_phi` samples
/// in `[0, 2π)`. The complex spherical components `e_theta` / `e_phi`
/// are stored row-major as `idx = i_theta * n_phi + i_phi`.
#[derive(Debug, Clone)]
pub struct FarField {
    /// Polar angles `θ ∈ [0, π]` (radians).
    pub theta: Vec<f64>,
    /// Azimuth angles `φ ∈ [0, 2π)` (radians).
    pub phi: Vec<f64>,
    /// `E_θ(θ, φ)`, row-major `i_theta * n_phi + i_phi`.
    pub e_theta: Vec<c64>,
    /// `E_φ(θ, φ)`, row-major `i_theta * n_phi + i_phi`.
    pub e_phi: Vec<c64>,
}

impl FarField {
    /// Number of polar samples.
    pub fn n_theta(&self) -> usize {
        self.theta.len()
    }

    /// Number of azimuth samples.
    pub fn n_phi(&self) -> usize {
        self.phi.len()
    }

    /// `|E(θ, φ)|²` at a grid index `(i_theta, i_phi)`.
    pub fn power_density(&self, i_theta: usize, i_phi: usize) -> f64 {
        let idx = i_theta * self.n_phi() + i_phi;
        self.e_theta[idx].norm_sqr() + self.e_phi[idx].norm_sqr()
    }
}

/// Spherical-current contribution sample: an equivalent-current face
/// plus its geometry, ready to be summed for each observation
/// direction. (Internal — built once from [`BoxFaceSample`]s.)
struct SurfaceCurrent {
    /// `J_s = n̂ × H`.
    j_s: [c64; 3],
    /// `M_s = −n̂ × E`.
    m_s: [c64; 3],
    /// Face centroid `r'`.
    r: [f64; 3],
    /// Face area (the quadrature weight).
    area: f64,
}

/// Complex cross product `a × b` for `c64` vectors.
fn cross_c(a: [c64; 3], b: [c64; 3]) -> [c64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `(real n̂) × (complex v)`.
fn cross_rc(n: [f64; 3], v: [c64; 3]) -> [c64; 3] {
    let nc = [
        c64::new(n[0], 0.0),
        c64::new(n[1], 0.0),
        c64::new(n[2], 0.0),
    ];
    cross_c(nc, v)
}

/// Build the Love equivalent surface currents from box face samples.
fn surface_currents(samples: &[BoxFaceSample]) -> Vec<SurfaceCurrent> {
    samples
        .iter()
        .map(|s| {
            let j_s = cross_rc(s.n_hat, s.h); // n̂ × H
            let nxe = cross_rc(s.n_hat, s.e); // n̂ × E
            let m_s = [-nxe[0], -nxe[1], -nxe[2]]; // −n̂ × E
            SurfaceCurrent {
                j_s,
                m_s,
                r: s.centroid,
                area: s.area,
            }
        })
        .collect()
}

/// Unit observation direction and its `(θ̂, φ̂)` basis for a `(θ, φ)`.
fn sph_basis(theta: f64, phi: f64) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let (st, ct) = theta.sin_cos();
    let (sp, cp) = phi.sin_cos();
    let r_hat = [st * cp, st * sp, ct];
    let theta_hat = [ct * cp, ct * sp, -st];
    let phi_hat = [-sp, cp, 0.0];
    (r_hat, theta_hat, phi_hat)
}

/// Project a complex vector onto a real unit direction.
fn project(v: [c64; 3], u: [f64; 3]) -> c64 {
    v[0] * c64::new(u[0], 0.0) + v[1] * c64::new(u[1], 0.0) + v[2] * c64::new(u[2], 0.0)
}

/// Compute the far field `E(θ, φ)` from the equivalent currents on the
/// closed Huygens box (Love surface equivalence + radiation integral).
///
/// Core entry point used by both the dipole linchpin test (which feeds
/// synthetic analytic samples) and the patch extraction (which feeds
/// the driven-solution box samples).
///
/// - `currents` are built from the box face `(n̂, E, H, area, r')`.
/// - `k = ω` (natural units).
/// - grid: `n_theta` samples on `[0, π]` (inclusive), `n_phi` on
///   `[0, 2π)`.
fn far_field_from_currents(
    samples: &[BoxFaceSample],
    k: f64,
    n_theta: usize,
    n_phi: usize,
) -> FarField {
    assert!(n_theta >= 2, "need at least 2 polar samples");
    assert!(n_phi >= 1, "need at least 1 azimuth sample");
    let currents = surface_currents(samples);

    let theta: Vec<f64> = (0..n_theta)
        .map(|i| PI * i as f64 / (n_theta - 1) as f64)
        .collect();
    let phi: Vec<f64> = (0..n_phi)
        .map(|j| 2.0 * PI * j as f64 / n_phi as f64)
        .collect();

    let mut e_theta = vec![c64::new(0.0, 0.0); n_theta * n_phi];
    let mut e_phi = vec![c64::new(0.0, 0.0); n_theta * n_phi];

    for (it, &th) in theta.iter().enumerate() {
        for (ip, &ph) in phi.iter().enumerate() {
            let (r_hat, theta_hat, phi_hat) = sph_basis(th, ph);

            // Radiation vectors N = ∮ J_s e^{+jk r̂·r'} dS',
            //                   L = ∮ M_s e^{+jk r̂·r'} dS'.
            let mut n_vec = [c64::new(0.0, 0.0); 3];
            let mut l_vec = [c64::new(0.0, 0.0); 3];
            for c in &currents {
                let kr = k * (r_hat[0] * c.r[0] + r_hat[1] * c.r[1] + r_hat[2] * c.r[2]);
                let phase = c64::new(kr.cos(), kr.sin()); // e^{+j k r̂·r'}
                let w = phase * c64::new(c.area, 0.0);
                for m in 0..3 {
                    n_vec[m] += c.j_s[m] * w;
                    l_vec[m] += c.m_s[m] * w;
                }
            }

            let n_theta_c = project(n_vec, theta_hat);
            let n_phi_c = project(n_vec, phi_hat);
            let l_theta_c = project(l_vec, theta_hat);
            let l_phi_c = project(l_vec, phi_hat);

            let eta = c64::new(ETA_0_NATURAL, 0.0);
            // E_θ ∝ −(L_φ + η N_θ), E_φ ∝ +(L_θ − η N_φ).
            let idx = it * n_phi + ip;
            e_theta[idx] = -(l_phi_c + eta * n_theta_c);
            e_phi[idx] = l_theta_c - eta * n_phi_c;
        }
    }

    FarField {
        theta,
        phi,
        e_theta,
        e_phi,
    }
}

/// Near-to-far-field transform of a driven near-field solution.
///
/// Samples the tangential `(E, H)` on the closed Huygens box
/// `[box_lo, box_hi]` (the same surface as
/// [`crate::scattering::flux_power_box`]) from the per-edge complex
/// field `e_edges`, applies Love equivalence, and integrates the
/// radiation vectors over a `(θ, φ)` grid.
///
/// `omega ≡ k` in natural units (`c = 1`); pass the same `omega` the
/// driven solve used.
///
/// # Panics
///
/// Same as [`crate::scattering::flux_power_box`] (empty / all-inside
/// box, `e_edges` length mismatch) and on degenerate grids
/// (`n_theta < 2` or `n_phi < 1`).
pub fn ntff_far_field(
    mesh: &TetMesh,
    omega: f64,
    e_edges: &[c64],
    box_lo: [f64; 3],
    box_hi: [f64; 3],
    n_theta: usize,
    n_phi: usize,
) -> FarField {
    let samples = box_surface_samples(mesh, omega, e_edges, box_lo, box_hi);
    far_field_from_currents(&samples, omega, n_theta, n_phi)
}

/// Far-field directivity from the radiated angular power.
///
/// `D(θ, φ) = 4π U(θ, φ) / ∮ U dΩ` with radiation intensity
/// `U = |E_θ|² + |E_φ|²` (the common far-field prefactor cancels). The
/// denominator is the trapezoidal integral over the `(θ, φ)` grid with
/// the `sinθ` solid-angle weight; the azimuth is periodic (rectangle
/// rule), the polar uses the trapezoid rule including the poles.
///
/// Returns `(d_max, d_grid)` where `d_grid` is row-major over the same
/// grid as the [`FarField`].
pub fn directivity(ff: &FarField) -> (f64, Vec<f64>) {
    let n_theta = ff.n_theta();
    let n_phi = ff.n_phi();

    // ∮ U dΩ = ∫₀^{2π} ∫₀^π U sinθ dθ dφ.
    // φ: uniform on [0, 2π), periodic → rectangle rule, weight Δφ.
    let d_phi = 2.0 * PI / n_phi as f64;
    let mut integral = 0.0_f64;
    for it in 0..n_theta {
        // θ: composite trapezoid weight. Endpoints get half a step;
        // interior nodes get the central span (θ_{i+1} − θ_{i−1})/2.
        let w_theta = if n_theta == 1 {
            PI
        } else if it == 0 {
            0.5 * (ff.theta[1] - ff.theta[0])
        } else if it == n_theta - 1 {
            0.5 * (ff.theta[n_theta - 1] - ff.theta[n_theta - 2])
        } else {
            0.5 * (ff.theta[it + 1] - ff.theta[it - 1])
        };
        let st = ff.theta[it].sin();
        for ip in 0..n_phi {
            let u = ff.power_density(it, ip);
            integral += u * st * w_theta * d_phi;
        }
    }

    let mut d_grid = vec![0.0_f64; n_theta * n_phi];
    let mut d_max = 0.0_f64;
    if integral > 0.0 {
        let scale = 4.0 * PI / integral;
        for it in 0..n_theta {
            for ip in 0..n_phi {
                let d = ff.power_density(it, ip) * scale;
                d_grid[it * n_phi + ip] = d;
                if d > d_max {
                    d_max = d;
                }
            }
        }
    }
    (d_max, d_grid)
}

/// Broadside (+z, `θ = 0`) directivity, averaged over the azimuth row
/// nearest the pole (the patch's main-lobe direction).
pub fn broadside_directivity(ff: &FarField) -> f64 {
    let (_d_max, d_grid) = directivity(ff);
    let n_phi = ff.n_phi();
    // θ = 0 is the first row.
    let row = &d_grid[0..n_phi];
    row.iter().sum::<f64>() / n_phi as f64
}

/// Gain `G = D · η` from a directivity and a radiation efficiency.
///
/// `eta` is clamped to `(0, 1]` (a passive radiator); a non-physical
/// efficiency outside that range is reported by the clamp rather than
/// producing a non-physical gain.
pub fn gain(directivity: f64, eta: f64) -> f64 {
    directivity * eta.clamp(0.0, 1.0)
}

/// Convert a linear (power) ratio to decibels: `10·log₁₀(x)`.
pub fn to_db(x: f64) -> f64 {
    10.0 * x.max(1e-300).log10()
}

/// A principal-plane pattern cut: `|E|` (normalized to its own max) vs
/// the polar angle `θ`, over the full `[0, π]` sweep at a fixed `φ`.
#[derive(Debug, Clone)]
pub struct PatternCut {
    /// Polar angles `θ ∈ [0, π]` (radians).
    pub theta: Vec<f64>,
    /// `|E(θ)|` normalized so its maximum over the cut is 1.
    pub e_norm: Vec<f64>,
}

/// Extract an E-plane (`φ = 0`, x-z plane) and an H-plane
/// (`φ = π/2`, y-z plane) principal-plane cut from a far field.
///
/// The cut at a requested `φ` snaps to the nearest azimuth column on
/// the grid. Returns `(e_plane, h_plane)`.
pub fn principal_plane_cuts(ff: &FarField) -> (PatternCut, PatternCut) {
    let e_plane = pattern_cut_at_phi(ff, 0.0);
    let h_plane = pattern_cut_at_phi(ff, PI / 2.0);
    (e_plane, h_plane)
}

/// Build a [`PatternCut`] at the grid azimuth column nearest `phi`.
fn pattern_cut_at_phi(ff: &FarField, phi: f64) -> PatternCut {
    let n_phi = ff.n_phi();
    // Nearest azimuth column (φ is periodic).
    let ip = (0..n_phi)
        .min_by(|&a, &b| {
            let da = ang_dist(ff.phi[a], phi);
            let db = ang_dist(ff.phi[b], phi);
            da.partial_cmp(&db).unwrap()
        })
        .unwrap();

    let mut mag: Vec<f64> = (0..ff.n_theta())
        .map(|it| ff.power_density(it, ip).sqrt())
        .collect();
    let max = mag.iter().cloned().fold(0.0_f64, f64::max);
    if max > 0.0 {
        for m in mag.iter_mut() {
            *m /= max;
        }
    }
    PatternCut {
        theta: ff.theta.clone(),
        e_norm: mag,
    }
}

/// Smallest angular distance between two azimuths on the circle.
fn ang_dist(a: f64, b: f64) -> f64 {
    let mut d = (a - b).rem_euclid(2.0 * PI);
    if d > PI {
        d = 2.0 * PI - d;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use crate::scattering::{box_surface_samples, cross};

    /// Analytic far-field components for a short z-directed dipole,
    /// built directly as box face samples (bypasses the FEM), so the
    /// NTFF integral is tested on a closed-form source.
    ///
    /// Exact Hertzian-dipole fields (Balanis 4-8a..c, natural units
    /// η₀ = 1, k = ω, dipole moment `I₀l = 1` along ẑ at the origin):
    ///
    /// ```text
    /// E_r  = (cosθ/(2π)) (2/r² + 2/(jk r³)) e^{−jkr},
    /// E_θ  = (sinθ/(4π)) (jk/r + 1/r² + 1/(jk r³)) e^{−jkr},
    /// H_φ  = (sinθ/(4π)) (jk/r + 1/r²) e^{−jkr},
    /// ```
    /// other components zero.
    fn dipole_fields(p: [f64; 3], k: f64) -> ([c64; 3], [c64; 3]) {
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        let ct = p[2] / r; // cosθ
        let rho = (p[0] * p[0] + p[1] * p[1]).sqrt();
        let st = rho / r; // sinθ
                          // Azimuth basis.
        let (cp, sp) = if rho > 1e-14 {
            (p[0] / rho, p[1] / rho)
        } else {
            (1.0, 0.0)
        };
        let r_hat = [p[0] / r, p[1] / r, p[2] / r];
        let theta_hat = [ct * cp, ct * sp, -st];
        let phi_hat = [-sp, cp, 0.0];

        let jk = c64::new(0.0, k);
        let e_neg_jkr = {
            let phase = -k * r;
            c64::new(phase.cos(), phase.sin())
        };
        let inv_r = c64::new(1.0 / r, 0.0);
        let inv_r2 = c64::new(1.0 / (r * r), 0.0);
        let inv_r3 = c64::new(1.0 / (r * r * r), 0.0);
        let one_over_jk = c64::new(1.0, 0.0) / jk;

        // E_r, E_θ, H_φ.
        let e_r = c64::new(ct / (2.0 * PI), 0.0)
            * (c64::new(2.0, 0.0) * inv_r2 + c64::new(2.0, 0.0) * one_over_jk * inv_r3)
            * e_neg_jkr;
        let e_th = c64::new(st / (4.0 * PI), 0.0)
            * (jk * inv_r + inv_r2 + one_over_jk * inv_r3)
            * e_neg_jkr;
        let h_ph = c64::new(st / (4.0 * PI), 0.0) * (jk * inv_r + inv_r2) * e_neg_jkr;

        let e = [
            e_r * c64::new(r_hat[0], 0.0) + e_th * c64::new(theta_hat[0], 0.0),
            e_r * c64::new(r_hat[1], 0.0) + e_th * c64::new(theta_hat[1], 0.0),
            e_r * c64::new(r_hat[2], 0.0) + e_th * c64::new(theta_hat[2], 0.0),
        ];
        let h = [
            h_ph * c64::new(phi_hat[0], 0.0),
            h_ph * c64::new(phi_hat[1], 0.0),
            h_ph * c64::new(phi_hat[2], 0.0),
        ];
        (e, h)
    }

    /// Build closed Huygens-box face samples for a synthetic source by
    /// reusing the FEM box-surface walk on a cube mesh, then overwriting
    /// the (E, H) at each face centroid with analytic values.
    ///
    /// `shift` translates the dipole away from the box center (used by
    /// the translation-invariance test). The cube spans `[0, side]³`;
    /// the analytic dipole sits at the cube center plus `shift`.
    fn dipole_box_samples(n: usize, side: f64, k: f64, shift: [f64; 3]) -> Vec<BoxFaceSample> {
        let mesh = cube_tet_mesh(n, side);
        // A box that selects all-but-the-outer-layer tets so the surface
        // is the cube's interior boundary. Shrink half a cell inward.
        let h = side / n as f64;
        let lo = [h * 0.5, h * 0.5, h * 0.5];
        let hi = [side - h * 0.5, side - h * 0.5, side - h * 0.5];
        let n_edges = mesh.edges().len();
        let e_edges = vec![c64::new(0.0, 0.0); n_edges];
        let mut samples = box_surface_samples(&mesh, k, &e_edges, lo, hi);
        let center = [
            0.5 * side + shift[0],
            0.5 * side + shift[1],
            0.5 * side + shift[2],
        ];
        for s in samples.iter_mut() {
            let rp = [
                s.centroid[0] - center[0],
                s.centroid[1] - center[1],
                s.centroid[2] - center[2],
            ];
            let (e, hf) = dipole_fields(rp, k);
            s.e = e;
            s.h = hf;
        }
        samples
    }

    /// THE LINCHPIN: the NTFF of an analytic short z-dipole must recover
    /// the `E_θ ∝ sinθ` pattern, `E_φ ≈ 0`, and directivity `D = 1.5`.
    #[test]
    fn dipole_recovers_sin_theta_and_directivity_1p5() {
        // Box ~2 wavelengths across so the source sits comfortably
        // inside and the surface is in the radiating/intermediate zone.
        let k = 1.0;
        let lambda = 2.0 * PI / k;
        let side = 2.0 * lambda;
        let n = 16;
        let samples = dipole_box_samples(n, side, k, [0.0, 0.0, 0.0]);

        let ff = far_field_from_currents(&samples, k, 37, 24);
        let (d_max, _d_grid) = directivity(&ff);

        // Directivity of a short dipole is exactly 1.5.
        eprintln!("dipole NTFF D_max = {d_max:.4} (analytic 1.5)");
        assert!(
            (d_max - 1.5).abs() < 0.15,
            "short-dipole directivity D = {d_max:.4}, expected 1.5 (±0.15)"
        );

        // Pattern shape: |E_θ| ∝ sinθ in the φ=0 plane; |E_φ| ≈ 0.
        let n_phi = ff.n_phi();
        // Index of the grid θ nearest a target angle.
        let nearest_theta = |target: f64| -> usize {
            (0..ff.n_theta())
                .min_by(|&a, &b| {
                    (ff.theta[a] - target)
                        .abs()
                        .partial_cmp(&(ff.theta[b] - target).abs())
                        .unwrap()
                })
                .unwrap()
        };
        // Equator (θ ≈ π/2) vs an oblique angle (θ ≈ π/6) at φ=0.
        let it_eq = nearest_theta(PI / 2.0);
        let it_obl = nearest_theta(PI / 6.0);
        let mag = |it: usize| ff.power_density(it, 0).sqrt();
        let ratio = mag(it_obl) / mag(it_eq);
        let expected = (ff.theta[it_obl]).sin() / (ff.theta[it_eq]).sin();
        eprintln!(
            "pattern ratio |E|(θ={:.3})/|E|(θ={:.3}) = {ratio:.4}, sinθ predicts {expected:.4}",
            ff.theta[it_obl], ff.theta[it_eq]
        );
        assert!(
            (ratio - expected).abs() < 0.1,
            "pattern is not sinθ: ratio {ratio:.4} vs {expected:.4}"
        );

        // |E_φ| should be negligible vs |E_θ| at the equator.
        let idx_eq = it_eq * n_phi;
        let e_th = ff.e_theta[idx_eq].norm();
        let e_ph = ff.e_phi[idx_eq].norm();
        assert!(
            e_ph < 0.1 * e_th.max(1e-30),
            "E_φ = {e_ph:.3e} not negligible vs E_θ = {e_th:.3e}"
        );
    }

    /// Phase-sign / convention gate: translating the dipole inside the
    /// box leaves the *magnitude* pattern |E(θ,φ)| invariant (only the
    /// common phase moves). A wrong-sign `e^{±jk r̂·r'}` breaks this.
    #[test]
    fn dipole_translation_invariance() {
        let k = 1.0;
        let lambda = 2.0 * PI / k;
        let side = 2.0 * lambda;
        let n = 24;
        // A modest shift keeps the source well away from every face so
        // the residual is dominated by the (sign-independent) common
        // phase factor, not by reactive near-field sampling on a face
        // the source has crept up on.
        let shift = [0.06 * lambda, -0.05 * lambda, 0.07 * lambda];

        let s0 = dipole_box_samples(n, side, k, [0.0, 0.0, 0.0]);
        let s1 = dipole_box_samples(n, side, k, shift);
        let ff0 = far_field_from_currents(&s0, k, 19, 12);
        let ff1 = far_field_from_currents(&s1, k, 19, 12);

        // Normalize the per-direction discrepancy by the *peak* |E| so a
        // pattern null (sinθ → 0 at the poles) does not blow up the
        // relative metric: a wrong-sign e^{±jk r̂·r'} produces an O(1)
        // distortion of the whole lobe, far above this floor.
        let peak0 = (0..ff0.n_theta())
            .flat_map(|it| (0..ff0.n_phi()).map(move |ip| (it, ip)))
            .map(|(it, ip)| ff0.power_density(it, ip).sqrt())
            .fold(0.0_f64, f64::max);
        assert!(peak0 > 0.0);

        let mut max_rel = 0.0_f64;
        for it in 0..ff0.n_theta() {
            for ip in 0..ff0.n_phi() {
                let m0 = ff0.power_density(it, ip).sqrt();
                let m1 = ff1.power_density(it, ip).sqrt();
                let rel = (m0 - m1).abs() / peak0;
                if rel > max_rel {
                    max_rel = rel;
                }
            }
        }
        eprintln!("max |E| change under translation (peak-normalized) = {max_rel:.3e}");
        // The correct sign converges toward 0 with mesh refinement
        // (observed 0.15 at n=16, 0.067 at n=24); the *wrong* sign gives
        // an O(1) ≈ 0.68 distortion — an order of magnitude larger and
        // refinement-insensitive. A 0.10 band cleanly separates the two
        // while staying inside the default-profile time budget.
        assert!(
            max_rel < 0.10,
            "translation changed |E| by {max_rel:.3e} of peak; the phase \
             sign e^{{±jk r̂·r'}} is likely wrong (a wrong sign gives ≈0.68)"
        );
    }

    /// Directivity quadrature sanity: a synthetic isotropic far field
    /// (|E| constant over the sphere) integrates to D = 1.
    #[test]
    fn isotropic_field_directivity_is_one() {
        let n_theta = 33;
        let n_phi = 24;
        let theta: Vec<f64> = (0..n_theta)
            .map(|i| PI * i as f64 / (n_theta - 1) as f64)
            .collect();
        let phi: Vec<f64> = (0..n_phi)
            .map(|j| 2.0 * PI * j as f64 / n_phi as f64)
            .collect();
        let one = c64::new(1.0, 0.0);
        let ff = FarField {
            theta,
            phi,
            e_theta: vec![one; n_theta * n_phi],
            e_phi: vec![c64::new(0.0, 0.0); n_theta * n_phi],
        };
        let (d_max, _) = directivity(&ff);
        eprintln!("isotropic D_max = {d_max:.4} (expected 1.0)");
        assert!(
            (d_max - 1.0).abs() < 0.02,
            "isotropic directivity should be 1.0, got {d_max:.4}"
        );
    }

    /// The NTFF consumes the same box faces as `flux_power_box`: the
    /// shared `box_surface_samples` walk yields one sample per surface
    /// face with unit outward normals and positive areas.
    #[test]
    fn surface_samples_well_formed() {
        let mesh = cube_tet_mesh(6, 1.0);
        let n_edges = mesh.edges().len();
        let e_edges = vec![c64::new(1.0, 0.0); n_edges];
        let samples = box_surface_samples(&mesh, 1.0, &e_edges, [0.2, 0.2, 0.2], [0.8, 0.8, 0.8]);
        assert!(!samples.is_empty(), "box must have a surface");
        for s in &samples {
            let nlen =
                (s.n_hat[0] * s.n_hat[0] + s.n_hat[1] * s.n_hat[1] + s.n_hat[2] * s.n_hat[2])
                    .sqrt();
            assert!((nlen - 1.0).abs() < 1e-9, "normal not unit: {nlen}");
            assert!(s.area > 0.0, "face area must be positive");
        }
    }

    /// `gain` clamps a non-physical efficiency into `[0, 1]`.
    #[test]
    fn gain_clamps_efficiency() {
        assert_eq!(gain(2.0, 0.5), 1.0);
        assert_eq!(gain(2.0, 1.5), 2.0); // η clamped to 1
        assert_eq!(gain(2.0, -0.3), 0.0); // η clamped to 0
    }

    /// Shared `cross` helper agrees with the right-hand rule.
    #[test]
    fn cross_right_hand() {
        let c = cross([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
        assert_eq!(c, [0.0, 0.0, 1.0]);
    }
}
