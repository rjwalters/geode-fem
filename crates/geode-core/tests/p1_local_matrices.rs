//! Validation tests for batched P1 local stiffness and mass matrices.
//!
//! Three layers of validation:
//!   1. The canonical reference tet — exact rationals, tightest tolerance.
//!   2. A batch of 32 deterministically-randomized affine tets compared
//!      against an independent CPU reference written in this file.
//!   3. The unit-cube fixture (5 tets) parsed via `GmshReader` — checks
//!      invariants that hold across the whole assembly: volume conservation,
//!      K symmetry, K row sums (partition of unity → zero), and M row sums
//!      (equal to per-element volume for consistent mass).

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Tensor, TensorData};

use geode_core::{batched_p1_local_matrices, DefaultBackend, GmshReader, MeshReader};

mod common;
use common::readback_f64;

type B = DefaultBackend;

const UNIT_CUBE_MSH: &[u8] = include_bytes!("fixtures/unit_cube.msh");

/// f32 tolerance — Burn's default float backend is f32, reference is f64.
const F32_TOL: f64 = 1e-5;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn coords_tensor_from_vec(tets: &[[[f64; 3]; 4]]) -> Tensor<B, 3> {
    let n = tets.len();
    let flat: Vec<f32> = tets
        .iter()
        .flat_map(|tet| tet.iter().flat_map(|v| v.iter().map(|&x| x as f32)))
        .collect();
    let data = TensorData::new(flat, [n, 4, 3]);
    Tensor::<B, 3>::from_data(data, &device())
}

/// CPU reference for a single tet — used as ground truth for the batched test.
fn p1_local_reference(verts: &[[f64; 3]; 4]) -> ([[f64; 4]; 4], [[f64; 4]; 4], f64) {
    let v = verts;
    let sub = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let e1 = sub(v[1], v[0]);
    let e2 = sub(v[2], v[0]);
    let e3 = sub(v[3], v[0]);

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
    let signed_volume = det / 6.0;
    let abs_det = det.abs();

    let g = [g0, g1, g2, g3];
    let mut k = [[0.0f64; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            k[i][j] = dot(g[i], g[j]) / (6.0 * abs_det);
        }
    }

    let v_elem = abs_det / 6.0;
    let mut m = [[0.0f64; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = if i == j { v_elem / 10.0 } else { v_elem / 20.0 };
        }
    }

    (k, m, signed_volume)
}

/// Deterministic "random" affine tet: translation + diagonally-dominant linear part.
fn deterministic_tet(seed: u32) -> [[f64; 3]; 4] {
    let mut state = seed.wrapping_mul(2_654_435_761);
    let mut next = || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (state as f64) / (u32::MAX as f64)
    };

    let t = [next() - 0.5, next() - 0.5, next() - 0.5];
    let mut a = [[0.0f64; 3]; 3];
    for (i, row) in a.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            *cell = if i == j {
                1.0 + 0.3 * next()
            } else {
                0.2 * (next() - 0.5)
            };
        }
    }

    let ref_v = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut out = [[0.0; 3]; 4];
    for v in 0..4 {
        for d in 0..3 {
            out[v][d] =
                t[d] + a[d][0] * ref_v[v][0] + a[d][1] * ref_v[v][1] + a[d][2] * ref_v[v][2];
        }
    }
    out
}

#[test]
fn reference_unit_tet_stiffness_matches_analytic() {
    let ref_tet = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let coords = coords_tensor_from_vec(&[ref_tet]);
    let result = batched_p1_local_matrices(coords);

    let k = readback_f64(result.k_local);
    // Analytic K for the reference tet, row-major.
    let expected = [
        3.0_f64, -1.0, -1.0, -1.0, -1.0, 1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 1.0,
    ];
    for (idx, (g, e)) in k.iter().zip(expected.iter()).enumerate() {
        let want = e / 6.0;
        assert!((g - want).abs() < F32_TOL, "K[{idx}] = {g}, want {want}");
    }
}

#[test]
fn reference_unit_tet_mass_matches_analytic() {
    let ref_tet = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let coords = coords_tensor_from_vec(&[ref_tet]);
    let result = batched_p1_local_matrices(coords);

    let m = readback_f64(result.m_local);
    // Analytic M = (V/20)*(I + ones) with V = 1/6, row-major.
    let pattern = [
        2.0_f64, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0,
    ];
    for (idx, (g, e)) in m.iter().zip(pattern.iter()).enumerate() {
        let want = e / 120.0; // (V/20) = 1/120, times pattern entry
        assert!((g - want).abs() < F32_TOL, "M[{idx}] = {g}, want {want}");
    }
}

#[test]
fn reference_unit_tet_signed_volume_is_one_sixth() {
    let ref_tet = [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let coords = coords_tensor_from_vec(&[ref_tet]);
    let result = batched_p1_local_matrices(coords);

    let v = readback_f64(result.signed_volumes);
    assert_eq!(v.len(), 1);
    assert!((v[0] - 1.0 / 6.0).abs() < F32_TOL);
}

#[test]
fn batched_random_tets_match_cpu_reference() {
    let n_elem: u32 = 32;
    let tets: Vec<[[f64; 3]; 4]> = (0..n_elem).map(deterministic_tet).collect();

    let coords = coords_tensor_from_vec(&tets);
    let result = batched_p1_local_matrices(coords);

    let k_flat = readback_f64(result.k_local);
    let m_flat = readback_f64(result.m_local);
    let v_flat = readback_f64(result.signed_volumes);

    for (e_idx, tet) in tets.iter().enumerate() {
        let (k_ref, m_ref, v_ref) = p1_local_reference(tet);

        let v_got = v_flat[e_idx];
        let v_tol = F32_TOL * (1.0 + v_ref.abs());
        assert!(
            (v_got - v_ref).abs() < v_tol,
            "elem {e_idx}: V = {v_got}, ref {v_ref}"
        );

        for i in 0..4 {
            for j in 0..4 {
                let k_got = k_flat[(e_idx * 16) + i * 4 + j];
                let k_ref_ij = k_ref[i][j];
                // Stiffness magnitudes can be O(1/V) — scale tolerance to reference.
                let k_tol = F32_TOL * (1.0 + k_ref_ij.abs());
                assert!(
                    (k_got - k_ref_ij).abs() < k_tol,
                    "elem {e_idx} K[{i},{j}]: got {k_got}, ref {k_ref_ij}"
                );

                let m_got = m_flat[(e_idx * 16) + i * 4 + j];
                let m_ref_ij = m_ref[i][j];
                let m_tol = F32_TOL * (1.0 + m_ref_ij.abs());
                assert!(
                    (m_got - m_ref_ij).abs() < m_tol,
                    "elem {e_idx} M[{i},{j}]: got {m_got}, ref {m_ref_ij}"
                );
            }
        }
    }
}

#[test]
fn unit_cube_fixture_volume_conservation() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    let tets: Vec<[[f64; 3]; 4]> = mesh
        .tets
        .iter()
        .map(|t| {
            let mut out = [[0.0; 3]; 4];
            for (slot, &idx) in out.iter_mut().zip(t.iter()) {
                *slot = mesh.nodes[idx as usize];
            }
            out
        })
        .collect();

    let coords = coords_tensor_from_vec(&tets);
    let result = batched_p1_local_matrices(coords);
    let volumes = readback_f64(result.signed_volumes);
    let total: f64 = volumes.iter().sum();
    assert!(
        (total.abs() - 1.0).abs() < 1e-5,
        "|sum signed volumes| = {} (expected 1.0)",
        total.abs()
    );
}

#[test]
fn unit_cube_fixture_invariants() {
    let mesh = GmshReader.read_tet_mesh(UNIT_CUBE_MSH).expect("parse");
    let tets: Vec<[[f64; 3]; 4]> = mesh
        .tets
        .iter()
        .map(|t| {
            let mut out = [[0.0; 3]; 4];
            for (slot, &idx) in out.iter_mut().zip(t.iter()) {
                *slot = mesh.nodes[idx as usize];
            }
            out
        })
        .collect();

    let coords = coords_tensor_from_vec(&tets);
    let result = batched_p1_local_matrices(coords);

    let k_flat = readback_f64(result.k_local);
    let m_flat = readback_f64(result.m_local);
    let v_flat = readback_f64(result.signed_volumes);

    for e in 0..tets.len() {
        let v_e = v_flat[e].abs();

        // K symmetric
        for i in 0..4 {
            for j in (i + 1)..4 {
                let kij = k_flat[e * 16 + i * 4 + j];
                let kji = k_flat[e * 16 + j * 4 + i];
                assert!(
                    (kij - kji).abs() < 1e-5,
                    "K not symmetric at elem {e} ({i},{j}): {kij} vs {kji}"
                );
            }
        }

        // K row sums equal zero (partition of unity: ∇1 = 0 ⇒ K·1 = 0).
        for i in 0..4 {
            let row_sum: f64 = (0..4).map(|j| k_flat[e * 16 + i * 4 + j]).sum();
            assert!(
                row_sum.abs() < 1e-4,
                "K row {i} of elem {e} sums to {row_sum} (expected 0)"
            );
        }

        // M row sums equal V_e (consistent mass: ∫_T φ_i dV = V/4, summed = V).
        for i in 0..4 {
            let row_sum: f64 = (0..4).map(|j| m_flat[e * 16 + i * 4 + j]).sum();
            assert!(
                (row_sum - v_e / 4.0).abs() < 1e-5 * (1.0 + v_e),
                "M row {i} of elem {e}: row_sum={row_sum}, expected V/4 = {}",
                v_e / 4.0
            );
        }
    }
}
