# Tuned gradient-free baselines and the scaling with parameter count (#1147)

The antenna v6 review (must-fixes M1–M3) asked three things:

- whether the claimed order-of-magnitude advantage of the adjoint over gradient-free search survives reasonable gradient-free settings;
- how the advantage depends on the number of parameters;
- how far design C's lower band edge drifts with the air margin.

Every number below comes from a committed TOML in this directory or from the 38-DOF references of #1126 and #1130. `scripts/tuned_summary.py --write` collects them into `tuned_summary.toml`. `crates/geode-core/tests/patch_conformal_tuned.rs` re-derives every count from the raw trajectories and pins it. It also checks that the summary is fresh: the file set, each source's sha256, and every derived count.

## Problem and counting

The problem is X4's (`../v6/README.md`, section X4), unchanged:

- the L1 fixture (`patch_conformal_L1.msh`, sha256 `b8119101…`);
- p = 1 and the unshifted 5-point band (2.400–2.4835 GHz);
- the soft-max |S11|² objective with β = 25;
- the 0.25 volume guard, and a cold start from x = 0;
- geode as the forward solver throughout.

The x = 0 value, −1.4695 dB, is the same in every record and is checked. That also fingerprints the fixture for the driver-written records, which do not carry the fixture hash.

Solves are counted on X4's rule:

- One solve is one band frequency with one LU factorization.
- **Adjoint:** 5 solves for the start, then 5 per line-search trial that passed the guard. The adjoint solve reuses the forward LU and is not counted separately.
- **Gradient-free:** `solves_so_far` at the first evaluation whose best-so-far reaches the level, less the 5 solves of the x = 0 evaluation.
- Guard-rejected samples cost no solve but use an evaluation.
- FD gates and verification sweeps are excluded.

A level not reached within a run's budget is **censored**. It is written as −1 in the summary and "not reached" below, with the run's evaluation budget and the solves it actually used.

The bases:

| n | basis | flags |
|---|---|---|
| 4 | rectangle: ΔW, ΔL, probe inset, probe width (X4's `--rect4`) | `--outline 8 --feed --rect4` |
| 14 | 2 outline B-splines per edge (degree 1) + 4 corners + 2 feed | `--outline 2 --corners 6 --feed` |
| 38 | X4's: 8 per edge + 4 corners + 2 feed | `--outline 8 --corners 6 --feed` |

Optimizers:

- **sep-CMA-ES:** the in-house `--gradient-free` implementation, λ = 4 + ⌊3 ln n⌋ (8, 11, 14).
- **Full CMA-ES:** pycma 4.5.0, full covariance with pycma's defaults (active CMA), the same λ, and guard failures ranked last by their violation (`scripts/cma_loop.py`).
- **NEWUOA:** PRIMA, `rhobeg` 0.5, `rhoend` 1e-3, through `--eval-server` (`scripts/palace_loop_prima.py --npt`). Guard failures are answered with X4's penalty.

## Results

Solves to each worst-of-band level (dB). The ratio is to the adjoint at the same n.

### B1–B3: 38 DOFs (adjoint: 20 / 30 / 30 / 35 / 60 solves to −5 / −8 / −9 / −9.5 / −10 dB)

| run | setting | budget (evals) | feasible | solves used | −9 dB | −9.5 dB | −10 dB | best (dB) |
|---|---|---|---|---|---|---|---|---|
| **B1** NEWUOA | npt = n + 2 = 40 | 190 | 189 | 945 | 380 (**12.7×**) | 400 (**11.4×**) | 650 (**10.8×**) | **−10.06** |
| ref. #1130 NEWUOA (Palace forward) | npt = 2n + 1 = 77 | 190 | 189 | 945 | 415 | 420 | not reached | −9.95 |
| **B2** sep-CMA-ES s11 | σ0 = 1.5 | 190 | 19 | 95 | not reached | not reached | not reached | −6.29 |
| **B2** sep-CMA-ES s12 | σ0 = 1.5 | 190 | 20 | 100 | not reached | not reached | not reached | −1.48 |
| **B2** sep-CMA-ES s13 | σ0 = 1.5 | 190 | 21 | 105 | not reached | not reached | not reached | −1.30 |
| B2, solve-matched (extra) s11 | σ0 = 1.5 | 1000 | 373 | 1865 | not reached | not reached | not reached | −8.81 |
| sep-CMA-ES s11 (extra) | σ0 = 1.0 | 190 | 92 | 460 | 380 (12.7×) | not reached | not reached | −9.29 |
| ref. X4 sep-CMA-ES (3 seeds) | σ0 = 0.5 | 114 / 190 / 190 | 107–157 | 535–785 | 385–405 | **385**–520 | 695 (one seed) | −9.56 to −10.02 |
| **B3** full CMA-ES s11 | σ0 = 0.5 | 190 | 139 | 695 | **230 (7.7×)** | 690 (19.7×) | not reached | −9.82 |
| **B3** full CMA-ES s11 | σ0 = 1.5 | 190 | 16 | 80 | not reached | not reached | not reached | −3.51 |

What this shows:

- **The tuned baselines do not bring the −9.5 dB ratio below 10×.** The best of every gradient-free run at 38 DOFs reaches −9.5 dB in 385 solves (X4 seed 2), against the adjoint's 35, which is 11×. B1 is 11.4×.
- **At −9 dB one setting does go below 10×.** Full-covariance CMA-ES at σ0 = 0.5 reaches −9 dB in 230 solves, 7.7× the adjoint's 30. That is one seed. The same run then needs 690 solves to −9.5 dB (19.7×) and does not reach −10 dB in 190 evaluations. At −8 dB the same run is also 7.7×, and sep-CMA-ES at σ0 = 1.0 is 9.8×. **So "an order of magnitude" holds at −9.5 and −10 dB, but not at every level: at −9 dB the best measured ratio is about 8×.**
- **A smaller NEWUOA interpolation set does not halve the cost.** With npt = 40, NEWUOA leaves x = 0 after its 40-point initial design (−6.1 dB at evaluation 43) but needs another 38 evaluations to reach −9.5 dB (evaluation 81, 400 solves). The npt = 77 run reached −9.5 dB at evaluation 85 (420 solves). The prediction in the review, about 6–7× at npt = 40, does not hold. B1 is the first NEWUOA run to reach −10 dB (650 solves, 10.8×), and its −10.06 dB is the best result of any 38-DOF run, adjoint included (−10.04).
- **σ0 = 1.5 is worse, not better, because of the volume guard.** At σ0 = 1.5, 89–92 % of the 38-D samples invert a tet (min volume ratio below 0.25), so a 190-evaluation run makes only 16–21 feasible evaluations (80–105 solves). The step size adapts down slowly (1.5 → about 1.0 over 13 generations). The plateau the review read as "σ too small" is not removed by a larger σ: the guard bounds the useful step. Given 1000 evaluations, about 1865 solves or 2.4× X4's budget, the σ0 = 1.5 seed reaches −8.81 dB and never −9 dB (at least 62× the adjoint's solves to −9 dB). Full CMA-ES at σ0 = 1.5 behaves the same way.

### B4: solves against the number of parameters (same cold start, no band-shift head start)

Adjoint L-BFGS (one run per n) against the **best** gradient-free run at that n at each level. Best-of-k favours the gradient-free side. At n = 4 and 14 there are 3 sep-CMA-ES seeds (σ0 = 1.5) and NEWUOA at npt = n + 2 and 2n + 1. At n = 38 the rows above are used.

| n | adjoint −9 / −9.5 / −10 | best gradient-free −9 / −9.5 / −10 | ratio −9 / −9.5 dB | adjoint best | best gradient-free (run) |
|---|---|---|---|---|---|
| 4 | 55 / 85 / not reached (guard-limited) | 60 / 205 / not reached | **1.1× / 2.4×** | −9.85 dB (110 solves) | −9.97 dB (NEWUOA npt 9, 550 solves) |
| 14 | 35 / 40 / 60 | 115 / 115 / not reached | **3.3× / 2.9×** | −10.00 dB (110 solves) | −9.99 dB (NEWUOA npt 16, 945 solves) |
| 38 | 30 / 35 / 60 | 230 / 385 / 650 | **7.7× / 11×** | −10.04 dB (125 solves) | −10.06 dB (NEWUOA npt 40, 945 solves) |

Per run (budget in evaluations; the solves used are the censoring point):

| n | run | budget | solves used | −9 dB | −9.5 dB | −10 dB | best (dB) |
|---|---|---|---|---|---|---|---|
| 4 | adjoint L-BFGS | 60 steps | 110 | 55 | 85 | not reached | −9.85 |
| 4 | sep-CMA-ES σ0 1.5 s11 / s12 / s13 | 120 | 450 / 460 / 530 | 270 / 320 / 175 | not reached / 365 / not reached | not reached | −9.37 / −9.96 / −9.24 |
| 4 | NEWUOA npt 6 / 9 | 120 | 585 / 550 | 60 / 185 | 305 / 205 | not reached | −9.92 / −9.97 |
| 14 | adjoint L-BFGS | 60 steps | 110 | 35 | 40 | 60 | −10.00 |
| 14 | sep-CMA-ES σ0 1.5 s11 / s12 / s13 | 190 | 615 / 545 / 515 | 430 / 255 / not reached | not reached | not reached | −9.50 / −9.44 / −8.02 |
| 14 | NEWUOA npt 16 / 29 | 190 | 945 / 945 | 115 / 250 | 115 / 370 | not reached | −9.99 / −9.96 |

What this shows:

- **The adjoint's advantage grows with the number of parameters.** To −9.5 dB it is 2.4×, 2.9× and 11× at n = 4, 14 and 38. To −9 dB it is 1.1×, 3.3× and 7.7×. At 4 parameters, which is what the target needs, NEWUOA is about as fast as the adjoint to −9 dB (60 against 55 solves).
- **At n = 4 the adjoint is not the better optimizer.** It stops on the volume guard at −9.85 dB (`distortion_limited`, final gradient norm 0.025). Three gradient-free runs end lower: −9.92, −9.96 and −9.97 dB. None reaches −10 dB, so the rectangle does not meet the coarse-model target from a cold, unshifted start on either side. The #1126 X4 rectangle met it only with the 38-DOF chain's band shift and a feed re-base.
- **At n = 14 the adjoint reaches −10.00 dB in 60 solves, the same count as at 38 DOFs.** No gradient-free run reaches −10 dB within 190 evaluations. The best, NEWUOA at npt 16, ends at −9.99 dB, so this censoring is marginal.
- **The −10 dB column carries an asymmetry.** Several gradient-free runs end within 0.05 dB of −10 dB, while the adjoint at n = 14 lands at −10.003 dB. Read the −9 and −9.5 dB columns as the cost comparison.

### B5: air-margin drift of design C (P1/L1, 41-point dense sweep 2.30–2.60 GHz plus the 9 band points, 50 points)

Design C (`../recentre/rbG_L1_copper_38_sm57_w16.toml`) is re-run with `--rebase-feed --feed-support-legacy --freeze-feed-width` on its own fixture `rbG_L1` and on two new fixtures with a 60 mm and an 80 mm air margin. Nothing else differs (`reference/gmsh/patch_conformal_rbGair{60,80}_L1.yaml`).

| air margin | fixture edges | dip (GHz) | lower −10 dB edge (GHz) | upper edge (GHz) | band (MHz) | lower edge vs 40 mm | upper edge vs 40 mm |
|---|---|---|---|---|---|---|---|
| 40 mm (re-run of `../recentre/`) | 150 541 | 2.38371 | 2.33934 | 2.43038 | 91.0 | 0 | 0 |
| 60 mm | 174 049 | 2.38805 | 2.34232 | 2.43462 | 92.3 | **+2.97 MHz** | +4.24 MHz |
| 80 mm | 206 139 | 2.38802 | 2.34359 | 2.43332 | 89.7 | **+4.25 MHz** | +2.95 MHz |

- **The lower edge rises monotonically with the air margin, by 3.0 and 4.3 MHz.** That is more than design A's 2.25 and 2.8 MHz (`../v6/README.md`, X3). The upper edge and the dip move by 2.9–4.3 MHz and are not monotone in the air margin. The remesh is part of each difference.
- **The 40 mm re-run reproduces the committed `../recentre/` sweep bit for bit** (largest |S11| difference 0 dB), so the shifts are against a reproduced baseline.
- These are P1/L1 shifts. P1/L1 sits about 60 MHz below design C's band on the fine models, so the transfer to the fine-model margins (6.5 / 2.9 MHz) is an assumption, as it was for design A. As an air-box term in the margin budget: **0 to 4.3 MHz on the lower edge**, which by itself takes most of design C's 6.5 MHz lower margin.

## Not done

- **Full CMA-ES has one seed per σ0**, so the 7.7× at −9 dB is a single trajectory. The claim it bounds is "at least one setting reaches −9 dB in under 10× the adjoint's solves".
- **sep-CMA-ES at n = 4 and 14 was run at σ0 = 1.5 only.** At 38 DOFs that was the worst setting. At n = 4 and 14 the guard rejects fewer samples (54–88 % feasible), but a smaller σ0 was not tried there, so the n = 4 and 14 best-of rows may be generous to the adjoint on the CMA side. NEWUOA, the best gradient-free method at n = 4 and 14, does not depend on σ0.
- **NEWUOA was run at `rhobeg` 0.5 only.** The intermediate npt (58) the review suggested was not run.
- **Bayesian optimization was not run.**
- **B5 used geode P1/L1 only.** No Palace p = 2 or 0.2 mm-rim run was made with the larger air margin.
- **No Palace-driven runs** were made for #1147. The geode–Palace agreement is #1130's (0.004 dB on shared designs).

## Reproducing

Each TOML's `[meta].command` gives the exact command. `<dir>` holds the fixtures. `patch_conformal_L1.msh` must have sha256 `b811910151dea09d65e6e056add3203a6df71d90903d24e8ff0ae3401c3b7dab`. The B5 fixtures' sha256 are in `../provenance/patch_conformal_rbG{,air60,air80}_L1.provenance.txt`. Regenerate the meshes with gmsh 4.15.2 on the Mac or x86, not arm64; the 40 mm fixture regenerated to its committed sha256 here. The logs are in `logs/` with host paths and addresses scrubbed: each run's stderr, the eval-server logs and the two queue logs.

- **Binary.** Every run used `patch_conformal_nested_dof` built in release mode at `dcd8df60` (main). This branch leaves the example unchanged. The driver-written and `--gradient-free` records say `generated_at_commit = "unknown"` because they ran outside a git checkout. The B5 records ran inside one and say `dcd8df60`.
- **Hosts.** Two AWS r8g.8xlarge (32 vCPU Graviton4, 256 GiB, Ubuntu 24.04 arm64, us-east-1), each running four streams of 8 threads (`GEODE_NUM_THREADS=8`). One band evaluation took about 80 s per stream.
- **PRIMA** (recorded per the #1138 Judge nit). PRIMA is built from source into a Python 3.12 venv:

  ```sh
  sudo apt-get install -y gfortran python3-dev python3-venv  # GNU Fortran / gcc 13.3.0 (Ubuntu 24.04)
  python3 -m venv venv && . venv/bin/activate
  pip install numpy==2.5.4 cma==4.5.0 "scikit-build-core<0.10" "cmake<4" ninja setuptools_scm scipy pybind11
  git clone https://github.com/libprima/prima.git && cd prima
  git checkout 43863c69ccd6408dedcd782d0dfdcbaad404dcde
  git submodule update --init --recursive    # python/pybind11 at v3.0.2 (45fab408)
  pip install --no-build-isolation -v .      # -> prima 0.7.3.dev1801+g43863c69c
  ```

  The build used scikit-build-core 0.9.10 and cmake 3.31.10. PRIMA's `pyproject.toml` at this commit fails with scikit-build-core ≥ 0.10 (`cmake.verbose`), and it needs the pybind11 submodule and `python3-dev`. This is the same `prima_version` as #1130.
- **AWS spend** was about $34: the two r8g.8xlarge ran 10.1 and 7.6 h (17.7 instance-hours at $1.885), plus their gp3 volumes.
