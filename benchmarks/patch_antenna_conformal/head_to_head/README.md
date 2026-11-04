# 3-way head-to-head: GEODE freeform vs low-DOF parametric vs FDTD-density (#651)

This directory assembles the epic #647 comparison **from committed artifacts only**. It runs no solver and spends nothing. `build_head_to_head.py` reads the artifacts, writes `head_to_head.json` and regenerates the tables below. `verify_head_to_head.py` recomputes everything and fails on drift.

```bash
python3 build_head_to_head.py          # regenerate JSON + tables
python3 verify_head_to_head.py         # pin JSON/tables to the sources
```

## How to read this

- **Every 2.4 GHz number is conditional on its mesh level** and uses UPML σ₀ = 1. `../nested_dof/README.md` shows that the optimizer fits discretization error: designs lose up to 8 dB when carried to a finer fixture (see the transfer table). Rows from different levels are never ranked against each other.
- **Method labels follow the #651 scope**: `gap_*` and `outline_16` are "parametric", `combined_64*` is "freeform". The `gap_16`/`gap_48` rows are spatially varying B-spline fields, not "a few scalars"; only `gap_1` and `gap_4` are few-parameter designs. This is *the* caveat on the parametric-vs-freeform gap.
- **The gap is measured, not assumed.** The data do not show that low-DOF cannot reach the freeform result: a 16-DOF spatially varying design reaches −10 dB on L1. The one-parameter and 4-parameter designs do not (they stay above −2.5 dB).
- **Radial DOFs thicken the substrate** (non-uniform-thickness ablation, not a 1.6 mm board). All optimized rows are idealized ablations.
- **Warm-started rows are not like-for-like** with cold rows (they carry the L1 bias).
- **The `smoke_73dof` row is a different problem** (16 × 12 mm patch at ~15 GHz). It is listed because it is the committed Phase 3 artifact (#650), not because it can be ranked against the 2.4 GHz rows.
- **FDTD-density has no |S11| at all**: the definitive 3-D Meep-adjoint run is operator-only (conda env plus AWS compute beyond one box's RAM) and was not performed. The FDTD column carries only the geometric staircasing floor and historical Meep timing on an unmatched flat scaffold. No claim of superiority over FDTD-density is made here.
- Cost cells say `not measured` where no artifact records one.

<!-- BEGIN GENERATED TABLE -->
### Optimized designs, own level (worst of 9 in-band points, dB)

| method | design | DOFs | level | order | σ₀ | start | initial | final | −10 dB? | wall | peak RSS (GiB) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| parametric | gap_1 | 1 | L0 | p1 | 1 | cold | -0.90 | -1.68 | no | 0.2 h | 0.8 |
| parametric | gap_4 | 4 | L0 | p1 | 1 | cold | -0.90 | -1.30 | no | 0.2 h | 0.8 |
| parametric | gap_16 | 16 | L0 | p1 | 1 | cold | -0.90 | -9.33 | no | 0.3 h | 0.8 |
| parametric | outline_16 | 16 | L0 | p1 | 1 | cold | -0.90 | -9.63 | no | 0.4 h | 0.8 |
| parametric | gap_48 | 48 | L0 | p1 | 1 | cold | -0.90 | -11.74 | yes | 0.3 h | 0.8 |
| freeform | combined_64 | 64 | L0 | p1 | 1 | cold | -0.90 | -13.78 | yes | 0.3 h | 0.8 |
| parametric | gap_1 | 1 | L1 | p1 | 1 | cold | -1.47 | -1.99 | no | 1.3 h | 5.7 |
| parametric | gap_4 | 4 | L1 | p1 | 1 | cold | -1.47 | -1.93 | no | 1.4 h | 5.7 |
| parametric | gap_16 | 16 | L1 | p1 | 1 | cold | -1.47 | -11.29 | yes | 1.9 h | 5.7 |
| parametric | outline_16 | 16 | L1 | p1 | 1 | cold | -1.47 | -9.98 | no | 1.7 h | 5.7 |
| parametric | gap_48 | 48 | L1 | p1 | 1 | cold | -1.47 | -13.15 | yes | 2.7 h | 5.7 |
| freeform | combined_64 | 64 | L1 | p1 | 1 | cold | -1.47 | -14.12 | yes | 2.7 h | 5.7 |
| parametric | gap_48_warm | 48 | L2 | p1 | 1 | warm | -9.67 | -13.29 | yes | 4.0 h | 19.1 |
| freeform | combined_64_cold | 64 | L2 | p1 | 1 | cold | -1.75 | -14.07 | yes | 4.6 h | 19.3 |
| freeform | combined_64_warm | 64 | L2 | p1 | 1 | warm | -9.85 | -15.87 | yes | 3.6 h | 19.2 |
| freeform | **smoke_73dof** (smoke fixture, ~15 GHz, different problem) | 73 | curved_smoke | p1 | 1 | cold | -5.51 | -12.06 | yes | not measured | not measured |

### Parametric-vs-freeform gap, per level (dB; positive = freeform better)

| level | freeform | its worst | best parametric | DOFs | its worst | gap | fewest-DOF parametric reaching −10 dB | note |
|---|---|---|---|---|---|---|---|---|
| L0 | combined_64 | -13.78 | gap_48 | 48 | -11.74 | 2.04 | gap_48 (48 DOFs, -11.74 dB) |  |
| L1 | combined_64 | -14.12 | gap_48 | 48 | -13.15 | 0.96 | gap_16 (16 DOFs, -11.29 dB) |  |
| L2 | combined_64_cold | -14.07 | gap_48_warm | 48 | -13.29 | 0.78 | gap_48_warm (48 DOFs, -13.29 dB) | warm-started rows are not like-for-like with cold rows |
| L2 | combined_64_warm | -15.87 | gap_48_warm | 48 | -13.29 | 2.58 | gap_48_warm (48 DOFs, -13.29 dB) | warm-started rows are not like-for-like with cold rows |

### Cross-level transfer (design carried to another fixture, worst of 9 in-band points, dB)

| design | DOFs | evaluated on | order | σ₀ | worst | wall | peak RSS (GiB) |
|---|---|---|---|---|---|---|---|
| L1_combined_64 | 64 | L0 | p1 | 1 | -6.04 | 0.0 h | 0.9 |
| L1_combined_64 | 64 | L1 | p1 | 1 | -14.12 | 0.1 h | 5.8 |
| L1_combined_64 | 64 | L1 | p2 | 1 | -5.69 | 1.3 h | 132.1 |
| L1_combined_64 | 64 | L2 | p1 | 1 | -9.85 | 0.2 h | 19.3 |
| L1_combined_64 | 64 | L3 | p1 | 1 | -7.57 | 1.1 h | 68.9 |
| L1_gap_48 | 48 | L1 | p1 | 1 | -13.15 | 0.1 h | 5.8 |
| L2_combined_64_cold | 64 | L1 | p2 | 1 | -9.16 | 2.7 h | 132.2 |
| L2_combined_64_cold | 64 | L3 | p1 | 1 | -12.47 | 1.7 h | 70.2 |
| L2_combined_64_warm | 64 | L1 | p2 | 1 | -8.26 | 1.2 h | 132.3 |
| L2_combined_64_warm | 64 | L3 | p1 | 1 | -11.48 | 1.7 h | 69.0 |
| initial | 1 | L1 | p1 | 1 | -1.47 | 0.0 h | 5.7 |
| initial | 1 | L1 | p2 | 1 | -2.42 | 1.3 h | 132.2 |
| initial | 1 | L2 | p1 | 1 | -1.75 | 0.2 h | 19.1 |
| initial | 1 | L3 | p1 | 1 | -2.04 | 1.1 h | 68.8 |

### FDTD-density (no |S11| measured)

| method | DOFs | level / σ₀ | worst-of-band \|S₁₁\| | cost |
|---|---|---|---|---|
| FDTD-density | not applicable (no density run performed) | n/a | not run — operator-gated | historical Meep timing on an unmatched flat scaffold, R=12 px/mm: 5.76 s/step, 37.4 GB RSS (AWS m6i.4xlarge (16 vCPU Intel Ice Lake, 61 GB RAM), Ubuntu 22.04) |

Geometric staircasing floor, patch arc (`staircasing_results.json`; geometry only, no EM claim):

| FDTD cells/mm | max boundary position error (mm) | perimeter rel. error |
|---|---|---|
| 1 | 0.5222 | 0.1111 |
| 2 | 0.2595 | 0.0732 |
| 4 | 0.1380 | 0.0859 |
| 8 | 0.0725 | 0.0890 |
| 16 | 0.0360 | 0.0953 |

| GEODE tet mesh | max facet radial error (mm) | RMS (mm) |
|---|---|---|
| L0 | 0.00436 | 0.00175 |
| L1 | 0.00113 | 0.00044 |
<!-- END GENERATED TABLE -->

## Provenance

| Table | Source artifacts |
|---|---|
| Optimized designs | `../nested_dof/L{0,1,2}_*.toml`, `../conformal_results.toml` |
| Gap | derived from the rows above (same level only) |
| Transfer | `../nested_dof/verify/*.toml` |
| FDTD-density | `../../fdtd_density_baseline/staircasing_results.json`, `meep_runtime_scaling.json` |
