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
//! See `crate::nedelec` for the math and orientation convention.

use std::collections::BTreeSet;

use burn::tensor::backend::Backend;
use burn::tensor::Tensor;
use burn::tensor::{IndexingUpdateOp, Int, TensorData};

use crate::assembly::{gather_tet_coords, SparsityPattern};
use crate::nedelec::batched_nedelec_local_matrices;
use crate::TetMesh;

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

fn sparsity_pattern_from_tet_edges(tet_edge_idx: &[[u32; 6]]) -> SparsityPattern {
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

/// Build a per-tet relative permittivity vector for the bundled
/// sphere-in-vacuum fixture, parameterized by the dielectric refractive
/// index `n_inside`.
///
/// Tets with physical tag [`crate::mesh::PHYS_SPHERE_INTERIOR`] receive
/// `epsilon_r = n_inside.powi(2)`; tets with
/// [`crate::mesh::PHYS_VACUUM_BUFFER`] receive `epsilon_r = 1.0`. Any
/// other (unexpected) tag also defaults to `1.0` — the fixture only
/// emits the two interior/buffer tags so this is a defensive default.
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
/// realizes a scalar-isotropic PML in the vacuum buffer region of the
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
/// - Tet centroid `r_c ≤ R_SPHERE`: `ε = n_inside² + 0j` (real dielectric).
/// - Tet centroid `r_c > R_SPHERE`: smooth quadratic absorption ramp
///   anchored at the dielectric interface,
///
///   ```text
///   ε(r) = 1 − j σ₀ ((r_c − R_SPHERE) / (R_BUFFER − R_SPHERE))²
///   ```
///
///   The `r → R_SPHERE` limit returns `ε = 1` (matches the vacuum) and
///   the `r → R_BUFFER` limit gives the full absorption `ε = 1 − jσ₀`.
///   The quadratic profile is the standard low-reflection start point
///   for discrete PMLs.
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
    use crate::mesh::{R_BUFFER, R_SPHERE};
    assert_eq!(
        physical_tags.len(),
        centroid_radii.len(),
        "physical_tags and centroid_radii length mismatch"
    );
    let eps_inside = n_inside * n_inside;
    let width = R_BUFFER - R_SPHERE;

    physical_tags
        .iter()
        .zip(centroid_radii.iter())
        .map(|(&tag, &r_c)| {
            if tag == crate::mesh::PHYS_SPHERE_INTERIOR {
                faer::c64::new(eps_inside, 0.0)
            } else {
                // Buffer region — apply quadratic PML ramp anchored at
                // r = R_SPHERE. Clamp to the [0, 1] normalized range so
                // any tet whose centroid drifts slightly outside R_BUFFER
                // (mesh nodes on the outer wall + interior tet) does not
                // overshoot.
                let u = ((r_c - R_SPHERE) / width).clamp(0.0, 1.0);
                let im = -sigma_0 * u * u;
                faer::c64::new(1.0, im)
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

/// Combine the real and imaginary parts of a Burn-resident complex
/// mass matrix into an owned `faer::Mat<faer::c64>`.
///
/// Mirrors [`crate::eigen::burn_matrix_to_faer`] for complex inputs.
/// Pulls both halves off the device once and zips them into the
/// complex output.
pub fn burn_complex_mass_to_faer<B: Backend>(
    m_re: Tensor<B, 2>,
    m_im: Tensor<B, 2>,
) -> faer::Mat<faer::c64> {
    let dims_re = m_re.dims();
    let dims_im = m_im.dims();
    assert_eq!(dims_re, dims_im, "M_re and M_im must have matching dims");
    let data_re: Vec<f32> = m_re.into_data().to_vec().expect("readback Re");
    let data_im: Vec<f32> = m_im.into_data().to_vec().expect("readback Im");
    let n = dims_re[0];
    faer::Mat::<faer::c64>::from_fn(n, dims_re[1], |i, j| {
        let idx = i * dims_re[1] + j;
        faer::c64::new(data_re[idx] as f64, data_im[idx] as f64)
    })
}
