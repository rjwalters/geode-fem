//! Global assembly of P1 element-local stiffness and mass matrices.
//!
//! Builds the global `K` and `M` matrices by scattering per-element
//! `[n_elem, 4, 4]` local tensors (from [`crate::elements::p1::batched_p1_local_matrices`])
//! into a flat `[n_dof * n_dof]` Burn tensor using 1-D
//! [`Tensor::scatter`](burn::tensor::Tensor::scatter) with
//! `IndexingUpdateOp::Add`, then reshaping to `[n_dof, n_dof]`.
//!
//! Dense storage is acceptable for v0 (the cube warmup is at most a few
//! thousand DOFs). A side-output [`SparsityPattern`] records the unique
//! `(row, col)` pairs touched during assembly, so a CSR projection can
//! slot in later without breaking callers.
//!
//! # Why 1-D `scatter` + Add
//!
//! Multiple tets share nodes, so many `(row, col)` global indices appear
//! more than once during assembly. `IndexingUpdateOp::Add` accumulates
//! duplicates correctly in the forward pass and through `Autodiff`
//! gradients — preserving differentiability end-to-end is the whole
//! point of using Burn instead of a CPU-only sparse linear-algebra crate.
//!
//! We use the 1-D `scatter(dim=0, …)` form (with linearized
//! `row * n_dof + col` indices) rather than the 2-D `scatter_nd` form,
//! since 1-D scatter has stable duplicate-accumulation semantics across
//! Burn's wgpu backend on this version. The end-to-end behavior is
//! identical for our purpose, and a future migration to `scatter_nd` is
//! a drop-in once that path is rock-solid.

use std::collections::BTreeSet;

use burn::tensor::ElementConversion;
use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
use burn::tensor::{IndexingUpdateOp, Int, TensorData};

use crate::elements::p1::batched_p1_local_matrices;
use crate::mesh::TetMesh;

/// Assembled global linear system in dense Burn-tensor form.
#[derive(Debug, Clone)]
pub struct GlobalSystem<B: Backend> {
    /// Global stiffness matrix `[n_dof, n_dof]`.
    pub k: Tensor<B, 2>,
    /// Global consistent mass matrix `[n_dof, n_dof]`.
    pub m: Tensor<B, 2>,
    /// `(row, col)` index pairs touched during assembly. Useful for a
    /// later CSR projection without re-walking the connectivity.
    pub sparsity: SparsityPattern,
}

/// CPU-side sparsity pattern: every `(row, col)` pair touched at least
/// once by the assembly. Always symmetric for P1 stiffness/mass.
#[derive(Debug, Clone, Default)]
pub struct SparsityPattern {
    pub rows: Vec<u32>,
    pub cols: Vec<u32>,
}

impl SparsityPattern {
    /// Returns the number of unique non-zero entries.
    pub fn nnz(&self) -> usize {
        self.rows.len()
    }
}

/// Push a `TetMesh` onto the given device as `(nodes, tets)` Burn tensors.
///
/// Returns `nodes: [n_nodes, 3]` as a float tensor at the active backend's
/// `B::FloatElem` precision (f64 on `ndarray`, f32 on `wgpu`/`cuda`) and
/// `tets: [n_elem, 4]` as an Int tensor.
///
/// # Precision
///
/// `TetMesh::nodes` holds coordinates as `f64`. Each value is converted
/// to `B::FloatElem` via [`ElementConversion::elem`] so that the f64
/// path under the `ndarray` backend actually delivers f64 K/M assembly
/// downstream. Earlier versions of this function force-cast to `f32`
/// regardless of `B::FloatElem`, which silently truncated precision on
/// the nominally-f64 CPU backend; that bug was the original surface of
/// issue #99 (discovered cross-backend in PR #98).
///
/// # Panics
///
/// Panics if any node index exceeds [`i32::MAX`] (~2.1 billion). Burn's
/// Int tensors are i32-backed, so very large meshes would otherwise wrap
/// silently. A non-panicking alternative will land alongside the sparse
/// eigensolver work where mesh sizes start to matter.
pub fn upload_mesh<B: Backend>(
    mesh: &TetMesh,
    device: &B::Device,
) -> (Tensor<B, 2>, Tensor<B, 2, Int>) {
    let n_nodes = mesh.n_nodes();
    let n_elem = mesh.n_tets();

    // Convert each coordinate to the backend's float element type so that
    // the f64-on-ndarray path carries full double precision into K/M.
    // `B::FloatElem` is `f32` on `wgpu`/`cuda` and `f64` on `ndarray` —
    // `ElementConversion::elem` is the canonical Burn-side cast across
    // both. See `burn-backend/src/backend/ops/modules/unfold.rs` for the
    // same `vec![..; n].elem()` idiom in upstream code.
    let node_data: Vec<B::FloatElem> = mesh
        .nodes
        .iter()
        .flat_map(|n| n.iter().map(|&x| x.elem::<B::FloatElem>()))
        .collect();
    let nodes = Tensor::<B, 2>::from_data(TensorData::new(node_data, [n_nodes, 3]), device);

    let tet_data: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| {
            t.iter().map(|&i| {
                i32::try_from(i).expect("node index does not fit in i32 (Burn Int tensor limit)")
            })
        })
        .collect();
    let tets = Tensor::<B, 2, Int>::from_data(TensorData::new(tet_data, [n_elem, 4]), device);

    (nodes, tets)
}

/// Gather per-tet vertex coordinates into a `[n_elem, 4, 3]` tensor by
/// looking up each tet's four node indices in the global `nodes` tensor.
///
/// This is the natural input shape for [`batched_p1_local_matrices`].
pub fn gather_tet_coords<B: Backend>(nodes: Tensor<B, 2>, tets: Tensor<B, 2, Int>) -> Tensor<B, 3> {
    let [n_elem, _] = tets.dims();
    // Flatten the [n_elem, 4] index tensor to [n_elem * 4] and use
    // `select(dim=0, ...)` to pull rows from `nodes`. The result is
    // [n_elem * 4, 3]; reshape to [n_elem, 4, 3].
    let flat_idx = tets.reshape([n_elem * 4]);
    let gathered = nodes.select(0, flat_idx);
    gathered.reshape([n_elem, 4, 3])
}

/// Assemble dense global stiffness and mass matrices from per-element
/// local matrices, preserving autodiff through 1-D `scatter(0, …, Add)`.
///
/// # Arguments
///
/// * `nodes` — `[n_nodes, 3]` global node coordinates.
/// * `tets`  — `[n_elem, 4]` connectivity (0-based linear node indices).
/// * `n_dof` — size of the global linear system. Usually equal to
///   `nodes.dims()[0]`, but exposed explicitly to allow over-allocation
///   in tests.
///
/// # Returns
///
/// A [`GlobalSystem`] with dense `K` and `M` plus the
/// [`SparsityPattern`] of unique non-zero entries.
pub fn assemble_global_p1<B: Backend>(
    nodes: Tensor<B, 2>,
    tets: Tensor<B, 2, Int>,
    n_dof: usize,
) -> GlobalSystem<B> {
    let device = nodes.device();
    let [n_elem, _] = tets.dims();

    // 1. Pull the connectivity host-side once. We use it both to build
    //    the sparsity pattern and to construct the scatter indices. The
    //    indices themselves are pure integers — autodiff doesn't flow
    //    through them — so building them on CPU is cheap and avoids the
    //    GPU `unsqueeze`/`expand`/`stack` ambiguities that surface on
    //    some backends for index-typed tensors.
    let tets_host = tets_to_cpu(&tets);

    // 2. Gather per-tet vertex coordinates and compute element-local
    //    stiffness/mass tensors (issue #10's surface).
    let coords = gather_tet_coords(nodes, tets);
    let p1 = batched_p1_local_matrices(coords);

    // 3. Build flat linear indices: `tets[e, i] * n_dof + tets[e, j]` for
    //    every (e, i, j). We assemble in a flattened [n_dof * n_dof] space
    //    via 1D `scatter(dim=0, …, Add)` — the simplest `scatter` form,
    //    and the one with the best-tested duplicate-index accumulation
    //    semantics across Burn backends. After scattering we reshape to
    //    the 2D global matrix.
    let mut linear_idx: Vec<i32> = Vec::with_capacity(n_elem * 16);
    let n_dof_i32 = n_dof as i32;
    for tet in &tets_host {
        for i in 0..4 {
            for j in 0..4 {
                linear_idx.push(tet[i] as i32 * n_dof_i32 + tet[j] as i32);
            }
        }
    }
    let flat_indices =
        Tensor::<B, 1, Int>::from_data(TensorData::new(linear_idx, [n_elem * 16]), &device);

    // 4. Flatten the local-matrix values to [n_elem * 16] and scatter-add
    //    into a flat [n_dof * n_dof] zero tensor. Autodiff flows through
    //    the values via `IndexingUpdateOp::Add`.
    let k_flat = p1.k_local.reshape([n_elem * 16]);
    let m_flat = p1.m_local.reshape([n_elem * 16]);

    let zeros_flat = Tensor::<B, 1>::zeros([n_dof * n_dof], &device);

    let k_flat_assembled =
        zeros_flat
            .clone()
            .scatter(0, flat_indices.clone(), k_flat, IndexingUpdateOp::Add);
    let m_flat_assembled = zeros_flat.scatter(0, flat_indices, m_flat, IndexingUpdateOp::Add);

    let k = k_flat_assembled.reshape([n_dof, n_dof]);
    let m = m_flat_assembled.reshape([n_dof, n_dof]);

    // 5. Sparsity pattern from the same host-side connectivity.
    let sparsity = sparsity_pattern_from_tets(&tets_host);

    GlobalSystem { k, m, sparsity }
}

/// Pull the `[n_elem, 4]` int connectivity off the device into a host Vec
/// of `[u32; 4]` rows. Used both for index construction and for the
/// sparsity pattern.
fn tets_to_cpu<B: Backend>(tets: &Tensor<B, 2, Int>) -> Vec<[u32; 4]> {
    let [n_elem, _] = tets.dims();
    let raw: Vec<i32> = tets.clone().into_data().to_vec().expect("readback i32");
    (0..n_elem)
        .map(|e| {
            [
                raw[e * 4] as u32,
                raw[e * 4 + 1] as u32,
                raw[e * 4 + 2] as u32,
                raw[e * 4 + 3] as u32,
            ]
        })
        .collect()
}

/// Construct the sparsity pattern by walking the host-side connectivity:
/// every `(tets[e, i], tets[e, j])` pair contributes one entry, with
/// duplicates collapsed.
fn sparsity_pattern_from_tets(tets: &[[u32; 4]]) -> SparsityPattern {
    let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
    for tet in tets {
        for i in 0..4 {
            for j in 0..4 {
                set.insert((tet[i], tet[j]));
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
