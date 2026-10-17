# `benchmarks/transmon_devicelayout_controls/` — controls for the DeviceLayout.jl SingleTransmon study (#1123, #1129, #1135)

The two independent reviews of transmon paper v11 asked for four things the
#1092 work (#1106 cross-check, #1111 re-optimization) had not measured:

1. a gradient-free control for #1111's "DeviceLayout's criterion in 3 Palace
   runs, where PRIMA used 9";
2. an independent convergence check of the extrapolated initial qubit
   (4.444 GHz, band [4.415, 4.677]) by Palace adaptive mesh refinement;
3. the mesh-family spread behind the ~2 % h/2 discrepancy of the #1111
   geometry caveat;
4. the inputs of the 4.4766 GHz circuit cap, and why the reviewers' shared
   limit (4.4802 GHz) lies above it.

Every frequency here is a Palace run (fba6a5b, the #1092 Dockerfile), and no
new geode solve was made. The one geode-derived input is δ = 215.954 µm, the
end correction of #1111's transmission-line model. #1111 calibrated it once
from a finite difference of two geode order-1 solves. The scaling-law Newton
uses it in its L column, exactly as the geode-gradient Newton does (see §1).
All values are in GHz unless stated.

§5 adds the three follow-ups of #1129 (from the v12 reviews): PRIMA re-run on
this Palace environment, the scaling-law Newton with a Palace-only δ (no geode input
at all), and a second refinement family on the step-2 design. §6 adds the
two follow-ups of #1135 (from the v13 reviews): a second refinement family on
the published final design, and PRIMA at two more initial trust radii.

## Files

- `results.toml`: the artifact (MANIFEST bucket E: hand provenance, Palace
  only). Written by `python3 -I benchmarks/transmon_devicelayout_controls/assemble.py`
  from the raw Palace cells and the committed #1079 / #1092 / #1111
  artifacts. Pinned by `crates/geode-core/tests/transmon_devicelayout_controls.rs`.
- `driver.py`: the two gradient-free controls and the conv-step design
  (DeviceLayout meshes on the Mac, Palace on an AWS worker).
- `ladder_palace.sh`: the Palace half of #1111's evaluation ladder at one
  design.
- `worker_amr.sh`: the worker side of the adaptive and refinement-scheme
  cells, with a host-memory sampler.
- `collect.sh`: syncs the workers and lays out the committed cells.
- Palace cells, logs and `provenance.txt`:
  `reference/fixtures/transmon_palace/results_controls_1123/`.
- #1129: `prima_driver.jl` (DeviceLayout's PRIMA call, each point a Palace
  run through `driver.py eval`; `--replay` re-runs PRIMA on the committed
  cells), `driver.py scaling-palace`, `collect_1129.sh`; cells, logs and
  `provenance.txt` in
  `reference/fixtures/transmon_palace/results_controls_1129/`.
- #1135: `prima_driver.jl --rhobeg <r> --prefix <p>` (the same PRIMA call
  with only the initial trust radius changed; cells `dl_<p><n>`),
  `rhobeg_probe.jl` (which points BOBYQA starts from at each rhobeg, no
  Palace), `collect_1135.sh`; cells, logs and `provenance.txt` in
  `reference/fixtures/transmon_palace/results_controls_1135/`.
- #1145: T6 is a geode run, `BRACKET_PART=grounded_box` of
  `crates/geode-core/examples/transmon_c_sigma_bracket.rs`, writing
  `../transmon_diffopt/c_sigma_bracket_grounded_box.toml` (pinned by
  `tests/transmon_diffopt.rs`); T7 is `worker_amr.sh amr_final_p2:24`,
  `collect_1145.sh`, and `[amr.final]` of `results.toml`; cell and logs in
  `reference/fixtures/transmon_palace/results_controls_1145/`.

## 1. Gradient-free Newton controls at DeviceLayout's discretization

The setup is exactly #1111's: Palace on DeviceLayout's own mesh_order 2 mesh at
solver_order 2, with its configfile verbatim; targets [3.0, 4.0]; criterion
err = Σ (f − t)² / t² / 2 < 1e-4. Runs are counted as #1111 counts them: the
committed initial-geometry run is run 1.

The two controls:

- **Scaling-law Newton.** Newton in DeviceLayout's variables x1 = √(620/c),
  x2 = 5000/L. The Jacobian is
  - c column: ∂F_q/∂x1 = F_q/x1, from f_q ∝ 1/√C_Σ ∝ x1. This uses no
    derivative; it replaces geode's shape derivative.
  - L column: ∂F_r/∂x2 from #1111's transmission-line model. This is the same
    column the geode-gradient Newton uses, with δ = 215.954 µm from #1111's
    one-time calibration (a single geode finite difference, counted in
    neither method's runs).
  - zero off-diagonals.

  It is re-evaluated from the Palace values at every point. The two Newton
  variants differ only in the c column.
- **Broyden.** One forward finite-difference probe per variable
  (Δx = −0.05) from the initial design gives the full 2×2 Jacobian. Each
  later step applies Broyden's rank-one update.

| method | Palace runs to criterion | first-step err | design at criterion (µm) |
|---|---|---|---|
| PRIMA (DeviceLayout, published) | 9 | (unpublished) | 1217.678, 7243.955 |
| Newton, geode-gradient Jacobian (#1111) | 3 | 7.9e-4 | 1225.346, 7223.250 |
| **Newton, scaling-law c column** (same L column) | **3** | **1.7e-4** | 1232.664, 7221.270 |
| Broyden from two probes | 5 (1 + 2 probes + 2 steps) | 6.2e-4 | 1224.847, 7232.447 |

- **Result.** With a scaling law in place of the shape derivative in the c
  column, Newton meets the criterion in the same 3 runs as the geode-gradient
  Newton. Its first step is 4.7× better (err 1.7e-4 against 7.9e-4).
  - The reviewers' surrogate predicted about 4 runs and err 3.0e-4. The
    measured result is 3 runs and 1.7e-4.
  - Broyden needs 5. Its first step comes from a local tangent at the initial
    design, like the geode gradient, and lands similarly (6.2e-4). The
    scaling law captures more of the curvature over the long first step.
- **What this means for #1111's claim.** In DeviceLayout's variables the
  problem is close to linear. The 3-versus-9 is Newton's (with any reasonable
  Jacobian) against PRIMA's model-building, not the shape derivative. Here the
  geode gradient's contribution to the run count is nil.
- **Sensitivity to δ** (`[controls.delta_sensitivity]`, arithmetic only, not
  run). The Jacobian is diagonal, so δ moves only the L component of each
  step and never the c component the comparison is about. For the first
  step:
  - δ = 0 gives L = 7016 µm;
  - #1111's δ = 215.954 µm gives 7141 µm;
  - the Palace-only δ = 564 µm, from the Broyden probe `dl_bp2` by the same
    f_r (L + δ) = const model, gives 7351 µm.

  δ is weakly determined, as #1111 already noted. The fully Palace-only
  variant was run in #1129 (§5, T2): 3 runs.
- **Build check.** `dl_initial_rerun` repeats run 1 on this issue's separately
  built Palace image, on 16 ranks, from a regenerated, byte-identical mesh. It
  matches the committed #1092 cell to 6 decimals (relative differences
  4e-10 and 7.9e-8).

### One step from the published final design toward the converged targets

The scaling-law step uses F = the part-1 converged estimate at the published
final design (3.2652 / 4.0880 GHz). It lands at (1442.479, 7408.181) µm,
which is 3.6 µm / 8.2 µm short of #1111's second geode step (cv2). The Palace
half of #1111's ladder there gives:

| design | steps | converged estimate (GHz) | vs targets |
|---|---|---|---|
| #1111 cv1 (geode gradient) | 1 | 2.9672, 4.0044 | −1.09 %, +0.11 % |
| #1111 cv2 (geode gradient) | 2 | 3.0047, 3.9984 | +0.16 %, −0.04 % |
| **scaling law** | **1** | **3.0040 [2.977, 3.235], 4.0048 [3.994, 4.055]** | **+0.13 %, +0.12 %** |

So one gradient-free step does what two geode-gradient steps did. Both targets
lie inside the band.

## 2. Palace adaptive mesh refinement on the initial geometry

**Setup.**
- The mesh is DeviceLayout's initial h mesh (the ladder's h), at solver
  order 2, in the PEC box with the sapphire tensor.
- The eigen setup (AMS/GMRES) is the one the ladder's order-2 cells use.
- `Model.Refinement` is MaxIts 12 and MaxSize 1e7, with Dörfler 0.7,
  nonconformal refinement and Tol 1e-6 (never reached).
- With N = 2, the error indicator sums the qubit and resonator modes.

| solve | ND DOFs | f_q | f_r | error indicator |
|---|---|---|---|---|
| 1 (h) | 953,098 | 4.168319 | 5.616100 | 0.396 |
| 2 | 1,062,790 | 4.290428 | 5.629079 | 0.336 |
| 3 | 2,195,368 | 4.362363 | 5.660006 | 0.251 |
| 4 | 4,811,406 | 4.405666 | 5.680677 | 0.189 |
| 5 | 11,014,340 | **4.429678** | 5.698577 | 0.141 |

- **No plateau within MaxSize.** The last step still moved the qubit by
  +0.55 % and the resonator by +0.32 %. The run took 3.1 h on 32 ranks, with
  a 93 GB peak. One more iteration would be about 25M DOFs at order 2, and
  more than 5 h on this host. That was outside this issue's budget.
- **Uniform vs adaptive.** Solve 3 (2.2M DOFs) already exceeds the uniform
  order-2 h/2 level (4.3118 GHz at 7.0M DOFs).
- **Qubit.**
  - The finest level, 4.4297, lies 0.32 % below the part-1 estimate 4.4438.
    It is inside the band [4.415, 4.677] and 1.05 % below the cap.
  - The qubit increments shrink by a steady factor of about 0.6 per doubling
    of DOFs. A fit f = f∞ − C N^−α through the last three solves gives
    **4.4552** (α = 0.80), and 4.4570 from solves 2–4. That is +0.26 % from
    the estimate, inside the band, and below the cap (4.4766) and the shared
    limit (4.4802).
  - The fit is an estimate, not a bound. Like the shared limit, it is exactly
    determined and ill-conditioned: +0.1 % on the finest level moves it by
    24 MHz. So its 21 MHz margin below the cap is not evidence either way.
- **Resonator.** It converges more slowly under this marking. Its finest
  level, 5.6986, is still below the part-1 band [5.723, 5.817]. Its
  three-point fit (α = 0.25, 5.778) is not reliable.

## 3. Mesh-family spread

The h/2 qubit level comes from two h meshes of the initial geometry
(DeviceLayout's, and the earlier generator's fixture `transmon_smoke.msh`)
times two uniform refinements. The refinements are:
- **gmsh `-refine`**, which splits every tet into 8.
- **Palace `UniformLevels 1`**, which first bisects the h elements at
  under-resolved interior boundaries (the metal sheet), then refines.

All levels are Palace order 1:

| h mesh | gmsh refinement | Palace refinement |
|---|---|---|
| DeviceLayout (h: 3.452045) | 3.877116 | 3.946881 (new) |
| fixture (h: 3.452749) | 3.893868 | 3.952805 |

- **Where the ~2 % comes from.** The ~2 % of the #1111 caveat (1.95 %) is
  almost all **refinement scheme**: Palace's refinement is 1.80 % / 1.51 %
  above gmsh's on each mesh. The **mesh family** accounts for only
  0.43 % / 0.15 %, and the two h meshes agree to 0.02 %.
- **Effect on the estimate.** The part-1 rule, applied to the second
  refinement family of the same h mesh (Palace refinement: order 1 at h,
  h/2, h/4; order 2 at h, h/2), gives the following:

  | family | estimate | band |
  |---|---|---|
  | a: gmsh refinement (#1092 ladder) | 4.4438 | [4.415, 4.677] |
  | b: Palace refinement (new) | 4.4720 | [4.458, 4.510] |

  The estimates are 0.64 % apart (resonator 0.62 %), and b's band lies inside
  a's. Family b's order-1 ladder runs at observed order 0.91, close to the
  asymptotic 1, against a's 0.62.
- **Proposal for v12.**
  - Keep the part-1 band, which already covers the second family.
  - Quote the converged initial qubit as the range of the two family
    estimates, **4.444–4.472 GHz**, and state the 0.64 % as the
    mesh-family / refinement-scheme spread.
  - The adaptive extrapolation, 4.455, lies inside that range. It is an
    ill-conditioned estimate (±24 MHz per 0.1 %).
  - Widening the band by the 1.8 % level spread would double-count the
    extrapolation.

## 4. The cap's tier, and the shared limit

- **The cap.** It is 1/(2π √(L_J (C_Σ,lower + C_J))) = **4.4766 GHz**, from:
  - L_J = 14.860 nH and C_J = 5.5 fF, the junction element of DeviceLayout's
    configfile;
  - C_Σ,lower = 79.56 fF, the certified sapphire-tensor bracket
    [79.56, 79.76] fF of `c_sigma_bracket_tensor.toml`. This is the island
    capacitance with the feedline floating and the resonator grounded,
    certified on `transmon_smoke.msh`;
  - L_s = 0 and k = 0. The #1079 L_s band [0.113, 0.125] nH and the #1078 k
    band [0.0227, 0.0310] only lower the qubit. With those bands, the
    static-route estimate is [4.449, 4.458].
- **Tier: model-dependent, not certified.** It is rigorous only given:
  - the lumped two-node circuit (one island node, L_J ∥ C_J, L_s ≥ 0, and
    coupling only to modes above the qubit);
  - a quasi-static C_Σ at 4.4 GHz;
  - the fixture geometry (agreeing to 0.02 % at h);
  - an f64 certificate.
- **The shared limit.** The order-1 (h/2, h/4) and order-2 (h, h/2) series
  fitted with one common limit and order:

  | series | order | limit |
  |---|---|---|
  | initial qubit (a) | 0.890 | 4.4802 |
  | initial resonator | 0.813 | 5.7758 |
  | final qubit / resonator | 0.876 / 0.855 | 3.3022 / 4.1071 |
  | cv2 qubit / resonator | 0.872 / 0.844 | 3.0389 / 4.0192 |
  | conv-step qubit / resonator | 0.859 / 0.855 | 3.0418 / 4.0245 |
  | initial qubit, family b | 1.023 | 4.4657 |
- **Why 4.4802 exceeds the cap (by 3.6 MHz, 0.08 %).**
  - The fit is exactly determined (four levels, four unknowns) and
    ill-conditioned. +0.1 % on the order-2 h/2 level alone moves it by
    20 MHz, so the excess equals a 0.019 % change of that one level (0.0186 %
    exact; 0.0179 % by the linear estimate). That is
    far inside the 0.15–1.8 % mesh and refinement-scheme effects of §3.
  - It also forces one order on two pre-asymptotic series.
  - On family b, which is close to its asymptotic order, the same
    construction gives 4.4657, below the cap.
  - So do both family estimates and the adaptive run's finest computed level
    (4.4297).
  - The adaptive run's extrapolation (4.4552) is left out of this list,
    because it is as ill-conditioned as the shared limit (24 MHz per 0.1 %).
  - So the excess lies within the conditioning of the family-a fit and is
    not evidence against the cap. The cap stays model-dependent.

## 5. #1129: PRIMA in this environment, Palace-only Newton, second family on cv2

All three run on the same Palace image as §1–4 (palace:cpu-1043, loaded from
#1123's saved build) on one us-east-2 worker. The T1/T2 runs used 16 ranks
and the T3 cells the other 16, concurrently.

### T1. PRIMA re-run in this Palace environment

"Environment" here means the Palace build, the mesher and the platform
together: the published run differs from ours in more than the Palace build,
since the mesh generator and host differ too.

`prima_driver.jl` repeats DeviceLayout's `run_optimization` call verbatim:
`prima(errfunc, [1.0, 1.0]; ftarget = 1e-4, xl = [0.6, 0.6], xu = [1.4, 1.4],
rhobeg = 0.2)`. With bounds only, PRIMA.jl 0.2.4 (PRIMA_jll 0.7.1, the
`reference/julia/devicelayout` pin) runs BOBYQA. The objective is the same
err, with cap_length = 620 µm / x1² and total_length = 5000 µm / x2 passed
exactly. Every point is a Palace run at DeviceLayout's discretization, the
same cells as the Newton controls, and run 1 is the committed `dl_initial`.

| run | x1, x2 | c, L (µm) | f_q, f_r | err |
|---|---|---|---|---|
| 1 | 1, 1 | 620.000, 5000.000 | 4.1683, 5.6132 | 1.6e-1 |
| 2 | 1.2, 1 | 430.556, 5000.000 | 4.9392, 5.6114 | 2.9e-1 |
| 3 | 1, 1.2 | 620.000, 4166.667 | 4.1753, 6.6065 | 2.9e-1 |
| 4 | 0.8, 1 | 968.750, 5000.000 | 3.3747, 5.6162 | 8.9e-2 |
| 5 | 1, 0.8 | 620.000, 6250.000 | 4.1601, 4.5838 | 8.5e-2 |
| 6 | 0.8153, 0.7233 | 932.759, 6912.680 | 3.4447, 4.1710 | 1.2e-2 |
| 7 | 0.7024, 0.6751 | 1256.543, 7405.993 | 2.9904, 3.9094 | 2.6e-4 |
| 8 | 0.6967, 0.6560 | 1277.379, 7622.252 | 2.9367, 3.8019 | 1.4e-3 |
| 9 | 0.6972, 0.6944 | 1275.486, 7200.135 | 2.9559, 4.0147 | 1.15e-4 |
| **10** | 0.7161, 0.6937 | **1209.125, 7207.551** | 3.0317, 4.0179 | **6.6e-5** |

- **Result: 10 Palace runs in this environment, against the published 9.** PRIMA
  stops at run 10 with `FTARGET_ACHIEVED` (info.nf = 10). Runs 1–5 are
  BOBYQA's initial interpolation set (x0, x0 ± 0.2 along each axis). Run 9
  misses the criterion by 15 %.
- In this environment the published final design already meets the criterion
  (`dl_final`: 3.0164 / 3.9956 GHz, err 1.5e-5). So the environment difference
  moves PRIMA's trajectory, not the achievability of the target. PRIMA lands
  at (1209.125, 7207.551) µm, 8.6 / 36.4 µm short of the published design.
- **What this means for the comparison.** In one Palace environment the count is
  3 (each Newton variant) against 10 (PRIMA), not 3 against 9. The published
  9 came from a different environment. The one-run difference is the
  trajectory's sensitivity to the environment, so neither count is a property of
  PRIMA alone.
- **Reproducibility.** `prima_driver.jl --replay
  reference/fixtures/transmon_palace/results_controls_1129` re-runs PRIMA on
  the committed cells without Palace. It reproduces `logs/prima_log.jsonl`
  line for line. The pins check every logged x against its cell's generator
  report (bit for bit) and every logged frequency against its cell.

### T2. Scaling-law Newton with the Palace-only δ

This is the §1 scaling-law Newton with δ = 563.879 µm, from `dl_initial` and
the Broyden probe `dl_bp2` by f_r (L + δ) = const, in place of #1111's
geode-calibrated 215.954 µm. No geode input of any kind enters
(`driver.py scaling-palace`).

| run | c, L (µm) | f_q, f_r | err |
|---|---|---|---|
| 1 | 620.000, 5000.000 | 4.1683, 5.6132 | 1.6e-1 |
| 2 | 1196.933, 7350.807 | 3.0400, 3.9367 | 2.1e-4 |
| **3** | **1229.026, 7225.712** | 3.0087, 4.0014 | **4.3e-6** |

- **Result: 3 runs, the same as with #1111's δ.** The first step lands where
  §1's δ-sensitivity predicted (7350.807 µm). Its err is 2.1e-4, against
  1.7e-4 with #1111's δ and 7.9e-4 with the geode gradient. So the larger δ
  overshoots L slightly (f_r 3.937 GHz), and the second step corrects it.
- So the 3-run Newton count no longer depends on any geode input.

### T3. Second refinement family on the step-2 design cv2

The "+19 % capacitor" claim rests on #1111's cv2 ladder (family a: gmsh
`-refine` of the DeviceLayout h mesh). Family b refines the same h mesh,
regenerated byte-identically (sha256 0558e1e6…), with Palace's
`UniformLevels` 1 and 2 instead, exactly as §3 did on the initial geometry.
The h cells are shared. Both families go through the part-1 rule:

| | f_q levels: p1 h, h/2, h/4; p2 h, h/2 | qubit estimate [band] | resonator estimate [band] |
|---|---|---|---|
| a: gmsh refinement (#1111) | 2.2387, 2.5613, 2.7780; 2.7784, 2.8966 | 3.0047 [2.979, 3.221] | 3.9984 [3.987, 4.067] |
| b: Palace refinement (new) | 2.2387, 2.5984, 2.8082; 2.7784, 2.9005 | **3.0203 [3.003, 3.101]** | 4.0225 [4.016, 4.043] |

- **Spread.** Family b sits 0.52 % (qubit) and 0.60 % (resonator) above
  family a, the same size as on the initial geometry (0.64 % / 0.62 %).
  Its band lies inside a's. Its order-1 ladder runs at observed order 0.78,
  against a's 0.57, and its shared limit is 3.029 GHz (order 0.96).
  Palace refinement puts the order-1 h/2 level 1.45 % above gmsh's.
- **cv2 against the targets.** Both targets lie just below family b's band
  (qubit +0.68 %, resonator +0.56 % over target). So on family b cv2 is
  slightly short of the converged design, not on it.
- **What it means for the capacitor.** f_q ∝ √(620/c) maps each estimate to
  the c that would put it on 3.0 GHz (df_q/dL = 0; an estimate, not a run):

  | | c on target (µm) | over the published 1217.678 µm |
  |---|---|---|
  | family a estimate | 1450.7 | +19.1 % |
  | family b estimate | 1465.7 | +20.4 % |
  | family b band | [1449, 1546] | [+19.0 %, +26.9 %] |
  | union of both bands | [1426, 1667] | [+17.1 %, +36.9 %] |
  | cv2 itself | 1446.078 | +18.8 % |

- **Result.** The second family confirms the claim and moves it slightly up.
  Quote the converged capacitor as **+19 % to +20 %** (the two family
  estimates, 1451–1466 µm). The +17 % to +37 % band of the two families
  combined is the honest uncertainty. By f_r (L + δ) = const, family b's
  resonator estimate asks for about 0.6 % (about 45 µm) more resonator length
  as well. The cells took 27 + 28 + 120 min on 16 ranks. `famcv2_u1_p2` ran
  for 7200 s (`time.txt`: 2:00:00; 7201 s between its start and end lines in
  `logs/orchestrate_fam.log`) and exited cleanly: exit status 0, its full
  `eig.csv` and Palace's closing timing summary are present, and no worker
  script sets a time limit, so the round number did not come from a cut-off.

## 6. #1135: second family on the final design, PRIMA trust-radius sensitivity

Both run on the same Palace image as §1–5 (palace:cpu-1043, loaded from
#1123's saved build) on one us-east-1 worker. The T4 cells used 16 ranks and
the two T5 runs 8 ranks each, all concurrently.

### T4. Second refinement family on the published final design

The "+8.7 %" for the published final design (1217.678 / 7243.955 µm) is the
#1092 part-1 estimate, 3.2652 GHz against the published 3.005 GHz. It rests on
one refinement family (family a: gmsh `-refine` of the DeviceLayout h mesh).
Family b refines the same h mesh, regenerated byte-identically on the Mac
(gmsh 4.15.2, sha256 2a7158b0…, the #1092 `final_o1_v22.msh`), with Palace's
`UniformLevels` 1 and 2, exactly as §3 did on the initial geometry and §5 T3
on cv2. The h cells are shared, and both families go through the part-1 rule:

| | f_q levels: p1 h, h/2, h/4; p2 h, h/2 | qubit estimate [band] | resonator estimate [band] |
|---|---|---|---|
| a: gmsh refinement (#1092) | 2.3927, 2.7659, 3.0099; 3.0164, 3.1464 | 3.2652 [3.236, 3.471] | 4.0880 [4.077, 4.139] |
| b: Palace refinement (new) | 2.3927, 2.8278, 3.0611; 3.0164, 3.1534 | **3.2924 [3.278, 3.331]** | 4.1126 [4.106, 4.125] |

- **Spread.** Family b sits 0.83 % (qubit) and 0.60 % (resonator) above
  family a. On the qubit that is wider than on the initial geometry
  (0.64 %) and on cv2 (0.52 %); the resonator spread is about the same
  (0.62 %, 0.60 %). Palace refinement puts the order-1 h/2 level 2.24 %
  above gmsh's (1.80 % initial, 1.45 % cv2).
- **Band.** Family b's band lies inside a's, so the union of the two bands
  is a's band, [3.236, 3.471] GHz. Family b's order-1 ladder runs at
  observed order 0.90 (a: 0.61), and its shared limit is 3.2848 GHz at
  order 1.03 (a: 3.3022 at 0.88).
- **Against the published value.**

  | | qubit over published 3.005 GHz | resonator over published 3.982 GHz |
  |---|---|---|
  | family a estimate | +8.66 % | +2.66 % |
  | family b estimate | +9.56 % | +3.28 % |
  | family b band | [+9.08 %, +10.84 %] | |
  | union of both bands | [+7.67 %, +15.51 %] | [+2.40 %, +3.94 %] |

  Both published values lie below both bands. Palace at DeviceLayout's own
  discretization (3.0164 GHz, `dl_final`) is 8.4 % below the family-b
  estimate.
- **Result.** The second family confirms the claim and moves it up. Quote
  the converged final qubit as the range of the two family estimates,
  **3.265–3.292 GHz, +8.7 % to +9.6 % over the published 3.005 GHz**, with
  the 0.83 % as the mesh-family / refinement-scheme spread, and keep
  family a's band [3.236, 3.471] (+7.7 % to +15.5 %) as the uncertainty.
  Both numbers are extrapolation estimates, not converged values.
- **Cost.** `famfinal_u1_p1` 26 min, `famfinal_u2_p1` 28 min (13.1M DOFs),
  `famfinal_u1_p2` 2.4 h (9.0M DOFs), all on 16 ranks with the T5 runs on
  the other 16. All three exited with status 0.

### T5. PRIMA's initial trust radius

`prima_driver.jl --rhobeg` repeats §5 T1's call with only `rhobeg` changed
(same objective, bounds [0.6, 1.4]², ftarget 1e-4, Palace environment, and
run 1 = the committed `dl_initial`).

- **rhobeg 0.4 cannot be run on this box.** BOBYQA needs
  0 < rhobeg ≤ min(xu − xl)/2. In binary64, (1.4 − 0.6)/2 is
  0.39999999999999997, one ulp below 0.4. So PRIMA rejects 0.4 ("Invalid
  RHOBEG … it is set to MINVAL(XU-XL)/4") and runs with 0.19999999999999998,
  which is DeviceLayout's 0.2 to within 3e-17. A first attempt at 0.4 was
  stopped after one Palace run, whose point was #1129's run 2 (x = [1.2, 1]).
  `rhobeg_probe.jl` shows this without Palace
  (`logs/rhobeg_probe.txt`). The large-radius run therefore uses
  0.39999999999999997, the largest value BOBYQA accepts, whose initial
  points sit on the box edges (x = 0.6 and 1.4).

| rhobeg | Palace runs to criterion | final err | design at criterion (µm) |
|---|---|---|---|
| 0.1 (new) | **10** | 2.1e-6 | 1235.705, 7216.766 |
| 0.2 (DeviceLayout's; §5 T1) | 10 | 6.6e-5 | 1209.125, 7207.551 |
| 0.39999999999999997 (new) | **12** | 3.7e-5 | 1245.900, 7291.401 |

- **Result: 10, 10 and 12 runs.** All three stop with `FTARGET_ACHIEVED`.
  Runs 1–5 of each are BOBYQA's initial set, x0 and x0 ± rhobeg along each
  axis. The largest radius spends its initial runs at the box edges
  (c = 316 µm and 1722 µm; L = 3571 µm and 8333 µm), far from the solution,
  and takes two runs more.
  - rhobeg 0.1 first gets below 1e-3 at run 8 (9.1e-4), then overshoots on
    the resonator at run 9 (3.4e-3) before meeting the criterion at run 10.
  - The large radius gets below 1e-3 at run 10 (3.9e-4) and misses by 27 %
    at run 11 (1.27e-4).
- **What this means for the comparison.** Over a 4× range of rhobeg the
  PRIMA count in this environment is 10–12, against the published 9 (a
  different environment) and 3 for every Newton variant. So the 3-versus-9
  of the v13 text does not hinge on DeviceLayout's choice of 0.2: none of
  the three radii gets below 10. This is three runs of one optimizer on one
  problem, not a distribution.
- **Reproducibility.** `prima_driver.jl --replay
  reference/fixtures/transmon_palace/results_controls_1135 --rhobeg <r>
  --prefix <pa | pb>` re-runs PRIMA on the committed cells without Palace.
  Both reproduce their `logs/prima_rhobeg*_log.jsonl` line for line, and
  the default (`--rhobeg 0.2 --prefix pr` on `results_controls_1129`) still
  reproduces #1129's log.
- **Cost.** 8 ranks per run: 14–21 min of Palace per run (about 16 min on
  average), plus 0.5–12 min of mesh generation on a loaded Mac.

## 7. #1145: grounded-box upper bound, adaptive refinement on the final design

The v16 reviews named two optional runs. T6 is a geode electrostatic solve
(its artifact is `../transmon_diffopt/c_sigma_bracket_grounded_box.toml`);
T7 is a Palace run (`[amr.final]` of `results.toml`).

### T6. Grounded-box upper bound on the initial-geometry C_Σ

The static route's bracket [79.56, 79.76] fF (`c_sigma_bracket_tensor.toml
[bracket_x0_tensor]`, #1091) and its dressed range [4.449, 4.458] GHz belong to
the natural-box model: ground is the ground-plane sheet only and the outer box
carries the natural (zero normal D) condition. The eigenproblem's Palace config
grounds the box (`PEC = metal + exterior_boundary`). Grounding only adds
constraints to the energy minimization, so the natural-box lower bound also
bounds the grounded-box C_Σ from below. The upper bound does not transfer.

- **Setup.** `BRACKET_PART=grounded_box` re-solves X⁰'s sapphire-tensor P2
  value on the same meshes as #1091 (uniform h, h/2 and the eight held-out
  rounds of `anchor_93p95`). Each natural-box value is asserted against the
  committed #1091 row of the same mesh. It then solves the same mesh again with
  every `exterior_boundary` face (vertex and edge DOFs) at 0 V. On every mesh
  the group is asserted to be the whole topological boundary, and the island
  and the feedline are asserted not to touch it. No lower bound is recomputed.
- **Result.**

  | mesh | tets | natural (fF) | grounded box (fF) | difference (fF) |
  |---|---|---|---|---|
  | uniform h | 133,314 | 91.717846 | 91.725009 | +0.007163 |
  | uniform h/2 | 1,066,512 | 85.008425 | 85.015234 | +0.006809 |
  | round 0 | 448,279 | 80.516226 | 80.522745 | +0.006519 |
  | round 7 | 3,388,197 | 79.758230 | 79.764730 | +0.006500 |

  Rounds 1–6 are in the TOML; the difference is +0.00650 to +0.00651 fF on
  every adaptive round. The grounded-box C_Σ is bounded by **[79.56, 79.77]
  fF** (the upper end is round 7, ceiled). The upper end moves by 0.01 fF at the
  two-decimal rounding. The same-mesh difference at round 7, **+0.0065 fF
  (8.1e-5 relative)**, is an estimate of the converged difference, not a bound.
  The converged difference is bounded by [0, 0.21] fF.
- **Dressed qubit.** The #1078/#1079 circuit over the same corners (L_s, k
  bands, resonator 5.7404 GHz) gives **[4.4492, 4.4579] GHz** for the grounded
  box, against [4.4494, 4.4579] for the natural box. The low end drops by 0.3
  MHz; the high end is set by the shared lower bound and does not move. The
  circuit terms are estimates, so this range is an estimate under the circuit
  model, not a bound on the eigenfrequency. The natural-box band is recomputed
  and asserted equal to the committed crosscheck band.
- **A role-identification bug, found by this run.** The first attempt stopped at
  round 7 with "island touches the outer box"
  (`../transmon_diffopt/c_sigma_bracket_grounded_box_attempt1.log`). On that
  mesh the adaptively refined island sheet has more triangles than the ground
  plane (42,129 against 41,422), and `TransmonFixture::split_metal_conductors`
  called the junction-touching component with the most triangles "ground". It
  now picks the one with the largest in-plane bounding box (the ground plane
  spans the chip), with triangle count as the tie-break. The swap affected only
  meshes this fine, and there the natural-box C_Σ (island to ground, feedline
  floating, no other conductor) is symmetric in the two terminals. So every
  committed natural-box value stands, and the round-7 natural replay
  reproduces its committed value. The grounded-box value is not symmetric,
  which is why this run caught it.
- **Cost.** 44 min wall and 17.9 GB peak RSS on 8 threads of the T7 worker
  (the first attempt took 36 min more before it stopped).

### T7. Palace adaptive refinement on the published final design

**Setup.** The same settings as §2, on the final design's h mesh
(`final_o1_v22.msh`, the #1092 ladder's h, sha256 2a7158b0...): the config is
`final_h_p2`'s with exactly the deltas #1123 applied to `initial_h_p2`, so it
differs from `amr_initial_p2` only in the mesh, the output path and the eigen
target (2.0 GHz, as `final_h_p2`). The pin checks this. 24 ranks, against
#1123's 32.

| solve | ND DOFs | f_q | f_r | error indicator |
|---|---|---|---|---|
| 1 (h) | 1,167,212 | 3.016362 | 3.997164 | 0.409 |
| 2 | 1,315,054 | 3.128649 | 4.000737 | 0.348 |
| 3 | 2,854,692 | 3.188362 | 4.030701 | 0.264 |
| 4 | 6,324,942 | 3.226379 | 4.043153 | 0.200 |
| 5 | 14,622,036 | **3.246796** | 4.055203 | 0.150 |

- **Finest level, no extrapolation: f_q = 3.2468 GHz**, +8.05 % over the
  published 3.005 GHz and +7.64 % over Palace at DeviceLayout's discretization
  (3.0164). The resonator's finest level, 4.0552 GHz, is +1.84 % over the
  published 3.982.
- **No plateau within MaxSize.** The last step still moved the qubit by
  +0.63 % and the resonator by +0.30 %. As in §2, the run stopped because the
  last solve passed 1e7 DOFs, not on a tolerance. Every level rises.
- **Against the §6 T4 family estimates.** The finest qubit level lies 0.56 %
  below family a's estimate (3.2652) and 1.39 % below family b's (3.2924). It
  is inside the union band [3.236, 3.471]. The finest resonator level lies
  below its union band [4.077, 4.139], as the initial geometry's did in §2.
  Since the qubit is still rising, these are finite-mesh levels approaching
  from below, not a converged value.
- **Fit (estimate).** f = f∞ − C N^−α through the last three solves gives
  **3.2672 GHz** (α = 0.83), between the two family estimates. It is exactly
  determined and ill-conditioned: +0.1 % on the finest level moves it by
  16 MHz. The resonator fit has α = 0.10 and is not reliable.
- **Cost.** 5.8 h wall on 24 ranks (the other 8 vCPU ran T6 and the gates for
  the first ~2 h), with a 121.9 GiB host peak in the last solve.

## Compute

AWS `geode-fem-bench`, us-east-1, tagged `project=geode-fem`, 2026-10-14.
All workers are terminated, and every output was synced back before
termination.

| worker | role | time | cost |
|---|---|---|---|
| amr1123 (r8g.8xlarge) | the adaptive run, fam_u1_p1, fam_u2_p1 | about 4.3 h | about $8.1 |
| dl1123 (r8g.4xlarge) | the controls (8–14 min per Palace run on 16 ranks, including upload), the rerun, fam_u1_p2 | about 4.5 h | about $4.2 |
| lad1123 (r8g.4xlarge) | the conv-step ladder | about 3.4 h | about $3.2 |
| gate1123 (r8g.4xlarge) | gates | about 1.5 h | about $1.4 |

The total is about $17. On 32 ranks, #1111's DeviceLayout-discretization runs took about 4 min each.

#1129 ran on one worker in **us-east-2** (`AWS_REGION=us-east-2`, same
profile and tags), 2026-10-15. It was terminated after every output was synced back.

| worker | role | time | cost |
|---|---|---|---|
| w1129 (r8g.8xlarge) | T1 (9 PRIMA runs) and T2 (2 runs) on 16 ranks, 8–10 min per run including mesh upload; T3 (three cells) on the other 16 ranks; gates | 3.3 h | about $6.2 |

#1135 ran on one worker in us-east-1 (same profile and tags), 2026-10-15.
It was terminated after every output was synced back.

| worker | role | time | cost |
|---|---|---|---|
| w1135 (r8g.8xlarge) | T4 (three cells) on 16 ranks; T5 (two PRIMA runs, 9 + 11 Palace runs) on 8 ranks each, concurrently; gates | 4.1 h | about $7.7 |

#1145 ran on one worker in us-east-2 (same profile and tags), 2026-10-15/16.
The idle alarm stopped it after T7 ended, it was restarted to sync the outputs
and run the final gate, and it was terminated after everything was synced back.

| worker | role | time | cost |
|---|---|---|---|
| w1145 (r8g.8xlarge) | T7 on 24 ranks; T6 (two attempts and a diagnostic replay) and gates on the other 8 vCPU | WALL_PLACEHOLDER | COST_PLACEHOLDER |
