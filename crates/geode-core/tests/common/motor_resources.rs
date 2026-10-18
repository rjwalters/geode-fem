//! Standalone process accounting for the motor benchmark (#1202).
//!
//! Not yet connected to evaluator requests. Each process samples its own
//! counters. Child CPU includes only descendants accounted for after waiting;
//! killing a process group may leave that total incomplete. Nested request
//! totals must not be added again to an inclusive run total. RSS observations
//! are lifetime high-water statistics, never interval deltas or tree peaks.

use serde_json::{Value, json};
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct Counters {
    pub user_seconds: f64,
    pub system_seconds: f64,
    pub maxrss_raw: i64,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub pid: u32,
    pub at: Instant,
    pub own: Result<Counters, String>,
    pub reaped_children: Result<Counters, String>,
}

/// Normalize only the two supported benchmark host families; retain raw units.
pub fn rss_bytes(raw: i64, os: &str) -> (Option<u64>, &'static str) {
    let nonnegative = u64::try_from(raw).ok();
    match os {
        "macos" => (nonnegative, "bytes"),
        "linux" => (nonnegative.and_then(|v| v.checked_mul(1024)), "KiB"),
        _ => (None, "unknown"),
    }
}

#[cfg(unix)]
fn read(who: nix::sys::resource::UsageWho) -> Result<Counters, String> {
    let r = nix::sys::resource::getrusage(who).map_err(|e| e.to_string())?;
    let seconds = |t: nix::sys::time::TimeVal| t.tv_sec() as f64 + t.tv_usec() as f64 / 1e6;
    Ok(Counters {
        user_seconds: seconds(r.user_time()),
        system_seconds: seconds(r.system_time()),
        maxrss_raw: r.max_rss(),
    })
}

/// Capture self and reaped children without changing the running computation.
pub fn snapshot() -> Snapshot {
    #[cfg(unix)]
    let (own, reaped_children) = {
        use nix::sys::resource::UsageWho;
        (read(UsageWho::RUSAGE_SELF), read(UsageWho::RUSAGE_CHILDREN))
    };
    #[cfg(not(unix))]
    let (own, reaped_children) = (
        Err("getrusage unavailable on this platform".to_string()),
        Err("getrusage unavailable on this platform".to_string()),
    );
    Snapshot {
        pid: std::process::id(),
        at: Instant::now(),
        own,
        reaped_children,
    }
}

fn raw(c: &Result<Counters, String>) -> Value {
    match c {
        Ok(c) => {
            let (bytes, unit) = rss_bytes(c.maxrss_raw, std::env::consts::OS);
            json!({"available": true, "user_seconds": c.user_seconds,
                "system_seconds": c.system_seconds, "maxrss_raw": c.maxrss_raw,
                "maxrss_raw_unit": unit, "maxrss_bytes": bytes})
        }
        Err(e) => json!({"available": false, "error": e}),
    }
}

fn cpu_delta(a: &Result<Counters, String>, b: &Result<Counters, String>) -> Option<Value> {
    let (Ok(a), Ok(b)) = (a, b) else {
        return None;
    };
    let user = b.user_seconds - a.user_seconds;
    let system = b.system_seconds - a.system_seconds;
    if ![user, system].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return None;
    }
    Some(json!({"user_seconds": user, "system_seconds": system,
                "total_seconds": user + system}))
}

/// Describe measured CPU and end-of-phase RSS with explicit completeness.
///
/// `children_complete` must come from knowledge of the owned process lifecycle,
/// not merely from a successful syscall. Errors retain raw evidence and produce
/// unavailable CPU; they do not become an optimizer acceptance condition.
pub fn interval(
    before: &Snapshot,
    after: &Snapshot,
    label: &str,
    children_complete: bool,
    incomplete_reason: Option<&str>,
) -> Value {
    let identity_ok = before.pid == after.pid && after.at >= before.at;
    let own = identity_ok
        .then(|| cpu_delta(&before.own, &after.own))
        .flatten();
    let children = identity_ok
        .then(|| cpu_delta(&before.reaped_children, &after.reaped_children))
        .flatten();
    let total = own
        .as_ref()
        .zip(children.as_ref())
        .and_then(|(a, b)| Some(a["total_seconds"].as_f64()? + b["total_seconds"].as_f64()?));
    let error = if !identity_ok {
        Some("snapshots differ in process identity or time order")
    } else if total.is_none() {
        Some("CPU counters unavailable, nonfinite or decreased")
    } else {
        None
    };
    json!({
        "label": label, "source": "nix::getrusage", "platform": std::env::consts::OS,
        "before": {"pid": before.pid, "self": raw(&before.own),
                   "reaped_children": raw(&before.reaped_children)},
        "after": {"pid": after.pid, "self": raw(&after.own),
                  "reaped_children": raw(&after.reaped_children)},
        "cpu": {"self": own, "reaped_children": children},
        "observed_cpu_seconds": total,
        "children_complete": children_complete,
        "cpu_complete": children_complete && error.is_none(),
        "incomplete_reason": if children_complete { None } else {
            Some(incomplete_reason.unwrap_or("descendant accounting not established"))
        },
        "error": error,
        "rss": {"self": raw(&after.own)["maxrss_bytes"],
                "reaped_children": raw(&after.reaped_children)["maxrss_bytes"],
                "semantics": "separate lifetime high-water observations in bytes; not interval deltas or a process-tree peak"},
        "aggregation": "self plus reaped children once; do not add nested request totals"
    })
}
