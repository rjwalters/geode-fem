//! Integration tests for the bundled sphere-in-vacuum mesh fixture
//! (issue #25).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII mesh of a dielectric
//! sphere of radius `R_SPHERE = 1.0` embedded in a vacuum buffer of outer
//! radius `R_BUFFER = 2.0`. These tests verify:
//!
//! - The loader parses the fixture without error.
//! - All four expected physical groups are present.
//! - Interior tets satisfy `|p| <= R_SPHERE + eps` at every vertex.
//! - Buffer tets satisfy `|p| >= R_SPHERE - eps` at every vertex.
//! - The total tet volume approximates `(4/3) π R_BUFFER^3` within 10%
//!   (low-res Gmsh meshes underestimate by a few percent; this is OK
//!   and tightens with refinement).
//! - Per-region volumes are within 10% of the analytical ball / shell
//!   volumes.
//! - All tets have positive signed volume (right-handed).

use geode_core::{
    read_sphere_fixture, PHYS_OUTER_BOUNDARY, PHYS_SPHERE_INTERIOR, PHYS_SPHERE_SURFACE,
    PHYS_VACUUM_BUFFER, R_BUFFER, R_SPHERE,
};

/// Surface tolerance for the radius check: vertices sitting on the
/// dielectric interface satisfy `|p| = R_SPHERE` only up to Gmsh's
/// geometric tolerance. 1e-6 is comfortably larger than Gmsh's defaults
/// while still being tight enough to catch a mis-classified tet.
const SURFACE_EPS: f64 = 1e-6;

fn norm(p: [f64; 3]) -> f64 {
    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()
}

fn signed_volume(p0: [f64; 3], p1: [f64; 3], p2: [f64; 3], p3: [f64; 3]) -> f64 {
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let e3 = [p3[0] - p0[0], p3[1] - p0[1], p3[2] - p0[2]];
    let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
    det / 6.0
}

#[test]
fn sphere_fixture_loads() {
    let f = read_sphere_fixture().expect("fixture load");
    assert!(f.mesh.n_nodes() > 0, "no nodes");
    assert!(f.mesh.n_tets() > 0, "no tets");
    assert_eq!(
        f.tet_physical_tags.len(),
        f.mesh.n_tets(),
        "per-tet tag count must match tet count",
    );
}

#[test]
fn sphere_fixture_physical_groups() {
    let f = read_sphere_fixture().expect("fixture load");
    // Volume groups: required.
    assert_eq!(
        f.mesh.physical_groups.get(&(3, PHYS_SPHERE_INTERIOR)),
        Some(&"sphere_interior".to_string()),
        "missing (3, 1) sphere_interior",
    );
    assert_eq!(
        f.mesh.physical_groups.get(&(3, PHYS_VACUUM_BUFFER)),
        Some(&"vacuum_buffer".to_string()),
        "missing (3, 2) vacuum_buffer",
    );
    // Surface groups: required for outer BC; interface optional but
    // present in this fixture.
    assert_eq!(
        f.mesh.physical_groups.get(&(2, PHYS_OUTER_BOUNDARY)),
        Some(&"outer_boundary".to_string()),
        "missing (2, 3) outer_boundary",
    );
    assert_eq!(
        f.mesh.physical_groups.get(&(2, PHYS_SPHERE_SURFACE)),
        Some(&"sphere_surface".to_string()),
        "missing (2, 4) sphere_surface",
    );
}

#[test]
fn sphere_fixture_interior_inside_sphere() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut checked = 0;
    for (tet_idx, &phys) in f.tet_physical_tags.iter().enumerate() {
        if phys != PHYS_SPHERE_INTERIOR {
            continue;
        }
        let tet = &f.mesh.tets[tet_idx];
        for &node_idx in tet {
            let p = f.mesh.nodes[node_idx as usize];
            let r = norm(p);
            assert!(
                r <= R_SPHERE + SURFACE_EPS,
                "interior tet {tet_idx} node {node_idx} at radius {r} > R_SPHERE",
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no interior tets to check");
}

#[test]
fn sphere_fixture_buffer_outside_sphere() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut checked = 0;
    for (tet_idx, &phys) in f.tet_physical_tags.iter().enumerate() {
        if phys != PHYS_VACUUM_BUFFER {
            continue;
        }
        let tet = &f.mesh.tets[tet_idx];
        for &node_idx in tet {
            let p = f.mesh.nodes[node_idx as usize];
            let r = norm(p);
            assert!(
                r >= R_SPHERE - SURFACE_EPS,
                "buffer tet {tet_idx} node {node_idx} at radius {r} < R_SPHERE",
            );
            assert!(
                r <= R_BUFFER + SURFACE_EPS,
                "buffer tet {tet_idx} node {node_idx} at radius {r} > R_BUFFER",
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no buffer tets to check");
}

#[test]
fn sphere_fixture_total_volume() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut total = 0.0;
    let mut interior = 0.0;
    let mut buffer = 0.0;
    for (tet_idx, tet) in f.mesh.tets.iter().enumerate() {
        let [a, b, c, d] = *tet;
        let v = signed_volume(
            f.mesh.nodes[a as usize],
            f.mesh.nodes[b as usize],
            f.mesh.nodes[c as usize],
            f.mesh.nodes[d as usize],
        )
        .abs();
        total += v;
        match f.tet_physical_tags[tet_idx] {
            PHYS_SPHERE_INTERIOR => interior += v,
            PHYS_VACUUM_BUFFER => buffer += v,
            other => panic!("unexpected per-tet physical tag {other}"),
        }
    }

    let expected_total = (4.0 / 3.0) * std::f64::consts::PI * R_BUFFER.powi(3);
    let expected_interior = (4.0 / 3.0) * std::f64::consts::PI * R_SPHERE.powi(3);
    let expected_buffer = expected_total - expected_interior;

    let rel_total = (total - expected_total).abs() / expected_total;
    let rel_interior = (interior - expected_interior).abs() / expected_interior;
    let rel_buffer = (buffer - expected_buffer).abs() / expected_buffer;

    // 10% is a comfortable bound for low-res faceted spheres. Each tet
    // on the curved boundary loses some volume to the faceting; the
    // bound tightens as `lc` shrinks in `mesh_scripts/sphere.geo`.
    assert!(
        rel_total < 0.10,
        "total volume {total} vs expected {expected_total} (rel err {rel_total})",
    );
    assert!(
        rel_interior < 0.10,
        "interior volume {interior} vs expected {expected_interior} (rel err {rel_interior})",
    );
    assert!(
        rel_buffer < 0.10,
        "buffer volume {buffer} vs expected {expected_buffer} (rel err {rel_buffer})",
    );
}

#[test]
fn sphere_fixture_tets_are_right_handed() {
    // Acceptance criterion from the issue: all tets must have positive
    // signed volume. Gmsh emits CCW-oriented tets by convention; this
    // test asserts the convention has not been broken in transit.
    let f = read_sphere_fixture().expect("fixture load");
    let mut bad = 0usize;
    for (tet_idx, tet) in f.mesh.tets.iter().enumerate() {
        let [a, b, c, d] = *tet;
        let v = signed_volume(
            f.mesh.nodes[a as usize],
            f.mesh.nodes[b as usize],
            f.mesh.nodes[c as usize],
            f.mesh.nodes[d as usize],
        );
        if v <= 0.0 {
            bad += 1;
            eprintln!("tet {tet_idx} has non-positive signed volume {v}");
        }
    }
    assert_eq!(bad, 0, "{bad} tets have non-positive signed volume");
}
