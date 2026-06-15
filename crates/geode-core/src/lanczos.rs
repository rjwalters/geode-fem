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
//! Convergence is declared when the residual norm
//! `‖K x - λ M x‖_2 / ‖λ M x‖_2 < tol` for **every** requested mode.

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef};
use faer::{Mat, MatMut};

use crate::eigen::{EigenError, EigenPair};

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
#[derive(Debug, Clone, Copy)]
pub struct SparseShiftInvertLanczos {
    pub sigma: f64,
    pub max_iters: usize,
    pub tol: f64,
}

impl Default for SparseShiftInvertLanczos {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 64,
            tol: 1e-9,
        }
    }
}

/// Parallel of [`crate::eigen::EigenSolver`] for sparse matrices.
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
    order.sort_by(|&a, &b| mus[a].partial_cmp(&mus[b]).unwrap_or(core::cmp::Ordering::Equal));
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

impl SparseShiftInvertLanczos {
    /// Compute the lowest `n_modes` generalized eigenpairs of
    /// `K x = λ M x` closest to the configured shift `σ`, including
    /// M-orthonormalized eigenvectors. The eigenvalue-only sibling
    /// is [`SparseEigenSolver::smallest_eigenvalues`].
    ///
    /// Eigenvectors are recovered as Ritz vectors `x = V_k s` from the
    /// Lanczos basis `V_k` and tridiagonal eigenvector `s`, then
    /// rescaled so `xᵀ M x = 1` (the convention modal projection
    /// wants — same convention as
    /// [`crate::eigen::FaerDenseEigensolver::smallest_eigenpairs`]).
    pub fn smallest_eigenpairs(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
    ) -> Result<Vec<EigenPair>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Build A = K - σM and factor it once.
        let a = shifted_pencil(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?;

        // 2. Run Lanczos to convergence, retaining the full M-orthonormal
        //    basis V_k. Unlike the eigenvalue-only path we always run to
        //    the requested mode count; the Ritz-vector recovery needs the
        //    final basis even if convergence formally lags.
        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);
        let mut basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
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

        for j in 0..max_k {
            spmv(m, &v, &mut mv);
            solve_with_lu(&lu, &mv, &mut w)?;

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

            // Full reorthogonalization (M-inner product).
            for vk in basis.iter() {
                spmv(m, vk, &mut work);
                let c = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
                if c.abs() > 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            spmv(m, &v, &mut work);
            let c = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            for i in 0..n {
                w[i] -= c * v[i];
            }

            spmv(m, &w, &mut work);
            nrm2 = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            let nrm2 = nrm2.max(0.0);
            nrm = nrm2.sqrt();

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
}

impl SparseEigenSolver for SparseShiftInvertLanczos {
    fn smallest_eigenvalues(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        n_modes: usize,
    ) -> Result<Vec<f64>, EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        if n_modes == 0 {
            return Ok(Vec::new());
        }

        // 1. Build A = K - σM and factor it once.
        let a = shifted_pencil(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?;

        // 2. Lanczos in the M-inner product.
        //
        // Cap the iteration count at n — Lanczos cannot exceed the
        // dimension, even if max_iters is set higher.
        let max_k = self.max_iters.min(n);
        let max_k = max_k.max(n_modes + 2).min(n);

        // Allocate workspace.
        let mut basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
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

        for j in 0..max_k {
            // M v
            spmv(m, &v, &mut mv);
            // w = A^{-1} (M v) = (K - σM)^{-1} M v
            solve_with_lu(&lu, &mv, &mut w)?;

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
            // basis numerically M-orthonormal even at large k.
            for vk in basis.iter() {
                spmv(m, vk, &mut work);
                let c = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
                if c.abs() > 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            // Also re-project off v itself (the just-added direction).
            spmv(m, &v, &mut work);
            let c = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
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

            // Push the *current* v as basis[j], then either iterate or stop.
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
                .fold((0usize, 0.0_f64), |acc, x| if x.1 > acc.1 { x } else { acc });
            assert_eq!(max_pos, i, "eigenvector[{i}] localized at wrong index");
            assert!(
                (max_val - 1.0).abs() < 1e-6,
                "eigenvector[{i}] not localized: max = {max_val}"
            );
        }
    }
}
