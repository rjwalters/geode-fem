"""Probe: ONNX expressibility of d⁰ *application* (discrete gradient matvec).

Epic #88, Phase I.3 (issue #169). This probe asks whether **applying**
the discrete gradient ``d⁰`` to a nodal field — ``g = d⁰ · φ`` — can be
expressed as a pure ONNX opset-18 graph, given that the **construction**
of ``d⁰`` (edge enumeration, dedup, orientation signs) is host-side per
the Phase G.6 ``build_edges`` finding
(``reference/onnx/audit/sphere_pec/probe_edge_enumeration.py``).

The construction/application split
==================================

``d⁰`` is an ``(n_edges, n_nodes)`` signed ``{-1, +1}`` incidence
matrix (``reference/numpy/derham.py::gradient_map``). Constructing it
requires ``np.unique(pairs, axis=0)`` — data-dependent output shape,
no ONNX lowering (G.6 verdict: secretly imperative, host-side).

But once the host hands the graph the **edge table** ``edges
(n_edges, 2) int64`` (or equivalently the COO triplets of ``d⁰``),
application is pure gather/scatter arithmetic with static shapes:

  - Row ``[a, b]`` of ``d⁰`` has exactly ``-1`` at column ``a`` and
    ``+1`` at column ``b``, so ``(d⁰·φ)[edge] = φ[b] - φ[a]``.

Strategy: two graph forms
=========================

(A) **Structured incidence form** — exploit the fixed 2-nnz-per-row
    structure: ``Gather(φ, edges[:, 1]) - Gather(φ, edges[:, 0])``.
    Two Gathers + one Sub. Runs in both int64 (bit-exact, the #149
    contract dtype) and float64.

(B) **Generic COO sparse matvec** — the form that generalizes to any
    pre-assembled host sparse matrix (d¹, d² included):
    ``y = ScatterND_add(zeros(n_rows), rows[:, None],
    data * Gather(x, cols))``. This is the load-bearing form for the
    L4 spec question: it consumes (rows, cols, data) as plain graph
    inputs, exactly the input-contract shape already established for
    ``tet_edge_idx`` / ``tet_edge_sign`` in Phase G.7.

Both forms are expected EMITTABLE: Gather, Sub, Mul, ConstantOfShape,
and ScatterND(reduction="add") are all native opset-18 over int64 and
float64 (the c128 frictions of Phase H.5 do not arise — the de Rham
chain is integer-valued).

Run
===

    python3 reference/onnx/audit/derham/probe_d0_apply.py
"""

from __future__ import annotations

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

from derham import build_edges, gradient_map  # noqa: E402

OPSET = 18

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


def build_structured_d0_graph(
    n_nodes: int, n_edges: int, elem_type: int
) -> onnx.ModelProto:
    """Graph (A) — structured incidence form of ``g = d⁰ · φ``.

    Inputs:  phi (n_nodes,) elem_type, edges (n_edges, 2) int64
    Output:  g (n_edges,) elem_type — per-edge ``φ[b] - φ[a]``.
    """
    nodes: list[onnx.NodeProto] = []

    phi_vi = oh.make_tensor_value_info("phi", elem_type, [n_nodes])
    edges_vi = oh.make_tensor_value_info("edges", TensorProto.INT64, [n_edges, 2])

    # Scalar column indices — Gather with a scalar index drops the axis,
    # yielding (n_edges,) directly (no Squeeze needed).
    nodes.append(_const("col0", np.array(0, dtype=np.int64)))
    nodes.append(_const("col1", np.array(1, dtype=np.int64)))
    nodes.append(oh.make_node("Gather", ["edges", "col0"], ["tail_idx"], axis=1))
    nodes.append(oh.make_node("Gather", ["edges", "col1"], ["head_idx"], axis=1))

    # φ at heads / tails, then the signed difference.
    nodes.append(oh.make_node("Gather", ["phi", "head_idx"], ["phi_head"], axis=0))
    nodes.append(oh.make_node("Gather", ["phi", "tail_idx"], ["phi_tail"], axis=0))
    nodes.append(oh.make_node("Sub", ["phi_head", "phi_tail"], ["g"]))

    g_vi = oh.make_tensor_value_info("g", elem_type, [n_edges])
    graph = oh.make_graph(
        nodes,
        name="d0_apply_structured",
        inputs=[phi_vi, edges_vi],
        outputs=[g_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def build_coo_matvec_graph(
    n_rows: int, n_cols: int, nnz: int, elem_type: int
) -> onnx.ModelProto:
    """Graph (B) — generic COO sparse matvec ``y = A · x``.

    Inputs:  x (n_cols,) elem_type,
             rows (nnz,) int64, cols (nnz,) int64, vals (nnz,) elem_type
    Output:  y (n_rows,) elem_type

    Operator-agnostic: works for any host-pre-assembled sparse matrix
    handed over as COO triplets (d⁰, d¹, d², Nédélec blocks, ...).
    """
    nodes: list[onnx.NodeProto] = []

    x_vi = oh.make_tensor_value_info("x", elem_type, [n_cols])
    rows_vi = oh.make_tensor_value_info("rows", TensorProto.INT64, [nnz])
    cols_vi = oh.make_tensor_value_info("cols", TensorProto.INT64, [nnz])
    vals_vi = oh.make_tensor_value_info("vals", elem_type, [nnz])

    # Per-nnz products: vals * x[cols]
    nodes.append(oh.make_node("Gather", ["x", "cols"], ["x_at_cols"], axis=0))
    nodes.append(oh.make_node("Mul", ["vals", "x_at_cols"], ["updates"]))

    # Indices for ScatterND: (nnz, 1)
    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows", "ax_neg1"], ["indices"]))

    # Zero output buffer (n_rows,) of the working dtype.
    zero_fill = (
        oh.make_tensor("z", TensorProto.INT64, [1], [0])
        if elem_type == TensorProto.INT64
        else oh.make_tensor("z", TensorProto.DOUBLE, [1], [0.0])
    )
    nodes.append(_const("shape_rows", np.array([n_rows], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_rows"],
        outputs=["zero_buf"],
        value=zero_fill,
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_buf", "indices", "updates"],
        outputs=["y"],
        reduction="add",
    ))

    y_vi = oh.make_tensor_value_info("y", elem_type, [n_rows])
    graph = oh.make_graph(
        nodes,
        name="coo_matvec",
        inputs=[x_vi, rows_vi, cols_vi, vals_vi],
        outputs=[y_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def _check_and_run(model, output_names, feeds):
    onnx.checker.check_model(model)
    sess = ort.InferenceSession(model.SerializeToString())
    return sess.run(output_names, feeds)


def main() -> int:
    print("== Probe: d⁰ application (discrete gradient matvec) — Phase I.3 ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Tiny test mesh: 2 tets sharing a face — same as the G.6/H.5 probes.
    tets_np = np.array(
        [[0, 1, 2, 3],
         [1, 2, 3, 4]],
        dtype=np.int64,
    )
    n_nodes = 5

    # HOST-SIDE construction (the G.6 "secretly imperative" boundary):
    # edge dedup + ordering happens here, in NumPy, never in the graph.
    edges = build_edges(tets_np)
    n_edges = int(edges.shape[0])
    d0 = gradient_map(n_nodes, edges)  # scipy CSR, int64 {-1, +1}
    d0_coo = d0.tocoo()
    print(f"Test mesh: n_tets={tets_np.shape[0]}, n_nodes={n_nodes}, "
          f"n_edges={n_edges}, d0 nnz={d0.nnz}")
    print()

    rng = np.random.default_rng(42)
    phi_i64 = rng.integers(-1000, 1000, size=n_nodes).astype(np.int64)
    phi_f64 = rng.standard_normal(n_nodes).astype(np.float64)

    g_ref_i64 = np.asarray(d0 @ phi_i64, dtype=np.int64)
    g_ref_f64 = phi_f64[edges[:, 1]] - phi_f64[edges[:, 0]]

    overall_ok = True

    # ------------------------------------------------------------- #
    # Graph (A) — structured incidence form, int64 and f64
    # ------------------------------------------------------------- #
    print("--- Graph (A): structured form  g = Gather(φ, head) - Gather(φ, tail) ---")
    for label, etype, phi, g_ref in [
        ("int64", TensorProto.INT64, phi_i64, g_ref_i64),
        ("float64", TensorProto.DOUBLE, phi_f64, g_ref_f64),
    ]:
        try:
            model = build_structured_d0_graph(n_nodes, n_edges, etype)
            (g_onnx,) = _check_and_run(model, ["g"], {"phi": phi, "edges": edges})
            err = int(np.max(np.abs(g_onnx - g_ref))) if label == "int64" \
                else float(np.max(np.abs(g_onnx - g_ref)))
            status = "OK"
        except Exception as e:  # noqa: BLE001
            status, err = f"FAIL ({e!r})", float("nan")
            overall_ok = False
        print(f"  [{label:7s}] checker+runtime: {status}   "
              f"max |g_onnx - g_ref| = {err:.3e}" if status == "OK"
              else f"  [{label:7s}] {status}")
        if status == "OK" and err != 0:
            overall_ok = False
    print()

    # ------------------------------------------------------------- #
    # Graph (B) — generic COO matvec, int64 and f64
    # ------------------------------------------------------------- #
    print("--- Graph (B): generic COO matvec  y = ScatterND_add(0, rows, vals·x[cols]) ---")
    rows = d0_coo.row.astype(np.int64)
    cols = d0_coo.col.astype(np.int64)
    for label, etype, x, vals, y_ref in [
        ("int64", TensorProto.INT64, phi_i64,
         d0_coo.data.astype(np.int64), g_ref_i64),
        ("float64", TensorProto.DOUBLE, phi_f64,
         d0_coo.data.astype(np.float64),
         np.asarray(d0.astype(np.float64) @ phi_f64)),
    ]:
        try:
            model = build_coo_matvec_graph(n_edges, n_nodes, int(d0.nnz), etype)
            (y_onnx,) = _check_and_run(
                model, ["y"], {"x": x, "rows": rows, "cols": cols, "vals": vals}
            )
            err = int(np.max(np.abs(y_onnx - y_ref))) if label == "int64" \
                else float(np.max(np.abs(y_onnx - y_ref)))
            status = "OK"
        except Exception as e:  # noqa: BLE001
            status, err = f"FAIL ({e!r})", float("nan")
            overall_ok = False
        print(f"  [{label:7s}] checker+runtime: {status}   "
              f"max |y_onnx - y_ref| = {err:.3e}" if status == "OK"
              else f"  [{label:7s}] {status}")
        if status == "OK" and err != 0:
            overall_ok = False
    print()

    # ------------------------------------------------------------- #
    # Operator inventory + verdict
    # ------------------------------------------------------------- #
    print("Operator inventory for d⁰ application:")
    print("--------------------------------------------------------------")
    print("  Gather (axis=0/1, int64+f64)      EMITTABLE  native opset 18")
    print("  Sub (int64, f64)                  EMITTABLE  native opset 18")
    print("  Mul (int64, f64)                  EMITTABLE  native opset 18")
    print("  Unsqueeze / Constant              EMITTABLE  native opset 18")
    print("  ConstantOfShape (int64/f64 fill)  EMITTABLE  native opset 18")
    print("  ScatterND reduction='add' (int64) EMITTABLE  native opset 18")
    print()
    print("Construction boundary (NOT probed here — inherited from G.6):")
    print("  build_edges (dedup + inverse map) BLOCKED    host-side; see")
    print("    reference/onnx/audit/sphere_pec/probe_edge_enumeration.py")
    print()
    if overall_ok:
        print("Verdict: d⁰ APPLICATION is GRAPH-PURE (emittable).")
        print("  Both the structured two-Gather form and the generic COO matvec")
        print("  lower to opset 18 and reproduce the NumPy/scipy reference")
        print("  bit-exactly in int64 (the #149 contract dtype) and f64.")
        print("  Construction stays host-side; the edge table (or COO triplets)")
        print("  crosses the L4-input boundary as plain int64 tensors.")
    else:
        print("Verdict: FAILED — see errors above; audit table is stale.")
    return 0 if overall_ok else 1


if __name__ == "__main__":
    sys.exit(main())
