"""NumPy reference for the discrete de Rham complex (d⁰, d¹, d²).

Issue #149 (Epic #88, Phase I bridge): mirrors the Burn-side operators
in ``crates/geode-core/src/derham.rs`` so the Rust ``geode_core::derham``
public API can be cross-checked at the integer-matrix level. Because the
operators are mathematically signed ``{-1, 0, +1}`` matrices, the
cross-check is *bit-exact* — there is no floating-point tolerance
question, only "do the integer patterns and signs match between
backends."

The discrete de Rham complex on a tetrahedral mesh::

    ℝ^{n_nodes}  --d⁰-->  ℝ^{n_edges}  --d¹-->  ℝ^{n_faces}  --d²-->  ℝ^{n_tets}
        H¹                H(curl)              H(div)             L²

with the bit-exact exactness identities ``d¹ · d⁰ ≡ 0`` and
``d² · d¹ ≡ 0`` (Hiptmair, *Acta Numerica* 2002 §4; Arnold–Falk–Winther,
*Acta Numerica* 2006 §1.2).

Sign conventions
================

These mirror ``crates/geode-core/src/derham.rs`` exactly. Any drift
between Burn and NumPy is a bug; the integer cross-check in
``crates/geode-validation/tests/derham_numpy_reference.rs`` is the
canary.

**Edge orientation** (lower-tag-first, ``a < b``)::

    d⁰[edge, a] = -1     (tail of the oriented edge)
    d⁰[edge, b] = +1     (head of the oriented edge)

**Face orientation** (ascending global triple ``a < b < c``, cycle
``a → b → c → a``)::

    d¹[face, edge(a, b)] = +1
    d¹[face, edge(b, c)] = +1
    d¹[face, edge(a, c)] = -1

**Tet face orientation** (per local face slot ``k`` of the tet, see
``TET_LOCAL_FACES`` in ``crates/geode-core/src/mesh/mod.rs``)::

    d²[tet, global_face_k] = (-1)^k · sign_k

where ``sign_k`` is the permutation parity of the local-face vertex
triple against the global ascending order. The ``(-1)^k`` alternation
is the simplicial boundary convention
``∂[v0, v1, v2, v3] = [v1,v2,v3] - [v0,v2,v3] + [v0,v1,v3] - [v0,v1,v2]``;
without it the ``d² · d¹ ≡ 0`` identity fails because two tets sharing
an interior face would contribute the same sign instead of opposite
signs.

References
==========

- ``crates/geode-core/src/derham.rs`` — Rust source of truth for ``d⁰``,
  ``d¹``, ``d²`` and their docstrings.
- ``crates/geode-core/src/mesh/mod.rs`` — ``TET_LOCAL_EDGES`` (line ~190),
  ``TET_LOCAL_FACES`` (line ~201), ``TET_LOCAL_FACE_EDGES`` (line ~247).
- Hiptmair, *Finite elements in computational electromagnetism*, Acta
  Numerica 11 (2002), §3 and §4.
- Arnold, Falk, Winther, *Finite element exterior calculus, homological
  techniques, and applications*, Acta Numerica 15 (2006), §1.2.

Public API
==========

- :func:`build_edges(tets)` — global edge table (``[n_edges, 2]``,
  lower-tag-first).
- :func:`build_faces(tets)` — global face table (``[n_faces, 3]``,
  ascending).
- :func:`build_tet_faces(tets, faces)` — per-tet
  ``(global_face_idx, sign_k)`` table.
- :func:`gradient_map(n_nodes, edges)` — ``d⁰`` as
  ``scipy.sparse.csr_matrix`` with integer ``{-1, +1}`` entries.
- :func:`curl_map(edges, faces)` — ``d¹`` as
  ``scipy.sparse.csr_matrix`` with integer ``{-1, +1}`` entries.
- :func:`divergence_map(tets, faces)` — ``d²`` as
  ``scipy.sparse.csr_matrix`` with integer ``{-1, +1}`` entries.
- :func:`euler_ranks(n_nodes, n_edges, n_faces, n_tets, n_boundary_faces)`
  — rank predictions for ``d⁰``, ``d¹``, ``d²`` from Euler-characteristic
  arithmetic (closed-mesh ranks; the cross-check pins these against
  measured CSR ranks for the bundled sphere fixture).
- :func:`spurious_dim_from_derham(nodes, edges, edge_mask, r_outer)` —
  interior-restricted ``rank(d⁰)`` classifier; the Phase G ``sphere_pec.py``
  reference imports this directly instead of inlining the dense-restrict
  computation.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import scipy.sparse

# Canonical local-edge / local-face / local-face-edge tables — mirror
# of the constants in ``crates/geode-core/src/mesh/mod.rs``. These are
# the single source of truth for the per-tet ordering on the Rust side;
# any drift here is a silent bug, so the tables are duplicated verbatim
# rather than re-derived.

TET_LOCAL_EDGES: list[tuple[int, int]] = [
    (0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3),
]
"""Canonical local edge → (local vertex pair) ordering on a tet.

Mirror of ``crates/geode-core/src/mesh/mod.rs::TET_LOCAL_EDGES`` (line
~190). Six local edges in fixed order: ``(0,1), (0,2), (0,3), (1,2),
(1,3), (2,3)``. Used by both :func:`build_edges` and any caller that
needs to scatter per-tet edge data into a global table.
"""

TET_LOCAL_FACES: list[tuple[int, int, int]] = [
    # Face k is opposite local vertex k; vertices listed in ascending
    # local order to pin the cyclic boundary traversal at a → b → c → a.
    (1, 2, 3),   # face 0 opposite vertex 0
    (0, 2, 3),   # face 1 opposite vertex 1
    (0, 1, 3),   # face 2 opposite vertex 2
    (0, 1, 2),   # face 3 opposite vertex 3
]
"""Canonical local face → (local vertex triple) ordering on a tet.

Mirror of ``crates/geode-core/src/mesh/mod.rs::TET_LOCAL_FACES`` (line
~201). Face ``k`` is opposite local vertex ``k``; the ascending-local
listing pins ``d¹ ∘ d⁰ ≡ 0`` bit-exactly (see Rust docstring).
"""


# --------------------------------------------------------------------------- #
# Mesh-level helpers — edges, faces, per-tet face/sign tables.
# --------------------------------------------------------------------------- #


def build_edges(tets) -> np.ndarray:
    """Build the deduplicated, globally-oriented edge table.

    Mirror of ``geode_core::TetMesh::edges`` (lower-tag-first, sorted
    lexicographically by ``(a, b)``). Edge ``i`` in the returned array
    is row ``i`` of the discrete gradient ``d⁰``.

    Parameters
    ----------
    tets : (n_tets, 4) int

    Returns
    -------
    edges : (n_edges, 2) int64
        Each row ``[a, b]`` satisfies ``a < b``. Sorted ascending by
        ``(a, b)`` lexicographically — matches the Rust ``BTreeSet``
        deduplication order in ``TetMesh::edges``.
    """
    tets = np.asarray(tets, dtype=np.int64)
    la_arr = np.asarray([la for la, _ in TET_LOCAL_EDGES], dtype=np.int64)
    lb_arr = np.asarray([lb for _, lb in TET_LOCAL_EDGES], dtype=np.int64)
    vert_a = tets[:, la_arr]  # (n_tets, 6)
    vert_b = tets[:, lb_arr]
    lo = np.minimum(vert_a, vert_b)
    hi = np.maximum(vert_a, vert_b)
    pair_flat = np.stack([lo.ravel(), hi.ravel()], axis=1)
    edges = np.unique(pair_flat, axis=0).astype(np.int64)
    return edges


def build_faces(tets) -> np.ndarray:
    """Build the deduplicated, globally-oriented face table.

    Mirror of ``geode_core::TetMesh::faces``: each face is a triple
    ``[a, b, c]`` with ``a < b < c``, sorted ascending lexicographically.
    Face ``i`` in the returned array is row ``i`` of the discrete curl
    ``d¹``.

    Parameters
    ----------
    tets : (n_tets, 4) int

    Returns
    -------
    faces : (n_faces, 3) int64
        Each row ``[a, b, c]`` satisfies ``a < b < c``. Sorted by
        ``(a, b, c)`` lex order, matching the Rust ``BTreeSet`` order.
    """
    tets = np.asarray(tets, dtype=np.int64)
    n_tets = tets.shape[0]
    triples = np.empty((n_tets * 4, 3), dtype=np.int64)
    for slot, lf in enumerate(TET_LOCAL_FACES):
        tri = tets[:, list(lf)]  # (n_tets, 3)
        tri_sorted = np.sort(tri, axis=1)
        triples[slot * n_tets:(slot + 1) * n_tets, :] = tri_sorted
    faces = np.unique(triples, axis=0).astype(np.int64)
    return faces


def _triple_permutation_sign(local: tuple[int, int, int]) -> int:
    """Sign of the permutation that sorts ``local`` into ascending order.

    +1 for an even permutation (identity or one of the two 3-cycles),
    -1 for an odd permutation (any transposition). Mirror of the
    private ``triple_permutation_sign`` helper in
    ``crates/geode-core/src/mesh/mod.rs``.
    """
    a, b, c = local
    # 3-element bubble sort, count swaps.
    swaps = 0
    arr = [a, b, c]
    if arr[0] > arr[1]:
        arr[0], arr[1] = arr[1], arr[0]
        swaps += 1
    if arr[1] > arr[2]:
        arr[1], arr[2] = arr[2], arr[1]
        swaps += 1
    if arr[0] > arr[1]:
        arr[0], arr[1] = arr[1], arr[0]
        swaps += 1
    return 1 if swaps % 2 == 0 else -1


def build_tet_faces(tets, faces) -> tuple[np.ndarray, np.ndarray]:
    """Per-tet ``(global_face_idx, sign_k)`` table.

    Mirror of ``geode_core::TetMesh::tet_faces`` — for each tet and each
    local-face slot ``k`` (in :data:`TET_LOCAL_FACES` order), return the
    global face index and the parity ``sign_k = ±1`` of the permutation
    that sorts the local-face vertex triple into ascending global order.

    Parameters
    ----------
    tets : (n_tets, 4) int
    faces : (n_faces, 3) int64 — output of :func:`build_faces`

    Returns
    -------
    tet_face_idx : (n_tets, 4) int64
        Per-tet, per-local-face slot global face index.
    tet_face_sign : (n_tets, 4) int8
        Per-tet, per-local-face slot permutation parity in ``{-1, +1}``.
    """
    tets = np.asarray(tets, dtype=np.int64)
    faces = np.asarray(faces, dtype=np.int64)
    n_tets = tets.shape[0]

    face_to_idx: dict[tuple[int, int, int], int] = {
        (int(faces[i, 0]), int(faces[i, 1]), int(faces[i, 2])): i
        for i in range(faces.shape[0])
    }

    tet_face_idx = np.empty((n_tets, 4), dtype=np.int64)
    tet_face_sign = np.empty((n_tets, 4), dtype=np.int8)
    for e in range(n_tets):
        for k, lf in enumerate(TET_LOCAL_FACES):
            local = (int(tets[e, lf[0]]), int(tets[e, lf[1]]), int(tets[e, lf[2]]))
            sorted_local = tuple(sorted(local))
            tet_face_idx[e, k] = face_to_idx[sorted_local]
            tet_face_sign[e, k] = _triple_permutation_sign(local)
    return tet_face_idx, tet_face_sign


# --------------------------------------------------------------------------- #
# d⁰, d¹, d² — signed integer sparse incidence matrices.
# --------------------------------------------------------------------------- #


def gradient_map(n_nodes: int, edges) -> scipy.sparse.csr_matrix:
    """Build the discrete gradient operator ``d⁰`` as a CSR matrix.

    Mirror of ``geode_core::derham::gradient_map``. Returns an
    ``n_edges × n_nodes`` sparse matrix with integer ``{-1, +1}``
    entries, stored as ``np.int64`` in the underlying ``data`` array.

    Each row corresponds to an edge ``[a, b]`` with ``a < b``, and has
    exactly two nonzeros: ``-1`` at column ``a`` (tail) and ``+1`` at
    column ``b`` (head). Applied to a nodal field ``φ``, row ``[a, b]``
    yields ``φ[b] - φ[a]``.

    Parameters
    ----------
    n_nodes : int — number of mesh nodes (column count).
    edges : (n_edges, 2) int — output of :func:`build_edges`.

    Returns
    -------
    d0 : scipy.sparse.csr_matrix ``(n_edges, n_nodes)``, dtype=int64
        Row-sorted CSR (``edges`` is sorted by ``(a, b)`` ascending, and
        within each row the two column indices appear in ascending order
        because ``a < b``).
    """
    edges = np.asarray(edges, dtype=np.int64)
    n_edges = int(edges.shape[0])
    # Two entries per edge: (-1) at column a, (+1) at column b.
    # We need them in column-ascending order per row to match
    # ``scipy.sparse.csr_matrix``'s canonical sorted-indices form, which
    # is what the Rust comparator will also canonicalize against.
    rows = np.repeat(np.arange(n_edges, dtype=np.int64), 2)
    cols = edges.reshape(-1)  # [a0, b0, a1, b1, ...] — a < b by construction.
    data = np.empty(2 * n_edges, dtype=np.int64)
    data[0::2] = -1
    data[1::2] = +1
    d0 = scipy.sparse.csr_matrix(
        (data, cols, np.arange(0, 2 * n_edges + 1, 2, dtype=np.int64)),
        shape=(n_edges, n_nodes),
    )
    d0.sort_indices()
    return d0


def curl_map(edges, faces) -> scipy.sparse.csr_matrix:
    """Build the discrete curl operator ``d¹`` as a CSR matrix.

    Mirror of ``geode_core::derham::curl_map``. Returns an
    ``n_faces × n_edges`` sparse matrix with integer ``{-1, +1}``
    entries.

    Each row corresponds to a face ``[a, b, c]`` with ``a < b < c``, and
    has exactly three nonzeros encoding the signed boundary cycle
    ``a → b → c → a``::

        d¹[face, edge(a, b)] = +1
        d¹[face, edge(b, c)] = +1
        d¹[face, edge(a, c)] = -1

    Parameters
    ----------
    edges : (n_edges, 2) int — output of :func:`build_edges`.
    faces : (n_faces, 3) int — output of :func:`build_faces`.

    Returns
    -------
    d1 : scipy.sparse.csr_matrix ``(n_faces, n_edges)``, dtype=int64
        Row-sorted CSR after :meth:`sort_indices`.
    """
    edges = np.asarray(edges, dtype=np.int64)
    faces = np.asarray(faces, dtype=np.int64)
    n_edges = int(edges.shape[0])
    n_faces = int(faces.shape[0])

    edge_to_idx: dict[tuple[int, int], int] = {
        (int(edges[i, 0]), int(edges[i, 1])): i for i in range(n_edges)
    }

    # Three triplets per face.
    rows = np.repeat(np.arange(n_faces, dtype=np.int64), 3)
    cols = np.empty(3 * n_faces, dtype=np.int64)
    data = np.empty(3 * n_faces, dtype=np.int64)
    for f in range(n_faces):
        a, b, c = int(faces[f, 0]), int(faces[f, 1]), int(faces[f, 2])
        # Per Rust docstring: a < b < c is asserted on input.
        cols[3 * f + 0] = edge_to_idx[(a, b)]
        cols[3 * f + 1] = edge_to_idx[(b, c)]
        cols[3 * f + 2] = edge_to_idx[(a, c)]
        data[3 * f + 0] = +1
        data[3 * f + 1] = +1
        data[3 * f + 2] = -1
    d1 = scipy.sparse.coo_matrix(
        (data, (rows, cols)), shape=(n_faces, n_edges)
    ).tocsr()
    d1.sort_indices()
    return d1


def divergence_map(tets, faces) -> scipy.sparse.csr_matrix:
    """Build the discrete divergence operator ``d²`` as a CSR matrix.

    Mirror of ``geode_core::derham::divergence_map``. Returns an
    ``n_tets × n_faces`` sparse matrix with integer ``{-1, +1}``
    entries. Each row has exactly four nonzeros, one per local face of
    the tet.

    Per the Rust docstring::

        d²[tet, global_face_k] = (-1)^k · sign_k

    where ``k ∈ {0, 1, 2, 3}`` is the local-face slot (face ``k`` is
    opposite local vertex ``k``; see :data:`TET_LOCAL_FACES`) and
    ``sign_k = ±1`` is the permutation parity from
    :func:`build_tet_faces`. The ``(-1)^k`` alternation is the
    simplicial boundary convention; without it the ``d² · d¹ ≡ 0``
    identity fails.

    Parameters
    ----------
    tets : (n_tets, 4) int
    faces : (n_faces, 3) int — output of :func:`build_faces`

    Returns
    -------
    d2 : scipy.sparse.csr_matrix ``(n_tets, n_faces)``, dtype=int64
        Row-sorted CSR after :meth:`sort_indices`.
    """
    tets = np.asarray(tets, dtype=np.int64)
    faces = np.asarray(faces, dtype=np.int64)
    n_tets = int(tets.shape[0])
    n_faces = int(faces.shape[0])

    tet_face_idx, tet_face_sign = build_tet_faces(tets, faces)

    # Four triplets per tet.
    rows = np.repeat(np.arange(n_tets, dtype=np.int64), 4)
    cols = tet_face_idx.reshape(-1).astype(np.int64)
    alt = np.array([1, -1, 1, -1], dtype=np.int64)  # (-1)^k for k=0..3
    data = (tet_face_sign.astype(np.int64) * alt[None, :]).reshape(-1)
    d2 = scipy.sparse.coo_matrix(
        (data, (rows, cols)), shape=(n_tets, n_faces)
    ).tocsr()
    d2.sort_indices()
    return d2


# --------------------------------------------------------------------------- #
# Rank predictions from Euler-characteristic arithmetic.
# --------------------------------------------------------------------------- #


def euler_ranks(
    n_nodes: int,
    n_edges: int,
    n_faces: int,
    n_tets: int,
) -> dict[str, int]:
    """Predict ``rank(d⁰)``, ``rank(d¹)``, ``rank(d²)`` on a closed
    contractible mesh (the bundled sphere fixture is a ball, contractible).

    On a contractible domain the de Rham complex is exact, so the
    Betti numbers vanish:

        β_0 = 1   (one connected component)
        β_1 = 0   (no 1-cycles that aren't boundaries)
        β_2 = 0   (no 2-cycles that aren't boundaries)

    The Euler characteristic identity ``χ = n_nodes - n_edges + n_faces
    - n_tets = β_0 - β_1 + β_2 - β_3 = 1`` (on a ball / contractible 3-
    domain) ties the four cell counts together. From the rank–nullity
    theorem applied to each operator,

        rank(d⁰) = n_nodes - dim ker(d⁰) = n_nodes - β_0
        rank(d¹) = (n_edges - rank(d⁰)) - β_1 = n_edges - n_nodes + 1
        rank(d²) = (n_faces - rank(d¹)) - β_2 = n_faces - n_edges + n_nodes - 1

    On the bundled sphere fixture (a 3-ball), the predictions are exact
    integers; the cross-check in the Rust harness pins them against the
    measured ranks of the loaded CSR matrices.

    Parameters
    ----------
    n_nodes, n_edges, n_faces, n_tets : int
        Cell counts. On a closed contractible 3-mesh these satisfy
        ``n_nodes - n_edges + n_faces - n_tets = 1``.

    Returns
    -------
    dict with keys ``rank_d0``, ``rank_d1``, ``rank_d2``, ``euler_chi``.
    """
    rank_d0 = n_nodes - 1
    rank_d1 = n_edges - n_nodes + 1
    rank_d2 = n_faces - n_edges + n_nodes - 1
    euler_chi = n_nodes - n_edges + n_faces - n_tets
    return {
        "rank_d0": int(rank_d0),
        "rank_d1": int(rank_d1),
        "rank_d2": int(rank_d2),
        "euler_chi": int(euler_chi),
    }


# --------------------------------------------------------------------------- #
# Phase G bridge — the interior-restricted d⁰ rank classifier consumed by
# ``sphere_pec.py::spurious_dim_from_derham``. Lives here (not in
# ``sphere_pec.py``) per the Issue #149 consolidation deliverable so the
# reference de Rham surface is the single source of truth.
# --------------------------------------------------------------------------- #


DERHAM_RANK_THRESHOLD_REL: float = 1e-12
"""Relative threshold for "near-zero singular value" in the d⁰ rank
computation. Mirror of ``geode_core::DERHAM_RANK_THRESHOLD_REL``.
Re-exported here so ``sphere_pec.py`` can import a single constant from
this module."""


def restrict_gradient_dense(edges, edge_mask, node_mask) -> np.ndarray:
    """Build the dense interior×interior restriction of d⁰.

    Mirror of ``geode_core::restrict_gradient_dense``: each edge
    contributes exactly two ±1.0 entries (lower-tag endpoint = -1,
    higher-tag = +1), filtered to
    ``edge_mask[i] & node_mask[a] & node_mask[b]``.

    This is a *dense* restriction (not the full sparse ``d⁰``) because
    the downstream consumer ``spurious_dim_from_derham`` runs a dense
    SVD on it via ``numpy.linalg.matrix_rank``.

    Parameters
    ----------
    edges : (n_edges, 2) int — output of :func:`build_edges`
    edge_mask : (n_edges,) bool — True on interior edges
    node_mask : (n_nodes,) bool — True on interior nodes

    Returns
    -------
    d0 : (n_interior_edges, n_interior_nodes) float64
        Dense interior-restricted discrete gradient operator.
    """
    edges = np.asarray(edges, dtype=np.int64)
    edge_mask = np.asarray(edge_mask, dtype=bool)
    node_mask = np.asarray(node_mask, dtype=bool)

    n_interior_nodes = int(np.sum(node_mask))
    n_interior_edges = int(np.sum(edge_mask))

    node_to_interior = -np.ones(node_mask.shape[0], dtype=np.int64)
    node_to_interior[node_mask] = np.arange(n_interior_nodes, dtype=np.int64)

    edge_to_interior = -np.ones(edge_mask.shape[0], dtype=np.int64)
    edge_to_interior[edge_mask] = np.arange(n_interior_edges, dtype=np.int64)

    d0 = np.zeros((n_interior_edges, n_interior_nodes), dtype=np.float64)
    for edge_idx, (a, b) in enumerate(edges):
        row = edge_to_interior[edge_idx]
        if row < 0:
            continue
        col_a = node_to_interior[a]
        col_b = node_to_interior[b]
        if col_a >= 0:
            d0[row, col_a] = -1.0
        if col_b >= 0:
            d0[row, col_b] = 1.0
    return d0


def spurious_dim_from_derham(
    nodes,
    edges,
    edge_mask,
    r_outer: float,
) -> int:
    """Algebraic spurious-mode dimension = ``rank(d⁰_interior)``.

    Mirror of ``geode_core::spurious_dim_from_derham``. The Nédélec
    curl-curl kernel is the image of the discrete gradient
    (``kernel(K) = image(d⁰)`` per Epic #57 Phase 3.A), so its rank is
    the spurious-mode count.

    Returns the rank computed via dense SVD with relative cutoff
    :data:`DERHAM_RANK_THRESHOLD_REL` × ``σ_max``, matching the Rust
    side bit-exactly through the LAPACK driver chosen by
    ``numpy.linalg.matrix_rank``.

    This function is consumed by ``sphere_pec.py`` (Phase G); the
    consolidation from an inline copy to this formal reference module
    is the Issue #149 Deliverable 4 refactor.

    Parameters
    ----------
    nodes : (n_nodes, 3) float64
    edges : (n_edges, 2) int64 — output of :func:`build_edges`.
    edge_mask : (n_edges,) bool — True on interior edges.
    r_outer : float — outer PEC wall radius.

    Returns
    -------
    n_spurious : int — ``rank(d⁰_interior)``.
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tol = 1e-6 * max(r_outer, 1.0)
    r = np.linalg.norm(nodes, axis=1)
    node_mask = np.abs(r - r_outer) >= tol
    d0 = restrict_gradient_dense(edges, edge_mask, node_mask)
    return int(
        np.linalg.matrix_rank(
            d0, tol=DERHAM_RANK_THRESHOLD_REL * np.linalg.norm(d0, ord=2)
        )
    )


# --------------------------------------------------------------------------- #
# Smoke / CLI entrypoint.
# --------------------------------------------------------------------------- #


def _read_msh_tets(mesh_path: Path) -> tuple[np.ndarray, np.ndarray]:
    """Load a Gmsh `.msh` fixture via ``meshio`` and return (nodes, tets).

    Concatenates every ``tetra`` cell block in mesh-file order, matching
    ``geode_core::GmshReader``'s block-order convention.
    """
    import meshio

    m = meshio.read(mesh_path)
    nodes = np.asarray(m.points, dtype=np.float64)
    tet_blocks: list[np.ndarray] = []
    for cells in m.cells:
        if cells.type == "tetra":
            tet_blocks.append(np.asarray(cells.data, dtype=np.int64))
    if not tet_blocks:
        raise ValueError(f"no tet cells in {mesh_path}")
    tets = np.concatenate(tet_blocks, axis=0)
    return nodes, tets


def _csr_summary(name: str, m: scipy.sparse.csr_matrix) -> dict:
    return {
        "name": name,
        "shape": list(m.shape),
        "nnz": int(m.nnz),
        "min_value": int(m.data.min()) if m.nnz else 0,
        "max_value": int(m.data.max()) if m.nnz else 0,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="NumPy reference for the discrete de Rham complex (d⁰, d¹, d²)."
    )
    parser.add_argument(
        "--mesh",
        type=Path,
        required=True,
        help="Path to a Gmsh `.msh` tet mesh (e.g. "
        "reference/fixtures/sphere_pec/sphere.msh).",
    )
    parser.add_argument(
        "--emit",
        type=Path,
        default=None,
        help="Optional path to write a JSON smoke summary (shapes + nnz + "
        "compositional-identity residuals).",
    )
    args = parser.parse_args(argv)

    nodes, tets = _read_msh_tets(args.mesh)
    n_nodes = int(nodes.shape[0])
    n_tets = int(tets.shape[0])

    edges = build_edges(tets)
    faces = build_faces(tets)
    n_edges = int(edges.shape[0])
    n_faces = int(faces.shape[0])

    d0 = gradient_map(n_nodes, edges)
    d1 = curl_map(edges, faces)
    d2 = divergence_map(tets, faces)

    # Algebraic exactness checks.
    d1_d0 = (d1 @ d0).tocsr()
    d1_d0.eliminate_zeros()
    d2_d1 = (d2 @ d1).tocsr()
    d2_d1.eliminate_zeros()

    print(f"mesh: {args.mesh}")
    print(f"  n_nodes={n_nodes}, n_edges={n_edges}, n_faces={n_faces}, n_tets={n_tets}")
    print(f"  d0: shape={d0.shape}, nnz={d0.nnz}")
    print(f"  d1: shape={d1.shape}, nnz={d1.nnz}")
    print(f"  d2: shape={d2.shape}, nnz={d2.nnz}")
    print(f"  d1 @ d0: shape={d1_d0.shape}, nnz={d1_d0.nnz} (should be 0)")
    print(f"  d2 @ d1: shape={d2_d1.shape}, nnz={d2_d1.nnz} (should be 0)")
    ranks = euler_ranks(n_nodes, n_edges, n_faces, n_tets)
    print(
        f"  Euler χ = {ranks['euler_chi']} (expected 1 for a ball); "
        f"rank predictions: d0={ranks['rank_d0']}, d1={ranks['rank_d1']}, "
        f"d2={ranks['rank_d2']}"
    )

    if args.emit is not None:
        summary = {
            "mesh": str(args.mesh),
            "n_nodes": n_nodes,
            "n_edges": n_edges,
            "n_faces": n_faces,
            "n_tets": n_tets,
            "d0": _csr_summary("d0", d0),
            "d1": _csr_summary("d1", d1),
            "d2": _csr_summary("d2", d2),
            "d1_d0_nnz": int(d1_d0.nnz),
            "d2_d1_nnz": int(d2_d1.nnz),
            "ranks": ranks,
        }
        args.emit.parent.mkdir(parents=True, exist_ok=True)
        with args.emit.open("w") as fh:
            json.dump(summary, fh, indent=2)
            fh.write("\n")
        print(f"wrote smoke summary to {args.emit}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
