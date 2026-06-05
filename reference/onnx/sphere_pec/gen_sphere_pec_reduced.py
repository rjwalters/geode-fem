"""Driver: build the ONNX sphere-PEC graph, run it, emit a schema-v1 sidecar.

Epic #88, Phase G.7 (issue #140). Mirror of
`reference/tf_java/sphere_pec/.../SpherePecMain.java` and the F.2
`reference/onnx/cube_cavity/gen_cube_cavity_reduced.py`, with the
Nédélec-specific host-computed topology inputs that the G.6 audit (PR
#138) settled on.

Per the audit:

  * ``build_edges`` (deduplication + inverse map + sign fill) is the
    *secretly-imperative* L4 escape on the Nédélec spine. It runs
    host-side via ``reference/numpy/sphere_pec.build_edges`` and its
    outputs — ``edges``, ``tet_edge_idx``, ``tet_edge_sign`` — are
    passed in as graph inputs.
  * ``epsilon_r`` (per-tet permittivity) is host-computed via
    ``reference/numpy/sphere_pec.build_epsilon_r``. This is technically
    graph-expressible (Equal + Where, Stage 1) but is deferred to host
    for symmetry with the TF-Java driver and to avoid adding the tag
    table as a separate graph input.
  * ``interior_idx`` is host-computed via ``np.flatnonzero(pec_mask)``
    (Stage 4b: NonZero is graph-expressible but introduces data-dependent
    shape; the audit's recommended design hosts it).

The resulting sidecar is consumed by the existing sphere-PEC eigensolve
driver ``reference/driver/eigensolve_sphere_pec_sidecar.py``, which
applies the shift-and-invert ARPACK eigensolve + spurious filter (the
shared L4 friction across every backend — there is no sparse generalized
eigensolver in ONNX).

Usage
=====
    python3 reference/onnx/sphere_pec/gen_sphere_pec_reduced.py \\
        --mesh reference/fixtures/sphere_pec/sphere.msh \\
        --n-index 1.5 \\
        --r-buffer 2.0 \\
        --out target/out/reduced_kM_sphere_pec_onnx.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import onnx
import onnxruntime as ort

HERE = Path(__file__).resolve().parent
REFERENCE_ROOT = HERE.parent.parent  # reference/
sys.path.insert(0, str(REFERENCE_ROOT / "numpy"))
sys.path.insert(0, str(HERE))

from sphere_pec import (  # noqa: E402
    R_BUFFER,
    build_edges,
    build_epsilon_r,
    read_sphere_fixture,
    sphere_pec_interior_edges,
)

from assembly_graph import build_sphere_pec_graph  # noqa: E402


def _input_field(value_list, shape, dtype, description):
    return {
        "shape": list(shape),
        "dtype": dtype,
        "description": description,
        "data": list(value_list),
    }


def _output_field(arr: np.ndarray, dtype: str, description: str,
                  tolerance_abs: float) -> dict:
    return {
        "shape": list(arr.shape) if hasattr(arr, "shape") else [1],
        "dtype": dtype,
        "description": description,
        "tolerance_abs": tolerance_abs,
        "data": arr.ravel().tolist() if hasattr(arr, "ravel") else [float(arr)],
    }


def _scalar_output(value: float, dtype: str, description: str,
                   tolerance_abs: float) -> dict:
    return {
        "shape": [1],
        "dtype": dtype,
        "description": description,
        "tolerance_abs": tolerance_abs,
        "data": [float(value)],
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "ONNX sphere-PEC Nédélec partial-assembly driver. Builds the "
            "ONNX graph, runs onnxruntime with host-computed topology, "
            "emits a schema-v1 sidecar consumed by "
            "reference/driver/eigensolve_sphere_pec_sidecar.py."
        )
    )
    default_mesh = (
        REFERENCE_ROOT / "fixtures" / "sphere_pec" / "sphere.msh"
    )
    parser.add_argument(
        "--mesh",
        type=Path,
        default=default_mesh,
        help=(
            "Path to the Gmsh .msh fixture "
            "(default reference/fixtures/sphere_pec/sphere.msh)."
        ),
    )
    parser.add_argument(
        "--n-index",
        type=float,
        default=1.5,
        help="Refractive index inside the dielectric sphere (default 1.5).",
    )
    parser.add_argument(
        "--r-buffer",
        type=float,
        default=R_BUFFER,
        help="Outer PEC wall radius (default R_BUFFER = 2.0).",
    )
    parser.add_argument(
        "--out",
        type=Path,
        required=True,
        help="Output JSON path for the reduced (K_int, M_int) sidecar.",
    )
    parser.add_argument(
        "--save-model",
        type=Path,
        default=None,
        help=(
            "Optional path to save the .onnx model file (for inspection / "
            "reproducibility of the graph payload)."
        ),
    )
    args = parser.parse_args()

    print(
        f"[onnx-sphere-pec] onnx={onnx.__version__}  "
        f"onnxruntime={ort.__version__}  mesh={args.mesh}  "
        f"n_index={args.n_index}  r_buffer={args.r_buffer}"
    )

    # ---- Mesh I/O + host-computed topology ----
    fixture = read_sphere_fixture(args.mesh)
    n_nodes = fixture.n_nodes
    n_tets = fixture.n_tets

    epsilon_r = build_epsilon_r(fixture.tet_physical_tags, n_inside=args.n_index)

    # Stage 2 (audit: NOT EXPRESSIBLE) — host-side dedup + inverse map.
    edges, tet_edge_idx, tet_edge_sign = build_edges(fixture.tets)
    n_edges = int(edges.shape[0])
    tet_edge_sign_f64 = tet_edge_sign.astype(np.float64)

    # Stage 4b (audit: caveat) — host-side NonZero on the PEC mask.
    interior_mask, on_boundary_np = sphere_pec_interior_edges(
        fixture.nodes, edges, r_outer=args.r_buffer
    )
    interior_idx = np.flatnonzero(interior_mask).astype(np.int64)
    n_int = int(interior_idx.size)

    print(
        f"[onnx-sphere-pec] n_nodes={n_nodes}, n_tets={n_tets}, "
        f"n_edges={n_edges}, n_int={n_int}"
    )

    # ---- Build the ONNX graph for this connectivity ----
    model = build_sphere_pec_graph(
        n_nodes=n_nodes,
        n_tets=n_tets,
        n_edges=n_edges,
        r_outer=args.r_buffer,
    )
    onnx.checker.check_model(model)
    print("[onnx-sphere-pec] onnx.checker.check_model: OK")

    if args.save_model is not None:
        args.save_model.parent.mkdir(parents=True, exist_ok=True)
        with open(args.save_model, "wb") as f:
            f.write(model.SerializeToString())
        print(f"[onnx-sphere-pec] Saved model to {args.save_model}")

    # ---- Run via onnxruntime ----
    sess = ort.InferenceSession(model.SerializeToString())
    outputs = sess.run(
        ["K_int", "M_int", "k_global", "m_global", "on_boundary"],
        {
            "nodes": fixture.nodes.astype(np.float64),
            "tets": fixture.tets.astype(np.int64),
            "edges": edges.astype(np.int64),
            "tet_edge_idx": tet_edge_idx.astype(np.int64),
            "tet_edge_sign": tet_edge_sign_f64,
            "epsilon_r": epsilon_r.astype(np.float64),
            "interior_idx": interior_idx,
        },
    )
    K_int, M_int, k_global, m_global, on_boundary_onnx = outputs

    assert K_int.shape == (n_int, n_int), K_int.shape
    assert M_int.shape == (n_int, n_int), M_int.shape

    # ---- Sanity readouts (sub-stage cross-check vs NumPy) ----
    tr_k = float(np.trace(K_int))
    tr_m = float(np.trace(M_int))
    frob_k = float(np.linalg.norm(K_int, ord="fro"))
    frob_m = float(np.linalg.norm(M_int, ord="fro"))
    print(f"[onnx-sphere-pec] trace(K_int)     = {tr_k:.12e}")
    print(f"[onnx-sphere-pec] trace(M_int)     = {tr_m:.12e}")
    print(f"[onnx-sphere-pec] Frobenius(K_int) = {frob_k:.12e}")
    print(f"[onnx-sphere-pec] Frobenius(M_int) = {frob_m:.12e}")

    # PEC mask cross-check (audit Stage 4a verdict).
    if not np.array_equal(on_boundary_onnx, on_boundary_np):
        n_disagree = int(np.sum(on_boundary_onnx != on_boundary_np))
        print(
            f"[onnx-sphere-pec] WARNING: on_boundary mask disagrees with "
            f"NumPy reference on {n_disagree} of {n_nodes} nodes"
        )
    else:
        print("[onnx-sphere-pec] on_boundary mask matches NumPy reference")

    # ---- Build the sidecar ----
    fixture_dict = {
        "schema_version": "1",
        "fixture_id": "sphere_pec/n774_pec_eigenmode_onnx",
        "description": (
            "ONNX partial-assembly reference for the vector-Nédélec "
            "sphere-PEC eigenmode pipeline (Epic #88 Phase G.7 / issue #140). "
            "The ONNX graph emits (K_int, M_int) directly via the host-"
            "computed-topology design that the G.6 audit (PR #138) "
            "recommended: edges, tet_edge_idx, tet_edge_sign, epsilon_r, "
            "and interior_idx are all host-computed (build_edges is the "
            "secretly-imperative L4 escape), then fed in as graph inputs. "
            "The eigensolve is delegated to SciPy via "
            "reference/driver/eigensolve_sphere_pec_sidecar.py (ONNX has "
            "no sparse generalized eigensolver — shared L4 friction "
            "across all backends)."
        ),
        "units": "lambda = k^2 (inverse-length squared); dimensionless mesh coordinates",
        "inputs": {
            "mesh_path": _input_field(
                [str(args.mesh)], [1], "str",
                "Path to the bundled sphere.msh fixture.",
            ),
            "n_index": _input_field(
                [args.n_index], [1], "f64",
                "Refractive index inside the dielectric sphere; "
                "epsilon_r = n^2 inside.",
            ),
            "r_buffer": _input_field(
                [args.r_buffer], [1], "f64",
                "Outer PEC wall radius (= R_BUFFER).",
            ),
            "n_int": _input_field(
                [n_int], [1], "i64",
                "Interior edge count (DOFs after PEC elimination).",
            ),
            "interior_idx": _input_field(
                interior_idx.tolist(), [n_int], "i64",
                "Interior-DOF row/col indices into the full (n_edges, "
                "n_edges) matrix. Host-computed via "
                "np.flatnonzero(pec_mask); the audit (Stage 4b) "
                "classified NonZero as graph-expressible but data-"
                "dependent-shape, so the recommended design hosts the "
                "idx derivation.",
            ),
            # eigensolve_sphere_pec_sidecar.py reads `n` / `side` aliases.
            "n": _input_field(
                [n_int], [1], "i64",
                "Interior DOF count (= n_int). Alias for sidecar driver "
                "compatibility.",
            ),
            "side": _input_field(
                [args.r_buffer], [1], "f64",
                "Outer PEC wall radius (= r_buffer). Alias for sidecar "
                "driver compatibility.",
            ),
        },
        "outputs": {
            "n_nodes": _scalar_output(
                float(n_nodes), "f64",
                "Number of mesh nodes. Strict equality.",
                0.5,
            ),
            "n_tets": _scalar_output(
                float(n_tets), "f64",
                "Number of mesh tets. Strict equality.",
                0.5,
            ),
            "n_edges": _scalar_output(
                float(n_edges), "f64",
                "Total global edge count (before PEC elimination). "
                "Host-computed via build_edges (audit Stage 2b: NOT "
                "EXPRESSIBLE in ONNX — secretly-imperative L4 escape).",
                0.5,
            ),
            "n_interior_edges": _scalar_output(
                float(n_int), "f64",
                "Interior edge count (DOFs after PEC elimination).",
                0.5,
            ),
            "k_diag_sum": _scalar_output(
                tr_k, "f64",
                "trace(K_int) — ONNX assembly readback.",
                1.0e-6,
            ),
            "m_diag_sum": _scalar_output(
                tr_m, "f64",
                "trace(M_int) — ONNX assembly readback.",
                1.0e-6,
            ),
            "k_int_frobenius": _scalar_output(
                frob_k, "f64",
                "Frobenius norm of K_int. Acceptance criterion: 1e-6 "
                "absolute vs the NumPy baseline (G.7 AC, same as F.2).",
                1.0e-6,
            ),
            "m_int_frobenius": _scalar_output(
                frob_m, "f64",
                "Frobenius norm of M_int. Acceptance criterion: 1e-6 "
                "absolute vs the NumPy baseline (G.7 AC, same as F.2).",
                1.0e-6,
            ),
            "k_int_diag": _output_field(
                np.diag(K_int).astype(np.float64), "f64",
                "Diagonal of K_int (per-DOF stiffness).",
                1.0e-8,
            ),
            "m_int_diag": _output_field(
                np.diag(M_int).astype(np.float64), "f64",
                "Diagonal of M_int (per-DOF mass).",
                1.0e-8,
            ),
            "k_int": _output_field(
                K_int, "f64",
                "Dirichlet-reduced Nédélec curl-curl stiffness matrix.",
                1.0e-8,
            ),
            "m_int": _output_field(
                M_int, "f64",
                "Dirichlet-reduced epsilon-scaled Nédélec mass matrix.",
                1.0e-8,
            ),
        },
        "provenance": {
            "source": (
                "reference/onnx/sphere_pec/assembly_graph.py via "
                "gen_sphere_pec_reduced.py (Epic #88 Phase G.7 / "
                "issue #140)"
            ),
            "verified_against": (
                "reference/numpy/sphere_pec.py and "
                "reference/fixtures/sphere_pec/baseline.json"
            ),
            "issue": "#140",
            "audit": "reference/onnx/audit/sphere_pec/nedelec_operator_audit.md",
            "note": (
                "Host-computed-topology design. The ONNX graph consumes "
                "edges, tet_edge_idx, tet_edge_sign, epsilon_r, and "
                "interior_idx as inputs because build_edges deduplication "
                "is the secretly-imperative L4 escape on the Nédélec "
                "spine (audit Stage 2b verdict)."
            ),
        },
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(fixture_dict, f, indent=2)
        f.write("\n")
    print(f"[onnx-sphere-pec] Wrote {args.out}")

    # Suppress unused-variable warnings from optional outputs.
    _ = k_global
    _ = m_global
    return 0


if __name__ == "__main__":
    sys.exit(main())
