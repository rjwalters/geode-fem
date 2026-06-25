//! Discrete de Rham complex operators on a tetrahedral mesh.
//!
//! This module provides the full discrete de Rham complex on a
//! tetrahedral mesh:
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
//! - [`divergence_map`] is `d²`: the discrete divergence / face-tet
//!   incidence matrix that sends a face-DOF field (a discrete 2-form /
//!   Raviart–Thomas normal flux) to its volume DOFs (a discrete 3-form
//!   / piecewise-constant volume density).
//!
//! Together, `(d⁰, d¹, d²)` form the algebraic backbone of the U(1)
//! gauge-symmetry test battery: the Nédélec curl-curl operator must
//! annihilate anything in the image of `d⁰` (gradients are curl-free),
//! and the compositions `d¹ ∘ d⁰ ≡ 0` and `d² ∘ d¹ ≡ 0` certify the
//! complex is exact at the edge and face levels respectively. The
//! bit-exact identity `curl_map · gradient_map = 0` is the Phase 2.B
//! acceptance test of Epic #57; the matching bit-exact identity
//! `divergence_map · curl_map = 0` is the deferred-`d²` follow-up
//! (issue #91).
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
//! # The `d²` operator
//!
//! For a mesh with `n_faces` faces and `n_tets` tets, `d²` is the
//! `n_tets × n_faces` integer incidence matrix. Each **row** is a tet,
//! and its four nonzeros are the signed face-incidence values pulled
//! directly from [`TetMesh::tet_faces`]: row `i` has a `±1` in the
//! column of each of the four global faces of tet `i`, where the sign
//! is `+1` when the local ascending-cycle orientation of the face
//! agrees with the global ascending cycle on the face's three vertex
//! tags, and `-1` otherwise (equivalently, the parity of the
//! permutation that sorts the local-face vertex triple).
//!
//! Applied to a face-DOF field `q` (Raviart–Thomas normal flux), row
//! `i` yields the signed sum `Σ_{f ∈ ∂T_i} σ_{T,f} q_f`, the discrete
//! flux out of tet `T_i` summed over its four faces — the
//! divergence-theorem value `∫_{T_i} ∇·q dV = ∮_{∂T_i} q · n dA` in
//! Whitney-`H(div)`-conforming form.
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
//! # The composition `d² ∘ d¹`
//!
//! On every tet `T`, the row of `d² · d¹` is the sum of the signed
//! 1-cycles bounding each of the four faces of `T`. Each interior edge
//! of `T` (every one of the six tet edges sits on exactly two of `T`'s
//! four faces) is traversed once in each of the two face-boundary
//! cycles, in opposite directions induced by the two faces' opposite
//! outward normals on the shared edge — so the two contributions cancel
//! at every column. The composition is therefore the `n_tets × n_edges`
//! zero matrix bit-exactly: each entry is an integer sum of at most
//! `4 faces · 3 edges-per-face = 12` terms drawn from `{-1, 0, +1}`,
//! well below `2^{53}`. This is the "boundary of a boundary is zero"
//! identity at the face level, the second exactness identity of the
//! discrete de Rham complex.
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
//!   DOFs, and `d²` as the signed face–tet incidence matrix.
//! - D. N. Arnold, R. S. Falk, R. Winther, *Finite element exterior
//!   calculus, homological techniques, and applications*, Acta Numerica
//!   15 (2006), 1–155, §1.2 ("The de Rham complex"): `d⁰`, `d¹`, and
//!   `d²` as the transposes of the simplicial boundary operators, with
//!   the sign convention given by the induced orientation of each
//!   oriented simplex's boundary; the bit-exact `dᵏ⁺¹ ∘ dᵏ ≡ 0`
//!   identities follow from `∂² = 0` on the corresponding chain
//!   complex.
//!
//! # Value type
//!
//! `d⁰`, `d¹`, and `d²` are mathematically *integer* matrices, but
//! `faer`'s sparse constructors require the value type to implement
//! `faer`'s `ComplexField`, which `i32` does not. We therefore store
//! the entries as `f64` (the exactly-representable integers `±1.0`).
//! This keeps the operators usable in `faer`'s sparse linear algebra
//! and lets the sparse products `d¹ · d⁰` and `d² · d¹` be formed
//! directly; for the small integers involved the products stay exact
//! in `f64` (bit-exact integer sums hold below `2^{53}`). It also
//! matches the value type of the rest of the sparse pipeline
//! ([`crate::sparse::SparseSystem`]).

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
/// the source-of-truth for the `d²` operator ([`divergence_map`]),
/// which scatters signed face DOFs into volume DOFs and whose
/// per-tet `(global_face_idx, sign)` table pins
/// `divergence_map(mesh) · curl_map(mesh) ≡ 0` bit-exactly.
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

/// Build the discrete divergence operator `d²` for `mesh`.
///
/// Returns an `n_tets × n_faces` sparse matrix in `faer`'s column-major
/// (CSC) format. Row `i` corresponds to tet `i` of [`TetMesh::tets`]
/// (and of [`TetMesh::tet_faces`]). Each row has exactly **four**
/// nonzeros, one per local face of the tet, with column index the
/// global face index from [`TetMesh::tet_faces`] and value
///
/// ```text
/// d²[tet_idx, global_face_idx] = (−1)^k · sign_k
/// ```
///
/// where `k ∈ {0, 1, 2, 3}` is the local-face slot (face `k` is
/// opposite local vertex `k`; see [`crate::mesh::TET_LOCAL_FACES`])
/// and `sign_k = mesh.tet_faces()[tet_idx][k].1 ∈ {+1, −1}` is the
/// permutation parity of the local-face vertex triple against the
/// global ascending order.
///
/// # Why the `(−1)^k` factor
///
/// The signed simplicial boundary of an oriented 3-simplex `[v0, v1,
/// v2, v3]` is
///
/// ```text
/// ∂[v0,v1,v2,v3] = [v1,v2,v3] − [v0,v2,v3] + [v0,v1,v3] − [v0,v1,v2],
/// ```
///
/// i.e. the face opposite local vertex `k` carries sign `(−1)^k`. The
/// faces in this expression are written in *increasing local-vertex*
/// order — which is exactly what [`crate::mesh::TET_LOCAL_FACES`]
/// emits. The further factor `sign_k` from [`TetMesh::tet_faces`]
/// converts the local-vertex-order face into the global ascending
/// face triple `(a, b, c)` with `a < b < c` used as the row of
/// [`curl_map`]; it is the parity of the permutation that sorts the
/// local-face vertex triple into ascending global order. Multiplying
/// the two gives the `d²` entry on the global ascending face
/// orientation.
///
/// Without the `(−1)^k`, two tets sharing an interior face would
/// contribute the *same* sign to that face's column (both signs
/// reflect the same permutation parity since both tets see the same
/// global face triple), and the `d²·d¹` cancellation argument would
/// fail row-by-row. With the `(−1)^k` factor included, the two
/// sharing tets get opposite contributions (their local-vertex
/// orientation around the shared face is reversed when one tet sits
/// on the "+ side" and the other on the "− side" of the face's
/// outward normal), and the second exactness identity `d² · d¹ ≡ 0`
/// holds bit-exactly.
///
/// The mesh-side scaffolding ([`TetMesh::tet_faces`]) deliberately
/// exposes only the global-orientation parity `sign_k` — the
/// alternating `(−1)^k` factor is operator-specific (it is part of
/// the de Rham `d²` convention, not of the face-DOF orientation
/// convention itself) and so lives here.
///
/// # Sign convention pins `d² · d¹ ≡ 0`
///
/// This sign convention pins `divergence_map(mesh) · curl_map(mesh)
/// ≡ 0` bit-exactly on any mesh — the second exactness identity of
/// the discrete de Rham complex (see the [module docs](crate::derham)
/// and Hiptmair §4, Arnold–Falk–Winther §1.2).
///
/// # Sparsity structure
///
/// - Every row has exactly four `±1.0` entries.
/// - Every interior face column has exactly two nonzeros, one per
///   sharing tet, with **opposite signs** (the two outward normals on
///   a shared face point in opposite directions, so the `(−1)^k ·
///   sign_k` contributions from the two tets disagree).
/// - Every boundary face column has exactly one nonzero (the unique
///   tet whose face is on `∂Ω`).
///
/// # Panics
///
/// Panics if `faer` rejects the constructed triplets (only possible on
/// an internal invariant violation — tet indices are bounded by
/// `n_tets`, face indices come from `tet_faces` which only emits
/// indices in `0..n_faces`, and the four (tet_idx, face_idx) pairs per
/// tet are distinct because the four local faces of a tet have
/// distinct vertex sets).
pub fn divergence_map(mesh: &TetMesh) -> SparseColMat<usize, f64> {
    let tet_faces = mesh.tet_faces();
    let n_tets = mesh.n_tets();
    let n_faces = mesh.faces().len();

    // Four triplets per tet: one per local face. Column is the global
    // face index from `TetMesh::tet_faces`; value is the local-face
    // alternating sign `(−1)^k` times the global-orientation parity
    // `sign_k` from `tet_faces`. See the function docstring for why
    // both factors are needed for `d² · d¹ ≡ 0`.
    let mut triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(4 * n_tets);
    for (tet_idx, faces_of_tet) in tet_faces.iter().enumerate() {
        for (local_face_k, &(global_face_idx, sign_k)) in faces_of_tet.iter().enumerate() {
            let alt_sign: f64 = if local_face_k % 2 == 0 { 1.0 } else { -1.0 };
            let entry = alt_sign * (sign_k as f64);
            triplets.push(Triplet::new(tet_idx, global_face_idx as usize, entry));
        }
    }

    SparseColMat::<usize, f64>::try_new_from_triplets(n_tets, n_faces, &triplets)
        .expect("d² triplets are well-formed: in-range indices, distinct per tet")
}

/// Apply `d²` to a face-DOF field without materializing the sparse
/// matrix.
///
/// Returns `d² · q`, the vector of volume DOFs of the discrete
/// divergence: for tet `T_i`,
///
/// ```text
/// out[i] = Σ_{k=0..4}  (−1)^k · sign_k · q[global_face_idx_k]
/// ```
///
/// where the four `(global_face_idx_k, sign_k)` pairs are read from
/// `mesh.tet_faces()[i]`. The output length is `mesh.n_tets()` and
/// entry `i` matches row `i` of [`divergence_map`]. See
/// [`divergence_map`] for the full sign-convention rationale.
///
/// Prefer this over `divergence_map(mesh)` followed by a sparse mat-vec
/// when you only need the divergence values and not the operator
/// itself.
///
/// # Panics
///
/// Panics if `face_field.len() != mesh.faces().len()`.
pub fn apply_divergence(mesh: &TetMesh, face_field: &[f64]) -> Vec<f64> {
    let n_faces = mesh.faces().len();
    assert_eq!(
        face_field.len(),
        n_faces,
        "face field length {} disagrees with mesh face count {}",
        face_field.len(),
        n_faces
    );

    mesh.tet_faces()
        .iter()
        .map(|faces_of_tet| {
            faces_of_tet
                .iter()
                .enumerate()
                .map(|(local_face_k, &(global_face_idx, sign_k))| {
                    let alt_sign: f64 = if local_face_k % 2 == 0 { 1.0 } else { -1.0 };
                    alt_sign * (sign_k as f64) * face_field[global_face_idx as usize]
                })
                .sum()
        })
        .collect()
}
