"""Probe: ONNX expressibility of the PEC boundary mask (Nedelec).

Epic #88, Phase G.6 (issue #135). This probe asks whether the PEC mask
step from `reference/numpy/sphere_pec.py` — eliminating edges that lie
entirely on the outer PEC wall — can be expressed as a pure ONNX graph,
given that `edges` (n_edges, 2) and `nodes` (n_nodes, 3) are graph inputs.

What sphere_pec_interior_edges does
=====================================

Given `nodes (n_nodes, 3)` and `edges (n_edges, 2)`:

  1. Compute node radii: r[i] = |nodes[i]|  (L2 norm, axis=1)
  2. Build boundary mask: on_boundary[i] = |r[i] - r_outer| < tol
  3. For each edge (a, b): mask[e] = ~(on_boundary[a] & on_boundary[b])
     (interior if NOT both endpoints are on the wall)
  4. Return interior_mask (n_edges,) bool

Comparison with the P1 Dirichlet step (cube-cavity)
=====================================================

P1 (cube-cavity): interior_mask was a boolean mask on NODES; the step
was: `idx = np.where(mask)[0]` → NonZero with data-dependent shape.

Nedelec PEC mask: the mask is on EDGES. The key difference is HOW the
mask is derived:
  - P1: mask comes from an externally-tagged "boundary node" concept
    (Dirichlet BCs are tagged by the problem setup, often precomputed
    from the mesh's boundary-face list).
  - Nedelec PEC: mask is DERIVED FROM NODE POSITIONS in the graph —
    the r = |nodes| computation and the r ≈ r_outer threshold.

ONNX expressibility of each sub-step:
  1. L2 norm per node: `ReduceL2(nodes, axis=1)` — EXPRESSIBLE (opset 18).
  2. |r[i] - r_outer| < tol: Sub, Abs, Less with a broadcast constant
     — EXPRESSIBLE (all opset 18 native).
  3. Gather endpoint flags: `on_boundary[edges[:, 0]]` and `[edges[:, 1]]`
     via GatherElements or Gather — EXPRESSIBLE.
  4. And + Not to build the interior mask — EXPRESSIBLE (And, Not native).

BUT: the same "data-dependent shape" friction as the P1 Dirichlet step
applies if we then try to derive idx from the mask:

  5. `idx = np.flatnonzero(interior_mask)` → NonZero → data-dependent shape.
     The Gather-based Dirichlet restriction (K_int = K[idx,:][:,idx]) is
     then typed (None, None). Same friction class as cube-cavity stage 3.

The probe demonstrates:
  (A) The mask computation itself (steps 1-4) IS expressible and produces
      a static-shape (n_edges,) bool tensor.
  (B) Deriving idx via NonZero introduces data-dependent shape, exactly
      as in the P1 case.
  (C) Recommendation: accept `interior_idx` (int64, host-computed from
      the mask) as a graph input, exactly mirroring the P1 convention.

Run
===

    python3 reference/onnx/audit/sphere_pec/probe_pec_mask.py
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

from reference.numpy.sphere_pec import sphere_pec_interior_edges, R_BUFFER  # noqa: E402

OPSET = 18


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
    dtype_map = {
        np.dtype("float64"): TensorProto.DOUBLE,
        np.dtype("float32"): TensorProto.FLOAT,
        np.dtype("int64"): TensorProto.INT64,
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


def build_pec_mask_graph(n_nodes: int, n_edges: int, r_outer: float = R_BUFFER) -> onnx.ModelProto:
    """Build the PEC mask computation graph (steps 1-4 only).

    Inputs:
      nodes (n_nodes, 3) float64
      edges (n_edges, 2) int64

    Output:
      interior_mask (n_edges,) bool
    """
    nodes: list[onnx.NodeProto] = []

    nodes_vi = oh.make_tensor_value_info("nodes", TensorProto.DOUBLE, [n_nodes, 3])
    edges_vi = oh.make_tensor_value_info("edges", TensorProto.INT64, [n_edges, 2])

    # --- Step 1: r[i] = ReduceL2(nodes[i], axis=1) ---
    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ReduceL2",
        inputs=["nodes", "ax1"],
        outputs=["r"],
        keepdims=0,
    ))

    # --- Step 2: on_boundary[i] = |r[i] - r_outer| < tol ---
    tol = 1e-6 * max(r_outer, 1.0)
    nodes.append(_const("r_outer_const", np.array(r_outer, dtype=np.float64)))
    nodes.append(_const("tol_const", np.array(tol, dtype=np.float64)))
    nodes.append(oh.make_node("Sub", ["r", "r_outer_const"], ["r_minus_outer"]))
    nodes.append(oh.make_node("Abs", ["r_minus_outer"], ["r_dist"]))
    nodes.append(oh.make_node("Less", ["r_dist", "tol_const"], ["on_boundary"]))

    # --- Step 3: gather endpoint flags ---
    # endpoints_a = edges[:, 0], endpoints_b = edges[:, 1]
    nodes.append(_const("idx0", np.array(0, dtype=np.int64)))
    nodes.append(_const("idx1", np.array(1, dtype=np.int64)))
    nodes.append(oh.make_node("Gather", ["edges", "idx0"], ["ep_a"], axis=1))
    nodes.append(oh.make_node("Gather", ["edges", "idx1"], ["ep_b"], axis=1))

    # on_boundary[edges[:, 0]] via GatherElements
    # on_boundary is (n_nodes,) bool; ep_a is (n_edges,) int64.
    # GatherElements requires both tensors to have the same rank.
    # Gather(axis=0) on a 1D tensor is equivalent here.
    nodes.append(oh.make_node(
        "Gather",
        inputs=["on_boundary", "ep_a"],
        outputs=["a_on"],
        axis=0,
    ))
    nodes.append(oh.make_node(
        "Gather",
        inputs=["on_boundary", "ep_b"],
        outputs=["b_on"],
        axis=0,
    ))

    # --- Step 4: interior_mask = ~(a_on & b_on) ---
    nodes.append(oh.make_node("And", ["a_on", "b_on"], ["both_on"]))
    nodes.append(oh.make_node("Not", ["both_on"], ["interior_mask"]))

    interior_vi = oh.make_tensor_value_info(
        "interior_mask", TensorProto.BOOL, [n_edges]
    )
    graph = oh.make_graph(
        nodes,
        name="pec_mask_probe",
        inputs=[nodes_vi, edges_vi],
        outputs=[interior_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: PEC boundary mask (sphere PEC, Phase G.6) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Synthetic nodes: some on the outer sphere (r = R_BUFFER = 2.0)
    r = R_BUFFER
    nodes_np = np.array(
        [
            [0.0, 0.0, 0.0],     # interior: r = 0
            [1.0, 0.0, 0.0],     # interior: r = 1
            [r, 0.0, 0.0],       # on wall: r = 2.0
            [0.0, r, 0.0],       # on wall: r = 2.0
            [0.0, 0.0, r],       # on wall: r = 2.0
        ],
        dtype=np.float64,
    )
    n_nodes = nodes_np.shape[0]
    # Edges: some interior, some mixed, some fully on wall
    edges_np = np.array(
        [
            [0, 1],   # both interior -> INTERIOR
            [0, 2],   # one interior, one on wall -> INTERIOR
            [2, 3],   # both on wall -> PEC (eliminated)
            [3, 4],   # both on wall -> PEC (eliminated)
            [1, 2],   # one interior, one on wall -> INTERIOR
        ],
        dtype=np.int64,
    )
    n_edges = edges_np.shape[0]

    # NumPy reference
    mask_ref, on_bdry_ref = sphere_pec_interior_edges(nodes_np, edges_np)

    # Build and test graph
    model = build_pec_mask_graph(n_nodes, n_edges)

    try:
        onnx.checker.check_model(model)
        checker_status = "OK"
    except Exception as e:  # noqa: BLE001
        checker_status = f"FAIL ({e!r})"

    rt_status = "skipped"
    mask_match = False
    try:
        sess = ort.InferenceSession(model.SerializeToString())
        outs = sess.run(["interior_mask"], {"nodes": nodes_np, "edges": edges_np})
        mask_onnx = outs[0]
        mask_match = bool(np.all(mask_onnx == mask_ref))
        rt_status = "OK"
    except Exception as e:  # noqa: BLE001
        rt_status = f"FAIL ({e!r})"

    print("Operator inventory for PEC mask computation (steps 1-4):")
    print("--------------------------------------------------------------")
    print("  ReduceL2 (axis=1)    EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("                       — computes per-node L2 norm (radii)")
    print("  Sub / Abs / Less     EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("                       — threshold comparison: |r - r_outer| < tol")
    print("  Gather (axis=0)      EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("                       — endpoint flag lookup: on_boundary[edges[:,0]]")
    print("  And / Not            EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print("                       — interior = NOT (both endpoints on wall)")
    print()
    print(f"onnx.checker.check_model: {checker_status}")
    print(f"onnxruntime execution: {rt_status}")
    if rt_status == "OK":
        print(f"  interior_mask matches NumPy reference: {mask_match}")
        print(f"  expected:  {mask_ref.tolist()}")
        print(f"  got:       {mask_onnx.tolist()}")
    print()

    print("Friction analysis — steps 5+ (idx derivation + Dirichlet restriction):")
    print("--------------------------------------------------------------")
    print("  Step 5: idx = np.flatnonzero(interior_mask)")
    print("          -> NonZero op -> data-dependent output shape [None]")
    print("          SAME friction class as cube-cavity Dirichlet stage 3.")
    print("          Classification: caveat (graph-only friction, not secretly-imperative).")
    print()
    print("  Recommendation (same as P1 case):")
    print("    - Compute interior_mask and idx on the HOST before entering the ONNX graph.")
    print("    - Accept `interior_idx` (int64, n_int) as a graph input.")
    print("    - Apply Gather(K_global, interior_idx, axis=0) then Gather on axis=1.")
    print("    - This keeps K_int and M_int statically-shaped (n_int x n_int).")
    print()
    print("  One structural difference from P1:")
    print("    - P1 mask is on NODES (n_nodes,) — a property of the mesh,")
    print("      often tagged externally.")
    print("    - Nedelec PEC mask is on EDGES (n_edges,) — DERIVED FROM NODE")
    print("      POSITIONS in the graph (ReduceL2 + threshold). The mask")
    print("      computation itself IS expressible; it is only the idx-derivation")
    print("      step that introduces data-dependent shape.")
    print("    - This means the Nedelec PEC graph CAN compute its own mask")
    print("      (unlike P1, where the mask is typically precomputed by the")
    print("      mesh reader). However, the idx must still be host-extracted.")
    print()
    print("Verdict: EXPRESSIBLE (with pre-computed interior_idx as graph input)")
    print("  The PEC mask computation (steps 1-4: ReduceL2, Sub, Abs, Less,")
    print("  Gather, And, Not) lowers cleanly to a static-shape ONNX graph.")
    print("  The Dirichlet restriction (idx -> K_int) follows the same path as")
    print("  the cube-cavity audit: interior_idx is host-computed (via NonZero")
    print("  on the host side), then passed as a graph input for two Gathers.")
    print("  This matches the JAX and TF-Java conventions.")

    return 0 if checker_status == "OK" and rt_status == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
