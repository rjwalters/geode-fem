//! Sparse `[nnz]` pattern-aligned Nédélec assembly vs the dense
//! `[n_edges²]` path (issue #218).
//!
//! The sparse assemblers run the *same* element-local kernels, signs
//! and 1-D `scatter(0, …, Add)` as the dense ones — only the scatter
//! target changes from a flat `[n_edges * n_edges]` tensor (i32
//! linear-index overflow at n_edges > 46_340, O(n²) memory) to a flat
//! `[nnz]` tensor indexed by precomputed pattern slots. Per pattern
//! entry the summands accumulate in the same `(e, i, j)` order, so on
//! the deterministic CPU backend the two paths must agree **bitwise**.
//!
//! Coverage:
//!
//! 1. dense-vs-sparse value equality over the recorded pattern for all
//!    three driven-path material assemblers (scalar complex ε,
//!    diagonal-anisotropic ε, matched-UPML full tensors) plus the
//!    σ damping matrix;
//! 2. pattern-slot map sanity (`slot_of` hits every recorded entry,
//!    misses off-pattern pairs);
//! 3. autodiff smoke through the `[nnz]` scatter (gradients w.r.t.
//!    node coordinates exist, finite, non-zero — mirrors the dense-path
//!    test at `tests/sigma_conductivity.rs`).

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Int, Tensor, TensorData};
use faer::c64;
use geode_core::{
    assemble_global_nedelec_with_anisotropic_epsilon,
    assemble_global_nedelec_with_anisotropic_epsilon_sparse,
    assemble_global_nedelec_with_complex_epsilon,
    assemble_global_nedelec_with_complex_epsilon_sparse, assemble_global_nedelec_with_full_tensors,
    assemble_global_nedelec_with_full_tensors_sparse, assemble_nedelec_sigma_damping,
    assemble_nedelec_sigma_damping_sparse, cube_tet_mesh, upload_mesh, DefaultBackend,
    NedelecScatterMap, SparsityPattern, TetMesh,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// Read a dense `[n, n]` Burn matrix to a host row-major Vec<f64>.
fn dense_to_host(t: Tensor<B, 2>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Read a `[nnz]` Burn value tensor to a host Vec<f64>.
fn vals_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Assert that the sparse `[nnz]` values equal the dense matrix read
/// over the same (sorted) pattern, bit-for-bit.
fn assert_pattern_equal(
    label: &str,
    pattern: &SparsityPattern,
    n_edges: usize,
    dense: &[f64],
    sparse: &[f64],
) {
    assert_eq!(sparse.len(), pattern.nnz(), "{label}: value length");
    for (idx, (&r, &c)) in pattern.rows.iter().zip(pattern.cols.iter()).enumerate() {
        let d = dense[r as usize * n_edges + c as usize];
        let s = sparse[idx];
        assert_eq!(
            d.to_bits(),
            s.to_bits(),
            "{label}: entry ({r}, {c}) dense {d:e} != sparse {s:e}"
        );
    }
}

/// Spatially varying complex scalar ε (lossy dielectric ramp).
fn varying_eps(mesh: &TetMesh) -> Vec<c64> {
    (0..mesh.n_tets())
        .map(|t| c64::new(1.0 + 0.07 * t as f64, -0.01 * (t % 5) as f64))
        .collect()
}

#[test]
fn sparse_complex_epsilon_matches_dense_bitwise() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps = varying_eps(&mesh);
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let dense = assemble_global_nedelec_with_complex_epsilon::<B>(
        nodes.clone(),
        tets.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps,
    );

    let scatter = NedelecScatterMap::new(&tet_idx);
    let sparse = assemble_global_nedelec_with_complex_epsilon_sparse::<B>(
        nodes, tets, &tet_sign, &scatter, &eps,
    );

    let pattern = scatter.pattern();
    assert_eq!(pattern.nnz(), dense.sparsity.nnz(), "pattern nnz mismatch");
    assert_pattern_equal(
        "K",
        pattern,
        n_edges,
        &dense_to_host(dense.k),
        &vals_to_host(sparse.k_vals),
    );
    assert_pattern_equal(
        "M_re",
        pattern,
        n_edges,
        &dense_to_host(dense.m_re),
        &vals_to_host(sparse.m_re_vals),
    );
    assert_pattern_equal(
        "M_im",
        pattern,
        n_edges,
        &dense_to_host(dense.m_im),
        &vals_to_host(sparse.m_im_vals),
    );
}

#[test]
fn sparse_anisotropic_epsilon_matches_dense_bitwise() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps_diag: Vec<[c64; 3]> = (0..mesh.n_tets())
        .map(|t| {
            [
                c64::new(1.0 + 0.05 * t as f64, -0.02),
                c64::new(1.3, -0.001 * t as f64),
                c64::new(0.9 + 0.01 * t as f64, 0.0),
            ]
        })
        .collect();
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let dense = assemble_global_nedelec_with_anisotropic_epsilon::<B>(
        nodes.clone(),
        tets.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_diag,
    );

    let scatter = NedelecScatterMap::new(&tet_idx);
    let sparse = assemble_global_nedelec_with_anisotropic_epsilon_sparse::<B>(
        nodes, tets, &tet_sign, &scatter, &eps_diag,
    );

    let pattern = scatter.pattern();
    assert_pattern_equal(
        "K",
        pattern,
        n_edges,
        &dense_to_host(dense.k),
        &vals_to_host(sparse.k_vals),
    );
    assert_pattern_equal(
        "M_re",
        pattern,
        n_edges,
        &dense_to_host(dense.m_re),
        &vals_to_host(sparse.m_re_vals),
    );
    assert_pattern_equal(
        "M_im",
        pattern,
        n_edges,
        &dense_to_host(dense.m_im),
        &vals_to_host(sparse.m_im_vals),
    );
}

#[test]
fn sparse_full_tensors_matches_dense_bitwise() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    // Symmetric complex 3×3 tensors with off-diagonal coupling.
    let eps_t: Vec<[[c64; 3]; 3]> = (0..mesh.n_tets())
        .map(|t| {
            let a = 1.0 + 0.03 * t as f64;
            [
                [c64::new(a, -0.02), c64::new(0.1, 0.01), c64::new(0.0, 0.0)],
                [
                    c64::new(0.1, 0.01),
                    c64::new(1.2, -0.05),
                    c64::new(0.05, 0.0),
                ],
                [
                    c64::new(0.0, 0.0),
                    c64::new(0.05, 0.0),
                    c64::new(0.8, -0.01),
                ],
            ]
        })
        .collect();
    let nu_t: Vec<[[c64; 3]; 3]> = (0..mesh.n_tets())
        .map(|t| {
            let a = 0.9 + 0.02 * t as f64;
            [
                [c64::new(a, 0.04), c64::new(0.0, 0.0), c64::new(-0.07, 0.02)],
                [c64::new(0.0, 0.0), c64::new(1.1, 0.01), c64::new(0.0, 0.0)],
                [
                    c64::new(-0.07, 0.02),
                    c64::new(0.0, 0.0),
                    c64::new(1.0, 0.03),
                ],
            ]
        })
        .collect();
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let dense = assemble_global_nedelec_with_full_tensors::<B>(
        nodes.clone(),
        tets.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_t,
        &nu_t,
    );

    let scatter = NedelecScatterMap::new(&tet_idx);
    let sparse = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes, tets, &tet_sign, &scatter, &eps_t, &nu_t,
    );

    let pattern = scatter.pattern();
    for (label, d, s) in [
        ("K_re", dense.k_re, sparse.k_re_vals),
        ("K_im", dense.k_im, sparse.k_im_vals),
        ("M_re", dense.m_re, sparse.m_re_vals),
        ("M_im", dense.m_im, sparse.m_im_vals),
    ] {
        assert_pattern_equal(label, pattern, n_edges, &dense_to_host(d), &vals_to_host(s));
    }
}

#[test]
fn sparse_sigma_damping_matches_dense_bitwise() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let sigma: Vec<f64> = (0..mesh.n_tets()).map(|t| 0.5 + 0.11 * t as f64).collect();
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let dense = assemble_nedelec_sigma_damping::<B>(
        nodes.clone(),
        tets.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &sigma,
    );

    let scatter = NedelecScatterMap::new(&tet_idx);
    let sparse =
        assemble_nedelec_sigma_damping_sparse::<B>(nodes, tets, &tet_sign, &scatter, &sigma);

    assert_pattern_equal(
        "C",
        scatter.pattern(),
        n_edges,
        &dense_to_host(dense),
        &vals_to_host(sparse),
    );
}

/// `slot_of` must locate every recorded `(row, col)` pair at its own
/// slot index and return `None` for pairs outside the pattern.
#[test]
fn scatter_map_slot_lookup_is_consistent() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (tet_idx, _) = edge_tables(&mesh);
    let scatter = NedelecScatterMap::new(&tet_idx);
    let pattern = scatter.pattern();

    for (idx, (&r, &c)) in pattern.rows.iter().zip(pattern.cols.iter()).enumerate() {
        assert_eq!(scatter.slot_of(r, c), Some(idx));
    }

    // The cube mesh is far from fully coupled: edge 0 cannot couple to
    // every other edge. Find one absent pair and check it misses.
    let n_edges = mesh.edges().len() as u32;
    let coupled_to_zero: std::collections::BTreeSet<u32> = pattern
        .rows
        .iter()
        .zip(pattern.cols.iter())
        .filter(|(&r, _)| r == 0)
        .map(|(_, &c)| c)
        .collect();
    let absent = (0..n_edges)
        .find(|c| !coupled_to_zero.contains(c))
        .expect("edge 0 must not couple to all edges on the 2-cube mesh");
    assert_eq!(scatter.slot_of(0, absent), None);
}

/// Autodiff smoke through the `[nnz]` scatter (mirrors the dense-path
/// test `sigma_damping_assembly_preserves_autodiff` in
/// `tests/sigma_conductivity.rs`): gradients w.r.t. node coordinates
/// must exist, be finite, and be non-zero somewhere.
#[test]
fn sparse_assembly_preserves_autodiff() {
    use burn::backend::Autodiff;
    type Ad = Autodiff<B>;

    let mesh = cube_tet_mesh(2, 1.0);
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let sigma: Vec<f64> = (0..mesh.n_tets()).map(|t| 0.5 + 0.11 * t as f64).collect();
    let scatter = NedelecScatterMap::new(&tet_idx);

    let n = mesh.n_nodes();
    let n_elem = mesh.n_tets();
    let ad_dev = <Ad as BackendTypes>::Device::default();
    let node_flat: Vec<f32> = mesh
        .nodes
        .iter()
        .flat_map(|p| p.iter().map(|&x| x as f32))
        .collect();
    let tet_flat: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| t.iter().map(|&i| i as i32))
        .collect();
    let nodes =
        Tensor::<Ad, 2>::from_data(TensorData::new(node_flat, [n, 3]), &ad_dev).require_grad();
    let tets = Tensor::<Ad, 2, Int>::from_data(TensorData::new(tet_flat, [n_elem, 4]), &ad_dev);

    let c_vals = assemble_nedelec_sigma_damping_sparse::<Ad>(
        nodes.clone(),
        tets,
        &tet_sign,
        &scatter,
        &sigma,
    );
    assert_eq!(c_vals.dims(), [scatter.nnz()]);

    let loss = c_vals.powf_scalar(2.0).sum();
    let grads = loss.backward();
    let dnodes = nodes
        .grad(&grads)
        .expect("gradient w.r.t. nodes should exist");
    let dnodes_vec: Vec<f64> = dnodes.into_data().iter::<f64>().collect();
    assert!(
        dnodes_vec.iter().all(|g| g.is_finite()),
        "all gradients must be finite"
    );
    assert!(
        dnodes_vec.iter().any(|g| g.abs() > 1e-6),
        "gradient should be non-zero somewhere"
    );
}
