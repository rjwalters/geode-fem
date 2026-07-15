//! AMS-lite (auxiliary-space Maxwell, Hiptmair–Xu 2007) preconditioner for
//! the matrix-free inner solve of the shift-invert eigensolver (issue #526,
//! follow-on to the matrix-free path of #524).
//!
//! # Why AMS
//!
//! The matrix-free inner CG ([`crate::eigen::lanczos`], `InnerSolver::MatrixFree`)
//! solves the shifted H(curl) curl-curl pencil `(K − σM) y = b` with only a
//! **Jacobi** (diagonal) preconditioner. That system is extremely
//! ill-conditioned: the Nédélec curl-curl stiffness `K` has a huge near-kernel
//! equal to `image(d⁰)` (the discrete gradients — `kernel(K) = image(d⁰)` by
//! the de-Rham identity), and Jacobi does nothing to damp those low-energy
//! gradient error components. On the 1.16M-DOF transmon eigensolve the
//! Jacobi-CG did not converge in 28 minutes.
//!
//! The Hiptmair–Xu **auxiliary-space** fix preconditions the curl-curl operator
//! by mapping the troublesome gradient error into the **nodal (H1) auxiliary
//! space** through the discrete gradient `G = d⁰_interior`, correcting it with a
//! cheap nodal Poisson-like solve there, and prolonging back with `G`. The
//! gradient near-kernel that Jacobi cannot see becomes an ordinary well-
//! conditioned nodal problem in the auxiliary space.
//!
//! # AMS-lite and the full three-space cycle
//!
//! The full AMS of Hiptmair–Xu uses **two** auxiliary spaces: the scalar
//! gradient space `G` and the **vector-nodal** space `Π` (three Cartesian
//! nodal-vector components interpolated onto the edge DOFs). The gradient
//! space alone damps the `image(d⁰)` near-kernel Jacobi cannot see; the
//! vector-nodal `Πᵀ A Π` block corrects the remaining H(curl) error components
//! the gradient space does not reach.
//!
//! This module implements both. The gradient-only two-space cycle (edge
//! smoother + `G (Gᵀ A G)⁻¹ Gᵀ`) is the default; when the caller supplies the
//! per-edge geometry (via [`InteriorGradient::with_edge_vectors`], issue #550)
//! the preconditioner additionally forms the vector-nodal interpolation `Π`
//! (`edge_dim × 3·node_dim`) and its coarse operator `Πᵀ A Π`, and adds the
//! `Π (ΠᵀAΠ)⁻¹ Πᵀ` correction — the complete Hiptmair–Xu three-space cycle.
//! The two auxiliary corrections are combined **additively** on the same
//! residual (as in the original Hiptmair–Xu splitting), which keeps each apply
//! mode symmetric positive definite (a sum of SPD subspace corrections wrapped
//! by the symmetric pre-/post-smooth). Two apply modes are provided:
//!
//! - **Multiplicative symmetric V-cycle** (`AmsLitePreconditioner::apply_vcycle`,
//!   the shipped default): damped pre-smooth, gradient-space coarse correction on
//!   the residual, damped post-smooth. Each stage sees the residual left by the
//!   previous one, so the corrections compound — this is what delivers the ≥5×
//!   inner-CG iteration reduction the acceptance criteria call for. The symmetric
//!   pre-/post-smooth around the self-adjoint coarse solve keeps the cycle SPD (a
//!   valid CG preconditioner), and a **damped** Jacobi smoother (`ω < 1`) is
//!   required because an undamped point-Jacobi is not a contraction across the
//!   wide H(curl) edge spectrum (an undamped multiplicative cycle diverges —
//!   measured).
//! - **Additive form** (`AmsLitePreconditioner::apply`): `z = D⁻¹ r + G C⁻¹ Gᵀ r`,
//!   a sum of two SPD operators. Simpler and matvec-free, but weaker (the smoother
//!   and coarse correction overlap on the low modes); retained as the fallback /
//!   reference form.
//!
//! The **gradient-space coarse correction** forms the nodal coupling
//! `C = Gᵀ A G` (`A = K − σM`, a `node_dim × node_dim` SPD matrix, one row per
//! free interior node — `node_dim ≪ edge_dim`), and each apply is `Gᵀ·`
//! (restrict to nodes), an **approximate coarse solve** of `C`, and `G·`
//! (prolong to edges). This is exactly the Hiptmair–Xu nodal auxiliary-space
//! correction that damps the gradient near-kernel Jacobi is blind to. A
//! preconditioner changes only convergence speed, never the fixed point, so the
//! eigenvalues are unchanged either way.
//!
//! # Coarse solve: multilevel-style few-sweep smoother, no global factor (#551)
//!
//! Through issue #550 the coarse operators `C = Gᵀ A G` and `Πᵀ A Π` were each
//! **LU-factored once** and applied by a **global triangular solve every inner-CG
//! iteration**. That serial global solve is `O(node_dim^{1.x})` per apply and was
//! the single-level bottleneck that stopped the 1.16M matrix-free eigensolve from
//! completing in minutes even after AMS-lite cut inner-CG iterations 5.36× — it is
//! exactly the part Palace/hypre make scale by using a **multilevel** coarse solve
//! (BoomerAMG) instead of a direct factor.
//!
//! Issue #551 replaces the direct factor with a **few-sweep symmetric
//! Gauss–Seidel** approximate coarse solve (`SgsCoarseSolver`): each apply is a
//! fixed number of forward+backward GS sweeps starting from a zero guess, which is
//! `O(nnz(C)) = O(node_dim)` work per apply and needs **no global factor** (only
//! the coarse operator's sparse rows and its inverse-diagonal are stored). This is
//! the smoother-based V-cycle the issue explicitly allows in place of a full AMG;
//! a symmetric (forward+backward) sweep count keeps each coarse solve
//! **self-adjoint and SPD** (for the SPD coarse operator SGS converges, so a fixed
//! number of symmetric SGS–Richardson iterations from a zero start is a symmetric
//! positive-definite approximate inverse), which is what keeps the whole
//! preconditioner a valid CG preconditioner. The coarse solve is now
//! **approximate** — it changes the iteration path, not the converged eigenvalues.
//!
//! The direct sparse-LU coarse solve is retained behind `CoarseSolve::Direct`
//! purely so the measurement harness can compare per-apply cost and inner-CG
//! iteration count against the new `CoarseSolve::SymmetricGaussSeidel` default
//! apples-to-apples; the shipped inner solve always uses the few-sweep smoother.
//!
//! # Memory
//!
//! `C = Gᵀ A G` is node-indexed and the few-sweep smoother stores only its sparse
//! rows + inverse-diagonal (`O(node_dim)`), an order of magnitude below the edge
//! pencil and with no global factor at all. The working set stays `O(N)`: the
//! borrowed edge operators, the length-`edge_dim` Jacobi diagonal, and the small
//! node-space coarse operators. No edge-space factorization is ever formed.
//!
//! The vector-nodal `Πᵀ A Π` block (issue #550) is node-vector-indexed
//! (`3·node_dim` square, still an order of magnitude below `edge_dim`) and is
//! solved by the same few-sweep smoother, so its working set is likewise `O(N)`.
//! The at-scale iteration numbers on the 133k / 1.16M meshes remain an
//! operator/AWS follow-up (issue #531 sub-phase 1c); this module delivers the
//! correct, SPD, spectrum-preserving three-space operator with an O(node_dim)
//! coarse solve and the local iteration-count measurement.

use faer::Mat;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::dense::EigenError;
use crate::eigen::projection::InteriorGradient;

/// Default damped-Jacobi smoother weight `ω` for the multiplicative V-cycle.
///
/// An undamped point-Jacobi smoother is not a contraction across the wide
/// H(curl) edge spectrum (the high-frequency edge modes have `‖I − D⁻¹A‖ > 1`),
/// so a multiplicative V-cycle built on it diverges. `ω = 0.5` restores a
/// contractive smoother, which is what makes the multiplicative cycle beat the
/// additive form.
const DEFAULT_SMOOTH_WEIGHT: f64 = 0.6;

/// Default number of **symmetric** (forward + backward) Gauss–Seidel sweeps for
/// the approximate coarse solve ([`SgsCoarseSolver`], issue #551).
///
/// The coarse operators `C = Gᵀ A G` and `Πᵀ A Π` are SPD and well-conditioned
/// relative to the edge pencil (they are nodal Poisson-like), so SGS converges
/// geometrically and a handful of sweeps is a good approximate inverse. Two
/// symmetric sweeps keep the coarse correction strong enough that inner-CG needs
/// no more iterations than the exact direct factor at the fixture sizes measured
/// locally, while making each coarse apply `O(node_dim)` with no global factor.
const DEFAULT_COARSE_SWEEPS: usize = 2;

/// Which coarse solver [`AmsLitePreconditioner::build_with_coarse`] wires into
/// the gradient-space `C = Gᵀ A G` and vector-nodal `Πᵀ A Π` corrections.
///
/// The shipped default is [`Self::SymmetricGaussSeidel`] — the O(node_dim)-per-
/// apply few-sweep smoother of issue #551. [`Self::Direct`] (the pre-#551 cached
/// sparse LU) is retained only so the measurement harness can compare the two
/// apples-to-apples.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CoarseSolve {
    /// Direct sparse LU of the coarse operator, applied by a global triangular
    /// solve each apply (the pre-#551 behavior; kept for measurement only, so it
    /// is only constructed under `cfg(test)`).
    #[cfg_attr(not(test), allow(dead_code))]
    Direct,
    /// Few-sweep symmetric Gauss–Seidel approximate solve — the O(node_dim)-per-
    /// apply coarse solve of issue #551. The `usize` is the symmetric sweep
    /// count (forward + backward per sweep).
    SymmetricGaussSeidel(usize),
}

impl Default for CoarseSolve {
    fn default() -> Self {
        CoarseSolve::SymmetricGaussSeidel(DEFAULT_COARSE_SWEEPS)
    }
}

/// A coarse solver for one of the SPD nodal coarse operators — either the exact
/// direct LU (measurement reference) or the few-sweep symmetric Gauss–Seidel
/// approximation shipped by issue #551.
pub(crate) enum CoarseSolver {
    /// Cached sparse LU; `solve` is a global triangular solve.
    Direct(Lu<usize, f64>),
    /// Few-sweep symmetric Gauss–Seidel; `solve` is `O(nnz)` per apply.
    Sgs(SgsCoarseSolver),
}

impl CoarseSolver {
    /// Build the selected coarse solver for the SPD coarse operator `mat`.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the [`CoarseSolve::Direct`] sparse LU
    /// factorization fails. The Gauss–Seidel variant is infallible.
    fn build(mat: &SparseColMat<usize, f64>, mode: CoarseSolve) -> Result<Self, EigenError> {
        match mode {
            CoarseSolve::Direct => {
                let lu = mat
                    .as_ref()
                    .sp_lu()
                    .map_err(|e| EigenError::FaerGevd(format!("coarse sparse LU: {e:?}")))?;
                Ok(CoarseSolver::Direct(lu))
            }
            CoarseSolve::SymmetricGaussSeidel(sweeps) => Ok(CoarseSolver::Sgs(
                SgsCoarseSolver::from_csc(mat.as_ref(), sweeps),
            )),
        }
    }

    /// Approximately (SGS) or exactly (Direct) solve `mat · out = b`.
    /// `out` is overwritten; `b` and `out` have length `mat.ncols()`.
    fn solve(&self, b: &[f64], out: &mut [f64]) {
        match self {
            CoarseSolver::Direct(lu) => {
                use faer::linalg::solvers::Solve;
                let n = b.len();
                let mut mat: Mat<f64> = Mat::from_fn(n, 1, |row, _| b[row]);
                lu.solve_in_place(mat.as_mut());
                for (row, o) in out.iter_mut().enumerate() {
                    *o = mat[(row, 0)];
                }
            }
            CoarseSolver::Sgs(sgs) => sgs.solve(b, out),
        }
    }
}

/// Few-sweep **symmetric Gauss–Seidel** approximate solver for an SPD coarse
/// operator (issue #551), replacing the single-level direct LU factor.
///
/// Holds a row-wise (CSR) copy of the coarse operator and its inverse-diagonal.
/// [`Self::solve`] runs `sweeps` symmetric sweeps (each = one forward GS sweep
/// followed by one backward GS sweep) starting from a zero guess. For an SPD
/// operator SGS converges, so a fixed number of symmetric SGS–Richardson
/// iterations from zero is a **symmetric positive-definite** approximate inverse
/// — exactly the property that keeps the enclosing V-cycle a valid CG
/// preconditioner. Each apply is `O(nnz)`; no global factor is stored.
pub(crate) struct SgsCoarseSolver {
    /// Operator dimension (`node_dim` for `C`, `3·node_dim` for `Πᵀ A Π`).
    n: usize,
    /// CSR row pointers (`n + 1`).
    row_ptr: Vec<usize>,
    /// CSR column indices (includes the diagonal entry).
    col_idx: Vec<usize>,
    /// CSR values, aligned with [`Self::col_idx`].
    val: Vec<f64>,
    /// Inverse main diagonal `1 / A_ii` (fallback `1.0` on a zero pivot).
    inv_diag: Vec<f64>,
    /// Symmetric sweep count (forward + backward per sweep).
    sweeps: usize,
}

impl SgsCoarseSolver {
    /// Build the CSR + inverse-diagonal from a CSC coarse operator. The coarse
    /// operator is symmetric, so its CSC columns are its CSR rows; we still
    /// transpose explicitly (cheap, `O(nnz)`) rather than assume the layout.
    fn from_csc(a: SparseColMatRef<'_, usize, f64>, sweeps: usize) -> Self {
        let n = a.ncols();
        let col_ptr = a.col_ptr();
        let row_idx = a.row_idx();
        let val = a.val();
        // Row buckets: rows[i] = list of (col, value) for row i of A.
        let mut rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for j in 0..n {
            for k in col_ptr[j]..col_ptr[j + 1] {
                rows[row_idx[k]].push((j, val[k]));
            }
        }
        let nnz: usize = rows.iter().map(|r| r.len()).sum();
        let mut row_ptr = Vec::with_capacity(n + 1);
        let mut col_idx = Vec::with_capacity(nnz);
        let mut vals = Vec::with_capacity(nnz);
        let mut inv_diag = vec![1.0_f64; n];
        row_ptr.push(0);
        for (i, row) in rows.iter().enumerate() {
            let mut dii = 0.0;
            for &(j, v) in row {
                col_idx.push(j);
                vals.push(v);
                if j == i {
                    dii += v;
                }
            }
            if dii.abs() > 0.0 {
                inv_diag[i] = 1.0 / dii;
            }
            row_ptr.push(col_idx.len());
        }
        Self {
            n,
            row_ptr,
            col_idx,
            val: vals,
            inv_diag,
            sweeps,
        }
    }

    /// One Gauss–Seidel update of row `i` in place: `y_i ← D_ii⁻¹ (b_i − Σ_{j≠i} A_ij y_j)`.
    #[inline]
    fn update_row(&self, i: usize, b: &[f64], y: &mut [f64]) {
        let mut s = b[i];
        for k in self.row_ptr[i]..self.row_ptr[i + 1] {
            let j = self.col_idx[k];
            if j != i {
                s -= self.val[k] * y[j];
            }
        }
        y[i] = self.inv_diag[i] * s;
    }

    /// Approximately solve `A y = b` with `sweeps` symmetric GS sweeps from a
    /// zero start. `y` is overwritten (length `n`).
    fn solve(&self, b: &[f64], y: &mut [f64]) {
        y.iter_mut().for_each(|v| *v = 0.0);
        for _ in 0..self.sweeps {
            for i in 0..self.n {
                self.update_row(i, b, y);
            }
            for i in (0..self.n).rev() {
                self.update_row(i, b, y);
            }
        }
    }

    /// `y = A · x` for the stored coarse operator (used by the coarse-solver
    /// residual/SPD unit tests).
    #[cfg(test)]
    fn spmv(&self, x: &[f64], y: &mut [f64]) {
        for (i, out) in y.iter_mut().enumerate() {
            let mut s = 0.0;
            for k in self.row_ptr[i]..self.row_ptr[i + 1] {
                s += self.val[k] * x[self.col_idx[k]];
            }
            *out = s;
        }
    }
}

/// `y = A · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == ncols`, `y.len() == nrows`.
fn spmv(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    y.iter_mut().for_each(|v| *v = 0.0);
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            y[row_idx[k]] += val[k] * xj;
        }
    }
}

/// `y = Aᵀ · x` for a CSC sparse matrix (overwrite). `A` is `nrows × ncols`;
/// `x.len() == nrows`, `y.len() == ncols`. Column `j` of `A` dotted with `x`
/// is entry `j` of `Aᵀx`.
fn spmv_transpose(a: SparseColMatRef<'_, usize, f64>, x: &[f64], y: &mut [f64]) {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let mut acc = 0.0;
        for k in col_ptr[j]..col_ptr[j + 1] {
            acc += val[k] * x[row_idx[k]];
        }
        y[j] = acc;
    }
}

/// The AMS-lite preconditioner: an edge Jacobi smoother plus a gradient-space
/// (nodal) coarse correction, sharing the discrete gradient `G` with the
/// divergence-free projector.
///
/// Built once (before the inner CG loop) from the discrete gradient `G` and the
/// two edge operators `K`, `M` plus the shift `σ`. Holds the Jacobi
/// inverse-diagonal of `A = K − σM`, the sparse `G`, and an O(node_dim)-per-apply
/// coarse solver for the nodal coupling `C = Gᵀ A G` (few-sweep symmetric
/// Gauss–Seidel by default, issue #551). [`Self::apply`] realizes the additive
/// apply `z = D⁻¹ r + G C⁻¹ Gᵀ r`.
pub(crate) struct AmsLitePreconditioner {
    /// Sparse discrete gradient `G` (`edge_dim × node_dim`), owned via a
    /// cloned [`InteriorGradient`] (which itself holds an owned `SparseColMat`).
    gradient: InteriorGradient,
    /// Jacobi inverse-diagonal `1 / (K_ii − σ M_ii)` (edge space), with a
    /// zero-pivot fallback to `1.0` — identical to the matrix-free baseline's
    /// Jacobi so the two paths agree when the coarse term is inactive.
    inv_diag: Vec<f64>,
    /// O(node_dim)-per-apply coarse solver for the nodal coupling
    /// `C = Gᵀ A G` (node-indexed, SPD) — few-sweep symmetric Gauss–Seidel by
    /// default (issue #551), or the cached direct LU under
    /// [`CoarseSolve::Direct`] for the measurement comparison.
    c_coarse: CoarseSolver,
    /// The vector-nodal interpolation `Π` (`edge_dim × 3·node_dim`) of the
    /// full Hiptmair–Xu AMS (issue #550), present only when the discrete
    /// gradient carried per-edge geometry
    /// ([`InteriorGradient::with_edge_vectors`]). `None` ⇒ the gradient-only
    /// two-space cycle.
    pi: Option<SparseColMat<usize, f64>>,
    /// O(node_dim)-per-apply coarse solver for the vector-nodal coarse operator
    /// `Πᵀ A Π` (`3·node_dim` square, SPD), paired with [`Self::pi`]. `Some`
    /// iff `pi` is `Some`.
    pi_coarse: Option<CoarseSolver>,
    /// Damped-Jacobi smoother weight `ω` for the multiplicative V-cycle
    /// ([`Self::apply_vcycle`]). An undamped (`ω = 1`) Jacobi smoother is not a
    /// contraction on the wide H(curl) spectrum, so the multiplicative cycle
    /// diverges; `ω < 1` restores a contractive smoother. Unused by the
    /// additive [`Self::apply`].
    smooth_weight: f64,
    /// Edge DOF count (rows of `G`, length of the vectors this acts on).
    edge_dim: usize,
    /// Free interior-node count (cols of `G`, size of the `C` solve).
    node_dim: usize,
}

impl AmsLitePreconditioner {
    /// Build the AMS-lite preconditioner.
    ///
    /// Assembles the nodal coupling `C = Gᵀ (K − σM) G` directly from the
    /// ultra-sparse `G` (≤2 nonzeros per row) and the edge operators — the same
    /// triplet outer-product assembly the divergence-free projector uses for
    /// `GᵀMG`, extended to the shifted pencil `A = K − σM`. `C` is then wired
    /// into the default O(node_dim)-per-apply coarse solver (few-sweep symmetric
    /// Gauss–Seidel, issue #551).
    ///
    /// `k` and `m` are the reduced edge operators (dimension `edge_dim`, which
    /// must equal `gradient.edge_dim()`); `sigma` is the shift.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the `C` assembly fails.
    pub(crate) fn build(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
    ) -> Result<Self, EigenError> {
        Self::build_with_coarse(gradient, k, m, sigma, CoarseSolve::default())
    }

    /// [`Self::build`] with an explicit coarse-solver choice. The shipped path
    /// uses [`CoarseSolve::default`] (few-sweep symmetric Gauss–Seidel); the
    /// measurement harness passes [`CoarseSolve::Direct`] to compare the exact
    /// direct factor against the approximate smoother apples-to-apples.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if a coarse operator assembly or (under
    /// [`CoarseSolve::Direct`]) its sparse LU factorization fails.
    pub(crate) fn build_with_coarse(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        coarse: CoarseSolve,
    ) -> Result<Self, EigenError> {
        let edge_dim = gradient.edge_dim();
        let node_dim = gradient.node_dim();
        assert_eq!(k.nrows(), edge_dim, "K rows must equal G rows (edge_dim)");
        assert_eq!(k.ncols(), edge_dim, "K cols must equal G rows (edge_dim)");
        assert_eq!(m.nrows(), edge_dim, "M rows must equal G rows (edge_dim)");
        assert_eq!(m.ncols(), edge_dim, "M cols must equal G rows (edge_dim)");

        // Jacobi inverse-diagonal of A = K − σM (edge space). This matches the
        // matrix-free baseline's `ShiftedMatrixFreeOp::precond` diagonal exactly.
        let mut dk = vec![0.0_f64; edge_dim];
        let mut dm = vec![0.0_f64; edge_dim];
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

        // Row view of G: g_rows[i] = list of (node_col, sign) (≤2 entries).
        let g_ref = gradient.matrix();
        let mut g_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
        {
            let col_ptr = g_ref.col_ptr();
            let row_idx = g_ref.row_idx();
            let val = g_ref.val();
            for col in 0..g_ref.ncols() {
                for kk in col_ptr[col]..col_ptr[col + 1] {
                    g_rows[row_idx[kk]].push((col, val[kk]));
                }
            }
        }

        // C = Gᵀ A G = Σ_{i,j : A[i,j]=v} v · gᵢ gⱼᵀ, with A = K − σM. We fold
        // the shift into the value stream: for a shared K/M sparsity pattern the
        // effective entry is K[i,j] − σ M[i,j]. We iterate K's and M's entries
        // separately with scales +1 and −σ and let faer's triplet dedup sum
        // coincident (p, q) contributions — no assumption that K and M share a
        // pattern.
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        accumulate_gtag(&g_rows, k, 1.0, &mut trips);
        if sigma != 0.0 {
            accumulate_gtag(&g_rows, m, -sigma, &mut trips);
        }

        let c = SparseColMat::<usize, f64>::try_new_from_triplets(node_dim, node_dim, &trips)
            .map_err(|e| EigenError::FaerGevd(format!("Gᵀ(K−σM)G assembly: {e:?}")))?;
        let c_coarse = CoarseSolver::build(&c, coarse)?;

        // Vector-nodal auxiliary space (full Hiptmair–Xu, issue #550): build
        // Π and its coarse solver for Πᵀ A Π, but ONLY when the caller supplied
        // the per-edge geometry. Without it we keep the gradient-only two-space
        // cycle (backward-compatible; Π is undefined without node coordinates).
        let (pi, pi_coarse) = match gradient.edge_vectors() {
            Some(edge_vectors) => {
                let pi = build_pi(&g_rows, edge_vectors, edge_dim, node_dim)?;
                let pi_ata = build_pi_ata(pi.as_ref(), k, m, sigma, node_dim)?;
                let pi_coarse = CoarseSolver::build(&pi_ata, coarse)?;
                (Some(pi), Some(pi_coarse))
            }
            None => (None, None),
        };

        Ok(Self {
            gradient: gradient.clone(),
            inv_diag,
            c_coarse,
            pi,
            pi_coarse,
            smooth_weight: DEFAULT_SMOOTH_WEIGHT,
            edge_dim,
            node_dim,
        })
    }

    /// Weighted edge Jacobi smooth `out = ω D⁻¹ r` (elementwise), where
    /// `D = diag(K − σM)`. `ω = 1` is the exact diagonal preconditioner (used
    /// by the additive [`Self::apply`]); the multiplicative V-cycle uses a
    /// damped `ω < 1` to keep the smoother contractive.
    fn jacobi_smooth_weighted(&self, r: &[f64], out: &mut [f64], weight: f64) {
        for i in 0..self.edge_dim {
            out[i] = weight * self.inv_diag[i] * r[i];
        }
    }

    /// Gradient-space (nodal) coarse correction `out = G C⁻¹ Gᵀ r`.
    ///
    /// Restricts the edge residual to nodes (`Gᵀ r`), solves the cached nodal
    /// system `C = Gᵀ A G`, and prolongs back to edges (`G ·`). This is the
    /// auxiliary-space term that damps the gradient near-kernel Jacobi is blind
    /// to; `out` is overwritten.
    fn coarse_correction(&self, r: &[f64], out: &mut [f64]) {
        // rc = Gᵀ r  (node space)
        let mut rc = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), r, &mut rc);
        // yc ≈ C⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; self.node_dim];
        self.c_coarse.solve(&rc, &mut yc);
        // out = G yc  (edge space)
        spmv(self.gradient.matrix(), &yc, out);
    }

    /// Vector-nodal coarse correction `out = Π (ΠᵀAΠ)⁻¹ Πᵀ r` (issue #550).
    ///
    /// The second Hiptmair–Xu auxiliary space: restricts the edge residual to
    /// the vector-nodal space (`Πᵀ r`, length `3·node_dim`), solves the cached
    /// `Πᵀ A Π` system there, and prolongs back to edges (`Π ·`). This corrects
    /// the H(curl) error components the scalar gradient space does not see.
    /// `out` is overwritten. Only called when [`Self::pi`] is `Some` (the
    /// three-space cycle); a no-op guard returns zeros otherwise.
    fn coarse_correction_pi(&self, r: &[f64], out: &mut [f64]) {
        let (Some(pi), Some(pi_coarse)) = (&self.pi, &self.pi_coarse) else {
            out.iter_mut().for_each(|v| *v = 0.0);
            return;
        };
        let pi_dim = 3 * self.node_dim;
        // rc = Πᵀ r  (vector-nodal space)
        let mut rc = vec![0.0_f64; pi_dim];
        spmv_transpose(pi.as_ref(), r, &mut rc);
        // yc ≈ (ΠᵀAΠ)⁻¹ rc  (few-sweep SGS, or exact LU under CoarseSolve::Direct)
        let mut yc = vec![0.0_f64; pi_dim];
        pi_coarse.solve(&rc, &mut yc);
        // out = Π yc  (edge space)
        spmv(pi.as_ref(), &yc, out);
    }

    /// Whether the full three-space (gradient + vector-nodal) cycle is active
    /// — i.e. the caller supplied per-edge geometry so `Π` could be built.
    #[cfg(test)]
    pub(crate) fn has_vector_nodal_space(&self) -> bool {
        self.pi.is_some()
    }

    /// Apply the AMS-lite preconditioner in **additive** form
    /// `z = D⁻¹ r + G C⁻¹ Gᵀ r (+ Π (ΠᵀAΠ)⁻¹ Πᵀ r)` (the reference / fallback
    /// form; the shipped default is the stronger multiplicative
    /// [`Self::apply_vcycle`]). The vector-nodal `Π` term is present only in the
    /// full three-space cycle (issue #550, when the gradient carried per-edge
    /// geometry); the gradient-only cycle drops it.
    ///
    /// The two SPD terms — the (undamped) edge Jacobi smoother and the
    /// gradient-space coarse correction — are summed independently, so the apply
    /// needs no operator matvec (unlike the V-cycle) and is guaranteed SPD as a
    /// sum of SPD operators. It damps the gradient near-kernel Jacobi is blind
    /// to, but is weaker than the V-cycle because the smoother and coarse
    /// correction overlap on the low modes. `r` and `z` are length `edge_dim`;
    /// `z` is overwritten.
    ///
    /// Currently used only as the reference form in the SPD unit test (the
    /// shipped inner solve uses [`Self::apply_vcycle`]), so it is `cfg(test)`;
    /// promote it if a caller ever selects the additive form at runtime.
    #[cfg(test)]
    pub(crate) fn apply(&self, r: &[f64], z: &mut [f64]) {
        debug_assert_eq!(r.len(), self.edge_dim);
        debug_assert_eq!(z.len(), self.edge_dim);
        // z = D⁻¹ r
        self.jacobi_smooth_weighted(r, z, 1.0);
        // z += G C⁻¹ Gᵀ r
        let mut coarse = vec![0.0_f64; self.edge_dim];
        self.coarse_correction(r, &mut coarse);
        for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
            *zi += ci;
        }
        // z += Π (ΠᵀAΠ)⁻¹ Πᵀ r  (vector-nodal space, full three-space cycle).
        if self.pi.is_some() {
            self.coarse_correction_pi(r, &mut coarse);
            for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                *zi += ci;
            }
        }
    }

    /// Apply the AMS-lite preconditioner as a **symmetric two-level V-cycle**
    /// `z = M_prec⁻¹ r`, given a closure `op_apply(x, y) ⇒ y = A x` for the
    /// shifted operator `A = K − σM` (the same matrix-free apply the outer CG
    /// uses).
    ///
    /// The cycle is
    ///
    /// ```text
    /// z  = D⁻¹ r                    (pre-smooth)
    /// r₁ = r − A z                  (residual)
    /// z += G C⁻¹ Gᵀ r₁             (gradient-space coarse correction)
    /// r₂ = r − A z                  (residual)
    /// z += D⁻¹ r₂                   (post-smooth)
    /// ```
    ///
    /// The symmetric pre-/post-smooth around the (self-adjoint) coarse
    /// correction makes the whole cycle **SPD** — a symmetric multigrid V-cycle
    /// with a symmetric (Jacobi) smoother and a symmetric coarse solve is a
    /// valid CG preconditioner. The multiplicative cycle is substantially
    /// stronger than the additive `D⁻¹ + G C⁻¹ Gᵀ` form (each correction sees
    /// the residual *after* the previous stage), which is what delivers the
    /// large inner-CG iteration reduction. `r` and `z` are length `edge_dim`.
    pub(crate) fn apply_vcycle<F>(&self, r: &[f64], z: &mut [f64], mut op_apply: F)
    where
        F: FnMut(&[f64], &mut [f64]),
    {
        debug_assert_eq!(r.len(), self.edge_dim);
        debug_assert_eq!(z.len(), self.edge_dim);
        let n = self.edge_dim;

        // Pre-smooth: z = ω D⁻¹ r.
        self.jacobi_smooth_weighted(r, z, self.smooth_weight);

        // Coarse correction on the post-pre-smooth residual r₁ = r − A z.
        let mut az = vec![0.0_f64; n];
        op_apply(z, &mut az);
        let mut resid: Vec<f64> = r.iter().zip(az.iter()).map(|(ri, ai)| ri - ai).collect();
        let mut coarse = vec![0.0_f64; n];
        self.coarse_correction(&resid, &mut coarse);
        for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
            *zi += ci;
        }
        // Vector-nodal coarse correction on the SAME residual r₁ (issue #550):
        // the two auxiliary corrections are combined additively (the original
        // Hiptmair–Xu splitting), so the middle stage is (C_G + C_Π) applied to
        // r₁. Summing two SPD subspace corrections keeps the middle stage SPD,
        // and the symmetric pre-/post-smooth around it keeps the whole cycle a
        // valid (SPD) CG preconditioner.
        if self.pi.is_some() {
            self.coarse_correction_pi(&resid, &mut coarse);
            for (zi, &ci) in z.iter_mut().zip(coarse.iter()) {
                *zi += ci;
            }
        }

        // Post-smooth on the residual r₂ = r − A z.
        op_apply(z, &mut az);
        for i in 0..n {
            resid[i] = r[i] - az[i];
        }
        let mut post = vec![0.0_f64; n];
        self.jacobi_smooth_weighted(&resid, &mut post, self.smooth_weight);
        for (zi, &pi) in z.iter_mut().zip(post.iter()) {
            *zi += pi;
        }
    }
}

/// Assemble the vector-nodal interpolation `Π` (`edge_dim × 3·node_dim`) of
/// the full Hiptmair–Xu AMS (issue #550) from `G`'s row incidence and the
/// per-edge geometry.
///
/// For lowest-order Nédélec elements the edge DOF is the tangential line
/// integral `∫_e v·t`. Interpolating a P1 nodal **vector** field
/// `V = Σ_c φ_c V_c` onto edge `e = (i, j)` gives the coefficient
/// `(1/2)(V_i + V_j)·(p_j − p_i)`: each endpoint contributes the same weight
/// `(1/2) d_e` per Cartesian component, where `d_e = p_j − p_i` is the edge
/// vector. So `Π` has exactly `G`'s node-column incidence (read from
/// `g_rows`), and for every free-node column `c` that edge `e` touches and
/// component `α ∈ {0,1,2}`, `Π[e, 3c+α] = (1/2) d_e[α]`. The column layout
/// interleaves the three Cartesian components per node (`3c+α`).
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if faer rejects the `Π` triplets.
fn build_pi(
    g_rows: &[Vec<(usize, f64)>],
    edge_vectors: &[[f64; 3]],
    edge_dim: usize,
    node_dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    assert_eq!(g_rows.len(), edge_dim, "g_rows length must equal edge_dim");
    assert_eq!(
        edge_vectors.len(),
        edge_dim,
        "edge_vectors length must equal edge_dim"
    );
    let pi_dim = 3 * node_dim;
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(6 * edge_dim);
    for (e, row) in g_rows.iter().enumerate() {
        let d = edge_vectors[e];
        for &(c, _sign) in row {
            for (alpha, &da) in d.iter().enumerate() {
                let v = 0.5 * da;
                if v != 0.0 {
                    trips.push(Triplet::new(e, 3 * c + alpha, v));
                }
            }
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, pi_dim, &trips)
        .map_err(|err| EigenError::FaerGevd(format!("Π assembly: {err:?}")))
}

/// Form the regularized vector-nodal coarse operator `Πᵀ (K − σM) Π + τI`
/// (issue #550). The outer-product assembly reuses [`accumulate_gtag`] with
/// `Π`'s ≤6-entry rows in place of `G`'s ≤2-entry rows. The caller wires the
/// returned SPD operator into a [`CoarseSolver`] (few-sweep SGS by default).
///
/// A tiny relative Tikhonov shift `τ = 1e-8 · max|diag|` is added to the
/// diagonal. `Π` can be rank-deficient on a coarse mesh (e.g. `3·node_dim >
/// edge_dim`, or colinear edges), which would make the exact `Πᵀ A Π` singular
/// (its LU would fail, and its Gauss–Seidel diagonal could vanish); the shift
/// restores SPD invertibility so both coarse solvers are well-posed. Because
/// `Π (ΠᵀAΠ + τI)⁻¹ Πᵀ` is still symmetric positive semidefinite, the
/// preconditioner stays SPD, and `τ` is negligible against the operator so the
/// correction is essentially unchanged.
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if the coarse assembly fails.
fn build_pi_ata(
    pi: SparseColMatRef<'_, usize, f64>,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    node_dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let pi_dim = 3 * node_dim;
    let edge_dim = pi.nrows();

    // Row view of Π: pi_rows[e] = list of (col, value) (≤6 entries).
    let mut pi_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
    {
        let col_ptr = pi.col_ptr();
        let row_idx = pi.row_idx();
        let val = pi.val();
        for col in 0..pi.ncols() {
            for kk in col_ptr[col]..col_ptr[col + 1] {
                pi_rows[row_idx[kk]].push((col, val[kk]));
            }
        }
    }

    // Πᵀ A Π = Σ_{A[i,j]=v} v · πᵢ πⱼᵀ, A = K − σM (same folded-shift stream
    // as C = Gᵀ A G).
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
    accumulate_gtag(&pi_rows, k, 1.0, &mut trips);
    if sigma != 0.0 {
        accumulate_gtag(&pi_rows, m, -sigma, &mut trips);
    }

    // Tikhonov guard: assemble once to read the diagonal magnitude, then add
    // τ·I so the coarse solve is well-posed even when Π is rank-deficient.
    let ata0 = SparseColMat::<usize, f64>::try_new_from_triplets(pi_dim, pi_dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("ΠᵀAΠ assembly: {e:?}")))?;
    let mut diag = vec![0.0_f64; pi_dim];
    csc_diagonal(ata0.as_ref(), &mut diag);
    let max_diag = diag.iter().fold(0.0_f64, |a, &d| a.max(d.abs()));
    let tau = if max_diag > 0.0 {
        1e-8 * max_diag
    } else {
        1e-12
    };
    for i in 0..pi_dim {
        trips.push(Triplet::new(i, i, tau));
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(pi_dim, pi_dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("ΠᵀAΠ assembly (regularized): {e:?}")))
}

/// Accumulate the triplets of `scale · Rᵀ A R` for a single operator `A`
/// (given in CSC) into `trips`, where `R` is a sparse restriction whose row
/// `i` is `rows[i]` (a list of `(col, weight)` — `G`'s ≤2-entry rows for the
/// gradient space, `Π`'s ≤6-entry rows for the vector-nodal space). Every
/// nonzero `A[i,j] = v` contributes the `|rows[i]|·|rows[j]|` coarse-indexed
/// triplets of `scale · v · rᵢ rⱼᵀ`, deduplicated later by faer.
fn accumulate_gtag(
    rows: &[Vec<(usize, f64)>],
    a: SparseColMatRef<'_, usize, f64>,
    scale: f64,
    trips: &mut Vec<Triplet<usize, usize, f64>>,
) {
    let cp = a.col_ptr();
    let ri = a.row_idx();
    let val = a.val();
    for j in 0..a.ncols() {
        let gj = &rows[j];
        if gj.is_empty() {
            continue;
        }
        for k in cp[j]..cp[j + 1] {
            let i = ri[k];
            let v = val[k] * scale;
            if v == 0.0 {
                continue;
            }
            let gi = &rows[i];
            if gi.is_empty() {
                continue;
            }
            for &(p, sp) in gi {
                let vsp = v * sp;
                for &(q, sq) in gj {
                    trips.push(Triplet::new(p, q, vsp * sq));
                }
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use faer::sparse::{SparseColMat, Triplet};

    /// Build the 1-D Laplacian pencil `K = tridiag(-1, 2, -1)`, `M = I`.
    fn laplacian(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
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
        (
            SparseColMat::try_new_from_triplets(n, n, &tk).unwrap(),
            SparseColMat::try_new_from_triplets(n, n, &tm).unwrap(),
        )
    }

    /// A synthetic discrete gradient `G` for an `edge_dim`-row pencil, with the
    /// ±1 incidence structure of `d⁰`: a 1-D chain of `edge_dim + 1` edges over
    /// `edge_dim + 2` nodes, whose two end edges are PEC-excluded so their
    /// endpoint nodes are grounded — mirroring the real interior mask. The two
    /// grounded end nodes remove the constant nullspace of a fully-free chain
    /// (`G·1 = 0`), so `Gᵀ A G` is SPD and full-column-rank, exactly as on a
    /// boundary-touching mesh. The kept interior edges reindex to `0..edge_dim`.
    ///
    /// Returns an [`InteriorGradient`] with `edge_dim` rows and a positive
    /// free-node column count (`< edge_dim`) — full column rank, so `Gᵀ A G`
    /// is SPD.
    fn chain_gradient(edge_dim: usize) -> InteriorGradient {
        // Total chain: `edge_dim + 2` edges e=[e, e+1] over `edge_dim + 3`
        // nodes. Exclude the first and last edge (PEC), keeping the middle
        // `edge_dim` edges as the reduced rows.
        let total_edges = edge_dim + 2;
        let n_nodes = edge_dim + 3;
        let edges: Vec<[u32; 2]> = (0..total_edges)
            .map(|e| [e as u32, (e + 1) as u32])
            .collect();
        let mut interior_mask = vec![true; total_edges];
        interior_mask[0] = false;
        interior_mask[total_edges - 1] = false;
        let mut edge_index = vec![None; total_edges];
        let mut row = 0usize;
        for (e, keep) in interior_mask.iter().enumerate() {
            if *keep {
                edge_index[e] = Some(row);
                row += 1;
            }
        }
        assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
        InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
    }

    /// The AMS-lite apply is symmetric positive definite: `zᵀ r > 0` for
    /// `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. CG requires an SPD
    /// preconditioner, so this is the correctness gate for using AMS-lite at all.
    #[test]
    fn ams_lite_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        assert_eq!(ams.edge_dim, n);
        assert!(
            ams.node_dim > 0 && ams.node_dim < n,
            "unexpected node_dim {} for edge_dim {n}",
            ams.node_dim
        );

        // A = K − σM apply (the operator the V-cycle needs for its residuals).
        let a_apply = |x: &[f64], y: &mut [f64]| {
            let mut kx = vec![0.0; n];
            let mut mx = vec![0.0; n];
            spmv(k.as_ref(), x, &mut kx);
            spmv(m.as_ref(), x, &mut mx);
            for i in 0..n {
                y[i] = kx[i] - sigma * mx[i];
            }
        };

        // Positive-definiteness: rᵀ (M_prec r) > 0 for several random-ish r.
        for seed in 0..5 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut z = vec![0.0; n];
            ams.apply_vcycle(&r, &mut z, a_apply);
            let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
            assert!(rz > 0.0, "AMS-lite not positive definite: rᵀz = {rz}");
        }

        // Symmetry: uᵀ M_prec v == vᵀ M_prec u.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        ams.apply_vcycle(&u, &mut mu, a_apply);
        ams.apply_vcycle(&v, &mut mv, a_apply);
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
            "AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
        );
    }

    /// The **additive** reference form `z = D⁻¹ r + G C⁻¹ Gᵀ r` is also SPD:
    /// `rᵀ z > 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`. It is a sum of two SPD
    /// operators, so this is expected; the test pins it (and exercises the
    /// matvec-free apply path used as the documented fallback).
    #[test]
    fn ams_lite_additive_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5;
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();

        for seed in 0..5 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut z = vec![0.0; n];
            ams.apply(&r, &mut z);
            let rz: f64 = r.iter().zip(z.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz > 0.0,
                "additive AMS-lite not positive definite: rᵀz = {rz}"
            );
        }

        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut mu = vec![0.0; n];
        let mut mv = vec![0.0; n];
        ams.apply(&u, &mut mu);
        ams.apply(&v, &mut mv);
        let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
        let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
            "additive AMS-lite not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
        );
    }

    /// The gradient-space coarse correction is exact on `image(G)`: for a pure
    /// gradient error `e = G y`, applying the coarse term of the preconditioner
    /// to `A e` recovers `e` (up to the Jacobi smoother contribution). This is
    /// the mechanism by which AMS damps the near-kernel Jacobi cannot see —
    /// here we check the coarse solve inverts `Gᵀ A G` exactly. Built with the
    /// **direct** coarse solver so the recovery is exact (the assembly of
    /// `C = Gᵀ A G` is what this pins; the approximate SGS solve is exercised by
    /// [`sgs_coarse_solve_is_spd_and_reduces_residual`]).
    #[test]
    fn coarse_correction_inverts_on_gradient_space() {
        let n = 10;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -1.0;
        let ams = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();

        // Coarse operator C = Gᵀ A G applied to yc, then solved back, must be
        // identity in node space.
        let node_dim = ams.node_dim;
        let yc: Vec<f64> = (0..node_dim).map(|i| ((i as f64) * 0.5).cos()).collect();
        // g_yc = G yc (edge)
        let mut g_yc = vec![0.0; n];
        spmv(ams.gradient.matrix(), &yc, &mut g_yc);
        // a_g_yc = A g_yc (edge), A = K − σM
        let mut kg = vec![0.0; n];
        let mut mg = vec![0.0; n];
        spmv(k.as_ref(), &g_yc, &mut kg);
        spmv(m.as_ref(), &g_yc, &mut mg);
        let a_g_yc: Vec<f64> = kg
            .iter()
            .zip(mg.iter())
            .map(|(ki, mi)| ki - sigma * mi)
            .collect();
        // rc = Gᵀ A g_yc (node)
        let mut rc = vec![0.0; node_dim];
        spmv_transpose(ams.gradient.matrix(), &a_g_yc, &mut rc);
        // solve C yc' = rc; yc' must equal yc (exact under the direct solver).
        let mut got = vec![0.0; node_dim];
        ams.c_coarse.solve(&rc, &mut got);
        for (i, want) in yc.iter().enumerate() {
            assert!(
                (got[i] - want).abs() < 1e-9,
                "coarse solve wrong at {i}: got {}, want {want}",
                got[i]
            );
        }
    }

    /// The same synthetic chain as [`chain_gradient`], but with 3-D **node
    /// geometry** attached so the full-AMS vector-nodal interpolation `Π` is
    /// built (issue #550). Node `i` is placed on a helix
    /// `p_i = (i, sin 0.7i, cos 0.5i)`, so consecutive edge vectors span all
    /// three Cartesian directions — `Π` is non-degenerate (its `x/y/z` blocks
    /// are all populated), exercising the real three-space code path rather
    /// than a colinear special case.
    fn chain_gradient_geom(edge_dim: usize) -> InteriorGradient {
        let total_edges = edge_dim + 2;
        let n_nodes = edge_dim + 3;
        let coords: Vec<[f64; 3]> = (0..n_nodes)
            .map(|i| {
                let f = i as f64;
                [f, (0.7 * f).sin(), (0.5 * f).cos()]
            })
            .collect();
        let edges: Vec<[u32; 2]> = (0..total_edges)
            .map(|e| [e as u32, (e + 1) as u32])
            .collect();
        let mut interior_mask = vec![true; total_edges];
        interior_mask[0] = false;
        interior_mask[total_edges - 1] = false;
        let mut edge_index = vec![None; total_edges];
        let mut edge_vectors = vec![[0.0_f64; 3]; edge_dim];
        let mut row = 0usize;
        for (e, keep) in interior_mask.iter().enumerate() {
            if *keep {
                edge_index[e] = Some(row);
                let [a, b] = edges[e];
                let (pa, pb) = (coords[a as usize], coords[b as usize]);
                edge_vectors[row] = [pb[0] - pa[0], pb[1] - pa[1], pb[2] - pa[2]];
                row += 1;
            }
        }
        assert_eq!(row, edge_dim, "kept edge count must equal edge_dim");
        InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim)
            .with_edge_vectors(edge_vectors)
    }

    /// Without attached geometry the preconditioner is the gradient-only
    /// two-space cycle (no `Π`); with geometry it is the full three-space
    /// cycle. This pins the switch [`AmsLitePreconditioner::build`] keys off.
    #[test]
    fn vector_nodal_space_present_iff_geometry_supplied() {
        let n = 10;
        let (k, m) = laplacian(n);
        let sigma = -0.5;

        let ams_grad_only =
            AmsLitePreconditioner::build(&chain_gradient(n), k.as_ref(), m.as_ref(), sigma)
                .unwrap();
        assert!(
            !ams_grad_only.has_vector_nodal_space(),
            "gradient-only build must not carry Π"
        );

        let ams_three =
            AmsLitePreconditioner::build(&chain_gradient_geom(n), k.as_ref(), m.as_ref(), sigma)
                .unwrap();
        assert!(
            ams_three.has_vector_nodal_space(),
            "three-space build must carry Π"
        );
        // Π has edge_dim rows and 3·node_dim columns.
        let pi = ams_three.pi.as_ref().unwrap();
        assert_eq!(pi.nrows(), n, "Π rows must equal edge_dim");
        assert_eq!(
            pi.ncols(),
            3 * ams_three.node_dim,
            "Π cols must equal 3·node_dim"
        );
    }

    /// ACCEPTANCE (issue #550): the **full three-space** AMS apply is SPD in
    /// BOTH modes — `zᵀ r > 0` for `r ≠ 0` and `⟨M_prec u, v⟩ = ⟨u, M_prec v⟩`.
    /// Adding the vector-nodal `Π (ΠᵀAΠ)⁻¹ Πᵀ` correction (a symmetric PSD
    /// subspace solve) to the existing SPD cycle must keep the preconditioner
    /// SPD, or it is not a valid CG preconditioner.
    #[test]
    fn full_three_space_apply_is_spd() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
        assert!(ams.has_vector_nodal_space());

        let a_apply = |x: &[f64], y: &mut [f64]| {
            let mut kx = vec![0.0; n];
            let mut mx = vec![0.0; n];
            spmv(k.as_ref(), x, &mut kx);
            spmv(m.as_ref(), x, &mut mx);
            for i in 0..n {
                y[i] = kx[i] - sigma * mx[i];
            }
        };

        // Positive-definiteness of BOTH apply modes on several residuals.
        for seed in 0..6 {
            let r: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();

            let mut z_v = vec![0.0; n];
            ams.apply_vcycle(&r, &mut z_v, a_apply);
            let rz_v: f64 = r.iter().zip(z_v.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz_v > 0.0,
                "three-space V-cycle not positive definite: rᵀz = {rz_v}"
            );

            let mut z_a = vec![0.0; n];
            ams.apply(&r, &mut z_a);
            let rz_a: f64 = r.iter().zip(z_a.iter()).map(|(a, b)| a * b).sum();
            assert!(
                rz_a > 0.0,
                "three-space additive apply not positive definite: rᵀz = {rz_a}"
            );
        }

        // Symmetry of BOTH modes: uᵀ M_prec v == vᵀ M_prec u.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos()).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        for mode in ["vcycle", "additive"] {
            let mut mu = vec![0.0; n];
            let mut mv = vec![0.0; n];
            if mode == "vcycle" {
                ams.apply_vcycle(&u, &mut mu, a_apply);
                ams.apply_vcycle(&v, &mut mv, a_apply);
            } else {
                ams.apply(&u, &mut mu);
                ams.apply(&v, &mut mv);
            }
            let umv: f64 = u.iter().zip(mv.iter()).map(|(a, b)| a * b).sum();
            let vmu: f64 = v.iter().zip(mu.iter()).map(|(a, b)| a * b).sum();
            assert!(
                (umv - vmu).abs() < 1e-10 * (umv.abs() + 1.0),
                "three-space {mode} not symmetric: uᵀM_prec v = {umv}, vᵀM_prec u = {vmu}"
            );
        }
    }

    /// The vector-nodal coarse solve inverts `Πᵀ A Π` on the vector-nodal
    /// space up to the tiny Tikhonov regularization: for a coarse vector `yc`,
    /// restricting `A Π yc` back through `Πᵀ` and solving the cached factor
    /// recovers `yc`. This pins that `Π` and its coarse operator are assembled
    /// consistently (the vector-nodal analogue of
    /// `coarse_correction_inverts_on_gradient_space`).
    #[test]
    fn pi_coarse_solve_inverts_on_vector_nodal_space() {
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -1.0;
        // Direct coarse solver so the recovery is exact — this test pins the Π /
        // ΠᵀAΠ assembly, not the approximate SGS solve.
        let ams = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();
        let pi = ams.pi.as_ref().unwrap();
        let pi_dim = 3 * ams.node_dim;

        let yc: Vec<f64> = (0..pi_dim)
            .map(|i| ((i as f64) * 0.37).sin() + 0.1)
            .collect();
        // pyc = Π yc (edge)
        let mut pyc = vec![0.0; n];
        spmv(pi.as_ref(), &yc, &mut pyc);
        // a_pyc = A pyc = (K − σM) pyc
        let mut kg = vec![0.0; n];
        let mut mg = vec![0.0; n];
        spmv(k.as_ref(), &pyc, &mut kg);
        spmv(m.as_ref(), &pyc, &mut mg);
        let a_pyc: Vec<f64> = kg
            .iter()
            .zip(mg.iter())
            .map(|(a, b)| a - sigma * b)
            .collect();
        // rc = Πᵀ a_pyc (coarse)
        let mut rc = vec![0.0; pi_dim];
        spmv_transpose(pi.as_ref(), &a_pyc, &mut rc);
        // solve (ΠᵀAΠ + τI) yc' = rc; with τ ≈ 1e-8·max|diag| the recovered
        // yc' matches yc to a loose tolerance on the populated directions.
        let mut ycp = vec![0.0; pi_dim];
        ams.pi_coarse.as_ref().unwrap().solve(&rc, &mut ycp);
        // Compare in the A-energy-agnostic sense: Π yc' ≈ Π yc (the physical
        // edge-space correction is what matters; the coarse coordinates can
        // differ in any Π-nullspace direction the regularization pins to ~0).
        let mut pycp = vec![0.0; n];
        spmv(pi.as_ref(), &ycp, &mut pycp);
        let num: f64 = pyc
            .iter()
            .zip(pycp.iter())
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
        let den: f64 = pyc.iter().map(|a| a * a).sum::<f64>().max(1e-30);
        assert!(
            (num / den).sqrt() < 1e-4,
            "Π coarse solve did not reproduce the edge-space correction: rel = {:.2e}",
            (num / den).sqrt()
        );
    }

    /// `y = (K − σM) x` for the shifted edge pencil (test helper).
    fn shifted_apply(
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        x: &[f64],
        y: &mut [f64],
    ) {
        let n = x.len();
        let mut kx = vec![0.0; n];
        let mut mx = vec![0.0; n];
        spmv(k, x, &mut kx);
        spmv(m, x, &mut mx);
        for i in 0..n {
            y[i] = kx[i] - sigma * mx[i];
        }
    }

    /// Preconditioned CG solving `(K − σM) x = b` with the AMS V-cycle as the
    /// preconditioner, returning the iteration count to reach the relative
    /// residual `tol` (or `max_it` if it stalls). Used by the measurement report
    /// to compare the direct vs SGS coarse solver apples-to-apples.
    fn pcg_ams_iters(
        ams: &AmsLitePreconditioner,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
        b: &[f64],
        tol: f64,
        max_it: usize,
    ) -> usize {
        let n = b.len();
        let dot = |a: &[f64], c: &[f64]| a.iter().zip(c.iter()).map(|(x, y)| x * y).sum::<f64>();
        let bnorm = dot(b, b).sqrt().max(1e-300);

        let mut x = vec![0.0; n];
        let mut r = b.to_vec(); // r = b − A·0
        let mut z = vec![0.0; n];
        ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
        let mut p = z.clone();
        let mut rz = dot(&r, &z);
        let mut ap = vec![0.0; n];
        for it in 1..=max_it {
            shifted_apply(k, m, sigma, &p, &mut ap);
            let denom = dot(&p, &ap);
            if denom.abs() < 1e-300 {
                return it;
            }
            let alpha = rz / denom;
            for i in 0..n {
                x[i] += alpha * p[i];
                r[i] -= alpha * ap[i];
            }
            if dot(&r, &r).sqrt() <= tol * bnorm {
                return it;
            }
            ams.apply_vcycle(&r, &mut z, |u, v| shifted_apply(k, m, sigma, u, v));
            let rz_new = dot(&r, &z);
            let beta = rz_new / rz;
            for i in 0..n {
                p[i] = z[i] + beta * p[i];
            }
            rz = rz_new;
        }
        max_it
    }

    /// The few-sweep symmetric Gauss–Seidel coarse solver ([`SgsCoarseSolver`])
    /// is itself an SPD operator (`b ↦ y ≈ A⁻¹ b`): symmetric and positive
    /// definite on an SPD coarse operator, and it strictly reduces the residual.
    /// These are the two properties the enclosing V-cycle relies on to stay a
    /// valid CG preconditioner (issue #551).
    #[test]
    fn sgs_coarse_solve_is_spd_and_reduces_residual() {
        // K = tridiag(-1, 2, -1) is SPD — a stand-in coarse operator.
        let n = 40;
        let (k, _m) = laplacian(n);
        let sgs = SgsCoarseSolver::from_csc(k.as_ref(), 2);

        // Symmetry: ⟨B u, v⟩ == ⟨u, B v⟩ for the approximate-inverse operator B.
        let u: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.9).cos() + 0.2).collect();
        let v: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.4 + 1.0).sin()).collect();
        let mut bu = vec![0.0; n];
        let mut bv = vec![0.0; n];
        sgs.solve(&u, &mut bu);
        sgs.solve(&v, &mut bv);
        let ubv: f64 = u.iter().zip(bv.iter()).map(|(a, b)| a * b).sum();
        let vbu: f64 = v.iter().zip(bu.iter()).map(|(a, b)| a * b).sum();
        assert!(
            (ubv - vbu).abs() < 1e-10 * (ubv.abs() + 1.0),
            "SGS coarse solve not symmetric: ⟨Bu,v⟩={ubv}, ⟨u,Bv⟩={vbu}"
        );

        // Positive definiteness + residual reduction on several right-hand sides.
        for seed in 0..5 {
            let b: Vec<f64> = (0..n)
                .map(|i| (((i + seed) as f64) * 0.7).sin() + 0.3)
                .collect();
            let mut y = vec![0.0; n];
            sgs.solve(&b, &mut y);
            let by: f64 = b.iter().zip(y.iter()).map(|(a, c)| a * c).sum();
            assert!(by > 0.0, "SGS coarse solve not positive definite: bᵀy={by}");

            // Residual ‖b − K y‖ strictly below ‖b‖ (the sweep makes progress).
            let mut ky = vec![0.0; n];
            sgs.spmv(&y, &mut ky);
            let resid: f64 = b
                .iter()
                .zip(ky.iter())
                .map(|(a, c)| (a - c) * (a - c))
                .sum::<f64>()
                .sqrt();
            let bnorm: f64 = b.iter().map(|a| a * a).sum::<f64>().sqrt();
            assert!(
                resid < bnorm,
                "SGS coarse solve did not reduce residual: ‖b−Ky‖={resid}, ‖b‖={bnorm}"
            );
        }

        // More sweeps ⇒ smaller residual (monotone convergence of the smoother).
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.3).cos()).collect();
        let residual = |sweeps: usize| -> f64 {
            let s = SgsCoarseSolver::from_csc(k.as_ref(), sweeps);
            let mut y = vec![0.0; n];
            s.solve(&b, &mut y);
            let mut ky = vec![0.0; n];
            s.spmv(&y, &mut ky);
            b.iter()
                .zip(ky.iter())
                .map(|(a, c)| (a - c) * (a - c))
                .sum::<f64>()
                .sqrt()
        };
        assert!(
            residual(4) < residual(1),
            "SGS residual did not decrease with more sweeps"
        );
    }

    /// MEASUREMENT (issue #551, `--nocapture`): report the per-apply coarse-
    /// correction wall-clock and the outer preconditioned-CG iteration count for
    /// the **direct** LU coarse factor vs the **few-sweep SGS** coarse solve,
    /// apples-to-apples on an SPD shifted pencil (`σ` below the spectrum).
    ///
    /// NOTE on this fixture: the 1-D chain `laplacian` is a *best case for the
    /// direct factor* — its coarse operator `C = Gᵀ A G` is essentially the whole
    /// 1-D problem, so the exact coarse solve converges the outer CG in a couple
    /// of iterations while the approximate SGS solve needs more. This overstates
    /// the iteration trade-off; the **representative** iteration comparison on
    /// the genuine 3-D Nédélec curl-curl pencil lives in
    /// `tests/transmon_eigenmode.rs::synthetic_ams_beats_jacobi_inner_iterations`,
    /// where the SGS coarse solve gives the **same** 5.35× inner-CG reduction the
    /// direct factor did (the physical `transmon_smoke.msh` at σ=4.5 GHz is an
    /// indefinite pencil where inner-CG cannot run — the deferred 1c point). What
    /// this test pins locally is the structural win: the SGS apply is markedly
    /// cheaper per call and needs no global factor, and the outer CG still
    /// converges. The honest numbers are printed for the PR body.
    #[test]
    fn coarse_solve_cost_and_iteration_report() {
        let n = 600;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -0.5; // below the spectrum ⇒ A = K − σM SPD

        let ams_direct = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::Direct,
        )
        .unwrap();
        let ams_sgs = AmsLitePreconditioner::build_with_coarse(
            &g,
            k.as_ref(),
            m.as_ref(),
            sigma,
            CoarseSolve::SymmetricGaussSeidel(DEFAULT_COARSE_SWEEPS),
        )
        .unwrap();

        // Per-apply coarse-correction wall-clock (average over many reps).
        let reps = 2000;
        let residual: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.11).sin() + 0.05).collect();
        let mut out = vec![0.0; n];

        let bench = |ams: &AmsLitePreconditioner, out: &mut [f64]| -> f64 {
            // warm up
            ams.coarse_correction(&residual, out);
            let t0 = std::time::Instant::now();
            for _ in 0..reps {
                ams.coarse_correction(&residual, out);
            }
            t0.elapsed().as_secs_f64() / reps as f64 * 1e6 // µs per apply
        };
        let us_direct = bench(&ams_direct, &mut out);
        let us_sgs = bench(&ams_sgs, &mut out);

        // Outer PCG iteration count with each coarse solver (same rhs, tol).
        let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.37).cos() + 0.1).collect();
        let tol = 1e-8;
        let max_it = 4 * n;
        let it_direct = pcg_ams_iters(&ams_direct, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);
        let it_sgs = pcg_ams_iters(&ams_sgs, k.as_ref(), m.as_ref(), sigma, &b, tol, max_it);

        eprintln!(
            "\n=== #551 coarse-solve report (laplacian n={n}, σ={sigma}, sweeps={DEFAULT_COARSE_SWEEPS}) ===\n\
             per-apply coarse correction: direct LU = {us_direct:.3} µs, SGS = {us_sgs:.3} µs \
             ({:.2}× direct)\n\
             outer PCG iterations (tol {tol:.0e}): direct = {it_direct}, SGS = {it_sgs}\n",
            us_sgs / us_direct.max(1e-12)
        );

        // Structural guarantee: the approximate SGS coarse solve keeps the outer
        // CG converging (a valid SPD preconditioner) and is markedly cheaper per
        // apply. We do NOT assert a tight iteration ratio here — on this 1-D
        // best-case chain the exact factor is unbeatable; the representative
        // ratio is the transmon-fixture test cited in the doc comment.
        assert!(it_direct > 0, "direct PCG performed no iterations");
        assert!(
            it_sgs > 0 && it_sgs < max_it,
            "SGS-preconditioned PCG failed to converge: it = {it_sgs} (max {max_it})"
        );
        assert!(
            us_sgs < us_direct,
            "SGS coarse apply was not cheaper than the direct factor: \
             SGS = {us_sgs:.3} µs, direct = {us_direct:.3} µs"
        );
    }
}
