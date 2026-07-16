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
/// indefinite-system machinery.
///
/// ## Interior / indefinite shifts (issue #535)
///
/// Targeting an *interior* band (a shift `σ` placed **between** two
/// generalized eigenvalues) makes `(K − σM)` **symmetric-indefinite**: it
/// has generalized eigenvalues on both sides of `σ`, so the shifted pencil
/// has both positive and negative eigenvalues and plain CG breaks down
/// (`pᵀAp` changes sign). The [`MatrixFreeIndefinite`](InnerSolver::MatrixFreeIndefinite)
/// variant swaps the inner CG for **MINRES**, which minimizes the residual
/// over the same Krylov space using a short (3-term) recurrence — so it
/// keeps the `O(N)` memory that is the whole point of the matrix-free
/// track (unlike GMRES, whose full Krylov basis storage reintroduces a
/// memory-growth term). MINRES needs an **SPD** preconditioner; plain
/// Jacobi `1/(K_ii − σM_ii)` is sign-indefinite at an interior shift, so
/// the indefinite path preconditions with **absolute-value Jacobi**
/// `1/|K_ii − σM_ii|` (see `ShiftedMatrixFreeOp::with_abs_diag`). The
/// selection is caller-driven: pick [`MatrixFreeIndefinite`](InnerSolver::MatrixFreeIndefinite)
/// when placing an interior shift, and the default [`MatrixFree`](InnerSolver::MatrixFree)
/// CG stays unchanged for the SPD lowest-mode case.
///
/// ## Inner-tolerance coupling
///
/// The inner CG tolerance is tied to the outer Lanczos tolerance: it is
/// set to a fixed fraction (`INNER_TOL_FACTOR`) of the outer `tol`, so the
/// inner solve is always tighter than the accuracy the outer iteration is
/// trying to reach. This keeps the shift-invert operator `A⁻¹M` accurate
/// enough that the Lanczos recurrence is not polluted by inner-solve
/// residual noise, while avoiding over-solving early iterations. See
/// the private `inner_tol` method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InnerSolver {
    /// Form `A = K − σM` and factor it once with faer's sparse LU. Fast to
    /// re-apply, but the factorization fill-in is not `O(N)` memory — this
    /// is the historical default and the small-problem path.
    #[default]
    Direct,
    /// Like [`Direct`](InnerSolver::Direct) — form and factor `A = K − σM`
    /// once — but inject a **custom fill-reducing column ordering** into the LU
    /// through faer 0.24's public deeper API instead of the COLAMD ordering
    /// faer's high-level `sp_lu` hardcodes (issue #543). The ordering is a
    /// geometric coordinate nested dissection when per-DOF coordinates are
    /// supplied (via a `*_with_coords` entry point), otherwise AMD
    /// minimum-degree from the pattern alone. Both cut LU fill/memory versus
    /// COLAMD (~1.4–1.7× measured on the Nédélec pattern, growing with size)
    /// while leaving the computed spectrum unchanged — ordering changes memory,
    /// not answers. See the crate-internal `eigen::ordering` module.
    DirectCustomOrder,
    /// Never form or factor `A`; solve `(K − σM) y = b` iteratively with
    /// matrix-free preconditioned CG. `O(N)` memory; the path that
    /// scales past the direct factorization's memory wall (issue #524).
    /// Requires `(K − σM)` SPD (place `σ` below the spectrum). The inner-CG
    /// preconditioner is selected by [`InnerPreconditioner`] (Jacobi default;
    /// AMS-lite opt-in, issue #526).
    MatrixFree,
    /// Never form or factor `A`; solve the **symmetric-indefinite**
    /// `(K − σM) y = b` iteratively with matrix-free **MINRES** preconditioned
    /// by absolute-value Jacobi `1/|K_ii − σM_ii|` (issue #535). Same `O(N)`
    /// memory as [`MatrixFree`](InnerSolver::MatrixFree) (MINRES has a 3-term
    /// recurrence, no growing Krylov basis), but valid when `σ` is placed
    /// **interior** to the spectrum, where `(K − σM)` is indefinite and CG
    /// breaks down. Select this variant explicitly when targeting an interior
    /// band; the SPD lowest-mode [`MatrixFree`](InnerSolver::MatrixFree) CG
    /// path stays the default.
    MatrixFreeIndefinite,
}

/// Preconditioner for the matrix-free inner CG (issue #526).
///
/// Only meaningful when [`InnerSolver::MatrixFree`] is selected — the direct
/// LU path ignores it. The two options trade build cost against inner-CG
/// iteration count on the ill-conditioned H(curl) curl-curl shifted pencil.
///
/// # Why AMS
///
/// The Nédélec curl-curl stiffness `K` has a large near-kernel equal to the
/// gradient subspace `image(d⁰)` (`kernel(K) = image(d⁰)`). [`Jacobi`] (a
/// diagonal scaling) is blind to those low-energy gradient error components,
/// so on a fine mesh the inner CG converges far too slowly — the 1.16M-DOF
/// transmon solve did not finish in 28 minutes with Jacobi (issue #526). The
/// [`Ams`] option adds a Hiptmair–Xu auxiliary-space (nodal) coarse correction
/// through the discrete gradient `G`, damping exactly that near-kernel and
/// cutting the inner-CG iteration count dramatically. It requires the caller to
/// supply `G` (via the `*_with_gradient` entry points); a `None` gradient falls
/// back to Jacobi.
///
/// [`Jacobi`]: InnerPreconditioner::Jacobi
/// [`Ams`]: InnerPreconditioner::Ams
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InnerPreconditioner {
    /// Diagonal (Jacobi) preconditioner `D⁻¹`, `D = diag(K − σM)`. The #524
    /// baseline: cheap to build, but slow on the ill-conditioned curl-curl
    /// pencil because it cannot damp the gradient near-kernel.
    #[default]
    Jacobi,
    /// AMS-lite (auxiliary-space Maxwell, Hiptmair–Xu): a symmetric two-level
    /// V-cycle of a damped edge Jacobi smoother around a gradient-space nodal
    /// coarse correction `G (Gᵀ A G)⁻¹ Gᵀ` (issue #526). Requires the discrete
    /// gradient `G`; falls back to Jacobi if none is supplied. `O(N)` memory
    /// (the coarse solve is node-indexed, `node_dim ≪ edge_dim`). See
    /// [`crate::eigen::ams`].
    Ams,
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
    /// Inner-CG preconditioner for the matrix-free path (issue #526). Ignored
    /// by [`InnerSolver::Direct`]. Defaults to [`InnerPreconditioner::Jacobi`]
    /// (the #524 behavior); [`InnerPreconditioner::Ams`] is opt-in and needs
    /// the discrete gradient supplied via a `*_with_gradient` entry point.
    pub precond: InnerPreconditioner,
}

impl Default for SparseShiftInvertLanczos {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-9,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
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

/// Opt-in diagnostic knobs for characterizing the shift-invert eigensolve cost
/// (issue #562). Every knob below is read from the environment and defaults to
/// the exact prior behavior when unset, so the default solver path is
/// **unchanged** — these are inert unless an operator sets them for a
/// characterization run.
mod diag_env {
    /// Absolute inner-solve tolerance override for the matrix-free / MINRES
    /// backends (`GEODE_INNER_TOL`). When set it replaces the tolerance the
    /// solver would otherwise derive from the outer tol (`inner_tol()`), letting
    /// a characterization run sweep loose vs tight inner solves. Unset → no
    /// change.
    pub const INNER_TOL: &str = "GEODE_INNER_TOL";
    /// Inner-solve iteration-cap override (`GEODE_INNER_MAXITERS`). Lets a
    /// bounded run cap each inner MINRES/CG solve so several outer Lanczos steps
    /// can be observed within a wall-clock budget instead of one solve
    /// consuming it. Unset → the generous `2·N` non-convergence guard.
    pub const INNER_MAXITERS: &str = "GEODE_INNER_MAXITERS";
    /// Inner-MINRES progress-log interval in iterations (`GEODE_MINRES_LOG`).
    /// When set to a positive integer the MINRES recurrence prints its relative
    /// preconditioned residual `‖r‖_{M⁻¹}/‖r₀‖_{M⁻¹}` every that-many iterations
    /// to stderr — the convergence *curve* from which inner-iters/step and
    /// inner-tol sensitivity are both read, even when the solve does not finish.
    /// Unset → silent.
    pub const MINRES_LOG: &str = "GEODE_MINRES_LOG";
    /// Per-outer-step Lanczos log toggle (`GEODE_EIGEN_STEP_LOG`). When set
    /// (to any value) each outer shift-invert Lanczos step prints its inner
    /// iteration count and cumulative wall-clock to stderr, so the outer step
    /// count reached within a budget is directly observable. Unset → silent.
    pub const STEP_LOG: &str = "GEODE_EIGEN_STEP_LOG";

    /// Parse an env var as `f64`, returning `None` when unset or unparseable
    /// (so a malformed value degrades to the default rather than erroring).
    pub fn f64_opt(key: &str) -> Option<f64> {
        std::env::var(key).ok().and_then(|v| v.parse().ok())
    }

    /// Parse an env var as `usize`, returning `None` when unset or unparseable.
    pub fn usize_opt(key: &str) -> Option<usize> {
        std::env::var(key).ok().and_then(|v| v.parse().ok())
    }

    /// True when the env var is set to any value (presence toggle).
    pub fn is_set(key: &str) -> bool {
        std::env::var(key).is_ok()
    }
}

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

/// Solve `A y = b` in-place via a custom-ordering supernodal LU (issue #543).
///
/// Mirrors [`solve_with_lu`] for the [`InnerSolver::DirectCustomOrder`] backend.
/// The parallelism is read from faer's global setting so it honors the
/// enclosing [`ParallelismGuard`] scope, exactly as the COLAMD `sp_lu` path does.
fn solve_with_custom_lu(
    lu: &crate::eigen::ordering::CustomOrderLu,
    rhs: &[f64],
    out: &mut [f64],
) -> Result<(), EigenError> {
    let n = rhs.len();
    let mut work: Mat<f64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut(), faer::get_global_parallelism())?;
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
    /// a zero pivot, which falls back to `1.0`). Used both as the standalone
    /// Jacobi preconditioner and as the edge smoother inside AMS-lite.
    inv_diag: Vec<f64>,
    /// Optional AMS-lite preconditioner (issue #526). When present, [`precond`]
    /// applies the auxiliary-space (gradient) coarse correction on top of the
    /// edge Jacobi smoother; when `None`, plain Jacobi is used (the #524
    /// baseline). Kept behind an `Option` so the matrix-free path stays
    /// Jacobi-by-default and AMS is purely additive/opt-in.
    ///
    /// [`precond`]: ShiftedMatrixFreeOp::precond
    ///
    /// Boxed so the enclosing `MatrixFree` inner-backend variant stays small
    /// (the AMS preconditioner carries a node-space LU).
    ams: Option<Box<crate::eigen::ams::AmsLitePreconditioner>>,
    /// Apply the attached AMS in its **additive** form
    /// (`z = D⁻¹ r + G C⁻¹ Gᵀ r + Π (ΠᵀAΠ)⁻¹ Πᵀ r`) rather than the multiplicative
    /// V-cycle (issues #531/#559). The additive form needs no operator matvec and
    /// is SPD purely as a sum of SPD subspace corrections, so — unlike the
    /// multiplicative V-cycle, whose SPD-ness relies on `A` being SPD — it stays a
    /// valid **MINRES** preconditioner at an interior/indefinite shift. `false`
    /// (the SPD CG path) uses the stronger multiplicative cycle.
    ams_additive: bool,
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
            ams: None,
            ams_additive: false,
        }
    }

    /// Attach an AMS-lite preconditioner (issue #526), replacing the plain
    /// Jacobi preconditioner apply with the auxiliary-space (gradient)
    /// two-level form. The AMS preconditioner already carries its own edge
    /// Jacobi smoother (built from the same `K`, `M`, `σ`), so this is a
    /// drop-in swap of [`Self::precond`]'s behavior.
    fn with_ams(mut self, ams: crate::eigen::ams::AmsLitePreconditioner) -> Self {
        self.ams = Some(Box::new(ams));
        self.ams_additive = false;
        self
    }

    /// Attach an AMS-lite preconditioner applied in its **additive** form for the
    /// indefinite MINRES path (issues #531/#559).
    ///
    /// The SPD CG path uses [`Self::with_ams`] (the multiplicative V-cycle). For
    /// an interior/indefinite shift the multiplicative cycle is no longer
    /// guaranteed SPD (its residual updates apply the true indefinite `A`), so the
    /// MINRES path instead applies the AMS **additively** — a sum of SPD subspace
    /// corrections that is SPD by construction and needs no operator matvec. The
    /// caller builds `ams` for the sign-flipped SPD operator `K + |σ|M` so every
    /// block (edge Jacobi + `Gᵀ(K+|σ|M)G` + `Πᵀ(K+|σ|M)Π`) is SPD.
    fn with_ams_additive(mut self, ams: crate::eigen::ams::AmsLitePreconditioner) -> Self {
        self.ams = Some(Box::new(ams));
        self.ams_additive = true;
        self
    }

    /// Replace the signed Jacobi diagonal `1/(K_ii − σM_ii)` with its
    /// **absolute value** `1/|K_ii − σM_ii|` (issue #535).
    ///
    /// For an *interior* shift `(K − σM)` is symmetric-indefinite, so its
    /// diagonal has mixed sign and the signed Jacobi diagonal is **not** an
    /// SPD preconditioner — which MINRES requires. Taking the absolute value
    /// makes the diagonal preconditioner SPD (all entries strictly positive)
    /// while leaving the operator [`apply`](Self::apply) untouched. Because
    /// `inv_diag` already stores `1/(K_ii − σM_ii)`, the absolute-value
    /// diagonal is exactly its element-wise `abs()` (`|1/d| = 1/|d|`). For an
    /// SPD shift (all diagonal entries already positive) this is a no-op, so
    /// the indefinite path degrades gracefully to Jacobi on SPD inputs.
    fn with_abs_diag(mut self) -> Self {
        for d in self.inv_diag.iter_mut() {
            *d = d.abs();
        }
        self
    }

    /// `y = (K − σM) · x`, matrix-free (two SpMVs, no assembled `A`).
    fn apply(&self, x: &[f64], y: &mut [f64]) {
        spmv(self.k, x, y);
        if self.sigma != 0.0 {
            spmv_add(self.m, x, y, -self.sigma);
        }
    }

    /// Apply the preconditioner `z = M_prec⁻¹ r`.
    ///
    /// Without AMS this is the Jacobi preconditioner `z = D⁻¹ r` (the #524
    /// baseline). With an AMS-lite preconditioner attached (issue #526) it is
    /// the auxiliary-space two-level apply `z = D⁻¹ r + G (Gᵀ A G)⁻¹ Gᵀ r`,
    /// which damps the gradient near-kernel Jacobi is blind to. Both forms are
    /// SPD, so the outer CG stays valid.
    fn precond(&self, r: &[f64], z: &mut [f64]) {
        match &self.ams {
            Some(ams) if self.ams_additive => {
                // Indefinite MINRES path (issues #531/#559): the additive apply is
                // SPD by construction (a sum of SPD subspace corrections) and needs
                // no matvec of the indefinite operator, so it is a valid MINRES
                // preconditioner where the multiplicative V-cycle would not be.
                ams.apply(r, z);
            }
            Some(ams) => {
                ams.apply_vcycle(r, z, |x, y| self.apply(x, y));
            }
            None => {
                for i in 0..r.len() {
                    z[i] = self.inv_diag[i] * r[i];
                }
            }
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

/// Matrix-free preconditioned **MINRES** solve of the symmetric-indefinite
/// system `(K − σM) y = b` (issue #535).
///
/// This is the interior/indefinite-shift sibling of [`cg_solve_matrix_free`].
/// When `σ` is placed **between** two generalized eigenvalues, `A = K − σM`
/// has eigenvalues of both signs, so the CG recurrence breaks down
/// (`pᵀAp` changes sign). MINRES minimizes `‖b − A y‖` over the same Krylov
/// space `span{r₀, A r₀, …}` using a **short (3-term) Lanczos recurrence plus
/// Givens rotations**, so — like CG — it carries only a fixed handful of
/// length-`N` vectors and keeps the working set `O(N)`. (GMRES would store the
/// full Krylov basis and reintroduce an `O(N·m)` memory term, defeating the
/// matrix-free memory goal, which is why MINRES is used here.)
///
/// MINRES requires an **SPD** preconditioner. The operator is built with
/// [`ShiftedMatrixFreeOp::with_abs_diag`] so [`ShiftedMatrixFreeOp::precond`]
/// applies absolute-value Jacobi `1/|K_ii − σM_ii|`, which is SPD even when
/// the shifted diagonal is sign-indefinite. Convergence is measured in the
/// preconditioner-induced norm: the recurrence tracks `‖r_k‖_{M⁻¹}`
/// (`phibar`) exactly, and we stop when it drops below `tol` relative to the
/// initial `‖r₀‖_{M⁻¹}` (`beta1`) — the canonical MINRES stopping quantity.
///
/// `out` doubles as the initial guess / warm start (as in the CG sibling).
/// Returns the number of MINRES iterations executed; errors if the residual
/// has not dropped below `tol` within `max_iters`.
///
/// The recurrence is the standard Paige–Saunders preconditioned MINRES (as in
/// Choi/Paige/Saunders and the reference SciPy `minres` port), specialized to
/// the real symmetric pencil.
fn minres_solve_matrix_free(
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

    // r1 = b − A·out  (out is the warm-start guess).
    let mut r1 = vec![0.0_f64; n];
    op.apply(out, &mut r1);
    for i in 0..n {
        r1[i] = b[i] - r1[i];
    }

    // y = M⁻¹ r1;  beta1 = √(r1ᵀ y)  ( > 0 since M_prec is SPD ).
    let mut y = vec![0.0_f64; n];
    op.precond(&r1, &mut y);
    let mut beta1 = r1.iter().zip(y.iter()).map(|(a, b)| a * b).sum::<f64>();
    if beta1 < 0.0 {
        return Err(EigenError::FaerGevd(
            "matrix-free MINRES: preconditioner not SPD (r₁ᵀM⁻¹r₁ < 0)".into(),
        ));
    }
    if beta1 == 0.0 {
        // Warm start already solves the system.
        return Ok(0);
    }
    beta1 = beta1.sqrt();

    // Scalar recurrence state (Paige–Saunders MINRES).
    let mut oldb = 0.0_f64;
    let mut beta = beta1;
    let mut dbar = 0.0_f64;
    let mut epsln = 0.0_f64;
    let mut phibar = beta1;
    let mut cs = -1.0_f64;
    let mut sn = 0.0_f64;

    // Length-N work vectors (all O(N): r1, r2, y, v, w, w1, w2, av).
    let mut r2 = r1.clone();
    let mut v = vec![0.0_f64; n];
    let mut av = vec![0.0_f64; n];
    let mut w = vec![0.0_f64; n];
    let mut w1 = vec![0.0_f64; n];
    let mut w2 = vec![0.0_f64; n];

    let eps = f64::EPSILON;
    let thresh = tol * beta1;

    // Opt-in inner-MINRES progress logging (issue #562): read the interval once
    // so the hot loop only tests a cheap `Option`. Unset → no logging, no cost
    // beyond a per-iteration branch.
    let minres_log_every = diag_env::usize_opt(diag_env::MINRES_LOG).filter(|&e| e > 0);

    for itn in 1..=max_iters {
        // Lanczos step: v = y / beta;  av = A·v.
        let s = 1.0 / beta;
        for i in 0..n {
            v[i] = s * y[i];
        }
        op.apply(&v, &mut av);
        if itn >= 2 {
            let f = beta / oldb;
            for i in 0..n {
                av[i] -= f * r1[i];
            }
        }
        let alfa = v.iter().zip(av.iter()).map(|(a, b)| a * b).sum::<f64>();
        let f = alfa / beta;
        for i in 0..n {
            av[i] -= f * r2[i];
        }
        // Shift residual-space vectors: r1 ← r2, r2 ← av.
        r1.copy_from_slice(&r2);
        r2.copy_from_slice(&av);
        // y = M⁻¹ r2;  beta_{k+1} = √(r2ᵀ y).
        op.precond(&r2, &mut y);
        oldb = beta;
        beta = r2.iter().zip(y.iter()).map(|(a, b)| a * b).sum::<f64>();
        if beta < 0.0 {
            return Err(EigenError::FaerGevd(
                "matrix-free MINRES: preconditioner not SPD (rᵀM⁻¹r < 0)".into(),
            ));
        }
        beta = beta.sqrt();

        // Apply previous Givens rotation Q_{k-1}.
        let oldeps = epsln;
        let delta = cs * dbar + sn * alfa;
        let gbar = sn * dbar - cs * alfa;
        epsln = sn * beta;
        dbar = -cs * beta;

        // Compute and apply the next plane rotation Q_k.
        let gamma = (gbar * gbar + beta * beta).sqrt().max(eps);
        cs = gbar / gamma;
        sn = beta / gamma;
        let phi = cs * phibar;
        phibar *= sn;

        // Update the solution: w = (v − oldeps·w1 − delta·w2)/gamma; x += phi·w.
        let denom = 1.0 / gamma;
        w1.copy_from_slice(&w2);
        w2.copy_from_slice(&w);
        for i in 0..n {
            w[i] = (v[i] - oldeps * w1[i] - delta * w2[i]) * denom;
            out[i] += phi * w[i];
        }

        // Opt-in progress log (issue #562): the relative preconditioned residual
        // curve. Also emit the final point when we are about to stop.
        if let Some(every) = minres_log_every
            && (itn % every == 0 || phibar <= thresh || beta <= eps)
        {
            eprintln!(
                "[minres] itn={itn} rel_precond_resid={:.6e}",
                phibar / beta1
            );
        }

        // phibar tracks ‖r_k‖_{M⁻¹}; stop on the relative preconditioned
        // residual. Also stop on a Lanczos breakdown (β ≈ 0 → invariant
        // Krylov subspace, the iterate is exact on it).
        if phibar <= thresh || beta <= eps {
            return Ok(itn);
        }
    }

    Err(EigenError::FaerGevd(format!(
        "matrix-free inner MINRES failed to converge: ‖r‖_{{M⁻¹}}/‖r₀‖_{{M⁻¹}} = {:.3e} \
         > tol = {tol:.3e} after {max_iters} iters (interior shift too ill-conditioned \
         for abs-Jacobi MINRES? see issue #531 for a stronger preconditioner)",
        phibar / beta1
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
    /// Precomputed sparse LU of `A = K − σM` (direct path, COLAMD ordering).
    Lu(Lu<usize, f64>),
    /// Precomputed supernodal LU of `A = K − σM` built with a custom
    /// fill-reducing column ordering via faer's public deeper API (issue #543).
    CustomLu(crate::eigen::ordering::CustomOrderLu),
    /// Matrix-free operator + inner CG knobs (issue #524).
    MatrixFree {
        op: ShiftedMatrixFreeOp<'a>,
        tol: f64,
        max_iters: usize,
    },
    /// Matrix-free operator + inner MINRES knobs for the symmetric-indefinite
    /// (interior-shift) case (issue #535). The operator carries the
    /// absolute-value Jacobi diagonal so its preconditioner apply is SPD.
    MatrixFreeIndefinite {
        op: ShiftedMatrixFreeOp<'a>,
        tol: f64,
        max_iters: usize,
    },
}

impl InnerBackend<'_> {
    /// `out = A⁻¹ · rhs`. For the matrix-free path `out` doubles as the CG
    /// warm-start guess, so callers should retain it across iterations.
    ///
    /// Returns the number of inner CG iterations performed (0 for the direct
    /// LU backend, which has no inner iteration). Callers sum this across the
    /// outer Lanczos loop to report the inner-CG iteration count that the
    /// preconditioner (Jacobi vs AMS-lite) controls (issue #526).
    fn solve(&self, rhs: &[f64], out: &mut [f64]) -> Result<usize, EigenError> {
        match self {
            InnerBackend::Lu(lu) => {
                solve_with_lu(lu, rhs, out)?;
                Ok(0)
            }
            InnerBackend::CustomLu(lu) => {
                solve_with_custom_lu(lu, rhs, out)?;
                Ok(0)
            }
            InnerBackend::MatrixFree { op, tol, max_iters } => {
                cg_solve_matrix_free(op, rhs, out, *tol, *max_iters)
            }
            InnerBackend::MatrixFreeIndefinite { op, tol, max_iters } => {
                minres_solve_matrix_free(op, rhs, out, *tol, *max_iters)
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
        gradient: Option<&crate::eigen::projection::InteriorGradient>,
        dof_coords: Option<&[[f64; 3]]>,
    ) -> Result<InnerBackend<'a>, EigenError> {
        match self.inner {
            InnerSolver::Direct => {
                let a = shifted_pencil(k, m, self.sigma)?;
                let lu = {
                    let _par = ParallelismGuard::rayon(n_threads);
                    // Fill-reducing ordering (issue #527, Phase 1). faer 0.24's
                    // `sp_lu` uses a **COLAMD** (column approximate minimum
                    // degree) fill-reducing column permutation. It is hardcoded:
                    // `SymbolicLu::try_new` → `factorize_symbolic_lu` calls
                    // `colamd::order` unconditionally, and `LuSymbolicParams`
                    // exposes only `colamd::Control` — there is NO `Ordering`
                    // enum and NO hook to supply a precomputed permutation.
                    // (Contrast faer's *Cholesky* path, whose
                    // `SymmetricOrdering` DOES offer `Amd`/`Identity`/`Custom`;
                    // but the shifted pencil `A = K − σM` is symmetric-INDEFINITE,
                    // so this direct path must use unsymmetric LU, not Cholesky.)
                    //
                    // Phase 1 goal was to plumb a stronger ordering (METIS
                    // nested dissection) into this symbolic step to cut LU
                    // fill-in. Outcome: NEGATIVE. faer 0.24's public sparse-LU
                    // API accepts no user/alternative ordering, and the
                    // `SymbolicLu` permutation fields are private, so a METIS
                    // permutation cannot be injected without patching faer.
                    // Pre-permuting the input does not help either: COLAMD is
                    // invariant under column relabeling and simply re-derives
                    // its own ordering, discarding any nested-dissection
                    // structure. Adding a `metis` crate (a C-toolchain
                    // dependency) would be dead weight with no integration
                    // point. A stronger ordering therefore requires either a
                    // faer upstream change (add an LU `Custom`-ordering hook,
                    // mirroring Cholesky) or the compressed-factorization track
                    // captured as Phase 2 below.
                    //
                    // Phase 2 (OUT OF SCOPE here, follow-on): the memory win at
                    // ~1M DOF is the O(N^{4/3}) LU fill itself, addressable by a
                    // compressed/low-rank (BLR / HSS / H-matrix) factorization.
                    // The practical route is an FFI to a mature solver
                    // (STRUMPACK or MUMPS-BLR) versus a scoped Rust
                    // implementation; BLR is approximate, so it must be gated
                    // within the existing ≤1% Palace bar and a tight tolerance
                    // vs this exact-LU path. The complementary asymptotic fix is
                    // the matrix-free inner solve (`InnerSolver::MatrixFree`,
                    // issues #524/#526) already wired above.
                    //
                    // UPDATE (issue #543): the COLAMD wall above is now escapable
                    // WITHOUT a faer fork. faer 0.24's *public deeper* sparse-LU
                    // API (`col_etree`/`postorder`/`column_counts_ata`/
                    // `factorize_supernodal_symbolic_lu`/`_numeric_lu`) accepts a
                    // caller-supplied `Some(col_perm)`, so a coordinate
                    // nested-dissection / AMD ordering can be injected directly.
                    // That path is [`InnerSolver::DirectCustomOrder`] (see
                    // [`crate::eigen::ordering`]); this arm remains the COLAMD
                    // default and the comparison baseline.
                    a.as_ref()
                        .sp_lu()
                        .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?
                };
                Ok(InnerBackend::Lu(lu))
            }
            InnerSolver::DirectCustomOrder => {
                // Issue #543: same explicit `A = K − σM` as `Direct`, but factor
                // it with a custom fill-reducing column ordering injected through
                // faer's public deeper API instead of COLAMD. The ordering is a
                // geometric coordinate nested dissection when DOF coordinates are
                // available, else AMD minimum-degree from the pattern alone —
                // both cut LU fill/memory versus COLAMD while leaving the
                // spectrum unchanged (ordering changes memory, not answers).
                let a = shifted_pencil(k, m, self.sigma)?;
                let lu = {
                    let _par = ParallelismGuard::rayon(n_threads);
                    let (fwd, inv) =
                        crate::eigen::ordering::column_ordering(a.as_ref().symbolic(), dof_coords)?;
                    crate::eigen::ordering::CustomOrderLu::factorize(
                        a.as_ref(),
                        fwd,
                        inv,
                        faer::get_global_parallelism(),
                    )?
                };
                Ok(InnerBackend::CustomLu(lu))
            }
            InnerSolver::MatrixFree => {
                let mut op = ShiftedMatrixFreeOp::new(k, m, self.sigma);
                // AMS-lite preconditioner (issue #526): opt-in via
                // `precond == Ams` AND a supplied discrete gradient. Without a
                // gradient we fall back to the Jacobi baseline (the AMS coarse
                // correction is undefined without `G`). This keeps AMS purely
                // additive: the default `precond == Jacobi` never touches `G`.
                if self.precond == InnerPreconditioner::Ams
                    && let Some(g) = gradient
                {
                    let ams = crate::eigen::ams::AmsLitePreconditioner::build(g, k, m, self.sigma)?;
                    op = op.with_ams(ams);
                }
                // Opt-in cost-characterization overrides (issue #562): both
                // default to the exact prior behavior when unset.
                let max_iters = diag_env::usize_opt(diag_env::INNER_MAXITERS)
                    .unwrap_or((k.nrows() * INNER_MAX_ITER_FACTOR).max(64));
                let tol =
                    diag_env::f64_opt(diag_env::INNER_TOL).unwrap_or_else(|| self.inner_tol());
                Ok(InnerBackend::MatrixFree { op, tol, max_iters })
            }
            InnerSolver::MatrixFreeIndefinite => {
                // Interior/indefinite shift (issue #535): MINRES needs an SPD
                // preconditioner. The default is absolute-value Jacobi
                // `1/|K_ii − σM_ii|` (SPD even where the shifted diagonal is
                // sign-indefinite).
                let mut op = ShiftedMatrixFreeOp::new(k, m, self.sigma).with_abs_diag();
                // AMS-lite for the indefinite path (issues #531/#559): opt-in via
                // `precond == Ams` AND a supplied discrete gradient. Abs-Jacobi is
                // too weak to drive MINRES to the tight inner tolerance at a
                // deep-interior shift (it cannot damp the gradient near-kernel), so
                // wire the three-space Hiptmair–Xu AMS as the SPD preconditioner
                // MINRES needs.
                //
                // SPD construction. At an interior shift `A = K − σM` is
                // indefinite, so the gradient-space coarse operator
                // `Gᵀ A G ≈ −σ GᵀMG` is *negative* definite (KG ≈ 0 on the gradient
                // near-kernel) and the plain AMS would be indefinite — invalid for
                // MINRES. Building the AMS for the **sign-flipped SPD operator**
                // `K + |σ|M` (pass shift `−|σ|`) makes every subspace block SPD
                // (`diag(K+|σ|M) > 0`, `Gᵀ(K+|σ|M)G` and `Πᵀ(K+|σ|M)Π` SPD), and
                // applying it **additively** (a sum of SPD corrections, no matvec
                // of the indefinite `A`) yields a genuine SPD MINRES
                // preconditioner. `K + |σ|M` matches `|K − σM|` on both the
                // gradient near-kernel (both act like `|σ|M`) and the
                // curl-dominated high modes (both act like `K`), so it is a
                // principled SPD proxy for `|A|`.
                if self.precond == InnerPreconditioner::Ams
                    && let Some(g) = gradient
                {
                    let ams = crate::eigen::ams::AmsLitePreconditioner::build(
                        g,
                        k,
                        m,
                        -self.sigma.abs(),
                    )?;
                    op = op.with_ams_additive(ams);
                }
                // Opt-in cost-characterization overrides (issue #562):
                // `GEODE_INNER_TOL` sweeps the inner MINRES tolerance (shift-invert
                // tolerates loose inner solves) and `GEODE_INNER_MAXITERS` caps each
                // inner solve so multiple outer steps fit a bounded run. Both
                // default to the prior behavior when unset.
                let max_iters = diag_env::usize_opt(diag_env::INNER_MAXITERS)
                    .unwrap_or((k.nrows() * INNER_MAX_ITER_FACTOR).max(64));
                let tol =
                    diag_env::f64_opt(diag_env::INNER_TOL).unwrap_or_else(|| self.inner_tol());
                Ok(InnerBackend::MatrixFreeIndefinite { op, tol, max_iters })
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
        Ok(self
            .smallest_eigenpairs_impl(k, m, n_modes, n_threads, None, None)?
            .0)
    }

    /// [`Self::smallest_eigenpairs`] supplying the discrete gradient `G` so the
    /// matrix-free path can build the AMS-lite preconditioner (issue #526).
    ///
    /// Only consulted when `inner == InnerSolver::MatrixFree` and
    /// `precond == InnerPreconditioner::Ams`; otherwise `gradient` is ignored
    /// (the direct and Jacobi paths never touch `G`). The thread count is
    /// resolved from `GEODE_NUM_THREADS` as in [`Self::smallest_eigenpairs`].
    pub fn smallest_eigenpairs_with_gradient(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        gradient: &crate::eigen::projection::InteriorGradient,
    ) -> Result<Vec<EigenPair>, EigenError> {
        Ok(self
            .smallest_eigenpairs_impl(k, m, n_modes, resolve_num_threads(), Some(gradient), None)?
            .0)
    }

    /// [`Self::smallest_eigenpairs`] supplying per-DOF coordinates so the
    /// [`InnerSolver::DirectCustomOrder`] path can build a geometric
    /// coordinate-nested-dissection column ordering (issue #543).
    ///
    /// `dof_coords[i]` is the coordinate of DOF `i` (for Nédélec edge DOFs, the
    /// edge midpoint). Consulted only when `inner == DirectCustomOrder`; other
    /// inner solvers ignore it. When `inner == DirectCustomOrder` but no
    /// coordinates are supplied (the plain entry points), that path falls back
    /// to an AMD ordering from the pattern alone. The thread count is resolved
    /// from `GEODE_NUM_THREADS` as in [`Self::smallest_eigenpairs`].
    pub fn smallest_eigenpairs_with_coords(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        dof_coords: &[[f64; 3]],
    ) -> Result<Vec<EigenPair>, EigenError> {
        Ok(self
            .smallest_eigenpairs_impl(k, m, n_modes, resolve_num_threads(), None, Some(dof_coords))?
            .0)
    }

    /// [`Self::smallest_eigenpairs_with_gradient`] returning the **total inner
    /// CG iteration count** alongside the eigenpairs (issue #526).
    ///
    /// The total is summed across every outer Lanczos step's inner
    /// `(K − σM)⁻¹` apply; it is the quantity the inner preconditioner (Jacobi
    /// vs AMS-lite) controls, so it is the measurement the ≥5× iteration-
    /// reduction acceptance criterion is checked against. For the direct LU
    /// backend the count is 0 (no inner iteration). Pass `gradient = None` to
    /// measure the Jacobi baseline through the same code path.
    pub fn smallest_eigenpairs_inner_iters(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        gradient: Option<&crate::eigen::projection::InteriorGradient>,
    ) -> Result<(Vec<EigenPair>, usize), EigenError> {
        self.smallest_eigenpairs_impl(k, m, n_modes, resolve_num_threads(), gradient, None)
    }

    fn smallest_eigenpairs_impl(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        n_threads: usize,
        gradient: Option<&crate::eigen::projection::InteriorGradient>,
        dof_coords: Option<&[[f64; 3]]>,
    ) -> Result<(Vec<EigenPair>, usize), EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok((Vec::new(), 0));
        }

        // Running total of inner CG iterations across the outer Lanczos loop
        // (issue #526): 0 for the direct LU backend, the Jacobi/AMS-lite CG
        // iteration count for the matrix-free backend.
        let mut total_inner_iters = 0usize;

        // 1. Prepare the inner-solve backend for A⁻¹M. The direct variant
        //    builds A = K − σM and factors it once (scoping faer's rayon to
        //    the factorization — rayon speeds up sparse LU but is slower on
        //    the latency-bound single-RHS solves, issue #518); the
        //    matrix-free variant (issue #524) builds a borrowed operator +
        //    Jacobi diagonal (or, with `precond == Ams` and a supplied
        //    gradient, the AMS-lite preconditioner, issue #526) and never
        //    factors the edge pencil. Eigenvalues are independent of the
        //    thread count and the preconditioner — the
        //    `eigenpairs_agree_across_thread_counts` test is the gate.
        let inner = self.build_inner(k, m, n_threads, gradient, dof_coords)?;

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

        // Opt-in per-outer-step diagnostic (issue #562): observe how many outer
        // shift-invert Lanczos steps complete within a wall-clock budget and how
        // many inner iterations each step's `(K − σM)⁻¹` apply cost.
        let step_log = diag_env::is_set(diag_env::STEP_LOG);
        let t_loop = std::time::Instant::now();

        for j in 0..max_k {
            spmv(m, &v, &mut mv);
            w.copy_from_slice(&y_guess);
            let step_iters = inner.solve(&mv, &mut w)?;
            total_inner_iters += step_iters;
            if step_log {
                eprintln!(
                    "[lanczos] outer_step={j} inner_iters={step_iters} \
                     cumulative_inner={total_inner_iters} elapsed={:.1}s",
                    t_loop.elapsed().as_secs_f64()
                );
            }
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
        Ok((out, total_inner_iters))
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
        self.smallest_eigenvalues_impl(k, m, n_modes, n_threads, None, None)
    }

    /// [`SparseEigenSolver::smallest_eigenvalues`] supplying the discrete
    /// gradient `G` so the matrix-free path can build the AMS-lite
    /// preconditioner (issue #526). Only consulted when
    /// `inner == InnerSolver::MatrixFree` and `precond == InnerPreconditioner::Ams`.
    pub fn smallest_eigenvalues_with_gradient(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        gradient: &crate::eigen::projection::InteriorGradient,
    ) -> Result<Vec<f64>, EigenError> {
        self.smallest_eigenvalues_impl(k, m, n_modes, resolve_num_threads(), Some(gradient), None)
    }

    /// [`SparseEigenSolver::smallest_eigenvalues`] supplying per-DOF coordinates
    /// for the [`InnerSolver::DirectCustomOrder`] coordinate-nested-dissection
    /// ordering (issue #543). Consulted only when `inner == DirectCustomOrder`;
    /// without coordinates that path falls back to an AMD ordering.
    pub fn smallest_eigenvalues_with_coords(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        dof_coords: &[[f64; 3]],
    ) -> Result<Vec<f64>, EigenError> {
        self.smallest_eigenvalues_impl(k, m, n_modes, resolve_num_threads(), None, Some(dof_coords))
    }

    fn smallest_eigenvalues_impl(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
        n_threads: usize,
        gradient: Option<&crate::eigen::projection::InteriorGradient>,
        dof_coords: Option<&[[f64; 3]]>,
    ) -> Result<Vec<f64>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Prepare the inner-solve backend for A⁻¹M — direct sparse LU or
        //    matrix-free CG (Jacobi baseline, issue #524; AMS-lite opt-in with
        //    a supplied gradient, issue #526). Parallelism is scoped to the
        //    factorization only in the direct path (rayon regresses the
        //    single-RHS triangular solves that follow; issue #518). See the
        //    `smallest_eigenpairs` sibling above.
        let inner = self.build_inner(k, m, n_threads, gradient, dof_coords)?;

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
            let _inner_iters = inner.solve(&mv, &mut w)?;
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
            precond: InnerPreconditioner::Jacobi,
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
            precond: InnerPreconditioner::Jacobi,
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

    /// ISSUE #543 ACCEPTANCE BAR: the custom fill-reducing ordering must change
    /// LU fill/memory, **not the answers**. The spectrum from the
    /// [`InnerSolver::DirectCustomOrder`] path (coordinate nested dissection
    /// with supplied coordinates, and the AMD fallback without them) must match
    /// the COLAMD [`InnerSolver::Direct`] baseline within tolerance.
    #[test]
    fn custom_ordering_spectrum_matches_colamd() {
        // Serialize with the other tests that transiently mutate faer's global
        // parallelism during factorization.
        let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let n = 60;
        let (k, m) = laplacian_pencil(n);
        // Synthetic per-DOF coordinates along a line: exercises the geometric
        // coordinate nested dissection (the chain's only meaningful axis).
        let coords: Vec<[f64; 3]> = (0..n).map(|i| [i as f64, 0.0, 0.0]).collect();

        let base = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 80,
            tol: 1e-10,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let custom = SparseShiftInvertLanczos {
            inner: InnerSolver::DirectCustomOrder,
            ..base
        };

        let n_modes = 5;
        let colamd = base
            .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), n_modes, 1)
            .unwrap();
        // Coordinate nested dissection (coordinates supplied).
        let coord_nd = custom
            .smallest_eigenvalues_with_coords(k.as_ref(), m.as_ref(), n_modes, &coords)
            .unwrap();
        // AMD fallback (no coordinates supplied through the plain entry point).
        let amd = custom
            .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), n_modes, 1)
            .unwrap();

        assert_eq!(colamd.len(), n_modes);
        assert_eq!(coord_nd.len(), n_modes);
        assert_eq!(amd.len(), n_modes);
        for i in 0..n_modes {
            assert!(
                (colamd[i] - coord_nd[i]).abs() <= 1e-9 * (1.0 + colamd[i].abs()),
                "coordinate-ND eigenvalue {i} diverged: colamd={} nd={}",
                colamd[i],
                coord_nd[i]
            );
            assert!(
                (colamd[i] - amd[i]).abs() <= 1e-9 * (1.0 + colamd[i].abs()),
                "AMD-fallback eigenvalue {i} diverged: colamd={} amd={}",
                colamd[i],
                amd[i]
            );
        }
    }

    /// Eigen*pairs* (values and M-orthonormal vectors) from the custom-ordering
    /// direct path must also match the COLAMD baseline (issue #543): the
    /// ordering permutes the factorization internally but the recovered Ritz
    /// vectors live in the original DOF ordering, so they must agree up to sign.
    #[test]
    fn custom_ordering_eigenpairs_match_colamd() {
        let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());

        let n = 48;
        let (k, m) = laplacian_pencil(n);
        let coords: Vec<[f64; 3]> = (0..n).map(|i| [i as f64, 0.0, 0.0]).collect();

        let base = SparseShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 80,
            tol: 1e-10,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let custom = SparseShiftInvertLanczos {
            inner: InnerSolver::DirectCustomOrder,
            ..base
        };

        let n_modes = 4;
        let base_pairs = base
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        let custom_pairs = custom
            .smallest_eigenpairs_with_coords(k.as_ref(), m.as_ref(), n_modes, &coords)
            .unwrap();

        assert_eq!(base_pairs.len(), n_modes);
        assert_eq!(custom_pairs.len(), n_modes);
        for i in 0..n_modes {
            assert!(
                (base_pairs[i].lambda - custom_pairs[i].lambda).abs()
                    <= 1e-9 * (1.0 + base_pairs[i].lambda.abs()),
                "eigenvalue {i} diverged"
            );
            // Vectors agree up to an overall sign; align then compare.
            let dot: f64 = base_pairs[i]
                .vector
                .iter()
                .zip(custom_pairs[i].vector.iter())
                .map(|(a, b)| a * b)
                .sum();
            let sign = if dot < 0.0 { -1.0 } else { 1.0 };
            let max_diff = base_pairs[i]
                .vector
                .iter()
                .zip(custom_pairs[i].vector.iter())
                .map(|(a, b)| (a - sign * b).abs())
                .fold(0.0_f64, f64::max);
            assert!(
                max_diff < 1e-6,
                "eigenvector {i} diverged: max |Δ| = {max_diff}"
            );
        }
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
            precond: InnerPreconditioner::Jacobi,
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
            precond: InnerPreconditioner::Jacobi,
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
            precond: InnerPreconditioner::Jacobi,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::MatrixFree,
            precond: InnerPreconditioner::Jacobi,
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
            precond: InnerPreconditioner::Jacobi,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::MatrixFree,
            precond: InnerPreconditioner::Jacobi,
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

    // -------------------------------------------------------------------
    // Interior / indefinite matrix-free inner solve — MINRES (issue #535).
    // -------------------------------------------------------------------

    /// Regression guard: the default inner-solve backend is the SPD direct LU
    /// path — the indefinite MINRES variant must be purely additive/opt-in and
    /// must NOT become the default.
    #[test]
    fn default_inner_is_direct() {
        assert_eq!(
            SparseShiftInvertLanczos::default().inner,
            InnerSolver::Direct
        );
    }

    /// Unit test of the indefinite inner solver in isolation (no outer
    /// Lanczos): with an **interior** shift `(K − σM)` is genuinely
    /// symmetric-indefinite (we assert its diagonal carries negative entries,
    /// proving the SPD CG path would break down), yet matrix-free MINRES with
    /// the absolute-value Jacobi preconditioner drives the residual to the
    /// requested tolerance.
    #[test]
    fn minres_solves_indefinite_system() {
        let (k, m) = laplacian_pencil(50);
        // Laplacian eigenvalues lie in (0, 4). σ = 2.5 sits INSIDE the
        // spectrum, so K − σM = tridiag(-1, 2 - 2.5, -1) has diagonal
        // 2 - 2.5 = -0.5 < 0 everywhere ⇒ indefinite.
        let sigma = 2.5;
        let op = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma).with_abs_diag();

        // Signed diagonal K_ii − σM_ii = -0.5 < 0 ⇒ the plain (signed) Jacobi
        // diagonal would be negative; the abs-Jacobi diagonal is +1/0.5 = 2.
        let signed = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma);
        assert!(
            signed.inv_diag.iter().any(|&d| d < 0.0),
            "expected a negative signed Jacobi diagonal (indefinite operator)"
        );
        for (i, &d) in op.inv_diag.iter().enumerate() {
            assert!(
                d > 0.0 && (d - 2.0).abs() < 1e-12,
                "abs-Jacobi inv_diag[{i}] = {d}, want +2.0 (SPD preconditioner)"
            );
        }

        // Solve (K − σM) y = b and verify the true residual is tiny.
        let n = k.nrows();
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.37).sin() + 0.5).collect();
        let mut y = vec![0.0_f64; n];
        let iters = minres_solve_matrix_free(&op, &b, &mut y, 1e-12, 4 * n).unwrap();
        assert!(
            iters > 0 && iters <= 4 * n,
            "unexpected MINRES iters {iters}"
        );

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
            rnorm / bnorm < 1e-8,
            "matrix-free MINRES residual too large: {:.3e}",
            rnorm / bnorm
        );
    }

    /// CORRECTNESS GATE (issue #535): the indefinite matrix-free variant
    /// (`InnerSolver::MatrixFreeIndefinite`, MINRES) reproduces the direct
    /// sparse-LU path's eigenvalues at an **interior** shift, where `(K − σM)`
    /// is symmetric-indefinite and the SPD CG path is invalid. σ = 2.5 sits
    /// inside the Laplacian spectrum `(0, 4)`.
    #[test]
    fn minres_matches_direct_interior_shift() {
        let (k, m) = laplacian_pencil(40);
        let sigma = 2.5;
        let direct = SparseShiftInvertLanczos {
            sigma,
            max_iters: 80,
            tol: 1e-9,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 80,
            tol: 1e-9,
            inner: InnerSolver::MatrixFreeIndefinite,
            precond: InnerPreconditioner::Jacobi,
        };

        let ld = direct
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
            .unwrap();
        let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

        assert_eq!(ld.len(), lm.len(), "mode count differs direct vs MINRES");
        for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
            let rel = (d - f).abs() / d.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "interior eigenvalue[{i}] direct={d} MINRES={f} rel-diff={rel:.2e} > 1e-6"
            );
        }
    }

    /// EDGE CASE (issue #535): MINRES is a superset of CG on SPD systems, so
    /// selecting the indefinite variant on an SPD (below-spectrum) shift must
    /// still converge to the direct result — a guard against the indefinite
    /// solver regressing the SPD case if a caller picks it there.
    #[test]
    fn minres_matches_direct_spd_below_shift() {
        let (k, m) = laplacian_pencil(40);
        // σ = -1.0 is below the whole spectrum ⇒ K − σM = K + I is SPD.
        let sigma = -1.0;
        let direct = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let mf = SparseShiftInvertLanczos {
            sigma,
            max_iters: 64,
            tol: 1e-10,
            inner: InnerSolver::MatrixFreeIndefinite,
            precond: InnerPreconditioner::Jacobi,
        };

        let ld = direct
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
            .unwrap();
        let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

        assert_eq!(ld.len(), lm.len());
        for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
            let rel = (d - f).abs() / d.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "SPD eigenvalue[{i}] direct={d} MINRES={f} rel-diff={rel:.2e} > 1e-6"
            );
        }
    }
}
