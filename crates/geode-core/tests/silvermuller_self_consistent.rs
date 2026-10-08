//! Self-consistent `k₀` iteration on the Silver-Müller pencil
//! (issue #36).
//!
//! Companion to `tests/sphere_silvermuller_eigenmode.rs` — same fixture
//! (`read_sphere_fixture`), same complex pencil `(K + j k₀ S, M)`, but
//! wraps the solver in `self_consistent_k` to drive `k₀ ← Re(k_target)`
//! at the resonant mode.
//!
//! # Two tiers (issue #917)
//!
//! **Sparse tier (`sparse_*`, runs in CI).** The loop runs on
//! [`SparseSelfConsistentEigensolver`]: each solve is a residual-checked
//! complex shift-invert Lanczos for the [`N_WINDOW`] eigenvalues nearest the
//! target, and the target is tracked by proximity
//! ([`ModeTarget::Nearest`]). The whole tier takes about a minute in
//! release. It is ignored under debug-assertions only because it is slow
//! unoptimized, so it runs in the release default tier:
//!
//! ```sh
//! cargo test -p geode-core --release --test silvermuller_self_consistent
//! ```
//!
//! **Dense reference tier (`#[ignore]`d, not in CI).** The original tests:
//! every solve is a full dense spectrum of the 4512-DOF pencil
//! ([`FaerComplexEigensolver`]), and the target is a frozen index into the
//! `|Re λ|`-sorted list. Each test runs about 21 such solves and takes tens
//! of minutes:
//!
//! ```sh
//! cargo test -p geode-core --release \
//!   --test silvermuller_self_consistent -- --ignored
//! ```
//!
//! # What the target modes are
//!
//! The dense tier takes as its target "the first index past the gradient
//! null cluster" in the `|Re λ|`-sorted list. At the seed `k₀ = 1` that is
//! index 368, and it is **not** the dielectric sphere's lowest resonance
//! (`λ ≈ 1.417 + 0.08j`, `k ≈ 1.19`, `Q ≈ 18`). It is an overdamped
//! surface mode of the Silver-Müller term, `λ ≈ 0.125 + 2.297j`
//! (`k ≈ 1.10 + 1.04j`, `Q ≈ 0.53`): several hundred of those have a
//! smaller `|Re λ|` than the resonance and sort ahead of it. The sparse
//! tier targets the same eigenvalues, so the two tiers follow the same
//! modes. Their seed values are the `SEED_*` constants below, and
//! `sparse_window_matches_dense_spectrum_at_seeds` checks them, and the
//! sparse window around each, against a dense solve in CI.

use burn::tensor::backend::BackendTypes;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_n_interior_nodes,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::assembly::surface::assemble_silver_muller_surface;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::burn_matrix_to_faer;
use geode_core::eigen::self_consistent::{
    ModeTarget, SelfConsistentEigensolver, SelfConsistentResult, SparseSelfConsistentEigensolver,
    self_consistent_k, self_consistent_k_with,
};
use geode_core::mesh::{PHYS_OUTER_BOUNDARY, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Eigenvalues per sparse solve: the window nearest the target.
const N_WINDOW: usize = 12;

/// Dense index 368 at `k₀ = 1`: the first index past the null cluster, the
/// target of `self_consistent_converges_for_target_mode`.
const SEED_K1_FIRST: (f64, f64) = (0.125079, 2.297035);
/// Dense index 369 at `k₀ = 1`: the frozen target of
/// `frozen_target_idx_prevents_mode_hop`.
const SEED_K1_SECOND: (f64, f64) = (0.136539, 3.060225);
/// Dense index 368 at `k₀ = 20`: the first index past the null cluster, the
/// target of `self_consistent_diverges_returns_clean_result`.
const SEED_K20_FIRST: (f64, f64) = (1.417260, 0.078003);

fn seed(z: (f64, f64)) -> faer::c64 {
    faer::c64::new(z.0, z.1)
}

/// The index of the first eigenvalue past the null cluster: the rule the
/// dense tier uses for its frozen target.
fn first_physical_index(lambdas: &[faer::c64]) -> usize {
    let max_abs = lambdas
        .iter()
        .map(|l| l.re.hypot(l.im))
        .fold(0.0_f64, f64::max);
    let spurious_threshold = 1e-3 * max_abs;
    lambdas
        .iter()
        .position(|l| l.re.hypot(l.im) > spurious_threshold)
        .expect("at least one physical mode")
}

/// Build the (K, S, M, n_eigs, first_physical_idx) tuple for the sphere
/// fixture at a given seed `k₀`. The first-physical-mode index is
/// detected once at the initial solve (the spurious-mode cluster
/// boundary) and returned so the self-consistent driver can use it as
/// the frozen `target_idx`.
fn build_sphere_system(
    seed_k0: f64,
) -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize, usize) {
    let (k_full, s_full, m_full, n_eigs) = build_sphere_matrices();

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
    let first_physical = first_physical_index(&lambdas);

    (k_full, s_full, m_full, n_eigs, first_physical)
}

/// The sphere fixture's `(K, S, M)` and the dense tier's `n_eigs` (twice
/// the gradient null-space dimension). No eigensolve.
fn build_sphere_matrices() -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize) {
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

    let spurious_lower_bound = sphere_n_interior_nodes(&f.mesh, geode_core::mesh::R_BUFFER);
    let n_eigs = (spurious_lower_bound * 2).max(20);

    (k_full, s_full, m_full, n_eigs)
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
#[ignore = "dense reference tier (#917): ~21 full dense solves of the 4512-DOF pencil, tens of minutes in release; CI runs the sparse_* tier"]
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
        SelfConsistentResult::ModeLost {
            last_k, iterations, ..
        } => (last_k, q_of(last_k), iterations, "ModeLost"),
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
#[ignore = "dense reference tier (#917): ~21 full dense solves of the 4512-DOF pencil, tens of minutes in release; CI runs the sparse_* tier"]
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
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            // Frozen-int self_consistent_k does not produce ModeLost,
            // so this branch should be unreachable for this driver.
            // Log defensively rather than panic.
            eprintln!(
                "unexpected ModeLost from frozen-int driver: iters={iterations}, \
                 last k = {:.4} + {:.4e}i, best_overlap = {best_overlap}",
                last_k.re, last_k.im
            );
        }
    }
}

#[test]
#[ignore = "dense reference tier (#917): ~21 full dense solves of the 4512-DOF pencil, tens of minutes in release; CI runs the sparse_* tier"]
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
                k.re,
                k_target,
                k_first
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
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            eprintln!(
                "unexpected ModeLost from frozen-int driver: iters={iterations}, \
                 last k = {:.4} + {:.4e}i, best_overlap = {best_overlap}",
                last_k.re, last_k.im
            );
        }
    }
}

// ---------------------------------------------------------------------
// Sparse tier (issue #917): the same scenarios on the sparse windowed
// solver with proximity tracking. Same assertions as the dense tests
// above, no tolerance loosened.
// ---------------------------------------------------------------------

/// `Re √λ` on the principal branch.
fn principal_re(lam: faer::c64) -> f64 {
    let r = (lam.re * lam.re + lam.im * lam.im).sqrt();
    ((r + lam.re) / 2.0).sqrt()
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_self_consistent_converges_for_target_mode() {
    // The sparse counterpart of `self_consistent_converges_for_target_mode`:
    // same seed `k₀`, same target eigenvalue (dense index 368 at the seed),
    // same acceptance.
    let k0 = 1.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();

    let result = self_consistent_k_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(SEED_K1_FIRST)),
        N_WINDOW,
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
        SelfConsistentResult::ModeLost {
            last_k, iterations, ..
        } => (last_k, q_of(last_k), iterations, "ModeLost"),
    };

    eprintln!(
        "sparse {label} in {iterations} iters: k = {:.6} + {:.6e}i, Q = {q:.4e}",
        final_k.re, final_k.im
    );
    assert!(
        final_k.re > 0.5 && final_k.re < 5.0,
        "Re(k) out of physical band: {} (label={label})",
        final_k.re
    );
    assert!(q.is_finite(), "Q is not finite: {q} (label={label})");
    assert!(q > 0.0, "Q must be positive: {q} (label={label})");
    // The proximity-tracked driver never reports `ModeLost`.
    assert_ne!(label, "ModeLost");
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_self_consistent_diverges_returns_clean_result() {
    // The sparse counterpart of
    // `self_consistent_diverges_returns_clean_result`: the pathological
    // seed `k₀ = 20`, targeting the eigenvalue the dense tier's index
    // points at there. The contract is a clean variant, not an error or a
    // panic.
    let k0 = 20.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();

    let result = self_consistent_k_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(SEED_K20_FIRST)),
        N_WINDOW,
        1e-6,
        20,
    )
    .expect("solve must not error (only return Diverged/MaxIterations/Converged)");

    match result {
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "sparse diverged after {iterations} iters; last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            assert!(iterations >= 1);
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "sparse max iters {iterations} hit without convergence; \
                 last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            assert_eq!(iterations, 20);
        }
        SelfConsistentResult::Converged { k, q, iterations } => {
            eprintln!(
                "sparse unexpectedly converged in {iterations} iters from seed=20.0: \
                 k = {:.4} + {:.4e}i, Q = {q:.4e}",
                k.re, k.im
            );
        }
        SelfConsistentResult::ModeLost { .. } => {
            panic!("the proximity-tracked driver never reports ModeLost: {result:?}")
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_proximity_target_prevents_mode_hop() {
    // The sparse counterpart of `frozen_target_idx_prevents_mode_hop`:
    // target the second mode past the null cluster (dense index 369 at the
    // seed) from the same seed `k₀ = 1`. Proximity to the previous target
    // plays the role of the frozen index: the run must stay on that mode
    // and not hop to its neighbour (dense index 368).
    let k0 = 1.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();

    let k_first = principal_re(seed(SEED_K1_FIRST));
    let k_target = principal_re(seed(SEED_K1_SECOND));
    eprintln!("seed Re(k) at first = {k_first:.4}, at tracked target = {k_target:.4}");

    let result = self_consistent_k_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(SEED_K1_SECOND)),
        N_WINDOW,
        1e-6,
        20,
    )
    .expect("self-consistent solve");

    let mid = 0.5 * (k_first + k_target);
    match result {
        SelfConsistentResult::Converged { k, q, iterations } => {
            eprintln!(
                "sparse tracked converged in {iterations} iters: \
                 k = {:.6} + {:.6e}i, Q = {q:.4e}",
                k.re, k.im
            );
            assert!(
                k.re >= mid - 1e-6 || k.re >= k_target - 0.5 * (k_target - k_first).abs(),
                "tracked target hopped: converged at {:.4} but expected ≈ {:.4} (neighbour {:.4})",
                k.re,
                k_target,
                k_first
            );
            assert!(q.is_finite() && q > 0.0, "Q invalid: {q}");
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "sparse tracked max_iter {iterations}: last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
            assert!(
                last_k.re >= mid - 1e-3,
                "tracked target hopped at max_iter: {:.4} vs target ~{:.4}",
                last_k.re,
                k_target
            );
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "sparse tracked diverged in {iterations} iters at k = {:.4} + {:.4e}i — \
                 acceptable as long as it didn't hop to the neighbour",
                last_k.re, last_k.im
            );
        }
        SelfConsistentResult::ModeLost { .. } => {
            panic!("the proximity-tracked driver never reports ModeLost: {result:?}")
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "dense solves of the 4512-DOF pencil; runs in release"
)]
fn sparse_window_matches_dense_spectrum_at_seeds() {
    // Ties the sparse tier to the dense one, with one dense solve per seed
    // `k₀`:
    //   1. the `SEED_*` constants are the eigenvalues the dense tier's
    //      index rule selects (first index past the null cluster, and the
    //      next one);
    //   2. the sparse window around each seed is a set of eigenvalues of
    //      the same pencil: every one of them is in the dense spectrum, and
    //      the one nearest the seed is the dense target itself.
    let (k_full, s_full, m_full, n_eigs) = build_sphere_matrices();
    let n = k_full.nrows();
    let sparse = SparseSelfConsistentEigensolver::default();

    for (k0, seeds) in [
        (1.0_f64, vec![(0usize, SEED_K1_FIRST), (1, SEED_K1_SECOND)]),
        (20.0, vec![(0, SEED_K20_FIRST)]),
    ] {
        // The whole dense spectrum, sorted by |Re λ|. Its first `n_eigs`
        // entries are the list the dense tier works with.
        let dense = FaerComplexEigensolver
            .smallest_complex_eigenvalues(k_full.as_ref(), s_full.as_ref(), m_full.as_ref(), k0, n)
            .expect("dense solve");
        let first_physical = first_physical_index(&dense[..n_eigs]);

        for (offset, pinned) in seeds {
            let target = dense[first_physical + offset];
            let pinned = seed(pinned);
            assert!(
                (target - pinned).norm() < 1e-5,
                "k0 = {k0}: dense index {} is {target}, the pinned seed is {pinned}",
                first_physical + offset
            );

            let window = sparse
                .pencil_eigenvalues(
                    k_full.as_ref(),
                    s_full.as_ref(),
                    m_full.as_ref(),
                    k0,
                    pinned,
                    N_WINDOW,
                )
                .expect("sparse window solve");
            assert!(!window.is_empty(), "k0 = {k0}: empty sparse window");
            let mut worst = 0.0_f64;
            for &lam in &window {
                let gap = dense
                    .iter()
                    .map(|d| (*d - lam).norm())
                    .fold(f64::INFINITY, f64::min);
                // Null-cluster members (λ ≈ 0) are compared absolutely
                // against the seed scale, the rest relatively.
                worst = worst.max(gap / lam.norm().max(pinned.norm()));
            }
            let nearest = window
                .iter()
                .copied()
                .min_by(|a, b| (*a - pinned).norm().total_cmp(&(*b - pinned).norm()))
                .unwrap();
            eprintln!(
                "k0 = {k0}, dense index {}: dense λ = {:.9}{:+.9}i, sparse λ = {:.9}{:+.9}i, \
                 |Δ| = {:.2e}; window of {} within {worst:.2e} (relative) of the dense spectrum",
                first_physical + offset,
                target.re,
                target.im,
                nearest.re,
                nearest.im,
                (nearest - target).norm(),
                window.len(),
            );
            assert!(
                (nearest - target).norm() <= 1e-8 * target.norm(),
                "k0 = {k0}: sparse target {nearest} differs from dense {target}"
            );
            assert!(
                worst <= 1e-8,
                "k0 = {k0}: a sparse window eigenvalue is {worst:.2e} (relative) from the \
                 dense spectrum"
            );
        }
    }
}
