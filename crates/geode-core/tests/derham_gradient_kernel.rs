//! Operator-level U(1) gauge-symmetry test for the Nédélec curl-curl
//! assembly (Epic #57, Phase 1.B; depends on the `d⁰` operator from #58).
//!
//! # What this proves
//!
//! Whitney 1-forms (the first-order Nédélec edge basis) contain every
//! discrete gradient `∇φ` of a nodal scalar field `φ`. Because `curl ∇φ ≡ 0`,
//! the assembled curl-curl matrix `K` must annihilate the image of the
//! discrete gradient operator `d⁰`: for any nodal `φ` vanishing on the
//! Dirichlet boundary, `g = d⁰ · φ` (restricted to the interior edge DOFs)
//! satisfies `K · g = 0`.
//!
//! The eigenmode tests (`nedelec_cavity.rs`, `sphere_pec_eigenmode.rs`)
//! already check this *kernel* via the eigenvalue **count** — the
//! curl-curl operator has a near-zero eigenvalue cluster of dimension
//! `n_interior_nodes`. But those eigenvalues only land near zero at
//! roughly f32 precision (the kernel cluster carries `~eps_f32 × λ_max`
//! noise from the f32 Burn-tensor assembly). This test is the **direct,
//! operator-level** statement: instead of asking the eigensolver to *find*
//! the kernel, we *hand it* known kernel vectors `d⁰ · φ` and check that
//! `K` sends them to zero:
//!
//! ```text
//! ‖K · g‖ / (‖K‖_F · ‖g‖) < tol
//! ```
//!
//! # The tolerance is backend-dependent (and that is the point)
//!
//! `d⁰` stores exact `±1.0` entries, so `g = d⁰·φ` is exact in f64 — the
//! residual floor is set entirely by the precision at which `K` was
//! **assembled and accumulated inside Burn**, not by the readout.
//! `burn_matrix_to_faer` widens `K` to f64 but cannot recover precision
//! already lost to f32 storage. So the floor tracks the backend's float
//! element type:
//!
//!   * **f64 backend** (`ndarray`, the CPU backend CI runs): observed ratio
//!     `~1e-18`, far under the `1e-12` f64-tight bound. This is the headline
//!     U(1) statement the epic asks for.
//!   * **f32 backend** (`wgpu` / `cuda`, the default GPU path): observed
//!     ratio `~1e-9`. Gauge annihilation still holds, but only to f32
//!     storage precision; asserting `1e-12` there would be a false failure.
//!
//! `residual_tol()` picks the bound from `size_of::<FloatElem<B>>()`, so
//! `cargo test` is green on either backend while the f64-tight claim is
//! genuinely exercised on the f64 backend. (See PR #74 / #58 for why `d⁰`
//! is stored as exact `±1.0` f64 rather than `i32`.)
//!
//! # Why this test is NOT `#[ignore]`'d (the faer-overflow question)
//!
//! The other Nédélec integration tests are `#[ignore]`'d because faer
//! 0.24's generalized-eigendecomposition path (`gevd::qz_real` /
//! `qz_complex`) panics under `debug-assertions` (an arithmetic overflow
//! in faer's internal pivoting). **This test never calls a generalized
//! eigensolver.** It only:
//!
//!   1. applies `d⁰` to a nodal field (an explicit edge-difference loop),
//!   2. reads `K` out as a dense `faer::Mat<f64>`,
//!   3. extracts the interior submatrix and computes a dense mat-vec
//!      `K_int · g_int`, and
//!   4. takes `‖·‖_2` / Frobenius norms.
//!
//! None of those touch `gevd`, so the faer debug-assertion overflow is not
//! reachable here. Verified empirically: the test is green in plain debug
//! mode (`cargo test -p geode-core --test derham_gradient_kernel`) under
//! both the default wgpu (f32) backend and the ndarray (f64) CPU backend,
//! each against its `residual_tol()` floor. No `#[ignore]` is needed, and
//! the test does not require `--release`.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --test derham_gradient_kernel
//! # CPU backend (CI parity):
//! cargo test --no-default-features --features ndarray \
//!     -p geode-core --test derham_gradient_kernel
//! ```

use burn::tensor::backend::BackendTypes;
use faer::Mat;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec, assemble_global_nedelec_with_complex_epsilon,
    build_complex_epsilon_r_pml, cube_pec_interior_edges, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::testing::TestBackend;
use geode_core::derham::apply_gradient;
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask};
use geode_core::mesh::{R_BUFFER, TetMesh, cube_tet_mesh, read_sphere_fixture};

type B = TestBackend;

/// Number of random φ fields per fixture.
const N_FIELDS: usize = 10;

/// Gauge-annihilation floor, keyed to the backend's float storage precision.
/// The `d⁰` image is exact, so the residual is bounded by the precision at
/// which `K` was assembled inside Burn — f64 on the `ndarray` CPU backend
/// (CI), f32 on the `wgpu`/`cuda` GPU backends. Reading `K` out as f64 via
/// `burn_matrix_to_faer` does not recover precision lost to f32 storage, so
/// the bound must track `size_of::<FloatElem<B>>()`.
fn residual_tol() -> f64 {
    match core::mem::size_of::<<B as BackendTypes>::FloatElem>() {
        8 => 1e-12, // f64 backend: the headline U(1) f64-tight statement
        _ => 1e-6,  // f32 backend: gauge annihilation to f32 storage precision
    }
}

/// Cube refinement — matches the `nedelec_cavity.rs` cavity tests so the
/// interior-node / interior-edge counts line up with the eigenmode story.
const N_CUBE: usize = 8;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Deterministic LCG, same constants and reduction as
/// `tests/p1_local_matrices.rs::deterministic_tet`, so failures are
/// reproducible across runs and machines. Returns a closure yielding
/// values in `[0, 1)`.
fn lcg(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed.wrapping_mul(2_654_435_761);
    move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state as f64) / (u32::MAX as f64)
    }
}

/// Build a discrete-`H¹₀` nodal field: random values in `[-1, 1)` at
/// interior nodes (where `interior_node_mask` is `true`), exactly `0.0` at
/// Dirichlet-boundary nodes. The boundary zeros are what make `K · d⁰ φ`
/// the *interior* gauge-kernel statement.
fn random_h1_zero_field(interior_node_mask: &[bool], next: &mut impl FnMut() -> f64) -> Vec<f64> {
    interior_node_mask
        .iter()
        .map(|&interior| if interior { 2.0 * next() - 1.0 } else { 0.0 })
        .collect()
}

/// Assemble the curl-curl matrix `K` for the cube PEC fixture and return
/// it as a dense `faer::Mat<f64>` alongside the node- and edge-level
/// interior masks needed to build the gauge field and restrict the system.
fn cube_fixture() -> (Mat<f64>, Vec<bool>, Vec<bool>, TetMesh) {
    let mesh = cube_tet_mesh(N_CUBE, 1.0);
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &device());

    let tet_edges_v = mesh.tet_edges();
    let n_edges = mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let sys = assemble_global_nedelec(nodes_t, tets_t, &tet_idx, &tet_sign, n_edges);
    let k_full = burn_matrix_to_faer(sys.k);

    let node_interior_mask = cube_interior_mask(&mesh.nodes, 1.0);
    let (_edges, edge_interior_mask) = cube_pec_interior_edges(&mesh, 1.0);
    assert_eq!(edge_interior_mask.len(), n_edges, "edge ordering mismatch");

    (k_full, node_interior_mask, edge_interior_mask, mesh)
}

/// Assemble the curl-curl matrix `K` for the sphere PML fixture and return
/// it as a dense `faer::Mat<f64>` alongside the node- and edge-level
/// interior masks (PEC outer wall at `r = R_BUFFER`).
///
/// The PML lives entirely in the **mass** matrix (a complex `ε(x)`
/// scaling); the curl-curl `K` is real and ε-independent. Gauge symmetry
/// is a property of `K` alone, so we assemble the complex-ε system and read
/// out only its real `K`.
fn sphere_pml_fixture() -> (Mat<f64>, Vec<bool>, Vec<bool>, TetMesh) {
    let f = read_sphere_fixture().expect("sphere fixture load");

    // PML profile mirrors `sphere_pml_eigenmode.rs`: n = 1.5 dielectric,
    // σ₀ = 5.0 quadratic absorption ramp in the buffer. Only the complex M
    // sees this; K is unaffected.
    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 5.0);

    let tet_edges_v = f.mesh.tet_edges();
    let n_edges = f.mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges_v
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex,
    );
    let k_full = burn_matrix_to_faer(sys.k);

    // Node-level interior mask: a node is interior iff it is NOT on the
    // outer PEC wall (r ≈ R_BUFFER). Mirrors the radius convention used by
    // `sphere_pec_interior_edges` / `sphere_n_interior_nodes`.
    let tol = 1e-6 * R_BUFFER.max(1.0);
    let node_interior_mask: Vec<bool> = f
        .mesh
        .nodes
        .iter()
        .map(|p| {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            (r - R_BUFFER).abs() >= tol
        })
        .collect();

    let (_edges, edge_interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(edge_interior_mask.len(), n_edges, "edge ordering mismatch");

    (k_full, node_interior_mask, edge_interior_mask, f.mesh)
}

/// Core driver: for a fixture's `K` (dense, full-edge), node/edge interior
/// masks, and mesh, draw `N_FIELDS` random `H¹₀` fields, push them through
/// `d⁰` and the interior-restricted `K`, and return the **maximum**
/// residual ratio `‖K_int · g_int‖ / (‖K_int‖_F · ‖g_int‖)` over all
/// fields. Asserts each individual field clears `residual_tol()`.
fn max_gauge_residual_ratio(
    fixture_name: &str,
    k_full: &Mat<f64>,
    node_interior_mask: &[bool],
    edge_interior_mask: &[bool],
    mesh: &TetMesh,
    seed_base: u32,
) -> f64 {
    // Restrict K to interior × interior edges. `apply_dirichlet_bc` returns
    // (K_int, M_int); we pass a dummy zero M and discard it.
    let dummy_m = Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_m.as_ref(), edge_interior_mask)
        .expect("BC reduction of K");
    let k_norm_f = k_int.norm_l2();
    assert!(
        k_norm_f > 0.0,
        "{fixture_name}: ‖K_int‖_F is zero — empty/degenerate system"
    );

    // Indices of the surviving interior edge DOFs, in order.
    let interior_edge_idx: Vec<usize> = edge_interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_edge_idx.len();
    assert!(dim > 0, "{fixture_name}: no interior edge DOFs survived");

    let n_interior_nodes = node_interior_mask.iter().filter(|&&b| b).count();
    let tol = residual_tol();
    eprintln!(
        "[{fixture_name}] {} interior edge DOFs, {} interior nodes, ‖K_int‖_F = {:.4e}, tol = {:.0e}",
        dim, n_interior_nodes, k_norm_f, tol
    );

    let mut max_ratio = 0.0_f64;
    for field in 0..N_FIELDS {
        let mut next = lcg(seed_base.wrapping_add(field as u32));
        let phi = random_h1_zero_field(node_interior_mask, &mut next);

        // g = d⁰ · φ over all edges, then restrict to interior DOFs.
        let g_full = apply_gradient(mesh, &phi);
        let g_int = Mat::<f64>::from_fn(dim, 1, |i, _| g_full[interior_edge_idx[i]]);
        let g_norm = g_int.norm_l2();
        // A nonzero H¹₀ field on a connected mesh has a nonzero gradient on
        // at least one interior edge; guard against a degenerate all-zero
        // draw so the ratio denominator is well-defined.
        assert!(
            g_norm > 0.0,
            "{fixture_name} field {field}: g = d⁰·φ vanished entirely (degenerate draw)"
        );

        // Residual r = K_int · g_int (dense mat-vec, no eigensolver).
        let residual = &k_int * &g_int;
        let r_norm = residual.norm_l2();
        let ratio = r_norm / (k_norm_f * g_norm);

        eprintln!(
            "[{fixture_name}] field {field:>2}: ‖K·g‖ = {:.3e}, ‖g‖ = {:.3e}, ratio = {:.3e}",
            r_norm, g_norm, ratio
        );

        assert!(
            ratio < tol,
            "{fixture_name} field {field}: gauge residual ratio {ratio:.3e} exceeds \
             tolerance {tol:.0e} — curl-curl assembly does not annihilate \
             the discrete gradient d⁰·φ to backend-float precision"
        );

        if ratio > max_ratio {
            max_ratio = ratio;
        }
    }
    max_ratio
}

#[test]
fn cube_pec_curl_curl_annihilates_gradients() {
    let (k_full, node_mask, edge_mask, mesh) = cube_fixture();
    let max_ratio = max_gauge_residual_ratio(
        "cube n=8 PEC",
        &k_full,
        &node_mask,
        &edge_mask,
        &mesh,
        0x_C0BE_0000,
    );
    eprintln!(
        "cube n=8 PEC: max gauge residual ratio over {} fields = {:.3e} (tol {:.0e})",
        N_FIELDS,
        max_ratio,
        residual_tol()
    );
    assert!(max_ratio < residual_tol());
}

#[test]
fn sphere_pml_curl_curl_annihilates_gradients() {
    let (k_full, node_mask, edge_mask, mesh) = sphere_pml_fixture();
    let max_ratio = max_gauge_residual_ratio(
        "sphere PML",
        &k_full,
        &node_mask,
        &edge_mask,
        &mesh,
        0x_5E1E_0000, // distinct seed family from the cube fixture
    );
    eprintln!(
        "sphere PML: max gauge residual ratio over {} fields = {:.3e} (tol {:.0e})",
        N_FIELDS,
        max_ratio,
        residual_tol()
    );
    assert!(max_ratio < residual_tol());
}
