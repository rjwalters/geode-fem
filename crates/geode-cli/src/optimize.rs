//! `geode optimize <spec>`: the gradient-based design loop (Epic #841
//! Phase 6b, issue #1261).
//!
//! No new adjoint math and no second parameter vocabulary. The spec's
//! `optimize` section ([`crate::spec::OptimizeSpec`]) names parameters and
//! observables of its `sensitivity` section (Phase 5a,
//! [`crate::s_sensitivity`]); this module
//!
//! 1. **resolves** the section against them ([`resolve`], also run by `geode
//!    check`);
//! 2. adapts the library's N-port S-matrix sensitivity to
//!    [`geode_optimize::Objective`] ([`Evaluator`]): a design vector is
//!    mapped to parameter values, the mesh is **morphed** from the loaded
//!    one along the fixed shape columns (topology frozen) and the materials
//!    are set, one forward + adjoint sweep returns every observable and its
//!    gradient, and the weighted objective and the constraints are chained
//!    from them ([`term_value`]);
//! 3. drives [`geode_optimize::Lbfgsb`] (no constraints) or
//!    [`geode_optimize::Mma`] (constraints) one iteration at a time,
//!    appending a JSONL history entry, checkpointing, and keeping the
//!    best-so-far spec and mesh.
//!
//! **Morph map.** `nodes(p) = nodes₀ + Σₖ (pₖ − pₖ⁰)/length_unit · colₖ`,
//! with `colₖ` the 5a shape column of the **loaded** mesh. The gradient is
//! taken along that same fixed column, so the objective and its gradient
//! describe one function of `p`, and a resumed run (same spec) reproduces
//! the uninterrupted one bit for bit.
//!
//! **Failed evaluations** (a forward error, the passivity tripwire, the
//! mesh-quality guard `min_volume_ratio`) are `EvalError`s: the optimizers
//! back off and count them. A failure at the starting design is fatal.
//!
//! **Re-meshing** (issue #1303, `optimize.remesh`). Without that section the
//! run only backs off at the guard. With it, the spec's mesh is rung 0 of a
//! *ladder* (the listed `rungs` follow): complete geode specs, each a mesh
//! and the parameter values it was meshed at. The topology stays frozen
//! within an outer iteration and its line search; at the iteration boundary,
//! when a trial in that iteration tripped the guard (or every `every`
//! iterations), the run moves to the nearest later rung that accepts the
//! current design, carries the design over as absolute physical values,
//! re-baselines the objective there and starts a **fresh** optimizer state
//! (no L-BFGS memory). A `remesh` line in the history records the objective
//! discontinuity. The checkpoint carries the active rung and the segment
//! bookkeeping (`geode_remesh`), so a resumed run is bit-exact across a
//! re-mesh. The waveguide iris golden is tracked in #1471.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use faer::c64;
use geode_core::mesh::TaggedTetMesh;
use geode_optimize::{
    Bounds, EvalError, Evaluation, Lbfgsb, LbfgsbOptions, Mma, MmaOptions, Objective,
    OptimizeError, Optimizer, Scaled, Status, StopCriteria,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::CliError;
use crate::port_solves::PortSolves;
use crate::problem::{self, Problem, SensitivityParameter};
use crate::progress::SweepOptions;
use crate::report::{FileRef, OptimizeFdCheck, OptimizeReport, OptimizeVariableResult, Provenance};
use crate::s_sensitivity::{self, Design};
use crate::spec::{
    Analysis, OptimizeAggregate, OptimizeSpec, SensitivityParameterKind, ShapeMotion, TargetMode,
};

/// The tag every mesh-quality-guard failure message starts with; the re-mesh
/// trigger finds it in an iteration's notes.
const GUARD_TAG: &str = "mesh-quality guard";
/// The checkpoint key holding [`RemeshState`] (ignored by the optimizers).
const CHECKPOINT_KEY: &str = "geode_remesh";

/// Default mesh-quality budget ([`OptimizeSpec::min_volume_ratio`]).
const DEFAULT_MIN_VOLUME_RATIO: f64 = 0.1;
/// Default passivity tolerance ([`OptimizeSpec::passivity_tol`]).
const DEFAULT_PASSIVITY_TOL: f64 = 1e-6;
/// Default KS sharpness.
const DEFAULT_KS_RHO: f64 = 50.0;
/// Default iteration cap.
const DEFAULT_MAX_ITER: usize = 50;
/// A design is feasible when every `gᵢ ≤` this.
const FEAS_TOL: f64 = 1e-6;
/// Evaluation diagnostics kept for the history lookup.
const DIAG_CACHE: usize = 64;

/// Output file names inside the output directory.
const HISTORY_FILE: &str = "history.jsonl";
const CHECKPOINT_FILE: &str = "checkpoint.json";
const BEST_SPEC_FILE: &str = "best_spec.json";
const BEST_MESH_FILE: &str = "best_mesh.msh";

// ---------------------------------------------------------------------------
// Resolution
// ---------------------------------------------------------------------------

fn invalid(msg: impl Into<String>) -> CliError {
    CliError::InvalidSpec(format!("`optimize`: {}", msg.into()))
}

/// A resolved design variable.
#[derive(Debug, Clone)]
pub struct Var {
    /// Index into the sensitivity parameters.
    pub index: usize,
    /// The parameter's report name.
    pub name: String,
    /// Bounds, starting value and optimizer unit, in the parameter's unit.
    pub lower: f64,
    /// Upper bound.
    pub upper: f64,
    /// Starting value.
    pub initial: f64,
    /// Length of one optimizer unit.
    pub scale: f64,
}

/// A resolved objective term.
#[derive(Debug, Clone)]
pub struct Term {
    /// Index into the sensitivity observables.
    pub obs: usize,
    /// Positions into `sensitivity.frequency_indices` inside the band.
    pub pos: Vec<usize>,
    /// How residuals aggregate.
    pub aggregate: OptimizeAggregate,
    /// Which side of the target.
    pub mode: TargetMode,
    /// Target value.
    pub target: f64,
    /// Weight.
    pub weight: f64,
    /// KS sharpness.
    pub rho: f64,
}

/// A resolved constraint.
#[derive(Debug, Clone)]
pub struct Con {
    /// Index into the sensitivity observables.
    pub obs: usize,
    /// Positions into `sensitivity.frequency_indices` inside the band.
    pub pos: Vec<usize>,
    /// `Below` or `Above`.
    pub mode: TargetMode,
    /// The bound.
    pub bound: f64,
    /// KS sharpness.
    pub rho: f64,
}

/// The resolved `optimize` section.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Design variables.
    pub vars: Vec<Var>,
    /// Objective terms.
    pub terms: Vec<Term>,
    /// Constraints.
    pub cons: Vec<Con>,
    /// Mesh-quality budget.
    pub min_volume_ratio: f64,
    /// Passivity tolerance.
    pub passivity_tol: f64,
    /// Stopping criteria.
    pub stop: StopCriteria,
    /// The re-mesh policy (`optimize.remesh`), when present.
    pub remesh: Option<RemeshPlan>,
}

/// The resolved `optimize.remesh` section.
#[derive(Debug, Clone)]
pub struct RemeshPlan {
    /// Cadence in iterations on a rung.
    pub every: Option<usize>,
    /// The later rungs' spec paths, as written.
    pub rungs: Vec<PathBuf>,
}

/// A sensitivity parameter's name in the `optimize` section.
fn param_name(prm: &SensitivityParameter) -> String {
    match &prm.shape {
        Some(sh) => sh.name.clone(),
        None => format!("{}({})", prm.kind.name(), prm.physical_group),
    }
}

fn band_positions(p: &Problem, at: &str, band: Option<[f64; 2]>) -> Result<Vec<usize>, CliError> {
    let sens = p.sensitivity.as_ref().expect("checked by resolve");
    let [lo, hi] = band.unwrap_or([f64::NEG_INFINITY, f64::INFINITY]);
    if lo.is_nan() || hi.is_nan() || lo > hi {
        return Err(invalid(format!(
            "{at}: band_hz [{lo}, {hi}] must be [lo, hi] with lo ≤ hi"
        )));
    }
    let pos: Vec<usize> = sens
        .frequency_indices
        .iter()
        .enumerate()
        .filter(|&(_, &fi)| (lo..=hi).contains(&p.frequencies[fi].hz))
        .map(|(s, _)| s)
        .collect();
    if pos.is_empty() {
        let f: Vec<String> = sens
            .frequency_indices
            .iter()
            .map(|&fi| format!("{}", p.frequencies[fi].hz))
            .collect();
        return Err(invalid(format!(
            "{at}: no differentiated frequency lies in band_hz [{lo}, {hi}] (they are {} Hz)",
            f.join(", ")
        )));
    }
    Ok(pos)
}

fn find_observable(p: &Problem, at: &str, label: &str) -> Result<usize, CliError> {
    let sens = p.sensitivity.as_ref().expect("checked by resolve");
    sens.observables
        .iter()
        .position(|o| o.label == label)
        .ok_or_else(|| {
            let known: Vec<&str> = sens.observables.iter().map(|o| o.label.as_str()).collect();
            invalid(format!(
                "{at}: no sensitivity observable labelled `{label}` (the labels are: {})",
                known.join("; ")
            ))
        })
}

/// Validate the spec's `optimize` section against its `sensitivity`
/// section and resolve it ([`Plan`]). `geode check` runs this too.
///
/// # Errors
///
/// `invalid_spec`, naming the offending field.
pub fn resolve(p: &Problem) -> Result<Plan, CliError> {
    let o: &OptimizeSpec = p
        .spec
        .optimize
        .as_ref()
        .ok_or_else(|| invalid("the spec has no `optimize` section"))?;
    if p.analysis != Analysis::Driven {
        return Err(invalid(
            "needs a driven spec (no `eigen` / `extract` / `capacitance` / `inductance` section)",
        ));
    }
    let Some(sens) = p.sensitivity.as_ref().filter(|s| s.n_port) else {
        return Err(invalid(
            "needs a `sensitivity` section with `observables` (the N-port path): its parameters \
             are the design variables and its observables the objective's terms",
        ));
    };
    if o.variables.is_empty() {
        return Err(invalid(
            "`variables` must list at least one design variable",
        ));
    }
    if o.objective.is_empty() {
        return Err(invalid("`objective` must list at least one term"));
    }
    let mut vars: Vec<Var> = Vec::new();
    for (i, v) in o.variables.iter().enumerate() {
        let at = format!("variables[{i}] (`{}`)", v.parameter);
        let Some(index) = sens
            .parameters
            .iter()
            .position(|q| param_name(q) == v.parameter)
        else {
            let known: Vec<String> = sens.parameters.iter().map(param_name).collect();
            return Err(invalid(format!(
                "{at}: no sensitivity parameter of that name (the names are: {})",
                known.join(", ")
            )));
        };
        if vars.iter().any(|w| w.index == index) {
            return Err(invalid(format!("{at}: listed twice")));
        }
        let p0 = sens.parameters[index].value;
        let (lo, hi) = (v.lower, v.upper);
        if !(lo.is_finite() && hi.is_finite() && lo < hi) {
            return Err(invalid(format!(
                "{at}: needs finite bounds with lower < upper (got [{lo}, {hi}])"
            )));
        }
        let initial = v.initial.unwrap_or(p0);
        if !(initial.is_finite() && (lo..=hi).contains(&initial)) {
            return Err(invalid(format!(
                "{at}: initial {initial} is outside the bounds [{lo}, {hi}]"
            )));
        }
        let scale = v.scale.unwrap_or(hi - lo);
        if !(scale.is_finite() && scale > 0.0) {
            return Err(invalid(format!("{at}: scale must be finite and > 0")));
        }
        vars.push(Var {
            index,
            name: v.parameter.clone(),
            lower: lo,
            upper: hi,
            initial,
            scale,
        });
    }
    let mut terms = Vec::new();
    for (i, t) in o.objective.iter().enumerate() {
        let at = format!("objective[{i}]");
        let weight = t.weight.unwrap_or(1.0);
        if !(weight.is_finite() && weight >= 0.0) {
            return Err(invalid(format!("{at}: weight must be finite and ≥ 0")));
        }
        let rho = t.ks_rho.unwrap_or(DEFAULT_KS_RHO);
        if !(rho.is_finite() && rho > 0.0) {
            return Err(invalid(format!("{at}: ks_rho must be finite and > 0")));
        }
        if !t.target.is_finite() {
            return Err(invalid(format!("{at}: target must be finite")));
        }
        terms.push(Term {
            obs: find_observable(p, &at, &t.observable)?,
            pos: band_positions(p, &at, t.band_hz)?,
            aggregate: t.aggregate.unwrap_or(OptimizeAggregate::SumSq),
            mode: t.mode.unwrap_or(TargetMode::Equal),
            target: t.target,
            weight,
            rho,
        });
    }
    let mut cons = Vec::new();
    for (i, c) in o.constraints.iter().enumerate() {
        let at = format!("constraints[{i}]");
        if c.mode == TargetMode::Equal {
            return Err(invalid(format!(
                "{at}: mode must be `below` or `above` (an inequality)"
            )));
        }
        let rho = c.ks_rho.unwrap_or(DEFAULT_KS_RHO);
        if !(rho.is_finite() && rho > 0.0 && c.bound.is_finite()) {
            return Err(invalid(format!(
                "{at}: bound must be finite and ks_rho finite and > 0"
            )));
        }
        cons.push(Con {
            obs: find_observable(p, &at, &c.observable)?,
            pos: band_positions(p, &at, c.band_hz)?,
            mode: c.mode,
            bound: c.bound,
            rho,
        });
    }
    let min_volume_ratio = o.min_volume_ratio.unwrap_or(DEFAULT_MIN_VOLUME_RATIO);
    if !(0.0..1.0).contains(&min_volume_ratio) {
        return Err(invalid("min_volume_ratio must satisfy 0 ≤ r < 1"));
    }
    let passivity_tol = o.passivity_tol.unwrap_or(DEFAULT_PASSIVITY_TOL);
    if !(passivity_tol.is_finite() && passivity_tol >= 0.0) {
        return Err(invalid("passivity_tol must be finite and ≥ 0"));
    }
    let mut stop = StopCriteria {
        max_iter: DEFAULT_MAX_ITER,
        ..StopCriteria::default()
    };
    if let Some(s) = &o.stop {
        stop.max_iter = s.max_iter.unwrap_or(stop.max_iter);
        stop.max_evals = s.max_evals.unwrap_or(stop.max_evals);
        stop.pg_tol = s.pg_tol.unwrap_or(stop.pg_tol);
        stop.f_rel_tol = s.f_rel_tol.unwrap_or(stop.f_rel_tol);
        if stop.max_iter == 0 || stop.max_evals == 0 || stop.pg_tol.is_nan() || stop.pg_tol < 0.0 {
            return Err(invalid(
                "stop: max_iter / max_evals must be ≥ 1 and pg_tol ≥ 0",
            ));
        }
    }
    let remesh = match &o.remesh {
        None => None,
        Some(r) => {
            if r.rungs.is_empty() {
                return Err(invalid("remesh: `rungs` must list at least one spec"));
            }
            if r.every == Some(0) {
                return Err(invalid("remesh: `every` must be ≥ 1"));
            }
            Some(RemeshPlan {
                every: r.every,
                rungs: r.rungs.clone(),
            })
        }
    };
    Ok(Plan {
        vars,
        terms,
        cons,
        min_volume_ratio,
        passivity_tol,
        stop,
        remesh,
    })
}

// ---------------------------------------------------------------------------
// Objective math (pure)
// ---------------------------------------------------------------------------

/// The residual `r` of value `v` against `target` and `dr/dv`.
fn residual(mode: TargetMode, target: f64, v: f64) -> (f64, f64) {
    match mode {
        TargetMode::Equal => (v - target, 1.0),
        TargetMode::Below if v > target => (v - target, 1.0),
        TargetMode::Above if v < target => (target - v, -1.0),
        _ => (0.0, 0.0),
    }
}

/// Kreisselmeier–Steinhauser smooth max of `s` with sharpness `rho`, and
/// its softmax weights (the gradient of the smooth max w.r.t. each `sᵢ`).
fn ks(s: &[f64], rho: f64) -> (f64, Vec<f64>) {
    let m = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let e: Vec<f64> = s.iter().map(|&x| (rho * (x - m)).exp()).collect();
    let sum: f64 = e.iter().sum();
    (m + sum.ln() / rho, e.iter().map(|x| x / sum).collect())
}

/// One term's unweighted value and gradient over the variables, from the
/// per-frequency `values[i]` and their gradients `grads[i]` (length `n`).
fn term_value(t: &Term, values: &[f64], grads: &[Vec<f64>]) -> (f64, Vec<f64>) {
    let n = grads.first().map_or(0, Vec::len);
    let rd: Vec<(f64, f64)> = values
        .iter()
        .map(|&v| residual(t.mode, t.target, v))
        .collect();
    // Weight of each frequency's squared residual in the aggregate.
    let (f, w): (f64, Vec<f64>) = match t.aggregate {
        OptimizeAggregate::SumSq => (rd.iter().map(|r| r.0 * r.0).sum(), vec![1.0; rd.len()]),
        OptimizeAggregate::Mean => {
            let c = 1.0 / rd.len() as f64;
            (
                c * rd.iter().map(|r| r.0 * r.0).sum::<f64>(),
                vec![c; rd.len()],
            )
        }
        OptimizeAggregate::Max => {
            let s: Vec<f64> = rd.iter().map(|r| r.0 * r.0).collect();
            ks(&s, t.rho)
        }
    };
    let mut g = vec![0.0; n];
    for ((&(r, dr), wi), gi) in rd.iter().zip(&w).zip(grads) {
        for (a, b) in g.iter_mut().zip(gi) {
            *a += wi * 2.0 * r * dr * b;
        }
    }
    (f, g)
}

/// One constraint's value `g ≤ 0` and gradient: the KS max over the band of
/// `v − bound` (below) or `bound − v` (above).
fn constraint_value(c: &Con, values: &[f64], grads: &[Vec<f64>]) -> (f64, Vec<f64>) {
    let n = grads.first().map_or(0, Vec::len);
    let sign = if c.mode == TargetMode::Below {
        1.0
    } else {
        -1.0
    };
    let s: Vec<f64> = values.iter().map(|&v| sign * (v - c.bound)).collect();
    let (g, w) = ks(&s, c.rho);
    let mut grad = vec![0.0; n];
    for (wi, gi) in w.iter().zip(grads) {
        for (a, b) in grad.iter_mut().zip(gi) {
            *a += wi * sign * b;
        }
    }
    (g, grad)
}

/// `σ_max` of the row-major `n × n` complex matrix `s`: the square root of
/// the dominant eigenvalue of `SᴴS`, by power iteration (200 steps; a slow
/// rate only occurs when the top two singular values nearly coincide, where
/// the Rayleigh quotient is already accurate).
pub fn sigma_max(s: &[c64], n: usize) -> f64 {
    let mut a = vec![c64::new(0.0, 0.0); n * n];
    for i in 0..n {
        for j in 0..n {
            a[i * n + j] = (0..n)
                .map(|k| s[k * n + i].conj() * s[k * n + j])
                .sum::<c64>();
        }
    }
    let mut v: Vec<c64> = (0..n)
        .map(|i| c64::new(1.0 + 0.1 * i as f64, 0.0))
        .collect();
    let mut lambda = 0.0;
    for _ in 0..200 {
        let w: Vec<c64> = (0..n)
            .map(|i| (0..n).map(|j| a[i * n + j] * v[j]).sum::<c64>())
            .collect();
        let norm = w.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt();
        let vn = v.iter().map(|x| x.norm_sqr()).sum::<f64>();
        lambda = v
            .iter()
            .zip(&w)
            .map(|(x, y)| (x.conj() * y).re)
            .sum::<f64>()
            / vn;
        if norm == 0.0 {
            return 0.0;
        }
        v = w.iter().map(|x| x / norm).collect();
    }
    lambda.max(0.0).sqrt()
}

// ---------------------------------------------------------------------------
// The evaluator (Objective adapter)
// ---------------------------------------------------------------------------

/// What one evaluation learned besides `f` and `∇f`.
#[derive(Debug, Clone, Default)]
struct Diag {
    sigma_max: f64,
    min_volume_ratio: f64,
    term_values: Vec<f64>,
    warnings: Vec<String>,
}

/// The CLI → [`Objective`] adapter over the 5a sensitivity path.
struct Evaluator<'a> {
    base: &'a Problem,
    d: &'a Design,
    plan: &'a Plan,
    /// The sensitivity parameters' values at the loaded design.
    p0: Vec<f64>,
    /// Frequency indices solved (all of them for a hybrid spec: tracking).
    solved: Vec<usize>,
    omegas: Vec<f64>,
    port_modes: bool,
    diag: RefCell<Vec<(Vec<u64>, Diag)>>,
}

fn bits(x: &[f64]) -> Vec<u64> {
    x.iter().map(|v| v.to_bits()).collect()
}

impl<'a> Evaluator<'a> {
    fn new(base: &'a Problem, d: &'a Design, plan: &'a Plan) -> Self {
        let sens = base.sensitivity.as_ref().expect("resolved");
        let hybrid = base.wave_ports.iter().any(|w| w.hybrid.is_some());
        let solved: Vec<usize> = if hybrid {
            (0..base.frequencies.len()).collect()
        } else {
            sens.frequency_indices.clone()
        };
        Self {
            base,
            d,
            plan,
            p0: sens.parameters.iter().map(|q| q.value).collect(),
            omegas: solved.iter().map(|&i| base.frequencies[i].k0).collect(),
            solved,
            port_modes: sens.observables.iter().any(|o| o.wave_port.is_some()),
            diag: RefCell::new(Vec::new()),
        }
    }

    /// Every sensitivity parameter's value for the variables' `phys`.
    fn params(&self, phys: &[f64]) -> Vec<f64> {
        let mut p = self.p0.clone();
        for (v, &x) in self.plan.vars.iter().zip(phys) {
            p[v.index] = x;
        }
        p
    }

    /// The problem at parameter values `params`: the loaded mesh morphed
    /// along the fixed shape columns, the materials set, the hybrid faces
    /// rebuilt (`accuracy`: keep their accuracy estimate, else off).
    fn morph(&self, params: &[f64], accuracy: bool) -> Result<Problem, CliError> {
        let base = self.base;
        let sens = base.sensitivity.as_ref().expect("resolved");
        let mut q = base.clone();
        let lu = base.length_unit_m();
        for (k, col) in self.d.columns.iter().enumerate() {
            let Some(col) = col else { continue };
            let t = (params[k] - self.p0[k]) / lu;
            if t == 0.0 {
                continue;
            }
            for (x, v) in q.tagged.mesh.nodes.iter_mut().zip(col) {
                x[0] += t * v[0];
                x[1] += t * v[1];
                x[2] += t * v[2];
            }
        }
        let mut regions: Vec<usize> = sens
            .parameters
            .iter()
            .filter(|m| m.kind.is_material())
            .map(|m| m.region)
            .collect();
        regions.sort_unstable();
        regions.dedup();
        for r in regions {
            let e0 = base.regions[r].eps_r;
            let (mut re, mut im) = (e0.re, e0.im);
            let of = |kind: SensitivityParameterKind| {
                sens.parameters
                    .iter()
                    .position(|m| m.region == r && m.kind == kind)
                    .map(|k| params[k])
            };
            if let Some(v) = of(SensitivityParameterKind::EpsR) {
                re = v;
            }
            if let Some(v) = of(SensitivityParameterKind::EpsRImag) {
                im = -v;
            }
            if let Some(v) = of(SensitivityParameterKind::TanDelta) {
                im = -re * v;
            }
            let e = c64::new(re, im);
            q.regions[r].eps_r = e;
            for (slot, &tr) in q.eps.iter_mut().zip(&sens.region_of_tet) {
                if tr == r {
                    *slot = e;
                }
            }
        }
        if !accuracy {
            for w in &mut q.wave_ports {
                if let Some(h) = &mut w.hybrid {
                    h.opts.accuracy = None;
                }
            }
        }
        q.rebuild_hybrid_faces()?;
        Ok(q)
    }

    fn evaluate_inner(&self, phys: &[f64]) -> Result<(Evaluation, Diag), String> {
        let params = self.params(phys);
        let q = self.morph(&params, false).map_err(|e| e.to_string())?;
        let ratio = geode_core::shape::min_tet_volume_ratio(&self.base.tagged.mesh, &q.tagged.mesh);
        if ratio.is_nan() || ratio < self.plan.min_volume_ratio {
            return Err(format!(
                "{GUARD_TAG}: the morphed mesh's smallest tet volume ratio {ratio:.3e} is \
                 below min_volume_ratio {} (an `optimize.remesh` ladder can replace the mesh)",
                self.plan.min_volume_ratio
            ));
        }
        let sens = q.sensitivity.as_ref().expect("resolved");
        let solves = PortSolves::new(&q);
        let sw = s_sensitivity::library_sweep(
            &q,
            &self.d.sdesign,
            &self.omegas,
            self.port_modes,
            &solves,
        )
        .map_err(|e| e.to_string())?;
        let n_lib = sw.params.len();
        let mut sigma = 0.0_f64;
        for pt in &sw.points {
            sigma = sigma.max(sigma_max(&pt.s, pt.n_ports));
        }
        if sigma.is_nan() || sigma > 1.0 + self.plan.passivity_tol {
            return Err(format!(
                "passivity tripwire: σ_max(S) = {sigma:.9} > 1 + {:e}",
                self.plan.passivity_tol
            ));
        }
        let mut warnings = Vec::new();
        for pt in &sw.points {
            for note in &pt.port_mode_warnings {
                warnings.push(format!(
                    "sensitivity at k0 = {}, wave port {} channel {}: {:?}",
                    pt.omega, note.port, note.channel, note.warning
                ));
            }
        }
        // (observable, position) → (value, gradient over the variables).
        let mut cache: HashMap<(usize, usize), (f64, Vec<f64>)> = HashMap::new();
        let mut at = |obs: usize, pos: usize| -> Result<(f64, Vec<f64>), String> {
            if let Some(hit) = cache.get(&(obs, pos)) {
                return Ok(hit.clone());
            }
            let fi = sens.frequency_indices[pos];
            let slot = self.solved.iter().position(|&s| s == fi).expect("solved");
            let (v, g) = s_sensitivity::point_observable(
                &q,
                &sens.observables[obs],
                &sw.points[slot],
                n_lib,
            )
            .map_err(|e| e.to_string())?;
            let gv: Vec<f64> = self
                .plan
                .vars
                .iter()
                .map(|var| {
                    let m = &self.d.map[var.index];
                    g[m.index] * m.factor
                })
                .collect();
            cache.insert((obs, pos), (v, gv.clone()));
            Ok((v, gv))
        };
        let n = self.plan.vars.len();
        let (mut f, mut grad) = (0.0, vec![0.0; n]);
        let mut term_values = Vec::new();
        for t in &self.plan.terms {
            let (mut vals, mut grads) = (Vec::new(), Vec::new());
            for &pos in &t.pos {
                let (v, g) = at(t.obs, pos)?;
                vals.push(v);
                grads.push(g);
            }
            let (ft, gt) = term_value(t, &vals, &grads);
            term_values.push(ft);
            f += t.weight * ft;
            for (a, b) in grad.iter_mut().zip(&gt) {
                *a += t.weight * b;
            }
        }
        let (mut gs, mut jac) = (Vec::new(), Vec::new());
        for c in &self.plan.cons {
            let (mut vals, mut grads) = (Vec::new(), Vec::new());
            for &pos in &c.pos {
                let (v, g) = at(c.obs, pos)?;
                vals.push(v);
                grads.push(g);
            }
            let (g, row) = constraint_value(c, &vals, &grads);
            gs.push(g);
            jac.push(row);
        }
        let ev = Evaluation::constrained(f, grad, gs, jac);
        let diag = Diag {
            sigma_max: sigma,
            min_volume_ratio: ratio,
            term_values,
            warnings,
        };
        Ok((ev, diag))
    }

    /// The diagnostics of the evaluation at `phys` (cached, else redone).
    fn diag_for(&self, phys: &[f64]) -> Option<Diag> {
        let key = bits(phys);
        if let Some((_, d)) = self.diag.borrow().iter().find(|(k, _)| *k == key) {
            return Some(d.clone());
        }
        self.evaluate_inner(phys).ok().map(|(_, d)| d)
    }

    /// The shipped forward's accuracy / mode warnings at `phys` (wave-port
    /// specs only; the sensitivity path switches the accuracy estimate off).
    fn forward_warnings(&self, phys: &[f64]) -> Vec<String> {
        if self.base.wave_ports.is_empty() {
            return Vec::new();
        }
        let run = || -> Result<Vec<String>, CliError> {
            let q = self.morph(&self.params(phys), true)?;
            let opts = SweepOptions {
                jobs: 1,
                jobs_explicit: false,
                progress: false,
            };
            let (_, _, _, warnings) = crate::driven::wave_sweep(&q, opts, &PortSolves::new(&q))?;
            Ok(warnings
                .iter()
                .map(|w| format!("{}: {}", w.kind, w.message))
                .collect())
        };
        run().unwrap_or_else(|e| vec![format!("forward_failed: {e}")])
    }
}

impl Objective for Evaluator<'_> {
    fn dim(&self) -> usize {
        self.plan.vars.len()
    }
    fn n_constraints(&self) -> usize {
        self.plan.cons.len()
    }
    fn evaluate(&mut self, phys: &[f64]) -> Result<Evaluation, EvalError> {
        let (ev, diag) = self.evaluate_inner(phys).map_err(EvalError::new)?;
        let mut cache = self.diag.borrow_mut();
        if cache.len() >= DIAG_CACHE {
            cache.remove(0);
        }
        cache.push((bits(phys), diag));
        Ok(ev)
    }
}

// ---------------------------------------------------------------------------
// Artifacts
// ---------------------------------------------------------------------------

fn io_err(path: &Path, err: std::io::Error) -> CliError {
    CliError::Io {
        path: path.to_path_buf(),
        err,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(|e| io_err(&tmp, e))?;
    std::fs::rename(&tmp, path).map_err(|e| io_err(path, e))
}

fn file_ref(dir: &Path, name: &str) -> Result<FileRef, CliError> {
    let path = dir.join(name);
    let bytes = std::fs::read(&path).map_err(|e| io_err(&path, e))?;
    Ok(FileRef {
        path: name.to_string(),
        sha256: Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    })
}

/// The tagged mesh as Gmsh MSH 4.1 ASCII (named physical volumes and
/// surfaces, one entity each), the format `geode` reads.
fn msh_text(t: &TaggedTetMesh) -> String {
    let name = |dim: i32, tag: i32| -> String {
        t.mesh
            .physical_groups
            .get(&(dim, tag))
            .cloned()
            .unwrap_or_else(|| format!("group_{dim}_{tag}"))
    };
    let tags = |all: &[i32]| -> Vec<i32> {
        let mut u: Vec<i32> = all.iter().copied().filter(|&x| x != 0).collect();
        u.sort_unstable();
        u.dedup();
        u
    };
    let vol_tags = tags(&t.tet_physical_tags);
    let surf_tags = tags(&t.triangle_physical_tags);
    let mut s = String::from("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", surf_tags.len() + vol_tags.len());
    for &tag in &surf_tags {
        let _ = writeln!(s, "2 {tag} \"{}\"", name(2, tag));
    }
    for &tag in &vol_tags {
        let _ = writeln!(s, "3 {tag} \"{}\"", name(3, tag));
    }
    s.push_str("$EndPhysicalNames\n");
    let _ = writeln!(s, "$Entities\n0 0 {} {}", surf_tags.len(), vol_tags.len());
    for (i, tag) in surf_tags.iter().enumerate() {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {tag} 0", i + 1);
    }
    let bound: Vec<String> = (1..=surf_tags.len()).map(|i| i.to_string()).collect();
    for (i, tag) in vol_tags.iter().enumerate() {
        let _ = writeln!(
            s,
            "{} 0 0 0 1 1 1 1 {tag} {} {}",
            i + 1,
            surf_tags.len(),
            bound.join(" ")
        );
    }
    s.push_str("$EndEntities\n");
    let n = t.mesh.nodes.len();
    let _ = writeln!(s, "$Nodes\n1 {n} 1 {n}\n3 1 0 {n}");
    for i in 1..=n {
        let _ = writeln!(s, "{i}");
    }
    for p in &t.mesh.nodes {
        let _ = writeln!(s, "{:.17e} {:.17e} {:.17e}", p[0], p[1], p[2]);
    }
    let count = |tags_of: &[i32], tag: i32| tags_of.iter().filter(|&&x| x == tag).count();
    let n_el: usize = surf_tags
        .iter()
        .map(|&g| count(&t.triangle_physical_tags, g))
        .sum::<usize>()
        + vol_tags
            .iter()
            .map(|&g| count(&t.tet_physical_tags, g))
            .sum::<usize>();
    let _ = writeln!(
        s,
        "$EndNodes\n$Elements\n{} {n_el} 1 {n_el}",
        surf_tags.len() + vol_tags.len()
    );
    let mut id = 1;
    for (i, &tag) in surf_tags.iter().enumerate() {
        let _ = writeln!(s, "2 {} 2 {}", i + 1, count(&t.triangle_physical_tags, tag));
        for (tri, _) in t
            .boundary_triangles
            .iter()
            .zip(&t.triangle_physical_tags)
            .filter(|(_, g)| **g == tag)
        {
            let _ = writeln!(s, "{id} {} {} {}", tri[0] + 1, tri[1] + 1, tri[2] + 1);
            id += 1;
        }
    }
    for (i, &tag) in vol_tags.iter().enumerate() {
        let _ = writeln!(s, "3 {} 4 {}", i + 1, count(&t.tet_physical_tags, tag));
        for (tet, _) in t
            .mesh
            .tets
            .iter()
            .zip(&t.tet_physical_tags)
            .filter(|(_, g)| **g == tag)
        {
            let _ = writeln!(
                s,
                "{id} {} {} {} {}",
                tet[0] + 1,
                tet[1] + 1,
                tet[2] + 1,
                tet[3] + 1
            );
            id += 1;
        }
    }
    s.push_str("$EndElements\n");
    s
}

/// The best design as a spec that re-runs from it: the morphed mesh (next
/// to the spec), the materials set, and each variable's `initial` / bounds
/// re-based onto the new mesh (a `translate` parameter is `0` at its own
/// mesh, so its bounds shift by the motion).
fn best_spec_json(ev: &Evaluator<'_>, phys: &[f64], rest: &[PathBuf]) -> Result<String, CliError> {
    let params = ev.params(phys);
    let q = ev.morph(&params, false)?;
    let sens = ev.base.sensitivity.as_ref().expect("resolved");
    let mut spec = ev.base.spec.clone();
    spec.mesh.path = PathBuf::from(BEST_MESH_FILE);
    let mut regions: Vec<usize> = sens
        .parameters
        .iter()
        .filter(|m| m.kind.is_material())
        .map(|m| m.region)
        .collect();
    regions.sort_unstable();
    regions.dedup();
    for r in regions {
        let (name, e) = (&q.regions[r].name, q.regions[r].eps_r);
        match spec
            .materials
            .iter_mut()
            .find(|m| &m.physical_group == name)
        {
            Some(m) => m.eps_r = [e.re, e.im],
            None => spec.materials.push(
                serde_json::from_value(serde_json::json!({
                    "physical_group": name, "eps_r": [e.re, e.im]
                }))
                .map_err(|e| invalid(format!("cannot record material: {e}")))?,
            ),
        }
    }
    if let Some(o) = &mut spec.optimize {
        // The best mesh stands in for the rung it was found on; only the
        // later rungs (by absolute path) remain to re-mesh onto.
        if rest.is_empty() {
            o.remesh = None;
        } else if let Some(r) = &mut o.remesh {
            r.rungs = rest.to_vec();
        }
        for ((v, var), &at) in o.variables.iter_mut().zip(&ev.plan.vars).zip(phys) {
            let prm = &sens.parameters[var.index];
            let translate = matches!(
                prm.shape.as_ref().map(|s| s.shape_motion),
                Some(ShapeMotion::Translate)
            );
            let shift = if translate { at - prm.value } else { 0.0 };
            v.lower = var.lower - shift;
            v.upper = var.upper - shift;
            v.initial = Some(at - shift);
        }
    }
    let mut json = serde_json::to_string_pretty(&spec)?;
    json.push('\n');
    Ok(json)
}

fn write_best(
    ev: &Evaluator<'_>,
    dir: &Path,
    phys: &[f64],
    rest: &[PathBuf],
) -> Result<(), CliError> {
    let q = ev.morph(&ev.params(phys), false)?;
    write_atomic(&dir.join(BEST_MESH_FILE), msh_text(&q.tagged).as_bytes())?;
    write_atomic(
        &dir.join(BEST_SPEC_FILE),
        best_spec_json(ev, phys, rest)?.as_bytes(),
    )
}

// ---------------------------------------------------------------------------
// The loop
// ---------------------------------------------------------------------------

/// One line of `history.jsonl`.
#[derive(Serialize)]
struct HistoryEntry {
    iter: usize,
    /// The variables' values, in the parameters' units, in `variables` order.
    x: Vec<f64>,
    objective: f64,
    /// `‖∇f‖∞` in optimizer units.
    gradient_norm: f64,
    /// First-order residual (projected gradient / KKT) in optimizer units.
    pg_norm: f64,
    constraints: Vec<f64>,
    max_violation: f64,
    step_length: f64,
    n_evals: usize,
    n_failed_evals: usize,
    failed_this_iter: usize,
    notes: Vec<String>,
    term_values: Vec<f64>,
    passivity_sigma_max: Option<f64>,
    min_volume_ratio: Option<f64>,
    /// Port-accuracy / mode-gap warnings at this iterate.
    warnings: Vec<String>,
}

/// The best design seen: feasible first, then lowest objective; with no
/// feasible design yet, the least violated.
#[derive(Debug, Clone)]
struct Best {
    phys: Vec<f64>,
    f: f64,
    violation: f64,
}

impl Best {
    fn improves(&self, f: f64, violation: f64) -> bool {
        if violation <= FEAS_TOL {
            self.violation > FEAS_TOL || f < self.f
        } else {
            self.violation > FEAS_TOL && violation < self.violation
        }
    }
}

/// Segment bookkeeping of a re-meshing run, stored in the checkpoint under
/// [`CHECKPOINT_KEY`]. A *segment* is the run on one rung since the last
/// re-mesh; the optimizer state counts from zero in each.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RemeshState {
    /// The active rung (0 = the spec's own mesh).
    rung: usize,
    /// Iterations completed before this segment (the history label of the
    /// segment's iteration 0).
    iter_base: usize,
    /// Evaluations / failed evaluations spent before this segment.
    evals_base: usize,
    failed_base: usize,
    /// History lines before this segment's first iteration line (earlier
    /// segments and the `remesh` line that opened this one).
    lines_before: usize,
    /// Re-meshes so far.
    remeshes: usize,
    /// Bits of the objective at the very first starting point.
    f_initial_bits: Option<u64>,
}

struct Ctx {
    dir: PathBuf,
    best: Option<Best>,
    history: Vec<String>,
    /// Whether the spec has an `optimize.remesh` section (the checkpoint
    /// then carries [`RemeshState`]).
    remesh: bool,
    rm: RemeshState,
    /// Absolute paths of rungs 1.. (empty without `optimize.remesh`).
    ladder: Vec<PathBuf>,
}

/// A loaded rung of the ladder.
struct Rung {
    p: Problem,
    d: Design,
    /// The spec path (`None` for rung 0, the spec itself).
    path: Option<PathBuf>,
}

/// Why an iteration ended in a re-mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// A trial in the iteration tripped the mesh-quality guard.
    Guard,
    /// The `every` cadence.
    Every,
}

impl Trigger {
    fn name(self) -> &'static str {
        match self {
            Trigger::Guard => "mesh_quality_guard",
            Trigger::Every => "every",
        }
    }
}

/// What the next segment learns about the re-mesh that opened it.
struct Pending {
    trigger: Trigger,
    from_rung: usize,
    to_rung: usize,
    rung: String,
    x: Vec<f64>,
    f_before: f64,
}

/// The `remesh` history line.
#[derive(Serialize)]
struct RemeshEntry {
    kind: &'static str,
    /// The iteration it follows (the label of the last iteration on the
    /// old mesh; the new mesh's baseline line carries the same label).
    iter: usize,
    trigger: &'static str,
    from_rung: usize,
    to_rung: usize,
    rung: String,
    /// The design carried over, in the parameters' units.
    x: Vec<f64>,
    objective_before: f64,
    objective_after: f64,
    objective_discontinuity: f64,
}

/// Whether `rec`'s notes show a guard failure.
fn guard_tripped(rec: Option<&geode_optimize::IterationRecord>) -> bool {
    rec.is_some_and(|r| r.notes.iter().any(|n| n.contains(GUARD_TAG)))
}

/// The nearest later rung (by distance in optimizer units between its
/// meshed parameter values and `phys`) whose mesh accepts `phys` under the
/// quality budget.
fn pick_rung(rungs: &[Rung], plan: &Plan, from: usize, phys: &[f64]) -> Option<usize> {
    (from + 1..rungs.len())
        .filter_map(|j| {
            let r = &rungs[j];
            let ev = Evaluator::new(&r.p, &r.d, plan);
            let q = ev.morph(&ev.params(phys), false).ok()?;
            let ratio = geode_core::shape::min_tet_volume_ratio(&r.p.tagged.mesh, &q.tagged.mesh);
            if ratio.is_nan() || ratio < plan.min_volume_ratio {
                return None;
            }
            let sens = r.p.sensitivity.as_ref()?;
            let dist = plan
                .vars
                .iter()
                .zip(phys)
                .map(|(v, &x)| ((x - sens.parameters[v.index].value) / v.scale).powi(2))
                .sum::<f64>();
            Some((j, dist))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(j, _)| j)
}

/// Rung `r` must describe the same sensitivity problem as the spec: the
/// plan's indices are used for every rung.
fn check_rung(base: &Problem, r: &Problem, at: &str) -> Result<(), CliError> {
    let (Some(a), Some(b)) = (base.sensitivity.as_ref(), r.sensitivity.as_ref()) else {
        return Err(invalid(format!(
            "remesh: {at} has no `sensitivity` section"
        )));
    };
    let names = |s: &crate::problem::SensitivityTarget| -> Vec<String> {
        s.parameters.iter().map(param_name).collect()
    };
    let labels = |s: &crate::problem::SensitivityTarget| -> Vec<String> {
        s.observables.iter().map(|o| o.label.clone()).collect()
    };
    let freqs = |p: &Problem, s: &crate::problem::SensitivityTarget| -> Vec<u64> {
        s.frequency_indices
            .iter()
            .map(|&i| p.frequencies[i].hz.to_bits())
            .collect()
    };
    if !b.n_port || names(a) != names(b) || labels(a) != labels(b) || freqs(base, a) != freqs(r, b)
    {
        return Err(invalid(format!(
            "remesh: {at} must have the same `sensitivity` parameters, observables and \
             differentiated frequencies as the spec"
        )));
    }
    Ok(())
}

/// Insert the segment bookkeeping into a checkpoint JSON text.
fn with_remesh_state(cp: &str, rm: &RemeshState) -> Result<String, CliError> {
    let mut v: serde_json::Value = serde_json::from_str(cp)?;
    v[CHECKPOINT_KEY] = serde_json::to_value(rm)?;
    Ok(serde_json::to_string_pretty(&v)?)
}

/// The segment bookkeeping of a checkpoint text (zeros when absent).
fn remesh_state_of(text: &str) -> Result<RemeshState, CliError> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    match v.get(CHECKPOINT_KEY) {
        Some(x) => Ok(serde_json::from_value(x.clone())?),
        None => Ok(RemeshState::default()),
    }
}

fn opt_err(e: OptimizeError) -> CliError {
    match e {
        OptimizeError::InitialEvaluation(e) => {
            CliError::SensitivitySolve(format!("the objective failed at the starting design: {e}"))
        }
        e => CliError::InvalidSpec(format!("`optimize`: {e}")),
    }
}

/// Fold a record into the best-so-far; whether it is the new best.
fn note_best(ctx: &mut Ctx, rec: &geode_optimize::IterationRecord, phys: &[f64]) -> bool {
    let improved = ctx
        .best
        .as_ref()
        .is_none_or(|b| b.improves(rec.f, rec.max_violation));
    if improved {
        ctx.best = Some(Best {
            phys: phys.to_vec(),
            f: rec.f,
            violation: rec.max_violation,
        });
    }
    improved
}

/// Drive one segment: until the optimizer finishes (`None`) or `check`
/// asks for a re-mesh at the iteration boundary (`Some`, the state is
/// returned as it stands).
#[allow(clippy::too_many_arguments)]
fn drive<O: Optimizer>(
    opt: &O,
    resumed: Option<O::State>,
    scaled: &mut Scaled<Evaluator<'_>>,
    bounds: &Bounds,
    x0: &[f64],
    ctx: &mut Ctx,
    pending: Option<&Pending>,
    check: &dyn Fn(&geode_optimize::Progress) -> Option<Trigger>,
) -> Result<(O::State, Option<Trigger>), CliError> {
    let is_resume = resumed.is_some();
    let mut state = match resumed {
        Some(s) => s,
        None => opt.start(scaled, x0, bounds).map_err(opt_err)?,
    };
    if ctx.rm.f_initial_bits.is_none() {
        ctx.rm.f_initial_bits = Some(O::progress(&state).f.to_bits());
    }
    if let (false, Some(pd)) = (is_resume, pending) {
        let after = O::progress(&state).f;
        let entry = RemeshEntry {
            kind: "remesh",
            iter: ctx.rm.iter_base,
            trigger: pd.trigger.name(),
            from_rung: pd.from_rung,
            to_rung: pd.to_rung,
            rung: pd.rung.clone(),
            x: pd.x.clone(),
            objective_before: pd.f_before,
            objective_after: after,
            objective_discontinuity: after - pd.f_before,
        };
        ctx.history.push(serde_json::to_string(&entry)?);
        ctx.rm.lines_before = ctx.history.len();
    }
    // Resume: keep the lines of the earlier segments and those of this one
    // up to the state's iteration (a later segment's lines are dropped).
    if is_resume && let Ok(text) = std::fs::read_to_string(ctx.dir.join(HISTORY_FILE)) {
        let keep = ctx.rm.iter_base + O::progress(&state).iter;
        let lines: Vec<&str> = text.lines().collect();
        let head = ctx.rm.lines_before.min(lines.len());
        ctx.history = lines[..head].iter().map(|l| (*l).to_string()).collect();
        ctx.history.extend(
            lines[head..]
                .iter()
                .filter(|l| {
                    serde_json::from_str::<serde_json::Value>(l)
                        .ok()
                        .filter(|v| v.get("kind").is_none())
                        .and_then(|v| v["iter"].as_u64())
                        .is_some_and(|i| i as usize <= keep)
                })
                .map(|l| (*l).to_string()),
        );
        ctx.rm.lines_before = head;
    }
    // The state's records: re-derive the best design and write the entries
    // not on disk yet (iteration 0 of a fresh segment).
    let present = ctx.history.len().saturating_sub(ctx.rm.lines_before);
    for (idx, rec) in O::progress(&state).history.clone().into_iter().enumerate() {
        let phys = scaled.to_physical(&rec.x);
        let improved = note_best(ctx, &rec, &phys);
        if idx >= present {
            record(opt, &state, &*scaled, ctx, &rec, &phys, improved)?;
        }
    }
    loop {
        if O::progress(&state).status.is_finished() {
            return Ok((state, None));
        }
        if let Some(t) = check(O::progress(&state)) {
            return Ok((state, Some(t)));
        }
        let before = O::progress(&state).history.len();
        opt.step(scaled, &mut state).map_err(opt_err)?;
        for rec in O::progress(&state).history[before..].iter().cloned() {
            let phys = scaled.to_physical(&rec.x);
            let improved = note_best(ctx, &rec, &phys);
            record(opt, &state, &*scaled, ctx, &rec, &phys, improved)?;
        }
    }
}

/// Write one iteration: history line, best artifacts, checkpoint.
fn record<O: Optimizer>(
    opt: &O,
    state: &O::State,
    scaled: &Scaled<Evaluator<'_>>,
    ctx: &mut Ctx,
    rec: &geode_optimize::IterationRecord,
    phys: &[f64],
    improved: bool,
) -> Result<(), CliError> {
    let ev = scaled.inner();
    let diag = ev.diag_for(phys);
    let mut warnings = diag
        .as_ref()
        .map(|d| d.warnings.clone())
        .unwrap_or_default();
    warnings.extend(ev.forward_warnings(phys));
    let entry = HistoryEntry {
        iter: ctx.rm.iter_base + rec.iter,
        x: phys.to_vec(),
        objective: rec.f,
        gradient_norm: O::progress(state)
            .grad
            .iter()
            .fold(0.0_f64, |m, g| m.max(g.abs())),
        pg_norm: rec.pg_norm,
        constraints: rec.constraints.clone(),
        max_violation: rec.max_violation,
        step_length: rec.step_length,
        n_evals: ctx.rm.evals_base + rec.n_evals,
        n_failed_evals: ctx.rm.failed_base + rec.n_failed_evals,
        failed_this_iter: rec.failed_this_iter,
        notes: rec.notes.clone(),
        term_values: diag
            .as_ref()
            .map(|d| d.term_values.clone())
            .unwrap_or_default(),
        passivity_sigma_max: diag.as_ref().map(|d| d.sigma_max),
        min_volume_ratio: diag.as_ref().map(|d| d.min_volume_ratio),
        warnings,
    };
    ctx.history.truncate(ctx.rm.lines_before + rec.iter);
    ctx.history.push(serde_json::to_string(&entry)?);
    let mut text = ctx.history.join("\n");
    text.push('\n');
    write_atomic(&ctx.dir.join(HISTORY_FILE), text.as_bytes())?;
    if improved || ctx.best.is_none() {
        let rest = ctx.ladder.get(ctx.rm.rung..).unwrap_or_default();
        write_best(ev, &ctx.dir, phys, rest)?;
    }
    let mut cp = opt.checkpoint_json(state).map_err(opt_err)?;
    if ctx.remesh {
        cp = with_remesh_state(&cp, &ctx.rm)?;
    }
    write_atomic(&ctx.dir.join(CHECKPOINT_FILE), cp.as_bytes())
}

/// A run that ended on its iteration / evaluation budget continues when
/// resumed (the budget is the spec's, read afresh).
fn reopen(status: &mut Status) {
    if matches!(status, Status::MaxIterations | Status::MaxEvaluations) {
        *status = Status::Running;
    }
}

fn status_text(s: &Status) -> String {
    match s {
        Status::Running => "running".to_string(),
        Status::Converged { reason } => format!("converged:{reason:?}").to_lowercase(),
        Status::MaxIterations => "max_iterations".to_string(),
        Status::MaxEvaluations => "max_evaluations".to_string(),
        Status::Stalled { message } => format!("stalled: {message}"),
    }
}

/// The Phase 5a FD self-check at design `phys`, through the shipped forward.
fn final_fd_check(ev: &Evaluator<'_>, phys: &[f64], jobs: usize) -> OptimizeFdCheck {
    let run = || -> Result<_, CliError> {
        let params = ev.params(phys);
        let q = ev.morph(&params, false)?;
        let mut sens = q.sensitivity.clone().expect("resolved");
        for (prm, &v) in sens.parameters.iter_mut().zip(&params) {
            prm.value = v;
        }
        sens.fd_check.get_or_insert((
            crate::spec::default_fd_relative_step(),
            crate::spec::default_fd_tolerance(),
        ));
        let rows = s_sensitivity::forward_rows(&q, jobs)?;
        let solves = PortSolves::new(&q);
        s_sensitivity::driven(&q, &sens, &rows, jobs, &solves)
    };
    match run() {
        Ok(r) => OptimizeFdCheck {
            passed: true,
            summary: r.fd_check,
            detail: None,
        },
        Err(e) => OptimizeFdCheck {
            passed: false,
            summary: None,
            detail: Some(e.to_string()),
        },
    }
}

/// One optimizer's hooks for the generic segment loop.
struct Alg<O: Optimizer> {
    /// The options a fresh run starts from.
    options: O::Options,
    /// Replace the stopping criteria in `options`.
    set_stop: fn(&mut O::Options, StopCriteria),
}

/// Run segments (one per rung visited) to the end of the run. Returns the
/// final progress, the evaluator's `Scaled` wrapper of the last segment and
/// the rung it ran on.
fn run_segments<'a, O: Optimizer>(
    alg: &Alg<O>,
    resumed: Option<O::State>,
    ctx: &mut Ctx,
    plan: &'a Plan,
    rungs: &'a [Rung],
) -> Result<(geode_optimize::Progress, Scaled<Evaluator<'a>>), CliError> {
    let offset: Vec<f64> = plan.vars.iter().map(|v| v.initial).collect();
    let scale: Vec<f64> = plan.vars.iter().map(|v| v.scale).collect();
    let phys_bounds = Bounds::new(
        plan.vars.iter().map(|v| v.lower).collect(),
        plan.vars.iter().map(|v| v.upper).collect(),
    )
    .map_err(opt_err)?;
    let mut resumed = resumed;
    let mut pending: Option<Pending> = None;
    let mut carry: Option<Vec<f64>> = None;
    loop {
        let rung = &rungs[ctx.rm.rung];
        let ev = Evaluator::new(&rung.p, &rung.d, plan);
        let mut scaled = Scaled::new(ev, offset.clone(), scale.clone(), 1.0);
        let bounds = scaled.bounds_to_design(&phys_bounds);
        let x0 = match &carry {
            Some(phys) => scaled.to_design(phys),
            None => vec![0.0; plan.vars.len()],
        };
        // The budget is the run's: this segment gets what the earlier ones
        // left.
        let mut stop = plan.stop.clone();
        stop.max_iter = stop.max_iter.saturating_sub(ctx.rm.iter_base).max(1);
        stop.max_evals = stop.max_evals.saturating_sub(ctx.rm.evals_base).max(1);
        let mut options = alg.options.clone();
        (alg.set_stop)(&mut options, stop);
        let opt = O::from_options(options);
        let (cur, n_rungs) = (ctx.rm.rung, rungs.len());
        let check = |p: &geode_optimize::Progress| -> Option<Trigger> {
            let policy = plan.remesh.as_ref()?;
            if guard_tripped(p.history.last()) {
                return Some(Trigger::Guard);
            }
            let k = policy.every?;
            if p.iter > 0 && p.iter.is_multiple_of(k) && cur + 1 < n_rungs {
                // Only when a later rung can take the design; else skip.
                let phys = scaled_phys(&offset, &scale, &p.x);
                return pick_rung(rungs, plan, cur, &phys).map(|_| Trigger::Every);
            }
            None
        };
        let (state, trigger) = drive(
            &opt,
            resumed.take(),
            &mut scaled,
            &bounds,
            &x0,
            ctx,
            pending.take().as_ref(),
            &check,
        )?;
        let prog = O::progress(&state).clone();
        let Some(trigger) = trigger else {
            return Ok((prog, scaled));
        };
        let phys = scaled.to_physical(&prog.x);
        let Some(next) = pick_rung(rungs, plan, ctx.rm.rung, &phys) else {
            return Err(invalid(format!(
                "remesh: the mesh-quality guard tripped after iteration {} on rung {} and no \
                 later rung accepts the design (min_volume_ratio {}); add a rung or relax \
                 `min_volume_ratio`",
                ctx.rm.iter_base + prog.iter,
                ctx.rm.rung,
                plan.min_volume_ratio
            )));
        };
        pending = Some(Pending {
            trigger,
            from_rung: ctx.rm.rung,
            to_rung: next,
            rung: rungs[next]
                .path
                .as_ref()
                .map_or_else(String::new, |q| q.display().to_string()),
            x: phys.clone(),
            f_before: prog.f,
        });
        ctx.rm.rung = next;
        ctx.rm.iter_base += prog.iter;
        ctx.rm.evals_base += prog.n_evals;
        ctx.rm.failed_base += prog.n_failed_evals;
        ctx.rm.remeshes += 1;
        // Objectives on different meshes are not comparable: the best-so-far
        // restarts on the new baseline.
        ctx.best = None;
        carry = Some(phys);
    }
}

fn scaled_phys(offset: &[f64], scale: &[f64], x: &[f64]) -> Vec<f64> {
    x.iter()
        .zip(offset)
        .zip(scale)
        .map(|((x, o), s)| o + s * x)
        .collect()
}

/// `geode optimize`: see the module docs.
///
/// # Errors
///
/// An invalid spec, a failed evaluation at the starting design, or an I/O
/// error writing the artifacts.
pub fn run(
    spec_path: &Path,
    provenance: Provenance,
    outdir: Option<&Path>,
    output: Option<&Path>,
    resume: Option<&Path>,
    jobs: usize,
) -> Result<OptimizeReport, CliError> {
    let t0 = Instant::now();
    let p = problem::load(spec_path, Some(Analysis::Driven))?;
    let plan = resolve(&p)?;
    p.print_load_warnings();
    let sens = p.sensitivity.as_ref().expect("resolved");
    // A `z0` observable on a channel with no line impedance: reject before
    // the first solve, like `geode driven`.
    crate::s_sensitivity::line_impedance_error(&p, sens, &PortSolves::new(&p))?;
    let dir = match (outdir, output.and_then(Path::parent)) {
        (Some(d), _) => d.to_path_buf(),
        (None, Some(d)) if !d.as_os_str().is_empty() => d.to_path_buf(),
        _ => PathBuf::from("geode-optimize"),
    };
    std::fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    let d = s_sensitivity::design(&p, sens)?;
    // The ladder: rung 0 is the spec itself, then the listed rungs.
    let spec_dir = spec_path.parent().unwrap_or_else(|| Path::new("."));
    let mut ladder: Vec<PathBuf> = Vec::new();
    let mut rungs = vec![Rung {
        p: p.clone(),
        d,
        path: None,
    }];
    for (i, r) in plan.remesh.iter().flat_map(|m| &m.rungs).enumerate() {
        let path = if r.is_absolute() {
            r.clone()
        } else {
            spec_dir.join(r)
        };
        let at = format!("rungs[{i}] ({})", path.display());
        let q = problem::load(&path, Some(Analysis::Driven))?;
        check_rung(&p, &q, &at)?;
        let qd = s_sensitivity::design(&q, q.sensitivity.as_ref().expect("checked"))?;
        ladder.push(std::path::absolute(&path).unwrap_or_else(|_| path.clone()));
        rungs.push(Rung {
            p: q,
            d: qd,
            path: Some(path),
        });
    }
    let mut ctx = Ctx {
        dir: dir.clone(),
        best: None,
        history: Vec::new(),
        remesh: plan.remesh.is_some(),
        rm: RemeshState::default(),
        ladder,
    };
    let resume_text = resume
        .map(|r| std::fs::read_to_string(r).map_err(|e| io_err(r, e)))
        .transpose()?;
    if let (Some(t), true) = (&resume_text, ctx.remesh) {
        ctx.rm = remesh_state_of(t)?;
        if ctx.rm.rung >= rungs.len() {
            return Err(invalid(format!(
                "remesh: the checkpoint is on rung {} but the spec has {} rungs",
                ctx.rm.rung,
                rungs.len()
            )));
        }
    }
    let (algorithm, state_progress, scaled) = if plan.cons.is_empty() {
        let (options, st) = match &resume_text {
            Some(t) => {
                // The stopping criteria are the spec's, so a resumed run can
                // be given a larger budget; the path itself is unchanged.
                let (o, mut s) = Lbfgsb::resume_json(t).map_err(opt_err)?;
                reopen(&mut s.progress.status);
                (o.options().clone(), Some(s))
            }
            None => (LbfgsbOptions::default(), None),
        };
        let alg = Alg::<Lbfgsb> {
            options,
            set_stop: |o, stop| o.stop = stop,
        };
        let (prog, sc) = run_segments(&alg, st, &mut ctx, &plan, &rungs)?;
        ("lbfgsb", prog, sc)
    } else {
        let (options, st) = match &resume_text {
            Some(t) => {
                let (o, mut s) = Mma::resume_json(t).map_err(opt_err)?;
                reopen(&mut s.progress.status);
                (o.options().clone(), Some(s))
            }
            None => (MmaOptions::default(), None),
        };
        let alg = Alg::<Mma> {
            options,
            set_stop: |o, stop| o.stop = stop,
        };
        let (prog, sc) = run_segments(&alg, st, &mut ctx, &plan, &rungs)?;
        ("mma", prog, sc)
    };
    let ev = scaled.inner();
    let best = ctx.best.clone().expect("iteration 0 is always recorded");
    let diag = ev.diag_for(&best.phys).unwrap_or_default();
    let fd = final_fd_check(ev, &best.phys, jobs);
    if !fd.passed {
        eprintln!(
            "warning: the final FD check did not pass: {}",
            fd.detail.as_deref().unwrap_or("?")
        );
    }
    let mut warnings = diag.warnings.clone();
    warnings.extend(ev.forward_warnings(&best.phys));
    let first = ctx.rm.f_initial_bits.map_or(f64::NAN, f64::from_bits);
    let best_rec = state_progress
        .history
        .iter()
        .find(|r| r.f == best.f && r.max_violation == best.violation);
    let report = OptimizeReport {
        provenance,
        kind: "optimize",
        status: "ok",
        algorithm,
        converged: state_progress.status.is_converged(),
        stop_status: status_text(&state_progress.status),
        iterations: ctx.rm.iter_base + state_progress.iter,
        evaluations: ctx.rm.evals_base + state_progress.n_evals,
        failed_evaluations: ctx.rm.failed_base + state_progress.n_failed_evals,
        remeshes: ctx.rm.remeshes,
        objective_initial: first,
        objective_best: best.f,
        max_violation_best: best.violation,
        variables: plan
            .vars
            .iter()
            .zip(&best.phys)
            .map(|(v, &x)| OptimizeVariableResult {
                parameter: v.name.clone(),
                initial: v.initial,
                value: x,
                lower: v.lower,
                upper: v.upper,
            })
            .collect(),
        term_values: diag.term_values.clone(),
        constraint_values: best_rec.map(|r| r.constraints.clone()).unwrap_or_default(),
        passivity_sigma_max: diag.sigma_max,
        min_volume_ratio: diag.min_volume_ratio,
        history_file: file_ref(&dir, HISTORY_FILE)?,
        checkpoint_file: file_ref(&dir, CHECKPOINT_FILE)?,
        best_spec_file: file_ref(&dir, BEST_SPEC_FILE)?,
        best_mesh_file: file_ref(&dir, BEST_MESH_FILE)?,
        final_fd_check: fd,
        warnings,
        wall_time_s: t0.elapsed().as_secs_f64(),
    };
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(re: f64, im: f64) -> c64 {
        c64::new(re, im)
    }

    #[test]
    fn passivity_tripwire_fires_on_a_non_passive_s() {
        // A matched passive 2-port: σ_max = 1 (a lossless unitary-ish S).
        let passive = [c(0.0, 0.0), c(0.0, 1.0), c(0.0, 1.0), c(0.0, 0.0)];
        assert!((sigma_max(&passive, 2) - 1.0).abs() < 1e-12);
        // Deliberately non-passive: gain 1.5 on S21.
        let gain = [c(0.1, 0.0), c(0.0, 0.2), c(1.5, 0.0), c(0.0, 0.0)];
        let s = sigma_max(&gain, 2);
        assert!(s > 1.0 + DEFAULT_PASSIVITY_TOL, "σ_max = {s}");
        // 1×1 is |S|.
        assert!((sigma_max(&[c(0.3, 0.4)], 1) - 0.5).abs() < 1e-12);
    }

    fn term(aggregate: OptimizeAggregate, mode: TargetMode) -> Term {
        Term {
            obs: 0,
            pos: vec![0, 1, 2],
            aggregate,
            mode,
            target: 0.5,
            weight: 1.0,
            rho: 30.0,
        }
    }

    /// The chained gradient of every aggregate / mode matches a central FD
    /// of the value along a made-up linear observable model `v = a + B x`.
    #[test]
    fn term_gradients_match_finite_differences() {
        let a = [0.2, 0.7, 0.9];
        let b = [[1.0, -2.0], [0.5, 1.5], [-1.0, 0.3]];
        let at = |x: [f64; 2]| -> (Vec<f64>, Vec<Vec<f64>>) {
            let v = (0..3)
                .map(|i| a[i] + b[i][0] * x[0] + b[i][1] * x[1])
                .collect();
            (v, b.iter().map(|r| r.to_vec()).collect())
        };
        let x = [0.13, -0.21];
        for agg in [
            OptimizeAggregate::SumSq,
            OptimizeAggregate::Mean,
            OptimizeAggregate::Max,
        ] {
            for mode in [TargetMode::Equal, TargetMode::Below, TargetMode::Above] {
                let t = term(agg, mode);
                let (v, g) = at(x);
                let (_, grad) = term_value(&t, &v, &g);
                for k in 0..2 {
                    let h = 1e-6;
                    let (mut xp, mut xm) = (x, x);
                    xp[k] += h;
                    xm[k] -= h;
                    let fp = term_value(&t, &at(xp).0, &g).0;
                    let fm = term_value(&t, &at(xm).0, &g).0;
                    let fd = (fp - fm) / (2.0 * h);
                    assert!(
                        (fd - grad[k]).abs() < 1e-6 * (1.0 + fd.abs()),
                        "{agg:?} {mode:?} d/dx{k}: {} vs FD {fd}",
                        grad[k]
                    );
                }
            }
        }
    }

    #[test]
    fn constraint_is_a_conservative_smooth_max() {
        let con = Con {
            obs: 0,
            pos: vec![0, 1],
            mode: TargetMode::Below,
            bound: 1.0,
            rho: 40.0,
        };
        let grads = vec![vec![1.0], vec![2.0]];
        let (g, grad) = constraint_value(&con, &[0.9, 1.2], &grads);
        // KS ≥ max(v − bound) = 0.2, and within ln(2)/ρ of it.
        assert!(
            g >= 0.2 - 1e-12 && g <= 0.2 + 2f64.ln() / 40.0 + 1e-12,
            "g = {g}"
        );
        assert!(
            grad[0] > 1.0 && grad[0] <= 2.0,
            "softmax mixes the gradients"
        );
        // `above` flips the sign of both the value and the gradient.
        let up = Con {
            mode: TargetMode::Above,
            ..con
        };
        let (g2, grad2) = constraint_value(&up, &[1.5, 1.2], &grads);
        assert!(g2 < 0.0 && grad2[0] < 0.0, "g = {g2}");
    }
}
