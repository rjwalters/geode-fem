"""JAX reference for the anisotropic-UPML dielectric-sphere Mie pipeline
(Epic #88 / Phase J.4 / Issue #173).

Port of ``reference/numpy/sphere_mie.py`` (Phase J.2, PR #179) to JAX:
the per-element Nédélec curl-curl stays on the existing real kernel
(``reference/jax/sphere_pec.py``), the per-element mass becomes the
**tensor-valued** complex kernel
:func:`_nedelec_local_mass_aniso_one` — the JAX transcription of
``geode_core::batched_nedelec_local_mass_anisotropic_diag`` /
``numpy.sphere_mie.batched_nedelec_local_mass_anisotropic_diag``
(per-axis cofactor gram contracted with the diagonal complex ε tensor).

Design choices (Phase J.4-specific)
===================================

* **Tensor-valued constitutive in the mass, in-graph.** Unlike the
  Phase H.3 scalar-isotropic port (``sphere_pml.py`` here), where the
  complex ε was a per-tet *scalar* multiplied onto a precomputed real
  mass block, the diagonal UPML tensor weights the three per-axis
  cofactor grams **independently** — the complex arithmetic cannot be
  factored out of the kernel. The batched kernel therefore consumes
  ``eps_diag : (n_tets, 3) complex128`` directly and runs in c128
  under ``jax_enable_x64``.

* **BCOO[complex128] global scatter.** The global K and M are
  materialised as ``jax.experimental.sparse.BCOO`` with
  ``complex128`` data (issue #173 explicitly asks for the
  ``BCOO[complex128]`` port), deduplicated via ``sum_duplicates``,
  then handed to SciPy CSR for the host-side eigensolve. Phase H.3
  avoided BCOO in the *differentiated* path; here the BCOO leg is the
  fixture-generation path while the autodiff probe keeps the H.3
  dense-scatter convention so the two probes stay comparable.

* **Eigensolve boundary (sidecar convention).** Dense host LAPACK
  ZGGEV via ``numpy.sphere_pml.eigensolve_complex_dense`` — the
  canonical-tiebreaker path established by PR #155/#179. The ARPACK
  shift-invert path is unusable on this pencil (near-singular
  gradient kernel at λ = 0; see the NumPy docstring). No backend
  lowers a generalized complex eigensolve in-graph (Stage 7 ONNX
  audit boundary), so the eigensolve stays out-of-graph.

* **σ₀ = 0 regression.** The tensor collapses to the real isotropic
  scalar (``ε_x = ε_y = ε_z = bg``); the spectrum must be real to f64
  precision and match the sphere_pml_small PEC anchor.

Autodiff probe (the Phase J.4 spec payoff)
==========================================

The 2026-06-05 finding on #88 (Phase H.3): ``jax.grad`` traces cleanly
through *scalar-isotropic* c128 PML assembly with zero custom VJPs,
explicitly caveated that anisotropic UPML was **not exercised**.
:func:`probe_autodiff_tensor_assembly` closes that caveat: the
differentiated parameter is the full per-tet **tensor** ``(n_tets, 3)``
split into real/imaginary halves, and the loss flows through the
per-axis complex kernel itself (vmap-traced), the complex scatter,
interior restriction, and the same ``tr(K_int) + |Tr(M_int)|²``
functional as the H.3 probe — reporting ``jit_ok`` / ``grad_ok`` /
``||grad_re||_∞`` / ``||grad_im||_∞`` in the identical format. If
custom VJPs were needed for the tensor path, the probe would fail
loudly here (and that would partially reverse the H.3 finding).

Usage
=====

    python3 reference/jax/sphere_mie.py             # CLI self-check (small mesh)
    python3 reference/jax/sphere_mie.py --sigma0 0.0  # PEC-collapse regression
    python3 reference/jax/gen_sphere_mie_fixture.py   # write jax_baseline.json
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import scipy.sparse

# ---------------------------------------------------------------------------
# Import JAX (hard dependency)
# ---------------------------------------------------------------------------

try:
    import jax
    import jax.numpy as jnp
    from jax.experimental import sparse as jsparse
except ImportError as _jax_err:
    raise ImportError(
        "JAX is required for reference/jax/sphere_mie.py. "
        "Install with: pip install 'jax[cpu]'"
    ) from _jax_err

# Force f64/c128 mode — the entire validation contract is f64/c128.
jax.config.update("jax_enable_x64", True)

# ---------------------------------------------------------------------------
# Import from siblings.
#
# The NumPy reference directory is pushed to the front of sys.path so
# that bare module names (`sphere_pec`, `sphere_pml`, `sphere_mie`,
# `nedelec_local_matrices`) always resolve to the *NumPy* modules —
# the JAX siblings with the same file names are only ever loaded by
# explicit file path under private module names (`_jax_*`). This is
# the same collision-avoidance pattern as `reference/jax/sphere_pml.py`.
# ---------------------------------------------------------------------------

HERE = Path(__file__).resolve().parent
REPO_REF = HERE.parent  # reference/

sys.path.insert(0, str(REPO_REF / "numpy"))

import importlib.util as _ilu  # noqa: E402


def _load_by_path(name: str, path: Path):
    spec = _ilu.spec_from_file_location(name, path)
    mod = _ilu.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# JAX sphere-PEC kernels (curl-curl + local-edge table) by file path.
_jax_pec = _load_by_path("_jax_sphere_pec_kernels", HERE / "sphere_pec.py")
_nedelec_cc_batch_jax = _jax_pec._nedelec_cc_batch_jax
_LOCAL_EDGES_ARR = _jax_pec._LOCAL_EDGES_ARR  # (6, 2) int32

# NumPy reference modules (algorithmic source of truth).
from sphere_pec import (  # noqa: E402
    R_BUFFER,
    R_PML_INNER,
    R_SPHERE,
    build_edges,
    read_sphere_fixture,
    sphere_n_interior_nodes,
    sphere_pec_interior_edges,
    spurious_dim_from_derham,
)
from sphere_pml import eigensolve_complex_dense  # noqa: E402

# The J.2 NumPy Mie module: constitutive tensor builder, λ → k helpers,
# and the J.1 catalogue classification — all shared verbatim so the JAX
# port differs from NumPy *only* in the assembly kernels.
from sphere_mie import (  # noqa: E402
    K0_REF,
    SIGMA_0_DEFAULT,
    build_anisotropic_pml_tensor_diag,
    classify_modes_against_catalogue,
    lambda_to_k,
    load_mie_roots_catalogue,
    q_factor_from_lambda,
    tet_centroids,
)

N_INDEX: float = 1.5

# Default mesh: the 197-tet small mesh shared with sphere_pml_small —
# the granularity that fits the default-CI budget (#158 / #164 / #160
# small-mesh precedent; the full-mesh dense ZGGEV is multi-minute).
_SMALL_MSH_PATH = REPO_REF / "fixtures" / "sphere_pml_small" / "sphere.msh"


# ---------------------------------------------------------------------------
# JAX tensor-ε Nédélec local mass kernel
# ---------------------------------------------------------------------------


def _nedelec_local_mass_aniso_one(verts: jnp.ndarray, eps_diag: jnp.ndarray) -> jnp.ndarray:
    """Per-element Nédélec mass under a diagonal complex permittivity tensor.

    JAX transcription of
    ``numpy.sphere_mie.batched_nedelec_local_mass_anisotropic_diag``
    (mirror of ``geode_core::batched_nedelec_local_mass_anisotropic_diag``)
    for a single tet:

    ```text
    M_ij = Σ_α ε_α / (120 |det|) [  (1+δ_ac) gg^(α)_bd − (1+δ_ad) gg^(α)_bc
                                  − (1+δ_bc) gg^(α)_ad + (1+δ_bd) gg^(α)_ac ]
    ```

    with the per-axis cofactor gram ``gg^(α)_pq = g_p[α] g_q[α]``.
    Since ``Σ_α gg^(α) = gg`` (the scalar gram), equal weights collapse
    to exactly ``ε ×`` the scalar mass — the isotropic-collapse
    regression exercised at σ₀ = 0.

    Parameters
    ----------
    verts : (4, 3) f64
    eps_diag : (3,) complex128 — ``(ε_x, ε_y, ε_z)`` for this tet.

    Returns
    -------
    m_local : (6, 6) complex128
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

    abs_det = jnp.abs(det)
    inv_abs_det = 1.0 / abs_det

    # Per-axis cofactor gram: gg_axis[α, p, q] = g_p[α] g_q[α].
    gg_axis = jnp.einsum("pa,qa->apq", g_mat, g_mat)  # (3, 4, 4)

    eps_c = eps_diag.astype(jnp.complex128)
    edges = _LOCAL_EDGES_ARR  # (6, 2)

    def m_entry(i_pair, j_pair):
        a, b = i_pair[0], i_pair[1]
        c, d = j_pair[0], j_pair[1]
        f_ac = jnp.where(a == c, 2.0, 1.0)
        f_ad = jnp.where(a == d, 2.0, 1.0)
        f_bc = jnp.where(b == c, 2.0, 1.0)
        f_bd = jnp.where(b == d, 2.0, 1.0)
        # Per-axis Kronecker-lifted term, shape (3,) real.
        m_term = (
            f_ac * gg_axis[:, b, d]
            - f_ad * gg_axis[:, b, c]
            - f_bc * gg_axis[:, a, d]
            + f_bd * gg_axis[:, a, c]
        )
        # Weight per axis by ε_α and sum over α — the tensor-specific
        # complex contraction (cannot be hoisted out of the kernel).
        return jnp.sum(m_term.astype(jnp.complex128) * eps_c) * inv_abs_det / 120.0

    def row_i(i_pair):
        def col_j(j_pair):
            return m_entry(i_pair, j_pair)

        return jax.vmap(col_j)(edges)  # (6,)

    return jax.vmap(row_i)(edges)  # (6, 6) complex128


# vmap + jit over element batch with per-element (3,)-tensor ε:
# (n_tets, 4, 3) f64, (n_tets, 3) c128 -> (n_tets, 6, 6) c128
_nedelec_mass_aniso_batch_jax = jax.jit(
    jax.vmap(_nedelec_local_mass_aniso_one, in_axes=(0, 0), out_axes=0)
)


# ---------------------------------------------------------------------------
# Global assembly: JAX kernels + BCOO[complex128] scatter
# ---------------------------------------------------------------------------


def assemble_global_nedelec_anisotropic_jax(
    nodes,
    tets,
    edges,
    tet_edge_idx,
    tet_edge_sign,
    eps_tensor_diag,
):
    """Assemble global Nédélec K (real-valued, typed complex) and tensor-ε
    complex M via JAX kernels and a ``BCOO[complex128]`` global scatter.

    Issue #173 asks for the BCOO[complex128] port explicitly: the
    per-element blocks are scattered into
    ``jax.experimental.sparse.BCOO`` complex128 matrices and
    deduplicated with ``sum_duplicates`` *inside JAX*; the deduplicated
    (indices, data) pair is then extracted to a SciPy CSR for the
    host-side dense eigensolve. (Phase H.3 deliberately avoided BCOO;
    this leg documents that BCOO carries c128 data without friction.)

    Returns
    -------
    K : scipy.sparse.csr_matrix complex128, shape ``(n_edges, n_edges)``
        (``Im(K) = 0`` identically — curl-curl is ε-independent.)
    M : scipy.sparse.csr_matrix complex128, shape ``(n_edges, n_edges)``
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    tet_edge_idx = np.asarray(tet_edge_idx, dtype=np.int64)
    tet_edge_sign = np.asarray(tet_edge_sign, dtype=np.float64)
    eps_tensor_diag = np.asarray(eps_tensor_diag, dtype=np.complex128)
    n_tets = tets.shape[0]
    n_edges = int(edges.shape[0])

    coords = nodes[tets, :]  # (n_tets, 4, 3)
    coords_jax = jnp.asarray(coords, dtype=jnp.float64)

    # Per-element blocks via the JAX kernels.
    k_local = _nedelec_cc_batch_jax(coords_jax)  # (n_tets, 6, 6) f64
    m_local = _nedelec_mass_aniso_batch_jax(
        coords_jax, jnp.asarray(eps_tensor_diag, dtype=jnp.complex128)
    )  # (n_tets, 6, 6) c128

    # Per-tet sign outer product s_i s_j (real-valued).
    sign_outer = jnp.asarray(
        tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :], dtype=jnp.float64
    )
    k_signed = (k_local * sign_outer).astype(jnp.complex128).reshape(-1)
    m_signed = (m_local * sign_outer.astype(jnp.complex128)).reshape(-1)

    rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).reshape(-1)
    cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).reshape(-1)
    indices = jnp.asarray(np.stack([rows, cols], axis=1), dtype=jnp.int32)

    # BCOO[complex128] scatter + in-JAX deduplication.
    k_bcoo = jsparse.BCOO((k_signed, indices), shape=(n_edges, n_edges)).sum_duplicates()
    m_bcoo = jsparse.BCOO((m_signed, indices), shape=(n_edges, n_edges)).sum_duplicates()

    def _bcoo_to_csr(bcoo) -> scipy.sparse.csr_matrix:
        data = np.asarray(bcoo.data, dtype=np.complex128)
        idx = np.asarray(bcoo.indices, dtype=np.int64)
        # sum_duplicates pads to nse with zero-data entries at
        # out-of-range sentinel indices on some versions; mask defensively.
        valid = (idx[:, 0] < n_edges) & (idx[:, 1] < n_edges)
        return scipy.sparse.coo_matrix(
            (data[valid], (idx[valid, 0], idx[valid, 1])),
            shape=(n_edges, n_edges),
            dtype=np.complex128,
        ).tocsr()

    return _bcoo_to_csr(k_bcoo), _bcoo_to_csr(m_bcoo)


def apply_dirichlet_complex(K, M, interior_mask):
    """PEC reduction preserving complex dtype (same as jax/sphere_pml.py)."""
    interior_mask = np.asarray(interior_mask, dtype=bool)
    interior = np.flatnonzero(interior_mask)
    K_int = K[interior, :][:, interior]
    M_int = M[interior, :][:, interior]
    return K_int.tocsr(), M_int.tocsr()


# ---------------------------------------------------------------------------
# End-to-end driver — mirror of numpy.sphere_mie.run_sphere_mie
# ---------------------------------------------------------------------------


def run_sphere_mie_jax(
    mesh_path=None,
    sigma_0: float = SIGMA_0_DEFAULT,
    n_index: float = N_INDEX,
    k0_ref: float = K0_REF,
    n_take: int = 5,
    r_outer: float = R_BUFFER,
) -> dict:
    """Full anisotropic-UPML Mie pipeline with JAX assembly; returns the
    same result dict shape as ``numpy.sphere_mie.run_sphere_mie`` so the
    fixture generators and cross-checks are interchangeable."""
    if mesh_path is None:
        mesh_path = _SMALL_MSH_PATH

    fixture = read_sphere_fixture(mesh_path)
    centroids = tet_centroids(fixture.nodes, fixture.tets)
    eps_tensor_diag = build_anisotropic_pml_tensor_diag(
        fixture.tet_physical_tags,
        centroids,
        n_inside=n_index,
        sigma_0=sigma_0,
        k0_ref=k0_ref,
    )
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=r_outer
    )
    n_interior_edges = int(np.sum(interior_mask))
    spurious_dim = sphere_n_interior_nodes(fixture.nodes, r_outer=r_outer)

    K, M = assemble_global_nedelec_anisotropic_jax(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        eps_tensor_diag,
    )
    K_int, M_int = apply_dirichlet_complex(K, M, interior_mask)

    n_spurious = int(
        spurious_dim_from_derham(fixture.nodes, edges, interior_mask, r_outer=r_outer)
    )

    n_request = spurious_dim + 8
    eigvals = eigensolve_complex_dense(K_int, M_int, k_take=n_request)

    if n_spurious + n_take > len(eigvals):
        raise RuntimeError(
            f"requested {n_take} physical modes but only "
            f"{len(eigvals) - n_spurious} available after spurious filter"
        )
    physical = eigvals[n_spurious : n_spurious + n_take]
    physical_ks = np.array([lambda_to_k(lam) for lam in physical])
    q_factor = q_factor_from_lambda(physical[0])

    # σ₀ = 0 regression metric over the full returned slice.
    max_abs_re = max(np.max(np.abs(eigvals.real)), 1.0)
    max_imag_rel = float(np.max(np.abs(eigvals.imag)) / max_abs_re)

    # Complex-symmetry residual on the interior mass.
    M_int_T = M_int.T.tocsr()
    sym_diff = (M_int - M_int_T).tocoo()
    m_sym_residual = float(np.abs(sym_diff.data).max()) if sym_diff.nnz else 0.0

    return {
        "n_nodes": fixture.n_nodes,
        "n_tets": fixture.n_tets,
        "n_edges": int(edges.shape[0]),
        "n_interior_edges": n_interior_edges,
        "spurious_dim": int(spurious_dim),
        "n_spurious": n_spurious,
        "centroids": centroids,
        "epsilon_tensor_diag": eps_tensor_diag,
        "K_int": K_int,
        "M_int": M_int,
        "eigenvalues_all": eigvals,
        "physical_eigenvalues": physical,
        "physical_ks": physical_ks,
        "q_factor_lowest_physical": q_factor,
        "m_int_complex_symmetry_residual": m_sym_residual,
        "max_imag_eigval_rel": max_imag_rel,
        "sigma_0": float(sigma_0),
        "n_index": float(n_index),
        "k0_ref": float(k0_ref),
    }


# ---------------------------------------------------------------------------
# Autodiff probe through the TENSOR-valued complex-ε assembly path
# (Phase J.4 explicit research deliverable — closes the H.3 caveat)
# ---------------------------------------------------------------------------
#
# The differentiated parameter is the per-tet diagonal tensor
# ``eps_diag : (n_tets, 3)`` split into real and imaginary halves
# (`jax.grad` wants real leaves at the top of the chain; Wirtinger
# support is opt-in). Unlike the H.3 scalar probe — which precomputed
# the real mass blocks and differentiated only through the final
# `ε × M_real` broadcast — the tensor probe flows the gradient through
# the **per-axis complex kernel itself** (`_nedelec_mass_aniso_batch_jax`
# traced under grad), because the per-axis contraction cannot be
# factored out of the kernel. The scatter and loss functional are kept
# identical to H.3 (`tr(K_int) + |Tr(M_int)|²`, dense `.at[].add`
# scatter) so the two probe reports are directly comparable.


def _build_static_mie_topology(mesh_path=None) -> dict:
    """Precompute and freeze the mesh-derived topology for the probe."""
    if mesh_path is None:
        mesh_path = _SMALL_MSH_PATH

    fixture = read_sphere_fixture(mesh_path)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=R_BUFFER
    )
    return {
        "nodes": fixture.nodes,
        "tets": fixture.tets,
        "tet_physical_tags": fixture.tet_physical_tags,
        "edges": edges,
        "tet_edge_idx": tet_edge_idx,
        "tet_edge_sign": tet_edge_sign,
        "interior_indices": np.flatnonzero(interior_mask),
        "n_edges": int(edges.shape[0]),
        "n_tets": int(fixture.tets.shape[0]),
    }


def _make_tensor_assembly_loss(topo: dict):
    """Build ``loss(eps_re, eps_im) -> jnp.float64`` flowing through the
    tensor-valued complex assembly. ``eps_re``/``eps_im`` have shape
    ``(n_tets, 3)`` — the full diagonal tensor parameter axis."""
    n_tets = topo["n_tets"]
    n_edges = topo["n_edges"]
    tet_edge_idx = np.asarray(topo["tet_edge_idx"], dtype=np.int64)
    tet_edge_sign = np.asarray(topo["tet_edge_sign"], dtype=np.float64)
    coords_np = topo["nodes"][topo["tets"], :].astype(np.float64)
    int_idx = np.asarray(topo["interior_indices"], dtype=np.int32)

    coords_jax = jnp.asarray(coords_np)
    sign_outer_jax = jnp.asarray(
        tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :], dtype=jnp.float64
    )
    rows_jax = jnp.asarray(
        np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).reshape(-1),
        dtype=jnp.int32,
    )
    cols_jax = jnp.asarray(
        np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).reshape(-1),
        dtype=jnp.int32,
    )
    int_idx_jax = jnp.asarray(int_idx, dtype=jnp.int32)

    # Real curl-curl blocks: ε-independent, precomputed (no parameter
    # dependence — same hoist as the H.3 probe).
    k_local_real = _nedelec_cc_batch_jax(coords_jax)  # (n_tets, 6, 6)
    k_signed = (k_local_real * sign_outer_jax).reshape(-1)

    def loss(eps_re: jnp.ndarray, eps_im: jnp.ndarray) -> jnp.ndarray:
        """tr(K_int) + |Tr(M_int)|² — grad flows through the tensor kernel."""
        eps_c = eps_re.astype(jnp.complex128) + 1j * eps_im.astype(jnp.complex128)
        # THE tensor path: per-axis complex kernel traced under grad.
        m_local = _nedelec_mass_aniso_batch_jax(coords_jax, eps_c)  # (n_tets, 6, 6) c128
        m_signed = (m_local * sign_outer_jax.astype(jnp.complex128)).reshape(-1)

        m_global = jnp.zeros((n_edges, n_edges), dtype=jnp.complex128)
        m_global = m_global.at[rows_jax, cols_jax].add(m_signed)
        k_global = jnp.zeros((n_edges, n_edges), dtype=jnp.float64)
        k_global = k_global.at[rows_jax, cols_jax].add(k_signed)

        m_int = m_global[jnp.ix_(int_idx_jax, int_idx_jax)]
        k_int = k_global[jnp.ix_(int_idx_jax, int_idx_jax)]

        tr_m = jnp.trace(m_int)
        return jnp.trace(k_int) + (tr_m.real**2 + tr_m.imag**2)

    return loss


def probe_autodiff_tensor_assembly(
    mesh_path=None,
    sigma_0: float = SIGMA_0_DEFAULT,
    k0_ref: float = K0_REF,
) -> dict:
    """Phase J.4 explicit research deliverable: ``jax.grad`` through the
    tensor-valued (anisotropic UPML) complex-ε assembly path.

    Reports the same fields as the H.3 scalar probe
    (``sphere_pml.probe_autodiff_complex_assembly``): ``jit_ok``,
    ``loss_value``, ``grad_ok``, ``grad_finite``, ``grad_max_abs_re``,
    ``grad_max_abs_im``, ``errors``. Documentation-only — gradient
    values are not FD-cross-checked on the complex side (same Wirtinger
    rationale as H.3)."""
    if mesh_path is None:
        mesh_path = _SMALL_MSH_PATH

    topo = _build_static_mie_topology(mesh_path)
    centroids = tet_centroids(topo["nodes"], topo["tets"])
    eps_diag = build_anisotropic_pml_tensor_diag(
        topo["tet_physical_tags"],
        centroids,
        n_inside=N_INDEX,
        sigma_0=sigma_0,
        k0_ref=k0_ref,
    )  # (n_tets, 3) c128

    eps_re0 = jnp.asarray(eps_diag.real, dtype=jnp.float64)
    eps_im0 = jnp.asarray(eps_diag.imag, dtype=jnp.float64)

    findings: dict = {
        "sigma_0": float(sigma_0),
        "k0_ref": float(k0_ref),
        "n_tets": int(topo["n_tets"]),
        "param_shape": list(eps_diag.shape),
        "jit_ok": None,
        "loss_value": None,
        "grad_ok": None,
        "grad_max_abs_re": None,
        "grad_max_abs_im": None,
        "grad_finite": None,
        "errors": [],
    }

    try:
        loss = _make_tensor_assembly_loss(topo)
        loss_jit = jax.jit(loss)
        val = float(loss_jit(eps_re0, eps_im0))
        findings["jit_ok"] = True
        findings["loss_value"] = val
    except Exception as e:  # pragma: no cover — friction artifact path
        findings["jit_ok"] = False
        findings["errors"].append(f"jit loss raise: {type(e).__name__}: {e}")
        return findings

    try:
        grad_fn = jax.jit(jax.grad(loss, argnums=(0, 1)))
        g_re, g_im = grad_fn(eps_re0, eps_im0)
        g_re_np = np.asarray(g_re)
        g_im_np = np.asarray(g_im)
        finite = bool(np.all(np.isfinite(g_re_np)) and np.all(np.isfinite(g_im_np)))
        findings["grad_ok"] = True
        findings["grad_max_abs_re"] = float(np.max(np.abs(g_re_np)))
        findings["grad_max_abs_im"] = float(np.max(np.abs(g_im_np)))
        findings["grad_finite"] = finite
    except Exception as e:  # pragma: no cover — friction artifact path
        findings["grad_ok"] = False
        findings["errors"].append(f"grad raise: {type(e).__name__}: {e}")

    return findings


# ---------------------------------------------------------------------------
# CLI self-check
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(
        description="JAX anisotropic-UPML sphere-Mie self-check (Phase J.4)"
    )
    parser.add_argument(
        "--fixture",
        type=str,
        default=str(_SMALL_MSH_PATH),
        help="Path to the Gmsh .msh fixture (default: small 197-tet mesh)",
    )
    parser.add_argument("--sigma0", type=float, default=SIGMA_0_DEFAULT)
    parser.add_argument("--n-index", type=float, default=N_INDEX)
    parser.add_argument("--k0-ref", type=float, default=K0_REF)
    parser.add_argument("--n-take", type=int, default=5)
    parser.add_argument("--skip-autodiff", action="store_true")
    args = parser.parse_args()

    print(
        f"== JAX sphere-Mie (anisotropic UPML) self-check ==  "
        f"jax={jax.__version__}, f64={jax.config.read('jax_enable_x64')}, "
        f"backend={jax.default_backend()}, σ₀={args.sigma0}, k₀_ref={args.k0_ref}"
    )

    result = run_sphere_mie_jax(
        Path(args.fixture),
        sigma_0=args.sigma0,
        n_index=args.n_index,
        k0_ref=args.k0_ref,
        n_take=args.n_take,
    )

    print(f"\nMesh: {result['n_nodes']} nodes, {result['n_tets']} tets")
    print(
        f"PEC reduction: {result['n_edges']} edges -> "
        f"{result['n_interior_edges']} interior DOFs"
    )
    print(
        f"spurious (predicted / d0-rank): "
        f"{result['spurious_dim']} / {result['n_spurious']}"
    )
    print(
        f"max|Im|/max|Re| over slice = {result['max_imag_eigval_rel']:.3e}; "
        f"M complex-symmetry residual = "
        f"{result['m_int_complex_symmetry_residual']:.3e}"
    )

    roots = load_mie_roots_catalogue()
    table = classify_modes_against_catalogue(result["physical_ks"], roots)
    print(f"\nlowest {args.n_take} physical modes (λ = k²):")
    for i, (lam, k, row) in enumerate(
        zip(result["physical_eigenvalues"], result["physical_ks"], table)
    ):
        print(
            f"  [{i}] λ = {lam.real:+.6e} {lam.imag:+.6e}j  "
            f"k = {k.real:.5f} {k.imag:+.5f}j  ->  "
            f"{row['pol']}_{row['l']},{row['n']} "
            f"(analytic k = {row['analytic_k']:.5f}, "
            f"rel err = {row['rel_err'] * 100:.2f}%)"
        )
    print(f"\nQ of lowest physical mode: {result['q_factor_lowest_physical']:.4f}")

    if not args.skip_autodiff:
        print("\n== Autodiff probe: jax.grad through TENSOR-ε (UPML) assembly ==")
        findings = probe_autodiff_tensor_assembly(
            mesh_path=Path(args.fixture), sigma_0=args.sigma0, k0_ref=args.k0_ref
        )
        print(
            f"  σ₀ = {findings['sigma_0']}, n_tets = {findings['n_tets']}, "
            f"param shape = {findings['param_shape']} (full diagonal tensor)"
        )
        print(f"  jit_ok      = {findings['jit_ok']}")
        print(f"  loss_value  = {findings['loss_value']}")
        print(f"  grad_ok     = {findings['grad_ok']}")
        print(f"  grad_finite = {findings['grad_finite']}")
        print(f"  ||grad_re||_∞ = {findings['grad_max_abs_re']}")
        print(f"  ||grad_im||_∞ = {findings['grad_max_abs_im']}")
        print(f"  errors      = {findings['errors'] or '(none)'}")
        print(
            "  NOTE: gradient values are NOT cross-checked against FD; "
            "the probe is documentation only (Phase H.3 convention)."
        )

    print("\nSelf-check: DONE")
    sys.exit(0)
