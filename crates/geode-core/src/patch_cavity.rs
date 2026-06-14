//! Cavity-model analytic oracle for rectangular microstrip patch
//! antennas (Epic #226 Phase 2, issue #228).
//!
//! The in-repo analytic oracle for the patch-antenna FEM benchmark
//! (`benchmarks/patch_antenna/`), playing the role
//! [`crate::mohan`] plays for the spiral inductor: closed-form
//! transmission-line / cavity-model expressions to sanity-check the
//! field-solver resonance, bandwidth, and input resistance.
//!
//! Implements the textbook rectangular-patch cavity model of Balanis,
//! *Antenna Theory: Analysis and Design*, 4th ed., §14.2 (the dominant
//! TM₀₁₀ mode of a `W × L` patch on a substrate of height `h` and
//! relative permittivity `ε_r`):
//!
//! - **Effective permittivity** (Hammerstad, Balanis 14-1):
//!   ```text
//!   ε_eff = (ε_r + 1)/2 + (ε_r − 1)/2 · [1 + 12 h/W]^(−1/2).
//!   ```
//! - **Fringing line extension** (Balanis 14-3):
//!   ```text
//!   ΔL = 0.412 h · (ε_eff + 0.3)(W/h + 0.264)
//!                 / [(ε_eff − 0.258)(W/h + 0.8)].
//!   ```
//! - **Resonant frequency** of the dominant TM₀₁₀ mode (Balanis 14-2 /
//!   14-4), with the fringing-extended effective length
//!   `L_eff = L + 2ΔL`:
//!   ```text
//!   f_res = c / (2 L_eff √ε_eff).
//!   ```
//!   Solving for the physical length that resonates at a target `f_r`
//!   inverts this: `L = c/(2 f_r √ε_eff) − 2ΔL` (Balanis 14-6 — note
//!   ΔL itself depends only on W/h and ε_eff, not on L).
//! - **Two-slot input resistance** at the radiating edge (Balanis
//!   14-17 with the self-/mutual-conductance 14-12 / 14-18a) and its
//!   `cos²` taper for an inset feed at depth `y₀` (Balanis 14-20a):
//!   ```text
//!   R_in(y₀) = R_in(0) · cos²(π y₀ / L),
//!   R_in(0) = 1 / (2 (G₁ + G₁₂)).
//!   ```
//! - **Fractional bandwidth** from the loaded quality factor (a
//!   Q-based order-of-magnitude estimate; FR-4 dielectric loss
//!   dominates for the bundled fixture):
//!   ```text
//!   BW ≈ (VSWR − 1) / (Q √VSWR),     Q ≈ 1 / tan δ  (loss-limited).
//!   ```
//!
//! These closed forms assume an idealized cavity (perfect magnetic
//! side walls, a single dominant mode, thin substrate) — a ~3–5 %
//! sanity oracle for the resonance, not a 1 %-grade reference: the
//! FR-4 `ε_r` is only specified to ±0.2 and the fringing model is
//! itself a curve fit. Lengths are in **meters**, frequencies in
//! **hertz**, conductances in **siemens**, resistances in **ohms**.

use std::f64::consts::PI;

/// Speed of light in vacuum (m/s).
pub const C_M_PER_S: f64 = 2.997_924_58e8;

/// Free-space impedance η₀ (Ω).
pub const ETA_0_OHM: f64 = 376.730_313_668;

/// Rectangular microstrip patch geometry + substrate (lengths in
/// meters).
#[derive(Debug, Clone, Copy)]
pub struct PatchCavity {
    /// Patch width `W` (the non-resonant, radiating-edge dimension).
    pub width: f64,
    /// Patch length `L` (the resonant dimension, ≈ λ/2 in the
    /// dielectric — sets `f_res`).
    pub length: f64,
    /// Substrate height `h`.
    pub height: f64,
    /// Substrate relative permittivity `ε_r`.
    pub eps_r: f64,
    /// Substrate loss tangent `tan δ` (used by the loss-limited Q /
    /// fractional-bandwidth estimate).
    pub tan_delta: f64,
}

impl PatchCavity {
    /// Hammerstad effective permittivity `ε_eff` (Balanis 14-1).
    pub fn epsilon_eff(&self) -> f64 {
        let w_over_h = self.width / self.height;
        0.5 * (self.eps_r + 1.0) + 0.5 * (self.eps_r - 1.0) * (1.0 + 12.0 / w_over_h).powf(-0.5)
    }

    /// Fringing line extension `ΔL` per radiating edge (Balanis 14-3).
    pub fn delta_l(&self) -> f64 {
        let e = self.epsilon_eff();
        let w_over_h = self.width / self.height;
        0.412 * self.height * ((e + 0.3) * (w_over_h + 0.264)) / ((e - 0.258) * (w_over_h + 0.8))
    }

    /// Effective (fringing-extended) resonant length `L_eff = L + 2ΔL`.
    pub fn effective_length(&self) -> f64 {
        self.length + 2.0 * self.delta_l()
    }

    /// Resonant frequency of the dominant TM₀₁₀ mode (Balanis 14-4):
    /// `f_res = c / (2 L_eff √ε_eff)`.
    pub fn resonant_frequency(&self) -> f64 {
        C_M_PER_S / (2.0 * self.effective_length() * self.epsilon_eff().sqrt())
    }

    /// Free-space wavelength at the cavity-model resonance (m).
    pub fn resonant_wavelength(&self) -> f64 {
        C_M_PER_S / self.resonant_frequency()
    }

    /// Single radiating-slot conductance `G₁` (Balanis 14-12, the
    /// `W/λ₀ ≪ 1` closed form valid for thin patches):
    /// `G₁ = (1/90)(W/λ₀)²` for `W < λ₀`.
    ///
    /// `lambda_0` is the free-space wavelength at the evaluation
    /// frequency (use [`resonant_wavelength`](Self::resonant_wavelength)
    /// for the resonant value).
    pub fn slot_conductance(&self, lambda_0: f64) -> f64 {
        let w_over_lambda = self.width / lambda_0;
        if w_over_lambda < 1.0 {
            (1.0 / 90.0) * w_over_lambda * w_over_lambda
        } else {
            // Balanis 14-8a wide-slot form.
            (1.0 / 120.0) * w_over_lambda
        }
    }

    /// Mutual conductance `G₁₂` between the two radiating slots
    /// (Balanis 14-18a), integrated for the dominant mode:
    /// `G₁₂ = (1/120π²) ∫₀^π [sin((k₀W/2)cosθ)/cosθ]² J₀(k₀ L sinθ) sin³θ dθ`.
    ///
    /// Evaluated at the free-space wavelength `lambda_0`.
    pub fn mutual_conductance(&self, lambda_0: f64) -> f64 {
        let k0 = 2.0 * PI / lambda_0;
        let n = 400;
        let mut acc = 0.0_f64;
        // Midpoint rule over θ ∈ (0, π).
        for i in 0..n {
            let theta = PI * (i as f64 + 0.5) / n as f64;
            let (s, c) = (theta.sin(), theta.cos());
            let arg = 0.5 * k0 * self.width * c;
            // sin(arg)/cosθ → (k₀W/2) as cosθ → 0 (l'Hôpital).
            let factor = if c.abs() < 1e-9 {
                0.5 * k0 * self.width
            } else {
                arg.sin() / c
            };
            let j0 = bessel_j0(k0 * self.length * s);
            acc += factor * factor * j0 * s * s * s;
        }
        let integral = acc * (PI / n as f64);
        integral / (120.0 * PI * PI)
    }

    /// Edge input resistance `R_in(0) = 1 / (2 (G₁ + G₁₂))` (Balanis
    /// 14-17, two radiating slots in odd-mode field distribution),
    /// evaluated at the cavity-model resonance.
    pub fn edge_resistance(&self) -> f64 {
        let lambda_0 = self.resonant_wavelength();
        let g1 = self.slot_conductance(lambda_0);
        let g12 = self.mutual_conductance(lambda_0);
        let r_ohm = 1.0 / (2.0 * (g1 + g12));
        // Convert the slot conductances (in S, SI) to a resistance in
        // ohms: G₁/G₁₂ are dimensionless-of-η₀ in Balanis' normalized
        // form (1/90·(W/λ)² etc. give the conductance in siemens when
        // the slot field is normalized to unit voltage), so 1/(2(G₁+G₁₂))
        // is already in ohms.
        r_ohm
    }

    /// Inset-fed input resistance at probe/inset depth `y0` from a
    /// radiating edge (Balanis 14-20a): `R_in(y0) = R_in(0) cos²(π y0/L)`.
    pub fn inset_resistance(&self, y0: f64) -> f64 {
        let taper = (PI * y0 / self.length).cos();
        self.edge_resistance() * taper * taper
    }

    /// Loss-limited quality factor `Q ≈ 1 / tan δ` — the dielectric
    /// dissipation factor that dominates the patch loaded-Q for a lossy
    /// FR-4 substrate (radiation and conductor Q are higher, so the
    /// total parallel Q is loss-limited). Returns `+∞` for `tan δ = 0`.
    pub fn loss_limited_q(&self) -> f64 {
        if self.tan_delta <= 0.0 {
            f64::INFINITY
        } else {
            1.0 / self.tan_delta
        }
    }

    /// Fractional −10 dB-ish impedance bandwidth from the loaded Q at a
    /// design VSWR (Balanis 14-... matched-bandwidth form):
    /// `BW = (VSWR − 1) / (Q √VSWR)`. For the conventional −10 dB
    /// reflection bandwidth use `vswr ≈ 1.9249` (|Γ| = 1/√10).
    pub fn fractional_bandwidth(&self, vswr: f64) -> f64 {
        let q = self.loss_limited_q();
        if !q.is_finite() {
            return 0.0;
        }
        (vswr - 1.0) / (q * vswr.sqrt())
    }

    /// Resonant physical length that places `f_res` at the target
    /// frequency `f_r` for this `W`/`h`/`ε_r` (Balanis 14-6 design
    /// inversion): `L = c/(2 f_r √ε_eff) − 2ΔL`. (ΔL and ε_eff depend
    /// only on `W/h` and `ε_r`, not on `L`, so `self.length` is
    /// ignored.)
    pub fn design_length(&self, f_r: f64) -> f64 {
        let e = self.epsilon_eff();
        C_M_PER_S / (2.0 * f_r * e.sqrt()) - 2.0 * self.delta_l()
    }
}

/// Bessel function of the first kind, order 0, via a polynomial
/// approximation (Abramowitz & Stegun 9.4.1 / 9.4.3). Used only by the
/// mutual-conductance slot integral, where `< 1e-7` accuracy is far
/// more than the cavity model itself warrants.
fn bessel_j0(x: f64) -> f64 {
    let ax = x.abs();
    if ax < 8.0 {
        let y = x * x;
        let p1 = 57_568_490_574.0
            + y * (-13_362_590_354.0
                + y * (651_619_640.7
                    + y * (-11_214_424.18 + y * (77_392.330_17 + y * -184.905_245_6))));
        let p2 = 57_568_490_411.0
            + y * (1_029_532_985.0
                + y * (9_494_680.718 + y * (59_272.648_53 + y * (267.853_271_2 + y))));
        p1 / p2
    } else {
        let z = 8.0 / ax;
        let y = z * z;
        let xx = ax - std::f64::consts::FRAC_PI_4;
        let p1 = 1.0
            + y * (-0.109_862_86e-2
                + y * (0.273_451_04e-4 + y * (-0.207_337_84e-5 + y * 0.209_388_72e-6)));
        let p2 = -0.156_249_995e-1
            + y * (0.143_048_8e-3
                + y * (-0.691_114_6e-5 + y * (0.762_109_5e-6 + y * -0.934_945_2e-7)));
        (std::f64::consts::FRAC_2_PI / ax).sqrt() * (xx.cos() * p1 - z * xx.sin() * p2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bundled benchmark patch fixture (`tests/fixtures/patch_2g4.msh`
    /// provenance): W = 38 mm, L = 29 mm, h = 1.6 mm FR-4
    /// (ε_r = 4.4, tan δ = 0.02).
    fn fixture_patch() -> PatchCavity {
        PatchCavity {
            width: 38.0e-3,
            length: 29.0e-3,
            height: 1.6e-3,
            eps_r: crate::mesh::patch::EPS_R_SUBSTRATE,
            tan_delta: crate::mesh::patch::TAN_DELTA_SUBSTRATE,
        }
    }

    /// Balanis 4e, Example 14.1: a rectangular patch designed for
    /// f_r = 10 GHz on ε_r = 2.2, h = 0.1588 cm. The textbook reports
    /// W = 1.186 cm, ε_eff = 1.972, ΔL = 0.081 cm, L = 0.906 cm — the
    /// published worked numbers are the oracle.
    fn balanis_example_14_1() -> PatchCavity {
        PatchCavity {
            width: 1.186e-2, // Balanis design output W = 1.186 cm
            length: 0.906e-2,
            height: 0.1588e-2,
            eps_r: 2.2,
            tan_delta: 0.0009, // RT/duroid 5880, not used for f_res
        }
    }

    /// ε_eff reproduces Balanis Example 14.1's published 1.972.
    #[test]
    fn balanis_epsilon_eff() {
        let e = balanis_example_14_1().epsilon_eff();
        assert!(
            (e - 1.972).abs() < 1e-3,
            "ε_eff = {e:.5}, Balanis Ex 14.1 reports 1.972"
        );
    }

    /// ΔL reproduces Balanis Example 14.1's published 0.081 cm.
    #[test]
    fn balanis_delta_l() {
        let dl_cm = balanis_example_14_1().delta_l() * 100.0;
        assert!(
            (dl_cm - 0.081).abs() < 1e-3,
            "ΔL = {dl_cm:.5} cm, Balanis Ex 14.1 reports 0.081 cm"
        );
    }

    /// The design length 0.906 cm resonates back at the 10 GHz target
    /// (the round-trip of Balanis 14-4 / 14-6).
    #[test]
    fn balanis_resonant_frequency_round_trip() {
        let f_ghz = balanis_example_14_1().resonant_frequency() / 1e9;
        assert!(
            (f_ghz - 10.0).abs() < 0.05,
            "f_res = {f_ghz:.4} GHz, Balanis Ex 14.1 designs for 10 GHz"
        );
    }

    /// The design inversion (Balanis 14-6) recovers the published
    /// L = 0.906 cm for the 10 GHz target.
    #[test]
    fn balanis_design_length() {
        let l_cm = balanis_example_14_1().design_length(10e9) * 100.0;
        assert!(
            (l_cm - 0.906).abs() < 1e-3,
            "design L = {l_cm:.5} cm, Balanis Ex 14.1 reports 0.906 cm"
        );
    }

    /// The edge input resistance of the Balanis example sits in the
    /// textbook hundreds-of-ohms range for a half-wave patch (Balanis
    /// notes edge resistances of ~200–300 Ω, tapered down by an inset
    /// to match 50 Ω).
    #[test]
    fn balanis_edge_resistance_in_range() {
        let r = balanis_example_14_1().edge_resistance();
        assert!(
            (100.0..400.0).contains(&r),
            "edge R_in(0) = {r:.1} Ω outside the expected (100, 400) Ω band"
        );
        // An inset to the patch center shorts the radiating-edge field,
        // driving R_in → 0.
        let r_center = balanis_example_14_1().inset_resistance(0.906e-2 / 2.0);
        assert!(
            r_center < 1.0,
            "R_in at the patch center should taper to ~0, got {r_center:.3} Ω"
        );
        // The inset taper is monotone decreasing from the edge inward.
        let r_quarter = balanis_example_14_1().inset_resistance(0.906e-2 / 4.0);
        assert!(r_quarter < r && r_quarter > r_center);
    }

    /// Pinned cavity-model resonance for the FR-4 fixture geometry
    /// (W = 38, L = 29, h = 1.6 mm, ε_r = 4.4): the value the FEM
    /// benchmark (`benchmarks/patch_antenna/`) quotes as its analytic
    /// oracle. Hand evaluation gives ε_eff ≈ 4.086, ΔL ≈ 0.739 mm,
    /// f_res ≈ 2.435 GHz.
    #[test]
    fn fixture_cavity_model_pinned() {
        let p = fixture_patch();
        assert!(
            (p.epsilon_eff() - 4.0856).abs() < 1e-3,
            "ε_eff = {}",
            p.epsilon_eff()
        );
        assert!(
            (p.delta_l() * 1000.0 - 0.7388).abs() < 1e-3,
            "ΔL = {} mm",
            p.delta_l() * 1000.0
        );
        let f_ghz = p.resonant_frequency() / 1e9;
        assert!(
            (f_ghz - 2.435).abs() < 0.01,
            "fixture cavity-model f_res = {f_ghz:.4} GHz, expected ≈ 2.435 GHz"
        );
    }

    /// Loss-limited Q and fractional bandwidth for FR-4 (tan δ = 0.02):
    /// Q ≈ 50, so the −10 dB (VSWR ≈ 1.92) fractional bandwidth is a
    /// percent-level figure.
    #[test]
    fn fixture_bandwidth_loss_limited() {
        let p = fixture_patch();
        assert!((p.loss_limited_q() - 50.0).abs() < 1e-9);
        // −10 dB return loss ↔ |Γ| = 1/√10 ↔ VSWR = (1+|Γ|)/(1−|Γ|).
        let gamma = (0.1_f64).sqrt();
        let vswr = (1.0 + gamma) / (1.0 - gamma);
        let bw = p.fractional_bandwidth(vswr);
        assert!(
            (0.005..0.05).contains(&bw),
            "FR-4 loss-limited fractional BW = {:.4} outside (0.5%, 5%)",
            bw
        );
    }

    /// J₀ sanity: J₀(0) = 1, first zero near 2.4048, and matches a few
    /// tabulated values to the approximation's accuracy.
    #[test]
    fn bessel_j0_known_values() {
        assert!((bessel_j0(0.0) - 1.0).abs() < 1e-7);
        assert!((bessel_j0(1.0) - 0.765_197_686_5).abs() < 1e-6);
        assert!((bessel_j0(2.404_825_558)).abs() < 1e-5);
        assert!((bessel_j0(10.0) - (-0.245_935_764_5)).abs() < 1e-6);
    }
}
