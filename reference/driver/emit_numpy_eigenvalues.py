"""Emit a fixture-schema JSON of NumPy cube-cavity eigenvalues (Epic #88 / #93).

Thin CLI shim around `reference/numpy/cube_cavity_minimal.solve_cube_cavity`
that writes the result as a fixture-schema-v1 JSON, matching the shape
that `compare_eigenvalues.py` consumes. This exists so the TF-Java CI
job can derive an apples-to-apples NumPy row without pulling in the
JAX dependency stack or the larger #92 canonical baseline pipeline.

Usage
=====
    python3 reference/driver/emit_numpy_eigenvalues.py \
        [--n 4] [--side 1.0] [--k 5] [--out path/to/numpy_baseline.json]
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_REF = HERE.parent  # reference/
sys.path.insert(0, str(REPO_REF / "numpy"))

from cube_cavity_minimal import solve_cube_cavity  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--n", type=int, default=4)
    parser.add_argument("--side", type=float, default=1.0)
    parser.add_argument("--k", type=int, default=5)
    parser.add_argument("--dense", action="store_true")
    parser.add_argument("--out", type=Path, default=Path("numpy_baseline.json"))
    args = parser.parse_args()

    result = solve_cube_cavity(n=args.n, side=args.side, k=args.k, dense=args.dense)
    fixture = {
        "schema_version": "1",
        "fixture_id": f"cube_cavity/n{args.n}_numpy_minimal_eigensolve",
        "description": (
            "Lowest k cube-cavity eigenvalues from reference/numpy/cube_cavity_minimal.py. "
            "Emitted in fixture-schema form so the four-way agreement table in "
            "reference/driver/compare_eigenvalues.py can consume it without "
            "language-specific code paths (Epic #88 / #93)."
        ),
        "units": "dimensionless",
        "inputs": {
            "n":    {"shape": [1], "dtype": "i64", "description": "Cells per side.",
                     "data": [args.n]},
            "side": {"shape": [1], "dtype": "f64", "description": "Cube side.",
                     "data": [args.side]},
        },
        "outputs": {
            "eigenvalues": {
                "shape": [args.k],
                "dtype": "f64",
                "description": "Lowest k eigenvalues, ascending.",
                "tolerance_abs": 1.0e-8,
                "data": result["eigenvalues"].tolist(),
            },
            "k_diag_sum": {
                "shape": [1], "dtype": "f64",
                "description": "trace(K_int).",
                "tolerance_abs": 1.0e-12,
                "data": [result["k_diag_sum"]],
            },
            "m_diag_sum": {
                "shape": [1], "dtype": "f64",
                "description": "trace(M_int).",
                "tolerance_abs": 1.0e-12,
                "data": [result["m_diag_sum"]],
            },
        },
        "provenance": {
            "source": "reference/numpy/cube_cavity_minimal.py via reference/driver/emit_numpy_eigenvalues.py",
            "issue": "#93 / #102",
        },
    }
    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"Wrote {args.out}", file=sys.stderr)
    print(f"Eigenvalues: {result['eigenvalues'].tolist()}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
