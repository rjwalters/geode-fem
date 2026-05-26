//! Scalar-PML dielectric-sphere eigenmode integration test (issue #28).
//!
//! Replaces the Silver-Müller absorbing boundary (#27) with a scalar
//! perfectly-matched-layer (PML) realized as a complex permittivity in
//! the existing `vacuum_buffer` region of the bundled sphere fixture.
//!
//! # Approach (scope-reduction Option 2)
//!
//! For this v0 cut we reuse the existing fixture topology — no new mesh
//! is generated — and treat the entire vacuum buffer (`R_SPHERE < r ≤
//! R_BUFFER`) as the PML. The outer wall stays PEC; the PML absorbs
//! outgoing radiation **before** it reaches the wall, so the PEC
//! boundary condition is essentially unreachable for well-trapped
//! modes.
//!
//! The PML is a UPML reduced to a scalar isotropic complex ε via the
//! standard quadratic absorption ramp,
//!
//! ```text
//! ε_r(r) = 1 − j σ₀ ((r − R_SPHERE) / (R_BUFFER − R_SPHERE))²
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
    FaerComplexEigensolver, R_BUFFER, R_SPHERE,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[test]
fn pml_profile_is_real_inside_imag_in_buffer() {
    // Smoke: the PML profile produces real ε in the dielectric region
    // and a strictly negative imaginary part in the vacuum buffer
    // (using the exp(+jωt) convention).
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
    let n_buffer = f
        .tet_physical_tags
        .iter()
        .filter(|&&t| t == geode_core::PHYS_VACUUM_BUFFER)
        .count();
    assert_eq!(
        n_interior_real_only,
        f.n_interior_tets(),
        "all interior tets must have real ε = 2.25"
    );

    let n_lossy = eps
        .iter()
        .zip(f.tet_physical_tags.iter())
        .filter(|(c, &t)| t == geode_core::PHYS_VACUUM_BUFFER && c.im < 0.0)
        .count();
    assert_eq!(
        n_lossy, n_buffer,
        "every buffer tet must carry strictly-negative Im(ε)"
    );

    // Re(ε) in the buffer is exactly 1 by construction.
    for (c, &t) in eps.iter().zip(f.tet_physical_tags.iter()) {
        if t == geode_core::PHYS_VACUUM_BUFFER {
            assert!(
                (c.re - 1.0).abs() < 1e-12,
                "buffer tet has Re(ε) = {} (expected 1)",
                c.re
            );
        }
    }
    eprintln!(
        "PML profile: {} interior tets at ε = 2.25 + 0j, {} buffer tets with Im(ε) < 0",
        n_interior_real_only, n_lossy
    );
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

    // Sanity: R_SPHERE matches the fixture convention used by the
    // PML profile. Both constants come from the same module so this is
    // really a self-consistency reminder for anyone editing either.
    eprintln!(
        "fixture invariants: R_SPHERE = {R_SPHERE}, R_BUFFER = {R_BUFFER} \
         (PML thickness = {})",
        R_BUFFER - R_SPHERE,
    );
}
