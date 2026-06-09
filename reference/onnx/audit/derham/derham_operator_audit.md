# Discrete de Rham ONNX expressibility audit (Phase I.3)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase I.3
deliverable. Tracking issue:
[#169](https://github.com/rjwalters/geode-fem/issues/169).

## Scope

This audit catalogs the operators of the **discrete de Rham chain**
(d⁰ gradient, d¹ curl, d² divergence) — as defined by the NumPy
reference in [`reference/numpy/derham.py`](../../../numpy/derham.py)
and the Burn implementation in `crates/geode-core/src/derham.rs` —
and records whether each lowers to a graph-only ONNX opset-18 form.

Unlike the previous audits (F.1 cube-cavity, G.6 sphere-PEC, H.5
sphere-PML), which walked a single assembly *pipeline*, this audit
deliberately splits each operator into two distinct questions:

1. **Construction** — edge/face enumeration, row deduplication,
   inverse-index maps, orientation signs. Expected verdict:
   host-side, confirming that the "topology belongs at the L4-input
   boundary" rule ([#5](https://github.com/rjwalters/geode-fem/issues/5),
   2026-06-05 comment; G.6 `build_edges` finding) generalizes from a
   Nédélec-assembly one-off to the full d-chain.
2. **Application** — `d⁰·φ`, `d¹·v`, `d²·w` as sparse matvecs over
   host-pre-assembled index tables. Question: is *application*
   graph-pure under opset 18 even though construction is not?

**Headline finding**: yes — the split is clean. Every construction
stage is **blocked** (host-side) by exactly the G.6 friction class
(data-dependent dedup shapes + hash-map inverse lookups), and every
application stage is **emittable** with zero friction: the de Rham
matrices are integer `{-1, 0, +1}` incidence matrices, so the c128
frictions that dominated H.5 never arise, and the int64 type path
through `Gather` / `Sub` / `Add` / `Mul` / `ConstantOfShape` /
`ScatterND(reduction="add")` is fully supported by opset 18 and
onnxruntime. The composed exactness identity `d¹ ∘ d⁰ ≡ 0` holds
**in-graph, bit-exactly**, on the full bundled sphere fixture.

## Pinned versions used for this audit

Same as Phase F.1 / G.6 / H.5:

| Package | Version |
|---|---|
| `onnx` | `1.21.0` |
| `onnxruntime` | `1.26.0` |
| Target opset | `18` |

See [`reference/onnx/requirements.txt`](../../requirements.txt) for the
pinned `pip` line. The exactness probe additionally needs `scipy` and
`meshio` from [`reference/numpy/requirements.txt`](../../../numpy/requirements.txt)
(it loads the bundled `.msh` fixture through `derham.py`).

## Classification convention

Same as H.5 (issue #157 convention):

| Marker | Meaning |
|---|---|
| **emittable** | Op lowers directly to opset 18 with no special handling. |
| **fallback** | Op cannot lower as-typed; a documented alternative lowering exists. |
| **blocked** | Op has no lowering — must be host-computed and passed as a graph input or executed in an out-of-graph sidecar. |

## Construction stages

No new probes were written for construction: every stage below is an
instance of the friction class already demonstrated empirically by
[`probe_edge_enumeration.py`](../sphere_pec/probe_edge_enumeration.py)
(G.6), which showed that `np.unique(..., axis=0)` has a data-dependent
output shape (ONNX `Unique` types it `[None]`) and that the
sorted-unique-**inverse** map (the Python dict) has no ONNX operator
equivalent. The de Rham constructors are structurally identical:

### Stage C1 — `build_edges` (d⁰ row space)

Inherited verbatim from G.6. Pair canonicalization (`Gather` + `Min` +
`Max`) is emittable; `np.unique(pairs, axis=0)` and the
`edge_to_idx` dict are not.

**Stage C1 verdict: BLOCKED (host-side).** Unchanged from G.6 /
PR [#142](https://github.com/rjwalters/geode-fem/pull/142).

### Stage C2 — `build_faces` (d¹ row space, d² column space)

| Operator (NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `tets[:, local_face]` per slot | `Gather(axis=1)` | **emittable** | Same as the G.6 pair extraction. |
| `np.sort(tri, axis=1)` (3-wide ascending) | `TopK`/`Sort`-free: `Min`/`Max`/median composition | **emittable** | Verbose but static-shape; graph-only friction. |
| `np.unique(triples, axis=0)` | **none** | **blocked** | Same data-dependent-shape friction as `build_edges`: `n_faces` is a topological property of the mesh. The 1-D `Unique` workaround (encode each triple as a scalar key) still yields a `[None]`-shaped output. |

**Stage C2 verdict: BLOCKED (host-side).** Same friction class as C1;
the dedup key is a vertex *triple* instead of a pair, which changes
nothing.

### Stage C3 — `curl_map` column indices (`edge_to_idx` lookup)

The d¹ COO column indices are `edge_to_idx[(a,b)]`,
`edge_to_idx[(b,c)]`, `edge_to_idx[(a,c)]` per face — a hash-map
inverse of the deduplicated edge table.

**Stage C3 verdict: BLOCKED (host-side).** Identical to the G.6
step-4/5 finding (no sorted-unique-with-inverse operator). The data
values of d¹ (`+1, +1, -1` per row) are a fixed pattern and would be
trivially emittable — but they are useless without the blocked column
indices, so the whole constructor is host-side.

### Stage C4 — `build_tet_faces` / `divergence_map` (face lookup + parity signs)

| Operator (NumPy verb) | ONNX expression | Status | Notes |
|---|---|---|---|
| `face_to_idx[sorted(local)]` | **none** | **blocked** | Hash-map inverse of `build_faces`, same as C3. |
| `_triple_permutation_sign` (3-element parity) | comparison ladder (`Less`/`Where`) | **emittable** | A fixed-depth comparator network; graph-only friction. |
| `(-1)^k` alternation | baked `Constant` | **emittable** | Static per-slot constant. |

**Stage C4 verdict: BLOCKED (host-side)** — the parity arithmetic is
emittable, but it is downstream of the blocked `face_to_idx` lookup.

### Construction summary

All four constructors fail on the **same two ops** that blocked
`build_edges` in G.6: row-deduplication with data-dependent output
shape, and hash-map inverse lookup. Nothing about the d-chain adds a
*new* construction friction, and nothing about it removes one. The
"topology belongs at the L4-input boundary" rule is therefore **not**
a Nédélec-assembly one-off — it is the general disposition for every
combinatorial-topology constructor in the codebase.

## Application stages

Probes:
[`probe_d0_apply.py`](probe_d0_apply.py),
[`probe_d1_apply.py`](probe_d1_apply.py),
[`probe_exactness_in_graph.py`](probe_exactness_in_graph.py).

Each operator is probed in two graph forms:

- **Structured incidence form** — exploits the fixed per-row nnz
  pattern (2 for d⁰, 3 for d¹, 4 for d²): pure `Gather` + `Add`/`Sub`
  on the host-provided index table, no scatter at all.
- **Generic COO matvec** — `y = ScatterND_add(zeros(n_rows),
  rows[:, None], vals · Gather(x, cols))`, consuming the COO triplets
  of the host-assembled matrix as plain graph inputs. This is the
  form that generalizes to *any* pre-assembled sparse operator.

### Stage A1 — d⁰ application (`g = d⁰ · φ`)

Probe: [`probe_d0_apply.py`](probe_d0_apply.py).

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `edges[:, 0]` / `edges[:, 1]` column split | `Gather(axis=1)` with scalar index | **emittable** | Scalar index drops the axis — no `Squeeze` needed. |
| `φ[heads]`, `φ[tails]` | `Gather(axis=0)` | **emittable** | int64 and f64. |
| `φ[b] - φ[a]` | `Sub` | **emittable** | int64 and f64. Bit-exact (`0.000e+00`) vs. NumPy in both dtypes. |
| `vals * φ[cols]` (COO form) | `Mul` | **emittable** | int64 and f64. |
| Zero buffer `(n_edges,)` | `ConstantOfShape` | **emittable** | **int64 fill is supported** (unlike the c128 fill blocked in H.5 Stage 5). |
| Accumulate | `ScatterND(reduction="add")` | **emittable** | **int64 reduction works in onnxruntime 1.26** — bit-exact vs. scipy. |

**Stage A1 verdict: EMITTABLE (graph-pure)**, both forms, both dtypes.

### Stage A2 — d¹ application (`c = d¹ · v`)

Probe: [`probe_d1_apply.py`](probe_d1_apply.py).

Host-side input: `face_edge_idx (n_faces, 3) int64` with columns
`[edge(a,b), edge(b,c), edge(a,c)]` (the C3 dict lookup output), or
equivalently the `curl_map` COO triplets.

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| Three column splits of `face_edge_idx` | `Gather(axis=1)` scalar index | **emittable** | |
| `v[e_ab] + v[e_bc] - v[e_ac]` | `Gather` × 3 + `Add` + `Sub` | **emittable** | The `+1, +1, -1` sign pattern is baked into the op choice — no sign tensor needed in the structured form. Bit-exact in int64 and f64. |
| Generic COO matvec on d¹ | `Gather` + `Mul` + `ConstantOfShape` + `ScatterND(add)` | **emittable** | Bit-exact vs. scipy in both dtypes. |

**Stage A2 verdict: EMITTABLE (graph-pure)**, both forms, both dtypes.

### Stage A3 — d² application (`s = d² · w`)

Probed in [`probe_d1_apply.py`](probe_d1_apply.py) Part (C): the
**identical** generic COO matvec builder runs on the `divergence_map`
triplets with zero modification (4 nnz per row, signs
`(-1)^k · sign_k` live in the host-provided `vals` tensor). Bit-exact
vs. scipy in int64 and f64.

**Stage A3 verdict: EMITTABLE (graph-pure).** Application
expressibility is a property of the COO input contract, not of any
particular d-operator.

### Stage A4 — composed in-graph exactness `d¹ · (d⁰ · φ) ≡ 0`

Probe: [`probe_exactness_in_graph.py`](probe_exactness_in_graph.py),
executed against the bundled sphere fixture
([`reference/fixtures/sphere_pec/sphere.msh`](../../../fixtures/sphere_pec/sphere.msh):
774 nodes, 4512 edges, 7074 faces, 3335 tets) after cross-checking the
cell counts against the [#149](https://github.com/rjwalters/geode-fem/issues/149)
baseline ([`reference/fixtures/derham/baseline.json`](../../../fixtures/derham/baseline.json),
fixture_id `derham/sphere_n774_d0_d1_d2`).

The assertion is a **graph node**, not a host-side comparison:

| Operator | ONNX expression | Status | Notes |
|---|---|---|---|
| `d⁰·φ` then `d¹·g` chained | (Stages A1 + A2 composed) | **emittable** | Both the structured form and the two-`ScatterND` COO form compose with no friction. |
| `max abs(residual)` | `Abs` + `ReduceMax(keepdims=0)` | **emittable** | int64 supported. |
| `== 0` assert | `Equal` against an int64 `Constant` | **emittable** | Emits a scalar `BOOL` graph output. |

Measured results (onnxruntime 1.26.0, full sphere fixture, random
int64 nodal field in `[-10⁶, 10⁶)`):

| Graph | `max abs d¹·(d⁰·φ)` | in-graph `exact` bool |
|---|---|---|
| Structured chain, int64 | `0` | `True` |
| Composed COO matvec chain (`ScatterND` × 2), int64 | `0` | `True` |
| Structured chain, **f64 control** | `8.882e-16` | `False` |

The f64 control row is a finding, not a failure: `g = fl(φ_b − φ_a)`
is already rounded, so the telescoping cancellation in the d¹ row only
holds to roundoff — in *any* backend, not just ONNX. The bit-exact
in-graph assert therefore **must** run on the integer channel, which
is exactly the dtype contract #149 fixed for the de Rham baselines
("bit-exact integer cross-check, no floating-point tolerance
question"). Opset 18's full int64 support for `Gather` / `Sub` /
`Add` / `Mul` / `ScatterND(add)` / `Abs` / `ReduceMax` / `Equal` is
what makes this possible.

**Stage A4 verdict: EMITTABLE (graph-pure).** The exactness identity
of the discrete de Rham complex survives lowering to a pure opset-18
graph.

## Per-stage verdicts (one-line summary)

| Stage | Verdict |
|---|---|
| C1. `build_edges` | **blocked** — host-side (inherited G.6; dedup + inverse map) |
| C2. `build_faces` | **blocked** — host-side (same friction, triple keys) |
| C3. `curl_map` column indices (`edge_to_idx`) | **blocked** — host-side (hash-map inverse) |
| C4. `build_tet_faces` / `divergence_map` indices | **blocked** — host-side (hash-map inverse; parity signs alone would be emittable) |
| A1. d⁰ application | **emittable** — `Gather` + `Sub`, or generic COO matvec; bit-exact int64 + f64 |
| A2. d¹ application | **emittable** — `Gather` × 3 + `Add` + `Sub`, or COO matvec; bit-exact |
| A3. d² application | **emittable** — same COO matvec builder, unmodified |
| A4. in-graph `d¹∘d⁰ ≡ 0` | **emittable** — holds bit-exactly under onnxruntime on the full sphere fixture (int64 channel) |

## Spec implications (L4 input boundary)

1. **The input-boundary rule generalizes.** Every de Rham constructor
   is blocked by the *same two ops* as `build_edges` (data-dependent
   dedup + hash-map inverse). "Topology belongs at the L4-input
   boundary" is the uniform disposition for the whole d-chain, not a
   Nédélec special case. An L4 spec can state it once, as a rule
   about *combinatorial topology constructors*, rather than
   enumerating per-operator exceptions.
2. **The d-operators can live in the traced L4 surface.** Application
   is graph-pure provided the operator arrives as a pre-assembled
   host artifact. Two equivalent input contracts work, and both are
   shapes the spec has already paid for:
   - *structured index tables* (`edges (n_edges, 2)`,
     `face_edge_idx (n_faces, 3)`, `tet_face_idx (n_tets, 4)` +
     sign tensor) — the same shape as the Phase G.7
     `tet_edge_idx` / `tet_edge_sign` contract;
   - *generic COO triplets* (`rows`, `cols`, `vals`) — one
     operator-agnostic matvec subgraph serves d⁰, d¹, d², and any
     future pre-assembled sparse operator.
3. **The integer channel is load-bearing.** The #149 "bit-exact
   integer cross-check" contract transfers into the graph unchanged
   because opset 18 supports int64 end-to-end, including
   `ScatterND(reduction="add")` and `ConstantOfShape` with int64
   fill. This is the mirror image of the H.5 finding: where c128 is
   a vestigial type with no kernel support, **int64 is a first-class
   citizen** — the de Rham chain dodges every H.5 friction by being
   integer-valued.
4. **f64 exactness is roundoff-bounded everywhere.** The f64 control
   (residual `8.9e-16` on unit-scale fields) documents that an
   in-graph exactness *assert* must use the integer dtype; an f64
   d-chain embedded in a physics graph should treat `d¹∘d⁰` as zero
   only to tolerance. This is backend-independent arithmetic, not an
   ONNX limitation, but it belongs in the spec's conformance-test
   wording.

## Cross-cutting frictions: G.6/H.5 vs. I.3 comparison

| Friction | G.6 / H.5 status | I.3 (de Rham) status | Change |
|---|---|---|---|
| Dedup with data-dependent shape (`np.unique` axis=0) | blocked (`build_edges`) | blocked (`build_edges`, `build_faces`) | **Unchanged** — now known to cover the full d-chain |
| Hash-map inverse lookup | blocked (`edge_to_idx`) | blocked (`edge_to_idx`, `face_to_idx`) | **Unchanged** |
| `ScatterND(reduction="add")` f64 | emittable | emittable | **Unchanged** |
| `ScatterND(reduction="add")` **int64** | untested | **emittable** (probed, bit-exact) | **NEW in I.3** |
| `ConstantOfShape` int64 fill | untested | **emittable** | **NEW in I.3** |
| int64 `Abs`/`ReduceMax`/`Equal` (in-graph assert) | untested | **emittable** | **NEW in I.3** |
| c128 type path | blocked (H.5 headline) | not applicable — d-chain is integer-valued | — |

## Re-running this audit

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt -r ../numpy/requirements.txt
python3 audit/derham/probe_d0_apply.py
python3 audit/derham/probe_d1_apply.py
python3 audit/derham/probe_exactness_in_graph.py
```

Each probe prints a one-screen verdict that should match the
corresponding row(s) in this table and exits nonzero if any bit-exact
check fails. If a probe's verdict disagrees with the table after a
version bump, the table is stale — re-audit. The construction
verdicts (C1–C4) are inherited from the G.6 probe; re-run
`audit/sphere_pec/probe_edge_enumeration.py` to refresh them.

## Acknowledgements / cross-references

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — overall
  framing; Phase I.3 scope.
- Issue [#149](https://github.com/rjwalters/geode-fem/issues/149) — the
  NumPy de Rham reference and integer baseline this audit consumes
  ([`reference/numpy/derham.py`](../../../numpy/derham.py),
  [`reference/fixtures/derham/baseline.json`](../../../fixtures/derham/baseline.json)).
- Issue [#5](https://github.com/rjwalters/geode-fem/issues/5) — the
  L4-input-boundary rule this audit confirms and generalizes.
- Phase G.6: [`nedelec_operator_audit.md`](../sphere_pec/nedelec_operator_audit.md)
  — origin of the `build_edges` "secretly imperative" finding and the
  construction friction class inherited here.
- Phase H.5: [`nedelec_pml_operator_audit.md`](../sphere_pml/nedelec_pml_operator_audit.md)
  — the c128 headline this audit's int64 finding mirrors.
- Burn-side source of truth: `crates/geode-core/src/derham.rs`
  (`gradient_map`, `curl_map`, `divergence_map`) and
  `crates/geode-core/src/mesh/mod.rs` (`TET_LOCAL_EDGES`,
  `TET_LOCAL_FACES`).
