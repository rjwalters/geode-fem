//! Unit test for `assemble_global_nedelec_with_nu` (issue #504): the thin
//! real-scalar-ν curl-curl-weighting wrapper — the dual of
//! `assemble_global_nedelec_with_epsilon` (which weights mass).
//!
//! Verifies that scaling every element by a uniform `ν_r` scales the
//! assembled stiffness `K` by exactly `ν_r` (and leaves the mass `M`
//! unweighted), matching the unweighted `assemble_global_nedelec` baseline.

use burn::backend::NdArray;
use burn::tensor::{Int, Tensor, TensorData};
use geode_core::assembly::nedelec::{assemble_global_nedelec, assemble_global_nedelec_with_nu};
use geode_core::mesh::cube_tet_mesh;

type B = NdArray<f32>;

fn edge_tables(mesh: &geode_core::mesh::TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    let idx = te
        .iter()
        .map(|row| {
            let mut r = [0u32; 6];
            for (s, &(g, _)) in r.iter_mut().zip(row.iter()) {
                *s = g;
            }
            r
        })
        .collect();
    let sign = te
        .iter()
        .map(|row| {
            let mut r = [0i8; 6];
            for (s, &(_, sg)) in r.iter_mut().zip(row.iter()) {
                *s = sg;
            }
            r
        })
        .collect();
    (idx, sign)
}

#[test]
fn nu_wrapper_scales_stiffness_leaves_mass() {
    let mesh = cube_tet_mesh(3, 1.0);
    let n_edges = mesh.edges().len();
    let (idx, sign) = edge_tables(&mesh);
    let dev = Default::default();
    let nodes_flat: Vec<f32> = mesh
        .nodes
        .iter()
        .flat_map(|n| n.iter().map(|&x| x as f32))
        .collect();
    let nodes = Tensor::<B, 2>::from_data(TensorData::new(nodes_flat, [mesh.n_nodes(), 3]), &dev);
    let tets_flat: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| t.iter().map(|&x| x as i32))
        .collect();
    let tets = Tensor::<B, 2, Int>::from_data(TensorData::new(tets_flat, [mesh.n_tets(), 4]), &dev);

    let base = assemble_global_nedelec::<B>(nodes.clone(), tets.clone(), &idx, &sign, n_edges);
    let nu = 3.5_f64;
    let nu_r = vec![nu; mesh.n_tets()];
    let weighted = assemble_global_nedelec_with_nu::<B>(
        nodes.clone(),
        tets.clone(),
        &idx,
        &sign,
        n_edges,
        &nu_r,
    );

    let kb: Vec<f32> = base.k.to_data().to_vec().unwrap();
    let kw: Vec<f32> = weighted.k.to_data().to_vec().unwrap();
    let mb: Vec<f32> = base.m.to_data().to_vec().unwrap();
    let mw: Vec<f32> = weighted.m.to_data().to_vec().unwrap();

    let mut max_k_err = 0.0f64;
    let mut max_m_err = 0.0f64;
    let mut scale = 0.0f64;
    for i in 0..kb.len() {
        // Stiffness scaled by nu.
        max_k_err = max_k_err.max((kw[i] as f64 - nu * kb[i] as f64).abs());
        scale = scale.max((kb[i] as f64).abs());
        // Mass unchanged.
        max_m_err = max_m_err.max((mw[i] as f64 - mb[i] as f64).abs());
    }
    assert!(
        max_k_err / (nu * scale) < 1e-5,
        "nu-wrapper K != nu * base K (max err {max_k_err:.3e})"
    );
    let mscale = mb.iter().map(|&x| (x as f64).abs()).fold(0.0, f64::max);
    assert!(
        max_m_err / mscale < 1e-5,
        "nu-wrapper must leave M unweighted (max err {max_m_err:.3e})"
    );
}
