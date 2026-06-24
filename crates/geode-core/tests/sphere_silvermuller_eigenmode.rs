//! Silver-Müller absorbing-BC dielectric-sphere eigenmode integration
//! test (issue #27).
//!
//! Replaces the PEC outer wall from `tests/sphere_pec_eigenmode.rs`
//! with the first-order Silver-Müller impedance condition
//!
//! ```text
//! n × (∇ × E) = -j k₀ (n × n × E)
//! ```
//!
//! evaluated on the outer boundary triangles of the bundled sphere-in-
//! vacuum fixture (#25). The discrete generalized eigenproblem
//! becomes
//!
//! ```text
//! (K + j k₀ S) E = k² M E
//! ```
//!
//! and the eigenvalues `k²` are now complex: `Im(k²) > 0` for
//! radiating physical modes (positive Q with the convention
//! `Q = Re(k) / (2 Im(k))`).
//!
//! # Why no quantitative Mie comparison yet
//!
//! Authoritative dielectric-sphere Mie roots (the zeros of the
//! `J_{ℓ+1/2}` / `H^{(1)}_{ℓ+1/2}` boundary determinant) are not yet
//! tabulated in this codebase. Producing them is a separate
//! root-finding job tracked as a follow-up to the curator's #8 plan.
//!
//! This test therefore uses a **soft acceptance** appropriate for the
//! coarse fixture and a first cut of the absorbing BC:
//!
//! 1. The complex generalized eigensolver runs without panic in
//!    release mode on the full (un-eliminated) edge DOF system.
//! 2. The lowest few non-spurious modes have `Re(k²) > 0`, i.e. they
//!    are oscillatory rather than purely decaying.
//! 3. At least one of the lowest physical modes has a non-trivial
//!    imaginary part (radiation loss is present — the whole point).
//!
//! # k₀ choice (v0)
//!
//! A first-order Silver-Müller BC requires a real `k₀` parameter (the
//! "guess" wavenumber the impedance is matched to). For v0 we use
//! `k₀ ≈ 1.0` — within the same band as the PEC ground-mode wavenumber
//! from #26 (k ≈ 1.19) and the naive dielectric estimate `π/(n·R) ≈ 2.1`.
//! A self-consistent fixed-point iteration on `k₀ = Re(k_ground)`
//! belongs to a follow-up issue (PML / higher-order ABC).
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --release --test sphere_silvermuller_eigenmode -- --ignored
//! ```
//!
//! Marked `#[ignore]` because faer 0.24's gevd path panics under
//! debug-assertions (same root cause as `tests/eigensolver.rs` and
//! `tests/sphere_pec_eigenmode.rs`).

use burn::tensor::backend::BackendTypes;

use geode_core::{
    ComplexEigenSolver, DefaultBackend, FaerComplexEigensolver, PHYS_OUTER_BOUNDARY,
    assemble_global_nedelec_with_epsilon, assemble_silver_muller_surface, build_epsilon_r,
    burn_matrix_to_faer, read_sphere_fixture, sphere_n_interior_nodes, upload_mesh,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[test]
fn silver_muller_surface_is_nonzero_on_outer_boundary() {
    // Smoke: the surface matrix actually has mass on the outer boundary
    // edges and stays zero off them.
    let f = read_sphere_fixture().expect("fixture load");
    let edges = f.mesh.edges();
    let s = assemble_silver_muller_surface(
        &f.mesh,
        &f.boundary_triangles,
        &f.triangle_physical_tags,
        PHYS_OUTER_BOUNDARY,
        &edges,
    );

    let n = s.nrows();
    assert_eq!(n, edges.len(), "S dimension must match edge count");

    // Total Frobenius mass should be strictly positive.
    let mut frob = 0.0_f64;
    let mut max_asym = 0.0_f64;
    for i in 0..n {
        for j in 0..n {
            frob += s[(i, j)] * s[(i, j)];
            let d = (s[(i, j)] - s[(j, i)]).abs();
            if d > max_asym {
                max_asym = d;
            }
        }
    }
    assert!(frob.sqrt() > 0.0, "S has no mass on the outer boundary");
    assert!(
        max_asym < 1e-10,
        "S must be symmetric; max |S - Sᵀ| = {max_asym}"
    );
    eprintln!(
        "Silver-Müller S: |S|_F = {:.4e}, max |S - Sᵀ| = {:.2e}",
        frob.sqrt(),
        max_asym
    );
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn sphere_silver_muller_eigenmode_spectrum() {
    // 1. Load the sphere fixture.
    let f = read_sphere_fixture().expect("fixture load");
    eprintln!(
        "sphere fixture: {} nodes, {} tets, {} boundary triangles",
        f.mesh.n_nodes(),
        f.mesh.n_tets(),
        f.boundary_triangles.len(),
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

    // 4. Upload + assemble K, M with ε-scaled mass.
    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &epsilon_r,
    );

    // 5. Silver-Müller surface matrix on outer-boundary triangles.
    //    NOTE: unlike the PEC test, we do NOT eliminate any DOFs — the
    //    impedance BC is a natural boundary condition, not an
    //    essential one, so outer-wall edges remain in the system and
    //    carry the radiation condition.
    let s_full = assemble_silver_muller_surface(
        &f.mesh,
        &f.boundary_triangles,
        &f.triangle_physical_tags,
        PHYS_OUTER_BOUNDARY,
        &edges,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);

    // 6. Solve the complex generalized eigenproblem.
    //
    //    We ask for enough eigenvalues to skip past the spurious
    //    gradient nullspace and grab the lowest physical modes. Using
    //    the same n_interior_nodes prediction as the PEC test, but
    //    bumped by the outer-wall node count since those DOFs are no
    //    longer eliminated (so the gradient kernel is correspondingly
    //    larger). Conservatively request 2× the interior-node count.
    let spurious_lower_bound = sphere_n_interior_nodes(&f.mesh, geode_core::R_BUFFER);
    let n_request = (spurious_lower_bound * 2).max(20);
    eprintln!(
        "PEC-equivalent spurious lower bound: {spurious_lower_bound}, requesting {n_request} eigenvalues"
    );

    // k₀ ≈ 1.0 — see module docs for rationale.
    let k0 = 1.0_f64;
    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            k0,
            n_request,
        )
        .expect("complex eigensolve");

    eprintln!(
        "lowest {} complex eigenvalues λ = k² (sorted by |Re(λ)|):",
        lambdas.len()
    );
    for (i, lam) in lambdas.iter().enumerate() {
        eprintln!("  λ[{i:>3}] = {:.4e} + {:.4e}i", lam.re, lam.im,);
    }

    // 7. Spurious-mode filter. The Whitney 1-form basis carries a
    //    huge gradient kernel: gradients of any H¹ scalar field are
    //    curl-free, so the discrete `K` has roughly
    //    `n_interior_nodes`-dimensional nullspace. With the Silver-
    //    Müller term these modes don't vanish exactly — they pick up
    //    tiny `j k₀ S` perturbations — but they cluster orders of
    //    magnitude below the first physical eigenvalue.
    //
    //    Detect the cluster as: contiguous block at the start of the
    //    by-|Re|-sorted list whose |λ| is below a fraction (1e-3) of
    //    the largest |λ| in the slice. The first index past this
    //    block is the first physical mode.
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
        "max |λ| in slice = {max_abs:.3e}, spurious threshold {spurious_threshold:.3e}, \
         {n_spurious} spurious modes (lower bound prediction: {spurious_lower_bound})"
    );

    // Acceptance 1: the spurious-mode count should be at least the
    // PEC-equivalent gradient-kernel lower bound (the gradient kernel
    // gets BIGGER without PEC elimination, never smaller).
    assert!(
        n_spurious >= spurious_lower_bound,
        "observed spurious count {n_spurious} below predicted floor \
         {spurious_lower_bound} — spectral gap detection or scaling is off"
    );

    // 8. Physical modes — first few above the cluster.
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

    // Acceptance 2: physical modes have strictly positive Re(λ) — they
    // are oscillatory in space, not pure decay. (Re(λ) = Re(k²) > 0
    // ⇒ k has a real component.)
    let n_oscillatory = physical.iter().filter(|(_, l)| l.re > 0.0).count();
    assert!(
        n_oscillatory > 0,
        "no oscillatory mode (Re > 0) in the first 5 physical candidates — \
         either ε scaling or the impedance sign is wrong"
    );

    // Acceptance 3: radiation loss is present.
    //    At least one of those modes must have non-trivial Im(λ).
    //    For a coarse mesh + first-order ABC we use a loose threshold
    //    (1e-3 relative to |λ|), well above f64 noise.
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
        "no radiating mode (all Im(λ) ≈ 0) — Silver-Müller term is not coupling in"
    );

    // 9. Diagnostic: dump Q factors for the first few physical modes.
    //    k = sqrt(λ) with Re(k) > 0 branch. Outgoing-wave convention:
    //    Im(k) > 0 → exponential decay in time (positive Q).
    eprintln!("\nQ-factor diagnostic for first 5 physical modes:");
    for (i, lam) in physical.iter() {
        // k = sqrt(λ) with Re(k) > 0 branch.
        let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
        let re_k = ((r + lam.re) / 2.0).sqrt();
        // Im(k) sign follows sign of Im(λ) for the Re(k) > 0 branch.
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
    }

    eprintln!(
        "\nsoft acceptance: solver returned {} eigenvalues, \
         {} spurious near zero, {} physical inspected, {} radiating",
        lambdas.len(),
        n_spurious,
        physical.len(),
        n_radiating
    );
}
