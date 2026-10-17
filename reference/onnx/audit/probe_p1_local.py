"""Probe: ONNX expressibility of P1 element-local matrices.

Epic #88, Phase F.1 (issue #116). This probe builds a candidate ONNX
graph that computes the per-element P1 stiffness and mass matrices for
the cube-cavity slice, mirroring `reference/jax/cube_cavity.py`
`_p1_local_one` (the JAX reference) and the TF-Java analog in
`reference/tf_java/cube_cavity/.../AssemblyGraph.java`.

Strategy
========

We hand-build the graph with `onnx.helper` (NOT `onnxscript`) so we can
observe each ONNX-IR-level operator directly. Where a primitive that's
native in NumPy/JAX has no ONNX equivalent, the probe records the
synthesis used (e.g. cross product as 6 muls + 3 subs + Concat) and
flags it as either:

  (a) `lowers cleanly`              — direct opset 18 operator
  (b) `lowers with synthesis`       — built from lower-level ops
  (c) `does not lower (graph-only)` — fundamentally graph-only friction
  (d) `does not lower (imperative)` — secretly-imperative L4 escape

After building, the graph is:
  1. Type-checked via `onnx.checker.check_model`.
  2. Executed via `onnxruntime.InferenceSession` against a known input
     (regular tet at the canonical reference coordinates).
  3. Compared numerically against the NumPy reference
     (`reference/numpy/p1_local_matrices.py`).

The numerical comparison is a sanity check that the lowering produces
the right answer — it is NOT a CI gate. The audit deliverable is the
operator inventory printed at the end.

Run
===

    python3 reference/onnx/audit/probe_p1_local.py
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
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[3])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

from reference.numpy.p1_local_matrices import batched_p1_local_matrices  # noqa: E402

OPSET = 18


# ---------------------------------------------------------------------------
# Candidate ONNX graph: per-element P1 local matrices
# ---------------------------------------------------------------------------

def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    """Wrap a small NumPy array as a `Constant` node in the graph."""
    return oh.make_node(
        "Constant",
        inputs=[],
        outputs=[name],
        value=oh.make_tensor(
            name=name + "_value",
            data_type=TensorProto.FLOAT if np_arr.dtype == np.float32 else
                      TensorProto.DOUBLE if np_arr.dtype == np.float64 else
                      TensorProto.INT64,
            dims=list(np_arr.shape),
            vals=np_arr.flatten().tolist(),
        ),
    )


def build_p1_local_graph() -> onnx.ModelProto:
    """Build a graph that takes `elem_coords` of shape (N, 4, 3) f64 and
    returns `K_local` (N, 4, 4) and `M_local` (N, 4, 4)."""
    nodes: list[onnx.NodeProto] = []

    # ----- Input: elem_coords (N, 4, 3) f64. N is symbolic. -----
    elem_coords_vi = oh.make_tensor_value_info(
        "elem_coords",
        TensorProto.DOUBLE,
        shape=["N", 4, 3],
    )

    # ----- Slice out v0..v3 along axis=1 with Gather. -----
    # Gather requires int64 indices in opset 18.
    nodes.append(_const("axis1_idx_0", np.array(0, dtype=np.int64)))
    nodes.append(_const("axis1_idx_1", np.array(1, dtype=np.int64)))
    nodes.append(_const("axis1_idx_2", np.array(2, dtype=np.int64)))
    nodes.append(_const("axis1_idx_3", np.array(3, dtype=np.int64)))

    for i in range(4):
        # Gather(input, indices, axis=1) — scalar index gives (N, 3).
        nodes.append(oh.make_node(
            "Gather",
            inputs=["elem_coords", f"axis1_idx_{i}"],
            outputs=[f"v{i}"],
            axis=1,
        ))

    # ----- Edge vectors e1 = v1 - v0, e2 = v2 - v0, e3 = v3 - v0. -----
    for i in (1, 2, 3):
        nodes.append(oh.make_node("Sub", [f"v{i}", "v0"], [f"e{i}"]))

    # ----- Per-row 3-vector cross products. -----
    # ONNX has no `cross` op. We synthesize cross(a, b) as:
    #   cx = ay*bz - az*by
    #   cy = az*bx - ax*bz
    #   cz = ax*by - ay*bx
    # then Stack along axis=1 with Unsqueeze + Concat.
    #
    # Components are gathered with axis=1 (after Gather we get shape (N,)
    # because the indexed axis collapses). Each component is later
    # Unsqueeze'd back to (N, 1) before Concat'ing to (N, 3).
    nodes.append(_const("axis_neg1_const", np.array([-1], dtype=np.int64)))
    # Shared per-component scalar indices (one set, reused across all
    # vector decompositions). Without caching, repeated calls to
    # `emit_components` would attempt to re-emit identical constants
    # and ONNX rejects the graph as non-SSA.
    for c in range(3):
        nodes.append(_const(f"comp_idx_{c}", np.array(c, dtype=np.int64)))

    emitted_components: set[str] = set()

    def emit_components(vec: str) -> tuple[str, str, str]:
        """Emit per-component gathers (N,) for one (N, 3) vector. Idempotent."""
        outs = []
        for c in range(3):
            out_name = f"{vec}_c{c}"
            if out_name not in emitted_components:
                nodes.append(oh.make_node(
                    "Gather",
                    inputs=[vec, f"comp_idx_{c}"],
                    outputs=[out_name],
                    axis=1,
                ))
                emitted_components.add(out_name)
            outs.append(out_name)
        return tuple(outs)  # type: ignore[return-value]

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
                inputs=[f"{out}_{comp}", "axis_neg1_const"],
                outputs=[f"{out}_{comp}_u"],
            ))
        nodes.append(oh.make_node(
            "Concat",
            inputs=[f"{out}_x_u", f"{out}_y_u", f"{out}_z_u"],
            outputs=[out],
            axis=1,
        ))

    emit_cross("e2", "e3", "g1")
    emit_cross("e3", "e1", "g2")
    emit_cross("e1", "e2", "g3")

    # ----- g0 = -(g1 + g2 + g3). -----
    nodes.append(oh.make_node("Add", ["g1", "g2"], ["g1_plus_g2"]))
    nodes.append(oh.make_node("Add", ["g1_plus_g2", "g3"], ["g_sum"]))
    nodes.append(oh.make_node("Neg", ["g_sum"], ["g0"]))

    # ----- det = sum(e1 * g1) along axis=1. -----
    nodes.append(oh.make_node("Mul", ["e1", "g1"], ["e1_g1"]))
    nodes.append(_const("axis1_const", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ReduceSum",
        inputs=["e1_g1", "axis1_const"],
        outputs=["det"],
        keepdims=0,
    ))
    nodes.append(oh.make_node("Abs", ["det"], ["abs_det"]))

    # ----- gMat = stack(g0, g1, g2, g3) along axis=1 → (N, 4, 3). -----
    # Reuse `axis1_const` (already defined for the ReduceSum above).
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

    # ----- gg = g_mat @ g_mat^T per-batch.
    # In ONNX, MatMul broadcasts over leading batch dims: with g_mat
    # shape (N, 4, 3) and g_mat_T shape (N, 3, 4), MatMul gives (N, 4, 4).
    # This is the L4-native behavior — no einsum trick required (unlike
    # TF-Java, which lacks rank-3 MatMul broadcasting and required
    # einsum). Documented in the audit.
    nodes.append(oh.make_node(
        "Transpose",
        inputs=["g_mat"],
        outputs=["g_mat_T"],
        perm=[0, 2, 1],
    ))
    nodes.append(oh.make_node("MatMul", ["g_mat", "g_mat_T"], ["gg"]))

    # ----- K_local = gg / (6 * abs_det). -----
    nodes.append(_const("six_const", np.array(6.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["six_const", "abs_det"], ["six_abs_det"]))
    # Reshape (N,) → (N, 1, 1) for broadcasting against (N, 4, 4).
    nodes.append(_const("reshape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["six_abs_det", "reshape_n11"],
        outputs=["six_abs_det_b"],
    ))
    nodes.append(oh.make_node("Div", ["gg", "six_abs_det_b"], ["k_local"]))

    # ----- M_local = mass_pattern * (abs_det / 120). -----
    mass_pattern = np.array(
        [
            [2.0, 1.0, 1.0, 1.0],
            [1.0, 2.0, 1.0, 1.0],
            [1.0, 1.0, 2.0, 1.0],
            [1.0, 1.0, 1.0, 2.0],
        ],
        dtype=np.float64,
    )
    nodes.append(_const("mass_pattern", mass_pattern))
    nodes.append(_const("one_twenty", np.array(120.0, dtype=np.float64)))
    nodes.append(oh.make_node("Div", ["abs_det", "one_twenty"], ["m_scale"]))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["m_scale", "reshape_n11"],
        outputs=["m_scale_b"],
    ))
    # Mass pattern needs broadcasting on a leading batch dim. Reshape
    # (4, 4) → (1, 4, 4); ONNX Mul then broadcasts (1, 4, 4) * (N, 1, 1).
    nodes.append(_const("reshape_144", np.array([1, 4, 4], dtype=np.int64)))
    nodes.append(oh.make_node(
        "Reshape",
        inputs=["mass_pattern", "reshape_144"],
        outputs=["mass_pattern_b"],
    ))
    nodes.append(oh.make_node("Mul", ["mass_pattern_b", "m_scale_b"], ["m_local"]))

    # ----- Outputs. -----
    k_vi = oh.make_tensor_value_info("k_local", TensorProto.DOUBLE, shape=["N", 4, 4])
    m_vi = oh.make_tensor_value_info("m_local", TensorProto.DOUBLE, shape=["N", 4, 4])

    graph = oh.make_graph(
        nodes,
        name="p1_local_probe",
        inputs=[elem_coords_vi],
        outputs=[k_vi, m_vi],
    )
    model = oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )
    return model


# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------


def main() -> int:
    print("== Probe: P1 local matrices (cube-cavity assembly spine, per-element step) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    model = build_p1_local_graph()

    # 1. Checker: does the model type-check?
    try:
        onnx.checker.check_model(model)
        checker_status = "OK"
    except Exception as e:  # noqa: BLE001
        checker_status = f"FAIL ({e!r})"

    # 2. Runtime: does the model execute and match NumPy?
    rt_status = "skipped"
    max_err_k = max_err_m = float("nan")
    try:
        sess = ort.InferenceSession(model.SerializeToString())
        # Canonical reference tet (unit-volume P1 reference).
        verts = np.array(
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            dtype=np.float64,
        )
        elem_coords = verts.reshape(1, 4, 3)  # (N=1, 4, 3)

        outs = sess.run(["k_local", "m_local"], {"elem_coords": elem_coords})
        k_onnx, m_onnx = outs

        k_np, m_np, _ = batched_p1_local_matrices(elem_coords)
        max_err_k = float(np.max(np.abs(k_onnx - k_np)))
        max_err_m = float(np.max(np.abs(m_onnx - m_np)))
        rt_status = "OK"
    except Exception as e:  # noqa: BLE001
        rt_status = f"FAIL ({e!r})"

    # 3. Operator inventory printout.
    print("Operator inventory for the per-element P1 spine:")
    print("--------------------------------------------------------------")
    print("  Sub                  lowers cleanly       (opset 18 native)")
    print("  Mul                  lowers cleanly       (opset 18 native)")
    print("  Add                  lowers cleanly       (opset 18 native)")
    print("  Neg                  lowers cleanly       (opset 18 native)")
    print("  Abs                  lowers cleanly       (opset 18 native)")
    print("  Div                  lowers cleanly       (opset 18 native)")
    print("  ReduceSum            lowers cleanly       (opset 18 native)")
    print("  Reshape              lowers cleanly       (opset 18 native)")
    print("  Transpose            lowers cleanly       (opset 18 native)")
    print("  Concat               lowers cleanly       (opset 18 native)")
    print("  Unsqueeze            lowers cleanly       (opset 18 native, axes-as-input form)")
    print("  Gather (axis=1)      lowers cleanly       (opset 18 native, int64 indices)")
    print("  MatMul (rank-3)      lowers cleanly       (opset 18 broadcasts batch dim;")
    print("                                             cf. TF-Java which required einsum)")
    print()
    print("  cross product        lowers WITH SYNTHESIS")
    print("                       (no native `Cross`; built as 6 Mul + 3 Sub + 3 Unsqueeze + Concat)")
    print("                       — graph-only friction: same disposition as the TF-Java")
    print("                         reference, which also hand-rolls this.")
    print()
    print("Cross-cutting frictions observed:")
    print("--------------------------------------------------------------")
    print("  - Constants must be int64 for Gather/Reshape/ReduceSum axis args;")
    print("    Python ints default to int32 if not explicit, which ONNX rejects.")
    print("    Same friction surfaces in the assembly probe — cast at the boundary.")
    print("  - Unsqueeze takes its `axes` as a tensor INPUT in opset 13+, not as")
    print("    an attribute (legacy form). Constant int64 axes nodes proliferate.")
    print("  - `Stack` is not an ONNX op: we Unsqueeze + Concat to emulate it.")
    print("    NumPy/JAX/TF-Java all have `stack`; ONNX views this as imperative")
    print("    sugar and forces the IR-level decomposition. Documentation, not a blocker.")
    print()
    print(f"onnx.checker.check_model: {checker_status}")
    print(f"onnxruntime execution: {rt_status}")
    if rt_status == "OK":
        print(f"  max |K_onnx - K_numpy| = {max_err_k:.3e}")
        print(f"  max |M_onnx - M_numpy| = {max_err_m:.3e}")
    print()
    print("Verdict: P1 local matrices lower CLEANLY (modulo a hand-rolled cross product")
    print("         and int64-axes constants), with no secretly-imperative L4 escape.")
    print("         All friction here is graph-only and identical to the TF-Java case.")

    # Exit code: zero if both checker and runtime are OK; non-zero is
    # informational (the probe is not a CI gate).
    return 0 if checker_status == "OK" and rt_status == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
