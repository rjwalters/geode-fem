//! Round-trip integration test for the Gmsh MSH 4.1 ASCII reader.
//!
//! Reads `tests/fixtures/unit_cube.msh` (8 nodes, 5 tets, one physical
//! group "domain") and asserts that every value parsed back matches
//! what was written into the fixture.

use geode_core::mesh::{GmshReader, MeshReader};

const UNIT_CUBE_MSH: &[u8] = include_bytes!("fixtures/unit_cube.msh");

/// Hand-mirror of the fixture, used as ground truth for the round-trip.
fn expected_nodes() -> [[f64; 3]; 8] {
    [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ]
}

/// Tet connectivity in 0-based indices (subtract 1 from each Gmsh tag).
///
/// All five tets are oriented so that `det(J) > 0` — the fourth tet's
/// last two vertices are swapped vs. the naive 5-tet split to keep the
/// vertex order right-handed (caught by `p1_local_matrices::unit_cube_fixture_volume_conservation`).
fn expected_tets() -> [[u32; 4]; 5] {
    [
        [0, 1, 2, 5],
        [0, 2, 3, 7],
        [0, 5, 7, 4],
        [0, 2, 7, 5],
        [2, 5, 6, 7],
    ]
}

#[test]
fn unit_cube_node_count() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    assert_eq!(mesh.n_nodes(), 8);
}

#[test]
fn unit_cube_tet_count() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    assert_eq!(mesh.n_tets(), 5);
}

#[test]
fn unit_cube_node_coordinates_round_trip() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    for (got, want) in mesh.nodes.iter().zip(expected_nodes().iter()) {
        for (a, b) in got.iter().zip(want.iter()) {
            assert!((a - b).abs() < 1e-12, "{a} != {b}");
        }
    }
}

#[test]
fn unit_cube_tet_connectivity_round_trip() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    assert_eq!(mesh.tets, expected_tets());
}

#[test]
fn unit_cube_physical_groups_round_trip() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    assert_eq!(mesh.physical_groups.len(), 1);
    let name = mesh
        .physical_groups
        .get(&(3, 1))
        .expect("physical group (3, 1) missing");
    assert_eq!(name, "domain");
}

#[test]
fn unit_cube_tet_volumes_are_positive() {
    // Sanity check: the 5-tet split of the unit cube should partition it
    // exactly. Sum of |det| / 6 across the five tets should equal 1.0.
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    let mut total_volume = 0.0;
    for tet in &mesh.tets {
        let [a, b, c, d] = *tet;
        let p0 = mesh.nodes[a as usize];
        let p1 = mesh.nodes[b as usize];
        let p2 = mesh.nodes[c as usize];
        let p3 = mesh.nodes[d as usize];
        let e1 = sub(p1, p0);
        let e2 = sub(p2, p0);
        let e3 = sub(p3, p0);
        let det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
            + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
        total_volume += det.abs() / 6.0;
    }
    assert!(
        (total_volume - 1.0).abs() < 1e-12,
        "sum of tet volumes = {total_volume}, expected 1.0"
    );
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
