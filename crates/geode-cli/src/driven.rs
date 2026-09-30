//! `geode driven`: port-driven frequency sweep → Z / Y / S per frequency.
//!
//! Wraps [`geode_core::driven::extraction::s_parameter_frequency_sweep_with_mode`],
//! which assembles the ω-independent operator **once**, then per
//! frequency factors (direct) or preconditions (iterative) `A(ω)` and
//! back-solves one right-hand side per port. Lumped ports and
//! Leontovich walls are composed into the same operator. With a single
//! port this is bit-for-bit the
//! [`geode_core::driven::extraction::driven_frequency_sweep`] path the
//! spiral-inductor benchmark uses.
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
//! * **Wave ports** build each port's modes from its tagged face
//!   ([`geode_core::driven::ports::PortFaceProjection::wave_port`]) and
//!   run [`solve_wave_port_sweep_with_mode`] (per frequency with UPML,
//!   batched otherwise). The result is a power-normalized channel
//!   S-matrix; wave ports define no port impedance.
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
use geode_core::driven::extraction::{SParameterSweepPoint, s_parameter_frequency_sweep_with_mode};
use geode_core::driven::ports::{
    LumpedPort, WavePort, WavePortSweepPoint, solve_wave_port_sweep_with_mode,
};
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
use crate::report::{
    Complex, DrivenReport, FarFieldResult, FrequencyResult, PortResult, Provenance, SolverStats,
    WaveChannelResult, WaveModeSummary, WavePortSummary,
};
use crate::spec::{Analysis, SolverSpec};

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
        let (results, solver) = sweep(&p, out.as_ref())?;
        (results, solver, Vec::new())
    } else {
        if out.is_some() {
            eprintln!(
                "note: --outdir: field / far-field export is not supported for wave-port specs \
                 (lumped ports only); nothing exported"
            );
        }
        wave_sweep(&p)?
    };
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
    })
}

/// Solver mode from the spec.
fn solver_mode(p: &Problem) -> SolverMode {
    match p.solver() {
        SolverSpec::Direct {} => SolverMode::Direct,
        SolverSpec::Iterative { tol, max_iters } => {
            SolverMode::Iterative(IterativeSettings::new(tol, max_iters))
        }
    }
}

/// Run `solve` over `omegas`: in one batched call when the materials are
/// ω-independent (no UPML — `DrivenMaterials::Scalar`), else once per
/// frequency with that frequency's `DrivenMaterials::MatchedUpml`
/// tensors (the library sweeps assemble once per call).
fn per_material_sweep<T>(
    p: &Problem,
    omegas: &[f64],
    mut solve: impl FnMut(DrivenMaterials<'_>, &[f64]) -> Result<Vec<T>, CliError>,
) -> Result<Vec<T>, CliError> {
    if p.upml.is_empty() {
        return solve(DrivenMaterials::Scalar(&p.eps), omegas);
    }
    let centroids = tet_centroids(&p.tagged.mesh);
    let mut out = Vec::with_capacity(omegas.len());
    for omega in omegas {
        let (epsilon_tensor, nu_tensor) = p.upml_tensors(&centroids, *omega);
        out.extend(solve(
            DrivenMaterials::MatchedUpml {
                epsilon_tensor: &epsilon_tensor,
                nu_tensor: &nu_tensor,
            },
            std::slice::from_ref(omega),
        )?);
    }
    Ok(out)
}

/// Run the port-driven sweep over `p.frequencies` (in that order) and
/// assemble the per-frequency Z / Y / S / per-port results and the
/// aggregate solver statistics. Non-finite residuals or impedances are
/// a hard [`CliError::NonFinite`]. With `out`, each row additionally
/// gets its exported field (and far field) — see [`Exporter::export_row`]; the
/// export solves are not counted in [`SolverStats`].
pub fn sweep(
    p: &Problem,
    out: Option<&OutDir>,
) -> Result<(Vec<FrequencyResult>, SolverStats), CliError> {
    let lumped: Vec<LumpedPort<'_>> = p
        .ports
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
        .collect();
    // Leontovich walls, then Silver-Müller walls (Z_s = η₀ = 1 natural).
    let surfaces: Vec<SurfaceImpedanceBc<'_>> = p
        .leontovich
        .iter()
        .map(|l| SurfaceImpedanceBc {
            triangles: &l.surface.triangles,
            model: SurfaceImpedanceModel::GoodConductor {
                sigma: l.sigma_natural,
            },
        })
        .chain(p.silver_muller.iter().map(|s| SurfaceImpedanceBc {
            triangles: &s.triangles,
            model: SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0)),
        }))
        .collect();
    let mode = solver_mode(p);
    let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();

    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    let t0 = Instant::now();
    let points: Vec<SParameterSweepPoint> = per_material_sweep(p, &omegas, |materials, w| {
        Ok(s_parameter_frequency_sweep_with_mode::<B>(
            &p.tagged.mesh,
            materials,
            None,
            &bcs,
            &lumped,
            &surfaces,
            w,
            mode,
            &device,
        )?)
    })?;
    let wall_time_s = t0.elapsed().as_secs_f64();

    let n = p.ports.len();
    let mut results = Vec::with_capacity(points.len());
    for (index, (pt, f)) in points.iter().zip(&p.frequencies).enumerate() {
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
            iterations: pt.iters_per_rhs.clone(),
            z_ohm: matrix(&z, n),
            y_s: invert(&z, n).map(|y| matrix(&y, n)),
            s: matrix(&pt.s.s, n),
            ports,
            wave_channels: Vec::new(),
            field_file: None,
            far_field: None,
        });
    }

    if let Some(out) = out {
        let exporter = Exporter::<B>::new(p, &lumped, &surfaces, mode, &device)?;
        for (index, (row, f)) in results.iter_mut().zip(&p.frequencies).enumerate() {
            exporter.export_row(out, index, f, row)?;
        }
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
        },
    ))
}

/// Wave-port sweep over `p.frequencies`: solve each port's cross-section
/// modes, run the rank-N SMW wave-port sweep, and report the channel
/// S-matrix plus the ports' solved modes.
pub fn wave_sweep(
    p: &Problem,
) -> Result<(Vec<FrequencyResult>, SolverStats, Vec<WavePortSummary>), CliError> {
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
    let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();
    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    let points: Vec<WavePortSweepPoint> = per_material_sweep(p, &omegas, |materials, w| {
        Ok(solve_wave_port_sweep_with_mode::<B>(
            &p.tagged.mesh,
            materials,
            None,
            &bcs,
            &ports,
            w,
            mode,
            &device,
        )?)
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
            iterations: pt.iters_per_rhs.clone(),
            z_ohm: Vec::new(),
            y_s: None,
            s: matrix(&pt.s, n),
            ports: Vec::new(),
            wave_channels,
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
        },
        summaries,
    ))
}

/// Per-row field / NTFF exporter for a lumped-port spec: the
/// ω-independent [`DrivenOperator`] (built once without UPML; per row
/// with it, since the stretch is ω-dependent) plus the NTFF box.
struct Exporter<'a, B: burn::tensor::backend::Backend> {
    p: &'a Problem,
    lumped: &'a [LumpedPort<'a>],
    surfaces: &'a [SurfaceImpedanceBc<'a>],
    mode: SolverMode,
    device: &'a B::Device,
    /// Zero volume source: the ports are the only drive.
    source: CurrentSource,
    /// The operator of a spec without UPML (ω-independent materials).
    scalar_op: Option<DrivenOperator>,
    /// Tet centroids (UPML tensors), empty without UPML.
    centroids: Vec<[f64; 3]>,
    /// Per-node `Re ε_r` for the `.vtu`.
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
        if p.upml.is_empty() {
            this.scalar_op = Some(this.assemble(DrivenMaterials::Scalar(&p.eps))?);
        } else {
            this.centroids = tet_centroids(mesh);
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
        let upml_op;
        let op = match &self.scalar_op {
            Some(op) => op,
            None => {
                let (epsilon_tensor, nu_tensor) = p.upml_tensors(&self.centroids, omega);
                upml_op = self.assemble(DrivenMaterials::MatchedUpml {
                    epsilon_tensor: &epsilon_tensor,
                    nu_tensor: &nu_tensor,
                })?;
                &upml_op
            }
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
        row.field_file = Some(out.write_field(
            &field_file_name(index),
            mesh,
            &sol.e_edges,
            true,
            &self.eps_nodes,
        )?);

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
