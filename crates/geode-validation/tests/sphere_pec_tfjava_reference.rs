//! Cross-backend sphere-PEC agreement test: Burn vs TF-Java reference (Issue #134).
//!
//! Loads `reference/fixtures/sphere_pec/tfjava_sidecar.json` (TF-Java reference
//! for the vector-Nédélec sphere-PEC eigenmode pipeline, Phase G.5, Epic #88)
//! and asserts Burn agreement at each sub-stage:
//!
//! 1. Fixture schema validation — `fixture_id`, `schema_version`, required
//!    output fields present. Runs under default `cargo test`.
//! 2. Integer cross-checks — `n_nodes`, `n_tets`, `n_edges`,
//!    `n_interior_edges`. Default `cargo test`.
//! 3. Assembly sub-stages — `k_int_frobenius`, `m_int_frobenius` (Frobenius
//!    norms) against the NumPy baseline. Default `cargo test`.
//! 4. Eigensolve — `physical_eigenvalues` (lowest 5 physical modes) against
//!    the NumPy baseline within 1e-5 relative (Epic #88 cross-IR floor).
//!    Ignored in debug builds only (slow there); runs in the default tier
//!    in release.
//!
//! # Key difference from JAX/Julia/NumPy harnesses
//!
//! The TF-Java sidecar does NOT embed the full K_int / M_int matrix data in
//! the checked-in fixture (3300×3300 = 10.9M entries ≈ 90 MB JSON). Instead,
//! this harness validates structural metadata (node/tet/edge counts, Frobenius
//! norms) from the placeholder fixture and compares against the NumPy baseline.
//!
//! The full matrix cross-check (K_int / M_int Frobenius norm comparison to
//! the TF-Java assembly output) runs in CI via the Python eigensolve driver
//! (`reference/driver/eigensolve_sphere_pec_sidecar.py`) which consumes the
//! assembly-time sidecar at CI run time.
//!
//! # Edge-index note
//!
//! TF-Java uses the same lexicographic edge ordering as NumPy (both sort via
//! TreeMap / np.unique respectively), so K_int / M_int diagonal ordering
//! agrees between TF-Java and NumPy. The Rust harness uses the Burn-side
//! Frobenius norms and per-DOF diagonals compared to the NumPy baseline.
//!
//! # Running
//!
//! Non-eigensolve tests (fixture schema, integer cross-checks, Frobenius norms):
//! ```sh
//! cargo test -p geode-validation --test sphere_pec_tfjava_reference
//! ```
//!
//! The eigensolve test runs in the default tier in release (issue #813).
//! It used to be `#[ignore]`d because
//! `geode_util::eigen::dense_lowest_eigenvalues` called faer 0.24's
//! generalized real QZ (`qz_real`), which panicked under debug-assertions;
//! it now delegates to `FaerDenseEigensolver` (dense shift-invert, issue
//! #800). It stays ignored in debug builds only, where it takes over 300 s.
//!
//! ```sh
//! cargo test -p geode-validation --release \
//!     --test sphere_pec_tfjava_reference -- --nocapture
//! ```

use common::Sidecar;

mod common;

const SIDECAR: Sidecar = Sidecar {
    name: "tfjava",
    display: "TFJAVA",
    file: "tfjava_sidecar.json",
    fixture_id: "sphere_pec/n774_pec_eigenmode_tfjava",
};

#[test]
fn tfjava_fixture_loads_with_canonical_schema() {
    SIDECAR.check_schema();
}

#[test]
fn sphere_pec_tfjava_mesh_substages_agree() {
    SIDECAR.check_mesh_substages();
}

#[test]
fn sphere_pec_tfjava_assembly_agrees_with_numpy_baseline() {
    // Runs under default `cargo test`.
    SIDECAR.check_assembly_vs_numpy();
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow in debug (over 300 s: the 3300-DOF sphere pencil); runs in the default tier in release"
)]
fn sphere_pec_tfjava_spectrum_agrees() {
    // Ignored in debug builds (over 300 s there).
    SIDECAR.check_spectrum_vs_numpy();
}
