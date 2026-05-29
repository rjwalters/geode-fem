//! Discrete de Rham complex operators on a tetrahedral mesh.
//!
//! This module provides the first map of the discrete de Rham complex —
//! the **discrete gradient** `d⁰` (a "coboundary" / incidence operator) —
//! which sends a nodal scalar field to the edge DOFs of its discrete
//! gradient. It is the algebraic backbone of the U(1) gauge-symmetry test
//! battery: the Nédélec curl-curl operator must annihilate anything in the
//! image of `d⁰` (gradients are curl-free), and in Phase 2 the composition
//! `d¹ · d⁰ = 0` certifies the complex is exact at the edge level.
//!
//! # The `d⁰` operator
//!
//! For a mesh with `n_nodes` vertices and `n_edges` edges, `d⁰` is the
//! `n_edges × n_nodes` integer incidence matrix. Each **row** is a global
//! edge `(a, b)` with `a < b` (the lower-tag-first orientation already
//! fixed by [`crate::mesh::TET_LOCAL_EDGES`] and the Nédélec edge-DOF
//! assembly), and has exactly two nonzeros:
//!
//! ```text
//! d⁰[edge, a] = -1     (tail of the oriented edge)
//! d⁰[edge, b] = +1     (head of the oriented edge)
//! ```
//!
//! Applied to a nodal field `φ`, row `(a, b)` yields `φ_b − φ_a`, the
//! signed difference along the oriented edge. For a Whitney 1-form (the
//! first-order Nédélec basis), this edge difference *is* the edge DOF of
//! the discrete gradient `∇φ`: the line integral of `∇φ` along the edge
//! equals `φ_b − φ_a` by the fundamental theorem of calculus, and the
//! Whitney edge DOF is exactly that tangential line integral. Hence the
//! sign convention here must match the Nédélec edge orientation so that
//! `assemble_global_nedelec`'s curl-curl operator consumes `d⁰`-output as
//! a genuine discrete gradient.
//!
//! # References for the sign convention
//!
//! The lower-tag-first edge orientation and the `(-1, +1)` incidence
//! pattern follow the standard finite-element exterior-calculus
//! treatment of the Whitney / discrete de Rham complex:
//!
//! - R. Hiptmair, *Finite elements in computational electromagnetism*,
//!   Acta Numerica 11 (2002), 237–339, §3 ("Discrete differential
//!   forms"): the discrete exterior derivative on edges is the signed
//!   node–edge incidence matrix, oriented by the global vertex ordering.
//! - D. N. Arnold, R. S. Falk, R. Winther, *Finite element exterior
//!   calculus, homological techniques, and applications*, Acta Numerica
//!   15 (2006), 1–155, §1.2 ("The de Rham complex"): `d⁰` is the
//!   transpose of the boundary operator on edges, giving `+1` at the head
//!   vertex and `−1` at the tail vertex of each oriented edge.
//!
//! # Value type
//!
//! `d⁰` is mathematically an *integer* matrix, but `faer`'s sparse
//! constructors require the value type to implement `faer`'s
//! `ComplexField`, which `i32` does not. We therefore store the entries
//! as `f64` (the exactly-representable integers `±1.0`). This keeps the
//! operator usable in `faer`'s sparse linear algebra and lets Phase 2 form
//! the sparse product `d¹ · d⁰` directly; for the small integers involved
//! the product stays exact in `f64`. It also matches the value type of
//! the rest of the sparse pipeline ([`crate::sparse::SparseSystem`]).

use faer::sparse::{SparseColMat, Triplet};

use crate::mesh::TetMesh;

/// Build the discrete gradient operator `d⁰` for `mesh`.
///
/// Returns an `n_edges × n_nodes` sparse matrix in `faer`'s column-major
/// (CSC) format. Row `i` corresponds to edge `i` of [`TetMesh::edges`]
/// (the canonical lower-tag-first global edge enumeration, identical to
/// the Nédélec assembly's edge ordering), with exactly two nonzeros:
/// `-1` in the column of the lower-tagged endpoint and `+1` in the column
/// of the higher-tagged endpoint.
///
/// See the [module docs](crate::derham) for the sign convention and its
/// references (Hiptmair §3, Arnold–Falk–Winther §1.2).
///
/// # Panics
///
/// Panics if `faer` rejects the constructed triplets (only possible on an
/// internal invariant violation — the edge list is deduplicated and the
/// node indices are bounded by `mesh.n_nodes()` by construction).
pub fn gradient_map(mesh: &TetMesh) -> SparseColMat<usize, f64> {
    let edges = mesh.edges();
    let n_edges = edges.len();
    let n_nodes = mesh.n_nodes();

    // Two triplets per edge: -1 at the tail (lower tag), +1 at the head.
    let mut triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(2 * n_edges);
    for (edge_idx, &[a, b]) in edges.iter().enumerate() {
        triplets.push(Triplet::new(edge_idx, a as usize, -1.0));
        triplets.push(Triplet::new(edge_idx, b as usize, 1.0));
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(n_edges, n_nodes, &triplets)
        .expect("d⁰ triplets are well-formed: in-range indices, no duplicates per edge")
}

/// Apply `d⁰` to a nodal field without materializing the sparse matrix.
///
/// Returns `d⁰ · φ`, the vector of edge DOFs of the discrete gradient:
/// for edge `(a, b)` (lower tag first), `out[edge] = nodal[b] − nodal[a]`.
/// The output length is `mesh.edges().len()` (= `n_edges`), and entry `i`
/// matches row `i` of [`gradient_map`].
///
/// Prefer this over `gradient_map(mesh)` followed by a sparse mat-vec when
/// you only need the gradient values and not the operator itself.
///
/// # Panics
///
/// Panics if `nodal.len() != mesh.n_nodes()`.
pub fn apply_gradient(mesh: &TetMesh, nodal: &[f64]) -> Vec<f64> {
    assert_eq!(
        nodal.len(),
        mesh.n_nodes(),
        "nodal field length {} disagrees with mesh node count {}",
        nodal.len(),
        mesh.n_nodes()
    );

    mesh.edges()
        .iter()
        .map(|&[a, b]| nodal[b as usize] - nodal[a as usize])
        .collect()
}
