//! Cross-backend anisotropic-UPML dielectric-sphere Mie agreement test:
//! Burn vs the TF-Java reference (issue #174, Epic #88 Phase J.5).
//!
//! Loads `reference/fixtures/sphere_mie_small/tfjava_baseline.json` — the
//! TF-Java port of the Phase J.2 NumPy Mie pipeline
//! (`reference/tf_java/sphere_mie`: per-tet diagonal complex tensor ε
//! carried as two parallel `[nTets, 3]` f64 placeholders through the
//! static graph since TF-Java 1.0.0 has no native c128 typed value,
//! `Re(M)` / `Im(M)` emitted via parallel `scatterNd` calls, fused into
//! a complex128 pencil by `reference/driver/eigensolve_from_sidecar.py
//! --problem sphere-mie` and solved with dense LAPACK ZGGEV) — and
//! asserts Burn agreement sub-stage by sub-stage, mirroring
//! `sphere_mie_jax_reference.rs`:
//!
//! 1. Schema-level: expected field set, `c128` dtype on the tensor and
//!    eigenvalue fields. (No autodiff probe — TF-Java's static graph has
//!    no `grad` surface; the Phase J.5 DX probe is instead the
//!    tensor-constitutive typed-graph friction recorded on #174/#88.)
//! 2. Constitutive: per-tet diagonal UPML tensor vs
//!    `geode_core::assembly::nedelec::build_anisotropic_pml_tensor_diag` at `1e-14` on |Δ|.
//! 3. Spectrum: Burn `FaerComplexEigensolver` vs the TF-Java-side LAPACK
//!    ZGGEV on the identical complex-symmetric tensor-ε pencil; strict
//!    cross-IR window = the mesh-split TM_1,1 triplet (#160
//!    cluster-closure convention), `Im(λ) < 0` sign scoped to this mesh
//!    (no PR #155 canonicalization — deliberate divergence from the
//!    scalar-PML TF-Java fixture).
//! 4. J.1 analytic anchor (TM_1,1, 8 % coarse-mesh band) and the
//!    `Q > 1.5` tripwire on both sides.
//! 5. Cross-fixture consistency: the TF-Java snapshot's anchors and
//!    physical eigenvalues agree with the NumPy J.2 snapshot (the two
//!    sidecars cannot silently drift).
//!
//! All tests run under default `cargo test -p geode-validation` — the
//! small-mesh (~214-DOF) complex GEVD is well under a second (#158 /
//! #164 precedent).
//!
//! This is one of two complementary drift gates: the
//! `tfjava-cube-cavity` workflow's `sphere-mie-tfjava` job builds the
//! Maven module, runs the live JVM tensor-ε assembly end-to-end, and
//! hard-gates the resulting eigenvalues against both the NumPy J.2
//! baseline and the committed snapshot; this test exercises the
//! committed snapshot from the Burn side.

use std::collections::BTreeMap;
use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use num_complex::Complex64;

use geode_core::analytic::mie::{MiePolarisation, merged_roots};
use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_anisotropic_epsilon, build_anisotropic_pml_tensor_diag,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges, tet_centroids,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{R_BUFFER, R_SPHERE, SphereFixture};
use geode_core::testing::TestBackend;
use geode_util::eigen::{q_factor_from_lambda, re_k_from_lambda};
use geode_util::fixture::load_small_sphere_fixture;
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

/// Q-factor lower band for the TM_1,1 triplet — mirror of
/// `Q_LOWER_BAND_TM11` in `crates/geode-core/tests/mie_sphere.rs`.
const Q_LOWER_BAND_TM11: f64 = 1.5;

/// Documented coarse-mesh acceptance band on the lowest mode's `Re(k)`
/// vs the analytic TM_1,1 (observed ≈ 6.6 % on the small mesh).
const TM11_REL_TOL: f64 = 0.08;

/// Per-position absolute |Δ| gate for the TF-Java-vs-NumPy physical
/// band (matches the JAX / Julia Mie baseline tolerance and the
/// `gen_tfjava_baseline.py` generation-time assertion).
const TFJAVA_NUMPY_ABS_TOL: f64 = 1.0e-4;

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

fn tfjava_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_mie_small/tfjava_baseline.json")
}

fn numpy_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_mie_small/baseline.json")
}

fn load_tfjava_fixture() -> Fixture {
    Fixture::load_from(&tfjava_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small/tfjava_baseline.json should load")
}

// ---------------------------------------------------------------------------
// Burn pipeline → complex (K_int, M_int) under the anisotropic UPML.
// (Mirror of the helper in sphere_mie_jax_reference.rs.)
// ---------------------------------------------------------------------------

struct BurnMiePipeline {
    n_nodes: usize,
    n_tets: usize,
    n_edges: usize,
    n_interior_edges: usize,
    spurious_dim: usize,
    epsilon_tensor_diag_flat: Vec<Complex64>,
    k_int_complex: faer::Mat<faer::c64>,
    m_int_complex: faer::Mat<faer::c64>,
}

fn run_burn_mie_pipeline(
    fixture: &SphereFixture,
    sigma_0: f64,
    n_index: f64,
    k0_ref: f64,
) -> BurnMiePipeline {
    let n_nodes = fixture.mesh.n_nodes();
    let n_tets = fixture.mesh.n_tets();

    let centroids = tet_centroids(&fixture.mesh);
    let eps_aniso = build_anisotropic_pml_tensor_diag(
        &fixture.tet_physical_tags,
        &centroids,
        n_index,
        sigma_0,
        k0_ref,
    );
    let epsilon_tensor_diag_flat: Vec<Complex64> =
        geode_util::convert::flatten_complex_rows(&eps_aniso);

    let edges = fixture.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = fixture.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (mask_edges, interior_mask) = sphere_pec_interior_edges(&fixture.mesh, R_BUFFER);
    assert_eq!(mask_edges.len(), n_edges, "Burn-side edge mask shape");
    let n_interior_edges = interior_mask.iter().filter(|&&b| b).count();
    let spurious_dim = sphere_n_interior_nodes(&fixture.mesh, R_BUFFER);

    let device = <B as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(&fixture.mesh, &device);
    let sys = assemble_global_nedelec_with_anisotropic_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &eps_aniso,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    let dummy_zero = faer::Mat::<f64>::zeros(k_full.nrows(), k_full.ncols());
    let (k_int, _) = apply_dirichlet_bc(k_full.as_ref(), dummy_zero.as_ref(), &interior_mask)
        .expect("Dirichlet BC reduction (K)");

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

    BurnMiePipeline {
        n_nodes,
        n_tets,
        n_edges,
        n_interior_edges,
        spurious_dim,
        epsilon_tensor_diag_flat,
        k_int_complex,
        m_int_complex,
    }
}

// ---------------------------------------------------------------------------
// Schema-level tests (no eigensolve)
// ---------------------------------------------------------------------------

#[test]
fn tfjava_mie_fixture_loads_with_expected_schema() {
    let fixture = load_tfjava_fixture();
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(
        fixture.fixture_id,
        "sphere_mie_small/n48_aniso_upml_mie_tfjava"
    );

    for key in [
        "mesh_path",
        "sigma_0",
        "k0_ref",
        "r_sphere",
        "r_pml_inner",
        "r_buffer",
        "n_index",
        "epsilon_tensor_diag",
    ] {
        assert!(
            fixture.inputs.contains_key(key),
            "missing input field '{key}' in TF-Java Mie fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_tensor_diag"].dtype, "c128");
    assert_eq!(
        fixture.inputs["epsilon_tensor_diag"].shape.len(),
        2,
        "epsilon_tensor_diag should be declared (n_tets, 3)"
    );
    assert_eq!(fixture.inputs["epsilon_tensor_diag"].shape[1], 3);

    // Same output contract as the JAX J.4 fixture minus the
    // JAX-specific autodiff probe fields (TF-Java's static graph has no
    // grad surface — the Phase J.5 DX record is the typed-graph
    // tensor-constitutive friction note on #174).
    for key in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "eigenvalues_lowest_complex",
        "physical_eigenvalues_complex",
        "strict_mode_window_len",
        "analytic_tm11_k",
        "lowest_physical_re_k",
        "tm11_rel_err_lowest",
        "q_factor_lowest_physical",
        "q_median_tm11_triplet",
        "sigma_zero_lowest_physical_re",
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "missing output field '{key}' in TF-Java Mie fixture"
        );
    }
    assert_eq!(fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128");
    assert_eq!(
        fixture.outputs["physical_eigenvalues_complex"].dtype,
        "c128"
    );
}

#[test]
fn tfjava_mie_analytic_anchor_matches_mie_roots() {
    // The fixture re-exports the J.1 catalogue's TM_1,1 root; pin it
    // against the live `merged_roots` computation.
    let fixture = load_tfjava_fixture();
    let anchor = fixture.output_scalar("analytic_tm11_k");

    let n_index = fixture.input_f64("n_index");
    let analytic = merged_roots(n_index, &[1, 2, 3], R_SPHERE, R_BUFFER, 3);
    let ground = analytic
        .iter()
        .min_by(|a, b| a.k.partial_cmp(&b.k).unwrap())
        .expect("at least one analytic root");
    assert_eq!(ground.pol, MiePolarisation::TM);
    assert_eq!(ground.l, 1);
    assert_eq!(ground.n, 1);
    let err = (ground.k - anchor).abs();
    assert!(
        err < 1e-9,
        "fixture analytic_tm11_k = {anchor} vs merged_roots TM_1,1 = {} (|Δ| = {err:.3e})",
        ground.k
    );
}

#[test]
fn tfjava_mie_consistent_with_numpy_small_baseline() {
    // Cross-fixture tie: the TF-Java and NumPy J.2 snapshots pin the
    // same pencil, so their scalar anchors must agree far inside the
    // per-field tolerances. Catches the two sidecars drifting apart
    // even if each individually still passes its own Burn gate.
    let tfjava = load_tfjava_fixture();
    let numpy = Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small/baseline.json should load");

    for (field, tol) in [
        ("analytic_tm11_k", 1e-12),
        ("lowest_physical_re_k", 1e-9),
        ("sigma_zero_lowest_physical_re", 1e-9),
        ("q_factor_lowest_physical", 1e-6),
        ("q_median_tm11_triplet", 1e-6),
        ("strict_mode_window_len", 0.0),
    ] {
        let a = tfjava.output_scalar(field);
        let b = numpy.output_scalar(field);
        assert!(
            (a - b).abs() <= tol,
            "TF-Java vs NumPy snapshot drift on `{field}`: {a} vs {b} (tol {tol:.0e})"
        );
    }
}

#[test]
fn tfjava_mie_physical_eigenvalues_agree_with_numpy_canonical() {
    // Per-position physical-band pin against the canonical NumPy J.2
    // baseline at 1e-4 absolute |Δ| — same direct (no conjugate
    // freedom) comparison the CI sphere-mie-tfjava job enforces on the
    // live JVM output. Unlike the scalar-PML TF-Java fixture there is
    // NO sign canonicalization: both fixtures carry the tensor
    // pencil's native Im(λ) < 0 on this mesh, so the diff is strict.
    let tfjava = load_tfjava_fixture();
    let numpy = Fixture::load_from(&numpy_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small/baseline.json should load");

    let tf_phys = tfjava
        .output_c128("physical_eigenvalues_complex")
        .expect("TF-Java physical_eigenvalues_complex decodes");
    let np_phys = numpy
        .output_c128("physical_eigenvalues_complex")
        .expect("NumPy physical_eigenvalues_complex decodes");

    let n_compare = std::cmp::min(tf_phys.data.len(), np_phys.data.len());
    assert!(
        n_compare > 0,
        "no physical eigenvalues to compare between TF-Java and NumPy"
    );

    let mut max_abs = 0.0_f64;
    let mut idx_max = 0usize;
    for (i, (t, n)) in tf_phys
        .data
        .iter()
        .zip(np_phys.data.iter())
        .enumerate()
        .take(n_compare)
    {
        let diff = (t - n).norm();
        if diff > max_abs {
            max_abs = diff;
            idx_max = i;
        }
        assert!(
            diff < TFJAVA_NUMPY_ABS_TOL,
            "physical[{i}]: |Δ| = {diff:.3e} exceeds {TFJAVA_NUMPY_ABS_TOL:.0e} \
             (TF-Java = {} {:+}j, NumPy = {} {:+}j)",
            t.re,
            t.im,
            n.re,
            n.im
        );
    }

    eprintln!(
        "sphere_mie_small TF-Java vs NumPy canonical agreement: max |Δ| = \
         {max_abs:.3e} at idx {idx_max} over {n_compare} physical modes \
         (tol = {TFJAVA_NUMPY_ABS_TOL:.0e})"
    );
}

#[test]
fn tfjava_mie_physical_eigenvalues_have_tensor_pencil_signature() {
    // Fixture-side sanity on the sign convention: with σ₀ > 0 the
    // anisotropic UPML tensor pencil's physical modes carry
    // Re(λ) > 0 and Im(λ) < 0 on this small mesh — the OPPOSITE
    // Im-sign of the scalar-PML TF-Java fixture (which canonicalizes
    // to Im(λ) > 0 per PR #155). A regenerated fixture that silently
    // re-applied the PR #155 flip would fail here before reaching the
    // eigensolve cross-checks.
    let fixture = load_tfjava_fixture();

    let sigma_0 = fixture.input_f64("sigma_0");
    assert!(
        sigma_0 > 0.0,
        "canonical TF-Java Mie fixture should carry σ₀ > 0 (got {sigma_0})"
    );

    let physical = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("physical_eigenvalues_complex decodes");
    assert!(
        !physical.data.is_empty(),
        "no physical eigenvalues recorded in TF-Java Mie fixture"
    );
    for (i, lam) in physical.data.iter().enumerate() {
        assert!(
            lam.re > 0.0,
            "physical[{i}] has Re(λ) = {} ≤ 0 — should be oscillatory",
            lam.re
        );
        assert!(
            lam.im < 0.0,
            "physical[{i}] has Im(λ) = {} ≥ 0 — the small-mesh tensor \
             pencil's physical modes carry Im(λ) < 0 (no PR #155 \
             canonicalization on the Mie chain)",
            lam.im
        );
    }
}

#[test]
fn tfjava_mie_epsilon_tensor_decodes() {
    // Bit-exact `epsilon_tensor_diag` round-trip through `input_c128`,
    // and Burn-side `build_anisotropic_pml_tensor_diag` agreement —
    // this pins the JVM twin (`SphereMesh.buildAnisotropicPmlTensorDiag`)
    // contract: the closed-form tensor profile must match Rust
    // tet-for-tet, axis-for-axis.
    let fixture = load_tfjava_fixture();
    let golden_eps = fixture
        .input_c128("epsilon_tensor_diag")
        .expect("c128 input decodes");

    let sigma_0 = fixture.input_f64("sigma_0");
    let n_index = fixture.input_f64("n_index");
    let k0_ref = fixture.input_f64("k0_ref");

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_mie_pipeline(&sphere, sigma_0, n_index, k0_ref);
    assert_eq!(
        burn.epsilon_tensor_diag_flat.len(),
        golden_eps.len(),
        "tensor length mismatch (flattened (n_tets, 3))"
    );

    let mut max_abs = 0.0_f64;
    for (i, (got, want)) in burn
        .epsilon_tensor_diag_flat
        .iter()
        .zip(golden_eps.iter())
        .enumerate()
    {
        let err = (got - want).norm();
        if err > max_abs {
            max_abs = err;
        }
        assert!(
            err < 1.0e-14,
            "epsilon_tensor_diag[{i}]: |Δ| = {err:.3e} exceeds 1e-14 \
             (Burn = {got}, TF-Java = {want})"
        );
    }
    eprintln!(
        "sphere_mie_small (TF-Java) epsilon_tensor_diag: Burn vs TF-Java max |Δ| = {max_abs:.3e}"
    );
}

// ---------------------------------------------------------------------------
// Small-mesh spectrum cross-check (default `cargo test`)
// ---------------------------------------------------------------------------

#[test]
fn tfjava_mie_small_spectrum_agrees_with_burn() {
    let fixture = load_tfjava_fixture();

    let sigma_0 = fixture.input_f64("sigma_0");
    let n_index = fixture.input_f64("n_index");
    let k0_ref = fixture.input_f64("k0_ref");

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_mie_pipeline(&sphere, sigma_0, n_index, k0_ref);

    // Mesh-shape integer cross-checks.
    assert_eq!(burn.n_nodes, fixture.output_scalar("n_nodes") as usize);
    assert_eq!(burn.n_tets, fixture.output_scalar("n_tets") as usize);
    assert_eq!(burn.n_edges, fixture.output_scalar("n_edges") as usize);
    assert_eq!(
        burn.n_interior_edges,
        fixture.output_scalar("n_interior_edges") as usize
    );
    assert_eq!(
        burn.spurious_dim,
        fixture.output_scalar("spurious_dim") as usize
    );

    // Solve the complex generalized tensor-ε pencil on the Burn side.
    let n_request = burn.spurious_dim + 8;
    let solver = FaerComplexEigensolver;
    let burn_eigvals_faer = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("Burn complex eigensolve on small Mie fixture");
    let burn_eigvals: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&burn_eigvals_faer);

    let golden_full = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    assert_eq!(
        burn_eigvals.len(),
        golden_full.data.len(),
        "Burn returned {} eigenvalues, TF-Java baseline has {} — request mismatch",
        burn_eigvals.len(),
        golden_full.data.len()
    );

    let n_spurious_ref = fixture.output_scalar("n_spurious_observed") as usize;
    assert_eq!(
        n_spurious_ref, burn.spurious_dim,
        "n_spurious_observed in fixture should match Burn's spurious_dim"
    );
    let golden_physical = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("c128 physical decodes");
    let physical_take = golden_physical.data.len();
    assert!(
        n_spurious_ref + physical_take <= burn_eigvals.len(),
        "spectrum too short to expose {physical_take} physical modes"
    );
    let burn_physical: Vec<Complex64> =
        burn_eigvals[n_spurious_ref..n_spurious_ref + physical_take].to_vec();

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert(
        "eigenvalues_lowest_complex".to_string(),
        burn_eigvals.clone(),
    );
    actual.insert(
        "physical_eigenvalues_complex".to_string(),
        burn_physical.clone(),
    );

    let report = geode_validation::compare_complex_against(&fixture, &actual);
    if !report.passed {
        eprintln!(
            "sphere_mie_small (TF-Java) complex-comparator report: {:#?}",
            report
        );
        for f in &report.fields {
            eprintln!(
                "  field={} passed={} tol={:.0e} max_abs={:?}",
                f.field, f.passed, f.tolerance_abs, f.max_abs_error
            );
        }
        panic!("sphere_mie_small complex spectrum disagreed with TF-Java baseline");
    }

    // Strict cross-IR window (#160): the TM_1,1 triplet.
    let window_len = fixture.output_scalar("strict_mode_window_len") as usize;
    assert!(window_len <= physical_take);
    let mut window_max = 0.0_f64;
    for (got, want) in burn_physical
        .iter()
        .zip(golden_physical.data.iter())
        .take(window_len)
    {
        let err = (got - want).norm();
        if err > window_max {
            window_max = err;
        }
    }
    let triplet_spread = burn_physical[window_len - 1].re - burn_physical[0].re;
    let gap_to_next = burn_physical[window_len].re - burn_physical[window_len - 1].re;
    assert!(
        gap_to_next > 2.0 * triplet_spread,
        "strict mode window does not end at a spectral gap on the Burn side \
         (spread = {triplet_spread:.4}, gap = {gap_to_next:.4})"
    );

    // Small-mesh sign note: physical Im(λ) < 0 on the tensor pencil
    // (mesh-dependent — see sphere_mie_numpy_reference.rs module docs).
    for (i, lam) in burn_physical.iter().take(window_len).enumerate() {
        assert!(
            lam.im <= 1e-10,
            "Burn physical[{i}] = {lam} has Im(λ) > 0 — the anisotropic \
             UPML pencil's physical modes carry Im(λ) < 0 on the small mesh"
        );
    }

    // J.1 analytic anchor: lowest mode within the 8 % band on both sides.
    let analytic_tm11_k = fixture.output_scalar("analytic_tm11_k");
    let burn_re_k = re_k_from_lambda(burn_physical[0]);
    let tfjava_re_k = fixture.output_scalar("lowest_physical_re_k");
    let burn_rel_err = (burn_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    let tfjava_rel_err = (tfjava_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    assert!(
        burn_rel_err < TM11_REL_TOL,
        "Burn lowest Re(k) = {burn_re_k:.5} differs from analytic TM_1,1 = \
         {analytic_tm11_k:.5} by {:.2}% (> {:.0}%)",
        burn_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
    assert!(
        tfjava_rel_err < TM11_REL_TOL,
        "TF-Java lowest Re(k) = {tfjava_re_k:.5} differs from analytic TM_1,1 = \
         {analytic_tm11_k:.5} by {:.2}% (> {:.0}%)",
        tfjava_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
    let re_k_delta = (burn_re_k - tfjava_re_k).abs();
    assert!(
        re_k_delta
            < fixture
                .output_f64("lowest_physical_re_k")
                .unwrap()
                .tolerance_abs,
        "Burn vs TF-Java lowest Re(k): |Δ| = {re_k_delta:.3e}"
    );

    // Q tripwire on both sides + regression floor.
    let burn_q = q_factor_from_lambda(burn_physical[0]);
    let golden_q = fixture.output_f64("q_factor_lowest_physical").unwrap();
    let tfjava_q = golden_q.data[0];
    assert!(
        burn_q > Q_LOWER_BAND_TM11,
        "Burn lowest-mode Q = {burn_q:.3} below band {Q_LOWER_BAND_TM11}"
    );
    assert!(
        tfjava_q > Q_LOWER_BAND_TM11,
        "TF-Java lowest-mode Q = {tfjava_q:.3} below band {Q_LOWER_BAND_TM11}"
    );
    let q_err = (burn_q - tfjava_q).abs();
    assert!(
        q_err < golden_q.tolerance_abs,
        "q_factor_lowest_physical: Burn = {burn_q:.4}, TF-Java = {tfjava_q:.4}, \
         |Δ| = {q_err:.3e} exceeds {:.0e}",
        golden_q.tolerance_abs
    );

    // Triplet median Q.
    let mut qs: Vec<f64> = burn_physical[..window_len]
        .iter()
        .map(|lam| q_factor_from_lambda(*lam))
        .collect();
    qs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let burn_q_median = qs[window_len / 2];
    let golden_q_median = fixture.output_f64("q_median_tm11_triplet").unwrap();
    assert!(
        burn_q_median > Q_LOWER_BAND_TM11,
        "Burn TM_1,1 triplet median Q = {burn_q_median:.3} below band {Q_LOWER_BAND_TM11}"
    );
    let q_median_err = (burn_q_median - golden_q_median.data[0]).abs();
    assert!(
        q_median_err < golden_q_median.tolerance_abs,
        "q_median_tm11_triplet: Burn = {burn_q_median:.4}, TF-Java = {:.4}, \
         |Δ| = {q_median_err:.3e}",
        golden_q_median.data[0]
    );

    eprintln!(
        "sphere_mie_small Burn-vs-TF-Java agreement: strict TM_1,1-triplet window \
         max |Δλ| = {window_max:.3e}; lowest Re(k): Burn {burn_re_k:.5} / TF-Java \
         {tfjava_re_k:.5} (analytic {analytic_tm11_k:.5}, rel err {:.2}% / {:.2}%, \
         band {:.0}%); Q: Burn {burn_q:.2} / TF-Java {tfjava_q:.2} (band > \
         {Q_LOWER_BAND_TM11}); triplet median Q = {burn_q_median:.2}",
        burn_rel_err * 100.0,
        tfjava_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
}
