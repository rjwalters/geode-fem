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
    let mut iter = 0;
    while !converged && iter < max_steps {
        // Damped-Newton update using the analytic derivative.
        theta -= alpha * step.residual_hz / step.de_c_hz_dtheta;
        iter += 1;
        step = record(iter, theta, &mut eval);
        trajectory.push(step);
        converged = step.residual_hz.abs() <= tol_hz;
    }

    DiffOptResult {
        converged,
        theta_final: step.theta,
        e_c_final_hz: step.e_c_hz,
        n_steps: trajectory.len() - 1,
        trajectory,
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
}
