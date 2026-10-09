//! Sparse tagged-mesh PEC-cavity eigensolve on the bundled dielectric
//! sphere fixture (issue #681): the `sphere_pec_eigenmode` benchmark
//! (issue #26/#69) re-run through
//! [`geode_core::eigen::pec_cavity::solve_tagged_pec_cavity_modes`] —
//! PEC by the `outer_boundary` physical group (not a radius test),
//! `ε_r = 2.25` on `sphere_interior` by name, sparse shift-invert Lanczos
//! instead of the dense full-spectrum solve.
//!
//! Acceptance is the benchmark's existing bound: each of the lowest 5
//! physical modes pairs to an analytic PEC-cavity Mie root
//! ([`geode_core::analytic::mie::merged_roots`]) within 2 % on `k`
//! (measured 0.16–0.41 %; tightened from 15 % in issue #986, which
//! corrected the analytic PEC-cavity roots).

use burn::tensor::backend::BackendTypes;
use geode_core::analytic::mie::merged_roots;
use geode_core::eigen::pec_cavity::{PecCavitySettings, solve_tagged_pec_cavity_modes};
use geode_core::mesh::{R_BUFFER, R_SPHERE, read_tagged_tet_mesh};
use geode_core::testing::TestBackend;

type B = TestBackend;

#[test]
fn sphere_pec_cavity_sparse_matches_mie_roots() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sphere.msh"),
    )
    .expect("read sphere.msh");
    let tagged = read_tagged_tet_mesh(&bytes).expect("parse sphere.msh");
    let device = <B as BackendTypes>::Device::default();

    // Shift k₀ = 1 sits below the first physical mode (λ ≈ 1.42).
    let settings = PecCavitySettings::new(1.0, 5);
    let t0 = std::time::Instant::now();
    let out = solve_tagged_pec_cavity_modes::<B>(
        &tagged,
        &["outer_boundary"],
        &[("sphere_interior", 2.25)],
        &settings,
        &device,
    )
    .expect("sparse cavity eigensolve");
    eprintln!(
        "n_interior = {}, null filtered = {}, wall = {:.2}s",
        out.n_interior,
        out.n_null_filtered,
        t0.elapsed().as_secs_f64()
    );

    let analytic = merged_roots(1.5, &[1, 2, 3, 4], R_SPHERE, R_BUFFER, 3);
    assert_eq!(out.modes.len(), 5);
    for (i, m) in out.modes.iter().enumerate() {
        let closest = analytic
            .iter()
            .min_by(|a, b| (a.k - m.k0).abs().total_cmp(&(b.k - m.k0).abs()))
            .unwrap();
        let rel = (m.k0 - closest.k).abs() / closest.k;
        eprintln!(
            "mode {i}: λ = {:.6}, k = {:.5}, residual = {:.2e} → {:?}_{},{} k = {:.5} ({:.2}%)",
            m.lambda,
            m.k0,
            m.residual_rel,
            closest.pol,
            closest.l,
            closest.n,
            closest.k,
            100.0 * rel
        );
        assert!(
            m.residual_rel < 1e-6,
            "mode {i} residual {}",
            m.residual_rel
        );
        assert!(
            rel <= 0.02,
            "mode {i}: k = {} is {:.2}% off",
            m.k0,
            100.0 * rel
        );
    }
}
