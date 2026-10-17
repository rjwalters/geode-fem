//! Cross-backend P1 local-matrices agreement test: Burn (in-tree) vs. NumPy.
//!
//! Migrated from `crates/geode-core/tests/p1_local_numpy_reference.rs` (PR #96
//! / issue #90) onto the canonical `geode-validation` harness as part of
//! issue #101. Five canonical tet cases, each pinned by its own
//! `reference/fixtures/p1_local/<case>.json` fixture in schema v1.
//!
//! # Why per-case fixtures
//!
//! The canonical schema (`reference/SCHEMA.md`) is "one fixture pins one
//! identity". The original `standard.json` bundled five cases under a
//! bespoke `meta + cases[]` envelope that bypassed the canonical loader.
//! Splitting into five canonical fixtures means:
//!
//!   * `Fixture::compare_against` works without any adapter shim.
//!   * `ComparisonReport::write_diff_artifact` produces one diff per case,
//!     keyed by `fixture_id`, which is exactly what the friction-mining
//!     loop wants when one case regresses but others pass.
//!   * No schema version bump — the canonical v1 absorbs the entire P1
//!     local-matrices reference set.
//!
//! # Per-backend tolerance discipline
//!
//! Each fixture declares a defensible-but-loose `tolerance_abs` per
//! output field — tight enough to catch catastrophic regression, loose
//! enough that the f32 default (wgpu / cuda) Burn backend doesn't flap.
//! The Rust test layers a tighter **backend-aware mixed abs/rel** check
//! on top to honor issue #90's acceptance criteria:
//!
//! | Backend  | Burn dtype | rel_tol | abs_tol |
//! |----------|------------|---------|---------|
//! | ndarray  | f64        | 1e-10   | 1e-12   |
//! | wgpu/cuda| f32        | 5e-5    | 1e-6    |
//!
//! The rel tolerance is the headline acceptance criterion; the abs floor
//! is what makes near-zero entries comparable without divide-by-zero
//! pathology.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use burn::prelude::Backend;
use burn::tensor::backend::BackendTypes;
use burn::tensor::{DType, Tensor, TensorData};

use geode_core::elements::p1::batched_p1_local_matrices;
use geode_core::testing::{TestBackend, device_tolerances};
use geode_util::compare::{MixedTol, check_close};
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Tolerances (backend-aware, mirrors the original p1_local test)
// ---------------------------------------------------------------------------

/// Backend-aware tolerances, selected by the device's float dtype.
fn active_tolerances() -> MixedTol {
    device_tolerances::<B, MixedTol>(
        &device(),
        &[
            // f64 path — issue #90 acceptance criterion.
            (
                "",
                DType::F64,
                MixedTol {
                    rel: 1.0e-10,
                    abs: 1.0e-12,
                },
            ),
            // f32 GPU path — looser bound per #88 dtype-honesty friction.
            (
                "",
                DType::F32,
                MixedTol {
                    rel: 5.0e-5,
                    abs: 1.0e-6,
                },
            ),
        ],
    )
    .expect("a tolerance case must match the active backend dtype")
}

// ---------------------------------------------------------------------------
// Fixture discovery + path helpers
// ---------------------------------------------------------------------------

fn fixture_dir() -> PathBuf {
    geode_validation::fixture_path("p1_local")
}

fn fixture_path(case: &str) -> PathBuf {
    fixture_dir().join(format!("{case}.json"))
}

/// The five canonical p1_local cases, in the order they were defined in
/// the legacy `standard.json`. Test failures cite case names, so this
/// list is the canonical case manifest.
const CASES: &[&str] = &[
    "canonical_reference_tet",
    "regular_tet",
    "anisotropic_well_shaped",
    "near_degenerate_sliver",
    "inverted_tet",
];

// ---------------------------------------------------------------------------
// Burn driver
// ---------------------------------------------------------------------------

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// True when the active backend stores floats as f64 (ndarray / wgpu<f64>
/// / metal<f64>); false for f32 backends (e.g. cuda). Replaces the former
/// `device_info().backend == "ndarray"` name check, which is wrong now
/// that the default test backend is f64 regardless of vendor.
fn device_is_f64() -> bool {
    Tensor::<B, 1>::zeros([0], &device()).dtype() == DType::F64
}

/// Build a `[1, 4, 3]` Burn tensor of the backend's float type from a
/// single tet's vertex array (flat `Vec<f64>` of length 12, row-major).
fn coords_tensor(vertices_flat: &[f64]) -> Tensor<B, 3> {
    assert_eq!(vertices_flat.len(), 12, "expected 12 f64 entries (1 tet)");
    if device_is_f64() {
        let data = TensorData::new(vertices_flat.to_vec(), [1, 4, 3]);
        Tensor::<B, 3>::from_data(data, &device())
    } else {
        let flat32: Vec<f32> = vertices_flat.iter().map(|&x| x as f32).collect();
        let data = TensorData::new(flat32, [1, 4, 3]);
        Tensor::<B, 3>::from_data(data, &device())
    }
}

/// Read a tensor of arbitrary rank back into a flat `Vec<f64>`, upcasting
/// the f32 path at readback so comparisons happen in f64 space.
fn tensor_to_vec_f64<const D: usize>(t: Tensor<B, D>) -> Vec<f64> {
    let data = t.into_data();
    if device_is_f64() {
        data.to_vec::<f64>().expect("readback f64")
    } else {
        data.to_vec::<f32>()
            .expect("readback f32")
            .into_iter()
            .map(f64::from)
            .collect()
    }
}

/// Run the Burn pipeline for one case, returning the actual-outputs map
/// in the shape the canonical `Fixture::compare_against` expects.
fn burn_actual(fixture: &Fixture) -> BTreeMap<String, Vec<f64>> {
    let coords_field = fixture
        .inputs
        .get("coords")
        .expect("fixture has `inputs.coords`");
    // The loader normalizes both nested and flat into a single flat f64
    // stream via `flatten_to_f64`, but `Field::data` exposes the raw
    // serde Value. Use the flattener directly via a tiny helper.
    let coords_flat = flatten_numeric(&coords_field.data);
    assert_eq!(
        coords_flat.len(),
        12,
        "fixture {} has unexpected coords length {}",
        fixture.fixture_id,
        coords_flat.len()
    );

    let coords = coords_tensor(&coords_flat);
    let result = batched_p1_local_matrices(coords);

    let k_flat = tensor_to_vec_f64(result.k_local);
    let m_flat = tensor_to_vec_f64(result.m_local);
    let v_flat = tensor_to_vec_f64(result.signed_volumes);

    let mut out = BTreeMap::new();
    out.insert("k_local".to_string(), k_flat);
    out.insert("m_local".to_string(), m_flat);
    out.insert("signed_volume".to_string(), v_flat);
    out
}

// Recursive JSON numeric flatten lives in the shared staging crate.
use geode_util::fixture::flatten_numeric;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn all_p1_local_fixtures_load_and_have_canonical_shape() {
    // Pure load-time smoke test — no Burn pipeline. Confirms each
    // per-case fixture parses, advertises schema v1, the right
    // fixture_id, and has the three output fields the test relies on.
    for case in CASES {
        let path = fixture_path(case);
        let fixture = Fixture::load_from(&path, FixtureFormat::Json).unwrap_or_else(|e| {
            panic!("failed to load fixture {}: {e}", path.display());
        });

        assert_eq!(fixture.schema_version, "1");
        assert_eq!(fixture.fixture_id, format!("p1_local/{case}"));
        for field in ["k_local", "m_local", "signed_volume"] {
            assert!(
                fixture.outputs.contains_key(field),
                "fixture {} missing output `{field}`",
                fixture.fixture_id
            );
        }
        assert!(
            fixture.inputs.contains_key("coords"),
            "fixture {} missing input `coords`",
            fixture.fixture_id
        );
    }
}

#[test]
fn burn_agrees_with_numpy_baseline_on_all_p1_local_cases() {
    let tol = active_tolerances();
    let backend = B::name(&device());
    eprintln!(
        "p1_local test: backend = {backend}, rel_tol = {:.0e}, abs_tol = {:.0e}",
        tol.rel, tol.abs
    );

    // Collect ALL per-case diff artifacts; emit one combined report on
    // failure so a reviewer can see every disagreement at once (and
    // decide whether it's a real bug, a tolerance miscalibration, or a
    // single case regressing).
    let artifact_dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let mut per_case_diffs: Vec<(String, Vec<String>)> = Vec::new();

    for case in CASES {
        let path = fixture_path(case);
        let fixture = Fixture::load_from(&path, FixtureFormat::Json)
            .unwrap_or_else(|e| panic!("load fixture {}: {e}", path.display()));

        let actual = burn_actual(&fixture);

        // First: structural pass via the canonical harness. The fixture's
        // own `tolerance_abs` is the loose tripwire; failures here mean
        // the discrepancy is bigger than even the f32 GPU envelope
        // expects. Always write the diff artifact (pass or fail) so the
        // friction-mining loop has the artifact even on green runs.
        let report = geode_validation::compare_against(&fixture, &actual);
        let artifact_path = artifact_dir.join(format!("p1_local_{case}_diff.json"));
        let _ = report.write_diff_artifact(&artifact_path);

        if !report.passed {
            per_case_diffs.push((
                case.to_string(),
                vec![format!(
                    "Fixture tripwire failed (n_failures = {}); diff artifact at {}; per-field report = {:#?}",
                    report.n_failures(),
                    artifact_path.display(),
                    report.fields
                )],
            ));
            continue;
        }

        // Second: tighter backend-aware rel/abs check (acceptance
        // criteria #90 — `1e-10` rel under f64, `5e-5` under f32). The
        // canonical schema only carries one absolute tolerance per
        // field, so the relative criterion is layered here.
        let mut case_failures: Vec<String> = Vec::new();

        // signed_volume (scalar).
        let want_v = fixture
            .output_f64("signed_volume")
            .expect("signed_volume present")
            .data[0];
        let got_v = actual["signed_volume"][0];
        if let Err(msg) = check_close(got_v, want_v, tol, 0.0, "signed_volume") {
            case_failures.push(msg);
        }

        // K_{ij}.
        let want_k = fixture.output_f64("k_local").expect("k_local present");
        let got_k = &actual["k_local"];
        for i in 0..4 {
            for j in 0..4 {
                let idx = i * 4 + j;
                if let Err(msg) = check_close(
                    got_k[idx],
                    want_k.data[idx],
                    tol,
                    0.0,
                    &format!("k_local[{i}][{j}]"),
                ) {
                    case_failures.push(msg);
                }
            }
        }

        // M_{ij}.
        let want_m = fixture.output_f64("m_local").expect("m_local present");
        let got_m = &actual["m_local"];
        for i in 0..4 {
            for j in 0..4 {
                let idx = i * 4 + j;
                if let Err(msg) = check_close(
                    got_m[idx],
                    want_m.data[idx],
                    tol,
                    0.0,
                    &format!("m_local[{i}][{j}]"),
                ) {
                    case_failures.push(msg);
                }
            }
        }

        if !case_failures.is_empty() {
            per_case_diffs.push((case.to_string(), case_failures));
        }
    }

    if !per_case_diffs.is_empty() {
        let mut report = String::from(
            "Burn / NumPy disagreement in P1 local matrices:\n\
             (per-backend rel_tol/abs_tol from active_tolerances())\n",
        );
        for (case, diffs) in &per_case_diffs {
            report.push_str(&format!("\ncase: {case}\n"));
            for d in diffs {
                report.push_str(&format!("  - {d}\n"));
            }
        }
        panic!("{report}");
    }
}

#[test]
fn inverted_tet_signed_volume_is_negative() {
    // Targeted regression: the inverted case is the only one designed
    // to exercise the negative-signed-volume diagnostic path. If this
    // assertion ever fires, somebody has reordered or regenerated the
    // fixture incorrectly.
    let path = fixture_path("inverted_tet");
    let fixture = Fixture::load_from(&path, FixtureFormat::Json).expect("load inverted fixture");
    let signed_v = fixture.output_f64("signed_volume").expect("signed_volume");
    assert!(
        signed_v.data[0] < 0.0,
        "inverted_tet fixture signed_volume is not negative: {}",
        signed_v.data[0]
    );

    let actual = burn_actual(&fixture);
    let burn_v = actual["signed_volume"][0];
    assert!(
        burn_v < 0.0,
        "Burn computed positive signed_volume {burn_v} for inverted_tet (expected negative)"
    );
}
