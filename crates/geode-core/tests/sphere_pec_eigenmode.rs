//! PEC dielectric-sphere eigenmode integration test (issue #26).
//!
//! Drives the full first-order Nédélec pipeline on the bundled
//! sphere-in-vacuum fixture (#25, layered per #38) with per-element
//! relative permittivity:
//!
//! - `ε_r = n² = 2.25` for tets tagged `sphere_interior` (n = 1.5).
//! - `ε_r = 1` for the surrounding vacuum (both `vacuum_gap` and
//!   `pml_shell` regions — see [`build_epsilon_r`]).
//!
//! The outer boundary `r = R_BUFFER` is the PEC wall (`n × E = 0`), so
//! every edge whose two endpoints both sit on that surface is removed
//! before the generalized eigensolve. The discrete curl-curl operator
//! has a large gradient kernel (Whitney 1-forms include all `∇φ` for
//! φ ∈ H¹_0); the spurious-mode dimension equals the rank of the
//! interior-restricted discrete gradient `d⁰_interior` (Epic #57, Phase
//! 3.A, Issue #81 — `kernel(K) = image(d⁰)` on the Whitney/Nédélec pair).
//!
//! # Acceptance
//!
//! We use a **soft acceptance** appropriate for the bundled coarse
//! fixture (774 nodes / 3335 tets). The lowest 5 physical eigenvalues
//! on this mesh sit at λ ≈ {1.42, 1.42, 1.42, 3.27, 3.28} — a 3-fold-
//! degenerate cluster near λ ≈ 1.42 followed by the next physical band
//! near λ ≈ 3.27. We assert:
//!
//! 1. The lowest 5 physical eigenvalues are positive and real.
//! 2. There is a clear gap between the spurious null cluster ceiling
//!    (largest |λ| classified as kernel) and the lowest physical mode
//!    `physical[0]` — ≥ 10× margin against the kernel threshold.
//! 3. The spurious-mode count matches the d⁰-rank prediction
//!    `rank(d⁰_interior)`, computed algebraically from the de-Rham
//!    operator independent of the eigenspectrum.
//! 4. Each of the lowest 5 physical eigenvalues pairs to an analytic
//!    Mie PEC-cavity root within 15 % relative on `k = √λ`.
//!
//! Previously this test used a largest-relative-gap eigenvalue
//! heuristic to count spurious modes. On the bundled 774-node fixture
//! that heuristic gave `n_spurious = 371` and mis-classified the
//! `λ ≈ 1.42` triplet as spurious; the d⁰-rank classifier gives the
//! algebraically correct `n_spurious = 368` (Issue #124). The d⁰
//! machinery comes from Issue #58 (`derham::gradient_map`) and Issue
//! #81 (`tests/derham_kernel_dim.rs::cube_pec_kernel_dim_matches_d0_rank`,
//! the precedent this test now extends to the sphere fixture).
//!
//! # Running
//!
//! This test runs under the **default (debug) `cargo test` profile**
//! without `#[ignore]`. faer 0.24's `gevd::qz_real` performs `usize`
//! subtractions that wrap during the QZ iteration and would panic with
//! `attempt to subtract with overflow` if integer overflow checks were
//! enabled (release math is correct). The workspace `Cargo.toml`
//! suppresses those checks via a top-level `overflow-checks = false` on
//! the `[profile.dev]` and `[profile.test]` profiles — a profile-level
//! override is required because cargo 1.96 cannot disable the check for
//! `faer` via a per-package override (see the comment block in
//! `Cargo.toml` for the full rationale, and
//! `tests/faer_qz_debug_overflow_guard.rs` for the always-on regression
//! guard). With that suppression in place this test is debug-safe:
//!
//! ```sh
//! cargo test -p geode-core --test sphere_pec_eigenmode
//! ```
//!
//! Note: the dense generalized eigensolve here is O(n³) over ~376
//! requested eigenvalues on the 774-node fixture and can take several
//! minutes even at `opt-level = 3`; it is debug-*safe*, not debug-fast.

use burn::tensor::backend::BackendTypes;

use geode_core::{
    DefaultBackend, EigenSolver, FaerDenseEigensolver, R_BUFFER, R_SPHERE, apply_dirichlet_bc,
    assemble_global_nedelec_with_epsilon, build_epsilon_r, burn_matrix_to_faer, merged_roots,
    read_sphere_fixture, sphere_pec_interior_edges, sphere_pec_node_interior_mask,
    spurious_dim_from_derham, upload_mesh,
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
    //
    // The spurious-mode dimension is computed *algebraically* via
    // `rank(d⁰_interior)` (Epic #57, Phase 3.A) — not via an eigenvalue
    // heuristic. This is the same machinery the cube/sphere precedents
    // in `tests/derham_kernel_dim.rs` use; on the bundled 774-node sphere
    // fixture it gives 368, which equals the number of interior nodes
    // (= dimension of `H¹_0(Ω) ∩ ℙ¹`) and exactly the kernel dimension
    // of the discrete curl-curl operator post-PEC reduction.
    let node_interior_mask = sphere_pec_node_interior_mask(&f.mesh, R_BUFFER);
    let spurious_dim = spurious_dim_from_derham(&f.mesh, &interior_mask, &node_interior_mask);
    eprintln!("d⁰-rank spurious-mode dimension: {spurious_dim}");
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

    // 8. Spurious-mode classifier: the first `spurious_dim` eigenvalues
    //    are in the gradient nullspace by construction
    //    (`kernel(K) = image(d⁰)`; see
    //    `tests/derham_kernel_dim.rs::cube_pec_kernel_dim_matches_d0_rank`
    //    for the integer-count proof on the cube fixture, and the
    //    `sphere_pml_kernel_dim_matches_d0_rank` companion on the
    //    sphere). No heuristic; the algebraic statement is tautological
    //    once `spurious_dim` is the d⁰ rank.
    let n_spurious = spurious_dim;
    let first_physical = n_spurious;
    let physical_band_floor = lambdas[first_physical];
    let largest_kernel_abs = lambdas[..n_spurious]
        .iter()
        .map(|l| l.abs())
        .fold(0.0_f64, f64::max);
    eprintln!(
        "algebraic spurious classifier: n_spurious = {n_spurious}; \
         largest |λ_kernel| = {:.3e}, physical[0] = {:.3e}",
        largest_kernel_abs, physical_band_floor
    );

    // Acceptance check 1 (replaces the old heuristic): the algebraic
    // d⁰-rank spurious count matches the number of strictly-interior
    // nodes. This is the discrete `H¹_0 → H(curl)` injectivity
    // statement: every interior nodal scalar gives a distinct gradient
    // edge-DOF mode. The check is redundant by construction with the
    // d⁰-rank computation but serves as a regression guard if the
    // `restrict_gradient_dense` / `rank_via_svd` helpers ever silently
    // drift.
    let n_interior_nodes = node_interior_mask.iter().filter(|&&b| b).count();
    assert_eq!(
        spurious_dim, n_interior_nodes,
        "d⁰-rank spurious dim {spurious_dim} differs from interior-node \
         count {n_interior_nodes} — discrete H¹_0 → H(curl) injectivity \
         broken at the algebraic level"
    );

    // Acceptance check 2: physical-band floor separation. The lowest
    // physical eigenvalue must sit well above the kernel cluster
    // ceiling — otherwise ε scaling or the PEC mask is silently off.
    // We require `physical[0] / largest_kernel_abs >= 10` (the same
    // 10× gap floor used by `derham_kernel_dim.rs`'s sphere PML test).
    //
    // The previous test required ≥ 100× on a heuristic "best ratio
    // jump". On the bundled 774-node fixture the spurious cluster
    // ceiling sits at ~3e-13 and `physical[0] ≈ 1.42`, giving a gap
    // of ~4.7e12 — twelve orders above the floor with plenty of
    // headroom. The 10× floor is the algebraically meaningful gap
    // (anything below 10× means the kernel cutoff in the SVD threshold
    // would be flirting with the physical band) and is the same floor
    // used by the cube/sphere de-Rham kernel-dim companion tests.
    let physical_gap = if largest_kernel_abs > 0.0 {
        physical_band_floor / largest_kernel_abs
    } else {
        f64::INFINITY
    };
    eprintln!(
        "physical-band gap: physical[0] / largest |λ_kernel| = {:.3e} \
         (require ≥ 10)",
        physical_gap
    );
    assert!(
        physical_gap >= 10.0,
        "physical-band floor sits only {physical_gap:.3e}× above kernel ceiling; \
         expected ≥ 10× (kernel cluster bleeding into physical band suggests \
         ε scaling or PEC mask is wrong)"
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

    // 10. Quantitative acceptance (issue #69): each of the lowest 5
    //     physical FEM eigenvalues `k_fem = √λ` pairs to the closest
    //     analytic PEC-cavity root from `mie::merged_roots` within ≤ 15 %
    //     relative on `k`.
    //
    //     The analytic catalog covers `l ∈ [1, 4]` with `n_max = 3`
    //     radial orders per `(l, polarisation)` — well past the lowest
    //     5 FEM roots, which on the bundled 313-node fixture sit in the
    //     `k ∈ [1, 4]` band where the catalog is dense.
    //
    //     **Pairing rule**: closest-root by absolute `k` distance, not
    //     index-ordered. Two FEM eigenvalues are allowed to pair to the
    //     same analytic root (e.g. mesh-asymmetry splitting of a `2l+1`
    //     degenerate multiplet) — we do not enforce distinct pairings.
    //
    //     **Tolerance**: the 15 % bound is calibrated to the bundled
    //     coarse fixture (313 nodes / ~1226 tets); convergence-under-
    //     refinement is the deferred sub-issue. Do not tighten this
    //     bound in this ticket.
    let analytic = merged_roots(n_index, &[1, 2, 3, 4], R_SPHERE, R_BUFFER, 3);
    assert!(
        !analytic.is_empty(),
        "analytic PEC catalog produced no roots — merged_roots wiring broken"
    );
    eprintln!(
        "analytic PEC catalog: {} roots for n = {}, R_s = {}, R_b = {}",
        analytic.len(),
        n_index,
        R_SPHERE,
        R_BUFFER,
    );

    let rel_tol = 0.15_f64;
    for (i, &lam) in physical.iter().enumerate() {
        let k_fem = lam.sqrt();
        let closest = analytic
            .iter()
            .min_by(|a, b| {
                (a.k - k_fem)
                    .abs()
                    .partial_cmp(&(b.k - k_fem).abs())
                    .unwrap()
            })
            .expect("non-empty analytic catalog");
        let rel_err = (k_fem - closest.k).abs() / closest.k;
        eprintln!(
            "  physical[{i}]: k_fem = {k_fem:.4} → closest analytic {pol:?}_{l},{n} k = {ka:.4} \
             (rel err {err:.2}%)",
            pol = closest.pol,
            l = closest.l,
            n = closest.n,
            ka = closest.k,
            err = 100.0 * rel_err,
        );
        assert!(
            rel_err <= rel_tol,
            "physical[{i}] k_fem = {k_fem:.4} does not pair to any analytic PEC root within \
             {tol:.0}%: closest is {pol:?}_{l},{n} at k = {ka:.4} (rel err {err:.2}%)",
            tol = 100.0 * rel_tol,
            pol = closest.pol,
            l = closest.l,
            n = closest.n,
            ka = closest.k,
            err = 100.0 * rel_err,
        );
    }
}
