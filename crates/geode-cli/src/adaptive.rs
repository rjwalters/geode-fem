//! Adaptive mesh refinement for `driven` and `eigen` specs (issue #1267,
//! Epic #835 Phase 6).
//!
//! [`load`] is the single entry point both subcommands use instead of
//! [`problem::load`]. Without an `adaptive_mesh` section it is
//! `problem::load`. With one it
//!
//! 1. loads the spec as written (validation, boundary and material
//!    resolution),
//! 2. runs [`geode_core::adapt::driver::adapt_driven`] / `adapt_eigen` on
//!    that problem,
//! 3. writes the final mesh as MSH 4.1 ([`write_tagged_tet_mesh`]) and
//!    loads the spec again on it with `adaptive_mesh` removed,
//!
//! so the report's S-parameters / modes are, by construction, those of an
//! ordinary run on the exported mesh (the writer round-trips exactly).
//! Nothing here re-implements the loop (Phase 3, #868).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::adapt::driver::{
    AdaptError, AdaptOptions, AdaptReport, DrivenAdaptSpec, EigenAdaptSpec, FaceBc, FaceCtx,
    LevelQuantities, LumpedPortSpec, Marking, StopReason, TetCtx, adapt_driven, adapt_eigen,
};
use geode_core::constants::{C_M_PER_S, ETA_0_OHM};
use geode_core::driven::solve::SurfaceImpedanceModel;
use geode_core::mesh::write_tagged_tet_mesh;
use sha2::{Digest, Sha256};

use crate::backend::CompiledBackend;
use crate::driven::{impedance_walls, solver_mode};
use crate::error::CliError;
use crate::problem::{self, Problem, to_frequency};
use crate::report::{AdaptiveIteration, AdaptiveMeshReport, FileRef};
use crate::spec::{AdaptiveGoal, AdaptiveMeshSpec, Analysis};

type B = CompiledBackend;

/// Load the spec at `spec_path` for `analysis`; run the adaptive loop first
/// when the spec has an `adaptive_mesh` section.
pub fn load(
    spec_path: &Path,
    analysis: Analysis,
) -> Result<(Problem, Option<AdaptiveMeshReport>), CliError> {
    let spec = problem::read_spec(spec_path)?;
    let spec_dir = spec_path.parent().unwrap_or_else(|| Path::new("."));
    let Some(am) = spec.adaptive_mesh.clone() else {
        return Ok((problem::load_parsed(spec, spec_dir, Some(analysis))?, None));
    };
    // The spec as written: validates `adaptive_mesh` against the analysis
    // before the mesh is read.
    let p0 = problem::load_parsed(spec.clone(), spec_dir, Some(analysis))?;
    p0.print_load_warnings();

    let opts = AdaptOptions {
        target_rel_error: am.target_error,
        max_iterations: am.max_iterations,
        max_dofs: am.max_dofs,
        marking: Marking::Dorfler {
            theta: am.marking_fraction,
        },
        ..AdaptOptions::default()
    };
    let (report, final_mesh, goal_name, goal_of) = match analysis {
        Analysis::Driven => {
            let r = run_driven(&p0, &am, &opts)?;
            (r.report, r.mesh, "energy", GoalValue::EtaRel)
        }
        Analysis::Eigen => {
            let r = run_eigen(&p0, &am, &opts)?;
            let goal = match am.goal {
                AdaptiveGoal::Eigenfrequency => GoalValue::EigenfrequencyHz {
                    length_unit_m: p0.length_unit_m(),
                },
                _ => GoalValue::EtaRel,
            };
            let name = match am.goal {
                AdaptiveGoal::Eigenfrequency => "eigenfrequency",
                _ => "energy",
            };
            (r.report, r.mesh, name, goal)
        }
        other => unreachable!("validated: adaptive_mesh on a `{}` spec", other.name()),
    };

    // Write the final mesh: to `export_mesh`, else to a private temp file
    // that is removed once the problem is loaded from it.
    let text = write_tagged_tet_mesh(&final_mesh).map_err(|err| CliError::Mesh {
        path: PathBuf::from("<adaptive final mesh>"),
        err,
    })?;
    let (mesh_file, mesh_path, temp) = match &am.export_mesh {
        Some(rel) => {
            let path = if rel.is_absolute() {
                rel.clone()
            } else {
                spec_dir.join(rel)
            };
            std::fs::write(&path, &text).map_err(|err| CliError::Io {
                path: path.clone(),
                err,
            })?;
            let file = FileRef {
                path: rel.display().to_string(),
                sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
            };
            (
                Some(file),
                std::path::absolute(&path).unwrap_or(path),
                false,
            )
        }
        None => {
            let path = std::env::temp_dir().join(format!(
                "geode-adaptive-{}-{:x}.msh",
                std::process::id(),
                Sha256::digest(text.as_bytes())
                    .iter()
                    .take(6)
                    .fold(0u64, |a, b| (a << 8) | u64::from(*b))
            ));
            std::fs::write(&path, &text).map_err(|err| CliError::Io {
                path: path.clone(),
                err,
            })?;
            (None, path, true)
        }
    };

    let mut final_spec = spec;
    final_spec.adaptive_mesh = None;
    final_spec.mesh.path = mesh_path.clone();
    let loaded = problem::load_parsed(final_spec, spec_dir, Some(analysis));
    if temp {
        let _ = std::fs::remove_file(&mesh_path);
    }
    let mut p = loaded?;
    if temp {
        p.mesh_path = PathBuf::from(format!(
            "<adaptive final mesh of {}>",
            p0.mesh_path.display()
        ));
    }
    // Keep the section on the resolved problem so `Problem::spec` still
    // describes what the user asked for.
    p.spec.adaptive_mesh = Some(am);

    let mut out = build_report(&report, goal_name, goal_of, &p);
    out.mesh_file = mesh_file;
    for w in &out.warnings {
        eprintln!("warning: adaptive_mesh: {w}");
    }
    Ok((p, Some(out)))
}

fn run_driven(
    p: &Problem,
    am: &AdaptiveMeshSpec,
    opts: &AdaptOptions,
) -> Result<geode_core::adapt::driver::DrivenAdaptResult, CliError> {
    let lu = p.length_unit_m();
    let omegas: Vec<f64> = match &am.adapt_frequencies {
        Some(f) => f
            .expand()
            .map_err(CliError::InvalidSpec)?
            .into_iter()
            .map(|v| to_frequency(v, f.unit, lu).k0)
            .collect(),
        None => p.frequencies.iter().map(|f| f.k0).collect(),
    };
    let eps_by_tag: BTreeMap<i32, c64> = p.regions.iter().map(|r| (r.tag, r.eps_r)).collect();
    let eps = move |t: &TetCtx| {
        eps_by_tag
            .get(&t.tag)
            .copied()
            .unwrap_or(c64::new(1.0, 0.0))
    };

    // Face classification by physical tag. Impedance walls are indexed
    // like `impedance_walls` (Leontovich, then Silver-Müller).
    let mut by_tag: BTreeMap<i32, FaceBc> = BTreeMap::new();
    for s in &p.pec {
        by_tag.insert(s.tag, FaceBc::Pec);
    }
    let n_leont = p.leontovich.len();
    for (i, l) in p.leontovich.iter().enumerate() {
        by_tag.insert(l.surface.tag, FaceBc::Impedance(i));
    }
    for (j, s) in p.silver_muller.iter().enumerate() {
        by_tag.insert(s.tag, FaceBc::Impedance(n_leont + j));
    }
    for (i, port) in p.ports.iter().enumerate() {
        by_tag.insert(port.surface.tag, FaceBc::LumpedPort(i));
    }
    let boundary = move |f: &FaceCtx| {
        f.tag
            .and_then(|t| by_tag.get(&t).copied())
            .unwrap_or(FaceBc::Natural)
    };
    let zero_j = |_: &TetCtx, _: [f64; 3]| [c64::new(0.0, 0.0); 3];
    let zero_div = |_: &TetCtx, _: [f64; 3]| c64::new(0.0, 0.0);

    let mut spec = DrivenAdaptSpec::new(
        p.tagged.clone(),
        omegas.first().copied().unwrap_or(0.0),
        &eps,
        &boundary,
        &zero_j,
        &zero_div,
    );
    spec.omegas = omegas;
    spec.surfaces = impedance_walls(p)
        .into_iter()
        .map(|w| -> SurfaceImpedanceModel { w.model })
        .collect();
    spec.lumped_ports = p
        .ports
        .iter()
        .map(|port| LumpedPortSpec {
            e_hat: port.e_hat,
            resistance: port.resistance_ohm / ETA_0_OHM,
            width: port.width,
            length: port.length,
            v_inc: port.v_inc,
        })
        .collect();
    spec.solver = solver_mode(p);
    let device = <B as BackendTypes>::Device::default();
    adapt_driven::<B>(&spec, opts, &device).map_err(adapt_error)
}

fn run_eigen(
    p: &Problem,
    am: &AdaptiveMeshSpec,
    opts: &AdaptOptions,
) -> Result<geode_core::adapt::driver::EigenAdaptResult, CliError> {
    let _ = am;
    let target = p.eigen.expect("eigen spec has a resolved eigen section");
    let eps_by_tag: BTreeMap<i32, f64> = p.regions.iter().map(|r| (r.tag, r.eps_r.re)).collect();
    let eps = move |t: &TetCtx| eps_by_tag.get(&t.tag).copied().unwrap_or(1.0);
    let pec_tags: Vec<i32> = p.pec.iter().map(|s| s.tag).collect();
    let boundary = move |f: &FaceCtx| match f.tag {
        Some(t) if pec_tags.contains(&t) => FaceBc::Pec,
        _ => FaceBc::Natural,
    };
    let mut spec = EigenAdaptSpec::new(
        p.tagged.clone(),
        target.sigma(),
        target.n_modes,
        &eps,
        &boundary,
    );
    spec.settings.max_iters = target.max_iters;
    spec.settings.tol = target.tol;
    spec.settings.residual_tol = target.residual_tol;
    let device = <B as BackendTypes>::Device::default();
    adapt_eigen::<B>(&spec, opts, &device).map_err(adapt_error)
}

fn adapt_error(e: AdaptError) -> CliError {
    match e {
        AdaptError::AtLevel { source, .. } => adapt_error(*source),
        AdaptError::InvalidOptions(_)
        | AdaptError::InvalidSpec(_)
        | AdaptError::Unsupported(_)
        | AdaptError::InitialMeshOverBudget { .. } => {
            CliError::InvalidSpec(format!("adaptive_mesh: {e}"))
        }
        other => CliError::AdaptiveMesh(other.to_string()),
    }
}

/// How an iteration's `goal_value` is read off the loop's level data.
#[derive(Clone, Copy)]
enum GoalValue {
    EtaRel,
    EigenfrequencyHz { length_unit_m: f64 },
}

fn build_report(
    r: &AdaptReport,
    goal: &'static str,
    goal_of: GoalValue,
    final_p: &Problem,
) -> AdaptiveMeshReport {
    let history: Vec<AdaptiveIteration> = r
        .history
        .iter()
        .map(|it| {
            let goal_value = match (goal_of, &it.quantities) {
                (GoalValue::EigenfrequencyHz { length_unit_m }, LevelQuantities::Eigen(modes)) => {
                    modes.first().map_or(f64::NAN, |m| {
                        m.lambda.max(0.0).sqrt() * C_M_PER_S
                            / (2.0 * std::f64::consts::PI * length_unit_m)
                    })
                }
                _ => it.eta_rel,
            };
            AdaptiveIteration {
                level: it.level,
                n_tets: it.n_tets,
                n_dofs: it.n_dofs,
                estimate: it.eta_rel,
                goal_value,
                marked_fraction: it.refinement.as_ref().map(|s| s.marked_fraction),
                min_dihedral_deg: it.min_dihedral_deg,
                pre_asymptotic: it.pre_asymptotic,
                wall_s: it.wall.total_s,
            }
        })
        .collect();
    let mut warnings: Vec<String> = Vec::new();
    if r.stop_reason != StopReason::TargetMet {
        let last = r.last();
        warnings.push(format!(
            "target_error {} not met: stopped with stop_reason `{}` at estimate {:.3e} \
             ({} tets, {} DOFs)",
            r.options.target_rel_error,
            r.stop_reason.name(),
            last.eta_rel,
            last.n_tets,
            last.n_dofs
        ));
    }
    warnings.extend(r.warnings.iter().map(ToString::to_string));
    AdaptiveMeshReport {
        goal,
        history,
        stop_reason: r.stop_reason.name(),
        final_n_tets: final_p.tagged.mesh.n_tets(),
        final_n_dofs: final_p.edges.len(),
        mesh_file: None,
        warnings,
    }
}
