//! Shared structured-box mesh fixtures and the log-log slope fit used across
//! the `geode-core` integration tests (issues #1599, #1607).
//!
//! Pulled in with `mod common;` and then `use common::box_fixtures::…;`.
//!
//! The box mesher itself now lives in the library at
//! [`geode_core::testing::mesh`] and is re-exported here, so the existing
//! `use common::box_fixtures::box_mesh_with;` call sites keep working. It had
//! to move out of `tests/common/` because `geode-cli`'s integration tests
//! need the same mesher and cannot reach another crate's `tests/` tree.
// Each test binary uses a subset — of the definitions below (`dead_code`) and
// of the re-exports above (`unused_imports`).
#![allow(dead_code, unused_imports)]

use geode_core::mesh::TetMesh;

pub use geode_core::testing::mesh::{box_mesh, box_mesh_with};

use super::Lcg;
use super::boundary_plane_faces;

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
