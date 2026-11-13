//! Shared structured-box mesh fixtures and the log-log slope fit used across
//! the `geode-core` integration tests (issue #1599).
//!
//! Pulled in with `mod common;` and then `use common::box_fixtures::…;`.
//!
//! The node formula here is `origin + len * i / n`, moved verbatim from the
//! six former per-file `box_mesh` copies. It is deliberately **not** the
//! `i * (size / n)` formula of `box_tet_mesh` in `common/periodic_fixtures.rs`
//! — the two can differ in the last ulp, and several callers assert tight
//! tolerances or finite-difference agreement. Unifying them is out of scope.
#![allow(dead_code)] // each test binary uses a subset

use geode_core::mesh::TetMesh;

use super::Lcg;
use super::boundary_plane_faces;

/// A structured box `origin + [0, len]` with `n` cells per axis, each hex
/// split into 6 tets, keeping the cells where `keep(i, j, k)` (unused nodes
/// dropped, relative order kept).
pub fn box_mesh_with(
    n: [usize; 3],
    len: [f64; 3],
    origin: [f64; 3],
    keep: impl Fn(usize, usize, usize) -> bool,
) -> TetMesh {
    let np = [n[0] + 1, n[1] + 1, n[2] + 1];
    let idx = |i: usize, j: usize, k: usize| (i + j * np[0] + k * np[0] * np[1]) as u32;
    let mut nodes = Vec::with_capacity(np[0] * np[1] * np[2]);
    for k in 0..np[2] {
        for j in 0..np[1] {
            for i in 0..np[0] {
                nodes.push([
                    origin[0] + len[0] * i as f64 / n[0] as f64,
                    origin[1] + len[1] * j as f64 / n[1] as f64,
                    origin[2] + len[2] * k as f64 / n[2] as f64,
                ]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                if !keep(i, j, k) {
                    continue;
                }
                let q = [
                    idx(i, j, k),
                    idx(i + 1, j, k),
                    idx(i + 1, j + 1, k),
                    idx(i, j + 1, k),
                    idx(i, j, k + 1),
                    idx(i + 1, j, k + 1),
                    idx(i + 1, j + 1, k + 1),
                    idx(i, j + 1, k + 1),
                ];
                tets.push([q[0], q[1], q[2], q[6]]);
                tets.push([q[0], q[2], q[3], q[6]]);
                tets.push([q[0], q[3], q[7], q[6]]);
                tets.push([q[0], q[7], q[4], q[6]]);
                tets.push([q[0], q[4], q[5], q[6]]);
                tets.push([q[0], q[5], q[1], q[6]]);
            }
        }
    }
    // Compact.
    let mut used = vec![false; nodes.len()];
    for t in &tets {
        for &v in t {
            used[v as usize] = true;
        }
    }
    let mut remap = vec![u32::MAX; nodes.len()];
    let mut kept = Vec::new();
    for (i, p) in nodes.iter().enumerate() {
        if used[i] {
            remap[i] = kept.len() as u32;
            kept.push(*p);
        }
    }
    for t in &mut tets {
        for v in t.iter_mut() {
            *v = remap[*v as usize];
        }
    }
    TetMesh {
        nodes: kept,
        tets,
        physical_groups: Default::default(),
    }
}

/// Structured box `[0, len]` with `n` cells per axis, 6 tets per hex.
///
/// `box_mesh_with` at `origin = [0.0; 3]` keeping every cell: adding `0.0`
/// leaves each coordinate bit-identical, and the compaction pass is the
/// identity when no cell is dropped, so this reproduces the former
/// two-argument `box_mesh` copies exactly.
pub fn box_mesh(n: [usize; 3], len: [f64; 3]) -> TetMesh {
    box_mesh_with(n, len, [0.0; 3], |_, _, _| true)
}

/// Jitter every node strictly inside the box by up to `amp` per axis
/// (the walls stay flat, and any node on the plane `keep_plane = (axis,
/// value)` stays on it).
pub fn jitter(
    mesh: &mut TetMesh,
    len: [f64; 3],
    amp: f64,
    seed: u64,
    keep_plane: Option<(usize, f64)>,
) {
    let mut rng = Lcg(seed);
    for p in mesh.nodes.iter_mut() {
        let inside = (0..3).all(|d| p[d] > 1e-12 && p[d] < len[d] - 1e-12);
        let (dx, dy, dz) = (rng.next(), rng.next(), rng.next());
        if !inside {
            continue;
        }
        let on_plane = keep_plane.is_some_and(|(axis, v)| (p[axis] - v).abs() < 1e-12);
        for (d, delta) in [dx, dy, dz].into_iter().enumerate() {
            if on_plane && keep_plane.is_some_and(|(axis, _)| axis == d) {
                continue;
            }
            p[d] += amp * delta;
        }
    }
}

/// The six walls of the box `[0, len]` as separate tagged triangle lists.
pub fn box_walls(mesh: &TetMesh, len: [f64; 3]) -> Vec<Vec<[u32; 3]>> {
    let mut out = Vec::new();
    for (axis, &l) in len.iter().enumerate() {
        out.push(boundary_plane_faces(mesh, axis, 0.0));
        out.push(boundary_plane_faces(mesh, axis, l));
    }
    out
}

/// Least-squares slope of `ln(errs)` against `ln(hs)` — the observed order
/// of convergence of a refinement sweep.
pub fn fit_slope(hs: &[f64], errs: &[f64]) -> f64 {
    let n = hs.len() as f64;
    let xs: Vec<f64> = hs.iter().map(|h| h.ln()).collect();
    let ys: Vec<f64> = errs.iter().map(|e| e.ln()).collect();
    let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
    let (mut num, mut den) = (0.0, 0.0);
    for (x, y) in xs.iter().zip(&ys) {
        num += (x - mx) * (y - my);
        den += (x - mx) * (x - mx);
    }
    num / den
}
