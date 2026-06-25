//! Sparse shift-and-invert Lanczos for **complex-symmetric** generalized
//! eigenproblems `K x = λ M x` where `K` and `M` are complex sparse
//! matrices and the pencil is complex-symmetric (`K^T = K`, `M^T = M`,
//! both *without* conjugation). This is the complex analog of
//! [`crate::lanczos::SparseShiftInvertLanczos`] (issue #53).
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
//! # Why this loop is not ported onto [`crate::iterate`]
//!
//! Like the real path ([`crate::lanczos`]), this complex-symmetric
//! variant shares the growing-basis structure: full reorthogonalization
//! keeps the whole history, so the Krylov basis gains one column per
//! iteration. That violates [`crate::iterate`] **contract restriction 1**
//! (loop-invariant carried-state shapes), so the loop is intentionally
//! left off the `iterate_while` combinator. See [`crate::lanczos`] for the
//! full rationale.

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef};
use faer::{Mat, MatMut, c64};

use crate::eigen::EigenError;

/// Sparse generalized complex-symmetric eigensolver via shift-and-invert
/// Lanczos.
///
/// Mirrors [`crate::lanczos::SparseShiftInvertLanczos`] for complex
/// matrices. `sigma` is a **real** shift — for the Mie path we want
/// the lowest physical `k²` eigenvalues, which are positive real
/// (with small imaginary parts from the PML). A real shift keeps the
/// LU factor of `K - σ M` cheap to set up.
#[derive(Debug, Clone, Copy)]
pub struct SparseComplexShiftInvertLanczos {
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

/// Parallel of [`crate::complex_eigen::ComplexEigenSolver`] for sparse
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
fn principal_sqrt(z: c64) -> c64 {
    if z.re == 0.0 && z.im == 0.0 {
        return c64::new(0.0, 0.0);
    }
    let r = (z.re * z.re + z.im * z.im).sqrt();
    let re = ((r + z.re) * 0.5).sqrt();
    let im_mag = ((r - z.re) * 0.5).sqrt();
    let im = if z.im >= 0.0 { im_mag } else { -im_mag };
    c64::new(re, im)
}

/// Build `K - σ M` as a fresh complex sparse matrix. Mirrors
/// `shifted_pencil` in [`crate::lanczos`].
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
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Build A = K - σM and factor it once.
        let a = shifted_pencil_complex(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("complex sparse LU: {e:?}")))?;

        // 2. Lanczos in the bilinear M-inner product. The tridiagonal
        //    `T_k` is complex symmetric (not Hermitian) and its
        //    eigenvalues approximate the inverse-shift map of the
        //    original pencil's eigenvalues.
        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);

        let mut basis: Vec<Vec<c64>> = Vec::with_capacity(max_k);
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
        if v_t_m_v.re.abs() + v_t_m_v.im.abs() < 1e-30 {
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
            // For each basis vector v_k, c = v_k^T M w = (M v_k)^T w.
            for vk in basis.iter() {
                spmv(m, vk, &mut work);
                let mut c = c64::new(0.0, 0.0);
                for i in 0..n {
                    c += work[i] * w[i];
                }
                if c.re != 0.0 || c.im != 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Re-project off v itself (about to enter basis).
            spmv(m, &v, &mut work);
            let mut c = c64::new(0.0, 0.0);
            for i in 0..n {
                c += work[i] * w[i];
            }
            for i in 0..n {
                w[i] -= c * v[i];
            }

            // β_j² = w^T M w.
            spmv(m, &w, &mut work);
            let w_t_m_w = bilinear(&w, &work);
            nrm = principal_sqrt(w_t_m_w);

            // Push current v as basis[j].
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
                    // complex here so we use its magnitude.
                    let mu_max = mus.iter().fold(0.0_f64, |a, mu| a.max(mu.re.hypot(mu.im)));
                    let beta_mag = nrm.re.hypot(nrm.im);
                    if beta_mag <= self.tol * mu_max.max(1.0) {
                        converged = Some(picked);
                        break;
                    }
                    converged = Some(picked);
                }
            }

            // Numerical breakdown — invariant subspace exhausted.
            if nrm.re.hypot(nrm.im) < 1e-14 {
                break;
            }

            beta.push(nrm);
            let inv = c64::new(1.0, 0.0) / nrm;
            v = w.iter().map(|x| *x * inv).collect();
        }

        converged.ok_or_else(|| {
            EigenError::FaerGevd(format!(
                "complex Lanczos terminated after {} iters without computing {} ritz pairs",
                alpha.len(),
                n_modes
            ))
        })
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
    /// [`crate::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs`]:
    /// it runs the same bilinear-form Lanczos (full reorthogonalization,
    /// the tridiagonal `T_k` complex symmetric), retains the full basis
    /// `V_k`, then recovers Ritz vectors `x = V_k s` from the complex
    /// tridiagonal eigenvectors `s`, maps `μ → σ + 1/μ`, sorts by
    /// `|λ − σ|`, and bilinear-M-normalizes each kept vector.
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

        // 1. Build A = K - σM and factor it once.
        let a = shifted_pencil_complex(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("complex sparse LU: {e:?}")))?;

        // 2. Lanczos in the bilinear M-inner product, retaining the full
        //    basis V_k for Ritz-vector recovery. Always run to the
        //    requested mode count (the recovery needs the basis even if
        //    convergence formally lags).
        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);
        let mut basis: Vec<Vec<c64>> = Vec::with_capacity(max_k);
        let mut alpha: Vec<c64> = Vec::with_capacity(max_k);
        let mut beta: Vec<c64> = Vec::with_capacity(max_k);

        let mut v: Vec<c64> = (0..n)
            .map(|i| c64::new((((i as f64) + 1.0) * 0.5432).sin(), 0.0))
            .collect();
        let mut mv = vec![c64::new(0.0, 0.0); n];
        spmv(m, &v, &mut mv);
        let v_t_m_v = bilinear(&v, &mv);
        if v_t_m_v.re.abs() + v_t_m_v.im.abs() < 1e-30 {
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

        let mut w = vec![c64::new(0.0, 0.0); n];
        let mut work = vec![c64::new(0.0, 0.0); n];

        for j in 0..max_k {
            spmv(m, &v, &mut mv);
            solve_with_lu(&lu, &mv, &mut w)?;

            let mut aj = c64::new(0.0, 0.0);
            for i in 0..n {
                aj += mv[i] * w[i];
            }
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

            // Full reorthogonalization in the bilinear M-inner product.
            for vk in basis.iter() {
                spmv(m, vk, &mut work);
                let mut c = c64::new(0.0, 0.0);
                for i in 0..n {
                    c += work[i] * w[i];
                }
                if c.re != 0.0 || c.im != 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            spmv(m, &v, &mut work);
            let mut c = c64::new(0.0, 0.0);
            for i in 0..n {
                c += work[i] * w[i];
            }
            for i in 0..n {
                w[i] -= c * v[i];
            }

            spmv(m, &w, &mut work);
            let w_t_m_w = bilinear(&w, &work);
            nrm = principal_sqrt(w_t_m_w);

            basis.push(core::mem::take(&mut v));

            // Convergence probe — same Kaniel–Saad-flavored bound as the
            // eigenvalue-only path.
            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let mus = tridiag_complex_eigenvalues(&alpha, &beta)?;
                let mu_max = mus.iter().fold(0.0_f64, |a, mu| a.max(mu.re.hypot(mu.im)));
                let beta_mag = nrm.re.hypot(nrm.im);
                if beta_mag <= self.tol * mu_max.max(1.0) {
                    break;
                }
            }

            if nrm.re.hypot(nrm.im) < 1e-14 {
                break;
            }

            beta.push(nrm);
            let inv = c64::new(1.0, 0.0) / nrm;
            v = w.iter().map(|x| *x * inv).collect();
        }

        if alpha.is_empty() {
            return Err(EigenError::FaerGevd(
                "complex Lanczos produced no iterations; trivial problem?".into(),
            ));
        }

        // 3. Tridiagonal eigenpairs.
        let (mus, s_mat) = tridiag_complex_eigenpairs(&alpha, &beta)?;
        let k_eff = mus.len();

        // 4. Build (λ, ritz_vector) pairs, filter near-zero μ, sort by
        //    |λ − σ| ascending.
        let sigma_c = c64::new(self.sigma, 0.0);
        let mut pairs: Vec<(c64, Vec<c64>)> = Vec::with_capacity(k_eff);
        for col in 0..k_eff {
            let mu = mus[col];
            if mu.re.hypot(mu.im) == 0.0 {
                continue;
            }
            let lambda = sigma_c + c64::new(1.0, 0.0) / mu;
            let mut x = vec![c64::new(0.0, 0.0); n];
            for row in 0..k_eff {
                let s_rc = s_mat[(row, col)];
                if s_rc.re == 0.0 && s_rc.im == 0.0 {
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
            let da = (a.0.re - self.sigma).hypot(a.0.im);
            let db = (b.0.re - self.sigma).hypot(b.0.im);
            da.partial_cmp(&db).unwrap_or(core::cmp::Ordering::Equal)
        });

        let take = n_modes.min(pairs.len());
        let mut picked: Vec<(c64, Vec<c64>)> = pairs.into_iter().take(take).collect();
        // Re-sort by ascending Re(λ) — matches the eigenvalue-only convention.
        picked.sort_by(|a, b| {
            a.0.re
                .partial_cmp(&b.0.re)
                .unwrap_or(core::cmp::Ordering::Equal)
        });

        // 5. Bilinear-M-normalize each Ritz vector: divide by sqrt(xᵀ M x).
        let mut out = Vec::with_capacity(take);
        for (lambda, mut x) in picked {
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
        Ok(out)
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
}
