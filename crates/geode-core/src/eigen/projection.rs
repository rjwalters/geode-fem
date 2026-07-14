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
//! show up as a near-zero-λ cluster (λ ≈ 1e-16…1e-17) **plus**,
//! occasionally, a gradient-adjacent mode that leaks *into* the physical
//! band (the transmon benchmark's spurious 3.4528 GHz mode).
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
//! the Ritz problem — the spurious mode and the near-zero cluster are gone
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
        }
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
/// `x.len() == nrows`, `y.len() == ncols`. `Aᵀ` in CSC is a row-walk of the
/// stored columns: column `j` of `A` dotted with `x` is entry `j` of `Aᵀx`.
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
        spmv(self.m, w, &mut mw);
        // rhs = Gᵀ · mw   (node-space)
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), &mw, &mut rhs);
        // y = C⁻¹ rhs
        let mut y_mat: Mat<f64> = Mat::from_fn(self.node_dim, 1, |r, _| rhs[r]);
        self.c_lu.solve_in_place(y_mat.as_mut());
        let y: Vec<f64> = (0..self.node_dim).map(|r| y_mat[(r, 0)]).collect();
        // gy = G · y   (edge-space)
        let mut gy = vec![0.0_f64; self.edge_dim];
        spmv(self.gradient.matrix(), &y, &mut gy);
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
        spmv(self.m, w, &mut mw);
        let m_norm2 = w.iter().zip(mw.iter()).map(|(a, b)| a * b).sum::<f64>();
        if m_norm2 <= 0.0 {
            return 0.0;
        }
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), &mw, &mut rhs);
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
        spmv(self.m, y, &mut my);
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
        spmv(self.m, x, &mut mw);
        // rhs = Gᵀ · mw   (node-space)
        let mut rhs = vec![0.0_f64; self.node_dim];
        spmv_transpose(self.gradient.matrix(), &mw, &mut rhs);
        // y = C⁻¹ rhs
        let mut y_mat: Mat<f64> = Mat::from_fn(self.node_dim, 1, |r, _| rhs[r]);
        self.c_lu.solve_in_place(y_mat.as_mut());
        let y: Vec<f64> = (0..self.node_dim).map(|r| y_mat[(r, 0)]).collect();
        // u = G · y   (edge-space, in image(G))
        let mut u = vec![0.0_f64; self.edge_dim];
        spmv(self.gradient.matrix(), &y, &mut u);
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
/// but it also **deflates the physical junction LC mode**: a lumped inductor is
/// a quasi-static, curl-free flux path, so the junction eigenvector lives
/// almost entirely in `image(d⁰)` (measured projected-norm ratio ≈ 1e-4, i.e.
/// 99.99% gradient). `P` cannot tell that one physical gradient direction from
/// the 13,746 spurious ones and removes it with the rest (issue #509 negative).
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
///   junction LC mode survives;
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
        spmv(bulk.m(), &u, &mut m_u_hat);
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
    /// is expected to be `O(1)` on the junction mode and `≈ 0` on the cavity
    /// modes.
    pub fn divergence_ratio(&self, w: &[f64]) -> f64 {
        self.bulk.divergence_ratio(w)
    }
}

/// `y = A · x` for a CSC sparse matrix, returning a fresh vector.
fn spmv_vec(a: SparseColMatRef<'_, usize, f64>, x: &[f64]) -> Vec<f64> {
    let mut y = vec![0.0_f64; a.nrows()];
    spmv(a, x, &mut y);
    y
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

/// Solve `A y = b` in-place via a precomputed LU factorization.
fn solve_with_lu(lu: &Lu<usize, f64>, rhs: &[f64], out: &mut [f64]) {
    use faer::linalg::solvers::Solve;
    let n = rhs.len();
    let mut work: Mat<f64> = Mat::from_fn(n, 1, |i, _| rhs[i]);
    lu.solve_in_place(work.as_mut());
    for (i, o) in out.iter_mut().enumerate() {
        *o = work[(i, 0)];
    }
}

/// Solve the symmetric tridiagonal eigenproblem returning eigenpairs
/// `(μ, s)` in ascending-μ order (same helper as the un-projected path).
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

/// Tunables for the projected shift-invert Lanczos (mirrors
/// [`crate::eigen::lanczos::SparseShiftInvertLanczos`] with an added
/// re-projection cadence).
#[derive(Debug, Clone, Copy)]
pub struct ProjectedShiftInvertLanczos {
    /// Shift `σ = k²`; Ritz values closest to `σ` converge first.
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
    /// enters the tridiagonal Ritz problem — the spurious mode and near-zero
    /// cluster are absent by construction, while the physical spectrum is
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

        // 1. Factor A = K − σM once.
        let a = shifted_pencil(k, m, self.sigma)?;
        let lu = a
            .as_ref()
            .sp_lu()
            .map_err(|e| EigenError::FaerGevd(format!("sparse LU: {e:?}")))?;

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
        let mut mv = spmv_vec(m, &v);
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

        for j in 0..max_k {
            diag.iterations = j + 1;
            spmv(m, &v, &mut mv);
            solve_with_lu(&lu, &mv, &mut w);

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

            spmv(m, &w, &mut work);
            nrm2 = w.iter().zip(work.iter()).map(|(a, b)| a * b).sum::<f64>();
            let nrm2 = nrm2.max(0.0);
            nrm = nrm2.sqrt();

            m_basis.push(core::mem::take(&mut mv));
            mv = vec![0.0_f64; n];
            basis.push(core::mem::take(&mut v));

            if alpha.len() >= n_modes && alpha.len() >= 2 {
                let (mus, _) = tridiag_eigenpairs(&alpha, &beta)?;
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
        pairs.sort_by(|a, b| {
            (a.0 - sigma)
                .abs()
                .partial_cmp(&(b.0 - sigma).abs())
                .unwrap_or(core::cmp::Ordering::Equal)
        });
        let take = n_modes.min(pairs.len());
        let mut picked: Vec<(f64, Vec<f64>)> = pairs.into_iter().take(take).collect();
        picked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

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
        diag.mode_divergence_ratios = out
            .iter()
            .map(|p| projector.divergence_ratio(&p.vector))
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
    use crate::eigen::transmon::{ModeReport, frequency_hz_from_lambda};

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
    let (pairs, diag) =
        solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), &projector, n_modes)?;

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

/// Solve the transmon eigenmodes with the **PORT-AWARE divergence-free
/// projection** (issue #514) — the composite solver that removes the bulk
/// gradient nullspace AND retains the physical junction LC mode, which the
/// bulk-`d⁰` projection of [`solve_transmon_eigenmodes_projected`] deflated
/// away (issue #509 negative).
///
/// # Composite construction
///
/// 1. Assemble the reduced real pencil `(K + K_port, M + M_port)` and build the
///    bulk `M`-orthogonal gradient projector `P` — exactly as the bulk
///    projected path does.
/// 2. Run one **ungauged** shift-invert Lanczos solve near the junction
///    frequency (`junction_sigma`) and extract the eigenvector `x_j` with the
///    largest junction participation — the physical junction LC mode.
/// 3. Build the [`PortAwareGradientProjector`] `P' = P + û ûᵀ M`, re-admitting
///    the junction-flux gradient direction `û = (I−P)x_j / ‖·‖_M`.
/// 4. Run projected Lanczos near `sigma` with `P'`. `P'` deflates the whole
///    13,747-mode gradient cluster except `span{û}`, so the cavity spectrum is
///    preserved (as under `P`) AND the junction mode survives.
///
/// The port-localized 3.4528 GHz spurious mode is genuinely solenoidal
/// (`M`-orthogonal to `image(d⁰)`), so it is untouched by BOTH `P` and the
/// rank-1 re-admission — it survives the port-aware projection and remains
/// filtered by frequency-matching against the Palace oracle (see the issue
/// #514 characterization: it is a port-formulation artifact, not a
/// bulk-gradient one).
///
/// Returns the modes (restored frequency + junction participation) and the
/// [`ProjectionDiagnostics`] for the port-aware run. Note the diagnostics'
/// divergence ratios are `O(1)` on the junction mode (its re-admitted flux
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
    // largest junction participation — the physical junction LC mode whose
    // (near-)gradient flux direction we re-admit.
    let ung = SparseShiftInvertLanczos {
        sigma: junction_sigma,
        max_iters: 96,
        tol: 1e-8,
        inner: crate::eigen::lanczos::InnerSolver::Direct,
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
    let (pairs, diag) =
        solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), &port_aware, n_modes)?;

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
    /// solenoidal (spurious port-localized) mode is `~1e-15`.
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
/// `‖Px‖_M/‖x‖_M`. This exposes *why* `P` deflates the junction LC mode: the
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
    };
    let pairs = solver.smallest_eigenpairs(k_red.as_ref(), m_red.as_ref(), n_modes)?;

    let mut out = Vec::with_capacity(pairs.len());
    for pair in &pairs {
        let x = &pair.vector;
        // M-norm ‖x‖_M = √(xᵀ M x).
        let mx = spmv_vec(m_red.as_ref(), x);
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
        let mpx = spmv_vec(m_red.as_ref(), &px);
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

/// Build a real reduced sparse matrix from a `[nnz]` value slice aligned to
/// the volume sparsity pattern, restricted to the interior DOFs, with extra
/// surface triplets summed on top. Mirrors the private helper in
/// [`crate::eigen::transmon`] (the projected entry point reuses the exact
/// same reduction so `K_red`/`M_red` match the ungauged path bit-for-bit).
fn assemble_reduced_real(
    pattern_rows: &[u32],
    pattern_cols: &[u32],
    vals: &[f64],
    extra: &[(usize, usize, f64)],
    interior_index: &[Option<usize>],
    dim: usize,
) -> Result<SparseColMat<usize, f64>, EigenError> {
    let mut trips: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(vals.len() + extra.len());
    for ((&r, &c), &v) in pattern_rows
        .iter()
        .zip(pattern_cols.iter())
        .zip(vals.iter())
    {
        if let (Some(ri), Some(ci)) = (interior_index[r as usize], interior_index[c as usize]) {
            trips.push(Triplet::new(ri, ci, v));
        }
    }
    for &(r, c, v) in extra {
        if let (Some(ri), Some(ci)) = (interior_index[r], interior_index[c]) {
            trips.push(Triplet::new(ri, ci, v));
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("reduced sparse assembly: {e:?}")))
}

/// Junction participation `p = (xᵀ K_port x) / (xᵀ (K + K_port) x)`, clamped
/// to `[0, 1]`. Mirrors the private metric in [`crate::eigen::transmon`].
fn junction_participation(
    k_total: &SparseColMat<usize, f64>,
    k_port: &SparseColMat<usize, f64>,
    x: &[f64],
) -> f64 {
    let num = quad_form(k_port, x);
    let den = quad_form(k_total, x);
    if den <= 0.0 {
        return 0.0;
    }
    (num / den).clamp(0.0, 1.0)
}

/// Quadratic form `xᵀ A x` for a CSC sparse matrix.
fn quad_form(a: &SparseColMat<usize, f64>, x: &[f64]) -> f64 {
    let col_ptr = a.col_ptr();
    let row_idx = a.row_idx();
    let val = a.val();
    let mut acc = 0.0;
    for j in 0..a.ncols() {
        let xj = x[j];
        if xj == 0.0 {
            continue;
        }
        for k in col_ptr[j]..col_ptr[j + 1] {
            acc += x[row_idx[k]] * val[k] * xj;
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::nedelec::{rank_via_svd, restrict_gradient_dense};
    use crate::mesh::{TetMesh, cube_tet_mesh};
    use faer::sparse::Triplet;

    /// Interior-node mask companion to a PEC edge mask: a node is interior
    /// iff it is NOT an endpoint of any excluded edge.
    fn interior_node_mask(edges: &[[u32; 2]], interior_mask: &[bool], n_nodes: usize) -> Vec<bool> {
        let mut grounded = vec![false; n_nodes];
        for (e, &keep) in interior_mask.iter().enumerate() {
            if !keep {
                grounded[edges[e][0] as usize] = true;
                grounded[edges[e][1] as usize] = true;
            }
        }
        grounded.iter().map(|&g| !g).collect()
    }

    /// The plain PEC interior reindex (drop excluded edges, compact the rest).
    fn pec_reindex(interior_mask: &[bool]) -> (Vec<Option<usize>>, usize) {
        let mut idx = vec![None; interior_mask.len()];
        let mut dim = 0usize;
        for (e, &keep) in interior_mask.iter().enumerate() {
            if keep {
                idx[e] = Some(dim);
                dim += 1;
            }
        }
        (idx, dim)
    }

    fn full_outer_pec(mesh: &TetMesh) -> Vec<bool> {
        let edges = mesh.edges();
        let metal: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| {
                let on = |c: usize, v: f64| {
                    f.iter()
                        .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12)
                };
                on(0, 0.0) || on(0, 1.0) || on(1, 0.0) || on(1, 1.0) || on(2, 0.0) || on(2, 1.0)
            })
            .collect();
        crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()])
    }

    /// The sparse `G = d⁰_interior` must have the same column rank as the
    /// dense diagnostic operator `restrict_gradient_dense` — bit-exact rank
    /// match with the de-Rham gradient rank (the acceptance-tied structural
    /// check, analogous to `gauge::tree_edge_count_matches_derham_rank`).
    #[test]
    fn sparse_gradient_node_dim_matches_derham_rank() {
        for n in [2usize, 3, 4] {
            let mesh = cube_tet_mesh(n, 1.0);
            let edges = mesh.edges();
            let n_nodes = mesh.n_nodes();
            let interior_mask = full_outer_pec(&mesh);
            let (edge_index, edge_dim) = pec_reindex(&interior_mask);

            let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

            let node_mask = interior_node_mask(&edges, &interior_mask, n_nodes);
            let d0 = restrict_gradient_dense(&mesh, &interior_mask, &node_mask);
            let rank = rank_via_svd(&d0, 1e-12);

            // Free-node count == number of interior nodes == d⁰ columns.
            assert_eq!(
                g.node_dim(),
                node_mask.iter().filter(|&&b| b).count(),
                "n={n}: G column count must equal free-interior-node count"
            );
            // On a connected boundary-touching mesh, d⁰_interior has full
            // column rank, so rank == node_dim.
            assert_eq!(
                g.node_dim(),
                rank,
                "n={n}: G node_dim {} must equal rank(d⁰_interior) {rank}",
                g.node_dim()
            );
            assert_eq!(g.edge_dim(), edge_dim);
        }
    }

    /// The sparse `G` is entry-for-entry the sparse form of the dense
    /// `restrict_gradient_dense` operator (same ±1 incidence, same reindex).
    #[test]
    fn sparse_gradient_matches_dense_entries() {
        let mesh = cube_tet_mesh(3, 1.0);
        let edges = mesh.edges();
        let n_nodes = mesh.n_nodes();
        let interior_mask = full_outer_pec(&mesh);
        let (edge_index, edge_dim) = pec_reindex(&interior_mask);
        let node_mask = interior_node_mask(&edges, &interior_mask, n_nodes);

        let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);
        let dense = restrict_gradient_dense(&mesh, &interior_mask, &node_mask);

        // Densify the sparse G and compare bit-for-bit.
        let gm = g.matrix();
        let mut sparse_dense = vec![0.0_f64; g.edge_dim() * g.node_dim()];
        let cp = gm.col_ptr();
        let ri = gm.row_idx();
        let val = gm.val();
        for col in 0..gm.ncols() {
            for kk in cp[col]..cp[col + 1] {
                sparse_dense[ri[kk] * g.node_dim() + col] += val[kk];
            }
        }
        assert_eq!(dense.nrows(), g.edge_dim());
        assert_eq!(dense.ncols(), g.node_dim());
        let mut max_diff = 0.0_f64;
        for r in 0..g.edge_dim() {
            for c in 0..g.node_dim() {
                max_diff = max_diff.max((dense[(r, c)] - sparse_dense[r * g.node_dim() + c]).abs());
            }
        }
        assert!(
            max_diff < 1e-15,
            "sparse G differs from dense d⁰: {max_diff}"
        );
    }

    /// The projector annihilates gradient fields: `‖P (G y)‖_M / ‖G y‖_M ≈ 0`
    /// for random `y` (with `M = I`, `‖·‖_M = ‖·‖₂`), and is idempotent on a
    /// generic vector (`‖P(Pw) − Pw‖ ≈ 0`). This is the core spectral claim.
    #[test]
    fn projector_annihilates_gradients_and_is_idempotent() {
        let mesh = cube_tet_mesh(3, 1.0);
        let edges = mesh.edges();
        let n_nodes = mesh.n_nodes();
        let interior_mask = full_outer_pec(&mesh);
        let (edge_index, edge_dim) = pec_reindex(&interior_mask);
        let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

        // M = identity on the reduced edge space (SPD, keeps the algebra
        // transparent: P then projects off image(G) in the ℓ² sense).
        let ident: Vec<Triplet<usize, usize, f64>> =
            (0..edge_dim).map(|i| Triplet::new(i, i, 1.0)).collect();
        let m =
            SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, edge_dim, &ident).unwrap();
        let proj = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();

        // Gradient field g_field = G · y for deterministic y.
        let y: Vec<f64> = (0..g.node_dim())
            .map(|i| (((i as f64) + 1.0) * 0.913).sin())
            .collect();
        let mut g_field = spmv_vec(g.matrix(), &y);
        let gnorm = g_field.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(gnorm > 1e-6, "gradient field must be nonzero");
        let mut projected = g_field.clone();
        proj.project_in_place(&mut projected).unwrap();
        let resid = projected.iter().map(|x| x * x).sum::<f64>().sqrt();
        assert!(
            resid / gnorm < 1e-8,
            "P must annihilate gradients: ‖P(Gy)‖/‖Gy‖ = {}",
            resid / gnorm
        );

        // Idempotence on a generic vector.
        let mut w: Vec<f64> = (0..edge_dim)
            .map(|i| (((i as f64) + 2.0) * 0.377).cos())
            .collect();
        proj.project_in_place(&mut w).unwrap();
        let pw = w.clone();
        proj.project_in_place(&mut w).unwrap();
        let diff = w
            .iter()
            .zip(pw.iter())
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        let pwn = pw.iter().map(|x| x * x).sum::<f64>().sqrt().max(1e-30);
        assert!(
            diff / pwn < 1e-8,
            "P not idempotent: ‖P²w − Pw‖/‖Pw‖ = {}",
            diff / pwn
        );
        // reuse g_field to silence the unused-mut lint path.
        g_field[0] = 0.0;
    }

    /// On a spurious-free small pencil (a plain SPD diagonal pencil with
    /// `G` having no rows in common with the spectrum), the projected
    /// eigenvalues match the un-projected reference. Here we use a trivial
    /// `G` with a single free node touching one edge, so `P` removes exactly
    /// that one direction; the remaining eigenvalues are unchanged.
    #[test]
    fn projected_matches_unprojected_on_physical_modes() {
        // Diagonal pencil λ_i = {1,2,3,4,5}, M = I.
        let n = 5usize;
        let tk: Vec<Triplet<usize, usize, f64>> =
            (0..n).map(|i| Triplet::new(i, i, (i + 1) as f64)).collect();
        let tm: Vec<Triplet<usize, usize, f64>> = (0..n).map(|i| Triplet::new(i, i, 1.0)).collect();
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();

        // G maps one free node to edge 0 only (a single gradient direction
        // e_0). P will remove the λ=1 eigenpair (localized at index 0),
        // leaving {2,3,4,5}.
        let g_trips = vec![Triplet::new(0usize, 0usize, 1.0)];
        let g_mat = SparseColMat::<usize, f64>::try_new_from_triplets(n, 1, &g_trips).unwrap();
        let gradient = InteriorGradient {
            g: g_mat,
            edge_dim: n,
            node_dim: 1,
        };
        let proj = MOrthogonalGradientProjector::build(&gradient, m.as_ref()).unwrap();

        let solver = ProjectedShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
            reproject_threshold: 1e-8,
        };
        let (pairs, diag) = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), &proj, 3)
            .unwrap();
        assert_eq!(pairs.len(), 3);
        // The λ=1 mode (in image(G)) is projected out; the smallest three
        // physical eigenvalues are now {2,3,4}.
        for (got, want) in pairs.iter().zip([2.0, 3.0, 4.0].iter()) {
            assert!(
                (got.lambda - want).abs() < 1e-7,
                "projected λ = {}, want {want}",
                got.lambda
            );
        }
        // Post-projection divergence stays at machine level.
        assert!(
            diag.max_post_projection_divergence < 1e-6,
            "post-projection divergence too large: {}",
            diag.max_post_projection_divergence
        );
    }

    /// The port-aware projector `P' = P + û ûᵀ M` is a genuine `M`-orthogonal
    /// projector that (a) still annihilates gradients M-orthogonal to the
    /// re-admitted direction, (b) is the identity on the divergence-free
    /// subspace, (c) is the identity on the re-admitted direction `û`, and
    /// (d) is idempotent — the algebra behind construction (b2) of issue #514,
    /// verified on a small non-identity-`M` cube fixture.
    #[test]
    fn port_aware_projector_readmits_one_gradient_direction() {
        let mesh = cube_tet_mesh(3, 1.0);
        let edges = mesh.edges();
        let n_nodes = mesh.n_nodes();
        let interior_mask = full_outer_pec(&mesh);
        let (edge_index, edge_dim) = pec_reindex(&interior_mask);
        let g = InteriorGradient::build(&edges, &interior_mask, &edge_index, n_nodes, edge_dim);

        // A non-trivial SPD M so the M-orthogonality is a real constraint (not
        // the ℓ² special case): M = I + 0.25 · (deterministic banded SPD).
        let mut m_trips: Vec<Triplet<usize, usize, f64>> = Vec::new();
        for i in 0..edge_dim {
            m_trips.push(Triplet::new(i, i, 1.0 + 0.5 * ((i % 5) as f64) / 5.0));
            if i + 1 < edge_dim {
                m_trips.push(Triplet::new(i, i + 1, 0.05));
                m_trips.push(Triplet::new(i + 1, i, 0.05));
            }
        }
        let m = SparseColMat::<usize, f64>::try_new_from_triplets(edge_dim, edge_dim, &m_trips)
            .unwrap();
        let bulk = MOrthogonalGradientProjector::build(&g, m.as_ref()).unwrap();

        let m_inner = |x: &[f64], y: &[f64]| -> f64 {
            let my = spmv_vec(m.as_ref(), y);
            x.iter().zip(my.iter()).map(|(a, b)| a * b).sum::<f64>()
        };
        let m_norm = |x: &[f64]| m_inner(x, x).sqrt();

        // Synthetic "junction eigenvector": mostly a gradient field G·a plus a
        // tiny solenoidal remainder (mirrors the real junction mode's
        // projected-norm ratio ≈ 1e-4).
        let a: Vec<f64> = (0..g.node_dim())
            .map(|i| (((i as f64) + 1.0) * 0.771).sin())
            .collect();
        let grad = spmv_vec(g.matrix(), &a);
        let mut sol: Vec<f64> = (0..edge_dim)
            .map(|i| (((i as f64) + 3.0) * 0.281).cos())
            .collect();
        bulk.project_in_place(&mut sol).unwrap(); // make it divergence-free
        let eps = 1e-4 * m_norm(&grad) / m_norm(&sol);
        let x_junction: Vec<f64> = grad
            .iter()
            .zip(sol.iter())
            .map(|(a, b)| a + eps * b)
            .collect();

        let pa = PortAwareGradientProjector::build(&bulk, &x_junction).unwrap();
        let uhat = pa.u_hat.clone();

        // (c) P' is the identity on û.
        {
            let mut w = uhat.clone();
            pa.project_in_place(&mut w).unwrap();
            let diff = w
                .iter()
                .zip(uhat.iter())
                .map(|(a, b)| a - b)
                .collect::<Vec<_>>();
            assert!(
                m_norm(&diff) / m_norm(&uhat) < 1e-8,
                "P' must retain the re-admitted direction û: {}",
                m_norm(&diff) / m_norm(&uhat)
            );
        }

        // (b) P' is the identity on the divergence-free subspace.
        {
            let mut s: Vec<f64> = (0..edge_dim)
                .map(|i| (((i as f64) + 7.0) * 0.517).sin())
                .collect();
            bulk.project_in_place(&mut s).unwrap();
            let mut w = s.clone();
            pa.project_in_place(&mut w).unwrap();
            let diff: Vec<f64> = w.iter().zip(s.iter()).map(|(a, b)| a - b).collect();
            assert!(
                m_norm(&diff) / m_norm(&s).max(1e-30) < 1e-8,
                "P' must be the identity on divergence-free fields: {}",
                m_norm(&diff) / m_norm(&s).max(1e-30)
            );
        }

        // (a) P' annihilates a gradient M-orthogonal to û. Use a DIFFERENT
        // gradient direction (independent `b`) so removing the û-component
        // leaves a substantial residual gradient (not a near-zero vector).
        {
            let b: Vec<f64> = (0..g.node_dim())
                .map(|i| (((i as f64) + 4.0) * 1.213).cos())
                .collect();
            let mut g_perp = spmv_vec(g.matrix(), &b);
            // remove the û-component: g_perp ← g_perp − (ûᵀ M g_perp) û
            let c = m_inner(&uhat, &g_perp);
            for (gi, &ui) in g_perp.iter_mut().zip(uhat.iter()) {
                *gi -= c * ui;
            }
            let g_norm = m_norm(&g_perp);
            let mut w = g_perp.clone();
            pa.project_in_place(&mut w).unwrap();
            assert!(
                m_norm(&w) / g_norm.max(1e-30) < 1e-7,
                "P' must annihilate gradients M-orthogonal to û: {}",
                m_norm(&w) / g_norm.max(1e-30)
            );
        }

        // (d) P' is idempotent.
        {
            let mut w: Vec<f64> = (0..edge_dim)
                .map(|i| (((i as f64) + 2.0) * 0.333).cos())
                .collect();
            pa.project_in_place(&mut w).unwrap();
            let pw = w.clone();
            pa.project_in_place(&mut w).unwrap();
            let diff: Vec<f64> = w.iter().zip(pw.iter()).map(|(a, b)| a - b).collect();
            assert!(
                m_norm(&diff) / m_norm(&pw).max(1e-30) < 1e-8,
                "P' not idempotent: {}",
                m_norm(&diff) / m_norm(&pw).max(1e-30)
            );
        }

        // The re-admitted direction retains almost the whole junction vector:
        // ‖P' x_junction‖_M / ‖x_junction‖_M ≈ 1 (contrast the bulk projector,
        // which deflates it to ≈ 0).
        let mut px = x_junction.clone();
        pa.project_in_place(&mut px).unwrap();
        let retained = m_norm(&px) / m_norm(&x_junction);
        assert!(
            retained > 0.999,
            "P' should retain the junction eigenvector, got ‖P'x‖/‖x‖ = {retained}"
        );
        let mut bx = x_junction.clone();
        bulk.project_in_place(&mut bx).unwrap();
        let bulk_retained = m_norm(&bx) / m_norm(&x_junction);
        assert!(
            bulk_retained < 1e-2,
            "bulk P should deflate the junction eigenvector, got {bulk_retained}"
        );
    }

    /// End-to-end: on a diagonal pencil whose λ=1 mode is a pure gradient
    /// (`e₀ ∈ image(G)`), the bulk projected solve deflates it (returns
    /// {2,3,4}), but the PORT-AWARE solve — handed `e₀` as the "junction"
    /// eigenvector — RE-ADMITS it and returns {1,2,3}. This is the synthetic
    /// analogue of retaining the transmon junction LC mode.
    #[test]
    fn port_aware_solve_retains_the_gradient_mode() {
        let n = 5usize;
        let tk: Vec<Triplet<usize, usize, f64>> =
            (0..n).map(|i| Triplet::new(i, i, (i + 1) as f64)).collect();
        let tm: Vec<Triplet<usize, usize, f64>> = (0..n).map(|i| Triplet::new(i, i, 1.0)).collect();
        let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
        let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();

        // G maps one free node to edge 0 (gradient direction e₀ ↔ λ=1 mode).
        let g_trips = vec![Triplet::new(0usize, 0usize, 1.0)];
        let g_mat = SparseColMat::<usize, f64>::try_new_from_triplets(n, 1, &g_trips).unwrap();
        let gradient = InteriorGradient {
            g: g_mat,
            edge_dim: n,
            node_dim: 1,
        };
        let bulk = MOrthogonalGradientProjector::build(&gradient, m.as_ref()).unwrap();

        // The "junction" eigenvector is the λ=1 eigenvector e₀ (pure gradient).
        let mut x_junction = vec![0.0_f64; n];
        x_junction[0] = 1.0;
        let pa = PortAwareGradientProjector::build(&bulk, &x_junction).unwrap();

        let solver = ProjectedShiftInvertLanczos {
            sigma: 0.0,
            max_iters: 50,
            tol: 1e-10,
            reproject_threshold: f64::INFINITY,
        };
        let (pairs, _diag) = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), &pa, 3)
            .unwrap();
        assert_eq!(pairs.len(), 3);
        // The λ=1 gradient mode is RE-ADMITTED; smallest three are {1,2,3}.
        for (got, want) in pairs.iter().zip([1.0, 2.0, 3.0].iter()) {
            assert!(
                (got.lambda - want).abs() < 1e-7,
                "port-aware λ = {}, want {want} (the gradient mode must be re-admitted)",
                got.lambda
            );
        }
    }
}
