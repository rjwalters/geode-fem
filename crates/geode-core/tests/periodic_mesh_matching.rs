//! Golden 1 (matching suite) and golden 2 (orientation coverage) of issue
//! #839 (Epic #837 Phase 1): periodic-mesh matching on tet meshes.
//!
//! - (a) exact x / y / z pairs of `cube_tet_mesh` match with no warnings;
//! - (b) slave jitter of `1e-9·L` is snapped silently, `0.05·h` is snapped
//!   **with** a warning (`n_snapped`, `max_snap_displacement`), and the mesh
//!   is exactly periodic afterwards;
//! - (c) a non-conforming pair (crossed diagonals; and nodes beyond the
//!   snap limit) is `NonConforming` with the right counts and the fix hint;
//! - (d) translation inference reproduces the true translation, and
//!   non-translate faces are rejected;
//! - (e) corner chaining on the triply periodic cube: every corner node
//!   aliases to one master with the right lattice vector `Σd`.
//!
//! Golden 2: lexicographic numbering never reverses an orientation; a random
//! renumbering does, and the edge sign then matches the geometry.

#[path = "common/periodic_fixtures.rs"]
mod fixtures;

use burn::tensor::backend::BackendTypes;
use faer::c64;

use geode_core::assembly::hcurl_space::HcurlSpace;
use geode_core::assembly::periodic::{DofAlias, PeriodicConstraint, PeriodicError};
use geode_core::driven::periodic::PeriodicDrivenOperator;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenMaterials, DrivenOperator, DrivenSource,
};
use geode_core::elements::ElementOrder;
use geode_core::mesh::cube_tet_mesh;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchError, PeriodicMatchOptions, PeriodicPair, PeriodicTransform,
    box_periodic_pairs,
};
use geode_core::testing::TestBackend;

use fixtures::{cube_tet_mesh_5, jitter_in_plane, renumber};

type B = TestBackend;

fn opts() -> PeriodicMatchOptions {
    PeriodicMatchOptions::default()
}

/// (a) Exact x / y / z pairs, no warnings, the expected counts.
#[test]
fn exact_cube_pairs_match_without_warnings() {
    let n = 3;
    let mut mesh = cube_tet_mesh(n, 2.0);
    let before = mesh.clone();
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).expect("exact match");
    assert_eq!(mesh, before, "an exact match must not move any node");
    let r = map.report();
    assert!(r.warnings().is_empty(), "warnings: {:?}", r.warnings());
    assert_eq!(r.n_snapped(), 0);
    assert_eq!(r.max_applied_displacement, 0.0);
    for p in &r.pairs {
        assert_eq!(p.n_nodes, (n + 1) * (n + 1));
        assert_eq!(p.n_faces, 2 * n * n);
        // Per face: n(n+1) edges in each of two directions + n² diagonals.
        assert_eq!(p.n_edges, 2 * n * (n + 1) + n * n);
        assert_eq!(p.n_snapped_silent, 0);
        assert_eq!(p.n_orientation_reversed, 0, "lexicographic numbering");
    }
}

/// (b) Jitter: `1e-9·L` silently, `0.05·h` with a warning.
#[test]
fn jittered_slave_nodes_are_snapped() {
    let (n, side) = (4, 1.0);
    let h = side / n as f64;
    for (amount, warned) in [(1e-9 * side, false), (0.05 * h, true)] {
        let reference = cube_tet_mesh(n, side);
        let mut mesh = reference.clone();
        let moved = jitter_in_plane(&mut mesh, 0, amount, 7, |p| (p[0] - side).abs() < 1e-12);
        assert_eq!(moved, (n - 1) * (n - 1), "interior x = L face nodes");
        let pairs = box_periodic_pairs(&mesh, &[0]);
        let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).expect("snappable");
        let r = &map.report().pairs[0];
        if warned {
            assert_eq!(r.n_snapped, moved);
            assert!(
                (r.max_snap_displacement - amount).abs() < 1e-9 * side,
                "max displacement {} vs jitter {amount}",
                r.max_snap_displacement
            );
            let w = map.report().warnings();
            assert_eq!(w.len(), 1);
            assert!(
                w[0].contains("snapped") && w[0].contains("Periodic Surface"),
                "{w:?}"
            );
        } else {
            assert_eq!(r.n_snapped, 0);
            assert_eq!(r.n_snapped_silent, moved);
            assert!(map.report().warnings().is_empty());
        }
        // Either way the mesh is now exactly periodic: the slave nodes sit
        // on the translated master images (the reference coordinates).
        for (p, q) in mesh.nodes.iter().zip(&reference.nodes) {
            for d in 0..3 {
                assert!((p[d] - q[d]).abs() < 1e-14, "{p:?} vs {q:?}");
            }
        }
    }
}

/// (c) Non-conforming pairs fail loudly with counts and the fix hint.
#[test]
fn non_conforming_pairs_are_rejected_with_counts() {
    // Crossed diagonals: the 5-tet split with n odd. Every node matches,
    // no triangle does.
    let n = 3;
    let mut mesh = cube_tet_mesh_5(n, 1.0);
    let before = mesh.clone();
    let pairs = box_periodic_pairs(&mesh, &[0]);
    let err = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap_err();
    assert_eq!(mesh, before, "a failed match must not modify the mesh");
    match &err {
        PeriodicMatchError::NonConforming {
            pair,
            n_unmatched_nodes,
            n_unmatched_faces,
            n_master_faces,
            n_slave_faces,
            max_nearest_image_distance,
            ..
        } => {
            assert_eq!(*pair, 0);
            assert_eq!(*n_unmatched_nodes, 0);
            assert_eq!((*n_master_faces, *n_slave_faces), (2 * n * n, 2 * n * n));
            // Every master and every slave triangle lacks a partner.
            assert_eq!(*n_unmatched_faces, 4 * n * n);
            assert!(*max_nearest_image_distance < 1e-12);
        }
        other => panic!("expected NonConforming, got {other:?}"),
    }
    let msg = err.to_string();
    for needle in [
        "Periodic Surface",
        "geode mesh --periodic",
        "do not conform",
    ] {
        assert!(msg.contains(needle), "missing `{needle}` in: {msg}");
    }

    // Nodes beyond the snap limit (0.4 h > 0.25 h).
    let (n, side) = (4, 1.0);
    let h = side / n as f64;
    let mut mesh = cube_tet_mesh(n, side);
    let moved = jitter_in_plane(&mut mesh, 0, 0.4 * h, 3, |p| (p[0] - side).abs() < 1e-12);
    let pairs = box_periodic_pairs(&mesh, &[0]);
    match PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap_err() {
        PeriodicMatchError::NonConforming {
            n_unmatched_nodes,
            max_nearest_image_distance,
            max_snap,
            ..
        } => {
            // Each moved slave node is unmatched on both sides.
            assert_eq!(n_unmatched_nodes, 2 * moved);
            assert!((max_snap - 0.25 * h).abs() < 1e-12);
            assert!(
                max_nearest_image_distance > max_snap,
                "{max_nearest_image_distance} vs {max_snap}"
            );
        }
        other => panic!("expected NonConforming, got {other:?}"),
    }
}

/// (d) Translation inference, and rejection of non-translates.
#[test]
fn translation_inference_and_non_translates() {
    let side = 1.5;
    let mut mesh = cube_tet_mesh(3, side);
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let inferred = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap();
    for (axis, p) in inferred.report().pairs.iter().enumerate() {
        assert!(p.translation_inferred);
        for d in 0..3 {
            let want = if d == axis { side } else { 0.0 };
            assert!(
                (p.translation[d] - want).abs() < 1e-12,
                "{:?}",
                p.translation
            );
        }
    }
    // Given translations reproduce the same map.
    let given: Vec<PeriodicPair> = pairs
        .iter()
        .enumerate()
        .map(|(axis, p)| {
            let mut d = [0.0; 3];
            d[axis] = side;
            PeriodicPair {
                transform: Some(PeriodicTransform::Translation(d)),
                ..p.clone()
            }
        })
        .collect();
    let explicit = PeriodicMap::build(&mut mesh, &given, &opts()).unwrap();
    for e in 0..mesh.edges().len() {
        assert_eq!(inferred.edge_alias(e), explicit.edge_alias(e));
    }
    // A wrong given translation is non-conforming.
    let mut wrong = given[0].clone();
    wrong.transform = Some(PeriodicTransform::Translation([side, 0.5 * side, 0.0]));
    assert!(matches!(
        PeriodicMap::build(&mut mesh, &[wrong], &opts()),
        Err(PeriodicMatchError::NonConforming { .. })
    ));
    // Faces that are not translates: x = 0 against y = side.
    let bad = PeriodicPair {
        master: pairs[0].master.clone(),
        slave: pairs[1].slave.clone(),
        transform: None,
    };
    let err = PeriodicMap::build(&mut mesh, &[bad], &opts()).unwrap_err();
    assert!(
        matches!(err, PeriodicMatchError::NotTranslates { pair: 0, .. }),
        "{err:?}"
    );
    assert!(err.to_string().contains("not translates"));
    // Malformed input is a typed error, not a panic.
    let interior = PeriodicPair {
        master: vec![[0, 1, 2]],
        slave: pairs[0].slave.clone(),
        transform: None,
    };
    assert!(matches!(
        PeriodicMap::build(&mut mesh, &[interior], &opts()),
        Err(PeriodicMatchError::InvalidPair { .. })
    ));
    let selfpair = PeriodicPair {
        master: pairs[0].master.clone(),
        slave: pairs[0].master.clone(),
        transform: None,
    };
    assert!(matches!(
        PeriodicMap::build(&mut mesh, &[selfpair], &opts()),
        Err(PeriodicMatchError::InvalidPair { .. })
    ));
}

/// (e) Corner chaining on the 3-torus.
#[test]
fn corner_chaining_on_the_three_torus() {
    let (n, side) = (2, 1.0);
    let mut mesh = renumber(&cube_tet_mesh(n, side), 11);
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap();
    let corners: Vec<usize> = (0..mesh.n_nodes())
        .filter(|&i| {
            mesh.nodes[i]
                .iter()
                .all(|&x| x.abs() < 1e-12 || (x - side).abs() < 1e-12)
        })
        .collect();
    assert_eq!(corners.len(), 8);
    let master = map.node_alias(corners[0]).unwrap().master;
    assert!(
        mesh.nodes[master].iter().all(|&x| x.abs() < 1e-12),
        "master at the origin"
    );
    for &c in &corners {
        let a = map.node_alias(c).unwrap();
        assert_eq!(a.master, master, "all 8 corners alias one master");
        for d in 0..3 {
            assert!(
                (mesh.nodes[c][d] - mesh.nodes[master][d] - a.shift[d]).abs() < 1e-12,
                "corner {c}: Σd {:?}",
                a.shift
            );
        }
    }
    // Every node and edge class is consistent: alias position = master + Σd.
    let edges = mesh.edges();
    let mid = |e: [u32; 2]| -> [f64; 3] {
        let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
        std::array::from_fn(|d| 0.5 * (p[d] + q[d]))
    };
    let mut class_size = std::collections::HashMap::new();
    for (e, ab) in edges.iter().enumerate() {
        if let Some(a) = map.edge_alias(e) {
            *class_size.entry(a.master).or_insert(0usize) += 1;
            let (pe, pm) = (mid(*ab), mid(edges[a.master]));
            for d in 0..3 {
                assert!((pe[d] - pm[d] - a.shift[d]).abs() < 1e-12);
            }
        }
    }
    // Box edges: 12 → 3 classes of 4 copies per segment (n segments each).
    assert_eq!(class_size.values().filter(|&&s| s == 4).count(), 3 * n);
    // Corner nodes: one class of 8; box-edge (non-corner) nodes: classes of 4.
    assert_eq!(map.report().n_chained_nodes, 8 + 12 * (n - 1));
}

/// Golden 2: a random renumbering reverses orientations, and the stored
/// sign matches the geometry (the slave edge runs parallel to the master
/// edge iff σ = +1).
#[test]
fn renumbering_exercises_reversed_orientations() {
    let side = 1.0;
    let lex = {
        let mut m = cube_tet_mesh(3, side);
        let pairs = box_periodic_pairs(&m, &[0, 1, 2]);
        PeriodicMap::build(&mut m, &pairs, &opts()).unwrap()
    };
    assert_eq!(lex.report().n_orientation_reversed(), 0);

    let mut mesh = renumber(&cube_tet_mesh(3, side), 2024);
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap();
    let n_rev = map.report().n_orientation_reversed();
    assert!(n_rev > 0, "renumbering must reverse some edge pairs");
    let edges = mesh.edges();
    let dir = |e: [u32; 2]| -> [f64; 3] {
        let (p, q) = (mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]);
        std::array::from_fn(|d| q[d] - p[d])
    };
    let mut n_neg = 0;
    for (e, ab) in edges.iter().enumerate() {
        if let Some(a) = map.edge_alias(e) {
            let (u, v) = (dir(*ab), dir(edges[a.master]));
            let dot: f64 = (0..3).map(|d| u[d] * v[d]).sum();
            assert!(dot.abs() > 1e-12);
            assert_eq!(a.sign, if dot > 0.0 { 1 } else { -1 }, "edge {e}");
            if a.sign < 0 {
                n_neg += 1;
            }
        }
    }
    assert!(n_neg > 0);
    eprintln!(
        "renumbered 3-torus n=3: {n_rev} reversed pair orientations, {n_neg} negative aliases"
    );
}

/// p=2 + periodic is a typed `Unsupported` error (constraint and driven
/// operator), never a silently wrong p=1 map applied to p=2 DOFs.
#[test]
fn p2_periodic_is_unsupported() {
    let mut mesh = cube_tet_mesh(2, 1.0);
    let pairs = box_periodic_pairs(&mesh, &[0]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap();
    let p2 = HcurlSpace::build(&mesh, ElementOrder::P2);
    assert!(matches!(
        PeriodicConstraint::build(&p2, &map, None),
        Err(PeriodicError::Unsupported { .. })
    ));

    // A p=2 driven operator under a (p=1) constraint is rejected as well.
    let p1 = HcurlSpace::build(&mesh, ElementOrder::P1);
    let c = PeriodicConstraint::build(&p1, &map, None).unwrap();
    let mask = p2.pec_interior_mask(&mesh, &[]).unwrap();
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let src = CurrentSource::from_centroids(&mesh, |_| [c64::new(0.0, 0.0); 3]);
    let device = <B as BackendTypes>::Device::default();
    let op = DrivenOperator::assemble_with_space::<B>(
        &p2,
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        DrivenSource::Constant(&src),
        &device,
    )
    .unwrap();
    let err = PeriodicDrivenOperator::new(op, &c)
        .err()
        .expect("p=2 must be rejected");
    assert!(matches!(err, PeriodicError::Unsupported { .. }), "{err:?}");
}

/// One-sided PEC (a master edge PEC, its slave not): both copies are
/// eliminated and a warning is recorded, not an error.
#[test]
fn one_sided_pec_eliminates_both_copies_with_a_warning() {
    let mut mesh = cube_tet_mesh(2, 1.0);
    let pairs = box_periodic_pairs(&mesh, &[0]);
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).unwrap();
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    // PEC on one master-face triangle only.
    let wall = [pairs[0].master[0]];
    let mask = space.pec_interior_mask(&mesh, &[&wall]).unwrap();
    let c = PeriodicConstraint::build(&space, &map, Some(&mask)).unwrap();
    assert_eq!(c.report().n_one_sided_pec, 3, "the triangle's three edges");
    assert_eq!(c.warnings().len(), 1);
    for (e, &keep) in mask.iter().enumerate() {
        if !keep {
            let a = map.edge_alias(e).unwrap();
            for f in 0..mask.len() {
                if map.edge_alias(f).is_some_and(|b| b.master == a.master) {
                    assert_eq!(c.alias(f), DofAlias::Eliminated, "copy {f} of PEC edge {e}");
                }
            }
        }
    }
}
