//! PEC dielectric-sphere eigenmode integration test (issue #26).
//!
//! Drives the full first-order Nédélec pipeline on the bundled
//! sphere-in-vacuum fixture (#25) with per-element relative permittivity:
//!
//! - `ε_r = n² = 2.25` for tets tagged `sphere_interior` (n = 1.5).
//! - `ε_r = 1`           for tets tagged `vacuum_buffer`.
//!
//! The outer boundary `r = R_BUFFER` is the PEC wall (`n × E = 0`), so
//! every edge whose two endpoints both sit on that surface is removed
//! before the generalized eigensolve. The discrete curl-curl operator
//! has a large gradient kernel (Whitney 1-forms include all `∇φ` for
//! φ ∈ H¹_0); the spurious-mode dimension equals the number of vertices
//! strictly inside the PEC sphere (i.e. not on the outer wall).
//!
//! # Acceptance
//!
//! We use a **soft acceptance** appropriate for the coarse fixture
//! (313 nodes / ~1226 tets). Authoritative Mie PEC-dielectric-sphere
//! roots for `n=1.5, R_sphere=1.0, R_buffer=2.0` are not yet tabulated
//! in this codebase; deriving them from scratch is its own non-trivial
//! root-finding problem (`J_{l+1/2}` zeros of the boundary determinant
//! across the dielectric interface). Until those roots land we assert:
//!
//! 1. The lowest 5 physical eigenvalues are positive and real.
//! 2. There is a clear spectral gap between the spurious nullspace
//!    cluster and the first physical mode (≥ 100× scale jump).
//! 3. The spurious-mode count matches the predicted gradient kernel
//!    dimension (= number of interior vertices not on the outer wall).
//!
//! Quantitative Mie comparison is tracked as a follow-up.
//!
//! # Running
//!
//! This test is `#[ignore]`d because faer 0.24's `gevd::qz_real` panics
//! under `debug-assertions` (same root cause as `tests/eigensolver.rs`).
//! Run with:
//!
//! ```sh
//! cargo test -p geode-core --release --test sphere_pec_eigenmode -- --ignored
//! ```

use burn::tensor::backend::BackendTypes;

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec_with_epsilon, build_epsilon_r, burn_matrix_to_faer,
    read_sphere_fixture, sphere_n_interior_nodes, sphere_pec_interior_edges, upload_mesh,
    DefaultBackend, EigenSolver, FaerDenseEigensolver, R_BUFFER,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[test]
fn epsilon_r_assignment_matches_physical_groups() {
    // Smoke: ε_r assignment matches the physical-group convention.
    let f = read_sphere_fixture().expect("fixture load");
    let eps = build_epsilon_r(&f.tet_physical_tags, 1.5);

    assert_eq!(eps.len(), f.mesh.n_tets());
    let interior_count = eps.iter().filter(|&&e| (e - 2.25).abs() < 1e-12).count();
    let vacuum_count = eps.iter().filter(|&&e| (e - 1.0).abs() < 1e-12).count();
    assert_eq!(interior_count, f.n_interior_tets());
    assert_eq!(vacuum_count, f.n_buffer_tets());
    assert_eq!(interior_count + vacuum_count, f.mesh.n_tets());
}

#[test]
fn pec_mask_excludes_outer_wall_edges() {
    // Edges with both endpoints on r = R_BUFFER are flagged as PEC
    // (mask false); every other edge is interior (mask true).
    let f = read_sphere_fixture().expect("fixture load");
    let (edges, mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    let tol = 1e-6_f64 * R_BUFFER;

    let on_wall = |i: u32| -> bool {
        let p = f.mesh.nodes[i as usize];
        let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
        (r - R_BUFFER).abs() < tol
    };

    for (e, &m) in edges.iter().zip(mask.iter()) {
        let on_a = on_wall(e[0]);
        let on_b = on_wall(e[1]);
        if on_a && on_b {
            assert!(!m, "edge {e:?} with both ends on outer wall must be PEC");
        } else {
            assert!(
                m,
                "edge {e:?} with at least one interior endpoint must survive"
            );
        }
    }

    let n_interior_edges = mask.iter().filter(|&&b| b).count();
    let n_pec_edges = mask.len() - n_interior_edges;
    assert!(n_interior_edges > 0, "no interior edges survived PEC mask");
    assert!(
        n_pec_edges > 0,
        "no PEC edges removed (outer wall missing?)"
    );

    eprintln!(
        "sphere PEC mask: {} edges total, {} interior, {} on outer wall",
        edges.len(),
        n_interior_edges,
        n_pec_edges,
    );
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
fn sphere_pec_eigenmode_spectrum() {
    // 1. Load the sphere fixture.
    let f = read_sphere_fixture().expect("fixture load");
    eprintln!(
        "sphere fixture: {} nodes, {} tets ({} interior + {} buffer)",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.n_interior_tets(),
        f.n_buffer_tets(),
    );

    // 2. Per-tet permittivity: ε_r = n² inside, 1 in the vacuum buffer.
    let n_index = 1.5_f64;
    let epsilon_r = build_epsilon_r(&f.tet_physical_tags, n_index);

    // 3. Edge tables and sign convention.
    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();
    eprintln!("global edges: {n_edges}");

    // 4. Upload + assemble with ε-scaled mass.
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &epsilon_r,
    );

    // 5. PEC edge mask + Dirichlet reduction.
    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "edge ordering mismatch");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "PEC reduction: {} edges → {} interior DOFs",
        n_edges, n_interior_edges
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);
    let (k_int, m_int) =
        apply_dirichlet_bc(k_full.as_ref(), m_full.as_ref(), &interior_mask).expect("BC reduction");

    // 6. Solve generalized eigenproblem.
    // We ask for enough eigenvalues to skip past the spurious gradient
    // nullspace and grab the lowest 5 physical modes.
    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    eprintln!("predicted spurious-mode count: {spurious_dim}");
    let n_request = spurious_dim + 8; // 5 physical + small safety margin
    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), n_request)
        .expect("eigensolve");

    // 7. Diagnostic dump of the lowest eigenvalues.
    eprintln!(
        "lowest {} eigenvalues (full spectrum slice):",
        lambdas.len()
    );
    for (i, lam) in lambdas.iter().enumerate() {
        eprintln!("  λ[{i:>3}] = {lam:.6e}");
    }

    // 8. Spurious-mode filter. Gradients of H¹_0 are in the kernel of the
    //    curl-curl operator; numerically they cluster near zero but not
    //    exactly at zero. We classify "spurious" as any eigenvalue whose
    //    magnitude is below a small fraction of the next (physical) mode.
    //
    //    Empirically the cleanest split is: sort, then find the largest
    //    relative gap inside the first `spurious_dim + 5` slots. The
    //    first index after that gap is the first physical mode.
    let mut gap_idx = 0usize;
    let mut best_gap = 0.0f64;
    let scan_to = (spurious_dim + 5).min(lambdas.len().saturating_sub(1));
    for i in 0..scan_to {
        // Use absolute jump on near-zero spurious cluster, relative once
        // we leave it.
        let a = lambdas[i].abs();
        let b = lambdas[i + 1].abs();
        // Avoid division by zero — fall back to absolute difference.
        let ratio = if a < 1e-9 { b } else { b / a };
        if ratio > best_gap {
            best_gap = ratio;
            gap_idx = i;
        }
    }
    eprintln!(
        "max ratio jump at index {gap_idx} → {gap_idx_plus_one}: ratio {best_gap:.3e}",
        gap_idx_plus_one = gap_idx + 1
    );
    let first_physical = gap_idx + 1;
    let n_spurious = first_physical;
    eprintln!("observed spurious count: {n_spurious} (predicted: {spurious_dim})");

    // Acceptance check 1: spurious count must match the predicted
    // gradient nullspace dimension exactly.
    assert_eq!(
        n_spurious, spurious_dim,
        "spurious count {n_spurious} disagrees with predicted gradient \
         nullspace dim {spurious_dim} — gradient kernel filter is off"
    );

    // Acceptance check 2: clear spectral gap (≥ 100×) between the
    // spurious cluster and the first physical mode. This catches the
    // case where ε scaling silently went sideways.
    assert!(
        best_gap > 1.0e2,
        "spurious → physical gap is only {best_gap:.3e}; expected ≥ 1e2 \
         (no clean separation suggests ε scaling or PEC mask is wrong)"
    );

    // Acceptance check 3: the lowest 5 physical eigenvalues are real,
    // strictly positive, and monotonically non-decreasing (sort
    // invariant from the eigensolver).
    let physical: Vec<f64> = lambdas
        .iter()
        .skip(first_physical)
        .take(5)
        .copied()
        .collect();
    assert_eq!(
        physical.len(),
        5,
        "did not recover 5 physical modes (got {})",
        physical.len()
    );
    eprintln!("lowest 5 physical eigenvalues (λ = k²) and k = √λ:");
    let mut prev = 0.0f64;
    for (i, &lam) in physical.iter().enumerate() {
        let k_val = lam.sqrt();
        eprintln!("  physical[{i}]: λ = {lam:.6e}, k = {k_val:.4} (1/length)");
        assert!(lam > 0.0, "physical[{i}] = {lam} must be strictly positive");
        assert!(
            lam >= prev,
            "physical eigenvalues must be monotone; physical[{i}] = {lam} < previous {prev}"
        );
        prev = lam;
    }

    // 9. Soft sanity: the ground physical mode k ≈ √λ should lie in a
    //    physically plausible band for a dielectric sphere of radius 1
    //    inside a PEC box of radius 2 with refractive index 1.5. The
    //    naive estimate for a TE₁₁₁-like dielectric Mie mode is
    //    k ≈ π / (n·R) ≈ 2.1, but the PEC outer wall pulls modes
    //    *down* (larger cavity) and the discretization on 313 nodes
    //    pushes them *up*. A wide band [0.5, 8.0] catches both
    //    directions while still flagging gross failures (e.g. a mode at
    //    k ~ 100 would indicate ε scaling didn't take).
    let k0 = physical[0].sqrt();
    assert!(
        (0.5..=8.0).contains(&k0),
        "ground-mode wavenumber k = {k0:.4} outside plausible band [0.5, 8.0]"
    );

    eprintln!(
        "soft acceptance: {} spurious modes filtered, 5 physical modes \
         positive/real, ground k = {k0:.4}",
        n_spurious
    );
}
