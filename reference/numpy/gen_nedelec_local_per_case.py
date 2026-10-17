"""Generate per-case canonical fixtures under ``fixtures/nedelec_local/<case>.json``.

Mirror of ``gen_p1_local_per_case.py`` for the Nédélec curl-conforming
edge-element kernel. Five canonical cases — same tet geometry as the
P1 fixture set, so a reviewer can hold the P1 and Nédélec fixtures
side-by-side for the same tet and see how the edge-element math
differs from the nodal-P1 math on identical geometry.

The five cases (per issue #117 acceptance criteria) are:

  1. Canonical reference tet ``[(0,0,0), (1,0,0), (0,1,0), (0,0,1)]``.
  2. Regular tet (vertices of a regular tetrahedron, centered at origin,
     unit edge length).
  3. Anisotropic-but-well-shaped tet (1 x 0.5 x 0.25 axis stretch on the
     reference tet — well-conditioned, but with non-unit aspect ratio).
  4. Near-degenerate sliver tet (three vertices nearly coplanar; small
     positive volume, condition number is bad but determinant is not zero).
  5. Inverted tet (vertices 2 and 3 swapped on the reference tet → the
     orientation flips, signed volume becomes negative).

Per-case canonical schema (per issue #101 / #117)
-------------------------------------------------
Each output file is in the canonical schema v1 documented in
``reference/SCHEMA.md``. The Rust comparator at
``crates/geode-validation/tests/nedelec_local_numpy_reference.rs``
layers a backend-aware mixed abs/rel check on top to enforce the
``1e-10`` rel / ``1e-12`` abs acceptance criterion under ``ndarray``
(f64) and ``5e-5`` rel / ``1e-6`` abs under f32 GPU backends.

Edge-orientation contract
-------------------------
The NumPy reference treats every local edge as oriented from the
lower-index local vertex to the higher (``s_i = +1`` for all i). The
fixture pins this explicitly via a ``tet_local_edge_signs`` input field
of shape ``(1, 6)`` so the Rust harness can apply the documented
``s_i s_j`` sign correction (``nedelec.rs:30-34``) to the Burn output
before comparing. This decouples the local-kernel test from the
global edge-table builder (a Phase G.2 concern).

Run
---
    python3 reference/numpy/gen_nedelec_local_per_case.py
"""

from __future__ import annotations

import json
import math
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/ -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "nedelec_local"

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.nedelec_local_matrices import batched_nedelec_local_matrices  # noqa: E402


def _ref_tet():
    """Canonical reference tet — the affine map identity."""
    return np.array(
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        dtype=np.float64,
    )


def _regular_tet():
    """Regular tetrahedron, unit edge length, centered at origin."""
    raw = np.array(
        [
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ],
        dtype=np.float64,
    )
    raw /= 2.0 * math.sqrt(2.0)
    return raw


def _anisotropic_tet():
    """Reference tet stretched anisotropically (1, 0.5, 0.25)."""
    base = _ref_tet()
    scale = np.array([1.0, 0.5, 0.25], dtype=np.float64)
    return base * scale


def _sliver_tet():
    """Near-degenerate sliver tet — v3 barely lifted out of the xy plane."""
    return np.array(
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.3, 0.3, 1.0e-6],
        ],
        dtype=np.float64,
    )


def _inverted_tet():
    """Reference tet with vertices 2 and 3 swapped — negative signed volume."""
    return np.array(
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        ],
        dtype=np.float64,
    )


# Each tuple: (case_slug, builder_fn, description, per-field tolerance_abs).
# Tolerances are loose absolute (catch catastrophic regression /
# structural mistakes); the Rust comparator layers a tighter
# backend-aware mixed abs/rel check on top.
#
# Scale notes (for picking absolute tolerances):
#   - K entries on the reference tet are O(1); regular tet O(10); the
#     anisotropic case is O(10) too. We use 1e-4 to clear the f32
#     5e-5 relative envelope on entries of magnitude 1.
#   - M entries on the reference tet are O(1e-2); on the sliver, the
#     M scale collapses to O(V) = O(1e-7), so absolute 1e-12 is
#     ~5e-5 of the max entry there.
#   - K on the sliver is O(1 / V) = O(1e6); 1e2 ~ 5e-5 of max.
CASES = [
    (
        "canonical_reference_tet",
        _ref_tet,
        "Nédélec edge-element local matrices for the canonical reference tet "
        "[(0,0,0),(1,0,0),(0,1,0),(0,0,1)] — the affine identity. On this tet "
        "the per-edge entry K[0,0] = 4/3 exactly and M[0,0] = 1/12 exactly; "
        "the rest of K and M follow from the closed-form math in "
        "crates/geode-core/src/nedelec.rs:51-74. Spot-check entries are "
        "documented in the file's `description` and verified at NumPy generation time.",
        {"k_local": 1.0e-4, "m_local": 1.0e-5, "signed_volume": 1.0e-5},
    ),
    (
        "regular_tet",
        _regular_tet,
        "Nédélec edge-element local matrices for a regular tetrahedron "
        "(unit edge length, centered at origin). Same geometry as the P1 "
        "regular_tet fixture; included so the two reference sets can be "
        "compared on identical tets.",
        {"k_local": 1.0e-3, "m_local": 1.0e-5, "signed_volume": 1.0e-5},
    ),
    (
        "anisotropic_well_shaped",
        _anisotropic_tet,
        "Nédélec edge-element local matrices for the reference tet scaled "
        "by (1, 0.5, 0.25) per axis — anisotropic but well-shaped. K "
        "entries scale up as the smaller axis shrinks.",
        {"k_local": 1.0e-3, "m_local": 1.0e-6, "signed_volume": 1.0e-6},
    ),
    (
        "near_degenerate_sliver",
        _sliver_tet,
        "Nédélec edge-element local matrices for a near-coplanar sliver tet "
        "(v3.z = 1e-6) — small but strictly positive volume. Stress case "
        "for numerical stability of the closed-form K/M; K entries reach "
        "O(1e6) and M entries O(1e4) here as 1/|det| amplifies the gram "
        "values through both the curl-curl gradient products and the "
        "Whitney mass (V/20) G_pq = gg_pq / (120 |det|) lift.",
        # K entries max O(1.3e6); 5e-5 * 1.3e6 ~ 65 → use 1e2 absolute.
        # M entries max O(2.6e4); 5e-5 * 2.6e4 ~ 1.3 → use 1e1 absolute.
        # V is O(1.7e-7); 1e-11 is ~6e-5 relative.
        {"k_local": 1.0e2, "m_local": 1.0e1, "signed_volume": 1.0e-11},
    ),
    (
        "inverted_tet",
        _inverted_tet,
        "Nédélec edge-element local matrices for the reference tet with "
        "vertices 2 and 3 swapped — same |V| as canonical_reference_tet "
        "but negative signed volume. K and M are computed from V = |det|/6 "
        "(the absolute volume), so the K and M entries are the *same shape* "
        "as the canonical case modulo edge-index permutation; the signed "
        "volume is the only diagnostic that flips sign.",
        {"k_local": 1.0e-4, "m_local": 1.0e-5, "signed_volume": 1.0e-5},
    ),
]


def _git_commit() -> str:
    """Return current git HEAD short SHA, or 'unknown'."""
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def _nested(a: np.ndarray) -> list:
    return a.tolist()


def main():
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)
    sha = _git_commit()

    summary = []
    for slug, builder, description, tols in CASES:
        verts = builder()
        coords = verts[None, :, :]  # (1, 4, 3)
        k, m, v = batched_nedelec_local_matrices(coords)

        # Pin the local-edge-sign vector for this case. G.1 always uses
        # all-+1 (lower-vertex-to-higher); G.2 will introduce non-trivial
        # signs when the global edge-table builder is exercised.
        edge_signs = np.ones((1, 6), dtype=np.float64)

        fixture = {
            "schema_version": "1",
            "fixture_id": f"nedelec_local/{slug}",
            "description": description,
            "units": "dimensionless",
            "inputs": {
                "coords": {
                    "shape": [1, 4, 3],
                    "dtype": "f64",
                    "description": (
                        "Per-element vertex coordinates "
                        "[n_elem, 4 vertices, 3 spatial dims]."
                    ),
                    "data": _nested(coords),
                },
                "tet_local_edge_signs": {
                    "shape": [1, 6],
                    "dtype": "f64",
                    "description": (
                        "Per-element, per-edge sign s_i in {+1, -1} recording "
                        "whether each local edge's orientation (lower local "
                        "vertex index → higher) agrees with the global edge "
                        "direction. NumPy reference always uses +1 for all "
                        "six edges; the Rust harness multiplies the Burn "
                        "output's [i, j] entry by s_i * s_j before comparing "
                        "(per nedelec.rs:30-34). Edge order is canonical: "
                        "[(0,1), (0,2), (0,3), (1,2), (1,3), (2,3)]."
                    ),
                    "data": _nested(edge_signs),
                },
            },
            "outputs": {
                "k_local": {
                    "shape": [1, 6, 6],
                    "dtype": "f64",
                    "description": (
                        "Local curl-curl K_{ij} = 4 V (G_ac G_bd - G_ad G_bc) "
                        "for the 6 edges in TET_LOCAL_EDGES order. Tolerance "
                        "is loose absolute (f32-friendly tripwire); the Rust "
                        "comparator layers a backend-aware mixed abs/rel "
                        "check on top."
                    ),
                    "tolerance_abs": tols["k_local"],
                    "data": _nested(k),
                },
                "m_local": {
                    "shape": [1, 6, 6],
                    "dtype": "f64",
                    "description": (
                        "Local Whitney 1-form mass M_{ij} per "
                        "nedelec.rs:67-74. Same edge order and sign "
                        "convention as k_local."
                    ),
                    "tolerance_abs": tols["m_local"],
                    "data": _nested(m),
                },
                "signed_volume": {
                    "shape": [1],
                    "dtype": "f64",
                    "description": (
                        "Signed element volume V = det(J)/6. Negative for "
                        "inverted tets; use |signed_volume| for assembly "
                        "weighting."
                    ),
                    "tolerance_abs": tols["signed_volume"],
                    "data": [float(v[0])],
                },
            },
            "provenance": {
                "source": (
                    "reference/numpy/nedelec_local_matrices.py via "
                    f"reference/numpy/gen_nedelec_local_per_case.py @ {sha}"
                ),
                "verified_against": (
                    "crates/geode-validation/tests/"
                    "nedelec_local_numpy_reference.rs"
                ),
                "issue": "#117 (Epic #88 Phase G.1)",
            },
        }

        out_path = FIXTURE_DIR / f"{slug}.json"
        with open(out_path, "w") as f:
            json.dump(fixture, f, indent=2, sort_keys=False)
            f.write("\n")

        signed_v = float(v[0])
        summary.append(
            (slug, out_path, signed_v, float(k.max()), float(np.abs(m).max()), os.path.getsize(out_path))
        )

    print(f"Wrote {len(summary)} per-case canonical fixtures to {FIXTURE_DIR}")
    for slug, path, signed_v, k_max, m_max, size in summary:
        print(
            f"  - {slug:30s} V_signed = {signed_v:+.6e}  "
            f"max|K| = {k_max:.3e}  max|M| = {m_max:.3e}  ({size} bytes)"
        )


if __name__ == "__main__":
    main()
