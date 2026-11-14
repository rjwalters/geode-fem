//! Shared helpers for the `geode` CLI integration tests (issue #1526).
//!
//! Include with `#[path = "support/cli.rs"] mod cli_support;`. The
//! `env!` macros expand in the including test crate, so
//! `CARGO_BIN_EXE_geode` and `CARGO_MANIFEST_DIR` resolve exactly as they
//! did when each test file carried its own copy.
#![allow(dead_code)] // each test binary uses a different subset

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Run the `geode` binary with `args` and capture its output.
pub fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// The `geode-cli` crate's manifest directory.
pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// A JSON value as `f64`, panicking with the value if it is not a number.
pub fn f64_at(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

/// The report JSON on `out`'s stdout, panicking with both streams if it is
/// not JSON.
pub fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Require a successful run and return its report JSON; `what` names the
/// run in the panic message.
pub fn ok(out: &Output, what: &str) -> Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    serde_json::from_slice(&out.stdout).expect("report JSON")
}

/// [`ok`], additionally asserting the report's `status` field is `"ok"`.
pub fn ok_status(out: &Output, what: &str) -> serde_json::Value {
    assert!(
        out.status.success(),
        "{what} failed ({}):\nstderr: {}\nstdout: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let v = json(out);
    assert_eq!(v["status"], "ok");
    v
}

/// `fixture` (under `tests/fixtures`) with its mesh path made absolute and
/// `edit` applied, written to `dir/name`.
pub fn spec_from(fixture: &str, dir: &Path, name: &str, edit: impl FnOnce(&mut Value)) -> PathBuf {
    let fixtures = manifest_dir().join("tests/fixtures");
    let raw = std::fs::read_to_string(fixtures.join(fixture)).unwrap();
    let mut v: Value = serde_json::from_str(&raw).unwrap();
    let mesh = fixtures
        .join(v["mesh"]["path"].as_str().unwrap())
        .canonicalize()
        .expect("fixture mesh");
    v["mesh"]["path"] = mesh.display().to_string().into();
    edit(&mut v);
    let path = dir.join(name);
    std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap()).unwrap();
    path
}

/// Assert the report's `spice_file` names `path` and carries its SHA-256;
/// return the file's text.
pub fn assert_file_ref(report: &serde_json::Value, path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let sf = &report["spice_file"];
    assert_eq!(sf["path"], path.display().to_string());
    assert_eq!(
        sf["sha256"],
        Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    text
}

/// Run `geode <sub>` on the JSON `spec` (written into the scratch `dir`)
/// and return the error report's `(code, message)`; asserts a non-zero
/// exit. Callers bind their own scratch directory — see the `run_err`
/// shims in `capacitance_golden.rs` / `inductance_golden.rs`.
pub fn run_err_in(dir: &Path, sub: &str, name: &str, spec: serde_json::Value) -> (String, String) {
    let path = dir.join("spec.json");
    std::fs::write(&path, spec.to_string()).unwrap();
    let out = geode(&[sub, path.to_str().unwrap()]);
    assert!(!out.status.success(), "{name}: expected failure");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("error report");
    assert_eq!(v["kind"], "error");
    (
        v["error"]["code"].as_str().unwrap().to_string(),
        v["error"]["message"].as_str().unwrap().to_string(),
    )
}
