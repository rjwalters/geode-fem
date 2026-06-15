//! Rectangular metallic waveguide transverse-modal eigensolver
//! integration test (Epic #234 wave-port, Phase 1, issue #235).
//!
//! Drives the new 2-D Whitney/Nédélec modal eigensolver
//! (`crate::waveguide_modes`) on the in-memory rectangular cross-section
//! generator (`rect_tri_mesh`) and pairs the lowest FEM cutoff
//! wavenumbers `k_c` against the analytic rectangular-waveguide oracle
//!
//! ```text
//! k_c(m, n) = √((m π / a)² + (n π / b)²).
//! ```
//!
//! Acceptance criteria (#235):
//!
//! 1. The 2-D modal eigensolve runs and produces `(k_c, λ)` pairs after
//!    filtering the gradient-nullspace cluster.
//! 2. The dominant TE₁₀ mode (`k_c = π / a`) is recovered within 2 % on a
//!    moderately refined 16×8 mesh — equivalent to a cutoff-frequency
//!    error of `c / (2a) × 2 % = 1.5 GHz × 2 %` for WR-90.
//! 3. Each of the next three eigenvalues pairs to a `(m, n) ≤ (3, 3)`
//!    analytic catalog root within 5 %, covering TE₂₀ / TE₀₁ / TM₁₁ on
//!    the WR-90-aspect (a / b = 2) test geometry.
//!
//! Phase 2 (#236) builds the wave-port boundary condition and S-parameter
//! reduction on top of these modal profiles; it is out of scope here.
//!
//! Running:
//!
//! ```sh
//! cargo test -p geode-core --test rect_waveguide_modes
//! ```
//!
//! Both the default debug profile and `--release` are supported.
//! `solve_rect_waveguide_modes` (and its eigenvector sibling) is now
//! backed by the sparse real-symmetric shift-invert Lanczos
//! ([`SparseShiftInvertLanczos`](crate::lanczos::SparseShiftInvertLanczos))
//! after PR #249. The previous dense `faer::generalized_eigen` (QZ)
//! path tripped a wrap-around overflow inside `gevd::qz_real` under
//! rustc's default debug overflow checks (issue #244); the sparse
//! path avoids the QZ algorithm entirely, so the workspace no longer
//! needs the `rustflags = ["-C", "overflow-checks=off"]` workaround.

use geode_core::{
    apply_pec_2d, assemble_2d_nedelec, rect_pec_interior_edges, rect_pec_interior_nodes,
    rect_tri_mesh, rect_waveguide_cutoff, solve_rect_waveguide_modes, spurious_dim_2d, EigenSolver,
    FaerDenseEigensolver,
};

/// Two-step sanity: PEC reduction and de-Rham nullspace dimension agree
/// with the structured-mesh counts (one degree of freedom per interior
/// edge; spurious-mode count equals the number of strictly-interior
/// nodes).
#[test]
fn pec_reduction_and_spurious_dim_consistent() {
    let (nx, ny) = (8usize, 4usize);
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(nx, ny, a, b);
    let (_edges, mask_edges) = rect_pec_interior_edges(&mesh, a, b);
    let mask_nodes = rect_pec_interior_nodes(&mesh, a, b);

    let n_interior_edges = mask_edges.iter().filter(|&&b| b).count();
    let n_interior_nodes = mask_nodes.iter().filter(|&&b| b).count();
    let spurious = spurious_dim_2d(&mesh, &mask_edges, &mask_nodes);

    eprintln!(
        "{nx}x{ny} structured rect mesh: \
         {} edges total, {n_interior_edges} interior, \
         {} nodes total, {n_interior_nodes} interior interior nodes, \
         spurious(d⁰_int rank) = {spurious}",
        mesh.edges().len(),
        mesh.n_nodes()
    );

    // Discrete H¹_0 → H(curl) injectivity: every interior nodal scalar
    // contributes a distinct gradient mode in the 2-D Whitney/Nédélec
    // pair (same statement as `sphere_pec_eigenmode.rs`'s acceptance #1).
    assert_eq!(
        spurious, n_interior_nodes,
        "d⁰-rank spurious dim {spurious} differs from interior-node \
         count {n_interior_nodes} on a structured rect mesh"
    );

    // 8x4 mesh: 9*5 = 45 nodes; boundary nodes = 2*(8+4) = 24, so
    // 21 interior nodes.
    assert_eq!(n_interior_nodes, 21);
}

/// Phase-1 acceptance: TE₁₀ / TE₂₀ / TE₀₁ / TM₁₁ recovery on a 16×8
/// structured mesh of the WR-90-aspect waveguide (a = 2, b = 1 in
/// arbitrary length units).
#[test]
fn rect_waveguide_modal_spectrum_matches_analytic() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(16, 8, a, b);
    let n_modes = 5;
    let modes = solve_rect_waveguide_modes(&mesh, a, b, n_modes).expect("modal eigensolve");
    assert_eq!(
        modes.len(),
        n_modes,
        "expected {n_modes} modes, got {}",
        modes.len()
    );

    let pi = std::f64::consts::PI;
    let kc_te10 = pi / a;

    eprintln!("rect waveguide modal cutoffs (a={a}, b={b}):");
    for (i, m) in modes.iter().enumerate() {
        eprintln!("  mode[{i}]: λ = {:.6e}, k_c = {:.5}", m.lambda, m.k_c);
    }

    // 1. Dominant mode is real and matches TE₁₀ within 2 %.
    let mode0 = &modes[0];
    assert!(mode0.lambda > 0.0, "TE₁₀ eigenvalue must be positive");
    assert!(mode0.k_c > 0.0, "TE₁₀ cutoff wavenumber must be positive");
    let rel_err_te10 = (mode0.k_c - kc_te10).abs() / kc_te10;
    eprintln!(
        "TE₁₀: fem k_c = {:.5}, analytic = {:.5} (rel err {:.2}%)",
        mode0.k_c,
        kc_te10,
        100.0 * rel_err_te10
    );
    assert!(
        rel_err_te10 < 0.02,
        "TE₁₀ cutoff disagreement: fem = {:.5}, analytic = {:.5} ({:.2}%)",
        mode0.k_c,
        kc_te10,
        100.0 * rel_err_te10
    );

    // 2. Each of the next three FEM cutoffs pairs to a `(m,n) ≤ (3,3)`
    //    analytic root within 5 %. Pairing is closest-k_c; two FEM modes
    //    may pair to the same analytic root when the analytic spectrum is
    //    degenerate (TE₂₀ and TE₀₁ are exactly degenerate at k_c = π for
    //    a / b = 2).
    let catalog: Vec<(u32, u32, f64)> = (0..=3)
        .flat_map(|m| (0..=3).map(move |n| (m as u32, n as u32)))
        .filter(|&(m, n)| !(m == 0 && n == 0))
        .map(|(m, n)| (m, n, rect_waveguide_cutoff(m, n, a, b)))
        .collect();
    let rel_tol = 0.05_f64;
    for (i, mode) in modes.iter().enumerate().take(4) {
        let closest = catalog
            .iter()
            .min_by(|x, y| {
                (x.2 - mode.k_c)
                    .abs()
                    .partial_cmp(&(y.2 - mode.k_c).abs())
                    .unwrap()
            })
            .expect("non-empty catalog");
        let rel_err = (mode.k_c - closest.2).abs() / closest.2;
        eprintln!(
            "  mode[{i}]: k_c = {:.5} → closest analytic ({},{}) k_c = {:.5} (rel err {:.2}%)",
            mode.k_c,
            closest.0,
            closest.1,
            closest.2,
            100.0 * rel_err
        );
        assert!(
            rel_err <= rel_tol,
            "mode[{i}] k_c = {:.5} does not pair to any (m,n) ≤ (3,3) within {:.0}%; \
             closest is ({},{}) at k_c = {:.5} ({:.2}%)",
            mode.k_c,
            100.0 * rel_tol,
            closest.0,
            closest.1,
            closest.2,
            100.0 * rel_err
        );
    }
}

/// Convergence sanity: refining the mesh from 4×2 to 8×4 to 16×8
/// monotonically improves the TE₁₀ cutoff error and pushes it well
/// below 1 % at the finest level. Documents the achieved errors per
/// the #235 acceptance criterion "document achieved errors".
///
/// Mesh sizes are kept modest because the test suite runs under the
/// default debug profile too. After PR #249 the underlying solver is
/// the sparse real-symmetric shift-invert Lanczos
/// ([`crate::lanczos::SparseShiftInvertLanczos`]) which scales much
/// better than the previous dense QZ path, but the test is still about
/// h-refinement convergence, not raw size.
#[test]
fn te10_cutoff_convergence() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let pi = std::f64::consts::PI;
    let kc_te10 = pi / a;
    let mut errors = Vec::new();
    for &(nx, ny) in &[(4usize, 2usize), (8, 4), (16, 8)] {
        let mesh = rect_tri_mesh(nx, ny, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 1).expect("modal eigensolve");
        let rel_err = (modes[0].k_c - kc_te10).abs() / kc_te10;
        eprintln!(
            "TE₁₀ on {nx}x{ny}: fem k_c = {:.5} (analytic {:.5}, rel err {:.3}%)",
            modes[0].k_c,
            kc_te10,
            100.0 * rel_err
        );
        errors.push(rel_err);
    }
    // Monotone decrease.
    assert!(
        errors[1] < errors[0],
        "TE₁₀ error did not decrease from 4x2 to 8x4: {:.3e} → {:.3e}",
        errors[0],
        errors[1]
    );
    assert!(
        errors[2] < errors[1],
        "TE₁₀ error did not decrease from 8x4 to 16x8: {:.3e} → {:.3e}",
        errors[1],
        errors[2]
    );
    // Finest level under 1 %.
    assert!(
        errors[2] < 1e-2,
        "TE₁₀ error on 16x8 mesh = {:.3e} not below 1 %",
        errors[2]
    );
}

/// Sanity that the assembled `K` and `M` are symmetric (a regression
/// guard for the per-element sign / scatter pattern).
#[test]
fn global_k_m_are_symmetric() {
    let mesh = rect_tri_mesh(4, 3, 1.7, 0.9);
    let (k, m) = assemble_2d_nedelec(&mesh);
    let n = k.nrows();
    for i in 0..n {
        for j in (i + 1)..n {
            assert!(
                (k[(i, j)] - k[(j, i)]).abs() < 1e-10,
                "K asymmetry at ({i},{j}): {:.3e} vs {:.3e}",
                k[(i, j)],
                k[(j, i)]
            );
            assert!(
                (m[(i, j)] - m[(j, i)]).abs() < 1e-10,
                "M asymmetry at ({i},{j}): {:.3e} vs {:.3e}",
                m[(i, j)],
                m[(j, i)]
            );
        }
    }
}

/// Sanity: the PEC-reduced pencil yields the same TE₁₀ cutoff via the
/// direct dense `EigenSolver` trait (`FaerDenseEigensolver`) as via the
/// convenience `solve_rect_waveguide_modes` wrapper (now backed by the
/// sparse `SparseShiftInvertLanczos` after PR #249). This locks the
/// wrapper's spurious-cluster filter and acts as a dense-vs-sparse
/// cross-check: any deviation flags either a solver swap regression or
/// a filtering bug.
///
/// Uses a small 6×3 mesh both to keep the dense oracle's O(n³) cost
/// manageable and because we only want a smoke-level check here (the
/// accuracy regression lives in
/// `rect_waveguide_modal_spectrum_matches_analytic` above).
#[test]
fn raw_eigensolver_matches_wrapper() {
    let (a, b) = (2.0_f64, 1.0_f64);
    let mesh = rect_tri_mesh(6, 3, a, b);
    let (k_global, m_global) = assemble_2d_nedelec(&mesh);
    let (_edges, mask_e) = rect_pec_interior_edges(&mesh, a, b);
    let mask_n = rect_pec_interior_nodes(&mesh, a, b);
    let (k_int, m_int) = apply_pec_2d(&k_global, &m_global, &mask_e);
    let spurious = spurious_dim_2d(&mesh, &mask_e, &mask_n);
    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), spurious + 3)
        .expect("eigensolve");
    let k_c_raw = lambdas[spurious].max(0.0).sqrt();

    let modes = solve_rect_waveguide_modes(&mesh, a, b, 3).expect("wrapper eigensolve");

    assert!(
        (k_c_raw - modes[0].k_c).abs() < 1e-9,
        "raw eigensolver TE₁₀ k_c = {:.6} differs from wrapper {:.6}",
        k_c_raw,
        modes[0].k_c
    );
}
