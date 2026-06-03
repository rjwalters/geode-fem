"""Generate ``fixtures/p1_local/standard.json`` — 5-tet P1 fixture.

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

Output format
-------------
JSON, schema documented in ``fixtures/p1_local/standard.schema.md``.

Each entry stores the input vertex coords and the *precomputed NumPy
baseline* (k_local, m_local, signed_volume) so the Rust harness can
verify Burn agreement without invoking Python at test time.

Floats are serialized via ``repr(float)`` so the JSON round-trips
exactly back into f64 (no decimal-to-binary slop). NumPy's
``np.float64.hex()`` would be even safer; we use ``repr`` here because
Python's ``repr`` is documented to be round-trip-faithful for f64 since
Python 3.1 and Rust's ``serde_json`` parses such strings losslessly into
``f64``.

Run
---
    python3 reference/numpy/gen_p1_local_standard.py
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
FIXTURE_PATH = REPO_ROOT / "reference" / "fixtures" / "p1_local" / "standard.json"

# Add this dir to sys.path so we can import the sibling module under the
# same name regardless of cwd.
sys.path.insert(0, str(HERE))
from p1_local_matrices import batched_p1_local_matrices  # noqa: E402


def _ref_tet():
    """Canonical reference tet — the affine map identity."""
    return np.array(
        [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        dtype=np.float64,
    )


def _regular_tet():
    """Regular tetrahedron, unit edge length, centered at origin.

    Vertices placed at the standard regular-tet coordinates (4 of the 8
    cube corners that form a regular tetrahedron), then scaled so all
    edges have length 1 and translated so the centroid is the origin.

    Vertex order chosen so signed volume is positive (right-handed):
    swapping any two flips the sign.
    """
    raw = np.array(
        [
            [1.0, 1.0, 1.0],
            [-1.0, -1.0, 1.0],
            [-1.0, 1.0, -1.0],
            [1.0, -1.0, -1.0],
        ],
        dtype=np.float64,
    )
    # raw has edge length 2*sqrt(2); scale so edges have length 1.
    raw /= 2.0 * math.sqrt(2.0)
    # centroid is already (0,0,0); no translation needed.
    return raw


def _anisotropic_tet():
    """Reference tet stretched anisotropically (1, 0.5, 0.25).

    Well-shaped but with non-unit aspect ratio; non-degenerate Jacobian.
    """
    base = _ref_tet()
    scale = np.array([1.0, 0.5, 0.25], dtype=np.float64)
    return base * scale  # broadcasts (4,3) * (3,) elementwise


def _sliver_tet():
    """Near-degenerate sliver tet.

    Three vertices nearly coplanar (v0, v1, v2 in the xy plane); v3 is
    just *barely* lifted out of the plane (z = 1e-6). Determinant is
    small but strictly positive — a stress case for numerical stability,
    not a degenerate case.
    """
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
    """Reference tet with vertices 2 and 3 swapped → orientation flips.

    The same vertex set as the canonical reference tet (so the *unsigned*
    volume is 1/6 and |K|, M are identical), but in a vertex order that
    yields a *negative* signed volume. This is the "mesh-quality
    diagnostic only" path through the P1 code — assembly contributions
    use |det| but the signed_volume readback must be negative.
    """
    return np.array(
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],  # was v3
            [0.0, 1.0, 0.0],  # was v2
        ],
        dtype=np.float64,
    )


CASES = [
    ("canonical_reference_tet", _ref_tet, "Canonical reference tet — the affine identity."),
    ("regular_tet", _regular_tet, "Regular tetrahedron, unit edge length, centered."),
    (
        "anisotropic_well_shaped",
        _anisotropic_tet,
        "Reference tet scaled by (1, 0.5, 0.25) per axis — anisotropic but well-shaped.",
    ),
    (
        "near_degenerate_sliver",
        _sliver_tet,
        "Near-coplanar tet with v3.z = 1e-6 — small but strictly positive volume.",
    ),
    (
        "inverted_tet",
        _inverted_tet,
        "Reference tet with v2/v3 swapped — same shape, negative signed volume.",
    ),
]


def _git_commit() -> str:
    """Return the current git HEAD short SHA, or 'unknown' if git is unavailable."""
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def _array_to_nested_list(a: np.ndarray) -> list:
    """Convert an ndarray to a JSON-safe nested list of Python floats.

    Uses Python's default float repr (round-trip-faithful for f64 per
    Python 3.1+) so the fixture loads back into identical bit patterns.
    """
    return a.tolist()


def main():
    cases_out = []
    for name, builder, description in CASES:
        verts = builder()
        coords = verts[None, :, :]  # (1, 4, 3) batch of size 1
        k, m, v = batched_p1_local_matrices(coords)
        cases_out.append(
            {
                "name": name,
                "description": description,
                "input": {
                    "vertices": _array_to_nested_list(verts),
                },
                "reference": {
                    "numpy": {
                        "k_local": _array_to_nested_list(k[0]),
                        "m_local": _array_to_nested_list(m[0]),
                        "signed_volume": float(v[0]),
                    },
                },
            }
        )

    fixture = {
        "meta": {
            "slice": "p1_local",
            "schema_version": 1,
            "generator": "reference/numpy/gen_p1_local_standard.py",
            "generator_commit": _git_commit(),
            "numpy_version": np.__version__,
            "python_version": "%d.%d.%d" % sys.version_info[:3],
            "issue": 90,
            "epic": 88,
            "note": (
                "Schema is documented in fixtures/p1_local/standard.schema.md. "
                "Floats are decimal-serialized; Python repr round-trips f64 "
                "losslessly since 3.1, and Rust serde_json parses such strings "
                "back into identical f64 bit patterns."
            ),
        },
        "cases": cases_out,
    }

    FIXTURE_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2, sort_keys=False)
        f.write("\n")  # trailing newline for tidy diffs

    print(f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes)")
    print(f"  Cases: {len(cases_out)}")
    for case in cases_out:
        v = case["reference"]["numpy"]["signed_volume"]
        print(f"    - {case['name']:30s} signed_volume = {v:+.6e}")


if __name__ == "__main__":
    main()
