# GEODE-FEM examples

This directory holds the **standalone example crates** for GEODE-FEM
(Epic #398). Each example is its own workspace member — a real binary
crate built on the shared [`geode-app`](../crates/geode-app) harness —
rather than a `cargo --example` target inside `geode-core`.

```
examples/
├── README.md            # this file (the per-example template + rules)
├── _support/            # geode-examples-support: shared, example-only viz glue
│   ├── Cargo.toml
│   └── src/lib.rs       # edge_field_to_nodes (Whitney edge-DOF → nodal E)
└── mie_sphere/          # pilot migration (Phase 2)
    ├── Cargo.toml
    └── src/main.rs
```

## Workspace wiring

The root `Cargo.toml` `members` list includes the glob `"examples/*"`, so
**every directory here that contains a `Cargo.toml` is automatically a
workspace member**. Adding a new example in Phase 3 requires *no* edit to
the root manifest (this deliberately avoids the merge contention seen in
epic #377). Non-package files like this `README.md` are ignored by the
member glob.

Verify the wiring with:

```sh
cargo metadata --no-deps --format-version 1 | grep -o '"name":"[^"]*"'
# … should list mie_sphere and geode-examples-support …
cargo build --workspace            # builds every example crate
cargo build --workspace --all-targets
```

## Per-example crate template

Copy this shape for a new example `<name>`:

### `examples/<name>/Cargo.toml`

```toml
[package]
name = "<name>"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true
description = "<one-line description>."

[[bin]]
name = "<name>"
path = "src/main.rs"

[dependencies]
clap = { workspace = true }
geode-app = { workspace = true }
geode-core = { workspace = true }
# Only if the example reconstructs Nédélec edge fields for a `.vtu` dump:
geode-examples-support = { workspace = true }
# Only if the example touches faer directly. Add `sparse-linalg` only if it
# uses the sparse solvers (SparseColMat / Triplet); the workspace base set
# (std, linalg) is otherwise enough:
faer = { workspace = true, features = ["sparse-linalg"] }
# Only if the example needs a Burn device handle / tensors directly:
burn = { workspace = true }

[lints.rust]
unsafe_code = "forbid"
```

**Backend features**: example crates define **no** backend features of
their own. The Burn backend (`wgpu` by default, `ndarray` on headless CI)
is selected by `geode-core`'s default features and the workspace
`--features` flags at build time — exactly like `geode-cli` and
`geode-validation`. Do **not** add `default-features = false` /
`wgpu` / `ndarray` to the `geode-core` dependency line.

### `examples/<name>/src/main.rs`

```rust
use std::process::ExitCode;

use clap::Parser;
use geode_app::{App, OutputDir, Verbosity};

/// One-line `--help` description.
#[derive(Parser)]
#[command(about = "…")]
struct Args {
    // Example-local flags go here, e.g.:
    //   #[arg(long)]
    //   dense: bool,

    /// Artifact output directory (`--out-dir`, default `artifacts/`).
    #[command(flatten)]
    out: OutputDir,

    /// Verbosity (`-v` / `-q`), fed to the logging seam.
    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let dir = self.out.resolve()?; // create + return the artifact dir
        // … the example body; map errors with `?` …
        let _ = dir;
        Ok(())
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<Args>()
}
```

`geode_app::main::<Args>()` parses the args, runs the (currently no-op)
observability seam on the resolved [`Verbosity`], calls `App::run`, and
maps `Ok → SUCCESS` / `Err → FAILURE` (printing the `error:` + `caused
by:` source chain to stderr). See `crates/geode-app` for the full harness
docs.

#### Output paths

Use the flattened `OutputDir` group for file outputs: call
`self.out.resolve()?` to create and obtain the `--out-dir` directory
(default `artifacts/`), then write fixed-name files inside it (e.g.
`out_dir.join("E_field.vtu")`). This routes the path/dir concern through
`geode-app` instead of hand-rolling `--export-field <path>` +
`create_dir_all(parent)`.

Benchmark artifacts that live at a **fixed committed repo path** (e.g.
`benchmarks/mie_sphere/results.toml`, consumed by a test) must NOT go
through `OutputDir`; keep the `env!("CARGO_MANIFEST_DIR")`-relative
walk-up. From an example crate root `examples/<name>/`, the workspace root
is two levels up (`../../`).

## `--release` requirement

Several examples must be run in `--release`. faer 0.24's dense generalized
eigensolver (`gevd` / QZ) path panics under `debug-assertions`
(issues #244 / #354), so any example that exercises the dense complex
eigensolver — including the dense fallback paths — must use release mode:

```sh
cargo run -p <name> --release
```

| Example | `--release` required? | Why |
|---------|-----------------------|-----|
| `mie_sphere` | **Yes** | dense `gevd` fallback (`--dense`) + slow optimized linear algebra |

CI builds example crates via `cargo build --workspace --all-targets`
(building is fine in debug; only *running* the `gevd` path panics). Do not
add a CI job that *runs* a `--release`-only example in debug.

## Pilot: `mie_sphere`

`mie_sphere` is the Phase 2 reference migration. It:

- derives `Args` with `clap`, flattening `OutputDir` + `Verbosity`;
- keeps `--dense`, `--scalar-pml`, and `--export-field` as example-local
  flags (`--export-field` is now a boolean toggle that writes
  `<out-dir>/E_mie.vtu`);
- implements `geode_app::App` and uses
  `geode_examples_support::edge_field_to_nodes` (no `#[path]` include);
- preserves the eigenmode report, the `benchmarks/mie_sphere/results.toml`
  artifact, and the exported `.vtu` content exactly.

```sh
cargo run -p mie_sphere --release                       # eigenmode benchmark
cargo run -p mie_sphere --release -- --dense            # dense oracle
cargo run -p mie_sphere --release -- --scalar-pml       # legacy PML cross-check
cargo run -p mie_sphere --release -- --export-field     # write artifacts/E_mie.vtu
```
