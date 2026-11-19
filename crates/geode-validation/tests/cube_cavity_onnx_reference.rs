//! Cube-cavity Helmholtz cross-check: Burn (in-tree) vs the ONNX baseline.
//!
//! Targets the n=10 ONNX baseline shipped in Phase F.2 (Epic #88 / issue
//! #123). The Burn pipeline is run programmatically at the same n=10 /
//! side=1.0 that produced `onnx_baseline.json`; the fixture's
//! `eigenvalues`, `k_diag_sum`, `m_diag_sum` are compared against Burn's
//! outputs through the canonical `Fixture::compare_against` path.
//!
//! Since issue #1656 the harness body (backend tolerance table, schema
//! smoke test, Burn-vs-baseline comparison) lives in `tests/common/mod.rs`,
//! shared verbatim with `cube_cavity_jax_reference.rs` (which this file
//! previously mirrored structurally); this file keeps only its fixture
//! path, its expected `fixture_id`, and thin `#[test]` wrappers.
//!
//! # Running
//!
//! The eigensolve test used to be `#[ignore]`d because faer 0.24's
//! generalized real QZ (`qz_real`) panicked under debug-assertions.
//! `FaerDenseEigensolver` no longer calls that QZ (dense shift-invert,
//! issue #800), so every test here runs in the default tier (issue #813;
//! about 27 s in debug):
//!
//! ```sh
//! cargo test -p geode-validation --test cube_cavity_onnx_reference
//! ```

use std::path::PathBuf;

mod common;
use common::{assert_cube_cavity_baseline_fixture_schema, compare_burn_cube_cavity_to_baseline};

const EXPECTED_FIXTURE_ID: &str = "cube_cavity/n10_onnx_baseline";

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("cube_cavity/onnx_baseline.json")
}

#[test]
fn onnx_baseline_fixture_loads_with_canonical_schema() {
    assert_cube_cavity_baseline_fixture_schema(&fixture_path(), EXPECTED_FIXTURE_ID, "ONNX");
}

#[test]
fn burn_cube_cavity_agrees_with_onnx_baseline() {
    compare_burn_cube_cavity_to_baseline(&fixture_path(), "ONNX", "cube_cavity_onnx_diff.json");
}
