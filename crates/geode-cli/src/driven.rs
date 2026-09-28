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
use geode_core::driven::solve::{
    DrivenBcs, DrivenMaterials, IterativeSettings, SolverMode, SurfaceImpedanceBc,
    SurfaceImpedanceModel,
};

use crate::backend::CompiledBackend;
use crate::check::{mesh_summary, port_summaries, silver_muller_summaries, upml_summaries};
use crate::error::CliError;
use crate::problem::{self, Problem};
use crate::report::{
    Complex, DrivenReport, FrequencyResult, PortResult, Provenance, SolverStats, WaveChannelResult,
    WaveModeSummary, WavePortSummary,
};
use crate::spec::{Analysis, SolverSpec};

/// Load, solve and report.
pub fn run(spec_path: &Path, provenance: Provenance) -> Result<DrivenReport, CliError> {
    let p = problem::load(spec_path, Some(Analysis::Driven))?;
    let (results, solver, wave_ports) = if p.wave_ports.is_empty() {
        let (results, solver) = sweep(&p)?;
        (results, solver, Vec::new())
    } else {
        wave_sweep(&p)?
    };
    Ok(DrivenReport {
        provenance,
        kind: "driven",
        status: "ok",
        mesh: mesh_summary(&p),
        ports: port_summaries(&p),
        wave_ports,
        silver_muller: silver_muller_summaries(&p),
        absorbing_regions: upml_summaries(&p),
        solver,
        results,
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
/// a hard [`CliError::NonFinite`].
pub fn sweep(p: &Problem) -> Result<(Vec<FrequencyResult>, SolverStats), CliError> {
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
}
