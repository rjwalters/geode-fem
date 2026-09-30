//! Optional scikit-rf read of a real `geode driven --touchstone` `.s1p`
//! (issue #713): the smoke spiral at two frequencies (spec order
//! descending, file ascending), loaded by an independent Touchstone
//! reader and compared value-for-value against the JSON report. The
//! 2- and 5-port layouts are covered from `render` directly in the
//! `src/touchstone.rs` unit tests.
//!
//! scikit-rf is not a CI dependency: without a Python that can
//! `import skrf` (`GEODE_SKRF`, else `python3`) the check is skipped with
//! a loud `SKIPPED` line on stderr. The geode-side round trip (our own
//! test-side parser) always runs.

use std::path::PathBuf;
use std::process::Command;

#[path = "support/skrf.rs"]
mod skrf_support;
#[path = "support/touchstone.rs"]
mod touchstone_support;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn scikit_rf_reads_a_driven_s1p() {
    let fixtures = manifest_dir().join("tests/fixtures");
    let raw = std::fs::read_to_string(fixtures.join("spiral_golden_smoke.json")).unwrap();
    let mut v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures.join(v["mesh"]["path"].as_str().unwrap());
    v["mesh"]["path"] = mesh.display().to_string().into();
    v["frequencies"] = serde_json::json!({ "unit": "ghz", "values": [5.0, 1.0] });
    let dir = std::env::temp_dir().join(format!("geode-skrf-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let spec = dir.join("spec.json");
    std::fs::write(&spec, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    let ts = dir.join("spiral.s1p");
    let out = Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(["driven", spec.to_str().unwrap(), "--touchstone"])
        .arg(&ts)
        .output()
        .expect("spawn geode");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    touchstone_support::assert_round_trip(&report, &ts);
    let (_, refs, rows) = touchstone_support::parse(&std::fs::read_to_string(&ts).unwrap());
    skrf_support::check("geode driven .s1p", &ts, &refs, &rows);
    std::fs::remove_dir_all(&dir).unwrap();
}
