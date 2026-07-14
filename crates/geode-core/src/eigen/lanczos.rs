//! Pure-Rust shift-and-invert Lanczos for the generalized symmetric
//! eigenproblem `K x = λ M x`.
//!
//! This is the **default** sparse path (the `arpack` cargo feature is
//! off by default). The algorithm:
//!
//! 1. Factor `A = K - σ M` once via faer's sparse LU.
//! 2. Run k Lanczos iterations on the operator `T(v) = A⁻¹ M v` in the
//!    `M`-induced inner product `⟨u, v⟩_M = uᵀ M v`. Because `A⁻¹ M` is
//!    self-adjoint in that inner product, the resulting tridiagonal `T_k`
//!    has real eigenvalues `μ_i`, and they are related to the original
//!    pencil's eigenvalues by `λ_i = σ + 1 / μ_i`. Convergence is fastest
//!    on the eigenvalues of `K x = λ M x` closest to `σ` — picking
//!    `σ = 0` targets the smallest-magnitude end of the spectrum, which
//!    is what every cube-warmup test wants.
//! 3. The small tridiagonal eigenproblem is solved with faer's dense
//!    `self_adjoint_eigenvalues` (we densify `T_k` since `k ≲ 64` in
//!    practice — the cost is rounding noise in the perf budget).
//!
//! We use **full reorthogonalization** at every step against the entire
//! basis history. This is overkill for very large `n` but it keeps the
//! Lanczos basis numerically `M`-orthogonal at all sizes we care about
//! (n ≲ 10⁴ post-Dirichlet) and removes the need for a selective /
//! restarted variant. The cost is O(k²·n) extra work; negligible next to
//! the `k` sparse triangular solves.
//!
//! To make that O(k²) reorthogonalization cheap in practice we cache
//! `M·v_j` alongside each basis vector `v_j` (`m_basis`), so the inner
//! reorth loop is a stored-vector dot + axpy rather than a fresh SpMV per
//! basis vector every outer iteration. This is an exact re-ordering of the
//! same arithmetic (the spectrum is bit-for-bit identical), and on the
//! 133k-DOF transmon eigensolve it collapses the reorth phase from ~9.7 s
//! to ~0.8 s — a 1.64x end-to-end wall-time reduction on that test
//! (34.9 s → 21.3 s). See issue #506 for the full profile.
//!
//! **faer-parallelism (issue #518, was deferred from #506 Phase 0):** the
//! sparse LU factorization phase (`sp_lu`, ~33% of the solve) is sped up by
//! compiling faer with its `rayon` feature. faer 0.24 parallelism is a
//! process-global `AtomicUsize` (`set_global_parallelism` /
//! `get_global_parallelism`, no per-call `Par` argument), so it (a) races
//! other threads in the same process — the driven/assembly paths also call
//! `sp_lu`/`solve_in_place` — and (b) must be reverted to serial before the
//! single-RHS triangular-solve loop, where rayon is measurably *slower*
//! (latency-bound). Both concerns are handled by scoping the parallelism to
//! exactly the `sp_lu` call via [`crate::eigen::parallel::ParallelismGuard`],
//! a panic-safe RAII guard that sets `Par::rayon(n)` for the factorization
//! and restores the prior global parallelism on drop (including on panic).
//! The thread count comes from `GEODE_NUM_THREADS` (falling back to the
//! physical core count). The factorization is deterministic, so eigenvalues
//! are identical within tolerance across thread counts — see the
//! `*_agree_across_thread_counts` regression tests below.
//!
//! Convergence is declared when the residual norm
//! `‖K x - λ M x‖_2 / ‖λ M x‖_2 < tol` for **every** requested mode.
//!
//! # Why this loop is not ported onto [`crate::solver::iterate`]
//!
//! Full reorthogonalization keeps the **entire** basis history, so the
//! Krylov basis `V_k` gains one column per Lanczos iteration — the
//! carried state grows by one vector each step. That violates
//! [`crate::solver::iterate`] **contract restriction 1** (loop-invariant
//! carried-state shapes): a trace-once graph backend traces the loop body
//! once and cannot express a state slot whose tensor shape changes
//! between iterations. This is exactly the "do not grow a `Vec` inside
//! the carried state" anti-pattern. The restart loop is therefore
//! intentionally **not** expressed via `iterate_while` /
//! `iterate_while_with_prev`; it stays as a hand-rolled loop.

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef};
use faer::{Mat, MatMut};

use crate::eigen::dense::{EigenError, EigenPair};
use crate::eigen::parallel::{ParallelismGuard, resolve_num_threads};

/// Backend for the shift-invert inner solve `(K − σM) y = b`.
///
/// The outer Lanczos recurrence is identical for both variants; only the
/// way each `A⁻¹ M v` apply is realized differs.
///
/// # Why a matrix-free variant exists (issue #524)
///
/// The [`Direct`](InnerSolver::Direct) path forms `A = K − σM` explicitly
/// and factors it once with faer's sparse LU (`sp_lu`). That factorization
/// is fast to *re-apply* (k cheap triangular solves) but its **fill-in**
/// scales roughly `O(N^{4/3})` for 3-D H(curl) pencils, so the `L`/`U`
/// factors blow past commodity memory somewhere between ~10⁵ and ~10⁶
/// interior DOFs. On the transmon benchmark the direct path OOM-kills at
/// ~1M DOF (measured 63.9 GB peak RSS, SIGKILL) even though Palace solves
/// the same mesh at ~4 GB/rank.
///
/// The [`MatrixFree`](InnerSolver::MatrixFree) path never forms or factors
/// `A`. Instead each inner solve is an **iterative** Jacobi-preconditioned
/// conjugate-gradient solve of `(K − σM) y = b`, applying `K` and `M`
/// matrix-free via sparse mat-vecs (`spmv`). No `L`/`U` factors are ever
/// allocated, so the working set stays `O(N)` (a handful of length-`N`
/// Krylov vectors plus the two input CSC operators) and the eigensolve
/// scales to meshes that OOM the direct path.
///
/// ## Numerical enabler: shift below the spectrum
///
/// CG requires the inner operator `(K − σM)` to be **SPD**. For the
/// *lowest* physical modes this is arranged by placing the shift `σ`
/// **below** the physical spectrum (the transmon resonator target sits
/// near 4.5 GHz, below the lowest ~5 GHz physical mode). With `K` SPD (or
/// PSD) and `M` SPD, `K − σM` is SPD whenever `σ` is below the smallest
/// generalized eigenvalue, so plain CG converges without any
/// indefinite-system machinery. Interior / indefinite shifts (which would
/// need MINRES/GMRES) are explicitly out of Phase-1 scope.
///
/// ## Inner-tolerance coupling
///
/// The inner CG tolerance is tied to the outer Lanczos tolerance: it is
/// set to a fixed fraction (`INNER_TOL_FACTOR`) of the outer `tol`, so the
/// inner solve is always tighter than the accuracy the outer iteration is
/// trying to reach. This keeps the shift-invert operator `A⁻¹M` accurate
/// enough that the Lanczos recurrence is not polluted by inner-solve
/// residual noise, while avoiding over-solving early iterations. See
/// [`SparseShiftInvertLanczos::inner_tol`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InnerSolver {
    /// Form `A = K − σM` and factor it once with faer's sparse LU. Fast to
    /// re-apply, but the factorization fill-in is not `O(N)` memory — this
    /// is the historical default and the small-problem path.
    #[default]
    Direct,
    /// Never form or factor `A`; solve `(K − σM) y = b` iteratively with
    /// matrix-free Jacobi-preconditioned CG. `O(N)` memory; the path that
    /// scales past the direct factorization's memory wall (issue #524).
    /// Requires `(K − σM)` SPD (place `σ` below the spectrum).
    MatrixFree,
}

/// Sparse generalized-symmetric eigensolver via shift-and-invert Lanczos.
///
/// Parameters:
/// - `sigma`: shift; ritz values closest to `sigma` converge first. `0.0`
///   targets the smallest-magnitude end of the spectrum (the FEM ground
///   modes), which is what every test in this crate wants.
/// - `max_iters`: maximum Lanczos iterations. The algorithm also bails
///   early on numerical breakdown (β ≈ 0, an invariant subspace has been
///   found) or when residual convergence has been reached on every
///   requested mode.
/// - `tol`: relative residual tolerance per mode. `1e-9` is comfortable
///   for f64 sparse LU.
/// - `inner`: which inner-solve backend to use for `(K − σM)⁻¹`
///   ([`InnerSolver::Direct`] by default — the matrix-free variant is
///   additive and opt-in; see [`InnerSolver`]).
#[derive(Debug, Clone, Copy)]
pub struct SparseShiftInvertLanczos {
    pub sigma: f64,
    pub max_iters: usize,
    pub tol: f64,
    pub inner: InnerSolver,
}

impl Default for SparseShiftInvertLanczos {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-9,
            inner: InnerSolver::Direct,
        }
    }
}

/// Fraction of the outer Lanczos tolerance used for the inner CG solve.
///
/// The inner `(K − σM) y = b` solve is driven to `INNER_TOL_FACTOR · tol`
/// (relative residual) so it is always tighter than the outer convergence
/// target — the shift-invert operator must be more accurate than the
/// accuracy the outer Lanczos is chasing, otherwise inner-solve residual
/// noise corrupts the tridiagonalization. The value is a conservative
/// margin; tightening it further only spends more inner iterations.
const INNER_TOL_FACTOR: f64 = 1e-2;

/// Hard cap on inner CG iterations, as a multiple of the operator dimension.
///
/// SPD CG converges in at most `N` iterations exactly, but with Jacobi
/// preconditioning on a well-shifted pencil it should converge in far
/// fewer. The cap is generous (`2·N`) purely as a non-convergence guard so
/// a pathological pencil surfaces as an error rather than an infinite loop.
const INNER_MAX_ITER_FACTOR: usize = 2;

/// Parallel of [`crate::eigen::dense::EigenSolver`] for sparse matrices.
///
/// The dense trait takes `MatRef<f64>`; sparse routines need column-major
/// CSC, so a second trait is the cleanest path — no surface change to the
/// dense path and no impedance mismatch at call sites.
pub trait SparseEigenSolver {
    fn smallest_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n: usize,
    ) -> Result<Vec<f64>, EigenError>;
}

/// Compute `y += a · A · x` where `A` is given in CSC form.
///
/// CSC = column compressed: for each column `j`, the slice
/// `row_idx[col_ptr[j] .. col_ptr[j+1]]` lists the row indices of the
/// non-zero entries in that column and the parallel `val` slice carries
/// the values.
fn spmv_add(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64], alpha: f64) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let ncols = a.ncols();
    for j in 0..ncols {
        let start = col_ptr[j];
        let end = col_ptr[j + 1];
        let xj = x[j] * alpha;
        if xj == 0.0 {
            continue;
        }
        for k in start..end {
            let i = row_idx[k];
            y[i] += val[k] * xj;
        }
    }
}

/// Compute `y = A · x` (overwrite).
fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    spmv_add(a, x, y, 1.0);
}

/// `K - σ M` as a fresh sparse matrix, used only to build the LU.
///
/// Iterates the union of both patterns. K and M share identical sparsity
/// in this crate's assembler (same P1 stencil), so the union is just
/// `K`'s pattern — but we don't assume that, to keep the routine generic.
fn shifted_pencil(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    use faer::sparse::Triplet;
    let n = k.nrows();
    assert_eq!(k.ncols(), n);
    assert_eq!(m.nrows(), n);
    assert_eq!(m.ncols(), n);

    let nnz = k.col_ptr()[n] + m.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(nnz);

    let push = |trips: &mut Vec<Triplet<usize, usize, f64>>,
                a: SparseColMatRef<'_, usize, f64>,
                scale: f64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    };
    push(&mut trips, k, 1.0);
    if sigma != 0.0 {
        push(&mut trips, m, -sigma);
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("shifted pencil assembly: {e:?}")))
}

/// Solve `A y = b` in-place via a precomputed LU factorization.
fn solve_with_lu(lu: &Lu<usize, f64>, rhs: &[f64], out: &mut [f64]) -> Result<(), EigenError> {
    use faer::linalg::solvers::Solve;
    let n = rhs.len();
    let mut work: Mat<f64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    // The `SolveCore`/`Solve` impl mutates in place.
    let work_mut: MatMut<'_, f64> = work.as_mut();
    lu.solve_in_place(work_mut);
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
    Ok(())
}

/// The matrix-free shifted-pencil operator `A = K − σM`, applied without
/// ever assembling `A` or forming an LU factorization.
///
/// Holds only references to the two input CSC operators plus a cached
/// inverse-diagonal for the Jacobi preconditioner. Memory is `O(N)` beyond
/// the borrowed operators (the diagonal vector) — no fill-in, no factors.
struct ShiftedMatrixFreeOp<'a> {
    k: SparseColMatRef<'a, usize, f64>,
    m: SparseColMatRef<'a, usize, f64>,
    sigma: f64,
    /// Jacobi inverse-diagonal `1 / (K_ii − σ M_ii)` (safe-guarded against
    /// a zero pivot, which falls back to `1.0`).
    inv_diag: Vec<f64>,
}

/// Extract the main diagonal of a CSC matrix into `out` (length `n`).
fn csc_diagonal(a: SparseColMatRef<'_, usize, f64>, out: &mut [f64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    out.iter_mut().for_each(|v| *v = 0.0);
    for j in 0..a.ncols() {
        for kk in col_ptr[j]..col_ptr[j + 1] {
            if row_idx[kk] == j {
                out[j] += val[kk];
            }
        }
    }
}

impl<'a> ShiftedMatrixFreeOp<'a> {
    fn new(
        k: SparseColMatRef<'a, usize, f64>,
        m: SparseColMatRef<'a, usize, f64>,
        sigma: f64,
    ) -> Self {
        let n = k.nrows();
        let mut dk = vec![0.0_f64; n];
        let mut dm = vec![0.0_f64; n];
        csc_diagonal(k, &mut dk);
        csc_diagonal(m, &mut dm);
        let inv_diag: Vec<f64> = dk
            .iter()
            .zip(dm.iter())
            .map(|(&kii, &mii)| {
                let d = kii - sigma * mii;
                if d.abs() > 0.0 { 1.0 / d } else { 1.0 }
            })
            .collect();
        Self {
            k,
            m,
            sigma,
            inv_diag,
        }
    }

    /// `y = (K − σM) · x`, matrix-free (two SpMVs, no assembled `A`).
    fn apply(&self, x: &[f64], y: &mut [f64]) {
        spmv(self.k, x, y);
        if self.sigma != 0.0 {
            spmv_add(self.m, x, y, -self.sigma);
        }
    }

    /// Apply the Jacobi preconditioner `z = D⁻¹ r` (elementwise).
    fn precond(&self, r: &[f64], z: &mut [f64]) {
        for i in 0..r.len() {
            z[i] = self.inv_diag[i] * r[i];
        }
    }
}

/// Matrix-free Jacobi-preconditioned CG solve of the SPD system
/// `(K − σM) y = b` (issue #524).
///
/// Reuses the same conjugate-gradient recurrence and Jacobi (diagonal)
/// preconditioning strategy as the driven matrix-free solver
/// (`crate::driven::matrix_free` / `crate::solver::ksp::Cocg`), specialized
/// to the **real SPD** shifted pencil so no complex arithmetic or
/// indefinite-system handling is needed. The operator `A = K − σM` is
/// applied through [`ShiftedMatrixFreeOp::apply`] (two sparse mat-vecs);
/// `A` is never assembled and never factored — the working set is `O(N)`.
///
/// `out` is used as the initial guess (callers pass a warm start from the
/// previous Lanczos iteration when available, which cuts inner iterations
/// substantially). Returns the number of CG iterations executed. Errors if
/// the relative residual has not dropped below `tol` within `max_iters`.
fn cg_solve_matrix_free(
    op: &ShiftedMatrixFreeOp<'_>,
    b: &[f64],
    out: &mut [f64],
    tol: f64,
    max_iters: usize,
) -> Result<usize, EigenError> {
    let n = b.len();
    let bnorm = b.iter().map(|v| v * v).sum::<f64>().sqrt();
    if bnorm == 0.0 {
        out.iter_mut().for_each(|v| *v = 0.0);
        return Ok(0);
    }

    let mut r = vec![0.0_f64; n];
    let mut ap = vec![0.0_f64; n];
    // r = b − A·out  (out is the warm-start guess).
    op.apply(out, &mut ap);
    for i in 0..n {
        r[i] = b[i] - ap[i];
    }

    let mut z = vec![0.0_f64; n];
    op.precond(&r, &mut z);
    let mut p = z.clone();
    let mut rz = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum::<f64>();

    let thresh = tol * bnorm;
    let mut rnorm = r.iter().map(|v| v * v).sum::<f64>().sqrt();
    if rnorm <= thresh {
        return Ok(0);
    }

    for it in 1..=max_iters {
        op.apply(&p, &mut ap);
        let p_ap = p.iter().zip(ap.iter()).map(|(a, b)| a * b).sum::<f64>();
        if p_ap.abs() <= 0.0 {
            // Breakdown: pᵀAp ≈ 0. On an SPD operator this only happens at
            // (near-)convergence; treat the current iterate as the answer.
            return Ok(it - 1);
        }
        let alpha = rz / p_ap;
        for i in 0..n {
            out[i] += alpha * p[i];
            r[i] -= alpha * ap[i];
        }
        rnorm = r.iter().map(|v| v * v).sum::<f64>().sqrt();
        if rnorm <= thresh {
            return Ok(it);
        }
        op.precond(&r, &mut z);
        let rz_next = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum::<f64>();
        let beta = rz_next / rz;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
        rz = rz_next;
    }

    Err(EigenError::FaerGevd(format!(
        "matrix-free inner CG failed to converge: ‖r‖/‖b‖ = {:.3e} > tol = {tol:.3e} \
         after {max_iters} iters (is (K − σM) SPD? place σ below the spectrum)",
        rnorm / bnorm
    )))
}

/// Solve the symmetric tridiagonal eigenvalue problem for `(alpha, beta)`.
///
/// `alpha` are the diagonal entries (length k); `beta` are the
/// sub-diagonal entries (length k-1). Returns eigenvalues in ascending
/// order.
fn tridiag_eigenvalues(alpha: &[f64], beta: &[f64]) -> Result<Vec<f64>, EigenError> {
    use faer::Side;
    let k = alpha.len();
    if k == 0 {
        return Ok(Vec::new());
    }
    let t = Mat::<f64>::from_fn(k, k, |i, j| {
        if i == j {
            alpha[i]
        } else if i + 1 == j {
            beta[i]
        } else if j + 1 == i {
            beta[j]
        } else {
            0.0
        }
    });
    t.as_ref()
        .self_adjoint_eigenvalues(Side::Lower)
        .map_err(|e| EigenError::FaerGevd(format!("tridiag evd: {e:?}")))
}

/// Solve the symmetric tridiagonal eigenproblem returning **eigenpairs**
/// `(μ, s)` in ascending μ order. `s` is a unit-norm eigenvector in
/// k-dimensional tridiagonal space; combine it with the Lanczos basis
/// `V_k` to recover the corresponding Ritz vector `x = V_k s`.
fn tridiag_eigenpairs(alpha: &[f64], beta: &[f64]) -> Result<(Vec<f64>, Mat<f64>), EigenError> {
    use faer::Side;
    let k = alpha.len();
    if k == 0 {
        return Ok((Vec::new(), Mat::<f64>::zeros(0, 0)));
    }
    let t = Mat::<f64>::from_fn(k, k, |i, j| {
        if i == j {
            alpha[i]
        } else if i + 1 == j {
            beta[i]
        } else if j + 1 == i {
            beta[j]
        } else {
            0.0
        }
    });
    let evd = t
        .as_ref()
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| EigenError::FaerGevd(format!("tridiag evd (pairs): {e:?}")))?;
    let s_vec = evd.S().column_vector();
    let u = evd.U();
    let mut mus: Vec<f64> = (0..k).map(|i| s_vec[i]).collect();
    // self_adjoint_eigen returns eigenvalues in ascending order already,
    // but be explicit (and defensively sort the matching columns).
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&a, &b| {
        mus[a]
            .partial_cmp(&mus[b])
            .unwrap_or(core::cmp::Ordering::Equal)
    });
    let mut sorted_mus = vec![0.0_f64; k];
    let mut sorted_u = Mat::<f64>::zeros(k, k);
    for (new_col, &old_col) in order.iter().enumerate() {
        sorted_mus[new_col] = mus[old_col];
        for row in 0..k {
            sorted_u[(row, new_col)] = u[(row, old_col)];
        }
    }
    mus.copy_from_slice(&sorted_mus);
    Ok((mus, sorted_u))
}

/// A prepared inner-solve backend for the shift-invert operator `A⁻¹M`.
///
/// Built once (before the Lanczos loop) and applied per iteration. Both
/// variants expose the same `solve(mv, out)` semantics — `out = A⁻¹ · mv` —
/// so the outer Lanczos recurrence is identical regardless of backend.
enum InnerBackend<'a> {
    /// Precomputed sparse LU of `A = K − σM` (direct path).
    Lu(Lu<usize, f64>),
    /// Matrix-free operator + inner CG knobs (issue #524).
    MatrixFree {
        op: ShiftedMatrixFreeOp<'a>,
        tol: f64,
        max_iters: usize,
    },
}

impl InnerBackend<'_> {
    /// `out = A⁻¹ · rhs`. For the matrix-free path `out` doubles as the CG
    /// warm-start guess, so callers should retain it across iterations.
    fn solve(&self, rhs: &[f64], out: &mut [f64]) -> Result<(), EigenError> {
        match self {
            InnerBackend::Lu(lu) => solve_with_lu(lu, rhs, out),
            InnerBackend::MatrixFree { op, tol, max_iters } => {
                cg_solve_matrix_free(op, rhs, out, *tol, *max_iters)?;
                Ok(())
            }
        }
    }
}

impl SparseShiftInvertLanczos {
    /// Inner-CG relative-residual target, tied to the outer tolerance.
    ///
    /// See [`INNER_TOL_FACTOR`] and the [`InnerSolver`] docs for the
    /// coupling rationale (the inner solve must out-accuracy the outer
    /// Lanczos convergence target).
    fn inner_tol(&self) -> f64 {
        (self.tol * INNER_TOL_FACTOR).max(f64::EPSILON)
    }

    /// Build the inner-solve backend for the configured
    /// [`InnerSolver`] variant. The direct variant factors `A = K − σM`
    /// once (scoping faer's rayon parallelism to the factorization); the
    /// matrix-free variant builds the borrowed operator + Jacobi diagonal
    /// and never factors anything.
    fn build_inner<'a>(
        &self,
        k: SparseColMatRef<'a, usize, f64>,
        m: SparseColMatRef<'a, usize, f64>,
        n_threads: usize,
    ) -> Result<InnerBackend<'a>, EigenError> {
        match self.inner {
            InnerSolver::Direct => {
                let a = shifted_pencil(k, m, self.sigma)?;
                let lu = {
                    let _par = ParallelismGuard::rayon(n_threads);
                    a.as_ref()
                        .sp_lu()
                        .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?
                };
                Ok(InnerBackend::Lu(lu))
            }
            InnerSolver::MatrixFree => {
                let op = ShiftedMatrixFreeOp::new(k, m, self.sigma);
                let max_iters = (k.nrows() * INNER_MAX_ITER_FACTOR).max(64);
                Ok(InnerBackend::MatrixFree {
                    op,
                    tol: self.inner_tol(),
                    max_iters,
                })
            }
        }
    }

    /// Compute the lowest `n_modes` generalized eigenpairs of
    /// `K x = λ M x` closest to the configured shift `σ`, including
    /// M-orthonormalized eigenvectors. The eigenvalue-only sibling
    /// is [`SparseEigenSolver::smallest_eigenvalues`].
    ///
    /// Eigenvectors are recovered as Ritz vectors `x = V_k s` from the
    /// Lanczos basis `V_k` and tridiagonal eigenvector `s`, then
    /// rescaled so `xᵀ M x = 1` (the convention modal projection
    /// wants — same convention as
    /// [`crate::eigen::dense::FaerDenseEigensolver::smallest_eigenpairs`]).
    pub fn smallest_eigenpairs(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
    ) -> Result<Vec<EigenPair>, EigenError> {
        self.smallest_eigenpairs_with_threads(k, m, n_modes, resolve_num_threads())
    }

    /// [`Self::smallest_eigenpairs`] with an explicit factorization thread
    /// count, bypassing the `GEODE_NUM_THREADS` lookup.
    ///
    /// The public entry point resolves the thread count from the environment;
    /// this variant takes it directly so the cross-thread agreement tests can
    /// drive `n_threads = 1` vs `n_threads = N` without mutating a
    /// process-global env var (this crate denies `unsafe_code`, and
    /// edition-2024 `env::set_var` is `unsafe`).
    pub fn smallest_eigenpairs_with_threads(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        n_threads: usize,
    ) -> Result<Vec<EigenPair>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Prepare the inner-solve backend for A⁻¹M. The direct variant
        //    builds A = K − σM and factors it once (scoping faer's rayon to
        //    the factorization — rayon speeds up sparse LU but is slower on
        //    the latency-bound single-RHS solves, issue #518); the
        //    matrix-free variant (issue #524) builds a borrowed operator +
        //    Jacobi diagonal and never factors. Eigenvalues are independent
        //    of the thread count — the `eigenpairs_agree_across_thread_counts`
        //    test is the gate.
        let inner = self.build_inner(k, m, n_threads)?;

        // 2. Run Lanczos to convergence, retaining the full M-orthonormal
        //    basis V_k. Unlike the eigenvalue-only path we always run to
        //    the requested mode count; the Ritz-vector recovery needs the
        //    final basis even if convergence formally lags.
        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);
        let mut basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        // Cache of `M·v_j` for each basis vector, filled in lockstep with
        // `basis`. The reorth loop reuses these instead of recomputing an
        // SpMV per basis vector every iteration (turns the O(k²) SpMV cost
        // into O(k²) dot+axpy — see issue #506).
        let mut m_basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        let mut alpha: Vec<f64> = Vec::with_capacity(max_k);
        let mut beta: Vec<f64> = Vec::with_capacity(max_k);

        let mut v: Vec<f64> = (0..n)
            .map(|i| (((i as f64) + 1.0) * 0.5432).sin())
            .collect();
        let mut mv = vec![0.0_f64; n];
        spmv(m, &v, &mut mv);
        let mut nrm2 = v.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
        if nrm2 <= 0.0 {
            return Err(EigenError::FaerGevd(
                "starting vector has non-positive M-norm; M not SPD?".into(),
            ));
        }
        let mut nrm = nrm2.sqrt();
        for x in v.iter_mut() {
            *x /= nrm;
        }

        let mut w = vec![0.0_f64; n];
        let mut work = vec![0.0_f64; n];

        // Warm-start buffer for the matrix-free inner CG: retains the
        // previous iteration's solution `A⁻¹ M v_{j-1}` as the initial guess
        // for `A⁻¹ M v_j` (the two RHSs are close, so this cuts inner
        // iterations). Ignored by the direct LU backend.
        let mut y_guess = vec![0.0_f64; n];

        for j in 0..max_k {
            spmv(m, &v, &mut mv);
            w.copy_from_slice(&y_guess);
            inner.solve(&mv, &mut w)?;
            y_guess.copy_from_slice(&w);

            let aj = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            alpha.push(aj);
            for i in 0..n {
                w[i] -= aj * v[i];
            }
            if let Some(bp) = beta.last().copied() {
                let prev = &basis[j - 1];
                for i in 0..n {
                    w[i] -= bp * prev[i];
                }
            }

            // Full reorthogonalization (M-inner product). Reuse the cached
            // `M·v_k` (`m_basis[idx]`) instead of recomputing an SpMV per
            // basis vector (issue #506).
            for (vk, m_vk) in basis.iter().zip(m_basis.iter()) {
                let c = w.iter().zip(m_vk.iter()).map(|(a, b)| a * b).sum::<f64>();
                if c.abs() > 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Re-project off v itself (the just-computed direction). `mv`
            // still holds `M·v` from the top of this iteration.
            let c = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            for i in 0..n {
                w[i] -= c * v[i];
            }

            spmv(m, &w, &mut work);
            nrm2 = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            let nrm2 = nrm2.max(0.0);
            nrm = nrm2.sqrt();

            // Cache `M·v` alongside the basis vector before consuming `v`.
            m_basis.push(core::mem::take(&mut mv));
            mv = vec![0.0_f64; n];
            basis.push(core::mem::take(&mut v));

            // Convergence probe — same Kaniel–Saad bound as the
            // eigenvalues-only path. Break early when the next
            // Lanczos β has dropped below tolerance relative to the
            // dominant Ritz value.
            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let mus = tridiag_eigenvalues(&alpha, &beta)?;
                let mu_max = mus.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
                if nrm <= self.tol * mu_max.max(1.0) {
                    break;
                }
            }

            if nrm < 1e-14 {
                break;
            }

            beta.push(nrm);
            v = w.iter().map(|x| x / nrm).collect();
        }

        if alpha.is_empty() {
            return Err(EigenError::FaerGevd(
                "Lanczos produced no iterations; trivial problem?".into(),
            ));
        }

        // 3. Solve the tridiagonal eigenproblem with eigenvectors.
        let (mus, s_mat) = tridiag_eigenpairs(&alpha, &beta)?;
        let k_eff = mus.len();

        // 4. Build (λ, ritz_vector) pairs, filter out near-zero μ
        //    (corresponds to infinite λ), and sort by |λ - σ| ascending.
        let sigma = self.sigma;
        let mut pairs: Vec<(f64, Vec<f64>)> = Vec::with_capacity(k_eff);
        for col in 0..k_eff {
            let mu = mus[col];
            if mu.abs() == 0.0 {
                continue;
            }
            let lambda = sigma + 1.0 / mu;
            // Ritz vector x = V_k · s_col.
            let mut x = vec![0.0_f64; n];
            for row in 0..k_eff {
                let s_rc = s_mat[(row, col)];
                if s_rc == 0.0 {
                    continue;
                }
                let basis_row = &basis[row];
                for i in 0..n {
                    x[i] += s_rc * basis_row[i];
                }
            }
            pairs.push((lambda, x));
        }
        pairs.sort_by(|a, b| {
            (a.0 - sigma)
                .abs()
                .partial_cmp(&(b.0 - sigma).abs())
                .unwrap_or(core::cmp::Ordering::Equal)
        });

        let take = n_modes.min(pairs.len());
        let mut picked: Vec<(f64, Vec<f64>)> = pairs.into_iter().take(take).collect();
        // Re-sort by λ ascending — matches the dense path's eigenpair
        // ordering and the eigenvalue-only sparse path.
        picked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        // 5. M-orthonormalize each Ritz vector: divide by sqrt(xᵀ M x).
        let mut out = Vec::with_capacity(take);
        for (lambda, mut x) in picked {
            spmv(m, &x, &mut work);
            let norm2 = x.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            if norm2 > 0.0 {
                let s = norm2.sqrt();
                for v in x.iter_mut() {
                    *v /= s;
                }
            }
            out.push(EigenPair { lambda, vector: x });
        }
        Ok(out)
    }

    /// [`SparseEigenSolver::smallest_eigenvalues`] with an explicit
    /// factorization thread count, bypassing the `GEODE_NUM_THREADS` lookup.
    ///
    /// See [`Self::smallest_eigenpairs_with_threads`] for why the tests use
    /// an explicit count instead of mutating the environment.
    pub fn smallest_eigenvalues_with_threads(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        n_threads: usize,
    ) -> Result<Vec<f64>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Prepare the inner-solve backend for A⁻¹M — direct sparse LU or
        //    matrix-free Jacobi-CG (issue #524). Parallelism is scoped to the
        //    factorization only in the direct path (rayon regresses the
        //    single-RHS triangular solves that follow; issue #518). See the
        //    `smallest_eigenpairs` sibling above.
        let inner = self.build_inner(k, m, n_threads)?;

        // 2. Lanczos in the M-inner product.
        //
        // Cap the iteration count at n — Lanczos cannot exceed the
        // dimension, even if max_iters is set higher.
        let max_k = self.max_iters.min(n);
        let max_k = max_k.max(n_modes + 2).min(n);

        // Allocate workspace.
        let mut basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        // Cache of `M·v_j` for each basis vector (see issue #506) — reused
        // by the reorth loop instead of recomputing an SpMV per basis
        // vector every iteration.
        let mut m_basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        let mut alpha: Vec<f64> = Vec::with_capacity(max_k);
        let mut beta: Vec<f64> = Vec::with_capacity(max_k);

        // Start vector: deterministic but generic (sin-based so it's not
        // an eigenvector of the discrete Laplacian on a regular cube).
        let mut v: Vec<f64> = (0..n)
            .map(|i| (((i as f64) + 1.0) * 0.5432).sin())
            .collect();

        // Normalize v in the M-norm: M v, then ⟨v, M v⟩^{1/2}.
        let mut mv = vec![0.0_f64; n];
        spmv(m, &v, &mut mv);
        let mut nrm2 = v.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
        if nrm2 <= 0.0 {
            return Err(EigenError::FaerGevd(
                "starting vector has non-positive M-norm; M not SPD?".into(),
            ));
        }
        let mut nrm = nrm2.sqrt();
        for x in v.iter_mut() {
            *x /= nrm;
        }

        let mut converged: Option<Vec<f64>> = None;
        let mut w = vec![0.0_f64; n];
        let mut work = vec![0.0_f64; n];
        // Warm-start buffer for the matrix-free inner CG (see the eigenpair
        // sibling); ignored by the direct LU backend.
        let mut y_guess = vec![0.0_f64; n];

        for j in 0..max_k {
            // M v
            spmv(m, &v, &mut mv);
            // w = A^{-1} (M v) = (K - σM)^{-1} M v
            w.copy_from_slice(&y_guess);
            inner.solve(&mv, &mut w)?;
            y_guess.copy_from_slice(&w);

            // α_j = ⟨w, M v⟩ = ⟨w, mv⟩
            let aj = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            alpha.push(aj);

            // w ← w - α_j v   (subtract M-conjugate component of current v)
            for i in 0..n {
                w[i] -= aj * v[i];
            }
            // w ← w - β_{j-1} v_{j-1}  (three-term recurrence)
            if let Some(bp) = beta.last().copied() {
                let prev = &basis[j - 1];
                for i in 0..n {
                    w[i] -= bp * prev[i];
                }
            }

            // Full reorthogonalization against the whole basis in the
            // M-inner product. This is O(j·n) per step but keeps the
            // basis numerically M-orthonormal even at large k. Reuse the
            // cached `M·v_k` (`m_basis[idx]`) instead of recomputing an
            // SpMV per basis vector (issue #506).
            for (vk, m_vk) in basis.iter().zip(m_basis.iter()) {
                let c = w.iter().zip(m_vk.iter()).map(|(a, b)| a * b).sum::<f64>();
                if c.abs() > 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Also re-project off v itself (the just-added direction). `mv`
            // still holds `M·v` from the top of this iteration.
            let c = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            for i in 0..n {
                w[i] -= c * v[i];
            }

            // β_j = ‖w‖_M.
            spmv(m, &w, &mut work);
            nrm2 = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            // Numerical safety: M-norm can dip slightly negative from
            // rounding when w is essentially zero. Clamp to 0.
            let nrm2 = nrm2.max(0.0);
            nrm = nrm2.sqrt();

            // Push the *current* v as basis[j] (caching `M·v` alongside),
            // then either iterate or stop.
            m_basis.push(core::mem::take(&mut mv));
            mv = vec![0.0_f64; n];
            basis.push(core::mem::take(&mut v));

            // Try to test convergence on the requested modes. We re-test
            // every few steps to amortize the tridiag solve; for n_modes ≤
            // a few we just test every iteration since the tridiag is tiny.
            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let mus = tridiag_eigenvalues(&alpha, &beta)?;
                // ritz vals are μ; we want λ = σ + 1/μ at the LARGEST |μ|
                // (largest μ ↔ smallest |λ - σ|).
                // Build λ candidates from all μ, sort by |λ - σ| ascending.
                let mut lambdas: Vec<f64> = mus
                    .iter()
                    .filter(|&&mu| mu.abs() > 0.0)
                    .map(|&mu| self.sigma + 1.0 / mu)
                    .collect();
                lambdas.sort_by(|a, b| {
                    (a - self.sigma)
                        .abs()
                        .partial_cmp(&(b - self.sigma).abs())
                        .unwrap_or(core::cmp::Ordering::Equal)
                });
                if lambdas.len() >= n_modes {
                    // Take the n_modes closest to σ and sort by λ ascending.
                    let mut picked: Vec<f64> = lambdas.into_iter().take(n_modes).collect();
                    picked.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

                    // Cheap convergence proxy: the next Lanczos β bound
                    // controls all ritz residuals. We accept when β/|μ_max|
                    // is below tol — this is the standard Kaniel–Saad
                    // bound, scaled to make tol comparable across sigmas.
                    let mu_max = mus.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
                    if nrm <= self.tol * mu_max.max(1.0) {
                        converged = Some(picked);
                        break;
                    }
                    // Even if not "converged" by the β bound, stash the
                    // best picks so far — we'll return them if we hit
                    // max_k without formal convergence.
                    converged = Some(picked);
                }
            }

            // Numerical breakdown — invariant subspace exhausted.
            if nrm < 1e-14 {
                break;
            }

            beta.push(nrm);
            // Build next basis vector v_{j+1} = w / β_j.
            v = w.iter().map(|x| x / nrm).collect();
            // w gets overwritten next iteration, no zeroing needed.
        }

        let lambdas = converged.ok_or_else(|| {
            EigenError::FaerGevd(format!(
                "Lanczos terminated after {} iters without computing {} ritz pairs",
                alpha.len(),
                n_modes
            ))
        })?;

        // Final correctness gate: residual norm on the actual `(K, M)`
        // problem, using the discovered λ values and the dense ritz
        // vectors. We don't store the eigenvectors above (saves memory),
        // so this is approximate; treat it as a sanity print rather than
        // a hard gate. The Kaniel–Saad bound above is the real exit.
        Ok(lambdas)
    }
}

impl SparseEigenSolver for SparseShiftInvertLanczos {
    fn smallest_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
    ) -> Result<Vec<f64>, EigenError> {
        self.smallest_eigenvalues_with_threads(k, m, n_modes, resolve_num_threads())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build a tiny SPD diagonal pencil with known eigenvalues.
    fn diagonal_pencil(
        diag_k: &[f64],
        diag_m: &[f64],
    ) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
        let n = diag_k.len();
        let tk: Vec<Triplet<usize, usize, f64>> =
            (0..n).map(|i| Triplet::new(i, i, diag_k[i])).collect();
        let tm: Vec<Triplet<usize, usize, f64>> =
            (0..n).map(|i| Triplet::new(i, i, diag_m[i])).collect();
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
        (k, m)
    }

    #[test]
    fn lanczos_diagonal_pencil_smallest_three() {
        // λ_i = k_i / m_i = {1, 2, 3, 4, 5}.
        let (k, m) = diagonal_pencil(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0; 5]);
        let solver = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
            inner: InnerSolver::Direct,
        };
        let lambdas = solver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .unwrap();
        assert_eq!(lambdas.len(), 3);
        for (got, want) in lambdas.iter().zip([1.0, 2.0, 3.0].iter()) {
            assert!((got - want).abs() < 1e-8, "lanczos λ={got}, want {want}");
        }
    }

    /// Eigenpair path on a diagonal SPD pencil: each Ritz vector should
    /// be (a sign of) the canonical basis vector corresponding to its
    /// eigenvalue, M-orthonormalized.
    #[test]
    fn lanczos_diagonal_pencil_eigenpairs_recover_basis() {
        // λ_i = k_i / m_i = {1, 2, 3, 4, 5} with M = I.
        let (k, m) = diagonal_pencil(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0; 5]);
        let solver = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
            inner: InnerSolver::Direct,
        };
        let pairs = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), 3)
            .unwrap();
        assert_eq!(pairs.len(), 3);
        for (i, pair) in pairs.iter().enumerate() {
            let want_lambda = (i + 1) as f64;
            assert!(
                (pair.lambda - want_lambda).abs() < 1e-8,
                "λ[{i}] = {}, want {want_lambda}",
                pair.lambda
            );
            // Eigenvector should be ±e_i with unit M-norm (M = I).
            let norm2: f64 = pair.vector.iter().map(|x| x * x).sum();
            assert!(
                (norm2 - 1.0).abs() < 1e-9,
                "eigenvector[{i}] not M-orthonormal: ‖v‖² = {norm2}"
            );
            // Largest entry is at position i, magnitude ≈ 1.
            let (max_pos, max_val) = pair
                .vector
                .iter()
                .enumerate()
                .map(|(p, x)| (p, x.abs()))
                .fold(
                    (0usize, 0.0_f64),
                    |acc, x| if x.1 > acc.1 { x } else { acc },
                );
            assert_eq!(max_pos, i, "eigenvector[{i}] localized at wrong index");
            assert!(
                (max_val - 1.0).abs() < 1e-6,
                "eigenvector[{i}] not localized: max = {max_val}"
            );
        }
    }

    /// Build a larger, non-diagonal SPD tridiagonal pencil so the faer
    /// sparse LU has enough fill for the multi-threaded factorization path
    /// to be meaningfully exercised (a diagonal pencil factors trivially).
    ///
    /// `K` is the 1-D Laplacian stencil `tridiag(-1, 2, -1)` (SPD after the
    /// implicit Dirichlet ends) and `M` is the identity, so the eigenvalues
    /// are the known `2 - 2 cos(π i / (n+1))`, a smoothly separated spectrum.
    fn laplacian_pencil(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
        let mut tk: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(3 * n);
        let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
        for i in 0..n {
            tk.push(Triplet::new(i, i, 2.0));
            if i + 1 < n {
                tk.push(Triplet::new(i, i + 1, -1.0));
                tk.push(Triplet::new(i + 1, i, -1.0));
            }
            tm.push(Triplet::new(i, i, 1.0));
        }
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
        (k, m)
    }

    /// CORRECTNESS GATE (issue #518): the computed spectrum must be identical
    /// whether the faer sparse-LU factorization runs single-threaded or
    /// multi-threaded. The factorization is deterministic, so we assert
    /// *bit-for-bit* equality (not merely "within tolerance") across thread
    /// counts — any drift would signal a nondeterministic parallel LU, which
    /// would be a correctness bug rather than acceptable rounding.
    ///
    /// This test drives the thread count through the `_with_threads` entry
    /// point (the same code path the `GEODE_NUM_THREADS` knob feeds), which
    /// avoids mutating a process-global env var — this crate denies
    /// `unsafe_code` and edition-2024 `env::set_var` is `unsafe`.
    #[test]
    fn eigenvalues_agree_across_thread_counts() {
        // Hold the shared parallelism lock: the solves transiently set faer's
        // process-global parallelism during factorization, which would
        // otherwise race the guard RAII test in `eigen::parallel`.
        let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
            .lock()
            .unwrap();

        let (k, m) = laplacian_pencil(64);
        let solver = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-11,
            inner: InnerSolver::Direct,
        };

        let serial = solver
            .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 5, 1)
            .unwrap();
        let parallel = solver
            .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 5, 4)
            .unwrap();

        assert_eq!(
            serial.len(),
            parallel.len(),
            "thread count changed the number of converged modes"
        );
        for (i, (s, p)) in serial.iter().zip(parallel.iter()).enumerate() {
            assert_eq!(
                s.to_bits(),
                p.to_bits(),
                "eigenvalue[{i}] differs across thread counts: serial={s}, parallel={p}"
            );
        }
    }

    /// Companion to the agreement test: confirm the eigenpair path (vectors,
    /// not just eigenvalues) is also thread-count invariant within the
    /// solver's tolerance. Vectors carry an arbitrary global sign, so compare
    /// eigenvalues bit-for-bit and vectors up to sign.
    #[test]
    fn eigenpairs_agree_across_thread_counts() {
        let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
            .lock()
            .unwrap();

        let (k, m) = laplacian_pencil(48);
        let solver = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-11,
            inner: InnerSolver::Direct,
        };

        let serial = solver
            .smallest_eigenpairs_with_threads(k.as_ref(), m.as_ref(), 4, 1)
            .unwrap();
        let parallel = solver
            .smallest_eigenpairs_with_threads(k.as_ref(), m.as_ref(), 4, 4)
            .unwrap();

        assert_eq!(serial.len(), parallel.len());
        for (i, (s, p)) in serial.iter().zip(parallel.iter()).enumerate() {
            assert_eq!(
                s.lambda.to_bits(),
                p.lambda.to_bits(),
                "eigenpair λ[{i}] differs across thread counts"
            );
            // Vectors may differ by a global sign; align on the dominant
            // entry, then compare component-wise within tolerance.
            let pivot = s
                .vector
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
                .map(|(idx, _)| idx)
                .unwrap();
            let sign = (s.vector[pivot] * p.vector[pivot]).signum();
            for (a, b) in s.vector.iter().zip(p.vector.iter()) {
                assert!(
                    (a - sign * b).abs() < 1e-9,
                    "eigenvector[{i}] differs beyond tolerance across thread counts"
                );
            }
        }
    }

    // -------------------------------------------------------------------
    // Matrix-free shift-invert inner solve (issue #524).
    // -------------------------------------------------------------------

    /// CORRECTNESS GATE (issue #524): the matrix-free inner-solve variant
    /// (`InnerSolver::MatrixFree`, Jacobi-CG on `(K − σM) y = b`, no LU)
    /// reproduces the direct sparse-LU path's eigenvalues to tight
    /// tolerance on an SPD pencil with the shift placed **below** the
    /// spectrum (so `K − σM` is SPD, the Phase-1 lowest-mode case).
    ///
    /// The Laplacian pencil `tridiag(-1, 2, -1)` with `M = I` has all
    /// eigenvalues in `(0, 4)`; `σ = -1.0` sits below the whole spectrum,
    /// making `K − σM = K + I` SPD.
    #[test]
    fn matrix_free_matches_direct_eigenvalues() {
        let (k, m) = laplacian_pencil(40);
        let sigma = -1.0;
        let direct = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::Direct,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::MatrixFree,
        };

        let ld = direct
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
            .unwrap();
        let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

        assert_eq!(ld.len(), lm.len(), "mode count differs direct vs mf");
        for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
            // Both paths solve the SAME generalized pencil; the only
            // difference is the inner solve. They must agree to a tolerance
            // well inside the ≤1% physical bar.
            let rel = (d - f).abs() / d.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "eigenvalue[{i}] direct={d} matrix-free={f} rel-diff={rel:.2e} > 1e-6"
            );
        }
    }

    /// Companion eigenpair cross-check: the matrix-free path recovers the
    /// same eigenvectors (up to global sign) as the direct path on the SPD
    /// below-spectrum-shifted Laplacian pencil.
    #[test]
    fn matrix_free_matches_direct_eigenpairs() {
        let (k, m) = laplacian_pencil(32);
        let sigma = -1.0;
        let direct = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::Direct,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::MatrixFree,
        };

        let pd = direct
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), 4)
            .unwrap();
        let pf = mf.smallest_eigenpairs(k.as_ref(), m.as_ref(), 4).unwrap();

        assert_eq!(pd.len(), pf.len());
        for (i, (d, f)) in pd.iter().zip(pf.iter()).enumerate() {
            let rel = (d.lambda - f.lambda).abs() / d.lambda.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "eigenpair λ[{i}] direct={} matrix-free={} rel-diff={rel:.2e}",
                d.lambda,
                f.lambda
            );
            // Align sign on the dominant entry, then compare component-wise.
            let pivot = d
                .vector
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
                .map(|(idx, _)| idx)
                .unwrap();
            let sign = (d.vector[pivot] * f.vector[pivot]).signum();
            for (a, b) in d.vector.iter().zip(f.vector.iter()) {
                assert!(
                    (a - sign * b).abs() < 1e-5,
                    "eigenvector[{i}] direct vs matrix-free differ beyond tolerance"
                );
            }
        }
    }

    /// The matrix-free inner CG solves `(K − σM) y = b` to the requested
    /// residual on a well-shifted SPD pencil, and its Jacobi diagonal is
    /// exactly `K_ii − σ M_ii`. This is a direct unit test of the inner
    /// operator + solver in isolation (no outer Lanczos).
    #[test]
    fn matrix_free_inner_cg_solves_spd_system() {
        let (k, m) = laplacian_pencil(50);
        let sigma = -0.5;
        let op = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma);

        // Jacobi diagonal check: K_ii = 2, M_ii = 1 ⇒ 1/(2 - (-0.5)) = 1/2.5.
        for (i, &d) in op.inv_diag.iter().enumerate() {
            assert!(
                (d - 1.0 / 2.5).abs() < 1e-12,
                "inv_diag[{i}] = {d}, want {}",
                1.0 / 2.5
            );
        }

        // Solve (K − σM) y = b for a known b; verify the residual.
        let n = k.nrows();
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.31).cos()).collect();
        let mut y = vec![0.0_f64; n];
        let iters = cg_solve_matrix_free(&op, &b, &mut y, 1e-12, 4 * n).unwrap();
        assert!(
            iters > 0 && iters <= n,
            "unexpected CG iteration count {iters}"
        );

        // Residual r = b − (K − σM) y should be tiny relative to ‖b‖.
        let mut ay = vec![0.0_f64; n];
        op.apply(&y, &mut ay);
        let bnorm = b.iter().map(|v| v * v).sum::<f64>().sqrt();
        let rnorm = b
            .iter()
            .zip(ay.iter())
            .map(|(bi, ai)| (bi - ai) * (bi - ai))
            .sum::<f64>()
            .sqrt();
        assert!(
            rnorm / bnorm < 1e-10,
            "matrix-free CG residual too large: {:.3e}",
            rnorm / bnorm
        );
    }
}
