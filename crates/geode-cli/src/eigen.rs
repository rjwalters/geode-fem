//! `geode eigen`: lossless PEC-cavity eigenmodes → resonant frequencies.
//!
//! Wraps [`geode_core::eigen::pec_cavity::solve_pec_cavity_modes`]: the
//! interior-reduced first-order Nédélec pencil `K x = k₀² M_ε x` (real
//! `ε_r` per volume region, PEC surfaces eliminated edge-exactly from
//! their tagged triangles) is assembled sparse and solved with the
//! pure-Rust sparse shift-invert Lanczos around `σ = k₀,shift²`. The
//! pencil is lossless, so every mode's `Q` is reported as `null`.
//!
//! With `--outdir` (issue #684) each mode's eigenvector — real,
//! `M_ε`-normalized, over the interior edges — is scattered back to the
//! full edge vector (PEC edges = 0) and written as `E_mode_<index>.vtu`
//! ([`crate::export`]). An eigenvector's overall sign is arbitrary.

use std::path::Path;
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::constants::C_M_PER_S;
use geode_core::eigen::pec_cavity::{PecCavitySettings, solve_pec_cavity_modes};

use crate::backend::CompiledBackend;
use crate::check::{eigen_settings_summary, mesh_summary, pec_summaries, region_summaries};
use crate::error::CliError;
use crate::export::{OutDir, eps_per_node, mode_file_name};
use crate::problem;
use crate::report::{EigenReport, EigenSolverStats, ModeResult, Provenance};
use crate::spec::Analysis;

/// Load, solve and report. With `outdir`, also export each mode's field.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
) -> Result<EigenReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Eigen))?;
    let out = OutDir::create_opt(outdir)?;
    let target = p.eigen.expect("eigen spec has a resolved eigen section");

    // `problem::load` rejected Im(ε_r) ≠ 0 for eigen specs.
    let eps_r: Vec<f64> = p.eps.iter().map(|e| e.re).collect();
    let settings = PecCavitySettings {
        max_iters: target.max_iters,
        tol: target.tol,
        residual_tol: target.residual_tol,
        ..PecCavitySettings::new(target.sigma(), target.n_modes)
    };

    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let t0 = Instant::now();
    let solved =
        solve_pec_cavity_modes::<B>(&p.tagged.mesh, &eps_r, &p.pec_mask, &settings, &device)?;
    let wall_time_s = t0.elapsed().as_secs_f64();

    // k₀ [rad / mesh unit] → f [Hz]: f = k₀ c / (2π L_unit).
    let k0_to_hz = C_M_PER_S / (2.0 * std::f64::consts::PI * p.length_unit_m());
    let eps_nodes = out
        .as_ref()
        .map(|_| eps_per_node(&p.tagged.mesh, &p.eps))
        .unwrap_or_default();
    let mut modes = Vec::with_capacity(solved.modes.len());
    for (index, m) in solved.modes.iter().enumerate() {
        if !(m.lambda.is_finite() && m.lambda > 0.0 && m.residual_rel.is_finite()) {
            return Err(CliError::NonFinite {
                index,
                what: format!("mode λ = {}, residual_rel = {}", m.lambda, m.residual_rel),
            });
        }
        let frequency_hz = m.k0 * k0_to_hz;
        let field_file = match &out {
            Some(out) => {
                if m.vector.iter().any(|x| !x.is_finite()) {
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
                    false,
                    &eps_nodes,
                )?)
            }
            None => None,
        };
        modes.push(ModeResult {
            index,
            lambda: m.lambda,
            k0: m.k0,
            frequency_hz,
            omega_rad_s: 2.0 * std::f64::consts::PI * frequency_hz,
            q: None,
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
        solver: EigenSolverStats {
            method: "shift_invert_lanczos",
            inner: "direct_lu",
            n_null_filtered: solved.n_null_filtered,
            residual_rel_max: modes.iter().map(|m| m.residual_rel).fold(0.0, f64::max),
            wall_time_s,
        },
        modes,
    })
}

/// Interior-edge vector (the `true` entries of `mask`, in global edge
/// order) → full-length complex edge vector with PEC edges zeroed.
fn scatter_interior(mask: &[bool], interior: &[f64]) -> Vec<c64> {
    let mut it = interior.iter();
    let full: Vec<c64> = mask
        .iter()
        .map(|&keep| {
            if keep {
                c64::new(*it.next().expect("interior vector too short"), 0.0)
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
        let full = scatter_interior(&[true, false, true, false], &[2.0, -3.0]);
        let re: Vec<f64> = full.iter().map(|z| z.re).collect();
        assert_eq!(re, [2.0, 0.0, -3.0, 0.0]);
        assert!(full.iter().all(|z| z.im == 0.0));
    }
}
