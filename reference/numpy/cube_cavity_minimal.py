"""Minimal NumPy cube-cavity Helmholtz pipeline (programmatic-mesh sibling).

This module is the NumPy oracle for the programmatic-mesh cube-cavity
spine slice (issue #93). It is the sibling of
:mod:`cube_cavity` — same assembly math, same eigensolve, different
mesh source: the programmatic ``cube_tet_mesh(n)`` rather than the
Gmsh-fixture ``unit_cube.msh`` at n=10. The JAX (#93) and TF-Java
(#93) backends consume the same programmatic mesh so the cross-backend
comparison is not contaminated by mesh-reader friction.

Issue #103 factored the mesh primitives (``cube_tet_mesh``,
``cube_interior_mask``, ``load_msh``, ``write_msh``) into the shared
:mod:`mesh` module so the two cube-cavity entry points share one source
of truth. This module re-exports ``cube_tet_mesh`` and
``cube_interior_mask`` so the existing JAX consumer
(``reference/jax/cube_cavity.py``) keeps importing them from
``cube_cavity_minimal`` without churn.

What this is
============

A faithful, line-by-line NumPy transcription of the *same* assembly
math the Burn path runs:

- Mesh: the programmatic `cube_tet_mesh(n, side=1.0)` from
  :mod:`mesh` — `(n+1)^3` nodes, `6 * n^3` tets, each hex split on the
  long diagonal. NumPy mirror of `geode_core::mesh::cube_tet_mesh`.
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

import inspect
import sys
from pathlib import Path

import numpy as np
import scipy.sparse as sp
import scipy.sparse.linalg as spla

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.p1_local_matrices import batched_p1_local_matrices  # noqa: E402

# Mesh primitives live in `mesh.py` (issue #103) — re-exported here so
# the JAX consumer (`reference/jax/cube_cavity.py`) keeps importing
# `cube_tet_mesh` and `cube_interior_mask` from this module without
# churn.
from reference.numpy.mesh import cube_interior_mask, cube_tet_mesh  # noqa: E402, F401


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


def _deterministic_arpack_kwargs(n, solver, complex_pencil=False):
    """Deterministic ARPACK kwargs (issue #191).

    Fixed-seed normalized start vector ``v0`` so near-degenerate
    clusters converge reproducibly run-to-run; on scipy >= 1.17 also a
    fixed-seed ``rng``, because the rewritten ARPACK wrapper draws its
    internal *restart* vectors from OS entropy even when ``v0`` is
    pinned (older Fortran-ARPACK scipy is deterministic given ``v0``).
    """
    rng = np.random.default_rng(0)
    if complex_pencil:
        v0 = rng.standard_normal(n) + 1j * rng.standard_normal(n)
    else:
        v0 = rng.standard_normal(n)
    v0 = v0 / np.linalg.norm(v0)
    kwargs = {"v0": v0}
    if "rng" in inspect.signature(solver).parameters:
        kwargs["rng"] = np.random.default_rng(0)
    return kwargs


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
        # Deterministic ARPACK iterations: reproducibility for
        # near-degenerate clusters (issue #191).
        det = _deterministic_arpack_kwargs(k_int.shape[0], spla.eigsh)
        eigvals, eigvecs = spla.eigsh(
            k_int, k=k, M=m_int, sigma=0.0, which="LM", **det
        )
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
