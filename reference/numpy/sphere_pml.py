"""NumPy reference for the scalar-isotropic-PML sphere eigenmode pipeline.

Issue #146 (Epic #88, Phase H.1): mirrors the Burn pipeline in
``crates/geode-core/tests/sphere_pml_eigenmode.rs`` so the two backends
can be cross-checked sub-stage by sub-stage with the **complex** ε and
**complex generalized** eigensolve introduced in Phase H.

This is the canonical-tiebreaker reference for Phase H — the Julia
(#147) and JAX (#148) ports will cross-check against the baseline this
file produces.

Relationship to ``sphere_pec.py`` (Phase G.2)
=============================================

The PEC and PML pipelines share **everything but the constitutive
piece**:

- Mesh I/O, edge enumeration, sign convention, PEC outer-wall mask, and
  the d⁰-rank spurious-mode classifier are imported verbatim from
  :mod:`sphere_pec`. No re-implementation, no duplication.
- The per-tet permittivity becomes **complex-valued** via
  :func:`build_complex_epsilon_r_pml` (mirrors
  ``geode_core::build_complex_epsilon_r_pml`` line-for-line).
- The global mass scatter is performed with ``complex128`` instead of
  ``float64`` (stiffness K stays real-valued; only the ε-scaled mass
  picks up the lossy imaginary content).
- The generalized eigensolve uses :func:`scipy.sparse.linalg.eigs`
  (non-Hermitian — the complex-ε mass is complex-symmetric but not
  Hermitian) with shift-and-invert at ``sigma=0+0j`` to recover the
  near-zero spurious cluster plus the lowest few physical modes.

Sign convention recap
=====================

We use the ``exp(+jωt)`` convention throughout. The PML profile produces
``Im(ε_r) < 0`` in the absorbing shell. The complex-symmetric pencil
``K x = λ M x`` admits eigenvalues with either sign of ``Im(λ)`` (no
enforced conjugation), so the Q-factor is reported in the sign-
agnostic k-space form

    Q = Re(k) / (2 |Im(k)|)      where k = sqrt(λ), Re(k) ≥ 0

which gives a positive Q for any absorbing mode regardless of which
imaginary-axis sign the eigenvector phase happens to pick. This
mirrors the Burn-side print convention in
``crates/geode-core/tests/sphere_pml_eigenmode.rs`` (lines 364-372).

σ₀ = 0 regression
=================

When the PML strength ``σ₀`` is zero, the complex permittivity is
identically real (and exactly equal to the PEC profile, modulo the
vacuum gap being labelled `PHYS_VACUUM_GAP` instead of folded in with
the PML shell), so:

- :func:`build_complex_epsilon_r_pml` returns ``Im(ε) = 0`` everywhere
  (bit-exact, no roundoff).
- The complex eigensolve produces eigenvalues that are real to
  numerical precision (``|Im(λ)| / max(|Re(λ)|, 1) < 1e-10``).
- The lowest physical eigenvalues match the Phase G PEC reference
  (``sphere_pec.run_sphere_pec``) bit-for-bit at the lowest band and
  to ``1e-10`` absolute on the spurious cluster.

The :func:`run_sphere_pml` orchestrator's ``--sigma0 0`` invocation is
the natural tiebreaker for any divergence between this reference and
the Burn-side ``pml_isotropic_sigma_zero_is_real`` integration test.

Public API
==========

- :func:`build_complex_epsilon_r_pml(tags, radii, n_inside, sigma_0)` —
  per-tet complex relative permittivity, matching the Burn function.
- :func:`tet_centroid_radii(nodes, tets)` — per-tet centroid radius
  (the geometric input to the PML profile, mirror of
  ``geode_core::tet_centroid_radii``).
- :func:`assemble_global_nedelec_complex(...)` — assembly with a
  complex-valued ε; returns ``(K, M)`` as complex CSR matrices (K is
  real-valued but stored as ``complex128`` for downstream uniformity).
- :func:`eigensolve_complex(K, M, k_request)` — lowest-k complex
  generalized eigenpairs.
- :func:`run_sphere_pml(mesh_path, sigma_0, n_index, n_take, r_outer)` —
  orchestrator; mirror of :func:`sphere_pec.run_sphere_pec`.
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import scipy.linalg
import scipy.sparse
import scipy.sparse.linalg

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

from reference.numpy.nedelec_local_matrices import batched_nedelec_local_matrices  # noqa: E402

# Reuse the Phase G mesh I/O, edge enumeration, PEC mask, and d⁰-rank
# classifier verbatim — the PML problem differs only in the constitutive
# scaling on the mass.
from reference.numpy.sphere_pec import (  # noqa: E402
    PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR,
    R_BUFFER,
    R_PML_INNER,
    R_SPHERE,
    SphereFixture,
    apply_dirichlet,
    build_edges,
    read_sphere_fixture,
    sphere_n_interior_nodes,
    sphere_pec_interior_edges,
    spurious_dim_from_derham,
)


# --------------------------------------------------------------------------- #
# Per-tet centroid radius — mirror of ``geode_core::tet_centroid_radii``.
# --------------------------------------------------------------------------- #


def tet_centroid_radii(nodes: np.ndarray, tets: np.ndarray) -> np.ndarray:
    """Per-tet centroid distance from the origin.

    Used by :func:`build_complex_epsilon_r_pml` to decide which tets
    sit in the absorbing shell and how strongly to absorb in each.
    Mirror of ``geode_core::tet_centroid_radii``.
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    # centroid = mean of the 4 vertex positions → shape (n_tets, 3).
    centroids = nodes[tets, :].mean(axis=1)
    return np.linalg.norm(centroids, axis=1)


# --------------------------------------------------------------------------- #
# Complex permittivity profile — mirror of
# ``geode_core::build_complex_epsilon_r_pml``.
# --------------------------------------------------------------------------- #


def build_complex_epsilon_r_pml(
    physical_tags: np.ndarray,
    centroid_radii: np.ndarray,
    n_inside: float = 1.5,
    sigma_0: float = 5.0,
) -> np.ndarray:
    """Per-tet **complex** relative permittivity realizing the scalar PML.

    Profile (matches ``geode_core::build_complex_epsilon_r_pml``):

    - Tet in ``sphere_interior`` (tag ``PHYS_SPHERE_INTERIOR``):
      ``ε = n_inside² + 0j`` (real dielectric).
    - Tet in ``vacuum_gap`` (any other tag except ``PHYS_PML_SHELL``):
      ``ε = 1 + 0j`` (real vacuum).
    - Tet in ``pml_shell`` (tag ``PHYS_PML_SHELL``): quadratic absorption
      ramp anchored at ``R_PML_INNER``,

      ```text
      ε(r) = 1 − j σ₀ ((r − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²
      ```

    The ramp coordinate ``u`` is clamped to ``[0, 1]`` so a tet whose
    centroid drifts slightly outside ``R_BUFFER`` (e.g. due to coarse
    surface meshing) does not overshoot.

    Sign convention: ``exp(+jωt)`` → outgoing-wave attenuation requires
    ``Im(ε) < 0``, which is what the ramp produces. See
    ``geode_core::build_complex_epsilon_r_pml`` docstring for the
    σ₀-tuning rule of thumb.

    Parameters
    ----------
    physical_tags : (n_tets,) int array
        Per-tet 3D physical-group tag (matches the
        ``f.tet_physical_tags`` field from :func:`read_sphere_fixture`).
    centroid_radii : (n_tets,) float array
        Per-tet centroid distance from the origin.
    n_inside : float
        Refractive index inside the dielectric sphere.
    sigma_0 : float
        PML absorption strength at ``r = R_BUFFER``. ``σ₀ = 0`` reduces
        the profile to the real PEC ε (the natural tiebreaker test).

    Returns
    -------
    eps : (n_tets,) complex128
    """
    tags = np.asarray(physical_tags, dtype=np.int32)
    radii = np.asarray(centroid_radii, dtype=np.float64)
    if tags.shape != radii.shape:
        raise ValueError(
            f"physical_tags and centroid_radii length mismatch: "
            f"{tags.shape} vs {radii.shape}"
        )

    eps = np.ones_like(radii, dtype=np.complex128)
    eps_inside = float(n_inside) * float(n_inside)
    width = R_BUFFER - R_PML_INNER

    # Dielectric interior: ε = n² + 0j.
    is_interior = tags == PHYS_SPHERE_INTERIOR
    eps[is_interior] = complex(eps_inside, 0.0)

    # PML shell: quadratic ramp with clamped u ∈ [0, 1].
    is_pml = tags == PHYS_PML_SHELL
    u = np.clip((radii[is_pml] - R_PML_INNER) / width, 0.0, 1.0)
    eps[is_pml] = 1.0 + 1j * (-sigma_0 * u * u)

    # Vacuum gap (anything not in the dielectric and not in the PML shell)
    # stays at the default ε = 1 + 0j set by np.ones_like above.

    return eps


# --------------------------------------------------------------------------- #
# Global assembly with a complex-valued ε.
# --------------------------------------------------------------------------- #


def assemble_global_nedelec_complex(
    nodes: np.ndarray,
    tets: np.ndarray,
    edges: np.ndarray,
    tet_edge_idx: np.ndarray,
    tet_edge_sign: np.ndarray,
    epsilon_r_complex: np.ndarray,
):
    """Assemble global Nédélec stiffness K (real) and complex ε-scaled mass M.

    Mirror of ``geode_core::assemble_global_nedelec_with_complex_epsilon``
    on the host side: the per-element [6×6] curl-curl is real; the
    per-element mass picks up the complex ε scaling and the global
    scatter is performed once into a complex CSR matrix.

    K is returned as ``complex128`` (with ``Im(K) = 0`` identically) so
    the downstream eigensolve sees a uniform dtype on the pencil. This
    matches the Burn-side convention (the complex pencil is
    ``(K_complex, M_complex)`` where K_complex is real-valued but
    typed complex).

    Returns
    -------
    K : scipy.sparse.csr_matrix complex128, shape ``(n_edges, n_edges)``
    M : scipy.sparse.csr_matrix complex128, shape ``(n_edges, n_edges)``
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    tet_edge_idx = np.asarray(tet_edge_idx, dtype=np.int64)
    tet_edge_sign = np.asarray(tet_edge_sign, dtype=np.float64)
    epsilon_r_complex = np.asarray(epsilon_r_complex, dtype=np.complex128)
    n_tets = tets.shape[0]
    n_edges = int(edges.shape[0])

    coords = nodes[tets, :]  # (n_tets, 4, 3)
    k_local, m_local, _ = batched_nedelec_local_matrices(coords)

    # Per-tet sign outer product s_i * s_j (real-valued).
    sign_outer = tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :]
    k_signed = k_local * sign_outer
    # Mass: scale by per-tet complex ε before applying sign outer product
    # (the two scalings commute since both are diagonal in tet-index).
    m_signed = (
        m_local.astype(np.complex128) * sign_outer * epsilon_r_complex[:, None, None]
    )

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
# Complex generalized eigensolve.
# --------------------------------------------------------------------------- #


def eigensolve_complex_dense(K, M, k_take: int):
    """Lowest-``k_take`` complex generalized eigenvalues of ``K x = λ M x``
    via the **dense** ``scipy.linalg.eig`` (LAPACK ZGGEV).

    This is the canonical-tiebreaker path for the PML pencil. The
    ARPACK shift-and-invert path (``scipy.sparse.linalg.eigs(...,
    sigma=0+0j)``) is **not usable** here: the curl-curl operator K
    carries a large gradient kernel (~368 DOFs on the bundled 774-node
    fixture), so the shift-invert factor ``(K − 0·M)^{-1}`` is on a
    near-singular operator and produces numerical garbage. Shifting to
    a non-singular σ in the physical band works for finding modes
    *near* that σ but biases the selection. The dense LAPACK path is
    O(n³) but it sees the *entire* spectrum, so the cluster + lowest
    physical band can be sliced deterministically.

    Why ``scipy.linalg.eig`` and not ``eigsh``: the complex-ε mass is
    **complex-symmetric** (``M = M^T``) but **not Hermitian**
    (``M ≠ M^*``). The Hermitian Lanczos guarantees of ``eigsh`` do
    not apply; the general non-Hermitian QZ algorithm is the correct
    choice. On the bundled 3300-DOF interior matrix this takes ~30 min
    single-threaded.

    Returns
    -------
    eigvals : (k_take,) complex128, sorted by ``|Re(λ)|`` ascending.
        Sorted to match the Burn-side ``FaerComplexEigensolver``
        ordering (``crates/geode-core/src/complex_eigen.rs:171-179``).
    """
    K_dense = np.asarray(K.toarray(), dtype=np.complex128)
    M_dense = np.asarray(M.toarray(), dtype=np.complex128)

    eigvals = scipy.linalg.eigvals(K_dense, M_dense)

    # LAPACK ZGGEV returns the eigenvalues as ratios ``α / β``; when
    # ``β ≈ 0`` it emits ``inf`` / ``nan`` for the infinite-eigenvalue
    # tokens. Filter these — they correspond to the singular part of
    # the pencil, not physical modes.
    finite = np.isfinite(eigvals.real) & np.isfinite(eigvals.imag)
    eigvals = eigvals[finite]

    # Sort by |Re(λ)| ascending to match Burn's
    # `FaerComplexEigensolver` ordering. This puts the near-zero
    # spurious cluster at the front of the returned slice.
    order = np.argsort(np.abs(eigvals.real))
    eigvals = eigvals[order]

    take = int(min(k_take, eigvals.shape[0]))
    return eigvals[:take]


# --------------------------------------------------------------------------- #
# End-to-end driver.
# --------------------------------------------------------------------------- #


def run_sphere_pml(
    mesh_path,
    sigma_0: float = 5.0,
    n_index: float = 1.5,
    n_take: int = 5,
    r_outer: float = R_BUFFER,
):
    """Full sphere-PML pipeline; returns a dict for cross-backend
    comparison.

    Parameters
    ----------
    mesh_path : str or Path
        Path to the bundled Gmsh `.msh` fixture
        (``reference/fixtures/sphere_pml/sphere.msh``).
    sigma_0 : float
        PML absorption strength at ``r = R_BUFFER``. The Burn-side
        integration test uses ``5.0``; the σ₀ = 0 regression case
        passes ``0.0``.
    n_index : float
        Dielectric refractive index inside ``r ≤ R_SPHERE``.
    n_take : int
        Number of *physical* eigenvalues to return after spurious
        filtering. The eigensolve fetches ``spurious_dim + 8`` so the
        physical band has headroom regardless of the exact spurious
        count observed.
    r_outer : float
        Outer PEC wall radius used for the Dirichlet mask. Defaults to
        :data:`R_BUFFER` from the bundled fixture.

    Returns
    -------
    dict with all sub-stage outputs needed by the cross-backend test:
        - ``n_nodes``, ``n_tets``, ``n_edges``, ``n_interior_edges``,
          ``spurious_dim``, ``n_spurious`` : shape / classifier diagnostics
        - ``nodes``, ``tets``, ``tet_physical_tags`` : the loaded mesh
        - ``centroid_radii`` : per-tet centroid radius (PML profile input)
        - ``epsilon_r_complex`` : per-tet complex ε (length ``n_tets``)
        - ``edges``, ``tet_edge_idx``, ``tet_edge_sign`` : edge tables
        - ``interior_mask`` : boolean edge mask (PEC removed)
        - ``K``, ``M`` : full assembled matrices (complex CSR, pre-Dirichlet)
        - ``K_int``, ``M_int`` : interior matrices (post-Dirichlet)
        - ``eigenvalues_all`` : raw lowest spectrum slice (complex128,
          length ``spurious_dim + 8``)
        - ``physical_eigenvalues`` : lowest ``n_take`` complex
          eigenvalues past the spurious cluster, ordered by |Re(λ)|.
        - ``q_factor_lowest_physical`` : ``Re(k) / (2 |Im(k)|)`` for
          ``k = sqrt(λ)`` of the lowest physical mode (sanity
          diagnostic; sign-agnostic to match the Burn-side print).
        - ``k_int_frobenius``, ``m_int_frobenius`` : Frobenius norms
        - ``m_int_complex_symmetry_residual`` : ``max(|M - M^T|)`` on
          the interior mass; the complex-ε mass is complex-symmetric
          (not Hermitian), so this should be at f64 roundoff.
        - ``max_imag_eigval_rel`` : ``max|Im(λ)| / max(|Re(λ)|, 1)``
          over the full spectrum (the σ₀=0 regression metric).
    """
    fixture = read_sphere_fixture(mesh_path)
    centroid_radii = tet_centroid_radii(fixture.nodes, fixture.tets)
    epsilon_r_complex = build_complex_epsilon_r_pml(
        fixture.tet_physical_tags,
        centroid_radii,
        n_inside=n_index,
        sigma_0=sigma_0,
    )
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=r_outer
    )
    n_interior_edges = int(np.sum(interior_mask))
    spurious_dim = sphere_n_interior_nodes(fixture.nodes, r_outer=r_outer)

    K, M = assemble_global_nedelec_complex(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        epsilon_r_complex,
    )
    K_int, M_int = apply_dirichlet(K, M, interior_mask)

    # Algebraic spurious-mode dimension via the discrete de-Rham `d⁰`
    # operator. The classifier is dispositionally the same under complex
    # ε (gradients of H¹_0 sit in the kernel of curl-curl independent of
    # ε scaling on the mass — Epic #57 risk note), so we can reuse the
    # PEC implementation verbatim.
    n_spurious_algebraic = spurious_dim_from_derham(
        fixture.nodes, edges, interior_mask, r_outer=r_outer
    )

    # Take spurious_dim + 8 lowest-|Re| eigenvalues from the dense
    # spectrum — enough to expose the entire spurious cluster plus a
    # handful of physical modes. The dense path returns the full
    # spectrum and slices; the spurious_dim + 8 ceiling on
    # `eigenvalues_all` keeps the on-disk fixture compact (matches the
    # PEC convention in `sphere_pec.run_sphere_pec`).
    n_request = spurious_dim + 8
    eigvals = eigensolve_complex_dense(K_int, M_int, k_take=n_request)

    # Spurious filter — the d⁰-rank count is invariant under complex ε
    # scaling on the mass, so the same algebraic split that worked for
    # PEC works here. The first n_spurious entries of the |Re(λ)|-sorted
    # eigvals are the near-zero spurious cluster; everything past that
    # is physical.
    n_spurious = int(n_spurious_algebraic)
    if n_spurious + n_take > len(eigvals):
        raise RuntimeError(
            f"requested {n_take} physical modes but only "
            f"{len(eigvals) - n_spurious} available after spurious filter; "
            f"increase n_request"
        )
    physical = eigvals[n_spurious : n_spurious + n_take]

    # Q-factor of the lowest physical mode. We use the sign-agnostic
    # k-space convention `Q = Re(k) / (2 |Im(k)|)`, mirroring the Burn-
    # side print in `tests/sphere_pml_eigenmode.rs::sphere_pml_eigenmode_spectrum`
    # (lines 364-372). This is independent of which sign Im(λ) takes
    # under the complex-symmetric pencil (the eigensolver does not
    # enforce a sign on the imaginary part of complex eigenvalues), so
    # the cross-backend comparator gets a stable, positive Q regardless
    # of the platform-specific eigenvector phase that ``scipy.linalg.eig``
    # vs ``faer`` may select.
    #
    # k = sqrt(λ) with Re(k) ≥ 0 branch:
    #     r       = |λ|
    #     Re(k)   = sqrt((r + Re(λ)) / 2)
    #     |Im(k)| = sqrt((r − Re(λ)) / 2)
    lam_lowest = physical[0]
    r = abs(lam_lowest)
    re_k = np.sqrt(max(0.5 * (r + lam_lowest.real), 0.0))
    im_k_mag = np.sqrt(max(0.5 * (r - lam_lowest.real), 0.0))
    if im_k_mag > 1e-12:
        q_factor = float(re_k / (2.0 * im_k_mag))
    else:
        q_factor = float("inf")

    # σ₀ = 0 regression metric: when the PML is off, every eigenvalue
    # must be real to numerical precision. We compute this over the
    # *full* returned slice (spurious + physical) because the spurious
    # cluster carries the largest |Im/Re| ratios under f64 ARPACK noise
    # — a "real" spurious cluster is the load-bearing PEC-regression
    # signal.
    max_abs_re = max(np.max(np.abs(eigvals.real)), 1.0)
    max_imag_rel = float(np.max(np.abs(eigvals.imag)) / max_abs_re)

    # Complex-symmetry residual on M_int: max|M_ij - M_ji|. The
    # complex-ε mass is complex-symmetric (`M = M^T`, NOT Hermitian),
    # since the scatter is symmetric in (i, j) by construction and the
    # per-element 6×6 block is symmetric. A non-trivial residual would
    # indicate a regression in either the local kernel or the COO->CSR
    # collapse.
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
        "centroid_radii": centroid_radii,
        "epsilon_r_complex": epsilon_r_complex,
        "edges": edges,
        "tet_edge_idx": tet_edge_idx,
        "tet_edge_sign": tet_edge_sign,
        "interior_mask": interior_mask,
        "K": K,
        "M": M,
        "K_int": K_int,
        "M_int": M_int,
        "eigenvalues_all": eigvals,
        "physical_eigenvalues": physical,
        "q_factor_lowest_physical": q_factor,
        "k_int_frobenius": float(scipy.sparse.linalg.norm(K_int, "fro")),
        "m_int_frobenius": float(scipy.sparse.linalg.norm(M_int, "fro")),
        "m_int_complex_symmetry_residual": m_sym_residual,
        "max_imag_eigval_rel": max_imag_rel,
        "sigma_0": float(sigma_0),
        "n_index": float(n_index),
    }


if __name__ == "__main__":
    # CLI: print the lowest 5 physical eigenvalues + Q-factor diagnostic
    # for the bundled fixture. Mirrors the convention of `sphere_pec.py`.
    import argparse

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--fixture",
        default=str(
            Path(__file__).resolve().parent.parent
            / "fixtures"
            / "sphere_pml"
            / "sphere.msh"
        ),
        help="Path to the bundled Gmsh .msh fixture.",
    )
    parser.add_argument(
        "--sigma0",
        type=float,
        default=5.0,
        help="PML absorption strength (0.0 invokes the PEC regression).",
    )
    parser.add_argument(
        "--n-index",
        type=float,
        default=1.5,
        help="Refractive index inside the dielectric sphere.",
    )
    args = parser.parse_args()

    result = run_sphere_pml(
        args.fixture,
        sigma_0=args.sigma0,
        n_index=args.n_index,
        n_take=5,
    )

    print(f"sphere fixture: {result['n_nodes']} nodes, {result['n_tets']} tets")
    print(f"global edges: {result['n_edges']}")
    print(
        f"PEC reduction: {result['n_edges']} edges -> "
        f"{result['n_interior_edges']} interior DOFs"
    )
    print(f"predicted spurious-mode count: {result['spurious_dim']}")
    print(f"observed spurious count (d0-rank): {result['n_spurious']}")
    print(
        f"sigma_0 = {result['sigma_0']}; max|Im(lambda)|/max(|Re(lambda)|, 1) "
        f"over full slice = {result['max_imag_eigval_rel']:.3e}"
    )
    print()
    print("lowest 5 physical complex eigenvalues (lambda = k^2):")
    for i, lam in enumerate(result["physical_eigenvalues"]):
        print(
            f"  physical[{i}]: lambda = {lam.real:+.6e} {lam.imag:+.6e}j"
        )
    print()
    print(
        f"Q-factor of the lowest physical mode "
        f"(Re(k)/(2|Im(k)|)): {result['q_factor_lowest_physical']:.4f}"
    )
