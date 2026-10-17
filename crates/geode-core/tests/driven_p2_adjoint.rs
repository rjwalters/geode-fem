//! Finite-difference validation of the **second-order (`p=2`)** driven-Nédélec
//! material discrete adjoint `∂g/∂ε` (issue #616, Epic #569 — the `p=2`
//! retention of the #576 material sensitivity).
//!
//! Mirrors the `p=1` driven-adjoint test bar (`src/driven/adjoint.rs` module
//! tests): a lossless real-ε driven cube cavity, the L² observable
//! `g = Σ_i |x_i|²`, a two-region design partition, and a central finite
//! difference of the whole `p=2` forward pipeline
//! ([`driven_solve_p2`]) against the single-solve adjoint gradient
//! ([`driven_material_adjoint_gradient_p2`]).

use faer::c64;
use std::f64::consts::PI;

use geode_core::assembly::nedelec_p2::{P2DofMap, assemble_p2_rhs_quad, cube_pec_interior_p2_dofs};
use geode_core::driven::adjoint::driven_material_adjoint_gradient_p2;
use geode_core::driven::solve::driven_solve_p2;
use geode_core::mesh::cube_tet_mesh;

/// Two design regions split at the cube mid-plane `x = 0.5`.
fn region_of_tet(mesh: &geode_core::mesh::TetMesh) -> Vec<usize> {
    mesh.tets
        .iter()
        .map(|tet| {
            let cx = tet.iter().map(|&v| mesh.nodes[v as usize][0]).sum::<f64>() / 4.0;
            usize::from(cx > 0.5)
        })
        .collect()
}

/// L² observable `g = Σ_i |x_i|²` and its Wirtinger cotangent `∂g/∂x_i = x̄_i`.
fn objective(x: &[c64]) -> (f64, Vec<c64>) {
    let g = x.iter().map(|z| z.norm_sqr()).sum::<f64>();
    let dg = x.iter().map(|z| z.conj()).collect();
    (g, dg)
}

#[test]
fn p2_material_adjoint_matches_finite_difference() {
    let mesh = cube_tet_mesh(2, 1.0);
    let dofs = P2DofMap::build(&mesh);
    let interior = cube_pec_interior_p2_dofs(&mesh, &dofs, 1.0);
    let omega = PI;
    let regions = region_of_tet(&mesh);
    let n_regions = 2;

    // ε per region: region 0 = 2.0, region 1 = 3.0.
    let eps_region = [2.0_f64, 3.0];
    let eps_r: Vec<f64> = regions.iter().map(|&r| eps_region[r]).collect();

    // ε-independent RHS: a constant volumetric current source (real → x real).
    let b_re = assemble_p2_rhs_quad(&mesh, &dofs, |_t, _x| [0.3, 0.5, 0.7]);
    let rhs_full: Vec<c64> = b_re.iter().map(|&v| c64::new(v, 0.0)).collect();

    // Single-solve adjoint gradient.
    let adj = driven_material_adjoint_gradient_p2(
        &mesh, &eps_r, &interior, omega, &rhs_full, &regions, n_regions, objective,
    )
    .expect("p2 material adjoint");
    assert!(
        adj.residual_rel < 1e-8,
        "adjoint forward residual {} too high",
        adj.residual_rel
    );
    assert_eq!(adj.n_factorizations, 1);

    // Central finite difference of the full forward pipeline, per region.
    let fd_h = 1e-6;
    let g_of = |eps_reg: &[f64; 2]| -> f64 {
        let eps: Vec<f64> = regions.iter().map(|&r| eps_reg[r]).collect();
        let sol =
            driven_solve_p2(&mesh, &eps, &interior, omega, &rhs_full).expect("p2 driven forward");
        objective(&sol.x).0
    };

    for k in 0..n_regions {
        let mut ep = eps_region;
        let mut em = eps_region;
        ep[k] += fd_h;
        em[k] -= fd_h;
        let grad_fd = (g_of(&ep) - g_of(&em)) / (2.0 * fd_h);
        let rel = (adj.grad[k] - grad_fd).abs() / grad_fd.abs().max(f64::MIN_POSITIVE);
        eprintln!(
            "region {k}: adjoint = {:.6e}, FD = {grad_fd:.6e}, rel-err = {rel:.3e}",
            adj.grad[k]
        );
        assert!(
            rel < 1e-4,
            "region {k}: p2 adjoint ∂g/∂ε rel-err {rel:.3e} exceeds 1e-4 \
             (adjoint {:.6e} vs FD {grad_fd:.6e})",
            adj.grad[k]
        );
    }
}
