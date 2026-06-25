//! Issue #327 performance harness: time the SMF-28 fiber p=2 modal solve at
//! the #322 probe configuration (~31k DOF) on the **sparse-direct** assembly
//! path.
//!
//! The #322 probe measured ~566 s on the previous **dense** assembly path
//! (which materialized two ~7.6 GB N×N `Mat<f64>` operators and scanned all
//! N² entries in `dense_to_sparse`). This harness reports the AFTER
//! (sparse-direct) wall-clock so the speedup can be quoted.
//!
//! Run with:
//!   cargo run --release -p geode-core --example fiber_smf28_solve_timing

use std::time::Instant;

use geode_core::analytic::waveguide::{
    disk_pec_interior_dofs2, disk_tri_mesh, epsilon_r_from_region_tags, n_dof_2d_nedelec2,
    solve_dielectric_modes2,
};

fn main() {
    // SMF-28 step-index fiber, #322 probe config.
    let n_core = 1.4504_f64;
    let n_clad = 1.4447_f64;
    let a = 4.1e-6_f64; // core radius (m)
    let lambda = 1.55e-6_f64;
    let k0 = 2.0 * std::f64::consts::PI / lambda;

    let outer = 10.0 * a;
    let (mesh, region_tags) = disk_tri_mesh(a, outer, 12, 132);
    let eps_r = epsilon_r_from_region_tags(&region_tags, |t| {
        if t == 1 {
            n_core * n_core
        } else {
            n_clad * n_clad
        }
    });
    let dof_mask = disk_pec_interior_dofs2(&mesh, outer);
    let n_dof = n_dof_2d_nedelec2(&mesh);

    eprintln!(
        "SMF-28 fiber: n_core={n_core}, n_clad={n_clad}, a={a:.3e} m, λ={lambda:.3e} m; \
         mesh disk_tri_mesh(a, 10a, 12, 132): {} tris, {} edges, n_dof(p=2)={n_dof}",
        mesh.n_tris(),
        mesh.edges().len(),
    );

    let t0 = Instant::now();
    let modes =
        solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1).expect("p=2 SMF-28 fiber solve");
    let dt = t0.elapsed();

    eprintln!(
        "AFTER (sparse-direct) solve_dielectric_modes2 wall-clock: {:.3} s",
        dt.as_secs_f64()
    );
    if let Some(m) = modes.first() {
        eprintln!("  fundamental n_eff = {:.6} (guided={})", m.n_eff, m.guided);
    } else {
        eprintln!("  (no bound mode returned)");
    }
}
