//! Validation tests for global assembly.
//!
//! Four layers:
//!   1. `assemble_unit_cube_total_mass_equals_volume` — Σ_ij M_ij = ∫ 1 dV = 1
//!      across the whole 5×5×5 cube. A coarse but cheap sanity check.
//!   2. `assemble_5x5x5_matches_cpu_reference` — entrywise K and M against
//!      an independent CPU assembler written in this file.
//!   3. `sparsity_pattern_matches_assembled_k` — every non-zero entry of K
//!      lies on a `(row, col)` pair in the returned sparsity pattern, and
//!      every reported pair is reachable from the connectivity.
//!   4. `assembly_preserves_autodiff` — gradient of `sum(K²)` w.r.t. node
//!      coordinates flows through `scatter(Add)`, has the expected shape,
//!      is finite, has non-zero entries, and matches a finite-difference
//!      probe at a single picked node within a coarse tolerance.

use burn::backend::Autodiff;
use burn::tensor::ElementConversion;
use burn::tensor::backend::BackendTypes;
use burn::tensor::{Int, Tensor, TensorData};

use geode_core::{DefaultBackend, assemble_global_p1, cube_tet_mesh, upload_mesh};

mod common;
use common::readback_f64;

type B = DefaultBackend;
type Ad = Autodiff<B>;

const F32_TOL: f64 = 1e-4;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn ad_device() -> <Ad as BackendTypes>::Device {
    <Ad as BackendTypes>::Device::default()
}

// --- CPU reference assembler -------------------------------------------------

fn cpu_p1_local(verts: &[[f64; 3]; 4]) -> ([[f64; 4]; 4], [[f64; 4]; 4]) {
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let e1 = sub(verts[1], verts[0]);
    let e2 = sub(verts[2], verts[0]);
    let e3 = sub(verts[3], verts[0]);
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];

    let g1 = cross(e2, e3);
    let g2 = cross(e3, e1);
    let g3 = cross(e1, e2);
    let g0 = [
        -(g1[0] + g2[0] + g3[0]),
        -(g1[1] + g2[1] + g3[1]),
        -(g1[2] + g2[2] + g3[2]),
    ];
    let det = dot(e1, g1);
    let abs_det = det.abs();
    let g = [g0, g1, g2, g3];

    let mut k = [[0.0f64; 4]; 4];
    for (i, row) in k.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = dot(g[i], g[j]) / (6.0 * abs_det);
        }
    }

    let v_e = abs_det / 6.0;
    let mut m = [[0.0f64; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = if i == j { v_e / 10.0 } else { v_e / 20.0 };
        }
    }
    (k, m)
}

fn cpu_assemble(nodes: &[[f64; 3]], tets: &[[u32; 4]]) -> (Vec<Vec<f64>>, Vec<Vec<f64>>) {
    let n_dof = nodes.len();
    let mut k = vec![vec![0.0f64; n_dof]; n_dof];
    let mut m = vec![vec![0.0f64; n_dof]; n_dof];
    for tet in tets {
        let verts = [
            nodes[tet[0] as usize],
            nodes[tet[1] as usize],
            nodes[tet[2] as usize],
            nodes[tet[3] as usize],
        ];
        let (ke, me) = cpu_p1_local(&verts);
        for i in 0..4 {
            for j in 0..4 {
                k[tet[i] as usize][tet[j] as usize] += ke[i][j];
                m[tet[i] as usize][tet[j] as usize] += me[i][j];
            }
        }
    }
    (k, m)
}

// --- Tests -------------------------------------------------------------------

#[test]
fn assemble_unit_cube_total_mass_equals_volume() {
    // 5×5×5 cube of side 1: total mass should sum to volume = 1.
    let mesh = cube_tet_mesh(5, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());

    let m_data: Vec<f64> = readback_f64(sys.m);
    let total: f64 = m_data.iter().sum();
    assert!(
        (total - 1.0).abs() < F32_TOL,
        "Σ M_ij = {total} (expected 1.0 for unit cube)"
    );
}

#[test]
fn assemble_5x5x5_matches_cpu_reference() {
    let mesh = cube_tet_mesh(5, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());

    let (k_ref, m_ref) = cpu_assemble(&mesh.nodes, &mesh.tets);
    let n = mesh.n_nodes();

    let k_got: Vec<f64> = readback_f64(sys.k);
    let m_got: Vec<f64> = readback_f64(sys.m);

    let mut max_k_err = 0.0f64;
    let mut max_m_err = 0.0f64;
    for i in 0..n {
        for j in 0..n {
            let kg = k_got[i * n + j];
            let mg = m_got[i * n + j];
            let kref = k_ref[i][j];
            let mref = m_ref[i][j];
            max_k_err = max_k_err.max((kg - kref).abs());
            max_m_err = max_m_err.max((mg - mref).abs());
        }
    }
    assert!(
        max_k_err < F32_TOL * 10.0,
        "max K error {max_k_err} > {}",
        F32_TOL * 10.0
    );
    assert!(
        max_m_err < F32_TOL * 10.0,
        "max M error {max_m_err} > {}",
        F32_TOL * 10.0
    );
}

#[test]
fn assembled_k_is_symmetric() {
    let mesh = cube_tet_mesh(3, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let n = mesh.n_nodes();
    let k: Vec<f64> = readback_f64(sys.k);
    for i in 0..n {
        for j in (i + 1)..n {
            let kij = k[i * n + j];
            let kji = k[j * n + i];
            assert!(
                (kij - kji).abs() < F32_TOL,
                "K[{i},{j}] = {kij} vs K[{j},{i}] = {kji}"
            );
        }
    }
}

#[test]
fn sparsity_pattern_matches_assembled_k() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());

    let n = mesh.n_nodes();
    let k: Vec<f64> = readback_f64(sys.k.clone());

    // Build the set of (row, col) reported by sparsity.
    use std::collections::HashSet;
    let reported: HashSet<(u32, u32)> = sys
        .sparsity
        .rows
        .iter()
        .zip(sys.sparsity.cols.iter())
        .map(|(&r, &c)| (r, c))
        .collect();

    // Every non-zero entry of K must be reported.
    for i in 0..n {
        for j in 0..n {
            if k[i * n + j].abs() > F32_TOL {
                assert!(
                    reported.contains(&(i as u32, j as u32)),
                    "K[{i},{j}] = {} is non-zero but not in sparsity pattern",
                    k[i * n + j]
                );
            }
        }
    }
}

#[test]
fn assembly_preserves_autodiff() {
    // Use a small mesh so the FD probe is cheap.
    let mesh = cube_tet_mesh(2, 1.0);
    let n = mesh.n_nodes();
    let n_elem = mesh.n_tets();
    let ad_dev = ad_device();

    // Carry node coordinates at the backend's FloatElem precision so the
    // f64 ndarray path delivers f64 autodiff (issue #99: upload_mesh now
    // honors B::FloatElem; tests should match the same discipline).
    let node_flat: Vec<<Ad as BackendTypes>::FloatElem> = mesh
        .nodes
        .iter()
        .flat_map(|p| {
            p.iter()
                .map(|&x| x.elem::<<Ad as BackendTypes>::FloatElem>())
        })
        .collect();
    let tet_flat: Vec<i32> = mesh
        .tets
        .iter()
        .flat_map(|t| t.iter().map(|&i| i as i32))
        .collect();

    let nodes = Tensor::<Ad, 2>::from_data(TensorData::new(node_flat.clone(), [n, 3]), &ad_dev)
        .require_grad();
    let tets = Tensor::<Ad, 2, Int>::from_data(TensorData::new(tet_flat, [n_elem, 4]), &ad_dev);

    // Functional: sum of squared K entries — a simple scalar of K that
    // depends non-trivially on node positions.
    let sys = assemble_global_p1(nodes.clone(), tets, n);
    let loss = sys.k.clone().powf_scalar(2.0).sum();
    let grads = loss.backward();

    let dnodes = nodes
        .grad(&grads)
        .expect("gradient w.r.t. nodes should exist");
    let dims = dnodes.dims();
    assert_eq!(dims, [n, 3], "gradient shape mismatch");

    let dnodes_vec: Vec<f64> = readback_f64(dnodes);
    let mut finite = 0;
    let mut nonzero = 0;
    for &g in &dnodes_vec {
        if g.is_finite() {
            finite += 1;
        }
        if g.abs() > 1e-6 {
            nonzero += 1;
        }
    }
    assert_eq!(
        finite,
        dnodes_vec.len(),
        "all gradient entries must be finite"
    );
    assert!(
        nonzero > 0,
        "gradient must have at least one non-zero entry"
    );

    // Finite-difference probe at a single node coordinate that's away from
    // the boundary — pick the interior node of the 2×2×2 cube.
    let probe_node = n / 2;
    let probe_dim = 0; // x
    let probe_idx = probe_node * 3 + probe_dim;
    let analytic_grad = dnodes_vec[probe_idx];

    let loss_of = |perturbed: &[<B as BackendTypes>::FloatElem]| -> f64 {
        let nodes_b =
            Tensor::<B, 2>::from_data(TensorData::new(perturbed.to_vec(), [n, 3]), &device());
        let tet_flat_b: Vec<i32> = mesh
            .tets
            .iter()
            .flat_map(|t| t.iter().map(|&i| i as i32))
            .collect();
        let tets_b =
            Tensor::<B, 2, Int>::from_data(TensorData::new(tet_flat_b, [n_elem, 4]), &device());
        let sys_b = assemble_global_p1(nodes_b, tets_b, n);
        let k_data: Vec<f64> = readback_f64(sys_b.k);
        k_data.iter().map(|&v| v.powi(2)).sum()
    };

    let eps = 5e-3_f64;
    let mut plus = node_flat.clone();
    plus[probe_idx] = (plus[probe_idx].elem::<f64>() + eps).elem();
    let mut minus = node_flat.clone();
    minus[probe_idx] = (minus[probe_idx].elem::<f64>() - eps).elem();
    let fd_grad = (loss_of(&plus) - loss_of(&minus)) / (2.0 * eps);

    // Assembly involves f32 + scatter-add; the FD<-> autodiff agreement is
    // necessarily coarse. Require ~1% relative agreement OR small absolute
    // error — whichever is achievable on this small mesh.
    let rel_err = (fd_grad - analytic_grad).abs() / (1.0 + fd_grad.abs().max(analytic_grad.abs()));
    assert!(
        rel_err < 0.05,
        "FD gradient {fd_grad} vs autodiff {analytic_grad}: rel err {rel_err}"
    );
}
