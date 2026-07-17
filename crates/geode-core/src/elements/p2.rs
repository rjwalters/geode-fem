//! P2 (quadratic Lagrange) nodal elements on affine tetrahedra
//! (Epic #475, issue #602).
//!
//! The 10-node quadratic Lagrange tet: one DOF per vertex plus one DOF per
//! edge midpoint. In barycentric coordinates `λ = (λ₀..λ₃)` the shape
//! functions are
//!
//! ```text
//!   vertex p:      N_p  = λ_p (2λ_p − 1)                (p = 0..3)
//!   edge (a, b):   N_ab = 4 λ_a λ_b                     (6 edges)
//! ```
//!
//! with gradients (constant `∇λ_p` on an affine tet)
//!
//! ```text
//!   ∇N_p  = (4λ_p − 1) ∇λ_p
//!   ∇N_ab = 4 (λ_a ∇λ_b + λ_b ∇λ_a)
//! ```
//!
//! # Local DOF ordering
//!
//! `[v0, v1, v2, v3, e01, e02, e03, e12, e13, e23]` — the four vertices in
//! tet order followed by the six edges in the canonical
//! [`TET_LOCAL_EDGES`] order. Global P2 DOF
//! numbering (vertex DOFs at their node index, edge-midpoint DOFs offset by
//! `n_nodes`) is the assembler's concern
//! ([`crate::assembly::electrostatic::assemble_electrostatic_p2`]); this
//! module is purely local. Edge-midpoint **value** DOFs are
//! orientation-independent, so the sign field of
//! [`TetMesh::tet_edges`](crate::mesh::TetMesh::tet_edges) is irrelevant
//! here (unlike the signed Nédélec edge DOFs).
//!
//! # Quadrature
//!
//! The stiffness integrand `∇N_i · ∇N_j` is **quadratic** in the
//! barycentrics on an affine tet, so the symmetric degree-2 4-point rule
//! ([`TET_QUAD4_A`]/[`TET_QUAD4_B`], shared with the Nédélec kernels)
//! integrates it **exactly** — no new rule is needed, and the local
//! stiffness below is exact up to rounding. The load integrals `∫ N_p dV`
//! are taken from the closed-form barycentric monomial formula
//! (`∫ λ^α dV = V α! 3!/(|α|+3)!`): `−V/20` per vertex DOF and `V/5` per
//! edge DOF (they sum to `V`, the integral of the partition of unity).
//!
//! This is the 3-D sibling of the 2-D P2 triangle path
//! (`crate::assembly::magnetostatic`, issue #472); the element functions
//! live here in `elements/` rather than inline in the assembler, per the
//! issue-#602 scoping.

use crate::elements::nedelec::{TET_QUAD4_A, TET_QUAD4_B};
use crate::mesh::TET_LOCAL_EDGES;

/// Number of local DOFs of the P2 Lagrange tet (4 vertices + 6 edges).
pub const TET_P2_DOFS: usize = 10;

/// The ten P2 shape-function values at a barycentric point.
///
/// Local order `[v0..v3, e01, e02, e03, e12, e13, e23]` (see module docs).
pub fn tet_p2_shape(lam: &[f64; 4]) -> [f64; TET_P2_DOFS] {
    let mut n = [0.0_f64; TET_P2_DOFS];
    for p in 0..4 {
        n[p] = lam[p] * (2.0 * lam[p] - 1.0);
    }
    for (e, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        n[4 + e] = 4.0 * lam[a] * lam[b];
    }
    n
}

/// The ten P2 shape-function gradients at a barycentric point, given the
/// (constant, per-affine-tet) barycentric gradients `∇λ_p`.
///
/// Local order `[v0..v3, e01, e02, e03, e12, e13, e23]` (see module docs).
pub fn tet_p2_grads(lam: &[f64; 4], bary: &[[f64; 3]; 4]) -> [[f64; 3]; TET_P2_DOFS] {
    let mut g = [[0.0_f64; 3]; TET_P2_DOFS];
    for p in 0..4 {
        let c = 4.0 * lam[p] - 1.0;
        for d in 0..3 {
            g[p][d] = c * bary[p][d];
        }
    }
    for (e, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
        for d in 0..3 {
            g[4 + e][d] = 4.0 * (lam[a] * bary[b][d] + lam[b] * bary[a][d]);
        }
    }
    g
}

/// Local P2 tet stiffness `K`, load-integral vector `∫ N_p dV`, and
/// (unsigned) volume for an affine tet.
///
/// `K_ij = ∫_T ∇N_i · ∇N_j dV`, evaluated with the degree-2 4-point rule —
/// **exact** for the quadratic integrand on an affine tet (see module
/// docs). The load vector carries the exact `∫ N_p dV` (`−V/20` for vertex
/// DOFs, `V/5` for edge DOFs), which is what a piecewise-constant source
/// `ρ` needs on the RHS: `b_p = ρ ∫ N_p dV`.
///
/// The material weighting (`ε₀ ε_r` etc.) is the caller's concern, matching
/// [`crate::assembly::electrostatic::tet_p1_local`].
pub fn tet_p2_local(
    coords: &[[f64; 3]; 4],
) -> ([[f64; TET_P2_DOFS]; TET_P2_DOFS], [f64; TET_P2_DOFS], f64) {
    // Barycentric gradients from the cofactor construction, matching the
    // host-side P1 kernel: e_i = v_i − v₀, g₁ = e₂×e₃, g₂ = e₃×e₁,
    // g₃ = e₁×e₂, det = e₁·g₁ = 6V, ∇λ_i = g_i/det, ∇λ₀ = −Σ ∇λ_i.
    let v0 = coords[0];
    let e1 = sub(coords[1], v0);
    let e2 = sub(coords[2], v0);
    let e3 = sub(coords[3], v0);
    let g1 = cross(e2, e3);
    let g2 = cross(e3, e1);
    let g3 = cross(e1, e2);
    let det = dot(e1, g1); // = 6V (signed)
    let vol = det.abs() / 6.0;
    let inv = 1.0 / det;
    let gl1 = [g1[0] * inv, g1[1] * inv, g1[2] * inv];
    let gl2 = [g2[0] * inv, g2[1] * inv, g2[2] * inv];
    let gl3 = [g3[0] * inv, g3[1] * inv, g3[2] * inv];
    let gl0 = [
        -(gl1[0] + gl2[0] + gl3[0]),
        -(gl1[1] + gl2[1] + gl3[1]),
        -(gl1[2] + gl2[2] + gl3[2]),
    ];
    let bary = [gl0, gl1, gl2, gl3];

    // Degree-2 symmetric 4-point rule: point q has λ_q = A, λ_{p≠q} = B,
    // weight V/4. Exact for the quadratic ∇N_i·∇N_j integrand.
    let w = vol / 4.0;
    let mut k = [[0.0_f64; TET_P2_DOFS]; TET_P2_DOFS];
    for q in 0..4 {
        let mut lam = [TET_QUAD4_B; 4];
        lam[q] = TET_QUAD4_A;
        let g = tet_p2_grads(&lam, &bary);
        for i in 0..TET_P2_DOFS {
            for j in 0..TET_P2_DOFS {
                k[i][j] += w * dot(g[i], g[j]);
            }
        }
    }

    // Exact load integrals ∫ N_p dV (barycentric monomial formula):
    //   vertex: ∫ λ(2λ−1) = 2·V/10 − V/4 = −V/20;  edge: 4·∫ λ_a λ_b = V/5.
    let mut load = [vol / 5.0; TET_P2_DOFS];
    for l in load.iter_mut().take(4) {
        *l = -vol / 20.0;
    }

    (k, load, vol)
}

#[inline]
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
#[inline]
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;

    const REF_TET: [[f64; 3]; 4] = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ];

    /// A deliberately skewed, non-degenerate tet for generality.
    const SKEW_TET: [[f64; 3]; 4] = [
        [0.1, 0.2, -0.3],
        [1.3, 0.4, 0.2],
        [-0.2, 1.1, 0.5],
        [0.3, -0.1, 1.4],
    ];

    /// The local coordinates of the ten P2 nodes of a tet (4 vertices, 6
    /// edge midpoints in `TET_LOCAL_EDGES` order).
    fn p2_node_coords(coords: &[[f64; 3]; 4]) -> [[f64; 3]; TET_P2_DOFS] {
        let mut x = [[0.0; 3]; TET_P2_DOFS];
        x[..4].copy_from_slice(coords);
        for (e, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
            for d in 0..3 {
                x[4 + e][d] = 0.5 * (coords[a][d] + coords[b][d]);
            }
        }
        x
    }

    #[test]
    fn partition_of_unity_and_zero_gradient_sum() {
        // At arbitrary barycentric points: Σ N_i = 1 and Σ ∇N_i = 0.
        let pts = [
            [0.25, 0.25, 0.25, 0.25],
            [
                0.585_410_196_624_968_5,
                0.138_196_601_125_010_5,
                0.138_196_601_125_010_5,
                0.138_196_601_125_010_5,
            ],
            [0.1, 0.2, 0.3, 0.4],
            [0.7, 0.05, 0.15, 0.1],
        ];
        // Barycentric gradients of the skew tet, via the exposed kernel's
        // internals reproduced (partition of unity is basis-only).
        for lam in &pts {
            let n = tet_p2_shape(lam);
            let s: f64 = n.iter().sum();
            assert!((s - 1.0).abs() < 1e-14, "Σ N = {s} != 1 at {lam:?}");
        }
        // Gradient sum: Σ ∇N_i = 3 Σ_p ∇λ_p (coefficient of each ∇λ_p is
        // (4λ_p − 1) + 4 Σ_{q≠p} λ_q = 3), which vanishes exactly when the
        // barycentric gradients satisfy Σ ∇λ_p = 0 — as any valid set does.
        // This synthetic set sums to zero, so it is a valid algebraic check.
        let bary = [
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [-1.0, -1.0, -1.0],
        ];
        for lam in &pts {
            let g = tet_p2_grads(lam, &bary);
            for d in 0..3 {
                let s: f64 = g.iter().map(|gi| gi[d]).sum();
                assert!(s.abs() < 1e-13, "Σ ∇N[{d}] = {s} != 0 at {lam:?}");
            }
        }
    }

    #[test]
    fn stiffness_rows_sum_to_zero_and_symmetric() {
        // Constant field ⇒ zero gradient: K · 1 = 0; and K is symmetric.
        let (k, _, _) = tet_p2_local(&SKEW_TET);
        for (i, row) in k.iter().enumerate() {
            let s: f64 = row.iter().sum();
            assert!(s.abs() < 1e-12, "stiffness row {i} sum {s} != 0");
            for (j, &kij) in row.iter().enumerate() {
                assert!(
                    (kij - k[j][i]).abs() < 1e-13,
                    "K not symmetric at ({i},{j})"
                );
            }
        }
    }

    #[test]
    fn load_integrals_sum_to_volume() {
        let (_, load, vol) = tet_p2_local(&SKEW_TET);
        let s: f64 = load.iter().sum();
        assert!(
            (s - vol).abs() < 1e-14 * vol.max(1.0),
            "Σ ∫N_p = {s} != V = {vol}"
        );
        // Exact per-DOF values.
        for l in load.iter().take(4) {
            assert!((l + vol / 20.0).abs() < 1e-15);
        }
        for l in load.iter().skip(4) {
            assert!((l - vol / 5.0).abs() < 1e-15);
        }
    }

    /// Dirichlet-energy exactness on a **linear** field: the P2 interpolant
    /// of `f = c·x` is exact, so `uᵀ K u = |∇f|² V`.
    #[test]
    fn linear_field_energy_exact() {
        for coords in [&REF_TET, &SKEW_TET] {
            let (k, _, vol) = tet_p2_local(coords);
            let f = |p: &[f64; 3]| 2.0 * p[0] - 0.5 * p[1] + 3.0 * p[2] + 1.0;
            let grad2 = 2.0_f64.powi(2) + 0.5_f64.powi(2) + 3.0_f64.powi(2);
            let x = p2_node_coords(coords);
            let u: Vec<f64> = x.iter().map(f).collect();
            let mut e = 0.0;
            for i in 0..TET_P2_DOFS {
                for j in 0..TET_P2_DOFS {
                    e += u[i] * k[i][j] * u[j];
                }
            }
            let want = grad2 * vol;
            assert!(
                (e - want).abs() < 1e-12 * want,
                "linear Dirichlet energy {e} != {want}"
            );
        }
    }

    /// Dirichlet-energy exactness on a **quadratic** field — the property
    /// that separates P2 from P1. On the reference tet, `f = x²` has
    /// `∫ |∇f|² dV = ∫ 4x² dV = 4·(V/10) = 4/60 = 1/15` (barycentric
    /// monomial formula with `λ₁ = x`, `V = 1/6`). The P2 interpolant of a
    /// quadratic is the quadratic itself, and the degree-2 rule integrates
    /// its gradient-product exactly, so `uᵀ K u` must equal `1/15` to
    /// rounding. P1 on this single tet would get `1/12` (the interpolant
    /// `u = x` has `|∇u|² = 1`... times V — a ~11% miss), so this test
    /// has teeth against a silently-linear basis.
    #[test]
    fn quadratic_field_energy_exact() {
        let (k, _, _) = tet_p2_local(&REF_TET);
        let x = p2_node_coords(&REF_TET);
        let u: Vec<f64> = x.iter().map(|p| p[0] * p[0]).collect();
        let mut e = 0.0;
        for i in 0..TET_P2_DOFS {
            for j in 0..TET_P2_DOFS {
                e += u[i] * k[i][j] * u[j];
            }
        }
        let want = 1.0 / 15.0;
        assert!(
            (e - want).abs() < 1e-13,
            "quadratic Dirichlet energy {e} != {want} (P1 would give {})",
            1.0 / 12.0
        );
    }
}
