//! 3D order-of-convergence gate for the second-order (`p=2`) Nédélec
//! **cavity eigenmode** path (Epic #475 parity gap #3, Epic #569; issue
//! #620, follow-on to #616/#621).
//!
//! The oracle is the **PEC cube cavity** `[0, 1]³`, whose vector curl-curl
//! spectrum is `λ = π²(m² + n² + p²)` with **at most one zero index**, so
//! the lowest physical eigenvalue is `λ₁ = 2π²` (multiplicity 3) — *not*
//! `3π²` (that is the scalar Dirichlet Laplacian). See
//! `tests/nedelec_cavity.rs` for the full mode-counting story.
//!
//! On a coarse cube refinement ladder we compute the lowest physical
//! eigenvalue at `p=1` and `p=2` (via [`solve_pec_cube_cavity_modes`],
//! filtering the curl-free gradient near-kernel), fit a log-log slope
//! `error ∝ h^p`, and assert:
//!
//! 1. the `p=2` convergence rate is meaningfully steeper than `p=1`, and
//! 2. at the finest mesh the `p=2` frequency error is below the `p=1` error.
//!
//! This mirrors the refine-and-gate structure of
//! `tests/nedelec2_convergence.rs` (the 2D transverse gate, left untouched);
//! this is the **separate** 3D eigen-frequency gate the issue calls for.
//!
//! It runs in **debug** via the sparse shift-invert Lanczos path (the dense
//! `FaerDenseEigensolver` panics under `debug-assertions` through faer's
//! `qz_real`). The cube is kept coarse (`n = 2, 3, 4`) so the `p=2` solve
//! (~4× the `p=1` DOF count) stays CI-fast.
//!
//! Running:
//! ```sh
//! cargo test -p geode-core --test nedelec_cavity_p2_convergence
//! ```

use geode_core::eigen::cavity::{ElementOrder, solve_pec_cube_cavity_modes};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::TestBackend;

use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Least-squares slope of `log(err)` vs `log(h)` (the empirical
/// convergence order `p` in `err ≈ C · h^p`).
fn fit_slope(hs: &[f64], errs: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let xs: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ys: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        num += (x - mean_x) * (y - mean_y);
        den += (x - mean_x) * (x - mean_x);
    }
    num / den
}

/// `p=2` vs `p=1` order-of-convergence on the PEC-cube first cavity mode.
#[test]
fn p2_cavity_frequency_converges_faster_than_p1() {
    let side = 1.0_f64;
    let two_pi2 = 2.0 * std::f64::consts::PI.powi(2); // analytic λ₁

    // Shift placed strictly above 0 and below λ₁ so `A = K − σM` is
    // non-singular and the physical band converges ahead of the (larger at
    // p=2) near-zero gradient nullspace. `null_tol` separates the physical
    // spectrum from the near-zero cluster (well below λ₁, well above the
    // gradient cluster).
    let sigma = 0.7 * two_pi2;
    let null_tol = 0.5 * two_pi2;

    // Coarse refinement ladder; h ∝ 1/n. Modest because the p=2 system is
    // ~4× the p=1 DOF count and the whole gate runs in debug.
    let ns = [2usize, 3, 4];

    let mut hs = Vec::new();
    let mut err_p1 = Vec::new();
    let mut err_p2 = Vec::new();

    for &n in &ns {
        let mesh = cube_tet_mesh(n, side);
        let h = side / n as f64;
        hs.push(h);

        // Request enough modes to clear the gradient nullspace and surface
        // the (triply-degenerate) first physical band.
        let n_modes = 12;

        let modes_p1 = solve_pec_cube_cavity_modes::<B>(
            &mesh,
            side,
            &device(),
            ElementOrder::P1,
            sigma,
            n_modes,
        )
        .expect("p=1 cavity eigensolve");
        let l1_p1 = modes_p1
            .first_physical(null_tol)
            .expect("p=1 physical mode above null_tol");
        let e1 = (l1_p1 - two_pi2).abs() / two_pi2;
        err_p1.push(e1);

        let modes_p2 = solve_pec_cube_cavity_modes::<B>(
            &mesh,
            side,
            &device(),
            ElementOrder::P2,
            sigma,
            n_modes,
        )
        .expect("p=2 cavity eigensolve");
        let l1_p2 = modes_p2
            .first_physical(null_tol)
            .expect("p=2 physical mode above null_tol");
        let e2 = (l1_p2 - two_pi2).abs() / two_pi2;
        err_p2.push(e2);

        eprintln!(
            "n={n} h={h:.4}: p1 λ₁={l1_p1:.4} (err {e1:.3e}, {n1} int dofs), \
             p2 λ₁={l1_p2:.4} (err {e2:.3e}, {n2} int dofs)",
            n1 = modes_p1.n_interior,
            n2 = modes_p2.n_interior,
        );
    }

    let slope_p1 = fit_slope(&hs, &err_p1);
    let slope_p2 = fit_slope(&hs, &err_p2);
    eprintln!(
        "PEC-cube λ₁ convergence slopes: p1 = {slope_p1:.3}, p2 = {slope_p2:.3} \
         (err_p1 {err_p1:?}, err_p2 {err_p2:?})"
    );

    // 1. p=2 is meaningfully steeper than p=1 on the identical meshes — the
    //    whole point of wiring the second-order element into the eigen path.
    //    Band chosen from the observed run (honest, conservative margin).
    assert!(
        slope_p2 > slope_p1 + 0.5,
        "p=2 cavity λ₁ slope {slope_p2:.3} not meaningfully steeper than p=1 {slope_p1:.3} \
         (err_p1 {err_p1:?}, err_p2 {err_p2:?})"
    );

    // 2. At the finest mesh the p=2 frequency error is below p=1.
    let last = ns.len() - 1;
    assert!(
        err_p2[last] < err_p1[last],
        "finest-mesh p=2 error {:.3e} not below p=1 error {:.3e}",
        err_p2[last],
        err_p1[last]
    );
}
