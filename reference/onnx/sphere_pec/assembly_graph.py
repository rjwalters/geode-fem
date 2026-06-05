"""End-to-end ONNX partial-assembly graph for the sphere-PEC Nédélec spine.

Epic #88, Phase G.7 (issue #140). This is the static-graph payload that
the G.6 audit (`reference/onnx/audit/sphere_pec/nedelec_operator_audit.md`,
PR #138) green-lit. It is the Nédélec analog of the cube-cavity F.2 graph
in `reference/onnx/cube_cavity/assembly_graph.py`, and the ONNX sibling of:

  - TF-Java: `reference/tf_java/sphere_pec/.../NedelecAssemblyGraph.java`
  - NumPy:   `reference/numpy/sphere_pec.py`

The graph closes the assembly spine at the Dirichlet boundary — i.e. it
emits ``K_int`` and ``M_int`` (interior-DOF restricted) and stops. The
generalized eigensolve remains a host-side sidecar boundary (shared L4
friction across every backend: there is no sparse generalized eigensolver
in ONNX).

Host-computed topology (the G.6 audit verdict)
==============================================

The central finding of the G.6 audit (Stage 2b verdict: NOT EXPRESSIBLE)
is that ``build_edges`` — the deduplication + inverse-map step that
turns the tet connectivity into the global Nédélec edge DOF table — is
a **secretly-imperative** L4 escape. It cannot be expressed in ONNX's
graph-only IR.

The graph therefore accepts the following host-computed inputs:

  - ``nodes``         f64 ``(n_nodes, 3)``  — mesh vertex coordinates
  - ``tets``          i64 ``(n_tets, 4)``   — tet connectivity
  - ``edges``         i64 ``(n_edges, 2)``  — host-computed via ``build_edges``
  - ``tet_edge_idx``  i64 ``(n_tets, 6)``   — host-computed via ``build_edges``
  - ``tet_edge_sign`` f64 ``(n_tets, 6)``   — host-computed via ``build_edges``
  - ``epsilon_r``     f64 ``(n_tets,)``     — host-computed via ``build_epsilon_r``
  - ``interior_idx``  i64 ``(n_int,)``      — host-computed via ``flatnonzero(pec_mask)``

The constants ``n_nodes``, ``n_tets``, ``n_edges`` are baked at graph
generation time (per-mesh specialization). The interior-DOF count
``n_int`` is left dynamic in the graph (the Dirichlet Gather honors any
length of ``interior_idx`` passed at runtime), exactly matching the F.2
contract.

Graph stages (per the G.6 audit's per-stage operator inventory)
===============================================================

1. **ε_r assignment** — already host-computed in this builder; we accept
   ``epsilon_r`` directly rather than re-deriving it inside the graph
   (Stage 1, audit: clean / Equal + Where; deferred to host for
   symmetry with the TF-Java driver pattern).
2. **Per-element Nédélec 6×6 local matrices** (audit Stage 3) — cofactor
   Gram via Einsum, then 36 entry-pair Gathers + Mul/Sub/Add to form the
   K and M numerators, finally broadcast-scale by ``(2/3)/|det|^3`` and
   ``1/(120 |det|)`` respectively.
3. **PEC mask computation** (audit Stage 4a) — ReduceL2(nodes) + Sub +
   Abs + Less to compute the boolean ``on_boundary`` per-node mask.
   The mask is exposed as a graph output for cross-backend validation;
   the index derivation ``flatnonzero(mask)`` itself remains host-side
   (audit Stage 4b: caveat — NonZero is graph-expressible but produces
   data-dependent shape; the host already computed ``interior_idx``).
4. **Global K/M scatter-add** (audit Stage 5, static-shape version) —
   sign outer product via Unsqueeze + Mul broadcasting, ε scaling, COO
   index table via Expand + Reshape + Concat, then ScatterND with
   ``reduction="add"`` onto a ConstantOfShape ``(n_edges, n_edges)``
   zero buffer. Because ``n_edges`` is baked at graph-generation time,
   the global buffer has static shape end-to-end.
5. **Dirichlet restriction** (audit Stage 6) — two successive Gather
   ops with the host-computed ``interior_idx``. Identical pattern to
   the F.2 cube-cavity graph; the only structural difference is that
   ``n_edges`` (rather than ``n_nodes``) is the indexed axis.

Authoring tool
==============

Raw ``onnx.helper`` (NOT ``onnxscript``), same as the F.2 cube-cavity
graph and the G.6 probe scripts. Rationale (from the F.1 audit, lines
51-55): ``onnxscript`` can hide imperative sugar that the audit needs to
surface at the IR level. The audit's contract — "every L4 op visible as
an opset-18 node" — only holds if we inherit the same authoring choice.
"""

from __future__ import annotations

import sys
from pathlib import Path
from typing import List

import numpy as np
import onnx
import onnx.helper as oh
from onnx import TensorProto

HERE = Path(__file__).resolve().parent
REFERENCE_ROOT = HERE.parent.parent
sys.path.insert(0, str(REFERENCE_ROOT / "numpy"))

from nedelec_local_matrices import TET_LOCAL_EDGES  # noqa: E402

OPSET = 18
IR_VERSION = 9


# The 36 (i,j) = ((a,b), (c,d)) pair-of-pairs, baked in as constants.
# This is the fixed 6x6 Nédélec local stiffness/mass index structure,
# identical to the one in `probe_nedelec_local.py`.
EDGE_PAIRS: list[tuple[int, int, int, int]] = [
    (a, b, c, d)
    for (a, b) in TET_LOCAL_EDGES
    for (c, d) in TET_LOCAL_EDGES
]


def _f(p: int, q: int) -> float:
    """Kronecker factor ``f_pq = 1 + delta_pq`` used by the mass entries."""
    return 2.0 if p == q else 1.0


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    """Wrap a small NumPy array as a `Constant` node in the graph.

    Local copy of the same helper used by the F.2 cube-cavity graph and
    the G.6 probe scripts. Same deferred-hygiene note: extraction to a
    shared ``reference/onnx/_helpers.py`` is a follow-up.
    """
    dtype_map = {
        np.dtype("float64"): TensorProto.DOUBLE,
        np.dtype("float32"): TensorProto.FLOAT,
        np.dtype("int64"): TensorProto.INT64,
        np.dtype("int32"): TensorProto.INT32,
        np.dtype("bool"): TensorProto.BOOL,
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


def _nedelec_local_nodes(nodes: List[onnx.NodeProto]) -> None:
    """Append nodes that compute (k_local, m_local) from `elem_coords`.

    Reads:
      ``elem_coords``  f64 (n_tets, 4, 3) — per-element vertex coords
    Writes:
      ``k_local``      f64 (n_tets, 6, 6)
      ``m_local``      f64 (n_tets, 6, 6)
      ``abs_det``      f64 (n_tets,)       — exposed for downstream debug

    Faithful port of the audit probe `probe_nedelec_local.py`, with
    unique constant/intermediate names so the graph stays SSA when used
    alongside the scatter step. Mirror of
    `reference/numpy/nedelec_local_matrices.py` and
    `crates/geode-core/src/nedelec.rs:199-210`.
    """
    # ---------- Axis constants ----------
    nodes.append(_const("ned_axis1", np.array([1], dtype=np.int64)))
    nodes.append(_const("ned_axis_neg1", np.array([-1], dtype=np.int64)))
    for i in range(4):
        nodes.append(_const(f"ned_idx_{i}", np.array(i, dtype=np.int64)))
    for c in range(3):
        nodes.append(_const(f"ned_comp_{c}", np.array(c, dtype=np.int64)))

    # ---------- Gather v0..v3 from elem_coords along axis=1 ----------
    for i in range(4):
        nodes.append(oh.make_node(
            "Gather",
            inputs=["elem_coords", f"ned_idx_{i}"],
            outputs=[f"ned_v{i}"],
            axis=1,
        ))

    # ---------- Edge vectors from v0 ----------
    for i in (1, 2, 3):
        nodes.append(oh.make_node("Sub", [f"ned_v{i}", "ned_v0"], [f"ned_e{i}"]))

    # ---------- Cross-product synthesis helpers ----------
    emitted_components: set = set()

    def emit_components(vec: str) -> tuple:
        outs = []
        for c in range(3):
            out_name = f"{vec}_c{c}"
            if out_name not in emitted_components:
                nodes.append(oh.make_node(
                    "Gather",
                    inputs=[vec, f"ned_comp_{c}"],
                    outputs=[out_name],
                    axis=1,
                ))
                emitted_components.add(out_name)
            outs.append(out_name)
        return tuple(outs)

    def emit_cross(a: str, b: str, out: str) -> None:
        ax, ay, az = emit_components(a)
        bx, by, bz = emit_components(b)
        nodes.append(oh.make_node("Mul", [ay, bz], [f"{out}_t1"]))
        nodes.append(oh.make_node("Mul", [az, by], [f"{out}_t2"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t1", f"{out}_t2"], [f"{out}_cx"]))
        nodes.append(oh.make_node("Mul", [az, bx], [f"{out}_t3"]))
        nodes.append(oh.make_node("Mul", [ax, bz], [f"{out}_t4"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t3", f"{out}_t4"], [f"{out}_cy"]))
        nodes.append(oh.make_node("Mul", [ax, by], [f"{out}_t5"]))
        nodes.append(oh.make_node("Mul", [ay, bx], [f"{out}_t6"]))
        nodes.append(oh.make_node("Sub", [f"{out}_t5", f"{out}_t6"], [f"{out}_cz"]))
        for comp in ("cx", "cy", "cz"):
            nodes.append(oh.make_node(
                "Unsqueeze",
                inputs=[f"{out}_{comp}", "ned_axis_neg1"],
                outputs=[f"{out}_{comp}_u"],
            ))
        nodes.append(oh.make_node(
            "Concat",
            inputs=[f"{out}_cx_u", f"{out}_cy_u", f"{out}_cz_u"],
            outputs=[out],
            axis=1,
        ))

    # g1 = cross(e2, e3), g2 = cross(e3, e1), g3 = cross(e1, e2)
    emit_cross("ned_e2", "ned_e3", "ned_g1")
    emit_cross("ned_e3", "ned_e1", "ned_g2")
    emit_cross("ned_e1", "ned_e2", "ned_g3")
    # g0 = -(g1 + g2 + g3)
    nodes.append(oh.make_node("Add", ["ned_g1", "ned_g2"], ["ned_g1_g2"]))
    nodes.append(oh.make_node("Add", ["ned_g1_g2", "ned_g3"], ["ned_g_sum"]))
    nodes.append(oh.make_node("Neg", ["ned_g_sum"], ["ned_g0"]))

    # ---------- det = sum(e1 * g1) along axis=1; abs_det = |det| ----------
    nodes.append(oh.make_node("Mul", ["ned_e1", "ned_g1"], ["ned_e1g1"]))
    nodes.append(oh.make_node(
        "ReduceSum",
        inputs=["ned_e1g1", "ned_axis1"],
        outputs=["ned_det"],
        keepdims=0,
    ))
    nodes.append(oh.make_node("Abs", ["ned_det"], ["abs_det"]))

    # ---------- Gram matrix gg (N, 4, 4) via Einsum: "eik,ejk->eij" ----------
    for g in ("ned_g0", "ned_g1", "ned_g2", "ned_g3"):
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[g, "ned_axis1"],
            outputs=[f"{g}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=["ned_g0_u", "ned_g1_u", "ned_g2_u", "ned_g3_u"],
        outputs=["ned_g_mat"],
        axis=1,
    ))
    nodes.append(oh.make_node(
        "Einsum",
        inputs=["ned_g_mat", "ned_g_mat"],
        outputs=["ned_gg"],
        equation="eik,ejk->eij",
    ))

    # ---------- Per-element scale factors ----------
    # inv_abs_det = 1 / abs_det  (N,)
    nodes.append(_const("ned_one_f64", np.array(1.0, dtype=np.float64)))
    nodes.append(oh.make_node("Div", ["ned_one_f64", "abs_det"], ["ned_inv_abs_det"]))
    nodes.append(oh.make_node(
        "Mul", ["ned_inv_abs_det", "ned_inv_abs_det"], ["ned_inv_abs_det2"]
    ))
    nodes.append(oh.make_node(
        "Mul", ["ned_inv_abs_det2", "ned_inv_abs_det"], ["ned_inv_abs_det3"]
    ))

    # Reshape scalars (N,) -> (N, 1, 1) for broadcasting against (N, 6, 6)
    nodes.append(_const("ned_shape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node(
        "Reshape", ["ned_inv_abs_det3", "ned_shape_n11"], ["ned_inv_det3_b"]
    ))
    nodes.append(oh.make_node(
        "Reshape", ["ned_inv_abs_det", "ned_shape_n11"], ["ned_inv_det_b"]
    ))

    # ---------- Per-pair Gram entry extraction ----------
    # For each unique (p, q) pair in {0,1,2,3} x {0,1,2,3} extract gg[:, p, q]
    # via two successive Gather ops (axis=1 → axis=1 on the 4-vector slice).
    emitted_rows: set = set()
    emitted_gg: set = set()

    def emit_gg(p: int, q: int) -> str:
        key = (p, q)
        name = f"ned_gg_{p}{q}"
        if key in emitted_gg:
            return name
        row_name = f"ned_gg_row{p}"
        if p not in emitted_rows:
            nodes.append(oh.make_node(
                "Gather",
                inputs=["ned_gg", f"ned_idx_{p}"],
                outputs=[row_name],
                axis=1,
            ))
            emitted_rows.add(p)
        nodes.append(oh.make_node(
            "Gather",
            inputs=[row_name, f"ned_idx_{q}"],
            outputs=[name],
            axis=1,
        ))
        emitted_gg.add(key)
        return name

    # Build K_ij and M_ij for each of 36 edge pairs.
    k_entries: list[str] = []
    m_entries: list[str] = []

    for idx, (a, b, c, d) in enumerate(EDGE_PAIRS):
        gg_ac = emit_gg(a, c)
        gg_ad = emit_gg(a, d)
        gg_bc = emit_gg(b, c)
        gg_bd = emit_gg(b, d)

        # K_{ij} = (2/3) * (gg_ac * gg_bd - gg_ad * gg_bc) / |det|^3
        nodes.append(oh.make_node("Mul", [gg_ac, gg_bd], [f"ned_kp{idx}_1"]))
        nodes.append(oh.make_node("Mul", [gg_ad, gg_bc], [f"ned_kp{idx}_2"]))
        nodes.append(oh.make_node(
            "Sub", [f"ned_kp{idx}_1", f"ned_kp{idx}_2"], [f"ned_kp{idx}_diff"]
        ))
        k_entries.append(f"ned_kp{idx}_diff")

        # M_{ij} terms with Kronecker factors (baked as float constants).
        f_ac = _f(a, c)
        f_ad = _f(a, d)
        f_bc = _f(b, c)
        f_bd = _f(b, d)

        nodes.append(_const(f"ned_fac_{idx}_ac", np.array(f_ac, dtype=np.float64)))
        nodes.append(_const(f"ned_fac_{idx}_ad", np.array(f_ad, dtype=np.float64)))
        nodes.append(_const(f"ned_fac_{idx}_bc", np.array(f_bc, dtype=np.float64)))
        nodes.append(_const(f"ned_fac_{idx}_bd", np.array(f_bd, dtype=np.float64)))

        nodes.append(oh.make_node("Mul", [f"ned_fac_{idx}_ac", gg_bd], [f"ned_mp{idx}_1"]))
        nodes.append(oh.make_node("Mul", [f"ned_fac_{idx}_ad", gg_bc], [f"ned_mp{idx}_2"]))
        nodes.append(oh.make_node("Mul", [f"ned_fac_{idx}_bc", gg_ad], [f"ned_mp{idx}_3"]))
        nodes.append(oh.make_node("Mul", [f"ned_fac_{idx}_bd", gg_ac], [f"ned_mp{idx}_4"]))
        nodes.append(oh.make_node(
            "Sub", [f"ned_mp{idx}_1", f"ned_mp{idx}_2"], [f"ned_mp{idx}_a"]
        ))
        nodes.append(oh.make_node(
            "Sub", [f"ned_mp{idx}_a", f"ned_mp{idx}_3"], [f"ned_mp{idx}_b"]
        ))
        nodes.append(oh.make_node(
            "Add", [f"ned_mp{idx}_b", f"ned_mp{idx}_4"], [f"ned_mp{idx}_term"]
        ))
        m_entries.append(f"ned_mp{idx}_term")

    # ---------- Stack 36 (N,) entries → (N, 36) → reshape to (N, 6, 6) ----------
    for name in k_entries:
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[name, "ned_axis_neg1"],
            outputs=[f"{name}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=[f"{name}_u" for name in k_entries],
        outputs=["ned_k_flat"],
        axis=1,
    ))
    nodes.append(_const("ned_shape_n66", np.array([-1, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["ned_k_flat", "ned_shape_n66"], ["ned_k_raw"]))
    nodes.append(_const("ned_two_thirds", np.array(2.0 / 3.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["ned_two_thirds", "ned_inv_det3_b"], ["ned_k_scale"]))
    nodes.append(oh.make_node("Mul", ["ned_k_raw", "ned_k_scale"], ["k_local"]))

    for name in m_entries:
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[name, "ned_axis_neg1"],
            outputs=[f"{name}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=[f"{name}_u" for name in m_entries],
        outputs=["ned_m_flat"],
        axis=1,
    ))
    nodes.append(oh.make_node("Reshape", ["ned_m_flat", "ned_shape_n66"], ["ned_m_raw"]))
    nodes.append(_const("ned_inv_120", np.array(1.0 / 120.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["ned_inv_120", "ned_inv_det_b"], ["ned_m_scale"]))
    nodes.append(oh.make_node("Mul", ["ned_m_raw", "ned_m_scale"], ["m_local"]))


def _pec_mask_nodes(nodes: List[onnx.NodeProto], r_outer: float) -> None:
    """Append nodes that compute the per-node PEC boundary mask.

    Reads:
      ``nodes``      f64 (n_nodes, 3)  — graph input
    Writes:
      ``on_boundary`` bool (n_nodes,)  — exposed for cross-backend validation

    Mirrors steps 1-4 of `probe_pec_mask.py` and the NumPy reference
    `sphere_pec.sphere_pec_interior_edges`. Step 5 (NonZero / idx
    derivation) is intentionally left to the host per the audit's Stage
    4b verdict — ``interior_idx`` enters as a separate graph input.
    """
    tol = 1e-6 * max(r_outer, 1.0)

    nodes.append(_const("pec_axis1", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ReduceL2",
        inputs=["nodes", "pec_axis1"],
        outputs=["pec_r"],
        keepdims=0,
    ))
    nodes.append(_const("pec_r_outer", np.array(r_outer, dtype=np.float64)))
    nodes.append(_const("pec_tol", np.array(tol, dtype=np.float64)))
    nodes.append(oh.make_node("Sub", ["pec_r", "pec_r_outer"], ["pec_r_minus"]))
    nodes.append(oh.make_node("Abs", ["pec_r_minus"], ["pec_r_dist"]))
    nodes.append(oh.make_node("Less", ["pec_r_dist", "pec_tol"], ["on_boundary"]))


def _scatter_assemble_nodes(
    nodes: List[onnx.NodeProto], n_tets: int, n_edges: int
) -> None:
    """Append nodes that scatter `k_local`/`m_local` into dense globals.

    Reads:
      ``k_local``       f64 (n_tets, 6, 6)
      ``m_local``       f64 (n_tets, 6, 6)
      ``tet_edge_idx``  i64 (n_tets, 6)  — graph input (host-computed)
      ``tet_edge_sign`` f64 (n_tets, 6)  — graph input (host-computed)
      ``epsilon_r``     f64 (n_tets,)    — graph input (host-computed)
    Writes:
      ``k_global``      f64 (n_edges, n_edges)
      ``m_global``      f64 (n_edges, n_edges)

    Mirrors `probe_nedelec_scatter.py` and the NumPy reference
    `sphere_pec.assemble_global_nedelec`. Because ``n_edges`` is baked at
    graph-build time (per-mesh specialization), the global buffer is
    statically shaped — the audit's Stage 5 "PARTIAL" caveat about
    data-dependent shape is sidestepped by host-baking ``n_edges``.
    """
    n_pairs = n_tets * 36  # flat (e, i, j) index count

    # ---------- Constants ----------
    nodes.append(_const("sct_ax1", np.array([1], dtype=np.int64)))
    nodes.append(_const("sct_ax2", np.array([2], dtype=np.int64)))
    nodes.append(_const("sct_ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(_const("sct_shape_n66", np.array([n_tets, 6, 6], dtype=np.int64)))
    nodes.append(_const("sct_flat_shape", np.array([n_pairs], dtype=np.int64)))
    nodes.append(_const("sct_buf_shape", np.array([n_edges, n_edges], dtype=np.int64)))
    nodes.append(_const("sct_eps_shape", np.array([-1, 1, 1], dtype=np.int64)))

    # ---------- (1) Sign outer product ----------
    # tet_edge_sign: (n_tets, 6) -> (n_tets, 6, 1) and (n_tets, 1, 6)
    nodes.append(oh.make_node(
        "Unsqueeze", ["tet_edge_sign", "sct_ax2"], ["sct_sign_col"]
    ))
    nodes.append(oh.make_node(
        "Unsqueeze", ["tet_edge_sign", "sct_ax1"], ["sct_sign_row"]
    ))
    nodes.append(oh.make_node(
        "Mul", ["sct_sign_col", "sct_sign_row"], ["sct_sign_outer"]
    ))

    # ---------- (2) Apply sign to k_local ----------
    nodes.append(oh.make_node("Mul", ["k_local", "sct_sign_outer"], ["sct_k_signed"]))

    # ---------- (3) Apply sign + epsilon to m_local ----------
    # epsilon_r: (n_tets,) -> (n_tets, 1, 1) for broadcast.
    nodes.append(oh.make_node("Reshape", ["epsilon_r", "sct_eps_shape"], ["sct_eps_b"]))
    nodes.append(oh.make_node(
        "Mul", ["m_local", "sct_sign_outer"], ["sct_m_signed_pre"]
    ))
    nodes.append(oh.make_node(
        "Mul", ["sct_m_signed_pre", "sct_eps_b"], ["sct_m_signed"]
    ))

    # ---------- (4) COO index construction ----------
    # rows[e,i,j] = tet_edge_idx[e,i], cols[e,i,j] = tet_edge_idx[e,j]
    nodes.append(oh.make_node(
        "Unsqueeze", ["tet_edge_idx", "sct_ax2"], ["sct_tei_col"]
    ))
    nodes.append(oh.make_node(
        "Unsqueeze", ["tet_edge_idx", "sct_ax1"], ["sct_tei_row"]
    ))
    nodes.append(oh.make_node(
        "Expand", ["sct_tei_col", "sct_shape_n66"], ["sct_rows_3d"]
    ))
    nodes.append(oh.make_node(
        "Expand", ["sct_tei_row", "sct_shape_n66"], ["sct_cols_3d"]
    ))

    # Flatten to (n_pairs,)
    nodes.append(oh.make_node(
        "Reshape", ["sct_rows_3d", "sct_flat_shape"], ["sct_rows_flat"]
    ))
    nodes.append(oh.make_node(
        "Reshape", ["sct_cols_3d", "sct_flat_shape"], ["sct_cols_flat"]
    ))
    nodes.append(oh.make_node(
        "Reshape", ["sct_k_signed", "sct_flat_shape"], ["sct_k_vals"]
    ))
    nodes.append(oh.make_node(
        "Reshape", ["sct_m_signed", "sct_flat_shape"], ["sct_m_vals"]
    ))

    # Stack (rows, cols) into (n_pairs, 2) index table.
    nodes.append(oh.make_node(
        "Unsqueeze", ["sct_rows_flat", "sct_ax_neg1"], ["sct_rows_col"]
    ))
    nodes.append(oh.make_node(
        "Unsqueeze", ["sct_cols_flat", "sct_ax_neg1"], ["sct_cols_col"]
    ))
    nodes.append(oh.make_node(
        "Concat", ["sct_rows_col", "sct_cols_col"], ["sct_indices"], axis=1
    ))

    # ---------- (5) Zero buffers ----------
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["sct_buf_shape"],
        outputs=["sct_k_zero"],
        value=oh.make_tensor(
            name="sct_k_zero_value",
            data_type=TensorProto.DOUBLE,
            dims=[1],
            vals=[0.0],
        ),
    ))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["sct_buf_shape"],
        outputs=["sct_m_zero"],
        value=oh.make_tensor(
            name="sct_m_zero_value",
            data_type=TensorProto.DOUBLE,
            dims=[1],
            vals=[0.0],
        ),
    ))

    # ---------- (6) ScatterND(reduction="add") ----------
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["sct_k_zero", "sct_indices", "sct_k_vals"],
        outputs=["k_global"],
        reduction="add",
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["sct_m_zero", "sct_indices", "sct_m_vals"],
        outputs=["m_global"],
        reduction="add",
    ))


def _dirichlet_restrict_nodes(nodes: List[onnx.NodeProto]) -> None:
    """Append nodes that compute `K_int = K[idx, :][:, idx]` and the same for M.

    Reads:
      ``k_global``      f64 (n_edges, n_edges)
      ``m_global``      f64 (n_edges, n_edges)
      ``interior_idx``  i64 (n_int,)   — graph input (host-computed)
    Writes:
      ``K_int``         f64 (n_int, n_int)
      ``M_int``         f64 (n_int, n_int)

    Identical pattern to the cube-cavity F.2 Dirichlet step; the only
    difference is that the indexed axis is the edge axis (length
    ``n_edges``) rather than the node axis.
    """
    nodes.append(oh.make_node(
        "Gather",
        inputs=["k_global", "interior_idx"],
        outputs=["dir_k_rows"],
        axis=0,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["dir_k_rows", "interior_idx"],
        outputs=["K_int"],
        axis=1,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["m_global", "interior_idx"],
        outputs=["dir_m_rows"],
        axis=0,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["dir_m_rows", "interior_idx"],
        outputs=["M_int"],
        axis=1,
    ))


def build_sphere_pec_graph(
    n_nodes: int,
    n_tets: int,
    n_edges: int,
    r_outer: float,
) -> onnx.ModelProto:
    """Build the end-to-end ONNX sphere-PEC partial-assembly graph.

    Parameters
    ----------
    n_nodes : int
        Total node count of the mesh.
    n_tets : int
        Total tet count of the mesh.
    n_edges : int
        Total edge count, as computed by `build_edges` on the host. Baked
        into the global buffer shape so the scatter remains statically
        shaped (sidesteps the audit's Stage 5 dynamic-shape caveat).
    r_outer : float
        Outer PEC wall radius for the PEC mask computation (= R_BUFFER).

    Notes
    -----
    The interior-DOF count ``n_int`` is *not* baked into the graph — the
    Dirichlet Gather honors any length of ``interior_idx`` passed at
    runtime. This matches the F.2 contract exactly and keeps the graph
    re-usable across PEC instantiations on the same mesh.
    """
    nodes: List[onnx.NodeProto] = []

    # ---------- Graph inputs ----------
    nodes_in = oh.make_tensor_value_info(
        "nodes", TensorProto.DOUBLE, shape=[n_nodes, 3]
    )
    tets_in = oh.make_tensor_value_info(
        "tets", TensorProto.INT64, shape=[n_tets, 4]
    )
    edges_in = oh.make_tensor_value_info(
        "edges", TensorProto.INT64, shape=[n_edges, 2]
    )
    tei_in = oh.make_tensor_value_info(
        "tet_edge_idx", TensorProto.INT64, shape=[n_tets, 6]
    )
    tes_in = oh.make_tensor_value_info(
        "tet_edge_sign", TensorProto.DOUBLE, shape=[n_tets, 6]
    )
    eps_in = oh.make_tensor_value_info(
        "epsilon_r", TensorProto.DOUBLE, shape=[n_tets]
    )
    idx_in = oh.make_tensor_value_info(
        "interior_idx", TensorProto.INT64, shape=["n_int"]
    )

    # ---------- Stage 0: gather elem_coords[e, i, :] = nodes[tets[e, i], :] ----------
    nodes.append(oh.make_node(
        "Gather",
        inputs=["nodes", "tets"],
        outputs=["elem_coords"],
        axis=0,
    ))

    # ---------- Stage 1: Nédélec local 6x6 matrices ----------
    _nedelec_local_nodes(nodes)

    # ---------- Stage 2: PEC boundary mask (per-node) ----------
    # Exposed as `on_boundary` graph output for cross-backend validation
    # of the audit's Stage 4a "mask is graph-expressible" finding.
    _pec_mask_nodes(nodes, r_outer)

    # ---------- Stage 3: global K/M scatter-add ----------
    _scatter_assemble_nodes(nodes, n_tets, n_edges)

    # ---------- Stage 4: Dirichlet restriction (interior_idx, host) ----------
    _dirichlet_restrict_nodes(nodes)

    # ---------- Graph outputs ----------
    k_int_out = oh.make_tensor_value_info(
        "K_int", TensorProto.DOUBLE, shape=["n_int", "n_int"]
    )
    m_int_out = oh.make_tensor_value_info(
        "M_int", TensorProto.DOUBLE, shape=["n_int", "n_int"]
    )
    # Intermediate outputs exposed for audit cross-checks (G.7 AC).
    k_global_out = oh.make_tensor_value_info(
        "k_global", TensorProto.DOUBLE, shape=[n_edges, n_edges]
    )
    m_global_out = oh.make_tensor_value_info(
        "m_global", TensorProto.DOUBLE, shape=[n_edges, n_edges]
    )
    on_boundary_out = oh.make_tensor_value_info(
        "on_boundary", TensorProto.BOOL, shape=[n_nodes]
    )
    # The unused `edges` input is silenced via Identity → discarded;
    # we accept it on the boundary so the host driver can stage it
    # alongside the other build_edges outputs without needing a wrapper.
    nodes.append(oh.make_node("Identity", ["edges"], ["edges_passthrough"]))
    edges_passthrough_out = oh.make_tensor_value_info(
        "edges_passthrough", TensorProto.INT64, shape=[n_edges, 2]
    )

    graph = oh.make_graph(
        nodes,
        name="sphere_pec_assembly",
        inputs=[nodes_in, tets_in, edges_in, tei_in, tes_in, eps_in, idx_in],
        outputs=[
            k_int_out,
            m_int_out,
            k_global_out,
            m_global_out,
            on_boundary_out,
            edges_passthrough_out,
        ],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=IR_VERSION,
    )


__all__ = [
    "build_sphere_pec_graph",
    "EDGE_PAIRS",
    "OPSET",
    "IR_VERSION",
]
