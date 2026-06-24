//! Discrete de Rham exactness — bit-exact `d¹ ∘ d⁰ ≡ 0` (issue #78,
//! Phase 2.B of Epic #57) and `d² ∘ d¹ ≡ 0` (issue #91, the deferred-
//! `d²` follow-up).
//!
//! With `gradient_map` (d⁰, #58 / PR #74), `curl_map` (d¹, #77 / PR
//! #79), and `divergence_map` (d², #91) all on `main`, both discrete
//! de Rham exactness identities reduce to one-line algebraic claims:
//! the sparse matrix products `d¹ · d⁰` and `d² · d¹` must each be
//! **exactly** the zero matrix.
//!
//! This is strictly stronger than the Phase 1.B numerical near-zero
//! statement (#59, residual ratio ~1e-12 on f64): every stored value
//! in each product (and every entry of the dense expansion) must equal
//! `0.0` bit-for-bit.
//!
//! The products are bit-exact because all three operators store
//! entries in `{-1, 0, +1}`, so every entry of either matrix product
//! is an integer sum of at most a small constant number of `±1` terms
//! — well below 2⁵³, the f64 exact-integer ceiling. (At most 6 terms
//! for `d¹·d⁰` and at most 12 terms for `d²·d¹`; cf. the per-row
//! nonzero counts of `d⁰`, `d¹`, and `d²`.)
//!
//! # Why this file is not `#[ignore]`d
//!
//! The test is pure host-side sparse linear algebra (construct two
//! `SparseColMat<usize, f64>` operators, multiply, walk the result).
//! It never touches faer's `gevd::qz_real` and so does not trip the
//! `debug-assertions` panic that forces other geode-core tests
//! (`eigensolver`, `sphere_pec_eigenmode`, …) to require `--release`.
//! Same reasoning as `derham_gradient_kernel.rs` (#59 / PR #75) and
//! the early-warning identity tests in `derham_curl.rs` (#77) and
//! `derham_divergence.rs` (#91).
//!
//! # Fixtures
//!
//! - Cube PEC at `n=8` (the canonical eigenmode/gauge-test
//!   refinement) — wider than the `n=2` cube case already in
//!   `derham_curl.rs` and `derham_divergence.rs`.
//! - The bundled sphere PML fixture (`read_sphere_fixture()`). The
//!   PML lives in `M`, not `K`, so both de Rham identities are
//!   properties of the mesh alone — no PML setup is required; we just
//!   read `f.mesh` and call `gradient_map` / `curl_map` /
//!   `divergence_map`.

use faer::sparse::SparseColMat;
use geode_core::{
    TetMesh, cube_tet_mesh, curl_map, divergence_map, gradient_map, read_sphere_fixture,
};

/// Assert that `&a * &b` is bit-exactly the zero matrix of shape
/// `expected_shape = (a.n_rows, b.n_cols)`. Walks both the stored CSC
/// value slice and the dense expansion (in that order); a hypothetical
/// future faer that strips structural zeros after multiplication is
/// caught by the dense walk.
///
/// `name` and the row/col labels are interpolated into failure
/// messages so a sign-convention regression in any of the three
/// operators surfaces with an actionable diagnostic.
///
/// Returns the number of stored nonzeros in the product (for
/// diagnostic reporting via `eprintln!`).
fn assert_composition_is_bit_exact_zero(
    name: &str,
    a: &SparseColMat<usize, f64>,
    b: &SparseColMat<usize, f64>,
    expected_shape: (usize, usize),
    row_label: &str,
    col_label: &str,
) -> usize {
    let (a_rows, a_cols) = a.shape();
    let (b_rows, b_cols) = b.shape();
    assert_eq!(
        a_cols, b_rows,
        "{name}: shape mismatch (a is {a_rows}×{a_cols}, b is {b_rows}×{b_cols})"
    );
    assert_eq!(
        (a_rows, b_cols),
        expected_shape,
        "{name}: product shape ({a_rows}×{b_cols}) disagrees with expected \
         ({} × {})",
        expected_shape.0,
        expected_shape.1
    );

    // faer 0.24: `&SparseColMat * &SparseColMat` is the canonical
    // sparse-product form (see #77 / PR #79 for the verified usage
    // pattern; same call site shape as `derham_curl.rs`).
    let product: SparseColMat<usize, f64> = a * b;

    assert_eq!(
        product.shape(),
        expected_shape,
        "{name}: product shape disagrees with expected after multiply"
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
            "{name}: stored value #{k} of product = {v}, expected 0 (bit-exact)"
        );
    }

    // Second sweep: dense expansion catches anything the structural
    // walk might miss (e.g. a hypothetical future faer that does
    // strip structural zeros — the dense walk is invariant to that).
    let dense = product.to_dense();
    let (n_rows, n_cols) = expected_shape;
    for r in 0..n_rows {
        for c in 0..n_cols {
            let v = dense[(r, c)];
            assert_eq!(
                v, 0.0,
                "{name}: product[{row_label}={r}, {col_label}={c}] = {v}, \
                 expected 0 (bit-exact)"
            );
        }
    }

    stored_nnz
}

/// Compute `d¹ · d⁰` on `mesh` and assert that every entry is
/// bit-exact zero. Returns `(n_faces, n_nodes, stored_nnz)`.
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

    let stored = assert_composition_is_bit_exact_zero(
        &format!("{label} d¹·d⁰"),
        &d1,
        &d0,
        (n_faces, n_nodes),
        "face",
        "node",
    );
    (n_faces, n_nodes, stored)
}

/// Compute `d² · d¹` on `mesh` and assert that every entry is
/// bit-exact zero. Returns `(n_tets, n_edges, stored_nnz)`.
fn assert_d2_d1_is_bit_exact_zero(mesh: &TetMesh, label: &str) -> (usize, usize, usize) {
    let d1 = curl_map(mesh);
    let d2 = divergence_map(mesh);

    let n_tets = mesh.n_tets();
    let n_edges = mesh.edges().len();
    let n_faces = mesh.faces().len();

    assert_eq!(
        d1.shape(),
        (n_faces, n_edges),
        "{label}: d¹ must be n_faces × n_edges"
    );
    assert_eq!(
        d2.shape(),
        (n_tets, n_faces),
        "{label}: d² must be n_tets × n_faces"
    );

    let stored = assert_composition_is_bit_exact_zero(
        &format!("{label} d²·d¹"),
        &d2,
        &d1,
        (n_tets, n_edges),
        "tet",
        "edge",
    );
    (n_tets, n_edges, stored)
}

#[test]
fn cube_pec_d1_d0_exact_sequence() {
    // Canonical n=8 cube refinement — same family as the
    // eigenmode/gauge-test fixtures (one wider than the n=2 cube
    // case already in `derham_curl.rs::curl_of_gradient_is_zero_on_cube_fixture`).
    let mesh = cube_tet_mesh(8, 1.0);
    let (n_faces, n_nodes, stored) = assert_d1_d0_is_bit_exact_zero(&mesh, "cube n=8");
    eprintln!(
        "cube n=8 d¹·d⁰: n_nodes={n_nodes}, n_edges={}, n_faces={n_faces}, stored_nnz={stored}",
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
        "sphere PML d¹·d⁰: n_nodes={n_nodes}, n_edges={}, n_faces={n_faces}, stored_nnz={stored}",
        f.mesh.edges().len()
    );

    assert!(
        n_faces > 0 && n_nodes > 0,
        "sphere PML: mesh must be nonempty (got n_faces={n_faces}, n_nodes={n_nodes})"
    );
}

#[test]
fn cube_pec_d2_d1_exact_sequence() {
    // Same canonical n=8 cube refinement, second exactness identity:
    // `d² · d¹ ≡ 0` (issue #91). Argument: each row of `d¹` is the
    // signed boundary 1-cycle of a face; each row of `d²` is the
    // signed boundary 2-chain of a tet. Their composition computes
    // the boundary of a boundary, which is zero — and because every
    // term is an integer sum of at most 12 terms in {-1, 0, +1}
    // (4 faces · 3 edges per face), the sum stays well below 2⁵³ and
    // is bit-exact in f64.
    let mesh = cube_tet_mesh(8, 1.0);
    let (n_tets, n_edges, stored) = assert_d2_d1_is_bit_exact_zero(&mesh, "cube n=8");
    eprintln!(
        "cube n=8 d²·d¹: n_tets={n_tets}, n_faces={}, n_edges={n_edges}, stored_nnz={stored}",
        mesh.faces().len()
    );

    assert!(
        n_tets > 0 && n_edges > 0,
        "cube n=8: mesh must be nonempty (got n_tets={n_tets}, n_edges={n_edges})"
    );
}

#[test]
fn sphere_pml_d2_d1_exact_sequence() {
    // The bundled sphere fixture — same reasoning as
    // `sphere_pml_d1_d0_exact_sequence`. The second exactness identity
    // depends only on the mesh connectivity, not on the PML; we just
    // read `f.mesh` and call the three cochain operators.
    let f = read_sphere_fixture().expect("sphere fixture load");
    let (n_tets, n_edges, stored) = assert_d2_d1_is_bit_exact_zero(&f.mesh, "sphere PML");
    eprintln!(
        "sphere PML d²·d¹: n_tets={n_tets}, n_faces={}, n_edges={n_edges}, stored_nnz={stored}",
        f.mesh.faces().len()
    );

    assert!(
        n_tets > 0 && n_edges > 0,
        "sphere PML: mesh must be nonempty (got n_tets={n_tets}, n_edges={n_edges})"
    );
}
