//! Matrix-free first-order Nédélec curl-curl + mass **matvec** on Burn
//! (#302 Phase 1).
//!
//! The assembled path in [`crate::assembly::nedelec`] scatters the signed
//! per-element `[n_elem, 6, 6]` local stiffness/mass into a global operator
//! (dense `[n_edges, n_edges]` or `[nnz]` CSR-style value tensor) and then
//! a downstream `spmv`/matmul applies it to a vector. This module skips the
//! global assembly entirely: it applies `y = A · x` element-locally, with
//! **no global CSR ever formed**, on Burn tensors, batched over all elements
//! at once. This is the "apply without materializing" composition the #302
//! GPU-resident-solve design (`Gᵀ ∘ B_𝒟ᵀ ∘ D ∘ B_𝒟 ∘ G`) is built on: the
//! dense `[n_edges²]` Burn operator is a non-starter at the 54 k-edge
//! benchmark scale (~23 GB), while the matrix-free apply only ever holds the
//! `[n_elem, 6, 6]` locals plus `[n_edges]` vectors.
//!
//! # Apply structure (rank-structured, element-axis batched)
//!
//! For a global edge-DOF vector `x ∈ ℝ^{n_edges}`, one `A · x` is:
//!
//! 1. **Gather** `G`: pull each tet's six edge DOFs from `x` into a batched
//!    `[n_elem, 6, 1]` stack, using the same `tet_edge_idx` global-edge
//!    indices the assembler scatters through.
//! 2. **Local apply** `D`: a batched `[n_elem, 6, 6] · [n_elem, 6, 1]` matmul
//!    against the **signed** local matrix `\tilde{A}^e_{ij} = s^e_i s^e_j
//!    A^e_{ij}` — the exact orientation-sign outer product
//!    ([`sign_outer`](crate::assembly::nedelec)) the assembled path folds in
//!    before scatter, so the two agree.
//! 3. **Scatter-add** `Gᵀ`: accumulate the `[n_elem, 6]` local results back
//!    into a zero `[n_edges]` vector with a 1-D `scatter(0, …, Add)`, the
//!    transpose of the gather and the same primitive the assembler uses.
//!
//! Because both the assembled operator and this apply fold the *same* signed
//! local matrices through the *same* edge-DOF numbering, `matrix_free(x)`
//! equals `assembled_A · x` to round-off — validated to ~1e-12 on the
//! ndarray-f64 backend in `tests/nedelec_matrix_free_equivalence.rs`.
//!
//! # Dirichlet / interior-DOF convention
//!
//! The assembled driven/eigen paths reduce to an **interior** submatrix by
//! deleting the rows/columns of PEC-constrained (boundary) edge DOFs. The
//! matrix-free apply cannot slice a submatrix out of an un-materialized
//! operator, so it reproduces the same reduction by **masking at gather and
//! scatter time**: constrained DOFs are zeroed in the input before the local
//! apply (kills the constrained *columns*) and zeroed in the output after
//! scatter (kills the constrained *rows*). The result is the interior
//! operator embedded back into the full `[n_edges]` space — i.e. it agrees
//! with the assembled full-space matrix restricted to interior DOFs, and is
//! zero on constrained DOFs. See [`MatrixFreeNedelecOperator::with_mask`] and
//! the masking unit test.
//!
//! # Precision / backend note
//!
//! The f64 conformance bar runs on the ndarray backend. `burn-cuda 0.21` has
//! **no f64** (cubecl disables it), so the CUDA leg is a feature-gated f32
//! smoke test only, deferred to the rented box — see the equivalence test
//! file. Nothing here enables CUDA in a default build.

use bunsen::contracts::{assert_shape_contract, define_shape_contract, unpack_shape_contract};
use burn::tensor::backend::Backend;
use burn::tensor::{IndexingUpdateOp, Int, Tensor, TensorData};

use crate::assembly::p1::gather_tet_coords;
use crate::elements::nedelec::batched_nedelec_local_matrices;

/// Number of vertices per linear tetrahedron. Bound into
/// [`MATVEC_CONNECTIVITY_CONTRACT`] as `nodes_per_tet`.
const NODES_PER_TET: usize = 4;

/// Number of local edge DOFs of a first-order Nédélec tetrahedron (its 6
/// edges). Bound into [`MATVEC_LOCAL_MATRIX_CONTRACT`] as `edges_per_tet`.
const EDGES_PER_TET: usize = 6;

// ---------------------------------------------------------------------------
// Named static shape contracts (Bunsen — mirrors the assembly/nedelec set)
// ---------------------------------------------------------------------------

// Connectivity table `T \in \mathbb{Z}^{n_elem × 4}` — the four global node
// indices of every tet. `nodes_per_tet` is bound to `NODES_PER_TET` (`= 4`).
define_shape_contract!(MATVEC_CONNECTIVITY_CONTRACT, ["n_elem", "nodes_per_tet"]);

// Signed element-local edge-matrix stack `\tilde{A}^{local} \in
// \mathbb{R}^{n_elem × 6 × 6}`: one `6×6` dense local operator per tet after
// the orientation-sign outer product `s_i s_j` is folded in. Both
// `edges_per_tet` axes are bound to `EDGES_PER_TET` (`= 6`).
define_shape_contract!(
    MATVEC_LOCAL_MATRIX_CONTRACT,
    ["n_elem", "edges_per_tet", "edges_per_tet"]
);

// Per-tet gathered / applied edge-DOF column stack `\in \mathbb{R}^{n_elem ×
// 6 × 1}` — the batched right-hand side (gather) and result (local apply) of
// the `[n_elem, 6, 6] · [n_elem, 6, 1]` matmul. `edges_per_tet` bound to 6,
// the trailing column axis to 1.
define_shape_contract!(
    MATVEC_ELEM_COLUMN_CONTRACT,
    ["n_elem", "edges_per_tet", "one"]
);

// Global edge-DOF vector `x \in \mathbb{R}^{n_edges}` — the matvec operand
// and result. `n_edges` is left free; the operator checks the supplied vector
// length against its own `n_edges` explicitly (a scalar, not a tensor axis).
define_shape_contract!(MATVEC_GLOBAL_VECTOR_CONTRACT, ["n_edges"]);

/// Matrix-free first-order Nédélec curl-curl + mass operator on a fixed mesh.
///
/// Holds the precomputed **signed** element-local matrices (stiffness `K` and
/// mass `M`, each `[n_elem, 6, 6]` with the `s_i s_j` orientation signs
/// folded in) and the gather/scatter edge-DOF numbering, so repeated
/// [`apply_k`](Self::apply_k) / [`apply_m`](Self::apply_m) /
/// [`apply_combination`](Self::apply_combination) calls (e.g. a Krylov loop)
/// reuse them without re-running the batched local kernel.
///
/// No global CSR / dense operator is ever formed — the memory footprint is
/// `O(n_elem · 36 + n_edges)`, not `O(n_edges²)` or `O(nnz)`.
#[derive(Debug, Clone)]
pub struct MatrixFreeNedelecOperator<B: Backend> {
    /// Signed local curl-curl stiffness `[n_elem, 6, 6]` (signs folded in).
    k_signed: Tensor<B, 3>,
    /// Signed local mass `[n_elem, 6, 6]` (signs folded in).
    m_signed: Tensor<B, 3>,
    /// Flattened per-`(element, local-edge)` global edge index `[n_elem * 6]`,
    /// used by both the gather (`select(0, …)`) and the scatter-add
    /// (`scatter(0, …, Add)`).
    edge_idx_flat: Tensor<B, 1, Int>,
    /// Optional interior-DOF mask `[n_edges]` (`1.0` interior, `0.0`
    /// constrained). `None` means the full-space (unconstrained) operator.
    mask: Option<Tensor<B, 1>>,
    /// Size of the global linear system (number of edge DOFs).
    n_edges: usize,
    /// Number of elements — cached for the batched reshape.
    n_elem: usize,
    /// Device the operator tensors live on.
    device: B::Device,
}

impl<B: Backend> MatrixFreeNedelecOperator<B> {
    /// Build the operator from a mesh (node coords + connectivity) and its
    /// per-tet edge index/sign tables, scaling the mass by a per-element
    /// relative permittivity `epsilon_r`.
    ///
    /// The stiffness carries no material weight (the curl-curl integrand is
    /// permittivity-independent); the mass is scaled per element by
    /// `epsilon_r[e]` — exactly the
    /// [`assemble_global_nedelec_with_epsilon`](crate::assembly::nedelec::assemble_global_nedelec_with_epsilon)
    /// convention, so the two paths' operators match.
    ///
    /// # Arguments
    ///
    /// * `nodes` — `[n_nodes, 3]` global node coordinates.
    /// * `tets` — `[n_elem, 4]` connectivity (0-based node indices).
    /// * `tet_edge_idx` — `[n_elem, 6]` global edge index per local edge
    ///   (from [`crate::mesh::TetMesh::tet_edges`]).
    /// * `tet_edge_sign` — `[n_elem, 6]` per-DOF orientation sign in
    ///   `{-1, +1}`.
    /// * `n_edges` — size of the global linear system.
    /// * `epsilon_r` — per-element relative permittivity `[n_elem]`
    ///   multiplying the mass. Pass all-`1.0` for the vacuum operator.
    ///
    /// # Panics
    ///
    /// Panics on any length mismatch between `tets`, the edge tables, and
    /// `epsilon_r`, or if `tets` is not `[*, 4]`.
    pub fn new(
        nodes: Tensor<B, 2>,
        tets: Tensor<B, 2, Int>,
        tet_edge_idx: &[[u32; 6]],
        tet_edge_sign: &[[i8; 6]],
        n_edges: usize,
        epsilon_r: &[f64],
    ) -> Self {
        let device = nodes.device();
        // Connectivity `T \in \mathbb{Z}^{n_elem × 4}` — extract `n_elem` and
        // check the 4-vertex arity through the shared named contract.
        let [n_elem] = unpack_shape_contract!(
            MATVEC_CONNECTIVITY_CONTRACT,
            &tets,
            &["n_elem"],
            &[("nodes_per_tet", NODES_PER_TET)],
        );
        assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
        assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
        assert_eq!(epsilon_r.len(), n_elem, "epsilon_r length mismatch");

        // Element-local stiffness and mass (sign-unaware).
        let coords = gather_tet_coords(nodes, tets);
        let local = batched_nedelec_local_matrices(coords);

        // Scale mass by per-element epsilon_r (broadcast over the 6×6 block) —
        // identical f32 upload to the assembled `_with_epsilon` path.
        let eps_flat: Vec<f32> = epsilon_r.iter().map(|&e| e as f32).collect();
        let eps_3d = Tensor::<B, 1>::from_data(TensorData::new(eps_flat, [n_elem]), &device)
            .unsqueeze_dim::<2>(1)
            .unsqueeze_dim::<3>(2); // [n_elem, 1, 1]
        let m_local_scaled = local.m_local.mul(eps_3d);

        // Orientation-sign outer product `s_i s_j`, `[n_elem, 6, 6]` — same
        // arithmetic (f32 upload, broadcast multiply) as the assembler, so the
        // signed locals agree bit-for-bit with the assembled path.
        let sign_flat: Vec<f32> = tet_edge_sign
            .iter()
            .flat_map(|row| row.iter().map(|&s| s as f32))
            .collect();
        let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
        let sign_row = sign_2d.clone().unsqueeze_dim::<3>(2); // [n_elem, 6, 1]
        let sign_col = sign_2d.unsqueeze_dim::<3>(1); // [n_elem, 1, 6]
        let sign_outer = sign_row.mul(sign_col); // [n_elem, 6, 6]

        let k_signed = local.k_local.mul(sign_outer.clone());
        let m_signed = m_local_scaled.mul(sign_outer);

        // Flatten the per-(element, local-edge) global edge indices to a single
        // `[n_elem * 6]` Int tensor, reused by every gather and scatter.
        let idx_flat: Vec<i32> = tet_edge_idx
            .iter()
            .flat_map(|row| row.iter().map(|&e| e as i32))
            .collect();
        let edge_idx_flat =
            Tensor::<B, 1, Int>::from_data(TensorData::new(idx_flat, [n_elem * 6]), &device);

        Self {
            k_signed,
            m_signed,
            edge_idx_flat,
            mask: None,
            n_edges,
            n_elem,
            device,
        }
    }

    /// Attach an interior-DOF mask `[n_edges]` (`true` = interior/kept,
    /// `false` = PEC-constrained/eliminated).
    ///
    /// With a mask attached, every [`apply_*`](Self::apply_k) result is the
    /// **interior** operator embedded in the full `[n_edges]` space: the
    /// operand is zeroed on constrained DOFs before the local apply (deletes
    /// constrained columns) and the result is zeroed on constrained DOFs after
    /// scatter (deletes constrained rows). This matches the row/column
    /// deletion the assembled driven/eigen paths perform to obtain their
    /// interior submatrix.
    ///
    /// # Panics
    ///
    /// Panics if `interior_mask.len() != n_edges`.
    #[must_use]
    pub fn with_mask(mut self, interior_mask: &[bool]) -> Self {
        assert_eq!(
            interior_mask.len(),
            self.n_edges,
            "interior_mask length must equal n_edges"
        );
        let mask_flat: Vec<f32> = interior_mask
            .iter()
            .map(|&keep| if keep { 1.0 } else { 0.0 })
            .collect();
        let mask =
            Tensor::<B, 1>::from_data(TensorData::new(mask_flat, [self.n_edges]), &self.device);
        self.mask = Some(mask);
        self
    }

    /// Number of global edge DOFs (the operator dimension).
    pub fn n_edges(&self) -> usize {
        self.n_edges
    }

    /// Number of elements the operator was built from.
    pub fn n_elem(&self) -> usize {
        self.n_elem
    }

    /// Apply the curl-curl stiffness: `y = K · x`.
    pub fn apply_k(&self, x: Tensor<B, 1>) -> Tensor<B, 1> {
        self.apply_local(x, &self.k_signed)
    }

    /// Apply the (ε-weighted) mass: `y = M · x`.
    pub fn apply_m(&self, x: Tensor<B, 1>) -> Tensor<B, 1> {
        self.apply_local(x, &self.m_signed)
    }

    /// Apply the driven-style combination `y = (α K + β M) · x` in a single
    /// gather/scatter pass.
    ///
    /// The driven operator (real part) is `A = K − ω² M`; pass `alpha = 1.0`,
    /// `beta = -omega * omega`. Computing the combined local operator
    /// `α K^e + β M^e` before the batched apply means only **one** gather and
    /// **one** scatter, half the traffic of separate `apply_k` + `apply_m`.
    pub fn apply_combination(&self, x: Tensor<B, 1>, alpha: f64, beta: f64) -> Tensor<B, 1> {
        let combined = self
            .k_signed
            .clone()
            .mul_scalar(alpha)
            .add(self.m_signed.clone().mul_scalar(beta));
        self.apply_local(x, &combined)
    }

    /// Shared gather → batched local apply → scatter-add kernel.
    ///
    /// `local` is a signed `[n_elem, 6, 6]` per-element operator. Returns the
    /// full-space `[n_edges]` result, with the interior mask applied on both
    /// sides if one is attached.
    fn apply_local(&self, x: Tensor<B, 1>, local: &Tensor<B, 3>) -> Tensor<B, 1> {
        // Operand `x \in \mathbb{R}^{n_edges}` — length must match the operator.
        assert_shape_contract!(MATVEC_GLOBAL_VECTOR_CONTRACT, &x, &[]);
        assert_eq!(
            x.dims()[0],
            self.n_edges,
            "operand length {} != operator n_edges {}",
            x.dims()[0],
            self.n_edges
        );

        // Mask constrained DOFs on the input (deletes constrained columns).
        let x = match &self.mask {
            Some(m) => x.mul(m.clone()),
            None => x,
        };

        // Signed local operator `\tilde{A}^{local} \in \mathbb{R}^{n_elem×6×6}`.
        assert_shape_contract!(
            MATVEC_LOCAL_MATRIX_CONTRACT,
            local,
            &[("edges_per_tet", EDGES_PER_TET)],
        );

        // 1. Gather `G`: pull each tet's six edge DOFs into `[n_elem, 6, 1]`.
        let x_gathered = x.select(0, self.edge_idx_flat.clone()); // [n_elem * 6]
        let x_elem = x_gathered.reshape([self.n_elem, EDGES_PER_TET, 1]);
        assert_shape_contract!(
            MATVEC_ELEM_COLUMN_CONTRACT,
            &x_elem,
            &[("edges_per_tet", EDGES_PER_TET), ("one", 1)],
        );

        // 2. Local apply `D`: batched `[n_elem, 6, 6] · [n_elem, 6, 1]`.
        let y_elem = local.clone().matmul(x_elem); // [n_elem, 6, 1]
        assert_shape_contract!(
            MATVEC_ELEM_COLUMN_CONTRACT,
            &y_elem,
            &[("edges_per_tet", EDGES_PER_TET), ("one", 1)],
        );

        // 3. Scatter-add `Gᵀ`: accumulate into a zero `[n_edges]` vector.
        let y_flat = y_elem.reshape([self.n_elem * EDGES_PER_TET]);
        let y = Tensor::<B, 1>::zeros([self.n_edges], &self.device).scatter(
            0,
            self.edge_idx_flat.clone(),
            y_flat,
            IndexingUpdateOp::Add,
        );

        // Mask constrained DOFs on the output (deletes constrained rows).
        match &self.mask {
            Some(m) => y.mul(m.clone()),
            None => y,
        }
    }
}
