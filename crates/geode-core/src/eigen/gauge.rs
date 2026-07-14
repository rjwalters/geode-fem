//! Tree-cotree gauging for the Nédélec curl-curl eigen path (issue #502).
//!
//! # The spurious gradient nullspace
//!
//! The first-order Nédélec curl-curl stiffness `K` has a large nullspace:
//! its kernel is exactly the image of the discrete gradient `d⁰`
//! (`kernel(K) = image(d⁰)`, the de-Rham identity — see
//! [`crate::assembly::nedelec::spurious_dim_from_derham`] and Epic #57
//! Phase 3.A). After PEC (Dirichlet) reduction, that kernel has dimension
//! `rank(d⁰_interior)`, one gradient mode per free (non-grounded) node of
//! the interior graph. These are non-physical: the physical curl-curl
//! spectrum lives in the cotree complement. In an *un-projected*
//! shift-invert Lanczos solve they appear as a near-zero-λ cluster
//! (λ ≈ 1e-16…1e-17) plus, occasionally, a gradient-adjacent mode that
//! leaks *into* the physical band (the transmon benchmark's spurious
//! 3.4528 GHz mode).
//!
//! # Tree-cotree: eliminate the gradient DOFs algebraically
//!
//! A **spanning tree** of the mesh node graph picks exactly
//! `rank(d⁰_interior)` edges whose removal from the edge (Nédélec DOF) set
//! leaves a basis on which `d⁰` restricted to the remaining **cotree**
//! edges is injective — i.e. the gradient nullspace is gone by
//! construction (Albanese–Rubinacci 1988; Hiptmair, *Acta Numerica* 2002,
//! §6). Because the eliminated **tree edges** are precisely one per free
//! node, the count of removed DOFs equals the algebraic spurious-mode
//! dimension exactly. The remaining cotree pencil is smaller and its
//! `K_cc` block has a trivial gradient nullspace, with **no change to the
//! Lanczos core** ([`crate::eigen::lanczos::SparseShiftInvertLanczos`]):
//! the gauge only reshapes the reduced `(K, M)` index set before the solve.
//!
//! # IMPORTANT: DOF elimination is a *source*-problem gauge, NOT a
//! spectrum-preserving eigen gauge
//!
//! Setting the tree-edge DOFs to zero (dropping their rows/cols) is the
//! standard gauge for the curl-curl **source** problem `K x = b` with a
//! solenoidal RHS — it removes `kernel(K) = image(d⁰)` and the cotree block
//! `K_cc` inherits the physical spectrum. It is **not** correct for the
//! generalized **eigenproblem** `K x = λ M x`: the physical eigenvectors
//! have nonzero tree-edge components, and dropping the tree rows/cols of
//! BOTH `K` and `M` imposes an artificial `x_tree = 0` constraint that
//! *shifts* the computed spectrum. Measured on the transmon fixture
//! (issue #502), the gauged resonator drifts 1.64% (5.2372 vs 5.1528 GHz)
//! — outside the ≤1% cross-validation bar — and a dense low cluster
//! persists. The spectrum-preserving construction is a **divergence-free
//! projection** `Zᵀ K Z, Zᵀ M Z` (the issue's other option / the filed
//! follow-on), for which this spanning tree supplies the cotree basis.
//!
//! This module therefore ships the *structural* tree-cotree machinery (the
//! spanning forest, the boundary convention, the count identity
//! `tree_edges == rank(d⁰_interior)`), which is correct and reusable, while
//! the eigen-path spurious-mode removal remains the documented follow-on.
//!
//! # The boundary (PEC) convention — get this right
//!
//! Gradient fields supported entirely on PEC-eliminated DOFs are already
//! gone with the interior reduction. The gauge must span the **interior**
//! node graph, and the PEC boundary must be treated as a **single grounded
//! super-node** (equivalently: the spanning tree is rooted at the
//! boundary). A naive tree that ignores the boundary either *leaks* (fails
//! to gauge a whole gradient-mode's worth of DOFs, because it plants a
//! redundant root inside a component that already touches ground) or
//! *over-constrains* (gauges away a physical cotree edge). Concretely:
//!
//! - A node is **grounded** iff it is an endpoint of at least one PEC
//!   (excluded) edge — i.e. it lies on a Dirichlet surface.
//! - All grounded nodes are pre-merged into one union-find component (the
//!   root). The forest is grown over the **kept interior edges** only.
//! - An interior edge joining two so-far-disjoint components is a **tree
//!   edge** (gauged away); an edge whose endpoints are already connected is
//!   a **cotree edge** (kept).
//!
//! With this convention the number of tree edges equals
//! `rank(d⁰_interior)` on a boundary-touching connected mesh, matching the
//! near-zero-λ cluster size the ungauged solve exhibits.

use std::collections::BTreeSet;

/// A tree-cotree gauge over the PEC-reduced interior edge set.
///
/// Built by [`TreeCotreeGauge::build`] from the global edge list and the
/// per-edge interior mask (the same `interior_mask` the ungauged path
/// consumes). It records which interior edges are **tree edges** (gauged
/// away) and provides a reindex from the ungauged interior DOF numbering
/// to the smaller **gauged** (cotree) DOF numbering.
#[derive(Debug, Clone)]
pub struct TreeCotreeGauge {
    /// Per-global-edge gauged DOF index: `Some(g)` for a kept **cotree**
    /// interior edge (its column in the gauged pencil), `None` for a PEC
    /// edge **or** a spanning-tree edge (both eliminated).
    gauged_index: Vec<Option<usize>>,
    /// Number of cotree (kept) interior DOFs = `gauged_dim`.
    gauged_dim: usize,
    /// Number of spanning-tree edges eliminated (= gradient-nullspace
    /// dimension on a boundary-touching connected mesh).
    tree_edges: usize,
    /// Number of interior DOFs *before* gauging (for the count check).
    interior_dim: usize,
}

/// Minimal union-find (disjoint-set) with path compression and union by
/// rank — near-linear over the edge count, no dense linear algebra, so it
/// scales to the 133k-DOF transmon mesh.
struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n as u32).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, mut x: u32) -> u32 {
        while self.parent[x as usize] != x {
            let gp = self.parent[self.parent[x as usize] as usize];
            self.parent[x as usize] = gp; // path halving
            x = gp;
        }
        x
    }

    /// Union `a` and `b`; returns `true` if they were previously disjoint
    /// (i.e. this call actually merged two components → a tree edge).
    fn union(&mut self, a: u32, b: u32) -> bool {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return false;
        }
        match self.rank[ra as usize].cmp(&self.rank[rb as usize]) {
            std::cmp::Ordering::Less => self.parent[ra as usize] = rb,
            std::cmp::Ordering::Greater => self.parent[rb as usize] = ra,
            std::cmp::Ordering::Equal => {
                self.parent[rb as usize] = ra;
                self.rank[ra as usize] += 1;
            }
        }
        true
    }
}

impl TreeCotreeGauge {
    /// Build the tree-cotree gauge from the global `edges` table and the
    /// per-edge `interior_mask` (`true` = kept interior edge, `false` = PEC
    /// edge). `n_nodes` is the mesh node count.
    ///
    /// The spanning forest is grown over the **kept** interior edges with
    /// all grounded (PEC-incident) nodes pre-merged into one root
    /// component. Interior edges that join two disjoint components are
    /// eliminated as tree edges; the rest are kept as cotree DOFs.
    ///
    /// # Panics
    ///
    /// Panics if `interior_mask.len() != edges.len()`, or if any edge
    /// endpoint is out of range of `n_nodes`.
    pub fn build(edges: &[[u32; 2]], interior_mask: &[bool], n_nodes: usize) -> Self {
        assert_eq!(
            interior_mask.len(),
            edges.len(),
            "interior_mask must align with the global edge list"
        );

        // Grounded nodes: any endpoint of a PEC (excluded) edge lies on a
        // Dirichlet surface. Collected first so the boundary acts as one
        // grounded super-node in the union-find below.
        let mut grounded: BTreeSet<u32> = BTreeSet::new();
        for (e, &keep) in interior_mask.iter().enumerate() {
            if !keep {
                let [a, b] = edges[e];
                grounded.insert(a);
                grounded.insert(b);
            }
        }

        let mut uf = UnionFind::new(n_nodes);
        // Pre-merge all grounded nodes into a single root super-node.
        let mut ground_root: Option<u32> = None;
        for &g in &grounded {
            match ground_root {
                None => ground_root = Some(g),
                Some(r) => {
                    uf.union(r, g);
                }
            }
        }

        // Grow the spanning forest over the kept interior edges. Because
        // the grounded nodes are already merged, the very first interior
        // edge that reaches the boundary component is (correctly) a tree
        // edge, and no redundant root is planted inside a grounded
        // component.
        let mut gauged_index = vec![None; edges.len()];
        let mut gauged_dim = 0usize;
        let mut tree_edges = 0usize;
        let mut interior_dim = 0usize;
        for (e, &keep) in interior_mask.iter().enumerate() {
            if !keep {
                continue;
            }
            interior_dim += 1;
            let [a, b] = edges[e];
            if uf.union(a, b) {
                // Joined two components → spanning-tree edge → gauge away.
                tree_edges += 1;
            } else {
                // Endpoints already connected → cotree edge → keep.
                gauged_index[e] = Some(gauged_dim);
                gauged_dim += 1;
            }
        }

        Self {
            gauged_index,
            gauged_dim,
            tree_edges,
            interior_dim,
        }
    }

    /// The gauged (cotree) DOF index of a global edge: `Some(g)` for a kept
    /// cotree interior edge, `None` for a PEC or spanning-tree edge.
    #[inline]
    pub fn gauged_index(&self, global_edge: usize) -> Option<usize> {
        self.gauged_index[global_edge]
    }

    /// The per-global-edge gauged reindex vector (drop-in replacement for
    /// the ungauged `interior_index` in the reduced assembly).
    pub fn gauged_index_map(&self) -> &[Option<usize>] {
        &self.gauged_index
    }

    /// Number of kept cotree DOFs (the dimension of the gauged pencil).
    #[inline]
    pub fn gauged_dim(&self) -> usize {
        self.gauged_dim
    }

    /// Number of spanning-tree edges eliminated. On a boundary-touching
    /// connected mesh this equals `rank(d⁰_interior)` — the gradient-
    /// nullspace dimension, i.e. the near-zero-λ cluster size the ungauged
    /// solve exhibits.
    #[inline]
    pub fn tree_edge_count(&self) -> usize {
        self.tree_edges
    }

    /// Number of interior DOFs before gauging (`gauged_dim + tree_edges`).
    #[inline]
    pub fn interior_dim(&self) -> usize {
        self.interior_dim
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assembly::nedelec::{rank_via_svd, restrict_gradient_dense};
    use crate::mesh::{TetMesh, cube_tet_mesh};

    /// Interior-node mask companion to a PEC edge mask: a node is interior
    /// iff it is NOT an endpoint of any excluded edge (mirrors the grounded
    /// convention used inside `build`).
    fn interior_node_mask(edges: &[[u32; 2]], interior_mask: &[bool], n_nodes: usize) -> Vec<bool> {
        let mut grounded = vec![false; n_nodes];
        for (e, &keep) in interior_mask.iter().enumerate() {
            if !keep {
                grounded[edges[e][0] as usize] = true;
                grounded[edges[e][1] as usize] = true;
            }
        }
        grounded.iter().map(|&g| !g).collect()
    }

    /// Full-outer-PEC cube: every boundary face is metal. The tree-edge
    /// count must equal `rank(d⁰_interior)` (the de-Rham gradient
    /// dimension), and the gauged dim must be `interior_dim − tree_edges`.
    #[test]
    fn tree_edge_count_matches_derham_rank() {
        for n in [2usize, 3, 4] {
            let mesh = cube_tet_mesh(n, 1.0);
            let edges = mesh.edges();
            let n_nodes = mesh.n_nodes();

            // PEC on the entire outer boundary of the cube.
            let metal: Vec<[u32; 3]> = mesh
                .faces()
                .into_iter()
                .filter(|f| {
                    let on = |c: usize, v: f64| {
                        f.iter()
                            .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12)
                    };
                    on(0, 0.0) || on(0, 1.0) || on(1, 0.0) || on(1, 1.0) || on(2, 0.0) || on(2, 1.0)
                })
                .collect();
            let interior_mask =
                crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);

            let gauge = TreeCotreeGauge::build(&edges, &interior_mask, n_nodes);

            // de-Rham gradient rank on the same interior/interior-node sets.
            let node_mask = interior_node_mask(&edges, &interior_mask, n_nodes);
            let d0 = restrict_gradient_dense(&mesh, &interior_mask, &node_mask);
            let rank = rank_via_svd(&d0, 1e-12);

            assert_eq!(
                gauge.tree_edge_count(),
                rank,
                "n={n}: tree edges {} must equal rank(d⁰_interior) {rank}",
                gauge.tree_edge_count()
            );
            assert_eq!(
                gauge.gauged_dim(),
                gauge.interior_dim() - gauge.tree_edge_count(),
                "gauged dim must be interior_dim − tree_edges"
            );
            assert!(
                gauge.gauged_dim() < gauge.interior_dim(),
                "gauging must shrink the pencil"
            );
        }
    }

    /// The gauged index map is a compaction: exactly `gauged_dim` entries
    /// are `Some`, contiguous `0..gauged_dim`, and every PEC or tree edge
    /// is `None`.
    #[test]
    fn gauged_index_is_a_dense_compaction() {
        let mesh = cube_tet_mesh(3, 1.0);
        let edges = mesh.edges();
        let n_nodes = mesh.n_nodes();
        let metal: Vec<[u32; 3]> = mesh
            .faces()
            .into_iter()
            .filter(|f| {
                let on = |c: usize, v: f64| {
                    f.iter()
                        .all(|&x| (mesh.nodes[x as usize][c] - v).abs() < 1e-12)
                };
                on(0, 0.0) || on(0, 1.0) || on(1, 0.0) || on(1, 1.0) || on(2, 0.0) || on(2, 1.0)
            })
            .collect();
        let interior_mask =
            crate::mesh::spiral::pec_interior_mask_from_triangles(&edges, &[metal.as_slice()]);
        let gauge = TreeCotreeGauge::build(&edges, &interior_mask, n_nodes);

        let mut seen = vec![false; gauge.gauged_dim()];
        let mut count = 0usize;
        for &idx in gauge.gauged_index_map() {
            if let Some(g) = idx {
                assert!(g < gauge.gauged_dim(), "gauged index out of range");
                assert!(!seen[g], "duplicate gauged index {g}");
                seen[g] = true;
                count += 1;
            }
        }
        assert_eq!(count, gauge.gauged_dim());
        assert!(seen.iter().all(|&s| s), "gauged indices not contiguous");
    }

    /// A tiny hand-checkable graph: single tet, no PEC. Six edges, four
    /// nodes, all one component. A spanning tree of 4 nodes has 3 edges, so
    /// 3 tree edges are eliminated and 3 cotree edges remain. With no PEC,
    /// the interior-node graph has 4 nodes / 1 component → rank(d⁰) = 3.
    #[test]
    fn single_tet_no_pec_hand_count() {
        let mesh = TetMesh {
            nodes: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            tets: vec![[0, 1, 2, 3]],
            physical_groups: Default::default(),
        };
        let edges = mesh.edges();
        assert_eq!(edges.len(), 6);
        let interior_mask = vec![true; edges.len()]; // no PEC
        let gauge = TreeCotreeGauge::build(&edges, &interior_mask, mesh.n_nodes());
        assert_eq!(gauge.tree_edge_count(), 3, "4 nodes → spanning tree of 3");
        assert_eq!(gauge.gauged_dim(), 3, "6 edges − 3 tree = 3 cotree");
        assert_eq!(gauge.interior_dim(), 6);
    }
}
