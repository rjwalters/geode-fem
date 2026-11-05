//! Shared Burn-side sphere-PEC pipeline for the `sphere_pec_*_reference.rs`
//! harnesses (Issue #1317).
//!
//! Each integration test is its own binary, so any item a given harness does
//! not read would warn; the module-level `allow(dead_code)` covers that.
//! Per-backend tolerance tables stay in each harness: their values (and even
//! their field sets) differ between harnesses.
#![allow(dead_code)]

use burn::tensor::backend::BackendTypes;
use faer::Mat;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_pec_interior_edges,
    sphere_pec_node_interior_mask, spurious_dim_from_derham,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

pub struct BurnPipeline {
    pub n_nodes: usize,
    pub n_tets: usize,
    pub epsilon_r: Vec<f64>,
    pub n_edges: usize,
    pub n_interior_edges: usize,
    pub spurious_dim: usize,
    pub k_int: Mat<f64>,
    pub m_int: Mat<f64>,
}

pub fn run_burn_pipeline() -> BurnPipeline {
    let fixture = read_sphere_fixture().expect("fixture load");
    let n_nodes = fixture.mesh.n_nodes();
    let n_tets = fixture.mesh.n_tets();

    let epsilon_r = build_epsilon_r(&fixture.tet_physical_tags, 1.5);

    let edges = fixture.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = fixture.mesh.tet_edges();
    let tet_edge_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_edge_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "Burn-side edge mask shape");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();

    let node_interior_mask = sphere_pec_node_interior_mask(&fixture.mesh, R_BUFFER);
    let spurious_dim = spurious_dim_from_derham(&fixture.mesh, &interior_mask, &node_interior_mask);

    let device = <B as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t,
        tets_t,
        &tet_edge_idx,
        &tet_edge_sign,
        n_edges,
        &epsilon_r,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);
    let (k_int, m_int) = apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &interior_mask)
        .expect("Dirichlet BC reduction");

    BurnPipeline {
        n_nodes,
        n_tets,
        epsilon_r,
        n_edges,
        n_interior_edges,
        spurious_dim,
        k_int,
        m_int,
    }
}
