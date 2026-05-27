# GEODE-FEM

**GPU-accelerated Electromagnetic Open Differentiable Engine — FEM/DG**

> Status: planning / bootstrap. No solver yet — this repo exists to anchor the
> design conversation and track issues as the project comes online.

GEODE-FEM is a [Burn](https://burn.dev)-based Rust implementation of a high-order
finite-element / discontinuous-Galerkin electromagnetic solver. It targets the
same use cases as [AWS Labs Palace](https://awslabs.github.io/palace/stable/) —
eigenmode analysis, frequency-domain driven simulation, time-domain — but built
natively on a differentiable tensor IR with GPU acceleration as a first-class
concern.

## Why

Palace is a state-of-the-art open-source FEM EM solver, but its C++/MFEM-based
implementation predates the era of tensor IRs with differentiable, multi-backend
GPU compilers. Expressing the same operators on top of
[Burn](https://github.com/tracel-ai/burn) unlocks:

- **Hardware portability** without per-backend kernels: CUDA, ROCm, Metal,
  Vulkan, WebGPU
- **Differentiable physics** for inverse design and topology optimization
- **Kernel fusion** across FEM stencils via Burn's JIT compiler
- **Rust** end-to-end: solver, geometry, mesh, I/O — no Python boundary

## Project family

GEODE-FEM is one of three complementary projects:

| Project | Role | Discretization | Status |
|---|---|---|---|
| [crutcher/palace_whiteroom](https://github.com/crutcher/palace_whiteroom) | Clean-room dissection of Palace into a layered specification (L1–L4) | FEM/DG (target) | Active analysis |
| [rjwalters/geode-fem](https://github.com/rjwalters/geode-fem) | Burn-based realization of the whiteroom L4 specification | FEM/DG | Bootstrap |
| [rjwalters/strata-fdtd](https://github.com/rjwalters/strata-fdtd) | FDTD time-domain solver (acoustic today, EM in progress) | FDTD | Active |

Strata and GEODE-FEM are sister codebases that use **different discretizations**
for **overlapping physics**. Mie resonances are the canonical cross-check
benchmark — analytical Mie series ↔ strata FDTD ↔ GEODE-FEM eigenmode.

## Roadmap (v0)

- [x] Cargo workspace skeleton with Burn dependency
- [ ] Scalar Helmholtz on a tetrahedral mesh (warmup before vector Maxwell)
- [ ] Vector curl-conforming (Nédélec) elements
- [ ] First eigenmode solver for a dielectric sphere
- [ ] Mie scattering benchmark vs. analytic series and strata-fdtd
- [ ] Map whiteroom L4 specification → GEODE-FEM operators

## Build

Requires Rust stable (1.92+, set in `rust-toolchain.toml`).

```sh
cargo build              # builds workspace with default `wgpu` backend
cargo test               # runs the GPU smoke test
cargo run --bin geode    # prints backend / device / smoke result
```

### Backend selection

`geode-core` selects one Burn backend at compile time via mutually-exclusive
features:

```sh
# default — wgpu (Metal on macOS, Vulkan on Linux, DX12 on Windows)
cargo build

# CUDA (requires a CUDA toolkit and an NVIDIA GPU)
cargo build -p geode-core --no-default-features --features cuda
```

Enabling both `wgpu` and `cuda`, or neither, is a hard compile error — see
`compile_error!` guards in `crates/geode-core/src/lib.rs`.

## System dependencies

The default build is **pure Rust**: backend GPU drivers (Metal, Vulkan, CUDA,
etc.) aside, no system Fortran/BLAS libraries are required. In particular,
sparse generalized eigensolves use a built-in shift-and-invert Lanczos
(`SparseShiftInvertLanczos`) that depends only on `faer`'s sparse LU.

The optional `arpack` Cargo feature (off by default) switches in an
ARPACK-backed driver via `arpack-ng-sys`. When enabled it requires:

- a system `libarpack` install (`brew install arpack` on macOS,
  `apt-get install libarpack2-dev` on Debian/Ubuntu), **and**
- the `arpack/arpack.h` C header — which the macOS Homebrew formula does
  *not* ship as of this writing; expect to vendor it or point `CFLAGS` at
  a manual checkout.

Because of the macOS header story the ARPACK driver currently ships as a
stub that returns an error from `smallest_eigenvalues`. The Lanczos
default satisfies the convergence acceptance for issue #13 without any
Fortran dependency. The ARPACK FFI will be wired up once the header
story is settled; tracked alongside follow-up sparse work.

### Workspace layout

```
crates/
  geode-core/   # solver primitives, Backend type alias, FEM trait sketches
  geode-cli/    # `geode` binary — prints device info and runs the smoke op
```

## Regression fixtures

Numerical baselines for the unit-cube Dirichlet Laplacian ground-mode
sweep are committed under
`crates/geode-core/tests/fixtures/cube_convergence.toml`. The values are
**not** analytic targets; they record what the current assembly +
`faer` eigensolver produces today. Their job is to catch unintended
regressions when assembly or the eigensolver change.

The diff-check test lives at
`crates/geode-core/tests/cube_convergence_regression.rs`. It is
`#[ignore]`d for the same reason as the other eigensolver tests:
faer 0.24's `gevd::qz_real` panics under debug-assertions. Run with:

```sh
# Run the regression diff-check (and all other ignored faer tests):
cargo test -p geode-core --release -- --ignored

# Run only the convergence regression:
cargo test -p geode-core --release \
    --test cube_convergence_regression -- --ignored
```

If an intentional change (e.g. mass-lumping, eigensolver swap) shifts
the per-level eigenvalues beyond the `1e-4` relative tolerance,
regenerate the fixture and commit it alongside the code change:

```sh
cargo run -p geode-core --release \
    --example regen_cube_convergence_fixture
```

Call out the regeneration in the PR description so reviewers know the
baseline drift is intentional.

## Performance baseline

A `criterion`-based bench harness lives under
[`crates/geode-core/benches/`](crates/geode-core/benches). It establishes
a wall-clock baseline for the FEM pipeline so future performance
work has something to push against. The current numbers (Apple Silicon,
default `wgpu` backend) are committed to
[`benchmarks/perf/baseline.toml`](benchmarks/perf/baseline.toml).

**Reproduce the measurements:**

```sh
# Runs all 5 benches; total wall-clock ≈ 25-30 min on M-series hardware,
# dominated by the Mie end-to-end (~70-90 s per sample × 10 samples).
cargo bench -p geode-core
```

Criterion writes per-bench HTML reports under `target/criterion/`
(gitignored). Extract a clean TOML summary (medians + median-absolute-
deviation as an IQR proxy) with:

```sh
cargo run -p geode-core --example extract_baseline
```

This walks `target/criterion/<bench>/<input>/new/estimates.json` and
overwrites `benchmarks/perf/baseline.toml`. The extractor is **not**
wired into `cargo bench` itself — re-running the analysis is then a
side-effect-free second step.

**Dominant cost (today, n=10 cube):**

| stage                              | median   |
| ---------------------------------- | -------- |
| `assemble_global_p1`               |  45 ms   |
| `assemble_global_nedelec` (real)   | 289 ms   |
| `assemble_global_nedelec` (cmplx)  | 407 ms   |
| `FaerDenseEigensolver`             | **5.95 s** |
| `SparseShiftInvertLanczos`         |  52 ms   |

Dense `generalized_eigen` on the 9³ = 729 interior pencil dwarfs every
other stage by **two orders of magnitude**. The pure-Rust sparse
shift-and-invert Lanczos (faer sparse LU) brings the eigensolve down
to roughly the same order as the Burn-side assembly, confirming the
follow-up roadmap: **the dense eigensolve is the bottleneck**, and
swapping it for sparse where applicable is the biggest win available.

**Dominant cost (today, Mie sphere fixture):**

The bundled 313-node sphere fixture (~1226 tets, ~7 k Nédélec edges)
runs the *complex* dense generalized eigensolve over a ~6 k × 6 k
interior pencil. Single-sample median wall-clock is **≈ 71 s**;
assembly + I/O on the same problem extrapolates to well under one
second from the cube assembly numbers. The dense complex eigensolve
is responsible for essentially the entire end-to-end cost. Migrating
this path to a sparse complex eigensolver (or, more cheaply, to a
shift-and-invert wrapper around the real symmetric Lanczos with
splittable mass) is the highest-leverage performance follow-up.

### Mie sphere (issue #4)

The project's stated north-star validation problem: FEM eigenmodes of a
dielectric sphere (refractive index `n = 1.5`, radius `R = 1`) inside a
vacuum buffer (`r ≤ R_buffer = 2`) terminated by a scalar isotropic PML,
compared against analytic resonance roots.

Run the benchmark:

```sh
cargo run -p geode-core --release --example mie_sphere
```

This prints a comparison table and writes
[`benchmarks/mie_sphere/results.toml`](benchmarks/mie_sphere/results.toml)
with the lowest 8 FEM modes paired against the extended analytic
catalog. The benchmark uses the **PEC-cavity dielectric resonator**
as the analytic ground truth (a closed cavity with PEC at `r = R_buffer`,
which is the limit the FEM hits as the PML absorption strength `σ₀ → 0`);
the open-space Mie WGM positions — which require Hankel functions and
complex Newton iteration — are tracked under #33, and the driven
scattering (`Q_ext`, `Q_sca` vs. `ka`) cross-check remains a separate
later step.

**Catalog (v1, issue #40)**: roots for angular orders `l ∈ [1, 4]`,
both TE and TM polarisations, lowest 5 radial overtones each (~40
entries). Each root carries its `(l, n, polarisation, multiplicity = 2l+1)`
label.

**Mode classification (v1, issue #40)**: the v0 nearest-`k` pairing
mis-labeled the second FEM triplet (Q ≈ 1.30) as a copy of TM_1,1.
v1 walks the catalog in ascending `k`, and for each analytic root
claims the next `2l + 1` consecutive FEM modes (sorted by `Re(k)`),
producing an unambiguous `(l, n, pol, m_idx)` label per mode. On the
bundled coarse fixture this identifies the lowest 3 FEM modes as the
TM_1,1 triplet (Q ≈ 2.1) and the next 3 as TE_1,1 (Q ≈ 1.3).

The same physical problem is computed in the time domain by the sister
project [`rjwalters/strata-fdtd`](https://github.com/rjwalters/strata-fdtd)
via FDTD; eigenfrequency-level cross-validation across the two
discretizations is the goal of this benchmark family.

The corresponding acceptance test
(`crates/geode-core/tests/mie_sphere.rs`) asserts (a) the lowest FEM
mode's `Re(k)` agrees with the analytic TM_1,1 root to within 30 % at
the bundled fixture's coarse resolution, and (b) the lowest TM_1,1
triplet's median Q is above 1.5 — a regression catch for PML
mis-configuration (σ₀ drift, mask break, vacuum-gap removal).
Tightening the Re(k) tolerance is the goal of follow-ups #33, #35, #38.

## License

MIT
