//! Generic convergence-loop combinators (issue #301).
//!
//! GEODE-FEM has several hand-rolled convergence loops — the
//! self-consistent `k₀` Newton iteration
//! ([`crate::silvermuller_self_consistent`]), the Lanczos restart loops
//! ([`crate::lanczos`] / [`crate::complex_lanczos`]), and the
//! bracketing/bisection root finder ([`crate::mie`]). This module
//! provides two small combinators that capture the *shape* shared by the
//! state-carry loops so the duplication collapses onto one tested
//! primitive:
//!
//! * [`iterate_while`] — a state-update loop with a **scalar** continue
//!   condition and a hard iteration cap. Each step maps the carried
//!   state to either a continuing state or a terminal value.
//! * [`iterate_while_with_prev`] — the same, but the step closure is
//!   handed both the *current* and the *previous* carried state, so
//!   recurrences (divergence guards, momentum, Aitken-style
//!   acceleration) can be expressed without the caller threading the
//!   history by hand.
//!
//! # Whiteroom L4 contract (the three graph-only restrictions)
//!
//! These combinators are deliberately shaped to match the firm
//! whiteroom **L4 `iterate_while` / `iterate_while_with_prev`** surface,
//! which survives a graph-only (trace-once) tensor backend *only* under
//! three restrictions (Pass-1 friction artifact 11). They are the
//! combinator's invariants — callers must honor them for the loop to be
//! portable to a graph backend:
//!
//! 1. **Loop-invariant carried-state shapes.** The carried state `S`
//!    must have the same logical shape on every iteration. A graph
//!    backend traces the loop body once; a state slot whose tensor shape
//!    changes between iterations cannot be expressed. (Concretely: do
//!    not grow a `Vec` inside the carried state — that is what disqualifies
//!    the Lanczos Krylov-basis loops; see the module-level note in
//!    [`crate::lanczos`].)
//! 2. **Scalar continue-condition.** The decision to continue or stop is
//!    a single scalar predicate evaluated inside the step, not an
//!    element-wise mask with a data-dependent reduction. There is no
//!    per-element `break`.
//! 3. **No data-dependent output counts.** The number of values the loop
//!    ultimately produces is fixed by the carried-state type, not by the
//!    data. The loop returns exactly one final state plus a report; it
//!    does not emit a runtime-variable-length stream of outputs.
//!
//! The combinators do **not** enforce these at the type level (they are
//! ordinary CPU control flow today); they document the contract so that
//! the eventual L4 mapping is a *realization* of an already-conforming
//! shape rather than a post-hoc retrofit.

/// Report describing how an [`iterate_while`] /
/// [`iterate_while_with_prev`] loop terminated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IterReport {
    /// Number of step evaluations performed (`1..=max_iters`). A value
    /// equal to `max_iters` together with `converged == false` means the
    /// cap was hit.
    pub iterations: usize,
    /// `true` if the step returned [`Step::Done`] before the cap;
    /// `false` if the loop exhausted `max_iters` while every step
    /// returned [`Step::Continue`].
    pub converged: bool,
}

/// Outcome of a single iteration step.
///
/// The step closure consumes the carried state and returns either a new
/// carried state to iterate again, or a terminal value that stops the
/// loop. Because the terminal type `T` is distinct from the carried type
/// `S`, callers can model "ran to a definitive answer" (`Done`) versus
/// "still refining" (`Continue`) without smuggling a sentinel into the
/// state — this keeps the continue-condition a clean scalar decision
/// (contract restriction 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<S, T> {
    /// Keep iterating with this updated carried state.
    Continue(S),
    /// Stop now with this terminal value (the scalar predicate fired).
    Done(T),
}

/// Result of running a combinator: either a terminal value produced by a
/// step, or the final carried state when the iteration cap was reached
/// first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IterOutcome<S, T> {
    /// A step returned [`Step::Done`] before the cap.
    Done(T),
    /// `max_iters` steps all returned [`Step::Continue`]; this is the
    /// last carried state.
    MaxIters(S),
}

/// Drive `step` from `state0` until it returns [`Step::Done`] or until
/// `max_iters` step evaluations have run, whichever comes first.
///
/// This is the state-update + scalar-predicate loop combinator. The
/// per-iteration continue/stop decision lives *inside* `step` (it returns
/// `Done(T)` when the scalar predicate fires); the combinator only owns
/// the counter and the hard cap. See the [module docs](self) for the
/// three graph-only contract restrictions the carried state must honor.
///
/// # Arguments
///
/// * `state0` — initial carried state.
/// * `max_iters` — hard cap on step evaluations. Must be `> 0`.
/// * `step` — maps the current iteration index (`1`-based) and the
///   carried state to a [`Step`]. Returning [`Step::Done`] stops the
///   loop immediately.
///
/// # Returns
///
/// `(outcome, report)` where `outcome` is [`IterOutcome::Done`] with the
/// terminal value if a step finished, or [`IterOutcome::MaxIters`] with
/// the final carried state if the cap was reached. `report.iterations`
/// counts the step evaluations actually performed.
///
/// # Panics
///
/// Panics if `max_iters == 0`.
pub fn iterate_while<S, T, F>(
    state0: S,
    max_iters: usize,
    mut step: F,
) -> (IterOutcome<S, T>, IterReport)
where
    F: FnMut(usize, S) -> Step<S, T>,
{
    assert!(max_iters > 0, "max_iters must be positive");

    let mut state = state0;
    for it in 1..=max_iters {
        match step(it, state) {
            Step::Done(value) => {
                return (
                    IterOutcome::Done(value),
                    IterReport {
                        iterations: it,
                        converged: true,
                    },
                );
            }
            Step::Continue(next) => {
                state = next;
            }
        }
    }

    (
        IterOutcome::MaxIters(state),
        IterReport {
            iterations: max_iters,
            converged: false,
        },
    )
}

/// Like [`iterate_while`], but the step closure also receives the
/// *previous* carried state (`None` on the first iteration), enabling
/// recurrences without the caller threading the history manually.
///
/// This is the recommended shape for fixed-point iterations that need to
/// compare the latest update against the one before it — e.g. a
/// divergence guard that aborts when the relative step *increases*
/// between successive iterations. The combinator owns the one-step
/// history; the carried state `S` must still be `Clone` and obey the
/// loop-invariant-shape restriction (contract restriction 1).
///
/// # Arguments
///
/// * `state0` — initial carried state.
/// * `max_iters` — hard cap on step evaluations. Must be `> 0`.
/// * `step` — maps `(iteration_index, prev_state, current_state)` to a
///   [`Step`]. `prev_state` is `None` for `iteration_index == 1` and
///   `Some(previous_carried_state)` thereafter.
///
/// # Returns
///
/// Same `(outcome, report)` contract as [`iterate_while`].
///
/// # Panics
///
/// Panics if `max_iters == 0`.
pub fn iterate_while_with_prev<S, T, F>(
    state0: S,
    max_iters: usize,
    mut step: F,
) -> (IterOutcome<S, T>, IterReport)
where
    S: Clone,
    F: FnMut(usize, Option<&S>, S) -> Step<S, T>,
{
    assert!(max_iters > 0, "max_iters must be positive");

    let mut state = state0;
    let mut prev: Option<S> = None;
    for it in 1..=max_iters {
        let current = state.clone();
        match step(it, prev.as_ref(), state) {
            Step::Done(value) => {
                return (
                    IterOutcome::Done(value),
                    IterReport {
                        iterations: it,
                        converged: true,
                    },
                );
            }
            Step::Continue(next) => {
                prev = Some(current);
                state = next;
            }
        }
    }

    (
        IterOutcome::MaxIters(state),
        IterReport {
            iterations: max_iters,
            converged: false,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterate_while_runs_to_done() {
        // Count up; stop when state reaches 5.
        let (outcome, report) = iterate_while(0i32, 100, |_it, s| {
            if s >= 5 {
                Step::Done(s)
            } else {
                Step::Continue(s + 1)
            }
        });
        assert_eq!(outcome, IterOutcome::Done(5));
        // it=1 sees 0 -> 1, ... it=6 sees 5 -> Done. Six evaluations.
        assert_eq!(
            report,
            IterReport {
                iterations: 6,
                converged: true,
            }
        );
    }

    #[test]
    fn iterate_while_hits_cap() {
        // Never finishes — should cap out and return final state.
        let (outcome, report) = iterate_while(0i32, 4, |_it, s| Step::<i32, i32>::Continue(s + 1));
        assert_eq!(outcome, IterOutcome::MaxIters(4));
        assert_eq!(
            report,
            IterReport {
                iterations: 4,
                converged: false,
            }
        );
    }

    #[test]
    fn iterate_while_iteration_index_is_one_based() {
        let mut seen = Vec::new();
        let (_outcome, _report) = iterate_while(0i32, 3, |it, s| {
            seen.push(it);
            Step::<i32, i32>::Continue(s + 1)
        });
        assert_eq!(seen, vec![1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "max_iters must be positive")]
    fn iterate_while_rejects_zero_cap() {
        let _ = iterate_while(0i32, 0, |_it, s| Step::<i32, i32>::Continue(s));
    }

    #[test]
    fn with_prev_threads_previous_state() {
        // On the first iter, prev is None. On subsequent iters prev is
        // the state from one step earlier. Stop when current == 3 and
        // return the (prev, current) pair we observed.
        let (outcome, report) = iterate_while_with_prev(0i32, 100, |_it, prev, s| {
            if s == 3 {
                Step::Done((prev.copied(), s))
            } else {
                Step::Continue(s + 1)
            }
        });
        // States visited as `current`: 0, 1, 2, 3. When current == 3,
        // prev should be 2.
        assert_eq!(outcome, IterOutcome::Done((Some(2), 3)));
        assert_eq!(
            report,
            IterReport {
                iterations: 4,
                converged: true,
            }
        );
    }

    #[test]
    fn with_prev_first_iter_prev_is_none() {
        let (outcome, _report) = iterate_while_with_prev(42i32, 100, |it, prev, s| {
            assert_eq!(it, 1);
            assert!(prev.is_none(), "prev must be None on the first iteration");
            Step::Done((prev.copied(), s))
        });
        assert_eq!(outcome, IterOutcome::Done((None, 42)));
    }

    #[test]
    fn with_prev_divergence_guard_pattern() {
        // Emulate the divergence guard from the self-consistent driver:
        // carry a residual, abort if it increases between steps. Here the
        // residual sequence is 8, 4, 6 (increases at step 3).
        let residuals = [8.0_f64, 4.0, 6.0, 1.0];
        let (outcome, report) = iterate_while_with_prev(0usize, 10, |_it, prev, idx| {
            let r = residuals[idx];
            if let Some(&prev_idx) = prev
                && r > residuals[prev_idx]
            {
                return Step::Done(format!("diverged at idx {idx}"));
            }
            Step::Continue(idx + 1)
        });
        assert_eq!(outcome, IterOutcome::Done("diverged at idx 2".to_string()));
        assert_eq!(report.iterations, 3);
        assert!(report.converged);
    }

    #[test]
    fn with_prev_hits_cap() {
        let (outcome, report) =
            iterate_while_with_prev(0i32, 3, |_it, _prev, s| Step::<i32, i32>::Continue(s + 1));
        assert_eq!(outcome, IterOutcome::MaxIters(3));
        assert_eq!(
            report,
            IterReport {
                iterations: 3,
                converged: false,
            }
        );
    }
}
