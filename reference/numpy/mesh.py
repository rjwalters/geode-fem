"""Shared NumPy mesh builders for the cube-cavity reference set.

Issue #103: factored out of ``cube_cavity.py`` and
``cube_cavity_minimal.py`` so both NumPy entry points (and the JAX +
TF-Java siblings that consume the programmatic builder) share a single
source of truth for mesh generation, mesh I/O, and the Dirichlet
boundary mask.

Public API
==========

- :func:`cube_tet_mesh(n, side=1.0)` — generate the canonical n-per-side
  tet-split unit cube. Mirror of ``geode_core::mesh::cube_tet_mesh``.
- :func:`cube_interior_mask(nodes, side=1.0)` — boolean mask, True for
  interior (free-DOF) nodes of ``[0, side]^3``. Mirror of
  ``geode_core::eigen::cube_interior_mask``.
- :func:`load_msh(path)` — read a Gmsh ``.msh`` file via ``meshio`` and
  return ``(nodes, tets)`` ndarrays.
- :func:`write_msh(path, nodes, tets)` — write a Gmsh ``.msh`` file via
  ``meshio``; output is consumable by ``geode_core::GmshReader``.

Both ``cube_cavity.py`` (n=10 + Gmsh-fixture path, issue #92) and
``cube_cavity_minimal.py`` (programmatic n=4 path, issue #93) re-export
these symbols so existing consumers (the JAX backend, the TF-Java
sidecar driver, and ``gen_cube_cavity_baseline.py``) keep working
without churn.
"""

from __future__ import annotations

import numpy as np


# --------------------------------------------------------------------------- #
# Programmatic mesh generation
# --------------------------------------------------------------------------- #


def cube_tet_mesh(n: int, side: float = 1.0):
    """Generate the n-per-side tet-split unit cube.

    Faithful NumPy mirror of ``geode_core::cube_tet_mesh`` (see
    ``crates/geode-core/src/mesh/mod.rs``). For each ``n x n x n`` hex
    cell we emit 6 right-handed tets sharing the long diagonal
    ``c[0] -> c[6]``. Node order matches Burn so the same node-indexed
    Dirichlet mask works on both backends.

    Parameters
    ----------
    n : int
        Hexes per side.
    side : float
        Cube side length (default 1.0).

    Returns
    -------
    nodes : ndarray, shape ``((n+1)**3, 3)``, dtype float64
        Node coordinates in lexicographic ``(i, j, k)`` order with ``i``
        fastest.
    tets : ndarray, shape ``(6 * n**3, 4)``, dtype int64
        Tet connectivity, 0-based linear node indices.
    """
    nps = n + 1
    h = side / n

    nodes = np.zeros((nps**3, 3), dtype=np.float64)
    for k in range(nps):
        for j in range(nps):
            for i in range(nps):
                nodes[i + j * nps + k * nps * nps] = [i * h, j * h, k * h]

    def node_idx(i, j, k):
        return i + j * nps + k * nps * nps

    tets = np.empty((6 * n**3, 4), dtype=np.int64)
    t = 0
    for k in range(n):
        for j in range(n):
            for i in range(n):
                c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ]
                # 6-tet split sharing diagonal c[0] -> c[6]. All right-handed.
                tets[t + 0] = [c[0], c[1], c[2], c[6]]
                tets[t + 1] = [c[0], c[2], c[3], c[6]]
                tets[t + 2] = [c[0], c[3], c[7], c[6]]
                tets[t + 3] = [c[0], c[7], c[4], c[6]]
                tets[t + 4] = [c[0], c[4], c[5], c[6]]
                tets[t + 5] = [c[0], c[5], c[1], c[6]]
                t += 6

    return nodes, tets


# --------------------------------------------------------------------------- #
# Dirichlet boundary mask
# --------------------------------------------------------------------------- #


def cube_interior_mask(nodes, side: float = 1.0):
    """Boolean mask: True if the node is strictly inside the cube.

    Mirrors ``geode_core::cube_interior_mask``. A node is "boundary"
    iff any coordinate is within ``1e-9 * max(side, 1)`` of 0 or
    ``side``.
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tol = 1e-9 * max(side, 1.0)
    on_boundary = (
        (nodes[:, 0] < tol)
        | (np.abs(nodes[:, 0] - side) < tol)
        | (nodes[:, 1] < tol)
        | (np.abs(nodes[:, 1] - side) < tol)
        | (nodes[:, 2] < tol)
        | (np.abs(nodes[:, 2] - side) < tol)
    )
    return ~on_boundary


# --------------------------------------------------------------------------- #
# Gmsh ``.msh`` I/O via meshio
# --------------------------------------------------------------------------- #


def load_msh(path):
    """Read a Gmsh ``.msh`` file and return ``(nodes, tets)`` as ndarrays.

    Uses ``meshio`` (well-tested cross-format mesh I/O). Only ``tetra``
    cells are kept — surface triangles and lines are silently dropped
    (they are valid inputs but not the volume elements we want here).
    """
    import meshio

    m = meshio.read(path)
    nodes = np.asarray(m.points, dtype=np.float64)
    tets_blocks = [c.data for c in m.cells if c.type == "tetra"]
    if not tets_blocks:
        raise ValueError(f"no tet cells in {path}")
    tets = np.concatenate(tets_blocks, axis=0).astype(np.int64)
    return nodes, tets


def write_msh(path, nodes, tets):
    """Write a Gmsh ``.msh`` file (MSH 4.1 ASCII) via ``meshio``.

    The output is consumable by ``geode_core::GmshReader`` and by any
    cross-backend reference impl (issue #93's JAX/TF-Java pipeline
    consumes the same file).
    """
    import meshio

    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    m = meshio.Mesh(points=nodes, cells=[("tetra", tets)])
    meshio.write(path, m, file_format="gmsh", binary=False)
