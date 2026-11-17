use super::*;
use faer::sparse::{SparseColMat, Triplet};

/// Issue #916: [`CheckedEigenpairs::localized_withheld`] lists exactly
/// `localized_rejected(σ)`, in order, and each entry's vector is the Ritz
/// vector of that pair (its recomputed residual matches).
fn assert_localized_withheld_matches(
    checked: &CheckedEigenpairs,
    k: SparseColMatRef<'_, usize, f64>,
    m: SparseColMatRef<'_, usize, f64>,
    sigma: f64,
) {
    let listed = checked.localized_rejected(sigma);
    assert_eq!(listed.len(), checked.localized_withheld.len());
    let n = k.nrows();
    let mut kx = vec![0.0; n];
    let mut mx = vec![0.0; n];
    for (&(lambda, rho), w) in listed.iter().zip(&checked.localized_withheld) {
        assert_eq!(w.pair.lambda, lambda);
        assert_eq!(w.residual, rho);
        assert_eq!(w.pair.vector.len(), n);
        let again = pair_relative_residual(k, m, lambda, &w.pair.vector, sigma, &mut kx, &mut mx);
        assert!(
            (again - rho).abs() <= 1e-6 * rho.max(1e-12),
            "vector residual {again:.3e} != recorded {rho:.3e}"
        );
    }
}

/// Build a tiny SPD diagonal pencil with known eigenvalues.
fn diagonal_pencil(
    diag_k: &[f64],
    diag_m: &[f64],
) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let n = diag_k.len();
    let tk: Vec<Triplet<usize, usize, f64>> =
        (0..n).map(|i| Triplet::new(i, i, diag_k[i])).collect();
    let tm: Vec<Triplet<usize, usize, f64>> =
        (0..n).map(|i| Triplet::new(i, i, diag_m[i])).collect();
    let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
    let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
    (k, m)
}

#[test]
fn lanczos_diagonal_pencil_smallest_three() {
    // λ_i = k_i / m_i = {1, 2, 3, 4, 5}.
    let (k, m) = diagonal_pencil(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0; 5]);
    let solver = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 50,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let lambdas = solver
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 3)
        .unwrap();
    assert_eq!(lambdas.len(), 3);
    for (got, want) in lambdas.iter().zip([1.0, 2.0, 3.0].iter()) {
        assert!((got - want).abs() < 1e-8, "lanczos λ={got}, want {want}");
    }
}

/// Eigenpair path on a diagonal SPD pencil: each Ritz vector should
/// be (a sign of) the canonical basis vector corresponding to its
/// eigenvalue, M-orthonormalized.
#[test]
fn lanczos_diagonal_pencil_eigenpairs_recover_basis() {
    // λ_i = k_i / m_i = {1, 2, 3, 4, 5} with M = I.
    let (k, m) = diagonal_pencil(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1.0; 5]);
    let solver = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 50,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let pairs = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), 3)
        .unwrap();
    assert_eq!(pairs.len(), 3);
    for (i, pair) in pairs.iter().enumerate() {
        let want_lambda = (i + 1) as f64;
        assert!(
            (pair.lambda - want_lambda).abs() < 1e-8,
            "λ[{i}] = {}, want {want_lambda}",
            pair.lambda
        );
        // Eigenvector should be ±e_i with unit M-norm (M = I).
        let norm2: f64 = pair.vector.iter().map(|x| x * x).sum();
        assert!(
            (norm2 - 1.0).abs() < 1e-9,
            "eigenvector[{i}] not M-orthonormal: ‖v‖² = {norm2}"
        );
        // Largest entry is at position i, magnitude ≈ 1.
        let (max_pos, max_val) = pair
            .vector
            .iter()
            .enumerate()
            .map(|(p, x)| (p, x.abs()))
            .fold(
                (0usize, 0.0_f64),
                |acc, x| if x.1 > acc.1 { x } else { acc },
            );
        assert_eq!(max_pos, i, "eigenvector[{i}] localized at wrong index");
        assert!(
            (max_val - 1.0).abs() < 1e-6,
            "eigenvector[{i}] not localized: max = {max_val}"
        );
    }
}

/// Build a larger, non-diagonal SPD tridiagonal pencil so the faer
/// sparse LU has enough fill for the multi-threaded factorization path
/// to be meaningfully exercised (a diagonal pencil factors trivially).
///
/// `K` is the 1-D Laplacian stencil `tridiag(-1, 2, -1)` (SPD after the
/// implicit Dirichlet ends) and `M` is the identity, so the eigenvalues
/// are the known `2 - 2 cos(π i / (n+1))`, a smoothly separated spectrum.
fn laplacian_pencil(n: usize) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let mut tk: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(3 * n);
    let mut tm: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(n);
    for i in 0..n {
        tk.push(Triplet::new(i, i, 2.0));
        if i + 1 < n {
            tk.push(Triplet::new(i, i + 1, -1.0));
            tk.push(Triplet::new(i + 1, i, -1.0));
        }
        tm.push(Triplet::new(i, i, 1.0));
    }
    let k = SparseColMat::try_new_from_triplets(n, n, &tk).unwrap();
    let m = SparseColMat::try_new_from_triplets(n, n, &tm).unwrap();
    (k, m)
}

/// Exact eigenvalues of [`laplacian_pencil`]: `2 − 2 cos(π i / (n+1))`.
fn laplacian_eigenvalues(n: usize) -> Vec<f64> {
    (1..=n)
        .map(|i| 2.0 - 2.0 * (std::f64::consts::PI * i as f64 / (n as f64 + 1.0)).cos())
        .collect()
}

/// Distance from `lambda` to the nearest exact eigenvalue.
fn nearest_gap(lambda: f64, exact: &[f64]) -> f64 {
    exact
        .iter()
        .map(|e| (lambda - e).abs())
        .fold(f64::INFINITY, f64::min)
}

/// When the first pass already converges every requested pair, the
/// checked solve returns exactly [`SparseShiftInvertLanczos::smallest_eigenpairs`],
/// bit for bit, with no extension (issue #798).
#[test]
fn checked_is_bit_identical_when_first_pass_converges() {
    let (k, m) = laplacian_pencil(200);
    let solver = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let plain = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), 4)
        .unwrap();
    let checked = solver
        .smallest_eigenpairs_checked(
            k.as_ref(),
            m.as_ref(),
            4,
            ConvergenceCheck {
                residual_tol: 1e-8,
                max_iters_cap: 200,
                window: None,
            },
        )
        .unwrap();
    assert!(!checked.extended);
    assert!(checked.rejected.is_empty());
    assert_eq!(checked.shortfall(), 0);
    assert_eq!(checked.pairs.len(), plain.len());
    for (a, b) in checked.pairs.iter().zip(&plain) {
        assert_eq!(a.lambda.to_bits(), b.lambda.to_bits());
        assert!(
            a.vector
                .iter()
                .zip(&b.vector)
                .all(|(x, y)| x.to_bits() == y.to_bits())
        );
    }
    assert!(checked.residuals.iter().all(|&r| r <= 1e-8));
}

/// A short Krylov budget at an interior shift leaks unconverged Ritz
/// pairs from the plain solve. The checked solve withholds them, and
/// with room to extend it converges the window instead (issue #798).
#[test]
fn checked_withholds_then_extends_unconverged_pairs() {
    let n = 400;
    let (k, m) = laplacian_pencil(n);
    let exact = laplacian_eigenvalues(n);
    let solver = SparseShiftInvertLanczos {
        sigma: 1.0,
        max_iters: 8,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let n_modes = 6;
    // The plain solve returns some pair that is no eigenvalue at all.
    let plain = solver
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
        .unwrap();
    let worst = plain
        .iter()
        .map(|p| nearest_gap(p.lambda, &exact))
        .fold(0.0_f64, f64::max);
    assert!(
        worst > 1e-6,
        "short budget should leak a tail pair (gap {worst:.3e})"
    );

    let tol = 1e-9;
    let check = |cap: usize, window: Option<(f64, f64)>| {
        solver
            .smallest_eigenpairs_checked(
                k.as_ref(),
                m.as_ref(),
                n_modes,
                ConvergenceCheck {
                    residual_tol: tol,
                    max_iters_cap: cap,
                    window,
                },
            )
            .unwrap()
    };
    // No room to extend: only converged pairs come back, the rest is an
    // explicit shortfall.
    let capped = check(0, None);
    assert!(!capped.extended);
    assert!(!capped.rejected.is_empty());
    assert_eq!(capped.pairs.len() + capped.rejected.len(), n_modes);
    assert!(capped.shortfall() > 0);
    assert!(!capped.localized_withheld.is_empty());
    assert_localized_withheld_matches(&capped, k.as_ref(), m.as_ref(), 1.0);
    // A window that excludes every unconverged pair never extends.
    let elsewhere = check(n, Some((-2.0, -1.0)));
    assert!(!elsewhere.extended);
    assert_eq!(elsewhere.pairs.len(), capped.pairs.len());
    // Room to extend: every requested pair the first pass located is
    // confirmed, and each returned pair is an exact eigenvalue.
    let extended = check(n, None);
    eprintln!(
        "short budget: plain worst gap {worst:.3e}; capped {} kept / {} rejected; \
             extended {} kept / {} rejected after {} steps",
        capped.pairs.len(),
        capped.rejected.len(),
        extended.pairs.len(),
        extended.rejected.len(),
        extended.lanczos_steps
    );
    assert!(extended.extended);
    assert!(extended.lanczos_steps > 8);
    assert_localized_withheld_matches(&extended, k.as_ref(), m.as_ref(), 1.0);
    assert!(extended.pairs.len() >= capped.pairs.len());
    for (p, &r) in extended.pairs.iter().zip(&extended.residuals) {
        assert!(r <= tol);
        assert!(
            nearest_gap(p.lambda, &exact) < 1e-9,
            "returned λ = {} is not an eigenvalue",
            p.lambda
        );
    }
    let mut kx = vec![0.0; n];
    let mut mx = vec![0.0; n];
    for (p, &r) in extended.pairs.iter().zip(&extended.residuals) {
        let again = pair_relative_residual(
            k.as_ref(),
            m.as_ref(),
            p.lambda,
            &p.vector,
            1.0,
            &mut kx,
            &mut mx,
        );
        assert_eq!(again.to_bits(), r.to_bits());
    }
}

/// [`laplacian_pencil`] with `K` scaled by `scale`, so `λ → scale · λ`.
fn scaled_laplacian_pencil(
    n: usize,
    scale: f64,
) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    let (k, m) = laplacian_pencil(n);
    let cp = k.col_ptr();
    let trips: Vec<Triplet<usize, usize, f64>> = (0..n)
        .flat_map(|j| (cp[j]..cp[j + 1]).map(move |p| (p, j)))
        .map(|(p, j)| Triplet::new(k.row_idx()[p], j, scale * k.val()[p]))
        .collect();
    (
        SparseColMat::<usize, f64>::try_new_from_triplets(n, n, &trips).unwrap(),
        m,
    )
}

/// Issue #828: on an SI-scaled pencil (`λ ~ 10¹³`, so `|μ| ~ 10⁻¹³`) the
/// old β-bound probe `β ≤ tol · max(μ_max, 1)` was the absolute `β ≤ tol`
/// and fired after `n_modes` steps, and the plain solve returned
/// unconverged pairs (worst gap `≈ 4e-3·scale` here). With the relative
/// probe the plain solve converges, and the checked solve has nothing
/// left to extend, so it is bit-identical to it.
#[test]
fn plain_solve_converges_on_si_scaled_pencils() {
    let n = 300;
    let scale = 1e13;
    let (k_si, m) = scaled_laplacian_pencil(n, scale);
    let exact: Vec<f64> = laplacian_eigenvalues(n).iter().map(|e| e * scale).collect();
    let n_modes = 4;
    let solver = SparseShiftInvertLanczos {
        sigma: 0.5 * scale,
        max_iters: 40,
        tol: 1e-9,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let plain = solver
        .smallest_eigenpairs(k_si.as_ref(), m.as_ref(), n_modes)
        .unwrap();
    let worst = plain
        .iter()
        .map(|p| nearest_gap(p.lambda, &exact))
        .fold(0.0_f64, f64::max);
    eprintln!("SI-scaled plain solve: worst gap {worst:.3e} (scale {scale:e})");
    assert_eq!(plain.len(), n_modes);
    assert!(worst < 1e-9 * scale, "plain worst gap {worst:.3e}");
    let values = solver
        .smallest_eigenvalues(k_si.as_ref(), m.as_ref(), n_modes)
        .unwrap();
    for (v, p) in values.iter().zip(&plain) {
        assert!((v - p.lambda).abs() < 1e-9 * scale, "{v} vs {}", p.lambda);
    }
    let checked = solver
        .smallest_eigenpairs_checked(
            k_si.as_ref(),
            m.as_ref(),
            n_modes,
            ConvergenceCheck {
                residual_tol: 1e-9,
                max_iters_cap: n,
                window: None,
            },
        )
        .unwrap();
    assert!(!checked.extended);
    assert_eq!(checked.shortfall(), 0);
    for (a, b) in checked.pairs.iter().zip(&plain) {
        assert_eq!(a.lambda.to_bits(), b.lambda.to_bits());
    }
}

/// A probe tolerance `tol ≥ 1` makes the β-bound probe fire at its first
/// test (`β ≤ ‖T_k‖ ≤ μ_max` always), while the window is unconverged.
/// The checked solve resumes the recurrence exactly where the probe
/// stopped it and converges (issue #798; before issue #828 the absolute
/// probe did the same on any SI-scaled pencil).
#[test]
fn checked_resumes_past_the_beta_bound() {
    let n = 300;
    let scale = 1e13;
    let (k_si, m) = scaled_laplacian_pencil(n, scale);
    let exact: Vec<f64> = laplacian_eigenvalues(n).iter().map(|e| e * scale).collect();
    let n_modes = 4;
    let solver = SparseShiftInvertLanczos {
        sigma: 0.5 * scale,
        max_iters: 40,
        tol: 1.0,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    // The plain solve stops on the probe and leaks unconverged pairs.
    let plain = solver
        .smallest_eigenpairs(k_si.as_ref(), m.as_ref(), n_modes)
        .unwrap();
    let worst = plain
        .iter()
        .map(|p| nearest_gap(p.lambda, &exact))
        .fold(0.0_f64, f64::max);
    assert!(worst > 1e-6 * scale, "plain worst gap {worst:.3e}");
    let checked = solver
        .smallest_eigenpairs_checked(
            k_si.as_ref(),
            m.as_ref(),
            n_modes,
            ConvergenceCheck {
                residual_tol: 1e-9,
                max_iters_cap: n,
                window: None,
            },
        )
        .unwrap();
    eprintln!(
        "loose probe: plain worst gap {worst:.3e}; {} kept, {} rejected, {} steps, \
             extended = {}",
        checked.pairs.len(),
        checked.rejected.len(),
        checked.lanczos_steps,
        checked.extended
    );
    assert!(checked.extended && checked.lanczos_steps > 2 * n_modes);
    assert_eq!(checked.shortfall(), 0);
    assert_localized_withheld_matches(&checked, k_si.as_ref(), m.as_ref(), solver.sigma);
    for p in &checked.pairs {
        assert!(nearest_gap(p.lambda, &exact) < 1e-9 * scale);
    }
}

/// Lossless unit-cube-shaped PEC cavity pencil of side `side` (`n = 3`).
fn cube_cavity_pencil(side: f64) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;
    let mesh = crate::mesh::cube_tet_mesh(3, side);
    let (_, mask) = crate::assembly::nedelec::cube_pec_interior_edges(&mesh, side);
    let eps = vec![1.0; mesh.n_tets()];
    let dev = <TestBackend as BackendTypes>::Device::default();
    crate::eigen::pec_cavity::assemble_lossless_pencil::<TestBackend>(&mesh, &eps, &mask, &dev)
        .unwrap()
}

/// Issue #828 regression: the same cavity meshed in three length units
/// (side `1`, `1e-6` and `1e3`) gives the same `λ · side²` and converged
/// pairs from the plain solve, on both the eigenpair and the
/// eigenvalue-only path.
///
/// `λ` scales as `side⁻²` and `μ = 1/(λ − σ)` as `side²`. With the old
/// `tol · max(μ_max, 1)` probe and absolute `β < 1e-14` breakdown, the
/// `1e-6` cavity (`|μ| ~ 1e-13`) stopped after one or two steps and
/// returned pairs with relative residual `O(1)`, and the unit cavity
/// (`μ ≈ 0.2`) ran on the absolute probe.
#[test]
fn plain_solve_is_mesh_unit_invariant() {
    let two_pi2 = 2.0 * core::f64::consts::PI.powi(2);
    let n_modes = 3;
    let solve = |side: f64| {
        let (k, m) = cube_cavity_pencil(side);
        let solver = SparseShiftInvertLanczos {
            sigma: 0.7 * two_pi2 / (side * side),
            max_iters: 64,
            tol: 1e-9,
            inner: InnerSolver::Direct,
            precond: InnerPreconditioner::Jacobi,
        };
        let pairs = solver
            .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        let values = solver
            .smallest_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
            .unwrap();
        let n = k.nrows();
        let (mut kx, mut mx) = (vec![0.0; n], vec![0.0; n]);
        let residuals: Vec<f64> = pairs
            .iter()
            .map(|p| {
                pair_relative_residual(
                    k.as_ref(),
                    m.as_ref(),
                    p.lambda,
                    &p.vector,
                    solver.sigma,
                    &mut kx,
                    &mut mx,
                )
            })
            .collect();
        let unit = |l: f64| l * side * side;
        (
            pairs.iter().map(|p| unit(p.lambda)).collect::<Vec<_>>(),
            values.into_iter().map(unit).collect::<Vec<_>>(),
            residuals,
        )
    };
    let (ref_pairs, ref_values, ref_res) = solve(1.0);
    for side in [1.0, 1e-6, 1e3] {
        let (pairs, values, res) = solve(side);
        eprintln!("side {side:e}: λ·side² = {pairs:?}, residuals {res:?}");
        assert_eq!(pairs.len(), n_modes, "side {side:e}");
        assert!(
            res.iter().all(|&r| r < 1e-9),
            "side {side:e}: residuals {res:?}"
        );
        for i in 0..n_modes {
            let rel = |a: f64, b: f64| (a - b).abs() / b.abs();
            assert!(
                rel(pairs[i], ref_pairs[i]) < 1e-10,
                "side {side:e}: pair {i} λ·side² = {} vs {}",
                pairs[i],
                ref_pairs[i]
            );
            assert!(
                rel(values[i], ref_values[i]) < 1e-10,
                "side {side:e}: value {i} λ·side² = {} vs {}",
                values[i],
                ref_values[i]
            );
        }
    }
    assert!(ref_res.iter().all(|&r| r < 1e-9));
}

/// Issue #946: the two direct backends come with a faer sequential scope
/// for their Lanczos loop, and the matrix-free backend comes without one.
/// What a live scope does to faer's global parallelism is asserted in
/// `tests/faer_global_parallelism.rs`.
#[test]
fn direct_backends_come_with_a_sequential_scope_and_matrix_free_does_not() {
    let (k, m) = laplacian_pencil(40);
    let scoped = |inner: InnerSolver| {
        let solver = SparseShiftInvertLanczos {
            inner,
            ..SparseShiftInvertLanczos::default()
        };
        let (_backend, scope) = solver
            .prepare_inner(k.as_ref(), m.as_ref(), 1, None)
            .expect("inner backend");
        scope.is_some()
    };
    assert!(scoped(InnerSolver::Direct));
    assert!(scoped(InnerSolver::DirectCustomOrder));
    assert!(!scoped(InnerSolver::MatrixFree));
}

/// ISSUE #543 ACCEPTANCE BAR: the custom fill-reducing ordering must change
/// LU fill/memory, **not the answers**. The spectrum from the
/// [`InnerSolver::DirectCustomOrder`] path (AMD ordering) must match
/// the COLAMD [`InnerSolver::Direct`] baseline within tolerance.
#[test]
fn custom_ordering_spectrum_matches_colamd() {
    // Serialize with the other tests that transiently mutate faer's global
    // parallelism during factorization.
    let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let n = 60;
    let (k, m) = laplacian_pencil(n);

    let base = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 80,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let custom = SparseShiftInvertLanczos {
        inner: InnerSolver::DirectCustomOrder,
        ..base
    };

    let n_modes = 5;
    let colamd = base
        .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), n_modes, 1)
        .unwrap();
    let amd = custom
        .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), n_modes, 1)
        .unwrap();

    assert_eq!(colamd.len(), n_modes);
    assert_eq!(amd.len(), n_modes);
    for i in 0..n_modes {
        assert!(
            (colamd[i] - amd[i]).abs() <= 1e-9 * (1.0 + colamd[i].abs()),
            "AMD eigenvalue {i} diverged: colamd={} amd={}",
            colamd[i],
            amd[i]
        );
    }
}

/// Eigen*pairs* (values and M-orthonormal vectors) from the custom-ordering
/// direct path must also match the COLAMD baseline (issue #543): the
/// ordering permutes the factorization internally but the recovered Ritz
/// vectors live in the original DOF ordering, so they must agree up to sign.
#[test]
fn custom_ordering_eigenpairs_match_colamd() {
    let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let n = 48;
    let (k, m) = laplacian_pencil(n);

    let base = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 80,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let custom = SparseShiftInvertLanczos {
        inner: InnerSolver::DirectCustomOrder,
        ..base
    };

    let n_modes = 4;
    let base_pairs = base
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
        .unwrap();
    let custom_pairs = custom
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
        .unwrap();

    assert_eq!(base_pairs.len(), n_modes);
    assert_eq!(custom_pairs.len(), n_modes);
    for i in 0..n_modes {
        assert!(
            (base_pairs[i].lambda - custom_pairs[i].lambda).abs()
                <= 1e-9 * (1.0 + base_pairs[i].lambda.abs()),
            "eigenvalue {i} diverged"
        );
        // Vectors agree up to an overall sign; align then compare.
        let dot: f64 = base_pairs[i]
            .vector
            .iter()
            .zip(custom_pairs[i].vector.iter())
            .map(|(a, b)| a * b)
            .sum();
        let sign = if dot < 0.0 { -1.0 } else { 1.0 };
        let max_diff = base_pairs[i]
            .vector
            .iter()
            .zip(custom_pairs[i].vector.iter())
            .map(|(a, b)| (a - sign * b).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            max_diff < 1e-6,
            "eigenvector {i} diverged: max |Δ| = {max_diff}"
        );
    }
}

/// CORRECTNESS GATE (issue #518): the computed spectrum must be identical
/// whether the faer sparse-LU factorization runs single-threaded or
/// multi-threaded. The factorization is deterministic, so we assert
/// *bit-for-bit* equality (not merely "within tolerance") across thread
/// counts — any drift would signal a nondeterministic parallel LU, which
/// would be a correctness bug rather than acceptable rounding.
///
/// Since issue #755 the factorization guard is `ParallelismGuard::cap`,
/// so the `1`-thread run here is a genuinely serial (`Par::Seq`) LU;
/// before that it left faer's all-core default in place.
///
/// This test drives the thread count through the `_with_threads` entry
/// point (the same code path the `GEODE_NUM_THREADS` knob feeds), which
/// avoids mutating a process-global env var — this crate denies
/// `unsafe_code` and edition-2024 `env::set_var` is `unsafe`.
#[test]
fn eigenvalues_agree_across_thread_counts() {
    // Hold the shared parallelism lock: the solves transiently set faer's
    // process-global parallelism during factorization, which would
    // otherwise race the guard RAII test in `eigen::parallel`.
    let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
        .lock()
        .unwrap();

    let (k, m) = laplacian_pencil(64);
    let solver = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 64,
        tol: 1e-11,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };

    let serial = solver
        .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 5, 1)
        .unwrap();
    let parallel = solver
        .smallest_eigenvalues_with_threads(k.as_ref(), m.as_ref(), 5, 4)
        .unwrap();

    assert_eq!(
        serial.len(),
        parallel.len(),
        "thread count changed the number of converged modes"
    );
    for (i, (s, p)) in serial.iter().zip(parallel.iter()).enumerate() {
        assert_eq!(
            s.to_bits(),
            p.to_bits(),
            "eigenvalue[{i}] differs across thread counts: serial={s}, parallel={p}"
        );
    }
}

/// Companion to the agreement test: confirm the eigenpair path (vectors,
/// not just eigenvalues) is also thread-count invariant within the
/// solver's tolerance. Vectors carry an arbitrary global sign, so compare
/// eigenvalues bit-for-bit and vectors up to sign.
#[test]
fn eigenpairs_agree_across_thread_counts() {
    let _lock = crate::eigen::parallel::PARALLELISM_TEST_LOCK
        .lock()
        .unwrap();

    let (k, m) = laplacian_pencil(48);
    let solver = SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 64,
        tol: 1e-11,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };

    let serial = solver
        .smallest_eigenpairs_with_threads(k.as_ref(), m.as_ref(), 4, 1)
        .unwrap();
    let parallel = solver
        .smallest_eigenpairs_with_threads(k.as_ref(), m.as_ref(), 4, 4)
        .unwrap();

    assert_eq!(serial.len(), parallel.len());
    for (i, (s, p)) in serial.iter().zip(parallel.iter()).enumerate() {
        assert_eq!(
            s.lambda.to_bits(),
            p.lambda.to_bits(),
            "eigenpair λ[{i}] differs across thread counts"
        );
        // Vectors may differ by a global sign; align on the dominant
        // entry, then compare component-wise within tolerance.
        let pivot = s
            .vector
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(idx, _)| idx)
            .unwrap();
        let sign = (s.vector[pivot] * p.vector[pivot]).signum();
        for (a, b) in s.vector.iter().zip(p.vector.iter()) {
            assert!(
                (a - sign * b).abs() < 1e-9,
                "eigenvector[{i}] differs beyond tolerance across thread counts"
            );
        }
    }
}

// -------------------------------------------------------------------
// Matrix-free shift-invert inner solve (issue #524).
// -------------------------------------------------------------------

/// CORRECTNESS GATE (issue #524): the matrix-free inner-solve variant
/// (`InnerSolver::MatrixFree`, Jacobi-CG on `(K − σM) y = b`, no LU)
/// reproduces the direct sparse-LU path's eigenvalues to tight
/// tolerance on an SPD pencil with the shift placed **below** the
/// spectrum (so `K − σM` is SPD, the Phase-1 lowest-mode case).
///
/// The Laplacian pencil `tridiag(-1, 2, -1)` with `M = I` has all
/// eigenvalues in `(0, 4)`; `σ = -1.0` sits below the whole spectrum,
/// making `K − σM = K + I` SPD.
#[test]
fn matrix_free_matches_direct_eigenvalues() {
    let (k, m) = laplacian_pencil(40);
    let sigma = -1.0;
    let direct = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let mf = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::MatrixFree,
        precond: InnerPreconditioner::Jacobi,
    };

    let ld = direct
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
        .unwrap();
    let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

    assert_eq!(ld.len(), lm.len(), "mode count differs direct vs mf");
    for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
        // Both paths solve the SAME generalized pencil; the only
        // difference is the inner solve. They must agree to a tolerance
        // well inside the ≤1% physical bar.
        let rel = (d - f).abs() / d.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "eigenvalue[{i}] direct={d} matrix-free={f} rel-diff={rel:.2e} > 1e-6"
        );
    }
}

/// Companion eigenpair cross-check: the matrix-free path recovers the
/// same eigenvectors (up to global sign) as the direct path on the SPD
/// below-spectrum-shifted Laplacian pencil.
#[test]
fn matrix_free_matches_direct_eigenpairs() {
    let (k, m) = laplacian_pencil(32);
    let sigma = -1.0;
    let direct = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let mf = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::MatrixFree,
        precond: InnerPreconditioner::Jacobi,
    };

    let pd = direct
        .smallest_eigenpairs(k.as_ref(), m.as_ref(), 4)
        .unwrap();
    let pf = mf.smallest_eigenpairs(k.as_ref(), m.as_ref(), 4).unwrap();

    assert_eq!(pd.len(), pf.len());
    for (i, (d, f)) in pd.iter().zip(pf.iter()).enumerate() {
        let rel = (d.lambda - f.lambda).abs() / d.lambda.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "eigenpair λ[{i}] direct={} matrix-free={} rel-diff={rel:.2e}",
            d.lambda,
            f.lambda
        );
        // Align sign on the dominant entry, then compare component-wise.
        let pivot = d
            .vector
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap())
            .map(|(idx, _)| idx)
            .unwrap();
        let sign = (d.vector[pivot] * f.vector[pivot]).signum();
        for (a, b) in d.vector.iter().zip(f.vector.iter()) {
            assert!(
                (a - sign * b).abs() < 1e-5,
                "eigenvector[{i}] direct vs matrix-free differ beyond tolerance"
            );
        }
    }
}

/// The matrix-free inner CG solves `(K − σM) y = b` to the requested
/// residual on a well-shifted SPD pencil, and its Jacobi diagonal is
/// exactly `K_ii − σ M_ii`. This is a direct unit test of the inner
/// operator + solver in isolation (no outer Lanczos).
#[test]
fn matrix_free_inner_cg_solves_spd_system() {
    let (k, m) = laplacian_pencil(50);
    let sigma = -0.5;
    let op = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma);

    // Jacobi diagonal check: K_ii = 2, M_ii = 1 ⇒ 1/(2 - (-0.5)) = 1/2.5.
    for (i, &d) in op.inv_diag.iter().enumerate() {
        assert!(
            (d - 1.0 / 2.5).abs() < 1e-12,
            "inv_diag[{i}] = {d}, want {}",
            1.0 / 2.5
        );
    }

    // Solve (K − σM) y = b for a known b; verify the residual.
    let n = k.nrows();
    let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.31).cos()).collect();
    let mut y = vec![0.0_f64; n];
    let iters = cg_solve_matrix_free(&op, &b, &mut y, 1e-12, 4 * n).unwrap();
    assert!(
        iters > 0 && iters <= n,
        "unexpected CG iteration count {iters}"
    );

    // Residual r = b − (K − σM) y should be tiny relative to ‖b‖.
    let mut ay = vec![0.0_f64; n];
    op.apply(&y, &mut ay);
    let bnorm = b.iter().map(|v| v * v).sum::<f64>().sqrt();
    let rnorm = b
        .iter()
        .zip(ay.iter())
        .map(|(bi, ai)| (bi - ai) * (bi - ai))
        .sum::<f64>()
        .sqrt();
    assert!(
        rnorm / bnorm < 1e-10,
        "matrix-free CG residual too large: {:.3e}",
        rnorm / bnorm
    );
}

// -------------------------------------------------------------------
// Interior / indefinite matrix-free inner solve — MINRES (issue #535).
// -------------------------------------------------------------------

/// Regression guard: the default inner-solve backend is the SPD direct LU
/// path — the indefinite MINRES variant must be purely additive/opt-in and
/// must NOT become the default.
#[test]
fn default_inner_is_direct() {
    assert_eq!(
        SparseShiftInvertLanczos::default().inner,
        InnerSolver::Direct
    );
}

/// Unit test of the indefinite inner solver in isolation (no outer
/// Lanczos): with an **interior** shift `(K − σM)` is genuinely
/// symmetric-indefinite (we assert its diagonal carries negative entries,
/// proving the SPD CG path would break down), yet matrix-free MINRES with
/// the absolute-value Jacobi preconditioner drives the residual to the
/// requested tolerance.
#[test]
fn minres_solves_indefinite_system() {
    let (k, m) = laplacian_pencil(50);
    // Laplacian eigenvalues lie in (0, 4). σ = 2.5 sits INSIDE the
    // spectrum, so K − σM = tridiag(-1, 2 - 2.5, -1) has diagonal
    // 2 - 2.5 = -0.5 < 0 everywhere ⇒ indefinite.
    let sigma = 2.5;
    let op = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma).with_abs_diag();

    // Signed diagonal K_ii − σM_ii = -0.5 < 0 ⇒ the plain (signed) Jacobi
    // diagonal would be negative; the abs-Jacobi diagonal is +1/0.5 = 2.
    let signed = ShiftedMatrixFreeOp::new(k.as_ref(), m.as_ref(), sigma);
    assert!(
        signed.inv_diag.iter().any(|&d| d < 0.0),
        "expected a negative signed Jacobi diagonal (indefinite operator)"
    );
    for (i, &d) in op.inv_diag.iter().enumerate() {
        assert!(
            d > 0.0 && (d - 2.0).abs() < 1e-12,
            "abs-Jacobi inv_diag[{i}] = {d}, want +2.0 (SPD preconditioner)"
        );
    }

    // Solve (K − σM) y = b and verify the true residual is tiny.
    let n = k.nrows();
    let b: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.37).sin() + 0.5).collect();
    let mut y = vec![0.0_f64; n];
    let iters = minres_solve_matrix_free(&op, &b, &mut y, 1e-12, 4 * n).unwrap();
    assert!(
        iters > 0 && iters <= 4 * n,
        "unexpected MINRES iters {iters}"
    );

    let mut ay = vec![0.0_f64; n];
    op.apply(&y, &mut ay);
    let bnorm = b.iter().map(|v| v * v).sum::<f64>().sqrt();
    let rnorm = b
        .iter()
        .zip(ay.iter())
        .map(|(bi, ai)| (bi - ai) * (bi - ai))
        .sum::<f64>()
        .sqrt();
    assert!(
        rnorm / bnorm < 1e-8,
        "matrix-free MINRES residual too large: {:.3e}",
        rnorm / bnorm
    );
}

/// CORRECTNESS GATE (issue #535): the indefinite matrix-free variant
/// (`InnerSolver::MatrixFreeIndefinite`, MINRES) reproduces the direct
/// sparse-LU path's eigenvalues at an **interior** shift, where `(K − σM)`
/// is symmetric-indefinite and the SPD CG path is invalid. σ = 2.5 sits
/// inside the Laplacian spectrum `(0, 4)`.
#[test]
fn minres_matches_direct_interior_shift() {
    let (k, m) = laplacian_pencil(40);
    let sigma = 2.5;
    let direct = SparseShiftInvertLanczos {
        sigma,
        max_iters: 80,
        tol: 1e-9,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let mf = SparseShiftInvertLanczos {
        sigma,
        max_iters: 80,
        tol: 1e-9,
        inner: InnerSolver::MatrixFreeIndefinite,
        precond: InnerPreconditioner::Jacobi,
    };

    let ld = direct
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
        .unwrap();
    let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

    assert_eq!(ld.len(), lm.len(), "mode count differs direct vs MINRES");
    for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
        let rel = (d - f).abs() / d.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "interior eigenvalue[{i}] direct={d} MINRES={f} rel-diff={rel:.2e} > 1e-6"
        );
    }
}

/// EDGE CASE (issue #535): MINRES is a superset of CG on SPD systems, so
/// selecting the indefinite variant on an SPD (below-spectrum) shift must
/// still converge to the direct result — a guard against the indefinite
/// solver regressing the SPD case if a caller picks it there.
#[test]
fn minres_matches_direct_spd_below_shift() {
    let (k, m) = laplacian_pencil(40);
    // σ = -1.0 is below the whole spectrum ⇒ K − σM = K + I is SPD.
    let sigma = -1.0;
    let direct = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::Direct,
        precond: InnerPreconditioner::Jacobi,
    };
    let mf = SparseShiftInvertLanczos {
        sigma,
        max_iters: 64,
        tol: 1e-10,
        inner: InnerSolver::MatrixFreeIndefinite,
        precond: InnerPreconditioner::Jacobi,
    };

    let ld = direct
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), 5)
        .unwrap();
    let lm = mf.smallest_eigenvalues(k.as_ref(), m.as_ref(), 5).unwrap();

    assert_eq!(ld.len(), lm.len());
    for (i, (d, f)) in ld.iter().zip(lm.iter()).enumerate() {
        let rel = (d - f).abs() / d.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "SPD eigenvalue[{i}] direct={d} MINRES={f} rel-diff={rel:.2e} > 1e-6"
        );
    }
}

/// Issue #852: the eigenpair tridiagonal solve is homogeneous. A
/// `k = 160` tridiagonal (above faer's divide-and-conquer threshold) with
/// a tight eigenvalue cluster, scaled by `c ∈ {1e-13, 1e-19, 1e8}`, gives
/// `c ×` the unscaled eigenvalues to round-off. Upstream faer 0.24's
/// absolute `8ε` deflation tolerance merged the cluster when `c ≪ 1`
/// (8.6e-4 drift at `c = 1e-13`); the `faier` fork's unit-norm pre-scale
/// (issue #908) keeps it apart without any scaling here.
#[test]
fn tridiag_eigenpairs_is_scale_invariant() {
    let k = 160;
    // A cluster of near-equal diagonal entries (like the gradient-null
    // Ritz values) plus a spread of distinct ones, weakly coupled.
    let alpha: Vec<f64> = (0..k)
        .map(|i| {
            if i % 4 == 0 {
                -1.0 + 1e-6 * i as f64
            } else {
                0.2 + 0.01 * ((i as f64) * 0.731).sin()
            }
        })
        .collect();
    let beta: Vec<f64> = (0..k - 1)
        .map(|i| 1e-3 * (1.0 + ((i as f64) * 0.377).cos()))
        .collect();
    let (mu_ref, _) = tridiag_eigenpairs(&alpha, &beta).unwrap();
    for c in [1e-13, 1e-19, 1e8] {
        let a: Vec<f64> = alpha.iter().map(|x| c * x).collect();
        let b: Vec<f64> = beta.iter().map(|x| c * x).collect();
        let (mu, s) = tridiag_eigenpairs(&a, &b).unwrap();
        let worst = mu
            .iter()
            .zip(&mu_ref)
            .map(|(m, r)| (m / c - r).abs())
            .fold(0.0, f64::max);
        assert!(worst < 1e-13, "scale {c:e}: eigenvalue drift {worst:e}");
        // The eigenvectors still diagonalize the scaled T.
        for col in [0, k / 2, k - 1] {
            let mut r2 = 0.0_f64;
            for i in 0..k {
                let mut tv = a[i] * s[(i, col)];
                if i > 0 {
                    tv += b[i - 1] * s[(i - 1, col)];
                }
                if i + 1 < k {
                    tv += b[i] * s[(i + 1, col)];
                }
                r2 += (tv - mu[col] * s[(i, col)]).powi(2);
            }
            assert!(r2.sqrt() < 1e-13 * c, "scale {c:e}: col {col} residual");
        }
    }
}
