"""End-to-end ONNX assembly graph for the cube-cavity scalar Helmholtz spine.

Epic #88, Phase F.2 (issue #123). This is the static-graph payload that
the F.1 audit (`reference/onnx/audit/cube_cavity_operator_audit.md`)
green-lit. It is the ONNX analog of:

  - JAX:    `reference/jax/cube_cavity.py` ``_assemble_dense_jax``
  - TF-Java: `reference/tf_java/cube_cavity/.../AssemblyGraph.java``

The graph closes the assembly spine at the Dirichlet boundary — i.e. it
emits ``K_int`` and ``M_int`` (interior-DOF restricted) and stops. The
eigensolve is the host-side sidecar boundary, identical convention to
JAX (`scipy.linalg.eigh` outside the jit) and TF-Java
(`eigensolve_from_tfjava.py` over a JSON sidecar).

Authoring tool
==============

This module uses raw ``onnx.helper`` (NOT ``onnxscript``). Rationale,
from the F.1 audit (audit doc lines 51–55): ``onnxscript`` is more
ergonomic but can hide imperative sugar that the audit needs to surface
at the IR level. The audit's contract — "every L4 op visible as an
opset-18 node" — only holds if F.2 inherits the same authoring choice.
The curator pass on issue #123 made this an explicit Phase F.2 decision.

Graph shape (per the F.1 audit recommendation)
==============================================

Inputs:
  - ``nodes``    f64 shape ``(n_nodes, 3)``  — runtime node coordinates
  - ``tets``     i64 shape ``(n_elem, 4)``   — tet connectivity (host-baked)
  - ``idx_int``  i64 shape ``(n_int,)``      — interior-DOF indices (host)

Outputs:
  - ``K_int``    f64 shape ``(n_int, n_int)``
  - ``M_int``    f64 shape ``(n_int, n_int)``

Stages (matching the audit's per-stage operator inventory):
  1. P1 local matrices — `MatMul`-based batched contraction of edge
     gradients; cross product synthesized as 6 Mul + 3 Sub + 3
     Unsqueeze + Concat (audit Stage 1).
  2. Global K/M scatter-add — `ScatterND(reduction="add")` onto a
     `ConstantOfShape` zero buffer (audit Stage 2).
  3. Dirichlet restriction — two successive `Gather`s with the
     host-computed ``idx_int`` (audit Stage 3, recommended convention).

Notes:
  - ``tets`` enters the graph as int64. The audit covers int32→int64
    casting at the boundary as graph-only friction (audit doc lines
    189–193); we pay that cost host-side instead of inside the graph.
  - ``MatMul`` broadcasts the batch dim natively (audit doc line 89),
    so the per-element ``(N, 4, 3) @ (N, 3, 4)`` contraction is one
    op — no einsum fallback (cf. TF-Java AssemblyGraph.java lines
    109–120, which had to drop to ``einsum``).
"""

from __future__ import annotations

from typing import List

import numpy as np
import onnx
import onnx.helper as oh
from onnx import TensorProto

OPSET = 18
IR_VERSION = 9


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    """Wrap a small NumPy array as a `Constant` node in the graph.

    Local copy of the same helper used by the F.1 audit probes. Per the
    F.2 curator decision, we keep this local rather than backporting
    into the probes — extraction to a shared ``reference/onnx/_helpers.py``
    is a deferred hygiene follow-up.
    """
    dtype_map = {
        np.dtype("float64"): TensorProto.DOUBLE,
        np.dtype("float32"): TensorProto.FLOAT,
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


def _p1_local_nodes(nodes: List[onnx.NodeProto]) -> None:
    """Append nodes that compute (k_local, m_local) given `elem_coords`.

    Reads:
      ``elem_coords``  f64 (N, 4, 3) — per-element vertex coords
    Writes:
      ``k_local``      f64 (N, 4, 4)
      ``m_local``      f64 (N, 4, 4)
      ``abs_det``      f64 (N,)       — exposed for downstream debug

    Mirrors `_p1_local_one` in `reference/jax/cube_cavity.py`. The
    structure is line-for-line identical to the audit probe
    `probe_p1_local.py`; only difference is that the constants used here
    have unique names (so the graph stays SSA when used alongside the
    scatter step).
    """
    # ---------- Constants ----------
    for c in (0, 1, 2, 3):
        nodes.append(_const(f"p1_v_idx_{c}", np.array(c, dtype=np.int64)))
    for c in range(3):
        nodes.append(_const(f"p1_comp_idx_{c}", np.array(c, dtype=np.int64)))
    nodes.append(_const("p1_axis_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(_const("p1_axis_1", np.array([1], dtype=np.int64)))
    nodes.append(_const("p1_reshape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(_const("p1_reshape_144", np.array([1, 4, 4], dtype=np.int64)))
    nodes.append(_const("p1_six", np.array(6.0, dtype=np.float64)))
    nodes.append(_const("p1_one_twenty", np.array(120.0, dtype=np.float64)))
    mass_pattern = np.array(
        [
            [2.0, 1.0, 1.0, 1.0],
            [1.0, 2.0, 1.0, 1.0],
            [1.0, 1.0, 2.0, 1.0],
            [1.0, 1.0, 1.0, 2.0],
        ],
        dtype=np.float64,
    )
    nodes.append(_const("p1_mass_pattern", mass_pattern))

    # ---------- Gather v0..v3 along axis=1 ----------
    for i in range(4):
        nodes.append(oh.make_node(
            "Gather",
            inputs=["elem_coords", f"p1_v_idx_{i}"],
            outputs=[f"p1_v{i}"],
            axis=1,
        ))

    # ---------- Edge vectors e1, e2, e3 ----------
    for i in (1, 2, 3):
        nodes.append(oh.make_node("Sub", [f"p1_v{i}", "p1_v0"], [f"p1_e{i}"]))

    # ---------- Cross-product synthesis helpers ----------
    emitted_components: set = set()

    def emit_components(vec: str) -> tuple:
        outs = []
        for c in range(3):
            out_name = f"{vec}_c{c}"
            if out_name not in emitted_components:
                nodes.append(oh.make_node(
                    "Gather",
                    inputs=[vec, f"p1_comp_idx_{c}"],
                    outputs=[out_name],
                    axis=1,
                ))
                emitted_components.add(out_name)
            outs.append(out_name)
        return tuple(outs)

    def emit_cross(a: str, b: str, out: str) -> None:
        ax, ay, az = emit_components(a)
        bx, by, bz = emit_components(b)
        # cx = ay*bz - az*by
        nodes.append(oh.make_node("Mul", [ay, bz], [f"{out}_t1"]))
        nodes.append(oh.make_node("Mul", [az, by], [f"{out}_t2"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t1", f"{out}_t2"], [f"{out}_x"]))
        # cy = az*bx - ax*bz
        nodes.append(oh.make_node("Mul", [az, bx], [f"{out}_t3"]))
        nodes.append(oh.make_node("Mul", [ax, bz], [f"{out}_t4"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t3", f"{out}_t4"], [f"{out}_y"]))
        # cz = ax*by - ay*bx
        nodes.append(oh.make_node("Mul", [ax, by], [f"{out}_t5"]))
        nodes.append(oh.make_node("Mul", [ay, bx], [f"{out}_t6"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t5", f"{out}_t6"], [f"{out}_z"]))
        # Stack components → (N, 3): Unsqueeze each to (N, 1), then Concat.
        for comp in ("x", "y", "z"):
            nodes.append(oh.make_node(
                "Unsqueeze",
                inputs=[f"{out}_{comp}", "p1_axis_neg1"],
                outputs=[f"{out}_{comp}_u"],
            ))
        nodes.append(oh.make_node(
            "Concat",
            inputs=[f"{out}_x_u", f"{out}_y_u", f"{out}_z_u"],
            outputs=[out],
            axis=1,
        ))

    emit_cross("p1_e2", "p1_e3", "p1_g1")
    emit_cross("p1_e3", "p1_e1", "p1_g2")
    emit_cross("p1_e1", "p1_e2", "p1_g3")

    # ---------- g0 = -(g1 + g2 + g3) ----------
    nodes.append(oh.make_node("Add", ["p1_g1", "p1_g2"], ["p1_g1_plus_g2"]))
    nodes.append(oh.make_node("Add", ["p1_g1_plus_g2", "p1_g3"], ["p1_g_sum"]))
    nodes.append(oh.make_node("Neg", ["p1_g_sum"], ["p1_g0"]))

    # ---------- det = sum(e1 * g1, axis=1) ; abs_det = |det| ----------
    nodes.append(oh.make_node("Mul", ["p1_e1", "p1_g1"], ["p1_e1_g1"]))
    nodes.append(oh.make_node(
        "ReduceSum",
        inputs=["p1_e1_g1", "p1_axis_1"],
        outputs=["p1_det"],
        keepdims=0,
    ))
    nodes.append(oh.make_node("Abs", ["p1_det"], ["abs_det"]))

    # ---------- gMat = stack(g0, g1, g2, g3) along axis=1 → (N, 4, 3) ----------
    for g in ("p1_g0", "p1_g1", "p1_g2", "p1_g3"):
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[g, "p1_axis_1"],
            outputs=[f"{g}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=["p1_g0_u", "p1_g1_u", "p1_g2_u", "p1_g3_u"],
        outputs=["p1_g_mat"],
        axis=1,
    ))

    # ---------- gg = g_mat @ g_mat^T per-batch (MatMul broadcasts batch dim) ----------
    nodes.append(oh.make_node(
        "Transpose",
        inputs=["p1_g_mat"],
        outputs=["p1_g_mat_T"],
        perm=[0, 2, 1],
    ))
    nodes.append(oh.make_node("MatMul", ["p1_g_mat", "p1_g_mat_T"], ["p1_gg"]))

    # ---------- k_local = gg / (6 * abs_det) ----------
    nodes.append(oh.make_node("Mul", ["p1_six", "abs_det"], ["p1_six_abs_det"]))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["p1_six_abs_det", "p1_reshape_n11"],
        outputs=["p1_six_abs_det_b"],
    ))
    nodes.append(oh.make_node("Div", ["p1_gg", "p1_six_abs_det_b"], ["k_local"]))

    # ---------- m_local = mass_pattern[None, :, :] * (abs_det/120)[:, None, None] ----------
    nodes.append(oh.make_node("Div", ["abs_det", "p1_one_twenty"], ["p1_m_scale"]))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["p1_m_scale", "p1_reshape_n11"],
        outputs=["p1_m_scale_b"],
    ))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["p1_mass_pattern", "p1_reshape_144"],
        outputs=["p1_mass_pattern_b"],
    ))
    nodes.append(oh.make_node("Mul", ["p1_mass_pattern_b", "p1_m_scale_b"], ["m_local"]))


def _scatter_assemble_nodes(nodes: List[onnx.NodeProto], n_nodes: int,
                            n_elem: int) -> None:
    """Append nodes that scatter `k_local`/`m_local` into dense globals.

    Reads:
      ``k_local`` f64 (N, 4, 4), ``m_local`` f64 (N, 4, 4),
      ``tets``    i64 (n_elem, 4)  — connectivity (graph input)
    Writes:
      ``k_global`` f64 (n_nodes, n_nodes)
      ``m_global`` f64 (n_nodes, n_nodes)

    Mirrors `_assemble_dense_jax` (JAX) / `tf.scatterNd` step (TF-Java)
    and the audit probe `probe_assembly_scatter.py`.
    """
    n_pairs = n_elem * 16  # flat (e, i, j) index count

    # ---------- Constants ----------
    nodes.append(_const("scatter_axis_2", np.array([2], dtype=np.int64)))
    nodes.append(_const("scatter_axis_1", np.array([1], dtype=np.int64)))
    nodes.append(_const("scatter_shape_n44", np.array([n_elem, 4, 4], dtype=np.int64)))
    nodes.append(_const("scatter_flat_shape",
                        np.array([n_pairs], dtype=np.int64)))
    nodes.append(_const("scatter_pair_flat_shape",
                        np.array([n_pairs, 1], dtype=np.int64)))
    nodes.append(_const("scatter_buf_shape",
                        np.array([n_nodes, n_nodes], dtype=np.int64)))

    # ---------- Build per-element (row, col) index pairs ----------
    # rows[e, i, j] = tets[e, i],  cols[e, i, j] = tets[e, j].
    # ONNX: Unsqueeze(tets, axis=2) → (n_elem, 4, 1)
    #       Unsqueeze(tets, axis=1) → (n_elem, 1, 4)
    # Then Expand to (n_elem, 4, 4). NB: ONNX has no `broadcast_to`, but
    # `Expand` is the equivalent (opset 8+).
    nodes.append(oh.make_node(
        "Unsqueeze",
        inputs=["tets", "scatter_axis_2"],
        outputs=["scatter_tets_rows"],
    ))
    nodes.append(oh.make_node(
        "Unsqueeze",
        inputs=["tets", "scatter_axis_1"],
        outputs=["scatter_tets_cols"],
    ))
    nodes.append(oh.make_node(
        "Expand",
        inputs=["scatter_tets_rows", "scatter_shape_n44"],
        outputs=["scatter_rows_3d"],
    ))
    nodes.append(oh.make_node(
        "Expand",
        inputs=["scatter_tets_cols", "scatter_shape_n44"],
        outputs=["scatter_cols_3d"],
    ))

    # Flatten to (n_pairs, 1) each, then Concat along axis=1 → (n_pairs, 2).
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["scatter_rows_3d", "scatter_pair_flat_shape"],
        outputs=["scatter_rows_flat"],
    ))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["scatter_cols_3d", "scatter_pair_flat_shape"],
        outputs=["scatter_cols_flat"],
    ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=["scatter_rows_flat", "scatter_cols_flat"],
        outputs=["scatter_indices"],
        axis=1,
    ))

    # ---------- Flatten k_local / m_local to (n_pairs,) ----------
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["k_local", "scatter_flat_shape"],
        outputs=["scatter_k_vals"],
    ))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["m_local", "scatter_flat_shape"],
        outputs=["scatter_m_vals"],
    ))

    # ---------- Zero buffer ----------
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["scatter_buf_shape"],
        outputs=["scatter_k_zero"],
        value=oh.make_tensor(
            name="scatter_k_zero_value",
            data_type=TensorProto.DOUBLE,
            dims=[1],
            vals=[0.0],
        ),
    ))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["scatter_buf_shape"],
        outputs=["scatter_m_zero"],
        value=oh.make_tensor(
            name="scatter_m_zero_value",
            data_type=TensorProto.DOUBLE,
            dims=[1],
            vals=[0.0],
        ),
    ))

    # ---------- ScatterND(reduction="add") ----------
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["scatter_k_zero", "scatter_indices", "scatter_k_vals"],
        outputs=["k_global"],
        reduction="add",
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["scatter_m_zero", "scatter_indices", "scatter_m_vals"],
        outputs=["m_global"],
        reduction="add",
    ))


def _dirichlet_restrict_nodes(nodes: List[onnx.NodeProto]) -> None:
    """Append nodes that compute `K_int = K[idx, :][:, idx]`.

    Reads:
      ``k_global`` f64 (n_nodes, n_nodes), ``m_global`` f64 (...)
      ``idx_int``  i64 (n_int,)  — graph input (host-computed per F.1
                                   audit recommendation)
    Writes:
      ``K_int`` f64 (n_int, n_int)
      ``M_int`` f64 (n_int, n_int)

    Two-Gather decomposition (audit Stage 3 / Path A).
    """
    nodes.append(oh.make_node(
        "Gather",
        inputs=["k_global", "idx_int"],
        outputs=["dirichlet_k_rows"],
        axis=0,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["dirichlet_k_rows", "idx_int"],
        outputs=["K_int"],
        axis=1,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["m_global", "idx_int"],
        outputs=["dirichlet_m_rows"],
        axis=0,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["dirichlet_m_rows", "idx_int"],
        outputs=["M_int"],
        axis=1,
    ))


def build_cube_cavity_graph(n_nodes: int, n_elem: int) -> onnx.ModelProto:
    """Build the end-to-end cube-cavity assembly ONNX graph.

    Parameters
    ----------
    n_nodes : int
        Total node count of the mesh.
    n_elem : int
        Total tet count of the mesh.

    Notes
    -----
    `n_nodes` and `n_elem` are baked into a small set of `Constant`
    shapes (the (n_nodes, n_nodes) zero buffer; the (n_elem, 4, 4)
    Expand shapes; etc.). The interior-DOF count `n_int` is *not* baked
    in — the Dirichlet Gather honors any length of ``idx_int`` passed
    at runtime. This matches the audit's static-shape contract: the
    only data-dependent shape would be `n_int`, and that is resolved by
    making `idx_int` a graph input rather than computing it via
    `NonZero` inside the graph (audit Stage 3 friction note).
    """
    nodes: List[onnx.NodeProto] = []

    # ---------- Graph inputs ----------
    nodes_in = oh.make_tensor_value_info(
        "nodes", TensorProto.DOUBLE, shape=[n_nodes, 3]
    )
    tets_in = oh.make_tensor_value_info(
        "tets", TensorProto.INT64, shape=[n_elem, 4]
    )
    idx_in = oh.make_tensor_value_info(
        "idx_int", TensorProto.INT64, shape=["n_int"]
    )

    # ---------- Stage 0: gather elem_coords[e, i, :] = nodes[tets[e, i], :] ----------
    # ONNX `Gather(nodes, tets, axis=0)` returns shape (n_elem, 4, 3)
    # because Gather flattens the indexed axis with the indices shape.
    nodes.append(oh.make_node(
        "Gather",
        inputs=["nodes", "tets"],
        outputs=["elem_coords"],
        axis=0,
    ))

    # ---------- Stage 1: P1 local matrices ----------
    _p1_local_nodes(nodes)

    # ---------- Stage 2: global K/M scatter-add ----------
    _scatter_assemble_nodes(nodes, n_nodes, n_elem)

    # ---------- Stage 3: Dirichlet restriction ----------
    _dirichlet_restrict_nodes(nodes)

    # ---------- Graph outputs ----------
    k_int_out = oh.make_tensor_value_info(
        "K_int", TensorProto.DOUBLE, shape=["n_int", "n_int"]
    )
    m_int_out = oh.make_tensor_value_info(
        "M_int", TensorProto.DOUBLE, shape=["n_int", "n_int"]
    )

    graph = oh.make_graph(
        nodes,
        name="cube_cavity_assembly",
        inputs=[nodes_in, tets_in, idx_in],
        outputs=[k_int_out, m_int_out],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=IR_VERSION,
    )


__all__ = ["build_cube_cavity_graph", "OPSET", "IR_VERSION"]
