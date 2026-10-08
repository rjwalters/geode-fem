# geode-core

The GEODE-FEM solver library. It holds the finite-element kernels written over
[Burn](https://burn.dev) tensors, global assembly into sparse `faer` systems,
the eigen and driven solvers, ports and boundary conditions, post-processing,
discrete-adjoint sensitivities and adaptive meshing. The `geode` CLI
([`../geode-cli`](../geode-cli/README.md)) and the benchmark drivers in
[`examples/`](../../examples/) are built on it. The root
[README](../../README.md) describes the validated benchmarks and the build
options in more detail.

## Modules

| Module | Contents |
|---|---|
| [`mesh`](src/mesh/) | `TetMesh`, the Gmsh MSH 4.1 reader, per-geometry fixtures and material tables, `PHYS_*` tags, partitioning |
| [`elements`](src/elements/) | Reference bases and batched local kernels: P1, P2, first- and second-order Nédélec, Whitney forms |
| [`assembly`](src/assembly/) | Global assembly: P1 and Nédélec (sparse `[nnz]`), electrostatic and magnetostatic systems, surface terms, periodic constraints, torque |
| [`derham`](src/derham/) | Discrete de Rham complex: gradient, curl and divergence incidence maps |
| [`eigen`](src/eigen/) | Generalized eigensolvers: dense `faer`, sparse shift-invert Lanczos (real and complex-symmetric), AMS preconditioning, Bloch, optional ARPACK |
| [`driven`](src/driven/) | Frequency-domain driven solve, lumped and wave ports, S-parameter extraction, adaptive (ROM) sweep, transient, Floquet, S-matrix sensitivities |
| [`solver`](src/solver/) | Krylov solvers (COCG with Jacobi / ILU / Chebyshev preconditioners), a GPU-resident Burn COCG, distributed matrix-free pieces |
| [`postproc`](src/postproc/) | Near-to-far-field transform (directivity, gain) and VTK `.vtu` writers |
| [`analytic`](src/analytic/) | Closed-form oracles: Mie, step-index fiber, dispersion, waveguide and port modes, patch cavity model, spiral, slotless PM motor |
| [`adjoint`](src/adjoint.rs), [`shape`](src/shape.rs) | Discrete-adjoint material and geometry sensitivities through a linear solve |
| [`adapt`](src/adapt/) | H(curl) a-posteriori error estimator, conforming refinement, adaptive loop |
| [`quantum`](src/quantum/) | Transmon parameters (`E_C`, `E_J`, spectrum, Kerr) from classical EM outputs |
| [`interop`](src/interop/) | Palace oracle configuration and result ingestion |
| [`traits`](src/traits/), [`prelude`](src/prelude.rs), [`constants`](src/constants.rs) | Core traits, common re-exports, physical constants |
| [`testing`](src/testing/) | Test-only backend selection and per-device tolerances (`testing` feature) |

Also in this crate: [`tests/`](tests/) (integration tests, with mesh fixtures
under [`tests/fixtures/`](tests/fixtures/)), [`examples/`](examples/) (benchmark
and design-loop binaries that write `benchmarks/*/results.toml`) and
[`benches/`](benches/) (criterion benches for assembly and eigensolves).

## Features

| Feature | Effect |
|---|---|
| `faer-parallel` (default) | Multi-threaded `faer` factorization and parallel host-side assembly via rayon |
| `wgpu`, `cuda`, `metal` | Burn GPU backends, all off by default. Enabling several is not an error: the first enabled of `cuda`, `metal`, `wgpu` is used. With none, tests use the `ndarray` f64 CPU backend |
| `autodiff` | Burn autodiff wrapper backend |
| `arpack` | ARPACK-backed sparse eigensolver; needs a system `libarpack` found by `pkg-config` |
| `spade-mesh` | In-process 2-D constrained Delaunay meshing for wave-port cross-sections |
| `testing` | The `testing` module |
| `flex` | Burn flex CPU backend, used only by the `flex_precision_spike` example |

## Build and test

```sh
cargo build -p geode-core
cargo test  -p geode-core --release                 # integration tests expect --release
cargo test  -p geode-core --release --test <name>    # one target
cargo run   -p geode-core --release --example motor_torque
cargo bench -p geode-core
```
