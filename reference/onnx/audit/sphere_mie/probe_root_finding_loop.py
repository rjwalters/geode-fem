"""Probe: ONNX expressibility of the analytic Mie root finder.

Epic #88, Phase J.6 (issue #175). The Mie root catalogue
(``reference/numpy/mie_roots.py``, mirror of
``crates/geode-core/src/mie.rs::find_roots``) is the epic's first
**iteration-shaped** primitive to hit the graph-only constraint. This
probe audits each sub-stage separately:

  (a) **Characteristic function evaluation** — spherical Bessel
      ladders (closed forms for l ≤ 1; upward recurrence for
      l ≤ x+1; Miller's downward recurrence for l > x+1) feeding the
      TE/TM characteristic functions. All loops are FIXED-count at
      graph-build time (l is a compile-time constant), so they unroll.
      Probed at l = 1 (the in-graph pipeline below) and at l = 4 (the
      catalogue's hardest case: 3-step upward ladder + 24-step
      unrolled Miller recurrence + per-element Where regime select).

  (b) **Grid scan + sign-change detection** — the 30 000-interval
      dense sampling produces a FIXED-shape (30000,) bracket mask:
      Slice + Mul + LessOrEqual + Abs + Min + IsNaN/IsInf + And.

  (c) **Bracket extraction / dedup / compaction** — the
      data-dependent-output-count stage. The 1e-5 consecutive dedup is
      a SEQUENTIAL scan (each decision depends on the previously
      *retained* root) — expressible only as a 30 000-trip ONNX
      ``Loop`` carrying the last-retained scalar (probed; works, but
      "tortured"). The final compaction to a dense root list is
      ``NonZero`` + ``Gather`` — runs under onnxruntime but with a
      data-dependent output shape, the same friction class as the G.6
      ``np.unique`` finding.

  (d) **Bisection refinement** — the Rust side runs 60 bisection
      steps per bracket with early exit. Probed in BOTH graph forms,
      vectorized over all 30 000 candidate intervals:
        - **unrolled**: 60 copies of the step body with Where-based
          convergence/activity masking;
        - **ONNX Loop**: trip-count 60 + a scalar all-lanes-converged
          continue-condition — i.e. a real `iterate-while-with-prev`.
      The two forms must agree BIT-EXACTLY (same op sequence), and
      both are validated against the brentq-refined NumPy reference
      roots and the Phase J.1 catalogue fixture
      (``reference/fixtures/mie_roots/baseline.json``).

The probe never self-compares: every numerical check is against
``reference/numpy/mie_roots.py`` (scipy ``spherical_jn``/``spherical_yn``
+ ``brentq``), scipy directly, or the J.1 baseline fixture.

A one-node native f64 ``Cos`` control graph (mirror of graph (A) in
``probe_tensor_eps_ramp.py``) asserts the ``NOT_IMPLEMENTED``
session-create failure that motivates the ``Sin(x + π/2)`` fallback in
``emit_cos_f64``; the assertion is folded into the overall PASS so the
fallback row gets re-audited if a future onnxruntime adds the kernel.

Run
===

    python3 reference/onnx/audit/sphere_mie/probe_root_finding_loop.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import onnx
import onnx.checker
import onnx.helper as oh
import onnxruntime as ort
from onnx import TensorProto
from scipy.special import spherical_jn, spherical_yn

HERE = Path(__file__).resolve().parent
REFERENCE_ROOT = HERE.parent.parent.parent
sys.path.insert(0, str(REFERENCE_ROOT / "numpy"))

import mie_roots  # noqa: E402
from mie_roots import (  # noqa: E402
    DEDUP_TOL,
    K_MAX,
    K_MIN,
    N_SAMPLES,
    POLE_SCALE_REJECT,
    TE,
    _dedup_consecutive,
    characteristic_te,
    resonance_roots,
)

OPSET = 18
N_INSIDE = 1.5
R_S = 1.0
R_B = 2.0
BASELINE = REFERENCE_ROOT / "fixtures" / "mie_roots" / "baseline.json"

# Bisection constants — mirror mie.rs::find_roots.
BISECT_ITERS = 60
BISECT_RTOL = 1e-12
# Miller downward start offset — mirror mie.rs::spherical_j_pair.
MILLER_OFFSET = 20
MILLER_RESCALE = 1e100


# --------------------------------------------------------------------------- #
# Tiny emitter: appends nodes with auto-unique value names.
# --------------------------------------------------------------------------- #


class Emitter:
    def __init__(self, prefix: str = "v"):
        self.nodes: list[onnx.NodeProto] = []
        self._n = 0
        self._prefix = prefix
        self._const_cache: dict[tuple, str] = {}

    def name(self, tag: str) -> str:
        self._n += 1
        return f"{self._prefix}_{tag}_{self._n}"

    def op(self, op_type: str, inputs: list[str], tag: str | None = None, **attrs) -> str:
        out = self.name(tag or op_type.lower())
        self.nodes.append(oh.make_node(op_type, inputs, [out], **attrs))
        return out

    def const(self, arr: np.ndarray, tag: str = "c") -> str:
        arr = np.asarray(arr)
        key = (arr.dtype.str, arr.shape, arr.tobytes())
        if key in self._const_cache:
            return self._const_cache[key]
        dtype_map = {
            np.dtype("float64"): TensorProto.DOUBLE,
            np.dtype("int64"): TensorProto.INT64,
            np.dtype("bool"): TensorProto.BOOL,
        }
        out = self.name(tag)
        self.nodes.append(
            oh.make_node(
                "Constant",
                [],
                [out],
                value=oh.make_tensor(
                    out + "_value",
                    dtype_map[arr.dtype],
                    list(arr.shape),
                    arr.flatten().tolist(),
                ),
            )
        )
        self._const_cache[key] = out
        return out

    def f64(self, val: float) -> str:
        return self.const(np.array(float(val), dtype=np.float64), "f")


# --------------------------------------------------------------------------- #
# In-graph spherical Bessel / Riccati-Bessel ladders.
# --------------------------------------------------------------------------- #


def emit_cos_f64(g: Emitter, x: str) -> str:
    """cos(x) for an f64 tensor — via ``Sin(x + π/2)``.

    FINDING (onnxruntime 1.26.0): the ``Cos`` (and ``Tan``) CPU kernels
    are registered for float32 only; ``Sin`` has a float64 kernel. A
    native f64 ``Cos`` node fails at session-create with
    ``NOT_IMPLEMENTED : Could not find an implementation for Cos(7)``.
    The shift identity costs one rounded addition (|err| ≲ ε·x), far
    inside this probe's tolerances; the alternative (Cast→f32→Cos→f64)
    would destroy the 1e-10 root contract.
    """
    half_pi = g.f64(np.pi / 2.0)
    shifted = g.op("Add", [x, half_pi], "xshift")
    return g.op("Sin", [shifted], "cos")


def emit_bessel_l01(g: Emitter, x: str) -> dict[str, str]:
    """Closed forms j0, j1, y0, y1 (and 1/x) for an f64 tensor x > 0."""
    s = g.op("Sin", [x], "sin")
    c = emit_cos_f64(g, x)
    inv_x = g.op("Reciprocal", [x], "invx")
    j0 = g.op("Mul", [s, inv_x], "j0")
    s_x2 = g.op("Mul", [j0, inv_x], "sx2")
    c_x = g.op("Mul", [c, inv_x], "cx")
    j1 = g.op("Sub", [s_x2, c_x], "j1")
    y0 = g.op("Neg", [c_x], "y0")
    c_x2 = g.op("Mul", [c_x, inv_x], "cx2")
    neg_y1a = g.op("Add", [c_x2, j0], "ny1")  # c/x² + s/x
    y1 = g.op("Neg", [neg_y1a], "y1")
    return {"j0": j0, "j1": j1, "y0": y0, "y1": y1, "inv_x": inv_x}


def emit_upward_ladder(g: Emitter, inv_x: str, f0: str, f1: str, l: int) -> tuple[str, str]:
    """Unrolled upward recurrence f_k = (2k−1)/x · f_{k−1} − f_{k−2}.

    Returns (f_{l−1}, f_l). Fixed trip count (l − 1 steps): l is a
    graph-build-time constant, so this is pure unrolled arithmetic.
    """
    prev, curr = f0, f1
    for k in range(2, l + 1):
        coeff = g.f64(2 * k - 1)
        t1 = g.op("Mul", [coeff, inv_x], f"up{k}a")
        t2 = g.op("Mul", [t1, curr], f"up{k}b")
        nxt = g.op("Sub", [t2, prev], f"up{k}")
        prev, curr = curr, nxt
    return prev, curr


def emit_miller_j_pair(g: Emitter, x: str, inv_x: str, j0: str, l: int) -> tuple[str, str]:
    """Unrolled Miller downward recurrence for (j_{l−1}, j_l).

    Mirror of ``mie.rs::spherical_j_pair``'s downward branch:
    l_start = l + 20, seed (j_{l_start+1}, j_{l_start}) = (0, 1),
    recurse j_{k−1} = (2k+1)/x · j_k − j_{k+1}, normalise against the
    closed form j0 = sin(x)/x. The conditional rescale-at-1e100 is
    value-dependent but CONTROL-static: it lowers to an unconditional
    per-element ``Where`` factor applied to all live state each step.
    Total: (l + 20) unrolled steps — fixed at graph-build time.
    """
    l_start = l + MILLER_OFFSET
    one = g.f64(1.0)
    thresh = g.f64(MILLER_RESCALE)
    zero_t = g.op("Mul", [x, g.f64(0.0)], "mz")  # 0-tensor shaped like x
    one_t = g.op("Add", [zero_t, one], "mone")  # 1-tensor shaped like x
    j_higher = zero_t
    j_high = one_t
    at_target = one_t if l == l_start else None
    at_target_m1 = None
    at_zero = None
    for k in range(l_start, 0, -1):
        coeff = g.f64(2 * k + 1)
        t1 = g.op("Mul", [coeff, inv_x], f"mi{k}a")
        t2 = g.op("Mul", [t1, j_high], f"mi{k}b")
        j_low = g.op("Sub", [t2, j_higher], f"mi{k}")
        j_higher, j_high = j_high, j_low
        if k - 1 == l:
            at_target = j_high
        if k - 1 == l - 1:
            at_target_m1 = j_high
        if k - 1 == 0:
            at_zero = j_high
        # Conditional rescale → unconditional Where factor.
        abs_hi = g.op("Abs", [j_high], f"mr{k}a")
        abs_hr = g.op("Abs", [j_higher], f"mr{k}b")
        scale = g.op("Max", [abs_hi, abs_hr], f"mr{k}c")
        big = g.op("Greater", [scale, thresh], f"mr{k}d")
        inv_scale = g.op("Reciprocal", [scale], f"mr{k}e")
        factor = g.op("Where", [big, inv_scale, one_t], f"mr{k}f")
        j_high = g.op("Mul", [j_high, factor], f"mr{k}g")
        j_higher = g.op("Mul", [j_higher, factor], f"mr{k}h")
        if at_target is not None:
            at_target = g.op("Mul", [at_target, factor], f"mr{k}i")
        if at_target_m1 is not None:
            at_target_m1 = g.op("Mul", [at_target_m1, factor], f"mr{k}j")
        if at_zero is not None:
            at_zero = g.op("Mul", [at_zero, factor], f"mr{k}k")
    norm = g.op("Div", [j0, at_zero], "mnorm")
    j_l = g.op("Mul", [at_target, norm], "mjl")
    j_lm1 = g.op("Mul", [at_target_m1, norm], "mjlm1")
    return j_lm1, j_l


def emit_bessel_pair(g: Emitter, x: str, l: int) -> dict[str, str]:
    """(j_{l−1}, j_l, y_{l−1}, y_l, 1/x) for an f64 tensor x > 0.

    l = 1: closed forms. l ≥ 2: y via upward ladder (always stable);
    j via per-element ``Where`` select between the upward ladder
    (stable for l ≤ x+1) and the unrolled Miller downward recurrence
    (l > x+1) — mirror of ``mie.rs::spherical_j_pair``'s regime split.
    """
    base = emit_bessel_l01(g, x)
    if l == 1:
        return {
            "j_lm1": base["j0"],
            "j_l": base["j1"],
            "y_lm1": base["y0"],
            "y_l": base["y1"],
            "inv_x": base["inv_x"],
        }
    inv_x = base["inv_x"]
    j_lm1_up, j_l_up = emit_upward_ladder(g, inv_x, base["j0"], base["j1"], l)
    y_lm1, y_l = emit_upward_ladder(g, inv_x, base["y0"], base["y1"], l)
    j_lm1_mi, j_l_mi = emit_miller_j_pair(g, x, inv_x, base["j0"], l)
    # Upward regime when l ≤ x + 1, i.e. x ≥ l − 1.
    boundary = g.f64(float(l - 1))
    use_up = g.op("GreaterOrEqual", [x, boundary], "useup")
    j_lm1 = g.op("Where", [use_up, j_lm1_up, j_lm1_mi], "jlm1")
    j_l = g.op("Where", [use_up, j_l_up, j_l_mi], "jl")
    return {"j_lm1": j_lm1, "j_l": j_l, "y_lm1": y_lm1, "y_l": y_l, "inv_x": inv_x}


def emit_riccati(g: Emitter, x: str, l: int) -> dict[str, str]:
    """ψ_l, ψ_l′, χ_l, χ_l′ at an f64 tensor x > 0 (Bohren-Huffman).

    ψ = x·j_l, ψ′ = j_l + x·j_l′ with j_l′ = j_{l−1} − (l+1)/x · j_l;
    χ = −x·y_l, χ′ = −y_l − x·y_l′ with y_l′ = y_{l−1} − (l+1)/x · y_l.
    """
    b = emit_bessel_pair(g, x, l)
    lp1 = g.f64(float(l + 1))
    lp1_x = g.op("Mul", [lp1, b["inv_x"]], "lp1x")

    psi = g.op("Mul", [x, b["j_l"]], "psi")
    jp_a = g.op("Mul", [lp1_x, b["j_l"]], "jpa")
    jp = g.op("Sub", [b["j_lm1"], jp_a], "jp")
    xjp = g.op("Mul", [x, jp], "xjp")
    psip = g.op("Add", [b["j_l"], xjp], "psip")

    xyl = g.op("Mul", [x, b["y_l"]], "xyl")
    chi = g.op("Neg", [xyl], "chi")
    yp_a = g.op("Mul", [lp1_x, b["y_l"]], "ypa")
    yp = g.op("Sub", [b["y_lm1"], yp_a], "yp")
    xyp = g.op("Mul", [x, yp], "xyp")
    neg_chip = g.op("Add", [b["y_l"], xyp], "nchip")
    chip = g.op("Neg", [neg_chip], "chip")
    return {"psi": psi, "psip": psip, "chi": chi, "chip": chip}


def emit_characteristic_te(g: Emitter, k: str, l: int) -> str:
    """TE characteristic function of mie_roots.characteristic_te, in-graph."""
    x_in = g.op("Mul", [k, g.f64(N_INSIDE * R_S)], "xin")
    x_s = g.op("Mul", [k, g.f64(R_S)], "xs")
    x_b = g.op("Mul", [k, g.f64(R_B)], "xb")

    rb = emit_riccati(g, x_b, l)
    rs = emit_riccati(g, x_s, l)
    ri = emit_riccati(g, x_in, l)

    big_a, big_b = rb["chi"], rb["psi"]
    t1 = g.op("Mul", [big_a, rs["psi"]], "bufa")
    t2 = g.op("Mul", [big_b, rs["chi"]], "bufb")
    buf = g.op("Sub", [t1, t2], "buf")
    t3 = g.op("Mul", [big_a, rs["psip"]], "bufpa")
    t4 = g.op("Mul", [big_b, rs["chip"]], "bufpb")
    bufp = g.op("Sub", [t3, t4], "bufp")

    lhs = g.op("Mul", [ri["psi"], bufp], "lhs")
    psip_n = g.op("Mul", [ri["psip"], g.f64(1.0 / N_INSIDE)], "psipn")
    rhs = g.op("Mul", [psip_n, buf], "rhs")
    return g.op("Sub", [lhs, rhs], "char_te")


# --------------------------------------------------------------------------- #
# In-graph bracket mask + bisection step (shared by unrolled and Loop forms).
# --------------------------------------------------------------------------- #


def emit_finite(g: Emitter, t: str) -> str:
    isnan = g.op("IsNaN", [t], "isnan")
    isinf = g.op("IsInf", [t], "isinf")
    bad = g.op("Or", [isnan, isinf], "bad")
    return g.op("Not", [bad], "fin")


def emit_bracket_mask(g: Emitter, fa: str, fb: str) -> str:
    """Mirror of the bracket-acceptance conditions in find_roots."""
    fin = g.op("And", [emit_finite(g, fa), emit_finite(g, fb)], "finab")
    zero = g.f64(0.0)
    eq_a = g.op("Equal", [fa, zero], "eqa")
    eq_b = g.op("Equal", [fb, zero], "eqb")
    both_zero = g.op("And", [eq_a, eq_b], "bz")
    prod = g.op("Mul", [fa, fb], "fafb")
    sign_ok = g.op("LessOrEqual", [prod, zero], "signok")
    abs_a = g.op("Abs", [fa], "absa")
    abs_b = g.op("Abs", [fb], "absb")
    min_ab = g.op("Min", [abs_a, abs_b], "minab")
    pole = g.op("Greater", [min_ab, g.f64(POLE_SCALE_REJECT)], "pole")
    keep = g.op("And", [fin, sign_ok], "k1")
    keep = g.op("And", [keep, g.op("Not", [both_zero], "nbz")], "k2")
    return g.op("And", [keep, g.op("Not", [pole], "npole")], "mask")


def emit_bisection_step(
    g: Emitter, lo: str, hi: str, f_lo: str, active: str, l: int
) -> tuple[str, str, str, str]:
    """One masked bisection step — mirror of the mie.rs inner loop.

    Rust control flow per bracket:
        mid = (lo+hi)/2; f_mid = f(mid)
        if !finite(f_mid): break
        if f_mid == 0 or hi−lo < 1e-12·max(|mid|,1): lo = hi = mid; break
        if f_lo·f_mid < 0: hi = mid else: lo = mid; f_lo = f_mid

    `break` lowers to clearing the per-lane `active` mask; the two
    update branches lower to Where selects gated on `active`.
    """
    half = g.f64(0.5)
    s = g.op("Add", [lo, hi], "sum")
    mid = g.op("Mul", [s, half], "mid")
    f_mid = emit_characteristic_te(g, mid, l)

    fin = emit_finite(g, f_mid)
    zero = g.f64(0.0)
    eq0 = g.op("Equal", [f_mid, zero], "eq0")
    width = g.op("Sub", [hi, lo], "width")
    abs_mid = g.op("Abs", [mid], "absmid")
    floor1 = g.op("Max", [abs_mid, g.f64(1.0)], "floor")
    tol = g.op("Mul", [floor1, g.f64(BISECT_RTOL)], "tol")
    small = g.op("Less", [width, tol], "small")
    conv = g.op("And", [fin, g.op("Or", [eq0, small], "convraw")], "conv")

    prod = g.op("Mul", [f_lo, f_mid], "flofm")
    sign_neg = g.op("Less", [prod, zero], "sneg")
    step_ok = g.op("And", [fin, g.op("Not", [conv], "nconv")], "stepok")
    gate = g.op("And", [active, step_ok], "gate")
    upd_hi = g.op("And", [gate, sign_neg], "updhi")
    upd_lo = g.op("And", [gate, g.op("Not", [sign_neg], "nsneg")], "updlo")
    take_conv = g.op("And", [active, conv], "takeconv")

    lo1 = g.op("Where", [upd_lo, mid, lo], "lo1")
    lo2 = g.op("Where", [take_conv, mid, lo1], "lo2")
    hi1 = g.op("Where", [upd_hi, mid, hi], "hi1")
    hi2 = g.op("Where", [take_conv, mid, hi1], "hi2")
    f_lo2 = g.op("Where", [upd_lo, f_mid, f_lo], "flo2")
    active2 = g.op("And", [active, step_ok], "act2")
    return lo2, hi2, f_lo2, active2


# --------------------------------------------------------------------------- #
# Graph builders.
# --------------------------------------------------------------------------- #


def make_model(g: Emitter, name: str, inputs, outputs) -> onnx.ModelProto:
    graph = oh.make_graph(g.nodes, name, inputs, outputs)
    return oh.make_model(
        graph, opset_imports=[oh.make_opsetid("", OPSET)], ir_version=9
    )


def build_native_cos_f64_graph() -> onnx.ModelProto:
    """One-node native f64 ``Cos`` — the expected-failure control for
    the ``emit_cos_f64`` fallback (mirror of graph (A) in
    ``probe_tensor_eps_ramp.py``). Session creation must FAIL with
    ``NOT_IMPLEMENTED`` under onnxruntime 1.26.0. If a future runtime
    registers the f64 kernel, this control makes the probe exit
    nonzero so the ``Sin(x + π/2)`` fallback row gets re-audited.
    """
    graph = oh.make_graph(
        [oh.make_node("Cos", ["x"], ["y"])],
        "native_cos_f64_control",
        [oh.make_tensor_value_info("x", TensorProto.DOUBLE, ["N"])],
        [oh.make_tensor_value_info("y", TensorProto.DOUBLE, ["N"])],
    )
    return oh.make_model(
        graph, opset_imports=[oh.make_opsetid("", OPSET)], ir_version=9
    )


def build_characteristic_graph(l: int) -> onnx.ModelProto:
    g = Emitter()
    f = emit_characteristic_te(g, "ks", l)
    g.nodes.append(oh.make_node("Identity", [f], ["f_out"]))
    return make_model(
        g,
        f"char_te_l{l}",
        [oh.make_tensor_value_info("ks", TensorProto.DOUBLE, ["G"])],
        [oh.make_tensor_value_info("f_out", TensorProto.DOUBLE, ["G"])],
    )


def build_bessel4_graph() -> onnx.ModelProto:
    g = Emitter()
    b = emit_bessel_pair(g, "xs", 4)
    for src, out in ((b["j_l"], "j4"), (b["j_lm1"], "j3"), (b["y_l"], "y4"), (b["y_lm1"], "y3")):
        g.nodes.append(oh.make_node("Identity", [src], [out]))
    return make_model(
        g,
        "bessel_l4",
        [oh.make_tensor_value_info("xs", TensorProto.DOUBLE, ["G"])],
        [oh.make_tensor_value_info(n, TensorProto.DOUBLE, ["G"]) for n in ("j3", "j4", "y3", "y4")],
    )


def build_pipeline_unrolled_graph(l: int, n_samples: int) -> onnx.ModelProto:
    """ks (n_samples+1,) → bracket mask (n,), refined roots (n,) — the
    bisection unrolled 60× with Where masking (form d-1), plus the
    sequential dedup Loop (stage c) and NonZero compaction."""
    g = Emitter()
    fs = emit_characteristic_te(g, "ks", l)

    n = n_samples
    starts0 = g.const(np.array([0], dtype=np.int64), "s0")
    starts1 = g.const(np.array([1], dtype=np.int64), "s1")
    ends_n = g.const(np.array([n], dtype=np.int64), "en")
    ends_n1 = g.const(np.array([n + 1], dtype=np.int64), "en1")
    ax0 = g.const(np.array([0], dtype=np.int64), "ax0")
    fa = g.op("Slice", [fs, starts0, ends_n, ax0], "fa")
    fb = g.op("Slice", [fs, starts1, ends_n1, ax0], "fb")
    ka = g.op("Slice", ["ks", starts0, ends_n, ax0], "ka")
    kb = g.op("Slice", ["ks", starts1, ends_n1, ax0], "kb")

    mask = emit_bracket_mask(g, fa, fb)

    lo, hi, f_lo, active = ka, kb, fa, mask
    for _ in range(BISECT_ITERS):
        lo, hi, f_lo, active = emit_bisection_step(g, lo, hi, f_lo, active, l)
    s = g.op("Add", [lo, hi], "rsum")
    roots = g.op("Mul", [s, g.f64(0.5)], "roots")

    # ---- Stage (c): sequential dedup scan as a 30 000-trip Loop ---- #
    # Carried state: last retained root (scalar). Scan output: keep_i.
    # The body reads root[i] / mask[i] from the OUTER scope via Gather
    # on the iteration counter — an implicit-capture subgraph.
    body = Emitter("dd")
    it_vi = oh.make_tensor_value_info("dd_iter", TensorProto.INT64, [])
    cond_vi = oh.make_tensor_value_info("dd_cond_in", TensorProto.BOOL, [])
    last_vi = oh.make_tensor_value_info("dd_last_in", TensorProto.DOUBLE, [])
    r_i = body.op("Gather", [roots, "dd_iter"], "ri", axis=0)
    m_i = body.op("Gather", [mask, "dd_iter"], "mi", axis=0)
    diff = body.op("Sub", [r_i, "dd_last_in"], "diff")
    adiff = body.op("Abs", [diff], "adiff")
    far = body.op("GreaterOrEqual", [adiff, body.f64(DEDUP_TOL)], "far")
    keep = body.op("And", [m_i, far], "keep")
    last_out = body.op("Where", [keep, r_i, "dd_last_in"], "lastout")
    body.nodes.append(oh.make_node("Identity", ["dd_cond_in"], ["dd_cond_out"]))
    body_graph = oh.make_graph(
        body.nodes,
        "dedup_body",
        [it_vi, cond_vi, last_vi],
        [
            oh.make_tensor_value_info("dd_cond_out", TensorProto.BOOL, []),
            oh.make_tensor_value_info(last_out, TensorProto.DOUBLE, []),
            oh.make_tensor_value_info(keep, TensorProto.BOOL, []),
        ],
    )
    trip = g.const(np.array(n, dtype=np.int64), "trip")
    cond_true = g.const(np.array(True, dtype=bool), "ct")
    last_init = g.f64(-1e300)
    loop_node = oh.make_node(
        "Loop",
        [trip, cond_true, last_init],
        ["dedup_last_final", "keep_mask"],
        body=body_graph,
    )
    g.nodes.append(loop_node)

    # ---- Compaction: NonZero + Gather (data-dependent shape) ---- #
    nz = g.op("NonZero", ["keep_mask"], "nz")  # (1, nnz) int64
    idx = g.op("Squeeze", [nz, ax0], "idx")  # (nnz,)
    g.nodes.append(oh.make_node("Gather", [roots, idx], ["roots_dedup"], axis=0))

    for src, out in ((mask, "mask_out"), (roots, "roots_out"), ("keep_mask", "keep_out")):
        g.nodes.append(oh.make_node("Identity", [src], [out]))

    return make_model(
        g,
        f"find_roots_te_l{l}_unrolled",
        [oh.make_tensor_value_info("ks", TensorProto.DOUBLE, [n + 1])],
        [
            oh.make_tensor_value_info("mask_out", TensorProto.BOOL, [n]),
            oh.make_tensor_value_info("roots_out", TensorProto.DOUBLE, [n]),
            oh.make_tensor_value_info("keep_out", TensorProto.BOOL, [n]),
            oh.make_tensor_value_info("roots_dedup", TensorProto.DOUBLE, ["NNZ"]),
        ],
    )


def build_pipeline_loop_graph(l: int, n_samples: int) -> onnx.ModelProto:
    """Same pipeline through the bracket mask, but the 60-step bisection
    is an ONNX ``Loop`` (form d-2): trip-count 60, loop-carried
    (lo, hi, f_lo, active), and a scalar continue-condition
    `any lane still active` — i.e. iterate-while-with-prev."""
    g = Emitter("o")
    fs = emit_characteristic_te(g, "ks", l)

    n = n_samples
    starts0 = g.const(np.array([0], dtype=np.int64), "s0")
    starts1 = g.const(np.array([1], dtype=np.int64), "s1")
    ends_n = g.const(np.array([n], dtype=np.int64), "en")
    ends_n1 = g.const(np.array([n + 1], dtype=np.int64), "en1")
    ax0 = g.const(np.array([0], dtype=np.int64), "ax0")
    fa = g.op("Slice", [fs, starts0, ends_n, ax0], "fa")
    fb = g.op("Slice", [fs, starts1, ends_n1, ax0], "fb")
    ka = g.op("Slice", ["ks", starts0, ends_n, ax0], "ka")
    kb = g.op("Slice", ["ks", starts1, ends_n1, ax0], "kb")
    mask = emit_bracket_mask(g, fa, fb)

    # Loop body: one bisection step + all-lanes-converged condition.
    body = Emitter("bi")
    it_vi = oh.make_tensor_value_info("bi_iter", TensorProto.INT64, [])
    cond_vi = oh.make_tensor_value_info("bi_cond_in", TensorProto.BOOL, [])
    lo_vi = oh.make_tensor_value_info("bi_lo_in", TensorProto.DOUBLE, [n])
    hi_vi = oh.make_tensor_value_info("bi_hi_in", TensorProto.DOUBLE, [n])
    flo_vi = oh.make_tensor_value_info("bi_flo_in", TensorProto.DOUBLE, [n])
    act_vi = oh.make_tensor_value_info("bi_act_in", TensorProto.BOOL, [n])
    lo2, hi2, flo2, act2 = emit_bisection_step(
        body, "bi_lo_in", "bi_hi_in", "bi_flo_in", "bi_act_in", l
    )
    # Continue while any lane is still active (early exit when all
    # lanes have converged) — the iterate-while condition is a SCALAR.
    act_i32 = body.op("Cast", [act2], "acti", to=TensorProto.INT32)
    any_active_i = body.op("ReduceMax", [act_i32], "anyi", keepdims=0)
    cond_out = body.op("Cast", [any_active_i], "condout", to=TensorProto.BOOL)
    body_graph = oh.make_graph(
        body.nodes,
        "bisect_body",
        [it_vi, cond_vi, lo_vi, hi_vi, flo_vi, act_vi],
        [
            oh.make_tensor_value_info(cond_out, TensorProto.BOOL, []),
            oh.make_tensor_value_info(lo2, TensorProto.DOUBLE, [n]),
            oh.make_tensor_value_info(hi2, TensorProto.DOUBLE, [n]),
            oh.make_tensor_value_info(flo2, TensorProto.DOUBLE, [n]),
            oh.make_tensor_value_info(act2, TensorProto.BOOL, [n]),
        ],
    )
    trip = g.const(np.array(BISECT_ITERS, dtype=np.int64), "trip")
    cond_true = g.const(np.array(True, dtype=bool), "ct")
    g.nodes.append(
        oh.make_node(
            "Loop",
            [trip, cond_true, ka, kb, fa, mask],
            ["lo_fin", "hi_fin", "flo_fin", "act_fin"],
            body=body_graph,
        )
    )
    s = g.op("Add", ["lo_fin", "hi_fin"], "rsum")
    roots = g.op("Mul", [s, g.f64(0.5)], "roots")
    g.nodes.append(oh.make_node("Identity", [mask], ["mask_out"]))
    g.nodes.append(oh.make_node("Identity", [roots], ["roots_out"]))

    return make_model(
        g,
        f"find_roots_te_l{l}_loop",
        [oh.make_tensor_value_info("ks", TensorProto.DOUBLE, [n + 1])],
        [
            oh.make_tensor_value_info("mask_out", TensorProto.BOOL, [n]),
            oh.make_tensor_value_info("roots_out", TensorProto.DOUBLE, [n]),
        ],
    )


def build_dedup_only_graph() -> onnx.ModelProto:
    """Standalone dedup Loop over caller-provided (vals, mask) — used
    for the synthetic near-duplicate check against _dedup_consecutive."""
    g = Emitter("sd")
    shape = g.op("Shape", ["vals"], "shape")
    trip0 = g.op("Gather", [shape, g.const(np.array(0, dtype=np.int64), "i0")], "n")
    body = Emitter("sdb")
    it_vi = oh.make_tensor_value_info("sdb_iter", TensorProto.INT64, [])
    cond_vi = oh.make_tensor_value_info("sdb_cond_in", TensorProto.BOOL, [])
    last_vi = oh.make_tensor_value_info("sdb_last_in", TensorProto.DOUBLE, [])
    r_i = body.op("Gather", ["vals", "sdb_iter"], "ri", axis=0)
    m_i = body.op("Gather", ["mask", "sdb_iter"], "mi", axis=0)
    diff = body.op("Sub", [r_i, "sdb_last_in"], "diff")
    adiff = body.op("Abs", [diff], "adiff")
    far = body.op("GreaterOrEqual", [adiff, body.f64(DEDUP_TOL)], "far")
    keep = body.op("And", [m_i, far], "keep")
    last_out = body.op("Where", [keep, r_i, "sdb_last_in"], "lastout")
    body.nodes.append(oh.make_node("Identity", ["sdb_cond_in"], ["sdb_cond_out"]))
    body_graph = oh.make_graph(
        body.nodes,
        "dedup_only_body",
        [it_vi, cond_vi, last_vi],
        [
            oh.make_tensor_value_info("sdb_cond_out", TensorProto.BOOL, []),
            oh.make_tensor_value_info(last_out, TensorProto.DOUBLE, []),
            oh.make_tensor_value_info(keep, TensorProto.BOOL, []),
        ],
    )
    cond_true = g.const(np.array(True, dtype=bool), "ct")
    g.nodes.append(
        oh.make_node(
            "Loop",
            [trip0, cond_true, g.f64(-1e300)],
            ["sd_last_final", "keep_mask_out"],
            body=body_graph,
        )
    )
    return make_model(
        g,
        "dedup_scan",
        [
            oh.make_tensor_value_info("vals", TensorProto.DOUBLE, ["N"]),
            oh.make_tensor_value_info("mask", TensorProto.BOOL, ["N"]),
        ],
        [oh.make_tensor_value_info("keep_mask_out", TensorProto.BOOL, ["N"])],
    )


# --------------------------------------------------------------------------- #
# Reference helpers (NumPy/scipy side — never the graph).
# --------------------------------------------------------------------------- #


def reference_bracket_mask(fs: np.ndarray) -> np.ndarray:
    """Transliteration of the acceptance conditions in
    mie_roots.find_roots (the loop body's `continue` ladder)."""
    fa, fb = fs[:-1], fs[1:]
    finite = np.isfinite(fa) & np.isfinite(fb)
    both_zero = (fa == 0.0) & (fb == 0.0)
    sign_ok = fa * fb <= 0.0
    pole = np.minimum(np.abs(fa), np.abs(fb)) > POLE_SCALE_REJECT
    return finite & sign_ok & ~both_zero & ~pole


def run(model: onnx.ModelProto, outputs: list[str], feeds: dict):
    onnx.checker.check_model(model)
    sess = ort.InferenceSession(model.SerializeToString())
    return sess.run(outputs, feeds)


# --------------------------------------------------------------------------- #
# Driver
# --------------------------------------------------------------------------- #


def main() -> int:  # noqa: PLR0915
    print("== Probe: Mie root finding in-graph (Phase J.6, TE l=1 pipeline) ==")
    print(f"onnx={onnx.__version__}  onnxruntime={ort.__version__}  opset={OPSET}")
    print()
    ok = True

    dk = (K_MAX - K_MIN) / N_SAMPLES
    ks = (K_MIN + dk * np.arange(N_SAMPLES + 1)).astype(np.float64)

    # ------------------------------------------------------------- #
    # Control: native f64 Cos (expected failure — freshness signal)
    # ------------------------------------------------------------- #
    print("--- Control: native f64 Cos node (expected session-create failure) ---")
    model_cos = build_native_cos_f64_graph()
    onnx.checker.check_model(model_cos)  # schema-OK; the missing piece is the kernel
    cos_failed = False
    try:
        ort.InferenceSession(model_cos.SerializeToString())
    except Exception as e:  # noqa: BLE001
        cos_failed = True
        msg = repr(e)
        if len(msg) > 400:
            msg = msg[:380] + "...(truncated)"
        print(f"  session create: FAIL (expected) — {msg}")
    else:
        print("  session create: OK — onnxruntime now registers an f64 Cos kernel;")
        print("  the Stage (a) Sin(x+π/2) fallback row in the audit is STALE — re-audit.")
    ok &= cos_failed
    print(f"  emit_cos_f64 Sin(x+π/2) fallback still required: "
          f"{'PASS' if cos_failed else 'FAIL'}")
    print("  (f64 Tan/Atan kernels are likewise missing in 1.26.0, but this")
    print("   pipeline never needs them — no control graph required.)")
    print()

    # ------------------------------------------------------------- #
    # (a) characteristic function evaluation
    # ------------------------------------------------------------- #
    print("--- Stage (a): characteristic function evaluation in-graph ---")
    f_ref_l1 = characteristic_te(N_INSIDE, 1, R_S, R_B, ks)
    (f_onnx_l1,) = run(build_characteristic_graph(1), ["f_out"], {"ks": ks})
    scale_l1 = np.maximum(np.abs(f_ref_l1), 1.0)
    err_l1 = float(np.max(np.abs(f_onnx_l1 - f_ref_l1) / scale_l1))
    a1_ok = err_l1 < 1e-10
    ok &= a1_ok
    print(f"  l=1 closed forms vs mie_roots.characteristic_te over {len(ks)} pts:")
    print(f"    max scaled error = {err_l1:.3e}  ({'PASS' if a1_ok else 'FAIL'} < 1e-10)")

    # l=4 Bessel ladder (upward + unrolled 24-step Miller + Where select)
    # across BOTH regimes (x < 3 → Miller; x ≥ 3 → upward); x range
    # covers all three argument streams (x_b = 2k reaches 40).
    xs = np.concatenate(
        [np.linspace(0.1, 5.0, 2001), np.linspace(5.0, 40.0, 2001)]
    ).astype(np.float64)
    j3o, j4o, y3o, y4o = run(build_bessel4_graph(), ["j3", "j4", "y3", "y4"], {"xs": xs})

    def dfact(n: int) -> float:  # (2l+1)!! etc.
        out = 1.0
        while n > 1:
            out *= n
            n -= 2
        return out

    def env_j(l: int, x: np.ndarray) -> np.ndarray:
        # Amplitude envelope: x^l/(2l+1)!! near 0, 1/x in the
        # oscillatory regime. A relative error against max(|ref|, 0.1·env)
        # is a genuine relative test where the function is small-but-
        # well-conditioned (Miller regime) and avoids the meaningless
        # blow-up exactly AT the large-x crossing zeros.
        return np.minimum(x**l / dfact(2 * l + 1), 1.0 / x)

    def env_y(l: int, x: np.ndarray) -> np.ndarray:
        return np.maximum(dfact(2 * l - 1) / x ** (l + 1), 1.0 / x)

    checks = [
        ("j3", j3o, spherical_jn(3, xs), env_j(3, xs)),
        ("j4", j4o, spherical_jn(4, xs), env_j(4, xs)),
        ("y3", y3o, spherical_yn(3, xs), env_y(3, xs)),
        ("y4", y4o, spherical_yn(4, xs), env_y(4, xs)),
    ]
    a2_ok = True
    for label, got, ref, env in checks:
        scale = np.maximum(np.abs(ref), 0.1 * env)
        err = float(np.max(np.abs(got - ref) / scale))
        passed = err < 1e-9
        a2_ok &= passed
        print(f"  l=4 {label} vs scipy over x∈[0.1,40] ({len(xs)} pts): "
              f"max envelope-scaled rel err = {err:.3e}  ({'PASS' if passed else 'FAIL'} < 1e-9)")
    ok &= a2_ok

    # l=4 characteristic function end-to-end. Scaled by the magnitude
    # of the two cancelling terms (cancellation at roots makes a plain
    # relative error meaningless there).
    f_ref_l4 = characteristic_te(N_INSIDE, 4, R_S, R_B, ks)
    (f_onnx_l4,) = run(build_characteristic_graph(4), ["f_out"], {"ks": ks})
    psi_in = mie_roots.psi(4, N_INSIDE * ks)
    psip_in = mie_roots.psi_prime(4, N_INSIDE * ks)
    big_a, big_b = mie_roots.chi(4, R_B * ks), mie_roots.psi(4, R_B * ks)
    buf = big_a * mie_roots.psi(4, ks) - big_b * mie_roots.chi(4, ks)
    bufp = big_a * mie_roots.psi_prime(4, ks) - big_b * mie_roots.chi_prime(4, ks)
    term_scale = np.maximum(np.abs(psi_in * bufp) + np.abs(psip_in / N_INSIDE * buf), 1.0)
    err_l4 = float(np.max(np.abs(f_onnx_l4 - f_ref_l4) / term_scale))
    a3_ok = err_l4 < 1e-9
    ok &= a3_ok
    print(f"  l=4 characteristic_te vs reference over the 30001-pt grid:")
    print(f"    max term-scaled error = {err_l4:.3e}  ({'PASS' if a3_ok else 'FAIL'} < 1e-9)")
    print()

    # ------------------------------------------------------------- #
    # Full pipeline graphs (stages b, c, d) — TE l=1.
    # ------------------------------------------------------------- #
    print("--- Stages (b)+(c)+(d): full find_roots pipeline in-graph (TE l=1) ---")
    model_unrolled = build_pipeline_unrolled_graph(1, N_SAMPLES)
    n_nodes_unrolled = len(model_unrolled.graph.node)
    print(f"  unrolled-form graph: {n_nodes_unrolled} top-level nodes "
          f"(60 bisection steps inlined)")
    mask_u, roots_u, keep_u, dedup_u = run(
        model_unrolled,
        ["mask_out", "roots_out", "keep_out", "roots_dedup"],
        {"ks": ks},
    )

    model_loop = build_pipeline_loop_graph(1, N_SAMPLES)
    n_nodes_loop = len(model_loop.graph.node)
    print(f"  Loop-form graph: {n_nodes_loop} top-level nodes "
          f"(bisection = ONNX Loop, trip-count 60 + all-converged early exit)")
    mask_l, roots_l = run(model_loop, ["mask_out", "roots_out"], {"ks": ks})

    # (b) bracket mask vs the find_roots acceptance conditions.
    fs_ref = characteristic_te(N_INSIDE, 1, R_S, R_B, ks)
    mask_ref = reference_bracket_mask(np.asarray(fs_ref))
    b_ok = bool(np.array_equal(mask_u, mask_ref)) and bool(np.array_equal(mask_l, mask_ref))
    ok &= b_ok
    print(f"  (b) bracket mask: {int(mask_u.sum())} brackets; "
          f"matches reference conditions exactly: {b_ok}")

    # (d) the two bisection forms must agree bit-exactly on bracketed lanes.
    bit_same = bool(np.array_equal(roots_u[mask_ref], roots_l[mask_ref]))
    ok &= bit_same
    print(f"  (d) unrolled vs Loop forms bit-identical on bracketed lanes: {bit_same}")

    # (d) refined roots vs the brentq-refined NumPy reference.
    ref_roots = np.array(
        [r.k for r in resonance_roots(TE, N_INSIDE, 1, R_S, R_B, n_max=10**9)]
    )
    graph_roots = roots_u[mask_u]
    graph_roots_dedup = _dedup_consecutive(list(graph_roots), DEDUP_TOL)
    d_ok = len(graph_roots_dedup) == len(ref_roots)
    rel = float(np.max(np.abs(np.array(graph_roots_dedup) - ref_roots) / ref_roots)) if d_ok else float("nan")
    d_ok = d_ok and rel < 1e-10
    ok &= d_ok
    print(f"  (d) {len(graph_roots_dedup)} in-graph roots vs {len(ref_roots)} brentq "
          f"reference roots: max rel err = {rel:.3e}  ({'PASS' if d_ok else 'FAIL'} < 1e-10)")

    # (d) vs the J.1 catalogue fixture (first 5 TE l=1 roots).
    with open(BASELINE) as fh:
        base = json.load(fh)
    outs = base["outputs"]
    sel = [
        i
        for i in range(len(outs["root_k"]["data"]))
        if outs["root_pol"]["data"][i] == 0 and outs["root_l"]["data"][i] == 1
    ]
    base_k = np.array([outs["root_k"]["data"][i] for i in sel])
    tol_abs = float(outs["root_k"]["tolerance_abs"])
    diffs = np.abs(np.array(graph_roots_dedup[: len(base_k)]) - base_k)
    base_ok = bool(np.all(diffs <= tol_abs))
    ok &= base_ok
    print(f"  (d) first {len(base_k)} roots vs J.1 baseline.json (tol_abs {tol_abs:g}): "
          f"max abs diff = {float(np.max(diffs)):.3e}  ({'PASS' if base_ok else 'FAIL'})")

    # (c) in-graph dedup Loop + NonZero compaction vs reference.
    c_count_ok = len(dedup_u) == len(ref_roots)
    c_rel = float(np.max(np.abs(dedup_u - ref_roots) / ref_roots)) if c_count_ok else float("nan")
    c_ok = c_count_ok and c_rel < 1e-10
    keep_matches_host = bool(
        np.array_equal(
            roots_u[keep_u],
            np.array(_dedup_consecutive(list(roots_u[mask_u]), DEDUP_TOL)),
        )
    )
    ok &= c_ok and keep_matches_host
    print(f"  (c) in-graph dedup Loop + NonZero compaction: {len(dedup_u)} roots, "
          f"max rel err vs reference = {c_rel:.3e}  ({'PASS' if c_ok else 'FAIL'})")
    print(f"      keep-mask matches host _dedup_consecutive semantics: {keep_matches_host}")

    # (c) synthetic near-duplicate dedup check (TE l=1 has no natural
    # duplicates, so exercise the tolerance logic explicitly).
    vals = np.array(
        [1.0, 1.0 + 0.4 * DEDUP_TOL, 1.0 + 1.7 * DEDUP_TOL, 2.0, 2.0, 3.0, 3.0 + 0.9 * DEDUP_TOL],
        dtype=np.float64,
    )
    msk = np.array([True, True, True, True, False, True, True])
    (keep_syn,) = run(build_dedup_only_graph(), ["keep_mask_out"], {"vals": vals, "mask": msk})
    ref_dedup = _dedup_consecutive(list(vals[msk]), DEDUP_TOL)
    got_dedup = list(vals[keep_syn])
    syn_ok = got_dedup == ref_dedup
    ok &= syn_ok
    print(f"  (c) synthetic near-duplicate dedup vs _dedup_consecutive: "
          f"{'PASS' if syn_ok else f'FAIL ({got_dedup} vs {ref_dedup})'}")
    print()

    # ------------------------------------------------------------- #
    # Verdicts
    # ------------------------------------------------------------- #
    print("Per-sub-stage verdicts:")
    print("--------------------------------------------------------------")
    print("  (a) characteristic evaluation   EMITTABLE — closed forms (l=1) and")
    print("      FIXED-count recurrences (upward ladder; 24-step Miller downward")
    print("      with Where-lowered rescale; Where regime select). l is a graph")
    print("      constant, so every Bessel loop unrolls. No Loop op needed.")
    print("  (b) grid scan + sign change     EMITTABLE — fixed-shape (30000,)")
    print("      mask via Slice/Mul/LessOrEqual/Min/Abs/IsNaN/IsInf/And.")
    print("  (c) bracket extraction + dedup  BLOCKED for static shapes —")
    print("      the dedup scan is sequential (carried last-retained value):")
    print("      expressible-but-tortured as a 30000-trip ONNX Loop; the final")
    print("      compaction is NonZero+Gather, which onnxruntime executes but")
    print("      with a DATA-DEPENDENT output shape (the np.unique friction")
    print("      class from G.6). Recommended: keep the dense (30000,) roots +")
    print("      keep-mask in-graph; compact at the host boundary.")
    print("  (d) bisection refinement        EMITTABLE, two ways —")
    print("      unrolled 60× with Where convergence masking, AND as an ONNX")
    print("      Loop (trip-count 60 + scalar all-lanes-converged condition).")
    print("      The forms agree bit-exactly; per-lane early exit lowers to")
    print("      masking, whole-loop early exit to the Loop condition.")
    print()
    print("Iterate-while finding (L4 spec implication):")
    print("  ONNX `Loop` IS an iterate-while-with-prev: loop-carried state,")
    print("  max trip count, scalar boolean continue-condition, and scan")
    print("  outputs. It survives the graph-only constraint PROVIDED:")
    print("    1. carried-state shapes are loop-invariant;")
    print("    2. the continue condition is a SCALAR reduction (per-lane")
    print("       convergence must lower to Where masking in the state);")
    print("    3. results with data-dependent COUNTS stay dense+mask in the")
    print("       graph and compact at the host boundary.")
    print()
    print(f"Overall: {'PASS' if ok else 'FAIL'}")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
