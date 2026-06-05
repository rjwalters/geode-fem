# `reference/driver/` — language-bridge drivers

Small driver scripts that consume a sidecar file produced by one
backend and close the spine at the boundary that backend can't
reach natively. Each script lives here (rather than under any
single backend) because it's the *seam* — the language-bridge —
that the friction-mining loop wants to highlight.

## `eigensolve_from_sidecar.py` — consolidated entry point

Single script parameterised by `--backend {tfjava|onnx}` and
`--problem {cube-cavity|sphere-pec}`. Consumes the fixture-schema
JSON sidecar emitted by one of the assembly backends and runs the
appropriate SciPy eigensolve, then emits a second fixture-schema
JSON with the eigenvalues for cross-IR comparison.

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
