# GEODE-FEM

**GPU-accelerated Electromagnetic Open Differentiable Engine — FEM/DG**

> Status: v0 milestone hit. End-to-end FEM stack (mesh I/O → P1 + Nédélec
> kernels → autodiff-preserving assembly → dense and sparse complex eigensolvers
> → PEC / Silver-Müller / PML absorbing BCs) is on `main`, validated against
> the analytic PEC-cavity Mie spectrum. See **Highlights** below.

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
| [rjwalters/geode-fem](https://github.com/rjwalters/geode-fem) | Burn-based realization of the whiteroom L4 specification | FEM/DG | v0 complete; Mie eigenmodes validated |
| [rjwalters/strata-fdtd](https://github.com/rjwalters/strata-fdtd) | FDTD time-domain solver (acoustic today, EM in progress) | FDTD | Active |

Strata and GEODE-FEM are sister codebases that use **different discretizations**
for **overlapping physics**. Mie resonances are the canonical cross-check
benchmark — analytical Mie series ↔ strata FDTD ↔ GEODE-FEM eigenmode.

## Highlights

| | |
|---|---|
| **Mie comparison** | Lowest TM_1,1 mode (n=1.5 dielectric sphere, R/R_buffer=1/2, PEC outer + PML buffer): FEM Re(k) ≈ 1.09 vs analytic 1.30343, **16% rel err / Q ≈ 5.8** on the bundled 774-node fixture. Diagnosed (see #54): mesh refinement does **not** improve this — the scalar-isotropic PML reflection is an h-independent modelling floor. |
| **Performance** | Sparse complex-symmetric Lanczos (Bai *Templates* §7.13) brings the Mie eigensolve from **126 s → 4 s** on the 774-node fixture (31× speedup; 107× on the original 313-node fixture). The Mie example uses the sparse path by default; pass `--dense` for the correctness oracle. |
| **Math correctness** | M_{ij} = ∫ N_i · N_j ε(x) dV is **complex-symmetric** (M^T = M), not Hermitian (M^H ≠ M) — the Mie inner product is bilinear, not sesquilinear. Caught by a builder during PR #55, validated by both empirical check (`Im(v^H M v) ≈ −58`) and by-hand derivation. |
| **Validated chain** | 27 PRs merged. Scalar Helmholtz cube modes (4.1% rel err at n=10), batched P1 + Nédélec local kernels with autodiff through assembly, dense (`faer::generalized_eigen`) and sparse (shift-and-invert Lanczos) eigensolvers, PEC cube + PEC sphere + Silver-Müller + PML absorbing BCs, all with regression tests. |

See [`benchmarks/mie_sphere/results.toml`](benchmarks/mie_sphere/results.toml) for
the current Mie comparison table and
[`benchmarks/perf/baseline.toml`](benchmarks/perf/baseline.toml) for wall-clock
baselines.

## Roadmap (v0)

- [x] Cargo workspace skeleton with Burn dependency
- [x] Scalar Helmholtz on a tetrahedral mesh (warmup before vector Maxwell)
- [x] Vector curl-conforming (Nédélec) elements
- [x] First eigenmode solver for a dielectric sphere
- [x] Mie scattering benchmark vs. analytic series and strata-fdtd
- [ ] Map whiteroom L4 specification → GEODE-FEM operators (tracker, ongoing)

### v1 (active)

- [ ] **Anisotropic UPML** (issue #54) — canonical fix for the 16% PML accuracy ceiling
- [ ] **Vector-tracking k₀** (issue #48) — replace frozen-index Newton for self-consistent resonance tracking
- [ ] **Driven scattering** (Q_ext, Q_sca vs. ka) — v1 of the Mie benchmark
- [ ] **Whiteroom L4 mapping** (issue #5) — once upstream slices stabilize

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

**Per-stage cost (n=10 cube):**

| stage                              | median   |
| ---------------------------------- | -------- |
| `assemble_global_p1`               |  45 ms   |
| `assemble_global_nedelec` (real)   | 289 ms   |
| `assemble_global_nedelec` (cmplx)  | 407 ms   |
| `FaerDenseEigensolver`             |  5.95 s  |
| `SparseShiftInvertLanczos`         |  52 ms   |

Dense `generalized_eigen` dwarfs every other stage by ~100×. The
pure-Rust sparse shift-and-invert Lanczos (faer sparse LU) brings the
eigensolve down to roughly the same order as the Burn-side assembly.

**Mie sphere end-to-end (774-node refined fixture, complex pencil):**

| solver path                                | median  |
| ------------------------------------------ | ------- |
| `FaerComplexEigensolver` (dense)           | 126.1 s |
| `SparseComplexShiftInvertLanczos` (sparse) | **4.07 s** |

**31× speedup at this scale; 107× on the original 313-node fixture.**
The sparse path is now the default in `examples/mie_sphere.rs`; pass
`--dense` for the correctness-oracle cross-check. The dense path is
retained because its math is straightforward, the sparse path's
bilinear Lanczos (Bai §7.13) has slightly weaker orthogonality
guarantees on tight clusters, and the dense numbers serve as the
ground truth that the sparse path is currently within ~5e-4 relative
error of on the lowest two physical modes.

### Mie sphere (issue #4)

The project's stated north-star validation problem: FEM eigenmodes of a
dielectric sphere (refractive index `n = 1.5`, radius `R = 1`) inside a
vacuum buffer (`r ≤ R_buffer = 2`) terminated by a scalar-isotropic PML,
compared against analytic resonance roots.

Run the benchmark:

```sh
cargo run -p geode-core --release --example mie_sphere           # sparse (default)
cargo run -p geode-core --release --example mie_sphere -- --dense # dense oracle
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

**Mesh**: bundled 774-node / 3335-tet fixture (`tests/fixtures/sphere.msh`,
regenerated from `mesh_scripts/sphere.geo` via Gmsh CLI). Layered into
`sphere_interior` (`r ≤ 1`) + `vacuum_gap` (`1 < r ≤ 1.5`) + `pml_shell`
(`1.5 < r ≤ 2`) + boundary triangles.

**Catalog**: roots for angular orders `l ∈ [1, 4]`, both TE and TM
polarisations, lowest 5 radial overtones each (~40 entries). Each
root carries its `(l, n, polarisation, multiplicity = 2l+1)` label.

**Mode classification**: walks the catalog in ascending `k` and for
each analytic root claims the next `2l + 1` consecutive FEM modes
(sorted by `Re(k)`), producing an unambiguous `(l, n, pol, m_idx)`
label per mode. On the bundled fixture this identifies the lowest 3
FEM modes as the TM_1,1 triplet (Q ≈ 5.8) and the next 3 as TE_1,1
(Q ≈ 3.1).

**Current numbers** (bundled fixture, σ₀ = 5.0):

| mode    | analytic kR | FEM Re(kR) | rel err Re(k) | Q     |
| ------- | ----------- | ---------- | ------------- | ----- |
| TM_1,1  | 1.30343     | ≈ 1.092    | ≈ 16.2%       | ≈ 5.8 |
| TE_1,1  | 1.88943     | ≈ 1.581    | ≈ 16.3%       | ≈ 3.1 |

**Accuracy ceiling — important strategic finding** (issue #52): the
TM_1,1 / TE_1,1 / TM_2,1 modes ALL sit at ~16% rel err independent of
mesh refinement. If P1 Nédélec discretization were the dominant
error, higher-l modes would be worse — they aren't. This is the
signature of an h-independent **scalar-isotropic PML reflection
floor** at the inner PML interface. Refining further does not help.

The canonical fix is **anisotropic UPML** with tensor permittivity
(tracked as [#54](https://github.com/rjwalters/geode-fem/issues/54)).
This is the next critical-path accuracy work and pairs naturally with
the just-landed sparse complex Lanczos (PR #55) since UPML requires
iterating at refined mesh where the dense eigensolve was previously
intractable.

The same physical problem is computed in the time domain by the sister
project [`rjwalters/strata-fdtd`](https://github.com/rjwalters/strata-fdtd)
via FDTD; eigenfrequency-level cross-validation across the two
discretizations is the goal of this benchmark family.

The acceptance test (`crates/geode-core/tests/mie_sphere.rs`) asserts
(a) the lowest FEM mode's `Re(k)` agrees with the analytic TM_1,1 root
to within 25% at the bundled fixture's resolution, and (b) the lowest
TM_1,1 triplet's median Q is above 1.5 — a regression catch for PML
mis-configuration (σ₀ drift, mask break, vacuum-gap removal).
Tightening the Re(k) tolerance is the goal of follow-ups #38, #48,
and especially #54.

## License

MIT
