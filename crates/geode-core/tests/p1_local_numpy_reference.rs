//! Cross-backend agreement test: GEODE-FEM (Burn) vs. NumPy reference.
//!
//! Loads `reference/fixtures/p1_local/standard.json` — a 5-tet fixture
//! whose `reference.numpy.{k_local,m_local,signed_volume}` baselines were
//! pre-computed by `reference/numpy/p1_local_matrices.py`. For each case,
//! runs the Burn `batched_p1_local_matrices` impl and asserts agreement
//! against the NumPy baseline within a backend-aware tolerance.
//!
//! See `reference/README.md` and
//! `reference/fixtures/p1_local/standard.schema.md` for the fixture
//! design and tolerance rationale. This is the v0 of the cross-backend
//! harness called for in issue #90 (parent epic #88); when #89 lands the
//! canonical harness, this test should migrate onto it.

use burn::tensor::backend::BackendTypes;
use burn::tensor::{Tensor, TensorData};
use serde::Deserialize;

use geode_core::{batched_p1_local_matrices, DefaultBackend};

type B = DefaultBackend;

/// JSON fixture, loaded at test time.
const FIXTURE_JSON: &str = include_str!("../../../reference/fixtures/p1_local/standard.json");

// Backend-dependent tolerance: ndarray is f64 (CI / `--features ndarray`),
// the GPU backends (wgpu, cuda) run f32. Acceptance criteria from
// issue #90 specify `1e-10` relative / `1e-12` near-zero absolute. Those
// numbers only make sense in f64; the f32 path needs the looser bound
// documented in `reference/fixtures/p1_local/standard.schema.md`.
#[cfg(feature = "ndarray")]
const REL_TOL: f64 = 1e-10;
#[cfg(feature = "ndarray")]
const ABS_TOL: f64 = 1e-12;

#[cfg(not(feature = "ndarray"))]
const REL_TOL: f64 = 5e-5;
#[cfg(not(feature = "ndarray"))]
const ABS_TOL: f64 = 1e-6;

#[derive(Debug, Deserialize)]
struct Fixture {
    #[allow(dead_code)]
    meta: Meta,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Meta {
    #[allow(dead_code)]
    slice: String,
    #[allow(dead_code)]
    schema_version: u32,
}

#[derive(Debug, Deserialize)]
struct Case {
    name: String,
    #[allow(dead_code)]
    description: String,
    input: CaseInput,
    reference: CaseReference,
}

#[derive(Debug, Deserialize)]
struct CaseInput {
    /// `[4][3]` — four (x,y,z) vertex coordinates.
    vertices: [[f64; 3]; 4],
}

#[derive(Debug, Deserialize)]
struct CaseReference {
    numpy: NumpyOutputs,
}

#[derive(Debug, Deserialize)]
struct NumpyOutputs {
    /// `[4][4]` — `K_{ij}`, row-major.
    k_local: [[f64; 4]; 4],
    /// `[4][4]` — `M_{ij}`, row-major.
    m_local: [[f64; 4]; 4],
    /// Signed element volume `det(J) / 6`.
    signed_volume: f64,
}

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Build a `[1, 4, 3]` Burn tensor of the backend's float type from a
/// single tet's vertex array.
///
/// `DefaultBackend` is f64 under `--features ndarray` and f32 under the
/// default (`wgpu`) and `cuda` paths. To honor the `1e-10` tolerance
/// under ndarray we have to feed f64 vertices in at their full f64 width;
/// under wgpu/cuda the GPU only stores f32 anyway so downcasting at the
/// boundary loses nothing. We feature-gate the input vector dtype to
/// match the backend's `FloatElem`. Only one of the two backend features
/// is ever active at once (enforced by `lib.rs` `compile_error!`s).
fn coords_tensor(vertices: &[[f64; 3]; 4]) -> Tensor<B, 3> {
    #[cfg(feature = "ndarray")]
    {
        let flat: Vec<f64> = vertices.iter().flat_map(|v| v.iter().copied()).collect();
        let data = TensorData::new(flat, [1, 4, 3]);
        Tensor::<B, 3>::from_data(data, &device())
    }
    #[cfg(not(feature = "ndarray"))]
    {
        let flat: Vec<f32> = vertices
            .iter()
            .flat_map(|v| v.iter().map(|&x| x as f32))
            .collect();
        let data = TensorData::new(flat, [1, 4, 3]);
        Tensor::<B, 3>::from_data(data, &device())
    }
}

/// Read a tensor of arbitrary rank back to a flat `Vec<f64>`, with the
/// f32-backend path up-converting at readback so all comparisons happen
/// in f64 space.
fn tensor_to_vec_f64<const D: usize>(t: Tensor<B, D>) -> Vec<f64> {
    let data = t.into_data();
    #[cfg(feature = "ndarray")]
    {
        data.to_vec::<f64>().expect("readback f64")
    }
    #[cfg(not(feature = "ndarray"))]
    {
        data.to_vec::<f32>()
            .expect("readback f32")
            .into_iter()
            .map(f64::from)
            .collect()
    }
}

/// Mixed absolute/relative tolerance check.
///
/// Returns `Ok(())` on success, `Err(msg)` describing the disagreement
/// on failure. Used by the per-case comparison so we get a structured
/// per-entry failure message (the "friction artifact" the harness is
/// supposed to produce).
fn check_close(got: f64, want: f64, label: &str) -> Result<(), String> {
    let abs_err = (got - want).abs();
    let allowed = ABS_TOL + REL_TOL * want.abs();
    if abs_err <= allowed {
        Ok(())
    } else {
        Err(format!(
            "{label}: got {got:.17e}, want {want:.17e}, |err| = {abs_err:.3e} (allowed {allowed:.3e}; rel_tol={REL_TOL:.0e}, abs_tol={ABS_TOL:.0e})"
        ))
    }
}

/// Run one case end-to-end and return all per-entry disagreements (empty
/// on success).
fn diff_case(case: &Case) -> Vec<String> {
    let coords = coords_tensor(&case.input.vertices);
    let result = batched_p1_local_matrices(coords);

    let k_flat = tensor_to_vec_f64(result.k_local);
    let m_flat = tensor_to_vec_f64(result.m_local);
    let v_flat = tensor_to_vec_f64(result.signed_volumes);

    assert_eq!(k_flat.len(), 16, "case {}: k_local size", case.name);
    assert_eq!(m_flat.len(), 16, "case {}: m_local size", case.name);
    assert_eq!(v_flat.len(), 1, "case {}: signed_volumes size", case.name);

    let mut diffs = Vec::new();

    // Signed volume.
    if let Err(msg) = check_close(
        v_flat[0],
        case.reference.numpy.signed_volume,
        "signed_volume",
    ) {
        diffs.push(msg);
    }

    // K_{ij} and M_{ij}.
    for i in 0..4 {
        for j in 0..4 {
            let idx = i * 4 + j;
            if let Err(msg) = check_close(
                k_flat[idx],
                case.reference.numpy.k_local[i][j],
                &format!("k_local[{i}][{j}]"),
            ) {
                diffs.push(msg);
            }
            if let Err(msg) = check_close(
                m_flat[idx],
                case.reference.numpy.m_local[i][j],
                &format!("m_local[{i}][{j}]"),
            ) {
                diffs.push(msg);
            }
        }
    }

    diffs
}

#[test]
fn fixture_parses_and_has_expected_cases() {
    let fixture: Fixture = serde_json::from_str(FIXTURE_JSON).expect("parse fixture");
    assert_eq!(fixture.meta.slice, "p1_local");
    assert_eq!(fixture.meta.schema_version, 1);
    assert_eq!(
        fixture.cases.len(),
        5,
        "expected 5 cases per #90 acceptance criteria"
    );

    let names: Vec<&str> = fixture.cases.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"canonical_reference_tet"));
    assert!(names.contains(&"regular_tet"));
    assert!(names.contains(&"anisotropic_well_shaped"));
    assert!(names.contains(&"near_degenerate_sliver"));
    assert!(names.contains(&"inverted_tet"));
}

#[test]
fn burn_agrees_with_numpy_baseline_on_all_cases() {
    let fixture: Fixture = serde_json::from_str(FIXTURE_JSON).expect("parse fixture");

    let mut all_failures: Vec<(String, Vec<String>)> = Vec::new();
    for case in &fixture.cases {
        let diffs = diff_case(case);
        if !diffs.is_empty() {
            all_failures.push((case.name.clone(), diffs));
        }
    }

    if !all_failures.is_empty() {
        // Emit a structured per-case / per-entry report. This *is* the
        // friction artifact: a reviewer can read the diff and decide
        // whether it's a bug in Burn, a bug in the NumPy reference, or
        // a tolerance-too-tight calibration issue.
        let mut report = String::from(
            "Burn / NumPy disagreement in P1 local matrices:\n\
             (rel_tol and abs_tol are backend-dependent; see reference/fixtures/p1_local/standard.schema.md)\n",
        );
        for (name, diffs) in &all_failures {
            report.push_str(&format!("\ncase: {name}\n"));
            for d in diffs {
                report.push_str(&format!("  - {d}\n"));
            }
        }
        panic!("{report}");
    }
}

#[test]
fn inverted_tet_signed_volume_is_negative() {
    // Targeted regression: the inverted case in the fixture is the only
    // one designed to exercise the negative-signed-volume diagnostic
    // path. If this assertion ever fires, somebody has reordered the
    // generator's CASES list incorrectly.
    let fixture: Fixture = serde_json::from_str(FIXTURE_JSON).expect("parse fixture");
    let inverted = fixture
        .cases
        .iter()
        .find(|c| c.name == "inverted_tet")
        .expect("fixture missing inverted_tet case");
    assert!(
        inverted.reference.numpy.signed_volume < 0.0,
        "inverted_tet baseline signed_volume is not negative: {}",
        inverted.reference.numpy.signed_volume
    );

    let coords = coords_tensor(&inverted.input.vertices);
    let result = batched_p1_local_matrices(coords);
    let v = tensor_to_vec_f64(result.signed_volumes);
    assert!(
        v[0] < 0.0,
        "Burn computed positive signed_volume {} for inverted_tet (expected negative)",
        v[0]
    );
}
