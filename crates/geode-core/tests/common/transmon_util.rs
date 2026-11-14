//! Small helpers shared by the transmon examples and tests via `#[path]`
//! includes (issue #1432), replacing per-file copies that had started to
//! drift.

#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use burn::tensor::Tensor;
use burn::tensor::backend::Backend;
use geode_core::assembly::electrostatic::tet_bary_grads;
use geode_core::mesh::{TaggedTetMesh, TetMesh, TransmonFixture};

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

/// A copy of `base` with every node displaced by `sum_k theta[k] * fields[k]`
/// — the geometry map of the transmon shape-optimization examples.
pub fn moved(base: &TetMesh, fields: &[Vec<[f64; 3]>], theta: &[f64; 3]) -> TetMesh {
    let mut mesh = base.clone();
    for (i, x) in mesh.nodes.iter_mut().enumerate() {
        for (field, step) in fields.iter().zip(theta) {
            for (d, xd) in x.iter_mut().enumerate() {
                *xd += step * field[i][d];
            }
        }
    }
    mesh
}

/// Rebuild a `TransmonFixture` around a refined/moved `TaggedTetMesh`,
/// carrying `original`'s physical tags across.
pub fn fixture(tagged: &TaggedTetMesh, original: &TransmonFixture) -> TransmonFixture {
    TransmonFixture {
        mesh: tagged.mesh.clone(),
        tags: original.tags,
        tet_physical_tags: tagged.tet_physical_tags.clone(),
        boundary_triangles: tagged.boundary_triangles.clone(),
        triangle_physical_tags: tagged.triangle_physical_tags.clone(),
    }
}

/// Flux-jump marking, excluding the actual conductor sheets where a normal
/// displacement jump is physical. This is a heuristic indicator; the acceptance
/// test is a separate solve on a more resolved discretization, not its value.
pub fn indicators(fx: &TransmonFixture, phi: &[f64]) -> Vec<f64> {
    let mesh = &fx.mesh;
    let eps = fx.epsilon_r_scalar();
    let mut pec = BTreeSet::new();
    for mut tri in fx.metal_triangles() {
        tri.sort_unstable();
        pec.insert(tri);
    }
    let mut faces: HashMap<[u32; 3], (usize, [f64; 3], f64)> = HashMap::new();
    let mut eta = vec![0.0; mesh.n_tets()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let mut flux = [0.0; 3];
        for a in 0..4 {
            for (d, f) in flux.iter_mut().enumerate() {
                *f += eps[t] * phi[tet[a] as usize] * grads[a][d];
            }
        }
        for local in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let mut key = local.map(|a| tet[a]);
            key.sort_unstable();
            if pec.contains(&key) {
                continue;
            }
            let p = key.map(|v| mesh.nodes[v as usize]);
            let a: [f64; 3] = std::array::from_fn(|d| p[1][d] - p[0][d]);
            let b: [f64; 3] = std::array::from_fn(|d| p[2][d] - p[0][d]);
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            if let Some((other, f, e)) = faces.remove(&key) {
                let jump: f64 = (0..3).map(|d| (flux[d] - f[d]) * n[d]).sum();
                let area2 = n.iter().map(|x| x * x).sum::<f64>().sqrt();
                let value = jump * jump / area2.sqrt() / eps[t].max(e);
                eta[t] += value;
                eta[other] += value;
            } else {
                faces.insert(key, (t, flux, eps[t]));
            }
        }
    }
    eta
}

/// Exact nodal prolongation of the geometry-map fields to a refined mesh.
pub fn prolong(
    fields: &[Vec<[f64; 3]>],
    p: faer::sparse::SparseColMatRef<'_, usize, f64>,
) -> Vec<Vec<[f64; 3]>> {
    fields
        .iter()
        .map(|field| {
            let mut fine = vec![[0.0; 3]; p.nrows()];
            for (col, value) in field.iter().enumerate() {
                for q in p.col_ptr()[col]..p.col_ptr()[col + 1] {
                    let row = p.row_idx()[q];
                    for (d, &v) in value.iter().enumerate() {
                        fine[row][d] += p.val()[q] * v;
                    }
                }
            }
            fine
        })
        .collect()
}

/// Mark the largest indicators carrying half of the indicator sum.
///
/// Deliberately *not* `geode_core::adapt::driver::dorfler_mark`: that rule also
/// marks every tet tied with the last marked one, skips zero indicators and
/// applies a relative tolerance on the threshold, so it can mark a different
/// set and change the refined meshes the transmon examples' published numbers
/// reproduce (issue #1582).
pub fn dorfler(eta: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..eta.len()).collect();
    order.sort_by(|&a, &b| eta[b].total_cmp(&eta[a]));
    let threshold = 0.5 * eta.iter().sum::<f64>();
    let mut sum = 0.0;
    let mut marked = Vec::new();
    for t in order {
        marked.push(t);
        sum += eta[t];
        if sum >= threshold {
            break;
        }
    }
    marked
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
