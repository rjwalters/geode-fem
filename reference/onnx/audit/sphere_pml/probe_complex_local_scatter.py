"""Probe: ONNX expressibility of complex Nédélec local + global scatter.

Epic #88, Phase H.5 (issue #157). This probe asks whether the complex
sphere-PML assembly variant of the Nédélec scatter (per
``reference/numpy/sphere_pml.py::assemble_global_nedelec_complex``) can
be expressed as a pure ONNX opset-18 graph.

What the PML assembly does on top of PEC
=========================================

Compared to the PEC scatter audited in
``reference/onnx/audit/sphere_pec/probe_nedelec_scatter.py``, the only
structural change is the **complex** ε scaling of the mass:

  - K_local (n_tets, 6, 6) f64 — real curl-curl (unchanged)
  - M_local (n_tets, 6, 6) f64 — real raw mass (unchanged)
  - epsilon_r (n_tets,) **complex128** — new in PML

  k_signed = k_local * sign_outer                         (real)
  m_signed = m_local.astype(c128) * sign_outer * eps[e]   (complex)
  K_global = scatter_add(real,    n_edges, n_edges)        (real)
  M_global = scatter_add(complex, n_edges, n_edges)        (complex)

Strategy: three sub-probes
==========================

(A) **Native c128 local scatter** — Construct the complex M_signed
    inside the graph by emitting `m_local * sign_outer * eps[e]` with
    `eps` typed COMPLEX128 (a graph input from a future host-computed
    `build_complex_epsilon_r_pml`). The runtime will reject the Mul
    over c128 — there is no opset-18 elementwise op that accepts c128.

(B) **Paired-real local scatter** — Emit two parallel scaffolds:
    `m_signed_re = m_local * sign_outer * eps_re[e]`
    `m_signed_im = m_local * sign_outer * eps_im[e]`
    Then two ScatterND calls into `M_re_global` and `M_im_global`.
    K_global stays a single real scatter. This IS expressible.

(C) **Native c128 ScatterND** — Schema-check only: ScatterND's type
    constraint `T` does list complex128 in opset 18, so the *graph*
    type-checks; but every upstream op that produces the c128 value
    tensor (Mul, Reshape after a c128 Constant, etc.) rejects c128 at
    runtime, so this is unreachable end-to-end.

Run
===

    python3 reference/onnx/audit/sphere_pml/probe_complex_local_scatter.py
"""

from __future__ import annotations

import sys
from pathlib import Path

import numpy as np
import onnx
import onnx.checker
import onnx.helper as oh
import onnxruntime as ort
import scipy.sparse
from onnx import TensorProto

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[4])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

from reference.numpy.nedelec_local_matrices import batched_nedelec_local_matrices  # noqa: E402
from reference.numpy.sphere_pec import build_edges  # noqa: E402

OPSET = 18
N_LOCAL = 6


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


def build_native_c128_scatter_graph(n_tets: int, n_edges: int) -> onnx.ModelProto:
    """Graph (A) — single complex M_global via c128 Mul + c128 ScatterND."""
    nodes: list[onnx.NodeProto] = []

    m_local_vi = oh.make_tensor_value_info("m_local", TensorProto.DOUBLE, [n_tets, 6, 6])
    tei_vi = oh.make_tensor_value_info("tet_edge_idx", TensorProto.INT64, [n_tets, 6])
    tes_vi = oh.make_tensor_value_info("tet_edge_sign", TensorProto.DOUBLE, [n_tets, 6])
    eps_vi = oh.make_tensor_value_info("epsilon_r", TensorProto.COMPLEX128, [n_tets])

    # Sign outer product (real)
    nodes.append(_const("ax2", np.array([2], dtype=np.int64)))
    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax2"], ["sign_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax1"], ["sign_row"]))
    nodes.append(oh.make_node("Mul", ["sign_col", "sign_row"], ["sign_outer"]))

    # Real m_signed before c128 scaling
    nodes.append(oh.make_node("Mul", ["m_local", "sign_outer"], ["m_signed_real"]))

    # Now reshape eps to (n_tets, 1, 1) — even Reshape on c128 schemati-
    # cally OK, but the c128 type tag is what blows up at session load.
    nodes.append(_const("shape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["epsilon_r", "shape_n11"], ["eps_b"]))

    # The problematic op: Mul over (f64, c128). This is rejected by both
    # the type system (no shared T) AND by the runtime kernel registry.
    nodes.append(oh.make_node("Mul", ["m_signed_real", "eps_b"], ["m_signed_c128"]))

    # Build the COO indices (real int64) — same as PEC
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax2"], ["tei_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax1"], ["tei_row"]))
    nodes.append(_const("target_shape", np.array([n_tets, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Expand", ["tei_col", "target_shape"], ["rows_3d"]))
    nodes.append(oh.make_node("Expand", ["tei_row", "target_shape"], ["cols_3d"]))
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["rows_3d", "shape_flat"], ["rows_flat"]))
    nodes.append(oh.make_node("Reshape", ["cols_3d", "shape_flat"], ["cols_flat"]))
    nodes.append(oh.make_node("Reshape", ["m_signed_c128", "shape_flat"], ["m_vals"]))
    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows_flat", "ax_neg1"], ["rows_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["cols_flat", "ax_neg1"], ["cols_col"]))
    nodes.append(oh.make_node("Concat", ["rows_col", "cols_col"], ["indices"], axis=1))

    # Zero c128 buffer (n_edges, n_edges) — ConstantOfShape does NOT
    # support c128 (T2 type constraint excludes c128/c64). We use a
    # workaround: produce a real zero buffer and try to "cast" it. Even
    # that fails because Cast doesn't accept c128 as a target.
    nodes.append(_const("shape_nn", np.array([n_edges, n_edges], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_nn"],
        outputs=["zero_buf_f64"],
        value=oh.make_tensor("z", TensorProto.DOUBLE, [1], [0.0]),
    ))
    # Best-effort: try the ScatterND with mismatched (f64 buf, c128 vals).
    # This will be rejected by the runtime as a type mismatch.
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_buf_f64", "indices", "m_vals"],
        outputs=["m_global"],
        reduction="add",
    ))

    m_global_vi = oh.make_tensor_value_info(
        "m_global", TensorProto.COMPLEX128, [n_edges, n_edges]
    )

    graph = oh.make_graph(
        nodes,
        name="complex_local_scatter_native",
        inputs=[m_local_vi, tei_vi, tes_vi, eps_vi],
        outputs=[m_global_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def build_paired_real_scatter_graph(n_tets: int, n_edges: int) -> onnx.ModelProto:
    """Graph (B) — paired-real M_re_global, M_im_global via two scatters."""
    nodes: list[onnx.NodeProto] = []

    m_local_vi = oh.make_tensor_value_info("m_local", TensorProto.DOUBLE, [n_tets, 6, 6])
    tei_vi = oh.make_tensor_value_info("tet_edge_idx", TensorProto.INT64, [n_tets, 6])
    tes_vi = oh.make_tensor_value_info("tet_edge_sign", TensorProto.DOUBLE, [n_tets, 6])
    eps_re_vi = oh.make_tensor_value_info("eps_re", TensorProto.DOUBLE, [n_tets])
    eps_im_vi = oh.make_tensor_value_info("eps_im", TensorProto.DOUBLE, [n_tets])

    # Sign outer product (real)
    nodes.append(_const("ax2", np.array([2], dtype=np.int64)))
    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax2"], ["sign_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax1"], ["sign_row"]))
    nodes.append(oh.make_node("Mul", ["sign_col", "sign_row"], ["sign_outer"]))

    # Apply sign to m_local once
    nodes.append(oh.make_node("Mul", ["m_local", "sign_outer"], ["m_signed_pre"]))

    # Two parallel scalings: real channel and imaginary channel
    nodes.append(_const("shape_n11", np.array([-1, 1, 1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["eps_re", "shape_n11"], ["eps_re_b"]))
    nodes.append(oh.make_node("Reshape", ["eps_im", "shape_n11"], ["eps_im_b"]))
    nodes.append(oh.make_node("Mul", ["m_signed_pre", "eps_re_b"], ["m_signed_re"]))
    nodes.append(oh.make_node("Mul", ["m_signed_pre", "eps_im_b"], ["m_signed_im"]))

    # COO indices (shared across both scatters)
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax2"], ["tei_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax1"], ["tei_row"]))
    nodes.append(_const("target_shape", np.array([n_tets, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Expand", ["tei_col", "target_shape"], ["rows_3d"]))
    nodes.append(oh.make_node("Expand", ["tei_row", "target_shape"], ["cols_3d"]))
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["rows_3d", "shape_flat"], ["rows_flat"]))
    nodes.append(oh.make_node("Reshape", ["cols_3d", "shape_flat"], ["cols_flat"]))
    nodes.append(oh.make_node("Reshape", ["m_signed_re", "shape_flat"], ["m_re_vals"]))
    nodes.append(oh.make_node("Reshape", ["m_signed_im", "shape_flat"], ["m_im_vals"]))
    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows_flat", "ax_neg1"], ["rows_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["cols_flat", "ax_neg1"], ["cols_col"]))
    nodes.append(oh.make_node("Concat", ["rows_col", "cols_col"], ["indices"], axis=1))

    # Two zero buffers + two scatters
    nodes.append(_const("shape_nn", np.array([n_edges, n_edges], dtype=np.int64)))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_nn"],
        outputs=["zero_re"],
        value=oh.make_tensor("zr", TensorProto.DOUBLE, [1], [0.0]),
    ))
    nodes.append(oh.make_node(
        "ConstantOfShape",
        inputs=["shape_nn"],
        outputs=["zero_im"],
        value=oh.make_tensor("zi", TensorProto.DOUBLE, [1], [0.0]),
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_re", "indices", "m_re_vals"],
        outputs=["m_re_global"],
        reduction="add",
    ))
    nodes.append(oh.make_node(
        "ScatterND",
        inputs=["zero_im", "indices", "m_im_vals"],
        outputs=["m_im_global"],
        reduction="add",
    ))

    re_vi = oh.make_tensor_value_info(
        "m_re_global", TensorProto.DOUBLE, [n_edges, n_edges]
    )
    im_vi = oh.make_tensor_value_info(
        "m_im_global", TensorProto.DOUBLE, [n_edges, n_edges]
    )

    graph = oh.make_graph(
        nodes,
        name="complex_local_scatter_paired_real",
        inputs=[m_local_vi, tei_vi, tes_vi, eps_re_vi, eps_im_vi],
        outputs=[re_vi, im_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: complex local + global scatter (sphere PML, Phase H.5) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # Synthetic mesh: 2 tets sharing a face — same as the PEC probe
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

    edges, tet_edge_idx, tet_edge_sign = build_edges(tets_np)
    n_edges = int(edges.shape[0])

    coords = nodes_np[tets_np, :]
    _, m_local, _ = batched_nedelec_local_matrices(coords)
    tet_edge_sign_f64 = tet_edge_sign.astype(np.float64)
    # Complex epsilon: per-tet (1 - 0.3j) for the second tet, real 2.25 for the first
    eps_c128 = np.array([2.25 + 0.0j, 1.0 - 0.3j], dtype=np.complex128)
    eps_re = eps_c128.real.astype(np.float64)
    eps_im = eps_c128.imag.astype(np.float64)

    print(f"Test mesh: n_tets={n_tets}, n_edges={n_edges}")
    print()

    # ------------------------------------------------------------- #
    # Graph (A) — native c128 scatter
    # ------------------------------------------------------------- #
    print("--- Graph (A): native c128 local scatter ---")
    model_a = build_native_c128_scatter_graph(n_tets, n_edges)
    try:
        onnx.checker.check_model(model_a)
        a_checker = "OK"
    except Exception as e:  # noqa: BLE001
        a_checker = "FAIL"
        a_checker_err = repr(e)
        print(f"  checker error: {a_checker_err[:300]}")
    print(f"onnx.checker: {a_checker}")

    a_rt = "skipped"
    a_err = ""
    if a_checker == "OK":
        try:
            sess_a = ort.InferenceSession(model_a.SerializeToString())
            sess_a.run(["m_global"], {
                "m_local": m_local,
                "tet_edge_idx": tet_edge_idx,
                "tet_edge_sign": tet_edge_sign_f64,
                "epsilon_r": eps_c128,
            })
            a_rt = "OK"
        except Exception as e:  # noqa: BLE001
            a_rt = "FAIL"
            a_err = repr(e)
    print(f"onnxruntime execution: {a_rt}")
    if a_err:
        msg = a_err if len(a_err) < 400 else a_err[:380] + "...(truncated)"
        print(f"  runtime error: {msg}")
    print()

    # ------------------------------------------------------------- #
    # Graph (B) — paired-real scatter
    # ------------------------------------------------------------- #
    print("--- Graph (B): paired-real (M_re_global, M_im_global) ---")
    model_b = build_paired_real_scatter_graph(n_tets, n_edges)
    try:
        onnx.checker.check_model(model_b)
        b_checker = "OK"
    except Exception as e:  # noqa: BLE001
        b_checker = f"FAIL ({e!r})"
    print(f"onnx.checker: {b_checker}")

    b_rt = "skipped"
    max_re_err = max_im_err = float("nan")
    try:
        sess_b = ort.InferenceSession(model_b.SerializeToString())
        outs = sess_b.run(["m_re_global", "m_im_global"], {
            "m_local": m_local,
            "tet_edge_idx": tet_edge_idx,
            "tet_edge_sign": tet_edge_sign_f64,
            "eps_re": eps_re,
            "eps_im": eps_im,
        })
        m_re_onnx, m_im_onnx = outs

        # NumPy complex reference
        sign_outer = tet_edge_sign_f64[:, :, None] * tet_edge_sign_f64[:, None, :]
        m_signed_c128 = (
            m_local.astype(np.complex128) * sign_outer * eps_c128[:, None, None]
        )
        rows = np.broadcast_to(tet_edge_idx[:, :, None], (n_tets, 6, 6)).ravel()
        cols = np.broadcast_to(tet_edge_idx[:, None, :], (n_tets, 6, 6)).ravel()
        m_ref = scipy.sparse.coo_matrix(
            (m_signed_c128.ravel(), (rows, cols)),
            shape=(n_edges, n_edges),
            dtype=np.complex128,
        ).toarray()

        max_re_err = float(np.max(np.abs(m_re_onnx - m_ref.real)))
        max_im_err = float(np.max(np.abs(m_im_onnx - m_ref.imag)))
        b_rt = "OK"
    except Exception as e:  # noqa: BLE001
        b_rt = f"FAIL ({e!r})"
    print(f"onnxruntime execution: {b_rt}")
    if b_rt == "OK":
        print(f"  max |M_re_onnx - Re(M_ref)| = {max_re_err:.3e}")
        print(f"  max |M_im_onnx - Im(M_ref)| = {max_im_err:.3e}")
    print()

    # ------------------------------------------------------------- #
    # Operator inventory + verdict
    # ------------------------------------------------------------- #
    print("Operator inventory for complex local + scatter:")
    print("--------------------------------------------------------------")
    print("  Mul over (f64, c128)           BLOCKED  no shared T type constraint;")
    print("                                          no runtime kernel registered.")
    print("  Mul over (c128, c128)          BLOCKED  same as above.")
    print("  ConstantOfShape with c128 fill BLOCKED  T2 type constraint excludes c128.")
    print("  ScatterND with c128 T          ACCEPTED schema-wise (c128 IS in T),")
    print("                                          but unreachable because no")
    print("                                          upstream op can produce the")
    print("                                          c128 value tensor.")
    print()
    print("  Two parallel f64 scatters       EXPRESSIBLE  one for Re(M), one for Im(M).")
    print("                                               K_global stays a single")
    print("                                               real scatter (K is real).")
    print("  Identical int64 indices, shared EXPRESSIBLE  the COO index construction")
    print("                                               is independent of dtype.")
    print()
    print("Verdict for Graph (A) — native c128 scatter:")
    print(f"  schema check: {a_checker}")
    print(f"  runtime:      {a_rt}")
    if a_err:
        print(f"  → BLOCKED. Runtime error: c128 unsupported MLDataType.")
    print()
    print("Verdict for Graph (B) — paired-real scatter:")
    print(f"  schema check: {b_checker}")
    print(f"  runtime:      {b_rt}")
    if b_rt == "OK":
        print("  → EXPRESSIBLE (fallback). Numerically bit-exact vs. NumPy ref.")
    print()
    print("Overall verdict: FALLBACK (paired-real lowering).")
    print("  The complex mass scatter must be split into two real scatters")
    print("  consuming `eps_re`, `eps_im` from the paired-real epsilon ramp.")
    print("  Downstream consumers (the LAPACK ZGGEV sidecar) re-assemble the")
    print("  c128 mass on the host: M_complex = M_re + 1j * M_im. This matches")
    print("  the eigensolve-sidecar convention from Phase G.7 / F.2: the host")
    print("  driver owns the complex generalized eigensolve boundary anyway.")
    print()

    ok = (a_rt in ("FAIL", "skipped")) and (b_checker == "OK") and (b_rt == "OK")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
