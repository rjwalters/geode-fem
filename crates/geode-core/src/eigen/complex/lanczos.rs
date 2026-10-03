//! Sparse shift-and-invert Lanczos for **complex-symmetric** generalized
//! eigenproblems `K x = λ M x` where `K` and `M` are complex sparse
//! matrices and the pencil is complex-symmetric (`K^T = K`, `M^T = M`,
//! both *without* conjugation). This is the complex analog of
//! [`crate::eigen::lanczos::SparseShiftInvertLanczos`] (issue #53).
//!
//! **Why complex-symmetric (not Hermitian).** The Mie pipeline's mass
//! matrix is built from `∫ N_i · N_j ε dV` where ε is a per-tetrahedron
//! complex scalar (1 for vacuum, 1 + j σ in the scalar PML region, see
//! [`crate::assembly::nedelec::build_complex_epsilon_r_pml`] and issue
//! #28). The bilinear form is symmetric in `(i, j)` — so the assembled
//! matrix satisfies `M[i,j] = M[j,i] ∈ ℂ`, which is **bilinear-symmetric**
//! (`M^T = M`) but **not Hermitian** (`M^H = M̄ ≠ M`). Empirically, the
//! Mie pencil from `assemble_global_nedelec_with_complex_epsilon` on
//! the bundled sphere fixture has Im(v^H M v) ≈ -58 on a random start
//! vector — definitively not Hermitian.
//!
//! For a complex-symmetric pencil, the natural Lanczos variant uses the
//! **bilinear form** `⟨u, v⟩_M = u^T M v` (no conjugation). With this
//! form, `(K - σM)^{-1} M` is "symmetric" under the bilinear form, the
//! Lanczos tridiagonal `T_k` is **complex symmetric** (not Hermitian),
//! and its eigenvalues approximate the original pencil's eigenvalues
//! after the shift-and-invert mapping `λ = σ + 1/μ`. This is the
//! Lanczos-with-bilinear-form variant covered in Bai et al.,
//! *Templates for the Solution of Algebraic Eigenvalue Problems*,
//! §7.13. It can break down in principle (the bilinear form is not
//! positive-definite — `v^T M v` can hit zero on a nonzero v), but for
//! moderate PML strength `M ≈ M_re + j O(σ_0) M_im` is close to a real
//! SPD operator and breakdown is extremely unlikely.
//!
//! # Algorithm
//!
//! 1. Build `A = K - σ M` (complex, sparse) and factor once via faer's
//!    complex `sp_lu`.
//! 2. Lanczos: at step `j`,
//!    - `w = A^{-1} (M v_j)`     (complex sparse triangular solves),
//!    - `α_j = v_j^T M w`        (complex; the bilinear M-inner product),
//!    - `w ← w - α_j v_j - β_{j-1} v_{j-1}`,
//!    - full reorthogonalization of `w` against `{v_0, …, v_j}` in
//!      the **bilinear** M-inner product (no conjugation anywhere),
//!    - `β_j = sqrt(w^T M w)`    (complex principal branch),
//!    - `v_{j+1} = w / β_j`.
//! 3. Solve the small **complex-symmetric** tridiagonal `T_k` for its
//!    eigenvalues via faer's dense non-symmetric `eigenvalues()`. The
//!    tridiagonal is at most `max_iters × max_iters` (~64), so the
//!    dense path is essentially free next to the sparse triangular
//!    solves.
//! 4. Map `μ → σ + 1/μ` in complex arithmetic, sort by `|λ - σ|`, and
//!    return the `n_modes` closest to `σ`.
//!
//! Full reorthogonalization at every step is the same defensive choice
//! as the real path — at the n ≈ 6000 / k ≈ 30 sizes we care about,
//! basis storage is < 6 MB and the orthogonalization cost is negligible
//! next to the sparse triangular solves.
//!
//! # Why this loop is not ported onto [`crate::solver::iterate`]
//!
//! Like the real path ([`crate::eigen::lanczos`]), this complex-symmetric
//! variant shares the growing-basis structure: full reorthogonalization
//! keeps the whole history, so the Krylov basis gains one column per
//! iteration. That violates [`crate::solver::iterate`] **contract restriction 1**
//! (loop-invariant carried-state shapes), so the loop is intentionally
//! left off the `iterate_while` combinator. See [`crate::eigen::lanczos`] for the
//! full rationale.

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef};
use faer::{Mat, MatMut, c64};

use crate::eigen::dense::EigenError;
use crate::eigen::lanczos::{
    ConvergenceCheck, EXTENSION_BREAKDOWN_REL, ExtendMode, HISTORICAL_BREAKDOWN_REL, LanczosStop,
    TARGET_MATCH_REL_FLOOR, krylov_breakdown, negligible,
};
use crate::eigen::parallel::{ParallelismGuard, resolve_num_threads};
use crate::eigen::shift_guard::{check_degenerate_shift, median_diag_ratio};

/// Sparse generalized complex-symmetric eigensolver via shift-and-invert
/// Lanczos.
///
/// Mirrors [`crate::eigen::lanczos::SparseShiftInvertLanczos`] for complex
/// matrices. `sigma` is a **real** shift — for the Mie path we want
/// the lowest physical `k²` eigenvalues, which are positive real
/// (with small imaginary parts from the PML). A real shift keeps the
/// LU factor of `K - σ M` cheap to set up.
///
/// # ⚠ Choosing `sigma` on curl-curl pencils
///
/// The [`Default`] shift is `σ = 0`, which targets the smallest-magnitude
/// end of the spectrum and is correct **only when `K` is non-singular**.
/// Any curl-curl Nédélec pencil (Mie, PML, cavity) has a discrete-gradient
/// null space in `K`, so `K − 0·M` is singular: the LU still "succeeds"
/// (pivots `~ε·‖K‖`), and Lanczos converges onto the null cluster instead
/// of the physical band. For such pencils place `σ` strictly between the
/// null cluster (`λ ≈ 0`) and the lowest eigenvalue of interest (e.g.
/// `σ = 1.0` in `k²` units for the bundled Mie sphere — see
/// `examples/mie_sphere`, issues #691 / #696).
///
/// As a best-effort backstop both solve paths run a post-solve check and
/// return [`EigenError::DegenerateShift`] when `σ` is numerically zero and
/// **every** returned Ritz value collapsed onto it (see
/// [`crate::eigen::shift_guard`]). A shift placed exactly on a nonzero
/// eigenvalue (mode tracking) is not rejected. Domain-specific
/// drivers that *know* their pencil has a null space should also reject
/// `σ ≤ 0` up front, as
/// [`crate::eigen::pec_cavity::solve_pec_cavity_modes`] does.
#[derive(Debug, Clone, Copy)]
pub struct SparseComplexShiftInvertLanczos {
    /// Real shift `σ`; Ritz values closest to `σ` converge first. **Must
    /// not be `0.0` (the default) on a pencil whose `K` has a null space**
    /// — see the struct docs.
    pub sigma: f64,
    pub max_iters: usize,
    pub tol: f64,
}

impl Default for SparseComplexShiftInvertLanczos {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-9,
        }
    }
}

/// Parallel of [`crate::eigen::complex::ComplexEigenSolver`] for sparse
/// complex-symmetric pencils. The dense `ComplexEigenSolver` runs full
/// non-symmetric QZ; this trait exploits bilinear-symmetry of the
/// pencil to run shift-and-invert Lanczos at a small constant factor
/// in iterations × sparse solves, returning complex eigenvalues.
pub trait SparseComplexEigenSolver {
    /// Solve `K x = λ M x` for the `n` eigenvalues closest to the
    /// solver's shift `σ`, sorted by ascending `Re(λ)`.
    fn smallest_complex_pencil_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n: usize,
    ) -> Result<Vec<c64>, EigenError>;
}

/// Compute `y += A · x` for complex `A` in CSC form.
pub(crate) fn spmv_add(a: SparseColMatRef<'_, usize, c64>, x: &[c64], y: &mut [c64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let ncols = a.ncols();
    for j in 0..ncols {
        let start = col_ptr[j];
        let end = col_ptr[j + 1];
        let xj = x[j];
        if xj.re == 0.0 && xj.im == 0.0 {
            continue;
        }
        for k in start..end {
            let i = row_idx[k];
            y[i] += val[k] * xj;
        }
    }
}

/// Compute `y = A · x` (overwrite) for complex sparse `A`.
pub(crate) fn spmv(a: SparseColMatRef<'_, usize, c64>, x: &[c64], y: &mut [c64]) {
    for v in y.iter_mut() {
        *v = c64::new(0.0, 0.0);
    }
    spmv_add(a, x, y);
}

/// Bilinear M-inner product `u^T M v = sum u[i] * (M v)[i]`. Note **no
/// conjugation** — this is the bilinear, not Hermitian, form. The
/// caller passes pre-computed `M v` to amortize.
fn bilinear(u: &[c64], mv: &[c64]) -> c64 {
    debug_assert_eq!(u.len(), mv.len());
    let mut acc = c64::new(0.0, 0.0);
    for i in 0..u.len() {
        acc += u[i] * mv[i];
    }
    acc
}

/// Principal complex square root with `Re(sqrt) ≥ 0`.
///
/// For the M-bilinear norm `β_j = sqrt(w^T M w)`, we need a consistent
/// branch. Picking `Re(sqrt) ≥ 0` keeps the basis vectors numerically
/// well-scaled (β is "almost real positive" when M is close to a real
/// SPD).
///
/// Delegates to the cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`] (issue #830). The old
/// `Im β = √(½(|z| − Re z))` lost `Im β` to cancellation when `wᵀMw` is
/// nearly real, i.e. on a weakly lossy (high-Q) pencil: the basis was then
/// M-normalized only to `≈ √ε`, and on a uniformly filled `tan δ = 1e-10`
/// cavity the Ritz values came back with residuals `≈ 1e-7` and `Q` off by
/// 88 % (now `≈ 1e-13` and `≈ 1e-14`).
fn principal_sqrt(z: c64) -> c64 {
    crate::eigen::wavenumber::principal_sqrt(z)
}

/// Build `K - σ M` as a fresh complex sparse matrix. Mirrors
/// `shifted_pencil` in [`crate::eigen::lanczos`].
fn shifted_pencil_complex(
    k: SparseColMatRef<'_, usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    sigma: f64,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    use faer::sparse::Triplet;
    let n = k.nrows();
    assert_eq!(k.ncols(), n);
    assert_eq!(m.nrows(), n);
    assert_eq!(m.ncols(), n);

    let nnz = k.col_ptr()[n] + m.col_ptr()[n];
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(nnz);

    let push = |trips: &mut Vec<Triplet<usize, usize, c64>>,
                a: SparseColMatRef<'_, usize, c64>,
                scale: c64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    };
    push(&mut trips, k, c64::new(1.0, 0.0));
    if sigma != 0.0 {
        push(&mut trips, m, c64::new(-sigma, 0.0));
    }

    SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("complex shifted pencil assembly: {e:?}")))
}

/// Solve `A y = b` in-place via a precomputed complex sparse LU.
pub(crate) fn solve_with_lu(
    lu: &Lu<usize, c64>,
    rhs: &[c64],
    out: &mut [c64],
) -> Result<(), EigenError> {
    use faer::linalg::solvers::Solve;
    let n = rhs.len();
    let mut work: Mat<c64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    let work_mut: MatMut<'_, c64> = work.as_mut();
    lu.solve_in_place(work_mut);
    for i in 0..n {
        out[i] = work[(i, 0)];
    }
    Ok(())
}

/// Solve the complex-symmetric tridiagonal eigenproblem for `(alpha,
/// beta)`. `alpha` (len k) is the diagonal and `beta` (len k-1) is the
/// sub-diagonal; the matrix is set up as both sub- and super-diagonal =
/// `beta` (complex-symmetric). Returns complex eigenvalues unsorted.
fn tridiag_complex_eigenvalues(alpha: &[c64], beta: &[c64]) -> Result<Vec<c64>, EigenError> {
    let k = alpha.len();
    if k == 0 {
        return Ok(Vec::new());
    }
    let t = Mat::<c64>::from_fn(k, k, |i, j| {
        if i == j {
            alpha[i]
        } else if i + 1 == j {
            beta[i]
        } else if j + 1 == i {
            beta[j]
        } else {
            c64::new(0.0, 0.0)
        }
    });
    t.as_ref()
        .eigenvalues()
        .map_err(|e| EigenError::FaerGevd(format!("tridiag complex evd: {e:?}")))
}

/// A single complex generalized eigenpair `(λ, x)` of a complex-symmetric
/// pencil `K x = λ M x`, with `x` recovered as a Ritz vector and rescaled
/// to unit **bilinear** M-norm (`xᵀ M x = 1`, no conjugation). The
/// eigenvalue `λ` is complex (its imaginary part carries the leaky/loss
/// content of the mode); `x` is a `Vec<c64>` of length `n`.
#[derive(Clone, Debug)]
pub struct ComplexEigenPair {
    /// Generalized eigenvalue `λ` (complex).
    pub lambda: c64,
    /// Eigenvector `x` (length `n`), bilinear-M-normalized and complex.
    pub vector: Vec<c64>,
}

/// Solve the complex-symmetric tridiagonal eigenproblem returning
/// **eigenpairs** `(μ, s)`. `s` is a column eigenvector in `k`-dimensional
/// tridiagonal space; combine it with the Lanczos basis `V_k` to recover
/// the corresponding Ritz vector `x = V_k s`. Uses faer's complex
/// non-symmetric `Eigen` decomposition (the tridiagonal is complex
/// symmetric but **not** Hermitian, so the self-adjoint path is wrong).
fn tridiag_complex_eigenpairs(
    alpha: &[c64],
    beta: &[c64],
) -> Result<(Vec<c64>, Mat<c64>), EigenError> {
    use faer::linalg::solvers::Eigen;
    let k = alpha.len();
    if k == 0 {
        return Ok((Vec::new(), Mat::<c64>::zeros(0, 0)));
    }
    let t = Mat::<c64>::from_fn(k, k, |i, j| {
        if i == j {
            alpha[i]
        } else if i + 1 == j {
            beta[i]
        } else if j + 1 == i {
            beta[j]
        } else {
            c64::new(0.0, 0.0)
        }
    });
    let evd = Eigen::new(t.as_ref())
        .map_err(|e| EigenError::FaerGevd(format!("tridiag complex evd (pairs): {e:?}")))?;
    let s_diag = evd.S().column_vector();
    let u = evd.U();
    let mus: Vec<c64> = (0..k).map(|i| s_diag[i]).collect();
    let mut u_owned = Mat::<c64>::zeros(k, k);
    for c in 0..k {
        for r in 0..k {
            u_owned[(r, c)] = u[(r, c)];
        }
    }
    Ok((mus, u_owned))
}

impl SparseComplexEigenSolver for SparseComplexShiftInvertLanczos {
    fn smallest_complex_pencil_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
    ) -> Result<Vec<c64>, EigenError> {
        self.smallest_complex_pencil_eigenvalues_with_threads(k, m, n_modes, resolve_num_threads())
    }
}

impl SparseComplexShiftInvertLanczos {
    /// [`SparseComplexEigenSolver::smallest_complex_pencil_eigenvalues`] with
    /// an explicit factorization thread count, bypassing the
    /// `GEODE_NUM_THREADS` lookup.
    ///
    /// See
    /// [`crate::eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenvalues_with_threads`]
    /// for why the cross-thread tests use an explicit count rather than
    /// mutating the process environment.
    pub fn smallest_complex_pencil_eigenvalues_with_threads(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
        n_threads: usize,
    ) -> Result<Vec<c64>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Build A = K - σM and factor it once. Scope faer's global
        //    parallelism to this factorization only (issue #518): rayon
        //    speeds up the complex sparse LU but regresses the latency-bound
        //    single-RHS triangular solves in the Lanczos loop, and the guard
        //    restores the prior global parallelism on drop. `cap` makes
        //    `n_threads == 1` a serial factorization.
        let lu = self.factor(k, m, n_threads)?;

        // 2. Lanczos in the bilinear M-inner product. The tridiagonal
        //    `T_k` is complex symmetric (not Hermitian) and its
        //    eigenvalues approximate the inverse-shift map of the
        //    original pencil's eigenvalues.
        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);

        let mut basis: Vec<Vec<c64>> = Vec::with_capacity(max_k);
        // Cache of `M·v_j` for each basis vector (see issue #506) — reused
        // by the reorth loop instead of recomputing an SpMV per basis
        // vector every iteration.
        let mut m_basis: Vec<Vec<c64>> = Vec::with_capacity(max_k);
        let mut alpha: Vec<c64> = Vec::with_capacity(max_k);
        let mut beta: Vec<c64> = Vec::with_capacity(max_k);

        // Deterministic start vector — sin-based real start. The
        // basis picks up complex components on the first solve.
        let mut v: Vec<c64> = (0..n)
            .map(|i| c64::new((((i as f64) + 1.0) * 0.5432).sin(), 0.0))
            .collect();

        // Normalize v in the bilinear M-norm: scale by 1 / sqrt(v^T M v).
        let mut mv = vec![c64::new(0.0, 0.0); n];
        spmv(m, &v, &mut mv);
        let v_t_m_v = bilinear(&v, &mv);
        // Bilinear M-norm² can be exactly zero for "isotropic"
        // vectors in the bilinear form. For the Mie problem this is
        // pathologically rare on a generic start; flag it and exit.
        if bilinear_isotropic(&v, &mv, v_t_m_v) {
            return Err(EigenError::FaerGevd(
                "starting vector is M-bilinear-isotropic (v^T M v ≈ 0); pick a different start"
                    .into(),
            ));
        }
        let mut nrm = principal_sqrt(v_t_m_v);
        let inv = c64::new(1.0, 0.0) / nrm;
        for x in v.iter_mut() {
            *x *= inv;
        }

        let mut converged: Option<Vec<c64>> = None;
        let mut w = vec![c64::new(0.0, 0.0); n];
        let mut work = vec![c64::new(0.0, 0.0); n];
        // Running `max |α_j|`, the scale of `T_k`, for the breakdown test.
        let mut alpha_scale = 0.0_f64;

        for j in 0..max_k {
            // M v
            spmv(m, &v, &mut mv);
            // w = A^{-1} (M v)
            solve_with_lu(&lu, &mv, &mut w)?;

            // α_j = v^T M w = (M v)^T w  (using M^T = M, bilinear form)
            //     = sum mv[i] * w[i].
            let mut aj = c64::new(0.0, 0.0);
            for i in 0..n {
                aj += mv[i] * w[i];
            }
            alpha.push(aj);
            alpha_scale = alpha_scale.max(aj.norm());

            // w ← w - α_j v_j
            for i in 0..n {
                w[i] -= aj * v[i];
            }
            // w ← w - β_{j-1} v_{j-1}
            if let Some(bp) = beta.last().copied() {
                let prev = &basis[j - 1];
                for i in 0..n {
                    w[i] -= bp * prev[i];
                }
            }

            // Full reorthogonalization in the bilinear M-inner product.
            // For each basis vector v_k, c = v_k^T M w = (M v_k)^T w. Reuse
            // the cached `M·v_k` (`m_basis[idx]`) instead of recomputing an
            // SpMV per basis vector (issue #506).
            for (vk, m_vk) in basis.iter().zip(m_basis.iter()) {
                let mut c = c64::new(0.0, 0.0);
                for i in 0..n {
                    c += m_vk[i] * w[i];
                }
                if c.re != 0.0 || c.im != 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Re-project off v itself (about to enter basis). `mv` still
            // holds `M·v` from the top of this iteration.
            let mut c = c64::new(0.0, 0.0);
            for i in 0..n {
                c += mv[i] * w[i];
            }
            for i in 0..n {
                w[i] -= c * v[i];
            }

            // β_j² = w^T M w.
            spmv(m, &w, &mut work);
            let w_t_m_w = bilinear(&w, &work);
            nrm = principal_sqrt(w_t_m_w);

            // Push current v as basis[j] (caching `M·v` alongside).
            m_basis.push(core::mem::take(&mut mv));
            mv = vec![c64::new(0.0, 0.0); n];
            basis.push(core::mem::take(&mut v));

            // Convergence probe on the complex tridiagonal.
            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let mus = tridiag_complex_eigenvalues(&alpha, &beta)?;
                let sigma_c = c64::new(self.sigma, 0.0);
                let mut lambdas: Vec<c64> = mus
                    .iter()
                    .filter(|mu| mu.re.hypot(mu.im) > 0.0)
                    .map(|mu| sigma_c + c64::new(1.0, 0.0) / *mu)
                    .collect();
                lambdas.sort_by(|a, b| {
                    let da = (a.re - self.sigma).hypot(a.im);
                    let db = (b.re - self.sigma).hypot(b.im);
                    da.partial_cmp(&db).unwrap_or(core::cmp::Ordering::Equal)
                });
                if lambdas.len() >= n_modes {
                    let mut picked: Vec<c64> = lambdas.into_iter().take(n_modes).collect();
                    // Final sort: ascending by Re(λ) — matches the dense
                    // ComplexEigenSolver's output convention.
                    picked.sort_by(|a, b| {
                        a.re.partial_cmp(&b.re)
                            .unwrap_or(core::cmp::Ordering::Equal)
                    });

                    // Kaniel–Saad-flavored convergence: |β_j| relative
                    // to the largest |μ| in the tridiagonal. β is
                    // complex here so we use its magnitude. Relative in
                    // every mesh length unit since issue #828 (the old
                    // `max(μ_max, 1)` floor made it absolute for |μ| < 1).
                    let mu_max = mus.iter().fold(0.0_f64, |a, mu| a.max(mu.re.hypot(mu.im)));
                    let beta_mag = nrm.re.hypot(nrm.im);
                    if beta_mag <= self.tol * mu_max {
                        converged = Some(picked);
                        break;
                    }
                    converged = Some(picked);
                }
            }

            // Numerical breakdown — invariant subspace exhausted. Relative
            // to the scale of `T_k` (issue #828; it was `|β| < 1e-14`).
            if krylov_breakdown(nrm.re.hypot(nrm.im), alpha_scale, HISTORICAL_BREAKDOWN_REL) {
                break;
            }

            beta.push(nrm);
            let inv = c64::new(1.0, 0.0) / nrm;
            v = w.iter().map(|x| *x * inv).collect();
        }

        let picked = converged.ok_or_else(|| {
            EigenError::FaerGevd(format!(
                "complex Lanczos terminated after {} iters without computing {} ritz pairs",
                alpha.len(),
                n_modes
            ))
        })?;
        // Degenerate-shift guard (issue #696): fail loudly instead of
        // returning a Ritz set collapsed onto a singular shift.
        self.check_not_degenerate(k, m, picked.iter().copied())?;
        Ok(picked)
    }

    /// Post-solve degenerate-shift check shared by both solve paths; see
    /// [`crate::eigen::shift_guard`].
    fn check_not_degenerate(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        lambdas: impl Iterator<Item = c64>,
    ) -> Result<(), EigenError> {
        let dists: Vec<f64> = lambdas.map(|l| (l.re - self.sigma).hypot(l.im)).collect();
        let scale = median_diag_ratio(k, m, |z: c64| z.re.hypot(z.im));
        check_degenerate_shift(self.sigma, &dists, scale)
    }
}

impl SparseComplexShiftInvertLanczos {
    /// Compute the `n_modes` complex generalized eigenpairs of the
    /// complex-symmetric pencil `K x = λ M x` closest to the configured
    /// real shift `σ`, including bilinear-M-normalized eigenvectors. The
    /// eigenvalue-only sibling is
    /// [`SparseComplexEigenSolver::smallest_complex_pencil_eigenvalues`].
    ///
    /// This is the complex analogue of
    /// [`crate::eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs`]:
    /// it runs the same bilinear-form Lanczos (full reorthogonalization,
    /// the tridiagonal `T_k` complex symmetric), retains the full basis
    /// `V_k`, then recovers Ritz vectors `x = V_k s` from the complex
    /// tridiagonal eigenvectors `s`, maps `μ → σ + 1/μ`, sorts by
    /// `|λ − σ|`, and bilinear-M-normalizes each kept vector.
    ///
    /// The returned pairs are **not** checked for convergence: the complex
    /// bilinear Lanczos has no interlacing guarantee, so an unconverged Ritz
    /// value can sit nearer `σ` than converged ones (issue #834). Callers
    /// that need converged pairs use [`Self::smallest_eigenpairs_checked`].
    ///
    /// Used by the PML dielectric modal solve (Epic #303 PML-B, issue
    /// #332): the eigenvectors are needed for the curl-energy filter and
    /// the core-energy-fraction field-shape confirmation.
    pub fn smallest_eigenpairs(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
    ) -> Result<Vec<ComplexEigenPair>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Build A = K − σM and factor it once (parallelism scoped to the
        //    factorization only, issue #518).
        let lu = self.factor(k, m, resolve_num_threads())?;

        // 2. Lanczos in the bilinear M-inner product, retaining the full
        //    basis V_k for Ritz-vector recovery.
        let max_k = self.initial_krylov_dim(n, n_modes);
        let mut run = ComplexLanczosRun::start(m, max_k)?;
        run.extend(&lu, m, n_modes, max_k, self.tol, ExtendMode::Historical)?;

        // 3–5. Ritz pairs nearest σ, ascending Re λ, degenerate-shift guard
        //      (issue #696), bilinear-M-normalized.
        let (values, s_mat) = run.ritz_values(self.sigma)?;
        let picks = nearest_picks(values, self.sigma, n_modes);
        self.check_not_degenerate(k, m, picks.iter().map(|p| p.0))?;
        Ok(run.ritz_vectors(m, &s_mat, &picks))
    }

    /// Krylov dimension of the first Lanczos pass for `n_modes` requested
    /// pairs on an `n`-dimensional pencil: `max_iters`, raised to at least
    /// `n_modes + 2`, never above `n`.
    fn initial_krylov_dim(&self, n: usize, n_modes: usize) -> usize {
        self.max_iters.min(n).max(n_modes + 2).min(n)
    }

    /// Build `A = K − σM` and factor it once, scoping faer's global
    /// parallelism to the factorization (issue #518).
    fn factor(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_threads: usize,
    ) -> Result<Lu<usize, c64>, EigenError> {
        let a = shifted_pencil_complex(k, m, self.sigma)?;
        let _par = ParallelismGuard::cap(n_threads);
        a.as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("complex sparse LU: {e:?}")))
    }

    /// [`Self::smallest_eigenpairs`] with a **per-pair convergence check**
    /// (issue #834), the complex-symmetric counterpart of
    /// [`crate::eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs_checked`]
    /// (issue #798).
    ///
    /// Only Ritz pairs whose true relative residual
    ///
    /// ```text
    ///   ρ(λ, x) = ‖K x − λ M x‖₂ / (max(|λ|, |σ|) · ‖M x‖₂)
    /// ```
    ///
    /// (Hermitian 2-norms, two sparse mat-vecs per pair) meets
    /// [`ConvergenceCheck::residual_tol`] are returned. [`ConvergenceCheck::window`]
    /// is an interval on `Re λ`.
    ///
    /// # Why the complex solver needs it more than the real one
    ///
    /// The bilinear (non-Hermitian) Lanczos has no interlacing guarantee: an
    /// unconverged Ritz value can land anywhere, including nearer `σ` than
    /// converged ones. The plain solve returns it as one of the `n_modes`
    /// nearest. Measured on the uniformly filled `tan δ = 0.1` sphere
    /// (PR #833): `λ = 0.4756 − 0.0767j`, relative residual 11, between the
    /// converged `0.83` triplet and the `1.65` multiplet. Whether such a
    /// value appears depends on round-off and the thread count.
    ///
    /// # Algorithm
    ///
    /// The same as the real checked solve:
    ///
    /// - **First pass**: exactly the Krylov run and arithmetic of
    ///   [`Self::smallest_eigenpairs`]; its `n_modes` pairs nearest `σ` are
    ///   classified as converged (`ρ ≤ residual_tol`), *localized* but
    ///   unconverged inside the window (`ρ · max(|λ|, |σ|) ≤ |λ − σ|`), or
    ///   neither. A spurious value like the one above is not localized, so
    ///   it is withheld and never becomes a target.
    /// - If every target (converged and localized in-window pairs) is
    ///   converged and, with no window, `n_modes` pairs are, the converged
    ///   pairs are returned **bit-identical** to [`Self::smallest_eigenpairs`]
    ///   and the rest is reported in [`CheckedComplexEigenpairs::rejected`].
    /// - **Extension**: otherwise the same recurrence continues from the
    ///   stored basis (no restart, no refactorization) in increments of
    ///   `max(n_modes, 8)` steps, until every target has a converged Ritz
    ///   pair within `max(ρ, 10⁻¹⁰) · max(|λ|, |σ|)` of it (and, with no
    ///   window, `n_modes` converged pairs are in hand), the cap is reached
    ///   or `β` breaks down (`|β| ≤ 10⁻¹³ · max |α_j|`). The first pass's
    ///   targets are tracked, rather than re-taking the nearest `n_modes`
    ///   each pass, for the reason given on the real solver. With no window
    ///   the result is topped up with the converged pairs nearest `σ`.
    ///
    /// The plain [`Self::smallest_eigenpairs`] is unchanged and unchecked.
    pub fn smallest_eigenpairs_checked(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
        check: ConvergenceCheck,
    ) -> Result<CheckedComplexEigenpairs, EigenError> {
        self.checked_impl(k, m, n_modes, n_modes, check, &|_| true)
    }

    /// [`Self::smallest_eigenpairs_checked`] restricted to the Ritz values a
    /// caller can use, for solvers that must filter **before** the
    /// closest-to-`σ` truncation (issue #834), for example
    /// [`crate::eigen::lossy_cavity`], whose gradient-nullspace and
    /// overdamped Ritz values can sit nearer `σ` than the physical modes.
    ///
    /// The first pass runs exactly [`Self::smallest_eigenpairs`] with
    /// `max(max_iters, n_modes)` requested pairs, that is every Ritz pair the
    /// basis yields. The degenerate-shift guard sees all of them. Those for
    /// which `eligible(λ)` is `false` are dropped and reported in
    /// [`CheckedComplexEigenpairs::screened`]. The `n_modes` eligible pairs
    /// nearest `σ` are then checked and, if needed, extended as in
    /// [`Self::smallest_eigenpairs_checked`]; every candidate considered in
    /// an extension (confirmations, top-up and withheld report) must also be
    /// eligible.
    pub fn smallest_eigenpairs_checked_filtered(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
        check: ConvergenceCheck,
        eligible: &dyn Fn(c64) -> bool,
    ) -> Result<CheckedComplexEigenpairs, EigenError> {
        let all = self.max_iters.max(n_modes).min(k.nrows());
        self.checked_impl(k, m, all, n_modes, check, eligible)
    }

    /// Shared body of the checked solves. The first pass is
    /// `smallest_eigenpairs(k, m, first_modes)`; of its pairs, the `n_modes`
    /// eligible ones nearest `σ` are checked.
    fn checked_impl(
        &self,
        k: SparseColMatRef<'_, usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        first_modes: usize,
        n_modes: usize,
        check: ConvergenceCheck,
        eligible: &dyn Fn(c64) -> bool,
    ) -> Result<CheckedComplexEigenpairs, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        let requested = n_modes.min(n);
        let mut out = CheckedComplexEigenpairs {
            pairs: Vec::new(),
            residuals: Vec::new(),
            rejected: Vec::new(),
            screened: Vec::new(),
            requested,
            lanczos_steps: 0,
            extended: false,
        };
        if requested == 0 {
            return Ok(out);
        }
        let first_modes = first_modes.max(n_modes);
        let lu = self.factor(k, m, resolve_num_threads())?;
        let sigma = self.sigma;
        let mut kx = vec![c64::new(0.0, 0.0); n];
        let mut mx = vec![c64::new(0.0, 0.0); n];
        let mut residuals_of = |pairs: &[ComplexEigenPair]| -> Vec<f64> {
            pairs
                .iter()
                .map(|p| {
                    complex_pair_relative_residual(
                        k, m, p.lambda, &p.vector, sigma, &mut kx, &mut mx,
                    )
                })
                .collect()
        };
        let dist = |l: c64| (l.re - sigma).hypot(l.im);
        let by_dist = |a: &(c64, usize), b: &(c64, usize)| {
            dist(a.0)
                .partial_cmp(&dist(b.0))
                .unwrap_or(core::cmp::Ordering::Equal)
        };
        let by_re = |a: &(c64, usize), b: &(c64, usize)| {
            a.0.re
                .partial_cmp(&b.0.re)
                .unwrap_or(core::cmp::Ordering::Equal)
        };
        let in_window = |l: c64| check.window.is_none_or(|(lo, hi)| lo < l.re && l.re < hi);

        // First pass: the historical Krylov dimension, selection and
        // degenerate-shift guard of `smallest_eigenpairs(k, m, first_modes)`.
        let first = self.initial_krylov_dim(n, first_modes);
        let mut run = ComplexLanczosRun::start(m, first)?;
        run.extend(&lu, m, first_modes, first, self.tol, ExtendMode::Historical)?;
        let (values, s_mat) = run.ritz_values(sigma)?;
        let first_picks = nearest_picks(values, sigma, first_modes);
        self.check_not_degenerate(k, m, first_picks.iter().map(|p| p.0))?;
        let (mut picks, screened): (RitzPicks, RitzPicks) =
            first_picks.into_iter().partition(|p| eligible(p.0));
        out.screened = screened.into_iter().map(|p| p.0).collect();
        if picks.len() > n_modes {
            // `n_modes` eligible pairs nearest σ, back in ascending Re λ.
            picks.sort_by(by_dist);
            picks.truncate(n_modes);
            picks.sort_by(by_re);
        }
        let pairs = run.ritz_vectors(m, &s_mat, &picks);
        let residuals = residuals_of(&pairs);
        let cap = check.max_iters_cap.max(first).min(n);
        let tol = check.residual_tol;

        let targets: Vec<ComplexRitzTarget> = pairs
            .iter()
            .zip(&residuals)
            .filter(|(p, r)| {
                **r <= tol
                    || (in_window(p.lambda) && complex_ritz_is_localized(p.lambda, **r, sigma))
            })
            .map(|(p, &residual)| ComplexRitzTarget {
                lambda: p.lambda,
                width: residual.max(TARGET_MATCH_REL_FLOOR) * p.lambda.norm().max(sigma.abs()),
                residual,
            })
            .collect();
        let n_converged = residuals.iter().filter(|&&r| r <= tol).count();
        let enough = check.window.is_some() || n_converged >= requested;
        if (enough && targets.iter().all(|t| t.residual <= tol))
            || run.broken_down()
            || first >= cap
        {
            out.lanczos_steps = run.alpha.len();
            out.split(pairs, residuals, tol);
            return Ok(out);
        }

        // Extension (see the real solver): run until every target has a
        // converged Ritz pair within its uncertainty (and, with no window,
        // `n_modes` converged pairs are in hand), then return the
        // confirmations, topped up with the converged eligible pairs nearest
        // σ when there is no window.
        out.extended = true;
        let increment = n_modes.max(8);
        let mut dim_target = first;
        loop {
            dim_target = (dim_target + increment).min(cap);
            run.extend(&lu, m, n_modes, dim_target, tol, ExtendMode::Extension)?;
            let (values, s_mat) = run.ritz_values(sigma)?;
            let values: Vec<(c64, usize)> = values.into_iter().filter(|v| eligible(v.0)).collect();
            let mut near_picks: Vec<(c64, usize)> = values
                .iter()
                .copied()
                .filter(|&(l, _)| targets.iter().any(|t| (l - t.lambda).norm() <= t.width))
                .collect();
            near_picks.sort_by(by_re);
            let near = run.ritz_vectors(m, &s_mat, &near_picks);
            let near_res = residuals_of(&near);
            let confirmed: Vec<bool> = targets
                .iter()
                .map(|t| {
                    near.iter()
                        .zip(&near_res)
                        .any(|(p, &r)| r <= tol && (p.lambda - t.lambda).norm() <= t.width)
                })
                .collect();
            let all_confirmed = confirmed.iter().all(|&c| c);
            let steps = run.alpha.len();
            let last = run.broken_down() || steps >= cap;
            if !all_confirmed && !last {
                continue;
            }
            let mut kept: Vec<(ComplexEigenPair, f64, usize)> = near
                .into_iter()
                .zip(near_res)
                .zip(&near_picks)
                .filter(|((_, r), _)| *r <= tol)
                .map(|((pair, r), pick)| (pair, r, pick.1))
                .collect();
            let want = if check.window.is_some() { 0 } else { requested };
            if kept.len() < want {
                let mut rest: Vec<(c64, usize)> = values
                    .iter()
                    .copied()
                    .filter(|v| !near_picks.iter().any(|p| p.1 == v.1))
                    .collect();
                rest.sort_by(by_dist);
                rest.truncate(want);
                let extra = run.ritz_vectors(m, &s_mat, &rest);
                let extra_res = residuals_of(&extra);
                for ((pair, res), pick) in extra.into_iter().zip(extra_res).zip(&rest) {
                    if kept.len() >= want {
                        break;
                    }
                    if res <= tol {
                        kept.push((pair, res, pick.1));
                    }
                }
            }
            if kept.len() < want && !last {
                continue;
            }
            kept.sort_by(|a, b| {
                a.0.lambda
                    .re
                    .partial_cmp(&b.0.lambda.re)
                    .unwrap_or(core::cmp::Ordering::Equal)
            });
            out.lanczos_steps = steps;
            // Withheld report, as on the real solver: the final run's
            // unconverged in-window pairs among the `requested` nearest σ
            // that were not returned, plus targets still unconfirmed.
            let mut nearest: Vec<(c64, usize)> = values
                .iter()
                .copied()
                .filter(|&(l, col)| in_window(l) && !kept.iter().any(|k| k.2 == col))
                .collect();
            nearest.sort_by(by_dist);
            let mut dists: Vec<f64> = values
                .iter()
                .filter(|v| in_window(v.0))
                .map(|v| dist(v.0))
                .collect();
            dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            let reach = dists.get(requested - 1).copied().unwrap_or(f64::INFINITY);
            nearest.retain(|v| dist(v.0) <= reach);
            let unaccounted = run.ritz_vectors(m, &s_mat, &nearest);
            let unaccounted_res = residuals_of(&unaccounted);
            let unconverged: Vec<(c64, f64)> = unaccounted
                .iter()
                .zip(unaccounted_res)
                .filter(|(_, r)| *r > tol)
                .map(|(p, r)| (p.lambda, r))
                .collect();
            for (pair, res, _) in kept {
                out.pairs.push(pair);
                out.residuals.push(res);
            }
            out.rejected.extend(
                targets
                    .iter()
                    .zip(&confirmed)
                    .filter(|(t, c)| {
                        !**c && !unconverged
                            .iter()
                            .any(|&(l, _)| (l - t.lambda).norm() <= t.width)
                    })
                    .map(|(t, _)| (t.lambda, t.residual)),
            );
            out.rejected.extend(unconverged);
            return Ok(out);
        }
    }
}

/// Result of [`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked`]
/// (issue #834): the converged pairs plus an explicit record of what was
/// withheld. The complex counterpart of
/// [`crate::eigen::lanczos::CheckedEigenpairs`].
#[derive(Debug, Clone)]
pub struct CheckedComplexEigenpairs {
    /// Converged pairs (relative residual ≤ the tolerance), ascending
    /// `Re λ`, bilinear-M-normalized. At most `requested` entries unless the
    /// run was [extended](Self::extended), when every converged pair
    /// confirming a first-pass pair is returned (topped up to `requested`
    /// with the converged pairs nearest `σ` when there is no window).
    pub pairs: Vec<ComplexEigenPair>,
    /// Relative true residual of each entry of [`Self::pairs`] (parallel).
    pub residuals: Vec<f64>,
    /// `(λ, residual)` of the unconverged Ritz pairs withheld although they
    /// rank among the `requested` (in-window, eligible) pairs nearest `σ`.
    /// Without an extension these are all the first pass's unconverged
    /// pairs. After one they are the final run's unconverged pairs in that
    /// range plus first-pass targets never confirmed before the cap.
    pub rejected: Vec<(c64, f64)>,
    /// First-pass Ritz values dropped by the `eligible` filter of
    /// [`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked_filtered`]
    /// (empty for the unfiltered solve).
    pub screened: Vec<c64>,
    /// Number of pairs asked for (`n_modes`, clamped to the pencil dimension).
    pub requested: usize,
    /// Lanczos steps (Krylov dimension) actually run, first pass included.
    pub lanczos_steps: usize,
    /// Whether the first pass left an unconverged pair and the run was
    /// extended. When `false`, [`Self::pairs`] is bit-identical to the
    /// first-pass pairs of [`SparseComplexShiftInvertLanczos::smallest_eigenpairs`]
    /// minus [`Self::rejected`].
    pub extended: bool,
}

impl CheckedComplexEigenpairs {
    /// How many fewer converged pairs than requested were returned:
    /// `requested − pairs.len()`, saturating at zero.
    pub fn shortfall(&self) -> usize {
        self.requested.saturating_sub(self.pairs.len())
    }

    /// Partition `pairs` into converged (kept) and rejected by `tol`.
    fn split(&mut self, pairs: Vec<ComplexEigenPair>, residuals: Vec<f64>, tol: f64) {
        for (pair, res) in pairs.into_iter().zip(residuals) {
            if res <= tol {
                self.pairs.push(pair);
                self.residuals.push(res);
            } else {
                self.rejected.push((pair.lambda, res));
            }
        }
    }
}

/// A first-pass Ritz pair the complex checked solve's extension must
/// confirm (issue #834).
#[derive(Debug, Clone, Copy)]
struct ComplexRitzTarget {
    lambda: c64,
    /// `max(ρ, TARGET_MATCH_REL_FLOOR) · max(|λ|, |σ|)`.
    width: f64,
    residual: f64,
}

/// Whether a complex Ritz value is **localized**: `ρ · max(|λ|, |σ|) ≤ |λ − σ|`
/// (issue #834; the complex form of the real solver's test).
fn complex_ritz_is_localized(lambda: c64, residual: f64, sigma: f64) -> bool {
    residual * lambda.norm().max(sigma.abs()) <= (lambda.re - sigma).hypot(lambda.im)
}

/// Relative true residual `‖K x − λ M x‖₂ / (max(|λ|, |σ|) · ‖M x‖₂)`
/// (Hermitian 2-norms) of a complex Ritz pair, using `kx` / `mx` as scratch
/// (issue #834).
fn complex_pair_relative_residual(
    k: SparseColMatRef<'_, usize, c64>,
    m: SparseColMatRef<'_, usize, c64>,
    lambda: c64,
    x: &[c64],
    sigma: f64,
    kx: &mut [c64],
    mx: &mut [c64],
) -> f64 {
    spmv(k, x, kx);
    spmv(m, x, mx);
    let mut r2 = 0.0_f64;
    let mut m2 = 0.0_f64;
    for (a, b) in kx.iter().zip(mx.iter()) {
        let r = *a - lambda * *b;
        r2 += r.re * r.re + r.im * r.im;
        m2 += b.re * b.re + b.im * b.im;
    }
    let scale = lambda.norm().max(sigma.abs()) * m2.sqrt();
    if scale > 0.0 {
        r2.sqrt() / scale
    } else {
        f64::INFINITY
    }
}

/// The `n_modes` Ritz values nearest `σ` (stable, so ties keep column
/// order), re-sorted by ascending `Re λ` (stable): the selection of
/// [`SparseComplexShiftInvertLanczos::smallest_eigenpairs`].
fn nearest_picks(mut values: Vec<(c64, usize)>, sigma: f64, n_modes: usize) -> Vec<(c64, usize)> {
    values.sort_by(|a, b| {
        let da = (a.0.re - sigma).hypot(a.0.im);
        let db = (b.0.re - sigma).hypot(b.0.im);
        da.partial_cmp(&db).unwrap_or(core::cmp::Ordering::Equal)
    });
    values.truncate(n_modes.min(values.len()));
    values.sort_by(|a, b| {
        a.0.re
            .partial_cmp(&b.0.re)
            .unwrap_or(core::cmp::Ordering::Equal)
    });
    values
}

/// Whether the bilinear self-product `vᵀ M v` (with `mv = M v`) vanishes to
/// working precision, i.e. `v` is M-bilinear-isotropic (issue #828).
///
/// The test is relative to `Σ |v_i| |(M v)_i|`, the size of the sum before
/// cancellation, so it does not depend on the mesh length unit (mass
/// entries scale as `L³`). It was the absolute `|Re| + |Im| < 1e-30`, which
/// a nanometre-scale mesh in metres would trip on any start vector.
fn bilinear_isotropic(v: &[c64], mv: &[c64], v_t_m_v: c64) -> bool {
    let size: f64 = v.iter().zip(mv).map(|(a, b)| a.norm() * b.norm()).sum();
    negligible(v_t_m_v.norm(), size, ISOTROPY_REL)
}

/// Relative threshold of [`bilinear_isotropic`].
const ISOTROPY_REL: f64 = 1e-14;

/// Ritz values `(λ, column of S)` and the tridiagonal eigenvector matrix.
type ComplexRitzValues = (RitzPicks, Mat<c64>);

/// Ritz values `(λ, column of S)`.
type RitzPicks = Vec<(c64, usize)>;

/// Resumable state of the complex bilinear shift-invert Lanczos recurrence
/// (issue #834), the complex counterpart of the real solver's run.
///
/// Holds the bilinear-M-orthonormal basis `V_k` (with cached `M·v_j`), the
/// complex tridiagonal coefficients and the next basis vector, so the
/// Krylov space can be extended after a first pass instead of restarted. A
/// single [`ComplexLanczosRun::extend`] to the full dimension performs
/// exactly the arithmetic of the historical single-loop implementation.
struct ComplexLanczosRun {
    basis: Vec<Vec<c64>>,
    /// `M·v_j` for each basis vector (issue #506).
    m_basis: Vec<Vec<c64>>,
    alpha: Vec<c64>,
    beta: Vec<c64>,
    /// Next basis vector `v_{k+1}` (valid unless [`Self::stop`] is set).
    v: Vec<c64>,
    mv: Vec<c64>,
    w: Vec<c64>,
    work: Vec<c64>,
    stop: Option<LanczosStop>,
    /// The next `β` at a [`LanczosStop::BetaBound`] stop (`w` still holds
    /// the unnormalized residual), so an extension resumes exactly.
    pending_beta: c64,
    /// Running `max |α_j|`, the scale of `T_k`, for the breakdown tests.
    alpha_scale: f64,
}

impl ComplexLanczosRun {
    /// Normalize the deterministic real start vector in the bilinear M-norm
    /// and allocate buffers.
    fn start(m: SparseColMatRef<'_, usize, c64>, capacity: usize) -> Result<Self, EigenError> {
        let n = m.nrows();
        // Deterministic start vector — sin-based real start. The basis
        // picks up complex components on the first solve.
        let mut v: Vec<c64> = (0..n)
            .map(|i| c64::new((((i as f64) + 1.0) * 0.5432).sin(), 0.0))
            .collect();
        let mut mv = vec![c64::new(0.0, 0.0); n];
        spmv(m, &v, &mut mv);
        let v_t_m_v = bilinear(&v, &mv);
        // Bilinear M-norm² can be exactly zero for "isotropic" vectors in
        // the bilinear form. For the Mie problem this is pathologically rare
        // on a generic start; flag it and exit.
        if bilinear_isotropic(&v, &mv, v_t_m_v) {
            return Err(EigenError::FaerGevd(
                "starting vector is M-bilinear-isotropic (v^T M v ≈ 0); pick a different start"
                    .into(),
            ));
        }
        let nrm = principal_sqrt(v_t_m_v);
        let inv = c64::new(1.0, 0.0) / nrm;
        for x in v.iter_mut() {
            *x *= inv;
        }
        Ok(Self {
            basis: Vec::with_capacity(capacity),
            m_basis: Vec::with_capacity(capacity),
            alpha: Vec::with_capacity(capacity),
            beta: Vec::with_capacity(capacity),
            v,
            mv,
            w: vec![c64::new(0.0, 0.0); n],
            work: vec![c64::new(0.0, 0.0); n],
            stop: None,
            pending_beta: c64::new(0.0, 0.0),
            alpha_scale: 0.0,
        })
    }

    /// Whether the Krylov space is exhausted (a breakdown stop, which no
    /// extension can resume).
    fn broken_down(&self) -> bool {
        self.stop == Some(LanczosStop::Breakdown)
    }

    /// Run Lanczos steps until the basis holds `target` vectors or the
    /// recurrence stops; see the real solver's `LanczosRun::extend` for the
    /// two [`ExtendMode`]s.
    fn extend(
        &mut self,
        lu: &Lu<usize, c64>,
        m: SparseColMatRef<'_, usize, c64>,
        n_modes: usize,
        target: usize,
        tol: f64,
        mode: ExtendMode,
    ) -> Result<(), EigenError> {
        let n = m.nrows();
        if mode == ExtendMode::Extension && self.stop == Some(LanczosStop::BetaBound) {
            let nrm = self.pending_beta;
            if krylov_breakdown(nrm.norm(), self.alpha_scale, EXTENSION_BREAKDOWN_REL) {
                self.stop = Some(LanczosStop::Breakdown);
            } else {
                self.beta.push(nrm);
                let inv = c64::new(1.0, 0.0) / nrm;
                self.v = self.w.iter().map(|x| *x * inv).collect();
                self.stop = None;
            }
        }
        while self.stop.is_none() && self.alpha.len() < target {
            let j = self.alpha.len();
            spmv(m, &self.v, &mut self.mv);
            solve_with_lu(lu, &self.mv, &mut self.w)?;

            let w = &mut self.w;
            let v = &self.v;
            let mv = &self.mv;
            let mut aj = c64::new(0.0, 0.0);
            for i in 0..n {
                aj += mv[i] * w[i];
            }
            self.alpha.push(aj);
            self.alpha_scale = self.alpha_scale.max(aj.norm());

            for i in 0..n {
                w[i] -= aj * v[i];
            }
            if let Some(bp) = self.beta.last().copied() {
                let prev = &self.basis[j - 1];
                for i in 0..n {
                    w[i] -= bp * prev[i];
                }
            }

            // Full reorthogonalization in the bilinear M-inner product,
            // reusing the cached `M·v_k` (issue #506).
            for (vk, m_vk) in self.basis.iter().zip(self.m_basis.iter()) {
                let mut c = c64::new(0.0, 0.0);
                for i in 0..n {
                    c += m_vk[i] * w[i];
                }
                if c.re != 0.0 || c.im != 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Re-project off v itself. `mv` still holds `M·v`.
            let mut c = c64::new(0.0, 0.0);
            for i in 0..n {
                c += mv[i] * w[i];
            }
            for i in 0..n {
                w[i] -= c * v[i];
            }

            // β_j² = wᵀ M w.
            spmv(m, w, &mut self.work);
            let w_t_m_w = bilinear(w, &self.work);
            let nrm = principal_sqrt(w_t_m_w);

            self.m_basis.push(core::mem::take(&mut self.mv));
            self.mv = vec![c64::new(0.0, 0.0); n];
            self.basis.push(core::mem::take(&mut self.v));

            let beta_mag = nrm.re.hypot(nrm.im);
            match mode {
                ExtendMode::Historical => {
                    // Kaniel–Saad-flavored convergence probe: |β_j| relative
                    // to the largest |μ| of the tridiagonal (relative since
                    // issue #828; it was `tol · max(μ_max, 1)`).
                    if self.alpha.len() >= n_modes && self.alpha.len() >= 2 {
                        let mus = tridiag_complex_eigenvalues(&self.alpha, &self.beta)?;
                        let mu_max = mus.iter().fold(0.0_f64, |a, mu| a.max(mu.re.hypot(mu.im)));
                        if beta_mag <= tol * mu_max {
                            self.stop = Some(LanczosStop::BetaBound);
                            self.pending_beta = nrm;
                            break;
                        }
                    }
                    if krylov_breakdown(beta_mag, self.alpha_scale, HISTORICAL_BREAKDOWN_REL) {
                        self.stop = Some(LanczosStop::Breakdown);
                        break;
                    }
                }
                ExtendMode::Extension => {
                    if krylov_breakdown(beta_mag, self.alpha_scale, EXTENSION_BREAKDOWN_REL) {
                        self.stop = Some(LanczosStop::Breakdown);
                        break;
                    }
                }
            }

            self.beta.push(nrm);
            let inv = c64::new(1.0, 0.0) / nrm;
            self.v = self.w.iter().map(|x| *x * inv).collect();
        }
        Ok(())
    }

    /// Solve the complex tridiagonal eigenproblem and return each Ritz value
    /// `λ = σ + 1/μ` (skipping `μ = 0`) with its column in `S`, in column
    /// order, together with `S`.
    fn ritz_values(&self, sigma: f64) -> Result<ComplexRitzValues, EigenError> {
        if self.alpha.is_empty() {
            return Err(EigenError::FaerGevd(
                "complex Lanczos produced no iterations; trivial problem?".into(),
            ));
        }
        let (mus, s_mat) = tridiag_complex_eigenpairs(&self.alpha, &self.beta)?;
        let sigma_c = c64::new(sigma, 0.0);
        let values = mus
            .iter()
            .enumerate()
            .filter(|(_, mu)| mu.re.hypot(mu.im) != 0.0)
            .map(|(col, mu)| (sigma_c + c64::new(1.0, 0.0) / *mu, col))
            .collect();
        Ok((values, s_mat))
    }

    /// Form the Ritz vectors `x = V_k s_col` of `picks` (in the given order)
    /// and bilinear-M-normalize each (`xᵀ M x = 1`).
    fn ritz_vectors(
        &self,
        m: SparseColMatRef<'_, usize, c64>,
        s_mat: &Mat<c64>,
        picks: &[(c64, usize)],
    ) -> Vec<ComplexEigenPair> {
        let n = m.nrows();
        let k_eff = s_mat.nrows();
        let mut work = vec![c64::new(0.0, 0.0); n];
        let mut out = Vec::with_capacity(picks.len());
        for &(lambda, col) in picks {
            let mut x = vec![c64::new(0.0, 0.0); n];
            for row in 0..k_eff {
                let s_rc = s_mat[(row, col)];
                if s_rc.re == 0.0 && s_rc.im == 0.0 {
                    continue;
                }
                let basis_row = &self.basis[row];
                for i in 0..n {
                    x[i] += s_rc * basis_row[i];
                }
            }
            spmv(m, &x, &mut work);
            let norm2 = bilinear(&x, &work);
            if norm2.re.abs() + norm2.im.abs() > 0.0 {
                let s = principal_sqrt(norm2);
                let inv = c64::new(1.0, 0.0) / s;
                for v in x.iter_mut() {
                    *v *= inv;
                }
            }
            out.push(ComplexEigenPair { lambda, vector: x });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build a small complex-symmetric diagonal pencil with known
    /// eigenvalues. K and M are both diagonal so the eigenvalues are
    /// trivially `k_i / m_i`.
    fn diagonal_complex_pencil(
        diag_k: &[c64],
        diag_m: &[c64],
    ) -> (SparseColMat<usize, c64>, SparseColMat<usize, c64>) {
        let n = diag_k.len();
        let tk: Vec<Triplet<usize, usize, c64>> =
            (0..n).map(|i| Triplet::new(i, i, diag_k[i])).collect();
        let tm: Vec<Triplet<usize, usize, c64>> =
            (0..n).map(|i| Triplet::new(i, i, diag_m[i])).collect();
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
        (k, m)
    }

    #[test]
    fn complex_lanczos_diagonal_real_pencil() {
        // M is purely real positive, K is purely real — should recover
        // the same eigenvalues as the real path.
        let diag_k: Vec<c64> = [1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .map(|&x| c64::new(x, 0.0))
            .collect();
        let diag_m: Vec<c64> = (0..5).map(|_| c64::new(1.0, 0.0)).collect();
        let (k, m) = diagonal_complex_pencil(&diag_k, &diag_m);

        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
        };
        let lambdas = solver
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .unwrap();
        assert_eq!(lambdas.len(), 3);
        for (got, want) in lambdas.iter().zip([1.0, 2.0, 3.0].iter()) {
            assert!(
                (got.re - want).abs() < 1e-8 && got.im.abs() < 1e-10,
                "complex lanczos λ={got}, want {want} + 0i"
            );
        }
    }

    #[test]
    fn complex_lanczos_diagonal_distinct_pencil() {
        // λ_i = k_i / m_i = {1, 1.5, 2, 2.5, 3}. Lowest three are
        // {1, 1.5, 2}.
        let diag_k: Vec<c64> = [1.0, 3.0, 6.0, 10.0, 15.0]
            .iter()
            .map(|&x| c64::new(x, 0.0))
            .collect();
        let diag_m: Vec<c64> = [1.0, 2.0, 3.0, 4.0, 5.0]
            .iter()
            .map(|&x| c64::new(x, 0.0))
            .collect();
        let (k, m) = diagonal_complex_pencil(&diag_k, &diag_m);

        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
        };
        let lambdas = solver
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .unwrap();
        assert_eq!(lambdas.len(), 3);
        for (got, want) in lambdas.iter().zip([1.0, 1.5, 2.0].iter()) {
            assert!(
                (got.re - want).abs() < 1e-8 && got.im.abs() < 1e-10,
                "complex lanczos λ={got}, want {want} + 0i"
            );
        }
    }

    #[test]
    fn complex_lanczos_diagonal_complex_pencil() {
        // K real positive, M = real + j·small imaginary. Then
        // λ_i = k_i / m_i are complex. Specifically:
        //   k = [1, 2]
        //   m = [1 + 0.1i, 2 + 0.2i]
        //   λ = [1/(1+0.1i), 2/(2+0.2i)] = [1/1.01 (1 - 0.1i), same].
        // Both eigenvalues equal `(1 - 0.1i) / 1.01`. Lanczos returns
        // them (with multiplicity) — distinct in numerical noise.
        // We use distinct ratios to avoid the degeneracy:
        //   k = [1, 4]
        //   m = [1 + 0.1i, 2 + 0.3i]
        let k_trips = vec![
            Triplet::new(0, 0, c64::new(1.0, 0.0)),
            Triplet::new(1, 1, c64::new(4.0, 0.0)),
        ];
        let m_trips = vec![
            Triplet::new(0, 0, c64::new(1.0, 0.1)),
            Triplet::new(1, 1, c64::new(2.0, 0.3)),
        ];
        let k = SparseColMat::try_new_from_triplets(2, 2, &k_trips).unwrap();
        let m = SparseColMat::try_new_from_triplets(2, 2, &m_trips).unwrap();

        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 10,
            tol: 1e-12,
        };
        let lambdas = solver
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), 2)
            .unwrap();
        assert_eq!(lambdas.len(), 2);

        let want0 = c64::new(1.0, 0.0) / c64::new(1.0, 0.1);
        let want1 = c64::new(4.0, 0.0) / c64::new(2.0, 0.3);
        // Sort references the same way Lanczos sorts the output
        // (ascending by Re).
        let (w_lo, w_hi) = if want0.re <= want1.re {
            (want0, want1)
        } else {
            (want1, want0)
        };
        let err0 = (lambdas[0] - w_lo).re.hypot((lambdas[0] - w_lo).im);
        let err1 = (lambdas[1] - w_hi).re.hypot((lambdas[1] - w_hi).im);
        assert!(
            err0 < 1e-9,
            "λ_0 = {}, want {}, err = {}",
            lambdas[0],
            w_lo,
            err0
        );
        assert!(
            err1 < 1e-9,
            "λ_1 = {}, want {}, err = {}",
            lambdas[1],
            w_hi,
            err1
        );
    }

    /// Eigenpair recovery (#332): the eigenvectors of a diagonal complex
    /// pencil are the canonical basis vectors. We assert the recovered
    /// eigenvalues match the eigenvalue-only path, the eigenvectors are
    /// bilinear-M-normalized (`xᵀ M x = 1`), and each `(λ, x)` satisfies
    /// the residual `K x − λ M x ≈ 0`.
    #[test]
    fn complex_lanczos_eigenpairs_diagonal() {
        // Distinct complex ratios λ_i = k_i / m_i over a 6×6 diagonal.
        let diag_k: Vec<c64> = [
            c64::new(1.0, 0.0),
            c64::new(3.0, 0.0),
            c64::new(6.0, 0.0),
            c64::new(10.0, 0.0),
            c64::new(15.0, 0.0),
            c64::new(21.0, 0.0),
        ]
        .to_vec();
        let diag_m: Vec<c64> = [
            c64::new(1.0, 0.05),
            c64::new(2.0, 0.10),
            c64::new(3.0, 0.15),
            c64::new(4.0, 0.20),
            c64::new(5.0, 0.25),
            c64::new(6.0, 0.30),
        ]
        .to_vec();
        let (k, m) = diagonal_complex_pencil(&diag_k, &diag_m);
        let n = diag_k.len();

        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-12,
        };
        let n_modes = 3;
        let pairs = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        let lambdas = solver
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        assert_eq!(pairs.len(), n_modes);

        for (idx, p) in pairs.iter().enumerate() {
            // Eigenvalue agrees with the eigenvalue-only path.
            let de = (p.lambda - lambdas[idx]).norm();
            assert!(
                de < 1e-8,
                "pair λ {} disagrees with eigenvalue-only λ {} (err {de:.3e})",
                p.lambda,
                lambdas[idx]
            );

            // Bilinear M-norm = 1: xᵀ M x = Σ m_i x_i².
            let xmx: c64 = (0..n).map(|i| diag_m[i] * p.vector[i] * p.vector[i]).sum();
            assert!(
                (xmx - c64::new(1.0, 0.0)).norm() < 1e-8,
                "eigenvector must be bilinear-M-normalized: xᵀMx = {xmx}"
            );

            // Residual K x − λ M x ≈ 0 (diagonal: per-component).
            for i in 0..n {
                let res = (diag_k[i] - p.lambda * diag_m[i]) * p.vector[i];
                assert!(
                    res.norm() < 1e-7,
                    "residual at row {i} too large: {res} (λ={})",
                    p.lambda
                );
            }
        }
    }

    /// CORRECTNESS GATE (issue #518), complex path: the computed spectrum
    /// must be identical whether the complex sparse-LU factorization runs
    /// single-threaded or multi-threaded. Uses a non-diagonal
    /// complex-symmetric tridiagonal pencil so the LU has real fill.
    ///
    /// Drives the thread count through the `_with_threads` entry point (the
    /// same path `GEODE_NUM_THREADS` feeds) to avoid mutating a
    /// process-global env var (this crate denies `unsafe_code`). Holds the
    /// shared parallelism lock so the transient global-parallelism change
    /// during factorization does not race the guard RAII test in
    /// `eigen::parallel`.
    #[test]
    fn complex_eigenvalues_agree_across_thread_counts() {
        let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
            .lock()
            .unwrap();

        // Complex-symmetric tridiagonal K = tridiag(-1, 2 + 0.1i, -1),
        // M = identity. Off-diagonal fill exercises the multi-threaded LU.
        let n = 40usize;
        let mut tk: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(3 * n);
        let mut tm: Vec<Triplet<usize, usize, c64>> = Vec::with_capacity(n);
        for i in 0..n {
            tk.push(Triplet::new(i, i, c64::new(2.0, 0.1)));
            if i + 1 < n {
                tk.push(Triplet::new(i, i + 1, c64::new(-1.0, 0.0)));
                tk.push(Triplet::new(i + 1, i, c64::new(-1.0, 0.0)));
            }
            tm.push(Triplet::new(i, i, c64::new(1.0, 0.0)));
        }
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();

        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-11,
        };

        let serial = solver
            .smallest_complex_pencil_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 4, 1)
            .unwrap();
        let parallel = solver
            .smallest_complex_pencil_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 4, 4)
            .unwrap();

        assert_eq!(
            serial.len(),
            parallel.len(),
            "thread count changed the number of converged modes"
        );
        for (i, (s, p)) in serial.iter().zip(parallel.iter()).enumerate() {
            assert_eq!(
                s.re.to_bits(),
                p.re.to_bits(),
                "eigenvalue[{i}] Re differs across thread counts: {s} vs {p}"
            );
            assert_eq!(
                s.im.to_bits(),
                p.im.to_bits(),
                "eigenvalue[{i}] Im differs across thread counts: {s} vs {p}"
            );
        }
    }

    /// The unit-cube PEC cavity curl-curl pencil (`n = 3`), lifted to
    /// complex. `K` has the discrete-gradient null space (one direction per
    /// free interior node) — the real-world shape of the issue #696 hazard.
    fn cube_curl_curl_complex_pencil() -> (SparseColMat<usize, c64>, SparseColMat<usize, c64>) {
        cube_curl_curl_complex_pencil_sized(1.0, c64::new(1.0, 0.0))
    }

    /// [`cube_curl_curl_complex_pencil`] for a cube of side `side`, with the
    /// mass scaled by the complex permittivity `eps_fill` (a uniform lossy
    /// fill for `Im eps_fill < 0`, so `λ = λ_lossless / eps`).
    fn cube_curl_curl_complex_pencil_sized(
        side: f64,
        eps_fill: c64,
    ) -> (SparseColMat<usize, c64>, SparseColMat<usize, c64>) {
        use crate::testing::TestBackend;
        use burn::tensor::backend::BackendTypes;
        let mesh = crate::mesh::cube_tet_mesh(3, side);
        let (_, mask) = crate::assembly::nedelec::cube_pec_interior_edges(&mesh, side);
        let eps = vec![1.0; mesh.n_tets()];
        let dev = <TestBackend as BackendTypes>::Device::default();
        let (k, m) = crate::eigen::pec_cavity::assemble_lossless_pencil::<TestBackend>(
            &mesh, &eps, &mask, &dev,
        )
        .unwrap();
        let lift = |a: &SparseColMat<usize, f64>, scale: c64| {
            let a = a.as_ref();
            let mut t = Vec::new();
            for j in 0..a.ncols() {
                for p in a.col_ptr()[j]..a.col_ptr()[j + 1] {
                    t.push(Triplet::new(a.row_idx()[p], j, scale * a.val()[p]));
                }
            }
            SparseColMat::try_new_from_triplets(a.nrows(), a.ncols(), &t).unwrap()
        };
        (lift(&k, c64::new(1.0, 0.0)), lift(&m, eps_fill))
    }

    /// Issue #696 root cause, reproduced on a real curl-curl pencil:
    /// `σ = 0` factors the singular `K` without error and every Ritz value
    /// collapses onto the gradient null cluster. Both solve paths must now
    /// fail with `DegenerateShift` instead of returning that set as `Ok`.
    #[test]
    fn zero_shift_on_curl_curl_pencil_is_rejected_as_degenerate() {
        let (k, m) = cube_curl_curl_complex_pencil();
        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-9,
        };
        for n_modes in [1usize, 4] {
            match solver.smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes) {
                Err(EigenError::DegenerateShift {
                    sigma, n_returned, ..
                }) => {
                    assert_eq!(sigma, 0.0);
                    assert_eq!(n_returned, n_modes);
                }
                other => panic!("n_modes={n_modes}: expected DegenerateShift, got {other:?}"),
            }
            assert!(
                matches!(
                    solver.smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes),
                    Err(EigenError::DegenerateShift { .. })
                ),
                "eigenpairs path must also reject σ = 0 (n_modes={n_modes})"
            );
        }
    }

    /// Non-trigger: the same curl-curl pencil with a physical shift, and
    /// with `σ` placed *very* close (1e-6 relative) to the lowest physical
    /// eigenvalue, solves normally — the detector must not fire on a
    /// legitimate problem whose lowest mode is near `σ`.
    #[test]
    fn physical_shift_near_lowest_mode_is_not_flagged() {
        let (k, m) = cube_curl_curl_complex_pencil();
        let two_pi2 = 2.0 * core::f64::consts::PI.powi(2);
        let base = SparseComplexShiftInvertLanczos {
            sigma: 0.7 * two_pi2,
            max_iters: 64,
            tol: 1e-9,
        };
        let l = base
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), 3)
            .expect("physical shift must solve");
        let lam0 = l[0];
        assert!(
            (lam0.re - two_pi2).abs() / two_pi2 < 0.25 && lam0.im.abs() < 1e-8,
            "lowest physical λ = {lam0:?}, want ≈ 2π² = {two_pi2}"
        );
        let near = SparseComplexShiftInvertLanczos {
            sigma: lam0.re * (1.0 + 1e-6),
            ..base
        };
        for n_modes in [1usize, 4] {
            let got = near
                .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
                .unwrap_or_else(|e| panic!("near-mode shift, n_modes={n_modes}: {e}"));
            assert!(
                got.iter().any(|z| (z.re - lam0.re).abs() < 1e-8 * lam0.re),
                "n_modes={n_modes}: lowest mode not recovered: {got:?}"
            );
            near.smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
                .unwrap_or_else(|e| panic!("near-mode eigenpairs, n_modes={n_modes}: {e}"));
        }
    }

    /// Non-trigger (PR #701 review): a shift placed **exactly** on a
    /// computed physical eigenvalue is a well-posed shift-invert solve
    /// (mode tracking / continuation re-solves at `σ = λ_prev`), even though
    /// every returned Ritz value then sits on `σ`. Covers `n_modes = 1` on a
    /// simple eigenvalue and `n_modes ∈ {1, 2, 3}` on the cube's lowest
    /// physical level (a triplet, λ ≈ 23.50), on both solve paths. Single-vector
    /// Lanczos under-resolves exact multiplicity in the survey solve (it
    /// returns two copies), so the level is picked as the first with ≥ 2
    /// coincident values; `n_modes = 3` at `σ` then asserts all three copies.
    #[test]
    fn shift_exactly_on_physical_eigenvalue_is_not_flagged() {
        let (k, m) = cube_curl_curl_complex_pencil();
        let two_pi2 = 2.0 * core::f64::consts::PI.powi(2);
        let base = SparseComplexShiftInvertLanczos {
            sigma: 0.7 * two_pi2,
            max_iters: 64,
            tol: 1e-9,
        };
        let l: Vec<f64> = base
            .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), 10)
            .expect("physical shift must solve")
            .iter()
            .map(|z| z.re)
            .collect();
        let rel = |a: f64, b: f64| (a - b).abs() / b.abs();
        // Group the physical (λ > 1, i.e. off the gradient null cluster)
        // eigenvalues into numerically-degenerate levels.
        let mut levels: Vec<(f64, usize)> = Vec::new();
        for &x in l.iter().filter(|&&x| x > 1.0) {
            match levels.last_mut() {
                Some((v, c)) if rel(x, *v) < 1e-8 => *c += 1,
                _ => levels.push((x, 1)),
            }
        }
        let triplet = levels
            .iter()
            .find(|(_, c)| *c >= 2)
            .unwrap_or_else(|| panic!("no multiplet among {l:?}"))
            .0;
        let simple = levels
            .iter()
            .find(|(_, c)| *c == 1)
            .unwrap_or_else(|| panic!("no simple eigenvalue among {l:?}"))
            .0;

        let check = |sigma: f64, n_modes: usize| {
            let at = SparseComplexShiftInvertLanczos { sigma, ..base };
            let got = at
                .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
                .unwrap_or_else(|e| panic!("σ = λ = {sigma}, n_modes={n_modes}: {e}"));
            assert_eq!(got.len(), n_modes);
            for z in &got {
                assert!(
                    rel(z.re, sigma) < 1e-8 && z.im.abs() < 1e-8 * sigma,
                    "σ = {sigma}, n_modes={n_modes}: λ = {z:?} not on σ"
                );
            }
            let pairs = at
                .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
                .unwrap_or_else(|e| panic!("eigenpairs σ = λ = {sigma}, n_modes={n_modes}: {e}"));
            assert_eq!(pairs.len(), n_modes);
            for p in &pairs {
                assert!(
                    rel(p.lambda.re, sigma) < 1e-8,
                    "eigenpairs σ = {sigma}, n_modes={n_modes}: λ = {:?} not on σ",
                    p.lambda
                );
            }
        };
        check(simple, 1);
        for n_modes in 1..=3 {
            check(triplet, n_modes);
        }
    }

    /// The seeded diagonal pencil of the issue #834 regression: `M = I`, the
    /// eigenvalues `0.82 − 0.02j`, `1.15 − 0.01j`, `1.3 − 0.005j` plus 37
    /// passive (`Im λ ≤ 0`) values spread around `σ = 1` at distances
    /// `0.25…4.25`. Built from `+ − × ÷` only (a rational angle
    /// parametrization, no `sin` / `cos`), so it is bit-identical on every
    /// platform.
    fn spurious_ritz_pencil() -> (Vec<c64>, SparseColMat<usize, c64>, SparseColMat<usize, c64>) {
        let lcg = |s: u64| {
            s.wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407)
        };
        let mut st = lcg(349);
        let mut rnd = || {
            st = lcg(st);
            ((st >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let n = 40;
        let mut lam = vec![
            c64::new(1.15, -0.01),
            c64::new(0.82, -0.02),
            c64::new(1.3, -0.005),
        ];
        while lam.len() < n {
            let r = 0.25 + 4.0 * rnd();
            let u = rnd();
            let t = u / (1.0 - u + 1e-3);
            let d = 1.0 + t * t;
            lam.push(c64::new(1.0 + r * (1.0 - t * t) / d, -r * 2.0 * t / d));
        }
        let ones: Vec<c64> = (0..n).map(|_| c64::new(1.0, 0.0)).collect();
        let (k, m) = diagonal_complex_pencil(&lam, &ones);
        (lam, k, m)
    }

    /// Issue #834 regression. On a passive pencil, a 16-step plain solve
    /// returns, among its 3 Ritz values nearest `σ = 1`, a spurious one
    /// (`λ ≈ 1.0900 + 0.1486j`, relative residual ≈ 2.2, with the unphysical
    /// sign of `Im λ`). It sits nearer `σ` than a converged mode, which no
    /// interlacing prevents in the bilinear Lanczos. The checked solve
    /// withholds it, extends the same Krylov run, and returns the 3 true
    /// eigenvalues nearest `σ`, all converged.
    #[test]
    fn checked_withholds_spurious_ritz_value_near_sigma() {
        let (lam, k, m) = spurious_ritz_pencil();
        let n = lam.len();
        let sigma = 1.0;
        let dist = |l: c64| (l.re - sigma).hypot(l.im);
        let solver = SparseComplexShiftInvertLanczos {
            sigma,
            max_iters: 16,
            tol: 1e-12,
        };
        let n_modes = 3;
        let residual = |p: &ComplexEigenPair| {
            let (mut kx, mut mx) = (vec![c64::new(0.0, 0.0); n], vec![c64::new(0.0, 0.0); n]);
            complex_pair_relative_residual(
                k.as_ref(),
                m.as_ref(),
                p.lambda,
                &p.vector,
                sigma,
                &mut kx,
                &mut mx,
            )
        };
        let plain = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        let plain_res: Vec<f64> = plain.iter().map(residual).collect();
        eprintln!(
            "plain: {:?}",
            plain
                .iter()
                .zip(&plain_res)
                .map(|(p, r)| (p.lambda, *r))
                .collect::<Vec<_>>()
        );
        let converged_far = plain
            .iter()
            .zip(&plain_res)
            .filter(|(_, r)| **r < 1e-9)
            .map(|(p, _)| dist(p.lambda))
            .fold(0.0_f64, f64::max);
        let spurious = plain
            .iter()
            .zip(&plain_res)
            .find(|(p, r)| **r > 1.0 && p.lambda.im > 0.0 && dist(p.lambda) < converged_far);
        assert!(
            spurious.is_some(),
            "the plain solve should return a spurious value nearer σ than a converged one"
        );

        let checked = solver
            .smallest_eigenpairs_checked(
                k.as_ref(),
                m.as_ref(),
                n_modes,
                ConvergenceCheck {
                    residual_tol: 1e-9,
                    max_iters_cap: n,
                    window: None,
                },
            )
            .unwrap();
        eprintln!(
            "checked: {:?}, rejected {:?}, {} steps, extended {}",
            checked
                .pairs
                .iter()
                .zip(&checked.residuals)
                .map(|(p, r)| (p.lambda, *r))
                .collect::<Vec<_>>(),
            checked.rejected,
            checked.lanczos_steps,
            checked.extended
        );
        assert!(checked.extended);
        assert_eq!(checked.shortfall(), 0);
        let mut exact = lam.clone();
        exact.sort_by(|a, b| dist(*a).total_cmp(&dist(*b)));
        let nearest_gap = |l: c64| {
            exact
                .iter()
                .map(|e| (l - e).norm())
                .fold(f64::INFINITY, f64::min)
        };
        for (p, &r) in checked.pairs.iter().zip(&checked.residuals) {
            assert!(r <= 1e-9);
            assert!(p.lambda.im <= 0.0, "unphysical λ = {} returned", p.lambda);
            assert!(
                nearest_gap(p.lambda) < 1e-9,
                "λ = {} is no eigenvalue",
                p.lambda
            );
        }
        for e in &exact[..n_modes] {
            assert!(
                checked.pairs.iter().any(|p| (p.lambda - e).norm() < 1e-9),
                "exact eigenvalue {e} nearest σ missing from {:?}",
                checked.pairs.iter().map(|p| p.lambda).collect::<Vec<_>>()
            );
        }
        // The eligibility filter is applied before the nearest-σ selection:
        // excluding the converged 0.82 mode yields the next eigenvalue out.
        let filtered = solver
            .smallest_eigenpairs_checked_filtered(
                k.as_ref(),
                m.as_ref(),
                n_modes,
                ConvergenceCheck {
                    residual_tol: 1e-9,
                    max_iters_cap: n,
                    window: None,
                },
                &|l: c64| l.re > 0.9,
            )
            .unwrap();
        assert_eq!(filtered.shortfall(), 0);
        assert!(filtered.screened.iter().all(|l| l.re <= 0.9));
        let eligible_exact: Vec<c64> = exact.iter().copied().filter(|l| l.re > 0.9).collect();
        for e in &eligible_exact[..n_modes] {
            assert!(
                filtered.pairs.iter().any(|p| (p.lambda - e).norm() < 1e-9),
                "eligible eigenvalue {e} missing"
            );
        }
        assert!(filtered.pairs.iter().all(|p| p.lambda.re > 0.9));
    }

    /// When the first pass converges, the checked solve returns exactly the
    /// plain solve's pairs, bit for bit, with no extension (issue #834).
    #[test]
    fn checked_is_bit_identical_when_first_pass_converges() {
        let (k, m) = cube_curl_curl_complex_pencil_sized(1.0, c64::new(1.0, -0.05));
        let two_pi2 = 2.0 * core::f64::consts::PI.powi(2);
        let solver = SparseComplexShiftInvertLanczos {
            sigma: 0.7 * two_pi2,
            max_iters: 64,
            tol: 1e-9,
        };
        let plain = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), 3)
            .unwrap();
        let checked = solver
            .smallest_eigenpairs_checked(
                k.as_ref(),
                m.as_ref(),
                3,
                ConvergenceCheck {
                    residual_tol: 1e-8,
                    max_iters_cap: 200,
                    window: None,
                },
            )
            .unwrap();
        assert!(!checked.extended);
        assert!(checked.rejected.is_empty());
        assert_eq!(checked.pairs.len(), plain.len());
        for (a, b) in checked.pairs.iter().zip(&plain) {
            assert_eq!(a.lambda.re.to_bits(), b.lambda.re.to_bits());
            assert_eq!(a.lambda.im.to_bits(), b.lambda.im.to_bits());
            assert!(
                a.vector
                    .iter()
                    .zip(&b.vector)
                    .all(|(x, y)| x.re.to_bits() == y.re.to_bits()
                        && x.im.to_bits() == y.im.to_bits())
            );
        }
    }

    /// Issue #828 regression: the same lossy cavity (`ε_r = 1 − 0.05j`)
    /// meshed with side `1`, `1e-6` and `1e3` gives the same `λ · side²` on
    /// the eigenvalue-only, eigenpair and checked paths, with converged
    /// pairs. With the old `tol · max(μ_max, 1)` probe, absolute `|β| < 1e-14`
    /// breakdown and absolute `|vᵀMv| < 1e-30` start-vector test, the `1e-6`
    /// cavity (`|μ| ~ 1e-13`) stopped after a step or two.
    #[test]
    fn complex_solves_are_mesh_unit_invariant() {
        let two_pi2 = 2.0 * core::f64::consts::PI.powi(2);
        let n_modes = 3;
        let eps = c64::new(1.0, -0.05);
        let solve = |side: f64| {
            let (k, m) = cube_curl_curl_complex_pencil_sized(side, eps);
            let sigma = 0.7 * two_pi2 / (side * side);
            let solver = SparseComplexShiftInvertLanczos {
                sigma,
                max_iters: 64,
                tol: 1e-9,
            };
            let unit = |l: c64| l * (side * side);
            let values: Vec<c64> = solver
                .smallest_complex_pencil_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
                .unwrap()
                .into_iter()
                .map(unit)
                .collect();
            let pairs = solver
                .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
                .unwrap();
            let n = k.nrows();
            let (mut kx, mut mx) = (vec![c64::new(0.0, 0.0); n], vec![c64::new(0.0, 0.0); n]);
            let res: Vec<f64> = pairs
                .iter()
                .map(|p| {
                    complex_pair_relative_residual(
                        k.as_ref(),
                        m.as_ref(),
                        p.lambda,
                        &p.vector,
                        sigma,
                        &mut kx,
                        &mut mx,
                    )
                })
                .collect();
            let checked = solver
                .smallest_eigenpairs_checked(
                    k.as_ref(),
                    m.as_ref(),
                    n_modes,
                    ConvergenceCheck {
                        residual_tol: 1e-9,
                        max_iters_cap: 200,
                        window: None,
                    },
                )
                .unwrap();
            (
                values,
                pairs.iter().map(|p| unit(p.lambda)).collect::<Vec<_>>(),
                res,
                checked
                    .pairs
                    .iter()
                    .map(|p| unit(p.lambda))
                    .collect::<Vec<_>>(),
            )
        };
        let (ref_values, ref_pairs, ref_res, ref_checked) = solve(1.0);
        assert!(
            ref_res.iter().all(|&r| r < 1e-9),
            "unit residuals {ref_res:?}"
        );
        for side in [1e-6, 1e3] {
            let (values, pairs, res, checked) = solve(side);
            eprintln!("side {side:e}: λ·side² = {pairs:?}, residuals {res:?}");
            assert!(
                res.iter().all(|&r| r < 1e-9),
                "side {side:e}: residuals {res:?}"
            );
            for (got, want, what) in [
                (&values, &ref_values, "eigenvalues"),
                (&pairs, &ref_pairs, "eigenpairs"),
                (&checked, &ref_checked, "checked"),
            ] {
                assert_eq!(got.len(), want.len(), "side {side:e} {what}");
                for (a, b) in got.iter().zip(want) {
                    assert!(
                        (a - b).norm() < 1e-10 * b.norm(),
                        "side {side:e} {what}: λ·side² = {a} vs {b}"
                    );
                }
            }
        }
    }
}
