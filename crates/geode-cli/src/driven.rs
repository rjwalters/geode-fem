//! `geode driven`: port-driven frequency sweep → Z / Y / S per frequency.
//!
//! The dense sweep is
//! [`geode_core::driven::extraction::s_parameter_frequency_sweep_with_mode`]
//! driven point by point: [`s_parameter_operator`] assembles the
//! ω-independent operator **once**, then [`s_parameter_point`] per
//! frequency factors (direct) or preconditions (iterative) `A(ω)` and
//! back-solves one right-hand side per port — up to `--jobs`
//! frequencies concurrently, rows in frequency order ([`crate::progress`],
//! issue #708). Lumped ports and Leontovich walls are composed into the
//! same operator. With a single port this is bit-for-bit the
//! [`geode_core::driven::extraction::driven_frequency_sweep`] path the
//! spiral-inductor benchmark uses.
//!
//! # Adaptive sweep (`sweep.adaptive`, issue #708)
//!
//! A block Galerkin reduced-order model ([`geode_core::driven::rom`],
//! [`RomDrive::PerPort`]) built from a few greedy full-order snapshot
//! solves; every other frequency is interpolated through it, and any
//! frequency still above the tolerance (or whose reduced solve is
//! singular) gets a full-order fallback solve.
//! Each report row says whether it was solved (`solved`), and
//! `solver.adaptive` carries the greedy diagnostics. `--outdir` exports
//! fields only for the solved rows.
//!
//! [`sweep`] (solve + Z / Y / S / per-port assembly) is shared with
//! `geode extract`, which post-processes the same results.
//!
//! # Open boundaries and wave ports (issue #683)
//!
//! * **Silver-Müller** walls join the Leontovich walls in the same
//!   surface-impedance list (`SurfaceImpedanceModel::Fixed(η₀ = 1)`).
//! * **Matched box-UPML** (`absorbing_regions`) makes the materials
//!   ω-dependent (the stretch carries `1/k₀`), which the batched
//!   assemble-once sweep cannot express. A UPML spec therefore runs the
//!   same library sweep **once per frequency** with that frequency's
//!   `DrivenMaterials::MatchedUpml` tensors (reassembling each time). A
//!   spec without UPML takes the batched path unchanged.
//! * **Dispersive materials** (`materials[].dispersion`, issue #757)
//!   take the same per-frequency path: each frequency's operator is
//!   assembled from [`Problem::eps_at`] (`ε_r(f)`; times the stretch with
//!   UPML), and each report row echoes the applied `ε_r(f)`.
//! * **Anisotropic materials** (`eps_r_diag` / `mu_r_diag`, issue #760)
//!   switch the operator to the same full-tensor (`MatchedUpml`)
//!   assembly with `ε = ε_r·Λ`, `ν = Λ⁻¹·μ_r⁻¹`
//!   ([`Problem::material_tensors`]; `Λ = I` outside a shell). They are
//!   ω-independent, so without UPML / dispersion the operator is still
//!   assembled once (and the adaptive sweep applies).
//! * **Wave ports** build each port's modes from its tagged face
//!   ([`geode_core::driven::ports::PortFaceProjection::wave_port`]) and
//!   run [`solve_wave_port_sweep_with_mode`] (per frequency with UPML,
//!   batched otherwise). The result is a power-normalized channel
//!   S-matrix; wave ports define no port impedance.
//!
//! # Material sensitivities (`sensitivity`, issue #739)
//!
//! With a `sensitivity` section (one lumped port, direct, dense, no UPML /
//! wave ports — enforced by `problem::load`), the report also carries
//! `∂|S11|²/∂ε_r` at every swept frequency from the port-loaded material
//! adjoint ([`crate::sensitivity::driven`]).
//!
//! # Field / far-field export (`--outdir`, issue #684)
//!
//! Only with `--outdir`, and only for **lumped-port** specs: the sweep
//! APIs return `Z` / `S` but not the fields (an N-port S-matrix is N
//! per-excitation solves), so [`Exporter::export_row`] does **one extra solve per
//! report row** on a [`DrivenOperator`] with every port driven at its
//! spec `v_inc` — the physical "driven as specified" state — and writes
//! its `E` as a `.vtu` ([`crate::export`]). With exactly one
//! `absorbing_regions` shell it also runs the NTFF
//! ([`geode_core::postproc::ntff`]) over the shell's inner wall shrunk
//! 10 % ([`NTFF_SHRINK`], `examples/patch_antenna`'s `FLUX_SHRINK`):
//! directivity, broadside gain and radiation efficiency.
//!
//! Wave-port specs export nothing: their physical field is a linear
//! combination of the per-channel SMW solves, which the library does not
//! return (deferred; see `crates/geode-cli/README.md`).

use std::path::Path;
use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::nedelec::tet_centroids;
use geode_core::constants::ETA_0_OHM;
use geode_core::driven::extraction::{
    SMatrix, SParameterSweepPoint, s_parameter_operator, s_parameter_point, z_from_port_readbacks,
};
use geode_core::driven::ports::{
    LumpedPort, WavePort, WavePortSweepPoint, solve_wave_port_sweep_with_mode,
};
use geode_core::driven::rom::{DrivenRom, RomDrive, RomError, RomExcitationPoint, RomSettings};
use geode_core::driven::scattering::flux_power_box;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, IterativeSettings, SolverMode,
    SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::postproc::ntff::{
    broadside_directivity, directivity, gain, ntff_far_field, principal_plane_cuts, to_db,
};

use crate::backend::CompiledBackend;
use crate::check::{mesh_summary, port_summaries, silver_muller_summaries, upml_summaries};
use crate::error::CliError;
use crate::export::{OutDir, PatternFile, eps_per_node, field_file_name, pattern_file_name};
use crate::problem::{self, Problem, UpmlRegion};
use crate::progress::{Progress, SweepOptions, par_map};
use crate::report::{
    AdaptiveSweepStats, Complex, DrivenReport, FarFieldResult, FrequencyResult, MaterialEpsResult,
    PortResult, Provenance, RoughnessKResult, SolverStats, WaveChannelResult, WaveModeSummary,
    WavePortSummary,
};
use crate::spec::{AdaptiveSweepSpec, Analysis, SolverSpec};
use serde_json::json;

/// NTFF / flux box = the UPML inner wall shrunk this fraction toward
/// its centre (`examples/patch_antenna`'s `FLUX_SHRINK`): keeps the
/// Huygens surface off the stretched-coordinate interface.
pub const NTFF_SHRINK: f64 = 0.10;
/// NTFF polar samples on `[0, π]` (2° steps; `examples/patch_antenna`).
pub const NTFF_N_THETA: usize = 91;
/// NTFF azimuth samples on `[0, 2π)` (5° steps; `examples/patch_antenna`).
pub const NTFF_N_PHI: usize = 72;

/// The NTFF box for the resolved `absorbing_regions` shells `upml` and
/// the mesh's tet `centroids`: `None` unless there is exactly one shell,
/// else that shell's inner
/// wall shrunk by [`NTFF_SHRINK`] — an [`CliError::InvalidSpec`] if no
/// centroid lies inside it. Only non-emptiness is checked: that the box
/// encloses the radiator and every port and lies entirely in air is the
/// spec author's responsibility (`crates/geode-cli/README.md`).
#[allow(clippy::type_complexity)]
fn ntff_box(
    upml: &[UpmlRegion],
    centroids: &[[f64; 3]],
) -> Result<Option<([f64; 3], [f64; 3])>, CliError> {
    let [u] = upml else {
        return Ok(None);
    };
    let (lo, hi) = shrink_box(u.air_lo, u.air_hi, NTFF_SHRINK);
    let in_box = |c: &[f64; 3]| (0..3).all(|k| c[k] >= lo[k] && c[k] <= hi[k]);
    if !centroids.iter().any(in_box) {
        return Err(CliError::InvalidSpec(format!(
            "--outdir far-field export: no tet centroid lies inside the NTFF box \
             {lo:?}–{hi:?} (absorbing region `{}` inner wall shrunk by {NTFF_SHRINK})",
            u.name
        )));
    }
    // Not every centroid is inside: the shell's stretched tets
    // (`n_tets_stretched ≥ 1`) lie beyond the inner wall.
    Ok(Some((lo, hi)))
}

/// Validate the `--outdir` export of `p` up front — right after
/// [`problem::load`], before the (expensive) sweep — so a spec whose
/// NTFF box holds no tet centroid fails in milliseconds rather than
/// after every frequency has been solved. Callers invoke it only when
/// `--outdir` was passed; [`Exporter`] re-derives the same box through
/// the same [`ntff_box`], so the two can never disagree.
pub fn validate_export(p: &Problem) -> Result<(), CliError> {
    if p.upml.len() == 1 {
        ntff_box(&p.upml, &tet_centroids(&p.tagged.mesh))?;
    }
    Ok(())
}

/// Load, solve and report. With `outdir`, lumped-port specs also export
/// per-row fields (and NTFF for a UPML open radiator); with
/// `touchstone`, lumped-port specs also write a Touchstone 2.0 `.sNp`
/// ([`crate::touchstone`]; wave-port specs are rejected before solving).
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
    touchstone: Option<&Path>,
    opts: SweepOptions,
) -> Result<DrivenReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Driven))?;
    if let Some(path) = touchstone {
        crate::touchstone::validate(&p, path)?;
    }
    // Wave-port specs export nothing, so there is nothing to validate.
    if outdir.is_some() && p.wave_ports.is_empty() {
        validate_export(&p)?;
    }
    let out = OutDir::create_opt(outdir)?;
    let (results, solver, wave_ports) = if p.wave_ports.is_empty() {
        let (results, solver) = sweep(&p, out.as_ref(), "driven", opts)?;
        (results, solver, Vec::new())
    } else {
        if out.is_some() {
            eprintln!(
                "note: --outdir: field / far-field export is not supported for wave-port specs \
                 (lumped ports only); nothing exported"
            );
        }
        wave_sweep(&p, opts)?
    };
    // `∂|S11|²/∂ε_r` per frequency (issue #739); `problem::load` admitted
    // the `sensitivity` section only for a one-lumped-port direct dense
    // sweep without UPML.
    let sensitivities = p
        .sensitivity
        .as_ref()
        .map(|sens| crate::sensitivity::driven(&p, sens, opts.jobs))
        .transpose()?;
    let ports = port_summaries(&p);
    let touchstone_file = touchstone
        .map(|path| {
            let refs: Vec<f64> = ports.iter().map(|q| q.resistance_ohm).collect();
            crate::touchstone::write(path, &provenance, &refs, &results)
        })
        .transpose()?;
    Ok(DrivenReport {
        provenance,
        kind: "driven",
        status: "ok",
        mesh: mesh_summary(&p),
        ports,
        wave_ports,
        silver_muller: silver_muller_summaries(&p),
        absorbing_regions: upml_summaries(&p),
        solver,
        results,
        touchstone_file,
        sensitivities,
    })
}

/// The spec's lumped ports as library [`LumpedPort`]s (resistance in
/// units of η₀), in spec order — shared by the sweep and the `|S11|²`
/// sensitivity ([`crate::sensitivity::driven`]).
pub fn lumped_ports(p: &Problem) -> Vec<LumpedPort<'_>> {
    p.ports
        .iter()
        .map(|port| LumpedPort {
            faces: &port.surface.triangles,
            e_hat: port.e_hat,
            // The solver's resistance is in units of η₀.
            resistance: port.resistance_ohm / ETA_0_OHM,
            width: port.width,
            length: port.length,
            v_inc: port.v_inc,
        })
        .collect()
}

/// The spec's impedance walls: Leontovich walls, then Silver-Müller walls
/// (`Z_s = η₀ = 1` natural) — shared by the sweep and the `|S11|²`
/// sensitivity ([`crate::sensitivity::driven`]).
pub fn impedance_walls(p: &Problem) -> Vec<SurfaceImpedanceBc<'_>> {
    p.leontovich
        .iter()
        .map(|l| SurfaceImpedanceBc {
            triangles: &l.surface.triangles,
            model: l.model(),
        })
        .chain(p.silver_muller.iter().map(|s| SurfaceImpedanceBc {
            triangles: &s.triangles,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        }))
        .collect()
}

/// Roughness loss factor `K(f)` of every rough Leontovich wall at the
/// natural frequency `k0` (issue #758; empty when every wall is smooth).
fn roughness_k(p: &Problem, k0: f64) -> Vec<RoughnessKResult> {
    p.leontovich
        .iter()
        .filter_map(|l| {
            l.roughness_k(k0).map(|k| RoughnessKResult {
                physical_group: l.surface.name.clone(),
                k,
            })
        })
        .collect()
}

/// Solver mode from the spec.
fn solver_mode(p: &Problem) -> SolverMode {
    match p.solver() {
        SolverSpec::Direct {} => SolverMode::Direct,
        SolverSpec::Iterative {
            tol,
            max_iters,
            preconditioner,
        } => SolverMode::Iterative(IterativeSettings::new(tol, max_iters).with_preconditioner(
            match preconditioner {
                crate::spec::PreconditionerSpec::Jacobi => {
                    geode_core::driven::solve::IterativePreconditioner::Jacobi
                }
                crate::spec::PreconditionerSpec::Ilu0 => {
                    geode_core::driven::solve::IterativePreconditioner::Ilu0
                }
                crate::spec::PreconditionerSpec::Ams => {
                    geode_core::driven::solve::IterativePreconditioner::AMS
                }
            },
        )),
    }
}

/// The driven materials of one frequency: the per-tet `ε_r(f)`
/// ([`Problem::eps_at`]) as a scalar, or — with `absorbing_regions` and /
/// or anisotropic materials ([`Problem::needs_tensor_materials`]) — the
/// full constitutive tensors built from it ([`Problem::material_tensors`]:
/// `ε = ε_r·Λ`, `ν = Λ⁻¹·μ_r⁻¹`).
enum FrequencyMaterials<'a> {
    Scalar(std::borrow::Cow<'a, [c64]>),
    #[allow(clippy::type_complexity)]
    Tensor(Vec<[[c64; 3]; 3]>, Vec<[[c64; 3]; 3]>),
}

impl<'a> FrequencyMaterials<'a> {
    /// `centroids` is `tet_centroids` of the mesh (unused without UPML).
    fn at(p: &'a Problem, centroids: &[[f64; 3]], f: &problem::Frequency) -> Self {
        let eps = p.eps_at(f.hz);
        if p.needs_tensor_materials() {
            let (epsilon_tensor, nu_tensor) = p.material_tensors(&eps, centroids, f.k0);
            Self::Tensor(epsilon_tensor, nu_tensor)
        } else {
            Self::Scalar(eps)
        }
    }

    /// The ω-independent materials of a spec without UPML or dispersion
    /// (`!p.is_frequency_dependent()`): the scalar [`Problem::eps`]
    /// borrowed as before, or the anisotropic tensors (issue #760).
    fn constant(p: &'a Problem) -> Self {
        debug_assert!(!p.is_frequency_dependent());
        if p.is_anisotropic() {
            // No UPML: centroids / k0 are not read.
            let (epsilon_tensor, nu_tensor) = p.material_tensors(&p.eps, &[], 0.0);
            Self::Tensor(epsilon_tensor, nu_tensor)
        } else {
            Self::Scalar(std::borrow::Cow::Borrowed(&p.eps))
        }
    }

    fn materials(&self) -> DrivenMaterials<'_> {
        match self {
            Self::Scalar(eps) => DrivenMaterials::Scalar(eps),
            Self::Tensor(epsilon_tensor, nu_tensor) => DrivenMaterials::MatchedUpml {
                epsilon_tensor,
                nu_tensor,
            },
        }
    }
}

/// Run `solve` over `p.frequencies` (passed as natural `k₀`): in one
/// batched call when the materials are ω-independent
/// ([`FrequencyMaterials::constant`]), else once per
/// frequency with that frequency's [`FrequencyMaterials`] — UPML
/// tensors and / or dispersive `ε_r(f)` (the library sweeps assemble
/// once per call).
fn per_material_sweep<T>(
    p: &Problem,
    mut solve: impl FnMut(DrivenMaterials<'_>, &[f64]) -> Result<Vec<T>, CliError>,
) -> Result<Vec<T>, CliError> {
    if !p.is_frequency_dependent() {
        let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();
        return solve(FrequencyMaterials::constant(p).materials(), &omegas);
    }
    let centroids = tet_centroids(&p.tagged.mesh);
    let mut out = Vec::with_capacity(p.frequencies.len());
    for f in &p.frequencies {
        let m = FrequencyMaterials::at(p, &centroids, f);
        out.extend(solve(m.materials(), std::slice::from_ref(&f.k0))?);
    }
    Ok(out)
}

/// Each dispersive region's `ε_r` at `hz` for a report row (issue #757;
/// empty when every material is constant).
fn dispersive_eps(p: &Problem, hz: f64) -> Vec<MaterialEpsResult> {
    p.dispersive_eps(hz)
        .into_iter()
        .map(|(name, e)| MaterialEpsResult {
            physical_group: name.to_string(),
            eps_r: pair(e),
        })
        .collect()
}

/// One lumped-port sweep row in solver (natural) units, before the
/// report conversion: `Z` (units of η₀), the S-matrix, the worst residual
/// (or adaptive residual indicator), the per-RHS Krylov iterations and,
/// for an adaptive sweep, whether the point was solved full-order.
struct LumpedRow {
    z: Vec<c64>,
    s: SMatrix,
    residual_rel: f64,
    iters_per_rhs: Vec<usize>,
    solved: Option<bool>,
}

impl LumpedRow {
    fn from_point(pt: SParameterSweepPoint, solved: Option<bool>) -> Self {
        Self {
            z: pt.z,
            s: pt.s,
            residual_rel: pt.residual_rel,
            iters_per_rhs: pt.iters_per_rhs,
            solved,
        }
    }
}

/// Run the port-driven sweep over `p.frequencies` (in that order) and
/// assemble the per-frequency Z / Y / S / per-port results and the
/// aggregate solver statistics. Non-finite residuals or impedances are
/// a hard [`CliError::NonFinite`]. With `out`, each row additionally
/// gets its exported field (and far field) — see [`Exporter::export_row`]; the
/// export solves are not counted in [`SolverStats`].
///
/// The dense sweep solves every frequency full-order
/// ([`s_parameter_point`] on one assembled operator, or one per frequency
/// with UPML), up to `opts.jobs` at a time; a spec with
/// `sweep.adaptive` runs the reduced-order-model sweep instead
/// ([`adaptive_sweep`], issue #708) and exports fields only for its
/// full-order rows. `command` names the subcommand in the
/// `sweep_start` progress event ([`crate::progress`]).
pub fn sweep(
    p: &Problem,
    out: Option<&OutDir>,
    command: &'static str,
    opts: SweepOptions,
) -> Result<(Vec<FrequencyResult>, SolverStats), CliError> {
    let lumped = lumped_ports(p);
    let surfaces = impedance_walls(p);
    let mode = solver_mode(p);

    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    let adaptive = p.spec.sweep.as_ref().and_then(|s| s.adaptive);
    let progress = Progress::new(opts.progress);
    progress.emit(
        "sweep_start",
        json!({
            "command": command,
            "method": if adaptive.is_some() { "adaptive" } else { "dense" },
            "n_frequencies": p.frequencies.len(),
            "jobs": opts.jobs,
        }),
    );
    let t0 = Instant::now();
    let (rows, adaptive_stats) = match adaptive {
        Some(a) => {
            let (rows, stats) =
                adaptive_sweep::<B>(p, &lumped, &surfaces, &bcs, a, opts, &progress)?;
            (rows, Some(stats))
        }
        None => (
            dense_sweep::<B>(p, &lumped, &surfaces, &bcs, mode, opts, &progress)?,
            None,
        ),
    };
    let wall_time_s = t0.elapsed().as_secs_f64();

    let n = p.ports.len();
    let mut results = Vec::with_capacity(rows.len());
    for (index, (pt, f)) in rows.iter().zip(&p.frequencies).enumerate() {
        let z: Vec<c64> = pt.z.iter().map(|&z| z * ETA_0_OHM).collect();
        if !pt.residual_rel.is_finite() {
            return Err(CliError::NonFinite {
                index,
                what: format!("residual_rel = {}", pt.residual_rel),
            });
        }
        if let Some(bad) = z.iter().find(|z| !(z.re.is_finite() && z.im.is_finite())) {
            return Err(CliError::NonFinite {
                index,
                what: format!("Z = {bad}"),
            });
        }
        let omega_rad_s = 2.0 * std::f64::consts::PI * f.hz;
        let ports = (0..n)
            .map(|k| {
                let zkk = z[k * n + k];
                let skk = pt.s.entry(k, k);
                PortResult {
                    index: k,
                    z_ohm: pair(zkk),
                    s: pair(skk),
                    s_db: 20.0 * skk.norm().log10(),
                    r_ohm: zkk.re,
                    l_h: zkk.im / omega_rad_s,
                    q: zkk.im / zkk.re,
                }
            })
            .collect();
        results.push(FrequencyResult {
            frequency_hz: f.hz,
            k0: f.k0,
            omega_rad_s,
            residual_rel: pt.residual_rel,
            solved: pt.solved,
            iterations: pt.iters_per_rhs.clone(),
            z_ohm: matrix(&z, n),
            y_s: invert(&z, n).map(|y| matrix(&y, n)),
            s: matrix(&pt.s.s, n),
            ports,
            wave_channels: Vec::new(),
            roughness_k: roughness_k(p, f.k0),
            materials: dispersive_eps(p, f.hz),
            field_file: None,
            far_field: None,
        });
    }
    let n_solved = rows.iter().filter(|r| r.solved != Some(false)).count();
    progress.emit(
        "sweep_done",
        json!({
            "n_frequencies": rows.len(),
            "n_solved": n_solved,
            "n_interpolated": rows.len() - n_solved,
        }),
    );

    if let Some(out) = out {
        if n_solved < rows.len() {
            eprintln!(
                "note: --outdir: the adaptive sweep exports fields only for its {n_solved} \
                 full-order rows (`solved: true`); {} interpolated rows have no field file",
                rows.len() - n_solved
            );
        }
        let exporter = Exporter::<B>::new(p, &lumped, &surfaces, mode, &device)?;
        for (index, (row, f)) in results.iter_mut().zip(&p.frequencies).enumerate() {
            if row.solved != Some(false) {
                exporter.export_row(out, index, f, row)?;
            }
        }
    }

    let summary = crate::check::solver_summary(p.solver());
    Ok((
        results,
        SolverStats {
            mode: summary.mode,
            tol: summary.tol,
            max_iters: summary.max_iters,
            iterations_max: rows
                .iter()
                .flat_map(|pt| pt.iters_per_rhs.iter().copied())
                .max()
                .unwrap_or(0),
            residual_rel_max: rows.iter().map(|pt| pt.residual_rel).fold(0.0, f64::max),
            wall_time_s,
            jobs: opts.jobs_explicit.then_some(opts.jobs),
            adaptive: adaptive_stats,
        },
    ))
}

/// Emit the `point` progress event of report row `index`.
fn emit_point(progress: &Progress, p: &Problem, index: usize, row: &LumpedRow) {
    progress.emit(
        "point",
        json!({
            "index": index,
            "frequency_hz": p.frequencies[index].hz,
            "solved": row.solved != Some(false),
            "residual_rel": row.residual_rel,
        }),
    );
}

/// Dense sweep: every frequency solved full-order, up to `opts.jobs`
/// concurrently ([`crate::progress::par_map`]; rows in frequency order).
/// Without UPML or dispersive materials the operator is assembled once
/// and shared by the workers (each frequency factors its own `A(ω)`);
/// otherwise each frequency assembles its own operator from that
/// frequency's [`FrequencyMaterials`] (stretch tensors and / or `ε_r(f)`).
fn dense_sweep<B: burn::tensor::backend::Backend>(
    p: &Problem,
    lumped: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    bcs: &DrivenBcs<'_>,
    mode: SolverMode,
    opts: SweepOptions,
    progress: &Progress,
) -> Result<Vec<LumpedRow>, CliError> {
    let mesh = &p.tagged.mesh;
    let n = p.frequencies.len();
    let solve_row = |index: usize, op: &DrivenOperator| -> Result<LumpedRow, CliError> {
        let device = <B as BackendTypes>::Device::default();
        let pt = s_parameter_point::<B>(op, p.frequencies[index].k0, mode, &device)?;
        let row = LumpedRow::from_point(pt, None);
        emit_point(progress, p, index, &row);
        Ok(row)
    };
    if !p.is_frequency_dependent() {
        let device = <B as BackendTypes>::Device::default();
        let op = s_parameter_operator::<B>(
            mesh,
            FrequencyMaterials::constant(p).materials(),
            None,
            bcs,
            lumped,
            surfaces,
            &device,
        )?;
        return par_map(n, opts.jobs, |index| solve_row(index, &op));
    }
    let centroids = tet_centroids(mesh);
    par_map(n, opts.jobs, |index| {
        let device = <B as BackendTypes>::Device::default();
        let m = FrequencyMaterials::at(p, &centroids, &p.frequencies[index]);
        let op =
            s_parameter_operator::<B>(mesh, m.materials(), None, bcs, lumped, surfaces, &device)?;
        solve_row(index, &op)
    })
}

/// Adaptive sweep (issue #708): a block Galerkin PROM over every port
/// excitation ([`DrivenRom::build_with`] with [`RomDrive::PerPort`]) on
/// the frequency grid, then every frequency evaluated through it. The
/// per-excitation V / I readbacks give `Z = V·I⁻¹` and the S-matrix with
/// the dense sweep's own arithmetic ([`z_from_port_readbacks`],
/// [`SMatrix::from_z_matrix`]). A frequency whose residual indicator is
/// still above `tolerance` (budget exhausted), or whose reduced system or
/// reduced port-current matrix is singular, gets a full-order fallback
/// solve (up to `opts.jobs` at a time). `problem::load` has already
/// rejected UPML, wave ports and the iterative solver.
fn adaptive_sweep<B: burn::tensor::backend::Backend>(
    p: &Problem,
    lumped: &[LumpedPort<'_>],
    surfaces: &[SurfaceImpedanceBc<'_>],
    bcs: &DrivenBcs<'_>,
    a: AdaptiveSweepSpec,
    opts: SweepOptions,
    progress: &Progress,
) -> Result<(Vec<LumpedRow>, AdaptiveSweepStats), CliError> {
    let device = <B as BackendTypes>::Device::default();
    let op = s_parameter_operator::<B>(
        &p.tagged.mesh,
        FrequencyMaterials::constant(p).materials(),
        None,
        bcs,
        lumped,
        surfaces,
        &device,
    )?;
    let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();
    // Snapshot ω are grid values verbatim, so the lookup is exact.
    let hz_of = |w: f64| {
        p.frequencies
            .iter()
            .find(|f| f.k0 == w)
            .map_or(f64::NAN, |f| f.hz)
    };
    let settings = RomSettings {
        tolerance: a.tolerance,
        max_snapshots: a.max_snapshots,
    };
    let mut n_snapshots = 0_usize;
    let rom = DrivenRom::build_with(&op, &omegas, &settings, RomDrive::PerPort, &mut |w| {
        n_snapshots += 1;
        progress.emit(
            "snapshot",
            json!({ "frequency_hz": hz_of(w), "n_snapshots": n_snapshots }),
        );
    })?;

    let n = op.n_ports();
    let z0: Vec<f64> = (0..n).map(|k| op.port_resistance(k)).collect();
    let mut rows: Vec<Option<LumpedRow>> = Vec::with_capacity(omegas.len());
    let mut fallback: Vec<usize> = Vec::new();
    for (index, &w) in omegas.iter().enumerate() {
        let Some((z, residual)) = reduced_z(rom.evaluate_excitations(w), a.tolerance, n)? else {
            fallback.push(index);
            rows.push(None);
            continue;
        };
        let s = SMatrix::from_z_matrix(&z, &z0);
        let row = LumpedRow {
            z,
            s,
            residual_rel: residual,
            iters_per_rhs: vec![0; n],
            solved: Some(rom.snapshot_omegas().contains(&w)),
        };
        emit_point(progress, p, index, &row);
        rows.push(Some(row));
    }
    let solved = par_map(fallback.len(), opts.jobs, |m| {
        let index = fallback[m];
        let device = <B as BackendTypes>::Device::default();
        let pt = s_parameter_point::<B>(&op, omegas[index], SolverMode::Direct, &device)?;
        let row = LumpedRow::from_point(pt, Some(true));
        emit_point(progress, p, index, &row);
        Ok(row)
    })?;
    for (&index, row) in fallback.iter().zip(solved) {
        rows[index] = Some(row);
    }
    let rows: Vec<LumpedRow> = rows
        .into_iter()
        .map(|r| r.expect("every row interpolated or solved"))
        .collect();

    let n_solved = rows.iter().filter(|r| r.solved == Some(true)).count();
    let mut fallback_frequencies_hz: Vec<f64> =
        fallback.iter().map(|&i| p.frequencies[i].hz).collect();
    fallback_frequencies_hz.sort_by(f64::total_cmp);
    let stats = AdaptiveSweepStats {
        tolerance: a.tolerance,
        max_snapshots: a.max_snapshots,
        converged: rom.converged(),
        worst_residual: rom.worst_residual(),
        reduced_order: rom.reduced_order(),
        snapshot_frequencies_hz: rom.snapshot_omegas().iter().map(|&w| hz_of(w)).collect(),
        fallback_frequencies_hz,
        n_solved,
        n_interpolated: rows.len() - n_solved,
        n_factorizations: rom.snapshot_omegas().len() + fallback.len(),
    };
    Ok((rows, stats))
}

/// `Z(ω)` and the residual indicator from one reduced evaluation, or
/// `None` when the frequency needs a full-order fallback solve: the
/// indicator is NaN or above `tolerance`, the reduced system is singular
/// ([`RomError::ReducedSolveSingular`], possible once the snapshot budget
/// is exhausted), or the per-excitation port-current matrix is singular
/// (`Z = V·I⁻¹` undefined or non-finite). Any other error aborts the
/// sweep: the full-order solve would hit it too (issue #747).
fn reduced_z(
    eval: Result<RomExcitationPoint, RomError>,
    tolerance: f64,
    n: usize,
) -> Result<Option<(Vec<c64>, f64)>, CliError> {
    let pt = match eval {
        Ok(pt) => pt,
        Err(RomError::ReducedSolveSingular { .. }) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    if pt.residual_indicator.is_nan() || pt.residual_indicator > tolerance {
        return Ok(None);
    }
    // v_mat[k][j] / i_mat[k][j]: port k under excitation j.
    let mut v_mat = vec![c64::new(0.0, 0.0); n * n];
    let mut i_mat = vec![c64::new(0.0, 0.0); n * n];
    for (j, ports) in pt.excitations.iter().enumerate() {
        for (k, c) in ports.iter().enumerate() {
            v_mat[k * n + j] = c.v;
            i_mat[k * n + j] = c.i;
        }
    }
    Ok(z_from_port_readbacks(&v_mat, &i_mat, n)
        .filter(|z| z.iter().all(|x| x.re.is_finite() && x.im.is_finite()))
        .map(|z| (z, pt.residual_indicator)))
}

/// Wave-port sweep over `p.frequencies`: solve each port's cross-section
/// modes, run the rank-N SMW wave-port sweep, and report the channel
/// S-matrix plus the ports' solved modes. Serial: `opts.jobs` is not
/// applied (noted on stderr when `> 1`), and the `point` progress events
/// follow the library sweep (all at once without UPML, per frequency
/// with it).
pub fn wave_sweep(
    p: &Problem,
    opts: SweepOptions,
) -> Result<(Vec<FrequencyResult>, SolverStats, Vec<WavePortSummary>), CliError> {
    if opts.jobs > 1 {
        eprintln!("note: --jobs: wave-port sweeps run serially (one frequency at a time)");
    }
    let progress = Progress::new(opts.progress);
    progress.emit(
        "sweep_start",
        json!({
            "command": "driven",
            "method": "dense",
            "n_frequencies": p.frequencies.len(),
            "jobs": 1,
        }),
    );
    let t0 = Instant::now();
    let ports: Vec<WavePort> = p
        .wave_ports
        .iter()
        .map(|w| {
            w.projection
                .wave_port(&p.edges, &w.a_inc)
                .map_err(|err| CliError::WavePort {
                    name: w.surface.name.clone(),
                    err,
                })
        })
        .collect::<Result<_, _>>()?;

    // k₀ → Hz for the cutoff report (k₀ is linear in f).
    let hz_per_k0 =
        crate::problem::to_frequency(1.0, crate::spec::FrequencyUnit::K0, p.length_unit_m()).hz;
    let mut channel = 0;
    let mut summaries = crate::check::wave_port_summaries(p);
    for (summary, port) in summaries.iter_mut().zip(&ports) {
        summary.modes = Some(
            port.modes
                .iter()
                .enumerate()
                .map(|(mode, m)| {
                    channel += 1;
                    WaveModeSummary {
                        mode,
                        channel: channel - 1,
                        k_c: m.k_c,
                        cutoff_hz: m.k_c * hz_per_k0,
                    }
                })
                .collect(),
        );
    }

    let mode = solver_mode(p);
    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    let mut done = 0_usize;
    let points: Vec<WavePortSweepPoint> = per_material_sweep(p, |materials, w| {
        let pts = solve_wave_port_sweep_with_mode::<B>(
            &p.tagged.mesh,
            materials,
            None,
            &bcs,
            &ports,
            w,
            mode,
            &device,
        )?;
        for pt in &pts {
            progress.emit(
                "point",
                json!({
                    "index": done,
                    "frequency_hz": p.frequencies[done].hz,
                    "solved": true,
                    "residual_rel": pt.residual_rel,
                }),
            );
            done += 1;
        }
        Ok(pts)
    })?;
    let wall_time_s = t0.elapsed().as_secs_f64();

    let mut results = Vec::with_capacity(points.len());
    for (index, (pt, f)) in points.iter().zip(&p.frequencies).enumerate() {
        let n = pt.n_channels;
        if !pt.residual_rel.is_finite() {
            return Err(CliError::NonFinite {
                index,
                what: format!("residual_rel = {}", pt.residual_rel),
            });
        }
        if let Some(bad) =
            pt.s.iter()
                .find(|z| !(z.re.is_finite() && z.im.is_finite()))
        {
            return Err(CliError::NonFinite {
                index,
                what: format!("S = {bad}"),
            });
        }
        let mut wave_channels = Vec::with_capacity(n);
        for (port, &k) in pt.port_mode_counts.iter().enumerate() {
            for m in 0..k {
                let c = pt.channel_index(port, m);
                let skk = pt.s[c * n + c];
                let beta = pt.beta[c];
                wave_channels.push(WaveChannelResult {
                    channel: c,
                    port,
                    mode: m,
                    beta: pair(beta),
                    propagating: beta.re > 0.0,
                    s: pair(skk),
                    s_db: 20.0 * skk.norm().log10(),
                });
            }
        }
        results.push(FrequencyResult {
            frequency_hz: f.hz,
            k0: f.k0,
            omega_rad_s: 2.0 * std::f64::consts::PI * f.hz,
            residual_rel: pt.residual_rel,
            solved: None,
            iterations: pt.iters_per_rhs.clone(),
            z_ohm: Vec::new(),
            y_s: None,
            s: matrix(&pt.s, n),
            ports: Vec::new(),
            wave_channels,
            roughness_k: roughness_k(p, f.k0),
            materials: dispersive_eps(p, f.hz),
            field_file: None,
            far_field: None,
        });
    }

    let summary = crate::check::solver_summary(p.solver());
    Ok((
        results,
        SolverStats {
            mode: summary.mode,
            tol: summary.tol,
            max_iters: summary.max_iters,
            iterations_max: points
                .iter()
                .flat_map(|pt| pt.iters_per_rhs.iter().copied())
                .max()
                .unwrap_or(0),
            residual_rel_max: points.iter().map(|pt| pt.residual_rel).fold(0.0, f64::max),
            wall_time_s,
            jobs: opts.jobs_explicit.then_some(opts.jobs),
            adaptive: None,
        },
        summaries,
    ))
}

/// Per-row field / NTFF exporter for a lumped-port spec: the
/// ω-independent [`DrivenOperator`] (built once without UPML or
/// dispersion; per row with either, since the materials are then
/// ω-dependent) plus the NTFF box.
struct Exporter<'a, B: burn::tensor::backend::Backend> {
    p: &'a Problem,
    lumped: &'a [LumpedPort<'a>],
    surfaces: &'a [SurfaceImpedanceBc<'a>],
    mode: SolverMode,
    device: &'a B::Device,
    /// Zero volume source: the ports are the only drive.
    source: CurrentSource,
    /// The operator of a spec without UPML or dispersion (ω-independent
    /// materials).
    scalar_op: Option<DrivenOperator>,
    /// Tet centroids (UPML tensors), empty for ω-independent materials.
    centroids: Vec<[f64; 3]>,
    /// Per-node `Re ε_r` for the `.vtu` (re-evaluated per row when a
    /// material is dispersive).
    eps_nodes: Vec<f64>,
    /// NTFF box, with exactly one UPML shell.
    ntff_box: Option<([f64; 3], [f64; 3])>,
}

impl<'a, B: burn::tensor::backend::Backend> Exporter<'a, B> {
    fn new(
        p: &'a Problem,
        lumped: &'a [LumpedPort<'a>],
        surfaces: &'a [SurfaceImpedanceBc<'a>],
        mode: SolverMode,
        device: &'a B::Device,
    ) -> Result<Self, CliError> {
        let mesh = &p.tagged.mesh;
        let mut this = Self {
            p,
            lumped,
            surfaces,
            mode,
            device,
            source: CurrentSource {
                j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
            },
            scalar_op: None,
            centroids: Vec::new(),
            eps_nodes: eps_per_node(mesh, &p.eps),
            ntff_box: None,
        };
        if p.is_frequency_dependent() {
            this.centroids = tet_centroids(mesh);
        } else {
            this.scalar_op = Some(this.assemble(FrequencyMaterials::constant(p).materials())?);
        }
        this.ntff_box = ntff_box(&p.upml, &this.centroids)?;
        Ok(this)
    }

    fn assemble(&self, materials: DrivenMaterials<'_>) -> Result<DrivenOperator, CliError> {
        Ok(DrivenOperator::assemble::<B>(
            &self.p.tagged.mesh,
            materials,
            None,
            &DrivenBcs {
                pec_interior_mask: &self.p.pec_mask,
            },
            self.lumped,
            self.surfaces,
            &self.source,
            self.device,
        )?)
    }

    /// One extra solve at row `index` (every port driven at its `v_inc`),
    /// written to `E_<index>.vtu`; plus the NTFF quantities and pattern
    /// file when [`Exporter::ntff_box`] is set.
    fn export_row(
        &self,
        out: &OutDir,
        index: usize,
        f: &crate::problem::Frequency,
        row: &mut FrequencyResult,
    ) -> Result<(), CliError> {
        let p = self.p;
        let mesh = &p.tagged.mesh;
        let omega = f.k0;
        let row_op;
        let op = match &self.scalar_op {
            Some(op) => op,
            None => {
                let m = FrequencyMaterials::at(p, &self.centroids, f);
                row_op = self.assemble(m.materials())?;
                &row_op
            }
        };
        let row_eps_nodes;
        let eps_nodes = if p.dispersion.is_empty() {
            &self.eps_nodes
        } else {
            row_eps_nodes = eps_per_node(mesh, &p.eps_at(f.hz));
            &row_eps_nodes
        };
        let (sol, _) = op.prepare_at::<B>(omega, self.mode, self.device)?.solve()?;
        if !sol.residual_rel.is_finite()
            || sol
                .e_edges
                .iter()
                .any(|e| !(e.re.is_finite() && e.im.is_finite()))
        {
            return Err(CliError::NonFinite {
                index,
                what: format!(
                    "--outdir export field (residual_rel = {})",
                    sol.residual_rel
                ),
            });
        }
        row.field_file =
            Some(out.write_field(&field_file_name(index), mesh, &sol.e_edges, true, eps_nodes)?);

        let Some((lo, hi)) = self.ntff_box else {
            return Ok(());
        };
        // Net input power Σ_k ½ Re(V_k I_k*) (η₀-normalized natural
        // units; cancels in η against the box flux).
        let p_in: f64 = (0..op.n_ports())
            .map(|k| {
                let v = op.port_voltage(k, &sol.e_edges);
                let i = op.port_current(k, v);
                0.5 * (v * i.conj()).re
            })
            .sum();
        let p_rad = flux_power_box(mesh, omega, &sol.e_edges, lo, hi);
        let efficiency = if p_in != 0.0 { p_rad / p_in } else { 0.0 };
        let ff = ntff_far_field(mesh, omega, &sol.e_edges, lo, hi, NTFF_N_THETA, NTFF_N_PHI);
        let (directivity_max, _) = directivity(&ff);
        let directivity_broadside = broadside_directivity(&ff);
        let gain_broadside = gain(directivity_broadside, efficiency);
        let (e_plane, h_plane) = principal_plane_cuts(&ff);
        let pattern_file = out.write_json(
            &pattern_file_name(index),
            &PatternFile::new(f.hz, e_plane, h_plane),
        )?;
        row.far_field = Some(FarFieldResult {
            box_lo: lo,
            box_hi: hi,
            directivity_max,
            directivity_broadside,
            gain_broadside,
            gain_broadside_db: to_db(gain_broadside),
            efficiency,
            pattern_file,
        });
        Ok(())
    }
}

/// `(lo, hi)` shrunk by `frac` of its half-extent toward its centre.
fn shrink_box(lo: [f64; 3], hi: [f64; 3], frac: f64) -> ([f64; 3], [f64; 3]) {
    let c: [f64; 3] = std::array::from_fn(|k| 0.5 * (lo[k] + hi[k]));
    let h: [f64; 3] = std::array::from_fn(|k| 0.5 * (hi[k] - lo[k]));
    (
        std::array::from_fn(|k| c[k] - (1.0 - frac) * h[k]),
        std::array::from_fn(|k| c[k] + (1.0 - frac) * h[k]),
    )
}

fn pair(z: c64) -> Complex {
    [z.re, z.im]
}

/// Row-major flat `n × n` → nested rows of `[re, im]`.
fn matrix(m: &[c64], n: usize) -> Vec<Vec<Complex>> {
    m.chunks(n)
        .map(|row| row.iter().copied().map(pair).collect())
        .collect()
}

/// Inverse of a row-major `n × n` complex matrix by Gauss–Jordan with
/// partial pivoting; `None` if (numerically) singular.
fn invert(a: &[c64], n: usize) -> Option<Vec<c64>> {
    let zero = c64::new(0.0, 0.0);
    let one = c64::new(1.0, 0.0);
    let mut m = a.to_vec();
    let mut inv = vec![zero; n * n];
    for i in 0..n {
        inv[i * n + i] = one;
    }
    let scale = a.iter().map(|z| z.norm()).fold(0.0, f64::max);
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&r1, &r2| m[r1 * n + col].norm().total_cmp(&m[r2 * n + col].norm()))?;
        if m[piv * n + col].norm() <= 1e-14 * scale {
            return None;
        }
        if piv != col {
            for c in 0..n {
                m.swap(piv * n + c, col * n + c);
                inv.swap(piv * n + c, col * n + c);
            }
        }
        let d = one / m[col * n + col];
        for c in 0..n {
            m[col * n + c] *= d;
            inv[col * n + c] *= d;
        }
        for r in 0..n {
            if r != col {
                let f = m[r * n + col];
                if f != zero {
                    for c in 0..n {
                        let (mc, ic) = (m[col * n + c], inv[col * n + c]);
                        m[r * n + c] -= f * mc;
                        inv[r * n + c] -= f * ic;
                    }
                }
            }
        }
    }
    Some(inv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use geode_core::driven::extraction::PortCircuit;

    fn circuit(v: f64, i: f64) -> PortCircuit {
        let (v, i) = (c64::new(v, 0.0), c64::new(i, 0.0));
        PortCircuit { v, i, z: v / i }
    }

    fn point(residual: f64, excitations: Vec<Vec<PortCircuit>>) -> RomExcitationPoint {
        RomExcitationPoint {
            omega: 1.0,
            residual_indicator: residual,
            excitations,
        }
    }

    /// Issue #747: the budget-exhaustion failure modes route to the
    /// full-order fallback (`Ok(None)`); other errors still abort.
    #[test]
    fn reduced_z_routes_singular_cases_to_fallback() {
        // Healthy single port: Z = V / I.
        let (z, r) = reduced_z(Ok(point(1e-9, vec![vec![circuit(2.0, 4.0)]])), 1e-6, 1)
            .unwrap()
            .unwrap();
        assert_eq!((z, r), (vec![c64::new(0.5, 0.0)], 1e-9));
        // Indicator above tolerance or NaN.
        let hi = point(1e-3, vec![vec![circuit(2.0, 4.0)]]);
        assert!(reduced_z(Ok(hi), 1e-6, 1).unwrap().is_none());
        let nan = point(f64::NAN, vec![vec![circuit(2.0, 4.0)]]);
        assert!(reduced_z(Ok(nan), 1e-6, 1).unwrap().is_none());
        // Singular reduced system.
        let singular = Err(RomError::ReducedSolveSingular {
            order: 3,
            omega: 1.0,
        });
        assert!(reduced_z(singular, 1e-6, 1).unwrap().is_none());
        // Singular port-current matrix: one port (V / 0 is non-finite) and
        // two ports (rank-1 I).
        let zero_i = point(1e-9, vec![vec![circuit(2.0, 0.0)]]);
        assert!(reduced_z(Ok(zero_i), 1e-6, 1).unwrap().is_none());
        let rank1 = point(
            1e-9,
            vec![
                vec![circuit(1.0, 1.0), circuit(1.0, 1.0)],
                vec![circuit(2.0, 1.0), circuit(3.0, 1.0)],
            ],
        );
        assert!(reduced_z(Ok(rank1), 1e-6, 2).unwrap().is_none());
        // Anything else still aborts the sweep.
        let invalid = Err(RomError::InvalidParameter("bad".into()));
        assert!(reduced_z(invalid, 1e-6, 1).is_err());
    }

    #[test]
    fn invert_two_by_two() {
        let a = [
            c64::new(2.0, 1.0),
            c64::new(0.5, 0.0),
            c64::new(0.5, 0.0),
            c64::new(3.0, -1.0),
        ];
        let inv = invert(&a, 2).unwrap();
        for r in 0..2 {
            for c in 0..2 {
                let mut acc = c64::new(0.0, 0.0);
                for k in 0..2 {
                    acc += a[r * 2 + k] * inv[k * 2 + c];
                }
                let want = if r == c { 1.0 } else { 0.0 };
                assert!((acc - c64::new(want, 0.0)).norm() < 1e-12);
            }
        }
        assert!(invert(&[c64::new(0.0, 0.0)], 1).is_none());
    }

    /// A shell whose inner wall is the box `[-10, 10]³`.
    fn shell(name: &str) -> UpmlRegion {
        UpmlRegion {
            name: name.into(),
            tag: 1,
            n_tets: 1,
            n_tets_stretched: 1,
            thickness: 1.0,
            sigma_0: 25.0,
            air_lo: [-10.0; 3],
            air_hi: [10.0; 3],
        }
    }

    #[test]
    fn ntff_box_is_the_single_shell_wall_shrunk() {
        // 0 or ≥ 2 shells: no NTFF, and no check (even with no centroid).
        assert_eq!(ntff_box(&[], &[]).unwrap(), None);
        assert_eq!(ntff_box(&[shell("a"), shell("b")], &[]).unwrap(), None);
        // One shell with a centroid inside the shrunk box.
        let (lo, hi) = ntff_box(&[shell("a")], &[[0.0; 3], [9.5, 0.0, 0.0]])
            .unwrap()
            .expect("one shell → an NTFF box");
        for k in 0..3 {
            assert!((lo[k] + 9.0).abs() < 1e-12 && (hi[k] - 9.0).abs() < 1e-12);
        }
    }

    #[test]
    fn ntff_box_without_a_centroid_inside_is_invalid_spec() {
        // Inside the shell's inner wall, but outside the 10 %-shrunk box.
        let err = ntff_box(&[shell("upml")], &[[9.5, 0.0, 0.0], [0.0, -9.5, 0.0]]).unwrap_err();
        let CliError::InvalidSpec(msg) = err else {
            panic!("expected InvalidSpec, got {err:?}");
        };
        assert!(
            msg.contains("no tet centroid lies inside the NTFF box"),
            "{msg}"
        );
        assert!(msg.contains("`upml`"), "{msg}");
        assert!(!msg.contains("  "), "stray whitespace: {msg:?}");
    }
}
