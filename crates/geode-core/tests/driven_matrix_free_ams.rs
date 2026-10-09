//! Matrix-free driven solve with the AMS preconditioner (issue #966).
//!
//! [`SolverMode::IterativeMatrixFree`] used to accept only its on-device
//! Jacobi. Issue #966 lets it take an explicit
//! [`IterativePreconditioner::Ams`]: the same Hiptmair–Xu AMS the assembled
//! [`SolverMode::Iterative`] path builds (same SPD proxy, same coarse solves),
//! applied on the host between the Burn operator applies, on the CPU ndarray
//! f64 backend only. These gates check that:
//!
//! 1. the matrix-free AMS solve matches the assembled AMS solve (and Direct)
//!    on the #520 σ-lossy parallel-plate fixture and on a Leontovich-wall
//!    variant, with an iteration count within 1.5× of the assembled AMS (the
//!    issue's acceptance factor; in practice the counts agree to within a
//!    couple of iterations, since the two Krylov operators differ only by
//!    round-off);
//! 2. it takes far fewer iterations than the matrix-free Jacobi solve;
//! 3. the coarse solve stays pluggable: an explicit non-default
//!    [`AmsCoarseSolve`] reaches the matrix-free path and gives the same
//!    outcome as on the assembled path;
//! 4. the default `Auto` still resolves to Jacobi on the matrix-free path,
//!    and a non-ndarray-f64 backend rejects AMS loudly.
//!
//! The observed iteration counts are printed to stderr (the
//! `iterative_sweep.rs` convention).

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::matrix_free::host_ams_supported;
use geode_core::driven::ports::LumpedPort;
use geode_core::driven::solve::{
    AmsCoarseSolve, CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator,
    DrivenSolution, IterativePreconditioner, IterativeSettings, PreconditionerFallbackReason,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Whether the active test backend is the one the host-side AMS supports
/// (CPU ndarray, f64). On any other backend the tests check the rejection.
fn host_ams_backend() -> bool {
    host_ams_supported::<B>(&device())
}

fn plane_faces(mesh: &TetMesh, axis: usize, value: f64) -> Vec<[u32; 3]> {
    mesh.faces()
        .into_iter()
        .filter(|f| {
            f.iter()
                .all(|&n| (mesh.nodes[n as usize][axis] - value).abs() < 1e-12)
        })
        .collect()
}

fn pec_mask_for_planes(mesh: &TetMesh, edges: &[[u32; 2]], planes: &[(usize, f64)]) -> Vec<bool> {
    edges
        .iter()
        .map(|e| {
            let a = mesh.nodes[e[0] as usize];
            let b = mesh.nodes[e[1] as usize];
            !planes.iter().any(|&(axis, value)| {
                (a[axis] - value).abs() < 1e-12 && (b[axis] - value).abs() < 1e-12
            })
        })
        .collect()
}

/// The #520 fixture (`tests/gpu_driven_scaling.rs`): a σ = 2 cube between PEC
/// planes y = 0, 1 with a lumped port on z = 0. With `leontovich`, the x = 1
/// face is additionally a good-conductor Leontovich wall, so the surface COO
/// correction carries an ω-dependent complex term too.
fn fixture(n: usize, leontovich: bool) -> DrivenOperator {
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let wall_faces = plane_faces(&mesh, 0, 1.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vec![c64::new(1.0, 0.0); mesh.n_tets()];
    let sigma = vec![2.0_f64; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let wall = SurfaceImpedanceBc {
        triangles: &wall_faces,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 50.0 },
    };
    let surfaces: &[SurfaceImpedanceBc<'_>] = if leontovich {
        std::slice::from_ref(&wall)
    } else {
        &[]
    };
    DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        surfaces,
        &CurrentSource {
            j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
        },
        &device(),
    )
    .expect("operator assembly")
}

fn rel_l2(a: &DrivenSolution, b: &DrivenSolution) -> f64 {
    let num: f64 = a
        .e_edges
        .iter()
        .zip(&b.e_edges)
        .map(|(x, y)| (x - y).norm_sqr())
        .sum();
    let den: f64 = b.e_edges.iter().map(|y| y.norm_sqr()).sum();
    (num / den).sqrt()
}

const TOL: f64 = 1e-8;
const MAX_ITERS: usize = 20_000;

fn ams(coarse: AmsCoarseSolve) -> IterativeSettings {
    IterativeSettings::new(TOL, MAX_ITERS)
        .with_preconditioner(IterativePreconditioner::Ams { coarse })
}

fn jacobi() -> IterativeSettings {
    IterativeSettings::new(TOL, MAX_ITERS).with_preconditioner(IterativePreconditioner::Jacobi)
}

/// Solve and return the solution and iteration count.
fn solve(op: &DrivenOperator, omega: f64, mode: SolverMode) -> (DrivenSolution, usize) {
    let solver = op
        .prepare_at::<B>(omega, mode, &device())
        .unwrap_or_else(|e| panic!("prepare_at({mode:?}): {e}"));
    let (sol, report) = solver
        .solve()
        .unwrap_or_else(|e| panic!("solve({mode:?}): {e}"));
    assert!(report.converged && report.residual_rel <= TOL, "{report:?}");
    (sol, report.iters)
}

/// Acceptance 1 and 2: matrix-free AMS against assembled AMS, Direct and
/// matrix-free Jacobi, on the #520 fixture and its Leontovich variant, at two
/// drive frequencies.
#[test]
fn matrix_free_ams_matches_assembled_ams() {
    if !host_ams_backend() {
        eprintln!("[#966] skipped: the host-side AMS needs the CPU ndarray f64 backend");
        return;
    }
    for leontovich in [false, true] {
        let op = fixture(6, leontovich);
        for omega in [0.05, 0.20] {
            let a = ams(AmsCoarseSolve::Auto);
            let (direct, _) = solve(&op, omega, SolverMode::Direct);
            let (asm, it_asm) = solve(&op, omega, SolverMode::Iterative(a));
            let (mf, it_mf) = solve(&op, omega, SolverMode::IterativeMatrixFree(a));
            let (_, it_mf_j) = solve(&op, omega, SolverMode::IterativeMatrixFree(jacobi()));
            let mf_vs_asm = rel_l2(&mf, &asm);
            let mf_vs_direct = rel_l2(&mf, &direct);
            eprintln!(
                "[#966] n=6 leontovich={leontovich} ω={omega}: iters assembled-AMS={it_asm} \
                 matrix-free-AMS={it_mf} matrix-free-Jacobi={it_mf_j}; rel-L2 mf-AMS vs \
                 assembled-AMS {mf_vs_asm:.2e}, vs Direct {mf_vs_direct:.2e}"
            );
            assert!(
                mf_vs_direct < 1e-6,
                "matrix-free AMS vs Direct {mf_vs_direct:.3e}"
            );
            assert!(
                mf_vs_asm < 1e-6,
                "matrix-free AMS vs assembled AMS {mf_vs_asm:.3e}"
            );
            // The acceptance factor, both ways.
            assert!(
                2 * it_mf <= 3 * it_asm && 2 * it_asm <= 3 * it_mf,
                "matrix-free AMS {it_mf} iterations vs assembled AMS {it_asm}: outside 1.5x"
            );
            // Tighter tripwire: the two Krylov operators differ only by
            // round-off, so the counts should all but coincide (they are
            // identical on this fixture with ndarray f64).
            assert!(
                it_mf.abs_diff(it_asm) <= 2.max(it_asm / 20),
                "matrix-free AMS {it_mf} iterations vs assembled AMS {it_asm}"
            );
            // AMS must beat Jacobi decisively on the matrix-free path too.
            assert!(
                4 * it_mf <= it_mf_j,
                "matrix-free AMS {it_mf} iterations is not 4x below matrix-free Jacobi {it_mf_j}"
            );
        }
    }
}

/// Acceptance 1: the handle reports the AMS it uses, with no fallback
/// warning, and the default `Auto` still resolves to Jacobi on the
/// matrix-free path (Jacobi stays the default).
#[test]
fn matrix_free_explicit_ams_is_honoured_and_auto_stays_jacobi() {
    let op = fixture(3, false);
    let explicit = SolverMode::IterativeMatrixFree(ams(AmsCoarseSolve::Auto));
    assert_eq!(
        op.resolve_preconditioner(explicit),
        Some((IterativePreconditioner::AMS, None))
    );
    let auto = SolverMode::IterativeMatrixFree(IterativeSettings::new(TOL, MAX_ITERS));
    let (pc, fallback) = op.resolve_preconditioner(auto).expect("iterative");
    assert_eq!(pc, IterativePreconditioner::Jacobi);
    let fallback = fallback.expect("auto on matrix-free warns");
    assert_eq!(fallback.reason, PreconditionerFallbackReason::MatrixFree);
    assert!(fallback.to_string().contains("explicitly"), "{fallback}");

    match op.prepare_at::<B>(0.1, explicit, &device()) {
        Ok(solver) => {
            assert!(host_ams_backend());
            assert!(solver.is_iterative());
            assert_eq!(solver.preconditioner(), Some(IterativePreconditioner::AMS));
            assert_eq!(solver.preconditioner_fallback(), None);
        }
        Err(e) => {
            // Acceptance 1: unsupported backends fail loudly.
            assert!(!host_ams_backend(), "unexpected setup error: {e}");
            assert!(
                matches!(&e, DrivenError::UnsupportedMatrixFree { reason } if reason.contains("ndarray")),
                "{e:?}"
            );
        }
    }
}

/// Acceptance 1: other explicit preconditioners are still rejected on the
/// matrix-free path, with a message naming both supported choices.
#[test]
fn matrix_free_still_rejects_ilu0_and_chebyshev() {
    let op = fixture(3, false);
    for pc in [
        IterativePreconditioner::Ilu0,
        IterativePreconditioner::Chebyshev { degree: 2 },
    ] {
        let mode = SolverMode::IterativeMatrixFree(
            IterativeSettings::new(TOL, MAX_ITERS).with_preconditioner(pc),
        );
        let err = op
            .prepare_at::<B>(0.1, mode, &device())
            .err()
            .expect("must reject");
        assert!(
            matches!(&err, DrivenError::UnsupportedMatrixFree { reason }
                if reason.contains(pc.name()) && reason.contains("AMS")),
            "{err:?}"
        );
    }
}

/// Coarse-solve pluggability: an explicit non-default gradient-space coarse
/// solve reaches the matrix-free AMS, and its outcome (converged with the
/// same iteration count to within 1.5x, or failed) is the assembled path's.
#[test]
fn matrix_free_ams_coarse_solve_is_pluggable() {
    if !host_ams_backend() {
        return;
    }
    let op = fixture(5, false);
    let omega = 0.1;
    for coarse in [AmsCoarseSolve::Amg, AmsCoarseSolve::SymmetricGaussSeidel] {
        let a = ams(coarse);
        let run = |mode: SolverMode| {
            let solver = op.prepare_at::<B>(omega, mode, &device()).expect("setup");
            assert_eq!(
                solver.preconditioner(),
                Some(IterativePreconditioner::Ams { coarse })
            );
            solver.solve().map(|(_, r)| r.iters)
        };
        let asm = run(SolverMode::Iterative(a));
        let mf = run(SolverMode::IterativeMatrixFree(a));
        eprintln!(
            "[#966] coarse={}: assembled {:?}, matrix-free {:?}",
            coarse.name(),
            asm.as_ref().map_err(|e| e.to_string()),
            mf.as_ref().map_err(|e| e.to_string())
        );
        match (asm, mf) {
            (Ok(i_a), Ok(i_m)) => assert!(
                2 * i_m <= 3 * i_a && 2 * i_a <= 3 * i_m,
                "coarse={}: matrix-free {i_m} vs assembled {i_a}",
                coarse.name()
            ),
            (Err(_), Err(_)) => {}
            (asm, mf) => panic!(
                "coarse={}: outcomes differ (assembled {:?}, matrix-free {:?})",
                coarse.name(),
                asm.map_err(|e| e.to_string()),
                mf.map_err(|e| e.to_string())
            ),
        }
    }
}
