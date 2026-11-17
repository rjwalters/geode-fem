//! Min-max design objectives by the **epigraph** form (Epic #1252 Phase 3,
//! issue #1515).
//!
//! A worst-case figure of merit over a grid of operating points,
//!
//! ```text
//! minimize_x  max_k a_k(x),
//! ```
//!
//! (for the array problem `a_k = |Γ_active(θ_k, φ_k, f_k)|²`) is non-smooth.
//! The epigraph form adds one design variable `t` and solves the smooth
//! problem
//!
//! ```text
//! minimize t   subject to   a_k(x) − t ≤ 0   (k = 1..K),
//! ```
//!
//! with the design vector `[x₀..x_{n−1}, t]`. [`Mma`](crate::Mma) takes the
//! `K` constraints as they are; no MMA change is needed. A p-norm would
//! smear the worst point and need a tuned `p`; the epigraph keeps the worst
//! point exact.
//!
//! **Adjoint reuse.** [`MinMaxPoints::values`] is the cheap forward pass
//! over the whole grid (one factorization per point for the Floquet cell).
//! [`MinMaxPoints::gradient`] is the expensive adjoint pass. [`Epigraph`]
//! calls it only for the *near-active* points, those with
//! `a_k ≥ max(t, max_j a_j) − margin`. A point outside that band gets a
//! Jacobian row of `(0, …, 0, −1)`: it is flat in `x`, and still moves with
//! `t`. The default margin is `∞` (every point gets its gradient), which is
//! exact. A finite margin is a **trade**: the MMA linearization treats a
//! skipped point as constant in `x` for that iteration, so it can be
//! violated by a step that raises it above the band. The optimizer then
//! sees it as active at the next evaluation.

use crate::objective::{EvalError, Evaluation, Objective};

/// The grid of operating points of a min-max problem. `a_k(x) ≥ 0` is the
/// per-point figure that is to be pushed down (for example `|Γ|²`).
pub trait MinMaxPoints {
    /// The number of design variables `n` (without the epigraph variable).
    fn dim(&self) -> usize;

    /// The number of grid points `K`.
    fn n_points(&self) -> usize;

    /// `a_k(x)` for every point.
    ///
    /// # Errors
    ///
    /// The forward solve failed at `x`.
    fn values(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError>;

    /// `∇ₓ a_k(x)` (length `n`) for one point.
    ///
    /// # Errors
    ///
    /// The adjoint solve failed at `x`.
    fn gradient(&mut self, k: usize, x: &[f64]) -> Result<Vec<f64>, EvalError>;
}

/// The epigraph [`Objective`] of a [`MinMaxPoints`] problem. The design
/// vector is `[x, t]`: `dim() = n + 1`, `n_constraints() = K`.
pub struct Epigraph<P> {
    points: P,
    margin: f64,
    n_gradients: usize,
}

impl<P: MinMaxPoints> Epigraph<P> {
    /// An epigraph objective that computes the gradient of every point.
    #[must_use]
    pub fn new(points: P) -> Self {
        Self {
            points,
            margin: f64::INFINITY,
            n_gradients: 0,
        }
    }

    /// Computes the gradient only for points with
    /// `a_k ≥ max(t, max_j a_j) − margin` (see the module docs).
    #[must_use]
    pub fn with_active_margin(mut self, margin: f64) -> Self {
        self.margin = margin;
        self
    }

    /// The number of per-point gradients (adjoint solves) computed so far.
    pub fn n_gradients(&self) -> usize {
        self.n_gradients
    }

    /// The wrapped grid.
    pub fn points(&self) -> &P {
        &self.points
    }

    /// The starting design `[x, max_k a_k(x)]`: the tightest feasible `t`.
    ///
    /// # Errors
    ///
    /// The forward pass failed at `x`.
    pub fn initial_design(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError> {
        let a = self.points.values(x)?;
        let mut d = x.to_vec();
        d.push(a.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v)));
        Ok(d)
    }
}

impl<P: MinMaxPoints> Objective for Epigraph<P> {
    fn dim(&self) -> usize {
        self.points.dim() + 1
    }

    fn n_constraints(&self) -> usize {
        self.points.n_points()
    }

    fn evaluate(&mut self, d: &[f64]) -> Result<Evaluation, EvalError> {
        let n = self.points.dim();
        if d.len() != n + 1 {
            return Err(EvalError::new(format!(
                "epigraph design has {} entries, expected {}",
                d.len(),
                n + 1
            )));
        }
        let (x, t) = (&d[..n], d[n]);
        let a = self.points.values(x)?;
        if a.len() != self.points.n_points() {
            return Err(EvalError::new(format!(
                "values() returned {} points, expected {}",
                a.len(),
                self.points.n_points()
            )));
        }
        let worst = a.iter().fold(f64::NEG_INFINITY, |m, &v| m.max(v));
        let cut = t.max(worst) - self.margin;
        let mut jac = Vec::with_capacity(a.len());
        for (k, &ak) in a.iter().enumerate() {
            let mut row = if ak >= cut {
                self.n_gradients += 1;
                let g = self.points.gradient(k, x)?;
                if g.len() != n {
                    return Err(EvalError::new(format!(
                        "gradient of point {k} has {} entries, the design has {n}",
                        g.len()
                    )));
                }
                g
            } else {
                vec![0.0; n]
            };
            row.push(-1.0);
            jac.push(row);
        }
        let mut grad = vec![0.0; n];
        grad.push(1.0);
        let g: Vec<f64> = a.iter().map(|&ak| ak - t).collect();
        Ok(Evaluation::constrained(t, grad, g, jac))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Bounds, Mma, Optimizer, check_gradient};

    /// `a₁ = (x−1)²`, `a₂ = 4(x+½)²`: they cross at `x = 0` where both are 1,
    /// and the max over the pair is minimized there.
    struct Pair {
        calls: usize,
    }
    impl MinMaxPoints for Pair {
        fn dim(&self) -> usize {
            1
        }
        fn n_points(&self) -> usize {
            2
        }
        fn values(&mut self, x: &[f64]) -> Result<Vec<f64>, EvalError> {
            Ok(vec![(x[0] - 1.0).powi(2), 4.0 * (x[0] + 0.5).powi(2)])
        }
        fn gradient(&mut self, k: usize, x: &[f64]) -> Result<Vec<f64>, EvalError> {
            self.calls += 1;
            Ok(vec![if k == 0 {
                2.0 * (x[0] - 1.0)
            } else {
                8.0 * (x[0] + 0.5)
            }])
        }
    }

    #[test]
    fn epigraph_jacobian_matches_fd_of_the_constraints() {
        let mut e = Epigraph::new(Pair { calls: 0 });
        let d = [0.3, 2.0];
        let ev = e.evaluate(&d).unwrap();
        for k in 0..2 {
            for i in 0..2 {
                let h = 1e-6;
                let mut p = d;
                p[i] += h;
                let gp = e.evaluate(&p).unwrap().constraints[k];
                p[i] -= 2.0 * h;
                let gm = e.evaluate(&p).unwrap().constraints[k];
                let fd = (gp - gm) / (2.0 * h);
                assert!((ev.jacobian[k][i] - fd).abs() < 1e-6, "J[{k}][{i}]");
            }
        }
        assert!(check_gradient(&mut e, &d, 1e-6).unwrap().max_rel_error < 1e-9);
    }

    #[test]
    fn mma_finds_the_crossing_of_the_two_points() {
        let mut e = Epigraph::new(Pair { calls: 0 });
        let d0 = e.initial_design(&[0.8]).unwrap();
        let bounds = Bounds::new(vec![-1.0, 0.0], vec![2.0, 10.0]).unwrap();
        let st = Mma::default().minimize(&mut e, &d0, &bounds).unwrap();
        assert!(
            st.progress.status.is_converged(),
            "{:?}",
            st.progress.status
        );
        assert!(st.x()[0].abs() < 1e-4, "x = {}", st.x()[0]);
        assert!((st.x()[1] - 1.0).abs() < 1e-4, "t = {}", st.x()[1]);
    }

    #[test]
    fn a_finite_margin_skips_inactive_gradients_only() {
        let mut full = Epigraph::new(Pair { calls: 0 });
        let mut cut = Epigraph::new(Pair { calls: 0 }).with_active_margin(0.5);
        // x = 0.9: a = (0.01, 7.84). Only point 2 is near-active.
        let d = [0.9, 7.84];
        let a = full.evaluate(&d).unwrap();
        let b = cut.evaluate(&d).unwrap();
        assert_eq!(full.n_gradients(), 2);
        assert_eq!(cut.n_gradients(), 1);
        assert_eq!(a.constraints, b.constraints);
        assert_eq!(b.jacobian[0], vec![0.0, -1.0]);
        assert_eq!(a.jacobian[1], b.jacobian[1]);
    }

    #[test]
    fn a_wrong_sized_gradient_is_a_typed_failure() {
        struct Bad;
        impl MinMaxPoints for Bad {
            fn dim(&self) -> usize {
                2
            }
            fn n_points(&self) -> usize {
                1
            }
            fn values(&mut self, _: &[f64]) -> Result<Vec<f64>, EvalError> {
                Ok(vec![1.0])
            }
            fn gradient(&mut self, _: usize, _: &[f64]) -> Result<Vec<f64>, EvalError> {
                Ok(vec![0.0])
            }
        }
        assert!(Epigraph::new(Bad).evaluate(&[0.0, 0.0, 1.0]).is_err());
    }
}
