//! Matrix-free driven solve equivalence gates (issue #302 Phase 3 / PR #493).
//!
//! [`SolverMode::IterativeMatrixFree`] wires PR #487's GPU-resident
//! [`geode_core::solver::ksp_burn::BurnCocg`] (over the Burn volume pencil plus
//! an on-device COO surface correction) into the driven back-solve seam. These
//! regressions verify that the matrix-free path:
//!
//! 1. produces `DrivenSolution`s / port circuit quantities matching the
//!    `Direct` (faer sparse-LU) path on the existing cube lumped-port and
//!    two-port fixtures to the tolerances `iterative_sweep.rs` already asserts
//!    for the assembled `Iterative` path (a σ-lossy cube keeps the pencil
//!    well-conditioned);
//! 2. records BurnCocg iteration counts **consistent** with the assembled-CSR
//!    Jacobi COCG on the same fixtures (the port `S_p` diagonal is folded into
//!    the matrix-free Jacobi preconditioner, so the two preconditioners match);
//! 3. rejects the unsupported configurations (wave-port sweeps, anisotropic /
//!    matched-UPML materials) with a clean [`DrivenError::UnsupportedMatrixFree`];
//! 4. leaves the existing `Direct` / `Iterative` modes untouched (they are
//!    exercised unchanged by `iterative_sweep.rs`).
//!
//! Fixtures are the *same* cube lumped-port / two-port meshes as
//! `iterative_sweep.rs` (extended, not reinvented) — ndarray-f64 in CI. The
//! observed iteration counts are printed to stderr so future runs surface
//! convergence degradation (matching the `iterative_sweep.rs` convention).

use burn::tensor::backend::BackendTypes;
use faer::c64;
use geode_core::driven::extraction::{
    driven_frequency_sweep, driven_frequency_sweep_with_mode, s_parameter_frequency_sweep,
    s_parameter_frequency_sweep_with_mode,
};
use geode_core::driven::ports::{LumpedPort, WavePort, solve_wave_port_sweep_with_mode};
use geode_core::driven::solve::{
    CurrentSource, DrivenBcs, DrivenError, DrivenMaterials, DrivenOperator, IterativeSettings,
    SolverMode, SurfaceImpedanceBc, SurfaceImpedanceModel,
};
use geode_core::mesh::{TetMesh, cube_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

fn zero_source(mesh: &TetMesh) -> CurrentSource {
    CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    }
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

// ===========================================================================
// 1. driven_frequency_sweep: matrix-free vs direct (cube lumped port)
// ===========================================================================

/// **Acceptance criterion 1 (single lumped port)**: matrix-free
/// `driven_frequency_sweep` matches the direct LU path's port impedance
/// `Z(ω)` on the σ-lossy parallel-plate resistor fixture — the same fixture
/// `driven_frequency_sweep_iterative_matches_direct` uses for the assembled
/// COCG path.
#[test]
fn driven_frequency_sweep_matrix_free_matches_direct() {
    let mesh = cube_tet_mesh(4, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let sigma = 2.0;
    let eps = vacuum(&mesh);
    let sigma_tet = vec![sigma; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.05_f64, 0.10, 0.20];

    let direct = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("direct sweep");

    let settings = IterativeSettings::new(1e-10, 5_000);
    let matrix_free = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    )
    .expect("matrix-free sweep");

    assert_eq!(direct.len(), matrix_free.len());
    for (pt_d, pt_m) in direct.iter().zip(matrix_free.iter()) {
        assert_eq!(pt_d.iters_per_rhs, vec![0]);
        assert_eq!(pt_m.iters_per_rhs.len(), 1);
        let iters = pt_m.iters_per_rhs[0];
        assert!(iters > 0, "matrix-free path must record iterations");

        let z_d = pt_d.ports[0].z;
        let z_m = pt_m.ports[0].z;
        let rel_diff = (z_d - z_m).norm() / z_d.norm().max(1e-30);
        eprintln!(
            "[#493 / driven_frequency_sweep] ω = {:.3}: Z_direct = {z_d}, \
             Z_matrix_free = {z_m}, rel_diff = {:.3e}, BurnCOCG iters = {iters}",
            pt_d.omega, rel_diff,
        );
        assert!(
            rel_diff < 1e-6,
            "ω = {}: Z agreement |Z_d − Z_mf| / |Z_d| = {} (tol 1e-6)",
            pt_d.omega,
            rel_diff
        );
        assert!(
            pt_m.residual_rel < 1e-8,
            "matrix-free residual too large at ω = {}: {}",
            pt_m.omega,
            pt_m.residual_rel
        );
    }
}

// ===========================================================================
// 2. s_parameter_frequency_sweep: matrix-free vs direct (two lumped ports)
// ===========================================================================

/// **Acceptance criterion 1 (two ports)**: matrix-free
/// `s_parameter_frequency_sweep` matches the direct LU S-matrix on the
/// two-port matched-stub cube fixture (same fixture as
/// `s_parameter_sweep_iterative_matches_direct`).
#[test]
fn s_parameter_sweep_matrix_free_matches_direct() {
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let port1_faces = plane_faces(&mesh, 2, 0.0);
    let port2_faces = plane_faces(&mesh, 2, 1.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    // A little volumetric σ sharpens conditioning for the Jacobi COCG so the
    // two-port cavity is not near-singular at these low ω.
    let sigma_tet = vec![0.5_f64; mesh.n_tets()];
    let port1 = LumpedPort {
        faces: &port1_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let port2 = LumpedPort {
        faces: &port2_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.10_f64, 0.20];

    let direct = s_parameter_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port1.clone(), port2.clone()],
        &[],
        &omegas,
        &device(),
    )
    .expect("direct S sweep");

    let settings = IterativeSettings::new(1e-10, 5_000);
    let matrix_free = s_parameter_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[port1.clone(), port2.clone()],
        &[],
        &omegas,
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    )
    .expect("matrix-free S sweep");

    assert_eq!(direct.len(), matrix_free.len());
    for (pt_d, pt_m) in direct.iter().zip(matrix_free.iter()) {
        assert_eq!(pt_d.iters_per_rhs.len(), 2);
        assert!(pt_d.iters_per_rhs.iter().all(|&i| i == 0));
        assert_eq!(pt_m.iters_per_rhs.len(), 2);
        assert!(pt_m.iters_per_rhs.iter().all(|&i| i > 0));

        let n = 2;
        let mut max_rel_diff = 0.0_f64;
        for k in 0..n * n {
            let s_d = pt_d.s.s[k];
            let s_m = pt_m.s.s[k];
            let rel_diff = (s_d - s_m).norm() / s_d.norm().max(1e-30);
            if rel_diff > max_rel_diff {
                max_rel_diff = rel_diff;
            }
        }
        eprintln!(
            "[#493 / s_parameter_sweep] ω = {:.3}: max |ΔS|/|S| = {:.3e}, \
             BurnCOCG iters per RHS = {:?}",
            pt_d.omega, max_rel_diff, pt_m.iters_per_rhs,
        );
        assert!(
            max_rel_diff < 1e-5,
            "ω = {}: max S-matrix rel_diff = {} (tol 1e-5)",
            pt_d.omega,
            max_rel_diff
        );
    }
}

// ===========================================================================
// 3. Iteration-count consistency: matrix-free BurnCocg vs assembled Cocg
// ===========================================================================

/// **Acceptance criterion 2**: the matrix-free BurnCocg iteration count is
/// consistent (within a small band) with the assembled-CSR Jacobi COCG on the
/// same σ-lossy cube lumped-port fixture — both precondition with the same
/// complex diagonal (volume + port surface), so the counts should be close.
#[test]
fn iteration_counts_track_assembled_cocg() {
    let mesh = cube_tet_mesh(4, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let sigma_tet = vec![2.0_f64; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.05_f64, 0.10, 0.20];
    let settings = IterativeSettings::new(1e-10, 5_000);

    let assembled = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        SolverMode::Iterative(settings),
        &device(),
    )
    .expect("assembled iterative sweep");

    let matrix_free = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    )
    .expect("matrix-free sweep");

    for (pt_a, pt_m) in assembled.iter().zip(matrix_free.iter()) {
        let it_a = pt_a.iters_per_rhs[0] as i64;
        let it_m = pt_m.iters_per_rhs[0] as i64;
        eprintln!(
            "[#493 / iteration-parity] ω = {:.3}: assembled Cocg iters = {it_a}, \
             matrix-free BurnCocg iters = {it_m}, Δ = {}",
            pt_a.omega,
            (it_m - it_a).abs(),
        );
        // Same preconditioner diagonal, same complex-symmetric pencil, same
        // tol — the two COCG recurrences track closely, but the substrates
        // differ (assembled Cocg iterates in interior `n_interior` space on
        // faer `c64`; BurnCocg iterates in full-edge masked space over
        // split-complex Burn tensors with batched-bmm summation order), so
        // exact iteration equality is not expected. Measured Δ on this fixture
        // is ≤ 9 (ω = 0.05: 545 vs 543; ω = 0.10: 496 vs 505; ω = 0.20 close);
        // a ±15 band flags a genuine convergence regression while tolerating
        // the substrate-level reordering (PR #487 measured exact-to-±4 on the
        // volume-only pencil; the port surface term widens it modestly).
        assert!(
            (it_m - it_a).abs() <= 15,
            "ω = {}: matrix-free iters {it_m} diverge from assembled {it_a} by > 15",
            pt_a.omega
        );
    }
}

// ===========================================================================
// 4. Leontovich surface (S_Γ) equivalence — the ω-dependent COO term
// ===========================================================================

/// **Acceptance criterion 1 (Leontovich surface)**: with a good-conductor
/// Leontovich wall (whose weak coefficient `iω/Z_s(ω) ∝ √ω·(1+i)` is
/// ω-dependent), the matrix-free full-field solution matches the direct LU
/// path — exercising the `S_Γ` COO correction (issue #493 Gap-2 accessor) and
/// its per-ω complex scalar folding. Driven by a volumetric current source
/// (no lumped ports), comparing the full `[n_edges]` `e_edges` vector.
#[test]
fn leontovich_surface_matrix_free_matches_direct() {
    let mesh = cube_tet_mesh(3, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &mask,
    };
    let wall = plane_faces(&mesh, 2, 1.0);
    let surfaces = [SurfaceImpedanceBc {
        triangles: &wall,
        model: SurfaceImpedanceModel::GoodConductor { sigma: 40.0 },
    }];
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[2]).sin(), 0.0),
            c64::new(0.1, 0.0),
        ]
    });

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &surfaces,
        &source,
        &device(),
    )
    .expect("operator assembly");

    let settings = IterativeSettings::new(1e-11, 10_000);
    for omega in [0.9_f64, 1.7] {
        let sol_direct = op.solve_at(omega).expect("direct solve_at");

        let solver = op
            .prepare_at::<B>(omega, SolverMode::IterativeMatrixFree(settings), &device())
            .expect("matrix-free prepare_at");
        let (sol_mf, report) = solver.solve().expect("matrix-free solve");

        // Full-field relative error against the direct solution.
        let mut num = 0.0_f64;
        let mut den = 0.0_f64;
        for (d, m) in sol_direct.e_edges.iter().zip(sol_mf.e_edges.iter()) {
            let diff = *d - *m;
            num += diff.re * diff.re + diff.im * diff.im;
            den += d.re * d.re + d.im * d.im;
        }
        let rel = (num / den.max(1e-30)).sqrt();
        eprintln!(
            "[#493 / leontovich] ω = {:.2}: full-field rel err = {:.3e}, \
             BurnCOCG iters = {}, residual_rel = {:.3e}",
            omega, rel, report.iters, report.residual_rel,
        );
        assert!(
            rel < 1e-6,
            "ω = {omega}: Leontovich matrix-free full-field rel err = {rel} (tol 1e-6)"
        );
    }
}

// ===========================================================================
// 5. Guard tests: unsupported configurations reject cleanly
// ===========================================================================

/// **Acceptance criterion 4 (material guard)**: an anisotropic `DiagTensor`
/// material request on the matrix-free path returns
/// `DrivenError::UnsupportedMatrixFree` — no silent degradation.
#[test]
fn matrix_free_rejects_diag_tensor_material() {
    let mesh = cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps_diag: Vec<[c64; 3]> =
        vec![[c64::new(1.0, 0.0), c64::new(1.0, 0.0), c64::new(1.0, 0.0)]; mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |_| {
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
    });
    let settings = IterativeSettings::new(1e-10, 5_000);
    let res = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::DiagTensor(&eps_diag),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &[1.0],
        &source,
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    );
    assert!(
        matches!(res, Err(DrivenError::UnsupportedMatrixFree { .. })),
        "expected UnsupportedMatrixFree, got {res:?}"
    );
}

/// **Acceptance criterion 4 (complex-ε guard)**: a lossy *complex* scalar ε
/// (scalar-PML class) is also out of matrix-free v1 scope and rejects cleanly
/// — the guard reads the retained-ingredient flag, not a heuristic.
#[test]
fn matrix_free_rejects_complex_scalar_epsilon() {
    let mesh = cube_tet_mesh(2, 1.0);
    let edges = mesh.edges();
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps: Vec<c64> = vec![c64::new(1.0, -0.2); mesh.n_tets()];
    let source = CurrentSource::from_centroids(&mesh, |_| {
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
    });
    let settings = IterativeSettings::new(1e-10, 5_000);
    let res = driven_frequency_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        &[],
        &[],
        &[1.0],
        &source,
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    );
    assert!(
        matches!(res, Err(DrivenError::UnsupportedMatrixFree { .. })),
        "expected UnsupportedMatrixFree, got {res:?}"
    );
}

/// **Acceptance criterion 4 (wave-port guard)**: a wave-port sweep on the
/// matrix-free path is rejected at the `solve_wave_port_sweep_with_mode` call
/// site (the rank-N SMW modal-Robin composition is a deferred follow-on).
#[test]
fn matrix_free_rejects_wave_port_sweep() {
    // An empty-mesh / trivially-constructed call is enough: the guard fires
    // before any assembly. Build a minimal valid single wave port so the
    // "needs at least one port" check passes and the mode guard is reached.
    let mesh = cube_tet_mesh(2, 1.0);
    let n_edges = mesh.edges().len();
    let faces = plane_faces(&mesh, 2, 0.0);
    let eps = vacuum(&mesh);
    let mask = vec![true; n_edges];
    // A dummy single-mode wave port (profile length = n_edges, nonzero a_inc).
    let mode_profile = vec![0.0_f64; n_edges];
    let port = WavePort::single_mode(faces, mode_profile, 1.0, c64::new(1.0, 0.0));
    let settings = IterativeSettings::new(1e-10, 5_000);
    let res = solve_wave_port_sweep_with_mode::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[2.5],
        SolverMode::IterativeMatrixFree(settings),
        &device(),
    );
    assert!(
        matches!(res, Err(DrivenError::UnsupportedMatrixFree { .. })),
        "expected UnsupportedMatrixFree for wave-port matrix-free, got {res:?}"
    );
}

// ===========================================================================
// 6. CUDA f32 smoke test — rented-EC2-box only, NEVER runs in CI.
// ===========================================================================
//
// `burn-cuda 0.21` disables f64 (cubecl asserts !supports_dtype(F64)), so the
// tight f64 driven-equivalence gates above cannot run on CUDA. This f32 leg is
// the fast *correctness* companion to the multi-size wall-clock scaling bench
// in `gpu_driven_scaling.rs` (issue #501 / PR #516, `#[ignore]` release bench,
// `n ∈ {6, 9, 12, 15}`): where that bench measures how the driven solve scales,
// this smoke is a single-fixture / single-ω gate that runs in the same
// few-seconds-to-15s ballpark as the sibling smokes `matrix_free_cuda_f32_smoke`
// (#483, 15.0s) and `cocg_burn_cuda_f32_smoke` (#487, 3.3s).
//
// It (a) drives one σ-lossy cube lumped-port solve through
// `SolverMode::IterativeMatrixFree` on the `Cuda` backend at f32 and asserts it
// *converges* (records iterations), and (b) compares the port impedance `Z(ω)`
// back to the f64 `Direct` (faer sparse-LU) reference computed on the ndarray
// `TestBackend`. The tolerance is sized from the measured f32 reality on PR
// #516's GPU run — at this small size the f32 matrix-free path lands ~1e-4 of
// Direct — so the gate is `rel-Z < 1e-3` (a loose f32-appropriate band, NOT the
// tight `1e-6` f64 gate in `driven_frequency_sweep_matrix_free_matches_direct`
// above).
//
// Both `cuda`-feature-gated and `#[ignore]`d so `cargo test` (default features,
// no ignored tests) skips it in CI; run explicitly on the box with
// `cargo test -p geode-core --features cuda --test driven_matrix_free_equivalence \
//    -- --ignored driven_matrix_free_cuda_f32_smoke`.
#[cfg(feature = "cuda")]
#[test]
#[ignore = "CUDA f32 smoke — rented EC2 box only, not CI (burn-cuda 0.21 has no f64)"]
fn driven_matrix_free_cuda_f32_smoke() {
    use burn::backend::Cuda;

    // Same σ-lossy parallel-plate cube lumped-port fixture as
    // `driven_frequency_sweep_matrix_free_matches_direct`, kept small: grid=4,
    // single lumped port, single ω (no sweep) — a fast correctness gate.
    let mesh = cube_tet_mesh(4, 1.0);
    let edges = mesh.edges();
    let port_faces = plane_faces(&mesh, 2, 0.0);
    let mask = pec_mask_for_planes(&mesh, &edges, &[(1, 0.0), (1, 1.0)]);
    let eps = vacuum(&mesh);
    let sigma_tet = vec![2.0_f64; mesh.n_tets()];
    let port = LumpedPort {
        faces: &port_faces,
        e_hat: [0.0, 1.0, 0.0],
        resistance: 1.0,
        width: 1.0,
        length: 1.0,
        v_inc: c64::new(1.0, 0.0),
    };
    let omegas = [0.10_f64];

    // f64 Direct reference (faer sparse LU) on the ndarray `TestBackend`.
    let direct = driven_frequency_sweep::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        &device(),
    )
    .expect("direct sweep (f64 reference)");

    // CUDA-f32 matrix-free driven solve. A loose f32 COCG stopping tolerance is
    // appropriate — f32 cannot reach the 1e-10 the f64 gates request.
    type Cu = Cuda;
    let dev_cu = <Cu as BackendTypes>::Device::default();
    let settings = IterativeSettings::new(1e-6, 5_000);
    let matrix_free = driven_frequency_sweep_with_mode::<Cu>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &omegas,
        &zero_source(&mesh),
        SolverMode::IterativeMatrixFree(settings),
        &dev_cu,
    )
    .expect("matrix-free sweep (CUDA f32)");

    assert_eq!(direct.len(), matrix_free.len());
    let pt_d = &direct[0];
    let pt_m = &matrix_free[0];

    // (a) It converges — records a positive iteration count.
    let iters = pt_m.iters_per_rhs[0];
    assert!(
        iters > 0,
        "CUDA f32 smoke: matrix-free path must record iterations"
    );

    // (b) Port Z(ω) agrees with the f64 Direct reference at an f32-loose band.
    let z_d = pt_d.ports[0].z;
    let z_m = pt_m.ports[0].z;
    let rel_diff = (z_d - z_m).norm() / z_d.norm().max(1e-30);
    eprintln!(
        "[#499 / driven CUDA f32 smoke] ω = {:.3}: Z_direct = {z_d}, \
         Z_cuda_f32 = {z_m}, rel_diff = {:.3e}, BurnCOCG iters = {iters}, \
         residual_rel = {:.3e}",
        pt_d.omega, rel_diff, pt_m.residual_rel,
    );
    assert!(
        rel_diff < 1e-3,
        "CUDA f32 smoke: Z agreement |Z_d − Z_mf| / |Z_d| = {rel_diff} (f32 tol 1e-3)"
    );
}
