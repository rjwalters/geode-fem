//! Small helpers shared by the transmon examples and tests via `#[path]`
//! includes (issue #1432), replacing per-file copies that had started to
//! drift.

#![allow(dead_code)]

use std::path::PathBuf;

use geode_core::mesh::TetMesh;

/// A copy of `mesh` with every node coordinate multiplied by `s`
/// (e.g. mesh units -> metres).
pub fn scaled_mesh(mesh: &TetMesh, s: f64) -> TetMesh {
    let mut m = mesh.clone();
    for n in m.nodes.iter_mut() {
        n[0] *= s;
        n[1] *= s;
        n[2] *= s;
    }
    m
}

/// Short (8-char) git HEAD of the repository, or `"unknown"` when git is
/// unavailable. Always queried from the repo root, independent of the cwd.
pub fn git_head() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}
