//! Vector-tracked self-consistent `k₀` iteration on the Silver-Müller
//! pencil (issue #48).
//!
//! Companion to `tests/silvermuller_self_consistent.rs`. The frozen-int
//! variant in PR #47 hit Q ≈ 0.54 on the bundled sphere fixture. Vector
//! tracking picks the mode with maximum bilinear-M-overlap against the
//! prior iteration's target, metric-consistent with the complex-symmetric
//! pencil, so it follows one eigenvector while the `|Re λ|` order
//! re-shuffles as `k₀` drifts. (The `Q ≈ 0.54` itself was not a lost
//! resonance: from the seed `k₀ = 1` both drivers follow the same
//! overdamped Silver-Müller mode, issue #940 and below.)
//!
//! # Two tiers (issue #917)
//!
//! As in `silvermuller_self_consistent.rs` (see its module docs for what
//! the target modes are, and for what proximity targeting does and does not
//! guarantee). The `sparse_*` tests run the vector-tracked driver on
//! [`SparseSelfConsistentEigensolver`] from the dense tier's seeds and run
//! in CI, in the release default tier:
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
//! # Which mode is the resonance (issue #940)
//!
//! The dense tier's frozen target, "the first index past the null
//! cluster", is the sphere's resonance only at the seed `k₀ = 20`. At
//! `k₀ = 1` it is an overdamped mode of the Silver-Müller term
//! (`silvermuller_self_consistent.rs` module docs), so
//! `vector_tracked_converges_on_lowest_mode` and its sparse counterpart
//! follow that mode, not the lowest resonance. The resonance is the `l = 1`
//! branch of `common/silvermuller_sphere.rs`: its self-consistent value on
//! this fixture is `k* = 0.557836 + 0.427141j` (`Q = 0.653`), computed from
//! the continuum problem in `analytic_sm_resonance_branch`. The value
//! `k ≈ 1.19`, `Q ≈ 18` is the same mode at `k₀ = 20`, where the boundary
//! term nearly makes the box a closed cavity; it is not self-consistent.
//!
//! - `sparse_vector_tracked_converges_on_sphere_resonance` (CI) runs the
//!   vector-tracked driver from `k₀ = 20` on that mode and checks the
//!   converged `k` and `Q` against `k*`, with the eigenvector overlap of
//!   every step.
//! - `vector_tracked_beats_frozen_int_idx` (dense tier) runs both drivers
//!   from that seed on the full spectrum: vector tracking converges on the
//!   resonance, the frozen index changes eigenvector and ends on the
//!   Silver-Müller mode. Until issue #940 this test started from `k₀ = 1`,
//!   where both drivers follow the same Silver-Müller mode, and failed its
//!   own 1.25× `Q` ratio (`Q = 0.5340` both).
//!   `synthetic_vector_tracked_beats_frozen_int_idx` checks the same
//!   comparison in CI on a 3-DOF pencil.
//!
//! `sparse_vector_tracking_agrees_with_proximity_tracking` checks that the
//! two sparse drivers agree on the sphere from the seed `k₀ = 1`. They do
//! not agree from every seed: from the second seed of
//! `silvermuller_self_consistent.rs` the proximity pick changes
//! eigenvector and vector tracking reports `ModeLost`
//! (`sparse_proximity_second_seed_changes_eigenvector` there).
//!
//! The sparse tier does not reproduce every dense outcome. The
//! vector-tracked driver can only pick a candidate its window holds, so
//! mode death from the seed `k₀ = 25` is `ModeLost` at iteration 2 here and
//! `MaxIterations(10)` in the dense tier, which scores 736 modes.

#[path = "common/silvermuller_sphere.rs"]
mod sm_sphere;

use geode_core::analytic::mie::{MiePolarisation, open_space_wgm_roots_n15};
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::self_consistent::{
    ModeTarget, SelfConsistentEigensolver, SelfConsistentResult, SparseSelfConsistentEigensolver,
    self_consistent_k, self_consistent_k_vector_tracked, self_consistent_k_vector_tracked_with,
    self_consistent_k_with,
};
use sm_sphere::{
    OuterBc, Recording, Solve, build_sphere_matrices, build_sphere_system, index_trace, m_bilinear,
    min_overlap, nearest_to_sigma, nonzeros, q_of, seed, sm_root, sm_self_consistent_k,
    tracked_trace,
};

/// Eigenvalues requested per sparse solve: the window nearest the target.
///
/// 12 is a cost choice, not a derived bound, and the outcome of a
/// vector-tracked run depends on it: the driver can only pick a candidate
/// the window holds. It is enough from the seeds `k₀ = 1`
/// (`sparse_vector_tracked_converges_on_lowest_mode` panics on `ModeLost`
/// and passes) and `k₀ = 20`
/// (`sparse_vector_tracked_converges_on_sphere_resonance`). It is not
/// enough from the seed `k₀ = 25`: windows of 12 and 48 lose the mode
/// (`ModeLost` at iteration 2), and a window of 160 tracks it to
/// convergence (`sparse_vector_tracked_handles_mode_death`).
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
/// Dense index 368 at `k₀ = 20`: the first index past the null cluster
/// there, which is the `l = 1` resonance at its near-PEC value (analytic
/// `λ = 1.406943 + 0.077894j`). The seed of
/// `sparse_vector_tracked_converges_on_sphere_resonance` and
/// `vector_tracked_beats_frozen_int_idx`. The same constant as in
/// `silvermuller_self_consistent.rs`, where
/// `sparse_window_matches_dense_spectrum_at_seeds` checks it against a
/// dense solve.
const SEED_K20_FIRST: (f64, f64) = (1.417260, 0.078003);

/// Relative tolerance between the FEM self-consistent `k` of the `l = 1`
/// resonance and the continuum value `k*`: the discretization error of the
/// 4512-DOF fixture. Measured 0.88 % (sparse and dense runs); at `k₀ = 20` the
/// same mode's `λ` is 0.73 % from the continuum value.
const RESONANCE_K_REL_TOL: f64 = 0.02;

/// Per-step report of a recorded dense run (issue #1021), one line per
/// solve, for the committed logs under
/// `benchmarks/mie_sphere/runs/*_sm_vector_tracking_dense/`.
///
/// It scores every candidate of a solve against the previous pick twice:
/// with the driver's phase-invariant score
/// `|uᵀ M v| / √(|uᵀ M u| · |vᵀ M v|)` (`new`) and with the score the driver
/// used before issue #988, `|uᵀ M v| / √(|Re uᵀ M u| · |Re vᵀ M v|)`
/// (`legacy`), on the same eigenvectors. Each line gives the pick (its index
/// in the `|Re λ|`-sorted list, `λ`, `k = √λ`, `Q`, both scores), the two
/// best other candidates under the new score, and the candidate the legacy
/// score would have picked. `frozen` is the frozen index of a
/// `ModeTarget::Index` run of `self_consistent_k_with`; `None` scores a
/// vector-tracked run, whose pick after the first solve is the candidate of
/// largest new score (the first one on a tie, as in the driver).
///
/// The legacy column is a counterfactual for one step: it says what the old
/// score would have picked from this solve's candidates, not what an
/// old-score run would have done, because that run's later solves depend on
/// its earlier picks.
fn dense_step_report(
    solves: &[Solve],
    m: faer::MatRef<f64>,
    first: usize,
    frozen: Option<usize>,
    label: &str,
) {
    let m = nonzeros(m);
    let mut prev: Option<&Vec<faer::c64>> = None;
    for (it, solve) in solves.iter().enumerate() {
        let n = solve.pairs.len();
        let head = format!("[{label}] it {:2} k0 = {:.6} n = {n}", it + 1, solve.k0);
        let describe = |idx: usize| {
            let lambda = solve.pairs[idx].0;
            let k = geode_core::eigen::wavenumber::principal_sqrt(lambda);
            format!(
                "idx {idx} λ = {:.6}{:+.6}i k = {:.6}{:+.6}i Q = {:.4}",
                lambda.re,
                lambda.im,
                k.re,
                k.im,
                q_of(k)
            )
        };
        let picked = match prev {
            None => {
                let picked = frozen.unwrap_or(first);
                eprintln!("{head} pick {} (seed)", describe(picked));
                picked
            }
            Some(u) => {
                let uu = m_bilinear(&m, u, u);
                let scores: Vec<(f64, f64)> = solve
                    .pairs
                    .iter()
                    .map(|(_, v)| {
                        let uv = m_bilinear(&m, u, v).norm();
                        let vv = m_bilinear(&m, v, v);
                        (
                            uv / (uu.norm() * vv.norm()).sqrt().max(1e-300),
                            uv / (uu.re.abs() * vv.re.abs()).sqrt().max(1e-300),
                        )
                    })
                    .collect();
                // Descending by the new score; the sort is stable, so the
                // first index wins a tie, as in the driver.
                let mut by_new: Vec<usize> = (0..n).collect();
                by_new.sort_by(|&a, &b| scores[b].0.total_cmp(&scores[a].0));
                let picked = frozen.unwrap_or(by_new[0]);
                let mut legacy_best = 0;
                for j in 1..n {
                    if scores[j].1 > scores[legacy_best].1 {
                        legacy_best = j;
                    }
                }
                let others: Vec<String> = by_new
                    .iter()
                    .filter(|&&j| j != picked)
                    .take(2)
                    .map(|&j| {
                        format!(
                            "idx {j} λ = {:.6}{:+.6}i new = {:.4} legacy = {:.4}",
                            solve.pairs[j].0.re, solve.pairs[j].0.im, scores[j].0, scores[j].1
                        )
                    })
                    .collect();
                eprintln!(
                    "{head} pick {} new = {:.4} legacy = {:.4} | next: {} | legacy argmax: idx \
                     {legacy_best} legacy = {:.4} ({})",
                    describe(picked),
                    scores[picked].0,
                    scores[picked].1,
                    others.join("; "),
                    scores[legacy_best].1,
                    if legacy_best == picked {
                        "the pick"
                    } else {
                        "NOT the pick"
                    }
                );
                picked
            }
        };
        prev = Some(&solve.pairs[picked].1);
    }
}

#[test]
#[ignore = "dense reference tier (#917): 10 to 30 full dense solves of the 4512-DOF pencil with eigenvectors, tens of minutes in release; CI runs the sparse_* tier"]
fn vector_tracked_converges_on_lowest_mode() {
    // Seed `k₀ = 1`, target the first index past the null cluster. The
    // name is inherited: that index is not the lowest resonance but an
    // overdamped Silver-Müller mode (`λ ≈ 0.125 + 2.297j`, issue #940), and
    // this run follows that mode to `MaxIterations(15)` at
    // `k = 1.2261 + 1.1480j` (`Q = 0.534`). The resonance is checked by
    // `sparse_vector_tracked_converges_on_sphere_resonance`.
    //
    // Acceptance: the run produces a valid result variant (no panic),
    // does not lose its mode, and ends with `Re k` in band and `Q > 0`.
    let seed = 1.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed);
    eprintln!("vector-tracked: n_eigs={n_eigs}, first physical idx={first_physical}");
    let t0 = std::time::Instant::now();

    // `self_consistent_k_vector_tracked` is this call with the dense solver
    // and `ModeTarget::Index`; the recording wrapper passes the solver's
    // pairs through unchanged and keeps them for the per-step report.
    let dense = FaerComplexEigensolver;
    let recording = Recording::new(&dense);
    let result = self_consistent_k_vector_tracked_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        ModeTarget::Index(first_physical),
        n_eigs,
        1e-6,
        15,
    )
    .expect("vector-tracked solve");
    eprintln!("lowest-mode: {result:?} after {:?}", t0.elapsed());
    dense_step_report(
        &recording.solves.borrow(),
        m_full.as_ref(),
        first_physical,
        None,
        "dense vector-tracked, k0=1",
    );
    let trace = tracked_trace(
        &recording.solves.borrow(),
        m_full.as_ref(),
        |_| first_physical,
        "dense vector-tracked, k0=1 (trace)",
    );
    eprintln!("lowest-mode: min overlap {:.4}", min_overlap(&trace));

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

/// The load-bearing comparison of #48 on the sphere, from a seed where the
/// frozen index does lose the resonance (issue #940): `k₀ = 20`, target
/// dense index 368, which there is the `l = 1` resonance
/// (`SEED_K20_FIRST`). Both drivers run on the full dense spectrum, and
/// every solve is recorded so the test scores the eigenvector each driver
/// picked.
///
/// - **Vector tracking** follows one eigenvector and converges on the
///   resonance: `k` within [`RESONANCE_K_REL_TOL`] of the continuum
///   `k* = 0.557836 + 0.427141j`. Measured: `Converged(28)` at
///   `k = 0.562710 + 0.430975j`, `Q = 0.6528` (0.88 % from `k*`), the same
///   picks as the sparse run of
///   `sparse_vector_tracked_converges_on_sphere_resonance` to `1e-12`. That
///   measurement predates issue #988: the overlap score then divided by
///   `|Re vᵀ M v|`, and the dense eigenvectors' arbitrary phases inflated
///   it (1.05 to 5.98 for the pick). The score now uses moduli and does
///   not depend on the phase, so the dense picks, being the sparse run's
///   eigenvectors up to a complex scale, should score what they score there
///   (0.879 at iteration 5, 0.999 or more elsewhere). That is not yet
///   re-measured on this tier. The test asserts that no step falls below
///   0.85. It does not assert the sparse test's margin over the best
///   unpicked candidate: the dense list holds 1104 candidates, not about
///   14, and their scores have not been measured.
/// - **The frozen index** changes eigenvector (a step with overlap below
///   0.5) and ends where the seed `k₀ = 1` run ends, on the overdamped
///   Silver-Müller mode (`MaxIterations(20)`, `k = 1.2262 + 1.1481j`), not
///   on the resonance. Measured: the change is at iteration 5 (the first
///   undamped step, `k₀` from 3.54 to 1.17), overlap 0.0021, where index
///   368 becomes a Silver-Müller mode (`λ = 0.170 + 2.692j`) and the
///   resonance has moved to `|Re λ|` rank 767.
///
/// Measured 7581 s (126 min) in release on a 28-core host at load 110 to
/// 130: 20 frozen and 28 tracked dense solves with 1104 eigenvectors each.
///
/// The test used to start from `k₀ = 1`, where index 368 is that
/// Silver-Müller mode from the start, both drivers follow it, and its
/// assertion (tracked `Q` at least 1.25 times frozen `Q`) failed with
/// `Q = 0.5340` both. A `Q` ratio would not be a sound test here either:
/// the resonance has `Q = 0.653` and the Silver-Müller mode `Q = 0.534`, a
/// ratio of 1.22. The assertions are on the eigenvector and on `k*`.
#[test]
#[ignore = "dense reference tier (#917, #940): 49 full dense solves of the 4512-DOF pencil with 1104 eigenvectors each, 126 min in release on a loaded host; CI runs sparse_vector_tracked_converges_on_sphere_resonance and synthetic_vector_tracked_beats_frozen_int_idx"]
fn vector_tracked_beats_frozen_int_idx() {
    let seed_k0 = 20.0_f64;
    let (k_full, s_full, m_full, n_eigs, first_physical) = build_sphere_system(seed_k0);
    // A longer list than the dense tier's 736 (twice the null-space
    // dimension): on the way to `k*` the resonance sorts at `|Re λ|` rank
    // 767 (measured at `k₀ = 1.17` and at `k₀ = 0.563`), behind several
    // hundred Silver-Müller modes, so a 736-entry list does not hold it
    // and vector tracking would report `ModeLost`. The frozen pick, entry
    // 368, does not depend on the length.
    let n_eigs = 3 * n_eigs / 2;
    let (k_star, _) = sm_self_consistent_k();
    let dense = FaerComplexEigensolver;
    let t0 = std::time::Instant::now();

    // Frozen index.
    let recording = Recording::new(&dense);
    let frozen = self_consistent_k_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed_k0,
        ModeTarget::Index(first_physical),
        n_eigs,
        1e-6,
        20,
    )
    .expect("frozen-int solve");
    let frozen_trace = index_trace(
        &recording.solves.borrow(),
        m_full.as_ref(),
        first_physical,
        "dense frozen index, k0=20",
    );
    eprintln!("frozen: {frozen:?} after {:?}", t0.elapsed());
    dense_step_report(
        &recording.solves.borrow(),
        m_full.as_ref(),
        first_physical,
        Some(first_physical),
        "dense frozen index, k0=20 (report)",
    );

    // Vector tracking from the same seed and index.
    let recording = Recording::new(&dense);
    let tracked = self_consistent_k_vector_tracked_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed_k0,
        ModeTarget::Index(first_physical),
        n_eigs,
        1e-6,
        40,
    )
    .expect("vector-tracked solve");
    let tracked_trace = tracked_trace(
        &recording.solves.borrow(),
        m_full.as_ref(),
        |_| first_physical,
        "dense vector-tracked, k0=20",
    );
    eprintln!("tracked: {tracked:?} after {:?}", t0.elapsed());
    dense_step_report(
        &recording.solves.borrow(),
        m_full.as_ref(),
        first_physical,
        None,
        "dense vector-tracked, k0=20 (report)",
    );

    // Both start on the resonance.
    assert_eq!(first_physical, 368);
    for trace in [&frozen_trace, &tracked_trace] {
        assert!(
            (trace[0].lambda - seed(SEED_K20_FIRST)).norm() < 1e-5,
            "seed pick {}",
            trace[0].lambda
        );
    }

    // Vector tracking: one eigenvector, converged on the resonance.
    let (k_tracked, q_tracked) = match tracked {
        SelfConsistentResult::Converged { k, q, .. } => (k, q),
        other => panic!("vector tracking: expected Converged, got {other:?}"),
    };
    let rel = (k_tracked - k_star).norm() / k_star.norm();
    eprintln!(
        "tracked k = {:.6}{:+.6}i (Q = {q_tracked:.4}), {:.2} % from k* = {:.6}{:+.6}i; \
         min overlap {:.4}",
        k_tracked.re,
        k_tracked.im,
        100.0 * rel,
        k_star.re,
        k_star.im,
        min_overlap(&tracked_trace)
    );
    assert!(
        rel <= RESONANCE_K_REL_TOL,
        "vector tracking ended {:.2} % from the resonance",
        100.0 * rel
    );
    assert!(min_overlap(&tracked_trace) > 0.85);

    // Frozen index: changes eigenvector and ends off the resonance.
    let k_frozen = match frozen {
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            assert_eq!(iterations, 20);
            last_k
        }
        other => panic!("frozen index: expected MaxIterations(20), got {other:?}"),
    };
    let hops: Vec<usize> = frozen_trace
        .iter()
        .enumerate()
        .filter(|(_, s)| s.overlap.is_some_and(|o| o < 0.5))
        .map(|(i, _)| i + 1)
        .collect();
    eprintln!(
        "frozen k = {:.6}{:+.6}i (Q = {:.4}); changes eigenvector at iterations {hops:?}",
        k_frozen.re,
        k_frozen.im,
        q_of(k_frozen)
    );
    assert!(
        !hops.is_empty(),
        "the frozen index kept one eigenvector: min overlap {}",
        min_overlap(&frozen_trace)
    );
    assert!(
        (k_frozen - faer::c64::new(1.2262, 1.1481)).norm() < 1e-3,
        "frozen k = {k_frozen}"
    );
    assert!((k_frozen - k_star).norm() > 0.5);
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

    let t0 = std::time::Instant::now();
    let dense = FaerComplexEigensolver;
    let recording = Recording::new(&dense);
    let result = self_consistent_k_vector_tracked_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        seed,
        ModeTarget::Index(n_eigs - 1),
        n_eigs,
        1e-6,
        10,
    )
    .expect("solve must not error (only return one of the result variants)");
    eprintln!("mode-death: {result:?} after {:?}", t0.elapsed());
    dense_step_report(
        &recording.solves.borrow(),
        m_full.as_ref(),
        n_eigs - 1,
        None,
        "dense vector-tracked, k0=25",
    );
    let trace = tracked_trace(
        &recording.solves.borrow(),
        m_full.as_ref(),
        |_| n_eigs - 1,
        "dense vector-tracked, k0=25 (trace)",
    );
    eprintln!("mode-death: min overlap {:.4}", min_overlap(&trace));

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
// Sparse tier (issue #917): the dense tier's seeds on the sparse windowed
// solver, with the dense tests' assertions and no tolerance loosened. The
// first-seed runs end where the dense ones do; the mode-death run does not
// (module docs).
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
    // From this seed the two sparse drivers walk the same fixed point: the
    // proximity one picks the nearest eigenvalue, the vector-tracked one
    // the largest eigenvector overlap, and here those are the same
    // candidate at every step (overlap 1.0000, next best 0.004). That is a
    // property of this seed, not of proximity targeting: from the second
    // seed the nearest eigenvalue is a different eigenvector four times
    // (`sparse_proximity_second_seed_changes_eigenvector`). (The frozen
    // integer index that
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

/// The analytic reference of the sphere tests (issue #940;
/// `common/silvermuller_sphere.rs` module docs): the `l = 1` branch of the
/// continuum fixture, from the PEC limit through Silver-Müller to open
/// space. Pure arithmetic, milliseconds.
#[test]
fn analytic_sm_resonance_branch() {
    let close = |a: faer::c64, b: (f64, f64), tol: f64, what: &str| {
        assert!(
            (a - seed(b)).norm() < tol,
            "{what}: {:.9}{:+.9}i, expected {}{:+}i",
            a.re,
            a.im,
            b.0,
            b.1
        );
    };

    // PEC wall, and Silver-Müller at a large k₀ approaching it.
    let (k_pec, r) = sm_root(faer::c64::new(1.19, 0.0), OuterBc::Pec);
    assert!(r < 1e-9, "PEC residual {r:e}");
    close(k_pec, (1.187095, 0.0), 1e-6, "PEC root");
    let (k_big, r) = sm_root(k_pec, OuterBc::SilverMuller(1e6));
    assert!(r < 1e-9, "k0 = 1e6 residual {r:e}");
    assert!((k_big - k_pec).norm() < 1e-5, "k0 = 1e6: {k_big}");

    // Silver-Müller at the seed k₀ = 20: the mode the FEM pencil has at
    // `SEED_K20_FIRST`, within the fixture's discretization error.
    let (k20, r) = sm_root(k_pec, OuterBc::SilverMuller(20.0));
    assert!(r < 1e-9, "k0 = 20 residual {r:e}");
    close(k20, (1.186600, 0.032822), 1e-6, "k0 = 20");
    assert!(
        (q_of(k20) - 18.08).abs() < 0.01,
        "Q at k0 = 20: {}",
        q_of(k20)
    );
    let fem = seed(SEED_K20_FIRST);
    let rel = (fem - k20 * k20).norm() / (k20 * k20).norm();
    eprintln!(
        "k0 = 20: analytic λ = {:.6}{:+.6}i, FEM λ = {fem}, {:.2} % apart",
        (k20 * k20).re,
        (k20 * k20).im,
        100.0 * rel
    );
    assert!(
        rel < 0.01,
        "FEM seed {:.2} % from the analytic λ",
        100.0 * rel
    );

    // The self-consistent resonance, k₀ = Re k*.
    let (k_star, r) = sm_self_consistent_k();
    assert!(r < 1e-9, "k* residual {r:e}");
    close(k_star, (0.557836, 0.427141), 1e-6, "k*");
    assert!((q_of(k_star) - 0.653).abs() < 1e-3, "Q* = {}", q_of(k_star));
    let (k_again, _) = sm_root(k_star, OuterBc::SilverMuller(k_star.re));
    assert!((k_again - k_star).norm() < 1e-9, "k* is not a fixed point");
    // The fixed-point map k₀ ↦ Re k(k₀) contracts there (slope 0.588).
    let h = 1e-5;
    let re_k_at = |k0: f64| sm_root(k_star, OuterBc::SilverMuller(k0)).0.re;
    let slope = (re_k_at(k_star.re + h) - re_k_at(k_star.re - h)) / (2.0 * h);
    eprintln!("k* = {k_star}, Q* = {:.4}, slope {slope:.4}", q_of(k_star));
    assert!((slope - 0.588).abs() < 1e-3, "slope {slope}");

    // The homotopy to the exact outgoing condition ends on the open-space
    // Mie pole of the catalog (conjugated: this sign convention has
    // Im k > 0 for a decaying mode). Continuation in t with steps that
    // move k by at most 0.02, so the path cannot jump to another root;
    // it speeds up near t = 1 (|dk/dt| about 4.6) but stays smooth.
    let (mut k, mut t, mut dt, mut steps) = (k_star, 0.0_f64, 0.025_f64, 0);
    while t < 1.0 {
        let t_next = (t + dt).min(1.0);
        let (next, r) = sm_root(k, OuterBc::Homotopy(t_next));
        if (next - k).norm() > 0.02 {
            dt /= 2.0;
            assert!(dt > 1e-6, "homotopy stalled at t = {t}: {k} -> {next}");
            continue;
        }
        assert!(r < 1e-9, "homotopy t = {t_next}: residual {r:e}");
        if (next - k).norm() < 0.005 {
            dt *= 1.5;
        }
        (k, t, steps) = (next, t_next, steps + 1);
    }
    eprintln!("homotopy: {steps} steps to k = {k}");
    // The branch is the a₁ electric dipole (`u′/ε` row), so the catalog
    // must call it TM_1,1 (issue #999 corrected the catalog, which had
    // TE and TM swapped and called this root TE_1,1).
    let mie = open_space_wgm_roots_n15()[0];
    assert_eq!((mie.pol, mie.l, mie.n), (MiePolarisation::TM, 1, 1));
    close(k, (mie.re_k, -mie.im_k), 1e-8, "open space");
}

/// The self-consistent resonance of the sphere fixture, in CI (issue #940).
///
/// The vector-tracked driver on the sparse solver, from `k₀ = 20` on the
/// `l = 1` resonance (`SEED_K20_FIRST`), with a window of [`N_WINDOW`].
/// Measured (release):
///
/// - `Converged(28)`, `k = 0.562710 + 0.430975j`, `Q = 0.6528`: 0.88 % from
///   the continuum `k* = 0.557836 + 0.427141j` (`Q* = 0.6530`), the
///   discretization error of the fixture.
/// - One eigenvector throughout: the overlap of each pick with the previous
///   one is 0.879 at iteration 5 (the first undamped step, `k₀` from 3.54
///   to 1.17), where the best unpicked candidate has 0.314, and 0.999 or
///   more at every other step.
/// - About 40 s on a loaded 28-core host (28 sparse solves).
///
/// It does not end on the `Q ≈ 18` of the same mode at `k₀ = 20`: that is
/// the near-PEC value, not a self-consistent one (module docs).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_vector_tracked_converges_on_sphere_resonance() {
    let t0 = std::time::Instant::now();
    let (k_star, _) = sm_self_consistent_k();
    let (k_full, s_full, m_full, _) = build_sphere_matrices();
    let sparse = SparseSelfConsistentEigensolver::default();
    let recording = Recording::new(&sparse);
    let result = self_consistent_k_vector_tracked_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        20.0,
        ModeTarget::Nearest(seed(SEED_K20_FIRST)),
        N_WINDOW,
        1e-6,
        40,
    )
    .expect("vector-tracked solve");
    let trace = tracked_trace(
        &recording.solves.borrow(),
        m_full.as_ref(),
        nearest_to_sigma,
        "sparse vector-tracked, k0=20",
    );
    eprintln!("{result:?} in {:?}", t0.elapsed());

    let (k, q, iterations) = match result {
        SelfConsistentResult::Converged { k, q, iterations } => (k, q, iterations),
        other => panic!("expected Converged, got {other:?}"),
    };
    // The seed pick is the resonance; the trace is the driver's.
    assert!((trace[0].lambda - seed(SEED_K20_FIRST)).norm() < 1e-5);
    assert_eq!(trace.len(), iterations);
    assert!((trace[iterations - 1].lambda - k * k).norm() < 1e-12);

    // The resonance.
    let rel = (k - k_star).norm() / k_star.norm();
    eprintln!(
        "k = {:.6}{:+.6}i, Q = {q:.4}; k* = {:.6}{:+.6}i, Q* = {:.4}; {:.2} % apart",
        k.re,
        k.im,
        k_star.re,
        k_star.im,
        q_of(k_star),
        100.0 * rel
    );
    assert!(
        rel <= RESONANCE_K_REL_TOL,
        "k is {:.2} % from the resonance k*",
        100.0 * rel
    );
    assert!(
        (q - q_of(k_star)).abs() <= 0.05 * q_of(k_star),
        "Q = {q}, Q* = {}",
        q_of(k_star)
    );
    // The deterministic outcome.
    assert_eq!(iterations, 28);
    assert!(
        (k - faer::c64::new(0.562710, 0.430975)).norm() < 1e-5,
        "k = {k}"
    );

    // One eigenvector: every pick continues the previous one, by a clear
    // margin over the other candidates.
    for (it, step) in trace.iter().enumerate().skip(1) {
        let overlap = step.overlap.expect("overlap past the first solve");
        let best = step.best_unpicked.expect("more than one candidate");
        assert!(
            overlap > 0.85 && overlap - best > 0.5,
            "iteration {}: overlap {overlap:.4}, best unpicked {best:.4}",
            it + 1
        );
    }
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
    // From the seed of `vector_tracked_handles_mode_death`: `k₀ = 25` and
    // the eigenvalue at the top of the dense window there. The dense test
    // accepts any clean variant. This one asserts the variant it gets,
    // `ModeLost` at iteration 2, so the mode-death path stays covered.
    //
    // The variant depends on the candidate set, as expected for a seed
    // this far from self-consistency. The first damped step takes `k₀`
    // from 25 to about 15.6, which moves the target's eigenvalue out of
    // the 12-pair window, so no candidate overlaps and the result is
    // `ModeLost` at iteration 2 (best overlap 0.173). The dense tier
    // scores its 736 lowest-`|Re λ|` modes instead and ends on
    // `MaxIterations(10)` at `k = 2.2580 + 2.0224j`: a different outcome,
    // not the same scenario. A sparse window of 48 also loses the mode and
    // one of 160 tracks it to `Converged` (`k = 7.3656 + 0.3593j`),
    // measured for PR #941 and not run here.
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
                 best_overlap = {best_overlap:.3}",
                last_k.re, last_k.im
            );
            assert!(best_overlap < 0.5);
            assert_eq!(iterations, 2);
        }
        other => {
            panic!("expected ModeLost at iteration 2 at a window of {N_WINDOW}, got {other:?}")
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
