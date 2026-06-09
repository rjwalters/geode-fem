# Mie sphere ONNX expressibility audit (Phase J.6)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase J.6
deliverable. Tracking issue:
[#175](https://github.com/rjwalters/geode-fem/issues/175).

## Scope

This audit covers the two operator families the Mie slice adds on top
of the previously audited spines (F.1 cube-cavity, G.6 sphere-PEC,
H.5 sphere-PML, I.3 de Rham), and records whether each lowers to a
graph-only ONNX opset-18 form:

1. **Anisotropic UPML tensor-ε construction + complex-symmetric
   scatter** — as defined by the NumPy reference in
   [`reference/numpy/sphere_mie.py`](../../../numpy/sphere_mie.py)
   (`build_anisotropic_pml_tensor_diag`,
   `batched_nedelec_local_mass_anisotropic_diag`,
   `assemble_global_nedelec_anisotropic`; issue #171 / PR #179) and the
   Burn implementation in `crates/geode-core/src/nedelec.rs` /
   `pml.rs` (issue #54). Expected to inherit the H.5 c128 verdicts;
   the J.6 question is whether the **paired-real lowering composes
   cleanly with the diagonal-3×3-per-tet tensor structure** or forces
   extra reshapes/transposes.
2. **Analytic Mie root finding** — as defined by
   [`reference/numpy/mie_roots.py`](../../../numpy/mie_roots.py)
   (issue #170 / PR #177, mirror of `crates/geode-core/src/mie.rs`):
   spherical-Bessel characteristic functions, 30 000-interval dense
   grid scan on (0.1, 20.0], sign-change bracketing with 1e8 pole
   rejection, 1e-5 consecutive dedup, and 60-step bisection
   refinement. This is the epic's first **iteration-shaped** primitive
   to hit the graph-only constraint, audited sub-stage by sub-stage
   because it bears directly on the whiteroom L4 `iterate-while` /
   `iterate-while-with-prev` surface.

**Headline findings:**

- The tensor-ε pair-lowering **composes cleanly**: because the UPML
  stretch enters the mass linearly and the geometric factor is real,
  the Re/Im channels factor through one shared real per-axis tensor.
  Cost over the scalar H.5 lowering: two extra `Einsum` contractions
  and one extra `ScatterND` — **zero** extra reshapes or transposes.
  Bit-exact (`0.000e+00`) against the NumPy reference end-to-end.
- The **entire Mie root finder runs as a single ONNX graph** under
  onnxruntime — grid scan, bracket mask, 60-step bisection (probed
  both unrolled and as an ONNX `Loop`), the sequential dedup scan
  (a 30 000-trip `Loop`), and even the final compaction
  (`NonZero` + `Gather`) — reproducing the J.1 catalogue roots to
  ~5e-13 relative. But two of those stages only work because
  onnxruntime tolerates what a static-shape consumer cannot: the
  dedup is a *sequential* scan (expressible-but-tortured), and the
  compaction has a **data-dependent output shape** (the G.6
  `np.unique` friction class). The static-shape disposition is:
  dense roots + keep-mask in-graph, compaction at the host boundary.
- **ONNX `Loop` is a working `iterate-while-with-prev`** — see the
  spec-implications section.
- **New kernel-coverage finding:** onnxruntime 1.26.0 registers a
  float64 CPU kernel for `Sin` but **not** for `Cos` (nor `Tan`,
  `Atan`). An f64 `Cos` node fails at session-create with
  `NOT_IMPLEMENTED`. Workaround used here: `cos(x) = Sin(x + π/2)`
  (error ≲ ε·x, far inside the 1e-10 root contract; the alternative
  f32 round-trip would destroy it).

## Pinned versions used for this audit

Same as Phase F.1 / G.6 / H.5 / I.3:

| Package | Version |
|---|---|
| `onnx` | `1.21.0` |
| `onnxruntime` | `1.26.0` |
| Target opset | `18` |

See [`reference/onnx/requirements.txt`](../../requirements.txt) for the
pinned `pip` line. The probes additionally need `scipy` from
[`reference/numpy/requirements.txt`](../../../numpy/requirements.txt)
(the root-finding reference uses `scipy.special` + `brentq`).

A throwaway venv reproduces the environment:

```bash
uv venv /tmp/onnx-audit-175
uv pip install --python /tmp/onnx-audit-175/bin/python \
    onnx==1.21.0 onnxruntime==1.26.0 onnxscript==0.7.0 "numpy>=1.26" scipy
```

## Classification convention

Same as H.5 / I.3 (issue #157 convention):

| Marker | Meaning |
|---|---|
| **emittable** | Op lowers directly to opset 18 with no special handling. |
| **fallback** | Op cannot lower as-typed; a documented alternative lowering exists. |
| **blocked** | Op has no lowering — must be host-computed and passed as a graph input or executed in an out-of-graph sidecar. |

## Target 1 — anisotropic UPML tensor-ε + complex-symmetric scatter

Probe: [`probe_tensor_eps_ramp.py`](probe_tensor_eps_ramp.py).

### Stage T1 — per-tet centroids (`tet_centroids`)

No probe required: `mean(nodes[tets], axis=1)` is `Gather` +
`ReduceMean`, the vector form of the H.5 Stage 1 radius computation.

**Stage T1 verdict: EMITTABLE.**

### Stage T2 — tensor-ε ramp (`build_anisotropic_pml_tensor_diag`)

The new arithmetic over the scalar H.5 ramp is the complex UPML
stretch `s = 1 − jσ/ω` and its reciprocal, applied per Cartesian axis
through the radial direction `r̂`:

```text
ε_α = bg · (s⁻¹ r̂_α² + s (1 − r̂_α²)),   bg = n² inside / 1 elsewhere.
```

| Operator (NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `‖centroids‖` (keepdims) | `Mul` + `ReduceSum(axes=[1], keepdims=1)` + `Sqrt` | **emittable** | The `(n,1)` shape broadcasts against every `(n,3)` tensor below. |
| Tag selectors, shell gate `r_c > R_PML_INNER` | `Equal` + `Unsqueeze` + `Greater` + `And` | **emittable** | Bool `(n,1)`. |
| Ramp `u = clip(...)`, `σ = σ₀u²` | `Sub`/`Mul`/`Clip` | **emittable** | Real f64, same as H.5 Stage 2. |
| **Complex stretch `s`, `1/s`** | `Mul`/`Add`/`Reciprocal`/`Neg` | **emittable (as paired-real)** | `s_re ≡ 1`, `s_im = −σ/ω`; `1/s = conj(s)/\|s\|²` has a **real** denominator `1 + s_im²` — the complex reciprocal lowers to 4 real ops, no complex arithmetic ever needed. |
| Radial direction `r̂`, guard `r > 1e-12` | `Reciprocal` + `Where` + `Mul` | **emittable** | Defensive branch of the Burn kernel lowers to one `Where`. |
| Per-axis assembly `ε_α` | `Mul`/`Add` with `(n,1)`-vs-`(n,3)` broadcast; `Where` on the `(n,1)` shell mask | **emittable** | The tensor structure is **one broadcast axis** — multidirectional broadcasting in `Mul`/`Where` covers it. No `Reshape`/`Transpose` nodes appear anywhere in the graph. |
| `(re, im) → complex128` output | none | **blocked** | Inherited H.5 Stage 2 verdict (no `Complex(re, im)` op; `Cast` excludes c128). Emit `eps_re`, `eps_im`, shape `(n,3)` each. |

Probe result: paired-real graph passes `onnx.checker` and
onnxruntime; **bit-exact (`0.000e+00`)** for both channels vs.
`build_anisotropic_pml_tensor_diag` on an 8-tet fixture spanning
interior, vacuum gap, the `r ≤ R_PML_INNER` shell guard, axis-aligned
and oblique mid-ramp directions, and the clamped `u = 1` overshoot.
(Bit-exactness includes the complex reciprocal: NumPy's `1/s` reduces
to the same `conj/|s|²` real arithmetic when `s_re = 1`.)

**Stage T2 verdict: FALLBACK (paired-real lowering), composing
cleanly with the tensor structure.** Same c128 root cause as H.5; the
diagonal tensor adds a broadcast axis, not a new friction.

### Stage T3 — anisotropic local mass (`batched_nedelec_local_mass_anisotropic_diag`)

The kernel splits the scalar cofactor gram per axis and contracts
against `ε_diag`:

```text
gg_axis[e,α,p,q] = g_p[α] g_q[α]
M[e,i,j] = Σ_α ε_α · (f_ac gg^α_bd − f_ad gg^α_bc − f_bc gg^α_ad + f_bd gg^α_ac) / (120 |det|)
```

| Operator (NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| Edge vectors, cross products, `det` | `Gather`/`Sub`/`Mul`/`Concat`/`ReduceSum` | **emittable** | Same component-wise cross-product lowering as the G.6 K_local inventory (ONNX has no `Cross` op; 9 Mul + 3 Sub per cross). |
| Per-axis cofactor gram | `Einsum("epa,eqa->eapq")` | **emittable** | ONE Einsum. The per-axis split costs an output axis, not a transpose. |
| Edge-pair Kronecker ladder | `Einsum("eapq,pqij->eaij")` against a baked `Constant` `(4,4,6,6)` coefficient tensor | **emittable** | The four-term `f_ac/f_ad/f_bc/f_bd` combination is a *structural linear map* from the 16 gram entries to the 36 edge pairs — it bakes into one constant tensor at graph-generation time. |
| **ε-weighting, Re/Im channels** | `Einsum("eaij,ea->eij")` × 2 | **emittable (paired-real)** | The per-axis mass term is REAL and shared; `Re(ε·M) = M·Re(ε)`, `Im(ε·M) = M·Im(ε)`. One extra Einsum per channel — **not** a 4-multiply complex product, and **zero** reshapes/transposes. |
| `1/(120 |det|)` scaling | `Abs`/`Mul`/`Reciprocal`/`Unsqueeze` | **emittable** | Shared by both channels. |

Probe result: **bit-exact (`0.000e+00`)** for both 6×6 channel blocks
vs. the NumPy reference on a 2-tet mesh with genuinely anisotropic
data (three distinct complex diagonal entries from an oblique clamped
shell centroid).

**Stage T3 verdict: FALLBACK (paired-real), composing cleanly.**

### Stage T4 — global complex-symmetric scatter

Unchanged from H.5 Stage 5: shared int64 COO indices, two f64 zero
buffers (`ConstantOfShape`), two `ScatterND(reduction="add")` calls.
Probe result: bit-exact vs.
`assemble_global_nedelec_anisotropic` (`scipy.sparse` complex
reference), and the assembled buffers are exactly complex-symmetric
(`max|M − Mᵀ| = 0` in both channels — the pencil is complex-symmetric,
not Hermitian, as required).

**Stage T4 verdict: FALLBACK (paired-real two-buffer scatter).**
Inherited H.5; the tensor ε changes nothing at this stage.

### Stage T5 — complex generalized eigensolve

**Out-of-graph by definition** — LAPACK ZGGEV sidecar
(`sphere_pml.eigensolve_complex_dense`), identical disposition to H.5
Stage 6. λ→k and Q post-processing live in the same sidecar.

**Stage T5 verdict: BLOCKED (out-of-graph sidecar).** Unchanged.

### Target 1 answer to the J.6 question

**The pair-lowering composes cleanly with the tensor structure.** No
additional reshapes or transposes appear anywhere: the diagonal
tensor is one extra (broadcast or Einsum) axis, the complex reciprocal
has a real denominator, and the linearity of ε in the mass means the
Re/Im channels share every real intermediate. Marginal cost over the
scalar H.5 lowering: 2 Einsums + 1 ScatterND.

## Target 2 — analytic Mie root finding

Probe: [`probe_root_finding_loop.py`](probe_root_finding_loop.py).
All numerical checks compare against
[`reference/numpy/mie_roots.py`](../../../numpy/mie_roots.py)
(scipy `spherical_jn`/`spherical_yn` + `brentq`), scipy directly, or
the J.1 catalogue fixture
([`reference/fixtures/mie_roots/baseline.json`](../../../fixtures/mie_roots/baseline.json))
— never against the graph itself.

### Stage R-a — characteristic function evaluation

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `sin(x)` | `Sin` | **emittable** | f64 kernel registered. |
| **`cos(x)`** | `Cos` | **fallback** | **f64 kernel NOT registered in onnxruntime 1.26.0** (`NOT_IMPLEMENTED: Could not find an implementation for Cos(7)`; `Tan`/`Atan` f64 likewise missing). Lowering: `Sin(x + π/2)`, error ≲ ε·x. |
| Closed forms `j₀,j₁,y₀,y₁` | `Mul`/`Sub`/`Neg`/`Reciprocal` | **emittable** | |
| Upward recurrence to `j_l, y_l` (`l ≤ x+1` regime) | unrolled `Mul`/`Sub`, `l−1` steps | **emittable** | `l` is a graph-build-time constant (catalogue: `l ≤ 4`) — the recurrence is a FIXED-count loop and unrolls. |
| Miller downward recurrence (`l > x+1` regime) | unrolled `Mul`/`Sub`, `l+20` steps; rescale-at-1e100 via `Abs`/`Max`/`Greater`/`Reciprocal`/`Where` | **emittable** | The Rust kernel's conditional rescale is value-dependent but **control-static**: it lowers to an unconditional per-element `Where` factor each step. |
| Regime select | `GreaterOrEqual(x, l−1)` + `Where` | **emittable** | Compute both branches, select per element — both branches are total over the catalogue window. |
| Riccati-Bessel `ψ, ψ′, χ, χ′` and the TE/TM combination | `Mul`/`Add`/`Sub`/`Neg` | **emittable** | Direct transliteration. |

Probe results (30 001-point catalogue grid):

| Check | Max error | Criterion |
|---|---|---|
| `l=1` characteristic vs. reference | `3.5e-15` (scaled) | `< 1e-10` |
| `l=4` `j₃,j₄,y₃,y₄` vs. scipy, x ∈ [0.1, 40] (both regimes) | `≤ 1.9e-14` (envelope-scaled rel) | `< 1e-9` |
| `l=4` characteristic vs. reference | `5.4e-15` (term-scaled) | `< 1e-9` |

**Stage R-a verdict: EMITTABLE** (with the `Sin(x+π/2)` cosine
fallback). The "spherical Bessel via recurrence at fixed l" intuition
from the issue holds: every loop is fixed-count and unrolls; no `Loop`
op is needed for function evaluation.

### Stage R-b — grid scan + sign-change detection

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| Endpoint pairs `f[i], f[i+1]` | `Slice` × 4 (also for `k` endpoints) | **emittable** | Fixed shapes `(30000,)`. |
| Finite check | `IsNaN`/`IsInf`/`Or`/`Not` | **emittable** | f64 kernels registered. |
| Sign change `f_a f_b ≤ 0`, both-zero exclusion | `Mul`/`LessOrEqual`/`Equal`/`And` | **emittable** | |
| Pole rejection `min(\|f_a\|,\|f_b\|) > 1e8` | `Abs`/`Min`/`Greater`/`Not` | **emittable** | |

Probe result: the in-graph mask (15 brackets for TE l=1) matches the
`find_roots` acceptance conditions exactly.

**Stage R-b verdict: EMITTABLE.** The scan is data-INdependent in
shape (fixed 30 000 intervals), so the bracket *mask* is graph-pure.

### Stage R-c — bracket extraction + dedup + compaction

This is where the data-dependent control flow lives, and the verdict
splits by representation:

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| Dense bracket mask (from R-b) | — | **emittable** | The recommended in-graph representation. |
| **Consecutive 1e-5 dedup** | `Loop` (30 000 trips) carrying the last-*retained* scalar, scan-output keep flag | **expressible-but-tortured** | The dedup decision depends on the previously retained root — an inherently SEQUENTIAL scan, not a parallel op. Probed: works under onnxruntime (sub-second), matches `_dedup_consecutive` semantics exactly, including a synthetic near-duplicate case. But it serializes 30 000 subgraph invocations to make ~15 decisions. |
| **Compaction (masked roots → dense list)** | `NonZero` + `Squeeze` + `Gather` | **blocked for static shapes** | Runs under onnxruntime (dynamic shapes tolerated at runtime), but the output count is data-dependent — the same friction class as the G.6 `np.unique` finding. A static-shape consumer (the L4 contract) cannot type this output. |

Probe result: the full in-graph chain (dedup `Loop` → `NonZero` →
`Gather`) emits exactly the 15 reference roots at `4.6e-13` max
relative error — so the *runtime* can do it; the *type system* of a
static-shape backend cannot.

**Stage R-c verdict: BLOCKED for static shapes (host-side
compaction).** Recommended design: the graph outputs the dense
`(30000,)` refined-root vector plus the boolean keep-mask; the host
compacts and truncates to `n_max`. The dedup scan may live on either
side of the boundary — in-graph it is correct but tortured; host-side
it is three lines.

### Stage R-d — bisection refinement

The Rust refinement runs 60 bisection steps per bracket with two
early exits (converged: `f_mid = 0` or width `< 1e-12·max(|mid|,1)`;
aborted: `f_mid` non-finite). Probed in **two** graph forms,
vectorized over all 30 000 candidate intervals (inactive lanes are
masked, never branched):

| Form | ONNX expression | Status | Notes |
|---|---|---|---|
| **Unrolled** | 60 inlined copies of the step body; per-lane `break` → `Where` masking on an `active` lane mask | **emittable** | 7 160 top-level nodes for the full pipeline. Pure data-flow; no control-flow op at all. |
| **`Loop`** | `Loop(trip=60, cond)` with loop-carried `(lo, hi, f_lo, active)` and a scalar all-lanes-converged continue-condition (`Cast`/`ReduceMax`) | **emittable** | 134 top-level nodes. Loop-carried bool tensors work; the body reuses the identical step emission. |
| Per-lane early exit | none | (lowered) | No per-lane `break` exists in either form — convergence freezes a lane via `Where`; the `Loop` cond can only stop ALL lanes. |

Probe results:

| Check | Result |
|---|---|
| Unrolled vs. `Loop` forms on bracketed lanes | **bit-identical** |
| 15 in-graph roots vs. brentq reference roots | max rel err `4.6e-13` (`< 1e-10` contract) |
| First 5 roots vs. J.1 `baseline.json` TE l=1 (tol 2e-9 abs) | max abs diff `1.2e-12` |

**Stage R-d verdict: EMITTABLE (both unrolled and `Loop` forms).**

## Per-stage verdicts (one-line summary)

| Stage | Verdict |
|---|---|
| T1. `tet_centroids` | **emittable** — Gather + ReduceMean (real) |
| T2. `build_anisotropic_pml_tensor_diag` | **fallback** — paired-real `(n,3)` outputs; complex stretch + reciprocal lower to real ops; tensor structure = one broadcast axis, zero reshapes |
| T3. anisotropic local mass kernel | **fallback** — 3-Einsum chain (per-axis gram, constant pair-ladder, ε-contraction); Re/Im share the real per-axis tensor; bit-exact |
| T4. global complex-symmetric scatter | **fallback** — paired-real two-buffer ScatterND (inherited H.5); bit-exact, exactly symmetric |
| T5. complex generalized eigensolve | **blocked** — out-of-graph ZGGEV sidecar (inherited H.5) |
| R-a. characteristic function evaluation | **emittable** — fixed-count unrolled Bessel recurrences; `Cos` f64 needs the `Sin(x+π/2)` fallback |
| R-b. grid scan + sign-change mask | **emittable** — fixed-shape `(30000,)` bool mask |
| R-c. bracket extraction + dedup + compaction | **blocked for static shapes** — dedup is a sequential 30 000-trip `Loop` (tortured); compaction is `NonZero` (data-dependent shape). Keep dense+mask in-graph, compact at host. |
| R-d. bisection refinement | **emittable** — unrolled-60-with-`Where` AND `Loop(60, cond)`; the two forms are bit-identical and hit the J.1 catalogue to 1.2e-12 |

## Spec implications (whiteroom L4 `iterate-while` surface)

This is the first concrete data point on whether the proposed L4
`iterate-while` / `iterate-while-with-prev` primitives survive a
graph-only backend. They do — with sharp boundary conditions:

1. **ONNX `Loop` IS `iterate-while-with-prev`.** It provides
   loop-carried state (any tensor types, including bool masks), an
   optional max trip count, a scalar boolean continue-condition
   evaluated per iteration, and per-iteration scan outputs. The
   bisection probe ran it end-to-end under onnxruntime with f64 + bool
   carried state and an early-exit condition, bit-identical to the
   unrolled form. An L4 `iterate-while` lowers directly.
2. **…provided three contract restrictions hold:**
   - *Loop-invariant state shapes.* Carried tensors must keep their
     shape across iterations (ONNX requires it; so would any
     static-shape IR).
   - *Scalar continue-condition.* There is no per-lane early exit.
     Per-element convergence must lower to `Where` masking inside the
     carried state (the `active`-mask idiom probed here); the
     condition may only be a reduction ("any lane still active").
     An L4 spec that allows element-wise `break` semantics would NOT
     lower; one phrased as "state update + scalar predicate" does.
   - *No data-dependent output counts.* `iterate-while` may not be
     used to grow a result list. Variable-count results (the bracket
     list, the deduped catalogue) stay dense + mask inside the graph
     and compact at the host boundary — the same rule as the G.6/I.3
     `np.unique` disposition, now confirmed for iteration outputs.
3. **Fixed-trip iteration should prefer unrolling when the body is
   cheap and the trip count is a compile-time constant.** Both Bessel
   recurrences (≤ 24 steps) and the 60-step bisection unroll into
   pure data-flow with no semantic loss; `Loop` buys a 53× smaller
   graph (134 vs. 7 160 nodes) at the cost of a control-flow op that
   some inference stacks optimize poorly. The L4 surface can treat
   "unroll vs. emit-loop" as a backend lowering choice, not a
   semantic one — the probe demonstrates the two are bit-identical.
4. **Sequential scans are the true "secretly imperative" residue.**
   The 1e-5 dedup (carried last-*retained* value) is the only stage
   that is *inherently* sequential — not because of shapes, but
   because its recurrence is non-associative. It is expressible as a
   degenerate 30 000-trip `Loop`, but that is a serialization, not a
   lowering. The L4 spec should classify consecutive-dedup with the
   topology constructors: host-side, at the input/output boundary.
5. **Kernel coverage is part of the contract.** The missing f64
   `Cos`/`Tan` kernels in onnxruntime 1.26.0 show that opset-18
   *schema* support does not imply *runtime* support even for
   real-valued, first-class ops (the mirror image of the H.5 c128
   finding, where the schema accepted what no kernel honored). An L4
   conformance statement must pin the runtime, not just the IR
   version; trig-identity fallbacks (`Sin(x+π/2)`) preserve f64
   accuracy where dtype round-trips would not.

## Cross-cutting frictions: F.1/G.6/H.5/I.3 vs. J.6 comparison

| Friction | Prior status | J.6 status | Change |
|---|---|---|---|
| c128 type path (construct / arithmetic / fill) | blocked (H.5 headline) | blocked — inherited; tensor-ε adds no new c128 surface | **Unchanged** |
| Paired-real lowering of complex constitutive data | fallback (H.5, scalar ε) | fallback — composes cleanly with diagonal-tensor ε (2 extra Einsums + 1 ScatterND, zero reshapes) | **Extended** |
| Data-dependent output shape (`np.unique` / `NonZero`) | blocked (G.6, I.3 constructors) | blocked — recurs for bracket extraction and dedup compaction | **Unchanged — now covers iteration outputs** |
| `ScatterND(reduction="add")` f64 | emittable | emittable | **Unchanged** |
| `Einsum` f64 | emittable (G.6) | emittable — including 4-D per-axis grams and constant-tensor contraction | **Unchanged** |
| **ONNX `Loop` (carried state + cond + scan outputs)** | untested | **emittable** — fixed-trip, early-exit, bool carried state, outer-scope capture, sequential scan all work under onnxruntime 1.26 | **NEW in J.6** |
| **f64 trig kernel coverage** | untested | **`Sin` registered; `Cos`/`Tan`/`Atan` f64 NOT registered** in onnxruntime 1.26.0 — `Sin(x+π/2)` fallback | **NEW in J.6** |
| Fixed-count recurrence unrolling (value-dependent rescale) | untested | **emittable** — Miller's recurrence with `Where`-lowered conditional rescale | **NEW in J.6** |

## Recommended design for a future Mie ONNX pipeline

Graph A (assembly — extends the H.5/G.7 skeleton):

- Inputs: `nodes`, `tets`, `tag`, host-precomputed `tet_edge_idx` /
  `tet_edge_sign` / `interior_idx` (G.6 disposition).
- In-graph: centroids → paired-real tensor ramp (`eps_re`, `eps_im`
  `(n_tets, 3)`) → 3-Einsum anisotropic mass → shared-index two-buffer
  scatter. Outputs `K_int`, `M_re_int`, `M_im_int`.
- Host sidecar: re-assemble c128 pencil, ZGGEV, λ→k, Q, classification
  against the catalogue.

Graph B (root catalogue — per `(l, pol)`, generated with `l` baked):

- Input: the `(30001,)` k-grid (or bake it as a Constant).
- In-graph: characteristic evaluation (unrolled Bessel ladders,
  `Sin(x+π/2)` cosine) → bracket mask → bisection (unrolled or
  `Loop(60, cond)` — bit-identical) → outputs dense `roots (30000,)` +
  `mask (30000,) bool`.
- Host: compact, dedup (3 lines), truncate to `n_max`, merge channels,
  sort — exactly the cheap sequential residue.

## Re-running this audit

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt -r ../numpy/requirements.txt
python3 audit/sphere_mie/probe_tensor_eps_ramp.py
python3 audit/sphere_mie/probe_root_finding_loop.py
```

Each probe prints a one-screen verdict block and exits nonzero if any
check fails. The probes' expected-failure controls are the load-bearing
freshness signals: the tensor probe's graph (A) asserts the c128 `Mul`
rejection, and the root-finding probe's native f64 `Cos` control
asserts the `NOT_IMPLEMENTED` session-create failure behind the
`Sin(x+π/2)` fallback. If a future onnxruntime registers either
kernel, the corresponding probe exits nonzero, the fallback row
collapses to emittable, and this audit is stale — re-audit. (f64
`Tan`/`Atan` are likewise missing but unused by the pipeline, so they
carry no control.)

## Acknowledgements / cross-references

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — Phase
  J scope; Comment 4 for the broader vestigial-c128 finding.
- Issue [#5](https://github.com/rjwalters/geode-fem/issues/5) — the
  L4-input-boundary rule; the iterate-while finding above is the
  first iteration-shaped extension of it (cross-posted per the issue
  #175 acceptance criteria).
- Phase J.1 ([#170](https://github.com/rjwalters/geode-fem/issues/170),
  PR #177): [`mie_roots.py`](../../../numpy/mie_roots.py) and the
  [catalogue fixture](../../../fixtures/mie_roots/baseline.json) this
  audit's roots are validated against.
- Phase J.2 ([#171](https://github.com/rjwalters/geode-fem/issues/171),
  PR #179): [`sphere_mie.py`](../../../numpy/sphere_mie.py) — the
  tensor-ε reference algebra.
- Phase H.5: [`nedelec_pml_operator_audit.md`](../sphere_pml/nedelec_pml_operator_audit.md)
  — the c128/paired-real baseline every Target-1 verdict builds on.
- Phase I.3: [`derham_operator_audit.md`](../derham/derham_operator_audit.md)
  — format precedent and the int64-is-first-class mirror finding.
- Burn-side source of truth: `crates/geode-core/src/mie.rs`
  (`spherical_j_pair`, `find_roots`, `resonance_roots`) and
  `crates/geode-core/src/nedelec.rs`
  (`batched_nedelec_local_mass_anisotropic_diag`).
