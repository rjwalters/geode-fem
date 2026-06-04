"""NumPy reference for batched Nédélec edge-element local curl-curl and mass.

Faithful transcription of the closed-form expressions documented in
``crates/geode-core/src/nedelec.rs`` (module-level docstring, lines 51-74).
Sister of ``reference/numpy/p1_local_matrices.py``: same geometry, same
``[n_elem, 4, 3]`` input shape, same affine machinery — but the per-element
output is the 6x6 Whitney edge-element pair, one entry per ordered edge
pair in ``TET_LOCAL_EDGES``.

Per issue #117 / Epic #88 phase G.1, this is intentionally *not* a clever
vectorized rewrite. It is the math, in code, in NumPy, with named
intermediates that match the symbols in the Rust docstring one-for-one.
Cross-checkability against ``nedelec.rs:51-74`` is the point — a reader
should be able to put this file side-by-side with the Burn-side kernel
and read the same formula in both.

The math
========

For a tet with vertices ``v_0, v_1, v_2, v_3``, the P1 nodal basis has
gradients ``grad(lambda_p) = g_p / det(J)`` where ``g_p`` is the same
"area-weighted gradient" used by the P1 reference. The gram matrix on
the physical gradients is

    G_pq = grad(lambda_p) . grad(lambda_q) = (g_p . g_q) / det(J)^2.

We deliberately *don't* form ``G`` explicitly. Instead, the documented
cofactor reformulation in ``nedelec.rs:199-210`` folds the
``1/det(J)^2`` factor into the per-entry K and M scales, keeping the
gram entries ``gg_pq = g_p . g_q`` at modest magnitude even on
near-degenerate tets where ``det(J) -> 0``. This matters in practice:
on the ``near_degenerate_sliver`` case (``det ~ 1e-6``), forming
``G_33 = gg_33/det^2`` directly costs ~6 decimal digits of mantissa to
the ``G*G - G*G`` cancellation in K, while the cofactor form keeps the
``gg*gg - gg*gg`` step well-conditioned and concentrates the blowup in
a clean ``1/|det|^3`` broadcast.

The 6 local edges, in the canonical ``TET_LOCAL_EDGES`` order, are

    edge 0: (a, b) = (0, 1)
    edge 1: (a, b) = (0, 2)
    edge 2: (a, b) = (0, 3)
    edge 3: (a, b) = (1, 2)
    edge 4: (a, b) = (1, 3)
    edge 5: (a, b) = (2, 3)

This reference treats every local edge as oriented from the lower-index
local vertex ``a`` to the higher-index local vertex ``b`` — i.e. all
local edge signs ``s_i = +1``. The global sign-flip step (per-tet
``s_i s_j`` row/column scaling, ``nedelec.rs:30-34``) is decoupled from
this local kernel and lives in the Rust harness.

With ``i = (a, b)``, ``j = (c, d)`` ordered pairs and
``V = |det(J)| / 6``,

Curl-curl (cofactor form, exact rewrite of ``4 V (G_ac G_bd - G_ad G_bc)``)::

    K_{ij} = (2 / 3) * (gg_ac gg_bd - gg_ad gg_bc) / |det|^3.   (eq. K)

Mass (Whitney 1-form integrated through ``int lambda_p lambda_q = (V/20)(1 + delta_pq)``,
also in cofactor form, with ``(V/20) G_pq = gg_pq / (120 |det|)``)::

    M_{ij} = (1 / (120 |det|)) [   (1 + delta_ac) gg_bd
                                 - (1 + delta_ad) gg_bc
                                 - (1 + delta_bc) gg_ad
                                 + (1 + delta_bd) gg_ac ].      (eq. M)

These are exact. No quadrature is required: the Whitney curl
``grad x N_i = 2 (grad lambda_a x grad lambda_b)`` is piecewise constant
per tet, and the four-Kronecker mass expansion above is the exact lift
of the cubic ``N_i . N_j`` polynomial under the standard tet moment
formula.

Public API
==========

``batched_nedelec_local_matrices(coords) -> (k_local, m_local, signed_volumes)``

``TET_LOCAL_EDGES`` — the 6-pair canonical ordering, exported for use by
the per-case fixture generator.
"""

from __future__ import annotations

from pathlib import Path
import sys

import numpy as np

# Reuse the P1 reference's signed-volume / area-weighted-gradient
# helpers verbatim. This is by design — the issue acceptance criteria
# call out that the signed-volume computation should not be duplicated.
HERE = Path(__file__).resolve().parent
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))
from p1_local_matrices import batched_p1_local_matrices  # noqa: E402

# Canonical local edge order on a tet. Mirrors
# ``crates/geode-core/src/mesh/mod.rs::TET_LOCAL_EDGES``; the order is
# fixed across the codebase and used by both the host-side edge-table
# builder and the batched local-matrix kernel.
TET_LOCAL_EDGES: tuple[tuple[int, int], ...] = (
    (0, 1),
    (0, 2),
    (0, 3),
    (1, 2),
    (1, 3),
    (2, 3),
)


def _cofactor_gram(coords: np.ndarray):
    """Return ``(gg, det, signed_volumes)`` for a batch of tets.

    ``gg`` has shape ``(n_elem, 4, 4)`` with ``gg[e, p, q] = g_p . g_q``,
    the **cofactor** gradient gram (i.e. the physical gradient
    ``grad(lambda_p)`` scaled by ``det(J)``). The physical gram is
    ``G_pq = gg_pq / det^2`` — but we deliberately *don't* form ``G`` here
    because the explicit divide-by-``det^2`` is numerically catastrophic
    on near-degenerate tets where ``det -> 0``. Instead, the divide is
    folded into the per-entry K and M scales (``1/|det|^3`` and
    ``1/|det|`` respectively, exactly matching the documented
    reformulation in ``crates/geode-core/src/nedelec.rs:199-210``).

    ``signed_volumes`` is taken from the P1 reference to avoid
    duplicating the ``det(J) / 6`` helper across two files.
    """
    coords = np.asarray(coords, dtype=np.float64)
    if coords.ndim != 3 or coords.shape[1:] != (4, 3):
        raise ValueError(
            f"expected coords shape (n_elem, 4, 3), got {coords.shape}"
        )

    # Per-vertex slices, each (n_elem, 3).
    v0 = coords[:, 0, :]
    v1 = coords[:, 1, :]
    v2 = coords[:, 2, :]
    v3 = coords[:, 3, :]

    # Edge vectors from v0, each (n_elem, 3).
    e1 = v1 - v0
    e2 = v2 - v0
    e3 = v3 - v0

    # Area-weighted basis gradients g_1, g_2, g_3 (each (n_elem, 3)).
    g1 = np.cross(e2, e3)
    g2 = np.cross(e3, e1)
    g3 = np.cross(e1, e2)
    # g_0 = -(g_1 + g_2 + g_3), shape (n_elem, 3).
    g0 = -(g1 + g2 + g3)

    # det(J) per element = e_1 . g_1, shape (n_elem,).
    det = np.einsum("ij,ij->i", e1, g1)

    # Stack g_p into a (n_elem, 4, 3) array.
    g_mat = np.stack([g0, g1, g2, g3], axis=1)

    # gg[e, p, q] = g_p . g_q via per-batch G @ G^T. Modest magnitudes
    # even on near-degenerate tets — the small-`det` blowup is moved
    # entirely to the per-entry K and M scale factors.
    gg = np.einsum("eik,ejk->eij", g_mat, g_mat)

    # Defer signed_volume to the P1 reference: V_signed = det(J) / 6.
    _, _, signed_volumes = batched_p1_local_matrices(coords)

    return gg, det, signed_volumes


def batched_nedelec_local_matrices(coords):
    """Compute Nédélec local curl-curl, mass, and signed volume per tet.

    Parameters
    ----------
    coords : ndarray, shape ``(n_elem, 4, 3)``, dtype float64
        Per-element tet vertex coordinates.

    Returns
    -------
    k_local : ndarray, shape ``(n_elem, 6, 6)``, dtype float64
        Local curl-curl stiffness ``K_{ij} = 4 V (G_ac G_bd - G_ad G_bc)``
        per the canonical edge order in :data:`TET_LOCAL_EDGES`. All
        local edge signs are treated as ``+1`` (lower-vertex-to-higher);
        the global ``s_i s_j`` correction is the caller's responsibility.
    m_local : ndarray, shape ``(n_elem, 6, 6)``, dtype float64
        Local mass matrix per equation (M) above. Same orientation
        convention as ``k_local``.
    signed_volumes : ndarray, shape ``(n_elem,)``, dtype float64
        Signed element volume ``det(J) / 6``. Negative for inverted tets.
    """
    coords = np.asarray(coords, dtype=np.float64)
    if coords.ndim != 3 or coords.shape[1:] != (4, 3):
        raise ValueError(
            f"expected coords shape (n_elem, 4, 3), got {coords.shape}"
        )

    n_elem = coords.shape[0]

    gg, det, signed_volumes = _cofactor_gram(coords)
    abs_det = np.abs(det)  # |det(J)|, shape (n_elem,)

    # Per-entry scale factors (mirror nedelec.rs:228-229). The blowup at
    # det -> 0 is concentrated in these scalar broadcasts; the gg entries
    # themselves stay O(1) on the sliver, so the gg*gg - gg*gg
    # cancellation in K stays well-conditioned and we don't lose 6+
    # digits of mantissa the way a precomputed G = gg/det^2 would.
    inv_abs_det = 1.0 / abs_det
    inv_abs_det3 = inv_abs_det * inv_abs_det * inv_abs_det

    k_local = np.zeros((n_elem, 6, 6), dtype=np.float64)
    m_local = np.zeros((n_elem, 6, 6), dtype=np.float64)

    for i, (a, b) in enumerate(TET_LOCAL_EDGES):
        for j, (c, d) in enumerate(TET_LOCAL_EDGES):
            # Per-element cofactor-gram entries: each shape (n_elem,).
            gg_ac = gg[:, a, c]
            gg_ad = gg[:, a, d]
            gg_bc = gg[:, b, c]
            gg_bd = gg[:, b, d]

            # Curl-curl in the documented cofactor reformulation
            # (nedelec.rs:199-210):
            #   K_{ij} = (2/3) * (gg_ac gg_bd - gg_ad gg_bc) / |det|^3.
            # Mathematically identical to `4 V (G_ac G_bd - G_ad G_bc)`
            # but numerically far better behaved on near-degenerate tets.
            k_local[:, i, j] = (
                (2.0 / 3.0)
                * (gg_ac * gg_bd - gg_ad * gg_bc)
                * inv_abs_det3
            )

            # Mass: four Kronecker-delta-lifted terms, each (1 + delta) G_pq.
            # Folding (V/20) G_pq = (|det|/120)(gg_pq/det²) = gg_pq/(120|det|)
            # into a single scalar multiply (nedelec.rs:208-210).
            f_ac = 2.0 if a == c else 1.0
            f_ad = 2.0 if a == d else 1.0
            f_bc = 2.0 if b == c else 1.0
            f_bd = 2.0 if b == d else 1.0
            m_term = (
                f_ac * gg_bd
                - f_ad * gg_bc
                - f_bc * gg_ad
                + f_bd * gg_ac
            )
            m_local[:, i, j] = m_term * inv_abs_det / 120.0

    return k_local, m_local, signed_volumes


if __name__ == "__main__":
    # Self-check on the canonical reference tet. With
    # grad(lambda_0) = (-1, -1, -1), grad(lambda_1) = (1, 0, 0),
    # grad(lambda_2) = (0, 1, 0), grad(lambda_3) = (0, 0, 1) and V = 1/6,
    # edge (0,1) = (a,b) = (0,1) gives
    #   K[0,0] = 4 V (G_00 G_11 - G_01 G_01)
    #          = (4/6) (3 * 1 - (-1)^2)
    #          = (4/6) * 2 = 4/3.
    # M[0,0] = (V/20) [ (1 + delta_00) G_11 - (1 + 0) G_10
    #                   - (1 + 0) G_10 + (1 + delta_11) G_00 ]
    #        = (1/120) [ 2*1 - (-1) - (-1) + 2*3 ]
    #        = (1/120) * 10 = 1/12.
    ref = np.array(
        [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]],
        dtype=np.float64,
    )
    k, m, v = batched_nedelec_local_matrices(ref)
    print("K (ref tet):")
    print(k[0])
    print("M (ref tet):")
    print(m[0])
    print(f"V (ref tet, signed) = {v[0]}")
    print(f"Expected K[0,0] = 4/3 = {4.0 / 3.0}; got {k[0, 0, 0]}")
    print(f"Expected M[0,0] = 1/12 = {1.0 / 12.0}; got {m[0, 0, 0]}")
