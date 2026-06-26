//! Tests for the discrete curl operator `d¹` (`geode_core::derham`).
//!
//! Covers the four acceptance checks from issue #77:
//!   1. Shape: `d¹` is `n_faces × n_edges`.
//!   2. Sparsity: every row has exactly three nonzeros, each `±1.0`.
//!   3. Single reference-tet hand check against a tabulated matrix.
//!   4. Face count on the cube fixture: matches the Euler-formula count
//!      `n_faces = (4·n_tets + n_boundary_triangles) / 2`.
//!
//! Plus the early-warning bit-exact `d¹ · d⁰ = 0` sanity check on both
//! the reference tet and the cube fixture (the formal acceptance test
//! belongs to Phase 2.B / issue #78, but we run it here too so a sign
//! mistake in [`TET_LOCAL_FACE_EDGES`] surfaces immediately).

use std::collections::BTreeMap;

use faer::sparse::SparseColMat;
use geode_core::derham::{curl_map, gradient_map};
use geode_core::mesh::{TetMesh, cube_tet_mesh};

/// A single tet on nodes 0..4. Connectivity is all that matters for
/// `d¹`; the coordinates below form a unit reference tet for good
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
fn shape_is_n_faces_by_n_edges() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d1 = curl_map(&mesh);
    let n_edges = mesh.edges().len();
    let n_faces = mesh.faces().len();

    assert_eq!(
        d1.shape(),
        (n_faces, n_edges),
        "d¹ must be n_faces × n_edges"
    );
}

#[test]
fn every_row_has_three_signed_unit_nonzeros() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d1 = curl_map(&mesh);
    let dense = d1.to_dense();

    let n_edges = mesh.edges().len();
    let n_faces = mesh.faces().len();

    for r in 0..n_faces {
        let mut nnz = 0usize;
        let mut plus = 0usize;
        let mut minus = 0usize;
        for c in 0..n_edges {
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
        assert_eq!(nnz, 3, "row {r} must have exactly 3 nonzeros");
        // Cycle (a→b→c→a) on (a,b,c) with a<b<c gives two +1s and one −1.
        assert_eq!(plus, 2, "row {r} must have exactly two +1 entries");
        assert_eq!(minus, 1, "row {r} must have exactly one −1 entry");
    }
}

#[test]
fn single_reference_tet_matches_hand_tabulated_matrix() {
    let mesh = reference_tet();
    let d1 = curl_map(&mesh);

    // 4 nodes → 6 edges → 4 faces.
    assert_eq!(d1.shape(), (4, 6));
    assert_eq!(
        mesh.edges(),
        vec![[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
        "edge enumeration must match the canonical lower-tag-first order"
    );
    // Faces are deduplicated lower-tag-first triples (a < b < c), sorted
    // ascending in (BTreeSet) lexicographic order.
    assert_eq!(
        mesh.faces(),
        vec![[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]],
        "face enumeration must match the canonical ascending order"
    );

    // Hand-tabulated d¹: rows = oriented faces (ascending cycle a→b→c→a),
    // cols = oriented edges (lower-tag-first).
    //
    // Edge column key: 0=(0,1), 1=(0,2), 2=(0,3), 3=(1,2), 4=(1,3), 5=(2,3).
    //
    //                 e0=(0,1)  e1=(0,2)  e2=(0,3)  e3=(1,2)  e4=(1,3)  e5=(2,3)
    // f0=(0,1,2):       +1        -1        0         +1        0         0
    //   cycle 0→1→2→0: (0,1)+ , (1,2)+ , (0,2)−
    // f1=(0,1,3):       +1        0         -1        0         +1        0
    //   cycle 0→1→3→0: (0,1)+ , (1,3)+ , (0,3)−
    // f2=(0,2,3):       0         +1        -1        0         0         +1
    //   cycle 0→2→3→0: (0,2)+ , (2,3)+ , (0,3)−
    // f3=(1,2,3):       0         0         0         +1        -1        +1
    //   cycle 1→2→3→1: (1,2)+ , (2,3)+ , (1,3)−
    let expected: [[f64; 6]; 4] = [
        [1.0, -1.0, 0.0, 1.0, 0.0, 0.0],
        [1.0, 0.0, -1.0, 0.0, 1.0, 0.0],
        [0.0, 1.0, -1.0, 0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0, 1.0, -1.0, 1.0],
    ];

    let dense = d1.to_dense();
    for (r, row) in expected.iter().enumerate() {
        for (c, &want) in row.iter().enumerate() {
            assert_eq!(
                dense[(r, c)],
                want,
                "d¹[{r}, {c}] = {} but expected {want}",
                dense[(r, c)]
            );
        }
    }
}

/// Count boundary triangles on the cube fixture by walking every local
/// face of every tet and tallying how many tets contain it. Faces
/// touched by exactly one tet are boundary; faces touched by exactly
/// two tets are interior.
fn cube_boundary_triangle_count(mesh: &TetMesh) -> usize {
    use std::collections::HashMap;

    // Local face vertex triples (face i opposite local vertex i, but the
    // specific cyclic order doesn't matter here — only the unordered
    // vertex set).
    const LOCAL_FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

    let mut counts: HashMap<(u32, u32, u32), usize> = HashMap::new();
    for tet in &mesh.tets {
        for lf in &LOCAL_FACES {
            let mut tri = [tet[lf[0]], tet[lf[1]], tet[lf[2]]];
            tri.sort_unstable();
            *counts.entry((tri[0], tri[1], tri[2])).or_insert(0) += 1;
        }
    }

    counts.values().filter(|&&c| c == 1).count()
}

#[test]
fn cube_face_count_matches_euler_formula() {
    // On a closed-tet mesh the per-tet face count totals 4·n_tets; each
    // interior face is shared by exactly two tets while each boundary
    // face is shared by exactly one. Therefore
    //
    //     4 · n_tets = 2 · n_interior_faces + 1 · n_boundary_faces
    //                = 2 · (n_faces − n_boundary_faces) + n_boundary_faces
    //                = 2 · n_faces − n_boundary_faces,
    //
    // which rearranges to
    //
    //     n_faces = (4 · n_tets + n_boundary_faces) / 2.
    //
    // This is the "boundary-triangle correction" called out in the
    // acceptance criteria: ignoring the boundary undercounts by exactly
    // n_boundary_faces / 2.
    for n in [1usize, 2, 3] {
        let mesh = cube_tet_mesh(n, 1.0);
        let n_tets = mesh.n_tets();
        let n_faces = mesh.faces().len();
        let n_boundary = cube_boundary_triangle_count(&mesh);

        assert_eq!(
            n_faces,
            (4 * n_tets + n_boundary) / 2,
            "cube n={n}: n_faces={n_faces} disagrees with Euler-formula \
             count (4·{n_tets} + {n_boundary}) / 2 = {}",
            (4 * n_tets + n_boundary) / 2
        );

        // Sanity: the 6 outer square faces of the unit cube each carry
        // 2·n² triangles (each n×n grid square is split into 2 tris).
        assert_eq!(
            n_boundary,
            6 * 2 * n * n,
            "cube n={n}: expected 12·n² boundary triangles, got {n_boundary}"
        );
    }
}

/// Helper: convert a sparse matrix to a dense `Vec<Vec<f64>>` for
/// element-wise inspection.
fn sparse_to_dense_vec(m: &SparseColMat<usize, f64>) -> Vec<Vec<f64>> {
    let dense = m.to_dense();
    let (n_rows, n_cols) = m.shape();
    (0..n_rows)
        .map(|r| (0..n_cols).map(|c| dense[(r, c)]).collect())
        .collect()
}

#[test]
fn curl_of_gradient_is_zero_on_reference_tet() {
    // d¹ ∘ d⁰ ≡ 0 — the discrete de Rham exactness identity. On the
    // reference tet, the product is the 4×4 zero matrix bit-exactly.
    let mesh = reference_tet();
    let d0 = gradient_map(&mesh);
    let d1 = curl_map(&mesh);

    let product = &d1 * &d0;
    let dense = sparse_to_dense_vec(&product);

    assert_eq!(
        product.shape(),
        (4, 4),
        "d¹·d⁰ on the reference tet must be n_faces × n_nodes"
    );
    for (r, row) in dense.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            assert_eq!(v, 0.0, "(d¹·d⁰)[{r}, {c}] = {v}, expected 0 (bit-exact)");
        }
    }
}

#[test]
fn curl_of_gradient_is_zero_on_cube_fixture() {
    // Same identity on the 2×2×2 hex-split cube — this is the early-
    // warning sibling of the formal Phase 2.B test (issue #78). If a
    // sign in TET_LOCAL_FACE_EDGES drifts, the diagnostic surfaces here
    // rather than in #78.
    let mesh = cube_tet_mesh(2, 1.0);
    let d0 = gradient_map(&mesh);
    let d1 = curl_map(&mesh);

    let product = &d1 * &d0;
    let dense = sparse_to_dense_vec(&product);

    let (n_rows, n_cols) = product.shape();
    assert_eq!(n_rows, mesh.faces().len());
    assert_eq!(n_cols, mesh.n_nodes());

    for (r, row) in dense.iter().enumerate() {
        for (c, &v) in row.iter().enumerate() {
            assert_eq!(
                v, 0.0,
                "(d¹·d⁰)[{r}, {c}] = {v} on cube fixture, expected 0 (bit-exact)"
            );
        }
    }
}
