//! Acceptance tests for the sparse generalized eigensolver path.
//!
//! Two checks, per the curator's #13 spec:
//!
//! 1. **Oracle agreement**: at small n, the sparse Lanczos result must
//!    agree with the dense `faer` backend to **1e-6 relative** on the
//!    lowest 5 modes. The dense backend is the correctness oracle.
//!
//! 2. **Convergence slope**: across three refinement levels, the (1,1,1)
//!    ground-mode error decreases at the second-order rate expected for
//!    P1 + consistent mass. Slope of `log(err)` vs `log(h)` is in
//!    `[-2.2, -1.8]`.
//!
//! The optional `arpack` feature also adds a third check
//! (`arpack_matches_dense_at_n5`) that exercises the same n=5 oracle
//! bound against the system ARPACK driver. It is feature-gated and only
//! compiled when `--features arpack` is on; it satisfies the issue #24
//! acceptance criterion (1e-6 oracle agreement at n=5).
//!
//! # Running these tests
//!
//! All tests are `#[ignore]`d by default with the same rationale as the
//! dense `tests/eigensolver.rs`: faer 0.24's dense generalized eigen path
//! (used by the oracle) panics under debug-assertions. The sparse path
//! itself does not depend on `qz_real`, but the oracle comparison does.
//! Run with:
//!
//! ```sh
//! cargo test -p geode-core --release --test sparse_eigensolver -- --ignored
//!
//! # With ARPACK (requires libarpack — see README §System dependencies):
//! cargo test --features arpack -p geode-core --release \
//!     --test sparse_eigensolver -- --ignored
//! ```

use geode_core::{
    DefaultBackend, EigenSolver, FaerDenseEigensolver, SparseEigenSolver, SparseShiftInvertLanczos,
    apply_dirichlet_bc, assemble_global_p1, burn_matrix_to_faer, cube_interior_mask, cube_tet_mesh,
    global_system_to_sparse, upload_mesh,
};

use burn::tensor::backend::BackendTypes;

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Solve the lowest `n_modes` eigenvalues with the sparse Lanczos backend
/// on an `n×n×n` unit-cube Dirichlet Laplacian.
fn sparse_eigs(n: usize, n_modes: usize) -> Vec<f64> {
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let sparse = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");

    SparseShiftInvertLanczos {
        sigma: 0.0,
        max_iters: 80,
        tol: 1e-10,
    }
    .smallest_eigenvalues(sparse.k.as_ref(), sparse.m.as_ref(), n_modes)
    .expect("sparse eigensolve")
}

/// Solve the lowest `n_modes` eigenvalues with the dense `faer` oracle on
/// the same mesh.
fn dense_eigs(n: usize, n_modes: usize) -> Vec<f64> {
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let (k, m) =
        apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &mask).expect("dense BC reduction");

    FaerDenseEigensolver
        .smallest_eigenvalues(k.as_ref(), m.as_ref(), n_modes)
        .expect("dense eigensolve")
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
fn sparse_matches_dense_at_n5() {
    // n=5 cube: 216 nodes, 64 interior — small enough for the dense oracle
    // and big enough that 5 modes is a real test of Lanczos convergence.
    let dense = dense_eigs(5, 5);
    let sparse = sparse_eigs(5, 5);

    eprintln!("dense  λ = {dense:.10?}");
    eprintln!("sparse λ = {sparse:.10?}");

    assert_eq!(sparse.len(), 5);
    for (i, (s, d)) in sparse.iter().zip(dense.iter()).enumerate() {
        let rel = (s - d).abs() / d.abs().max(1.0);
        assert!(
            rel < 1e-6,
            "λ[{i}]: sparse {s}, dense {d}, rel err {rel:.3e} exceeds 1e-6"
        );
    }
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
fn sparse_convergence_slope() {
    // Ground-mode (1,1,1) target = 3π². Refine across n ∈ {6, 10, 14} and
    // fit log(err) vs log(h). Slope should be in [-2.2, -1.8] for the
    // standard P1 + consistent-mass O(h²) rate.
    let target = 3.0 * std::f64::consts::PI.powi(2);

    let ns = [6_usize, 10, 14];
    let mut h = Vec::with_capacity(ns.len());
    let mut e = Vec::with_capacity(ns.len());
    for &n in ns.iter() {
        let g = sparse_eigs(n, 1)[0];
        let err = (g - target).abs() / target;
        let h_n = 1.0 / (n as f64);
        eprintln!("n={n:>3}  h={h_n:.4}  λ₁={g:.6}  rel_err={err:.4e}");
        h.push(h_n);
        e.push(err);
    }

    // Least-squares slope of log(err) vs log(h).
    let n_pts = h.len() as f64;
    let lh: Vec<f64> = h.iter().map(|x| x.ln()).collect();
    let le: Vec<f64> = e.iter().map(|x| x.ln()).collect();
    let mean_lh = lh.iter().sum::<f64>() / n_pts;
    let mean_le = le.iter().sum::<f64>() / n_pts;
    let num: f64 = lh
        .iter()
        .zip(le.iter())
        .map(|(x, y)| (x - mean_lh) * (y - mean_le))
        .sum();
    let den: f64 = lh.iter().map(|x| (x - mean_lh).powi(2)).sum();
    let slope = num / den;

    eprintln!("convergence slope (log err vs log h) = {slope:.4} (expect ≈ 2)");
    assert!(
        (1.8..=2.2).contains(&slope),
        "convergence slope {slope:.4} not in [1.8, 2.2] — expected O(h²)"
    );
}

// ---------------------------------------------------------------------------
// Issue #24: ARPACK-backed sparse eigensolver oracle agreement at n=5.
//
// Only built when `--features arpack` is on. Same fixture and tolerance
// as the Lanczos check above, but uses `ArpackEigensolver` instead of
// `SparseShiftInvertLanczos`. This satisfies the issue acceptance
// (matches dense oracle to 1e-6 at n=5).
// ---------------------------------------------------------------------------

#[cfg(feature = "arpack")]
mod arpack_oracle {
    use super::*;
    use geode_core::ArpackEigensolver;

    fn arpack_eigs(n: usize, n_modes: usize) -> Vec<f64> {
        let mesh = cube_tet_mesh(n, 1.0);
        let (nodes, tets) = upload_mesh::<B>(&mesh, &device());
        let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
        let mask = cube_interior_mask(&mesh.nodes, 1.0);
        let sparse = global_system_to_sparse(sys, Some(&mask)).expect("sparse projection");

        ArpackEigensolver::default()
            .smallest_eigenvalues(sparse.k.as_ref(), sparse.m.as_ref(), n_modes)
            .expect("ARPACK eigensolve")
    }

    #[test]
    #[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
    fn arpack_matches_dense_at_n5() {
        // Same fixture as sparse_matches_dense_at_n5: n=5 cube, 5 modes,
        // 1e-6 relative tolerance against the faer dense oracle.
        let dense = dense_eigs(5, 5);
        let arpack = arpack_eigs(5, 5);

        eprintln!("dense  λ = {dense:.10?}");
        eprintln!("arpack λ = {arpack:.10?}");

        assert_eq!(arpack.len(), 5);
        for (i, (s, d)) in arpack.iter().zip(dense.iter()).enumerate() {
            let rel = (s - d).abs() / d.abs().max(1.0);
            assert!(
                rel < 1e-6,
                "λ[{i}]: arpack {s}, dense {d}, rel err {rel:.3e} exceeds 1e-6"
            );
        }
    }
}
