//! Shared Whitney 1-form triangle-face kernel (issue #208).
//!
//! Both the Silver-Müller / Leontovich impedance boundary conditions
//! ([`crate::assembly::surface`]) and the uniform lumped port
//! ([`crate::lumped_port`]) integrate first-order (Whitney) Nédélec
//! edge-element traces over flat boundary triangles. Before this module
//! existed the per-face geometry (area, in-plane barycentric gradients,
//! lower-tag-first edge orientation signs) and the tangential
//! surface-mass quadrature were duplicated in both files — see issue
//! #208. This `pub(crate)` module hosts the single implementation; the
//! public assembly entry points (`assemble_surface_mass*`,
//! `assemble_port_surface_mass`, `assemble_port_flux`) stay where they
//! were and delegate here.
//!
//! # Discretization (see `silvermuller.rs` module docs for the full
//! derivation)
//!
//! On a **flat** triangle the Whitney trace `N_e = λ_a ∇λ_b − λ_b ∇λ_a`
//! lies in the face plane, so the BAC-CAB identity rank-reduces the
//! tangential integrand: `(n × N_i) · (n × N_j) = N_i · N_j`. The mass
//! integrand is degree-2 in barycentric coordinates and is integrated
//! exactly with the 3-point edge-midpoint rule (Hammer-Stroud,
//! degree-2 exact):
//!
//! ```text
//! ∫_T f dA ≈ (area / 3) · [f(m_01) + f(m_02) + f(m_12)]
//! ```
//!
//! # Sign convention
//!
//! Edge DOFs use the same lower-tag-first global orientation as
//! [`crate::assembly::nedelec`]: for a global edge `(va, vb)` with
//! `va < vb` the basis direction is `va → vb` (sign +1); a local face
//! edge contributes with sign `+1` if its local direction produces
//! global tags in ascending order and `-1` otherwise. Face entry
//! `(i, j)` is multiplied by `s_i · s_j` before scattering into the
//! global matrix — the same rule as the volume assembly.

use std::collections::HashMap;

use crate::mesh::TetMesh;

/// Local edge order on a triangle face. Mirrors `TET_LOCAL_EDGES` for
/// tets: lower local index first.
pub(crate) const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// `BARYCENTRIC_MIDPOINTS[q][k]` = value of `λ_k` at quadrature point
/// `q` of the 3-point edge-midpoint rule. `q=0` → midpoint of edge
/// `(0,1)`, `q=1` → edge `(0,2)`, `q=2` → edge `(1,2)`.
pub(crate) const BARYCENTRIC_MIDPOINTS: [[f64; 3]; 3] = [
    [0.5, 0.5, 0.0], // m_01
    [0.5, 0.0, 0.5], // m_02
    [0.0, 0.5, 0.5], // m_12
];

/// Per-face Whitney geometry shared by the mass and flux kernels.
pub(crate) struct FaceGeometry {
    /// Triangle area.
    pub(crate) area: f64,
    /// In-plane gradients of the three barycentric coordinates.
    pub(crate) grad_lambda: [[f64; 3]; 3],
    /// `(global_edge_index, orientation_sign)` for the three local
    /// edges in [`TRI_LOCAL_EDGES`] order.
    pub(crate) edge_info: [(u32, i8); 3],
}

/// Build the `(lo, hi) → global edge index` lookup from the global
/// edge table (`mesh.edges()` order, lower tag first).
pub(crate) fn edge_lookup(edges: &[[u32; 2]]) -> HashMap<(u32, u32), u32> {
    let mut lookup = HashMap::with_capacity(edges.len());
    for (idx, e) in edges.iter().enumerate() {
        lookup.insert((e[0], e[1]), idx as u32);
    }
    lookup
}

/// Compute the per-face Whitney geometry: area, in-plane barycentric
/// gradients, and the `(global_edge_index, sign)` mapping for the three
/// local edges.
///
/// The triangle's unit normal direction (right-hand rule on the vertex
/// order) only enters through the in-plane gradient formula
/// `∇λ_k = (n̂ × edge_opposite_k) / (2·area)`; its sign does not affect
/// any of the integrals built on top (the mass integrand only sees
/// in-plane components, the flux integrand is linear in `∇λ`
/// differences), so winding order does not matter for callers.
///
/// # Panics
///
/// Panics if a triangle edge does not appear in `edge_lookup` — i.e. if
/// the triangle is not a face of the tet mesh whose edge table was used
/// to build the lookup.
pub(crate) fn face_geometry(
    tri: &[u32; 3],
    v: &[[f64; 3]; 3],
    edge_lookup: &HashMap<(u32, u32), u32>,
) -> FaceGeometry {
    let e10 = sub3(v[1], v[0]);
    let e20 = sub3(v[2], v[0]);
    let cross = cross3(e10, e20);
    let two_area = norm3(cross);
    let area = 0.5 * two_area;
    // Unit normal — direction is the right-hand-rule one from vertex
    // ordering. Sign does not matter (see doc comment above).
    let n_hat = [
        cross[0] / two_area,
        cross[1] / two_area,
        cross[2] / two_area,
    ];

    // In-plane gradients of triangle barycentric coords:
    //   ∇λ_k = (n_hat × edge_opposite_k) / (2 area)
    // where edge_opposite_k goes from v_{(k+1)%3} to v_{(k+2)%3}.
    // This formula puts ∇λ_k perpendicular to the opposite edge, in the
    // triangle plane, with the right magnitude (1 / height_from_k).
    let opp = [sub3(v[2], v[1]), sub3(v[0], v[2]), sub3(v[1], v[0])];
    let grad_lambda: [[f64; 3]; 3] = [
        scale3(cross3(n_hat, opp[0]), 1.0 / two_area),
        scale3(cross3(n_hat, opp[1]), 1.0 / two_area),
        scale3(cross3(n_hat, opp[2]), 1.0 / two_area),
    ];

    // Global-edge mapping + orientation sign for each local triangle
    // edge: compare the global node tags of the endpoints (lower tag
    // first), matching the volume assembly convention.
    let mut edge_info: [(u32, i8); 3] = [(0, 1); 3];
    for (k, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
        let ga = tri[la];
        let gb = tri[lb];
        let (lo, hi, sign) = if ga < gb {
            (ga, gb, 1i8)
        } else {
            (gb, ga, -1i8)
        };
        let gidx = *edge_lookup
            .get(&(lo, hi))
            .expect("triangle edge must appear in global edge table");
        edge_info[k] = (gidx, sign);
    }

    FaceGeometry {
        area,
        grad_lambda,
        edge_info,
    }
}

/// Build the 3×3 face contribution `S^face_{ij} = ∫_T N_i · N_j dA`
/// (unsigned, local-edge order [`TRI_LOCAL_EDGES`]) with the 3-point
/// edge-midpoint quadrature on the Whitney 1-form trace basis.
///
/// The integrand is exactly quadratic in barycentric coordinates, so
/// the rule reproduces the analytic face mass to floating-point
/// precision — see `silvermuller::tests::face_mass_matches_analytic`.
pub(crate) fn face_mass_block(geo: &FaceGeometry) -> [[f64; 3]; 3] {
    let weight = geo.area / 3.0;
    let mut block = [[0.0_f64; 3]; 3];
    for lam in BARYCENTRIC_MIDPOINTS.iter() {
        // Evaluate the three Whitney basis vectors at this midpoint:
        // N_e(λ) = λ_la · ∇λ_lb − λ_lb · ∇λ_la for local edge (la, lb).
        let mut basis_q: [[f64; 3]; 3] = [[0.0; 3]; 3];
        for (k, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let term_a = scale3(geo.grad_lambda[lb], lam[la]);
            let term_b = scale3(geo.grad_lambda[la], lam[lb]);
            basis_q[k] = sub3(term_a, term_b);
        }
        for i in 0..3 {
            for j in 0..3 {
                block[i][j] += weight * dot3(basis_q[i], basis_q[j]);
            }
        }
    }
    block
}

/// Assemble the tangential-trace surface mass
/// `S_{ij} = ∮_Γ (n × N_i) · (n × N_j) dS = ∮_Γ N_i · N_j dS`
/// over an explicit triangle list as signed `(row, col, value)` triplets
/// over global edge indices.
///
/// Duplicate `(row, col)` entries (edges shared by adjacent faces) are
/// **not** summed — the caller accumulates them. Triplets appear in
/// face order, then row-major within each 3×3 face block, so summing
/// them reproduces the dense accumulation order exactly.
///
/// This is the single kernel behind both
/// [`crate::assembly::surface::assemble_surface_mass_triplets`] and
/// [`crate::lumped_port::assemble_port_surface_mass`] (issue #208).
///
/// # Panics
///
/// Panics if a triangle edge does not appear in `edges` — i.e. if the
/// triangles are not faces of the tet mesh whose edge table was passed.
pub(crate) fn assemble_surface_mass_triplets(
    mesh: &TetMesh,
    triangles: &[[u32; 3]],
    edges: &[[u32; 2]],
) -> Vec<(usize, usize, f64)> {
    let lookup = edge_lookup(edges);

    let mut triplets = Vec::with_capacity(triangles.len() * 9);
    for tri in triangles.iter() {
        let v: [[f64; 3]; 3] = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let geo = face_geometry(tri, &v, &lookup);
        let block = face_mass_block(&geo);

        for (row, &(gi, si)) in block.iter().zip(geo.edge_info.iter()) {
            for (&val, &(gj, sj)) in row.iter().zip(geo.edge_info.iter()) {
                triplets.push((gi as usize, gj as usize, val * (si as f64) * (sj as f64)));
            }
        }
    }

    triplets
}

#[inline]
pub(crate) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub(crate) fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub(crate) fn norm3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[inline]
pub(crate) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
