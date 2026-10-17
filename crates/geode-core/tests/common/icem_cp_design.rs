//! Design gradients `dJ/dθ` of the ICEM sector torque over the **free-form
//! control-point design space** (issue #1179): the 19 geometric components
//! of [`IcemCpShapeMap`] (9 scalars and 10 control-point offsets) through
//! the shape adjoint of [`SectorShapeProblem`] (#1174), plus `LENGTH`,
//! `OPERATING_ANGLE` and `ScaleR`, and the step sweeps that re-move the
//! mesh.
//!
//! This extends `icem_design.rs` (#1177) to the wider space and reuses its
//! current rules, couplings, evaluations, step rows and Taylor window. The
//! non-geometric components and both current rules follow the definitions
//! documented there. The including crate root must declare `mod icem;`,
//! `mod icem_shape;` and `mod icem_design;` next to this module.
#![allow(dead_code)] // each binary uses a subset

use std::f64::consts::PI;
use std::time::Instant;

use geode_core::assembly::magnetostatic_nonlinear::NewtonConfig;
use geode_core::assembly::magnetostatic_sector::SectorError;
use geode_core::assembly::magnetostatic_shape::{
    AngleSetGradient, OperatingPoint, SectorShapeProblem, directional,
};
use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, IcemCpShapeMap, N_CP_PARAMS, N_OFFSETS, OFFSET_NAMES, cp_param_names,
};
use geode_core::mesh::icem_parametric::{N_PARAMS, PARAM_NAMES};
use serde_json::Value;

use crate::icem::{AreaMode, IcemModel, IronLaw};
use crate::icem_design::{
    CurrentBase, CurrentRule, Evaluation, Mode, StepRow, StepSweep, d_current_d_phi0, group1_area,
    objective_pair, rule_current,
};
use crate::icem_shape::{self, OPT_DEGREES, shape_problem};

/// Components: 19 geometric, `LENGTH`, `OPERATING_ANGLE`, `ScaleR`.
pub const N_CP_DESIGN: usize = N_CP_PARAMS + 3;
/// Index of `LENGTH`.
pub const KC_LENGTH: usize = N_CP_PARAMS;
/// Index of `OPERATING_ANGLE`.
pub const KC_ANGLE: usize = N_CP_PARAMS + 1;
/// Index of `ScaleR`.
pub const KC_SCALE: usize = N_CP_PARAMS + 2;

/// Component names: [`cp_param_names`] then the three non-geometric ones.
pub fn cp_design_names() -> [&'static str; N_CP_DESIGN] {
    let mut out = [""; N_CP_DESIGN];
    out[..N_CP_PARAMS].copy_from_slice(&cp_param_names());
    out[KC_LENGTH] = "LENGTH";
    out[KC_ANGLE] = "OPERATING_ANGLE";
    out[KC_SCALE] = "ScaleR";
    out
}

/// Index of an offset component.
pub fn offset_index(name: &str) -> usize {
    N_PARAMS
        + OFFSET_NAMES
            .iter()
            .position(|n| *n == name)
            .unwrap_or_else(|| panic!("unknown offset {name}"))
}

/// A full design point of the control-point space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpDesignPoint {
    /// The 19 geometric components.
    pub geo: IcemCpDesign,
    /// Axial length (m).
    pub length: f64,
    /// Load angle (degrees).
    pub phi0_deg: f64,
    /// `ScaleR`.
    pub scale_r: f64,
}

impl CpDesignPoint {
    /// Design `which` (`initial` or `optimized`) of `reference_data.json`
    /// with the free-form `offsets`.
    pub fn from_reference(reference: &Value, which: &str, offsets: &[f64; N_OFFSETS]) -> Self {
        let p = &reference["designs"][which]["parameters_by_source_registration_order"];
        let get = |n: &str| p[n].as_f64().expect("parameter");
        let mut scalars = geode_core::mesh::icem_parametric::IcemDesign {
            values: [0.0; N_PARAMS],
        };
        for (k, n) in PARAM_NAMES.iter().enumerate() {
            scalars.values[k] = get(n);
        }
        Self {
            geo: IcemCpDesign::from_parts(&scalars, offsets),
            length: get("LENGTH"),
            phi0_deg: get("OPERATING_ANGLE"),
            scale_r: get("ScaleR"),
        }
    }

    /// Component `k`.
    pub fn get(&self, k: usize) -> f64 {
        match k {
            KC_LENGTH => self.length,
            KC_ANGLE => self.phi0_deg,
            KC_SCALE => self.scale_r,
            _ => self.geo.values[k],
        }
    }

    /// Set component `k`.
    pub fn set(&mut self, k: usize, v: f64) {
        match k {
            KC_LENGTH => self.length = v,
            KC_ANGLE => self.phi0_deg = v,
            KC_SCALE => self.scale_r = v,
            _ => self.geo.values[k] = v,
        }
    }

    /// The design with component `k` moved by `h`.
    pub fn moved(&self, k: usize, h: f64) -> Self {
        let mut d = *self;
        d.set(k, self.get(k) + h);
        d
    }
}

/// The model on one mesh, its shape problem and the control-point map.
pub struct CpDesignSetup {
    pub model: IcemModel,
    pub map: IcemCpShapeMap,
    pub problem0: SectorShapeProblem,
    pub mode: Mode,
    pub map_seconds: f64,
}

impl CpDesignSetup {
    /// Build from MSH bytes meshed at `theta0`.
    pub fn new(
        level: usize,
        msh: &[u8],
        reference: &Value,
        mode: Mode,
        theta0: IcemCpDesign,
    ) -> Result<Self, SectorError> {
        let mut model =
            IcemModel::new(level, msh, reference, IronLaw::Table, AreaMode::Mesh, None)?;
        model.coupling = mode.coupling();
        let t0 = Instant::now();
        let map = IcemCpShapeMap::new(&model.tagged, theta0)
            .map_err(|e| SectorError::InvalidInput(format!("shape map: {e}")))?;
        let map_seconds = t0.elapsed().as_secs_f64();
        let problem0 = shape_problem(&model)?;
        Ok(Self {
            model,
            map,
            problem0,
            mode,
            map_seconds,
        })
    }

    /// The shape problem with the nodes at `X(geo)`.
    ///
    /// # Errors
    ///
    /// No geometry at `geo`, or an inverted triangle.
    pub fn problem_at(&self, geo: &IcemCpDesign) -> Result<SectorShapeProblem, SectorError> {
        let x = self
            .map
            .positions(geo)
            .map_err(|e| SectorError::InvalidInput(format!("positions: {e}")))?;
        self.problem0
            .with_node_positions(self.model.band.expand_nodal(&x))
    }

    /// Split-mesh velocities `∂X/∂θ_k` at `geo` for the 19 components.
    pub fn velocities(&self, geo: &IcemCpDesign) -> Result<Vec<Vec<[f64; 2]>>, SectorError> {
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
    pub fn operating_points(&self, dp: &CpDesignPoint, i_peak: f64) -> Vec<OperatingPoint> {
        icem_shape::operating_points(&self.model, i_peak, dp.phi0_deg * PI / 180.0, &OPT_DEGREES)
    }
}

/// Solve the angle set at `dp` under `rule` (warm start from `initials`).
pub fn evaluate(
    setup: &CpDesignSetup,
    dp: &CpDesignPoint,
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
pub struct CpDesignGradient {
    pub mean: f64,
    pub std: f64,
    pub torques: Vec<f64>,
    /// `d mean / dθ_k`, prescribed current.
    pub d_mean: [f64; N_CP_DESIGN],
    pub d_std: [f64; N_CP_DESIGN],
    /// The same under [`CurrentRule::Jinit`].
    pub d_mean_jinit: [f64; N_CP_DESIGN],
    pub d_std_jinit: [f64; N_CP_DESIGN],
    /// `d mean / dI_peak`, `d std / dI_peak` (N m / A).
    pub d_ipeak: [f64; 2],
    /// `dA_1/dθ_k / A_1` (1/m) for the 19 geometric components.
    pub d_log_a1: [f64; N_CP_PARAMS],
    pub forward_seconds: f64,
    pub adjoint_seconds: f64,
    pub velocity_seconds: f64,
    pub contract_seconds: f64,
    pub newton_iterations: usize,
}

/// Both objectives' gradients at `dp` (base peak current `i_peak`), and the
/// base evaluation. The same assembly as `icem_design::design_gradient`,
/// over 22 components.
pub fn design_gradient(
    setup: &CpDesignSetup,
    dp: &CpDesignPoint,
    i_peak: f64,
    cfg: &NewtonConfig,
) -> Result<(CpDesignGradient, Evaluation), SectorError> {
    let cb = CurrentBase {
        i_base: i_peak,
        a1_base: 1.0,
        s_base: dp.scale_r,
    };
    let ev = evaluate(setup, dp, CurrentRule::Prescribed, &cb, cfg, None)?;
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
    let mut d_mean = [0.0; N_CP_DESIGN];
    let mut d_std = [0.0; N_CP_DESIGN];
    let dm = p.design_gradient(&g.mean_grad, &vel)?;
    let ds = p.design_gradient(&g.std_grad, &vel)?;
    for k in 0..N_CP_PARAMS {
        d_mean[k] = s2 * dm[k];
        d_std[k] = s2 * ds[k];
    }
    let mean = s2 * g.mean;
    let std = s2 * g.std;
    let torques: Vec<f64> = g.torques.iter().map(|t| s2 * t).collect();
    d_mean[KC_LENGTH] = mean / dp.length;
    d_std[KC_LENGTH] = std / dp.length;
    let phi0 = dp.phi0_deg * PI / 180.0;
    let ops = setup.operating_points(dp, i_peak);
    let dt_dphi: Vec<f64> = g
        .group_current_grads
        .iter()
        .zip(&OPT_DEGREES)
        .map(|(gcg, &deg)| {
            let di = d_current_d_phi0(i_peak, phi0, deg as f64 * PI / 180.0);
            s2 * gcg.iter().zip(&di).map(|(a, b)| a * b).sum::<f64>()
        })
        .collect();
    let [am, as_] = objective_pair(&torques, &dt_dphi);
    d_mean[KC_ANGLE] = am * PI / 180.0;
    d_std[KC_ANGLE] = as_ * PI / 180.0;
    d_mean[KC_SCALE] = 2.0 * mean / dp.scale_r;
    d_std[KC_SCALE] = 2.0 * std / dp.scale_r;

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
    let mut d_log_a1 = [0.0; N_CP_PARAMS];
    for k in 0..N_CP_PARAMS {
        d_log_a1[k] = directional(da1, &vel[k]) / a1;
    }
    let mut d_mean_jinit = d_mean;
    let mut d_std_jinit = d_std;
    for k in 0..N_CP_PARAMS {
        d_mean_jinit[k] += d_ipeak[0] * i_peak * d_log_a1[k];
        d_std_jinit[k] += d_ipeak[1] * i_peak * d_log_a1[k];
    }
    d_mean_jinit[KC_SCALE] += d_ipeak[0] * i_peak / dp.scale_r;
    d_std_jinit[KC_SCALE] += d_ipeak[1] * i_peak / dp.scale_r;
    let contract_seconds = t2.elapsed().as_secs_f64();

    let newton_iterations = ev
        .solves
        .iter()
        .map(|s| s.solution.solution.iterations)
        .sum();
    let forward_seconds = ev.solves.iter().map(|s| s.forward_seconds).sum();
    Ok((
        CpDesignGradient {
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

/// `[mean, std]` at `dp` moved by `h` in component `k` (mesh re-moved for a
/// geometric component), warm started from `base`.
#[allow(clippy::too_many_arguments)]
pub fn objectives_at(
    setup: &CpDesignSetup,
    dp: &CpDesignPoint,
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

/// `J(θ ± h e_k)` for each `h` in `hs`; steps where either side has no
/// valid mesh are returned separately (with the error).
#[allow(clippy::too_many_arguments)]
pub fn step_sweep(
    setup: &CpDesignSetup,
    dp: &CpDesignPoint,
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
            if k < N_CP_PARAMS {
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
