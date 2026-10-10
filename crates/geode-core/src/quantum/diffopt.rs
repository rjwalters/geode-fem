//! Gradient-based transmon-parameter optimization (Epic #476 / #569,
//! issue #584) — the differentiable-design centerpiece.
//!
//! This is the small, method-only kernel behind the paper's convergence
//! figure: a scale-free **damped-Newton** iteration that drives a scalar
//! geometry parameter `θ` to hit a **target charging energy** `E_C/h`, using
//! the analytic `∂(E_C/h)/∂θ` produced by the electrostatic-energy adjoint
//! ([`crate::shape::capacitance_shape_gradient`] chained through
//! [`crate::quantum::transmon::d_e_c_hz_d_c_sigma`], issue #583).
//!
//! # Why damped Newton (not a hand-tuned learning rate)
//!
//! The target is a 1-D root-find, `E_C(θ) − E_C_target = 0`. The Newton
//! update
//!
//! ```text
//!   θ ← θ − α · (E_C(θ) − E_C_target) / (∂E_C/∂θ)
//! ```
//!
//! is **scale-free** — there is no learning rate to tune, because the
//! analytic derivative sets the step. With the full step `α = 1` a *linear*
//! response (the clean parallel-plate fixture where `E_C(θ)` is exactly
//! affine) converges in a **single step**; a damping `α ∈ (0, 1)` produces a
//! multi-point geometric trajectory (the convergence curve) while remaining a
//! textbook method, not a fabricated schedule.
//!
//! # The capability argument (honest framing)
//!
//! Each iteration evaluates `(C, E_C, ∂E_C/∂θ)` with **one forward + one
//! adjoint solve** (a single LU factorization — see
//! [`crate::shape::CapacitanceShapeGradient`]). The derivative-free
//! incumbent workflow (parameter sweep / finite differences) instead spends
//! `N_params` **extra** forward solves *per step* just to estimate the
//! gradient. This module's claim is precisely that step-count / capability
//! contrast — **gradient-based vs derivative-free** — NOT a wall-clock
//! speedup versus any specific tool, which we have not measured here.
//!
//! The evaluator is injected as a closure so this kernel is solver-agnostic
//! and unit-testable against a closed-form `E_C(θ)`; the `transmon_diffopt`
//! example and the `transmon_diffopt` integration test wire it to a real
//! per-iteration FEM capacitance solve on a parallel-plate fixture.

/// One recorded iteration of the gradient-based `E_C`-to-target optimization
/// — the row behind the convergence figure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiffOptStep {
    /// Iteration index (0 = the starting geometry, before any step).
    pub iter: usize,
    /// Geometry parameter `θ` at this iteration.
    pub theta: f64,
    /// Self-capacitance `C_Σ` (F) from the fresh forward solve at `θ`.
    pub c_self_farad: f64,
    /// Charging energy `E_C/h` (Hz) at `θ`, `E_C = e²/(2 C_Σ)`.
    pub e_c_hz: f64,
    /// Signed residual `E_C(θ) − E_C_target` (Hz).
    pub residual_hz: f64,
    /// Least-squares objective `(E_C(θ) − E_C_target)²` (Hz²).
    pub objective_hz2: f64,
    /// Analytic design gradient `∂(E_C/h)/∂θ` (Hz per unit θ) at `θ`.
    pub de_c_hz_dtheta: f64,
}

/// Result of a [`optimize_e_c_to_target`] run: the full per-iteration
/// trajectory plus the converged summary.
#[derive(Debug, Clone)]
pub struct DiffOptResult {
    /// Every recorded iteration, oldest first (index 0 is the start).
    pub trajectory: Vec<DiffOptStep>,
    /// True iff the final `|E_C − E_C_target|` fell within `tol_hz`.
    pub converged: bool,
    /// True iff the iteration stopped because the box-projected update could
    /// no longer move `θ` — it is pinned on a [`optimize_e_c_to_target_bounded`]
    /// bound with the target still out of tolerance. The honest-partial
    /// outcome of a constrained run (e.g. a mesh-distortion limit tighter
    /// than the design target, issue #589). Always `false` for the
    /// unbounded variants.
    pub stalled_at_bound: bool,
    /// The converged parameter `θ` (the last trajectory point's `θ`).
    pub theta_final: f64,
    /// The converged `E_C/h` (Hz) — the last forward-solved value.
    pub e_c_final_hz: f64,
    /// Number of Newton steps actually taken (trajectory length − 1).
    pub n_steps: usize,
}

/// Damped-Newton drive of a scalar geometry parameter `θ` to a target
/// charging energy `E_C_target` (Hz), using the analytic `∂E_C/∂θ`.
///
/// `eval(θ)` must return `(C_Σ, E_C/h, ∂(E_C/h)/∂θ)` from a **fresh forward
/// (and adjoint) solve** at `θ` — this is what makes the trajectory a real
/// pipeline result rather than a linearized extrapolation. The update is
///
/// ```text
///   θ ← θ − α · (E_C(θ) − E_C_target) / (∂E_C/∂θ)
/// ```
///
/// with damping `alpha ∈ (0, 1]` (`1.0` = full Newton). Iteration stops when
/// `|E_C(θ) − E_C_target| ≤ tol_hz` or after `max_steps` steps; the starting
/// point and every post-step evaluation are recorded in
/// [`DiffOptResult::trajectory`].
///
/// # Panics
///
/// Panics if `alpha` is not in `(0, 1]`, if `tol_hz < 0`, or if `eval`
/// returns a non-finite / zero gradient (a degenerate parameterization the
/// Newton step cannot use).
pub fn optimize_e_c_to_target<F>(
    e_c_target_hz: f64,
    theta0: f64,
    alpha: f64,
    tol_hz: f64,
    max_steps: usize,
    eval: F,
) -> DiffOptResult
where
    F: FnMut(f64) -> (f64, f64, f64),
{
    optimize_e_c_to_target_clamped(
        e_c_target_hz,
        theta0,
        alpha,
        tol_hz,
        max_steps,
        f64::INFINITY,
        eval,
    )
}

/// [`optimize_e_c_to_target`] with a **trust-region-style step clamp**: each
/// damped-Newton update is limited to `|Δθ| ≤ max_abs_dtheta`.
///
/// On a real (non-affine) geometry the first Newton steps extrapolate a
/// local linearization far outside its validity — and, for **mesh-morphing**
/// parameterizations, a too-large step can outright invert tets of the
/// fixed-topology mesh (issue #589's island-pad scale). Clamping keeps every
/// visited `θ` within a bounded, mesh-safe move per step while preserving
/// the scale-free Newton behavior near the solution (once the unclamped step
/// is smaller than the clamp, the iteration is plain damped Newton).
///
/// # Panics
///
/// Same as [`optimize_e_c_to_target`], plus `max_abs_dtheta` must be
/// positive (it may be `f64::INFINITY` for no clamping).
#[allow(clippy::too_many_arguments)]
pub fn optimize_e_c_to_target_clamped<F>(
    e_c_target_hz: f64,
    theta0: f64,
    alpha: f64,
    tol_hz: f64,
    max_steps: usize,
    max_abs_dtheta: f64,
    eval: F,
) -> DiffOptResult
where
    F: FnMut(f64) -> (f64, f64, f64),
{
    optimize_e_c_to_target_bounded(
        e_c_target_hz,
        theta0,
        alpha,
        tol_hz,
        max_steps,
        max_abs_dtheta,
        (f64::NEG_INFINITY, f64::INFINITY),
        eval,
    )
}

/// [`optimize_e_c_to_target_clamped`] with an additional **box constraint**
/// `θ ∈ [theta_bounds.0, theta_bounds.1]`: every clamped Newton update is
/// projected back into the box before evaluation.
///
/// This is the constrained-design driver for a parameter with a hard
/// validity limit — issue #589's fixed-topology island-pad scale, where the
/// mesh inverts beyond a bisected distortion boundary, is the motivating
/// case. If the projected update can no longer move `θ` (it is pinned on a
/// bound with the residual still above `tol_hz`), the run stops and reports
/// [`DiffOptResult::stalled_at_bound`] — the honest-partial outcome
/// ("converged to the constraint boundary"), never a fabricated
/// convergence.
///
/// # Panics
///
/// Same as [`optimize_e_c_to_target_clamped`], plus the box must be
/// non-empty and contain `theta0`.
#[allow(clippy::too_many_arguments)]
pub fn optimize_e_c_to_target_bounded<F>(
    e_c_target_hz: f64,
    theta0: f64,
    alpha: f64,
    tol_hz: f64,
    max_steps: usize,
    max_abs_dtheta: f64,
    theta_bounds: (f64, f64),
    mut eval: F,
) -> DiffOptResult
where
    F: FnMut(f64) -> (f64, f64, f64),
{
    assert!(
        alpha > 0.0 && alpha <= 1.0,
        "damping alpha must be in (0, 1], got {alpha}"
    );
    assert!(tol_hz >= 0.0, "tol_hz must be non-negative, got {tol_hz}");
    assert!(
        max_abs_dtheta > 0.0,
        "max_abs_dtheta must be positive, got {max_abs_dtheta}"
    );
    let (theta_lo, theta_hi) = theta_bounds;
    assert!(
        theta_lo <= theta0 && theta0 <= theta_hi,
        "theta bounds [{theta_lo}, {theta_hi}] must contain theta0 = {theta0}"
    );

    let mut trajectory = Vec::with_capacity(max_steps + 1);
    let mut theta = theta0;

    // Record a fresh-solve evaluation at the current θ.
    let record = |iter: usize, theta: f64, eval: &mut F| -> DiffOptStep {
        let (c_self, e_c, de_c_dtheta) = eval(theta);
        assert!(
            de_c_dtheta.is_finite() && de_c_dtheta != 0.0,
            "degenerate gradient ∂E_C/∂θ = {de_c_dtheta} at θ = {theta}"
        );
        let residual = e_c - e_c_target_hz;
        DiffOptStep {
            iter,
            theta,
            c_self_farad: c_self,
            e_c_hz: e_c,
            residual_hz: residual,
            objective_hz2: residual * residual,
            de_c_hz_dtheta: de_c_dtheta,
        }
    };

    // Iteration 0: the starting geometry.
    let mut step = record(0, theta, &mut eval);
    trajectory.push(step);

    let mut converged = step.residual_hz.abs() <= tol_hz;
    let mut stalled_at_bound = false;
    let mut iter = 0;
    while !converged && iter < max_steps {
        // Damped-Newton update using the analytic derivative, clamped to
        // the per-step trust region and projected into the θ box.
        let dtheta = (-alpha * step.residual_hz / step.de_c_hz_dtheta)
            .clamp(-max_abs_dtheta, max_abs_dtheta);
        let theta_new = (theta + dtheta).clamp(theta_lo, theta_hi);
        if theta_new == theta {
            // Pinned on a bound with the target still out of tolerance:
            // stop honestly rather than re-evaluating the same design.
            stalled_at_bound = true;
            break;
        }
        theta = theta_new;
        iter += 1;
        step = record(iter, theta, &mut eval);
        trajectory.push(step);
        converged = step.residual_hz.abs() <= tol_hz;
    }

    DiffOptResult {
        converged,
        stalled_at_bound,
        theta_final: step.theta,
        e_c_final_hz: step.e_c_hz,
        n_steps: trajectory.len() - 1,
        trajectory,
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Multi-parameter bounded Gauss-Newton with linearized inequality
// constraints (Epic #569 / umbrella #1034, issue #1036, Phase B).
// ─────────────────────────────────────────────────────────────────────────
//
// The scalar drivers above take one θ. The transmon design has P ≥ 3
// parameters and M = 1 target (E_C), and Phase C (#1037) adds a second
// target (a hold on β = C_g/C_Σ) and a fourth parameter. The step is
//
//   min ½‖S⁻¹Δθ‖²   s.t.   J Δθ = −t·r,   box ∩ trust region,   a_k·Δθ ≥ b_k,
//
// in the fixed scaling S, with t ∈ [0, 1] maximized first: t = 1 is the full
// minimum-norm Gauss-Newton step; t < 1 is the largest fraction of the
// linearized residual the constraints allow. The rows a_k·Δθ ≥ b_k are
// linearized inequality constraints the problem reports when a trial step
// fails its own (nonlinear) check; on the transmon they are tets whose volume
// ratio against the original mesh would fall below the floor.
//
// The QP is tiny (P parameters), so it is solved exactly by enumerating the
// active sets of at most P − M constraints: for each candidate set the
// equality-constrained minimum-norm point is a small dense solve, and the
// feasible candidate of smallest norm is the optimum (the KKT active set of a
// strictly convex QP can always be chosen linearly independent).

/// Values and Jacobian of the targets at one geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct MultiEval {
    /// One value per target (on the transmon, `E_C/h` in Hz).
    pub values: Vec<f64>,
    /// `jacobian[j][i] = ∂values[j]/∂Δθ_i` at this geometry.
    pub jacobian: Vec<Vec<f64>>,
}

/// A linearized inequality constraint on the step: `grad · Δθ ≥ rhs`.
#[derive(Debug, Clone, PartialEq)]
pub struct StepConstraint {
    /// A problem-defined identifier (on the transmon, the tet index).
    pub id: usize,
    /// The constraint row, `∂q/∂Δθ` at the current iterate.
    pub grad: Vec<f64>,
    /// The right-hand side. The optimizer uses `min(rhs, 0)`, so the zero step
    /// is always feasible and a constraint can only stop further decrease.
    pub rhs: f64,
}

/// The problem's geometry-only verdict on a trial step (no solve).
#[derive(Debug, Clone, PartialEq)]
pub struct StepCheck {
    /// True iff the trial step satisfies the problem's nonlinear constraints.
    pub feasible: bool,
    /// The worst constraint quantity against the original geometry (on the
    /// transmon, the min tet volume ratio against `X⁰`).
    pub min_quality_base: f64,
    /// The worst constraint quantity against the current iterate (min tet
    /// volume ratio against `X_k`).
    pub min_quality_step: f64,
    /// Violated constraints, most violated first, linearized about the
    /// current iterate. Empty when `feasible`.
    pub violated: Vec<StepConstraint>,
}

/// A design problem driven by [`optimize_multiparam_bounded`].
///
/// The problem owns the geometry. A re-morphing problem moves its own
/// coordinates on [`MultiParamProblem::accept_step`] and rebuilds its
/// parameter fields there, so `Δθ` is always a step from the current iterate
/// and the accumulated `θ` is a trace, not a coordinate.
pub trait MultiParamProblem {
    /// Number of parameters `P`.
    fn n_params(&self) -> usize;
    /// Values and Jacobian at the current accepted iterate.
    fn current(&self) -> MultiEval;
    /// Geometry-only check of `current + Δθ` (cheap: no solve).
    fn check_step(&mut self, dtheta: &[f64]) -> StepCheck;
    /// A fresh solve at `current + Δθ`: values there, and the Jacobian with
    /// respect to a step taken from there. The problem may cache the trial for
    /// [`MultiParamProblem::accept_step`].
    fn evaluate_step(&mut self, dtheta: &[f64]) -> MultiEval;
    /// Make `current + Δθ` (the last evaluated step) the new iterate.
    fn accept_step(&mut self, dtheta: &[f64]);
    /// An independent fresh evaluation of the current iterate's values, used
    /// to confirm every accepted step. `None` if the problem has none.
    fn confirm(&mut self) -> Option<Vec<f64>> {
        None
    }
}

/// Settings of [`optimize_multiparam_bounded`].
#[derive(Debug, Clone, PartialEq)]
pub struct MultiParamOptions {
    /// One target value per row of [`MultiEval::values`].
    pub targets: Vec<f64>,
    /// Convergence tolerance per target: `|value − target| ≤ tol`.
    pub tolerances: Vec<f64>,
    /// The fixed parameter scaling `S` (positive): the step minimizes
    /// `‖S⁻¹Δθ‖` and the trust region is `|Δθ_i| ≤ max_scaled_step · S_i`.
    /// It must stay fixed for the whole run (with one target and P ≥ 2 the
    /// step is not unique, and `S` is what picks it).
    pub scaling: Vec<f64>,
    /// Starting accumulated parameters.
    pub theta0: Vec<f64>,
    /// Box on the accumulated parameters, `(lo, hi)` per parameter.
    pub theta_bounds: Vec<(f64, f64)>,
    /// Trust region in scaled units.
    pub max_scaled_step: f64,
    /// Maximum number of accepted steps.
    pub max_steps: usize,
    /// Maximum rounds of adding violated constraints per step.
    pub max_constraint_rounds: usize,
    /// Maximum step halvings (geometry, then descent) per step.
    pub max_backtracks: usize,
    /// The run stalls if the largest feasible fraction `t` of the linearized
    /// residual falls to or below this.
    pub min_progress: f64,
}

/// A constraint active in a step's QP solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveConstraint {
    /// Parameter `i` on its box lower bound.
    LowerBound(usize),
    /// Parameter `i` on its box upper bound.
    UpperBound(usize),
    /// Parameter `i` on the trust region.
    TrustRegion(usize),
    /// A problem constraint ([`StepConstraint::id`]).
    Problem(usize),
}

/// Why a run stopped short of convergence.
#[derive(Debug, Clone, PartialEq)]
pub enum MultiParamOutcome {
    /// Every `|value − target| ≤ tol`.
    Converged,
    /// No feasible step makes progress: the active constraints are listed.
    Stalled {
        /// The constraints active in the last QP solution.
        active: Vec<ActiveConstraint>,
        /// A one-line diagnosis.
        reason: String,
    },
    /// The step budget ran out.
    MaxSteps,
}

/// One accepted iterate (index 0 is the start).
#[derive(Debug, Clone, PartialEq)]
pub struct MultiParamStep {
    /// Iteration index.
    pub iter: usize,
    /// Accumulated parameters (a trace when the problem re-morphs).
    pub theta: Vec<f64>,
    /// The step that produced this iterate (zeros at index 0).
    pub dtheta: Vec<f64>,
    /// Values from the problem's own evaluation.
    pub values: Vec<f64>,
    /// `values − targets`.
    pub residuals: Vec<f64>,
    /// `Σ_j (residual_j / tol_j)²`.
    pub objective: f64,
    /// The fraction `t` of the linearized residual the step's QP targeted
    /// (1 at index 0).
    pub progress: f64,
    /// Constraints active in the step's QP solution.
    pub active: Vec<ActiveConstraint>,
    /// The problem's worst constraint quantity against the original geometry.
    pub min_quality_base: f64,
    /// The problem's worst constraint quantity against the previous iterate.
    pub min_quality_step: f64,
    /// Geometry checks spent on this step.
    pub n_geometry_checks: usize,
    /// Step halvings on this step.
    pub n_backtracks: usize,
    /// The independent confirmation of `values`, if the problem has one.
    pub confirmed: Option<Vec<f64>>,
    /// Worst `|confirmed − values| / |values|` (0 if not confirmed).
    pub confirm_rel: f64,
}

/// Result of [`optimize_multiparam_bounded`].
#[derive(Debug, Clone)]
pub struct MultiParamResult {
    /// Every accepted iterate, oldest first.
    pub trajectory: Vec<MultiParamStep>,
    /// How the run ended.
    pub outcome: MultiParamOutcome,
    /// Problem evaluations (fresh solves), accepted or not.
    pub n_evaluations: usize,
    /// Geometry checks over the whole run.
    pub n_geometry_checks: usize,
}

impl MultiParamResult {
    /// True iff the run converged.
    pub fn converged(&self) -> bool {
        self.outcome == MultiParamOutcome::Converged
    }

    /// The last accepted iterate.
    pub fn last(&self) -> &MultiParamStep {
        self.trajectory
            .last()
            .expect("trajectory has the start row")
    }
}

/// One linear inequality `row · u ≥ rhs` in scaled coordinates.
#[derive(Debug, Clone)]
struct Ineq {
    row: Vec<f64>,
    rhs: f64,
    tag: ActiveConstraint,
}

/// Solve the small dense system `A x = b` by Gaussian elimination with
/// partial pivoting; `None` if `A` is singular to `rel_tol` of its largest
/// diagonal-scale entry.
fn solve_small(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    let scale = a
        .iter()
        .flat_map(|r| r.iter())
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    for col in 0..n {
        let piv = (col..n).max_by(|&p, &q| a[p][col].abs().total_cmp(&a[q][col].abs()))?;
        if a[piv][col].abs() <= 1e-12 * scale {
            return None;
        }
        a.swap(col, piv);
        b.swap(col, piv);
        let (top, rest) = a.split_at_mut(col + 1);
        let pivot_row = &top[col];
        for (k, row) in rest.iter_mut().enumerate() {
            let f = row[col] / pivot_row[col];
            if f != 0.0 {
                for (x, pv) in row.iter_mut().zip(pivot_row).skip(col) {
                    *x -= f * pv;
                }
                b[col + 1 + k] -= f * b[col];
            }
        }
    }
    let mut x = vec![0.0; n];
    for r in (0..n).rev() {
        let s: f64 = (r + 1..n).map(|c| a[r][c] * x[c]).sum();
        x[r] = (b[r] - s) / a[r][r];
    }
    Some(x)
}

/// The minimum-norm `u` with `rows · u = rhs`, or `None` if the rows are
/// dependent.
fn min_norm_solution(rows: &[&[f64]], rhs: &[f64], p: usize) -> Option<Vec<f64>> {
    let m = rows.len();
    if m == 0 {
        return Some(vec![0.0; p]);
    }
    let g: Vec<Vec<f64>> = (0..m)
        .map(|i| {
            (0..m)
                .map(|j| rows[i].iter().zip(rows[j]).map(|(a, b)| a * b).sum())
                .collect()
        })
        .collect();
    let lam = solve_small(g, rhs.to_vec())?;
    let mut u = vec![0.0; p];
    for (row, l) in rows.iter().zip(&lam) {
        for (ui, ri) in u.iter_mut().zip(row.iter()) {
            *ui += l * ri;
        }
    }
    Some(u)
}

/// All subsets of `0..n` of size at most `k`, smallest first.
fn subsets_up_to(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = vec![Vec::new()];
    let mut frontier = vec![Vec::new()];
    for _ in 0..k.min(n) {
        let mut next = Vec::new();
        for s in &frontier {
            let start = s.last().map_or(0, |&l| l + 1);
            for j in start..n {
                let mut t = s.clone();
                t.push(j);
                next.push(t);
            }
        }
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out
}

/// Exact minimum-norm solution of `eq_rows · u = eq_rhs`, `ineq` by
/// active-set enumeration; `None` if infeasible.
fn qp_min_norm(
    eq_rows: &[Vec<f64>],
    eq_rhs: &[f64],
    ineq: &[Ineq],
    p: usize,
) -> Option<(Vec<f64>, Vec<ActiveConstraint>)> {
    let free = p.saturating_sub(eq_rows.len());
    let feas_tol = |rhs: f64, row: &[f64]| {
        1e-12 * (1.0 + rhs.abs() + row.iter().fold(0.0_f64, |m, v| m.max(v.abs())))
    };
    let mut best: Option<(f64, Vec<f64>)> = None;
    for set in subsets_up_to(ineq.len(), free) {
        let mut rows: Vec<&[f64]> = eq_rows.iter().map(Vec::as_slice).collect();
        let mut rhs = eq_rhs.to_vec();
        for &k in &set {
            rows.push(&ineq[k].row);
            rhs.push(ineq[k].rhs);
        }
        let Some(u) = min_norm_solution(&rows, &rhs, p) else {
            continue;
        };
        let eq_ok = eq_rows.iter().zip(eq_rhs).all(|(r, b)| {
            let v: f64 = r.iter().zip(&u).map(|(a, x)| a * x).sum();
            (v - b).abs() <= feas_tol(*b, r)
        });
        let ineq_ok = ineq.iter().all(|c| {
            let v: f64 = c.row.iter().zip(&u).map(|(a, x)| a * x).sum();
            v >= c.rhs - feas_tol(c.rhs, &c.row)
        });
        if eq_ok && ineq_ok {
            let norm: f64 = u.iter().map(|x| x * x).sum();
            if best.as_ref().is_none_or(|(n, _)| norm < *n) {
                best = Some((norm, u));
            }
        }
    }
    best.map(|(_, u)| {
        let active = ineq
            .iter()
            .filter(|c| {
                let v: f64 = c.row.iter().zip(&u).map(|(a, x)| a * x).sum();
                (v - c.rhs).abs() <= 1e-8 * (1.0 + c.rhs.abs())
            })
            .map(|c| c.tag)
            .collect();
        (u, active)
    })
}

/// The step QP: maximize `t ∈ [0, 1]`, then the minimum-norm scaled step.
/// Returns `(Δθ, t, active)`.
fn constrained_step(
    jac: &[Vec<f64>],
    residuals: &[f64],
    opts: &MultiParamOptions,
    theta: &[f64],
    problem_rows: &[StepConstraint],
) -> (Vec<f64>, f64, Vec<ActiveConstraint>) {
    let p = theta.len();
    let s = &opts.scaling;
    let tau = opts.max_scaled_step;
    let mut ineq: Vec<Ineq> = Vec::new();
    for i in 0..p {
        let (blo, bhi) = opts.theta_bounds[i];
        let lo_box = (blo - theta[i]) / s[i];
        let hi_box = (bhi - theta[i]) / s[i];
        let mut e = vec![0.0; p];
        e[i] = 1.0;
        let (lo, lo_tag) = if lo_box >= -tau {
            (lo_box, ActiveConstraint::LowerBound(i))
        } else {
            (-tau, ActiveConstraint::TrustRegion(i))
        };
        let (hi, hi_tag) = if hi_box <= tau {
            (hi_box, ActiveConstraint::UpperBound(i))
        } else {
            (tau, ActiveConstraint::TrustRegion(i))
        };
        ineq.push(Ineq {
            row: e.clone(),
            rhs: lo.min(0.0),
            tag: lo_tag,
        });
        ineq.push(Ineq {
            row: e.iter().map(|v| -v).collect(),
            rhs: (-hi).min(0.0),
            tag: hi_tag,
        });
    }
    for c in problem_rows {
        ineq.push(Ineq {
            row: c.grad.iter().zip(s).map(|(g, si)| g * si).collect(),
            rhs: c.rhs.min(0.0),
            tag: ActiveConstraint::Problem(c.id),
        });
    }
    // Normalize every row to unit length (rhs with it): the E_C rows are ~1e8
    // Hz per unit θ, the bound rows 1, and the Gram solves must not mistake
    // that scale gap for rank deficiency.
    let norm = |r: &[f64]| r.iter().map(|x| x * x).sum::<f64>().sqrt();
    for c in &mut ineq {
        let n = norm(&c.row);
        if n > 0.0 {
            c.row.iter_mut().for_each(|x| *x /= n);
            c.rhs /= n;
        }
    }
    let mut eq_rows: Vec<Vec<f64>> = Vec::with_capacity(jac.len());
    let mut eq_res: Vec<f64> = Vec::with_capacity(jac.len());
    for (r, res) in jac.iter().zip(residuals) {
        let row: Vec<f64> = r.iter().zip(s).map(|(g, si)| g * si).collect();
        let n = norm(&row);
        if n > 0.0 {
            eq_rows.push(row.iter().map(|x| x / n).collect());
            eq_res.push(res / n);
        } else {
            eq_rows.push(row);
            eq_res.push(*res);
        }
    }
    let solve = |t: f64| {
        let rhs: Vec<f64> = eq_res.iter().map(|r| -t * r).collect();
        qp_min_norm(&eq_rows, &rhs, &ineq, p)
    };
    // Snap onto the box / trust rows (the feasibility tolerance may leave
    // the solution a few ulps outside them).
    let (lo_u, hi_u): (Vec<f64>, Vec<f64>) = (0..p)
        .map(|i| (ineq[2 * i].rhs, -ineq[2 * i + 1].rhs))
        .unzip();
    let unscale = |u: &[f64]| -> Vec<f64> {
        u.iter()
            .zip(s)
            .enumerate()
            .map(|(i, (x, si))| x.clamp(lo_u[i], hi_u[i]) * si)
            .collect()
    };
    if let Some((u, active)) = solve(1.0) {
        return (unscale(&u), 1.0, active);
    }
    // Largest feasible t by bisection (feasibility in t is an interval that
    // contains 0, where u = 0 satisfies every row).
    let (mut good, mut bad) = (0.0_f64, 1.0_f64);
    for _ in 0..60 {
        let mid = 0.5 * (good + bad);
        if solve(mid).is_some() {
            good = mid;
        } else {
            bad = mid;
        }
    }
    match solve(good) {
        Some((u, active)) => (unscale(&u), good, active),
        None => {
            let active = ineq
                .iter()
                .filter(|c| c.rhs == 0.0)
                .map(|c| c.tag)
                .collect();
            (vec![0.0; p], 0.0, active)
        }
    }
}

fn weighted_objective(residuals: &[f64], tols: &[f64]) -> f64 {
    residuals
        .iter()
        .zip(tols)
        .map(|(r, t)| (r / t).powi(2))
        .sum()
}

/// **Bounded multi-parameter Gauss-Newton** toward one or more targets, with
/// box bounds, a scaled trust region, and linearized problem constraints
/// (issue #1036).
///
/// Each iteration:
///
/// 1. solves the step QP (module comment above): the largest fraction
///    `t ≤ 1` of the linearized residual reachable under the box, the trust
///    region and the problem rows gathered so far, then the minimum-norm
///    step in the scaling `S`;
/// 2. checks the trial geometry ([`MultiParamProblem::check_step`], no
///    solve). If it fails, the violated constraints are added and the QP is
///    re-solved, up to `max_constraint_rounds`; then the step is halved until
///    the geometry is valid;
/// 3. evaluates the trial with a fresh solve and accepts it only if the
///    objective `Σ (r_j/tol_j)²` decreases (else halves the step);
/// 4. after accepting, asks the problem for an independent confirmation and
///    records its agreement.
///
/// It stops when every `|r_j| ≤ tol_j` ([`MultiParamOutcome::Converged`]),
/// when no feasible step makes progress ([`MultiParamOutcome::Stalled`],
/// with the active constraints: the honest outcome when the target lies
/// outside the reachable set), or after `max_steps`.
///
/// # Panics
///
/// Panics if the option vectors do not match the problem's `P` and `M`, a
/// scaling or tolerance is not positive, `theta0` lies outside the box, or
/// the problem returns a Jacobian of the wrong shape.
#[allow(clippy::too_many_lines)]
pub fn optimize_multiparam_bounded<P: MultiParamProblem>(
    problem: &mut P,
    opts: &MultiParamOptions,
) -> MultiParamResult {
    let p = problem.n_params();
    let m = opts.targets.len();
    assert!(m >= 1, "at least one target");
    assert_eq!(opts.tolerances.len(), m, "one tolerance per target");
    assert_eq!(opts.scaling.len(), p, "one scale per parameter");
    assert_eq!(opts.theta0.len(), p, "theta0 length");
    assert_eq!(opts.theta_bounds.len(), p, "one bound pair per parameter");
    assert!(
        opts.tolerances.iter().all(|t| *t > 0.0),
        "tolerances must be positive"
    );
    assert!(
        opts.scaling.iter().all(|s| s.is_finite() && *s > 0.0),
        "scaling must be finite and positive"
    );
    assert!(
        opts.max_scaled_step > 0.0,
        "max_scaled_step must be positive"
    );
    for (i, (&t0, &(lo, hi))) in opts.theta0.iter().zip(&opts.theta_bounds).enumerate() {
        assert!(
            lo <= t0 && t0 <= hi,
            "theta bounds [{lo}, {hi}] of parameter {i} must contain theta0 = {t0}"
        );
    }
    let check_eval = |e: &MultiEval| {
        assert_eq!(
            e.values.len(),
            m,
            "problem returned {} values",
            e.values.len()
        );
        assert_eq!(e.jacobian.len(), m, "Jacobian rows");
        assert!(e.jacobian.iter().all(|r| r.len() == p), "Jacobian columns");
    };
    let residuals_of = |e: &MultiEval| -> Vec<f64> {
        e.values
            .iter()
            .zip(&opts.targets)
            .map(|(v, t)| v - t)
            .collect()
    };
    let confirm_rel = |values: &[f64], confirmed: &Option<Vec<f64>>| -> f64 {
        confirmed.as_ref().map_or(0.0, |c| {
            values
                .iter()
                .zip(c)
                .map(|(v, x)| (x - v).abs() / v.abs().max(f64::MIN_POSITIVE))
                .fold(0.0_f64, f64::max)
        })
    };

    let mut eval = problem.current();
    check_eval(&eval);
    let mut theta = opts.theta0.clone();
    let mut residuals = residuals_of(&eval);
    let mut objective = weighted_objective(&residuals, &opts.tolerances);
    let confirmed0 = problem.confirm();
    let mut trajectory = vec![MultiParamStep {
        iter: 0,
        theta: theta.clone(),
        dtheta: vec![0.0; p],
        values: eval.values.clone(),
        residuals: residuals.clone(),
        objective,
        progress: 1.0,
        active: Vec::new(),
        min_quality_base: f64::NAN,
        min_quality_step: f64::NAN,
        n_geometry_checks: 0,
        n_backtracks: 0,
        confirm_rel: confirm_rel(&eval.values, &confirmed0),
        confirmed: confirmed0,
    }];
    let mut n_evaluations = 0usize;
    let mut n_geometry_checks = 0usize;
    let converged_now = |r: &[f64]| r.iter().zip(&opts.tolerances).all(|(r, t)| r.abs() <= *t);

    let outcome = 'run: {
        for iter in 1..=opts.max_steps {
            if converged_now(&residuals) {
                break 'run MultiParamOutcome::Converged;
            }
            let mut rows: Vec<StepConstraint> = Vec::new();
            let mut step_checks = 0usize;
            let mut backtracks = 0usize;
            let (mut dtheta, progress, active, check) = {
                let mut round = 0usize;
                loop {
                    let (d, t, act) =
                        constrained_step(&eval.jacobian, &residuals, opts, &theta, &rows);
                    if t <= opts.min_progress || d.iter().all(|x| *x == 0.0) {
                        break 'run MultiParamOutcome::Stalled {
                            active: act,
                            reason: format!(
                                "no feasible step makes progress (largest feasible fraction of \
                                 the linearized residual t = {t:.3e})"
                            ),
                        };
                    }
                    let chk = problem.check_step(&d);
                    step_checks += 1;
                    if chk.feasible {
                        break (d, t, act, chk);
                    }
                    let new: Vec<StepConstraint> = chk
                        .violated
                        .iter()
                        .filter(|c| rows.iter().all(|r| r.id != c.id))
                        .cloned()
                        .collect();
                    round += 1;
                    if new.is_empty() || round >= opts.max_constraint_rounds {
                        // Linearization exhausted: halve along d until valid.
                        let mut h = d;
                        let mut found = None;
                        for _ in 0..opts.max_backtracks {
                            for x in h.iter_mut() {
                                *x *= 0.5;
                            }
                            backtracks += 1;
                            let c2 = problem.check_step(&h);
                            step_checks += 1;
                            if c2.feasible {
                                found = Some(c2);
                                break;
                            }
                        }
                        match found {
                            Some(c2) => break (h, t, act, c2),
                            None => {
                                break 'run MultiParamOutcome::Stalled {
                                    active: act,
                                    reason: "no geometrically valid step after backtracking"
                                        .to_string(),
                                };
                            }
                        }
                    }
                    rows.extend(new);
                }
            };
            // Fresh solve; accept only on descent.
            let mut check = check;
            let new_eval = loop {
                let e = problem.evaluate_step(&dtheta);
                n_evaluations += 1;
                check_eval(&e);
                let r = residuals_of(&e);
                if weighted_objective(&r, &opts.tolerances) < objective {
                    break e;
                }
                if backtracks >= opts.max_backtracks {
                    n_geometry_checks += step_checks;
                    break 'run MultiParamOutcome::Stalled {
                        active: active.clone(),
                        reason: "the fresh solve did not decrease the objective after \
                                 backtracking"
                            .to_string(),
                    };
                }
                for x in dtheta.iter_mut() {
                    *x *= 0.5;
                }
                backtracks += 1;
                check = problem.check_step(&dtheta);
                step_checks += 1;
                if !check.feasible {
                    n_geometry_checks += step_checks;
                    break 'run MultiParamOutcome::Stalled {
                        active: active.clone(),
                        reason: "a halved descent step left the valid geometry".to_string(),
                    };
                }
            };
            n_geometry_checks += step_checks;
            problem.accept_step(&dtheta);
            for (t, d) in theta.iter_mut().zip(&dtheta) {
                *t += d;
            }
            eval = new_eval;
            residuals = residuals_of(&eval);
            objective = weighted_objective(&residuals, &opts.tolerances);
            let confirmed = problem.confirm();
            trajectory.push(MultiParamStep {
                iter,
                theta: theta.clone(),
                dtheta,
                values: eval.values.clone(),
                residuals: residuals.clone(),
                objective,
                progress,
                active,
                min_quality_base: check.min_quality_base,
                min_quality_step: check.min_quality_step,
                n_geometry_checks: step_checks,
                n_backtracks: backtracks,
                confirm_rel: confirm_rel(&eval.values, &confirmed),
                confirmed,
            });
        }
        if converged_now(&residuals) {
            MultiParamOutcome::Converged
        } else {
            MultiParamOutcome::MaxSteps
        }
    };

    MultiParamResult {
        trajectory,
        outcome,
        n_evaluations,
        n_geometry_checks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Closed-form `E_C(θ)` for the clean parallel-plate fixture used by the
    /// example: gap `d = d0 (1 + θ)`, so `C(θ) = C0 / (1 + θ)` and
    /// `E_C(θ) = E_C0 (1 + θ)` — exactly affine in θ. `∂E_C/∂θ = E_C0`.
    fn analytic_eval(e_c0_hz: f64, c0_farad: f64) -> impl FnMut(f64) -> (f64, f64, f64) {
        move |theta: f64| {
            let c = c0_farad / (1.0 + theta);
            let e_c = e_c0_hz * (1.0 + theta);
            let de_c_dtheta = e_c0_hz; // d/dθ [E_C0 (1+θ)]
            (c, e_c, de_c_dtheta)
        }
    }

    /// Full Newton (`α = 1`) on the exactly-affine response lands the target
    /// in a SINGLE step — the headline capability result.
    #[test]
    fn full_newton_converges_in_one_step() {
        let e_c0 = 0.16143e9; // 120 fF start
        let c0 = 120e-15;
        let target = 0.2156e9; // the transmon anchor
        let res = optimize_e_c_to_target(target, 0.0, 1.0, 1.0, 20, analytic_eval(e_c0, c0));
        assert!(res.converged, "Newton must converge");
        assert_eq!(res.n_steps, 1, "affine response ⇒ exactly one Newton step");
        assert!(
            (res.e_c_final_hz - target).abs() < 1e-3,
            "final E_C {} vs target {target}",
            res.e_c_final_hz
        );
        // Closed-form θ* = target/E_C0 − 1.
        let theta_star = target / e_c0 - 1.0;
        assert!(
            (res.theta_final - theta_star).abs() < 1e-9,
            "θ_final {} vs closed-form θ* {theta_star}",
            res.theta_final
        );
    }

    /// Damped Newton (`α = 0.5`) produces a monotone, multi-point geometric
    /// trajectory that still converges to the same target — the convergence
    /// curve for the figure. The residual must shrink every step.
    #[test]
    fn damped_newton_geometric_trajectory() {
        let e_c0 = 0.16143e9;
        let c0 = 120e-15;
        let target = 0.2156e9;
        let tol = 1e3; // 1 kHz
        let res = optimize_e_c_to_target(target, 0.0, 0.5, tol, 50, analytic_eval(e_c0, c0));
        assert!(res.converged, "damped Newton must converge within tol");
        assert!(
            res.n_steps > 1 && res.n_steps < 40,
            "expected a handful of steps, got {}",
            res.n_steps
        );
        // Strictly decreasing residual magnitude (geometric contraction).
        for w in res.trajectory.windows(2) {
            assert!(
                w[1].residual_hz.abs() < w[0].residual_hz.abs(),
                "residual must shrink: {} → {}",
                w[0].residual_hz,
                w[1].residual_hz
            );
        }
        // For an affine response, α = 0.5 contracts the residual by exactly
        // (1 − α) = 0.5 each step.
        let r0 = res.trajectory[0].residual_hz.abs();
        let r1 = res.trajectory[1].residual_hz.abs();
        assert!(
            (r1 / r0 - 0.5).abs() < 1e-9,
            "α = 0.5 ⇒ residual ratio 0.5, got {}",
            r1 / r0
        );
    }

    /// Already-on-target start converges in zero steps.
    #[test]
    fn zero_steps_when_started_on_target() {
        let target = 0.2e9;
        // E_C0 chosen so θ = 0 already sits on target.
        let res = optimize_e_c_to_target(target, 0.0, 1.0, 1.0, 10, analytic_eval(target, 100e-15));
        assert!(res.converged);
        assert_eq!(res.n_steps, 0);
    }

    #[test]
    #[should_panic(expected = "damping alpha must be in")]
    fn rejects_bad_alpha() {
        optimize_e_c_to_target(1.0e9, 0.0, 1.5, 1.0, 10, analytic_eval(1.0e9, 100e-15));
    }

    /// The step clamp bounds every update to `|Δθ| ≤ max_abs_dtheta`, walks
    /// monotonically toward the target while the unclamped Newton step
    /// exceeds the clamp, and still converges to the same closed-form θ*.
    #[test]
    fn clamped_newton_bounds_every_step_and_converges() {
        let e_c0 = 0.16143e9;
        let c0 = 120e-15;
        let target = 0.2156e9; // θ* = target/E_C0 − 1 ≈ 0.3357
        let clamp = 0.1;
        let res = optimize_e_c_to_target_clamped(
            target,
            0.0,
            1.0,
            1.0,
            20,
            clamp,
            analytic_eval(e_c0, c0),
        );
        assert!(res.converged, "clamped Newton must converge");
        // Every step honors the clamp; the early steps are exactly clamped
        // (the affine unclamped step would be θ* ≈ 0.336 > 0.1 immediately).
        for w in res.trajectory.windows(2) {
            let dt = w[1].theta - w[0].theta;
            assert!(
                dt.abs() <= clamp + 1e-12,
                "step {} → {}: |Δθ| = {} exceeds clamp {clamp}",
                w[0].iter,
                w[1].iter,
                dt.abs()
            );
        }
        let first = res.trajectory[1].theta - res.trajectory[0].theta;
        assert!(
            (first - clamp).abs() < 1e-12,
            "first step must be exactly clamped, got Δθ = {first}"
        );
        // Clamping costs extra steps (θ*/clamp ≈ 4 clamped legs) but lands
        // the same closed-form optimum.
        let theta_star = target / e_c0 - 1.0;
        assert!(res.n_steps >= 4, "expected ≥4 steps, got {}", res.n_steps);
        assert!(
            (res.theta_final - theta_star).abs() < 1e-9,
            "θ_final {} vs closed-form θ* {theta_star}",
            res.theta_final
        );
    }

    /// A box bound TIGHTER than the optimum: the run walks to the bound,
    /// reports `stalled_at_bound` (not a fabricated convergence), and its
    /// final θ is exactly the bound. With the bound RELAXED past θ*, the
    /// same run converges normally with `stalled_at_bound == false`.
    #[test]
    fn bounded_newton_stalls_honestly_at_a_tight_bound() {
        let e_c0 = 0.16143e9;
        let c0 = 120e-15;
        let target = 0.2156e9; // θ* ≈ 0.3357, beyond the 0.2 bound below
        let bound = 0.2;
        let res = optimize_e_c_to_target_bounded(
            target,
            0.0,
            1.0,
            1.0,
            20,
            f64::INFINITY,
            (0.0, bound),
            analytic_eval(e_c0, c0),
        );
        assert!(!res.converged, "must NOT report convergence at the bound");
        assert!(res.stalled_at_bound, "must report the bound stall");
        assert_eq!(res.theta_final, bound, "final θ must sit on the bound");
        // Every visited θ honors the box.
        for s in &res.trajectory {
            assert!((0.0..=bound).contains(&s.theta), "θ {} out of box", s.theta);
        }
        // The residual at the bound is the honest partial result: it moved
        // toward the target but did not reach it.
        let r0 = res.trajectory[0].residual_hz.abs();
        let rf = res.trajectory.last().unwrap().residual_hz.abs();
        assert!(rf < r0, "the bound-constrained run must still improve");
        assert!(rf > 1.0, "residual at the bound must remain nonzero");

        // Relaxing the bound past θ* recovers plain convergence.
        let free = optimize_e_c_to_target_bounded(
            target,
            0.0,
            1.0,
            1.0,
            20,
            f64::INFINITY,
            (0.0, 1.0),
            analytic_eval(e_c0, c0),
        );
        assert!(free.converged && !free.stalled_at_bound);
    }

    #[test]
    #[should_panic(expected = "must contain theta0")]
    fn rejects_theta0_outside_bounds() {
        optimize_e_c_to_target_bounded(
            1.0e9,
            0.5,
            1.0,
            1.0,
            10,
            1.0,
            (-0.1, 0.1),
            analytic_eval(1.0e9, 100e-15),
        );
    }

    #[test]
    #[should_panic(expected = "max_abs_dtheta must be positive")]
    fn rejects_bad_clamp() {
        optimize_e_c_to_target_clamped(
            1.0e9,
            0.0,
            1.0,
            1.0,
            10,
            0.0,
            analytic_eval(1.0e9, 100e-15),
        );
    }

    // ─────────────────────────────────────────────────────────────────────
    // Multi-parameter bounded Gauss-Newton (issue #1036).
    // ─────────────────────────────────────────────────────────────────────

    type ValueFn = Box<dyn Fn(&[f64]) -> (Vec<f64>, Vec<Vec<f64>>)>;
    type QualityFn = Box<dyn Fn(&[f64]) -> (f64, Vec<f64>)>;

    /// A closed-form problem: values and Jacobian from `f(θ)`, and an
    /// optional nonlinear quality `q(θ) ≥ floor` playing the mesh constraint.
    struct Analytic {
        theta: Vec<f64>,
        f: ValueFn,
        q: Option<QualityFn>,
        floor: f64,
        pending: Option<Vec<f64>>,
        confirms: usize,
    }

    impl Analytic {
        fn new(p: usize, f: ValueFn) -> Self {
            Self {
                theta: vec![0.0; p],
                f,
                q: None,
                floor: 0.0,
                pending: None,
                confirms: 0,
            }
        }
        fn at(&self, d: &[f64]) -> Vec<f64> {
            self.theta.iter().zip(d).map(|(t, x)| t + x).collect()
        }
    }

    impl MultiParamProblem for Analytic {
        fn n_params(&self) -> usize {
            self.theta.len()
        }
        fn current(&self) -> MultiEval {
            let (values, jacobian) = (self.f)(&self.theta);
            MultiEval { values, jacobian }
        }
        fn check_step(&mut self, d: &[f64]) -> StepCheck {
            let Some(q) = &self.q else {
                return StepCheck {
                    feasible: true,
                    min_quality_base: 1.0,
                    min_quality_step: 1.0,
                    violated: Vec::new(),
                };
            };
            let (v, _) = q(&self.at(d));
            let (now, grad) = q(&self.theta);
            StepCheck {
                feasible: v >= self.floor,
                min_quality_base: v,
                min_quality_step: v / now,
                violated: if v >= self.floor {
                    Vec::new()
                } else {
                    vec![StepConstraint {
                        id: 7,
                        grad,
                        rhs: self.floor + 0.01 - now,
                    }]
                },
            }
        }
        fn evaluate_step(&mut self, d: &[f64]) -> MultiEval {
            self.pending = Some(d.to_vec());
            let (values, jacobian) = (self.f)(&self.at(d));
            MultiEval { values, jacobian }
        }
        fn accept_step(&mut self, d: &[f64]) {
            assert_eq!(self.pending.take().as_deref(), Some(d));
            self.theta = self.at(d);
        }
        fn confirm(&mut self) -> Option<Vec<f64>> {
            self.confirms += 1;
            Some((self.f)(&self.theta).0)
        }
    }

    /// Parallel plate `C = ε A/d` with gap `d = d0 (1 + θ_0)` and area
    /// `A = A0 (1 + θ_1)`: `E_C = E0 (1 + θ_0)/(1 + θ_1)`.
    fn plate(e0: f64) -> ValueFn {
        Box::new(move |t: &[f64]| {
            let e = e0 * (1.0 + t[0]) / (1.0 + t[1]);
            (vec![e], vec![vec![e0 / (1.0 + t[1]), -e / (1.0 + t[1])]])
        })
    }

    fn opts(target: f64, p: usize, bounds: (f64, f64)) -> MultiParamOptions {
        MultiParamOptions {
            targets: vec![target],
            tolerances: vec![1.0],
            scaling: vec![1.0; p],
            theta0: vec![0.0; p],
            theta_bounds: vec![bounds; p],
            max_scaled_step: 0.2,
            max_steps: 60,
            max_constraint_rounds: 8,
            max_backtracks: 30,
            min_progress: 1e-9,
        }
    }

    /// Two parameters, one target inside the box: converges to tolerance,
    /// every iterate stays in the box and the trust region, every accepted
    /// step is confirmed, and the residual decreases monotonically.
    #[test]
    fn multiparam_plate_converges_inside_the_box() {
        let e0 = 0.16e9;
        let target = e0 / 0.7; // C = 0.7 C0
        let mut prob = Analytic::new(2, plate(e0));
        let o = opts(target, 2, (-0.5, 1.0));
        let res = optimize_multiparam_bounded(&mut prob, &o);
        assert!(res.converged(), "outcome {:?}", res.outcome);
        let last = res.last();
        assert!(last.residuals[0].abs() <= 1.0);
        let (t0, t1) = (last.theta[0], last.theta[1]);
        let c_ratio = (1.0 + t1) / (1.0 + t0);
        assert!((c_ratio - 0.7).abs() < 1e-8, "C/C0 = {c_ratio}");
        // Both parameters move (minimum norm in unit scaling shares the work).
        assert!(t0 > 0.0 && t1 < 0.0, "θ = {:?}", last.theta);
        for w in res.trajectory.windows(2) {
            assert!(w[1].objective < w[0].objective);
            for d in &w[1].dtheta {
                assert!(d.abs() <= 0.2 + 1e-12, "trust region violated: {d}");
            }
        }
        for s in &res.trajectory {
            assert!(s.theta.iter().all(|t| (-0.5..=1.0).contains(t)));
            assert!(s.confirmed.is_some() && s.confirm_rel <= 1e-15);
        }
        assert_eq!(prob.confirms, res.trajectory.len());
    }

    /// A target outside the box stops on the box corner with an honest
    /// `Stalled` outcome naming the active bounds, not a fabricated
    /// convergence.
    #[test]
    fn multiparam_plate_stalls_honestly_outside_the_box() {
        let e0 = 0.16e9;
        let target = e0 / 0.2; // C = 0.2 C0; the box allows only C ≥ 0.467 C0
        let mut prob = Analytic::new(2, plate(e0));
        let mut o = opts(target, 2, (0.0, 0.0));
        o.theta_bounds = vec![(0.0, 0.5), (-0.3, 0.0)];
        let res = optimize_multiparam_bounded(&mut prob, &o);
        assert!(!res.converged());
        match &res.outcome {
            MultiParamOutcome::Stalled { active, .. } => {
                assert!(
                    active.contains(&ActiveConstraint::UpperBound(0)),
                    "{active:?}"
                );
                assert!(
                    active.contains(&ActiveConstraint::LowerBound(1)),
                    "{active:?}"
                );
            }
            other => panic!("expected a stall, got {other:?}"),
        }
        let last = res.last();
        assert!((last.theta[0] - 0.5).abs() < 1e-12 && (last.theta[1] + 0.3).abs() < 1e-12);
        assert!(last.residuals[0].abs() > 1e6, "the gap must remain");
        let first = &res.trajectory[0];
        assert!(last.residuals[0].abs() < first.residuals[0].abs());
    }

    /// A nonlinear quality constraint on the area parameter
    /// (`q = (1 + θ_1)³ ≥ 0.25`, i.e. `θ_1 ≥ −0.37`) with a scaling that
    /// favours area: the optimizer hits the constraint, slides along it onto
    /// the gap parameter, and still converges; every accepted iterate meets
    /// the floor.
    #[test]
    fn multiparam_slides_along_a_linearized_constraint() {
        let e0 = 0.16e9;
        let target = e0 / 0.4; // C = 0.4 C0
        let mut prob = Analytic::new(2, plate(e0));
        prob.q = Some(Box::new(|t: &[f64]| {
            ((1.0 + t[1]).powi(3), vec![0.0, 3.0 * (1.0 + t[1]).powi(2)])
        }));
        prob.floor = 0.25;
        let mut o = opts(target, 2, (-0.9, 3.0));
        o.scaling = vec![0.1, 1.0];
        o.max_scaled_step = 2.0;
        let res = optimize_multiparam_bounded(&mut prob, &o);
        assert!(res.converged(), "outcome {:?}", res.outcome);
        let mut hit = false;
        for s in &res.trajectory[1..] {
            assert!(
                s.min_quality_base >= 0.25,
                "accepted q {}",
                s.min_quality_base
            );
            hit |= s.active.contains(&ActiveConstraint::Problem(7));
        }
        assert!(hit, "the quality constraint must become active");
        let last = res.last();
        assert!(
            last.theta[1] < -0.3,
            "area used up to the constraint: {:?}",
            last.theta
        );
        assert!(last.theta[0] > 0.0, "the gap takes over the rest");
    }

    /// The Phase C hook: two targets and three parameters (a hold row
    /// `θ_1 − θ_2 = 0` beside the `E_C` row) converge together.
    #[test]
    fn multiparam_two_targets_three_parameters() {
        let e0 = 0.16e9;
        let target = e0 / 0.6;
        let f: ValueFn = Box::new(move |t: &[f64]| {
            let a = (1.0 + t[1]) * (1.0 + t[2]);
            let e = e0 * (1.0 + t[0]) / a;
            (
                vec![e, t[1] - t[2]],
                vec![
                    vec![e0 / a, -e / (1.0 + t[1]), -e / (1.0 + t[2])],
                    vec![0.0, 1.0, -1.0],
                ],
            )
        });
        let mut prob = Analytic::new(3, f);
        let mut o = opts(target, 3, (-0.5, 1.0));
        o.targets = vec![target, 0.0];
        o.tolerances = vec![1.0, 1e-9];
        let res = optimize_multiparam_bounded(&mut prob, &o);
        assert!(res.converged(), "outcome {:?}", res.outcome);
        let last = res.last();
        assert!((last.theta[1] - last.theta[2]).abs() <= 1e-9);
        assert!(last.residuals[0].abs() <= 1.0);
    }

    #[test]
    fn qp_enumeration_finds_the_projected_minimum_norm_point() {
        // min |u|² s.t. u0 + u1 = 2, u0 ≤ 0.5  →  u = (0.5, 1.5).
        let ineq = vec![Ineq {
            row: vec![-1.0, 0.0],
            rhs: -0.5,
            tag: ActiveConstraint::UpperBound(0),
        }];
        let (u, act) = qp_min_norm(&[vec![1.0, 1.0]], &[2.0], &ineq, 2).unwrap();
        assert!((u[0] - 0.5).abs() < 1e-14 && (u[1] - 1.5).abs() < 1e-14);
        assert_eq!(act, vec![ActiveConstraint::UpperBound(0)]);
        // Infeasible: u0 + u1 = 2 with u0 ≤ 0, u1 ≤ 0.
        let ineq2 = vec![
            Ineq {
                row: vec![-1.0, 0.0],
                rhs: 0.0,
                tag: ActiveConstraint::UpperBound(0),
            },
            Ineq {
                row: vec![0.0, -1.0],
                rhs: 0.0,
                tag: ActiveConstraint::UpperBound(1),
            },
        ];
        assert!(qp_min_norm(&[vec![1.0, 1.0]], &[2.0], &ineq2, 2).is_none());
    }
}
