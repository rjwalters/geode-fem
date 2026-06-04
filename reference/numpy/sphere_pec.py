"""NumPy reference for the vector-Nédélec sphere-PEC eigenmode pipeline.

Issue #118 (Epic #88, Phase G.2): mirrors the full Burn pipeline in
NumPy so the two backends can be cross-checked sub-stage by sub-stage.

The pipeline
============

1. Mesh I/O — load the Gmsh `.msh` fixture via ``meshio``, including the
   ``gmsh:physical`` cell-data tags. The same `.msh` is the canonical
   fixture used by ``crates/geode-core/tests/sphere_pec_eigenmode.rs``;
   it is copied verbatim into ``reference/fixtures/sphere_pec/sphere.msh``
   so the NumPy reference is runnable from a fresh checkout without
   `cargo`. See :func:`read_sphere_fixture`.
2. ε_r assignment — per-tet relative permittivity: ``n²`` inside the
   dielectric sphere (``PHYS_SPHERE_INTERIOR``), 1 in the vacuum buffer
   (``PHYS_VACUUM_GAP`` and ``PHYS_PML_SHELL`` — this phase is PEC
   *without* PML). See :func:`build_epsilon_r`.
3. Edge enumeration + sign convention — build the globally-oriented
   ``edges: [n_edges, 2]`` (lower-tag-first, ``a < b``) and per-tet
   ``tet_edges: [n_tets, 6, 2]`` of ``(global_edge_idx, sign)`` pairs
   matching the canonical ``TET_LOCAL_EDGES`` ordering. ``sign = +1`` if
   the local edge direction agrees with the global lower-tag-first
   orientation, ``-1`` otherwise. See :func:`build_edges`.
4. PEC mask — edges with *both* endpoints on the outer wall
   ``r = R_BUFFER`` are removed before the eigensolve. See
   :func:`sphere_pec_interior_edges`.
5. Global assembly — scatter the per-element ``[6, 6]`` Nédélec curl-
   curl and ε-scaled mass into ``[n_edges, n_edges]`` CSR via
   ``scipy.sparse.coo_matrix(...).tocsr()``. The ``s_i s_j`` sign flips
   are applied to the local blocks before the scatter (faithful port of
   ``crates/geode-core/src/nedelec_assembly.rs``). See
   :func:`assemble_global_nedelec`.
6. Dirichlet reduction — restrict ``K, M`` to the interior edges by
   row+column extraction. See :func:`apply_dirichlet`.
7. Eigensolve — ``scipy.sparse.linalg.eigsh(K_int, k=n_request,
   M=M_int, sigma=0.0, which='LM')`` recovers
   ``n_request = spurious_dim + 8`` lowest eigenvalues. The shift-and-
   invert is essential because the spurious nullspace is large and
   numerically scattered around zero — ARPACK's default no-shift mode
   would converge slowly and unreliably onto the cluster center of
   mass rather than the cluster's outer rim.
8. Spurious-mode filter — same largest-relative-gap heuristic as the
   Burn test (``sphere_pec_eigenmode.rs:194-215``) verbatim. The
   observed spurious count must equal the predicted gradient-kernel
   dimension (number of interior vertices not on the outer wall) — that
   integer match is a bit-exact cross-check on edge orientation +
   boundary masking.

Sign convention recap
=====================

For an edge connecting global nodes ``(a, b)``, the canonical direction
is from the lower-index endpoint to the higher-index endpoint:

    oriented_edge(a, b) = (min(a, b), max(a, b))

Per tet, edges are listed in the fixed canonical local order (matches
``geode_core::mesh::TET_LOCAL_EDGES``)::

    local edge 0: (v_local_0, v_local_1)
    local edge 1: (v_local_0, v_local_2)
    local edge 2: (v_local_0, v_local_3)
    local edge 3: (v_local_1, v_local_2)
    local edge 4: (v_local_1, v_local_3)
    local edge 5: (v_local_2, v_local_3)

Each tet's local edge ``i`` carries a sign ``s_i in {+1, -1}`` that
records whether the local edge direction agrees with the global
lower-tag-first orientation. The local 6x6 stiffness/mass rows and
columns are flipped by ``s_i s_j`` before scatter into the global
system.

Public API
==========

- :func:`read_sphere_fixture(path)` — load the `.msh` via ``meshio``,
  return ``SphereFixture(nodes, tets, tet_physical_tags)``.
- :func:`build_epsilon_r(tags, n)` — per-tet ``ε_r`` vector.
- :func:`build_edges(tets)` — global edge table + per-tet edge-sign table.
- :func:`sphere_pec_interior_edges(nodes, edges, r_outer)` — interior
  edge mask (PEC removal).
- :func:`sphere_n_interior_nodes(nodes, r_outer)` — predicted spurious
  count.
- :func:`assemble_global_nedelec(nodes, tets, edges, tet_edges, epsilon_r)`
  — assembled ``(K, M)`` as scipy CSR matrices.
- :func:`apply_dirichlet(K, M, mask)` — restrict K, M to interior DOFs.
- :func:`eigensolve(K, M, k)` — lowest-k generalized eigenpairs via
  shift-and-invert eigsh.
- :func:`filter_spurious(lambdas, spurious_dim)` — implement the
  largest-relative-gap heuristic; return ``(n_spurious, physical)``.
- :func:`run_sphere_pec(mesh_path, n_index=1.5, n_take=5)` — orchestrator
  returning a dict with all intermediate quantities for cross-backend
  comparison.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import scipy.sparse
import scipy.sparse.linalg

# Allow `python3 sphere_pec.py` to find sibling modules regardless of cwd.
HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))
from nedelec_local_matrices import (  # noqa: E402
    TET_LOCAL_EDGES,
    batched_nedelec_local_matrices,
)


# --------------------------------------------------------------------------- #
# Sphere fixture geometry — mirror of ``geode_core::mesh::sphere`` constants.
# --------------------------------------------------------------------------- #

R_SPHERE: float = 1.0
"""Inner dielectric sphere radius used by the bundled fixture."""

R_PML_INNER: float = 1.5
"""PML inner interface radius. Not used in this phase (PEC, no PML)."""

R_BUFFER: float = 2.0
"""Outer vacuum buffer radius == PEC wall location."""

# Physical-group tags — mirror of ``geode_core::mesh::sphere::PHYS_*``.
# Cross-reference: `mesh_scripts/sphere.geo` (Burn-side fixture source);
# `crates/geode-core/src/mesh/sphere.rs:73-104` (Rust-side constants).
PHYS_SPHERE_INTERIOR: int = 1
"""3D tag: tets in ``r <= R_SPHERE``."""

PHYS_VACUUM_GAP: int = 2
"""3D tag: tets in ``R_SPHERE < r <= R_PML_INNER``."""

PHYS_PML_SHELL: int = 5
"""3D tag: tets in ``R_PML_INNER < r <= R_BUFFER``."""

PHYS_OUTER_BOUNDARY: int = 3
"""2D tag: surface triangles on ``r = R_BUFFER``."""

PHYS_SPHERE_SURFACE: int = 4
"""2D tag: surface triangles on ``r = R_SPHERE``."""

PHYS_PML_INTERFACE: int = 6
"""2D tag: surface triangles on ``r = R_PML_INNER``."""


# --------------------------------------------------------------------------- #
# Mesh I/O — meshio-backed Gmsh `.msh` reader with physical-tag retention.
# --------------------------------------------------------------------------- #


@dataclass
class SphereFixture:
    """Loaded sphere mesh + per-tet 3D physical tags.

    Mirror of ``geode_core::mesh::sphere::SphereFixture`` (only the
    fields downstream consumers actually need; surface-triangle data is
    omitted because the PEC mask works off node positions, not the
    `outer_boundary` triangle group).
    """

    nodes: np.ndarray
    """``(n_nodes, 3)`` float64 node coordinates."""

    tets: np.ndarray
    """``(n_tets, 4)`` int64 tet connectivity (0-based)."""

    tet_physical_tags: np.ndarray
    """``(n_tets,)`` int32 per-tet 3D physical-group tag."""

    @property
    def n_nodes(self) -> int:
        return int(self.nodes.shape[0])

    @property
    def n_tets(self) -> int:
        return int(self.tets.shape[0])


def read_sphere_fixture(path) -> SphereFixture:
    """Load the bundled sphere fixture via ``meshio``.

    Returns a :class:`SphereFixture` whose ``tet_physical_tags`` is the
    concatenation of the per-block ``gmsh:physical`` arrays for every
    ``tetra`` cell block, in the same block order that
    ``geode_core::GmshReader`` and ``read_sphere_fixture`` use on the
    Rust side. This makes ``tets[e]`` and ``tet_physical_tags[e]``
    bit-identical to ``mesh.tets[e]`` and ``f.tet_physical_tags[e]`` on
    the Burn side.

    Surface triangles (``triangle`` blocks) are read but discarded — we
    only need them for the boundary group constants, which are pinned
    by ``PHYS_*`` constants above.
    """
    import meshio

    m = meshio.read(path)
    nodes = np.asarray(m.points, dtype=np.float64)

    tet_blocks: list[np.ndarray] = []
    phys_blocks: list[np.ndarray] = []
    if "gmsh:physical" not in m.cell_data:
        raise ValueError(
            f"{path}: meshio output is missing the gmsh:physical cell data; "
            "expected MSH 4.x with $PhysicalNames"
        )
    phys_per_block = m.cell_data["gmsh:physical"]
    for cells, phys in zip(m.cells, phys_per_block):
        if cells.type == "tetra":
            tet_blocks.append(np.asarray(cells.data, dtype=np.int64))
            phys_blocks.append(np.asarray(phys, dtype=np.int32))
    if not tet_blocks:
        raise ValueError(f"no tet cells in {path}")

    tets = np.concatenate(tet_blocks, axis=0)
    tet_physical_tags = np.concatenate(phys_blocks, axis=0)
    return SphereFixture(nodes=nodes, tets=tets, tet_physical_tags=tet_physical_tags)


# --------------------------------------------------------------------------- #
# ε_r assignment — mirror of ``geode_core::build_epsilon_r``.
# --------------------------------------------------------------------------- #


def build_epsilon_r(physical_tags, n_inside: float = 1.5) -> np.ndarray:
    """Per-tet relative permittivity: ``n²`` inside, 1 in the vacuum buffer.

    Tets tagged ``PHYS_SPHERE_INTERIOR`` get ``n_inside ** 2``; every
    other tag (``PHYS_VACUUM_GAP``, ``PHYS_PML_SHELL``, or anything
    else) gets ``1.0``. Faithful port of
    ``crates/geode-core/src/nedelec_assembly.rs::build_epsilon_r``.
    """
    tags = np.asarray(physical_tags, dtype=np.int32)
    eps_inside = float(n_inside) * float(n_inside)
    out = np.where(tags == PHYS_SPHERE_INTERIOR, eps_inside, 1.0)
    return out.astype(np.float64)


# --------------------------------------------------------------------------- #
# Edge enumeration + sign convention.
# --------------------------------------------------------------------------- #


def build_edges(tets):
    """Build the globally-oriented edge table and per-tet edge-sign table.

    Mirror of ``geode_core::TetMesh::edges`` + ``::tet_edges``.

    Parameters
    ----------
    tets : (n_tets, 4) int array

    Returns
    -------
    edges : (n_edges, 2) int64
        Sorted-unique global edge list; each row ``[a, b]`` has
        ``a < b`` (lower-tagged endpoint first — the canonical
        orientation for Nédélec edge DOFs).
    tet_edge_idx : (n_tets, 6) int64
        Per-tet, per-local-edge global edge index. Local edge order is
        :data:`TET_LOCAL_EDGES`.
    tet_edge_sign : (n_tets, 6) int8
        ``+1`` if local edge direction matches the global lower-tag-
        first orientation, ``-1`` otherwise.
    """
    tets = np.asarray(tets, dtype=np.int64)
    n_tets = tets.shape[0]

    # 1. Collect every (lo, hi) pair from every local edge of every tet.
    #    Local edges in `TET_LOCAL_EDGES` order, flatten to (n_tets * 6, 2).
    la_arr = np.asarray([la for la, _ in TET_LOCAL_EDGES], dtype=np.int64)
    lb_arr = np.asarray([lb for _, lb in TET_LOCAL_EDGES], dtype=np.int64)
    # vert_a[e, k] = tets[e, la_arr[k]], shape (n_tets, 6).
    vert_a = tets[:, la_arr]
    vert_b = tets[:, lb_arr]
    lo = np.minimum(vert_a, vert_b)  # (n_tets, 6)
    hi = np.maximum(vert_a, vert_b)  # (n_tets, 6)

    # 2. Build the deduplicated global edge list, sorted lexicographically.
    pair_flat = np.stack([lo.ravel(), hi.ravel()], axis=1)  # (n_tets*6, 2)
    # np.unique sorts and dedupes; with axis=0 it works row-wise.
    edges = np.unique(pair_flat, axis=0).astype(np.int64)
    n_edges = edges.shape[0]

    # 3. Build a (lo, hi) -> global edge index map. We use a dict
    #    because dict-of-tuple lookup is more readable than struct-array
    #    indexing for this size of mesh (~5k edges).
    edge_to_idx: dict[tuple[int, int], int] = {
        (int(edges[i, 0]), int(edges[i, 1])): i for i in range(n_edges)
    }

    # 4. Per-tet edge index + sign.
    tet_edge_idx = np.empty((n_tets, 6), dtype=np.int64)
    tet_edge_sign = np.empty((n_tets, 6), dtype=np.int8)
    for e in range(n_tets):
        for k in range(6):
            a = int(vert_a[e, k])
            b = int(vert_b[e, k])
            if a < b:
                tet_edge_sign[e, k] = 1
                tet_edge_idx[e, k] = edge_to_idx[(a, b)]
            else:
                tet_edge_sign[e, k] = -1
                tet_edge_idx[e, k] = edge_to_idx[(b, a)]
    return edges, tet_edge_idx, tet_edge_sign


# --------------------------------------------------------------------------- #
# PEC mask + interior-node count (spurious-mode dimension).
# --------------------------------------------------------------------------- #


def sphere_pec_interior_edges(nodes, edges, r_outer: float = R_BUFFER):
    """Return ``(interior_mask, on_boundary)`` for the sphere PEC problem.

    Mirror of ``geode_core::sphere_pec_interior_edges``: a node is "on
    the outer PEC wall" iff its radius is within ``tol = 1e-6 *
    max(r_outer, 1)`` of ``r_outer``. An edge is *interior*
    (``mask[e] == True``) iff *at least one* endpoint is strictly
    inside; equivalently, an edge is PEC-eliminated iff *both*
    endpoints lie on the outer sphere. (The Burn helper returns
    ``(edges, mask)``; we instead return ``(mask, on_boundary)`` so the
    edge table can be a positional / shared input.)

    Returns
    -------
    interior_mask : (n_edges,) bool
        ``True`` for edges with at least one strictly-interior endpoint.
    on_boundary : (n_nodes,) bool
        ``True`` for nodes on the outer PEC wall (``|p| ≈ r_outer``).
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    edges = np.asarray(edges, dtype=np.int64)
    tol = 1e-6 * max(r_outer, 1.0)
    r = np.linalg.norm(nodes, axis=1)
    on_boundary = np.abs(r - r_outer) < tol
    a_on = on_boundary[edges[:, 0]]
    b_on = on_boundary[edges[:, 1]]
    interior_mask = ~(a_on & b_on)
    return interior_mask, on_boundary


def sphere_n_interior_nodes(nodes, r_outer: float = R_BUFFER) -> int:
    """Number of nodes *strictly* inside the outer PEC sphere.

    This is the predicted dimension of the discrete curl-curl gradient
    nullspace (the "spurious" eigenvalues that cluster near zero). It
    is the integer cross-check that the boundary-masking and edge-
    orientation logic both match the Burn side.
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tol = 1e-6 * max(r_outer, 1.0)
    r = np.linalg.norm(nodes, axis=1)
    return int(np.sum(np.abs(r - r_outer) >= tol))


# --------------------------------------------------------------------------- #
# Global assembly — ε-scaled Nédélec curl-curl + mass via scipy COO->CSR.
# --------------------------------------------------------------------------- #


def assemble_global_nedelec(nodes, tets, edges, tet_edge_idx, tet_edge_sign, epsilon_r):
    """Assemble global Nédélec stiffness ``K`` and ε-scaled mass ``M``.

    Faithful port of
    ``geode_core::assemble_global_nedelec_with_epsilon``: build per-
    element ``[6, 6]`` Nédélec curl-curl and mass via
    :func:`batched_nedelec_local_matrices`, apply the ``s_i s_j`` sign
    flips, scale mass by ``epsilon_r[e]``, then scatter into the global
    ``[n_edges, n_edges]`` system. COO->CSR collapses duplicate
    ``(row, col)`` triplets by sum (the documented scipy behavior).

    Parameters
    ----------
    nodes : (n_nodes, 3) float64
    tets : (n_tets, 4) int
    edges : (n_edges, 2) int — used only for its row count
    tet_edge_idx : (n_tets, 6) int — from :func:`build_edges`
    tet_edge_sign : (n_tets, 6) int8 — from :func:`build_edges`
    epsilon_r : (n_tets,) float64 — per-tet relative permittivity

    Returns
    -------
    K : scipy.sparse.csr_matrix ``(n_edges, n_edges)`` float64
    M : scipy.sparse.csr_matrix ``(n_edges, n_edges)`` float64
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    tet_edge_idx = np.asarray(tet_edge_idx, dtype=np.int64)
    tet_edge_sign = np.asarray(tet_edge_sign, dtype=np.float64)
    epsilon_r = np.asarray(epsilon_r, dtype=np.float64)
    n_tets = tets.shape[0]
    n_edges = int(edges.shape[0])

    coords = nodes[tets, :]  # (n_tets, 4, 3)
    k_local, m_local, _ = batched_nedelec_local_matrices(coords)

    # Apply per-tet sign outer product: sign[e, i] * sign[e, j].
    # tet_edge_sign has shape (n_tets, 6); outer product → (n_tets, 6, 6).
    sign_outer = tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :]
    k_signed = k_local * sign_outer
    # Mass: scale by per-tet ε before applying sign outer product. The
    # two scalings commute (both diagonal in tet-index) so the order
    # within the multiply doesn't matter.
    m_signed = m_local * sign_outer * epsilon_r[:, None, None]

    # Build COO triplets. For every (e, i, j):
    #   rows[e, i, j] = tet_edge_idx[e, i]
    #   cols[e, i, j] = tet_edge_idx[e, j]
    rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).reshape(-1)
    cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).reshape(-1)
    k_vals = k_signed.reshape(-1)
    m_vals = m_signed.reshape(-1)

    K = scipy.sparse.coo_matrix(
        (k_vals, (rows, cols)), shape=(n_edges, n_edges)
    ).tocsr()
    M = scipy.sparse.coo_matrix(
        (m_vals, (rows, cols)), shape=(n_edges, n_edges)
    ).tocsr()
    return K, M


def apply_dirichlet(K, M, interior_mask):
    """Restrict ``K, M`` to the rows/cols where ``interior_mask`` is True.

    Returns ``(K_int, M_int)`` of shape ``(n_int, n_int)``. The dropped
    PEC rows/cols implement ``n × E = 0`` on the outer wall.
    """
    interior_mask = np.asarray(interior_mask, dtype=bool)
    interior = np.flatnonzero(interior_mask)
    K_int = K[interior, :][:, interior]
    M_int = M[interior, :][:, interior]
    return K_int.tocsr(), M_int.tocsr()


# --------------------------------------------------------------------------- #
# Generalized eigensolve + spurious-mode filter.
# --------------------------------------------------------------------------- #


def eigensolve(K, M, k_request: int):
    """Lowest-``k_request`` generalized eigenpairs of ``K x = λ M x``.

    Uses ``scipy.sparse.linalg.eigsh`` with shift-and-invert at
    ``sigma=0``, ``which='LM'`` — the canonical recipe for the
    "smallest eigenvalues of a symmetric generalized pencil".

    Why shift-and-invert is essential here (unlike the cube cavity):
    the Nédélec curl-curl operator has a *large* gradient kernel
    (dimension ≈ ``sphere_n_interior_nodes``). After PEC reduction the
    DC kernel collapses, but the kernel modes do *not* hit zero
    exactly — they cluster near zero with f64-roundoff noise. ARPACK's
    default ``sigma=None`` (Lanczos in the regular mode) would converge
    onto the kernel's center-of-mass eigenvalue, not the cluster's
    outer rim, and then iterate slowly through ~``spurious_dim``
    near-zero eigenvalues before reaching the physical band. Shift-
    and-invert at σ=0 maps the entire near-zero cluster onto the high
    end of the spectrum of ``(K - σ M)^{-1} M`` and recovers the
    cluster + lowest physical modes in a single block of iterations.
    """
    eigvals, eigvecs = scipy.sparse.linalg.eigsh(
        K.astype(np.float64),
        k=int(k_request),
        M=M.astype(np.float64),
        sigma=0.0,
        which="LM",
    )
    # eigsh in shift-invert mode returns eigenvalues not necessarily sorted.
    order = np.argsort(eigvals)
    return eigvals[order], eigvecs[:, order]


def filter_spurious(lambdas, spurious_dim: int):
    """Largest-relative-gap heuristic — verbatim port of
    ``crates/geode-core/tests/sphere_pec_eigenmode.rs:194-215``.

    Scans the first ``spurious_dim + 5`` slots, finds the largest
    "ratio jump" between consecutive eigenvalues (absolute on the near-
    zero cluster, relative once we leave it), and returns the index
    after that gap as the start of the physical band.

    Returns
    -------
    n_spurious : int
        Number of eigenvalues classified as spurious (the index of the
        first physical mode).
    physical_idx : int
        Same as ``n_spurious`` (alias, kept explicit because the
        downstream filter reads more naturally with both names).
    best_gap : float
        The ratio at the chosen split. The Burn test asserts this is
        ≥ 100x (acceptance criterion 2: clear gap between spurious
        cluster and first physical mode).
    """
    lambdas = np.asarray(lambdas, dtype=np.float64)
    gap_idx = 0
    best_gap = 0.0
    scan_to = min(spurious_dim + 5, len(lambdas) - 1)
    for i in range(scan_to):
        a = abs(lambdas[i])
        b = abs(lambdas[i + 1])
        if a < 1e-9:
            ratio = b
        else:
            ratio = b / a
        if ratio > best_gap:
            best_gap = ratio
            gap_idx = i
    first_physical = gap_idx + 1
    return first_physical, first_physical, best_gap


# --------------------------------------------------------------------------- #
# End-to-end driver — fixture generator + standalone runs both call this.
# --------------------------------------------------------------------------- #


def run_sphere_pec(
    mesh_path,
    n_index: float = 1.5,
    n_take: int = 5,
    r_outer: float = R_BUFFER,
):
    """Full sphere-PEC pipeline; returns a dict for cross-backend
    comparison.

    Parameters
    ----------
    mesh_path : str or Path
        Path to the bundled Gmsh `.msh` fixture
        (``reference/fixtures/sphere_pec/sphere.msh``).
    n_index : float
        Dielectric refractive index inside ``r ≤ R_SPHERE``.
        ``ε_r = n_index²`` inside, ``1`` elsewhere.
    n_take : int
        Number of *physical* eigenvalues to return after spurious
        filtering. The eigensolve fetches ``spurious_dim + 8`` (same
        heuristic as the Burn test).

    Returns
    -------
    dict with all sub-stage outputs needed by
    ``crates/geode-validation/tests/sphere_pec_numpy_reference.rs``:
        - ``n_nodes``, ``n_tets``, ``n_edges``, ``n_interior_edges``,
          ``spurious_dim`` : shape diagnostics
        - ``nodes``, ``tets``, ``tet_physical_tags`` : the loaded mesh
        - ``epsilon_r`` : per-tet ε_r vector (float64)
        - ``edges`` : (n_edges, 2) global edge table
        - ``tet_edge_idx``, ``tet_edge_sign`` : per-tet edge sign tables
        - ``interior_mask`` : boolean edge mask (PEC removed)
        - ``K, M`` : full ε-scaled assembled matrices (pre-Dirichlet)
        - ``K_int, M_int`` : interior matrices (post-Dirichlet)
        - ``eigenvalues_all`` : raw lowest spectrum slice (length
          ``spurious_dim + 8``)
        - ``n_spurious`` : observed spurious count (filter heuristic)
        - ``best_gap`` : largest ratio jump (≥ 100 acceptance gate)
        - ``physical_eigenvalues`` : lowest ``n_take`` physical
          eigenvalues, ascending
        - ``k_int_frobenius``, ``m_int_frobenius`` : Frobenius norms
        - ``k_int_diag``, ``m_int_diag`` : interior diagonals
    """
    fixture = read_sphere_fixture(mesh_path)
    epsilon_r = build_epsilon_r(fixture.tet_physical_tags, n_inside=n_index)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=r_outer
    )
    n_interior_edges = int(np.sum(interior_mask))
    spurious_dim = sphere_n_interior_nodes(fixture.nodes, r_outer=r_outer)

    K, M = assemble_global_nedelec(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        epsilon_r,
    )
    K_int, M_int = apply_dirichlet(K, M, interior_mask)

    n_request = spurious_dim + 8
    eigvals, eigvecs = eigensolve(K_int, M_int, k_request=n_request)

    n_spurious, _, best_gap = filter_spurious(eigvals, spurious_dim)
    if n_spurious + n_take > len(eigvals):
        raise RuntimeError(
            f"requested {n_take} physical modes but only "
            f"{len(eigvals) - n_spurious} available after spurious filter; "
            f"increase n_request or check the heuristic"
        )
    physical = eigvals[n_spurious : n_spurious + n_take]

    return {
        "n_nodes": fixture.n_nodes,
        "n_tets": fixture.n_tets,
        "n_edges": int(edges.shape[0]),
        "n_interior_edges": n_interior_edges,
        "spurious_dim": int(spurious_dim),
        "nodes": fixture.nodes,
        "tets": fixture.tets,
        "tet_physical_tags": fixture.tet_physical_tags,
        "epsilon_r": epsilon_r,
        "edges": edges,
        "tet_edge_idx": tet_edge_idx,
        "tet_edge_sign": tet_edge_sign,
        "interior_mask": interior_mask,
        "K": K,
        "M": M,
        "K_int": K_int,
        "M_int": M_int,
        "eigenvalues_all": eigvals,
        "eigenvectors_all": eigvecs,
        "n_spurious": int(n_spurious),
        "best_gap": float(best_gap),
        "physical_eigenvalues": physical,
        "k_int_frobenius": float(scipy.sparse.linalg.norm(K_int, "fro")),
        "m_int_frobenius": float(scipy.sparse.linalg.norm(M_int, "fro")),
        "k_int_diag": K_int.diagonal().astype(np.float64),
        "m_int_diag": M_int.diagonal().astype(np.float64),
    }


if __name__ == "__main__":
    # CLI: print the lowest 5 physical eigenvalues for the bundled fixture.
    msh = (
        Path(__file__).resolve().parent.parent
        / "fixtures"
        / "sphere_pec"
        / "sphere.msh"
    )
    result = run_sphere_pec(msh, n_index=1.5, n_take=5)
    print(f"sphere fixture: {result['n_nodes']} nodes, {result['n_tets']} tets")
    print(f"global edges: {result['n_edges']}")
    print(
        f"PEC reduction: {result['n_edges']} edges -> "
        f"{result['n_interior_edges']} interior DOFs"
    )
    print(f"predicted spurious-mode count: {result['spurious_dim']}")
    print(f"observed spurious count: {result['n_spurious']}")
    print(f"max ratio jump (spurious -> physical): {result['best_gap']:.3e}")
    print()
    print("lowest 5 physical eigenvalues (λ = k²) and k = sqrt(λ):")
    for i, lam in enumerate(result["physical_eigenvalues"]):
        k_val = float(np.sqrt(lam))
        print(f"  physical[{i}]: λ = {lam:.6e}, k = {k_val:.4f} (1/length)")
