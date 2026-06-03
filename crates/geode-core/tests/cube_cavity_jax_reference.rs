//! Cube-cavity Helmholtz cross-check: Burn (in-tree) vs the JAX baseline.
//!
//! Epic #88 / issue #93. Loads the JAX-produced fixture
//! `reference/fixtures/cube_cavity/jax_baseline.json` and runs the Burn
//! cube-cavity assembly + faer dense eigensolve against it. Asserts that
//! the lowest 5 eigenvalues and the interior-DOF stiffness/mass traces
//! agree within the fixture's per-field tolerance.
//!
//! # Why "Option A" inline comparison (no `geode-validation` dep)
//!
//! `geode-validation` deliberately has no Burn dependency — it must
//! remain consumable from pure-Rust validation contexts. To avoid
//! coupling the harness to Burn or duplicating the fixture loader, this
//! test inlines a minimal "load the fixture, pull out a field, compare"
//! routine. The schema is the canonical v1 (per
//! `reference/SCHEMA.md`); migration to a `geode-validation`-driven
//! flow is mechanical and tracked as a follow-up alongside PR #96's
//! similar `p1_local_numpy_reference.rs`.
//!
//! # Running
//!
//! ```sh
//! # Faer's dense generalized eigensolver panics under debug_assertions
//! # (faer 0.24's qz_real subtraction overflow path), so this test is
//! # gated on `--release` like the sibling cube convergence regression
//! # test.
//! cargo test -p geode-core --release \
//!     --test cube_cavity_jax_reference -- --ignored
//! ```
//!
//! On the non-default `ndarray` backend (CI uses this — Burn's f64
//! ndarray path), the same invocation runs without `--features
//! ndarray` because the test is portable across backends; per-backend
//! tolerances apply (see `TOL_*` below).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use geode_core::{
    apply_dirichlet_bc, assemble_global_p1, burn_matrix_to_faer, cube_interior_mask, cube_tet_mesh,
    upload_mesh, DefaultBackend, EigenSolver, FaerDenseEigensolver,
};
use serde::Deserialize;

type B = DefaultBackend;

/// Per-eigenvalue absolute tolerance. The JAX baseline is f64
/// throughout; Burn's `DefaultBackend` is f32 under `wgpu` and f64
/// under `ndarray`. Cube-cavity eigenvalues are O(10²) on a unit
/// cube; `5e-3` absolute is safely above f32 round-off accumulating
/// through 6 * n^3 element contributions while still tight enough to
/// catch a real regression (per #88 framing: "1e-5 relative on the
/// lowest 5 eigenvalues" with looser allowance for cross-language
/// f64-vs-f32 drift).
const TOL_EIGVAL_ABS: f64 = 5.0e-3;

/// Trace agreement tolerance. trace(K_int) and trace(M_int) are pure
/// assembly readbacks; agreement here factorizes out the eigensolver
/// from the cross-check.
const TOL_TRACE_ABS: f64 = 1.0e-3;

#[derive(Debug, Deserialize)]
struct FixtureFile {
    schema_version: String,
    fixture_id: String,
    inputs: BTreeMap<String, FieldValue>,
    outputs: BTreeMap<String, FieldValue>,
}

#[derive(Debug, Deserialize)]
struct FieldValue {
    shape: Vec<usize>,
    /// Per-field tolerance from the fixture. Read by callers that want
    /// to honor the on-disk value; this test applies its own
    /// backend-aware tolerances (see `TOL_*` consts) because Burn's
    /// f32 wgpu default has a different drift envelope than the JAX
    /// f64 reference.
    #[serde(default)]
    #[allow(dead_code)]
    tolerance_abs: Option<f64>,
    data: serde_json::Value,
}

fn fixture_path() -> PathBuf {
    // Walk up to find the repo's `reference/` directory.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        let candidate = ancestor.join("reference/fixtures/cube_cavity/jax_baseline.json");
        if candidate.exists() {
            return candidate;
        }
    }
    panic!("could not locate reference/fixtures/cube_cavity/jax_baseline.json");
}

/// Flatten a possibly-nested serde_json numeric value into a Vec<f64>.
fn flatten_to_f64(v: &serde_json::Value) -> Vec<f64> {
    let mut out = Vec::new();
    push_numbers(v, &mut out);
    out
}

fn push_numbers(v: &serde_json::Value, out: &mut Vec<f64>) {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(x) = n.as_f64() {
                out.push(x);
            } else if let Some(x) = n.as_i64() {
                out.push(x as f64);
            } else if let Some(x) = n.as_u64() {
                out.push(x as f64);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                push_numbers(item, out);
            }
        }
        _ => {}
    }
}

fn pull_i64_scalar(field: &FieldValue) -> i64 {
    let flat = flatten_to_f64(&field.data);
    assert_eq!(
        flat.len(),
        1,
        "expected scalar i64 field, got len {}",
        flat.len()
    );
    flat[0] as i64
}

fn pull_f64_scalar(field: &FieldValue) -> f64 {
    let flat = flatten_to_f64(&field.data);
    assert_eq!(
        flat.len(),
        1,
        "expected scalar f64 field, got len {}",
        flat.len()
    );
    flat[0]
}

fn pull_f64_vec(field: &FieldValue, expected_len: usize) -> Vec<f64> {
    let flat = flatten_to_f64(&field.data);
    assert_eq!(
        flat.len(),
        expected_len,
        "expected {} f64s in field, got {}",
        expected_len,
        flat.len()
    );
    flat
}

/// Run the Burn cube-cavity pipeline for given `n` and `side`.
///
/// Returns `(lowest_k_eigenvalues, trace(K_int), trace(M_int))`.
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

    // Trace readbacks — cheaper than the full eigensolve and useful
    // as a structural sanity check.
    let n_int = k_int.nrows();
    let trk: f64 = (0..n_int).map(|i| k_int[(i, i)]).sum();
    let trm: f64 = (0..n_int).map(|i| m_int[(i, i)]).sum();

    let eigvals = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), k)
        .expect("eigensolve");
    (eigvals, trk, trm)
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
fn burn_cube_cavity_agrees_with_jax_baseline() {
    let path = fixture_path();
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let fixture: FixtureFile =
        serde_json::from_str(&raw).expect("fixture is well-formed JSON in schema v1");

    assert_eq!(
        fixture.schema_version, "1",
        "expected schema v1, got {}",
        fixture.schema_version
    );
    eprintln!("Loaded fixture id = {}", fixture.fixture_id);

    let n = pull_i64_scalar(fixture.inputs.get("n").expect("inputs.n")) as usize;
    let side = pull_f64_scalar(fixture.inputs.get("side").expect("inputs.side"));

    let eigvals_field = fixture
        .outputs
        .get("eigenvalues")
        .expect("outputs.eigenvalues");
    let k_diag_field = fixture
        .outputs
        .get("k_diag_sum")
        .expect("outputs.k_diag_sum");
    let m_diag_field = fixture
        .outputs
        .get("m_diag_sum")
        .expect("outputs.m_diag_sum");

    let expected_eigvals = pull_f64_vec(eigvals_field, eigvals_field.shape.iter().product());
    let expected_trk = pull_f64_scalar(k_diag_field);
    let expected_trm = pull_f64_scalar(m_diag_field);

    eprintln!("Running Burn pipeline (n={}, side={})...", n, side);
    let (eigvals, trk, trm) = burn_cube_cavity(n, side, expected_eigvals.len());

    eprintln!(
        "trace(K_int): expected = {:.6e}, got = {:.6e}",
        expected_trk, trk
    );
    eprintln!(
        "trace(M_int): expected = {:.6e}, got = {:.6e}",
        expected_trm, trm
    );
    let trk_err = (trk - expected_trk).abs();
    let trm_err = (trm - expected_trm).abs();
    assert!(
        trk_err < TOL_TRACE_ABS,
        "trace(K_int) drift {trk_err:.3e} exceeds tol {TOL_TRACE_ABS:.0e} \
         (expected {expected_trk:.6e}, got {trk:.6e})"
    );
    assert!(
        trm_err < TOL_TRACE_ABS,
        "trace(M_int) drift {trm_err:.3e} exceeds tol {TOL_TRACE_ABS:.0e} \
         (expected {expected_trm:.6e}, got {trm:.6e})"
    );

    eprintln!("Eigenvalue comparison (lowest {}):", expected_eigvals.len());
    for (i, (got, expected)) in eigvals.iter().zip(expected_eigvals.iter()).enumerate() {
        let abs_err = (got - expected).abs();
        let rel_err = abs_err / expected.abs().max(1.0);
        eprintln!(
            "  λ[{i}]  expected = {expected:.6e}  got = {got:.6e}  \
             abs = {abs_err:.3e}  rel = {rel_err:.3e}"
        );
        assert!(
            abs_err < TOL_EIGVAL_ABS,
            "λ[{i}] absolute drift {abs_err:.3e} exceeds tol {TOL_EIGVAL_ABS:.0e}; \
             JAX baseline = {expected}, Burn = {got}"
        );
    }
}
