//! Divergence-free (discrete-Helmholtz) projection for the Nédélec
//! curl-curl **eigen** path — the spectrum-preserving gauge (issue #509,
//! follow-on to #502 / PR #508).
//!
//! # Why a projection and not DOF elimination
//!
//! The first-order Nédélec curl-curl stiffness `K` has a large kernel
//! equal to the image of the discrete gradient `d⁰`
//! (`kernel(K) = image(d⁰)`, the de-Rham identity). After PEC reduction
//! that kernel has dimension `rank(d⁰_interior)`, one gradient mode per
//! free interior node. In an un-projected shift-invert Lanczos solve these
//! show up as a near-zero-λ cluster (λ ≈ 1e-16…1e-17). (The transmon
//! benchmark's 3.4528 GHz mode is **not** one of them: issue #514 measured
//! it solenoidal, divergence ratio 6.18e-15, so no gradient projection
//! touches it. It is the transmon qubit mode, which Palace confirms below its
//! committed target (issue #1043; `benchmarks/transmon_eigen/results.toml`
//! `[modes.qubit]`). See `docs/research/transmon-lumped-port-formulation.md`.)
//!
//! The tree-cotree **DOF-elimination** gauge ([`crate::eigen::gauge`],
//! PR #508) removes exactly the right *count* of gradient DOFs, but it is
//! **not spectrum-preserving for the generalized eigenproblem**
//! `K x = λ M x`: dropping the tree rows/cols of BOTH `K` and `M` imposes an
//! artificial `x_tree = 0` constraint on the physical eigenvectors (which
//! carry nonzero tree-edge components), shifting the spectrum (measured
//! 1.64% resonator drift, outside the ≤1% bar).
//!
//! The spectrum-preserving construction is an **M-orthogonal projection**
//! onto the divergence-free (solenoidal / cotree) subspace. Let
//! `G = d⁰_interior` be the sparse interior-restricted discrete gradient
//! (interior-edge rows, free-interior-node columns). The
//! `M`-orthogonal projector onto the complement of `image(G)` is
//!
//! ```text
//! P = I − G (Gᵀ M G)⁻¹ Gᵀ M.
//! ```
//!
//! `P` is idempotent (`P² = P`), `M`-self-adjoint (`(M P)ᵀ = M P`), and
//! annihilates every gradient field (`P G y = 0` for all `y`) while acting
//! as the identity on the divergence-free subspace `{v : Gᵀ M v = 0}`.
//! Applying `P` after every Lanczos step confines the Krylov space to the
//! physical (solenoidal) subspace, so the gradient nullspace never enters
//! the Ritz problem — the gradient near-zero cluster is gone
//! *by construction*, and the physical spectrum is preserved because `P`
//! does not touch it (`P v = v` for divergence-free `v`). This is Palace's
//! approach and the one that composes with future matvec-based iterative
//! eigensolvers (LOBPCG / JD), the #302 Phase-4 GPU-eigensolve prerequisite.
//!
//! # Cost
//!
//! `Gᵀ M G` is a **node**-indexed SPD sparse system: at most
//! `rank(d⁰_interior)` × `rank(d⁰_interior)` (13,747² on the transmon mesh,
//! an order of magnitude below the 133k-DOF edge pencil). It is factored
//! **once** (`sp_lu`) and amortized across the whole Lanczos run — the same
//! amortization pattern the shift-invert LU
//! ([`crate::eigen::lanczos::SparseShiftInvertLanczos`]) already uses for
//! `(K − σM)`. Each projection is then a `Gᵀ (M · w)` SpMV, one triangular
//! solve against the cached factorization, and a `G ·` SpMV.

use faer::Mat;
use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};

use crate::eigen::dense::{EigenError, EigenPair};
use crate::eigen::lanczos::{HISTORICAL_BREAKDOWN_REL, krylov_breakdown, tridiag_eigenpairs};
use crate::eigen::shift_guard::{
    DEGENERATE_SHIFT_REL_TOL, check_degenerate_shift, check_gradient_probe, median_diag_ratio,
};
use crate::eigen::transmon::{assemble_reduced_real, junction_participation};
use crate::sparse_ops::{lu_solve, spmv, spmv_into, spmv_transpose_into};

/// The sparse interior-restricted discrete gradient `G = d⁰_interior` plus
/// the reduced dimensions it maps between.
///
/// `G` is `edge_dim × node_dim`: rows are the reduced interior edge DOFs
/// (the same reindex the PEC-reduced pencil uses), columns are the free
/// (non-grounded) interior nodes. It is a *restriction and reindex* of the
/// full-space [`crate::derham::gradient_map`], consistent with the reduced
/// `(K, M)` pencil — NOT a fresh assembly.
#[derive(Debug, Clone)]
pub struct InteriorGradient {
    /// The sparse `edge_dim × node_dim` gradient operator.
    g: SparseColMat<usize, f64>,
    /// Reduced interior-edge DOF count (rows of `G`, = pencil dimension).
    edge_dim: usize,
    /// Free interior-node count (cols of `G`, = `rank(d⁰_interior)` on a
    /// connected boundary-touching mesh).
    node_dim: usize,
    /// Optional per-reduced-edge-row geometric edge vector
    /// `d_e = p_b − p_a` (with `edges[e] = [a, b]`, the same `−1@a / +1@b`
    /// orientation `G` encodes), length `edge_dim`. Supplied by the caller
    /// when the mesh node coordinates are available; consumed by the
    /// full-AMS vector-nodal interpolation `Π` (Hiptmair–Xu, issue #550) in
    /// [`crate::eigen::ams::AmsLitePreconditioner`]. `None` on the pure
    /// divergence-free-projection path (which never needs `Π`); its presence
    /// is exactly what switches the AMS preconditioner from the gradient-only
    /// two-space cycle to the full three-space cycle.
    edge_vectors: Option<Vec<[f64; 3]>>,
}

impl InteriorGradient {
    /// Build `G = d⁰_interior` from the global edge list, the per-edge
    /// interior mask, and the **reduced** edge reindex `edge_index`
    /// (`Some(r)` = kept interior edge at reduced row `r`, `None` =
    /// eliminated). `n_nodes` is the mesh node count; `edge_dim` is the
    /// reduced pencil dimension (the number of `Some` entries in
    /// `edge_index`).
    ///
    /// A node is a **free** (interior) column iff it is NOT grounded — i.e.
    /// it is not an endpoint of any PEC (excluded) edge. This mirrors the
    /// grounded super-node convention of [`crate::eigen::gauge`] and the
    /// node mask of
    /// [`crate::assembly::nedelec::restrict_gradient_dense`] (grounded
    /// endpoints produce dropped columns), so the sparse `G` here is the
    /// bit-exact sparse analogue of that dense diagnostic operator.
    ///
    /// # Panics
    ///
    /// Panics if `interior_mask.len() != edges.len()`,
    /// `edge_index.len() != edges.len()`, or any endpoint is out of range.
    pub fn build(
        edges: &[[u32; 2]],
        interior_mask: &[bool],
        edge_index: &[Option<usize>],
        n_nodes: usize,
        edge_dim: usize,
    ) -> Self {
        let (g, node_dim) = crate::derham::interior_gradient_map(
            edges,
            interior_mask,
            edge_index,
            n_nodes,
            edge_dim,
        );
        Self {
            g,
            edge_dim,
            node_dim,
            edge_vectors: None,
        }
    }

    /// Attach the per-reduced-edge-row geometric edge vectors
    /// `d_e = p_b − p_a` (`edges[e] = [a, b]`), length `edge_dim`, enabling
    /// the full-AMS vector-nodal interpolation `Π` (issue #550). Returns
    /// `self` for builder-style chaining.
    ///
    /// The vectors are indexed by **reduced edge row** (the same reindex `G`'s
    /// rows use), so `edge_vectors[row]` is the geometry of the interior edge
    /// mapped to row `row`. The `Π` block reads the node-column *incidence*
    /// (which free nodes an edge touches) from `G`'s own sparsity and the edge
    /// *geometry* from these vectors, so no separate free-node → global-node
    /// map is needed.
    ///
    /// # Panics
    ///
    /// Panics if `edge_vectors.len() != edge_dim`.
    #[must_use]
    pub fn with_edge_vectors(mut self, edge_vectors: Vec<[f64; 3]>) -> Self {
        assert_eq!(
            edge_vectors.len(),
            self.edge_dim,
            "edge_vectors length must equal edge_dim (one per reduced edge row)"
        );
        self.edge_vectors = Some(edge_vectors);
        self
    }

    /// The per-reduced-edge-row edge vectors `d_e = p_b − p_a`, if attached
    /// via [`Self::with_edge_vectors`]. `None` means no geometry was supplied,
    /// so the AMS preconditioner runs the gradient-only (two-space) cycle.
    #[inline]
    pub fn edge_vectors(&self) -> Option<&[[f64; 3]]> {
        self.edge_vectors.as_deref()
    }

    /// The sparse gradient operator `G` (`edge_dim × node_dim`).
    #[inline]
    pub fn matrix(&self) -> SparseColMatRef<'_, usize, f64> {
        self.g.as_ref()
    }

    /// Reduced interior-edge DOF count (rows of `G`).
    #[inline]
    pub fn edge_dim(&self) -> usize {
        self.edge_dim
    }

    /// Free interior-node count (cols of `G`) = `rank(d⁰_interior)` on a
    /// connected boundary-touching mesh — the gradient-nullspace dimension.
    #[inline]
    pub fn node_dim(&self) -> usize {
        self.node_dim
    }
}

/// The `M`-orthogonal projector `P = I − G (Gᵀ M G)⁻¹ Gᵀ M` onto the
/// divergence-free subspace, with `Gᵀ M G` factored once.
///
/// Holds the sparse gradient `G` and a cached LU of the SPD node-indexed
/// coupling matrix `C = Gᵀ M G`. [`Self::project_in_place`] applies `P` to a
/// vector; the factorization is reused for every projection across the whole
/// Lanczos run.
pub struct MOrthogonalGradientProjector<'m> {
    gradient: &'m InteriorGradient,
    /// The reduced edge mass `M` (borrowed; used for the `M · w` in `P`).
    m: SparseColMatRef<'m, usize, f64>,
    /// Cached LU of `C = Gᵀ M G` (node-indexed, SPD).
    c_lu: Lu<usize, f64>,
    /// Edge dimension (rows of `G`, length of vectors `P` acts on).
    edge_dim: usize,
    /// Node dimension (cols of `G`, size of the `C` solve).
    node_dim: usize,
}

impl<'m> MOrthogonalGradientProjector<'m> {
    /// Build the projector: form `C = Gᵀ M G` and factor it once.
    ///
    /// `C` is assembled directly from the ultra-sparse `G` (≤2 nonzeros per
    /// row) and the reduced edge mass `M`: for every nonzero `M[i,j] = v`,
    /// the outer product `v · gᵢ gⱼᵀ` (with `gᵢ` the ≤2-nonzero row `i` of
    /// `G`) contributes ≤4 node-indexed triplets, deduplicated by faer. This
    /// is `O(nnz(M))` host work and avoids a general sparse-sparse product.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if the `C` assembly or its sparse LU
    /// factorization fails (e.g. `C` singular — should not happen for an SPD
    /// `M` and a full-column-rank `G`).
    pub fn build(
        gradient: &'m InteriorGradient,
        m: SparseColMatRef<'m, usize, f64>,
    ) -> Result<Self, EigenError> {
        let edge_dim = gradient.edge_dim();
        let node_dim = gradient.node_dim();
        assert_eq!(m.nrows(), edge_dim, "M rows must equal G rows (edge_dim)");
        assert_eq!(m.ncols(), edge_dim, "M cols must equal G rows (edge_dim)");

        // Row view of G: g_rows[i] = list of (node_col, sign) (≤2 entries).
        let g = gradient.matrix();
        let mut g_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); edge_dim];
        {
            let col_ptr = g.col_ptr();
            let row_idx = g.row_idx();
            let val = g.val();
            for col in 0..g.ncols() {
                for k in col_ptr[col]..col_ptr[col + 1] {
                    g_rows[row_idx[k]].push((col, val[k]));
                }
            }
        }

        // C = Gᵀ M G = Σ_{i,j : M[i,j]=v} v · gᵢ gⱼᵀ.
        let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        let mcp = m.col_ptr();
        let mri = m.row_idx();
        let mval = m.val();
        for j in 0..edge_dim {
            let gj = &g_rows[j];
            if gj.is_empty() {
                continue;
            }
            for k in mcp[j]..mcp[j + 1] {
                let i = mri[k];
                let v = mval[k];
                if v == 0.0 {
                    continue;
                }
                let gi = &g_rows[i];
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

        let c = SparseColMat::<usize, f64>::try_new_from_triplets(node_dim, node_dim, &trips)
            .map_err(|e| EigenError::FaerGevd(format!("GᵀMG assembly: {e:?}")))?;
        let c_lu = c
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("GᵀMG sparse LU: {e:?}")))?;

        Ok(Self {
            gradient,
            m,
            c_lu,
            edge_dim,
            node_dim,
        })
    }

    /// Free interior-node count (the size of the inner `C` solve).
    #[inline]
    pub fn node_dim(&self) -> usize {
        self.node_dim
    }

    /// Apply `P = I − G (Gᵀ M G)⁻¹ Gᵀ M` to `w` in place.
    ///
    /// Steps: `mw = M·w`; `rhs = Gᵀ·mw`; solve `C·y = rhs`; `w ← w − G·y`.
    /// After this, `Gᵀ M w ≈ 0` (up to the LU solve accuracy), i.e. `w` is
    /// divergence-free.
    pub fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        use faer::linalg::solvers::Solve;
        assert_eq!(w.len(), self.edge_dim, "projected vector length mismatch");

        // mw = M · w
        let mut mw = vec![0.0_f64; self.edge_dim];
        spmv_into(self.m, w, &mut mw);
        // rhs = Gᵀ · mw   (node-space)
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose_into(self.gradient.matrix(), &mw, &mut rhs);
        // y = C⁻¹ rhs
        let mut y_mat: Mat<f64> = Mat::from_fn(self.node_dim, 1, |r, _| rhs[r]);
        self.c_lu.solve_in_place(y_mat.as_mut());
        let y: Vec<f64> = (0..self.node_dim).map(|r| y_mat[(r, 0)]).collect();
        // gy = G · y   (edge-space)
        let mut gy = vec![0.0_f64; self.edge_dim];
        spmv_into(self.gradient.matrix(), &y, &mut gy);
        // w ← w − G·y
        for (wi, &gyi) in w.iter_mut().zip(gy.iter()) {
            *wi -= gyi;
        }
        Ok(())
    }

    /// Divergence residual of `w` relative to its `M`-norm:
    /// `‖Gᵀ M w‖₂ / ‖w‖_M`. Zero (up to rounding) for a divergence-free
    /// `w`; a drift diagnostic used to decide whether re-projection is
    /// needed. Returns `0` if `‖w‖_M ≈ 0`.
    pub fn divergence_ratio(&self, w: &[f64]) -> f64 {
        assert_eq!(w.len(), self.edge_dim, "vector length mismatch");
        let mut mw = vec![0.0_f64; self.edge_dim];
        spmv_into(self.m, w, &mut mw);
        let m_norm2 = w.iter().zip(mw.iter()).map(|(a, b)| a * b).sum::<f64>();
        if m_norm2 <= 0.0 {
            return 0.0;
        }
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose_into(self.gradient.matrix(), &mw, &mut rhs);
        let res2 = rhs.iter().map(|x| x * x).sum::<f64>();
        (res2 / m_norm2).sqrt()
    }

    /// Edge dimension (rows of `G`, length of vectors `P` acts on).
    #[inline]
    pub fn edge_dim(&self) -> usize {
        self.edge_dim
    }

    /// The reduced edge mass `M` this projector deflates against.
    #[inline]
    pub fn m(&self) -> SparseColMatRef<'m, usize, f64> {
        self.m
    }

    /// `M`-inner product `xᵀ M y`.
    fn m_inner(&self, x: &[f64], y: &[f64]) -> f64 {
        let mut my = vec![0.0_f64; self.edge_dim];
        spmv_into(self.m, y, &mut my);
        x.iter().zip(my.iter()).map(|(a, b)| a * b).sum::<f64>()
    }

    /// The `M`-orthogonal projection of `x` **onto** `image(G)`:
    /// `u = (I − P) x = G (Gᵀ M G)⁻¹ Gᵀ M x`.
    ///
    /// This is the *near-gradient part* of `x` — the complement of
    /// [`Self::project_in_place`]. For the transmon junction eigenvector (99.99%
    /// inside `image(d⁰)`, projected-norm ratio ≈ 1e-4) this recovers the
    /// junction-flux gradient direction almost in its entirety. Because
    /// `u ∈ image(G)`, `P u = 0` exactly — the property the port-aware
    /// re-admission ([`PortAwareGradientProjector`]) relies on.
    pub fn gradient_component(&self, x: &[f64]) -> Result<Vec<f64>, EigenError> {
        use faer::linalg::solvers::Solve;
        assert_eq!(x.len(), self.edge_dim, "vector length mismatch");
        // mw = M · x
        let mut mw = vec![0.0_f64; self.edge_dim];
        spmv_into(self.m, x, &mut mw);
        // rhs = Gᵀ · mw   (node-space)
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose_into(self.gradient.matrix(), &mw, &mut rhs);
        // y = C⁻¹ rhs
        let mut y_mat: Mat<f64> = Mat::from_fn(self.node_dim, 1, |r, _| rhs[r]);
        self.c_lu.solve_in_place(y_mat.as_mut());
        let y: Vec<f64> = (0..self.node_dim).map(|r| y_mat[(r, 0)]).collect();
        // u = G · y   (edge-space, in image(G))
        let mut u = vec![0.0_f64; self.edge_dim];
        spmv_into(self.gradient.matrix(), &y, &mut u);
        Ok(u)
    }
}

/// A **port-aware** `M`-orthogonal projector that deflates the bulk gradient
/// nullspace `image(d⁰_interior)` EXCEPT for one re-admitted direction — the
/// near-gradient junction-flux mode.
///
/// # Why the bulk projector alone is not enough
///
/// The bulk projector `P = I − G(GᵀMG)⁻¹GᵀM`
/// ([`MOrthogonalGradientProjector`]) annihilates *all* of `image(d⁰)`. That
/// removes the 13,747-mode gradient cluster and preserves the cavity spectrum,
/// but it also **deflates the 17.49 GHz port-sheet mode** of the distributed
/// shunt: a lumped inductor is a quasi-static, curl-free flux path, so that
/// eigenvector lives almost entirely in `image(d⁰)` (measured projected-norm
/// ratio ≈ 1e-4, i.e. 99.99% gradient). `P` cannot tell that one gradient
/// direction from the other 13,746 and removes it with the rest (issue #509).
/// Issue #509 read this mode as the physical junction LC mode. Issue #1045
/// re-reads it, on the evidence of #1043 and #1044, as the zero-port-voltage
/// sheet resonance of the distributed `S_Γ` term (Palace has it too); the
/// rank-1 port-voltage shunt A2 removes it, and with A2 the plain bulk
/// projector is exact. This projector re-admits it to match the committed
/// Palace oracle, which lists it.
///
/// # The construction: re-admit `span{û}`
///
/// Let `x_j` be the (ungauged) junction eigenvector and
/// `u = (I − P) x_j ∈ image(G)` its near-gradient part
/// ([`MOrthogonalGradientProjector::gradient_component`]), M-normalized to
/// `û = u / ‖u‖_M`. The port-aware projector is
///
/// ```text
/// P' = P + û ûᵀ M.
/// ```
///
/// Because `u ∈ image(G)` we have `P û = 0`, and `û` is `M`-orthogonal to the
/// solenoidal subspace (`ûᵀ M s = (Gy)ᵀ M s = yᵀ (Gᵀ M s) = 0` for
/// divergence-free `s`). From these two facts `P'` is:
///
/// - **idempotent** (`P'² = P'`) and **`M`-self-adjoint** (`(M P')ᵀ = M P'`) —
///   a genuine `M`-orthogonal projector;
/// - the **identity on the divergence-free subspace** (`P' s = s`, since
///   `P s = s` and `ûᵀ M s = 0`) — so the cavity spectrum is preserved exactly
///   as under `P`;
/// - the **identity on `span{û}`** (`P' û = P û + û(ûᵀ M û) = 0 + û`) — so the
///   junction-flux direction is RE-ADMITTED to the Krylov space and the
///   17.49 GHz port-sheet mode survives;
/// - the **annihilator of `image(G) ⊖ span{û}`** (any gradient `g` with
///   `ûᵀ M g = 0` maps to `P g + û(ûᵀ M g) = 0`) — so the bulk 13,747-mode
///   cluster is still gone.
///
/// The net range of `P'` is `(divergence-free subspace) ⊕ span{û}`: everything
/// physical, nothing spurious-gradient. This is construction (b2) of issue #514
/// — a surgical, measurement-driven deflation that reuses the merged bulk
/// projector unchanged and adds a single rank-1 `M`-orthogonal update.
pub struct PortAwareGradientProjector<'m> {
    /// The bulk `M`-orthogonal gradient projector `P` (borrowed).
    bulk: &'m MOrthogonalGradientProjector<'m>,
    /// The re-admitted junction-flux direction `û`, `M`-normalized
    /// (`ûᵀ M û = 1`), living in `image(G)`.
    u_hat: Vec<f64>,
    /// `M · û`, cached for the rank-1 update `û (ûᵀ M w)`.
    m_u_hat: Vec<f64>,
    /// Edge dimension (length of vectors `P'` acts on).
    edge_dim: usize,
}

impl<'m> PortAwareGradientProjector<'m> {
    /// Build the port-aware projector from the bulk projector and the raw
    /// (ungauged) junction eigenvector `x_junction`.
    ///
    /// Extracts `u = (I − P) x_junction ∈ image(G)`, the junction-flux gradient
    /// direction, and `M`-normalizes it to `û`. The bulk projector is borrowed
    /// unchanged; only the rank-1 re-admission term is added.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if `x_junction`'s near-gradient part has
    /// vanishing `M`-norm (i.e. `x_junction` is already divergence-free — not a
    /// junction-flux mode, so there is nothing to re-admit).
    pub fn build(
        bulk: &'m MOrthogonalGradientProjector<'m>,
        x_junction: &[f64],
    ) -> Result<Self, EigenError> {
        let edge_dim = bulk.edge_dim();
        assert_eq!(
            x_junction.len(),
            edge_dim,
            "junction eigenvector length must equal edge_dim"
        );
        let mut u = bulk.gradient_component(x_junction)?;
        let m_norm2 = bulk.m_inner(&u, &u);
        if m_norm2 <= 0.0 {
            return Err(EigenError::FaerGevd(
                "junction eigenvector has no image(G) component to re-admit \
                 (already divergence-free)"
                    .into(),
            ));
        }
        let inv = 1.0 / m_norm2.sqrt();
        for v in u.iter_mut() {
            *v *= inv;
        }
        let mut m_u_hat = vec![0.0_f64; edge_dim];
        spmv_into(bulk.m(), &u, &mut m_u_hat);
        Ok(Self {
            bulk,
            u_hat: u,
            m_u_hat,
            edge_dim,
        })
    }

    /// Edge dimension (length of vectors `P'` acts on).
    #[inline]
    pub fn edge_dim(&self) -> usize {
        self.edge_dim
    }

    /// Apply `P' = P + û ûᵀ M` to `w` in place: `w ← P w + û (ûᵀ M w)`.
    ///
    /// # Errors
    ///
    /// Propagates [`EigenError`] from the bulk projector's inner `C` solve.
    pub fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        assert_eq!(w.len(), self.edge_dim, "projected vector length mismatch");
        // c = ûᵀ M w  (computed BEFORE P w — û ∈ image(G), P w ⟂_M û, so the
        // coefficient must be read off the ORIGINAL w, not the projected one).
        let c = self
            .m_u_hat
            .iter()
            .zip(w.iter())
            .map(|(mu, wi)| mu * wi)
            .sum::<f64>();
        // w ← P w
        self.bulk.project_in_place(w)?;
        // w ← w + c û
        for (wi, &ui) in w.iter_mut().zip(self.u_hat.iter()) {
            *wi += c * ui;
        }
        Ok(())
    }

    /// Divergence residual `‖Gᵀ M w‖ / ‖w‖_M` via the bulk projector — a
    /// diagnostic. A `P'`-projected vector is NOT divergence-free in general
    /// (it carries the re-admitted `û` component, which is a gradient), so this
    /// is expected to be `O(1)` on the 17.49 GHz port-sheet mode and `≈ 0` on the cavity
    /// modes.
    pub fn divergence_ratio(&self, w: &[f64]) -> f64 {
        self.bulk.divergence_ratio(w)
    }
}

/// A port-aware `M`-orthogonal projector whose re-admitted gradient subspace
/// is read off the **port operator** instead of an eigenvector (issue #950).
///
/// # Which gradients a nonzero-λ eigenvector carries
///
/// Let the reduced pencil be `K = K_vol + K_port`, with `K_port` a surface
/// term on a port patch (the junction's `ℓ/(w L̃) S_Γ`). The interior gradient
/// `G` satisfies `K_vol G = 0`, so `Gᵀ K = Gᵀ K_port`. For any eigenpair
/// `K x = λ M x` with `λ ≠ 0`, multiplying by `Gᵀ` gives
///
/// ```text
/// Gᵀ M x = (1/λ) Gᵀ K_port x,
/// ```
///
/// and the right-hand side is supported on the **port nodes**: the free
/// nodes with an incident edge in the support of `K_port`. Write `E` for the
/// selection of those `r` nodes. The gradient part of every such eigenvector,
/// `(I − P) x = G (GᵀMG)⁻¹ Gᵀ M x`, therefore lies in
///
/// ```text
/// V = span G Z,   Z = (GᵀMG)⁻¹ E   (node_dim × r).
/// ```
///
/// So every eigenvector with `λ ≠ 0` lies in `(divergence-free) ⊕ V`, for
/// every mode at once and in exact arithmetic. No eigensolve is needed to
/// find the junction-flux direction, and the 17.49 GHz port-sheet mode is not
/// approximated by a single re-admitted vector taken from another solve.
///
/// # The projector
///
/// With `φ = (GᵀMG)⁻¹ Gᵀ M w` (the bulk projector's node potential),
/// `H = Eᵀ Z` (the `r × r` port block of `(GᵀMG)⁻¹`, SPD) and
/// `ψ = φ − Z H⁻¹ Eᵀ φ`,
///
/// ```text
/// P_V w = P w + G Z H⁻¹ Eᵀ φ = w − G ψ.
/// ```
///
/// `G Z H⁻¹ Eᵀ φ` is the `M`-orthogonal projection of `w` onto `V`
/// (`(GZ)ᵀ M (GZ) = H`, `(GZ)ᵀ M w = Eᵀ φ`), and `V ⊂ image(G)` is
/// `M`-orthogonal to the divergence-free subspace, so `P_V = P + Q_V` is an
/// `M`-orthogonal projector onto `(divergence-free) ⊕ V`.
///
/// # What else it admits
///
/// The range may contain vectors of `kernel(K)`: `V` can be larger than the
/// span of the eigenvectors' gradient parts, and the divergence-free subspace
/// already contains the `λ = 0` potential of any port-free floating conductor.
/// Those come out of the Lanczos solve as exact `λ = 0` Ritz pairs and are
/// dropped by [`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`].
///
/// # Cost
///
/// `r` extra solves with the bulk projector's cached `GᵀMG` factorization at
/// build time, `node_dim × r` dense storage for `Z`, and one dense `r × r` LU.
/// Each application costs one bulk projection plus an `O(node_dim · r)`
/// update. No factorization of `K − σM` is involved.
pub struct PortSubspaceGradientProjector<'m> {
    /// The bulk `M`-orthogonal gradient projector `P` (borrowed).
    bulk: &'m MOrthogonalGradientProjector<'m>,
    /// Node columns of `G` selected by `E` (the port nodes), ascending.
    port_nodes: Vec<usize>,
    /// `Z = (GᵀMG)⁻¹ E`, `node_dim × r`.
    z: Mat<f64>,
    /// Dense LU of `H = Eᵀ Z` (`r × r`); `None` when `r = 0`.
    h_lu: Option<faer::linalg::solvers::PartialPivLu<f64>>,
}

impl<'m> PortSubspaceGradientProjector<'m> {
    /// The port nodes of a reduced port operator: the free-node columns of
    /// `gradient` with an incident edge row in the support of `k_port`
    /// (the support of `Gᵀ K_port`), ascending.
    ///
    /// # Panics
    ///
    /// Panics if `k_port` is not `edge_dim × edge_dim`.
    pub fn port_nodes(
        gradient: &InteriorGradient,
        k_port: SparseColMatRef<'_, usize, f64>,
    ) -> Vec<usize> {
        let edge_dim = gradient.edge_dim();
        assert_eq!(k_port.nrows(), edge_dim, "K_port rows must equal edge_dim");
        assert_eq!(k_port.ncols(), edge_dim, "K_port cols must equal edge_dim");
        let mut port_edge = vec![false; edge_dim];
        let cp = k_port.col_ptr();
        let ri = k_port.row_idx();
        let val = k_port.val();
        for j in 0..edge_dim {
            for p in cp[j]..cp[j + 1] {
                if val[p] != 0.0 {
                    port_edge[ri[p]] = true;
                    port_edge[j] = true;
                }
            }
        }
        let g = gradient.matrix();
        let gcp = g.col_ptr();
        let gri = g.row_idx();
        (0..g.ncols())
            .filter(|&col| (gcp[col]..gcp[col + 1]).any(|p| port_edge[gri[p]]))
            .collect()
    }

    /// Build the projector from the bulk projector and the port nodes
    /// (typically [`Self::port_nodes`] of the reduced `K_port`).
    ///
    /// An empty `port_nodes` gives the bulk projector `P` unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`EigenError::FaerGevd`] if a node index is out of range or
    /// `H = Eᵀ (GᵀMG)⁻¹ E` is not positive definite (repeated nodes make it
    /// singular).
    pub fn build(
        bulk: &'m MOrthogonalGradientProjector<'m>,
        port_nodes: &[usize],
    ) -> Result<Self, EigenError> {
        use faer::linalg::solvers::Solve;
        let node_dim = bulk.node_dim();
        let r = port_nodes.len();
        if let Some(&bad) = port_nodes.iter().find(|&&j| j >= node_dim) {
            return Err(EigenError::FaerGevd(format!(
                "port node {bad} out of range (node_dim {node_dim})"
            )));
        }
        let mut z = Mat::<f64>::zeros(node_dim, r);
        for (c, &j) in port_nodes.iter().enumerate() {
            z[(j, c)] = 1.0;
        }
        if r > 0 {
            bulk.c_lu.solve_in_place(z.as_mut());
        }
        let h = Mat::<f64>::from_fn(r, r, |a, b| z[(port_nodes[a], b)]);
        let h_lu = if r == 0 {
            None
        } else {
            // H is the port block of (GᵀMG)⁻¹: symmetric positive definite
            // for distinct nodes. Check that before using a plain LU on it.
            if h.llt(faer::Side::Lower).is_err() {
                return Err(EigenError::FaerGevd(
                    "port block of (GᵀMG)⁻¹ is not positive definite \
                     (repeated port node?)"
                        .into(),
                ));
            }
            Some(h.partial_piv_lu())
        };
        Ok(Self {
            bulk,
            port_nodes: port_nodes.to_vec(),
            z,
            h_lu,
        })
    }

    /// Number of re-admitted gradient directions `r` (port nodes).
    #[inline]
    pub fn n_port_nodes(&self) -> usize {
        self.port_nodes.len()
    }

    /// Edge dimension (length of vectors `P_V` acts on).
    #[inline]
    pub fn edge_dim(&self) -> usize {
        self.bulk.edge_dim()
    }

    /// Apply `P_V` to `w` in place: `w ← w − G (φ − Z H⁻¹ Eᵀ φ)` with
    /// `φ = (GᵀMG)⁻¹ Gᵀ M w`.
    ///
    /// # Errors
    ///
    /// Never fails today; the `Result` mirrors the other projectors.
    pub fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        use faer::linalg::solvers::Solve;
        let bulk = self.bulk;
        assert_eq!(w.len(), bulk.edge_dim, "projected vector length mismatch");
        let mut mw = vec![0.0_f64; bulk.edge_dim];
        spmv_into(bulk.m, w, &mut mw);
        let mut rhs = vec![0.0_f64; bulk.node_dim];
        spmv_transpose_into(bulk.gradient.matrix(), &mw, &mut rhs);
        let mut phi: Mat<f64> = Mat::from_fn(bulk.node_dim, 1, |i, _| rhs[i]);
        bulk.c_lu.solve_in_place(phi.as_mut());
        let mut psi: Vec<f64> = (0..bulk.node_dim).map(|i| phi[(i, 0)]).collect();
        if let Some(h_lu) = &self.h_lu {
            let r = self.port_nodes.len();
            let mut c: Mat<f64> = Mat::from_fn(r, 1, |a, _| psi[self.port_nodes[a]]);
            h_lu.solve_in_place(c.as_mut());
            for b in 0..r {
                let cb = c[(b, 0)];
                if cb == 0.0 {
                    continue;
                }
                let col = self.z.col(b);
                for (pi, zi) in psi.iter_mut().zip(col.iter()) {
                    *pi -= cb * zi;
                }
            }
        }
        let mut g_psi = vec![0.0_f64; bulk.edge_dim];
        spmv_into(bulk.gradient.matrix(), &psi, &mut g_psi);
        for (wi, gi) in w.iter_mut().zip(g_psi.iter()) {
            *wi -= gi;
        }
        Ok(())
    }

    /// Divergence residual `‖Gᵀ M w‖ / ‖w‖_M` via the bulk projector (a
    /// diagnostic; `O(1)` on modes that carry a port gradient).
    pub fn divergence_ratio(&self, w: &[f64]) -> f64 {
        self.bulk.divergence_ratio(w)
    }
}

impl KrylovProjector for PortSubspaceGradientProjector<'_> {
    fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        PortSubspaceGradientProjector::project_in_place(self, w)
    }
    fn divergence_ratio(&self, w: &[f64]) -> f64 {
        PortSubspaceGradientProjector::divergence_ratio(self, w)
    }
    fn edge_dim(&self) -> usize {
        self.bulk.edge_dim()
    }
}

/// `K − σ M` as a fresh sparse matrix, used only to build the shift-invert
/// LU (same construction as the un-projected Lanczos path).
fn shifted_pencil(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let n = k.nrows();
    let mut trips: Vec<Triplet<usize, usize, f64>> =
        Vec::with_capacity(k.col_ptr()[n] + m.col_ptr()[n]);
    let mut push = |a: SparseColMatRef<'_, usize, f64>, scale: f64| {
        let cp = a.col_ptr();
        let ri = a.row_idx();
        let v = a.val();
        for j in 0..a.ncols() {
            for k in cp[j]..cp[j + 1] {
                trips.push(Triplet::new(ri[k], j, scale * v[k]));
            }
        }
    };
    push(k, 1.0);
    if sigma != 0.0 {
        push(m, -sigma);
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("shifted pencil assembly: {e:?}")))
}

/// Relative eigen-residual `‖K x − λ M x‖₂ / (|λ| ‖M x‖₂)`, with `|σ|` in
/// place of `|λ|` when `λ = 0` exactly (and `NaN` if both are zero or
/// `M x = 0`).
fn relative_residual(
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    lambda: f64,
    x: &[f64],
    sigma: f64,
) -> f64 {
    let kx = spmv(k, x);
    let mx = spmv(m, x);
    let r = kx
        .iter()
        .zip(mx.iter())
        .map(|(a, b)| (a - lambda * b).powi(2))
        .sum::<f64>()
        .sqrt();
    let mx_norm = mx.iter().map(|v| v * v).sum::<f64>().sqrt();
    let scale = if lambda != 0.0 {
        lambda.abs()
    } else {
        sigma.abs()
    };
    let den = scale * mx_norm;
    if den > 0.0 { r / den } else { f64::NAN }
}

/// Tunables for the projected shift-invert Lanczos (mirrors
/// [`crate::eigen::lanczos::SparseShiftInvertLanczos`] with an added
/// re-projection cadence).
///
/// # ⚠ Choosing `sigma`
///
/// The projection deflates the gradient subspace from each **Krylov
/// vector**; it does not modify the operator `A = K − σM` that is
/// factored. The [`Default`] `σ = 0` therefore factors the bare curl-curl
/// `K`, which is singular on its discrete-gradient null space. Always pass
/// an explicit `σ` strictly between the null cluster (`λ ≈ 0`) and the
/// lowest eigenvalue of interest (both production callers, e.g.
/// [`solve_transmon_eigenmodes_projected`], do). As a best-effort
/// backstop the solve returns [`EigenError::DegenerateShift`] when `σ` is
/// numerically zero and either `K − σM` is singular along the gradient
/// subspace (pre-solve probe) or **every** returned Ritz value collapsed
/// onto `σ` (issue #696, see [`crate::eigen::shift_guard`]). A shift placed
/// exactly on a nonzero eigenvalue is not rejected.
#[derive(Debug, Clone, Copy)]
pub struct ProjectedShiftInvertLanczos {
    /// Shift `σ = k²`; Ritz values closest to `σ` converge first. **Do not
    /// leave at the default `0.0` on a curl-curl pencil** — see the struct
    /// docs.
    pub sigma: f64,
    /// Maximum Lanczos iterations.
    pub max_iters: usize,
    /// Relative residual (Kaniel–Saad β-bound) tolerance.
    pub tol: f64,
    /// Re-project the running direction whenever its divergence ratio
    /// exceeds this threshold (numerical-hygiene guard against drift back
    /// into the gradient subspace over many iterations). One projection per
    /// step already runs unconditionally; this triggers a *second* pass only
    /// when drift accumulates. `1e-8` is comfortable for f64.
    pub reproject_threshold: f64,
}

impl Default for ProjectedShiftInvertLanczos {
    fn default() -> Self {
        Self {
            sigma: 0.0,
            max_iters: 96,
            tol: 1e-8,
            reproject_threshold: 1e-8,
        }
    }
}

/// Diagnostics recorded during a projected solve (non-gating; drives the
/// benchmark's drift / re-projection reporting).
#[derive(Debug, Clone, Default)]
pub struct ProjectionDiagnostics {
    /// Number of Lanczos iterations actually run.
    pub iterations: usize,
    /// Number of *extra* (second-pass) re-projections triggered by drift.
    pub reprojections: usize,
    /// Largest divergence ratio observed on a fresh Krylov vector *before*
    /// its mandatory projection (how far the raw `A⁻¹ M` step wandered into
    /// the gradient subspace).
    pub max_pre_projection_divergence: f64,
    /// Largest divergence ratio observed *after* projection (the residual
    /// leak the projector could not remove — should stay near machine eps).
    pub max_post_projection_divergence: f64,
    /// Divergence ratio `‖Gᵀ M x‖ / ‖x‖_M` of each returned Ritz vector, in
    /// the same order as the returned modes. Near-zero for a genuinely
    /// solenoidal (physical) mode; a mode that is *not* a bulk-gradient
    /// artifact (e.g. a port-localized near-nullspace mode) can still have a
    /// small ratio yet survive the projection, so this quantifies which
    /// surviving modes are truly divergence-free vs. gradient remnants.
    pub mode_divergence_ratios: Vec<f64>,
    /// Relative eigen-residual `‖K x − λ M x‖₂ / (|λ| ‖M x‖₂)` of each
    /// returned Ritz pair, in the same order as the returned modes (issue
    /// #950): the per-result accuracy a caller should check before trusting
    /// a mode. `σ` stands in for `|λ|` when `λ = 0` exactly.
    pub mode_residual_rels: Vec<f64>,
    /// Number of sparse LU factorizations of a shifted pencil `K − σM` the
    /// solve performed (the `Gᵀ M G` factorization of the projector is not
    /// counted). `1` for a single projected run; the composite port-aware
    /// solves report their total (issue #950).
    pub shifted_factorizations: usize,
    /// Ritz pairs dropped by the null-mode filter of
    /// [`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`] (issue
    /// #950); `0` when the filter is not used.
    pub null_modes_dropped: usize,
    /// The `|λ|` ceiling of that filter,
    /// [`DEGENERATE_SHIFT_REL_TOL`]` × median(|K_ii| / |M_ii|)`; `0.0` when
    /// the filter is not used.
    pub null_ceiling: f64,
    /// Largest `|λ|` among the dropped null modes (`0.0` if none). Together
    /// with [`Self::null_ceiling`] and the smallest returned `|λ|` it shows the
    /// margin on both sides of the cut.
    pub max_dropped_null_lambda: f64,
    /// Gradient directions a port-aware projector re-admits: the port-node
    /// count `r` of [`solve_transmon_eigenmodes_port_subspace`], `1` for the
    /// eigenvector route of [`solve_transmon_eigenmodes_port_aware`], `0`
    /// for the bulk projector.
    pub readmitted_gradient_directions: usize,
}

/// A projector that can be applied in place to a Krylov vector and report a
/// divergence ratio — the common interface the projected Lanczos core drives.
///
/// Implemented by both the bulk [`MOrthogonalGradientProjector`] (deflates all
/// of `image(d⁰)`) and the [`PortAwareGradientProjector`] (deflates all of
/// `image(d⁰)` except the re-admitted junction-flux direction). Making the
/// Lanczos core generic over this trait lets the *same* recurrence, mandatory
/// projection, and drift-reprojection logic serve both paths — the port-aware
/// solve is not a fork of the eigensolver, only a different projector.
pub trait KrylovProjector {
    /// Apply the projector to `w` in place.
    fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError>;
    /// Divergence residual `‖Gᵀ M w‖ / ‖w‖_M` (drift diagnostic).
    fn divergence_ratio(&self, w: &[f64]) -> f64;
    /// Edge dimension (length of the vectors the projector acts on).
    fn edge_dim(&self) -> usize;
}

impl KrylovProjector for MOrthogonalGradientProjector<'_> {
    fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        MOrthogonalGradientProjector::project_in_place(self, w)
    }
    fn divergence_ratio(&self, w: &[f64]) -> f64 {
        MOrthogonalGradientProjector::divergence_ratio(self, w)
    }
    fn edge_dim(&self) -> usize {
        self.edge_dim
    }
}

impl KrylovProjector for PortAwareGradientProjector<'_> {
    fn project_in_place(&self, w: &mut [f64]) -> Result<(), EigenError> {
        PortAwareGradientProjector::project_in_place(self, w)
    }
    fn divergence_ratio(&self, w: &[f64]) -> f64 {
        PortAwareGradientProjector::divergence_ratio(self, w)
    }
    fn edge_dim(&self) -> usize {
        self.edge_dim
    }
}

impl ProjectedShiftInvertLanczos {
    /// Projected shift-invert Lanczos: identical to the un-projected core
    /// ([`crate::eigen::lanczos::SparseShiftInvertLanczos::smallest_eigenpairs`])
    /// except that every fresh Krylov vector is `M`-orthogonally projected
    /// onto the divergence-free subspace (`w ← P w`) *before* the three-term
    /// recurrence and reorthogonalization. This confines the whole Krylov
    /// space to the solenoidal subspace, so the gradient nullspace never
    /// enters the tridiagonal Ritz problem — the gradient near-zero
    /// cluster is absent by construction, while the physical spectrum is
    /// preserved (`P` acts as the identity on divergence-free fields).
    ///
    /// Returns the lowest `n_modes` eigenpairs closest to `σ`, plus the
    /// [`ProjectionDiagnostics`] for the run.
    ///
    /// # Errors
    ///
    /// Propagates [`EigenError`] from the shift-invert LU, the projector, or
    /// the tridiagonal solve.
    pub fn smallest_eigenpairs<P: KrylovProjector>(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        projector: &P,
        n_modes: usize,
    ) -> Result<(Vec<EigenPair>, ProjectionDiagnostics), EigenError> {
        self.run(k, m, projector, n_modes, false)
    }

    /// [`Self::smallest_eigenpairs`] with the **exact null modes of `K`
    /// dropped before the `n_modes` cut** (issue #950), so that physical
    /// modes fill every requested slot.
    ///
    /// # Which pairs are dropped
    ///
    /// A Ritz pair is a null mode when `|λ| ≤ τ` with
    /// `τ = `[`DEGENERATE_SHIFT_REL_TOL`]` × median(|K_ii| / |M_ii|)` — the
    /// same "numerically zero eigenvalue" scale the #696 degenerate-shift
    /// detector uses ([`crate::eigen::shift_guard`]). It is not a tuned
    /// number and uses no reference spectrum:
    ///
    /// - A vector in `kernel(K)` has `λ = 0` exactly; in floating point its
    ///   Ritz value is `O(ε · ‖K‖/‖M‖)`, six orders **below** `τ`. On the
    ///   port-aware transmon solves these are the curl-free fields a
    ///   projection onto `image(d⁰_interior)` cannot reach (the potential of a
    ///   floating, port-free conductor is a gradient of a scalar that is
    ///   constant on that conductor, not of one that vanishes on all metal),
    ///   plus any curl-free direction a port-aware re-admission brings back.
    /// - A physical FEM eigenvalue is `λ ≳ scale · (h/L)²`, so it falls
    ///   below `τ` only on a mesh with `h/L ≲ 1e-5`. In frequency, a physical
    ///   mode would be dropped only if it sat about five decades (`√1e-10`)
    ///   below the median cell frequency `√scale`. The diagnostics report
    ///   `τ`, the largest dropped `|λ|` and (via the returned modes) the
    ///   smallest kept one, so the margin on each side is visible per run.
    ///
    /// The filter runs over **every** Ritz pair of the Krylov basis before the
    /// closest-to-`σ` truncation, so a dropped null mode is replaced by the
    /// next pair rather than leaving its slot empty. If the basis holds fewer
    /// than `n_modes` non-null pairs the solve returns the ones it has (it
    /// does not fail); compare the returned length with `n_modes`.
    ///
    /// # Errors
    ///
    /// As [`Self::smallest_eigenpairs`].
    pub fn smallest_nonnull_eigenpairs<P: KrylovProjector>(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        projector: &P,
        n_modes: usize,
    ) -> Result<(Vec<EigenPair>, ProjectionDiagnostics), EigenError> {
        self.run(k, m, projector, n_modes, true)
    }

    /// The shared projected shift-invert core; `drop_null` enables the
    /// null-mode filter of [`Self::smallest_nonnull_eigenpairs`].
    fn run<P: KrylovProjector>(
        &self,
        k: SparseColMatRef<'_, usize, f64>,
        m: SparseColMatRef<'_, usize, f64>,
        projector: &P,
        n_modes: usize,
        drop_null: bool,
    ) -> Result<(Vec<EigenPair>, ProjectionDiagnostics), EigenError> {
        let n = k.nrows();
        assert_eq!(k.ncols(), n, "K must be square");
        assert_eq!(m.nrows(), n, "M and K must agree in size");
        assert_eq!(m.ncols(), n);
        assert_eq!(
            projector.edge_dim(),
            n,
            "projector edge_dim must equal pencil dimension"
        );
        let mut diag = ProjectionDiagnostics::default();
        if n_modes == 0 {
            return Ok((Vec::new(), diag));
        }

        // 0. Degenerate-shift probe (issue #696). The projector certifies a
        //    gradient subspace exists; if `K − σM` is numerically singular
        //    on it, the factorization below would "succeed" and the
        //    projected Krylov run would return round-off garbage as `Ok`.
        //    `g = (I − P) z` is a gradient-subspace vector for any `z`.
        let pencil_scale = median_diag_ratio(k, m, f64::abs);
        {
            let z: Vec<f64> = (0..n)
                .map(|i| (((i as f64) + 1.0) * 0.6437).cos())
                .collect();
            let mut pz = z.clone();
            projector.project_in_place(&mut pz)?;
            let g: Vec<f64> = z.iter().zip(pz.iter()).map(|(a, b)| a - b).collect();
            let mg = spmv(m, &g);
            let kg = spmv(k, &g);
            let norm = |v: &mut dyn Iterator<Item = f64>| v.map(|x| x * x).sum::<f64>().sqrt();
            let a_g = norm(&mut kg.iter().zip(mg.iter()).map(|(a, b)| a - self.sigma * b));
            let m_g = norm(&mut mg.iter().copied());
            check_gradient_probe(self.sigma, a_g, m_g, pencil_scale)?;
        }

        // 1. Factor A = K − σM once.
        let a = shifted_pencil(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?;
        diag.shifted_factorizations = 1;

        let max_k = self.max_iters.min(n).max(n_modes + 2).min(n);
        let mut basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        let mut m_basis: Vec<Vec<f64>> = Vec::with_capacity(max_k);
        let mut alpha: Vec<f64> = Vec::with_capacity(max_k);
        let mut beta: Vec<f64> = Vec::with_capacity(max_k);

        // Start vector: deterministic sin-based, projected onto the
        // divergence-free subspace before M-normalization so the whole run
        // starts solenoidal.
        let mut v: Vec<f64> = (0..n)
            .map(|i| (((i as f64) + 1.0) * 0.5432).sin())
            .collect();
        projector.project_in_place(&mut v)?;
        let mut mv = spmv(m, &v);
        let mut nrm2 = v.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
        if nrm2 <= 0.0 {
            return Err(EigenError::FaerGevd(
                "projected starting vector has non-positive M-norm".into(),
            ));
        }
        let mut nrm = nrm2.sqrt();
        for x in v.iter_mut() {
            *x /= nrm;
        }

        let mut w = vec![0.0_f64; n];
        let mut work = vec![0.0_f64; n];
        // Running `max |α_j|`, the scale of `T_k`, for the breakdown test.
        let mut alpha_scale = 0.0_f64;

        for j in 0..max_k {
            diag.iterations = j + 1;
            spmv_into(m, &v, &mut mv);
            lu_solve(&lu, &mv, &mut w);

            // --- Divergence-free projection of the fresh Krylov vector. ---
            let pre = projector.divergence_ratio(&w);
            diag.max_pre_projection_divergence = diag.max_pre_projection_divergence.max(pre);
            projector.project_in_place(&mut w)?;
            let mut post = projector.divergence_ratio(&w);
            // Numerical hygiene: a single pass leaves a tiny residual leak;
            // re-project if drift is above threshold (measured + reported).
            if post > self.reproject_threshold {
                projector.project_in_place(&mut w)?;
                diag.reprojections += 1;
                post = projector.divergence_ratio(&w);
            }
            diag.max_post_projection_divergence = diag.max_post_projection_divergence.max(post);

            let aj = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            alpha.push(aj);
            alpha_scale = alpha_scale.max(aj.abs());
            for i in 0..n {
                w[i] -= aj * v[i];
            }
            if let Some(bp) = beta.last().copied() {
                let prev = &basis[j - 1];
                for i in 0..n {
                    w[i] -= bp * prev[i];
                }
            }

            // Full M-reorthogonalization against the whole basis, reusing
            // the cached M·v_k (same as the un-projected path).
            for (vk, m_vk) in basis.iter().zip(m_basis.iter()) {
                let c = w.iter().zip(m_vk.iter()).map(|(a, b)| a * b).sum::<f64>();
                if c.abs() > 0.0 {
                    for i in 0..n {
                        w[i] -= c * vk[i];
                    }
                }
            }
            let c = w.iter().zip(mv.iter()).map(|(a, b)| a * b).sum::<f64>();
            for i in 0..n {
                w[i] -= c * v[i];
            }

            spmv_into(m, &w, &mut work);
            nrm2 = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            let nrm2 = nrm2.max(0.0);
            nrm = nrm2.sqrt();

            m_basis.push(core::mem::take(&mut mv));
            mv = vec![0.0_f64; n];
            basis.push(core::mem::take(&mut v));

            // β-bound probe and breakdown, both relative to the scale of
            // `T_k` so they behave the same in every mesh length unit
            // (issue #828; they were `β ≤ tol · max(μ_max, 1)` and the
            // absolute `β < 1e-14`). Same tests as `eigen::lanczos`.
            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let (mus, _) = tridiag_eigenpairs(&alpha, &beta)?;
                let mu_max = mus.iter().fold(0.0_f64, |a, &b| a.max(b.abs()));
                if nrm <= self.tol * mu_max {
                    break;
                }
            }
            if krylov_breakdown(nrm, alpha_scale, HISTORICAL_BREAKDOWN_REL) {
                break;
            }

            beta.push(nrm);
            v = w.iter().map(|x| x / nrm).collect();
        }

        if alpha.is_empty() {
            return Err(EigenError::FaerGevd(
                "projected Lanczos produced no iterations".into(),
            ));
        }

        // Tridiagonal eigenpairs → Ritz vectors → M-orthonormalize.
        let (mus, s_mat) = tridiag_eigenpairs(&alpha, &beta)?;
        let k_eff = mus.len();
        let sigma = self.sigma;
        let mut pairs: Vec<(f64, Vec<f64>)> = Vec::with_capacity(k_eff);
        for col in 0..k_eff {
            let mu = mus[col];
            if mu.abs() == 0.0 {
                continue;
            }
            let lambda = sigma + 1.0 / mu;
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
        if drop_null {
            // Issue #950: drop the exact null modes of K before the n_modes
            // cut (see `smallest_nonnull_eigenpairs`). A NaN λ fails the
            // `<=` test and is kept, so a broken pair stays visible.
            let ceiling = DEGENERATE_SHIFT_REL_TOL * pencil_scale;
            diag.null_ceiling = ceiling;
            if ceiling.is_finite() && ceiling > 0.0 {
                let before = pairs.len();
                let mut max_dropped = 0.0_f64;
                pairs.retain(|(lambda, _)| {
                    let null = lambda.abs() <= ceiling;
                    if null {
                        max_dropped = max_dropped.max(lambda.abs());
                    }
                    !null
                });
                diag.null_modes_dropped = before - pairs.len();
                diag.max_dropped_null_lambda = max_dropped;
            }
        }
        pairs.sort_by(|a, b| {
            (a.0 - sigma)
                .abs()
                .partial_cmp(&(b.0 - sigma).abs())
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let take = n_modes.min(pairs.len());
        let mut picked: Vec<(f64, Vec<f64>)> = pairs.into_iter().take(take).collect();
        picked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

        // Degenerate-shift guard (issue #696): fail loudly instead of
        // returning a Ritz set collapsed onto a singular shift.
        let dists: Vec<f64> = picked.iter().map(|p| (p.0 - sigma).abs()).collect();
        check_degenerate_shift(sigma, &dists, pencil_scale)?;

        let mut out = Vec::with_capacity(take);
        for (lambda, mut x) in picked {
            spmv_into(m, &x, &mut work);
            let norm2 = x.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            if norm2 > 0.0 {
                let s = norm2.sqrt();
                for v in x.iter_mut() {
                    *v /= s;
                }
            }
            out.push(EigenPair { lambda, vector: x });
        }
        diag.mode_divergence_ratios = out
            .iter()
            .map(|p| projector.divergence_ratio(&p.vector))
            .collect();
        diag.mode_residual_rels = out
            .iter()
            .map(|p| relative_residual(k, m, p.lambda, &p.vector, sigma))
            .collect();
        Ok((out, diag))
    }
}

/// Solve the transmon eigenmodes with the **spectrum-preserving
/// divergence-free projection** (issue #509) — the projected analogue of
/// [`crate::eigen::transmon::solve_transmon_eigenmodes`].
///
/// Assembles the reduced real pencil `(K + K_port) x = λ (M + M_port) x`
/// over the PEC-interior DOFs (the same plain interior reindex the ungauged
/// path uses — this is a *projection*, not a DOF elimination, so no
/// tree-cotree reindex is applied), builds `G = d⁰_interior` and the
/// `M`-orthogonal projector `P = I − G(GᵀMG)⁻¹GᵀM`, then runs projected
/// shift-invert Lanczos near `sigma`. The gradient nullspace is deflated
/// every iteration, so the spurious gradient-adjacent mode and the near-zero
/// cluster are absent while the physical spectrum is preserved.
///
/// Returns the modes (restored physical frequency + junction participation)
/// and the [`ProjectionDiagnostics`] for the run (iteration count, drift,
/// re-projection count).
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly, the projector build
/// (`GᵀMG` LU), or the projected Lanczos solve.
pub fn solve_transmon_eigenmodes_projected(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<
    (
        Vec<crate::eigen::transmon::ModeReport>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    projected_transmon_impl(
        pencil,
        crate::eigen::transmon::JunctionPortModel::DistributedSurface,
        sigma,
        n_modes,
        m_per_unit,
        false,
    )
    .map(|(modes, diag)| (modes.into_iter().map(|m| m.report).collect(), diag))
}

/// [`solve_transmon_eigenmodes_projected`] with an explicit junction
/// [`crate::eigen::transmon::JunctionPortModel`] (issue #1044).
///
/// Unlike [`solve_transmon_eigenmodes_projected`], this entry point drops the
/// **exact null modes of `K`** before the `n_modes` cut, with the
/// reference-free filter of
/// [`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`] (issue #950;
/// `|λ| ≤ 1e-10 × median(K_ii/M_ii)`, no reference spectrum). On the 133k
/// fixture the bulk projector leaves one such pair (`λ ≈ −4e-20`, a curl-free
/// field that `image(d⁰_interior)` cannot reach) under either model, and
/// without the filter it takes one of the `n_modes` slots.
///
/// With `DistributedSurface` the bulk projector `P` is not exact, because
/// `Gᵀ K_port ≠ 0` for the distributed `S_Γ`: it deflates the 17.49 GHz
/// port-sheet mode and leaves residuals up to about `5e-3` on the 133k
/// fixture (issue #509).
///
/// With `PortVoltage` (the rank-1 A2 shunt,
/// [`crate::eigen::transmon::LumpedPortVoltageShunt`]) `Gᵀ b = 0`, so
/// `Gᵀ K_port = Gᵀ M_port = 0` and `P` is exact: `image(d⁰_interior)` is in
/// the kernel of `K + K_port`, `P` commutes with the shift-invert operator,
/// and no port-aware re-admission ([`PortAwareGradientProjector`],
/// [`PortSubspaceGradientProjector`]) is needed. Those projectors exist only to
/// keep the zero-voltage bump mode, which A2 does not have. On the 133k
/// fixture the A2 residuals are `≤ 1e-8`.
///
/// # Errors
///
/// As [`solve_transmon_eigenmodes_projected`].
pub fn solve_transmon_eigenmodes_projected_with_port_model(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    port_model: crate::eigen::transmon::JunctionPortModel,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<
    (
        Vec<crate::eigen::transmon::ModeReport>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    projected_transmon_impl(pencil, port_model, sigma, n_modes, m_per_unit, true)
        .map(|(modes, diag)| (modes.into_iter().map(|m| m.report).collect(), diag))
}

/// [`solve_transmon_eigenmodes_projected_with_port_model`] that also returns
/// each mode's reduced interior-DOF eigenvector (issue #1092), for the
/// Hellmann–Feynman geometry gradient of an eigenfrequency.
///
/// Each [`crate::eigen::transmon::TransmonMode::vector`] is in the plain
/// PEC-interior DOF ordering of the pencil (the same reduction as
/// [`solve_transmon_eigenmodes_projected_with_port_model`]) and is
/// `M`-orthonormalized against the reduced `M + M_port` by the projected
/// Lanczos Ritz step. The reports are bit-identical to the
/// vector-free entry point.
///
/// # Errors
///
/// As [`solve_transmon_eigenmodes_projected`].
pub fn solve_transmon_eigenmodes_projected_with_port_model_full(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    port_model: crate::eigen::transmon::JunctionPortModel,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<
    (
        Vec<crate::eigen::transmon::TransmonMode>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    projected_transmon_impl(pencil, port_model, sigma, n_modes, m_per_unit, true)
}

/// Shared body of the bulk-projected transmon solves; `drop_null` selects
/// [`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`].
fn projected_transmon_impl(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    port_model: crate::eigen::transmon::JunctionPortModel,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
    drop_null: bool,
) -> Result<
    (
        Vec<crate::eigen::transmon::TransmonMode>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    use crate::eigen::transmon::{ModeReport, TransmonMode, frequency_hz_from_lambda};

    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );

    // Plain PEC interior reindex (drop excluded edges, compact the rest).
    let mut interior_index = vec![None; n_edges];
    let mut dim = 0usize;
    for (e, &keep) in pencil.interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no interior DOFs after PEC reduction".into(),
        ));
    }

    let pattern = pencil.scatter.pattern();
    assert_eq!(pencil.k_vals.len(), pattern.nnz(), "k_vals length mismatch");
    assert_eq!(pencil.m_vals.len(), pattern.nnz(), "m_vals length mismatch");

    let (k_port, m_port) = port_model.port_triplets(&pencil.shunt, pencil.mesh, pencil.edges);

    let k_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.k_vals,
        &k_port,
        &interior_index,
        dim,
    )?;
    let m_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.m_vals,
        &m_port,
        &interior_index,
        dim,
    )?;
    let k_port_red = assemble_reduced_real(&[], &[], &[], &k_port, &interior_index, dim)?;

    // G = d⁰_interior and the M-orthogonal divergence-free projector.
    let gradient = InteriorGradient::build(
        pencil.edges,
        pencil.interior_mask,
        &interior_index,
        pencil.mesh.n_nodes(),
        dim,
    );
    let projector = MOrthogonalGradientProjector::build(&gradient, m_red.as_ref())?;

    let solver = ProjectedShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-8,
        reproject_threshold: 1e-8,
    };
    let (pairs, diag) = if drop_null {
        solver.smallest_nonnull_eigenpairs(k_red.as_ref(), m_red.as_ref(), &projector, n_modes)?
    } else {
        solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), &projector, n_modes)?
    };

    let modes = pairs
        .into_iter()
        .map(|pair| TransmonMode {
            report: ModeReport {
                lambda: pair.lambda,
                frequency_hz: frequency_hz_from_lambda(pair.lambda, m_per_unit),
                participation: junction_participation(&k_red, &k_port_red, &pair.vector),
            },
            vector: pair.vector,
        })
        .collect();
    Ok((modes, diag))
}

/// Solve the transmon eigenmodes with the **PORT-AWARE divergence-free
/// projection** (issue #514) — the composite solver that removes the bulk
/// gradient nullspace AND retains the 17.49 GHz port-sheet mode, which the
/// bulk-`d⁰` projection of [`solve_transmon_eigenmodes_projected`] deflated
/// away (issue #509). Issue #509 / #514 called that mode the physical
/// junction LC mode; issue #1045 re-reads it as the port-sheet artifact of the
/// distributed shunt (see [`PortAwareGradientProjector`]). Retaining it
/// reproduces Palace's committed `Target = 4.5 GHz` spectrum, which contains it.
///
/// # Composite construction
///
/// 1. Assemble the reduced real pencil `(K + K_port, M + M_port)` and build the
///    bulk `M`-orthogonal gradient projector `P` — exactly as the bulk
///    projected path does.
/// 2. Run one **ungauged** shift-invert Lanczos solve near the junction
///    frequency (`junction_sigma`) and extract the eigenvector `x_j` with the
///    largest junction participation — the 17.49 GHz port-sheet mode.
/// 3. Build the [`PortAwareGradientProjector`] `P' = P + û ûᵀ M`, re-admitting
///    the junction-flux gradient direction `û = (I−P)x_j / ‖·‖_M`.
/// 4. Run projected Lanczos near `sigma` with `P'`. `P'` deflates the whole
///    13,747-mode gradient cluster except `span{û}`, so the cavity spectrum is
///    preserved (as under `P`) AND the port-sheet mode survives.
///
/// The junction-localized 3.4528 GHz mode is genuinely solenoidal
/// (`M`-orthogonal to `image(d⁰)`), so it is untouched by BOTH `P` and the
/// rank-1 re-admission and survives the port-aware projection. It is the
/// transmon qubit mode: Palace returns it at 3.452749 GHz when its `Target` is
/// below it (issue #1043). Before issue #1045 these docs called it a
/// port-formulation artifact, filtered by frequency-matching against the
/// Palace oracle; that reading is withdrawn.
///
/// Exact null modes of `K` (`|λ|` at round-off, e.g. the potential of a
/// port-free floating conductor, which `image(d⁰_interior)` does not
/// contain) are dropped before the `n_modes` cut (issue #950; see
/// [`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`]).
///
/// This route factors two shifted pencils (`K − σ_j M` for the extract and
/// `K − σM` for the band). [`solve_transmon_eigenmodes_port_subspace`] reaches
/// the same physical spectrum with one factorization and no extract.
///
/// Returns the modes (restored frequency + junction participation) and the
/// [`ProjectionDiagnostics`] for the port-aware run. Note the diagnostics'
/// divergence ratios are `O(1)` on the port-sheet mode (its re-admitted flux
/// direction is a gradient) and `≈ 0` on the cavity modes — a `P'`-projected
/// vector is not globally divergence-free by construction.
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly, either projector build,
/// or either Lanczos solve.
pub fn solve_transmon_eigenmodes_port_aware(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    sigma: f64,
    junction_sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<
    (
        Vec<crate::eigen::transmon::ModeReport>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    use crate::eigen::lanczos::SparseShiftInvertLanczos;
    use crate::eigen::transmon::{ModeReport, frequency_hz_from_lambda};

    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );

    // Plain PEC interior reindex (identical to the ungauged / bulk-projected
    // paths — this is a projection, not a DOF elimination).
    let mut interior_index = vec![None; n_edges];
    let mut dim = 0usize;
    for (e, &keep) in pencil.interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no interior DOFs after PEC reduction".into(),
        ));
    }

    let pattern = pencil.scatter.pattern();
    assert_eq!(pencil.k_vals.len(), pattern.nnz(), "k_vals length mismatch");
    assert_eq!(pencil.m_vals.len(), pattern.nnz(), "m_vals length mismatch");

    let k_port = pencil.shunt.k_port_triplets(pencil.mesh, pencil.edges);
    let m_port = pencil.shunt.m_port_triplets(pencil.mesh, pencil.edges);

    let k_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.k_vals,
        &k_port,
        &interior_index,
        dim,
    )?;
    let m_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.m_vals,
        &m_port,
        &interior_index,
        dim,
    )?;
    let k_port_red = assemble_reduced_real(&[], &[], &[], &k_port, &interior_index, dim)?;

    // G = d⁰_interior and the BULK M-orthogonal divergence-free projector.
    let gradient = InteriorGradient::build(
        pencil.edges,
        pencil.interior_mask,
        &interior_index,
        pencil.mesh.n_nodes(),
        dim,
    );
    let bulk = MOrthogonalGradientProjector::build(&gradient, m_red.as_ref())?;

    // --- Step 1: extract the ungauged junction eigenvector near junction_sigma.
    // Run a small ungauged shift-invert solve and pick the eigenvector with the
    // largest junction participation — the 17.49 GHz port-sheet mode whose
    // (near-)gradient flux direction we re-admit.
    let ung = SparseShiftInvertLanczos {
        sigma: junction_sigma,
        max_iters: 96,
        tol: 1e-8,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    let jpairs = ung.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), 6)?;
    let x_junction = jpairs
        .iter()
        .max_by(|a, b| {
            junction_participation(&k_red, &k_port_red, &a.vector)
                .partial_cmp(&junction_participation(&k_red, &k_port_red, &b.vector))
                .unwrap_or(core::cmp::Ordering::Equal)
        })
        .ok_or_else(|| {
            EigenError::FaerGevd("ungauged junction solve returned no eigenpairs".into())
        })?;

    // --- Step 2: build the port-aware projector P' = P + û ûᵀ M. ---
    let port_aware = PortAwareGradientProjector::build(&bulk, &x_junction.vector)?;

    // --- Step 3: port-aware projected Lanczos over the physical band. ---
    // The re-projection-cadence guard keys off the divergence ratio, which is
    // O(1) here (the re-admitted junction direction is a gradient), so disable
    // it (a huge threshold) — P' is idempotent, and its single mandatory pass
    // per step already confines the Krylov space correctly.
    let solver = ProjectedShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-8,
        reproject_threshold: f64::INFINITY,
    };
    // Issue #950: drop exact null modes of K (|λ| at round-off) before the
    // n_modes cut, so the returned slots hold nonzero modes.
    let (pairs, mut diag) =
        solver.smallest_nonnull_eigenpairs(k_red.as_ref(), m_red.as_ref(), &port_aware, n_modes)?;
    // The junction extract factored K − σ_j M; the band solve factored K − σM.
    diag.shifted_factorizations += 1;
    diag.readmitted_gradient_directions = 1;

    let modes = pairs
        .iter()
        .map(|pair| ModeReport {
            lambda: pair.lambda,
            frequency_hz: frequency_hz_from_lambda(pair.lambda, m_per_unit),
            participation: junction_participation(&k_red, &k_port_red, &pair.vector),
        })
        .collect();
    Ok((modes, diag))
}

/// Solve the transmon eigenmodes with the **port-subspace** divergence-free
/// projection (issue #950): the one-factorization, extract-free form of
/// [`solve_transmon_eigenmodes_port_aware`].
///
/// 1. Assemble the reduced real pencil `(K + K_port, M + M_port)` and build
///    the bulk `M`-orthogonal gradient projector `P` (one `GᵀMG` LU).
/// 2. Read the port nodes off the support of `Gᵀ K_port` and build the
///    [`PortSubspaceGradientProjector`] `P_V`, which re-admits the gradient
///    directions `G (GᵀMG)⁻¹ E` that every `λ ≠ 0` eigenvector's gradient
///    part lies in. No eigensolve is needed for this.
/// 3. Run projected shift-invert Lanczos at `sigma` with `P_V` (the only
///    factorization of a shifted pencil), dropping exact null modes of `K`
///    before the `n_modes` cut
///    ([`ProjectedShiftInvertLanczos::smallest_nonnull_eigenpairs`]).
///
/// It returns the 17.49 GHz port-sheet mode and the cavity modes without a
/// junction shift, and without the near-zero survivor of the extract route.
/// The solenoidal 3.45 GHz mode of the transmon fixture is returned too: it
/// is the transmon qubit mode (issue #1043), not a port artifact as these docs
/// said before issue #1045. The port-sheet mode is the non-physical one; the
/// rank-1 port-voltage shunt A2 (`JunctionPortModel::PortVoltage`, issue
/// #1044) removes it.
///
/// Returns the modes (frequency + junction participation, ascending `λ`) and
/// the [`ProjectionDiagnostics`], whose `mode_residual_rels` give each mode's
/// relative eigen-residual and whose `null_*` fields report the filter. If the
/// Krylov basis yields fewer than `n_modes` non-null pairs, fewer modes are
/// returned rather than an error.
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly, the projector builds
/// or the projected Lanczos solve.
pub fn solve_transmon_eigenmodes_port_subspace(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<
    (
        Vec<crate::eigen::transmon::ModeReport>,
        ProjectionDiagnostics,
    ),
    EigenError,
> {
    use crate::eigen::transmon::{ModeReport, frequency_hz_from_lambda};

    let sys = reduced_transmon_system(pencil)?;
    let gradient = InteriorGradient::build(
        pencil.edges,
        pencil.interior_mask,
        &sys.interior_index,
        pencil.mesh.n_nodes(),
        sys.dim,
    );
    let bulk = MOrthogonalGradientProjector::build(&gradient, sys.m_red.as_ref())?;
    let port_nodes = PortSubspaceGradientProjector::port_nodes(&gradient, sys.k_port_red.as_ref());
    let port = PortSubspaceGradientProjector::build(&bulk, &port_nodes)?;

    // P_V carries O(1) divergence on the port-gradient modes by design, so
    // the drift re-projection (keyed on the divergence ratio) is disabled;
    // P_V is idempotent and one pass per step confines the Krylov space.
    let solver = ProjectedShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-8,
        reproject_threshold: f64::INFINITY,
    };
    let (pairs, mut diag) = solver.smallest_nonnull_eigenpairs(
        sys.k_red.as_ref(),
        sys.m_red.as_ref(),
        &port,
        n_modes,
    )?;
    diag.readmitted_gradient_directions = port.n_port_nodes();

    let modes = pairs
        .iter()
        .map(|pair| ModeReport {
            lambda: pair.lambda,
            frequency_hz: frequency_hz_from_lambda(pair.lambda, m_per_unit),
            participation: junction_participation(&sys.k_red, &sys.k_port_red, &pair.vector),
        })
        .collect();
    Ok((modes, diag))
}

/// The reduced real transmon system shared by the projected entry points.
struct ReducedTransmonSystem {
    interior_index: Vec<Option<usize>>,
    dim: usize,
    k_red: SparseColMat<usize, f64>,
    m_red: SparseColMat<usize, f64>,
    k_port_red: SparseColMat<usize, f64>,
}

/// Plain PEC interior reindex plus the reduced `K + K_port`, `M + M_port`
/// and `K_port` (the same reduction as the ungauged path).
fn reduced_transmon_system(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
) -> Result<ReducedTransmonSystem, EigenError> {
    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );
    let mut interior_index = vec![None; n_edges];
    let mut dim = 0usize;
    for (e, &keep) in pencil.interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no interior DOFs after PEC reduction".into(),
        ));
    }
    let pattern = pencil.scatter.pattern();
    assert_eq!(pencil.k_vals.len(), pattern.nnz(), "k_vals length mismatch");
    assert_eq!(pencil.m_vals.len(), pattern.nnz(), "m_vals length mismatch");
    let k_port = pencil.shunt.k_port_triplets(pencil.mesh, pencil.edges);
    let m_port = pencil.shunt.m_port_triplets(pencil.mesh, pencil.edges);
    let k_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.k_vals,
        &k_port,
        &interior_index,
        dim,
    )?;
    let m_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.m_vals,
        &m_port,
        &interior_index,
        dim,
    )?;
    let k_port_red = assemble_reduced_real(&[], &[], &[], &k_port, &interior_index, dim)?;
    Ok(ReducedTransmonSystem {
        interior_index,
        dim,
        k_red,
        m_red,
        k_port_red,
    })
}

/// A single UNGAUGED eigenpair with its divergence diagnostics against the
/// bulk-`d⁰` projector — the measurement vehicle that lets a caller inspect
/// the near-gradient character of a raw (un-projected) eigenvector.
///
/// The [`ModeReport`](crate::eigen::transmon::ModeReport)-carrying entry
/// points drop `EigenPair::vector`, so this struct exposes the two scalar
/// diagnostics the deflation mechanism turns on — computed here against the
/// exact same `M`-orthogonal projector the projected solve uses — without
/// returning the (large) raw vector to the caller.
#[derive(Debug, Clone)]
pub struct UngaugedModeDivergence {
    /// Restored physical frequency (Hz) of the eigenmode.
    pub frequency_hz: f64,
    /// Junction stiffness-participation `p = xᵀK_port x / xᵀ(K+K_port)x`.
    pub participation: f64,
    /// Divergence residual `‖Gᵀ M x‖ / ‖x‖_M` of the UNGAUGED eigenvector.
    /// A near-gradient (junction-flux) mode is `O(1)` here; a genuinely
    /// solenoidal mode (the 3.45 GHz qubit mode, the cavity modes) is `~1e-15`.
    pub divergence_ratio: f64,
    /// M-normalized norm of the projected eigenvector, `‖P x‖_M / ‖x‖_M`.
    /// `≈ 0` for a mode that lives almost entirely in `image(d⁰)` (deflated
    /// away by `P`); `≈ 1` for a divergence-free mode `P` leaves in place.
    pub projected_norm_ratio: f64,
}

/// Solve the UNGAUGED transmon pencil and measure each returned eigenvector's
/// near-gradient character against the bulk-`d⁰` `M`-orthogonal projector
/// (issue #509 deflation-mechanism measurement).
///
/// This is the direct-measurement counterpart to
/// [`solve_transmon_eigenmodes_projected`]: it runs the committed **ungauged**
/// [`SparseShiftInvertLanczos`](crate::eigen::lanczos::SparseShiftInvertLanczos)
/// core (which returns the full [`EigenPair::vector`] the `ModeReport` path
/// discards), then — using the SAME `G = d⁰_interior` and `M`-orthogonal
/// projector `P = I − G(GᵀMG)⁻¹GᵀM` the projected path builds — reports, per
/// mode, the divergence ratio `‖GᵀMx‖/‖x‖_M` and the projected-norm ratio
/// `‖Px‖_M/‖x‖_M`. This exposes *why* `P` deflates the 17.49 GHz port-sheet mode: the
/// junction eigenvector is a near-gradient (curl-free lumped-inductor flux
/// path), so its divergence ratio is `O(1)` and its projected norm ≈ 0 —
/// directly, on the raw eigenvector, rather than inferred from the mode's
/// disappearance in the projected spectrum.
///
/// Returns one [`UngaugedModeDivergence`] per returned mode, in the same
/// (ascending-λ) order as
/// [`solve_transmon_eigenmodes`](crate::eigen::transmon::solve_transmon_eigenmodes).
///
/// # Errors
///
/// Propagates [`EigenError`] from the reduced assembly, the projector build,
/// or the ungauged Lanczos solve.
pub fn ungauged_mode_divergences(
    pencil: &crate::eigen::transmon::TransmonPencil<'_>,
    sigma: f64,
    n_modes: usize,
    m_per_unit: f64,
) -> Result<Vec<UngaugedModeDivergence>, EigenError> {
    use crate::eigen::lanczos::SparseShiftInvertLanczos;
    use crate::eigen::transmon::frequency_hz_from_lambda;

    let n_edges = pencil.edges.len();
    assert_eq!(
        pencil.interior_mask.len(),
        n_edges,
        "interior mask length must equal edge count"
    );

    // Plain PEC interior reindex — the exact reduction the ungauged committed
    // path (`solve_transmon_eigenmodes`) uses.
    let mut interior_index = vec![None; n_edges];
    let mut dim = 0usize;
    for (e, &keep) in pencil.interior_mask.iter().enumerate() {
        if keep {
            interior_index[e] = Some(dim);
            dim += 1;
        }
    }
    if dim == 0 {
        return Err(EigenError::FaerGevd(
            "no interior DOFs after PEC reduction".into(),
        ));
    }

    let pattern = pencil.scatter.pattern();
    assert_eq!(pencil.k_vals.len(), pattern.nnz(), "k_vals length mismatch");
    assert_eq!(pencil.m_vals.len(), pattern.nnz(), "m_vals length mismatch");

    let k_port = pencil.shunt.k_port_triplets(pencil.mesh, pencil.edges);
    let m_port = pencil.shunt.m_port_triplets(pencil.mesh, pencil.edges);

    let k_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.k_vals,
        &k_port,
        &interior_index,
        dim,
    )?;
    let m_red = assemble_reduced_real(
        &pattern.rows,
        &pattern.cols,
        pencil.m_vals,
        &m_port,
        &interior_index,
        dim,
    )?;
    let k_port_red = assemble_reduced_real(&[], &[], &[], &k_port, &interior_index, dim)?;

    // The SAME projector the projected solve builds (bulk d⁰).
    let gradient = InteriorGradient::build(
        pencil.edges,
        pencil.interior_mask,
        &interior_index,
        pencil.mesh.n_nodes(),
        dim,
    );
    let projector = MOrthogonalGradientProjector::build(&gradient, m_red.as_ref())?;

    // Run the UNGAUGED core: it returns EigenPair.vector (which the
    // ModeReport path drops) so we can measure the raw eigenvector directly.
    let solver = SparseShiftInvertLanczos {
        sigma,
        max_iters: 96,
        tol: 1e-8,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
        precond: crate::eigen::lanczos::InnerPreconditioner::Jacobi,
    };
    let pairs = solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), n_modes)?;

    let mut out = Vec::with_capacity(pairs.len());
    for pair in &pairs {
        let x = &pair.vector;
        // M-norm ‖x‖_M = √(xᵀ M x).
        let mx = spmv(m_red.as_ref(), x);
        let m_norm = x
            .iter()
            .zip(mx.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>()
            .max(0.0)
            .sqrt();
        // ‖P x‖_M / ‖x‖_M.
        let mut px = x.clone();
        projector.project_in_place(&mut px)?;
        let mpx = spmv(m_red.as_ref(), &px);
        let px_norm = px
            .iter()
            .zip(mpx.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>()
            .max(0.0)
            .sqrt();
        let projected_norm_ratio = if m_norm > 0.0 { px_norm / m_norm } else { 0.0 };
        out.push(UngaugedModeDivergence {
            frequency_hz: frequency_hz_from_lambda(pair.lambda, m_per_unit),
            participation: junction_participation(&k_red, &k_port_red, x),
            divergence_ratio: projector.divergence_ratio(x),
            projected_norm_ratio,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
