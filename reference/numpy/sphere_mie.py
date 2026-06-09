"""NumPy reference for the anisotropic-UPML dielectric-sphere Mie pipeline.

Issue #171 (Epic #88, Phase J.2): mirrors the Burn end-to-end Mie
acceptance pipeline in ``crates/geode-core/tests/mie_sphere.rs`` —
dielectric sphere (``n = 1.5``, ``R_s = 1.0``, ``R_b = 2.0``) with the
**anisotropic UPML** (``σ₀ = 5.0``, ``k₀_ref = 2.0``, issue #54) — so
the two backends can be cross-checked sub-stage by sub-stage and both
can be anchored to the Phase J.1 analytic Mie-root catalogue
(``reference/fixtures/mie_roots/baseline.json``).

This file carries the anisotropic-UPML port that Phase H deferred:
:mod:`sphere_pml` (#146) cross-implemented the *scalar isotropic* PML
only. The Burn-side Mie acceptance depends on the anisotropic UPML to
get under the ~16 % scalar-PML reflection ceiling documented in issue
#49 (observed TM_1,1 error ≈ 5.7 % with UPML on the refined fixture).
A reference Mie slice at scalar-PML accuracy would be anchored to the
wrong physics, so the UPML port lands here.

Relationship to ``sphere_pml.py`` (Phase H.1)
=============================================

Everything except the constitutive piece is shared:

- Mesh I/O, edge enumeration, sign convention, PEC outer-wall mask, and
  the d⁰-rank spurious-mode classifier are imported verbatim from
  :mod:`sphere_pec` (exactly as :mod:`sphere_pml` does).
- The per-tet permittivity becomes a **diagonal complex tensor** (3
  complex entries per tet, global Cartesian basis) via
  :func:`build_anisotropic_pml_tensor_diag` — a line-for-line mirror of
  ``geode_core::build_anisotropic_pml_tensor_diag``.
- The local mass kernel becomes the per-axis split
  :func:`batched_nedelec_local_mass_anisotropic_diag` — mirror of
  ``geode_core::batched_nedelec_local_mass_anisotropic_diag``
  (``crates/geode-core/src/nedelec.rs``). The integrand
  ``N_iᵀ diag(ε_x, ε_y, ε_z) N_j`` is the scalar mass formula with the
  gradient gram ``G_pq = ∇λ_p · ∇λ_q`` replaced by the per-component
  product ``G^(α)_pq = (∇λ_p)_α (∇λ_q)_α``; summing the three per-axis
  matrices with equal weights recovers the scalar mass exactly (the
  natural isotropic-collapse regression).
- The generalized eigensolve is the dense LAPACK ZGGEV path
  (:func:`sphere_pml.eigensolve_complex_dense`) on the
  complex-symmetric pencil ``K x = λ M x``.

Diagonal-only UPML (exactness note)
===================================

The Burn kernel implements the *diagonal-only* simplification of the
full Sacks UPML tensor ``R · diag(1/s_r, s_t, s_t) · Rᵀ``. For the
current profile the radial and transverse stretches coincide
(``s_r = s_t = 1 − jσ(r)/ω``), so the full tensor's off-diagonals are
identically zero and the diagonal-only kernel is **exact** for this
profile — see the diagonal expansion in
``geode_core::build_anisotropic_pml_tensor_diag``'s docstring:

    ε_α = (1/s_r) r̂_α² + s_t (1 − r̂_α²),     α ∈ {x, y, z}.

Sign convention
===============

``exp(+jωt)`` throughout; outgoing absorption requires ``Im(ε) < 0``
in the shell, which the ramp produces. Q is reported in the
sign-agnostic k-space form ``Q = Re(k) / (2 |Im(k)|)``, ``k = √λ``
on the ``Re(k) ≥ 0`` branch, mirroring ``tests/mie_sphere.rs``.

Public API
==========

- :func:`tet_centroids(nodes, tets)` — per-tet centroid (vector, not
  just radius — the tensor builder needs the radial direction).
- :func:`build_anisotropic_pml_tensor_diag(...)` — per-tet diagonal
  complex permittivity tensor, mirror of the Burn function.
- :func:`batched_nedelec_local_mass_anisotropic_diag(coords, eps_diag)`
  — per-element complex 6×6 mass under the diagonal tensor.
- :func:`assemble_global_nedelec_anisotropic(...)` — global assembly;
  returns ``(K, M)`` complex CSR (K real-valued, typed complex).
- :func:`classify_modes_against_catalogue(ks, catalogue)` — nearest-
  root classification of FEM modes against the J.1 analytic catalogue.
- :func:`run_sphere_mie(...)` — end-to-end orchestrator, mirror of
  :func:`sphere_pml.run_sphere_pml`.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import scipy.sparse
import scipy.sparse.linalg

HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from nedelec_local_matrices import TET_LOCAL_EDGES, _cofactor_gram  # noqa: E402

# Reuse the Phase G/H mesh I/O, edge enumeration, PEC mask, and d⁰-rank
# classifier verbatim — this pipeline differs only in the constitutive
# scaling on the mass.
from sphere_pec import (  # noqa: E402
    PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR,
    R_BUFFER,
    R_PML_INNER,
    R_SPHERE,
    apply_dirichlet,
    build_edges,
    read_sphere_fixture,
    sphere_n_interior_nodes,
    sphere_pec_interior_edges,
    spurious_dim_from_derham,
)
from sphere_pml import eigensolve_complex_dense  # noqa: E402

# Reference wavenumber used by the anisotropic UPML stretching profile.
# Mirror of `K0_REF` in `crates/geode-core/tests/mie_sphere.rs` and
# `examples/mie_sphere.rs`.
K0_REF: float = 2.0

# Default PML absorption strength — the Burn-side Mie acceptance value.
SIGMA_0_DEFAULT: float = 5.0


# --------------------------------------------------------------------------- #
# Per-tet centroids — mirror of ``geode_core::tet_centroids``.
# --------------------------------------------------------------------------- #


def tet_centroids(nodes: np.ndarray, tets: np.ndarray) -> np.ndarray:
    """Per-tet centroid positions, shape ``(n_tets, 3)``.

    Companion to :func:`sphere_pml.tet_centroid_radii` for callers that
    need the full vector centroid — the anisotropic tensor builder
    needs the radial *direction*, not just its magnitude. Mirror of
    ``geode_core::tet_centroids``.
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    return nodes[tets, :].mean(axis=1)


# --------------------------------------------------------------------------- #
# Anisotropic UPML diagonal tensor — mirror of
# ``geode_core::build_anisotropic_pml_tensor_diag``.
# --------------------------------------------------------------------------- #


def build_anisotropic_pml_tensor_diag(
    physical_tags: np.ndarray,
    centroids: np.ndarray,
    n_inside: float = 1.5,
    sigma_0: float = SIGMA_0_DEFAULT,
    k0_ref: float = K0_REF,
) -> np.ndarray:
    """Per-tet **diagonal anisotropic** complex permittivity tensor.

    Line-for-line mirror of
    ``geode_core::build_anisotropic_pml_tensor_diag`` (issue #54):

    - Tet in ``sphere_interior``: real isotropic ``(n², n², n²)``.
    - Tet in ``vacuum_gap`` (any tag other than ``PHYS_PML_SHELL``), or
      a PML-shell tet whose centroid sits at ``r_c ≤ R_PML_INNER``:
      real isotropic ``(1, 1, 1)``.
    - Tet in ``pml_shell`` with ``r_c > R_PML_INNER``: the simplified
      Sacks UPML with ``s_r = s_t = s = 1 − jσ(r_c)/ω``,

      ```text
      σ(r_c) = σ₀ · clamp((r_c − R_PML_INNER) / (R_BUFFER − R_PML_INNER), 0, 1)²
      ε_α    = (1/s) r̂_α² + s (1 − r̂_α²),     r̂ = c / |c|.
      ```

    ``ω`` is approximated by ``k0_ref`` (``max(k0_ref, 1e-12)``), the
    reference-wavenumber heuristic shared with Silver-Müller.

    Sign convention: ``exp(+jωt)`` → ``Im(ε) < 0`` in the shell.

    Parameters
    ----------
    physical_tags : (n_tets,) int array
    centroids : (n_tets, 3) float array
    n_inside, sigma_0, k0_ref : floats — see the Burn docstring.

    Returns
    -------
    eps_diag : (n_tets, 3) complex128 — ``(ε_x, ε_y, ε_z)`` per tet.
    """
    tags = np.asarray(physical_tags, dtype=np.int32)
    centroids = np.asarray(centroids, dtype=np.float64)
    if centroids.ndim != 2 or centroids.shape[1] != 3:
        raise ValueError(f"expected centroids shape (n_tets, 3), got {centroids.shape}")
    if tags.shape[0] != centroids.shape[0]:
        raise ValueError(
            f"physical_tags and centroids length mismatch: "
            f"{tags.shape[0]} vs {centroids.shape[0]}"
        )

    n_tets = tags.shape[0]
    eps_inside = float(n_inside) * float(n_inside)
    width = R_BUFFER - R_PML_INNER
    omega = max(float(k0_ref), 1e-12)

    r_c = np.linalg.norm(centroids, axis=1)

    # Background scalar: n² in the dielectric, 1 elsewhere.
    eps_scalar = np.where(tags == PHYS_SPHERE_INTERIOR, eps_inside, 1.0)

    # Default: real isotropic (covers interior, vacuum gap, and the
    # defensive r_c <= R_PML_INNER guard inside the shell).
    eps_diag = np.empty((n_tets, 3), dtype=np.complex128)
    eps_diag[:, :] = eps_scalar[:, None].astype(np.complex128)

    # PML shell with centroid strictly past R_PML_INNER.
    in_shell = (tags == PHYS_PML_SHELL) & (r_c > R_PML_INNER)
    if np.any(in_shell):
        r_shell = r_c[in_shell]
        u = np.clip((r_shell - R_PML_INNER) / width, 0.0, 1.0)
        sigma = sigma_0 * u * u
        s = 1.0 + 1j * (-sigma / omega)  # s_r = s_t = 1 - jσ/ω
        s_inv = 1.0 / s

        # Radial unit vector at the centroid (guarded |c| ≈ 0, matching
        # the Burn-side defensive branch).
        inv_r = np.where(r_shell > 1e-12, 1.0 / r_shell, 0.0)
        r_hat = centroids[in_shell, :] * inv_r[:, None]  # (n_shell, 3)

        # ε_α = bg · (s_inv r̂_α² + s (1 − r̂_α²))
        bg = eps_scalar[in_shell].astype(np.complex128)
        w = r_hat * r_hat  # r̂_α², shape (n_shell, 3)
        eps_diag[in_shell, :] = bg[:, None] * (
            s_inv[:, None] * w + s[:, None] * (1.0 - w)
        )

    return eps_diag


# --------------------------------------------------------------------------- #
# Anisotropic local mass — mirror of
# ``geode_core::batched_nedelec_local_mass_anisotropic_diag``.
# --------------------------------------------------------------------------- #


def batched_nedelec_local_mass_anisotropic_diag(
    coords: np.ndarray, eps_diag: np.ndarray
) -> np.ndarray:
    """Per-element Nédélec local mass under a diagonal permittivity tensor.

    The integrand ``N_iᵀ · diag(ε_x, ε_y, ε_z) · N_j`` splits per axis:
    the scalar mass formula (``nedelec_local_matrices.py`` eq. M) holds
    per component with the cofactor gram ``gg_pq = g_p · g_q`` replaced
    by the per-axis product ``gg^(α)_pq = g_p[α] g_q[α]``:

    ```text
    M_ij = Σ_α ε_α / (120 |det|) [  (1+δ_ac) gg^(α)_bd − (1+δ_ad) gg^(α)_bc
                                  − (1+δ_bc) gg^(α)_ad + (1+δ_bd) gg^(α)_ac ]
    ```

    Since ``Σ_α gg^(α)_pq = gg_pq``, equal weights ``ε_x = ε_y = ε_z = ε``
    collapse this to exactly ``ε ×`` the scalar mass — the natural
    isotropic-collapse regression exercised by the σ₀ = 0 tests.

    Mirror of ``geode_core::batched_nedelec_local_mass_anisotropic_diag``
    (``crates/geode-core/src/nedelec.rs``). Same orientation caveat as
    the scalar kernel: all local edge signs are ``+1``; the global
    ``s_i s_j`` correction is the caller's responsibility.

    Parameters
    ----------
    coords : (n_elem, 4, 3) float64
    eps_diag : (n_elem, 3) complex128 (or anything castable)

    Returns
    -------
    m_local : (n_elem, 6, 6) complex128
    """
    coords = np.asarray(coords, dtype=np.float64)
    eps_diag = np.asarray(eps_diag, dtype=np.complex128)
    if coords.ndim != 3 or coords.shape[1:] != (4, 3):
        raise ValueError(f"expected coords shape (n_elem, 4, 3), got {coords.shape}")
    n_elem = coords.shape[0]
    if eps_diag.shape != (n_elem, 3):
        raise ValueError(
            f"expected eps_diag shape ({n_elem}, 3), got {eps_diag.shape}"
        )

    # Reuse the cofactor machinery from the scalar kernel; we need the
    # raw g_p vectors per axis, so recompute the small geometric pieces
    # here (same formulas as `_cofactor_gram`, kept in lockstep).
    v0 = coords[:, 0, :]
    v1 = coords[:, 1, :]
    v2 = coords[:, 2, :]
    v3 = coords[:, 3, :]
    e1 = v1 - v0
    e2 = v2 - v0
    e3 = v3 - v0
    g1 = np.cross(e2, e3)
    g2 = np.cross(e3, e1)
    g3 = np.cross(e1, e2)
    g0 = -(g1 + g2 + g3)
    det = np.einsum("ij,ij->i", e1, g1)
    g_mat = np.stack([g0, g1, g2, g3], axis=1)  # (n_elem, 4, 3)

    abs_det = np.abs(det)
    inv_abs_det = 1.0 / abs_det

    # Per-axis cofactor gram: gg_axis[e, α, p, q] = g_p[α] g_q[α].
    # (Σ_α gg_axis[:, α] == _cofactor_gram(coords)[0] — the scalar gram.)
    gg_axis = np.einsum("epa,eqa->eapq", g_mat, g_mat)  # (n_elem, 3, 4, 4)

    m_local = np.zeros((n_elem, 6, 6), dtype=np.complex128)
    for i, (a, b) in enumerate(TET_LOCAL_EDGES):
        for j, (c, d) in enumerate(TET_LOCAL_EDGES):
            f_ac = 2.0 if a == c else 1.0
            f_ad = 2.0 if a == d else 1.0
            f_bc = 2.0 if b == c else 1.0
            f_bd = 2.0 if b == d else 1.0
            # Per-axis Kronecker-lifted term, shape (n_elem, 3).
            m_term = (
                f_ac * gg_axis[:, :, b, d]
                - f_ad * gg_axis[:, :, b, c]
                - f_bc * gg_axis[:, :, a, d]
                + f_bd * gg_axis[:, :, a, c]
            )
            # Weight per axis by ε_α and sum over α.
            m_local[:, i, j] = (
                np.einsum("ea,ea->e", m_term.astype(np.complex128), eps_diag)
                * inv_abs_det
                / 120.0
            )

    return m_local


# --------------------------------------------------------------------------- #
# Global assembly with the diagonal tensor ε.
# --------------------------------------------------------------------------- #


def assemble_global_nedelec_anisotropic(
    nodes: np.ndarray,
    tets: np.ndarray,
    edges: np.ndarray,
    tet_edge_idx: np.ndarray,
    tet_edge_sign: np.ndarray,
    eps_tensor_diag: np.ndarray,
):
    """Assemble global Nédélec stiffness K (real) and tensor-ε complex mass M.

    Mirror of ``geode_core::assemble_global_nedelec_with_anisotropic_epsilon``
    on the host side: the per-element [6×6] curl-curl is real and
    permittivity-independent; the per-element mass picks up the diagonal
    tensor ε via :func:`batched_nedelec_local_mass_anisotropic_diag` and
    the global scatter is performed once into a complex CSR matrix.

    K is returned as ``complex128`` (with ``Im(K) = 0`` identically) so
    the downstream eigensolve sees a uniform dtype on the pencil —
    matching the :func:`sphere_pml.assemble_global_nedelec_complex`
    convention.

    Returns
    -------
    K : scipy.sparse.csr_matrix complex128, shape ``(n_edges, n_edges)``
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

    # Stiffness: reuse the scalar kernel (curl-curl is ε-independent).
    gg, det, _sv = _cofactor_gram(coords)
    abs_det = np.abs(det)
    inv_abs_det3 = 1.0 / (abs_det * abs_det * abs_det)
    k_local = np.zeros((n_tets, 6, 6), dtype=np.float64)
    for i, (a, b) in enumerate(TET_LOCAL_EDGES):
        for j, (c, d) in enumerate(TET_LOCAL_EDGES):
            k_local[:, i, j] = (
                (2.0 / 3.0)
                * (gg[:, a, c] * gg[:, b, d] - gg[:, a, d] * gg[:, b, c])
                * inv_abs_det3
            )

    # Mass: anisotropic complex kernel.
    m_local = batched_nedelec_local_mass_anisotropic_diag(coords, eps_tensor_diag)

    # Per-tet sign outer product s_i * s_j (real-valued).
    sign_outer = tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :]
    k_signed = k_local * sign_outer
    m_signed = m_local * sign_outer

    rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).reshape(-1)
    cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).reshape(-1)
    k_vals = k_signed.reshape(-1).astype(np.complex128)
    m_vals = m_signed.reshape(-1)

    K = scipy.sparse.coo_matrix(
        (k_vals, (rows, cols)), shape=(n_edges, n_edges), dtype=np.complex128
    ).tocsr()
    M = scipy.sparse.coo_matrix(
        (m_vals, (rows, cols)), shape=(n_edges, n_edges), dtype=np.complex128
    ).tocsr()
    return K, M


# --------------------------------------------------------------------------- #
# λ → k and Q helpers (principal branch, sign-agnostic Q).
# --------------------------------------------------------------------------- #


def lambda_to_k(lam: complex) -> complex:
    """``k = √λ`` on the principal branch ``Re(k) ≥ 0``, with
    ``sign(Im(k)) = sign(Im(λ))`` — mirror of the λ → k conversion in
    ``crates/geode-core/tests/mie_sphere.rs``."""
    lam = complex(lam)
    r = abs(lam)
    re_k = np.sqrt(max(0.5 * (r + lam.real), 0.0))
    im_k_mag = np.sqrt(max(0.5 * (r - lam.real), 0.0))
    im_k = im_k_mag if lam.imag >= 0.0 else -im_k_mag
    return complex(re_k, im_k)


def q_factor_from_lambda(lam: complex) -> float:
    """Sign-agnostic ``Q = Re(k) / (2 |Im(k)|)`` for ``k = √λ``."""
    k = lambda_to_k(lam)
    if abs(k.imag) > 1e-12:
        return float(k.real / (2.0 * abs(k.imag)))
    return float("inf")


# --------------------------------------------------------------------------- #
# Mode classification against the J.1 analytic catalogue.
# --------------------------------------------------------------------------- #


def load_mie_roots_catalogue(catalogue_path=None) -> list[dict]:
    """Load the Phase J.1 analytic Mie-root catalogue
    (``reference/fixtures/mie_roots/baseline.json``) into a list of
    ``{"pol": "TE"|"TM", "l": int, "n": int, "multiplicity": int,
    "k": float}`` dicts sorted by ``k`` ascending."""
    if catalogue_path is None:
        catalogue_path = (
            HERE.parent / "fixtures" / "mie_roots" / "baseline.json"
        )
    with open(catalogue_path) as f:
        fixture = json.load(f)
    out = fixture["outputs"]
    pols = out["root_pol"]["data"]
    ls = out["root_l"]["data"]
    ns = out["root_n"]["data"]
    mults = out["root_multiplicity"]["data"]
    ks = out["root_k"]["data"]
    roots = [
        {
            "pol": "TM" if int(round(p)) == 1 else "TE",
            "l": int(round(l)),
            "n": int(round(n)),
            "multiplicity": int(round(m)),
            "k": float(k),
        }
        for p, l, n, m, k in zip(pols, ls, ns, mults, ks)
    ]
    roots.sort(key=lambda r: r["k"])
    return roots


def classify_modes_against_catalogue(
    physical_ks: np.ndarray, roots: list[dict]
) -> list[dict]:
    """Nearest-root classification of FEM modes against the analytic
    catalogue.

    For each FEM ``Re(k)``, find the catalogue root minimizing the
    relative error ``|Re(k) − k_root| / k_root`` and report the
    assignment. This is deliberately simple — the cross-backend
    acceptance only leans on the *lowest* mode's TM_1,1 assignment; the
    rest of the table is diagnostic.
    """
    out = []
    for k_fem in np.atleast_1d(physical_ks):
        re_k = float(np.real(k_fem))
        best = min(roots, key=lambda r: abs(re_k - r["k"]) / r["k"])
        out.append(
            {
                "fem_re_k": re_k,
                "pol": best["pol"],
                "l": best["l"],
                "n": best["n"],
                "analytic_k": best["k"],
                "rel_err": abs(re_k - best["k"]) / best["k"],
            }
        )
    return out


# --------------------------------------------------------------------------- #
# End-to-end driver.
# --------------------------------------------------------------------------- #


def run_sphere_mie(
    mesh_path,
    sigma_0: float = SIGMA_0_DEFAULT,
    n_index: float = 1.5,
    k0_ref: float = K0_REF,
    n_take: int = 5,
    r_outer: float = R_BUFFER,
):
    """Full anisotropic-UPML Mie pipeline; returns a dict for
    cross-backend comparison. Mirror of :func:`sphere_pml.run_sphere_pml`
    with the constitutive piece swapped for the diagonal UPML tensor.

    Returns
    -------
    dict with the sub-stage outputs needed by the cross-backend test:
        - ``n_nodes``, ``n_tets``, ``n_edges``, ``n_interior_edges``,
          ``spurious_dim``, ``n_spurious`` : shape / classifier diagnostics
        - ``centroids`` : per-tet centroid positions (tensor input)
        - ``epsilon_tensor_diag`` : per-tet diagonal tensor, ``(n_tets, 3)``
        - ``K_int``, ``M_int`` : interior complex matrices (post-Dirichlet)
        - ``eigenvalues_all`` : lowest ``spurious_dim + 8`` eigenvalues,
          sorted by ``|Re(λ)|`` ascending (Burn FaerComplexEigensolver order)
        - ``physical_eigenvalues`` : lowest ``n_take`` complex
          eigenvalues past the d⁰-rank spurious split
        - ``physical_ks`` : ``√λ`` (principal branch) per physical mode
        - ``q_factor_lowest_physical`` : sign-agnostic Q of the lowest mode
        - ``m_int_complex_symmetry_residual`` : ``max|M − Mᵀ|`` (the
          tensor-ε mass stays complex-symmetric — the diagonal tensor
          weights symmetric per-axis blocks)
        - ``max_imag_eigval_rel`` : σ₀ = 0 regression metric
    """
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

    K, M = assemble_global_nedelec_anisotropic(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        eps_tensor_diag,
    )
    K_int, M_int = apply_dirichlet(K, M, interior_mask)

    # d⁰-rank spurious classifier — invariant under the tensor-ε mass
    # scaling (gradients of H¹₀ sit in the curl-curl kernel independent
    # of the constitutive law on the mass).
    n_spurious = int(
        spurious_dim_from_derham(fixture.nodes, edges, interior_mask, r_outer=r_outer)
    )

    n_request = spurious_dim + 8
    eigvals = eigensolve_complex_dense(K_int, M_int, k_take=n_request)

    if n_spurious + n_take > len(eigvals):
        raise RuntimeError(
            f"requested {n_take} physical modes but only "
            f"{len(eigvals) - n_spurious} available after spurious filter; "
            f"increase n_request"
        )
    physical = eigvals[n_spurious : n_spurious + n_take]
    physical_ks = np.array([lambda_to_k(lam) for lam in physical])
    q_factor = q_factor_from_lambda(physical[0])

    # σ₀ = 0 regression metric over the full returned slice.
    max_abs_re = max(np.max(np.abs(eigvals.real)), 1.0)
    max_imag_rel = float(np.max(np.abs(eigvals.imag)) / max_abs_re)

    # Complex-symmetry residual on the interior mass — the diagonal
    # tensor weights per-axis blocks that are individually symmetric in
    # (i, j), so M must stay complex-symmetric (not Hermitian).
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
        "nodes": fixture.nodes,
        "tets": fixture.tets,
        "tet_physical_tags": fixture.tet_physical_tags,
        "centroids": centroids,
        "epsilon_tensor_diag": eps_tensor_diag,
        "edges": edges,
        "tet_edge_idx": tet_edge_idx,
        "tet_edge_sign": tet_edge_sign,
        "interior_mask": interior_mask,
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


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--fixture",
        default=str(
            HERE.parent / "fixtures" / "sphere_pml_small" / "sphere.msh"
        ),
        help="Path to the Gmsh .msh fixture (default: small mesh).",
    )
    parser.add_argument("--sigma0", type=float, default=SIGMA_0_DEFAULT)
    parser.add_argument("--n-index", type=float, default=1.5)
    parser.add_argument("--k0-ref", type=float, default=K0_REF)
    parser.add_argument("--n-take", type=int, default=5)
    args = parser.parse_args()

    result = run_sphere_mie(
        args.fixture,
        sigma_0=args.sigma0,
        n_index=args.n_index,
        k0_ref=args.k0_ref,
        n_take=args.n_take,
    )

    print(f"sphere fixture: {result['n_nodes']} nodes, {result['n_tets']} tets")
    print(
        f"PEC reduction: {result['n_edges']} edges -> "
        f"{result['n_interior_edges']} interior DOFs"
    )
    print(f"spurious (predicted / d0-rank): "
          f"{result['spurious_dim']} / {result['n_spurious']}")
    print(
        f"sigma_0 = {result['sigma_0']}, k0_ref = {result['k0_ref']}; "
        f"max|Im|/max|Re| over slice = {result['max_imag_eigval_rel']:.3e}"
    )
    print(f"M complex-symmetry residual: "
          f"{result['m_int_complex_symmetry_residual']:.3e}")
    print()
    roots = load_mie_roots_catalogue()
    table = classify_modes_against_catalogue(
        result["physical_ks"], roots
    )
    print(f"lowest {args.n_take} physical modes (lambda = k^2):")
    for i, (lam, k, row) in enumerate(
        zip(result["physical_eigenvalues"], result["physical_ks"], table)
    ):
        print(
            f"  [{i}] lambda = {lam.real:+.6e} {lam.imag:+.6e}j  "
            f"k = {k.real:.5f} {k.imag:+.5f}j  ->  "
            f"{row['pol']}_{row['l']},{row['n']} "
            f"(analytic k = {row['analytic_k']:.5f}, "
            f"rel err = {row['rel_err'] * 100:.2f}%)"
        )
    print()
    print(
        f"Q of lowest physical mode: {result['q_factor_lowest_physical']:.4f}"
    )
