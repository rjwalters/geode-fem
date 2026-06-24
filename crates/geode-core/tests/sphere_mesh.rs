//! Integration tests for the bundled sphere-in-vacuum mesh fixture
//! (issues #25, #38).
//!
//! The fixture is a Gmsh-generated MSH 4.1 ASCII mesh of a dielectric
//! sphere of radius `R_SPHERE = 1.0` embedded in two concentric vacuum
//! shells (`vacuum_gap` for `R_SPHERE < r ≤ R_PML_INNER` and
//! `pml_shell` for `R_PML_INNER < r ≤ R_BUFFER`). These tests verify:
//!
//! - The loader parses the fixture without error.
//! - All expected physical groups are present.
//! - Interior tets satisfy `|p| <= R_SPHERE + eps` at every vertex.
//! - Vacuum-gap tets are sandwiched between `R_SPHERE` and `R_PML_INNER`.
//! - PML-shell tets are sandwiched between `R_PML_INNER` and `R_BUFFER`.
//! - The total tet volume approximates `(4/3) π R_BUFFER^3` within 10%
//!   (low-res Gmsh meshes underestimate by a few percent; this is OK
//!   and tightens with refinement).
//! - Per-region volumes are within 10% of the analytical ball / shell
//!   volumes.
//! - All tets have positive signed volume (right-handed).

use geode_core::{
    PHYS_OUTER_BOUNDARY, PHYS_PML_SHELL, PHYS_SPHERE_INTERIOR, PHYS_SPHERE_SURFACE,
    PHYS_VACUUM_GAP, R_BUFFER, R_PML_INNER, R_SPHERE, read_sphere_fixture,
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
        f.mesh.physical_groups.get(&(3, PHYS_VACUUM_GAP)),
        Some(&"vacuum_gap".to_string()),
        "missing (3, 2) vacuum_gap",
    );
    assert_eq!(
        f.mesh.physical_groups.get(&(3, PHYS_PML_SHELL)),
        Some(&"pml_shell".to_string()),
        "missing (3, 5) pml_shell",
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
fn sphere_fixture_vacuum_gap_in_inner_shell() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut checked = 0;
    for (tet_idx, &phys) in f.tet_physical_tags.iter().enumerate() {
        if phys != PHYS_VACUUM_GAP {
            continue;
        }
        let tet = &f.mesh.tets[tet_idx];
        for &node_idx in tet {
            let p = f.mesh.nodes[node_idx as usize];
            let r = norm(p);
            assert!(
                r >= R_SPHERE - SURFACE_EPS,
                "vacuum-gap tet {tet_idx} node {node_idx} at radius {r} < R_SPHERE",
            );
            assert!(
                r <= R_PML_INNER + SURFACE_EPS,
                "vacuum-gap tet {tet_idx} node {node_idx} at radius {r} > R_PML_INNER",
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no vacuum-gap tets to check");
}

#[test]
fn sphere_fixture_pml_shell_in_outer_shell() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut checked = 0;
    for (tet_idx, &phys) in f.tet_physical_tags.iter().enumerate() {
        if phys != PHYS_PML_SHELL {
            continue;
        }
        let tet = &f.mesh.tets[tet_idx];
        for &node_idx in tet {
            let p = f.mesh.nodes[node_idx as usize];
            let r = norm(p);
            assert!(
                r >= R_PML_INNER - SURFACE_EPS,
                "pml-shell tet {tet_idx} node {node_idx} at radius {r} < R_PML_INNER",
            );
            assert!(
                r <= R_BUFFER + SURFACE_EPS,
                "pml-shell tet {tet_idx} node {node_idx} at radius {r} > R_BUFFER",
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "no pml-shell tets to check");
}

#[test]
fn sphere_fixture_total_volume() {
    let f = read_sphere_fixture().expect("fixture load");
    let mut total = 0.0;
    let mut interior = 0.0;
    let mut vacuum_gap = 0.0;
    let mut pml_shell = 0.0;
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
            PHYS_VACUUM_GAP => vacuum_gap += v,
            PHYS_PML_SHELL => pml_shell += v,
            other => panic!("unexpected per-tet physical tag {other}"),
        }
    }

    let four_thirds_pi = (4.0 / 3.0) * std::f64::consts::PI;
    let expected_total = four_thirds_pi * R_BUFFER.powi(3);
    let expected_interior = four_thirds_pi * R_SPHERE.powi(3);
    let expected_vacuum_gap = four_thirds_pi * (R_PML_INNER.powi(3) - R_SPHERE.powi(3));
    let expected_pml_shell = four_thirds_pi * (R_BUFFER.powi(3) - R_PML_INNER.powi(3));

    let rel = |got: f64, want: f64| (got - want).abs() / want;
    let rel_total = rel(total, expected_total);
    let rel_interior = rel(interior, expected_interior);
    let rel_vacuum_gap = rel(vacuum_gap, expected_vacuum_gap);
    let rel_pml_shell = rel(pml_shell, expected_pml_shell);

    // 12% is a comfortable bound for low-res faceted spheres. Each tet
    // on the curved boundary loses some volume to the faceting; the
    // bound tightens as `lc` shrinks in `mesh_scripts/sphere.geo`. The
    // gap and PML shells individually have less material than the
    // single old `vacuum_buffer`, so we widen the tolerance slightly
    // (10% → 12%) for those two.
    assert!(
        rel_total < 0.10,
        "total volume {total} vs expected {expected_total} (rel err {rel_total})",
    );
    assert!(
        rel_interior < 0.10,
        "interior volume {interior} vs expected {expected_interior} (rel err {rel_interior})",
    );
    assert!(
        rel_vacuum_gap < 0.12,
        "vacuum-gap volume {vacuum_gap} vs expected {expected_vacuum_gap} (rel err {rel_vacuum_gap})",
    );
    assert!(
        rel_pml_shell < 0.12,
        "pml-shell volume {pml_shell} vs expected {expected_pml_shell} (rel err {rel_pml_shell})",
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
