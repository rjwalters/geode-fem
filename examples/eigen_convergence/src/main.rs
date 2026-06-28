//! Quick convergence probe — prints (1,1,1) ground-mode eigenvalue for
//! a few mesh refinements so we can see the O(h²) rate.
//!
//! Run with:  `cargo run -p eigen_convergence --release`

use std::process::ExitCode;

use burn::tensor::backend::BackendTypes;
use clap::Parser;
use geode_app::{App, Verbosity};
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::testing::TestBackend;
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing;

type B = TestBackend;

/// Ground-mode convergence probe for the unit-cube Dirichlet Laplacian.
#[derive(Parser)]
#[command(
    about = "Convergence probe: (1,1,1) ground-mode eigenvalue vs mesh refinement (O(h²) rate)."
)]
struct Args {
    /// Verbosity (`-v` / `-q`), fed to the logging seam.
    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let device = <B as BackendTypes>::Device::default();
        let analytic = 3.0 * std::f64::consts::PI.powi(2);

        let pi2 = std::f64::consts::PI.powi(2);
        let targets = [3.0 * pi2, 6.0 * pi2, 6.0 * pi2, 6.0 * pi2, 9.0 * pi2];

        println!("Ground-mode convergence:");
        println!("n   h      n_int   λ_h        λ/(π²)   err vs analytic");
        println!("--  -----  ------  ---------  -------  ----------------");
        for n in [3, 4, 5, 6, 8, 10, 12] {
            let mesh = cube_tet_mesh(n, 1.0);
            let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
            let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
            let k = burn_matrix_to_faer(sys.k);
            let m = burn_matrix_to_faer(sys.m);
            let mask = cube_interior_mask(&mesh.nodes, 1.0);
            let (k_int, m_int) = apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask)?;
            let n_int = k_int.nrows();
            let lambdas =
                FaerDenseEigensolver.smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), 1)?;
            let lam = lambdas[0];
            let rel = (lam - analytic) / analytic * 100.0;
            let h = 1.0 / n as f64;
            println!(
                "{n:<2}  {h:.3}  {n_int:<6}  {lam:9.4}  {:.4}  {rel:+.4}%",
                lam / std::f64::consts::PI.powi(2)
            );
        }

        println!();
        println!("Lowest 5 eigenvalues at n=10 (h=0.1):");
        println!("idx  target/π²   λ_h/π²    rel err");
        println!("---  ---------   ------    ---------");
        let mesh = cube_tet_mesh(10, 1.0);
        let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
        let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
        let k = burn_matrix_to_faer(sys.k);
        let m = burn_matrix_to_faer(sys.m);
        let mask = cube_interior_mask(&mesh.nodes, 1.0);
        let (k_int, m_int) = apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask)?;
        let lambdas =
            FaerDenseEigensolver.smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), 5)?;
        for (i, (got, want)) in lambdas.iter().zip(targets.iter()).enumerate() {
            let rel = (got - want).abs() / want * 100.0;
            println!(
                "{i:<3}  {:.4}      {:.4}    {rel:+.4}%",
                want / pi2,
                got / pi2,
            );
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
