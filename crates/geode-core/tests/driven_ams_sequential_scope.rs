//! faer's process-global parallelism around an AMS-preconditioned driven
//! solve (issue #946).
//!
//! The AMS V-cycle runs a cached sparse LU through faer's
//! `Lu::solve_in_place` twice per COCG iteration. That call takes its thread
//! count from a process-global setting, and with the default (every core)
//! each of those small solves went out to the rayon pool. The driven
//! back-solve now holds a
//! [`geode_core::eigen::parallel::SequentialSolveScope`] for the Krylov solve.
//!
//! This target holds **one** `#[test]` on purpose. The assertions read a
//! process-global value, and in the library's unit-test binary other tests
//! change it concurrently (every direct eigensolve caps it around its
//! factorization). Here nothing else runs in the process, so each assertion
//! is exact. Do not add a second test to this file.
//!
//! That the scope is held *while* the Krylov solve runs is asserted by the
//! unit test `ams_back_solve_holds_a_sequential_scope_and_jacobi_does_not`
//! in `driven/solve.rs`, which can see inside the back-solve.

use burn::tensor::backend::BackendTypes;
use faer::{Par, c64, get_global_parallelism};
use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
    IterativePreconditioner, IterativeSettings, SolverMode,
};
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
fn sequential_scope_and_ams_solve_restore_the_callers_parallelism() {
    // The caller's setting: neither faer's default nor `Par::Seq`, so
    // "restored" cannot be mistaken for "left sequential" or "reset to the
    // default". Without the `faer-parallel` feature the cap is a no-op and
    // the caller's setting is whatever faer starts with.
    let _caller = ParallelismGuard::cap(3);
    let caller = get_global_parallelism();
    if cfg!(feature = "faer-parallel") {
        assert_eq!(caller, Par::rayon(3));
    }

    // ---- the scope itself -------------------------------------------------
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

    // ---- the driven solve ---------------------------------------------------
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
