# Matched Meep harness, version 2, and the corrected ladder (#1146)

`DIAGNOSIS.md` (#1088, PR #1144) traced the Meep/GEODE disagreement on the bent
`patch_conformal_L0` board to the v1 harness. Its one-cell `mp.metal` shells, run without
subpixel averaging, give Ez and the tangential field different effective gaps on a curved
board. The same setup also had three smaller faults:

- a 2.0 mm discrete gap in place of 1.6 mm;
- a port of 66.7 Ω (R2) or 42.9 Ω (R4) in place of 50 Ω;
- an FR-4 loss that is exact only at 2.45 GHz.

This record fixes the four items in a new harness version,
`matched_antenna_v2.py`. It then checks the fixed harness on the flat board and runs the
corrected bent ladder at R2.5 and R5.

**Version 1 is unchanged.** `matched_antenna.py` (SHA-256 `66d3285f…`) and
`diag_antenna.py` are untouched, so the committed R1/R2/R4 and diagnosis records keep
their provenance. `meep_r4_verify.py` and `meep_diag_verify.py` still pass. On this host
the v1 harness reproduces the committed R1 antenna run and the R2 RC oracles **bit for
bit** (section 2).

Records: `benchmarks/fdtd_density_baseline/meep_runs/v2/`. Verify with the standard
library (the checks stay active under `python -O`):

```sh
python3 benchmarks/fdtd_density_baseline/meep_v2_verify.py
python3 -O benchmarks/fdtd_density_baseline/meep_v2_verify.py
python3 benchmarks/fdtd_density_baseline/meep_v2_mutation_test.py   # every corruption must fail
```

`--write` regenerates `meep_runs/v2/ladder.json`. The verifier checks the environment
replay, v1 integrity, the probes and oracles, the grid audits (recounted from the
recorded slices), every spectrum and its stopping test, and the GEODE/Palace comparison.
It re-asserts each finding below against its own tolerance. Every v2 record was written
by one harness version, `matched_antenna_v2.py` `af907349…`, archived in `v2/sources/`.

## Result

RESULT_PLACEHOLDER

## 1. What version 2 changes

Everything is built on Meep's Yee lattice in integer cell indices (dx = 1/R).

| Item | v1 (`matched_antenna.py`) | v2 (`matched_antenna_v2.py`) |
|---|---|---|
| Lattice | not used | The origin is a lattice point. E_a is sampled half a cell off the lattice along a and on it along the other axes: Ez at (i, j, k + ½)·dx. The probe confirms this for even and odd cell counts, and every production run re-reads it from Meep's chi1inv before stepping. |
| Metal | One-cell `mp.metal` ring-sector shells outside the substrate faces | Zero-thickness PEC sheets on tangential-E planes (only the Ex/Ey samples on the plane are metal), as for GEODE's PEC faces. The bent board is a staircase of vertical columns. Each column takes the lattice plane k_mid nearest the curved mid-surface at its centre, with ground at k_mid − n/2 and patch at k_mid + n/2. A one-cell vertical PEC riser on the shared Ez line closes each conductor where neighbouring columns differ. |
| Gap | 2.0 mm discrete (R2, R4) | n = h/dx cells for Ex, Ey and Ez in every column: 4 at R2.5, 8 at R5. Only resolutions where n is a whole even number are accepted. |
| Port | dx × w × h resistor block, R2 66.7 Ω, R4 42.9 Ω | A resistor on one Ez line, n_w = w/dx rows by n samples, so the discrete resistance is exactly **50.0 Ω**: 4 × 4 samples at R2.5, 8 × 8 at R5. |
| Source | Ez current sheet (0, w, h) | One point current per resistor Ez sample, with moment dx² (the sheet's moment per sample). The probe shows the v1-style sheet puts **23.4%** of its current on samples outside its own, because Meep integrates sources with hat functions (0.875² of the weight stays inside). A point current on a sample deposits only on that sample. |
| Voltage | Box DFT monitor interpolated to voxel centres | The discrete line integral of the same resistor samples, read by zero-size Yee-grid DFT monitors |
| FR-4 loss | D-conductivity at 2.45 GHz: tan δ ∝ 1/f | Two models. `lorentz`: a Lorentzian at 20 f₀ plus D-conductivity, solved for ε′ = 4.4, tan δ = 0.02 and d tan δ/df = 0 at f₀ = 2.36 GHz. `conductivity`: v1's model re-anchored at 2.36 GHz. Production uses `conductivity` (section 5). |
| Open substrate face | n/a (no tangential samples on the face) | Tangential samples on the open top face take ε = (1 + 4.4)/2, the exact average for a grid-aligned tangential interface |
| DFT window | 2.2–2.7 GHz | 1.6–3.0 GHz in 0.01 GHz steps; the v1 Gaussian pulse is kept |

Lengths that are not whole cells are snapped and recorded in each run's `layout`.

- **Patch length.** 29 mm is 72.5 cells at R2.5, so the R2.5 patch is **28.8 mm** (72
  cells, the same samples Meep's own block gives). At R5 it is exactly 29.0 mm (145
  cells), with the board centred half a cell off the origin.
- **Probe inset.** It is exact at R2.5 (7.0 mm from the patch edge, centre y = 7.4 mm). At
  R5 it is **7.1 mm**, because a 50 Ω port of 8 rows cannot also sit exactly 7.0 mm from
  an edge on the same lattice.
- **Patch width.** The bent patch covers the columns that lie wholly inside its 38 mm arc
  on the mid-surface: 37.2 mm projected at R2.5.

These are O(dx) differences that are fixed per resolution, and they belong to the
R2.5 → R5 trend.

## 2. Checks of the harness

All checks ran on the production host with the production image.

| Check | Result |
|---|---|
| v1 R1 antenna (8 ranks here, 4 in the committed record) | snapshots identical to `meep_runs/initial_R1.json` |
| v1 RC oracles R2 (volume, sheet) | snapshots identical to `aws_pilot/rc_{volume,sheet}_R2.json`; `matched_geometry_verify.py` passes |
| Probe: Yee offsets, 24- and 25-cell boxes | Ex (½, 0, 0), Ey (0, ½, 0), Ez (0, 0, ½) for both parities |
| Probe: one step of a constant point current | only the sourced sample changes, by exactly −dt/dx; a point DFT monitor reads it, and one on the neighbouring sample reads 0 |
| Sheet-equivalent point currents with the v1 readout, R2 | reproduce v1 `rc_sheet_R2` to 5.8e-15 in S11 (the antenna's normalization) |
| Volume RC oracle (point currents and point monitors) | max relative impedance error 4.12e-4 (R2, equal to v1's), 2.63e-4 (R2.5), 8.82e-5 (R5) |
| FR-4 material oracle, recovered ε(f) | `lorentz`: ε′ within 3.1e-5 (R2.5) and 1.0e-5 (R5) of the design, tan δ within 1.3e-4 and 5.2e-5 relative. `conductivity` R5: tan δ within 5.1e-5. Meep's own `Medium.epsilon` equals the design formula to 1e-12. |
| Grid audit, R2.5 flat and bent (initialization only) and in every production run | no errors: every patch column's Ex reads metal–(n − 1) FR-4–metal, every interior Ez line n FR-4 samples (n − 1 on staircase steps, between two risers), port samples and Ey links resistor, neighbours FR-4, patch rows as laid out |

One Meep detail found here: a zero-size Yee-grid DFT volume exactly on a binary-exact
sample position (R2, dx = 0.5) reads 0 in Meep 1.34. The monitors are therefore placed
1e-7 dx off the sample, which puts a relative weight of 1e-7 on the neighbouring sample.
The R2 oracle and the probe use this placement.

Discrete board (from the audits; half-patch counts in `ladder.json`):

| Grid | Gap (Ex / Ez) | Patch columns | Interior Ez lines on a step | Port | DIAGNOSIS E/H factor |
|---|---|---|---|---|---|
| flat R2.5 | 4 / 4 cells | 95 (38.0 mm) | 0 of 94 | 4 × 4, 50.0 Ω | 1.000 |
| bent R2.5 | 4 / 4 cells | 93 (37.2 mm) | 8 of 92 (3 free samples) | 4 × 4, 50.0 Ω | 0.986 |
| bent R5 | R5_GRID_ROW |

The E/H factor is DIAGNOSIS.md's model, applied here only as a description of the
staircase steps. That model overpredicted the step effect tenfold on the v1 cell
staircase, and section 3 shows it does so here too.

## 3. Runs and results

One r8g.8xlarge in us-east-1. Settings as in the v1 R2/R4 and diagnosis runs: stepping
1200 time units after the source, then checks every 600, stopping when the maximum
complex-S11 change is ≤ 0.001 (at most 6 checks at R2.5 and 5 at R5). All runs met that
tolerance. A small DFT change is a stopping diagnostic, not a certified error bound.
FR-4 uses the `conductivity` model anchored at 2.36 GHz. Dips are parabolic vertices of
|S11| in dB; Z at the dip is interpolated.

| Run | Ranks | Checks (final drift) | Dip (GHz) | Depth (dB) | Z at dip (Ω) |
|---|---|---|---|---|---|
| `c_flat_R2.5` | 16 | 4 (8.0e-4) | **2.36833** | −29.2 | 48.0 + 0.5j |
| `c_bent_R2.5` | 16 | 4 (6.2e-4) | **2.36673** | −23.4 | 44.8 − 2.7j |
| `c_bent_R5` | 32 | R5_RUN_ROW |

Meep uses exp(−iωt) and GEODE exp(+iωt), which flips Im Z. |S11| and Re Z are unaffected.

GEODE and Palace on the same fixture (`convergence.toml`, `v6/v6_summary.toml`):

| Reference | Flat dip (GHz) | Bent dip (GHz) |
|---|---|---|
| GEODE P1 L3 / P2 L1 | 2.34336 / 2.35802 | 2.34219 / 2.35664 (the gate-G1 lower bound) |
| GEODE P1 X1 combined rung (1.74 M DOF) | — | 2.36595 (Re Z 45.9 Ω) |
| GEODE rim extrapolation (X1 fit, f_∞ / order-1 Richardson) | — | 2.37951 / 2.37235 |
| Palace L1 p2 / p3 | — | 2.35703 / 2.36597 |
| GEODE bend shift (bent − flat), every level | −0.97 to −1.58 MHz | |

COMPARISON_PLACEHOLDER

## 4. Cost on the same host type

Both codes ran on AWS r8g.8xlarge (32 Graviton4 vCPU, 256 GiB). Meep's memory is the
container cgroup peak, sampled every 30 s. GEODE's is `ps` RSS sampled every 50 ms.

| Run | Output | Wall time | Memory |
|---|---|---|---|
| Meep v2 flat R2.5, 72.3 M cells, 16 ranks (beside the bent run) | full 1.6–3.0 GHz spectrum, 141 points | 6,900 s (1.92 h) | 19.1 GiB |
| Meep v2 bent R2.5, same | same | 6,903 s (1.92 h) | 19.1 GiB |
| Meep v2 bent R5, R5_CELLS cells, 32 ranks | same | R5_COST_ROW |
| GEODE P1 X1 combined rung, 1.74 M DOF (v6 X1) | dip by pre-scan and golden section, 19 solves | 11,019 s (3.06 h) | 208.1 GiB |

COST_PLACEHOLDER

## 5. Residuals and limitations

- **FR-4 loss.** Production uses tan δ = 0.02 · 2.36/f, which is 0.0205–0.0195 across
  2.30–2.42 GHz and 0.0295/0.0157 at the window edges (1.6/3.0 GHz). This affects loss and
  depth, not the resonance. The flat-loss `lorentz` model holds tan δ at 0.02000–0.02001
  over 2.30–2.42 GHz and ≤ 0.0215 over the whole window, and its discrete Meep form
  matches the design (section 2). It was not used for production because Meep allocates
  polarization fields for every chunk the board touches. Measured on the two R2.5 runs
  before they were stopped (`logs/killed_lorentz_*`), it took 438 B/cell (31.8 GB
  container) against 280 B/cell, and 0.42–0.47 against 0.30–0.31 s/step. R5 would need
  about 253 GB, which does not fit this host.
- **Staircase steps.** On the bent board a fraction of interior Ez lines sit on steps,
  with n − 1 free samples between two risers: 8 of 92 at R2.5 (R5_STEPS at R5). The
  gap is vertical: n·dx = 1.6 mm in every column, while the true board's vertical gap
  is 1.600–1.629 mm (h/cos φ) and its normal gap is 1.6 mm.
- **Snapped lengths.** See section 1: the patch is 28.8 mm at R2.5 and the inset 7.1 mm
  at R5.
- **Not convergence-tested here.** The PML (25 mm, Meep's default profile) and the box,
  both matched to GEODE's UPML extents, as in v1.
- **DFT decimation.** Meep 1.34 updates DFTs in batches; a check interval of 20 time
  units can see no update. The production interval is 600.
- **Two bent resolutions.** They give a trend, not a convergence order.

## 6. Incidents (records kept)

- **First production attempt.** It wrote a partial record after the first check with the
  drift still `inf`, which strict JSON rejects. Both R2.5 runs aborted at t = 2033
  (`crashed/`). The fix writes `null` there. The whole antenna path was then exercised on
  a small-domain configuration before the runs were restarted.
- **Lorentzian production runs.** They were stopped after about 1,000 steps, for the
  memory and speed reasons above (`logs/killed_lorentz_*`).
- **Superseded checks.** The probes, oracles and R2.5 audits first ran with an earlier
  harness version (`bccbd309…`). They were re-run with `af907349…` and reproduced the
  oracle errors exactly. Only the re-runs are committed.

## Spend and teardown

SPEND_PLACEHOLDER
