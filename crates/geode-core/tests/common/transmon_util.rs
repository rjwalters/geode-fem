//! Small helpers shared by the transmon examples and tests via `#[path]`
//! includes (issue #1432), replacing per-file copies that had started to
//! drift.

#![allow(dead_code)]

use std::path::PathBuf;

use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
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

/// Read a `[nnz]` Burn value tensor to a host `Vec<f64>`.
pub fn vals_to_host<B: Backend>(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

/// Split `mesh.tet_edges()` into the per-tet `(idx, sign)` tables the
/// assemblers take.
pub fn edge_tables(mesh: &TetMesh) -> (Vec<[u32; 6]>, Vec<[i8; 6]>) {
    let te = mesh.tet_edges();
    (
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect(),
        te.iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect(),
    )
}

/// Format a slice as `[a, b, c]` with `prec` decimal places.
pub fn arr(v: &[f64], prec: usize) -> String {
    format!(
        "[{}]",
        v.iter()
            .map(|x| format!("{x:.prec$}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Format `x` rounded *down* to `dp` decimal places (conservative bracket
/// endpoints).
pub fn floor_fmt(x: f64, dp: usize) -> String {
    let p = 10f64.powi(dp as i32);
    format!("{:.dp$}", (x * p).floor() / p)
}

/// Six times the signed volume of the tetrahedron with vertices `v`.
pub fn signed_det6(v: &[[f64; 3]; 4]) -> f64 {
    let e1 = [v[1][0] - v[0][0], v[1][1] - v[0][1], v[1][2] - v[0][2]];
    let e2 = [v[2][0] - v[0][0], v[2][1] - v[0][1], v[2][2] - v[0][2]];
    let e3 = [v[3][0] - v[0][0], v[3][1] - v[0][1], v[3][2] - v[0][2]];
    e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) - e1[1] * (e2[0] * e3[2] - e2[2] * e3[0])
        + e1[2] * (e2[0] * e3[1] - e2[1] * e3[0])
}

/// Look up a dotted key path (`"a.b.c"`) in a TOML document, panicking with
/// the missing segment when absent.
pub fn tget<'a>(v: &'a toml::Value, path: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur
}

/// Read a TOML array of numbers (floats or integers) at a dotted key path.
pub fn tarr(v: &toml::Value, path: &str) -> Vec<f64> {
    tget(v, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| {
            x.as_float()
                .or_else(|| x.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

/// Read a TOML number (float or integer) at a dotted key path.
pub fn tf(v: &toml::Value, path: &str) -> f64 {
    let x = tget(v, path);
    x.as_float()
        .or_else(|| x.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

/// Read a TOML string at a dotted key path.
pub fn tstr<'a>(v: &'a toml::Value, path: &str) -> &'a str {
    tget(v, path)
        .as_str()
        .unwrap_or_else(|| panic!("{path} is not a string"))
}
