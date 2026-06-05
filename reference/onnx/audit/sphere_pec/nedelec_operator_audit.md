# Nédélec sphere-PEC ONNX expressibility audit (Phase G.6)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase G.6
deliverable. Tracking issue: [#135](https://github.com/rjwalters/geode-fem/issues/135).

## Scope

This audit catalogs the L4 operators on the **vector-Nédélec sphere-PEC
assembly spine** and records whether each lowers to a graph-only ONNX
form. The sphere-PEC case uses Whitney edge elements (Nédélec H(curl)),
making it structurally richer than the cube-cavity P1 scalar case audited
in Phase F.1 ([cube_cavity_operator_audit.md](../cube_cavity_operator_audit.md)).

The sphere-PEC assembly spine — as defined by the NumPy reference in
[`reference/numpy/sphere_pec.py`](../../../numpy/sphere_pec.py) and the
Burn implementation in `crates/geode-core/src/nedelec_assembly.rs` —
consists of these stages:

1. **ε_r assignment** (`build_epsilon_r`): per-tet relative permittivity
   from physical-group tags. Shape: `(n_tets,)`.
2. **Edge enumeration** (`build_edges`): build globally-oriented edge
   table `edges (n_edges, 2)` and per-tet edge-sign tables
   `tet_edge_idx (n_tets, 6)`, `tet_edge_sign (n_tets, 6)`. This is
   the Nédélec-specific preprocessing that has no P1 analog.
3. **Per-element Nédélec local matrices** (shape `(n_tets, 6, 6)`):
   cofactor-Gram construction, curl-curl stiffness K, ε-scaled mass M.
4. **PEC boundary mask** (`sphere_pec_interior_edges`): edge mask derived
   from node positions (ReduceL2 + threshold).
5. **Global K/M scatter-add** (shape `(n_edges, n_edges)`): scatter the
   per-element 6×6 signed blocks into a dense buffer.
6. **Dirichlet restriction** (`apply_dirichlet`): restrict K, M to the
   interior edges by row+column extraction.
7. **Generalized eigensolve**: `scipy.sparse.linalg.eigsh` on host
   (out of scope — same L4 boundary as cube-cavity).

The d⁰-rank spurious-mode classifier (`spurious_dim_from_derham`) is
host-imperative and out of scope for the same reason as the eigensolve.

This audit covers **stages 1–6** (stage 7 is shared L4 friction across
all backends).

## Pinned versions used for this audit

Same as Phase F.1:

| Package | Version |
|---|---|
| `onnx` | `1.21.0` |
| `onnxruntime` | `1.26.0` |
| Target opset | `18` |

See [`reference/onnx/requirements.txt`](../../requirements.txt) for the
pinned `pip` line.

## Classification convention

Inherits the Phase F.1 classification scheme:

| Marker | Meaning |
|---|---|
| **clean** | Direct opset-18 operator; lowers without any synthesis. |
| **synth** | No native op, but a graph-only synthesis from lower-level ops works. Documentation, not a blocker. |
| **caveat** | The op lowers, but introduces a downstream constraint (e.g. data-dependent shape). |
| **graph-only friction** | Forced by the ONNX IR's static-graph form; the L4 operator IS expressible, the friction is overhead. |
| **secretly imperative** | An L4 operator that relies on imperative escape; would block end-to-end lowering. **This is the high-value friction-mining outcome.** |

## Operator table

### Stage 1 — ε_r assignment

No probe required (this is a simple conditional broadcast: no ONNX
friction beyond `Where` opset-18 native). Inherits from Stage 1 of F.1
(scalar broadcast).

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `where(tags == PHYS_ID, n², 1.0)` | `Equal` + `Where` | clean | Native. |
| Scalar broadcast `(n_tets,)` | — | clean | No shape friction. |

### Stage 2 — Edge enumeration (`build_edges`)

Probe: [`probe_edge_enumeration.py`](probe_edge_enumeration.py).

#### Stage 2a — Local pair extraction + canonicalization (steps 1–2)

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `tets[:, la_arr]` (gather per-local-edge endpoints) | `Gather(axis=1)` | clean | Native; int64 indices. |
| `min(a, b)`, `max(a, b)` (orient lo-hi) | `Min`, `Max` | clean | Native elementwise. |
| `reshape(-1)` (flatten) | `Reshape` | clean | Native. |
| `stack([lo, hi], axis=1)` | `Unsqueeze` + `Concat` | graph-only friction | Same Stack synthesis as cube-cavity. |

Probe result: checker OK, runtime OK, max err = 0.000e+00.

#### Stage 2b — Deduplication + inverse map (steps 3–5)

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `np.unique(pair_flat, axis=0)` (deduplicate row-wise) | `Unique` (1D encoded form) | **secretly imperative** | `Unique` exists for 1D flat arrays; 2D row-dedup requires encoding as a scalar key. Output `n_edges` is **data-dependent** — the ONNX graph types it as `[None]`. More critically, the Python `dict` lookup `edge_to_idx[(lo, hi)]` (steps 4–5) has no ONNX operator equivalent. This is an imperative escape. |
| `{(lo,hi): i ...}` (hash-map construction) | **None** | **secretly imperative** | No ONNX operator for sorted-rank / inverse-permutation construction. |
| Per-tet for-loop with dict lookup (fill `tet_edge_idx`, `tet_edge_sign`) | **None** | **secretly imperative** | Data-dependent scatter with variable stride. Cannot be expressed in a static graph. |

#### Stage 2 verdict

**NOT EXPRESSIBLE** for steps 3–5. The local pair extraction (steps 1–2)
lowers cleanly, but the deduplication, hash-map inverse, and per-tet
sign-fill are fundamentally host-imperative.

**This is the most important finding of Phase G.6.** The P1 cube-cavity
assembly had no equivalent step — node DOFs map directly from the
connectivity table without deduplication. Edge DOFs require a
topological sort + inverse-map step that is **Nédélec-specific** and
has no analogue in the P1 reference. Every backend computes `build_edges`
outside any traced/compiled function boundary. ONNX makes this explicit.

**Design implication**: `edges (n_edges, 2)`, `tet_edge_idx (n_tets, 6)`,
and `tet_edge_sign (n_tets, 6)` must be **host-computed graph inputs**
for any Nédélec ONNX assembly graph.

### Stage 3 — Per-element Nédélec local matrices

Probe: [`probe_nedelec_local.py`](probe_nedelec_local.py).

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `v_i - v_j` (vertex subtraction) | `Sub` | clean | Same as P1. |
| `cross(a, b)` (3-vector cross product) | **synth**: `6 Mul + 3 Sub + 3 Unsqueeze + Concat` | graph-only friction | Same as P1 cube-cavity. No native `Cross`. |
| `einsum("eik,ejk->eij", g_mat, g_mat)` (cofactor Gram matrix) | `Einsum` | clean | Native opset 12+. **Preferred over MatMul+Transpose here** because the named-index form matches the formula in `nedelec.rs:199-210` one-for-one. |
| `Abs(det)` | `Abs` | clean | Native. |
| `ReduceSum` (det = sum along axis=1) | `ReduceSum` | clean | Axes-as-input form (opset 13+). |
| `Gather(gg, axis=1, idx=p)` + `Gather(axis=1, idx=q)` → `gg_pq` | `Gather` × 2 | clean | 4×4 Gram entry extraction for each of 36 edge pairs. |
| `gg_ac * gg_bd - gg_ad * gg_bc` (K_ij numerator) | `Mul` + `Sub` | clean | 2 muls + 1 sub per entry; 36 entries. |
| `f_ac * gg_bd - ... + f_bd * gg_ac` (M_ij terms) | `Mul` + `Sub` + `Add` × 2 | clean | Kronecker factors `f_pq` baked as scalar `Constant` nodes. 4 muls + 3 add/subs per entry; 36 entries. |
| `(2/3) / abs_det³` broadcast scale | `Div` + `Mul` × 2 + `Reshape` | clean | Scale precomputed as `inv_abs_det3`; reshaped to (N, 1, 1) for broadcast. |
| `1/(120 * abs_det)` broadcast scale | `Div` + `Reshape` | clean | Same broadcast pattern. |
| Stack 36 (N,) entries → (N, 6, 6) | `Unsqueeze` × 36 + `Concat` + `Reshape` | graph-only friction | The 36-entry `Unsqueeze + Concat` chain is verbose but graph-safe. Same disposition as Stage 1's 4-entry version in cube-cavity. |

Probe result: checker OK, runtime OK, max |K_onnx − K_numpy| = 0.000e+00,
max |M_onnx − M_numpy| = 0.000e+00 (bit-exact in f64).

#### Stage 3 — P1 vs. Nédélec structural comparison

| Dimension | P1 (cube-cavity) | Nédélec (sphere-PEC) |
|---|---|---|
| Output shape per element | (N, 4, 4) — 16 entries | (N, 6, 6) — 36 entries |
| Index structure | Node pairs (4×4) | Edge pairs via TET_LOCAL_EDGES (6×6) |
| Gram pivot | g_mat @ g_mat^T via MatMul+Transpose | same via `Einsum("eik,ejk->eij")` |
| Scale formula | `gg / (6 * |det|)` for K | `(2/3) * (gg_ac gg_bd - gg_ad gg_bc) / |det|³` for K |
| Mass formula | constant pattern × det/120 | four-Kronecker-delta terms / (120 |det|) |
| Graph-only friction | cross synth + Stack | same + 2.25× more Gather/Mul nodes |
| Secretly imperative | none | none |

**Stage 3 verdict: EXPRESSIBLE** — the 6×6 index structure is fixed for
all tets and baked as graph-level constants. No dynamic dispatch; the
graph size grows by ~2.25× vs. P1 but the classification is unchanged.

### Stage 4 — PEC boundary mask

Probe: [`probe_pec_mask.py`](probe_pec_mask.py).

#### Stage 4a — Mask computation (steps 1–4)

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `np.linalg.norm(nodes, axis=1)` (node radii) | `ReduceL2(axis=1)` | clean | Native opset 18. |
| `abs(r - r_outer) < tol` (boundary threshold) | `Sub` + `Abs` + `Less` | clean | Broadcast constant `r_outer`; all native. |
| `on_boundary[edges[:, 0]]` (endpoint flag lookup) | `Gather(axis=0)` | clean | Native; same as Dirichlet Gather in F.1 Stage 3. |
| `~(a_on & b_on)` (interior = not both on wall) | `And` + `Not` | clean | Native boolean ops. |

Probe result: checker OK, runtime OK, mask matches NumPy reference exactly.

#### Stage 4b — Index derivation (step 5 onward)

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `idx = np.flatnonzero(interior_mask)` | `NonZero` + `Squeeze` | **caveat** | Output shape is `(None,)` — data-dependent. Propagates `(None, None)` shape downstream to K_int, M_int. Same classification as cube-cavity Stage 3 (F.1). |
| `K[idx, :][:, idx]` with precomputed `idx` | `Gather(axis=0)` + `Gather(axis=1)` | clean | If `idx` is a graph input (host-computed), this lowers cleanly. Recommended design. |

**Stage 4 verdict: EXPRESSIBLE** — the mask computation (steps 1–4)
lowers cleanly. Deriving `idx` in-graph via NonZero introduces a
data-dependent shape (caveat, not secretly-imperative). Recommended
design: host-compute `interior_idx` from the mask and pass as a graph
input, mirroring the JAX and TF-Java conventions.

**New finding vs. P1**: the Nédélec PEC mask IS derivable *inside* the
ONNX graph from node positions (ReduceL2 + threshold), unlike the P1
case where the Dirichlet mask comes from an external tag. The mask
computation itself is graph-only; only the index extraction breaks
static shape. This is a qualitative improvement over the P1 case (where
the entire mask was typically precomputed on the host).

### Stage 5 — Global K/M scatter-add

Probe: [`probe_nedelec_scatter.py`](probe_nedelec_scatter.py).

#### Stage 5 — Static-shape version (n_edges known at build time)

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `sign[e,i] * sign[e,j]` (outer product) | `Unsqueeze(ax=2)` + `Unsqueeze(ax=1)` + `Mul` | clean | Broadcasting (N,6,1) × (N,1,6) → (N,6,6). New vs. P1. |
| `k_local * sign_outer` (sign correction) | `Mul` | clean | Native elementwise. |
| `m_local * sign_outer * eps` (sign + ε scaling) | `Mul` × 2 + `Reshape` | clean | `eps` reshaped to (N,1,1) for broadcast. |
| `tet_edge_idx[:, :, None]` broadcast → rows | `Unsqueeze(ax=2)` + `Expand` | clean | Native. |
| `tet_edge_idx[:, None, :]` broadcast → cols | `Unsqueeze(ax=1)` + `Expand` | clean | Native. |
| Flatten rows, cols, vals to 1-D | `Reshape` | clean | Native. |
| Build `(M, 2)` index table | `Unsqueeze` + `Concat` | clean | Same pattern as cube-cavity. |
| `np.zeros((n_edges, n_edges))` zero buffer | `ConstantOfShape` | clean | Native opset 9+. |
| `buf.at[rows, cols].add(vals)` scatter-add | `ScatterND(reduction="add")` | clean | Same as cube-cavity; the critical op for phase assembly. |

Probe result: checker OK, runtime OK, max |K_onnx − K_numpy| = 0.000e+00,
max |M_onnx − M_numpy| = 0.000e+00 (bit-exact).

#### Stage 5 — Dynamic-shape friction (real pipeline)

The static-shape test passes perfectly. In the real pipeline, `n_edges`
is the output of `build_edges` (stage 2: NOT EXPRESSIBLE) and is a
runtime value not known at graph-build time. This means:

| Sub-issue | Impact |
|---|---|
| `n_edges` unknown at graph-build time | `ConstantOfShape` must accept a dynamic shape tensor; buffer is typed `(n_edges, n_edges)` where `n_edges` is `None` from static-shape inference. |
| K_global, M_global output types | `(None, None)` downstream; breaks compile-time tensor-shape validation. |
| Downstream Dirichlet Gather | Inherits the dynamic first axis from K_global. |

**Stage 5 verdict: PARTIAL** — the scatter-add mechanics (ScatterND +
sign outer product) lower cleanly when `n_edges` is a static constant.
In the real pipeline, `n_edges` is data-dependent (from `build_edges`),
which propagates a dynamic-shape axis through the global matrices.
This is graph-only friction (not secretly-imperative — the scatter
mechanism itself is correct), inherited from the `build_edges` escape.

### Stage 6 — Dirichlet restriction (interior edge set)

No separate probe required; this is structurally identical to the cube-
cavity Stage 3 (F.1), applied to edges instead of nodes.

| Operator (L4 / NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `K[interior_idx, :]` with precomputed `idx` | `Gather(axis=0)` | clean | Same as F.1. |
| `K[:, interior_idx]` | `Gather(axis=1)` | clean | Same as F.1. |
| `idx = np.flatnonzero(interior_mask)` | `NonZero` + `Squeeze` | caveat | Data-dependent shape; same as F.1. |

Design recommendation: identical to cube-cavity. Accept `interior_idx
(n_int,) int64` as a host-computed graph input. This keeps the
restricted matrices `K_int (n_int, n_int)`, `M_int (n_int, n_int)`
statically-shaped at the eigensolve boundary.

Note: there is an additional complication not present in P1 — `n_int`
(number of interior edges) depends on `n_edges` (which is already
data-dependent from `build_edges`). Even with `interior_idx` as a
graph input, the shape of `K_int` depends on `len(interior_idx)` which
is a runtime value. This means `K_int` is `(None, None)` regardless.
The recommended mitigation is to specialize the graph per mesh (baking
`n_edges` and `n_int` as constants at graph generation time), which is
how the cube-cavity end-to-end graph in Phase F.2 is constructed.

### Stage 7 — Generalized eigensolve (boundary, shared L4 friction)

Not in this audit. Same disposition as cube-cavity Stage 4: all backends
offload to SciPy ARPACK or an equivalent on the host. ONNX has no
`Eigh` op; this is shared L4 friction across the reference set.

## Cross-cutting frictions: F.1 vs. G.6 comparison

| Friction | F.1 (cube-cavity P1) | G.6 (sphere-PEC Nédélec) | Change |
|---|---|---|---|
| No `Stack` op (`Unsqueeze + Concat`) | graph-only | graph-only | **Unchanged** |
| Int64 axes required | graph-only | graph-only | **Unchanged** |
| `MatMul` broadcasts batch dim | clean | — (uses `Einsum` instead) | **Einsum is cleaner for named-index formulas** |
| `ScatterND(reduction="add")` | clean | clean | **Unchanged** |
| `NonZero` (mask→idx) introduces data-dep shape | caveat | caveat | **Unchanged; same recommendation** |
| **`build_edges` deduplication + inverse map** | **N/A (no edge DOFs)** | **secretly imperative** | **NEW in G.6 — Nédélec-specific escape** |
| **Sign outer product (s_i s_j correction)** | **N/A** | graph-only friction | **NEW in G.6 — expressible but more complex** |
| **Global buffer size `(n_edges, n_edges)`** | static (n_nodes known) | **data-dependent** | **NEW in G.6 — inherited from build_edges** |
| Complex arithmetic | deferred (P1 real-symmetric) | deferred (PEC no PML) | **Unchanged** |

## Per-stage verdicts (one-line summary)

| Stage | Verdict |
|---|---|
| 1. ε_r assignment | clean (Equal + Where) |
| 2a. Local pair extraction | clean (Gather + Min/Max) |
| 2b. Deduplication + inverse map | NOT EXPRESSIBLE — secretly-imperative escape (dict lookup + topological sort) |
| 3. Nédélec local 6×6 matrices | EXPRESSIBLE — same graph-only friction as P1; 2.25× more nodes |
| 4a. PEC mask computation | EXPRESSIBLE — ReduceL2 + threshold + boolean ops |
| 4b. PEC mask → interior idx | caveat — NonZero introduces data-dependent shape (same as F.1 Stage 3) |
| 5. Global K/M scatter-add (static n_edges) | EXPRESSIBLE — ScatterND(reduction="add") + sign outer product |
| 5. Global K/M scatter-add (real pipeline) | PARTIAL — data-dependent shape from n_edges; scatter mechanics are correct |
| 6. Dirichlet restriction | clean (two Gathers) if interior_idx is host-computed; caveat if derived in-graph |
| 7. Eigensolve | out of scope — shared L4 friction |

## What this audit found (summary for L4 calculus)

### New findings vs. Phase F.1

**1. `build_edges` is the first secretly-imperative L4 escape on the
sphere-PEC assembly spine.** The cube-cavity P1 audit found no
secretly-imperative operators. The Nédélec case immediately hits one in
stage 2: the deduplication + inverse-map construction is not expressible
in any static-graph IR (ONNX, XLA, or TF-Java). This is not
ONNX-specific — it is inherent to the L4 Nédélec abstraction.
The edge DOF table is a combinatorial artifact of H(curl) elements.

**2. The global buffer size becomes data-dependent.** In P1, `n_nodes`
is a static integer (the number of input vertices). In Nédélec,
`n_edges` is the output of `build_edges`, which is not expressible in
the graph. This cascades: the zero buffer, K_global, M_global, and
K_int/M_int all have a dynamic first axis in the real pipeline.

**3. The sign outer product is a new friction surface.** The Nédélec
assembly requires a per-tet s_i × s_j outer product (from `tet_edge_sign`)
applied to the 6×6 local blocks. This IS expressible (via Unsqueeze +
Mul broadcasting) but is absent from the P1 case. Classification:
graph-only friction.

**4. The PEC mask computation is *more* graph-expressible than the P1
Dirichlet mask.** In the cube-cavity case, the Dirichlet mask comes from
an external tagging (not computed in the graph). The Nédélec PEC mask
is derived from node positions inside the ONNX graph (ReduceL2 +
threshold), which IS graph-expressible. Only the subsequent idx
derivation breaks static shape — and the same caveat applies to both
cases.

### What is expressible in the Nédélec ONNX graph

Given pre-computed host inputs `edges`, `tet_edge_idx`, `tet_edge_sign`,
and `interior_idx` (and static constants `n_edges`, `n_int` baked at
graph-generation time):

- Per-element 6×6 K_local and M_local: fully expressible (Stage 3).
- Sign correction and ε scaling: fully expressible (Stage 5).
- COO index construction and scatter-add: fully expressible (Stage 5).
- PEC mask computation: expressible for the boolean mask (Stage 4a).
- Dirichlet restriction with precomputed idx: fully expressible (Stage 6).

### What is not expressible (must be host-computed)

- `build_edges` (Stage 2b): deduplication, inverse map, sign fill.
  These are host inputs: `edges`, `tet_edge_idx`, `tet_edge_sign`.
- `interior_idx` derivation from the boolean mask: host-computed via
  `np.flatnonzero(interior_mask)`.
- `spurious_dim_from_derham` (SVD-based rank computation): host imperative.
- Eigensolve: host imperative (shared L4 friction across all backends).

## Recommended design for a future Nédélec ONNX assembly graph

Graph inputs:
- `nodes (n_nodes, 3) float64` — mesh vertex coordinates
- `tets (n_tets, 4) int64` — tet connectivity
- `tet_edge_idx (n_tets, 6) int64` — HOST-COMPUTED (from `build_edges`)
- `tet_edge_sign (n_tets, 6) float64` — HOST-COMPUTED (from `build_edges`)
- `epsilon_r (n_tets,) float64` — HOST-COMPUTED (from `build_epsilon_r`)
- `interior_idx (n_int,) int64` — HOST-COMPUTED (from `flatnonzero(pec_mask)`)

Graph constants (baked at generation time, not from host inputs):
- `n_edges: int` — from `len(edges)` (output of `build_edges`)
- `n_int: int` — from `len(interior_idx)` (output of `flatnonzero`)

Graph outputs:
- `K_int (n_int, n_int) float64`
- `M_int (n_int, n_int) float64`

Eigensolve seam: same JSON-sidecar pattern as Phase F.2. Host driver
calls `scipy.sparse.linalg.eigsh` on the ONNX-produced K_int, M_int.

## Acknowledgements / cross-references

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — overall
  framing and Phase G.6 scope.
- Phase F.1 analog: [`cube_cavity_operator_audit.md`](../cube_cavity_operator_audit.md)
  — the baseline comparison for all F.1 vs. G.6 rows above.
- Friction tracker [#5](https://github.com/rjwalters/geode-fem/issues/5)
  — a roll-up of these findings is posted as a comment on #5 when this
  PR lands.
- NumPy sphere-PEC reference: [`reference/numpy/sphere_pec.py`](../../../numpy/sphere_pec.py).
- NumPy Nédélec local matrices: [`reference/numpy/nedelec_local_matrices.py`](../../../numpy/nedelec_local_matrices.py).
- Burn-side kernel: `crates/geode-core/src/nedelec_assembly.rs`.

## Re-running this audit

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt
python3 audit/sphere_pec/probe_nedelec_local.py
python3 audit/sphere_pec/probe_edge_enumeration.py
python3 audit/sphere_pec/probe_nedelec_scatter.py
python3 audit/sphere_pec/probe_pec_mask.py
```

Each probe prints a one-screen verdict that should match the
corresponding row(s) in this table. If a probe's verdict disagrees
with the table after a version bump, the table is stale — re-audit.

## Phase G.7 — empirical confirmation of the recommended design

The Phase G.7 implementation
([`reference/onnx/sphere_pec/assembly_graph.py`](../../sphere_pec/assembly_graph.py),
issue [#140](https://github.com/rjwalters/geode-fem/issues/140))
realises the recommended design verbatim — host-computed
`edges`, `tet_edge_idx`, `tet_edge_sign`, `epsilon_r`, and
`interior_idx`; `n_edges` baked at graph generation time; `n_int`
left dynamic in the Dirichlet Gather. The driver
[`reference/onnx/sphere_pec/gen_sphere_pec_reduced.py`](../../sphere_pec/gen_sphere_pec_reduced.py)
runs this graph end-to-end on the bundled 774-node sphere fixture and
emits a schema-v1 sidecar consumed by
[`reference/driver/eigensolve_sphere_pec_sidecar.py`](../../../driver/eigensolve_sphere_pec_sidecar.py).

Empirical results on the canonical fixture (n_nodes=774, n_tets=3335,
n_edges=4512, n_int=3300):

| Cross-check | ONNX vs NumPy baseline |
|---|---|
| `K_int` Frobenius norm | **bit-exact** (abs diff = 0.000e+00) |
| `M_int` Frobenius norm | **bit-exact** (abs diff = 0.000e+00) |
| `K_int` sorted-diagonal max-abs | 3.2e-14 (f64 noise floor) |
| `M_int` sorted-diagonal max-abs | 1.1e-16 (f64 noise floor) |
| Physical eigenvalues λ[1..5] | rel err ≤ 2.5e-9 (G.7 AC: 2e-5 rel) |
| `on_boundary` mask | exact match (3 mask bits per audit Stage 4a) |

The scatter-add accumulation order concern flagged by Phase G.5 (where
the TF-Java `scatterNd` produced ~1.2e-5 relative drift on the lowest
eigenvalue, motivating a tolerance loosening from 1e-5 → 2e-5) does
**not** appear in the ONNX path: onnxruntime's `ScatterND` reduction
mode evidently produces the same accumulation order as scipy's
COO→CSR collapse on this fixture (or at least one whose f64 ULP error
is dominated by floating-point rounding in the local matrix arithmetic,
not in the scatter). The strict 1e-5 cross-IR floor holds with margin.

**Verdict confirmation**: the recommended design works end-to-end on
the sphere fixture with f64 numerical behavior indistinguishable from
the NumPy baseline. Stages 1, 3, 4a, 5 (static `n_edges`), and 6 of
this audit are realised in the resulting partial graph. Stages 2b and
4b remain host-side (the audit's central findings).

See [issue #140](https://github.com/rjwalters/geode-fem/issues/140) for
the Phase G.7 PR and full empirical numbers.
