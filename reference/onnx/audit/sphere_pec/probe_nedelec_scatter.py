"""Probe: ONNX expressibility of the Nedelec irregular edge scatter-add.

Epic #88, Phase G.6 (issue #135). This probe asks whether the global
Nedelec K/M assembly step (scatter per-element 6x6 blocks into a global
(n_edges, n_edges) buffer) can be expressed as a pure ONNX graph, given
that `edges`, `tet_edge_idx`, and `tet_edge_sign` are HOST-COMPUTED
inputs (per the `build_edges` verdict: NOT EXPRESSIBLE, so they enter
as pre-computed graph inputs exactly like `tets` and `nodes`).

What assemble_global_nedelec does
==================================

Given:
  - k_local (n_tets, 6, 6) and m_local (n_tets, 6, 6) — per-element matrices
  - tet_edge_idx (n_tets, 6) int64 — global edge index per local edge
  - tet_edge_sign (n_tets, 6) float64 — +/-1 sign per local edge
  - epsilon_r (n_tets,) float64 — per-tet permittivity
  - n_edges: int — number of global edges

It:
  1. Applies sign corrections: k_signed[e,i,j] = k_local[e,i,j] * sign[e,i] * sign[e,j]
  2. Applies epsilon scaling: m_signed[e,i,j] = m_local[e,i,j] * sign * eps[e]
  3. Builds COO triplets: rows[e,i,j] = tet_edge_idx[e,i],
                           cols[e,i,j] = tet_edge_idx[e,j]
  4. Scatter-adds vals[e,i,j] into K_global[rows[e,i,j], cols[e,i,j]]

Comparison with P1 scatter (cube-cavity)
==========================================

The P1 scatter (probe_assembly_scatter.py) used ScatterND with
reduction="add" and was EXPRESSIBLE cleanly.

The Nedelec scatter has additional complications:

  (A) The sign correction involves an OUTER PRODUCT of (n_tets, 6) signs:
      sign_outer[e, i, j] = sign[e, i] * sign[e, j]
      This outer product IS expressible via Mul with Unsqueeze broadcasting.

  (B) The COO index construction involves broadcasting tet_edge_idx:
      rows[e, i, j] = tet_edge_idx[e, i]  (broadcast j-dim)
      cols[e, i, j] = tet_edge_idx[e, j]  (broadcast i-dim)
      This IS expressible via Expand/Unsqueeze + Reshape.

  (C) The final ScatterND reduction="add" step is the SAME as P1 —
      EXPRESSIBLE via ScatterND(reduction="add").

  (D) HOWEVER: n_edges is DATA-DEPENDENT (output of build_edges). The
      zero-buffer initialization `np.zeros((n_edges, n_edges))` requires
      knowing n_edges at graph construction time OR accepting it as a
      shape-carrying input.

      In the P1 case, n_nodes is fixed by the mesh (a static integer).
      In the Nedelec case, n_edges is the output of a deduplicated edge
      enumeration — it is a GRAPH INPUT, not a constant. This forces the
      ConstantOfShape node to take a runtime-shaped input rather than a
      baked constant, which is legal in ONNX (ConstantOfShape accepts a
      dynamic shape tensor) but means the output buffer shape is dynamic.

      ScatterND still works on a dynamic-shape buffer, but the output
      type becomes (n_edges, n_edges) where n_edges is unknown at compile
      time. This is the same "data-dependent shape" friction class as
      NonZero in the Dirichlet probe — it propagates through to K_global.

      Classification: PARTIAL — the scatter itself is expressible, but
      the global buffer size is data-dependent (inherited from build_edges),
      which breaks static shape inference on K_global.

This probe demonstrates:
  (1) The sign outer product IS expressible.
  (2) The COO index construction IS expressible.
  (3) ScatterND(reduction="add") on the signed 6x6 blocks IS expressible
      when n_edges is fixed at graph-build time.
  (4) The n_edges dependency on the dynamic Unique output breaks the
      static-shape contract for the global buffer.

Run
===

    python3 reference/onnx/audit/sphere_pec/probe_nedelec_scatter.py
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

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[4])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

from reference.numpy.nedelec_local_matrices import batched_nedelec_local_matrices  # noqa: E402
from reference.numpy.sphere_pec import build_edges  # noqa: E402

OPSET = 18

N_LOCAL = 6  # 6 edges per tet


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    dtype_map = {
        np.dtype("float64"): TensorProto.DOUBLE,
        np.dtype("int64"): TensorProto.INT64,
        np.dtype("int32"): TensorProto.INT32,
    }
    return oh.make_node(
        "Constant",
        inputs=[],
        outputs=[name],
        value=oh.make_tensor(
            name=name + "_value",
            data_type=dtype_map[np_arr.dtype],
            dims=list(np_arr.shape),
            vals=np_arr.flatten().tolist(),
        ),
    )


def build_nedelec_scatter_graph(n_tets: int, n_edges: int) -> onnx.ModelProto:
    """Build the Nedelec scatter-add graph with n_tets and n_edges fixed.

    Inputs:
      k_local (n_tets, 6, 6) float64
      m_local (n_tets, 6, 6) float64
      tet_edge_idx (n_tets, 6) int64
      tet_edge_sign (n_tets, 6) float64
      epsilon_r (n_tets,) float64

    Outputs:
      k_global (n_edges, n_edges) float64
      m_global (n_edges, n_edges) float64
    """
    nodes: list[onnx.NodeProto] = []

    k_local_vi = oh.make_tensor_value_info("k_local", TensorProto.DOUBLE, [n_tets, 6, 6])
    m_local_vi = oh.make_tensor_value_info("m_local", TensorProto.DOUBLE, [n_tets, 6, 6])
    tei_vi = oh.make_tensor_value_info("tet_edge_idx", TensorProto.INT64, [n_tets, 6])
    tes_vi = oh.make_tensor_value_info("tet_edge_sign", TensorProto.DOUBLE, [n_tets, 6])
    eps_vi = oh.make_tensor_value_info("epsilon_r", TensorProto.DOUBLE, [n_tets])

    # --- (1) Sign outer product: sign_outer[e,i,j] = sign[e,i] * sign[e,j] ---
    # tet_edge_sign: (n_tets, 6) -> unsqueeze to (n_tets, 6, 1) and (n_tets, 1, 6)
    nodes.append(_const("ax2", np.array([2], dtype=np.int64)))
    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax2"], ["sign_col"]))   # (N,6,1)
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax1"], ["sign_row"]))   # (N,1,6)
    nodes.append(oh.make_node("Mul", ["sign_col", "sign_row"], ["sign_outer"]))       # (N,6,6)

    # --- (2) Apply sign to k_local ---
    nodes.append(oh.make_node("Mul", ["k_local", "sign_outer"], ["k_signed"]))

    # --- (3) Apply sign + epsilon to m_local ---
    # epsilon_r: (n_tets,) -> reshape to (n_tets, 1, 1) for broadcasting
    nodes.append(_const("shape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["epsilon_r", "shape_n11"], ["eps_b"]))
    nodes.append(oh.make_node("Mul", ["m_local", "sign_outer"], ["m_signed_pre"]))
    nodes.append(oh.make_node("Mul", ["m_signed_pre", "eps_b"], ["m_signed"]))

    # --- (4) COO index construction ---
    # rows[e,i,j] = tet_edge_idx[e,i] (broadcast j-dim)
    # cols[e,i,j] = tet_edge_idx[e,j] (broadcast i-dim)
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax2"], ["tei_col"]))  # (N,6,1)
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax1"], ["tei_row"]))  # (N,1,6)

    # Broadcast to (n_tets, 6, 6)
    nodes.append(_const("target_shape", np.array([n_tets, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Expand", ["tei_col", "target_shape"], ["rows_3d"]))
    nodes.append(oh.make_node("Expand", ["tei_row", "target_shape"], ["cols_3d"]))

    # Flatten to (n_tets * 36,)
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["rows_3d", "shape_flat"], ["rows_flat"]))
    nodes.append(oh.make_node("Reshape", ["cols_3d", "shape_flat"], ["cols_flat"]))
    nodes.append(oh.make_node("Reshape", ["k_signed", "shape_flat"], ["k_vals"]))
    nodes.append(oh.make_node("Reshape", ["m_signed", "shape_flat"], ["m_vals"]))

    # Stack (rows, cols) into (n_tets*36, 2) index table
    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows_flat", "ax_neg1"], ["rows_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["cols_flat", "ax_neg1"], ["cols_col"]))
    nodes.append(oh.make_node("Concat", ["rows_col", "cols_col"], ["indices"], axis=1))

    # --- (5) Zero buffer of shape (n_edges, n_edges) ---
    # n_edges is FIXED AT GRAPH-BUILD TIME here (static-shape version).
    # The friction note below explains why this becomes data-dependent
    # in the real pipeline (after build_edges).
    nodes.append(_const("shape_nn", np.array([n_edges, n_edges], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_nn"],
        outputs=["zero_buf_k"],
        value=oh.make_tensor("zero_f64", TensorProto.DOUBLE, [1], [0.0]),
    ))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_nn"],
        outputs=["zero_buf_m"],
        value=oh.make_tensor("zero_f64_m", TensorProto.DOUBLE, [1], [0.0]),
    ))

    # --- (6) ScatterND with reduction="add" ---
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_buf_k", "indices", "k_vals"],
        outputs=["k_global"],
        reduction="add",
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_buf_m", "indices", "m_vals"],
        outputs=["m_global"],
        reduction="add",
    ))

    k_global_vi = oh.make_tensor_value_info("k_global", TensorProto.DOUBLE, [n_edges, n_edges])
    m_global_vi = oh.make_tensor_value_info("m_global", TensorProto.DOUBLE, [n_edges, n_edges])

    graph = oh.make_graph(
        nodes,
        name="nedelec_scatter_probe",
        inputs=[k_local_vi, m_local_vi, tei_vi, tes_vi, eps_vi],
        outputs=[k_global_vi, m_global_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: Nedelec edge scatter-add (sphere PEC, Phase G.6) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Synthetic mesh: 2 tets sharing a face
    import scipy.sparse

    tets_np = np.array(
        [[0, 1, 2, 3],
         [1, 2, 3, 4]],
        dtype=np.int64,
    )
    nodes_np = np.array(
        [[0.0, 0.0, 0.0],
         [1.0, 0.0, 0.0],
         [0.0, 1.0, 0.0],
         [0.0, 0.0, 1.0],
         [0.5, 0.5, 0.5]],
        dtype=np.float64,
    )
    n_tets = tets_np.shape[0]
    n_nodes = nodes_np.shape[0]

    # Host-computed inputs (from build_edges — NOT in the ONNX graph)
    edges, tet_edge_idx, tet_edge_sign = build_edges(tets_np)
    n_edges = int(edges.shape[0])

    # Per-element local matrices
    coords = nodes_np[tets_np, :]
    k_local, m_local, _ = batched_nedelec_local_matrices(coords)
    tet_edge_sign_f64 = tet_edge_sign.astype(np.float64)
    epsilon_r = np.ones(n_tets, dtype=np.float64)

    print(f"Test mesh: n_tets={n_tets}, n_nodes={n_nodes}, n_edges={n_edges}")
    print()

    # Build and check the ONNX scatter graph
    model = build_nedelec_scatter_graph(n_tets, n_edges)

    try:
        onnx.checker.check_model(model)
        checker_status = "OK"
    except Exception as e:  # noqa: BLE001
        checker_status = f"FAIL ({e!r})"

    rt_status = "skipped"
    max_err_k = max_err_m = float("nan")
    try:
        sess = ort.InferenceSession(model.SerializeToString())
        outs = sess.run(["k_global", "m_global"], {
            "k_local": k_local,
            "m_local": m_local,
            "tet_edge_idx": tet_edge_idx,
            "tet_edge_sign": tet_edge_sign_f64,
            "epsilon_r": epsilon_r,
        })
        k_global_onnx, m_global_onnx = outs

        # NumPy reference scatter
        sign_outer = tet_edge_sign_f64[:, :, None] * tet_edge_sign_f64[:, None, :]
        k_signed = k_local * sign_outer
        m_signed = m_local * sign_outer * epsilon_r[:, None, None]
        rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).ravel()
        cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).ravel()
        k_ref_sparse = scipy.sparse.coo_matrix(
            (k_signed.ravel(), (rows, cols)), shape=(n_edges, n_edges)
        ).toarray()
        m_ref_sparse = scipy.sparse.coo_matrix(
            (m_signed.ravel(), (rows, cols)), shape=(n_edges, n_edges)
        ).toarray()

        max_err_k = float(np.max(np.abs(k_global_onnx - k_ref_sparse)))
        max_err_m = float(np.max(np.abs(m_global_onnx - m_ref_sparse)))
        rt_status = "OK"
    except Exception as e:  # noqa: BLE001
        rt_status = f"FAIL ({e!r})"

    print("Operator inventory for Nedelec edge scatter-add:")
    print("--------------------------------------------------------------")
    print("  Unsqueeze / Expand / Mul   EXPRESSIBLE  lowers cleanly (sign outer product)")
    print("  Reshape                    EXPRESSIBLE  lowers cleanly (flatten 6x6 blocks)")
    print("  Concat                     EXPRESSIBLE  lowers cleanly (build (M,2) indices)")
    print("  ConstantOfShape            EXPRESSIBLE  lowers cleanly (zero buffer)")
    print("  ScatterND(reduction='add') EXPRESSIBLE  lowers cleanly (opset 16+ native)")
    print()
    print(f"onnx.checker.check_model: {checker_status}")
    print(f"onnxruntime execution: {rt_status}")
    if rt_status == "OK":
        print(f"  max |K_onnx - K_numpy| = {max_err_k:.3e}")
        print(f"  max |M_onnx - M_numpy| = {max_err_m:.3e}")
    print()

    print("Friction analysis — why the real pipeline is PARTIAL:")
    print("--------------------------------------------------------------")
    print("  1. STATIC-SHAPE VERSION (tested above): when n_edges is known at")
    print("     graph-build time, the scatter IS fully expressible. All operators")
    print("     lower cleanly via ScatterND(reduction='add'). This is the clean path.")
    print()
    print("  2. DYNAMIC-SHAPE VERSION (real pipeline): n_edges is the output of")
    print("     build_edges (NOT EXPRESSIBLE). So n_edges is a runtime value.")
    print("     The ConstantOfShape node can accept a dynamic shape tensor,")
    print("     making the zero buffer (n_edges, n_edges) dynamic. This means:")
    print("       - k_global and m_global are typed (n_edges, n_edges) where")
    print("         n_edges is unknown at compile time.")
    print("       - Static shape inference is broken: any downstream consumer")
    print("         sees (None, None) for K_global.")
    print("       - The Dirichlet restriction (Gather on interior-edge indices)")
    print("         inherits this data-dependent shape from K_global.")
    print()
    print("  3. SIGN OUTER PRODUCT — a new friction vs. P1:")
    print("     The P1 scatter had no sign correction. Nedelec requires an outer")
    print("     product of (n_tets, 6) sign vectors. This IS expressible via")
    print("     Unsqueeze + Mul broadcasting, but it is structurally more complex")
    print("     than the P1 scatter. Classification: graph-only friction.")
    print()
    print("  4. BUFFER SIZE (n_edges vs. n_nodes):")
    print("     P1: global buffer is (n_nodes, n_nodes) — known from the input")
    print("         shape of `tets`/`nodes` (static).")
    print("     Nedelec: global buffer is (n_edges, n_edges) — unknown until")
    print("              build_edges completes (data-dependent).")
    print("     This is the structural difference that breaks static shape.")
    print()
    print("Verdict: PARTIAL")
    print("  The scatter-add mechanics (ScatterND + sign outer product) lower")
    print("  cleanly when n_edges is a static graph constant. In the real pipeline,")
    print("  n_edges is data-dependent (from build_edges), which propagates a")
    print("  dynamic-shape axis through k_global and m_global. The graph is")
    print("  EXPRESSIBLE in ONNX but NOT statically-shaped end-to-end.")
    print("  Friction class: graph-only (data-dependent shape from build_edges;")
    print("  the scatter mechanism itself is not secretly-imperative).")

    return 0 if checker_status == "OK" and rt_status == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
