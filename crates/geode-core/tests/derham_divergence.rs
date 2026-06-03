//! Tests for the discrete divergence operator `d²`
//! (`geode_core::derham::divergence_map`).
//!
//! Covers the four acceptance checks from issue #91:
//!   1. Shape: `d²` is `n_tets × n_faces`.
//!   2. Sparsity: every row has exactly four nonzeros, each `±1.0`.
//!   3. Internal-face columns have exactly two entries with opposite
//!      signs; boundary-face columns have exactly one entry.
//!   4. Single reference-tet hand check (single all-boundary tet → row
//!      of four +1s).
//!
//! Plus an early-warning bit-exact `d² · d¹ = 0` sanity check on the
//! reference tet and the cube `n=2` fixture. The formal acceptance
//! tests on the cube `n=8` and sphere PML fixtures live in
//! `derham_exact_sequence.rs`, alongside the `d¹ · d⁰ = 0` ones, so
//! both bit-exact compositions read at one site.
//!
//! # Why this file is not `#[ignore]`d
//!
//! The test is pure host-side sparse linear algebra (construct
//! `SparseColMat<usize, f64>` operators, multiply, walk the result).
//! It never touches faer's `gevd::qz_real` and so does not trip the
//! `debug-assertions` panic that forces other geode-core tests
//! (`eigensolver`, `sphere_pec_eigenmode`, …) to require `--release`.
//! Same reasoning as `derham_exact_sequence.rs` (#78 / PR #80) and
//! `derham_curl.rs` (#77 / PR #79).

use std::collections::{BTreeMap, HashMap};

use geode_core::{apply_divergence, cube_tet_mesh, curl_map, divergence_map, TetMesh};

/// A single tet on nodes 0..4. Connectivity is all that matters for
/// `d²`; the coordinates below form a unit reference tet for good
/// measure.
fn reference_tet() -> TetMesh {
    TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ],
        tets: vec![[0, 1, 2, 3]],
        physical_groups: BTreeMap::new(),
    }
}

#[test]
fn shape_is_n_tets_by_n_faces() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d2 = divergence_map(&mesh);
    let n_tets = mesh.n_tets();
    let n_faces = mesh.faces().len();

    assert_eq!(d2.shape(), (n_tets, n_faces), "d² must be n_tets × n_faces");
}

#[test]
fn every_row_has_four_signed_unit_nonzeros() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d2 = divergence_map(&mesh);
    let dense = d2.to_dense();

    let n_tets = mesh.n_tets();
    let n_faces = mesh.faces().len();

    for r in 0..n_tets {
        let mut nnz = 0usize;
        let mut plus = 0usize;
        let mut minus = 0usize;
        for c in 0..n_faces {
            let v = dense[(r, c)];
            if v != 0.0 {
                nnz += 1;
                if v == 1.0 {
                    plus += 1;
                } else if v == -1.0 {
                    minus += 1;
                } else {
                    panic!("row {r}, col {c}: unexpected entry {v}, expected ±1");
                }
            }
        }
        assert_eq!(nnz, 4, "row {r} must have exactly 4 nonzeros");
        assert_eq!(
            plus + minus,
            4,
            "row {r}: every stored value must be exactly ±1.0 (bit-exact)"
        );
    }
}

#[test]
fn single_reference_tet_matches_hand_tabulated_row() {
    let mesh = reference_tet();
    let d2 = divergence_map(&mesh);

    // 1 tet, 4 faces.
    assert_eq!(d2.shape(), (1, 4));
    assert_eq!(
        mesh.faces(),
        vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        "face enumeration must match the canonical ascending order"
    );

    // For the trivially-ordered reference tet [0,1,2,3], every local
    // face's ascending-local cycle (from TET_LOCAL_FACES) is *already*
    // the ascending-global cycle, so `tet_faces[0]` returns all four
    // `sign_k` values as +1 (verified independently in `mesh::tests::
    // tet_faces_on_reference_tet_have_positive_signs`).
    //
    // The divergence row therefore reduces to the alternating
    // `(−1)^k · 1` pattern over local-face slots `k = 0..4`. Mapped to
    // global face indices, that is:
    //
    //   local k=0 (face opp v0 = {1,2,3} = global face 3): (−1)^0 = +1
    //   local k=1 (face opp v1 = {0,2,3} = global face 2): (−1)^1 = −1
    //   local k=2 (face opp v2 = {0,1,3} = global face 1): (−1)^2 = +1
    //   local k=3 (face opp v3 = {0,1,2} = global face 0): (−1)^3 = −1
    //
    // so the d² row, indexed by global face column, is:
    //   col 0 (face {0,1,2}):  −1
    //   col 1 (face {0,1,3}):  +1
    //   col 2 (face {0,2,3}):  −1
    //   col 3 (face {1,2,3}):  +1
    //
    // This is the signed simplicial boundary of [v0,v1,v2,v3]:
    //   ∂[v0,v1,v2,v3] = [v1,v2,v3] − [v0,v2,v3] + [v0,v1,v3] − [v0,v1,v2]
    // — see the `divergence_map` docstring for the full argument.
    let dense = d2.to_dense();
    let expected_row = [-1.0, 1.0, -1.0, 1.0];
    for (c, &want) in expected_row.iter().enumerate() {
        assert_eq!(
            dense[(0, c)],
            want,
            "d²[0, {c}] = {} but expected {want}",
            dense[(0, c)]
        );
    }
}

#[test]
fn internal_face_columns_have_signed_pair_boundary_faces_have_single() {
    // On a closed-tet mesh every interior face is shared by exactly
    // two tets and every boundary face by exactly one. The d² column
    // for an interior face must therefore have two nonzeros with
    // opposite signs (the two tets' outward normals on the shared face
    // disagree, so their `tet_faces` signs disagree); a boundary-face
    // column must have exactly one nonzero.
    let mesh = cube_tet_mesh(2, 1.0);
    let d2 = divergence_map(&mesh);
    let dense = d2.to_dense();

    let n_tets = mesh.n_tets();
    let n_faces = mesh.faces().len();

    // Independent ground truth: count for each face the number of tets
    // that touch it (1 = boundary, 2 = interior).
    const LOCAL_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];
    let face_index: HashMap<(u32, u32, u32), usize> = mesh
        .faces()
        .iter()
        .enumerate()
        .map(|(i, f)| ((f[0], f[1], f[2]), i))
        .collect();
    let mut tets_per_face = vec![0usize; n_faces];
    for tet in &mesh.tets {
        for lf in &LOCAL_FACES {
            let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
            tri.sort_unstable();
            let key = (tri[0], tri[1], tri[2]);
            let idx = face_index[&key];
            tets_per_face[idx] += 1;
        }
    }

    let mut n_interior = 0usize;
    let mut n_boundary = 0usize;
    for c in 0..n_faces {
        let mut nnz = 0usize;
        let mut sum = 0.0f64;
        for r in 0..n_tets {
            let v = dense[(r, c)];
            if v != 0.0 {
                nnz += 1;
                sum += v;
                assert!(
                    v == 1.0 || v == -1.0,
                    "d²[{r}, {c}] = {v}, expected ±1 (bit-exact)"
                );
            }
        }
        match tets_per_face[c] {
            1 => {
                assert_eq!(
                    nnz, 1,
                    "boundary face column {c} must have exactly 1 nonzero, got {nnz}"
                );
                n_boundary += 1;
            }
            2 => {
                assert_eq!(
                    nnz, 2,
                    "interior face column {c} must have exactly 2 nonzeros, got {nnz}"
                );
                assert_eq!(
                    sum, 0.0,
                    "interior face column {c}: the two ±1 entries must cancel \
                     (sum = {sum} ≠ 0 means same-sign, which would break d²·d¹=0)"
                );
                n_interior += 1;
            }
            other => panic!(
                "face column {c}: {other} tets touch this face on a closed tet mesh \
                 (expected 1 or 2)"
            ),
        }
    }

    // Sanity: the cube at n=2 has both interior and boundary faces.
    assert!(
        n_interior > 0,
        "cube n=2: expected at least one interior face"
    );
    assert!(
        n_boundary > 0,
        "cube n=2: expected at least one boundary face"
    );
}

#[test]
fn divergence_of_curl_is_zero_on_reference_tet() {
    // d² ∘ d¹ ≡ 0 — the second discrete de Rham exactness identity.
    // On the reference tet, the product is the 1×6 zero row bit-exactly.
    let mesh = reference_tet();
    let d1 = curl_map(&mesh);
    let d2 = divergence_map(&mesh);

    let product = &d2 * &d1;
    let dense = product.to_dense();

    let (n_rows, n_cols) = product.shape();
    assert_eq!(
        (n_rows, n_cols),
        (1, 6),
        "d² · d¹ on the reference tet must be n_tets × n_edges = 1 × 6"
    );
    for r in 0..n_rows {
        for c in 0..n_cols {
            let v = dense[(r, c)];
            assert_eq!(v, 0.0, "(d²·d¹)[{r}, {c}] = {v}, expected 0 (bit-exact)");
        }
    }
}

#[test]
fn divergence_of_curl_is_zero_on_cube_fixture() {
    // Same identity on the 2×2×2 hex-split cube — early-warning sibling
    // of the formal acceptance test in `derham_exact_sequence.rs`.
    let mesh = cube_tet_mesh(2, 1.0);
    let d1 = curl_map(&mesh);
    let d2 = divergence_map(&mesh);

    let product = &d2 * &d1;
    let dense = product.to_dense();

    let (n_rows, n_cols) = product.shape();
    assert_eq!(n_rows, mesh.n_tets());
    assert_eq!(n_cols, mesh.edges().len());

    for r in 0..n_rows {
        for c in 0..n_cols {
            let v = dense[(r, c)];
            assert_eq!(
                v, 0.0,
                "(d²·d¹)[{r}, {c}] = {v} on cube fixture, expected 0 (bit-exact)"
            );
        }
    }
}

#[test]
fn apply_divergence_matches_sparse_matvec() {
    // The bare-vector convenience must agree exactly with the
    // materialised sparse mat-vec — identical sign and ordering
    // contract.
    let mesh = cube_tet_mesh(2, 1.0);
    let d2 = divergence_map(&mesh);
    let n_faces = mesh.faces().len();

    // Deterministic non-trivial test field.
    let face_field: Vec<f64> = (0..n_faces).map(|i| (i as f64) - 7.0).collect();

    let got = apply_divergence(&mesh, &face_field);

    // Hand-rolled sparse mat-vec via the dense expansion (the matrix is
    // tiny on n=2). Matches the convention used elsewhere in the test
    // suite (e.g. `derham_curl.rs::sparse_to_dense_vec`).
    let dense = d2.to_dense();
    let n_tets = mesh.n_tets();
    let mut want = vec![0.0f64; n_tets];
    for r in 0..n_tets {
        for c in 0..n_faces {
            want[r] += dense[(r, c)] * face_field[c];
        }
    }

    assert_eq!(got.len(), n_tets);
    for (i, (&g, &w)) in got.iter().zip(want.iter()).enumerate() {
        assert_eq!(g, w, "tet {i}: apply_divergence={g} ≠ matvec={w}");
    }
}

#[test]
fn apply_divergence_of_apply_gradient_via_curl_chain_is_zero() {
    // End-to-end identity on the convenience functions: a nodal field
    // φ has zero divergence after gradient + curl because curl of a
    // gradient is zero. Reuses `apply_gradient` from #58 / PR #74.
    // (This is a sanity check on the function-level convenience APIs;
    // the operator-level identity is verified in
    // `derham_exact_sequence.rs`.)
    let mesh = cube_tet_mesh(2, 1.0);
    let d1 = curl_map(&mesh);
    let n_nodes = mesh.n_nodes();
    let n_edges = mesh.edges().len();

    // Deterministic non-trivial nodal field.
    let phi: Vec<f64> = (0..n_nodes).map(|i| (i as f64) * 0.5 - 1.0).collect();

    // edge field = gradient(φ)
    let edge_field = geode_core::apply_gradient(&mesh, &phi);
    assert_eq!(edge_field.len(), n_edges);

    // face field = d¹ · edge_field (via dense expansion; same matvec
    // convention as `apply_divergence_matches_sparse_matvec`).
    let d1_dense = d1.to_dense();
    let n_faces = mesh.faces().len();
    let mut face_field = vec![0.0f64; n_faces];
    for r in 0..n_faces {
        for c in 0..n_edges {
            face_field[r] += d1_dense[(r, c)] * edge_field[c];
        }
    }

    // Sanity: ensure d¹·d⁰ produces a truly zero face field before we
    // exercise apply_divergence on it (the chain is already exercised
    // operator-side in `derham_exact_sequence.rs`, but checking it
    // here makes the failure mode of the final assertion much easier
    // to localise).
    for (i, &v) in face_field.iter().enumerate() {
        assert_eq!(
            v, 0.0,
            "face field component {i} = {v} is nonzero — d¹·d⁰ should give bit-exact zero"
        );
    }

    // div(curl(grad φ)) = 0 elementwise.
    let div_field = apply_divergence(&mesh, &face_field);
    for (i, &v) in div_field.iter().enumerate() {
        assert_eq!(
            v, 0.0,
            "tet {i}: apply_divergence(apply_curl(apply_gradient(φ))) = {v}, expected 0"
        );
    }
}
