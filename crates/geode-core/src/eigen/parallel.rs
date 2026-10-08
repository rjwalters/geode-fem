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
//! 2. The factorization's thread count should not leak into the single-RHS
//!    triangular-solve loop, where rayon is measurably *slower* (the per-solve
//!    work is latency-bound). Scoping the guard tightly around the
//!    factorization (not the whole Lanczos loop) restores the prior global
//!    afterward. Note that the prior global is whatever the process had set;
//!    faer 0.24's own default is `Par::rayon(0)` (every core), not serial. So
//!    the guard alone leaves the solve loop on the rayon pool, and the loop
//!    holds a [`SequentialSolveScope`] as well (next section).
//!
//! # RAII, panic-safety, and the correctness gate
//!
//! [`ParallelismGuard`] records the prior global parallelism on construction,
//! sets the requested parallelism ([`ParallelismGuard::cap`]: `Par::Seq` for
//! `n <= 1`, `Par::rayon(n)` otherwise), and restores the prior value on
//! `Drop`. Because
//! `Drop` runs during stack unwinding, the prior value is restored **even if
//! the factorization panics** — the process is never left in a globally
//! parallel state by accident.
//!
//! The eigensolves use [`ParallelismGuard::cap`], so `GEODE_NUM_THREADS=1`
//! really does give a serial factorization (faer's global default is every
//! core, so leaving the global untouched would not). For a **fixed** thread
//! count faer's sparse LU is deterministic, so repeated runs at the same
//! count are bit-identical. Across **different** thread counts the result is
//! not guaranteed to be bit-identical: a parallel factorization may group
//! floating-point operations differently, so eigenvalues can differ at
//! roundoff (well inside the solver tolerance). On the small pencils in the
//! cross-thread agreement tests ([`crate::eigen::lanczos`] and
//! [`crate::eigen::complex::SparseComplexShiftInvertLanczos`]) serial and
//! 4-thread factorizations do agree bit-for-bit, and those tests assert it as
//! a determinism tripwire, but larger problems should be compared within
//! tolerance.
//!
//! # Sequential scopes for repeated small solves
//!
//! faer's sparse `Lu::solve_in_place` reads the same global and hands it to
//! the triangular solves on the supernodes. A loop that calls it once or
//! twice per step must not run those solves on the rayon pool: each one is
//! small, and sharing it out costs far more than it saves (issue #946; the
//! measurements are on [`SequentialSolveScope`]'s two users). The two loops
//! that do this are the driven solve's AMS-preconditioned COCG (the V-cycle's
//! coarse solves) and the direct shift-invert Lanczos loop. Each holds a
//! [`SequentialSolveScope`], which sets `Par::Seq` for the whole loop. Unlike
//! [`ParallelismGuard`] it is reference-counted, so scopes held by concurrent
//! solves on different threads can end in any order.
//!
//! # When faer is built without `rayon`
//!
//! `Par::Rayon` only exists when faer is compiled with its `rayon` feature.
//! This module compiles either way: without `rayon` the guard is a no-op that
//! leaves the (already serial) global parallelism untouched, so callers do
//! not need to feature-gate their use of it.

use std::cell::Cell;
use std::env;
use std::sync::{Mutex, MutexGuard, PoisonError};

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
///
/// The lock only serializes the tests that take it. Most eigensolver tests do
/// not, and every direct eigensolve changes the global twice: a
/// [`ParallelismGuard`] around its factorization and a
/// [`SequentialSolveScope`] around its Lanczos loop (issue #946). So no test
/// in this binary may assert on the *value* of faer's global. Those
/// assertions live in the integration target
/// `tests/faer_global_parallelism.rs`, which holds a single test and so has
/// the process to itself.
#[cfg(test)]
pub(crate) static PARALLELISM_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

thread_local! {
    /// Per-thread override installed by [`with_thread_budget`].
    static THREAD_BUDGET: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Resolve the number of threads faer should use for the factorization.
///
/// Precedence:
/// 1. A per-thread budget installed by [`with_thread_budget`] on the
///    calling thread (used when several solves run concurrently).
/// 2. `GEODE_NUM_THREADS`, if set to a parseable positive integer.
/// 3. [`std::thread::available_parallelism`] (physical/logical core count).
/// 4. `1` as a last resort if the platform cannot report a core count.
///
/// A value of `0` or an unparseable value falls through to the core-count
/// default rather than being treated as "serial" — request one thread
/// explicitly (`GEODE_NUM_THREADS=1`) for the single-threaded path.
pub fn resolve_num_threads() -> usize {
    if let Some(n) = THREAD_BUDGET.with(Cell::get) {
        return n;
    }
    match parse_num_threads(env::var(NUM_THREADS_ENV).ok().as_deref()) {
        Some(n) => n,
        None => std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1),
    }
}

/// Run `f` with [`resolve_num_threads`] returning `n.max(1)` **on the
/// calling thread**, restoring the previous value afterward (also on panic).
///
/// This is the per-worker thread budget for callers that run several solves
/// concurrently on their own threads (the CLI's `--jobs`, issues #747/#755).
/// Every host-side pool sized from [`resolve_num_threads`] (the Nédélec
/// sparsity / scatter-map build via [`install_on_pool`]) and every
/// factorization guarded with `ParallelismGuard::cap(resolve_num_threads())`
/// inside `f` then takes only its share of the cores, instead of each
/// concurrent worker asking for the whole machine.
///
/// The override is thread-local: it does not propagate to threads that `f`
/// spawns, and it does not touch faer's process-global parallelism (pair it
/// with a [`ParallelismGuard::cap`] for that).
pub fn with_thread_budget<R>(n: usize, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<usize>);
    impl Drop for Restore {
        fn drop(&mut self) {
            THREAD_BUDGET.with(|b| b.set(self.0));
        }
    }
    let _restore = Restore(THREAD_BUDGET.with(|b| b.replace(Some(n.max(1)))));
    f()
}

/// Run `f` on a scoped rayon thread pool of exactly `n_threads` workers,
/// returning its result.
///
/// This is the host-side counterpart to [`ParallelismGuard`]: where the guard
/// scopes faer's *process-global* parallelism around the sparse factorization,
/// this scopes *rayon's* worker count around a block of data-parallel host
/// work (the FEM assembler's index/pattern construction, issue #522). Building
/// a dedicated pool — rather than relying on rayon's implicit global pool —
/// means a single `GEODE_NUM_THREADS` knob (via [`resolve_num_threads`])
/// controls the assembly thread count deterministically, independent of
/// however many cores rayon's global pool would otherwise grab.
///
/// `f` is **always** run inside a freshly built pool of exactly
/// `n_threads.max(1)` workers — including the `n_threads == 1` case, which
/// yields a single-worker pool that runs the (rayon-based) closure serially.
/// This is deliberate: any rayon parallel iterator inside `f` that is *not*
/// wrapped in a pool would otherwise escape to rayon's implicit global pool
/// (all machine cores), so a naive "n <= 1 ⇒ just call f()" shortcut would
/// silently ignore the `GEODE_NUM_THREADS=1` cap and run fully parallel. A
/// one-thread pool makes `n_threads` a hard cap at every count, which is what
/// the strong-scaling measurement and the serial correctness baseline require.
///
/// The parallel work inside `f` must be *order-preserving* (rayon collect /
/// `flat_map_iter`) so its result is bit-for-bit identical at any thread count
/// — the assembler builds only integer index vectors, so there is no
/// floating-point reduction whose order could change.
///
/// Only compiled when the `faer-parallel` feature is on (which also pulls in
/// the `rayon` dependency); the serial build never references it.
#[cfg(feature = "faer-parallel")]
pub fn install_on_pool<R, F>(n_threads: usize, f: F) -> R
where
    R: Send,
    F: FnOnce() -> R + Send,
{
    match rayon::ThreadPoolBuilder::new()
        .num_threads(n_threads.max(1))
        .build()
    {
        Ok(pool) => pool.install(f),
        // A pool-construction failure is non-fatal: fall back to running the
        // (order-preserving) closure on the ambient pool. The result is
        // identical — only the thread-count cap is lost.
        Err(_) => f(),
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
///     let _par = ParallelismGuard::cap(resolve_num_threads());
///     a.as_ref().sp_lu()?
/// }; // prior global parallelism restored here, even on panic
/// ```
///
/// Prefer [`ParallelismGuard::cap`], which makes `n <= 1` serial. The legacy
/// [`ParallelismGuard::rayon`] constructor leaves the global untouched for
/// `n <= 1`. Both are no-ops when faer is compiled without its `rayon`
/// feature, in which case `Par::Rayon` does not exist and the global is
/// already serial.
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
    /// `n` is the number of threads. `n <= 1` leaves the global **untouched**,
    /// and faer 0.24's global default is `Par::rayon(0)` (every core), so with
    /// the default global this does **not** give a serial factorization for
    /// `GEODE_NUM_THREADS=1`. Use [`ParallelismGuard::cap`] when `1` must
    /// mean serial; every in-tree factorization call site does.
    pub fn rayon(n: usize) -> Self {
        let prior = get_global_parallelism();
        let changed = Self::try_set_rayon(n);
        Self { prior, changed }
    }

    /// Record the current global parallelism, then cap it at `n` threads:
    /// `Par::Seq` for `n <= 1`, `Par::rayon(n)` otherwise.
    ///
    /// Unlike [`ParallelismGuard::rayon`], `n == 1` really does make the
    /// factorization serial rather than leaving the global (by default
    /// `Par::rayon(0)`, i.e. every core) untouched. This is the per-worker
    /// budget used when several factorizations run concurrently (the CLI's
    /// `--jobs`, issue #747), where each must take only its share of the
    /// cores. Without faer's `rayon` feature the global is already serial
    /// and this is a no-op.
    pub fn cap(n: usize) -> Self {
        let prior = get_global_parallelism();
        let changed = if n <= 1 {
            if cfg!(feature = "faer-parallel") {
                set_global_parallelism(Par::Seq);
                true
            } else {
                false
            }
        } else {
            Self::try_set_rayon(n)
        };
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

/// Bookkeeping for the live [`SequentialSolveScope`]s: how many there are,
/// and the global parallelism the first of them replaced.
///
/// Kept apart from faer's global so the save / restore rule can be tested
/// without touching process state.
#[derive(Debug)]
struct SequentialScopes {
    live: usize,
    prior: Option<Par>,
}

impl SequentialScopes {
    const fn new() -> Self {
        Self {
            live: 0,
            prior: None,
        }
    }

    /// Register one more scope. `current` is the global parallelism right
    /// now. Returns `true` when the caller must set the global to `Par::Seq`,
    /// which is the case for the first live scope only: that scope's
    /// `current` is the value to restore later.
    fn enter(&mut self, current: Par) -> bool {
        self.live += 1;
        if self.live == 1 {
            self.prior = Some(current);
            true
        } else {
            false
        }
    }

    /// Unregister one scope. Returns the parallelism to restore when it was
    /// the last live one, and `None` while any other scope is still live.
    fn leave(&mut self) -> Option<Par> {
        self.live = self.live.saturating_sub(1);
        if self.live == 0 {
            self.prior.take()
        } else {
            None
        }
    }
}

static SEQUENTIAL_SCOPES: Mutex<SequentialScopes> = Mutex::new(SequentialScopes::new());

/// Lock [`SEQUENTIAL_SCOPES`]. A poisoned lock is still used: the state is
/// two plain fields and every update leaves it consistent.
fn sequential_scopes() -> MutexGuard<'static, SequentialScopes> {
    SEQUENTIAL_SCOPES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// RAII scope that makes faer's process-global parallelism `Par::Seq` while
/// it is alive (issue #946).
///
/// Hold one around a loop that calls a faer sparse `solve_in_place` on every
/// step, such as a Krylov solve whose preconditioner does:
///
/// ```ignore
/// let _seq = SequentialSolveScope::enter();
/// ksp.solve(a, b, x, &precond)?; // restored here too, on the `?` return
/// ```
///
/// Take **one scope per solve, not one per preconditioner application**. The
/// global is a single atomic shared by every thread, so each change is a
/// window in which another thread's faer call sees the wrong value.
///
/// # Why not [`ParallelismGuard::cap`]`(1)`
///
/// A `ParallelismGuard` restores the value it saw when it was built. Two of
/// them on different threads that end in the opposite order to the one they
/// started in leave the global wrong: the one built second saved `Par::Seq`,
/// and if it is dropped last it restores `Par::Seq` for good. Concurrent
/// solves do end in arbitrary order (the CLI's `--jobs`), so this scope is
/// reference-counted instead: the first live scope saves the prior value and
/// sets `Par::Seq`, later ones only count, and the last one to drop restores
/// the saved value. The drop runs on early return and during panic unwinding.
///
/// # What it does not do
///
/// It does not isolate a thread, and it does not coordinate with
/// [`ParallelismGuard`]:
///
/// - A faer factorization started on another thread while a scope is live
///   also runs sequentially, unless that thread sets its own
///   `ParallelismGuard`.
/// - A scope and a `ParallelismGuard` on two threads that overlap without
///   nesting each restore what they saw, as two overlapping
///   `ParallelismGuard`s already do. The one that ends last wins: the global
///   can be left at `Par::Seq`, or at the guard's thread count.
///
/// Neither changes a result beyond roundoff: the setting is how many threads
/// faer uses, not what it computes. In this crate the second case leaves the
/// value unchanged whenever every guard caps at the same count, which is how
/// the eigensolvers and the CLI's `--jobs` use them.
#[derive(Debug)]
#[must_use = "the scope ends on drop; binding it to `_` drops it immediately"]
pub struct SequentialSolveScope {
    _private: (),
}

impl SequentialSolveScope {
    /// Start a sequential scope. See the type docs.
    pub fn enter() -> Self {
        // The lock is held across the read and the write of faer's global so
        // two scopes starting together cannot both think they are the first.
        let mut scopes = sequential_scopes();
        if scopes.enter(get_global_parallelism()) {
            set_global_parallelism(Par::Seq);
        }
        Self { _private: () }
    }
}

impl Drop for SequentialSolveScope {
    fn drop(&mut self) {
        let mut scopes = sequential_scopes();
        if let Some(prior) = scopes.leave() {
            set_global_parallelism(prior);
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

    /// The save / restore rule of [`SequentialScopes`], on plain values: only
    /// the first live scope asks for `Par::Seq` and records the prior value,
    /// and only the last one to leave hands it back, whatever the order.
    #[test]
    fn sequential_scopes_restore_the_prior_only_when_the_last_one_leaves() {
        let prior = Par::rayon(3);
        let mut scopes = SequentialScopes::new();

        assert!(scopes.enter(prior), "the first scope must set Par::Seq");
        // A second scope starts while the global already reads Par::Seq. It
        // must not overwrite the saved prior with that.
        assert!(!scopes.enter(Par::Seq), "a nested scope must not set again");
        assert!(!scopes.enter(Par::Seq));
        assert_eq!(scopes.leave(), None, "two scopes are still live");
        assert_eq!(scopes.leave(), None, "one scope is still live");
        assert_eq!(scopes.leave(), Some(prior), "the last scope restores");

        // Fully released: the next scope is a first scope again, with its own
        // prior, and an unbalanced extra `leave` restores nothing.
        assert_eq!(scopes.leave(), None);
        assert!(scopes.enter(Par::Seq));
        assert_eq!(scopes.leave(), Some(Par::Seq));
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

    /// `with_thread_budget` overrides `resolve_num_threads` on the calling
    /// thread only, nests, clamps 0 to 1, and restores on exit and on panic.
    #[test]
    fn thread_budget_overrides_and_restores() {
        let ambient = resolve_num_threads();
        assert_eq!(with_thread_budget(3, resolve_num_threads), 3);
        assert_eq!(with_thread_budget(0, resolve_num_threads), 1);
        with_thread_budget(5, || {
            assert_eq!(with_thread_budget(2, resolve_num_threads), 2);
            assert_eq!(resolve_num_threads(), 5);
            // Not inherited by other threads.
            let other = std::thread::spawn(resolve_num_threads).join().unwrap();
            assert_eq!(other, ambient);
        });
        assert_eq!(resolve_num_threads(), ambient);
        let r = std::panic::catch_unwind(|| with_thread_budget(7, || panic!("boom")));
        assert!(r.is_err());
        assert_eq!(resolve_num_threads(), ambient);
    }
}
