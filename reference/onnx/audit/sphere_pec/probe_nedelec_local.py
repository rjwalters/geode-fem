"""Probe: ONNX expressibility of Nedelec element-local 6x6 curl-curl matrix.

Epic #88, Phase G.6 (issue #135). This probe asks whether the per-element
6x6 Nedelec curl-curl and mass matrices can be expressed as a pure ONNX
graph, mirroring `reference/numpy/nedelec_local_matrices.py` and the
NumPy reference in `reference/numpy/sphere_pec.py`.

Assembly spine for one element
==============================

Stage 1 of the sphere PEC assembly generates, per tet:

    (1) cofactor-form area-weighted gradients g_0..g_3  (shapes (N, 3))
    (2) cofactor Gram matrix gg[e, p, q] = g_p . g_q    (N, 4, 4)
    (3) |det(J)| per element                             (N,)
    (4) K_{ij} = (2/3) * (gg_ac gg_bd - gg_ad gg_bc) / |det|^3
    (5) M_{ij} = (1/(120|det|)) * [f_ac gg_bd - f_ad gg_bc
                                    - f_bc gg_ad + f_bd gg_ac]

where i = (a,b) and j = (c,d) range over TET_LOCAL_EDGES (6 pairs each),
and f_pq = (1 + delta_pq).

The key structural difference from the P1 case (Stage 1 of cube-cavity):
- P1 outputs a (N, 4, 4) matrix (nodes x nodes)
- Nedelec outputs a (N, 6, 6) matrix (edges x edges)
- The 6x6 index structure (TET_LOCAL_EDGES pair-of-pairs) must be
  baked in as constants; there is no looped dynamic dispatch in the
  static graph.

The good news: every operation on `gg` (element-wise products, constant
scalar multiplications, sums) is a combination of Gather, Mul, Sub, Add,
and Div. The 6x6 structure is FIXED for all tets (shape does not depend
on data values). The same Einsum operator or explicit Gather decomposition
that works for P1 generalizes here.

Strategy
========

We build the graph with `onnx.helper` (NOT `onnxscript`). The Gram
matrix gg is computed as g_mat @ g_mat^T via Einsum (cleaner than the
MatMul + Transpose used in probe_p1_local.py because gg has shape
(N, 4, 4) but we need to slice 6x6 entries by (a,b,c,d) tuples). Then
each of the 36 K_{ij} and M_{ij} entries is assembled by:

    Gather(gg, row=a, col=c) * Gather(gg, row=b, col=d) - ...

Explicitly: 36 * 4 = 144 Gather ops + 36 * (2 Mul + 1 Sub) K-entries
plus 36 * (4 Mul + 3 Add/Sub) M-entries. This is graph-correct and
corresponds exactly to the nested for-loop in `nedelec_local_matrices.py`.

In practice a real Nedelec graph would use Einsum to express the 6x6
index contraction more compactly. We demonstrate that path as a
`PARTIAL: EXPRESSIBLE via Einsum` note — but build the explicit Gather
version to surface the friction at the IR level.

Run
===

    python3 reference/onnx/audit/sphere_pec/probe_nedelec_local.py
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

from reference.numpy.nedelec_local_matrices import (  # noqa: E402
    TET_LOCAL_EDGES,
    batched_nedelec_local_matrices,
)

OPSET = 18

# The 36 (i,j) = ((a,b), (c,d)) pair-of-pairs, baked in as constants.
# This is the fixed 6x6 Nedelec local stiffness/mass index structure.
EDGE_PAIRS: list[tuple[int, int, int, int]] = [
    (a, b, c, d)
    for (a, b) in TET_LOCAL_EDGES
    for (c, d) in TET_LOCAL_EDGES
]

# Kronecker delta factors f_pq = 1 + delta_pq, precomputed for all (a,c)
# (b,d) (a,d) (b,c) pairs used by the mass formula.
def _f(p: int, q: int) -> float:
    return 2.0 if p == q else 1.0


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
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


def build_nedelec_local_graph() -> onnx.ModelProto:
    """Build a graph that takes elem_coords (N, 4, 3) f64 and returns
    k_local (N, 6, 6) and m_local (N, 6, 6).

    Inner loop structure:
      1. Compute g0..g3 (cofactor gradients) via Sub + cross synthesis.
      2. Compute gg (4x4 Gram matrix) via Einsum.
      3. Compute |det| via ReduceSum.
      4. For each of 36 edge pairs, extract 4 Gram entries via Gather
         and form K_{ij}, M_{ij}.
      5. Stack the 36 entries into (N, 6, 6) via Reshape.
    """
    nodes: list[onnx.NodeProto] = []

    elem_coords_vi = oh.make_tensor_value_info(
        "elem_coords", TensorProto.DOUBLE, shape=["N", 4, 3]
    )

    # --- Axis constants ---
    nodes.append(_const("axis1_const", np.array([1], dtype=np.int64)))
    nodes.append(_const("axis_neg1_const", np.array([-1], dtype=np.int64)))
    for i in range(4):
        nodes.append(_const(f"idx_{i}", np.array(i, dtype=np.int64)))

    # --- Gather v0..v3 from elem_coords along axis=1 ---
    for i in range(4):
        nodes.append(oh.make_node(
            "Gather",
            inputs=["elem_coords", f"idx_{i}"],
            outputs=[f"v{i}"],
            axis=1,
        ))

    # --- Edge vectors from v0 ---
    for i in (1, 2, 3):
        nodes.append(oh.make_node("Sub", [f"v{i}", "v0"], [f"e{i}"]))

    # --- Cross product synthesis: cross(a, b) -> (N, 3) ---
    # ONNX has no native Cross. We synthesize as 6 Mul + 3 Sub + Unsqueeze + Concat.
    nodes.append(_const("comp_0", np.array(0, dtype=np.int64)))
    nodes.append(_const("comp_1", np.array(1, dtype=np.int64)))
    nodes.append(_const("comp_2", np.array(2, dtype=np.int64)))

    emitted_components: set[str] = set()

    def emit_components(vec: str) -> tuple[str, str, str]:
        outs = []
        for c in range(3):
            out_name = f"{vec}_c{c}"
            if out_name not in emitted_components:
                nodes.append(oh.make_node(
                    "Gather",
                    inputs=[vec, f"comp_{c}"],
                    outputs=[out_name],
                    axis=1,
                ))
                emitted_components.add(out_name)
            outs.append(out_name)
        return tuple(outs)  # type: ignore[return-value]

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
                inputs=[f"{out}_{comp}", "axis_neg1_const"],
                outputs=[f"{out}_{comp}_u"],
            ))
        nodes.append(oh.make_node(
            "Concat",
            inputs=[f"{out}_cx_u", f"{out}_cy_u", f"{out}_cz_u"],
            outputs=[out],
            axis=1,
        ))

    # g1 = cross(e2, e3), g2 = cross(e3, e1), g3 = cross(e1, e2)
    emit_cross("e2", "e3", "g1")
    emit_cross("e3", "e1", "g2")
    emit_cross("e1", "e2", "g3")
    # g0 = -(g1 + g2 + g3)
    nodes.append(oh.make_node("Add", ["g1", "g2"], ["g1_g2"]))
    nodes.append(oh.make_node("Add", ["g1_g2", "g3"], ["g_sum"]))
    nodes.append(oh.make_node("Neg", ["g_sum"], ["g0"]))

    # --- det = sum(e1 * g1) along axis=1 ---
    nodes.append(oh.make_node("Mul", ["e1", "g1"], ["e1g1"]))
    nodes.append(oh.make_node(
        "ReduceSum",
        inputs=["e1g1", "axis1_const"],
        outputs=["det"],
        keepdims=0,
    ))
    nodes.append(oh.make_node("Abs", ["det"], ["abs_det"]))

    # --- Gram matrix gg (N, 4, 4) via Einsum: "eik,ejk->eij" ---
    # Stack g0..g3 into g_mat (N, 4, 3)
    for g in ("g0", "g1", "g2", "g3"):
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[g, "axis1_const"],
            outputs=[f"{g}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=["g0_u", "g1_u", "g2_u", "g3_u"],
        outputs=["g_mat"],
        axis=1,
    ))
    # Einsum: g_mat (N, 4, 3) x g_mat^T -> gg (N, 4, 4)
    nodes.append(oh.make_node(
        "Einsum",
        inputs=["g_mat", "g_mat"],
        outputs=["gg"],
        equation="eik,ejk->eij",
    ))

    # --- Per-element scale factors ---
    # inv_abs_det = 1 / abs_det  (N,)
    nodes.append(_const("one_f64", np.array(1.0, dtype=np.float64)))
    nodes.append(oh.make_node("Div", ["one_f64", "abs_det"], ["inv_abs_det"]))
    # inv_abs_det3 = inv_abs_det^3
    nodes.append(oh.make_node("Mul", ["inv_abs_det", "inv_abs_det"], ["inv_abs_det2"]))
    nodes.append(oh.make_node("Mul", ["inv_abs_det2", "inv_abs_det"], ["inv_abs_det3"]))

    # Reshape scalars (N,) -> (N, 1, 1) for broadcasting against (N, 6, 6)
    nodes.append(_const("shape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["inv_abs_det3", "shape_n11"], ["inv_det3_b"]))
    nodes.append(oh.make_node("Reshape", ["inv_abs_det", "shape_n11"], ["inv_det_b"]))

    # --- Per-pair Gram entry extraction ---
    # For each unique (p, q) pair in {0,1,2,3} x {0,1,2,3} that appears
    # in EDGE_PAIRS, extract gg[:, p, q] via Gather(axis=1) + Gather(axis=2).
    # Shape: (N, 4, 4) -Gather(ax=1, idx=p)-> (N, 4) -Gather(ax=1, idx=q)-> (N,)
    emitted_gg: set[tuple[int, int]] = set()

    def emit_gg(p: int, q: int) -> str:
        key = (p, q)
        name = f"gg_{p}{q}"
        if key not in emitted_gg:
            # First gather along axis=1 (rows)
            row_name = f"gg_row{p}"
            if (p, -1) not in emitted_gg:
                nodes.append(oh.make_node(
                    "Gather",
                    inputs=["gg", f"idx_{p}"],
                    outputs=[row_name],
                    axis=1,
                ))
                emitted_gg.add((p, -1))
            # Then gather along axis=1 again (cols of the row-slice)
            nodes.append(oh.make_node(
                "Gather",
                inputs=[row_name, f"idx_{q}"],
                outputs=[name],
                axis=1,
            ))
            emitted_gg.add(key)
        return name

    # Build K_ij and M_ij for each of 36 edge pairs
    k_entries: list[str] = []
    m_entries: list[str] = []

    for idx, (a, b, c, d) in enumerate(EDGE_PAIRS):
        gg_ac = emit_gg(a, c)
        gg_ad = emit_gg(a, d)
        gg_bc = emit_gg(b, c)
        gg_bd = emit_gg(b, d)

        # K_{ij} = (2/3) * (gg_ac * gg_bd - gg_ad * gg_bc) / |det|^3
        # We'll accumulate the cross products in a matrix and broadcast-divide later.
        nodes.append(oh.make_node("Mul", [gg_ac, gg_bd], [f"kp{idx}_1"]))
        nodes.append(oh.make_node("Mul", [gg_ad, gg_bc], [f"kp{idx}_2"]))
        nodes.append(oh.make_node("Sub", [f"kp{idx}_1", f"kp{idx}_2"], [f"kp{idx}_diff"]))
        k_entries.append(f"kp{idx}_diff")

        # M_{ij} terms with Kronecker factors (baked as float constants)
        f_ac = _f(a, c)
        f_ad = _f(a, d)
        f_bc = _f(b, c)
        f_bd = _f(b, d)

        # M_term = f_ac * gg_bd - f_ad * gg_bc - f_bc * gg_ad + f_bd * gg_ac
        nodes.append(_const(f"fac_{idx}_ac", np.array(f_ac, dtype=np.float64)))
        nodes.append(_const(f"fac_{idx}_ad", np.array(f_ad, dtype=np.float64)))
        nodes.append(_const(f"fac_{idx}_bc", np.array(f_bc, dtype=np.float64)))
        nodes.append(_const(f"fac_{idx}_bd", np.array(f_bd, dtype=np.float64)))

        nodes.append(oh.make_node("Mul", [f"fac_{idx}_ac", gg_bd], [f"mp{idx}_1"]))
        nodes.append(oh.make_node("Mul", [f"fac_{idx}_ad", gg_bc], [f"mp{idx}_2"]))
        nodes.append(oh.make_node("Mul", [f"fac_{idx}_bc", gg_ad], [f"mp{idx}_3"]))
        nodes.append(oh.make_node("Mul", [f"fac_{idx}_bd", gg_ac], [f"mp{idx}_4"]))
        nodes.append(oh.make_node("Sub", [f"mp{idx}_1", f"mp{idx}_2"], [f"mp{idx}_a"]))
        nodes.append(oh.make_node("Sub", [f"mp{idx}_a", f"mp{idx}_3"], [f"mp{idx}_b"]))
        nodes.append(oh.make_node("Add", [f"mp{idx}_b", f"mp{idx}_4"], [f"mp{idx}_term"]))
        m_entries.append(f"mp{idx}_term")

    # Stack 36 (N,) entries into (N, 36) -> reshape to (N, 6, 6)
    # Unsqueeze each to (N, 1) then Concat along axis=1
    for i, name in enumerate(k_entries):
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[name, "axis_neg1_const"],
            outputs=[f"{name}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=[f"{name}_u" for name in k_entries],
        outputs=["k_flat"],
        axis=1,
    ))
    nodes.append(_const("shape_n66", np.array([-1, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["k_flat", "shape_n66"], ["k_raw"]))
    # Apply (2/3) * (1/|det|^3) scale
    nodes.append(_const("two_thirds", np.array(2.0 / 3.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["two_thirds", "inv_det3_b"], ["k_scale"]))
    nodes.append(oh.make_node("Mul", ["k_raw", "k_scale"], ["k_local"]))

    for i, name in enumerate(m_entries):
        nodes.append(oh.make_node(
            "Unsqueeze",
            inputs=[name, "axis_neg1_const"],
            outputs=[f"{name}_u"],
        ))
    nodes.append(oh.make_node(
        "Concat",
        inputs=[f"{name}_u" for name in m_entries],
        outputs=["m_flat"],
        axis=1,
    ))
    nodes.append(oh.make_node("Reshape", ["m_flat", "shape_n66"], ["m_raw"]))
    # Apply (1/120) * (1/|det|) scale
    nodes.append(_const("inv_120", np.array(1.0 / 120.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["inv_120", "inv_det_b"], ["m_scale"]))
    nodes.append(oh.make_node("Mul", ["m_raw", "m_scale"], ["m_local"]))

    k_vi = oh.make_tensor_value_info("k_local", TensorProto.DOUBLE, shape=["N", 6, 6])
    m_vi = oh.make_tensor_value_info("m_local", TensorProto.DOUBLE, shape=["N", 6, 6])

    graph = oh.make_graph(
        nodes,
        name="nedelec_local_probe",
        inputs=[elem_coords_vi],
        outputs=[k_vi, m_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: Nedelec local 6x6 curl-curl + mass (sphere PEC, Phase G.6) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    model = build_nedelec_local_graph()

    try:
        onnx.checker.check_model(model)
        checker_status = "OK"
    except Exception as e:  # noqa: BLE001
        checker_status = f"FAIL ({e!r})"

    rt_status = "skipped"
    max_err_k = max_err_m = float("nan")
    try:
        sess = ort.InferenceSession(model.SerializeToString())
        # Canonical reference tet
        verts = np.array(
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            dtype=np.float64,
        )
        elem_coords = verts.reshape(1, 4, 3)

        outs = sess.run(["k_local", "m_local"], {"elem_coords": elem_coords})
        k_onnx, m_onnx = outs

        k_np, m_np, _ = batched_nedelec_local_matrices(elem_coords)
        max_err_k = float(np.max(np.abs(k_onnx - k_np)))
        max_err_m = float(np.max(np.abs(m_onnx - m_np)))
        rt_status = "OK"
    except Exception as e:  # noqa: BLE001
        rt_status = f"FAIL ({e!r})"

    print("Operator inventory for Nedelec 6x6 local matrix assembly:")
    print("--------------------------------------------------------------")
    print("  Gather (axis=1)       EXPRESSIBLE  lowers cleanly (opset 18 native, int64 idx)")
    print("  Sub / Add / Neg       EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("  Mul / Div             EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("  Abs                   EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("  ReduceSum             EXPRESSIBLE  lowers cleanly (opset 18 native, axes-as-input)")
    print("  Einsum (eik,ejk->eij) EXPRESSIBLE  lowers cleanly (opset 12+ native)")
    print("                                     NOTE: Einsum is preferred over MatMul+Transpose")
    print("                                     here because the 4x4 Gram contraction reads")
    print("                                     more clearly as the named index formula.")
    print("  Unsqueeze / Concat    EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("                                     — Stack synthesis (same as P1).")
    print("  Reshape               EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print()
    print("  cross product         EXPRESSIBLE  (synthesized: 6 Mul + 3 Sub + Unsqueeze + Concat)")
    print("                                     — same graph-only friction as cube-cavity P1.")
    print()
    print("Key structural difference from P1:")
    print("--------------------------------------------------------------")
    print("  P1: outputs (N, 4, 4)  — node x node, 16 entries")
    print("  Nedelec: outputs (N, 6, 6)  — edge x edge, 36 entries")
    print("  The 6x6 index structure (TET_LOCAL_EDGES pair-of-pairs) is")
    print("  baked in as graph-level constants. No dynamic dispatch; the")
    print("  static graph size grows by 36/16 = 2.25x vs. P1, but the")
    print("  graph-only expressibility classification is unchanged.")
    print()
    print("  The Gram matrix (4x4) is shared for both K and M, so the")
    print("  gg (N,4,4) tensor serves as the pivot: all 36 entry formulas")
    print("  reduce to 4-index Gather + arithmetic on gg columns.")
    print("  This is EXPRESSIBLE via Einsum or Gather decomposition.")
    print()
    print(f"onnx.checker.check_model: {checker_status}")
    print(f"onnxruntime execution: {rt_status}")
    if rt_status == "OK":
        print(f"  max |K_onnx - K_numpy| = {max_err_k:.3e}")
        print(f"  max |M_onnx - M_numpy| = {max_err_m:.3e}")
    print()
    if checker_status == "OK" and rt_status == "OK":
        print("Verdict: EXPRESSIBLE")
        print("  Nedelec 6x6 local matrices lower to pure ONNX opset-18 graph.")
        print("  Friction: same graph-only overhead as P1 (no Stack op, int64")
        print("  axis constants, cross-product synthesis). The 36-entry expansion")
        print("  is verbose but does not introduce any secretly-imperative escape.")
    else:
        print("Verdict: CHECK FAILED — see errors above.")

    return 0 if checker_status == "OK" and rt_status == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
