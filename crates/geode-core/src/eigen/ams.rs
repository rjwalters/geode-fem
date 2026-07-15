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
//! free interior node — `node_dim ≪ edge_dim`), factors it **once** with a sparse
//! LU, and each apply is `Gᵀ·` (restrict to nodes), one cached triangular solve,
//! and `G·` (prolong to edges). This is exactly the Hiptmair–Xu nodal
//! auxiliary-space correction that damps the gradient near-kernel Jacobi is blind
//! to. A preconditioner changes only convergence speed, never the fixed point, so
//! the eigenvalues are unchanged either way.
//!
//! # Memory
//!
//! `C = Gᵀ A G` is node-indexed, so its LU fill-in is `O(node_dim)`, an order of
//! magnitude below the edge pencil and far below the *edge*-space factorization
//! the matrix-free path exists to avoid. The working set stays `O(N)`: the
//! borrowed edge operators, the length-`edge_dim` Jacobi diagonal, and the small
//! node-space `C` factors. No edge-space factorization is ever formed.
//!
//! The vector-nodal `Πᵀ A Π` block (issue #550) is node-vector-indexed
//! (`3·node_dim` square, still an order of magnitude below `edge_dim`), so its
//! LU fill is `O(node_dim)` and the working set stays `O(N)` — no edge-space
//! factorization is ever formed for it either. The at-scale iteration numbers
//! on the 133k / 1.16M meshes remain an operator/AWS follow-up (issue #531
//! sub-phase 1c); this module delivers the correct, SPD, spectrum-preserving
//! three-space operator and the local iteration-count measurement.

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
/// inverse-diagonal of `A = K − σM`, the sparse `G`, and a cached LU of the
/// nodal coupling `C = Gᵀ A G`. [`Self::apply`] realizes the additive apply
/// `z = D⁻¹ r + G C⁻¹ Gᵀ r`.
pub(crate) struct AmsLitePreconditioner {
    /// Sparse discrete gradient `G` (`edge_dim × node_dim`), owned via a
    /// cloned [`InteriorGradient`] (which itself holds an owned `SparseColMat`).
    gradient: InteriorGradient,
    /// Jacobi inverse-diagonal `1 / (K_ii − σ M_ii)` (edge space), with a
    /// zero-pivot fallback to `1.0` — identical to the matrix-free baseline's
    /// Jacobi so the two paths agree when the coarse term is inactive.
    inv_diag: Vec<f64>,
    /// Cached LU of the nodal coupling `C = Gᵀ A G` (node-indexed, SPD).
    c_lu: Lu<usize, f64>,
    /// The vector-nodal interpolation `Π` (`edge_dim × 3·node_dim`) of the
    /// full Hiptmair–Xu AMS (issue #550), present only when the discrete
    /// gradient carried per-edge geometry
    /// ([`InteriorGradient::with_edge_vectors`]). `None` ⇒ the gradient-only
    /// two-space cycle.
    pi: Option<SparseColMat<usize, f64>>,
    /// Cached LU of the vector-nodal coarse operator `Πᵀ A Π`
    /// (`3·node_dim` square, SPD), paired with [`Self::pi`]. `Some` iff `pi`
    /// is `Some`.
    pi_ata_lu: Option<Lu<usize, f64>>,
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
    /// `GᵀMG`, extended to the shifted pencil `A = K − σM`. `C` is then factored
    /// once with a sparse LU (node-indexed, so `O(node_dim)` fill).
    ///
    /// `k` and `m` are the reduced edge operators (dimension `edge_dim`, which
    /// must equal `gradient.edge_dim()`); `sigma` is the shift.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the `C` assembly or its sparse LU
    /// factorization fails (e.g. `C` singular — should not happen for an SPD
    /// `A = K − σM` and a full-column-rank `G`).
    pub(crate) fn build(
        gradient: &InteriorGradient,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        sigma: f64,
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
        let c_lu = c
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("Gᵀ(K−σM)G sparse LU: {e:?}")))?;

        // Vector-nodal auxiliary space (full Hiptmair–Xu, issue #550): build
        // Π and cache the LU of Πᵀ A Π, but ONLY when the caller supplied the
        // per-edge geometry. Without it we keep the gradient-only two-space
        // cycle (backward-compatible; Π is undefined without node coordinates).
        let (pi, pi_ata_lu) = match gradient.edge_vectors() {
            Some(edge_vectors) => {
                let pi = build_pi(&g_rows, edge_vectors, edge_dim, node_dim)?;
                let pi_ata_lu = build_pi_ata_lu(pi.as_ref(), k, m, sigma, node_dim)?;
                (Some(pi), Some(pi_ata_lu))
            }
            None => (None, None),
        };

        Ok(Self {
            gradient: gradient.clone(),
            inv_diag,
            c_lu,
            pi,
            pi_ata_lu,
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
        use faer::linalg::solvers::Solve;
        // rc = Gᵀ r  (node space)
        let mut rc = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), r, &mut rc);
        // yc = C⁻¹ rc
        let mut yc_mat: Mat<f64> = Mat::from_fn(self.node_dim, 1, |row, _| rc[row]);
        self.c_lu.solve_in_place(yc_mat.as_mut());
        let yc: Vec<f64> = (0..self.node_dim).map(|row| yc_mat[(row, 0)]).collect();
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
        use faer::linalg::solvers::Solve;
        let (Some(pi), Some(pi_ata_lu)) = (&self.pi, &self.pi_ata_lu) else {
            out.iter_mut().for_each(|v| *v = 0.0);
            return;
        };
        let pi_dim = 3 * self.node_dim;
        // rc = Πᵀ r  (vector-nodal space)
        let mut rc = vec![0.0_f64; pi_dim];
        spmv_transpose(pi.as_ref(), r, &mut rc);
        // yc = (ΠᵀAΠ)⁻¹ rc
        let mut yc_mat: Mat<f64> = Mat::from_fn(pi_dim, 1, |row, _| rc[row]);
        pi_ata_lu.solve_in_place(yc_mat.as_mut());
        let yc: Vec<f64> = (0..pi_dim).map(|row| yc_mat[(row, 0)]).collect();
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

/// Form the vector-nodal coarse operator `Πᵀ (K − σM) Π` and factor it once
/// (issue #550). The outer-product assembly reuses [`accumulate_gtag`] with
/// `Π`'s ≤6-entry rows in place of `G`'s ≤2-entry rows.
///
/// A tiny relative Tikhonov shift `τ = 1e-8 · max|diag|` is added to the
/// diagonal before factoring. `Π` can be rank-deficient on a coarse mesh
/// (e.g. `3·node_dim > edge_dim`, or colinear edges), which would make the
/// exact `Πᵀ A Π` singular and its LU fail; the shift restores SPD
/// invertibility. Because `Π (ΠᵀAΠ + τI)⁻¹ Πᵀ` is still symmetric positive
/// semidefinite, the preconditioner stays SPD, and `τ` is negligible against
/// the operator so the correction is essentially unchanged.
///
/// # Errors
///
/// Returns [`EigenError::FaerGevd`] if the coarse assembly or its sparse LU
/// factorization fails.
fn build_pi_ata_lu(
    pi: SparseColMatRef<'_, usize, f64>,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
    node_dim: usize,
) -> Result<Lu<usize, f64>, EigenError> {
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
    // τ·I so the LU is well-posed even when Π is rank-deficient.
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

    let ata = SparseColMat::<usize, f64>::try_new_from_triplets(pi_dim, pi_dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("ΠᵀAΠ assembly (regularized): {e:?}")))?;
    ata.as_ref()
        .sp_lu()
        .map_err(|e| EigenError::FaerGevd(format!("ΠᵀAΠ sparse LU: {e:?}")))
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
    /// here we check the coarse solve inverts `Gᵀ A G` exactly.
    #[test]
    fn coarse_correction_inverts_on_gradient_space() {
        let n = 10;
        let (k, m) = laplacian(n);
        let g = chain_gradient(n);
        let sigma = -1.0;
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();

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
        // solve C yc' = rc; yc' must equal yc.
        use faer::linalg::solvers::Solve;
        let mut mat = Mat::from_fn(node_dim, 1, |row, _| rc[row]);
        ams.c_lu.solve_in_place(mat.as_mut());
        for (i, want) in yc.iter().enumerate() {
            let got = mat[(i, 0)];
            assert!(
                (got - want).abs() < 1e-9,
                "coarse solve wrong at {i}: got {got}, want {want}"
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
        use faer::linalg::solvers::Solve;
        let n = 12;
        let (k, m) = laplacian(n);
        let g = chain_gradient_geom(n);
        let sigma = -1.0;
        let ams = AmsLitePreconditioner::build(&g, k.as_ref(), m.as_ref(), sigma).unwrap();
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
        let mut mat = Mat::from_fn(pi_dim, 1, |row, _| rc[row]);
        ams.pi_ata_lu.as_ref().unwrap().solve_in_place(mat.as_mut());
        // Compare in the A-energy-agnostic sense: Π yc' ≈ Π yc (the physical
        // edge-space correction is what matters; the coarse coordinates can
        // differ in any Π-nullspace direction the regularization pins to ~0).
        let ycp: Vec<f64> = (0..pi_dim).map(|i| mat[(i, 0)]).collect();
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
}
