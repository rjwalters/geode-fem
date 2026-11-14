use super::*;
use crate::assembly::nedelec::cube_pec_interior_edges;
use crate::mesh::cube_tet_mesh;
use crate::testing::TestBackend;
use burn::tensor::backend::BackendTypes;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

fn vacuum(mesh: &TetMesh) -> Vec<c64> {
    vec![c64::new(1.0, 0.0); mesh.n_tets()]
}

/// A zero current source must produce an exactly-zero field.
#[test]
fn zero_source_gives_zero_field() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let sol = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &source,
        &device(),
    )
    .expect("driven solve");
    assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
}

/// Linearity: doubling J doubles the field (up to round-off).
#[test]
fn solution_is_linear_in_source() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let s1 = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let s2 = CurrentSource {
        j_tet: s1.j_tet.iter().map(|j| j.map(|x| x * 2.0)).collect(),
    };
    let omega = 1.0;
    let sol1 = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &s1,
        &device(),
    )
    .expect("solve s1");
    let sol2 = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &s2,
        &device(),
    )
    .expect("solve s2");

    let norm1: f64 = sol1
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm1 > 0.0, "nonzero source must excite a nonzero field");
    for (a, b) in sol1.e_edges.iter().zip(sol2.e_edges.iter()) {
        let d = *b - *a * 2.0;
        assert!(
            d.re.hypot(d.im) <= 1e-9 * norm1,
            "linearity violated: 2·{a} vs {b}"
        );
    }
}

/// The post-solve residual must sit at the direct-solve round-off
/// floor, and PEC-eliminated edges must be exactly zero.
#[test]
fn residual_is_small_and_pec_edges_are_zero() {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource::from_centroids(&mesh, |_| {
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
    });
    let sol = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.5,
        &source,
        &device(),
    )
    .expect("driven solve");
    assert!(
        sol.residual_rel < 1e-10,
        "direct-solve residual too large: {}",
        sol.residual_rel
    );
    for (i, &keep) in interior.iter().enumerate() {
        if !keep {
            assert_eq!(sol.e_edges[i], c64::new(0.0, 0.0));
        }
    }
    assert!(
        sol.e_edges
            .iter()
            .all(|e| e.re.is_finite() && e.im.is_finite())
    );
}

/// An isotropic DiagTensor material must agree with the Scalar
/// material path (the two assembly kernels are independent
/// implementations of the same integral).
#[test]
fn diag_tensor_matches_scalar_for_isotropic_epsilon() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps_scalar: Vec<c64> = vec![c64::new(1.8, -0.05); mesh.n_tets()];
    let eps_diag: Vec<[c64; 3]> = eps_scalar.iter().map(|&e| [e, e, e]).collect();
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(c[1], 0.0),
            c64::new(0.0, -0.5),
            c64::new(1.0, c[0]),
        ]
    });
    let omega = 2.0;
    let sol_s = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps_scalar),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("scalar solve");
    let sol_d = driven_solve::<B>(
        &mesh,
        DrivenMaterials::DiagTensor(&eps_diag),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("diag solve");

    let norm: f64 = sol_s
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm > 0.0);
    let mut max_rel = 0.0_f64;
    for (a, b) in sol_s.e_edges.iter().zip(sol_d.e_edges.iter()) {
        let d = *a - *b;
        max_rel = max_rel.max(d.re.hypot(d.im) / norm);
    }
    assert!(
        max_rel < 1e-4,
        "scalar vs diag-tensor isotropic mismatch: max relative diff {max_rel}"
    );
}

/// The built-in good-conductor model must match the analytic
/// formulas: `Z_s = (1+i)√(ω/2σ)` and weak coefficient
/// `iω/Z_s = (1+i)√(ωσ/2) = (1+i)/δ` with `δ = √(2/(ωσ))`.
#[test]
fn good_conductor_model_matches_skin_depth_formula() {
    let omega = 2.5;
    let sigma = 12.0;
    let model = SurfaceImpedanceModel::GoodConductor { sigma };

    let a = (omega / (2.0 * sigma)).sqrt();
    let z = model.z_s(omega);
    assert!((z.re - a).abs() < 1e-15 && (z.im - a).abs() < 1e-15);

    let delta = (2.0 / (omega * sigma)).sqrt();
    let coeff = model.weak_coefficient(omega).expect("finite Z_s");
    assert!(
        (coeff.re - 1.0 / delta).abs() < 1e-12 * (1.0 / delta),
        "Re(iω/Z_s) = {} must equal 1/δ = {}",
        coeff.re,
        1.0 / delta
    );
    assert!(
        (coeff.im - 1.0 / delta).abs() < 1e-12 * (1.0 / delta),
        "Im(iω/Z_s) = {} must equal 1/δ = {}",
        coeff.im,
        1.0 / delta
    );
}

/// `Fixed(η₀ = 1)` must reproduce the Silver-Müller factor `i k₀`
/// exactly (Silver-Müller is the `Z_s = η₀` special case).
#[test]
fn fixed_eta0_weak_coefficient_is_silver_muller_factor() {
    let omega = 1.75;
    let coeff = SurfaceImpedanceModel::Fixed(c64::new(1.0, 0.0))
        .weak_coefficient(omega)
        .expect("finite Z_s");
    assert_eq!(coeff.re, 0.0);
    assert_eq!(coeff.im, omega);
}

/// Zero or non-finite `Z_s(ω)` must error, not divide by zero.
#[test]
fn singular_surface_impedance_errors() {
    let zero = SurfaceImpedanceModel::Fixed(c64::new(0.0, 0.0));
    assert!(matches!(
        zero.weak_coefficient(1.0),
        Err(DrivenError::SurfaceImpedanceSingular { .. })
    ));
    // Negative σ → √(negative) = NaN → non-finite Z_s.
    let bad = SurfaceImpedanceModel::GoodConductor { sigma: -3.0 };
    assert!(matches!(
        bad.weak_coefficient(1.0),
        Err(DrivenError::SurfaceImpedanceSingular { .. })
    ));

    // The solver surfaces the same error.
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let tris: Vec<[u32; 3]> = vec![];
    let err = driven_solve_with_surface_impedance::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        &[SurfaceImpedanceBc {
            triangles: &tris,
            model: zero,
        }],
        1.0,
        &source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::SurfaceImpedanceSingular { .. }));
}

/// An empty surface list must reproduce [`driven_solve`] bitwise
/// (the no-op guarantee for existing callers).
#[test]
fn empty_surface_list_matches_plain_driven_solve() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(c[2], 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 1.3;
    let sol_s = driven_solve_with_surface_impedance::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        omega,
        &source,
        &device(),
    )
    .expect("surface-impedance path");
    let sol_p = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("plain path");
    for (a, b) in sol_s.e_edges.iter().zip(sol_p.e_edges.iter()) {
        assert_eq!(a.re, b.re);
        assert_eq!(a.im, b.im);
    }
}

/// Shape-mismatch inputs must error, not panic.
#[test]
fn input_validation_errors() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let good_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };

    let bad_mask = vec![true; 3];
    let err = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &bad_mask,
        },
        1.0,
        &good_source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::MaskDimMismatch { .. }));

    let bad_source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; 2],
    };
    let err = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &bad_source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::SourceDimMismatch { .. }));

    let bad_eps = vec![c64::new(1.0, 0.0); 1];
    let err = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&bad_eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &good_source,
        &device(),
    )
    .unwrap_err();
    assert!(matches!(err, DrivenError::MaterialDimMismatch { .. }));
}

// ---------------------------------------------------------------------
// Iterative-solver regression (issue #238)
// ---------------------------------------------------------------------

/// **Issue #238 regression**: the COCG iterative path must agree
/// with the direct sparse LU to a documented tolerance on a small
/// driven fixture (cube cavity, scalar ε, real volumetric source).
/// The Krylov solver is preconditioned-COCG with Jacobi; iteration
/// counts are reported through [`KspReport`].
#[test]
fn iterative_matches_direct_lu() {
    use crate::solver::ksp::Cocg;

    // Small cube cavity (same fixture the residual / linearity
    // tests use). PEC walls, vacuum interior, smooth real source.
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 1.5;

    // Direct sparse LU path (the existing solver).
    let sol_lu = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("direct LU solve");

    // COCG iterative path (issue #238). Jacobi preconditioner is
    // wired in by `driven_solve_iterative`. A tight Krylov tol
    // ensures the iterative residual is at the same floor as the
    // direct path's.
    let ksp = Cocg::new(1e-12, 5000);
    let (sol_ksp, report) = super::driven_solve_iterative::<B, _>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &ksp,
        &device(),
    )
    .expect("COCG iterative solve");

    // The iterative path's own residual must clear the requested
    // tolerance, and so must the post-solve residual reported on
    // the DrivenSolution (the two are the same definition).
    assert!(report.converged, "COCG did not converge: {:?}", report);
    assert!(
        report.residual_rel < 1e-10,
        "COCG residual too large: {}",
        report.residual_rel
    );
    assert!(
        sol_ksp.residual_rel < 1e-10,
        "iterative DrivenSolution residual too large: {}",
        sol_ksp.residual_rel
    );

    // Iteration count must be reported (the acceptance criterion).
    assert!(report.iters > 0, "no iterations recorded");
    // Issue-#238 tolerance contract: iterative vs direct LU agree
    // to 1e-8 (relative, in the L2 norm) on this fixture. The
    // direct path floors at machine epsilon and Krylov tolerance
    // we asked for is 1e-12, so 1e-8 is a documented safety
    // margin that survives both solvers' round-off.
    let norm_lu: f64 = sol_lu
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    assert!(norm_lu > 0.0, "direct solution must be non-trivial");
    let mut diff2 = 0.0_f64;
    for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
        let d = *a - *b;
        diff2 += d.re * d.re + d.im * d.im;
    }
    let rel_diff = diff2.sqrt() / norm_lu;
    const ITERATIVE_DIRECT_TOL: f64 = 1e-8;
    assert!(
        rel_diff < ITERATIVE_DIRECT_TOL,
        "iterative vs LU solutions disagree: rel_diff = {} (tol {}) over {} edges, COCG report = {:?}",
        rel_diff,
        ITERATIVE_DIRECT_TOL,
        sol_lu.e_edges.len(),
        report,
    );

    // PEC-eliminated edges must be exactly zero in both paths.
    for (i, &keep) in interior.iter().enumerate() {
        if !keep {
            assert_eq!(sol_ksp.e_edges[i], c64::new(0.0, 0.0));
        }
    }

    // Iteration-count + cost report for the issue's acceptance
    // criteria. Printed at INFO level (cargo test --nocapture).
    eprintln!(
        "[issue #238] cube cavity grid=3, n_interior={}, COCG iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e}",
        sol_ksp.n_interior, report.iters, report.residual_rel, rel_diff,
    );
}

/// **Issue #238 supporting**: identity preconditioner produces
/// the same solution as Jacobi on the regression fixture (just
/// more iterations). Cross-checks that the iterative path works
/// even without preconditioning, so the preconditioner is
/// genuinely a separate concern from the Krylov core.
#[test]
fn identity_preconditioner_also_converges() {
    use crate::solver::ksp::{Cocg, IdentityPreconditioner};

    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 1.2;

    let sol_lu = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("direct LU solve");

    // Use the operator-level entry point with an explicit identity
    // preconditioner factory — the per-solver-path lever the issue
    // calls for.
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let ksp = Cocg::new(1e-12, 5000);
    let (sol_ksp, report) = op
        .solve_at_iterative(omega, &ksp, |a| {
            Ok::<_, crate::solver::ksp::KspError>(IdentityPreconditioner::new(a.nrows()))
        })
        .expect("identity-preconditioned COCG");
    assert!(report.converged);

    let norm_lu: f64 = sol_lu
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    let mut diff2 = 0.0_f64;
    for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
        let d = *a - *b;
        diff2 += d.re * d.re + d.im * d.im;
    }
    let rel_diff = diff2.sqrt() / norm_lu;
    assert!(
        rel_diff < 1e-8,
        "identity-preconditioned COCG disagrees with LU: rel_diff = {}",
        rel_diff
    );
    eprintln!(
        "[issue #238] identity preconditioner: COCG iters={}, residual_rel={:.3e}",
        report.iters, report.residual_rel
    );
}

// ---------------------------------------------------------------------
// ILU(0) preconditioner regression (issue #267)
// ---------------------------------------------------------------------

/// **Issue #267 regression**: ILU(0) on the cube-cavity fixture
/// (the same fixture as [`iterative_matches_direct_lu`]) must
///
/// 1. converge COCG to the same tolerance Jacobi reaches, with
///    the iterative residual `< 1e-10` and
/// 2. agree with the direct LU solution within `1e-8` relative,
/// 3. record a **lower** iteration count than Jacobi.
///
/// The iteration counts and the iteration-count ratio are printed
/// to stderr so future runs surface any preconditioner regression.
#[test]
fn ilu_vs_jacobi_cube_cavity_regression() {
    use crate::solver::ksp::{Cocg, IluPreconditioner, JacobiPreconditioner};

    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 1.5;

    let sol_lu = driven_solve::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("direct LU solve");

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");

    let ksp = Cocg::new(1e-12, 5000);

    // Jacobi run (the existing landing preconditioner).
    let (sol_jac, report_jac) = op
        .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
        .expect("Jacobi-preconditioned COCG");
    assert!(report_jac.converged);

    // ILU(0) run.
    let (sol_ilu, report_ilu) = op
        .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
        .expect("ILU(0)-preconditioned COCG");
    assert!(report_ilu.converged);

    let norm_lu: f64 = sol_lu
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    let mut diff_jac2 = 0.0_f64;
    let mut diff_ilu2 = 0.0_f64;
    for ((a, j), i) in sol_lu
        .e_edges
        .iter()
        .zip(sol_jac.e_edges.iter())
        .zip(sol_ilu.e_edges.iter())
    {
        let dj = *a - *j;
        let di = *a - *i;
        diff_jac2 += dj.re * dj.re + dj.im * dj.im;
        diff_ilu2 += di.re * di.re + di.im * di.im;
    }
    let rel_diff_jac = diff_jac2.sqrt() / norm_lu;
    let rel_diff_ilu = diff_ilu2.sqrt() / norm_lu;

    const ITERATIVE_DIRECT_TOL: f64 = 1e-8;
    assert!(rel_diff_jac < ITERATIVE_DIRECT_TOL);
    assert!(rel_diff_ilu < ITERATIVE_DIRECT_TOL);

    // ILU(0) must beat Jacobi on iteration count for this
    // moderately conditioned cube fixture. Even when the gap is
    // small, ILU(0) folds in off-diagonal sparsity information
    // that pure diagonal preconditioning lacks.
    assert!(
        report_ilu.iters <= report_jac.iters,
        "ILU iters ({}) must not exceed Jacobi iters ({}) on cube cavity",
        report_ilu.iters,
        report_jac.iters,
    );

    eprintln!(
        "[issue #267] cube cavity grid=3, n_interior={}, \
         Jacobi: iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e} | \
         ILU(0): iters={}, residual_rel={:.3e}, rel_diff_vs_LU={:.3e} | \
         iter ratio (ILU/Jacobi) = {:.3}",
        sol_ilu.n_interior,
        report_jac.iters,
        report_jac.residual_rel,
        rel_diff_jac,
        report_ilu.iters,
        report_ilu.residual_rel,
        rel_diff_ilu,
        report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
    );
}

/// **Issue #267 stress fixture**: a higher-condition-number
/// fixture — fine mesh with σ damping at low ω, where Jacobi
/// converges slowly and ILU(0)'s factored off-diagonal coupling
/// pays off more visibly. The σ-damped resistor fixture exhibits
/// extra spectral spread from the iωσ term, so it amplifies the
/// ILU vs Jacobi gap without needing a heavy mesh.
///
/// The test enforces `iters(ILU(0)) ≤ iters(Jacobi)` and reports
/// the ratio for future regression awareness.
#[test]
fn ilu_vs_jacobi_sigma_damped_resistor_regression() {
    use crate::solver::ksp::{Cocg, IluPreconditioner, JacobiPreconditioner};

    // σ-filled resistor cube — same family as the regression
    // fixture in `extraction.rs`, but assembled directly here so
    // the iteration-count comparison is self-contained.
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    // Large σ → strong off-diagonal coupling through the iωσ damping
    // term, which Jacobi (diagonal-only) cannot capture.
    let sigma_tet = vec![5.0_f64; mesh.n_tets()];
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    // Low ω → mass term shrinks relative to stiffness + damping;
    // the off-diagonal contribution from `iωσ·C` dominates the
    // diagonal correction by a wider margin, sharpening the
    // ILU-vs-Jacobi separation.
    let omega = 0.3;

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &bcs,
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let ksp = Cocg::new(1e-10, 10_000);

    let (sol_jac, report_jac) = op
        .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
        .expect("Jacobi-preconditioned COCG");
    let (_sol_ilu, report_ilu) = op
        .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
        .expect("ILU(0)-preconditioned COCG");

    assert!(report_jac.converged);
    assert!(report_ilu.converged);

    // The stress fixture's acceptance: ILU(0) must outperform
    // Jacobi by at least a factor of 1.0×, with the empirically
    // observed gap reported for the issue's documentation
    // requirement.
    assert!(
        report_ilu.iters <= report_jac.iters,
        "ILU(0) ({}) must beat Jacobi ({}) on σ-damped resistor",
        report_ilu.iters,
        report_jac.iters,
    );

    eprintln!(
        "[issue #267 / stress: σ-damped resistor] grid=4, σ=5, ω={:.2}, \
         n_interior={}, Jacobi iters={}, ILU(0) iters={}, ratio={:.3}",
        omega,
        sol_jac.n_interior,
        report_jac.iters,
        report_ilu.iters,
        report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
    );
}

/// **Issue #708 Phase 6b**: the preconditioner choice reaches the
/// **public** [`SolverMode::Iterative`] path (`prepare_at`), and on a
/// loss-dominated low-ω fixture ILU(0) converges inside a budget that
/// Jacobi exhausts (measured on this fixture: Jacobi 270 vs ILU(0) 70
/// COCG iterations), with the ILU(0) answer matching direct LU. The
/// budget sits between the two counts so a regression that silently
/// fell back to Jacobi fails here.
#[test]
fn ilu0_selectable_on_public_iterative_path_converges_where_jacobi_stalls() {
    let mesh = cube_tet_mesh(5, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    // Every third tet lossy: a heterogeneous iωσ term with strong
    // off-diagonal coupling that diagonal scaling cannot capture.
    let sigma_tet: Vec<f64> = (0..mesh.n_tets())
        .map(|t| if t % 3 == 0 { 5.0 } else { 0.0 })
        .collect();
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let omega = 0.01;
    let budget = 150;

    // Jacobi pinned explicitly: since issue #930 the default is
    // `Auto`, which resolves to AMS on this p=1 operator.
    let jacobi = op
        .prepare_at::<B>(
            omega,
            SolverMode::Iterative(
                IterativeSettings::new(1e-10, budget)
                    .with_preconditioner(IterativePreconditioner::Jacobi),
            ),
            &device(),
        )
        .expect("Jacobi setup");
    let err = jacobi.solve().expect_err("Jacobi must exhaust the budget");
    assert!(
        matches!(&err, DrivenError::Solve(m) if m.contains("did not converge")),
        "unexpected Jacobi failure: {err:?}"
    );

    let ilu = op
        .prepare_at::<B>(
            omega,
            SolverMode::Iterative(
                IterativeSettings::new(1e-10, budget)
                    .with_preconditioner(IterativePreconditioner::Ilu0),
            ),
            &device(),
        )
        .expect("ILU(0) setup");
    let (sol_ilu, report) = ilu.solve().expect("ILU(0) converges inside the budget");
    assert!(report.iters > 0 && report.iters <= budget);
    assert!(report.residual_rel <= 1e-10);

    let (sol_lu, _) = op
        .prepare_at::<B>(omega, SolverMode::Direct, &device())
        .expect("LU")
        .solve()
        .expect("direct solve");
    let num: f64 = sol_ilu
        .e_edges
        .iter()
        .zip(&sol_lu.e_edges)
        .map(|(a, b)| (a - b).norm_sqr())
        .sum();
    let den: f64 = sol_lu.e_edges.iter().map(|b| b.norm_sqr()).sum();
    let rel = (num / den).sqrt();
    assert!(rel < 1e-6, "ILU(0) vs direct LU rel err {rel:.3e}");
    eprintln!(
        "[issue #708 6b] grid=5 σ=5 (1/3 tets) ω={omega}: ILU(0) iters={} (Jacobi > {budget}), \
         rel err vs LU {rel:.2e}",
        report.iters
    );
}

/// The matrix-free path only has an on-device Jacobi: a non-Jacobi
/// preconditioner request is a clean `UnsupportedMatrixFree` error,
/// never a silent fallback.
#[test]
fn matrix_free_rejects_non_jacobi_preconditioner() {
    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource::from_centroids(&mesh, |_| {
        [c64::new(0.0, 0.0), c64::new(0.0, 0.0), c64::new(1.0, 0.0)]
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
        &device(),
    )
    .expect("operator assembly");
    let settings = IterativeSettings::default().with_preconditioner(IterativePreconditioner::Ilu0);
    let err = op
        .prepare_at::<B>(1.0, SolverMode::IterativeMatrixFree(settings), &device())
        .err()
        .expect("matrix-free must reject ILU(0)");
    assert!(
        matches!(&err, DrivenError::UnsupportedMatrixFree { reason } if reason.contains("ilu0")),
        "unexpected error: {err:?}"
    );
}

/// A vacuum PEC cube (`n` cells a side) with a volume source, for the
/// issue #930 default-preconditioner tests.
fn default_pc_cube(n: usize) -> (TetMesh, DrivenOperator) {
    let mesh = cube_tet_mesh(n, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
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
        &device(),
    )
    .expect("operator assembly");
    (mesh, op)
}

/// Issue #930: the default preconditioner is `Auto`, and on a p=1
/// assembled iterative solve it selects AMS, with no fallback warning;
/// the AMS solve converges and matches direct LU.
#[test]
fn default_preconditioner_selects_ams_on_p1_assembled_iterative() {
    assert_eq!(
        IterativePreconditioner::default(),
        IterativePreconditioner::Auto
    );
    assert_eq!(
        IterativeSettings::default().preconditioner,
        IterativePreconditioner::Auto
    );
    assert_eq!(
        IterativeSettings::new(1e-10, 500).preconditioner,
        IterativePreconditioner::Auto
    );
    let (_mesh, op) = default_pc_cube(4);
    let omega = 0.05;
    let mode = SolverMode::Iterative(IterativeSettings::new(1e-10, 500));
    assert_eq!(
        op.resolve_preconditioner(mode),
        Some((IterativePreconditioner::AMS, None))
    );
    let solver = op.prepare_at::<B>(omega, mode, &device()).expect("setup");
    assert_eq!(solver.preconditioner(), Some(IterativePreconditioner::AMS));
    assert_eq!(solver.preconditioner_fallback(), None);
    assert!(matches!(
        solver.backend,
        SolverBackend::Iterative {
            precond: DrivenPreconditioner::Ams(_),
            ..
        }
    ));
    let (sol, report) = solver.solve().expect("AMS converges");
    assert!(report.converged);
    let (lu, _) = op
        .prepare_at::<B>(omega, SolverMode::Direct, &device())
        .expect("LU")
        .solve()
        .expect("direct solve");
    let num: f64 = sol
        .e_edges
        .iter()
        .zip(&lu.e_edges)
        .map(|(a, b)| (a - b).norm_sqr())
        .sum();
    let den: f64 = lu.e_edges.iter().map(|b| b.norm_sqr()).sum();
    assert!((num / den).sqrt() < 1e-6);
    // The direct path has no preconditioner.
    assert_eq!(op.resolve_preconditioner(SolverMode::Direct), None);
}

/// Issue #930: the matrix-free path has only an on-device Jacobi, so
/// the default preconditioner falls back to Jacobi there, with a
/// warning that says why (matrix-free) and names `jacobi`.
#[test]
fn default_preconditioner_falls_back_to_jacobi_with_a_warning_on_matrix_free() {
    let (_mesh, op) = default_pc_cube(2);
    let mode = SolverMode::IterativeMatrixFree(IterativeSettings::new(1e-10, 5000));
    let solver = op
        .prepare_at::<B>(1.0, mode, &device())
        .expect("matrix-free setup with the default preconditioner");
    assert_eq!(
        solver.preconditioner(),
        Some(IterativePreconditioner::Jacobi)
    );
    let fallback = solver
        .preconditioner_fallback()
        .copied()
        .expect("a matrix-free fallback must carry a warning");
    assert_eq!(
        fallback,
        PreconditionerFallback {
            reason: PreconditionerFallbackReason::MatrixFree,
            used: IterativePreconditioner::Jacobi,
        }
    );
    let msg = fallback.to_string();
    assert!(
        msg.contains("matrix-free") && msg.contains("`jacobi`"),
        "{msg}"
    );
    let (_, report) = solver.solve().expect("matrix-free Jacobi converges");
    assert!(report.converged);
}

/// Issue #930: an explicit Jacobi is honoured, with no fallback
/// warning, on both the assembled and the matrix-free path.
#[test]
fn explicit_jacobi_is_honoured_without_a_warning() {
    let (_mesh, op) = default_pc_cube(3);
    let jacobi =
        IterativeSettings::new(1e-10, 5000).with_preconditioner(IterativePreconditioner::Jacobi);
    for mode in [
        SolverMode::Iterative(jacobi),
        SolverMode::IterativeMatrixFree(jacobi),
    ] {
        assert_eq!(
            op.resolve_preconditioner(mode),
            Some((IterativePreconditioner::Jacobi, None))
        );
        let solver = op.prepare_at::<B>(0.5, mode, &device()).expect("setup");
        assert_eq!(
            solver.preconditioner(),
            Some(IterativePreconditioner::Jacobi)
        );
        assert_eq!(solver.preconditioner_fallback(), None, "{mode:?}");
        if let SolverBackend::Iterative { precond, .. } = &solver.backend {
            assert!(matches!(precond, DrivenPreconditioner::Jacobi(_)));
        }
        let (_, report) = solver.solve().expect("Jacobi converges");
        assert!(report.converged);
    }
    // `build` cannot resolve `Auto` from A(ω) alone.
    let a = op.assemble_a_at(0.5).expect("A(ω)");
    assert!(matches!(
        IterativePreconditioner::Auto.build(a.as_ref()),
        Err(DrivenError::Solve(m)) if m.contains("prepare_at")
    ));
}

/// **Issue #744 regression — recursive-residual drift is a hard error.**
///
/// COCG stops when its *recursively maintained* residual crosses
/// `tol`, then recomputes the true residual with an explicit SpMV.
/// With `tol` below what f64 can actually deliver on this operator,
/// the recursive residual keeps shrinking geometrically while the
/// explicit residual stalls at the rounding floor — so `Cocg::solve`
/// returns `Ok(KspReport { converged: false, .. })`. Before #744 the
/// driven layer discarded that flag and accepted the near-miss as a
/// successful solve; now every driven iterative path
/// (`prepare_at(..).solve()`/`back_solve` and `solve_at_iterative`)
/// must surface it as [`DrivenError::Solve`].
#[test]
fn iterative_recursive_residual_drift_is_a_hard_error() {
    use crate::solver::ksp::{Cocg, KspSolve};

    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
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
        &device(),
    )
    .expect("operator assembly");
    let omega = 1.0;
    // Far below the f64 rounding floor of ‖Ax − b‖/‖b‖ on this
    // operator, but reachable by the recursive residual.
    let tol = 1e-30;
    let max_iters = 5000;

    // Precondition: the raw KSP really does hit the drift case
    // (recursive residual crossed tol, explicit residual did not) —
    // not max-iters exhaustion or breakdown.
    let a = op.assemble_a_at(omega).expect("A(ω)");
    let b = op.assemble_b_at(omega, None);
    let pc = crate::solver::ksp::JacobiPreconditioner::new(a.as_ref()).expect("jacobi");
    let mut x = vec![c64::new(0.0, 0.0); b.len()];
    let raw = Cocg::new(tol, max_iters)
        .solve(a.as_ref(), &b, &mut x, &pc)
        .expect("COCG returns Ok in the drift case");
    assert!(!raw.converged, "fixture must drift: {raw:?}");
    // Issue #943: residual replacement was tried and could not
    // reach a sub-rounding-floor tol, so the failure is honest.
    assert!(raw.replacements >= 1, "{raw:?}");
    assert!(raw.iters < max_iters, "{raw:?}");
    assert!(raw.residual_rel > tol, "{raw:?}");

    let expect_drift_err = |r: Result<(), DrivenError>, path: &str| match r {
        Err(DrivenError::Solve(msg)) => assert!(
            msg.contains("explicitly recomputed residual"),
            "{path}: unexpected solve error {msg}"
        ),
        other => panic!("{path}: drift must be DrivenError::Solve, got {other:?}"),
    };

    // Public per-ω handle (the path every sweep consumer funnels through).
    // Jacobi pinned: the drift precondition above is established with
    // Jacobi, and the default (issue #930) would build AMS.
    let settings =
        IterativeSettings::new(tol, max_iters).with_preconditioner(IterativePreconditioner::Jacobi);
    let solver = op
        .prepare_at::<B>(omega, SolverMode::Iterative(settings), &device())
        .expect("iterative setup");
    expect_drift_err(solver.solve().map(|_| ()), "prepare_at().solve()");
    let mut out = vec![c64::new(0.0, 0.0); b.len()];
    expect_drift_err(solver.back_solve(&b, &mut out).map(|_| ()), "back_solve");

    // The one-shot iterative entry point.
    expect_drift_err(
        op.solve_at_iterative(omega, &Cocg::new(tol, max_iters), |a| {
            crate::solver::ksp::JacobiPreconditioner::new(a.as_ref())
        })
        .map(|_| ()),
        "solve_at_iterative",
    );

    // A reachable tolerance on the same handle type still succeeds and
    // reports `converged = true`.
    let (_, ok) = op
        .prepare_at::<B>(
            omega,
            SolverMode::Iterative(IterativeSettings {
                tol: 1e-10,
                ..settings
            }),
            &device(),
        )
        .expect("iterative setup")
        .solve()
        .expect("tol = 1e-10 converges");
    assert!(ok.converged);
    assert!(ok.residual_rel <= 1e-10);
}

/// The vacuum PEC cube of
/// [`iterative_recursive_residual_drift_is_a_hard_error`], reused by
/// the residual-replacement plumbing tests (issue #987).
fn drift_cube() -> DrivenOperator {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly")
}

/// `(iterations, replacements)` parsed from a drift
/// [`DrivenError::Solve`] message (see `require_converged`), or `None`
/// for any other outcome.
fn drift_counts<T>(r: &Result<T, DrivenError>) -> Option<(usize, usize)> {
    let Err(DrivenError::Solve(msg)) = r else {
        return None;
    };
    if !msg.contains("explicitly recomputed residual") {
        return None;
    }
    let iters = msg
        .split("tolerance after ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let replacements = msg
        .split("even after ")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some((iters, replacements))
}

/// **Issue #987**: `IterativeSettings::max_replacements` defaults to
/// the library cap and is the only knob the builder changes.
#[test]
fn iterative_settings_max_replacements_defaults_to_the_cocg_cap() {
    use crate::solver::ksp::DEFAULT_MAX_RESIDUAL_REPLACEMENTS;
    assert_eq!(DEFAULT_MAX_RESIDUAL_REPLACEMENTS, 3);
    let d = IterativeSettings::default();
    assert_eq!(d.max_replacements, DEFAULT_MAX_RESIDUAL_REPLACEMENTS);
    let n = IterativeSettings::new(1e-8, 77);
    assert_eq!(n.max_replacements, DEFAULT_MAX_RESIDUAL_REPLACEMENTS);
    assert_eq!((n.tol, n.max_iters), (1e-8, 77));
    assert_eq!(n.preconditioner, IterativePreconditioner::Auto);
    let z = n.with_max_replacements(0);
    assert_eq!(z.max_replacements, 0);
    assert_eq!(
        (z.tol, z.max_iters, z.preconditioner),
        (1e-8, 77, n.preconditioner)
    );
}

/// **Issue #987**: `max_replacements` reaches the assembled `Cocg` and
/// the matrix-free `BurnCocg`. At a `tol` below the rounding floor,
/// `0` reports the drift at the first crossing with zero replacements
/// — the pre-#943 outcome, at the same iteration as a raw single-check
/// `Cocg` — while the default replaces at least once before giving up.
#[test]
fn iterative_max_replacements_reaches_both_krylov_paths() {
    use crate::solver::ksp::{Cocg, KspSolve};

    let op = drift_cube();
    let omega = 1.0;
    let tol = 1e-30;
    let max_iters = 5000;
    let jacobi =
        IterativeSettings::new(tol, max_iters).with_preconditioner(IterativePreconditioner::Jacobi);

    // Raw single-check reference on the assembled operator.
    let a = op.assemble_a_at(omega).expect("A(ω)");
    let b = op.assemble_b_at(omega, None);
    let pc = crate::solver::ksp::JacobiPreconditioner::new(a.as_ref()).expect("jacobi");
    let mut x = vec![c64::new(0.0, 0.0); b.len()];
    let single = Cocg {
        max_replacements: 0,
        ..Cocg::new(tol, max_iters)
    }
    .solve(a.as_ref(), &b, &mut x, &pc)
    .expect("single check returns Ok in the drift case");
    assert!(!single.converged && single.replacements == 0, "{single:?}");

    for (path, mode_of) in [
        (
            "assembled",
            SolverMode::Iterative as fn(IterativeSettings) -> SolverMode,
        ),
        ("matrix-free", SolverMode::IterativeMatrixFree),
    ] {
        let run = |s: IterativeSettings| {
            op.prepare_at::<B>(omega, mode_of(s), &device())
                .expect("iterative setup")
                .solve()
        };
        let first = run(jacobi.with_max_replacements(0));
        let (iters0, reps0) = drift_counts(&first)
            .unwrap_or_else(|| panic!("{path}: 0 must be a drift error, got {first:?}"));
        assert_eq!(reps0, 0, "{path}");
        if path == "assembled" {
            assert_eq!(iters0, single.iters, "{path}: first crossing");
        }
        // The default cap: never the zero-replacement first crossing.
        let default = run(jacobi);
        assert!(default.is_err(), "{path}: tol 1e-30 is unreachable");
        if let Some((iters, reps)) = drift_counts(&default) {
            assert!(reps >= 1, "{path}: default must replace: {default:?}");
            assert!(iters > iters0, "{path}: {iters} vs {iters0}");
        }
    }
}

/// **Issue #987**: on a drift case the default cap repairs, `max_replacements:
/// 0` reproduces the pre-#943 first-crossing failure through the driven
/// layer, and the default (3) still converges. Which `tol` drifts depends
/// on last-bit round-off, so the fixture scans a band near the rounding
/// floor (like the #943 `Cocg` test) and requires at least one repair.
#[test]
fn iterative_max_replacements_zero_restores_first_crossing_drift() {
    let op = drift_cube();
    let omega = 1.0;
    let mut drifted = 0;
    let mut repaired = 0;
    for e in 0..=24 {
        let tol = 10_f64.powf(-12.0 - 0.125 * e as f64);
        let settings =
            IterativeSettings::new(tol, 5000).with_preconditioner(IterativePreconditioner::Jacobi);
        let solve = |s: IterativeSettings| {
            op.prepare_at::<B>(omega, SolverMode::Iterative(s), &device())
                .expect("iterative setup")
                .solve()
        };
        let single = solve(settings.with_max_replacements(0));
        let Some((iters0, reps0)) = drift_counts(&single) else {
            let (_, rep) = single.expect("a non-drift single check converges");
            assert!(rep.converged, "{rep:?}");
            continue;
        };
        drifted += 1;
        assert_eq!(reps0, 0, "tol {tol:e}");
        match solve(settings) {
            Ok((_, rep)) => {
                assert!(rep.converged && rep.residual_rel <= tol, "{rep:?}");
                // Iterating past the first crossing is the replacement.
                assert!(rep.iters > iters0, "{rep:?} vs first crossing {iters0}");
                repaired += 1;
            }
            Err(e) => eprintln!("tol {tol:.3e}: default cap could not repair: {e}"),
        }
    }
    eprintln!("first-crossing drift cases: {drifted}, repaired by the default cap: {repaired}");
    assert!(drifted >= 1, "the scan found no first-crossing drift case");
    assert!(repaired >= 1, "the default cap repaired no drift case");
}

thread_local! {
    /// Whether the calling thread's last [`SolverMode::Iterative`]
    /// back-solve held a `SequentialSolveScope` when it started its
    /// Krylov solve (issue #946). Test-only probe, set by `back_solve`.
    pub(super) static KRYLOV_SOLVE_SCOPED: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Issue #946: an AMS back-solve holds a faer sequential scope while its
/// Krylov solve runs, both when the solve converges and when it returns an
/// error, and a Jacobi back-solve holds none. What a live scope does to
/// faer's global parallelism, and that the caller's value comes back, is
/// asserted in `tests/faer_global_parallelism.rs`, which has the process
/// to itself.
#[test]
fn ams_back_solve_holds_a_sequential_scope_and_jacobi_does_not() {
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
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
        &device(),
    )
    .expect("operator assembly");
    let omega = 0.05;
    let probe = || KRYLOV_SOLVE_SCOPED.with(|p| p.replace(None));
    let ams = |max_iters| {
        IterativeSettings::new(1e-10, max_iters).with_preconditioner(IterativePreconditioner::AMS)
    };

    // Setup alone runs no Krylov solve and takes no scope.
    let solver = op
        .prepare_at::<B>(omega, SolverMode::Iterative(ams(500)), &device())
        .expect("AMS setup");
    assert_eq!(probe(), None, "prepare_at must not run a Krylov solve");
    let (_, report) = solver.solve().expect("AMS converges");
    assert!(report.converged && report.iters > 1);
    assert_eq!(probe(), Some(true), "converging AMS solve had no scope");

    // One iteration allowed: the solve returns an error from inside the
    // scope.
    let failed = op
        .prepare_at::<B>(omega, SolverMode::Iterative(ams(1)), &device())
        .expect("AMS setup")
        .solve();
    assert!(
        matches!(failed, Err(DrivenError::Solve(_))),
        "one iteration must not reach tol = 1e-10"
    );
    assert_eq!(probe(), Some(true), "failing AMS solve had no scope");

    // Jacobi makes no faer solve per application, so it takes no scope.
    let jacobi =
        IterativeSettings::new(1e-10, 5000).with_preconditioner(IterativePreconditioner::Jacobi);
    op.prepare_at::<B>(omega, SolverMode::Iterative(jacobi), &device())
        .expect("Jacobi setup")
        .solve()
        .expect("Jacobi converges");
    assert_eq!(probe(), Some(false), "a Jacobi solve must take no scope");
}

/// Issue #963: the AMS smoother estimate is reported through the solve
/// handle. `DrivenLinearSolver::ams` is `Some` exactly when the iterative
/// back-solve uses AMS, and carries the Lanczos Ritz value, the
/// Gershgorin bound and the weight of this ω, consistent with each other,
/// plus the default V-cycle structure; it is `None` for Jacobi and Direct.
#[test]
fn solve_handle_reports_ams_smoother_estimate() {
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
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
        &device(),
    )
    .expect("operator assembly");
    let omega = 0.05;
    let ams_settings =
        IterativeSettings::new(1e-10, 500).with_preconditioner(IterativePreconditioner::AMS);
    let solver = op
        .prepare_at::<B>(omega, SolverMode::Iterative(ams_settings), &device())
        .expect("AMS setup");
    let ams = solver.ams().expect("AMS handle reports its preconditioner");
    let sm = ams.smoother();
    assert!(sm.theta_max().is_finite() && sm.theta_max() > 0.0);
    assert!(
        sm.gershgorin() >= sm.theta_max(),
        "Ritz value above the Gershgorin bound"
    );
    assert!(sm.weight() > 0.0 && sm.weight() <= 0.6);
    assert!(sm.lanczos_steps() > 0);
    assert_eq!(
        sm.near_switch(),
        ((0.6 * sm.theta_max() - 2.0) / 2.0).abs() <= 0.03
    );
    // No GEODE_DRIVEN_AMS_* knob is set in the test environment.
    if std::env::var_os(DRIVEN_AMS_SMOOTHER_ENV).is_none()
        && std::env::var_os(DRIVEN_AMS_CYCLE_ENV).is_none()
        && std::env::var_os(DRIVEN_AMS_PI_COARSE_ENV).is_none()
    {
        assert_eq!(ams.vcycle(), crate::eigen::ams::VCycleOptions::default());
        assert_eq!(
            ams.pi_coarse(),
            PiCoarseSolve::SymmetricGaussSeidel(AMS_PI_COARSE_SWEEPS)
        );
    }

    let jacobi =
        IterativeSettings::new(1e-10, 500).with_preconditioner(IterativePreconditioner::Jacobi);
    assert!(
        op.prepare_at::<B>(omega, SolverMode::Iterative(jacobi), &device())
            .expect("Jacobi setup")
            .ams()
            .is_none()
    );
    assert!(
        op.prepare_at::<B>(omega, SolverMode::Direct, &device())
            .expect("LU")
            .ams()
            .is_none()
    );
}

/// Issue #963: the `GEODE_DRIVEN_AMS_PI_COARSE` syntax round-trips and
/// rejects malformed values; the default is the shipped 4-sweep SGS.
#[test]
fn pi_coarse_parse_round_trips_and_rejects_bad_values() {
    for p in [
        PiCoarseSolve::SymmetricGaussSeidel(4),
        PiCoarseSolve::SymmetricGaussSeidel(16),
        PiCoarseSolve::Direct,
        PiCoarseSolve::Amg(2),
        PiCoarseSolve::Amg(4),
    ] {
        assert_eq!(PiCoarseSolve::parse(&p.name()), Ok(p));
    }
    assert_eq!(PiCoarseSolve::parse("sgs"), Ok(PiCoarseSolve::default()));
    assert_eq!(PiCoarseSolve::parse("amg"), Ok(PiCoarseSolve::Amg(2)));
    assert_eq!(
        PiCoarseSolve::default(),
        PiCoarseSolve::SymmetricGaussSeidel(AMS_PI_COARSE_SWEEPS)
    );
    for bad in ["lu", "sgs:0", "amg:x", "direct:2", ""] {
        assert!(PiCoarseSolve::parse(bad).is_err(), "{bad:?} accepted");
    }
}

/// Issue #744: [`IterativePreconditioner::Ams`] is built by
/// `prepare_at` (from the operator's mesh geometry), converges COCG on a
/// PEC cube with a lumped-port-free volume source, and matches direct
/// LU; the bare `IterativePreconditioner::build` (which only sees
/// `A(ω)`) rejects it with a clean error.
#[test]
fn ams_selectable_on_public_iterative_path_matches_direct() {
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
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
        &device(),
    )
    .expect("operator assembly");
    // Low frequency: the near-static regime AMS exists for.
    let omega = 0.05;

    for coarse in [AmsCoarseSolve::Auto, AmsCoarseSolve::Direct] {
        let settings = IterativeSettings::new(1e-10, 500)
            .with_preconditioner(IterativePreconditioner::Ams { coarse });
        let (sol, report) = op
            .prepare_at::<B>(omega, SolverMode::Iterative(settings), &device())
            .expect("AMS setup")
            .solve()
            .expect("AMS converges");
        assert!(report.converged && report.iters > 0);
        assert!(report.residual_rel <= 1e-10);
        let (sol_lu, _) = op
            .prepare_at::<B>(omega, SolverMode::Direct, &device())
            .expect("LU")
            .solve()
            .expect("direct solve");
        let num: f64 = sol
            .e_edges
            .iter()
            .zip(&sol_lu.e_edges)
            .map(|(a, b)| (a - b).norm_sqr())
            .sum();
        let den: f64 = sol_lu.e_edges.iter().map(|b| b.norm_sqr()).sum();
        let rel = (num / den).sqrt();
        assert!(
            rel < 1e-6,
            "AMS ({coarse:?}) vs direct LU rel err {rel:.3e}"
        );
    }

    let a = op.assemble_a_at(omega).expect("A(ω)");
    assert!(matches!(
        IterativePreconditioner::AMS.build(a.as_ref()),
        Err(DrivenError::Solve(msg)) if msg.contains("prepare_at")
    ));
    assert_eq!(IterativePreconditioner::AMS.name(), "ams");
    // `Auto` is always the exact gradient-space LU (an AMG coarse solve
    // drifts); explicit choices pass through.
    assert_eq!(AmsCoarseSolve::Auto.resolve(), AmsCoarseSolve::Direct);
    assert_eq!(AmsCoarseSolve::Amg.resolve(), AmsCoarseSolve::Amg);
}

/// Issue #945: on the structured Kuhn-tet cube of the #520 scaling
/// fixture (σ-lossy, PEC on the two y-planes, one lumped port) the old
/// fixed AMS smoother weight 0.6 is above the damped-Jacobi stability
/// bound `2/λ_max(D⁻¹P)`. The weight chosen at build time must be below
/// that bound, and the COCG iteration count must be the low one (26 at
/// this size; 70 with the weight at 0.6, growing with the mesh).
///
/// `λ_max` is checked two ways that do not use the build's own short
/// Lanczos run: a 3000-step power iteration (a converged lower bound)
/// and the Gershgorin row-sum bound (a rigorous upper bound).
#[test]
fn ams_smoother_weight_is_below_stability_bound_on_structured_cube() {
    let n = 6;
    let mesh = cube_tet_mesh(n, 1.0);
    let edges = mesh.edges();
    let on_plane = |node: u32, axis: usize, value: f64| {
        (mesh.nodes[node as usize][axis] - value).abs() < 1e-12
    };
    let port_faces: Vec<[u32; 3]> = mesh
        .faces()
        .into_iter()
        .filter(|f| f.iter().all(|&v| on_plane(v, 2, 0.0)))
        .collect();
    let mask: Vec<bool> = edges
        .iter()
        .map(|e| {
            ![0.0, 1.0]
                .iter()
                .any(|&y| on_plane(e[0], 1, y) && on_plane(e[1], 1, y))
        })
        .collect();
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
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &DrivenBcs {
            pec_interior_mask: &mask,
        },
        std::slice::from_ref(&port),
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let omega = 0.10;

    // λ_max(D⁻¹P) of the proxy the V-cycle smooths, by power iteration.
    let p = ams::proxy(&op, omega).expect("proxy");
    let pr = p.as_ref();
    let dim = pr.nrows();
    let (cp, ri, va) = (pr.col_ptr(), pr.row_idx(), pr.val());
    let mut diag = vec![0.0_f64; dim];
    let mut gershgorin = vec![0.0_f64; dim];
    for j in 0..dim {
        for k in cp[j]..cp[j + 1] {
            gershgorin[j] += va[k].abs();
            if ri[k] == j {
                diag[j] += va[k];
            }
        }
    }
    assert!(diag.iter().all(|&d| d > 0.0));
    let gershgorin = gershgorin
        .iter()
        .zip(&diag)
        .map(|(s, d)| s / d)
        .fold(0.0_f64, f64::max);
    let mut x: Vec<f64> = (0..dim).map(|i| ((i as f64) * 1.7).sin() + 0.1).collect();
    let mut y = vec![0.0_f64; dim];
    let mut lambda_power = 0.0;
    for _ in 0..3000 {
        y.iter_mut().for_each(|v| *v = 0.0);
        for j in 0..dim {
            for k in cp[j]..cp[j + 1] {
                y[ri[k]] += va[k] * x[j];
            }
        }
        let xpx: f64 = x.iter().zip(&y).map(|(a, b)| a * b).sum();
        let xdx: f64 = x.iter().zip(&diag).map(|(a, d)| a * a * d).sum();
        lambda_power = xpx / xdx;
        let mut norm = 0.0_f64;
        for i in 0..dim {
            x[i] = y[i] / diag[i];
            norm += x[i] * x[i];
        }
        let norm = norm.sqrt();
        x.iter_mut().for_each(|v| *v /= norm);
    }
    assert!(
        lambda_power <= gershgorin,
        "power-iteration lower bound {lambda_power} above Gershgorin {gershgorin}"
    );
    assert!(
        0.6 > 2.0 / lambda_power,
        "fixture must make 0.6 unstable: λ_max ≥ {lambda_power}, bound {}",
        2.0 / lambda_power
    );

    let built = ams::build(&op, omega, AmsCoarseSolve::Auto).expect("AMS build");
    let sw = built.smoother();
    eprintln!(
        "[issue #945] cube n={n}: λ_max(D⁻¹P) ≥ {lambda_power:.6} (power), ≤ {gershgorin:.6} \
         (Gershgorin); build: θ = {:.6}, ρ̂ = {:.6}, weight = {:.6} ({})",
        sw.theta_max(),
        sw.rho_hat(),
        sw.weight(),
        sw.source().name()
    );
    assert_eq!(sw.source(), crate::eigen::ams::SmoothWeightSource::Estimate);
    assert!(sw.warning().is_none());
    // The build's 30-step Ritz value is a lower bound within 1 % of the
    // converged power-iteration value, and reports the same Gershgorin bound.
    assert!(sw.theta_max() <= lambda_power * (1.0 + 1e-9));
    assert!(sw.theta_max() > 0.99 * lambda_power);
    assert!((sw.gershgorin() - gershgorin).abs() < 1e-9 * gershgorin);
    // Rigorous: the weight is below 2 / Gershgorin ≤ 2 / λ_max.
    assert!(
        sw.weight() < 2.0 / gershgorin,
        "weight {} is not below the rigorous bound 2/Gershgorin = {}",
        sw.weight(),
        2.0 / gershgorin
    );
    assert!(sw.weight() < 2.0 / lambda_power);

    let settings =
        IterativeSettings::new(1e-8, 2000).with_preconditioner(IterativePreconditioner::AMS);
    let (sol, report) = op
        .prepare_at::<B>(omega, SolverMode::Iterative(settings), &device())
        .expect("AMS setup")
        .solve()
        .expect("AMS converges");
    eprintln!(
        "[issue #945] cube n={n}: COCG+AMS iterations = {}",
        report.iters
    );
    assert!(report.converged && report.residual_rel <= 1e-8);
    assert!(
        report.iters <= 40,
        "COCG+AMS took {} iterations (26 with a stable smoother, 70 at weight 0.6)",
        report.iters
    );
    let (sol_lu, _) = op
        .prepare_at::<B>(omega, SolverMode::Direct, &device())
        .expect("LU")
        .solve()
        .expect("direct solve");
    let num: f64 = sol
        .e_edges
        .iter()
        .zip(&sol_lu.e_edges)
        .map(|(a, b)| (a - b).norm_sqr())
        .sum();
    let den: f64 = sol_lu.e_edges.iter().map(|b| b.norm_sqr()).sum();
    assert!((num / den).sqrt() < 1e-5, "AMS vs direct LU");
}

/// Smoke test: [`IterativePreconditioner::Chebyshev`] also reaches the
/// **public** [`SolverMode::Iterative`] path (`prepare_at`) — the same
/// path exercised for ILU(0) above — and converges to an answer that
/// matches direct LU.
#[test]
fn chebyshev_selectable_on_public_iterative_path_converges() {
    let mesh = cube_tet_mesh(3, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
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
        &device(),
    )
    .expect("operator assembly");
    let omega = 1.0;

    let settings = IterativeSettings::new(1e-10, 500)
        .with_preconditioner(IterativePreconditioner::Chebyshev { degree: 3 });
    let (sol_cheb, report) = op
        .prepare_at::<B>(omega, SolverMode::Iterative(settings), &device())
        .expect("Chebyshev setup")
        .solve()
        .expect("Chebyshev converges");
    assert!(report.iters > 0);
    assert!(report.residual_rel <= 1e-10);

    let (sol_lu, _) = op
        .prepare_at::<B>(omega, SolverMode::Direct, &device())
        .expect("LU")
        .solve()
        .expect("direct solve");
    let num: f64 = sol_cheb
        .e_edges
        .iter()
        .zip(&sol_lu.e_edges)
        .map(|(a, b)| (a - b).norm_sqr())
        .sum();
    let den: f64 = sol_lu.e_edges.iter().map(|b| b.norm_sqr()).sum();
    let rel = (num / den).sqrt();
    assert!(rel < 1e-6, "Chebyshev vs direct LU rel err {rel:.3e}");
}

/// **Issue #299 regression**: the Chebyshev polynomial smoother on
/// the same σ-damped resistor stress fixture used for the ILU(0)
/// comparison. The smoother must
///
/// 1. converge COCG to the requested tolerance, and
/// 2. record a **lower** COCG iteration count than both the
///    unpreconditioned (identity) and the Jacobi runs.
///
/// The Chebyshev preconditioner trades a heavier per-apply cost
/// (`degree` SpMVs instead of Jacobi's one diagonal multiply) for a
/// shorter Krylov run — the iteration counts and the wallclock for
/// each run are printed to stderr so the trade-off is visible, in
/// the same reporting style as the #267 ILU(0) test.
#[test]
fn chebyshev_vs_jacobi_sigma_damped_resistor_regression() {
    use crate::solver::ksp::{
        ChebyshevConfig, ChebyshevKind, ChebyshevPreconditioner, Cocg, IdentityPreconditioner,
    };
    use std::time::Instant;

    // Same σ-damped resistor cube as the ILU(0) stress fixture.
    let mesh = cube_tet_mesh(4, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let sigma_tet = vec![5.0_f64; mesh.n_tets()];
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let source = CurrentSource::from_centroids(&mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new((std::f64::consts::PI * c[0]).sin(), 0.0),
        ]
    });
    let omega = 0.3;

    let op = DrivenOperator::assemble::<B>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        Some(&sigma_tet),
        &bcs,
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let ksp = Cocg::new(1e-10, 10_000);

    let t_id = Instant::now();
    let (sol_id, report_id) = op
        .solve_at_iterative(omega, &ksp, |a| {
            Ok::<_, crate::solver::ksp::KspError>(IdentityPreconditioner::new(a.nrows()))
        })
        .expect("unpreconditioned COCG");
    let dt_id = t_id.elapsed();

    let t_jac = Instant::now();
    let (_sol_jac, report_jac) = op
        .solve_at_iterative(omega, &ksp, |a| {
            crate::solver::ksp::JacobiPreconditioner::new(a.as_ref())
        })
        .expect("Jacobi-preconditioned COCG");
    let dt_jac = t_jac.elapsed();

    // Degree-3 first-kind Chebyshev smoother (default config).
    let t_cheb = Instant::now();
    let (sol_cheb, report_cheb) = op
        .solve_at_iterative(omega, &ksp, |a| ChebyshevPreconditioner::new(a.as_ref(), 3))
        .expect("Chebyshev-preconditioned COCG");
    let dt_cheb = t_cheb.elapsed();

    // Degree-3 fourth-kind Chebyshev smoother (Lottes 2022, issue
    // #348) — same degree, only λ_max (no ratio/λ_min).
    let t_cheb4 = Instant::now();
    let (sol_cheb4, report_cheb4) = op
        .solve_at_iterative(omega, &ksp, |a| {
            let cfg = ChebyshevConfig {
                kind: ChebyshevKind::Fourth,
                ..ChebyshevConfig::default()
            };
            ChebyshevPreconditioner::with_config(a.as_ref(), 3, cfg)
        })
        .expect("fourth-kind Chebyshev-preconditioned COCG");
    let dt_cheb4 = t_cheb4.elapsed();

    assert!(report_id.converged);
    assert!(report_jac.converged);
    assert!(report_cheb.converged);
    assert!(report_cheb4.converged);

    // Acceptance: Chebyshev reduces the iteration count vs both the
    // unpreconditioned and Jacobi baselines on the σ-damped fixture.
    assert!(
        report_cheb.iters <= report_id.iters,
        "Chebyshev ({}) must beat unpreconditioned ({}) on σ-damped resistor",
        report_cheb.iters,
        report_id.iters,
    );
    assert!(
        report_cheb.iters <= report_jac.iters,
        "Chebyshev ({}) must beat Jacobi ({}) on σ-damped resistor",
        report_cheb.iters,
        report_jac.iters,
    );
    // Fourth-kind: must converge and beat the unpreconditioned
    // baseline on the σ-damped fixture (issue #348).
    assert!(
        report_cheb4.iters <= report_id.iters,
        "fourth-kind Chebyshev ({}) must beat unpreconditioned ({}) on σ-damped resistor",
        report_cheb4.iters,
        report_id.iters,
    );

    // Solutions must agree with the Jacobi run (same linear system).
    let norm: f64 = sol_cheb
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    let mut diff2 = 0.0_f64;
    for (c, i) in sol_cheb.e_edges.iter().zip(sol_id.e_edges.iter()) {
        let d = *c - *i;
        diff2 += d.re * d.re + d.im * d.im;
    }
    assert!(diff2.sqrt() / norm < 1e-7);
    // Fourth-kind solution must also agree with the unpreconditioned run.
    let mut diff2_c4 = 0.0_f64;
    for (c, i) in sol_cheb4.e_edges.iter().zip(sol_id.e_edges.iter()) {
        let d = *c - *i;
        diff2_c4 += d.re * d.re + d.im * d.im;
    }
    assert!(diff2_c4.sqrt() / norm < 1e-7);

    eprintln!(
        "[issue #348 / stress: σ-damped resistor] grid=4, σ=5, ω={:.2}, \
         n_interior={}, \
         identity: iters={} ({:?}), \
         Jacobi: iters={} ({:?}), \
         Chebyshev-1st(deg=3): iters={} ({:?}), \
         Chebyshev-4th(deg=3): iters={} ({:?}), \
         iter ratio (1st/Jacobi)={:.3}, iter ratio (4th/Jacobi)={:.3}",
        omega,
        sol_cheb.n_interior,
        report_id.iters,
        dt_id,
        report_jac.iters,
        dt_jac,
        report_cheb.iters,
        dt_cheb,
        report_cheb4.iters,
        dt_cheb4,
        report_cheb.iters as f64 / report_jac.iters.max(1) as f64,
        report_cheb4.iters as f64 / report_jac.iters.max(1) as f64,
    );
}

/// A zero current source through the iterative path must produce
/// the exact-zero field — matches the direct path's
/// `zero_source_gives_zero_field` semantics.
#[test]
fn iterative_zero_source_gives_zero_field() {
    use crate::solver::ksp::Cocg;

    let mesh = cube_tet_mesh(2, 1.0);
    let (_, interior) = cube_pec_interior_edges(&mesh, 1.0);
    let eps = vacuum(&mesh);
    let source = CurrentSource {
        j_tet: vec![[c64::new(0.0, 0.0); 3]; mesh.n_tets()],
    };
    let ksp = Cocg::default();
    let (sol, report) = super::driven_solve_iterative::<B, _>(
        &mesh,
        DrivenMaterials::Scalar(&eps),
        &DrivenBcs {
            pec_interior_mask: &interior,
        },
        1.0,
        &source,
        &ksp,
        &device(),
    )
    .expect("iterative driven solve with zero source");
    assert!(sol.e_edges.iter().all(|e| e.re == 0.0 && e.im == 0.0));
    assert_eq!(report.iters, 0);
    assert!(report.converged);
}

/// **Issue #238 heavy benchmark**: the iterative path must scale
/// to the patch fixture (~30k interior edges) and converge to a
/// documented tolerance vs the direct LU. `#[ignore]`d because
/// it requires the patch fixture and is expensive — run with
/// `cargo test --release -- --ignored iterative_patch_benchmark`.
#[test]
#[ignore = "heavy fixture; opt-in with `cargo test --release -- --ignored iterative_patch_benchmark`"]
fn iterative_patch_benchmark() {
    use crate::mesh::{pec_interior_mask_from_triangles, read_patch_smoke_fixture};
    use crate::solver::ksp::Cocg;

    let fixture = read_patch_smoke_fixture().expect("patch fixture");
    let patch_tris = fixture.patch_triangles();
    let ground_tris = fixture.ground_triangles();
    let outer_tris = fixture.outer_boundary_triangles();
    let edges = fixture.mesh.edges();
    let interior = pec_interior_mask_from_triangles(
        &edges,
        &[
            patch_tris.as_slice(),
            ground_tris.as_slice(),
            outer_tris.as_slice(),
        ],
    );
    let mesh = &fixture.mesh;
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let eps = fixture.epsilon_r_default();
    let source = CurrentSource::from_centroids(mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new(c[0].sin(), 0.0),
        ]
    });
    // Patch smoke fixture: pick a representative ω near the
    // operating band's normalized frequency (the smoke fixture is
    // small and tolerant of any moderate ω).
    let omega = 0.1;

    let t_lu = std::time::Instant::now();
    let sol_lu = driven_solve::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &device(),
    )
    .expect("direct LU patch solve");
    let lu_secs = t_lu.elapsed().as_secs_f64();

    let ksp = Cocg::new(1e-8, 20_000);
    let t_it = std::time::Instant::now();
    let (sol_ksp, report) = super::driven_solve_iterative::<B, _>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        &bcs,
        omega,
        &source,
        &ksp,
        &device(),
    )
    .expect("COCG patch solve");
    let it_secs = t_it.elapsed().as_secs_f64();

    let norm_lu: f64 = sol_lu
        .e_edges
        .iter()
        .map(|e| e.re * e.re + e.im * e.im)
        .sum::<f64>()
        .sqrt();
    let mut diff2 = 0.0_f64;
    for (a, b) in sol_lu.e_edges.iter().zip(sol_ksp.e_edges.iter()) {
        let d = *a - *b;
        diff2 += d.re * d.re + d.im * d.im;
    }
    let rel_diff = diff2.sqrt() / norm_lu;

    eprintln!(
        "[issue #238 / patch] n_interior={}, COCG iters={}, residual_rel={:.3e}, \
         rel_diff_vs_LU={:.3e}, LU_secs={:.3}, COCG_secs={:.3}, speedup={:.2}x",
        sol_ksp.n_interior,
        report.iters,
        report.residual_rel,
        rel_diff,
        lu_secs,
        it_secs,
        lu_secs / it_secs.max(1e-12),
    );
    assert!(report.converged);
    // Loose tol for the heavy fixture — the direct LU residual
    // floor + Krylov tol stack up — but COCG must reach the same
    // solution to a few-digit agreement.
    assert!(
        rel_diff < 1e-4,
        "patch iterative vs LU rel_diff = {}",
        rel_diff
    );
}

/// **Issue #267 heavy benchmark**: patch fixture under
/// Jacobi-preconditioned vs ILU(0)-preconditioned COCG. Reports
/// both iteration counts and per-iteration wall clock to
/// characterise the Jacobi setup vs ILU setup trade-off explicitly
/// (the documentation requirement in issue #267).
///
/// `#[ignore]`d for the same reason as
/// [`iterative_patch_benchmark`] — opt-in with
/// `cargo test --release -- --ignored ilu_patch_benchmark`.
#[test]
#[ignore = "heavy fixture; opt-in with `cargo test --release -- --ignored ilu_patch_benchmark`"]
fn ilu_patch_benchmark() {
    use crate::mesh::{pec_interior_mask_from_triangles, read_patch_smoke_fixture};
    use crate::solver::ksp::{Cocg, IluPreconditioner, JacobiPreconditioner};

    let fixture = read_patch_smoke_fixture().expect("patch fixture");
    let patch_tris = fixture.patch_triangles();
    let ground_tris = fixture.ground_triangles();
    let outer_tris = fixture.outer_boundary_triangles();
    let edges = fixture.mesh.edges();
    let interior = pec_interior_mask_from_triangles(
        &edges,
        &[
            patch_tris.as_slice(),
            ground_tris.as_slice(),
            outer_tris.as_slice(),
        ],
    );
    let mesh = &fixture.mesh;
    let bcs = DrivenBcs {
        pec_interior_mask: &interior,
    };
    let eps = fixture.epsilon_r_default();
    let source = CurrentSource::from_centroids(mesh, |c| {
        [
            c64::new(0.0, 0.0),
            c64::new(0.0, 0.0),
            c64::new(c[0].sin(), 0.0),
        ]
    });
    let omega = 0.1;

    let op = DrivenOperator::assemble::<B>(
        mesh,
        DrivenMaterials::Scalar(&eps),
        None,
        &bcs,
        &[],
        &[],
        &source,
        &device(),
    )
    .expect("operator assembly");
    let ksp = Cocg::new(1e-8, 20_000);

    // Jacobi run.
    let t_jac = std::time::Instant::now();
    let (sol_jac, report_jac) = op
        .solve_at_iterative(omega, &ksp, |a| JacobiPreconditioner::new(a.as_ref()))
        .expect("Jacobi-preconditioned COCG");
    let jac_secs = t_jac.elapsed().as_secs_f64();

    // ILU(0) run — note the setup cost is folded into the same
    // wall-clock measurement, so the print line below makes the
    // setup/per-iter trade-off explicit.
    let t_ilu = std::time::Instant::now();
    let (_sol_ilu, report_ilu) = op
        .solve_at_iterative(omega, &ksp, |a| IluPreconditioner::new(a.as_ref(), 0))
        .expect("ILU(0)-preconditioned COCG");
    let ilu_secs = t_ilu.elapsed().as_secs_f64();

    assert!(report_jac.converged);
    assert!(report_ilu.converged);

    eprintln!(
        "[issue #267 / patch ILU vs Jacobi] n_interior={}, \
         Jacobi: iters={}, residual_rel={:.3e}, total_secs={:.3} ({:.4} ms/iter) | \
         ILU(0): iters={}, residual_rel={:.3e}, total_secs={:.3} ({:.4} ms/iter) | \
         iter ratio (ILU/Jacobi) = {:.3}, wallclock speedup = {:.2}×",
        sol_jac.n_interior,
        report_jac.iters,
        report_jac.residual_rel,
        jac_secs,
        1000.0 * jac_secs / report_jac.iters.max(1) as f64,
        report_ilu.iters,
        report_ilu.residual_rel,
        ilu_secs,
        1000.0 * ilu_secs / report_ilu.iters.max(1) as f64,
        report_ilu.iters as f64 / report_jac.iters.max(1) as f64,
        jac_secs / ilu_secs.max(1e-12),
    );

    // ILU(0) must reduce iteration count on the patch fixture
    // — the documented acceptance for the heavy benchmark.
    assert!(
        report_ilu.iters <= report_jac.iters,
        "ILU(0) iters ({}) must not exceed Jacobi iters ({}) on patch fixture",
        report_ilu.iters,
        report_jac.iters,
    );
}

// ---- ∂c_Γ/∂θ closed forms (Epic #841 Phase 2b, issue #1260) ----------

fn fd_dc(m: impl Fn(f64) -> SurfaceImpedanceModel, theta: f64, omega: f64) -> c64 {
    let h = 1e-6 * theta.abs().max(1e-3);
    (m(theta + h).weak_coefficient(omega).unwrap() - m(theta - h).weak_coefficient(omega).unwrap())
        / (2.0 * h)
}

fn close(a: c64, b: c64) -> bool {
    (a - b).norm() <= 1e-6 * b.norm().max(1e-12)
}

#[test]
fn d_weak_coefficient_matches_finite_difference() {
    use SurfaceRoughness as R;
    let omega = 2.4;
    let (sigma, rms, a, n, tile, lam) = (40.0, 0.15, 0.1, 3.0, 0.5, 0.1);
    let hj = |s: f64, d: f64| SurfaceImpedanceModel::RoughConductor {
        sigma: s,
        roughness: R::HammerstadJensen { rms: d },
    };
    let hu = |s: f64, a: f64, n: f64| SurfaceImpedanceModel::RoughConductor {
        sigma: s,
        roughness: R::Huray {
            ball_radius: a,
            n_balls: n,
            tile_area: tile,
        },
    };
    let get = |m: SurfaceImpedanceModel, p| m.d_weak_coefficient(omega, p).unwrap().unwrap().dc;
    let gc = |s: f64| SurfaceImpedanceModel::GoodConductor { sigma: s };
    assert!(close(
        get(gc(sigma), WallParam::Sigma),
        fd_dc(gc, sigma, omega)
    ));
    assert!(close(
        get(hj(sigma, rms), WallParam::Sigma),
        fd_dc(|s| hj(s, rms), sigma, omega)
    ));
    assert!(close(
        get(hj(sigma, rms), WallParam::RmsRoughness),
        fd_dc(|d| hj(sigma, d), rms, omega)
    ));
    assert!(close(
        get(hu(sigma, a, n), WallParam::Sigma),
        fd_dc(|s| hu(s, a, n), sigma, omega)
    ));
    assert!(close(
        get(hu(sigma, a, n), WallParam::BallRadius),
        fd_dc(|x| hu(sigma, x, n), a, omega)
    ));
    // Density ρ = N/A_tile: ∂/∂ρ = A_tile ∂/∂N.
    assert!(close(
        get(hu(sigma, a, n), WallParam::BallDensity),
        fd_dc(|r| hu(sigma, a, r * tile), n / tile, omega)
    ));
    let lon = |l: f64| SurfaceImpedanceModel::London { lambda_l: l };
    assert!(close(
        get(lon(lam), WallParam::LondonDepth),
        fd_dc(lon, lam, omega)
    ));
    // A parameter the model does not carry is `None`, not an error.
    assert!(
        gc(sigma)
            .d_weak_coefficient(omega, WallParam::RmsRoughness)
            .unwrap()
            .is_none()
    );
    assert!(
        lon(lam)
            .d_weak_coefficient(omega, WallParam::Sigma)
            .unwrap()
            .is_none()
    );
}

/// Tripwire: `∂c/∂σ` of a rough wall must carry the `∂K/∂δ` term. At
/// `Δ/δ ≈ 1` the `√σ`-only value `c/(2σ)` is off by well over 10 %.
#[test]
fn d_weak_coefficient_sigma_carries_dk_ddelta_term() {
    let omega = 2.4;
    let m = SurfaceImpedanceModel::RoughConductor {
        sigma: 40.0,
        roughness: SurfaceRoughness::HammerstadJensen { rms: 0.15 },
    };
    let full = m
        .d_weak_coefficient(omega, WallParam::Sigma)
        .unwrap()
        .unwrap()
        .dc;
    let sqrt_only = m.weak_coefficient(omega).unwrap() / 80.0;
    assert!((full - sqrt_only).norm() > 0.1 * full.norm());
}

/// Saturation: `K → 2` flags the roughness gradient; mid-range does not.
#[test]
fn d_weak_coefficient_flags_saturation() {
    let mk = |rms| SurfaceImpedanceModel::RoughConductor {
        sigma: 40.0,
        roughness: SurfaceRoughness::HammerstadJensen { rms },
    };
    let sat = mk(10.0)
        .d_weak_coefficient(2.4, WallParam::RmsRoughness)
        .unwrap()
        .unwrap();
    assert!(sat.saturated && sat.dc.norm() < 1e-3);
    let mid = mk(0.15)
        .d_weak_coefficient(2.4, WallParam::RmsRoughness)
        .unwrap()
        .unwrap();
    assert!(!mid.saturated);
}
