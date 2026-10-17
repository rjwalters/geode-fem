"""Probe: ONNX expressibility of d¹ *application* (discrete curl matvec).

Epic #88, Phase I.3 (issue #169). Companion to ``probe_d0_apply.py``:
asks whether **applying** the discrete curl ``d¹`` to an edge field —
``c = d¹ · v`` — is graph-pure under opset 18, given that the
**construction** of ``d¹`` (face dedup via ``np.unique(..., axis=0)``
plus the ``edge_to_idx`` dict lookup in
``reference/numpy/derham.py::curl_map``) is host-side, by the same
friction class as the G.6 ``build_edges`` finding.

The construction/application split
==================================

``d¹`` is an ``(n_faces, n_edges)`` signed ``{-1, +1}`` incidence
matrix with exactly three nonzeros per row (face ``[a, b, c]``,
``a < b < c``, boundary cycle ``a → b → c → a``):

    d¹[face, edge(a, b)] = +1
    d¹[face, edge(b, c)] = +1
    d¹[face, edge(a, c)] = -1

Constructing the column indices needs ``build_faces`` (row dedup —
data-dependent shape) and the ``edge_to_idx`` hash map (no ONNX
equivalent). Both are host-side. But once the host hands the graph a
``face_edge_idx (n_faces, 3) int64`` table (columns ordered
``[edge(a,b), edge(b,c), edge(a,c)]``) — or the raw COO triplets —
application is three Gathers and two adds:

    c = v[face_edge_idx[:, 0]] + v[face_edge_idx[:, 1]]
        - v[face_edge_idx[:, 2]]

Strategy: three sub-probes
==========================

(A) **Structured incidence form** — three scalar-index Gathers on the
    host-provided ``face_edge_idx`` table, Add + Sub. int64 + f64.

(B) **Generic COO sparse matvec** — same builder shape as the d⁰
    probe: ``ScatterND_add(zeros(n_faces), rows, data · v[cols])``
    consuming the COO triplets of ``curl_map`` as graph inputs.

(C) **d² via the same generic builder** — the COO matvec is operator-
    agnostic, so the divergence ``d² · w`` (4 nnz per row, signs
    ``(-1)^k · sign_k`` from host-side ``build_tet_faces``) runs
    through the *identical* graph builder with different inputs. This
    extends the application verdict to the full d-chain without a
    third probe file.

Run
===

    python3 reference/onnx/audit/derham/probe_d1_apply.py
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

from reference.numpy.derham import (  # noqa: E402
    build_edges,
    build_faces,
    curl_map,
    divergence_map,
)

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


def host_face_edge_idx(edges: np.ndarray, faces: np.ndarray) -> np.ndarray:
    """HOST-SIDE: per-face edge-index table, columns [e_ab, e_bc, e_ac].

    This is the dict-lookup step from ``curl_map`` — the part of d¹
    construction that has no ONNX lowering (hash-map inverse of the
    deduplicated edge table). It runs in NumPy/Python and its output
    crosses the L4-input boundary as a plain int64 tensor.
    """
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


def build_structured_d1_graph(
    n_edges: int, n_faces: int, elem_type: int
) -> onnx.ModelProto:
    """Graph (A) — structured incidence form of ``c = d¹ · v``.

    Inputs:  v (n_edges,) elem_type, face_edge_idx (n_faces, 3) int64
    Output:  c (n_faces,) elem_type — per-face ``v[e_ab] + v[e_bc] - v[e_ac]``.
    """
    nodes: list[onnx.NodeProto] = []

    v_vi = oh.make_tensor_value_info("v", elem_type, [n_edges])
    fei_vi = oh.make_tensor_value_info(
        "face_edge_idx", TensorProto.INT64, [n_faces, 3]
    )

    for k, name in [(0, "idx_ab"), (1, "idx_bc"), (2, "idx_ac")]:
        nodes.append(_const(f"col{k}", np.array(k, dtype=np.int64)))
        nodes.append(oh.make_node(
            "Gather", ["face_edge_idx", f"col{k}"], [name], axis=1
        ))
    nodes.append(oh.make_node("Gather", ["v", "idx_ab"], ["v_ab"], axis=0))
    nodes.append(oh.make_node("Gather", ["v", "idx_bc"], ["v_bc"], axis=0))
    nodes.append(oh.make_node("Gather", ["v", "idx_ac"], ["v_ac"], axis=0))

    nodes.append(oh.make_node("Add", ["v_ab", "v_bc"], ["v_sum"]))
    nodes.append(oh.make_node("Sub", ["v_sum", "v_ac"], ["c"]))

    c_vi = oh.make_tensor_value_info("c", elem_type, [n_faces])
    graph = oh.make_graph(
        nodes,
        name="d1_apply_structured",
        inputs=[v_vi, fei_vi],
        outputs=[c_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def build_coo_matvec_graph(
    n_rows: int, n_cols: int, nnz: int, elem_type: int
) -> onnx.ModelProto:
    """Graph (B)/(C) — generic COO sparse matvec ``y = A · x``.

    Identical builder to ``probe_d0_apply.py``; duplicated so each
    probe stays self-contained (existing audit convention).
    """
    nodes: list[onnx.NodeProto] = []

    x_vi = oh.make_tensor_value_info("x", elem_type, [n_cols])
    rows_vi = oh.make_tensor_value_info("rows", TensorProto.INT64, [nnz])
    cols_vi = oh.make_tensor_value_info("cols", TensorProto.INT64, [nnz])
    vals_vi = oh.make_tensor_value_info("vals", elem_type, [nnz])

    nodes.append(oh.make_node("Gather", ["x", "cols"], ["x_at_cols"], axis=0))
    nodes.append(oh.make_node("Mul", ["vals", "x_at_cols"], ["updates"]))

    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows", "ax_neg1"], ["indices"]))

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


def _run_coo_cases(title, sp_mat, x_i64, x_f64):
    """Run the generic COO matvec graph (int64 + f64) for a scipy CSR."""
    coo = sp_mat.tocoo()
    rows = coo.row.astype(np.int64)
    cols = coo.col.astype(np.int64)
    n_rows, n_cols = sp_mat.shape
    nnz = int(sp_mat.nnz)
    ok = True
    print(title)
    for label, etype, x, vals, y_ref in [
        ("int64", TensorProto.INT64, x_i64,
         coo.data.astype(np.int64), np.asarray(sp_mat @ x_i64, dtype=np.int64)),
        ("float64", TensorProto.DOUBLE, x_f64,
         coo.data.astype(np.float64),
         np.asarray(sp_mat.astype(np.float64) @ x_f64)),
    ]:
        try:
            model = build_coo_matvec_graph(n_rows, n_cols, nnz, etype)
            (y_onnx,) = _check_and_run(
                model, ["y"], {"x": x, "rows": rows, "cols": cols, "vals": vals}
            )
            err = float(np.max(np.abs(y_onnx - y_ref)))
            status = "OK"
        except Exception as e:  # noqa: BLE001
            status, err = f"FAIL ({e!r})", float("nan")
            ok = False
        print(f"  [{label:7s}] checker+runtime: {status}   "
              f"max |y_onnx - y_ref| = {err:.3e}" if status == "OK"
              else f"  [{label:7s}] {status}")
        if status == "OK" and err != 0:
            ok = False
    print()
    return ok


def main() -> int:
    print("== Probe: d¹ application (discrete curl matvec) — Phase I.3 ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Tiny test mesh: 2 tets sharing a face — same as the G.6/H.5 probes.
    tets_np = np.array(
        [[0, 1, 2, 3],
         [1, 2, 3, 4]],
        dtype=np.int64,
    )

    # HOST-SIDE construction: edge/face dedup + dict inverse maps.
    edges = build_edges(tets_np)
    faces = build_faces(tets_np)
    n_edges = int(edges.shape[0])
    n_faces = int(faces.shape[0])
    d1 = curl_map(edges, faces)            # (n_faces, n_edges), int64
    d2 = divergence_map(tets_np, faces)    # (n_tets, n_faces), int64
    face_edge_idx = host_face_edge_idx(edges, faces)
    print(f"Test mesh: n_tets={tets_np.shape[0]}, n_edges={n_edges}, "
          f"n_faces={n_faces}, d1 nnz={d1.nnz}, d2 nnz={d2.nnz}")
    print()

    rng = np.random.default_rng(42)
    v_i64 = rng.integers(-1000, 1000, size=n_edges).astype(np.int64)
    v_f64 = rng.standard_normal(n_edges).astype(np.float64)
    w_i64 = rng.integers(-1000, 1000, size=n_faces).astype(np.int64)
    w_f64 = rng.standard_normal(n_faces).astype(np.float64)

    overall_ok = True

    # ------------------------------------------------------------- #
    # Graph (A) — structured incidence form, int64 and f64
    # ------------------------------------------------------------- #
    print("--- Graph (A): structured form  c = v[e_ab] + v[e_bc] - v[e_ac] ---")
    c_ref_i64 = np.asarray(d1 @ v_i64, dtype=np.int64)
    c_ref_f64 = (
        v_f64[face_edge_idx[:, 0]]
        + v_f64[face_edge_idx[:, 1]]
        - v_f64[face_edge_idx[:, 2]]
    )
    for label, etype, v, c_ref in [
        ("int64", TensorProto.INT64, v_i64, c_ref_i64),
        ("float64", TensorProto.DOUBLE, v_f64, c_ref_f64),
    ]:
        try:
            model = build_structured_d1_graph(n_edges, n_faces, etype)
            (c_onnx,) = _check_and_run(
                model, ["c"], {"v": v, "face_edge_idx": face_edge_idx}
            )
            err = float(np.max(np.abs(c_onnx - c_ref)))
            status = "OK"
        except Exception as e:  # noqa: BLE001
            status, err = f"FAIL ({e!r})", float("nan")
            overall_ok = False
        print(f"  [{label:7s}] checker+runtime: {status}   "
              f"max |c_onnx - c_ref| = {err:.3e}" if status == "OK"
              else f"  [{label:7s}] {status}")
        if status == "OK" and err != 0:
            overall_ok = False
    print()

    # ------------------------------------------------------------- #
    # Graph (B) — generic COO matvec on d¹
    # ------------------------------------------------------------- #
    overall_ok &= _run_coo_cases(
        "--- Graph (B): generic COO matvec on d¹ (curl) ---",
        d1, v_i64, v_f64,
    )

    # ------------------------------------------------------------- #
    # Graph (C) — the SAME generic builder applied to d² (divergence)
    # ------------------------------------------------------------- #
    overall_ok &= _run_coo_cases(
        "--- Graph (C): identical COO matvec builder on d² (divergence) ---",
        d2, w_i64, w_f64,
    )

    # ------------------------------------------------------------- #
    # Operator inventory + verdict
    # ------------------------------------------------------------- #
    print("Operator inventory for d¹ / d² application:")
    print("--------------------------------------------------------------")
    print("  Gather (axis=0/1, int64+f64)      EMITTABLE  native opset 18")
    print("  Add / Sub / Mul (int64, f64)      EMITTABLE  native opset 18")
    print("  ConstantOfShape (int64/f64 fill)  EMITTABLE  native opset 18")
    print("  ScatterND reduction='add' (int64) EMITTABLE  native opset 18")
    print()
    print("Construction boundary (NOT probed here — host-side by the G.6")
    print("friction class):")
    print("  build_faces (np.unique axis=0)    BLOCKED    data-dependent n_faces")
    print("  edge_to_idx / face_to_idx dicts   BLOCKED    no hash-map / sorted-")
    print("                                               unique-inverse op")
    print("  build_tet_faces parity signs      BLOCKED    consumes the dict above")
    print()
    if overall_ok:
        print("Verdict: d¹ (and d²) APPLICATION is GRAPH-PURE (emittable).")
        print("  The structured three-Gather form and the operator-agnostic COO")
        print("  matvec both lower to opset 18 and match scipy bit-exactly in")
        print("  int64 and f64. The same builder served d¹ and d² unchanged —")
        print("  application expressibility is a property of the COO input")
        print("  contract, not of any particular d-operator.")
    else:
        print("Verdict: FAILED — see errors above; audit table is stale.")
    return 0 if overall_ok else 1


if __name__ == "__main__":
    sys.exit(main())
