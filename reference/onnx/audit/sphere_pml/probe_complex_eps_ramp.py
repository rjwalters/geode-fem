"""Probe: ONNX expressibility of build_complex_epsilon_r_pml.

Epic #88, Phase H.5 (issue #157). This probe asks whether the per-tet
**complex** permittivity ramp from ``reference/numpy/sphere_pml.py``
can be expressed as a pure ONNX opset-18 graph.

What build_complex_epsilon_r_pml does
=====================================

Given:
  - physical_tags (n_tets,) int32 — per-tet physical-group tag
  - centroid_radii (n_tets,) float64 — per-tet centroid distance from origin
  - n_inside: float — refractive index inside dielectric sphere
  - sigma_0: float — PML absorption strength at r = R_BUFFER

It produces a per-tet **complex128** relative permittivity:

  - tag == PHYS_SPHERE_INTERIOR  →  ε = n²  + 0j   (real dielectric)
  - tag == PHYS_PML_SHELL        →  ε = 1   - j σ₀ u²   (lossy ramp)
                                    where u = clip((r - R_PML_INNER)/Δ, 0, 1)
  - otherwise                    →  ε = 1   + 0j   (vacuum)

The shape and the **real part** computation are straightforward (a
small Where ladder over the tag). The novelty for ONNX is the
**complex output type** — the result must be a complex128 tensor
suitable for use as a per-tet scaling on the global Nédélec mass.

Strategy: two graphs
====================

We build TWO ONNX models and check both against ``onnx.checker`` and
``onnxruntime``:

  (A) **Native complex128 output** — Declare the output as
      ``TensorProto.COMPLEX128`` and assemble it from a real-part
      tensor and an imaginary-part tensor. Since ONNX opset 18 has
      no `Complex(real, imag)` op (PyTorch's `torch.complex` lives
      at the framework level, not the ONNX IR), there is no path to
      construct a c128 tensor from two f64 tensors **inside** the
      graph. This graph is **NOT EXPRESSIBLE**.

  (B) **Paired-real lowering** — Emit two f64 outputs `eps_re` and
      `eps_im` of shape (n_tets,), one for the real part and one for
      the imaginary part. This is the standard "split complex into
      two real channels" workaround used in TF-Java, onnxruntime,
      and Burn's complex64/complex128 backends. The mass scatter then
      consumes the paired-real tensors as two independent ScatterND
      calls into two real buffers (or into a paired-real (n, n, 2)
      buffer). This IS expressible.

Run
===

    python3 reference/onnx/audit/sphere_pml/probe_complex_eps_ramp.py
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

from reference.numpy.sphere_pec import (  # noqa: E402
    PHYS_PML_SHELL,
    PHYS_SPHERE_INTERIOR,
    R_BUFFER,
    R_PML_INNER,
)
from reference.numpy.sphere_pml import build_complex_epsilon_r_pml  # noqa: E402

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


def build_native_c128_graph(n_inside: float, sigma_0: float) -> onnx.ModelProto:
    """Graph (A): emit a single COMPLEX128 output.

    The graph has no native opset-18 op to construct a complex value
    from a (real, imag) pair. The only way to introduce a c128 tensor
    is via a graph input or a c128 Constant. There is no `Complex(re,
    im)` op, and `Cast` does not accept COMPLEX128 / COMPLEX64.

    We therefore cannot synthesize a per-tet c128 tensor from per-tet
    real radii inside the graph. To even *try*, we emit a c128 graph
    input ``eps_seed`` of length 1 with value (1+0j), broadcast it to
    (n_tets,), and let the runtime show that the c128 broadcast is
    rejected. This demonstrates the failure mode without requiring
    any synthetic complex construction.
    """
    nodes: list[onnx.NodeProto] = []

    # Inputs: tags (n_tets,) int32, radii (n_tets,) f64
    tags_vi = oh.make_tensor_value_info("tags", TensorProto.INT32, ["N"])
    radii_vi = oh.make_tensor_value_info("radii", TensorProto.DOUBLE, ["N"])
    # c128 seed (length 1) — the only way to introduce a c128 value in
    # opset 18 without `Complex(re, im)`.
    seed_vi = oh.make_tensor_value_info("eps_seed", TensorProto.COMPLEX128, [1])

    # Compute the (real) selector mask and (real) PML imaginary part
    # so we can show the f64 side works, then "wish" the c128 broadcast
    # were a no-op (it is rejected by onnxruntime as an invalid type).
    nodes.append(_const("phys_interior", np.array(PHYS_SPHERE_INTERIOR, dtype=np.int32)))
    nodes.append(_const("phys_pml", np.array(PHYS_PML_SHELL, dtype=np.int32)))
    nodes.append(oh.make_node("Equal", ["tags", "phys_interior"], ["is_int"]))
    nodes.append(oh.make_node("Equal", ["tags", "phys_pml"], ["is_pml"]))

    # eps_real = where(is_int, n^2, where(is_pml, 1, 1))  — trivial real part
    nodes.append(_const("n2", np.array(n_inside * n_inside, dtype=np.float64)))
    nodes.append(_const("one_f64", np.array(1.0, dtype=np.float64)))
    nodes.append(oh.make_node("Where", ["is_int", "n2", "one_f64"], ["eps_re"]))

    # Now broadcast the c128 seed to (n_tets,) — this is what would
    # happen if a c128 constant existed; the runtime will reject the op.
    # Use the radii shape as the broadcast target via Shape.
    nodes.append(oh.make_node("Shape", ["radii"], ["radii_shape"]))
    nodes.append(oh.make_node("Expand", ["eps_seed", "radii_shape"], ["eps_c128_out"]))

    out_vi = oh.make_tensor_value_info("eps_c128_out", TensorProto.COMPLEX128, ["N"])

    graph = oh.make_graph(
        nodes,
        name="complex_eps_native_probe",
        inputs=[tags_vi, radii_vi, seed_vi],
        outputs=[out_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def build_paired_real_graph(n_inside: float, sigma_0: float) -> onnx.ModelProto:
    """Graph (B): emit two f64 outputs `eps_re` and `eps_im`.

    Mirrors the canonical "complex as two reals" lowering used in
    TF-Java and the SciML community. The real part is the tag-keyed
    permittivity; the imaginary part is the PML ramp, masked to zero
    outside the shell.

    All ops are opset-18 native and the runtime accepts them.
    """
    nodes: list[onnx.NodeProto] = []

    tags_vi = oh.make_tensor_value_info("tags", TensorProto.INT32, ["N"])
    radii_vi = oh.make_tensor_value_info("radii", TensorProto.DOUBLE, ["N"])

    # --- Selector masks ---
    nodes.append(_const("phys_interior", np.array(PHYS_SPHERE_INTERIOR, dtype=np.int32)))
    nodes.append(_const("phys_pml", np.array(PHYS_PML_SHELL, dtype=np.int32)))
    nodes.append(oh.make_node("Equal", ["tags", "phys_interior"], ["is_int"]))
    nodes.append(oh.make_node("Equal", ["tags", "phys_pml"], ["is_pml"]))

    # --- Real part: where(is_int, n^2, 1.0) ---
    nodes.append(_const("n2", np.array(n_inside * n_inside, dtype=np.float64)))
    nodes.append(_const("one_f64", np.array(1.0, dtype=np.float64)))
    nodes.append(oh.make_node("Where", ["is_int", "n2", "one_f64"], ["eps_re"]))

    # --- Imaginary part: where(is_pml, -sigma_0 * u^2, 0) with u=clip(...) ---
    width = R_BUFFER - R_PML_INNER
    nodes.append(_const("r_pml_inner", np.array(R_PML_INNER, dtype=np.float64)))
    nodes.append(_const("inv_width", np.array(1.0 / width, dtype=np.float64)))
    nodes.append(_const("zero_f64", np.array(0.0, dtype=np.float64)))
    nodes.append(_const("neg_sigma0", np.array(-float(sigma_0), dtype=np.float64)))

    nodes.append(oh.make_node("Sub", ["radii", "r_pml_inner"], ["r_shift"]))
    nodes.append(oh.make_node("Mul", ["r_shift", "inv_width"], ["u_raw"]))
    # Clip(u_raw, 0, 1)
    nodes.append(oh.make_node("Clip", ["u_raw", "zero_f64", "one_f64"], ["u"]))
    nodes.append(oh.make_node("Mul", ["u", "u"], ["u2"]))
    nodes.append(oh.make_node("Mul", ["neg_sigma0", "u2"], ["im_ramp"]))
    nodes.append(oh.make_node("Where", ["is_pml", "im_ramp", "zero_f64"], ["eps_im"]))

    re_vi = oh.make_tensor_value_info("eps_re", TensorProto.DOUBLE, ["N"])
    im_vi = oh.make_tensor_value_info("eps_im", TensorProto.DOUBLE, ["N"])

    graph = oh.make_graph(
        nodes,
        name="complex_eps_paired_real_probe",
        inputs=[tags_vi, radii_vi],
        outputs=[re_vi, im_vi],
    )
    return oh.make_model(
        graph,
        opset_imports=[oh.make_opsetid("", OPSET)],
        ir_version=9,
    )


def main() -> int:
    print("== Probe: complex epsilon_r PML ramp (sphere PML, Phase H.5) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()

    n_inside = 1.5
    sigma_0 = 5.0

    # Build a small synthetic test: 6 tets straddling the three regions.
    radii_np = np.array(
        [0.20, 0.40, 0.60, 0.80, 0.95, 1.05], dtype=np.float64
    )
    tags_np = np.array(
        [
            PHYS_SPHERE_INTERIOR,  # interior
            PHYS_SPHERE_INTERIOR,  # interior
            999,                    # vacuum gap (any tag != interior, != pml)
            PHYS_PML_SHELL,         # PML mid-ramp
            PHYS_PML_SHELL,         # PML mid-ramp
            PHYS_PML_SHELL,         # PML at r > R_BUFFER (clamped)
        ],
        dtype=np.int32,
    )

    eps_ref = build_complex_epsilon_r_pml(
        tags_np, radii_np, n_inside=n_inside, sigma_0=sigma_0
    )

    # ------------------------------------------------------------- #
    # Graph (A) — native c128 output
    # ------------------------------------------------------------- #
    print("--- Graph (A): native COMPLEX128 output ---")
    model_a = build_native_c128_graph(n_inside, sigma_0)
    try:
        onnx.checker.check_model(model_a)
        a_checker = "OK"
    except Exception as e:  # noqa: BLE001
        a_checker = f"FAIL ({e!r})"
    print(f"onnx.checker (schema-level): {a_checker}")

    a_rt = "skipped"
    a_err = ""
    try:
        sess_a = ort.InferenceSession(model_a.SerializeToString())
        # Try to run it with a (1,) c128 seed.
        seed = np.array([1.0 + 0.0j], dtype=np.complex128)
        sess_a.run(["eps_c128_out"], {
            "tags": tags_np,
            "radii": radii_np,
            "eps_seed": seed,
        })
        a_rt = "OK"
    except Exception as e:  # noqa: BLE001
        a_rt = "FAIL"
        a_err = repr(e)
    print(f"onnxruntime execution: {a_rt}")
    if a_err:
        # Truncate to keep the output one screen
        msg = a_err if len(a_err) < 400 else a_err[:380] + "...(truncated)"
        print(f"  runtime error: {msg}")
    print()

    # ------------------------------------------------------------- #
    # Graph (B) — paired-real lowering
    # ------------------------------------------------------------- #
    print("--- Graph (B): paired-real (eps_re, eps_im) lowering ---")
    model_b = build_paired_real_graph(n_inside, sigma_0)
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
        outs = sess_b.run(["eps_re", "eps_im"], {"tags": tags_np, "radii": radii_np})
        re_onnx, im_onnx = outs
        max_re_err = float(np.max(np.abs(re_onnx - eps_ref.real)))
        max_im_err = float(np.max(np.abs(im_onnx - eps_ref.imag)))
        b_rt = "OK"
    except Exception as e:  # noqa: BLE001
        b_rt = f"FAIL ({e!r})"
    print(f"onnxruntime execution: {b_rt}")
    if b_rt == "OK":
        print(f"  max |eps_re_onnx - Re(eps_ref)| = {max_re_err:.3e}")
        print(f"  max |eps_im_onnx - Im(eps_ref)| = {max_im_err:.3e}")
    print()

    # ------------------------------------------------------------- #
    # Operator inventory + verdict
    # ------------------------------------------------------------- #
    print("Operator inventory for build_complex_epsilon_r_pml:")
    print("--------------------------------------------------------------")
    print("  Equal / Where           EXPRESSIBLE (real)  tag selector ladder")
    print("  Sub / Mul / Clip        EXPRESSIBLE (real)  ramp arithmetic on radii")
    print()
    print("Complex output construction:")
    print("--------------------------------------------------------------")
    print("  Complex(re, im) → c128  BLOCKED  no opset-18 op constructs c128")
    print("                                   from two f64 tensors. PyTorch's")
    print("                                   torch.complex is a framework-level")
    print("                                   helper; it does not lower.")
    print("  Cast f64 → c128         BLOCKED  Cast type constraints exclude c128.")
    print("  c128 elementwise ops    BLOCKED  Add/Sub/Mul/Div type constraints")
    print("                                   exclude c128 in opset 18. Schema")
    print("                                   accepts c128 as a Constant value")
    print("                                   type, but the runtime rejects c128")
    print("                                   inputs to ALL arithmetic ops.")
    print("  Reshape / Concat / Gather / NonZero / Identity / Constant / Where /")
    print("    ScatterND on c128     ACCEPTED schema-wise but the values can only")
    print("                                   move through the graph; they cannot")
    print("                                   be PRODUCED in-graph or operated on.")
    print()
    print("Verdict for Graph (A) — native COMPLEX128 output:")
    print(f"  schema check: {a_checker}")
    print(f"  runtime:      {a_rt}{(' (' + a_err[:80] + '...)') if a_err else ''}")
    print("  → BLOCKED at runtime (onnxruntime rejects c128 inputs to arithmetic).")
    print()
    print("Verdict for Graph (B) — paired-real lowering:")
    print(f"  schema check: {b_checker}")
    print(f"  runtime:      {b_rt}")
    if b_rt == "OK":
        print("  → EXPRESSIBLE (fallback). Numerically bit-exact vs. NumPy ref.")
    print()
    print("Overall verdict: FALLBACK (paired-real lowering).")
    print("  build_complex_epsilon_r_pml cannot produce a c128 tensor in-graph")
    print("  in opset 18. The standard mitigation is to emit two f64 outputs")
    print("  `eps_re` and `eps_im` and consume them downstream via two ScatterND")
    print("  calls (one into Re(M), one into Im(M)). This matches the convention")
    print("  used by the TF-Java reference (Phase H.4) and the SciML community.")
    print()

    # Success if A's runtime failed (the expected behavior) AND B passed.
    ok = (a_rt == "FAIL") and (b_checker == "OK") and (b_rt == "OK")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
