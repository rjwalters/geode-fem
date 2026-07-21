# FDTD-density runtime scaling — the computational-intractability axis

Measured 2026-07-21 on AWS `m6i.4xlarge` (16 vCPU, 61 GB) with the vendored
`meep-baseline:cpu` container (Meep 1.34.0), on the curved conformal open-radiator
problem from `reference/meep/docker/conformal_baseline_3d.py`. Full data +
projections in `meep_runtime_scaling.json`.

## Measured (single-tenant, seconds/timestep over 61 steps after warmup)

| R (px/mm) | Yee cells | s/step | peak RAM |
|-----------|-----------|--------|----------|
| 4  | 10.3 M  | 0.233 s | 1.6 GB |
| 6  | 34.8 M  | 0.742 s | 4.9 GB |
| 8  | 82.6 M  | 1.715 s | 11.3 GB |
| 10 | 161.3 M | 3.386 s | 21.8 GB |
| 12 | 278.7 M | 5.761 s | 37.4 GB |

Clean power laws: cells `= 161280·R³`; `s/step ∝ R^2.92`; `RAM ∝ R^2.89`;
`dt ∝ 1/R` (CFL/Courant, measured 0.125→0.0417). Steps to a fixed physical
field-decay time therefore scale `∝ R`, so **one forward solve scales `∝ R⁴`**.

## Why this is intractable for a curved conductor

- **Absolute anchor (measured):** at R=8 a single forward solve ran **≥421 steps
  and did not reach field decay within 15 min** → ≥12 min/solve, ≥24 min/gradient.
  A 3-frequency, ≥12-eval band optimization is **≥14 hours at R=8** (a resolution
  that still badly staircases the curve), **≥70 hours at R=12**.
- **RAM wall:** R=14 ≈ 60 GB, R=16 ≈ 83 GB — **R≥14 does not fit** on this 61 GB box.
- **Curve-faithful resolution** (from `staircasing_results.json`: ~6000 cells across
  the ~24 mm feature for ~1 µm fidelity) is **~250 px/mm ⇒ ~10¹⁵ cells** — absurd,
  ~30× beyond the RAM wall before any timestep cost.

## The head-to-head

GEODE's unstructured-tet Nédélec shape adjoint solved the **full 73-DOF freeform
curved-conformal band match to −12.06 dB in 6 steps, one reused factorization,
seconds-to-minutes**, exact at fixed DOF (no staircasing) — see
`../patch_antenna_conformal/conformal_results.toml`.

So the paper's comparative claim rests on **two measured axes**: geometric fidelity
(structured grids staircase the conductor — `staircasing_results.json`) and compute
(structured grids can't practically *run* it at faithful resolution — this file).
A converged FDTD-density optimization is not required to substantiate the claim; it
would only re-confirm the intractability, so it is left as documented future work.
