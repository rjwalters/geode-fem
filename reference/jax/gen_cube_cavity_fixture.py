"""Generate `reference/fixtures/cube_cavity/jax_baseline.json` from JAX.

Produces a `schema_version: "1"` fixture (compatible with
`crates/geode-validation/src/fixture.rs::Fixture`) containing the lowest
5 eigenvalues from the JAX cube-cavity pipeline, plus the structural
sanity outputs (interior-DOF trace of K and M) that the Rust harness
will cross-check the JAX impl against.

Why JAX produces the fixture
============================

This PR ships in parallel with #92, which produces the canonical NumPy
baseline. To keep #93 honest (per the issue body's three-option
coordination plan), #93 lands its own JAX-produced baseline. Once #92
merges, a follow-up PR replaces this JAX-produced fixture with the
NumPy-produced canonical one and the JAX pipeline is verified against
the canonical (rather than against itself). At that point this script
becomes a regression diff: "does JAX still agree with NumPy?"

JAX numerical agreement with the in-tree NumPy reference is verified
inline at fixture-gen time so the baseline file is trustworthy.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/ -> repo root
# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.jax.cube_cavity import solve_cube_cavity_jax  # noqa: E402
from reference.numpy.cube_cavity_minimal import solve_cube_cavity  # noqa: E402


def _git_commit() -> str:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--n", type=int, default=4,
                        help="Cells per side (default 4 → 27 interior DOFs, dense eigh)")
    parser.add_argument("--side", type=float, default=1.0)
    parser.add_argument("--k", type=int, default=5)
    parser.add_argument("--out",
                        default=str(REPO_ROOT / "reference" / "fixtures"
                                    / "cube_cavity" / "jax_baseline.json"))
    args = parser.parse_args()

    print(f"Solving JAX cube-cavity (n={args.n}, side={args.side}, k={args.k})...")
    jx = solve_cube_cavity_jax(n=args.n, side=args.side, k=args.k)

    print(f"  n_interior_dofs = {jx.n_dofs_interior}, n_tets = {jx.n_tets}")
    print(f"  eigenvalues = {jx.eigenvalues}")
    print(f"  trace(K_int) = {jx.k_diag_sum:.12e}")
    print(f"  trace(M_int) = {jx.m_diag_sum:.12e}")

    # Independent NumPy cross-check (sanity guard at fixture-write time).
    print("\nCross-check vs NumPy reference impl:")
    np_res = solve_cube_cavity(n=args.n, side=args.side, k=args.k)
    eig_abs = np.abs(jx.eigenvalues - np_res["eigenvalues"])
    eig_rel = eig_abs / np.maximum(np.abs(np_res["eigenvalues"]), 1.0)
    print(f"  max |JAX - NumPy| eigenvalues = {eig_abs.max():.3e}")
    print(f"  max rel(JAX, NumPy) eigenvalues = {eig_rel.max():.3e}")
    if eig_rel.max() > 1e-10:
        print("  WARNING: JAX-NumPy disagreement exceeds 1e-10. "
              "Investigate before publishing baseline.")
        sys.exit(2)

    fixture = {
        "schema_version": "1",
        "fixture_id": f"cube_cavity/n{args.n}_first_five_modes",
        "description": (
            f"Lowest {args.k} scalar Helmholtz eigenvalues of the unit cube "
            f"with homogeneous Dirichlet BC, on a programmatic cube_tet_mesh "
            f"of n={args.n} ({jx.n_dofs_interior} interior DOFs). JAX baseline "
            "ships in #93 in parallel with #92's NumPy canonical baseline; "
            "values are cross-checked at fixture-gen time."
        ),
        "units": "dimensionless (k^2 with side normalized to 1)",
        "inputs": {
            "n": {
                "shape": [1],
                "dtype": "i64",
                "description": "Cells per side of the programmatic cube_tet_mesh.",
                "data": [args.n],
            },
            "side": {
                "shape": [1],
                "dtype": "f64",
                "description": "Cube edge length.",
                "data": [args.side],
            },
        },
        "outputs": {
            "eigenvalues": {
                "shape": [args.k],
                "dtype": "f64",
                "description": (
                    f"Lowest {args.k} eigenvalues, ascending. "
                    "Cross-language f64 reproducibility allows ~1e-5 relative drift "
                    "per #88 framing; this fixture uses 1e-8 absolute since the "
                    "in-tree NumPy and JAX values agree to ~1e-13."
                ),
                "tolerance_abs": 1.0e-8,
                "data": jx.eigenvalues.tolist(),
            },
            "k_diag_sum": {
                "shape": [1],
                "dtype": "f64",
                "description": "trace(K_int) — interior-DOF stiffness diagonal sum; autodiff anchor.",
                "tolerance_abs": 1.0e-12,
                "data": [jx.k_diag_sum],
            },
            "m_diag_sum": {
                "shape": [1],
                "dtype": "f64",
                "description": "trace(M_int) — interior-DOF mass diagonal sum.",
                "tolerance_abs": 1.0e-12,
                "data": [jx.m_diag_sum],
            },
        },
        "provenance": {
            "source": "reference/jax/cube_cavity.py (Epic #88 / #93)",
            "verified_against": (
                "reference/numpy/cube_cavity_minimal.py — eigenvalues agree to "
                f"{eig_rel.max():.1e} relative at fixture-gen time"
            ),
            "issue": "#93 (cube-cavity JAX + TF-Java reference)",
        },
    }

    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"\nWrote {out_path} ({os.path.getsize(out_path)} bytes)")
    print(f"  generator_commit = {_git_commit()}")


if __name__ == "__main__":
    main()
