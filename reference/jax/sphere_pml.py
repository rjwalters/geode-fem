"""JAX reference for the scalar-isotropic sphere-PML Nédélec eigenmode pipeline
(Epic #88 / Phase H.3 / Issue #148).

Mirrors `reference/jax/sphere_pec.py` (#128 / PR #131), with one
material-side change: per-tet relative permittivity is **complex**
(`complex128`) via the quadratic PML stretching factor. The Nédélec
local kernels are unchanged — the existing `_nedelec_cc_batch_jax`
runs over real cofactors and the mass kernel multiplies in the
complex `ε_r` after the real-valued tensor product is computed.

Design choices (Phase H.3-specific)
===================================

* **Complex constitutive in the mass.** The PML profile produces
  `ε_r ∈ ℂ`. Per-tet mass = `ε_r[e] * M_e_real`. We do the multiply
  on the host-side (NumPy) for the SciPy scatter path and inside the
  trace functional for the JAX autodiff probe path. The curl-curl
  block stays real (matches the Burn-side
  `assemble_global_nedelec_with_complex_epsilon` decomposition where
  K is real and M is complex).

* **Sparse vs dense.** Two assembly paths coexist here:
  1. ``assemble_global_nedelec_pml_jax`` — runs JAX `vmap+jit` for the
     local matrices, multiplies in complex ε, and uses
     `scipy.sparse.coo_matrix` for the global scatter (just like
     sphere_pec.py). This is the path the fixture generator and the
     end-to-end eigensolve use.
  2. ``tr_k_plus_m_int_from_eps_re_im`` — a small dense JAX-traced
     functional for the **autodiff probe**. Avoids `BCOO[complex128]`
     in the differentiated path because `BCOO` round-trips through
     `scipy.sparse` extraction with a complex dtype loss/round-trip
     friction that is itself the friction artifact to document.

* **Eigensolve boundary.** Same as Phase G.3: no backend lowers the
  generalized eigensolve in-graph (cf. `reference/onnx/audit/
  sphere_pec/nedelec_operator_audit.md` Stage 7). We materialize the
  SciPy CSR and call `scipy.sparse.linalg.eigs` (NOT `eigsh`: the
  complex-symmetric pencil is non-Hermitian even though it is complex-
  symmetric structurally).

* **σ₀ = 0 regression.** When called with `sigma_0 = 0`, the PML
  profile collapses to real ε = {n², 1, 1} and the spectrum should
  match the Phase G PEC baseline within ARPACK noise.

Differentiability probe (deliverable per issue body, friction target 2)
=======================================================================

The function ``probe_autodiff_complex_assembly`` runs the explicit
research deliverable: `jax.grad` of a scalar functional that flows
**through** complex assembly. The probe is documentation-only — it
prints the gradient value and any failure modes. It does **not**
hard-assert correctness (no FD cross-check on the complex side
because there is no canonical real-loss-of-complex-output convention
that maps cleanly to a single-precision gradient — see Wirtinger
calculus notes in the JAX docs). What we want to know is:

1. Does `jax.jit` lower the complex assembly path without errors?
2. Does `jax.grad` produce a finite gradient through it?
3. Are there NaNs or `complex128 unsupported` failures?

All three outcomes (success / silent-success / explicit-failure) are
spec-mining payoff for Epic #88.

Usage
=====

    python3 reference/jax/sphere_pml.py            # CLI self-check
    python3 reference/jax/sphere_pml.py --sigma0 5.0
    python3 reference/jax/sphere_pml.py --sigma0 0.0  # PEC regression
    python3 reference/jax/gen_sphere_pml_fixture.py  # write jax_baseline.json
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import NamedTuple

import numpy as np
import scipy.sparse
import scipy.sparse.linalg

# ---------------------------------------------------------------------------
# Import JAX (hard dependency)
# ---------------------------------------------------------------------------

try:
    import jax
    import jax.numpy as jnp
except ImportError as _jax_err:
    raise ImportError(
        "JAX is required for reference/jax/sphere_pml.py. "
        "Install with: pip install 'jax[cpu]'"
    ) from _jax_err

# Force f64/c128 mode — the entire validation contract is f64/c128.
jax.config.update("jax_enable_x64", True)

# ---------------------------------------------------------------------------
# Import from siblings: NumPy reference (mesh I/O, edge tables, spurious
# classifier, PEC mask) and the JAX sphere_pec module (local kernels).
# ---------------------------------------------------------------------------

HERE = Path(__file__).resolve().parent
REPO_REF = HERE.parent  # reference/

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187). Package-qualified imports
# disambiguate the same-named NumPy and JAX modules.
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

# JAX sphere-PEC kernels (sibling module; same basename as the NumPy
# reference, disambiguated by the package path).
from reference.jax import sphere_pec as _jax_pec  # noqa: E402

_nedelec_cc_batch_jax = _jax_pec._nedelec_cc_batch_jax
_nedelec_mass_batch_jax = _jax_pec._nedelec_mass_batch_jax

# NumPy-side helpers (algorithmic source of truth shared with PEC).
from reference.numpy.sphere_pec import (  # noqa: E402
    _deterministic_arpack_kwargs,
    PHYS_SPHERE_INTERIOR,
    PHYS_PML_SHELL,
    PHYS_VACUUM_GAP,
    R_BUFFER,
    R_PML_INNER,
    R_SPHERE,
    TET_LOCAL_EDGES,
    apply_dirichlet,
    build_edges,
    read_sphere_fixture,
    sphere_pec_interior_edges,
    sphere_n_interior_nodes,
    spurious_dim_from_derham,
)


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

N_INDEX: float = 1.5
SIGMA_0_DEFAULT: float = 5.0  # matches sphere_pml_eigenmode.rs

_MSH_PATH = REPO_REF / "fixtures" / "sphere_pml" / "sphere.msh"


# ---------------------------------------------------------------------------
# PML constitutive (mirror of geode_core::build_complex_epsilon_r_pml)
# ---------------------------------------------------------------------------


def tet_centroid_radii(nodes: np.ndarray, tets: np.ndarray) -> np.ndarray:
    """Per-tet centroid radius |c|, c = mean(verts). Mirror of the Rust
    `tet_centroid_radii` helper."""
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    centroids = nodes[tets, :].mean(axis=1)  # (n_tets, 3)
    return np.linalg.norm(centroids, axis=1)


def build_complex_epsilon_r_pml(
    physical_tags: np.ndarray,
    centroid_radii: np.ndarray,
    n_inside: float = N_INDEX,
    sigma_0: float = SIGMA_0_DEFAULT,
    r_pml_inner: float = R_PML_INNER,
    r_buffer: float = R_BUFFER,
) -> np.ndarray:
    """Per-tet complex relative permittivity for the scalar-isotropic
    sphere PML. Faithful port of
    ``geode_core::build_complex_epsilon_r_pml``.

    Profile (from the Rust docstring):

    * dielectric (`PHYS_SPHERE_INTERIOR`): ``ε = n² + 0j``
    * vacuum gap (`PHYS_VACUUM_GAP` or anything not PML / not dielectric):
      ``ε = 1 + 0j``
    * PML shell (`PHYS_PML_SHELL`): quadratic ramp,
      ``ε = 1 − j σ₀ ((r − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²``

    Sign convention: `exp(+jωt)` ⇒ `Im(ε) < 0` for outgoing attenuation.
    """
    tags = np.asarray(physical_tags, dtype=np.int32)
    rc = np.asarray(centroid_radii, dtype=np.float64)
    assert tags.shape == rc.shape, "physical_tags and centroid_radii length mismatch"

    eps_inside = float(n_inside) * float(n_inside)
    width = r_buffer - r_pml_inner

    eps = np.full(tags.shape, 1.0 + 0j, dtype=np.complex128)
    eps[tags == PHYS_SPHERE_INTERIOR] = complex(eps_inside, 0.0)
    pml = tags == PHYS_PML_SHELL
    u = np.clip((rc[pml] - r_pml_inner) / width, 0.0, 1.0)
    eps[pml] = 1.0 + 1j * (-sigma_0 * u * u)
    return eps


# ---------------------------------------------------------------------------
# JAX-accelerated assembly with complex per-tet ε
# ---------------------------------------------------------------------------


def assemble_global_nedelec_pml_jax(
    nodes,
    tets,
    edges,
    tet_edge_idx,
    tet_edge_sign,
    epsilon_r_complex,
):
    """Assemble global K (real) and M (complex) for the PML pipeline.

    - K (curl-curl): real, computed once via the JAX real kernel.
    - M (ε-mass): complex; computed as real M_e via the JAX kernel
      with `eps_r = 1`, then multiplied by the per-tet complex ε on the
      host side and scattered to a complex CSR.

    The split keeps the JAX kernels real (matches the existing PEC code
    path bit-for-bit) and isolates the complex multiplication to the
    cheap per-tet step where the autodiff probe can hook in.

    Returns
    -------
    K : scipy.sparse.csr_matrix (n_edges, n_edges) — real float64
    M : scipy.sparse.csr_matrix (n_edges, n_edges) — complex128
    """
    nodes = np.asarray(nodes, dtype=np.float64)
    tets = np.asarray(tets, dtype=np.int64)
    tet_edge_idx = np.asarray(tet_edge_idx, dtype=np.int64)
    tet_edge_sign = np.asarray(tet_edge_sign, dtype=np.float64)
    eps_complex = np.asarray(epsilon_r_complex, dtype=np.complex128)
    n_tets = tets.shape[0]
    n_edges = int(edges.shape[0])

    # Per-element vertex coordinates: (n_tets, 4, 3)
    coords = nodes[tets, :]
    coords_jax = jnp.asarray(coords, dtype=jnp.float64)

    # Real K_local via the existing JAX kernel.
    k_local_jax = _nedelec_cc_batch_jax(coords_jax)  # (n_tets, 6, 6)

    # Real M_local with eps=1, then host-side multiply by complex ε.
    ones_eps = jnp.ones((n_tets,), dtype=jnp.float64)
    m_local_real_jax = _nedelec_mass_batch_jax(coords_jax, ones_eps)  # (n_tets, 6, 6)

    k_local = np.asarray(k_local_jax)
    m_local_real = np.asarray(m_local_real_jax)

    # Apply per-tet sign outer product.
    sign_outer = tet_edge_sign[:, :, None] * tet_edge_sign[:, None, :]
    k_signed = k_local * sign_outer  # real
    m_signed_complex = m_local_real * sign_outer * eps_complex[:, None, None]  # complex

    rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).reshape(-1)
    cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).reshape(-1)
    k_vals = k_signed.reshape(-1)
    m_vals = m_signed_complex.reshape(-1)

    K = scipy.sparse.coo_matrix(
        (k_vals, (rows, cols)), shape=(n_edges, n_edges)
    ).tocsr()
    M = scipy.sparse.coo_matrix(
        (m_vals, (rows, cols)), shape=(n_edges, n_edges), dtype=np.complex128
    ).tocsr()
    return K, M


def apply_dirichlet_complex(K, M, interior_mask):
    """Like apply_dirichlet, but preserves complex dtype on M."""
    interior_mask = np.asarray(interior_mask, dtype=bool)
    interior = np.flatnonzero(interior_mask)
    K_int = K[interior, :][:, interior]
    M_int = M[interior, :][:, interior]
    return K_int.tocsr(), M_int.tocsr()


# ---------------------------------------------------------------------------
# Complex eigensolve via SciPy shim (Phase G.3 boundary convention)
# ---------------------------------------------------------------------------


def eigensolve_complex(
    K_int, M_int, k_request: int, sigma: complex = 0.9 + 0j
):
    """Lowest-k generalized complex eigenpairs of ``K x = λ M x``.

    Uses ``scipy.sparse.linalg.eigs`` (non-Hermitian sparse complex)
    with shift-and-invert at ``sigma`` and ``which='LM'``. NOT
    ``eigsh`` — the complex-symmetric pencil is structurally non-
    Hermitian under SciPy's convention.

    Choice of shift
    ---------------
    The PML eigenproblem has a spurious cluster of ~``spurious_dim``
    modes near ``λ = 0`` (gradient kernel of curl-curl) that are
    largely unaffected by the lossy ε scaling. A shift at ``σ = 0``
    would saturate the Krylov subspace on this cluster before reaching
    the physical band.

    The caller passes the canonical-band shift. For σ₀=5.0 the
    canonical NumPy lowest-physical band sits at λ ≈ 1.18 + 0.21j
    (PR #155) and the orchestrator uses ``sigma = 1.18 + 0.2j``.
    The shift is **outside the spurious cluster** so the converged
    modes are physical by construction.

    For ``σ₀ = 0`` (PEC regression), the physical band sits near
    `Re(λ) ≈ 1.42` and the orchestrator shifts there.

    Returns
    -------
    eigvals : (k_request,) complex128, sorted ascending by ``Re(λ)``.
    eigvecs : (n_int, k_request) complex128.
    """
    K_c = K_int.astype(np.complex128)
    M_c = M_int.astype(np.complex128)
    # Deterministic ARPACK iterations: reproducibility for
    # near-degenerate clusters (issue #191). Complex pencil, so the
    # start vector seeds both real and imaginary parts.
    det = _deterministic_arpack_kwargs(
        K_c.shape[0], scipy.sparse.linalg.eigs, complex_pencil=True
    )
    eigvals, eigvecs = scipy.sparse.linalg.eigs(
        K_c,
        k=int(k_request),
        M=M_c,
        sigma=complex(sigma),
        which="LM",
        tol=1e-10,
        **det,
    )
    order = np.argsort(eigvals.real)
    return eigvals[order], eigvecs[:, order]


def select_absorbing_physical_modes(
    eigvals_complex: np.ndarray,
    n_take: int = 5,
    re_floor: float = 1e-3,
    im_pos_floor: float = 1e-6,
) -> np.ndarray:
    """Pick the ``n_take`` lowest-`Re(λ)` modes with the physical PML signature.

    Filters
    -------
    - ``Re(λ) > re_floor`` : skips any spurious mode that snuck through.
    - ``Im(λ) > im_pos_floor`` : selects the absorbing branch under the
      canonical `Im(λ) > 0` convention (Epic #88 / PR #155 NumPy
      canonical tiebreaker). Each physical mode shows up as a near-
      conjugate pair under SciPy's non-Hermitian complex solver; we
      report only the canonical-branch member (Im(λ) > 0), discarding
      the conjugate (Im(λ) < 0) artefact.

    Returns the modes in ascending-Re order.
    """
    eigvals_complex = np.asarray(eigvals_complex, dtype=np.complex128)
    mask = (eigvals_complex.real > re_floor) & (eigvals_complex.imag > im_pos_floor)
    candidates = eigvals_complex[mask]
    candidates = candidates[np.argsort(candidates.real)]
    if len(candidates) < n_take:
        # Fall back to whatever modes we have (caller decides how to handle).
        return candidates
    return candidates[:n_take]


# ---------------------------------------------------------------------------
# Spurious filter (inherited from PEC — algebraic d⁰-rank survives lossy ε)
# ---------------------------------------------------------------------------


def split_spurious_complex(
    eigvals_complex: np.ndarray, n_spurious: int
) -> tuple[np.ndarray, np.ndarray]:
    """Slice the complex spectrum at the precomputed spurious count.

    Algebraic justification: gradients of H¹_0 live in the kernel of
    curl-curl regardless of complex scaling on the mass; the d⁰-rank
    classifier carries over from the PEC case unchanged.
    """
    eigvals_complex = np.asarray(eigvals_complex, dtype=np.complex128)
    n_spurious = int(n_spurious)
    if n_spurious < 0 or n_spurious > len(eigvals_complex):
        raise ValueError(
            f"invalid n_spurious={n_spurious}; must be in [0, {len(eigvals_complex)}]"
        )
    spurious = eigvals_complex[:n_spurious]
    physical = eigvals_complex[n_spurious:]
    return spurious, physical


def q_factor(lam: complex) -> float:
    """Quality factor (sign-agnostic): Q = Re(λ) / (2 |Im(λ)|).

    Matches the NumPy/Burn canonical formula so |Q| is invariant under
    the conjugate-branch choice. Positive for any absorbing mode.
    """
    return float(lam.real / (2.0 * abs(lam.imag)))


# ---------------------------------------------------------------------------
# Result type
# ---------------------------------------------------------------------------


class JaxSpherePmlResult(NamedTuple):
    n_nodes: int
    n_tets: int
    n_edges: int
    n_interior_edges: int
    spurious_dim: int
    epsilon_r_complex: np.ndarray             # shape [n_tets], c128
    eigenvalues_lowest_complex: np.ndarray    # shape [spurious_dim + 8], c128
    physical_eigenvalues_complex: np.ndarray  # shape [n_take], c128
    q_factor_lowest_physical: float
    sigma_0: float


# ---------------------------------------------------------------------------
# Main end-to-end solver
# ---------------------------------------------------------------------------


def solve_sphere_pml_jax(
    mesh_path=None,
    n_index: float = N_INDEX,
    sigma_0: float = SIGMA_0_DEFAULT,
    n_take: int = 5,
) -> JaxSpherePmlResult:
    """Run the JAX sphere-PML Nédélec pipeline end-to-end."""
    if mesh_path is None:
        mesh_path = _MSH_PATH

    fixture = read_sphere_fixture(mesh_path)
    radii = tet_centroid_radii(fixture.nodes, fixture.tets)
    eps_complex = build_complex_epsilon_r_pml(
        fixture.tet_physical_tags, radii, n_inside=n_index, sigma_0=sigma_0
    )

    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)

    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=R_BUFFER
    )
    n_interior_edges = int(np.sum(interior_mask))
    spurious_dim = sphere_n_interior_nodes(fixture.nodes, r_outer=R_BUFFER)

    K, M = assemble_global_nedelec_pml_jax(
        fixture.nodes,
        fixture.tets,
        edges,
        tet_edge_idx,
        tet_edge_sign,
        eps_complex,
    )
    K_int, M_int = apply_dirichlet_complex(K, M, interior_mask)

    n_spurious_algebraic = spurious_dim_from_derham(
        fixture.nodes, edges, interior_mask, r_outer=R_BUFFER
    )

    # Eigensolve via shift in the physical band. The NumPy canonical
    # (PR #155) puts the lowest σ₀=5 physical band at Re(λ) ≈ 1.18 +
    # 0.21j; the prior σ=0.9+0j shift pulled a lower-Re basin
    # (sub-band cluster near 0.89) that disagrees with NumPy by ~25%.
    # We now shift in the canonical band to match.
    n_request = max(40, 2 * n_take + 8)
    if sigma_0 == 0.0:
        # PEC band sits at λ ≈ 1.42 (no PML) — shift there.
        shift = 1.4 + 0j
    else:
        # Canonical σ₀=5 lowest physical band per PR #155.
        shift = 1.18 + 0.2j

    eigvals_complex, _eigvecs = eigensolve_complex(
        K_int, M_int, k_request=n_request, sigma=shift
    )

    if sigma_0 == 0.0:
        # PEC regression: imag is ARPACK noise; sort by Re and take lowest
        # `n_take` positive-real modes.
        real_modes = np.sort(eigvals_complex.real[eigvals_complex.real > 1e-3])
        physical_take = (real_modes[:n_take]).astype(np.complex128)
        q = float("nan")
    else:
        physical_take = select_absorbing_physical_modes(
            eigvals_complex, n_take=n_take
        )
        if len(physical_take) < n_take:
            raise RuntimeError(
                f"requested {n_take} absorbing physical modes but only "
                f"{len(physical_take)} pass the (Re>0, Im>0) filter "
                f"in the {n_request}-mode slice; increase n_request "
                f"or adjust shift"
            )
        q = q_factor(complex(physical_take[0]))

    return JaxSpherePmlResult(
        n_nodes=fixture.n_nodes,
        n_tets=fixture.n_tets,
        n_edges=int(edges.shape[0]),
        n_interior_edges=n_interior_edges,
        spurious_dim=int(spurious_dim),
        epsilon_r_complex=eps_complex,
        eigenvalues_lowest_complex=eigvals_complex.astype(np.complex128),
        physical_eigenvalues_complex=physical_take.astype(np.complex128),
        q_factor_lowest_physical=q,
        sigma_0=float(sigma_0),
    )


# ---------------------------------------------------------------------------
# Autodiff probe (Phase H.3 explicit research deliverable)
# ---------------------------------------------------------------------------
#
# We flow `jax.grad` through a small dense assembly path whose **input
# is the per-tet complex permittivity vector** (split into real and
# imaginary components for the parameter axis — `jax.grad` requires
# real-valued inputs at the top of the chain).
#
# The functional is `loss(re, im) = Re(Tr(K_int + M_int_complex))` —
# trivially differentiable in finite arithmetic, but the question of
# interest is whether the **trace through complex assembly** lowers
# under XLA and produces a finite gradient. The smoke functional is
# the minimum non-trivial scalar that closes over complex assembly.


def _build_static_pml_topology(mesh_path=None):
    """Precompute and freeze the mesh-derived topology for the autodiff probe.

    Returns the host-side arrays needed for a closed-over JAX function.
    """
    if mesh_path is None:
        mesh_path = _MSH_PATH

    fixture = read_sphere_fixture(mesh_path)
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    interior_mask, _on_wall = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=R_BUFFER
    )
    interior_indices = np.flatnonzero(interior_mask)
    return {
        "nodes": fixture.nodes,
        "tets": fixture.tets,
        "tet_physical_tags": fixture.tet_physical_tags,
        "edges": edges,
        "tet_edge_idx": tet_edge_idx,
        "tet_edge_sign": tet_edge_sign,
        "interior_indices": interior_indices,
        "n_edges": int(edges.shape[0]),
        "n_tets": int(fixture.tets.shape[0]),
    }


def _make_complex_assembly_loss(topo):
    """Build a `loss(eps_re, eps_im) -> jnp.float64` flowing through complex
    assembly. Closed over precomputed mesh topology.

    Splits the complex `ε_r` into two real arrays so the differentiated
    parameter is real (JAX's `jax.grad` requires a real input by default;
    Wirtinger calculus support exists but is opt-in).
    """
    n_tets = topo["n_tets"]
    n_edges = topo["n_edges"]
    tet_edge_idx = np.asarray(topo["tet_edge_idx"], dtype=np.int64)
    tet_edge_sign = np.asarray(topo["tet_edge_sign"], dtype=np.float64)
    coords_np = topo["nodes"][topo["tets"], :].astype(np.float64)  # (n_tets, 4, 3)
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

    # Pre-compute real K_local once (no autodiff parameter dependence).
    k_local_real = _nedelec_cc_batch_jax(coords_jax)  # (n_tets, 6, 6)
    k_signed = (k_local_real * sign_outer_jax).reshape(-1)  # (n_tets*36,)

    # Pre-compute real M_local once (ε=1) — autodiff parameter is the
    # per-tet complex ε that multiplies this on the fly.
    ones_eps = jnp.ones((n_tets,), dtype=jnp.float64)
    m_local_real = _nedelec_mass_batch_jax(coords_jax, ones_eps)  # (n_tets, 6, 6)
    m_local_signed = m_local_real * sign_outer_jax  # (n_tets, 6, 6) real

    def loss(eps_re: jnp.ndarray, eps_im: jnp.ndarray) -> jnp.ndarray:
        """Re(Tr(K_int + M_int)) — flows grad through complex assembly."""
        eps_complex = eps_re.astype(jnp.complex128) + 1j * eps_im.astype(jnp.complex128)
        # Broadcast per-tet ε across the (6,6) block.
        m_signed_c = (
            m_local_signed.astype(jnp.complex128) * eps_complex[:, None, None]
        ).reshape(-1)  # (n_tets*36,) complex128

        # Scatter into dense global complex M, and dense global real K.
        m_global = jnp.zeros((n_edges, n_edges), dtype=jnp.complex128)
        m_global = m_global.at[rows_jax, cols_jax].add(m_signed_c)
        k_global = jnp.zeros((n_edges, n_edges), dtype=jnp.float64)
        k_global = k_global.at[rows_jax, cols_jax].add(k_signed)

        # Interior restriction.
        m_int = m_global[jnp.ix_(int_idx_jax, int_idx_jax)]
        k_int = k_global[jnp.ix_(int_idx_jax, int_idx_jax)]

        # Combine. K_int trace (real) + |Tr(M_int)|² gives a scalar
        # functional that depends on **both** Re(ε) and Im(ε), so the
        # gradient probe is informative on the complex axis too. The
        # `|·|²` formulation is the standard "PML loss" surrogate for
        # an autodiff probe — it tests whether XLA traces the
        # `complex × complex` multiply through to a real loss cleanly.
        tr_m = jnp.trace(m_int)
        return jnp.trace(k_int) + (tr_m.real ** 2 + tr_m.imag ** 2)

    return loss


def probe_autodiff_complex_assembly(mesh_path=None, sigma_0: float = SIGMA_0_DEFAULT):
    """Phase H.3 explicit research deliverable.

    Runs `jax.grad` through complex assembly and reports:
      - whether the JIT-compiled loss runs at all,
      - whether the gradient is finite (no NaN/inf),
      - basic sanity numbers (loss value, ||grad||_∞).

    Does NOT assert numerical correctness — this is the friction-mining
    probe; per the issue, the value's correctness is "not asserted; the
    probe is documentation, not validation."
    """
    if mesh_path is None:
        mesh_path = _MSH_PATH

    topo = _build_static_pml_topology(mesh_path)
    radii = tet_centroid_radii(topo["nodes"], topo["tets"])
    eps_complex = build_complex_epsilon_r_pml(
        topo["tet_physical_tags"], radii, n_inside=N_INDEX, sigma_0=sigma_0
    )

    eps_re0 = jnp.asarray(eps_complex.real, dtype=jnp.float64)
    eps_im0 = jnp.asarray(eps_complex.imag, dtype=jnp.float64)

    findings: dict = {
        "sigma_0": float(sigma_0),
        "n_tets": int(topo["n_tets"]),
        "jit_ok": None,
        "loss_value": None,
        "grad_ok": None,
        "grad_max_abs_re": None,
        "grad_max_abs_im": None,
        "grad_finite": None,
        "errors": [],
    }

    try:
        loss = _make_complex_assembly_loss(topo)
        # JIT lowering smoke.
        loss_jit = jax.jit(loss)
        val = float(loss_jit(eps_re0, eps_im0))
        findings["jit_ok"] = True
        findings["loss_value"] = val
    except Exception as e:  # pragma: no cover — friction artifact path
        findings["jit_ok"] = False
        findings["errors"].append(f"jit loss raise: {type(e).__name__}: {e}")
        return findings

    try:
        # Differentiate w.r.t. both real-axis and imag-axis halves of ε.
        # This is the *interesting* gradient — it asks how the loss
        # changes under perturbations of the **imaginary** PML profile
        # (i.e. under absorption strength). If JAX traces complex
        # assembly cleanly, both gradient components are finite.
        grad_fn = jax.grad(loss, argnums=(0, 1))
        grad_fn = jax.jit(grad_fn)
        g_re, g_im = grad_fn(eps_re0, eps_im0)
        g_re_np = np.asarray(g_re)
        g_im_np = np.asarray(g_im)
        finite = (
            np.all(np.isfinite(g_re_np)) and np.all(np.isfinite(g_im_np))
        )
        findings["grad_ok"] = True
        findings["grad_max_abs_re"] = float(np.max(np.abs(g_re_np)))
        findings["grad_max_abs_im"] = float(np.max(np.abs(g_im_np)))
        findings["grad_finite"] = bool(finite)
    except Exception as e:  # pragma: no cover — friction artifact path
        findings["grad_ok"] = False
        findings["errors"].append(f"grad raise: {type(e).__name__}: {e}")

    return findings


# ---------------------------------------------------------------------------
# CLI self-check
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="JAX sphere-PML Nédélec self-check")
    parser.add_argument(
        "--fixture",
        type=str,
        default=str(_MSH_PATH),
        help="Path to bundled Gmsh .msh fixture",
    )
    parser.add_argument(
        "--sigma0", type=float, default=SIGMA_0_DEFAULT,
        help=f"PML absorption strength (default {SIGMA_0_DEFAULT})",
    )
    parser.add_argument(
        "--skip-autodiff", action="store_true",
        help="Skip the autodiff probe",
    )
    args = parser.parse_args()

    print(f"== JAX sphere-PML self-check ==  jax={jax.__version__}, "
          f"f64={jax.config.read('jax_enable_x64')}, "
          f"backend={jax.default_backend()}, σ₀={args.sigma0}")

    result = solve_sphere_pml_jax(mesh_path=Path(args.fixture), sigma_0=args.sigma0)

    print(f"\nMesh: {result.n_nodes} nodes, {result.n_tets} tets")
    print(f"Global edges: {result.n_edges}")
    print(f"Interior DOFs (after PEC): {result.n_interior_edges}")
    print(f"spurious_dim = {result.spurious_dim}  (expected 368)")
    print(f"|ε|_max = {np.max(np.abs(result.epsilon_r_complex)):.4f}, "
          f"min Im(ε) = {np.min(result.epsilon_r_complex.imag):.4f}")

    print(f"\nLowest {len(result.physical_eigenvalues_complex)} "
          f"physical eigenvalues (λ = k²):")
    for i, lam in enumerate(result.physical_eigenvalues_complex):
        print(f"  λ[{i}] = {lam.real:+.6e} {lam.imag:+.6e}j")
    print(f"\nQ_lowest_physical = {result.q_factor_lowest_physical:.4f}  "
          f"(Q = Re(λ)/(2|Im(λ)|); positive ⇒ absorbing — canonical Im(λ) > 0)")

    if not args.skip_autodiff:
        print("\n== Autodiff probe: jax.grad through complex assembly ==")
        findings = probe_autodiff_complex_assembly(
            mesh_path=Path(args.fixture), sigma_0=args.sigma0
        )
        print(f"  σ₀ = {findings['sigma_0']}, n_tets = {findings['n_tets']}")
        print(f"  jit_ok    = {findings['jit_ok']}")
        print(f"  loss_value = {findings['loss_value']}")
        print(f"  grad_ok   = {findings['grad_ok']}")
        print(f"  grad_finite = {findings['grad_finite']}")
        print(f"  ||grad_re||_∞ = {findings['grad_max_abs_re']}")
        print(f"  ||grad_im||_∞ = {findings['grad_max_abs_im']}")
        if findings["errors"]:
            print(f"  errors  = {findings['errors']}")
        else:
            print("  errors  = (none)")
        print("  NOTE: gradient values are NOT cross-checked against FD; "
              "the probe is documentation only (per Issue #148).")

    print("\nSelf-check: DONE")
    sys.exit(0)
