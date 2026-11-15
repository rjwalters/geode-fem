//! Cube-cavity Helmholtz cross-check: Burn (in-tree) vs the JAX baseline.
//!
//! Migrated from `crates/geode-core/tests/cube_cavity_jax_reference.rs`
//! (PR #97 / issue #93) onto the canonical `geode-validation` harness as
//! part of issue #101.
//!
//! Since issue #1656 the harness body (backend tolerance table, schema
//! smoke test, Burn-vs-baseline comparison) lives in `tests/common/mod.rs`,
//! shared verbatim with `cube_cavity_onnx_reference.rs`; this file keeps
//! only its fixture path, its expected `fixture_id`, and thin `#[test]`
//! wrappers.
//!
//! Loads `reference/fixtures/cube_cavity/jax_baseline.json` (canonical
//! schema v1, single fixture, single set of outputs: lowest-5
//! eigenvalues, `trace(K_int)`, `trace(M_int)`) and runs the Burn
//! cube-cavity assembly + faer dense eigensolve against it.
//!
//! # Running
//!
//! The eigensolve test used to be `#[ignore]`d because faer 0.24's
//! generalized real QZ (`qz_real`) panicked under debug-assertions.
//! `FaerDenseEigensolver` no longer calls that QZ (dense shift-invert,
//! issue #800), so every test here runs in the default tier (issue #813):
//!
//! ```sh
//! cargo test -p geode-validation --test cube_cavity_jax_reference
//! ```

use std::path::PathBuf;

mod common;
use common::{assert_cube_cavity_baseline_fixture_schema, compare_burn_cube_cavity_to_baseline};

const EXPECTED_FIXTURE_ID: &str = "cube_cavity/n4_first_five_modes";

fn fixture_path() -> PathBuf {
    geode_validation::fixture_path("cube_cavity/jax_baseline.json")
}

#[test]
fn jax_baseline_fixture_loads_with_canonical_schema() {
    assert_cube_cavity_baseline_fixture_schema(&fixture_path(), EXPECTED_FIXTURE_ID, "JAX");
}

#[test]
fn burn_cube_cavity_agrees_with_jax_baseline() {
    compare_burn_cube_cavity_to_baseline(&fixture_path(), "JAX", "cube_cavity_jax_diff.json");
}
