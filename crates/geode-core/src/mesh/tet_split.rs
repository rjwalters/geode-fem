//! Shared hex/prism → tet splitting helpers for the analytic mesh fixtures
//! ([`super::electrostatic_fixtures`], [`super::magnetostatic_fixtures`]).
//!
//! Both fixture families must use the *same* splits so that faces shared by
//! neighbouring prisms and hexes match; keeping one copy here guarantees it.

use crate::vec3::{cross3, dot3, sub3};

/// Split a hex cell (8 corner node indices in the standard VTK ordering) into
/// 6 tets sharing the `c[0]→c[6]` diagonal — the same split
/// [`crate::mesh::cube_tet_mesh`] uses. Reorders each tet to positive signed
/// volume using the actual node coordinates.
pub(crate) fn push_hex_tets(tets: &mut Vec<[u32; 4]>, c: &[u32; 8], nodes: &[[f64; 3]]) {
    const SPLIT: [[usize; 4]; 6] = [
        [0, 1, 2, 6],
        [0, 2, 3, 6],
        [0, 3, 7, 6],
        [0, 7, 4, 6],
        [0, 4, 5, 6],
        [0, 5, 1, 6],
    ];
    for s in &SPLIT {
        tets.push(oriented_tet([c[s[0]], c[s[1]], c[s[2]], c[s[3]]], nodes));
    }
}

/// Split a triangular prism (bottom triangle `b0,b1,b2`; top `t0,t1,t2`) into
/// 3 tets. Input order: `[b0, b1, b2, t0, t1, t2]`. Reorients each tet to
/// positive signed volume.
pub(crate) fn push_prism_tets(tets: &mut Vec<[u32; 4]>, p: [u32; 6], nodes: &[[f64; 3]]) {
    let [b0, b1, b2, t0, t1, t2] = p;
    let split = [[b0, b1, b2, t2], [b0, b1, t2, t1], [b0, t1, t2, t0]];
    for s in &split {
        tets.push(oriented_tet(*s, nodes));
    }
}

/// Return the tet with its last two vertices swapped if needed so the signed
/// volume is positive.
fn oriented_tet(t: [u32; 4], nodes: &[[f64; 3]]) -> [u32; 4] {
    let v0 = nodes[t[0] as usize];
    let e1 = sub3(nodes[t[1] as usize], v0);
    let e2 = sub3(nodes[t[2] as usize], v0);
    let e3 = sub3(nodes[t[3] as usize], v0);
    let det = dot3(e1, cross3(e2, e3));
    if det < 0.0 {
        [t[0], t[1], t[3], t[2]]
    } else {
        t
    }
}
