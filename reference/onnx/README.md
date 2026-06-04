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
  the cube-cavity operator-by-operator audit. **Landed in this PR.**
  Lives under [`audit/`](audit/) — Markdown gap-list plus three runnable
  probe scripts. No CI gate; the audit *is* the deliverable.
- **Phase F.2 — end-to-end cube-cavity ONNX assembly graph (planned)**:
  the runtime payload (`cube_cavity/`), the sidecar driver
  (`reference/driver/eigensolve_from_onnx.py` or a refactored
  backend-agnostic shared driver), and the CI gate analogous to
  `.github/workflows/tfjava-cube-cavity.yml`. Filed after this PR
  merges so its curation can incorporate what F.1 actually learned.

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
└── (planned Phase F.2)
    ├── cube_cavity/
    │   ├── assembly_graph.py          — analog of tf_java/.../AssemblyGraph.java
    │   └── gen_cube_cavity_reduced.py — emits reduced_kM.json sidecar
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

## Pointer to Phase F.2 planned layout

The Phase F.2 issue (to be filed) will land:

```
reference/onnx/cube_cavity/
├── assembly_graph.py                  — builds the ONNX assembly graph
│                                        (analog of TF-Java AssemblyGraph.java).
│                                        Graph inputs: nodes, tets, idx_int.
│                                        Graph outputs: K_int, M_int.
└── gen_cube_cavity_reduced.py         — runs onnxruntime over the graph
                                        and emits reduced_kM.json (same schema
                                        as TF-Java's sidecar).
reference/driver/
└── eigensolve_from_onnx.py            — near-clone of eigensolve_from_tfjava.py
                                        (or refactored into a backend-agnostic
                                        eigensolve_from_sidecar.py — decision
                                        belongs in F.2's curation).
.github/workflows/onnx-cube-cavity.yml — CI gate, mirroring tfjava-cube-cavity.yml
                                        for three-way cross-IR agreement
                                        (ONNX vs JAX vs NumPy at rtol=1e-5).
```

The F.2 design is **derived from**, not independent of, the audit
deliverable in this directory.

## Parent epic + related

- Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) — cross-validated L4 lowerings.
- Friction tracker [#5](https://github.com/rjwalters/geode-fem/issues/5) — friction artifacts roll up here.
- This phase (F.1, audit): issue [#116](https://github.com/rjwalters/geode-fem/issues/116).
- Sibling backends already shipped: NumPy ([#92](https://github.com/rjwalters/geode-fem/issues/92)),
  JAX + TF-Java ([#93](https://github.com/rjwalters/geode-fem/issues/93)).
- TF-Java CI gate (the pattern Phase F.2 will mirror):
  `.github/workflows/tfjava-cube-cavity.yml`.
