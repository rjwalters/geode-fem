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
//!
//! Issue #856 (least-squares refit of an inferred translation), on the #855
//! Judge's probe, a renumbered `6³` 3-torus with **every** node of the three
//! hi faces (edges and corners included) jittered in all directions by up to
//! `0.04 h`, `transform: None`:
//!
//! - (f) the refitted translation is exactly the least-squares one: the true
//!   translation plus the mean jitter offset over the matched pairs (an
//!   identity, to round-off), and the warning names it and its residual;
//! - (g) with jitter whose mean offset over each pair is zero, the inferred
//!   translation is exact, the snap restores the cube, and the periodic
//!   spectrum (lowest 14 modes, degeneracies included) equals the
//!   exact-translation spectrum to round-off. The bounding-box centroid
//!   translation (the pre-#856 inference) shifts the same spectrum by
//!   `~10⁻³` and splits its degeneracies;
//! - (g') on the literal probe (generic jitter, non-zero mean offset) the
//!   translation is not identifiable from the mesh, so no estimator is exact;
//!   least squares cuts the spectrum error about sixfold;
//! - (h) the #862 Judge's probe: on a `61²`–`101²` face with every slave
//!   node jittered (in-plane or 3D, `transform: None`) the fit uses every
//!   matched pair, not a few pairs that agree by coincidence, and the
//!   translation is the all-pairs least-squares one.
//!
//! Measured (release): (f) LS identity to `2e-16`, LS translation error
//! `3.3–3.9e-4` vs bounding box `1.2–3.8e-3`; (g) snapped mesh vs the exact
//! cube `1.1e-16`, spectrum `9.5e-16` (given translation `2.6e-15`), every
//! degeneracy kept exactly (two 6-fold clusters at 37.395 and 39.135), bounding
//! box `3.6e-3` with splits of `3.9e-3`; (g') spectrum error `8.8e-4` (LS) vs
//! `5.4e-3` (bounding box).

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
use geode_core::eigen::pec_cavity::{PecCavityMaterials, PecCavitySettings};
use geode_core::eigen::periodic_cavity::solve_periodic_cavity_modes;
use geode_core::elements::ElementOrder;
use geode_core::mesh::periodic::{
    PeriodicMap, PeriodicMatchError, PeriodicMatchOptions, PeriodicPair, PeriodicTransform,
    box_periodic_pairs,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

use fixtures::{Rng, box_tet_mesh, cube_tet_mesh_5, jitter_in_plane, renumber};

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
        // The exact edges and corners (16 of 25 pairs) are an exactly
        // periodic subset: the translation is exact, bit for bit (#856).
        assert_eq!(r.translation, [side, 0.0, 0.0]);
        assert_eq!(r.translation_fit_pairs, (n + 1) * (n + 1) - moved);
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

// ---------------------------------------------------------------------------
// Issue #856: least-squares refit of an inferred translation
// ---------------------------------------------------------------------------

/// Cells per axis of the #855 Judge's 3-torus probe.
const TORUS_N: usize = 6;

/// The renumbered unit `6³` 3-torus and its three box pairs (computed on the
/// unjittered mesh: the triangles are by index, so jittering afterwards
/// keeps them).
fn judge_torus() -> (TetMesh, Vec<PeriodicPair>) {
    let mesh = renumber(&box_tet_mesh([TORUS_N; 3], [1.0; 3]), 856);
    let pairs = box_periodic_pairs(&mesh, &[0, 1, 2]);
    (mesh, pairs)
}

/// Lattice coordinates `round(x · n)` of every node of the unjittered torus.
fn lattice(mesh: &TetMesh) -> Vec<[usize; 3]> {
    mesh.nodes
        .iter()
        .map(|p| std::array::from_fn(|d| (p[d] * TORUS_N as f64).round() as usize))
        .collect()
}

/// Per-node jitter (zero off the hi faces): every node with some lattice
/// coordinate `= n`, edges and corners included, moves by a random vector of
/// length up to `amp`. With `mean_zero`, the 25 face-interior nodes of each
/// hi face (which belong to that pair only) absorb the pair's mean offset,
/// so `Σ (J_slave − J_master) = 0` over every pair's matched nodes; the field
/// is then rescaled to max length `amp`.
fn hi_face_jitter(mesh: &TetMesh, amp: f64, seed: u64, mean_zero: bool) -> Vec<[f64; 3]> {
    let lat = lattice(mesh);
    let n = TORUS_N;
    let mut rng = Rng::new(seed);
    let mut jit: Vec<[f64; 3]> = lat
        .iter()
        .map(|l| {
            if l.contains(&n) {
                let r: [f64; 3] = std::array::from_fn(|_| 2.0 * rng.uniform() - 1.0);
                let s = amp * rng.uniform() / (r.iter().map(|v| v * v).sum::<f64>().sqrt());
                r.map(|v| v * s)
            } else {
                [0.0; 3]
            }
        })
        .collect();
    if mean_zero {
        for a in 0..3 {
            let s = pair_offset_sum(&lat, &jit, a);
            let interior: Vec<usize> = (0..lat.len())
                .filter(|&i| {
                    lat[i][a] == n && (0..3).all(|d| d == a || (1..n).contains(&lat[i][d]))
                })
                .collect();
            assert_eq!(interior.len(), (n - 1) * (n - 1));
            for &i in &interior {
                for d in 0..3 {
                    jit[i][d] -= s[d] / interior.len() as f64;
                }
            }
        }
        let max = jit
            .iter()
            .map(|v| v.iter().map(|c| c * c).sum::<f64>().sqrt())
            .fold(0.0, f64::max);
        for v in &mut jit {
            for c in v.iter_mut() {
                *c *= amp / max;
            }
        }
        for a in 0..3 {
            let s = pair_offset_sum(&lat, &jit, a);
            assert!(s.iter().all(|c| c.abs() < 1e-15), "pair {a}: {s:?}");
        }
    }
    jit
}

/// `Σ (J_slave − J_master)` over the matched node pairs of the axis-`a` pair
/// (master lattice coordinate `0`, slave `n`).
fn pair_offset_sum(lat: &[[usize; 3]], jit: &[[f64; 3]], a: usize) -> [f64; 3] {
    let by_lat: std::collections::HashMap<[usize; 3], usize> =
        lat.iter().enumerate().map(|(i, l)| (*l, i)).collect();
    let mut s = [0.0; 3];
    for (i, l) in lat.iter().enumerate() {
        if l[a] != 0 {
            continue;
        }
        let mut ls = *l;
        ls[a] = TORUS_N;
        let j = by_lat[&ls];
        for d in 0..3 {
            s[d] += jit[j][d] - jit[i][d];
        }
    }
    s
}

fn apply_jitter(mesh: &mut TetMesh, jit: &[[f64; 3]]) {
    for (p, j) in mesh.nodes.iter_mut().zip(jit) {
        for d in 0..3 {
            p[d] += j[d];
        }
    }
}

/// The pre-#856 inference: master → slave bounding-box centroid offset.
fn bbox_translation(mesh: &TetMesh, pair: &PeriodicPair) -> [f64; 3] {
    let centre = |tris: &[[u32; 3]]| -> [f64; 3] {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for &v in tris.iter().flatten() {
            for d in 0..3 {
                lo[d] = lo[d].min(mesh.nodes[v as usize][d]);
                hi[d] = hi[d].max(mesh.nodes[v as usize][d]);
            }
        }
        std::array::from_fn(|d| 0.5 * (lo[d] + hi[d]))
    };
    let (m, s) = (centre(&pair.master), centre(&pair.slave));
    std::array::from_fn(|d| s[d] - m[d])
}

fn with_translations(pairs: &[PeriodicPair], d: &[[f64; 3]]) -> Vec<PeriodicPair> {
    pairs
        .iter()
        .zip(d)
        .map(|(p, &t)| PeriodicPair {
            transform: Some(PeriodicTransform::Translation(t)),
            ..p.clone()
        })
        .collect()
}

fn unit(a: usize) -> [f64; 3] {
    let mut d = [0.0; 3];
    d[a] = 1.0;
    d
}

/// (f) The refitted translation is the least-squares one, to round-off:
/// `d = e_a + mean(J_slave − J_master)` over the 49 matched pairs. The
/// warning reports it with its fit size and residual.
#[test]
fn inferred_translation_is_the_least_squares_fit() {
    let (base, pairs) = judge_torus();
    let h = 1.0 / TORUS_N as f64;
    let jit = hi_face_jitter(&base, 0.04 * h, 7, false);
    let lat = lattice(&base);
    let mut mesh = base.clone();
    apply_jitter(&mut mesh, &jit);
    let bbox: Vec<[f64; 3]> = pairs.iter().map(|p| bbox_translation(&mesh, p)).collect();
    let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).expect("snappable");
    let r = map.report();
    let n_pairs = (TORUS_N + 1) * (TORUS_N + 1);
    for (a, p) in r.pairs.iter().enumerate() {
        let s = pair_offset_sum(&lat, &jit, a);
        let want: [f64; 3] = std::array::from_fn(|d| unit(a)[d] + s[d] / n_pairs as f64);
        let err = (0..3)
            .map(|d| (p.translation[d] - want[d]).abs())
            .fold(0.0, f64::max);
        let bbox_err = (0..3)
            .map(|d| (bbox[a][d] - unit(a)[d]).abs())
            .fold(0.0, f64::max);
        let ls_err = (0..3)
            .map(|d| (p.translation[d] - unit(a)[d]).abs())
            .fold(0.0, f64::max);
        eprintln!(
            "pair {a}: LS d = {:?} (vs LS identity {err:.1e}; error vs exact {ls_err:.3e}), \
             bbox d error {bbox_err:.3e}, residual {:.3e}, {} snapped",
            p.translation, p.translation_residual, p.n_snapped
        );
        assert!(p.translation_inferred);
        assert_eq!(
            p.translation_fit_pairs, n_pairs,
            "fully jittered: fit to every pair"
        );
        assert!(err < 1e-15, "pair {a}: LS identity off by {err:e}");
        assert!(p.translation_residual > 0.0);
        assert!(p.n_snapped > 0);
    }
    let w = r.warnings();
    assert_eq!(w.len(), 3, "{w:?}");
    for (a, msg) in w.iter().enumerate() {
        let d = r.pairs[a].translation;
        assert!(
            msg.contains("inferred")
                && msg.contains("least squares over 49 node pairs")
                && msg.contains("RMS residual")
                && msg.contains(&format!("{:.9e}", d[a]))
                && msg.contains("PeriodicPair::transform"),
            "{msg}"
        );
    }
}

/// Lowest 14 periodic modes of the 3-torus `mesh` built with `pairs`
/// (`PeriodicMap::build` snaps the mesh).
fn torus_spectrum(mut mesh: TetMesh, pairs: &[PeriodicPair]) -> (Vec<f64>, TetMesh) {
    let map = PeriodicMap::build(&mut mesh, pairs, &opts()).expect("snappable");
    let space = HcurlSpace::build(&mesh, ElementOrder::P1);
    let c = PeriodicConstraint::build(&space, &map, None).unwrap();
    let eps = vec![1.0; mesh.n_tets()];
    let settings = PecCavitySettings {
        max_iters: 300,
        ..PecCavitySettings::new(0.7 * 4.0 * std::f64::consts::PI.powi(2), 14)
    };
    let device = <B as BackendTypes>::Device::default();
    let modes = solve_periodic_cavity_modes::<B>(
        &mesh,
        &PecCavityMaterials::Isotropic(&eps),
        &c,
        &settings,
        &device,
    )
    .expect("torus solve");
    (modes.modes.iter().map(|m| m.lambda).collect(), mesh)
}

fn max_rel_diff(a: &[f64], b: &[f64]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs() / y.abs())
        .fold(0.0, f64::max)
}

/// Largest relative spread inside the reference's degenerate clusters
/// (values agreeing to `1e-8` relative in `reference`).
fn degeneracy_split(got: &[f64], reference: &[f64]) -> f64 {
    let mut worst = 0.0_f64;
    let mut i = 0;
    while i < reference.len() {
        let mut j = i + 1;
        while j < reference.len() && (reference[j] - reference[i]).abs() < 1e-8 * reference[i] {
            j += 1;
        }
        if j - i > 1 {
            let (lo, hi) = got[i..j]
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| {
                    (l.min(v), h.max(v))
                });
            worst = worst.max((hi - lo) / reference[i]);
        }
        i = j;
    }
    worst
}

/// (g) Mean-zero jitter: the least-squares translation is exact, so the
/// snapped torus and its spectrum match the exact-translation case to
/// round-off and keep every degeneracy. The pre-#856 bounding-box
/// translation, applied to the same jittered mesh, shifts the spectrum and
/// splits the degeneracies (the test is sensitive to the inferred `d`).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn jittered_torus_spectrum_matches_the_exact_translation() {
    let (base, pairs) = judge_torus();
    let h = 1.0 / TORUS_N as f64;
    let exact: Vec<[f64; 3]> = (0..3).map(unit).collect();
    let (reference, ref_mesh) = torus_spectrum(base.clone(), &with_translations(&pairs, &exact));
    let clusters: Vec<usize> = {
        let mut sizes = Vec::new();
        let mut i = 0;
        while i < reference.len() {
            let mut j = i + 1;
            while j < reference.len() && (reference[j] - reference[i]).abs() < 1e-8 * reference[i] {
                j += 1;
            }
            sizes.push(j - i);
            i = j;
        }
        sizes
    };
    eprintln!("exact torus: lambda = {reference:.6?} (clusters {clusters:?})");
    assert!(
        clusters.iter().any(|&c| c >= 6),
        "the reference has a >= 6-fold cluster: {clusters:?}"
    );

    let mut jittered = base.clone();
    apply_jitter(&mut jittered, &hi_face_jitter(&base, 0.04 * h, 31, true));

    // Exact translation given: the snap restores the cube.
    let (given, _) = torus_spectrum(jittered.clone(), &with_translations(&pairs, &exact));
    // Inferred (least squares).
    let (inferred, ls_mesh) = torus_spectrum(jittered.clone(), &pairs);
    // Pre-#856 inference (bounding-box centroids of the jittered faces).
    let bbox: Vec<[f64; 3]> = pairs
        .iter()
        .map(|p| bbox_translation(&jittered, p))
        .collect();
    let (old, _) = torus_spectrum(jittered.clone(), &with_translations(&pairs, &bbox));

    let mesh_diff = ls_mesh
        .nodes
        .iter()
        .zip(&ref_mesh.nodes)
        .flat_map(|(p, q)| (0..3).map(move |d| (p[d] - q[d]).abs()))
        .fold(0.0, f64::max);
    let (d_given, d_ls, d_old) = (
        max_rel_diff(&given, &reference),
        max_rel_diff(&inferred, &reference),
        max_rel_diff(&old, &reference),
    );
    let (s_ls, s_old) = (
        degeneracy_split(&inferred, &reference),
        degeneracy_split(&old, &reference),
    );
    eprintln!(
        "mean-zero jitter 0.04h: snapped LS mesh vs exact {mesh_diff:.2e}; spectrum rel diff: \
         given {d_given:.2e}, LS {d_ls:.2e}, bbox {d_old:.3e}; degeneracy split: LS {s_ls:.2e}, \
         bbox {s_old:.3e}"
    );
    eprintln!("bbox translations {bbox:?}");
    eprintln!("bbox spectrum {old:.6?}");
    assert!(
        mesh_diff < 1e-14,
        "LS snap did not restore the cube: {mesh_diff:e}"
    );
    assert!(d_given < 1e-10, "given translation: {d_given:e}");
    assert!(d_ls < 1e-10, "LS translation: spectrum drift {d_ls:e}");
    assert!(
        s_ls < 1e-10,
        "LS translation split a degeneracy by {s_ls:e}"
    );
    // Sensitivity: the bounding-box translation is visibly wrong here.
    assert!(d_old > 1e-4, "bbox spectrum drift only {d_old:e}");
    assert!(s_old > 1e-4, "bbox split only {s_old:e}");
}

/// (g') The literal Judge probe (generic jitter, non-zero mean offset): the
/// least-squares translation's spectrum error is smaller than the
/// bounding-box one's. (Exactness needs the mean offset to vanish, (g); here
/// the error is the mean offset, (f).)
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "eigen solves are slow in debug; runs in release CI"
)]
fn judge_probe_spectrum_error_shrinks_under_least_squares() {
    let (base, pairs) = judge_torus();
    let h = 1.0 / TORUS_N as f64;
    let exact: Vec<[f64; 3]> = (0..3).map(unit).collect();
    let (reference, _) = torus_spectrum(base.clone(), &with_translations(&pairs, &exact));
    let mut jittered = base.clone();
    apply_jitter(&mut jittered, &hi_face_jitter(&base, 0.04 * h, 7, false));
    let (inferred, _) = torus_spectrum(jittered.clone(), &pairs);
    let bbox: Vec<[f64; 3]> = pairs
        .iter()
        .map(|p| bbox_translation(&jittered, p))
        .collect();
    let (old, _) = torus_spectrum(jittered.clone(), &with_translations(&pairs, &bbox));
    let (d_ls, d_old) = (
        max_rel_diff(&inferred, &reference),
        max_rel_diff(&old, &reference),
    );
    let (s_ls, s_old) = (
        degeneracy_split(&inferred, &reference),
        degeneracy_split(&old, &reference),
    );
    eprintln!(
        "Judge probe (generic jitter 0.04h): spectrum rel diff LS {d_ls:.3e} vs bbox \
         {d_old:.3e}; degeneracy split LS {s_ls:.3e} vs bbox {s_old:.3e}"
    );
    assert!(d_ls < d_old, "LS {d_ls:e} vs bbox {d_old:e}");
}

/// (h) The #862 Judge's probe: on a large face with **every** slave node
/// jittered (uniformly in a box of half-width `0.04 h`, in-plane or in 3D),
/// coincidental agreement between a few jittered offsets must not pass for
/// an exactly periodic subset. The fit uses every matched pair and the
/// translation is the all-pairs mean offset (the least-squares one) to
/// round-off. Before the fix, 2–5 coincidental pairs agreeing to `snap_tol`
/// were fitted, with translation errors 25–275× the least-squares one.
#[test]
fn large_fully_jittered_face_fits_every_pair() {
    // (cells per face side, period, in-plane jitter)
    for (n, period, in_plane) in [
        (100, 1.0, true),
        (100, 1.0, false),
        (60, 10.0, true),
        (60, 10.0, false),
    ] {
        let (nx, ny) = (2usize, n);
        let mut mesh = box_tet_mesh([nx, n, n], [period; 3]);
        let pairs = box_periodic_pairs(&mesh, &[0]);
        let h = period / n as f64;
        let amp = 0.04 * h;
        let mut rng = Rng::new(862 + n as u64 + in_plane as u64);
        for p in mesh.nodes.iter_mut() {
            if (p[0] - period).abs() < 1e-12 * period {
                for (c, x) in p.iter_mut().enumerate() {
                    let u = amp * (2.0 * rng.uniform() - 1.0);
                    if !(in_plane && c == 0) {
                        *x += u;
                    }
                }
            }
        }
        // All-pairs mean offset: master (0, j, k) ↔ slave (nx, j, k).
        let idx = |i: usize, j: usize, k: usize| i + j * (nx + 1) + k * (nx + 1) * (ny + 1);
        let mut mean = [0.0; 3];
        for k in 0..=n {
            for j in 0..=n {
                let (m, s) = (mesh.nodes[idx(0, j, k)], mesh.nodes[idx(nx, j, k)]);
                for c in 0..3 {
                    mean[c] += s[c] - m[c];
                }
            }
        }
        let n_pairs = (n + 1) * (n + 1);
        let mean = mean.map(|v| v / n_pairs as f64);
        let truth = [period, 0.0, 0.0];
        let dist = |a: [f64; 3], b: [f64; 3]| -> f64 {
            (0..3).map(|c| (a[c] - b[c]).powi(2)).sum::<f64>().sqrt()
        };

        let map = PeriodicMap::build(&mut mesh, &pairs, &opts()).expect("snappable");
        let r = &map.report().pairs[0];
        let d = r.translation;
        let snap_tol = opts().snap_tol_rel * period;
        eprintln!(
            "{n}x{n} face, period {period}, {} jitter: fit {} of {} pairs, |d - true| \
             {:.3e}, |LS - true| {:.3e}",
            if in_plane { "in-plane" } else { "3D" },
            r.translation_fit_pairs,
            r.n_nodes,
            dist(d, truth),
            dist(mean, truth)
        );
        assert_eq!(r.n_nodes, n_pairs);
        assert_eq!(r.translation_fit_pairs, n_pairs, "fit to every pair");
        for c in 0..3 {
            // A refitted component within `snap_tol` of zero is zeroed.
            let zeroed = d[c] == 0.0 && mean[c].abs() <= snap_tol;
            assert!(
                zeroed || (d[c] - mean[c]).abs() <= 1e-13 * period,
                "component {c}: {} vs LS {}",
                d[c],
                mean[c]
            );
        }
        assert!(dist(d, truth) <= dist(mean, truth) + 1e-13 * period);
        let w = map.report().warnings();
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(
            w[0].contains(&format!("least squares over {n_pairs} node pairs"))
                && w[0].contains("estimated translation error"),
            "{}",
            w[0]
        );
    }
}
