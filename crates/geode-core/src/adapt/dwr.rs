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
//!   holds with the enriched weight. Implemented by [`dwr_p2`]: one extra
//!   p=2 assembly + factorization of the driven operator on the same mesh,
//!   the p=1 solution prolonged hierarchically into the p=2 space, the
//!   p=2 residual `b − A₂ E_h` and the p=2 dual `A₂ᵀ z = ∂J/∂x`.
//! - [`DwrEstimator::Product`] (`"dwr-product"`): the product indicator
//!   `η_T(E_h) · η_T(z)` of two energy-norm estimates
//!   ([`crate::adapt::estimator`]). It is a **heuristic**: it is unsigned,
//!   carries no proof of reliability and gives a ranking of cells, not a
//!   signed error value. [`DwrEstimator::is_heuristic`] is `true` and
//!   [`DwrEstimate::summary`] says so.
//!
//! # What is not here
//!
//! The eigenfrequency goal lives in [`crate::adapt::dwr_eigen`]. The
//! S-parameter goals, the adaptive-loop hook and the `driven/adjoint.rs`
//! bridge landed in later steps of #1266.

use faer::c64;

use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::solve::DrivenOperator;
use crate::elements::ElementOrder;

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
    /// [`dwr_p2`] was given a space or operator that is not the p=2 one on
    /// matching DOFs.
    NotP2Enrichment(&'static str),
    /// The p=2 operator factorization or a back-substitution failed.
    Driven(String),
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
            Self::NotP2Enrichment(why) => write!(f, "dwr-p2: {why}"),
            Self::Driven(e) => write!(f, "dwr-p2: p=2 operator failed: {e}"),
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

/// Like [`dwr_from_residual_and_weight`], but for DOFs shared by several
/// cells: the product `residual_i · weight_i` of DOF `i` is split equally
/// among the `m_i` cells that contain it (`cell_dofs(c)` lists cell `c`'s
/// DOFs), so the cell terms still sum exactly to `Σ_i residual_i · weight_i`.
///
/// This is the standard partition-of-unity attribution for edge/face DOFs;
/// it is a localisation choice, not a change to the global estimate.
///
/// # Errors
///
/// [`DwrError::LengthMismatch`] if `weight` and `residual` differ in length;
/// [`DwrError::CellOutOfRange`] if a cell lists a DOF `>= residual.len()`.
pub fn dwr_from_residual_and_weight_shared<'a>(
    residual: &[c64],
    weight: &[c64],
    n_cells: usize,
    cell_dofs: impl Fn(usize) -> &'a [u32],
    estimator: DwrEstimator,
) -> Result<DwrEstimate, DwrError> {
    if weight.len() != residual.len() {
        return Err(DwrError::LengthMismatch {
            what: "weight",
            expected: residual.len(),
            got: weight.len(),
        });
    }
    let n = residual.len();
    let mut mult = vec![0u32; n];
    for c in 0..n_cells {
        for &d in cell_dofs(c) {
            let d = d as usize;
            if d >= n {
                return Err(DwrError::CellOutOfRange {
                    dof: d,
                    cell: c,
                    n_cells,
                });
            }
            mult[d] += 1;
        }
    }
    let mut terms = vec![c64::new(0.0, 0.0); n_cells];
    for (c, term) in terms.iter_mut().enumerate() {
        for &d in cell_dofs(c) {
            let d = d as usize;
            *term += residual[d] * weight[d] * (1.0 / f64::from(mult[d]));
        }
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

/// Inputs of the p=2 enrichment: a p=2 driven operator on the same mesh as
/// the p=1 solution being judged.
#[derive(Clone, Copy)]
pub struct P2Enrichment<'a> {
    /// The p=2 [`HcurlSpace`] of the mesh the p=1 solution lives on.
    pub space: &'a HcurlSpace,
    /// The driven operator assembled on [`Self::space`] with the same
    /// materials, PEC mask, ports, surfaces and source as the p=1 problem.
    pub op: &'a DrivenOperator,
    /// Angular frequency of the solve.
    pub omega: f64,
    /// The excited port for an N-port sweep, or `None` for the baked source.
    pub excited: Option<usize>,
}

/// Goal-oriented estimate with `ω_T` from a **p=2 enrichment** of the dual
/// (`"dwr-p2"`).
///
/// - `x_h_p1`: the p=1 solution (`[n_edges]`, `mesh.edges()` order);
/// - `dual_rhs_p2`: `∂J/∂x` as a full-length p=2 DOF vector (the goal's
///   [`rhs`](crate::adapt::goal::GoalFunctional::rhs) expressed on the p=2
///   space, so the goal sees the enriched DOFs too).
///
/// The p=1 solution is prolonged hierarchically
/// ([`HcurlSpace::prolong_p1`]); the residual is `r = b − A₂ E_h`; the dual
/// solves `A₂ᵀ z = dual_rhs_p2`; the weight is `z` with its Whitney (p=1,
/// `W`) DOFs removed, i.e. `ω = z − I_h z` in the hierarchical basis. The
/// per-cell terms split each shared DOF equally among its cells
/// ([`dwr_from_residual_and_weight_shared`]).
///
/// When `x_h_p1` is the Galerkin solution of the same problem the `W` block
/// of `r` is zero, and for a linear goal `Σ_T ρ_T ω_T = J(E_{p2}) − J(E_h)`
/// is the exact p=2 goal error. The estimate is a *p=2 error* estimate: it
/// captures the p=1 → p=2 gain, not the error of the p=2 solution itself.
///
/// **Complex-symmetric assumption.** The dual is back-solved on the forward
/// factorization (`Aᵀ = A`). That holds for the driven pencil assembled
/// today; a Bloch-periodic (non-symmetric) operator needs a transpose solve
/// instead, as noted in [`crate::adapt::goal`].
///
/// Cost: one p=2 assembly (done by the caller) and one LU of `A₂(ω)`, plus
/// two back-solves; nothing is re-meshed.
///
/// # Errors
///
/// [`DwrError::NotP2Enrichment`] if the space/operator are not p=2 on
/// matching DOF counts or a vector length is wrong; [`DwrError::Driven`] if
/// the factorization or a back-solve fails.
pub fn dwr_p2(
    enr: &P2Enrichment<'_>,
    x_h_p1: &[c64],
    dual_rhs_p2: &[c64],
) -> Result<DwrEstimate, DwrError> {
    let (space, op) = (enr.space, enr.op);
    if space.order() != ElementOrder::P2 || op.order() != ElementOrder::P2 {
        return Err(DwrError::NotP2Enrichment("space and operator must be p=2"));
    }
    if op.n_dofs() != space.n_dofs() {
        return Err(DwrError::NotP2Enrichment(
            "operator and space have different DOF counts",
        ));
    }
    if x_h_p1.len() != space.n_edges() {
        return Err(DwrError::NotP2Enrichment(
            "p=1 solution length != number of edges",
        ));
    }
    if dual_rhs_p2.len() != space.n_dofs() {
        return Err(DwrError::NotP2Enrichment(
            "dual rhs length != number of p=2 DOFs",
        ));
    }
    let fac = op
        .factor_at(enr.omega)
        .map_err(|e| DwrError::Driven(e.to_string()))?;
    let i2f = op.interior_to_full();
    let n_int = i2f.len();
    let zero = c64::new(0.0, 0.0);

    // Residual r = b − A₂ E_h on the interior DOFs.
    let x_full = space.prolong_p1(x_h_p1);
    let x_int: Vec<c64> = i2f.iter().map(|&f| x_full[f]).collect();
    let mut ax = vec![zero; n_int];
    fac.spmv_a(&x_int, &mut ax);
    let b_int = op.assemble_b_at(enr.omega, enr.excited);
    let mut residual = vec![zero; space.n_dofs()];
    for (k, &f) in i2f.iter().enumerate() {
        residual[f] = b_int[k] - ax[k];
    }

    // Dual z = A₂⁻ᵀ g (A₂ᵀ = A₂), then remove the Whitney part.
    let g_int: Vec<c64> = i2f.iter().map(|&f| dual_rhs_p2[f]).collect();
    let mut z_int = vec![zero; n_int];
    fac.back_solve(&g_int, &mut z_int)
        .map_err(|e| DwrError::Driven(e.to_string()))?;
    let mut weight = vec![zero; space.n_dofs()];
    for (k, &f) in i2f.iter().enumerate() {
        weight[f] = z_int[k];
    }
    for ge in 0..space.n_edges() {
        weight[space.edge_dofs(ge)[0] as usize] = zero;
    }

    dwr_from_residual_and_weight_shared(
        &residual,
        &weight,
        space.n_tets(),
        |t| space.tet_dofs(t),
        DwrEstimator::P2Enrichment,
    )
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
