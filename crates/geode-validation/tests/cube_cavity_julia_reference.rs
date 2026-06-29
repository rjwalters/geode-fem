//! Cube-cavity Helmholtz cross-check: Burn (in-tree) vs the Julia baseline.
//!
//! Sibling to `cube_cavity_jax_reference.rs` (issue #93 / PR #97) and
//! `cube_cavity_numpy_reference.rs` (issue #92 / PR #98). Loads
//! `reference/fixtures/cube_cavity/julia_baseline.json` and asserts
//! Burn agreement on:
//!
//! - the lowest 5 eigenvalues (AC#2 of issue #115)
//! - the sub-stage Frobenius norms `‖K_int‖_F`, `‖M_int‖_F` (AC#3)
//! - the per-DOF diagonals `diag(K_int)`, `diag(M_int)` (AC#3)
//!
//! The Julia baseline runs the same canonical n=10 mesh fixture as
//! `baseline.json`, so this test exercises Burn's mesh-loading +
//! assembly path against an independent libarpack-backed reference
//! (Arpack.jl vs scipy.sparse.linalg.eigsh — both wrap the same
//! underlying solver, so eigenvalues agree to ~1e-13 across the
//! Julia/NumPy boundary).
//!
//! # Running
//!
//! Faer's dense generalized eigensolver panics under debug_assertions
//! (faer 0.24's `qz_real` subtraction overflow path), so the
//! eigenvalue-comparison test is gated on `--release` like the sibling
//! NumPy / JAX references:
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --test cube_cavity_julia_reference -- --ignored
//! ```
//!
//! The sub-stage assembly tests (mesh shape, Frobenius, diagonals) run
//! under default `cargo test` because they do not touch `qz_real`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use burn::prelude::Backend;
use burn::tensor::DType;
use burn::tensor::backend::BackendTypes;
use geode_core::assembly::p1::{assemble_global_p1, upload_mesh};
use geode_core::eigen::dense::{
    EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer, cube_interior_mask,
};
use geode_core::mesh::{GmshReader, MeshReader};
use geode_core::testing::{TestBackend, device_tolerances};
use geode_validation::{Fixture, FixtureFormat};

type B = TestBackend;

// ---------------------------------------------------------------------------
// Backend-aware tolerance overrides
// ---------------------------------------------------------------------------
//
// Same backend-aware pattern as `cube_cavity_jax_reference.rs` and
// `cube_cavity_numpy_reference.rs`. The fixture itself pins the tighter
// f64 cross-platform floor (calibrated by PR #113 / issue #110: `1e-9`
// on K-side Frobenius/diag, `5e-9` on M-side diag, `1e-8` on M-side
// Frobenius). Those are right for Julia-vs-NumPy comparison but too
// tight for Burn's f32-truncating upload_mesh path on GPU backends
// (whiteroom #5). We relax per backend below before invoking
// `Fixture::compare_against` so the diff artifact reflects what the test
// actually enforces.

#[derive(Debug, Clone, Copy)]
struct BackendTolerances {
    /// Absolute tolerance on lowest-5 eigenvalues. Cube-cavity n=10
    /// eigvals are O(10²) (3π² ≈ 30 to 9π² ≈ 89); 5e-3 absolute is
    /// ~5e-5 relative — comfortably above f32 accumulation through
    /// 6·10³ element contributions, tight enough to catch a real
    /// regression. Under f64 (ndarray) we tighten by an order.
    eigvals_abs: f64,
    /// Absolute tolerance on `‖K_int‖_F`. K entries are O(1) per DOF
    /// (h-independent for stiffness), n_int = 729, so ‖K_int‖_F ≈ 17.
    frobenius_k_abs: f64,
    /// Absolute tolerance on `‖M_int‖_F`. M entries scale as h³ ≈ 1e-3,
    /// so ‖M_int‖_F is ~3 orders smaller than ‖K_int‖_F.
    frobenius_m_abs: f64,
    /// Absolute tolerance on each entry of `diag(K_int)`. K-diag entries
    /// are O(1) per DOF.
    diag_k_abs: f64,
    /// Absolute tolerance on each entry of `diag(M_int)`. M-diag entries
    /// are O(h³) ≈ 4e-4.
    diag_m_abs: f64,
}

const NDARRAY_F64_TOLERANCES: BackendTolerances = BackendTolerances {
    // Even under --features ndarray, upload_mesh truncates coordinates
    // to f32 at the tensor boundary (assembly.rs:83), so the in-tree
    // f64 path still inherits ~f32-roughly-1e-7 precision on assembled
    // matrices. Once whiteroom #5 lands the upload fix, tighten these
    // back to the fixture's intrinsic f64 cross-platform floor.
    eigvals_abs: 5.0e-5,
    frobenius_k_abs: 1.0e-5,
    frobenius_m_abs: 1.0e-7,
    diag_k_abs: 1.0e-6,
    diag_m_abs: 1.0e-9,
};

const GPU_F32_TOLERANCES: BackendTolerances = BackendTolerances {
    eigvals_abs: 5.0e-3,
    frobenius_k_abs: 1.0e-3,
    frobenius_m_abs: 1.0e-5,
    diag_k_abs: 1.0e-4,
    diag_m_abs: 1.0e-7,
};

impl BackendTolerances {
    /// Tolerance envelope for the active backend device, selected by the
    /// device's float dtype (tight f64 on ndarray/wgpu<f64>/metal<f64>,
    /// looser f32 otherwise).
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
// Repo / fixture paths
// ---------------------------------------------------------------------------

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("cube_cavity/julia_baseline.json")
}

fn mesh_path() -> PathBuf {
    geode_validation::fixture_path("cube_cavity/unit_cube.msh")
}

// ---------------------------------------------------------------------------
// Burn pipeline (mesh-fixture path matching the Julia / NumPy n=10 setup)
// ---------------------------------------------------------------------------

struct BurnCubeCavityResult {
    n_int: usize,
    eigenvalues: Vec<f64>,
    k_int_frobenius: f64,
    m_int_frobenius: f64,
    k_int_diag: Vec<f64>,
    m_int_diag: Vec<f64>,
}

fn burn_cube_cavity_from_mesh(k: usize, side: f64, want_eigs: bool) -> BurnCubeCavityResult {
    let device = <B as BackendTypes>::Device::default();
    let bytes = std::fs::read(mesh_path()).expect("read unit_cube.msh bytes");
    let mesh = GmshReader
        .read_tet_mesh(&bytes)
        .expect("parse unit_cube.msh");
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k_mat = burn_matrix_to_faer(sys.k);
    let m_mat = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, side);
    let (k_int, m_int) =
        apply_dirichlet_bc(k_mat.as_ref(), m_mat.as_ref(), &mask).expect("BC reduction");

    let n_int = k_int.nrows();
    let mut k_diag = Vec::with_capacity(n_int);
    let mut m_diag = Vec::with_capacity(n_int);
    let mut k_fro_sq = 0.0_f64;
    let mut m_fro_sq = 0.0_f64;
    for i in 0..n_int {
        for j in 0..n_int {
            let kij = k_int[(i, j)];
            let mij = m_int[(i, j)];
            k_fro_sq += kij * kij;
            m_fro_sq += mij * mij;
        }
        k_diag.push(k_int[(i, i)]);
        m_diag.push(m_int[(i, i)]);
    }

    let eigenvalues = if want_eigs {
        FaerDenseEigensolver
            .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), k)
            .expect("eigensolve")
    } else {
        Vec::new()
    };

    BurnCubeCavityResult {
        n_int,
        eigenvalues,
        k_int_frobenius: k_fro_sq.sqrt(),
        m_int_frobenius: m_fro_sq.sqrt(),
        k_int_diag: k_diag,
        m_int_diag: m_diag,
    }
}

// ---------------------------------------------------------------------------
// Fixture helpers
// ---------------------------------------------------------------------------

fn flatten_numeric(v: &serde_json::Value) -> Vec<f64> {
    let mut out = Vec::new();
    push(v, &mut out);
    return out;

    fn push(v: &serde_json::Value, out: &mut Vec<f64>) {
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
                    push(item, out);
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn julia_baseline_fixture_loads_with_canonical_schema() {
    // Pure load-time smoke test — no Burn pipeline, no faer. Confirms
    // the canonical loader is happy with the Julia baseline shape.
    let fixture = Fixture::load_from(&fixture_path(), FixtureFormat::Json)
        .expect("julia_baseline.json should load");
    assert_eq!(fixture.schema_version, "1");
    for field in [
        "eigenvalues",
        "k_int_frobenius",
        "m_int_frobenius",
        "k_int_diag",
        "m_int_diag",
        "n_int",
    ] {
        assert!(
            fixture.outputs.contains_key(field),
            "Julia baseline fixture missing output `{field}`"
        );
    }
    for field in ["n_per_side", "side"] {
        assert!(
            fixture.inputs.contains_key(field),
            "Julia baseline fixture missing input `{field}`"
        );
    }

    let eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    assert_eq!(eigvals.shape, &[5]);
    assert_eq!(eigvals.numel(), 5);
}

#[test]
fn burn_substage_agrees_with_julia_baseline() {
    // Sub-stage assembly cross-check — does NOT touch the faer
    // eigensolver, so it runs under default `cargo test`. This is the
    // AC#3 enforcement: Frobenius norms and per-DOF diagonals against
    // the Julia baseline at the cross-platform f64 floor.
    let fixture =
        Fixture::load_from(&fixture_path(), FixtureFormat::Json).expect("load julia_baseline.json");

    let side = flatten_numeric(&fixture.inputs["side"].data)[0];
    let burn = burn_cube_cavity_from_mesh(/*k=*/ 0, side, /*want_eigs=*/ false);

    // Confirm Burn's n_int matches the fixture's (so the per-DOF
    // diagonal comparison below operates on aligned vectors).
    let fix_n_int = flatten_numeric(&fixture.outputs["n_int"].data)[0] as usize;
    assert_eq!(
        burn.n_int, fix_n_int,
        "Burn n_int ({}) disagrees with Julia n_int ({})",
        burn.n_int, fix_n_int
    );

    let device = <B as BackendTypes>::Device::default();
    let tol = BackendTolerances::for_device::<B>(&device);
    eprintln!(
        "backend = {}, frobenius_k_abs = {:.0e}, frobenius_m_abs = {:.0e}, diag_k_abs = {:.0e}, diag_m_abs = {:.0e}",
        B::name(&device),
        tol.frobenius_k_abs,
        tol.frobenius_m_abs,
        tol.diag_k_abs,
        tol.diag_m_abs,
    );

    let mut actual = BTreeMap::new();
    actual.insert("k_int_frobenius".to_string(), vec![burn.k_int_frobenius]);
    actual.insert("m_int_frobenius".to_string(), vec![burn.m_int_frobenius]);
    actual.insert("k_int_diag".to_string(), burn.k_int_diag.clone());
    actual.insert("m_int_diag".to_string(), burn.m_int_diag.clone());
    actual.insert("n_int".to_string(), vec![burn.n_int as f64]);

    // Relax fixture's intrinsic f64 floor to the backend-active envelope.
    let mut relaxed = fixture.clone();
    if let Some(f) = relaxed.outputs.get_mut("k_int_frobenius") {
        f.tolerance_abs = tol.frobenius_k_abs;
    }
    if let Some(f) = relaxed.outputs.get_mut("m_int_frobenius") {
        f.tolerance_abs = tol.frobenius_m_abs;
    }
    if let Some(f) = relaxed.outputs.get_mut("k_int_diag") {
        f.tolerance_abs = tol.diag_k_abs;
    }
    if let Some(f) = relaxed.outputs.get_mut("m_int_diag") {
        f.tolerance_abs = tol.diag_m_abs;
    }
    // Strip output fields the sub-stage assertion doesn't cover so
    // ComparisonReport doesn't complain about missing actuals.
    relaxed.outputs.retain(|k, _| {
        matches!(
            k.as_str(),
            "k_int_frobenius" | "m_int_frobenius" | "k_int_diag" | "m_int_diag" | "n_int"
        )
    });

    let report = relaxed.compare_against(&actual);

    let artifact_path =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("cube_cavity_julia_substage_diff.json");
    let _ = report.write_diff_artifact(&artifact_path);

    eprintln!(
        "‖K_int‖_F: expected = {:.12e}, got = {:.12e}",
        flatten_numeric(&fixture.outputs["k_int_frobenius"].data)[0],
        burn.k_int_frobenius,
    );
    eprintln!(
        "‖M_int‖_F: expected = {:.12e}, got = {:.12e}",
        flatten_numeric(&fixture.outputs["m_int_frobenius"].data)[0],
        burn.m_int_frobenius,
    );

    if !report.passed {
        panic!(
            "Burn cube-cavity sub-stage disagrees with Julia baseline; \
             diff artifact at {} (n_failures = {}); \
             per-field report = {:#?}",
            artifact_path.display(),
            report.n_failures(),
            report.fields
        );
    }
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with `cargo test -p geode-validation --release -- --ignored`"]
fn burn_cube_cavity_agrees_with_julia_baseline() {
    let fixture =
        Fixture::load_from(&fixture_path(), FixtureFormat::Json).expect("load julia_baseline.json");

    let side = flatten_numeric(&fixture.inputs["side"].data)[0];
    eprintln!(
        "Julia baseline fixture id = {}, side = {side}",
        fixture.fixture_id
    );

    let expected_eigvals = fixture
        .output_f64("eigenvalues")
        .expect("eigenvalues field");
    let n_eigs = expected_eigvals.numel();

    let burn = burn_cube_cavity_from_mesh(n_eigs, side, /*want_eigs=*/ true);

    let mut actual = BTreeMap::new();
    actual.insert("eigenvalues".to_string(), burn.eigenvalues.clone());

    let device = <B as BackendTypes>::Device::default();
    let tol = BackendTolerances::for_device::<B>(&device);
    eprintln!(
        "backend = {}, eigvals_abs_tol = {:.0e}",
        B::name(&device),
        tol.eigvals_abs,
    );

    // We only assert eigenvalues here (the sub-stage Frobenius/diag
    // assertion lives in `burn_substage_agrees_with_julia_baseline`,
    // which runs under default `cargo test`).
    let mut relaxed = fixture.clone();
    if let Some(field) = relaxed.outputs.get_mut("eigenvalues") {
        field.tolerance_abs = tol.eigvals_abs;
    }
    relaxed.outputs.retain(|k, _| k == "eigenvalues");

    let report = relaxed.compare_against(&actual);

    let artifact_path =
        Path::new(env!("CARGO_TARGET_TMPDIR")).join("cube_cavity_julia_eigvals_diff.json");
    let _ = report.write_diff_artifact(&artifact_path);

    eprintln!("Eigenvalue comparison (lowest {n_eigs}):");
    for (i, (got, expected)) in burn
        .eigenvalues
        .iter()
        .zip(expected_eigvals.data.iter())
        .enumerate()
    {
        let abs_err = (got - expected).abs();
        let rel_err = abs_err / expected.abs().max(1.0);
        eprintln!(
            "  λ[{i}]  expected = {expected:.6e}  got = {got:.6e}  abs = {abs_err:.3e}  rel = {rel_err:.3e}"
        );
    }

    if !report.passed {
        panic!(
            "Burn cube-cavity eigenvalues disagree with Julia baseline; \
             diff artifact at {} (n_failures = {}); \
             per-field report = {:#?}",
            artifact_path.display(),
            report.n_failures(),
            report.fields
        );
    }
}
