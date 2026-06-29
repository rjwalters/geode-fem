//! Repository-relative paths and git provenance, shared by the
//! validation tests and the example binaries.
//!
//! These helpers were previously copy-pasted as private `repo_root()`
//! functions in ~20 reference-test files and byte-identical
//! `current_commit()` functions in ~9 example binaries. Centralizing them
//! here removes the duplication and gives the examples a single
//! dependency-injected source for fixture paths and commit stamping.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Absolute path to the workspace root — the nearest ancestor of this
/// crate's source directory that contains a `reference/` directory.
///
/// Resolved from this crate's compile-time `CARGO_MANIFEST_DIR`, so it
/// returns the same workspace root whether it is called from a
/// geode-validation test or from an example binary that depends on this
/// crate (the manifest dir baked in is always `crates/geode-validation`).
///
/// # Panics
///
/// Panics if no ancestor contains a `reference/` directory — i.e. the
/// compiled artifact was moved away from its source checkout.
pub fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest.ancestors() {
        if ancestor.join("reference").is_dir() {
            return ancestor.to_path_buf();
        }
    }
    panic!(
        "could not find a `reference/` directory walking up from {}",
        manifest.display()
    );
}

/// Path to a fixture under `reference/fixtures/`, given its path relative
/// to that directory (e.g. `"sphere_pec/julia_baseline.json"`).
///
/// This is the canonical way to name a reference fixture or mesh; pair it
/// with [`crate::Fixture::load_json`] to load JSON fixtures in one call.
pub fn fixture_path(relative: impl AsRef<Path>) -> PathBuf {
    repo_root().join("reference/fixtures").join(relative)
}

/// The current git commit (`git rev-parse HEAD`), or `"unknown"` when git
/// is unavailable or the command fails.
///
/// Used to stamp provenance into regenerated benchmark / result files so
/// a committed artifact records the tree it was produced from.
pub fn current_commit() -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}
