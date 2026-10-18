//! Focused checks for the standalone benchmark resource sampler (#1202).
#[path = "common/motor_resources.rs"]
mod resources;

use resources::{Counters, Snapshot, interval, rss_bytes, snapshot};

#[test]
fn units_are_platform_specific_and_unknown_units_stay_unavailable() {
    assert_eq!(rss_bytes(1234, "macos"), (Some(1234), "bytes"));
    assert_eq!(rss_bytes(1234, "linux"), (Some(1234 * 1024), "KiB"));
    assert_eq!(rss_bytes(1234, "unverified"), (None, "unknown"));
    assert_eq!(rss_bytes(-1, "linux"), (None, "KiB"));
    assert_eq!(rss_bytes(i64::MAX, "linux"), (None, "KiB"));
}

fn fixture(user: f64, system: f64, child_user: f64, child_system: f64) -> Snapshot {
    Snapshot {
        pid: 123,
        at: std::time::Instant::now(),
        own: Ok(Counters {
            user_seconds: user,
            system_seconds: system,
            maxrss_raw: 1000,
        }),
        reaped_children: Ok(Counters {
            user_seconds: child_user,
            system_seconds: child_system,
            maxrss_raw: 2000,
        }),
    }
}

#[test]
fn cpu_is_differenced_once_and_rss_is_neither_subtracted_nor_summed() {
    let before = fixture(2.0, 1.0, 4.0, 3.0);
    let after = fixture(3.0, 1.5, 6.0, 3.25);
    let r = interval(&before, &after, "fixture", true, None);
    assert_eq!(r["cpu"]["self"]["total_seconds"], 1.5);
    assert_eq!(r["cpu"]["reaped_children"]["total_seconds"], 2.25);
    assert_eq!(r["observed_cpu_seconds"], 3.75);
    assert_eq!(r["cpu_complete"], true);
    assert_eq!(
        r["rss"]["self"].as_u64(),
        rss_bytes(1000, std::env::consts::OS).0
    );
    assert_eq!(
        r["rss"]["reaped_children"].as_u64(),
        rss_bytes(2000, std::env::consts::OS).0
    );
    assert!(r["rss"].get("total").is_none());
}

#[test]
fn interrupted_and_invalid_intervals_do_not_claim_complete_accounting() {
    let before = fixture(2.0, 1.0, 4.0, 3.0);
    let mut after = fixture(3.0, 1.5, 6.0, 3.25);
    let r = interval(
        &before,
        &after,
        "aborted",
        false,
        Some("unreaped descendants"),
    );
    assert_eq!(r["observed_cpu_seconds"], 3.75);
    assert_eq!(r["cpu_complete"], false);
    assert_eq!(r["incomplete_reason"], "unreaped descendants");
    after.pid += 1;
    let r = interval(&before, &after, "wrong process", true, None);
    assert!(r["observed_cpu_seconds"].is_null());
    assert_eq!(r["cpu_complete"], false);
    after.pid = before.pid;
    after.own = Err("fixture unavailable".to_string());
    assert!(
        interval(&before, &after, "failed syscall", true, None)["observed_cpu_seconds"].is_null()
    );
}

#[test]
#[cfg(not(unix))]
fn unsupported_platform_reports_unavailable() {
    let before = snapshot();
    let after = snapshot();
    let r = interval(&before, &after, "unsupported platform", true, None);
    assert_eq!(r["after"]["self"]["available"], false);
    assert_eq!(r["after"]["reaped_children"]["available"], false);
    assert!(r["observed_cpu_seconds"].is_null());
    assert_eq!(r["cpu_complete"], false);
}

#[test]
#[cfg(unix)]
fn live_self_counters_and_waited_child_cpu_are_observed() {
    let before = snapshot();
    let mut value = 1_u64;
    let start = std::time::Instant::now();
    while start.elapsed().as_millis() < 20 {
        value = std::hint::black_box(value.wrapping_mul(6364136223846793005).wrapping_add(1));
    }
    std::hint::black_box(value);
    let child = std::process::Command::new("python3")
        .args([
            "-I",
            "-c",
            "import time; end=time.process_time()+.03\nwhile time.process_time()<end: pass",
        ])
        .status()
        .expect("Python child fixture");
    assert!(child.success());
    let after = snapshot();
    let r = interval(&before, &after, "isolated process fixture", true, None);
    assert_eq!(r["cpu_complete"], true);
    assert!(r["cpu"]["self"]["total_seconds"].as_f64().unwrap() > 0.0);
    assert!(
        r["cpu"]["reaped_children"]["total_seconds"]
            .as_f64()
            .unwrap()
            >= 0.03
    );
}
