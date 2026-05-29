//! Discrete de Rham exactness — bit-exact `d¹ ∘ d⁰ ≡ 0` (issue #78,
//! Phase 2.B of Epic #57).
//!
//! With `gradient_map` (d⁰, #58 / PR #74) and `curl_map` (d¹, #77 /
//! PR #79) both on `main`, the discrete de Rham identity reduces to a
//! one-line algebraic claim: the sparse matrix product `d¹ · d⁰` must
//! be **exactly** the zero matrix.
//!
//! This is strictly stronger than the Phase 1.B numerical
//! near-zero statement (#59, residual ratio ~1e-12 on f64): every
//! stored value in the product (and every entry of the dense
//! expansion) must equal `0.0` bit-for-bit.
//!
//! The product is bit-exact because both d⁰ and d¹ store entries in
//! `{-1, 0, +1}`, so every entry of the matrix product is an integer
//! sum of at most a small constant number of `±1` terms — well below
//! 2⁵³, the f64 exact-integer ceiling.
//!
//! # Why this file is not `#[ignore]`d
//!
//! The test is pure host-side sparse linear algebra (construct two
//! `SparseColMat<usize, f64>` operators, multiply, walk the result).
//! It never touches faer's `gevd::qz_real` and so does not trip the
//! `debug-assertions` panic that forces other geode-core tests
//! (`eigensolver`, `sphere_pec_eigenmode`, …) to require `--release`.
//! Same reasoning as `derham_gradient_kernel.rs` (#59 / PR #75) and
//! the early-warning identity tests in `derham_curl.rs` (#77).
//!
//! # Fixtures
//!
//! - Cube PEC at `n=8` (the canonical eigenmode/gauge-test
//!   refinement) — wider than the `n=2` cube case already in
//!   `derham_curl.rs`.
//! - The bundled sphere PML fixture (`read_sphere_fixture()`). The
//!   PML lives in `M`, not `K`, so the de Rham identity is a
//!   property of the mesh alone — no PML setup is required; we just
//!   read `f.mesh` and call `gradient_map` / `curl_map`.

use faer::sparse::SparseColMat;
use geode_core::{cube_tet_mesh, curl_map, gradient_map, read_sphere_fixture, TetMesh};

/// Compute `d¹ · d⁰` on `mesh` and assert that every entry of the
/// result is bit-exact zero (both the stored sparse values and the
/// dense expansion). On failure, prints the offending
/// `(face_index, node_index, value)` triple so a sign-convention
/// regression in `gradient_map` or `curl_map` surfaces with an
/// actionable diagnostic (issue #78 acceptance criterion).
fn assert_d1_d0_is_bit_exact_zero(mesh: &TetMesh, label: &str) -> (usize, usize, usize) {
    let d0 = gradient_map(mesh);
    let d1 = curl_map(mesh);

    let n_nodes = mesh.n_nodes();
    let n_edges = mesh.edges().len();
    let n_faces = mesh.faces().len();

    assert_eq!(
        d0.shape(),
        (n_edges, n_nodes),
        "{label}: d⁰ must be n_edges × n_nodes"
    );
    assert_eq!(
        d1.shape(),
        (n_faces, n_edges),
        "{label}: d¹ must be n_faces × n_edges"
    );

    // faer 0.24: `&SparseColMat * &SparseColMat` is the canonical
    // sparse-product form (see #77 / PR #79 for the verified usage
    // pattern; same call site shape as `derham_curl.rs`).
    let product: SparseColMat<usize, f64> = &d1 * &d0;

    assert_eq!(
        product.shape(),
        (n_faces, n_nodes),
        "{label}: d¹·d⁰ must be n_faces × n_nodes"
    );

    // First sweep: walk the stored value slice directly (faer 0.24
    // exposes it via `.val()` on the CSC reference). faer does not
    // strip structural zeros after multiplication in general, so a
    // numeric-zero entry can still appear in the slice — we require
    // each such entry to be `== 0.0` bit-for-bit. We do not paper
    // over this with a tolerance; the point of this test is the
    // algebraic identity.
    let stored_nnz = product.as_ref().val().len();
    for (k, &v) in product.as_ref().val().iter().enumerate() {
        assert_eq!(
            v, 0.0,
            "{label}: stored value #{k} of d¹·d⁰ = {v}, expected 0 (bit-exact)"
        );
    }

    // Second sweep: dense expansion catches anything the structural
    // walk might miss (e.g. a hypothetical future faer that does
    // strip structural zeros — the dense walk is invariant to that).
    // Mirrors the assertion style used in `derham_curl.rs`'s
    // `curl_of_gradient_is_zero_on_*` tests (#77 / PR #79).
    let dense = product.to_dense();
    for r in 0..n_faces {
        for c in 0..n_nodes {
            let v = dense[(r, c)];
            assert_eq!(
                v, 0.0,
                "{label}: (d¹·d⁰)[face={r}, node={c}] = {v}, expected 0 (bit-exact)"
            );
        }
    }

    (n_faces, n_nodes, stored_nnz)
}

#[test]
fn cube_pec_d1_d0_exact_sequence() {
    // Canonical n=8 cube refinement — same family as the
    // eigenmode/gauge-test fixtures (one wider than the n=2 cube
    // case already in `derham_curl.rs::curl_of_gradient_is_zero_on_cube_fixture`).
    let mesh = cube_tet_mesh(8, 1.0);
    let (n_faces, n_nodes, stored) = assert_d1_d0_is_bit_exact_zero(&mesh, "cube n=8");
    eprintln!(
        "cube n=8: n_nodes={n_nodes}, n_edges={}, n_faces={n_faces}, stored_nnz={stored}",
        mesh.edges().len()
    );

    // Sanity: the cube mesh at n=8 must have plenty of faces and
    // nodes — otherwise the assertion above is vacuously true.
    // Exact counts depend on `cube_tet_mesh`'s tessellation; we
    // only assert nontriviality here so a future refactor that
    // accidentally returns an empty mesh fails loudly.
    assert!(
        n_faces > 0 && n_nodes > 0,
        "cube n=8: mesh must be nonempty (got n_faces={n_faces}, n_nodes={n_nodes})"
    );
}

#[test]
fn sphere_pml_d1_d0_exact_sequence() {
    // The bundled sphere fixture — the same mesh used by the
    // eigenmode/Silver–Müller tests. The PML lives in M (the mass
    // matrix), so it does not affect the cochain operators d⁰ and
    // d¹ at all; the de Rham identity is a property of the mesh
    // connectivity only.
    let f = read_sphere_fixture().expect("sphere fixture load");
    let (n_faces, n_nodes, stored) = assert_d1_d0_is_bit_exact_zero(&f.mesh, "sphere PML");
    eprintln!(
        "sphere PML: n_nodes={n_nodes}, n_edges={}, n_faces={n_faces}, stored_nnz={stored}",
        f.mesh.edges().len()
    );

    assert!(
        n_faces > 0 && n_nodes > 0,
        "sphere PML: mesh must be nonempty (got n_faces={n_faces}, n_nodes={n_nodes})"
    );
}
