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

use crate::complex_eigen::{ComplexEigenSolver, FaerComplexEigensolver};
use crate::eigen::EigenError;
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
        /// Q-factor `Re(k) / (2 |Im(k)|)`. `f64::INFINITY` if `Im(k)`
        /// is below `1e-12` (effectively lossless under the discretization).
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
    assert!(
        n_eigs > target_idx,
        "n_eigs ({n_eigs}) must be > target_idx ({target_idx}) so the target mode is in the slice"
    );
    assert!(max_iter > 0, "max_iter must be positive");

    let solver = FaerComplexEigensolver;

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
            dk_rel: None,
        },
        max_iter,
        |it, prev, state| {
            let SelfConsistentState { k0, last_k, .. } = state;

            let lambdas = match solver.smallest_complex_eigenvalues(k_mat, s_mat, m_mat, k0, n_eigs)
            {
                Ok(l) => l,
                Err(e) => return Step::Done(Err(e)),
            };
            if lambdas.len() <= target_idx {
                // Solver returned fewer eigenvalues than requested (e.g.
                // mass-pencil singularities skipped). Treat as divergence
                // at the last stable point.
                return Step::Done(Ok(SelfConsistentResult::Diverged {
                    last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                    iterations: it,
                }));
            }

            let lambda = lambdas[target_idx];
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
    assert!(
        n_eigs > initial_target_idx,
        "n_eigs ({n_eigs}) must be > initial_target_idx ({initial_target_idx})"
    );
    assert!(max_iter > 0, "max_iter must be positive");

    let solver = FaerComplexEigensolver;
    let mut k0 = initial_k0;
    let mut prev_v: Option<Vec<c64>> = None;
    let mut last_k: Option<c64> = None;
    let mut prev_dk_rel: Option<f64> = None;

    for it in 1..=max_iter {
        let pairs = solver.smallest_complex_pairs(k_mat, s_mat, m_mat, k0, n_eigs)?;
        if pairs.is_empty() {
            return Ok(SelfConsistentResult::Diverged {
                last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                iterations: it,
            });
        }

        // Pick target: seed iteration uses integer index; subsequent
        // iterations use max-overlap.
        let (lambda, mut v_sel, best_overlap) = match &prev_v {
            None => {
                if pairs.len() <= initial_target_idx {
                    return Ok(SelfConsistentResult::Diverged {
                        last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                        iterations: it,
                    });
                }
                let (lam, v) = pairs[initial_target_idx].clone();
                (lam, v, 1.0)
            }
            Some(prev) => {
                // Compute |⟨prev, v_j⟩_M| for each candidate. We
                // do not re-normalize candidates first (faer's
                // eigenvectors come out of QZ in an arbitrary
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
        prev_v = Some(v_sel);
    }

    Ok(SelfConsistentResult::MaxIterations {
        last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
        iterations: max_iter,
    })
}

/// Principal square root of a complex number: the branch with
/// `Re(sqrt(z)) ≥ 0`. For a real positive `z` this is the usual
/// positive square root; for `z` with `Im(z) > 0` (radiating modes,
/// `λ = k²` with positive Q under our convention) the result has
/// `Im(sqrt(z)) > 0` as required for outgoing waves.
fn principal_sqrt(z: c64) -> c64 {
    let r = (z.re * z.re + z.im * z.im).sqrt();
    let re_k = ((r + z.re) / 2.0).sqrt();
    let im_k_mag = ((r - z.re) / 2.0).sqrt();
    let im_k = if z.im >= 0.0 { im_k_mag } else { -im_k_mag };
    c64::new(re_k, im_k)
}

/// `Q = Re(k) / (2 |Im(k)|)`; `f64::INFINITY` when `|Im(k)| < 1e-12`.
fn q_factor(k: c64) -> f64 {
    if k.im.abs() < 1e-12 {
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
