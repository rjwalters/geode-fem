//! Golden test (issue #681, Epic #680 Phase 1): the PEC-walled
//! dielectric-sphere cavity benchmark (`geode-core`'s
//! `tests/sphere_pec_eigenmode.rs`, issues #26/#69) re-expressed as a
//! `geode` eigen spec and run through the real `geode eigen` binary.
//!
//! The spec (`tests/fixtures/sphere_pec_golden.json`) binds everything by
//! physical-group **name** on the bundled `sphere.msh`: `ε_r = 2.25`
//! (`n = 1.5`) on `sphere_interior`, every other region vacuum by
//! default, PEC on the `outer_boundary` surface (`r = R_BUFFER = 2`), no
//! ports. The benchmark's existing acceptance is kept unchanged: each of
//! the lowest 5 physical modes pairs to an analytic PEC-cavity Mie root
//! ([`geode_core::analytic::mie::merged_roots`]) within **15 %** on `k`.
//!
//! This deliberately is **not** the `examples/mie_sphere` UPML benchmark:
//! that needs UPML-as-material and complex quasimode eigensolvers, which
//! are Phase 3 scope (issue #683).
//!
//! Two tiers, mirroring `tests/spiral_golden.rs`:
//!
//! 1. **Golden** (default `cargo test`, ~15 s debug): the CLI report held
//!    to the 15 % Mie-root bound, plus report-contract checks (ascending
//!    modes, `q = null`, `f = k₀ c / (2π L)`, tiny residuals).
//! 2. **Dense oracle** (`#[ignore]`d — full dense QZ on the 3300-DOF
//!    pencil, ~1 min in release, far longer in debug): the CLI's sparse shift-invert Lanczos modes must
//!    equal the lowest 5 physical eigenvalues of the **same** pencil from
//!    the dense `FaerDenseEigensolver` (the path the original benchmark
//!    uses) to 1e-6 relative — i.e. Lanczos did not skip a mode of a
//!    near-degenerate multiplet. Run with:
//!
//!    ```sh
//!    cargo test -p geode-cli --release --test sphere_pec_golden -- --ignored
//!    ```

use std::path::PathBuf;
use std::process::Command;

use geode_core::analytic::mie::merged_roots;
use geode_core::constants::C_M_PER_S;
use geode_core::mesh::{R_BUFFER, R_SPHERE};

/// The benchmark's existing tolerance (`sphere_pec_eigenmode.rs`).
const REL_TOL: f64 = 0.15;
/// Refractive index of the sphere (`ε_r = 2.25`).
const N_INDEX: f64 = 1.5;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sphere_pec_golden.json")
}

fn run_eigen() -> serde_json::Value {
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .arg("eigen")
        .arg(fixture())
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "geode eigen failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report is JSON")
}

/// `(λ, k₀)` of every reported mode.
fn modes(report: &serde_json::Value) -> Vec<(f64, f64)> {
    report["modes"]
        .as_array()
        .expect("modes array")
        .iter()
        .map(|m| (m["lambda"].as_f64().unwrap(), m["k0"].as_f64().unwrap()))
        .collect()
}

#[test]
fn sphere_pec_cavity_golden_matches_mie_roots() {
    let report = run_eigen();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["kind"], "eigen");
    assert_eq!(report["status"], "ok");
    assert_eq!(report["solver"]["method"], "shift_invert_lanczos");
    assert_eq!(report["eigen"]["n_modes"], 5);
    assert_eq!(report["eigen"]["sigma"], 1.0);
    assert_eq!(report["pec"][0]["physical_group"], "outer_boundary");
    assert_eq!(report["mesh"]["n_tets"], 3335);

    let length_unit_m = report["mesh"]["length_unit_m"].as_f64().unwrap();
    let analytic = merged_roots(N_INDEX, &[1, 2, 3, 4], R_SPHERE, R_BUFFER, 3);
    let got = report["modes"].as_array().unwrap();
    assert_eq!(got.len(), 5, "5 physical modes requested");

    let mut prev = 0.0_f64;
    for (i, m) in got.iter().enumerate() {
        let k0 = m["k0"].as_f64().unwrap();
        let lambda = m["lambda"].as_f64().unwrap();
        let f_hz = m["frequency_hz"].as_f64().unwrap();
        let residual = m["residual_rel"].as_f64().unwrap();
        assert_eq!(m["index"], i);
        assert!(m["q"].is_null(), "lossless pencil: Q must be null");
        assert!(lambda > prev, "modes ascending");
        prev = lambda;
        assert!((k0 * k0 - lambda).abs() < 1e-12 * lambda);
        let want_hz = k0 * C_M_PER_S / (2.0 * std::f64::consts::PI * length_unit_m);
        assert!(
            (f_hz - want_hz).abs() < 1e-12 * want_hz,
            "f = k0 c / (2π L)"
        );
        // Well under the `eigen.residual_tol` gate (1e-6).
        assert!(residual < 1e-9, "mode {i} residual {residual}");

        let closest = analytic
            .iter()
            .min_by(|a, b| (a.k - k0).abs().total_cmp(&(b.k - k0).abs()))
            .expect("analytic catalog");
        let rel = (k0 - closest.k).abs() / closest.k;
        eprintln!(
            "mode {i}: k0 = {k0:.5} ({:.4} GHz) → {:?}_{},{} k = {:.5}: {:.2}% (tol {:.0}%)",
            f_hz / 1e9,
            closest.pol,
            closest.l,
            closest.n,
            closest.k,
            100.0 * rel,
            100.0 * REL_TOL
        );
        assert!(
            rel <= REL_TOL,
            "mode {i}: k0 = {k0:.5} is {:.2}% from the closest analytic root {:?}_{},{} \
             (k = {:.5}); tolerance {:.0}%",
            100.0 * rel,
            closest.pol,
            closest.l,
            closest.n,
            closest.k,
            100.0 * REL_TOL
        );
    }
}

/// Dense-oracle tier: same pencil (same tagged-triangle PEC mask, same
/// per-tet `ε_r`), solved by the full dense generalized eigensolver.
#[test]
#[ignore = "dense QZ on the 3300-DOF sphere pencil (~1 min release); run with --release -- --ignored"]
fn sphere_pec_cavity_sparse_matches_dense_oracle() {
    use burn::tensor::backend::BackendTypes;
    use geode_core::assembly::nedelec::assemble_global_nedelec_with_epsilon;
    use geode_core::assembly::p1::upload_mesh;
    use geode_core::eigen::dense::{
        EigenSolver, FaerDenseEigensolver, apply_dirichlet_bc, burn_matrix_to_faer,
    };
    use geode_core::mesh::{pec_interior_mask_from_triangles, read_tagged_tet_mesh};

    type B = burn::backend::NdArray<f64, i32>;

    let got = modes(&run_eigen());

    let bytes = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../geode-core/tests/fixtures/sphere.msh"),
    )
    .expect("read sphere.msh");
    let tagged = read_tagged_tet_mesh(&bytes).expect("parse");
    let mesh = &tagged.mesh;
    let interior = tagged.physical_group_tag(3, "sphere_interior").unwrap();
    let outer = tagged.physical_group_tag(2, "outer_boundary").unwrap();
    let eps: Vec<f64> = tagged
        .tet_physical_tags
        .iter()
        .map(|&t| if t == interior { 2.25 } else { 1.0 })
        .collect();
    let outer_tris = tagged.triangles_with_tag(outer);
    let edges = mesh.edges();
    let mask = pec_interior_mask_from_triangles(&edges, &[outer_tris.as_slice()]);

    let tet_edges = mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|r| std::array::from_fn(|i| r[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|r| std::array::from_fn(|i| r[i].1))
        .collect();
    let device = <B as BackendTypes>::Device::default();
    let (nodes_t, tets_t) = upload_mesh::<B>(mesh, &device);
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t,
        tets_t,
        &tet_idx,
        &tet_sign,
        edges.len(),
        &eps,
    );
    let k = burn_matrix_to_faer(sys.k);
    let m = burn_matrix_to_faer(sys.m);
    let (k_int, m_int) = apply_dirichlet_bc(k.as_ref(), m.as_ref(), &mask).expect("reduce");

    // Gradient-nullspace dimension = nodes off the PEC wall.
    let mut on_wall = vec![false; mesh.n_nodes()];
    for t in &outer_tris {
        for &n in t {
            on_wall[n as usize] = true;
        }
    }
    let n_null = on_wall.iter().filter(|&&w| !w).count();
    let lambdas = FaerDenseEigensolver
        .smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), n_null + 10)
        .expect("dense eigensolve");
    let physical: Vec<f64> = lambdas.into_iter().filter(|&l| l > 1e-3).take(5).collect();
    assert_eq!(physical.len(), 5);

    for (i, ((lambda, _), want)) in got.iter().zip(&physical).enumerate() {
        let rel = (lambda - want).abs() / want;
        eprintln!("mode {i}: sparse λ = {lambda:.9}, dense λ = {want:.9}, rel {rel:.2e}");
        assert!(rel < 1e-6, "mode {i}: sparse {lambda} vs dense {want}");
    }
}
