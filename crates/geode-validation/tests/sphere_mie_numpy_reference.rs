//! Cross-backend anisotropic-UPML dielectric-sphere Mie agreement test
//! (issue #171, Epic #88 Phase J.2).
//!
//! Loads the NumPy references for the end-to-end Mie pipeline of
//! `crates/geode-core/tests/mie_sphere.rs` — dielectric sphere
//! (`n = 1.5`, `R_s = 1.0`, `R_b = 2.0`) with the **anisotropic UPML**
//! (`σ₀ = 5.0`, `k₀_ref = 2.0`, issue #54) — and asserts Burn agreement
//! sub-stage by sub-stage:
//!
//! 1. Schema-level: the fixtures carry the expected field set with
//!    `c128` dtype on `epsilon_tensor_diag` /
//!    `eigenvalues_lowest_complex` / `physical_eigenvalues_complex`.
//! 2. Constitutive: the full per-tet diagonal UPML tensor
//!    (`geode_core::assembly::nedelec::build_anisotropic_pml_tensor_diag`) at `1e-14`
//!    absolute on `|Δ|` (bit-exact f64 round-trip).
//! 3. Spectrum: Burn `FaerComplexEigensolver` vs NumPy LAPACK ZGGEV on
//!    the identical complex-symmetric tensor-ε pencil. The **strict
//!    cross-IR mode window** is the first `strict_mode_window_len = 3`
//!    physical modes — the mesh-split TM_1,1 triplet (multiplicity
//!    2l+1 = 3), closed at a spectral gap per the #160 cluster-closure
//!    convention (never bisect a degenerate multiplet; taking 5 would
//!    cut into the TE_1,1 / TM_2,1 band at λ ≈ 3.3). Positions [3, 4]
//!    are still compared at the same per-field tolerance (dense vs
//!    dense sees the whole spectrum), but the closed-cluster claim is
//!    scoped to the triplet.
//! 4. J.1 analytic anchor: both Burn and NumPy lowest-mode `Re(k)`
//!    within the documented 8 % coarse-mesh band of the analytic
//!    TM_1,1 root (`k ≈ 1.30343` from
//!    `reference/fixtures/mie_roots/baseline.json`), and the fixture's
//!    re-exported anchor agrees with `geode_core::analytic::mie::merged_roots` at
//!    `1e-9`.
//! 5. Q tripwire: Q of the lowest mode and the TM_1,1-triplet median Q
//!    above `Q_LOWER_BAND_TM11 = 1.5` on both sides — the
//!    PML-misconfiguration tripwire from `mie_sphere.rs` (σ₀ drift,
//!    mask break, vacuum-gap removal).
//! 6. σ₀ = 0 collapse: the tensor degenerates to the real isotropic
//!    scalar, `Im(M) = 0` bit-exactly, the spectrum is real to f64
//!    precision, and the lowest physical `Re(λ)` matches the in-fixture
//!    PEC anchor.
//!
//! # Sign convention note
//!
//! Unlike the scalar-isotropic PML (PR #155: physical `Im(λ) > 0`
//! everywhere), the sign of `Im(λ)` on the anisotropic UPML pencil's
//! physical band is **mesh-dependent**: the radial tensor entry
//! carries `1/s_r` (`Im > 0`) while the transverse entries carry
//! `s_t` (`Im < 0`), and which contribution wins depends on how the
//! discrete modes overlap the shell. Observed: `Im(λ) < 0` on the
//! 197-tet small mesh, `Im(λ) > 0` on the refined 774-node mesh. In
//! both cases the sign is a property of the pencil — not a solver
//! choice (eigenvalues of a fixed complex-symmetric pencil are
//! uniquely determined; only eigenvector phase is ambiguous), and
//! LAPACK ZGGEV and faer QZ agree on it. Q stays sign-agnostic, and
//! the per-fixture sign assertions below are scoped to their mesh.
//!
//! # Small mesh vs full mesh
//!
//! Following the #158 / #164 precedent, the **small-mesh** tests
//! (197-tet mesh shared with `sphere_pml_small`) run under default
//! `cargo test -p geode-validation` — the ~214-DOF complex GEVD is
//! well under a second. The **full-mesh** test
//! (`sphere_mie_spectrum_agrees_with_numpy`, 774-node fixture) is
//! `#[ignore]`-gated: faer 0.24's complex GEVD panics under
//! debug-assertions and is multi-minute even in release on the
//! ~3300-DOF interior pencil (measured ~3 min on an M-series laptop;
//! the historical 60+ min figure from the Phase H.1 docs applies to
//! older hardware). Run with:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --test sphere_mie_numpy_reference -- --ignored --nocapture
//! ```

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
use geode_core::mesh::{
    R_BUFFER, R_SPHERE, SphereFixture, read_sphere_fixture, read_sphere_fixture_from_bytes,
};
use geode_core::testing::TestBackend;
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

/// Q-factor lower band for the TM_1,1 triplet — mirror of
/// `Q_LOWER_BAND_TM11` in `crates/geode-core/tests/mie_sphere.rs`.
const Q_LOWER_BAND_TM11: f64 = 1.5;

/// Documented coarse-mesh acceptance band on the lowest mode's `Re(k)`
/// vs the analytic TM_1,1 — mirror of the 8 % assertion in
/// `mie_sphere.rs` (observed ≈ 5.7 % full mesh, ≈ 6.6 % small mesh).
const TM11_REL_TOL: f64 = 0.08;

// ---------------------------------------------------------------------------
// Fixture paths
// ---------------------------------------------------------------------------

fn full_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_mie/baseline.json")
}

fn small_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_mie_small/baseline.json")
}

/// The small mesh is shared with the #158 sphere_pml_small fixture —
/// not duplicated under sphere_mie_small/.
fn small_mesh_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pml_small/sphere.msh")
}

fn load_small_sphere_fixture() -> SphereFixture {
    let bytes = std::fs::read(small_mesh_path()).expect("read small-mesh sphere.msh bytes");
    read_sphere_fixture_from_bytes(&bytes).expect("parse small-mesh sphere.msh")
}

// ---------------------------------------------------------------------------
// Burn pipeline → complex (K_int, M_int) under the anisotropic UPML.
// ---------------------------------------------------------------------------

struct BurnMiePipeline {
    n_nodes: usize,
    n_tets: usize,
    n_edges: usize,
    n_interior_edges: usize,
    spurious_dim: usize,
    /// Row-major flattened (tet, axis) diagonal tensor — matches the
    /// fixture's `epsilon_tensor_diag` on-disk layout.
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
// λ → k and Q helpers (principal branch, sign-agnostic Q) — mirror of
// `mie_sphere.rs` and `reference/numpy/sphere_mie.py`.
// ---------------------------------------------------------------------------

fn re_k_from_lambda(lambda: Complex64) -> f64 {
    let r = (lambda.re * lambda.re + lambda.im * lambda.im).sqrt();
    (0.5 * (r + lambda.re)).max(0.0).sqrt()
}

fn q_factor_from_lambda(lambda: Complex64) -> f64 {
    let r = (lambda.re * lambda.re + lambda.im * lambda.im).sqrt();
    let re_k = (0.5 * (r + lambda.re)).max(0.0).sqrt();
    let im_k_mag = (0.5 * (r - lambda.re)).max(0.0).sqrt();
    if im_k_mag > 1e-12 {
        re_k / (2.0 * im_k_mag)
    } else {
        f64::INFINITY
    }
}

// ---------------------------------------------------------------------------
// Schema-level tests (no eigensolve; default `cargo test`)
// ---------------------------------------------------------------------------

fn assert_mie_fixture_schema(fixture: &Fixture, expect_sigma_zero_anchor: bool) {
    assert_eq!(fixture.schema_version, "1");

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
            "missing input field '{key}' in sphere_mie fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_tensor_diag"].dtype, "c128");
    assert_eq!(
        fixture.inputs["epsilon_tensor_diag"].shape.len(),
        2,
        "epsilon_tensor_diag should be declared (n_tets, 3)"
    );
    assert_eq!(fixture.inputs["epsilon_tensor_diag"].shape[1], 3);

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
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "missing output field '{key}' in sphere_mie fixture"
        );
    }
    if expect_sigma_zero_anchor {
        assert!(
            fixture
                .outputs
                .contains_key("sigma_zero_lowest_physical_re"),
            "missing in-fixture σ₀=0 PEC anchor"
        );
    }
    assert_eq!(fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128");
    assert_eq!(
        fixture.outputs["physical_eigenvalues_complex"].dtype,
        "c128"
    );
    assert_eq!(fixture.outputs["q_factor_lowest_physical"].dtype, "f64");
}

#[test]
fn sphere_mie_small_fixture_loads_with_expected_schema() {
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small baseline.json should load");
    assert_eq!(fixture.fixture_id, "sphere_mie_small/n48_aniso_upml_mie");
    assert_mie_fixture_schema(&fixture, true);
}

#[test]
fn sphere_mie_small_analytic_anchor_matches_mie_roots() {
    // The fixture re-exports the J.1 catalogue's TM_1,1 root; pin it
    // against both the live `merged_roots` computation and the J.1
    // fixture so the two Phase J fixtures cannot silently drift apart.
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small baseline.json should load");
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

    // And against the J.1 fixture directly.
    let j1 = Fixture::load_from(
        &geode_validation::fixture_path("mie_roots/baseline.json"),
        FixtureFormat::Json,
    )
    .expect("mie_roots baseline.json should load");
    let pols = j1.output_f64("root_pol").unwrap().data.clone();
    let ls = j1.output_f64("root_l").unwrap().data.clone();
    let ns = j1.output_f64("root_n").unwrap().data.clone();
    let ks = j1.output_f64("root_k").unwrap().data.clone();
    let tm11_j1 = (0..ks.len())
        .find(|&i| {
            pols[i].round() as i64 == 1 && ls[i].round() as i64 == 1 && ns[i].round() as i64 == 1
        })
        .map(|i| ks[i])
        .expect("TM_1,1 in the J.1 catalogue");
    assert!(
        (tm11_j1 - anchor).abs() < 1e-12,
        "fixture analytic_tm11_k = {anchor} vs J.1 catalogue TM_1,1 = {tm11_j1}"
    );
}

#[test]
fn sphere_mie_small_epsilon_tensor_decodes() {
    // Bit-exact `epsilon_tensor_diag` round-trip through `input_c128`,
    // and Burn-side `build_anisotropic_pml_tensor_diag` agreement at
    // f64 precision. Runs under default `cargo test` (no eigensolve).
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small baseline.json should load");
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
             (Burn = {got}, NumPy = {want})"
        );
    }
    eprintln!(
        "sphere_mie_small epsilon_tensor_diag: Burn vs NumPy max |Δ| = {max_abs:.3e} \
         (anisotropic UPML tensor round-trips through c128 at f64 floor)"
    );
}

// ---------------------------------------------------------------------------
// Small-mesh spectrum cross-check (default `cargo test`, per #158/#164
// precedent — the ~214-DOF complex GEVD is well under a second).
// ---------------------------------------------------------------------------

#[test]
fn sphere_mie_small_spectrum_agrees_with_numpy() {
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small baseline.json should load");

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
    let t_start = std::time::Instant::now();
    let solver = FaerComplexEigensolver;
    let burn_eigvals_faer = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("Burn complex eigensolve on small Mie fixture");
    let gevd_wall = t_start.elapsed();
    eprintln!(
        "sphere_mie_small Burn complex GEVD on {0}×{0} pencil: {1:.3} s",
        burn.n_interior_edges,
        gevd_wall.as_secs_f64()
    );
    assert!(
        gevd_wall.as_secs_f64() < 30.0,
        "small-mesh Burn complex GEVD took {:.2} s, exceeds the 30 s default-CI budget",
        gevd_wall.as_secs_f64()
    );

    let burn_eigvals: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&burn_eigvals_faer);

    // Full-slice + physical-band comparison via the c128 comparator.
    let golden_full = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    assert_eq!(
        burn_eigvals.len(),
        golden_full.data.len(),
        "Burn returned {} eigenvalues, NumPy baseline has {} — request mismatch",
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
        eprintln!("sphere_mie_small complex-comparator report: {:#?}", report);
        for f in &report.fields {
            eprintln!(
                "  field={} passed={} tol={:.0e} max_abs={:?}",
                f.field, f.passed, f.tolerance_abs, f.max_abs_error
            );
        }
        panic!("sphere_mie_small complex spectrum disagreed with NumPy baseline");
    }

    // Strict cross-IR window (#160 cluster closure): the TM_1,1
    // triplet, closed at a spectral gap. Report the measured residual
    // on the window separately — dense-vs-dense on the identical
    // pencil should be far inside the per-field tolerance.
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
    // Cluster-closure sanity on the Burn side: the window must end at
    // a spectral gap (gap to the next band ≫ intra-triplet spread).
    let triplet_spread = burn_physical[window_len - 1].re - burn_physical[0].re;
    let gap_to_next = burn_physical[window_len].re - burn_physical[window_len - 1].re;
    assert!(
        gap_to_next > 2.0 * triplet_spread,
        "strict mode window does not end at a spectral gap on the Burn side \
         (spread = {triplet_spread:.4}, gap = {gap_to_next:.4}) — \
         cluster-closure convention (#160) violated"
    );

    // Anisotropic-pencil sign note: physical Im(λ) < 0 on the small
    // mesh (mesh-dependent — the refined mesh shows Im(λ) > 0; see
    // module docs) — assert it so a silent sign flip in either
    // backend trips.
    for (i, lam) in burn_physical.iter().take(window_len).enumerate() {
        assert!(
            lam.im <= 1e-10,
            "Burn physical[{i}] = {lam} has Im(λ) > 0 — the anisotropic \
             UPML pencil's physical modes carry Im(λ) < 0 on the small mesh"
        );
    }

    // J.1 analytic anchor: lowest mode within the documented 8 % band
    // on both sides.
    let analytic_tm11_k = fixture.output_scalar("analytic_tm11_k");
    let burn_re_k = re_k_from_lambda(burn_physical[0]);
    let numpy_re_k = fixture.output_scalar("lowest_physical_re_k");
    let burn_rel_err = (burn_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    let numpy_rel_err = (numpy_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    assert!(
        burn_rel_err < TM11_REL_TOL,
        "Burn lowest Re(k) = {burn_re_k:.5} differs from analytic TM_1,1 = \
         {analytic_tm11_k:.5} by {:.2}% (> {:.0}%)",
        burn_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
    assert!(
        numpy_rel_err < TM11_REL_TOL,
        "NumPy lowest Re(k) = {numpy_re_k:.5} differs from analytic TM_1,1 = \
         {analytic_tm11_k:.5} by {:.2}% (> {:.0}%)",
        numpy_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
    let re_k_delta = (burn_re_k - numpy_re_k).abs();
    assert!(
        re_k_delta
            < fixture
                .output_f64("lowest_physical_re_k")
                .unwrap()
                .tolerance_abs,
        "Burn vs NumPy lowest Re(k): |Δ| = {re_k_delta:.3e}"
    );

    // Q tripwire (Q_LOWER_BAND_TM11) on both sides + regression floor.
    let burn_q = q_factor_from_lambda(burn_physical[0]);
    let golden_q = fixture.output_f64("q_factor_lowest_physical").unwrap();
    let numpy_q = golden_q.data[0];
    assert!(
        burn_q > Q_LOWER_BAND_TM11,
        "Burn lowest-mode Q = {burn_q:.3} below band {Q_LOWER_BAND_TM11} — \
         likely PML σ₀ drift, mask break, or vacuum-gap removal"
    );
    assert!(
        numpy_q > Q_LOWER_BAND_TM11,
        "NumPy lowest-mode Q = {numpy_q:.3} below band {Q_LOWER_BAND_TM11}"
    );
    let q_err = (burn_q - numpy_q).abs();
    assert!(
        q_err < golden_q.tolerance_abs,
        "q_factor_lowest_physical: Burn = {burn_q:.4}, NumPy = {numpy_q:.4}, \
         |Δ| = {q_err:.3e} exceeds {:.0e} (note dQ/dIm(λ) ≈ Q/|Im(λ)| amplification)",
        golden_q.tolerance_abs
    );

    // Triplet median Q — mirror of mie_sphere_tm11_triplet_q_above_band.
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
        "q_median_tm11_triplet: Burn = {burn_q_median:.4}, NumPy = {:.4}, \
         |Δ| = {q_median_err:.3e}",
        golden_q_median.data[0]
    );

    eprintln!(
        "sphere_mie_small cross-backend agreement: strict TM_1,1-triplet window \
         max |Δλ| = {window_max:.3e}; lowest Re(k): Burn {burn_re_k:.5} / NumPy \
         {numpy_re_k:.5} (analytic {analytic_tm11_k:.5}, rel err {:.2}% / {:.2}%, \
         band {:.0}%); Q: Burn {burn_q:.2} / NumPy {numpy_q:.2} (band > \
         {Q_LOWER_BAND_TM11}); triplet median Q = {burn_q_median:.2}",
        burn_rel_err * 100.0,
        numpy_rel_err * 100.0,
        TM11_REL_TOL * 100.0
    );
}

#[test]
fn sphere_mie_small_sigma_zero_collapses_to_real_isotropic() {
    // σ₀ = 0 regression: the anisotropic tensor must degenerate to the
    // real isotropic scalar everywhere (all three axes equal, Im = 0
    // bit-exactly), the assembled complex mass must carry no imaginary
    // content, the spectrum must be real to f64 precision, and the
    // lowest physical Re(λ) must hit the in-fixture PEC anchor.
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie_small baseline.json should load");
    let n_index = fixture.input_f64("n_index");
    let k0_ref = fixture.input_f64("k0_ref");

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_mie_pipeline(&sphere, 0.0, n_index, k0_ref);

    // (1) Tensor degenerates: Im = 0 exactly, all axes equal.
    for (i, chunk) in burn.epsilon_tensor_diag_flat.chunks_exact(3).enumerate() {
        for (a, c) in chunk.iter().enumerate() {
            assert_eq!(
                c.im, 0.0,
                "σ₀ = 0: epsilon_tensor_diag[{i}][{a}].im should be exactly 0"
            );
        }
        assert_eq!(
            chunk[0], chunk[1],
            "σ₀ = 0: tensor not isotropic at tet {i}"
        );
        assert_eq!(
            chunk[0], chunk[2],
            "σ₀ = 0: tensor not isotropic at tet {i}"
        );
    }

    // (2) Im(M_int) ≈ 0 at f64 precision.
    let m_int = &burn.m_int_complex;
    let mut max_abs_im = 0.0_f64;
    for j in 0..m_int.ncols() {
        for i in 0..m_int.nrows() {
            let v = m_int[(i, j)].im.abs();
            if v > max_abs_im {
                max_abs_im = v;
            }
        }
    }
    assert!(
        max_abs_im < 1.0e-12,
        "σ₀ = 0: assembled M_int_complex Im leaked, max |Im(M_ij)| = {max_abs_im:.3e}"
    );

    // (3) Spectrum real to f64 precision.
    let n_request = burn.spurious_dim + 5;
    let solver = FaerComplexEigensolver;
    let eigvals = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("σ₀ = 0 small-mesh complex eigensolve");
    let mut max_rel_im = 0.0_f64;
    for lam in &eigvals {
        let rel = lam.im.abs() / lam.re.abs().max(1.0);
        if rel > max_rel_im {
            max_rel_im = rel;
        }
    }
    assert!(
        max_rel_im < 1.0e-10,
        "σ₀ = 0 spectrum should be real to f64 precision; observed \
         max |Im(λ)|/max(|Re(λ)|, 1) = {max_rel_im:.3e}"
    );

    // (4) Lowest physical Re(λ) vs the in-fixture PEC anchor.
    let anchor = fixture
        .output_f64("sigma_zero_lowest_physical_re")
        .expect("sigma_zero_lowest_physical_re decodes");
    let n_spurious = burn.spurious_dim;
    assert!(n_spurious < eigvals.len());
    let burn_lowest = eigvals[n_spurious];
    let re_err = (burn_lowest.re - anchor.data[0]).abs();
    assert!(
        re_err < anchor.tolerance_abs,
        "σ₀ = 0: lowest physical Re(λ) = {:.6} vs in-fixture PEC anchor {:.6}, \
         |Δ| = {re_err:.3e} exceeds {:.0e}",
        burn_lowest.re,
        anchor.data[0],
        anchor.tolerance_abs
    );
    eprintln!(
        "σ₀ = 0 (sphere_mie_small) collapse: max |Im(λ)|/|Re(λ)| = {max_rel_im:.3e}, \
         lowest physical Re(λ) = {:.6} (anchor {:.6}, |Δ| = {re_err:.3e})",
        burn_lowest.re, anchor.data[0]
    );
}

// ---------------------------------------------------------------------------
// Full-mesh fixture (release-gated, #[ignore]) — the 774-node bundled
// sphere at the exact mie_sphere.rs acceptance parameters.
// ---------------------------------------------------------------------------

#[test]
fn sphere_mie_full_fixture_loads_with_expected_schema() {
    let fixture = Fixture::load_from(&full_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie baseline.json should load");
    assert_eq!(fixture.fixture_id, "sphere_mie/n774_aniso_upml_mie");
    assert_mie_fixture_schema(&fixture, false);
}

#[test]
#[ignore = "faer 0.24 complex GEVD panics under debug-assertions and is multi-minute in release on the ~3300-DOF pencil; run with `cargo test -p geode-validation --release --test sphere_mie_numpy_reference -- --ignored`"]
fn sphere_mie_spectrum_agrees_with_numpy() {
    let fixture = Fixture::load_from(&full_fixture_path(), FixtureFormat::Json)
        .expect("sphere_mie baseline.json should load");

    let sigma_0 = fixture.input_f64("sigma_0");
    let n_index = fixture.input_f64("n_index");
    let k0_ref = fixture.input_f64("k0_ref");

    let sphere = read_sphere_fixture().expect("bundled sphere fixture load");
    let burn = run_burn_mie_pipeline(&sphere, sigma_0, n_index, k0_ref);

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

    // Tensor cross-check at the f64 floor (the full-mesh sibling of
    // the small-mesh decode test, folded in here to avoid a second
    // multi-minute pipeline run under default `cargo test`).
    let golden_eps = fixture
        .input_c128("epsilon_tensor_diag")
        .expect("c128 input decodes");
    assert_eq!(burn.epsilon_tensor_diag_flat.len(), golden_eps.len());
    for (i, (got, want)) in burn
        .epsilon_tensor_diag_flat
        .iter()
        .zip(golden_eps.iter())
        .enumerate()
    {
        assert!(
            (got - want).norm() < 1.0e-14,
            "epsilon_tensor_diag[{i}]: Burn = {got}, NumPy = {want}"
        );
    }

    let n_request = burn.spurious_dim + 8;
    let solver = FaerComplexEigensolver;
    let burn_eigvals_faer = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("Burn complex eigensolve on full Mie fixture");
    let burn_eigvals: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&burn_eigvals_faer);

    let golden_full = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    assert_eq!(burn_eigvals.len(), golden_full.data.len());

    let n_spurious_ref = fixture.output_scalar("n_spurious_observed") as usize;
    assert_eq!(n_spurious_ref, burn.spurious_dim);
    let golden_physical = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("c128 physical decodes");
    let physical_take = golden_physical.data.len();
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
        eprintln!("sphere_mie complex-comparator report: {:#?}", report);
        panic!("sphere_mie full-mesh complex spectrum disagreed with NumPy baseline");
    }

    // 8 % TM_1,1 band + Q tripwire, both sides.
    let analytic_tm11_k = fixture.output_scalar("analytic_tm11_k");
    let burn_re_k = re_k_from_lambda(burn_physical[0]);
    let numpy_re_k = fixture.output_scalar("lowest_physical_re_k");
    let burn_rel_err = (burn_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    let numpy_rel_err = (numpy_re_k - analytic_tm11_k).abs() / analytic_tm11_k;
    assert!(burn_rel_err < TM11_REL_TOL);
    assert!(numpy_rel_err < TM11_REL_TOL);

    let burn_q = q_factor_from_lambda(burn_physical[0]);
    let numpy_q = fixture.output_scalar("q_factor_lowest_physical");
    assert!(burn_q > Q_LOWER_BAND_TM11);
    assert!(numpy_q > Q_LOWER_BAND_TM11);

    eprintln!(
        "sphere_mie full-mesh agreement: lowest Re(k): Burn {burn_re_k:.5} / NumPy \
         {numpy_re_k:.5} (analytic {analytic_tm11_k:.5}, rel err {:.2}% / {:.2}%); \
         Q: Burn {burn_q:.2} / NumPy {numpy_q:.2}",
        burn_rel_err * 100.0,
        numpy_rel_err * 100.0
    );
}
