//! **Krylov iterative solvers** for the complex-symmetric driven system
//! `A(ω) x = b` (issue #238).
//!
//! All driven and eigen solves currently exit to a direct sparse LU
//! ([`faer::sparse::linalg::solvers::Lu`]) at the solve boundary
//! ([`crate::driven::FactoredDrivenOperator`],
//! [`crate::eigen::complex`]). Direct factorization caps the problem
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
//!   Lanczos in [`crate::eigen::complex`].
//! - A [`Preconditioner`] trait with four concrete preconditioners:
//!   [`IdentityPreconditioner`] (the no-op), [`JacobiPreconditioner`]
//!   (diagonal scaling — `M = diag(A)`, the simplest and cheapest
//!   left-preconditioner), [`IluPreconditioner`] (incomplete-LU
//!   factorization on `A`'s sparsity pattern — heavier setup, much
//!   lower iteration counts on ill-conditioned operators; see
//!   issue #267), and [`ChebyshevPreconditioner`] (a fixed-degree
//!   first-kind Chebyshev polynomial smoother on the Jacobi-scaled
//!   operator — matrix-free-friendly, only SpMV + AXPY, no triangular
//!   factor; see issue #299).
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

use crate::eigen::complex::spmv;

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
// IluPreconditioner — incomplete LU(0) on the sparsity pattern of A
// ---------------------------------------------------------------------------

/// **Incomplete-LU(0) preconditioner** for the complex-symmetric driven
/// pencil (issue #267).
///
/// Classical ILU(0): factor `A ≈ L · U` where the unit-lower-triangular
/// `L` and upper-triangular `U` are constrained to the **original
/// sparsity pattern of `A`**. Fill-in beyond that pattern is dropped
/// (this is the "level 0" in "ILU(k)"). The factorization stores both
/// triangles in a single CSC buffer of the same shape as `A` — `L` in
/// the strict lower triangle, `U` (with diagonal) in the upper triangle.
///
/// # Math note (complex-symmetric / COCG compatibility)
///
/// COCG uses the **bilinear** form `r^T z` — no conjugation. The
/// preconditioner application `z = M⁻¹ r = U⁻¹ L⁻¹ r` uses only standard
/// complex `*` / `/` arithmetic (the `c64` operators), so the
/// factorization is consistent with the bilinear form: `A` is treated
/// as a complex matrix with `A^T = A`, the LU is the bilinear LU, and
/// `M⁻¹` composes cleanly with the COCG inner product. The
/// factorization does **not** conjugate anywhere — the same
/// invariant the bilinear-form Lanczos in [`crate::eigen::complex`]
/// relies on.
///
/// # Setup vs application cost trade-off
///
/// ILU has a one-shot **factorization cost** (one pass over the
/// sparsity pattern of `A`, sub-cubic but heavier than Jacobi's `O(n)`
/// diagonal copy), amortized across many Krylov iterations. The
/// per-application cost is two triangular solves on the same sparsity
/// (each `O(nnz)`), versus Jacobi's `O(n)` element-wise multiply.
/// ILU pays off whenever it shaves enough Krylov iterations to recoup
/// its setup; for moderately ill-conditioned operators (high-Q
/// resonant cavities, surface-impedance-loaded structures, fine
/// meshes near cutoff) this break-even is comfortable.
///
/// # Errors at construction
///
/// Returns [`KspError::Breakdown`] (with `kind = "ILU diag"`) if a
/// diagonal pivot is zero or non-finite during factorization — this
/// happens when `A` is singular or extremely ill-conditioned at the
/// drop-pattern level. Fall back to Jacobi or direct LU in that case.
///
/// # Fill level
///
/// Constructed via [`IluPreconditioner::new`] which is ILU(0) — no
/// fill beyond `A`'s pattern. Higher fill levels (`ILU(k)` with
/// `k ≥ 1`) are deliberately out of scope for issue #267; the
/// constructor's signature reserves the `fill_level` parameter for a
/// future extension but currently asserts `fill_level == 0`.
#[derive(Debug, Clone)]
pub struct IluPreconditioner {
    /// System size.
    n: usize,
    /// Factor values: strict-lower-triangle entries hold `L_{ij}` with
    /// `i > j`; the diagonal and strict-upper entries hold the `U`
    /// values (so `U` has the explicit diagonal). The CSC layout is
    /// implicit via [`Self::row_entries`] — we don't keep the original
    /// `col_ptr` / `row_idx` because both the forward and the backward
    /// triangular solves run off the row-indexed view.
    vals: Vec<c64>,
    /// For each column `j`, position in `vals` of the diagonal entry
    /// `U_{jj}` (used for the per-column pivot and the back-solve
    /// scaling).
    diag_pos: Vec<usize>,
    /// For each row `i`, the **sorted** list of `(col_j, csc_pos)`
    /// pairs of entries in row `i`. Built once at construction; used
    /// both during factorization (to look up `L_{ik}` for `k < j` on
    /// the column-`j` update) and during the triangular solves.
    row_entries: Vec<Vec<(usize, usize)>>,
}

impl IluPreconditioner {
    /// Build the ILU(0) factorization of `a`. `a` must be a square
    /// CSC matrix; the factorization shares its sparsity pattern.
    ///
    /// # Arguments
    ///
    /// - `a` — the matrix to factor (the assembled driven operator
    ///   `A(ω)`).
    /// - `fill_level` — must be `0` (ILU(0)). Higher fill levels are
    ///   reserved for a future extension and currently rejected with
    ///   a panic; see [`IluPreconditioner`] docs.
    ///
    /// # Errors
    ///
    /// [`KspError::Breakdown`] if a pivot vanishes during factorization
    /// (the matrix is singular or ill-conditioned at the ILU(0)-drop
    /// pattern). The construction never silently returns NaNs.
    ///
    /// # Panics
    ///
    /// Panics if `a` is non-square, if `fill_level != 0`, or if a column
    /// of `a` does not have an explicit diagonal entry (ILU(0) requires
    /// the diagonal to be in the sparsity pattern — assembled FEM
    /// operators always have it).
    pub fn new(a: SparseColMatRef<'_, usize, c64>, fill_level: usize) -> Result<Self, KspError> {
        let n = a.nrows();
        assert_eq!(a.ncols(), n, "IluPreconditioner requires square A");
        assert_eq!(
            fill_level, 0,
            "IluPreconditioner currently supports ILU(0) only (issue #267 scope rails)"
        );

        let a_col_ptr = a.col_ptr();
        let a_row_idx = a.row_idx();
        let a_val = a.val();

        // Copy A's values into the factorization buffer. The CSC
        // pattern stays identical with A's; the values are overwritten
        // in place by the ILU(0) elimination.
        let mut vals: Vec<c64> = a_val.to_vec();

        // Per-column diagonal position. Required: every column has
        // an explicit diagonal entry.
        let mut diag_pos = vec![usize::MAX; n];
        for (j, diag_slot) in diag_pos.iter_mut().enumerate() {
            let start = a_col_ptr[j];
            let end = a_col_ptr[j + 1];
            for (offset, &row) in a_row_idx[start..end].iter().enumerate() {
                if row == j {
                    *diag_slot = start + offset;
                    break;
                }
            }
            assert!(
                *diag_slot != usize::MAX,
                "IluPreconditioner: column {j} has no explicit diagonal entry"
            );
        }

        // Build per-row entry index: for each row i, the sorted list
        // of (col_j, csc_pos) pairs. CSC stores columns sorted by row
        // already, so we walk columns left-to-right and append; the
        // result is sorted by column for each row.
        let mut row_entries: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for j in 0..n {
            let start = a_col_ptr[j];
            let end = a_col_ptr[j + 1];
            for (offset, &i) in a_row_idx[start..end].iter().enumerate() {
                row_entries[i].push((j, start + offset));
            }
        }
        // row_entries[i] is already sorted by column because we walked
        // j in increasing order.

        // ILU(0) elimination. The "IKJ" form (Saad §10.3.2):
        //
        // for i = 1..n:
        //     for k = 0..i with (i,k) in pattern:
        //         a_ik /= a_kk
        //         for j = k+1..n with (i,j) in pattern AND (k,j) in pattern:
        //             a_ij -= a_ik * a_kj
        //
        // The "AND (k,j) in pattern" clause is the ILU(0) drop rule —
        // any contribution that would land outside A's original
        // pattern is discarded.
        //
        // We index by row using `row_entries[i]` (the row-wise list of
        // CSC positions). For each row `i`, we walk through its
        // entries in column-sorted order, accumulating L pivots and
        // applying drops as we go.
        for i in 1..n {
            // Snapshot row i's entries — we'll walk through them in
            // column-sorted order.
            let row_i = row_entries[i].clone();

            for (idx, &(k, pos_ik)) in row_i.iter().enumerate() {
                if k >= i {
                    break; // We've reached the diagonal or upper — stop.
                }

                // Divide a_ik by the pivot U_kk to form L_ik.
                let u_kk = vals[diag_pos[k]];
                let mag2 = u_kk.re * u_kk.re + u_kk.im * u_kk.im;
                if !u_kk.re.is_finite() || !u_kk.im.is_finite() || mag2 == 0.0 {
                    return Err(KspError::Breakdown {
                        iter: 0,
                        kind: "ILU diag",
                        value_re: u_kk.re,
                        value_im: u_kk.im,
                    });
                }
                let l_ik = vals[pos_ik] / u_kk;
                vals[pos_ik] = l_ik;

                // Update a_ij for j > k where both (i,j) and (k,j) are
                // in the pattern.
                //
                // (i,j) entries: walk forward in row_i starting at
                // idx+1 (still column-sorted).
                //
                // (k,j) entries: walk forward in row_entries[k] from
                // the first entry with col >= k+1 (the first entry
                // after L_kk).

                let row_k = &row_entries[k];
                // Find the start in row_k where col > k.
                let mut p_k = match row_k.binary_search_by_key(&k, |&(c, _)| c) {
                    Ok(found) => found + 1,
                    Err(insert) => insert,
                };
                let mut p_i = idx + 1;

                while p_i < row_i.len() && p_k < row_k.len() {
                    let (j_i, pos_ij) = row_i[p_i];
                    let (j_k, pos_kj) = row_k[p_k];
                    match j_i.cmp(&j_k) {
                        std::cmp::Ordering::Equal => {
                            // Both (i, j) and (k, j) are in the
                            // pattern — apply the ILU(0) drop-free
                            // update.
                            let kj = vals[pos_kj];
                            vals[pos_ij] -= l_ik * kj;
                            p_i += 1;
                            p_k += 1;
                        }
                        std::cmp::Ordering::Less => {
                            // (i, j_i) has no matching (k, j_i) entry
                            // — drop the implied fill at level 0.
                            p_i += 1;
                        }
                        std::cmp::Ordering::Greater => {
                            // (k, j_k) has no matching (i, j_k) entry
                            // — same drop.
                            p_k += 1;
                        }
                    }
                }
            }

            // Final pivot check on the diagonal (caught now rather than
            // on the next column's solve).
            let u_ii = vals[diag_pos[i]];
            let mag2 = u_ii.re * u_ii.re + u_ii.im * u_ii.im;
            if !u_ii.re.is_finite() || !u_ii.im.is_finite() || mag2 == 0.0 {
                return Err(KspError::Breakdown {
                    iter: 0,
                    kind: "ILU diag",
                    value_re: u_ii.re,
                    value_im: u_ii.im,
                });
            }
        }
        // Also check the very first column's pivot (the loop above
        // skips i=0 since there's no row 0 lower-triangular update;
        // a singular A[0,0] would otherwise slip through and explode
        // only at apply time).
        {
            let u_00 = vals[diag_pos[0]];
            let mag2 = u_00.re * u_00.re + u_00.im * u_00.im;
            if !u_00.re.is_finite() || !u_00.im.is_finite() || mag2 == 0.0 {
                return Err(KspError::Breakdown {
                    iter: 0,
                    kind: "ILU diag",
                    value_re: u_00.re,
                    value_im: u_00.im,
                });
            }
        }

        Ok(Self {
            n,
            vals,
            diag_pos,
            row_entries,
        })
    }
}

impl Preconditioner for IluPreconditioner {
    /// Apply `z = M⁻¹ r = U⁻¹ L⁻¹ r` via two triangular solves.
    ///
    /// `L` is unit-lower-triangular (the strict-lower part of `vals`,
    /// implicit unit diagonal). `U` is upper-triangular with explicit
    /// diagonal at `diag_pos[j]`. Both solves use standard complex
    /// arithmetic — no conjugation — keeping the application
    /// consistent with the COCG bilinear inner product.
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        debug_assert_eq!(r.len(), self.n);
        debug_assert_eq!(z.len(), self.n);

        // Forward solve: L y = r. We write y into z (in place).
        //
        // For i = 0..n: y_i = r_i - Σ_{k<i, (i,k) in pattern} L_ik · y_k.
        // The "L" entries are the row-i entries with column < i (strict
        // lower triangle); `row_entries[i]` lists them in sorted order.
        for i in 0..self.n {
            let mut acc = r[i];
            for &(k, pos_ik) in &self.row_entries[i] {
                if k >= i {
                    break;
                }
                acc -= self.vals[pos_ik] * z[k];
            }
            z[i] = acc;
        }

        // Backward solve: U z = y (y was overwritten into z above).
        //
        // For i = n-1..=0:
        //   z_i = (y_i - Σ_{j>i, (i,j) in pattern} U_ij · z_j) / U_ii.
        // The "U" entries are the row-i entries with column ≥ i. The
        // diagonal entry is U_ii (at diag_pos[i]); strict-upper entries
        // are at columns > i in row_entries[i].
        for i in (0..self.n).rev() {
            let mut acc = z[i];
            for &(j, pos_ij) in &self.row_entries[i] {
                if j > i {
                    acc -= self.vals[pos_ij] * z[j];
                }
            }
            z[i] = acc / self.vals[self.diag_pos[i]];
        }
    }

    fn dim(&self) -> usize {
        self.n
    }
}

// ---------------------------------------------------------------------------
// ChebyshevPreconditioner — fixed-degree first-kind polynomial smoother
// ---------------------------------------------------------------------------

/// Which Chebyshev polynomial family the smoother uses.
///
/// Both kinds are implemented. The **first kind** (issue #299) is the
/// classic Saad §12.3 iteration over `[λ_min, λ_max]`. The **fourth
/// kind** (Lottes 2022, the form Palace/MFEM expose) targets the upper
/// spectrum using only `λ_max` (issue #348).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChebyshevKind {
    /// First-kind Chebyshev iteration (Saad §12.3) — the v1 default.
    /// Uses the spectral interval `[λ_min, λ_max]` (hence the `ratio`
    /// heuristic for `λ_min`).
    First,
    /// Fourth-kind Chebyshev smoother (Lottes 2022, *Optimal polynomial
    /// smoothers for multigrid*) — the **unweighted** variant
    /// (Lottes Algorithm 3 / MFEM `OperatorChebyshevSmoother`). Needs
    /// only `λ_max` (no `λ_min`/`ratio`); the smoothing window is
    /// `[0, λ_max]`, which is exactly the high-frequency band a
    /// multigrid smoother is meant to damp.
    Fourth,
}

/// **Chebyshev polynomial-smoother preconditioner** for the
/// complex-symmetric driven pencil (issue #299).
///
/// This realizes the firm whiteroom `chebyshev` L4 surface. Given the
/// residual `r`, the preconditioner returns `z = p_k(Â) D⁻¹ r ≈ A⁻¹ r`
/// where `Â = D⁻¹ A` is the Jacobi-scaled operator, `D = diag(A)`, and
/// `p_k` is the degree-`k` first-kind Chebyshev iteration polynomial
/// over the spectral interval `[λ_min, λ_max]` of `Â`. Unlike ILU(0)
/// it needs **no triangular factor** — each apply is `k` SpMVs plus
/// AXPYs, which is trivially parallel and matrix-free-friendly. This is
/// the standard smoother in Palace's geometric-multigrid preconditioner.
///
/// # Which variant
///
/// The default is the **first-kind** Chebyshev iteration
/// ([`ChebyshevKind::First`], `[λ_min, λ_max]`). The **fourth-kind**
/// smoother ([`ChebyshevKind::Fourth`], Lottes 2022) is also available;
/// it needs only `λ_max` (the `ratio`/`λ_min` heuristic is unused) and
/// smooths over `[0, λ_max]`. See [`ChebyshevKind`] for the trade-off.
///
/// # Algorithm — first-kind Chebyshev iteration
///
/// We approximately solve `Â ẑ = r̂` (with `r̂ = D⁻¹ r`) by the
/// three-term first-kind Chebyshev recurrence (Saad, *Iterative
/// Methods*, §12.3.2; Gutknecht & Röllin 2002). With
/// `θ = (λ_max + λ_min)/2`, `δ = (λ_max − λ_min)/2`, `σ₁ = θ/δ`, and a
/// zero initial guess `ẑ₀ = 0` (so the initial residual is `r̂`):
///
/// ```text
/// ρ₀ = 1/σ₁
/// d  = (1/θ) r̂                 (first correction)
/// ẑ  = d
/// s  = r̂ − Â d                 (residual after step 0)
/// for j = 1 .. k-1:
///     ρ_j = 1 / (2σ₁ − ρ_{j-1})
///     d   = ρ_j ρ_{j-1} d + (2 ρ_j / δ) s
///     ẑ  += d
///     s  -= Â d
/// ```
///
/// `z = ẑ`. A degree of `k = 0` returns `z = D⁻¹ r` exactly — i.e. the
/// smoother degenerates to **Jacobi** (no polynomial steps), which the
/// unit tests assert.
///
/// # Algorithm — fourth-kind Chebyshev smoother (Lottes 2022)
///
/// The **unweighted** fourth-kind smoother (Lottes 2022, Algorithm 3;
/// the form MFEM's `OperatorChebyshevSmoother` and Palace expose) needs
/// only `λ_max` — there is no `λ_min`, so the `ratio` knob is ignored.
/// It smooths over `[0, λ_max]`, the high-frequency band. With
/// `r̂ = D⁻¹ r` (the residual of the scaled system at the zero initial
/// guess `ẑ₀ = 0`), `s ← r̂`, `d ← 0`, `ẑ ← 0`:
///
/// ```text
/// for i = 1 .. k:
///     β_i  = (2i − 3) / (2i + 1)        (0 at i = 1, since d = 0)
///     γ_i  = (8i − 4) / ((2i + 1) λ_max)
///     d    = β_i d + γ_i s
///     ẑ   += d
///     s   -= Â d
/// ```
///
/// `z = ẑ`. As with the first-kind path, `k = 0` returns `z = D⁻¹ r`
/// (Jacobi). The coefficients are the standard unweighted fourth-kind
/// recurrence; the optional optimized/weighted "Opt4" variant
/// (Lottes Table 1) is *not* implemented here — the unweighted form is
/// the documented choice.
///
/// # Eigenvalue-bound estimation and its cost
///
/// `λ_max(Â)` is estimated by a few steps of **power iteration** on the
/// diagonally-scaled operator `Â = D⁻¹ A`, using the magnitude of the
/// Rayleigh-like quotient `|vᵀ Â v| / |vᵀ v|` (bilinear, no
/// conjugation — see the math note below). The estimate is then padded
/// by a small safety factor (`1.1×`) so the true spectral radius stays
/// inside the Chebyshev interval. `λ_min` is set to `λ_max / ratio`
/// with a configurable `ratio` (smoother default `≈ 30`, targeting the
/// upper part of the spectrum — the high-frequency error a smoother is
/// meant to damp). The cost is `O(n_power · nnz)` one-shot at
/// construction (default `n_power = 10`), amortized over every apply —
/// the same SpMV kernel the iteration itself uses.
///
/// # Math note (complex-symmetric / COCG compatibility)
///
/// COCG uses the **bilinear** form `r^T z` — no conjugation. Every
/// inner operation here uses standard complex `*` / `/` / `+` `c64`
/// arithmetic: the diagonal scaling `D⁻¹`, the SpMV `Â v`, the AXPYs,
/// and the power-iteration Rayleigh quotient `vᵀ Â v` (bilinear, **not**
/// `v^H Â v`). The smoother polynomial `p_k(Â)` is therefore a complex
/// polynomial in the complex-symmetric operator and composes cleanly
/// with the COCG inner product — the same "no conjugation" invariant
/// [`IluPreconditioner`] and the bilinear-form Lanczos in
/// [`crate::eigen::complex`] rely on. The eigenvalue *interval*
/// `[λ_min, λ_max]` is taken on the real axis from the magnitudes of
/// the estimate; for the lossy/PML pencils we target (close to real
/// SPD with a small absorbing imaginary part) the spectrum hugs the
/// positive real axis, so a real interval is the right smoothing window.
#[derive(Debug, Clone)]
pub struct ChebyshevPreconditioner {
    /// System size.
    n: usize,
    /// Which Chebyshev family the [`apply`](Preconditioner::apply) path
    /// runs (first- or fourth-kind).
    kind: ChebyshevKind,
    /// Polynomial degree `k` (number of Chebyshev steps). `0` ⇒ Jacobi.
    degree: usize,
    /// Reciprocal of `diag(A)` — the Jacobi scaling `D⁻¹`.
    inv_diag: Vec<c64>,
    /// CSC column pointers of `A` (owned copy for the apply-time SpMV).
    col_ptr: Vec<usize>,
    /// CSC row indices of `A`.
    row_idx: Vec<usize>,
    /// CSC values of `A`.
    val: Vec<c64>,
    /// Estimated spectral lower bound `λ_min` of `Â = D⁻¹ A`.
    lambda_min: f64,
    /// Estimated spectral upper bound `λ_max` of `Â = D⁻¹ A`.
    lambda_max: f64,
}

/// Tuning knobs for [`ChebyshevPreconditioner`]. Built via
/// [`ChebyshevConfig::default`] (a sensible smoother default) or by
/// setting the fields directly.
#[derive(Debug, Clone, Copy)]
pub struct ChebyshevConfig {
    /// Which Chebyshev family ([`ChebyshevKind::First`] or
    /// [`ChebyshevKind::Fourth`]). The fourth-kind path ignores `ratio`
    /// (it uses only `λ_max`).
    pub kind: ChebyshevKind,
    /// Ratio `λ_max / λ_min` defining the smoothing interval. Larger ⇒
    /// the interval reaches further toward the origin (more of the
    /// spectrum smoothed) at the cost of weaker high-frequency damping.
    /// Smoother default `≈ 30`.
    pub ratio: f64,
    /// Number of power-iteration steps for the `λ_max` estimate.
    pub power_iters: usize,
    /// Safety factor applied to the `λ_max` estimate so the true
    /// spectral radius stays inside the Chebyshev interval. `≥ 1`.
    pub safety_factor: f64,
}

impl Default for ChebyshevConfig {
    fn default() -> Self {
        Self {
            kind: ChebyshevKind::First,
            ratio: 30.0,
            power_iters: 10,
            safety_factor: 1.1,
        }
    }
}

impl ChebyshevPreconditioner {
    /// Build a degree-`degree` first-kind Chebyshev smoother for the
    /// complex sparse CSC matrix `a`, using the default
    /// [`ChebyshevConfig`] (first-kind, `ratio ≈ 30`, 10 power-iteration
    /// steps, `1.1×` safety padding). For the fourth-kind smoother
    /// (Lottes 2022) set [`ChebyshevConfig::kind`] and use
    /// [`with_config`](Self::with_config). The matrix must be square.
    ///
    /// `degree = 0` is the Jacobi degenerate case (`apply` returns
    /// `D⁻¹ r`); it skips the power-iteration estimate entirely.
    ///
    /// # Errors
    ///
    /// [`KspError::Breakdown`] (with `kind = "diag(A)"`) if any diagonal
    /// entry of `a` is zero or non-finite — the Jacobi scaling the
    /// smoother is built on is undefined in that case.
    pub fn new(a: SparseColMatRef<'_, usize, c64>, degree: usize) -> Result<Self, KspError> {
        Self::with_config(a, degree, ChebyshevConfig::default())
    }

    /// Build the smoother with an explicit [`ChebyshevConfig`].
    ///
    /// # Errors
    ///
    /// [`KspError::Breakdown`] if any diagonal entry of `a` is zero or
    /// non-finite (`kind = "diag(A)"`), mirroring
    /// [`JacobiPreconditioner::new`].
    ///
    /// # Panics
    ///
    /// Panics if `a` is non-square, or if `config.ratio <= 1.0` (the
    /// first-kind interval would be degenerate). The `ratio` check is
    /// enforced for both kinds for a uniform contract even though the
    /// fourth-kind path does not use `λ_min`.
    pub fn with_config(
        a: SparseColMatRef<'_, usize, c64>,
        degree: usize,
        config: ChebyshevConfig,
    ) -> Result<Self, KspError> {
        let n = a.nrows();
        assert_eq!(a.ncols(), n, "ChebyshevPreconditioner requires square A");
        assert!(
            config.ratio > 1.0,
            "ChebyshevPreconditioner: ratio must be > 1 (got {})",
            config.ratio
        );
        assert!(
            config.safety_factor >= 1.0,
            "ChebyshevPreconditioner: safety_factor must be ≥ 1 (got {})",
            config.safety_factor
        );

        // Own the CSC arrays so `apply` can run SpMV without holding a
        // borrow of the original matrix.
        let col_ptr = a.col_ptr().to_vec();
        let row_idx = a.row_idx().to_vec();
        let val = a.val().to_vec();

        // Extract diag(A) and cache its reciprocal — same construction
        // as JacobiPreconditioner.
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
            inv_diag[j] = c64::new(d.re / mag2, -d.im / mag2);
        }

        // Eigenvalue-bound estimation. Degree 0 reduces to Jacobi and
        // never touches the polynomial path, so the (cheap-but-nonzero)
        // power iteration is skipped.
        let (lambda_min, lambda_max) = if degree == 0 {
            (1.0, 1.0)
        } else {
            let lambda_max =
                estimate_lambda_max(&col_ptr, &row_idx, &val, &inv_diag, config.power_iters)
                    * config.safety_factor;
            let lambda_min = lambda_max / config.ratio;
            (lambda_min, lambda_max)
        };

        Ok(Self {
            n,
            kind: config.kind,
            degree,
            inv_diag,
            col_ptr,
            row_idx,
            val,
            lambda_min,
            lambda_max,
        })
    }

    /// The estimated spectral interval `[λ_min, λ_max]` of the
    /// Jacobi-scaled operator `Â = D⁻¹ A`. Exposed for diagnostics and
    /// tests; for `degree = 0` both bounds are `1.0` (unused).
    pub fn spectral_interval(&self) -> (f64, f64) {
        (self.lambda_min, self.lambda_max)
    }

    /// Apply the Jacobi-scaled operator: `out = D⁻¹ A v`. Uses only
    /// standard complex arithmetic (no conjugation), consistent with
    /// the COCG bilinear form.
    fn apply_scaled_operator(&self, v: &[c64], out: &mut [c64]) {
        // out = A v
        for o in out.iter_mut() {
            *o = c64::new(0.0, 0.0);
        }
        for (j, &xj) in v.iter().enumerate() {
            if xj.re == 0.0 && xj.im == 0.0 {
                continue;
            }
            for k in self.col_ptr[j]..self.col_ptr[j + 1] {
                let i = self.row_idx[k];
                out[i] += self.val[k] * xj;
            }
        }
        // out = D⁻¹ (A v)
        for (o, d) in out.iter_mut().zip(self.inv_diag.iter()) {
            *o *= *d;
        }
    }
}

/// Power-iteration estimate of `λ_max(D⁻¹ A)` using the **bilinear**
/// Rayleigh quotient `|vᵀ Â v| / |vᵀ v|` (no conjugation — consistent
/// with the COCG complex-symmetric form). Returns a strictly positive
/// estimate; degenerate inputs fall back to `1.0`.
fn estimate_lambda_max(
    col_ptr: &[usize],
    row_idx: &[usize],
    val: &[c64],
    inv_diag: &[c64],
    iters: usize,
) -> f64 {
    let n = inv_diag.len();
    if n == 0 {
        return 1.0;
    }
    // Deterministic, non-degenerate start vector.
    let mut v: Vec<c64> = (0..n)
        .map(|i| c64::new(1.0 + (i as f64) * 1e-3, 0.0))
        .collect();
    let mut av = vec![c64::new(0.0, 0.0); n];

    let scaled_spmv = |v: &[c64], out: &mut [c64]| {
        for o in out.iter_mut() {
            *o = c64::new(0.0, 0.0);
        }
        for (j, &xj) in v.iter().enumerate() {
            if xj.re == 0.0 && xj.im == 0.0 {
                continue;
            }
            for k in col_ptr[j]..col_ptr[j + 1] {
                let i = row_idx[k];
                out[i] += val[k] * xj;
            }
        }
        for (o, d) in out.iter_mut().zip(inv_diag.iter()) {
            *o *= *d;
        }
    };

    let mut lambda = 1.0_f64;
    for _ in 0..iters.max(1) {
        scaled_spmv(&v, &mut av);
        // Bilinear Rayleigh quotient λ ≈ |vᵀ Â v| / |vᵀ v|.
        let mut num = c64::new(0.0, 0.0);
        let mut den = c64::new(0.0, 0.0);
        for (&vi, &avi) in v.iter().zip(av.iter()) {
            num += vi * avi;
            den += vi * vi;
        }
        let den_mag = (den.re * den.re + den.im * den.im).sqrt();
        if den_mag > 0.0 {
            let num_mag = (num.re * num.re + num.im * num.im).sqrt();
            let q = num_mag / den_mag;
            if q.is_finite() && q > 0.0 {
                lambda = q;
            }
        }
        // Normalize av by its Euclidean norm to form the next iterate.
        let nrm = euclid_norm(&av);
        if !nrm.is_finite() || nrm == 0.0 {
            break;
        }
        let inv = 1.0 / nrm;
        for (vi, &avi) in v.iter_mut().zip(av.iter()) {
            *vi = avi * c64::new(inv, 0.0);
        }
    }

    if lambda.is_finite() && lambda > 0.0 {
        lambda
    } else {
        1.0
    }
}

impl ChebyshevPreconditioner {
    /// First-kind three-term recurrence (Saad §12.3.2) over the interval
    /// `[λ_min, λ_max]`. `s` enters as the scaled residual `r̂ = D⁻¹ r`
    /// and `z` holds the accumulating solution. All arithmetic is
    /// standard complex (no conjugation).
    fn apply_first_kind(&self, s: &mut [c64], z: &mut [c64]) {
        let theta = 0.5 * (self.lambda_max + self.lambda_min);
        let delta = 0.5 * (self.lambda_max - self.lambda_min);
        let sigma1 = theta / delta;

        // First step (j = 0): d = (1/θ) r̂, ẑ = d, residual s -= Â d.
        let inv_theta = c64::new(1.0 / theta, 0.0);
        let mut d = vec![c64::new(0.0, 0.0); self.n];
        for ((di, zi), &si) in d.iter_mut().zip(z.iter_mut()).zip(s.iter()) {
            *di = si * inv_theta;
            *zi = *di;
        }
        let mut ad = vec![c64::new(0.0, 0.0); self.n];
        self.apply_scaled_operator(&d, &mut ad);
        for (si, &adi) in s.iter_mut().zip(ad.iter()) {
            *si -= adi;
        }

        // Three-term recurrence for j = 1 .. degree-1.
        let mut rho_prev = 1.0 / sigma1;
        for _ in 1..self.degree {
            let rho = 1.0 / (2.0 * sigma1 - rho_prev);
            let c_d = c64::new(rho * rho_prev, 0.0);
            let c_s = c64::new(2.0 * rho / delta, 0.0);
            for ((di, zi), &si) in d.iter_mut().zip(z.iter_mut()).zip(s.iter()) {
                *di = c_d * *di + c_s * si;
                *zi += *di;
            }
            self.apply_scaled_operator(&d, &mut ad);
            for (si, &adi) in s.iter_mut().zip(ad.iter()) {
                *si -= adi;
            }
            rho_prev = rho;
        }
    }

    /// Unweighted fourth-kind Chebyshev smoother (Lottes 2022,
    /// Algorithm 3 / MFEM `OperatorChebyshevSmoother`) over `[0, λ_max]`.
    /// `s` enters as the scaled residual `r̂ = D⁻¹ r` and `z` holds the
    /// accumulating solution. Uses only `λ_max` (no `λ_min`/`ratio`).
    /// All arithmetic is standard complex (no conjugation).
    fn apply_fourth_kind(&self, s: &mut [c64], z: &mut [c64]) {
        let lambda_max = self.lambda_max;
        let mut d = vec![c64::new(0.0, 0.0); self.n];
        let mut ad = vec![c64::new(0.0, 0.0); self.n];

        // i = 1 .. degree:
        //   d  = β_i d + γ_i s,  β_i = (2i−3)/(2i+1), γ_i = (8i−4)/((2i+1)λ_max)
        //   z += d
        //   s -= Â d
        for i in 1..=self.degree {
            let fi = i as f64;
            let beta = (2.0 * fi - 3.0) / (2.0 * fi + 1.0);
            let gamma = (8.0 * fi - 4.0) / ((2.0 * fi + 1.0) * lambda_max);
            let c_beta = c64::new(beta, 0.0);
            let c_gamma = c64::new(gamma, 0.0);
            for ((di, zi), &si) in d.iter_mut().zip(z.iter_mut()).zip(s.iter()) {
                *di = c_beta * *di + c_gamma * si;
                *zi += *di;
            }
            self.apply_scaled_operator(&d, &mut ad);
            for (si, &adi) in s.iter_mut().zip(ad.iter()) {
                *si -= adi;
            }
        }
    }
}

impl Preconditioner for ChebyshevPreconditioner {
    /// Apply the fixed-degree Chebyshev smoother:
    /// `z = p_k(Â) D⁻¹ r ≈ A⁻¹ r`. Degree `0` returns `z = D⁻¹ r`
    /// (Jacobi) for either kind. Dispatches to the first- or fourth-kind
    /// recurrence per [`ChebyshevKind`]. All arithmetic is standard
    /// complex (no conjugation), preserving the COCG bilinear inner
    /// product.
    fn apply(&self, r: &[c64], z: &mut [c64]) {
        debug_assert_eq!(r.len(), self.n);
        debug_assert_eq!(z.len(), self.n);

        // r̂ = D⁻¹ r — the Jacobi-scaled right-hand side / initial
        // residual (zero initial guess ẑ₀ = 0).
        let mut s = vec![c64::new(0.0, 0.0); self.n];
        for ((si, &ri), &di) in s.iter_mut().zip(r.iter()).zip(self.inv_diag.iter()) {
            *si = ri * di;
        }

        // Degree 0 ⇒ Jacobi: z = D⁻¹ r (both kinds).
        if self.degree == 0 {
            z.copy_from_slice(&s);
            return;
        }

        // z starts at the zero solution; the recurrences accumulate into it.
        for zi in z.iter_mut() {
            *zi = c64::new(0.0, 0.0);
        }

        match self.kind {
            ChebyshevKind::First => self.apply_first_kind(&mut s, z),
            ChebyshevKind::Fourth => self.apply_fourth_kind(&mut s, z),
        }
    }

    fn dim(&self) -> usize {
        self.n
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
/// [`crate::eigen::complex`] uses.
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
/// [`crate::eigen::complex`] uses for the Lanczos pencil.
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

    // -----------------------------------------------------------------
    // ILU(0) preconditioner — unit tests (issue #267)
    // -----------------------------------------------------------------

    /// ILU(0) on a **tridiagonal** matrix is exact LU (no fill outside
    /// the tridiagonal pattern is needed), so the preconditioned COCG
    /// must converge in **one iteration**.
    #[test]
    fn ilu0_on_tridiagonal_is_exact_lu() {
        // 4×4 real-symmetric tridiagonal SPD:
        // [[4, 1, 0, 0],
        //  [1, 4, 1, 0],
        //  [0, 1, 4, 1],
        //  [0, 0, 1, 4]]
        let n = 4;
        let mut trips = Vec::new();
        for i in 0..n {
            trips.push(Triplet::new(i, i, c64::new(4.0, 0.0)));
            if i + 1 < n {
                trips.push(Triplet::new(i, i + 1, c64::new(1.0, 0.0)));
                trips.push(Triplet::new(i + 1, i, c64::new(1.0, 0.0)));
            }
        }
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = (0..n).map(|i| c64::new(i as f64 + 1.0, 0.0)).collect();
        let mut x = vec![c64::new(0.0, 0.0); n];

        let pc = IluPreconditioner::new(a.as_ref(), 0).expect("ILU(0) factorization");
        // Tridiagonal: no fill exists outside the pattern, so ILU(0)
        // is an exact LU — preconditioned COCG terminates in one
        // iteration with residual ≈ 0.
        let cocg = Cocg::new(1e-12, 50);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges with ILU(0)");
        assert!(report.converged);
        assert!(
            report.iters <= 2,
            "tridiagonal ILU(0) should solve in ≤ 2 iters (no fill possible), got {}",
            report.iters
        );

        // Cross-check the solution by multiplying back.
        let mut ax = vec![c64::new(0.0, 0.0); n];
        crate::eigen::complex::spmv(a.as_ref(), &x, &mut ax);
        for i in 0..n {
            assert!((ax[i] - b[i]).norm() < 1e-10);
        }
    }

    /// ILU(0) on a **complex-symmetric lossy** matrix must converge
    /// COCG to round-off, exercising the "no conjugation" math note
    /// from issue #267.
    #[test]
    fn ilu0_solves_complex_symmetric_lossy() {
        // Same fixture as `cocg_solves_complex_symmetric_lossy_system`.
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
        let pc = IluPreconditioner::new(a.as_ref(), 0).expect("ILU(0) factorization");
        let cocg = Cocg::new(1e-12, 100);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges");
        assert!(report.converged);
        assert!(report.residual_rel < 1e-10, "{:?}", report);
    }

    /// ILU(0) factorization must reject a zero-pivot matrix instead of
    /// silently producing NaNs.
    #[test]
    fn ilu0_rejects_zero_pivot() {
        // Diagonal entry of column 0 is zero (Triplet zero is summed in
        // — try_new_from_triplets accepts it).
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(0.0, 0.0)),
            Triplet::new(1, 1, c64::new(1.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let err = IluPreconditioner::new(a.as_ref(), 0).unwrap_err();
        assert!(matches!(err, KspError::Breakdown { .. }));
    }

    /// ILU(0) reduces or matches Jacobi iteration count on a denser
    /// SPD-ish fixture — direct comparison of preconditioner quality
    /// on the same Krylov core.
    #[test]
    fn ilu0_iterations_le_jacobi_on_spd_ish() {
        // 5×5 dense-ish symmetric SPD. Jacobi (diagonal only) is far
        // from the inverse here; ILU(0) is exact LU for this fully
        // populated pattern.
        let n = 5;
        // Build a symmetric SPD matrix: A = G + 5I with G symmetric.
        let g = [
            [0.0, 0.5, 0.3, 0.2, 0.1],
            [0.5, 0.0, 0.4, 0.3, 0.2],
            [0.3, 0.4, 0.0, 0.5, 0.3],
            [0.2, 0.3, 0.5, 0.0, 0.4],
            [0.1, 0.2, 0.3, 0.4, 0.0],
        ];
        let mut trips = Vec::new();
        for (i, row) in g.iter().enumerate() {
            for (j, &v_g) in row.iter().enumerate() {
                let v = if i == j { v_g + 5.0 } else { v_g };
                trips.push(Triplet::new(i, j, c64::new(v, 0.0)));
            }
        }
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = (0..n).map(|i| c64::new(i as f64 + 1.0, 0.0)).collect();

        let cocg = Cocg::new(1e-12, 200);

        let mut x_j = vec![c64::new(0.0, 0.0); n];
        let jacobi = JacobiPreconditioner::new(a.as_ref()).unwrap();
        let report_j = cocg.solve(a.as_ref(), &b, &mut x_j, &jacobi).unwrap();

        let mut x_i = vec![c64::new(0.0, 0.0); n];
        let ilu = IluPreconditioner::new(a.as_ref(), 0).unwrap();
        let report_i = cocg.solve(a.as_ref(), &b, &mut x_i, &ilu).unwrap();

        assert!(report_j.converged && report_i.converged);
        // For a fully populated 5×5 pattern, ILU(0) is exact LU and
        // hence solves in ≤ 2 COCG iterations; Jacobi typically needs
        // more.
        assert!(
            report_i.iters <= report_j.iters,
            "ILU(0) iters ({}) must not exceed Jacobi iters ({}) on this fixture",
            report_i.iters,
            report_j.iters,
        );
        // And the solutions must agree.
        let diff: f64 = x_i
            .iter()
            .zip(x_j.iter())
            .map(|(a, b)| (a - b).norm_sqr())
            .sum::<f64>()
            .sqrt();
        let norm: f64 = x_j.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt();
        assert!(diff / norm < 1e-8);
    }

    /// ILU(0) panics on `fill_level != 0` — scope rails for issue
    /// #267 (no ILU(k≥1) yet).
    #[test]
    #[should_panic(expected = "ILU(0)")]
    fn ilu_panics_on_nonzero_fill_level() {
        let trips = vec![
            Triplet::new(0_usize, 0, c64::new(2.0, 0.0)),
            Triplet::new(1, 1, c64::new(3.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let _ = IluPreconditioner::new(a.as_ref(), 1);
    }

    // -----------------------------------------------------------------
    // Chebyshev preconditioner — unit tests (issue #299)
    // -----------------------------------------------------------------

    /// Build the shared 3×3 complex-symmetric lossy fixture (the same
    /// one the COCG and ILU(0) tests use).
    fn complex_symmetric_lossy_3x3() -> SparseColMat<usize, c64> {
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
        SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap()
    }

    /// Degree 0 must reduce **exactly** to the Jacobi preconditioner —
    /// `z = D⁻¹ r` with no polynomial steps. Verified element-wise
    /// against [`JacobiPreconditioner`].
    #[test]
    fn chebyshev_degree_zero_is_jacobi() {
        let a = complex_symmetric_lossy_3x3();
        let cheb = ChebyshevPreconditioner::new(a.as_ref(), 0).unwrap();
        let jac = JacobiPreconditioner::new(a.as_ref()).unwrap();

        let r = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];
        let mut z_cheb = vec![c64::new(0.0, 0.0); 3];
        let mut z_jac = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&r, &mut z_cheb);
        jac.apply(&r, &mut z_jac);
        for i in 0..3 {
            assert!(
                (z_cheb[i] - z_jac[i]).norm() < 1e-15,
                "degree-0 Chebyshev must match Jacobi at index {i}: {:?} vs {:?}",
                z_cheb[i],
                z_jac[i],
            );
        }
    }

    /// **Bilinear-form / no-conjugation check.** The smoother applies a
    /// real polynomial `p_k(Â)` in the complex-symmetric operator
    /// `Â = D⁻¹A`. If any inner step accidentally conjugated, the output
    /// would differ from the same polynomial evaluated with conjugation
    /// flipped. We assert the smoother is **linear** over `c64` scalars
    /// — `M⁻¹(α r) = α M⁻¹(r)` for a genuinely complex `α` — which a
    /// conjugating implementation (anti-linear in the conjugated slots)
    /// cannot satisfy. This is the direct analog of the constraint
    /// [`IluPreconditioner`] documents.
    #[test]
    fn chebyshev_preserves_bilinear_form_no_conjugation() {
        let a = complex_symmetric_lossy_3x3();
        let cheb = ChebyshevPreconditioner::new(a.as_ref(), 4).unwrap();

        let r = vec![c64::new(1.0, -0.7), c64::new(-0.4, 1.3), c64::new(0.9, 0.2)];
        // A scalar with a non-trivial imaginary part — the discriminator
        // between linear (correct) and anti-linear (conjugating) maps.
        let alpha = c64::new(0.5, -1.2);

        let mut z_r = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&r, &mut z_r);

        let ar: Vec<c64> = r.iter().map(|&x| alpha * x).collect();
        let mut z_ar = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&ar, &mut z_ar);

        // Linearity: M⁻¹(α r) == α M⁻¹(r). A conjugating implementation
        // would instead produce ᾱ M⁻¹(r) (or worse), failing here.
        for i in 0..3 {
            let expected = alpha * z_r[i];
            assert!(
                (z_ar[i] - expected).norm() < 1e-12,
                "Chebyshev apply must be c64-linear (no conjugation) at index {i}: \
                 got {:?}, expected {:?}",
                z_ar[i],
                expected,
            );
        }
    }

    /// COCG with the Chebyshev smoother must drive the small
    /// complex-symmetric lossy system to round-off — the end-to-end
    /// "no conjugation" check (a conjugating smoother would break COCG's
    /// bilinear recurrence and either stall or diverge).
    #[test]
    fn chebyshev_solves_complex_symmetric_lossy() {
        let a = complex_symmetric_lossy_3x3();
        let b: Vec<c64> = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];

        let mut x = vec![c64::new(0.0, 0.0); 3];
        let pc = ChebyshevPreconditioner::new(a.as_ref(), 3).expect("Chebyshev smoother");
        let cocg = Cocg::new(1e-12, 200);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges with Chebyshev");
        assert!(report.converged);
        assert!(report.residual_rel < 1e-10, "{:?}", report);
    }

    /// The Chebyshev preconditioner must reject a zero-diagonal matrix
    /// (its Jacobi scaling is undefined), mirroring Jacobi/ILU.
    #[test]
    fn chebyshev_rejects_zero_diagonal() {
        let trips = vec![
            Triplet::new(0_usize, 1, c64::new(1.0, 0.0)),
            Triplet::new(1, 0, c64::new(1.0, 0.0)),
        ];
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(2, 2, &trips).unwrap();
        let err = ChebyshevPreconditioner::new(a.as_ref(), 2).unwrap_err();
        assert!(matches!(err, KspError::Breakdown { .. }));
    }

    /// Build a fourth-kind (Lottes 2022) smoother of the given degree on
    /// the shared 3×3 lossy fixture.
    fn chebyshev_fourth(a: &SparseColMat<usize, c64>, degree: usize) -> ChebyshevPreconditioner {
        let config = ChebyshevConfig {
            kind: ChebyshevKind::Fourth,
            ..ChebyshevConfig::default()
        };
        ChebyshevPreconditioner::with_config(a.as_ref(), degree, config)
            .expect("fourth-kind smoother")
    }

    /// **Issue #348**: the fourth-kind selector must construct and run a
    /// degree-`k` smoother with **no panic** (the old #299 scope rail is
    /// gone). Both kinds must produce finite output of the right shape.
    #[test]
    fn chebyshev_fourth_kind_runs_without_panic() {
        let a = complex_symmetric_lossy_3x3();
        let cheb = chebyshev_fourth(&a, 4);
        assert_eq!(cheb.dim(), 3);

        let r = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];
        let mut z = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&r, &mut z);
        for zi in &z {
            assert!(zi.re.is_finite() && zi.im.is_finite(), "{zi:?}");
        }
        // A nonzero RHS must produce a nonzero correction.
        let mag: f64 = z.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        assert!(mag > 0.0);
    }

    /// **Issue #348**: degree-0 fourth-kind must reduce **exactly** to
    /// Jacobi (`z = D⁻¹ r`), the same degenerate contract as first-kind.
    #[test]
    fn chebyshev_fourth_kind_degree_zero_is_jacobi() {
        let a = complex_symmetric_lossy_3x3();
        let cheb = chebyshev_fourth(&a, 0);
        let jac = JacobiPreconditioner::new(a.as_ref()).unwrap();

        let r = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];
        let mut z_cheb = vec![c64::new(0.0, 0.0); 3];
        let mut z_jac = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&r, &mut z_cheb);
        jac.apply(&r, &mut z_jac);
        for i in 0..3 {
            assert!(
                (z_cheb[i] - z_jac[i]).norm() < 1e-15,
                "degree-0 fourth-kind Chebyshev must match Jacobi at index {i}: \
                 {:?} vs {:?}",
                z_cheb[i],
                z_jac[i],
            );
        }
    }

    /// **Issue #348 — bilinear-form / no-conjugation check** for the
    /// fourth-kind smoother. Identical discriminator to the first-kind
    /// test: a genuinely complex scalar `α` must commute through the
    /// apply (`M⁻¹(α r) = α M⁻¹(r)`). A conjugating (anti-linear)
    /// implementation would instead yield `ᾱ M⁻¹(r)` and fail — this is
    /// the load-bearing invariant for COCG's bilinear recurrence.
    #[test]
    fn chebyshev_fourth_kind_preserves_bilinear_form_no_conjugation() {
        let a = complex_symmetric_lossy_3x3();
        let cheb = chebyshev_fourth(&a, 4);

        let r = vec![c64::new(1.0, -0.7), c64::new(-0.4, 1.3), c64::new(0.9, 0.2)];
        let alpha = c64::new(0.5, -1.2);

        let mut z_r = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&r, &mut z_r);

        let ar: Vec<c64> = r.iter().map(|&x| alpha * x).collect();
        let mut z_ar = vec![c64::new(0.0, 0.0); 3];
        cheb.apply(&ar, &mut z_ar);

        for i in 0..3 {
            let expected = alpha * z_r[i];
            assert!(
                (z_ar[i] - expected).norm() < 1e-12,
                "fourth-kind apply must be c64-linear (no conjugation) at index \
                 {i}: got {:?}, expected {:?}",
                z_ar[i],
                expected,
            );
        }
    }

    /// **Issue #348**: COCG with the fourth-kind smoother must drive the
    /// small complex-symmetric lossy system to round-off — the
    /// end-to-end "no conjugation" check (a conjugating smoother would
    /// break COCG's bilinear recurrence and stall or diverge).
    #[test]
    fn chebyshev_fourth_kind_solves_complex_symmetric_lossy() {
        let a = complex_symmetric_lossy_3x3();
        let b: Vec<c64> = vec![c64::new(1.0, -0.2), c64::new(2.0, 0.1), c64::new(0.5, 0.3)];

        let mut x = vec![c64::new(0.0, 0.0); 3];
        let pc = chebyshev_fourth(&a, 3);
        let cocg = Cocg::new(1e-12, 200);
        let report = cocg
            .solve(a.as_ref(), &b, &mut x, &pc)
            .expect("COCG converges with fourth-kind Chebyshev");
        assert!(report.converged);
        assert!(report.residual_rel < 1e-10, "{:?}", report);
    }

    /// **Issue #348**: a positive-degree fourth-kind smoother must not
    /// increase the COCG iteration count versus the unpreconditioned
    /// (identity) solve on the denser SPD-ish fixture, and must agree on
    /// the solution. Reports both counts (and the first-kind count) to
    /// stderr, mirroring the #299 reporting style.
    #[test]
    fn chebyshev_fourth_kind_iterations_lt_unpreconditioned_on_spd_ish() {
        let n = 5;
        let g = [
            [0.0, 0.5, 0.3, 0.2, 0.1],
            [0.5, 0.0, 0.4, 0.3, 0.2],
            [0.3, 0.4, 0.0, 0.5, 0.3],
            [0.2, 0.3, 0.5, 0.0, 0.4],
            [0.1, 0.2, 0.3, 0.4, 0.0],
        ];
        let mut trips = Vec::new();
        for (i, row) in g.iter().enumerate() {
            for (j, &v_g) in row.iter().enumerate() {
                let v = if i == j { v_g + 5.0 } else { v_g };
                trips.push(Triplet::new(i, j, c64::new(v, 0.0)));
            }
        }
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = (0..n).map(|i| c64::new(i as f64 + 1.0, 0.0)).collect();

        let cocg = Cocg::new(1e-12, 500);

        let mut x_id = vec![c64::new(0.0, 0.0); n];
        let id = IdentityPreconditioner::new(n);
        let report_id = cocg.solve(a.as_ref(), &b, &mut x_id, &id).unwrap();

        let mut x_c1 = vec![c64::new(0.0, 0.0); n];
        let c1 = ChebyshevPreconditioner::new(a.as_ref(), 4).unwrap();
        let report_c1 = cocg.solve(a.as_ref(), &b, &mut x_c1, &c1).unwrap();

        let mut x_c4 = vec![c64::new(0.0, 0.0); n];
        let c4 = chebyshev_fourth(&a, 4);
        let report_c4 = cocg.solve(a.as_ref(), &b, &mut x_c4, &c4).unwrap();

        assert!(report_id.converged && report_c1.converged && report_c4.converged);
        assert!(
            report_c4.iters <= report_id.iters,
            "fourth-kind iters ({}) must not exceed unpreconditioned iters ({})",
            report_c4.iters,
            report_id.iters,
        );

        // Solution must agree with the unpreconditioned run.
        let diff: f64 = x_c4
            .iter()
            .zip(x_id.iter())
            .map(|(a, b)| (a - b).norm_sqr())
            .sum::<f64>()
            .sqrt();
        let norm: f64 = x_id.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt();
        assert!(diff / norm < 1e-8);

        eprintln!(
            "[issue #348] Chebyshev kinds vs unpreconditioned on 5×5 SPD-ish: \
             identity iters={}, first-kind(deg=4) iters={}, \
             fourth-kind(deg=4) iters={}, interval={:?}",
            report_id.iters,
            report_c1.iters,
            report_c4.iters,
            c4.spectral_interval(),
        );
    }

    /// A positive-degree Chebyshev smoother must reduce the COCG
    /// iteration count versus the **unpreconditioned** (identity) solve
    /// on a denser SPD-ish fixture — the same fixture the ILU-vs-Jacobi
    /// unit test uses. Reports both counts to stderr.
    #[test]
    fn chebyshev_iterations_lt_unpreconditioned_on_spd_ish() {
        let n = 5;
        let g = [
            [0.0, 0.5, 0.3, 0.2, 0.1],
            [0.5, 0.0, 0.4, 0.3, 0.2],
            [0.3, 0.4, 0.0, 0.5, 0.3],
            [0.2, 0.3, 0.5, 0.0, 0.4],
            [0.1, 0.2, 0.3, 0.4, 0.0],
        ];
        let mut trips = Vec::new();
        for (i, row) in g.iter().enumerate() {
            for (j, &v_g) in row.iter().enumerate() {
                let v = if i == j { v_g + 5.0 } else { v_g };
                trips.push(Triplet::new(i, j, c64::new(v, 0.0)));
            }
        }
        let a = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips).unwrap();
        let b: Vec<c64> = (0..n).map(|i| c64::new(i as f64 + 1.0, 0.0)).collect();

        let cocg = Cocg::new(1e-12, 500);

        let mut x_id = vec![c64::new(0.0, 0.0); n];
        let id = IdentityPreconditioner::new(n);
        let report_id = cocg.solve(a.as_ref(), &b, &mut x_id, &id).unwrap();

        let mut x_cheb = vec![c64::new(0.0, 0.0); n];
        let cheb = ChebyshevPreconditioner::new(a.as_ref(), 4).unwrap();
        let report_cheb = cocg.solve(a.as_ref(), &b, &mut x_cheb, &cheb).unwrap();

        assert!(report_id.converged && report_cheb.converged);
        assert!(
            report_cheb.iters <= report_id.iters,
            "Chebyshev iters ({}) must not exceed unpreconditioned iters ({})",
            report_cheb.iters,
            report_id.iters,
        );
        // Solutions must agree.
        let diff: f64 = x_cheb
            .iter()
            .zip(x_id.iter())
            .map(|(a, b)| (a - b).norm_sqr())
            .sum::<f64>()
            .sqrt();
        let norm: f64 = x_id.iter().map(|x| x.norm_sqr()).sum::<f64>().sqrt();
        assert!(diff / norm < 1e-8);

        eprintln!(
            "[issue #299] Chebyshev(deg=4) vs unpreconditioned on 5×5 SPD-ish: \
             identity iters={}, Chebyshev iters={}, interval={:?}",
            report_id.iters,
            report_cheb.iters,
            cheb.spectral_interval(),
        );
    }
}
