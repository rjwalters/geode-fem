//! Shared set-up for the **design gradients** `dJ/dθ` of the ICEM sector
//! torque (motor paper M3c, issue #1177): the 9 geometric parameters of
//! [`IcemShapeMap`] (#1173) through the shape adjoint of
//! [`SectorShapeProblem`] (#1174), plus `LENGTH`, `OPERATING_ANGLE` and
//! `ScaleR`, and the finite-difference drivers that re-move the mesh.
//!
//! Used by `tests/motor_design_gradient.rs` (CI-fast checks) and
//! `examples/icem_design_gradient.rs` (the study in
//! `benchmarks/motor_shape_gradient/design/`). The including crate root
//! must declare `mod icem;` (`icem_forward.rs`) and `mod icem_shape;`
//! (`icem_shape.rs`) next to this module.
//!
//! # Parameters
//!
//! Index `0..9` are [`PARAM_NAMES`] (metres), then:
//!
//! * `LENGTH` (m): the torque is the torque per unit length times `LENGTH`,
//!   so `dJ/dL = J / L` (analytic).
//! * `OPERATING_ANGLE` (degrees): the load angle `φ0` of
//!   `I_q = I_peak sin(φ0 + 3β + offset_q)`. `dJ/dφ0` is the adjoint of the
//!   current right-hand side, `Σ_g (dT/dI_g) dI_g/dφ0`
//!   ([`TorqueShapeGradient::group_current_grad`](geode_core::assembly::magnetostatic_shape::TorqueShapeGradient::group_current_grad)),
//!   with the analytic `dI_g/dφ0 = I_peak cos(φ0 + 3β + offset_q) N sign`.
//! * `ScaleR`: the reference evaluates the unscaled geometry and multiplies
//!   the torque by `ScaleR²` (`../motor_reference/parametric/README.md`).
//!   With the current prescribed in the unscaled geometry, `J = s² J_u` and
//!   `dJ/ds = 2 J / s` (analytic, no solve depends on `s`).
//!
//! # Current rules
//!
//! * [`CurrentRule::Prescribed`]: the peak current is fixed (`J_g = I_g /
//!   A_g(X)`, the #1174 rule): a larger slot carries the same current at a
//!   lower density.
//! * [`CurrentRule::Jinit`]: the reference optimizer's rule
//!   (`conventions.json` `excitation.setup_vs_optimization`): the current
//!   density of winding group 1 is fixed, so `I_peak ∝ A_1(X) · s`. Here
//!   `I_peak(X) = I_base A_1(X) / A_1(X_base)` with the mesh area, so the
//!   base solve is shared with the prescribed rule and
//!   `dJ/dθ_k|Jinit = dJ/dθ_k|I + (dJ/dI_peak) I_peak (dA_1/dθ_k) / A_1`
//!   (and `+ (dJ/dI_peak) I_peak / s` for `ScaleR`).
#![allow(dead_code)] // each binary uses a subset

use std::f64::consts::PI;
use std::time::Instant;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::{CouplingMode, SectorError};
use geode_core::assembly::magnetostatic_shape::{
    AngleSetGradient, OperatingPoint, SectorShapeProblem, ShapeSolve, ShapeTorque,
    mean_std_parameter_gradient,
};
use geode_core::mesh::icem_parametric::{IcemDesign, IcemShapeMap, N_PARAMS, PARAM_NAMES};
use serde_json::Value;

use crate::icem::{self, AreaMode, IcemModel, IronLaw, POLE_PAIRS, TURNS, WINDING_GROUPS};
use crate::icem_shape::{self, OPT_DEGREES, shape_problem};

/// Number of design parameters: 9 geometric, `LENGTH`, `OPERATING_ANGLE`,
/// `ScaleR`.
pub const N_DESIGN: usize = N_PARAMS + 3;
/// Index of `LENGTH`.
pub const K_LENGTH: usize = N_PARAMS;
/// Index of `OPERATING_ANGLE`.
pub const K_ANGLE: usize = N_PARAMS + 1;
/// Index of `ScaleR`.
pub const K_SCALE: usize = N_PARAMS + 2;

/// Parameter names, [`PARAM_NAMES`] then the three non-geometric ones.
pub fn design_names() -> [&'static str; N_DESIGN] {
    let mut out = [""; N_DESIGN];
    out[..N_PARAMS].copy_from_slice(&PARAM_NAMES);
    out[K_LENGTH] = "LENGTH";
    out[K_ANGLE] = "OPERATING_ANGLE";
    out[K_SCALE] = "ScaleR";
    out
}

/// Unit of parameter `k` in the tables.
pub fn design_unit(k: usize) -> &'static str {
    match k {
        K_ANGLE => "deg",
        K_SCALE => "1",
        _ => "m",
    }
}

/// A full design point (reference units: metres, degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DesignPoint {
    /// The 9 geometric parameters.
    pub geo: IcemDesign,
    /// Axial length (m).
    pub length: f64,
    /// Load angle `OPERATING_ANGLE` (degrees).
    pub phi0_deg: f64,
    /// `ScaleR`.
    pub scale_r: f64,
}

impl DesignPoint {
    /// Design `which` (`initial` or `optimized`) of `reference_data.json`.
    pub fn from_reference(reference: &Value, which: &str) -> Self {
        let p = &reference["designs"][which]["parameters_by_source_registration_order"];
        let get = |n: &str| p[n].as_f64().expect("parameter");
        let mut values = [0.0; N_PARAMS];
        for (k, n) in PARAM_NAMES.iter().enumerate() {
            values[k] = get(n);
        }
        Self {
            geo: IcemDesign { values },
            length: get("LENGTH"),
            phi0_deg: get("OPERATING_ANGLE"),
            scale_r: get("ScaleR"),
        }
    }

    /// `self + t (other − self)`, parameter by parameter.
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let mut out = *self;
        for k in 0..N_DESIGN {
            out.set(k, self.get(k) + t * (other.get(k) - self.get(k)));
        }
        out
    }

    /// Parameter `k`.
    pub fn get(&self, k: usize) -> f64 {
        match k {
            K_LENGTH => self.length,
            K_ANGLE => self.phi0_deg,
            K_SCALE => self.scale_r,
            _ => self.geo.values[k],
        }
    }

    /// Set parameter `k`.
    pub fn set(&mut self, k: usize, v: f64) {
        match k {
            K_LENGTH => self.length = v,
            K_ANGLE => self.phi0_deg = v,
            K_SCALE => self.scale_r = v,
            _ => self.geo.values[k] = v,
        }
    }

    /// The design with parameter `k` moved by `h`.
    pub fn moved(&self, k: usize, h: f64) -> Self {
        let mut d = *self;
        d.set(k, self.get(k) + h);
        d
    }
}

/// How the peak current depends on the design (module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrentRule {
    /// Fixed peak current.
    Prescribed,
    /// Fixed current density in winding group 1 (the reference optimizer).
    Jinit,
}

impl CurrentRule {
    pub fn name(self) -> &'static str {
        match self {
            CurrentRule::Prescribed => "prescribed",
            CurrentRule::Jinit => "jinit",
        }
    }
}

/// Harmonic orders of the validation coupling, n = 3:6:93 (experiment 2,
/// #1185: twice the reference's n_max = 45).
pub fn validation_harmonics() -> Vec<usize> {
    (3..=93).step_by(6).collect()
}

/// Coupling and torque estimator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Harmonic coupling n = 3:6:45 (the reference model), coupling torque.
    /// The training model of experiment 2 (#1185).
    H45,
    /// Harmonic coupling n = 3:6:93, coupling torque. The validation model
    /// of experiment 2 (#1185, amendment 1).
    H93,
    /// Conforming band, Arkkio torque over both gap bands. Not a valid
    /// estimator once free-form offsets move a band boundary (#1183).
    Conforming,
    /// Conforming band, Arkkio torque over the outer gap band only
    /// (`icem_shape::outer_arkkio_spec`: tag 5, normalised by 44.5-45.0 mm,
    /// `IcemModel::arkkio(_, 1)`), refused by the band-area guard of
    /// `SectorShapeProblem::solve` (#1183) if the band left its annulus. The
    /// conforming spot check of experiment 2: fresh meshes only, designs
    /// with zero stator offsets only (#1185 amendment 1).
    ConformingOuter,
}

impl Mode {
    pub fn key(self) -> &'static str {
        match self {
            Mode::H45 => "h45",
            Mode::H93 => "h93",
            Mode::Conforming => "conforming",
            Mode::ConformingOuter => "conforming-outer",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Mode::H45 => "h45/coupling",
            Mode::H93 => "h93/coupling",
            Mode::Conforming => "conforming/Arkkio",
            Mode::ConformingOuter => "conforming/outer-band Arkkio",
        }
    }

    pub fn coupling(self) -> CouplingMode {
        match self {
            Mode::H45 => CouplingMode::Harmonic(icem::REFERENCE_HARMONICS.to_vec()),
            Mode::H93 => CouplingMode::Harmonic(validation_harmonics()),
            Mode::Conforming | Mode::ConformingOuter => CouplingMode::Conforming,
        }
    }

    /// Whether the torque is an Arkkio functional (needs a conforming or
    /// harmonic field, valid only while its band is the annulus).
    pub fn is_arkkio(self) -> bool {
        matches!(self, Mode::Conforming | Mode::ConformingOuter)
    }

    /// The torque functional at axial length `l_axial`.
    pub fn torque(self, l_axial: f64) -> ShapeTorque {
        match (self, icem_shape::arkkio_spec()) {
            (Mode::H45 | Mode::H93, _) => ShapeTorque::Coupling { l_axial },
            (Mode::Conforming, ShapeTorque::Arkkio { terms, .. }) => {
                ShapeTorque::Arkkio { terms, l_axial }
            }
            (Mode::ConformingOuter, _) => match icem_shape::outer_arkkio_spec() {
                ShapeTorque::Arkkio { terms, .. } => ShapeTorque::Arkkio { terms, l_axial },
                ShapeTorque::Coupling { .. } => unreachable!("outer_arkkio_spec is Arkkio"),
            },
            _ => unreachable!("arkkio_spec is an Arkkio functional"),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "h45" => Mode::H45,
            "h93" => Mode::H93,
            "conforming" => Mode::Conforming,
            "conforming-outer" => Mode::ConformingOuter,
            o => panic!("unknown mode {o}"),
        }
    }
}

/// The model on one mesh, its shape problem and the parametrization.
pub struct DesignSetup {
    pub model: IcemModel,
    pub map: IcemShapeMap,
    /// The shape problem at the mesh's own node positions (`θ0`).
    pub problem0: SectorShapeProblem,
    pub mode: Mode,
    /// Rotor angles of the angle set (integer mechanical degrees). Default
    /// [`OPT_DEGREES`], the reference's 10-angle grid; the pre-registered
    /// experiment-2 grid g20 is `0..20` (#1185).
    pub degrees: Vec<i64>,
    /// Wall time of the map's attachment and factorization (s).
    pub map_seconds: f64,
}

impl DesignSetup {
    /// Build from MSH bytes meshed at `theta0`.
    pub fn new(
        level: usize,
        msh: &[u8],
        reference: &Value,
        mode: Mode,
        theta0: IcemDesign,
    ) -> Result<Self, SectorError> {
        let mut model =
            IcemModel::new(level, msh, reference, IronLaw::Table, AreaMode::Mesh, None)?;
        model.coupling = mode.coupling();
        let t0 = Instant::now();
        let map = IcemShapeMap::new(&model.tagged, theta0)
            .map_err(|e| SectorError::InvalidInput(format!("shape map: {e}")))?;
        let map_seconds = t0.elapsed().as_secs_f64();
        let problem0 = shape_problem(&model)?;
        Ok(Self {
            model,
            map,
            problem0,
            mode,
            degrees: OPT_DEGREES.to_vec(),
            map_seconds,
        })
    }

    /// The shape problem with the nodes at `X(geo)`.
    ///
    /// # Errors
    ///
    /// No geometry at `geo`, or an inverted triangle.
    pub fn problem_at(&self, geo: &IcemDesign) -> Result<SectorShapeProblem, SectorError> {
        let x = self
            .map
            .positions(geo)
            .map_err(|e| SectorError::InvalidInput(format!("positions: {e}")))?;
        self.problem0
            .with_node_positions(self.model.band.expand_nodal(&x))
    }

    /// Split-mesh velocities `∂X/∂θ_k` at `geo`, `k` in [`PARAM_NAMES`]
    /// order.
    pub fn velocities(&self, geo: &IcemDesign) -> Result<Vec<Vec<[f64; 2]>>, SectorError> {
        let jac = self
            .map
            .jacobian(geo)
            .map_err(|e| SectorError::InvalidInput(format!("jacobian: {e}")))?;
        Ok(jac
            .iter()
            .map(|v| self.model.band.expand_nodal(v))
            .collect())
    }

    /// The operating points of the optimization grid at `dp`, peak current
    /// `i_peak`.
    pub fn operating_points(&self, dp: &DesignPoint, i_peak: f64) -> Vec<OperatingPoint> {
        icem_shape::operating_points(&self.model, i_peak, dp.phi0_deg * PI / 180.0, &self.degrees)
    }
}

/// Mesh area of winding group 1 (m²) of a shape problem.
pub fn group1_area(p: &SectorShapeProblem) -> f64 {
    p.group_areas()[0]
}

/// The peak current at problem `p` under `rule`, for base current `i_base`
/// at group-1 area `a1_base` and `ScaleR` `s` relative to `s_base`.
pub fn rule_current(
    rule: CurrentRule,
    p: &SectorShapeProblem,
    i_base: f64,
    a1_base: f64,
    s: f64,
    s_base: f64,
) -> f64 {
    match rule {
        CurrentRule::Prescribed => i_base,
        CurrentRule::Jinit => i_base * group1_area(p) / a1_base * s / s_base,
    }
}

/// Objective values at a design: the per-angle torques (N m, `ScaleR²` and
/// `LENGTH` applied), the solves, the problem and the peak current used.
pub struct Evaluation {
    pub torques: Vec<f64>,
    pub solves: Vec<ShapeSolve>,
    pub problem: SectorShapeProblem,
    pub i_peak: f64,
    pub seconds: f64,
}

impl Evaluation {
    pub fn mean(&self) -> f64 {
        icem_shape::Objective::Mean.value(&self.torques)
    }

    pub fn std(&self) -> f64 {
        icem_shape::Objective::Std.value(&self.torques)
    }

    pub fn initials(&self) -> Vec<Vec<f64>> {
        self.solves
            .iter()
            .map(|s| s.solution.solution.a_z.clone())
            .collect()
    }
}

/// Base current state of a gradient study: the base peak current and
/// group-1 area for [`CurrentRule::Jinit`].
#[derive(Debug, Clone, Copy)]
pub struct CurrentBase {
    pub i_base: f64,
    pub a1_base: f64,
    pub s_base: f64,
}

/// Solve the angle set at `dp` under `rule` (warm start from `initials`).
pub fn evaluate(
    setup: &DesignSetup,
    dp: &DesignPoint,
    rule: CurrentRule,
    cb: &CurrentBase,
    cfg: &NewtonConfig,
    initials: Option<&[Vec<f64>]>,
) -> Result<Evaluation, SectorError> {
    let t0 = Instant::now();
    let problem = setup.problem_at(&dp.geo)?;
    let i_peak = rule_current(rule, &problem, cb.i_base, cb.a1_base, dp.scale_r, cb.s_base);
    let ops = setup.operating_points(dp, i_peak);
    let torque = setup.mode.torque(dp.length);
    let solves = problem.solve_angle_set(&ops, &torque, cfg, initials)?;
    let s2 = dp.scale_r * dp.scale_r;
    let torques = solves.iter().map(|s| s2 * s.torque).collect();
    Ok(Evaluation {
        torques,
        solves,
        problem,
        i_peak,
        seconds: t0.elapsed().as_secs_f64(),
    })
}

/// The design gradient of both objectives at one design and current.
#[derive(Debug, Clone)]
pub struct DesignGradient {
    pub mean: f64,
    pub std: f64,
    pub torques: Vec<f64>,
    /// `d mean / dθ_k`, prescribed current.
    pub d_mean: [f64; N_DESIGN],
    /// `d std / dθ_k`, prescribed current.
    pub d_std: [f64; N_DESIGN],
    /// The same under [`CurrentRule::Jinit`].
    pub d_mean_jinit: [f64; N_DESIGN],
    pub d_std_jinit: [f64; N_DESIGN],
    /// `d mean / dI_peak`, `d std / dI_peak` (N m / A).
    pub d_ipeak: [f64; 2],
    /// `dA_1/dθ_k / A_1` (1/m) for the geometric parameters.
    pub d_log_a1: [f64; N_PARAMS],
    /// Wall times (s): forward sweep, adjoints, velocities (`∂X/∂θ`, split),
    /// contractions and current terms.
    pub forward_seconds: f64,
    pub adjoint_seconds: f64,
    pub velocity_seconds: f64,
    pub contract_seconds: f64,
    pub newton_iterations: usize,
}

/// `dI_g/dφ0` (A·turns per rad) at an operating angle `beta` (rad).
pub fn d_current_d_phi0(i_peak: f64, phi0: f64, beta: f64) -> Vec<f64> {
    WINDING_GROUPS
        .iter()
        .map(|&(ph, sign)| i_peak * (phi0 + POLE_PAIRS * beta + ph.offset()).cos() * TURNS * sign)
        .collect()
}

/// Mean/std derivatives from per-point scalar derivatives, scaled by `s2`.
pub fn objective_pair(t: &[f64], dt: &[f64]) -> [f64; 2] {
    let (m, s) = mean_std_parameter_gradient(t, dt);
    [m, s]
}

/// Both objectives' gradients at `dp` (prescribed current `i_peak`), and
/// the base evaluation.
pub fn design_gradient(
    setup: &DesignSetup,
    dp: &DesignPoint,
    i_peak: f64,
    cfg: &NewtonConfig,
) -> Result<(DesignGradient, Evaluation), SectorError> {
    design_gradient_from(setup, dp, i_peak, cfg, None)
}

/// [`design_gradient`] with the forward solves warm-started from
/// `initials` (one `A_z` per operating point; `None` is the cold start of
/// [`design_gradient`]).
pub fn design_gradient_from(
    setup: &DesignSetup,
    dp: &DesignPoint,
    i_peak: f64,
    cfg: &NewtonConfig,
    initials: Option<&[Vec<f64>]>,
) -> Result<(DesignGradient, Evaluation), SectorError> {
    let cb = CurrentBase {
        i_base: i_peak,
        a1_base: 1.0,
        s_base: dp.scale_r,
    };
    let ev = evaluate(setup, dp, CurrentRule::Prescribed, &cb, cfg, initials)?;
    let p = &ev.problem;
    let torque = setup.mode.torque(dp.length);
    let t0 = Instant::now();
    let g: AngleSetGradient = p.angle_set_gradient(&ev.solves, &torque)?;
    let adjoint_seconds = t0.elapsed().as_secs_f64();
    let s2 = dp.scale_r * dp.scale_r;
    let t1 = Instant::now();
    let vel = setup.velocities(&dp.geo)?;
    let velocity_seconds = t1.elapsed().as_secs_f64();

    let t2 = Instant::now();
    let mut d_mean = [0.0; N_DESIGN];
    let mut d_std = [0.0; N_DESIGN];
    let dm = p.design_gradient(&g.mean_grad, &vel)?;
    let ds = p.design_gradient(&g.std_grad, &vel)?;
    for k in 0..N_PARAMS {
        d_mean[k] = s2 * dm[k];
        d_std[k] = s2 * ds[k];
    }
    let mean = s2 * g.mean;
    let std = s2 * g.std;
    let torques: Vec<f64> = g.torques.iter().map(|t| s2 * t).collect();
    // LENGTH: T is linear in L.
    d_mean[K_LENGTH] = mean / dp.length;
    d_std[K_LENGTH] = std / dp.length;
    // OPERATING_ANGLE (degrees): adjoint of the current right-hand side.
    let phi0 = dp.phi0_deg * PI / 180.0;
    let ops = setup.operating_points(dp, i_peak);
    let dt_dphi: Vec<f64> = ops
        .iter()
        .zip(&g.group_current_grads)
        .zip(&setup.degrees)
        .map(|((_, gcg), &deg)| {
            let di = d_current_d_phi0(i_peak, phi0, deg as f64 * PI / 180.0);
            s2 * gcg.iter().zip(&di).map(|(a, b)| a * b).sum::<f64>()
        })
        .collect();
    let [am, as_] = objective_pair(&torques, &dt_dphi);
    d_mean[K_ANGLE] = am * PI / 180.0;
    d_std[K_ANGLE] = as_ * PI / 180.0;
    // ScaleR: J = s² J_u.
    d_mean[K_SCALE] = 2.0 * mean / dp.scale_r;
    d_std[K_SCALE] = 2.0 * std / dp.scale_r;

    // Jinit rule: + (dJ/dI_peak) I_peak d(log A_1)/dθ.
    let d_ipeak = if i_peak != 0.0 {
        let dt_di: Vec<f64> = ops
            .iter()
            .zip(&g.group_current_grads)
            .map(|(op, gcg)| {
                s2 * gcg
                    .iter()
                    .zip(&op.group_currents)
                    .map(|(a, ig)| a * ig / i_peak)
                    .sum::<f64>()
            })
            .collect();
        objective_pair(&torques, &dt_di)
    } else {
        [0.0, 0.0]
    };
    let a1 = group1_area(p);
    let da1 = &p.group_area_gradients()[0];
    let mut d_log_a1 = [0.0; N_PARAMS];
    for k in 0..N_PARAMS {
        d_log_a1[k] = geode_core::assembly::magnetostatic_shape::directional(da1, &vel[k]) / a1;
    }
    let mut d_mean_jinit = d_mean;
    let mut d_std_jinit = d_std;
    for k in 0..N_PARAMS {
        d_mean_jinit[k] += d_ipeak[0] * i_peak * d_log_a1[k];
        d_std_jinit[k] += d_ipeak[1] * i_peak * d_log_a1[k];
    }
    d_mean_jinit[K_SCALE] += d_ipeak[0] * i_peak / dp.scale_r;
    d_std_jinit[K_SCALE] += d_ipeak[1] * i_peak / dp.scale_r;
    let contract_seconds = t2.elapsed().as_secs_f64();

    let newton_iterations = ev
        .solves
        .iter()
        .map(|s| s.solution.solution.iterations)
        .sum();
    let forward_seconds = ev.solves.iter().map(|s| s.forward_seconds).sum();
    Ok((
        DesignGradient {
            mean,
            std,
            torques,
            d_mean,
            d_std,
            d_mean_jinit,
            d_std_jinit,
            d_ipeak,
            d_log_a1,
            forward_seconds,
            adjoint_seconds,
            velocity_seconds,
            contract_seconds,
            newton_iterations,
        },
        ev,
    ))
}

/// Objective values `[mean, std]` at `dp` moved by `h` in parameter `k`
/// (mesh re-moved through `X(θ ± h)` for a geometric parameter), warm
/// started from `base`.
#[allow(clippy::too_many_arguments)]
pub fn objectives_at(
    setup: &DesignSetup,
    dp: &DesignPoint,
    k: usize,
    h: f64,
    rule: CurrentRule,
    cb: &CurrentBase,
    cfg: &NewtonConfig,
    base: &Evaluation,
) -> Result<[f64; 2], SectorError> {
    let d = dp.moved(k, h);
    let init = base.initials();
    let ev = evaluate(setup, &d, rule, cb, cfg, Some(&init))?;
    Ok([ev.mean(), ev.std()])
}

/// One step of a parameter sweep: `J(θ ± h e_k)` for both objectives.
#[derive(Debug, Clone, Copy)]
pub struct StepRow {
    pub h: f64,
    pub plus: [f64; 2],
    pub minus: [f64; 2],
}

impl StepRow {
    /// Central difference of objective `o` (0 mean, 1 std).
    pub fn central(&self, o: usize) -> f64 {
        (self.plus[o] - self.minus[o]) / (2.0 * self.h)
    }

    /// Taylor remainder `|J(θ + h) − J(θ) − h dJ|` of objective `o`.
    pub fn remainder(&self, o: usize, j0: f64, dj: f64) -> f64 {
        (self.plus[o] - j0 - self.h * dj).abs()
    }
}

/// The valid steps of a sweep and the rejected ones with the reason.
pub type StepSweep = (Vec<StepRow>, Vec<(f64, String)>);

/// `J(θ ± h e_k)` for each `h` in `hs`; steps where either side has no
/// valid mesh are returned separately (with the error).
#[allow(clippy::too_many_arguments)]
pub fn step_sweep(
    setup: &DesignSetup,
    dp: &DesignPoint,
    k: usize,
    hs: &[f64],
    rule: CurrentRule,
    cb: &CurrentBase,
    cfg: &NewtonConfig,
    base: &Evaluation,
) -> Result<StepSweep, SectorError> {
    let mut rows = Vec::new();
    let mut rejected = Vec::new();
    for &h in hs {
        let valid = |s: f64| -> Result<(), String> {
            if k < N_PARAMS {
                setup
                    .problem_at(&dp.moved(k, s).geo)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Ok(())
            }
        };
        if let Err(e) = valid(h).and_then(|_| valid(-h)) {
            rejected.push((h, e));
            continue;
        }
        let plus = objectives_at(setup, dp, k, h, rule, cb, cfg, base)?;
        let minus = objectives_at(setup, dp, k, -h, rule, cb, cfg, base)?;
        rows.push(StepRow { h, plus, minus });
    }
    Ok((rows, rejected))
}

/// The Taylor window: the widest run of consecutive steps (largest to
/// smallest `h`) over which `remainder / h²` stays within a factor 2 of its
/// value at the step nearest `h_ref` (the #1174 definition). Returns
/// `(h_hi, h_lo, decades)`, or `None` when the reference step is missing.
pub fn taylor_window(hs: &[f64], rem: &[f64], h_ref: f64) -> Option<(f64, f64, f64)> {
    if hs.is_empty() {
        return None;
    }
    let iref = (0..hs.len()).min_by(|&a, &b| {
        (hs[a].ln() - h_ref.ln())
            .abs()
            .total_cmp(&(hs[b].ln() - h_ref.ln()).abs())
    })?;
    let c = |i: usize| rem[i] / (hs[i] * hs[i]);
    let cref = c(iref);
    if !(cref.is_finite() && cref > 0.0) {
        return None;
    }
    let ok = |i: usize| {
        let r = c(i) / cref;
        r.is_finite() && (0.5..=2.0).contains(&r)
    };
    let mut lo = iref;
    while lo > 0 && ok(lo - 1) {
        lo -= 1;
    }
    let mut hi = iref;
    while hi + 1 < hs.len() && ok(hi + 1) {
        hi += 1;
    }
    let (a, b) = (hs[lo], hs[hi]);
    let (h_hi, h_lo) = if a >= b { (a, b) } else { (b, a) };
    Some((h_hi, h_lo, (h_hi / h_lo).log10()))
}
