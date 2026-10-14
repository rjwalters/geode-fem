# `benchmarks/transmon_devicelayout_controls/` — controls for the DeviceLayout.jl SingleTransmon study (#1123)

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

Everything here is Palace (fba6a5b, the #1092 Dockerfile) and arithmetic.
No geode solve enters any number. All values are in GHz unless stated.

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

## 1. Gradient-free Newton controls at DeviceLayout's discretization

The setup is exactly #1111's: Palace on DeviceLayout's own mesh_order 2 mesh at
solver_order 2, with its configfile verbatim; targets [3.0, 4.0]; criterion
err = Σ (f − t)² / t² / 2 < 1e-4. Runs are counted as #1111 counts them: the
committed initial-geometry run is run 1.

The two controls:

- **Scaling-law Newton.** Newton in DeviceLayout's variables x1 = √(620/c),
  x2 = 5000/L. The Jacobian is
  - ∂F_q/∂x1 = F_q/x1, from f_q ∝ 1/√C_Σ ∝ x1;
  - ∂F_r/∂x2 from #1111's transmission-line model (δ = 216 µm);
  - zero off-diagonals.

  It is re-evaluated from the Palace values at every point, and no
  field-solve derivative is used.
- **Broyden.** One forward finite-difference probe per variable
  (Δx = −0.05) from the initial design gives the full 2×2 Jacobian. Each
  later step applies Broyden's rank-one update.

| method | Palace runs to criterion | first-step err | design at criterion (µm) |
|---|---|---|---|
| PRIMA (DeviceLayout, published) | 9 | (unpublished) | 1217.678, 7243.955 |
| Newton, geode-gradient Jacobian (#1111) | 3 | 7.9e-4 | 1225.346, 7223.250 |
| **Newton, scaling-law Jacobian** | **3** | **1.7e-4** | 1232.664, 7221.270 |
| Broyden from two probes | 5 (1 + 2 probes + 2 steps) | 6.2e-4 | 1224.847, 7232.447 |

- **Result.** The scaling-law Jacobian with no derivative at all meets the
  criterion in the same 3 runs as the geode-gradient Newton, and its first
  step is 4.7× better (err 1.7e-4 against 7.9e-4).
  - The reviewers' surrogate predicted about 4 runs and err 3.0e-4. The
    measured result is 3 runs and 1.7e-4.
  - Broyden needs 5. Its first step comes from a local tangent at the initial
    design, like the geode gradient, and lands similarly (6.2e-4). The
    scaling law captures more of the curvature over the long first step.
- **What this means for #1111's claim.** In DeviceLayout's variables the
  problem is close to linear. The 3-versus-9 is Newton's (with any reasonable
  Jacobian) against PRIMA's model-building, not the shape derivative. Here the
  geode gradient's contribution to the run count is nil.
- **Build check.** `dl_initial_rerun` repeats run 1 on this issue's separately
  built Palace image, on 16 ranks, from a regenerated, byte-identical mesh. It
  matches the committed #1092 cell to every printed digit.

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
  - The adaptive extrapolation, 4.455, lies inside that range.
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
    20 MHz, so the excess equals a 0.018 % change of that one level. That is
    far inside the 0.15–1.8 % mesh and refinement-scheme effects of §3.
  - It also forces one order on two pre-asymptotic series.
  - On family b, which is close to its asymptotic order, the same
    construction gives 4.4657, below the cap.
  - So does every independent computation: the adaptive run's finest level
    (4.4297) and extrapolation (4.4552), and both family estimates.
  - So the excess lies within the conditioning of the family-a fit and is
    not evidence against the cap. The cap stays model-dependent.

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
