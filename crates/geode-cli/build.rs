//! Bake the git revision into the `geode` binary for `geode --version`
//! and the report `git_sha` provenance field (issue #673).
//!
//! Resolution order for `GEODE_GIT_SHA` (exported to the crate via
//! `cargo:rustc-env`):
//!
//! 1. `git rev-parse --short=12 HEAD` in the source tree, suffixed with
//!    `-dirty` when tracked files differ from `HEAD`;
//! 2. the `GEODE_GIT_SHA` build-time environment variable (for builds
//!    outside a git checkout, e.g. a vendored source tarball);
//! 3. the literal `unknown`.

use std::path::Path;
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn main() {
    println!("cargo:rerun-if-env-changed=GEODE_GIT_SHA");

    let sha = match git(&["rev-parse", "--short=12", "HEAD"]) {
        Some(sha) => {
            // Rebuild when HEAD moves or the index changes (dirty state).
            // Works for linked worktrees too: HEAD/index live in the
            // per-worktree git dir, branch refs in the common dir.
            if let Some(git_dir) = git(&["rev-parse", "--git-dir"]) {
                let git_dir = Path::new(&git_dir);
                println!("cargo:rerun-if-changed={}", git_dir.join("HEAD").display());
                println!("cargo:rerun-if-changed={}", git_dir.join("index").display());
            }
            if let (Some(common), Some(head_ref)) = (
                git(&["rev-parse", "--git-common-dir"]),
                git(&["rev-parse", "--symbolic-full-name", "HEAD"]),
            ) {
                let common = Path::new(&common);
                println!(
                    "cargo:rerun-if-changed={}",
                    common.join(&head_ref).display()
                );
                println!(
                    "cargo:rerun-if-changed={}",
                    common.join("packed-refs").display()
                );
            }
            let dirty = git(&["status", "--porcelain", "--untracked-files=no"]).is_some();
            if dirty { format!("{sha}-dirty") } else { sha }
        }
        None => std::env::var("GEODE_GIT_SHA")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
    };

    println!("cargo:rustc-env=GEODE_GIT_SHA={sha}");
}
