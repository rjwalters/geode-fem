//! The **p=2 tangential-trace surface kernel** (issue #857, Epic #836
//! Phase 1b): the boundary-triangle building block of every p=2 surface
//! term of the order-generic
//! [`crate::driven::solve::DrivenOperator::assemble_with_space`] — lumped
//! ports, Leontovich / rough-conductor / London impedance walls and the
//! Silver-Müller absorber.
//!
//! # The trace space: 8 DOFs per boundary triangle
//!
//! The tangential trace `n × (E × n)` of the second-order first-kind
//! Nédélec tet space ([`crate::elements::nedelec_p2`]) on a face `(a, b, c)`
//! is the 8-dimensional second-order Nédélec triangle space. With the face
//! vertices in **ascending global order** `a < b < c` (the convention of
//! [`crate::assembly::hcurl_space::HcurlSpace`]) and `∇_s λ_p` the in-plane
//! (surface) barycentric gradients, the 8 trace functions are, in the local
//! layout used throughout this module,
//!
//! ```text
//!   0  W_ab = λ_a ∇_sλ_b − λ_b ∇_sλ_a     1  Q_ab = ∇_s(λ_a λ_b)
//!   2  W_ac                               3  Q_ac
//!   4  W_bc                               5  Q_bc
//!   6  φ0 = λ_c W_ab                      7  φ1 = λ_a W_bc
//! ```
//!
//! — exactly the layout of the 2-D element
//! [`crate::analytic::waveguide::tri_nedelec2_local`]
//! (`[W₀, Q₀, W₁, Q₁, W₂, Q₂, I₀ = λ₂W₀, I₁ = λ₀W₂]` on vertices `0, 1, 2`).
//!
//! **Why this is the trace of the tet space.** On the face, the tet
//! barycentric of the opposite vertex vanishes and its gradient is normal,
//! and for a face vertex `p` the tangential part of `∇λ_p` is the surface
//! gradient `∇_sλ_p`. So the tet's edge functions `W, Q` on the three face
//! edges and its face pair `(φ0, φ1)` restrict to the functions above, and
//! the tet's other 10 functions (edges / faces off this face) have zero
//! tangential trace. Under the ascending convention both the tet and the
//! triangle label the face the same way, so the 8 trace DOFs **are** the
//! global DOFs [`HcurlSpace::edge_dofs`] (`[W, Q]`, `W` oriented low→high)
//! and [`HcurlSpace::face_dofs`] (`[φ0, φ1]`) — unit sign, no `2×2` mixing,
//! independent of which tet owns the face. A caller whose face labelling is
//! not ascending would need [`HcurlSpace::face_dof_transform`]; this kernel
//! sorts first, so it never does. The trace identity is pinned by
//! `tests/surface_p2_trace.rs` (tet restriction ≡ this kernel ≡
//! `tri_nedelec2_local`, to round-off).
//!
//! # Integrals
//!
//! - **Surface mass** `S[i, j] = ∮ (n×N_i)·(n×N_j) dS = ∮ N_t,i · N_t,j dS`
//!   ([`tri_nedelec2_surface_mass`]): degree-4 integrand, integrated with
//!   the 6-point degree-4 Dunavant rule
//!   [`crate::analytic::waveguide::TRI_QUAD_DEG4`] (exact on a flat
//!   triangle). Real symmetric positive semi-definite, so every surface
//!   coefficient `iω/Z_s(ω)` keeps `A(ω)ᵀ = A(ω)`.
//! - **Port flux** `f_i = ∮ N_t,i · ê dS` ([`tri_nedelec2_surface_flux`]):
//!   degree-2, same rule. Only the in-plane part of `ê` contributes, as at
//!   p=1.
//!
//! Both are ω-independent and assembled once per operator.

use crate::analytic::waveguide::TRI_QUAD_DEG4;
use crate::assembly::hcurl_space::HcurlSpace;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// Number of tangential-trace DOFs of the p=2 Nédélec space on one
/// triangle (3 edges × 2 + 1 face × 2).
pub const TRI_NEDELEC2_TRACE_DOFS: usize = 8;

/// The local face edges `(a, b), (a, c), (b, c)` of the trace layout, as
/// positions in the ascending vertex triple.
const TRACE_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

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

/// In-plane (surface) barycentric gradients `∇_sλ_p` of a 3-D triangle and
/// its (unsigned) area.
///
/// With `e1 = v1 − v0`, `e2 = v2 − v0`, `n = e1 × e2` (`|n| = 2·area`):
/// `∇_sλ1 = (e2 × n)/|n|²`, `∇_sλ2 = (n × e1)/|n|²`,
/// `∇_sλ0 = −∇_sλ1 − ∇_sλ2`. Each is tangential, and `∇_sλ_p · (v_q − v_0)
/// = δ_pq − δ_p0` for `q ∈ {1, 2}`.
///
/// # Panics
///
/// Panics on a degenerate (zero-area) triangle.
pub fn tri_surface_gradients(v: &[[f64; 3]; 3]) -> ([[f64; 3]; 3], f64) {
    let e1 = sub(v[1], v[0]);
    let e2 = sub(v[2], v[0]);
    let n = cross(e1, e2);
    let n2 = dot(n, n);
    assert!(n2 > 0.0, "degenerate surface triangle {v:?}");
    let g1 = cross(e2, n).map(|x| x / n2);
    let g2 = cross(n, e1).map(|x| x / n2);
    let g0 = [-(g1[0] + g2[0]), -(g1[1] + g2[1]), -(g1[2] + g2[2])];
    ([g0, g1, g2], 0.5 * n2.sqrt())
}

/// The 8 p=2 tangential-trace basis vectors (module-level layout) at the
/// barycentric point `lam` of a triangle whose vertices are in **ascending
/// global order**, given its surface gradients `grad = ∇_sλ_p`
/// ([`tri_surface_gradients`]).
pub fn tri_nedelec2_trace_shapes(
    lam: &[f64; 3],
    grad: &[[f64; 3]; 3],
) -> [[f64; 3]; TRI_NEDELEC2_TRACE_DOFS] {
    let w = |a: usize, b: usize| -> [f64; 3] {
        std::array::from_fn(|d| lam[a] * grad[b][d] - lam[b] * grad[a][d])
    };
    let q = |a: usize, b: usize| -> [f64; 3] {
        std::array::from_fn(|d| lam[a] * grad[b][d] + lam[b] * grad[a][d])
    };
    let mut out = [[0.0_f64; 3]; TRI_NEDELEC2_TRACE_DOFS];
    for (k, &(a, b)) in TRACE_EDGES.iter().enumerate() {
        out[2 * k] = w(a, b);
        out[2 * k + 1] = q(a, b);
    }
    let w_ab = w(0, 1);
    let w_bc = w(1, 2);
    out[6] = w_ab.map(|x| lam[2] * x);
    out[7] = w_bc.map(|x| lam[0] * x);
    out
}

/// Local p=2 tangential surface mass `S[i, j] = ∫_T N_t,i · N_t,j dS` of a
/// triangle with vertices in **ascending global order** (module-level
/// layout). Integrated exactly with the degree-4 rule; real symmetric.
pub fn tri_nedelec2_surface_mass(
    v: &[[f64; 3]; 3],
) -> [[f64; TRI_NEDELEC2_TRACE_DOFS]; TRI_NEDELEC2_TRACE_DOFS] {
    let (grad, area) = tri_surface_gradients(v);
    let mut s = [[0.0_f64; TRI_NEDELEC2_TRACE_DOFS]; TRI_NEDELEC2_TRACE_DOFS];
    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area;
        let n = tri_nedelec2_trace_shapes(&lam, &grad);
        for i in 0..TRI_NEDELEC2_TRACE_DOFS {
            for j in 0..TRI_NEDELEC2_TRACE_DOFS {
                s[i][j] += w * dot(n[i], n[j]);
            }
        }
    }
    s
}

/// Local p=2 port flux `f_i = ∫_T N_t,i · ê dS` of a triangle with vertices
/// in **ascending global order** (module-level layout). Only the in-plane
/// part of `e_hat` contributes.
pub fn tri_nedelec2_surface_flux(
    v: &[[f64; 3]; 3],
    e_hat: [f64; 3],
) -> [f64; TRI_NEDELEC2_TRACE_DOFS] {
    let (grad, area) = tri_surface_gradients(v);
    let mut f = [0.0_f64; TRI_NEDELEC2_TRACE_DOFS];
    for row in TRI_QUAD_DEG4.iter() {
        let lam = [row[0], row[1], row[2]];
        let w = row[3] * area;
        let n = tri_nedelec2_trace_shapes(&lam, &grad);
        for (fi, ni) in f.iter_mut().zip(n.iter()) {
            *fi += w * dot(*ni, e_hat);
        }
    }
    f
}

/// The 8 global trace DOFs of boundary triangle `tri` (any winding) on a
/// **p=2** space, in the module-level layout, together with the triangle's
/// vertices sorted into ascending global order (the order the local
/// kernels must be evaluated on).
///
/// Returns `None` if the triangle has a repeated node or is not a face of
/// the space's mesh (one of its edges or the face itself is missing from
/// the space's tables) — callers turn that into
/// [`crate::driven::solve::DrivenError::SurfaceNotOnMesh`].
///
/// # Panics
///
/// Panics if `space` is not a p=2 space.
pub fn p2_trace_dofs(
    space: &HcurlSpace,
    tri: &[u32; 3],
) -> Option<([u32; TRI_NEDELEC2_TRACE_DOFS], [u32; 3])> {
    assert_eq!(
        space.order(),
        ElementOrder::P2,
        "p2_trace_dofs needs a p=2 space"
    );
    let mut s = *tri;
    s.sort_unstable();
    if s[0] == s[1] || s[1] == s[2] {
        return None;
    }
    let edges = space.edges();
    let faces = space.faces();
    let mut out = [0u32; TRI_NEDELEC2_TRACE_DOFS];
    for (k, &(a, b)) in TRACE_EDGES.iter().enumerate() {
        let ge = edges.binary_search(&[s[a], s[b]]).ok()?;
        let d = space.edge_dofs(ge);
        out[2 * k] = d[0];
        out[2 * k + 1] = d[1];
    }
    let gf = faces.binary_search(&s).ok()?;
    let d = space.face_dofs(gf);
    out[6] = d[0];
    out[7] = d[1];
    Some((out, s))
}

/// Coordinates of the ascending vertex triple `s`.
fn coords(mesh: &TetMesh, s: &[u32; 3]) -> [[f64; 3]; 3] {
    std::array::from_fn(|i| mesh.nodes[s[i] as usize])
}

/// Assemble the p=2 tangential surface mass of the triangles `tris` as
/// global `(row, col, value)` triplets over the space's DOFs (duplicates
/// unsummed, 64 per triangle, triangle order then row-major) — the p=2
/// analogue of
/// [`crate::assembly::surface::assemble_surface_mass_triplets`].
///
/// # Errors
///
/// `Err(tri)` with the first triangle (caller's node order) that is not a
/// face of the space's mesh.
///
/// # Panics
///
/// Panics if `space` is not a p=2 space or `mesh` is not its mesh.
pub fn assemble_p2_surface_mass_triplets(
    space: &HcurlSpace,
    mesh: &TetMesh,
    tris: &[[u32; 3]],
) -> Result<Vec<(usize, usize, f64)>, [u32; 3]> {
    assert_eq!(mesh.n_nodes(), space.n_nodes(), "space / mesh mismatch");
    let mut out = Vec::with_capacity(tris.len() * TRI_NEDELEC2_TRACE_DOFS.pow(2));
    for tri in tris {
        let (dofs, s) = p2_trace_dofs(space, tri).ok_or(*tri)?;
        let m = tri_nedelec2_surface_mass(&coords(mesh, &s));
        for i in 0..TRI_NEDELEC2_TRACE_DOFS {
            for j in 0..TRI_NEDELEC2_TRACE_DOFS {
                out.push((dofs[i] as usize, dofs[j] as usize, m[i][j]));
            }
        }
    }
    Ok(out)
}

/// Assemble the p=2 port flux `f_i = ∮ N_i · ê dS` of the triangles `tris`
/// as a full-length `[space.n_dofs()]` real vector — the p=2 analogue of
/// [`crate::driven::ports::assemble_port_flux`]. It serves both the
/// lumped-port excitation and the voltage readout `V = (1/w) Σ f_i E_i`.
///
/// # Errors
///
/// `Err(tri)` with the first triangle (caller's node order) that is not a
/// face of the space's mesh.
///
/// # Panics
///
/// Panics if `space` is not a p=2 space or `mesh` is not its mesh.
pub fn assemble_p2_port_flux(
    space: &HcurlSpace,
    mesh: &TetMesh,
    tris: &[[u32; 3]],
    e_hat: [f64; 3],
) -> Result<Vec<f64>, [u32; 3]> {
    assert_eq!(mesh.n_nodes(), space.n_nodes(), "space / mesh mismatch");
    let mut flux = vec![0.0_f64; space.n_dofs()];
    for tri in tris {
        let (dofs, s) = p2_trace_dofs(space, tri).ok_or(*tri)?;
        let f = tri_nedelec2_surface_flux(&coords(mesh, &s), e_hat);
        for (d, fi) in dofs.iter().zip(f.iter()) {
            flux[*d as usize] += fi;
        }
    }
    Ok(flux)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The surface gradients are tangential and dual to the edge vectors.
    #[test]
    fn surface_gradients_are_dual_and_tangential() {
        let v = [[0.1, 0.2, 0.3], [1.3, 0.4, -0.2], [0.2, 1.1, 0.9]];
        let (g, area) = tri_surface_gradients(&v);
        let n = cross(sub(v[1], v[0]), sub(v[2], v[0]));
        for (p, gp) in g.iter().enumerate() {
            assert!(dot(*gp, n).abs() < 1e-14);
            for q in 0..3 {
                let want = f64::from(u8::from(p == q)) - f64::from(u8::from(p == 0));
                if q > 0 {
                    assert!((dot(*gp, sub(v[q], v[0])) - want).abs() < 1e-14);
                }
            }
        }
        assert!((area - 0.5 * dot(n, n).sqrt()).abs() < 1e-15);
    }

    /// The local surface mass is symmetric with a positive diagonal.
    #[test]
    fn mass_is_symmetric_psd() {
        let v = [[0.0, 0.0, 0.0], [1.0, 0.2, 0.1], [0.3, 0.9, -0.2]];
        let s = tri_nedelec2_surface_mass(&v);
        for i in 0..8 {
            assert!(s[i][i] > 0.0);
            for j in 0..8 {
                assert!((s[i][j] - s[j][i]).abs() < 1e-15);
            }
        }
    }
}
