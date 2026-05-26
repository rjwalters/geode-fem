//! First-order Silver-Müller absorbing boundary condition (issue #27).
//!
//! Replaces the PEC outer wall with the impedance condition
//!
//! ```text
//! n × (∇ × E) = -j k₀ n × (n × E)
//! ```
//!
//! In the curl-curl weak form `(∇ × v) · (∇ × E) - k² ε v · E = 0`,
//! integration by parts on the boundary picks up the surface term
//! `∮ (n × v) · (∇ × E)` which the Silver-Müller substitution turns into
//!
//! ```text
//! ∮ (n × v) · (-j k₀ n × n × E) = -j k₀ ∮ (n × v) · (n × n × E)
//!                                = j k₀ ∮ (n × v) · (n × E_t)            (using BAC-CAB)
//!                                = j k₀ ∮ (n × N_i) · (n × N_j) E_j      (matrix form)
//! ```
//!
//! The generalized eigenproblem becomes complex:
//!
//! ```text
//! (K - j k₀ S) E = k² M E
//! ```
//!
//! where `S_{ij} = ∮_{∂Ω_outer} (n × N_i) · (n × N_j) dS` is the new
//! **real symmetric** surface matrix assembled by this module. The
//! caller multiplies by `j k₀` at solve time (kept out of the kernel
//! so the surface kernel stays purely `f64`).
//!
//! # Discretization
//!
//! For first-order (Whitney) Nédélec edge elements restricted to a
//! flat triangle face with outward unit normal `n`, the tangential
//! trace `n × N_e` lies entirely in the triangle plane. For two
//! tangential vectors `u`, `v` in that plane, the BAC-CAB identity
//! gives `(n × u) · (n × v) = (n·n)(u·v) - (n·u)(n·v) = u·v`, so
//! the face contribution reduces to
//!
//! ```text
//! S^face_{ij} = ∫_T N_i · N_j  dA
//! ```
//!
//! with `N_e = λ_a ∇λ_b - λ_b ∇λ_a` the 2D Whitney basis on the
//! triangle (in-plane gradients of barycentric coords). We evaluate
//! this with a **single centroid quadrature point**: the integrand is
//! piecewise-linear in barycentric coords (one `λ` factor times one
//! constant gradient), so a 1-point rule at `λ_k = 1/3` is exact up
//! to the linear part and produces a clean rank-1-style contribution.
//! Higher-order quadrature would be exact, but the centroid rule is
//! what the issue spec recommends for v0 (it's also the natural
//! choice for the flat-triangle Whitney form).
//!
//! # Sign convention
//!
//! Edge DOFs use the same lower-tag-first global orientation as
//! [`crate::nedelec_assembly`]: for a global edge `(va, vb)` with
//! `va < vb` the basis direction is `va → vb` (sign +1); the
//! triangle's local edge contributes with sign `+1` if its local
//! direction `(la, lb)` produces global tags in ascending order and
//! `-1` otherwise. Same scatter rule as the volume assembly: face
//! entry `(i, j)` is multiplied by `s_i · s_j` before adding to the
//! global `S`.

use std::collections::HashMap;

use faer::Mat;

use crate::mesh::TetMesh;

/// Assemble the dense Silver-Müller surface matrix `S` on the outer
/// boundary triangles.
///
/// Returns a real, symmetric `[n_edges, n_edges]` matrix whose only
/// non-zero rows/columns correspond to edges that touch at least one
/// face triangle tagged `outer_tag`. The caller multiplies by `j k₀`
/// to form the complex pencil
///
/// ```text
/// (K + j k₀ S) E = k² M E
/// ```
///
/// **Sign note on the pencil.** With the convention above, the weak
/// form contribution from the impedance BC is `+j k₀ ∮ (n × N_i) · (n × N_j)`
/// (see module docs). Some references absorb the sign into `k₀` (taking
/// the time convention `e^{+iωt}` rather than `e^{-iωt}`); we use the
/// `+j k₀ S` convention here, which produces eigenvalues `k²` with
/// `Im(k²) > 0` for radiating modes (Q > 0 with the standard
/// `Q = Re(k) / (2 Im(k))` definition).
///
/// # Arguments
///
/// * `mesh` — the volume tet mesh (used for node coordinates).
/// * `boundary_triangles` — `[n_triangles][3]` 0-based node indices
///   into `mesh.nodes`. Triangles can be at any orientation; the
///   triangle's outward normal is implicitly defined by the right-hand
///   rule on the listed vertex order. We don't need the actual normal
///   for the rank-reduced integrand `N · N`, so winding order does not
///   matter for the result.
/// * `boundary_tags` — `[n_triangles]` 2D physical tag per triangle.
/// * `outer_tag` — only triangles whose tag equals this value contribute.
/// * `edges` — the global edge list (`mesh.edges()`); used to build the
///   `(va, vb) → edge_index` lookup so face edges scatter into the
///   correct global rows/columns.
///
/// # Implementation
///
/// One 1-point centroid quadrature per face is used (see module docs
/// for why this is exact for first-order Whitney on flat triangles).
/// The 3×3 face contribution is built dense and scattered with the
/// global edge sign on each axis.
pub fn assemble_silver_muller_surface(
    mesh: &TetMesh,
    boundary_triangles: &[[u32; 3]],
    boundary_tags: &[i32],
    outer_tag: i32,
    edges: &[[u32; 2]],
) -> Mat<f64> {
    assert_eq!(
        boundary_triangles.len(),
        boundary_tags.len(),
        "triangles and tags length mismatch"
    );

    let n_edges = edges.len();
    let mut s = Mat::<f64>::zeros(n_edges, n_edges);

    // Build edge lookup: (lo, hi) -> global edge index.
    let mut edge_lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(n_edges);
    for (idx, e) in edges.iter().enumerate() {
        edge_lookup.insert((e[0], e[1]), idx as u32);
    }

    for (tri, &tag) in boundary_triangles.iter().zip(boundary_tags.iter()) {
        if tag != outer_tag {
            continue;
        }
        let v: [[f64; 3]; 3] = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (face_s, edge_info) = face_silver_muller_block(tri, &v, &edge_lookup);

        for i in 0..3 {
            let (gi, si) = edge_info[i];
            for j in 0..3 {
                let (gj, sj) = edge_info[j];
                let val = face_s[i][j] * (si as f64) * (sj as f64);
                let cur = s[(gi as usize, gj as usize)];
                s[(gi as usize, gj as usize)] = cur + val;
            }
        }
    }

    s
}

/// Local edge order on a triangle face. Mirrors `TET_LOCAL_EDGES` for
/// tets: lower local index first.
const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// Build the 3×3 face contribution `S^face_{ij} = ∫_T N_i · N_j dA`
/// using 1-point centroid quadrature on the first-order Whitney 1-form
/// basis of a flat triangle, and the matching `(global_edge_idx, sign)`
/// for each of the three local edges.
fn face_silver_muller_block(
    tri: &[u32; 3],
    v: &[[f64; 3]; 3],
    edge_lookup: &HashMap<(u32, u32), u32>,
) -> ([[f64; 3]; 3], [(u32, i8); 3]) {
    // Triangle edges in 3D and the unit outward normal direction (raw
    // cross product gives 2·area * n_hat). We never need the actual n
    // for the integrand (rank-reducing identity `(n × u) · (n × v) = u · v`
    // for in-plane u, v) but we DO need the area.
    let e10 = sub3(v[1], v[0]);
    let e20 = sub3(v[2], v[0]);
    let cross = cross3(e10, e20);
    let two_area = norm3(cross);
    let area = 0.5 * two_area;
    // Unit normal — direction is the right-hand-rule one from vertex
    // ordering. Sign does not matter because the integrand only sees
    // the in-plane components of the basis.
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

    // Whitney basis at centroid: λ_a = λ_b = 1/3 for all k.
    //   N_e(c) = λ_a · ∇λ_b - λ_b · ∇λ_a = (1/3) (∇λ_b - ∇λ_a).
    // Sign + global-edge mapping comes from comparing the global node
    // tags of the local edge endpoints (lower-tag first).
    let mut basis_c: [[f64; 3]; 3] = [[0.0; 3]; 3];
    let mut edge_info: [(u32, i8); 3] = [(0, 1); 3];
    for (k, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
        // local-vertex direction la -> lb.
        let n_local = scale3(sub3(grad_lambda[lb], grad_lambda[la]), 1.0 / 3.0);
        basis_c[k] = n_local;

        // global-edge orientation: lower tag first.
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

    // S^face_{ij} ≈ area · N_i(c) · N_j(c)  (1-point centroid rule).
    let mut block = [[0.0_f64; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            block[i][j] = area * dot3(basis_c[i], basis_c[j]);
        }
    }
    (block, edge_info)
}

#[inline]
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
fn norm3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[inline]
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::TetMesh;
    use std::collections::BTreeMap;

    fn one_tet_with_face() -> (TetMesh, Vec<[u32; 3]>, Vec<i32>) {
        // Single tet with one face on the outer boundary (tag = 3).
        // Vertices: 0 at origin, 1, 2 in z=0 plane, 3 above.
        let nodes = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let tets = vec![[0u32, 1, 2, 3]];
        let mesh = TetMesh {
            nodes,
            tets,
            physical_groups: BTreeMap::new(),
        };
        // Outer face: the z=0 triangle (vertices 0,1,2). Tag 3 = outer.
        let tris = vec![[0u32, 1, 2]];
        let tags = vec![3i32];
        (mesh, tris, tags)
    }

    #[test]
    fn surface_matrix_is_symmetric() {
        let (mesh, tris, tags) = one_tet_with_face();
        let edges = mesh.edges();
        let s = assemble_silver_muller_surface(&mesh, &tris, &tags, 3, &edges);
        let n = s.nrows();
        let mut max_asym = 0.0_f64;
        for i in 0..n {
            for j in 0..n {
                let d = (s[(i, j)] - s[(j, i)]).abs();
                if d > max_asym {
                    max_asym = d;
                }
            }
        }
        assert!(max_asym < 1e-12, "S not symmetric: max |S-Sᵀ| = {max_asym}");
    }

    #[test]
    fn surface_matrix_is_zero_when_no_tagged_faces() {
        let (mesh, tris, _tags) = one_tet_with_face();
        // All tags zero — nothing matches outer_tag=3.
        let tags = vec![0i32];
        let edges = mesh.edges();
        let s = assemble_silver_muller_surface(&mesh, &tris, &tags, 3, &edges);
        let n = s.nrows();
        for i in 0..n {
            for j in 0..n {
                assert_eq!(s[(i, j)], 0.0);
            }
        }
    }

    #[test]
    fn surface_matrix_is_nonzero_on_face_edges() {
        let (mesh, tris, tags) = one_tet_with_face();
        let edges = mesh.edges();
        let s = assemble_silver_muller_surface(&mesh, &tris, &tags, 3, &edges);

        // Find the three edges of the outer triangle in the global edge
        // table. Their diagonal entries S_{ee} must be strictly positive
        // (it's ∫ N_e · N_e, a strict L² norm on a non-degenerate face).
        let face_edges = [[0u32, 1], [0, 2], [1, 2]];
        for fe in face_edges.iter() {
            let idx = edges
                .iter()
                .position(|e| e == fe)
                .expect("face edge missing from global table");
            assert!(
                s[(idx, idx)] > 0.0,
                "diagonal S[{idx},{idx}] = {} must be positive",
                s[(idx, idx)]
            );
        }

        // Non-face edges (touching v3) must have zero diagonal.
        for fe in &[[0u32, 3], [1, 3], [2, 3]] {
            let idx = edges
                .iter()
                .position(|e| e == fe)
                .expect("non-face edge missing");
            assert_eq!(s[(idx, idx)], 0.0, "non-face diagonal must be zero");
        }
    }
}
