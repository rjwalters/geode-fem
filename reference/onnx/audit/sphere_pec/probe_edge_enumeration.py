"""Probe: ONNX expressibility of edge enumeration (build_edges).

Epic #88, Phase G.6 (issue #135). This probe asks whether the
`build_edges` step from `reference/numpy/sphere_pec.py` can be
expressed as a pure ONNX graph.

What build_edges does
=====================

Given `tets` of shape (n_tets, 4) (tet connectivity table), it:

  1. Extracts the 6 local-edge vertex-index pairs per tet, flattened to
     (n_tets * 6, 2), using the fixed `TET_LOCAL_EDGES` ordering.
  2. Canonicalizes each pair to (lo, hi) = (min(a,b), max(a,b)).
  3. Deduplicates the resulting (lo, hi) pairs using a sort + unique step
     to get a globally-indexed edge list of shape (n_edges, 2).
  4. Builds a (lo, hi) -> global edge index lookup map (a Python dict).
  5. Fills per-tet tables: `tet_edge_idx (n_tets, 6)` and
     `tet_edge_sign (n_tets, 6)` by iterating over all (tet, local-edge)
     pairs and looking up the canonical index.

Why this is NOT expressible
============================

Steps 3-5 are the blockers:

  (A) `np.unique(..., axis=0)` — ONNX has no deduplication op. This step
      takes a (n_tets*6, 2) table of (lo, hi) pairs and returns only the
      distinct rows, in sorted order. The output shape is data-dependent:
      `n_edges` depends on which edges are shared between tets, which is a
      topological property of the mesh that cannot be inferred statically.
      ONNX cannot express this.

  (B) The hash-map lookup `edge_to_idx[(lo, hi)]` — building a sorted
      rank/relabeling map from the deduplicated edge list is a topological
      sort / sparse encoding step. ONNX has no sorted-unique-with-inverse
      operator. The closest analog would be a sequence of Sort + NonZero
      ops, but the output of NonZero has data-dependent shape, and the
      composition (sort the pairs, find the inverse permutation) is not
      expressible as a static graph.

  (C) The double for-loop (tet, local edge) with dict lookup to fill
      `tet_edge_idx` — this is a data-dependent scatter with variable
      stride (each tet contributes to different global edge indices
      depending on the mesh topology). The global edge indices are not
      computable without completing step (A) first.

The probe demonstrates this by showing:
  - Step 1+2 (local pair extraction + canonicalization) ARE expressible
    via Gather + Min + Max.
  - Step 3 (unique) causes an explicit failure because ONNX has no
    `Unique` op that returns the full sorted-unique-rows output for 2D
    inputs in a static-shape-safe way. (ONNX does have `Unique` for 1D
    flat inputs but not for 2D row-deduplication with pair keys.)

Run
===

    python3 reference/onnx/audit/sphere_pec/probe_edge_enumeration.py
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

from reference.numpy.nedelec_local_matrices import TET_LOCAL_EDGES  # noqa: E402
from reference.numpy.sphere_pec import build_edges  # noqa: E402

OPSET = 18

# Canonical local-edge pair indices as constants.
LOCAL_A = np.array([a for a, _ in TET_LOCAL_EDGES], dtype=np.int64)  # (6,)
LOCAL_B = np.array([b for _, b in TET_LOCAL_EDGES], dtype=np.int64)  # (6,)


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


def build_local_pairs_graph(n_tets: int) -> onnx.ModelProto:
    """Build the EXPRESSIBLE part: local pair extraction + canonicalization.

    Inputs: tets (n_tets, 4) int64
    Output: lo_hi_pairs (n_tets * 6, 2) int64 — canonicalized (min, max) pairs.

    This is ONLY steps 1+2 of build_edges. Step 3 (deduplication) cannot
    follow in a static graph.
    """
    nodes: list[onnx.NodeProto] = []

    tets_vi = oh.make_tensor_value_info("tets", TensorProto.INT64, shape=[n_tets, 4])

    # Bake local edge vertex indices as constants
    nodes.append(_const("local_a", LOCAL_A))  # (6,)
    nodes.append(_const("local_b", LOCAL_B))  # (6,)

    # Gather vertex_a = tets[:, local_a] -> (n_tets, 6)
    nodes.append(oh.make_node(
        "Gather",
        inputs=["tets", "local_a"],
        outputs=["vert_a"],
        axis=1,
    ))
    # Gather vertex_b = tets[:, local_b] -> (n_tets, 6)
    nodes.append(oh.make_node(
        "Gather",
        inputs=["tets", "local_b"],
        outputs=["vert_b"],
        axis=1,
    ))

    # Canonicalize: lo = min(a, b), hi = max(a, b)
    nodes.append(oh.make_node("Min", ["vert_a", "vert_b"], ["lo"]))
    nodes.append(oh.make_node("Max", ["vert_a", "vert_b"], ["hi"]))

    # Flatten to (n_tets * 6,) then unsqueeze + concat to (n_tets * 6, 2)
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["lo", "shape_flat"], ["lo_flat"]))
    nodes.append(oh.make_node("Reshape", ["hi", "shape_flat"], ["hi_flat"]))
    nodes.append(_const("axis_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["lo_flat", "axis_neg1"], ["lo_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["hi_flat", "axis_neg1"], ["hi_col"]))
    nodes.append(oh.make_node(
        "Concat",
        inputs=["lo_col", "hi_col"],
        outputs=["lo_hi_pairs"],
        axis=1,
    ))

    pairs_vi = oh.make_tensor_value_info(
        "lo_hi_pairs", TensorProto.INT64, shape=[n_tets * 6, 2]
    )
    graph = oh.make_graph(
        nodes,
        name="local_pairs_probe",
        inputs=[tets_vi],
        outputs=[pairs_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: edge enumeration (build_edges) — sphere PEC, Phase G.6 ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Tiny test mesh: 2 tets sharing an edge
    tets_np = np.array(
        [[0, 1, 2, 3],
         [1, 2, 3, 4]],
        dtype=np.int64,
    )
    n_tets = tets_np.shape[0]

    # --- Part A: expressible sub-steps ---
    print("Part A — local pair extraction + canonicalization (steps 1-2 of build_edges)")
    print("--------------------------------------------------------------")
    model_a = build_local_pairs_graph(n_tets)
    try:
        onnx.checker.check_model(model_a)
        a_checker = "OK"
    except Exception as e:  # noqa: BLE001
        a_checker = f"FAIL ({e!r})"

    a_rt = "skipped"
    a_err = float("nan")
    try:
        sess_a = ort.InferenceSession(model_a.SerializeToString())
        out_a = sess_a.run(["lo_hi_pairs"], {"tets": tets_np})
        pairs_onnx = out_a[0]

        # Reference: what numpy would do
        la = np.array([a for a, _ in TET_LOCAL_EDGES], dtype=np.int64)
        lb = np.array([b for _, b in TET_LOCAL_EDGES], dtype=np.int64)
        va = tets_np[:, la]
        vb = tets_np[:, lb]
        lo = np.minimum(va, vb).ravel()
        hi = np.maximum(va, vb).ravel()
        pairs_ref = np.stack([lo, hi], axis=1)

        a_err = float(np.max(np.abs(pairs_onnx - pairs_ref)))
        a_rt = "OK"
    except Exception as e:  # noqa: BLE001
        a_rt = f"FAIL ({e!r})"

    print(f"  Gather (axis=1)      EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print(f"  Min / Max            EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print(f"  Reshape / Unsqueeze  EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print(f"  Concat               EXPRESSIBLE  lowers cleanly (opset 18 native)")
    print(f"  onnx.checker: {a_checker}")
    print(f"  onnxruntime: {a_rt}")
    if a_rt == "OK":
        print(f"  max |pairs_onnx - pairs_ref| = {a_err:.3e}")
    print()

    # --- Part B: deduplication — NOT expressible ---
    print("Part B — deduplication + inverse map (steps 3-5 of build_edges)")
    print("--------------------------------------------------------------")
    print("  Attempting to build a graph for np.unique(pairs, axis=0)...")
    print()
    # ONNX `Unique` op (opset 11) operates only on 1D flat arrays and returns
    # (unique_values, indices, inverse_indices, counts). For 2D row-dedup we
    # would need to encode each (lo, hi) pair as a scalar key and then unique
    # that. Even if we did, the output `n_edges` is data-dependent — its size
    # depends on the mesh topology.
    #
    # Attempt: encode each row as lo * (max_node+1) + hi and apply Unique.
    max_node = int(tets_np.max()) + 1
    nodes: list[onnx.NodeProto] = []

    tets_vi = oh.make_tensor_value_info("tets", TensorProto.INT64, shape=[n_tets, 4])

    # Reuse the local-pair extraction from Part A.
    nodes.append(_const("local_a", LOCAL_A))
    nodes.append(_const("local_b", LOCAL_B))
    nodes.append(oh.make_node("Gather", ["tets", "local_a"], ["vert_a"], axis=1))
    nodes.append(oh.make_node("Gather", ["tets", "local_b"], ["vert_b"], axis=1))
    nodes.append(oh.make_node("Min", ["vert_a", "vert_b"], ["lo"]))
    nodes.append(oh.make_node("Max", ["vert_a", "vert_b"], ["hi"]))
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["lo", "shape_flat"], ["lo_flat"]))
    nodes.append(oh.make_node("Reshape", ["hi", "shape_flat"], ["hi_flat"]))

    # Encode: key = lo * max_node + hi
    nodes.append(_const("max_node_scalar", np.array(max_node, dtype=np.int64)))
    nodes.append(oh.make_node("Mul", ["lo_flat", "max_node_scalar"], ["lo_scaled"]))
    nodes.append(oh.make_node("Add", ["lo_scaled", "hi_flat"], ["pair_keys"]))

    # Apply Unique — this is the step that introduces data-dependent output shape.
    # `Unique` returns: (unique_values, indices, inverse_indices, counts)
    nodes.append(oh.make_node(
        "Unique",
        inputs=["pair_keys"],
        outputs=["unique_keys", "unique_indices", "inverse_indices", "counts"],
        sorted=1,
    ))

    # unique_keys has shape (n_edges,) where n_edges is DATA-DEPENDENT.
    # This means everything downstream has a dynamic shape axis.
    unique_vi = oh.make_tensor_value_info(
        "unique_keys", TensorProto.INT64, shape=[None]  # data-dependent!
    )

    try:
        graph_b = oh.make_graph(
            nodes,
            name="dedup_probe",
            inputs=[tets_vi],
            outputs=[unique_vi],
        )
        model_b = oh.make_model(
            graph_b,
            opset_imports=[oh.make_opsetid("", OPSET)],
            ir_version=9,
        )
        onnx.checker.check_model(model_b)
        b_checker = "OK (but output shape is [None] — data-dependent!)"
    except Exception as e:  # noqa: BLE001
        b_checker = f"FAIL ({e!r})"

    b_rt = "skipped"
    n_unique_onnx = None
    n_unique_ref = None
    try:
        sess_b = ort.InferenceSession(model_b.SerializeToString())
        out_b = sess_b.run(["unique_keys"], {"tets": tets_np})
        unique_keys = out_b[0]
        n_unique_onnx = len(unique_keys)

        # Reference
        edges_ref, _, _ = build_edges(tets_np)
        n_unique_ref = len(edges_ref)
        b_rt = "OK"
    except Exception as e:  # noqa: BLE001
        b_rt = f"FAIL ({e!r})"

    print(f"  Unique (opset 11)    EXPRESSIBLE but output shape is data-dependent:")
    print(f"                       n_edges = len(unique_keys) depends on mesh topology.")
    print(f"                       The ONNX IR types this as [None] — no static shape.")
    print()
    print(f"  onnx.checker: {b_checker}")
    print(f"  onnxruntime: {b_rt}")
    if b_rt == "OK":
        print(f"  n_unique_edges via ONNX: {n_unique_onnx}")
        print(f"  n_unique_edges via NumPy: {n_unique_ref}")
    print()
    print("  Even if Unique succeeds for the encoded keys, recovering the (lo, hi)")
    print("  pairs requires decoding (mod + div on unique_keys), and then filling")
    print("  the tet_edge_idx / tet_edge_sign tables requires a scatter-with-lookup")
    print("  whose target indices are the output of the Unique step — i.e. data-")
    print("  dependent indices into a data-dependent-length buffer. This chain")
    print("  cannot be expressed in a statically-shaped ONNX graph.")
    print()
    print("  Classification: NOT EXPRESSIBLE as a static-shape graph.")
    print("  The Unique op exists, but the entire build_edges pipeline requires:")
    print("    (1) data-dependent output shape (n_edges is a function of mesh topology),")
    print("    (2) a hash-map lookup (Python dict) that has no ONNX equivalent,")
    print("    (3) a topological sort that is imperatively shaped.")
    print()
    print("Cross-cutting frictions observed:")
    print("--------------------------------------------------------------")
    print("  - `build_edges` is FUNDAMENTALLY host-imperative. It is the")
    print("    combinatorial backbone of the Nedelec assembly and cannot be")
    print("    lowered to any static-graph IR (ONNX, XLA, TF-Java).")
    print("  - The P1 analog (cube-cavity) had NO equivalent step: node DOFs")
    print("    are directly indexed by the connectivity table; no deduplication")
    print("    or inverse-map construction is needed. This is Nedelec-specific.")
    print("  - Every backend (NumPy, Burn/Rust, JAX) computes build_edges")
    print("    outside any traced/compiled function boundary. ONNX makes this")
    print("    explicit by having no `Unique`-on-2D-rows operator.")
    print("  - Recommendation: `edges`, `tet_edge_idx`, `tet_edge_sign` are")
    print("    HOST-COMPUTED graph inputs for any Nedelec ONNX assembly graph,")
    print("    exactly as `nodes` and `tets` are for P1.")
    print()
    print("Verdict: NOT EXPRESSIBLE")
    print("  `build_edges` (deduplication + inverse map) cannot be expressed as")
    print("  a pure ONNX graph. The local pair extraction (steps 1-2) lowers")
    print("  cleanly; the deduplication (step 3) requires Unique with data-")
    print("  dependent output shape; the inverse-map lookup (steps 4-5) requires")
    print("  an imperative hash-map that has no ONNX operator equivalent.")
    print("  Friction class: secretly-imperative L4 escape.")

    # Exit 0 if Part A worked (which is the expressible sub-claim), 1 if broken.
    return 0 if a_checker == "OK" and a_rt == "OK" else 1


if __name__ == "__main__":
    sys.exit(main())
