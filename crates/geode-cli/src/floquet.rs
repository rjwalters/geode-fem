//! Floquet-port driven unit cell (Epic #837 Phase 4b): `floquet_ports` +
//! `incidence` / `scan` on a periodic spec.
//!
//! The spec maps onto core's [`FloquetCell`]; nothing about diffraction
//! orders, channel sets or the `propagating` flag is re-derived here. Per
//! incidence angle the cell is solved at every frequency; the report holds
//! one row per (incident polarization, propagating output channel), and
//! `--touchstone` writes one file per angle with the channels named
//! `P<port>_<m>_<n>_<TE|TM>` (port 1-based).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::floquet::{
    FloquetCell, FloquetCellSpec, FloquetError, FloquetIncidence, FloquetLabel,
    FloquetPolarization, FloquetPortSpec, FloquetSettings, FloquetSolution,
};

use crate::backend::CompiledBackend;
use crate::check::mesh_summary;
use crate::error::CliError;
use crate::export::file_ref_at;
use crate::problem::{FloquetSetup, Problem};
use crate::progress::SweepOptions;
use crate::report::{
    DrivenReport, FileRef, FloquetAngleResult, FloquetPoint, FloquetPortSummary, FloquetReport,
    FloquetRow, Provenance, SolverStats, WarningResult,
};

fn pol_name(p: FloquetPolarization) -> &'static str {
    match p {
        FloquetPolarization::Te => "TE",
        FloquetPolarization::Tm => "TM",
    }
}

/// The Touchstone channel name of a label: `P<port>_<m>_<n>_<TE|TM>`
/// (port numbered from 1; `m`, `n` signed).
pub fn channel_name(l: &FloquetLabel) -> String {
    format!("P{}_{}_{}_{}", l.0 + 1, l.1.m, l.1.n, pol_name(l.2))
}

fn map_err(e: FloquetError) -> CliError {
    match e {
        FloquetError::InvalidSpec(m) => CliError::InvalidSpec(m),
        other => CliError::FloquetSolve(other.to_string()),
    }
}

/// Assemble the unit cell of `p` (shared by `geode driven` and the
/// `geode check` validity pass).
pub fn assemble(p: &Problem, setup: &FloquetSetup) -> Result<FloquetCell, CliError> {
    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let periodic = p
        .periodic
        .as_ref()
        .ok_or_else(|| CliError::InvalidSpec("no `boundary_conditions.periodic` pairs".into()))?;
    let spec = FloquetCellSpec {
        eps_r: &p.eps,
        sigma: p.sigma(),
        pec_interior_mask: (!p.pec.is_empty()).then_some(p.pec_mask.as_slice()),
        ports: [0, 1].map(|k| FloquetPortSpec {
            triangles: &setup.ports[k].surface.triangles,
            eps_r: setup.ports[k].eps_r,
        }),
        settings: FloquetSettings {
            n_evanescent: setup.ports[0].n_evanescent.max(setup.ports[1].n_evanescent),
            ..FloquetSettings::default()
        },
    };
    FloquetCell::assemble::<B>(&p.tagged.mesh, &periodic.map, &spec, &device).map_err(map_err)
}

fn warning(message: String) -> WarningResult {
    eprintln!("warning: {message}");
    WarningResult {
        kind: "floquet",
        wave_port: None,
        physical_group: None,
        message,
    }
}

/// The per-angle Touchstone path: `<stem>_theta<θ>_phi<φ>.s<N>p` next to
/// the `--touchstone` argument (whose own extension is replaced: the port
/// count `N` is only known after the solve and may differ per angle).
fn angle_path(base: &Path, theta: f64, phi: f64, n_ports: usize) -> PathBuf {
    let num = |x: f64| {
        let s = format!("{x:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let stem = base
        .file_stem()
        .map_or_else(|| "floquet".into(), |s| s.to_string_lossy().into_owned());
    base.with_file_name(format!(
        "{stem}_theta{}_phi{}.s{n_ports}p",
        num(theta),
        num(phi)
    ))
}

fn write_angle_touchstone(
    base: &Path,
    provenance: &Provenance,
    theta: f64,
    phi: f64,
    sols: &[(f64, FloquetSolution)],
) -> Result<FileRef, CliError> {
    // Fixed shape: the union of the labels that propagate anywhere in the
    // sweep; an order that is not yet propagating at a frequency is zero.
    let labels: Vec<FloquetLabel> = sols
        .iter()
        .flat_map(|(_, s)| s.propagating_labels())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let rows: Vec<crate::touchstone::Row> = sols
        .iter()
        .map(|(hz, s)| {
            let m = s
                .s_on_labels(&labels)
                .into_iter()
                .map(|r| r.into_iter().map(|z| [z.re, z.im]).collect())
                .collect();
            (*hz, m)
        })
        .collect();
    let mut comments = vec![
        format!(
            "Touchstone 2.0 written by geode {} ({})",
            provenance.geode_version, provenance.git_sha
        ),
        format!("spec: {}", provenance.spec_path),
        format!("Floquet-port unit cell at theta = {theta} deg, phi = {phi} deg"),
        "modal power-normalized S over the propagating Floquet channels; [Reference] is nominal"
            .to_string(),
    ];
    for (i, l) in labels.iter().enumerate() {
        comments.push(format!("Port[{}] = {}", i + 1, channel_name(l)));
    }
    let refs = vec![50.0; labels.len()];
    let text = crate::touchstone::render_rows(&comments, &refs, rows)?;
    let path = angle_path(base, theta, phi, labels.len());
    std::fs::write(&path, text).map_err(|err| CliError::Io {
        path: path.clone(),
        err,
    })?;
    file_ref_at(&path, path.display().to_string())
}

/// Run the Floquet-port analysis of a loaded problem.
pub fn run(
    p: &Problem,
    provenance: Provenance,
    touchstone: Option<&Path>,
    opts: SweepOptions,
) -> Result<DrivenReport, CliError> {
    let setup = p.floquet.as_ref().expect("floquet problem");
    let mut warnings: Vec<WarningResult> = p.preconditioner_warnings();
    if p.spec.sweep.as_ref().is_some_and(|s| s.adaptive.is_some()) {
        warnings.push(warning(
            "sweep.adaptive: a fixed-angle sweep changes the Bloch phase k_t with frequency, so \
             the reduced-order model does not apply; running the dense sweep (every frequency \
             solved at full order)"
                .to_string(),
        ));
    }
    if opts.jobs > 1 {
        eprintln!("note: --jobs: Floquet-port sweeps run serially (one solve at a time)");
    }
    let t0 = Instant::now();
    let cell = assemble(p, setup)?;
    let mut angles = Vec::with_capacity(setup.angles.len());
    let mut residual_max = 0.0_f64;
    let mut done = 0_usize;
    for &[theta, phi] in &setup.angles {
        let inc = FloquetIncidence::degrees(theta, phi, 0);
        let mut sols: Vec<(f64, FloquetSolution)> = Vec::with_capacity(p.frequencies.len());
        let mut points = Vec::with_capacity(p.frequencies.len());
        for f in &p.frequencies {
            let sol = cell.solve_incidence(f.k0, &inc).map_err(map_err)?;
            if !sol.residual_rel.is_finite()
                || sol
                    .s
                    .iter()
                    .flatten()
                    .any(|z| !z.re.is_finite() || !z.im.is_finite())
            {
                return Err(CliError::NonFinite {
                    index: done,
                    what: format!("Floquet S at theta = {theta}, phi = {phi}, f = {} Hz", f.hz),
                });
            }
            residual_max = residual_max.max(sol.residual_rel);
            for w in &sol.warnings {
                warnings.push(warning(format!(
                    "theta = {theta} deg, phi = {phi} deg, f = {:e} Hz: {w}",
                    f.hz
                )));
            }
            let mut rows = Vec::new();
            for &pol in &setup.pols {
                let Some(j) = sol.specular_index(0, pol) else {
                    warnings.push(warning(format!(
                        "theta = {theta} deg, f = {:e} Hz: the specular {} order does not \
                         propagate at port 0 (total internal reflection of the port medium); no \
                         rows for this incident polarization",
                        f.hz,
                        pol_name(pol)
                    )));
                    continue;
                };
                for (i, &c) in sol.propagating.iter().enumerate() {
                    let ch = &sol.channels[c];
                    let s: c64 = sol.s[i][j];
                    rows.push(FloquetRow {
                        incident_pol: pol_name(pol),
                        port: ch.port,
                        m: ch.order.m,
                        n: ch.order.n,
                        pol: pol_name(ch.pol),
                        propagating: ch.propagating,
                        k_z: [ch.k_z.re, ch.k_z.im],
                        s: [s.re, s.im],
                        efficiency: s.re * s.re + s.im * s.im,
                    });
                }
            }
            points.push(FloquetPoint {
                frequency_hz: f.hz,
                k0: f.k0,
                residual_rel: sol.residual_rel,
                n_propagating: sol.propagating.len(),
                rows,
            });
            sols.push((f.hz, sol));
            done += 1;
        }
        let touchstone_file = touchstone
            .map(|base| write_angle_touchstone(base, &provenance, theta, phi, &sols))
            .transpose()?;
        angles.push(FloquetAngleResult {
            theta_deg: theta,
            phi_deg: phi,
            points,
            touchstone_file,
        });
    }
    let wall_time_s = t0.elapsed().as_secs_f64();
    let summary = crate::check::solver_summary(p.solver());
    Ok(DrivenReport {
        provenance,
        kind: "driven",
        status: "ok",
        mesh: mesh_summary(p),
        ports: Vec::new(),
        wave_ports: Vec::new(),
        silver_muller: Vec::new(),
        absorbing_regions: Vec::new(),
        solver: SolverStats {
            mode: summary.mode,
            tol: summary.tol,
            max_iters: summary.max_iters,
            preconditioner: summary.preconditioner,
            iterations_max: 0,
            residual_rel_max: residual_max,
            wall_time_s,
            jobs: opts.jobs_explicit.then_some(opts.jobs),
            adaptive: None,
        },
        results: Vec::new(),
        touchstone_file: None,
        sensitivities: None,
        warnings,
        adaptive_mesh: None,
        accuracy: None,
        floquet: Some(FloquetReport {
            ports: setup
                .ports
                .iter()
                .map(|d| FloquetPortSummary {
                    physical_group: d.surface.name.clone(),
                    eps_r: d.eps_r,
                    n_evanescent: d.n_evanescent,
                    n_triangles: d.surface.triangles.len(),
                })
                .collect(),
            incident_polarizations: setup.pols.iter().map(|&q| pol_name(q)).collect(),
            scan: setup.is_scan,
            angles,
        }),
    })
}
