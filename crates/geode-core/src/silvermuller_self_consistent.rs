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
    let mut k0 = initial_k0;
    let mut last_k: Option<c64> = None;
    let mut prev_dk_rel: Option<f64> = None;

    for it in 1..=max_iter {
        let lambdas = solver.smallest_complex_eigenvalues(k_mat, s_mat, m_mat, k0, n_eigs)?;
        if lambdas.len() <= target_idx {
            // Solver returned fewer eigenvalues than requested (e.g.
            // mass-pencil singularities skipped). Treat as divergence
            // at the last stable point.
            return Ok(SelfConsistentResult::Diverged {
                last_k: last_k.unwrap_or(c64::new(k0, 0.0)),
                iterations: it,
            });
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
            return Ok(SelfConsistentResult::Converged {
                k: k_target,
                q,
                iterations: it,
            });
        }

        // Divergence guard — only active *after* the damped phase, and
        // we need at least one prior step to compare against.
        let dk_rel = abs_dk / k0_mag;
        if it > DAMPED_ITERATIONS {
            if let Some(prev) = prev_dk_rel {
                if dk_rel > prev {
                    return Ok(SelfConsistentResult::Diverged {
                        last_k: last_k.unwrap_or(k_target),
                        iterations: it,
                    });
                }
            }
        }
        prev_dk_rel = Some(dk_rel);

        // Damped update.
        let alpha = if it <= DAMPED_ITERATIONS { 0.5 } else { 1.0 };
        k0 += alpha * dk;
        last_k = Some(k_target);
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
}
