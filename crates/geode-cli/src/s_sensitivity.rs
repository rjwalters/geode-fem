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
//! shape), and the disagreement is floored at 1 % of the observable's
//! natural gradient scale and at the FD estimate's own round-off over the
//! tolerance (`fd_floor`, issue #890; see the README).

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
use crate::port_solves::PortSolves;
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
/// Round-off `η_S` of one forward solve's S entry, absolute (S is
/// normalised, `|S_ij| ≤ 1`). The FD estimate of an entry is only good to
/// its forward's round-off over `h` ([`fd_roundoff`], `h` the step), so a
/// disagreement below that is no evidence of an adjoint error (see
/// [`fd_floor`]).
///
/// Measured on the hybrid-face microstrip cookbook (issue #890: a rigid
/// translation of the whole model, whose every FD difference `f(p+h) −
/// f(p−h)` is pure round-off, at relative steps `1e-3`, `1e-4`, `1e-5`):
/// `≤ 1.6e-13` on `S11` (−68 dB) and `≤ 1e-13` on `S21`; ~10× margin.
const FD_ROUNDOFF_S: f64 = 2e-12;
/// Relative round-off `η_mode` of a forward's port-mode value (`z0`,
/// `ε_eff`: the 2-D face eigen solve), measured as [`FD_ROUNDOFF_S`]:
/// `≤ 1.2e-12` on `ε_eff`, `≤ 5.8e-13` on `z0`; ~10× margin.
const FD_ROUNDOFF_PORT_MODE: f64 = 1e-11;

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
/// Geometric ports are the run's shared face solves (issue #952).
fn wave_specs(p: &Problem, solves: &PortSolves<'_>) -> Result<Vec<WavePortSpec>, CliError> {
    p.wave_ports
        .iter()
        .enumerate()
        .map(|(k, w)| match &w.hybrid {
            Some(h) => {
                let mut port = crate::hybrid::hybrid_port(w, h);
                port.opts.accuracy = None;
                Ok(WavePortSpec::from(port))
            }
            None => {
                let mut port = solves.wave_port(k)?;
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
    solves: &PortSolves<'_>,
) -> Result<SSensitivitySweep, CliError> {
    let mesh = &p.tagged.mesh;
    let lumped = crate::driven::lumped_ports(p);
    let walls = crate::driven::impedance_walls(p);
    let wave = wave_specs(p, solves)?;
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

/// The round-off of observable `o`'s value in one forward, read at the
/// design-point report row: an S entry carries absolute noise
/// `η_S` ([`FD_ROUNDOFF_S`]), a port-mode value `η_mode·|z|`
/// ([`FD_ROUNDOFF_PORT_MODE`]);
/// the form maps a change `δz` to `|δz|` (`real`, `imag`, `mag`), `2|z||δz|`
/// (`mag_sq`), `(20/ln 10)|δz|/|z|` (dB) or `(180/π)|δz|/|z|` (degrees). A
/// deep null's dB or phase is noise-dominated accordingly — and so is its
/// gradient, which carries the same `1/|z|`.
fn fd_roundoff(p: &Problem, o: &ObservableDef, row: &FrequencyResult) -> f64 {
    let (z, noise) = match o.quantity {
        ObservableQuantity::S => {
            let [i, j] = o.entries[0];
            (row_s(row, i, j).norm(), FD_ROUNDOFF_S)
        }
        ObservableQuantity::SSumSq => {
            return FD_ROUNDOFF_S
                * 2.0
                * o.entries
                    .iter()
                    .map(|&[i, j]| row_s(row, i, j).norm())
                    .sum::<f64>()
                    .max(1.0);
        }
        ObservableQuantity::Z0 | ObservableQuantity::EpsEff => {
            let z = row_port_mode(p, o, row).map_or(1.0, |z| z.norm());
            (z, FD_ROUNDOFF_PORT_MODE * z)
        }
    };
    let z = z.max(f64::MIN_POSITIVE);
    noise
        * match o.form {
            ObservableForm::Real | ObservableForm::Imag | ObservableForm::Mag => 1.0,
            ObservableForm::MagSq => 2.0 * z,
            ObservableForm::Db => 20.0 / std::f64::consts::LN_10 / z,
            ObservableForm::PhaseDeg => 180.0 / std::f64::consts::PI / z,
        }
}

/// The FD-check denominator floor of one driven entry (issue #890): the
/// largest of
///
/// * `FD_SCALE_FLOOR · gmax` — 1 % of the largest |gradient| of the same
///   (parameter, observable) over the frequencies;
/// * `FD_SCALE_FLOOR · M / L` — 1 % of the observable's natural gradient
///   scale, its magnitude scale `M` ([`magnitude_scale`]) per the
///   parameter's natural scale `L` ([`parameter_scale`]). Unlike `gmax`
///   it does not vanish with an entry that is zero by symmetry (a lateral
///   strip shift in a symmetric box, a rigid motion);
/// * `ρ / (h · tol)` — the FD estimate's own round-off `ρ / h` (`ρ` the
///   forward's round-off of the value, [`fd_roundoff`]; ×4 for the
///   one-sided difference, whose weights sum to 8 over `2h`) over the
///   tolerance: a disagreement within the FD's round-off passes, any
///   larger one is judged.
///
/// What still fails: an adjoint error `δ = r·|g|` (`r` = 1 %, say) is
/// caught whenever `r·|g| > tol · floor`, i.e. on every entry with `|g|`
/// above `max(0.01·M/L·tol/r, ρ/(h·r))` — for `r = 1 %` and the default
/// `tol = step = 1e-4`, entries above `1e-4·M/L` and above `100·ρ/(1e-4·L)`
/// (for an S entry, a gradient whose underlying `|∂S/∂p|·L` exceeds
/// `2e-6`): far below any live gradient (the cookbook's strip-width
/// `∂ε_eff/∂w` is `147 /m` against `M/L = 1.7e3 /m`).
fn fd_floor(
    gmax: f64,
    magnitude: f64,
    roundoff: f64,
    scale: f64,
    h: f64,
    tol: f64,
    one_sided: bool,
) -> f64 {
    let fd_noise = roundoff / h * if one_sided { 4.0 } else { 1.0 };
    (FD_SCALE_FLOOR * gmax)
        .max(FD_SCALE_FLOOR * magnitude / scale)
        .max(fd_noise / tol)
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

/// The rejection of a `z0` observable on a channel that carries no net
/// conductor current (issue #953): a TE / TM waveguide mode of the port
/// face, such as a coax TE₁₁, has no line impedance, so its report
/// `z_line_ohm` is `null` and there is nothing to differentiate (the
/// library's `Z_PI` there is a ratio of round-off; its own guard is issue
/// #991). Runs each observed hybrid port's face sweep only (no 3-D solve),
/// as `--touchstone` classification does, so the run fails before the
/// expensive solve. `Ok(())` when every `z0` observable has a line
/// impedance at every selected frequency.
pub fn line_impedance_error(
    p: &Problem,
    sens: &SensitivityTarget,
    solves: &PortSolves<'_>,
) -> Result<(), CliError> {
    let mut ports: Vec<usize> = sens
        .observables
        .iter()
        .filter(|o| o.quantity == ObservableQuantity::Z0)
        .filter_map(|o| o.wave_port)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    for k in ports {
        let w = &p.wave_ports[k];
        let Some(h) = w.hybrid.as_ref() else {
            continue;
        };
        // Shared with `--touchstone` classification (issue #952).
        let sweep = solves.hybrid_face_sweep(k)?;
        for o in sens
            .observables
            .iter()
            .filter(|o| o.quantity == ObservableQuantity::Z0 && o.wave_port == Some(k))
        {
            for &fi in &sens.frequency_indices {
                let Some(ch) = sweep
                    .report
                    .points
                    .get(fi)
                    .and_then(|pt| pt.channels.get(o.mode))
                else {
                    continue;
                };
                let flagged = crate::hybrid::channel_result(h, ch)
                    .line
                    .is_some_and(|l| l.no_net_current);
                if flagged {
                    return Err(CliError::InvalidSpec(format!(
                        "`sensitivity` observable `{}`: wave port `{}` mode {} carries no net \
                         conductor current at {} Hz (its conductor currents cancel to \
                         round-off: a TE/TM waveguide mode of the port face, such as a coax \
                         TE11, not a line mode), so it has no line impedance to differentiate \
                         (its report `z_line_ohm` is null); differentiate this channel's \
                         `eps_eff` or an S entry instead, or observe `z0` on a line (TEM / \
                         quasi-TEM) channel",
                        o.label, w.surface.name, o.mode, p.frequencies[fi].hz
                    )));
                }
            }
        }
    }
    Ok(())
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
///   face normal, i.e. the wall itself is reshaped or displaced; or has a
///   node on its free perimeter (an edge used by exactly one of the
///   group's triangles) moving across that edge in the wall's plane, i.e.
///   a finite plate (a forgotten ground plane) is resized. Motion in a
///   wall's own plane whose perimeter is held (a wall sliding with a slab
///   it bounds, its edges on pinned port faces) and hybrid wave-port faces
///   (which move in-plane with the cross-section by design) are not
///   flagged;
/// * `sensitivity_shape_tangential` (issue #893) — the mesh moves, the
///   geometry does not: no face of any surface of the model (the mesh
///   boundary, every named surface group — walls, port faces, the
///   parameter's own groups —, every material interface and UPML shell
///   face, see [`model_faces`]) has a node moving along its normal, and no
///   named surface group has a free-perimeter node moving across its edge
///   ([`moves_perimeter`]). Every surface only slides within itself, so
///   the morph is a reparametrisation: the continuum gradient is zero and
///   the reported one is discretisation sensitivity. Deliberately
///   conservative — one normal-moving node anywhere silences it (so a
///   slide along a curved, faceted surface is not flagged);
/// * `sensitivity_shape_moves_interface` (issue #893) — a material
///   interface (a face between two volume groups of different material)
///   has a node, not one of the parameter's own, moving along the face
///   normal: the harmonic extension drags the interface with the motion.
///   One warning per pair of volume groups, named in the message; raised
///   whatever is pinned, because an interface is rarely a named surface
///   group and then cannot be pinned. Faces on a `pec` / `leontovich` /
///   `silver_muller` wall are left to `sensitivity_shape_moves_boundary`.
///
/// A rigid motion raises neither of the last two (it is its own warning).
fn shape_warnings(
    p: &Problem,
    prm: &SensitivityParameter,
    col: &[[f64; 3]],
    faces: &ModelFaces,
) -> Vec<WarningResult> {
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
        let moves_normal = surf
            .triangles
            .iter()
            .any(|t| normal_motion(&p.tagged.mesh.nodes, t, &own, col) > STILL_TOL * max);
        if moves_normal || moves_perimeter(&p.tagged.mesh.nodes, &surf.triangles, &own, col, max) {
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
    if rigid {
        return out;
    }
    let nodes = &p.tagged.mesh.nodes;
    let none = std::collections::HashSet::new();

    // Purely tangential motion: no surface moves along its normal, no named
    // surface's free perimeter moves across itself.
    let tangential = faces
        .interfaces
        .iter()
        .map(|f| &f.0)
        .chain(&faces.other)
        .chain(&p.tagged.boundary_triangles)
        .all(|t| normal_motion(nodes, t, &none, col) <= STILL_TOL * max)
        && {
            let mut tags: Vec<i32> = p.tagged.triangle_physical_tags.clone();
            tags.sort_unstable();
            tags.dedup();
            tags.into_iter().filter(|&t| t > 0).all(|tag| {
                !moves_perimeter(nodes, &p.tagged.triangles_with_tag(tag), &none, col, max)
            })
        };
    if tangential {
        let w = WarningResult {
            kind: "sensitivity_shape_tangential",
            wave_port: None,
            physical_group: None,
            message: format!(
                "shape parameter `{}` moves the mesh but not the geometry: no wall, port face, \
                 material interface or group of its own moves along its normal, and no named \
                 surface's free edge moves across itself — every surface only slides within \
                 itself, so the morph is a reparametrisation, the continuum gradient of `{}` is \
                 zero and the reported gradients are discretisation sensitivity (they vanish \
                 under mesh refinement), not a design sensitivity",
                sh.name, sh.name
            ),
        };
        eprintln!("warning: {}", w.message);
        out.push(w);
        return out;
    }

    // Material interfaces dragged along their normal by the extension.
    let key = |t: &[u32; 3]| {
        let mut k = *t;
        k.sort_unstable();
        k
    };
    let on_wall: std::collections::HashSet<[u32; 3]> = p
        .pec
        .iter()
        .chain(p.leontovich.iter().map(|l| &l.surface))
        .chain(&p.silver_muller)
        .flat_map(|s| s.triangles.iter().map(key))
        .collect();
    // Per region pair: (faces moving, faces, largest normal motion, the
    // moving faces).
    type Drift = (usize, usize, f64, Vec<[u32; 3]>);
    let mut pairs: std::collections::BTreeMap<(usize, usize), Drift> =
        std::collections::BTreeMap::new();
    for (t, ra, rb) in &faces.interfaces {
        if on_wall.contains(&key(t)) {
            continue;
        }
        let e = pairs.entry((*ra, *rb)).or_default();
        e.1 += 1;
        let m = normal_motion(nodes, t, &own, col);
        if m > STILL_TOL * max {
            e.0 += 1;
            e.2 = e.2.max(m);
            e.3.push(key(t));
        }
    }
    let tag_of: std::collections::HashMap<[u32; 3], i32> = if pairs.values().any(|d| d.0 > 0) {
        p.tagged
            .boundary_triangles
            .iter()
            .map(key)
            .zip(p.tagged.triangle_physical_tags.iter().copied())
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    for ((ra, rb), (moving, total, most, tris)) in pairs {
        if moving == 0 {
            continue;
        }
        let (a, b) = (&p.regions[ra].name, &p.regions[rb].name);
        let mut named: Vec<&str> = tris
            .iter()
            .filter_map(|t| tag_of.get(t))
            .filter_map(|tag| p.tagged.mesh.physical_groups.get(&(2, *tag)))
            .map(String::as_str)
            .collect();
        named.sort_unstable();
        named.dedup();
        let remedy = if named.is_empty() {
            "these faces are not in a named surface group, so they cannot be listed in \
             `pinned`: tag the interface as a surface group in the mesh and pin it if it is \
             fixed"
                .to_string()
        } else {
            format!(
                "add the surface group(s) `{}` to `pinned` if the interface is fixed",
                named.join("`, `")
            )
        };
        let w = WarningResult {
            kind: "sensitivity_shape_moves_interface",
            wave_port: None,
            physical_group: None,
            message: format!(
                "shape parameter `{}` moves the material interface between the volume groups \
                 `{a}` and `{b}` along its normal ({moving} of {total} faces, by up to {:.2} of \
                 the parameter's largest node motion) on nodes that are not its own: the \
                 harmonic extension drags that interface, so the gradient is of a morph that \
                 also reshapes both regions — {remedy}",
                sh.name,
                most / max
            ),
        };
        eprintln!("warning: {}", w.message);
        out.push(w);
    }
    out
}

/// The largest motion along the face normal of triangle `t` over its nodes
/// that are not in `skip` (`0` for a degenerate triangle).
fn normal_motion(
    nodes: &[[f64; 3]],
    t: &[u32; 3],
    skip: &std::collections::HashSet<u32>,
    col: &[[f64; 3]],
) -> f64 {
    let x = |i: usize| nodes[t[i] as usize];
    let (a, b, c) = (x(0), x(1), x(2));
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let nn = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if nn == 0.0 {
        return 0.0;
    }
    t.iter()
        .filter(|k| !skip.contains(k))
        .map(|&k| {
            let m = col[k as usize];
            (m[0] * n[0] + m[1] * n[1] + m[2] * n[2]).abs() / nn
        })
        .fold(0.0, f64::max)
}

/// The untagged surfaces of the model, from the tet adjacency (issue #893).
struct ModelFaces {
    /// Material interfaces: a face shared by two tets of different volume
    /// groups whose materials differ, with the two groups' indices into
    /// [`Problem::regions`] (ascending).
    interfaces: Vec<([u32; 3], usize, usize)>,
    /// The mesh boundary (a face of one tet) and the faces of a UPML shell
    /// (two tets of different [`Problem::upml_of_tet`]).
    other: Vec<[u32; 3]>,
}

/// The [`ModelFaces`] of `p`. Two volume groups have different materials
/// when their per-tet `ε_r`, `μ_r` or diagonal tensors differ, or either is
/// dispersive (two groups given one material are one medium: the face
/// between them is not an interface).
fn model_faces(p: &Problem, region_of_tet: &[usize]) -> ModelFaces {
    const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let dispersive = |r: usize| p.dispersion.iter().any(|d| d.tag == p.regions[r].tag);
    let diag = |a: usize, b: usize| {
        p.eps_diag.get(a).copied().flatten() != p.eps_diag.get(b).copied().flatten()
            || p.mu_diag.get(a).copied().flatten() != p.mu_diag.get(b).copied().flatten()
    };
    let upml = |t: usize| p.upml_of_tet.get(t).copied().flatten();
    let mut first: std::collections::HashMap<[u32; 3], usize> = std::collections::HashMap::new();
    let mut out = ModelFaces {
        interfaces: Vec::new(),
        other: Vec::new(),
    };
    for (b, tet) in p.tagged.mesh.tets.iter().enumerate() {
        for f in FACES {
            let tri = [tet[f[0]], tet[f[1]], tet[f[2]]];
            let mut k = tri;
            k.sort_unstable();
            let Some(a) = first.remove(&k) else {
                first.insert(k, b);
                continue;
            };
            let (ra, rb) = (region_of_tet[a], region_of_tet[b]);
            if ra != rb
                && (p.eps[a] != p.eps[b]
                    || p.mu_r[a] != p.mu_r[b]
                    || diag(a, b)
                    || dispersive(ra)
                    || dispersive(rb))
            {
                out.interfaces.push((tri, ra.min(rb), ra.max(rb)));
            } else if upml(a) != upml(b) {
                out.other.push(tri);
            }
        }
    }
    // What is left has one tet: the mesh boundary (sorted: the map's order
    // is not deterministic).
    let mut boundary: Vec<[u32; 3]> = first.into_keys().collect();
    boundary.sort_unstable();
    out.other.extend(boundary);
    out
}

/// Whether a node (not in `own`) on the free perimeter of the surface
/// `tris` — an edge used by exactly one of its triangles — moves across
/// that edge within the triangle's plane (along `n × e`) by more than
/// `STILL_TOL * max`: the plate's extent changes. Either sign counts (the
/// parameter's sign is arbitrary: a shrink is a resize too); sliding along
/// the edge and normal motion (checked separately) do not.
fn moves_perimeter(
    nodes: &[[f64; 3]],
    tris: &[[u32; 3]],
    own: &std::collections::HashSet<u32>,
    col: &[[f64; 3]],
    max: f64,
) -> bool {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |u: [f64; 3], v: [f64; 3]| {
        [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ]
    };
    let dot = |u: [f64; 3], v: [f64; 3]| u[0] * v[0] + u[1] * v[1] + u[2] * v[2];
    let mut uses: std::collections::HashMap<(u32, u32), (u32, usize)> =
        std::collections::HashMap::new();
    for (ti, t) in tris.iter().enumerate() {
        for i in 0..3 {
            let (a, b) = (t[i], t[(i + 1) % 3]);
            uses.entry((a.min(b), a.max(b))).or_insert((0, ti)).0 += 1;
        }
    }
    uses.iter()
        .filter(|(_, (n, _))| *n == 1)
        .any(|(&(a, b), &(_, ti))| {
            let t = tris[ti];
            let c = t.iter().copied().find(|&k| k != a && k != b).unwrap_or(a);
            let (xa, xb, xc) = (nodes[a as usize], nodes[b as usize], nodes[c as usize]);
            let e = sub(xb, xa);
            let n = cross(e, sub(xc, xa));
            let d = cross(n, e);
            let dn = dot(d, d).sqrt();
            if dn == 0.0 {
                return false;
            }
            [a, b]
                .iter()
                .filter(|k| !own.contains(k))
                .any(|&k| dot(col[k as usize], d).abs() / dn > STILL_TOL * max)
        })
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
        // `q`'s own face solves: a perturbed problem is not the run's.
        crate::driven::wave_sweep(q, opts, &PortSolves::new(q))?.0
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
    solves: &PortSolves<'_>,
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
    let sw = library_sweep(p, &d.sdesign, &omegas, port_modes, solves)?;
    let n_lib = sw.params.len();
    let point_of = |fi: usize| -> &geode_core::driven::s_sensitivity::SSensitivityPoint {
        let at = solved.iter().position(|&s| s == fi).expect("solved");
        &sw.points[at]
    };

    // Forward parity against the report's own rows.
    let mut parity = 0.0_f64;
    let faces = d
        .columns
        .iter()
        .any(Option::is_some)
        .then(|| model_faces(p, &sens.region_of_tet));
    let mut warnings: Vec<WarningResult> = sens
        .parameters
        .iter()
        .zip(&d.columns)
        .filter_map(|(prm, col)| Some(shape_warnings(p, prm, col.as_ref()?, faces.as_ref()?)))
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
            let want = row_port_mode(p, o, row);
            let got = port_mode_complex(p, o, pt);
            // Backstop of `line_impedance_error` (issue #953): a `z0` the
            // report does not carry (no net conductor current) is never a
            // value, whatever the library computes there (#991).
            if o.quantity == ObservableQuantity::Z0 && want.is_none() && got.is_some() {
                return Err(CliError::InvalidSpec(format!(
                    "`sensitivity` observable `{}` at {} Hz: the report carries no line \
                     impedance for this channel (`z_line_ohm` is null: it carries no net \
                     conductor current), so there is no `z0` to differentiate; differentiate \
                     its `eps_eff` or an S entry instead",
                    o.label, row.frequency_hz
                )));
            }
            if let (Some(want), Some(got)) = (want, got) {
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
                        let floor = fd_floor(
                            gmax,
                            magnitude_scale(o.form, e.value),
                            fd_roundoff(p, o, base_rows[si]),
                            scale,
                            h,
                            tol,
                            one_sided,
                        );
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

    /// Issue #890: the FD floor passes the symmetry-zero entries measured on
    /// the hybrid microstrip cookbook (release `geode driven`, default step
    /// and tolerance `1e-4`) and still fails a 1 % adjoint error on every
    /// live entry — including one ~6e-4 of its natural scale.
    #[test]
    fn fd_floor_passes_symmetry_zeros_and_fails_one_percent_errors() {
        let tol = 1e-4;
        let step = 1e-4;
        let db = 20.0 / std::f64::consts::LN_10;
        let s11 = 3.79e-4; // |S11| at the design point (−68.4 dB)
        // Strip width (stretch, L = w) and lateral strip shift (translate,
        // L = the strip's bounding-box size).
        let (l_width, l_shift) = (1.91e-3, 2.02e-3);
        let check = |g: f64, fd: f64, gmax: f64, m: f64, rho: f64, l: f64| {
            let floor = fd_floor(gmax, m, rho, l, step * l, tol, false);
            rel_error(g, fd, floor)
        };
        // The forward's round-off of each value (`fd_roundoff`).
        let (s_db, s_deg) = (FD_ROUNDOFF_S * db / s11, FD_ROUNDOFF_S * 57.296);
        let mode = |z: f64| FD_ROUNDOFF_PORT_MODE * z;
        // (gradient, FD, M, ρ, L): the entries of the two reproductions.
        let zeros = [
            // Case 1, lateral shift with the shield pinned: ∂ε_eff, mesh
            // asymmetry noise (rel 3.9e-4 under the old floor).
            (-3.2946e-2, -3.2959e-2, 3.3282, mode(3.3282), l_shift),
            // Case 2, rigid translation: ∂ dB(S11) (old floor: rel 0.98),
            // ∂z0, ∂ε_eff, ∂∠S21.
            (1.1209e-4, 6.9107e-3, db, s_db, l_shift),
            (1.3584e-12, 1.5188e-5, 49.273, mode(49.273), l_shift),
            (-1.1296e-14, -2.0750e-6, 3.3282, mode(3.3282), l_shift),
            (-2.7965e-7, 1.6342e-6, 57.296, s_deg, l_shift),
        ];
        for (g, fd, m, rho, l) in zeros {
            let rel = check(g, fd, g.abs(), m, rho, l);
            assert!(rel <= tol / 10.0, "symmetry zero {g} vs {fd}: rel {rel:e}");
        }
        // Live entries: (gradient, FD, M, ρ, L). The strip-width ∂z0,
        // ∂ε_eff, ∂ dB(S11), ∂∠S21, and the lateral shift's ∂z0 (−15.08 Ω/m
        // against a natural scale M/L = 2.4e4 Ω/m).
        let live = [
            (
                -1.5297e4,
                -1.5297e4 + 2.552e-4,
                49.273,
                mode(49.273),
                l_width,
            ),
            (147.33, 147.33 - 4.666e-5, 3.3282, mode(3.3282), l_width),
            (2973.4, 2973.4 - 1.071e-2, db, s_db, l_width),
            (-181.68, -181.68 + 1.045e-5, 57.296, s_deg, l_width),
            (-15.082, -15.082 + 8.955e-5, 49.273, mode(49.273), l_shift),
        ];
        for (g, fd, m, rho, l) in live {
            assert!(check(g, fd, g.abs(), m, rho, l) <= tol, "{g} vs {fd}");
            for r in [1.01, 0.99] {
                let bad = g * r;
                let rel = check(bad, fd, bad.abs(), m, rho, l);
                // The sub-natural z0 entry fails at rel 6.2e-4; the rest at ≥ 2e-3.
                assert!(rel > 5.0 * tol, "a 1 % error on {g} passes: rel {rel:e}");
            }
        }
        // The one-sided difference's round-off is 4× the central one's.
        let c = fd_floor(0.0, 0.0, 1.0, 1.0, 1e-4, tol, false);
        let o = fd_floor(0.0, 0.0, 1.0, 1.0, 1e-4, tol, true);
        assert!((o / c - 4.0).abs() < 1e-12, "{o} vs {c}");
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
