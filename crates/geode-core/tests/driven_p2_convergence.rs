//! Driven `p=2` vs `p=1` order-of-convergence gate (issue #616, Epic
//! #475/#569 — follow-on to #613).
//!
//! Manufactured driven solution on the unit-cube PEC cavity `[0,1]³` with the
//! smooth analytic field
//!
//! ```text
//!   E(x) = (0, 0, sin(πx) sin(πy)),     ∇·E = 0,   n × E = 0 on ∂cube,
//!   ∇×∇×E = 2π² E,
//! ```
//!
//! (the same fixture as `tests/driven_manufactured.rs`, which pins the `p=1`
//! O(h) rate). We drive both the first-order edge path
//! ([`driven_solve_quad`]) and the opt-in second-order path
//! ([`driven_solve_p2`], the [`ElementOrder::P2`] wiring this issue adds) at
//! `ω = π` (below the lowest cavity resonance `2π²`, so `A = K − ω² M` is
//! non-singular) with the manufactured volumetric source
//! `f = (2π² − ω²) E`, then measure the **true L² field error**
//! `‖E_h − E‖_{L²(Ω)}` by quadrature reconstruction — the *same* degree-≥4 tet
//! rule for both orders, so the comparison is apples-to-apples.
//!
//! Gate (mirrors the `fit_slope` refine-and-gate pattern of
//! `tests/nedelec2_convergence.rs`, kept untouched):
//!
//! 1. the `p=2` log-log slope is **strictly better** than `p=1`, and
//! 2. the `p=2` absolute error is below `p=1` at the coarsest shared mesh.

use faer::c64;
use std::f64::consts::PI;

use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::assembly::nedelec_p2::{
    P2DofMap, assemble_p2_rhs_quad, cube_pec_interior_p2_dofs, p2_field_at,
};
use geode_core::driven::solve::{
    DrivenBcs, DrivenMaterials, QuadCurrentSource, driven_solve_p2, driven_solve_quad,
};
use geode_core::elements::nedelec_p2::{tet_barycentric_gradients, tet_quad_deg4};
use geode_core::mesh::{TET_LOCAL_EDGES, cube_tet_mesh};
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Least-squares slope of `log(err)` vs `log(h)` — the empirical convergence
/// order `p` in `err ≈ C h^p` (same estimator as `nedelec2_convergence.rs`).
fn fit_slope(hs: &[f64], errs: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let xs: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ys: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x) * (x - mean_x);
    }
    num / den
}

/// Analytic manufactured field `E = (0, 0, sin(πx) sin(πy))`.
fn e_exact(x: [f64; 3]) -> [f64; 3] {
    [0.0, 0.0, (PI * x[0]).sin() * (PI * x[1]).sin()]
}

const OMEGA: f64 = PI;

/// L² field error of the **p=1** driven solve at refinement `n`.
fn p1_l2_error(n: usize) -> f64 {
    let mesh = cube_tet_mesh(n, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);

    // b = iω ∫ N·J with J = −i f/ω reproduces b = ∫ N·f, f = (2π²−ω²) E.
    let amp = (2.0 * PI * PI - OMEGA * OMEGA) / OMEGA;
    let source = QuadCurrentSource::from_fn(&mesh, |_t, x| {
        let e = e_exact(x);
        [
            c64::new(0.0, -amp * e[0]),
            c64::new(0.0, -amp * e[1]),
            c64::new(0.0, -amp * e[2]),
        ]
    });
    let eps: Vec<c64> = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let sol = driven_solve_quad::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        OMEGA,
        &source,
        &device(),
    )
    .expect("p1 driven solve");
    assert!(sol.residual_rel < 1e-8, "n={n}: p1 residual too high");

    // L² error by the same deg-≥4 rule as p=2, Whitney reconstruction.
    let tet_edges = mesh.tet_edges();
    let rule = tet_quad_deg4();
    let mut err2 = 0.0_f64;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (grad, vol) = tet_barycentric_gradients(&coords);
        let vol_abs = vol.abs();
        // Local signed edge DOFs.
        let dloc: [c64; 6] = std::array::from_fn(|slot| {
            let (gidx, sign) = tet_edges[t][slot];
            sol.e_edges[gidx as usize] * (sign as f64)
        });
        for (lam, frac) in &rule {
            let x: [f64; 3] = std::array::from_fn(|d| (0..4).map(|p| lam[p] * coords[p][d]).sum());
            // Whitney reconstruction E_h = Σ d W, W = λ_a ∇λ_b − λ_b ∇λ_a.
            let mut eh = [c64::new(0.0, 0.0); 3];
            for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
                for (d, eh_d) in eh.iter_mut().enumerate() {
                    let w = lam[a] * grad[b][d] - lam[b] * grad[a][d];
                    *eh_d += dloc[slot] * w;
                }
            }
            let ee = e_exact(x);
            let w = vol_abs * frac;
            for d in 0..3 {
                let diff = eh[d] - c64::new(ee[d], 0.0);
                err2 += w * (diff.re * diff.re + diff.im * diff.im);
            }
        }
    }
    err2.sqrt()
}

/// L² field error of the **p=2** driven solve at refinement `n`.
fn p2_l2_error(n: usize) -> f64 {
    let mesh = cube_tet_mesh(n, 1.0);
    let dofs = P2DofMap::build(&mesh);
    let eps_tet = vec![1.0_f64; mesh.n_tets()];
    let interior = cube_pec_interior_p2_dofs(&mesh, &dofs, 1.0);

    // rhs_full = ∫ N·f, f = (2π²−ω²) E (real; A is real so x is real).
    let amp = 2.0 * PI * PI - OMEGA * OMEGA;
    let b_re = assemble_p2_rhs_quad(&mesh, &dofs, |_t, x| {
        let e = e_exact(x);
        [amp * e[0], amp * e[1], amp * e[2]]
    });
    let rhs_full: Vec<c64> = b_re.iter().map(|&v| c64::new(v, 0.0)).collect();

    let sol =
        driven_solve_p2(&mesh, &eps_tet, &interior, OMEGA, &rhs_full).expect("p2 driven solve");
    assert!(sol.residual_rel < 1e-8, "n={n}: p2 residual too high");

    let rule = tet_quad_deg4();
    let mut err2 = 0.0_f64;
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords: [[f64; 3]; 4] = std::array::from_fn(|i| mesh.nodes[tet[i] as usize]);
        let (_grad, vol) = tet_barycentric_gradients(&coords);
        let vol_abs = vol.abs();
        for (lam, frac) in &rule {
            let x: [f64; 3] = std::array::from_fn(|d| (0..4).map(|p| lam[p] * coords[p][d]).sum());
            let eh = p2_field_at(&mesh, &dofs, &sol.x, t, lam);
            let ee = e_exact(x);
            let w = vol_abs * frac;
            for d in 0..3 {
                let diff = eh[d] - c64::new(ee[d], 0.0);
                err2 += w * (diff.re * diff.re + diff.im * diff.im);
            }
        }
    }
    err2.sqrt()
}

#[test]
fn p2_driven_converges_faster_than_p1_on_cube_cavity() {
    let ns = [2usize, 3, 4];
    let hs: Vec<f64> = ns.iter().map(|&n| 1.0 / n as f64).collect();
    let err_p1: Vec<f64> = ns.iter().map(|&n| p1_l2_error(n)).collect();
    let err_p2: Vec<f64> = ns.iter().map(|&n| p2_l2_error(n)).collect();

    let slope_p1 = fit_slope(&hs, &err_p1);
    let slope_p2 = fit_slope(&hs, &err_p2);
    eprintln!("p1 L2 errors: {err_p1:?}  slope = {slope_p1:.3}");
    eprintln!("p2 L2 errors: {err_p2:?}  slope = {slope_p2:.3}");

    // (1) p=2 slope strictly better than p=1 (with margin against noise).
    assert!(
        slope_p2 > slope_p1 + 0.5,
        "p=2 slope {slope_p2:.3} not strictly better than p=1 slope {slope_p1:.3}"
    );
    // p=2 should be at least ~O(h^1.5) (asymptotic O(h²) on coarse meshes).
    assert!(
        slope_p2 >= 1.5,
        "p=2 convergence slope {slope_p2:.3} below the expected order"
    );
    // (2) p=2 absolute error below p=1 at the coarsest shared mesh.
    assert!(
        err_p2[0] < err_p1[0],
        "p=2 coarse error {} not below p=1 coarse error {}",
        err_p2[0],
        err_p1[0]
    );
}
