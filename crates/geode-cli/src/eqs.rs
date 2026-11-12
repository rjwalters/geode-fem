//! `geode eqs`: electro-quasi-static transient or small-signal admittance
//! between named terminals, with optional floating conductors (issue
//! #1494).
//!
//! Wraps [`geode_core::assembly::eqs_transient`]: per-tet `eps` (from
//! `materials[].eps_r`) and `sigma` (`materials[].conductivity_s_m`),
//! terminals as pinned node sets, floating conductors as equipotential
//! unknowns. Exactly one mode per spec: `transient` (θ-method response to
//! the terminal waveforms) or `frequencies` (`Y = G + jωC`).
//!
//! Units: the library integrates in mesh units with `ε` in F/m and `σ` in
//! S/m, so `C` and `K` — hence every current, `G`, `C` and `Y` — carry one
//! factor of the mesh length unit `L` (multiplied back in here). Time and
//! `dt` are in seconds and need no scaling; potentials are in volts; the
//! region-mean `E` scales by `1/L`.

use std::path::Path;
use std::time::Instant;

use geode_core::assembly::eqs_transient::{
    EqsFloating, EqsOptions, EqsRegion, EqsTerminal, assemble_eqs, eqs_admittance,
};

use crate::check::{mesh_summary, region_summaries};
use crate::error::CliError;
use crate::problem::{self, Conductor, EqsMode};
use crate::report::{
    ConductorSummary, EqsAdmittanceReport, EqsReport, EqsSettingsSummary, EqsSolverStats,
    EqsStepReport, EqsTransientReport, Provenance,
};
use crate::spec::Analysis;

fn summaries(list: &[Conductor]) -> Vec<ConductorSummary> {
    list.iter()
        .map(|k| ConductorSummary {
            physical_group: k.surface.name.clone(),
            tag: k.surface.tag,
            n_triangles: k.surface.triangles.len(),
            n_nodes: k.nodes.len(),
        })
        .collect()
}

fn scaled(m: &[Vec<f64>], scale: f64) -> Result<Vec<Vec<f64>>, CliError> {
    let out: Vec<Vec<f64>> = m
        .iter()
        .map(|row| row.iter().map(|x| x * scale).collect())
        .collect();
    for (i, row) in out.iter().enumerate() {
        if let Some(bad) = row.iter().find(|x| !x.is_finite()) {
            return Err(CliError::NonFinite {
                index: i,
                what: format!("admittance row {i} has a non-finite entry {bad}"),
            });
        }
    }
    Ok(out)
}

/// Trapezoid `∫ V I dt` over the reported steps, per terminal.
fn trapezoid(t: &[f64], v: &[Vec<f64>], i: &[Vec<f64>], terminal: usize) -> f64 {
    (1..t.len())
        .map(|k| {
            let p0 = v[k - 1][terminal] * i[k - 1][terminal];
            let p1 = v[k][terminal] * i[k][terminal];
            0.5 * (p0 + p1) * (t[k] - t[k - 1])
        })
        .sum()
}

/// Midpoint `∫ V I dt` for the displacement channel: the mean of the two
/// step-end voltages times the step's displacement charge increment
/// `I_disp[k] · Δt` (the solver defines `I_disp[k] = ΔQ_k / Δt` for any θ).
/// Unlike an end-of-step `V I` product, a closed lossless pulse nets to
/// round-off, including for discontinuous edges.
fn midpoint_disp(t: &[f64], v: &[Vec<f64>], i: &[Vec<f64>], terminal: usize) -> f64 {
    (1..t.len())
        .map(|k| {
            let vm = 0.5 * (v[k - 1][terminal] + v[k][terminal]);
            vm * i[k][terminal] * (t[k] - t[k - 1])
        })
        .sum()
}

/// Load, solve and report.
pub fn run(spec_path: &Path, provenance: Provenance) -> Result<EqsReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Eqs))?;
    let target = p.eqs.as_ref().expect("eqs spec has a resolved eqs section");
    let mesh = &p.tagged.mesh;
    let scale = p.length_unit_m();

    let terminals: Vec<EqsTerminal> = target
        .terminals
        .iter()
        .map(|c| EqsTerminal {
            name: c.surface.name.clone(),
            nodes: c.nodes.clone(),
        })
        .collect();
    let floating: Vec<EqsFloating> = target
        .floating
        .iter()
        .map(|c| EqsFloating {
            name: c.surface.name.clone(),
            nodes: c.nodes.clone(),
        })
        .collect();

    let t0 = Instant::now();
    let (transient, admittance, method) = match &target.mode {
        EqsMode::Transient { dt, n_steps, theta } => {
            let sys = assemble_eqs(
                mesh,
                &target.eps_f_m,
                &target.sigma_s_m,
                &terminals,
                &floating,
                EqsOptions {
                    dt: *dt,
                    theta: *theta,
                },
            )?;
            let waveforms: Vec<&crate::spec::WaveformSpec> = target
                .waveforms
                .iter()
                .map(|w| w.as_ref().expect("transient terminals have waveforms"))
                .collect();
            let drive = |t: f64| -> Vec<f64> { waveforms.iter().map(|w| w.eval(t)).collect() };
            let regions: Vec<EqsRegion> = target
                .regions
                .iter()
                .filter(|r| !r.tets.is_empty())
                .map(|r| EqsRegion {
                    name: r.name.clone(),
                    tets: r.tets.clone(),
                })
                .collect();
            let hist = sys.run(mesh, &drive, *n_steps, &regions)?;
            let steps: Vec<EqsStepReport> = hist
                .steps
                .iter()
                .map(|s| EqsStepReport {
                    t_s: s.t,
                    v: s.v.clone(),
                    floating_phi_v: s.floating_phi.clone(),
                    i_cond_a: s.i_cond.iter().map(|x| x * scale).collect(),
                    i_disp_a: s.i_disp.iter().map(|x| x * scale).collect(),
                    floating_current_a: s.floating_current.iter().map(|x| x * scale).collect(),
                    region_e_v_m: s
                        .region_e
                        .iter()
                        .map(|e| [e[0] / scale, e[1] / scale, e[2] / scale])
                        .collect(),
                    charge_residual: s.charge_residual,
                })
                .collect();
            let finite = |x: &f64| x.is_finite();
            for (k, s) in steps.iter().enumerate() {
                let ok = s.i_cond_a.iter().all(finite)
                    && s.i_disp_a.iter().all(finite)
                    && s.floating_phi_v.iter().all(finite);
                if !ok {
                    return Err(CliError::NonFinite {
                        index: k,
                        what: format!("transient step {k} has a non-finite value"),
                    });
                }
            }
            let t: Vec<f64> = steps.iter().map(|s| s.t_s).collect();
            let v: Vec<Vec<f64>> = steps.iter().map(|s| s.v.clone()).collect();
            let ic: Vec<Vec<f64>> = steps.iter().map(|s| s.i_cond_a.clone()).collect();
            let id: Vec<Vec<f64>> = steps.iter().map(|s| s.i_disp_a.clone()).collect();
            let nt = terminals.len();
            let report = EqsTransientReport {
                dt_s: *dt,
                n_steps: *n_steps,
                theta: *theta,
                regions: hist.region_names.clone(),
                delivered_energy_cond_j: (0..nt).map(|i| trapezoid(&t, &v, &ic, i)).collect(),
                delivered_energy_disp_j: (0..nt).map(|i| midpoint_disp(&t, &v, &id, i)).collect(),
                energy_quadrature: "cond:trapezoid,disp:midpoint",
                steps,
            };
            (Some(report), None, "theta_method")
        }
        EqsMode::Frequencies(freqs) => {
            let mut out = Vec::with_capacity(freqs.len());
            for &f in freqs {
                let omega = 2.0 * std::f64::consts::PI * f;
                let y = eqs_admittance(
                    mesh,
                    &target.eps_f_m,
                    &target.sigma_s_m,
                    &terminals,
                    &floating,
                    omega,
                )?;
                let g = scaled(&y.y_re, scale)?;
                let y_im = scaled(&y.y_im, scale)?;
                let c_farad = (omega > 0.0).then(|| {
                    y_im.iter()
                        .map(|row| row.iter().map(|x| x / omega).collect())
                        .collect()
                });
                out.push(EqsAdmittanceReport {
                    frequency_hz: f,
                    omega_rad_s: omega,
                    g_siemens: g,
                    y_im_siemens: y_im,
                    c_farad,
                });
            }
            (None, Some(out), "complex_block")
        }
    };
    let wall_time_s = t0.elapsed().as_secs_f64();

    Ok(EqsReport {
        provenance,
        kind: "eqs",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        eqs: EqsSettingsSummary {
            terminals: summaries(&target.terminals),
            floating: summaries(&target.floating),
            mode: if transient.is_some() {
                "transient"
            } else {
                "admittance"
            },
            element: "p1_tet",
            n_dof: mesh.n_nodes(),
        },
        terminals: target
            .terminals
            .iter()
            .map(|c| c.surface.name.clone())
            .collect(),
        floating: target
            .floating
            .iter()
            .map(|c| c.surface.name.clone())
            .collect(),
        transient,
        admittance,
        solver: EqsSolverStats {
            method,
            inner: "direct_lu",
            wall_time_s,
        },
    })
}
