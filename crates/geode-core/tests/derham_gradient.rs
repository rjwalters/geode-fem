//! Tests for the discrete gradient operator `d⁰` (`geode_core::derham`).
//!
//! Covers the four acceptance checks from issue #58:
//!   1. Shape: `d⁰` is `n_edges × n_nodes`.
//!   2. Sparsity: every row has exactly two nonzeros (one `+1`, one `−1`)
//!      summing to zero.
//!   3. Single reference-tet hand check against a tabulated matrix.
//!   4. Closed-loop consistency: the signed edge-gradient sum around any
//!      triangular face of the cube fixture vanishes (precursor to the
//!      Phase 2 `d¹ · d⁰ = 0` test).

use std::collections::{BTreeMap, HashMap};

use geode_core::{TetMesh, apply_gradient, cube_tet_mesh, gradient_map};

/// A single tet on nodes 0..4. Connectivity is all that matters for `d⁰`;
/// the coordinates below form a unit reference tet for good measure.
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
fn shape_is_n_edges_by_n_nodes() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d0 = gradient_map(&mesh);
    let n_edges = mesh.edges().len();
    let n_nodes = mesh.n_nodes();

    assert_eq!(
        d0.shape(),
        (n_edges, n_nodes),
        "d⁰ must be n_edges × n_nodes"
    );
}

#[test]
fn every_row_has_two_opposite_unit_nonzeros() {
    let mesh = cube_tet_mesh(2, 1.0);
    let d0 = gradient_map(&mesh);
    let dense = d0.to_dense();

    let n_edges = mesh.edges().len();
    let n_nodes = mesh.n_nodes();

    for r in 0..n_edges {
        let mut nnz = 0usize;
        let mut row_sum = 0.0f64;
        let mut saw_plus = false;
        let mut saw_minus = false;
        for c in 0..n_nodes {
            let v = dense[(r, c)];
            if v != 0.0 {
                nnz += 1;
                row_sum += v;
                if v == 1.0 {
                    saw_plus = true;
                } else if v == -1.0 {
                    saw_minus = true;
                } else {
                    panic!("row {r}, col {c}: unexpected entry {v}, expected ±1");
                }
            }
        }
        assert_eq!(nnz, 2, "row {r} must have exactly 2 nonzeros");
        assert_eq!(row_sum, 0.0, "row {r} nonzeros must sum to zero");
        assert!(
            saw_plus && saw_minus,
            "row {r} must have exactly one +1 and one -1"
        );
    }
}

#[test]
fn single_reference_tet_matches_hand_tabulated_matrix() {
    let mesh = reference_tet();
    let d0 = gradient_map(&mesh);

    // 6 edges (TET_LOCAL_EDGES order, already lower-tag-first), 4 nodes.
    assert_eq!(d0.shape(), (6, 4));
    assert_eq!(
        mesh.edges(),
        vec![[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]],
        "edge enumeration must match the canonical lower-tag-first order"
    );

    // Hand-tabulated d⁰: rows = oriented edges, cols = nodes.
    //            n0  n1  n2  n3
    // (0,1):    -1  +1   0   0
    // (0,2):    -1   0  +1   0
    // (0,3):    -1   0   0  +1
    // (1,2):     0  -1  +1   0
    // (1,3):     0  -1   0  +1
    // (2,3):     0   0  -1  +1
    let expected: [[f64; 4]; 6] = [
        [-1.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 1.0, 0.0],
        [-1.0, 0.0, 0.0, 1.0],
        [0.0, -1.0, 1.0, 0.0],
        [0.0, -1.0, 0.0, 1.0],
        [0.0, 0.0, -1.0, 1.0],
    ];

    let dense = d0.to_dense();
    for (r, row) in expected.iter().enumerate() {
        for (c, &want) in row.iter().enumerate() {
            assert_eq!(
                dense[(r, c)],
                want,
                "d⁰[{r}, {c}] = {} but expected {want}",
                dense[(r, c)]
            );
        }
    }

    // apply_gradient must agree with the materialized operator on a sample
    // field: row (a,b) yields φ_b − φ_a.
    let phi = [10.0, 3.0, 7.0, 1.0];
    let grad = apply_gradient(&mesh, &phi);
    let expected_grad = [
        phi[1] - phi[0], // (0,1)
        phi[2] - phi[0], // (0,2)
        phi[3] - phi[0], // (0,3)
        phi[2] - phi[1], // (1,2)
        phi[3] - phi[1], // (1,3)
        phi[3] - phi[2], // (2,3)
    ];
    assert_eq!(grad, expected_grad);
}

/// Signed edge-gradient along a directed leg `u → v`.
///
/// `grad` holds the lower-tag-first edge differences (`φ_hi − φ_lo`), so a
/// traversal in the canonical direction takes the value as-is and a reverse
/// traversal negates it. `idx` maps a canonical edge `(lo, hi)` to its row.
fn directed_leg(u: u32, v: u32, idx: &HashMap<(u32, u32), usize>, grad: &[f64]) -> f64 {
    if u < v {
        grad[idx[&(u, v)]]
    } else {
        -grad[idx[&(v, u)]]
    }
}

#[test]
fn closed_loop_signed_sum_vanishes_on_cube_faces() {
    let mesh = cube_tet_mesh(2, 1.0);

    // Arbitrary nodal field: a deterministic non-affine scramble so the
    // cancellation is structural, not an artifact of a linear field.
    let phi: Vec<f64> = (0..mesh.n_nodes())
        .map(|i| {
            let x = i as f64;
            (x * 0.7).sin() * 3.0 + x * x * 0.013 - (x * 1.9).cos()
        })
        .collect();

    let grad = apply_gradient(&mesh, &phi);

    // Edge → row-index lookup (canonical lower-tag-first edges).
    let edges = mesh.edges();
    let idx: HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, &[a, b])| ((a, b), i))
        .collect();

    // The four triangular faces of a tet [v0, v1, v2, v3].
    let faces = |t: &[u32; 4]| {
        [
            [t[0], t[1], t[2]],
            [t[0], t[1], t[3]],
            [t[0], t[2], t[3]],
            [t[1], t[2], t[3]],
        ]
    };

    for tet in &mesh.tets {
        for [a, b, c] in faces(tet) {
            // Walk the directed cycle a → b → c → a.
            let loop_sum = directed_leg(a, b, &idx, &grad)
                + directed_leg(b, c, &idx, &grad)
                + directed_leg(c, a, &idx, &grad);
            assert!(
                loop_sum.abs() < 1e-12,
                "signed edge-gradient sum around face ({a},{b},{c}) = {loop_sum}, expected 0"
            );
        }
    }
}
