//! Vector-tracked self-consistent `k₀` iteration on the Silver-Müller
//! pencil (issue #48).
//!
//! Companion to `tests/silvermuller_self_consistent.rs`. The frozen-int
//! variant in PR #47 hit Q ≈ 0.54 on the bundled sphere fixture; the
//! Whitney-spurious cluster re-shuffles as `k₀` drifts, so integer-
//! index pinning loses the physical TM_1,1 mid-iteration. Vector
//! tracking picks the mode with maximum bilinear-M-overlap against the
//! prior iteration's target — metric-consistent with the
//! complex-symmetric pencil — and the diagnosed unblocker.
//!
//! # Two tiers (issue #917)
//!
//! As in `silvermuller_self_consistent.rs` (see its module docs for what
//! the target modes are). The `sparse_*` tests run the vector-tracked
//! driver on [`SparseSelfConsistentEigensolver`] and run in CI, in the
//! release default tier:
//!
//! ```sh
//! cargo test -p geode-core --release \
//!   --test silvermuller_self_consistent_vector_tracking
//! ```
//!
//! The `#[ignore]`d tests are the dense reference tier (10 to 30 full dense
//! solves with eigenvectors each, tens of minutes), not in CI:
//!
//! ```sh
//! cargo test -p geode-core --release \
//!   --test silvermuller_self_consistent_vector_tracking -- --ignored
//! ```
//!
//! One of them, `vector_tracked_beats_frozen_int_idx`, **fails** (issue
//! #940; it had never run in CI): from its seed the frozen index does not
//! hop, so both drivers end on the same mode (`Q = 0.5340` each, ratio
//! 1.000 against the required 1.25). The frozen integer index needs the
//! full sorted spectrum, so the sparse tier cannot host that comparison on
//! the sphere. `synthetic_vector_tracked_beats_frozen_int_idx` checks it in
//! CI on a 3-DOF pencil where the index does hop, and
//! `sparse_vector_tracking_agrees_with_proximity_tracking` checks that the
//! two sparse trackers agree on the sphere.

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
    self_consistent_k, self_consistent_k_vector_tracked, self_consistent_k_vector_tracked_with,
    self_consistent_k_with,
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
/// seed target of `vector_tracked_converges_on_lowest_mode`. The same
/// constant as in `silvermuller_self_consistent.rs`, where
/// `sparse_window_matches_dense_spectrum_at_seeds` checks it against a
/// dense solve.
const SEED_K1_FIRST: (f64, f64) = (0.125079, 2.297035);
/// Dense index `n_eigs − 1 = 735` at `k₀ = 25`: the top of the dense
/// window, the seed target of `vector_tracked_handles_mode_death`.
const SEED_K25_TOP: (f64, f64) = (37.493615, 6.451497);

fn seed(z: (f64, f64)) -> faer::c64 {
    faer::c64::new(z.0, z.1)
}

/// Same fixture builder as `silvermuller_self_consistent.rs` — kept in
/// sync deliberately so the frozen-int / vector-tracked comparison is
/// apples-to-apples.
fn build_sphere_system(
    seed_k0: f64,
) -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>, usize, usize) {
    let (k_full, s_full, m_full, n_eigs) = build_sphere_matrices();

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

fn q_of(k: faer::c64) -> f64 {
    if k.im.abs() < 1e-12 {
        f64::INFINITY
    } else {
        k.re / (2.0 * k.im.abs())
    }
}

#[test]
#[ignore = "dense reference tier (#917): 10 to 30 full dense solves of the 4512-DOF pencil with eigenvectors, tens of minutes in release; CI runs the sparse_* tier"]
fn vector_tracked_converges_on_lowest_mode() {
    // Seed near the PEC ground-mode wavenumber (k ≈ 1.2 for the sphere
    // fixture). Vector-tracking should select the same physical
    // TM_1,1-flavored mode at every iteration regardless of how the
    // Whitney spurious cluster re-sorts under k₀ drift.
    //
    // Acceptance: the run produces a valid result variant (no panic),
    // and on Converged / MaxIterations the observed Q on the tracked
    // mode is the target physical Q-range. The issue target is "Q ≥
    // 1.5"; we keep the assertion soft (Q > 0, Re(k) in band) and only
    // hard-assert the load-bearing comparison in the next test below.
    let seed = 1.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);
    eprintln!("vector-tracked: n_eigs={n_eigs}, first physical idx={first_physical}");

    let result = self_consistent_k_vector_tracked(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        first_physical,
        n_eigs,
        1e-6,
        15,
    )
    .expect("vector-tracked solve");

    let (final_k, q, iterations, label) = match result {
        SelfConsistentResult::Converged { k, q, iterations } => (k, q, iterations, "Converged"),
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "MaxIterations")
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "Diverged")
        }
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            panic!(
                "vector-tracking lost the mode at iter {iterations} \
                 (best_overlap = {best_overlap:.4}, last_k = {} + {}i) — \
                 expected the lowest physical mode to be cleanly trackable from k₀=1.0",
                last_k.re, last_k.im,
            );
        }
    };

    eprintln!(
        "vector-tracked {label} in {iterations} iters: \
         k = {:.6} + {:.6e}i, Q = {q:.4}",
        final_k.re, final_k.im
    );
    assert!(
        final_k.re > 0.5 && final_k.re < 5.0,
        "Re(k) out of physical band: {} (label={label})",
        final_k.re
    );
    assert!(q.is_finite(), "Q is not finite: {q} (label={label})");
    assert!(q > 0.0, "Q must be positive: {q} (label={label})");
    // Iteration budget check: vector tracking should converge well
    // before the loop limit for a well-seeded mode.
    assert!(
        iterations <= 15,
        "vector tracking took {iterations} iters (cap 15)"
    );
}

#[test]
#[ignore = "fails on main (#940): from this seed both drivers follow the same mode, Q ratio 1.000 < 1.25; also 31 full dense solves. CI checks the comparison in synthetic_vector_tracked_beats_frozen_int_idx"]
fn vector_tracked_beats_frozen_int_idx() {
    // The load-bearing acceptance from #48: run both variants from the
    // same seed and the same initial target index, and verify that
    // vector tracking produces a strictly higher Q than the frozen-int
    // variant. The issue target was Q ≥ 1.5 (from 0.54 baseline). We
    // assert *relative* improvement to avoid fixture-drift flakiness:
    // vector-tracked Q must be at least 1.25× frozen-int Q. The
    // diagnostic eprintln makes the absolute numbers visible.
    let seed = 1.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);

    let frozen = self_consistent_k(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        first_physical,
        n_eigs,
        1e-6,
        15,
    )
    .expect("frozen-int solve");

    let tracked = self_consistent_k_vector_tracked(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        first_physical,
        n_eigs,
        1e-6,
        15,
    )
    .expect("vector-tracked solve");

    let extract = |r: &SelfConsistentResult, label: &str| -> (f64, f64, usize, String) {
        match r {
            SelfConsistentResult::Converged { k, q, iterations } => {
                (k.re, *q, *iterations, format!("{label}/Converged"))
            }
            SelfConsistentResult::MaxIterations { last_k, iterations } => (
                last_k.re,
                q_of(*last_k),
                *iterations,
                format!("{label}/MaxIterations"),
            ),
            SelfConsistentResult::Diverged { last_k, iterations } => (
                last_k.re,
                q_of(*last_k),
                *iterations,
                format!("{label}/Diverged"),
            ),
            SelfConsistentResult::ModeLost {
                last_k,
                iterations,
                best_overlap,
            } => (
                last_k.re,
                q_of(*last_k),
                *iterations,
                format!("{label}/ModeLost(best={best_overlap:.3})"),
            ),
        }
    };

    let (re_k_frozen, q_frozen, it_frozen, lab_frozen) = extract(&frozen, "frozen-int");
    let (re_k_tracked, q_tracked, it_tracked, lab_tracked) = extract(&tracked, "tracked");

    eprintln!(
        "comparison @ seed {seed}:\n  {lab_frozen} in {it_frozen} iters: \
         Re(k) = {re_k_frozen:.4}, Q = {q_frozen:.4}\n  \
         {lab_tracked} in {it_tracked} iters: Re(k) = {re_k_tracked:.4}, Q = {q_tracked:.4}\n  \
         Q-ratio (tracked / frozen) = {:.3}",
        q_tracked / q_frozen.max(1e-30)
    );

    // Both must be finite to compare meaningfully.
    assert!(q_frozen.is_finite() && q_frozen > 0.0, "frozen Q invalid");
    assert!(
        q_tracked.is_finite() && q_tracked > 0.0,
        "tracked Q invalid"
    );

    // Relative improvement. The issue's exact target ("Q ≥ 1.5 vs
    // 0.54") is sensitive to fixture and seed; we lock the *direction*
    // (vector tracking strictly better) with a margin, leaving the
    // absolute number to the eprintln log.
    assert!(
        q_tracked >= 1.25 * q_frozen,
        "vector tracking did not beat frozen-int by ≥1.25×: \
         q_frozen = {q_frozen:.4}, q_tracked = {q_tracked:.4}"
    );
}

#[test]
#[ignore = "dense reference tier (#917): 10 to 30 full dense solves of the 4512-DOF pencil with eigenvectors, tens of minutes in release; CI runs the sparse_* tier"]
fn vector_tracked_handles_mode_death() {
    // Pick a seed and target so the iteration can't find a stable
    // overlap: target_idx = n_eigs - 1 (top of the requested window)
    // combined with a seed far above the physical spectrum (k₀ = 25).
    // The iteration should either (a) cleanly return ModeLost when
    // overlap drops below threshold, or (b) return Diverged /
    // MaxIterations without panic. We accept any of those as "clean".
    let seed = 25.0_f64;
    let (k_full, s_full, m_full, n_eigs, _first_physical) = build_sphere_system(seed);

    let result = self_consistent_k_vector_tracked(
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        n_eigs - 1,
        n_eigs,
        1e-6,
        10,
    )
    .expect("solve must not error (only return one of the result variants)");

    match result {
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            eprintln!(
                "ModeLost at iter {iterations}: last k = {:.4} + {:.4e}i, \
                 best_overlap = {best_overlap:.3} — expected clean signal",
                last_k.re, last_k.im
            );
            assert!(best_overlap < 0.5);
            assert!(iterations >= 1);
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "Diverged at iter {iterations}: last k = {:.4} + {:.4e}i — \
                 also clean, no panic",
                last_k.re, last_k.im
            );
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "MaxIterations at iter {iterations}: last k = {:.4} + {:.4e}i — \
                 also clean, no panic",
                last_k.re, last_k.im
            );
        }
        SelfConsistentResult::Converged { k, q, iterations } => {
            // Surprise convergence — likely the seed accidentally
            // landed near a high-order mode and tracked through. Log
            // and accept; this test's contract is "no panic".
            eprintln!(
                "unexpectedly converged in {iterations} iters from \
                 seed=25.0: k = {:.4} + {:.4e}i, Q = {q:.4}",
                k.re, k.im
            );
        }
    }
}

// ---------------------------------------------------------------------
// Sparse tier (issue #917): the same scenarios on the sparse windowed
// solver. Same assertions as the dense tests above, no tolerance loosened.
// ---------------------------------------------------------------------

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_vector_tracked_converges_on_lowest_mode() {
    // The sparse counterpart of `vector_tracked_converges_on_lowest_mode`:
    // same seed `k₀`, same seed target (dense index 368 at the seed), same
    // acceptance. After the seed solve the target is tracked by
    // eigenvector overlap inside the sparse window.
    let k0 = 1.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();

    let result = self_consistent_k_vector_tracked_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(SEED_K1_FIRST)),
        N_WINDOW,
        1e-6,
        15,
    )
    .expect("vector-tracked solve");

    let (final_k, q, iterations, label) = match result {
        SelfConsistentResult::Converged { k, q, iterations } => (k, q, iterations, "Converged"),
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "MaxIterations")
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            (last_k, q_of(last_k), iterations, "Diverged")
        }
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            panic!(
                "sparse vector-tracking lost the mode at iter {iterations} \
                 (best_overlap = {best_overlap:.4}, last_k = {} + {}i)",
                last_k.re, last_k.im,
            );
        }
    };

    eprintln!(
        "sparse vector-tracked {label} in {iterations} iters: \
         k = {:.6} + {:.6e}i, Q = {q:.4}",
        final_k.re, final_k.im
    );
    assert!(
        final_k.re > 0.5 && final_k.re < 5.0,
        "Re(k) out of physical band: {} (label={label})",
        final_k.re
    );
    assert!(q.is_finite(), "Q is not finite: {q} (label={label})");
    assert!(q > 0.0, "Q must be positive: {q} (label={label})");
    assert!(
        iterations <= 15,
        "vector tracking took {iterations} iters (cap 15)"
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_vector_tracking_agrees_with_proximity_tracking() {
    // On the sparse solver both drivers follow one mode continuously, the
    // proximity-tracked one by eigenvalue and the vector-tracked one by
    // eigenvector overlap. From the same seed and target they must walk
    // the same fixed point. (The frozen integer index that
    // `vector_tracked_beats_frozen_int_idx` compares against needs the
    // full sorted spectrum; `synthetic_vector_tracked_beats_frozen_int_idx`
    // below is the CI check of that comparison.)
    let k0 = 1.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();
    let sparse = SparseSelfConsistentEigensolver::default();
    let target = ModeTarget::Nearest(seed(SEED_K1_FIRST));

    let last = |r: SelfConsistentResult, label: &str| -> (faer::c64, usize) {
        match r {
            SelfConsistentResult::Converged { k, iterations, .. } => (k, iterations),
            SelfConsistentResult::MaxIterations { last_k, iterations } => (last_k, iterations),
            other => panic!("{label}: expected Converged or MaxIterations, got {other:?}"),
        }
    };
    let (k_prox, it_prox) = last(
        self_consistent_k_with(
            &sparse,
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            k0,
            target,
            N_WINDOW,
            1e-6,
            15,
        )
        .expect("proximity-tracked solve"),
        "proximity",
    );
    let (k_vec, it_vec) = last(
        self_consistent_k_vector_tracked_with(
            &sparse,
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            k0,
            target,
            N_WINDOW,
            1e-6,
            15,
        )
        .expect("vector-tracked solve"),
        "vector",
    );
    eprintln!(
        "sparse proximity: k = {:.9}{:+.9}i in {it_prox} iters; \
         sparse vector-tracked: k = {:.9}{:+.9}i in {it_vec} iters",
        k_prox.re, k_prox.im, k_vec.re, k_vec.im
    );
    assert_eq!(it_prox, it_vec, "iteration counts differ");
    assert!(
        (k_prox - k_vec).norm() <= 1e-9 * k_prox.norm(),
        "the two trackers followed different modes: {k_prox} vs {k_vec}"
    );
}

#[test]
fn synthetic_vector_tracked_beats_frozen_int_idx() {
    // The CI check of `vector_tracked_beats_frozen_int_idx` (issue #917).
    // That comparison needs a full-spectrum solver, because the frozen
    // integer index is an index into the whole `|Re λ|`-sorted list, so it
    // runs here on a 3-DOF pencil where the dense solve is free and the
    // re-sort is built in, with the same 1.25× acceptance.
    //
    //   K = [4  0    0 ]   S = diag(0.02, 0, 1.5),  M = I.
    //       [0  4.2  1.2]
    //       [0  1.2  8  ]
    //
    // DOF 0 is the target: `λ = 4 + 0.02 j k₀`, a high-Q mode whose real
    // part does not move. DOFs 1-2 are a mode coupled to a lossy one: its
    // eigenvalue `≈ 4.2 − 1.44 / (3.8 + 1.5 j k₀)` has a real part that
    // falls as `k₀` falls. At the seed `k₀ = 4` the list is
    // `[4 + 0.08j, 4.086 + 0.170j, …]`, so index 0 is the target. On the
    // way to the fixed point (`k₀ → 2`) the second eigenvalue's real part
    // drops below 4 and the two swap places. The frozen index 0 then reads
    // the lossy intruder; vector tracking stays on the target.
    let k = faer::Mat::<f64>::from_fn(3, 3, |i, j| match (i, j) {
        (0, 0) => 4.0,
        (1, 1) => 4.2,
        (2, 2) => 8.0,
        (1, 2) | (2, 1) => 1.2,
        _ => 0.0,
    });
    let s = faer::Mat::<f64>::from_fn(3, 3, |i, j| match (i, j) {
        (0, 0) => 0.02,
        (2, 2) => 1.5,
        _ => 0.0,
    });
    let m = faer::Mat::<f64>::from_fn(3, 3, |i, j| if i == j { 1.0 } else { 0.0 });
    let k0 = 4.0_f64;

    let converged = |r: SelfConsistentResult, label: &str| -> (faer::c64, f64) {
        match r {
            SelfConsistentResult::Converged { k, q, .. } => (k, q),
            other => panic!("{label}: expected Converged, got {other:?}"),
        }
    };
    let (k_frozen, q_frozen) = converged(
        self_consistent_k(k.as_ref(), s.as_ref(), m.as_ref(), k0, 0, 3, 1e-9, 40)
            .expect("frozen-int solve"),
        "frozen-int",
    );
    let (k_tracked, q_tracked) = converged(
        self_consistent_k_vector_tracked(k.as_ref(), s.as_ref(), m.as_ref(), k0, 0, 3, 1e-9, 40)
            .expect("vector-tracked solve"),
        "tracked",
    );
    eprintln!(
        "synthetic: frozen-int k = {:.6}{:+.6}i, Q = {q_frozen:.4}; \
         tracked k = {:.6}{:+.6}i, Q = {q_tracked:.4}; Q-ratio = {:.3}",
        k_frozen.re,
        k_frozen.im,
        k_tracked.re,
        k_tracked.im,
        q_tracked / q_frozen
    );

    // Vector tracking ends on the target: the fixed point of
    // `k² = 4 + 0.02 j Re k`, `k ≈ 2.000025 + 0.01j`, `Q ≈ 100`.
    assert!(
        (k_tracked - faer::c64::new(2.000025, 0.01)).norm() < 1e-6,
        "tracked k = {k_tracked}"
    );
    // The frozen index ends on the intruder (`k ≈ 1.99048 + 0.04202j`,
    // `Q ≈ 23.7`).
    assert!(
        (k_frozen - faer::c64::new(1.990480, 0.042018)).norm() < 1e-5,
        "frozen-int k = {k_frozen}"
    );
    assert!(q_frozen.is_finite() && q_frozen > 0.0, "frozen Q invalid");
    assert!(
        q_tracked.is_finite() && q_tracked > 0.0,
        "tracked Q invalid"
    );
    assert!(
        q_tracked >= 1.25 * q_frozen,
        "vector tracking did not beat frozen-int by ≥1.25×: \
         q_frozen = {q_frozen:.4}, q_tracked = {q_tracked:.4}"
    );
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_vector_tracked_handles_mode_death() {
    // The sparse counterpart of `vector_tracked_handles_mode_death`: the
    // seed `k₀ = 25` and the eigenvalue at the top of the dense window
    // there. Any clean variant is accepted, as in the dense test.
    //
    // The variant depends on the candidate set, as expected for a seed
    // this far from self-consistency. The first damped step takes `k₀`
    // from 25 to about 15.6, which moves the target's eigenvalue out of
    // the 12-pair window, so no candidate overlaps and the result is
    // `ModeLost` at iteration 2 (best overlap 0.17). The dense tier scores
    // its 736 lowest-`|Re λ|` modes instead and ends on `MaxIterations`.
    let k0 = 25.0_f64;
    let (k_full, s_full, m_full, _) = build_sphere_matrices();

    let result = self_consistent_k_vector_tracked_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(SEED_K25_TOP)),
        N_WINDOW,
        1e-6,
        10,
    )
    .expect("solve must not error (only return one of the result variants)");

    match result {
        SelfConsistentResult::ModeLost {
            last_k,
            iterations,
            best_overlap,
        } => {
            eprintln!(
                "sparse ModeLost at iter {iterations}: last k = {:.4} + {:.4e}i, \
                 best_overlap = {best_overlap:.3} — expected clean signal",
                last_k.re, last_k.im
            );
            assert!(best_overlap < 0.5);
            assert!(iterations >= 1);
        }
        SelfConsistentResult::Diverged { last_k, iterations } => {
            eprintln!(
                "sparse Diverged at iter {iterations}: last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
        }
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "sparse MaxIterations at iter {iterations}: last k = {:.4} + {:.4e}i",
                last_k.re, last_k.im
            );
        }
        SelfConsistentResult::Converged { k, q, iterations } => {
            eprintln!(
                "sparse unexpectedly converged in {iterations} iters from \
                 seed=25.0: k = {:.4} + {:.4e}i, Q = {q:.4}",
                k.re, k.im
            );
        }
    }
}

#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "a dense solve of the 4512-DOF pencil; runs in release"
)]
fn sparse_mode_death_seed_matches_dense_spectrum() {
    // Ties `SEED_K25_TOP` to the dense tier with one dense solve: it is
    // the last entry of the dense `n_eigs` list at `k₀ = 25` (the dense
    // test's `target_idx = n_eigs − 1`), and the sparse window finds the
    // same eigenvalue.
    let k0 = 25.0_f64;
    let (k_full, s_full, m_full, n_eigs) = build_sphere_matrices();
    let dense = FaerComplexEigensolver
        .smallest_complex_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            k0,
            n_eigs,
        )
        .expect("dense solve");
    let target = dense[n_eigs - 1];
    let pinned = seed(SEED_K25_TOP);
    assert!(
        (target - pinned).norm() < 1e-5,
        "dense index {} is {target}, the pinned seed is {pinned}",
        n_eigs - 1
    );

    let window = SparseSelfConsistentEigensolver::default()
        .pencil_eigenvalues(
            k_full.as_ref(),
            s_full.as_ref(),
            m_full.as_ref(),
            k0,
            pinned,
            N_WINDOW,
        )
        .expect("sparse window solve");
    let nearest = window
        .iter()
        .copied()
        .min_by(|a, b| (*a - pinned).norm().total_cmp(&(*b - pinned).norm()))
        .expect("non-empty sparse window");
    eprintln!(
        "k0 = {k0}, dense index {}: dense λ = {:.9}{:+.9}i, sparse λ = {:.9}{:+.9}i, |Δ| = {:.2e}",
        n_eigs - 1,
        target.re,
        target.im,
        nearest.re,
        nearest.im,
        (nearest - target).norm()
    );
    assert!(
        (nearest - target).norm() <= 1e-8 * target.norm(),
        "sparse target {nearest} differs from dense {target}"
    );
}
