//! Regression gate for issue #771: the PEC edge masks are **face-exact**.
//!
//! An edge DOF is pinned by `n × E = 0` iff the edge *lies in* the PEC wall,
//! i.e. it is an edge of a wall triangle. The pre-#771 helpers used the
//! node rule "both endpoints on the boundary", which also pins every
//! interior **chord** whose two endpoints sit on *different* wall faces —
//! `12(n − 1)` edges on `cube_tet_mesh(n, ..)` (one per cell along each of
//! the 12 cube edges), the body diagonal at `n = 1`, and the `2·n_theta`
//! cap-to-side-wall rim chords on the cylinder fixtures.
//!
//! Each case checks the helper against the boundary-face edge set
//! (`TetMesh::boundary_faces`): **zero** wrongly eliminated, **zero**
//! missed. The old node rule is re-derived inline (not via the deprecated
//! helper) to pin how many chords it used to over-eliminate, so a silent
//! regression to it fails loudly. The sphere helper's mask is asserted
//! bit-identical to the old rule on both bundled sphere fixtures — the
//! cross-language sphere reference sidecars depend on that.
//!
//! ```sh
//! cargo test -p geode-core --test pec_mask_face_exact
//! ```

use std::collections::HashSet;

use geode_core::assembly::nedelec::{
    boundary_pec_interior_edges, cube_pec_interior_edges, sphere_pec_interior_edges,
};
use geode_core::mesh::magnetostatic_fixtures::{
    cylinder_pec_interior_mask, loop_pair_mesh, solid_coax_mesh,
};
use geode_core::mesh::{
    R_BUFFER, TetMesh, cube_tet_mesh, read_sphere_fine_fixture, read_sphere_fixture,
};

/// Edges (sorted pairs) of the given triangles.
fn tri_edges(tris: &[[u32; 3]]) -> HashSet<[u32; 2]> {
    let mut out = HashSet::new();
    for t in tris {
        // Triangles are sorted triples, so each pair is canonical (lo, hi).
        out.insert([t[0], t[1]]);
        out.insert([t[0], t[2]]);
        out.insert([t[1], t[2]]);
    }
    out
}

/// `(wrongly_eliminated, missed)` of `mask` against the edge set of the
/// boundary faces selected by `on_wall`.
fn audit(
    mesh: &TetMesh,
    edges: &[[u32; 2]],
    mask: &[bool],
    on_wall: impl Fn(&[u32; 3]) -> bool,
) -> (usize, usize) {
    let walls: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| on_wall(f))
        .collect();
    let wall_edges = tri_edges(&walls);
    let wrong = edges
        .iter()
        .zip(mask)
        .filter(|(e, keep)| !**keep && !wall_edges.contains(*e))
        .count();
    let missed = edges
        .iter()
        .zip(mask)
        .filter(|(e, keep)| **keep && wall_edges.contains(*e))
        .count();
    (wrong, missed)
}

/// The pre-#771 node rule: eliminate iff both endpoints satisfy `on_bdry`.
fn node_rule(mesh: &TetMesh, edges: &[[u32; 2]], on_bdry: impl Fn([f64; 3]) -> bool) -> Vec<bool> {
    let flag: Vec<bool> = mesh.nodes.iter().map(|&p| on_bdry(p)).collect();
    edges
        .iter()
        .map(|e| !(flag[e[0] as usize] && flag[e[1] as usize]))
        .collect()
}

fn n_pinned(mask: &[bool]) -> usize {
    mask.iter().filter(|&&k| !k).count()
}

#[test]
fn boundary_faces_are_the_faces_of_exactly_one_tet() {
    // Two tets sharing face [1, 2, 3]: 8 faces, 6 on the boundary.
    let mesh = TetMesh {
        nodes: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
        ],
        tets: vec![[0, 1, 2, 3], [3, 2, 1, 4]],
        ..Default::default()
    };
    let b = mesh.boundary_faces();
    assert_eq!(
        b,
        vec![
            [0, 1, 2],
            [0, 1, 3],
            [0, 2, 3],
            [1, 2, 4],
            [1, 3, 4],
            [2, 3, 4]
        ],
        "sorted triples, ascending, shared face excluded"
    );

    // The cube's surface: 6 walls × n² squares × 2 triangles.
    for n in [1usize, 2, 3, 5] {
        let mesh = cube_tet_mesh(n, 1.0);
        let b = mesh.boundary_faces();
        assert_eq!(b.len(), 12 * n * n, "n={n} cube boundary face count");
        // Every boundary face lies in one wall plane.
        for f in &b {
            let p = f.map(|v| mesh.nodes[v as usize]);
            let in_plane = (0..3).any(|d| {
                [0.0, 1.0]
                    .iter()
                    .any(|&w| p.iter().all(|q| (q[d] - w).abs() < 1e-12))
            });
            assert!(in_plane, "n={n}: boundary face {f:?} not on a cube wall");
        }
    }
}

#[test]
fn cube_mask_eliminates_no_interior_chord() {
    let tol = 1e-9;
    let on_cube = |p: [f64; 3]| p.iter().any(|&c| c.abs() < tol || (c - 1.0).abs() < tol);
    for n in [1usize, 2, 4, 8] {
        let mesh = cube_tet_mesh(n, 1.0);
        let (edges, mask) = cube_pec_interior_edges(&mesh, 1.0);
        let (wrong, missed) = audit(&mesh, &edges, &mask, |_| true);
        assert_eq!(wrong, 0, "n={n}: interior chords wrongly eliminated");
        assert_eq!(missed, 0, "n={n}: wall edges left free");

        // The wall carries exactly the surface-triangulation edges:
        // 6 walls × (3n² + 2n) edges, minus the 12 n cube-edge segments
        // counted twice.
        assert_eq!(
            n_pinned(&mask),
            6 * (3 * n * n + 2 * n) - 12 * n,
            "n={n} pinned edge count"
        );

        // The old node rule over-eliminated 12(n − 1) chords (n ≥ 2) and
        // the body diagonal at n = 1.
        let old = node_rule(&mesh, &edges, on_cube);
        let expected_bias = if n == 1 { 1 } else { 12 * (n - 1) };
        assert_eq!(
            n_pinned(&old) - n_pinned(&mask),
            expected_bias,
            "n={n}: node-rule over-elimination count"
        );
        assert!(
            old.iter().zip(&mask).all(|(o, m)| !*o || *m),
            "n={n}: face-exact mask must keep every edge the node rule kept"
        );
    }
}

#[test]
fn cylinder_masks_eliminate_no_rim_chord() {
    // Solid-coax fixture of the L' ≤ 1% oracle.
    let (a, b, length, nth) = (1.0_f64, 3.0_f64, 1.0_f64, 64usize);
    let fx = solid_coax_mesh(a, b, length, nth, 32, 3);
    let (edges, mask) = cylinder_pec_interior_mask(&fx.mesh, b, length);
    let (wrong, missed) = audit(&fx.mesh, &edges, &mask, |_| true);
    assert_eq!(wrong, 0, "coax: interior rim chords wrongly eliminated");
    assert_eq!(missed, 0, "coax: wall edges left free");
    let on_cyl = |r_out: f64, len: f64| {
        move |p: [f64; 3]| {
            (p[0].hypot(p[1]) - r_out).abs() < 1e-6 * r_out
                || p[2].abs() < 1e-6 * len
                || (p[2] - len).abs() < 1e-6 * len
        }
    };
    let old = node_rule(&fx.mesh, &edges, on_cyl(b, length));
    assert_eq!(
        n_pinned(&old) - n_pinned(&mask),
        2 * nth,
        "coax: node rule over-eliminated one rim chord per azimuthal cell per cap"
    );

    // Loop-pair fixture of the Maxwell-mutual oracle (moderate in-suite
    // resolution): the PEC shield + caps are again the whole outer surface.
    let (rbox, llen, nth) = (5.0_f64, 5.0_f64, 24usize);
    let lp = loop_pair_mesh(1.0, 2.0, 1.0, 3.0, rbox, llen, nth, 12, 12);
    let (edges, mask) = cylinder_pec_interior_mask(&lp.mesh, rbox, llen);
    let (wrong, missed) = audit(&lp.mesh, &edges, &mask, |_| true);
    assert_eq!(
        wrong, 0,
        "loop pair: interior rim chords wrongly eliminated"
    );
    assert_eq!(missed, 0, "loop pair: wall edges left free");
    let old = node_rule(&lp.mesh, &edges, on_cyl(rbox, llen));
    assert_eq!(
        n_pinned(&old) - n_pinned(&mask),
        2 * nth,
        "loop pair: node-rule over-elimination count"
    );
}

#[test]
fn face_filter_selects_a_subset_of_walls() {
    // Only the z = 0 wall is PEC: exactly the edges of its 2n² triangles
    // ((n+1)·n·2 axis-aligned + n² diagonals) are pinned.
    let n = 3;
    let mesh = cube_tet_mesh(n, 1.0);
    let (edges, mask) =
        boundary_pec_interior_edges(&mesh, |tri| tri.iter().all(|p| p[2].abs() < 1e-12));
    assert_eq!(n_pinned(&mask), 2 * n * (n + 1) + n * n);
    for (e, &keep) in edges.iter().zip(&mask) {
        let on_floor = e.iter().all(|&v| mesh.nodes[v as usize][2].abs() < 1e-12);
        assert_eq!(keep, !on_floor, "edge {e:?}");
    }
}

#[test]
fn sphere_mask_is_bit_identical_to_the_node_rule_on_the_bundled_fixtures() {
    // The Mie sphere goldens and the cross-language sphere sidecars were
    // generated with the node rule. On these fixtures no interior chord
    // joins two outer-wall nodes, so the face-exact mask must not move.
    let on_sphere = |p: [f64; 3]| {
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        (r - R_BUFFER).abs() < 1e-6 * R_BUFFER.max(1.0)
    };
    for (name, fx) in [
        ("sphere.msh", read_sphere_fixture().expect("sphere fixture")),
        (
            "sphere_fine.msh",
            read_sphere_fine_fixture().expect("sphere_fine fixture"),
        ),
    ] {
        let (edges, mask) = sphere_pec_interior_edges(&fx.mesh, R_BUFFER);
        let old = node_rule(&fx.mesh, &edges, on_sphere);
        assert_eq!(mask, old, "{name}: sphere mask moved");
        let (wrong, missed) = audit(&fx.mesh, &edges, &mask, |_| true);
        assert_eq!((wrong, missed), (0, 0), "{name}: not face-exact");
        assert!(n_pinned(&mask) > 0, "{name}: no wall edges pinned");
    }
}
