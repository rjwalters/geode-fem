//! Eigenfrequency goal-oriented error estimation (Epic #835 Phase 4, issue
//! #1266, step 5).
//!
//! The goal is the eigenvalue `λ = k²` of a mode of `K x = λ M x` (the
//! eigenproblem is self-adjoint when lossless and complex-symmetric when
//! lossy, so the left eigenvector equals the right one and **the dual of the
//! eigenvalue goal is the eigenvector itself**; no separate adjoint solve).
//!
//! # Estimate
//!
//! With `x_h` the p=1 eigenvector scaled so `x_hᵀ M x_h = 1` (bilinear
//! pairing, no conjugation) and `r = (K − λ_h M) x_h` the residual, the DWR
//! identity for the Rayleigh quotient is
//!
//! ```text
//! λ − λ_h ≈ Σ_T ρ_T ω_T ,   ρ_T ω_T = Σ_{i ∈ T} r_i ω_i ,   ω = x̂ ,
//! ```
//!
//! where `x̂` is the **p=2 enrichment of the eigenvector**: one step of
//! inverse iteration `x̂ = y / (x_hᵀ M y)`, `(K₂ − λ_h M₂) y = M₂ x_h`, in the
//! p=2 space on the same mesh with `x_h` prolonged hierarchically. Because
//! `x_h` is Galerkin, `r` has no Whitney component, so only the hierarchical
//! DOFs of `x̂` contribute (`ω = x̂ − I_h x̂`). The global value equals
//! `rᵀ x̂ = λ_{p2} − λ_h` (the p=1 → p=2 eigenvalue gain, one extra p=2
//! assembly and LU per estimate), localised by the partition-of-unity split
//! of [`dwr_from_residual_and_weight_shared`]. It is labelled
//! [`DwrEstimator::P2Enrichment`] (`"dwr-p2"`).
//!
//! # Re k and Q
//!
//! [`eigen_goal_error`] converts an eigenvalue error to `k = √λ`,
//! `Re k` and `Q = Re k / (2 |Im k|)` by evaluating the map at `λ_h` and at
//! `λ_h + Δλ` (first-order accurate, valid for complex `λ_h` too). A lossless
//! mode has `Im k = 0`, so `Q` is `None` (infinite).
//!
//! # Scope
//!
//! [`dwr_eigen_p2`] takes a real `λ_h` because [`DrivenOperator`] factors at a
//! real `ω`; a lossy mode with complex `λ_h` can use [`eigen_goal_error`] on
//! a `Δλ` from its own enrichment. The pairing is the bilinear one of a
//! complex-symmetric operator; a Bloch-phase operator needs a transpose
//! solve (see [`crate::adapt::goal`]).

use faer::c64;

use crate::adapt::dwr::{DwrError, DwrEstimate, DwrEstimator, dwr_from_residual_and_weight_shared};
use crate::assembly::hcurl_space::HcurlSpace;
use crate::driven::solve::DrivenOperator;
use crate::eigen::complex::spmv;
use crate::elements::ElementOrder;

/// An eigenvalue-goal error estimate and its `Re k` / `Q` images.
#[derive(Debug, Clone)]
pub struct EigenDwrEstimate {
    /// The per-cell / global estimate of `λ − λ_h` (`signed`), labelled
    /// `"dwr-p2"`.
    pub dwr: DwrEstimate,
    /// The estimated eigenvalue error `Δλ = λ − λ_h` (`dwr.signed`).
    pub d_lambda: c64,
    /// Its image on `Re k`, `Q`.
    pub goal: EigenGoalError,
}

/// The error of the eigenfrequency goals `Re k` and `Q` implied by an
/// eigenvalue error `Δλ` at `λ_h`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EigenGoalError {
    /// `Δλ = λ − λ_h`.
    pub d_lambda: c64,
    /// `Re k(λ_h + Δλ) − Re k(λ_h)` with `k = √λ`.
    pub d_re_k: f64,
    /// `Q(λ_h + Δλ) − Q(λ_h)` with `Q = Re k / (2 |Im k|)`; `None` when
    /// either mode is lossless (`Im k = 0`, `Q = ∞`).
    pub d_q: Option<f64>,
}

fn quality(k: c64) -> Option<f64> {
    (k.im.abs() > 0.0).then(|| k.re / (2.0 * k.im.abs()))
}

/// `Re k` and `Q` of the eigenvalue `λ = k²` (principal root).
pub fn k_and_q(lambda: c64) -> (f64, Option<f64>) {
    let k = lambda.sqrt();
    (k.re, quality(k))
}

/// Map an eigenvalue error `d_lambda` at `lambda_h` to errors of `Re k` and
/// `Q`.
pub fn eigen_goal_error(lambda_h: c64, d_lambda: c64) -> EigenGoalError {
    let (k0, q0) = k_and_q(lambda_h);
    let (k1, q1) = k_and_q(lambda_h + d_lambda);
    EigenGoalError {
        d_lambda,
        d_re_k: k1 - k0,
        d_q: q0.zip(q1).map(|(a, b)| b - a),
    }
}

/// The eigenfrequency DWR estimate of a p=1 eigenpair from a p=2
/// enrichment (see the [module docs](self)).
///
/// - `space` / `op`: the p=2 [`HcurlSpace`] and the p=2 [`DrivenOperator`]
///   of the same mesh, materials and PEC mask as the eigenproblem
///   (`A(ω) = K − ω² M`, no ports or surfaces);
/// - `x_h`: the p=1 eigenvector over `mesh.edges()` (PEC entries zero), any
///   scaling;
/// - `lambda_h`: its eigenvalue (`ω = √λ_h`).
///
/// # Errors
///
/// [`DwrError::NotP2Enrichment`] for a non-p=2 space/operator, wrong lengths
/// or a non-positive / degenerate eigenpair; [`DwrError::Driven`] if a matrix
/// assembly or the factorization fails.
pub fn dwr_eigen_p2(
    space: &HcurlSpace,
    op: &DrivenOperator,
    x_h: &[c64],
    lambda_h: f64,
) -> Result<EigenDwrEstimate, DwrError> {
    if space.order() != ElementOrder::P2 || op.order() != ElementOrder::P2 {
        return Err(DwrError::NotP2Enrichment("space and operator must be p=2"));
    }
    if op.n_dofs() != space.n_dofs() {
        return Err(DwrError::NotP2Enrichment(
            "operator and space have different DOF counts",
        ));
    }
    if x_h.len() != space.n_edges() {
        return Err(DwrError::NotP2Enrichment(
            "p=1 eigenvector length != number of edges",
        ));
    }
    if !(lambda_h.is_finite() && lambda_h > 0.0) {
        return Err(DwrError::NotP2Enrichment(
            "eigenvalue must be finite and positive",
        ));
    }
    let omega = lambda_h.sqrt();
    let zero = c64::new(0.0, 0.0);
    let i2f = op.interior_to_full();
    let n_int = i2f.len();
    let drv = |e: crate::driven::solve::DrivenError| DwrError::Driven(e.to_string());

    let x_full = space.prolong_p1(x_h);
    let mut x: Vec<c64> = i2f.iter().map(|&f| x_full[f]).collect();

    // M x = (K x − A(ω) x) / λ_h, with K = A(0).
    let k_mat = op.matrix_at(0.0).map_err(drv)?;
    let a_mat = op.matrix_at(omega).map_err(drv)?;
    let mut kx = vec![zero; n_int];
    let mut ax = vec![zero; n_int];
    spmv(k_mat.as_ref(), &x, &mut kx);
    spmv(a_mat.as_ref(), &x, &mut ax);
    let mx = |kx: &[c64], ax: &[c64]| -> Vec<c64> {
        kx.iter()
            .zip(ax)
            .map(|(k, a)| (k - a) * (1.0 / lambda_h))
            .collect()
    };
    let dot = |a: &[c64], b: &[c64]| a.iter().zip(b).fold(zero, |s, (p, q)| s + p * q);

    // Normalise x_hᵀ M x_h = 1.
    let norm2 = dot(&x, &mx(&kx, &ax));
    if !(norm2.norm() > 0.0 && norm2.norm().is_finite()) {
        return Err(DwrError::NotP2Enrichment(
            "eigenvector has zero M-norm (zero vector?)",
        ));
    }
    let s = c64::new(1.0, 0.0) / norm2.sqrt();
    for v in &mut x {
        *v *= s;
    }
    for v in kx.iter_mut().chain(ax.iter_mut()) {
        *v *= s;
    }
    let m_x = mx(&kx, &ax);

    // One inverse-iteration step in the p=2 space: (K₂ − λ_h M₂) y = M₂ x.
    let fac = op.factor_at(omega).map_err(drv)?;
    let mut y = vec![zero; n_int];
    fac.back_solve(&m_x, &mut y).map_err(drv)?;
    let scale = dot(&x, &mx_of(&k_mat, &a_mat, &y, lambda_h));
    if !(scale.norm() > 0.0 && scale.norm().is_finite()) {
        return Err(DwrError::NotP2Enrichment(
            "degenerate enrichment (x_hᵀ M y = 0)",
        ));
    }

    // Residual r = A x (zero on the Whitney rows for a Galerkin pair) and
    // weight ω = x̂ with the Whitney DOFs removed.
    let mut residual = vec![zero; space.n_dofs()];
    let mut weight = vec![zero; space.n_dofs()];
    for (k, &f) in i2f.iter().enumerate() {
        residual[f] = ax[k];
        weight[f] = y[k] / scale;
    }
    for ge in 0..space.n_edges() {
        weight[space.edge_dofs(ge)[0] as usize] = zero;
    }
    let dwr = dwr_from_residual_and_weight_shared(
        &residual,
        &weight,
        space.n_tets(),
        |t| space.tet_dofs(t),
        DwrEstimator::P2Enrichment,
    )?;
    let d_lambda = dwr.signed.unwrap_or(zero);
    Ok(EigenDwrEstimate {
        goal: eigen_goal_error(c64::new(lambda_h, 0.0), d_lambda),
        d_lambda,
        dwr,
    })
}

/// `M v = (K v − A(ω) v) / λ_h` for an interior vector.
fn mx_of(
    k_mat: &faer::sparse::SparseColMat<usize, c64>,
    a_mat: &faer::sparse::SparseColMat<usize, c64>,
    v: &[c64],
    lambda_h: f64,
) -> Vec<c64> {
    let zero = c64::new(0.0, 0.0);
    let mut kv = vec![zero; v.len()];
    let mut av = vec![zero; v.len()];
    spmv(k_mat.as_ref(), v, &mut kv);
    spmv(a_mat.as_ref(), v, &mut av);
    kv.iter()
        .zip(&av)
        .map(|(k, a)| (k - a) * (1.0 / lambda_h))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossless_mode_has_infinite_q() {
        let g = eigen_goal_error(c64::new(4.0, 0.0), c64::new(-0.1, 0.0));
        assert!((g.d_re_k - (3.9f64.sqrt() - 2.0)).abs() < 1e-14);
        assert!(g.d_q.is_none());
    }

    #[test]
    fn lossy_mode_reports_q_and_re_k() {
        // k = 2 + 0.05i  →  Q = 20.
        let k = c64::new(2.0, 0.05);
        let lam = k * k;
        let (re, q) = k_and_q(lam);
        assert!((re - 2.0).abs() < 1e-14);
        assert!((q.unwrap() - 20.0).abs() < 1e-12);
        // Perturb λ so k → 1.99 + 0.0505i (Q = 1.99 / 0.101).
        let k2 = c64::new(1.99, 0.0505);
        let g = eigen_goal_error(lam, k2 * k2 - lam);
        assert!((g.d_re_k + 0.01).abs() < 1e-12);
        assert!((g.d_q.unwrap() - (1.99 / 0.101 - 20.0)).abs() < 1e-9);
    }
}
