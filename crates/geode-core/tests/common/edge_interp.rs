//! Shared Nédélec/Whitney edge interpolant and its 4-point Gauss–Legendre
//! quadrature table (issue #1623).
//!
//! The lowest-order Nédélec edge DOF of a field `E` is the tangential line
//! integral over the mesh edge, taken from the lower-tagged to the
//! higher-tagged global node (the canonical edge direction of
//! `TetMesh::edges`):
//!
//! ```text
//! x_a[e] = ∫_e E · dl ≈ Σ_i w_i (E(p + s_i dl) · dl),   dl = q − p.
//! ```
//!
//! Four integration tests built this same integral — and hand-typed the same
//! 15-significant-digit quadrature rule — independently. A typo in one copy
//! of that table neither fails to compile nor contradicts the other three; it
//! silently shifts the reference a convergence-order assertion is measured
//! against, in one test binary only. Hence one definition here.
//!
//! Pulled in with `#[path = "common/edge_interp.rs"] mod edge_interp;`.
#![allow(dead_code)] // each test binary uses a subset

use faer::c64;

use geode_core::mesh::TetMesh;

/// 4-point Gauss–Legendre nodes/weights on `[0, 1]` (exact through degree 7).
pub const GL4: [(f64, f64); 4] = [
    (0.5 - 0.430_568_155_797_026, 0.173_927_422_568_727),
    (0.5 - 0.169_990_521_792_428, 0.326_072_577_431_273),
    (0.5 + 0.169_990_521_792_428, 0.326_072_577_431_273),
    (0.5 + 0.430_568_155_797_026, 0.173_927_422_568_727),
];

/// Edge interpolant `∫_e E · dl` of a complex vector field, lower → higher
/// global node, for every edge of `mesh` in `TetMesh::edges` order.
///
/// `e` is sampled at the four Gauss–Legendre points of each edge. The full
/// `E · dl` contraction is taken **inside** the accumulation loop, so a field
/// that is identically zero along a component contributes exact zeros rather
/// than being special-cased away.
///
/// That differs from hoisting `dl` out of the sum — `(Σ_i w_i ψ_i) dz`, which
/// three of the callers consolidated here used to do — only by the rounding of
/// the scaling. The two are bit-identical whenever `dl` is a power of two, as
/// it is on every structured mesh these tests build, and otherwise differ by
/// at most ~2 ulp.
pub fn edge_interpolant(mesh: &TetMesh, e: impl Fn([f64; 3]) -> [c64; 3]) -> Vec<c64> {
    mesh.edges()
        .iter()
        .map(|ed| {
            let p = mesh.nodes[ed[0] as usize];
            let q = mesh.nodes[ed[1] as usize];
            let dl = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
            let mut acc = c64::new(0.0, 0.0);
            for &(s, w) in &GL4 {
                let x = [p[0] + s * dl[0], p[1] + s * dl[1], p[2] + s * dl[2]];
                let v = e(x);
                acc += (v[0] * dl[0] + v[1] * dl[1] + v[2] * dl[2]) * w;
            }
            acc
        })
        .collect()
}

/// [`edge_interpolant`] for a real-valued field.
///
/// A thin wrapper so the real callers need not carry `c64` through their
/// analytic expressions. The imaginary channel is exactly zero at every
/// quadrature point, so taking `.re` is lossless rather than a truncation.
pub fn edge_interpolant_re(mesh: &TetMesh, e: impl Fn([f64; 3]) -> [f64; 3]) -> Vec<f64> {
    edge_interpolant(mesh, |x| e(x).map(|a| c64::new(a, 0.0)))
        .into_iter()
        .map(|z| z.re)
        .collect()
}
