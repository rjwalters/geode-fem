//! Sweep execution options for `geode driven` / `geode extract` (issue
//! #708): JSONL progress events on stderr (`--progress`) and parallel
//! frequency points (`--jobs`).
//!
//! # Progress events
//!
//! With `--progress`, the sweep writes one JSON object per line to
//! **stderr** as work completes (the report still goes to stdout / `-o`,
//! unchanged). Every event carries `event` (its type) and `elapsed_s`
//! (seconds since the sweep started); the types are:
//!
//! | `event` | extra fields | when |
//! |---|---|---|
//! | `sweep_start` | `command`, `method` (`"dense"` / `"adaptive"`), `n_frequencies`, `jobs` | once, before the first solve |
//! | `snapshot` | `frequency_hz`, `n_snapshots` | adaptive only: a greedy full-order snapshot solve finished |
//! | `point` | `index`, `frequency_hz`, `solved`, `residual_rel` | one per requested frequency, in **completion** order |
//! | `sweep_done` | `n_frequencies`, `n_solved`, `n_interpolated` | once, after the last point |
//!
//! `point.solved` is `true` for a full-order solve and `false` for a
//! frequency interpolated by the adaptive sweep's reduced-order model
//! (whose `residual_rel` is then the residual indicator). `index` is the
//! row in the report's `results`. Lines are written whole and flushed, so
//! a reader may parse them incrementally; with `--jobs > 1` the `point`
//! events arrive in completion order, not frequency order. `--outdir`
//! export solves emit no events.
//!
//! # Parallel frequency points
//!
//! `--jobs N` solves up to `N` frequencies of a dense sweep concurrently
//! (scoped worker threads pulling the next frequency index; results are
//! stored by index, so the report order is deterministic and identical
//! to `--jobs 1` at the same per-factorization thread count; see below).
//! Each in-flight frequency holds its **own** sparse LU
//! factorization (and, with `absorbing_regions`, its own assembled
//! operator), so peak memory grows roughly `N`-fold over the serial
//! sweep's per-frequency footprint — `geode check`'s `resources`
//! estimate is per frequency.
//!
//! The concurrent factorizations share one thread budget: while `N > 1`
//! workers run, faer's parallelism is capped at `T / N` threads per
//! factorization (at least 1), where `T` is `--threads` or, without it,
//! the core count. So `--jobs 4` on a 28-core machine runs four 7-thread
//! factorizations rather than four that each ask for 28 (issue #747).
//! `--jobs 1` is unchanged. faer's parallel LU differs across thread
//! counts only at roundoff, so `--jobs 2 --threads 4` is bit-identical
//! to `--jobs 1 --threads 2`.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use geode_core::eigen::parallel::{ParallelismGuard, resolve_num_threads};
use serde_json::{Map, Value, json};

use crate::error::CliError;

/// How to execute a driven / extract sweep.
#[derive(Debug, Clone, Copy)]
pub struct SweepOptions {
    /// Frequencies solved concurrently (`≥ 1`).
    pub jobs: usize,
    /// Whether `--jobs` was given (echoed in the report's
    /// `solver.jobs`).
    pub jobs_explicit: bool,
    /// Emit JSONL progress events on stderr.
    pub progress: bool,
}

impl Default for SweepOptions {
    fn default() -> Self {
        Self {
            jobs: 1,
            jobs_explicit: false,
            progress: false,
        }
    }
}

/// JSONL progress emitter (a no-op unless enabled).
pub struct Progress {
    enabled: bool,
    t0: Instant,
}

impl Progress {
    /// Start the sweep clock.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            t0: Instant::now(),
        }
    }

    /// Write one event line `{"event": event, "elapsed_s": …, fields…}`
    /// to stderr (whole line, flushed). `fields` must be a JSON object.
    pub fn emit(&self, event: &str, fields: Value) {
        if !self.enabled {
            return;
        }
        let mut obj = Map::new();
        obj.insert("event".into(), json!(event));
        obj.insert("elapsed_s".into(), json!(self.t0.elapsed().as_secs_f64()));
        if let Value::Object(extra) = fields {
            obj.extend(extra);
        }
        let mut line = Value::Object(obj).to_string();
        line.push('\n');
        let mut err = std::io::stderr().lock();
        // Progress is best effort: a closed stderr must not fail the solve.
        let _ = err.write_all(line.as_bytes()).and_then(|()| err.flush());
    }
}

/// `f(0), …, f(n−1)` with up to `jobs` concurrent workers, results in
/// index order. `jobs ≤ 1` (or `n ≤ 1`) runs serially on the calling
/// thread, stopping at the first error. In parallel, workers stop picking
/// up new indices after any failure and the **lowest-index** error among
/// the completed calls is returned (deterministic for a failure that does
/// not depend on scheduling).
///
/// In parallel, the faer factorization thread budget
/// ([`resolve_num_threads`]: `--threads`, else every core) is split
/// across the workers for the duration of the call — each concurrent
/// factorization gets `budget / workers` threads (at least 1, i.e.
/// serial) instead of every one claiming the whole budget (issue #747).
/// The serial path is untouched.
pub fn par_map<T: Send>(
    n: usize,
    jobs: usize,
    f: impl Fn(usize) -> Result<T, CliError> + Sync,
) -> Result<Vec<T>, CliError> {
    if jobs <= 1 || n <= 1 {
        return (0..n).map(f).collect();
    }
    let workers = jobs.min(n);
    let _par = ParallelismGuard::cap(per_job_threads(resolve_num_threads(), workers));
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let slots: Mutex<Vec<Option<Result<T, CliError>>>> = Mutex::new((0..n).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let r = f(i);
                    if r.is_err() {
                        failed.store(true, Ordering::Relaxed);
                    }
                    slots.lock().expect("par_map slot lock")[i] = Some(r);
                }
            });
        }
    });
    // Indices are handed out in increasing order, so every index below a
    // failed one was picked up and completed: the first error in index
    // order is the lowest-index failure. Without a failure every slot is
    // filled.
    slots
        .into_inner()
        .expect("par_map slot lock")
        .into_iter()
        .flatten()
        .collect()
}

/// Per-worker faer thread budget: `total / workers`, at least 1.
fn per_job_threads(total: usize, workers: usize) -> usize {
    (total / workers.max(1)).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn per_job_threads_splits_the_budget() {
        assert_eq!(per_job_threads(28, 4), 7);
        assert_eq!(per_job_threads(8, 3), 2);
        assert_eq!(per_job_threads(4, 8), 1);
        assert_eq!(per_job_threads(1, 1), 1);
        assert_eq!(per_job_threads(6, 0), 6);
    }

    #[test]
    fn par_map_preserves_order_for_any_job_count() {
        for jobs in [1, 2, 3, 8, 64] {
            let out = par_map(17, jobs, |i| Ok(i * i)).unwrap();
            assert_eq!(out, (0..17).map(|i| i * i).collect::<Vec<_>>());
        }
        assert!(par_map(0, 4, Ok).unwrap().is_empty());
    }

    #[test]
    fn par_map_returns_the_lowest_index_error() {
        for jobs in [1, 4] {
            let err = par_map(10, jobs, |i| {
                if i == 3 || i == 7 {
                    Err(CliError::InvalidSpec(format!("bad {i}")))
                } else {
                    Ok(i)
                }
            })
            .unwrap_err();
            // Serial stops at 3; parallel may also have computed 7, but
            // the lowest index wins.
            assert_eq!(err.to_string(), "invalid problem spec: bad 3");
        }
    }
}
