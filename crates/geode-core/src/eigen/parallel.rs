//! Process-global faer parallelism control for the sparse eigensolves.
//!
//! # Why this module exists
//!
//! The default sparse path factors `A = K - σ M` once via faer's sparse LU
//! (`sp_lu`) before running shift-and-invert Lanczos. On the 133k-DOF
//! transmon eigensolve that factorization is ~33% of the wall time, and it
//! is the phase faer can parallelize with rayon (see the design notes at
//! the top of [`crate::eigen::lanczos`] and issue #518). Compiling faer with
//! its `rayon` feature and setting the global parallelism to `Par::rayon(n)`
//! for the factorization is the fair core-for-core comparison against
//! Palace's MPI ranks.
//!
//! # faer 0.24's process-global parallelism
//!
//! faer 0.24 exposes parallelism as a **process-global `AtomicUsize`**:
//! [`faer::set_global_parallelism`] / [`faer::get_global_parallelism`], with
//! **no** per-call `Par` argument on `sp_lu`. That has two consequences the
//! design notes call out:
//!
//! 1. The setting is global, so it races other threads in the same process
//!    that also call faer routines. In this crate the eigensolve owns the
//!    factorization scope, so we set the global exactly around `sp_lu` and
//!    restore it immediately afterward.
//! 2. It must be reverted to serial before the single-RHS triangular-solve
//!    loop, where rayon is measurably *slower* (the per-solve work is
//!    latency-bound). Scoping the guard tightly around the factorization
//!    (not the whole Lanczos loop) achieves exactly this.
//!
//! # RAII, panic-safety, and the correctness gate
//!
//! [`ParallelismGuard`] records the prior global parallelism on construction,
//! sets `Par::rayon(n)`, and restores the prior value on `Drop`. Because
//! `Drop` runs during stack unwinding, the prior value is restored **even if
//! the factorization panics** — the process is never left in a globally
//! parallel state by accident.
//!
//! The number of threads is fixed for a given factorization but does **not**
//! change the arithmetic: faer's sparse LU is a deterministic factorization
//! whose result is independent of the thread count. The eigenvalues are
//! therefore identical (within the existing tolerance) whether run at 1 or N
//! threads; see the cross-thread agreement tests in [`crate::eigen::lanczos`]
//! and [`crate::eigen::complex::lanczos`].
//!
//! # When faer is built without `rayon`
//!
//! `Par::Rayon` only exists when faer is compiled with its `rayon` feature.
//! This module compiles either way: without `rayon` the guard is a no-op that
//! leaves the (already serial) global parallelism untouched, so callers do
//! not need to feature-gate their use of it.

use std::env;

use faer::{Par, get_global_parallelism, set_global_parallelism};

/// The environment variable that overrides the eigensolve thread count.
///
/// When unset (or unparseable), the eigensolve falls back to the physical
/// core count via [`std::thread::available_parallelism`].
pub const NUM_THREADS_ENV: &str = "GEODE_NUM_THREADS";

/// Serialization lock for tests that touch faer's **process-global**
/// parallelism (via [`ParallelismGuard`] or a factorization that sets it).
///
/// `cargo test` runs test functions on multiple threads by default, and
/// faer's global parallelism is a single shared `AtomicUsize`. A test that
/// asserts "the global equals X" can therefore race another test that
/// concurrently sets it to Y. Any test in the crate that observes or mutates
/// the global parallelism must hold this lock for the duration of the
/// observation so those tests run one at a time.
#[cfg(test)]
pub(crate) static PARALLELISM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Resolve the number of threads faer should use for the factorization.
///
/// Precedence:
/// 1. `GEODE_NUM_THREADS`, if set to a parseable positive integer.
/// 2. [`std::thread::available_parallelism`] (physical/logical core count).
/// 3. `1` as a last resort if the platform cannot report a core count.
///
/// A value of `0` or an unparseable value falls through to the core-count
/// default rather than being treated as "serial" — request one thread
/// explicitly (`GEODE_NUM_THREADS=1`) for the single-threaded path.
pub fn resolve_num_threads() -> usize {
    match parse_num_threads(env::var(NUM_THREADS_ENV).ok().as_deref()) {
        Some(n) => n,
        None => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    }
}

/// Pure parse of the `GEODE_NUM_THREADS` value, split out so it can be
/// unit-tested without mutating the process environment (this crate denies
/// `unsafe_code`, and edition-2024 `env::set_var` is `unsafe`).
///
/// Returns `Some(n)` for a positive integer, and `None` (meaning "fall back
/// to the core count") for an unset, empty, zero, or unparseable value.
fn parse_num_threads(raw: Option<&str>) -> Option<usize> {
    raw?.trim().parse::<usize>().ok().filter(|&n| n > 0)
}

/// Panic-safe RAII guard that sets faer's process-global parallelism to
/// `Par::rayon(n)` for its lifetime and restores the prior value on drop.
///
/// Construct one immediately before a faer factorization and let it drop at
/// the end of the factorization scope:
///
/// ```ignore
/// let lu = {
///     let _par = ParallelismGuard::rayon(resolve_num_threads());
///     a.as_ref().sp_lu()?
/// }; // prior global parallelism restored here, even on panic
/// ```
///
/// The guard is a no-op when `n <= 1`, or when faer is compiled without its
/// `rayon` feature (in which case `Par::Rayon` does not exist and the global
/// is left at its serial default).
#[derive(Debug)]
#[must_use = "the guard restores parallelism on drop; binding it to `_` drops it immediately"]
pub struct ParallelismGuard {
    prior: Par,
    /// Whether we actually changed the global parallelism. If we did not
    /// (n <= 1, or faer built without `rayon`), `Drop` skips the restore to
    /// avoid a spurious `set_global_parallelism` call.
    changed: bool,
}

impl ParallelismGuard {
    /// Record the current global parallelism, then set `Par::rayon(n)`.
    ///
    /// `n` is the number of threads. `n <= 1` leaves the global untouched
    /// (the serial default), so a caller can pass `resolve_num_threads()`
    /// unconditionally and get single-threaded behavior for
    /// `GEODE_NUM_THREADS=1`.
    pub fn rayon(n: usize) -> Self {
        let prior = get_global_parallelism();
        let changed = Self::try_set_rayon(n);
        Self { prior, changed }
    }

    /// Set the global parallelism to `Par::rayon(n)` and report whether the
    /// global was actually changed.
    ///
    /// Returns `false` (leaving the global untouched) when `n <= 1` or when
    /// faer was built without its `rayon` feature.
    fn try_set_rayon(n: usize) -> bool {
        if n <= 1 {
            return false;
        }
        // `Par::rayon` and `Par::Rayon` only exist when faer is compiled with
        // its `rayon` feature. This crate's `faer-parallel` feature (on by
        // default) turns that feature on and gates the parallel arm below.
        set_rayon_parallelism(n)
    }
}

impl Drop for ParallelismGuard {
    fn drop(&mut self) {
        if self.changed {
            // Runs during normal scope exit *and* during panic unwinding, so
            // the global parallelism is always restored to its prior value.
            set_global_parallelism(self.prior);
        }
    }
}

/// Set faer's global parallelism to `Par::rayon(n)` when faer's `rayon`
/// feature is enabled; otherwise a no-op that reports `false`.
///
/// faer re-exports the `Par::Rayon` variant only under its own `rayon`
/// feature. We cannot name `Par::rayon` unconditionally, so the two arms
/// below are selected by this crate's `faer-parallel` feature, which turns
/// faer's `rayon` feature on. `faer-parallel` is a default feature, so the
/// parallel arm is the one that compiles in normal builds.
#[cfg(feature = "faer-parallel")]
fn set_rayon_parallelism(n: usize) -> bool {
    set_global_parallelism(Par::rayon(n));
    true
}

#[cfg(not(feature = "faer-parallel"))]
fn set_rayon_parallelism(_n: usize) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RAII behavior of [`ParallelismGuard`]: restore-on-normal-drop,
    /// restore-on-panic, and single-thread-is-a-no-op.
    ///
    /// These three assertions all observe/mutate faer's process-global
    /// parallelism, so they are combined into one `#[test]` that holds
    /// [`PARALLELISM_TEST_LOCK`] for its whole body — otherwise a concurrent
    /// parallelism-touching test could change the global between the set and
    /// the assert. (`cargo test` runs test functions on multiple threads.)
    #[test]
    fn guard_raii_restores_global_parallelism() {
        let _lock = PARALLELISM_TEST_LOCK.lock().unwrap();

        // 1. Restore on normal scope exit.
        let before = get_global_parallelism();
        {
            let _g = ParallelismGuard::rayon(4);
            // (the global may or may not have changed, depending on faer's
            // rayon feature — but it must be back to `before` after the scope)
        }
        assert_eq!(
            get_global_parallelism(),
            before,
            "global parallelism not restored after guard drop"
        );

        // 2. Restore even when the scope panics (Drop runs during unwind).
        let result = std::panic::catch_unwind(|| {
            let _g = ParallelismGuard::rayon(4);
            panic!("boom inside the factorization scope");
        });
        assert!(result.is_err(), "expected the inner closure to panic");
        assert_eq!(
            get_global_parallelism(),
            before,
            "global parallelism not restored after a panicking scope"
        );

        // 3. Requesting a single thread must not change the global at all.
        {
            let _g = ParallelismGuard::rayon(1);
            assert_eq!(
                get_global_parallelism(),
                before,
                "requesting 1 thread should leave global parallelism unchanged"
            );
        }
        assert_eq!(get_global_parallelism(), before);
    }

    /// The `GEODE_NUM_THREADS` parse honors a positive integer and falls
    /// through (to the core-count default) on unset/empty/zero/garbage.
    ///
    /// Tested via the pure [`parse_num_threads`] helper so no `unsafe`
    /// `env::set_var` is needed (this crate denies `unsafe_code`).
    #[test]
    fn parse_num_threads_precedence() {
        assert_eq!(parse_num_threads(Some("3")), Some(3));
        assert_eq!(parse_num_threads(Some("  8 ")), Some(8));
        assert_eq!(parse_num_threads(Some("0")), None);
        assert_eq!(parse_num_threads(Some("")), None);
        assert_eq!(parse_num_threads(Some("not-a-number")), None);
        assert_eq!(parse_num_threads(Some("-4")), None);
        assert_eq!(parse_num_threads(None), None);
    }

    /// `resolve_num_threads` always reports a positive count: with the env
    /// var unset (the CI/default case) it falls back to the core count.
    #[test]
    fn resolve_num_threads_is_positive() {
        assert!(resolve_num_threads() >= 1);
    }
}
