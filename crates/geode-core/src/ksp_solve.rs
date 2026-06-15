//! **Krylov iterative solvers** for the complex-symmetric driven system
//! `A(ω) x = b` (issue #238).
//!
//! All driven and eigen solves currently exit to a direct sparse LU
//! ([`faer::sparse::linalg::solvers::Lu`]) at the solve boundary
//! ([`crate::driven::FactoredDrivenOperator`],
//! [`crate::complex_lanczos`]). Direct factorization caps the problem
//! size at what fill-in can afford; for larger structures (the
//! ~30k-edge patch fixture, future 3-D stacks) the next ceiling is the
//! solve itself.
//!
//! This module realizes the `ksp_solve` / `krylov_step` /
//! `preconditioning-framework` L4 surfaces of the operator tracker
//! (#5) — the iterative-solve corner of `ksp_solve` complementing the
//! existing direct-solve corner. It exposes:
//!
//! - The [`KspSolve`] trait — the iterative analog of the LU "solve at
//!   ω" boundary. Implementors take the assembled sparse `A` and RHS
//!   `b` and produce the solution + a [`KspReport`] of iteration count
//!   / final residual.
//! - A **complex-symmetric COCG** solver ([`Cocg`]), the natural
//!   Krylov choice when `A^T = A` without conjugation (the assembled
//!   driven pencil's invariant — see [`crate::driven`] and PR #55).
//!   Real-CG carries over verbatim by replacing the Hermitian
//!   `(r, z) = r^H z` inner product with the bilinear `r^T z`. The
//!   same complex-symmetric structure powers the bilinear-form
//!   Lanczos in [`crate::complex_lanczos`].
//! - A [`Preconditioner`] trait with two concrete preconditioners:
//!   [`IdentityPreconditioner`] (the no-op) and [`JacobiPreconditioner`]
//!   (diagonal scaling — `M = diag(A)`, the simplest and cheapest
//!   left-preconditioner).
//!
//! # Algorithm — preconditioned COCG
//!
//! For complex-symmetric `A` (`A^T = A`, **bilinear**, no conjugation)
//! and SPD-ish preconditioner `M` (we only require `M^{-1}` applicable
//! and the bilinear `r^T M^{-1} r` to not vanish), the preconditioned
//! COCG iteration is:
//!
//! ```text
//! x_0 given (we use 0),  r_0 = b - A x_0,  z_0 = M^{-1} r_0,  p_0 = z_0
//! ρ_0 = r_0^T z_0                     (bilinear, no conjugation)
//! for k = 0, 1, …:
//!     q = A p_k
//!     α = ρ_k / (p_k^T q)             (bilinear)
//!     x_{k+1} = x_k + α p_k
//!     r_{k+1} = r_k - α q
//!     if ‖r_{k+1}‖₂ ≤ tol · ‖b‖₂: stop
//!     z_{k+1} = M^{-1} r_{k+1}
//!     ρ_{k+1} = r_{k+1}^T z_{k+1}
//!     β = ρ_{k+1} / ρ_k
//!     p_{k+1} = z_{k+1} + β p_k
//! ```
//!
//! See e.g. van der Vorst & Mellisen (1990), Bai et al. *Templates*
//! §6.8. COCG can in principle break down (the bilinear form is not
//! positive definite, so `ρ_k` or `p^T q` can vanish), but for the
//! lossy / PML driven pencils we care about — where `A` is close to a
//! real SPD with a small absorbing imaginary part — breakdown is
//! extremely unlikely on the grid sizes the test suite covers. The
//! solver guards against this by returning a [`KspError::Breakdown`]
//! rather than producing NaNs.
//!
//! # Convergence reporting
//!
//! Every solve returns a [`KspReport`] with the iteration count,
//! final relative residual `‖A x − b‖₂ / ‖b‖₂`, and a flag indicating
//! whether the tolerance was reached. The acceptance criteria for
//! issue #238 require iteration counts to be reported on the patch
//! benchmark fixture; this is the channel.

use faer::c64;
use faer::sparse::SparseColMatRef;

use crate::complex_lanczos::spmv;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the Krylov solver path.
#[derive(Debug, thiserror::Error)]
pub enum KspError {
    /// The Krylov iteration broke down — a bilinear inner product
    /// (`r^T z` or `p^T A p`) vanished or went non-finite before
    /// convergence. COCG is not unconditionally robust; if this
    /// happens, switch to a stronger preconditioner (Jacobi → ILU →
    /// direct) or back off to the direct path.
    #[error(
        "COCG breakdown at iteration {iter}: bilinear inner product {kind} = {value_re}+{value_im}i \
         is zero or non-finite (the complex-symmetric Krylov form is not unconditionally robust; \
         a stronger preconditioner or the direct solver is required)"
    )]
    Breakdown {
        /// Iteration index at which the breakdown was detected.
        iter: usize,
        /// Which bilinear quantity failed (`"r^T z"` or `"p^T A p"`).
        kind: &'static str,
        /// Real and imaginary parts of the offending scalar.
        value_re: f64,
        value_im: f64,
    },
    /// The iteration ran out of steps without reaching the requested
    /// tolerance. Bumping [`Cocg::max_iters`] or using a stronger
    /// preconditioner usually fixes this; in the worst case fall back
    /// to direct LU.
    #[error(
        "COCG did not converge in {iter} iterations: relative residual {residual_rel} > tol {tol}"
    )]
    NotConverged {
        iter: usize,
        residual_rel: f64,
        tol: f64,
    },
    /// The right-hand side is identically zero (after the optional
    /// scaling). The solution is trivially zero, but callers that
    /// asked for a Krylov solve probably want to know.
    #[error("right-hand side has ‖b‖₂ = 0 (trivial solution); no iteration to run")]
    ZeroRhs,
    /// Dimension mismatch between the operator and the RHS / output
    /// buffer.
    #[error("Krylov dimension mismatch: A is {n}×{n}, but {what} has length {got}")]
    DimMismatch {
        n: usize,
        what: &'static str,
        got: usize,
    },
}

// ---------------------------------------------------------------------------
// KspReport
// ---------------------------------------------------------------------------

/// Outcome of a Krylov solve.
///
/// Returned by every [`KspSolve::solve`] call. The relative residual
/// is `‖A x − b‖₂ / ‖b‖₂`, computed with an explicit spmv on the
/// final iterate — the **same** residual definition the direct path
/// reports in [`crate::driven::DrivenSolution::residual_rel`], so the
/// two are directly comparable.
#[derive(Debug, Clone, Copy)]
pub struct KspReport {
    /// Number of Krylov iterations that actually ran.
    pub iters: usize,
    /// Final relative residual `‖A x − b‖₂ / ‖b‖₂`.
    pub residual_rel: f64,
    /// `true` if the tolerance was met within the iteration budget,
    /// `false` if the iteration was cut off (callers should treat the
    /// returned `x` as an approximation in that case — the iterative
    /// entry points return [`KspError::NotConverged`] before reaching
    /// the caller, but this flag is preserved for diagnostics).
    pub converged: bool,
}

// ---------------------------------------------------------------------------
// Preconditioner trait + implementations
// ---------------------------------------------------------------------------

/// Left-preconditioner for the Krylov solver: applies `z = M^{-1} r`.
///
/// The preconditioner does not need to be explicitly stored — only
/// the application matters. Implementations are responsible for
/// their own setup (e.g. extracting the diagonal of `A` at
/// construction time).
///
/// All concrete preconditioners in this module are **complex**,
/// matching the driven-system arithmetic.
pub trait Preconditioner {
    /// Apply `z = M^{-1} r`. `z` and `r` are the same length as the
    /// system. Implementations may panic on length mismatch; the
    /// [`Cocg`] driver checks lengths once up front.
    fn apply(&self, r: &[c64], z: &mut [c64]);

    /// Size `n` the preconditioner was built for (the system
    /// dimension). Used for sanity checks.
    fn dim(&self) -> usize;
}

/// Identity preconditioner — applies `z = r`. Useful as a baseline
/// (unpreconditioned COCG) and for testing the iteration core in
/// isolation.
#[derive(Debug, Clone, Copy)]
pub struct IdentityPreconditioner {
    n: usize,
}

impl IdentityPreconditioner {
    /// Build the identity preconditioner for a system of size `n`.
    pub fn new(n: usize) -> Self {
        Self { n }
    }
}

impl Preconditioner for IdentityPreconditioner {
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        debug_assert_eq!(r.len(), self.n);
        debug_assert_eq!(z.len(), self.n);
        z.copy_from_slice(r);
    }
    fn dim(&self) -> usize {
        self.n
    }
}

/// Jacobi (diagonal) preconditioner — `M = diag(A)`, so the
/// application is `z_i = r_i / A_{ii}`. The cheapest non-trivial
/// preconditioner; effective whenever `A` is diagonally dominant or
/// close to it (the driven curl-curl pencil at moderate frequencies
/// usually is).
///
/// Construction caches the reciprocal of each diagonal entry up
/// front, so each apply is one complex multiply per DOF.
///
/// # Errors at construction
///
/// Returns [`KspError::Breakdown`] (with `kind = "diag(A)"`) if any
/// diagonal entry is zero or non-finite — the Jacobi preconditioner
/// is not defined in that case.
#[derive(Debug, Clone)]
pub struct JacobiPreconditioner {
    inv_diag: Vec<c64>,
}

impl JacobiPreconditioner {
    /// Extract `diag(A)` from a complex sparse CSC matrix and store
    /// its reciprocal. The matrix must be square.
    pub fn new(a: SparseColMatRef<'_, usize, c64>) -> Result<Self, KspError> {
        let n = a.nrows();
        assert_eq!(a.ncols(), n, "JacobiPreconditioner requires square A");
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        let mut diag = vec![c64::new(0.0, 0.0); n];
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                let i = row_idx[k];
                if i == j {
                    diag[j] += val[k];
                }
            }
        }
        let mut inv_diag = vec![c64::new(0.0, 0.0); n];
        for (j, d) in diag.iter().enumerate() {
            if !(d.re.is_finite() && d.im.is_finite()) {
                return Err(KspError::Breakdown {
                    iter: 0,
                    kind: "diag(A)",
                    value_re: d.re,
                    value_im: d.im,
                });
            }
            let mag2 = d.re * d.re + d.im * d.im;
            if mag2 == 0.0 {
                return Err(KspError::Breakdown {
                    iter: 0,
                    kind: "diag(A)",
                    value_re: d.re,
                    value_im: d.im,
                });
            }
            // 1 / (a + bi) = (a − bi) / (a² + b²).
            inv_diag[j] = c64::new(d.re / mag2, -d.im / mag2);
        }
        Ok(Self { inv_diag })
    }
}

impl Preconditioner for JacobiPreconditioner {
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        debug_assert_eq!(r.len(), self.inv_diag.len());
        debug_assert_eq!(z.len(), self.inv_diag.len());
        for i in 0..r.len() {
            z[i] = r[i] * self.inv_diag[i];
        }
    }
    fn dim(&self) -> usize {
        self.inv_diag.len()
    }
}

// ---------------------------------------------------------------------------
// KspSolve trait
// ---------------------------------------------------------------------------

/// Krylov-solver boundary — the iterative analog of the direct LU
/// factor+solve path. Mirrors the role of
/// [`faer::sparse::linalg::solvers::Lu`] in
/// [`crate::driven::FactoredDrivenOperator`]: takes the assembled
/// sparse `A` and RHS `b` and produces the solution + a convergence
/// report.
///
/// The trait is intentionally narrow — no per-frequency caching, no
/// material plumbing. The driven layer assembles `A(ω)` and `b(ω)`
/// using its existing machinery and hands the pair off; switching
/// solvers in `driven_solve_iterative` is a matter of swapping which
/// `KspSolve` implementation is passed.
pub trait KspSolve {
    /// Solve `A x = b` for `x` (overwriting `x` with the solution,
    /// which is also returned implicitly by reference). The caller
    /// initializes `x` (typically to zero — COCG's starting guess
    /// `x_0 = 0` makes `r_0 = b`).
    ///
    /// # Errors
    ///
    /// Returns [`KspError::DimMismatch`] on shape mismatch,
    /// [`KspError::ZeroRhs`] if `b ≡ 0`,
    /// [`KspError::Breakdown`] on bilinear-form failure, and
    /// [`KspError::NotConverged`] when the iteration budget is
    /// exhausted before reaching tolerance.
    fn solve<P: Preconditioner>(
        &self,
        a: SparseColMatRef<'_, usize, c64>,
        b: &[c64],
        x: &mut [c64],
        precond: &P,
    ) -> Result<KspReport, KspError>;
}

// ---------------------------------------------------------------------------
// COCG
// ---------------------------------------------------------------------------

/// **Conjugate Orthogonal Conjugate Gradient** for complex-symmetric
/// `A` (`A^T = A`, bilinear / no conjugation — the driven pencil
/// invariant, see PR #55). Real-CG carries over verbatim with the
/// Hermitian inner product replaced by the bilinear `u^T v` — same
/// substitution the complex-symmetric Lanczos in
/// [`crate::complex_lanczos`] uses.
///
/// # Configuration
///
/// - `tol` — relative-residual stopping criterion `‖r‖₂ ≤ tol·‖b‖₂`.
/// - `max_iters` — iteration budget; `KspError::NotConverged` if
///   exhausted.
/// - `breakdown_tol` — magnitude below which a bilinear inner
///   product is considered zero (breakdown). Default `1e-300` —
///   essentially "underflowed to zero".
///
/// `KspReport::iters` is the iteration count actually executed and
/// is the figure of merit issue #238 asks for in the regression test.
#[derive(Debug, Clone, Copy)]
pub struct Cocg {
    /// Convergence tolerance on the relative residual.
    pub tol: f64,
    /// Maximum number of iterations.
    pub max_iters: usize,
    /// Threshold below which `|r^T z|` or `|p^T A p|` is treated as a
    /// breakdown.
    pub breakdown_tol: f64,
}

impl Default for Cocg {
    fn default() -> Self {
        Self {
            tol: 1e-10,
            max_iters: 2000,
            breakdown_tol: 1e-300,
        }
    }
}

impl Cocg {
    /// Convenience constructor — `tol` and `max_iters` only, with
    /// the default breakdown threshold.
    pub fn new(tol: f64, max_iters: usize) -> Self {
        Self {
            tol,
            max_iters,
            ..Default::default()
        }
    }
}

/// `acc = u^T v` (**bilinear**, no conjugation) — the complex-symmetric
/// Krylov inner product. Mirrors the bilinear form
/// [`crate::complex_lanczos`] uses for the Lanczos pencil.
fn bilinear_dot(u: &[c64], v: &[c64]) -> c64 {
    debug_assert_eq!(u.len(), v.len());
    let mut acc = c64::new(0.0, 0.0);
    for i in 0..u.len() {
        acc += u[i] * v[i];
    }
    acc
}

/// `‖v‖₂` (Euclidean norm) — for the residual stopping criterion the
/// natural choice is the standard Hermitian norm: it is what the
/// direct path reports as `residual_rel`, and what users expect
/// "‖A x − b‖" to mean.
fn euclid_norm(v: &[c64]) -> f64 {
    let mut acc = 0.0_f64;
    for x in v {
        acc += x.re * x.re + x.im * x.im;
    }
    acc.sqrt()
}

impl KspSolve for Cocg {
    fn solve<P: Preconditioner>(
        &self,
        a: SparseColMatRef<'_, usize, c64>,
        b: &[c64],
        x: &mut [c64],
        precond: &P,
    ) -> Result<KspReport, KspError> {
        let n = a.nrows();
        if a.ncols() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "A.ncols",
                got: a.ncols(),
            });
        }
        if b.len() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "b",
                got: b.len(),
            });
        }
        if x.len() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "x",
                got: x.len(),
            });
        }
        if precond.dim() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "preconditioner",
                got: precond.dim(),
            });
        }

        let b_norm = euclid_norm(b);
        if b_norm == 0.0 {
            return Err(KspError::ZeroRhs);
        }
        let target = self.tol * b_norm;

        // r = b - A x  (x typically starts at 0, in which case r = b).
        let mut r = vec![c64::new(0.0, 0.0); n];
        spmv(a, x, &mut r); // r = A x
        for i in 0..n {
            r[i] = b[i] - r[i];
        }

        // Early exit if the initial guess already satisfies the tolerance.
        let r_norm = euclid_norm(&r);
        if r_norm <= target {
            return Ok(KspReport {
                iters: 0,
                residual_rel: r_norm / b_norm,
                converged: true,
            });
        }

        // z = M^{-1} r, p = z, ρ = r^T z.
        let mut z = vec![c64::new(0.0, 0.0); n];
        precond.apply(&r, &mut z);
        let mut p = z.clone();
        let mut rho = bilinear_dot(&r, &z);
        let bd_check =
            |val: c64, iter: usize, kind: &'static str, tol: f64| -> Result<(), KspError> {
                let mag2 = val.re * val.re + val.im * val.im;
                if !val.re.is_finite() || !val.im.is_finite() || mag2 < tol * tol {
                    Err(KspError::Breakdown {
                        iter,
                        kind,
                        value_re: val.re,
                        value_im: val.im,
                    })
                } else {
                    Ok(())
                }
            };
        bd_check(rho, 0, "r^T z", self.breakdown_tol)?;

        let mut q = vec![c64::new(0.0, 0.0); n];

        for k in 0..self.max_iters {
            // q = A p
            spmv(a, &p, &mut q);

            // α = ρ / (p^T q)
            let pq = bilinear_dot(&p, &q);
            bd_check(pq, k, "p^T A p", self.breakdown_tol)?;
            let alpha = rho / pq;

            // x += α p; r -= α q
            for i in 0..n {
                x[i] += alpha * p[i];
                r[i] -= alpha * q[i];
            }

            // Tolerance check on the recursively maintained residual.
            let r_norm = euclid_norm(&r);
            if r_norm <= target {
                // Recompute the true residual to report a reliable
                // figure (the recursively maintained `r` can drift on
                // ill-conditioned systems; the explicit spmv matches
                // what the direct path reports).
                let mut ax = vec![c64::new(0.0, 0.0); n];
                spmv(a, x, &mut ax);
                let mut true_r = 0.0_f64;
                for i in 0..n {
                    let d = ax[i] - b[i];
                    true_r += d.re * d.re + d.im * d.im;
                }
                let residual_rel = true_r.sqrt() / b_norm;
                return Ok(KspReport {
                    iters: k + 1,
                    residual_rel,
                    converged: residual_rel <= self.tol,
                });
            }

            // z = M^{-1} r, ρ_new = r^T z, β = ρ_new / ρ
            precond.apply(&r, &mut z);
            let rho_new = bilinear_dot(&r, &z);
            bd_check(rho_new, k + 1, "r^T z", self.breakdown_tol)?;
            let beta = rho_new / rho;
            rho = rho_new;

            // p = z + β p
            for i in 0..n {
                p[i] = z[i] + beta * p[i];
            }
        }

        // Out of iterations — compute the true residual for the report.
        let mut ax = vec![c64::new(0.0, 0.0); n];
        spmv(a, x, &mut ax);
        let mut true_r = 0.0_f64;
        for i in 0..n {
            let d = ax[i] - b[i];
            true_r += d.re * d.re + d.im * d.im;
        }
        let residual_rel = true_r.sqrt() / b_norm;
        Err(KspError::NotConverged {
            iter: self.max_iters,
            residual_rel,
            tol: self.tol,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build a small **real-symmetric positive-definite** complex
    /// matrix (Im ≡ 0) and check COCG converges. The bilinear `u^T v`
    /// reduces to the real dot product on real vectors, so COCG
    /// degenerates to ordinary CG and must converge in ≤ n iterations.
    #[test]
    fn cocg_recovers_cg_on_real_spd_system() {
        // 3x3 SPD: [[4,1,0],[1,3,1],[0,1,2]]
        let n = 3;
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(4.0, 0.0)),
            Triplet::new(0, 1, c64::new(1.0, 0.0)),
            Triplet::new(1, 0, c64::new(1.0, 0.0)),
            Triplet::new(1, 1, c64::new(3.0, 0.0)),
            Triplet::new(1, 2, c64::new(1.0, 0.0)),
            Triplet::new(2, 1, c64::new(1.0, 0.0)),
            Triplet::new(2, 2, c64::new(2.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = vec![c64::new(1.0, 0.0), c64::new(2.0, 0.0), c64::new(3.0, 0.0)];

        let mut x = vec![c64::new(0.0, 0.0); n];
        let pc = IdentityPreconditioner::new(n);
        let cocg = Cocg::new(1e-12, 100);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges on a 3×3 SPD system");
        assert!(report.converged);
        assert!(report.iters <= n, "CG hits SPD in ≤ n iters");
        // Check the residual.
        let mut ax = vec![c64::new(0.0, 0.0); n];
        spmv(a.as_ref(), &x, &mut ax);
        let mut res2 = 0.0;
        for i in 0..n {
            let d = ax[i] - b[i];
            res2 += d.re * d.re + d.im * d.im;
        }
        assert!(res2.sqrt() < 1e-9);
    }

    /// COCG must solve a small **complex-symmetric** (`A^T = A`, no
    /// conjugation) lossy system to round-off.
    #[test]
    fn cocg_solves_complex_symmetric_lossy_system() {
        // A = [[4+0.1i, 1+0.05i, 0],
        //      [1+0.05i, 3+0.2i, 1],
        //      [0,        1,      2+0.1i]]
        // Symmetric (A[i,j] = A[j,i]) but with non-trivial imaginary parts.
        let n = 3;
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(4.0, 0.1)),
            Triplet::new(0, 1, c64::new(1.0, 0.05)),
            Triplet::new(1, 0, c64::new(1.0, 0.05)),
            Triplet::new(1, 1, c64::new(3.0, 0.2)),
            Triplet::new(1, 2, c64::new(1.0, 0.0)),
            Triplet::new(2, 1, c64::new(1.0, 0.0)),
            Triplet::new(2, 2, c64::new(2.0, 0.1)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];

        let mut x = vec![c64::new(0.0, 0.0); n];
        let pc = JacobiPreconditioner::new(a.as_ref()).expect("nonzero diagonal");
        let cocg = Cocg::new(1e-12, 100);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges");
        assert!(report.converged);
        assert!(report.residual_rel < 1e-10, "{:?}", report);
    }

    /// Jacobi preconditioner must reject a zero-diagonal matrix.
    #[test]
    fn jacobi_rejects_zero_diagonal() {
        let trips = vec![
            Triplet::new(0_usize, 1, c64::new(1.0, 0.0)),
            Triplet::new(1, 0, c64::new(1.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let err = JacobiPreconditioner::new(a.as_ref()).unwrap_err();
        assert!(matches!(err, KspError::Breakdown { .. }));
    }

    /// A zero RHS must be reported, not silently consumed.
    #[test]
    fn zero_rhs_is_an_error() {
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(2.0, 0.0)),
            Triplet::new(1, 1, c64::new(3.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let b = vec![c64::new(0.0, 0.0); 2];
        let mut x = vec![c64::new(0.0, 0.0); 2];
        let pc = IdentityPreconditioner::new(2);
        let err = Cocg::default()
            .solve(a.as_ref(), &b, &mut x, &pc)
            .unwrap_err();
        assert!(matches!(err, KspError::ZeroRhs));
    }

    /// Identity preconditioner is exactly the no-op.
    #[test]
    fn identity_preconditioner_is_noop() {
        let r = vec![c64::new(1.0, -2.0), c64::new(3.0, 4.0)];
        let mut z = vec![c64::new(0.0, 0.0); 2];
        IdentityPreconditioner::new(2).apply(&r, &mut z);
        assert_eq!(z, r);
    }

    /// Jacobi preconditioner inverts each diagonal entry.
    #[test]
    fn jacobi_inverts_each_diagonal_entry() {
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(2.0, 0.0)),
            Triplet::new(1, 1, c64::new(0.0, 4.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let pc = JacobiPreconditioner::new(a.as_ref()).unwrap();
        let r = vec![c64::new(4.0, 0.0), c64::new(8.0, 0.0)];
        let mut z = vec![c64::new(0.0, 0.0); 2];
        pc.apply(&r, &mut z);
        // 4 / 2 = 2; 8 / (4i) = -2i
        assert!((z[0].re - 2.0).abs() < 1e-15 && z[0].im.abs() < 1e-15);
        assert!(z[1].re.abs() < 1e-15 && (z[1].im - (-2.0)).abs() < 1e-15);
    }

    /// `NotConverged` is returned (not silent garbage) when the
    /// budget is exhausted.
    #[test]
    fn budget_exhaustion_returns_not_converged() {
        // Reuse the SPD 3×3 from the first test but allow only 1 iter.
        let n = 3;
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(4.0, 0.0)),
            Triplet::new(0, 1, c64::new(1.0, 0.0)),
            Triplet::new(1, 0, c64::new(1.0, 0.0)),
            Triplet::new(1, 1, c64::new(3.0, 0.0)),
            Triplet::new(1, 2, c64::new(1.0, 0.0)),
            Triplet::new(2, 1, c64::new(1.0, 0.0)),
            Triplet::new(2, 2, c64::new(2.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = vec![c64::new(1.0, 0.0), c64::new(2.0, 0.0), c64::new(3.0, 0.0)];
        let mut x = vec![c64::new(0.0, 0.0); n];
        let pc = IdentityPreconditioner::new(n);
        let cocg = Cocg::new(1e-15, 1); // tight tol + 1-iter budget
        let err = cocg.solve(a.as_ref(), &b, &mut x, &pc).unwrap_err();
        assert!(matches!(err, KspError::NotConverged { .. }));
    }
}
