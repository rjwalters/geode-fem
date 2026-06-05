//! Scaffolding smoke test for the Phase H sphere-PML fixture (issue
//! #145, parent epic #88).
//!
//! Covers:
//!
//!   1. Loading the stub `sphere_pml/baseline.json` fixture from disk
//!      and validating its schema-conformance (top-level keys, expected
//!      input/output field set, declared dtypes).
//!   2. Round-tripping the `c128`-dtype `eigenvalues_lowest_complex`
//!      output through `Fixture::output_c128` and asserting the
//!      real-imag interleaved on-disk encoding decodes to the expected
//!      `Vec<Complex64>`.
//!   3. Exercising the complex comparator on a small synthetic case:
//!         - actual == golden  ⇒ passes with `max_abs_error ≈ 0`,
//!         - actual + ε·i      ⇒ fails with `|Δ| = ε` reported on the
//!           `WorstOffender` and `n_violations = N`.
//!   4. Reading the `epsilon_r_complex` input field via
//!      `Fixture::input_c128` to confirm the input-side c128 path
//!      also works (per-backend impls #146/#147/#148 will consume this).
//!
//! Per the issue body, this is **scaffolding**: full Phase H numerics
//! (real PML eigensolve output) land with H.1 (#146), H.2 (#147), H.3
//! (#148). The fixture here is intentionally stub-quality so the
//! comparator machinery and `c128` API surface land first.

use std::collections::BTreeMap;
use std::path::PathBuf;

use geode_validation::diff::FieldStatus;
use geode_validation::{Complex64, Fixture, FixtureFormat};

/// Walk up from `CARGO_MANIFEST_DIR` to find the repo root (the
/// directory that contains `reference/`). Mirrors the helper in
/// `tests/smoke.rs`.
fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find a `reference/` directory walking up from {}",
        manifest.display()
    );
}

fn sphere_pml_fixture_path() -> PathBuf {
    repo_root().join("reference/fixtures/sphere_pml/baseline.json")
}

#[test]
fn stub_fixture_loads_and_has_expected_schema() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, "sphere_pml/n774_pml_eigenmode_stub");

    // Inputs declared by the schema doc.
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
            "missing input field '{key}' in sphere_pml stub fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_r_complex"].dtype, "c128");

    // Outputs declared by the schema doc.
    for key in [
        "n_nodes",
        "n_tets",
        "eigenvalues_lowest_complex",
        "q_factor_lowest_physical",
    ] {
        assert!(
            fixture.outputs.contains_key(key),
            "missing output field '{key}' in sphere_pml stub fixture"
        );
    }
    assert_eq!(
        fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128",
        "the c128 schema branch must be exercised by at least one output field"
    );
    assert_eq!(fixture.outputs["q_factor_lowest_physical"].dtype, "f64");

    // Sanity on the input mesh-derived counts: stub generator pulled
    // these from the bundled sphere.msh, so they're not synthetic.
    assert_eq!(fixture.outputs["n_nodes"].data, serde_json::json!([774.0]));
    assert_eq!(fixture.outputs["n_tets"].data, serde_json::json!([3335.0]));
}

#[test]
fn output_c128_decodes_real_imag_interleave() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    assert_eq!(golden.shape, &[2]);
    assert_eq!(golden.numel(), 2);
    assert_eq!(golden.tolerance_abs, 1.0e-6);

    // The stub generator wrote:
    //   entry 0 = 1e-13 + 0i  (spurious cluster sentinel)
    //   entry 1 = 1.42 - 0.1i (physical-band stub with PML loss)
    assert_eq!(golden.data.len(), 2);
    assert!((golden.data[0] - Complex64::new(1.0e-13, 0.0)).norm() < 1e-20);
    assert!((golden.data[1] - Complex64::new(1.42, -0.1)).norm() < 1e-15);
}

#[test]
fn input_c128_decodes_real_imag_interleave() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let eps = fixture
        .input_c128("epsilon_r_complex")
        .expect("c128 input decodes");

    // The stub generator wrote a 4-entry slice illustrating the three
    // mesh regions (dielectric / vacuum / PML ramp endpoint + mid).
    assert_eq!(eps.len(), 4);
    assert!((eps[0] - Complex64::new(2.25, 0.0)).norm() < 1e-15); // dielectric
    assert!((eps[1] - Complex64::new(1.0, 0.0)).norm() < 1e-15); // vacuum gap
    assert!((eps[2] - Complex64::new(1.0, -5.0)).norm() < 1e-15); // PML ramp endpoint
    assert!((eps[3] - Complex64::new(1.0, -1.25)).norm() < 1e-15); // PML mid-shell
}

#[test]
fn complex_comparator_passes_on_exact_match() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");

    // Build an "actual" map that exactly matches the golden values.
    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert("eigenvalues_lowest_complex".to_string(), golden.data.clone());

    let report = fixture.compare_complex_against(&actual);
    assert!(
        report.passed,
        "exact-match actual should pass complex comparator; report = {report:#?}"
    );

    let lam_diff = report
        .fields
        .iter()
        .find(|f| f.field == "eigenvalues_lowest_complex")
        .expect("eigenvalues_lowest_complex appears in complex report");
    assert!(lam_diff.passed);
    assert!(matches!(lam_diff.status, FieldStatus::Ok));
    let err = lam_diff
        .max_abs_error
        .expect("passed complex fields report max_abs_error");
    assert!(err < 1e-15, "exact match should give |Δ| ≈ 0, got {err}");

    // The f64-only outputs (n_nodes, n_tets, q_factor_lowest_physical)
    // do not appear in the complex report — they're handled by the
    // real-valued `compare` path. Confirms the dtype split is clean.
    assert!(report
        .fields
        .iter()
        .all(|f| f.field == "eigenvalues_lowest_complex"));
}

#[test]
fn complex_comparator_fails_on_imag_perturbation_with_correct_tolerance() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");

    // Perturb every entry by +1e-3 in the imaginary direction. The
    // fixture's per-field tolerance is 1e-6 on |Δ|, so 1e-3 is three
    // orders of magnitude over tolerance — every element should be a
    // violation, and the worst-offender's |Δ| should be ~1e-3.
    let perturbation = Complex64::new(0.0, 1.0e-3);
    let perturbed: Vec<Complex64> = golden.data.iter().map(|z| z + perturbation).collect();

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert("eigenvalues_lowest_complex".to_string(), perturbed);

    let report = fixture.compare_complex_against(&actual);
    assert!(
        !report.passed,
        "imag-perturbed actual should fail complex comparator; report = {report:#?}"
    );

    let lam_diff = report
        .fields
        .iter()
        .find(|f| f.field == "eigenvalues_lowest_complex")
        .expect("eigenvalues_lowest_complex appears in complex report");
    assert!(!lam_diff.passed);
    match &lam_diff.status {
        FieldStatus::ToleranceExceeded { n_violations } => {
            assert_eq!(
                *n_violations, 2,
                "both stub entries should exceed 1e-6 tolerance under a 1e-3 imag bump"
            );
        }
        other => panic!("expected ToleranceExceeded, got {other:?}"),
    }
    let worst = lam_diff
        .worst_offender
        .as_ref()
        .expect("worst offender recorded for complex tolerance failure");
    // |0 + 1e-3 i| = 1e-3
    assert!(
        (worst.abs_error - 1.0e-3).abs() < 1e-12,
        "worst-offender |Δ| should equal the injected imag perturbation, got {}",
        worst.abs_error
    );
}

#[test]
fn complex_comparator_reports_missing_field_when_actual_omitted() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    let report = fixture.compare_complex_against(&actual);
    assert!(!report.passed);

    let lam_diff = report
        .fields
        .iter()
        .find(|f| f.field == "eigenvalues_lowest_complex")
        .expect("eigenvalues_lowest_complex entry present in complex report");
    assert!(matches!(lam_diff.status, FieldStatus::MissingFromActual));
}

#[test]
fn complex_comparator_reports_shape_mismatch() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    // Only one entry — golden is length 2.
    actual.insert(
        "eigenvalues_lowest_complex".to_string(),
        vec![Complex64::new(1.42, -0.1)],
    );

    let report = fixture.compare_complex_against(&actual);
    assert!(!report.passed);
    let lam_diff = report
        .fields
        .iter()
        .find(|f| f.field == "eigenvalues_lowest_complex")
        .expect("eigenvalues_lowest_complex entry present");
    match &lam_diff.status {
        FieldStatus::ShapeMismatch { expected, actual } => {
            assert_eq!(*expected, 2);
            assert_eq!(*actual, 1);
        }
        other => panic!("expected ShapeMismatch, got {other:?}"),
    }
}

#[test]
fn real_comparator_skips_c128_fields_in_mixed_dtype_fixture() {
    // The real-valued `compare_against` path must skip `c128` outputs
    // (they belong to `compare_complex_against`). This pins the
    // dtype-aware splitting so a fixture mixing real + complex outputs
    // (like the Phase H one we're building) doesn't trip over its own
    // schema.
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml stub fixture should load");

    let mut actual: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    actual.insert("n_nodes".to_string(), vec![774.0]);
    actual.insert("n_tets".to_string(), vec![3335.0]);
    actual.insert("q_factor_lowest_physical".to_string(), vec![7.1]);
    // Deliberately do NOT supply eigenvalues_lowest_complex here —
    // the real comparator should skip the c128 field entirely.

    let report = fixture.compare_against(&actual);
    assert!(
        report.passed,
        "real comparator should pass on f64 outputs while skipping c128; report = {report:#?}"
    );
    // No c128 field should appear in the real-comparator report.
    assert!(report
        .fields
        .iter()
        .all(|f| f.field != "eigenvalues_lowest_complex"));
    // All three f64 fields should be present.
    for k in ["n_nodes", "n_tets", "q_factor_lowest_physical"] {
        assert!(
            report.fields.iter().any(|f| f.field == k),
            "real comparator missing field {k}"
        );
    }
}
