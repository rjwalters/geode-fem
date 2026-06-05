//! Cross-IR sphere-PML loader/comparator smoke against the Julia baseline
//! (issue #147, Phase H.2).
//!
//! Loads `reference/fixtures/sphere_pml/julia_baseline.json` (the Julia
//! reference for the vector-Nédélec sphere-PML eigenmode pipeline) and asserts:
//!
//! 1. **Schema conformance**: the canonical v1 schema is satisfied; all
//!    declared input / output fields are present with the expected dtypes
//!    (`f64` for shape counts and the Q-factor, `c128` for the complex
//!    permittivity input and complex eigenvalue outputs).
//! 2. **c128 round-trip**: `Fixture::output_c128` and `Fixture::input_c128`
//!    decode the real-imag interleaved on-disk payload into
//!    `Vec<Complex64>` whose length matches `prod(shape)`.
//! 3. **σ₀ = 0 PEC-collapse**: the `eigenvalues_lowest_complex_sigma0` field
//!    agrees with the canonical Phase G.4 NumPy PEC `physical_eigenvalues`
//!    field at 1e-6 relative on `Re(λ)` and 1e-10 absolute on `Im(λ)`. This
//!    is the **structural correctness** check: with σ₀ = 0 the complex ε
//!    reduces to the real PEC case and the spectrum must collapse onto the
//!    Phase G.4 baseline.
//! 4. **σ₀ = 5 PML signature**: the `eigenvalues_lowest_complex` field has
//!    all `Im(λ) ≤ 0` (negative imaginary, per the exp(+jωt) absorbing
//!    convention) and the `q_factor_lowest_physical` is positive. This is
//!    the **physical-content** check: the PML actually absorbs.
//! 5. **Self-comparator round-trip**: the c128 comparator (#145 / PR #151)
//!    passes when actual == golden, exercising the Wave 1 infrastructure
//!    that this baseline is built against.
//!
//! # Cross-backend comparison (deferred)
//!
//! Comparison against the H.1 NumPy baseline (#146, in-flight) and the
//! Burn-side PML eigensolve (gated on `SparseComplexShiftInvertLanczos`
//! via `crates/geode-core/tests/sphere_pml_eigenmode.rs`) is deferred to
//! a follow-on test under the same fixture. The Julia baseline lands first
//! and is the **complex-arithmetic reference of record** for the PML phase
//! per Epic #88 principle 4.
//!
//! # Running
//!
//! The schema/loader/comparator tests run under the default `cargo test`:
//!
//! ```sh
//! cargo test -p geode-validation --test sphere_pml_julia_reference
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use geode_validation::diff::FieldStatus;
use geode_validation::{Complex64, Fixture, FixtureFormat};

/// Walk up from `CARGO_MANIFEST_DIR` to find the repo root.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find `reference/` directory walking up from {}",
        manifest.display()
    );
}

fn fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/sphere_pml/julia_baseline.json")
}

/// Phase G.4 NumPy PEC physical eigenvalues (from
/// `reference/fixtures/sphere_pec/baseline.json`). Used as the σ₀ = 0
/// PEC-collapse target.
const NUMPY_PEC_PHYSICAL: [f64; 5] = [
    1.4195415502066517,
    1.4204339541482647,
    1.4206625078898854,
    3.2718741181859423,
    3.277498156786518,
];

// ---------------------------------------------------------------------------
// Schema + loader smoke
// ---------------------------------------------------------------------------

#[test]
fn julia_pml_fixture_loads_with_canonical_schema() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    assert_eq!(fixture.fixture_id, "sphere_pml/n774_pml_eigenmode_julia");
    assert_eq!(fixture.schema_version, "1");

    // Required input fields.
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
            "Julia PML fixture missing required input `{key}`"
        );
    }
    assert_eq!(
        fixture.inputs["epsilon_r_complex"].dtype, "c128",
        "epsilon_r_complex must be c128 (the input-side complex contract)"
    );

    // Required output fields.
    for key in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "k_int_frobenius_complex",
        "m_int_frobenius_complex",
        "eigenvalues_lowest_complex",
        "eigenvalues_lowest_complex_sigma0",
        "q_factor_lowest_physical",
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "Julia PML fixture missing required output `{key}`"
        );
    }
    assert_eq!(
        fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128",
        "eigenvalues_lowest_complex must be c128"
    );
    assert_eq!(
        fixture.outputs["eigenvalues_lowest_complex_sigma0"].dtype, "c128",
        "eigenvalues_lowest_complex_sigma0 must be c128"
    );
    assert_eq!(
        fixture.outputs["q_factor_lowest_physical"].dtype, "f64",
        "q_factor_lowest_physical is a real-valued derived metric"
    );
}

#[test]
fn julia_pml_fixture_mesh_counts_match_reference() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    // The bundled sphere.msh is the same fixture used by sphere_pec/ —
    // these counts are invariant across Phase G / Phase H.
    let n_nodes = fixture.output_f64("n_nodes").unwrap().data[0];
    let n_tets = fixture.output_f64("n_tets").unwrap().data[0];
    let n_edges = fixture.output_f64("n_edges").unwrap().data[0];
    let n_int = fixture.output_f64("n_interior_edges").unwrap().data[0];
    let spurious_dim = fixture.output_f64("spurious_dim").unwrap().data[0];
    let n_sp_observed = fixture.output_f64("n_spurious_observed").unwrap().data[0];

    assert_eq!(n_nodes as usize, 774, "n_nodes (sphere mesh)");
    assert_eq!(n_tets as usize, 3335, "n_tets (sphere mesh)");
    assert_eq!(n_edges as usize, 4512, "n_edges (sphere mesh)");
    assert_eq!(n_int as usize, 3300, "n_interior_edges (PEC reduction)");
    assert_eq!(spurious_dim as usize, 368, "spurious_dim (interior nodes)");
    assert_eq!(
        n_sp_observed as usize, 368,
        "n_spurious_observed (d⁰ rank — unaffected by complex ε scaling)"
    );
}

// ---------------------------------------------------------------------------
// c128 round-trip
// ---------------------------------------------------------------------------

#[test]
fn julia_pml_epsilon_r_complex_decodes_to_n_tets_length() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let eps = fixture
        .input_c128("epsilon_r_complex")
        .expect("c128 input decodes");
    assert_eq!(
        eps.len(),
        3335,
        "epsilon_r_complex must have length n_tets = 3335"
    );

    // The dielectric region: ε_r = n² = 2.25 (purely real). At least
    // one tet must have this signature.
    let n_dielectric = eps
        .iter()
        .filter(|c| (c.re - 2.25).abs() < 1e-12 && c.im.abs() < 1e-12)
        .count();
    assert!(
        n_dielectric > 0,
        "expected at least one dielectric tet with ε_r = 2.25 + 0j, found 0"
    );

    // The PML shell: Im(ε_r) < 0 (PML absorption signature). At least
    // one tet must have non-trivial negative imaginary part.
    let n_pml = eps.iter().filter(|c| c.im < -1e-6).count();
    assert!(
        n_pml > 0,
        "expected PML-shell tets with Im(ε_r) < 0, found 0"
    );

    // Vacuum gap: ε_r = 1 (purely real). Everywhere outside the
    // dielectric and the absorbing shell.
    let n_vacuum = eps
        .iter()
        .filter(|c| (c.re - 1.0).abs() < 1e-12 && c.im.abs() < 1e-12)
        .count();
    assert!(
        n_vacuum > 0,
        "expected vacuum-gap tets with ε_r = 1 + 0j, found 0"
    );

    // Sum of region counts should equal n_tets.
    assert_eq!(
        n_dielectric + n_pml + n_vacuum,
        3335,
        "dielectric + PML + vacuum partition must cover all 3335 tets"
    );
}

#[test]
fn julia_pml_eigenvalues_lowest_complex_decodes() {
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let eigs = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    assert_eq!(
        eigs.numel(),
        5,
        "eigenvalues_lowest_complex declares shape [5]"
    );
    assert_eq!(eigs.data.len(), 5);

    // Every entry must be finite.
    for (i, lam) in eigs.data.iter().enumerate() {
        assert!(
            lam.re.is_finite() && lam.im.is_finite(),
            "eigenvalues_lowest_complex[{i}] = {lam:?} is not finite"
        );
    }
}

// ---------------------------------------------------------------------------
// Physical content checks
// ---------------------------------------------------------------------------

#[test]
fn julia_pml_sigma0_zero_collapses_to_numpy_pec_baseline() {
    // Structural correctness: with σ₀ = 0, the complex eigensolve must
    // recover the Phase G.4 NumPy PEC `physical_eigenvalues` to 1e-6
    // relative on Re(λ) and 1e-10 absolute on Im(λ).
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let eigs_pec = fixture
        .output_c128("eigenvalues_lowest_complex_sigma0")
        .expect("σ₀=0 collapse field decodes");
    assert_eq!(eigs_pec.data.len(), 5);

    for (i, (lam_jul, &lam_np)) in eigs_pec
        .data
        .iter()
        .zip(NUMPY_PEC_PHYSICAL.iter())
        .enumerate()
    {
        let re_err = (lam_jul.re - lam_np).abs() / lam_np.abs();
        assert!(
            re_err < 1e-6,
            "physical[{i}] Re(λ) mismatch vs NumPy PEC: Julia={}, NumPy={}, rel err {re_err:.3e}",
            lam_jul.re,
            lam_np
        );
        assert!(
            lam_jul.im.abs() < 1e-10,
            "physical[{i}] |Im(λ)| = {} > 1e-10 — σ₀=0 collapse should be real to LAPACK ULP",
            lam_jul.im.abs()
        );
    }
}

#[test]
fn julia_pml_sigma0_five_has_pml_absorption_signature() {
    // Physical-content correctness: with σ₀ = 5, all lowest physical
    // modes must have Im(λ) ≤ 0 (exp(+jωt) absorbing convention).
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let eigs = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    let q = fixture.output_f64("q_factor_lowest_physical").unwrap().data[0];

    for (i, lam) in eigs.data.iter().enumerate() {
        // Allow tiny positive imaginary at ULP for the most weakly
        // coupled trapped mode (the test passes if Im ≤ 1e-6).
        assert!(
            lam.im <= 1e-6,
            "physical[{i}] = {lam:?} has Im(λ) > 0 — sign-convention error or \
             un-filtered Arpack ghost mode"
        );
    }

    // Q-factor must be finite and positive (a sensible PML Q is in the
    // range ~1 to ~1000; the lowest trapped mode often shows Q > 100).
    assert!(q.is_finite(), "Q-factor must be finite, got {q}");
    assert!(
        q > 0.0,
        "Q-factor for the lowest physical mode must be positive (PML absorption), got {q}"
    );

    // Re(λ) must shift up from PEC baseline (1.42 triplet) — with σ₀=5
    // the PML coupling pulls the lowest physical band up to ~1.9. Loose
    // sanity bound: should be in [1.4, 3.0].
    let re_lowest = eigs.data[0].re;
    assert!(
        (1.4..3.0).contains(&re_lowest),
        "Re(λ_lowest_physical) = {re_lowest} outside [1.4, 3.0] sanity band for σ₀=5 PML"
    );
}

// ---------------------------------------------------------------------------
// Comparator round-trip
// ---------------------------------------------------------------------------

#[test]
fn julia_pml_complex_comparator_self_round_trip() {
    // Wave 1 (#145, PR #151) shipped the c128 comparator. This test
    // exercises it end-to-end on the Julia H.2 fixture: feeding the
    // golden values back as "actual" should produce a clean pass.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let mut complex_actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    for (name, output) in fixture.iter_outputs() {
        if output.dtype == "c128" {
            let g = fixture.output_c128(name).unwrap();
            complex_actual.insert(name.to_string(), g.data.clone());
        }
    }
    let report = fixture.compare_complex_against(&complex_actual);
    assert!(
        report.passed,
        "self-round-trip on c128 outputs should pass; report = {report:#?}"
    );
    // Every c128 field must report Ok.
    for diff in &report.fields {
        assert!(
            matches!(diff.status, FieldStatus::Ok),
            "c128 field `{}` did not pass self-round-trip; status = {:?}",
            diff.field,
            diff.status
        );
        let max_err = diff.max_abs_error.expect("Ok diff carries max_abs_error");
        assert!(
            max_err < 1e-15,
            "c128 field `{}` self-round-trip |Δ| = {max_err:.3e} should be < 1e-15",
            diff.field
        );
    }
}

#[test]
fn julia_pml_real_comparator_skips_c128_outputs() {
    // The real-valued `compare_against` path must skip c128 outputs so
    // a mixed-dtype fixture (like this one) compares cleanly. Feeds
    // back the f64-only outputs and asserts the real comparator passes
    // without complaining about the missing c128 fields.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");

    let mut real_actual: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for (name, output) in fixture.iter_outputs() {
        if output.dtype == "f64" {
            let g = fixture.output_f64(name).unwrap();
            real_actual.insert(name.to_string(), g.data.clone());
        }
    }
    let report = fixture.compare_against(&real_actual);
    assert!(
        report.passed,
        "real comparator self-round-trip on f64 outputs should pass; report = {report:#?}"
    );

    // No c128 field should appear in the real-comparator report.
    for diff in &report.fields {
        let dtype = fixture
            .outputs
            .get(&diff.field)
            .map(|o| o.dtype.as_str())
            .unwrap_or("?");
        assert_eq!(
            dtype, "f64",
            "real comparator should skip c128 fields; saw `{}` (dtype={})",
            diff.field, dtype
        );
    }
}
