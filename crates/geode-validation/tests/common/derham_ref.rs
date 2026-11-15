//! Shared body of the `derham_numpy_reference.rs` (issue #149) and
//! `derham_julia_reference.rs` (issue #168) harnesses (issue #1667).
//!
//! Both binaries pin the Burn-side discrete de Rham operators (`d⁰`,
//! `d¹`, `d²`) bit-exactly against a reference fixture on the bundled
//! sphere mesh; they differ only in which fixture they load. Each
//! harness binary supplies its fixture file name and `fixture_id` and
//! keeps its own doc comment and rank-test commentary. The three-way
//! Burn ↔ NumPy ↔ Julia agreement is pinned by the *fixtures* and the
//! generator-side cross-assert, not by the two harness bodies being
//! textually distinct, so sharing this body does not weaken the
//! cross-check.
#![allow(dead_code)]

use std::path::PathBuf;

use faer::sparse::SparseColMat;
use geode_core::mesh::{SphereFixture, read_sphere_fixture};
use geode_util::compare::cross_check_operator;
use geode_util::convert::faer_signed_csc_to_csr_i64;
use geode_util::fixture::fixture_scalar_i64;
use geode_util::math::dense_nnz;
use geode_validation::{Fixture, FixtureFormat};

/// The 25 output keys every derham reference fixture must carry.
const REQUIRED_OUTPUTS: [&str; 25] = [
    "n_nodes",
    "n_edges",
    "n_faces",
    "n_tets",
    "euler_chi",
    "d0_shape",
    "d0_nnz",
    "d0_indptr",
    "d0_indices",
    "d0_data",
    "d1_shape",
    "d1_nnz",
    "d1_indptr",
    "d1_indices",
    "d1_data",
    "d2_shape",
    "d2_nnz",
    "d2_indptr",
    "d2_indices",
    "d2_data",
    "d1_d0_nnz",
    "d2_d1_nnz",
    "rank_d0",
    "rank_d1",
    "rank_d2",
];

/// Resolve a derham reference fixture path relative to the repo's
/// `reference/fixtures/` root.
pub fn fixture_path(fixture_file: &str) -> PathBuf {
    geode_validation::fixture_path(fixture_file)
}

/// Load the reference fixture, panicking with the same message the
/// per-file loaders used (`"<fixture file> should load"`).
pub fn load_fixture(fixture_file: &str) -> Fixture {
    let path = fixture_path(fixture_file);
    let msg = format!("{fixture_file} should load");
    Fixture::load_from(&path, FixtureFormat::Json).expect(&msg)
}

/// Load the bundled sphere Burn-side mesh fixture.
pub fn sphere_fixture() -> SphereFixture {
    read_sphere_fixture().expect("sphere fixture load")
}

/// Canonical-schema check: the fixture's `fixture_id`, schema version
/// and the full 25-key required-output list.
pub fn assert_canonical_schema(fixture: &Fixture, fixture_id: &str) {
    assert_eq!(fixture.fixture_id, fixture_id);
    assert_eq!(fixture.schema_version, "1");

    for expected in REQUIRED_OUTPUTS {
        assert!(
            fixture.outputs.contains_key(expected),
            "fixture missing required output `{expected}`"
        );
    }
}

/// Cell counts (`n_nodes`, `n_edges`, `n_faces`, `n_tets`) and the
/// Euler characteristic χ = 1 agree with the fixture.
pub fn assert_cell_counts_agree(fixture: &Fixture) {
    let f = sphere_fixture();
    let mesh = &f.mesh;

    let n_nodes = mesh.n_nodes();
    let n_tets = mesh.n_tets();
    let n_edges = mesh.edges().len();
    let n_faces = mesh.faces().len();
    let euler = n_nodes as i64 - n_edges as i64 + n_faces as i64 - n_tets as i64;

    assert_eq!(n_nodes as i64, fixture_scalar_i64(fixture, "n_nodes"));
    assert_eq!(n_edges as i64, fixture_scalar_i64(fixture, "n_edges"));
    assert_eq!(n_faces as i64, fixture_scalar_i64(fixture, "n_faces"));
    assert_eq!(n_tets as i64, fixture_scalar_i64(fixture, "n_tets"));
    assert_eq!(euler, fixture_scalar_i64(fixture, "euler_chi"));
    assert_eq!(
        euler, 1,
        "Euler characteristic for the bundled sphere fixture should be 1 \
         (a 3-ball is contractible); got {euler}"
    );
}

/// One dᵏ operator's Burn-side CSR projection matches the fixture
/// bit-exactly (shape, nnz and full row-sorted integer triples).
pub fn assert_csr_matches_bit_exact(op: &str, map: &SparseColMat<usize, f64>, fixture: &Fixture) {
    let csr = faer_signed_csc_to_csr_i64(map);
    cross_check_operator(op, &csr, fixture);
}

/// Compositional exactness identity `dᵏ⁺¹ ∘ dᵏ ≡ 0`: the sparse
/// product's nnz must be zero, matching the fixture's pinned `nnz_key`.
pub fn assert_composition_is_bit_exact_zero(
    fixture: &Fixture,
    outer: &SparseColMat<usize, f64>,
    inner: &SparseColMat<usize, f64>,
    nnz_key: &str,
    product_label: &str,
    drift_hint: &str,
) {
    // faer 0.24: `&SparseColMat * &SparseColMat` returns SparseColMat
    // (same form used in `derham_exact_sequence.rs:88`).
    let prod: SparseColMat<usize, f64> = outer * inner;
    let nnz = dense_nnz(&prod);

    let want_nnz = fixture_scalar_i64(fixture, nnz_key) as usize;
    assert_eq!(
        want_nnz, 0,
        "fixture's {nnz_key} should be 0 (algebraic exactness); got {want_nnz}"
    );
    assert_eq!(
        nnz, 0,
        "{product_label} should be bit-exactly the zero matrix; got nnz = {nnz} \
         (sign-convention drift {drift_hint}?)"
    );
}

/// The Euler-arithmetic rank predictions (`rank_d0`, `rank_d1`,
/// `rank_d2`) stored in the fixture hold for the bundled sphere mesh.
pub fn assert_euler_rank_predictions_hold(fixture: &Fixture) {
    let f = sphere_fixture();
    let mesh = &f.mesh;

    let n_nodes = mesh.n_nodes() as i64;
    let n_edges = mesh.edges().len() as i64;
    let n_faces = mesh.faces().len() as i64;

    let rank_d0 = n_nodes - 1;
    let rank_d1 = n_edges - n_nodes + 1;
    let rank_d2 = n_faces - n_edges + n_nodes - 1;

    assert_eq!(rank_d0, fixture_scalar_i64(fixture, "rank_d0"));
    assert_eq!(rank_d1, fixture_scalar_i64(fixture, "rank_d1"));
    assert_eq!(rank_d2, fixture_scalar_i64(fixture, "rank_d2"));
}
