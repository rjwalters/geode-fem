# ONNX reference implementations

Graph-only constraint check per **Epic [#88](https://github.com/rjwalters/geode-fem/issues/88)**.
Forces the L4 calculus through a graph-only expressibility constraint,
exposing primitives that are secretly imperative — and, when the
forcing is positive, surfacing cross-backend conventions (like "compute
`idx` on the host") that none of the other reference backends needed
to articulate explicitly.

## Status

Phase F is split into two sequenced issues under Epic #88:

- **Phase F.1 — expressibility audit (issue [#116](https://github.com/rjwalters/geode-fem/issues/116))**:
  the cube-cavity operator-by-operator audit. Lives under
  [`audit/`](audit/) — Markdown gap-list plus three runnable probe
  scripts. No CI gate; the audit *is* the deliverable.
- **Phase F.2 — end-to-end cube-cavity ONNX assembly graph
  (issue [#123](https://github.com/rjwalters/geode-fem/issues/123))**:
  shipped. The runtime payload lives in [`cube_cavity/`](cube_cavity/),
  the sidecar driver is
  [`reference/driver/eigensolve_from_onnx.py`](../driver/eigensolve_from_onnx.py),
  and the CI gate is
  [`.github/workflows/onnx-cube-cavity.yml`](../../.github/workflows/onnx-cube-cavity.yml)
  — modeled on `tfjava-cube-cavity.yml` and aligned with the n=10
  convention of the canonical NumPy baseline.

## Boundary conventions inherited from the reference set

These decisions are not ONNX-specific — they are convention-set across
the Epic #88 reference backends and ONNX inherits them unchanged.

- **Eigensolve lives in a host driver**, not inside the ONNX graph.
  Same seam as TF-Java (which has no `Eig` op and emits a JSON sidecar
  for SciPy ARPACK) and JAX (which puts `scipy.linalg.eigh` outside
  the jit boundary). ONNX has no `Eigh` op either, but even if it did,
  the convention would not change. See the Epic #88 Phase C wording:
  *"differentiability of assembly tested (eigensolve boundary
  allowed)"*.
- **Complex arithmetic is deferred to the PML phase (Phase H).** The
  cube-cavity case is real-symmetric throughout. ONNX's narrow
  complex-tensor surface is the highest-yield friction-mining target
  for PML, but is **not** exercised here. Filing that friction artifact
  belongs to Phase H, not F.
- **Dirichlet `idx` is host-computed.** This is one of the few
  conventions Phase F.1 made *explicit* — see the audit table for the
  reasoning. JAX and TF-Java already follow this convention implicitly;
  ONNX's static-shape contract forces us to name it.

## Layout

```
reference/onnx/
├── README.md                          — this file
├── requirements.txt                   — pinned onnx + onnxruntime + onnxscript
├── audit/                             — Phase F.1 deliverable
│   ├── cube_cavity_operator_audit.md  — the gap-list / operator inventory
│   ├── probe_p1_local.py              — per-element P1 local matrices
│   ├── probe_assembly_scatter.py      — global K/M scatter-add (ScatterND)
│   └── probe_dirichlet_mask.py        — interior-DOF restriction (NonZero)
└── cube_cavity/                       — Phase F.2 runtime payload
    ├── README.md                      — graph design + reproduction notes
    ├── assembly_graph.py              — analog of tf_java/.../AssemblyGraph.java
    └── gen_cube_cavity_reduced.py     — emits reduced_kM.json sidecar
```

## Re-running the Phase F.1 probes

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt
python3 audit/probe_p1_local.py
python3 audit/probe_assembly_scatter.py
python3 audit/probe_dirichlet_mask.py
```

Each probe ends with a one-screen verdict. The verdicts roll up into
the audit table in [`audit/cube_cavity_operator_audit.md`](audit/cube_cavity_operator_audit.md).
If a probe's verdict diverges from the table after a version bump,
the table is stale — re-audit.

## Re-running the Phase F.2 pipeline

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt
mkdir -p ../../target/out
python3 cube_cavity/gen_cube_cavity_reduced.py \
    --n 10 --side 1.0 --out ../../target/out/reduced_kM.json
python3 ../driver/eigensolve_from_onnx.py \
    ../../target/out/reduced_kM.json --k 5 \
    --out ../../target/out/eigenresult_onnx.json
python3 ../driver/compare_eigenvalues.py \
    --onnx ../../target/out/eigenresult_onnx.json \
    --jax  ../fixtures/cube_cavity/jax_baseline.json \
    --numpy ../fixtures/cube_cavity/baseline.json \
    --skip-jax-comparison --k 5 --rtol 1e-5
```

See [`cube_cavity/README.md`](cube_cavity/README.md) for graph-design
details and the rationale for raw `onnx.helper` over `onnxscript`. The
CI gate at [`.github/workflows/onnx-cube-cavity.yml`](../../.github/workflows/onnx-cube-cavity.yml)
runs the same three steps on every PR that touches the ONNX or NumPy
reference paths.

## Phase F.1 headline finding (one paragraph)

The cube-cavity assembly spine lowers to graph-only ONNX **cleanly**.
Every L4 operator on stages 1–3 (per-element matrices, global scatter-
add, Dirichlet restriction) either has a direct opset-18 equivalent or
decomposes graph-only into a small number of lower-level ops. **No
secretly-imperative L4 escape was surfaced.** The single audit finding
worth highlighting is that `np.where(mask)[0]` lowers via ONNX
`NonZero` but introduces a data-dependent shape — this is graph-only
friction (the L4 verb IS expressible), but it forces an explicit
host-side `idx` convention that JAX and TF-Java both follow implicitly.
The ONNX path made the convention legible. This is the Phase F.1
"forcing function" working as Epic #88 anticipated.

The audit is therefore **green** for Phase F.2 to proceed: the
end-to-end cube-cavity assembly graph is feasible, with no expected
blockers above the documented graph-only frictions.

## Friction artifacts filed on #5

A roll-up of these findings — tagged with the L4 operator names and
the recommendation for Phase F.2's graph design — is filed as a
comment on the [whiteroom friction tracker (#5)](https://github.com/rjwalters/geode-fem/issues/5).
Per Epic #88 §"Friction-mining feedback loop", #5 is where these
artifacts accumulate for upstream
[`crutcher/palace_whiteroom`](https://github.com/crutcher/palace_whiteroom)
review.

## Phase F.2 design — derived from the audit

The Phase F.2 runtime payload follows directly from the audit's
operator-by-operator findings. Inherited design points (shipped in
issue #123 / this directory):

- Graph inputs: `nodes (n_nodes, 3) f64`, `tets (n_elem, 4) i64`,
  `idx_int (n_int,) i64`. Host-computed `idx`; `NonZero` is excluded
  from the graph (audit Stage 3 recommendation).
- Graph outputs: `K_int (n_int, n_int) f64`, `M_int (n_int, n_int) f64`.
- Stage 1 uses `MatMul` for the batched `(N, 4, 3) @ (N, 3, 4)`
  contraction — ONNX broadcasts the batch dim natively (no `einsum`
  fallback like TF-Java had to drop to; audit Stage 1).
- Stage 2 uses `ScatterND(reduction="add")` on a `ConstantOfShape` zero
  buffer (audit Stage 2 headline operator).
- Stage 3 uses two successive `Gather`s for the outer-product
  `np.ix_(idx, idx)` (audit Stage 3, Path A).
- Authoring: raw `onnx.helper`, not `onnxscript`, to preserve the
  audit's IR-level transparency contract (audit doc lines 51–55; F.2
  curator decision on issue #123).
- Eigensolve seam: a JSON sidecar consumed by
  [`reference/driver/eigensolve_from_onnx.py`](../driver/eigensolve_from_onnx.py)
  — near-clone of `eigensolve_from_tfjava.py`. Consolidation into a
  backend-agnostic `eigensolve_from_sidecar.py` is deferred to a
  follow-up issue once a second sidecar consumer triggers the
  generalization.

## Parent epic + related

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — cross-validated L4 lowerings.
- Friction tracker [#5](https://github.com/rjwalters/geode-fem/issues/5) — friction artifacts roll up here.
- This phase (F.1, audit): issue [#116](https://github.com/rjwalters/geode-fem/issues/116).
- Sibling backends already shipped: NumPy ([#92](https://github.com/rjwalters/geode-fem/issues/92)),
  JAX + TF-Java ([#93](https://github.com/rjwalters/geode-fem/issues/93)).
- TF-Java CI gate (the pattern Phase F.2 will mirror):
  `.github/workflows/tfjava-cube-cavity.yml`.
