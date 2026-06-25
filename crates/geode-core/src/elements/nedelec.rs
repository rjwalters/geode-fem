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
//! and `crate::assembly::nedelec` handles global assembly with sign
//! flips. The PEC rectangular-cavity eigenvalue acceptance test is
//! deferred until the dense generalized eigensolver (`crate::eigen`,
//! issue #12 / PR #19) lands on `main` — that test is the natural
//! integration point and is tracked as a follow-up. The local kernel
//! is independently validated against (a) hand-tabulated values on the
//! unit reference tet, (b) 32 deterministic affine tets cross-checked
//! against a CPU reference, (c) rigid-motion / dilation invariants,
//! and (d) symmetry / sign-flip behavior on a two-tet shared-edge mesh.

use burn::tensor::Tensor;
use burn::tensor::backend::Backend;

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
///   convention in [`crate::elements::p1::batched_p1_local_matrices`].
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

/// Batched first-order Nédélec element-local right-hand-side vector for
/// a **piecewise-constant** volumetric current density `J`.
///
/// Computes, for each tet `T` and each local edge `i = (a, b)`,
///
/// ```text
/// b_local_i = ∫_T N_i · J dV,
/// ```
///
/// where `J` is held constant per element (typically sampled at the tet
/// centroid). With the Whitney 1-form basis `N_i = λ_a ∇λ_b − λ_b ∇λ_a`
/// and `∫_T λ_p dV = V/4`, the integral is closed-form:
///
/// ```text
/// ∫_T N_i dV = (V/4) (∇λ_b − ∇λ_a)
///            = sign(det J) / 24 · (g_b − g_a),
/// ```
///
/// using `∇λ_p = g_p / det(J)` (cofactor vectors `g_p`) and
/// `V = |det(J)| / 6`. Unlike the K/M kernels — where the gradients
/// appear in **pairs** and the sign of `det(J)` cancels — the RHS is
/// linear in the gradients, so the orientation sign of the tet must be
/// kept.
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]` per-tet vertex coordinates (same
///   convention as [`batched_nedelec_local_matrices`]).
/// * `j_tet` — `[n_elem, 3]` per-tet constant current density.
///
/// # Returns
///
/// `[n_elem, 6]` local RHS entries in canonical local-edge order
/// ([`TET_LOCAL_EDGES`]). Sign-unaware: the per-tet local-vs-global
/// orientation sign `s_i` must be applied at assembly time (a single
/// factor `s_i`, not the `s_i s_j` outer product of the matrix path).
///
/// # Panics
///
/// Panics if `coords` is not `[*, 4, 3]` or `j_tet` is not `[*, 3]`
/// with matching element counts.
pub fn batched_nedelec_local_rhs<B: Backend>(
    coords: Tensor<B, 3>,
    j_tet: Tensor<B, 2>,
) -> Tensor<B, 2> {
    let dims = coords.dims();
    let n_elem = dims[0];
    assert_eq!(dims[1], 4, "expected 4 vertices per tet, got {}", dims[1]);
    assert_eq!(dims[2], 3, "expected 3-D coordinates, got {}", dims[2]);
    let j_dims = j_tet.dims();
    assert_eq!(
        j_dims,
        [n_elem, 3],
        "expected j_tet shape [n_elem, 3], got {:?}",
        j_dims
    );

    // Same per-vertex / edge / cofactor / det machinery as the matrix
    // kernels.
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

    let det = e1.mul(g1.clone()).sum_dim(1).squeeze_dim::<1>(1); // [n_elem]
    let g0 = (g1.clone() + g2.clone() + g3.clone()).neg();

    // sign(det) / 24 per element. det / |det| is exactly ±1 in floating
    // point for non-degenerate tets (degenerate tets produce NaN here,
    // exactly as they produce Inf/NaN in the K/M kernels).
    let factor = det.clone().div(det.abs()).div_scalar(24.0); // [n_elem]

    let g_mat = Tensor::<B, 2>::stack::<3>(vec![g0, g1, g2, g3], 1); // [n_elem, 4, 3]
    let g_row = |p: usize| -> Tensor<B, 2> {
        g_mat
            .clone()
            .slice([0..n_elem, p..p + 1, 0..3])
            .squeeze_dim::<2>(1)
    };

    let mut entries: Vec<Tensor<B, 1>> = Vec::with_capacity(6);
    for &(a, b) in TET_LOCAL_EDGES.iter() {
        // (g_b − g_a) · J, per element.
        let diff = g_row(b) - g_row(a); // [n_elem, 3]
        let dot = diff.mul(j_tet.clone()).sum_dim(1).squeeze_dim::<1>(1); // [n_elem]
        entries.push(dot.mul(factor.clone()));
    }

    Tensor::<B, 1>::stack::<2>(entries, 1) // [n_elem, 6]
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

/// Barycentric weight of the "own" vertex in the symmetric degree-2
/// 4-point tet quadrature rule: point `q` has `λ_q = TET_QUAD4_A` and
/// `λ_p = TET_QUAD4_B` for `p ≠ q`, with equal weights `V/4`. Shared
/// with the host-side reference assembly in [`crate::driven::scattering`] so
/// the two paths integrate spatially varying sources identically.
pub const TET_QUAD4_A: f64 = 0.585_410_196_624_968_5;
/// Barycentric weight of the three "other" vertices in the degree-2
/// 4-point tet rule (see [`TET_QUAD4_A`]).
pub const TET_QUAD4_B: f64 = 0.138_196_601_125_010_5;

/// Per-vertex cofactor vectors `g_p` (`[n_elem, 4, 3]`, row `p` = `g_p`)
/// and the per-element determinant `det(J)` (`[n_elem]`) shared by the
/// tensor-weighted kernels below. `∇λ_p = g_p / det(J)`.
fn cofactor_rows_and_det<B: Backend>(coords: Tensor<B, 3>) -> (Tensor<B, 3>, Tensor<B, 1>) {
    let dims = coords.dims();
    let n_elem = dims[0];
    assert_eq!(dims[1], 4, "expected 4 vertices per tet, got {}", dims[1]);
    assert_eq!(dims[2], 3, "expected 3-D coordinates, got {}", dims[2]);

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

    let g_mat = Tensor::<B, 2>::stack::<3>(vec![g0, g1, g2, g3], 1); // [n_elem, 4, 3]
    (g_mat, det)
}

/// Per-element Nédélec curl-curl (stiffness) matrix with a **full 3×3**
/// per-tet weight tensor `W` on the curls (issue #199):
///
/// ```text
/// K^W_ij = ∫_T (∇×N_i)ᵀ · W · (∇×N_j) dV = V · c_iᵀ W c_j,
/// ```
///
/// where `c_i = 2(∇λ_a × ∇λ_b)` is the constant per-tet curl of Whitney
/// edge basis `i = (a, b)`. This is the stiffness analogue of the
/// weighted mass kernels: for the matched (full Sacks) UPML the curl
/// weight is `W = Λ⁻¹` (`μ = Λ` stretched, so `μ⁻¹ = Λ⁻¹` lands on the
/// curl-curl term). Complex weights run as two passes (`Re(W)`,
/// `Im(W)`), exactly like the complex-ε mass path.
///
/// With `W = I` this reduces to the `k_local` of
/// [`batched_nedelec_local_matrices`] (Lagrange identity
/// `(u×v)·(w×z) = (u·w)(v·z) − (u·z)(v·w)`).
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]` per-tet vertex coordinates.
/// * `weight` — `[n_elem, 3, 3]` per-tet weight tensor in the global
///   Cartesian basis (real; complex weights are split by the caller).
///
/// # Returns
///
/// `[n_elem, 6, 6]` local stiffness (sign-unaware, same orientation
/// caveat as [`batched_nedelec_local_matrices`]).
pub fn batched_nedelec_local_stiffness_weighted<B: Backend>(
    coords: Tensor<B, 3>,
    weight: Tensor<B, 3>,
) -> Tensor<B, 3> {
    let n_elem = coords.dims()[0];
    let w_dims = weight.dims();
    assert_eq!(
        w_dims,
        [n_elem, 3, 3],
        "expected weight shape [n_elem, 3, 3], got {:?}",
        w_dims
    );

    let (g_mat, det) = cofactor_rows_and_det(coords);

    // Unnormalized curl directions: cr_i = g_a × g_b per local edge,
    // shape [n_elem, 6, 3]. The physical curl is 2(∇λ_a × ∇λ_b)
    // = 2 cr_i / det²; the dangling scale factors are folded below.
    let g_row = |p: usize| -> Tensor<B, 2> {
        g_mat
            .clone()
            .slice([0..n_elem, p..p + 1, 0..3])
            .squeeze_dim::<2>(1)
    };
    let cr_rows: Vec<Tensor<B, 2>> = TET_LOCAL_EDGES
        .iter()
        .map(|&(a, b)| g_row(a).cross(g_row(b), 1))
        .collect();
    let cr = Tensor::<B, 2>::stack::<3>(cr_rows, 1); // [n_elem, 6, 3]

    // K_ij = V · (2 cr_i / det²)ᵀ W (2 cr_j / det²)
    //      = (|det|/6) · 4/det⁴ · cr_iᵀ W cr_j
    //      = (2 / (3 |det|³)) · cr_iᵀ W cr_j.
    // (det⁴ = |det|⁴ — even power.) Same f64-literal note as the scalar
    // kernel: keep `2/3` in double precision.
    let abs_det = det.abs();
    let inv_abs_det = abs_det.recip();
    let scale = (inv_abs_det.clone() * inv_abs_det.clone() * inv_abs_det).mul_scalar(2.0_f64 / 3.0);
    let scale_3d = scale.unsqueeze_dim::<2>(1).unsqueeze_dim::<3>(2); // [n_elem, 1, 1]

    cr.clone()
        .matmul(weight)
        .matmul(cr.swap_dims(1, 2))
        .mul(scale_3d)
}

/// Per-element Nédélec local mass matrix for a **full 3×3** per-tet
/// weight tensor `W` in the global Cartesian basis (issue #199):
///
/// ```text
/// M^W_ij = ∫_T N_iᵀ · W · N_j dV.
/// ```
///
/// Generalizes [`batched_nedelec_local_mass_anisotropic_diag`] by
/// keeping the off-diagonal entries of `W` — required for the matched
/// (full Sacks) UPML whose Cartesian tensor `ε = ε_r·Λ` has
/// off-diagonals away from the coordinate axes. With
/// `∫_T λ_p λ_q dV = (V/20)(1 + δ_pq)` the closed form is the scalar
/// mass formula with the gradient gram `G_pq = ∇λ_p · ∇λ_q` replaced
/// by the weighted gram `G^W_pq = ∇λ_pᵀ W ∇λ_q`:
///
/// ```text
/// M^W_ij = (V/20) [   (1 + δ_ac) G^W_bd − (1 + δ_ad) G^W_bc
///                   − (1 + δ_bc) G^W_ad + (1 + δ_bd) G^W_ac ].
/// ```
///
/// Note `G^W` is asymmetric for asymmetric `W`; the index order above
/// (first basis index contracts the **left** slot of `W`) matches the
/// host-path reference in [`crate::driven::scattering`]. Complex weights run as
/// two passes (`Re(W)`, `Im(W)`).
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]` per-tet vertex coordinates.
/// * `weight` — `[n_elem, 3, 3]` per-tet weight tensor (real part or
///   imaginary part of the complex constitutive tensor).
///
/// # Returns
///
/// `[n_elem, 6, 6]` local mass matrix (sign-unaware, same orientation
/// caveat as [`batched_nedelec_local_matrices`]).
pub fn batched_nedelec_local_mass_anisotropic_full<B: Backend>(
    coords: Tensor<B, 3>,
    weight: Tensor<B, 3>,
) -> Tensor<B, 3> {
    let n_elem = coords.dims()[0];
    let w_dims = weight.dims();
    assert_eq!(
        w_dims,
        [n_elem, 3, 3],
        "expected weight shape [n_elem, 3, 3], got {:?}",
        w_dims
    );

    let (g_mat, det) = cofactor_rows_and_det(coords);
    let abs_det = det.abs();
    let inv_abs_det = abs_det.recip();

    // Weighted gram gw_pq = g_pᵀ W g_q, shape [n_elem, 4, 4]. The
    // physical weighted gram is G^W_pq = gw_pq / det²; the /det² and
    // (V/20) = (|det|/120) collapse into 1/(120 |det|) below.
    let gw = g_mat.clone().matmul(weight).matmul(g_mat.swap_dims(1, 2));

    let gw_entry = |p: usize, q: usize| -> Tensor<B, 1> {
        gw.clone()
            .slice([0..n_elem, p..p + 1, q..q + 1])
            .squeeze_dim::<2>(1)
            .squeeze_dim::<1>(1)
    };

    let mut m_entries: Vec<Tensor<B, 1>> = Vec::with_capacity(36);
    for &(a, b) in TET_LOCAL_EDGES.iter() {
        for &(c, d) in TET_LOCAL_EDGES.iter() {
            let f_ac = if a == c { 2.0_f64 } else { 1.0_f64 };
            let f_ad = if a == d { 2.0_f64 } else { 1.0_f64 };
            let f_bc = if b == c { 2.0_f64 } else { 1.0_f64 };
            let f_bd = if b == d { 2.0_f64 } else { 1.0_f64 };

            let term = gw_entry(b, d).mul_scalar(f_ac)
                - gw_entry(b, c).mul_scalar(f_ad)
                - gw_entry(a, d).mul_scalar(f_bc)
                + gw_entry(a, c).mul_scalar(f_bd);
            m_entries.push(term.mul(inv_abs_det.clone()).div_scalar(120.0));
        }
    }

    let m_stacked = Tensor::<B, 1>::stack::<2>(m_entries, 1); // [n_elem, 36]
    m_stacked.reshape([n_elem, 6, 6])
}

/// Batched Nédélec element-local RHS with the **degree-2 (4-point)**
/// tet quadrature for a spatially varying current density `J(x)`
/// sampled at the quadrature points (issue #199):
///
/// ```text
/// b_local_i = ∫_T N_i · J dV ≈ Σ_q (V/4) N_i(x_q) · J_q,
/// ```
///
/// with the symmetric rule `λ_p(x_q) = TET_QUAD4_A` if `p = q` else
/// `TET_QUAD4_B` ([`TET_QUAD4_A`]/[`TET_QUAD4_B`] — the same rule the
/// host-side reference assembly in
/// [`crate::driven::scattering::solve_scattered_field_matched_upml`] uses, so
/// the two RHS paths agree to round-off for the same samples).
///
/// # Closed form used
///
/// With `N_i = λ_a ∇λ_b − λ_b ∇λ_a`, `∇λ_p = g_p / det(J)`, and
/// `D_qp = J_q · g_p`,
///
/// ```text
/// b_i = sign(det)/24 · [ (A − B)(D_ab − D_ba) + B (S_b − S_a) ],
/// S_p = Σ_q D_qp,
/// ```
///
/// which reduces **exactly** to [`batched_nedelec_local_rhs`] for a
/// per-tet-constant `J` (then `D_qp = J·g_p` for all `q` and
/// `A + 3B = 1`). Complex `J` runs as two passes (Re, Im), like the
/// constant-`J` path.
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]` per-tet vertex coordinates.
/// * `j_quad` — `[n_elem, 4, 3]` current density at the four degree-2
///   quadrature points, in the rule's point order (point `q` is the
///   one with weight `TET_QUAD4_A` on vertex `q`).
///
/// # Returns
///
/// `[n_elem, 6]` local RHS in canonical local-edge order, sign-unaware
/// (same caveat as [`batched_nedelec_local_rhs`]).
pub fn batched_nedelec_local_rhs_quad4<B: Backend>(
    coords: Tensor<B, 3>,
    j_quad: Tensor<B, 3>,
) -> Tensor<B, 2> {
    let n_elem = coords.dims()[0];
    let j_dims = j_quad.dims();
    assert_eq!(
        j_dims,
        [n_elem, 4, 3],
        "expected j_quad shape [n_elem, 4, 3], got {:?}",
        j_dims
    );

    let (g_mat, det) = cofactor_rows_and_det(coords);

    // D[q][p] = J_q · g_p, shape [n_elem, 4, 4].
    let d = j_quad.matmul(g_mat.swap_dims(1, 2));
    // S[p] = Σ_q D[q][p], shape [n_elem, 4] (sum over the quad axis).
    let s = d.clone().sum_dim(1).squeeze_dim::<2>(1);

    let d_entry = |q: usize, p: usize| -> Tensor<B, 1> {
        d.clone()
            .slice([0..n_elem, q..q + 1, p..p + 1])
            .squeeze_dim::<2>(1)
            .squeeze_dim::<1>(1)
    };
    let s_entry =
        |p: usize| -> Tensor<B, 1> { s.clone().slice([0..n_elem, p..p + 1]).squeeze_dim::<1>(1) };

    // sign(det)/24 — same orientation handling as the constant-J RHS.
    let factor = det.clone().div(det.abs()).div_scalar(24.0); // [n_elem]
    let a_minus_b = TET_QUAD4_A - TET_QUAD4_B;

    let mut entries: Vec<Tensor<B, 1>> = Vec::with_capacity(6);
    for &(a, b) in TET_LOCAL_EDGES.iter() {
        let t = (d_entry(a, b) - d_entry(b, a)).mul_scalar(a_minus_b)
            + (s_entry(b) - s_entry(a)).mul_scalar(TET_QUAD4_B);
        entries.push(t.mul(factor.clone()));
    }
    Tensor::<B, 1>::stack::<2>(entries, 1) // [n_elem, 6]
}
