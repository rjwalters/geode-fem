//! 2D transverse modal eigensolver for waveguide port cross-sections
//! (Epic #234, Phase 1, issue #235).
//!
//! Given a 2-D triangle mesh representing the cross-section of a
//! cylindrical waveguide whose axis is the `z`-direction, solve the
//! transverse vector eigenproblem
//!
//! ```text
//! ∇_t × ∇_t × e_t = k_c² e_t,    e_t × n = 0 on the PEC wall,
//! ```
//!
//! producing the cutoff wavenumber `k_c` and the discrete transverse
//! Whitney/Nédélec edge-DOF profile of the supported propagating /
//! evanescent modes. For a guided angular frequency `ω`, the propagation
//! constant of the corresponding mode is
//!
//! ```text
//! β(ω) = √(ω²/c² − k_c²)    (real → propagating, imaginary → evanescent).
//! ```
//!
//! # Discretisation
//!
//! The transverse vector field is discretised in the first-order
//! Whitney/Nédélec edge-element space on triangles. For an edge
//! `i = (a, b)` of a triangle `T` with vertex barycentrics `λ_a, λ_b`,
//!
//! ```text
//! N_i(x) = λ_a ∇λ_b − λ_b ∇λ_a,
//! ∇ × N_i = 2 (∇λ_a × ∇λ_b) ẑ   (scalar in 2D).
//! ```
//!
//! With `G_pq = ∇λ_p · ∇λ_q` the gradient gram and `A` the triangle area,
//! the local 3×3 curl-curl and mass matrices admit closed-form entries
//! (no quadrature needed):
//!
//! ```text
//! K_ij = 4 A (G_aa G_bb − G_ab²)              if i = (a,b) and j = (a,b)
//!        4 A (G_ac G_bd − G_ad G_bc)          general (a,b), (c,d)
//! M_ij = (A/12)[ (1+δ_ac) G_bd − (1+δ_ad) G_bc
//!              − (1+δ_bc) G_ad + (1+δ_bd) G_ac ]
//! ```
//!
//! (the 2-D analogue of the 3-D tet formulas in `crate::nedelec`).
//!
//! # Edge / sign convention
//!
//! Edges are globally oriented from the lower-tagged endpoint to the
//! higher-tagged endpoint (same convention as the 3-D Nédélec module).
//! Within a triangle, local edges are listed in the canonical order
//! `(v0,v1)`, `(v0,v2)`, `(v1,v2)`. A per-triangle sign of `±1` per
//! local edge records whether the local orientation agrees with the
//! global one; rows and columns of the local 3×3 matrices are flipped
//! by `s_i s_j` before scatter into the global system.
//!
//! # Spurious / gradient nullspace
//!
//! The discrete curl-curl operator has a large gradient nullspace
//! `kernel(K) = image(d⁰)` (Whitney 1-forms include `∇φ` for every
//! `φ ∈ H¹_0`). Numerically these appear as a cluster of near-zero
//! eigenvalues that must be filtered before the physical modal spectrum
//! is read off; the spurious-mode count equals the number of interior
//! nodes after PEC reduction. This mirrors the 3-D path in
//! `nedelec_assembly::spurious_dim_from_derham`.

use faer::Mat;

use crate::eigen::{EigenError, EigenSolver, FaerDenseEigensolver};

/// Canonical local edge ordering on a triangle.
///
/// For a triangle with local vertices `(v0, v1, v2)`, the three edges in
/// canonical order are `(v0,v1), (v0,v2), (v1,v2)`. Mirrors
/// `crate::mesh::TET_LOCAL_EDGES` for the 2-D case.
pub const TRI_LOCAL_EDGES: [(usize, usize); 3] = [(0, 1), (0, 2), (1, 2)];

/// CPU-side triangle mesh produced by the in-memory rectangular
/// generator or a 2-D port-cross-section fixture loader.
///
/// Node indices are 0-based linear indices into `nodes`. Each node is
/// stored as `[x, y]` (the cross-section is parameterised in 2-D
/// regardless of the embedding 3-D port plane).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriMesh {
    /// Node coordinates, `nodes[i] = [x, y]`.
    pub nodes: Vec<[f64; 2]>,
    /// Triangle connectivity: each triangle's three 0-based node indices.
    pub tris: Vec<[u32; 3]>,
}

impl TriMesh {
    pub fn n_nodes(&self) -> usize {
        self.nodes.len()
    }

    pub fn n_tris(&self) -> usize {
        self.tris.len()
    }

    /// Build the deduplicated, globally-oriented edge list of this mesh.
    ///
    /// Each edge `[a, b]` is stored with `a < b` (lower-tagged endpoint
    /// first). `edges.len()` is the global Whitney/Nédélec system size.
    pub fn edges(&self) -> Vec<[u32; 2]> {
        use std::collections::BTreeSet;
        let mut set: BTreeSet<(u32, u32)> = BTreeSet::new();
        for tri in &self.tris {
            for &(la, lb) in TRI_LOCAL_EDGES.iter() {
                let a = tri[la];
                let b = tri[lb];
                let (lo, hi) = if a < b { (a, b) } else { (b, a) };
                set.insert((lo, hi));
            }
        }
        set.into_iter().map(|(a, b)| [a, b]).collect()
    }

    /// For each triangle, return the three `(global_edge_index, sign)`
    /// pairs in the canonical local-edge order ([`TRI_LOCAL_EDGES`]).
    ///
    /// `sign` is `+1` if the local edge orientation agrees with the
    /// global edge direction (lower global node → higher global node),
    /// and `-1` otherwise.
    pub fn tri_edges(&self) -> Vec<[(u32, i8); 3]> {
        use std::collections::HashMap;
        let edges = self.edges();
        let mut lookup: HashMap<(u32, u32), u32> = HashMap::with_capacity(edges.len());
        for (idx, e) in edges.iter().enumerate() {
            lookup.insert((e[0], e[1]), idx as u32);
        }

        self.tris
            .iter()
            .map(|tri| {
                let mut out = [(0u32, 1i8); 3];
                for (slot, &(la, lb)) in out.iter_mut().zip(TRI_LOCAL_EDGES.iter()) {
                    let a = tri[la];
                    let b = tri[lb];
                    let (lo, hi, sign) = if a < b { (a, b, 1i8) } else { (b, a, -1i8) };
                    let idx = *lookup
                        .get(&(lo, hi))
                        .expect("edge derived from triangle must be in edge table");
                    *slot = (idx, sign);
                }
                out
            })
            .collect()
    }
}

/// Generate a triangulated rectangle `[0, width] × [0, height]` with
/// `nx × ny` quads, each quad split into two right-handed triangles
/// sharing the lower-left-to-upper-right diagonal.
///
/// Produces `(nx+1)(ny+1)` nodes and `2 * nx * ny` triangles. All
/// triangles are listed counter-clockwise (positive signed area).
///
/// This is the 2-D analogue of [`crate::cube_tet_mesh`] — a programmatic
/// rectangular waveguide cross-section that doubles as our fixture for
/// the Phase-1 modal eigensolver acceptance test (analytic TE/TM oracle).
pub fn rect_tri_mesh(nx: usize, ny: usize, width: f64, height: f64) -> TriMesh {
    assert!(nx >= 1 && ny >= 1, "rect_tri_mesh requires nx, ny ≥ 1");
    let npx = nx + 1;
    let npy = ny + 1;
    let hx = width / nx as f64;
    let hy = height / ny as f64;
    let node_idx = |i: usize, j: usize| -> u32 { (i + j * npx) as u32 };

    let mut nodes = Vec::with_capacity(npx * npy);
    for j in 0..npy {
        for i in 0..npx {
            nodes.push([i as f64 * hx, j as f64 * hy]);
        }
    }

    let mut tris = Vec::with_capacity(2 * nx * ny);
    for j in 0..ny {
        for i in 0..nx {
            let c = [
                node_idx(i, j),
                node_idx(i + 1, j),
                node_idx(i + 1, j + 1),
                node_idx(i, j + 1),
            ];
            // Two CCW triangles sharing the c[0]→c[2] diagonal.
            tris.push([c[0], c[1], c[2]]);
            tris.push([c[0], c[2], c[3]]);
        }
    }

    TriMesh { nodes, tris }
}

/// Build the PEC interior-edge mask for a rectangle `[0,W] × [0,H]`:
/// an edge is **interior** (mask `true`) unless its two endpoints lie on
/// the **same wall** (i.e., the edge segment lies along the PEC
/// boundary). PEC tangential continuity is `n × E = 0` on the wall, and
/// the Whitney DOF on a wall-aligned edge is exactly the line integral
/// of `E_tangential` along the wall — so those edges (and only those)
/// are forced to zero.
///
/// A diagonal interior edge that happens to connect a node on the bottom
/// wall to one on the right wall (a corner-adjacent diagonal in the
/// structured `rect_tri_mesh`) is **not** wall-aligned and must remain
/// an interior DOF — gating those out would silently over-constrain the
/// eigenproblem.
///
/// Returns `(edges, interior_edge_mask)` aligned with [`TriMesh::edges`].
pub fn rect_pec_interior_edges(
    mesh: &TriMesh,
    width: f64,
    height: f64,
) -> (Vec<[u32; 2]>, Vec<bool>) {
    let tol = 1e-9 * width.max(height).max(1.0);
    // Per node: which walls (if any) it lies on.
    //   bit 0: x = 0   (left)
    //   bit 1: x = W   (right)
    //   bit 2: y = 0   (bottom)
    //   bit 3: y = H   (top)
    let wall_bits: Vec<u8> = mesh
        .nodes
        .iter()
        .map(|p| {
            let mut b = 0u8;
            if p[0].abs() < tol {
                b |= 1;
            }
            if (p[0] - width).abs() < tol {
                b |= 2;
            }
            if p[1].abs() < tol {
                b |= 4;
            }
            if (p[1] - height).abs() < tol {
                b |= 8;
            }
            b
        })
        .collect();
    let edges = mesh.edges();
    // An edge is wall-aligned iff its two endpoints share at least one
    // wall bit (so they are co-linear along that wall). Bitwise AND
    // captures this exactly.
    let mask = edges
        .iter()
        .map(|e| (wall_bits[e[0] as usize] & wall_bits[e[1] as usize]) == 0)
        .collect();
    (edges, mask)
}

/// Build the PEC interior-node mask for a rectangle `[0,W] × [0,H]`:
/// `true` for nodes strictly inside the open rectangle, `false` for
/// nodes on any wall.
pub fn rect_pec_interior_nodes(mesh: &TriMesh, width: f64, height: f64) -> Vec<bool> {
    let tol = 1e-9 * width.max(height).max(1.0);
    mesh.nodes
        .iter()
        .map(|p| {
            !(p[0].abs() < tol
                || (p[0] - width).abs() < tol
                || p[1].abs() < tol
                || (p[1] - height).abs() < tol)
        })
        .collect()
}

/// Closed-form local 3×3 Whitney/Nédélec stiffness (curl-curl) and mass
/// matrices for an affine triangle.
///
/// `coords` are the three vertex coordinates `[v0, v1, v2]` with each
/// vertex `[x, y]`. Returns `(k_local, m_local, signed_area)`. The
/// signed area is `((v1-v0) × (v2-v0))_z / 2` — positive for CCW vertex
/// ordering.
///
/// Rows/columns follow the canonical local-edge order
/// ([`TRI_LOCAL_EDGES`]). Sign flips for the **global** orientation are
/// the caller's responsibility (applied at assembly time).
pub fn tri_nedelec_local(coords: &[[f64; 2]; 3]) -> ([[f64; 3]; 3], [[f64; 3]; 3], f64) {
    // Edge vectors from v0.
    let e1 = [coords[1][0] - coords[0][0], coords[1][1] - coords[0][1]];
    let e2 = [coords[2][0] - coords[0][0], coords[2][1] - coords[0][1]];

    // Signed double area (det of [e1 | e2]).
    let det = e1[0] * e2[1] - e1[1] * e2[0];
    let area = 0.5 * det;
    let abs_det = det.abs();

    // Gradients of the three barycentrics (rotate edge vectors 90° and
    // divide by det). For a 2-D affine triangle:
    //   ∇λ_0 = ( (y1 - y2),  (x2 - x1) ) / det
    //   ∇λ_1 = ( (y2 - y0),  (x0 - x2) ) / det
    //   ∇λ_2 = ( (y0 - y1),  (x1 - x0) ) / det
    let grad = [
        [
            (coords[1][1] - coords[2][1]) / det,
            (coords[2][0] - coords[1][0]) / det,
        ],
        [
            (coords[2][1] - coords[0][1]) / det,
            (coords[0][0] - coords[2][0]) / det,
        ],
        [
            (coords[0][1] - coords[1][1]) / det,
            (coords[1][0] - coords[0][0]) / det,
        ],
    ];

    // Gram matrix G_pq = ∇λ_p · ∇λ_q.
    let mut gram = [[0.0_f64; 3]; 3];
    for p in 0..3 {
        for q in 0..3 {
            gram[p][q] = grad[p][0] * grad[q][0] + grad[p][1] * grad[q][1];
        }
    }

    let area_abs = 0.5 * abs_det;
    let mut k_local = [[0.0_f64; 3]; 3];
    let mut m_local = [[0.0_f64; 3]; 3];

    for (i, &(a, b)) in TRI_LOCAL_EDGES.iter().enumerate() {
        for (j, &(c, d)) in TRI_LOCAL_EDGES.iter().enumerate() {
            // Curl-curl: K_ij = 4 A (G_ac G_bd − G_ad G_bc).
            //
            // Derivation: ∇×N_i = 2 (∇λ_a × ∇λ_b)_z. The product of two
            // 2-D cross products expands as
            //   (u × v)(w × z) = (u·w)(v·z) − (u·z)(v·w),
            // so ∫ (∇×N_i)(∇×N_j) dA = 4 A [G_ac G_bd − G_ad G_bc].
            k_local[i][j] = 4.0 * area_abs * (gram[a][c] * gram[b][d] - gram[a][d] * gram[b][c]);

            // Mass: same Kronecker-delta expansion as the 3-D version
            // but with the 2-D quadrature constant (1/12 instead of
            // 1/20):
            //   ∫ λ_p λ_q dA = (A/12) (1 + δ_pq).
            let f_ac = if a == c { 2.0 } else { 1.0 };
            let f_ad = if a == d { 2.0 } else { 1.0 };
            let f_bc = if b == c { 2.0 } else { 1.0 };
            let f_bd = if b == d { 2.0 } else { 1.0 };
            m_local[i][j] = (area_abs / 12.0)
                * (f_ac * gram[b][d] - f_ad * gram[b][c] - f_bc * gram[a][d] + f_bd * gram[a][c]);
        }
    }

    (k_local, m_local, area)
}

/// Assemble dense global Whitney/Nédélec stiffness `K` (curl-curl) and
/// mass `M` for a 2-D triangle mesh.
///
/// Returns `(K, M)` of size `[n_edges, n_edges]`. Triangle-local 3×3
/// blocks are scattered with the per-DOF sign that records the local-vs-
/// global edge orientation (`crate::waveguide_modes::TriMesh::tri_edges`).
pub fn assemble_2d_nedelec(mesh: &TriMesh) -> (Mat<f64>, Mat<f64>) {
    let edges = mesh.edges();
    let n_edges = edges.len();
    let tri_edges = mesh.tri_edges();

    let mut k = Mat::<f64>::zeros(n_edges, n_edges);
    let mut m = Mat::<f64>::zeros(n_edges, n_edges);

    for (tri, row) in mesh.tris.iter().zip(tri_edges.iter()) {
        let coords = [
            mesh.nodes[tri[0] as usize],
            mesh.nodes[tri[1] as usize],
            mesh.nodes[tri[2] as usize],
        ];
        let (k_local, m_local, signed_area) = tri_nedelec_local(&coords);
        assert!(
            signed_area > 0.0,
            "rect_tri_mesh / TriMesh must produce CCW triangles; got signed area {signed_area}"
        );
        for i in 0..3 {
            let (gi, si) = row[i];
            for j in 0..3 {
                let (gj, sj) = row[j];
                let s = (si as f64) * (sj as f64);
                k[(gi as usize, gj as usize)] += s * k_local[i][j];
                m[(gi as usize, gj as usize)] += s * m_local[i][j];
            }
        }
    }

    (k, m)
}

/// Restrict `K` and `M` to interior edges (PEC reduction).
pub fn apply_pec_2d(
    k: &Mat<f64>,
    m: &Mat<f64>,
    interior_edge_mask: &[bool],
) -> (Mat<f64>, Mat<f64>) {
    assert_eq!(k.nrows(), interior_edge_mask.len());
    let interior: Vec<usize> = interior_edge_mask
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b { Some(i) } else { None })
        .collect();
    let dim = interior.len();
    let k_int = Mat::<f64>::from_fn(dim, dim, |i, j| k[(interior[i], interior[j])]);
    let m_int = Mat::<f64>::from_fn(dim, dim, |i, j| m[(interior[i], interior[j])]);
    (k_int, m_int)
}

/// Algebraically-correct spurious-mode dimension for the 2-D Nédélec
/// curl-curl operator on a triangle mesh after PEC reduction.
///
/// Equals `rank(d⁰_interior)` where `d⁰_interior` is the discrete
/// gradient restricted to interior edges × interior nodes. The de-Rham
/// identity `kernel(K) = image(d⁰)` holds in the 2-D Whitney pair too,
/// so this is exactly the count of near-zero eigenvalues of the
/// generalized pencil `(K_int, M_int)`. Mirrors
/// `nedelec_assembly::spurious_dim_from_derham` for the 3-D case.
pub fn spurious_dim_2d(
    mesh: &TriMesh,
    interior_edge_mask: &[bool],
    interior_node_mask: &[bool],
) -> usize {
    let d0 = restrict_gradient_dense_2d(mesh, interior_edge_mask, interior_node_mask);
    rank_via_svd_2d(&d0, 1e-12)
}

/// Build the dense interior×interior restriction of the de-Rham `d⁰`
/// operator (discrete gradient) directly from the 2-D edge list.
///
/// Each edge contributes `±1` at its two endpoint columns, filtered to
/// `edge_mask[i] && node_mask[a] && node_mask[b]`.
pub fn restrict_gradient_dense_2d(
    mesh: &TriMesh,
    edge_mask: &[bool],
    node_mask: &[bool],
) -> Mat<f64> {
    let mut node_to_interior: Vec<Option<usize>> = Vec::with_capacity(node_mask.len());
    let mut n_interior_nodes = 0usize;
    for &b in node_mask {
        if b {
            node_to_interior.push(Some(n_interior_nodes));
            n_interior_nodes += 1;
        } else {
            node_to_interior.push(None);
        }
    }
    let mut edge_to_interior: Vec<Option<usize>> = Vec::with_capacity(edge_mask.len());
    let mut n_interior_edges = 0usize;
    for &b in edge_mask {
        if b {
            edge_to_interior.push(Some(n_interior_edges));
            n_interior_edges += 1;
        } else {
            edge_to_interior.push(None);
        }
    }
    let edges = mesh.edges();
    assert_eq!(edges.len(), edge_mask.len());
    assert_eq!(node_mask.len(), mesh.n_nodes());

    let mut d0 = Mat::<f64>::zeros(n_interior_edges, n_interior_nodes);
    for (edge_idx, &[a, b]) in edges.iter().enumerate() {
        let Some(row) = edge_to_interior[edge_idx] else {
            continue;
        };
        if let Some(col) = node_to_interior[a as usize] {
            d0[(row, col)] = -1.0;
        }
        if let Some(col) = node_to_interior[b as usize] {
            d0[(row, col)] = 1.0;
        }
    }
    d0
}

fn rank_via_svd_2d(d0: &Mat<f64>, threshold_rel: f64) -> usize {
    let sigmas = d0
        .as_ref()
        .singular_values()
        .expect("dense SVD of d⁰_interior failed");
    let sigma_max = sigmas.first().copied().unwrap_or(0.0);
    let threshold = threshold_rel * sigma_max;
    sigmas.iter().filter(|&&s| s > threshold).count()
}

/// A single transverse mode of a waveguide cross-section.
#[derive(Debug, Clone)]
pub struct WaveguideMode {
    /// Cutoff wavenumber `k_c` (rad / length). Modes with `k_c = 0` are
    /// TEM (a guided constant field, only present in multiply-connected
    /// cross-sections — not the simply-connected rectangular case here).
    pub k_c: f64,
    /// The corresponding generalized eigenvalue `λ = k_c²`. Returned
    /// alongside `k_c` because the eigensolver yields `λ` directly and
    /// callers sometimes want both.
    pub lambda: f64,
}

impl WaveguideMode {
    /// Propagation constant `β` at angular frequency `ω` (with `c = ω/k`).
    ///
    /// Returns `β = √(ω²/c² − k_c²)` for `ω/c > k_c` (propagating) and
    /// `β = i √(k_c² − ω²/c²)` for `ω/c < k_c` (evanescent); the latter
    /// case is reported as `(0.0, β_im)`, the former as `(β_re, 0.0)`.
    pub fn beta(&self, omega: f64, c: f64) -> (f64, f64) {
        let k0 = omega / c;
        let arg = k0 * k0 - self.k_c * self.k_c;
        if arg >= 0.0 {
            (arg.sqrt(), 0.0)
        } else {
            (0.0, (-arg).sqrt())
        }
    }
}

/// Compute the lowest `n_modes` transverse modal cutoffs of the
/// rectangular waveguide cross-section meshed by `mesh`, with PEC walls
/// on the rectangle `[0,W] × [0,H]`.
///
/// Returns the modes ordered by increasing `k_c` (after dropping the
/// gradient-nullspace cluster).
pub fn solve_rect_waveguide_modes(
    mesh: &TriMesh,
    width: f64,
    height: f64,
    n_modes: usize,
) -> Result<Vec<WaveguideMode>, EigenError> {
    let (k_global, m_global) = assemble_2d_nedelec(mesh);
    let (_edges, interior_edges) = rect_pec_interior_edges(mesh, width, height);
    let interior_nodes = rect_pec_interior_nodes(mesh, width, height);
    let (k_int, m_int) = apply_pec_2d(&k_global, &m_global, &interior_edges);

    let spurious = spurious_dim_2d(mesh, &interior_edges, &interior_nodes);
    let n_request = spurious + n_modes;
    let lambdas =
        FaerDenseEigensolver.smallest_eigenvalues(k_int.as_ref(), m_int.as_ref(), n_request)?;

    // Skip the gradient null-cluster (the lowest `spurious` eigenvalues).
    // Remaining eigenvalues are the squared cutoff wavenumbers.
    let modes = lambdas
        .into_iter()
        .skip(spurious)
        .take(n_modes)
        .map(|lambda| {
            // Clamp tiny negatives (round-off) at zero before sqrt.
            let lam_pos = lambda.max(0.0);
            WaveguideMode {
                k_c: lam_pos.sqrt(),
                lambda,
            }
        })
        .collect();
    Ok(modes)
}

/// Analytic TE/TM cutoff wavenumbers for a rectangular metallic
/// waveguide of inner dimensions `a × b` (with `a ≥ b` by convention).
///
/// ```text
/// k_c(m, n) = √((m π / a)² + (n π / b)²)
/// ```
///
/// where the dominant TE₁₀ mode has `k_c = π / a` (so `f_c = c / (2 a)`),
/// followed by TE₂₀ (`2 π / a`), TE₀₁ (`π / b`), and the lowest TM mode
/// TM₁₁ at `√((π/a)² + (π/b)²)`.
///
/// `family` is informational only (kind label) — the cutoff formula is
/// the same for TE and TM, and the lowest TM mode requires both `m ≥ 1`
/// and `n ≥ 1` (TE allows `m` or `n` to be zero but not both).
pub fn rect_waveguide_cutoff(m: u32, n: u32, a: f64, b: f64) -> f64 {
    let mx = (m as f64) * std::f64::consts::PI / a;
    let ny = (n as f64) * std::f64::consts::PI / b;
    (mx * mx + ny * ny).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_tri_mesh_smoke() {
        let mesh = rect_tri_mesh(2, 2, 1.0, 1.0);
        assert_eq!(mesh.n_nodes(), 9);
        assert_eq!(mesh.n_tris(), 8);
        // Edge count = (nx+1)*ny + nx*(ny+1) + nx*ny  (horizontal +
        // vertical + diagonals) = 3*2 + 2*3 + 2*2 = 16.
        assert_eq!(mesh.edges().len(), 16);
    }

    #[test]
    fn tri_local_signed_area_positive_for_ccw() {
        let coords = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let (_, _, area) = tri_nedelec_local(&coords);
        assert!((area - 0.5).abs() < 1e-15);
    }

    #[test]
    fn tri_local_matrices_symmetric() {
        // Off-axis affine triangle.
        let coords = [[0.3, -0.1], [1.2, 0.4], [0.7, 1.1]];
        let (k, m, _) = tri_nedelec_local(&coords);
        for i in 0..3 {
            for j in (i + 1)..3 {
                assert!(
                    (k[i][j] - k[j][i]).abs() < 1e-12,
                    "K not symmetric: ({i},{j})"
                );
                assert!(
                    (m[i][j] - m[j][i]).abs() < 1e-12,
                    "M not symmetric: ({i},{j})"
                );
            }
        }
    }

    #[test]
    fn pec_mask_excludes_only_boundary_edges() {
        let mesh = rect_tri_mesh(3, 3, 1.5, 0.7);
        let (edges, mask) = rect_pec_interior_edges(&mesh, 1.5, 0.7);
        assert_eq!(edges.len(), mask.len());
        let n_interior = mask.iter().filter(|&&b| b).count();
        let n_pec = mask.len() - n_interior;
        assert!(n_interior > 0);
        assert!(n_pec > 0);
        // Sanity: the rectangle boundary has 2*(3+3) = 12 edges in this
        // structured mesh (3 horizontal × 2 walls + 3 vertical × 2 walls).
        assert_eq!(n_pec, 12);
    }

    #[test]
    fn waveguide_mode_beta_branch() {
        let m = WaveguideMode {
            k_c: 1.0,
            lambda: 1.0,
        };
        let c = 1.0;
        // Below cutoff (ω/c < k_c).
        let (re, im) = m.beta(0.5, c);
        assert!(re == 0.0);
        assert!((im - (1.0 - 0.25_f64).sqrt()).abs() < 1e-15);
        // Above cutoff.
        let (re, im) = m.beta(2.0, c);
        assert!(im == 0.0);
        assert!((re - (4.0 - 1.0_f64).sqrt()).abs() < 1e-15);
    }

    #[test]
    fn rect_waveguide_cutoff_te10_te20() {
        let a = 22.86e-3; // WR-90 inner width.
        let b = 10.16e-3;
        let pi = std::f64::consts::PI;
        assert!((rect_waveguide_cutoff(1, 0, a, b) - pi / a).abs() < 1e-12);
        assert!((rect_waveguide_cutoff(2, 0, a, b) - 2.0 * pi / a).abs() < 1e-12);
        assert!((rect_waveguide_cutoff(0, 1, a, b) - pi / b).abs() < 1e-12);
    }

    /// Whitney/Nédélec rectangular-waveguide cutoff regression: the
    /// lowest four eigenmodes of the curl-curl pencil on a 16×8 mesh
    /// pair to TE₁₀, TE₂₀, TE₀₁, and TM₁₁ within a few percent. This
    /// is the load-bearing Phase-1 acceptance test (#235) — once it
    /// passes the eigensolver is consuming a 2-D triangle mesh, doing
    /// the de-Rham nullspace filter, and producing modal cutoffs that
    /// match the analytic oracle.
    #[test]
    fn rect_waveguide_te10_matches_analytic() {
        // WR-90-ish dimensions in arbitrary length units.
        let a = 2.0;
        let b = 1.0;
        let mesh = rect_tri_mesh(16, 8, a, b);
        let modes = solve_rect_waveguide_modes(&mesh, a, b, 5).expect("modal eigensolve");

        // TE₁₀ at k_c = π/a is the dominant mode.
        let pi = std::f64::consts::PI;
        let kc_te10 = pi / a;
        let rel_err_te10 = (modes[0].k_c - kc_te10).abs() / kc_te10;
        eprintln!(
            "modal cutoffs: TE10 fem k_c = {:.4} (analytic {:.4}, rel err {:.2}%)",
            modes[0].k_c,
            kc_te10,
            100.0 * rel_err_te10
        );
        assert!(
            rel_err_te10 < 0.03,
            "TE10 cutoff disagreement: fem = {:.4}, analytic = {:.4} ({:.2}%)",
            modes[0].k_c,
            kc_te10,
            100.0 * rel_err_te10
        );

        // The remaining four FEM cutoffs should pair to the lowest
        // analytic catalog roots within 5 %. We pair by closest k_c.
        let catalog: Vec<(u32, u32, f64)> = (0..=3)
            .flat_map(|m| (0..=3).map(move |n| (m as u32, n as u32)))
            .filter(|&(m, n)| !(m == 0 && n == 0))
            .map(|(m, n)| (m, n, rect_waveguide_cutoff(m, n, a, b)))
            .collect();

        for (i, mode) in modes.iter().enumerate().take(4) {
            let closest = catalog
                .iter()
                .min_by(|a, b| {
                    (a.2 - mode.k_c)
                        .abs()
                        .partial_cmp(&(b.2 - mode.k_c).abs())
                        .unwrap()
                })
                .unwrap();
            let rel_err = (mode.k_c - closest.2).abs() / closest.2;
            eprintln!(
                "  mode[{i}]: k_c = {:.4}  →  TE/TM ({},{}) analytic k_c = {:.4} ({:.2}%)",
                mode.k_c,
                closest.0,
                closest.1,
                closest.2,
                100.0 * rel_err
            );
            assert!(
                rel_err < 0.05,
                "mode[{i}] k_c = {:.4} fails to pair to any (m,n) within 5 %",
                mode.k_c
            );
        }
    }
}
