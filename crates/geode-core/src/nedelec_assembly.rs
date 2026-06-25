//! Global assembly of first-order Nédélec element-local matrices.
//!
//! Mirrors `assembly.rs`: builds dense global `K` and `M` matrices by
//! scattering per-element `[n_elem, 6, 6]` local tensors into a flat
//! `[n_edges * n_edges]` Burn tensor using 1-D
//! [`Tensor::scatter`](burn::tensor::Tensor::scatter) with
//! `IndexingUpdateOp::Add`, then reshaping to `[n_edges, n_edges]`.
//!
//! The Nédélec-specific bookkeeping is:
//!
//! 1. The DOFs are edges, not nodes — see [`crate::mesh::TetMesh::edges`].
//! 2. Each tet's six local-edge contributions carry a **sign** (the
//!    relative orientation between the local edge direction and the
//!    canonical global edge direction). When scattering local 6×6
//!    entry `(i, j)` into the global system, the value is multiplied
//!    by `s_i * s_j`.
//!
//! See `crate::elements::nedelec` for the math and orientation convention.

use std::collections::BTreeSet;

use burn::tensor::ElementConversion;
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
use burn::tensor::{IndexingUpdateOp, Int, TensorData};
use faer::Mat;

use crate::TetMesh;
use crate::assembly::{SparsityPattern, gather_tet_coords};
use crate::elements::nedelec::{
    batched_nedelec_local_mass_anisotropic_diag, batched_nedelec_local_mass_anisotropic_full,
    batched_nedelec_local_matrices, batched_nedelec_local_stiffness_weighted,
};

/// Assembled global Nédélec linear system in dense Burn-tensor form.
#[derive(Debug, Clone)]
pub struct NedelecGlobalSystem<B: Backend> {
    /// Global curl-curl stiffness matrix `[n_edges, n_edges]`.
    pub k: Tensor<B, 2>,
    /// Global mass matrix `[n_edges, n_edges]`.
    pub m: Tensor<B, 2>,
    /// `(row, col)` index pairs touched during assembly. Always
    /// symmetric for the Nédélec curl-curl / mass pair.
    pub sparsity: SparsityPattern,
}

/// Assembled global Nédélec system with **both** matrices complex:
/// full-3×3-tensor-weighted curl-curl stiffness `K(ν)` and mass
/// `M(ε)` (matched UPML, issue #199).
///
/// Where [`NedelecComplexGlobalSystem`] keeps `K` real (only ε is
/// stretched), the matched (full Sacks) UPML stretches both
/// constitutive tensors, so the curl-curl weight `ν = Λ⁻¹` is complex
/// too. Combine with [`burn_complex_mass_to_faer`] (which is weight-
/// agnostic — it just zips a Re/Im pair) on the host side.
#[derive(Debug, Clone)]
pub struct NedelecFullTensorGlobalSystem<B: Backend> {
    /// Real part of the ν-weighted curl-curl stiffness, `[n_edges, n_edges]`.
    pub k_re: Tensor<B, 2>,
    /// Imaginary part of the ν-weighted curl-curl stiffness.
    pub k_im: Tensor<B, 2>,
    /// Real part of the ε-weighted mass matrix.
    pub m_re: Tensor<B, 2>,
    /// Imaginary part of the ε-weighted mass matrix.
    pub m_im: Tensor<B, 2>,
    /// `(row, col)` index pairs touched during assembly.
    pub sparsity: SparsityPattern,
}

/// Assembled global Nédélec system with complex-valued (PML / lossy
/// dielectric) mass matrix.
///
/// Same shape as [`NedelecGlobalSystem`], but the mass is split into its
/// real and imaginary parts as two separate Burn tensors. Callers that
/// want a `faer::Mat<faer::c64>` can combine them with
/// [`burn_complex_mass_to_faer`].
#[derive(Debug, Clone)]
pub struct NedelecComplexGlobalSystem<B: Backend> {
    /// Global curl-curl stiffness matrix `[n_edges, n_edges]` (real).
    pub k: Tensor<B, 2>,
    /// Real part of the complex mass matrix, `[n_edges, n_edges]`.
    pub m_re: Tensor<B, 2>,
    /// Imaginary part of the complex mass matrix, `[n_edges, n_edges]`.
    pub m_im: Tensor<B, 2>,
    /// `(row, col)` index pairs touched during assembly.
    pub sparsity: SparsityPattern,
}

/// Assemble dense global Nédélec stiffness and mass matrices from
/// per-element local matrices, preserving autodiff through 1-D
/// `scatter(0, …, Add)`.
///
/// # Arguments
///
/// * `nodes` — `[n_nodes, 3]` global node coordinates.
/// * `tets`  — `[n_elem, 4]` connectivity (0-based linear node indices).
/// * `tet_edge_idx` — `[n_elem, 6]` global edge index for each local
///   edge of each tet (from [`TetMesh::tet_edges`]).
/// * `tet_edge_sign` — `[n_elem, 6]` per-DOF orientation sign in `{-1, +1}`
///   as `f32`.
/// * `n_edges` — size of the global linear system. Usually equal to
///   `mesh.edges().len()`.
///
/// # Returns
///
/// A [`NedelecGlobalSystem`] with dense `K` and `M` plus the
/// [`SparsityPattern`] of unique non-zero entries.
pub fn assemble_global_nedelec<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
) -> NedelecGlobalSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");

    // 1. Compute element-local Nédélec stiffness and mass.
    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords);

    // 2. Build the per-element sign tensor `[n_elem, 6]` and the
    //    outer product `[n_elem, 6, 6]` of signs. This multiplies
    //    `k_local` and `m_local` to account for orientation flips.
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
    // Outer product per element: sign[:, i] * sign[:, j].
    let sign_row = sign_2d.clone().unsqueeze_dim::<3>(2); // [n_elem, 6, 1]
    let sign_col = sign_2d.unsqueeze_dim::<3>(1); // [n_elem, 1, 6]
    let sign_outer = sign_row.mul(sign_col); // [n_elem, 6, 6]

    let k_signed = local.k_local.mul(sign_outer.clone());
    let m_signed = local.m_local.mul(sign_outer);

    // 3. Build flat linear indices: `tet_edge_idx[e, i] * n_edges + tet_edge_idx[e, j]`
    //    for every (e, i, j). Same 1-D scatter pattern as P1 assembly.
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 36);
    let n_edges_i32 = n_edges as i32;
    for row in tet_edge_idx {
        for i in 0..6 {
            for j in 0..6 {
                linear_idx.push(row[i] as i32 * n_edges_i32 + row[j] as i32);
            }
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 36]), &device);

    // 4. Flatten local values to [n_elem * 36] and scatter-add into
    //    a flat [n_edges * n_edges] zero tensor. Autodiff flows
    //    through the values via IndexingUpdateOp::Add.
    let k_flat = k_signed.reshape([n_elem * 36]);
    let m_flat = m_signed.reshape([n_elem * 36]);

    let zeros_flat = Tensor::<B, 1>::zeros([n_edges * n_edges], &device);
    let k_flat_assembled =
        zeros_flat
            .clone()
            .scatter(0, flat_indices.clone(), k_flat, IndexingUpdateOp::Add);
    let m_flat_assembled = zeros_flat.scatter(0, flat_indices, m_flat, IndexingUpdateOp::Add);

    let k = k_flat_assembled.reshape([n_edges, n_edges]);
    let m = m_flat_assembled.reshape([n_edges, n_edges]);

    // 5. Sparsity pattern from host-side tet_edge_idx.
    let sparsity = sparsity_pattern_from_tet_edges(tet_edge_idx);

    NedelecGlobalSystem { k, m, sparsity }
}

/// Build the Nédélec edge-DOF [`SparsityPattern`] from the host-side
/// per-tet global edge indices: every `(tet_edge_idx[e][i],
/// tet_edge_idx[e][j])` pair contributes one entry, duplicates
/// collapsed. Entries are **sorted lexicographically** by `(row, col)`
/// (BTreeSet iteration order) — the invariant the slot binary search of
/// [`NedelecScatterMap`] relies on.
pub fn sparsity_pattern_from_tet_edges(tet_edge_idx: &[[u32; 6]]) -> SparsityPattern {
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for row in tet_edge_idx {
        for i in 0..6 {
            for j in 0..6 {
                set.insert((row[i], row[j]));
            }
        }
    }
    let mut rows = Vec::with_capacity(set.len());
    let mut cols = Vec::with_capacity(set.len());
    for (r, c) in set {
        rows.push(r);
        cols.push(c);
    }
    SparsityPattern { rows, cols }
}

/// Binary search a `(row, col)` pair in the lexicographically sorted
/// pattern produced by [`sparsity_pattern_from_tet_edges`]. Returns the
/// slot index, or `None` if the entry is not in the pattern.
fn pattern_slot(pattern: &SparsityPattern, row: u32, col: u32) -> Option<usize> {
    let key = (row, col);
    let mut lo = 0usize;
    let mut hi = pattern.rows.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if (pattern.rows[mid], pattern.cols[mid]) < key {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    if lo < pattern.rows.len() && (pattern.rows[lo], pattern.cols[lo]) == key {
        Some(lo)
    } else {
        None
    }
}

/// Host-side scatter map from per-element local 6×6 entries to slots of
/// the sorted global [`SparsityPattern`] (issue #218).
///
/// The dense assemblers in this module scatter into a flat
/// `[n_edges * n_edges]` tensor with `row * n_edges + col` linear
/// indices, which (a) overflows Burn's i32 Int index for
/// `n_edges > 46_340` and (b) costs O(n_edges²) memory. This map
/// precomputes, for every `(element, i, j)` local entry, the index of
/// its `(row, col)` pair in the sorted pattern, so the same
/// autodiff-preserving 1-D `scatter(0, …, Add)` can target a flat
/// `[nnz]` tensor instead. For a tet mesh `nnz ≈ 15–20 × n_edges`
/// (~1 M entries at the 54 k-edge spiral benchmark fixture, vs ~3 G
/// dense), and i32 slot indices stay safe to ~2.1 B non-zeros —
/// comfortably past the 100 k-edge direct-sparse-LU budget.
///
/// Construction is O(36 · n_elem · log nnz) host work. The indices are
/// pure integers (no autodiff flows through them); gradients flow
/// through the scattered *values* exactly as on the dense path.
#[derive(Debug, Clone)]
pub struct NedelecScatterMap {
    pattern: SparsityPattern,
    /// Pattern-slot index of every `(e, i, j)` local entry, `[n_elem * 36]`.
    slot_idx: Vec<i32>,
}

impl NedelecScatterMap {
    /// Build the map from the per-tet global edge indices
    /// (`tet_edge_idx` from [`TetMesh::tet_edges`]).
    ///
    /// # Panics
    ///
    /// Panics if the pattern's `nnz` exceeds `i32::MAX` (the Burn Int
    /// tensor index limit — ~2.1 B non-zeros).
    pub fn new(tet_edge_idx: &[[u32; 6]]) -> Self {
        let pattern = sparsity_pattern_from_tet_edges(tet_edge_idx);
        assert!(
            pattern.nnz() <= i32::MAX as usize,
            "sparsity pattern nnz {} exceeds the i32 Burn Int index range",
            pattern.nnz()
        );
        let mut slot_idx = Vec::with_capacity(tet_edge_idx.len() * 36);
        for row in tet_edge_idx {
            for i in 0..6 {
                for j in 0..6 {
                    let slot = pattern_slot(&pattern, row[i], row[j])
                        .expect("every (e, i, j) entry is in the pattern by construction");
                    slot_idx.push(slot as i32);
                }
            }
        }
        Self { pattern, slot_idx }
    }

    /// The sorted sparsity pattern the slot indices point into.
    pub fn pattern(&self) -> &SparsityPattern {
        &self.pattern
    }

    /// Number of unique non-zero entries — the length of every `[nnz]`
    /// value tensor assembled through this map.
    pub fn nnz(&self) -> usize {
        self.pattern.nnz()
    }

    /// Number of elements the map was built from.
    pub fn n_elem(&self) -> usize {
        self.slot_idx.len() / 36
    }

    /// Pattern slot of a global `(row, col)` entry, if present
    /// (O(log nnz) binary search). Lets host-side surface assemblies
    /// (port / Leontovich masses, whose entries are subsets of the
    /// volume pattern) align their values with the `[nnz]` layout.
    pub fn slot_of(&self, row: u32, col: u32) -> Option<usize> {
        pattern_slot(&self.pattern, row, col)
    }

    /// Upload the per-`(e, i, j)` slot indices as a Burn Int tensor
    /// `[n_elem * 36]` for the 1-D scatter.
    fn slot_tensor<B: Backend>(&self, device: &B::Device) -> Tensor<B, 1, Int> {
        Tensor::<B, 1, Int>::from_data(
            TensorData::new(self.slot_idx.clone(), [self.slot_idx.len()]),
            device,
        )
    }
}

/// Per-element orientation-sign outer product `[n_elem, 6, 6]` —
/// `sign[e, i] * sign[e, j]` — shared by the sparse assemblers. Same
/// arithmetic (f32 upload, broadcasted multiply) as the inline code of
/// the dense assemblers, so the two paths agree bitwise.
fn sign_outer_tensor<B: Backend>(tet_edge_sign: &[[i8; 6]], device: &B::Device) -> Tensor<B, 3> {
    let n_elem = tet_edge_sign.len();
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), device);
    let sign_row = sign_2d.clone().unsqueeze_dim::<3>(2); // [n_elem, 6, 1]
    let sign_col = sign_2d.unsqueeze_dim::<3>(1); // [n_elem, 1, 6]
    sign_row.mul(sign_col)
}

/// Scatter one signed `[n_elem, 6, 6]` local tensor into a flat `[nnz]`
/// value tensor through the precomputed pattern-slot indices. Autodiff
/// flows through the values via `IndexingUpdateOp::Add`, exactly as on
/// the dense `[n_edges²]` path.
fn scatter_to_pattern_vals<B: Backend>(
    signed_local: Tensor<B, 3>,
    slot_indices: &Tensor<B, 1, Int>,
    nnz: usize,
    device: &B::Device,
) -> Tensor<B, 1> {
    let [n_elem, _, _] = signed_local.dims();
    Tensor::<B, 1>::zeros([nnz], device).scatter(
        0,
        slot_indices.clone(),
        signed_local.reshape([n_elem * 36]),
        IndexingUpdateOp::Add,
    )
}

/// Sparse-values counterpart of [`NedelecComplexGlobalSystem`]
/// (issue #218): the same assembly arithmetic, but each matrix is a
/// flat `[nnz]` Burn value tensor aligned with the sorted
/// [`SparsityPattern`] of the [`NedelecScatterMap`] it was assembled
/// through, instead of a dense `[n_edges, n_edges]` matrix. Peak memory
/// is O(nnz), and no i32 linear-index overflow occurs for
/// `n_edges > 46_340`.
#[derive(Debug, Clone)]
pub struct NedelecSparseComplexSystem<B: Backend> {
    /// Curl-curl stiffness values (real), `[nnz]` in pattern order.
    pub k_vals: Tensor<B, 1>,
    /// Real part of the ε-weighted mass values, `[nnz]`.
    pub m_re_vals: Tensor<B, 1>,
    /// Imaginary part of the ε-weighted mass values, `[nnz]`.
    pub m_im_vals: Tensor<B, 1>,
}

/// Sparse-values counterpart of [`NedelecFullTensorGlobalSystem`]
/// (matched UPML, issue #199) — see [`NedelecSparseComplexSystem`] for
/// the layout convention.
#[derive(Debug, Clone)]
pub struct NedelecSparseFullTensorSystem<B: Backend> {
    /// Real part of the ν-weighted curl-curl stiffness values, `[nnz]`.
    pub k_re_vals: Tensor<B, 1>,
    /// Imaginary part of the ν-weighted curl-curl stiffness values.
    pub k_im_vals: Tensor<B, 1>,
    /// Real part of the ε-weighted mass values, `[nnz]`.
    pub m_re_vals: Tensor<B, 1>,
    /// Imaginary part of the ε-weighted mass values, `[nnz]`.
    pub m_im_vals: Tensor<B, 1>,
}

/// Build a boolean mask `[n_edges]` marking each global edge as either
/// **interior** (`true`) or **on the PEC boundary** (`false`).
///
/// An edge is treated as on the boundary iff **both** endpoints lie on
/// the boundary surface of the mesh. The caller supplies a per-node
/// boolean array indicating boundary-ness; the typical use is the
/// rectangular cube box, where a node is on the boundary iff any of
/// its coordinates equals the box min or max.
///
/// For PEC (perfect electric conductor) BC, `n × E = 0` on the
/// boundary surface, which forces every edge DOF whose edge lies on
/// the surface to zero. The returned mask identifies the **kept**
/// interior edges.
pub fn pec_interior_edge_mask(edges: &[[u32; 2]], on_boundary: &[bool]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = e[0] as usize;
            let b = e[1] as usize;
            !(on_boundary[a] && on_boundary[b])
        })
        .collect()
}

/// Convenience: identify nodes on the boundary of `cube_tet_mesh(n, side)`
/// (any coordinate equal to `0` or `side`) and return the interior-edge
/// mask via [`pec_interior_edge_mask`].
///
/// Returns `(edges, interior_mask)` so the caller can locate the
/// interior edge indices without re-deriving the edge list.
pub fn cube_pec_interior_edges(mesh: &TetMesh, side: f64) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-9 * side.max(1.0);
    let on_boundary: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|n| n.iter().any(|&c| c.abs() < tol || (c - side).abs() < tol))
        .collect();
    let edges = mesh.edges();
    let mask = pec_interior_edge_mask(&edges, &on_boundary);
    (edges, mask)
}

/// PEC interior-edge mask for the sphere-in-vacuum mesh fixture.
///
/// A node is treated as "on the outer PEC wall" iff its radius
/// `r = |p|` is within `tol` of `r_outer`. An edge is **interior**
/// (`mask[e] == true`) iff at least one endpoint is strictly inside
/// `r_outer`; equivalently, an edge is PEC-eliminated iff **both**
/// endpoints lie on the outer sphere. This matches the same
/// `both-endpoints-on-boundary` convention as
/// [`pec_interior_edge_mask`] and the cube helper.
///
/// Returns `(edges, interior_mask)`.
pub fn sphere_pec_interior_edges(mesh: &TetMesh, r_outer: f64) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-6 * r_outer.max(1.0);
    let on_boundary: Vec<bool> = mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (r - r_outer).abs() < tol
        })
        .collect();
    let edges = mesh.edges();
    let mask = pec_interior_edge_mask(&edges, &on_boundary);
    (edges, mask)
}

/// Count nodes that lie strictly inside `r_outer` (i.e. **not** on the
/// outer PEC sphere). The Nédélec gradient nullspace has dimension
/// equal to the number of interior nodes after PEC elimination — use
/// this to size the spurious-mode filter.
pub fn sphere_n_interior_nodes(mesh: &TetMesh, r_outer: f64) -> usize {
    let tol = 1e-6 * r_outer.max(1.0);
    mesh.nodes
        .iter()
        .filter(|p| {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (r - r_outer).abs() >= tol
        })
        .count()
}

/// Relative threshold for "near-zero singular value" used by the
/// [`rank_via_svd`] rank counter on the de-Rham `d⁰_interior` operator.
///
/// With `σ_max = O(1)` for an incidence matrix (Poincaré spectral floor
/// `σ ≳ O(1)` set by mesh connectivity), this puts the absolute cutoff
/// at ~1e-12 — three orders below the smallest non-kernel σ observed on
/// the bundled cube and sphere fixtures and two orders above the f64
/// SVD-noise floor (~1e-14 · σ_max). Matches the constant the
/// `tests/derham_kernel_dim.rs` precedent uses.
pub const DERHAM_RANK_THRESHOLD_REL: f64 = 1e-12;

/// Build the dense interior×interior restriction of the de-Rham `d⁰`
/// operator directly from the mesh edge list.
///
/// Each edge contributes exactly two `±1.0` entries (lower-tag endpoint
/// `= -1`, higher-tag `= +1`), filtered to
/// `edge_mask[i] && node_mask[a] && node_mask[b]` per the de-Rham
/// Phase 1 convention (see [`crate::derham::gradient_map`] for the sign
/// convention and the underlying sparse operator).
///
/// Returns the dense matrix in `faer::Mat` (column-major) layout —
/// `nrows = #interior_edges`, `ncols = #interior_nodes`. Equivalent to
/// taking `gradient_map(mesh)` (sparse) and slicing rows by `edge_mask`,
/// columns by `node_mask`, but avoids the sparse-to-dense densify step
/// that faer 0.24 does not directly expose.
///
/// `edge_mask.len()` must equal `mesh.edges().len()` and
/// `node_mask.len()` must equal `mesh.n_nodes()`.
///
/// This was originally lifted from
/// `crates/geode-core/tests/derham_kernel_dim.rs` (Issue #81) so it can
/// be reused by integration tests, the geode-validation cross-checks,
/// and downstream callers without copy-pasting the dense materialisation.
pub fn restrict_gradient_dense(mesh: &TetMesh, edge_mask: &[bool], node_mask: &[bool]) -> Mat<f64> {
    // Map global node index → interior column (None if boundary).
    let mut node_to_interior: Vec<Option<usize>> = Vec::with_capacity(node_mask.len());
    let mut n_interior_nodes = 0usize;
    for &b in node_mask {
        if b {
            node_to_interior.push(Some(n_interior_nodes));
            n_interior_nodes += 1;
        } else {
            node_to_interior.push(None);
        }
    }

    // Map global edge index → interior row (None if boundary edge).
    let mut edge_to_interior: Vec<Option<usize>> = Vec::with_capacity(edge_mask.len());
    let mut n_interior_edges = 0usize;
    for &b in edge_mask {
        if b {
            edge_to_interior.push(Some(n_interior_edges));
            n_interior_edges += 1;
        } else {
            edge_to_interior.push(None);
        }
    }

    let edges = mesh.edges();
    assert_eq!(
        edges.len(),
        edge_mask.len(),
        "edge_mask must align with mesh.edges()"
    );
    assert_eq!(
        node_mask.len(),
        mesh.n_nodes(),
        "node_mask must align with mesh.n_nodes()"
    );

    // Start with a zero matrix and stamp ±1.0 at the two endpoint
    // columns (when both endpoints survive the node mask). Boundary
    // endpoints simply produce zero-column entries — this matches
    // "drop column k" in the matrix sense.
    let mut d0 = Mat::<f64>::zeros(n_interior_edges, n_interior_nodes);
    for (edge_idx, &[a, b]) in edges.iter().enumerate() {
        let Some(row) = edge_to_interior[edge_idx] else {
            continue;
        };
        if let Some(col) = node_to_interior[a as usize] {
            d0[(row, col)] = -1.0;
        }
        if let Some(col) = node_to_interior[b as usize] {
            d0[(row, col)] = 1.0;
        }
    }
    d0
}

/// Count singular values above `threshold_rel · σ_max` for the dense
/// `d⁰_interior` operator built by [`restrict_gradient_dense`].
///
/// Returns the rank (= number of singular values above the cutoff).
/// The full-rank case for `d⁰_interior` (the discrete gradient) has a
/// very clean spectral gap: the smallest σ above the kernel is set by
/// the mesh connectivity (a discrete Poincaré constant ~`O(1)`), while
/// the kernel σ's are mathematically zero. With
/// `threshold_rel = `[`DERHAM_RANK_THRESHOLD_REL`] (`1e-12`), the cutoff
/// sits well above the f64 SVD-noise floor (~1e-14 · σ_max) and well
/// below the smallest non-kernel σ on either fixture.
///
/// # Panics
///
/// Panics if `faer::Mat::singular_values()` returns `None` (an internal
/// SVD failure — should not happen on a finite incidence matrix).
pub fn rank_via_svd(d0: &Mat<f64>, threshold_rel: f64) -> usize {
    let sigmas = d0
        .as_ref()
        .singular_values()
        .expect("dense SVD of d⁰_interior failed");
    // Sorted descending per faer docs.
    let sigma_max = sigmas.first().copied().unwrap_or(0.0);
    let threshold = threshold_rel * sigma_max;
    sigmas.iter().filter(|&&s| s > threshold).count()
}

/// Spurious-mode dimension via the de-Rham `d⁰` operator.
///
/// Returns `rank(d⁰_interior)`, which is the algebraically correct
/// number of spurious near-zero eigenvalues of the Nédélec curl-curl
/// generalized pencil `(K_int, M_int)` after Dirichlet (PEC) reduction.
/// The Nédélec curl-curl kernel is the image of the discrete gradient
/// (`kernel(K) = image(d⁰)` per Epic #57, Phase 3.A; see
/// `tests/derham_kernel_dim.rs::cube_pec_kernel_dim_matches_d0_rank`),
/// so its rank is the spurious-mode count.
///
/// Unlike the deprecated largest-relative-gap eigenvalue heuristic,
/// this classifier has no calibration knob and gives the algebraically
/// correct answer for any PEC fixture with any number of low-lying
/// physical eigenvalue clusters.
///
/// `edge_mask.len()` must equal `mesh.edges().len()` and
/// `node_mask.len()` must equal `mesh.n_nodes()`.
pub fn spurious_dim_from_derham(mesh: &TetMesh, edge_mask: &[bool], node_mask: &[bool]) -> usize {
    let d0 = restrict_gradient_dense(mesh, edge_mask, node_mask);
    rank_via_svd(&d0, DERHAM_RANK_THRESHOLD_REL)
}

/// Per-node "strictly inside the outer PEC sphere" mask for the bundled
/// sphere fixture. A node with radius `|p|` within `tol` of `r_outer`
/// is on the wall (`false`); every other node is interior (`true`).
///
/// Companion to [`sphere_pec_interior_edges`] (which returns the edge-
/// side mask). Together the pair gives the inputs for
/// [`spurious_dim_from_derham`] on the sphere PEC case.
pub fn sphere_pec_node_interior_mask(mesh: &TetMesh, r_outer: f64) -> Vec<bool> {
    let tol = 1e-6 * r_outer.max(1.0);
    mesh.nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (r - r_outer).abs() >= tol
        })
        .collect()
}

/// Build a per-tet relative permittivity vector for the bundled
/// sphere-in-vacuum fixture, parameterized by the dielectric refractive
/// index `n_inside`.
///
/// Tets with physical tag [`crate::mesh::PHYS_SPHERE_INTERIOR`] receive
/// `epsilon_r = n_inside.powi(2)`; tets in any of the surrounding
/// vacuum regions ([`crate::mesh::PHYS_VACUUM_GAP`] and
/// [`crate::mesh::PHYS_PML_SHELL`], plus any unexpected tag for
/// defensive default) receive `epsilon_r = 1.0`.
///
/// `physical_tags.len()` must equal the number of tets in the mesh.
pub fn build_epsilon_r(physical_tags: &[i32], n_inside: f64) -> Vec<f64> {
    let eps_inside = n_inside * n_inside;
    physical_tags
        .iter()
        .map(|&t| {
            if t == crate::mesh::PHYS_SPHERE_INTERIOR {
                eps_inside
            } else {
                1.0
            }
        })
        .collect()
}

/// Compute the per-tet centroid radius `|c_e| = |(p₀ + p₁ + p₂ + p₃) / 4|`
/// for every tet in `mesh`. Returned in `mesh.tets` order.
///
/// Used by the PML profile in
/// [`build_complex_epsilon_r_pml`] to decide which tets sit in the
/// absorbing layer and how strongly to absorb in each.
pub fn tet_centroid_radii(mesh: &TetMesh) -> Vec<f64> {
    mesh.tets
        .iter()
        .map(|tet| {
            let mut c = [0.0_f64; 3];
            for &v in tet {
                let p = mesh.nodes[v as usize];
                c[0] += p[0];
                c[1] += p[1];
                c[2] += p[2];
            }
            c[0] *= 0.25;
            c[1] *= 0.25;
            c[2] *= 0.25;
            (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
        })
        .collect()
}

/// Build a per-tet **complex** relative permittivity vector that
/// realizes a scalar-isotropic PML in the outer absorbing shell of the
/// bundled sphere fixture.
///
/// This is a UPML-reduced-to-isotropic approximation (sometimes called a
/// "lossy buffer" or scalar PML): instead of stretching the spatial
/// coordinate via a tensor `Λ = diag(s_r, s_θ, s_φ)`, we collapse the
/// stretching to a scalar `s_r → 1 + (i/ω) σ(r)` and absorb it into a
/// scalar complex ε. It is **less effective** than a properly
/// anisotropic split-field PML — the tangential field components do not
/// see the absorption — but it requires no changes to the constitutive
/// tensor data path and is a defensible v0 starting point.
///
/// # Profile
///
/// - Tet in `sphere_interior` (`r_c ≤ R_SPHERE`): `ε = n_inside² + 0j`
///   (real dielectric).
/// - Tet in `vacuum_gap` (`R_SPHERE < r_c ≤ R_PML_INNER`): `ε = 1 + 0j`
///   (real vacuum — no absorption inside the gap).
/// - Tet in `pml_shell` (`R_PML_INNER < r_c ≤ R_BUFFER`): smooth
///   quadratic absorption ramp anchored at the PML inner interface,
///
///   ```text
///   ε(r) = 1 − j σ₀ ((r_c − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²
///   ```
///
///   The `r → R_PML_INNER` limit returns `ε = 1` (matches the vacuum
///   gap) and the `r → R_BUFFER` limit gives the full absorption
///   `ε = 1 − jσ₀`. The quadratic profile is the standard low-
///   reflection start point for discrete PMLs.
///
/// # σ₀ tuning
///
/// The σ₀ parameter sets the peak absorption at the outer wall. The
/// bundled sphere test uses `σ₀ = 5.0`, which is **picked by hand** for
/// the bundled fixture's coarse mesh and roughly resonant k₀ ≈ 2
/// (dielectric n = 1.5, R_SPHERE = 1.0 → ground k ≈ π / (n·R) ≈ 2.1).
/// It is not derived from a target reflection-coefficient calculation.
///
/// Rule-of-thumb tuning for a quadratic ramp of thickness
/// `L = R_BUFFER − R_PML_INNER` and operating wavenumber `k₀`:
///
/// - Theoretical reflection of the scalar-ε approximation is roughly
///   `R(θ) ≈ exp(−2 σ₀ L cos θ · k₀ / 3)` (the factor of 3 comes from
///   integrating the quadratic profile, after Bérenger 1994 / Taflove
///   §7.7). For our fixture `L = 0.5`, `k₀ ≈ 2`, so `σ₀ ≈ 5` gives
///   `R(0) ≈ e⁻³·³³ ≈ 0.036` at normal incidence — small enough to
///   keep PML-induced reflections below the dominant discretization
///   error on our coarse mesh, but not so large that the discrete PML
///   itself starts to reflect.
///   This is the **normal-incidence plane-wave** reflection
///   coefficient on a flat slab; the curved wavefronts emitted by a
///   sphere have a different effective absorption (and the angle θ
///   here is the local incidence angle on the PML inner interface,
///   not the polar coordinate), so the formula is an order-of-
///   magnitude guide rather than a precise reflection budget.
/// - As a working rule, scale `σ₀ ∝ √ω` when changing the operating
///   frequency: low-k modes need stronger absorption per unit length
///   to attenuate the longer wavelength, while very-high-k modes are
///   already well-trapped and tolerate weaker σ₀.
/// - If you refine the mesh inside the PML, you can usually push σ₀
///   higher without exciting numerical reflections.
/// - **Practical upper limit on the bundled fixture.** The layered
///   sphere fixture (post #42) sits near `Q ≈ 5.7` at `σ₀ = 5`, with
///   headroom to push higher. The empirical sweet spot for this mesh
///   density is roughly `σ₀ ∈ [5, 50]`: below 5 the absorption is too
///   weak and outgoing energy reflects off the outer PEC wall; above
///   ~50 the discrete jump in `Im(ε)` at the PML inner interface
///   starts to dominate, and the reflection from the **discrete**
///   interface (not the analytic profile) sets the floor. Mesh
///   refinement inside the shell pushes this ceiling up.
/// - `σ₀ = 0` reduces this routine to a real `ε = 1` everywhere
///   outside the dielectric (vacuum on both shells), recovering the
///   PEC-sphere eigenproblem. The `sphere_pml_eigenmode_sigma_zero`
///   regression test exercises this limit.
///
/// # Sign convention
///
/// We use the `exp(+jωt)` time convention (same as the rest of the
/// codebase — see `silvermuller.rs`). Outgoing-wave attenuation
/// therefore requires `Im(ε) < 0`, which is what the ramp produces.
///
/// `physical_tags.len()` and `centroid_radii.len()` must both equal the
/// number of tets in the mesh.
pub fn build_complex_epsilon_r_pml(
    physical_tags: &[i32],
    centroid_radii: &[f64],
    n_inside: f64,
    sigma_0: f64,
) -> Vec<faer::c64> {
    use crate::mesh::{R_BUFFER, R_PML_INNER};
    assert_eq!(
        physical_tags.len(),
        centroid_radii.len(),
        "physical_tags and centroid_radii length mismatch"
    );
    let eps_inside = n_inside * n_inside;
    let width = R_BUFFER - R_PML_INNER;

    physical_tags
        .iter()
        .zip(centroid_radii.iter())
        .map(|(&tag, &r_c)| {
            if tag == crate::mesh::PHYS_SPHERE_INTERIOR {
                faer::c64::new(eps_inside, 0.0)
            } else if tag == crate::mesh::PHYS_PML_SHELL {
                // Absorbing layer — apply quadratic PML ramp anchored
                // at r = R_PML_INNER. Clamp to the [0, 1] normalized
                // range so any tet whose centroid drifts slightly
                // outside R_BUFFER does not overshoot.
                let u = ((r_c - R_PML_INNER) / width).clamp(0.0, 1.0);
                let im = -sigma_0 * u * u;
                faer::c64::new(1.0, im)
            } else {
                // Vacuum gap (or any unrecognised tag): real vacuum.
                faer::c64::new(1.0, 0.0)
            }
        })
        .collect()
}

/// Variant of [`assemble_global_nedelec`] that takes a per-element
/// relative permittivity `epsilon_r: [n_elem]` and scales the mass
/// matrix accordingly: `M_e ← epsilon_r[e] * M_e`. Stiffness (curl-
/// curl) is unchanged.
///
/// Scaling at the element level is mathematically equivalent to
/// scaling the integrand `∫ φ_i · φ_j dV` by a piecewise-constant
/// `ε(x)` whose value on element `e` is `epsilon_r[e]`. The kernel
/// itself stays vacuum-agnostic; the per-region material assignment
/// lives in the caller (or here, in this thin wrapper).
pub fn assemble_global_nedelec_with_epsilon<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    epsilon_r: &[f64],
) -> NedelecGlobalSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(epsilon_r.len(), n_elem, "epsilon_r length mismatch");

    // 1. Compute element-local Nédélec stiffness and mass.
    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords);

    // 1b. Scale per-element mass by epsilon_r via broadcasting.
    //     eps tensor has shape [n_elem]; unsqueeze to [n_elem, 1, 1]
    //     so the multiply broadcasts across the 6×6 block.
    let eps_flat: Vec<f32> = epsilon_r.iter().map(|&e| e as f32).collect();
    let eps_1d = Tensor::<B, 1>::from_data(TensorData::new(eps_flat, [n_elem]), &device);
    let eps_3d = eps_1d.unsqueeze_dim::<2>(1).unsqueeze_dim::<3>(2); // [n_elem, 1, 1]
    let m_local_scaled = local.m_local.mul(eps_3d);

    // 2. Build the per-element sign tensor `[n_elem, 6]` and the
    //    outer product `[n_elem, 6, 6]` of signs. This multiplies
    //    `k_local` and `m_local` to account for orientation flips.
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

    // 3. Build flat linear indices.
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 36);
    let n_edges_i32 = n_edges as i32;
    for row in tet_edge_idx {
        for i in 0..6 {
            for j in 0..6 {
                linear_idx.push(row[i] as i32 * n_edges_i32 + row[j] as i32);
            }
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 36]), &device);

    // 4. Scatter-add into a flat zero tensor.
    let k_flat = k_signed.reshape([n_elem * 36]);
    let m_flat = m_signed.reshape([n_elem * 36]);

    let zeros_flat = Tensor::<B, 1>::zeros([n_edges * n_edges], &device);
    let k_flat_assembled =
        zeros_flat
            .clone()
            .scatter(0, flat_indices.clone(), k_flat, IndexingUpdateOp::Add);
    let m_flat_assembled = zeros_flat.scatter(0, flat_indices, m_flat, IndexingUpdateOp::Add);

    let k = k_flat_assembled.reshape([n_edges, n_edges]);
    let m = m_flat_assembled.reshape([n_edges, n_edges]);

    let sparsity = sparsity_pattern_from_tet_edges(tet_edge_idx);

    NedelecGlobalSystem { k, m, sparsity }
}

/// Assemble the global Nédélec **conductivity damping matrix**
/// `C_ij = ∫ N_i · N_j σ(x) dV` for a piecewise-constant per-tet
/// electrical conductivity `σ` (issue #196).
///
/// # Where C enters the physics
///
/// In the frequency domain (`exp(+jωt)` convention) a finite
/// conductivity adds the conduction current `J_c = σE` to Ampère's
/// law, which is equivalent to the effective complex permittivity
///
/// ```text
/// ε_eff(ω) = ε − i σ/ω        (Im(ε_eff) < 0 ⇒ absorption)
/// ```
///
/// The discrete driven system becomes (natural units, `c = μ₀ = ε₀ = 1`)
///
/// ```text
/// A(ω) = K + iω C(σ) − ω² M(ε)  ≡  K − ω² M(ε_eff(ω)).
/// ```
///
/// **Chosen factorization-friendly form**: this codebase keeps `C`
/// separate rather than folding σ into a frequency-dependent complex
/// mass, because `K`, `M`, and `C` are all ω-independent — a frequency
/// sweep (the Epic #193 use case: R(ω)/L(ω) extraction) re-forms
/// `A(ω)` per frequency by a cheap linear combination instead of
/// re-running the Burn assembly. The two forms are algebraically
/// identical; [`build_complex_epsilon_eff`] provides the folded
/// `ε_eff` bridge for the fixed-ω eigenpencil path.
///
/// # Complex symmetry
///
/// `C` is a σ-weighted *real symmetric* mass matrix, so adding `iωC`
/// preserves the established complex-symmetric (NOT Hermitian) pencil
/// invariant (`Aᵀ = A`, see README "Math correctness" / PR #55).
///
/// # Units
///
/// `σ` is in natural units `1/length`: `σ_nat = σ_SI · Z₀ · L_unit`
/// with `Z₀ = √(μ₀/ε₀) ≈ 376.73 Ω` and `L_unit` the metres per mesh
/// length unit. The analytic skin depth is `δ = √(2/(ω σ_nat))`.
///
/// # Implementation / autodiff
///
/// The integrand is identical to the ε-weighted mass, so this is a
/// thin wrapper over [`assemble_global_nedelec_with_epsilon`] with the
/// per-tet weights set to `σ`, keeping the autodiff-preserving batched
/// local kernel + 1-D `scatter(0, …, Add)` path intact.
pub fn assemble_nedelec_sigma_damping<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    sigma_tet: &[f64],
) -> Tensor<B, 2> {
    assemble_global_nedelec_with_epsilon(
        nodes,
        tets,
        tet_edge_idx,
        tet_edge_sign,
        n_edges,
        sigma_tet,
    )
    .m
}

/// Fold a per-tet conductivity into the per-tet complex permittivity:
/// `ε_eff = ε − i σ/ω` (`exp(+jωt)` convention, natural units — see
/// [`assemble_nedelec_sigma_damping`] for the conventions and the
/// equivalence `K − ω²M(ε_eff) = K + iωC(σ) − ω²M(ε)`).
///
/// This is the bridge for the **eigenpencil** path: a fixed-ω complex-
/// symmetric eigensolve with lossy volumetric materials reuses the
/// existing [`assemble_global_nedelec_with_complex_epsilon`] machinery
/// unchanged by passing the folded `ε_eff`.
///
/// # Panics
///
/// Panics if the slice lengths disagree or `omega <= 0` (the fold is
/// singular at ω = 0).
pub fn build_complex_epsilon_eff(
    epsilon_r: &[faer::c64],
    sigma_tet: &[f64],
    omega: f64,
) -> Vec<faer::c64> {
    assert_eq!(
        epsilon_r.len(),
        sigma_tet.len(),
        "epsilon_r and sigma_tet length mismatch"
    );
    assert!(omega > 0.0, "omega must be positive to fold sigma into eps");
    epsilon_r
        .iter()
        .zip(sigma_tet.iter())
        .map(|(&eps, &sigma)| faer::c64::new(eps.re, eps.im - sigma / omega))
        .collect()
}

/// Variant of [`assemble_global_nedelec_with_epsilon`] that accepts a
/// **complex** per-tet permittivity and returns the real K plus the
/// real/imaginary parts of the complex-scaled mass matrix.
///
/// The implementation reuses the real-ε path twice: once with
/// `Re(ε)` and once with `Im(ε)`. This avoids threading
/// `Complex<f32>` through the Burn tensor pipeline (which currently
/// has no complex dtype) and keeps the GPU/autodiff-friendly scatter
/// path intact. The caller combines `m_re + j m_im` on the host side
/// (e.g. via [`burn_complex_mass_to_faer`]) before handing the system
/// to a complex eigensolver.
///
/// # Cost
///
/// Two extra scatter passes on the mass; stiffness is assembled once
/// and shared (caller-side: see `K = sys.k`). For the bundled sphere
/// fixture (~7k edges, ~1226 tets) this is dominated by the dense
/// global matrix shape, so the cost difference vs the real path is
/// in the noise.
pub fn assemble_global_nedelec_with_complex_epsilon<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    epsilon_r_complex: &[faer::c64],
) -> NedelecComplexGlobalSystem<B> {
    assert_eq!(
        epsilon_r_complex.len(),
        tet_edge_idx.len(),
        "complex epsilon_r length mismatch"
    );

    let eps_re: Vec<f64> = epsilon_r_complex.iter().map(|c| c.re).collect();
    let eps_im: Vec<f64> = epsilon_r_complex.iter().map(|c| c.im).collect();

    // Real-part assembly produces K and Re(M); we reuse its K and
    // sparsity, then run a second assembly with eps = Im(ε) and keep
    // only the mass output.
    let sys_re = assemble_global_nedelec_with_epsilon(
        nodes.clone(),
        tets.clone(),
        tet_edge_idx,
        tet_edge_sign,
        n_edges,
        &eps_re,
    );
    let sys_im = assemble_global_nedelec_with_epsilon(
        nodes,
        tets,
        tet_edge_idx,
        tet_edge_sign,
        n_edges,
        &eps_im,
    );

    NedelecComplexGlobalSystem {
        k: sys_re.k,
        m_re: sys_re.m,
        m_im: sys_im.m,
        sparsity: sys_re.sparsity,
    }
}

/// Sparse (`[nnz]` pattern-aligned) variant of
/// [`assemble_global_nedelec_with_complex_epsilon`] (issue #218).
///
/// Same element-local kernels, same f32 weight upload, same orientation
/// signs and the same autodiff-preserving 1-D `scatter(0, …, Add)` —
/// only the scatter target changes from the dense flat `[n_edges²]`
/// tensor to a flat `[nnz]` tensor indexed by the precomputed
/// pattern-slot map. Per pattern entry, the summands accumulate in the
/// same `(e, i, j)` order as on the dense path, so the values agree
/// with the dense matrices read over the same pattern to bit precision
/// on a deterministic backend. Unlike the dense complex path (two full
/// passes through the scalar-ε assembler), the local matrices are
/// computed **once** and the stiffness is scattered once.
///
/// `scatter` must be built from the same `tet_edge_idx` used for
/// `tet_edge_sign` (see [`NedelecScatterMap::new`]).
pub fn assemble_global_nedelec_with_complex_epsilon_sparse<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_r_complex: &[faer::c64],
) -> NedelecSparseComplexSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        scatter.n_elem(),
        n_elem,
        "scatter map element count mismatch"
    );
    assert_eq!(
        epsilon_r_complex.len(),
        n_elem,
        "complex epsilon_r length mismatch"
    );

    // 1. Element-local stiffness and mass — computed once (the dense
    //    complex path runs the scalar assembler twice and discards a
    //    duplicate K).
    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords);

    // 1b. Scale per-element mass by Re(ε) / Im(ε) via broadcasting —
    //     same f32 upload as the dense scalar-ε path.
    let eps_re_flat: Vec<f32> = epsilon_r_complex.iter().map(|c| c.re as f32).collect();
    let eps_im_flat: Vec<f32> = epsilon_r_complex.iter().map(|c| c.im as f32).collect();
    let eps_re_3d = Tensor::<B, 1>::from_data(TensorData::new(eps_re_flat, [n_elem]), &device)
        .unsqueeze_dim::<2>(1)
        .unsqueeze_dim::<3>(2); // [n_elem, 1, 1]
    let eps_im_3d = Tensor::<B, 1>::from_data(TensorData::new(eps_im_flat, [n_elem]), &device)
        .unsqueeze_dim::<2>(1)
        .unsqueeze_dim::<3>(2);
    let m_re_local = local.m_local.clone().mul(eps_re_3d);
    let m_im_local = local.m_local.mul(eps_im_3d);

    // 2. Orientation-sign outer product.
    let sign_outer = sign_outer_tensor::<B>(tet_edge_sign, &device);
    let k_signed = local.k_local.mul(sign_outer.clone());
    let m_re_signed = m_re_local.mul(sign_outer.clone());
    let m_im_signed = m_im_local.mul(sign_outer);

    // 3. Scatter-add into flat [nnz] value tensors.
    let slot_indices = scatter.slot_tensor::<B>(&device);
    let nnz = scatter.nnz();
    NedelecSparseComplexSystem {
        k_vals: scatter_to_pattern_vals(k_signed, &slot_indices, nnz, &device),
        m_re_vals: scatter_to_pattern_vals(m_re_signed, &slot_indices, nnz, &device),
        m_im_vals: scatter_to_pattern_vals(m_im_signed, &slot_indices, nnz, &device),
    }
}

/// Sparse (`[nnz]` pattern-aligned) variant of
/// [`assemble_nedelec_sigma_damping`] (issue #218): the σ-weighted
/// damping values `C_ij = ∫ N_i · N_j σ dV` in pattern order. Same
/// kernels, f32 weight upload, signs and values-side autodiff as the
/// dense path — only the scatter target changes.
pub fn assemble_nedelec_sigma_damping_sparse<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    sigma_tet: &[f64],
) -> Tensor<B, 1> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        scatter.n_elem(),
        n_elem,
        "scatter map element count mismatch"
    );
    assert_eq!(sigma_tet.len(), n_elem, "sigma_tet length mismatch");

    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords);

    let sigma_flat: Vec<f32> = sigma_tet.iter().map(|&s| s as f32).collect();
    let sigma_3d = Tensor::<B, 1>::from_data(TensorData::new(sigma_flat, [n_elem]), &device)
        .unsqueeze_dim::<2>(1)
        .unsqueeze_dim::<3>(2); // [n_elem, 1, 1]
    let c_local = local.m_local.mul(sigma_3d);

    let sign_outer = sign_outer_tensor::<B>(tet_edge_sign, &device);
    let c_signed = c_local.mul(sign_outer);

    let slot_indices = scatter.slot_tensor::<B>(&device);
    scatter_to_pattern_vals(c_signed, &slot_indices, scatter.nnz(), &device)
}

/// Build a per-tet **diagonal anisotropic** complex permittivity tensor
/// in the global Cartesian basis for the bundled sphere fixture's PML
/// shell (issue #54).
///
/// This implements the diagonal-only UPML simplification:
/// `ε(x) = R · diag(1/s_r, s_t, s_t) · R^T`, restricted to its main
/// diagonal entries (`ε_x, ε_y, ε_z`). For the simplified UPML
/// `s_r = s_t = 1 - jσ(r)/ω` (Sacks et al. 1995, §III), the
/// per-tet diagonal evaluates to
///
/// ```text
/// ε_α(x) = (1/s_r) r̂_α² + s_t (1 - r̂_α²),     α ∈ {x, y, z},
/// ```
///
/// where `r̂ = c / |c|` is the outward radial unit vector at the tet
/// centroid `c`. Outside the PML shell (interior + vacuum gap) the
/// tensor reduces to the real scalar `(ε_r, ε_r, ε_r)`.
///
/// # σ(r) profile
///
/// Same quadratic ramp as [`build_complex_epsilon_r_pml`]:
/// `σ(r_c) = σ₀ · ((r_c − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²`.
/// `σ₀ = 0` collapses the tensor to the real scalar everywhere, which
/// is the regression test point: with `n_inside = 1.0` it must
/// reproduce the PEC-cavity numbers bit-identically.
///
/// # ω heuristic
///
/// The constitutive frequency `ω` is approximated by `k0_ref` (the
/// reference wavenumber convention shared with Silver-Müller). For a
/// driven eigenproblem the iteration over k₀ is left to follow-up
/// #48.
///
/// # Sign convention
///
/// `exp(+jωt)` time convention. Outgoing absorption requires
/// `Im(ε) < 0`, which is what the ramp produces.
///
/// `physical_tags.len()`, `centroid_radii.len()`, and `centroids.len()`
/// must all equal the number of tets in the mesh.
pub fn build_anisotropic_pml_tensor_diag(
    physical_tags: &[i32],
    centroids: &[[f64; 3]],
    n_inside: f64,
    sigma_0: f64,
    k0_ref: f64,
) -> Vec<[faer::c64; 3]> {
    use crate::mesh::{R_BUFFER, R_PML_INNER};
    assert_eq!(
        physical_tags.len(),
        centroids.len(),
        "physical_tags and centroids length mismatch"
    );
    let eps_inside = n_inside * n_inside;
    let width = R_BUFFER - R_PML_INNER;
    let omega = k0_ref.max(1e-12);

    physical_tags
        .iter()
        .zip(centroids.iter())
        .map(|(&tag, c)| {
            let r_c = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
            let eps_scalar = if tag == crate::mesh::PHYS_SPHERE_INTERIOR {
                eps_inside
            } else {
                1.0
            };

            if tag != crate::mesh::PHYS_PML_SHELL || r_c <= R_PML_INNER {
                // Interior dielectric or vacuum gap: real, isotropic.
                let v = faer::c64::new(eps_scalar, 0.0);
                return [v, v, v];
            }

            // PML shell: build s_r = s_t = 1 - jσ/ω and combine with
            // r̂ to form the diagonal in Cartesian.
            let u = ((r_c - R_PML_INNER) / width).clamp(0.0, 1.0);
            let sigma = sigma_0 * u * u;
            let s = faer::c64::new(1.0, -sigma / omega); // s_r = s_t = 1 - jσ/ω
            let s_inv = faer::c64::new(1.0, 0.0) / s;

            // Radial unit vector at the centroid. Guard against |c| ≈ 0
            // (PML shell tets are always well away from the origin, but
            // defensive guard is cheap).
            let inv_r = if r_c > 1e-12 { 1.0 / r_c } else { 0.0 };
            let rx = c[0] * inv_r;
            let ry = c[1] * inv_r;
            let rz = c[2] * inv_r;

            // ε_α = s_inv · r̂_α² + s · (1 - r̂_α²)
            let bg = faer::c64::new(eps_scalar, 0.0);
            let mk = |r_alpha: f64| -> faer::c64 {
                let w = r_alpha * r_alpha;
                bg * (s_inv * w + s * (1.0 - w))
            };
            [mk(rx), mk(ry), mk(rz)]
        })
        .collect()
}

/// Compute per-tet centroids `(x, y, z)` for every tet in `mesh`.
///
/// Returned in `mesh.tets` order; companion to [`tet_centroid_radii`]
/// for callers that need the full vector centroid (e.g. the
/// anisotropic PML tensor builder, which needs the radial direction
/// not just its magnitude).
pub fn tet_centroids(mesh: &TetMesh) -> Vec<[f64; 3]> {
    mesh.tets
        .iter()
        .map(|tet| {
            let mut c = [0.0_f64; 3];
            for &v in tet {
                let p = mesh.nodes[v as usize];
                c[0] += p[0];
                c[1] += p[1];
                c[2] += p[2];
            }
            c[0] *= 0.25;
            c[1] *= 0.25;
            c[2] *= 0.25;
            c
        })
        .collect()
}

/// Anisotropic-ε variant of [`assemble_global_nedelec_with_complex_epsilon`].
///
/// Accepts a **diagonal** per-tet complex permittivity tensor in the
/// global Cartesian basis (3 complex entries per tet) and assembles
/// the resulting complex mass matrix using
/// [`batched_nedelec_local_mass_anisotropic_diag`]. The stiffness
/// (curl-curl) and sparsity pattern are computed exactly as in the
/// scalar path.
///
/// Returns the same [`NedelecComplexGlobalSystem`] struct as the
/// scalar-ε complex assembler so downstream eigensolvers and PEC
/// reductions don't have to branch on the PML variant.
///
/// # Implementation
///
/// Real and imaginary parts of the per-tet tensor are pushed through
/// the new kernel as two separate Burn tensors (Re-pass, Im-pass),
/// matching the two-pass split established for the scalar complex
/// path. Stiffness is computed once via
/// [`batched_nedelec_local_matrices`] and reused — the curl-curl
/// integrand is permittivity-independent.
pub fn assemble_global_nedelec_with_anisotropic_epsilon<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    epsilon_tensor_diag: &[[faer::c64; 3]],
) -> NedelecComplexGlobalSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        epsilon_tensor_diag.len(),
        n_elem,
        "epsilon_tensor_diag length mismatch"
    );

    // 1. Compute element-local Nédélec stiffness (real, permittivity-
    //    independent) and the anisotropic mass twice (Re-pass + Im-pass).
    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords.clone());

    let eps_re_flat: Vec<f32> = epsilon_tensor_diag
        .iter()
        .flat_map(|row| row.iter().map(|c| c.re as f32))
        .collect();
    let eps_im_flat: Vec<f32> = epsilon_tensor_diag
        .iter()
        .flat_map(|row| row.iter().map(|c| c.im as f32))
        .collect();
    let eps_re_tensor =
        Tensor::<B, 2>::from_data(TensorData::new(eps_re_flat, [n_elem, 3]), &device);
    let eps_im_tensor =
        Tensor::<B, 2>::from_data(TensorData::new(eps_im_flat, [n_elem, 3]), &device);

    let m_local_re = batched_nedelec_local_mass_anisotropic_diag(coords.clone(), eps_re_tensor);
    let m_local_im = batched_nedelec_local_mass_anisotropic_diag(coords, eps_im_tensor);

    // 2. Sign outer product.
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
    let sign_row = sign_2d.clone().unsqueeze_dim::<3>(2);
    let sign_col = sign_2d.unsqueeze_dim::<3>(1);
    let sign_outer = sign_row.mul(sign_col);

    let k_signed = local.k_local.mul(sign_outer.clone());
    let m_re_signed = m_local_re.mul(sign_outer.clone());
    let m_im_signed = m_local_im.mul(sign_outer);

    // 3. Flat scatter indices.
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 36);
    let n_edges_i32 = n_edges as i32;
    for row in tet_edge_idx {
        for i in 0..6 {
            for j in 0..6 {
                linear_idx.push(row[i] as i32 * n_edges_i32 + row[j] as i32);
            }
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 36]), &device);

    // 4. Scatter-add into flat zero tensors.
    let k_flat = k_signed.reshape([n_elem * 36]);
    let m_re_flat = m_re_signed.reshape([n_elem * 36]);
    let m_im_flat = m_im_signed.reshape([n_elem * 36]);

    let zeros_flat = Tensor::<B, 1>::zeros([n_edges * n_edges], &device);
    let k_assembled =
        zeros_flat
            .clone()
            .scatter(0, flat_indices.clone(), k_flat, IndexingUpdateOp::Add);
    let m_re_assembled =
        zeros_flat
            .clone()
            .scatter(0, flat_indices.clone(), m_re_flat, IndexingUpdateOp::Add);
    let m_im_assembled = zeros_flat.scatter(0, flat_indices, m_im_flat, IndexingUpdateOp::Add);

    let k = k_assembled.reshape([n_edges, n_edges]);
    let m_re = m_re_assembled.reshape([n_edges, n_edges]);
    let m_im = m_im_assembled.reshape([n_edges, n_edges]);

    let sparsity = sparsity_pattern_from_tet_edges(tet_edge_idx);

    NedelecComplexGlobalSystem {
        k,
        m_re,
        m_im,
        sparsity,
    }
}

/// Sparse (`[nnz]` pattern-aligned) variant of
/// [`assemble_global_nedelec_with_anisotropic_epsilon`] (issue #218).
/// Same kernels (Re-pass + Im-pass through
/// [`batched_nedelec_local_mass_anisotropic_diag`]), signs and
/// values-side autodiff as the dense path — only the scatter target
/// changes. See [`assemble_global_nedelec_with_complex_epsilon_sparse`]
/// for the layout convention.
pub fn assemble_global_nedelec_with_anisotropic_epsilon_sparse<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_tensor_diag: &[[faer::c64; 3]],
) -> NedelecSparseComplexSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        scatter.n_elem(),
        n_elem,
        "scatter map element count mismatch"
    );
    assert_eq!(
        epsilon_tensor_diag.len(),
        n_elem,
        "epsilon_tensor_diag length mismatch"
    );

    // 1. Element-local stiffness (real) + anisotropic mass (Re/Im pass).
    let coords = gather_tet_coords(nodes, tets);
    let local = batched_nedelec_local_matrices(coords.clone());

    let eps_re_flat: Vec<f32> = epsilon_tensor_diag
        .iter()
        .flat_map(|row| row.iter().map(|c| c.re as f32))
        .collect();
    let eps_im_flat: Vec<f32> = epsilon_tensor_diag
        .iter()
        .flat_map(|row| row.iter().map(|c| c.im as f32))
        .collect();
    let eps_re_tensor =
        Tensor::<B, 2>::from_data(TensorData::new(eps_re_flat, [n_elem, 3]), &device);
    let eps_im_tensor =
        Tensor::<B, 2>::from_data(TensorData::new(eps_im_flat, [n_elem, 3]), &device);

    let m_local_re = batched_nedelec_local_mass_anisotropic_diag(coords.clone(), eps_re_tensor);
    let m_local_im = batched_nedelec_local_mass_anisotropic_diag(coords, eps_im_tensor);

    // 2. Signs + 3. flat [nnz] scatters.
    let sign_outer = sign_outer_tensor::<B>(tet_edge_sign, &device);
    let k_signed = local.k_local.mul(sign_outer.clone());
    let m_re_signed = m_local_re.mul(sign_outer.clone());
    let m_im_signed = m_local_im.mul(sign_outer);

    let slot_indices = scatter.slot_tensor::<B>(&device);
    let nnz = scatter.nnz();
    NedelecSparseComplexSystem {
        k_vals: scatter_to_pattern_vals(k_signed, &slot_indices, nnz, &device),
        m_re_vals: scatter_to_pattern_vals(m_re_signed, &slot_indices, nnz, &device),
        m_im_vals: scatter_to_pattern_vals(m_im_signed, &slot_indices, nnz, &device),
    }
}

/// Upload one real component (Re or Im) of a per-tet 3×3 complex
/// tensor field as a `[n_elem, 3, 3]` Burn tensor at the backend's
/// full float precision (`B::FloatElem` — f64 on the ndarray CPU
/// backend, f32 on the GPU backends; same idiom as
/// [`crate::assembly::upload_mesh`]).
fn upload_tensor33_component<B: Backend>(
    field: &[[[faer::c64; 3]; 3]],
    pick: impl Fn(&faer::c64) -> f64,
    device: &B::Device,
) -> Tensor<B, 3> {
    let n_elem = field.len();
    let flat: Vec<B::FloatElem> = field
        .iter()
        .flat_map(|t| t.iter().flat_map(|row| row.iter().map(|c| pick(c).elem())))
        .collect();
    Tensor::<B, 3>::from_data(TensorData::new(flat, [n_elem, 3, 3]), device)
}

/// Matched-UPML (full Sacks) variant of the global Nédélec assembly
/// (issue #199): **full 3×3 complex** per-tet weight tensors on both
/// the curl-curl stiffness and the mass,
///
/// ```text
/// K(ν)_ij = ∫ (∇×N_i)ᵀ ν (∇×N_j) dV,    M(ε)_ij = ∫ N_iᵀ ε N_j dV,
/// ```
///
/// with `ν = Λ⁻¹` (the `μ = Λ` stretch lands on the curl term) and
/// `ε = ε_r·Λ` for the matched UPML
/// ([`crate::scattering::upml_matched_tensors`] /
/// [`crate::scattering::build_matched_upml_materials`]). Off-diagonal
/// tensor entries are kept — unlike the diagonal-restriction
/// approximation of [`build_anisotropic_pml_tensor_diag`] — so the
/// result agrees with the host-assembled oracle
/// ([`crate::scattering::solve_scattered_field_matched_upml`]) at
/// assembly precision.
///
/// # Implementation / autodiff
///
/// Each complex weight is split into Re/Im passes through the new
/// batched kernels ([`batched_nedelec_local_stiffness_weighted`],
/// [`batched_nedelec_local_mass_anisotropic_full`]) — four kernel
/// invocations total — and scattered through the same autodiff-
/// preserving 1-D `scatter(0, …, Add)` path as every other assembler
/// in this module. Combine Re/Im pairs on the host with
/// [`burn_complex_mass_to_faer`].
///
/// # Symmetry
///
/// For symmetric weights (Λ and Λ⁻¹ are symmetric) all four outputs
/// are symmetric, preserving the complex-symmetric (`Aᵀ = A`, NOT
/// Hermitian) pencil invariant.
pub fn assemble_global_nedelec_with_full_tensors<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    epsilon_tensor: &[[[faer::c64; 3]; 3]],
    nu_tensor: &[[[faer::c64; 3]; 3]],
) -> NedelecFullTensorGlobalSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        epsilon_tensor.len(),
        n_elem,
        "epsilon_tensor length mismatch"
    );
    assert_eq!(nu_tensor.len(), n_elem, "nu_tensor length mismatch");

    // 1. Element-local weighted matrices: Re/Im pass per weight.
    let coords = gather_tet_coords(nodes, tets);
    let eps_re = upload_tensor33_component::<B>(epsilon_tensor, |c| c.re, &device);
    let eps_im = upload_tensor33_component::<B>(epsilon_tensor, |c| c.im, &device);
    let nu_re = upload_tensor33_component::<B>(nu_tensor, |c| c.re, &device);
    let nu_im = upload_tensor33_component::<B>(nu_tensor, |c| c.im, &device);

    let k_local_re = batched_nedelec_local_stiffness_weighted(coords.clone(), nu_re);
    let k_local_im = batched_nedelec_local_stiffness_weighted(coords.clone(), nu_im);
    let m_local_re = batched_nedelec_local_mass_anisotropic_full(coords.clone(), eps_re);
    let m_local_im = batched_nedelec_local_mass_anisotropic_full(coords, eps_im);

    // 2. Sign outer product (orientation flips).
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
    let sign_row = sign_2d.clone().unsqueeze_dim::<3>(2);
    let sign_col = sign_2d.unsqueeze_dim::<3>(1);
    let sign_outer = sign_row.mul(sign_col);

    // 3. Flat scatter indices (shared by all four matrices).
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 36);
    let n_edges_i32 = n_edges as i32;
    for row in tet_edge_idx {
        for i in 0..6 {
            for j in 0..6 {
                linear_idx.push(row[i] as i32 * n_edges_i32 + row[j] as i32);
            }
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 36]), &device);

    // 4. Scatter-add each signed local matrix into a flat zero tensor.
    let zeros_flat = Tensor::<B, 1>::zeros([n_edges * n_edges], &device);
    let scatter_one = |local: Tensor<B, 3>| -> Tensor<B, 2> {
        let signed = local.mul(sign_outer.clone());
        zeros_flat
            .clone()
            .scatter(
                0,
                flat_indices.clone(),
                signed.reshape([n_elem * 36]),
                IndexingUpdateOp::Add,
            )
            .reshape([n_edges, n_edges])
    };

    let k_re = scatter_one(k_local_re);
    let k_im = scatter_one(k_local_im);
    let m_re = scatter_one(m_local_re);
    let m_im = scatter_one(m_local_im);

    let sparsity = sparsity_pattern_from_tet_edges(tet_edge_idx);

    NedelecFullTensorGlobalSystem {
        k_re,
        k_im,
        m_re,
        m_im,
        sparsity,
    }
}

/// Sparse (`[nnz]` pattern-aligned) variant of
/// [`assemble_global_nedelec_with_full_tensors`] (matched UPML,
/// issues #199/#218). Same four weighted kernel invocations, signs and
/// values-side autodiff as the dense path — only the scatter target
/// changes. See [`assemble_global_nedelec_with_complex_epsilon_sparse`]
/// for the layout convention.
pub fn assemble_global_nedelec_with_full_tensors_sparse<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_sign: &[[i8; 6]],
    scatter: &NedelecScatterMap,
    epsilon_tensor: &[[[faer::c64; 3]; 3]],
    nu_tensor: &[[[faer::c64; 3]; 3]],
) -> NedelecSparseFullTensorSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(
        scatter.n_elem(),
        n_elem,
        "scatter map element count mismatch"
    );
    assert_eq!(
        epsilon_tensor.len(),
        n_elem,
        "epsilon_tensor length mismatch"
    );
    assert_eq!(nu_tensor.len(), n_elem, "nu_tensor length mismatch");

    // 1. Element-local weighted matrices: Re/Im pass per weight.
    let coords = gather_tet_coords(nodes, tets);
    let eps_re = upload_tensor33_component::<B>(epsilon_tensor, |c| c.re, &device);
    let eps_im = upload_tensor33_component::<B>(epsilon_tensor, |c| c.im, &device);
    let nu_re = upload_tensor33_component::<B>(nu_tensor, |c| c.re, &device);
    let nu_im = upload_tensor33_component::<B>(nu_tensor, |c| c.im, &device);

    let k_local_re = batched_nedelec_local_stiffness_weighted(coords.clone(), nu_re);
    let k_local_im = batched_nedelec_local_stiffness_weighted(coords.clone(), nu_im);
    let m_local_re = batched_nedelec_local_mass_anisotropic_full(coords.clone(), eps_re);
    let m_local_im = batched_nedelec_local_mass_anisotropic_full(coords, eps_im);

    // 2. Signs + 3. flat [nnz] scatters (shared slot indices).
    let sign_outer = sign_outer_tensor::<B>(tet_edge_sign, &device);
    let slot_indices = scatter.slot_tensor::<B>(&device);
    let nnz = scatter.nnz();
    let scatter_one = |local: Tensor<B, 3>| -> Tensor<B, 1> {
        scatter_to_pattern_vals(local.mul(sign_outer.clone()), &slot_indices, nnz, &device)
    };

    NedelecSparseFullTensorSystem {
        k_re_vals: scatter_one(k_local_re),
        k_im_vals: scatter_one(k_local_im),
        m_re_vals: scatter_one(m_local_re),
        m_im_vals: scatter_one(m_local_im),
    }
}

/// Assemble the global Nédélec right-hand-side vector for a
/// **piecewise-constant** volumetric current density `J`.
///
/// Computes `b_i = ∫_Ω N_i · J dV` over the global edge basis (no `iωμ₀`
/// prefactor — that is applied by the caller, see
/// [`crate::driven::driven_solve`]). Follows the same batched-local-
/// kernel + autodiff-preserving 1-D `scatter(0, …, Add)` pattern as
/// [`assemble_global_nedelec`]: the per-element `[n_elem, 6]` local RHS
/// from [`crate::elements::nedelec::batched_nedelec_local_rhs`] is multiplied by
/// the per-DOF orientation sign `s_i` (a single factor — the RHS is
/// linear in the basis, unlike the `s_i s_j` outer product of the
/// matrix path) and scatter-added into a `[n_edges]` zero tensor.
///
/// # Arguments
///
/// * `nodes`, `tets`, `tet_edge_idx`, `tet_edge_sign`, `n_edges` — same
///   as [`assemble_global_nedelec`].
/// * `j_tet` — `[n_elem][3]` per-tet constant current density (typically
///   `J(x)` sampled at tet centroids; see [`tet_centroids`]).
///
/// # Returns
///
/// Dense `[n_edges]` Burn tensor with the assembled linear functional.
pub fn assemble_nedelec_current_rhs<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    j_tet: &[[f64; 3]],
) -> Tensor<B, 1> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(j_tet.len(), n_elem, "j_tet length mismatch");

    // 1. Per-element local RHS.
    let coords = gather_tet_coords(nodes, tets);
    let j_flat: Vec<f32> = j_tet
        .iter()
        .flat_map(|row| row.iter().map(|&x| x as f32))
        .collect();
    let j_tensor = Tensor::<B, 2>::from_data(TensorData::new(j_flat, [n_elem, 3]), &device);
    let local = crate::elements::nedelec::batched_nedelec_local_rhs(coords, j_tensor); // [n_elem, 6]

    // 2. Apply per-DOF orientation signs (single factor for a vector).
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
    let b_signed = local.mul(sign_2d);

    // 3. Flat scatter indices: global edge index per (e, i).
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 6);
    for row in tet_edge_idx {
        for &edge in row.iter() {
            linear_idx.push(edge as i32);
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 6]), &device);

    // 4. Scatter-add into a [n_edges] zero tensor. Autodiff flows
    //    through the values via IndexingUpdateOp::Add.
    let b_flat = b_signed.reshape([n_elem * 6]);
    let zeros = Tensor::<B, 1>::zeros([n_edges], &device);
    zeros.scatter(0, flat_indices, b_flat, IndexingUpdateOp::Add)
}

/// Degree-2 (4-point) quadrature variant of
/// [`assemble_nedelec_current_rhs`] for a **spatially varying**
/// current density sampled at the per-tet quadrature points
/// (issue #199): `b_i = Σ_T Σ_q (V/4) N_i(x_q) · J(x_q)`.
///
/// Same batched-local-kernel
/// ([`crate::elements::nedelec::batched_nedelec_local_rhs_quad4`]) + sign +
/// autodiff-preserving 1-D `scatter(0, …, Add)` structure as the
/// constant-`J` assembler; reduces to it exactly when the four samples
/// of a tet are equal. The samples are uploaded at the backend's full
/// float precision (`B::FloatElem`).
///
/// * `j_quad` — `[n_elem][4][3]` per-tet current density at the four
///   degree-2 quadrature points (see
///   [`crate::elements::nedelec::TET_QUAD4_A`] for the point convention; use
///   [`crate::driven::QuadCurrentSource`] to sample a continuous
///   `J(x)`).
pub fn assemble_nedelec_current_rhs_quad4<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    tet_edge_idx: &[[u32; 6]],
    tet_edge_sign: &[[i8; 6]],
    n_edges: usize,
    j_quad: &[[[f64; 3]; 4]],
) -> Tensor<B, 1> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();
    assert_eq!(tet_edge_idx.len(), n_elem, "tet_edge_idx length mismatch");
    assert_eq!(tet_edge_sign.len(), n_elem, "tet_edge_sign length mismatch");
    assert_eq!(j_quad.len(), n_elem, "j_quad length mismatch");

    // 1. Per-element local RHS from the quadrature samples.
    let coords = gather_tet_coords(nodes, tets);
    let j_flat: Vec<B::FloatElem> = j_quad
        .iter()
        .flat_map(|t| t.iter().flat_map(|q| q.iter().map(|&x| x.elem())))
        .collect();
    let j_tensor = Tensor::<B, 3>::from_data(TensorData::new(j_flat, [n_elem, 4, 3]), &device);
    let local = crate::elements::nedelec::batched_nedelec_local_rhs_quad4(coords, j_tensor); // [n_elem, 6]

    // 2. Apply per-DOF orientation signs (single factor for a vector).
    let sign_flat: Vec<f32> = tet_edge_sign
        .iter()
        .flat_map(|row| row.iter().map(|&s| s as f32))
        .collect();
    let sign_2d = Tensor::<B, 2>::from_data(TensorData::new(sign_flat, [n_elem, 6]), &device);
    let b_signed = local.mul(sign_2d);

    // 3. Flat scatter indices: global edge index per (e, i).
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 6);
    for row in tet_edge_idx {
        for &edge in row.iter() {
            linear_idx.push(edge as i32);
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 6]), &device);

    // 4. Scatter-add into a [n_edges] zero tensor.
    let b_flat = b_signed.reshape([n_elem * 6]);
    let zeros = Tensor::<B, 1>::zeros([n_edges], &device);
    zeros.scatter(0, flat_indices, b_flat, IndexingUpdateOp::Add)
}

/// Combine the real and imaginary parts of a Burn-resident complex
/// mass matrix into an owned `faer::Mat<faer::c64>`.
///
/// Mirrors [`crate::eigen::burn_matrix_to_faer`] for complex inputs.
/// Pulls both halves off the device once and zips them into the
/// complex output. `TensorData::iter::<f64>` reads the values as f64
/// regardless of the backend's stored float dtype (f32 on the wgpu/cuda
/// GPU backends, f64 on the ndarray CPU backend), so this is genuinely
/// backend-agnostic — the f32 GPU path upcasts and the f64 CPU path is
/// read losslessly.
pub fn burn_complex_mass_to_faer<B: Backend>(
    m_re: Tensor<B, 2>,
    m_im: Tensor<B, 2>,
) -> faer::Mat<faer::c64> {
    let dims_re = m_re.dims();
    let dims_im = m_im.dims();
    assert_eq!(dims_re, dims_im, "M_re and M_im must have matching dims");
    let data_re: Vec<f64> = m_re.into_data().iter::<f64>().collect();
    let data_im: Vec<f64> = m_im.into_data().iter::<f64>().collect();
    let n = dims_re[0];
    faer::Mat::<faer::c64>::from_fn(n, dims_re[1], |i, j| {
        let idx = i * dims_re[1] + j;
        faer::c64::new(data_re[idx], data_im[idx])
    })
}
