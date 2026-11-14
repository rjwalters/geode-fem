//! Structured box tet-mesh fixtures shared by tests across the workspace
//! (issues #1599, #1607).
//!
//! This is the one home for the `origin + len * i / n` box mesher that was
//! copied into seven integration tests. It lives in the library (behind the
//! `testing` feature) rather than in `geode-core/tests/common/` because
//! `geode-cli`'s own integration tests need it too, and a sibling crate
//! cannot reach another crate's `tests/` tree.
//!
//! The node formula is deliberately **not** the `i * (size / n)` formula of
//! `box_tet_mesh` in `geode-core/tests/common/periodic_fixtures.rs` — the two
//! can differ in the last ulp, and several callers assert tight tolerances or
//! finite-difference agreement. Unifying them is out of scope.

use crate::mesh::TetMesh;

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
/// [`box_mesh_with`] at `origin = [0.0; 3]` keeping every cell: adding `0.0`
/// leaves each coordinate bit-identical, and the compaction pass is the
/// identity when no cell is dropped, so this reproduces the former
/// two-argument `box_mesh` copies exactly.
pub fn box_mesh(n: [usize; 3], len: [f64; 3]) -> TetMesh {
    box_mesh_with(n, len, [0.0; 3], |_, _, _| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The keep-all / zero-origin wrapper is the identity on the filtered
    /// mesher — the property the former two-argument copies relied on for
    /// their golden values not to move.
    #[test]
    fn box_mesh_matches_unfiltered_box_mesh_with() {
        let a = box_mesh([3, 2, 4], [1.0, 0.8, 0.6]);
        let b = box_mesh_with([3, 2, 4], [1.0, 0.8, 0.6], [0.0; 3], |_, _, _| true);
        assert_eq!(a.nodes, b.nodes);
        assert_eq!(a.tets, b.tets);
    }

    /// Grid size and the 6-tet-per-hex split.
    #[test]
    fn unfiltered_box_has_full_grid_and_six_tets_per_hex() {
        let n = [3, 2, 4];
        let m = box_mesh(n, [1.0, 1.0, 1.0]);
        assert_eq!(m.nodes.len(), 4 * 3 * 5);
        assert_eq!(m.tets.len(), 6 * 3 * 2 * 4);
    }

    /// Node coordinates are exactly `origin + len * i / n`.
    #[test]
    fn node_formula_is_origin_plus_len_times_i_over_n() {
        let n = [2, 2, 2];
        let len = [1.0, 0.8, 0.6];
        let origin = [-1.0, -1.0, 0.0];
        let m = box_mesh_with(n, len, origin, |_, _, _| true);
        // Node (i, j, k) is at index i + j*np0 + k*np0*np1 with np = n + 1.
        let np = [n[0] + 1, n[1] + 1, n[2] + 1];
        for k in 0..np[2] {
            for j in 0..np[1] {
                for i in 0..np[0] {
                    let got = m.nodes[i + j * np[0] + k * np[0] * np[1]];
                    assert_eq!(got[0], origin[0] + len[0] * i as f64 / n[0] as f64);
                    assert_eq!(got[1], origin[1] + len[1] * j as f64 / n[1] as f64);
                    assert_eq!(got[2], origin[2] + len[2] * k as f64 / n[2] as f64);
                }
            }
        }
    }

    /// A `keep` filter drops the excluded cells and compacts away the nodes
    /// that no surviving tet references.
    #[test]
    fn keep_filter_drops_cells_and_compacts_nodes() {
        // Keep one cell of a 2x2x2 grid: 6 tets over the 8 corners of that hex.
        let m = box_mesh_with([2; 3], [1.0; 3], [0.0; 3], |i, j, k| {
            i == 0 && j == 0 && k == 0
        });
        assert_eq!(m.tets.len(), 6);
        assert_eq!(m.nodes.len(), 8);
        // Every index is in range and every node is referenced.
        let mut seen = vec![false; m.nodes.len()];
        for t in &m.tets {
            for &v in t {
                seen[v as usize] = true;
            }
        }
        assert!(seen.into_iter().all(|s| s));
    }
}
