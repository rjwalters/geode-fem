//! Cross-backend Nédélec local-matrices agreement test: Burn (in-tree) vs. NumPy.
//!
//! Sister of `p1_local_numpy_reference.rs` (issue #90 / PR #96, migrated
//! onto the canonical `geode-validation` harness in PR #105). This file
//! is the Phase G.1 cross-check for the vector spine (issue #117 /
//! Epic #88), validating the closed-form 6×6 curl-curl and mass kernels
//! in `crates/geode-core/src/nedelec.rs:51-74` against a faithful NumPy
//! transcription at `reference/numpy/nedelec_local_matrices.py`.
//!
//! # Edge-orientation contract
//!
//! The NumPy reference treats every local edge as oriented from the
//! lower-index local vertex to the higher (`s_i = +1` for all six
//! edges). Each fixture pins this convention via an
//! `inputs.tet_local_edge_signs` field of shape `(1, 6)`. The Burn-side
//! kernel `batched_nedelec_local_matrices` uses the same canonical local
//! ordering — `TET_LOCAL_EDGES = [(0,1),(0,2),(0,3),(1,2),(1,3),(2,3)]`
//! — so for G.1 the comparison is identity-on-signs. We still read the
//! sign vector out of the fixture and apply the documented `s_i s_j`
//! correction (`nedelec.rs:30-34`) to the Burn output BEFORE comparing,
//! so the moment G.2 introduces non-trivial signs (the global edge
//! table builder) this harness needs zero changes.
//!
//! # Per-backend tolerance discipline
//!
//! Same pattern as `p1_local_numpy_reference.rs` (PR #105 / issue #101):
//!
//! | Backend  | Burn dtype | rel_tol | abs_tol |
//! |----------|------------|---------|---------|
//! | ndarray  | f64        | 1e-10   | 1e-12   |
//! | wgpu/cuda| f32        | 5e-5    | 1e-6    |
//!
//! The fixture's own `tolerance_abs` (per OutputField) is layered as a
//! per-field absolute floor: the effective tolerance for each entry is
//! ``max(tol.abs + tol.rel * |want|, fixture.tolerance_abs)``. For the
//! four well-conditioned tets the fixture floor is loose-but-real
//! (~1e-4) and the tight backend-aware mixed envelope is what fires.
//! For ``near_degenerate_sliver`` — where the Nédélec curl-curl closed
//! form has an intrinsic ``gg * gg - gg * gg`` catastrophic cancellation
//! inside a ``1/|det|^3`` amplification, *regardless* of impl — the
//! fixture declares ``tolerance_abs = 1e2`` on K, which the harness
//! respects rather than flagging an algorithmically-unavoidable
//! disagreement as a bug. This is the lever the spec gives us:
//! ``reference/SCHEMA.md`` documents per-field absolute tolerances as
//! the durable "loose tripwire" for known-stress regimes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use burn::prelude::Backend;
use burn::tensor::backend::BackendTypes;
use burn::tensor::{DType, Tensor, TensorData};

use geode_core::elements::nedelec::batched_nedelec_local_matrices;
use geode_core::testing::{TestBackend, device_tolerances};
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Tolerances (backend-aware, mirrors p1_local_numpy_reference.rs)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct MixedTol {
    rel: f64,
    abs: f64,
}

/// Backend-aware tolerances, selected by the device's float dtype.
fn active_tolerances() -> MixedTol {
    device_tolerances::<B, MixedTol>(
        &device(),
        &[
            // f64 path — issue #117 acceptance criterion.
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

/// Mixed absolute/relative tolerance check with an optional per-field
/// absolute floor from the fixture's own declared `tolerance_abs`. The
/// effective tolerance is the looser of (a) the backend-aware mixed
/// abs/rel envelope and (b) the fixture-declared absolute floor.
///
/// This per-field floor matters for the `near_degenerate_sliver` case:
/// the Nédélec curl-curl closed form has a ``gg_ac gg_bd - gg_ad gg_bc``
/// catastrophic-cancellation step inside a ``1/|det|^3`` amplification,
/// so even in pure-f64 the sliver's K entries lose ~6 decimal digits to
/// roundoff regardless of impl. The fixture declares ``tolerance_abs =
/// 1e2`` on K there, encoding that *intrinsic* loss; the tight
/// ``1e-10 rel / 1e-12 abs`` check is meaningful for the four
/// well-conditioned cases but unrealistic for the sliver.
fn check_close(
    got: f64,
    want: f64,
    tol: MixedTol,
    fixture_floor: f64,
    label: &str,
) -> Result<(), String> {
    let abs_err = (got - want).abs();
    let mixed = tol.abs + tol.rel * want.abs();
    let allowed = mixed.max(fixture_floor);
    if abs_err <= allowed {
        Ok(())
    } else {
        Err(format!(
            "{label}: got {got:.17e}, want {want:.17e}, |err| = {abs_err:.3e} \
             (allowed {allowed:.3e} = max(mixed {mixed:.3e}, fixture_floor {fixture_floor:.3e}); \
             rel_tol={:.0e}, abs_tol={:.0e})",
            tol.rel, tol.abs
        ))
    }
}

// ---------------------------------------------------------------------------
// Fixture discovery + path helpers
// ---------------------------------------------------------------------------

fn fixture_dir() -> PathBuf {
    geode_validation::fixture_path("nedelec_local")
}

fn fixture_path(case: &str) -> PathBuf {
    fixture_dir().join(format!("{case}.json"))
}

/// The five canonical nedelec_local cases. Test failures cite case
/// names, so this list is the canonical case manifest.
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

/// Run the Burn pipeline for one case and apply the per-edge sign
/// correction `s_i s_j` to the [6×6] K and M matrices before returning.
///
/// G.1 fixtures use all-+1 signs, so this is identity today; the
/// machinery is in place for the G.2 fixtures which exercise the
/// global edge-table builder's non-trivial sign vectors.
fn burn_actual(fixture: &Fixture) -> BTreeMap<String, Vec<f64>> {
    let coords_field = fixture
        .inputs
        .get("coords")
        .expect("fixture has `inputs.coords`");
    let coords_flat = flatten_numeric(&coords_field.data);
    assert_eq!(
        coords_flat.len(),
        12,
        "fixture {} has unexpected coords length {}",
        fixture.fixture_id,
        coords_flat.len()
    );

    let signs_field = fixture
        .inputs
        .get("tet_local_edge_signs")
        .expect("fixture has `inputs.tet_local_edge_signs`");
    let signs_flat = flatten_numeric(&signs_field.data);
    assert_eq!(
        signs_flat.len(),
        6,
        "fixture {} has unexpected tet_local_edge_signs length {} (want 6)",
        fixture.fixture_id,
        signs_flat.len()
    );
    let mut signs = [1.0_f64; 6];
    for (i, &s) in signs_flat.iter().enumerate() {
        // Defensive: signs must be ±1 exactly.
        assert!(
            s == 1.0 || s == -1.0,
            "fixture {} tet_local_edge_signs[{i}] = {s} is not ±1",
            fixture.fixture_id
        );
        signs[i] = s;
    }

    let coords = coords_tensor(&coords_flat);
    let result = batched_nedelec_local_matrices(coords);

    let mut k_flat = tensor_to_vec_f64(result.k_local);
    let mut m_flat = tensor_to_vec_f64(result.m_local);
    let v_flat = tensor_to_vec_f64(result.signed_volumes);

    assert_eq!(k_flat.len(), 36, "Burn K should be [1, 6, 6] = 36 entries");
    assert_eq!(m_flat.len(), 36, "Burn M should be [1, 6, 6] = 36 entries");

    // Apply the per-edge sign correction K[i,j] *= s_i s_j (and ditto
    // for M). The NumPy reference is written for all-+1 local signs,
    // so this matches the reference convention before comparison.
    for i in 0..6 {
        for j in 0..6 {
            let idx = i * 6 + j;
            let s_ij = signs[i] * signs[j];
            k_flat[idx] *= s_ij;
            m_flat[idx] *= s_ij;
        }
    }

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
fn all_nedelec_local_fixtures_load_and_have_canonical_shape() {
    // Pure load-time smoke test — no Burn pipeline. Confirms each
    // per-case fixture parses, advertises schema v1, the right
    // fixture_id, and has the expected input/output fields.
    for case in CASES {
        let path = fixture_path(case);
        let fixture = Fixture::load_from(&path, FixtureFormat::Json).unwrap_or_else(|e| {
            panic!("failed to load fixture {}: {e}", path.display());
        });

        assert_eq!(fixture.schema_version, "1");
        assert_eq!(fixture.fixture_id, format!("nedelec_local/{case}"));
        for field in ["k_local", "m_local", "signed_volume"] {
            assert!(
                fixture.outputs.contains_key(field),
                "fixture {} missing output `{field}`",
                fixture.fixture_id
            );
        }
        for field in ["coords", "tet_local_edge_signs"] {
            assert!(
                fixture.inputs.contains_key(field),
                "fixture {} missing input `{field}`",
                fixture.fixture_id
            );
        }
    }
}

/// Cases for which the f32 GPU backends (wgpu / cuda) cannot
/// meaningfully represent the closed-form Nédélec output. The sliver
/// tet's K kernel is intrinsically a ``gg * gg - gg * gg`` cancellation
/// amplified by ``1/|det|^3 ~ 1e18``; f64 absorbs the loss but f32
/// blows up by 5+ orders of magnitude even when the algorithm is
/// bit-perfect. Skipping in f32 is more honest than baking a huge
/// fixture-floor to mask it. The sliver still runs in f64 (where the
/// ``1e-10`` rel acceptance criterion fires meaningfully on the four
/// well-conditioned cases).
fn case_skipped_on_f32(case: &str) -> bool {
    case == "near_degenerate_sliver"
}

#[test]
fn burn_agrees_with_numpy_baseline_on_all_nedelec_local_cases() {
    let tol = active_tolerances();
    let backend = B::name(&device());
    let is_f64_path = device_is_f64();
    eprintln!(
        "nedelec_local test: backend = {backend}, rel_tol = {:.0e}, abs_tol = {:.0e}",
        tol.rel, tol.abs
    );

    // Collect ALL per-case diff artifacts; emit one combined report on
    // failure so a reviewer can see every disagreement at once.
    let artifact_dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
    let mut per_case_diffs: Vec<(String, Vec<String>)> = Vec::new();

    for case in CASES {
        if !is_f64_path && case_skipped_on_f32(case) {
            eprintln!(
                "  - skipping case {case} on f32 backend (kernel is \
                 numerically infeasible in single precision; see \
                 `case_skipped_on_f32` for rationale)"
            );
            continue;
        }

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
        let artifact_path = artifact_dir.join(format!("nedelec_local_{case}_diff.json"));
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
        // criteria #117 — `1e-10` rel under f64, `5e-5` under f32).
        let mut case_failures: Vec<String> = Vec::new();

        // signed_volume (scalar).
        let want_v_field = fixture
            .output_f64("signed_volume")
            .expect("signed_volume present");
        let want_v = want_v_field.data[0];
        let got_v = actual["signed_volume"][0];
        if let Err(msg) = check_close(
            got_v,
            want_v,
            tol,
            want_v_field.tolerance_abs,
            "signed_volume",
        ) {
            case_failures.push(msg);
        }

        // K_{ij}, 6×6.
        let want_k = fixture.output_f64("k_local").expect("k_local present");
        let got_k = &actual["k_local"];
        for i in 0..6 {
            for j in 0..6 {
                let idx = i * 6 + j;
                if let Err(msg) = check_close(
                    got_k[idx],
                    want_k.data[idx],
                    tol,
                    want_k.tolerance_abs,
                    &format!("k_local[{i}][{j}]"),
                ) {
                    case_failures.push(msg);
                }
            }
        }

        // M_{ij}, 6×6.
        let want_m = fixture.output_f64("m_local").expect("m_local present");
        let got_m = &actual["m_local"];
        for i in 0..6 {
            for j in 0..6 {
                let idx = i * 6 + j;
                if let Err(msg) = check_close(
                    got_m[idx],
                    want_m.data[idx],
                    tol,
                    want_m.tolerance_abs,
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
            "Burn / NumPy disagreement in Nédélec local matrices:\n\
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
fn canonical_reference_tet_pins_hand_verified_entries() {
    // Hand-verified entries on the canonical reference tet [(0,0,0),
    // (1,0,0), (0,1,0), (0,0,1)] for the issue #117 acceptance
    // criterion ("at least one hand-verified entry … documented in the
    // fixture comments"). The math behind these values:
    //
    //   grad(lambda_0) = (-1, -1, -1)   grad(lambda_1) = (1, 0, 0)
    //   grad(lambda_2) = (0, 1, 0)      grad(lambda_3) = (0, 0, 1)
    //   V = 1/6,  det(J) = 1.
    //
    //   gram G is then
    //          [  3 -1 -1 -1 ]
    //     G =  [ -1  1  0  0 ]
    //          [ -1  0  1  0 ]
    //          [ -1  0  0  1 ]
    //
    //   Edge 0 = (a, b) = (0, 1):
    //     K[0, 0] = 4 V (G_00 G_11 - G_01 G_01)
    //             = (4/6) (3*1 - (-1)^2) = (4/6)*2 = 4/3.
    //     M[0, 0] = (V/20) [ (1 + delta_00) G_11 - (1) G_10
    //                        - (1) G_10 + (1 + delta_11) G_00 ]
    //             = (1/120) [ 2*1 - (-1) - (-1) + 2*3 ]
    //             = (1/120) * 10 = 1/12.
    //
    //   Edge 5 = (a, b) = (2, 3) — last edge in TET_LOCAL_EDGES:
    //     K[5, 5] = 4 V (G_22 G_33 - G_23 G_23)
    //             = (4/6) (1*1 - 0*0) = 2/3.
    //     M[5, 5] = (V/20) [ (1 + delta_22) G_33 - (1) G_23
    //                        - (1) G_23 + (1 + delta_33) G_22 ]
    //             = (1/120) [ 2*1 - 0 - 0 + 2*1 ] = (1/120)*4 = 1/30.
    let path = fixture_path("canonical_reference_tet");
    let fixture = Fixture::load_from(&path, FixtureFormat::Json).expect("load canonical fixture");
    let k = fixture.output_f64("k_local").expect("k_local present");
    let m = fixture.output_f64("m_local").expect("m_local present");
    let v = fixture.output_f64("signed_volume").expect("signed_volume");

    let tight = MixedTol {
        rel: 1.0e-12,
        abs: 1.0e-14,
    };
    // Hand-verified spot checks bypass the fixture floor — these are
    // exact rational values, no intrinsic precision loss to absorb.
    // Row-major 6×6 flat index: idx(i, j) = i * 6 + j.
    check_close(v.data[0], 1.0 / 6.0, tight, 0.0, "fixture V").unwrap();
    check_close(k.data[0], 4.0 / 3.0, tight, 0.0, "fixture K[0,0]").unwrap();
    check_close(m.data[0], 1.0 / 12.0, tight, 0.0, "fixture M[0,0]").unwrap();
    check_close(k.data[5 * 6 + 5], 2.0 / 3.0, tight, 0.0, "fixture K[5,5]").unwrap();
    check_close(m.data[5 * 6 + 5], 1.0 / 30.0, tight, 0.0, "fixture M[5,5]").unwrap();
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

#[test]
fn all_g1_fixtures_use_unit_local_edge_signs() {
    // G.1 contract: the local kernel comparison is decoupled from the
    // global edge-table builder by pinning all-+1 local edge signs in
    // every fixture. G.2 (issue #118) will introduce non-trivial signs.
    // This assertion firing means somebody regenerated a G.1 fixture
    // with a non-canonical sign convention — likely a bug.
    for case in CASES {
        let path = fixture_path(case);
        let fixture = Fixture::load_from(&path, FixtureFormat::Json)
            .unwrap_or_else(|e| panic!("load fixture {}: {e}", path.display()));
        let signs_field = fixture
            .inputs
            .get("tet_local_edge_signs")
            .expect("fixture has tet_local_edge_signs");
        let signs_flat = flatten_numeric(&signs_field.data);
        assert_eq!(signs_flat.len(), 6, "case {case}: expected 6 edge signs");
        for (i, &s) in signs_flat.iter().enumerate() {
            assert_eq!(
                s, 1.0,
                "case {case}: tet_local_edge_signs[{i}] = {s}, expected +1.0 \
                 (G.1 fixtures all use canonical lower-vertex-to-higher orientation)"
            );
        }
    }
}
