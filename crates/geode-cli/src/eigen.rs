//! `geode eigen`: PEC-cavity eigenmodes → resonant frequencies (and `Q`).
//!
//! Two pencils, selected from the resolved spec
//! ([`crate::problem::Problem::has_complex_materials`]):
//!
//! - **Lossless** (real `ε_r`, no `absorbing_regions`) — wraps
//!   [`geode_core::eigen::pec_cavity::solve_pec_cavity_modes`]: the
//!   interior-reduced first-order Nédélec pencil `K x = k₀² M_ε x`
//!   (PEC surfaces eliminated edge-exactly from their tagged triangles),
//!   assembled sparse and solved with the pure-Rust sparse shift-invert
//!   Lanczos around `σ = k₀,shift²`. Every mode's `Q` is `null`.
//! - **Lossy / open** (any `Im(ε_r) < 0`, and / or `absorbing_regions`;
//!   issue #706) — wraps
//!   [`geode_core::eigen::lossy_cavity::solve_lossy_cavity_modes`]: the
//!   same pencil with complex `ε_r` (isotropic) or, with
//!   `absorbing_regions`, the matched box-UPML tensors
//!   ([`crate::problem::Problem::upml_tensors`]) **frozen at the shift
//!   frequency** `k₀ = eigen.shift` — a linear complex-symmetric pencil
//!   solved with the sparse complex shift-invert Lanczos. Modes carry
//!   complex `λ` / `k₀` (`lambda_im`, `k0_im`), `f = Re(k₀)·c / (2πL)` and
//!   `Q = Re(k₀) / (2|Im(k₀)|)` (`geode_util::eigen::q_factor`;
//!   `exp(+jωt)`, so `Im(k₀) > 0` decays). The UPML is not iterated to
//!   self-consistency with each mode's `Re(k₀)`: modes far from the shift
//!   see a mistuned absorber (a documented approximation).
//!
//! With `--outdir` (issue #684) each mode's eigenvector over the interior
//! edges is scattered back to the full edge vector (PEC edges = 0) and
//! written as `E_mode_<index>.vtu` ([`crate::export`]): `E_real` only for
//! a lossless pencil (arbitrary sign), `E_real` + `E_imag` for a complex
//! one (arbitrary complex phase).

use std::path::Path;
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::nedelec::tet_centroids;
use geode_core::constants::C_M_PER_S;
use geode_core::eigen::lossy_cavity::{
    LossyCavityMaterials, LossyCavitySettings, solve_lossy_cavity_modes,
};
use geode_core::eigen::pec_cavity::{PecCavitySettings, solve_pec_cavity_modes};

use crate::backend::CompiledBackend;
use crate::check::{
    eigen_settings_summary, mesh_summary, pec_summaries, region_summaries, upml_summaries,
};
use crate::error::CliError;
use crate::export::{OutDir, eps_per_node, mode_file_name};
use crate::problem::{self, EigenTarget, Problem};
use crate::report::{EigenReport, EigenSolverStats, ModeResult, Provenance};
use crate::spec::Analysis;

type B = CompiledBackend;

/// One solved mode, pencil-agnostic: complex `λ` / `k₀` (imaginary parts
/// exactly `0` on the lossless path) and an interior-edge eigenvector.
struct SolvedMode {
    lambda: c64,
    k0: c64,
    residual_rel: f64,
    vector: Vec<c64>,
}

/// Pencil-agnostic solve result.
struct Solved {
    modes: Vec<SolvedMode>,
    n_null_filtered: usize,
    n_overdamped_filtered: Option<usize>,
    complex: bool,
    upml_reference_k0: Option<f64>,
}

fn solve_lossless(p: &Problem, target: &EigenTarget) -> Result<Solved, CliError> {
    let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();
    let settings = PecCavitySettings {
        max_iters: target.max_iters,
        tol: target.tol,
        residual_tol: target.residual_tol,
        ..PecCavitySettings::new(target.sigma(), target.n_modes)
    };
    let device = <B as BackendTypes>::Device::default();
    let solved =
        solve_pec_cavity_modes::<B>(&p.tagged.mesh, &eps_r, &p.pec_mask, &settings, &device)?;
    Ok(Solved {
        modes: solved
            .modes
            .into_iter()
            .map(|m| SolvedMode {
                lambda: c64::new(m.lambda, 0.0),
                k0: c64::new(m.k0, 0.0),
                residual_rel: m.residual_rel,
                vector: m.vector.iter().map(|&x| c64::new(x, 0.0)).collect(),
            })
            .collect(),
        n_null_filtered: solved.n_null_filtered,
        n_overdamped_filtered: None,
        complex: false,
        upml_reference_k0: None,
    })
}

fn solve_lossy(p: &Problem, target: &EigenTarget) -> Result<Solved, CliError> {
    let settings = LossyCavitySettings {
        max_iters: target.max_iters,
        tol: target.tol,
        residual_tol: target.residual_tol,
        ..LossyCavitySettings::new(target.sigma(), target.n_modes)
    };
    let device = <B as BackendTypes>::Device::default();
    // UPML frozen at the shift: one linear pencil, no self-consistency.
    let upml_reference_k0 = (!p.upml.is_empty()).then_some(target.shift.k0);
    let tensors =
        upml_reference_k0.map(|k0_ref| p.upml_tensors(&tet_centroids(&p.tagged.mesh), k0_ref));
    let materials = match &tensors {
        Some((eps, nu)) => LossyCavityMaterials::Tensor { eps, nu },
        None => LossyCavityMaterials::Isotropic(&p.eps),
    };
    let solved =
        solve_lossy_cavity_modes::<B>(&p.tagged.mesh, &materials, &p.pec_mask, &settings, &device)?;
    Ok(Solved {
        modes: solved
            .modes
            .into_iter()
            .map(|m| SolvedMode {
                lambda: m.lambda,
                k0: m.k0,
                residual_rel: m.residual_rel,
                vector: m.vector,
            })
            .collect(),
        n_null_filtered: solved.n_null_filtered,
        n_overdamped_filtered: Some(solved.n_overdamped_filtered),
        complex: true,
        upml_reference_k0,
    })
}

/// Load, solve and report. With `outdir`, also export each mode's field.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
) -> Result<EigenReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Eigen))?;
    let out = OutDir::create_opt(outdir)?;
    let target = p.eigen.expect("eigen spec has a resolved eigen section");

    let t0 = Instant::now();
    let solved = if p.has_complex_materials() {
        solve_lossy(&p, &target)?
    } else {
        solve_lossless(&p, &target)?
    };
    let wall_time_s = t0.elapsed().as_secs_f64();

    // k₀ [rad / mesh unit] → f [Hz]: f = Re(k₀) c / (2π L_unit).
    let k0_to_hz = C_M_PER_S / (2.0 * std::f64::consts::PI * p.length_unit_m());
    let eps_nodes = out
        .as_ref()
        .map(|_| eps_per_node(&p.tagged.mesh, &p.eps))
        .unwrap_or_default();
    let finite = |z: c64| z.re.is_finite() && z.im.is_finite();
    let mut modes = Vec::with_capacity(solved.modes.len());
    for (index, m) in solved.modes.iter().enumerate() {
        if !(finite(m.lambda) && m.lambda.re > 0.0 && m.residual_rel.is_finite()) {
            return Err(CliError::NonFinite {
                index,
                what: format!(
                    "mode λ = {} {:+}j, residual_rel = {}",
                    m.lambda.re, m.lambda.im, m.residual_rel
                ),
            });
        }
        let frequency_hz = m.k0.re * k0_to_hz;
        let field_file = match &out {
            Some(out) => {
                if m.vector.iter().any(|x| !finite(*x)) {
                    return Err(CliError::NonFinite {
                        index,
                        what: "--outdir export: non-finite eigenvector entry".into(),
                    });
                }
                let e_edges = scatter_interior(&p.pec_mask, &m.vector);
                Some(out.write_field(
                    &mode_file_name(index),
                    &p.tagged.mesh,
                    &e_edges,
                    solved.complex,
                    &eps_nodes,
                )?)
            }
            None => None,
        };
        // Q = Re k / (2|Im k|); +∞ (→ null) for a numerically lossless mode.
        let q = solved
            .complex
            .then(|| geode_util::eigen::q_factor(m.k0))
            .filter(|q| q.is_finite());
        modes.push(ModeResult {
            index,
            lambda: m.lambda.re,
            k0: m.k0.re,
            lambda_im: solved.complex.then_some(m.lambda.im),
            k0_im: solved.complex.then_some(m.k0.im),
            frequency_hz,
            omega_rad_s: 2.0 * std::f64::consts::PI * frequency_hz,
            q,
            residual_rel: m.residual_rel,
            field_file,
        });
    }

    Ok(EigenReport {
        provenance,
        kind: "eigen",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        pec: pec_summaries(&p),
        eigen: eigen_settings_summary(&p).expect("eigen spec"),
        absorbing_regions: upml_summaries(&p),
        solver: EigenSolverStats {
            method: "shift_invert_lanczos",
            inner: "direct_lu",
            n_null_filtered: solved.n_null_filtered,
            n_overdamped_filtered: solved.n_overdamped_filtered,
            residual_rel_max: modes.iter().map(|m| m.residual_rel).fold(0.0, f64::max),
            wall_time_s,
            pencil: if solved.complex {
                "complex_symmetric"
            } else {
                "real_symmetric"
            },
            upml_reference_k0: solved.upml_reference_k0,
        },
        modes,
    })
}

/// Interior-edge vector (the `true` entries of `mask`, in global edge
/// order) → full-length complex edge vector with PEC edges zeroed.
fn scatter_interior(mask: &[bool], interior: &[c64]) -> Vec<c64> {
    let mut it = interior.iter();
    let full: Vec<c64> = mask
        .iter()
        .map(|&keep| {
            if keep {
                *it.next().expect("interior vector too short")
            } else {
                c64::new(0.0, 0.0)
            }
        })
        .collect();
    assert!(it.next().is_none(), "interior vector too long");
    full
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scatter_interior_zeroes_pec_edges() {
        let full = scatter_interior(
            &[true, false, true, false],
            &[c64::new(2.0, 0.5), c64::new(-3.0, 0.0)],
        );
        let re: Vec<f64> = full.iter().map(|z| z.re).collect();
        let im: Vec<f64> = full.iter().map(|z| z.im).collect();
        assert_eq!(re, [2.0, 0.0, -3.0, 0.0]);
        assert_eq!(im, [0.5, 0.0, 0.0, 0.0]);
    }
}
