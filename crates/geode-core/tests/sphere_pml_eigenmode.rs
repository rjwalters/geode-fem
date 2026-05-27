//! Scalar-PML dielectric-sphere eigenmode integration test (issues
//! #28, #38).
//!
//! Replaces the Silver-Müller absorbing boundary (#27) with a scalar
//! perfectly-matched-layer (PML) realized as a complex permittivity in
//! the outer absorbing shell of the bundled sphere fixture.
//!
//! # Approach (issue #38 hardening)
//!
//! The fixture has three nested regions:
//!
//!   - `sphere_interior`  (`r ≤ R_SPHERE`)        — dielectric, ε = n²
//!   - `vacuum_gap`       (`R_SPHERE < r ≤ R_PML_INNER`) — real vacuum
//!   - `pml_shell`        (`R_PML_INNER < r ≤ R_BUFFER`) — absorbing PML
//!
//! The vacuum gap provides un-stretched space for outgoing waves to
//! propagate before reaching the absorbing layer; the outer wall is
//! PEC and is essentially unreachable for well-trapped modes. The PML
//! quadratic ramp is anchored at `R_PML_INNER`, not `R_SPHERE`, so the
//! lossy region no longer abuts the dielectric.
//!
//! ```text
//! ε_r(r) = 1 − j σ₀ ((r − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²
//! ```
//!
//! This is **less effective** than a fully anisotropic split-field PML
//! (the tangential field components do not see the absorption) but it
//! requires no constitutive-tensor refactor and is a defensible
//! starting point. See [`geode_core::build_complex_epsilon_r_pml`] for
//! the profile and sign-convention discussion.
//!
//! # Acceptance (soft)
//!
//! 1. The complex generalized eigensolver runs without panic in release
//!    mode on the full (PEC-reduced) edge system with complex M.
//! 2. Spurious gradient modes still cluster near zero — the gradient
//!    kernel survives the lossy-ε scaling because gradients of H¹_0
//!    are not amplified or suppressed by `ε(x)` scalar scaling on the
//!    mass.
//! 3. At least one of the lowest physical modes has non-trivial Im(λ)
//!    (the PML absorbs radiation — that's the whole point).
//! 4. Q-factor of the lowest physical mode is computed and printed; the
//!    Silver-Müller baseline from #27 reported Q ≈ 0.5 for the same
//!    fixture. We do **not** hard-assert "Q > Silver-Müller" — see the
//!    PR body for the negative-result discussion if the absorption is
//!    weaker than hoped.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --release --test sphere_pml_eigenmode -- --ignored
//! ```
//!
//! Marked `#[ignore]` because faer 0.24's `gevd` path panics under
//! `debug-assertions` (same root cause as the PEC and Silver-Müller
//! tests).

use burn::tensor::backend::BackendTypes;

use geode_core::{
    apply_dirichlet_bc, assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, burn_matrix_to_faer, read_sphere_fixture, sphere_n_interior_nodes,
    sphere_pec_interior_edges, tet_centroid_radii, upload_mesh, ComplexEigenSolver, DefaultBackend,
    FaerComplexEigensolver, R_BUFFER, R_PML_INNER, R_SPHERE,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[test]
fn pml_profile_is_real_inside_imag_in_buffer() {
    // Smoke: the PML profile produces real ε in the dielectric and in
    // the real-vacuum gap, and a strictly negative imaginary part in
    // the absorbing PML shell (using the exp(+jωt) convention).
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 5.0);

    assert_eq!(eps.len(), f.mesh.n_tets());

    let n_interior_real_only = eps
        .iter()
        .zip(f.tet_physical_tags.iter())
        .filter(|(c, &t)| {
            t == geode_core::PHYS_SPHERE_INTERIOR
                && c.im.abs() < 1e-12
                && (c.re - 2.25).abs() < 1e-9
        })
        .count();
    assert_eq!(
        n_interior_real_only,
        f.n_interior_tets(),
        "all interior tets must have real ε = 2.25"
    );

    // Vacuum-gap tets must carry exactly ε = 1 + 0j (real vacuum).
    let n_gap_real_one = eps
        .iter()
        .zip(f.tet_physical_tags.iter())
        .filter(|(c, &t)| {
            t == geode_core::PHYS_VACUUM_GAP && c.im.abs() < 1e-12 && (c.re - 1.0).abs() < 1e-12
        })
        .count();
    assert_eq!(
        n_gap_real_one,
        f.n_vacuum_gap_tets(),
        "all vacuum-gap tets must have ε = 1 + 0j (no absorption inside the gap)"
    );

    // PML-shell tets must carry strictly-negative Im(ε); Re(ε) = 1.
    let n_pml_lossy = eps
        .iter()
        .zip(f.tet_physical_tags.iter())
        .filter(|(c, &t)| t == geode_core::PHYS_PML_SHELL && c.im < 0.0)
        .count();
    assert_eq!(
        n_pml_lossy,
        f.n_pml_shell_tets(),
        "every PML-shell tet must carry strictly-negative Im(ε)"
    );
    for (c, &t) in eps.iter().zip(f.tet_physical_tags.iter()) {
        if t == geode_core::PHYS_PML_SHELL {
            assert!(
                (c.re - 1.0).abs() < 1e-12,
                "PML-shell tet has Re(ε) = {} (expected 1)",
                c.re
            );
        }
    }
    eprintln!(
        "PML profile: {} interior tets at ε = 2.25 + 0j, {} gap tets at ε = 1 + 0j, \
         {} PML-shell tets with Im(ε) < 0",
        n_interior_real_only, n_gap_real_one, n_pml_lossy
    );
}

#[test]
fn pml_profile_sigma_zero_is_real_everywhere() {
    // Regression test for the σ₀ = 0 limit (issue #38):
    //
    // Setting σ₀ = 0 should reduce the complex-ε PML pipeline to a
    // **real** dielectric problem (real ε = n² inside the sphere,
    // ε = 1 in both the vacuum gap and the PML shell). Any non-zero
    // imaginary part here would indicate a regression in the complex-ε
    // plumbing (e.g. an off-by-one in the ramp coordinate that leaks
    // imaginary content even at zero absorption strength).
    let f = read_sphere_fixture().expect("fixture load");
    let radii = tet_centroid_radii(&f.mesh);
    let eps = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 0.0);
    assert_eq!(eps.len(), f.mesh.n_tets());

    for (i, c) in eps.iter().enumerate() {
        assert_eq!(
            c.im, 0.0,
            "σ₀ = 0 should yield exactly real ε; tet {i} has Im(ε) = {} \
             (physical tag = {})",
            c.im, f.tet_physical_tags[i]
        );
    }

    // Spot-check the real part: dielectric tets at 2.25, everything
    // else at 1.0.
    for (i, c) in eps.iter().enumerate() {
        let expected_re = if f.tet_physical_tags[i] == geode_core::PHYS_SPHERE_INTERIOR {
            2.25
        } else {
            1.0
        };
        assert!(
            (c.re - expected_re).abs() < 1e-12,
            "tet {i} σ₀=0 Re(ε) = {} (expected {expected_re})",
            c.re
        );
    }
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn sphere_pml_eigenmode_spectrum() {
    // 1. Load the sphere fixture.
    let f = read_sphere_fixture().expect("fixture load");
    eprintln!(
        "sphere fixture: {} nodes, {} tets, {} boundary triangles",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.boundary_triangles.len(),
    );

    // 2. PML profile: real ε = n² in the dielectric, quadratic absorption
    //    ramp in the buffer. σ₀ = 5.0 is a starting absorption strength;
    //    tuning belongs to a follow-up.
    let n_index = 1.5_f64;
    let sigma_0 = 5.0_f64;
    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, n_index, sigma_0);
    eprintln!(
        "PML profile: dielectric ε = {} + 0j, max |Im(ε)| in buffer = {}",
        n_index * n_index,
        sigma_0,
    );

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

    // 4. Upload + assemble K (real) and complex M.
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex,
    );

    // 5. PEC outer-wall reduction. The PML does the absorbing; the
    //    outer wall is PEC. Reuse the existing real-only mask helper
    //    and apply it to K, Re(M), Im(M) independently before
    //    recombining.
    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "edge ordering mismatch");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();
    eprintln!(
        "PEC reduction: {} edges → {} interior DOFs",
        n_edges, n_interior_edges
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // Reduce K (real) and the two halves of M individually so we can
    // re-zip them after; faer 0.24's gevd needs the full pencil at
    // call time anyway.
    // Use the existing real-only Dirichlet reduction on K; do the
    // analogous slice on M_complex by hand using the same mask.
    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("BC reduction K");

    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    // 6. Solve the complex generalized eigenproblem.
    //    Request enough eigenvalues to skip past the gradient nullspace
    //    plus the lowest few physical modes.
    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 10;
    eprintln!("predicted spurious-mode count: {spurious_dim}, requesting {n_request} eigenvalues",);

    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

    eprintln!(
        "lowest {} complex eigenvalues λ = k² (sorted by |Re(λ)|):",
        lambdas.len()
    );
    for (i, lam) in lambdas.iter().enumerate() {
        eprintln!("  λ[{i:>3}] = {:.4e} + {:.4e}i", lam.re, lam.im);
    }

    // 7. Spurious-mode filter. The gradient kernel of the Whitney
    //    1-form basis is independent of the mass scaling, so we expect
    //    the same `spurious_dim`-sized cluster as the PEC test (modulo
    //    tiny imaginary perturbations from the lossy ε scaling on the
    //    mass-matrix gradient block).
    //
    //    Detect by relative-magnitude jump: anything below 1e-3 of the
    //    largest |λ| in the requested slice is treated as spurious.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one mode above the spurious threshold");
    let n_spurious = first_physical;
    eprintln!(
        "max |λ| = {max_abs:.3e}, spurious threshold {spurious_threshold:.3e}, \
         {n_spurious} spurious modes (predicted ≥ {spurious_dim})"
    );

    // Acceptance 1: spurious cluster still present.
    assert!(
        n_spurious >= spurious_dim,
        "observed spurious count {n_spurious} below predicted floor \
         {spurious_dim} — the gradient kernel was unexpectedly lifted by the PML"
    );

    // 8. Physical modes — first few above the spurious cluster.
    let physical: Vec<(usize, faer::c64)> = lambdas
        .iter()
        .enumerate()
        .skip(first_physical)
        .take(5)
        .map(|(i, l)| (i, *l))
        .collect();
    eprintln!("first physical-mode candidates:");
    for (i, lam) in physical.iter() {
        eprintln!("  λ[{i:>3}] = {:.4e} + {:.4e}i", lam.re, lam.im);
    }

    // Acceptance 2: at least one oscillatory mode (Re(λ) > 0).
    let n_oscillatory = physical.iter().filter(|(_, l)| l.re > 0.0).count();
    assert!(
        n_oscillatory > 0,
        "no oscillatory mode (Re > 0) in first 5 physical candidates — \
         the PML profile may be over-absorbing"
    );

    // Acceptance 3: PML is absorbing — at least one mode has Im(λ) ≠ 0.
    let n_radiating = physical
        .iter()
        .filter(|(_, l)| l.im.abs() > 1e-3 * l.re.hypot(l.im).max(1.0))
        .count();
    eprintln!(
        "radiating modes among first 5 physical: {} / {}",
        n_radiating,
        physical.len()
    );
    assert!(
        n_radiating > 0,
        "no radiating mode (all Im(λ) ≈ 0) — PML is not coupling in"
    );

    // 9. Q-factor diagnostic for the first 5 physical modes.
    //    k = sqrt(λ) with Re(k) > 0 branch.
    eprintln!(
        "\nQ-factor diagnostic (PML, σ₀ = {sigma_0}). \
         Silver-Müller baseline from #27 had Q ≈ 0.5 for the ground mode."
    );
    let mut best_q: Option<f64> = None;
    for (i, lam) in physical.iter() {
        let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
        let re_k = ((r + lam.re) / 2.0).sqrt();
        let im_k_mag = ((r - lam.re) / 2.0).sqrt();
        let im_k = if lam.im >= 0.0 { im_k_mag } else { -im_k_mag };
        let q = if im_k.abs() > 1e-12 {
            re_k / (2.0 * im_k.abs())
        } else {
            f64::INFINITY
        };
        eprintln!(
            "  λ[{i:>3}] = {:.4e} + {:.4e}i → k = {:.4} + {:.4e}i → Q = {:.3e}",
            lam.re, lam.im, re_k, im_k, q
        );
        if q.is_finite() {
            best_q = match best_q {
                Some(prev) if prev > q => Some(prev),
                _ => Some(q),
            };
        }
    }

    eprintln!(
        "\nsoft acceptance: {} spurious modes near zero, {} physical inspected, {} radiating",
        n_spurious,
        physical.len(),
        n_radiating
    );
    if let Some(q) = best_q {
        eprintln!(
            "best Q among first 5 physical modes: {:.3e}. \
             Silver-Müller #27 reference: ≈ 0.5.",
            q
        );
    }

    // Sanity: the inner sphere has roughly R_SPHERE = 1 and refractive
    // index n = 1.5, so a naive dielectric Mie mode lies near
    // k ≈ π / (n·R) ≈ 2.1. With the PEC outer wall on a coarse mesh
    // the ground mode wanders, but values outside [0.1, 20] would
    // indicate a gross scaling failure.
    let k_ground_re_sq = physical[0].1.re;
    assert!(
        (0.01..=400.0).contains(&k_ground_re_sq),
        "ground Re(k²) = {k_ground_re_sq:.4} outside the plausibility band [0.01, 400]"
    );

    // Sanity: the radii used by the PML profile are exactly the ones
    // baked into the fixture's `mesh/sphere.rs` constants. Both
    // constants come from the same module so this is really a self-
    // consistency reminder for anyone editing either.
    eprintln!(
        "fixture invariants: R_SPHERE = {R_SPHERE}, R_PML_INNER = {R_PML_INNER}, \
         R_BUFFER = {R_BUFFER} (vacuum gap = {}, PML thickness = {})",
        R_PML_INNER - R_SPHERE,
        R_BUFFER - R_PML_INNER,
    );
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn sphere_pml_eigenmode_sigma_zero_is_real() {
    // Issue #38 regression test: when σ₀ = 0 the complex-ε pipeline
    // collapses to a real-ε generalized eigenproblem (dielectric inside
    // a PEC cavity), so all eigenvalues must be real to f64 precision.
    //
    // Any non-zero imaginary content here indicates a regression in the
    // complex-ε plumbing (e.g. a rounding accumulation in the imag-
    // mass scatter, or a sign flip in one of the assembly halves).
    let f = read_sphere_fixture().expect("fixture load");
    let n_index = 1.5_f64;
    let sigma_0 = 0.0_f64;
    let radii = tet_centroid_radii(&f.mesh);
    let eps_complex = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, n_index, sigma_0);

    // Sanity: profile is real before we even touch the assembler.
    for (i, c) in eps_complex.iter().enumerate() {
        assert_eq!(
            c.im, 0.0,
            "σ₀ = 0 → ε must be real at the profile level; tet {i} im = {}",
            c.im
        );
    }

    let tet_edges = f.mesh.tet_edges();
    let n_edges = f.mesh.edges().len();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // Acceptance A: the assembled M_im must be (numerically) zero, not
    // just per-element zero — this catches a regression where the
    // scatter accumulates noisy bookkeeping into Im(M).
    let n = m_complex_full.nrows();
    let mut max_abs_im = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            let v = m_complex_full[(i, j)].im.abs();
            if v > max_abs_im {
                max_abs_im = v;
            }
        }
    }
    assert!(
        max_abs_im < 1e-12,
        "σ₀ = 0: assembled Im(M) leaked, max |Im(M_ij)| = {max_abs_im:.3e} (expected 0)"
    );

    // Now run the full eigensolver as a belt-and-braces check.
    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "edge ordering mismatch");

    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("BC reduction K");

    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int_complex = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_complex_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_complex =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));

    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 5;

    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(
            k_int_complex.as_ref(),
            m_int_complex.as_ref(),
            n_request,
        )
        .expect("complex eigensolve");

    // Acceptance B: every eigenvalue must be real (Im(λ) tiny relative
    // to |Re(λ)|). The complex eigensolver itself does not enforce a
    // real spectrum even for Hermitian inputs, so a small numerical
    // tolerance is appropriate.
    let mut max_relative_im = 0.0_f64;
    for (i, lam) in lambdas.iter().enumerate() {
        let scale = lam.re.abs().max(1.0);
        let rel = lam.im.abs() / scale;
        if rel > max_relative_im {
            max_relative_im = rel;
        }
        eprintln!(
            "  λ[{i:>3}] = {:.4e} + {:.4e}i  (|Im/Re| ≤ {:.2e})",
            lam.re, lam.im, rel
        );
    }
    eprintln!(
        "σ₀ = 0 spectrum: max |Im(λ)| / max(|Re(λ)|, 1) = {max_relative_im:.3e} \
         (over {} eigenvalues)",
        lambdas.len()
    );

    // Real-spectrum tolerance: in practice the observed
    // `max |Im(λ)| / max(|Re(λ)|, 1)` runs at ~2e-13 — small
    // eigenvalues sit at f64 machine epsilon (~1e-16) while the
    // largest eigenvalues (|λ| ~ 3) carry imaginary noise around
    // ~1e-13 from f64 accumulation in the eigensolve. The complex
    // pencil collapses to a real one when σ₀ = 0, so the lack of
    // systematic imaginary content is the property we want to
    // guard. We bound at 1e-10 — three orders of magnitude above
    // the observed floor — to catch any regression that introduces
    // *systematic* imaginary content (much tighter than the old
    // 1e-5 bound, which would not have flagged a non-trivial
    // Im(M) leak).
    assert!(
        max_relative_im < 1e-10,
        "σ₀ = 0 spectrum should be real to f64 precision; \
         observed max |Im(λ)|/|Re(λ)| = {max_relative_im:.3e}"
    );
}
