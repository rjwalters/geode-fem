//! P1 (linear) tetrahedral reference element with batched local matrices.
//!
//! For affine tets, the Jacobian is constant per element and the local
//! stiffness and consistent mass matrices have closed-form expressions.
//! This module computes both for a batch of `n_elem` tets as a single
//! fused Burn tensor expression — no per-element loop, no per-tet kernel
//! launch.
//!
//! # Geometry recap
//!
//! Given a tet with vertices `v_0, v_1, v_2, v_3` and edge vectors
//! `e_k = v_k - v_0`, the Jacobian of the affine map from the reference
//! tet `[(0,0,0), (1,0,0), (0,1,0), (0,0,1)]` to the physical tet is
//! `J = [e_1 | e_2 | e_3]` (3×3, edges as columns).
//!
//! Define the area-weighted basis-function gradients (each a 3-vector)
//! by the standard 3×3 inverse identity:
//!
//! ```text
//! g_1 = e_2 × e_3,    g_2 = e_3 × e_1,    g_3 = e_1 × e_2,
//! g_0 = -(g_1 + g_2 + g_3),
//! det = e_1 · g_1 = det(J).
//! ```
//!
//! Then `∇φ_i = g_i / det(J)`, the signed element volume is `V_signed = det/6`,
//! the (positive) element volume is `V = |det|/6`, and the local matrices are:
//!
//! ```text
//! K_{ij} = V (∇φ_i · ∇φ_j) = (g_i · g_j) / (6 |det|),
//! M_{ij} = (V / 20) (1 + δ_{ij})    (consistent mass).
//! ```

use bunsen::contracts::{define_shape_contract, unpack_shape_contract};
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;

/// Number of vertices per linear tetrahedral (P1) element. Bound into
/// `P1_TET_COORDS_CONTRACT` as `nodes_per_tet` so the 4-vertex arity is
/// machine-checked rather than left implicit in a `4` literal.
const NODES_PER_TET: usize = 4;

/// Spatial dimension of the vertex coordinates (3-D). Bound into
/// `P1_TET_COORDS_CONTRACT` as `spatial`.
const SPATIAL: usize = 3;

// ---------------------------------------------------------------------------
// Named static shape contracts (Bunsen, Epic #355 Phase 3)
// ---------------------------------------------------------------------------
//
// The recurring FEM tensor shape of the P1 basis-evaluation path, named once so
// the machine-checked invariant is reused instead of being re-spelled as a bare
// `let dims = coords.dims(); assert_eq!(dims[1], 4, …)` destructure. Follows the
// Phase 2 template established in `crate::assembly::p1` (PR #467).

// Batched per-tet vertex-coordinate stack
// `X^e \in \mathbb{R}^{n_elem × 4 × 3}`: one `4 × 3` block per element, row `i`
// the 3-D coordinates of the tet's vertex `i` (`X^e[i, :]`), feeding the affine
// element Jacobian `J = [v_1 - v_0 | v_2 - v_0 | v_3 - v_0]`. The
// `nodes_per_tet` axis is bound to `NODES_PER_TET` (`= 4`) and `spatial` to
// `SPATIAL` (`= 3`) at the call site, so coordinates built with the wrong tet
// arity or spatial dimension panic with a named-axis diagnostic instead of
// silently mis-slicing the per-vertex extraction below.
define_shape_contract!(
    P1_TET_COORDS_CONTRACT,
    ["n_elem", "nodes_per_tet", "spatial"]
);

/// Batched P1 element-local matrices.
#[derive(Debug, Clone)]
pub struct P1LocalMatrices<B: Backend> {
    /// Local stiffness `[n_elem, 4, 4]`.
    pub k_local: Tensor<B, 3>,
    /// Local consistent mass `[n_elem, 4, 4]`.
    pub m_local: Tensor<B, 3>,
    /// Signed element volumes `[n_elem]` — equals `det(J) / 6`.
    ///
    /// Negative entries indicate vertex-orientation reversal (inverted tets).
    /// **For assembly weighting, use `signed_volumes.abs()`.** The sign here
    /// is a mesh-quality diagnostic only; integrals `∫_T f dV` need the
    /// unsigned volume `V = |det(J)|/6` or contributions from inverted tets
    /// will cancel rather than add.
    pub signed_volumes: Tensor<B, 1>,
}

/// Compute batched P1 local stiffness and consistent mass matrices for a
/// batch of affine tetrahedra given by their per-element vertex coords.
///
/// # Arguments
///
/// * `coords` — `[n_elem, 4, 3]`. Vertex 0 (i.e. `coords[:, 0, :]`) is the
///   "base" used for forming the edge vectors `e_k = v_k - v_0`.
///
/// # Returns
///
/// `P1LocalMatrices { k_local, m_local, signed_volumes }`. See module-level
/// docs for the mathematical definitions.
///
/// # Panics
///
/// Panics if `coords` does not have shape `[*, 4, 3]` — validated via Bunsen's
/// `P1_TET_COORDS_CONTRACT`, which fires a `Shape Error` naming the offending
/// axis (`nodes_per_tet` or `spatial`) instead of the former bare
/// `assert_eq!(dims[1], 4, …)` / `assert_eq!(dims[2], 3, …)` pair. This is a
/// cold, one-shot check (one call per assembly, not per element), so the plain
/// `unpack_shape_contract!` variant is used rather than the periodic one.
pub fn batched_p1_local_matrices<B: Backend>(coords: Tensor<B, 3>) -> P1LocalMatrices<B> {
    let device = coords.device();
    // Batched vertex-coordinate stack `X^e \in \mathbb{R}^{n_elem × 4 × 3}` —
    // extract `n_elem` and check the `nodes_per_tet = 4` / `spatial = 3` axes
    // through the shared named contract.
    let [n_elem] = unpack_shape_contract!(
        P1_TET_COORDS_CONTRACT,
        &coords,
        &["n_elem"],
        &[("nodes_per_tet", NODES_PER_TET), ("spatial", SPATIAL)],
    );

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

    // Cross products: g_i for i in 1..=3, each [n_elem, 3].
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

    // K_ij = (g_i · g_j) / (6 |det|).
    // (G @ G^T) has shape [n_elem, 4, 4] with entries (g_i · g_j).
    let g_t = g_mat.clone().swap_dims(1, 2);
    let gg = g_mat.matmul(g_t);

    // Per-element scale 1/(6 |det|), broadcast to [n_elem, 1, 1].
    let abs_det = det.abs();
    let k_scale = abs_det
        .clone()
        .recip()
        .div_scalar(6.0)
        .unsqueeze_dim::<2>(1)
        .unsqueeze_dim::<3>(2);
    let k_local = gg.mul(k_scale);

    // Consistent mass pattern: (I_4 + ones_4x4) — 2 on diagonal, 1 off-diagonal.
    // Reusable constant per call; broadcast across the batch.
    let mass_pattern_data: [[f32; 4]; 4] = [
        [2.0, 1.0, 1.0, 1.0],
        [1.0, 2.0, 1.0, 1.0],
        [1.0, 1.0, 2.0, 1.0],
        [1.0, 1.0, 1.0, 2.0],
    ];
    let mass_pattern =
        Tensor::<B, 2>::from_floats(mass_pattern_data, &device).unsqueeze_dim::<3>(0);
    // Scale: V / 20 per element, computed as |det| / 120; broadcast to [n_elem, 1, 1].
    let m_scale = abs_det
        .div_scalar(120.0)
        .unsqueeze_dim::<2>(1)
        .unsqueeze_dim::<3>(2);
    let m_local = mass_pattern.mul(m_scale);

    P1LocalMatrices {
        k_local,
        m_local,
        signed_volumes,
    }
}
