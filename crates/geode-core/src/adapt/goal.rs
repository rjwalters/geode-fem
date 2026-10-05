//! The goal-functional interface shared by goal-oriented error estimation
//! (Epic #835 Phase 4, DWR) and the differentiable-EDA epic (#841).
//!
//! # Why a shared trait
//!
//! A scalar quantity of interest `J(x)` of the discrete solution `x` (an
//! S-parameter, a port impedance, an eigenfrequency, a field energy) needs
//! the same two things in both epics:
//!
//! - its **value** `J(x)`, for reporting and line searches;
//! - its **derivative** `∂J/∂x`, which is the right-hand side of the adjoint
//!   (dual) problem `A(ω)ᵀ z = ∂J/∂x`.
//!
//! The design gradient (#841) contracts `z` with `∂A/∂p`. The DWR estimate
//! (#835 Phase 4) weights the cell residuals of
//! [`crate::adapt::estimator`] by `z − I_h z`. One adjoint solve therefore
//! serves both. Implement this trait once per quantity and both epics can
//! consume it.
//!
//! # Conventions
//!
//! - `x` is the full-length DOF vector of the
//!   [`crate::assembly::hcurl_space::HcurlSpace`] it was solved in (PEC DOFs
//!   are exact zeros), so a goal is order-generic: it never assumes the p=1
//!   `n_edges` layout.
//! - [`GoalFunctional::rhs`] is the **complex-linear** (holomorphic)
//!   derivative `∂J/∂x_i`, not a Wirtinger conjugate derivative. A goal
//!   that is not holomorphic in `x` (for example `|S₁₁|²`) must document the
//!   convention it uses instead.
//! - The adjoint system is `A(ω)ᵀ z = rhs`. The driven operator is
//!   complex-symmetric, so `Aᵀ = A` and the adjoint is a back-solve on the
//!   forward factorization. That is **not** assumed by this trait: with a
//!   Bloch phase (#837) `A_r(k)ᵀ = A_r(−k) ≠ A_r(k)`, and the consumer must
//!   then use a transpose solve (#841's sibling note).
//!
//! # What is not here
//!
//! DWR itself (cell residual × dual weight), the S-parameter / Z₀ /
//! eigenfrequency goals and the adjoint-solution type are Phase 4 and #842
//! work. [`LinearGoal`] is the reference implementation the trait's
//! contract is tested on.

use faer::c64;

/// A scalar quantity of interest of a discrete H(curl) solution (see the
/// [module docs](self)).
pub trait GoalFunctional {
    /// A short stable identifier for reports (for example `"s11"`).
    fn name(&self) -> &str;

    /// The goal value `J(x)` for the full-length DOF vector `x`.
    fn value(&self, x: &[c64]) -> c64;

    /// The complex-linear derivative `∂J/∂x` at `x`: the right-hand side of
    /// the adjoint problem `A(ω)ᵀ z = ∂J/∂x`. Same length as `x`. A linear
    /// goal returns its weight vector regardless of `x`.
    fn rhs(&self, x: &[c64]) -> Vec<c64>;

    /// Whether `J` is linear in `x`. A linear goal's DWR identity
    /// `J(E) − J(E_h) = ⟨residual, z⟩` is exact; a nonlinear goal's is a
    /// first-order (linearised) estimate. Defaults to `false`, the
    /// conservative answer.
    fn is_linear(&self) -> bool {
        false
    }
}

/// The linear goal `J(x) = Σ_i w_i x_i` (no conjugation), the reference
/// implementation of [`GoalFunctional`]. A port voltage line integral, a
/// field probe or a projection onto a fixed mode are all of this form.
#[derive(Debug, Clone)]
pub struct LinearGoal {
    name: String,
    weights: Vec<c64>,
}

impl LinearGoal {
    /// A linear goal with the given name and weight vector (one weight per
    /// DOF of the space the solution lives in).
    pub fn new(name: impl Into<String>, weights: Vec<c64>) -> Self {
        Self {
            name: name.into(),
            weights,
        }
    }

    /// The weight vector `w`.
    pub fn weights(&self) -> &[c64] {
        &self.weights
    }
}

impl GoalFunctional for LinearGoal {
    fn name(&self) -> &str {
        &self.name
    }

    /// # Panics
    ///
    /// Panics if `x.len()` differs from the weight vector's length (a
    /// solution from another space is a programmer error).
    fn value(&self, x: &[c64]) -> c64 {
        assert_eq!(
            x.len(),
            self.weights.len(),
            "goal `{}`: DOF vector length {} != weight length {}",
            self.name,
            x.len(),
            self.weights.len()
        );
        self.weights
            .iter()
            .zip(x)
            .fold(c64::new(0.0, 0.0), |acc, (w, v)| acc + w * v)
    }

    fn rhs(&self, _x: &[c64]) -> Vec<c64> {
        self.weights.clone()
    }

    fn is_linear(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `rhs` is the derivative of `value`: a complex directional finite
    /// difference reproduces `rhs · d` (exactly, since the goal is linear).
    #[test]
    fn linear_goal_rhs_is_the_derivative_of_value() {
        let w: Vec<c64> = (0..5)
            .map(|i| c64::new(1.0 + i as f64, -0.5 * i as f64))
            .collect();
        let goal = LinearGoal::new("probe", w.clone());
        let x: Vec<c64> = (0..5).map(|i| c64::new(0.3 * i as f64, 1.0)).collect();
        let d: Vec<c64> = (0..5).map(|i| c64::new(-1.0, 0.2 * i as f64)).collect();
        let t = c64::new(0.0, 1e-3);
        let xp: Vec<c64> = x.iter().zip(&d).map(|(a, b)| a + t * b).collect();
        let fd = (goal.value(&xp) - goal.value(&x)) / t;
        let g = goal.rhs(&x);
        let an = g
            .iter()
            .zip(&d)
            .fold(c64::new(0.0, 0.0), |acc, (a, b)| acc + a * b);
        assert!((fd - an).norm() <= 1e-10 * an.norm());
        assert!(goal.is_linear());
        assert_eq!(goal.name(), "probe");
        assert_eq!(goal.weights(), w.as_slice());
    }
}
