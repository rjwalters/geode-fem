//! Impedance-extraction post-processing: `Z(ω) → L(ω), R(ω), Q(ω),
//! S-parameters` over port-driven solves (Epic #193, issue #203).
//!
//! Given a driven solve excited through a lumped port
//! ([`crate::lumped_port::LumpedPort`], issue #202), this module
//! reduces the field solution to the circuit quantities that are the
//! epic's actual deliverable:
//!
//! ```text
//! Z(ω)  = V / I                 (port input impedance; V from the
//!                                port-field projection, I from the
//!                                Thevenin admittance relation)
//! R(ω)  = Re Z(ω)               (series resistance)
//! L(ω)  = Im Z(ω) / ω           (series inductance)
//! Q(ω)  = Im Z(ω) / Re Z(ω)     (quality factor)
//! S₁₁(ω) = (Z − Z₀) / (Z + Z₀)  (reflection vs real reference Z₀)
//! ```
//!
//! plus self-resonance detection from the `Im Z(ω)` zero crossing when
//! a sweep brackets it.
//!
//! Everything here is **post-processing over [`DrivenSolution`]** — no
//! new assembly physics. The field-to-circuit reduction reuses the
//! lumped-port flux functional `f_i = ∮ N_i · ê dS` (the same discrete
//! functional that drives the port, so the drive/measure pair is
//! adjoint-consistent; see `lumped_port.rs`).
//!
//! # Frequency sweeps
//!
//! `A(ω) = K + iωC − ω²M` re-forms per frequency by linear combination
//! of ω-independent matrices (the design rationale recorded in
//! PR #198), so the sweep driver [`driven_frequency_sweep`] assembles
//! once through [`DrivenOperator`] and then *re-factors per ω, never
//! re-assembles*. The ω-dependent complex coefficients of the port and
//! Leontovich surface terms (issue #204) are cheap host-side scalar
//! rescales applied inside [`DrivenOperator::solve_at`].
//!
//! # Multi-port S-parameters
//!
//! [`SMatrix`] carries the n-port structure, but only the single-port
//! constructor ([`SMatrix::from_single_port_z`]) is implemented in this
//! phase — the multi-port S-matrix needs one driven solve per excited
//! port and lands with the Phase 3 spiral work (Epic #193).

use burn::tensor::backend::Backend;
use faer::c64;

use crate::driven::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, SurfaceImpedanceBc,
};
use crate::lumped_port::{port_current, port_voltage, LumpedPort};
use crate::TetMesh;

/// Circuit quantities of one port at one frequency, read off a driven
/// solution.
#[derive(Debug, Clone, Copy)]
pub struct PortCircuit {
    /// Port voltage `V = (1/w) ∮ E · ê dS`.
    pub v: c64,
    /// Port current `I = (2 V_inc − V) / R`.
    pub i: c64,
    /// Input impedance `Z = V / I`.
    pub z: c64,
}

impl PortCircuit {
    /// Series resistance `R(ω) = Re Z`.
    pub fn resistance(&self) -> f64 {
        self.z.re
    }

    /// Series inductance `L(ω) = Im Z / ω`.
    pub fn inductance(&self, omega: f64) -> f64 {
        inductance(self.z, omega)
    }

    /// Quality factor `Q(ω) = Im Z / Re Z`.
    pub fn quality_factor(&self) -> f64 {
        quality_factor(self.z)
    }

    /// Single-port reflection coefficient `S₁₁` vs the real reference
    /// impedance `z0`.
    pub fn s11(&self, z0: f64) -> c64 {
        s11(self.z, z0)
    }
}

/// Extract the port circuit quantities `V`, `I`, `Z` from a single
/// driven solution (`e_edges` in `mesh.edges()` order, e.g.
/// [`crate::driven::DrivenSolution::e_edges`]).
///
/// Thin composition of [`crate::lumped_port::port_voltage`] and
/// [`crate::lumped_port::port_current`]; sweeps should prefer
/// [`driven_frequency_sweep`], which reuses the assembled operator and
/// the cached port flux across frequencies.
pub fn extract_port_circuit(
    mesh: &TetMesh,
    port: &LumpedPort<'_>,
    edges: &[[u32; 2]],
    e_edges: &[c64],
) -> PortCircuit {
    let v = port_voltage(mesh, port, edges, e_edges);
    let i = port_current(port, v);
    PortCircuit { v, i, z: v / i }
}

/// Series inductance `L(ω) = Im Z / ω`.
pub fn inductance(z: c64, omega: f64) -> f64 {
    z.im / omega
}

/// Quality factor `Q(ω) = Im Z / Re Z` (±∞ for a lossless reactance).
pub fn quality_factor(z: c64) -> f64 {
    z.im / z.re
}

/// Single-port reflection coefficient vs a **real** reference impedance:
///
/// ```text
/// S₁₁ = (Z − Z₀) / (Z + Z₀).
/// ```
///
/// Limits: `Z = Z₀` (matched) → 0; `Z → 0` (short) → −1;
/// `|Z| → ∞` (open) → +1.
pub fn s11(z: c64, z0: f64) -> c64 {
    (z - z0) / (z + z0)
}

/// Single-frequency S-parameter matrix vs a real reference impedance
/// `Z₀` (n-port structure; Phase 2 implements the single-port path).
///
/// The multi-port constructor requires one driven solve per excited
/// port (column `j` of `S` comes from driving port `j` with all other
/// ports passively terminated) and is deferred to the Phase 3 spiral
/// work of Epic #193 — only [`SMatrix::from_single_port_z`] exists
/// here, and it is exact.
#[derive(Debug, Clone)]
pub struct SMatrix {
    /// Real reference impedance `Z₀`.
    pub z0: f64,
    /// Number of ports `n`.
    pub n_ports: usize,
    /// Row-major `n × n` entries.
    pub s: Vec<c64>,
}

impl SMatrix {
    /// Exact single-port S-matrix from the port input impedance:
    /// `S = [S₁₁]` with `S₁₁ = (Z − Z₀)/(Z + Z₀)`.
    pub fn from_single_port_z(z: c64, z0: f64) -> Self {
        Self {
            z0,
            n_ports: 1,
            s: vec![s11(z, z0)],
        }
    }

    /// Entry `S[i][j]` (0-based).
    ///
    /// # Panics
    ///
    /// Panics if `i` or `j` is out of range.
    pub fn entry(&self, i: usize, j: usize) -> c64 {
        assert!(i < self.n_ports && j < self.n_ports, "S-matrix index");
        self.s[i * self.n_ports + j]
    }
}

/// One frequency point of a port-driven sweep.
#[derive(Debug, Clone)]
pub struct SweepPoint {
    /// Frequency `ω ≡ k₀` (natural units, as in [`crate::driven`]).
    pub omega: f64,
    /// Direct-solve relative residual at this frequency.
    pub residual_rel: f64,
    /// Per-port circuit quantities, in the order the ports were passed
    /// to the sweep.
    pub ports: Vec<PortCircuit>,
}

/// Frequency-sweep driver over a port-driven structure: assemble the
/// ω-independent operator **once** ([`DrivenOperator::assemble`]), then
/// re-form + re-factor `A(ω)` and extract `V`, `I`, `Z` at every
/// requested frequency.
///
/// The expensive Burn volume assembly of `K`, `M(ε)`, `C(σ)` and the
/// source moments runs once for the whole sweep; per frequency only
/// scalar recombination, the sparse LU, and the port readouts remain.
/// One sweep point reproduces the corresponding single-ω
/// [`crate::driven::driven_solve_with_ports`] /
/// [`crate::driven::driven_solve_with_surface_impedance`] call exactly
/// (same arithmetic, same triplet stream).
///
/// `surfaces` composes Leontovich impedance walls (issue #204) into the
/// sweep; their ω-dependent scalar coefficient is re-evaluated at every
/// frequency, as that issue's sweep caveat requires. Pass `&[]` for
/// none.
///
/// # Errors
///
/// Any [`DrivenError`] from assembly or from the per-ω solves; the
/// sweep stops at the first failing frequency.
#[allow(clippy::too_many_arguments)]
pub fn driven_frequency_sweep<B: Backend>(
    mesh: &TetMesh,
    materials: DrivenMaterials<'_>,
    sigma_tet: Option<&[f64]>,
    bcs: &DrivenBcs<'_>,
    ports: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    omegas: &[f64],
    source: &CurrentSource,
    device: &B::Device,
) -> Result<Vec<SweepPoint>, DrivenError> {
    let op = DrivenOperator::assemble::<B>(
        mesh, materials, sigma_tet, bcs, ports, surfaces, source, device,
    )?;
    omegas
        .iter()
        .map(|&omega| {
            let sol = op.solve_at(omega)?;
            let ports = (0..op.n_ports())
                .map(|p| {
                    let v = op.port_voltage(p, &sol.e_edges);
                    let i = op.port_current(p, v);
                    PortCircuit { v, i, z: v / i }
                })
                .collect();
            Ok(SweepPoint {
                omega,
                residual_rel: sol.residual_rel,
                ports,
            })
        })
        .collect()
}

/// All `Im Z(ω)` sign changes of a sampled impedance curve, located by
/// linear interpolation between consecutive samples (an exact-zero
/// sample reports its own ω).
///
/// A sign change marks either a **series-type resonance** (a true zero
/// of `Im Z` — for an inductor the inductive→capacitive `+ → −`
/// crossing is the self-resonant frequency) or a sign flip **through a
/// pole** (parallel anti-resonance, where `|Im Z|` blows up at the
/// bracketing samples instead of shrinking). Distinguishing the two
/// requires inspecting `|Im Z|` near the crossing or sweeping the
/// admittance instead; callers with lossy structures (finite `Re Z`)
/// see finite peaks in both cases and the interpolated ω remains a
/// useful bracket.
///
/// `omegas` and `zs` must have equal length and `omegas` must be
/// strictly increasing; non-finite samples are skipped.
pub fn im_z_zero_crossings(omegas: &[f64], zs: &[c64]) -> Vec<f64> {
    assert_eq!(omegas.len(), zs.len(), "omegas/zs length mismatch");
    let mut crossings = Vec::new();
    let mut prev: Option<(f64, f64)> = None; // (ω, Im Z)
    for (&omega, &z) in omegas.iter().zip(zs.iter()) {
        if !z.im.is_finite() {
            prev = None;
            continue;
        }
        if z.im == 0.0 {
            crossings.push(omega);
            prev = Some((omega, z.im));
            continue;
        }
        if let Some((w1, im1)) = prev {
            if im1 != 0.0 && im1.signum() != z.im.signum() {
                // Linear interpolation of the bracketed zero.
                crossings.push(w1 + (omega - w1) * im1 / (im1 - z.im));
            }
        }
        prev = Some((omega, z.im));
    }
    crossings
}

/// Self-resonant frequency estimate: the first `Im Z(ω)` zero crossing
/// the sweep brackets ([`im_z_zero_crossings`]), or `None` if the sweep
/// does not bracket one.
pub fn detect_srf(omegas: &[f64], zs: &[c64]) -> Option<f64> {
    im_z_zero_crossings(omegas, zs).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(re: f64, im: f64) -> c64 {
        c64::new(re, im)
    }

    /// R/L/Q definitions on a synthetic impedance.
    #[test]
    fn circuit_quantities_match_definitions() {
        let omega = 2.0;
        let z = c(0.5, 3.0);
        let pc = PortCircuit {
            v: c(1.0, 0.0),
            i: c(1.0, 0.0),
            z,
        };
        assert_eq!(pc.resistance(), 0.5);
        assert_eq!(pc.inductance(omega), 1.5);
        assert_eq!(pc.quality_factor(), 6.0);
        assert_eq!(inductance(z, omega), 1.5);
        assert_eq!(quality_factor(z), 6.0);
    }

    /// S₁₁ limits: matched → 0, short → −1, open → +1.
    #[test]
    fn s11_limits() {
        let z0 = 50.0;
        assert!(s11(c(50.0, 0.0), z0).norm() < 1e-15);
        assert!((s11(c(0.0, 0.0), z0) - c(-1.0, 0.0)).norm() < 1e-15);
        let open = s11(c(1e12, 0.0), z0);
        assert!((open - c(1.0, 0.0)).norm() < 1e-9);
        // Lossless reactance reflects with unit magnitude.
        let reactive = s11(c(0.0, 17.0), z0);
        assert!((reactive.norm() - 1.0).abs() < 1e-15);
    }

    /// Single-port S-matrix is the scalar S₁₁.
    #[test]
    fn single_port_s_matrix() {
        let z = c(25.0, 10.0);
        let m = SMatrix::from_single_port_z(z, 50.0);
        assert_eq!(m.n_ports, 1);
        assert_eq!(m.entry(0, 0), s11(z, 50.0));
    }

    /// SRF detection on a series-LC impedance `Im Z = ωL − 1/(ωC)`:
    /// the analytic resonance `ω₀ = 1/√(LC)` is bracketed and located
    /// to interpolation accuracy.
    #[test]
    fn detects_series_resonance_zero_crossing() {
        let (l, cap) = (2.0_f64, 0.125_f64);
        let omega0 = 1.0 / (l * cap).sqrt(); // = 2.0
        let omegas: Vec<f64> = (1..=12).map(|k| 0.3 * k as f64).collect();
        let zs: Vec<c64> = omegas
            .iter()
            .map(|&w| c(0.01, l * w - 1.0 / (cap * w)))
            .collect();
        let srf = detect_srf(&omegas, &zs).expect("sweep brackets the resonance");
        assert!(
            (srf - omega0).abs() < 0.05,
            "series-LC SRF: got {srf}, want {omega0}"
        );
    }

    /// A monotone inductive curve has no crossing; a sweep that does
    /// not bracket the resonance returns `None`.
    #[test]
    fn no_crossing_returns_none() {
        let omegas = [1.0, 2.0, 3.0];
        let zs = [c(0.1, 1.0), c(0.1, 2.0), c(0.1, 3.0)];
        assert!(detect_srf(&omegas, &zs).is_none());
        assert!(im_z_zero_crossings(&omegas, &zs).is_empty());
    }

    /// An exact-zero sample reports its own ω; multiple crossings are
    /// all reported in order.
    #[test]
    fn exact_zero_and_multiple_crossings() {
        let omegas = [1.0, 2.0, 3.0, 4.0];
        let zs = [c(0.1, -1.0), c(0.1, 0.0), c(0.1, 1.0), c(0.1, -1.0)];
        let crossings = im_z_zero_crossings(&omegas, &zs);
        assert_eq!(crossings.len(), 2);
        assert_eq!(crossings[0], 2.0);
        assert!((crossings[1] - 3.5).abs() < 1e-15);
    }
}
