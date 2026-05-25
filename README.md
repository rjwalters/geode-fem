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

## License

MIT
