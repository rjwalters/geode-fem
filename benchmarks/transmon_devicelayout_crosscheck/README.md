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
(`dlpec_<tag>`).

## Rule

Every level rises under refinement: the conforming discretization
over-estimates the island capacitance, so each series approaches from
below. The thin-sheet edge singularity (E ~ r^-1/2) limits the asymptotic
eigenvalue rate to O(h) on uniform meshes at any element order. The
order-1 ladder still shows a pre-asymptotic 0.61 (qubit) and 0.70–0.73
(resonator).

- **Estimate:** the mean of the order-1 series (`h/2`, `h/4`) and the
  order-2 series (`h`, `h/2`), each extrapolated at order 1.
- **Band:** spans those two values and the same two series extrapolated at
  the observed order, which extrapolates further.

This is an extrapolation with a band, not a bound.

**Independent static route (initial geometry).** The certified tensor C_Σ
of X⁰ (`../transmon_diffopt/c_sigma_bracket_tensor.toml`, #1091), read
through the two-node circuit of #1078/#1079 (L_J + L_s, C_Σ + C_J,
coupling k to the resonator) over every corner of the C_Σ, L_s and k bands.

## Results

| | published | Palace at DeviceLayout's discretization | converged estimate | band |
|---|---|---|---|---|
| initial qubit | 4.14 | 4.1683 (+0.68 %) | **4.444** (+7.3 %) | [4.432, 4.677] |
| initial resonator | 5.591 | 5.6132 (+0.40 %) | **5.740** (+2.7 %) | [5.727, 5.817] |
| final qubit | 3.005 | 3.0164 (+0.38 %) | **3.265** (+8.7 %) | [3.254, 3.471] |
| final resonator | 3.982 | 3.9956 (+0.34 %) | **4.088** (+2.7 %) | [4.080, 4.139] |

All frequencies are in GHz.

- **Reproduction.** Palace reproduces both published pairs at DeviceLayout's
  own discretization to under 0.7 %.
- **Published values are under-resolved.** Both published pairs lie below
  every band, so DeviceLayout's published values are not converged values.
- **Static route.** It gives an initial qubit of [4.449, 4.458] GHz. That is
  inside the eigen band and 0.22 % above the estimate.
- **Same-mesh agreement.** geode A2 and Palace order 1 agree to 1e-4 % on the
  qubit and 0.03 % on the resonator, on every mesh where both ran.
