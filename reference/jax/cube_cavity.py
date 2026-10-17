"""JAX reference for the cube-cavity scalar Helmholtz eigenproblem (Epic #88 / #93).

This is the second backend in #88's reference set, after NumPy. The
algorithmic structure mirrors `reference/numpy/cube_cavity_minimal.py`
exactly; the difference is that the assembly path is expressed in JAX
(``jit`` + ``vmap``) so we can exercise XLA tracing and JAX autodiff on
the same math NumPy runs.

Per #88 Phase C wording ("differentiability of assembly tested
(eigensolve boundary allowed)") and the #93 acceptance criteria, the
eigensolve drops to SciPy at the boundary — JAX has no native sparse
generalized eigensolver, and forcing one would defeat the point of
sequencing JAX with TF-Java (both XLA) against NumPy (canonical
tiebreaker).

Autodiff anchor
===============

`tr(K_int)` as a function of node coordinates is differentiable through
the assembly path. `tr_k_interior_grad` exposes this via `jax.grad`,
and `verify_autodiff_finite_difference` checks the JAX gradient
against a central finite difference within `1e-5`. This is #93's
required AC#2.

DX notes
========

See `reference/jax/README.md` for the JAX-DX friction observations
this implementation surfaced (per the JAX-DX follow-up comment on
#88).
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import NamedTuple

import jax
import jax.numpy as jnp
import numpy as np
import scipy.sparse as sp
import scipy.sparse.linalg as spla

HERE = Path(__file__).resolve().parent
REPO_REF = HERE.parent  # reference/
# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.cube_cavity_minimal import (  # noqa: E402
    _deterministic_arpack_kwargs,
    cube_interior_mask,
    cube_tet_mesh,
)

# Force JAX into f64 mode — the entire validation contract here is f64.
jax.config.update("jax_enable_x64", True)


class JaxCubeCavityResult(NamedTuple):
    n: int
    side: float
    n_nodes_total: int
    n_dofs_interior: int
    n_tets: int
    eigenvalues: np.ndarray
    eigenvectors: np.ndarray
    k_diag_sum: float
    m_diag_sum: float


# ---------------------------------------------------------------------------
# Element-local matrices in JAX
# ---------------------------------------------------------------------------


def _p1_local_one(verts: jnp.ndarray):
    """JAX per-tet P1 local matrices. Faithful transcription of
    ``reference/numpy/p1_local_matrices.py`` for a single tet.

    Parameters
    ----------
    verts : jnp.ndarray, shape (4, 3), dtype f64
        Vertex coordinates of one tetrahedron.

    Returns
    -------
    k_local : jnp.ndarray, shape (4, 4), dtype f64
    m_local : jnp.ndarray, shape (4, 4), dtype f64
    signed_volume : f64 scalar
    """
    v0, v1, v2, v3 = verts[0], verts[1], verts[2], verts[3]
    e1 = v1 - v0
    e2 = v2 - v0
    e3 = v3 - v0

    g1 = jnp.cross(e2, e3)
    g2 = jnp.cross(e3, e1)
    g3 = jnp.cross(e1, e2)
    g0 = -(g1 + g2 + g3)

    det = jnp.dot(e1, g1)
    signed_volume = det / 6.0
    abs_det = jnp.abs(det)

    g_mat = jnp.stack([g0, g1, g2, g3], axis=0)  # (4, 3)
    gg = g_mat @ g_mat.T  # (4, 4)
    k_local = gg / (6.0 * abs_det)

    mass_pattern = jnp.array(
        [
            [2.0, 1.0, 1.0, 1.0],
            [1.0, 2.0, 1.0, 1.0],
            [1.0, 1.0, 2.0, 1.0],
            [1.0, 1.0, 1.0, 2.0],
        ],
        dtype=jnp.float64,
    )
    m_local = mass_pattern * (abs_det / 120.0)
    return k_local, m_local, signed_volume


# vmap across the n_elem axis; jit for XLA lowering.
batched_p1_local_matrices_jax = jax.jit(
    jax.vmap(_p1_local_one, in_axes=0, out_axes=0)
)


# ---------------------------------------------------------------------------
# Assembly (JAX side produces dense K, M; SciPy boundary handles eigensolve).
# ---------------------------------------------------------------------------


def _assemble_dense_jax(nodes: jnp.ndarray, tets: np.ndarray):
    """Assemble dense global K, M as JAX tensors.

    Dense assembly via ``jax.ops.segment_sum``-style scatter is awkward;
    instead we materialize a (n_nodes, n_nodes) buffer of zeros and use
    ``.at[rows, cols].add(...)`` — JAX's analog of scatter-with-add.
    This stays end-to-end traceable for autodiff.

    Parameters
    ----------
    nodes : jnp.ndarray, shape (n_nodes, 3), dtype f64
    tets : np.ndarray (NOT JAX), shape (n_elem, 4), dtype int

    Returns
    -------
    k_global : jnp.ndarray, shape (n_nodes, n_nodes), dtype f64
    m_global : jnp.ndarray, shape (n_nodes, n_nodes), dtype f64
    """
    n_nodes = nodes.shape[0]
    elem_coords = nodes[tets]  # (n_elem, 4, 3)

    k_local, m_local, _ = batched_p1_local_matrices_jax(elem_coords)

    # Flat scatter indices for each (e, i, j) entry: row = tets[e, i], col = tets[e, j].
    n_elem = tets.shape[0]
    tets_arr = jnp.asarray(tets, dtype=jnp.int32)
    # Outer-product index pattern: rows = tets[:, :, None] broadcast to (n_elem, 4, 4),
    # cols = tets[:, None, :] broadcast similarly.
    rows = jnp.broadcast_to(tets_arr[:, :, None], (n_elem, 4, 4))
    cols = jnp.broadcast_to(tets_arr[:, None, :], (n_elem, 4, 4))

    k_global = jnp.zeros((n_nodes, n_nodes), dtype=jnp.float64)
    m_global = jnp.zeros((n_nodes, n_nodes), dtype=jnp.float64)
    k_global = k_global.at[rows.ravel(), cols.ravel()].add(k_local.ravel())
    m_global = m_global.at[rows.ravel(), cols.ravel()].add(m_local.ravel())
    return k_global, m_global


# JIT once over the structural shape; tet connectivity passed as static.
def _assemble_dense_jit_factory(tets: np.ndarray):
    tets_static = tuple(map(tuple, tets.tolist()))  # hashable for static_argnums

    @jax.jit
    def _impl(nodes):
        return _assemble_dense_jax(nodes, tets)
    return _impl


# ---------------------------------------------------------------------------
# End-to-end driver
# ---------------------------------------------------------------------------


def solve_cube_cavity_jax(n: int = 4, side: float = 1.0, k: int = 5,
                          dense_eigh: bool | None = None) -> JaxCubeCavityResult:
    """Run the JAX cube-cavity pipeline and return the lowest `k` modes.

    The mesh and Dirichlet-mask construction are shared with the NumPy
    path (same `cube_tet_mesh`, same `cube_interior_mask`). Assembly is
    in JAX; eigensolve falls through to SciPy.
    """
    nodes_np, tets_np = cube_tet_mesh(n, side)
    mask = cube_interior_mask(nodes_np, side)
    idx = np.where(mask)[0]
    n_int = int(idx.size)

    nodes_jax = jnp.asarray(nodes_np, dtype=jnp.float64)
    assemble = _assemble_dense_jit_factory(tets_np)
    k_global, m_global = assemble(nodes_jax)

    # Force materialization (otherwise device → host transfer is implicit
    # and harder to time).
    k_global_np = np.asarray(k_global)
    m_global_np = np.asarray(m_global)

    k_int = k_global_np[np.ix_(idx, idx)]
    m_int = m_global_np[np.ix_(idx, idx)]

    if dense_eigh is None:
        dense_eigh = n_int < 30

    if dense_eigh:
        from scipy.linalg import eigh

        eigvals, eigvecs = eigh(k_int, m_int)
        eigvals = eigvals[:k]
        eigvecs = eigvecs[:, :k]
    else:
        k_sparse = sp.csr_matrix(k_int)
        m_sparse = sp.csr_matrix(m_int)
        # Deterministic ARPACK iterations: reproducibility for
        # near-degenerate clusters (issue #191).
        det = _deterministic_arpack_kwargs(k_sparse.shape[0], spla.eigsh)
        eigvals, eigvecs = spla.eigsh(
            k_sparse, k=k, M=m_sparse, sigma=0.0, which="LM", **det
        )
        order = np.argsort(eigvals)
        eigvals = eigvals[order]
        eigvecs = eigvecs[:, order]

    return JaxCubeCavityResult(
        n=n,
        side=side,
        n_nodes_total=int(nodes_np.shape[0]),
        n_dofs_interior=n_int,
        n_tets=int(tets_np.shape[0]),
        eigenvalues=eigvals.astype(np.float64),
        eigenvectors=eigvecs.astype(np.float64),
        k_diag_sum=float(np.trace(k_int)),
        m_diag_sum=float(np.trace(m_int)),
    )


# ---------------------------------------------------------------------------
# Autodiff anchor (AC#2)
# ---------------------------------------------------------------------------


def tr_k_interior_from_nodes(nodes_flat: jnp.ndarray, n: int, side: float):
    """Scalar functional `tr(K_int)` as a function of node coordinates.

    Flattened-input form so `jax.grad` operates cleanly on a 1-D array.
    The interior mask is computed from the *original* (unperturbed) mesh
    and held fixed during differentiation — this is the standard
    convention for shape-derivative-of-fixed-topology problems.
    """
    nodes_np, tets_np = cube_tet_mesh(n, side)
    mask = cube_interior_mask(nodes_np, side)
    idx_np = np.where(mask)[0]
    idx = jnp.asarray(idx_np, dtype=jnp.int32)

    n_nodes = nodes_np.shape[0]
    nodes_3d = nodes_flat.reshape(n_nodes, 3)
    k_global, _ = _assemble_dense_jax(nodes_3d, tets_np)
    k_int = k_global[jnp.ix_(idx, idx)]
    return jnp.trace(k_int)


def verify_autodiff_finite_difference(n: int = 3, side: float = 1.0,
                                      eps: float = 1e-6,
                                      max_components: int = 6,
                                      tol: float = 1e-5,
                                      seed: int = 0):
    """Compare `jax.grad(tr_K_int)` against a central finite difference.

    Returns a dict reporting per-component (analytic, fd, abs_error,
    rel_error). The test is "tight" — within `tol = 1e-5` relative on
    every component checked.

    Why not the whole gradient?
    ---------------------------
    A complete check would compute the FD gradient over all 3 *
    (n+1)**3 components; that is 3 * 4**3 = 192 forward+backward evals
    for n=3, which is fast, but the *informative* part of the
    comparison is just "does any one component agree", so we sample
    `max_components` random components for the headline assertion and
    return the full per-component report for inspection.
    """
    nodes_np, _tets_np = cube_tet_mesh(n, side)
    nodes_flat0 = nodes_np.flatten().astype(np.float64)
    n_total = nodes_flat0.size

    grad_fn = jax.grad(tr_k_interior_from_nodes, argnums=0)
    nodes_flat_jax = jnp.asarray(nodes_flat0)
    g_analytic = np.asarray(grad_fn(nodes_flat_jax, n, side))

    rng = np.random.default_rng(seed)
    components = rng.choice(n_total, size=min(max_components, n_total), replace=False)

    results = []
    for c in components:
        nf_plus = nodes_flat0.copy()
        nf_minus = nodes_flat0.copy()
        nf_plus[c] += eps
        nf_minus[c] -= eps
        f_plus = float(tr_k_interior_from_nodes(jnp.asarray(nf_plus), n, side))
        f_minus = float(tr_k_interior_from_nodes(jnp.asarray(nf_minus), n, side))
        fd = (f_plus - f_minus) / (2.0 * eps)
        a = float(g_analytic[c])
        abs_err = abs(a - fd)
        denom = max(abs(a), abs(fd), 1.0)
        rel_err = abs_err / denom
        results.append({
            "component": int(c),
            "analytic": a,
            "finite_diff": fd,
            "abs_error": abs_err,
            "rel_error": rel_err,
            "within_tol": rel_err < tol,
        })

    all_pass = all(r["within_tol"] for r in results)
    return {
        "n": n,
        "side": side,
        "eps": eps,
        "tol": tol,
        "n_components_checked": len(components),
        "all_within_tol": all_pass,
        "per_component": results,
    }


# ---------------------------------------------------------------------------
# CLI: self-check
# ---------------------------------------------------------------------------


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("--n", type=int, default=4, help="Cells per side")
    parser.add_argument("--side", type=float, default=1.0)
    parser.add_argument("--skip-autodiff", action="store_true")
    parser.add_argument("--autodiff-n", type=int, default=3)
    args = parser.parse_args()

    print(f"== JAX cube-cavity self-check ==  jax={jax.__version__}, "
          f"f64={jax.config.read('jax_enable_x64')}, backend={jax.default_backend()}")

    result = solve_cube_cavity_jax(n=args.n, side=args.side)
    pi2 = np.pi * np.pi
    targets = np.array([3.0, 6.0, 6.0, 6.0, 9.0]) * pi2
    print(f"\nn={result.n}, side={result.side}, "
          f"n_int={result.n_dofs_interior}, n_tets={result.n_tets}")
    print("Lowest 5 eigenvalues:")
    for i, (lam, target) in enumerate(zip(result.eigenvalues, targets)):
        rel = abs(lam - target) / target
        print(f"  λ[{i}] = {lam:.6e}  target = {target:.6e}  rel_err = {rel:.3e}")
    print(f"trace(K_int) = {result.k_diag_sum:.6e}")
    print(f"trace(M_int) = {result.m_diag_sum:.6e}")

    if not args.skip_autodiff:
        print("\n== Autodiff anchor: jax.grad(tr(K_int)) vs finite difference ==")
        ad = verify_autodiff_finite_difference(n=args.autodiff_n)
        print(f"n={ad['n']}, eps={ad['eps']:.1e}, tol={ad['tol']:.1e}, "
              f"checked={ad['n_components_checked']} components")
        for r in ad["per_component"]:
            mark = "OK" if r["within_tol"] else "FAIL"
            print(f"  c={r['component']:3d}  analytic={r['analytic']:+.6e}  "
                  f"fd={r['finite_diff']:+.6e}  rel_err={r['rel_error']:.3e} [{mark}]")
        print(f"\nAll within tol: {ad['all_within_tol']}")
