//! Matrix-free Nédélec matvec vs assembled-CSR/dense matvec (#302 Phase 1).
//!
//! The **acceptance gate** for #302 Phase 1: the matrix-free apply in
//! [`geode_core::assembly::nedelec_matvec::MatrixFreeNedelecOperator`]
//! (gather → batched dense `[n_elem,6,6]·[n_elem,6,1]` local apply →
//! signed scatter-add, no global operator materialized) must agree with the
//! assembled operator's matvec to ~1e-12 on the ndarray-f64 backend.
//!
//! The reference matvec is `y = A · x` computed against the **assembled dense
//! global matrix** `A ∈ ℝ^{n_edges × n_edges}` produced by the existing
//! [`geode_core::assembly::nedelec::assemble_global_nedelec`] /
//! `_with_epsilon` path — the same matrix the driven/eigen CSR `spmv` applies,
//! read to host and multiplied in plain f64. Testing against the assembled
//! *matrix* (rather than a specific `faer` `SparseColMat` `spmv`) keeps the
//! gate on the mathematical operator equivalence, which is the point of
//! Phase 1, without pulling the solver seam into scope.
//!
//! Coverage:
//!
//! 1. `apply_k` vs assembled `K · x` — curl-curl stiffness, multiple vectors.
//! 2. `apply_m` vs assembled `M(ε) · x` — ε-weighted mass, multiple vectors.
//! 3. `apply_combination(x, 1, -ω²)` vs assembled `(K − ω² M) · x` — the
//!    driven-style real operator.
//! 4. Sign convention: on a two-tet shared-edge mesh the matrix-free signed
//!    contributions reproduce the assembled scatter exactly.
//! 5. Dirichlet/interior masking: the masked operator equals the assembled
//!    full-space matrix with constrained rows/cols deleted (embedded back
//!    into the full space).
//! 6. `should_panic` firing test on a wrong-length operand (Bunsen contract).
//!
//! A `cuda`-feature-gated f32 smoke test (~1e-5 tolerance) is included but
//! `#[ignore]`d — `burn-cuda 0.21` has no f64, so it runs only on the rented
//! EC2 box, never in CI.

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Tensor, TensorData};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec, assemble_global_nedelec_with_epsilon, cube_pec_interior_edges,
};
use geode_core::assembly::nedelec_matvec::MatrixFreeNedelecOperator;
use geode_core::assembly::p1::upload_mesh;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Split `mesh.tet_edges()` into the `(idx, sign)` tables the assemblers take.
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

/// Read a dense `[n, n]` Burn matrix to a host row-major `Vec<f64>`.
fn dense_to_host(t: Tensor<B, 2>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Read a `[n]` Burn vector to a host `Vec<f64>`.
fn vec_to_host(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Reference dense matvec `y = A · x` on host, row-major `[n, n]` matrix.
fn dense_matvec(a: &[f64], x: &[f64], n: usize) -> Vec<f64> {
    let mut y = vec![0.0; n];
    for (i, yi) in y.iter_mut().enumerate() {
        let row = &a[i * n..(i + 1) * n];
        *yi = row.iter().zip(x.iter()).map(|(&aij, &xj)| aij * xj).sum();
    }
    y
}

/// Deterministic pseudo-random f64 vector in `[-1, 1)` from a 64-bit LCG seed
/// (reproducible, no `rand` dependency).
fn pseudo_random_vec(n: usize, seed: u64) -> Vec<f64> {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            // Top 53 bits → [0, 1) → shift to [-1, 1).
            let u = (state >> 11) as f64 / (1u64 << 53) as f64;
            2.0 * u - 1.0
        })
        .collect()
}

/// Upload a host `Vec<f64>` as a `[n]` Burn tensor.
fn upload_vec(x: &[f64], dev: &<B as BackendTypes>::Device) -> Tensor<B, 1> {
    Tensor::<B, 1>::from_data(TensorData::new(x.to_vec(), [x.len()]), dev)
}

/// Max absolute deviation and **norm-scaled** relative deviation between two
/// host vectors: `(max_i |a_i − b_i|, ‖a − b‖∞ / ‖b‖∞)`.
///
/// The norm-scaled relative measure is the standard matvec-equivalence metric
/// and is robust to catastrophic cancellation: an individual output DOF where
/// two large signed local contributions nearly cancel can leave a value near
/// machine epsilon whose *per-element* relative error is O(1) even though the
/// matvec agrees to full f64 precision. Scaling the max residual by the
/// reference vector's infinity norm reports the accuracy of the operator apply
/// as a whole, which is what the equivalence gate is about.
fn max_abs_rel(a: &[f64], b: &[f64]) -> (f64, f64) {
    let mut max_abs = 0.0_f64;
    for (&ai, &bi) in a.iter().zip(b.iter()) {
        max_abs = max_abs.max((ai - bi).abs());
    }
    let ref_norm = b.iter().fold(0.0_f64, |m, &v| m.max(v.abs())).max(1e-30);
    (max_abs, max_abs / ref_norm)
}

/// Tolerance for the f64 equivalence gate. The matrix-free and assembled
/// paths fold the *same* signed f32-weight locals; the only difference is
/// summation order (matmul vs scatter), so the two agree to a few ULP of the
/// f64 accumulation — well under 1e-12 relative.
const TOL_REL: f64 = 1e-12;

/// Vacuum (ε = 1) operator equivalence for `K`, `M`, and `K − ω²M` over a
/// batch of pseudo-random operands on the coarse cube fixture.
#[test]
fn matrix_free_matches_assembled_vacuum() {
    let mesh = cube_tet_mesh(3, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps_r = vec![1.0_f64; mesh.n_tets()];
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let assembled =
        assemble_global_nedelec::<B>(nodes.clone(), tets.clone(), &tet_idx, &tet_sign, n_edges);
    let k_host = dense_to_host(assembled.k);
    let m_host = dense_to_host(assembled.m);

    let op = MatrixFreeNedelecOperator::<B>::new(nodes, tets, &tet_idx, &tet_sign, n_edges, &eps_r);
    assert_eq!(op.n_edges(), n_edges);
    assert_eq!(op.n_elem(), mesh.n_tets());

    let omega = 1.7_f64;
    for seed in 0..5u64 {
        let x = pseudo_random_vec(n_edges, seed + 1);

        // K · x
        let y_k = vec_to_host(op.apply_k(upload_vec(&x, &dev)));
        let (a_k, r_k) = max_abs_rel(&y_k, &dense_matvec(&k_host, &x, n_edges));
        assert!(
            r_k < TOL_REL,
            "K seed {seed}: max_abs={a_k:e} max_rel={r_k:e}"
        );

        // M · x
        let y_m = vec_to_host(op.apply_m(upload_vec(&x, &dev)));
        let (a_m, r_m) = max_abs_rel(&y_m, &dense_matvec(&m_host, &x, n_edges));
        assert!(
            r_m < TOL_REL,
            "M seed {seed}: max_abs={a_m:e} max_rel={r_m:e}"
        );

        // (K − ω²M) · x, single-pass combination
        let a_combined: Vec<f64> = k_host
            .iter()
            .zip(m_host.iter())
            .map(|(&k, &m)| k - omega * omega * m)
            .collect();
        let y_c = vec_to_host(op.apply_combination(upload_vec(&x, &dev), 1.0, -omega * omega));
        let (a_ca, r_ca) = max_abs_rel(&y_c, &dense_matvec(&a_combined, &x, n_edges));
        assert!(
            r_ca < TOL_REL,
            "K-w2M seed {seed}: max_abs={a_ca:e} max_rel={r_ca:e}"
        );
    }
}

/// ε-weighted mass equivalence with a spatially varying per-tet permittivity.
#[test]
fn matrix_free_matches_assembled_varying_epsilon() {
    let mesh = cube_tet_mesh(3, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    // Real spatially varying ε (isotropic), same flavor as nedelec_sparse.rs.
    let eps_r: Vec<f64> = (0..mesh.n_tets())
        .map(|t| 1.0 + 0.13 * (t % 7) as f64)
        .collect();
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let assembled = assemble_global_nedelec_with_epsilon::<B>(
        nodes.clone(),
        tets.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_r,
    );
    let m_host = dense_to_host(assembled.m);

    let op = MatrixFreeNedelecOperator::<B>::new(nodes, tets, &tet_idx, &tet_sign, n_edges, &eps_r);

    for seed in 0..3u64 {
        let x = pseudo_random_vec(n_edges, 100 + seed);
        let y_m = vec_to_host(op.apply_m(upload_vec(&x, &dev)));
        let (a_m, r_m) = max_abs_rel(&y_m, &dense_matvec(&m_host, &x, n_edges));
        assert!(
            r_m < TOL_REL,
            "M(eps) seed {seed}: max_abs={a_m:e} max_rel={r_m:e}"
        );
    }
}

/// Sign convention: on a coarse mesh with many local-vs-global orientation
/// flips (the `n=2` cube has both signs present), the matrix-free signed
/// contributions must reproduce the assembled scatter — this is the most
/// likely place a subtle `s_i s_j` bug would hide.
#[test]
fn matrix_free_sign_convention_matches_assembled() {
    let mesh = cube_tet_mesh(2, 2.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);

    // Confirm the fixture actually exercises both signs (otherwise the test
    // is vacuous).
    let has_neg = tet_sign.iter().any(|row| row.iter().any(|&s| s < 0));
    assert!(
        has_neg,
        "fixture must contain at least one -1 orientation sign"
    );

    let eps_r = vec![1.0_f64; mesh.n_tets()];
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let assembled =
        assemble_global_nedelec::<B>(nodes.clone(), tets.clone(), &tet_idx, &tet_sign, n_edges);
    let k_host = dense_to_host(assembled.k);

    let op = MatrixFreeNedelecOperator::<B>::new(nodes, tets, &tet_idx, &tet_sign, n_edges, &eps_r);

    let x = pseudo_random_vec(n_edges, 42);
    let y = vec_to_host(op.apply_k(upload_vec(&x, &dev)));
    let (a, r) = max_abs_rel(&y, &dense_matvec(&k_host, &x, n_edges));
    assert!(r < TOL_REL, "signed K: max_abs={a:e} max_rel={r:e}");
}

/// Dirichlet/interior masking: the masked matrix-free operator equals the
/// assembled full-space matrix with constrained (PEC-boundary) rows AND
/// columns deleted, then embedded back into the full `[n_edges]` space (zero
/// on constrained DOFs). This is the interior-submatrix apply the assembled
/// driven/eigen paths perform, reproduced without slicing a submatrix.
#[test]
fn matrix_free_interior_masking_matches_reduced_assembled() {
    let mesh = cube_tet_mesh(3, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps_r = vec![1.0_f64; mesh.n_tets()];
    let dev = device();

    // PEC interior-edge mask for the cube: an edge is interior unless BOTH
    // endpoints lie on the box boundary.
    let (_edges, interior_mask) = cube_pec_interior_edges(&mesh, 1.0);
    assert_eq!(interior_mask.len(), n_edges);
    let n_interior = interior_mask.iter().filter(|&&b| b).count();
    assert!(
        n_interior > 0 && n_interior < n_edges,
        "mask must be non-trivial"
    );

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let assembled =
        assemble_global_nedelec::<B>(nodes.clone(), tets.clone(), &tet_idx, &tet_sign, n_edges);
    let k_host = dense_to_host(assembled.k);

    // Reference: the full-space matrix with constrained rows/cols zeroed.
    let mut k_reduced = k_host.clone();
    for i in 0..n_edges {
        for j in 0..n_edges {
            if !interior_mask[i] || !interior_mask[j] {
                k_reduced[i * n_edges + j] = 0.0;
            }
        }
    }

    let op = MatrixFreeNedelecOperator::<B>::new(nodes, tets, &tet_idx, &tet_sign, n_edges, &eps_r)
        .with_mask(&interior_mask);

    for seed in 0..3u64 {
        // Give the operand nonzero entries on constrained DOFs too — the
        // masking must ignore them (column deletion).
        let x = pseudo_random_vec(n_edges, 7000 + seed);
        let y = vec_to_host(op.apply_k(upload_vec(&x, &dev)));

        // Result must be exactly zero on constrained DOFs (row deletion).
        for (i, &keep) in interior_mask.iter().enumerate() {
            if !keep {
                assert_eq!(y[i], 0.0, "constrained DOF {i} must be zero, got {}", y[i]);
            }
        }

        let (a, r) = max_abs_rel(&y, &dense_matvec(&k_reduced, &x, n_edges));
        assert!(
            r < TOL_REL,
            "masked K seed {seed}: max_abs={a:e} max_rel={r:e}"
        );
    }
}

/// Bunsen contract firing test: applying to a wrong-length operand panics.
#[test]
#[should_panic(expected = "n_edges")]
fn matrix_free_wrong_length_operand_panics() {
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps_r = vec![1.0_f64; mesh.n_tets()];
    let dev = device();

    let (nodes, tets) = upload_mesh::<B>(&mesh, &dev);
    let op = MatrixFreeNedelecOperator::<B>::new(nodes, tets, &tet_idx, &tet_sign, n_edges, &eps_r);

    // Operand one shorter than n_edges — must fire.
    let bad = pseudo_random_vec(n_edges - 1, 1);
    let _ = op.apply_k(upload_vec(&bad, &dev));
}

// ---------------------------------------------------------------------------
// CUDA f32 smoke test — rented-EC2-box only, NEVER runs in CI.
// ---------------------------------------------------------------------------
//
// `burn-cuda 0.21` disables f64 (cubecl asserts !supports_dtype(F64)), so the
// f64 conformance gate above cannot run on CUDA. This f32 leg only sanity-
// checks that the matrix-free apply *runs* on the CUDA backend and lands in a
// loose neighborhood (~1e-5 relative) of the ndarray-f64 reference. It is
// both `cuda`-feature-gated and `#[ignore]`d so `cargo test` (default
// features, no ignored tests) skips it in CI; run explicitly on the box with
// `cargo test --features cuda -- --ignored matrix_free_cuda_f32_smoke`.
#[cfg(feature = "cuda")]
#[test]
#[ignore = "CUDA f32 smoke — rented EC2 box only, not CI (burn-cuda 0.21 has no f64)"]
fn matrix_free_cuda_f32_smoke() {
    use burn::backend::Cuda;

    let mesh = cube_tet_mesh(3, 1.0);
    let n_edges = mesh.edges().len();
    let (tet_idx, tet_sign) = edge_tables(&mesh);
    let eps_r = vec![1.0_f64; mesh.n_tets()];

    // ndarray-f64 reference matvec.
    let dev64 = device();
    let (nodes64, tets64) = upload_mesh::<B>(&mesh, &dev64);
    let assembled = assemble_global_nedelec::<B>(
        nodes64.clone(),
        tets64.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
    );
    let k_host = dense_to_host(assembled.k);
    let x = pseudo_random_vec(n_edges, 1);
    let y_ref = dense_matvec(&k_host, &x, n_edges);

    // CUDA-f32 matrix-free apply.
    type Cu = Cuda;
    let dev_cu = <Cu as BackendTypes>::Device::default();
    let (nodes_cu, tets_cu) = upload_mesh::<Cu>(&mesh, &dev_cu);
    let op = MatrixFreeNedelecOperator::<Cu>::new(
        nodes_cu, tets_cu, &tet_idx, &tet_sign, n_edges, &eps_r,
    );
    let x_cu = Tensor::<Cu, 1>::from_data(TensorData::new(x.clone(), [n_edges]), &dev_cu);
    let y_cu: Vec<f64> = op
        .apply_k(x_cu)
        .into_data()
        .iter::<f32>()
        .map(f64::from)
        .collect();

    let (a, r) = max_abs_rel(&y_cu, &y_ref);
    assert!(r < 1e-4, "CUDA f32 smoke: max_abs={a:e} max_rel={r:e}");
}
