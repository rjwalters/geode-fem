# Nédélec sphere-PML ONNX expressibility audit (Phase H.5)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase H.5
deliverable. Tracking issue:
[#157](https://github.com/rjwalters/geode-fem/issues/157).

## Scope

This audit catalogs the operators added by the **sphere-PML** assembly
spine on top of the sphere-PEC spine audited in Phase G.6
([`nedelec_operator_audit.md`](../sphere_pec/nedelec_operator_audit.md))
and records whether each lowers to a graph-only ONNX opset-18 form.

The sphere-PML pipeline — as defined by the NumPy reference in
[`reference/numpy/sphere_pml.py`](../../../numpy/sphere_pml.py) and the
Burn implementation in `crates/geode-core/src/nedelec_assembly.rs`
(`assemble_global_nedelec_with_complex_epsilon`) and
`crates/geode-core/src/pml.rs` (`build_complex_epsilon_r_pml`,
`tet_centroid_radii`) — differs from PEC in exactly **one**
constitutive piece: the per-tet permittivity becomes **complex** and
the mass scatter accumulates a complex-symmetric (not Hermitian)
matrix. Everything else (mesh I/O, edge enumeration, PEC outer mask,
Dirichlet restriction, d⁰ rank classifier) is imported verbatim from
the PEC pipeline; those stages inherit their PEC verdicts.

The new stages audited here are:

1. **`tet_centroid_radii`** — real, identical structure to ε_r
   assignment. No new ONNX friction.
2. **`build_complex_epsilon_r_pml`** — per-tet complex relative
   permittivity from physical-group tag + centroid radius. **Real
   ramp arithmetic, complex output.**
3. **Complex local Nédélec stiffness scatter** — per-element K block
   is real-valued, but lives inside a complex (K, M) pencil so the
   sidecar boundary types it as complex128.
4. **Complex local Nédélec mass scatter** — `m_local * sign_outer *
   epsilon_r_complex` is a c128 outer product on the per-element 6×6.
5. **Complex global assembly** — scatter c128 values into a c128
   (n_edges, n_edges) buffer via ScatterND.
6. **Complex generalized eigensolve** — LAPACK ZGGEV on the host.
   Out-of-graph by definition, same disposition as the PEC sidecar
   (and the cube-cavity F.2 eigensolve).

**Headline finding**: opset 18 has no first-class `complex128` (or
`complex64`) typed value path. The TypeProto enum lists `COMPLEX64=14`
and `COMPLEX128=15`, and `onnx.checker.check_model` schema-accepts
c128 in `Constant`/`Identity`/`Reshape`/`Gather`/`Scatter*`/`Where`
output positions — but **every** arithmetic op (`Add`, `Sub`, `Mul`,
`Div`, `Pow`, `MatMul`, `Einsum`, `ReduceSum`, …) and the
`ConstantOfShape` fill type and `Cast` exclude c128 from their type
constraints, and `onnxruntime` rejects c128 inputs even to the ops
whose schema accepts them. This is the broader graph-only finding
flagged in Comment 4 of Epic [#88](https://github.com/rjwalters/geode-fem/issues/88):
the ONNX IR's c128 type is a vestigial datum that no kernel honors.

## Pinned versions used for this audit

Same as Phase F.1 / G.6:

| Package | Version |
|---|---|
| `onnx` | `1.21.0` |
| `onnxruntime` | `1.26.0` |
| Target opset | `18` |

See [`reference/onnx/requirements.txt`](../../requirements.txt) for the
pinned `pip` line.

## Classification convention

Per-operator status follows the issue #157 convention:

| Marker | Meaning |
|---|---|
| **emittable** | Op lowers directly to opset 18 with no special handling. |
| **fallback** | Op cannot lower as-typed (e.g. c128 forbidden); a paired-real or alternative lowering exists and is documented. |
| **blocked** | Op has no lowering — must be host-computed and passed as a graph input or executed in an out-of-graph sidecar. |

This is a slight reframing of the Phase F.1 / G.6 markers
(`clean` / `synth` / `caveat` / `secretly imperative`) to match the
issue's request, with the following mapping:

- `clean` / `synth` (real-valued ops working in opset 18) → **emittable**
- "graph-only friction" (real, just verbose) → **emittable**
- "complex op not supported, but paired-real lowering exists" → **fallback**
- "secretly imperative" + "no lowering, host-only" → **blocked**

## Operator table

### Stage 1 — Per-tet centroid radius (`tet_centroid_radii`)

No probe required; this is `mean(nodes[tets], axis=1)` followed by
`ReduceL2(axis=1)`. Identical structure to the PEC mask Stage 4a
(node radii) but on the per-tet centroid instead of per-node.

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `nodes[tets, :]` gather | `Gather(axis=0)` on `nodes` with `tets` flattened | **emittable** | Native opset-18. Same as the per-tet coord gather in Stage 3 of G.6. |
| `mean(..., axis=1)` over 4-vertex group | `ReduceMean(axes=[1])` | **emittable** | Native. |
| `np.linalg.norm(centroids, axis=1)` | `ReduceL2(axes=[1])` | **emittable** | Native opset 18, same as PEC Stage 4a. |

**Stage 1 verdict: EMITTABLE.** Real-valued; no friction beyond the
PEC pattern already audited in G.6.

### Stage 2 — Complex ε ramp (`build_complex_epsilon_r_pml`)

Probe: [`probe_complex_eps_ramp.py`](probe_complex_eps_ramp.py).

The PML profile is a tag-keyed Where ladder plus a quadratic ramp on
the radii. The arithmetic is **all real** until the final
`(re, im) → c128` step. That last step has no opset-18 lowering.

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `tags == PHYS_SPHERE_INTERIOR`, `tags == PHYS_PML_SHELL` | `Equal` | **emittable** | Native, int32. |
| `where(is_int, n², 1.0)` (real part) | `Where` | **emittable** | Native, f64. |
| `(radii − R_PML_INNER) / Δ` | `Sub` + `Mul` | **emittable** | Native f64. |
| `clip(u, 0, 1)` | `Clip` | **emittable** | Native f64. |
| `u * u` | `Mul` | **emittable** | Native. |
| `−σ₀ · u²` | `Mul` | **emittable** | Native. |
| `where(is_pml, im_ramp, 0)` (imag part) | `Where` | **emittable** | Native f64. |
| **`re + 1j * im → complex128`** | **none** | **blocked** | **No `Complex(re, im)` op in opset 18.** `Cast` excludes c128 as a target. The only ways to introduce a c128 tensor are (a) a c128 graph input, or (b) a c128 `Constant` — neither constructs c128 from two f64 tensors. |

**Recommended lowering (fallback): paired-real outputs.** Emit two
f64 outputs `eps_re (n_tets,)` and `eps_im (n_tets,)`. The downstream
scatter consumes them as two parallel real channels. This mirrors the
"complex as two reals" convention used by TF-Java (Phase H.4),
JAX-on-TPU, and the SciML community.

Probe result: paired-real graph passes `onnx.checker` and
`onnxruntime`; bit-exact match (`0.000e+00`) vs. the NumPy reference
on a 6-tet test case spanning all three regions (interior, vacuum,
PML at three radii including the clamped `u = 1` overshoot).

The native-c128 control graph (A) fails at session-load time with
the precise error:

```
[ONNXRuntimeError] : 1 : FAIL : Exception during loading:
MLDataType for: tensor(complex128) is not currently registered
or supported
```

This is captured automatically when running the probe; the failure
itself is the load-bearing evidence.

**Stage 2 verdict: FALLBACK (paired-real lowering).** The op as
specified (single c128 output) is BLOCKED; the paired-real
alternative is EMITTABLE.

### Stage 3 — Complex local Nédélec stiffness scatter

**Structurally unchanged from G.6 Stage 3 + Stage 5.** The K curl-curl
local block is real-valued in both PEC and PML; only the eigensolve
boundary sees it as complex128 (where it has `Im(K) = 0` identically).

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| Per-element 6×6 K_local computation | (see G.6 Stage 3 — full inventory of Gather/Mul/Sub/Add/Einsum) | **emittable** | Unchanged from G.6. Bit-exact in f64. |
| Sign outer product `s_i s_j` | `Unsqueeze` × 2 + `Mul` | **emittable** | Unchanged from G.6 Stage 5. |
| `k_local * sign_outer` | `Mul` (f64) | **emittable** | Unchanged from G.6 Stage 5. |
| Real ScatterND into `(n_edges, n_edges)` f64 buffer | `ConstantOfShape` + `ScatterND(reduction="add")` | **emittable** | Unchanged from G.6 Stage 5. |
| Eigensolve boundary types K as complex128 | (downstream) | **fallback** | The sidecar driver promotes K to c128 (with `Im(K) = 0`) before calling LAPACK ZGGEV. The promotion happens **on the host**, not in the graph. |

**Stage 3 verdict: EMITTABLE** (real graph; eigensolve sidecar
handles the c128 promotion on the host). No new ONNX friction over G.6.

### Stage 4 — Complex local Nédélec mass scatter

Probe: [`probe_complex_local_scatter.py`](probe_complex_local_scatter.py).

The mass `M_local` is real, but it picks up a per-tet **complex**
scaling: `m_signed_c128 = m_local * sign_outer * epsilon_r_complex[e]`.
The Mul between an f64 tensor and a c128 tensor has no opset-18
lowering.

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| Per-element 6×6 M_local computation | (see G.6 Stage 3) | **emittable** | Real. Unchanged from G.6. |
| Sign outer product `s_i s_j` | `Unsqueeze` × 2 + `Mul` | **emittable** | Real. Unchanged from G.6 Stage 5. |
| `m_local * sign_outer` | `Mul` (f64) | **emittable** | Real. |
| **`m_signed_real * eps_complex[e]`** (f64 × c128) | `Mul` | **blocked** | `Mul`'s `T` type constraint in opset 18 excludes c128/c64. Runtime error: `Type 'tensor(complex128)' of input parameter (eps_b) of operator (Mul) in node () is invalid.` |
| **`m_signed_c128 * m_signed_c128`** (c128 × c128) | `Mul` | **blocked** | Same type constraint exclusion. |
| `Reshape` over c128 | `Reshape` | (schema-OK) **blocked at upstream** | Reshape schema accepts c128 as `T`, but no upstream op can produce the c128 tensor to feed it. |

**Recommended lowering (fallback): paired-real Mul + paired-real
ScatterND.** Emit `m_signed_re = m_local * sign_outer * eps_re[e]`
and `m_signed_im = m_local * sign_outer * eps_im[e]`. The COO
indices (int64) and sign outer product (f64) are shared between the
two channels; only the final Mul-by-ε and the terminal ScatterND
duplicate. The graph runtime cost is ~2× the PEC scatter (one extra
Mul + one extra ScatterND per call), not 4× as a naïve "complex
multiplication adds 4 real multiplies" estimate might suggest,
because `m_local * sign_outer` is shared and `m_local` is real
(so `Re(eps · M) = Re(eps) · M` and `Im(eps · M) = Im(eps) · M`).

Probe result: paired-real graph passes `onnx.checker` and
`onnxruntime`; bit-exact match (`0.000e+00`) for both `Re(M_global)`
and `Im(M_global)` vs. the NumPy `scipy.sparse.coo_matrix` complex
reference on a 2-tet (n_edges=9) test case with a non-trivial
imaginary ε on one of the two tets.

**Stage 4 verdict: FALLBACK (paired-real lowering).** The single-Mul
c128 path is BLOCKED; the two-Mul paired-real alternative is
EMITTABLE and is the recommended design.

### Stage 5 — Complex global assembly (sparse scatter into c128 K, M)

Same situation as Stage 4 at the global-buffer level. The c128
ScatterND **schema-accepts** c128 in its `T` type constraint, but
two upstream blockers prevent reaching it:

1. **`ConstantOfShape` does not support c128 as a fill type.** Its
   `T2` (value) type constraint excludes both c128 and c64. There is
   no way to initialize a zero c128 buffer in-graph.
2. **No c128 value tensor exists** to feed `ScatterND` as the
   `updates` input, because Stage 4's Mul is blocked.

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| Zero c128 buffer `(n_edges, n_edges)` | `ConstantOfShape` | **blocked** | `T2` excludes c128. Workaround: emit two f64 zero buffers via `ConstantOfShape` (which IS emittable) — see Stage 4 paired-real recommendation. |
| `ScatterND(reduction="add")` on c128 buffer | `ScatterND` | schema-OK, **unreachable** | `T` accepts c128, but no upstream c128 producer exists in opset 18. |
| Paired-real two-buffer ScatterND | `ConstantOfShape` × 2 + `ScatterND` × 2 | **emittable** | Recommended. Same int64 indices shared between Re and Im scatters. |
| Host re-assembly `M = M_re + 1j * M_im` | (out-of-graph) | **blocked** | Done in the sidecar driver, not in the ONNX graph. Same convention as the eigensolve sidecar. |

**Stage 5 verdict: FALLBACK (paired-real two-buffer scatter).**
Single-buffer c128 ScatterND is BLOCKED; paired-real two-buffer
alternative is EMITTABLE.

### Stage 6 — Complex generalized eigensolve (LAPACK ZGGEV)

**Out-of-graph by definition**, same disposition as the PEC
eigensolve sidecar (G.6 Stage 7) and the cube-cavity F.2 eigensolve
(F.1 Stage 4).

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `scipy.linalg.eig(K_int, M_int)` (LAPACK ZGGEV) | **none** | **blocked** | No `Eigh` op in ONNX; no `Eig` op for non-Hermitian pencils. The PML pencil is complex-symmetric (`M = Mᵀ`) but **not** Hermitian, so even a hypothetical `Eigh` would not apply. ARPACK `eigs` shift-and-invert at σ=0 fails for the curl-curl operator (large gradient kernel makes `(K − σM)⁻¹` near-singular at σ=0) — see [`sphere_pml.py:eigensolve_complex_dense`](../../../numpy/sphere_pml.py) for the dense-LAPACK rationale. The dense path is what the sidecar runs. |

**Stage 6 verdict: BLOCKED (out-of-graph sidecar).** Same convention
as Phase F.2 / G.7: the ONNX graph emits the c128 (K_int, M_int)
pair (in paired-real form), the host driver re-assembles complex
matrices, calls `scipy.linalg.eig`, and writes the result to the
schema-v1 sidecar JSON.

## Cross-cutting frictions: G.6 vs. H.5 comparison

| Friction | G.6 (sphere-PEC Nédélec) | H.5 (sphere-PML Nédélec) | Change |
|---|---|---|---|
| `build_edges` secretly-imperative | yes (host-precomputed in [PR #142](https://github.com/rjwalters/geode-fem/pull/142)) | yes (inherited verbatim) | **Unchanged** |
| Sign outer product (real) | emittable, graph-only friction | emittable, graph-only friction | **Unchanged** |
| Global buffer size `(n_edges, n_edges)` | data-dependent (inherited from `build_edges`) | data-dependent (inherited) | **Unchanged** |
| `NonZero` (mask→idx) data-dependent shape | caveat (recommendation: host-precompute) | caveat (inherited) | **Unchanged** |
| ScatterND `reduction="add"` (f64) | emittable | emittable (for K and for paired Re/Im of M) | **Unchanged** |
| **Complex permittivity construction** | N/A (no PML) | **blocked** (no `Complex(re, im)` op; `Cast` excludes c128) | **NEW in H.5** |
| **f64 × c128 elementwise Mul** | N/A | **blocked** (`T` type constraint excludes c128) | **NEW in H.5** |
| **`ConstantOfShape` with c128 fill** | N/A | **blocked** (`T2` excludes c128) | **NEW in H.5** |
| **c128 ScatterND** | N/A | schema-OK, unreachable | **NEW in H.5** |
| **Complex generalized eigensolve** | N/A (real `eigsh`) | **blocked** out-of-graph; same disposition as the real `eigsh` boundary in G.6/F.1 | **NEW in H.5; same sidecar pattern** |

### Friction class summary

All of the H.5 new findings collapse into one underlying friction:
**ONNX opset 18 does not provide a usable c128 type path.** The type
exists in the IR enum and is accepted as a schema-level value type
by a handful of value-passing ops (Constant, Identity, Reshape,
Gather, ScatterND, Where), but:

- No op constructs c128 from real parts (`Complex` missing).
- No arithmetic op accepts c128 (`Add`/`Sub`/`Mul`/`Div`/`MatMul`/`Einsum`/`ReduceSum`/etc. exclude c128 from `T`).
- No reduction op accepts c128.
- `ConstantOfShape` cannot initialize a c128 buffer.
- `Cast` cannot promote real → c128 or c128 → real.
- `onnxruntime` rejects c128 inputs even where the schema accepts them.

The result is that **c128 values can move through the graph but
cannot be produced or operated on inside it**. The only viable
lowering is paired-real: split every c128 tensor into two f64
tensors and let the host re-assemble at the sidecar boundary.

This matches the broader graph-only finding from Comment 4 on Epic
[#88](https://github.com/rjwalters/geode-fem/issues/88): the c128
type is a vestigial signpost in the ONNX IR, not a working dtype.
Any backend that wants complex arithmetic must lower it to
paired-real before emitting ONNX.

## Per-stage verdicts (one-line summary)

| Stage | Verdict |
|---|---|
| 1. `tet_centroid_radii` | **emittable** — Gather + ReduceMean + ReduceL2 (real) |
| 2. `build_complex_epsilon_r_pml` | **fallback** — real ramp is emittable; c128 output is **blocked** (no `Complex(re, im)` op). Emit two f64 outputs `eps_re`, `eps_im`. |
| 3. Complex local stiffness scatter | **emittable** — K_local is real-valued; promotion to c128 happens at the eigensolve sidecar (out-of-graph) |
| 4. Complex local mass scatter | **fallback** — `m_local * sign_outer * eps_complex` Mul is **blocked**; emit paired-real `m_signed_re`, `m_signed_im` |
| 5. Complex global assembly | **fallback** — single-buffer c128 ScatterND is **blocked** (no c128 ConstantOfShape, no c128 upstream); use two paired-real ScatterND calls into `M_re_global`, `M_im_global` |
| 6. Complex generalized eigensolve | **blocked** — out-of-graph LAPACK ZGGEV sidecar, same as PEC `eigsh` sidecar |

## Recommended design for a future Nédélec sphere-PML ONNX assembly graph

Building on the [Phase G.7 design](../sphere_pec/nedelec_operator_audit.md#phase-g7--empirical-confirmation-of-the-recommended-design)
that proved out the recommended Nédélec PEC graph:

Graph inputs:
- `nodes (n_nodes, 3) float64` — mesh vertex coordinates
- `tets (n_tets, 4) int64` — tet connectivity
- `tag (n_tets,) int32` — per-tet physical-group tag (replaces `epsilon_r`)
- `tet_edge_idx (n_tets, 6) int64` — host-computed (from `build_edges`)
- `tet_edge_sign (n_tets, 6) float64` — host-computed (from `build_edges`)
- `interior_idx (n_int,) int64` — host-computed (from `flatnonzero(pec_mask)`)
- `n_inside: float`, `sigma_0: float` — scalar constants baked at graph generation

Graph constants (baked at generation time):
- `n_edges: int` — from `len(edges)` (output of `build_edges`)
- `n_int: int` — from `len(interior_idx)`
- `R_PML_INNER`, `R_BUFFER` — fixture geometry constants

Graph outputs (paired-real):
- `K_int (n_int, n_int) float64` — real curl-curl interior
- `M_re_int (n_int, n_int) float64` — real part of complex ε-mass
- `M_im_int (n_int, n_int) float64` — imag part of complex ε-mass

Sidecar host steps (out-of-graph):
1. Re-assemble `M_int_complex = M_re_int + 1j * M_im_int`.
2. Promote K to c128 with `K_int_complex = K_int + 0j` for uniform
   pencil dtype.
3. Call `scipy.linalg.eig(K_int_complex, M_int_complex)` (dense
   LAPACK ZGGEV — see [`sphere_pml.eigensolve_complex_dense`](../../../numpy/sphere_pml.py) for why ARPACK shift-and-invert is not usable).
4. Filter infinite eigenvalues, sort by `|Re(λ)|`, split spurious
   cluster off via the d⁰ rank (`spurious_dim_from_derham`, also
   host-side per G.6 Stage 7).
5. Emit schema-v1 sidecar (extends the Phase H schema with the
   c128 mass channel).

Total c128 surface area in the graph: **zero**. The host owns every
c128 value.

## Acknowledgements / cross-references

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — overall
  framing and Phase H scope. See Comment 4 for the broader graph-only
  finding that c128 is a vestigial type across ONNX/TF/XLA IRs.
- Phase G.6 analog: [`nedelec_operator_audit.md`](../sphere_pec/nedelec_operator_audit.md)
  — the baseline comparison for every "unchanged" row above. The
  `build_edges` "secretly-imperative" finding from G.6 is inherited
  verbatim; PR [#142](https://github.com/rjwalters/geode-fem/pull/142)
  is where the host-precompute landed.
- Phase F.1 baseline: [`cube_cavity_operator_audit.md`](../cube_cavity_operator_audit.md)
  — the original real-valued audit that set the classification
  convention.
- NumPy sphere-PML reference: [`reference/numpy/sphere_pml.py`](../../../numpy/sphere_pml.py).
- Burn-side kernels:
  `crates/geode-core/src/pml.rs` (build_complex_epsilon_r_pml,
  tet_centroid_radii) and
  `crates/geode-core/src/nedelec_assembly.rs::assemble_global_nedelec_with_complex_epsilon`.
- Phase G.7 PR [#140](https://github.com/rjwalters/geode-fem/issues/140)
  — empirical confirmation of the PEC recommended design; the PML
  graph should follow the same skeleton with paired-real M outputs.

## Re-running this audit

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt
python3 audit/sphere_pml/probe_complex_eps_ramp.py
python3 audit/sphere_pml/probe_complex_local_scatter.py
```

Each probe prints a one-screen verdict that should match the
corresponding row(s) in this table. If a probe's verdict disagrees
with the table after a version bump, the table is stale — re-audit.
In particular, if a future onnxruntime release registers a c128
MLDataType for `Add`/`Mul`/`ScatterND`, the entire "fallback (paired-
real)" column collapses to "emittable" and this audit is obsolete.
The probes' explicit runtime-error capture is the load-bearing
freshness signal.
