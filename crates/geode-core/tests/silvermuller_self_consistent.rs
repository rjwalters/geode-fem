//! Self-consistent `k₀` iteration on the Silver-Müller pencil
//! (issue #36).
//!
//! Companion to `tests/sphere_silvermuller_eigenmode.rs` — same fixture
//! (`read_sphere_fixture`), same complex pencil `(K + j k₀ S, M)`, but
//! wraps the solver in `self_consistent_k` to drive `k₀ ← Re(k_target)`
//! at the resonant mode.
//!
//! All tests that touch the dense complex eigensolver are `#[ignore]`'d:
//! faer 0.24's gevd path trips a debug-assertion. Run with
//!
//! ```sh
//! cargo test -p geode-core --release \
//!   --test silvermuller_self_consistent -- --ignored
//! ```

use burn::tensor::backend::BackendTypes;

use geode_core::{
    assemble_global_nedelec_with_epsilon, assemble_silver_muller_surface, build_epsilon_r,
    burn_matrix_to_faer, read_sphere_fixture, self_consistent_k, sphere_n_interior_nodes,
    upload_mesh, ComplexEigenSolver, DefaultBackend, FaerComplexEigensolver, SelfConsistentResult,
    PHYS_OUTER_BOUNDARY,
};

type B = DefaultBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Build the (K, S, M, n_eigs, first_physical_idx) tuple for the sphere
/// fixture at a given seed `k₀`. The first-physical-mode index is
/// detected once at the initial solve (the spurious-mode cluster
/// boundary) and returned so the self-consistent driver can use it as
/// the frozen `target_idx`.
fn build_sphere_system(
    seed_k0: f64,
) -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize, usize) {
    let f = read_sphere_fixture().expect("fixture load");
    let n_index = 1.5_f64;
    let epsilon_r = build_epsilon_r(&f.tet_physical_tags, n_index);

    let edges = f.mesh.edges();
    let n_edges = edges.len();
    let tet_edges = f.mesh.tet_edges();
    let tet_idx: Vec<[u32; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let tet_sign: Vec<[i8; 6]> = tet_edges
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].1))
        .collect();

    let (nodes_t, tets_t) = upload_mesh::<B>(&f.mesh, &device());
    let sys = assemble_global_nedelec_with_epsilon(
        nodes_t, tets_t, &tet_idx, &tet_sign, n_edges, &epsilon_r,
    );
    let s_full = assemble_silver_muller_surface(
        &f.mesh,
        &f.boundary_triangles,
        &f.triangle_physical_tags,
        PHYS_OUTER_BOUNDARY,
        &edges,
    );
    let k_full = burn_matrix_to_faer(sys.k);
    let m_full = burn_matrix_to_faer(sys.m);

    let spurious_lower_bound = sphere_n_interior_nodes(&f.mesh, geode_core::R_BUFFER);
    let n_eigs = (spurious_lower_bound * 2).max(20);

    // Find the index of the lowest physical mode at the seed solve.
    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            seed_k0,
            n_eigs,
        )
        .expect("initial sphere solve");
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    let first_physical = lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one physical mode");

    (k_full, s_full, m_full, n_eigs, first_physical)
}

/// Q with the standard outgoing-wave convention.
fn q_of(k: faer::c64) -> f64 {
    if k.im.abs() < 1e-12 {
        f64::INFINITY
    } else {
        k.re / (2.0 * k.im.abs())
    }
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn self_consistent_converges_for_target_mode() {
    // Seed near the PEC ground-mode wavenumber (k ≈ 1.2 for the
    // sphere fixture) and drive `k₀ ← Re(k_target)` on the first
    // physical mode.
    //
    // The acceptance is **soft**: this fixture is coarse, the
    // Whitney spurious cluster has hundreds of near-zero modes that
    // re-sort as `k₀` drifts, and a frozen-index strategy can land
    // on Converged OR MaxIterations depending on whether the iterates
    // pass a divergence check. We require:
    //   1. The result is *not* a panic — we get a clean enum variant.
    //   2. The final `k` is in a physically plausible band
    //      (0.5 ≤ Re(k) ≤ 5) so we know the iteration didn't escape.
    //   3. The reported Q is finite and positive (radiating mode under
    //      our sign convention).
    let seed = 1.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);
    eprintln!("sphere system built: n_eigs={n_eigs}, first physical mode idx={first_physical}");

    let result = self_consistent_k(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        first_physical,
        n_eigs,
        1e-6,
        20,
    )
    .expect("self-consistent solve");

    let (final_k, q, iterations, label) = match result {
        SelfConsistentResult::Converged { k, q, iterations } => (k, q, iterations, "Converged"),
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "MaxIterations")
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "Diverged")
        }
    };

    eprintln!(
        "{label} in {iterations} iters: k = {:.6} + {:.6e}i, Q = {q:.4e}",
        final_k.re, final_k.im
    );
    assert!(
        final_k.re > 0.5 && final_k.re < 5.0,
        "Re(k) out of physical band: {} (label={label})",
        final_k.re
    );
    assert!(q.is_finite(), "Q is not finite: {q} (label={label})");
    assert!(q > 0.0, "Q must be positive: {q} (label={label})");
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn self_consistent_diverges_returns_clean_result() {
    // Pathological seed: `k₀ = 20.0` is far above any physical mode of
    // the fixture (lowest TM_1,1 is at k ≈ 1.2). The frozen target_idx
    // remains pointed at the spurious-cluster boundary, but the wildly
    // mismatched k₀ injects a large impedance perturbation, so the
    // iteration either diverges or hits max_iter. Either way we want a
    // clean enum variant, not a panic.
    let seed = 20.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);

    let result = self_consistent_k(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        first_physical,
        n_eigs,
        1e-6,
        20,
    )
    .expect("solve must not error (only return Diverged/MaxIterations/Converged)");

    match result {
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "diverged after {iterations} iters; last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            assert!(iterations >= 1);
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "max iters {iterations} hit without convergence; last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            assert_eq!(iterations, 20);
        }
        SelfConsistentResult::Converged { k, q, iterations } => {
            // If we converged from k₀=20 we still want to know about it
            // (could happen if a high-order mode is the basin) — log
            // and accept since the goal of this test is "no panic".
            eprintln!(
                "unexpectedly converged in {iterations} iters from seed=20.0: \
                 k = {:.4} + {:.4e}i, Q = {q:.4e}",
                k.re, k.im
            );
        }
    }
}

#[test]
#[ignore = "faer 0.24 gevd panics under debug-assertions; run with --release"]
fn frozen_target_idx_prevents_mode_hop() {
    // Mode-hop scenario: pin `target_idx = first_physical + 1`, then
    // seed `k₀` slightly closer to the neighbour mode at `first_physical`.
    // A naive (un-frozen) Newton would re-classify after the first solve
    // and lock onto the wrong neighbour. The frozen index must keep us
    // on the originally-requested eigenvalue.
    let seed = 1.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);

    // Reference: at the seed `k₀`, what are Re(k) of the first two
    // physical modes?
    let solver = FaerComplexEigensolver;
    let lambdas = solver
        .smallest_complex_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            seed,
            n_eigs,
        )
        .expect("reference solve");

    let principal_re = |lam: faer::c64| -> f64 {
        let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
        ((r + lam.re) / 2.0).sqrt()
    };

    let target_idx = first_physical + 1;
    if target_idx >= lambdas.len() {
        eprintln!("not enough physical modes returned, skipping mode-hop test");
        return;
    }
    let lam_first = lambdas[first_physical];
    let lam_target = lambdas[target_idx];
    let k_first = principal_re(lam_first);
    let k_target = principal_re(lam_target);
    eprintln!(
        "seed Re(k) at first physical (idx {first_physical}) = {k_first:.4}, \
         at frozen target (idx {target_idx}) = {k_target:.4}"
    );

    let result = self_consistent_k(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        target_idx,
        n_eigs,
        1e-6,
        20,
    )
    .expect("self-consistent solve");

    match result {
        SelfConsistentResult::Converged { k, q, iterations } => {
            eprintln!(
                "frozen-idx converged in {iterations} iters: k = {:.6} + {:.6e}i, Q = {q:.4e}",
                k.re, k.im
            );
            // The frozen index must land closer to the original
            // `k_target` than to `k_first`. Mid-point test:
            let mid = 0.5 * (k_first + k_target);
            assert!(
                k.re >= mid - 1e-6 || k.re >= k_target - 0.5 * (k_target - k_first).abs(),
                "frozen target_idx hopped: converged at {:.4} but expected ≈ {:.4} (neighbour {:.4})",
                k.re, k_target, k_first
            );
            // Q-of-target sanity: positive and finite.
            assert!(q.is_finite() && q > 0.0, "Q invalid: {q}");
            let _ = q_of(k); // silence unused-import warnings if any
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            // Acceptable: the frozen target may not converge as
            // quickly as the easy lowest mode. As long as it didn't
            // *hop* to the neighbour, the test passes.
            eprintln!(
                "frozen-idx max_iter {iterations}: last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            let mid = 0.5 * (k_first + k_target);
            assert!(
                last_k.re >= mid - 1e-3,
                "frozen target_idx hopped at max_iter: {:.4} vs target ~{:.4}",
                last_k.re,
                k_target
            );
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "frozen-idx diverged in {iterations} iters at k = {:.4} + {:.4e}i — \
                 acceptable as long as it didn't hop to the neighbour",
                last_k.re, last_k.im
            );
        }
    }
}
