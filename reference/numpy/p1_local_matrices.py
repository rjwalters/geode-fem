"""NumPy reference for batched P1 tetrahedral local stiffness/mass.

Faithful transcription of the closed-form expressions documented in
`crates/geode-core/src/p1.rs` (module-level docstring, lines 9-31).

Per issue #90, this is intentionally *not* a clever vectorized rewrite.
It is the math, in code, in NumPy, with named intermediates that match
the symbols in the docstring one-for-one. Cross-checkability against
the math is the point.

The math
========

Given a tet with vertices ``v_0, v_1, v_2, v_3`` and edge vectors
``e_k = v_k - v_0``, the Jacobian of the affine map from the reference
tet ``[(0,0,0), (1,0,0), (0,1,0), (0,0,1)]`` to the physical tet is
``J = [e_1 | e_2 | e_3]`` (3x3, edges as columns).

Area-weighted basis-function gradients (each a 3-vector):

    g_1 = e_2 x e_3,    g_2 = e_3 x e_1,    g_3 = e_1 x e_2,
    g_0 = -(g_1 + g_2 + g_3),
    det = e_1 . g_1 = det(J).

Then ``grad(phi_i) = g_i / det(J)``, the signed element volume is
``V_signed = det / 6``, the (positive) element volume is
``V = |det| / 6``, and the local matrices are:

    K_{ij} = V (grad(phi_i) . grad(phi_j)) = (g_i . g_j) / (6 |det|),
    M_{ij} = (V / 20) (1 + delta_{ij})    (consistent mass).

Public API
==========

``batched_p1_local_matrices(coords) -> (k_local, m_local, signed_volumes)``
"""

from __future__ import annotations

import numpy as np


def batched_p1_local_matrices(coords):
    """Compute P1 local stiffness, mass, and signed volume for a batch of tets.

    Parameters
    ----------
    coords : ndarray, shape ``(n_elem, 4, 3)``, dtype float64
        Per-element tet vertex coordinates. ``coords[:, 0, :]`` is the
        "base" vertex used to form edges ``e_k = v_k - v_0``.

    Returns
    -------
    k_local : ndarray, shape ``(n_elem, 4, 4)``, dtype float64
        Local stiffness matrices ``K_{ij} = V (grad(phi_i) . grad(phi_j))``.
    m_local : ndarray, shape ``(n_elem, 4, 4)``, dtype float64
        Local consistent mass ``M_{ij} = (V/20)(1 + delta_{ij})``.
    signed_volumes : ndarray, shape ``(n_elem,)``, dtype float64
        Signed element volume ``det(J) / 6``. Negative for inverted tets.
        For assembly weighting use ``abs(signed_volumes)`` — the sign is a
        mesh-quality diagnostic only.
    """
    coords = np.asarray(coords, dtype=np.float64)
    if coords.ndim != 3 or coords.shape[1:] != (4, 3):
        raise ValueError(
            f"expected coords shape (n_elem, 4, 3), got {coords.shape}"
        )

    n_elem = coords.shape[0]

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
    # np.cross over the last axis is the per-element 3-vector cross product.
    g1 = np.cross(e2, e3)
    g2 = np.cross(e3, e1)
    g3 = np.cross(e1, e2)

    # g_0 = -(g_1 + g_2 + g_3), shape (n_elem, 3).
    g0 = -(g1 + g2 + g3)

    # det(J) per element = e_1 . g_1, shape (n_elem,).
    # einsum here is just the per-row dot product e1[i, :] . g1[i, :].
    det = np.einsum("ij,ij->i", e1, g1)

    # Signed volume: det / 6, shape (n_elem,). Negative for inverted tets.
    signed_volumes = det / 6.0

    # Stack G as (n_elem, 4, 3) with row i = g_i.
    g_mat = np.stack([g0, g1, g2, g3], axis=1)

    # (G @ G^T)_{ij} = g_i . g_j, shape (n_elem, 4, 4).
    # einsum "eik,ejk->eij" is the per-batch matrix-times-its-transpose op.
    gg = np.einsum("eik,ejk->eij", g_mat, g_mat)

    # K_{ij} = (g_i . g_j) / (6 |det|), shape (n_elem, 4, 4).
    abs_det = np.abs(det)
    k_scale = 1.0 / (6.0 * abs_det)  # shape (n_elem,)
    k_local = gg * k_scale[:, None, None]

    # M = (V / 20)(I_4 + ones_4x4): 2 on diagonal, 1 off-diagonal.
    # V = |det| / 6, so V / 20 = |det| / 120.
    mass_pattern = np.array(
        [
            [2.0, 1.0, 1.0, 1.0],
            [1.0, 2.0, 1.0, 1.0],
            [1.0, 1.0, 2.0, 1.0],
            [1.0, 1.0, 1.0, 2.0],
        ],
        dtype=np.float64,
    )
    m_scale = abs_det / 120.0  # shape (n_elem,)
    m_local = mass_pattern[None, :, :] * m_scale[:, None, None]

    return k_local, m_local, signed_volumes


if __name__ == "__main__":
    # Self-check on the canonical reference tet — K should be (1/6)*[[3,-1,-1,-1],
    # [-1,1,0,0], [-1,0,1,0], [-1,0,0,1]] and M should be (1/120)*[[2,1,1,1],
    # [1,2,1,1], [1,1,2,1], [1,1,1,2]] and V = 1/6.
    ref = np.array(
        [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]],
        dtype=np.float64,
    )
    k, m, v = batched_p1_local_matrices(ref)
    print("K (ref tet):")
    print(k[0])
    print("M (ref tet):")
    print(m[0])
    print(f"V (ref tet, signed) = {v[0]}")
