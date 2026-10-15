# Matched Meep R4 run (#1088)

This records the R4 (0.25 mm cells) run of the physical bent-patch fixture under the
2026-10-15 operator ruling (fund the converged run, $250 total cap; R4 first, R8 when
quota allows). It is a discretization measurement, **not** a solver cost-to-accuracy
comparison, and it does **not** establish spatial convergence. R8 was not launched.

Records: `benchmarks/fdtd_density_baseline/meep_runs/aws_r4/`. Verify locally with the
standard library (the checks remain active under `python -O`):

```sh
python3 benchmarks/fdtd_density_baseline/meep_r4_verify.py
python3 -O benchmarks/fdtd_density_baseline/meep_r4_verify.py
```

The verifier re-validates the R4 spectrum, using the same checks as the R2 record. It
checks that the stopping settings match R2 and that the us-east-2 RC replay matches
the committed us-east-1 RC spectra exactly. It also checks the package set against the
lock and the worker price ladder. Finally, it recomputes `resolution_trend.json`
(R1/R2/R4 trend and R8 projection) from the committed records and logs, and checks
`SHA256SUMS`. `--write` regenerates the trend file.

## Environment

One dedicated `r8g.8xlarge` (32 Neoverse-V2 cores, 247 GiB usable) in **us-east-2c**,
worker `geode-worker-meep-1088`. us-east-1 was not used because it is out of quota and
shared with the fleet. The us-east-2 quota is 32 vCPU. `ssm:GetParameters` is denied in
us-east-2, so `geode-worker.sh` resolved the Ubuntu 24.04 arm64 AMI through
`describe-images`. The image was rebuilt from `Dockerfile.matched` and the explicit lock.
All **179** package URLs were replayed, and the exported package set matches the lock.
The new image ID differs from the us-east-1 build; the package set does not. The RC
oracles at R4 reproduce the committed `aws_pilot/rc_{volume,sheet}_R4.json` spectra
bit-for-bit (max relative impedance error 1.1847e-4), and the geometry occupancy check
passes. `run_r4.sh` is the exact production wrapper.

| Setting | R2 (#1105) | R4 (this run) |
|---|---|---|
| Ranks / threads per rank | 32 / 1 | 32 / 1 |
| `--after-source` / `--check-interval` / `--max-checks` / tolerance | 1200 / 600 / 4 / 0.001 | 1200 / 600 / 4 / 0.001 |
| Docker memory limit | 200 GiB | 240 GiB |
| `--max-estimated-gib` | default 48 | 120 (planning estimate 110.37 GiB) |
| Harness `matched_antenna.py` SHA-256 | `66d3285f…` | `66d3285f…` (unchanged) |

Container memory was sampled from the cgroup `memory.peak` every 30 s. That is a
simultaneous container peak, unlike the per-rank RSS sum the harness reports.

## R4 result

| Quantity | R1 | R2 | R4 |
|---|---:|---:|---:|
| Logged grid | not recorded | 384×366×263 | 768×732×526 (295.7 M cells) |
| Ranks | 4 | 32 | 32 |
| Wall time (s) | 309.9 | 1,829.2 | **21,299.5** (5.92 h) |
| Sum of rank peak RSS (GiB) | 1.84 | 14.46 | **73.07** |
| Container cgroup peak (GiB) | — | — | **71.27** |
| Final time / steps | 3833.5 | 4433.5 / 17,734 | 4433.375 / 35,467 |
| Checks run; temporal tolerance met | 3; yes | 4; yes | 4; yes |
| Final max complex-S11 drift | 9.42e-4 | 6.60e-4 | **2.68e-4** |
| Minimum sampled \|S11\| | −7.10 dB at 2.70 GHz (edge) | −2.71 dB at 2.20 GHz (edge) | **−12.22 dB at 2.20 GHz (edge)** |
| Worst \|S11\| in 2.4–2.4835 GHz | −1.19 dB | −1.20 dB | −0.89 dB |
| Z at 2.45 GHz (Meep convention, Ω) | 4.323 − 12.030j | 3.581 − 8.423j | **2.837 − 9.380j** |

The successive R4 drifts were 0.0153105, 0.00436666, 0.00104201 and **0.000267994**. As
at R2, the 0.001 criterion is met only on the fourth and final check. A small DFT change
is a stopping diagnostic, not a certified error bound. Stepping ran at a median
0.588 s/step (R2: 0.0989 s/step). Wall time per step, including initialization and DFT
extraction, was 0.601 s.

## R1/R2/R4 trend: no spatial convergence

The maximum complex-S11 change between grids is **0.47094** from R1 to R2 and
**0.50115** from R2 to R4. The ratio is 0.94, so the difference between successive grids
did **not** shrink on refinement. These three grids show no spatial convergence, and no
convergence order can be inferred from them.

The spectrum still changes qualitatively. At R4, the reactance crosses zero near
2.30 GHz, and |S11| falls steeply to −12.2 dB at the 2.20 GHz lower band edge. This
suggests a resonance at or below the bottom of the sampled 2.2–2.7 GHz window, but the
window does not resolve it. None of the three grids has an interior minimum. This is a
diagnostic observation. It is not attributed to one cause. Grid-dependent model
elements remain as candidates: one-cell metal shells and port resistor, staircased
arcs, no subpixel averaging, 1/f loss and an unconverged PML. GEODE equivalence is not
established. See `meep_runs/aws_r4/spectra.png`, which was rendered by `meep_r4_plot.py`
in the locked image with Matplotlib 3.11.2.

## What R8 would need (from R4 actuals)

The full arithmetic is in `resolution_trend.json` → `r8_projection`. Each axis doubles
(2,365.6 M cells), dt halves, and the final time is assumed unchanged (70,934 steps).

- **Memory: about 570 GiB** (8× the R4 cgroup peak) to **585 GiB** (8× the rank-RSS
  sum). That is far below the 882 GiB from the 400 B/cell planning formula and the
  "~0.9 TiB" in the 2026-10-15 ruling. With 15% headroom, the smallest fitting r8g is
  **r8g.24xlarge** (96 vCPU, 768 GiB). r8g.16xlarge (512 GiB) does not fit. r8g.48xlarge
  (192 vCPU, 1,536 GiB) also fits.
- **Time:** the measured R4 cost is 6.36e-8 rank-seconds per cell-step (R2:
  8.56e-8). Each R8 rank on 192 ranks holds about 12.3 M cells, against 9.2 M at R4,
  so the per-rank work is comparable. Stepping alone at ideal efficiency:
  **15.4 h on r8g.48xlarge** or **30.9 h on r8g.24xlarge**. At an *assumed* 70%
  parallel efficiency (not measured), these become 22.1 h and 44.1 h. Two-socket NUMA
  and MPI communication on the larger hosts are not characterized.
- **Cost:** r8g prices are linear in size, so the two hosts cost the same per solution:
  **about $175 at ideal efficiency, $250 at 70%**. These are stepping only, at the
  2026-10-12 us-east-1 list rates; r8g.24xlarge is derived from the linear ladder,
  $5.65536/h. Initialization, image rebuild, and any extra check windows (each 600
  time units ≈ 9,600 steps ≈ 2.1 h on r8g.48xlarge at ideal efficiency) add to this.
- **Quota:** r8g.24xlarge needs 96 vCPU and r8g.48xlarge needs 192. Neither fits
  us-east-2 (32 vCPU). Both need room in the shared us-east-1 quota (300 vCPU; the
  512 request is pending).

**Budget position:** about $13.0 of the $250 cap has been estimated so far (R2 $1.45 + R4
$11.54), leaving about $237. The ideal-efficiency R8 fits; the 70%-efficiency case does
not. Before production, measure a short R8 allocation/stepping probe on the target host
to replace the assumed efficiency. Also decide whether to keep the 2.2–2.7 GHz DFT
window. The R4 feature lies at its lower edge, so R8 on the same window may not resolve
it. Widening the window would change the harness hash and need its own provenance
record. These are operator decisions; nothing here launches R8.

## Spend and teardown

Worker launched 2026-10-15T03:37:05Z and was **terminated** at 09:44:28Z (22,043 s).
Its 200 GB volume was deleted with it, and no tagged volume remains in us-east-2. A
scheduled OS shutdown at 16:04Z bounded the run independently. The conservative
elapsed-time estimate is **$11.54**: 6.12 h × $1.88512/h, the us-east-1 table rate,
assumed equal in us-east-2. That is not an AWS bill. The R2 pilot's retained us-east-1
volume (meep1, about $16/month) is unchanged.
