//! Exact closed-form air-gap field of a **slotless surface-PM annulus**
//! (Epic #448, Phase 2 — the primary field-accuracy oracle).
//!
//! A radially-magnetized surface-PM band `R1 ≤ r ≤ R2` carrying the
//! sinusoidal radial magnetization
//!
//! ```text
//!   M = M_r r̂ ,   M_r(θ) = M0 cos(p θ)          (p pole-pairs, p ≥ 2)
//! ```
//!
//! in an **open, homogeneous** domain (recoil permeability `μ_rec = 1`, no
//! iron), produces a pure multipole air-gap field. This is the classic
//! Zhu & Howe (1993) magnetic-field-analysis result, obtained here by
//! matching a magnetic scalar potential `φ_m` (with `∇²φ_m = ∇·M` in the
//! magnet, `0` outside, and `M·n̂` surface-charge jumps) across the three
//! coaxial regions `r < R1`, `R1 < r < R2`, `r > R2`.
//!
//! # Exterior field (`r ≥ R2`, the air gap and beyond)
//!
//! ```text
//!   B_r(r, θ) = C · r^{-(p+1)} · cos(p θ)
//!   B_θ(r, θ) = C · r^{-(p+1)} · sin(p θ)
//!   C = μ₀ M0 p (R2^{p+1} − R1^{p+1}) / [2 (p + 1)]
//! ```
//!
//! Note `|B| = |C| r^{-(p+1)}` is **θ-independent** on any coaxial ring —
//! the hallmark of a single-harmonic radial-magnetization multipole.
//!
//! # Self-validation (gate the oracle before the solver)
//!
//! In the **thin-magnet limit** `t = R2 − R1 → 0` about a mean radius
//! `R = (R1 + R2)/2`, the annulus reduces to a `θ`-dependent bound current
//! sheet. Expanding `C`,
//!
//! ```text
//!   C → μ₀ M0 p t R^p / 2 = μ₀ K0 R^{p+1} / 2 ,   K0 = M0 p t / R
//! ```
//!
//! which is **exactly** the exterior multipole coefficient of a
//! z-directed current sheet `K_z = K0 cos(p θ)` at radius `R`, obtained
//! from the 2-D free-space Green's function (Biot–Savart), independently
//! of the scalar-potential matching. [`current_sheet_exterior_coeff`]
//! encodes that Biot–Savart result and
//! [`self_validation_rel_error`] measures the agreement — the ≤ 0.5 %
//! gate of the test plan (#448 risk note "cross-check against the coaxial
//! current-sheet result in a limiting case").

use std::f64::consts::PI;

/// Vacuum permeability `μ₀` (SI, T·m/A).
pub const MU_0: f64 = 4.0e-7 * PI;

/// Parameters of a radially-magnetized slotless surface-PM annulus.
///
/// The magnetization is `M = M0 cos(p θ) r̂` over the band `R1 ≤ r ≤ R2`,
/// with recoil permeability `μ_rec = 1` (magnet magnetically indistinct
/// from air) and an open exterior — the regime in which the exterior field
/// is the single closed-form multipole [`SlotlessPm::exterior_field`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotlessPm {
    /// Magnet inner radius `R1` (m), `0 < R1 < R2`.
    pub r_inner: f64,
    /// Magnet outer radius `R2` (m).
    pub r_outer: f64,
    /// Radial-magnetization amplitude `M0` (A/m): `M_r(θ) = M0 cos(p θ)`.
    pub m0: f64,
    /// Pole-pair count `p` (≥ 2; `p = 1` is excluded — its scalar-potential
    /// particular solution is the degenerate `r ln r` branch, not the
    /// `r/(1−p²)` branch this closed form uses).
    pub pole_pairs: u32,
}

impl SlotlessPm {
    /// Construct a validated slotless-PM annulus.
    ///
    /// # Panics
    ///
    /// Panics unless `0 < r_inner < r_outer` (both finite), `m0` is finite,
    /// and `pole_pairs ≥ 2`.
    pub fn new(r_inner: f64, r_outer: f64, m0: f64, pole_pairs: u32) -> Self {
        assert!(
            r_inner.is_finite() && r_outer.is_finite() && 0.0 < r_inner && r_inner < r_outer,
            "SlotlessPm requires 0 < r_inner ({r_inner}) < r_outer ({r_outer}), both finite"
        );
        assert!(m0.is_finite(), "SlotlessPm requires finite m0 (got {m0})");
        assert!(
            pole_pairs >= 2,
            "SlotlessPm requires pole_pairs ≥ 2 (got {pole_pairs}); p = 1 needs the \
             degenerate r·ln r particular solution this closed form does not implement"
        );
        Self {
            r_inner,
            r_outer,
            m0,
            pole_pairs,
        }
    }

    /// Exterior multipole coefficient
    /// `C = μ₀ M0 p (R2^{p+1} − R1^{p+1}) / [2 (p + 1)]`.
    ///
    /// `B_r = C r^{-(p+1)} cos(p θ)`, `B_θ = C r^{-(p+1)} sin(p θ)` for
    /// `r ≥ R2`.
    pub fn exterior_coeff(&self) -> f64 {
        let p = self.pole_pairs as f64;
        MU_0 * self.m0 * p * (self.r_outer.powf(p + 1.0) - self.r_inner.powf(p + 1.0))
            / (2.0 * (p + 1.0))
    }

    /// Exact exterior air-gap flux density `(B_r, B_θ)` at polar
    /// `(r, θ)` for `r ≥ R2`.
    ///
    /// # Panics
    ///
    /// Panics if `r < r_outer` (the closed form here is the **exterior**
    /// solution; the in-magnet and inner-bore branches are not exposed
    /// because the air-gap benchmark only samples `r > R2`).
    pub fn exterior_field(&self, r: f64, theta: f64) -> (f64, f64) {
        assert!(
            r >= self.r_outer,
            "exterior_field is valid only for r ≥ r_outer ({}), got r = {r}",
            self.r_outer
        );
        let p = self.pole_pairs as f64;
        let radial = self.exterior_coeff() * r.powf(-(p + 1.0));
        let ang = p * theta;
        (radial * ang.cos(), radial * ang.sin())
    }

    /// Exact exterior flux density in **Cartesian** components
    /// `(B_x, B_y)` at `(x, y)`, for `√(x²+y²) ≥ R2`. Convenience wrapper
    /// around [`exterior_field`](Self::exterior_field) for comparison
    /// against the FEM solver's Cartesian `B`.
    pub fn exterior_field_xy(&self, x: f64, y: f64) -> (f64, f64) {
        let r = (x * x + y * y).sqrt();
        let theta = y.atan2(x);
        let (b_r, b_th) = self.exterior_field(r, theta);
        let (c, s) = (theta.cos(), theta.sin());
        // (B_x, B_y) = B_r r̂ + B_θ θ̂, r̂ = (c, s), θ̂ = (−s, c).
        (b_r * c - b_th * s, b_r * s + b_th * c)
    }

    /// Mean magnet radius `R = (R1 + R2)/2` and thickness `t = R2 − R1`.
    pub fn mean_radius_thickness(&self) -> (f64, f64) {
        (
            0.5 * (self.r_inner + self.r_outer),
            self.r_outer - self.r_inner,
        )
    }

    /// Effective thin-band current-sheet amplitude `K0 = M0 p t / R`
    /// (A/m) — the z-directed sheet `K_z = K0 cos(p θ)` at radius `R` this
    /// annulus reduces to as `t → 0`.
    pub fn equivalent_sheet_k0(&self) -> f64 {
        let (r, t) = self.mean_radius_thickness();
        self.m0 * self.pole_pairs as f64 * t / r
    }
}

/// Exterior multipole coefficient of a z-directed current sheet
/// `K_z = K0 cos(p θ)` at radius `R` in open free space, from the 2-D
/// free-space Green's function (Biot–Savart):
///
/// ```text
///   B_r = D r^{-(p+1)} sin(p θ) ,   B_θ = D r^{-(p+1)} cos(p θ) ,
///   D = μ₀ K0 R^{p+1} / 2 .
/// ```
///
/// This is an **independent** derivation (Green's function, not scalar-
/// potential matching), so agreement with the thin-band limit of
/// [`SlotlessPm::exterior_coeff`] cross-validates the oracle. Only the
/// **magnitude** `|D|` is used for the phase-robust self-validation (the
/// current-sheet pattern is 90° rotated in θ from the radial-magnet
/// pattern, but `|B|` is θ-independent for both).
pub fn current_sheet_exterior_coeff(k0: f64, r_sheet: f64, pole_pairs: u32) -> f64 {
    let p = pole_pairs as f64;
    MU_0 * k0 * r_sheet.powf(p + 1.0) / 2.0
}

/// Relative error `|C_scalar / D_sheet − 1|` between the slotless-PM
/// exterior coefficient and its Biot–Savart current-sheet equivalent, in
/// the thin-magnet limit. The test-plan self-validation gate (≤ 0.5 %):
/// as `t/R → 0` this error → 0 like `O((t/R)²)`.
pub fn self_validation_rel_error(pm: &SlotlessPm) -> f64 {
    let c_scalar = pm.exterior_coeff();
    let (r, _t) = pm.mean_radius_thickness();
    let d_sheet = current_sheet_exterior_coeff(pm.equivalent_sheet_k0(), r, pm.pole_pairs);
    ((c_scalar / d_sheet) - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exterior_magnitude_is_theta_independent() {
        let pm = SlotlessPm::new(0.030, 0.040, 1.0e6, 2);
        let r = 0.045_f64;
        let c = pm.exterior_coeff() * r.powf(-3.0);
        for k in 0..16 {
            let theta = 2.0 * PI * k as f64 / 16.0;
            let (br, bth) = pm.exterior_field(r, theta);
            let mag = (br * br + bth * bth).sqrt();
            assert!(
                (mag - c.abs()).abs() <= 1e-9 * c.abs(),
                "|B| not θ-independent: {mag} vs {}",
                c.abs()
            );
        }
    }

    #[test]
    fn thin_band_self_validates_against_current_sheet() {
        // Shrinking t/R drives the scalar-potential coefficient onto the
        // independent Biot–Savart current-sheet coefficient.
        let r_mean = 0.040;
        let mut prev = f64::INFINITY;
        for &tf in &[0.02, 0.01, 0.005] {
            let t = tf * r_mean;
            let pm = SlotlessPm::new(r_mean - t / 2.0, r_mean + t / 2.0, 1.0e6, 2);
            let err = self_validation_rel_error(&pm);
            assert!(
                err <= 5e-3,
                "self-validation err {:.4}% at t/R={tf} exceeds 0.5% gate",
                err * 100.0
            );
            assert!(err < prev, "self-validation not monotone in t/R");
            prev = err;
        }
    }

    #[test]
    fn cartesian_matches_polar_conversion() {
        let pm = SlotlessPm::new(0.030, 0.040, 5.0e5, 3);
        let r = 0.050;
        for k in 0..8 {
            let theta = 2.0 * PI * k as f64 / 8.0;
            let (br, bth) = pm.exterior_field(r, theta);
            let (c, s) = (theta.cos(), theta.sin());
            let (bx_e, by_e) = (br * c - bth * s, br * s + bth * c);
            let (bx, by) = pm.exterior_field_xy(r * c, r * s);
            // Scale the tolerance by the field magnitude (individual
            // Cartesian components can be near zero while the total is not).
            let scale = (bx_e * bx_e + by_e * by_e).sqrt().max(1e-12);
            assert!((bx - bx_e).abs() <= 1e-12 * scale);
            assert!((by - by_e).abs() <= 1e-12 * scale);
        }
    }

    #[test]
    #[should_panic(expected = "pole_pairs ≥ 2")]
    fn rejects_p1() {
        let _ = SlotlessPm::new(0.030, 0.040, 1.0, 1);
    }

    #[test]
    #[should_panic(expected = "r ≥ r_outer")]
    fn interior_field_panics() {
        let pm = SlotlessPm::new(0.030, 0.040, 1.0, 2);
        let _ = pm.exterior_field(0.035, 0.0);
    }
}
