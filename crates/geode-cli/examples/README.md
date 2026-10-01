# `geode` cookbook

Runnable, worked examples for every `geode` analysis (issue #709). Each
directory holds the input(s) and a README with the exact command, what
the example models, and the output to expect.

| Analysis | Example | Input | Runtime (debug / release) |
|---|---|---|---|
| `driven` | [3.5-turn spiral inductor, 1–20 GHz sweep](driven/README.md) | `driven/spiral_inductor.json` | ~30 s / a few s |
| `driven` (adaptive) | [the same spiral, 40-point adaptive sweep (10 full solves)](driven/README.md#adaptive-40-point-sweep-sweepadaptive-issue-708) | `driven/spiral_inductor_adaptive.json` | ~1.5 min / ~7 s |
| `driven` (rough Cu) | [the same spiral with 1 µm RMS Hammerstad copper roughness](driven/README.md#rough-copper-roughness-issue-758) | `driven/spiral_inductor_rough.json` | ~30 s / a few s |
| `extract` | [SLCFET spiral, `L₀` by f → 0 extrapolation](extract/README.md) | `extract/slcfet_spiral.json` | ~30 s / a few s |
| `eigen` | [PEC-walled dielectric-sphere cavity modes, lossless and lossy (f, Q)](eigen/README.md) | `eigen/sphere_cavity.json`, `eigen/lossy_sphere_cavity.json` | ~12 s / ~1 s (lossy: ~40 s / ~1 s) |
| `capacitance` | [coax and triax Maxwell capacitance matrices](capacitance/README.md) | `capacitance/coax.json`, `capacitance/triax.toml` | < 1 s |
| `inductance` | [coax and triax Maxwell inductance matrices](inductance/README.md) | `inductance/coax.json`, `inductance/triax.toml` | ~1 s |
| `sensitivity` | [material gradients `∂C/∂ε_r`, `∂L/∂ν_r`, `∂\|S11\|²/∂ε_r` with an FD self-check](sensitivity/README.md) | `sensitivity/capacitance_coax.json`, `sensitivity/inductance_triax.toml`, `sensitivity/driven_spiral.json` | ~8 s each (driven: ~1.5 min) (debug) |
| `mesh` → solve | [layout → Gmsh mesh → driven / capacitance / inductance](mesh/README.md) | `mesh/*.layout.json` | a few s (needs Gmsh) |

Commands are written from the repository root, but they work from
anywhere: a spec's relative `mesh.path` is resolved against the spec's own
directory. The meshes are the committed test meshes under
`crates/geode-core/tests/fixtures/`, so a `cargo install --git` checkout or
a clone is all you need (plus [Gmsh](https://gmsh.info) for `geode mesh`).
Every run writes one JSON report to stdout (or `-o report.json`); `geode
check <spec>` validates a spec and prints DOF counts and a resource
estimate without solving.

## Where the inputs come from

Every spec here (and `mesh/spiral_inductor.layout.json`) except
`driven/spiral_inductor_adaptive.json` (the spiral fixture re-cut as a
40-point adaptive sweep; its numbers are the issue-#708 measurement, and
`tests/adaptive_sweep_golden.rs` pins a 16-point twin) is a
**byte-identical copy of a golden-test fixture** in
`crates/geode-cli/tests/fixtures/`: the numbers quoted in the READMEs are
the ones the golden tests pin, and `tests/cookbook.rs` fails if a copy
drifts from its fixture. The two static layouts in `mesh/` are the
default-tier geometries of `tests/mesh_static_golden.rs`. CI runs `geode
check` on every spec here and `geode mesh` (+ `check` on the starter spec)
on every layout.

## Validating your own inputs

The input and output contracts are published as JSON Schema (draft
2020-12) in [`../schemas/`](../schemas/) and printed by `geode schema
spec|report|layout`. A schema catches typos, wrong types and unknown keys
before you run anything; it does **not** replace `geode check`, which also
enforces the cross-field rules (see the crate README, "JSON Schema").

```sh
geode schema spec > spec.schema.json
check-jsonschema --schemafile spec.schema.json my_spec.json   # or any 2020-12 validator
geode check my_spec.json                                      # the authority
```
