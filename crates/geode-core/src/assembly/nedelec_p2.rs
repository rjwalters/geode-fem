//! Second-order (`p=2`) Nédélec **global** assembly on a tetrahedral mesh
//! (Epic #475 parity gap #3, Epic #569; issue #616, follow-on to #613).
//!
//! [`crate::elements::nedelec_p2`] shipped the 20-DOF element (basis, curls,
//! degree-≥4 quadrature, and — the hard part — the ascending-global-vertex
//! orientation convention). This module does the global wiring that #613
//! deliberately deferred: a global DOF numbering, an order-aware `K`/`M`
//! assembler, and a quadrature-sampled RHS, all in plain `f64`/faer sparse
//! form (no Burn backend). It is the substrate the opt-in `p=2` driven
//! forward path ([`crate::driven::solve::driven_solve_p2`]) and the `p=2`
//! material adjoint ([`crate::driven::adjoint::driven_material_adjoint_gradient_p2`])
//! build on.
//!
//! # Global DOF numbering (`edges×2 + faces×2`)
//!
//! The `p=2` space has 12 edge DOFs + 8 face DOFs per tet over a global
//! numbering with **two** DOFs per global edge and **two** per global face:
//!
//! ```text
//!   edge global index ge ∈ [0, n_edges)  →  global DOFs  2·ge      (W_ge)
//!                                                        2·ge + 1  (Q_ge)
//!   face global index gf ∈ [0, n_faces)  →  global DOFs  2·n_edges + 2·gf      (φ0_gf)
//!                                                        2·n_edges + 2·gf + 1  (φ1_gf)
//!   n_dofs = 2·n_edges + 2·n_faces
//! ```
//!
//! Global edges/faces come from [`TetMesh::edges`]/[`TetMesh::faces`] (canonical
//! ascending-vertex tuples), so the numbering is deterministic and mesh-derived.
//!
//! # Orientation: unit-sign scatter via the ascending-vertex convention
//!
//! **This module does NOT use the `p=1` sign machinery** (`tet_edge_sign` /
//! `sign_outer_tensor`). Per the [`crate::elements::nedelec_p2`] module docs,
//! the element absorbs *both* edge and face orientation when it is built on the
//! tet's four vertices **reordered into ascending global-tag order**
//! ([`ascending_vertex_perm`]). After that reorder:
//!
//! - every local edge `(la, lb)` with `la < lb` has ascending global endpoints,
//!   so `W`/`Q` are the same physical functions in every incident tet, and
//! - every local face `(a, b, c)` with `a < b < c` has ascending global
//!   vertices, so `φ0`/`φ1` are the same physical functions in every incident
//!   tet.
//!
//! Under this convention **all 20 DOFs scatter with unit sign and no `2×2` face
//! mixing** — mixing in the `p=1` sign outer product would *double-apply*
//! orientation (red flag #1 of the issue).
//!
//! # Coefficient / quadrature assumptions
//!
//! The element rule [`tet_quad_deg4`] is exact for the degree-2 curl-curl and
//! degree-4 mass integrands of a **per-tet-constant** coefficient. This module
//! assembles the ε-weighted mass with **per-tet-constant `ε`**
//! (`M(ε) = Σ_t ε_t · M_local(t)`, `ε` pulled out of the integral exactly), so
//! deg-4 remains exact. The quadrature-sampled RHS ([`assemble_p2_rhs_quad`])
//! samples `J(x)` at the same rule's points; for a smooth `J` this converges at
//! the rule's order, and it is exact whenever `N_i · J` stays within degree 4
//! (e.g. `J` at most linear per tet). Spatially varying `ε` or higher-degree
//! `J` would need a higher Duffy rule — out of scope here and stated explicitly.

use std::collections::HashMap;

use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use crate::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS, TET_NEDELEC2_FACE_DOF_BASE, ascending_vertex_perm,
    tet_barycentric_gradients, tet_nedelec2_local, tet_nedelec2_local_rhs, tet_nedelec2_shapes,
    tet_quad_deg4,
};
use crate::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES, TetMesh};

/// Global `p=2` Nédélec DOF numbering for a mesh: the `edges×2 + faces×2`
/// layout plus, per tet, the 20 global DOF indices in the element's **local**
/// layout (the layout of the element built on ascending-vertex-sorted coords)
/// and the vertex permutation that realises the ascending-global-vertex
/// orientation convention.
#[derive(Debug, Clone)]
pub struct P2DofMap {
    /// Number of global edges (`= mesh.edges().len()`).
    pub n_edges: usize,
    /// Number of global faces (`= mesh.faces().len()`).
    pub n_faces: usize,
    /// Total global DOF count `2·n_edges + 2·n_faces`.
    pub n_dofs: usize,
    /// Per-tet global DOF indices, one `[20]` array per tet, in the local DOF
    /// layout of [`tet_nedelec2_local`] evaluated on the **sorted** coords
    /// (see [`P2DofMap::sorted_coords`]).
    pub tet_dofs: Vec<[usize; TET_NEDELEC2_DOFS]>,
    /// Per-tet ascending-global-vertex permutation ([`ascending_vertex_perm`]).
    pub tet_perm: Vec<[usize; 4]>,
}

impl P2DofMap {
    /// Build the global `p=2` DOF numbering from the mesh connectivity.
    ///
    /// Deterministic and orientation-consistent: every incident tet maps its
    /// shared edge/face DOFs to the same global indices with unit sign.
    pub fn build(mesh: &TetMesh) -> Self {
        let edges = mesh.edges();
        let faces = mesh.faces();
        let n_edges = edges.len();
        let n_faces = faces.len();

        let mut edge_lookup: HashMap<(u32, u32), usize> = HashMap::with_capacity(n_edges);
        for (i, e) in edges.iter().enumerate() {
            edge_lookup.insert((e[0], e[1]), i);
        }
        let mut face_lookup: HashMap<(u32, u32, u32), usize> = HashMap::with_capacity(n_faces);
        for (i, f) in faces.iter().enumerate() {
            face_lookup.insert((f[0], f[1], f[2]), i);
        }

        let face_base = 2 * n_edges;
        let mut tet_dofs = Vec::with_capacity(mesh.n_tets());
        let mut tet_perm = Vec::with_capacity(mesh.n_tets());

        for tet in &mesh.tets {
            let tags: [u32; 4] = *tet;
            let perm = ascending_vertex_perm(&tags);
            // Global tags in ascending order (the sorted element's vertices).
            let sorted_tags: [u32; 4] = std::array::from_fn(|i| tags[perm[i]]);

            let mut dofs = [0usize; TET_NEDELEC2_DOFS];

            // Edge DOFs: local edge (la, lb), la < lb, so sorted global
            // endpoints are already ascending (lo < hi).
            for (e, &(la, lb)) in TET_LOCAL_EDGES.iter().enumerate() {
                let lo = sorted_tags[la];
                let hi = sorted_tags[lb];
                debug_assert!(lo < hi, "sorted-vertex edge must be ascending");
                let ge = *edge_lookup
                    .get(&(lo, hi))
                    .expect("edge derived from tet must be in edge table");
                dofs[2 * e] = 2 * ge; // W_ge
                dofs[2 * e + 1] = 2 * ge + 1; // Q_ge
            }

            // Face DOFs: local face (a, b, c), a < b < c, so sorted global
            // triple is ascending.
            for (f, tri) in TET_LOCAL_FACES.iter().enumerate() {
                let (a, b, cc) = (tri[0], tri[1], tri[2]);
                let ta = sorted_tags[a];
                let tb = sorted_tags[b];
                let tc = sorted_tags[cc];
                debug_assert!(ta < tb && tb < tc, "sorted-vertex face must be ascending");
                let gf = *face_lookup
                    .get(&(ta, tb, tc))
                    .expect("face derived from tet must be in face table");
                let base = TET_NEDELEC2_FACE_DOF_BASE + 2 * f;
                dofs[base] = face_base + 2 * gf; // φ0_gf
                dofs[base + 1] = face_base + 2 * gf + 1; // φ1_gf
            }

            tet_dofs.push(dofs);
            tet_perm.push(perm);
        }

        Self {
            n_edges,
            n_faces,
            n_dofs: 2 * n_edges + 2 * n_faces,
            tet_dofs,
            tet_perm,
        }
    }

    /// The tet's four vertex coordinates reordered into ascending global-tag
    /// order — the coords [`tet_nedelec2_local`] and friends must be built on
    /// so the local DOF rows/cols line up with [`P2DofMap::tet_dofs`].
    #[inline]
    pub fn sorted_coords(&self, mesh: &TetMesh, t: usize) -> [[f64; 3]; 4] {
        let tet = &mesh.tets[t];
        let perm = &self.tet_perm[t];
        std::array::from_fn(|i| mesh.nodes[tet[perm[i]] as usize])
    }
}

/// A tet's local 20×20 curl-curl `K` and (unweighted) mass `M`, both built on
/// the ascending-vertex-sorted coords so their rows/cols scatter with unit
/// sign into [`P2DofMap::tet_dofs`].
///
/// `M` here is the **ε = 1** mass; the ε-weighting is applied at scatter time
/// (`M(ε) = Σ_t ε_t · M_local(t)`) and, for the adjoint, per region.
#[allow(clippy::type_complexity)]
pub fn tet_p2_local_sorted(
    dofs: &P2DofMap,
    mesh: &TetMesh,
    t: usize,
) -> (
    [[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
    [[f64; TET_NEDELEC2_DOFS]; TET_NEDELEC2_DOFS],
) {
    let coords = dofs.sorted_coords(mesh, t);
    let (k, m, _vol) = tet_nedelec2_local(&coords);
    (k, m)
}

/// Assemble the global `p=2` curl-curl stiffness `K` and the ε-weighted mass
/// `M(ε)` as real `f64` sparse matrices over the `edges×2 + faces×2` numbering.
///
/// `eps_tet` is the per-tet **real** relative permittivity (length
/// `mesh.n_tets()`); the mass is `M(ε) = Σ_t ε_tet[t] · M_local(t)` — ε pulled
/// out of the (deg-4-exact) integral, exact for per-tet-constant ε. `K` is
/// ε-independent.
///
/// Returns `(K, M)`; both are symmetric. Orientation is unit-sign (no `p=1`
/// sign machinery), per the module docs.
///
/// # Panics
///
/// Panics if `eps_tet.len() != mesh.n_tets()`.
pub fn assemble_p2_km(
    mesh: &TetMesh,
    dofs: &P2DofMap,
    eps_tet: &[f64],
) -> (SparseColMat<usize, f64>, SparseColMat<usize, f64>) {
    assert_eq!(
        eps_tet.len(),
        mesh.n_tets(),
        "eps_tet length must equal n_tets"
    );
    let cap = mesh.n_tets() * TET_NEDELEC2_DOFS * TET_NEDELEC2_DOFS;
    let mut k_tr: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);
    let mut m_tr: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(cap);

    for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
        let (k_loc, m_loc) = tet_p2_local_sorted(dofs, mesh, t);
        let eps = eps_tet[t];
        for i in 0..TET_NEDELEC2_DOFS {
            let gi = gdofs[i];
            for j in 0..TET_NEDELEC2_DOFS {
                let gj = gdofs[j];
                k_tr.push(Triplet::new(gi, gj, k_loc[i][j]));
                m_tr.push(Triplet::new(gi, gj, eps * m_loc[i][j]));
            }
        }
    }

    let k = SparseColMat::<usize, f64>::try_new_from_triplets(dofs.n_dofs, dofs.n_dofs, &k_tr)
        .expect("p=2 K sparse assembly");
    let m = SparseColMat::<usize, f64>::try_new_from_triplets(dofs.n_dofs, dofs.n_dofs, &m_tr)
        .expect("p=2 M sparse assembly");
    (k, m)
}

/// Assemble the global `p=2` RHS moments `b_i = ∫_T N_i · J dV` for a
/// **per-tet-constant** current density `J` (real components), scattered with
/// unit sign. Length `dofs.n_dofs`.
///
/// # Panics
///
/// Panics if `j_tet.len() != mesh.n_tets()`.
pub fn assemble_p2_rhs_constant(mesh: &TetMesh, dofs: &P2DofMap, j_tet: &[[f64; 3]]) -> Vec<f64> {
    assert_eq!(j_tet.len(), mesh.n_tets(), "j_tet length must equal n_tets");
    let mut b = vec![0.0_f64; dofs.n_dofs];
    for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
        let coords = dofs.sorted_coords(mesh, t);
        let b_loc = tet_nedelec2_local_rhs(&coords, j_tet[t]);
        for (i, &bi) in b_loc.iter().enumerate() {
            b[gdofs[i]] += bi;
        }
    }
    b
}

/// Assemble the global `p=2` RHS moments `b_i = ∫_T N_i · J(x) dV` for a
/// **spatially varying** current density `J(x)`, sampled at the degree-≥4 tet
/// quadrature points ([`tet_quad_deg4`]) — the `p=2` analogue of the `p=1`
/// `assemble_nedelec_current_rhs_quad4` pattern (red flag #3 / AC #3).
///
/// `j_at(t, x)` returns the (real) current density at physical point `x` in tet
/// `t`. For a smooth `J` the moment converges at the rule's order; it is exact
/// whenever `N_i · J` is within degree 4 per tet (e.g. `J` at most linear).
/// Length `dofs.n_dofs`, unit-sign scatter.
pub fn assemble_p2_rhs_quad(
    mesh: &TetMesh,
    dofs: &P2DofMap,
    j_at: impl Fn(usize, [f64; 3]) -> [f64; 3],
) -> Vec<f64> {
    let rule = tet_quad_deg4();
    let mut b = vec![0.0_f64; dofs.n_dofs];
    for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
        let coords = dofs.sorted_coords(mesh, t);
        let (bary, signed_vol) = tet_barycentric_gradients(&coords);
        let vol_abs = signed_vol.abs();
        let mut b_loc = [0.0_f64; TET_NEDELEC2_DOFS];
        for (lam, frac) in &rule {
            // Physical quadrature point x = Σ λ_p v_p (sorted coords).
            let x: [f64; 3] =
                std::array::from_fn(|d| (0..4).map(|p| lam[p] * coords[p][d]).sum::<f64>());
            let j = j_at(t, x);
            let (n, _c) = tet_nedelec2_shapes(lam, &bary);
            let w = vol_abs * frac;
            for (i, bl) in b_loc.iter_mut().enumerate() {
                *bl += w * (n[i][0] * j[0] + n[i][1] * j[1] + n[i][2] * j[2]);
            }
        }
        for (i, &bl) in b_loc.iter().enumerate() {
            b[gdofs[i]] += bl;
        }
    }
    b
}

/// Per-entry ingredients of the interior driven pencil `A(ω) = K − ω² M(ε)`
/// over the kept `p=2` DOFs, for the direct sparse path and the adjoint.
///
/// Returns `(remap, n_interior, kept)`:
/// - `remap[g]` maps a full DOF index `g ∈ [0, n_dofs)` to its interior index,
///   or `-1` if `g` is PEC-eliminated (`interior_mask[g] == false`).
/// - `n_interior` is the number of kept DOFs.
/// - `kept` is the list of per-tet **interior** matrix entries
///   `(interior_row, interior_col, K_local, εM_local)` (duplicates at shared
///   DOFs are intended — faer's triplet constructor and a plain accumulation
///   spmv both sum them). Build the complex operator as
///   `A = Σ (K_local − ω² εM_local)` and the residual spmv from the same list.
///
/// # Panics
///
/// Panics if `eps_tet.len() != mesh.n_tets()` or
/// `interior_mask.len() != dofs.n_dofs`.
#[allow(clippy::type_complexity)]
pub fn p2_interior_km(
    mesh: &TetMesh,
    dofs: &P2DofMap,
    eps_tet: &[f64],
    interior_mask: &[bool],
) -> (Vec<i64>, usize, Vec<(usize, usize, f64, f64)>) {
    assert_eq!(eps_tet.len(), mesh.n_tets(), "eps_tet length");
    assert_eq!(interior_mask.len(), dofs.n_dofs, "interior_mask length");

    let mut remap = vec![-1_i64; dofs.n_dofs];
    let mut n_interior = 0usize;
    for (g, &keep) in interior_mask.iter().enumerate() {
        if keep {
            remap[g] = n_interior as i64;
            n_interior += 1;
        }
    }

    let mut kept: Vec<(usize, usize, f64, f64)> =
        Vec::with_capacity(mesh.n_tets() * TET_NEDELEC2_DOFS * TET_NEDELEC2_DOFS);
    for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
        let (k_loc, m_loc) = tet_p2_local_sorted(dofs, mesh, t);
        let eps = eps_tet[t];
        for i in 0..TET_NEDELEC2_DOFS {
            let ri = remap[gdofs[i]];
            if ri < 0 {
                continue;
            }
            for j in 0..TET_NEDELEC2_DOFS {
                let rj = remap[gdofs[j]];
                if rj < 0 {
                    continue;
                }
                kept.push((ri as usize, rj as usize, k_loc[i][j], eps * m_loc[i][j]));
            }
        }
    }
    (remap, n_interior, kept)
}

/// Accumulate the per-region mass action `(M_k x)` over interior DOFs, where
/// `M_k` is the `p=2` mass with `ε` set to the indicator of region `k`
/// (`region_of_tet[t] == k`). Used by the material adjoint
/// (`dg/dε_k = 2 ω² Re[λᵀ M_k x]`). Returns a length-`n_interior` vector.
pub fn p2_region_mass_action(
    mesh: &TetMesh,
    dofs: &P2DofMap,
    remap: &[i64],
    n_interior: usize,
    region_of_tet: &[usize],
    region_k: usize,
    x_int: &[c64],
) -> Vec<c64> {
    let mut out = vec![c64::new(0.0, 0.0); n_interior];
    for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
        if region_of_tet[t] != region_k {
            continue;
        }
        let (_k_loc, m_loc) = tet_p2_local_sorted(dofs, mesh, t);
        for i in 0..TET_NEDELEC2_DOFS {
            let ri = remap[gdofs[i]];
            if ri < 0 {
                continue;
            }
            let ri = ri as usize;
            for j in 0..TET_NEDELEC2_DOFS {
                let rj = remap[gdofs[j]];
                if rj < 0 {
                    continue;
                }
                out[ri] += c64::new(m_loc[i][j], 0.0) * x_int[rj as usize];
            }
        }
    }
    out
}

/// Interior (kept) `p=2` DOF mask for a **cube PEC cavity** `[0, side]³`:
/// a DOF is eliminated (`false`) iff its edge/face lies entirely on a boundary
/// plane (`n × E = 0`).
///
/// The geometric predicate is plane-based and exact for a box: an edge is on
/// the boundary iff both endpoints share a coordinate equal to `0` or `side`;
/// a face is on the boundary iff all three vertices do. Length `dofs.n_dofs`.
pub fn cube_pec_interior_p2_dofs(mesh: &TetMesh, dofs: &P2DofMap, side: f64) -> Vec<bool> {
    let tol = 1e-9 * side.max(1.0);
    let edges = mesh.edges();
    let faces = mesh.faces();
    let on_plane_pair = |a: usize, b: usize| -> bool {
        let pa = &mesh.nodes[a];
        let pb = &mesh.nodes[b];
        (0..3).any(|d| {
            let lo = pa[d].abs() < tol && pb[d].abs() < tol;
            let hi = (pa[d] - side).abs() < tol && (pb[d] - side).abs() < tol;
            lo || hi
        })
    };
    let on_plane_triple = |a: usize, b: usize, c: usize| -> bool {
        let pa = &mesh.nodes[a];
        let pb = &mesh.nodes[b];
        let pc = &mesh.nodes[c];
        (0..3).any(|d| {
            let lo = pa[d].abs() < tol && pb[d].abs() < tol && pc[d].abs() < tol;
            let hi = (pa[d] - side).abs() < tol
                && (pb[d] - side).abs() < tol
                && (pc[d] - side).abs() < tol;
            lo || hi
        })
    };

    let mut keep = vec![true; dofs.n_dofs];
    for (ge, e) in edges.iter().enumerate() {
        if on_plane_pair(e[0] as usize, e[1] as usize) {
            keep[2 * ge] = false;
            keep[2 * ge + 1] = false;
        }
    }
    let face_base = 2 * dofs.n_edges;
    for (gf, f) in faces.iter().enumerate() {
        if on_plane_triple(f[0] as usize, f[1] as usize, f[2] as usize) {
            keep[face_base + 2 * gf] = false;
            keep[face_base + 2 * gf + 1] = false;
        }
    }
    keep
}

/// Reconstruct the `p=2` vector field `E_h(x)` from a full-length DOF vector at
/// a point inside tet `t` given by barycentrics `lam` **in the mesh tet's
/// natural (unsorted) vertex order** (`lam[i]` weights `mesh.tets[t][i]`).
///
/// The permutation to the ascending-vertex-sorted assembly basis is applied
/// internally, so callers pass the same `lam` they use to form the physical
/// point `x = Σ lam[i] · mesh.nodes[tet[i]]` — no manual sorting, and no
/// sorted/unsorted mismatch. Intended for convergence post-processing (L2
/// error against an analytic field).
pub fn p2_field_at(
    mesh: &TetMesh,
    dofs: &P2DofMap,
    x_dof: &[c64],
    t: usize,
    lam: &[f64; 4],
) -> [c64; 3] {
    let coords = dofs.sorted_coords(mesh, t);
    // Permute the caller's natural-order barycentrics into the sorted-vertex
    // order the basis is built on: lam_sorted[i] = lam[perm[i]].
    let perm = &dofs.tet_perm[t];
    let lam_sorted: [f64; 4] = std::array::from_fn(|i| lam[perm[i]]);
    let (bary, _vol) = tet_barycentric_gradients(&coords);
    let (n, _c) = tet_nedelec2_shapes(&lam_sorted, &bary);
    let gdofs = &dofs.tet_dofs[t];
    let mut e = [c64::new(0.0, 0.0); 3];
    for i in 0..TET_NEDELEC2_DOFS {
        let coeff = x_dof[gdofs[i]];
        for (d, ed) in e.iter_mut().enumerate() {
            *ed += coeff * n[i][d];
        }
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    #[test]
    fn dof_numbering_is_consistent_and_bounded() {
        let mesh = cube_tet_mesh(2, 1.0);
        let dofs = P2DofMap::build(&mesh);
        assert_eq!(dofs.n_edges, mesh.edges().len());
        assert_eq!(dofs.n_faces, mesh.faces().len());
        assert_eq!(dofs.n_dofs, 2 * dofs.n_edges + 2 * dofs.n_faces);
        // Every global DOF index is in range, and each tet uses 20 distinct DOFs.
        for gdofs in &dofs.tet_dofs {
            let mut seen = gdofs.to_vec();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(seen.len(), TET_NEDELEC2_DOFS, "20 distinct DOFs per tet");
            assert!(gdofs.iter().all(|&g| g < dofs.n_dofs));
        }
    }

    #[test]
    fn shared_face_dofs_agree_across_incident_tets() {
        // Every interior face is shared by exactly two tets; both must map the
        // face's two φ DOFs to the SAME global indices (conformity).
        let mesh = cube_tet_mesh(2, 1.0);
        let dofs = P2DofMap::build(&mesh);
        let faces = mesh.faces();
        let face_base = 2 * dofs.n_edges;
        // Collect, per global face, the set of (φ0, φ1) global DOFs seen.
        let mut per_face: Vec<Option<(usize, usize)>> = vec![None; faces.len()];
        for gdofs in &dofs.tet_dofs {
            for f in 0..4 {
                let base = TET_NEDELEC2_FACE_DOF_BASE + 2 * f;
                let d0 = gdofs[base];
                let d1 = gdofs[base + 1];
                // recover gf from the global DOF index
                let gf = (d0 - face_base) / 2;
                let expect = (face_base + 2 * gf, face_base + 2 * gf + 1);
                assert_eq!((d0, d1), expect);
                match per_face[gf] {
                    None => per_face[gf] = Some((d0, d1)),
                    Some(prev) => assert_eq!(prev, (d0, d1)),
                }
            }
        }
    }

    #[test]
    fn stiffness_annihilates_a_global_gradient_field() {
        // The curl-curl form has the gradient space in its kernel. A linear
        // scalar field φ = a·x has constant ∇φ; its lowest-order Whitney (W)
        // edge representation uses coefficient φ(b) − φ(a) on edge (a<b). With a
        // globally consistent unit-sign assembly the curl-curl energy xᵀ K x of
        // that gradient field must vanish to round-off.
        let mesh = cube_tet_mesh(2, 1.0);
        let dofs = P2DofMap::build(&mesh);
        let edges = mesh.edges();
        let phi = |p: &[f64; 3]| p[0] + 2.0 * p[1] + 3.0 * p[2];
        let mut x = vec![0.0_f64; dofs.n_dofs];
        for (ge, e) in edges.iter().enumerate() {
            let pa = &mesh.nodes[e[0] as usize];
            let pb = &mesh.nodes[e[1] as usize];
            x[2 * ge] = phi(pb) - phi(pa); // W (Whitney) coefficient
        }
        let mut energy = 0.0_f64;
        let mut kfrob = 0.0_f64;
        for (t, gdofs) in dofs.tet_dofs.iter().enumerate() {
            let (k_loc, _m) = tet_p2_local_sorted(&dofs, &mesh, t);
            for i in 0..TET_NEDELEC2_DOFS {
                for j in 0..TET_NEDELEC2_DOFS {
                    energy += x[gdofs[i]] * k_loc[i][j] * x[gdofs[j]];
                    kfrob += k_loc[i][j].abs();
                }
            }
        }
        assert!(kfrob > 1.0, "K must be nontrivial, got Frobenius-1 {kfrob}");
        assert!(
            energy.abs() < 1e-9,
            "curl-curl energy of a global gradient field must vanish, got {energy}"
        );
    }
}
