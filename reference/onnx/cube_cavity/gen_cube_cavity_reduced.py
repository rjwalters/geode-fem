"""Driver: build the ONNX cube-cavity graph, run it, emit a schema-v1 sidecar.

Epic #88, Phase F.2 (issue #123). Mirror of
``reference/tf_java/cube_cavity/.../CubeCavityMain.java`` — programmatic
mesh, ``onnxruntime`` over the static graph, JSON sidecar in the same
schema the TF-Java driver emits and that
``reference/driver/eigensolve_from_onnx.py`` consumes.

Per the F.1 audit's recommendation, the Dirichlet ``idx`` is computed
host-side via ``np.where(mask)[0]`` and fed into the graph as an int64
input. This keeps the assembly graph statically shaped end-to-end (the
audit doc lines 146–152 are the source of truth here).

Usage
=====
    python3 reference/onnx/cube_cavity/gen_cube_cavity_reduced.py \\
        --n 10 --side 1.0 --out target/out/reduced_kM.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[3])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

from reference.numpy.mesh import cube_interior_mask, cube_tet_mesh  # noqa: E402
from reference.onnx.cube_cavity.assembly_graph import build_cube_cavity_graph  # noqa: E402


def _scalar_input_field(value, dtype: str, description: str) -> dict:
    return {
        "shape": [1],
        "dtype": dtype,
        "description": description,
        "data": [value],
    }


def _matrix_output_field(arr: np.ndarray, description: str,
                         tolerance_abs: float) -> dict:
    return {
        "shape": list(arr.shape),
        "dtype": "f64",
        "description": description,
        "tolerance_abs": tolerance_abs,
        "data": arr.ravel().tolist(),
    }


def _scalar_output_field(value: float, description: str,
                         tolerance_abs: float) -> dict:
    return {
        "shape": [1],
        "dtype": "f64",
        "description": description,
        "tolerance_abs": tolerance_abs,
        "data": [value],
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--n", type=int, default=10,
                        help="Cells per side (default 10, matches baseline.json).")
    parser.add_argument("--side", type=float, default=1.0,
                        help="Cube edge length (default 1.0).")
    parser.add_argument("--out", type=Path, required=True,
                        help="Output JSON path for the reduced (K_int, M_int) sidecar.")
    args = parser.parse_args()

    n = args.n
    side = args.side
    print(f"[onnx-cube-cavity] onnx={onnx.__version__}  "
          f"onnxruntime={ort.__version__}  n={n}  side={side}")

    # ---- Programmatic mesh + interior mask (same convention as TF-Java) ----
    nodes_np, tets_np = cube_tet_mesh(n, side)
    n_nodes = int(nodes_np.shape[0])
    n_elem = int(tets_np.shape[0])
    mask = cube_interior_mask(nodes_np, side)
    idx_np = np.where(mask)[0].astype(np.int64)
    n_int = int(idx_np.size)
    print(f"[onnx-cube-cavity] n_nodes={n_nodes}, n_elem={n_elem}, n_int={n_int}")

    # ---- Build the ONNX graph for this connectivity ----
    model = build_cube_cavity_graph(n_nodes=n_nodes, n_elem=n_elem)
    onnx.checker.check_model(model)
    print("[onnx-cube-cavity] onnx.checker.check_model: OK")

    # ---- Run via onnxruntime ----
    sess = ort.InferenceSession(model.SerializeToString())
    K_int, M_int = sess.run(
        ["K_int", "M_int"],
        {
            "nodes": nodes_np.astype(np.float64),
            "tets": tets_np.astype(np.int64),
            "idx_int": idx_np,
        },
    )
    assert K_int.shape == (n_int, n_int), K_int.shape
    assert M_int.shape == (n_int, n_int), M_int.shape

    tr_k = float(np.trace(K_int))
    tr_m = float(np.trace(M_int))
    print(f"[onnx-cube-cavity] trace(K_int) = {tr_k:.12e}")
    print(f"[onnx-cube-cavity] trace(M_int) = {tr_m:.12e}")

    # ---- Build the sidecar (schema v1, mirroring CubeCavityMain.java) ----
    fixture = {
        "schema_version": "1",
        "fixture_id": f"cube_cavity/n{n}_onnx_reduced",
        "description": (
            "Dirichlet-reduced (K_int, M_int) matrices for the unit-cube "
            "scalar Helmholtz problem, produced by the ONNX static-graph "
            "assembly pipeline (Epic #88 Phase F.2 / issue #123). Consumed "
            "by reference/driver/eigensolve_from_onnx.py to close the "
            "eigenproblem at the same SciPy seam TF-Java uses."
        ),
        "units": "dimensionless",
        "inputs": {
            "n": _scalar_input_field(n, "i64", "Cells per side."),
            "side": _scalar_input_field(side, "f64", "Cube edge length."),
            "interior_idx": {
                "shape": [n_int],
                "dtype": "i64",
                "description": (
                    "Interior-DOF row/col indices into the full (n_nodes, "
                    "n_nodes) matrix. Computed host-side per the F.1 audit "
                    "(NonZero would lower but introduce data-dependent "
                    "shape; see audit doc lines 146-152)."
                ),
                "data": idx_np.tolist(),
            },
        },
        "outputs": {
            "k_diag_sum": _scalar_output_field(
                tr_k,
                "trace(K_int) -- ONNX assembly readback.",
                1.0e-12,
            ),
            "m_diag_sum": _scalar_output_field(
                tr_m,
                "trace(M_int) -- ONNX assembly readback.",
                1.0e-12,
            ),
            "k_int": _matrix_output_field(
                K_int,
                "Dirichlet-reduced stiffness matrix.",
                1.0e-10,
            ),
            "m_int": _matrix_output_field(
                M_int,
                "Dirichlet-reduced mass matrix.",
                1.0e-10,
            ),
        },
        "provenance": {
            "source": (
                "reference/onnx/cube_cavity/assembly_graph.py via "
                "gen_cube_cavity_reduced.py (Epic #88 Phase F.2 / "
                "issue #123)"
            ),
            "verified_against": (
                "reference/numpy/cube_cavity_minimal.py and "
                "reference/jax/cube_cavity.py"
            ),
            "issue": "#123",
            "audit": "reference/onnx/audit/cube_cavity_operator_audit.md",
        },
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"[onnx-cube-cavity] Wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
