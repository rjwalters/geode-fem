# Julia reference implementations

Complex-arithmetic reference backend per **Epic #88**. Probes whether
complex-arithmetic ergonomics surface different L4 friction than the
f64-pair representation used in Burn/NumPy.

## Status

- **`cube_cavity.jl`** — full scalar-Helmholtz cube-cavity pipeline
  (Epic #88 / Phase E, issue #115). Assembly in pure Julia via
  `SparseArrays`; eigensolve via `Arpack.jl`. The cube-cavity slice
  is real-symmetric, so this PR is the toolchain bring-up for Phases
  G–J (Nédélec, PML, NLEPS) where complex types will exercise
  Julia's complex-arithmetic ergonomics directly.
- **`mesh.jl`** — local mesh primitives (`cube_tet_mesh`,
  `cube_interior_mask`, inline MSH 4.1 ASCII `load_msh`). We
  deliberately avoid the `Gmsh.jl` native dependency (~50 MB libgmsh)
  since the MSH files we consume here have a simple, well-documented
  structure (see Open Question #3 in the issue #115 curator pass).
- **`gen_cube_cavity_baseline.jl`** — fixture generator producing
  `reference/fixtures/cube_cavity/julia_baseline.json` in the
  canonical v1 schema. CI regenerates this fixture on every run and
  asserts cross-IR agreement against the NumPy canonical baseline.

## Toolchain bootstrap

The reference uses Julia 1.10 LTS, `Arpack.jl 0.5`, and `JSON3.jl`.
Dependencies are pinned in `Project.toml`; the curator pass on issue
#115 recommends committing `Manifest.toml` for applications, but the
initial PR ships without it to avoid lock-file churn (revisit per
Open Question #1 if reproducibility friction surfaces).

```sh
# One-time setup: resolve and precompile dependencies.
julia --project=reference/julia -e 'using Pkg; Pkg.instantiate()'

# Self-check: run the pipeline with the canonical n=10 mesh fixture.
julia --project=reference/julia reference/julia/cube_cavity.jl \
    --mesh reference/fixtures/cube_cavity/unit_cube.msh

# Regenerate the Julia baseline fixture.
julia --project=reference/julia reference/julia/gen_cube_cavity_baseline.jl \
    --n 10 --mesh reference/fixtures/cube_cavity/unit_cube.msh \
    --out reference/fixtures/cube_cavity/julia_baseline.json
```

## Invocation convention

The Julia pipeline mirrors the NumPy / JAX / TF-Java siblings: it
runs as a subprocess emitting a sidecar JSON in the canonical
`schema_version: "1"` shape. This keeps the cross-IR comparator
driver (`reference/driver/compare_eigenvalues.py`) framework-light
and toolchain-agnostic.

```
julia --project=. cube_cavity.jl [--n <int>] [--side <float>]
                                  [--mesh <path>] [--out <path>]
```

## What the AC#2 cross-IR check exercises

The Julia eigenvalues agree with the NumPy canonical `baseline.json`
to ~`1e-13` relative because `Arpack.jl` and
`scipy.sparse.linalg.eigsh` both bind the same underlying
`libarpack`. This is the **cleanest possible isolation** of the
"Julia vs NumPy" friction question for the eigensolve step — surface
a different result only if Julia's wrapping introduces drift, not
because of algorithm choice.

The 1e-5 relative tolerance gate (per Epic #88's cross-language
reproducibility framing) leaves four orders of headroom above the
actual observed drift, so the gate catches real regressions without
flapping on transient libarpack ABI quirks.

## What AC#3 enforces — sub-stage f64 cross-platform floor

The Julia baseline ships sub-stage diagnostics (`k_int_frobenius`,
`m_int_frobenius`, `k_int_diag`, `m_int_diag`) pinned at the same
cross-platform floor that PR #113 (issue #110) calibrated for the
NumPy canonical baseline:

| Field             | Tolerance |
|-------------------|-----------|
| `k_int_frobenius` | `1e-9` abs |
| `m_int_frobenius` | `1e-8` abs |
| `k_int_diag`      | `1e-9` abs |
| `m_int_diag`      | `5e-9` abs |

These are the same bounds that absorbed the rustc/LLVM SIMD-reduction-
order variance across Ubuntu / macOS arm64 / macOS Intel. Julia's
BLAS (OpenBLAS by default) has its own reduction-order quirks; the
expectation is that Julia lands inside the same `5e-9` envelope. **If
Julia exceeds the floor on any CI matrix platform, that is a Friction
Artifact** (file on #5 per Epic #88's friction-mining loop) — not a
license to loosen the floor.

## ARPACK iteration-trace caveat

Re-runs against a different `Arpack.jl` version may produce
eigenvectors differing by orthogonal rotation within degenerate
clusters. The **eigenvalues** are stable; the **eigenvectors**
require the subspace-overlap convention (see `baseline.schema.md`
"Per-cluster subspace-overlap convention" in the cube-cavity fixture
directory). The Julia fixture stores eigenvalues + sub-stage
diagnostics only (mirroring `jax_baseline.json`), not eigenvectors —
sufficient for AC#2 / AC#5 / AC#6.

## Julia complex-arithmetic friction observations

This section is the durable record the Epic #88 framing asks for —
observations go to #5 as supporting evidence, not side-channel
grumbling.

Initially empty: the cube-cavity slice is real-symmetric, so this
PR (#115) does not surface complex-arithmetic friction directly. The
slot is reserved for Phase G's Nédélec curl-curl (where complex
permittivities will exercise complex-typed assembly) and Phase H's
PML (where stretched-coordinate complex frequencies surface
directly).

### Positive friction artifact: Julia's f64 default

Where JAX requires an explicit
`jax.config.update("jax_enable_x64", True)` at module top (a JAX-DX
friction artifact recorded in `reference/jax/README.md`), Julia is
f64 by default. The fact that Julia "just is" f64 is itself an
interesting L4 friction observation — **what JAX requires you to
explicitly enable, Julia gives you for free**.

### Eigensolver-choice note

Per Epic #88's complex-arithmetic principle, `Arpack.jl` is the
default for Phase E because it binds the same `libarpack` used by
NumPy/SciPy. Override candidates (`KrylovKit.jl`, `ArnoldiMethod.jl`)
are pure-Julia Lanczos/Arnoldi implementations that would dilute the
iteration-trace agreement signal with NumPy; switch to them only if
`Arpack.jl` has install/build issues on a CI runner, and **document
the switch as the first Julia-specific Friction Artifact filed on
#5** (don't switch silently).

## Mesh I/O — inline parser vs `Gmsh.jl`

`mesh.jl` includes an inline MSH 4.1 ASCII parser. The decision tree
(per the curator pass on issue #115):

- `Gmsh.jl` binds libgmsh (~50 MB native dependency); pulling it in
  for CI runs would dominate the cold-cache wall-clock budget.
- The `.msh` fixture we consume here (`unit_cube.msh`) is MSH 4.1
  ASCII, simple enough to parse inline in ~150 lines of Julia.
- The inline parser also serves as a small forcing function: if
  Phase G (Nédélec) needs higher-order mesh fields (curve elements,
  geometry tags), the right place to add them is here, in
  source-controlled Julia, not behind a libgmsh ABI.

## Planned layout

This directory will grow per-spine-slice files alongside
`cube_cavity.jl`. The pattern (per `reference/README.md`):

```
reference/julia/
├── README.md                            ← this file
├── Project.toml                         ← pinned deps
├── mesh.jl                              ← cube_tet_mesh, load_msh
├── cube_cavity.jl                       ← Epic #88 / #115 (Phase E)
├── gen_cube_cavity_baseline.jl
└── <next_slice>.jl                      ← future spine slices
```
