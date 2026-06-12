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
//! triangle (in-plane gradients of barycentric coords). The integrand
//! `N_i · N_j` is **degree-2 polynomial** in barycentric coordinates
//! (each Whitney basis is degree-1, the inner product is degree-2),
//! so we evaluate it with the **3-point edge-midpoint quadrature**
//! (Hammer-Stroud, degree-2 exact):
//!
//! ```text
//! ∫_T f(λ) dA ≈ (area / 3) · [f(m_01) + f(m_02) + f(m_12)]
//! ```
//!
//! where `m_ab` is the midpoint of local edge `(v_a, v_b)`. This rule
//! is degree-2 exact on the standard reference triangle (vs. the
//! 1-point centroid rule shipped in PR #34, which is only degree-1
//! exact and produced per-edge biases of factor 5/6 on the diagonal
//! `(0,1)` and 2/3 on `(1,2)` — see issue #35). The implementation
//! is verified against the closed-form face mass on the unit right
//! triangle to f64 precision; see the `face_mass_matches_analytic`
//! unit test below.
//!
//! # Why the BAC-CAB rank reduction is valid here
//!
//! For first-order Nédélec on a **flat** triangle, the basis traces
//! `N_e` lie entirely in the triangle plane: `N_e = λ_a ∇λ_b - λ_b ∇λ_a`,
//! and both `∇λ_a` and `∇λ_b` are tangent to the face (they're the
//! 2D gradients of barycentric coords). With `u, v` both tangent and
//! `n` the unit normal, `n·u = n·v = 0`, so
//! `(n × u) · (n × v) = (n·n)(u·v) - (n·u)(n·v) = u · v`. This
//! identity fails for curved faces (where the basis acquires a
//! normal component); we only use flat-faceted meshes.
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
/// 3-point edge-midpoint quadrature (Hammer-Stroud, degree-2 exact)
/// per face — see module docs. The 3×3 face contribution is built
/// dense and scattered with the global edge sign on each axis.
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
    let selected: Vec<[u32; 3]> = boundary_triangles
        .iter()
        .zip(boundary_tags.iter())
        .filter(|(_, &tag)| tag == outer_tag)
        .map(|(tri, _)| *tri)
        .collect();
    assemble_surface_mass(mesh, &selected, edges)
}

/// Assemble the tangential-trace surface mass
/// `S_{ij} = ∮_Γ (n × N_i) · (n × N_j) dS = ∮_Γ N_i · N_j dS`
/// over an explicit list of boundary triangles (no tag filtering).
///
/// This is the tag-free core of [`assemble_silver_muller_surface`] and
/// is shared by every impedance-type boundary condition that is a
/// complex multiple of the same surface integral:
///
/// - **Silver-Müller** (issue #27): coefficient `+j k₀ / η₀` (natural
///   units `η₀ = 1`, so `+j k₀ S`).
/// - **Leontovich surface impedance** (Epic #193, issue #204):
///   coefficient `+j ω / Z_s(ω)` — see
///   [`crate::driven::driven_solve_with_surface_impedance`].
///
/// Returns a real, symmetric `[n_edges, n_edges]` dense matrix whose
/// only non-zero rows/columns are the edges of the listed triangles.
/// The caller applies the complex coefficient at solve time so the
/// kernel stays purely `f64` (and, for ω-dependent coefficients, so the
/// matrix can be assembled once and rescaled per frequency).
///
/// # Panics
///
/// Panics if a triangle edge does not appear in `edges` — i.e. if the
/// triangles are not faces of the tet mesh whose edge table was passed.
pub fn assemble_surface_mass(
    mesh: &TetMesh,
    triangles: &[[u32; 3]],
    edges: &[[u32; 2]],
) -> Mat<f64> {
    let n_edges = edges.len();
    let mut s = Mat::<f64>::zeros(n_edges, n_edges);
    for (r, c, v) in assemble_surface_mass_triplets(mesh, triangles, edges) {
        s[(r, c)] += v;
    }
    s
}

/// Sparse-triplet core of [`assemble_surface_mass`] (issue #218): the
/// same face-block-wise kernel, returned as signed `(row, col, value)`
/// triplets over global edge indices instead of a dense
/// `[n_edges, n_edges]` matrix (which costs O(n_edges²) memory —
/// ~23.7 GB at the 54 k-edge spiral benchmark fixture).
///
/// Duplicate `(row, col)` entries (edges shared by adjacent faces) are
/// **not** summed — the caller accumulates them (e.g. faer's
/// `try_new_from_triplets`, or a pattern-slot scatter as in
/// [`crate::driven::DrivenOperator`]). Triplets appear in face order,
/// so summing them reproduces the dense accumulation order exactly.
/// Same convention as
/// [`crate::lumped_port::assemble_port_surface_mass`].
///
/// # Panics
///
/// Panics if a triangle edge does not appear in `edges` — i.e. if the
/// triangles are not faces of the tet mesh whose edge table was passed.
pub fn assemble_surface_mass_triplets(
    mesh: &TetMesh,
    triangles: &[[u32; 3]],
    edges: &[[u32; 2]],
) -> Vec<(usize, usize, f64)> {
    // Build edge lookup: (lo, hi) -> global edge index.
    let mut edge_lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(edges.len());
    for (idx, e) in edges.iter().enumerate() {
        edge_lookup.insert((e[0], e[1]), idx as u32);
    }

    let mut triplets = Vec::with_capacity(triangles.len() * 9);
    for tri in triangles.iter() {
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
                triplets.push((gi as usize, gj as usize, val));
            }
        }
    }

    triplets
}

/// Local edge order on a triangle face. Mirrors `TET_LOCAL_EDGES` for
/// tets: lower local index first.
const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// Build the 3×3 face contribution `S^face_{ij} = ∫_T N_i · N_j dA`
/// using the 3-point edge-midpoint quadrature (Hammer-Stroud, degree-2
/// exact) on the first-order Whitney 1-form basis of a flat triangle,
/// and the matching `(global_edge_idx, sign)` for each of the three
/// local edges.
///
/// The integrand `N_i · N_j` is exactly quadratic in barycentric
/// coordinates, so the 3-point edge-midpoint rule reproduces the
/// analytic face mass to floating-point precision. See the
/// `face_mass_matches_analytic` unit test.
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

    // Compute the global-edge mapping and orientation sign for each
    // local triangle edge. Sign + global-edge mapping comes from
    // comparing the global node tags of the local edge endpoints
    // (lower-tag first), matching the volume assembly convention.
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

    // 3-point edge-midpoint quadrature (Hammer-Stroud, degree-2 exact):
    //   ∫_T f dA ≈ (area/3) · [f(m_01) + f(m_02) + f(m_12)]
    // where m_ab is the midpoint of local edge (la, lb). In barycentric
    // coords this is λ_la = λ_lb = 1/2, λ_other = 0.
    //
    // We evaluate the 3 Whitney basis functions at each of the 3 edge
    // midpoints, then sum the outer products.
    //
    // BARYCENTRIC_MIDPOINTS[q][k] = value of λ_k at quadrature point q.
    // q=0 → midpoint of edge (0,1), q=1 → edge (0,2), q=2 → edge (1,2).
    const BARYCENTRIC_MIDPOINTS: [[f64; 3]; 3] = [
        [0.5, 0.5, 0.0], // m_01
        [0.5, 0.0, 0.5], // m_02
        [0.0, 0.5, 0.5], // m_12
    ];

    let weight = area / 3.0;
    let mut block = [[0.0_f64; 3]; 3];
    for lam in BARYCENTRIC_MIDPOINTS.iter() {
        // Evaluate the three Whitney basis vectors at this midpoint.
        // N_e(λ) = λ_la · ∇λ_lb − λ_lb · ∇λ_la for local edge (la, lb).
        let mut basis_q: [[f64; 3]; 3] = [[0.0; 3]; 3];
        for (k, &(la, lb)) in TRI_LOCAL_EDGES.iter().enumerate() {
            let term_a = scale3(grad_lambda[lb], lam[la]);
            let term_b = scale3(grad_lambda[la], lam[lb]);
            basis_q[k] = sub3(term_a, term_b);
        }
        for i in 0..3 {
            for j in 0..3 {
                block[i][j] += weight * dot3(basis_q[i], basis_q[j]);
            }
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

    #[test]
    fn face_mass_matches_analytic() {
        // Regression test for issue #35: verify the 3×3 face contribution
        // matches the hand-derived closed-form `∫_T N_i · N_j dA` to
        // f64 precision on the unit right triangle T with vertices
        // v_0 = (0,0,0), v_1 = (1,0,0), v_2 = (0,1,0), area = 1/2.
        //
        // Whitney basis (local edge directions la → lb):
        //   N_01 = λ_0 ∇λ_1 - λ_1 ∇λ_0 = (1 - y, x, 0)
        //   N_02 = λ_0 ∇λ_2 - λ_2 ∇λ_0 = (y, 1 - x, 0)
        //   N_12 = λ_1 ∇λ_2 - λ_2 ∇λ_1 = (-y, x, 0)
        //
        // Direct integration (using ∫_T λ_i^a λ_j^b λ_k^c dA =
        // 2·area · (a!b!c!) / (a+b+c+2)! ) gives the closed-form
        // face-mass matrix in the local-edge order [(0,1), (0,2), (1,2)]:
        //
        //   S_analytic = [ 1/3   1/6   0   ]
        //                [ 1/6   1/3   0   ]
        //                [ 0     0     1/6 ]
        //
        // Reference: Bossavit, "Whitney forms: a class of finite
        // elements for three-dimensional computations in
        // electromagnetism", IEE Proc. A 135 (1988); Hiptmair, "Finite
        // elements in computational electromagnetism", Acta Numerica
        // 11 (2002), §3.5 (Whitney 1-form face mass on a flat
        // triangle). With the BAC-CAB rank reduction the surface
        // integrand `(n × N_i) · (n × N_j)` reduces to `N_i · N_j`
        // on flat faces, so S = M^face.
        //
        // The pre-#35 1-point centroid rule produced:
        //   S_centroid = [ 5/18  ...   ...  ]  — diagonal (0,1) off by 5/6
        //                [ ...   5/18  ...  ]
        //                [ ...   ...   1/9  ]  — diagonal (1,2) off by 2/3
        //
        // The new 3-point edge-midpoint rule is degree-2 exact on
        // quadratics in barycentric coords, so it reproduces
        // S_analytic to f64 precision.
        let nodes = vec![
            [0.0_f64, 0.0, 0.0], // v_0
            [1.0, 0.0, 0.0],     // v_1
            [0.0, 1.0, 0.0],     // v_2
            [0.0, 0.0, 1.0],     // v_3 (off-face dummy so tet is non-degenerate)
        ];
        let tets = vec![[0u32, 1, 2, 3]];
        let mesh = TetMesh {
            nodes,
            tets,
            physical_groups: BTreeMap::new(),
        };
        let tris = vec![[0u32, 1, 2]];
        let tags = vec![3i32];
        let edges = mesh.edges();
        let s = assemble_silver_muller_surface(&mesh, &tris, &tags, 3, &edges);

        // Locate the three face-edge rows/cols in the global edge table.
        let i01 = edges
            .iter()
            .position(|e| e == &[0u32, 1])
            .expect("(0,1) edge missing");
        let i02 = edges
            .iter()
            .position(|e| e == &[0u32, 2])
            .expect("(0,2) edge missing");
        let i12 = edges
            .iter()
            .position(|e| e == &[1u32, 2])
            .expect("(1,2) edge missing");

        // All global edges (0,1), (0,2), (1,2) have ga<gb, so the
        // orientation sign is +1 for each — the assembled entry equals
        // the unsigned analytic value exactly.
        let analytic = [
            (i01, i01, 1.0 / 3.0),
            (i02, i02, 1.0 / 3.0),
            (i12, i12, 1.0 / 6.0),
            (i01, i02, 1.0 / 6.0),
            (i02, i01, 1.0 / 6.0),
            (i01, i12, 0.0),
            (i12, i01, 0.0),
            (i02, i12, 0.0),
            (i12, i02, 0.0),
        ];
        let tol = 1e-14;
        for (i, j, expected) in analytic.iter() {
            let got = s[(*i, *j)];
            assert!(
                (got - expected).abs() < tol,
                "S[{i},{j}] = {got:.17e}, expected {expected:.17e} \
                 (diff {:.3e})",
                (got - expected).abs()
            );
        }

        // Sanity: any entry not in the face-edge sub-block must be zero.
        let face_rows = [i01, i02, i12];
        for r in 0..s.nrows() {
            for c in 0..s.ncols() {
                if face_rows.contains(&r) && face_rows.contains(&c) {
                    continue;
                }
                assert_eq!(s[(r, c)], 0.0, "off-face entry S[{r},{c}] non-zero");
            }
        }
    }
}
