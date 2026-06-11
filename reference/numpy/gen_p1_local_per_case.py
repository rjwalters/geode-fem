"""Generate per-case canonical fixtures under ``fixtures/p1_local/<case>.json``.

The five cases (per issue #90 acceptance criteria) are:

  1. Canonical reference tet ``[(0,0,0), (1,0,0), (0,1,0), (0,0,1)]``.
  2. Regular tet (vertices of a regular tetrahedron, centered at origin,
     unit edge length).
  3. Anisotropic-but-well-shaped tet (1 x 0.5 x 0.25 axis stretch on the
     reference tet — well-conditioned, but with non-unit aspect ratio).
  4. Near-degenerate sliver tet (three vertices nearly coplanar; small
     positive volume, condition number is bad but determinant is not zero).
  5. Inverted tet (vertices 2 and 3 swapped on the reference tet → the
     orientation flips, signed volume becomes negative).

Per-case canonical schema (per issue #101)
------------------------------------------
Each output file is in the canonical schema v1 documented in
``reference/SCHEMA.md`` (one fixture pins one identity). This script
replaces the legacy ``gen_p1_local_standard.py`` which emitted a
bespoke multi-case ``standard.json`` bundle.

Tolerances per field are intentionally loose absolute (catch
catastrophic regression / structural mistakes); the Rust comparator
``crates/geode-validation/tests/p1_local_numpy_reference.rs`` layers a
tighter **backend-aware mixed abs/rel** check on top to enforce the
``1e-10`` rel / ``1e-12`` abs acceptance criterion under ``ndarray``
(f64) and ``5e-5`` rel / ``1e-6`` abs under the f32 GPU backends.

Run
---
    python3 reference/numpy/gen_p1_local_per_case.py
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
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "p1_local"

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.p1_local_matrices import batched_p1_local_matrices  # noqa: E402


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


# Each tuple: (case_slug, builder_fn, description, per-field tolerance_abs)
# The per-field absolute tolerances are picked to be loose-enough-for-f32:
# `tolerance_abs ~ max(1e-6, 5e-5 * |max_entry|)` per field. The Rust
# comparator layers a tighter backend-aware mixed abs/rel check on top.
CASES = [
    (
        "canonical_reference_tet",
        _ref_tet,
        "P1 local matrices for the canonical reference tet [(0,0,0),(1,0,0),(0,1,0),(0,0,1)] — the affine identity.",
        {"k_local": 1.0e-4, "m_local": 1.0e-5, "signed_volume": 1.0e-5},
    ),
    (
        "regular_tet",
        _regular_tet,
        "P1 local matrices for a regular tetrahedron (unit edge length, centered at origin).",
        {"k_local": 1.0e-4, "m_local": 1.0e-5, "signed_volume": 1.0e-5},
    ),
    (
        "anisotropic_well_shaped",
        _anisotropic_tet,
        "P1 local matrices for the reference tet scaled by (1, 0.5, 0.25) per axis — anisotropic but well-shaped.",
        {"k_local": 1.0e-4, "m_local": 1.0e-6, "signed_volume": 1.0e-6},
    ),
    (
        "near_degenerate_sliver",
        _sliver_tet,
        "P1 local matrices for a near-coplanar sliver tet (v3.z = 1e-6) — small but strictly positive volume. Stress case for numerical stability.",
        # K entries are O(1e5), so 1e1 ≈ 5e-5 * 2e5 in the absolute sense.
        # M entries are O(1e-8); 1e-12 is ~5e-5 relative.
        # V is O(1e-7); 1e-11 is ~5e-5 relative.
        {"k_local": 1.0e1, "m_local": 1.0e-12, "signed_volume": 1.0e-11},
    ),
    (
        "inverted_tet",
        _inverted_tet,
        "P1 local matrices for the reference tet with vertices 2 and 3 swapped — same shape as canonical_reference_tet but negative signed volume. Exercises the signed-volume diagnostic path.",
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
        k, m, v = batched_p1_local_matrices(coords)

        fixture = {
            "schema_version": "1",
            "fixture_id": f"p1_local/{slug}",
            "description": description,
            "units": "dimensionless",
            "inputs": {
                "coords": {
                    "shape": [1, 4, 3],
                    "dtype": "f64",
                    "description": "Per-element vertex coordinates [n_elem, 4 vertices, 3 spatial dims].",
                    "data": _nested(coords),
                },
            },
            "outputs": {
                "k_local": {
                    "shape": [1, 4, 4],
                    "dtype": "f64",
                    "description": "Local stiffness K_{ij}. Tolerance is loose absolute (f32-friendly tripwire); the Rust comparator layers a tighter backend-aware mixed abs/rel check on top.",
                    "tolerance_abs": tols["k_local"],
                    "data": _nested(k),
                },
                "m_local": {
                    "shape": [1, 4, 4],
                    "dtype": "f64",
                    "description": "Local consistent mass M_{ij}.",
                    "tolerance_abs": tols["m_local"],
                    "data": _nested(m),
                },
                "signed_volume": {
                    "shape": [1],
                    "dtype": "f64",
                    "description": "Signed element volume V = det(J)/6.",
                    "tolerance_abs": tols["signed_volume"],
                    "data": [float(v[0])],
                },
            },
            "provenance": {
                "source": f"reference/numpy/p1_local_matrices.py via reference/numpy/gen_p1_local_per_case.py @ {sha}",
                "verified_against": "crates/geode-validation/tests/p1_local_numpy_reference.rs",
                "issue": "#90 / #101 (consolidation onto geode-validation harness)",
            },
        }

        out_path = FIXTURE_DIR / f"{slug}.json"
        with open(out_path, "w") as f:
            json.dump(fixture, f, indent=2, sort_keys=False)
            f.write("\n")

        signed_v = float(v[0])
        summary.append((slug, out_path, signed_v, os.path.getsize(out_path)))

    print(f"Wrote {len(summary)} per-case canonical fixtures to {FIXTURE_DIR}")
    for slug, path, signed_v, size in summary:
        print(f"  - {slug:30s} signed_volume = {signed_v:+.6e}  ({size} bytes)")


if __name__ == "__main__":
    main()
