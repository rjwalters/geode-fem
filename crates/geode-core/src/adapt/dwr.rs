//! Dual-weighted-residual (DWR) goal-oriented error estimation core
//! (Epic #835 Phase 4, issue #1266, step 1).
//!
//! For a goal `J` of the discrete solution `x_h` of `A x = b`, with the dual
//! (adjoint) solution `z` of `Aᵀ z = ∂J/∂x` (the [`GoalFunctional::rhs`](crate::adapt::goal::GoalFunctional::rhs) of
//! [`crate::adapt::goal`]), the error identity for a linear goal is
//!
//! ```text
//! J(x) − J(x_h) = zᵀ (b − A x_h) = Σ_T ρ_T · ω_T ,
//! ```
//!
//! where `ρ_T` is the cell residual and `ω_T` the dual weight (`z − I_h z`
//! in the finite-element setting). For a nonlinear goal
//! ([`GoalFunctional::is_linear`](crate::adapt::goal::GoalFunctional::is_linear) is `false`) the identity is a first-order
//! linearised estimate.
//!
//! # Estimators and honesty about reliability
//!
//! Every [`DwrEstimate`] carries the [`DwrEstimator`] that produced it:
//!
//! - [`DwrEstimator::P2Enrichment`] (`"dwr-p2"`): `ω_T` from a p=2
//!   enrichment of `z` on the same mesh. Signed, and the identity above
//!   holds with the enriched weight. **Not implemented yet** (step 2 of
//!   #1266); the variant fixes the report label.
//! - [`DwrEstimator::Product`] (`"dwr-product"`): the product indicator
//!   `η_T(E_h) · η_T(z)` of two energy-norm estimates
//!   ([`crate::adapt::estimator`]). It is a **heuristic**: it is unsigned,
//!   carries no proof of reliability and gives a ranking of cells, not a
//!   signed error value. [`DwrEstimator::is_heuristic`] is `true` and
//!   [`DwrEstimate::summary`] says so.
//!
//! # What is not here
//!
//! The `dwr-p2` weights, the S-parameter and eigenfrequency goals, the
//! adaptive-loop hook and the `driven/adjoint.rs` bridge are later steps of
//! #1266.

use faer::c64;

use crate::adapt::estimator::ErrorEstimate;

/// Which weight construction produced a [`DwrEstimate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DwrEstimator {
    /// `ω_T` from a p=2 enrichment of the dual solution on the same mesh.
    P2Enrichment,
    /// The heuristic product indicator `η_T(E_h) · η_T(z)`.
    Product,
}

impl DwrEstimator {
    /// The stable report label: `"dwr-p2"` or `"dwr-product"`.
    pub fn label(self) -> &'static str {
        match self {
            Self::P2Enrichment => "dwr-p2",
            Self::Product => "dwr-product",
        }
    }

    /// `true` when the estimator has no reliability argument (the product
    /// fallback).
    pub fn is_heuristic(self) -> bool {
        matches!(self, Self::Product)
    }
}

/// Errors from the DWR core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DwrError {
    /// Two inputs that must be parallel have different lengths.
    LengthMismatch {
        what: &'static str,
        expected: usize,
        got: usize,
    },
    /// A DOF is assigned to a cell index `>= n_cells`.
    CellOutOfRange {
        dof: usize,
        cell: usize,
        n_cells: usize,
    },
}

impl std::fmt::Display for DwrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch {
                what,
                expected,
                got,
            } => write!(f, "dwr: {what} has length {got}, expected {expected}"),
            Self::CellOutOfRange { dof, cell, n_cells } => {
                write!(
                    f,
                    "dwr: DOF {dof} assigned to cell {cell} >= n_cells {n_cells}"
                )
            }
        }
    }
}

impl std::error::Error for DwrError {}

/// A goal-oriented error estimate: a per-cell indicator and a global value.
#[derive(Debug, Clone)]
pub struct DwrEstimate {
    /// Which construction produced this estimate.
    pub estimator: DwrEstimator,
    /// Per-cell indicator `η_T^goal ≥ 0`, the ranking a marking strategy
    /// consumes. `|ρ_T ω_T|` for signed terms, `η_T(E_h) η_T(z)` for the
    /// product fallback.
    pub eta_t: Vec<f64>,
    /// Signed per-cell terms `ρ_T ω_T`; `None` for the unsigned product
    /// fallback.
    pub terms: Option<Vec<c64>>,
    /// The global signed estimate `Σ_T ρ_T ω_T` of `J(x) − J(x_h)`; `None`
    /// for the unsigned product fallback.
    pub signed: Option<c64>,
}

impl DwrEstimate {
    /// `Σ_T η_T^goal`: an unsigned magnitude that is meaningful for every
    /// estimator (an upper bound on `|signed|` when `signed` exists).
    pub fn eta_sum(&self) -> f64 {
        self.eta_t.iter().sum()
    }

    /// One-line report that always names the estimator and flags a
    /// heuristic one.
    pub fn summary(&self) -> String {
        let tag = if self.estimator.is_heuristic() {
            " (HEURISTIC: no reliability guarantee, ranking only)"
        } else {
            ""
        };
        match self.signed {
            Some(s) => format!(
                "estimator: \"{}\"{tag}; J−J_h ≈ {:.6e}{:+.6e}i; Ση_T = {:.6e}",
                self.estimator.label(),
                s.re,
                s.im,
                self.eta_sum()
            ),
            None => format!(
                "estimator: \"{}\"{tag}; Ση_T = {:.6e}",
                self.estimator.label(),
                self.eta_sum()
            ),
        }
    }
}

/// Signed DWR estimate from a residual and a dual weight, both indexed by
/// DOF, with every DOF owned by exactly one cell (`owner[i]`).
///
/// `ρ_T ω_T = Σ_{i ∈ T} residual_i · weight_i` (no conjugation: the pairing
/// is the complex-bilinear one of the complex-symmetric operator). With the
/// exact dual `weight = z` and `residual = b − A x_h` of a linear goal the
/// sum over cells equals `J(x) − J(x_h)` exactly.
///
/// # Errors
///
/// [`DwrError`] on mismatched lengths or an owner index `>= n_cells`.
pub fn dwr_from_residual_and_weight(
    residual: &[c64],
    weight: &[c64],
    owner: &[usize],
    n_cells: usize,
    estimator: DwrEstimator,
) -> Result<DwrEstimate, DwrError> {
    for (what, len) in [("weight", weight.len()), ("owner", owner.len())] {
        if len != residual.len() {
            return Err(DwrError::LengthMismatch {
                what,
                expected: residual.len(),
                got: len,
            });
        }
    }
    let mut terms = vec![c64::new(0.0, 0.0); n_cells];
    for (i, &c) in owner.iter().enumerate() {
        if c >= n_cells {
            return Err(DwrError::CellOutOfRange {
                dof: i,
                cell: c,
                n_cells,
            });
        }
        terms[c] += residual[i] * weight[i];
    }
    let signed = terms.iter().fold(c64::new(0.0, 0.0), |a, t| a + t);
    let eta_t = terms.iter().map(|t| t.norm()).collect();
    Ok(DwrEstimate {
        estimator,
        eta_t,
        terms: Some(terms),
        signed: Some(signed),
    })
}

/// The heuristic product fallback `η_T^goal = η_T(E_h) · η_T(z)` from the
/// energy-norm estimates of the primal and the dual solution on the same
/// mesh. Labelled [`DwrEstimator::Product`] (`"dwr-product"`); unsigned.
///
/// # Errors
///
/// [`DwrError::LengthMismatch`] if the two estimates cover different tet
/// counts.
pub fn dwr_product(primal: &ErrorEstimate, dual: &ErrorEstimate) -> Result<DwrEstimate, DwrError> {
    if primal.eta_t2.len() != dual.eta_t2.len() {
        return Err(DwrError::LengthMismatch {
            what: "dual estimate",
            expected: primal.eta_t2.len(),
            got: dual.eta_t2.len(),
        });
    }
    let eta_t = primal
        .eta_t2
        .iter()
        .zip(&dual.eta_t2)
        .map(|(p, d)| p.max(0.0).sqrt() * d.max(0.0).sqrt())
        .collect();
    Ok(DwrEstimate {
        estimator: DwrEstimator::Product,
        eta_t,
        terms: None,
        signed: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapt::goal::{GoalFunctional, LinearGoal};

    fn c(re: f64, im: f64) -> c64 {
        c64::new(re, im)
    }

    /// Dense complex solve by Gaussian elimination with partial pivoting
    /// (test-only, tiny systems).
    #[allow(clippy::needless_range_loop)]
    fn solve(a: &[Vec<c64>], b: &[c64]) -> Vec<c64> {
        let n = b.len();
        let mut m: Vec<Vec<c64>> = a
            .iter()
            .zip(b)
            .map(|(r, &bi)| {
                let mut r = r.clone();
                r.push(bi);
                r
            })
            .collect();
        for k in 0..n {
            let p = (k..n)
                .max_by(|&i, &j| m[i][k].norm().total_cmp(&m[j][k].norm()))
                .unwrap();
            m.swap(k, p);
            for i in k + 1..n {
                let f = m[i][k] / m[k][k];
                for j in k..=n {
                    let t = m[k][j];
                    m[i][j] -= f * t;
                }
            }
        }
        let mut x = vec![c(0.0, 0.0); n];
        for i in (0..n).rev() {
            let mut s = m[i][n];
            for j in i + 1..n {
                s -= m[i][j] * x[j];
            }
            x[i] = s / m[i][i];
        }
        x
    }

    fn matvec(a: &[Vec<c64>], x: &[c64]) -> Vec<c64> {
        a.iter()
            .map(|r| r.iter().zip(x).fold(c(0.0, 0.0), |s, (m, v)| s + m * v))
            .collect()
    }

    /// A complex-symmetric 6×6 system with a known exact answer.
    #[allow(clippy::needless_range_loop)]
    fn system() -> (Vec<Vec<c64>>, Vec<c64>) {
        let n = 6;
        let mut a = vec![vec![c(0.0, 0.0); n]; n];
        for i in 0..n {
            a[i][i] = c(4.0 + i as f64, 0.5);
            for j in i + 1..n {
                let v = c(1.0 / (1.0 + (j - i) as f64), 0.1 * (i + j) as f64);
                a[i][j] = v;
                a[j][i] = v; // complex-symmetric, not Hermitian
            }
        }
        let b: Vec<c64> = (0..n).map(|i| c(1.0 + i as f64, -0.3 * i as f64)).collect();
        (a, b)
    }

    /// Linear goal with a known answer: with the exact dual `Aᵀz = w` and a
    /// perturbed `x_h`, `Σ_T ρ_T ω_T` equals `J(x) − J(x_h)` to rounding, for
    /// any partition of the DOFs into cells.
    #[test]
    fn linear_goal_signed_estimate_is_exact_on_a_known_system() {
        let (a, b) = system();
        let x = solve(&a, &b);
        let w: Vec<c64> = (0..6)
            .map(|i| c(0.5 + i as f64, 1.0 - 0.2 * i as f64))
            .collect();
        let goal = LinearGoal::new("probe", w);
        // Perturbed "discrete" solution.
        let xh: Vec<c64> = x
            .iter()
            .enumerate()
            .map(|(i, v)| v + c(0.05 * (i as f64 + 1.0), -0.02 * i as f64))
            .collect();
        // Dual: Aᵀ z = rhs; A is complex-symmetric so Aᵀ = A.
        let z = solve(&a, &goal.rhs(&xh));
        let ax = matvec(&a, &xh);
        let residual: Vec<c64> = b.iter().zip(&ax).map(|(bi, v)| bi - v).collect();
        let owner = [0usize, 0, 1, 1, 2, 2];
        let est =
            dwr_from_residual_and_weight(&residual, &z, &owner, 3, DwrEstimator::P2Enrichment)
                .unwrap();
        let exact = goal.value(&x) - goal.value(&xh);
        let got = est.signed.unwrap();
        assert!(
            (got - exact).norm() <= 1e-12 * exact.norm(),
            "{got} vs {exact}"
        );
        assert_eq!(est.eta_t.len(), 3);
        assert!(est.eta_sum() >= got.norm() - 1e-15, "Ση_T bounds |signed|");
        let terms = est.terms.unwrap();
        for (t, e) in terms.iter().zip(&est.eta_t) {
            assert_eq!(t.norm(), *e);
        }
    }

    #[test]
    fn bad_inputs_are_rejected() {
        let r = vec![c(1.0, 0.0); 3];
        let e = dwr_from_residual_and_weight(&r, &r[..2], &[0, 0, 0], 1, DwrEstimator::Product);
        assert!(matches!(
            e,
            Err(DwrError::LengthMismatch { what: "weight", .. })
        ));
        let e = dwr_from_residual_and_weight(&r, &r, &[0, 0, 5], 2, DwrEstimator::Product);
        assert!(matches!(e, Err(DwrError::CellOutOfRange { dof: 2, .. })));
    }

    /// Negative control: the product fallback must be labelled heuristic in
    /// both the estimator label and the report line. Dropping either fails
    /// this test.
    #[test]
    fn product_fallback_is_labelled_heuristic() {
        assert_eq!(DwrEstimator::Product.label(), "dwr-product");
        assert!(DwrEstimator::Product.is_heuristic());
        assert_eq!(DwrEstimator::P2Enrichment.label(), "dwr-p2");
        assert!(!DwrEstimator::P2Enrichment.is_heuristic());
        let est = DwrEstimate {
            estimator: DwrEstimator::Product,
            eta_t: vec![1.0, 2.0],
            terms: None,
            signed: None,
        };
        let s = est.summary();
        assert!(s.contains("dwr-product"), "{s}");
        assert!(s.contains("HEURISTIC"), "{s}");
        let p2 = DwrEstimate {
            estimator: DwrEstimator::P2Enrichment,
            eta_t: vec![1.0],
            terms: Some(vec![c(1.0, 0.0)]),
            signed: Some(c(1.0, 0.0)),
        };
        assert!(!p2.summary().contains("HEURISTIC"));
    }
}
