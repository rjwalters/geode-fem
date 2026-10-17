//! Shared driver types: bounds, stopping criteria, run status, the
//! per-iteration history, the [`Optimizer`] trait and JSON checkpoints.

use crate::exact;
use crate::objective::{EvalError, Evaluation, Objective};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Box bounds `lower ≤ x ≤ upper`. An infinite entry means "unbounded on
/// that side". MMA needs finite bounds on every variable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    /// Lower bounds (`-inf` allowed).
    #[serde(with = "exact::vec")]
    pub lower: Vec<f64>,
    /// Upper bounds (`+inf` allowed).
    #[serde(with = "exact::vec")]
    pub upper: Vec<f64>,
}

impl Bounds {
    /// Bounds from explicit vectors.
    ///
    /// # Errors
    ///
    /// The lengths differ, an entry is `NaN`, or `lower_i > upper_i`.
    pub fn new(lower: Vec<f64>, upper: Vec<f64>) -> Result<Self, OptimizeError> {
        let b = Self { lower, upper };
        b.validate(b.lower.len())?;
        Ok(b)
    }

    /// No bounds on any of the `n` variables.
    #[must_use]
    pub fn unbounded(n: usize) -> Self {
        Self {
            lower: vec![f64::NEG_INFINITY; n],
            upper: vec![f64::INFINITY; n],
        }
    }

    /// The same interval `[lo, hi]` on all `n` variables.
    #[must_use]
    pub fn uniform(n: usize, lo: f64, hi: f64) -> Self {
        Self {
            lower: vec![lo; n],
            upper: vec![hi; n],
        }
    }

    /// The number of variables.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lower.len()
    }

    /// Whether there are no variables.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lower.is_empty()
    }

    /// Checks the bounds against the dimension `n`.
    ///
    /// # Errors
    ///
    /// The lengths are not `n`, an entry is `NaN`, or `lower_i > upper_i`.
    pub fn validate(&self, n: usize) -> Result<(), OptimizeError> {
        if self.lower.len() != n || self.upper.len() != n {
            return Err(OptimizeError::InvalidInput(format!(
                "bounds have {} / {} entries, the design has {n}",
                self.lower.len(),
                self.upper.len()
            )));
        }
        for i in 0..n {
            let (l, u) = (self.lower[i], self.upper[i]);
            if l.is_nan() || u.is_nan() || l > u {
                return Err(OptimizeError::InvalidInput(format!(
                    "bound {i}: lower {l} must be ≤ upper {u} (and not NaN)"
                )));
            }
        }
        Ok(())
    }

    /// The projection of `x` onto the box.
    #[must_use]
    pub fn project(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .enumerate()
            .map(|(i, &v)| v.clamp(self.lower[i], self.upper[i]))
            .collect()
    }

    /// The infinity norm of the projected gradient,
    /// `‖P(x − g) − x‖_∞`. It is zero exactly at a first-order stationary
    /// point of the box-constrained problem.
    #[must_use]
    pub fn projected_gradient_norm(&self, x: &[f64], g: &[f64]) -> f64 {
        x.iter()
            .zip(g)
            .enumerate()
            .fold(0.0_f64, |m, (i, (&xi, &gi))| {
                let p = (xi - gi).clamp(self.lower[i], self.upper[i]) - xi;
                m.max(p.abs())
            })
    }
}

/// When to stop. Every criterion is checked after each iteration (and the
/// gradient criterion also at the starting point).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StopCriteria {
    /// Converged when the first-order residual is at most this:
    /// L-BFGS-B uses the projected-gradient inf-norm; MMA uses the
    /// inf-norm of the projected Lagrangian gradient together with
    /// complementarity (see `mma`).
    #[serde(with = "exact::f64s")]
    pub pg_tol: f64,
    /// Converged when `|f_prev − f| / max(|f_prev|, |f|, 1) ≤ f_rel_tol`.
    /// For MMA this applies only at a feasible iterate. Set `0` to disable.
    #[serde(with = "exact::f64s")]
    pub f_rel_tol: f64,
    /// Converged when the step `‖x_k − x_{k−1}‖_∞`, measured relative to
    /// `max(1, ‖x‖_∞)`, is at most this (MMA only, at a feasible iterate).
    /// Set `0` to disable.
    #[serde(with = "exact::f64s")]
    pub x_tol: f64,
    /// MMA: an iterate is feasible when `max_i g_i ≤ feas_tol`.
    #[serde(with = "exact::f64s")]
    pub feas_tol: f64,
    /// Stop after this many iterations (status [`Status::MaxIterations`]).
    pub max_iter: usize,
    /// Stop once this many evaluations have been spent, counting failed
    /// ones (status [`Status::MaxEvaluations`]).
    pub max_evals: usize,
}

impl Default for StopCriteria {
    fn default() -> Self {
        Self {
            pg_tol: 1e-6,
            f_rel_tol: 1e-12,
            x_tol: 0.0,
            feas_tol: 1e-6,
            max_iter: 1000,
            max_evals: 5000,
        }
    }
}

/// Why a run converged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Convergence {
    /// The first-order residual fell below [`StopCriteria::pg_tol`].
    FirstOrder,
    /// The relative objective change fell below [`StopCriteria::f_rel_tol`].
    RelativeFChange,
    /// The step fell below [`StopCriteria::x_tol`].
    StepSize,
}

/// The state of a run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Status {
    /// Not finished: [`Optimizer::step`] / [`Optimizer::run`] may continue.
    /// A run paused by an observer (for a checkpoint) stays `Running`.
    Running,
    /// A convergence criterion was met.
    Converged {
        /// Which one.
        reason: Convergence,
    },
    /// [`StopCriteria::max_iter`] was reached.
    MaxIterations,
    /// [`StopCriteria::max_evals`] was reached.
    MaxEvaluations,
    /// No further progress is possible (e.g. the line search found no
    /// acceptable step even from the steepest-descent direction, or every
    /// backed-off trial point failed to evaluate).
    Stalled {
        /// What happened.
        message: String,
    },
}

impl Status {
    /// Whether the run has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        !matches!(self, Status::Running)
    }

    /// Whether the run ended by meeting a convergence criterion.
    #[must_use]
    pub fn is_converged(&self) -> bool {
        matches!(self, Status::Converged { .. })
    }
}

/// One history entry, written after each iteration (and once for the
/// starting point, as iteration 0).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IterationRecord {
    /// The iteration number (0 = the starting point).
    pub iter: usize,
    /// The iterate.
    #[serde(with = "exact::vec")]
    pub x: Vec<f64>,
    /// The objective value.
    #[serde(with = "exact::f64s")]
    pub f: f64,
    /// The first-order residual used by [`StopCriteria::pg_tol`].
    #[serde(with = "exact::f64s")]
    pub pg_norm: f64,
    /// The constraint values (empty if none).
    #[serde(with = "exact::vec")]
    pub constraints: Vec<f64>,
    /// `max(0, max_i g_i)`.
    #[serde(with = "exact::f64s")]
    pub max_violation: f64,
    /// L-BFGS-B: the accepted line-search step `α` along the search
    /// direction. MMA: the fraction of the subproblem step taken (1 unless
    /// backed off after failures).
    #[serde(with = "exact::f64s")]
    pub step_length: f64,
    /// `‖x_k − x_{k−1}‖_∞`.
    #[serde(with = "exact::f64s")]
    pub step_norm: f64,
    /// Evaluations spent so far (cumulative, including failed ones).
    pub n_evals: usize,
    /// Failed evaluations so far (cumulative).
    pub n_failed_evals: usize,
    /// Failed evaluations during this iteration.
    pub failed_this_iter: usize,
    /// Notable events, e.g. "L-BFGS memory reset" or the message of a
    /// failed evaluation the run backed off from.
    pub notes: Vec<String>,
}

/// The algorithm-independent part of an optimizer state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    /// The current iterate.
    #[serde(with = "exact::vec")]
    pub x: Vec<f64>,
    /// `f(x)`.
    #[serde(with = "exact::f64s")]
    pub f: f64,
    /// `∇f(x)`.
    #[serde(with = "exact::vec")]
    pub grad: Vec<f64>,
    /// `g_i(x)`.
    #[serde(with = "exact::vec")]
    pub constraints: Vec<f64>,
    /// `∇g_i(x)`.
    #[serde(with = "exact::vec2")]
    pub jacobian: Vec<Vec<f64>>,
    /// The box bounds.
    pub bounds: Bounds,
    /// Completed iterations.
    pub iter: usize,
    /// Evaluations spent, including failed ones.
    pub n_evals: usize,
    /// Failed evaluations.
    pub n_failed_evals: usize,
    /// The run status.
    pub status: Status,
    /// One record per iteration, starting with iteration 0.
    pub history: Vec<IterationRecord>,
}

impl Progress {
    pub(crate) fn new(x: Vec<f64>, e: Evaluation, bounds: Bounds) -> Self {
        Self {
            x,
            f: e.f,
            grad: e.grad,
            constraints: e.constraints,
            jacobian: e.jacobian,
            bounds,
            iter: 0,
            n_evals: 1,
            n_failed_evals: 0,
            status: Status::Running,
            history: Vec::new(),
        }
    }

    pub(crate) fn set_eval(&mut self, x: Vec<f64>, e: Evaluation) {
        self.x = x;
        self.f = e.f;
        self.grad = e.grad;
        self.constraints = e.constraints;
        self.jacobian = e.jacobian;
    }

    /// The current evaluation as an [`Evaluation`].
    #[must_use]
    pub fn evaluation(&self) -> Evaluation {
        Evaluation::constrained(
            self.f,
            self.grad.clone(),
            self.constraints.clone(),
            self.jacobian.clone(),
        )
    }

    /// `max(0, max_i g_i)` at the current iterate.
    #[must_use]
    pub fn max_violation(&self) -> f64 {
        self.constraints.iter().fold(0.0_f64, |m, &g| m.max(g))
    }

    pub(crate) fn record(
        &mut self,
        pg_norm: f64,
        step_length: f64,
        step_norm: f64,
        failed_this_iter: usize,
        notes: Vec<String>,
    ) {
        let max_violation = self.max_violation();
        self.history.push(IterationRecord {
            iter: self.iter,
            x: self.x.clone(),
            f: self.f,
            pg_norm,
            constraints: self.constraints.clone(),
            max_violation,
            step_length,
            step_norm,
            n_evals: self.n_evals,
            n_failed_evals: self.n_failed_evals,
            failed_this_iter,
            notes,
        });
    }

    /// Applies the iteration-count, evaluation-budget and convergence tests
    /// after an iteration that moved from `f_prev` to `self.f`.
    pub(crate) fn check_stop(
        &mut self,
        stop: &StopCriteria,
        pg_norm: f64,
        f_prev: Option<f64>,
        step_rel: Option<f64>,
        feasible: bool,
    ) {
        if feasible && pg_norm <= stop.pg_tol {
            self.status = Status::Converged {
                reason: Convergence::FirstOrder,
            };
            return;
        }
        if let Some(fp) = f_prev {
            let denom = fp.abs().max(self.f.abs()).max(1.0);
            if feasible && stop.f_rel_tol > 0.0 && (fp - self.f).abs() / denom <= stop.f_rel_tol {
                self.status = Status::Converged {
                    reason: Convergence::RelativeFChange,
                };
                return;
            }
        }
        if let Some(s) = step_rel
            && feasible
            && stop.x_tol > 0.0
            && s <= stop.x_tol
        {
            self.status = Status::Converged {
                reason: Convergence::StepSize,
            };
            return;
        }
        if self.iter >= stop.max_iter {
            self.status = Status::MaxIterations;
        } else if self.n_evals >= stop.max_evals {
            self.status = Status::MaxEvaluations;
        }
    }
}

/// What an observer passed to [`Optimizer::run`] wants after an iteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// Keep iterating.
    Continue,
    /// Pause: return now with the status still [`Status::Running`] (for
    /// example after writing a checkpoint, or on a wall-clock budget).
    Stop,
}

/// Errors that end a run. Failed evaluations at trial points are *not*
/// errors (the optimizers back off and record them).
#[derive(Debug, thiserror::Error)]
pub enum OptimizeError {
    /// Bad dimensions, bounds, options or starting point.
    #[error("invalid optimizer input: {0}")]
    InvalidInput(String),
    /// The evaluation at the starting point failed, so there is nothing to
    /// back off to.
    #[error("the objective failed at the starting point: {0}")]
    InitialEvaluation(EvalError),
    /// A checkpoint could not be written or read.
    #[error("checkpoint: {0}")]
    Checkpoint(String),
}

/// A gradient-based optimizer. One [`Optimizer::step`] is one outer
/// iteration, so a driver can checkpoint, log or re-mesh between
/// iterations. The whole run state lives in [`Optimizer::State`] (no hidden
/// state in the optimizer itself), so a state restored from a checkpoint
/// continues bit-identically.
pub trait Optimizer: Sized {
    /// The serializable options.
    type Options: Clone + Serialize + DeserializeOwned;
    /// The serializable run state.
    type State: Clone + Serialize + DeserializeOwned;
    /// The algorithm tag stored in checkpoints (e.g. `"lbfgsb"`).
    const ALGORITHM: &'static str;

    /// Builds the optimizer from its options.
    fn from_options(options: Self::Options) -> Self;

    /// The options.
    fn options(&self) -> &Self::Options;

    /// Evaluates at `x0` (projected onto `bounds`) and returns the initial
    /// state.
    ///
    /// # Errors
    ///
    /// Invalid input, or the evaluation at `x0` fails.
    fn start(
        &self,
        obj: &mut dyn Objective,
        x0: &[f64],
        bounds: &Bounds,
    ) -> Result<Self::State, OptimizeError>;

    /// Performs one iteration if the status is [`Status::Running`].
    ///
    /// # Errors
    ///
    /// The state does not match the objective's dimensions.
    fn step(&self, obj: &mut dyn Objective, state: &mut Self::State) -> Result<(), OptimizeError>;

    /// The algorithm-independent part of `state`.
    fn progress(state: &Self::State) -> &Progress;

    /// Iterates until the run finishes or `observer` returns
    /// [`Control::Stop`]. The observer runs after every iteration.
    ///
    /// # Errors
    ///
    /// As [`Optimizer::step`].
    fn run(
        &self,
        obj: &mut dyn Objective,
        state: &mut Self::State,
        observer: &mut dyn FnMut(&Self::State) -> Control,
    ) -> Result<Status, OptimizeError> {
        while !Self::progress(state).status.is_finished() {
            self.step(obj, state)?;
            if observer(state) == Control::Stop {
                break;
            }
        }
        Ok(Self::progress(state).status.clone())
    }

    /// [`Optimizer::start`] then [`Optimizer::run`] to completion.
    ///
    /// # Errors
    ///
    /// As [`Optimizer::start`] and [`Optimizer::step`].
    fn minimize(
        &self,
        obj: &mut dyn Objective,
        x0: &[f64],
        bounds: &Bounds,
    ) -> Result<Self::State, OptimizeError> {
        let mut state = self.start(obj, x0, bounds)?;
        self.run(obj, &mut state, &mut |_| Control::Continue)?;
        Ok(state)
    }

    /// Serializes the options and state into a [`Checkpoint`] JSON string.
    ///
    /// # Errors
    ///
    /// Serialization fails.
    fn checkpoint_json(&self, state: &Self::State) -> Result<String, OptimizeError> {
        let cp = Checkpoint {
            format: CHECKPOINT_FORMAT.to_string(),
            algorithm: Self::ALGORITHM.to_string(),
            options: self.options().clone(),
            state: state.clone(),
        };
        serde_json::to_string_pretty(&cp).map_err(|e| OptimizeError::Checkpoint(e.to_string()))
    }

    /// Restores an optimizer and its state from [`Optimizer::checkpoint_json`]
    /// output. Continuing with [`Optimizer::run`] reproduces the
    /// uninterrupted run bit for bit (given a deterministic objective).
    ///
    /// # Errors
    ///
    /// The JSON is malformed, or it is a different format or algorithm.
    fn resume_json(json: &str) -> Result<(Self, Self::State), OptimizeError> {
        let cp: Checkpoint<Self::Options, Self::State> =
            serde_json::from_str(json).map_err(|e| OptimizeError::Checkpoint(e.to_string()))?;
        if cp.format != CHECKPOINT_FORMAT {
            return Err(OptimizeError::Checkpoint(format!(
                "format {:?}, expected {CHECKPOINT_FORMAT:?}",
                cp.format
            )));
        }
        if cp.algorithm != Self::ALGORITHM {
            return Err(OptimizeError::Checkpoint(format!(
                "checkpoint is for {:?}, not {:?}",
                cp.algorithm,
                Self::ALGORITHM
            )));
        }
        Ok((Self::from_options(cp.options), cp.state))
    }
}

/// The checkpoint format tag.
pub const CHECKPOINT_FORMAT: &str = "geode-optimize/1";

/// A serialized run: format tag, algorithm, options and full state.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint<O, S> {
    /// Always [`CHECKPOINT_FORMAT`].
    pub format: String,
    /// [`Optimizer::ALGORITHM`].
    pub algorithm: String,
    /// The optimizer options.
    pub options: O,
    /// The run state.
    pub state: S,
}

/// Evaluates and validates, counting the evaluation (and a failure).
pub(crate) fn counted_eval(
    obj: &mut dyn Objective,
    x: &[f64],
    n_evals: &mut usize,
    n_failed: &mut usize,
) -> Result<Evaluation, EvalError> {
    *n_evals += 1;
    let r = obj
        .evaluate(x)
        .and_then(|e| e.validate(x.len(), obj.n_constraints()).map(|()| e));
    if r.is_err() {
        *n_failed += 1;
    }
    r
}

/// The common `start` checks and the evaluation at `x0`.
pub(crate) fn start_progress(
    obj: &mut dyn Objective,
    x0: &[f64],
    bounds: &Bounds,
) -> Result<Progress, OptimizeError> {
    let n = obj.dim();
    if x0.len() != n {
        return Err(OptimizeError::InvalidInput(format!(
            "x0 has {} entries, the objective has {n}",
            x0.len()
        )));
    }
    if x0.iter().any(|v| !v.is_finite()) {
        return Err(OptimizeError::InvalidInput("x0 is not finite".into()));
    }
    bounds.validate(n)?;
    let x = bounds.project(x0);
    let e = obj
        .evaluate(&x)
        .and_then(|e| e.validate(n, obj.n_constraints()).map(|()| e))
        .map_err(OptimizeError::InitialEvaluation)?;
    Ok(Progress::new(x, e, bounds.clone()))
}

pub(crate) fn check_dims(obj: &dyn Objective, p: &Progress) -> Result<(), OptimizeError> {
    if obj.dim() != p.x.len() || obj.n_constraints() != p.constraints.len() {
        return Err(OptimizeError::InvalidInput(format!(
            "state is for n = {}, m = {}; the objective has n = {}, m = {}",
            p.x.len(),
            p.constraints.len(),
            obj.dim(),
            obj.n_constraints()
        )));
    }
    Ok(())
}
