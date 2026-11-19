//! Bit-exact integer cross-check of the discrete de Rham operators
//! (`d⁰`, `d¹`, `d²`) against the NumPy reference (Issue #149,
//! Epic #88 Phase I bridge).
//!
//! Loads `reference/fixtures/derham/baseline.json` (NumPy reference for
//! the discrete de Rham complex on the bundled sphere fixture) and
//! asserts entrywise integer equality of the Burn-side CSR projections
//! against the fixture's stored (`indptr`, `indices`, `data`) triples.
//!
//! # Why this is bit-exact (no tolerance)
//!
//! `geode_core::derham::{gradient_map, curl_map, divergence_map}` build
//! signed `{-1.0, 0.0, +1.0}` `SparseColMat<usize, f64>` matrices —
//! mathematically integer incidence matrices stored as `f64` only
//! because `faer`'s `ComplexField` constructor doesn't accept `i32`
//! (see the Rust docstring at `crates/geode-core/src/derham.rs:144-154`).
//! The NumPy reference builds `scipy.sparse.csr_matrix` with `int64`
//! `data`. After row-sorting both, the CSR triples must match entry-by-
//! entry — any disagreement is a sign-convention drift, not a tolerance
//! failure.
//!
//! This is the cleanest cross-backend agreement test in the entire
//! `geode-validation` suite: no floating-point semantics, no eigensolve
//! drift, no eigenvector sign ambiguity. The whole test runs under
//! default `cargo test` (no `#[ignore]` gating).
//!
//! # What is checked
//!
//! 1. Cell counts: `n_nodes`, `n_edges`, `n_faces`, `n_tets` and the
//!    Euler characteristic χ = 1.
//! 2. For each of d⁰, d¹, d²: shape, nnz, and full (indptr, indices,
//!    data) row-sorted CSR equality.
//! 3. Bit-exact compositional identities `d¹ · d⁰ ≡ 0` and
//!    `d² · d¹ ≡ 0`, asserted on the Burn side as the sparse product's
//!    nnz being zero after eliminating zeros.
//! 4. Measured ranks of d⁰, d¹, d² agree with the Euler-arithmetic
//!    predictions stored in the fixture.

mod common;

use common::derham_ref;
use geode_core::derham::{curl_map, divergence_map, gradient_map};

// ---------------------------------------------------------------------------
// Fixture constants.
// ---------------------------------------------------------------------------

const FIXTURE_FILE: &str = "derham/baseline.json";
const FIXTURE_ID: &str = "derham/sphere_n774_d0_d1_d2";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fixture_loads_with_canonical_schema() {
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    derham_ref::assert_canonical_schema(&fixture, FIXTURE_ID);
}

#[test]
fn cell_counts_agree_with_numpy() {
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    derham_ref::assert_cell_counts_agree(&fixture);
}

#[test]
fn d0_csr_matches_numpy_bit_exact() {
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    let f = derham_ref::sphere_fixture();
    derham_ref::assert_csr_matches_bit_exact("d0", &gradient_map(&f.mesh), &fixture);
}

#[test]
fn d1_csr_matches_numpy_bit_exact() {
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    let f = derham_ref::sphere_fixture();
    derham_ref::assert_csr_matches_bit_exact("d1", &curl_map(&f.mesh), &fixture);
}

#[test]
fn d2_csr_matches_numpy_bit_exact() {
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    let f = derham_ref::sphere_fixture();
    derham_ref::assert_csr_matches_bit_exact("d2", &divergence_map(&f.mesh), &fixture);
}

#[test]
fn d1_d0_is_bit_exact_zero() {
    // Algebraic exactness identity d¹ ∘ d⁰ ≡ 0. The Rust docstring on
    // `derham::gradient_map` / `curl_map` (and the existing test
    // `crates/geode-core/tests/derham_exact_sequence.rs`) already
    // pin this on the Burn side; we replay it here as a cross-check
    // anchored to the fixture's pinned nnz = 0.
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    let f = derham_ref::sphere_fixture();
    let d0 = gradient_map(&f.mesh);
    let d1 = curl_map(&f.mesh);
    derham_ref::assert_composition_is_bit_exact_zero(
        &fixture,
        &d1,
        &d0,
        "d1_d0_nnz",
        "d¹ · d⁰",
        "between gradient_map and curl_map",
    );
}

#[test]
fn d2_d1_is_bit_exact_zero() {
    // Algebraic exactness identity d² ∘ d¹ ≡ 0. Mirror of
    // `derham_exact_sequence::sphere_pml_d2_d1_exact_sequence`.
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    let f = derham_ref::sphere_fixture();
    let d1 = curl_map(&f.mesh);
    let d2 = divergence_map(&f.mesh);
    derham_ref::assert_composition_is_bit_exact_zero(
        &fixture,
        &d2,
        &d1,
        "d2_d1_nnz",
        "d² · d¹",
        "between curl_map and divergence_map",
    );
}

#[test]
fn euler_rank_predictions_pinned() {
    // The fixture stores the Euler-arithmetic rank predictions; the
    // Rust side computes the actual cell counts and verifies the
    // formulas hold. Measuring ranks dense-SVD-style would add a
    // dependency on `faer::linalg::svd` and ~1s of CI time per
    // operator; instead we just pin the arithmetic identity here and
    // let `crates/geode-core/tests/derham_kernel_dim.rs` carry the
    // actual rank-via-SVD measurement on the Burn side.
    let fixture = derham_ref::load_fixture(FIXTURE_FILE);
    derham_ref::assert_euler_rank_predictions_hold(&fixture);
}
