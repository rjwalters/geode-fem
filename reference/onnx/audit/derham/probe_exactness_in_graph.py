"""Probe: in-graph de Rham exactness ``d¹ · (d⁰ · φ) ≡ 0`` (sphere fixture).

Epic #88, Phase I.3 (issue #169), Deliverable 3. Builds a **single**
ONNX opset-18 graph that applies d⁰ then d¹ to a nodal field and
asserts the exactness identity ``d¹ ∘ d⁰ ≡ 0`` *inside the graph*,
then executes it under onnxruntime against the bundled sphere fixture
(``reference/fixtures/sphere_pec/sphere.msh``) — the same mesh behind
the #149 baseline (``reference/fixtures/derham/baseline.json``,
fixture_id ``derham/sphere_n774_d0_d1_d2``).

Graph design
============

Host-side (the L4-input boundary, never in the graph):
  - ``build_edges`` / ``build_faces`` — dedup + ordering (G.6 verdict)
  - ``face_edge_idx (n_faces, 3) int64`` — the edge_to_idx dict lookup

In-graph (all native opset 18, int64):

    g        = Gather(φ, edges[:, 1]) - Gather(φ, edges[:, 0])   # d⁰·φ
    r        = g[fei[:, 0]] + g[fei[:, 1]] - g[fei[:, 2]]        # d¹·g
    max_abs  = ReduceMax(Abs(r))                                  # scalar
    exact    = Equal(max_abs, 0)                                  # BOOL

The graph *outputs* the boolean ``exact`` — the assertion itself is a
graph node, not a host-side comparison. The host merely reads it.

A second composed graph runs the same chain through the **generic COO
matvec** form (two ScatterND(reduction="add") stages consuming the
COO triplets of ``gradient_map`` / ``curl_map`` as graph inputs),
confirming the operator-agnostic input contract composes.

dtype note (why int64, not f64)
===============================

The d-operators are integer ``{-1, 0, +1}`` matrices and the #149
exactness contract is **bit-exact at the integer level**. In int64
the in-graph identity holds exactly (integer adds are associative).
In f64, ``g = fl(φ_b − φ_a)`` is already rounded, so the telescoping
sum ``g_ab + g_bc − g_ac`` is only zero to roundoff — this probe also
runs the f64 variant and reports the measured residual to document
that the *in-graph assert* must use an integer dtype.

Run
===

    python3 reference/onnx/audit/derham/probe_exactness_in_graph.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import onnx
import onnx.checker
import onnx.helper as oh
import onnxruntime as ort
from onnx import TensorProto

HERE = Path(__file__).resolve().parent
REFERENCE_ROOT = HERE.parent.parent.parent
sys.path.insert(0, str(REFERENCE_ROOT / "numpy"))

from derham import (  # noqa: E402
    _read_msh_tets,
    build_edges,
    build_faces,
    curl_map,
    gradient_map,
)

OPSET = 18
MESH_PATH = REFERENCE_ROOT / "fixtures" / "sphere_pec" / "sphere.msh"
BASELINE_PATH = REFERENCE_ROOT / "fixtures" / "derham" / "baseline.json"

DTYPE_MAP = {
    np.dtype("float64"): TensorProto.DOUBLE,
    np.dtype("int64"): TensorProto.INT64,
    np.dtype("int32"): TensorProto.INT32,
}


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    return oh.make_node(
        "Constant",
        inputs=[],
        outputs=[name],
        value=oh.make_tensor(
            name=name + "_value",
            data_type=DTYPE_MAP[np_arr.dtype],
            dims=list(np_arr.shape),
            vals=np_arr.flatten().tolist(),
        ),
    )


def build_structured_exactness_graph(
    n_nodes: int, n_edges: int, n_faces: int, elem_type: int
) -> onnx.ModelProto:
    """Composed structured graph: φ → d⁰·φ → d¹·(d⁰·φ) → in-graph zero check.

    Inputs:  phi (n_nodes,) elem_type,
             edges (n_edges, 2) int64,
             face_edge_idx (n_faces, 3) int64
    Outputs: residual (n_faces,) elem_type,
             max_abs () elem_type,
             exact () BOOL — the in-graph assertion d¹∘d⁰ ≡ 0.
    """
    nodes: list[onnx.NodeProto] = []

    phi_vi = oh.make_tensor_value_info("phi", elem_type, [n_nodes])
    edges_vi = oh.make_tensor_value_info("edges", TensorProto.INT64, [n_edges, 2])
    fei_vi = oh.make_tensor_value_info(
        "face_edge_idx", TensorProto.INT64, [n_faces, 3]
    )

    # --- d⁰ · φ (structured: two Gathers + Sub) ---
    nodes.append(_const("e_col0", np.array(0, dtype=np.int64)))
    nodes.append(_const("e_col1", np.array(1, dtype=np.int64)))
    nodes.append(oh.make_node("Gather", ["edges", "e_col0"], ["tail_idx"], axis=1))
    nodes.append(oh.make_node("Gather", ["edges", "e_col1"], ["head_idx"], axis=1))
    nodes.append(oh.make_node("Gather", ["phi", "head_idx"], ["phi_head"], axis=0))
    nodes.append(oh.make_node("Gather", ["phi", "tail_idx"], ["phi_tail"], axis=0))
    nodes.append(oh.make_node("Sub", ["phi_head", "phi_tail"], ["g"]))

    # --- d¹ · g (structured: three Gathers + Add + Sub) ---
    for k, name in [(0, "idx_ab"), (1, "idx_bc"), (2, "idx_ac")]:
        nodes.append(_const(f"f_col{k}", np.array(k, dtype=np.int64)))
        nodes.append(oh.make_node(
            "Gather", ["face_edge_idx", f"f_col{k}"], [name], axis=1
        ))
    nodes.append(oh.make_node("Gather", ["g", "idx_ab"], ["g_ab"], axis=0))
    nodes.append(oh.make_node("Gather", ["g", "idx_bc"], ["g_bc"], axis=0))
    nodes.append(oh.make_node("Gather", ["g", "idx_ac"], ["g_ac"], axis=0))
    nodes.append(oh.make_node("Add", ["g_ab", "g_bc"], ["g_sum"]))
    nodes.append(oh.make_node("Sub", ["g_sum", "g_ac"], ["residual"]))

    # --- In-graph zero assertion ---
    nodes.append(oh.make_node("Abs", ["residual"], ["abs_residual"]))
    nodes.append(oh.make_node(
        "ReduceMax", ["abs_residual"], ["max_abs"], keepdims=0
    ))
    zero = (
        np.array(0, dtype=np.int64)
        if elem_type == TensorProto.INT64
        else np.array(0.0, dtype=np.float64)
    )
    nodes.append(_const("zero_scalar", zero))
    nodes.append(oh.make_node("Equal", ["max_abs", "zero_scalar"], ["exact"]))

    outputs = [
        oh.make_tensor_value_info("residual", elem_type, [n_faces]),
        oh.make_tensor_value_info("max_abs", elem_type, []),
        oh.make_tensor_value_info("exact", TensorProto.BOOL, []),
    ]
    graph = oh.make_graph(
        nodes,
        name="derham_exactness_structured",
        inputs=[phi_vi, edges_vi, fei_vi],
        outputs=outputs,
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def build_coo_exactness_graph(
    n_nodes: int, n_edges: int, n_faces: int,
    nnz_d0: int, nnz_d1: int,
) -> onnx.ModelProto:
    """Composed COO-matvec graph (int64): two chained sparse matvecs.

    Inputs:  phi (n_nodes,) int64,
             d0_rows/d0_cols (nnz_d0,) int64, d0_vals (nnz_d0,) int64,
             d1_rows/d1_cols (nnz_d1,) int64, d1_vals (nnz_d1,) int64
    Outputs: max_abs () int64, exact () BOOL.

    This is the operator-agnostic form: the d-matrices arrive as
    pre-assembled host COO triplets, the same input-contract shape as
    the Phase G.7 edge tables.
    """
    nodes: list[onnx.NodeProto] = []
    et = TensorProto.INT64

    inputs = [
        oh.make_tensor_value_info("phi", et, [n_nodes]),
        oh.make_tensor_value_info("d0_rows", et, [nnz_d0]),
        oh.make_tensor_value_info("d0_cols", et, [nnz_d0]),
        oh.make_tensor_value_info("d0_vals", et, [nnz_d0]),
        oh.make_tensor_value_info("d1_rows", et, [nnz_d1]),
        oh.make_tensor_value_info("d1_cols", et, [nnz_d1]),
        oh.make_tensor_value_info("d1_vals", et, [nnz_d1]),
    ]

    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    zero_fill = oh.make_tensor("z", TensorProto.INT64, [1], [0])

    def coo_matvec(tag: str, x: str, n_rows: int, y: str) -> None:
        nodes.append(oh.make_node(
            "Gather", [x, f"{tag}_cols"], [f"{tag}_x_at_cols"], axis=0
        ))
        nodes.append(oh.make_node(
            "Mul", [f"{tag}_vals", f"{tag}_x_at_cols"], [f"{tag}_updates"]
        ))
        nodes.append(oh.make_node(
            "Unsqueeze", [f"{tag}_rows", "ax_neg1"], [f"{tag}_indices"]
        ))
        nodes.append(_const(
            f"{tag}_shape", np.array([n_rows], dtype=np.int64)
        ))
        nodes.append(oh.make_node(
            "ConstantOfShape",
            inputs=[f"{tag}_shape"],
            outputs=[f"{tag}_zero"],
            value=zero_fill,
        ))
        nodes.append(oh.make_node(
            "ScatterND",
            inputs=[f"{tag}_zero", f"{tag}_indices", f"{tag}_updates"],
            outputs=[y],
            reduction="add",
        ))

    coo_matvec("d0", "phi", n_edges, "g")      # g = d⁰ · φ
    coo_matvec("d1", "g", n_faces, "residual")  # r = d¹ · g

    nodes.append(oh.make_node("Abs", ["residual"], ["abs_residual"]))
    nodes.append(oh.make_node(
        "ReduceMax", ["abs_residual"], ["max_abs"], keepdims=0
    ))
    nodes.append(_const("zero_scalar", np.array(0, dtype=np.int64)))
    nodes.append(oh.make_node("Equal", ["max_abs", "zero_scalar"], ["exact"]))

    outputs = [
        oh.make_tensor_value_info("max_abs", et, []),
        oh.make_tensor_value_info("exact", TensorProto.BOOL, []),
    ]
    graph = oh.make_graph(
        nodes,
        name="derham_exactness_coo",
        inputs=inputs,
        outputs=outputs,
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def host_face_edge_idx(edges: np.ndarray, faces: np.ndarray) -> np.ndarray:
    """HOST-SIDE dict lookup — see probe_d1_apply.py for the rationale."""
    edge_to_idx = {
        (int(edges[i, 0]), int(edges[i, 1])): i for i in range(edges.shape[0])
    }
    n_faces = faces.shape[0]
    fei = np.empty((n_faces, 3), dtype=np.int64)
    for f in range(n_faces):
        a, b, c = int(faces[f, 0]), int(faces[f, 1]), int(faces[f, 2])
        fei[f, 0] = edge_to_idx[(a, b)]
        fei[f, 1] = edge_to_idx[(b, c)]
        fei[f, 2] = edge_to_idx[(a, c)]
    return fei


def _baseline_scalar(baseline: dict, key: str) -> int:
    return int(round(baseline["outputs"][key]["data"][0]))


def main() -> int:
    print("== Probe: in-graph exactness d¹·(d⁰·φ) ≡ 0 on the sphere fixture ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # ------------------------------------------------------------- #
    # Host-side: load fixture, build topology, cross-check vs #149
    # ------------------------------------------------------------- #
    nodes_xyz, tets = _read_msh_tets(MESH_PATH)
    n_nodes = int(nodes_xyz.shape[0])
    n_tets = int(tets.shape[0])
    edges = build_edges(tets)
    faces = build_faces(tets)
    n_edges = int(edges.shape[0])
    n_faces = int(faces.shape[0])
    face_edge_idx = host_face_edge_idx(edges, faces)
    d0 = gradient_map(n_nodes, edges)
    d1 = curl_map(edges, faces)

    print(f"fixture: {MESH_PATH.relative_to(REFERENCE_ROOT.parent)}")
    print(f"  n_nodes={n_nodes}, n_edges={n_edges}, n_faces={n_faces}, "
          f"n_tets={n_tets}")

    with BASELINE_PATH.open() as fh:
        baseline = json.load(fh)
    baseline_ok = (
        baseline["fixture_id"] == "derham/sphere_n774_d0_d1_d2"
        and _baseline_scalar(baseline, "n_nodes") == n_nodes
        and _baseline_scalar(baseline, "n_edges") == n_edges
        and _baseline_scalar(baseline, "n_faces") == n_faces
        and _baseline_scalar(baseline, "n_tets") == n_tets
    )
    print(f"  #149 baseline cross-check ({baseline['fixture_id']}): "
          f"{'MATCH' if baseline_ok else 'MISMATCH'}")
    print()

    rng = np.random.default_rng(2026)
    phi_i64 = rng.integers(-10**6, 10**6, size=n_nodes).astype(np.int64)
    phi_f64 = rng.standard_normal(n_nodes).astype(np.float64)

    overall_ok = baseline_ok

    # ------------------------------------------------------------- #
    # Graph 1 — structured composed chain, int64 (the in-graph assert)
    # ------------------------------------------------------------- #
    print("--- Graph 1: structured d¹∘d⁰ chain, int64, in-graph Equal-zero ---")
    model = build_structured_exactness_graph(
        n_nodes, n_edges, n_faces, TensorProto.INT64
    )
    onnx.checker.check_model(model)
    sess = ort.InferenceSession(model.SerializeToString())
    residual, max_abs, exact = sess.run(
        ["residual", "max_abs", "exact"],
        {"phi": phi_i64, "edges": edges, "face_edge_idx": face_edge_idx},
    )
    print(f"  in-graph max|d¹·(d⁰·φ)| = {int(max_abs)}")
    print(f"  in-graph exactness bool  = {bool(exact)}")
    # Host-side scipy cross-check of the same residual.
    r_scipy = np.asarray((d1 @ (d0 @ phi_i64)), dtype=np.int64)
    scipy_match = bool(np.array_equal(residual, r_scipy))
    print(f"  residual vs scipy d1@(d0@φ): {'bit-exact' if scipy_match else 'MISMATCH'}")
    if not (bool(exact) and int(max_abs) == 0 and scipy_match):
        overall_ok = False
    print()

    # ------------------------------------------------------------- #
    # Graph 2 — composed COO-matvec chain, int64 (operator-agnostic)
    # ------------------------------------------------------------- #
    print("--- Graph 2: composed COO matvec chain (ScatterND × 2), int64 ---")
    d0_coo, d1_coo = d0.tocoo(), d1.tocoo()
    model2 = build_coo_exactness_graph(
        n_nodes, n_edges, n_faces, int(d0.nnz), int(d1.nnz)
    )
    onnx.checker.check_model(model2)
    sess2 = ort.InferenceSession(model2.SerializeToString())
    max_abs2, exact2 = sess2.run(
        ["max_abs", "exact"],
        {
            "phi": phi_i64,
            "d0_rows": d0_coo.row.astype(np.int64),
            "d0_cols": d0_coo.col.astype(np.int64),
            "d0_vals": d0_coo.data.astype(np.int64),
            "d1_rows": d1_coo.row.astype(np.int64),
            "d1_cols": d1_coo.col.astype(np.int64),
            "d1_vals": d1_coo.data.astype(np.int64),
        },
    )
    print(f"  in-graph max|d¹·(d⁰·φ)| = {int(max_abs2)}")
    print(f"  in-graph exactness bool  = {bool(exact2)}")
    if not (bool(exact2) and int(max_abs2) == 0):
        overall_ok = False
    print()

    # ------------------------------------------------------------- #
    # Graph 3 — f64 control: why the in-graph assert is int64
    # ------------------------------------------------------------- #
    print("--- Graph 3 (control): structured chain in float64 ---")
    model3 = build_structured_exactness_graph(
        n_nodes, n_edges, n_faces, TensorProto.DOUBLE
    )
    onnx.checker.check_model(model3)
    sess3 = ort.InferenceSession(model3.SerializeToString())
    _, max_abs3, exact3 = sess3.run(
        ["residual", "max_abs", "exact"],
        {"phi": phi_f64, "edges": edges, "face_edge_idx": face_edge_idx},
    )
    print(f"  f64 max|d¹·(d⁰·φ)| = {float(max_abs3):.3e}   "
          f"(roundoff in g = fl(φ_b − φ_a); not an ONNX defect)")
    print(f"  f64 Equal-zero bool = {bool(exact3)}")
    print("  → the bit-exact in-graph assert requires the int64 channel;")
    print("    f64 exactness only holds to roundoff, as in any backend.")
    print()

    # ------------------------------------------------------------- #
    # Verdict
    # ------------------------------------------------------------- #
    if overall_ok:
        print("Verdict: d¹∘d⁰ ≡ 0 holds IN-GRAPH under onnxruntime, bit-exactly,")
        print("  on the full bundled sphere fixture (774 nodes / 4512 edges /")
        print("  7074 faces), in both the structured-incidence form and the")
        print("  operator-agnostic COO-matvec form. The exactness identity of")
        print("  the discrete de Rham complex survives the lowering to a pure")
        print("  opset-18 graph — only the topology tables cross the host")
        print("  boundary.")
    else:
        print("Verdict: FAILED — see above; audit table is stale.")
    return 0 if overall_ok else 1


if __name__ == "__main__":
    sys.exit(main())
