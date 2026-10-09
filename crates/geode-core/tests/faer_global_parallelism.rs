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
//! second test to this file: add a block to the one test instead.
//!
//! Where a block needs a second thread, that thread is stepped through
//! channels ([`OnAnotherThread`]), so the order of events is fixed and no
//! block depends on timing.
//!
//! Issue #956 added three more holders: the complex-symmetric shift-invert
//! Lanczos loop, the AMS coarse LU solve (per call) and the transient
//! stepper's back-solve (per step). Each must leave the caller's setting in
//! place when it returns.
//!
//! That the scope is held *while* each loop runs is asserted by unit tests
//! that can see inside the solvers:
//! `ams_back_solve_holds_a_sequential_scope_and_jacobi_does_not` in
//! `driven/solve.rs`,
//! `direct_backends_come_with_a_sequential_scope_and_matrix_free_does_not` in
//! `eigen/lanczos.rs`, and (#956) `lanczos_solves_run_under_a_sequential_scope`
//! in `eigen/complex/lanczos.rs`,
//! `eigen_ams_coarse_lu_solves_run_under_a_sequential_scope` in `eigen/ams.rs`
//! and `step_back_solve_runs_under_a_sequential_scope` in
//! `driven/transient.rs`.

use burn::tensor::backend::BackendTypes;
use faer::{Par, c64, get_global_parallelism, set_global_parallelism};
use geode_core::assembly::nedelec::cube_pec_interior_edges;
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::assembly::sparse::global_system_to_sparse;
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
    IterativePreconditioner, IterativeSettings, SolverMode,
};
use geode_core::driven::transient::{TransientScheme, TransientSolver};
use geode_core::eigen::complex::SparseComplexShiftInvertLanczos;
use geode_core::eigen::dense::cube_interior_mask;
use geode_core::eigen::lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
use geode_core::eigen::parallel::{ParallelismGuard, SequentialSolveScope};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::TestBackend;
use std::sync::mpsc;
use std::thread;

type B = TestBackend;

/// Something built on a second thread and dropped there on request, so a
/// test can place its construction and its drop exactly among the events on
/// the main thread.
struct OnAnotherThread {
    release: mpsc::Sender<()>,
    thread: thread::JoinHandle<()>,
}

impl OnAnotherThread {
    /// Build `make()` on a new thread. Returns once it has been built.
    fn build<T>(make: impl FnOnce() -> T + Send + 'static) -> Self {
        let (built_tx, built_rx) = mpsc::channel();
        let (release, release_rx) = mpsc::channel::<()>();
        let thread = thread::spawn(move || {
            let held = make();
            built_tx.send(()).expect("main thread is waiting");
            release_rx.recv().expect("main thread releases");
            drop(held);
        });
        built_rx.recv().expect("the other thread built its value");
        Self { release, thread }
    }

    /// Drop the value on its thread. Returns once it has been dropped.
    fn drop_there(self) {
        self.release.send(()).expect("the other thread is waiting");
        self.thread.join().expect("the other thread");
    }
}

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

    // `cap(3)` sets three threads, and restores.
    {
        let _g = ParallelismGuard::cap(3);
        if cfg!(feature = "faer-parallel") {
            assert_eq!(get_global_parallelism(), Par::rayon(3));
        }
    }
    assert_eq!(get_global_parallelism(), ambient, "cap(3) not restored");

    // ---- guards and scopes that overlap (issue #946 review) -----------------
    // Guards and scopes share one registry: `Par::Seq` while any scope is
    // live, else the cap of the live guard built last, else the value from
    // before the first of them. These blocks need real caps, so they run
    // with the `faer-parallel` feature only (without it a guard is a no-op).
    if cfg!(feature = "faer-parallel") {
        // An ambient value that is neither faer's default, nor `Par::Seq`,
        // nor any cap used below. Nothing is live here, so writing the
        // global directly simply makes this the ambient value.
        let ambient5 = Par::rayon(5);
        set_global_parallelism(ambient5);

        // (a) scope starts, guard starts on another thread, scope ends,
        // guard ends. With save-and-restore types the guard saved the
        // scope's `Par::Seq` and restored it last, for good. This is two
        // concurrent direct eigensolves: one's factorization starts during
        // the other's Lanczos loop and outlives it.
        let scope = SequentialSolveScope::enter();
        let guard = OnAnotherThread::build(|| ParallelismGuard::cap(3));
        let both_live = get_global_parallelism();
        drop(scope);
        let guard_only = get_global_parallelism();
        guard.drop_there();
        let neither = get_global_parallelism();
        let later = {
            drop(SequentialSolveScope::enter());
            get_global_parallelism()
        };
        assert_eq!(
            neither, ambient5,
            "(a) scope, guard, scope ends, guard ends: ambient not restored"
        );
        assert_eq!(later, ambient5, "(a) a later scope lost the ambient value");
        assert_eq!(
            both_live,
            Par::Seq,
            "(a) a guard on another thread put a live scope back on the pool"
        );
        assert_eq!(
            guard_only,
            Par::rayon(3),
            "(a) the guard's cap must apply once the scope has ended"
        );

        // (b) the mirror: guard starts, scope starts on another thread,
        // guard ends, scope ends.
        let guard = ParallelismGuard::cap(3);
        assert_eq!(get_global_parallelism(), Par::rayon(3));
        let scope = OnAnotherThread::build(SequentialSolveScope::enter);
        assert_eq!(get_global_parallelism(), Par::Seq);
        drop(guard);
        assert_eq!(
            get_global_parallelism(),
            Par::Seq,
            "(b) a dropped guard put a live scope back on the pool"
        );
        scope.drop_there();
        assert_eq!(
            get_global_parallelism(),
            ambient5,
            "(b) guard, scope, guard ends, scope ends: ambient not restored"
        );

        // (c) the two nested orders, with the guard on the other thread.
        let scope = SequentialSolveScope::enter();
        let guard = OnAnotherThread::build(|| ParallelismGuard::cap(3));
        guard.drop_there();
        assert_eq!(get_global_parallelism(), Par::Seq);
        drop(scope);
        assert_eq!(get_global_parallelism(), ambient5, "(c) scope around guard");
        let guard = OnAnotherThread::build(|| ParallelismGuard::cap(3));
        let scope = SequentialSolveScope::enter();
        assert_eq!(get_global_parallelism(), Par::Seq);
        drop(scope);
        assert_eq!(get_global_parallelism(), Par::rayon(3));
        guard.drop_there();
        assert_eq!(get_global_parallelism(), ambient5, "(c) guard around scope");

        // (d) two guards with different caps on two threads, dropped in the
        // order they were built. The cap built last applies while both are
        // live and stays until its guard drops.
        let two = ParallelismGuard::cap(2);
        let four = OnAnotherThread::build(|| ParallelismGuard::cap(4));
        assert_eq!(get_global_parallelism(), Par::rayon(4));
        drop(two);
        assert_eq!(
            get_global_parallelism(),
            Par::rayon(4),
            "(d) dropping the older guard must leave the live guard's cap"
        );
        four.drop_there();
        assert_eq!(
            get_global_parallelism(),
            ambient5,
            "(d) two guards dropped out of order: ambient not restored"
        );

        // (e) the same out-of-order drop on one thread, and the nested
        // order, which behaves as it always did.
        let two = ParallelismGuard::cap(2);
        let four = ParallelismGuard::cap(4);
        drop(two);
        assert_eq!(get_global_parallelism(), Par::rayon(4));
        drop(four);
        assert_eq!(get_global_parallelism(), ambient5, "(e) out of order");
        {
            let _two = ParallelismGuard::cap(2);
            {
                let _four = ParallelismGuard::cap(4);
                assert_eq!(get_global_parallelism(), Par::rayon(4));
            }
            assert_eq!(get_global_parallelism(), Par::rayon(2));
        }
        assert_eq!(get_global_parallelism(), ambient5, "(e) nested");

        // (f) a direct write while a guard is live is not remembered: the
        // next enter or drop sets the global by the rule, and the ambient
        // value comes back at the end.
        let guard = ParallelismGuard::cap(3);
        set_global_parallelism(Par::rayon(7));
        assert_eq!(get_global_parallelism(), Par::rayon(7));
        {
            let _seq = SequentialSolveScope::enter();
            assert_eq!(get_global_parallelism(), Par::Seq);
        }
        assert_eq!(get_global_parallelism(), Par::rayon(3));
        set_global_parallelism(Par::rayon(7));
        drop(guard);
        assert_eq!(get_global_parallelism(), ambient5, "(f) direct write kept");

        // Back to what the process started with, for the rest of the test.
        set_global_parallelism(ambient);
    }

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
    // first one first). The second scope is started on another thread, as a
    // concurrent solve would start it.
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

    // ---- the complex eigensolve and the transient stepper (issue #956) --------
    // The complex Lanczos holds a scope for its loop, the stepper one per
    // step. Neither may leave it behind.
    let lift = |a: &faer::sparse::SparseColMat<usize, f64>, s: c64| {
        let r = a.as_ref();
        let t: Vec<_> = (0..r.ncols())
            .flat_map(|j| {
                (r.col_ptr()[j]..r.col_ptr()[j + 1])
                    .map(move |p| faer::sparse::Triplet::new(r.row_idx()[p], j, s * r.val()[p]))
            })
            .collect();
        faer::sparse::SparseColMat::try_new_from_triplets(r.nrows(), r.ncols(), &t).unwrap()
    };
    let complex = SparseComplexShiftInvertLanczos::default()
        .smallest_eigenpairs(
            lift(&pencil.k, c64::new(1.0, 0.02)).as_ref(),
            lift(&pencil.m, c64::new(1.0, 0.0)).as_ref(),
            3,
        )
        .expect("complex eigensolve");
    assert_eq!(complex.len(), 3);
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after a complex eigensolve"
    );
    let transient = TransientSolver::new(&op).expect("transient solver");
    let mut stepper = transient
        .factor(0.1, TransientScheme::generalized_alpha(0.8))
        .expect("transient factor");
    let force = vec![1.0; transient.n_interior()];
    stepper.step(&force, &force);
    assert_eq!(
        get_global_parallelism(),
        caller,
        "not restored after a transient step"
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
