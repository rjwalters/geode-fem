//! Regression diff-check for the cube ground-mode convergence sweep.
//!
//! This test loads the committed fixture
//! `tests/fixtures/cube_convergence.toml`, re-runs the same refinement
//! sweep that `examples/eigen_convergence.rs` (and the regen utility,
//! `examples/regen_cube_convergence_fixture.rs`) cover, and asserts that
//! every per-level eigenvalue matches its fixture entry to a relative
//! tolerance of `1e-4`.
//!
//! The fixture values are **baselines**, not analytic targets — they
//! record "what this build of the code produces", not "what physics
//! says". Their purpose is to flag unintended numerical drift in the
//! assembly or the eigensolver. If you intentionally change those (e.g.
//! switch mass-lumping, swap eigensolver backends, fix a sign bug),
//! regenerate the fixture with:
//!
//! ```sh
//! cargo run -p geode-core --release \
//!     --example regen_cube_convergence_fixture
//! ```
//!
//! and commit the new TOML alongside the code change.
//!
//! # Running this test
//!
//! Like `tests/eigensolver.rs`, this test is `#[ignore]`d because
//! faer 0.24's `gevd::qz_real` panics under debug-assertions
//! (`attempt to subtract with overflow`). Run with:
//!
//! ```sh
//! cargo test -p geode-core --release -- --ignored
//! ```

use std::fs;
use std::path::PathBuf;

use burn::tensor::backend::BackendTypes;
use geode_core::{
    apply_dirichlet_bc, assemble_global_p1, burn_matrix_to_faer, cube_interior_mask, cube_tet_mesh,
    upload_mesh, DefaultBackend, EigenSolver, FaerDenseEigensolver,
};

type B = DefaultBackend;

/// Relative tolerance for the per-level eigenvalue diff-check.
///
/// 1e-4 is comfortably above the f32 round-off that the Burn-side
/// assembly contributes (matrices are computed in f32 on the device
/// and only upcast to f64 at the faer boundary), and tight enough
/// that real regressions in assembly or eigensolver semantics will
/// trip it. If a deterministic platform difference forces this to
/// loosen, document the reason in the fixture and the PR.
const REL_TOL: f64 = 1e-4;

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("cube_convergence.toml")
}

fn ground_mode(n: usize) -> f64 {
    let device = <B as BackendTypes>::Device::default();
    let mesh = cube_tet_mesh(n, 1.0);
    let (nodes, tets) = upload_mesh::<B>(&mesh, &device);
    let sys = assemble_global_p1(nodes, tets, mesh.n_nodes());
    let k = burn_matrix_to_faer(sys.k);
    let m = burn_matrix_to_faer(sys.m);
    let mask = cube_interior_mask(&mesh.nodes, 1.0);
    let (k_int, m_int) = apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask).expect("BC reduction");
    FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), 1)
        .expect("eigensolve")[0]
}

#[test]
#[ignore = "faer 0.24 qz_real panics under debug-assertions; run with --release"]
fn cube_convergence_matches_fixture() {
    let path = fixture_path();
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    let doc: toml::Value = toml::from_str(&raw).expect("fixture is valid TOML");

    let levels: Vec<i64> = doc
        .get("meta")
        .and_then(|m| m.get("levels"))
        .and_then(|l| l.as_array())
        .expect("meta.levels missing")
        .iter()
        .map(|v| v.as_integer().expect("level is integer"))
        .collect();
    assert!(!levels.is_empty(), "fixture defines at least one level");

    for level in &levels {
        let n = *level as usize;
        let key = format!("n_{n}");
        let section = doc
            .get(&key)
            .unwrap_or_else(|| panic!("fixture missing section [{key}]"));
        let expected = section
            .get("eigenvalue")
            .and_then(|v| v.as_float())
            .unwrap_or_else(|| panic!("[{key}].eigenvalue missing or not float"));

        let got = ground_mode(n);
        let rel = (got - expected).abs() / expected.abs().max(1.0);
        eprintln!("n = {n:>2}  λ_fixture = {expected:.12e}  λ_got = {got:.12e}  rel = {rel:.3e}");
        assert!(
            rel < REL_TOL,
            "n = {n}: eigenvalue drift {rel:.3e} exceeds tol {REL_TOL:.0e}; \
             fixture = {expected}, got = {got}. \
             Regenerate with `cargo run -p geode-core --release \
             --example regen_cube_convergence_fixture` if intentional."
        );
    }
}
