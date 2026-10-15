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
order-1 ladder still shows a pre-asymptotic 0.62 (initial qubit; 0.615008
from the ladder levels, stored as 0.6150) and 0.61 (final qubit), and
0.70–0.73 (resonator).

- **Estimate:** the mean of the order-1 series (`h/2`, `h/4`) and the
  order-2 series (`h`, `h/2`). Each is extrapolated at order 1 **in the
  frequency f** (`2 f₂ − f₁`).
- **Band:** spans six values:
  - those two;
  - the same two series at order 1 in the eigenvalue variable λ ∝ f²
    (`√(2 f₂² − f₁²)`), which is lower and sets the low side;
  - the same two series in f at the observed order, which is higher.

This is an extrapolation with a band, not a bound. The band's upper side is
**kept wide on purpose**: the observed-order values are an
extrapolation-only scenario, and the circuit cap below is not applied to
it. For the initial geometry, `[static_cross_check.initial].f_q_band_capped_ghz`
gives the band with the cap applied.

**The rule changed during part 1.** The work-in-progress rule took
Richardson at the observed order only, with a band reaching down to the
finest measured levels, and put the initial qubit at 4.63 GHz. The static
route below rules that out:
- The certified lower bound on C_Σ, with L_J alone, gives
  **4.477 GHz** (`circuit_cap_f_q_ghz`).
- Adding L_s ≥ 0 lowers the bare frequency.
- A coupling k > 0 only lowers the dressed qubit, which is the lower root of
  the two-node circuit.

So, under the circuit model, the initial qubit cannot exceed 4.477 GHz. The
rule now extrapolates at the asymptotic order 1. The eigenvalue-variable
values joined the band in the #1106 review. `results.toml` records this in
`rule_history`.

**Independent static route (initial geometry).** The certified tensor C_Σ
of X⁰ (`../transmon_diffopt/c_sigma_bracket_tensor.toml`, #1091) is read
through the two-node circuit of #1078/#1079 (L_J + L_s, C_Σ + C_J, coupling k
to the resonator), over every corner of the C_Σ, L_s and k bands.
- **Caveat: a different mesh of the same geometry.** The bracket is
  certified on `transmon_smoke.msh`, which the earlier generator produced.
  It is not certified on this benchmark's DeviceLayout mesh.
  - At h, Palace agrees on the two meshes: 3.4527 GHz on the fixture against
    3.4520 GHz here.
  - Their h/2 levels differ because they are different meshes: 3.9528 GHz
    from Palace's own refinement of the fixture (#1081) against 3.8771 GHz
    from gmsh's uniform refinement here.

## Results

| | published | Palace at DeviceLayout's discretization | converged estimate | band |
|---|---|---|---|---|
| initial qubit | 4.14 | 4.1683 (+0.68 %) | **4.444** (+7.3 %) | [4.415, 4.677]; capped [4.415, 4.477] |
| initial resonator | 5.591 | 5.6132 (+0.40 %) | **5.740** (+2.7 %) | [5.723, 5.817] |
| final qubit | 3.005 | 3.0164 (+0.38 %) | **3.265** (+8.7 %) | [3.236, 3.471] |
| final resonator | 3.982 | 3.9956 (+0.34 %) | **4.088** (+2.7 %) | [4.077, 4.139] |

All frequencies are in GHz.

- **Reproduction.** Palace reproduces both published pairs at DeviceLayout's
  own discretization to under 0.7 %.
- **Published values are under-resolved.** Both published pairs lie below
  every band, and therefore below the converged estimate.
- **Static route.** It gives an initial qubit of [4.449, 4.458] GHz. That is
  inside the eigen band and 0.22 % above the estimate.
- **Same-mesh agreement.** geode A2 and Palace order 1 agree to 1e-4 % on the
  qubit at every level.
  - **Resonator at h: 0.03 %.** On the DeviceLayout `h` meshes, Palace adds
    4946 elements (initial) and 5944 elements (final) by local bisection.
    Its log line reads "local bisection for under-resolved interior
    boundaries": the tets whose vertices all lie on the metal sheet. So
    Palace's `h` mesh is slightly finer than geode's.
  - **Resonator at h/2: 0.0000 %.** The refined meshes need no bisection,
    so both codes solve the same discrete problem.
  - **Checks.** Palace's linear solver is not the cause: the initial `h`
    cell rerun with AMS instead of SuperLU gives 5.176055 GHz again. Neither
    is the file format: geode on a gmsh MSH 4.1 round-trip of the mesh gives
    5.177592 GHz again.
- **Logs.** The committed solve logs print the peak RSS as `Some(…)`, the
  `Option` debug format of the generator at the time of the run. The
  generator now prints the number; the run records were always plain
  numbers.
- **Commits in the run records.** The four `geode_runs/` records were
  solved at b61fb044, the pre-rebase twin of 02038b40.
  `crates/geode-core/src`, this example and `Cargo.lock` are identical
  between the two, so `generated_at_commit` names 02038b40.
