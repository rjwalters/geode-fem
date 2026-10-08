//! Self-consistent `k₀` iteration for the Silver-Müller pencil
//! (issue #36).
//!
//! The first-order Silver-Müller absorbing BC matches a tangential
//! impedance to a single *guess* wavenumber `k₀`. The resulting pencil
//!
//! ```text
//! (K + j k₀ S) E = k² M E
//! ```
//!
//! is correct for the **resonant** mode only when `k₀ = Re(k_target)`.
//! Pick `k₀` too far from resonance and the impedance is mismatched,
//! injecting an artificial reflection that degrades the observed Q.
//!
//! This module wraps the existing complex eigensolver in a damped fixed
//! point on `k₀ ← Re(sqrt(λ_target))`:
//!
//! 1. Solve the pencil at the current `k₀`, sort eigenvalues by
//!    `|Re(λ)|` ascending (matches `FaerComplexEigensolver`).
//! 2. Pick the *caller-frozen* `target_idx` — the index of the
//!    physical mode the caller identified at iteration 0. We do **not**
//!    re-classify between iterations: the sort order can shuffle the
//!    spurious-mode cluster at the front of the list when `k₀` drifts,
//!    and Newton-style mode hopping ruins convergence.
//! 3. Take `k_target = sqrt(λ_target)` on the principal branch
//!    (`Re(k) > 0`); update with damping
//!
//!    ```text
//!    k₀_new = k₀_old + α · (Re(k_target) - k₀_old)
//!    ```
//!
//!    using `α = 0.5` for the first three iterations to dampen Newton
//!    overshoot when seeded far from resonance, then `α = 1.0`.
//! 4. Convergence: `|k₀_new - k₀_old| < tol`.
//! 5. Divergence guard: after the damped phase, if `|Δk₀| / k₀`
//!    *increases* between successive iterations we abort and return
//!    `SelfConsistentResult::Diverged { last_k, .. }` rather than
//!    blowing up. Typical cause is the seed landing in the basin of a
//!    neighbour mode and the un-frozen Newton trying to hop.
//!
//! # Pluggable eigensolver and target rule (issue #917)
//!
//! [`self_consistent_k`] and [`self_consistent_k_vector_tracked`] solve the
//! full dense spectrum on every iteration ([`FaerComplexEigensolver`]) and
//! take the target as an index into it. That is `O(n³)` per iteration: about
//! 21 dense solves of the 4512-DOF sphere pencil per run.
//!
//! [`self_consistent_k_with`] and [`self_consistent_k_vector_tracked_with`]
//! take the eigensolver ([`SelfConsistentEigensolver`]) and the target rule
//! ([`ModeTarget`]) as arguments. With [`SparseSelfConsistentEigensolver`]
//! and [`ModeTarget::Nearest`] each iteration is one sparse shift-invert
//! Lanczos for the few eigenvalues nearest the previous target. The damped
//! update, the convergence test and the divergence guard are shared, and
//! the dense functions are the `(FaerComplexEigensolver, ModeTarget::Index)`
//! case of the pluggable ones.
//!
//! The two target rules are not interchangeable. A frozen index re-reads
//! "the `i`-th smallest `|Re λ|`" on every solve, so it can change mode
//! when the sorted order shuffles. [`ModeTarget::Nearest`] follows one
//! eigenvalue continuously. They agree while the order around the target
//! does not change.
//!
//! # Scope: Silver-Müller only — not PML
//!
//! The PML pencil (#28) takes a damping coefficient `σ₀`, not a
//! wavenumber. Iterating `σ₀ ← Re(k)` would be dimensionally wrong:
//! `σ₀` tunes absorption strength, not the frequency entering the BC
//! kernel. PML self-consistency (if it matters for Q) is a separate
//! Q-maximization sweep over `σ₀`, not a Newton iteration.
//!
//! # Q-factor convention
//!
//! With `k = sqrt(λ)` taken on the `Re(k) > 0` branch and the radiating
//! convention `Im(k) > 0` (decay in time), we report
//!
//! ```text
//! Q = Re(k) / (2 · Im(k)).
//! ```
//!
//! `Im(k_target)` can in principle land slightly negative for badly
//! seeded or noise-dominated modes; we report `Q` based on
//! `Im(k).abs()` and propagate the actual `Complex<k>` so callers can
//! sanity-check the sign.

use faer::c64;
use faer::mat::MatRef;

use faer::sparse::{SparseColMat, Triplet};

use crate::eigen::complex::{
    ComplexEigenPair, ComplexEigenSolver, FaerComplexEigensolver, SparseComplexShiftInvertLanczos,
};
use crate::eigen::dense::EigenError;
use crate::eigen::lanczos::ConvergenceCheck;
use crate::solver::iterate::{IterOutcome, Step, iterate_while_with_prev};

/// Outcome of [`self_consistent_k`].
#[derive(Debug, Clone)]
pub enum SelfConsistentResult {
    /// Fixed point converged: `|Δk₀| < tol` and the divergence guard
    /// never tripped.
    Converged {
        /// Complex wavenumber `k = sqrt(λ_target)` on the
        /// `Re(k) > 0` branch.
        k: c64,
        /// Q-factor `Re(k) / (2 |Im(k)|)` ([`q_factor`]). `f64::INFINITY`
        /// if `|Im(k)|` is at round-off relative to `|Re(k)|`
        /// ([`Q_LOSSLESS_REL_TOL`]; effectively lossless).
        q: f64,
        /// Number of solve calls performed (≥ 1).
        iterations: usize,
    },
    /// Divergence guard tripped: `|Δk₀| / k₀` increased between two
    /// post-damped iterations. The last stable wavenumber is reported.
    Diverged {
        /// Last `k = sqrt(λ_target)` before divergence was detected.
        last_k: c64,
        /// Iteration count when the guard fired.
        iterations: usize,
    },
    /// Loop ran out of iterations without converging or diverging.
    MaxIterations {
        /// Last `k = sqrt(λ_target)`.
        last_k: c64,
        /// Iteration count (== `max_iter`).
        iterations: usize,
    },
    /// Vector-tracked mode died: the maximum overlap of the previous
    /// iteration's eigenvector against any candidate at the current
    /// iteration fell below the configured threshold. This signals the
    /// physical mode has been swamped by spurious / radiative-tail
    /// content and is distinct from divergence on `|Δk₀|`. Only
    /// returned by [`self_consistent_k_vector_tracked`].
    ModeLost {
        /// Last stable `k = sqrt(λ_target)` before the mode was lost.
        last_k: c64,
        /// Iteration count when the overlap dropped.
        iterations: usize,
        /// The best overlap seen at the failing iteration. A value
        /// well below 0.5 indicates the seed is far from any
        /// resonance; a value just below threshold may indicate a
        /// genuine close mode collision (deflation might recover it).
        best_overlap: f64,
    },
}

/// Number of solve calls at which the damping factor switches from
/// `0.5` (overshoot guard) to `1.0` (full Newton step).
const DAMPED_ITERATIONS: usize = 3;

/// Run a damped fixed-point iteration `k₀ ← Re(sqrt(λ_target))` on the
/// Silver-Müller pencil `(K + j k₀ S, M)`.
///
/// # Arguments
///
/// * `k_mat` — real curl-curl stiffness `[n_dofs, n_dofs]`.
/// * `s_mat` — real Silver-Müller surface matrix `[n_dofs, n_dofs]`.
/// * `m_mat` — real ε-scaled mass `[n_dofs, n_dofs]`.
/// * `initial_k0` — starting wavenumber for the iteration. Should be in
///   the basin of attraction of the target mode (typically the heuristic
///   PEC ground-mode `k`).
/// * `target_idx` — index into the by-`|Re(λ)|` sorted eigenvalue list
///   that the caller has identified as the physical mode of interest.
///   This index is **frozen** across iterations (no re-classification).
/// * `n_eigs` — number of lowest eigenvalues to request per solve;
///   must be `> target_idx`. Should be large enough to clear the
///   spurious-mode cluster comfortably.
/// * `tol` — convergence tolerance on `|Δk₀|` (typical `1e-6`).
/// * `max_iter` — maximum solve count before giving up (typical `20`).
///
/// # Returns
///
/// [`SelfConsistentResult`] describing the outcome. The
/// [`EigenError`] is propagated unchanged when any individual solve fails.
#[allow(clippy::too_many_arguments)]
pub fn self_consistent_k(
    k_mat: MatRef<f64>,
    s_mat: MatRef<f64>,
    m_mat: MatRef<f64>,
    initial_k0: f64,
    target_idx: usize,
    n_eigs: usize,
    tol: f64,
    max_iter: usize,
) -> Result<SelfConsistentResult, EigenError> {
    self_consistent_k_with(
        &FaerComplexEigensolver,
        k_mat,
        s_mat,
        m_mat,
        initial_k0,
        ModeTarget::Index(target_idx),
        n_eigs,
        tol,
        max_iter,
    )
}

/// How a self-consistent driver picks the target mode out of each solve
/// (issue #917).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModeTarget {
    /// The historical frozen index into the solver's list, sorted by
    /// `|Re λ|` ascending. Only meaningful for a solver that returns the
    /// whole low end of the spectrum, such as [`FaerComplexEigensolver`]:
    /// a windowed solver like [`SparseSelfConsistentEigensolver`] returns a
    /// different set of modes at every shift, so an index into it does not
    /// name a mode.
    Index(usize),
    /// Track by proximity: the first solve picks the eigenvalue nearest
    /// this seed `λ = k²`, and each later solve picks the eigenvalue nearest
    /// the previous solve's target. This needs only the few eigenvalues
    /// near the target, so it works with a windowed solver.
    Nearest(c64),
}

impl ModeTarget {
    /// Pick the target out of `lambdas`, given the previous target `prev`
    /// (`None` on the first solve). Returns `None` when the list is too
    /// short (index past the end, or empty).
    fn pick(self, lambdas: &[c64], prev: Option<c64>) -> Option<usize> {
        match self {
            ModeTarget::Index(i) => (i < lambdas.len()).then_some(i),
            ModeTarget::Nearest(seed) => {
                let anchor = prev.unwrap_or(seed);
                lambdas
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (*a.1 - anchor).norm().total_cmp(&(*b.1 - anchor).norm()))
                    .map(|(i, _)| i)
            }
        }
    }

    /// The shift hint passed to the eigensolver: the previous target (or
    /// the seed) for [`ModeTarget::Nearest`], `k₀²` for
    /// [`ModeTarget::Index`].
    fn shift_hint(self, k0: f64, prev: Option<c64>) -> c64 {
        match self {
            ModeTarget::Index(_) => c64::new(k0 * k0, 0.0),
            ModeTarget::Nearest(seed) => prev.unwrap_or(seed),
        }
    }
}

/// The eigensolver a self-consistent driver calls once per iteration
/// (issue #917): the eigenpairs of the Silver-Müller pencil
/// `(K + j k₀ S) E = λ M E` at the current `k₀`.
///
/// `sigma` is a hint for where the target is in the complex `λ` plane: the
/// previous target under [`ModeTarget::Nearest`]. A full-spectrum solver
/// ignores it. A windowed solver returns the eigenvalues nearest it.
///
/// Every [`ComplexEigenSolver`] implements this trait by ignoring `sigma`
/// and returning its lowest-`n` list by `|Re λ|`, so the dense
/// [`FaerComplexEigensolver`] (the default of [`self_consistent_k`] and
/// [`self_consistent_k_vector_tracked`]) keeps its exact behaviour.
/// [`SparseSelfConsistentEigensolver`] is the sparse windowed solver.
pub trait SelfConsistentEigensolver {
    /// Eigenvalues of `(K + j k₀ S, M)`, at most `n`.
    #[allow(clippy::too_many_arguments)]
    fn pencil_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        sigma: c64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError>;

    /// Eigenpairs `(λ, x)` of `(K + j k₀ S, M)`, at most `n`. Vector
    /// normalization is up to the solver; the vector-tracked driver
    /// re-normalizes.
    #[allow(clippy::too_many_arguments)]
    fn pencil_eigenpairs(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        sigma: c64,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError>;
}

impl<T: ComplexEigenSolver + ?Sized> SelfConsistentEigensolver for T {
    fn pencil_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        _sigma: c64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError> {
        self.smallest_complex_eigenvalues(k, s, m, k0, n)
    }

    fn pencil_eigenpairs(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        _sigma: c64,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        self.smallest_complex_pairs(k, s, m, k0, n)
    }
}

/// Sparse windowed eigensolver for the self-consistent drivers (issue
/// #917): the residual-checked complex shift-invert Lanczos
/// ([`SparseComplexShiftInvertLanczos::smallest_eigenpairs_checked`]) at the
/// shift `σ` the driver passes, returning the `n` converged pairs nearest
/// `σ`.
///
/// `σ` is complex, because a Silver-Müller target can sit far from the real
/// axis (the overdamped surface modes of the sphere fixture have
/// `λ ≈ 0.13 + 2.3j`, behind the whole gradient null cluster as seen from
/// any real shift). The Lanczos solver takes a real shift, so the imaginary
/// part is moved into the pencil: `(A − j·Im σ·M, M)` has the eigenvalues
/// `λ − j·Im σ` and the same eigenvectors, and is still complex symmetric.
/// It is solved at the real shift `Re σ` and `j·Im σ` is added back.
///
/// The dense inputs are converted to compressed sparse columns on every
/// call (`O(n²)` scan, cheap next to a dense `O(n³)` solve). Use it with
/// [`ModeTarget::Nearest`]: the returned window moves with `σ`, so a frozen
/// index into it does not name a mode.
///
/// # What the window does to each driver
///
/// - **Proximity tracking** ([`self_consistent_k_with`] with
///   [`ModeTarget::Nearest`]) is unaffected: `σ` is the previous target, so
///   the eigenvalue nearest it is always in the window. On the 4512-DOF
///   sphere pencil the run agrees with the dense solver under the same rule
///   to `1e-12` in the final `k` (issue #917).
/// - **Vector tracking** ([`self_consistent_k_vector_tracked_with`]) scores
///   only the `n` candidates in the window. If one `k₀` step moves the
///   target's eigenvalue farther than the window reaches, no candidate
///   overlaps and the driver returns
///   [`SelfConsistentResult::ModeLost`], where a wider window would have
///   kept the mode. That happens for a seed far from self-consistency
///   (sphere pencil, seed `k₀ = 25`, a mode at `λ ≈ 37.5 + 6.5j`: lost at
///   `n = 12` and `n = 48`, tracked to convergence at `n = 160`). Raise
///   `n_eigs` if a well-seeded run reports `ModeLost`.
///
/// # Holes
///
/// A **hole at the target** fails loudly: if the checked solve withholds a
/// localized (genuine but unconverged) eigenvalue nearer `σ` than every
/// pair it returns, the call returns [`EigenError::FaerGevd`]. Under
/// [`ModeTarget::Nearest`] `σ` is the previous target, so that withheld
/// eigenvalue is the one the driver would have picked, and returning the
/// window would make it pick a farther mode in its place. Unconverged
/// values farther out than the nearest returned pair are not an error: a
/// window that reaches the gradient null cluster (`λ ≈ 0`, hundreds of
/// modes) always has some, and the driver never picks past the nearest.
#[derive(Debug, Clone, Copy)]
pub struct SparseSelfConsistentEigensolver {
    /// Krylov dimension of the first Lanczos pass
    /// ([`SparseComplexShiftInvertLanczos::max_iters`]).
    pub max_iters: usize,
    /// Lanczos tolerance ([`SparseComplexShiftInvertLanczos::tol`]).
    pub tol: f64,
    /// Largest accepted relative true residual of a returned pair
    /// ([`ConvergenceCheck::residual_tol`]).
    pub residual_tol: f64,
    /// Cap on the Krylov dimension of an extended run
    /// ([`ConvergenceCheck::max_iters_cap`]).
    pub max_iters_cap: usize,
}

impl Default for SparseSelfConsistentEigensolver {
    fn default() -> Self {
        Self {
            max_iters: 64,
            tol: 1e-9,
            residual_tol: 1e-8,
            max_iters_cap: 400,
        }
    }
}

impl SparseSelfConsistentEigensolver {
    /// Converged pairs nearest `sigma`, ascending `Re λ`.
    #[allow(clippy::too_many_arguments)]
    fn solve(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        sigma: c64,
        n: usize,
    ) -> Result<Vec<ComplexEigenPair>, EigenError> {
        // A' = K + j (k₀ S − Im σ · M): the pencil with `Im σ` shifted out.
        let tau = c64::new(0.0, sigma.im);
        let a = dense_to_csc(k, &[(s, c64::new(0.0, k0)), (m, -tau)])?;
        let b = dense_to_csc(m, &[])?;
        let sigma_re = sigma.re;
        let lanczos = SparseComplexShiftInvertLanczos {
            sigma: sigma_re,
            max_iters: self.max_iters,
            tol: self.tol,
        };
        let checked = lanczos.smallest_eigenpairs_checked(
            a.as_ref(),
            b.as_ref(),
            n,
            ConvergenceCheck {
                residual_tol: self.residual_tol,
                max_iters_cap: self.max_iters_cap,
                window: None,
            },
        )?;
        // Distance from σ to the nearest returned pair (∞ if none came back).
        let reach = checked
            .pairs
            .iter()
            .map(|p| (p.lambda.re - sigma_re).hypot(p.lambda.im))
            .fold(f64::INFINITY, f64::min);
        if let Some((lambda, residual)) = checked.localized_hole(sigma_re, reach, |_| true) {
            let lambda = lambda + tau;
            return Err(EigenError::FaerGevd(format!(
                "self-consistent sparse solve at k0 = {k0}, sigma = {sigma}: the eigenvalue \
                 nearest sigma, {lambda} (relative residual {residual:.3e}), did not \
                 converge within {} Lanczos steps; refusing to return a window with a \
                 hole at the target (issue #917)",
                checked.lanczos_steps
            )));
        }
        let mut pairs = checked.pairs;
        for p in &mut pairs {
            p.lambda += tau;
        }
        Ok(pairs)
    }
}

impl SelfConsistentEigensolver for SparseSelfConsistentEigensolver {
    fn pencil_eigenvalues(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        sigma: c64,
        n: usize,
    ) -> Result<Vec<c64>, EigenError> {
        Ok(self
            .solve(k, s, m, k0, sigma, n)?
            .into_iter()
            .map(|p| p.lambda)
            .collect())
    }

    fn pencil_eigenpairs(
        &self,
        k: MatRef<f64>,
        s: MatRef<f64>,
        m: MatRef<f64>,
        k0: f64,
        sigma: c64,
        n: usize,
    ) -> Result<Vec<(c64, Vec<c64>)>, EigenError> {
        Ok(self
            .solve(k, s, m, k0, sigma, n)?
            .into_iter()
            .map(|p| (p.lambda, p.vector))
            .collect())
    }
}

/// `A + Σ scaleᵢ · Bᵢ` as a complex CSC matrix, keeping the entries where
/// any of the dense inputs is nonzero.
fn dense_to_csc(
    a: MatRef<f64>,
    terms: &[(MatRef<f64>, c64)],
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let (nr, nc) = (a.nrows(), a.ncols());
    let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
    for j in 0..nc {
        for i in 0..nr {
            let mut v = c64::new(a[(i, j)], 0.0);
            let mut structural = v.re != 0.0;
            for (bm, scale) in terms {
                let b = bm[(i, j)];
                structural |= b != 0.0;
                v += *scale * b;
            }
            if structural {
                trips.push(Triplet::new(i, j, v));
            }
        }
    }
    SparseColMat::<usize, c64>::try_new_from_triplets(nr, nc, &trips)
        .map_err(|e| EigenError::FaerGevd(format!("self-consistent CSC conversion: {e:?}")))
}

/// [`self_consistent_k`] with a pluggable eigensolver and target rule
/// (issue #917).
///
/// `solver` is called once per iteration with the shift hint of
/// [`ModeTarget`]; `target` picks the mode out of each solve. With
/// `(&FaerComplexEigensolver, ModeTarget::Index(i))` this is exactly
/// [`self_consistent_k`]. With
/// `(&SparseSelfConsistentEigensolver::default(), ModeTarget::Nearest(λ₀))`
/// each solve is a sparse Lanczos for the `n_eigs` eigenvalues nearest the
/// previous target instead of a dense full-spectrum solve.
///
/// The damping, convergence test and divergence guard are those of
/// [`self_consistent_k`].
///
/// # Panics
///
/// If `max_iter == 0`, or `target` is [`ModeTarget::Index`]`(i)` with
/// `n_eigs <= i`.
#[allow(clippy::too_many_arguments)]
pub fn self_consistent_k_with<S: SelfConsistentEigensolver + ?Sized>(
    solver: &S,
    k_mat: MatRef<f64>,
    s_mat: MatRef<f64>,
    m_mat: MatRef<f64>,
    initial_k0: f64,
    target: ModeTarget,
    n_eigs: usize,
    tol: f64,
    max_iter: usize,
) -> Result<SelfConsistentResult, EigenError> {
    if let ModeTarget::Index(target_idx) = target {
        assert!(
            n_eigs > target_idx,
            "n_eigs ({n_eigs}) must be > target_idx ({target_idx}) so the target mode is in the slice"
        );
    }
    assert!(max_iter > 0, "max_iter must be positive");

    // The carried state is the fixed-point's loop-invariant slot
    // (contract restriction 1): the current seed `k₀`, the last stable
    // `k_target` (`None` until the first successful solve), and this
    // step's relative residual `|Δk₀| / k₀` (`None` on entry, used by
    // the divergence guard via the combinator's `prev`). All scalars —
    // the shape never changes across iterations.
    //
    // We use `iterate_while_with_prev` rather than `iterate_while` so the
    // divergence guard reads the *previous* iteration's `dk_rel` from the
    // combinator's one-step history instead of threading it by hand. The
    // continue/stop decision inside the step is a single scalar predicate
    // (`abs_dk < tol`, plus the guard / fewer-eigs branches), satisfying
    // contract restriction 2; the loop yields exactly one terminal
    // `SelfConsistentResult` (restriction 3).
    let (outcome, report) = iterate_while_with_prev(
        SelfConsistentState {
            k0: initial_k0,
            last_k: None,
            last_lambda: None,
            dk_rel: None,
        },
        max_iter,
        |it, prev, state| {
            let SelfConsistentState {
                k0,
                last_k,
                last_lambda,
                ..
            } = state;

            let sigma = target.shift_hint(k0, last_lambda);
            let lambdas = match solver.pencil_eigenvalues(k_mat, s_mat, m_mat, k0, sigma, n_eigs) {
                Ok(l) => l,
                Err(e) => return Step::Done(Err(e)),
            };
            let Some(picked) = target.pick(&lambdas, last_lambda) else {
                // Solver returned fewer eigenvalues than requested (e.g.
                // mass-pencil singularities skipped). Treat as divergence
                // at the last stable point.
                return Step::Done(Ok(SelfConsistentResult::Diverged {
                    last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                    iterations: it,
                }));
            };

            let lambda = lambdas[picked];
            let k_target = principal_sqrt(lambda);
            let re_k = k_target.re;

            let dk = re_k - k0;
            let abs_dk = dk.abs();
            let k0_mag = k0.abs().max(f64::EPSILON);

            // Convergence check — measured against the **undamped** step,
            // since `|Re(k) - k₀|` is the fixed-point residual.
            if abs_dk < tol {
                let q = q_factor(k_target);
                return Step::Done(Ok(SelfConsistentResult::Converged {
                    k: k_target,
                    q,
                    iterations: it,
                }));
            }

            // Divergence guard — only active *after* the damped phase, and
            // we need at least one prior step to compare against.
            let dk_rel = abs_dk / k0_mag;
            if it > DAMPED_ITERATIONS
                && let Some(prev_dk_rel) = prev.and_then(|p| p.dk_rel)
                && dk_rel > prev_dk_rel
            {
                return Step::Done(Ok(SelfConsistentResult::Diverged {
                    last_k: last_k.unwrap_or(k_target),
                    iterations: it,
                }));
            }

            // Damped update.
            let alpha = if it <= DAMPED_ITERATIONS { 0.5 } else { 1.0 };
            Step::Continue(SelfConsistentState {
                k0: k0 + alpha * dk,
                last_k: Some(k_target),
                last_lambda: Some(lambda),
                dk_rel: Some(dk_rel),
            })
        },
    );

    match outcome {
        IterOutcome::Done(result) => result,
        IterOutcome::MaxIters(state) => Ok(SelfConsistentResult::MaxIterations {
            last_k: state.last_k.unwrap_or(c64::new(state.k0, 0.0)),
            iterations: report.iterations,
        }),
    }
}

/// Loop-invariant carried state for the self-consistent `k₀` fixed
/// point (issue #301). All slots are scalars, so the shape is constant
/// across iterations — the graph-only contract restriction 1.
#[derive(Debug, Clone, Copy)]
struct SelfConsistentState {
    /// Current seed wavenumber for the next solve.
    k0: f64,
    /// Last stable `k = sqrt(λ_target)` (`None` before the first solve).
    last_k: Option<c64>,
    /// Last target eigenvalue `λ_target` (`None` before the first solve);
    /// the anchor of [`ModeTarget::Nearest`] and its shift hint.
    last_lambda: Option<c64>,
    /// This step's relative residual `|Δk₀| / k₀` (`None` on entry).
    /// Read from the combinator's `prev` by the divergence guard.
    dk_rel: Option<f64>,
}

/// Minimum acceptable bilinear M-overlap between successive iterations'
/// target eigenvectors. Below this threshold we declare the mode lost.
const MODE_OVERLAP_THRESHOLD: f64 = 0.5;

/// Compute the matrix-vector product `M v` where `M` is real and `v`
/// is complex. Result stored in `out` (must be pre-sized to `m.nrows()`).
fn matvec_real_complex(m: MatRef<f64>, v: &[c64], out: &mut [c64]) {
    let n = m.nrows();
    debug_assert_eq!(v.len(), n);
    debug_assert_eq!(out.len(), n);
    debug_assert_eq!(m.ncols(), n);

    for x in out.iter_mut() {
        x.re = 0.0;
        x.im = 0.0;
    }
    // Column-walk for cache-friendliness: out[i] += M[i,j] * v[j].
    for j in 0..n {
        let vj = v[j];
        if vj.re == 0.0 && vj.im == 0.0 {
            continue;
        }
        for i in 0..n {
            let mij = m[(i, j)];
            if mij == 0.0 {
                continue;
            }
            out[i].re += mij * vj.re;
            out[i].im += mij * vj.im;
        }
    }
}

/// Bilinear complex dot product `u^T v = sum u[i] * v[i]` (no
/// conjugation). For the complex-symmetric Mie / Silver-Müller pencil
/// the natural M-inner-product is `u^T M v` — this helper applies the
/// final dot once `M v` is precomputed.
fn complex_dot(u: &[c64], v: &[c64]) -> c64 {
    debug_assert_eq!(u.len(), v.len());
    let mut acc = c64::new(0.0, 0.0);
    for i in 0..u.len() {
        acc += u[i] * v[i];
    }
    acc
}

/// Normalize `v` so that the bilinear M-norm `sqrt(Re(v^T M v))` is
/// one. Returns `false` if `v^T M v` is M-bilinear-isotropic (norm
/// numerically zero), in which case `v` is left unchanged and the
/// caller should treat this as a mode-tracking failure.
///
/// Why the real-part trick: for a complex-symmetric pencil the
/// bilinear self-product `v^T M v` is complex in general. Taking the
/// real part and discarding sign gives a positive scalar consistent
/// with `|v|_M` when M is close to a real SPD; this is the same
/// convention as the sparse Lanczos path (PR #55).
fn normalize_m_bilinear(v: &mut [c64], m: MatRef<f64>) -> bool {
    let n = v.len();
    let mut mv = vec![c64::new(0.0, 0.0); n];
    matvec_real_complex(m, v, &mut mv);
    let vtmv = complex_dot(v, &mv);
    let nrm2 = vtmv.re;
    if nrm2.abs() < 1e-30 {
        return false;
    }
    let nrm = nrm2.abs().sqrt();
    let scale = if nrm2 >= 0.0 { 1.0 / nrm } else { -1.0 / nrm };
    for x in v.iter_mut() {
        x.re *= scale;
        x.im *= scale;
    }
    true
}

/// Run a vector-tracked self-consistent `k₀` iteration on the
/// Silver-Müller pencil `(K + j k₀ S, M)`.
///
/// Unlike [`self_consistent_k`] — which pins a frozen integer index
/// into the by-`|Re(λ)|` sorted spectrum — this driver tracks the
/// **eigenvector** of the target mode. At iteration `i+1` the picked
/// mode is the one with maximum bilinear M-overlap against iteration
/// `i`'s target eigenvector, normalized in the bilinear M-norm.
///
/// This is the diagnosed unblocker for the Whitney-spurious-cluster
/// re-shuffling problem: when ~177 spurious modes near `k = 0`
/// reorder under `k₀` drift, integer-index pinning loses the physical
/// target mid-iteration. Vector tracking is metric-aware and follows
/// the physical mode through the spurious noise.
///
/// # Arguments
///
/// * `k_mat`, `s_mat`, `m_mat` — see [`self_consistent_k`].
/// * `initial_k0` — seed wavenumber.
/// * `initial_target_idx` — the integer index of the physical mode in
///   the **initial** solve at `initial_k0`. After iteration 0 the
///   integer index is discarded and tracking proceeds by eigenvector
///   overlap.
/// * `n_eigs` — number of lowest-`|Re(λ)|` modes per solve. Must be
///   `> initial_target_idx`. Should also be ample enough that the
///   target mode stays within the returned window as `k₀` drifts.
/// * `tol` — convergence tolerance on `|Δk₀|`.
/// * `max_iter` — maximum solve count.
///
/// # Returns
///
/// [`SelfConsistentResult`] including the new `ModeLost` variant if
/// the maximum overlap falls below `MODE_OVERLAP_THRESHOLD` at any
/// iteration past the seed.
#[allow(clippy::too_many_arguments)]
pub fn self_consistent_k_vector_tracked(
    k_mat: MatRef<f64>,
    s_mat: MatRef<f64>,
    m_mat: MatRef<f64>,
    initial_k0: f64,
    initial_target_idx: usize,
    n_eigs: usize,
    tol: f64,
    max_iter: usize,
) -> Result<SelfConsistentResult, EigenError> {
    self_consistent_k_vector_tracked_with(
        &FaerComplexEigensolver,
        k_mat,
        s_mat,
        m_mat,
        initial_k0,
        ModeTarget::Index(initial_target_idx),
        n_eigs,
        tol,
        max_iter,
    )
}

/// [`self_consistent_k_vector_tracked`] with a pluggable eigensolver and
/// seed rule (issue #917).
///
/// `initial` picks the target out of the **first** solve only
/// ([`ModeTarget::Index`] or [`ModeTarget::Nearest`]); every later solve
/// picks the candidate of maximum M-overlap with the previous target, as
/// in [`self_consistent_k_vector_tracked`]. The solver's shift hint is
/// [`ModeTarget`]'s on the first solve and the previous target `λ`
/// afterwards. With `(&FaerComplexEigensolver, ModeTarget::Index(i))` this
/// is exactly [`self_consistent_k_vector_tracked`].
///
/// # Panics
///
/// If `max_iter == 0`, or `initial` is [`ModeTarget::Index`]`(i)` with
/// `n_eigs <= i`.
#[allow(clippy::too_many_arguments)]
pub fn self_consistent_k_vector_tracked_with<S: SelfConsistentEigensolver + ?Sized>(
    solver: &S,
    k_mat: MatRef<f64>,
    s_mat: MatRef<f64>,
    m_mat: MatRef<f64>,
    initial_k0: f64,
    initial: ModeTarget,
    n_eigs: usize,
    tol: f64,
    max_iter: usize,
) -> Result<SelfConsistentResult, EigenError> {
    if let ModeTarget::Index(initial_target_idx) = initial {
        assert!(
            n_eigs > initial_target_idx,
            "n_eigs ({n_eigs}) must be > initial_target_idx ({initial_target_idx})"
        );
    }
    assert!(max_iter > 0, "max_iter must be positive");

    let mut k0 = initial_k0;
    let mut prev_v: Option<Vec<c64>> = None;
    let mut last_k: Option<c64> = None;
    let mut last_lambda: Option<c64> = None;
    let mut prev_dk_rel: Option<f64> = None;

    for it in 1..=max_iter {
        let sigma = last_lambda.unwrap_or_else(|| initial.shift_hint(k0, None));
        let pairs = solver.pencil_eigenpairs(k_mat, s_mat, m_mat, k0, sigma, n_eigs)?;
        if pairs.is_empty() {
            return Ok(SelfConsistentResult::Diverged {
                last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                iterations: it,
            });
        }

        // Pick target: seed iteration uses `initial` (integer index or
        // nearest eigenvalue); subsequent iterations use max-overlap.
        let (lambda, mut v_sel, best_overlap) = match &prev_v {
            None => {
                let lambdas: Vec<c64> = pairs.iter().map(|p| p.0).collect();
                let Some(picked) = initial.pick(&lambdas, None) else {
                    return Ok(SelfConsistentResult::Diverged {
                        last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                        iterations: it,
                    });
                };
                let (lam, v) = pairs[picked].clone();
                (lam, v, 1.0)
            }
            Some(prev) => {
                // Compute |⟨prev, v_j⟩_M| for each candidate. We
                // do not re-normalize candidates first (the dense
                // solver's eigenvectors come out in an arbitrary
                // normalization); instead we score by the
                // **normalized** overlap
                //   |⟨prev, v_j⟩_M| / sqrt(|⟨v_j, v_j⟩_M|)
                // which is invariant to the candidate's M-norm.
                // `prev` is already M-normalized so its denominator
                // factor is 1.
                //
                // Cost: we precompute `M v_j` once per candidate
                // (O(n^2)) and reuse it for both the cross-overlap
                // (`prev^T (M v_j)`) and the self-overlap
                // (`v_j^T (M v_j)`). This brings the per-iter
                // overlap cost from 2·n_eigs·n^2 down to n_eigs·n^2.
                let n = prev.len();
                let mut best_idx = 0usize;
                let mut best_score = -1.0_f64;
                let mut mv = vec![c64::new(0.0, 0.0); n];
                for (j, (_lam_j, v_j)) in pairs.iter().enumerate() {
                    matvec_real_complex(m_mat, v_j, &mut mv);
                    let overlap = complex_dot(prev, &mv);
                    let self_overlap = complex_dot(v_j, &mv);
                    let mag = overlap.re.hypot(overlap.im);
                    let self_norm = self_overlap.re.abs().sqrt().max(1e-30);
                    let score = mag / self_norm;
                    if score > best_score {
                        best_score = score;
                        best_idx = j;
                    }
                }

                if best_score < MODE_OVERLAP_THRESHOLD {
                    return Ok(SelfConsistentResult::ModeLost {
                        last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                        iterations: it,
                        best_overlap: best_score.max(0.0),
                    });
                }

                let (lam, v) = pairs[best_idx].clone();
                (lam, v, best_score)
            }
        };
        let _ = best_overlap; // tracked for diagnostics, surfaced via ModeLost

        // M-normalize the selected eigenvector before storing as
        // prev_v. If it's bilinear-isotropic, treat as mode death.
        if !normalize_m_bilinear(&mut v_sel, m_mat) {
            return Ok(SelfConsistentResult::ModeLost {
                last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                iterations: it,
                best_overlap: 0.0,
            });
        }

        let k_target = principal_sqrt(lambda);
        let re_k = k_target.re;
        let dk = re_k - k0;
        let abs_dk = dk.abs();
        let k0_mag = k0.abs().max(f64::EPSILON);

        if abs_dk < tol {
            let q = q_factor(k_target);
            return Ok(SelfConsistentResult::Converged {
                k: k_target,
                q,
                iterations: it,
            });
        }

        let dk_rel = abs_dk / k0_mag;
        if it > DAMPED_ITERATIONS
            && let Some(prev) = prev_dk_rel
            && dk_rel > prev
        {
            return Ok(SelfConsistentResult::Diverged {
                last_k: last_k.unwrap_or(k_target),
                iterations: it,
            });
        }
        prev_dk_rel = Some(dk_rel);

        let alpha = if it <= DAMPED_ITERATIONS { 0.5 } else { 1.0 };
        k0 += alpha * dk;
        last_k = Some(k_target);
        last_lambda = Some(lambda);
        prev_v = Some(v_sel);
    }

    Ok(SelfConsistentResult::MaxIterations {
        last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
        iterations: max_iter,
    })
}

/// Principal square root `Re √z ≥ 0` (for `Im z > 0`, a radiating mode
/// under our convention, `Im √z > 0` as required for outgoing waves).
/// Delegates to the shared cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`] (issue #830).
fn principal_sqrt(z: c64) -> c64 {
    crate::eigen::wavenumber::principal_sqrt(z)
}

/// Relative round-off band of [`q_factor`]: a wavenumber with
/// `|Im k| ≤ Q_LOSSLESS_REL_TOL · |Re k|` is numerically lossless and
/// gets `Q = ∞`.
///
/// The test is **relative** (issue #826). `k` is in rad per mesh length
/// unit, so an absolute cutoff (the old `|Im k| < 1e-12`) meant different
/// things on different meshes: on a μm mesh at 5 GHz (`Re k ≈ 1e-4`) it
/// reported every mode with `Q ≳ 5e7` as infinite, while the same mesh in
/// metres stayed finite up to `Q ≈ 5e13`. Scaling the mesh by `s` scales
/// `Re k` and `Im k` by `1/s` together, so this test (and `Q` itself) is
/// unit-invariant.
///
/// `16 ε ≈ 3.6e-15` is a few ulps of `Re k`: an `Im k` that small is
/// below what the `f64` eigenvalue can resolve. The largest finite `Q` it
/// allows is `1/(2·16ε) ≈ 1.4e14`.
///
/// Precision: `Q` is only as good as `Im k`. Every `λ → k` conversion
/// (here, `lossy_cavity::principal_k0` and
/// `geode_util::eigen::k_from_lambda`) uses the cancellation-free
/// [`crate::eigen::wavenumber::principal_sqrt`], so `Im k` is accurate to
/// a few ulps of itself for any `λ` and the whole finite range up to
/// `≈ 1.4e14` is usable (issue #830; the old `√(½(|λ| − Re λ))` form lost
/// `≈ log₁₀(2Q²)` digits and returned `Im k = 0` from `Q ≈ 6.7e7`). What
/// remains is the accuracy of `Im λ` itself: a finite `Q` near the
/// ceiling may be eigensolver round-off on an essentially lossless mode,
/// which is honest (`Q` that large is lossless for every practical
/// purpose) but is not `∞`.
pub const Q_LOSSLESS_REL_TOL: f64 = 16.0 * f64::EPSILON;

/// Quality factor of a complex wavenumber: `Q = Re(k) / (2 |Im(k)|)`,
/// or `f64::INFINITY` when `|Im(k)| ≤ Q_LOSSLESS_REL_TOL · |Re(k)|`
/// (numerically lossless; see [`Q_LOSSLESS_REL_TOL`] for why the test is
/// relative). `k = 0` counts as lossless.
///
/// `Q` uses `|Im k|`, so a growing mode (`Im k < 0` under `exp(+jωt)`)
/// also gets a positive `Q`; check the sign of `Im k` separately if that
/// matters.
pub fn q_factor(k: c64) -> f64 {
    if k.im.abs() <= Q_LOSSLESS_REL_TOL * k.re.abs() {
        f64::INFINITY
    } else {
        k.re / (2.0 * k.im.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_sqrt_real_positive() {
        let s = principal_sqrt(c64::new(4.0, 0.0));
        assert!((s.re - 2.0).abs() < 1e-12);
        assert!(s.im.abs() < 1e-12);
    }

    #[test]
    fn principal_sqrt_upper_half_plane() {
        // λ = 1 + 0.1j → k should have Re > 0, Im > 0 (radiating).
        let s = principal_sqrt(c64::new(1.0, 0.1));
        assert!(s.re > 0.0, "Re(sqrt(λ)) must be > 0");
        assert!(s.im > 0.0, "Im(sqrt(λ)) must follow sign(Im(λ))");
        // Numerical: sqrt(1 + 0.1i) ≈ 1.00125 + 0.04994i
        assert!((s.re * s.re - s.im * s.im - 1.0).abs() < 1e-10);
        assert!((2.0 * s.re * s.im - 0.1).abs() < 1e-10);
    }

    #[test]
    fn q_factor_lossless_is_infinite() {
        let q = q_factor(c64::new(1.0, 0.0));
        assert_eq!(q, f64::INFINITY);
    }

    /// Issue #826: the same physical mode on meshes authored in different
    /// length units (`k` scales as `1/s`) gets the same `Q`, and the same
    /// lossless classification. The old absolute `|Im k| < 1e-12` cutoff
    /// reported the `Q = 1e9` mode as infinite on the μm-scale `k`.
    #[test]
    fn q_factor_is_mesh_unit_invariant() {
        // Re k at 5 GHz is ≈ 104.8 rad/m.
        let re_k_m = 104.8;
        for q_true in [10.0, 1e3, 1e9, 1e13] {
            let k_m = c64::new(re_k_m, re_k_m / (2.0 * q_true));
            for s in [1.0, 1e-3, 1e-6, 1e6] {
                // A mesh unit of `s` metres: k in rad/unit is `k_m · s`.
                let k = k_m * s;
                let q = q_factor(k);
                assert!(
                    q.is_finite() && ((q - q_true) / q_true).abs() < 1e-12,
                    "unit {s} m, Q_true {q_true}: got {q}"
                );
            }
        }
        // Lossless (Im k exactly 0 or at a few ulps) is ∞ in every unit,
        // and so is k = 0.
        for s in [1.0, 1e-6, 1e6] {
            assert_eq!(q_factor(c64::new(re_k_m * s, 0.0)), f64::INFINITY);
            let ulp_im = 4.0 * f64::EPSILON * re_k_m * s;
            assert_eq!(q_factor(c64::new(re_k_m * s, ulp_im)), f64::INFINITY);
        }
        assert_eq!(q_factor(c64::new(0.0, 0.0)), f64::INFINITY);
    }

    /// Issue #826, end to end: the self-consistent driver on one pencil
    /// written in three length units. With `k` scaled by `c` (`K` by `c²`,
    /// `S` by `c`, `k₀` and `tol` by `c`), the converged `k` scales by `c`
    /// and `Q` is unchanged. At `c = 1e-10`, `Im k ≈ 5e-13`, which the old
    /// absolute cutoff reported as `Q = ∞`.
    #[test]
    fn self_consistent_q_is_mesh_unit_invariant() {
        let run =
            |c: f64| {
                let k = faer::Mat::<f64>::from_fn(2, 2, |i, j| {
                    if i == j { [1.0, 4.0][i] * c * c } else { 0.0 }
                });
                let s = faer::Mat::<f64>::from_fn(2, 2, |i, j| if i == j { 0.01 * c } else { 0.0 });
                let m = faer::Mat::<f64>::from_fn(2, 2, |i, j| if i == j { 1.0 } else { 0.0 });
                match self_consistent_k(
                    k.as_ref(),
                    s.as_ref(),
                    m.as_ref(),
                    0.9 * c,
                    0,
                    2,
                    1e-10 * c,
                    50,
                )
                .expect("diagonal lossy pencil")
                {
                    SelfConsistentResult::Converged { k, q, iterations } => (k / c, q, iterations),
                    other => panic!("c = {c}: expected Converged, got {other:?}"),
                }
            };
        let (k_ref, q_ref, it_ref) = run(1.0);
        assert!(q_ref.is_finite() && q_ref > 10.0, "reference Q = {q_ref}");
        for c in [1e-10, 1e-6, 1e6] {
            let (k, q, it) = run(c);
            assert!(
                (k - k_ref).norm() <= 1e-9 * k_ref.norm(),
                "c = {c}: k/c = {k}, want {k_ref}"
            );
            assert!(q.is_finite(), "c = {c}: Q reported infinite");
            assert!(
                ((q - q_ref) / q_ref).abs() < 1e-6,
                "c = {c}: Q = {q}, want {q_ref}"
            );
            assert_eq!(it, it_ref, "c = {c}: iteration count");
        }
    }

    #[test]
    fn q_factor_standard_formula() {
        let q = q_factor(c64::new(2.0, 0.1));
        // Q = 2.0 / (2 * 0.1) = 10.
        assert!((q - 10.0).abs() < 1e-12);
    }

    #[test]
    fn matvec_real_complex_identity_returns_input() {
        // M = I, so M·v = v.
        let m = faer::Mat::<f64>::from_fn(3, 3, |i, j| if i == j { 1.0 } else { 0.0 });
        let v = vec![c64::new(1.0, 2.0), c64::new(3.0, -1.0), c64::new(0.5, 0.5)];
        let mut out = vec![c64::new(0.0, 0.0); 3];
        matvec_real_complex(m.as_ref(), &v, &mut out);
        for (got, want) in out.iter().zip(v.iter()) {
            assert!(((got.re - want.re).abs() + (got.im - want.im).abs()) < 1e-12);
        }
    }

    #[test]
    fn matvec_real_complex_diagonal_scales() {
        // M = diag(2, 3), v = (1+i, 1-i). M v = (2+2i, 3-3i).
        let m = faer::Mat::<f64>::from_fn(2, 2, |i, j| {
            if i == 0 && j == 0 {
                2.0
            } else if i == 1 && j == 1 {
                3.0
            } else {
                0.0
            }
        });
        let v = vec![c64::new(1.0, 1.0), c64::new(1.0, -1.0)];
        let mut out = vec![c64::new(0.0, 0.0); 2];
        matvec_real_complex(m.as_ref(), &v, &mut out);
        assert!((out[0].re - 2.0).abs() < 1e-12 && (out[0].im - 2.0).abs() < 1e-12);
        assert!((out[1].re - 3.0).abs() < 1e-12 && (out[1].im - (-3.0)).abs() < 1e-12);
    }

    #[test]
    fn complex_dot_is_bilinear_no_conjugation() {
        // u = (1+i, 2), v = (3, 1-i). u^T v = (1+i)*3 + 2*(1-i)
        //                            = 3+3i + 2-2i = 5 + i.
        let u = vec![c64::new(1.0, 1.0), c64::new(2.0, 0.0)];
        let v = vec![c64::new(3.0, 0.0), c64::new(1.0, -1.0)];
        let d = complex_dot(&u, &v);
        assert!((d.re - 5.0).abs() < 1e-12);
        assert!((d.im - 1.0).abs() < 1e-12);
        // Sanity: Hermitian u^H v would be (1-i)*3 + 2*(1-i) = 3-3i + 2-2i = 5 - 5i,
        // which is different — confirms we're bilinear, not sesquilinear.
    }

    #[test]
    fn normalize_m_bilinear_rescales_to_unit_norm() {
        // M = I (real), v = (2, 0, 0). Bilinear v^T M v = 4. After
        // normalization, v should be (1, 0, 0) so v^T M v = 1.
        let m = faer::Mat::<f64>::from_fn(3, 3, |i, j| if i == j { 1.0 } else { 0.0 });
        let mut v = vec![c64::new(2.0, 0.0), c64::new(0.0, 0.0), c64::new(0.0, 0.0)];
        let ok = normalize_m_bilinear(&mut v, m.as_ref());
        assert!(ok);
        assert!((v[0].re - 1.0).abs() < 1e-12);

        // Check post-norm v^T M v ≈ 1.
        let mut mv = vec![c64::new(0.0, 0.0); 3];
        matvec_real_complex(m.as_ref(), &v, &mut mv);
        let n2 = complex_dot(&v, &mv);
        assert!((n2.re - 1.0).abs() < 1e-12);
    }

    #[test]
    fn smallest_complex_pairs_returns_unit_eigvec_pair() {
        // Tiny pencil: K = diag(1, 4), S = 0, M = I. Eigenvalues of
        // (K, M) are {1, 4}. Eigenvectors are e_0 = (1, 0) and
        // e_1 = (0, 1) up to scale. Verify the pair API returns them
        // in ascending |Re(λ)| order.
        let k = faer::Mat::<f64>::from_fn(2, 2, |i, j| {
            if i == 0 && j == 0 {
                1.0
            } else if i == 1 && j == 1 {
                4.0
            } else {
                0.0
            }
        });
        let s = faer::Mat::<f64>::from_fn(2, 2, |_, _| 0.0);
        let m = faer::Mat::<f64>::from_fn(2, 2, |i, j| if i == j { 1.0 } else { 0.0 });
        let solver = FaerComplexEigensolver;
        let pairs = solver
            .smallest_complex_pairs(k.as_ref(), s.as_ref(), m.as_ref(), 0.0, 2)
            .expect("pairs solve");
        assert_eq!(pairs.len(), 2);

        // λ_0 ≈ 1, λ_1 ≈ 4 (by |Re| ascending).
        assert!((pairs[0].0.re - 1.0).abs() < 1e-10);
        assert!((pairs[1].0.re - 4.0).abs() < 1e-10);

        // Eigenvector at λ=1 should be along e_0 (faer normalization
        // is arbitrary; check it's a unit-axis vector).
        let v0 = &pairs[0].1;
        assert_eq!(v0.len(), 2);
        let v0_axis0 = v0[0].re.hypot(v0[0].im);
        let v0_axis1 = v0[1].re.hypot(v0[1].im);
        assert!(
            v0_axis0 > v0_axis1 * 100.0,
            "v(λ=1) should be axis-aligned with e_0: got |v[0]|={v0_axis0} vs |v[1]|={v0_axis1}"
        );

        let v1 = &pairs[1].1;
        let v1_axis0 = v1[0].re.hypot(v1[0].im);
        let v1_axis1 = v1[1].re.hypot(v1[1].im);
        assert!(
            v1_axis1 > v1_axis0 * 100.0,
            "v(λ=4) should be axis-aligned with e_1: got |v[0]|={v1_axis0} vs |v[1]|={v1_axis1}"
        );
    }

    #[test]
    fn vector_tracked_synthetic_picks_overlap_target() {
        // Build a 3-d pencil with K = diag(1, 2, 3), M = I. The
        // eigenvalues are {1, 2, 3} with axis-aligned eigenvectors.
        // We seed `k₀ = sqrt(2) ≈ 1.414` and tell the driver
        // `initial_target_idx = 1` (the middle mode). For a
        // diagonal-by-construction problem the spectrum doesn't
        // reorder under k₀ drift (S = 0), so the tracked variant
        // converges trivially. Asserts the driver completes a
        // Converged result.
        let k = faer::Mat::<f64>::from_fn(3, 3, |i, j| if i == j { (i + 1) as f64 } else { 0.0 });
        let s = faer::Mat::<f64>::from_fn(3, 3, |_, _| 0.0);
        let m = faer::Mat::<f64>::from_fn(3, 3, |i, j| if i == j { 1.0 } else { 0.0 });

        let r = self_consistent_k_vector_tracked(
            k.as_ref(),
            s.as_ref(),
            m.as_ref(),
            std::f64::consts::SQRT_2,
            1,
            3,
            1e-6,
            10,
        )
        .expect("synthetic vector-tracked solve");

        match r {
            SelfConsistentResult::Converged { k, q, iterations } => {
                // λ = 2, so Re(k) = sqrt(2). Q is infinity for a real
                // pencil (Im(k) = 0).
                assert!(
                    (k.re - std::f64::consts::SQRT_2).abs() < 1e-6,
                    "converged k.re = {} (want sqrt(2))",
                    k.re
                );
                assert!(q.is_infinite(), "lossless pencil should yield Q = inf");
                assert!((1..=10).contains(&iterations));
            }
            other => panic!("expected Converged from a trivially diagonal pencil, got {other:?}"),
        }
    }

    #[test]
    fn self_consistent_k_bit_identical_regression_diagonal_pencil() {
        // Issue #301: regression-lock the self-consistent k₀ Newton
        // driver after refactoring it onto `iterate_while_with_prev`.
        //
        // A diagonal pencil K = diag(1, 4), M = I, S = 0 has exact
        // eigenvalues {1, 4}; faer returns λ₀ = 1 + 0i bit-exactly, so
        // `k_target = sqrt(1) = 1.0` on every solve and the damped
        // fixed point is fully deterministic:
        //   k₀: 0.5 →(α=.5) .75 →(α=.5) .875 →(α=.5) .9375 →(α=1) 1.0
        //   →(Δ=0 < tol) Converged at k = 1.0 on iteration 5.
        // Any drift in the iteration arithmetic (damping schedule,
        // residual test, divergence-guard threading via `with_prev`)
        // would move `k.re` off 1.0 or change the iteration count, so
        // this asserts both to full precision.
        let k = faer::Mat::<f64>::from_fn(2, 2, |i, j| {
            if i == 0 && j == 0 {
                1.0
            } else if i == 1 && j == 1 {
                4.0
            } else {
                0.0
            }
        });
        let s = faer::Mat::<f64>::from_fn(2, 2, |_, _| 0.0);
        let m = faer::Mat::<f64>::from_fn(2, 2, |i, j| if i == j { 1.0 } else { 0.0 });

        let r = self_consistent_k(k.as_ref(), s.as_ref(), m.as_ref(), 0.5, 0, 2, 1e-6, 20)
            .expect("diagonal-pencil self-consistent solve");

        match r {
            SelfConsistentResult::Converged { k, q, iterations } => {
                assert_eq!(k.re, 1.0, "converged k.re must be bit-exact 1.0");
                assert_eq!(k.im, 0.0, "converged k.im must be bit-exact 0.0");
                assert!(q.is_infinite(), "lossless real pencil → Q = inf");
                assert_eq!(iterations, 5, "deterministic damped path → 5 solves");
            }
            other => panic!("expected Converged from diagonal pencil, got {other:?}"),
        }
    }

    /// A 1-D "open resonator" pencil for the pluggable-solver tests (issue
    /// #917): `K` the Dirichlet-Neumann second-difference matrix, `M = h·I`
    /// lumped, `S` a unit impedance on the last node. Its spectrum is simple
    /// and complex, and it moves with `k₀`.
    fn open_chain(n: usize) -> (faer::Mat<f64>, faer::Mat<f64>, faer::Mat<f64>) {
        let h = 1.0 / n as f64;
        let k = faer::Mat::<f64>::from_fn(n, n, |i, j| {
            if i == j {
                if i == n - 1 { 1.0 / h } else { 2.0 / h }
            } else if i.abs_diff(j) == 1 {
                -1.0 / h
            } else {
                0.0
            }
        });
        let s =
            faer::Mat::<f64>::from_fn(n, n, |i, j| if i == j && i == n - 1 { 1.0 } else { 0.0 });
        let m = faer::Mat::<f64>::from_fn(n, n, |i, j| if i == j { h } else { 0.0 });
        (k, s, m)
    }

    fn converged(r: SelfConsistentResult) -> (c64, f64, usize) {
        match r {
            SelfConsistentResult::Converged { k, q, iterations } => (k, q, iterations),
            other => panic!("expected Converged, got {other:?}"),
        }
    }

    #[test]
    fn mode_target_pick_and_shift_hint() {
        let l = [c64::new(0.0, 0.0), c64::new(1.0, 0.1), c64::new(4.0, 0.2)];
        assert_eq!(ModeTarget::Index(2).pick(&l, None), Some(2));
        assert_eq!(ModeTarget::Index(3).pick(&l, None), None);
        // The previous target is ignored by a frozen index.
        assert_eq!(ModeTarget::Index(0).pick(&l, Some(l[2])), Some(0));
        let near = ModeTarget::Nearest(c64::new(1.2, 0.0));
        assert_eq!(near.pick(&l, None), Some(1));
        // Once a previous target exists it is the anchor, not the seed.
        assert_eq!(near.pick(&l, Some(c64::new(3.5, 0.0))), Some(2));
        assert_eq!(near.pick(&[], None), None);
        assert_eq!(
            ModeTarget::Index(1).shift_hint(3.0, Some(l[2])),
            c64::new(9.0, 0.0)
        );
        assert_eq!(near.shift_hint(3.0, None), c64::new(1.2, 0.0));
        assert_eq!(near.shift_hint(3.0, Some(l[2])), l[2]);
    }

    /// Issue #917: on the same pencil, the sparse windowed solver with
    /// proximity tracking walks the same fixed point as the dense solver
    /// with a frozen index, for both drivers and for two different modes.
    #[test]
    fn sparse_nearest_matches_dense_frozen_index() {
        let (k, s, m) = open_chain(60);
        let n = k.nrows();
        let sparse = SparseSelfConsistentEigensolver::default();
        for (idx, k0) in [(1usize, 4.0_f64), (3, 10.0)] {
            // The dense list at the seed names the target for both runs.
            let seed_lambda = FaerComplexEigensolver
                .smallest_complex_eigenvalues(k.as_ref(), s.as_ref(), m.as_ref(), k0, n)
                .expect("dense seed solve")[idx];
            let (kd, qd, itd) = converged(
                self_consistent_k(k.as_ref(), s.as_ref(), m.as_ref(), k0, idx, n, 1e-9, 40)
                    .expect("dense frozen-index run"),
            );
            let (ks, qs, its) = converged(
                self_consistent_k_with(
                    &sparse,
                    k.as_ref(),
                    s.as_ref(),
                    m.as_ref(),
                    k0,
                    ModeTarget::Nearest(seed_lambda),
                    4,
                    1e-9,
                    40,
                )
                .expect("sparse nearest run"),
            );
            assert!(qd.is_finite() && qd > 0.0, "mode {idx}: dense Q = {qd}");
            assert!(
                (ks - kd).norm() <= 1e-8 * kd.norm(),
                "mode {idx}: sparse k = {ks}, dense k = {kd}"
            );
            assert!(((qs - qd) / qd).abs() < 1e-6, "mode {idx}: Q {qs} vs {qd}");
            assert_eq!(its, itd, "mode {idx}: iteration count");

            let (kvd, _, itvd) = converged(
                self_consistent_k_vector_tracked(
                    k.as_ref(),
                    s.as_ref(),
                    m.as_ref(),
                    k0,
                    idx,
                    n,
                    1e-9,
                    40,
                )
                .expect("dense vector-tracked run"),
            );
            let (kvs, _, itvs) = converged(
                self_consistent_k_vector_tracked_with(
                    &sparse,
                    k.as_ref(),
                    s.as_ref(),
                    m.as_ref(),
                    k0,
                    ModeTarget::Nearest(seed_lambda),
                    4,
                    1e-9,
                    40,
                )
                .expect("sparse vector-tracked run"),
            );
            assert!(
                (kvs - kvd).norm() <= 1e-8 * kvd.norm(),
                "mode {idx}: sparse tracked k = {kvs}, dense tracked k = {kvd}"
            );
            assert_eq!(itvs, itvd, "mode {idx}: tracked iteration count");
            // Both drivers follow the same mode here.
            assert!((kvd - kd).norm() <= 1e-8 * kd.norm());
        }
    }

    /// Issue #917: `self_consistent_k_with` with the dense solver and a
    /// frozen index is `self_consistent_k`, bit for bit.
    #[test]
    fn with_dense_index_is_the_default_driver() {
        let (k, s, m) = open_chain(24);
        let n = k.nrows();
        let a = self_consistent_k(k.as_ref(), s.as_ref(), m.as_ref(), 4.0, 1, n, 1e-9, 40).unwrap();
        let b = self_consistent_k_with(
            &FaerComplexEigensolver,
            k.as_ref(),
            s.as_ref(),
            m.as_ref(),
            4.0,
            ModeTarget::Index(1),
            n,
            1e-9,
            40,
        )
        .unwrap();
        let ((ka, qa, ia), (kb, qb, ib)) = (converged(a), converged(b));
        assert_eq!((ka.re, ka.im, qa, ia), (kb.re, kb.im, qb, ib));
    }

    #[test]
    fn normalize_m_bilinear_rejects_isotropic_vector() {
        // M = diag(1, -1), v = (1, 1). Bilinear v^T M v = 1 - 1 = 0.
        // Should be flagged as isotropic.
        let m = faer::Mat::<f64>::from_fn(2, 2, |i, j| {
            if i == 0 && j == 0 {
                1.0
            } else if i == 1 && j == 1 {
                -1.0
            } else {
                0.0
            }
        });
        let mut v = vec![c64::new(1.0, 0.0), c64::new(1.0, 0.0)];
        let ok = normalize_m_bilinear(&mut v, m.as_ref());
        assert!(!ok, "isotropic v must return false from normalize");
    }
}
