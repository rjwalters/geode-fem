# `reference/driver/` — language-bridge drivers

Small driver scripts that consume a sidecar file produced by one
backend and close the spine at the boundary that backend can't
reach natively. Each script lives here (rather than under any
single backend) because it's the *seam* — the language-bridge —
that the friction-mining loop wants to highlight.

## `eigensolve_from_sidecar.py` — consolidated entry point

Single script parameterised by `--backend {tfjava|onnx}` and
`--problem {cube-cavity|sphere-pec|sphere-pml|sphere-mie}`. Consumes
the fixture-schema JSON sidecar emitted by one of the assembly
backends and runs the appropriate SciPy eigensolve, then emits a
second fixture-schema JSON with the eigenvalues for cross-IR
comparison.

```bash
# Cube cavity (scalar Helmholtz, P1, default problem):
python3 reference/driver/eigensolve_from_sidecar.py \
    path/to/reduced_kM.json \
    --backend tfjava \
    --k 5 --out path/to/eigenresult.json

# Sphere PEC (Nédélec curl-curl, shift-and-invert ARPACK with
# d⁰-rank spurious filter):
python3 reference/driver/eigensolve_from_sidecar.py \
    path/to/reduced_kM_sphere_pec.json \
    --backend onnx --problem sphere-pec \
    --baseline reference/fixtures/sphere_pec/baseline.json \
    --rtol 1e-5 --k 5 \
    --out path/to/eigenresult_sphere_pec.json
```

### Summary sidecars (invariants-only mode, sphere-PEC)

The checked-in fixtures `reference/fixtures/sphere_pec/tfjava_sidecar.json`
and `onnx_sidecar.json` are **summary** sidecars: their `outputs` carry
scalar assembly invariants (`k_int_frobenius`, `m_int_frobenius`,
`k_diag_sum`, `m_diag_sum`, mesh counts) instead of the full matrices
(3300x3300 dense f64 would be ~90 MB of JSON each — see the docblock in
`crates/geode-validation/tests/sphere_pec_tfjava_reference.rs`).

The driver auto-detects a summary sidecar (no `outputs.k_int`) and runs
an **invariants-only comparison** against the NumPy baseline instead of
an eigensolve, clearly labelling the output. The gate is 1e-4 relative,
matching the Rust harness `frobenius_rel`. The live CI paths are
unaffected: CI regenerates full sidecars from the JVM / ONNX assembly
runs, which take the full eigensolve path (issue #186).

```bash
# Smoke the checked-in summary fixtures (no eigensolve):
python3 reference/driver/eigensolve_from_sidecar.py \
    reference/fixtures/sphere_pec/tfjava_sidecar.json \
    --backend tfjava --problem sphere-pec \
    --out /tmp/invariants_pec_tfjava.json
python3 reference/driver/eigensolve_from_sidecar.py \
    reference/fixtures/sphere_pec/onnx_sidecar.json \
    --backend onnx --problem sphere-pec \
    --out /tmp/invariants_pec_onnx.json
```

## `make_numpy_sidecar.py` — local smoke for the full-matrix paths

The other problem families check in no sidecar at all (CI synthesizes
them from the live TF-Java / ONNX assemblies). To smoke the driver's
full-matrix eigensolve paths locally without a Java/ONNX toolchain,
`make_numpy_sidecar.py` synthesizes a schema-compatible sidecar from
the in-tree NumPy reference assemblies (the issue-174 recovery
pattern):

```bash
# Cube cavity:
python3 reference/driver/make_numpy_sidecar.py \
    --problem cube-cavity --n 4 --out /tmp/reduced_kM.json
python3 reference/driver/eigensolve_from_sidecar.py \
    /tmp/reduced_kM.json --backend tfjava --k 5 \
    --out /tmp/eigenresult.json

# Sphere PML (small 48-node mesh; gate vs the small baseline):
python3 reference/driver/make_numpy_sidecar.py \
    --problem sphere-pml --out /tmp/reduced_kM_sphere_pml.json
python3 reference/driver/eigensolve_from_sidecar.py \
    /tmp/reduced_kM_sphere_pml.json \
    --backend tfjava --problem sphere-pml --k 5 \
    --baseline reference/fixtures/sphere_pml_small/baseline.json \
    --rtol 1e-4 --out /tmp/eigenresult_sphere_pml.json

# Sphere Mie (small mesh, tensor-ε UPML; hard per-position gate):
python3 reference/driver/make_numpy_sidecar.py \
    --problem sphere-mie --out /tmp/reduced_kM_sphere_mie.json
python3 reference/driver/eigensolve_from_sidecar.py \
    /tmp/reduced_kM_sphere_mie.json \
    --backend tfjava --problem sphere-mie --k 5 \
    --baseline reference/fixtures/sphere_mie_small/baseline.json \
    --rtol 1e-4 --out /tmp/eigenresult_sphere_mie.json

# Sphere PEC full-matrix path (CI-sized: ~430 MB temp JSON, minutes):
python3 reference/driver/make_numpy_sidecar.py \
    --problem sphere-pec --out /tmp/reduced_kM_sphere_pec.json
python3 reference/driver/eigensolve_from_sidecar.py \
    /tmp/reduced_kM_sphere_pec.json \
    --backend tfjava --problem sphere-pec --k 5 \
    --baseline reference/fixtures/sphere_pec/baseline.json \
    --rtol 1e-5 --out /tmp/eigenresult_sphere_pec.json
```

Why a separate driver? Both TF-Java and ONNX lack a native sparse
generalized eigensolver. Per #93's acceptance criteria, delegating
to SciPy (the same solver the in-tree NumPy reference uses) gives
an apples-to-apples cross-check; for the sphere-PEC Nédélec
problem (Issue #134) the additional shift-and-invert ARPACK path
and d⁰-rank spurious-mode classifier (Issue #124 / PR #126) close
the gap between the raw assembly output and the physical
eigenvalues.

See `reference/tf_java/README.md` and `reference/onnx/README.md`
for the per-backend workflows.

## Deprecated shims

The following scripts are thin wrappers that inject the right
`--backend` / `--problem` flags and delegate to
`eigensolve_from_sidecar.py`. They are preserved so existing CI
workflows and external callers continue to work without
invocation-line changes; prefer the consolidated entry point for
new callers.

| Shim | Consolidated equivalent |
|---|---|
| `eigensolve_from_tfjava.py` | `eigensolve_from_sidecar.py --backend tfjava --problem cube-cavity` |
| `eigensolve_from_onnx.py` | `eigensolve_from_sidecar.py --backend onnx --problem cube-cavity` |
| `eigensolve_sphere_pec_sidecar.py` | `eigensolve_from_sidecar.py --backend tfjava --problem sphere-pec` (default backend; pass `--backend onnx` to override) |
