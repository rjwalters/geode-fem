//! Schema-level smoke test for the Phase H sphere-PML fixture (issues
//! #145 / #146, parent epic #88).
//!
//! Originally introduced as scaffolding for #145 (when the fixture was
//! stub-quality). After the H.1 promotion (#146) the fixture carries
//! full numerical content from the NumPy reference, but the comparator
//! / schema-level checks remain useful as fast (no-eigensolve) gates.
//!
//! Covers:
//!
//!   1. Loading the promoted `sphere_pml/baseline.json` fixture and
//!      validating its schema-conformance (top-level keys, expected
//!      input/output field set, declared dtypes).
//!   2. Round-tripping the `c128`-dtype `eigenvalues_lowest_complex`
//!      output through `Fixture::output_c128` and asserting the
//!      real-imag interleaved on-disk encoding decodes cleanly.
//!   3. Exercising the complex comparator on the loaded golden:
//!         - actual == golden  ⇒ passes with `max_abs_error ≈ 0`,
//!         - actual + ε·i      ⇒ fails with `|Δ| = ε` reported on the
//!           `WorstOffender` and `n_violations = N`.
//!   4. Reading the `epsilon_r_complex` input field via
//!      `Fixture::input_c128` to confirm the input-side c128 path
//!      also works (the full [n_tets] vector post-promotion).

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
    assert_eq!(fixture.fixture_id, "sphere_pml/n774_pml_eigenmode");

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
            "missing input field '{key}' in sphere_pml fixture"
        );
    }
    assert_eq!(fixture.inputs["epsilon_r_complex"].dtype, "c128");

    // Outputs declared by the schema doc (promoted, post-#146).
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
            "missing output field '{key}' in sphere_pml fixture"
        );
    }
    assert_eq!(
        fixture.outputs["eigenvalues_lowest_complex"].dtype, "c128",
        "the c128 schema branch must be exercised by at least one output field"
    );
    assert_eq!(fixture.outputs["physical_eigenvalues_complex"].dtype, "c128");
    assert_eq!(fixture.outputs["q_factor_lowest_physical"].dtype, "f64");

    // Sanity on the input mesh-derived counts: pulled from the bundled
    // sphere.msh, so they're not synthetic.
    assert_eq!(fixture.outputs["n_nodes"].data, serde_json::json!([774.0]));
    assert_eq!(fixture.outputs["n_tets"].data, serde_json::json!([3335.0]));
}

#[test]
fn output_c128_decodes_real_imag_interleave() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    // Post-#146 promotion: full `spurious_dim + 8 = 376`-entry spectrum.
    // We do not pin the exact shape here so a future
    // request-size change does not require a smoke-test edit; just
    // assert the comparator round-trips and the tolerance matches.
    assert!(golden.shape.len() == 1 && golden.shape[0] >= 10);
    assert_eq!(golden.numel(), golden.shape[0]);
    assert!(golden.tolerance_abs > 0.0 && golden.tolerance_abs < 1.0);

    // No element-level value pin here — the actual values come from
    // the dense LAPACK eigensolve and live in the cross-backend test.
    assert_eq!(golden.data.len(), golden.numel());
    // Every entry must be finite.
    for z in &golden.data {
        assert!(z.re.is_finite() && z.im.is_finite());
    }
}

#[test]
fn input_c128_decodes_real_imag_interleave() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml fixture should load");

    let eps = fixture
        .input_c128("epsilon_r_complex")
        .expect("c128 input decodes");

    // Post-#146 promotion: full [n_tets] vector. n_tets == 3335 on the
    // bundled fixture. Spot-check the structural invariants without
    // pinning per-element values (those come from the cross-backend
    // test):
    //   - inside-the-sphere tets have ε = 2.25 + 0j
    //   - vacuum-gap tets have ε = 1 + 0j
    //   - PML-shell tets have Re(ε) = 1, Im(ε) < 0
    assert_eq!(eps.len(), 3335);
    let n_real_dielectric = eps
        .iter()
        .filter(|z| (z.re - 2.25).abs() < 1e-12 && z.im == 0.0)
        .count();
    let n_real_vacuum = eps
        .iter()
        .filter(|z| (z.re - 1.0).abs() < 1e-12 && z.im == 0.0)
        .count();
    let n_pml_lossy = eps
        .iter()
        .filter(|z| (z.re - 1.0).abs() < 1e-12 && z.im < 0.0)
        .count();
    // These three regions partition the mesh; together they must
    // account for every tet.
    assert_eq!(
        n_real_dielectric + n_real_vacuum + n_pml_lossy,
        eps.len(),
        "every tet should land in one of the three regions"
    );
    assert!(n_real_dielectric > 0, "expected at least one dielectric tet");
    assert!(n_real_vacuum > 0, "expected at least one vacuum-gap tet");
    assert!(n_pml_lossy > 0, "expected at least one lossy PML-shell tet");
}

#[test]
fn complex_comparator_passes_on_exact_match() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml fixture should load");

    let golden_full = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    let golden_phys = fixture
        .output_c128("physical_eigenvalues_complex")
        .expect("c128 physical output decodes");

    // Build an "actual" map that exactly matches both c128 golden
    // fields. Promoted fixture (#146) has two c128 outputs — the
    // comparator must round-trip both.
    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    actual.insert(
        "eigenvalues_lowest_complex".to_string(),
        golden_full.data.clone(),
    );
    actual.insert(
        "physical_eigenvalues_complex".to_string(),
        golden_phys.data.clone(),
    );

    let report = fixture.compare_complex_against(&actual);
    assert!(
        report.passed,
        "exact-match actual should pass complex comparator; report = {report:#?}"
    );

    for field_name in ["eigenvalues_lowest_complex", "physical_eigenvalues_complex"] {
        let diff = report
            .fields
            .iter()
            .find(|f| f.field == field_name)
            .unwrap_or_else(|| panic!("{field_name} appears in complex report"));
        assert!(diff.passed);
        assert!(matches!(diff.status, FieldStatus::Ok));
        let err = diff
            .max_abs_error
            .expect("passed complex fields report max_abs_error");
        assert!(err < 1e-15, "exact match should give |Δ| ≈ 0, got {err}");
    }

    // The f64-only outputs (n_nodes, n_tets, q_factor_lowest_physical, …)
    // do not appear in the complex report — they're handled by the
    // real-valued `compare` path. Confirms the dtype split is clean.
    for f in &report.fields {
        assert!(
            f.field == "eigenvalues_lowest_complex"
                || f.field == "physical_eigenvalues_complex",
            "f64-dtype field {} leaked into the complex report",
            f.field
        );
    }
}

#[test]
fn complex_comparator_fails_on_imag_perturbation_with_correct_tolerance() {
    let fixture = Fixture::load_from(&sphere_pml_fixture_path(), FixtureFormat::Json)
        .expect("sphere_pml fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");

    // Perturb every entry by a bump that's well above the field's
    // tolerance. The fixture's per-field tolerance is 1e-5 on |Δ|
    // (post-#146); so 1e-3 is two orders of magnitude over — every
    // element should be a violation, and the worst-offender's |Δ|
    // should be ~1e-3.
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
            // Every entry exceeded by 1e-3 vs the 1e-5 tolerance, so
            // n_violations must equal the field length.
            assert_eq!(
                *n_violations,
                golden.data.len(),
                "every entry should exceed tolerance under a 1e-3 imag bump"
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
        .expect("sphere_pml fixture should load");

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
        .expect("sphere_pml fixture should load");

    let golden = fixture
        .output_c128("eigenvalues_lowest_complex")
        .expect("c128 output decodes");
    let expected_n = golden.numel();

    let mut actual: BTreeMap<String, Vec<Complex64>> = BTreeMap::new();
    // Only one entry — golden has many.
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
            assert_eq!(*expected, expected_n);
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
        .expect("sphere_pml fixture should load");

    // Supply every f64-typed output the promoted fixture declares.
    let mut actual: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    actual.insert("n_nodes".to_string(), vec![774.0]);
    actual.insert("n_tets".to_string(), vec![3335.0]);
    // The remaining f64 fields' values come from the fixture itself —
    // we just round-trip them so the comparator passes on this case.
    for f64_field in [
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "q_factor_lowest_physical",
    ] {
        let v = fixture.output_f64(f64_field).unwrap().data.clone();
        actual.insert(f64_field.to_string(), v);
    }
    // Deliberately do NOT supply the c128 outputs — the real
    // comparator should skip them entirely.

    let report = fixture.compare_against(&actual);
    assert!(
        report.passed,
        "real comparator should pass on f64 outputs while skipping c128; report = {report:#?}"
    );
    // No c128 field should appear in the real-comparator report.
    for f in &report.fields {
        assert!(
            f.field != "eigenvalues_lowest_complex"
                && f.field != "physical_eigenvalues_complex"
                && f.field != "epsilon_r_complex",
            "c128 field {} leaked into the real-comparator report",
            f.field
        );
    }
    // Every f64 field present.
    for k in [
        "n_nodes",
        "n_tets",
        "n_edges",
        "n_interior_edges",
        "spurious_dim",
        "n_spurious_observed",
        "q_factor_lowest_physical",
    ] {
        assert!(
            report.fields.iter().any(|f| f.field == k),
            "real comparator missing field {k}"
        );
    }
}
