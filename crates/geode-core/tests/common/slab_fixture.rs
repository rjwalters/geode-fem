//! Shared truncated-slab fixture for the surface-impedance driven tests
//! (`london_surface_impedance.rs`, `leontovich_surface_impedance.rs`).
//!
//! Pulled in with
//! `#[path = "common/slab_fixture.rs"] mod slab_fixture;`.
//!
//! # The fixture
//!
//! The unit cube truncated to the vacuum half `x ≤ x_cut`, with the
//! truncation wall exposed as a list of conforming surface triangles.
//!
//! Node coordinates (and hence node indices) are shared with the full
//! `cube_tet_mesh(n, 1.0)`, so centerline edges can be compared between
//! the truncated and full meshes by node-pair key.

#![allow(dead_code)] // each test binary uses a subset

use faer::c64;
use geode_core::driven::solve::CurrentSource;
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use std::collections::BTreeMap;
use std::f64::consts::PI;

const GEOM_TOL: f64 = 1e-9;

/// The unit cube truncated to the vacuum half `x ≤ x_cut`, with the
/// truncation wall exposed as a list of conforming surface triangles.
///
/// Node coordinates (and hence node indices) are shared with the full
/// `cube_tet_mesh(n, 1.0)`, so centerline edges can be compared between
/// the truncated and full meshes by node-pair key.
pub struct TruncatedSlab {
    pub mesh: TetMesh,
    /// Triangles on the `x = x_cut` wall.
    pub wall_tris: Vec<[u32; 3]>,
    /// Index (into `mesh.tets`) of the tet owning each wall triangle.
    pub wall_tet: Vec<usize>,
    /// PEC mask: side/back walls eliminated, `x = x_cut` wall KEPT
    /// (impedance surface).
    pub interior_mask: Vec<bool>,
    /// PEC mask with the `x = x_cut` wall eliminated too (full PEC box).
    pub interior_mask_pec_wall: Vec<bool>,
}

pub fn truncated_slab(n: usize, x_cut: f64) -> TruncatedSlab {
    let full = cube_tet_mesh(n, 1.0);
    let tets: Vec<[u32; 4]> = full
        .tets
        .iter()
        .filter(|t| {
            t.iter()
                .all(|&v| full.nodes[v as usize][0] <= x_cut + GEOM_TOL)
        })
        .copied()
        .collect();
    assert!(!tets.is_empty(), "truncation removed every tet");
    let mesh = TetMesh {
        nodes: full.nodes,
        tets,
        physical_groups: BTreeMap::new(),
    };

    // Wall triangles: tet faces whose three vertices all sit on x = x_cut.
    let mut wall_tris = Vec::new();
    let mut wall_tet = Vec::new();
    for (ti, tet) in mesh.tets.iter().enumerate() {
        for omit in 0..4 {
            let tri: [u32; 3] = {
                let mut it = (0..4).filter(|&v| v != omit).map(|v| tet[v]);
                std::array::from_fn(|_| it.next().unwrap())
            };
            if tri
                .iter()
                .all(|&v| (mesh.nodes[v as usize][0] - x_cut).abs() < GEOM_TOL)
            {
                wall_tris.push(tri);
                wall_tet.push(ti);
            }
        }
    }
    assert_eq!(
        wall_tris.len(),
        2 * n * n,
        "expected 2 wall triangles per hex face"
    );

    let edges = mesh.edges();
    let on = |v: u32, k: usize, val: f64| (mesh.nodes[v as usize][k] - val).abs() < GEOM_TOL;
    let on_pec_side = |e: &[u32; 2]| {
        let planes = [(0usize, 0.0), (1, 0.0), (1, 1.0), (2, 0.0), (2, 1.0)];
        planes
            .iter()
            .any(|&(k, val)| on(e[0], k, val) && on(e[1], k, val))
    };
    let interior_mask: Vec<bool> = edges.iter().map(|e| !on_pec_side(e)).collect();
    let interior_mask_pec_wall: Vec<bool> = edges
        .iter()
        .zip(interior_mask.iter())
        .map(|(e, &keep)| keep && !(on(e[0], 0, x_cut) && on(e[1], 0, x_cut)))
        .collect();

    TruncatedSlab {
        mesh,
        wall_tris,
        wall_tet,
        interior_mask,
        interior_mask_pec_wall,
    }
}

/// The slab-fixture source `J = ẑ sin(πy)` on the first element layer
/// `x < h` (identical closure for the full and truncated meshes — the
/// truncated mesh simply has no tets beyond `x_cut`).
pub fn slab_source(mesh: &TetMesh, h: f64) -> CurrentSource {
    CurrentSource::from_centroids(mesh, |c| {
        let jz = if c[0] < h { (PI * c[1]).sin() } else { 0.0 };
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(jz, 0.0)]
    })
}
