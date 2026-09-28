//! `geode extract`: driven sweep → per-port L / R / Q, the quasi-static
//! `L₀` (f → 0 Richardson extrapolation) and the self-resonant frequency.
//!
//! The solve is exactly `geode driven`'s ([`crate::driven::sweep`]) over
//! the extract spec's solved frequency list (the ascending union of
//! `frequencies` and `extract.anchor_frequencies`). Per port `k`, from the
//! diagonal `Z_kk`:
//!
//! * `L₀` — [`geode_core::driven::extraction::extrapolate_l0`] on the
//!   `(f, L = Im Z_kk / ω)` samples at the anchor frequencies: two-point
//!   Richardson extrapolation of `L(f) ≈ L₀ − a·f²` on the two lowest
//!   anchors, plus a consistency estimate from the third-lowest. With
//!   `extract.l0_rel_tol` set, an estimate above the gate fails the run
//!   (`solve_failed`) instead of reporting an unconverged `L₀`.
//! * SRF — [`geode_core::driven::extraction::im_z_zero_crossings`] over
//!   the whole (ascending) sweep; the first crossing is the SRF.
//!
//! With `--outdir`, every report row also gets `geode driven`'s field /
//! far-field export ([`crate::driven`], issue #684).

use std::path::Path;

use faer::c64;
use geode_core::driven::extraction::{extrapolate_l0, im_z_zero_crossings};

use crate::check::{
    extract_settings_summary, mesh_summary, port_summaries, silver_muller_summaries, upml_summaries,
};
use crate::driven;
use crate::error::CliError;
use crate::export::OutDir;
use crate::problem;
use crate::report::{ExtractReport, PortExtraction, Provenance};
use crate::spec::Analysis;

/// Load, solve, extract and report.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
) -> Result<ExtractReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Extract))?;
    let out = OutDir::create_opt(outdir)?;
    let target = p
        .extract
        .clone()
        .expect("extract spec has a resolved extract section");
    let (results, solver) = driven::sweep(&p, out.as_ref())?;

    // `p.frequencies` (hence `results`) is strictly ascending for an
    // extract spec, as `im_z_zero_crossings` requires. The interpolation
    // is linear in its abscissa, so crossings come back directly in Hz.
    let freqs_hz: Vec<f64> = results.iter().map(|r| r.frequency_hz).collect();
    let mut extraction = Vec::with_capacity(p.ports.len());
    for k in 0..p.ports.len() {
        let points: Vec<(f64, f64)> = target
            .anchors
            .iter()
            .map(|&i| (results[i].frequency_hz, results[i].ports[k].l_h))
            .collect();
        let x = extrapolate_l0(&points)
            .filter(|x| x.l0.is_finite())
            .ok_or_else(|| CliError::NonFinite {
                index: target.anchors[0],
                what: format!("L0 extrapolation for port {k} from (f, L) = {points:?}"),
            })?;
        let rel = x.error_estimate.map(|e| e / x.l0.abs());
        if let Some(tol) = target.l0_rel_tol {
            // Validated: ≥ 3 distinct anchors, so the estimate exists.
            let r = rel.unwrap_or(f64::NAN);
            if r.is_nan() || r > tol {
                return Err(CliError::L0NotConverged {
                    port: k,
                    l0_h: x.l0,
                    anchors_hz: x.anchors,
                    check_hz: x.check_frequency.unwrap_or(f64::NAN),
                    rel: r,
                    tol,
                });
            }
        }
        let zs: Vec<c64> = results
            .iter()
            .map(|r| c64::new(r.ports[k].z_ohm[0], r.ports[k].z_ohm[1]))
            .collect();
        let crossings = im_z_zero_crossings(&freqs_hz, &zs);
        extraction.push(PortExtraction {
            index: k,
            l0_h: x.l0,
            l0_anchor_frequencies_hz: x.anchors,
            l0_error_estimate_h: x.error_estimate,
            l0_error_estimate_rel: rel,
            l0_check_frequency_hz: x.check_frequency,
            srf_hz: crossings.first().copied(),
            im_z_zero_crossings_hz: crossings,
        });
    }

    Ok(ExtractReport {
        provenance,
        kind: "extract",
        status: "ok",
        mesh: mesh_summary(&p),
        ports: port_summaries(&p),
        silver_muller: silver_muller_summaries(&p),
        absorbing_regions: upml_summaries(&p),
        extract: extract_settings_summary(&p).expect("extract spec"),
        solver,
        results,
        extraction,
    })
}
