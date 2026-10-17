//! **Prism extrusion of a 2-D triangulation** into a conforming tetrahedral
//! mesh (Epic #778 Phase 3b, issue #817).
//!
//! [`extrude_tri_mesh`] sweeps a [`TriMesh`] cross-section along `z` through
//! `nz` slabs and splits every prism into three tetrahedra. The split is
//! the **sorted-vertex rule**: with the bottom triangle's nodes sorted by
//! their 2-D index `v₀ < v₁ < v₂` (primes on the top layer), the tets are
//!
//! ```text
//!   {v₀, v₁, v₂, v₂'},   {v₀, v₁, v₁', v₂'},   {v₀, v₀', v₁', v₂'}.
//! ```
//!
//! Every lateral quad `(a, b, b', a')` with `a < b` is then cut by the
//! diagonal `a → b'` (lower bottom node to higher top node), a choice that
//! depends only on the quad's own two nodes. Two prisms sharing a quad
//! therefore cut it the same way and the mesh is **conforming** for any
//! input triangulation. The split creates no interior edge: every tet edge
//! is a horizontal edge (a 2-D edge on a layer), a vertical edge (a 2-D node
//! over a slab) or a lateral diagonal (a 2-D edge over a slab), which
//! [`ExtrudedEdge`] names.
//!
//! Node numbering is layer-major, `3-D node = layer · n₂ + 2-D node`, so the
//! map from either end face to the 2-D mesh is monotone: the projected port
//! face of [`crate::driven::ports::project_port_face`] reproduces the 2-D
//! node numbering, edge orientation and therefore the 2-D edge DOFs without
//! a sign flip. Each tet records its source triangle and slab, so per-triangle
//! data (permittivity, conductor flags) carries over to the volume.
//!
//! Reused by the 3-D strip-line fixtures of
//! [`crate::driven::ports::strip_line_section`] and meant for the lossy (P4)
//! and CLI (P5) phases.

use std::collections::{BTreeMap, HashMap};

use crate::analytic::waveguide::TriMesh;
use crate::mesh::TetMesh;

/// What a 3-D edge of an [`ExtrudedTriMesh`] is, in terms of the 2-D mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtrudedEdge {
    /// 2-D edge `edge` (index into [`TriMesh::edges`]) on layer `layer`
    /// (`z = z[layer]`).
    Horizontal {
        /// 2-D edge index.
        edge: usize,
        /// Layer index (`0..=nz`).
        layer: usize,
    },
    /// The segment over 2-D node `node` through slab `slab`
    /// (`z[slab] .. z[slab + 1]`).
    Vertical {
        /// 2-D node index.
        node: usize,
        /// Slab index (`0..nz`).
        slab: usize,
    },
    /// The diagonal of the lateral quad over 2-D edge `edge` in slab `slab`
    /// (from the lower-indexed bottom node to the higher-indexed top node).
    Diagonal {
        /// 2-D edge index.
        edge: usize,
        /// Slab index (`0..nz`).
        slab: usize,
    },
}

/// Output of [`extrude_tri_mesh`] / [`extrude_tri_mesh_layers`].
#[derive(Debug, Clone)]
pub struct ExtrudedTriMesh {
    /// The tetrahedral mesh (right-handed tets).
    pub mesh: TetMesh,
    /// Number of 2-D nodes `n₂` (3-D node `layer · n₂ + node`).
    pub n_face_nodes: usize,
    /// Layer coordinates `z[0] < … < z[nz]`.
    pub z: Vec<f64>,
    /// Source 2-D triangle of every tet.
    pub tet_source_tri: Vec<usize>,
    /// Slab of every tet.
    pub tet_slab: Vec<usize>,
    /// The `z = z[0]` end face: the 2-D triangles in 2-D order (3-D nodes).
    pub port1_faces: Vec<[u32; 3]>,
    /// The `z = z[nz]` end face, 2-D triangle order.
    pub port2_faces: Vec<[u32; 3]>,
    /// The 2-D edge table the [`ExtrudedEdge`] indices refer to.
    pub face_edges: Vec<[u32; 2]>,
}

/// Extrude `tri` (CCW or CW triangles) along `z ∈ [0, length]` through `nz`
/// equal slabs (module docs).
///
/// # Panics
///
/// Panics if `nz == 0` or `length ≤ 0`.
pub fn extrude_tri_mesh(tri: &TriMesh, nz: usize, length: f64) -> ExtrudedTriMesh {
    assert!(nz >= 1, "extrusion needs at least one slab");
    assert!(length > 0.0, "extrusion length must be positive");
    let z: Vec<f64> = (0..=nz).map(|k| length * k as f64 / nz as f64).collect();
    extrude_tri_mesh_layers(tri, &z)
}

/// [`extrude_tri_mesh`] through the given strictly increasing layer
/// coordinates `z` (at least two).
///
/// # Panics
///
/// Panics on fewer than two layers or a non-increasing `z`.
pub fn extrude_tri_mesh_layers(tri: &TriMesh, z: &[f64]) -> ExtrudedTriMesh {
    assert!(z.len() >= 2, "extrusion needs at least two layers");
    assert!(
        z.windows(2).all(|w| w[1] > w[0]),
        "layer coordinates must be strictly increasing"
    );
    let n2 = tri.n_nodes();
    let nz = z.len() - 1;
    let node = |layer: usize, v: u32| (layer * n2) as u32 + v;
    let mut nodes = Vec::with_capacity(n2 * (nz + 1));
    for &zk in z {
        for p in &tri.nodes {
            nodes.push([p[0], p[1], zk]);
        }
    }
    let mut tets = Vec::with_capacity(3 * nz * tri.n_tris());
    let mut tet_source_tri = Vec::with_capacity(tets.capacity());
    let mut tet_slab = Vec::with_capacity(tets.capacity());
    for slab in 0..nz {
        for (ti, t) in tri.tris.iter().enumerate() {
            let mut v = *t;
            v.sort_unstable();
            let (b, u) = (|k: usize| node(slab, v[k]), |k: usize| node(slab + 1, v[k]));
            for mut tet in [
                [b(0), b(1), b(2), u(2)],
                [b(0), b(1), u(1), u(2)],
                [b(0), u(0), u(1), u(2)],
            ] {
                if signed_volume(&nodes, &tet) < 0.0 {
                    tet.swap(0, 1);
                }
                tets.push(tet);
                tet_source_tri.push(ti);
                tet_slab.push(slab);
            }
        }
    }
    let port1_faces = tri.tris.iter().map(|t| t.map(|v| node(0, v))).collect();
    let port2_faces = tri.tris.iter().map(|t| t.map(|v| node(nz, v))).collect();
    ExtrudedTriMesh {
        mesh: TetMesh {
            nodes,
            tets,
            physical_groups: BTreeMap::new(),
        },
        n_face_nodes: n2,
        z: z.to_vec(),
        tet_source_tri,
        tet_slab,
        port1_faces,
        port2_faces,
        face_edges: tri.edges(),
    }
}

fn signed_volume(nodes: &[[f64; 3]], t: &[u32; 4]) -> f64 {
    let p = |k: usize| nodes[t[k] as usize];
    let d = |a: [f64; 3], b: [f64; 3]| [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let (a, b, c) = (d(p(0), p(1)), d(p(0), p(2)), d(p(0), p(3)));
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

impl ExtrudedTriMesh {
    /// Number of slabs `nz`.
    pub fn n_slabs(&self) -> usize {
        self.z.len() - 1
    }

    /// Extrusion length `z[nz] − z[0]`.
    pub fn length(&self) -> f64 {
        self.z[self.z.len() - 1] - self.z[0]
    }

    /// Classify every edge of `mesh.edges()` (same order).
    ///
    /// # Panics
    ///
    /// Panics if an edge is none of the three kinds (impossible for a mesh
    /// built by this module).
    pub fn edge_kinds(&self) -> Vec<ExtrudedEdge> {
        let n2 = self.n_face_nodes as u32;
        let lookup: HashMap<(u32, u32), usize> = self
            .face_edges
            .iter()
            .enumerate()
            .map(|(i, e)| ((e[0], e[1]), i))
            .collect();
        self.mesh
            .edges()
            .iter()
            .map(|&[a, b]| {
                let (la, va) = ((a / n2) as usize, a % n2);
                let (lb, vb) = ((b / n2) as usize, b % n2);
                if la == lb {
                    ExtrudedEdge::Horizontal {
                        edge: lookup[&(va.min(vb), va.max(vb))],
                        layer: la,
                    }
                } else if va == vb {
                    ExtrudedEdge::Vertical {
                        node: va as usize,
                        slab: la.min(lb),
                    }
                } else {
                    let e = *lookup
                        .get(&(va.min(vb), va.max(vb)))
                        .expect("lateral diagonal over a 2-D edge");
                    ExtrudedEdge::Diagonal {
                        edge: e,
                        slab: la.min(lb),
                    }
                }
            })
            .collect()
    }

    /// A per-edge mask over `mesh.edges()` from a predicate on
    /// [`ExtrudedEdge`] (e.g. the PEC set of a conductor that changes along
    /// `z`). The predicate returns `true` for an edge to **eliminate**; the
    /// result is a [`crate::driven::solve::DrivenBcs::pec_interior_mask`]
    /// (`true` = kept).
    pub fn pec_interior_mask_by(&self, mut is_pec: impl FnMut(ExtrudedEdge) -> bool) -> Vec<bool> {
        self.edge_kinds().into_iter().map(|k| !is_pec(k)).collect()
    }

    /// The `z`-invariant PEC mask of a 2-D PEC edge set `pec_edges_2d`
    /// (over [`Self::face_edges`]; e.g. [`crate::analytic::port_modes::HybridPecMasks::pec_edges`]):
    /// every horizontal and lateral-diagonal edge over a PEC 2-D edge and
    /// every vertical edge over a PEC 2-D node (a node touching a PEC edge)
    /// is eliminated. That is exactly the set of edges lying on the extruded
    /// PEC surfaces (shield walls, sheet strips, the walls of carved
    /// conductors). The two end faces are **not** eliminated beyond their
    /// PEC 2-D edges (they are the ports).
    ///
    /// # Panics
    ///
    /// Panics if `pec_edges_2d` has the wrong length.
    pub fn pec_interior_mask(&self, pec_edges_2d: &[bool]) -> Vec<bool> {
        assert_eq!(
            pec_edges_2d.len(),
            self.face_edges.len(),
            "pec_edges_2d length"
        );
        let mut pec_node = vec![false; self.n_face_nodes];
        for (e, &p) in self.face_edges.iter().zip(pec_edges_2d) {
            if p {
                pec_node[e[0] as usize] = true;
                pec_node[e[1] as usize] = true;
            }
        }
        self.pec_interior_mask_by(|k| match k {
            ExtrudedEdge::Horizontal { edge, .. } | ExtrudedEdge::Diagonal { edge, .. } => {
                pec_edges_2d[edge]
            }
            ExtrudedEdge::Vertical { node, .. } => pec_node[node],
        })
    }

    /// The two lateral triangles over 2-D edge `edge` in slab `slab` (3-D
    /// nodes), split along the [`ExtrudedEdge::Diagonal`]. A list of these
    /// over a conductor's 2-D edges is the conductor surface as triangles,
    /// the input [`crate::mesh::pec_interior_mask_from_triangles`] expects.
    pub fn lateral_triangles(&self, edge: usize, slab: usize) -> [[u32; 3]; 2] {
        let n2 = self.n_face_nodes as u32;
        let [a, b] = self.face_edges[edge];
        let (lo, hi) = (slab as u32 * n2, (slab as u32 + 1) * n2);
        [[lo + a, lo + b, hi + b], [lo + a, hi + a, hi + b]]
    }

    /// Per-tet values from per-triangle ones (`value_tri[tet_source_tri]`).
    pub fn per_tet<T: Copy>(&self, value_tri: &[T]) -> Vec<T> {
        self.tet_source_tri.iter().map(|&t| value_tri[t]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytic::waveguide::rect_tri_mesh;

    fn jittered_mesh() -> TriMesh {
        let mut m = rect_tri_mesh(5, 4, 2.0, 1.0);
        // Deterministic interior jitter; keeps the triangulation valid.
        for (k, p) in m.nodes.iter_mut().enumerate() {
            if p[0] > 1e-9 && p[0] < 2.0 - 1e-9 && p[1] > 1e-9 && p[1] < 1.0 - 1e-9 {
                let s = ((k * 7919) % 13) as f64 / 13.0 - 0.5;
                p[0] += 0.08 * s;
                p[1] += 0.05 * (0.5 - s);
            }
        }
        m
    }

    #[test]
    fn extrusion_is_conforming_and_right_handed() {
        let tri = jittered_mesh();
        let ex = extrude_tri_mesh(&tri, 3, 1.5);
        let mesh = &ex.mesh;
        assert_eq!(mesh.n_tets(), 3 * 3 * tri.n_tris());
        let mut vol = 0.0;
        for t in &mesh.tets {
            let v = signed_volume(&mesh.nodes, t);
            assert!(v > 0.0, "tet {t:?} volume {v}");
            vol += v / 6.0;
        }
        assert!((vol - 2.0 * 1.5).abs() < 1e-12, "volume {vol}");
        // Conforming: every interior triangular face is shared by exactly two
        // tets, every boundary face by one, and the boundary area is the
        // prism surface.
        let mut faces: HashMap<[u32; 3], usize> = HashMap::new();
        for t in &mesh.tets {
            for lf in &crate::mesh::TET_LOCAL_FACES {
                let mut f = [t[lf[0]], t[lf[1]], t[lf[2]]];
                f.sort_unstable();
                *faces.entry(f).or_default() += 1;
            }
        }
        let mut boundary_area = 0.0;
        for (f, &c) in &faces {
            assert!(c <= 2, "face {f:?} shared by {c} tets");
            if c == 1 {
                let p = |k: usize| mesh.nodes[f[k] as usize];
                let d = |a: [f64; 3], b: [f64; 3]| [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let (a, b) = (d(p(0), p(1)), d(p(0), p(2)));
                let cr = [
                    a[1] * b[2] - a[2] * b[1],
                    a[2] * b[0] - a[0] * b[2],
                    a[0] * b[1] - a[1] * b[0],
                ];
                boundary_area += 0.5 * (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            }
        }
        let expect = 2.0 * 2.0 * 1.0 + 2.0 * (2.0 + 1.0) * 1.5;
        assert!((boundary_area - expect).abs() < 1e-10, "{boundary_area}");
        // Euler characteristic of a ball: V − E + F − T = 1.
        let (v, e, f, t) = (
            mesh.n_nodes() as i64,
            mesh.edges().len() as i64,
            faces.len() as i64,
            mesh.n_tets() as i64,
        );
        assert_eq!(v - e + f - t, 1);
    }

    #[test]
    fn edge_kinds_cover_every_edge_once() {
        let tri = jittered_mesh();
        let ex = extrude_tri_mesh_layers(&tri, &[0.0, 0.3, 1.0]);
        let kinds = ex.edge_kinds();
        let n_e2 = tri.edges().len();
        let n_n2 = tri.n_nodes();
        let hor = kinds
            .iter()
            .filter(|k| matches!(k, ExtrudedEdge::Horizontal { .. }))
            .count();
        let ver = kinds
            .iter()
            .filter(|k| matches!(k, ExtrudedEdge::Vertical { .. }))
            .count();
        let dia = kinds
            .iter()
            .filter(|k| matches!(k, ExtrudedEdge::Diagonal { .. }))
            .count();
        assert_eq!(hor, 3 * n_e2);
        assert_eq!(ver, 2 * n_n2);
        assert_eq!(dia, 2 * n_e2);
        // The triangle route to the PEC mask equals the edge-kind route.
        let pec2: Vec<bool> = (0..n_e2).map(|e| e % 3 == 0).collect();
        let mask = ex.pec_interior_mask(&pec2);
        let edges = ex.mesh.edges();
        let mut tris = Vec::new();
        for slab in 0..ex.n_slabs() {
            for e in (0..n_e2).filter(|&e| pec2[e]) {
                tris.extend(ex.lateral_triangles(e, slab));
            }
        }
        for layer in 0..=ex.n_slabs() {
            for e in (0..n_e2).filter(|&e| pec2[e]) {
                let [a, b] = tri.edges()[e];
                let o = (layer * n_n2) as u32;
                // A degenerate "triangle" carrying just the horizontal edge.
                tris.push([o + a, o + b, o + b]);
            }
        }
        let via_tris = crate::mesh::pec_interior_mask_from_triangles(&edges, &[tris.as_slice()]);
        assert_eq!(mask, via_tris);
    }
}
