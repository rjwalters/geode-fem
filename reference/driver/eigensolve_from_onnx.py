"""Eigensolve driver for the ONNX cube-cavity sidecar (Epic #88 / Phase F.2 / issue #123).

Near-clone of `eigensolve_from_tfjava.py`. The ONNX assembly graph
(``reference/onnx/cube_cavity/assembly_graph.py``) closes the assembly
spine at the Dirichlet boundary and emits a schema-v1 JSON sidecar with
``K_int``/``M_int``. This script picks up that sidecar and runs the
eigensolve via SciPy — identical seam to TF-Java's, identical schema.

Per the F.2 curator pass on issue #123, this is a near-clone rather
than a consolidated `eigensolve_from_sidecar.py`. The generalization
into a backend-agnostic driver is filed as a separate follow-up issue,
gated on having two existing call sites to factor from.

Usage
=====
    python3 reference/driver/eigensolve_from_onnx.py \\
        path/to/reduced_kM.json \\
        [--k 5] [--dense] [--out path/to/eigenresult.json]

The output JSON is in fixture-schema v1 so the harness can compare it
to the JAX / NumPy baselines without language-specific code paths
(see `compare_eigenvalues.py`).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np


def _flatten_to_array(field, dtype=np.float64):
    """Per the fixture schema, fields may be nested or flat; this
    flattens to a 1-D ndarray of the declared dtype."""
    if isinstance(field, dict):
        data = field["data"]
        shape = field["shape"]
    else:
        raise ValueError(f"unexpected field shape: {type(field)}")
    arr = np.asarray(data, dtype=dtype).ravel()
    expected = int(np.prod(shape))
    if arr.size != expected:
        raise ValueError(
            f"data length {arr.size} does not match shape {shape} (= {expected})"
        )
    return arr.reshape(shape)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("sidecar", help="Path to the ONNX reduced_kM.json sidecar.")
    parser.add_argument("--k", type=int, default=5,
                        help="Number of lowest eigenmodes to extract.")
    parser.add_argument("--dense", action="store_true",
                        help="Force dense eigh (else auto-select by problem size).")
    parser.add_argument("--out", default="eigenresult.json")
    args = parser.parse_args()

    sidecar_path = Path(args.sidecar)
    if not sidecar_path.exists():
        print(f"Sidecar not found: {sidecar_path}", file=sys.stderr)
        sys.exit(2)
    with open(sidecar_path) as f:
        sidecar = json.load(f)

    k_int = _flatten_to_array(sidecar["outputs"]["k_int"], dtype=np.float64)
    m_int = _flatten_to_array(sidecar["outputs"]["m_int"], dtype=np.float64)
    n_int = k_int.shape[0]
    if k_int.shape != (n_int, n_int) or m_int.shape != (n_int, n_int):
        print(f"Expected square matrices, got K {k_int.shape}, M {m_int.shape}",
              file=sys.stderr)
        sys.exit(3)

    n = int(sidecar["inputs"]["n"]["data"][0])
    side = float(sidecar["inputs"]["side"]["data"][0])
    print(f"Loaded ONNX sidecar: n={n}, side={side}, n_int={n_int}")
    print(f"  trace(K_int) = {np.trace(k_int):.12e}")
    print(f"  trace(M_int) = {np.trace(m_int):.12e}")

    if args.dense or n_int < 30:
        from scipy.linalg import eigh
        eigvals, eigvecs = eigh(k_int, m_int)
        eigvals = eigvals[:args.k]
        eigvecs = eigvecs[:, :args.k]
        solver = "scipy.linalg.eigh (dense)"
    else:
        import scipy.sparse as sp
        import scipy.sparse.linalg as spla
        k_sp = sp.csr_matrix(k_int)
        m_sp = sp.csr_matrix(m_int)
        eigvals, eigvecs = spla.eigsh(k_sp, k=args.k, M=m_sp, sigma=0.0, which="LM")
        order = np.argsort(eigvals)
        eigvals = eigvals[order]
        eigvecs = eigvecs[:, order]
        solver = "scipy.sparse.linalg.eigsh (ARPACK, shift-invert sigma=0)"

    print(f"Solver: {solver}")
    print("Lowest eigenvalues:")
    for i, lam in enumerate(eigvals):
        print(f"  λ[{i}] = {lam:.6e}")

    # Build fixture-schema-shaped output for harness comparison.
    result_fixture = {
        "schema_version": "1",
        "fixture_id": f"cube_cavity/n{n}_onnx_eigensolve",
        "description": (
            "Eigenvalues from the ONNX assembly + SciPy eigensolve seam. "
            "Cross-checked against the JAX and NumPy baselines; see Epic "
            "#88 Phase F.2 (issue #123)."
        ),
        "units": "dimensionless",
        "inputs": {
            "n": {"shape": [1], "dtype": "i64", "description": "Cells per side.", "data": [n]},
            "side": {"shape": [1], "dtype": "f64", "description": "Cube side.", "data": [side]},
        },
        "outputs": {
            "eigenvalues": {
                "shape": [args.k],
                "dtype": "f64",
                "description": (
                    "Lowest 5 scalar Helmholtz eigenvalues from ONNX assembly "
                    "+ SciPy eigensolve. Cross-language drift tolerance is "
                    "1e-8 absolute (consistent with the JAX baseline tolerance)."
                ),
                "tolerance_abs": 1.0e-8,
                "data": eigvals.tolist(),
            },
        },
        "provenance": {
            "source": (
                "reference/onnx/cube_cavity (assembly) -> "
                "reference/driver/eigensolve_from_onnx.py (eigensolve seam)"
            ),
            "verified_against": "reference/jax/cube_cavity.py and reference/numpy/cube_cavity_minimal.py",
            "issue": "#123",
        },
    }
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(result_fixture, f, indent=2)
        f.write("\n")
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
