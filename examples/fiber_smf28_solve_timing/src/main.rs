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
//!   cargo run --release -p fiber_smf28_solve_timing
//!
//! This is an Epic #398 standalone example crate
//! (`examples/fiber_smf28_solve_timing/`), migrated from the old
//! `crates/geode-core/examples/fiber_smf28_solve_timing.rs`. The timing body
//! is preserved exactly; only the entry point changed (hand-rolled `fn main`
//! → `clap` derive + `geode_app::App`).

use std::process::ExitCode;
use std::time::Instant;

use clap::Parser;
use geode_app::{App, Verbosity};
use geode_core::analytic::waveguide::{
    disk_pec_interior_dofs2, disk_tri_mesh, epsilon_r_from_region_tags, n_dof_2d_nedelec2,
    solve_dielectric_modes2,
};

/// SMF-28 fiber p=2 solve-timing harness CLI.
///
/// The original example took no arguments; this flattens the shared
/// `geode-app` `-v`/`-q` verbosity group and keeps the timing body identical.
#[derive(Parser)]
#[command(
    about = "Performance harness timing the SMF-28 fiber p=2 modal solve on the sparse-direct path (issue #327)."
)]
struct Args {
    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
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
        let modes = solve_dielectric_modes2(&mesh, &eps_r, &dof_mask, k0, 1)
            .expect("p=2 SMF-28 fiber solve");
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
        Ok(())
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<Args>()
}
