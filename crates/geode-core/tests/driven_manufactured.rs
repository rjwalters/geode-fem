//! Manufactured-solution validation of the driven solve (issue #194).
//!
//! # Manufactured solution
//!
//! On the unit PEC cube `[0,1]³` choose the analytic field
//!
//! ```text
//! E(x, y, z) = (0, 0, ψ),     ψ = sin(πx) sin(πy),
//! ```
//!
//! which satisfies `n × E = 0` on all six faces (E_x = E_y = 0
//! everywhere; E_z vanishes on the four faces where ẑ is tangential).
//! Since `∇·E = ∂_z ψ = 0`, the double curl reduces to `−ΔE`:
//!
//! ```text
//! ∇×∇×E = (0, 0, 2π² ψ).
//! ```
//!
//! The driven solve's strong form (natural units, `exp(+jωt)`; see
//! `geode_core::driven` module docs) is `∇×∇×E − ω² E = iω J`, so the
//! current that manufactures `E` is
//!
//! ```text
//! J = (2π² − ω²) / (iω) · (0, 0, ψ) = −i (2π² − ω²)/ω · (0, 0, ψ).
//! ```
//!
//! We drive at `ω = π` (`ω² = π² ≈ 9.87`), comfortably inside the gap
//! between the gradient nullspace at λ = 0 and the lowest physical PEC
//! cavity resonance at `λ = 2π² ≈ 19.74`, so `A(ω) = K − ω²M` is far
//! from singular at every refinement level.
//!
//! # Error metric and expected order
//!
//! The discrete solution `x_h` is compared against the Nédélec edge
//! interpolant of the analytic field, `x_a[e] = ∫_e E · dl` (4-point
//! Gauss–Legendre per edge), in the mass-matrix norm
//! `‖v‖_M = sqrt(v^H M v)` — a discrete L²(Ω) norm. For lowest-order
//! Nédélec elements the L² convergence rate is **O(h)**; the
//! solution-to-interpolant distance measured here may superconverge on
//! the structured cube mesh, so the acceptance bound is `order ≥ 0.9`
//! with the observed order printed for the record.
//!
//! `J` is sampled per-tet at centroids (midpoint quadrature), an O(h²)
//! consistency perturbation that does not limit the O(h) rate.

use burn::tensor::backend::BackendTypes;
use faer::c64;
use std::f64::consts::PI;

use geode_core::assembly::nedelec::{assemble_global_nedelec, cube_pec_interior_edges};
use geode_core::assembly::p1::upload_mesh;
use geode_core::backend::DefaultBackend;
use geode_core::driven::solve::{CurrentSource, DrivenBcs, DrivenMaterials, driven_solve};
use geode_core::eigen::dense::burn_matrix_to_faer;
use geode_core::elements::nedelec::batched_nedelec_local_rhs;
use geode_core::mesh::cube_tet_mesh;

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// 4-point Gauss–Legendre nodes/weights on [0, 1].
const GAUSS_4: [(f64, f64); 4] = [
    (0.5 - 0.430568155797026, 0.173927422568727),
    (0.5 - 0.169990521792428, 0.326072577431273),
    (0.5 + 0.169990521792428, 0.326072577431273),
    (0.5 + 0.430568155797026, 0.173927422568727),
];

fn psi(x: f64, y: f64) -> f64 {
    (PI * x).sin() * (PI * y).sin()
}

/// Analytic edge DOF `∫_e E · dl` for `E = (0, 0, ψ)`, integrated from
/// the lower-tagged endpoint to the higher-tagged endpoint (the global
/// canonical edge direction).
fn analytic_edge_dof(p: [f64; 3], q: [f64; 3]) -> f64 {
    let dz = q[2] - p[2];
    if dz == 0.0 {
        return 0.0;
    }
    let mut acc = 0.0;
    for &(t, w) in GAUSS_4.iter() {
        let x = p[0] + t * (q[0] - p[0]);
        let y = p[1] + t * (q[1] - p[1]);
        acc += w * psi(x, y);
    }
    acc * dz
}

/// Run the manufactured-solution driven solve at refinement `n` and
/// return the relative M-norm error against the analytic interpolant.
fn manufactured_error(n: usize) -> f64 {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);

    let omega = PI;
    // J = −i (2π² − ω²)/ω (0, 0, ψ), sampled at tet centroids.
    let j_amp = -(2.0 * PI * PI - omega * omega) / omega;
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new(0.0, j_amp * psi(c[0], c[1])),
        ]
    });

    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let sol = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        omega,
        &source,
        &device(),
    )
    .expect("driven solve");
    assert!(
        sol.residual_rel < 1e-8,
        "n={n}: direct-solve residual {} above floor",
        sol.residual_rel
    );

    // Analytic interpolant DOFs (zero on PEC edges by construction of E).
    let x_a: Vec<f64> = edges
        .iter()
        .map(|e| analytic_edge_dof(mesh.nodes[e[0] as usize], mesh.nodes[e[1] as usize]))
        .collect();

    // M-norm error: assemble the vacuum mass for the discrete L² metric.
    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    let (nodes_t, tets_t) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_nedelec(nodes_t, tets_t, &tet_idx, &tet_sign, edges.len());
    let m = burn_matrix_to_faer(sys.m);

    // err² = e^H M e with e = x_h − x_a; reference ‖x_a‖²_M = x_a^T M x_a.
    let n_e = edges.len();
    let e_vec: Vec<c64> = (0..n_e)
        .map(|i| sol.e_edges[i] - c64::new(x_a[i], 0.0))
        .collect();
    let mut err2 = 0.0_f64;
    let mut ref2 = 0.0_f64;
    for i in 0..n_e {
        let mut me = c64::new(0.0, 0.0);
        let mut ma = 0.0_f64;
        for j in 0..n_e {
            let mij = m[(i, j)];
            if mij != 0.0 {
                me += e_vec[j] * mij;
                ma += x_a[j] * mij;
            }
        }
        // e^H M e — conjugate the left factor.
        err2 += e_vec[i].re * me.re + e_vec[i].im * me.im;
        ref2 += x_a[i] * ma;
    }
    assert!(ref2 > 0.0, "analytic interpolant must be nonzero");
    let rel = (err2.max(0.0) / ref2).sqrt();
    eprintln!(
        "n = {n:>2}: h = {:.4}, relative M-norm error = {rel:.6e}",
        1.0 / n as f64
    );
    rel
}

#[test]
fn manufactured_solution_converges_under_refinement() {
    let errs: Vec<f64> = [2_usize, 4, 8]
        .iter()
        .map(|&n| manufactured_error(n))
        .collect();

    // Errors must decrease monotonically under refinement.
    assert!(
        errs[1] < errs[0] && errs[2] < errs[1],
        "errors not monotonically decreasing: {errs:?}"
    );

    let order_24 = (errs[0] / errs[1]).log2();
    let order_48 = (errs[1] / errs[2]).log2();
    eprintln!("observed convergence order: 2→4 = {order_24:.3}, 4→8 = {order_48:.3}");

    // Lowest-order Nédélec gives O(h) in L²; superconvergence against
    // the interpolant on the structured mesh may push this toward 2.
    assert!(
        order_48 > 0.9,
        "observed order {order_48:.3} below the O(h) acceptance floor (errors: {errs:?})"
    );
}

/// Hand-check of the local RHS kernel on the reference tet
/// (0,0,0)-(1,0,0)-(0,1,0)-(0,0,1): with `det J = 1`, `V = 1/6`, and
/// `∇λ = {(-1,-1,-1), x̂, ŷ, ẑ}`,
///
/// ```text
/// ∫_T N_i dV = (V/4)(∇λ_b − ∇λ_a) = (1/24)(∇λ_b − ∇λ_a).
/// ```
#[test]
fn local_rhs_reference_tet_hand_values() {
    use burn::tensor::{Tensor, TensorData};

    let dev = device();
    let coords = Tensor::<B, 3>::from_data(
        TensorData::new(
            vec![
                0.0_f32, 0.0, 0.0, // v0
                1.0, 0.0, 0.0, // v1
                0.0, 1.0, 0.0, // v2
                0.0, 0.0, 1.0, // v3
            ],
            [1, 4, 3],
        ),
        &dev,
    );

    // ∇λ per vertex on the reference tet.
    let grad = [
        [-1.0_f64, -1.0, -1.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let local_edges = [(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)];

    for (axis, j_vec) in [
        [1.0_f32, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.5, -2.0, 3.0],
    ]
    .iter()
    .enumerate()
    {
        let j = Tensor::<B, 2>::from_data(TensorData::new(j_vec.to_vec(), [1, 3]), &dev);
        let rhs = batched_nedelec_local_rhs(coords.clone(), j);
        let got: Vec<f64> = rhs.into_data().iter::<f64>().collect();
        assert_eq!(got.len(), 6);

        for (i, &(a, b)) in local_edges.iter().enumerate() {
            let want: f64 = (0..3)
                .map(|k| (grad[b][k] - grad[a][k]) * j_vec[k] as f64)
                .sum::<f64>()
                / 24.0;
            assert!(
                (got[i] - want).abs() < 1e-6,
                "case {axis}, local edge {i}: got {}, want {want}",
                got[i]
            );
        }
    }
}
