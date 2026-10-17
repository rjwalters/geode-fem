# Cube-cavity ONNX expressibility audit (Phase F.1)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase F.1
deliverable. Tracking issue: [#116](https://github.com/rjwalters/geode-fem/issues/116).

## Scope

This audit catalogs the L4 operators on the **scalar-Helmholtz
cube-cavity assembly spine** and records whether each lowers to a
graph-only ONNX form. The cube-cavity case is real-symmetric throughout
(no PML, no driven sources), so the complex-arithmetic friction surface
is deliberately deferred to a later phase.

The cube-cavity assembly spine — as defined by the reference
implementations in [`reference/numpy/cube_cavity_minimal.py`](../../numpy/cube_cavity_minimal.py),
[`reference/jax/cube_cavity.py`](../../jax/cube_cavity.py), and
[`reference/tf_java/cube_cavity/.../AssemblyGraph.java`](../../tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/AssemblyGraph.java)
— consists of these stages:

1. **Per-element P1 local matrices** (`K_local`, `M_local`, shape
   `(n_elem, 4, 4)`): edge vectors, cross products, dot products, scale
   by `1/det` and `det/120`, plus a constant mass pattern.
2. **Global K/M scatter-add** (shape `(n_nodes, n_nodes)`): scatter the
   per-element entries into a zero buffer along the connectivity-table
   indices.
3. **Dirichlet restriction** (`K_int`, `M_int`, shape `(n_int, n_int)`):
   drop the boundary rows/cols using an interior-DOF index set.
4. **Generalized eigensolve** (`K_int v = λ M_int v`): solve for the
   lowest `k` modes.

Per the Epic #88 convention (Phase C wording: *"differentiability of
assembly tested (eigensolve boundary allowed)"*), stage 4 lives in a
host driver across all reference backends. ONNX inherits this
convention unchanged — there is no `Eig`/`Eigh` op in ONNX Runtime,
and adding one would not actually change the scope decision because
the eigensolve is *already* a sidecar boundary in TF-Java
([eigensolve_from_tfjava.py](../../driver/eigensolve_from_tfjava.py))
and in JAX (`scipy.linalg.eigh` at the end of `solve_cube_cavity_jax`).

The audit therefore covers stages **1–3**.

## Pinned versions used for this audit

| Package | Version |
|---|---|
| `onnx` | `1.21.0` |
| `onnxruntime` | `1.26.0` |
| `onnxscript` | `0.7.0` (available for authoring, not used in probes) |
| Target opset | `18` |

See [`reference/onnx/requirements.txt`](../requirements.txt) for the
pinned `pip` line. The probes here use raw `onnx.helper` (not
`onnxscript`) so that what shows up at the IR level is exactly what
the audit reports — `onnxscript` is more ergonomic but can hide
imperative sugar that the audit needs to surface.

## Classification convention

Each row in the operator table below is one of:

| Marker | Meaning |
|---|---|
| **clean** | Direct opset-18 operator; lowers without any synthesis. |
| **synth** | No native op, but a graph-only synthesis from lower-level ops works (e.g. cross product as `6 Mul + 3 Sub + 3 Unsqueeze + Concat`). Documentation, not a blocker. |
| **caveat** | The op lowers, but introduces a downstream constraint (e.g. data-dependent shape) that materially affects the graph's static-shape inference. |
| **graph-only friction** | Forced by the ONNX IR's static-graph form; the L4 operator IS expressible, the friction is overhead. |
| **secretly imperative** | An L4 operator that relies on imperative escape; would block end-to-end lowering. **This is the high-value friction-mining outcome.** |

## Operator table

### Stage 1 — Per-element P1 local matrices

Probe: [`probe_p1_local.py`](probe_p1_local.py).

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `v_i - v_j` (vector subtraction) | `Sub` | clean | Native. |
| `e_i * e_j` (elementwise multiply) | `Mul` | clean | Native. |
| `a + b` (add) | `Add` | clean | Native. |
| `-x` (negate) | `Neg` | clean | Native. |
| `\|x\|` (abs) | `Abs` | clean | Native. |
| `a / b` (divide) | `Div` | clean | Native; broadcasts. |
| `sum(...)` reduction along axis | `ReduceSum` | clean | Axis is a graph input (int64), not an attribute. |
| Reshape `(N,) → (N, 1, 1)` | `Reshape` | clean | Native. |
| Transpose last two axes (batched) | `Transpose(perm=[0, 2, 1])` | clean | Native. |
| `stack([a, b, c], axis=1)` | **synth**: `Unsqueeze` each + `Concat` | graph-only friction | ONNX has no `Stack` op; the L4 verb decomposes to two ONNX ops. Same shape as TF-Java's hand-rolled stack inside `cross3`. |
| `v[:, i]` (component extract, axis=1) | `Gather(axis=1)` | clean | Native; **int64 indices required**. |
| `cross(a, b)` (3-vector cross product) | **synth**: `6 Mul + 3 Sub + 3 Unsqueeze + Concat` | graph-only friction | No native `Cross`. Same disposition as TF-Java (`AssemblyGraph.cross3`). Mathematically transparent; the synthesis is line-for-line. |
| `A @ B` (rank-3 batched matmul) | `MatMul` | clean | **L4-native broadcasting**: opset 18 `MatMul` broadcasts over the leading batch dim of `(N, 4, 3) @ (N, 3, 4)` and returns `(N, 4, 4)` directly. Compare TF-Java, whose `tf.linalg.matMul` does **not** broadcast over a batch dim and forced a fallback to `einsum("eik,ejk->eij", gMat, gMat)`. This is one of the rare cases where ONNX is *less* friction than another XLA-shaped IR. |
| Constant `mass_pattern` (4×4 baked) | `Constant` | clean | Native. |
| Broadcast `(1, 4, 4) * (N, 1, 1)` → `(N, 4, 4)` | `Mul` | clean | Native (NumPy-style broadcasting). |

Stage 1 numerical check: probe matches the NumPy reference at `max |K_onnx - K_numpy| = 0.000e+00` (bit-exact in f64 for the canonical reference tet).

### Stage 2 — Global K/M scatter-add

Probe: [`probe_assembly_scatter.py`](probe_assembly_scatter.py).

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| Allocate zero buffer of shape `(n_nodes, n_nodes)` | `ConstantOfShape` | clean | Native (opset 9+). |
| Stack `(rows, cols)` into a `(M, 2)` index table | `Unsqueeze` + `Concat` | clean | Same Stack-decomposition pattern as stage 1. |
| **`buf.at[rows, cols].add(vals)` — scatter-add into dense buffer** | **`ScatterND(reduction="add")`** | **clean** | **This is the critical operator for Phase F.2 end-to-end assembly.** ONNX has had `ScatterND` since opset 11 and `reduction="add"` since opset 16. Semantics match JAX `.at[...].add(...)` and TF-Java `tf.scatterNd` on a zero buffer. **Non-destructive** (returns a new tensor — there is no in-place mutation here). |
| Cast `int32` indices to `int64` | `Cast` | clean | `ScatterND` requires int64 indices in opset 18. Same friction as TF-Java (`tf.dtypes.cast(indexPairs, TInt64.class)`). Boundary cost, not a lowering blocker. |

Stage 2 numerical check: probe matches an explicit NumPy `np.add.at(...)` scatter at `max err = 0.000e+00`.

**Headline finding**: ScatterND-with-reduction-add is the
single most consequential operator for Phase F.2, and it
lowers cleanly. The end-to-end ONNX assembly graph is feasible
through stage 2 inclusive.

### Stage 3 — Dirichlet restriction (interior-DOF set)

Probe: [`probe_dirichlet_mask.py`](probe_dirichlet_mask.py).

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `K[idx, :]` with precomputed `idx` (int64) | `Gather(axis=0)` | clean | Native. |
| `K[:, idx]` with precomputed `idx` (int64) | `Gather(axis=1)` | clean | Native. |
| `K[np.ix_(idx, idx)]` (outer-product fancy index) | `Gather(axis=0)` then `Gather(axis=1)` | clean | The L4 verb is two-axis fancy indexing; ONNX has no single op for that, but the obvious decomposition into successive axis-Gathers works. |
| **`idx = np.where(mask)[0]`** | **`NonZero` then `Squeeze`** | **caveat** | The op lowers, BUT `NonZero` returns a shape `(rank, n_nonzero)` tensor whose `n_nonzero` axis is data-dependent. Everything downstream that consumes `idx` (the two `Gather`s, then any further reductions) inherits a `(None, None)` static shape. The graph remains valid; static-shape inference is lost. |

#### Friction note on stage 3 (the most informative row in this audit)

`np.where(mask)[0]` — building an index set from a boolean mask — is
the operation we expected to surface as a "secretly imperative L4"
escape. The audit finding is more nuanced than expected:

- ONNX **has** `NonZero` (opset 9+), so the operator IS expressible
  in a graph-only IR.
- However, `NonZero`'s output shape depends on the input *values*, not
  just the input *shape*. The ONNX type system marks the downstream
  `idx` (and everything that consumes it) as having an unknown
  intermediate axis. This propagates: `K_int` is typed `(None, None)`
  rather than `(n_int, n_int)`. The graph is valid, but the
  shape-inference contract that XLA-style IRs trade on collapses
  downstream.
- **Classification: graph-only friction, not secretly-imperative.**
  The L4 verb (`where`) IS in the ONNX vocabulary; what's lost is the
  *static* dimensional information that compile-time graph optimizers
  use. This matches the JAX experience exactly: `jnp.nonzero` is a
  `jit`-blocker for similar reasons, and the JAX reference
  ([`cube_cavity.py`](../../jax/cube_cavity.py) lines 192–195) computes
  `idx` on the **host side**, outside the jit boundary.
- **Recommendation for Phase F.2**: the ONNX assembly graph should
  accept `idx` as a *graph input* (computed by the host driver), not
  derive it inside the graph. This:
  - keeps the graph statically-shaped end-to-end,
  - mirrors the JAX convention (which is the closest L4-shape sibling),
  - mirrors the TF-Java convention (Dirichlet masking happens in the
    JVM driver, not inside the assembly graph).

This is the same conclusion the TF-Java reference reached implicitly,
but ONNX's static-shape contract forces us to name it explicitly. **The
"forcing function" worked exactly as Epic #88 anticipated** — putting
the L4 calculus through a graph-only IR exposed a cross-backend
convention that none of the existing backends needed to articulate.

### Stage 4 — Generalized eigensolve (boundary, shared L4 friction)

Not in this audit. Across the entire reference set:

| Backend | Eigensolve location |
|---|---|
| NumPy | `scipy.sparse.linalg.eigsh` on host |
| JAX | `scipy.linalg.eigh` outside the jit boundary |
| TF-Java | `scipy.sparse.linalg.eigsh` via JSON sidecar |
| ONNX (Phase F.2) | `scipy.sparse.linalg.eigsh` via JSON sidecar (planned, mirroring TF-Java) |
| Burn | `FaerDenseEigensolver` or `arpack` FFI on host |

The eigensolve is **shared L4 friction** — every backend offloads it
to the same SciPy ARPACK boundary. There is no ONNX-specific gap to
report here. (If ONNX's surface eventually grew an `Eigh` op, the
audit would not change scope, because the seam is convention-set
across the reference set, not ONNX-specific.)

## Cross-cutting frictions

Observations that apply across multiple stages of the audit:

1. **Axis arguments are graph inputs, not attributes (opset 13+).**
   Operations like `Unsqueeze`, `Squeeze`, `ReduceSum`, `Concat` take
   their `axes`/`axis` arguments as `int64` tensor inputs. This
   produces a lot of `Constant` int64 axis nodes scattered through
   any non-trivial graph. The cost is verbosity, not expressibility.
   `onnxscript` hides this; raw `onnx.helper` (as used in the probes)
   makes it explicit. This is graph-only friction.
2. **Index/shape dtypes must be int64.** Several ops (`Gather`,
   `ScatterND`, `Reshape`, `Unsqueeze`'s axes) require int64
   indices/shapes. JAX accepts int32 freely; NumPy converts silently;
   TF-Java already had to cast (`tf.dtypes.cast(..., TInt64.class)`).
   ONNX inherits this — cast at the I/O boundary.
3. **No `Stack` op.** NumPy `np.stack`, JAX `jnp.stack`, TF-Java
   `tf.stack` all exist as first-class verbs. ONNX views them as
   imperative sugar and forces `Unsqueeze + Concat` (one
   `Unsqueeze` per operand, then `Concat`). The decomposition is
   shape-safe and graph-only — documentation, not a blocker.
4. **`MatMul` broadcasts over the batch dim natively** — a rare case
   where ONNX is *less* friction than another XLA-shaped IR (TF-Java
   forced a fallback to `einsum`).
5. **Static-shape preservation across the entire stage 1 + stage 2
   path.** With `idx` passed as a graph input (per the stage 3
   recommendation), the ONNX assembly graph stays statically shaped
   from `nodes (n_nodes, 3)` and `tets (n_elem, 4)` through to
   `K_global (n_nodes, n_nodes)`. This is the property Phase F.2
   needs to validate end-to-end.
6. **Complex arithmetic.** Deferred to the PML phase (Phase H). ONNX
   has only narrow complex support (e.g. `DFT`, no general complex
   tensor algebra). For cube-cavity (real symmetric), this is not
   exercised.

## Per-stage verdicts (one-line summary)

| Stage | Verdict |
|---|---|
| 1. P1 local matrices | clean (modulo two synth ops: `Stack` and `cross`) |
| 2. Global K/M scatter-add | clean (via `ScatterND(reduction="add")`) |
| 3. Dirichlet restriction | clean *if* `idx` is host-computed; caveat (data-dependent shape) *if* derived in-graph via `NonZero` |
| 4. Eigensolve | out of scope — shared L4 friction handled by host driver across all backends |

## What this means for Phase F.2

The Phase F.2 follow-on issue (cube-cavity end-to-end ONNX assembly
graph + sidecar eigensolve) is **feasible**. The recommended design
shape, derived from this audit:

- Graph inputs: `nodes (n_nodes, 3) float64`, `tets (n_elem, 4) int64`,
  `idx_int (n_int,) int64`.
- Graph outputs: `K_int (n_int, n_int) float64`, `M_int (n_int, n_int)
  float64`.
- The Dirichlet step uses precomputed `idx`, exactly mirroring the
  JAX and TF-Java conventions.
- Eigensolve seam: ONNX Runtime emits a JSON sidecar identical in
  schema to `reference/tf_java/cube_cavity/.../CubeCavityMain.java`'s
  output, consumed by a near-clone of
  `reference/driver/eigensolve_from_tfjava.py` (or by a refactored
  backend-agnostic `eigensolve_from_sidecar.py` — design decision
  belongs in F.2's curation pass).
- CI gate: mirrors `.github/workflows/tfjava-cube-cavity.yml`; three-
  way cross-IR agreement (ONNX vs JAX vs NumPy) at `rtol=1e-5`.

**Operators that WOULD become CI assertions in Phase F.2**: every
"clean" / "synth" row in the tables above. The only operator that
is explicitly *excluded* from the F.2 graph (by audit recommendation)
is `NonZero` — Dirichlet mask resolution happens host-side.

## What this audit did NOT find

- **No secretly-imperative L4 escape on the cube-cavity assembly spine.**
  Every L4 operator on stages 1–3 either lowers cleanly to opset 18 or
  decomposes graph-only. This is a positive Phase F.1 outcome: the
  cube-cavity slice (real-symmetric, scalar Helmholtz, programmatic
  mesh) is a *clean* L4 reference. Friction surfaces are deferred
  to:
  - **Phase G** (Nédélec elements / curl operator): higher-order edge
    DOFs, RT/Nédélec lifts. To be audited when filed.
  - **Phase H** (PML, complex arithmetic): ONNX's narrow complex-tensor
    surface will be the next high-yield friction-mining target.

## Acknowledgements / cross-references

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — overall
  framing of cross-validated L4 lowerings, including the
  "friction-mining feedback loop" wording this audit deliverable
  inherits from.
- Friction tracker [#5](https://github.com/rjwalters/geode-fem/issues/5)
  — a roll-up of these findings is posted as a comment on #5 when this
  PR lands.
- JAX-side analog: [`reference/jax/cube_cavity.py`](../../jax/cube_cavity.py)
  and the JAX-DX notes in [`reference/jax/README.md`](../../jax/README.md).
- TF-Java analog: [`reference/tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/AssemblyGraph.java`](../../tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/AssemblyGraph.java).

## Re-running this audit

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt
python3 audit/probe_p1_local.py
python3 audit/probe_assembly_scatter.py
python3 audit/probe_dirichlet_mask.py
```

Each probe prints a one-screen verdict that should match the
corresponding row(s) in this table. If a probe's verdict disagrees
with the table after a version bump, the table is stale — re-audit.
