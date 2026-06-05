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
- **`sphere_pec.jl` + `gen_sphere_pec_baseline.jl`** — vector-Nédélec
  sphere-PEC eigenmode pipeline (Epic #88 / Phase G.4, issue #129).
  First Nédélec slice; eigensolve fallback path (Issue #133) uses
  `LinearAlgebra.eigen` on the dense pencil since n ≈ 3300 is small
  enough and the spurious cluster at λ ≈ 0 confuses sparse Arnoldi
  in regular-inverse mode.
- **`sphere_pml.jl` + `gen_sphere_pml_baseline.jl`** — scalar-isotropic
  sphere-PML Nédélec eigenmode pipeline (Epic #88 / Phase H.2, issue
  #147). **First complex-arithmetic spine slice**: per-tet ε_r is
  `ComplexF64`, mass matrix is `SparseMatrixCSC{ComplexF64,Int}`,
  eigensolve via `Arpack.eigs` shift-invert. Pipeline cross-checks
  against the Phase G.4 NumPy PEC baseline at σ₀ = 0 (the
  PEC-collapse regression).

## Toolchain bootstrap

The reference uses Julia 1.10 LTS, `Arpack.jl 0.5`, and `JSON3.jl`.
Dependencies are pinned in `Project.toml`; `Manifest.toml` is
committed per the curator's Open Question #1 recommendation
(reproducible builds for an application). This directory is a plain
Julia *environment*, not a package — there is no `name`/`uuid`/`version`
in `Project.toml` and no `src/` module, which keeps
`Pkg.instantiate()` from trying to precompile a non-existent
top-level module.

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

The cube-cavity slice is real-symmetric, so PR #115 does not surface
complex-arithmetic friction directly. The slot remains open for Phase
G's Nédélec curl-curl (where complex permittivities will exercise
complex-typed assembly) and Phase H's PML (where stretched-coordinate
complex frequencies surface directly).

### Phase H.2 (sphere PML) — Julia ergonomics wins

The scalar-isotropic sphere PML phase (`sphere_pml.jl`, issue #147)
is the first cross-IR spine slice where Julia's native `ComplexF64`
types do real work. Observations:

**Positive: native complex types remove the spine ceremony.**
The per-region ε_r assignment is **one line of Julia per region** —
no `complex128` dtype declaration, no NumPy `view(np.float64)`
interleave, no Burn-side paired-real `Mat<f64>` representation:

```julia
eps_complex[t] = ComplexF64(1.0, -sigma0 * u * u)  # PML shell
```

The mass-matrix scatter then "Just Works":
`V_mass[p] = (s * M_local[le_i, le_j]) * eps_t` — Julia promotes the
real `M_local` × complex `eps_t` to ComplexF64 at the multiplication
site without any annotation. The resulting `SparseMatrixCSC{ComplexF64,Int}`
flows through `Arpack.eigs` without any wrapper. **The Burn side has
a ~500-line `SparseComplexShiftInvertLanczos` re-implementing this
from faer primitives** because faer 0.24 lacks a sparse complex
shift-invert. Julia gives the same in one line: `eigs(K, M; nev, sigma)`.

**Negative: `Arpack.eigs` shift-invert calling-convention bug for
complex generalized problems.** The `explicittransform=:auto` branch
(the default) for generalized complex problems with `sigma ≠ 0` swaps
`:LM ↔ :SM` internally in a way that returns eigenvalues *farthest*
from σ rather than closest. **Workaround**: pass
`explicittransform=:none` to delegate shift-invert to libarpack
natively. This is the same family of friction as the cube_cavity
calling-convention divergence above — same root cause (Julia wrapping
of libarpack with non-scipy-compatible semantics), surfaced again on
the complex generalized branch. Recorded in
`sphere_pml.jl::eigensolve_physical_shift_invert`'s docstring.

**Negative: σ-shift choice is non-obvious for the sphere mesh.** The
Phase G.4 PEC fix uses σ ≈ 0.01 (just above the spurious cluster at 0)
with `nev = spurious_dim + 8 = 376` to grab all spurious + 8 physical
in one shot. That pattern is **not viable for the complex case**:
`nev = 376` from a 3300-dim sparse complex pencil exceeds Arpack's
practical convergence budget on the bundled mesh (timeouts at 10+
minutes; a dense `LinearAlgebra.eigen` on the 3300×3300 ComplexF64
pencil takes 30+ minutes on Apple Silicon — much steeper cost
asymmetry vs. the real-symmetric `eigen(Symmetric, Symmetric)` path
that finishes in seconds).

The H.2 fix: shift σ = 2.0 (**above** the physical band) so Arpack
converges to physical modes by geometric proximity. Spurious cluster
at distance 2.0, physical at distance ≈ 0.58 (PEC) or ≈ 0.14 (σ₀=5
PML). With ratio > 3×, Arnoldi stays out of the 368-dim spurious
invariant subspace. Tried `σ ∈ (0, λ_phys)` (e.g. 0.8, 1.0, 1.42)
experimentally — all spuriously dominated by the degenerate cluster.

**Negative: Arpack non-determinism without fixed `v0`.** The default
random starting vector gives ~1e-2 variability in converged
eigenvalues across runs. Fixed by passing `v0 = ones(ComplexF64, n) /
sqrt(n)`. Recorded in the eigensolver docstring.

**Negative: Arpack ghost conjugate-pair modes.** For non-Hermitian
generalized problems, Arpack occasionally returns a `λ̄`-partner of a
near-real physical mode (Im(λ) > 0 instead of ≤ 0). Filter via
`imag(lam) <= tol` and request `n_physical + n_extra` modes to absorb
the loss. Sign-convention check (`Im(λ) ≤ 0` under exp(+jωt))
catches it.

**Net: spec-mining payoff for Julia's inclusion is real.** The
constitutive + assembly layer (the things FEM users actually write)
is genuinely cleaner. The eigensolve layer has a 4-line set of
gotchas that should be promoted to the calling-convention notes for
any Phase J (NLEPS) work that lands later.

### Real-arithmetic friction: Arpack.jl 0.5 shift-invert API divergence

The first concrete Julia friction artifact, surfaced during PR #115
review: **the canonical SciPy recipe
`eigsh(K, k, M=M, sigma=0, which="LM")` for "lowest generalized
eigenvalues" does not work as-is in `Arpack.jl 0.5`.** When the
problem is generalized (B matrix present) and `sigma !== nothing`,
Arpack.jl 0.5 takes the `:auto` `explicittransform=:shiftinvert`
path, swaps `:LM ↔ :SM` internally, factorizes `σB - A = -K` at
σ=0, and solves the standard problem for `-K⁻¹M` with `:SM` — which
returns the *largest* generalized eigenvalues of the original pencil,
the opposite of what the user requested. The post-processing step
`λ = σ - 1/μ` does invert the transform, but on the wrong end of the
spectrum.

Workaround: use Arpack.jl's regular-inverse mode — `eigs(K, M; nev,
which=:SM)` with no `sigma`. This factorizes M once and Lanczos-
iterates on `M⁻¹K` asking for smallest-magnitude eigenvalues. Matches
the dense `eigvals(K, M)` reference to ~1e-13.

This is a **calling-convention divergence between Julia and SciPy on
top of the same libarpack**, exactly the kind of L4 friction Epic #88
is designed to surface. The two ecosystems wrap the same Fortran with
incompatible conventions, and the iteration trace agreement that
motivated the Arpack.jl choice still holds for the *operator* — just
not for the SciPy-shaped API call. Recorded here, in the docstring
of `cube_cavity.jl::eigensolve_arpack`, and in the
`provenance.verified_against` field of the generated fixture. File on
#5 as supporting evidence for the friction-mining loop.

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
