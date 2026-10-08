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
//! [`ParallelismGuard`] caps the global for its lifetime
//! ([`ParallelismGuard::cap`]: `Par::Seq` for `n <= 1`, `Par::rayon(n)`
//! otherwise) and lifts the cap on `Drop`. Because `Drop` runs during stack
//! unwinding, that happens **even if the factorization panics**, so the
//! process is never left in a globally parallel state by accident.
//!
//! Guards and [`SequentialSolveScope`]s register in one mutex-protected
//! registry instead of each saving and restoring the value it saw. The global
//! is `Par::Seq` while any scope is live, else the cap of the live guard
//! built last, else the ambient value from before the first of them. Any
//! interleaving of guards and scopes on any threads therefore ends at the
//! ambient value; the details are on [`ParallelismGuard`].
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
//! [`SequentialSolveScope`], which sets `Par::Seq` for the whole loop. Scopes
//! are counted, so those held by concurrent solves on different threads can
//! end in any order, and a `ParallelismGuard` built on another thread
//! meanwhile cannot put the loop back on the rayon pool.
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

/// The one piece of state behind [`ParallelismGuard`] and
/// [`SequentialSolveScope`]: what is live, and the value faer's global had
/// before any of it was.
///
/// Neither type keeps a "previous value" of its own. Each registers here on
/// construction and unregisters on drop, and after every change faer's global
/// is set to [`GlobalParallelism::target`]:
///
/// 1. `Par::Seq` while any scope is live;
/// 2. otherwise the cap of the live guard that was built last;
/// 3. otherwise the ambient value, which is also forgotten at that point.
///
/// The methods take and return plain values and never touch faer's global, so
/// the rule can be tested without touching process state.
#[derive(Debug)]
struct GlobalParallelism {
    /// The global as it was when the first guard or scope became live.
    /// `Some` exactly while at least one guard or scope is live.
    ambient: Option<Par>,
    /// Number of live [`SequentialSolveScope`]s.
    seq_scopes: usize,
    /// The live guards as `(id, cap)`, in the order they were built.
    guards: Vec<(u64, Par)>,
    /// Id for the next guard.
    next_guard_id: u64,
}

impl GlobalParallelism {
    const fn new() -> Self {
        Self {
            ambient: None,
            seq_scopes: 0,
            guards: Vec::new(),
            next_guard_id: 0,
        }
    }

    /// Record `current` as the ambient value if nothing is live yet.
    fn capture_ambient(&mut self, current: Par) {
        if self.ambient.is_none() {
            self.ambient = Some(current);
        }
    }

    /// The value the global must have now, or `None` when nothing is live
    /// and the global is not ours to set.
    fn target(&self) -> Option<Par> {
        if self.seq_scopes > 0 {
            Some(Par::Seq)
        } else if let Some(&(_, cap)) = self.guards.last() {
            Some(cap)
        } else {
            None
        }
    }

    /// The value to set after something was unregistered: the target while
    /// anything is still live, else the ambient value (handed back once).
    fn target_after_leave(&mut self) -> Option<Par> {
        self.target().or_else(|| self.ambient.take())
    }

    /// Register a scope. `current` is faer's global right now. Returns the
    /// value to set.
    fn enter_scope(&mut self, current: Par) -> Par {
        self.capture_ambient(current);
        self.seq_scopes += 1;
        Par::Seq
    }

    /// Unregister a scope. Returns the value to set, or `None` when no scope
    /// was registered.
    fn leave_scope(&mut self) -> Option<Par> {
        if self.seq_scopes == 0 {
            return None;
        }
        self.seq_scopes -= 1;
        self.target_after_leave()
    }

    /// Register a guard capping at `cap`. `current` is faer's global right
    /// now. Returns the guard's id and the value to set.
    fn enter_guard(&mut self, cap: Par, current: Par) -> (u64, Par) {
        self.capture_ambient(current);
        let id = self.next_guard_id;
        self.next_guard_id = self.next_guard_id.wrapping_add(1);
        self.guards.push((id, cap));
        // A live scope outranks the new guard.
        (id, self.target().unwrap_or(cap))
    }

    /// Unregister the guard `id`. Returns the value to set, or `None` when
    /// no such guard was registered.
    fn leave_guard(&mut self, id: u64) -> Option<Par> {
        let at = self.guards.iter().position(|&(g, _)| g == id)?;
        self.guards.remove(at);
        self.target_after_leave()
    }
}

static GLOBAL_PARALLELISM: Mutex<GlobalParallelism> = Mutex::new(GlobalParallelism::new());

/// Lock [`GLOBAL_PARALLELISM`]. A poisoned lock is still used: nothing that
/// can panic runs while it is held, and every update leaves the state
/// consistent.
fn global_parallelism() -> MutexGuard<'static, GlobalParallelism> {
    GLOBAL_PARALLELISM
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Panic-safe RAII guard that caps faer's process-global parallelism for its
/// lifetime.
///
/// Construct one immediately before a faer factorization and let it drop at
/// the end of the factorization scope:
///
/// ```ignore
/// let lu = {
///     let _par = ParallelismGuard::cap(resolve_num_threads());
///     a.as_ref().sp_lu()?
/// }; // the cap is lifted here, even on panic
/// ```
///
/// Prefer [`ParallelismGuard::cap`], which makes `n <= 1` serial. The legacy
/// [`ParallelismGuard::rayon`] constructor leaves the global untouched for
/// `n <= 1`. Both are no-ops when faer is compiled without its `rayon`
/// feature, in which case `Par::Rayon` does not exist and the global is
/// already serial.
///
/// # Several guards and scopes at once
///
/// Guards and [`SequentialSolveScope`]s share one registry behind one mutex.
/// A guard does not save and restore a value of its own. While anything is
/// live, faer's global is:
///
/// 1. `Par::Seq` if any `SequentialSolveScope` is live, on any thread;
/// 2. otherwise the cap of the live guard that was **built last**, on any
///    thread;
/// 3. and once the last guard or scope has dropped, the *ambient* value: what
///    the global was when the first of them was built.
///
/// So the drop order does not matter. Any interleaving of guards and scopes,
/// on any threads, ends with the global at the ambient value.
///
/// Consequences worth knowing:
///
/// - **Nesting on one thread, dropped innermost first,** behaves as it always
///   did: the inner cap applies until the inner guard drops, then the outer
///   cap again, then the ambient value.
/// - **Dropping out of order** (an outer guard before an inner one, or two
///   guards on two threads) used to leave the global at whatever the last
///   guard to drop had saved, which could be a dead guard's cap for the rest
///   of the process. Now the remaining guard's cap stays in force until it
///   drops, and the ambient value comes back after it.
/// - **A guard built while a scope is live does not raise the global.** The
///   factorization it covers runs sequentially until the scope ends. The cap
///   takes effect at that point if the guard is still live.
/// - **Guards on different threads with different caps** share the one
///   global, so all of them run at the cap built last. faer has no per-call
///   thread count for `sp_lu` to do better with.
/// - **Something else writes faer's global while a guard or scope is live**
///   (a direct [`faer::set_global_parallelism`] call): the written value
///   lasts until the next guard or scope is built or dropped, which sets the
///   global by the rule above. It is not remembered, and the ambient value is
///   what comes back at the end. A write made while nothing is live is simply
///   the new ambient value.
#[derive(Debug)]
#[must_use = "the guard restores parallelism on drop; binding it to `_` drops it immediately"]
pub struct ParallelismGuard {
    /// This guard's id in the registry. `None` when the guard is a no-op
    /// (`rayon(n)` with `n <= 1`, or faer built without `rayon`): it was
    /// never registered and `Drop` leaves the global alone.
    id: Option<u64>,
}

impl ParallelismGuard {
    /// Cap the global parallelism at `Par::rayon(n)` until the guard drops.
    ///
    /// `n` is the number of threads. `n <= 1` leaves the global **untouched**,
    /// and faer 0.24's global default is `Par::rayon(0)` (every core), so with
    /// the default global this does **not** give a serial factorization for
    /// `GEODE_NUM_THREADS=1`. Use [`ParallelismGuard::cap`] when `1` must
    /// mean serial; every in-tree factorization call site does.
    pub fn rayon(n: usize) -> Self {
        Self::register(if n <= 1 { None } else { rayon_par(n) })
    }

    /// Cap the global parallelism at `n` threads until the guard drops:
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
        Self::register(if n > 1 {
            rayon_par(n)
        } else if cfg!(feature = "faer-parallel") {
            Some(Par::Seq)
        } else {
            None
        })
    }

    /// Register a guard for `cap` and set the global by the registry's rule.
    /// `None` builds a no-op guard.
    fn register(cap: Option<Par>) -> Self {
        let Some(cap) = cap else {
            return Self { id: None };
        };
        // The lock is held across the read and the write of faer's global,
        // as it is in every other enter and drop, so the registry and the
        // global cannot disagree.
        let mut state = global_parallelism();
        let (id, target) = state.enter_guard(cap, get_global_parallelism());
        set_global_parallelism(target);
        Self { id: Some(id) }
    }
}

impl Drop for ParallelismGuard {
    fn drop(&mut self) {
        // Runs during normal scope exit *and* during panic unwinding.
        if let Some(id) = self.id {
            let mut state = global_parallelism();
            if let Some(target) = state.leave_guard(id) {
                set_global_parallelism(target);
            }
        }
    }
}

/// RAII scope that makes faer's process-global parallelism `Par::Seq` while
/// it is alive (issue #946).
///
/// Hold one around a loop that calls a faer sparse `solve_in_place` on every
/// step, such as a Krylov solve whose preconditioner does:
///
/// ```ignore
/// let _seq = SequentialSolveScope::enter();
/// ksp.solve(a, b, x, &precond)?; // ends here too, on the `?` return
/// ```
///
/// Take **one scope per solve, not one per preconditioner application**. The
/// global is a single atomic shared by every thread, so each change is a
/// window in which another thread's faer call sees the wrong value.
///
/// # Overlap with other scopes and with [`ParallelismGuard`]
///
/// Scopes and guards share one registry behind one mutex; the rule is on
/// [`ParallelismGuard`]. For a scope it means:
///
/// - The global is `Par::Seq` for as long as **any** scope is live on any
///   thread. Scopes are counted, so concurrent solves can end in any order
///   (the CLI's `--jobs`).
/// - A `ParallelismGuard` built or dropped on any thread while a scope is
///   live does not change the global. Its cap applies once the last scope
///   has ended, if the guard is still live then.
/// - When the last scope ends and no guard is live, the global returns to
///   the ambient value: what it was before the first guard or scope
///   started. That holds for every order in which overlapping scopes and
///   guards start and end.
///
/// The drop runs on early return and during panic unwinding.
///
/// # What it does not do
///
/// It does not isolate a thread. A faer factorization that another thread
/// starts while a scope is live runs sequentially, **with or without a
/// `ParallelismGuard` of its own**, until the scope ends. In this crate that
/// reaches concurrent solves only: no solver factors inside its own scope.
/// The result changes at roundoff at most, since the setting is how many
/// threads faer uses and not what it computes.
///
/// A direct [`faer::set_global_parallelism`] call made while a scope is live
/// is not coordinated: see the last point on [`ParallelismGuard`].
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
        let mut state = global_parallelism();
        let target = state.enter_scope(get_global_parallelism());
        set_global_parallelism(target);
        Self { _private: () }
    }
}

impl Drop for SequentialSolveScope {
    fn drop(&mut self) {
        let mut state = global_parallelism();
        if let Some(target) = state.leave_scope() {
            set_global_parallelism(target);
        }
    }
}

/// `Par::rayon(n)` when faer's `rayon` feature is enabled, else `None`.
///
/// faer re-exports the `Par::Rayon` variant only under its own `rayon`
/// feature. We cannot name `Par::rayon` unconditionally, so the two arms
/// below are selected by this crate's `faer-parallel` feature, which turns
/// faer's `rayon` feature on. `faer-parallel` is a default feature, so the
/// parallel arm is the one that compiles in normal builds.
#[cfg(feature = "faer-parallel")]
fn rayon_par(n: usize) -> Option<Par> {
    Some(Par::rayon(n))
}

#[cfg(not(feature = "faer-parallel"))]
fn rayon_par(_n: usize) -> Option<Par> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scopes alone, on plain values: the global is `Par::Seq` while any is
    /// live and the ambient value comes back only when the last one leaves.
    #[test]
    fn scopes_hand_back_the_ambient_value_only_when_the_last_one_leaves() {
        let ambient = Par::rayon(3);
        let mut state = GlobalParallelism::new();

        assert_eq!(state.enter_scope(ambient), Par::Seq);
        // A second scope starts while the global already reads Par::Seq. It
        // must not replace the ambient value with that.
        assert_eq!(state.enter_scope(Par::Seq), Par::Seq);
        assert_eq!(state.enter_scope(Par::Seq), Par::Seq);
        assert_eq!(state.leave_scope(), Some(Par::Seq), "two still live");
        assert_eq!(state.leave_scope(), Some(Par::Seq), "one still live");
        assert_eq!(state.leave_scope(), Some(ambient), "the last restores");

        // Fully released: an unbalanced extra `leave` sets nothing, and the
        // next scope is a first scope again with its own ambient value.
        assert_eq!(state.leave_scope(), None);
        assert_eq!(state.enter_scope(Par::Seq), Par::Seq);
        assert_eq!(state.leave_scope(), Some(Par::Seq));
        assert_eq!(state.ambient, None);
    }

    /// Guards alone: the cap built last applies, whatever the drop order, and
    /// the ambient value comes back after the last one.
    #[test]
    fn guards_apply_the_cap_built_last_in_any_drop_order() {
        let ambient = Par::rayon(5);
        let (two, four) = (Par::rayon(2), Par::rayon(4));
        let mut state = GlobalParallelism::new();

        // Innermost first: outer cap again, then ambient.
        let (outer, set) = state.enter_guard(two, ambient);
        assert_eq!(set, two);
        let (inner, set) = state.enter_guard(four, two);
        assert_eq!(set, four);
        assert_eq!(state.leave_guard(inner), Some(two));
        assert_eq!(state.leave_guard(outer), Some(ambient));

        // Outer first: the inner cap stays, then ambient. The value read
        // from the global on the second enter is not what gets restored.
        let (outer, _) = state.enter_guard(two, ambient);
        let (inner, _) = state.enter_guard(four, two);
        assert_eq!(state.leave_guard(outer), Some(four));
        assert_eq!(state.leave_guard(outer), None, "already unregistered");
        assert_eq!(state.leave_guard(inner), Some(ambient));
        assert!(state.guards.is_empty() && state.ambient.is_none());
    }

    /// Every interleaving of one scope and one guard: `Par::Seq` while the
    /// scope is live, the guard's cap when only the guard is, and the ambient
    /// value at the end. The third order is the one that used to end at
    /// `Par::Seq` (issue #946 review).
    #[test]
    fn a_scope_and_a_guard_end_at_the_ambient_value_in_every_order() {
        let ambient = Par::rayon(5);
        let cap = Par::rayon(3);

        // guard starts, scope starts, guard ends, scope ends
        let mut s = GlobalParallelism::new();
        let (g, set) = s.enter_guard(cap, ambient);
        assert_eq!(set, cap);
        assert_eq!(s.enter_scope(cap), Par::Seq);
        assert_eq!(s.leave_guard(g), Some(Par::Seq));
        assert_eq!(s.leave_scope(), Some(ambient));

        // guard starts, scope starts, scope ends, guard ends
        let mut s = GlobalParallelism::new();
        let (g, _) = s.enter_guard(cap, ambient);
        assert_eq!(s.enter_scope(cap), Par::Seq);
        assert_eq!(s.leave_scope(), Some(cap));
        assert_eq!(s.leave_guard(g), Some(ambient));

        // scope starts, guard starts, scope ends, guard ends
        let mut s = GlobalParallelism::new();
        assert_eq!(s.enter_scope(ambient), Par::Seq);
        let (g, set) = s.enter_guard(cap, Par::Seq);
        assert_eq!(set, Par::Seq, "a guard must not raise a live scope");
        assert_eq!(s.leave_scope(), Some(cap));
        assert_eq!(s.leave_guard(g), Some(ambient));

        // scope starts, guard starts, guard ends, scope ends
        let mut s = GlobalParallelism::new();
        assert_eq!(s.enter_scope(ambient), Par::Seq);
        let (g, _) = s.enter_guard(cap, Par::Seq);
        assert_eq!(s.leave_guard(g), Some(Par::Seq));
        assert_eq!(s.leave_scope(), Some(ambient));
        assert!(s.ambient.is_none());
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
