//! First-order Nédélec (Whitney 1-form) curl-conforming elements on tets.
//!
//! Each tet contributes **6 edge DOFs**, one per edge. The DOF value
//! represents the tangential component of the vector field along its
//! edge. Globally consistent **edge orientation** is the load-bearing
//! correctness item: two tets that share an edge must agree on its
//! direction or the assembled `K`/`M` are nonsense (they may even look
//! SPD).
//!
//! # Edge orientation convention
//!
//! For an edge connecting global nodes `(a, b)`, the canonical direction
//! is from the lower-index endpoint to the higher-index endpoint:
//!
//! ```text
//! oriented_edge(a, b) = (min(a, b), max(a, b))
//! ```
//!
//! Per tet, edges are listed in the fixed canonical local order:
//!
//! ```text
//! local edge 0: (v_local_0, v_local_1)
//! local edge 1: (v_local_0, v_local_2)
//! local edge 2: (v_local_0, v_local_3)
//! local edge 3: (v_local_1, v_local_2)
//! local edge 4: (v_local_1, v_local_3)
//! local edge 5: (v_local_2, v_local_3)
//! ```
//!
//! Each tet's local edge `i` has a **sign** `s_i ∈ {+1, -1}` that records
//! whether the local edge direction (lower → higher local index of the
//! two endpoint globals at the time the edge is built) matches the
//! global orientation. The local 6×6 stiffness/mass rows and columns are
//! flipped by `s_i s_j` before scatter into the global system.
//!
//! # Whitney 1-form basis
//!
//! For edge `i = (a, b)` (lower-tagged endpoint first), with `λ_k` the
//! P1 barycentric of vertex `k`,
//!
//! ```text
//! N_i(x) = λ_a(x) ∇λ_b − λ_b(x) ∇λ_a.
//! ```
//!
//! The curl is piecewise constant per tet:
//!
//! ```text
//! ∇ × N_i = 2 (∇λ_a × ∇λ_b).
//! ```
//!
//! # Closed-form local matrices
//!
//! For an affine tet with positive volume `V = |det(J)| / 6` and gram
//! matrix `G_pq = ∇λ_p · ∇λ_q`, the per-element 6×6 curl-curl and mass
//! matrices have closed-form entries — no quadrature required.
//!
//! ## Curl-curl (stiffness)
//!
//! Using `(u × v) · (w × z) = (u·w)(v·z) − (u·z)(v·w)`,
//!
//! ```text
//! K_ij = 4 V [ G_ac G_bd − G_ad G_bc ],   i = (a, b), j = (c, d).
//! ```
//!
//! ## Mass
//!
//! Expanding `N_i · N_j` and using `∫_T λ_p λ_q dV = (V / 20)(1 + δ_pq)`,
//!
//! ```text
//! M_ij = (V / 20) [   (1 + δ_ac) G_bd
//!                   − (1 + δ_ad) G_bc
//!                   − (1 + δ_bc) G_ad
//!                   + (1 + δ_bd) G_ac ].
//! ```
//!
//! # Spurious-modes note
//!
//! The discrete curl-curl operator has a large kernel: gradients of any
//! H¹ scalar are curl-free, so `dim(ker K) ≥ n_interior_nodes`. These
//! show up as near-zero eigenvalues in a generalized `K v = λ M v` solve
//! and must be filtered before comparing the physical (non-spurious)
//! spectrum to the analytic Maxwell cavity modes.
//!
//! # Status (issue #7)
//!
//! This module provides the per-element kernel (`batched_nedelec_local_matrices`)
//! and `crate::nedelec_assembly` handles global assembly with sign
//! flips. The PEC rectangular-cavity eigenvalue acceptance test is
//! deferred until the dense generalized eigensolver (`crate::eigen`,
//! issue #12 / PR #19) lands on `main` — that test is the natural
//! integration point and is tracked as a follow-up. The local kernel
//! is independently validated against (a) hand-tabulated values on the
//! unit reference tet, (b) 32 deterministic affine tets cross-checked
//! against a CPU reference, (c) rigid-motion / dilation invariants,
//! and (d) symmetry / sign-flip behavior on a two-tet shared-edge mesh.

use burn::tensor::backend::Backend;
use burn::tensor::Tensor;

/// Canonical local edge → (local vertex pair) ordering on a tet.
///
/// Re-exported from [`crate::mesh::TET_LOCAL_EDGES`], which is the single
/// source of truth. The order is fixed across the codebase and used by
/// both the host-side edge-table builder ([`crate::mesh::TetMesh::edges`])
/// and the batched local-matrix kernel ([`batched_nedelec_local_matrices`]).
pub use crate::mesh::TET_LOCAL_EDGES;

/// Returns the canonical local edge ordering (`(local_a, local_b)` pairs).
///
/// Convenience wrapper around [`TET_LOCAL_EDGES`] for callers that prefer
/// a function form (e.g., for `const`-context calls that need the array
/// by value rather than by reference).
pub const fn tet_edges() -> [(usize, usize); 6] {
    TET_LOCAL_EDGES
}

/// Batched first-order Nédélec element-local matrices.
#[derive(Debug, Clone)]
pub struct NedelecLocalMatrices<B: Backend> {
    /// Local curl-curl stiffness `[n_elem, 6, 6]`.
    ///
    /// Sign-unaware: the per-tet local-vs-global orientation sign must
    /// be applied at assembly time (the `[n_elem, 6]` sign vector that
    /// comes from [`crate::mesh::TetMesh::tet_edges`] multiplies rows
    /// and columns).
    pub k_local: Tensor<B, 3>,
    /// Local mass matrix `[n_elem, 6, 6]`. Same sign caveat as `k_local`.
    pub m_local: Tensor<B, 3>,
    /// Signed element volumes `[n_elem]` — equals `det(J) / 6`.
    ///
    /// Negative entries indicate vertex-orientation reversal (inverted
    /// tets). For assembly weighting, use `|signed_volumes|`.
    pub signed_volumes: Tensor<B, 1>,
}

/// Compute batched first-order Nédélec local stiffness and mass for a
/// batch of affine tets given by their per-element vertex coords.
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]`. Vertex 0 (`coords[:, 0, :]`) is the
///   base used to form the edge vectors `e_k = v_k - v_0`. Matches the
///   convention in [`crate::p1::batched_p1_local_matrices`].
///
/// # Returns
///
/// `NedelecLocalMatrices { k_local, m_local, signed_volumes }`. The
/// returned local matrices are computed assuming each local edge
/// orientation is `(min(local_a, local_b), max(...))` — i.e. the
/// canonical order from [`TET_LOCAL_EDGES`]. Sign flips for the
/// **global** orientation are applied at assembly time.
///
/// # Panics
///
/// Panics if `coords` does not have shape `[*, 4, 3]`.
pub fn batched_nedelec_local_matrices<B: Backend>(coords: Tensor<B, 3>) -> NedelecLocalMatrices<B> {
    let dims = coords.dims();
    let n_elem = dims[0];
    assert_eq!(dims[1], 4, "expected 4 vertices per tet, got {}", dims[1]);
    assert_eq!(dims[2], 3, "expected 3-D coordinates, got {}", dims[2]);

    // Extract per-vertex coordinate tensors of shape [n_elem, 3].
    let v0 = coords
        .clone()
        .slice([0..n_elem, 0..1, 0..3])
        .squeeze_dim::<2>(1);
    let v1 = coords
        .clone()
        .slice([0..n_elem, 1..2, 0..3])
        .squeeze_dim::<2>(1);
    let v2 = coords
        .clone()
        .slice([0..n_elem, 2..3, 0..3])
        .squeeze_dim::<2>(1);
    let v3 = coords.slice([0..n_elem, 3..4, 0..3]).squeeze_dim::<2>(1);

    // Edge vectors from v0, each [n_elem, 3].
    let e1 = v1 - v0.clone();
    let e2 = v2 - v0.clone();
    let e3 = v3 - v0;

    // Cofactor cross products: g_i for i in 1..=3, each [n_elem, 3].
    // ∇λ_i = g_i / det(J) (in P1; same identity here).
    let g1 = e2.clone().cross(e3.clone(), 1);
    let g2 = e3.clone().cross(e1.clone(), 1);
    let g3 = e1.clone().cross(e2.clone(), 1);

    // det(J) per element = e_1 · g_1, shape [n_elem].
    let det = e1.mul(g1.clone()).sum_dim(1).squeeze_dim::<1>(1);
    let signed_volumes = det.clone().div_scalar(6.0);

    // g_0 = -(g_1 + g_2 + g_3), shape [n_elem, 3].
    let g0 = (g1.clone() + g2.clone() + g3.clone()).neg();

    // Stack into G: [n_elem, 4, 3] with row i = g_i.
    let g_mat = Tensor::<B, 2>::stack::<3>(vec![g0, g1, g2, g3], 1);

    // (G @ G^T) has shape [n_elem, 4, 4] with entries (g_i · g_j). The
    // physical gradient gram is G_pq = ∇λ_p · ∇λ_q = gg_pq / det². The
    // scale factors are folded into per-entry K and M scales below.
    //
    // Curl-curl entry:
    //   K_ij = 4 V [G_ac G_bd − G_ad G_bc]
    //        = (4 |det|/6) · (gg_ac gg_bd − gg_ad gg_bc) / det^4
    //        = (2 / (3 |det|^3)) · (gg_ac gg_bd − gg_ad gg_bc).
    //
    // Mass entry:
    //   M_ij = (V/20) [ (1+δ_ac) G_bd − (1+δ_ad) G_bc
    //                  − (1+δ_bc) G_ad + (1+δ_bd) G_ac ]
    //   with (V/20) G_pq = (|det|/120) · (gg_pq / det²) = gg_pq / (120 |det|).
    let g_t = g_mat.clone().swap_dims(1, 2);
    let gg = g_mat.matmul(g_t); // [n_elem, 4, 4], (g_i · g_j)

    // Helper closure: extract gg[:, p, q] as [n_elem] tensor.
    let gg_entry = |p: usize, q: usize| -> Tensor<B, 1> {
        gg.clone()
            .slice([0..n_elem, p..p + 1, q..q + 1])
            .squeeze_dim::<2>(1)
            .squeeze_dim::<1>(1)
    };

    // Build a flat Vec<Tensor<B, 1>> of length 36 for K and 36 for M,
    // then stack into [n_elem, 36] and reshape to [n_elem, 6, 6].

    // Absolute determinant per element.
    let abs_det = det.abs();
    // 1/|det|^3 for K, 1/|det| for M (computed once, broadcast).
    let inv_abs_det = abs_det.clone().recip();
    let inv_abs_det3 = inv_abs_det.clone() * inv_abs_det.clone() * inv_abs_det.clone();

    let mut k_entries: Vec<Tensor<B, 1>> = Vec::with_capacity(36);
    let mut m_entries: Vec<Tensor<B, 1>> = Vec::with_capacity(36);

    for &(a, b) in TET_LOCAL_EDGES.iter() {
        for &(c, d) in TET_LOCAL_EDGES.iter() {
            // K_ij = (2/3) · (gg_ac gg_bd − gg_ad gg_bc) / |det|^3
            //
            // The `2/3` scalar is computed in f64 — using an `f32` literal
            // truncates the ratio to single precision *before* upcasting,
            // costing ~5e-8 of accuracy on the f64 backend and dragging the
            // backend-agnostic tolerance floor down to 1e-7. The
            // f32-backend `mul_scalar` still converts to f32 internally.
            let k_term = gg_entry(a, c).mul(gg_entry(b, d)) - gg_entry(a, d).mul(gg_entry(b, c));
            let k_val = k_term.mul(inv_abs_det3.clone()).mul_scalar(2.0_f64 / 3.0);
            k_entries.push(k_val);

            // M_ij = (V/20) [ (1 + δ_ac) G_bd − (1 + δ_ad) G_bc
            //              − (1 + δ_bc) G_ad + (1 + δ_bd) G_ac ]
            // With (V/20) G_pq = gg_pq / (120 |det|), each prefactor (1 + δ)
            // is a constant we fold into a scalar multiply. (`1.0` and
            // `2.0` are exact in f32, so the literal type only matters
            // for code-doc consistency with the K block above.)
            let f_ac = if a == c { 2.0_f64 } else { 1.0_f64 };
            let f_ad = if a == d { 2.0_f64 } else { 1.0_f64 };
            let f_bc = if b == c { 2.0_f64 } else { 1.0_f64 };
            let f_bd = if b == d { 2.0_f64 } else { 1.0_f64 };

            let m_term = gg_entry(b, d).mul_scalar(f_ac)
                - gg_entry(b, c).mul_scalar(f_ad)
                - gg_entry(a, d).mul_scalar(f_bc)
                + gg_entry(a, c).mul_scalar(f_bd);
            let m_val = m_term.mul(inv_abs_det.clone()).div_scalar(120.0);
            m_entries.push(m_val);
        }
    }

    let k_stacked = Tensor::<B, 1>::stack::<2>(k_entries, 1); // [n_elem, 36]
    let m_stacked = Tensor::<B, 1>::stack::<2>(m_entries, 1);
    let k_local = k_stacked.reshape([n_elem, 6, 6]);
    let m_local = m_stacked.reshape([n_elem, 6, 6]);

    NedelecLocalMatrices {
        k_local,
        m_local,
        signed_volumes,
    }
}

/// Per-element Nédélec local mass matrix for a **diagonal anisotropic**
/// permittivity tensor expressed in the global Cartesian basis.
///
/// The integrand becomes
///
/// ```text
/// (N_i)^T · diag(ε_x, ε_y, ε_z) · N_j
///    = ε_x N_{i,x} N_{j,x} + ε_y N_{i,y} N_{j,y} + ε_z N_{i,z} N_{j,z},
/// ```
///
/// which is exactly the existing scalar mass formula with the gradient
/// gram `G_pq = ∇λ_p · ∇λ_q` replaced by the per-component product
/// `G^(α)_pq = (∇λ_p)_α (∇λ_q)_α`. The three per-axis local matrices
/// are linearly combined per element with the supplied diagonal entries.
///
/// This is the **diagonal-only** UPML simplification (Option B in
/// issue #54): the off-diagonal terms of the full rotation
/// `R · diag(1/s_r, s_t, s_t) · R^T` are dropped. For axis-aligned
/// directions (the dominant absorption channels on a Cartesian-ish
/// mesh) the diagonal already carries the correct anisotropy; for
/// off-axis tets it is an approximation, but still strictly more
/// accurate than scalar-isotropic.
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]` per-tet vertex coordinates.
/// * `eps_diag` — `[n_elem, 3]` per-tet, per-axis weights to apply
///   to the three component-product mass matrices before summing.
///
/// # Returns
///
/// `[n_elem, 6, 6]` local mass matrix (sign-unaware, same orientation
/// caveat as [`batched_nedelec_local_matrices`]).
pub fn batched_nedelec_local_mass_anisotropic_diag<B: Backend>(
    coords: Tensor<B, 3>,
    eps_diag: Tensor<B, 2>,
) -> Tensor<B, 3> {
    let dims = coords.dims();
    let n_elem = dims[0];
    assert_eq!(dims[1], 4, "expected 4 vertices per tet, got {}", dims[1]);
    assert_eq!(dims[2], 3, "expected 3-D coordinates, got {}", dims[2]);
    let eps_dims = eps_diag.dims();
    assert_eq!(
        eps_dims,
        [n_elem, 3],
        "expected eps_diag shape [n_elem, 3], got {:?}",
        eps_dims
    );

    // Reuse the same per-vertex / edge / cofactor / det machinery as
    // the scalar kernel.
    let v0 = coords
        .clone()
        .slice([0..n_elem, 0..1, 0..3])
        .squeeze_dim::<2>(1);
    let v1 = coords
        .clone()
        .slice([0..n_elem, 1..2, 0..3])
        .squeeze_dim::<2>(1);
    let v2 = coords
        .clone()
        .slice([0..n_elem, 2..3, 0..3])
        .squeeze_dim::<2>(1);
    let v3 = coords.slice([0..n_elem, 3..4, 0..3]).squeeze_dim::<2>(1);

    let e1 = v1 - v0.clone();
    let e2 = v2 - v0.clone();
    let e3 = v3 - v0;

    let g1 = e2.clone().cross(e3.clone(), 1);
    let g2 = e3.clone().cross(e1.clone(), 1);
    let g3 = e1.clone().cross(e2.clone(), 1);

    let det = e1.mul(g1.clone()).sum_dim(1).squeeze_dim::<1>(1);
    let g0 = (g1.clone() + g2.clone() + g3.clone()).neg();

    // Stack into G: [n_elem, 4, 3] with row p = g_p (cofactor column,
    // shape [3]).
    let g_mat = Tensor::<B, 2>::stack::<3>(vec![g0, g1, g2, g3], 1); // [n_elem, 4, 3]

    // Per-axis component product G^(α)_pq = g_p[α] g_q[α] / det²,
    // captured here as gg^(α)_pq = g_p[α] g_q[α].
    //
    // Compute by slicing the α-column out of g_mat, getting a
    // [n_elem, 4] tensor per axis, then forming the outer product
    // [n_elem, 4, 4].
    let abs_det = det.abs();
    let inv_abs_det = abs_det.clone().recip();

    // Per-axis epsilon weights: [n_elem] tensors for α ∈ {x, y, z}.
    let eps_x = eps_diag
        .clone()
        .slice([0..n_elem, 0..1])
        .squeeze_dim::<1>(1);
    let eps_y = eps_diag
        .clone()
        .slice([0..n_elem, 1..2])
        .squeeze_dim::<1>(1);
    let eps_z = eps_diag.slice([0..n_elem, 2..3]).squeeze_dim::<1>(1);

    let per_axis_gg = |alpha: usize| -> Tensor<B, 3> {
        // g_mat[:, :, alpha] → [n_elem, 4]
        let g_col = g_mat
            .clone()
            .slice([0..n_elem, 0..4, alpha..alpha + 1])
            .squeeze_dim::<2>(2);
        // outer product per element: [n_elem, 4, 1] × [n_elem, 1, 4]
        let col_row = g_col.clone().unsqueeze_dim::<3>(2); // [n_elem, 4, 1]
        let col_col = g_col.unsqueeze_dim::<3>(1); // [n_elem, 1, 4]
        col_row.mul(col_col) // [n_elem, 4, 4]
    };

    let gg_x = per_axis_gg(0);
    let gg_y = per_axis_gg(1);
    let gg_z = per_axis_gg(2);

    let gg_entry = |gg: &Tensor<B, 3>, p: usize, q: usize| -> Tensor<B, 1> {
        gg.clone()
            .slice([0..n_elem, p..p + 1, q..q + 1])
            .squeeze_dim::<2>(1)
            .squeeze_dim::<1>(1)
    };

    let mut m_entries: Vec<Tensor<B, 1>> = Vec::with_capacity(36);

    for &(a, b) in TET_LOCAL_EDGES.iter() {
        for &(c, d) in TET_LOCAL_EDGES.iter() {
            let f_ac = if a == c { 2.0_f32 } else { 1.0_f32 };
            let f_ad = if a == d { 2.0_f32 } else { 1.0_f32 };
            let f_bc = if b == c { 2.0_f32 } else { 1.0_f32 };
            let f_bd = if b == d { 2.0_f32 } else { 1.0_f32 };

            // Per-axis closed-form integrand. Same shape as the scalar
            // kernel but with G replaced by gg^(α) / det²; the /det²
            // and (V/20) = (|det|/120) collapse into 1/(120 |det|).
            let m_axis = |gg: &Tensor<B, 3>, eps: &Tensor<B, 1>| -> Tensor<B, 1> {
                let term = gg_entry(gg, b, d).mul_scalar(f_ac)
                    - gg_entry(gg, b, c).mul_scalar(f_ad)
                    - gg_entry(gg, a, d).mul_scalar(f_bc)
                    + gg_entry(gg, a, c).mul_scalar(f_bd);
                term.mul(inv_abs_det.clone())
                    .div_scalar(120.0)
                    .mul(eps.clone())
            };

            let m_val = m_axis(&gg_x, &eps_x) + m_axis(&gg_y, &eps_y) + m_axis(&gg_z, &eps_z);
            m_entries.push(m_val);
        }
    }

    let m_stacked = Tensor::<B, 1>::stack::<2>(m_entries, 1); // [n_elem, 36]
    m_stacked.reshape([n_elem, 6, 6])
}
