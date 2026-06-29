//! Repo-relative paths and git provenance, shared by the validation tests
//! and the example binaries.
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
/// crate (the manifest dir baked in is always `crates/geode-util`).
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
/// with a fixture loader (e.g. `geode_validation::Fixture::load_json`) to
/// load JSON fixtures in one call.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_root_is_absolute_and_contains_reference() {
        let root = repo_root();
        assert!(
            root.is_absolute(),
            "repo_root must be an absolute path, got {}",
            root.display()
        );
        assert!(
            root.join("reference").is_dir(),
            "repo_root {} must contain a reference/ directory",
            root.display()
        );
    }

    #[test]
    fn repo_root_is_stable_across_calls() {
        // Resolved from a compile-time constant, so repeated calls must
        // return byte-identical paths.
        assert_eq!(repo_root(), repo_root());
    }

    #[test]
    fn fixture_path_is_rooted_under_reference_fixtures() {
        let p = fixture_path("sphere_pec/julia_baseline.json");
        assert!(
            p.starts_with(repo_root()),
            "fixture path must live under the repo root"
        );
        assert!(
            p.ends_with("reference/fixtures/sphere_pec/julia_baseline.json"),
            "unexpected fixture path tail: {}",
            p.display()
        );
    }

    #[test]
    fn fixture_path_joins_relative_input_onto_fixtures_dir() {
        // A bare relative file name lands directly in reference/fixtures/.
        let p = fixture_path("baseline.json");
        assert_eq!(
            p,
            repo_root().join("reference/fixtures").join("baseline.json")
        );
        assert!(p.is_absolute());
    }

    #[test]
    fn fixture_path_accepts_path_and_pathbuf_inputs() {
        // `impl AsRef<Path>` must accept the common owned/borrowed forms,
        // all yielding the same join.
        let from_str = fixture_path("a/b.json");
        let from_path = fixture_path(Path::new("a/b.json"));
        let from_pathbuf = fixture_path(PathBuf::from("a/b.json"));
        assert_eq!(from_str, from_path);
        assert_eq!(from_str, from_pathbuf);
    }

    #[test]
    fn fixture_path_with_empty_relative_is_the_fixtures_dir() {
        // Joining an empty relative component is a no-op, so the result is
        // exactly the fixtures directory itself.
        assert_eq!(fixture_path(""), repo_root().join("reference/fixtures"));
    }

    #[test]
    fn current_commit_is_nonempty_and_trimmed() {
        let commit = current_commit();
        assert!(!commit.is_empty(), "current_commit must never be empty");
        assert_eq!(
            commit,
            commit.trim(),
            "current_commit must be trimmed of surrounding whitespace"
        );
    }

    #[test]
    fn current_commit_is_either_a_sha_or_the_unknown_fallback() {
        // Deterministic shape check that holds whether or not the test
        // host has git / is inside a work tree: the result is either the
        // literal "unknown" fallback or a 40-char lowercase-or-upper hex
        // SHA-1. We never assert a specific commit.
        let commit = current_commit();
        let is_unknown = commit == "unknown";
        let is_sha = commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit());
        assert!(
            is_unknown || is_sha,
            "current_commit must be \"unknown\" or a 40-char hex SHA, got {commit:?}"
        );
    }
}
