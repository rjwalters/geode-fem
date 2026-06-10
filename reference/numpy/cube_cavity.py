"""NumPy reference for the scalar-Helmholtz cube-cavity eigenmode pipeline.

Issue #92 (Epic #88, Phase B): mirrors the full Burn pipeline in NumPy
so the two backends can be cross-checked sub-stage by sub-stage.

The pipeline
============

1. Mesh I/O — either generate the canonical tet-split cube
   (``cube_tet_mesh(n)``) or load a ``.msh`` via ``meshio``
   (``load_msh(path)``). Both live in :mod:`mesh` (issue #103) and are
   re-exported here for backward compatibility with consumers like
   ``gen_cube_cavity_baseline.py``.
2. P1 local matrices — delegated to :mod:`p1_local_matrices` (NumPy
   reference shared with issue #90; pinned by the per-case fixtures
   under ``reference/fixtures/p1_local/<case>.json`` per #101).
3. Global assembly — scatter per-element ``[4, 4]`` local matrices into
   global ``[n_nodes, n_nodes]`` CSR via
   ``scipy.sparse.coo_matrix(...).tocsr()``. Stays close to the math:
   no clever optimization, no symmetry compression.
4. Dirichlet boundary conditions — restrict K, M to the interior nodes
   (``cube_interior_mask``) by row+column extraction.
5. Generalized eigensolve — ``scipy.sparse.linalg.eigsh`` with
   ``sigma=0`` shift-and-invert at the lowest end of the spectrum.

The eigenvalues are then in the same units as the Burn path. The
expected analytic targets for the unit cube with Dirichlet boundaries
are the Laplacian eigenvalues ``(p² + q² + r²)·π²`` for positive
integers ``p, q, r``:

    λ_0 = (1+1+1)·π² = 3π²        (mode (1,1,1))
    λ_1 = λ_2 = λ_3 = 6π²         (modes (2,1,1), (1,2,1), (1,1,2))
    λ_4 = 9π²                     (mode (2,2,1) permutations — 3 modes
                                   sharing this value, but on a coarse
                                   mesh only the first is recovered in
                                   the lowest 5 alongside the 3 modes
                                   at 6π². See README for details.)

On an n=10 mesh both the Burn path (issue #3) and this NumPy reference
hit the lowest 5 modes ``{3, 6, 6, 6, 9}·π²`` to roughly 4% relative
error — the residual being the standard P1 discretization error
``O(h²)`` on a 10×10×10 cube.

Public API
==========

Mesh primitives (re-exported from :mod:`mesh` for backward compat):

- :func:`cube_tet_mesh(n)` — generate the tet-split cube mesh.
- :func:`load_msh(path)` — read a Gmsh ``.msh`` file via ``meshio``.
- :func:`write_msh(path, nodes, tets)` — write a Gmsh ``.msh`` file.
- :func:`cube_interior_mask(nodes)` — boolean mask of interior nodes.

Assembly + eigensolve:

- :func:`assemble_global_p1(nodes, tets)` — assembled ``(K, M)`` as
  scipy CSR matrices.
- :func:`apply_dirichlet(K, M, mask)` — restrict K, M to interior DOFs.
- :func:`eigensolve(K, M, k=5)` — lowest-``k`` generalized eigenpairs.
- :func:`run_cube_cavity(n=10, k=5)` — orchestrator returning a dict
  with all intermediate quantities for cross-backend comparison.
"""

from __future__ import annotations

import inspect
import sys
from pathlib import Path

import numpy as np
import scipy.sparse
import scipy.sparse.linalg

# Allow `python3 cube_cavity.py` to find the sibling modules regardless of cwd.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from p1_local_matrices import batched_p1_local_matrices  # noqa: E402

# Mesh primitives live in `mesh.py` (issue #103) and are re-exported
# here so existing callers (`gen_cube_cavity_baseline.py`, downstream
# tests) keep importing `cube_tet_mesh`, `load_msh`, `write_msh`, and
# `cube_interior_mask` from `cube_cavity` without churn.
from mesh import (  # noqa: E402, F401
    cube_interior_mask,
    cube_tet_mesh,
    load_msh,
    write_msh,
)


# --------------------------------------------------------------------------- #
# Global P1 assembly via coo_matrix.tocsr()
# --------------------------------------------------------------------------- #


def assemble_global_p1(nodes, tets):
    """Assemble global stiffness ``K`` and consistent mass ``M`` (CSR).

    Builds per-element ``[4, 4]`` locals (from
    :func:`p1_local_matrices.batched_p1_local_matrices`) then scatters
    into the global COO triplet, which scipy's COO->CSR conversion
    collapses duplicates by sum (the documented behavior of
    ``coo_matrix.tocsr``).

    Stays close to the math; no clever sparsity-pattern caching. The
    cube-cavity n=10 mesh is well under 10⁴ DOFs so wall time is a
    non-issue.

    Returns
    -------
    K : scipy.sparse.csr_matrix, shape ``(n_nodes, n_nodes)``, dtype f64
    M : scipy.sparse.csr_matrix, shape ``(n_nodes, n_nodes)``, dtype f64
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    n_nodes = nodes.shape[0]
    n_elem = tets.shape[0]

    # Per-element vertex coords: shape (n_elem, 4, 3).
    coords = nodes[tets, :]
    k_local, m_local, _signed_v = batched_p1_local_matrices(coords)

    # Row / col index arrays for every (e, i, j) triple, shape (n_elem*16,).
    # rows[e*16 + i*4 + j] = tets[e, i]
    # cols[e*16 + i*4 + j] = tets[e, j]
    rows = np.repeat(tets, 4, axis=1).reshape(n_elem * 16)
    cols = np.tile(tets, (1, 4)).reshape(n_elem * 16)

    k_vals = k_local.reshape(n_elem * 16)
    m_vals = m_local.reshape(n_elem * 16)

    K = scipy.sparse.coo_matrix(
        (k_vals, (rows, cols)), shape=(n_nodes, n_nodes)
    ).tocsr()
    M = scipy.sparse.coo_matrix(
        (m_vals, (rows, cols)), shape=(n_nodes, n_nodes)
    ).tocsr()

    return K, M


def apply_dirichlet(K, M, mask):
    """Restrict ``K, M`` to the rows/cols where ``mask`` is True.

    Returns ``(K_int, M_int)`` of shape ``(n_int, n_int)``. The dropped
    boundary rows/cols implement homogeneous Dirichlet conditions on
    the eliminated DOFs.
    """
    mask = np.asarray(mask, dtype=bool)
    interior = np.flatnonzero(mask)
    # CSR slicing the row dim then the col dim is the standard idiom.
    K_int = K[interior, :][:, interior]
    M_int = M[interior, :][:, interior]
    # Convert back to CSR (the chained slice yields CSR in scipy >= 1.4).
    return K_int.tocsr(), M_int.tocsr()


# --------------------------------------------------------------------------- #
# Generalized eigensolve
# --------------------------------------------------------------------------- #


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


def eigensolve(K, M, k: int = 5):
    """Lowest-k generalized eigenpairs of ``K x = λ M x``.

    Uses ``scipy.sparse.linalg.eigsh`` (ARPACK-backed) with
    shift-and-invert at ``sigma=0`` and ``which='LM'`` — the canonical
    "smallest eigenvalues of a symmetric generalized pencil" recipe.

    Both ``K`` and ``M`` should be symmetric; ``M`` should be SPD.

    Returns
    -------
    eigvals : ndarray, shape ``(k,)``, dtype f64, ascending order.
    eigvecs : ndarray, shape ``(n_int, k)``, dtype f64.
        Columns are the corresponding eigenvectors, M-orthonormal
        (eigsh normalizes columns to satisfy ``v_i^T M v_j = δ_ij``).
    """
    # eigsh's shift-and-invert path requires a sparse SPD K-σM; at σ=0 that
    # is just K, which is positive *semi*-definite (the constant mode is in
    # the null space, but Dirichlet-restricted K is SPD on the interior).
    # Deterministic ARPACK iterations: reproducibility for
    # near-degenerate clusters (issue #191).
    det = _deterministic_arpack_kwargs(K.shape[0], scipy.sparse.linalg.eigsh)
    eigvals, eigvecs = scipy.sparse.linalg.eigsh(
        K.astype(np.float64),
        k=k,
        M=M.astype(np.float64),
        sigma=0.0,
        which="LM",
        **det,
    )
    # eigsh in shift-invert mode returns eigenvalues not necessarily sorted.
    order = np.argsort(eigvals)
    return eigvals[order], eigvecs[:, order]


# --------------------------------------------------------------------------- #
# End-to-end driver — used by the fixture generator and standalone runs
# --------------------------------------------------------------------------- #


def run_cube_cavity(
    n: int = 10,
    k: int = 5,
    mesh_path: str | None = None,
    side: float = 1.0,
):
    """Full cube-cavity pipeline; returns a dict for cross-backend comparison.

    Either ``n`` (generate the mesh in-process) or ``mesh_path`` (load a
    Gmsh ``.msh``) controls the mesh source.

    Returns
    -------
    dict with keys:
        - ``n_nodes``, ``n_tets``, ``n_int``: shape diagnostics
        - ``nodes``, ``tets``: the mesh used
        - ``interior_mask``: boolean array length ``n_nodes``
        - ``K``, ``M``: global CSR matrices (full, pre-Dirichlet)
        - ``K_int``, ``M_int``: interior CSR matrices (post-Dirichlet)
        - ``eigenvalues``: ascending, length ``k``
        - ``eigenvectors``: M-orthonormal columns, shape ``(n_int, k)``
        - ``k_frobenius``, ``m_frobenius``: Frobenius norms of K, M (full)
        - ``k_int_frobenius``, ``m_int_frobenius``: same for interior
        - ``k_int_diag``, ``m_int_diag``: diagonals of K_int, M_int
    """
    if mesh_path is not None:
        nodes, tets = load_msh(mesh_path)
    else:
        nodes, tets = cube_tet_mesh(n, side=side)

    K, M = assemble_global_p1(nodes, tets)
    mask = cube_interior_mask(nodes, side=side)
    K_int, M_int = apply_dirichlet(K, M, mask)
    eigvals, eigvecs = eigensolve(K_int, M_int, k=k)

    return {
        "n_nodes": int(nodes.shape[0]),
        "n_tets": int(tets.shape[0]),
        "n_int": int(K_int.shape[0]),
        "nodes": nodes,
        "tets": tets,
        "interior_mask": mask,
        "K": K,
        "M": M,
        "K_int": K_int,
        "M_int": M_int,
        "eigenvalues": eigvals,
        "eigenvectors": eigvecs,
        "k_frobenius": float(scipy.sparse.linalg.norm(K, "fro")),
        "m_frobenius": float(scipy.sparse.linalg.norm(M, "fro")),
        "k_int_frobenius": float(scipy.sparse.linalg.norm(K_int, "fro")),
        "m_int_frobenius": float(scipy.sparse.linalg.norm(M_int, "fro")),
        "k_int_diag": K_int.diagonal().astype(np.float64),
        "m_int_diag": M_int.diagonal().astype(np.float64),
    }


# --------------------------------------------------------------------------- #
# Analytic targets — Dirichlet eigenvalues of the unit cube Laplacian
# --------------------------------------------------------------------------- #


def analytic_lowest_five():
    """Lowest 5 Dirichlet Laplacian eigenvalues on the unit cube.

    Mode (p, q, r) has eigenvalue ``(p² + q² + r²)·π²``. The five
    smallest by ``p²+q²+r²``:

        (1,1,1) — 3π²
        (2,1,1), (1,2,1), (1,1,2) — 6π² (3-fold degenerate)
        (2,2,1) and permutations — 9π² (3-fold degenerate; we only
        pull the first into the lowest-5 set since the next 9π² mode
        is interleaved with other 9π² modes — see eigensolver tests
        ``crates/geode-core/examples/eigen_convergence.rs`` which uses
        exactly this 5-tuple for the same reason).
    """
    pi2 = np.pi**2
    return np.array(
        [3.0 * pi2, 6.0 * pi2, 6.0 * pi2, 6.0 * pi2, 9.0 * pi2], dtype=np.float64
    )


if __name__ == "__main__":
    # CLI: print the lowest-5 eigenvalues for the n=10 mesh.
    result = run_cube_cavity(n=10, k=5)
    targets = analytic_lowest_five()
    pi2 = np.pi**2

    print(f"NumPy cube-cavity n=10: n_int = {result['n_int']}")
    print()
    print("idx  target/π²   λ_h/π²    rel err")
    print("---  ---------   ------    ---------")
    for i, (got, want) in enumerate(zip(result["eigenvalues"], targets)):
        rel = abs(got - want) / want * 100.0
        print(f"{i:<3}  {want / pi2:.4f}      {got / pi2:.4f}    {rel:+.4f}%")
