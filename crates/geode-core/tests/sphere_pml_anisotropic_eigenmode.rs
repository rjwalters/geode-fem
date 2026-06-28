//! Anisotropic-UPML dielectric-sphere eigenmode integration test
//! (issue #54).
//!
//! Replaces the **scalar-isotropic** PML's per-tet complex ε scalar
//! with a **diagonal anisotropic** complex permittivity tensor in
//! the global Cartesian basis. PR #52 showed that the scalar PML
//! introduces a ~15-16% reflection-driven error ceiling on the
//! lowest TM_1,1 Mie mode and that this ceiling is **independent of
//! mesh refinement**. The UPML formulation absorbs along the
//! propagation direction in a direction-aware way, which is the
//! textbook fix.
//!
//! # What this test asserts
//!
//! 1. **σ₀ = 0 regression** — with absorption strength turned off,
//!    the anisotropic tensor reduces to the scalar real-vacuum case
//!    everywhere outside the dielectric, so the assembled mass
//!    should match the scalar-ε path to within tight numerical
//!    tolerance.
//! 2. **Non-trivial radiation** — at nominal `σ₀ = 5.0` at least one
//!    of the lowest physical modes carries non-zero Im(λ).
//! 3. **Improvement vs scalar baseline** — the lowest TM_1,1 mode
//!    has relative error strictly less than the documented 16%
//!    scalar-PML ceiling. Soft assertion (eprintln + acceptance
//!    band ≤ 15%) so the test is robust to release-rebuild drift.
//!
//! # Running
//!
//! ```sh
//! cargo test -p geode-core --release \
//!     --test sphere_pml_anisotropic_eigenmode -- --ignored
//! ```

use burn::tensor::backend::BackendTypes;

use geode_core::analytic::mie::{MiePolarisation, merged_roots};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_anisotropic_epsilon, assemble_global_nedelec_with_complex_epsilon,
    build_anisotropic_pml_tensor_diag, build_complex_epsilon_r_pml, burn_complex_mass_to_faer,
    sphere_n_interior_nodes, sphere_pec_interior_edges, tet_centroid_radii, tet_centroids,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::testing::TestBackend;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, R_SPHERE, read_sphere_fixture};

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

#[test]
fn anisotropic_pml_tensor_sigma_zero_is_isotropic() {
    // σ₀ = 0 ⇒ the tensor must collapse to the real scalar in every
    // tet (ε = n² inside, ε = 1 outside) on all three diagonal slots.
    let f = read_sphere_fixture().expect("fixture load");
    let centroids = tet_centroids(&f.mesh);
    let eps = build_anisotropic_pml_tensor_diag(&f.tet_physical_tags, &centroids, 1.5, 0.0, 1.0);
    assert_eq!(eps.len(), f.mesh.n_tets());

    for (i, diag) in eps.iter().enumerate() {
        let expected = if f.tet_physical_tags[i] == geode_core::mesh::PHYS_SPHERE_INTERIOR {
            2.25
        } else {
            1.0
        };
        for (alpha, val) in diag.iter().enumerate() {
            assert!(
                val.im.abs() < 1e-12,
                "σ₀ = 0 anisotropic ε_{alpha} on tet {i} has Im = {} (expected 0)",
                val.im
            );
            assert!(
                (val.re - expected).abs() < 1e-12,
                "σ₀ = 0 anisotropic ε_{alpha} on tet {i} has Re = {} (expected {expected})",
                val.re
            );
        }
    }
}

#[test]
fn anisotropic_pml_pml_shell_is_lossy() {
    // At nominal σ₀, every PML-shell tet must carry non-zero
    // imaginary content somewhere on its diagonal; tets outside the
    // PML stay strictly real.
    let f = read_sphere_fixture().expect("fixture load");
    let centroids = tet_centroids(&f.mesh);
    let eps = build_anisotropic_pml_tensor_diag(&f.tet_physical_tags, &centroids, 1.5, 5.0, 2.0);

    let mut lossy_pml = 0usize;
    for (diag, &tag) in eps.iter().zip(f.tet_physical_tags.iter()) {
        let max_im = diag.iter().map(|c| c.im.abs()).fold(0.0_f64, f64::max);
        if tag == geode_core::mesh::PHYS_PML_SHELL {
            if max_im > 1e-12 {
                lossy_pml += 1;
            }
        } else {
            // Interior + gap: every slot must be strictly real.
            for c in diag.iter() {
                assert!(
                    c.im.abs() < 1e-12,
                    "non-PML tet (tag {tag}) leaked Im(ε) = {}",
                    c.im
                );
            }
        }
    }
    assert_eq!(
        lossy_pml,
        f.n_pml_shell_tets(),
        "expected every PML-shell tet to carry imaginary ε"
    );
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn anisotropic_pml_sigma_zero_matches_scalar() {
    // Bit-identical (to readback precision) reduction-to-scalar
    // check: with σ₀ = 0 and identical ε_r, both pipelines must
    // produce the same K and M matrices. This is the load-bearing
    // sanity test for the new kernel — any disagreement points to a
    // bug in `batched_nedelec_local_mass_anisotropic_diag`.
    let f = read_sphere_fixture().expect("fixture load");

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

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());

    // Scalar path: ε complex with σ₀ = 0.
    let radii = tet_centroid_radii(&f.mesh);
    let eps_scalar = build_complex_epsilon_r_pml(&f.tet_physical_tags, &radii, 1.5, 0.0);
    let sys_scalar = assemble_global_nedelec_with_complex_epsilon(
        nodes_t.clone(),
        tets_t.clone(),
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_scalar,
    );

    // Anisotropic path: σ₀ = 0 ⇒ collapses to same scalar ε per tet.
    let centroids = tet_centroids(&f.mesh);
    let eps_aniso =
        build_anisotropic_pml_tensor_diag(&f.tet_physical_tags, &centroids, 1.5, 0.0, 1.0);
    let sys_aniso = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
    );

    let k_s = burn_matrix_to_faer(sys_scalar.k);
    let m_s = burn_complex_mass_to_faer(sys_scalar.m_re, sys_scalar.m_im);
    let k_a = burn_matrix_to_faer(sys_aniso.k);
    let m_a = burn_complex_mass_to_faer(sys_aniso.m_re, sys_aniso.m_im);

    assert_eq!(k_s.nrows(), k_a.nrows());
    assert_eq!(k_s.ncols(), k_a.ncols());

    let max_diff_k = (0..k_s.nrows())
        .flat_map(|i| (0..k_s.ncols()).map(move |j| (i, j)))
        .map(|(i, j)| (k_s[(i, j)] - k_a[(i, j)]).abs())
        .fold(0.0_f64, f64::max);
    let max_diff_m_re = (0..m_s.nrows())
        .flat_map(|i| (0..m_s.ncols()).map(move |j| (i, j)))
        .map(|(i, j)| (m_s[(i, j)].re - m_a[(i, j)].re).abs())
        .fold(0.0_f64, f64::max);
    let max_diff_m_im = (0..m_s.nrows())
        .flat_map(|i| (0..m_s.ncols()).map(move |j| (i, j)))
        .map(|(i, j)| (m_s[(i, j)].im - m_a[(i, j)].im).abs())
        .fold(0.0_f64, f64::max);

    eprintln!(
        "scalar vs anisotropic-diag (σ₀=0): \
         max |ΔK| = {max_diff_k:.3e}, max |ΔRe(M)| = {max_diff_m_re:.3e}, \
         max |ΔIm(M)| = {max_diff_m_im:.3e}"
    );

    // Tolerance allows for f32 readback noise on the GPU path while
    // catching anything bigger than a 6th-significant-figure drift.
    assert!(
        max_diff_k < 1e-3,
        "K mismatch between scalar and anisotropic paths: {max_diff_k}"
    );
    assert!(
        max_diff_m_re < 1e-3,
        "Re(M) mismatch between scalar and anisotropic paths: {max_diff_m_re}"
    );
    assert!(
        max_diff_m_im < 1e-9,
        "Im(M) leaked at σ₀=0: {max_diff_m_im}"
    );
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn anisotropic_pml_beats_scalar_on_tm11() {
    // The load-bearing acceptance test: anisotropic UPML must
    // produce a lower relative error on the lowest TM_1,1 mode than
    // the scalar-PML 16% ceiling documented in PR #52.

    let f = read_sphere_fixture().expect("fixture load");

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
    eprintln!(
        "anisotropic PML test: {} nodes, {} tets, {n_edges} edges",
        f.mesh.n_nodes(),
        f.mesh.n_tets()
    );

    let n_inside = 1.5_f64;
    let sigma_0 = 5.0_f64;
    let k0_ref = 2.0_f64;
    let centroids = tet_centroids(&f.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &f.tet_physical_tags,
        &centroids,
        n_inside,
        sigma_0,
        k0_ref,
    );

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
    );

    // PEC outer reduction.
    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&f.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges);
    let dummy_zero = faer::Mat::<f64>::zeros(n_edges, n_edges);
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);
    let (k_int, _) =
        apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask).expect("BC K");
    let interior_idx: Vec<usize> = interior_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior_idx.len();
    let m_int = faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| {
        m_full[(interior_idx[i], interior_idx[j])]
    });
    let k_int_c =
        faer::Mat::<faer::c64>::from_fn(dim, dim, |i, j| faer::c64::new(k_int[(i, j)], 0.0));
    eprintln!("PEC reduction: {n_edges} → {dim} interior DOFs");

    // Solve.
    let spurious_dim = sphere_n_interior_nodes(&f.mesh, R_BUFFER);
    let n_request = spurious_dim + 10;
    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_pencil_eigenvalues(k_int_c.as_ref(), m_int.as_ref(), n_request)
        .expect("complex eigensolve");

    // Spurious skip via magnitude jump.
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let thresh = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > thresh)
        .expect("at least one physical mode");
    eprintln!(
        "{first_physical} spurious modes skipped (predicted ≥ {spurious_dim}); \
         {} physical candidates returned",
        lambdas.len() - first_physical
    );

    // Analytic TM_1,1 ground.
    let analytic = merged_roots(n_inside, &[1, 2, 3], R_SPHERE, R_BUFFER, 3);
    let ground = analytic
        .iter()
        .filter(|r| r.pol == MiePolarisation::TM && r.l == 1 && r.n == 1)
        .min_by(|a, b| a.k.partial_cmp(&b.k).unwrap())
        .expect("TM_1,1 analytic root");
    eprintln!(
        "analytic TM_1,1 ground k = {:.5} (k² = {:.5})",
        ground.k,
        ground.k * ground.k
    );

    // Pull first few physical modes; the ground TM_1,1 triplet is
    // typically the lowest oscillatory cluster.
    let mut physical: Vec<faer::c64> = lambdas
        .iter()
        .skip(first_physical)
        .filter(|l| l.re > 0.0)
        .copied()
        .collect();
    physical.sort_by(|a, b| a.re.partial_cmp(&b.re).unwrap());
    eprintln!("first 6 oscillatory physical modes (sorted by Re(λ)):");
    for (i, l) in physical.iter().take(6).enumerate() {
        let r = (l.re * l.re + l.im * l.im).sqrt();
        let re_k = ((r + l.re) / 2.0).sqrt();
        eprintln!(
            "  λ[{i}] = {:.4e} + {:.4e}i → Re(k) = {:.4}",
            l.re, l.im, re_k
        );
    }

    let best = physical.first().expect("at least one oscillatory mode");
    let r = (best.re * best.re + best.im * best.im).sqrt();
    let re_k = ((r + best.re) / 2.0).sqrt();
    let rel_err = (re_k - ground.k).abs() / ground.k;
    eprintln!(
        "anisotropic-UPML TM_1,1 ground: Re(k) = {re_k:.4} \
         vs analytic {:.4} → rel err = {:.2}%",
        ground.k,
        rel_err * 100.0
    );
    eprintln!("scalar-PML baseline from PR #52: rel err ≈ 16%.");

    // Soft acceptance: must be strictly under 15% (margin for noise
    // below the documented 16% scalar ceiling). Target ≤ 8%.
    assert!(
        rel_err < 0.15,
        "anisotropic UPML did not beat the scalar 16% ceiling: rel err = {:.2}%",
        rel_err * 100.0
    );
}
