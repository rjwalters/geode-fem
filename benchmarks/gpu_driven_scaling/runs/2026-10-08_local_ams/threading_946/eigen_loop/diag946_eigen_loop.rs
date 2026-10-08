//! THROWAWAY diagnostic for issue #946 (not committed): does the eigen direct
//! shift-invert Lanczos loop contend on the rayon pool the way the AMS
//! V-cycle's triangular solves do?
//!
//! DIAG946_N       cube_tet_mesh size (default 40)
//! DIAG946_NEV     eigenvalues requested (default 20)
//! DIAG946_SEQ_LOOP=1  hold ParallelismGuard::cap(1) around the whole solve.
//!     The solver's own guard still caps the factorization at
//!     resolve_num_threads() and restores Par::Seq afterwards, so the
//!     factorization stays parallel and only the Lanczos loop goes sequential.
#![allow(unsafe_code)]
use burn::tensor::backend::BackendTypes;
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::assembly::sparse::global_system_to_sparse;
use geode_core::eigen::dense::cube_interior_mask;
use geode_core::eigen::lanczos::{SparseEigenSolver, SparseShiftInvertLanczos};
use geode_core::eigen::parallel::ParallelismGuard;
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::TestBackend;

type B = TestBackend;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Timeval {
    sec: i64,
    usec: i32,
    _pad: i32,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct RUsage {
    utime: Timeval,
    stime: Timeval,
    rest: [i64; 14],
}
unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut RUsage) -> i32;
}
/// (user, sys) CPU seconds of this process so far.
fn cpu() -> (f64, f64) {
    let mut r = RUsage::default();
    assert_eq!(unsafe { getrusage(0, &mut r) }, 0);
    let t = |v: Timeval| v.sec as f64 + v.usec as f64 * 1e-6;
    (t(r.utime), t(r.stime))
}

#[test]
#[ignore]
fn diag946_eigen_loop() {
    let var = |k: &str, d: usize| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let n = var("DIAG946_N", 40);
    let nev = var("DIAG946_NEV", 20);
    let seq_loop = std::env::var("DIAG946_SEQ_LOOP").is_ok();
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let sp = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");
    let solver = SparseShiftInvertLanczos::default();
    let t = std::time::Instant::now();
    let (u0, s0) = cpu();
    let _outer = seq_loop.then(|| ParallelismGuard::cap(1));
    let lam = solver
        .smallest_eigenvalues(sp.k.as_ref(), sp.m.as_ref(), nev)
        .expect("eigensolve");
    let dt = t.elapsed().as_secs_f64();
    let (u1, s1) = cpu();
    println!(
        "diag946 solve_user_s={:.3} solve_sys_s={:.3}",
        u1 - u0,
        s1 - s0
    );
    println!(
        "diag946 n={n} dofs={} nev={nev} seq_loop={seq_loop} solve_s={dt:.3} par_after={:?}",
        sp.k.nrows(),
        faer::get_global_parallelism()
    );
    for (i, l) in lam.iter().enumerate() {
        println!("diag946 lambda[{i}] = {l:.17e}");
    }
}
