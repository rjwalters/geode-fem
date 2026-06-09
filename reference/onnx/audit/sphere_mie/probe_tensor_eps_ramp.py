"""Probe: ONNX expressibility of the anisotropic UPML tensor-ε pipeline.

Epic #88, Phase J.6 (issue #175). This probe asks whether the
**diagonal anisotropic** complex permittivity construction of the Mie
slice (``reference/numpy/sphere_mie.py::build_anisotropic_pml_tensor_diag``,
issue #171 / PR #179) and the per-axis cofactor-gram mass kernel +
complex-symmetric scatter
(``batched_nedelec_local_mass_anisotropic_diag`` /
``assemble_global_nedelec_anisotropic``) can be expressed as pure ONNX
opset-18 graphs.

Relation to the H.5 probes
==========================

Phase H.5 (``../sphere_pml/probe_complex_eps_ramp.py``,
``probe_complex_local_scatter.py``) established that opset 18's c128
type is vestigial: no op constructs c128 from two f64 tensors, no
arithmetic op accepts c128, and onnxruntime rejects c128 inputs even
where the schema accepts them. The recommended lowering is
**paired-real**: every c128 tensor becomes two f64 tensors.

The NEW question for J.6 is whether that pair-lowering **composes
cleanly with the tensor structure** — the constitutive datum is now a
diagonal 3×3 tensor per tet, ``eps_diag (n_tets, 3) complex128``, and
the mass kernel contracts a per-axis cofactor gram against it. Does
the Re/Im split force extra reshapes or transposes?

Strategy: three graphs
======================

  (A) **Native c128 control** — a minimal graph that multiplies a
      real per-axis mass term by a c128 ``eps_diag`` input. Expected
      to fail at session load (re-confirms the H.5 headline on this
      pipeline's shapes; the failure is the load-bearing evidence).

  (B) **Paired-real tensor-ε ramp** — tags (n,) int32 + centroids
      (n, 3) f64 → ``eps_re``, ``eps_im`` (n, 3) f64. The UPML stretch
      ``s = 1 − jσ/ω`` and its complex reciprocal ``1/s`` lower to
      real arithmetic (the denominator ``|s|² = 1 + (σ/ω)²`` is real),
      and the per-axis structure ``ε_α = bg·(s⁻¹ r̂_α² + s (1 − r̂_α²))``
      is plain (n,1)-against-(n,3) broadcasting. Compared elementwise
      against ``build_anisotropic_pml_tensor_diag``.

  (C) **Paired-real anisotropic mass + scatter** — coords (m,4,3) +
      ``eps_re``/``eps_im`` (m,3) + edge tables → per-element 6×6
      Re/Im mass blocks AND the global (n_edges, n_edges) Re/Im
      buffers via two ScatterND calls. The per-axis cofactor gram is
      one Einsum (``epa,eqa->eapq``); the edge-pair Kronecker ladder
      is a constant (4,4,6,6) coefficient tensor contracted by a
      second Einsum; the ε-weighting is a third Einsum per channel.
      Because the geometric factor is REAL and shared, the Re and Im
      channels each cost ONE extra Einsum — not a 4-multiply complex
      product — and no reshapes/transposes appear that the scalar
      (H.5) lowering didn't already have. Compared against
      ``batched_nedelec_local_mass_anisotropic_diag`` and
      ``assemble_global_nedelec_anisotropic``.

Run
===

    python3 reference/onnx/audit/sphere_mie/probe_tensor_eps_ramp.py
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

HERE = Path(__file__).resolve().parent
REFERENCE_ROOT = HERE.parent.parent.parent
sys.path.insert(0, str(REFERENCE_ROOT / "numpy"))

from nedelec_local_matrices import TET_LOCAL_EDGES  # noqa: E402
from sphere_pec import (  # noqa: E402
    PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR,
    R_BUFFER,
    R_PML_INNER,
    build_edges,
)
from sphere_mie import (  # noqa: E402
    K0_REF,
    SIGMA_0_DEFAULT,
    assemble_global_nedelec_anisotropic,
    batched_nedelec_local_mass_anisotropic_diag,
    build_anisotropic_pml_tensor_diag,
)

OPSET = 18
N_INSIDE = 1.5


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


def edge_pair_coefficient_tensor() -> np.ndarray:
    """Constant T (4, 4, 6, 6) with the edge-pair Kronecker ladder.

    ``m_term[e, α, i, j] = Σ_pq gg_axis[e, α, p, q] · T[p, q, i, j]``
    reproduces the four-term combination in
    ``batched_nedelec_local_mass_anisotropic_diag``:

        f_ac·gg[b,d] − f_ad·gg[b,c] − f_bc·gg[a,d] + f_bd·gg[a,c]

    T is structural (depends only on TET_LOCAL_EDGES), so it is baked
    into the graph as a Constant — no reshapes or transposes needed.
    """
    t = np.zeros((4, 4, 6, 6), dtype=np.float64)
    for i, (a, b) in enumerate(TET_LOCAL_EDGES):
        for j, (c, d) in enumerate(TET_LOCAL_EDGES):
            f_ac = 2.0 if a == c else 1.0
            f_ad = 2.0 if a == d else 1.0
            f_bc = 2.0 if b == c else 1.0
            f_bd = 2.0 if b == d else 1.0
            t[b, d, i, j] += f_ac
            t[b, c, i, j] -= f_ad
            t[a, d, i, j] -= f_bc
            t[a, c, i, j] += f_bd
    return t


# --------------------------------------------------------------------------- #
# Graph (A) — native c128 control (expected to fail at session load).
# --------------------------------------------------------------------------- #


def build_native_c128_graph(n_tets: int) -> onnx.ModelProto:
    """Multiply a real (m, 3, 6, 6) mass term by a c128 (m, 3) eps_diag."""
    nodes: list[onnx.NodeProto] = []
    m_term_vi = oh.make_tensor_value_info(
        "m_term", TensorProto.DOUBLE, [n_tets, 3, 6, 6]
    )
    eps_vi = oh.make_tensor_value_info(
        "eps_diag", TensorProto.COMPLEX128, [n_tets, 3]
    )
    nodes.append(_const("shape_b", np.array([n_tets, 3, 1, 1], dtype=np.int64)))
    # Reshape on c128 is schema-OK; the c128 Mul is the blocked op.
    nodes.append(oh.make_node("Reshape", ["eps_diag", "shape_b"], ["eps_b"]))
    nodes.append(oh.make_node("Mul", ["m_term", "eps_b"], ["m_axis_c128"]))
    out_vi = oh.make_tensor_value_info(
        "m_axis_c128", TensorProto.COMPLEX128, [n_tets, 3, 6, 6]
    )
    graph = oh.make_graph(
        nodes,
        name="tensor_eps_native_c128_probe",
        inputs=[m_term_vi, eps_vi],
        outputs=[out_vi],
    )
    return oh.make_model(
        graph, opset_imports=[oh.make_opsetid("", OPSET)], ir_version=9
    )


# --------------------------------------------------------------------------- #
# Graph (B) — paired-real anisotropic tensor-ε ramp.
# --------------------------------------------------------------------------- #


def build_paired_real_tensor_ramp_graph() -> onnx.ModelProto:
    """tags (n,) int32 + centroids (n, 3) f64 → eps_re, eps_im (n, 3) f64.

    Mirrors ``build_anisotropic_pml_tensor_diag`` with the complex
    stretch lowered to real arithmetic:

        s      = 1 − jσ/ω          → (s_re, s_im) = (1, −σ/ω)
        1/s    = conj(s)/|s|²      → (s_re/d, −s_im/d), d = s_re² + s_im²
        ε_α    = bg·(s⁻¹ w_α + s (1 − w_α)),  w_α = r̂_α²

    bg and w are real, so Re/Im factor through shared real tensors.
    The per-axis structure is (n,1)-vs-(n,3) broadcasting only.
    """
    nodes: list[onnx.NodeProto] = []

    tags_vi = oh.make_tensor_value_info("tags", TensorProto.INT32, ["N"])
    cent_vi = oh.make_tensor_value_info("centroids", TensorProto.DOUBLE, ["N", 3])

    width = R_BUFFER - R_PML_INNER
    omega = max(float(K0_REF), 1e-12)

    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(_const("phys_interior", np.array(PHYS_SPHERE_INTERIOR, dtype=np.int32)))
    nodes.append(_const("phys_pml", np.array(PHYS_PML_SHELL, dtype=np.int32)))
    nodes.append(_const("n2", np.array(N_INSIDE * N_INSIDE, dtype=np.float64)))
    nodes.append(_const("one", np.array(1.0, dtype=np.float64)))
    nodes.append(_const("zero", np.array(0.0, dtype=np.float64)))
    nodes.append(_const("r_inner", np.array(R_PML_INNER, dtype=np.float64)))
    nodes.append(_const("inv_width", np.array(1.0 / width, dtype=np.float64)))
    nodes.append(_const("sigma0", np.array(float(SIGMA_0_DEFAULT), dtype=np.float64)))
    nodes.append(_const("neg_inv_omega", np.array(-1.0 / omega, dtype=np.float64)))
    nodes.append(_const("r_guard", np.array(1e-12, dtype=np.float64)))

    # r_c (n, 1) — keepdims so everything below broadcasts against (n, 3).
    nodes.append(oh.make_node("Mul", ["centroids", "centroids"], ["c_sq"]))
    nodes.append(oh.make_node("ReduceSum", ["c_sq", "ax1"], ["r2"], keepdims=1))
    nodes.append(oh.make_node("Sqrt", ["r2"], ["r_c"]))

    # Selector masks (n, 1).
    nodes.append(oh.make_node("Equal", ["tags", "phys_interior"], ["is_int_flat"]))
    nodes.append(oh.make_node("Equal", ["tags", "phys_pml"], ["is_pml_flat"]))
    nodes.append(oh.make_node("Unsqueeze", ["is_int_flat", "ax1"], ["is_int"]))
    nodes.append(oh.make_node("Unsqueeze", ["is_pml_flat", "ax1"], ["is_pml"]))
    nodes.append(oh.make_node("Greater", ["r_c", "r_inner"], ["past_inner"]))
    nodes.append(oh.make_node("And", ["is_pml", "past_inner"], ["in_shell"]))

    # Background scalar bg (n, 1): n² inside, 1 elsewhere.
    nodes.append(oh.make_node("Where", ["is_int", "n2", "one"], ["bg"]))

    # Ramp: u = clip((r_c − R_PML_INNER)/Δ, 0, 1); σ = σ₀ u².
    nodes.append(oh.make_node("Sub", ["r_c", "r_inner"], ["r_shift"]))
    nodes.append(oh.make_node("Mul", ["r_shift", "inv_width"], ["u_raw"]))
    nodes.append(oh.make_node("Clip", ["u_raw", "zero", "one"], ["u"]))
    nodes.append(oh.make_node("Mul", ["u", "u"], ["u2"]))
    nodes.append(oh.make_node("Mul", ["sigma0", "u2"], ["sigma"]))

    # Complex stretch s = 1 − jσ/ω lowered: s_re = 1, s_im = −σ/ω.
    nodes.append(oh.make_node("Mul", ["sigma", "neg_inv_omega"], ["s_im"]))
    # 1/s = conj(s)/|s|²: denom = 1 + s_im² (s_re ≡ 1).
    nodes.append(oh.make_node("Mul", ["s_im", "s_im"], ["s_im2"]))
    nodes.append(oh.make_node("Add", ["one", "s_im2"], ["denom"]))
    nodes.append(oh.make_node("Reciprocal", ["denom"], ["inv_denom"]))
    nodes.append(oh.make_node("Neg", ["s_im"], ["neg_s_im"]))
    nodes.append(oh.make_node("Mul", ["neg_s_im", "inv_denom"], ["sinv_im"]))
    # sinv_re = s_re/denom = inv_denom (since s_re ≡ 1).

    # Radial direction r̂ (n, 3), with the |c| ≈ 0 defensive guard.
    nodes.append(oh.make_node("Greater", ["r_c", "r_guard"], ["r_pos"]))
    nodes.append(oh.make_node("Reciprocal", ["r_c"], ["inv_r_raw"]))
    nodes.append(oh.make_node("Where", ["r_pos", "inv_r_raw", "zero"], ["inv_r"]))
    nodes.append(oh.make_node("Mul", ["centroids", "inv_r"], ["r_hat"]))
    nodes.append(oh.make_node("Mul", ["r_hat", "r_hat"], ["w"]))
    nodes.append(oh.make_node("Sub", ["one", "w"], ["one_minus_w"]))

    # ε_α = bg·(s⁻¹ w_α + s (1 − w_α)) — Re and Im channels, (n,1)·(n,3).
    nodes.append(oh.make_node("Mul", ["inv_denom", "w"], ["re_a"]))
    # s_re ≡ 1 → s_re·(1−w) = one_minus_w.
    nodes.append(oh.make_node("Add", ["re_a", "one_minus_w"], ["shell_re_unit"]))
    nodes.append(oh.make_node("Mul", ["bg", "shell_re_unit"], ["shell_re"]))

    nodes.append(oh.make_node("Mul", ["sinv_im", "w"], ["im_a"]))
    nodes.append(oh.make_node("Mul", ["s_im", "one_minus_w"], ["im_b"]))
    nodes.append(oh.make_node("Add", ["im_a", "im_b"], ["shell_im_unit"]))
    nodes.append(oh.make_node("Mul", ["bg", "shell_im_unit"], ["shell_im"]))

    # Default (interior / vacuum / shell-guard): real isotropic bg.
    # Where broadcasts the (n,1) condition AND the (n,1)/scalar branches
    # against the (n,3) shell values — no Expand/Reshape needed.
    nodes.append(oh.make_node("Where", ["in_shell", "shell_re", "bg"], ["eps_re"]))
    nodes.append(oh.make_node("Where", ["in_shell", "shell_im", "zero"], ["eps_im"]))

    re_vi = oh.make_tensor_value_info("eps_re", TensorProto.DOUBLE, ["N", 3])
    im_vi = oh.make_tensor_value_info("eps_im", TensorProto.DOUBLE, ["N", 3])

    graph = oh.make_graph(
        nodes,
        name="tensor_eps_paired_real_ramp",
        inputs=[tags_vi, cent_vi],
        outputs=[re_vi, im_vi],
    )
    return oh.make_model(
        graph, opset_imports=[oh.make_opsetid("", OPSET)], ir_version=9
    )


# --------------------------------------------------------------------------- #
# Graph (C) — paired-real anisotropic local mass + global scatter.
# --------------------------------------------------------------------------- #


def _emit_cross(nodes, a, b, out, tag):
    """cross(a, b) for (m, 3) tensors via component Gather + Mul/Sub."""
    for comp, idx in (("x", 0), ("y", 1), ("z", 2)):
        nodes.append(_const(f"{tag}_i{comp}", np.array(idx, dtype=np.int64)))
        nodes.append(
            oh.make_node("Gather", [a, f"{tag}_i{comp}"], [f"{tag}_a{comp}"], axis=1)
        )
        nodes.append(
            oh.make_node("Gather", [b, f"{tag}_i{comp}"], [f"{tag}_b{comp}"], axis=1)
        )
    pieces = []
    for comp, (p, q) in (("x", ("y", "z")), ("y", ("z", "x")), ("z", ("x", "y"))):
        nodes.append(
            oh.make_node("Mul", [f"{tag}_a{p}", f"{tag}_b{q}"], [f"{tag}_m1{comp}"])
        )
        nodes.append(
            oh.make_node("Mul", [f"{tag}_a{q}", f"{tag}_b{p}"], [f"{tag}_m2{comp}"])
        )
        nodes.append(
            oh.make_node("Sub", [f"{tag}_m1{comp}", f"{tag}_m2{comp}"], [f"{tag}_c{comp}"])
        )
        nodes.append(
            oh.make_node("Unsqueeze", [f"{tag}_c{comp}", "ax1"], [f"{tag}_cu{comp}"])
        )
        pieces.append(f"{tag}_cu{comp}")
    nodes.append(oh.make_node("Concat", pieces, [out], axis=1))


def build_paired_real_mass_scatter_graph(n_tets: int, n_edges: int) -> onnx.ModelProto:
    """coords + paired-real eps_diag + edge tables → local 6×6 Re/Im
    blocks and global (n_edges, n_edges) Re/Im buffers."""
    nodes: list[onnx.NodeProto] = []

    coords_vi = oh.make_tensor_value_info(
        "coords", TensorProto.DOUBLE, [n_tets, 4, 3]
    )
    eps_re_vi = oh.make_tensor_value_info("eps_re", TensorProto.DOUBLE, [n_tets, 3])
    eps_im_vi = oh.make_tensor_value_info("eps_im", TensorProto.DOUBLE, [n_tets, 3])
    tei_vi = oh.make_tensor_value_info("tet_edge_idx", TensorProto.INT64, [n_tets, 6])
    tes_vi = oh.make_tensor_value_info("tet_edge_sign", TensorProto.DOUBLE, [n_tets, 6])

    nodes.append(_const("ax1", np.array([1], dtype=np.int64)))
    nodes.append(_const("ax2", np.array([2], dtype=np.int64)))

    # Vertex extraction v0..v3 (m, 3) and edge vectors.
    for vi in range(4):
        nodes.append(_const(f"vidx{vi}", np.array(vi, dtype=np.int64)))
        nodes.append(oh.make_node("Gather", ["coords", f"vidx{vi}"], [f"v{vi}"], axis=1))
    nodes.append(oh.make_node("Sub", ["v1", "v0"], ["e1"]))
    nodes.append(oh.make_node("Sub", ["v2", "v0"], ["e2"]))
    nodes.append(oh.make_node("Sub", ["v3", "v0"], ["e3"]))

    # Cofactor vectors g1 = e2×e3, g2 = e3×e1, g3 = e1×e2, g0 = −(g1+g2+g3).
    _emit_cross(nodes, "e2", "e3", "g1", "x1")
    _emit_cross(nodes, "e3", "e1", "g2", "x2")
    _emit_cross(nodes, "e1", "e2", "g3", "x3")
    nodes.append(oh.make_node("Add", ["g1", "g2"], ["g12"]))
    nodes.append(oh.make_node("Add", ["g12", "g3"], ["g123"]))
    nodes.append(oh.make_node("Neg", ["g123"], ["g0"]))

    # det = Σ e1·g1 (m,); scale = 1/(120 |det|) as (m, 1, 1).
    nodes.append(oh.make_node("Mul", ["e1", "g1"], ["e1g1"]))
    nodes.append(oh.make_node("ReduceSum", ["e1g1", "ax1"], ["det"], keepdims=0))
    nodes.append(oh.make_node("Abs", ["det"], ["abs_det"]))
    nodes.append(_const("c120", np.array(120.0, dtype=np.float64)))
    nodes.append(oh.make_node("Mul", ["abs_det", "c120"], ["det120"]))
    nodes.append(oh.make_node("Reciprocal", ["det120"], ["scale_flat"]))
    nodes.append(_const("ax12", np.array([1, 2], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["scale_flat", "ax12"], ["scale"]))

    # g_mat (m, 4, 3) via Unsqueeze + Concat.
    for gi in range(4):
        nodes.append(oh.make_node("Unsqueeze", [f"g{gi}", "ax1"], [f"g{gi}_u"]))
    nodes.append(
        oh.make_node("Concat", ["g0_u", "g1_u", "g2_u", "g3_u"], ["g_mat"], axis=1)
    )

    # Per-axis cofactor gram: gg_axis (m, 3, 4, 4) — ONE Einsum.
    nodes.append(
        oh.make_node("Einsum", ["g_mat", "g_mat"], ["gg_axis"], equation="epa,eqa->eapq")
    )

    # Edge-pair ladder: constant T (4,4,6,6), m_axis (m, 3, 6, 6).
    nodes.append(_const("T_pairs", edge_pair_coefficient_tensor()))
    nodes.append(
        oh.make_node(
            "Einsum", ["gg_axis", "T_pairs"], ["m_axis"], equation="eapq,pqij->eaij"
        )
    )

    # ε-weighting: Re/Im channels share m_axis — one Einsum each.
    nodes.append(
        oh.make_node("Einsum", ["m_axis", "eps_re"], ["m_re_raw"], equation="eaij,ea->eij")
    )
    nodes.append(
        oh.make_node("Einsum", ["m_axis", "eps_im"], ["m_im_raw"], equation="eaij,ea->eij")
    )
    nodes.append(oh.make_node("Mul", ["m_re_raw", "scale"], ["m_re_local"]))
    nodes.append(oh.make_node("Mul", ["m_im_raw", "scale"], ["m_im_local"]))

    # Sign outer product (real, shared) and signed blocks.
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax2"], ["sign_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_sign", "ax1"], ["sign_row"]))
    nodes.append(oh.make_node("Mul", ["sign_col", "sign_row"], ["sign_outer"]))
    nodes.append(oh.make_node("Mul", ["m_re_local", "sign_outer"], ["m_re_signed"]))
    nodes.append(oh.make_node("Mul", ["m_im_local", "sign_outer"], ["m_im_signed"]))

    # COO indices — identical to the H.5/G.6 pattern, shared by Re/Im.
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax2"], ["tei_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["tet_edge_idx", "ax1"], ["tei_row"]))
    nodes.append(_const("target_shape", np.array([n_tets, 6, 6], dtype=np.int64)))
    nodes.append(oh.make_node("Expand", ["tei_col", "target_shape"], ["rows_3d"]))
    nodes.append(oh.make_node("Expand", ["tei_row", "target_shape"], ["cols_3d"]))
    nodes.append(_const("shape_flat", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Reshape", ["rows_3d", "shape_flat"], ["rows_flat"]))
    nodes.append(oh.make_node("Reshape", ["cols_3d", "shape_flat"], ["cols_flat"]))
    nodes.append(oh.make_node("Reshape", ["m_re_signed", "shape_flat"], ["m_re_vals"]))
    nodes.append(oh.make_node("Reshape", ["m_im_signed", "shape_flat"], ["m_im_vals"]))
    nodes.append(_const("ax_neg1", np.array([-1], dtype=np.int64)))
    nodes.append(oh.make_node("Unsqueeze", ["rows_flat", "ax_neg1"], ["rows_col"]))
    nodes.append(oh.make_node("Unsqueeze", ["cols_flat", "ax_neg1"], ["cols_col"]))
    nodes.append(oh.make_node("Concat", ["rows_col", "cols_col"], ["indices"], axis=1))

    # Two zero buffers + two scatters (paired-real, H.5 disposition).
    nodes.append(_const("shape_nn", np.array([n_edges, n_edges], dtype=np.int64)))
    for ch in ("re", "im"):
        nodes.append(
            oh.make_node(
                "ConstantOfShape",
                inputs=["shape_nn"],
                outputs=[f"zero_{ch}"],
                value=oh.make_tensor(f"z{ch}", TensorProto.DOUBLE, [1], [0.0]),
            )
        )
        nodes.append(
            oh.make_node(
                "ScatterND",
                inputs=[f"zero_{ch}", "indices", f"m_{ch}_vals"],
                outputs=[f"m_{ch}_global"],
                reduction="add",
            )
        )

    outs = [
        oh.make_tensor_value_info("m_re_local", TensorProto.DOUBLE, [n_tets, 6, 6]),
        oh.make_tensor_value_info("m_im_local", TensorProto.DOUBLE, [n_tets, 6, 6]),
        oh.make_tensor_value_info("m_re_global", TensorProto.DOUBLE, [n_edges, n_edges]),
        oh.make_tensor_value_info("m_im_global", TensorProto.DOUBLE, [n_edges, n_edges]),
    ]
    graph = oh.make_graph(
        nodes,
        name="tensor_eps_paired_real_mass_scatter",
        inputs=[coords_vi, eps_re_vi, eps_im_vi, tei_vi, tes_vi],
        outputs=outs,
    )
    return oh.make_model(
        graph, opset_imports=[oh.make_opsetid("", OPSET)], ir_version=9
    )


# --------------------------------------------------------------------------- #
# Driver
# --------------------------------------------------------------------------- #


def main() -> int:
    print("== Probe: anisotropic UPML tensor-ε ramp + mass scatter (Phase J.6) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    # ------------------------------------------------------------- #
    # Test fixture for the ramp: 8 tets spanning all regions and
    # axis-aligned / oblique radial directions.
    # ------------------------------------------------------------- #
    centroids_np = np.array(
        [
            [0.10, 0.10, 0.10],   # interior (oblique, r ≈ 0.17)
            [0.50, 0.00, 0.00],   # interior (axis-aligned)
            [1.20, 0.30, 0.00],   # vacuum gap
            [1.40, 0.00, 0.30],   # PML tag but r ≤ R_PML_INNER → guard branch
            [1.70, 0.00, 0.00],   # PML mid-ramp, x-aligned (w = (1,0,0))
            [0.00, 1.80, 0.00],   # PML mid-ramp, y-aligned
            [1.00, 1.00, 1.00],   # PML mid-ramp, oblique (r ≈ 1.73)
            [1.50, 1.20, 0.80],   # PML past R_BUFFER → clamped u = 1
        ],
        dtype=np.float64,
    )
    tags_np = np.array(
        [
            PHYS_SPHERE_INTERIOR,
            PHYS_SPHERE_INTERIOR,
            999,
            PHYS_PML_SHELL,
            PHYS_PML_SHELL,
            PHYS_PML_SHELL,
            PHYS_PML_SHELL,
            PHYS_PML_SHELL,
        ],
        dtype=np.int32,
    )
    # Sanity: row 3 really exercises the r ≤ R_PML_INNER shell guard.
    assert np.linalg.norm(centroids_np[3]) <= R_PML_INNER

    eps_ref = build_anisotropic_pml_tensor_diag(
        tags_np, centroids_np, n_inside=N_INSIDE, sigma_0=SIGMA_0_DEFAULT, k0_ref=K0_REF
    )

    # ------------------------------------------------------------- #
    # Graph (A) — native c128 control
    # ------------------------------------------------------------- #
    print("--- Graph (A): native c128 Mul over (m_term f64, eps_diag c128) ---")
    model_a = build_native_c128_graph(n_tets=2)
    try:
        onnx.checker.check_model(model_a)
        a_checker = "OK"
    except Exception as e:  # noqa: BLE001
        a_checker = f"FAIL ({e!r:.200})"
    print(f"onnx.checker: {a_checker}")

    a_rt = "skipped"
    a_err = ""
    try:
        sess_a = ort.InferenceSession(model_a.SerializeToString())
        sess_a.run(
            ["m_axis_c128"],
            {
                "m_term": np.zeros((2, 3, 6, 6), dtype=np.float64),
                "eps_diag": np.ones((2, 3), dtype=np.complex128),
            },
        )
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
    # Graph (B) — paired-real tensor ramp
    # ------------------------------------------------------------- #
    print("--- Graph (B): paired-real tensor-ε ramp (eps_re, eps_im (n,3)) ---")
    model_b = build_paired_real_tensor_ramp_graph()
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
        re_onnx, im_onnx = sess_b.run(
            ["eps_re", "eps_im"], {"tags": tags_np, "centroids": centroids_np}
        )
        max_re_err = float(np.max(np.abs(re_onnx - eps_ref.real)))
        max_im_err = float(np.max(np.abs(im_onnx - eps_ref.imag)))
        b_rt = "OK"
    except Exception as e:  # noqa: BLE001
        b_rt = f"FAIL ({e!r})"
    print(f"onnxruntime execution: {b_rt}")
    if b_rt == "OK":
        print(f"  max |eps_re_onnx - Re(eps_ref)| = {max_re_err:.3e}")
        print(f"  max |eps_im_onnx - Im(eps_ref)| = {max_im_err:.3e}")
    b_ok = b_rt == "OK" and max_re_err < 1e-13 and max_im_err < 1e-13
    print()

    # ------------------------------------------------------------- #
    # Graph (C) — paired-real anisotropic mass + scatter
    # ------------------------------------------------------------- #
    print("--- Graph (C): paired-real anisotropic mass kernel + scatter ---")
    # 2-tet mesh sharing a face (same as the H.5 scatter probe).
    tets_np = np.array([[0, 1, 2, 3], [1, 2, 3, 4]], dtype=np.int64)
    nodes_np = np.array(
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.5, 0.5],
        ],
        dtype=np.float64,
    )
    n_tets = tets_np.shape[0]
    edges, tet_edge_idx, tet_edge_sign = build_edges(tets_np)
    n_edges = int(edges.shape[0])
    tet_edge_sign_f64 = tet_edge_sign.astype(np.float64)
    coords = nodes_np[tets_np, :]

    # Non-trivial anisotropic ε: take REFERENCE ramp values from two of
    # the synthetic centroids above — one interior (real isotropic) and
    # the clamped oblique shell tet, whose unequal direction components
    # (1.5, 1.2, 0.8)/|c| give three DISTINCT complex diagonal entries —
    # so the mass test consumes genuinely anisotropic data.
    eps_diag_c = np.vstack([eps_ref[1], eps_ref[7]])  # (2, 3) complex128
    assert np.any(eps_diag_c.imag != 0.0)
    assert not np.allclose(eps_diag_c[1].real, eps_diag_c[1, 0].real)
    eps_re_np = np.ascontiguousarray(eps_diag_c.real)
    eps_im_np = np.ascontiguousarray(eps_diag_c.imag)

    m_local_ref = batched_nedelec_local_mass_anisotropic_diag(coords, eps_diag_c)
    _k_ref, m_global_ref = assemble_global_nedelec_anisotropic(
        nodes_np, tets_np, edges, tet_edge_idx, tet_edge_sign_f64, eps_diag_c
    )
    m_global_ref = m_global_ref.toarray()

    model_c = build_paired_real_mass_scatter_graph(n_tets, n_edges)
    try:
        onnx.checker.check_model(model_c)
        c_checker = "OK"
    except Exception as e:  # noqa: BLE001
        c_checker = f"FAIL ({e!r})"
    print(f"onnx.checker: {c_checker}")

    c_rt = "skipped"
    errs = {}
    try:
        sess_c = ort.InferenceSession(model_c.SerializeToString())
        outs = sess_c.run(
            ["m_re_local", "m_im_local", "m_re_global", "m_im_global"],
            {
                "coords": coords,
                "eps_re": eps_re_np,
                "eps_im": eps_im_np,
                "tet_edge_idx": tet_edge_idx,
                "tet_edge_sign": tet_edge_sign_f64,
            },
        )
        m_re_loc, m_im_loc, m_re_glob, m_im_glob = outs
        errs["local Re"] = float(np.max(np.abs(m_re_loc - m_local_ref.real)))
        errs["local Im"] = float(np.max(np.abs(m_im_loc - m_local_ref.imag)))
        errs["global Re"] = float(np.max(np.abs(m_re_glob - m_global_ref.real)))
        errs["global Im"] = float(np.max(np.abs(m_im_glob - m_global_ref.imag)))
        c_rt = "OK"
    except Exception as e:  # noqa: BLE001
        c_rt = f"FAIL ({e!r})"
    print(f"onnxruntime execution: {c_rt}")
    c_ok = c_rt == "OK"
    if c_rt == "OK":
        for label, err in errs.items():
            print(f"  max |{label} onnx - ref| = {err:.3e}")
            # Einsum association order differs from the NumPy loop, so
            # allow roundoff (values are O(0.01..0.1) on this mesh).
            c_ok = c_ok and err < 1e-14
        # Complex-symmetric (NOT Hermitian) check on the assembled global.
        sym_re = float(np.max(np.abs(m_re_glob - m_re_glob.T)))
        sym_im = float(np.max(np.abs(m_im_glob - m_im_glob.T)))
        print(f"  complex-symmetry: max|Re(M)-Re(M)^T| = {sym_re:.3e}, "
              f"max|Im(M)-Im(M)^T| = {sym_im:.3e}")
        c_ok = c_ok and sym_re == 0.0 and sym_im == 0.0
    print()

    # ------------------------------------------------------------- #
    # Verdict
    # ------------------------------------------------------------- #
    print("Verdicts:")
    print("--------------------------------------------------------------")
    print(f"  (A) native c128 tensor Mul:    schema={a_checker}, runtime={a_rt}")
    print("      → BLOCKED (inherits H.5: no c128 kernel in onnxruntime).")
    print(f"  (B) paired-real tensor ramp:   {'EXPRESSIBLE' if b_ok else 'FAILED'}")
    print("      → the complex stretch s and 1/s lower to REAL arithmetic")
    print("        (real denominator |s|²); the diagonal-tensor structure is")
    print("        one broadcast axis, (n,1) against (n,3). No reshapes or")
    print("        transposes beyond the scalar H.5 lowering.")
    print(f"  (C) paired-real mass+scatter:  {'EXPRESSIBLE' if c_ok else 'FAILED'}")
    print("      → per-axis cofactor gram = 1 Einsum; edge-pair ladder = 1")
    print("        Einsum against a constant (4,4,6,6) tensor; ε-weighting =")
    print("        1 Einsum PER CHANNEL sharing the real m_axis. Pair-lowering")
    print("        composes cleanly: 2 extra Einsums + 1 extra ScatterND, not")
    print("        a 4-multiply complex product, and zero extra reshapes.")
    print()
    overall = (a_rt == "FAIL") and b_ok and c_ok
    print(f"Overall: {'FALLBACK (paired-real lowering) — composes cleanly with' if overall else 'PROBE FAILED for'} the tensor structure.")
    return 0 if overall else 1


if __name__ == "__main__":
    sys.exit(main())
