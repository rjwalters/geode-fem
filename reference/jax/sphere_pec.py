"""JAX reference for the sphere-PEC Nédélec eigenmode pipeline (Epic #88 / Phase G.3).

Mirrors reference/numpy/sphere_pec.py but the per-element Nédélec
local matrix computation (curl-curl + ε-mass) runs under jax.vmap/jit.
Edge enumeration, global COO scatter, and PEC mask stay in NumPy
(dynamic shapes, not XLA-traceable).

Autodiff anchor: tr(K_int_cc) as a function of node coordinates is
differentiable through the JAX per-element assembly path.

Usage
=====

    python3 reference/jax/sphere_pec.py          # CLI self-check
    python3 reference/jax/gen_sphere_pec_fixture.py  # write jax_baseline.json

JAX availability
================

If JAX is not installed (`import jax` fails) the module raises
``ImportError`` with a clear message. Install with:

    pip install "jax[cpu]"

The CLI self-check and fixture generator both handle the import error
gracefully and fall back to printing a placeholder message.
"""

from __future__ import annotations

import functools
import sys
from pathlib import Path
from typing import NamedTuple

import numpy as np
import scipy.sparse
import scipy.sparse.linalg

# ---------------------------------------------------------------------------
# Import JAX (hard dependency for this module)
# ---------------------------------------------------------------------------

try:
    import jax
    import jax.numpy as jnp
except ImportError as _jax_err:
    raise ImportError(
        "JAX is required for reference/jax/sphere_pec.py. "
        "Install with: pip install 'jax[cpu]'"
    ) from _jax_err

# Force f64 mode — the entire validation contract is f64.
jax.config.update("jax_enable_x64", True)

# ---------------------------------------------------------------------------
# Import from the NumPy reference (the algorithmic source of truth)
# ---------------------------------------------------------------------------

HERE = Path(__file__).resolve().parent
REPO_REF = HERE.parent  # reference/
sys.path.insert(0, str(REPO_REF / "numpy"))

from sphere_pec import (  # noqa: E402
    PHYS_SPHERE_INTERIOR,
    PHYS_VACUUM_GAP,
    R_BUFFER,
    R_SPHERE,
    TET_LOCAL_EDGES,
    assemble_global_nedelec,
    apply_dirichlet,
    build_edges,
    build_epsilon_r,
    eigensolve,
    filter_spurious,
    read_sphere_fixture,
    restrict_gradient_dense,
    sphere_pec_interior_edges,
    sphere_n_interior_nodes,
    spurious_dim_from_derham,
)


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

N_INDEX: float = 1.5
"""Refractive index inside the dielectric sphere (mirrors geode-core default)."""

_MSH_PATH = REPO_REF / "fixtures" / "sphere_pec" / "sphere.msh"


# ---------------------------------------------------------------------------
# JAX-accelerated per-element local matrix computation
# ---------------------------------------------------------------------------


def _cofactor_gram_jax(verts: jnp.ndarray):
    """Area-weighted cofactor gram for a single tet.

    Parameters
    ----------
    verts : jnp.ndarray, shape (4, 3)
        Four vertex coordinates.

    Returns
    -------
    gg : jnp.ndarray, shape (4, 4)
        Cofactor-gram matrix gg[p, q] = g_p . g_q.
    det : scalar
        det(J) = e1 . (e2 x e3).
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
    g_mat = jnp.stack([g0, g1, g2, g3], axis=0)  # (4, 3)
    gg = g_mat @ g_mat.T  # (4, 4)
    return gg, det


# Canonical local edge table as a JAX-compatible constant.
# Shape: (6, 2) — each row is (local_vertex_a, local_vertex_b).
_LOCAL_EDGES_ARR = jnp.array(
    [(a, b) for a, b in TET_LOCAL_EDGES], dtype=jnp.int32
)  # (6, 2)


def _nedelec_local_cc_one(verts: jnp.ndarray) -> jnp.ndarray:
    """JAX per-element Nédélec curl-curl local matrix for a single tet.

    Faithful JAX transcription of the cofactor reformulation in
    ``reference/numpy/nedelec_local_matrices.py::batched_nedelec_local_matrices``
    for a single element.

    Parameters
    ----------
    verts : jnp.ndarray, shape (4, 3)

    Returns
    -------
    k_local : jnp.ndarray, shape (6, 6)
    """
    gg, det = _cofactor_gram_jax(verts)
    abs_det = jnp.abs(det)
    inv_abs_det3 = 1.0 / (abs_det * abs_det * abs_det)

    def k_entry(i_pair, j_pair):
        a, b = i_pair[0], i_pair[1]
        c, d = j_pair[0], j_pair[1]
        gg_ac = gg[a, c]
        gg_ad = gg[a, d]
        gg_bc = gg[b, c]
        gg_bd = gg[b, d]
        return (2.0 / 3.0) * (gg_ac * gg_bd - gg_ad * gg_bc) * inv_abs_det3

    # Vectorise over all (i, j) pairs via vmap-over-pairs.
    # We build the 6x6 matrix row-by-row using jnp.stack over vmap results.
    edges = _LOCAL_EDGES_ARR  # (6, 2)

    def row_i(i_pair):
        def col_j(j_pair):
            return k_entry(i_pair, j_pair)
        return jax.vmap(col_j)(edges)  # (6,)

    k_local = jax.vmap(row_i)(edges)  # (6, 6)
    return k_local


def _nedelec_local_mass_one(verts: jnp.ndarray, eps_r: float) -> jnp.ndarray:
    """JAX per-element Nédélec ε-mass local matrix for a single tet.

    Faithful JAX transcription of the mass formula from
    ``reference/numpy/nedelec_local_matrices.py``.

    Parameters
    ----------
    verts : jnp.ndarray, shape (4, 3)
    eps_r : scalar float — per-tet relative permittivity

    Returns
    -------
    m_local : jnp.ndarray, shape (6, 6)
    """
    gg, det = _cofactor_gram_jax(verts)
    abs_det = jnp.abs(det)
    inv_abs_det = 1.0 / abs_det
    scale = inv_abs_det / 120.0 * eps_r

    edges = _LOCAL_EDGES_ARR  # (6, 2)

    def m_entry(i_pair, j_pair):
        a, b = i_pair[0], i_pair[1]
        c, d = j_pair[0], j_pair[1]
        gg_ac = gg[a, c]
        gg_ad = gg[a, d]
        gg_bc = gg[b, c]
        gg_bd = gg[b, d]
        f_ac = jnp.where(a == c, 2.0, 1.0)
        f_ad = jnp.where(a == d, 2.0, 1.0)
        f_bc = jnp.where(b == c, 2.0, 1.0)
        f_bd = jnp.where(b == d, 2.0, 1.0)
        return (f_ac * gg_bd - f_ad * gg_bc - f_bc * gg_ad + f_bd * gg_ac) * scale

    def row_i(i_pair):
        def col_j(j_pair):
            return m_entry(i_pair, j_pair)
        return jax.vmap(col_j)(edges)  # (6,)

    m_local = jax.vmap(row_i)(edges)  # (6, 6)
    return m_local


# vmap + jit over element batch: (n_tets, 4, 3) -> (n_tets, 6, 6)
_nedelec_cc_batch_jax = jax.jit(
    jax.vmap(_nedelec_local_cc_one, in_axes=0, out_axes=0)
)

# vmap + jit over element batch with per-element eps_r:
# (n_tets, 4, 3), (n_tets,) -> (n_tets, 6, 6)
_nedelec_mass_batch_jax = jax.jit(
    jax.vmap(_nedelec_local_mass_one, in_axes=(0, 0), out_axes=0)
)


# ---------------------------------------------------------------------------
# JAX-accelerated assembly: per-element via vmap, scatter in NumPy
# ---------------------------------------------------------------------------


def assemble_global_nedelec_jax(
    nodes,
    tets,
    edges,
    tet_edge_idx,
    tet_edge_sign,
    epsilon_r,
):
    """Assemble global Nédélec stiffness K and ε-mass M with JAX per-element kernels.

    Drop-in replacement for ``sphere_pec.assemble_global_nedelec`` but the
    per-element local matrix computation runs under ``jax.vmap``/``jit``
    instead of NumPy loops. The global COO scatter (sum over duplicate
    (row, col) pairs) stays in NumPy/scipy because dynamic shapes are not
    XLA-traceable.

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

    # Per-element vertex coordinates: (n_tets, 4, 3)
    coords = nodes[tets, :]

    # JAX computation: per-element local matrices
    coords_jax = jnp.asarray(coords, dtype=jnp.float64)
    eps_r_jax = jnp.asarray(epsilon_r, dtype=jnp.float64)

    k_local_jax = _nedelec_cc_batch_jax(coords_jax)  # (n_tets, 6, 6)
    m_local_jax = _nedelec_mass_batch_jax(coords_jax, eps_r_jax)  # (n_tets, 6, 6)

    # Transfer back to NumPy for scatter
    k_local = np.asarray(k_local_jax)
    m_local = np.asarray(m_local_jax)

    # Apply per-tet sign outer product: sign[e, i] * sign[e, j]
    sign_outer = tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :]
    k_signed = k_local * sign_outer
    m_signed = m_local * sign_outer

    # Build COO triplets
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


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


class JaxSpherePecResult(NamedTuple):
    n_nodes: int
    n_tets: int
    n_edges: int
    n_interior_edges: int
    spurious_dim: int
    k_int_frobenius: float
    m_int_frobenius: float
    k_int_diag: np.ndarray     # shape [n_interior_edges]
    m_int_diag: np.ndarray     # shape [n_interior_edges]
    eigenvalues_lowest: np.ndarray   # shape [spurious_dim + 8]
    eigenvalues_physical: np.ndarray  # shape [5]


# ---------------------------------------------------------------------------
# Main end-to-end solver
# ---------------------------------------------------------------------------


def solve_sphere_pec_jax(mesh_path=None, n_index: float = 1.5, n_take: int = 5) -> JaxSpherePecResult:
    """Run the JAX sphere-PEC Nédélec pipeline.

    Mirrors ``reference/numpy/sphere_pec.py::run_sphere_pec`` but the
    per-element curl-curl and ε-mass computation runs under JAX vmap/jit.
    Edge enumeration, global COO scatter, and PEC mask remain in NumPy
    (dynamic shapes, not XLA-traceable). Eigensolve is via SciPy
    shift-and-invert eigsh (JAX has no native sparse generalized eigensolver).

    Parameters
    ----------
    mesh_path : str or Path, optional
        Path to the bundled Gmsh `.msh` fixture. Defaults to
        ``reference/fixtures/sphere_pec/sphere.msh``.
    n_index : float
        Refractive index of the dielectric sphere interior (default 1.5).
    n_take : int
        Number of physical eigenvalues to return after spurious filtering.

    Returns
    -------
    JaxSpherePecResult
    """
    if mesh_path is None:
        mesh_path = _MSH_PATH

    # 1. Load mesh (NumPy/meshio)
    fixture = read_sphere_fixture(mesh_path)

    # 2. Build epsilon_r (NumPy)
    epsilon_r = build_epsilon_r(fixture.tet_physical_tags, n_inside=n_index)

    # 3. Build edges + tet_edge_idx + tet_edge_sign (NumPy)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)

    # 4. Build PEC mask (NumPy)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=R_BUFFER
    )
    n_interior_edges = int(np.sum(interior_mask))
    spurious_dim = sphere_n_interior_nodes(fixture.nodes, r_outer=R_BUFFER)

    # 5. Assemble per-element matrices via JAX vmap, scatter in NumPy
    K, M = assemble_global_nedelec_jax(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        epsilon_r,
    )

    # 6. Apply Dirichlet BC (NumPy)
    K_int, M_int = apply_dirichlet(K, M, interior_mask)

    # 7. Spurious dim from d⁰ rank (NumPy SVD)
    n_spurious_algebraic = spurious_dim_from_derham(
        fixture.nodes, edges, interior_mask, r_outer=R_BUFFER
    )

    # 8. Eigensolve: scipy shift-and-invert eigsh
    n_request = spurious_dim + 8
    eigvals, _eigvecs = eigensolve(K_int, M_int, k_request=n_request)

    # 9. Filter spurious modes
    n_spurious, _, _best_gap = filter_spurious(eigvals, n_spurious_algebraic)
    if n_spurious + n_take > len(eigvals):
        raise RuntimeError(
            f"requested {n_take} physical modes but only "
            f"{len(eigvals) - n_spurious} available after spurious filter"
        )
    physical = eigvals[n_spurious: n_spurious + n_take]

    return JaxSpherePecResult(
        n_nodes=fixture.n_nodes,
        n_tets=fixture.n_tets,
        n_edges=int(edges.shape[0]),
        n_interior_edges=n_interior_edges,
        spurious_dim=int(spurious_dim),
        k_int_frobenius=float(scipy.sparse.linalg.norm(K_int, "fro")),
        m_int_frobenius=float(scipy.sparse.linalg.norm(M_int, "fro")),
        k_int_diag=K_int.diagonal().astype(np.float64),
        m_int_diag=M_int.diagonal().astype(np.float64),
        eigenvalues_lowest=eigvals.astype(np.float64),
        eigenvalues_physical=physical.astype(np.float64),
    )


# ---------------------------------------------------------------------------
# Autodiff anchor
# ---------------------------------------------------------------------------


def tr_k_int_cc_from_nodes(nodes_flat: jnp.ndarray, mesh_path=None) -> jnp.ndarray:
    """Scalar functional tr(K_int_cc) as a function of flattened node coordinates.

    Differentiable through the JAX per-element assembly path. The interior
    mask and edge table are computed from the *original* (unperturbed) mesh
    and held fixed during differentiation — standard convention for
    shape-derivative-of-fixed-topology problems.

    Parameters
    ----------
    nodes_flat : jnp.ndarray, shape (n_nodes * 3,), dtype f64
        Flattened node coordinates (row-major, same order as fixture.nodes).
    mesh_path : optional path to .msh file

    Returns
    -------
    scalar jnp.ndarray — tr(K_int_cc)
    """
    if mesh_path is None:
        mesh_path = _MSH_PATH

    fixture = read_sphere_fixture(mesh_path)
    n_nodes = fixture.nodes.shape[0]
    epsilon_r = build_epsilon_r(fixture.tet_physical_tags, n_inside=N_INDEX)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=R_BUFFER
    )
    interior_indices = np.flatnonzero(interior_mask)

    tets_np = np.asarray(fixture.tets, dtype=np.int64)
    tet_edge_idx_np = np.asarray(tet_edge_idx, dtype=np.int64)
    tet_edge_sign_np = np.asarray(tet_edge_sign, dtype=np.float64)
    eps_r_jax = jnp.asarray(epsilon_r, dtype=jnp.float64)
    int_idx = jnp.asarray(interior_indices, dtype=jnp.int32)

    n_edges = edges.shape[0]
    n_tets = fixture.tets.shape[0]

    # Reshape flat input to (n_nodes, 3)
    nodes_3d = nodes_flat.reshape(n_nodes, 3)

    # Per-element vertex coordinates: (n_tets, 4, 3)
    tets_jax = jnp.asarray(tets_np, dtype=jnp.int32)
    coords = nodes_3d[tets_jax]  # (n_tets, 4, 3)

    # JAX per-element curl-curl (no epsilon — curl-curl only, not mass)
    k_local = _nedelec_cc_batch_jax(coords)  # (n_tets, 6, 6)

    # Sign outer product
    sign_outer = jnp.asarray(
        tet_edge_sign_np[:, :, None] * tet_edge_sign_np[:, None, :],
        dtype=jnp.float64,
    )
    k_signed = k_local * sign_outer  # (n_tets, 6, 6)

    # Dense global scatter: K_global[row, col] += k_signed[e, i, j]
    rows_np = np.broadcast_to(tet_edge_idx_np[:, :, None], (n_tets, 6, 6)).reshape(-1)
    cols_np = np.broadcast_to(tet_edge_idx_np[:, None, :], (n_tets, 6, 6)).reshape(-1)
    rows_jax = jnp.asarray(rows_np, dtype=jnp.int32)
    cols_jax = jnp.asarray(cols_np, dtype=jnp.int32)

    k_global = jnp.zeros((n_edges, n_edges), dtype=jnp.float64)
    k_global = k_global.at[rows_jax, cols_jax].add(k_signed.reshape(-1))

    # Restrict to interior and take trace
    k_int = k_global[jnp.ix_(int_idx, int_idx)]
    return jnp.trace(k_int)


def verify_autodiff_fd(n_components: int = 5, eps: float = 1e-5, tol: float = 1e-4, seed: int = 0):
    """Check jax.grad(tr_k_int_cc) vs central finite difference on sampled components.

    Returns a dict with per-component results and an ``all_within_tol`` flag.
    Tolerance is 1e-4 relative (loose, because tr(K_int_cc) involves large
    cancellations for interior nodes sharing many tets).
    """
    fixture = read_sphere_fixture(_MSH_PATH)
    nodes_flat0 = fixture.nodes.flatten().astype(np.float64)
    n_total = nodes_flat0.size

    grad_fn = jax.grad(tr_k_int_cc_from_nodes, argnums=0)
    nodes_flat_jax = jnp.asarray(nodes_flat0)
    g_analytic = np.asarray(grad_fn(nodes_flat_jax))

    rng = np.random.default_rng(seed)
    components = rng.choice(n_total, size=min(n_components, n_total), replace=False)

    results = []
    for c in components:
        nf_plus = nodes_flat0.copy()
        nf_minus = nodes_flat0.copy()
        nf_plus[c] += eps
        nf_minus[c] -= eps
        f_plus = float(tr_k_int_cc_from_nodes(jnp.asarray(nf_plus)))
        f_minus = float(tr_k_int_cc_from_nodes(jnp.asarray(nf_minus)))
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
        "n_components_checked": len(results),
        "eps": eps,
        "tol": tol,
        "all_within_tol": all_pass,
        "per_component": results,
    }


# ---------------------------------------------------------------------------
# CLI self-check
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="JAX sphere-PEC Nédélec self-check")
    parser.add_argument("--skip-autodiff", action="store_true",
                        help="Skip the autodiff anchor verification")
    parser.add_argument("--autodiff-components", type=int, default=5)
    args = parser.parse_args()

    print(f"== JAX sphere-PEC self-check ==  jax={jax.__version__}, "
          f"f64={jax.config.read('jax_enable_x64')}, backend={jax.default_backend()}")

    result = solve_sphere_pec_jax()
    expected_physical = [1.4195, 1.4204, 1.4207, 3.272, 3.277]

    print(f"\nMesh: {result.n_nodes} nodes, {result.n_tets} tets")
    print(f"Global edges: {result.n_edges}")
    print(f"Interior DOFs (after PEC): {result.n_interior_edges}")
    print(f"spurious_dim = {result.spurious_dim}  (expected 368)")
    print(f"K_int Frobenius = {result.k_int_frobenius:.6e}")
    print(f"M_int Frobenius = {result.m_int_frobenius:.6e}")

    print("\nLowest 5 physical eigenvalues (lambda = k^2):")
    all_ok = True
    for i, (lam, exp) in enumerate(zip(result.eigenvalues_physical, expected_physical)):
        rel = abs(lam - exp) / max(abs(exp), 1.0)
        ok = rel < 1e-3
        if not ok:
            all_ok = False
        mark = "OK" if ok else "FAIL"
        print(f"  lambda[{i}] = {lam:.6e}  (expected ~{exp:.4f})  rel_err = {rel:.3e} [{mark}]")

    if not args.skip_autodiff:
        print("\n== Autodiff anchor: jax.grad(tr(K_int_cc)) vs finite difference ==")
        ad = verify_autodiff_fd(n_components=args.autodiff_components)
        print(f"eps={ad['eps']:.1e}, tol={ad['tol']:.1e}, "
              f"checked={ad['n_components_checked']} components")
        for r in ad["per_component"]:
            mark = "OK" if r["within_tol"] else "FAIL"
            print(f"  c={r['component']:4d}  analytic={r['analytic']:+.6e}  "
                  f"fd={r['finite_diff']:+.6e}  rel_err={r['rel_error']:.3e} [{mark}]")
        if not ad["all_within_tol"]:
            all_ok = False
        print(f"\nAll within tol: {ad['all_within_tol']}")

    print(f"\nSelf-check: {'PASS' if all_ok else 'FAIL'}")
    sys.exit(0 if all_ok else 1)
