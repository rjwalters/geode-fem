# GEODE-FEM examples

This directory holds the **standalone example crates** for GEODE-FEM
(Epic #398). Each example is its own workspace member — a real binary
crate built on the shared [`geode-app`](../crates/geode-app) harness —
rather than a `cargo --example` target inside `geode-core`. Run any of
them with:

```sh
cargo run -p <name> [--release] [-- <example-flags>]
```

Every example also accepts the shared `geode-app` flags via
`#[command(flatten)]`: `--out-dir <dir>` (artifact output directory,
default `artifacts/`) and `-v` / `-q` (verbosity). Those are omitted
from the per-example invocations below.

## Example index (12 crates)

> `--release` note: faer 0.24's dense generalized eigensolver
> (`gevd` / QZ) panics under `debug-assertions` (issues #244 / #354), so
> every example that exercises a dense complex eigensolve must run in
> release mode. Only `extract_baseline` (a pure TOML post-processor) runs
> fine in debug.

### Eigenmode / scattering benchmarks

- **`mie_sphere`** — `--release` — Mie-sphere FEM-eigenmode-vs-analytic
  benchmark (Epic #398 pilot crate). Writes
  `benchmarks/mie_sphere/results.toml`.
  ```sh
  cargo run -p mie_sphere --release                   # anisotropic UPML, sparse (default)
  cargo run -p mie_sphere --release -- --dense        # dense QZ oracle fallback
  cargo run -p mie_sphere --release -- --scalar-pml   # legacy scalar-PML cross-check
  cargo run -p mie_sphere --release -- --export-field # also write <out-dir>/E_mie.vtu
  ```

- **`mie_driven_scattering`** — `--release` — Driven Mie scattering
  benchmark: `Q_ext` / `Q_sca` vs. `ka` against the analytic Mie series
  (issue #195).
  ```sh
  cargo run -p mie_driven_scattering --release           # coarse fixture
  cargo run -p mie_driven_scattering --release -- --fine # finer fixture
  ```

- **`mie_open_quasimode`** — `--release` — Matched (full Sacks) UPML
  open-space quasi-mode benchmark vs. the open-space Mie WGM complex roots
  (issue #213).
  ```sh
  cargo run -p mie_open_quasimode --release            # sparse (default)
  cargo run -p mie_open_quasimode --release -- --dense # dense QZ oracle fallback
  ```

### Antenna / interconnect extraction

- **`patch_antenna`** — `--release` —
  S11 / resonance / bandwidth / efficiency / radiation benchmark vs the
  Balanis cavity-model oracle (issue #228). The positional mode selects
  the run (default `benchmark`); `--export-field` / `--export-sweep`
  override it to dump `.vtu` artifacts into `--out-dir`.
  ```sh
  cargo run -p patch_antenna --release                # benchmark S11 sweep (default)
  cargo run -p patch_antenna --release -- smoke       # coarse fixture
  cargo run -p patch_antenna --release -- matched     # impedance-matched fixture
  cargo run -p patch_antenna --release -- pattern     # NTFF radiation pattern
  cargo run -p patch_antenna --release -- pattern-3d  # 3D radiation-lobe .vtu
  cargo run -p patch_antenna --release -- --export-field --out-dir artifacts/viz
  cargo run -p patch_antenna --release -- --export-sweep --out-dir artifacts/viz/patch_sweep \
      --f-start 2.0 --f-stop 3.0 --n 11
  ```
  (Other modes: `pattern-smoke`, `pattern-matched`.)

- **`spiral_inductor`** — `--release` — Spiral-inductor extraction
  benchmark: L / R / Q / S11 / SRF vs the Mohan analytic + `mom` PEEC
  oracles (issue #211).
  ```sh
  cargo run -p spiral_inductor --release          # benchmark fixture (default)
  cargo run -p spiral_inductor --release -- smoke # coarse fixture
  cargo run -p spiral_inductor --release -- --export-field
  ```

- **`slcfet_3hp_spiral`** — `--release` — SLCFET 3HP (GaN-on-SiC)
  spiral-inductor extraction benchmark: L / R / Q / S11 / SRF vs the `mom`
  PEEC + Mohan oracles (issue #212).
  ```sh
  cargo run -p slcfet_3hp_spiral --release          # benchmark fixture (default)
  cargo run -p slcfet_3hp_spiral --release -- smoke # coarse fixture
  ```

### Waveguide / fiber benchmarks

- **`soi_waveguide`** — `--release` — Silicon-on-insulator strip-waveguide
  benchmark: fundamental quasi-TE `n_eff` vs the effective-index-method
  oracle (issue #306).
  ```sh
  cargo run -p soi_waveguide --release
  ```

- **`step_index_fiber`** — `--release` — SMF-28 step-index circular-fiber
  benchmark vs the exact LP-mode oracle on the PML-terminated
  complex-pencil solver (issue #333).
  ```sh
  cargo run -p step_index_fiber --release
  ```

- **`fiber_smf28_solve_timing`** — `--release` — Performance harness timing
  the SMF-28 fiber `p=2` modal solve on the sparse-direct assembly path
  (issue #327).
  ```sh
  cargo run -p fiber_smf28_solve_timing --release
  ```

### Convergence / tooling

- **`eigen_convergence`** — `--release` — Quick convergence probe: prints
  the (1,1,1) ground-mode eigenvalue of the unit-cube Dirichlet Laplacian
  across mesh refinements to show the O(h²) rate.
  ```sh
  cargo run -p eigen_convergence --release
  ```

- **`regen_cube_convergence_fixture`** — `--release` — Regenerate the
  committed cube-convergence regression fixture
  (`crates/geode-core/tests/fixtures/cube_convergence.toml`).
  ```sh
  cargo run -p regen_cube_convergence_fixture --release
  ```

- **`extract_baseline`** — *(debug OK)* — Walk `target/criterion` and write
  a clean TOML perf baseline (medians + IQR) to
  `benchmarks/perf/baseline.toml` (issue #50). Run after
  `cargo bench -p geode-core`.
  ```sh
  cargo run -p extract_baseline
  ```

## Workspace wiring

The root `Cargo.toml` `members` list includes the glob `"examples/*"`, so
**every directory here that contains a `Cargo.toml` is automatically a
workspace member** — adding a new example requires *no* edit to the root
manifest (this deliberately avoids the merge contention seen in epic
#377). Non-package files like this `README.md` are ignored by the member
glob.

Verify the wiring with:

```sh
cargo metadata --no-deps --format-version 1 | grep -o '"name":"[^"]*"'
# … should list all 12 example crates …
cargo build --workspace            # builds every example crate
cargo build --workspace --all-targets
```

CI builds the example crates via `cargo build --workspace --all-targets`
(building is fine in debug; only *running* the `gevd` path panics). Do not
add a CI job that *runs* a `--release`-only example in debug.

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
# Only if the example reconstructs Nédélec edge fields for a `.vtu` dump
# (call `geode_util::viz::edge_field_to_nodes`):
geode-util = { workspace = true }
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
