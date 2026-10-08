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
//! target, and the target is picked by proximity
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
//! smaller `|Re λ|` than the resonance and sort ahead of it.
//!
//! # What the two tiers share, and what they do not
//!
//! The sparse tier **starts** from the same eigenvalues as the dense tier:
//! the `SEED_*` constants below, which
//! `sparse_window_matches_dense_spectrum_at_seeds` checks, with the sparse
//! window around each, against a dense solve in CI. After the first solve
//! the two tiers use different target rules, so they are not the same
//! scenarios and only one of the three ends on the same point:
//!
//! | Seed | Dense, frozen index | Sparse, [`ModeTarget::Nearest`] |
//! |---|---|---|
//! | `k₀ = 1`, index 368 | `MaxIterations(20)`, `k = 1.226173 + 1.148073j` | the same point; overlap 1.0000 at every step |
//! | `k₀ = 20`, index 368 | `MaxIterations(20)`, `k = 1.2262 + 1.1481j` | `MaxIterations(20)`, `k = 0.562802 + 0.431012j`; overlap 0.879 or more at every step |
//! | `k₀ = 1`, index 369 | `MaxIterations(20)`, `k = 1.6413 + 1.5285j` | `Converged(16)`, `k = 1.306367 + 1.176238j`; the pick changes eigenvector at iterations 2 to 5 |
//!
//! "Overlap" is the bilinear M-overlap between the eigenvectors of two
//! successive picks, the score the vector-tracked driver uses. The sparse
//! tests measure it on every run ([`proximity_trace`]) and assert it.
//!
//! **Proximity targeting follows the nearest eigenvalue, not the same
//! eigenvector.** The pick continues the previous mode only if, after the
//! `k₀` step, that mode's eigenvalue is still the one nearest its old
//! position. A step small against the local eigenvalue spacing guarantees
//! that. None of the three sphere runs has such a step (first step in `λ`
//! against spacing at the seed: 0.117 against 0.092, 0.070 against 0.0009,
//! 0.068 against 0.014), so none is guaranteed, and the tests measure what
//! happens. The first two rows keep their mode, the second narrowly (a
//! near-degenerate partner is 0.0002 farther away at iteration 2). The
//! third does not: the run changes eigenvector four times before it
//! settles on a self-consistent mode that is not the continuation of the
//! seed (`sparse_proximity_second_seed_changes_eigenvector`).
//!
//! The property "proximity keeps the mode where a frozen index hops" is
//! true, and checked with the same overlap measurement, on the 3-DOF
//! pencil of `synthetic_proximity_keeps_mode_where_frozen_index_hops`,
//! where the step (0.02) is small against the spacing (0.12). It is not
//! shown on the sphere. Following one eigenvector is the job of the
//! vector-tracked driver
//! (`silvermuller_self_consistent_vector_tracking.rs`).

use burn::tensor::backend::BackendTypes;

use geode_core::assembly::nedelec::{
    assemble_global_nedelec_with_epsilon, build_epsilon_r, sphere_n_interior_nodes,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::assembly::surface::assemble_silver_muller_surface;
use geode_core::eigen::complex::{ComplexEigenSolver, FaerComplexEigensolver};
use geode_core::eigen::dense::{EigenError, burn_matrix_to_faer};
use geode_core::eigen::self_consistent::{
    ModeTarget, SelfConsistentEigensolver, SelfConsistentResult, SparseSelfConsistentEigensolver,
    self_consistent_k, self_consistent_k_vector_tracked_with, self_consistent_k_with,
};
use geode_core::mesh::{PHYS_OUTER_BOUNDARY, read_sphere_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;

fn device() -> <B as BackendTypes>::Device {
    <B as BackendTypes>::Device::default()
}

/// Eigenvalues requested per sparse solve: the window nearest the target.
///
/// 12 is a cost choice, not a derived bound: it is the smallest round
/// window that carries the two runs that do follow one mode (seeds
/// `k₀ = 1` first and `k₀ = 20`). The proximity pick is the eigenvalue
/// nearest the shift, which is in any window, so these three runs do not
/// depend on it: a window of 48 gives the same picks from the second seed.
/// The vector-tracked driver does depend on it, because it can only pick a
/// candidate the window holds: mode death at seed `k₀ = 25` is `ModeLost`
/// at windows of 12 and 48 and tracked to convergence at 160
/// (`silvermuller_self_consistent_vector_tracking.rs`).
const N_WINDOW: usize = 12;

/// Dense index 368 at `k₀ = 1`: the first index past the null cluster, the
/// target of `self_consistent_converges_for_target_mode`.
const SEED_K1_FIRST: (f64, f64) = (0.125079, 2.297035);
/// Dense index 369 at `k₀ = 1`: the frozen target of
/// `frozen_target_idx_prevents_mode_hop`, and the seed of
/// `sparse_proximity_second_seed_changes_eigenvector`.
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
// Sparse tier (issue #917): the dense tier's seeds on the sparse windowed
// solver with proximity targeting. The seeds are the same; the target rule
// is not, so the end points are not all the same (module docs). Each test
// pins its deterministic outcome and the measured overlap between
// successive picks.
// ---------------------------------------------------------------------

/// One recorded eigensolve of a self-consistent run.
struct Solve {
    k0: f64,
    sigma: faer::c64,
    pairs: Vec<(faer::c64, Vec<faer::c64>)>,
}

/// Wraps an eigensolver and records every solve the driver makes, with
/// eigenvectors, so a test can score what the driver picked. It changes
/// nothing about the run: the driver gets the inner solver's eigenvalues.
struct Recording<'a, S: ?Sized> {
    inner: &'a S,
    solves: std::cell::RefCell<Vec<Solve>>,
}

impl<'a, S: SelfConsistentEigensolver + ?Sized> Recording<'a, S> {
    fn new(inner: &'a S) -> Self {
        Self {
            inner,
            solves: std::cell::RefCell::new(Vec::new()),
        }
    }
}

impl<S: SelfConsistentEigensolver + ?Sized> SelfConsistentEigensolver for Recording<'_, S> {
    fn pencil_eigenvalues(
        &self,
        k: faer::MatRef<f64>,
        s: faer::MatRef<f64>,
        m: faer::MatRef<f64>,
        k0: f64,
        sigma: faer::c64,
        n: usize,
    ) -> Result<Vec<faer::c64>, EigenError> {
        Ok(self
            .pencil_eigenpairs(k, s, m, k0, sigma, n)?
            .into_iter()
            .map(|p| p.0)
            .collect())
    }

    fn pencil_eigenpairs(
        &self,
        k: faer::MatRef<f64>,
        s: faer::MatRef<f64>,
        m: faer::MatRef<f64>,
        k0: f64,
        sigma: faer::c64,
        n: usize,
    ) -> Result<Vec<(faer::c64, Vec<faer::c64>)>, EigenError> {
        let pairs = self.inner.pencil_eigenpairs(k, s, m, k0, sigma, n)?;
        self.solves.borrow_mut().push(Solve {
            k0,
            sigma,
            pairs: pairs.clone(),
        });
        Ok(pairs)
    }
}

/// One step of a recorded run: what was picked, and how it relates to the
/// previous pick.
#[derive(Debug, Clone, Copy)]
struct TraceStep {
    k0: f64,
    /// The picked eigenvalue.
    lambda: faer::c64,
    /// Bilinear M-overlap of the pick with the previous pick (`None` at the
    /// first solve).
    overlap: Option<f64>,
    /// The largest overlap with the previous pick among the candidates that
    /// were **not** picked (`None` at the first solve or with one candidate).
    best_unpicked: Option<f64>,
}

/// The nonzeros of a dense matrix, so the overlaps below cost `O(nnz)`.
fn nonzeros(m: faer::MatRef<f64>) -> Vec<(usize, usize, f64)> {
    let mut out = Vec::new();
    for j in 0..m.ncols() {
        for i in 0..m.nrows() {
            let v = m[(i, j)];
            if v != 0.0 {
                out.push((i, j, v));
            }
        }
    }
    out
}

/// Bilinear form `uᵀ M v` (no conjugation), the inner product of the
/// complex-symmetric pencil.
fn m_bilinear(m: &[(usize, usize, f64)], u: &[faer::c64], v: &[faer::c64]) -> faer::c64 {
    let mut acc = faer::c64::new(0.0, 0.0);
    for &(i, j, mij) in m {
        acc += u[i] * v[j] * mij;
    }
    acc
}

/// `|uᵀ M v| / √(|Re uᵀ M u| · |Re vᵀ M v|)`: the score
/// `self_consistent_k_vector_tracked` gives a candidate `v` against the
/// previous target `u`. 1 for the same eigenvector, 0 for two different
/// eigenvectors of one pencil (they are M-orthogonal).
fn m_overlap(m: &[(usize, usize, f64)], u: &[faer::c64], v: &[faer::c64]) -> f64 {
    let uv = m_bilinear(m, u, v);
    let uu = m_bilinear(m, u, u).re.abs();
    let vv = m_bilinear(m, v, v).re.abs();
    uv.re.hypot(uv.im) / (uu * vv).sqrt().max(1e-300)
}

/// Score a recorded run. `pick` returns the index the driver's target rule
/// selected in a solve.
fn overlap_trace(
    solves: &[Solve],
    m: faer::MatRef<f64>,
    pick: impl Fn(&Solve) -> usize,
) -> Vec<TraceStep> {
    let m = nonzeros(m);
    let mut prev: Option<&Vec<faer::c64>> = None;
    let mut out = Vec::new();
    for solve in solves {
        let picked = pick(solve);
        let (overlap, best_unpicked) = match prev {
            None => (None, None),
            Some(u) => {
                let scores: Vec<f64> = solve
                    .pairs
                    .iter()
                    .map(|(_, v)| m_overlap(&m, u, v))
                    .collect();
                let best_unpicked = scores
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != picked)
                    .map(|(_, s)| *s)
                    .fold(None, |acc: Option<f64>, s| {
                        Some(acc.map_or(s, |a| a.max(s)))
                    });
                (Some(scores[picked]), best_unpicked)
            }
        };
        out.push(TraceStep {
            k0: solve.k0,
            lambda: solve.pairs[picked].0,
            overlap,
            best_unpicked,
        });
        prev = Some(&solve.pairs[picked].1);
    }
    out
}

/// Score a recorded [`ModeTarget::Nearest`] run of `self_consistent_k_with`.
///
/// The pick is reconstructed from the record, not re-derived: under
/// `Nearest` the driver passes its anchor (the seed, then the previous
/// target) as the shift `σ` and picks the eigenvalue nearest it. The
/// function checks that reading against the record: every `σ` after the
/// first must be the previous solve's pick, bit for bit.
fn proximity_trace(solves: &[Solve], m: faer::MatRef<f64>, label: &str) -> Vec<TraceStep> {
    let nearest = |solve: &Solve| -> usize {
        solve
            .pairs
            .iter()
            .enumerate()
            .min_by(|a, b| {
                (a.1.0 - solve.sigma)
                    .norm()
                    .total_cmp(&(b.1.0 - solve.sigma).norm())
            })
            .map(|(i, _)| i)
            .expect("non-empty solve")
    };
    let trace = overlap_trace(solves, m, nearest);
    for (i, step) in trace.iter().enumerate() {
        if i > 0 {
            assert_eq!(
                (solves[i].sigma.re, solves[i].sigma.im),
                (trace[i - 1].lambda.re, trace[i - 1].lambda.im),
                "{label}: solve {} was not shifted at the previous pick",
                i + 1
            );
        }
        eprintln!(
            "[{label}] it {:2} k0 = {:.6} n = {:2} pick λ = {:.6}{:+.6}i overlap = {} \
             best unpicked = {}",
            i + 1,
            step.k0,
            solves[i].pairs.len(),
            step.lambda.re,
            step.lambda.im,
            step.overlap.map_or("n/a".into(), |o| format!("{o:.4}")),
            step.best_unpicked
                .map_or("n/a".into(), |o| format!("{o:.4}")),
        );
    }
    trace
}

/// The smallest overlap between successive picks over a whole trace.
fn min_overlap(trace: &[TraceStep]) -> f64 {
    trace
        .iter()
        .filter_map(|s| s.overlap)
        .fold(f64::INFINITY, f64::min)
}

/// Runs proximity targeting on the sparse solver from `(k0, target)` and
/// returns the result with its overlap trace.
fn sparse_proximity_run(
    k0: f64,
    target: (f64, f64),
    label: &str,
) -> (SelfConsistentResult, Vec<TraceStep>) {
    let (k_full, s_full, m_full, _) = build_sphere_matrices();
    let sparse = SparseSelfConsistentEigensolver::default();
    let recording = Recording::new(&sparse);
    let result = self_consistent_k_with(
        &recording,
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        k0,
        ModeTarget::Nearest(seed(target)),
        N_WINDOW,
        1e-6,
        20,
    )
    .expect("solve must not error (only return a result variant)");
    let trace = proximity_trace(&recording.solves.borrow(), m_full.as_ref(), label);
    (result, trace)
}

/// From the dense tier's seed of `self_consistent_converges_for_target_mode`
/// (`k₀ = 1`, dense index 368), with that test's acceptance.
///
/// "Converges" is the inherited name. The run does **not** converge: it
/// ends on `MaxIterations(20)` with `|Δk₀| ≈ 2e-6` against the `1e-6`
/// tolerance, as the dense run does, and at the same point
/// (`k = 1.226173 + 1.148073j`). The proximity pick is the same eigenvector
/// at every step (overlap 1.0000; the best unpicked candidate has 0.004).
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_self_consistent_converges_for_target_mode() {
    let (result, trace) = sparse_proximity_run(1.0, SEED_K1_FIRST, "k0=1 first");

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

    // The deterministic outcome, which is also the dense tier's.
    assert_eq!((label, iterations), ("MaxIterations", 20));
    assert!(
        (final_k - faer::c64::new(1.226173, 1.148073)).norm() < 1e-5,
        "final k = {final_k}"
    );
    // One eigenvector throughout.
    assert_eq!(trace.len(), 20);
    assert!(
        min_overlap(&trace) > 0.999,
        "the pick changed eigenvector: min overlap {}",
        min_overlap(&trace)
    );
}

/// From the dense tier's seed of
/// `self_consistent_diverges_returns_clean_result`: the pathological
/// `k₀ = 20`, targeting the eigenvalue dense index 368 points at there
/// (`λ = 1.41726 + 0.07800j`, the sphere's lowest resonance). The contract
/// inherited from the dense test is a clean variant, not an error or a
/// panic.
///
/// **This run ends on a different point than the dense tier.** Both are
/// `MaxIterations(20)`, but the dense run's last `k` is `1.2262 + 1.1481j`
/// and this one's is `0.562802 + 0.431012j`. The cause is the target rule,
/// not the solver (the dense solver under `Nearest` gave this run's result
/// to 1e-12 when measured once for PR #941; that run is not in CI):
///
/// - The dense run re-reads **index 368** of the `|Re λ|`-sorted list on
///   every solve, whichever eigenvalue holds that place. It ends at the
///   point the `k₀ = 1` run above ends at, the overdamped surface mode.
/// - This run picks the eigenvalue **nearest the previous pick**. Here that
///   does follow one eigenvector: the overlap between successive picks is
///   0.879 or more at every step (the minimum is at iteration 5, where the
///   first undamped step takes `k₀` from 3.54 to 1.17). It does so
///   narrowly. The target has a near-degenerate partner that is 0.0002 to
///   0.0006 farther from the previous pick at iterations 2 to 5 (0.0697
///   against 0.0699 at iteration 2), so this is a measured outcome, not a
///   guarantee of the rule.
///
/// The dense run was not overlap-traced (39 min). That it leaves this
/// run's path follows from the two sharing the first solve and ending
/// apart; at which step, and with what overlap, was not measured.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_self_consistent_diverges_returns_clean_result() {
    let (result, trace) = sparse_proximity_run(20.0, SEED_K20_FIRST, "k0=20 first");

    // The deterministic sparse outcome.
    match result {
        SelfConsistentResult::MaxIterations { last_k, iterations } => {
            eprintln!(
                "sparse max iters {iterations} hit without convergence; \
                 last k = {:.6} + {:.6}i",
                last_k.re, last_k.im
            );
            assert_eq!(iterations, 20);
            assert!(
                (last_k - faer::c64::new(0.562802, 0.431012)).norm() < 1e-5,
                "last k = {last_k}"
            );
            // Not the dense tier's end point.
            assert!((last_k - faer::c64::new(1.2262, 1.1481)).norm() > 0.5);
        }
        other => panic!("expected MaxIterations(20), got {other:?}"),
    }
    // One eigenvector throughout: measured minimum 0.8791.
    assert_eq!(trace.len(), 20);
    assert!(
        min_overlap(&trace) > 0.85,
        "the pick changed eigenvector: min overlap {}",
        min_overlap(&trace)
    );
}

/// Proximity targeting from the dense tier's second seed (`k₀ = 1`, dense
/// index 369, `λ = 0.136539 + 3.060225j`) reaches a fixed point, **on a
/// different mode than the one it was seeded on**.
///
/// This test replaces `sparse_proximity_target_prevents_mode_hop`, whose
/// claim was false: its only assertion (`Re k` above the midpoint 1.18306)
/// also passes on the neighbour it named, which ends at `Re k = 1.2262`.
/// What the run does, measured here on every CI run:
///
/// | Iteration | `k₀` | Pick `λ` | Overlap with previous pick | Best unpicked |
/// |---|---|---|---|---|
/// | 1 | 1.0000 | 0.136539 + 3.060225j (seed) | n/a | n/a |
/// | 2 | 1.1324 | 0.204888 + 3.062373j | 0.0005 | 0.0042 |
/// | 3 | 1.2060 | 0.321538 + 3.110533j | 0.0024 | 0.0109 |
/// | 4 | 1.2595 | 0.329021 + 3.059574j | 0.0180 | 0.9988 |
/// | 5 | 1.3050 | 0.322460 + 3.070058j | 0.0028 | 0.9997 |
/// | 6 to 16 | to 1.3064 | settles on 0.323059 + 3.073197j | 1.0000 | 0.0002 or less |
///
/// - The pick changes eigenvector at iterations 2, 3, 4 and 5. At 4 and 5
///   the continuation of the current pick is in the window and is not the
///   nearest eigenvalue, so it is not picked.
/// - The cause is step size against spacing: the nearest neighbour of the
///   seed is 0.014 away and the first damped step moves the pick by 0.068.
///   A window of 48 gave the same picks in the PR #941 review, so a wider
///   window does not help.
/// - The end point (`Converged(16)`, `k = 1.306367 + 1.176238j`,
///   `Q = 0.5553`) is a genuine self-consistent mode: the last eleven picks
///   are one eigenvector. It is not the continuation of dense index 369,
///   and it is not where the dense frozen index ends either
///   (`MaxIterations(20)`, `k = 1.6413 + 1.5285j`).
/// - The vector-tracked driver from the same seed reports `ModeLost` at
///   iteration 2 (best overlap 0.0042): no candidate in the window
///   continues the seed mode. That is the honest outcome, and the reason to
///   use vector tracking when the identity of the mode matters.
///
/// So on this fixture **no target rule follows dense index 369**: the
/// frozen index, proximity and vector tracking each do something else, and
/// only vector tracking says so. Issue #940 records this next to the
/// frozen-index failure. Where proximity does keep a mode that a frozen
/// index loses, see `synthetic_proximity_keeps_mode_where_frozen_index_hops`.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "slow unoptimized (4512-DOF sparse LU per iteration); runs in release"
)]
fn sparse_proximity_second_seed_changes_eigenvector() {
    let (result, trace) = sparse_proximity_run(1.0, SEED_K1_SECOND, "k0=1 second");

    // The deterministic outcome: a fixed point.
    match result {
        SelfConsistentResult::Converged { k, q, iterations } => {
            eprintln!(
                "sparse proximity converged in {iterations} iters: \
                 k = {:.6} + {:.6}i, Q = {q:.4}",
                k.re, k.im
            );
            assert_eq!(iterations, 16);
            assert!(
                (k - faer::c64::new(1.306367, 1.176238)).norm() < 1e-5,
                "k = {k}"
            );
            assert!((q - 0.5553).abs() < 1e-3, "Q = {q}");
        }
        other => panic!("expected Converged(16), got {other:?}"),
    }
    assert_eq!(trace.len(), 16);

    // It is not the seed mode: the pick changes eigenvector at iterations
    // 2 to 5 (measured overlaps 0.0005, 0.0024, 0.0180, 0.0028).
    for (it, step) in trace.iter().enumerate().map(|(i, s)| (i + 1, s)).skip(1) {
        let overlap = step.overlap.expect("overlap past the first solve");
        if it <= 5 {
            assert!(
                overlap < 0.1,
                "iteration {it}: overlap {overlap:.4}; the pick is expected to change eigenvector"
            );
        } else {
            // From iteration 6 the pick is one eigenvector.
            assert!(
                overlap > 0.999,
                "iteration {it}: overlap {overlap:.4}; the pick is expected to have settled"
            );
        }
    }
    // At iterations 4 and 5 the continuation of the current pick was in the
    // window (0.9988, 0.9997) and proximity passed it over.
    for it in [4usize, 5] {
        let best = trace[it - 1]
            .best_unpicked
            .expect("more than one candidate");
        assert!(
            best > 0.99,
            "iteration {it}: best unpicked overlap {best:.4}"
        );
    }

    // Vector tracking from the same seed reports the loss.
    let (k_full, s_full, m_full, _) = build_sphere_matrices();
    let tracked = self_consistent_k_vector_tracked_with(
        &SparseSelfConsistentEigensolver::default(),
        k_full.as_ref(),
        s_full.as_ref(),
        m_full.as_ref(),
        1.0,
        ModeTarget::Nearest(seed(SEED_K1_SECOND)),
        N_WINDOW,
        1e-6,
        20,
    )
    .expect("vector-tracked solve");
    match tracked {
        SelfConsistentResult::ModeLost {
            iterations,
            best_overlap,
            ..
        } => {
            eprintln!(
                "vector-tracked: ModeLost at iter {iterations}, best overlap {best_overlap:.4}"
            );
            assert_eq!(iterations, 2);
            assert!(best_overlap < 0.1, "best overlap {best_overlap}");
        }
        other => panic!("expected ModeLost at iteration 2, got {other:?}"),
    }
}

/// The property the sphere run above does **not** have, on a fixture where
/// it holds: proximity targeting keeps the mode where a frozen index hops.
///
/// The 3-DOF pencil of `synthetic_vector_tracked_beats_frozen_int_idx`:
///
/// ```text
///   K = [4  0    0 ]   S = diag(0.02, 0, 1.5),  M = I.
///       [0  4.2  1.2]
///       [0  1.2  8  ]
/// ```
///
/// DOF 0 is the target, `λ = 4 + 0.02 j k₀`. DOFs 1-2 carry an intruder,
/// `λ ≈ 4.2 − 1.44 / (3.8 + 1.5 j k₀)`, whose real part falls below 4 on
/// the way from the seed `k₀ = 4` to the fixed point `k₀ ≈ 2`, so the two
/// swap places in the `|Re λ|` order.
///
/// - `ModeTarget::Index(0)` re-reads the first entry and ends on the
///   intruder, `k = 1.990480 + 0.042018j`: the pick at iteration 3 has
///   overlap 0.0000 with the pick at iteration 2.
/// - `ModeTarget::Nearest` seeded at the target stays on it,
///   `k = 2.000025 + 0.01j`, with overlap 1.0000 at each of its 5 steps
///   (best unpicked 0.0000), on the dense solver and on the sparse
///   windowed one (2 of the 3 eigenvalues per solve).
///
/// It holds because the condition holds: one `k₀` step moves the target's
/// eigenvalue by at most 0.02, and the intruder is never nearer than 0.12.
/// The block structure makes the overlaps exactly 0 or 1.
#[test]
fn synthetic_proximity_keeps_mode_where_frozen_index_hops() {
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
    let target_k = faer::c64::new(2.000025, 0.01);
    let intruder_k = faer::c64::new(1.990480, 0.042018);

    let converged = |r: SelfConsistentResult, label: &str| -> faer::c64 {
        match r {
            SelfConsistentResult::Converged { k, .. } => k,
            other => panic!("{label}: expected Converged, got {other:?}"),
        }
    };

    // Frozen index 0 on the dense solver: hops to the intruder.
    let dense = FaerComplexEigensolver;
    let recording = Recording::new(&dense);
    let k_frozen = converged(
        self_consistent_k_with(
            &recording,
            k.as_ref(),
            s.as_ref(),
            m.as_ref(),
            k0,
            ModeTarget::Index(0),
            3,
            1e-9,
            40,
        )
        .expect("frozen-index solve"),
        "frozen index",
    );
    let frozen_trace = overlap_trace(&recording.solves.borrow(), m.as_ref(), |_| 0);
    for (i, step) in frozen_trace.iter().enumerate() {
        eprintln!(
            "[synthetic frozen index] it {:2} k0 = {:.6} pick λ = {:.6}{:+.6}i overlap = {}",
            i + 1,
            step.k0,
            step.lambda.re,
            step.lambda.im,
            step.overlap.map_or("n/a".into(), |o| format!("{o:.4}")),
        );
    }
    assert!(
        (k_frozen - intruder_k).norm() < 1e-5,
        "frozen-index k = {k_frozen}"
    );
    let hops = frozen_trace
        .iter()
        .filter(|s| s.overlap.is_some_and(|o| o < 0.5))
        .count();
    assert_eq!(hops, 1, "the frozen index hops once");
    assert!(min_overlap(&frozen_trace) < 1e-9);

    // Proximity on the dense solver, then on the sparse windowed solver
    // with a window of 2 of the 3 eigenvalues: both stay on the target.
    let sparse = SparseSelfConsistentEigensolver::default();
    let solvers: [(&str, &dyn SelfConsistentEigensolver, usize); 2] =
        [("dense", &dense, 3), ("sparse", &sparse, 2)];
    for (label, solver, n_eigs) in solvers {
        let recording = Recording::new(solver);
        let k_near = converged(
            self_consistent_k_with(
                &recording,
                k.as_ref(),
                s.as_ref(),
                m.as_ref(),
                k0,
                // Just off the seed eigenvalue `4 + 0.08j`, which as a
                // shift would make the first shifted solve exactly singular.
                ModeTarget::Nearest(faer::c64::new(4.0, 0.1)),
                n_eigs,
                1e-9,
                40,
            )
            .expect("proximity solve"),
            label,
        );
        let trace = proximity_trace(
            &recording.solves.borrow(),
            m.as_ref(),
            &format!("synthetic proximity, {label}"),
        );
        assert!(
            (k_near - target_k).norm() < 1e-6,
            "{label}: proximity k = {k_near}"
        );
        assert!(trace.len() > 3, "{label}: {} solves", trace.len());
        assert!(
            min_overlap(&trace) > 1.0 - 1e-9,
            "{label}: the proximity pick changed eigenvector: min overlap {}",
            min_overlap(&trace)
        );
        // The seed pick is the target mode, `λ = 4 + 0.08j` at `k₀ = 4`.
        assert!((trace[0].lambda - faer::c64::new(4.0, 0.08)).norm() < 1e-9);
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
