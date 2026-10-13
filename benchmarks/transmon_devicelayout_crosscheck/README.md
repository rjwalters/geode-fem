# `benchmarks/transmon_devicelayout_crosscheck/` — DeviceLayout.jl SingleTransmon cross-check (#1092)

The AWS DeviceLayout.jl blog's SingleTransmon example publishes two
designs with their Palace eigenfrequencies:

| Design | capacitor_length | resonator total_length | published (qubit, resonator) |
|---|---|---|---|
| initial | 620.0 µm | 5000.0 µm | [4.14, 5.591] GHz |
| final (PRIMA, 9 Palace runs) | 1217.678 µm | 7243.955 µm | [3.005, 3.982] GHz |

Both are computed at DeviceLayout's discretization (mesh_order 2,
solver_order 2) on DeviceLayout's own mesh, with the blog's warning that
the values are discretization-sensitive. This benchmark meshes both
geometries with DeviceLayout itself
(`reference/julia/generate_transmon_param.jl`, the two parameters exposed)
and solves them with geode's A2 tensor-ε eigen path and with Palace on a
resolution ladder, then compares the converged estimates with the published
pairs.

## Files

- `results.toml` — the artifact (MANIFEST bucket A). Written by
  `cargo run -p geode-core --release --example transmon_devicelayout_crosscheck`
  (assemble mode) from the inputs below. Pinned by
  `crates/geode-core/tests/transmon_devicelayout_crosscheck.rs`.
- `geode_runs/<label>.toml` — one A2 solve each (`initial_h`, `initial_h2`,
  `final_h`, `final_h2`), written by the same example in solve mode on an
  AWS r8g.8xlarge (the `h/2` solves factor about 1.3M-edge pencils).
- `logs/` — the solve logs and GNU `time -v` records of those runs.
- Palace cells: `reference/fixtures/transmon_palace/results_crosscheck_1092/`
  (`provenance.txt`, `mkcfg.py`, `run.sh`, per-cell outputs and logs).
- Meshes: `reference/fixtures/transmon_devicelayout/` (the two order-1
  meshes and the generator reports; the refinements and DeviceLayout's
  second-order meshes are regenerated, sha256 in `provenance.txt`).

## Ladder

Per geometry: Palace order 1 on `h`, `h/2`, `h/4` (the DeviceLayout mesh and
its uniform gmsh refinements), Palace order 2 on `h` and `h/2`, geode A2 on
`h` and `h/2` (same meshes, same-mesh agreement with Palace order 1), and
Palace on DeviceLayout's second-order mesh twice: the blog's exact config
(`dl_<tag>`: absorbing far field, 50 Ω readout ports) and the PEC-box model
(`dlpec_<tag>`). The converged estimate is the mean of the Richardson
extrapolations of the order-1 and order-2 series, with a band spanning both
extrapolations and both finest measured values. The numbers are in
`results.toml` and in the PR that landed them.
