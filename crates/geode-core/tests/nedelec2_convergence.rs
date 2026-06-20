//! Manufactured order-of-convergence gate for the p=2 (second-order)
//! Nédélec transverse modal element (Epic #318 Phase 2.5C).
//!
//! The "manufactured" target is the **rectangular PEC cavity TE/TM cutoff
//! spectrum**, whose eigenvalues `k_c² = (mπ/a)² + (nπ/b)²` are known in
//! closed form (`rect_waveguide_cutoff`). On a sequence of uniformly
//! refined `rect_tri_mesh` meshes we measure the eigenvalue error of the
//! lowest mode (TE₁₀) for the first-order Whitney element and the
//! second-order element, fit a log-log slope `error ∝ h^p`, and assert:
//!
//! 1. the p=2 convergence rate is ≈ O(h²) (slope ≳ 1.8), and
//! 2. it is **strictly faster** than the p=1 rate on the *identical*
//!    meshes (the whole point of the epic), and
//! 3. at the finest mesh the p=2 error is below the p=1 error.
//!
//! This mirrors the structure of `tests/eigensolver.rs` /
//! `tests/driven_manufactured.rs` (refine, fit a slope, gate on the rate).
//!
//! Running:
//! ```sh
//! cargo test -p geode-core --test nedelec2_convergence
//! ```

use geode_core::{
    rect_pec_interior_dofs2, rect_pec_interior_edges, rect_tri_mesh, rect_waveguide_cutoff,
    solve_rect_waveguide_modes, solve_rect_waveguide_modes2_cutoffs,
};

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

/// p=2 vs p=1 manufactured order-of-convergence on the TE₁₀ cavity cutoff.
#[test]
fn p2_converges_faster_than_p1_on_te10_cutoff() {
    // WR-90-aspect cavity (a/b = 2). The lowest cutoff is TE₁₀ at
    // k_c² = (π/a)².
    let (a, b) = (2.0_f64, 1.0_f64);
    let kc2_exact = {
        let kc = rect_waveguide_cutoff(1, 0, a, b);
        kc * kc
    };

    // A sequence of uniformly refined structured meshes. h ∝ 1/ny.
    // Kept modest (the p=2 dense assembly/solve is ~4× the p=1 system and
    // runs in debug); the rate is already unambiguous over these levels.
    let nys = [3usize, 4, 6];

    let mut hs = Vec::new();
    let mut err_p1 = Vec::new();
    let mut err_p2 = Vec::new();

    for &ny in &nys {
        let nx = 2 * ny; // keep aspect cells square (a/b = 2)
        let mesh = rect_tri_mesh(nx, ny, a, b);
        let h = b / ny as f64;
        hs.push(h);

        // p=1: lowest physical cutoff from the Whitney solver.
        let modes_p1 = solve_rect_waveguide_modes(&mesh, a, b, 1).expect("p=1 modal solve");
        let kc2_p1 = modes_p1[0].lambda;
        let e1 = (kc2_p1 - kc2_exact).abs() / kc2_exact;
        err_p1.push(e1);

        // p=2: lowest physical cutoff from the second-order solver.
        let dof_mask = rect_pec_interior_dofs2(&mesh, a, b);
        let cutoffs_p2 =
            solve_rect_waveguide_modes2_cutoffs(&mesh, &dof_mask, 1).expect("p=2 modal solve");
        let kc2_p2 = cutoffs_p2[0];
        let e2 = (kc2_p2 - kc2_exact).abs() / kc2_exact;
        err_p2.push(e2);

        // Sanity: the p=2 interior-DOF count is the p=2 system size.
        let (_e, edge_int) = rect_pec_interior_edges(&mesh, a, b);
        let n_int_p1 = edge_int.iter().filter(|&&x| x).count();
        let n_int_p2 = dof_mask.iter().filter(|&&x| x).count();

        eprintln!(
            "ny={ny:>2} h={h:.4}: p1 err={e1:.3e} (dofs {n_int_p1}), \
             p2 err={e2:.3e} (dofs {n_int_p2})"
        );
    }

    let slope_p1 = fit_slope(&hs, &err_p1);
    let slope_p2 = fit_slope(&hs, &err_p2);
    eprintln!("convergence slopes: p1 = {slope_p1:.3}, p2 = {slope_p2:.3}");

    // 1. p=2 achieves (at least) ~O(h²). Whitney cavity eigenvalue
    //    convergence is O(h²); the p=2 element should reach ~O(h⁴) on
    //    eigenvalues but we gate conservatively at ≳ 3.0 to leave room for
    //    the coarse-mesh pre-asymptotic regime and Lanczos tolerance.
    assert!(
        slope_p2 >= 3.0,
        "p=2 cutoff eigenvalue convergence slope {slope_p2:.3} below the expected \
         O(h^≳3) rate (errors {err_p2:?})"
    );

    // 2. p=2 is strictly faster than p=1 on the identical meshes.
    assert!(
        slope_p2 > slope_p1 + 0.5,
        "p=2 slope {slope_p2:.3} not strictly faster than p=1 slope {slope_p1:.3}"
    );

    // 3. At the finest mesh the p=2 error is well below p=1.
    let last = nys.len() - 1;
    assert!(
        err_p2[last] < err_p1[last],
        "finest-mesh p=2 error {:.3e} not below p=1 error {:.3e}",
        err_p2[last],
        err_p1[last]
    );
}

/// Metallic regression: the p=2 element reproduces the TE₁₀ / TE₂₀ / TE₀₁
/// cutoffs at least as accurately as p=1 on a fixed moderate mesh.
#[test]
fn p2_metallic_cutoffs_at_least_as_accurate_as_p1() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(12, 6, a, b);

    // Analytic catalog roots for the lowest modes.
    let te10 = rect_waveguide_cutoff(1, 0, a, b);
    let te20 = rect_waveguide_cutoff(2, 0, a, b);
    let te01 = rect_waveguide_cutoff(0, 1, a, b);
    // TE₂₀ and TE₀₁ are exactly degenerate at a/b = 2 (k_c = π).
    assert!((te20 - te01).abs() < 1e-12);

    // p=1 cutoffs.
    let modes_p1 = solve_rect_waveguide_modes(&mesh, a, b, 3).expect("p=1 modal solve");
    let kc_p1: Vec<f64> = modes_p1.iter().map(|m| m.k_c).collect();

    // p=2 cutoffs (eigenvalues → k_c).
    let dof_mask = rect_pec_interior_dofs2(&mesh, a, b);
    let kc_p2: Vec<f64> = solve_rect_waveguide_modes2_cutoffs(&mesh, &dof_mask, 3)
        .expect("p=2 modal solve")
        .iter()
        .map(|l| l.sqrt())
        .collect();

    eprintln!("p1 k_c = {kc_p1:?}");
    eprintln!("p2 k_c = {kc_p2:?}");

    // Pair the lowest FEM cutoff to TE₁₀ and compare relative errors.
    let err_p1_te10 = (kc_p1[0] - te10).abs() / te10;
    let err_p2_te10 = (kc_p2[0] - te10).abs() / te10;
    eprintln!(
        "TE10: p1 err {:.3e}, p2 err {:.3e}",
        err_p1_te10, err_p2_te10
    );

    // p=2 must be at least as accurate as p=1 on the dominant mode.
    assert!(
        err_p2_te10 <= err_p1_te10 + 1e-12,
        "p=2 TE10 error {err_p2_te10:.3e} worse than p=1 {err_p1_te10:.3e}"
    );

    // And the p=2 dominant cutoff is itself within 1% of analytic.
    assert!(
        err_p2_te10 < 0.01,
        "p=2 TE10 cutoff error {err_p2_te10:.3e} exceeds 1%"
    );

    // The next p=2 cutoff pairs to the degenerate TE₂₀/TE₀₁ root within 2%.
    let err_p2_next = (kc_p2[1] - te20).abs() / te20;
    assert!(
        err_p2_next < 0.02,
        "p=2 second cutoff {:.5} does not pair to TE20/TE01 ({:.5}) within 2% ({:.3e})",
        kc_p2[1],
        te20,
        err_p2_next
    );
}
