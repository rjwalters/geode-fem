# ONNX cube-cavity assembly graph (Phase F.2)

Epic [#88](https://github.com/rjwalters/geode-fem/issues/88) Phase F.2,
issue [#123](https://github.com/rjwalters/geode-fem/issues/123). This is
the runtime payload — the end-to-end ONNX assembly graph that the F.1
audit ([`../audit/cube_cavity_operator_audit.md`](../audit/cube_cavity_operator_audit.md))
established as feasible.

## Layout

- [`assembly_graph.py`](assembly_graph.py) — builds the static ONNX
  graph that lowers the cube-cavity assembly spine end-to-end. Inputs
  are `nodes`, `tets`, `idx_int`; outputs are `K_int`, `M_int`. Analog
  of `reference/tf_java/cube_cavity/.../AssemblyGraph.java`.
- [`gen_cube_cavity_reduced.py`](gen_cube_cavity_reduced.py) — driver
  that runs the graph through `onnxruntime` and emits a schema-v1
  sidecar JSON in the same shape `CubeCavityMain.java` does. Analog of
  the TF-Java `CubeCavityMain.java` entry point.

## Authoring tool: raw `onnx.helper`, not `onnxscript`

The F.1 audit explicitly chose raw `onnx.helper` over `onnxscript` so
that every L4 operator is visible as an opset-18 node at the IR level
(see [audit doc lines 51–55](../audit/cube_cavity_operator_audit.md)).
`onnxscript`'s ergonomics can hide imperative sugar that the audit
needs to surface; Phase F.2 inherits the same authoring choice to
preserve the audit's transparency contract. This decision is locked in
by the issue #123 curator pass and documented here for future readers
who may wonder why F.2 is verbose.

## Graph shape

Inherited from the F.1 audit (audit doc §"What this means for Phase F.2"):

| Direction | Name      | Type | Shape                | Notes                                      |
|-----------|-----------|------|----------------------|--------------------------------------------|
| input     | `nodes`   | f64  | `(n_nodes, 3)`       | Mesh node coordinates.                     |
| input     | `tets`    | i64  | `(n_elem, 4)`        | Connectivity (int64 at the I/O boundary).  |
| input     | `idx_int` | i64  | `(n_int,)`           | Interior-DOF indices, **host-computed**.   |
| output    | `K_int`   | f64  | `(n_int, n_int)`     | Stiffness, Dirichlet-restricted.           |
| output    | `M_int`   | f64  | `(n_int, n_int)`     | Mass, Dirichlet-restricted.                |

`idx_int` is host-computed (via `np.where(mask)[0]`) per the F.1 audit's
explicit recommendation — using ONNX `NonZero` inside the graph would
lower cleanly but produce a data-dependent shape that breaks static
shape inference for everything downstream (see [audit Stage 3 friction
note](../audit/cube_cavity_operator_audit.md#friction-note-on-stage-3-the-most-informative-row-in-this-audit)).
JAX and TF-Java follow this convention implicitly; the ONNX path makes
it explicit.

## Eigensolve seam

The graph **stops at** `K_int`/`M_int`. The eigensolve is the host-side
sidecar boundary, identical convention to JAX (`scipy.linalg.eigh`
outside the jit) and TF-Java (`eigensolve_from_tfjava.py` over a JSON
sidecar). The Phase F.2 driver
([`gen_cube_cavity_reduced.py`](gen_cube_cavity_reduced.py)) emits a
schema-v1 sidecar; the companion
[`reference/driver/eigensolve_from_onnx.py`](../../driver/eigensolve_from_onnx.py)
runs the SciPy eigensolve over that sidecar.

## Re-running

```bash
cd reference/onnx
python3 -m pip install -r requirements.txt   # onnx 1.21.0, onnxruntime 1.26.0
mkdir -p ../../target/out
python3 cube_cavity/gen_cube_cavity_reduced.py \
    --n 10 --side 1.0 --out ../../target/out/reduced_kM.json
python3 ../driver/eigensolve_from_onnx.py \
    ../../target/out/reduced_kM.json --k 5 \
    --out ../../target/out/eigenresult_onnx.json
```

The expected eigenvalues for `n=10`, `side=1.0` are the same as the
n=10 NumPy canonical baseline
(`reference/fixtures/cube_cavity/baseline.json`); cross-IR agreement
(rtol=1e-5) is gated by `.github/workflows/onnx-cube-cavity.yml`.

## opset / version policy

Inherited from F.1 unchanged:

- `onnx==1.21.0`, `onnxruntime==1.26.0` (see
  [`../requirements.txt`](../requirements.txt)).
- Target opset 18.

Bumping any of these requires re-running the three audit probes
(`probe_p1_local.py`, `probe_assembly_scatter.py`, `probe_dirichlet_mask.py`)
and updating the audit table if any verdict diverges. The CI gate for
F.2 (`onnx-cube-cavity.yml`) is the runtime check; the audit is the
expressibility check. They are complementary — both must pass for the
ONNX reference to remain green.

## Pointers

- F.1 audit deliverable: [`../audit/cube_cavity_operator_audit.md`](../audit/cube_cavity_operator_audit.md).
- Issue: [#123](https://github.com/rjwalters/geode-fem/issues/123) (Phase F.2).
- Parent epic: [#88](https://github.com/rjwalters/geode-fem/issues/88).
- TF-Java sibling (the pattern F.2 mirrors):
  [`../../tf_java/cube_cavity/`](../../tf_java/cube_cavity/).
- JAX sibling: [`../../jax/cube_cavity.py`](../../jax/cube_cavity.py).
- Sidecar driver: [`../../driver/eigensolve_from_onnx.py`](../../driver/eigensolve_from_onnx.py).
- CI gate: [`../../../.github/workflows/onnx-cube-cavity.yml`](../../../.github/workflows/onnx-cube-cavity.yml).
