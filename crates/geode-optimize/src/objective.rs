//! The [`Objective`] trait: the one interface between an optimizer and a
//! physics evaluation.
//!
//! An adjoint solve produces the figure of merit **and** its gradient from
//! one forward + adjoint pass (for an S-parameter objective,
//! `geode_core::driven::s_sensitivity::s_matrix_vjp` returns `SVjp {
//! objective, grad, .. }`). So one [`Objective::evaluate`] call returns `f`,
//! `∇f`, and for constrained problems the constraint values `g_i` and their
//! gradient rows, all at the same `x`.
//!
//! An evaluation may **fail**. For example, the forward solve can hit a
//! singular operator at a trial geometry, or a morph can invert a tet. Return
//! `Err(EvalError)`. The optimizers back off (a shorter line-search step, or
//! a more conservative MMA subproblem) instead of aborting, and they count
//! every failure in the iteration history. Only a failure at the starting
//! point is fatal. A non-finite `f`, gradient or constraint value is treated
//! the same way as a failure.

use serde::{Deserialize, Serialize};

/// The result of one evaluation at a point `x` of dimension `n`.
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    /// The objective value `f(x)`.
    pub f: f64,
    /// The gradient `∇f(x)`, of length `n`.
    pub grad: Vec<f64>,
    /// The constraint values `g_i(x)`, for constraints posed as `g_i(x) ≤ 0`
    /// (length `m`, empty for an unconstrained or box-only problem).
    pub constraints: Vec<f64>,
    /// The constraint Jacobian, one row `∇g_i(x)` (length `n`) per
    /// constraint.
    pub jacobian: Vec<Vec<f64>>,
}

impl Evaluation {
    /// An evaluation with no general constraints.
    #[must_use]
    pub fn unconstrained(f: f64, grad: Vec<f64>) -> Self {
        Self {
            f,
            grad,
            constraints: Vec::new(),
            jacobian: Vec::new(),
        }
    }

    /// An evaluation with constraint values `g` (meaning `g_i ≤ 0`) and the
    /// Jacobian rows `jac[i] = ∇g_i`.
    #[must_use]
    pub fn constrained(f: f64, grad: Vec<f64>, g: Vec<f64>, jac: Vec<Vec<f64>>) -> Self {
        Self {
            f,
            grad,
            constraints: g,
            jacobian: jac,
        }
    }

    /// Checks the dimensions against `(n, m)` and that every value is finite.
    ///
    /// # Errors
    ///
    /// An [`EvalError`] naming the first problem found. The optimizers treat
    /// it like a failed evaluation.
    pub fn validate(&self, n: usize, m: usize) -> Result<(), EvalError> {
        if self.grad.len() != n {
            return Err(EvalError::new(format!(
                "gradient has {} entries, the design has {n}",
                self.grad.len()
            )));
        }
        if self.constraints.len() != m || self.jacobian.len() != m {
            return Err(EvalError::new(format!(
                "{} constraint values / {} Jacobian rows, the objective declares {m} constraints",
                self.constraints.len(),
                self.jacobian.len()
            )));
        }
        if let Some(i) = self.jacobian.iter().position(|r| r.len() != n) {
            return Err(EvalError::new(format!(
                "Jacobian row {i} has {} entries, the design has {n}",
                self.jacobian[i].len()
            )));
        }
        let finite = self.f.is_finite()
            && self.grad.iter().all(|v| v.is_finite())
            && self.constraints.iter().all(|v| v.is_finite())
            && self.jacobian.iter().flatten().all(|v| v.is_finite());
        if !finite {
            return Err(EvalError::new(
                "non-finite objective, gradient or constraint value",
            ));
        }
        Ok(())
    }

    /// The largest constraint violation, `max(0, max_i g_i)`.
    #[must_use]
    pub fn max_violation(&self) -> f64 {
        self.constraints.iter().fold(0.0_f64, |m, &g| m.max(g))
    }
}

/// A failed evaluation (the forward or adjoint solve returned an error at
/// this point). It carries the message only, so it serializes into the
/// history.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct EvalError {
    /// A human-readable reason, e.g. the solver's error text.
    pub message: String,
}

impl EvalError {
    /// A failure with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Wraps any error (e.g. `SSensitivityError`) by its `Display` text.
    pub fn from_error<E: std::error::Error + ?Sized>(e: &E) -> Self {
        Self::new(e.to_string())
    }
}

/// A differentiable problem for the optimizers.
///
/// L-BFGS-B requires `n_constraints() == 0`; MMA accepts any `m ≥ 0`.
pub trait Objective {
    /// The number of design variables `n`.
    fn dim(&self) -> usize;

    /// The number of general inequality constraints `m` (`g_i(x) ≤ 0`).
    fn n_constraints(&self) -> usize {
        0
    }

    /// Evaluates `f`, `∇f` and (if `m > 0`) the constraints and their
    /// Jacobian at `x`, all from one solve.
    ///
    /// # Errors
    ///
    /// The evaluation failed at `x`. The optimizers back off and record it.
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError>;
}

impl<T: Objective + ?Sized> Objective for &mut T {
    fn dim(&self) -> usize {
        (**self).dim()
    }
    fn n_constraints(&self) -> usize {
        (**self).n_constraints()
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        (**self).evaluate(x)
    }
}

/// An unconstrained [`Objective`] from a closure returning `(f, ∇f)`.
///
/// This is the shape of an adjoint VJP: for example, a closure that builds
/// an `SDesign` from `x`, calls `s_matrix_vjp`, and returns
/// `(vjp.objective, vjp.grad)`. See the `s_matrix_vjp_adapter` example.
pub struct FnObjective<F> {
    dim: usize,
    f: F,
}

impl<F> FnObjective<F>
where
    F: FnMut(&[f64]) -> Result<(f64, Vec<f64>), EvalError>,
{
    /// Wraps `f` as an `n`-dimensional objective.
    pub fn new(dim: usize, f: F) -> Self {
        Self { dim, f }
    }
}

impl<F> Objective for FnObjective<F>
where
    F: FnMut(&[f64]) -> Result<(f64, Vec<f64>), EvalError>,
{
    fn dim(&self) -> usize {
        self.dim
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let (f, g) = (self.f)(x)?;
        Ok(Evaluation::unconstrained(f, g))
    }
}

/// A constrained [`Objective`] from a closure returning a full
/// [`Evaluation`].
pub struct FnConstrained<F> {
    dim: usize,
    m: usize,
    f: F,
}

impl<F> FnConstrained<F>
where
    F: FnMut(&[f64]) -> Result<Evaluation, EvalError>,
{
    /// Wraps `f` as an `n`-dimensional objective with `m` constraints.
    pub fn new(dim: usize, m: usize, f: F) -> Self {
        Self { dim, m, f }
    }
}

impl<F> Objective for FnConstrained<F>
where
    F: FnMut(&[f64]) -> Result<Evaluation, EvalError>,
{
    fn dim(&self) -> usize {
        self.dim
    }
    fn n_constraints(&self) -> usize {
        self.m
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        (self.f)(x)
    }
}

/// Affine design-variable scaling: the optimizer works on `x`, and the
/// physics sees `p = offset + scale ⊙ x`.
///
/// `geode optimize` gives each design variable "bounds, an initial value and
/// a scale" (#841 P6b). Optimizing in O(1) design units rather than metres or
/// relative permittivity keeps the L-BFGS initial Hessian and the MMA
/// asymptotes well conditioned. The gradient is chained exactly:
/// `∂f/∂x = weight · scale ⊙ ∂f/∂p`, and the constraint Jacobian columns are
/// scaled the same way (constraint values are not weighted). Set
/// `weight = -1` to **maximize** the inner objective.
pub struct Scaled<O> {
    inner: O,
    offset: Vec<f64>,
    scale: Vec<f64>,
    weight: f64,
}

impl<O: Objective> Scaled<O> {
    /// Wraps `inner` with `p = offset + scale ⊙ x` and `f = weight · f_inner`.
    ///
    /// # Panics
    ///
    /// `offset` / `scale` lengths differ from `inner.dim()`, or a scale entry
    /// is zero or not finite.
    pub fn new(inner: O, offset: Vec<f64>, scale: Vec<f64>, weight: f64) -> Self {
        assert_eq!(offset.len(), inner.dim(), "offset length");
        assert_eq!(scale.len(), inner.dim(), "scale length");
        assert!(
            scale.iter().all(|s| s.is_finite() && *s != 0.0),
            "scale entries must be finite and non-zero"
        );
        Self {
            inner,
            offset,
            scale,
            weight,
        }
    }

    /// The physical point `p` for the design point `x`.
    #[must_use]
    pub fn to_physical(&self, x: &[f64]) -> Vec<f64> {
        x.iter()
            .zip(&self.offset)
            .zip(&self.scale)
            .map(|((x, o), s)| o + s * x)
            .collect()
    }

    /// The design point `x` for the physical point `p`.
    #[must_use]
    pub fn to_design(&self, p: &[f64]) -> Vec<f64> {
        p.iter()
            .zip(&self.offset)
            .zip(&self.scale)
            .map(|((p, o), s)| (p - o) / s)
            .collect()
    }

    /// Maps physical bounds to design bounds (a negative scale swaps them).
    #[must_use]
    pub fn bounds_to_design(&self, b: &crate::Bounds) -> crate::Bounds {
        let lo = self.to_design(&b.lower);
        let hi = self.to_design(&b.upper);
        let (lower, upper) = lo
            .iter()
            .zip(&hi)
            .map(|(a, b)| (a.min(*b), a.max(*b)))
            .unzip();
        crate::Bounds { lower, upper }
    }

    /// The wrapped objective.
    pub fn inner(&self) -> &O {
        &self.inner
    }
}

impl<O: Objective> Objective for Scaled<O> {
    fn dim(&self) -> usize {
        self.inner.dim()
    }
    fn n_constraints(&self) -> usize {
        self.inner.n_constraints()
    }
    fn evaluate(&mut self, x: &[f64]) -> Result<Evaluation, EvalError> {
        let p = self.to_physical(x);
        let mut e = self.inner.evaluate(&p)?;
        e.f *= self.weight;
        for (g, s) in e.grad.iter_mut().zip(&self.scale) {
            *g *= self.weight * s;
        }
        for row in &mut e.jacobian {
            for (g, s) in row.iter_mut().zip(&self.scale) {
                *g *= s;
            }
        }
        Ok(e)
    }
}

/// The result of [`check_gradient`].
#[derive(Clone, Debug, PartialEq)]
pub struct GradientCheck {
    /// The adjoint (analytic) gradient at `x`.
    pub analytic: Vec<f64>,
    /// The central finite-difference gradient at `x`.
    pub finite_difference: Vec<f64>,
    /// `max_i |a_i − fd_i| / max(|fd|_∞, floor)`.
    pub max_rel_error: f64,
}

/// Central-difference check of `∇f` at `x` with per-coordinate step
/// `h · max(1, |x_i|)`. This is the self-check every `geode optimize` run
/// reports on its final design (#841 P6b). It costs `2n + 1` evaluations.
///
/// # Errors
///
/// Any evaluation fails.
pub fn check_gradient(
    obj: &mut dyn Objective,
    x: &[f64],
    h: f64,
) -> Result<GradientCheck, EvalError> {
    let base = obj.evaluate(x)?;
    let mut fd = Vec::with_capacity(x.len());
    let mut xp = x.to_vec();
    for i in 0..x.len() {
        let step = h * x[i].abs().max(1.0);
        xp[i] = x[i] + step;
        let fp = obj.evaluate(&xp)?.f;
        xp[i] = x[i] - step;
        let fm = obj.evaluate(&xp)?.f;
        xp[i] = x[i];
        fd.push((fp - fm) / (2.0 * step));
    }
    let scale = fd.iter().fold(1e-300_f64, |m, v| m.max(v.abs()));
    let err = base
        .grad
        .iter()
        .zip(&fd)
        .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
    Ok(GradientCheck {
        analytic: base.grad,
        finite_difference: fd,
        max_rel_error: err / scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Quad;
    impl Objective for Quad {
        fn dim(&self) -> usize {
            2
        }
        fn n_constraints(&self) -> usize {
            1
        }
        fn evaluate(&mut self, p: &[f64]) -> Result<Evaluation, EvalError> {
            Ok(Evaluation::constrained(
                p[0] * p[0] + 3.0 * p[0] * p[1],
                vec![2.0 * p[0] + 3.0 * p[1], 3.0 * p[0]],
                vec![p[0] - p[1] * p[1]],
                vec![vec![1.0, -2.0 * p[1]]],
            ))
        }
    }

    #[test]
    fn scaled_chains_gradient_and_jacobian_exactly() {
        let mut s = Scaled::new(Quad, vec![1.0, -2.0], vec![1e-3, 4.0], -2.0);
        let x = [0.7, 0.3];
        let chk = check_gradient(&mut s, &x, 1e-6).unwrap();
        assert!(chk.max_rel_error < 1e-8, "{chk:?}");
        let e = s.evaluate(&x).unwrap();
        let p = s.to_physical(&x);
        let h = 1e-6;
        let mut xp = x;
        xp[1] += h;
        let gp = s.evaluate(&xp).unwrap().constraints[0];
        xp[1] -= 2.0 * h;
        let gm = s.evaluate(&xp).unwrap().constraints[0];
        assert!((e.jacobian[0][1] - (gp - gm) / (2.0 * h)).abs() < 1e-6);
        let back = s.to_design(&p);
        assert!((back[0] - x[0]).abs() < 1e-12 && (back[1] - x[1]).abs() < 1e-12);
    }

    #[test]
    fn validate_rejects_bad_shapes_and_nan() {
        let e = Evaluation::unconstrained(1.0, vec![0.0]);
        assert!(e.validate(1, 0).is_ok());
        assert!(e.validate(2, 0).is_err());
        assert!(e.validate(1, 1).is_err());
        assert!(
            Evaluation::unconstrained(f64::NAN, vec![0.0])
                .validate(1, 0)
                .is_err()
        );
    }
}
