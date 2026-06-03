"""Minimal NumPy cube-cavity Helmholtz pipeline (for #93's parallel coordination).

This module exists so the JAX (#93) and TF-Java (#93) implementations
have a self-contained, reproducible NumPy baseline to agree with *before*
issue #92 lands its canonical `reference/numpy/cube_cavity.py` with the
full Gmsh-fixture flow. When #92 merges, this module becomes a thin
wrapper around (or is replaced by) #92's canonical implementation; the
shape of inputs/outputs is intentionally aligned so the migration is
mechanical.

Why duplicate?
==============

The sweep that scheduled #93 ran wave 2 in parallel with #92. The honest
answer to that scheduling decision is to ship *both* pipelines and let
the harness verify they agree. If #92's eventual baseline differs from
this one, that disagreement is itself an Epic #88 friction artifact (it
implies a meshing or normalization convention drift between sibling
references — informative, by design).

What this is
============

A faithful, line-by-line NumPy transcription of the *same* assembly
math the Burn path runs:

- Mesh: the programmatic `cube_tet_mesh(n, side=1.0)` from
  `geode-core::mesh` — `(n+1)^3` nodes, `6 * n^3` tets, each hex split
  on the long diagonal. Re-implemented here in NumPy to keep this file
  zero-dependency on the Rust side.
- Local matrices: `reference/numpy/p1_local_matrices.py` (already
  landed by #90).
- Global assembly: COO triples → CSR via `scipy.sparse`.
- Dirichlet BC: drop boundary rows/cols.
- Eigensolve: `scipy.sparse.linalg.eigsh(K, k=5, M=M, sigma=0.0)`.

Outputs
=======

`solve_cube_cavity(n, side=1.0, k=5)` returns a dict:

    {
        "n": n,
        "side": side,
        "n_nodes_total": (n+1)**3,
        "n_dofs_interior": (n-1)**3,
        "n_tets": 6 * n**3,
        "eigenvalues": ndarray shape (k,),  # ascending
        "eigenvectors": ndarray shape (n_int, k),  # M-orthonormal
        "k_diag_sum": float,  # trace(K_int), used as autodiff anchor
        "m_diag_sum": float,  # trace(M_int), sanity check
    }

The analytic targets for `side=1.0` are eigenvalues at
`{3, 6, 6, 6, 9} * pi**2 ~= {29.61, 59.22, 59.22, 59.22, 88.83}`.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import scipy.sparse as sp
import scipy.sparse.linalg as spla

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from p1_local_matrices import batched_p1_local_matrices  # noqa: E402


def cube_tet_mesh(n: int, side: float = 1.0):
    """Mirror `geode_core::mesh::cube_tet_mesh` in NumPy.

    Returns (nodes, tets) where:
      - nodes: ndarray shape ((n+1)**3, 3) of vertex coordinates
      - tets: ndarray shape (6 * n**3, 4) of int connectivity

    Each hex is split into 6 right-handed tets sharing the long diagonal
    c[0] → c[6]. Vertex ordering matches the Rust path exactly.
    """
    nps = n + 1
    h = side / n
    # Build nodes in (k, j, i) order so node_idx(i, j, k) = i + j*nps + k*nps^2.
    coords = np.empty((nps**3, 3), dtype=np.float64)
    for k in range(nps):
        for j in range(nps):
            for i in range(nps):
                lin = i + j * nps + k * nps * nps
                coords[lin] = [i * h, j * h, k * h]

    def node_idx(i, j, k):
        return i + j * nps + k * nps * nps

    tets = []
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
                tets.append([c[0], c[1], c[2], c[6]])
                tets.append([c[0], c[2], c[3], c[6]])
                tets.append([c[0], c[3], c[7], c[6]])
                tets.append([c[0], c[7], c[4], c[6]])
                tets.append([c[0], c[4], c[5], c[6]])
                tets.append([c[0], c[5], c[1], c[6]])
    return coords, np.asarray(tets, dtype=np.int64)


def cube_interior_mask(nodes: np.ndarray, side: float = 1.0):
    """Mirror `geode_core::eigen::cube_interior_mask`.

    True = interior (free DOF). False = on any face of [0, side]^3.
    """
    tol = 1e-9 * max(side, 1.0)
    x, y, z = nodes[:, 0], nodes[:, 1], nodes[:, 2]
    on_boundary = (
        (x < tol)
        | (np.abs(x - side) < tol)
        | (y < tol)
        | (np.abs(y - side) < tol)
        | (z < tol)
        | (np.abs(z - side) < tol)
    )
    return ~on_boundary


def assemble_global_p1(nodes: np.ndarray, tets: np.ndarray):
    """Assemble global K, M (CSR) from per-element P1 local matrices.

    Uses COO triples (rows, cols, vals) so duplicate `(i, j)` entries
    from shared nodes accumulate correctly when `tocsr()` runs.
    """
    n_nodes = nodes.shape[0]
    n_elem = tets.shape[0]

    elem_coords = nodes[tets]  # shape (n_elem, 4, 3)
    k_local, m_local, _signed = batched_p1_local_matrices(elem_coords)

    # Flatten local (e, i, j) → global (row, col, val).
    rows = np.repeat(tets, 4, axis=1).reshape(n_elem, 4, 4)  # row = tets[e, i]
    cols = np.tile(tets, 4).reshape(n_elem, 4, 4)  # col = tets[e, j]

    k_csr = sp.coo_matrix(
        (k_local.ravel(), (rows.ravel(), cols.ravel())),
        shape=(n_nodes, n_nodes),
    ).tocsr()
    m_csr = sp.coo_matrix(
        (m_local.ravel(), (rows.ravel(), cols.ravel())),
        shape=(n_nodes, n_nodes),
    ).tocsr()
    return k_csr, m_csr


def restrict_to_interior(k_csr, m_csr, mask: np.ndarray):
    """Drop boundary rows/cols of K, M (homogeneous Dirichlet BC)."""
    idx = np.where(mask)[0]
    # Slice both axes via fancy indexing on CSR.
    k_int = k_csr[idx, :][:, idx]
    m_int = m_csr[idx, :][:, idx]
    return k_int, m_int


def solve_cube_cavity(n: int = 4, side: float = 1.0, k: int = 5, dense: bool = False):
    """End-to-end cube-cavity scalar Helmholtz eigenproblem.

    Returns a dict with eigenvalues + eigenvectors and traces useful as
    differentiation anchors.

    Parameters
    ----------
    n : int
        Cells per side (creates `(n+1)**3` nodes, `6 * n**3` tets).
    side : float
        Cube side length. Analytic eigenvalues scale as `(pi / side)**2`.
    k : int
        Number of lowest eigenmodes to return.
    dense : bool
        If True, use `scipy.linalg.eigh` on a dense matrix (only viable
        for very small `n`); useful as a tiebreaker against the ARPACK
        path for small meshes.
    """
    nodes, tets = cube_tet_mesh(n, side)
    k_csr, m_csr = assemble_global_p1(nodes, tets)
    mask = cube_interior_mask(nodes, side)
    k_int, m_int = restrict_to_interior(k_csr, m_csr, mask)

    n_int = k_int.shape[0]
    if dense or n_int < 30:
        # Dense path: more robust for very small problems (k=5 on n=4 has
        # n_int=27 interior DOFs, which is below ARPACK's typical guard).
        from scipy.linalg import eigh

        k_dense = k_int.toarray()
        m_dense = m_int.toarray()
        eigvals, eigvecs = eigh(k_dense, m_dense)
        eigvals = eigvals[:k]
        eigvecs = eigvecs[:, :k]
    else:
        # Sparse path: ARPACK shift-and-invert at sigma=0.
        eigvals, eigvecs = spla.eigsh(k_int, k=k, M=m_int, sigma=0.0, which="LM")
        # eigsh returns in ascending magnitude of (1/(lam-sigma)); sort by lam.
        order = np.argsort(eigvals)
        eigvals = eigvals[order]
        eigvecs = eigvecs[:, order]

    return {
        "n": n,
        "side": side,
        "n_nodes_total": int(nodes.shape[0]),
        "n_dofs_interior": int(n_int),
        "n_tets": int(tets.shape[0]),
        "eigenvalues": eigvals.astype(np.float64),
        "eigenvectors": eigvecs.astype(np.float64),
        "k_diag_sum": float(k_int.diagonal().sum()),
        "m_diag_sum": float(m_int.diagonal().sum()),
    }


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--n", type=int, default=4)
    parser.add_argument("--side", type=float, default=1.0)
    parser.add_argument("--dense", action="store_true")
    args = parser.parse_args()

    result = solve_cube_cavity(n=args.n, side=args.side, dense=args.dense)
    pi2 = np.pi * np.pi
    targets = np.array([3.0, 6.0, 6.0, 6.0, 9.0]) * pi2
    print(f"n={result['n']}, side={result['side']}, "
          f"n_int={result['n_dofs_interior']}, n_tets={result['n_tets']}")
    print("Lowest 5 eigenvalues:")
    for i, (lam, target) in enumerate(zip(result["eigenvalues"], targets)):
        rel = abs(lam - target) / target
        print(f"  λ[{i}] = {lam:.6e}  target = {target:.6e}  rel_err = {rel:.3e}")
    print(f"trace(K_int) = {result['k_diag_sum']:.6e}")
    print(f"trace(M_int) = {result['m_diag_sum']:.6e}")
