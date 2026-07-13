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

// ─────────────────────────────────────────────────────────────────────
// Driven interaction torque (Epic #448, Phase 3b — the capstone oracle)
// ─────────────────────────────────────────────────────────────────────
//
// A pure-PM slotless machine has **zero** net torque by symmetry (the
// exterior integrand `B_r B_θ ∝ cos(pθ) sin(pθ)` integrates to zero on any
// coaxial contour), so it is a *degenerate* torque discriminator — a
// mesh-symmetry artifact, not physics (#448 AC #4, #339 lesson). To obtain a
// **non-trivial `T(θ_r)` with a clean closed form**, drive the same air gap
// with a *stator winding current sheet* fixed in the stator frame and
// compute the **PM-vs-stator interaction torque**, which has the classic
// `T ∝ cos(p θ_r)` closed form.
//
// # Geometry / fields
//
// The rotor PM (band `R1 ≤ r ≤ R2`, magnetization `M0 cos(p(θ−θ_r)) r̂`,
// mechanically rotated by `θ_r`) produces the **exterior** multipole in the
// air gap (`r ≥ R2`):
//
// ```text
//   B_r^M = C_M r^{-(p+1)} cos(p(θ−θ_r)) ,  B_θ^M = C_M r^{-(p+1)} sin(p(θ−θ_r)) ,
//   C_M = μ₀ M0 p (R2^{p+1} − R1^{p+1}) / [2 (p+1)]   (= SlotlessPm::exterior_coeff).
// ```
//
// The stator winding, an axial current density `J_z(r,θ) = J0 cos(p θ)`
// distributed over an **outer** band `R_a ≤ r ≤ R_b` (with `R_b > R_a >`
// the gap radius), produces a *regular* interior harmonic in the gap
// (`r ≤ R_a`). A single sheet `dK = J0 dr'` at radius `r'` contributes
// interior `B_θ = −(μ₀ dK/2)(r/r')^{p−1} cos(pθ)` (2-D Green's-function /
// tangential-`H`-jump matching), so integrating the band:
//
// ```text
//   B_r^S(r,θ) = −G_S r^{p−1} sin(p θ) ,  B_θ^S(r,θ) = −G_S r^{p−1} cos(p θ) ,
//   G_S = (μ₀ J0 / 2) ∫_{R_a}^{R_b} r'^{−(p−1)} dr' .
// ```
//
// # Torque
//
// The Maxwell-stress torque per axial length `L` on the rotor, on any gap
// contour `r_g` inside both bands, is `T = (L r_g²/μ₀) ∮ B_r B_θ dθ` with
// `B = B^M + B^S`. Both self-terms integrate to zero; the cross-term
// integrand collapses (product-to-sum) to the **θ-independent** constant
// `C_M r_g^{−(p+1)} · (−G_S r_g^{p−1}) · cos(p θ_r)`, whence
//
// ```text
//   T(θ_r) = −(2π L / μ₀) · C_M · G_S · cos(p θ_r) .
// ```
//
// The contour radius `r_g` **cancels** — the torque is a conserved flux
// through any gap contour, a strong invariant this benchmark exploits.

/// A slotless surface-PM rotor driven by a `θ`-distributed stator winding
/// current sheet — the Epic #448 capstone torque oracle. Wraps a
/// [`SlotlessPm`] rotor (rotated mechanically by `theta_r`) plus a stator
/// axial-current band `R_a ≤ r ≤ R_b` carrying `J_z = J0 cos(p θ)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotlessPmDriven {
    /// The rotor permanent-magnet annulus (its `exterior_coeff` is `C_M`).
    pub rotor: SlotlessPm,
    /// Stator winding-band inner radius `R_a` (m), `R_a > R2` (outside the gap).
    pub r_stator_inner: f64,
    /// Stator winding-band outer radius `R_b` (m), `R_b > R_a`.
    pub r_stator_outer: f64,
    /// Stator peak axial current density `J0` (A/m²): `J_z = J0 cos(p θ)`
    /// over the band, `0` elsewhere. The winding shares the rotor's pole-pair
    /// count `p` (a synchronous machine).
    pub j0: f64,
    /// Axial (stack) length `L` (m) — the per-length torque scales linearly.
    pub l_axial: f64,
}

impl SlotlessPmDriven {
    /// Construct a validated driven configuration.
    ///
    /// # Panics
    ///
    /// Panics unless the stator band lies strictly outside the rotor
    /// (`r_stator_inner > rotor.r_outer`), `r_stator_outer > r_stator_inner`,
    /// and `j0`, `l_axial` are finite with `l_axial > 0`.
    pub fn new(
        rotor: SlotlessPm,
        r_stator_inner: f64,
        r_stator_outer: f64,
        j0: f64,
        l_axial: f64,
    ) -> Self {
        assert!(
            r_stator_inner.is_finite()
                && r_stator_outer.is_finite()
                && r_stator_inner > rotor.r_outer
                && r_stator_outer > r_stator_inner,
            "SlotlessPmDriven requires rotor.r_outer ({}) < r_stator_inner ({r_stator_inner}) \
             < r_stator_outer ({r_stator_outer})",
            rotor.r_outer
        );
        assert!(
            j0.is_finite() && l_axial.is_finite() && l_axial > 0.0,
            "SlotlessPmDriven requires finite j0 ({j0}) and l_axial > 0 (got {l_axial})"
        );
        Self {
            rotor,
            r_stator_inner,
            r_stator_outer,
            j0,
            l_axial,
        }
    }

    /// Stator interior-field coefficient
    /// `G_S = (μ₀ J0 / 2) ∫_{R_a}^{R_b} r'^{−(p−1)} dr'`, so that in the gap
    /// (`r ≤ R_a`) the stator field is
    /// `B_r^S = −G_S r^{p−1} sin(pθ)`, `B_θ^S = −G_S r^{p−1} cos(pθ)`.
    ///
    /// The radial integral is the `p = 2` logarithm `ln(R_b/R_a)` and the
    /// general power law `(R_b^{2−p} − R_a^{2−p})/(2−p)` otherwise.
    pub fn stator_interior_coeff(&self) -> f64 {
        let p = self.rotor.pole_pairs as f64;
        let (ra, rb) = (self.r_stator_inner, self.r_stator_outer);
        let radial_integral = if (p - 2.0).abs() < 1e-12 {
            (rb / ra).ln()
        } else {
            (rb.powf(2.0 - p) - ra.powf(2.0 - p)) / (2.0 - p)
        };
        0.5 * MU_0 * self.j0 * radial_integral
    }

    /// Interior stator flux density `(B_r^S, B_θ^S)` at polar `(r, θ)` for
    /// `r ≤ r_stator_inner` (the gap region), from the distributed-winding
    /// harmonic. Used by the numeric-quadrature self-check of [`Self::torque`].
    ///
    /// # Panics
    ///
    /// Panics if `r > r_stator_inner` (the interior branch is not valid
    /// inside or beyond the winding band).
    pub fn stator_interior_field(&self, r: f64, theta: f64) -> (f64, f64) {
        assert!(
            r <= self.r_stator_inner,
            "stator_interior_field valid only for r ≤ r_stator_inner ({}), got r = {r}",
            self.r_stator_inner
        );
        let p = self.rotor.pole_pairs as f64;
        let g = self.stator_interior_coeff() * r.powf(p - 1.0);
        let ang = p * theta;
        (-g * ang.sin(), -g * ang.cos())
    }

    /// Total gap flux density `(B_r, B_θ)` = rotor exterior + stator interior
    /// at polar `(r, θ)`, with the rotor mechanically rotated by `theta_r`.
    /// Valid for `rotor.r_outer ≤ r ≤ r_stator_inner`.
    pub fn total_gap_field(&self, r: f64, theta: f64, theta_r: f64) -> (f64, f64) {
        // Rotor field with the magnet pattern rotated by θ_r: evaluate the
        // (unrotated) exterior multipole at the de-rotated angle θ − θ_r,
        // which shifts the cos/sin arguments to p(θ − θ_r).
        let (brm, bthm) = self.rotor.exterior_field(r, theta - theta_r);
        let (brs, bths) = self.stator_interior_field(r, theta);
        (brm + brs, bthm + bths)
    }

    /// Closed-form driven interaction torque per axial length at rotor angle
    /// `theta_r`:
    ///
    /// ```text
    ///   T(θ_r) = −(2π L / μ₀) · C_M · G_S · cos(p θ_r) ,
    /// ```
    ///
    /// with `C_M = rotor.exterior_coeff()` and `G_S =
    /// stator_interior_coeff()`. This is the exact torque on the rotor from
    /// the PM-vs-stator interaction; the self-torques are identically zero.
    pub fn torque(&self, theta_r: f64) -> f64 {
        let p = self.rotor.pole_pairs as f64;
        let c_m = self.rotor.exterior_coeff();
        let g_s = self.stator_interior_coeff();
        -(std::f64::consts::TAU * self.l_axial / MU_0) * c_m * g_s * (p * theta_r).cos()
    }

    /// Peak torque amplitude `|T|_max = (2π L / μ₀) |C_M G_S|` (the `θ_r = 0`
    /// value up to sign). A convenience for reporting / tearsheet scaling.
    pub fn torque_amplitude(&self) -> f64 {
        (std::f64::consts::TAU * self.l_axial / MU_0
            * self.rotor.exterior_coeff()
            * self.stator_interior_coeff())
        .abs()
    }

    /// Numeric-quadrature cross-check of [`Self::torque`]: evaluate the Maxwell
    /// stress line integral `(L r_g²/μ₀) ∮ B_r B_θ dθ` on a gap contour of
    /// radius `r_g`, sampling the *analytic* total field at `n` equal-angle
    /// points (periodic trapezoid rule). Must agree with [`Self::torque`] to
    /// quadrature precision — the derivation's internal gate, run *before*
    /// the FE solve is compared against it.
    ///
    /// # Panics
    ///
    /// Panics unless `rotor.r_outer ≤ r_g ≤ r_stator_inner` and `n ≥ 3`.
    pub fn torque_by_quadrature(&self, theta_r: f64, r_g: f64, n: usize) -> f64 {
        assert!(
            r_g >= self.rotor.r_outer && r_g <= self.r_stator_inner && n >= 3,
            "torque_by_quadrature: need rotor.r_outer ≤ r_g ≤ r_stator_inner and n ≥ 3"
        );
        let dtheta = std::f64::consts::TAU / n as f64;
        let mut acc = 0.0;
        for i in 0..n {
            let theta = std::f64::consts::TAU * i as f64 / n as f64;
            let (br, bth) = self.total_gap_field(r_g, theta, theta_r);
            acc += br * bth;
        }
        self.l_axial * r_g * r_g / MU_0 * acc * dtheta
    }
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

    // ── Driven interaction torque ──────────────────────────────────────

    fn nominal_driven() -> SlotlessPmDriven {
        // Rotor: 2-pole-pair NdFeB band; stator winding in an outer band.
        let rotor = SlotlessPm::new(0.030, 0.040, 1.2 / MU_0, 2);
        SlotlessPmDriven::new(rotor, 0.050, 0.060, 5.0e6, 0.05)
    }

    #[test]
    fn driven_torque_matches_quadrature() {
        // The closed-form torque must equal the numeric Maxwell-stress
        // contour integral of the analytic total field, to quadrature
        // precision, at every rotor angle and independently of r_g.
        let d = nominal_driven();
        let p = d.rotor.pole_pairs;
        for k in 0..24 {
            let theta_r = std::f64::consts::TAU * k as f64 / (24.0 * p as f64);
            let t_closed = d.torque(theta_r);
            for &r_g in &[0.043_f64, 0.045, 0.048] {
                let t_quad = d.torque_by_quadrature(theta_r, r_g, 256);
                let scale = d.torque_amplitude().max(1e-30);
                assert!(
                    (t_closed - t_quad).abs() <= 1e-9 * scale,
                    "closed-form {t_closed} vs quadrature {t_quad} at θ_r={theta_r}, r_g={r_g}"
                );
            }
        }
    }

    #[test]
    fn driven_torque_is_theta_dependent_and_nonzero() {
        // Discriminator-isolation: the analytic T(θ_r) must be a non-trivial
        // (non-constant, non-zero) function of the rotor angle — otherwise
        // the benchmark grades a mesh-symmetry artifact (#448 AC #4).
        let d = nominal_driven();
        let amp = d.torque_amplitude();
        assert!(amp > 0.0, "driven torque amplitude must be non-zero");
        // cos(p·θ_r): peak at θ_r = 0, zero at the quarter electrical period.
        let p = d.rotor.pole_pairs as f64;
        assert!((d.torque(0.0).abs() - amp).abs() <= 1e-9 * amp);
        let quarter = std::f64::consts::FRAC_PI_2 / p; // p·θ_r = π/2
        assert!(
            d.torque(quarter).abs() <= 1e-9 * amp,
            "torque should vanish at the quarter electrical period"
        );
    }

    #[test]
    fn driven_torque_scales_linearly_with_drive_and_length() {
        // T ∝ J0 (through G_S) and ∝ L — sanity on the prefactors.
        let d = nominal_driven();
        let d2 = SlotlessPmDriven::new(d.rotor, 0.050, 0.060, 2.0 * d.j0, 3.0 * d.l_axial);
        assert!((d2.torque(0.0) - 6.0 * d.torque(0.0)).abs() <= 1e-6 * d2.torque(0.0).abs());
    }

    #[test]
    fn stator_p2_coeff_uses_log_integral() {
        // For p = 2 the radial integral is ln(R_b/R_a); check G_S directly.
        let d = nominal_driven();
        let expect = 0.5 * MU_0 * d.j0 * (d.r_stator_outer / d.r_stator_inner).ln();
        assert!((d.stator_interior_coeff() - expect).abs() <= 1e-12 * expect.abs());
    }

    #[test]
    #[should_panic(expected = "r_stator_inner")]
    fn driven_rejects_overlapping_stator() {
        let rotor = SlotlessPm::new(0.030, 0.040, 1.0, 2);
        // Stator inner radius inside the rotor → rejected.
        let _ = SlotlessPmDriven::new(rotor, 0.035, 0.060, 1.0, 0.05);
    }
}
