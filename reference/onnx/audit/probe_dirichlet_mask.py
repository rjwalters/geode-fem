"""Probe: ONNX expressibility of the Dirichlet-mask / interior-DOF step.

Epic #88, Phase F.1 (issue #116). After global K, M are assembled, the
cube-cavity pipeline applies homogeneous Dirichlet BC by restricting to
interior DOFs:

  - NumPy:  `idx = np.where(mask)[0]; K_int = K_csr[idx, :][:, idx]`
  - JAX:    `idx = np.where(mask)[0]; K_int = K_global[jnp.ix_(idx, idx)]`
  - TF-Java: (deferred to the host-driver sidecar — Dirichlet masking
              happens AFTER the assembly graph closes, not inside it.)

This probe asks: can ONNX express the **interior restriction step
itself** as a graph operator?

Two distinct sub-questions:

  (A) Given a precomputed `idx` array (int64, shape (n_int,)), can we
      lower `K_int = K_global[idx, :][:, idx]` to a pure ONNX graph?
      → Yes, via two `Gather` ops (one per axis). Tested below.

  (B) Given a boolean `mask` (shape (n_nodes,)), can we lower the
      `idx = np.where(mask)[0]` step to a pure ONNX graph?
      → THIS IS THE INTERESTING ONE. `np.where`/`jnp.nonzero` returns
      a tensor of *data-dependent shape* (its length depends on how
      many `True` entries are in `mask`). ONNX has had `NonZero`
      since opset 9, but it produces shapes that depend on input
      values, which is a documented graph-only friction class (data-
      dependent shapes are legal in ONNX but break many downstream
      shape inference passes — most importantly, they prevent the
      eigensolve boundary from seeing a statically-shaped K_int).

The probe demonstrates BOTH paths and prints a verdict that
distinguishes them.

Run
===

    python3 reference/onnx/audit/probe_dirichlet_mask.py
"""

from __future__ import annotations

import sys

import numpy as np
import onnx
import onnx.checker
import onnx.helper as oh
import onnxruntime as ort
from onnx import TensorProto

OPSET = 18


def _const(name: str, np_arr: np.ndarray) -> onnx.NodeProto:
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


# ---------------------------------------------------------------------------
# Path A: precomputed idx → K_int via two Gather ops
# ---------------------------------------------------------------------------


def build_gather_restrict_graph(n_nodes: int) -> onnx.ModelProto:
    """Inputs: K_global (n_nodes, n_nodes) f64, idx (n_int,) int64.
    Output: K_int (n_int, n_int) f64.
    """
    nodes: list[onnx.NodeProto] = []

    k_vi = oh.make_tensor_value_info("k_global", TensorProto.DOUBLE,
                                     shape=[n_nodes, n_nodes])
    idx_vi = oh.make_tensor_value_info("idx", TensorProto.INT64, shape=["n_int"])

    # K_global[idx, :] = Gather along axis=0.
    nodes.append(oh.make_node(
        "Gather",
        inputs=["k_global", "idx"],
        outputs=["k_rows"],
        axis=0,
    ))
    # [:, idx] = Gather along axis=1.
    nodes.append(oh.make_node(
        "Gather",
        inputs=["k_rows", "idx"],
        outputs=["k_int"],
        axis=1,
    ))

    k_int_vi = oh.make_tensor_value_info(
        "k_int", TensorProto.DOUBLE, shape=["n_int", "n_int"]
    )
    graph = oh.make_graph(
        nodes,
        name="gather_restrict_probe",
        inputs=[k_vi, idx_vi],
        outputs=[k_int_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


# ---------------------------------------------------------------------------
# Path B: mask → idx via NonZero (data-dependent shape) → K_int
# ---------------------------------------------------------------------------


def build_mask_to_kint_graph(n_nodes: int) -> onnx.ModelProto:
    """Inputs: K_global (n_nodes, n_nodes) f64, mask (n_nodes,) bool.
    Output: K_int (?, ?) f64 — data-dependent shape.
    """
    nodes: list[onnx.NodeProto] = []

    k_vi = oh.make_tensor_value_info("k_global", TensorProto.DOUBLE,
                                     shape=[n_nodes, n_nodes])
    mask_vi = oh.make_tensor_value_info("mask", TensorProto.BOOL, shape=[n_nodes])

    # NonZero(mask) returns shape (rank, n_nonzero). For a 1-D input,
    # that's (1, n_nonzero). Squeeze axis=0 to get (n_nonzero,).
    nodes.append(oh.make_node("NonZero", ["mask"], ["nonzero_2d"]))
    nodes.append(_const("axis0_const", np.array([0], dtype=np.int64)))
    nodes.append(oh.make_node(
        "Squeeze",
        inputs=["nonzero_2d", "axis0_const"],
        outputs=["idx"],
    ))

    # K_global[idx, :] then [:, idx] via two Gathers.
    nodes.append(oh.make_node("Gather", ["k_global", "idx"], ["k_rows"], axis=0))
    nodes.append(oh.make_node("Gather", ["k_rows", "idx"], ["k_int"], axis=1))

    k_int_vi = oh.make_tensor_value_info(
        "k_int", TensorProto.DOUBLE, shape=[None, None]
    )
    graph = oh.make_graph(
        nodes,
        name="mask_to_kint_probe",
        inputs=[k_vi, mask_vi],
        outputs=[k_int_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


# ---------------------------------------------------------------------------
# Verdict
# ---------------------------------------------------------------------------


def main() -> int:
    print("== Probe: Dirichlet mask / interior-DOF restriction ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Synthetic test: 5x5 K_global with a 3-node interior mask.
    n_nodes = 5
    k_global = np.arange(n_nodes * n_nodes, dtype=np.float64).reshape(n_nodes, n_nodes)
    mask_np = np.array([False, True, True, True, False])  # interior = nodes 1,2,3
    idx_np = np.where(mask_np)[0].astype(np.int64)
    k_int_ref = k_global[np.ix_(idx_np, idx_np)]

    # ------- Path A: gather with precomputed idx. -------
    print("Path A — precomputed `idx` (int64) → K_int via two Gathers")
    print("--------------------------------------------------------------")
    model_a = build_gather_restrict_graph(n_nodes)
    try:
        onnx.checker.check_model(model_a)
        a_checker = "OK"
    except Exception as e:  # noqa: BLE001
        a_checker = f"FAIL ({e!r})"
    a_rt = "skipped"
    a_err = float("nan")
    try:
        sess_a = ort.InferenceSession(model_a.SerializeToString())
        out_a = sess_a.run(["k_int"], {"k_global": k_global, "idx": idx_np})
        a_err = float(np.max(np.abs(out_a[0] - k_int_ref)))
        a_rt = "OK"
    except Exception as e:  # noqa: BLE001
        a_rt = f"FAIL ({e!r})"
    print(f"  Gather(axis=0)         lowers cleanly       (opset 18 native)")
    print(f"  Gather(axis=1)         lowers cleanly       (opset 18 native)")
    print(f"  onnx.checker:          {a_checker}")
    print(f"  onnxruntime execution: {a_rt}")
    if a_rt == "OK":
        print(f"  max |K_int_onnx - K_int_numpy| = {a_err:.3e}")
    print()

    # ------- Path B: mask → idx via NonZero. -------
    print("Path B — boolean `mask` → idx via NonZero (data-dependent shape)")
    print("--------------------------------------------------------------")
    model_b = build_mask_to_kint_graph(n_nodes)
    try:
        onnx.checker.check_model(model_b)
        b_checker = "OK"
    except Exception as e:  # noqa: BLE001
        b_checker = f"FAIL ({e!r})"
    b_rt = "skipped"
    b_err = float("nan")
    try:
        sess_b = ort.InferenceSession(model_b.SerializeToString())
        out_b = sess_b.run(["k_int"], {"k_global": k_global, "mask": mask_np})
        b_err = float(np.max(np.abs(out_b[0] - k_int_ref)))
        b_rt = "OK"
    except Exception as e:  # noqa: BLE001
        b_rt = f"FAIL ({e!r})"
    print(f"  NonZero                lowers cleanly       (opset 18 native)")
    print(f"                         — BUT returns a data-dependent shape.")
    print(f"                           See the friction note below.")
    print(f"  Squeeze, Gather        lowers cleanly       (opset 18 native)")
    print(f"  onnx.checker:          {b_checker}")
    print(f"  onnxruntime execution: {b_rt}")
    if b_rt == "OK":
        print(f"  max |K_int_onnx - K_int_numpy| = {b_err:.3e}")
    print()

    print("Cross-cutting frictions observed:")
    print("--------------------------------------------------------------")
    print("  - `idx = np.where(mask)[0]` is the canonical 'secretly")
    print("    imperative' L4 friction. In Python/JAX/NumPy it returns")
    print("    a data-dependent-shape int tensor whose runtime size")
    print("    cannot be inferred statically. ONNX `NonZero` lowers it")
    print("    to the graph, but at the cost of breaking static shape")
    print("    inference for everything downstream — K_int is then")
    print("    typed `[None, None]` from ONNX's point of view, which")
    print("    blocks compile-time tensor-shape validation.")
    print()
    print("    Classification: 'lowers with caveat'. ONNX has the")
    print("    operator (NonZero), but it propagates dynamic shapes")
    print("    through the rest of the graph. This is GRAPH-ONLY")
    print("    friction, not a secretly-imperative escape — the L4")
    print("    operator IS expressible, but the static-graph optimizer")
    print("    loses its rank/shape grip downstream.")
    print()
    print("  - Practical lowering recommendation for Phase F.2: compute")
    print("    `idx` on the host BEFORE entering the graph (mirroring")
    print("    TF-Java, where the Dirichlet step happens in the JVM")
    print("    sidecar). This keeps the assembly graph statically-")
    print("    shaped end-to-end, and matches the eigensolve-at-the-")
    print("    boundary convention adopted across the reference set.")
    print()
    print("  - JAX has the same issue with `jnp.nonzero` (returns")
    print("    dynamic-shape arrays that defeat XLA static tracing),")
    print("    and JAX's `cube_cavity.py` ALSO computes `idx` outside")
    print("    the jit boundary. So this is shared L4 friction across")
    print("    XLA-shaped IRs — not ONNX-specific.")
    print()
    print("  - The Burn analog (geode_core::dirichlet) computes the")
    print("    mask imperatively on the host. The 'graph-only' phrasing")
    print("    here is exposing that this is, in fact, an imperative")
    print("    construction in every backend — only the ONNX path")
    print("    forces us to name it explicitly.")
    print()
    print("Verdict: gathering with a precomputed `idx` (Path A) lowers CLEANLY.")
    print("         The mask→idx step itself (Path B) lowers via NonZero, but")
    print("         introduces a data-dependent shape that breaks downstream")
    print("         static-shape inference. Recommended Phase F.2 disposition:")
    print("         host-side `idx` construction (same as JAX, same as TF-Java);")
    print("         the ONNX assembly graph accepts `idx` as a graph input.")

    return 0 if a_checker == "OK" and a_rt == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
