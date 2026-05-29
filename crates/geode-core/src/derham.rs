//! Discrete de Rham complex operators on a tetrahedral mesh.
//!
//! This module provides the first two maps of the discrete de Rham
//! complex on a tetrahedral mesh:
//!
//! ```text
//! ℝ^{n_nodes}  --d⁰-->  ℝ^{n_edges}  --d¹-->  ℝ^{n_faces}  --d²-->  ℝ^{n_tets}
//!     H¹              H(curl)             H(div)             L²
//! ```
//!
//! - [`gradient_map`] is `d⁰`: the discrete gradient / node-edge
//!   incidence matrix that sends a nodal scalar field to its edge DOFs.
//! - [`curl_map`] is `d¹`: the discrete curl / edge-face incidence
//!   matrix that sends an edge-DOF field (a discrete 1-form / Nédélec
//!   tangential line integral) to its face DOFs (a discrete 2-form /
//!   Raviart–Thomas normal flux).
//!
//! Together, `(d⁰, d¹)` form the algebraic backbone of the U(1)
//! gauge-symmetry test battery: the Nédélec curl-curl operator must
//! annihilate anything in the image of `d⁰` (gradients are curl-free),
//! and the composition `d¹ ∘ d⁰ ≡ 0` certifies the complex is exact at
//! the edge level. The bit-exact identity `curl_map · gradient_map = 0`
//! is the Phase 2.B acceptance test of Epic #57.
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
//! # The `d¹` operator
//!
//! For a mesh with `n_edges` edges and `n_faces` faces, `d¹` is the
//! `n_faces × n_edges` integer incidence matrix. Each **row** is a global
//! face `(a, b, c)` with `a < b < c` (the lower-tag-first orientation
//! fixed by [`crate::mesh::TET_LOCAL_FACES`] / [`TetMesh::faces`]) and
//! has exactly three nonzeros, encoding the signed boundary cycle
//! `a → b → c → a`:
//!
//! ```text
//! d¹[face, edge(a, b)] = +1    (forward leg of the cycle)
//! d¹[face, edge(b, c)] = +1    (forward leg of the cycle)
//! d¹[face, edge(a, c)] = -1    (reverse of the canonical (a, c) edge)
//! ```
//!
//! Applied to an edge-DOF field `u`, row `(a, b, c)` yields the signed
//! circulation `u[(a,b)] + u[(b,c)] − u[(a,c)]`, which for a Whitney
//! 1-form (Nédélec edge DOF = tangential line integral) is exactly the
//! Stokes-theorem value `∮_∂f u · dℓ = ∫_f (∇ × u) · n dA`. The face DOF
//! convention matched here is Raviart–Thomas normal flux on the
//! ascending-cycle orientation of the face.
//!
//! # The composition `d¹ ∘ d⁰`
//!
//! On every global face `(a, b, c)` with `a < b < c`, the column-`k`
//! entry of `d¹ · d⁰` is
//!
//! ```text
//! (+1)(δ_{k,b} − δ_{k,a}) + (+1)(δ_{k,c} − δ_{k,b}) + (−1)(δ_{k,c} − δ_{k,a}) ≡ 0
//! ```
//!
//! for every `k`. The identity is bit-exact in `f64` because every term
//! is `±1.0` and the partial sums fit well below `2^{53}`.
//!
//! # References for the sign conventions
//!
//! The lower-tag-first edge / face orientations and the signed-incidence
//! patterns follow the standard finite-element exterior-calculus
//! treatment of the Whitney / discrete de Rham complex:
//!
//! - R. Hiptmair, *Finite elements in computational electromagnetism*,
//!   Acta Numerica 11 (2002), 237–339, §3 ("Discrete differential
//!   forms") for `d⁰` as the signed node–edge incidence matrix; §4
//!   ("Higher order forms" / face conventions) for `d¹` as the signed
//!   edge–face incidence matrix and the lift to Raviart–Thomas face
//!   DOFs.
//! - D. N. Arnold, R. S. Falk, R. Winther, *Finite element exterior
//!   calculus, homological techniques, and applications*, Acta Numerica
//!   15 (2006), 1–155, §1.2 ("The de Rham complex"): `d⁰` and `d¹` as
//!   the transposes of the simplicial boundary operators, with the sign
//!   convention given by the induced orientation of each oriented
//!   simplex's boundary.
//!
//! # Value type
//!
//! `d⁰` and `d¹` are mathematically *integer* matrices, but `faer`'s
//! sparse constructors require the value type to implement `faer`'s
//! `ComplexField`, which `i32` does not. We therefore store the entries
//! as `f64` (the exactly-representable integers `±1.0`). This keeps the
//! operators usable in `faer`'s sparse linear algebra and lets the
//! sparse product `d¹ · d⁰` be formed directly; for the small integers
//! involved the product stays exact in `f64` (bit-exact integer sums
//! hold below `2^{53}`). It also matches the value type of the rest of
//! the sparse pipeline ([`crate::sparse::SparseSystem`]).

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

/// Build the discrete curl operator `d¹` for `mesh`.
///
/// Returns an `n_faces × n_edges` sparse matrix in `faer`'s column-major
/// (CSC) format. Row `i` corresponds to face `i` of [`TetMesh::faces`]
/// (the canonical lower-tag-first global face enumeration `(a, b, c)`
/// with `a < b < c`), with exactly three nonzeros encoding the signed
/// boundary cycle `a → b → c → a`:
///
/// ```text
/// d¹[face, edge(a, b)] = +1
/// d¹[face, edge(b, c)] = +1
/// d¹[face, edge(a, c)] = -1
/// ```
///
/// The choice pins `curl_map(mesh) · gradient_map(mesh) ≡ 0` bit-exactly
/// on any mesh — the discrete `d¹ ∘ d⁰ ≡ 0` identity of the de Rham
/// complex (see the [module docs](crate::derham) and Hiptmair §4,
/// Arnold–Falk–Winther §1.2).
///
/// # Implementation notes
///
/// We assemble `d¹` from the *global* face enumeration
/// ([`TetMesh::faces`], one row per deduplicated face), mirroring how
/// [`gradient_map`] enumerates global edges. Walking each global face
/// `(a, b, c)` in the ascending cycle gives the three triplets directly,
/// independent of any local tet's orientation. The per-tet view
/// ([`TetMesh::tet_faces`] / [`crate::mesh::TET_LOCAL_FACE_EDGES`]) is
/// reserved for the eventual `d²` operator that will scatter signed
/// face DOFs into volume DOFs.
///
/// # Panics
///
/// Panics if `faer` rejects the constructed triplets (only possible on
/// an internal invariant violation — face indices are bounded by
/// `n_faces` and edge indices by `n_edges` by construction).
pub fn curl_map(mesh: &TetMesh) -> SparseColMat<usize, f64> {
    use std::collections::HashMap;

    let edges = mesh.edges();
    let faces = mesh.faces();
    let n_edges = edges.len();
    let n_faces = faces.len();

    // Edge lookup: canonical (a, b) with a < b → global edge index.
    let mut edge_idx: HashMap<(u32, u32), u32> = HashMap::with_capacity(n_edges);
    for (i, e) in edges.iter().enumerate() {
        edge_idx.insert((e[0], e[1]), i as u32);
    }

    // For each global face (α, β, γ) with α < β < γ, emit three triplets:
    //   (face, edge(α, β),  +1)
    //   (face, edge(β, γ),  +1)
    //   (face, edge(α, γ),  -1)
    //
    // We enumerate global faces directly (not per-tet) so each row is
    // populated exactly once; this keeps the sign pattern decoupled from
    // any tet-local accounting and matches `gradient_map`'s row-per-edge
    // structure.
    let mut triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(3 * n_faces);
    for (face_idx, &[a, b, c]) in faces.iter().enumerate() {
        debug_assert!(a < b && b < c, "faces() must return ascending triples");
        let e_ab = edge_idx[&(a, b)] as usize;
        let e_bc = edge_idx[&(b, c)] as usize;
        let e_ac = edge_idx[&(a, c)] as usize;
        triplets.push(Triplet::new(face_idx, e_ab, 1.0));
        triplets.push(Triplet::new(face_idx, e_bc, 1.0));
        triplets.push(Triplet::new(face_idx, e_ac, -1.0));
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(n_faces, n_edges, &triplets)
        .expect("d¹ triplets are well-formed: in-range indices, distinct per face")
}
