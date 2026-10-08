//! faer's process-global parallelism: the guard, the scope and the two solve
//! loops that hold a scope (issues #518 and #946).
//!
//! faer takes the thread count of a sparse factorization or triangular solve
//! from one process-global setting. geode changes it in two ways:
//!
//! - [`geode_core::eigen::parallel::ParallelismGuard`] caps it around a
//!   factorization;
//! - [`geode_core::eigen::parallel::SequentialSolveScope`] makes it
//!   `Par::Seq` around a loop of small triangular solves. Two loops hold one:
//!   the driven back-solve's AMS-preconditioned COCG, whose V-cycle runs a
//!   cached sparse LU twice per iteration, and the direct shift-invert Lanczos
//!   loop, which runs one per step. With the default setting (every core)
//!   each of those solves went out to the rayon pool and the threads spent
//!   their time contending.
//!
//! This target holds **one** `#[test]` on purpose. The assertions read a
//! process-global value, and in the library's unit-test binary other tests
//! change it concurrently (every direct eigensolve changes it twice). Here
//! nothing else runs in the process, so each assertion is exact. Do not add a
//! second test to this file.
//!
//! That the scope is held *while* each loop runs is asserted by unit tests
//! that can see inside the solvers:
//! `ams_back_solve_holds_a_sequential_scope_and_jacobi_does_not` in
//! `driven/solve.rs` and
//! `direct_backends_come_with_a_sequential_scope_and_matrix_free_does_not` in
//! `eigen/lanczos.rs`.

use burn::tensor::backend::BackendTypes;
use faer::{Par, c64, get_global_parallelism};
use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::assembly::sparse::global_system_to_sparse;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
    IterativePreconditioner, IterativeSettings, SolverMode,
};
use geode_core::eigen::dense::cube_interior_mask;
use geode_core::eigen::lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
use geode_core::eigen::parallel::{ParallelismGuard, SequentialSolveScope};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

/// A solve that fails inside its sequential scope, as a Krylov solve that
/// does not converge does.
fn failing_solve() -> Result<(), String> {
    let _seq = SequentialSolveScope::enter();
    assert_eq!(get_global_parallelism(), Par::Seq);
    Err::<(), String>("did not converge".to_string())?;
    unreachable!("the `?` above returns");
}

#[test]
fn guards_scopes_and_solves_restore_the_callers_parallelism() {
    // ---- ParallelismGuard (issue #518) --------------------------------------
    let ambient = get_global_parallelism();
    // Restored on a normal scope exit.
    {
        let _g = ParallelismGuard::rayon(4);
    }
    assert_eq!(get_global_parallelism(), ambient, "rayon(4) not restored");
    // Restored when the scope panics (Drop runs during unwind).
    let panicked = std::panic::catch_unwind(|| {
        let _g = ParallelismGuard::rayon(4);
        panic!("boom inside the factorization scope");
    });
    assert!(panicked.is_err());
    assert_eq!(
        get_global_parallelism(),
        ambient,
        "rayon(4) not restored after a panicking scope"
    );
    // `rayon(1)` is not a serial request: it leaves the global alone.
    {
        let _g = ParallelismGuard::rayon(1);
        assert_eq!(get_global_parallelism(), ambient);
    }
    assert_eq!(get_global_parallelism(), ambient);
    // `cap(1)` is one (with faer's rayon feature), and restores.
    {
        let _g = ParallelismGuard::cap(1);
        if cfg!(feature = "faer-parallel") {
            assert_eq!(get_global_parallelism(), Par::Seq);
        }
    }
    assert_eq!(get_global_parallelism(), ambient, "cap(1) not restored");

    // The caller's setting: neither faer's default nor `Par::Seq`, so
    // "restored" cannot be mistaken for "left sequential" or "reset to the
    // default". Without the `faer-parallel` feature the cap is a no-op and
    // the caller's setting is whatever faer starts with.
    let _caller = ParallelismGuard::cap(3);
    let caller = get_global_parallelism();
    if cfg!(feature = "faer-parallel") {
        assert_eq!(caller, Par::rayon(3));
    }

    // ---- SequentialSolveScope (issue #946) ----------------------------------
    // Normal exit.
    {
        let _seq = SequentialSolveScope::enter();
        assert_eq!(get_global_parallelism(), Par::Seq);
    }
    assert_eq!(get_global_parallelism(), caller, "not restored on drop");

    // Early return through `?`.
    assert!(failing_solve().is_err());
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after an early error return"
    );

    // Panic inside the scope.
    let panicked = std::panic::catch_unwind(|| {
        let _seq = SequentialSolveScope::enter();
        panic!("boom inside the solve");
    });
    assert!(panicked.is_err());
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after a panicking scope"
    );

    // Two overlapping scopes that end in the order they started in (the
    // first one first). A pair of `ParallelismGuard::cap(1)` gets this wrong:
    // the second saved `Par::Seq` and restores it last. The second scope is
    // started on another thread, as a concurrent solve would start it.
    let first = SequentialSolveScope::enter();
    let second = std::thread::spawn(SequentialSolveScope::enter)
        .join()
        .expect("scope thread");
    assert_eq!(get_global_parallelism(), Par::Seq);
    drop(first);
    assert_eq!(
        get_global_parallelism(),
        Par::Seq,
        "a scope is still live, so the global must stay sequential"
    );
    drop(second);
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after the last of two overlapping scopes"
    );

    // ---- the AMS-preconditioned driven solve --------------------------------
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[2]).sin(), 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.3),
        ]
    });
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        &[],
        &[],
        &source,
        &device,
    )
    .expect("operator assembly");
    let omega = 0.05;
    let ams = |max_iters| {
        SolverMode::Iterative(
            IterativeSettings::new(1e-10, max_iters)
                .with_preconditioner(IterativePreconditioner::AMS),
        )
    };

    // The setup factors the coarse operators and is outside the scope.
    let solver = op
        .prepare_at::<B>(omega, ams(500), &device)
        .expect("AMS setup");
    assert_eq!(
        get_global_parallelism(),
        caller,
        "the AMS setup changed the caller's parallelism"
    );

    // A converging AMS solve.
    let (sol, report) = solver.solve().expect("AMS converges");
    assert!(report.converged && report.iters > 1);
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after a converged AMS solve"
    );

    // An AMS solve that returns an error from inside the scope.
    let failed = op
        .prepare_at::<B>(omega, ams(1), &device)
        .expect("AMS setup")
        .solve();
    assert!(
        matches!(failed, Err(DrivenError::Solve(_))),
        "one iteration must not reach tol = 1e-10"
    );
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after an AMS solve that returned an error"
    );

    // ---- the direct eigensolve ----------------------------------------------
    // Its factorization is capped by a guard and its Lanczos loop runs under
    // a scope. Both must be gone when it returns.
    let pencil = {
        let mesh = cube_tet_mesh(5, 1.0);
        let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
        let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
        let mask = cube_interior_mask(&mesh.nodes, 1.0);
        global_system_to_sparse(sys, Some(&mask)).expect("sparse projection")
    };
    let lambdas = SparseShiftInvertLanczos::default()
        .smallest_eigenvalues(pencil.k.as_ref(), pencil.m.as_ref(), 3)
        .expect("direct eigensolve");
    assert_eq!(lambdas.len(), 3);
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after a direct eigensolve"
    );

    // ---- the answer -----------------------------------------------------------
    // The answer does not depend on the caller's setting: the same solve
    // from a sequential caller gives the same field, bit for bit. (The
    // coarse factorization runs at the caller's thread count in both; on
    // this small operator faer's sparse LU is identical at 1 and 3 threads.)
    let sol_seq = {
        let _seq_caller = ParallelismGuard::cap(1);
        op.prepare_at::<B>(omega, ams(500), &device)
            .expect("AMS setup")
            .solve()
            .expect("AMS converges")
            .0
    };
    assert_eq!(get_global_parallelism(), caller);
    assert!(
        sol.e_edges
            .iter()
            .zip(&sol_seq.e_edges)
            .all(|(a, b)| a.re.to_bits() == b.re.to_bits() && a.im.to_bits() == b.im.to_bits()),
        "AMS field differs between a 3-thread and a sequential caller"
    );
}
