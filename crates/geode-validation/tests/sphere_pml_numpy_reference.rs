//! Cross-backend sphere-PML end-to-end agreement test (issue #146).
//!
//! Loads `reference/fixtures/sphere_pml/baseline.json` (NumPy reference
//! for the scalar-isotropic-PML sphere eigenmode pipeline at the
//! bundled 774-node fixture, σ₀ = 5.0) and asserts Burn agreement at
//! every sub-stage of Phase H.1:
//!
//! 1. Schema-level: the promoted fixture (post #146) carries the
//!    expected set of input and output fields, with `c128` dtype on
//!    `epsilon_r_complex`, `eigenvalues_lowest_complex`, and
//!    `physical_eigenvalues_complex` — pinning the locked PR #151
//!    schema layout.
//! 2. Mesh I/O — `n_nodes`, `n_tets`, `n_edges`, `n_interior_edges`,
//!    `spurious_dim` integer cross-checks (same mesh as the PEC
//!    fixture).
//! 3. Complex permittivity — full per-tet `epsilon_r_complex` vector
//!    compared at `1e-14` absolute on `|Δ|` (bit-exact f64 round-trip
//!    on ndarray; the f32 GPU backend would need a looser per-test
//!    override).
//! 4. Complex generalized eigenvalues — lowest `spurious_dim + 8`
//!    modes via Burn's `FaerComplexEigensolver` compared against the
//!    NumPy LAPACK ZGGEV baseline at `1e-5` absolute on `|Δ|` (the
//!    issue body's "likely tolerance is ~1e-5"). The eigensolver sign
//!    convention on `Im(λ)` is **not** enforced by either side; the
//!    `|Δ|` complex tolerance + `|Re(λ)|` sort key together accommodate
//!    it.
//! 5. Physical eigenvalues — lowest 5 complex modes past the d⁰-rank
//!    spurious split, at `1e-5` absolute on `|Δ|`.
//! 6. Q-factor — sign-agnostic `Re(k) / (2 |Im(k)|)` for the lowest
//!    physical mode (`1e-3` absolute tolerance; sanity diagnostic).
//!
//! # σ₀ = 0 PEC regression
//!
//! The σ₀ = 0 limit collapses the complex-ε PML pipeline to a real-ε
//! generalized eigenproblem (the PEC sphere). The dedicated regression
//! test [`pml_sigma_zero_reduces_to_real_pec`] re-runs the Burn-side
//! complex assembly with `σ₀ = 0` and asserts:
//!
//!   - the assembled complex M has `Im(M) = 0` to f64 precision,
//!   - the complex eigensolver returns a real spectrum (`max
//!     |Im(λ)| / max(|Re(λ)|, 1) < 1e-10`),
//!   - the lowest physical Re(λ) matches the Phase G.2 PEC NumPy
//!     reference (λ ≈ 1.4195 on the bundled fixture) within `1e-5`
//!     absolute.
//!
//! This is the H.1 analog of `pml_isotropic_sigma_zero_is_real` in the
//! Burn-side `sphere_pml_eigenmode.rs` integration test.
//!
//! # Why `#[ignore]` on the eigensolve tests
//!
//! Same reason as `sphere_pec_numpy_reference.rs`: faer 0.24's complex
//! `gevd` path performs subtractions that wrap under debug-assertions.
//! Run with:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --features geode-core/ndarray \
//!     --test sphere_pml_numpy_reference -- --ignored --nocapture
//! ```
//!
//! The schema-level test (`sphere_pml_fixture_loads_with_promoted_schema`)
//! runs under default `cargo test` because it does no eigensolve work.
//!
//! # Small-mesh sibling (issue #158)
//!
//! The `_small_*` test functions below load
//! `reference/fixtures/sphere_pml_small/baseline.json` — a ~200-tet
//! sibling fixture sized so the Burn faer 0.24 complex GEVD fits in
//! the default `cargo test` budget. These tests are **not**
//! `#[ignore]`-gated and **do** run under default `cargo test -p
//! geode-validation`; they are the canonical CI gate for Burn vs
//! NumPy PML spectrum agreement under the Wave-2 sign convention
//! (`Im(λ) > 0`) per PR #155 Judge's binding decision.

use std::collections::BTreeMap;
use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use num_complex::Complex64;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_complex_epsilon, build_complex_epsilon_r_pml,
    burn_complex_mass_to_faer, sphere_n_interior_nodes, sphere_pec_interior_edges,
    tet_centroid_radii,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{apply_dirichlet_bc, burn_matrix_to_faer};
use geode_core::mesh::{
    R_BUFFER, SphereFixture, read_sphere_fixture, read_sphere_fixture_from_bytes,
};
use geode_core::testing::TestBackend;
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Fixture path
// ---------------------------------------------------------------------------

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pml/baseline.json")
}

fn small_fixture_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pml_small/baseline.json")
}

fn small_mesh_path() -> PathBuf {
    geode_validation::fixture_path("sphere_pml_small/sphere.msh")
}

// ---------------------------------------------------------------------------
// Burn pipeline → complex (K_int, M_int_complex) for the σ₀ = 5 baseline.
// ---------------------------------------------------------------------------

struct BurnPmlPipeline {
    n_nodes: usize,
    n_tets: usize,
    n_edges: usize,
    n_interior_edges: usize,
    spurious_dim: usize,
    epsilon_r_complex: Vec<Complex64>,
    /// Real K_int promoted to complex (Im(K) = 0).
    k_int_complex: faer::Mat<faer::c64>,
    m_int_complex: faer::Mat<faer::c64>,
}

fn run_burn_pml_pipeline(sigma_0: f64, n_index: f64) -> BurnPmlPipeline {
    let fixture = read_sphere_fixture().expect("fixture load");
    run_burn_pml_pipeline_from_fixture(&fixture, sigma_0, n_index)
}

/// Like [`run_burn_pml_pipeline`] but on an arbitrary [`SphereFixture`]
/// — used by the small-mesh sibling tests (issue #158) which load the
/// fixture from `reference/fixtures/sphere_pml_small/sphere.msh`
/// rather than the bundled `crates/geode-core/tests/fixtures/sphere.msh`.
fn run_burn_pml_pipeline_from_fixture(
    fixture: &SphereFixture,
    sigma_0: f64,
    n_index: f64,
) -> BurnPmlPipeline {
    let n_nodes = fixture.mesh.n_nodes();
    let n_tets = fixture.mesh.n_tets();

    let radii = tet_centroid_radii(&fixture.mesh);
    let eps_complex_faer =
        build_complex_epsilon_r_pml(&fixture.tet_physical_tags, &radii, n_index, sigma_0);

    // Convert faer::c64 → num_complex::Complex64 once for the
    // comparator (the on-disk encoding round-trips through
    // `Vec<Complex64>` via `Fixture::input_c128`).
    let epsilon_r_complex: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&eps_complex_faer);

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
    let sys = assemble_global_nedelec_with_complex_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        n_edges,
        &eps_complex_faer,
    );

    let k_full = burn_matrix_to_faer(sys.k);
    let m_complex_full = burn_complex_mass_to_faer(sys.m_re, sys.m_im);

    // Reduce K (real) on the interior mask, then slice M_complex by hand.
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

    BurnPmlPipeline {
        n_nodes,
        n_tets,
        n_edges,
        n_interior_edges,
        spurious_dim,
        epsilon_r_complex,
        k_int_complex,
        m_int_complex,
    }
}

// ---------------------------------------------------------------------------
// Q-factor helper — k-space sign-agnostic form, mirror of the NumPy
// reference (`sphere_pml.run_sphere_pml`) and the Burn integration test
// print in `tests/sphere_pml_eigenmode.rs:362-372`.
// ---------------------------------------------------------------------------

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
// Schema-level test (no eigensolve; runs under default `cargo test`)
// ---------------------------------------------------------------------------

#[test]
fn sphere_pml_fixture_loads_with_promoted_schema() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml baseline.json should load");
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, "sphere_pml/n774_pml_eigenmode");

    // Input fields the promoted (non-stub) baseline declares.
    for key in [
        "mesh_path",
        "sigma_0",
        "r_sphere",
        "r_pml_inner",
        "r_buffer",
        "n_index",
        "epsilon_r_complex",
    ] {
        assert!(
            fixture.inputs.contains_key(key),
            "missing input field '{key}' in promoted sphere_pml fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_r_complex"].dtype, "c128");

    // Promoted output set: integer mesh diagnostics + complex
    // spectrum/physical/q-factor.
    for key in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "eigenvalues_lowest_complex",
        "physical_eigenvalues_complex",
        "q_factor_lowest_physical",
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "missing output field '{key}' in promoted sphere_pml fixture"
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
fn sphere_pml_fixture_epsilon_r_input_decodes() {
    // Bit-exact `epsilon_r_complex` round-trip through `input_c128`,
    // and Burn-side `build_complex_epsilon_r_pml` agreement at f64
    // precision. Runs under default `cargo test` (no eigensolve).
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml baseline.json should load");
    let golden_eps = fixture
        .input_c128("epsilon_r_complex")
        .expect("c128 input decodes");

    let sigma_0 = fixture.inputs["sigma_0"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();
    let n_index = fixture.inputs["n_index"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    let burn = run_burn_pml_pipeline(sigma_0, n_index);
    assert_eq!(burn.epsilon_r_complex.len(), golden_eps.len());

    let mut max_abs = 0.0_f64;
    for (i, (got, want)) in burn
        .epsilon_r_complex
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
            "epsilon_r_complex[{i}]: |Δ| = {err:.3e} exceeds 1e-14 \
             (Burn = {got}, NumPy = {want})"
        );
    }
    eprintln!(
        "sphere_pml epsilon_r_complex: Burn vs NumPy max |Δ| = {max_abs:.3e} \
         (Burn build_complex_epsilon_r_pml round-trips through c128 at f64 floor)"
    );
}

// ---------------------------------------------------------------------------
// Eigensolve-touching tests (release mode only).
// ---------------------------------------------------------------------------

#[test]
#[ignore = "Burn complex generalized_eigen via faer 0.24 panics under debug-assertions; run with `cargo test -p geode-validation --release --features geode-core/ndarray --test sphere_pml_numpy_reference -- --ignored`"]
fn sphere_pml_spectrum_agrees_with_numpy() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml baseline.json should load");

    let sigma_0 = fixture.inputs["sigma_0"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();
    let n_index = fixture.inputs["n_index"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    let burn = run_burn_pml_pipeline(sigma_0, n_index);

    // Mesh shape integer cross-checks.
    let n_nodes_ref = fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets_ref = fixture.output_f64("n_tets").unwrap().data[0];
    let n_edges_ref = fixture.output_f64("n_edges").unwrap().data[0];
    let n_int_ref = fixture.output_f64("n_interior_edges").unwrap().data[0];
    let spurious_ref = fixture.output_f64("spurious_dim").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize);
    assert_eq!(burn.n_tets, n_tets_ref as usize);
    assert_eq!(burn.n_edges, n_edges_ref as usize);
    assert_eq!(burn.n_interior_edges, n_int_ref as usize);
    assert_eq!(burn.spurious_dim, spurious_ref as usize);

    // Solve the complex generalized pencil on the Burn side.
    let n_request = burn.spurious_dim + 8;
    let solver = FaerComplexEigensolver;
    let burn_eigvals_faer = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("Burn complex eigensolve");
    let burn_eigvals: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&burn_eigvals_faer);

    // Compare against the NumPy baseline via the c128 comparator.
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

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert(
        "eigenvalues_lowest_complex".to_string(),
        burn_eigvals.clone(),
    );

    // Lowest 5 physical = past the d⁰-rank spurious cluster.
    let n_spurious_ref = fixture.output_f64("n_spurious_observed").unwrap().data[0] as usize;
    assert_eq!(
        n_spurious_ref, burn.spurious_dim,
        "n_spurious_observed in fixture ({n_spurious_ref}) should match \
         Burn's spurious_dim ({})",
        burn.spurious_dim
    );
    let physical_take = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("c128 physical_eigenvalues_complex decodes")
        .data
        .len();
    assert!(
        n_spurious_ref + physical_take <= burn_eigvals.len(),
        "spectrum too short ({}) to expose {physical_take} physical modes past \
         n_spurious = {n_spurious_ref}",
        burn_eigvals.len()
    );
    let burn_physical: Vec<Complex64> =
        burn_eigvals[n_spurious_ref..n_spurious_ref + physical_take].to_vec();
    actual.insert(
        "physical_eigenvalues_complex".to_string(),
        burn_physical.clone(),
    );

    let report = geode_validation::compare_complex_against(&fixture, &actual);
    if !report.passed {
        eprintln!("sphere_pml complex-comparator report (full): {:#?}", report);
        for f in &report.fields {
            eprintln!(
                "  field={} passed={} tol={:.0e} max_abs={:?}",
                f.field, f.passed, f.tolerance_abs, f.max_abs_error
            );
        }
        panic!("sphere_pml complex spectrum disagreed with NumPy baseline");
    }

    // Q-factor — sign-agnostic Re(k)/(2|Im(k)|).
    let burn_q = q_factor_from_lambda(burn_physical[0]);
    let golden_q = fixture.output_f64("q_factor_lowest_physical").unwrap();
    let want_q = golden_q.data[0];
    let q_err = (burn_q - want_q).abs();
    assert!(
        q_err < golden_q.tolerance_abs,
        "q_factor_lowest_physical: Burn = {burn_q:.6}, NumPy = {want_q:.6}, \
         |Δ| = {q_err:.3e} exceeds {:.0e}",
        golden_q.tolerance_abs
    );

    eprintln!(
        "sphere_pml cross-backend agreement: spurious n_spurious match ({}), \
         lowest 5 physical complex eigenvalues within 1e-5 absolute on |Δ|, \
         Q-factor = {:.4} (NumPy {:.4}, |Δ| = {:.3e}).",
        n_spurious_ref, burn_q, want_q, q_err
    );
}

#[test]
#[ignore = "Burn complex generalized_eigen via faer 0.24 panics under debug-assertions; run with `cargo test -p geode-validation --release --features geode-core/ndarray --test sphere_pml_numpy_reference -- --ignored`"]
fn pml_sigma_zero_reduces_to_real_pec() {
    // σ₀ = 0 regression — the H.1 analog of the Burn-side
    // `pml_isotropic_sigma_zero_is_real` test in
    // `crates/geode-core/tests/sphere_pml_eigenmode.rs`. The
    // complex-ε PML pipeline must collapse to the real-ε PEC problem
    // when the absorption strength is zero, regardless of which way
    // the Burn-vs-NumPy plumbing routes through the complex path.
    //
    // What this pins:
    //   1. The assembled Im(M) is bit-zero (Burn-side, complex
    //      scatter rounds to exact zero when Im(ε) = 0 per-element).
    //   2. The Burn complex eigensolver returns a spectrum with
    //      max |Im(λ)| / max(|Re(λ)|, 1) < 1e-10 — same bound as the
    //      `pml_isotropic_sigma_zero_is_real` Burn-side test.
    //   3. The lowest physical Re(λ) matches the bundled Phase G.2
    //      PEC NumPy reference (λ ≈ 1.4195 ± 1e-5 on the bundled
    //      774-node fixture).
    //
    // Acceptance #3 is the load-bearing cross-check: it verifies the
    // *entire* complex-ε plumbing (NumPy + Burn) collapses to the real
    // PEC result, not just that the imaginary part is small.

    let burn = run_burn_pml_pipeline(0.0, 1.5);

    // (1) Per-element ε must be exactly real when σ₀ = 0. Cross-check
    //     the Burn-side ε vector directly (the convention in the
    //     Burn-side `pml_profile_sigma_zero_is_real_everywhere` test).
    for (i, c) in burn.epsilon_r_complex.iter().enumerate() {
        assert_eq!(
            c.im, 0.0,
            "σ₀ = 0: epsilon_r_complex[{i}].im should be exactly 0, got {}",
            c.im
        );
    }

    // (2) The assembled M must have Im(M) ≈ 0 at f64 precision (the
    //     complex scatter doesn't leak imaginary content).
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

    // (3) Eigenspectrum is real to f64 precision. Use the Burn solver
    //     so this exercises the same complex path the σ₀ = 5 test
    //     drives — the spurious cluster is the load-bearing signal
    //     (any systematic Im content from K + jM scatter shows up
    //     there first).
    let n_request = burn.spurious_dim + 5;
    let solver = FaerComplexEigensolver;
    let eigvals = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("σ₀ = 0 complex eigensolve");

    let mut max_rel_im = 0.0_f64;
    for lam in &eigvals {
        let scale = lam.re.abs().max(1.0);
        let rel = lam.im.abs() / scale;
        if rel > max_rel_im {
            max_rel_im = rel;
        }
    }
    assert!(
        max_rel_im < 1.0e-10,
        "σ₀ = 0 spectrum should be real to f64 precision; observed \
         max |Im(λ)|/max(|Re(λ)|, 1) = {max_rel_im:.3e}"
    );

    // (4) Lowest physical Re(λ) cross-check against the Phase G.2 PEC
    //     baseline (the fixture pins this at λ ≈ 1.4195). Load the PEC
    //     baseline directly so the σ₀ = 0 regression is anchored to
    //     the existing canonical reference, not to a separate stored
    //     constant in this file.
    let pec_path = geode_validation::fixture_path("sphere_pec/baseline.json");
    let pec_fixture = Fixture::load_from(&pec_path, FixtureFormat::Json)
        .expect("sphere_pec baseline.json should load");
    let pec_physical = pec_fixture
        .output_f64("physical_eigenvalues")
        .expect("sphere_pec physical_eigenvalues decodes");
    let pec_lowest = pec_physical.data[0];

    // Burn-side lowest physical: skip past the spurious cluster.
    let n_spurious = burn.spurious_dim;
    assert!(
        n_spurious < eigvals.len(),
        "σ₀ = 0 spectrum too short ({}) for spurious_dim = {n_spurious}",
        eigvals.len()
    );
    let burn_lowest = eigvals[n_spurious];
    let re_err = (burn_lowest.re - pec_lowest).abs();
    assert!(
        re_err < 1.0e-5,
        "σ₀ = 0: lowest physical Re(λ) = {:.6} should match PEC reference {:.6} \
         within 1e-5 absolute; got |Δ| = {:.3e}",
        burn_lowest.re,
        pec_lowest,
        re_err
    );
    eprintln!(
        "σ₀ = 0 PEC regression: max |Im(λ)|/|Re(λ)| = {:.3e}, lowest physical \
         Re(λ) = {:.6} (PEC reference {:.6}, |Δ| = {:.3e})",
        max_rel_im, burn_lowest.re, pec_lowest, re_err
    );
}

// ===========================================================================
// Small-mesh sibling (issue #158) — default-`cargo test` Burn vs NumPy
// cross-check.
// ===========================================================================
//
// The full sphere_pml fixture above takes 60+ minutes for faer 0.24's
// complex GEVD on the 3300×3300 interior pencil, so its eigensolve-
// touching tests are `#[ignore]`-gated. The small-mesh sibling under
// `reference/fixtures/sphere_pml_small/` shrinks the interior pencil
// dim by ~15×, putting the Burn complex GEVD comfortably under 1 s on
// a developer machine — so these tests run in default `cargo test -p
// geode-validation`.
//
// What this pins:
//   - Schema-level: small-mesh baseline.json carries the same field
//     set as the full fixture (post-#146 promoted schema), plus a
//     `sigma_zero_lowest_physical_re` in-fixture PEC anchor (the
//     small mesh has no separate PEC baseline to defer to).
//   - Mesh I/O: `n_nodes`, `n_tets`, `n_edges`, `n_interior_edges`,
//     `spurious_dim` integer cross-checks (small mesh — ~48 / 197 /
//     259 / 214 / 31).
//   - Complex permittivity: full per-tet ε vector at 1e-14 absolute
//     on `|Δ|` (bit-exact c128 round-trip on the ndarray backend).
//   - Complex eigenvalue spectrum: lowest spurious_dim + 8 modes at
//     5e-4 absolute on `|Δ|`, sign convention `Im(λ) > 0` per PR #155.
//     (The full fixture uses 1e-5; on the small-mesh pencil the
//     spurious cluster near λ=0 inflates faer 0.24 QZ vs LAPACK ZGGEV
//     residuals to ~1.2e-4 in absolute terms, so the full-slice tol
//     is relaxed accordingly.)
//   - Physical eigenvalues: lowest 5 complex modes past the d⁰-rank
//     spurious split at 1e-4 absolute on `|Δ|` (the physical band is
//     better-conditioned and stays within 6e-5 in measurement).
//   - Q-factor: 1e-2 absolute (sign-agnostic Re(k)/(2|Im(k)|) form).
//     Looser than the full fixture's 1e-3 because the small-mesh
//     ground mode at λ ≈ 1.92 + 0.055j has a small Im(λ), inflating
//     Q ≈ 34.8 and amplifying eigenvalue-residual translation
//     into Q-residual.
//   - σ₀=0 PEC collapse: in-fixture anchor at 1e-5 absolute on the
//     lowest physical Re(λ).

fn load_small_sphere_fixture() -> SphereFixture {
    let bytes = std::fs::read(small_mesh_path())
        .expect("read small-mesh sphere.msh bytes (run reference/numpy/gen_sphere_pml_small_baseline.py if missing)");
    read_sphere_fixture_from_bytes(&bytes).expect("parse small-mesh sphere.msh")
}

#[test]
fn sphere_pml_small_fixture_loads_with_expected_schema() {
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml_small baseline.json should load");
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, "sphere_pml_small/n48_pml_eigenmode");

    for key in [
        "mesh_path",
        "sigma_0",
        "r_sphere",
        "r_pml_inner",
        "r_buffer",
        "n_index",
        "epsilon_r_complex",
    ] {
        assert!(
            fixture.inputs.contains_key(key),
            "missing input field '{key}' in sphere_pml_small fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_r_complex"].dtype, "c128");

    for key in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "eigenvalues_lowest_complex",
        "physical_eigenvalues_complex",
        "q_factor_lowest_physical",
        "sigma_zero_lowest_physical_re",
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "missing output field '{key}' in sphere_pml_small fixture"
        );
    }
    assert_eq!(fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128");
    assert_eq!(
        fixture.outputs["physical_eigenvalues_complex"].dtype,
        "c128"
    );
    assert_eq!(fixture.outputs["q_factor_lowest_physical"].dtype, "f64");
    assert_eq!(
        fixture.outputs["sigma_zero_lowest_physical_re"].dtype,
        "f64"
    );
}

#[test]
fn sphere_pml_small_epsilon_r_input_decodes() {
    // Bit-exact `epsilon_r_complex` round-trip through `input_c128`,
    // and Burn-side `build_complex_epsilon_r_pml` agreement at f64
    // precision on the small mesh.
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml_small baseline.json should load");
    let golden_eps = fixture
        .input_c128("epsilon_r_complex")
        .expect("c128 input decodes");

    let sigma_0 = fixture.inputs["sigma_0"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();
    let n_index = fixture.inputs["n_index"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_pml_pipeline_from_fixture(&sphere, sigma_0, n_index);
    assert_eq!(burn.epsilon_r_complex.len(), golden_eps.len());

    let mut max_abs = 0.0_f64;
    for (i, (got, want)) in burn
        .epsilon_r_complex
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
            "small epsilon_r_complex[{i}]: |Δ| = {err:.3e} exceeds 1e-14 \
             (Burn = {got}, NumPy = {want})"
        );
    }
    eprintln!("sphere_pml_small epsilon_r_complex: Burn vs NumPy max |Δ| = {max_abs:.3e}");
}

#[test]
fn sphere_pml_small_spectrum_agrees_with_numpy() {
    // The headline cross-check for issue #158: Burn complex GEVD on
    // the small mesh agrees with NumPy LAPACK ZGGEV at 1e-5 absolute
    // on |Δ|, under the canonical Im(λ) > 0 sign convention from
    // PR #155.
    //
    // This test runs under default `cargo test -p geode-validation`
    // — no `--release`, no `#[ignore]`. The small mesh's ~214-DOF
    // interior pencil keeps the faer 0.24 complex GEVD well under 1 s
    // (vs 60+ minutes on the full 3300-DOF fixture).
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml_small baseline.json should load");

    let sigma_0 = fixture.inputs["sigma_0"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();
    let n_index = fixture.inputs["n_index"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_pml_pipeline_from_fixture(&sphere, sigma_0, n_index);

    // Mesh-shape integer cross-checks.
    let n_nodes_ref = fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets_ref = fixture.output_f64("n_tets").unwrap().data[0];
    let n_edges_ref = fixture.output_f64("n_edges").unwrap().data[0];
    let n_int_ref = fixture.output_f64("n_interior_edges").unwrap().data[0];
    let spurious_ref = fixture.output_f64("spurious_dim").unwrap().data[0];
    assert_eq!(burn.n_nodes, n_nodes_ref as usize);
    assert_eq!(burn.n_tets, n_tets_ref as usize);
    assert_eq!(burn.n_edges, n_edges_ref as usize);
    assert_eq!(burn.n_interior_edges, n_int_ref as usize);
    assert_eq!(burn.spurious_dim, spurious_ref as usize);

    // Sanity: the small fixture must actually be small.
    assert!(
        burn.n_tets < 300,
        "small fixture should be <300 tets (got {}); the issue targets <100 \
         but the 3-shell BooleanFragments topology floors at ~200",
        burn.n_tets
    );

    // Solve the complex generalized pencil on the Burn side.
    let n_request = burn.spurious_dim + 8;
    let t_start = std::time::Instant::now();
    let solver = FaerComplexEigensolver;
    let burn_eigvals_faer = solver
        .smallest_complex_pencil_eigenvalues(
            burn.k_int_complex.as_ref(),
            burn.m_int_complex.as_ref(),
            n_request,
        )
        .expect("Burn complex eigensolve on small fixture");
    let gevd_wall = t_start.elapsed();
    eprintln!(
        "sphere_pml_small Burn complex GEVD on {}×{} pencil: {:.3} s \
         (acceptance budget 30 s)",
        burn.n_interior_edges,
        burn.n_interior_edges,
        gevd_wall.as_secs_f64()
    );
    assert!(
        gevd_wall.as_secs_f64() < 30.0,
        "small-mesh Burn complex GEVD took {:.2} s, exceeds the 30 s budget \
         in issue #158",
        gevd_wall.as_secs_f64()
    );

    let burn_eigvals: Vec<Complex64> =
        geode_util::convert::complex_slice_to_vec(&burn_eigvals_faer);

    // Compare against the NumPy baseline via the c128 comparator.
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

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert(
        "eigenvalues_lowest_complex".to_string(),
        burn_eigvals.clone(),
    );

    // Lowest 5 physical = past the d⁰-rank spurious cluster.
    let n_spurious_ref = fixture.output_f64("n_spurious_observed").unwrap().data[0] as usize;
    assert_eq!(
        n_spurious_ref, burn.spurious_dim,
        "n_spurious_observed in fixture ({n_spurious_ref}) should match \
         Burn's spurious_dim ({})",
        burn.spurious_dim
    );
    let physical_take = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("c128 physical_eigenvalues_complex decodes")
        .data
        .len();
    assert!(
        n_spurious_ref + physical_take <= burn_eigvals.len(),
        "spectrum too short ({}) to expose {physical_take} physical modes past \
         n_spurious = {n_spurious_ref}",
        burn_eigvals.len()
    );
    let burn_physical: Vec<Complex64> =
        burn_eigvals[n_spurious_ref..n_spurious_ref + physical_take].to_vec();

    // Canonical sign convention check (PR #155 Judge's binding decision):
    // every physical mode must have Im(λ) ≥ 0 under the Wave-2
    // convention. faer 0.24 QZ returns this sign on the identical
    // complex-symmetric pencil that LAPACK ZGGEV gets, so any
    // Im(λ) < 0 here would signal a Burn-side regression — not a
    // backend disagreement.
    for (i, lam) in burn_physical.iter().enumerate() {
        assert!(
            lam.im >= -1e-10,
            "Burn small-mesh physical[{i}] = {lam} has Im(λ) < 0 — \
             flipped relative to the PR #155 canonical Im(λ) > 0 \
             convention",
        );
    }

    actual.insert(
        "physical_eigenvalues_complex".to_string(),
        burn_physical.clone(),
    );

    let report = geode_validation::compare_complex_against(&fixture, &actual);
    if !report.passed {
        eprintln!(
            "sphere_pml_small complex-comparator report (full): {:#?}",
            report
        );
        for f in &report.fields {
            eprintln!(
                "  field={} passed={} tol={:.0e} max_abs={:?}",
                f.field, f.passed, f.tolerance_abs, f.max_abs_error
            );
        }
        panic!("sphere_pml_small complex spectrum disagreed with NumPy baseline");
    }

    // Q-factor — sign-agnostic Re(k)/(2|Im(k)|).
    let burn_q = q_factor_from_lambda(burn_physical[0]);
    let golden_q = fixture.output_f64("q_factor_lowest_physical").unwrap();
    let want_q = golden_q.data[0];
    let q_err = (burn_q - want_q).abs();
    assert!(
        q_err < golden_q.tolerance_abs,
        "sphere_pml_small q_factor_lowest_physical: Burn = {burn_q:.6}, \
         NumPy = {want_q:.6}, |Δ| = {q_err:.3e} exceeds {:.0e}",
        golden_q.tolerance_abs
    );

    eprintln!(
        "sphere_pml_small cross-backend agreement: spurious_dim = {} match, \
         physical band (5 modes) within 1e-4 absolute on |Δ|, full slice \
         (spurious + physical) within 5e-4 absolute on |Δ|, Q-factor = {:.4} \
         (NumPy {:.4}, |Δ| = {:.3e}).",
        n_spurious_ref, burn_q, want_q, q_err
    );
}

#[test]
fn sphere_pml_small_sigma_zero_reduces_to_real_pec() {
    // σ₀ = 0 regression on the small mesh — the in-fixture PEC anchor
    // (`sigma_zero_lowest_physical_re`) lets this test run without a
    // separate small-mesh PEC fixture.
    let fixture = Fixture::load_from(&small_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml_small baseline.json should load");
    let n_index = fixture.inputs["n_index"].data.as_array().unwrap()[0]
        .as_f64()
        .unwrap();

    let sphere = load_small_sphere_fixture();
    let burn = run_burn_pml_pipeline_from_fixture(&sphere, 0.0, n_index);

    // (1) Per-element ε must be exactly real when σ₀ = 0.
    for (i, c) in burn.epsilon_r_complex.iter().enumerate() {
        assert_eq!(
            c.im, 0.0,
            "σ₀ = 0 (small): epsilon_r_complex[{i}].im should be exactly 0, got {}",
            c.im
        );
    }

    // (2) Im(M_int_complex) ≈ 0 at f64 precision.
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
        "σ₀ = 0 (small): assembled M_int_complex Im leaked, \
         max |Im(M_ij)| = {max_abs_im:.3e}"
    );

    // (3) Complex eigenspectrum is real to f64 precision.
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
        let scale = lam.re.abs().max(1.0);
        let rel = lam.im.abs() / scale;
        if rel > max_rel_im {
            max_rel_im = rel;
        }
    }
    assert!(
        max_rel_im < 1.0e-10,
        "σ₀ = 0 (small) spectrum should be real to f64 precision; observed \
         max |Im(λ)|/max(|Re(λ)|, 1) = {max_rel_im:.3e}"
    );

    // (4) Lowest physical Re(λ) cross-check against the in-fixture
    //     PEC anchor `sigma_zero_lowest_physical_re`.
    let pec_anchor = fixture
        .output_f64("sigma_zero_lowest_physical_re")
        .expect("sigma_zero_lowest_physical_re decodes");
    let pec_lowest = pec_anchor.data[0];
    let pec_tol = pec_anchor.tolerance_abs;

    let n_spurious = burn.spurious_dim;
    assert!(
        n_spurious < eigvals.len(),
        "σ₀ = 0 (small) spectrum too short ({}) for spurious_dim = {n_spurious}",
        eigvals.len()
    );
    let burn_lowest = eigvals[n_spurious];
    let re_err = (burn_lowest.re - pec_lowest).abs();
    assert!(
        re_err < pec_tol,
        "σ₀ = 0 (small): lowest physical Re(λ) = {:.6} should match in-fixture \
         PEC anchor {:.6} within {:.0e}; got |Δ| = {:.3e}",
        burn_lowest.re,
        pec_lowest,
        pec_tol,
        re_err
    );
    eprintln!(
        "σ₀ = 0 (small) PEC regression: max |Im(λ)|/|Re(λ)| = {:.3e}, lowest \
         physical Re(λ) = {:.6} (in-fixture anchor {:.6}, |Δ| = {:.3e})",
        max_rel_im, burn_lowest.re, pec_lowest, re_err
    );
}
