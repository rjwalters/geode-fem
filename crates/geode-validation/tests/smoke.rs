//! End-to-end smoke tests for the reference-comparison harness.
//!
//! These tests demonstrate the full Phase-A loop:
//!
//!   1. Load the canonical P1-reference-tet fixture from disk.
//!   2. Produce a "Rust hand-coded" actual output for the same tet.
//!   3. Compare against the fixture and assert agreement within `1e-12`
//!      (the acceptance criterion in #89).
//!   4. Deliberately corrupt one entry, re-compare, and assert that
//!      the harness produces a structured diff artifact naming the
//!      bad field, the worst offender, and the per-field tolerance.
//!
//! The fixture itself lives under `reference/fixtures/...` at repo
//! root so the same file is consumable by future NumPy / JAX / Julia
//! reference impls without duplication.

use std::collections::BTreeMap;
use std::path::PathBuf;

use geode_validation::diff::FieldStatus;
use geode_validation::{Fixture, FixtureFormat};

/// Walk up from `CARGO_MANIFEST_DIR` to find the repo root (the
/// directory that contains `reference/`). Worktrees place the crate
/// at `.loom/worktrees/issue-N/crates/geode-validation/`, so the
/// number of `..` hops isn't fixed.
fn smoke_fixture_path() -> PathBuf {
    geode_validation::fixture_path("p1_reference_tet/local_stiffness.json")
}

/// Hand-coded "Rust implementation" of the P1 reference-tet local
/// matrices and signed volume. Mirrors `p1_local_reference` in
/// `crates/geode-core/tests/p1_local_matrices.rs` but is duplicated
/// here on purpose: the harness lives in `geode-validation` and
/// shouldn't take a dependency on `geode-core` just for one tet's
/// worth of math. The agreement between this hand-coded baseline and
/// the on-disk fixture is what's actually being tested.
fn p1_reference_tet_actual() -> BTreeMap<String, Vec<f64>> {
    // Volume of the canonical reference tet = 1/6.
    let v = 1.0_f64 / 6.0;

    // K_{ij} = V * grad_phi_i . grad_phi_j  with the basis-function
    // gradients listed in `reference/fixtures/.../local_stiffness.json`'s
    // description.
    //
    // Row-major flattening of a [1, 4, 4] tensor.
    let k_local: Vec<f64> = vec![
        0.5, -v, -v, -v, -v, v, 0.0, 0.0, -v, 0.0, v, 0.0, -v, 0.0, 0.0, v,
    ];

    // M_{ij} = (V/20)(1 + delta_{ij}).
    let mass_diag = 2.0 * v / 20.0; // 1/60
    let mass_off = v / 20.0; // 1/120
    let m_local: Vec<f64> = vec![
        mass_diag, mass_off, mass_off, mass_off, mass_off, mass_diag, mass_off, mass_off, mass_off,
        mass_off, mass_diag, mass_off, mass_off, mass_off, mass_off, mass_diag,
    ];

    let signed_volume: Vec<f64> = vec![v];

    let mut out = BTreeMap::new();
    out.insert("k_local".to_string(), k_local);
    out.insert("m_local".to_string(), m_local);
    out.insert("signed_volume".to_string(), signed_volume);
    out
}

#[test]
fn smoke_fixture_loads_and_compares_within_1e_minus_12() {
    let fixture = Fixture::load_from(&smoke_fixture_path(), FixtureFormat::Json)
        .expect("smoke fixture should load");
    assert_eq!(fixture.fixture_id, "p1_reference_tet/local_stiffness");
    assert_eq!(fixture.schema_version, "1");
    assert!(fixture.outputs.contains_key("k_local"));
    assert!(fixture.outputs.contains_key("m_local"));
    assert!(fixture.outputs.contains_key("signed_volume"));

    let actual = p1_reference_tet_actual();
    let report = fixture.compare_against(&actual);
    assert!(
        report.passed,
        "Rust hand-coded baseline should match fixture within tolerance; report = {:#?}",
        report
    );
    assert_eq!(report.n_failures(), 0);

    // Every field's max abs error should be at machine precision
    // (exact rationals on both sides), well under 1e-12.
    for f in &report.fields {
        let err = f.max_abs_error.expect("passed fields report max_abs_error");
        assert!(
            err < 1e-12,
            "field {} drifted past 1e-12 (got {})",
            f.field,
            err
        );
    }
}

#[test]
fn deliberate_corruption_produces_structured_diff_artifact() {
    let fixture = Fixture::load_from(&smoke_fixture_path(), FixtureFormat::Json)
        .expect("smoke fixture should load");

    // Start from the correct baseline and deliberately corrupt
    // K_{0,0}. This simulates a real friction case: a backend
    // implementation that almost agrees but has one bad entry.
    let mut actual = p1_reference_tet_actual();
    {
        let k = actual.get_mut("k_local").expect("k_local present");
        k[0] += 1e-3; // way past the 1e-12 tolerance
    }

    let report = fixture.compare_against(&actual);
    assert!(!report.passed, "corrupted baseline should fail comparison");
    assert_eq!(report.n_failures(), 1, "exactly one field should fail");

    // Find the failing field and check it's k_local with the expected
    // worst-offender at linear index 0.
    let k_diff = report
        .fields
        .iter()
        .find(|f| f.field == "k_local")
        .expect("k_local field present in report");
    assert!(!k_diff.passed);
    match &k_diff.status {
        FieldStatus::ToleranceExceeded { n_violations } => {
            assert_eq!(*n_violations, 1, "exactly one element violated tolerance");
        }
        other => panic!("expected ToleranceExceeded, got {:?}", other),
    }
    let worst = k_diff
        .worst_offender
        .as_ref()
        .expect("worst offender recorded");
    assert_eq!(worst.index, 0, "linear index 0 is the corrupted entry");
    assert!(
        (worst.abs_error - 1e-3).abs() < 1e-15,
        "worst-offender abs_error should equal the injected delta"
    );

    // Other fields should still pass — failures must not "infect"
    // unrelated fields.
    for f in &report.fields {
        if f.field == "k_local" {
            continue;
        }
        assert!(f.passed, "field {} should still pass; got {:?}", f.field, f);
    }

    // Round-trip: write the diff artifact and re-load it to confirm
    // it's well-formed JSON consumable by a downstream tool.
    let out = tempdir_path("diff-artifact.json");
    report
        .write_diff_artifact(&out)
        .expect("diff artifact should write");
    let raw = std::fs::read_to_string(&out).expect("artifact readable");
    let reloaded: geode_validation::ComparisonReport =
        serde_json::from_str(&raw).expect("artifact is valid JSON");
    assert!(!reloaded.passed);
    assert_eq!(reloaded.fixture_id, "p1_reference_tet/local_stiffness");
    assert_eq!(reloaded.n_failures(), 1);

    // Clean up.
    let _ = std::fs::remove_file(&out);
}

#[test]
fn missing_field_in_actual_is_reported_as_distinct_failure_mode() {
    let fixture = Fixture::load_from(&smoke_fixture_path(), FixtureFormat::Json)
        .expect("smoke fixture should load");
    let mut actual = p1_reference_tet_actual();
    actual.remove("m_local");

    let report = fixture.compare_against(&actual);
    assert!(!report.passed);
    let m_diff = report
        .fields
        .iter()
        .find(|f| f.field == "m_local")
        .expect("m_local entry present in report");
    assert!(matches!(m_diff.status, FieldStatus::MissingFromActual));
}

#[test]
fn shape_mismatch_in_actual_is_reported_as_distinct_failure_mode() {
    let fixture = Fixture::load_from(&smoke_fixture_path(), FixtureFormat::Json)
        .expect("smoke fixture should load");
    let mut actual = p1_reference_tet_actual();
    // Truncate k_local so its length no longer matches the golden shape.
    actual.get_mut("k_local").unwrap().truncate(10);

    let report = fixture.compare_against(&actual);
    assert!(!report.passed);
    let k_diff = report
        .fields
        .iter()
        .find(|f| f.field == "k_local")
        .expect("k_local entry present in report");
    match &k_diff.status {
        FieldStatus::ShapeMismatch { expected, actual } => {
            assert_eq!(*expected, 16);
            assert_eq!(*actual, 10);
        }
        other => panic!("expected ShapeMismatch, got {:?}", other),
    }
}

/// Tiny tempdir helper — `std::env::temp_dir()` + a unique-ish file
/// name per test invocation. Avoids pulling in the `tempfile` crate
/// for one path's worth of friction.
fn tempdir_path(name: &str) -> PathBuf {
    let pid = std::process::id();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("geode-validation-{pid}-{ts}-{name}"));
    p
}
