//! **N-port driven sensitivities** through the CLI (Epic #841 Phase 5a,
//! issue #883): a driven spec's `sensitivity` section with `observables`,
//! `frequencies`, the loss / shape parameter kinds, or any port set other
//! than one lumped port ([`crate::problem::is_n_port_sensitivity`]) adds a
//! `sensitivities` block of `observable = "driven_observables"`.
//!
//! No new adjoint math lives here. The spec's forward — lumped, wave
//! (geometric, filled), mixed, walled and hybrid (microstrip / stripline)
//! ports on the direct-LU scalar-ε operator — is handed to the library's
//! [`s_matrix_sensitivity_sweep`] (Epic #841 Phase 1 / 3b: `∂S_qp/∂θ` for
//! every S entry from the forward's own solves, zero extra solves by
//! reciprocity), and the port-mode observables of hybrid ports come from
//! the same call ([`SSensitivityOptions::port_mode_observables`], Phase 3a's
//! bordered 2-D derivative). This module only binds the spec to the
//! library and chains the real observables:
//!
//! * **parameters** — `eps_r` (`ε′`, `ε″` held) and `eps_r_imag` (`ε″`, `ε′`
//!   held) are the library's `SParam::EpsPrime` / `EpsDoublePrime` of the
//!   group's region; `tan_delta` (`ε′` held) is `ε′·∂/∂ε″`; a `shape`
//!   parameter is one column of
//!   [`ShapeDesign::from_group_motions`] (the named groups' motion, pinned
//!   groups, harmonic extension), its gradient per metre (`÷ length_unit_m`).
//!   A geometric wave port whose fill is a design region is bound to it
//!   (`SDesign::port_fill`), so its `β`, drive and power weights follow the
//!   region (#777);
//! * **observables** — with `z` complex and `∂z` its holomorphic
//!   derivative along a real parameter: `real` = `Re ∂z`, `imag` = `Im ∂z`,
//!   `mag_sq` = `2 Re(z̄ ∂z)`, `mag` = `Re(z̄ ∂z)/|z|`, `db` =
//!   `(20/ln 10)·Re(z̄ ∂z)/|z|²`, `phase_deg` = `(180/π)·Im(z̄ ∂z)/|z|²`;
//!   `s_sum_sq` sums `mag_sq`.
//!
//! **Forward parity.** The sensitivity solves its own forward on the same
//! operator; every differentiated frequency's S matrix is compared to the
//! report's `results[].s` ([`FORWARD_PARITY_TOL`]) and port-mode values to the
//! report's `ε_eff` / `z_line_ohm` ([`PORT_MODE_PARITY_TOL`]), so a gradient is
//! never reported for a different forward than the one in the report. A
//! hybrid spec runs the sensitivity over every swept frequency (mode
//! tracking is a sweep-wide assignment) and reports the selected ones.
//!
//! **FD self-check** (`fd_check` / `--check-gradient`): every parameter is
//! re-run through the **shipped** `geode driven` forward
//! ([`crate::driven::sweep`] / [`crate::driven::wave_sweep`]) at `p ± h`
//! — material: the region's `ε_r` changed; shape: the mesh nodes moved by
//! `±h` times the parameter's column (never re-meshed), the hybrid faces
//! rebuilt on the moved mesh — and each entry is compared to the central
//! difference. A loss parameter within one step of its lossless bound
//! (`ε″ = 0`, `tan δ = 0`) uses the one-sided second-order difference
//! `(f(2h)·(−1) + 4f(h) − 3f(0))/(2h)`: `ε″ < 0` is gain, and a filled
//! port's outgoing branch flips there (the library's passive-side
//! derivative). The step is `relative_step` times the parameter's natural
//! scale (`|ε′|`, `|ε_r|` for `ε″`, `1` for `tan δ`, the motion length for
//! shape), and the disagreement is floored at the central difference's
//! round-off (see the README).

use std::time::Instant;

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::driven::ports::WavePortSpec;
use geode_core::driven::s_sensitivity::{
    MaterialDesign, PortModeObservables, SDesign, SNetwork, SSensitivityError, SSensitivityOptions,
    SSensitivitySweep, ShapeDesign, s_matrix_sensitivity_sweep,
};
use geode_core::driven::solve::{DrivenBcs, DrivenMaterials, ElementOrder};

use crate::backend::CompiledBackend;
use crate::error::CliError;
use crate::problem::{ObservableDef, Problem, SensitivityParameter, SensitivityTarget};
use crate::progress::SweepOptions;
use crate::report::{
    FdCheckSummary, FrequencyResult, SensitivityEntry, SensitivityObservableSummary,
    SensitivityParameterSummary, SensitivityReport, WarningResult,
};
use crate::sensitivity::{FD_SCALE_FLOOR, rel_error};
use crate::spec::{
    ImpedanceDefinition, ObservableForm, ObservableQuantity, SensitivityParameterKind,
};

/// Largest accepted relative difference between the sensitivity's S matrix
/// and the report's (two forwards of one operator: round-off apart).
pub const FORWARD_PARITY_TOL: f64 = 1e-8;
/// Largest accepted relative difference between a port-mode value of the
/// sensitivity (Phase 3a's evaluator, `zᵀBz` pseudo-power) and the report's
/// (the shipped readout): the two agree on eigenmodes to the eigen-solve
/// residual.
pub const PORT_MODE_PARITY_TOL: f64 = 1e-6;
/// Relative round-off of one forward solve's observable, the noise floor of
/// a central difference: an entry whose gradient and FD estimate both sit
/// below `FD_ROUNDOFF · M / h` (`M` the observable's magnitude scale, `h`
/// the step) is structurally zero — e.g. `|S21|` of a uniform guide under
/// a rigid shift of a slab — and is not judged against the difference of
/// two round-off-level numbers.
const FD_ROUNDOFF: f64 = 1e-10;

type B = CompiledBackend;

/// How a CLI parameter maps onto the library's flat parameter list.
#[derive(Debug, Clone, Copy)]
struct ParamMap {
    /// Index into [`SSensitivitySweep::params`].
    index: usize,
    /// Chain factor (`ε′` for `tan δ`, `1/length_unit_m` for shape).
    factor: f64,
}

/// The library design of the spec's parameters, the CLI → library map and
/// the shape columns (mesh units per unit parameter in mesh units).
struct Design {
    sdesign: SDesign,
    /// Per CLI parameter: the library parameter of its derivative.
    map: Vec<ParamMap>,
    /// Per CLI parameter: its node-motion column (shape parameters only).
    columns: Vec<Option<Vec<[f64; 3]>>>,
}

/// The library error of the sensitivity, as a CLI error: every fenced or
/// inconsistent combination (`Unsupported`, `InvalidDesign`, a degenerate
/// face-mode cluster) is the spec's — `invalid_spec`, with the library's
/// message naming what lifts it; a forward failure is `solve_failed`.
fn lib_error(e: SSensitivityError) -> CliError {
    match e {
        SSensitivityError::Driven(e) => CliError::Solve(e),
        e @ (SSensitivityError::Unsupported { .. }
        | SSensitivityError::InvalidDesign(_)
        | SSensitivityError::DegenerateCluster { .. }) => {
            CliError::InvalidSpec(format!("`sensitivity`: {e}"))
        }
        e => CliError::SensitivitySolve(e.to_string()),
    }
}

/// Build the library design: one material region per distinct volume group
/// with a material parameter (in order of first appearance), the
/// geometric-port fills that follow a design region, and one shape column
/// per shape parameter.
fn design(p: &Problem, sens: &SensitivityTarget) -> Result<Design, CliError> {
    let mut regions: Vec<usize> = Vec::new();
    let (mut need_prime, mut need_dprime) = (false, false);
    for prm in sens.parameters.iter().filter(|q| q.kind.is_material()) {
        if !regions.contains(&prm.region) {
            regions.push(prm.region);
        }
        match prm.kind {
            SensitivityParameterKind::EpsR => need_prime = true,
            _ => need_dprime = true,
        }
    }
    let material = if regions.is_empty() {
        None
    } else {
        let labels: Vec<Option<usize>> = sens
            .region_of_tet
            .iter()
            .map(|r| regions.iter().position(|q| q == r))
            .collect();
        let names = regions.iter().map(|&r| p.regions[r].name.clone()).collect();
        Some(
            MaterialDesign::from_regions(labels, names)
                .map_err(lib_error)?
                .with_components(need_prime, need_dprime),
        )
    };
    // A geometric port filled by a design region follows it (#777): bind it.
    let port_fill: Vec<Option<usize>> = p
        .wave_ports
        .iter()
        .map(|w| {
            if w.hybrid.is_some() {
                return None;
            }
            let r = sens.region_of_tet[w.fill.tet];
            regions.iter().position(|&q| q == r)
        })
        .collect();
    let mut columns = Vec::with_capacity(sens.parameters.len());
    let mut shape_cols = Vec::new();
    let mut shape_names = Vec::new();
    for prm in &sens.parameters {
        match &prm.shape {
            Some(sh) => {
                let pinned: Vec<&str> = sh.pinned.iter().map(String::as_str).collect();
                let one = ShapeDesign::from_group_motions(
                    &p.tagged,
                    std::slice::from_ref(&sh.motion),
                    &pinned,
                )
                .map_err(lib_error)?;
                let col = one.column(0).to_vec();
                shape_cols.push(col.clone());
                shape_names.push(sh.name.clone());
                columns.push(Some(col));
            }
            None => columns.push(None),
        }
    }
    let shape = if shape_cols.is_empty() {
        None
    } else {
        Some(ShapeDesign::from_columns(shape_cols, shape_names).map_err(lib_error)?)
    };
    let sdesign = SDesign {
        material,
        shape,
        port_fill,
    };
    // The library's flat order: every region's ε′ (if selected), then ε″,
    // then the shape columns.
    let n_reg = regions.len();
    let dprime_base = if need_prime { n_reg } else { 0 };
    let shape_base = dprime_base + if need_dprime { n_reg } else { 0 };
    let mut next_shape = 0;
    let map = sens
        .parameters
        .iter()
        .map(|prm| {
            let slot = |r: usize| regions.iter().position(|&q| q == r).expect("a region");
            match prm.kind {
                SensitivityParameterKind::EpsR => ParamMap {
                    index: slot(prm.region),
                    factor: 1.0,
                },
                SensitivityParameterKind::EpsRImag => ParamMap {
                    index: dprime_base + slot(prm.region),
                    factor: 1.0,
                },
                SensitivityParameterKind::TanDelta => ParamMap {
                    index: dprime_base + slot(prm.region),
                    factor: p.regions[prm.region].eps_r.re,
                },
                SensitivityParameterKind::Shape => {
                    let m = ParamMap {
                        index: shape_base + next_shape,
                        factor: 1.0 / p.length_unit_m(),
                    };
                    next_shape += 1;
                    m
                }
                SensitivityParameterKind::NuR | SensitivityParameterKind::MuR => {
                    unreachable!("rejected for driven specs by problem::load")
                }
            }
        })
        .collect();
    Ok(Design {
        sdesign,
        map,
        columns,
    })
}

/// The spec's wave ports as library [`WavePortSpec`]s, exactly as the
/// shipped sweep builds them (geometric ports at their constant fill, hybrid
/// ports with the accuracy estimate off — it is a diagnostic, not part of S).
fn wave_specs(p: &Problem) -> Result<Vec<WavePortSpec>, CliError> {
    p.wave_ports
        .iter()
        .map(|w| match &w.hybrid {
            Some(h) => {
                let mut port = crate::hybrid::hybrid_port(w, h);
                port.opts.accuracy = None;
                Ok(WavePortSpec::from(port))
            }
            None => {
                let mut port = w.projection.wave_port(&p.edges, &w.a_inc).map_err(|err| {
                    CliError::WavePort {
                        name: w.surface.name.clone(),
                        err,
                    }
                })?;
                port.medium = p.port_medium(w);
                Ok(WavePortSpec::Geometric(port))
            }
        })
        .collect()
}

/// Run the library sensitivity on the spec's forward at `omegas`
/// (`port_modes`: also the hybrid port-mode observables). The golden
/// `tests/sensitivity_nport.rs` rebuilds this network in-process from the
/// mesh and checks the CLI's gradients against it bit for bit.
fn library_sweep(
    p: &Problem,
    design: &SDesign,
    omegas: &[f64],
    port_modes: bool,
) -> Result<SSensitivitySweep, CliError> {
    let mesh = &p.tagged.mesh;
    let lumped = crate::driven::lumped_ports(p);
    let walls = crate::driven::impedance_walls(p);
    let wave = wave_specs(p)?;
    let bcs = DrivenBcs {
        pec_interior_mask: &p.pec_mask,
    };
    // The CLI has no element-order field yet (Epic #836 Phase 5a); a p=2
    // space would be the library's typed `Unsupported` (#836 Phase 4).
    let space = HcurlSpace::build(mesh, ElementOrder::P1);
    let net = SNetwork {
        space: &space,
        mesh,
        materials: DrivenMaterials::Scalar(&p.eps),
        sigma_tet: None,
        bcs: &bcs,
        lumped: &lumped,
        wave: &wave,
        surfaces: &walls,
    };
    let opts = SSensitivityOptions {
        solver_mode: crate::driven::solver_mode(p),
        port_mode_observables: port_modes,
        ..SSensitivityOptions::default()
    };
    let device = <B as BackendTypes>::Device::default();
    s_matrix_sensitivity_sweep::<B>(&net, omegas, design, &opts, &device).map_err(lib_error)
}

/// `(value, ∂value)` of a real form of complex `z` with derivative `dz`.
fn form_of(form: ObservableForm, z: c64, dz: c64) -> (f64, f64) {
    let m2 = z.norm_sqr();
    let zd = z.conj() * dz;
    match form {
        ObservableForm::Real => (z.re, dz.re),
        ObservableForm::Imag => (z.im, dz.im),
        ObservableForm::MagSq => (m2, 2.0 * zd.re),
        ObservableForm::Mag => (m2.sqrt(), zd.re / m2.sqrt()),
        ObservableForm::Db => (
            20.0 * m2.sqrt().log10(),
            20.0 / std::f64::consts::LN_10 * zd.re / m2,
        ),
        ObservableForm::PhaseDeg => (z.im.atan2(z.re).to_degrees(), (zd.im / m2).to_degrees()),
    }
}

/// The magnitude scale `M` of a form's value for the FD round-off floor:
/// `max(|value|, 1)`, times `20/ln 10` for dB and `180/π` for degrees (the
/// value's change per unit relative change of `z`).
fn magnitude_scale(form: ObservableForm, value: f64) -> f64 {
    match form {
        ObservableForm::Db => 20.0 / std::f64::consts::LN_10,
        ObservableForm::PhaseDeg => 180.0 / std::f64::consts::PI,
        _ => value.abs().max(1.0),
    }
}

/// The value of a real form of `z` (the FD forward's readout).
fn form_value(form: ObservableForm, z: c64) -> f64 {
    form_of(form, z, c64::new(0.0, 0.0)).0
}

/// The line impedance of the port's definition.
fn z_of_definition(
    d: ImpedanceDefinition,
    line: &geode_core::analytic::port_mode_sensitivity::LineImpedances,
) -> Option<c64> {
    match d {
        ImpedanceDefinition::PowerCurrent => Some(line.z_pi),
        ImpedanceDefinition::PowerVoltage => line.z_pv,
        ImpedanceDefinition::VoltageCurrent => line.z_vi,
    }
}

/// `∂Z` of the port's definition, per library parameter.
fn dz_of_definition(d: ImpedanceDefinition, o: &PortModeObservables) -> Option<&[c64]> {
    match d {
        ImpedanceDefinition::PowerCurrent => o.d_z_pi.as_deref(),
        ImpedanceDefinition::PowerVoltage => o.d_z_pv.as_deref(),
        ImpedanceDefinition::VoltageCurrent => o.d_z_vi.as_deref(),
    }
}

/// The impedance definition of hybrid wave port `k`.
fn definition(p: &Problem, k: usize) -> ImpedanceDefinition {
    p.wave_ports[k]
        .hybrid
        .as_ref()
        .and_then(|h| h.impedance_definition)
        .unwrap_or(ImpedanceDefinition::PowerCurrent)
}

/// The S-channel index of wave port `k`'s reported channel `mode`.
fn wave_channel(p: &Problem, k: usize, mode: usize) -> usize {
    p.ports.len()
        + p.wave_ports[..k]
            .iter()
            .map(|w| w.a_inc.len())
            .sum::<usize>()
        + mode
}

/// One report row's S entry `[i][j]`.
fn row_s(row: &FrequencyResult, i: usize, j: usize) -> c64 {
    let z = row.s[i][j];
    c64::new(z[0], z[1])
}

/// The complex port-mode value of observable `o` in a forward report row
/// (`ε_eff = β²/k₀²` from the channel's `β`, `z0` the row's
/// `z_line_ohm`), or `None` when the row does not carry it.
fn row_port_mode(p: &Problem, o: &ObservableDef, row: &FrequencyResult) -> Option<c64> {
    let k = o.wave_port?;
    let c = wave_channel(p, k, o.mode);
    let ch = row.wave_channels.iter().find(|ch| ch.channel == c)?;
    match o.quantity {
        ObservableQuantity::EpsEff => {
            let b = c64::new(ch.beta[0], ch.beta[1]);
            Some(b * b / (row.k0 * row.k0))
        }
        ObservableQuantity::Z0 => {
            let z = ch.hybrid.as_ref()?.z_line_ohm?;
            Some(c64::new(z[0], z[1]))
        }
        _ => None,
    }
}

/// The real value of observable `o` in a forward report row (the FD
/// readout).
fn row_value(p: &Problem, o: &ObservableDef, row: &FrequencyResult) -> Result<f64, CliError> {
    match o.quantity {
        ObservableQuantity::S => {
            let [i, j] = o.entries[0];
            Ok(form_value(o.form, row_s(row, i, j)))
        }
        ObservableQuantity::SSumSq => Ok(o
            .entries
            .iter()
            .map(|&[i, j]| row_s(row, i, j).norm_sqr())
            .sum()),
        ObservableQuantity::Z0 | ObservableQuantity::EpsEff => row_port_mode(p, o, row)
            .map(|z| form_value(o.form, z))
            .ok_or_else(|| {
                CliError::SensitivitySolve(format!(
                    "the FD forward at {} Hz carries no `{}` for {} (the channel stopped \
                     propagating, or lost its line impedance, under the perturbation — lower \
                     sensitivity.fd_check.relative_step)",
                    row.frequency_hz,
                    o.quantity.name(),
                    o.label
                ))
            }),
    }
}

/// The value and per-library-parameter complex derivative of observable
/// `o` at one sensitivity point.
fn point_observable(
    p: &Problem,
    o: &ObservableDef,
    pt: &geode_core::driven::s_sensitivity::SSensitivityPoint,
    n_lib: usize,
) -> Result<(f64, Vec<f64>), CliError> {
    let n = pt.n_ports;
    match o.quantity {
        ObservableQuantity::S => {
            let [i, j] = o.entries[0];
            let z = pt.s[i * n + j];
            let value = form_value(o.form, z);
            let grad = (0..n_lib)
                .map(|t| form_of(o.form, z, pt.ds[t][i * n + j]).1)
                .collect();
            Ok((value, grad))
        }
        ObservableQuantity::SSumSq => {
            let value = o
                .entries
                .iter()
                .map(|&[i, j]| pt.s[i * n + j].norm_sqr())
                .sum();
            let grad = (0..n_lib)
                .map(|t| {
                    o.entries
                        .iter()
                        .map(|&[i, j]| 2.0 * (pt.s[i * n + j].conj() * pt.ds[t][i * n + j]).re)
                        .sum()
                })
                .collect();
            Ok((value, grad))
        }
        ObservableQuantity::Z0 | ObservableQuantity::EpsEff => {
            let k = o.wave_port.expect("a port-mode observable");
            let entry = pt
                .port_modes
                .iter()
                .find(|e| e.port == k && e.channel == o.mode)
                .ok_or_else(|| {
                    CliError::SensitivitySolve(format!(
                        "no port-mode observables for {} at ω = {}",
                        o.label, pt.omega
                    ))
                })?;
            let obs = entry.result.as_ref().map_err(|msg| {
                CliError::InvalidSpec(format!(
                    "`sensitivity`: {} at k0 = {}: the port-mode derivative is undefined: {msg}",
                    o.label, pt.omega
                ))
            })?;
            let (z, dz): (c64, &[c64]) = match o.quantity {
                ObservableQuantity::EpsEff => (obs.eps_eff, &obs.d_eps_eff),
                _ => {
                    let d = definition(p, k);
                    let (Some(z), Some(dz)) = (
                        obs.line.as_ref().and_then(|l| z_of_definition(d, l)),
                        dz_of_definition(d, obs),
                    ) else {
                        return Err(CliError::InvalidSpec(format!(
                            "`sensitivity`: {} at k0 = {}: the `{}` line impedance is \
                             undefined for this channel (no voltage path from the shield to \
                             the conductor) — use `power_current`",
                            o.label,
                            pt.omega,
                            d.name()
                        )));
                    };
                    (z, dz)
                }
            };
            let value = form_value(o.form, z);
            let grad = dz.iter().map(|&d| form_of(o.form, z, d).1).collect();
            Ok((value, grad))
        }
    }
}

/// The report's summary of observable `o`.
fn observable_summary(p: &Problem, o: &ObservableDef) -> SensitivityObservableSummary {
    SensitivityObservableSummary {
        quantity: o.quantity.name(),
        form: o.form.name(),
        label: o.label.clone(),
        unit: o.unit,
        entries: o.entries.clone(),
        wave_port: o.wave_port.map(|k| p.wave_ports[k].surface.name.clone()),
        mode: o.wave_port.map(|_| o.mode),
        impedance_definition: (o.quantity == ObservableQuantity::Z0)
            .then(|| definition(p, o.wave_port.expect("z0")).name()),
    }
}

/// Every named 2-D group a column moves (by more than 1e-9 of its largest
/// node motion).
fn moved_groups(p: &Problem, col: &[[f64; 3]]) -> Vec<String> {
    let norm = |v: &[f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let max = col.iter().map(norm).fold(0.0, f64::max);
    let mut out: Vec<String> = p
        .tagged
        .mesh
        .physical_groups
        .iter()
        .filter(|((dim, _), _)| *dim == 2)
        .filter(|((_, tag), _)| {
            p.tagged
                .triangles_with_tag(*tag)
                .iter()
                .flatten()
                .any(|&n| norm(&col[n as usize]) > 1e-9 * max)
        })
        .map(|(_, name)| name.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Relative motion below which a node (or a node's normal motion) counts as
/// still: the harmonic extension is exact zero on pinned / untouched
/// components, so this only absorbs round-off.
const STILL_TOL: f64 = 1e-9;

/// Warnings of shape parameter `prm` whose column is `col` (never errors:
/// the gradient is of a legitimate, if probably unintended, morph):
///
/// * `sensitivity_shape_rigid` — every node moves with one velocity (no
///   `pinned` group, so the harmonic extension is a constant): the whole
///   model translates rigidly and every gradient is zero by construction;
/// * `sensitivity_shape_moves_boundary` — a `pec` / `leontovich` /
///   `silver_muller` group that is not one of the parameter's own groups
///   has a node (not one of the moving groups' nodes) moving along its
///   face normal, i.e. the wall itself is reshaped or displaced. Motion in
///   a wall's own plane (a wall sliding with a slab it bounds) and hybrid
///   wave-port faces (which move in-plane with the cross-section by
///   design) are not flagged.
fn shape_warnings(p: &Problem, prm: &SensitivityParameter, col: &[[f64; 3]]) -> Vec<WarningResult> {
    let Some(sh) = prm.shape.as_ref() else {
        return Vec::new();
    };
    let norm = |v: &[f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let max = col.iter().map(norm).fold(0.0, f64::max);
    if max == 0.0 || !max.is_finite() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let v0 = col[0];
    let rigid = col
        .iter()
        .all(|v| norm(&[v[0] - v0[0], v[1] - v0[1], v[2] - v0[2]]) <= STILL_TOL * max);
    if rigid {
        let w = WarningResult {
            kind: "sensitivity_shape_rigid",
            wave_port: None,
            physical_group: None,
            message: format!(
                "shape parameter `{}` moves every node of the model with one velocity: with \
                 no `pinned` group the motion's harmonic extension is a constant, so the whole \
                 model translates rigidly and every gradient of `{}` is zero by construction — \
                 add the fixed groups (shield, ground, port faces, outer walls) to `pinned`",
                sh.name, sh.name
            ),
        };
        eprintln!("warning: {}", w.message);
        out.push(w);
    }
    let own: std::collections::HashSet<u32> = sh
        .motion
        .nodes(&p.tagged)
        .map(|n| n.into_iter().collect())
        .unwrap_or_default();
    let walls = p
        .pec
        .iter()
        .map(|s| ("pec", s))
        .chain(p.leontovich.iter().map(|l| ("leontovich", &l.surface)))
        .chain(p.silver_muller.iter().map(|s| ("silver_muller", s)));
    let mut flagged: Vec<(&str, &str)> = Vec::new();
    for (bc, surf) in walls {
        if sh.motion.groups.contains(&surf.name) || flagged.iter().any(|(_, n)| *n == surf.name) {
            continue;
        }
        let moves_normal = surf.triangles.iter().any(|t| {
            let x = |i: usize| p.tagged.mesh.nodes[t[i] as usize];
            let (a, b, c) = (x(0), x(1), x(2));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let n = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let nn = norm(&n);
            if nn == 0.0 {
                return false;
            }
            t.iter().filter(|k| !own.contains(k)).any(|&k| {
                let m = col[k as usize];
                (m[0] * n[0] + m[1] * n[1] + m[2] * n[2]).abs() / nn > STILL_TOL * max
            })
        });
        if moves_normal {
            flagged.push((bc, &surf.name));
        }
    }
    for (bc, name) in flagged {
        let w = WarningResult {
            kind: "sensitivity_shape_moves_boundary",
            wave_port: None,
            physical_group: Some(name.to_string()),
            message: format!(
                "shape parameter `{}` moves the `{bc}` boundary `{name}` (not one of its own \
                 groups) off its plane: the gradient is of a morph that also reshapes or \
                 displaces that wall — add `{name}` to `pinned` if it is fixed",
                sh.name
            ),
        };
        eprintln!("warning: {}", w.message);
        out.push(w);
    }
    out
}

/// The parameter's summary in the report.
fn parameter_summary(
    p: &Problem,
    prm: &SensitivityParameter,
    col: Option<&Vec<[f64; 3]>>,
) -> SensitivityParameterSummary {
    let sh = prm.shape.as_ref();
    SensitivityParameterSummary {
        physical_group: prm.physical_group.clone(),
        kind: prm.kind.name(),
        value: prm.value,
        name: sh.map(|s| s.name.clone()),
        motion: sh.map(|s| s.shape_motion.name()),
        axis: sh.map(|s| match s.motion.kind {
            geode_core::driven::s_sensitivity::GroupMotionKind::Translate { dir } => dir,
            geode_core::driven::s_sensitivity::GroupMotionKind::Stretch { axis } => axis,
        }),
        pinned: sh.map(|s| s.pinned.clone()).unwrap_or_default(),
        moved_groups: col.map(|c| moved_groups(p, c)),
        unit: Some(if sh.is_some() { "m" } else { "1" }),
    }
}

/// The parameter's natural scale (the FD step and floor scale), in its own
/// unit: `|ε′|` for `eps_r`; `|ε_r|` for `eps_r_imag` and `1` for
/// `tan δ` — a loss step is a step of the same relative size in `ε_r` as an
/// `eps_r` step (a step relative to a small `ε″` itself sits in the solve's
/// round-off); a shape parameter's motion length (the stretch extent, else
/// the moving nodes' bounding-box size).
fn parameter_scale(p: &Problem, prm: &SensitivityParameter) -> f64 {
    match (&prm.shape, prm.kind) {
        (Some(sh), _) => sh.fd_length * p.length_unit_m(),
        (None, SensitivityParameterKind::EpsRImag) => p.regions[prm.region].eps_r.norm(),
        (None, SensitivityParameterKind::TanDelta) => 1.0,
        _ => prm.value.abs(),
    }
}

/// The spec's problem with parameter `prm` moved by `delta` (its own unit)
/// from the design point: the region's `ε_r` changed, or every node moved
/// by `delta` (in mesh units: `÷ length_unit_m`) times the column, the
/// hybrid faces rebuilt and the accuracy estimate off.
fn perturbed(
    p: &Problem,
    prm: &SensitivityParameter,
    col: Option<&Vec<[f64; 3]>>,
    delta: f64,
) -> Result<Problem, CliError> {
    let mut q = p.clone();
    match (prm.kind, col) {
        (SensitivityParameterKind::Shape, Some(col)) => {
            let t = delta / p.length_unit_m();
            for (x, v) in q.tagged.mesh.nodes.iter_mut().zip(col) {
                x[0] += t * v[0];
                x[1] += t * v[1];
                x[2] += t * v[2];
            }
            let ratio = geode_core::shape::min_tet_volume_ratio(&p.tagged.mesh, &q.tagged.mesh);
            if ratio.is_nan() || ratio <= 0.0 {
                return Err(CliError::InvalidSpec(format!(
                    "`sensitivity` FD check: the step {delta:e} m of shape parameter `{}` \
                     inverts a tet (min volume ratio {ratio:.3e}) — lower \
                     sensitivity.fd_check.relative_step",
                    prm.physical_group
                )));
            }
        }
        (kind, _) => {
            for (e, &r) in q
                .eps
                .iter_mut()
                .zip(&p.sensitivity.as_ref().expect("a target").region_of_tet)
            {
                if r != prm.region {
                    continue;
                }
                *e = match kind {
                    SensitivityParameterKind::EpsR => c64::new(prm.value + delta, e.im),
                    SensitivityParameterKind::EpsRImag => c64::new(e.re, -(prm.value + delta)),
                    SensitivityParameterKind::TanDelta => {
                        c64::new(e.re, -e.re * (prm.value + delta))
                    }
                    _ => unreachable!("driven material kinds"),
                };
            }
        }
    }
    for w in &mut q.wave_ports {
        if let Some(h) = &mut w.hybrid {
            h.opts.accuracy = None;
        }
    }
    q.rebuild_hybrid_faces()?;
    Ok(q)
}

/// The shipped driven forward's report rows of `q`.
fn forward_rows(q: &Problem, jobs: usize) -> Result<Vec<FrequencyResult>, CliError> {
    let opts = SweepOptions {
        jobs: if q.wave_ports.is_empty() { jobs } else { 1 },
        jobs_explicit: false,
        progress: false,
    };
    Ok(if q.wave_ports.is_empty() {
        crate::driven::sweep(q, None, "driven", opts)?.0
    } else {
        crate::driven::wave_sweep(q, opts)?.0
    })
}

/// `∂(observable)/∂(parameter)` of every N-port observable at every
/// selected frequency (module docs). `rows` are the report's own forward
/// rows (every swept frequency, in order).
pub fn driven(
    p: &Problem,
    sens: &SensitivityTarget,
    rows: &[FrequencyResult],
    jobs: usize,
) -> Result<SensitivityReport, CliError> {
    let t0 = Instant::now();
    let d = design(p, sens)?;
    let hybrid = p.wave_ports.iter().any(|w| w.hybrid.is_some());
    // Hybrid tracking is a sweep-wide assignment: run every frequency then.
    let solved: Vec<usize> = if hybrid {
        (0..p.frequencies.len()).collect()
    } else {
        sens.frequency_indices.clone()
    };
    let omegas: Vec<f64> = solved.iter().map(|&i| p.frequencies[i].k0).collect();
    let port_modes = sens.observables.iter().any(|o| o.wave_port.is_some());
    let sw = library_sweep(p, &d.sdesign, &omegas, port_modes)?;
    let n_lib = sw.params.len();
    let point_of = |fi: usize| -> &geode_core::driven::s_sensitivity::SSensitivityPoint {
        let at = solved.iter().position(|&s| s == fi).expect("solved");
        &sw.points[at]
    };

    // Forward parity against the report's own rows.
    let mut parity = 0.0_f64;
    let mut warnings: Vec<WarningResult> = sens
        .parameters
        .iter()
        .zip(&d.columns)
        .filter_map(|(prm, col)| col.as_ref().map(|c| shape_warnings(p, prm, c)))
        .flatten()
        .collect();
    for &fi in &sens.frequency_indices {
        let pt = point_of(fi);
        let row = &rows[fi];
        let n = pt.n_ports;
        if row.s.len() != n {
            return Err(CliError::SensitivitySolve(format!(
                "the sensitivity's S matrix is {n}×{n}, the report's {}×{}",
                row.s.len(),
                row.s.len()
            )));
        }
        let scale = (0..n * n)
            .map(|k| row_s(row, k / n, k % n).norm())
            .fold(0.0, f64::max)
            .max(f64::MIN_POSITIVE);
        for k in 0..n * n {
            parity = parity.max((pt.s[k] - row_s(row, k / n, k % n)).norm() / scale);
        }
        for o in sens.observables.iter().filter(|o| o.wave_port.is_some()) {
            if let (Some(want), Some(got)) = (row_port_mode(p, o, row), port_mode_complex(p, o, pt))
            {
                let rel = (got - want).norm() / want.norm().max(f64::MIN_POSITIVE);
                if rel.is_nan() || rel > PORT_MODE_PARITY_TOL {
                    return Err(CliError::SensitivitySolve(format!(
                        "{} at {} Hz: the sensitivity's value {got} differs from the report's \
                         {want} (rel {rel:.3e} > {PORT_MODE_PARITY_TOL:e})",
                        o.label, row.frequency_hz
                    )));
                }
            }
        }
        for note in &pt.port_mode_warnings {
            let w = &p.wave_ports[note.port];
            warnings.push(WarningResult {
                kind: "sensitivity_port_mode",
                wave_port: Some(note.port),
                physical_group: Some(w.surface.name.clone()),
                message: format!(
                    "sensitivity at {} Hz, wave port `{}` channel {}: {:?} — the gradient of a \
                     near-degenerate face mode holds only within a parameter radius of order \
                     the gap; keep design steps small or separate the modes",
                    row.frequency_hz, w.surface.name, note.channel, note.warning
                ),
            });
        }
    }
    if parity.is_nan() || parity > FORWARD_PARITY_TOL {
        return Err(CliError::SensitivitySolve(format!(
            "the sensitivity's forward S differs from the report's by {parity:.3e} (relative) > \
             {FORWARD_PARITY_TOL:e}: the gradient would not be of the reported forward"
        )));
    }

    // Values and gradients: entries[k][o][f].
    let n_obs = sens.observables.len();
    let n_f = sens.frequency_indices.len();
    let mut values = vec![vec![0.0; n_f]; n_obs];
    let mut grads = vec![vec![vec![0.0; n_f]; n_obs]; sens.parameters.len()];
    for (oi, o) in sens.observables.iter().enumerate() {
        for (si, &fi) in sens.frequency_indices.iter().enumerate() {
            let (v, g) = point_observable(p, o, point_of(fi), n_lib)?;
            values[oi][si] = v;
            for (k, m) in d.map.iter().enumerate() {
                grads[k][oi][si] = g[m.index] * m.factor;
            }
        }
    }
    let mut entries = Vec::with_capacity(sens.parameters.len() * n_obs * n_f);
    for (k, prm) in sens.parameters.iter().enumerate() {
        for (oi, o) in sens.observables.iter().enumerate() {
            for (si, &fi) in sens.frequency_indices.iter().enumerate() {
                let (value, gradient) = (values[oi][si], grads[k][oi][si]);
                if !(value.is_finite() && gradient.is_finite()) {
                    return Err(CliError::NonFinite {
                        index: fi,
                        what: format!(
                            "sensitivity of {} w.r.t. {} is {gradient} (value {value})",
                            o.label, prm.physical_group
                        ),
                    });
                }
                entries.push(SensitivityEntry {
                    parameter: k,
                    index: vec![fi],
                    value,
                    gradient,
                    observable: Some(oi),
                    frequency_hz: Some(p.frequencies[fi].hz),
                    fd_gradient: None,
                    fd_rel_error: None,
                });
            }
        }
    }

    // FD self-check through the shipped forward.
    let fd_check = match sens.fd_check {
        None => None,
        Some((step, tol)) => {
            let mut max_rel = 0.0_f64;
            let mut worst: Option<String> = None;
            let mut n_solves = 0;
            // The FD forward only needs the differentiated frequencies (a
            // hybrid spec keeps the whole sweep: tracking).
            let base_rows: Vec<&FrequencyResult> =
                sens.frequency_indices.iter().map(|&fi| &rows[fi]).collect();
            for (k, prm) in sens.parameters.iter().enumerate() {
                let col = d.columns[k].as_ref();
                let scale = parameter_scale(p, prm);
                let h = step * scale;
                // A loss parameter within one step of its lossless bound:
                // `p − h` would be gain (and flip a filled port's branch).
                let one_sided = prm.value < h
                    && matches!(
                        prm.kind,
                        SensitivityParameterKind::EpsRImag | SensitivityParameterKind::TanDelta
                    );
                let run = |delta: f64| -> Result<Vec<FrequencyResult>, CliError> {
                    let mut q = perturbed(p, prm, col, delta)?;
                    if !hybrid {
                        q.frequencies = sens
                            .frequency_indices
                            .iter()
                            .map(|&fi| p.frequencies[fi])
                            .collect();
                    }
                    let out = forward_rows(&q, jobs)?;
                    Ok(if hybrid {
                        sens.frequency_indices
                            .iter()
                            .map(|&fi| out[fi].clone())
                            .collect()
                    } else {
                        out
                    })
                };
                let (a, b) = if one_sided {
                    (run(h)?, run(2.0 * h)?)
                } else {
                    (run(h)?, run(-h)?)
                };
                n_solves += 2;
                for (oi, o) in sens.observables.iter().enumerate() {
                    let gmax = grads[k][oi].iter().map(|g| g.abs()).fold(0.0, f64::max);
                    for si in 0..n_f {
                        let va = row_value(p, o, &a[si])?;
                        let vb = row_value(p, o, &b[si])?;
                        let wrap = |x: f64| {
                            if o.form == ObservableForm::PhaseDeg {
                                (x + 180.0).rem_euclid(360.0) - 180.0
                            } else {
                                x
                            }
                        };
                        let fd = if one_sided {
                            let v0 = row_value(p, o, base_rows[si])?;
                            (4.0 * wrap(va - v0) - wrap(vb - v0)) / (2.0 * h)
                        } else {
                            wrap(va - vb) / (2.0 * h)
                        };
                        let e = &mut entries[(k * n_obs + oi) * n_f + si];
                        let floor = (FD_SCALE_FLOOR * gmax)
                            .max(FD_ROUNDOFF * magnitude_scale(o.form, e.value) / h);
                        let rel = rel_error(e.gradient, fd, floor);
                        let key = if rel.is_nan() { f64::INFINITY } else { rel };
                        if worst.is_none() || key > max_rel {
                            max_rel = key;
                            worst = Some(format!(
                                "parameter {} ({}), {} at {} Hz: gradient {:.6e} vs FD \
                                 {fd:.6e} (rel {rel:.3e})",
                                display(prm),
                                prm.kind.name(),
                                o.label,
                                p.frequencies[e.index[0]].hz,
                                e.gradient
                            ));
                        }
                        e.fd_gradient = Some(fd);
                        e.fd_rel_error = Some(rel);
                    }
                }
            }
            if max_rel > tol {
                return Err(CliError::FdCheckFailed {
                    detail: worst.unwrap_or_default(),
                    tolerance: tol,
                });
            }
            Some(FdCheckSummary {
                relative_step: step,
                tolerance: tol,
                max_rel_error: max_rel,
                n_forward_solves: n_solves,
            })
        }
    };
    Ok(SensitivityReport {
        observable: "driven_observables",
        observable_unit: "per_observable",
        method: "adjoint_s_matrix",
        parameters: sens
            .parameters
            .iter()
            .zip(&d.columns)
            .map(|(prm, col)| parameter_summary(p, prm, col.as_ref()))
            .collect(),
        entries,
        fd_check,
        wall_time_s: t0.elapsed().as_secs_f64(),
        observables: sens
            .observables
            .iter()
            .map(|o| observable_summary(p, o))
            .collect(),
        forward_parity: Some(parity),
        warnings,
    })
}

/// A parameter's name in messages: the shape name, else its volume group.
fn display(prm: &SensitivityParameter) -> String {
    match &prm.shape {
        Some(sh) => format!("`{}`", sh.name),
        None => format!("`{}`", prm.physical_group),
    }
}

/// The complex value of port-mode observable `o` at a sensitivity point.
fn port_mode_complex(
    p: &Problem,
    o: &ObservableDef,
    pt: &geode_core::driven::s_sensitivity::SSensitivityPoint,
) -> Option<c64> {
    let k = o.wave_port?;
    let e = pt
        .port_modes
        .iter()
        .find(|e| e.port == k && e.channel == o.mode)?;
    let obs = e.result.as_ref().ok()?;
    match o.quantity {
        ObservableQuantity::EpsEff => Some(obs.eps_eff),
        _ => obs
            .line
            .as_ref()
            .and_then(|l| z_of_definition(definition(p, k), l)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every form's derivative against a complex-step-free central FD of the
    /// form along `z(t) = z₀ + t·dz`.
    #[test]
    fn form_derivatives_match_finite_differences() {
        let z0 = c64::new(0.3, -0.4);
        let dz = c64::new(-0.7, 0.2);
        for form in [
            ObservableForm::Real,
            ObservableForm::Imag,
            ObservableForm::MagSq,
            ObservableForm::Mag,
            ObservableForm::Db,
            ObservableForm::PhaseDeg,
        ] {
            let h = 1e-6;
            let f = |t: f64| form_value(form, z0 + dz * t);
            let fd = (f(h) - f(-h)) / (2.0 * h);
            let (_, g) = form_of(form, z0, dz);
            assert!(
                (g - fd).abs() <= 1e-7 * g.abs().max(1.0),
                "{form:?}: {g} vs {fd}"
            );
        }
    }

    #[test]
    fn library_errors_map_to_typed_cli_errors() {
        let u = SSensitivityError::Unsupported {
            feature: "an element order 2 space".into(),
            phase: "Epic #836 Phase 4",
            hint: "use p=1".into(),
        };
        let e = lib_error(u);
        assert_eq!(e.code(), "invalid_spec");
        assert!(e.to_string().contains("#836 Phase 4"), "{e}");
        let d = lib_error(SSensitivityError::DegenerateCluster {
            port: 0,
            omega: 1.0,
            channel: 1,
            members: vec![0, 1],
            hint: "keep θ off this port face".into(),
        });
        assert_eq!(d.code(), "invalid_spec");
        assert!(d.to_string().contains("degenerate"), "{d}");
        assert_eq!(
            lib_error(SSensitivityError::Internal("x".into())).code(),
            "solve_failed"
        );
    }
}
