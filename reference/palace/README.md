# `reference/palace/` — Palace 3D oracle scaffolding (issue #239)

[Palace](https://github.com/awslabs/palace) (Petascale Algorithm for
Linear And Coupled Electromagnetic Simulation) is an open-source
MFEM-based 3D full-wave finite-element solver. It is the **eventual
gold-standard oracle** for the geode-fem driven-port benchmarks (Epic
#226 patch antenna, Epic #193 spiral inductor) — same wave equation,
independent implementation, similar Nédélec discretization.

**Status: scaffolded, not run.** Palace itself is *not* installed on
the geode-fem dev machine — only a Docker build recipe, now vendored
in-repo at `reference/palace/docker/`. This
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
├── geode_patch_baseline/
│   ├── Cargo.toml                       — offline-driver workspace
│   └── src/main.rs                      — Palace JSON config emitter for
│                                          the FR-4 patch fixture (#228)
└── geode_spiral_baseline/
    ├── Cargo.toml                       — offline-driver workspace
    └── src/main.rs                      — Palace JSON config emitter for
                                          the 3.5-turn spiral inductor
                                          fixture (#211 / #266)
```

The committed config outputs live under `reference/fixtures/`:

```
reference/fixtures/patch_palace/
├── palace_config.json                   — Palace 0.13 driven-port config
└── palace_config.provenance.txt         — generator provenance (mesh
                                          sha256, sweep / boundary
                                          mapping, operator workflow)

reference/fixtures/spiral_palace/
├── palace_config.json                   — Palace 0.13 driven-port config
└── palace_config.provenance.txt         — generator provenance
```

## Generating the config

```sh
# Patch antenna
cd reference/palace/geode_patch_baseline
cargo run --release
# → reference/fixtures/patch_palace/palace_config.json
#   reference/fixtures/patch_palace/palace_config.provenance.txt

# Spiral inductor (parity, issue #266)
cd reference/palace/geode_spiral_baseline
cargo run --release
# → reference/fixtures/spiral_palace/palace_config.json
#   reference/fixtures/spiral_palace/palace_config.provenance.txt
```

The generators are **offline** — they do *not* link against Palace; they
only write JSON. The in-repo Docker image
(`reference/palace/docker/`, built from `Dockerfile`) is the suggested runtime.

## Running Palace (operator-assisted)

From the geode-fem repo root, with a working Palace install:

```sh
# Patch antenna
palace -np 4 reference/fixtures/patch_palace/palace_config.json
# Or via the sister-repo Docker image (paths mounted at /work):
#   docker run --rm -v $(pwd):/work palace:latest \
#     -np 4 /work/reference/fixtures/patch_palace/palace_config.json

# Spiral inductor
palace -np 4 reference/fixtures/spiral_palace/palace_config.json
```

Palace writes the standard driven-solve artifacts (`s-parameters.csv`,
`port-V.csv`, `port-I.csv`, ...) under `postpro/patch_palace/` or
`postpro/spiral_palace/` respectively. The spiral fixture has roughly
2× the unique edges of the patch (~54k vs ~30k); plan for a
proportionally longer Palace run per frequency.

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

The benchmark tests
(`crates/geode-core/tests/patch_antenna_extraction.rs::
fem_vs_palace_oracle_within_band_or_skip_with_note` for the patch and
`crates/geode-core/tests/spiral_inductor_benchmark.rs::
fem_vs_palace_oracle_within_band_or_skip_with_note` for the spiral) then
compare the committed FEM sweep against the populated Palace block:

- **Patch antenna**: 5 % S11-dip-frequency band and a 0.10 absolute
  `|S11|` tracking band.
- **Spiral inductor**: 5 % L band at the 1 GHz reference point and a
  10 % Q tracking band at 4 GHz mid-band. (Note: the spiral Palace
  config currently emits PEC conductors — the lossless limit — while
  the FEM benchmark uses Leontovich surface impedance for skin-depth
  loss; expect a Q ratio > 1 in the lossless direction until matched
  conductor loss is configured. See the test's calibrated-band docs.)

**Until** an operator populates the slot, those tests print a clear
"SKIP" note (with the operator workflow inlined) and pass — the slot
in `benchmarks/{patch_antenna,spiral_inductor}/results.toml` stays
`status = "pending_operator_run"`. The tests **never silently pass**
on a missing oracle.

## Why "offline driver" and not a built-in test?

Same reason as the other offline drivers (e.g. `reference/numpy/`):

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
- **#239** — Palace scaffolding (config generator + ingester + patch
  benchmark test)
- **#266** — spiral parity (mirrors the patch wiring under the
  spiral benchmark)
- **#5** — operator-only tracker (catches the "the operator has to
  run X" steps so they don't get lost)
