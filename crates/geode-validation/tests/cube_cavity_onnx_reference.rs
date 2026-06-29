//! Cube-cavity Helmholtz cross-check: Burn (in-tree) vs the ONNX baseline.
//!
//! Structural mirror of `cube_cavity_jax_reference.rs` (Epic #88 / #93
//! sibling) targeting the n=10 ONNX baseline shipped in Phase F.2
//! (Epic #88 / issue #123). The Burn pipeline is run programmatically at
//! the same n=10 / side=1.0 that produced `onnx_baseline.json`; the
//! fixture's `eigenvalues`, `k_diag_sum`, `m_diag_sum` are compared
//! against Burn's outputs through the canonical `Fixture::compare_against`
//! path.
//!
//! # Running
//!
//! Faer's dense generalized eigensolver panics under debug_assertions
//! (faer 0.24's `qz_real` subtraction overflow path), so this test is
//! gated on `--release` like the sibling JAX / NumPy reference tests:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --test cube_cavity_onnx_reference -- --ignored
//! ```

use burn::prelude::Backend;
use burn::tensor::DType;
use burn::tensor::backend::BackendTypes;
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::cube_tet_mesh;
use geode_core::testing::{TestBackend, device_tolerances};
use geode_validation::{Fixture, FixtureFormat};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Tolerances
// ---------------------------------------------------------------------------
//
// Cases live in the test; geode-core supplies only the selector. Keyed by
// the device float dtype: tight f64 on f64 backends, looser f32 otherwise.

#[derive(Debug, Clone, Copy)]
struct BackendTolerances {
    /// Absolute tolerance on lowest-5 eigenvalues at n=10. The n=10
    /// eigenvalues are O(10¹–10²); 5e-5 absolute is ~5e-7 relative at the
    /// lowest mode, above f32 accumulation and tight enough to catch a
    /// real regression.
    eigvals_abs: f64,
    /// Absolute tolerance on trace(K_int) / trace(M_int) — pure assembly
    /// readbacks (Burn `upload_mesh` f32-truncation friction, whiteroom #5).
    trace_abs: f64,
}

const NDARRAY_F64_TOLERANCES: BackendTolerances = BackendTolerances {
    eigvals_abs: 5.0e-5,
    trace_abs: 1.0e-5,
};

const GPU_F32_TOLERANCES: BackendTolerances = BackendTolerances {
    eigvals_abs: 5.0e-3,
    trace_abs: 1.0e-3,
};

impl BackendTolerances {
    /// Tolerance envelope for the active backend device, selected by the
    /// device's float dtype.
    fn for_device<B: Backend>(device: &B::Device) -> Self {
        device_tolerances::<B, BackendTolerances>(
            device,
            &[
                ("", DType::F64, NDARRAY_F64_TOLERANCES),
                ("", DType::F32, GPU_F32_TOLERANCES),
            ],
        )
        .expect("a tolerance case must match the active backend dtype")
    }
}

// ---------------------------------------------------------------------------
// Fixture path
// ---------------------------------------------------------------------------

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("cube_cavity/onnx_baseline.json")
}

// ---------------------------------------------------------------------------
// Burn pipeline
// ---------------------------------------------------------------------------

/// Run the Burn cube-cavity pipeline for given `n` (programmatic mesh)
/// and `side`. Returns `(lowest_k_eigenvalues, trace(K_int),
/// trace(M_int))`.
fn burn_cube_cavity(n: usize, side: f64, k: usize) -> (Vec<f64>, f64, f64) {
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, side);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k_mat = burn_matrix_to_faer(sys.k);
    let m_mat = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, side);
    let (k_int, m_int) =
        apply_dirichlet_bc(k_mat.as_ref(), m_mat.as_ref(), &mask).expect("BC reduction");

    let n_int = k_int.nrows();
    let trk: f64 = (0..n_int).map(|i| k_int[(i, i)]).sum();
    let trm: f64 = (0..n_int).map(|i| m_int[(i, i)]).sum();

    let eigvals = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), k)
        .expect("eigensolve");
    (eigvals, trk, trm)
}

// ---------------------------------------------------------------------------
// Fixture input helpers
// ---------------------------------------------------------------------------

/// Pull an `i64` scalar input from the fixture (declared `dtype = i64`,
/// shape `[1]`). Goes through the f64 flattener — the loader doesn't
/// have a typed integer accessor at v1, so we round-trip via f64.
fn input_i64(fixture: &Fixture, key: &str) -> i64 {
    let field = fixture
        .inputs
        .get(key)
        .unwrap_or_else(|| panic!("fixture missing input `{key}`"));
    let v = flatten_numeric(&field.data);
    assert_eq!(
        v.len(),
        1,
        "expected scalar input `{key}`, got len {}",
        v.len()
    );
    v[0] as i64
}

/// Pull an `f64` scalar input from the fixture.
fn input_f64(fixture: &Fixture, key: &str) -> f64 {
    let field = fixture
        .inputs
        .get(key)
        .unwrap_or_else(|| panic!("fixture missing input `{key}`"));
    let v = flatten_numeric(&field.data);
    assert_eq!(
        v.len(),
        1,
        "expected scalar input `{key}`, got len {}",
        v.len()
    );
    v[0]
}

// Recursive JSON numeric flatten lives in the shared staging crate.
use geode_util::fixture::flatten_numeric;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn onnx_baseline_fixture_loads_with_canonical_schema() {
    // Pure load-time smoke test — no Burn pipeline, no faer. Confirms
    // the canonical loader is happy with the ONNX baseline shape.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("onnx_baseline.json should load");
    assert_eq!(fixture.schema_version, "1");
    assert_eq!(fixture.fixture_id, "cube_cavity/n10_onnx_baseline");
    for field in ["eigenvalues", "k_diag_sum", "m_diag_sum"] {
        assert!(
            fixture.outputs.contains_key(field),
            "ONNX baseline fixture missing output `{field}`"
        );
    }
    for field in ["n", "side"] {
        assert!(
            fixture.inputs.contains_key(field),
            "ONNX baseline fixture missing input `{field}`"
        );
    }

    let eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    assert_eq!(eigvals.shape, &[5]);
    assert_eq!(eigvals.numel(), 5);
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with `cargo test -p geode-validation --release -- --ignored`"]
fn burn_cube_cavity_agrees_with_onnx_baseline() {
    let fixture =
        Fixture::load_from(&fixture_path(), FixtureFormat::Json).expect("load onnx_baseline.json");

    let n = input_i64(&fixture, "n") as usize;
    let side = input_f64(&fixture, "side");
    eprintln!(
        "ONNX baseline fixture id = {}, n = {n}, side = {side}",
        fixture.fixture_id
    );

    let expected_eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    let n_eigs = expected_eigvals.numel();

    let (eigvals, trk, trm) = burn_cube_cavity(n, side, n_eigs);

    // Build the actual-outputs map for the canonical comparator. We
    // also relax the fixture's per-field `tolerance_abs` to our
    // backend-aware override before calling `compare_against` so the
    // diff artifact reflects the actually-enforced bound (the original
    // fixture tolerances target the NumPy/ONNX in-tree cross-check at
    // ~1e-15, which Burn cannot honor through its f32 upload path).
    let mut actual = BTreeMap::new();
    actual.insert("eigenvalues".to_string(), eigvals.clone());
    actual.insert("k_diag_sum".to_string(), vec![trk]);
    actual.insert("m_diag_sum".to_string(), vec![trm]);

    let device = Default::default();
    let tol = BackendTolerances::for_device::<B>(&device);

    eprintln!(
        "backend = {}, eigvals_abs_tol = {:.0e}, trace_abs_tol = {:.0e}",
        B::name(&device),
        tol.eigvals_abs,
        tol.trace_abs
    );

    let mut relaxed = fixture.clone();
    if let Some(field) = relaxed.outputs.get_mut("eigenvalues") {
        field.tolerance_abs = tol.eigvals_abs;
    }
    if let Some(field) = relaxed.outputs.get_mut("k_diag_sum") {
        field.tolerance_abs = tol.trace_abs;
    }
    if let Some(field) = relaxed.outputs.get_mut("m_diag_sum") {
        field.tolerance_abs = tol.trace_abs;
    }

    let report = relaxed.compare_against(&actual);

    // Always write the diff artifact (pass or fail) so the
    // friction-mining loop has the artifact even on green runs.
    let artifact_path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("cube_cavity_onnx_diff.json");
    let _ = report.write_diff_artifact(&artifact_path);

    // Pretty-print eigenvalues for the green-run log.
    eprintln!(
        "trace(K_int): expected = {:.6e}, got = {:.6e}",
        flatten_numeric(&fixture.outputs["k_diag_sum"].data)[0],
        trk,
    );
    eprintln!(
        "trace(M_int): expected = {:.6e}, got = {:.6e}",
        flatten_numeric(&fixture.outputs["m_diag_sum"].data)[0],
        trm,
    );
    eprintln!("Eigenvalue comparison (lowest {n_eigs}):");
    for (i, (got, expected)) in eigvals.iter().zip(expected_eigvals.data.iter()).enumerate() {
        let abs_err = (got - expected).abs();
        let rel_err = abs_err / expected.abs().max(1.0);
        eprintln!(
            "  λ[{i}]  expected = {expected:.6e}  got = {got:.6e}  abs = {abs_err:.3e}  rel = {rel_err:.3e}"
        );
    }

    if !report.passed {
        panic!(
            "Burn cube-cavity disagrees with ONNX baseline; \
             diff artifact at {} (n_failures = {}); \
             per-field report = {:#?}",
            artifact_path.display(),
            report.n_failures(),
            report.fields
        );
    }
}
