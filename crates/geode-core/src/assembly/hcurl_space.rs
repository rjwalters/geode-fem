//! The order-pluggable H(curl) space (issue #838, Epic #836 Phase 1a).
//!
//! [`HcurlSpace`] is the **one object** that owns the global DOF layout of a
//! first-kind Nédélec space on a [`TetMesh`]: the DOF count, the per-tet
//! local→global gather and its orientation, the entity→DOF maps for edges
//! and faces, the face-exact PEC mask, field and curl evaluation, the
//! hierarchical `p=1 → p=2` injection, and the face-DOF relabelling
//! transform. Element order is a property of this object, selected by
//! [`ElementOrder`]; consumers (the order-generic
//! [`crate::driven::solve::DrivenOperator::assemble_with_space`], and later
//! the eigen, port, adjoint and CLI layers) consume it instead of branching
//! on order locally.
//!
//! # Orders
//!
//! - **p=1** (Whitney): one DOF per edge, numbered exactly as
//!   [`TetMesh::edges`] (so `n_dofs = n_edges`), with the per-tet layout and
//!   edge signs of [`TetMesh::tet_edges`]. Every p=1 accessor reproduces the
//!   pre-#838 tables bit for bit; [`HcurlSpace::pec_interior_mask`] *calls*
//!   [`crate::mesh::pec_interior_mask_from_triangles`].
//! - **p=2** ([`crate::elements::nedelec_p2`]): two DOFs per edge `(W, Q)`
//!   and two per face `(φ0, φ1)`, numbered exactly as
//!   [`crate::assembly::nedelec_p2::P2DofMap`]
//!   (`edge ge → 2ge, 2ge+1`; `face gf → 2E + 2gf, 2E + 2gf + 1`), with the
//!   ascending-global-vertex orientation convention: the element is built on
//!   the tet's vertices sorted by global tag and every DOF scatters with
//!   unit sign.
//!
//! # Per-entity DOF counts (the hp hook)
//!
//! The space stores a DOF **count and offset per edge and per face**, and a
//! per-tet offset into a flat gather list, rather than one global order.
//! Only uniform order is built here ([`HcurlSpace::build`]), but variable
//! per-element order (hp, owned by the AMR epic #835) is an extension of
//! these tables, not a rewrite: every accessor already reads the per-entity
//! counts ([`HcurlSpace::edge_dof_count`], [`HcurlSpace::face_dof_count`],
//! [`HcurlSpace::tet_dofs`] returns a slice of per-tet length).
//!
//! # Hooks for the sibling v0.9 epics
//!
//! - **AMR (#835): rebuild per mesh.** The space holds no reference to the
//!   mesh and caches only connectivity-derived tables. After every
//!   refinement, call [`HcurlSpace::build`] on the new mesh; no DOF table,
//!   mask or operator built on the old mesh may be reused. The orientation
//!   convention depends only on global vertex tags, so conformity survives
//!   refinement and renumbering. [`HcurlSpace::prolong_p1`] is the exact
//!   hierarchical injection `V_1 ⊂ V_2` on one mesh (the p-surplus
//!   indicator's substrate).
//! - **Periodic / Floquet (#837): pairing DOFs across faces.**
//!   [`HcurlSpace::edge_dofs`] / [`HcurlSpace::face_dofs`] give the global
//!   DOFs of a master/slave entity. Under the ascending convention an edge's
//!   `Q` DOF is orientation-free and its `W` DOF flips sign when the paired
//!   edges' ascending orders differ; a face's `(φ0, φ1)` pair **mixes by a
//!   non-diagonal 2×2** when the paired faces' ascending vertex orders
//!   differ. That 2×2 is [`HcurlSpace::face_dof_transform`]. A p=1 periodic
//!   map applied to p=2 DOFs is silently wrong; use the transform.
//! - **Differentiable EDA (#841) / Phase 4 adjoints.** The order-generic
//!   solve is [`crate::driven::solve::DrivenOperator::assemble_with_space`]
//!   followed by `factor_at(ω)`; `A(ω)ᵀ = A(ω)`, so the adjoint solve is a
//!   back-solve on the same factorization. Per-tet sensitivities scatter
//!   through [`HcurlSpace::tet_dofs`] and evaluate fields with
//!   [`HcurlSpace::field_at`] / [`HcurlSpace::curl_at`].

use std::collections::{BTreeSet, HashMap};

use faer::c64;

use crate::elements::ElementOrder;
use crate::elements::nedelec_p2::{
    TET_NEDELEC2_DOFS, ascending_vertex_perm, tet_barycentric_gradients, tet_nedelec2_shapes,
};
use crate::mesh::{TET_LOCAL_EDGES, TET_LOCAL_FACES, TetMesh};

/// How a tet's local DOFs are oriented against the global ones
/// ([`HcurlSpace::tet_orientation`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TetOrientation {
    /// p=1: the six local Whitney functions are built on the tet's
    /// **natural** vertex order and scatter with these signs (`+1` when the
    /// local edge `(la, lb)` of [`TET_LOCAL_EDGES`] runs low→high in global
    /// tags), exactly as [`TetMesh::tet_edges`].
    EdgeSigns([i8; 6]),
    /// p=2: the element is built on the tet's vertices reordered by this
    /// permutation (ascending global tag, [`ascending_vertex_perm`]) and
    /// every DOF scatters with unit sign. `perm[i]` is the natural local
    /// vertex placed at sorted position `i`.
    AscendingPerm([usize; 4]),
}

/// An order-pluggable first-kind Nédélec H(curl) space on a tet mesh (see
/// the [module docs](self)).
#[derive(Debug, Clone)]
pub struct HcurlSpace {
    order: ElementOrder,
    n_nodes: usize,
    n_tets: usize,
    /// Global edges, as [`TetMesh::edges`] (`a < b`, sorted).
    edges: Vec<[u32; 2]>,
    /// Global faces, as [`TetMesh::faces`] (`a < b < c`, sorted).
    faces: Vec<[u32; 3]>,
    /// Edge `e` owns global DOFs `edge_ptr[e] .. edge_ptr[e + 1]`.
    edge_ptr: Vec<u32>,
    /// Face `f` owns global DOFs `face_ptr[f] .. face_ptr[f + 1]`.
    face_ptr: Vec<u32>,
    /// `0 .. n_dofs`: entity DOFs are contiguous, so the entity accessors
    /// return slices of this identity list.
    dof_ids: Vec<u32>,
    /// Tet `t`'s local→global gather is `tet_list[tet_ptr[t] .. tet_ptr[t+1]]`.
    tet_ptr: Vec<usize>,
    tet_list: Vec<u32>,
    /// Per-tet orientation data: edge signs at p=1, the ascending vertex
    /// permutation at p=2.
    tet_orient: Vec<TetOrientation>,
}

impl HcurlSpace {
    /// Build the uniform-order space of `order` on `mesh`.
    ///
    /// Deterministic and connectivity-derived. **Rebuild it for every
    /// mesh** (AMR): nothing built on one mesh is valid on another.
    pub fn build(mesh: &TetMesh, order: ElementOrder) -> Self {
        let edges = mesh.edges();
        let faces = mesh.faces();
        let n_edges = edges.len();
        let n_faces = faces.len();

        // Per-entity DOF counts → contiguous offsets (edges first, then
        // faces). Uniform here; the hp extension only changes the counts.
        let mut edge_ptr = Vec::with_capacity(n_edges + 1);
        let mut next = 0u32;
        edge_ptr.push(next);
        for _ in 0..n_edges {
            next += order.dofs_per_edge() as u32;
            edge_ptr.push(next);
        }
        let mut face_ptr = Vec::with_capacity(n_faces + 1);
        face_ptr.push(next);
        for _ in 0..n_faces {
            next += order.dofs_per_face() as u32;
            face_ptr.push(next);
        }
        let n_dofs = next as usize;

        let mut tet_ptr = Vec::with_capacity(mesh.n_tets() + 1);
        tet_ptr.push(0usize);
        let mut tet_list: Vec<u32> = Vec::with_capacity(mesh.n_tets() * order.dofs_per_tet());
        let mut tet_orient = Vec::with_capacity(mesh.n_tets());

        match order {
            ElementOrder::P1 => {
                // Exactly the pre-#838 tables: TetMesh::tet_edges.
                for row in mesh.tet_edges() {
                    let mut signs = [0i8; 6];
                    for (slot, &(ge, sign)) in row.iter().enumerate() {
                        tet_list.push(ge);
                        signs[slot] = sign;
                    }
                    tet_orient.push(TetOrientation::EdgeSigns(signs));
                    tet_ptr.push(tet_list.len());
                }
            }
            ElementOrder::P2 => {
                let edge_lookup: HashMap<(u32, u32), usize> = edges
                    .iter()
                    .enumerate()
                    .map(|(i, e)| ((e[0], e[1]), i))
                    .collect();
                let face_lookup: HashMap<(u32, u32, u32), usize> = faces
                    .iter()
                    .enumerate()
                    .map(|(i, f)| ((f[0], f[1], f[2]), i))
                    .collect();
                for tet in &mesh.tets {
                    let perm = ascending_vertex_perm(tet);
                    let s: [u32; 4] = std::array::from_fn(|i| tet[perm[i]]);
                    // Local layout of `tet_nedelec2_local` on sorted coords:
                    // edges (W, Q) in TET_LOCAL_EDGES order, then faces
                    // (φ0, φ1) in TET_LOCAL_FACES order.
                    for &(la, lb) in TET_LOCAL_EDGES.iter() {
                        let ge = edge_lookup[&(s[la], s[lb])];
                        for d in edge_ptr[ge]..edge_ptr[ge + 1] {
                            tet_list.push(d);
                        }
                    }
                    for tri in TET_LOCAL_FACES.iter() {
                        let gf = face_lookup[&(s[tri[0]], s[tri[1]], s[tri[2]])];
                        for d in face_ptr[gf]..face_ptr[gf + 1] {
                            tet_list.push(d);
                        }
                    }
                    tet_orient.push(TetOrientation::AscendingPerm(perm));
                    tet_ptr.push(tet_list.len());
                }
            }
        }

        Self {
            order,
            n_nodes: mesh.n_nodes(),
            n_tets: mesh.n_tets(),
            edges,
            faces,
            edge_ptr,
            face_ptr,
            dof_ids: (0..n_dofs as u32).collect(),
            tet_ptr,
            tet_list,
            tet_orient,
        }
    }

    /// The (uniform) element order this space was built with.
    pub fn order(&self) -> ElementOrder {
        self.order
    }

    /// Total global DOF count: `n_edges` at p=1, `2·n_edges + 2·n_faces` at
    /// p=2.
    pub fn n_dofs(&self) -> usize {
        self.dof_ids.len()
    }

    /// Number of tets of the mesh this space was built on.
    pub fn n_tets(&self) -> usize {
        self.n_tets
    }

    /// Number of nodes of the mesh this space was built on.
    pub fn n_nodes(&self) -> usize {
        self.n_nodes
    }

    /// Number of global edges.
    pub fn n_edges(&self) -> usize {
        self.edges.len()
    }

    /// Number of global faces.
    pub fn n_faces(&self) -> usize {
        self.faces.len()
    }

    /// The global edge table, identical to [`TetMesh::edges`].
    pub fn edges(&self) -> &[[u32; 2]] {
        &self.edges
    }

    /// The global face table, identical to [`TetMesh::faces`].
    pub fn faces(&self) -> &[[u32; 3]] {
        &self.faces
    }

    /// DOFs carried by global edge `edge_gid` (the per-entity count).
    pub fn edge_dof_count(&self, edge_gid: usize) -> usize {
        (self.edge_ptr[edge_gid + 1] - self.edge_ptr[edge_gid]) as usize
    }

    /// DOFs carried by global face `face_gid` (the per-entity count).
    pub fn face_dof_count(&self, face_gid: usize) -> usize {
        (self.face_ptr[face_gid + 1] - self.face_ptr[face_gid]) as usize
    }

    /// Global DOFs of tet `t` in the element's local layout: 6 edge DOFs at
    /// p=1 (natural vertex order, [`TET_LOCAL_EDGES`]); 20 at p=2 (the
    /// `tet_nedelec2_local` layout on ascending-sorted vertices).
    pub fn tet_dofs(&self, t: usize) -> &[u32] {
        &self.tet_list[self.tet_ptr[t]..self.tet_ptr[t + 1]]
    }

    /// Orientation of tet `t`'s local DOFs: edge signs at p=1, the
    /// ascending vertex permutation at p=2.
    pub fn tet_orientation(&self, t: usize) -> TetOrientation {
        self.tet_orient[t]
    }

    /// Global DOFs of edge `edge_gid`: `[e]` at p=1, `[W, Q]` at p=2. The
    /// `W` (Whitney) DOF is oriented low→high in global tags; `Q` is
    /// orientation-free.
    pub fn edge_dofs(&self, edge_gid: usize) -> &[u32] {
        let (a, b) = (self.edge_ptr[edge_gid], self.edge_ptr[edge_gid + 1]);
        &self.dof_ids[a as usize..b as usize]
    }

    /// Global DOFs of face `face_gid`: `[]` at p=1, `[φ0, φ1]` at p=2 (in
    /// the face's ascending-vertex labelling; see
    /// [`HcurlSpace::face_dof_transform`] for any other labelling).
    pub fn face_dofs(&self, face_gid: usize) -> &[u32] {
        let (a, b) = (self.face_ptr[face_gid], self.face_ptr[face_gid + 1]);
        &self.dof_ids[a as usize..b as usize]
    }

    /// The vertex coordinates of tet `t` in the order its local basis is
    /// built on: natural at p=1, ascending global tag at p=2.
    pub fn tet_local_coords(&self, mesh: &TetMesh, t: usize) -> [[f64; 3]; 4] {
        let tet = &mesh.tets[t];
        match self.tet_orient[t] {
            TetOrientation::EdgeSigns(_) => std::array::from_fn(|i| mesh.nodes[tet[i] as usize]),
            TetOrientation::AscendingPerm(perm) => {
                std::array::from_fn(|i| mesh.nodes[tet[perm[i]] as usize])
            }
        }
    }

    /// Face-exact PEC interior mask over [`HcurlSpace::n_dofs`] (`true` =
    /// kept, `false` = eliminated), for the wall triangles in `walls`
    /// (issue #780).
    ///
    /// - **p=1:** returns [`crate::mesh::pec_interior_mask_from_triangles`]
    ///   on the space's edge table, i.e. the pre-#838 mask bit for bit.
    /// - **p=2:** eliminates `(W, Q)` of every edge of a wall triangle and
    ///   `(φ0, φ1)` of every wall face. An edge or face whose vertices all
    ///   lie on the boundary but which is **not** an edge/face of a listed
    ///   wall triangle stays free (a chord; in the `n = 1` cube that includes
    ///   the body diagonal and the interior faces). This is the #780 lesson
    ///   carried to p=2.
    ///
    /// # Panics
    ///
    /// Panics if `mesh` is not the mesh the space was built on (node or tet
    /// count differ), or — at p ≥ 2, where the face DOFs need it — if a wall
    /// triangle is not a face of the mesh (it would leave its face DOFs
    /// silently free).
    pub fn pec_interior_mask(&self, mesh: &TetMesh, walls: &[&[[u32; 3]]]) -> Vec<bool> {
        self.assert_mesh(mesh);
        if self.order == ElementOrder::P1 {
            return crate::mesh::pec_interior_mask_from_triangles(&self.edges, walls);
        }
        let mut wall_edges: BTreeSet<(u32, u32)> = BTreeSet::new();
        let mut wall_faces: BTreeSet<(u32, u32, u32)> = BTreeSet::new();
        for list in walls {
            for tri in *list {
                for &(a, b) in &[(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                    wall_edges.insert(if a < b { (a, b) } else { (b, a) });
                }
                let mut s = *tri;
                s.sort_unstable();
                wall_faces.insert((s[0], s[1], s[2]));
            }
        }
        let mut keep = vec![true; self.n_dofs()];
        for (ge, e) in self.edges.iter().enumerate() {
            if wall_edges.contains(&(e[0], e[1])) {
                for &d in self.edge_dofs(ge) {
                    keep[d as usize] = false;
                }
            }
        }
        let mut found = 0usize;
        for (gf, f) in self.faces.iter().enumerate() {
            if wall_faces.contains(&(f[0], f[1], f[2])) {
                found += 1;
                for &d in self.face_dofs(gf) {
                    keep[d as usize] = false;
                }
            }
        }
        assert_eq!(
            found,
            wall_faces.len(),
            "HcurlSpace::pec_interior_mask: {} wall triangle(s) are not faces of the mesh; \
             their face DOFs would stay free at {}",
            wall_faces.len() - found,
            self.order
        );
        keep
    }

    /// The H(curl) field `E_h(x)` of the full-length DOF vector `x` at the
    /// point of tet `t` with barycentrics `bary` given in the tet's
    /// **natural** vertex order (`bary[i]` weights `mesh.tets[t][i]`).
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != self.n_dofs()`.
    pub fn field_at(&self, mesh: &TetMesh, t: usize, bary: [f64; 4], x: &[c64]) -> [c64; 3] {
        self.eval(mesh, t, bary, x, false)
    }

    /// The curl `∇×E_h(x)` of the full-length DOF vector `x` at the point of
    /// tet `t` with natural-order barycentrics `bary` (see
    /// [`HcurlSpace::field_at`]).
    ///
    /// # Panics
    ///
    /// Panics if `x.len() != self.n_dofs()`.
    pub fn curl_at(&self, mesh: &TetMesh, t: usize, bary: [f64; 4], x: &[c64]) -> [c64; 3] {
        self.eval(mesh, t, bary, x, true)
    }

    fn eval(&self, mesh: &TetMesh, t: usize, bary: [f64; 4], x: &[c64], curl: bool) -> [c64; 3] {
        assert_eq!(x.len(), self.n_dofs(), "DOF vector length != n_dofs");
        let coords = self.tet_local_coords(mesh, t);
        let (grad, _vol) = tet_barycentric_gradients(&coords);
        let dofs = self.tet_dofs(t);
        let mut out = [c64::new(0.0, 0.0); 3];
        match self.tet_orient[t] {
            TetOrientation::EdgeSigns(signs) => {
                for (slot, &(a, b)) in TET_LOCAL_EDGES.iter().enumerate() {
                    let coeff = x[dofs[slot] as usize] * f64::from(signs[slot]);
                    let v: [f64; 3] = if curl {
                        let cr = cross(grad[a], grad[b]);
                        [2.0 * cr[0], 2.0 * cr[1], 2.0 * cr[2]]
                    } else {
                        std::array::from_fn(|d| bary[a] * grad[b][d] - bary[b] * grad[a][d])
                    };
                    for d in 0..3 {
                        out[d] += coeff * v[d];
                    }
                }
            }
            TetOrientation::AscendingPerm(perm) => {
                let lam: [f64; 4] = std::array::from_fn(|i| bary[perm[i]]);
                let (n, c) = tet_nedelec2_shapes(&lam, &grad);
                let shapes = if curl { &c } else { &n };
                for i in 0..TET_NEDELEC2_DOFS {
                    let coeff = x[dofs[i] as usize];
                    for d in 0..3 {
                        out[d] += coeff * shapes[i][d];
                    }
                }
            }
        }
        out
    }

    /// Hierarchical injection of a p=1 (Whitney edge) DOF vector into this
    /// space: the field is reproduced exactly (`V_1 ⊂ V_p`).
    ///
    /// At p=1 this is a copy. At p=2 the p=2 edge function `W_ab` *is* the
    /// Whitney function of the low→high global edge, so each edge's `W` DOF
    /// takes the p=1 coefficient and every `Q` and face DOF is zero.
    ///
    /// # Panics
    ///
    /// Panics if `x_p1.len() != self.n_edges()`.
    pub fn prolong_p1(&self, x_p1: &[c64]) -> Vec<c64> {
        assert_eq!(x_p1.len(), self.n_edges(), "p=1 vector length != n_edges");
        if self.order == ElementOrder::P1 {
            return x_p1.to_vec();
        }
        let mut out = vec![c64::new(0.0, 0.0); self.n_dofs()];
        for (ge, &v) in x_p1.iter().enumerate() {
            // The first DOF of each edge is its Whitney (W) DOF.
            out[self.edge_ptr[ge] as usize] = v;
        }
        out
    }

    /// The 2×2 map of a face's p=2 DOF pair under a **relabelling** of its
    /// vertices — the hook periodic/Floquet pairing needs (#837).
    ///
    /// Let `(a, b, c)` be the face's vertices in ascending global order and
    /// `φ0 = λ_c W_ab`, `φ1 = λ_a W_bc` its face functions
    /// ([`crate::elements::nedelec_p2`]), with `W_xy = λ_x∇λ_y − λ_y∇λ_x`.
    /// `local_perm` names a relabelling: its vertex `k` is ascending vertex
    /// `local_perm[k]` (so `[0, 1, 2]` is the identity). The face functions
    /// built in that labelling, `φ0' = λ_{c'} W_{a'b'}` and
    /// `φ1' = λ_{a'} W_{b'c'}` with `(a', b', c')` the relabelled triple, are
    /// exact combinations of the ascending pair (because
    /// `λ_c W_ab + λ_a W_bc + λ_b W_ca ≡ 0`, no edge function enters):
    ///
    /// ```text
    ///   φ'_i = Σ_j T[i][j] φ_j,        T = face_dof_transform(local_perm)
    /// ```
    ///
    /// so a field `Σ c'_i φ'_i = Σ c_j φ_j` has ascending DOFs `c = Tᵀ c'`.
    /// For the odd relabellings `T` is not a signed permutation: a raw
    /// relabelling **mixes** φ0 and φ1 (the element docs' warning). Each
    /// entry is in `{−1, 0, 1}`, and `T` is invertible.
    ///
    /// # Panics
    ///
    /// Panics if `local_perm` is not a permutation of `{0, 1, 2}`.
    pub fn face_dof_transform(local_perm: [usize; 3]) -> [[f64; 2]; 2] {
        let mut seen = [false; 3];
        for &p in &local_perm {
            assert!(
                p < 3 && !seen[p],
                "local_perm {local_perm:?} is not a permutation"
            );
            seen[p] = true;
        }
        let [a, b, c] = local_perm;
        [face_fn_coeffs(a, b, c), face_fn_coeffs(b, c, a)]
    }

    fn assert_mesh(&self, mesh: &TetMesh) {
        assert!(
            mesh.n_nodes() == self.n_nodes && mesh.n_tets() == self.n_tets,
            "HcurlSpace was built on a different mesh ({} nodes / {} tets, got {} / {}); \
             rebuild the space for every mesh",
            self.n_nodes,
            self.n_tets,
            mesh.n_nodes(),
            mesh.n_tets()
        );
    }
}

/// Coefficients `[c0, c1]` of `ψ(x, y, z) = λ_z W_xy` (ascending local
/// face vertices `0 < 1 < 2`, distinct `x, y, z`) in the ascending pair
/// `φ0 = ψ(0,1,2)`, `φ1 = ψ(1,2,0)`, using `ψ(y,x,z) = −ψ(x,y,z)` and
/// `ψ(2,0,1) = −φ0 − φ1`.
fn face_fn_coeffs(x: usize, y: usize, z: usize) -> [f64; 2] {
    // Normalise to the canonical (x, y) order for the multiplier vertex z.
    let (canon, sign) = match z {
        2 => ((0, 1), if (x, y) == (0, 1) { 1.0 } else { -1.0 }),
        0 => ((1, 2), if (x, y) == (1, 2) { 1.0 } else { -1.0 }),
        _ => ((2, 0), if (x, y) == (2, 0) { 1.0 } else { -1.0 }),
    };
    let base = match canon {
        (0, 1) => [1.0, 0.0],
        (1, 2) => [0.0, 1.0],
        _ => [-1.0, -1.0],
    };
    [sign * base[0], sign * base[1]]
}

#[inline]
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::nedelec_p2::P2DofMap;
    use crate::mesh::cube_tet_mesh;

    #[test]
    fn p1_tables_match_the_mesh_tables() {
        let mesh = cube_tet_mesh(2, 1.0);
        let s = HcurlSpace::build(&mesh, ElementOrder::P1);
        assert_eq!(s.n_dofs(), mesh.edges().len());
        assert_eq!(s.edges(), mesh.edges().as_slice());
        for (t, row) in mesh.tet_edges().iter().enumerate() {
            let ids: Vec<u32> = row.iter().map(|r| r.0).collect();
            assert_eq!(s.tet_dofs(t), ids.as_slice());
            let signs: [i8; 6] = std::array::from_fn(|i| row[i].1);
            assert_eq!(s.tet_orientation(t), TetOrientation::EdgeSigns(signs));
        }
        assert!(s.face_dofs(0).is_empty());
        assert_eq!(s.edge_dofs(3), &[3]);
    }

    #[test]
    fn p2_numbering_matches_p2_dof_map() {
        let mesh = cube_tet_mesh(2, 1.0);
        let s = HcurlSpace::build(&mesh, ElementOrder::P2);
        let m = P2DofMap::build(&mesh);
        assert_eq!(s.n_dofs(), m.n_dofs);
        for t in 0..mesh.n_tets() {
            let want: Vec<u32> = m.tet_dofs[t].iter().map(|&d| d as u32).collect();
            assert_eq!(s.tet_dofs(t), want.as_slice());
            assert_eq!(
                s.tet_orientation(t),
                TetOrientation::AscendingPerm(m.tet_perm[t])
            );
        }
        assert_eq!(s.edge_dofs(5), &[10, 11]);
        let fb = 2 * s.n_edges() as u32;
        assert_eq!(s.face_dofs(2), &[fb + 4, fb + 5]);
    }

    #[test]
    fn identity_transform_is_identity() {
        assert_eq!(
            HcurlSpace::face_dof_transform([0, 1, 2]),
            [[1.0, 0.0], [0.0, 1.0]]
        );
    }
}
