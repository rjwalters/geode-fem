# crates/

The library and binary crates of the GEODE-FEM Cargo workspace. All of them
inherit version, edition, license and MSRV from `[workspace.package]` in the
root [`Cargo.toml`](../Cargo.toml). The standalone benchmark drivers
(`mie_sphere`, `patch_antenna`, `spiral_inductor`, …) are separate workspace
members under [`examples/`](../examples/).

| Crate | Purpose |
|---|---|
| [`geode-core`](geode-core/README.md) | The solver: FEM kernels over Burn, assembly, eigensolvers, the driven solve, ports and boundary conditions, post-processing, adjoint sensitivities, adaptive meshing |
| [`geode-cli`](geode-cli/README.md) | The `geode` binary: JSON/TOML problem spec plus a Gmsh mesh in, one versioned JSON report out |
| [`geode-optimize`](geode-optimize/README.md) | Pure-Rust, physics-independent L-BFGS-B and MMA optimizers with checkpoint/resume |
| [`geode-util`](geode-util/README.md) | Staging layer above `geode-core`: shared math, conversion, fixture, interop and viz helpers |
| [`geode-validation`](geode-validation/README.md) | Cross-backend reference tests against NumPy / JAX / Julia / ONNX / TF-Java golden fixtures |
| [`geode-app`](geode-app/README.md) | Shared clap harness, argument groups and exit-code lifecycle for the example binaries |

Dependency direction: `geode-core` is the bottom of the stack and depends on
no other workspace crate (it uses `geode-util` only as a dev-dependency).
`geode-util` and `geode-validation` sit above it, `geode-app` is independent,
and `geode-cli` pulls in `geode-core`, `geode-util` and `geode-app`.
`geode-optimize` has no workspace dependencies.

Build and test the whole workspace from the repository root:

```sh
cargo build
cargo test -p geode-core --release    # most integration tests need --release
```
