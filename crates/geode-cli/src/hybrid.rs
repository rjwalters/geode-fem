//! **Hybrid wave ports** in the CLI (Epic #778 Phase 5, issue #807).
//!
//! `problem::load` routes a wave port to the hybrid path when its face is
//! inhomogeneous (more than one material) or carries a floating conductor
//! (a PEC strip, sheet or carved hole: microstrip, stripline, coax;
//! [`crate::problem::HybridRoute`]). Such a port's modes come from the
//! full-vector `E_t`–`E_z` port pencil of `geode-core`
//! ([`geode_core::driven::ports::HybridWavePort`]), re-solved at every
//! frequency and tracked across the sweep; the TE-only TM-cutoff guard of
//! geometric ports (#808) does not apply (core checks that every
//! propagating face mode is a reported channel instead).
//!
//! This module runs a spec with at least one hybrid port ([`sweep`]: the
//! core spec sweeps, the dispersive ones when a material is dispersive,
//! geometric ports alongside as [`WavePortSpec::Geometric`]), turns the
//! per-port reports into the report's per-channel
//! [`HybridChannelResult`]s, port summaries and warnings, and previews a
//! hybrid port without the 3-D solve ([`face_sweep`], for `geode check` and
//! the `--touchstone` channel classification).
//!
//! # Defaults (operator decision on #804: design usefulness)
//!
//! * mesh-induced complex evanescent pairs are carried and terminated (core
//!   always sets `carry_complex_pairs`), with a warning;
//! * [`DEFAULT_N_TERMINATION_EVANESCENT`] evanescent modes are terminated
//!   beyond the reported channels;
//! * the per-mode accuracy estimate is on, warning above 0.5 % (configurable)
//!   for `β` and, on a lossy face, for `α`; a failed refined solve degrades
//!   to an "accuracy unavailable" warning;
//! * the line-impedance accuracy estimate is on for every channel with a
//!   line impedance (the `h/2` re-evaluation of `z_line_ohm` at the observed
//!   rate, `geode_core::driven::ports::ImpedanceAccuracy`), warning above
//!   1 % (configurable) for the port's `impedance_definition`, on `check`
//!   and every `driven` run — the impedance `--touchstone` renormalizes to
//!   must never be silently wrong (#807 review);
//! * every warning is in the report (`warnings[]`) and on stderr.
//!
//! [`DEFAULT_N_TERMINATION_EVANESCENT`]: crate::spec::DEFAULT_N_TERMINATION_EVANESCENT

use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::ports::{
    HybridChannelReport, HybridFaceSweep, HybridPortReport, HybridWavePort, LineImpedance,
    PortWarning, PortWarningKind, WavePortSpec, solve_hybrid_port_face_sweep,
    solve_mixed_port_spec_sweep_dispersive_with_mode, solve_mixed_port_spec_sweep_with_mode,
    solve_wave_port_spec_sweep_dispersive_with_mode, solve_wave_port_spec_sweep_with_mode,
};
use geode_core::driven::solve::{DrivenBcs, DrivenError, DrivenMaterials};
use serde_json::json;

use crate::backend::CompiledBackend;
use crate::driven::{ChannelPoint, channel_rows, emit_channel_point, passivity_warnings};
use crate::error::CliError;
use crate::problem::{HybridPortDef, Problem, WavePortDef};
use crate::progress::{Progress, SweepOptions};
use crate::report::{
    FaceConductorSummary, FrequencyResult, HybridChannelResult, HybridModeSummary,
    HybridPointSummary, HybridPortSummary, ImpedanceAccuracyResult, LineImpedanceResult,
    SolverStats, WarningResult, WaveModeSummary, WavePortSummary,
};
use crate::spec::ImpedanceDefinition;

/// The core [`HybridWavePort`] of wave port `w` (its resolved face and
/// options, one channel per `a_inc` entry).
pub fn hybrid_port(w: &WavePortDef, h: &HybridPortDef) -> HybridWavePort {
    HybridWavePort::new(h.face.clone(), w.a_inc.clone()).with_opts(h.opts)
}

/// Hz of the natural frequency `k0` (the spec's own value when it is a
/// sweep point, so a dispersive model is evaluated at exactly that
/// frequency).
fn hz_of(p: &Problem, k0: f64) -> f64 {
    p.frequencies.iter().find(|f| f.k0 == k0).map_or_else(
        || crate::problem::to_frequency(k0, crate::spec::FrequencyUnit::K0, p.length_unit_m()).hz,
        |f| f.hz,
    )
}

/// A core error of a hybrid sweep as a CLI error: a port-level rejection
/// (`InvalidPort`: a propagating mode left unterminated — raise `n_modes` —,
/// a reported channel that would split a degenerate cluster or is a complex
/// pair, …) is `invalid_spec`, naming the hybrid port when `index`
/// identifies one; anything else stays `solve_failed`.
fn hybrid_error(p: &Problem, e: DrivenError) -> CliError {
    match e {
        DrivenError::InvalidPort { index, reason } => {
            let n_lumped = p.ports.len();
            // Core numbers a hybrid port's setup errors after the lumped
            // ports and its per-frequency errors by wave-port index.
            let named = [index.checked_sub(n_lumped), Some(index)]
                .into_iter()
                .flatten()
                .find_map(|i| p.wave_ports.get(i).filter(|w| w.hybrid.is_some()));
            let units = if reason.contains("ω =") {
                " (ω = k₀ in rad per mesh length unit)"
            } else {
                ""
            };
            CliError::InvalidSpec(match named {
                Some(w) => format!("wave port `{}`: {reason}{units}", w.surface.name),
                None => format!("{reason}{units}"),
            })
        }
        e => CliError::Solve(e),
    }
}

/// The port-face half of wave port `index`'s hybrid sweep over the spec's
/// frequencies, without the 3-D solve (`geode check`, `--touchstone`
/// classification): exactly the per-port report and warnings `driven`
/// produces. `accuracy = false` skips the accuracy estimate (classification
/// needs only the modes).
///
/// # Panics
///
/// If wave port `index` is not hybrid.
pub fn face_sweep(p: &Problem, index: usize, accuracy: bool) -> Result<HybridFaceSweep, CliError> {
    let w = &p.wave_ports[index];
    let h = w.hybrid.as_ref().expect("a hybrid wave port");
    let mut port = hybrid_port(w, h);
    if !accuracy {
        port.opts.accuracy = None;
    }
    let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();
    let eps_at = |k0: f64| p.eps_at(hz_of(p, k0)).into_owned();
    // A dispersive spec runs every hybrid port through the dispersive sweep
    // (the face `ε(ω)` read from the volume), so the preview does too.
    let dispersive: Option<&dyn Fn(f64) -> Vec<c64>> =
        (!p.dispersion.is_empty()).then_some(&eps_at);
    solve_hybrid_port_face_sweep(&p.tagged.mesh, &port, index, &omegas, dispersive)
        .map_err(|e| hybrid_error(p, e))
}

/// The report diagnostics of one hybrid channel ([`HybridChannelReport`] →
/// [`HybridChannelResult`]): line impedances (real or, on a lossy face,
/// complex), the even / odd label of a two-conductor channel and the
/// `z_line_ohm` of the port's impedance definition.
pub fn channel_result(h: &HybridPortDef, c: &HybridChannelReport) -> HybridChannelResult {
    let re = |x: f64| [x, 0.0];
    let cx = |z: c64| [z.re, z.im];
    let line = match (&c.line, &c.line_lossy) {
        (Some(l), _) => Some(LineImpedanceResult {
            z_pi_ohm: re(l.z_pi),
            z_pv_ohm: l.z_pv.map(re),
            z_vi_ohm: l.z_vi.map(re),
            currents: l.currents.iter().copied().map(re).collect(),
            voltages: l.voltages.iter().map(|v| v.map(re)).collect(),
        }),
        (None, Some(l)) => Some(LineImpedanceResult {
            z_pi_ohm: cx(l.z_pi),
            z_pv_ohm: l.z_pv.map(cx),
            z_vi_ohm: l.z_vi.map(cx),
            currents: l.currents.iter().copied().map(cx).collect(),
            voltages: l.voltages.iter().map(|v| v.map(cx)).collect(),
        }),
        (None, None) => None,
    };
    // Even / odd: the two conductor currents in phase or in anti-phase
    // (Re(I₁·Ī₂), invariant under the mode's overall sign / phase).
    let coupled_mode = line.as_ref().and_then(|l| match l.currents[..] {
        [a, b] => {
            let x = a[0] * b[0] + a[1] * b[1];
            Some(if x > 0.0 { "even" } else { "odd" })
        }
        _ => None,
    });
    let z_line_ohm = h
        .impedance_definition
        .zip(line.as_ref())
        .and_then(|(d, l)| match d {
            ImpedanceDefinition::PowerCurrent => Some(l.z_pi_ohm),
            ImpedanceDefinition::PowerVoltage => l.z_pv_ohm,
            ImpedanceDefinition::VoltageCurrent => l.z_vi_ohm,
        });
    let z_line_accuracy = h
        .impedance_definition
        .zip(c.impedance_accuracy.as_ref())
        .and_then(|(d, a)| a.get(line_impedance(d)))
        .map(|e| ImpedanceAccuracyResult {
            estimate: e.estimate,
            z_refined_ohm: cx(e.z_refined),
            rate: e.rate,
            rate_observed: e.rate_observed,
        });
    HybridChannelResult {
        z_line_accuracy,
        ez_energy_fraction: c.ez_energy_fraction,
        accuracy: c.accuracy.as_ref().map(|a| a.estimate),
        alpha_accuracy: c.alpha_accuracy,
        residual: c.residual,
        residual_floor: c.residual_floor,
        floor_accepted: c.floor_accepted,
        track_overlap: c.track_overlap,
        cluster_size: c.cluster_size,
        coupled_mode,
        line,
        z_line_ohm,
    }
}

/// The core [`LineImpedance`] of an impedance definition.
pub fn line_impedance(d: ImpedanceDefinition) -> LineImpedance {
    match d {
        ImpedanceDefinition::PowerCurrent => LineImpedance::PowerCurrent,
        ImpedanceDefinition::PowerVoltage => LineImpedance::PowerVoltage,
        ImpedanceDefinition::VoltageCurrent => LineImpedance::VoltageCurrent,
    }
}

/// What the port's impedance definition depends on (report note).
fn impedance_note(d: ImpedanceDefinition) -> &'static str {
    match d {
        ImpedanceDefinition::PowerCurrent => {
            "Z_PI = 2P/sum|I|^2: contour-independent (the default; the definition the 3-D \
             width-step golden validates)"
        }
        ImpedanceDefinition::PowerVoltage | ImpedanceDefinition::VoltageCurrent => {
            "Z_PV / Z_VI use the automatic voltage path from the shield to the nearest conductor \
             point (a microstrip's strip edge): path-dependent, and drifting from Z_PI with \
             frequency (1.01x -> 1.11x over k0*h = 0.05 ... 0.2 on a shielded microstrip)"
        }
    }
}

/// The [`HybridPortSummary`] of hybrid port `w`; `report` (a sweep or
/// face-sweep report) adds the observed rates and the worst tracking
/// overlap.
pub fn summary(
    w: &WavePortDef,
    h: &HybridPortDef,
    report: Option<&HybridPortReport>,
) -> HybridPortSummary {
    let face = &h.face;
    HybridPortSummary {
        reason: h.route.name(),
        physical_groups: w.fill.groups.clone(),
        conductors: face
            .conductors
            .iter()
            .map(|c| FaceConductorSummary {
                centroid: c.centroid,
                voltage_path_nodes: c.voltage_path.len(),
            })
            .collect(),
        n_free_edges: face.max_modes(),
        n_free_nodes: face.free_node_mask.iter().filter(|&&f| f).count(),
        mesh_size: face.mesh_size(),
        lossy: h.lossy,
        dispersive: h.dispersive,
        per_frequency_resolve: true,
        n_termination_evanescent: h.opts.n_termination_evanescent,
        n_termination_evanescent_clamped: h.termination_clamped,
        accuracy_threshold: h.opts.accuracy.map(|a| a.threshold),
        impedance_accuracy_threshold: h
            .opts
            .accuracy
            .filter(|_| h.impedance_definition.is_some())
            .and_then(|a| a.impedance_threshold),
        impedance_definition: h.impedance_definition.map(ImpedanceDefinition::name),
        impedance_note: h.impedance_definition.map(impedance_note),
        observed_rates: report.and_then(|r| r.observed_rates.clone()),
        worst_track_overlap: report.and_then(|r| {
            r.points
                .iter()
                .flat_map(|pt| pt.channels.iter().filter_map(|c| c.track_overlap))
                .reduce(f64::min)
        }),
        frequencies: Vec::new(),
    }
}

/// The per-frequency preview of a face sweep (`geode check`):
/// `channel0` is the port's first flat S-matrix channel.
pub fn point_summaries(
    p: &Problem,
    h: &HybridPortDef,
    report: &HybridPortReport,
    channel0: usize,
) -> Vec<HybridPointSummary> {
    report
        .points
        .iter()
        .zip(&p.frequencies)
        .map(|(pt, f)| HybridPointSummary {
            frequency_hz: f.hz,
            k0: f.k0,
            n_propagating: pt.n_propagating,
            termination_real: pt.termination_real,
            termination_pairs: pt.termination_pairs,
            multiplicity_certified: pt.multiplicity_certified,
            channels: pt
                .channels
                .iter()
                .enumerate()
                .map(|(mode, c)| {
                    let propagating = c.beta.re > c.beta.im.abs();
                    HybridModeSummary {
                        channel: channel0 + mode,
                        mode,
                        beta: [c.beta.re, c.beta.im],
                        propagating,
                        eps_eff: propagating.then(|| (c.beta.re / f.k0).powi(2)),
                        normalization: crate::driven::normalization(c.beta),
                        hybrid: channel_result(h, c),
                    }
                })
                .collect(),
        })
        .collect()
}

/// The report spelling of a core warning kind.
fn warning_kind(k: &PortWarningKind) -> &'static str {
    match k {
        PortWarningKind::ComplexPairTerminated { .. } => "complex_pair_terminated",
        PortWarningKind::ComplexPairDropped { .. } => "complex_pair_dropped",
        PortWarningKind::AccuracyAboveThreshold { .. } => "accuracy_above_threshold",
        PortWarningKind::AttenuationAccuracyAboveThreshold { .. } => {
            "attenuation_accuracy_above_threshold"
        }
        PortWarningKind::AccuracyUnavailable { .. } => "accuracy_unavailable",
        PortWarningKind::ImpedanceAccuracyAboveThreshold { .. } => {
            "impedance_accuracy_above_threshold"
        }
        PortWarningKind::ImpedanceAccuracyUnavailable { .. } => "impedance_accuracy_unavailable",
        PortWarningKind::MultiplicityUncertified { .. } => "multiplicity_uncertified",
        PortWarningKind::ClusterSplit { .. } => "cluster_split",
        PortWarningKind::NonCanonicalClusterBasis { .. } => "non_canonical_cluster_basis",
    }
}

/// Report-level warnings of a hybrid sweep (or face sweep) — the clamped
/// termination notes of the hybrid ports in `ports` (wave-port indices),
/// then the core warnings — each also printed on stderr. Core messages
/// count frequencies in natural units (`ω = k₀`, rad per mesh length unit).
pub fn warnings(p: &Problem, ports: &[usize], core: &[PortWarning]) -> Vec<WarningResult> {
    let mut out = Vec::new();
    for &i in ports {
        let w = &p.wave_ports[i];
        if let Some(h) = w.hybrid.as_ref().filter(|h| h.termination_clamped) {
            out.push(WarningResult {
                kind: "termination_clamped",
                wave_port: Some(i),
                physical_group: Some(w.surface.name.clone()),
                message: format!(
                    "the port face holds only {} physical mode(s), so {} evanescent mode(s) are \
                     terminated beyond the {} reported channel(s) instead of the default {}; \
                     refine the port face for the full termination",
                    h.face.max_modes(),
                    h.opts.n_termination_evanescent,
                    w.a_inc.len(),
                    crate::spec::DEFAULT_N_TERMINATION_EVANESCENT
                ),
            });
        }
    }
    for w in core {
        out.push(WarningResult {
            kind: warning_kind(&w.kind),
            wave_port: Some(w.port),
            physical_group: p.wave_ports.get(w.port).map(|d| d.surface.name.clone()),
            message: format!("{} (ω = k₀ in rad per mesh length unit)", w.message),
        });
    }
    for w in &out {
        match &w.physical_group {
            Some(g) => eprintln!("warning: wave port `{g}`: {}", w.message),
            None => eprintln!("warning: {}", w.message),
        }
    }
    out
}

/// The wave / mixed sweep of a spec with at least one hybrid wave port
/// (issue #807): the core spec sweep over every frequency in one call (so
/// the hybrid modes are tracked), its dispersive variant when a material is
/// dispersive (the volume and every hybrid face read `ε(ω)` from the same
/// per-tet vector), geometric ports alongside at their constant fill. Rows,
/// per-channel hybrid diagnostics, port summaries and warnings as
/// [`crate::driven::wave_sweep`] returns them. Serial, like the geometric
/// wave sweep.
#[allow(clippy::type_complexity)]
pub fn sweep(
    p: &Problem,
    opts: SweepOptions,
) -> Result<
    (
        Vec<FrequencyResult>,
        SolverStats,
        Vec<WavePortSummary>,
        Vec<WarningResult>,
    ),
    CliError,
> {
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
    // `problem::load` rejected UPML and `sweep.adaptive` with hybrid ports,
    // and a dispersive fill on a geometric port of this spec, so the
    // geometric ports' fills are constant.
    let first_hz = p.frequencies.first().map_or(0.0, |f| f.hz);
    let specs: Vec<WavePortSpec> = p
        .wave_ports
        .iter()
        .map(|w| match &w.hybrid {
            Some(h) => Ok(WavePortSpec::from(hybrid_port(w, h))),
            None => {
                let mut port = w.projection.wave_port(&p.edges, &w.a_inc).map_err(|err| {
                    CliError::WavePort {
                        name: w.surface.name.clone(),
                        err,
                    }
                })?;
                port.medium = p.port_medium_at(w, first_hz);
                Ok(WavePortSpec::Geometric(port))
            }
        })
        .collect::<Result<_, CliError>>()?;
    let lumped = crate::driven::lumped_ports(p);
    let surfaces = crate::driven::impedance_walls(p);
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    let mode = crate::driven::solver_mode(p);
    let omegas: Vec<f64> = p.frequencies.iter().map(|f| f.k0).collect();
    type B = CompiledBackend;
    let device = <B as BackendTypes>::Device::default();
    let mesh = &p.tagged.mesh;
    let eps_at = |k0: f64| p.eps_at(hz_of(p, k0)).into_owned();
    let tensors;
    let materials = if p.is_anisotropic() {
        // No UPML: centroids / k0 are not read.
        tensors = p.material_tensors(&p.eps, &[], 0.0);
        DrivenMaterials::MatchedUpml {
            epsilon_tensor: &tensors.0,
            nu_tensor: &tensors.1,
        }
    } else {
        DrivenMaterials::Scalar(&p.eps)
    };
    let err = |e| hybrid_error(p, e);
    let dispersive = !p.dispersion.is_empty();
    let (points, reports, core_warnings): (Vec<ChannelPoint>, _, _) = if lumped.is_empty() {
        let out = if dispersive {
            solve_wave_port_spec_sweep_dispersive_with_mode::<B>(
                mesh, &eps_at, None, &bcs, &specs, &surfaces, &omegas, mode, &device,
            )
        } else {
            solve_wave_port_spec_sweep_with_mode::<B>(
                mesh, materials, None, &bcs, &specs, &surfaces, &omegas, mode, &device,
            )
        }
        .map_err(err)?;
        (
            out.points.into_iter().map(ChannelPoint::from).collect(),
            out.hybrid,
            out.warnings,
        )
    } else {
        let out = if dispersive {
            solve_mixed_port_spec_sweep_dispersive_with_mode::<B>(
                mesh, &eps_at, None, &bcs, &lumped, &specs, &surfaces, &omegas, mode, &device,
            )
        } else {
            solve_mixed_port_spec_sweep_with_mode::<B>(
                mesh, materials, None, &bcs, &lumped, &specs, &surfaces, &omegas, mode, &device,
            )
        }
        .map_err(err)?;
        (
            out.points.into_iter().map(ChannelPoint::from).collect(),
            out.hybrid,
            out.warnings,
        )
    };
    for (index, pt) in points.iter().enumerate() {
        emit_channel_point(&progress, p, index, pt);
    }
    let wall_time_s = t0.elapsed().as_secs_f64();

    let mut results = channel_rows(p, &points)?;
    let report_of = |port: usize| -> &HybridPortReport {
        reports
            .iter()
            .find(|r| r.port == port)
            .expect("core reports every hybrid port")
    };
    for (r, row) in results.iter_mut().enumerate() {
        for ch in &mut row.wave_channels {
            if let Some(h) = &p.wave_ports[ch.port].hybrid {
                let c = &report_of(ch.port).points[r].channels[ch.mode];
                ch.hybrid = Some(channel_result(h, c));
            }
        }
    }

    // Summaries: geometric ports get their solved modes (as the geometric
    // sweep reports them), hybrid ports their sweep diagnostics.
    let hz_per_k0 =
        crate::problem::to_frequency(1.0, crate::spec::FrequencyUnit::K0, p.length_unit_m()).hz;
    let mut summaries = crate::check::wave_port_summaries(p);
    let mut channel = p.ports.len();
    for ((summary, spec), w) in summaries.iter_mut().zip(&specs).zip(&p.wave_ports) {
        match (spec, &w.hybrid) {
            (WavePortSpec::Geometric(port), None) => {
                let medium = p.port_medium_at(w, first_hz);
                summary.modes = Some(
                    port.modes
                        .iter()
                        .enumerate()
                        .map(|(mode, m)| WaveModeSummary {
                            mode,
                            channel: channel + mode,
                            k_c: m.k_c,
                            cutoff_hz: medium.cutoff_k0(m.k_c) * hz_per_k0,
                        })
                        .collect(),
                );
            }
            (_, Some(h)) => {
                summary.hybrid = Some(self::summary(w, h, Some(report_of(summary.index))));
            }
            (WavePortSpec::Hybrid(_), None) => unreachable!("specs follow the routes"),
        }
        channel += w.a_inc.len();
    }

    let hybrid_ports: Vec<usize> = (0..p.wave_ports.len())
        .filter(|&i| p.wave_ports[i].hybrid.is_some())
        .collect();
    let mut warns = warnings(p, &hybrid_ports, &core_warnings);
    warns.extend(passivity_warnings(&results));

    let s = crate::check::solver_summary(p.solver());
    Ok((
        results,
        SolverStats {
            mode: s.mode,
            tol: s.tol,
            max_iters: s.max_iters,
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
        warns,
    ))
}
