//! Shared helpers for the `geode` CLI integration tests (issue #1526).
//!
//! Include with `#[path = "support/cli.rs"] mod cli_support;`. The
//! `env!` macros expand in the including test crate, so
//! `CARGO_BIN_EXE_geode` and `CARGO_MANIFEST_DIR` resolve exactly as they
//! did when each test file carried its own copy.
#![allow(dead_code)] // each test binary uses a different subset

use std::path::PathBuf;
use std::process::{Command, Output};

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
