# `reference/palace/` — Palace 3D oracle scaffolding (issue #239)

[Palace](https://github.com/awslabs/palace) (Petascale Algorithm for
Linear And Coupled Electromagnetic Simulation) is an open-source
MFEM-based 3D full-wave finite-element solver. It is the **eventual
gold-standard oracle** for the geode-fem driven-port benchmarks (Epic
#226 patch antenna, Epic #193 spiral inductor) — same wave equation,
independent implementation, similar Nédélec discretization.

**Status: scaffolded, not run.** Palace itself is *not* installed on
the geode-fem dev machine — only a Docker build recipe exists in the
sister monorepo at `~/GitHub/sphere/eda/mom/docker/palace`. This
directory holds the **glue around** Palace — the config generator and
the operator workflow — so a Palace reference can be produced by
anyone with a working install (or the Docker image) and slotted into
the benchmark TOMLs.

The corresponding **result ingester** (parser that converts Palace's
output artifacts into the `[oracles.palace]` TOML block the benchmark
tests consume) is in `crates/geode-core/src/palace.rs`, where it is
unit-tested against synthetic fixtures under
`crates/geode-core/tests/fixtures/palace/`.

## Layout

```
reference/palace/
├── README.md                            — this file
└── geode_patch_baseline/
    ├── Cargo.toml                       — offline-driver workspace
    └── src/main.rs                      — Palace JSON config emitter for
                                          the FR-4 patch fixture (#228)
```

The committed config output lives at
`reference/fixtures/patch_palace/`:

```
reference/fixtures/patch_palace/
├── palace_config.json                   — Palace 0.13 driven-port config
└── palace_config.provenance.txt         — generator provenance (mesh
                                          sha256, sweep / boundary
                                          mapping, operator workflow)
```

## Generating the config

```sh
cd reference/palace/geode_patch_baseline
cargo run --release
# → reference/fixtures/patch_palace/palace_config.json
#   reference/fixtures/patch_palace/palace_config.provenance.txt
```

The generator is **offline** — it does *not* link against Palace; it
only writes JSON. The sister-repo Docker image
(`~/GitHub/sphere/eda/mom/docker/palace`) is the suggested runtime.

## Running Palace (operator-assisted)

From the geode-fem repo root, with a working Palace install:

```sh
palace -np 4 reference/fixtures/patch_palace/palace_config.json
# Or via the sister-repo Docker image (paths mounted at /work):
#   docker run --rm -v $(pwd):/work palace:latest \
#     -np 4 /work/reference/fixtures/patch_palace/palace_config.json
```

Palace writes the standard driven-solve artifacts (`s-parameters.csv`,
`port-V.csv`, `port-I.csv`, ...) under `postpro/patch_palace/`.

## Ingesting results

The s-parameters CSV is what the geode-fem `[oracles.palace]` slot
consumes. From a Rust call site (e.g. a one-shot ingest binary):

```rust
use geode_core::palace::{PalaceResults, PALACE_DEFAULT_PORT_OHM};

let r = PalaceResults::from_palace_csv_file(
    std::path::Path::new("postpro/patch_palace/s-parameters.csv"),
    "0.13.0-git",                                  // Palace version string
    "<sha256 of palace_config.json>",              // provenance
    PALACE_DEFAULT_PORT_OHM,
)?;
// Then serialize `r` into the `[oracles.palace]` block of
// `benchmarks/patch_antenna/results.toml` (populated shape — see
// `PalaceOracleSlot` in `crates/geode-core/src/palace.rs`).
```

The benchmark test
(`crates/geode-core/tests/patch_antenna_extraction.rs::
fem_vs_palace_oracle_within_band_or_skip_with_note`) then compares the
committed FEM sweep against the populated Palace block within a 5 %
S11-dip-frequency band and a 0.10 absolute `|S11|` tracking band.

**Until** an operator populates the slot, that test prints a clear
"SKIP" note (with the operator workflow inlined) and passes — the slot
in `benchmarks/patch_antenna/results.toml` stays
`status = "pending_operator_run"`. The test **never silently passes**
on a missing oracle.

## Why "offline driver" and not a built-in test?

Same reason as `reference/mom/geode_*_baseline/`:

- Palace (and mom) are heavyweight third-party solvers that are not
  installed everywhere; making the geode workspace depend on either
  would block every contributor from running `cargo test`.
- The offline driver pattern keeps the **config generation** path in
  Rust (so it stays in sync with the geode-fem fixture API), while the
  **run** stays an explicit operator step.
- The result ingester (`crates/geode-core/src/palace.rs`) is the only
  Rust code on the hot path of CI; it's parsed and tested against a
  clearly-labeled synthetic fixture.

## Parent epics

- **#226** — patch antenna benchmark (Phase 2 #228, matched feed #237)
- **#193** — spiral inductor benchmark (Phase 3 #211)
- **#239** — this scaffolding (config generator + ingester + one
  wired benchmark test)
- **#5** — operator-only tracker (catches the "the operator has to
  run X" steps so they don't get lost)
