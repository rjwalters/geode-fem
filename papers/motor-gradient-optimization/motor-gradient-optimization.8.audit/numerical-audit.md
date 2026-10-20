# Numerical audit: motor-gradient-optimization.8

## Method

`python3 verify.py` (the configured `artifact_verify` command, run from the version dir of a `git archive 92120067` scratch copy) passes with exit 0. That covers all inherited v1-v6 blocks plus both `verify_exp2.py` blocks.

Separately from verify_exp2.py, three auditor scripts recompute every Section 7 number from the package data, along with every number in the revised abstract, introduction, discussion and conclusion. The scripts are `auditor-scripts/x8.py`, `x8t7.py` and `x8s.py`, with outputs `*.out`. The data are `figures/src/data/exp2/runs.json`, `refinement.json` and `provenance/setup/*.json`. The scripts share no code with the verifier or the renderer:

- **Credit** is recomputed from `status`, `admissible`, `mean_full >= anchor.mean_floor` (the floor test itself, not `mean_ok`), `wall_fe <= 120` and `F_val + max(0.1 F_val, 2.5e-7) <= T`. It does not use the harness `credited` flag. The recomputed rule disagrees with the harness flags on 0 validations.
- **Inventory binding**: the inventory hashes to `0650f4ee…`, with 5,453 entries. 100 of 100 run files match it, and the three setup records (`session.json`, `calibration_primary.json`, `anchor_preflight.json`) hash to their inventory entries.
- **F in refinement.json** was recomputed from the raw per-angle torques (population variance summed over currents). The maximum relative deviation over all 213 checks is 4.4e-16.
- **Selection rule**: re-derived from the validation records as the lowest feasible F over both repeats, plus the earliest repeat-1 crossing per method, space and target. All 35 cohort roles (10 representatives and 25 crossings) come out with the same run and validation index as `refinement.json`, with 0 mismatches.
- **Tables 5, 6 and 7** were regenerated independently and match in 25/25, 25/25 and 33/33 rows at displayed precision. Table 7 is compared programmatically cell by cell (`x8t7.out`: 0 diffs).
- **Re-render**: `render_exp2.py`, run in an isolated `uv` environment (numpy 2.5.4, matplotlib 3.11.2) on the scratch copy, reproduces both Exp 2 figures (PDF and PNG) and all three Exp 2 tables byte-identically. No figure is stale: every rendered output's mtime is later than its script and data.

Convention, as in the v5-v7 audits: "within", "at most", "below" and "less than" are read as true upper bounds; every other number is read as a rounded display value.

## Anchor, protocol and session (§7.1)

| Text claim (optimization.tex line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| F0 = 9.6034e-3 (N m)², μ0 = 1.44084 N m (74-75) | runs.json anchor | 9.603404e-3; 1.4408390 | yes | |
| Floor 1.0025 μ0 (60, 82) | anchor.mean_floor / μ0 | 1.0025000 | yes | |
| COBYQA 2n+1; n = 8 (A), 18 (B) → 17, 37 (17-18, 176-178) | refinement.json `x` length | 8; 18 | yes | |
| 100 runs (104) | runs.json | 100 unique (method, space, start, repeat) | yes | |
| NumPy 2.5.4, SciPy 1.18.1, OpenBLAS, x86-64 (107-108) | session.json `linear_algebra` | numpy 2.5.4; scipy 1.18.1; scipy-openblas; x86_64 Linux | yes | "c7a.4xlarge" is not in the setup records. It is documented in `refs/cross-platform-anchor-protocol.json` and `refs/main-run-runbook.md` (advisory N7) |
| 15 concurrent single-threaded processes (108) | session.json `procs`; calibration `processes` | 15; 15 | yes | |
| F0 agreed to 2.4e-13 relative, within 1e-9; spaces bit for bit (110-112) | anchor_preflight.json | 2.404e-13 (both spaces); tolerance 1e-9; `spaces_agree_bitwise: true` | yes | |
| Unit 70.94 s, median of 45 concurrent cold validations (112-113) | calibration_primary.json | 45 timings; median 70.935572 s (= unit in every run) | yes | |
| No recovery, deferral or contamination; load peak 21.1 < 1.5N = 22.5 (114-116) | runs.json | 0/100 contaminated; max `in_run_load_max` 21.089; `longest_overload_s` 0 in all runs | yes | |
| 109 CPU-hours (116) | Σ in_run_wall_seconds | 109.09 h | yes | Summed single-threaded process wall time |
| Stops: 38 T99, 46 converged, 13 budget, 3 solver stops (116-118) | stop_reason | 38 / 46 / 13 / 3 | yes | |
| Solver stops: A, space A, LHS 2 both repeats, about 43 units; LHS 3 r1, 119 units (118-120) | in_run_wall_seconds / unit | 42.5, 43.1; 119.1 | yes | |

## Results (§7.2)

| Text claim (line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| All 50 repeat pairs share the validated-objective sequence up to the shorter length; differ in length only on budget or solver stop (124-127) | runs.json | 50/50 prefix-identical; 5 length differences, stop reasons {budget, solver_stopped} only | yes | |
| 111 crossings attained in both repeats; median 2.1%, at most 12.4% relative to repeat 1 (129-131) | \|t2/t1 − 1\| | 111; median 2.131%; max 12.360% (D, space B, LHS 3, T99) | yes | Resolves v7 C2 |
| Space A: no run below F/F0 = 0.043; none reaches T99 (136) | min credited F_val, all space-A runs | 0.043196; no T99 in either repeat | yes | Fig. 8 plots (F+δF)/F0, whose space-A minimum is 0.0475 (advisory N6) |
| Best of A, B, D, E reach 0.043F0; C 0.049F0 (137-138) | per-method min over both repeats | A 0.04321, B 0.04320, D 0.04321, E 0.04332; C 0.04893 | yes | |
| All five within 0.17% above the mean floor (138-139) | best design mean_full / floor − 1 | 0.124, 0.130, 0.123, 0.163, 0.156% | yes | |
| LHS 4: A, B ≈ 0.09F0; D 0.044F0; C and E no credited validation in either repeat (142-143) | best per run | A 0.0906, B 0.0904, D 0.0445 (both repeats); no-credit files = E and C, a, lhs4, r1 and r2 | yes | |
| Space B best 0.004-0.009F0; every method except C reaches T99 from ≥ 4 of 5 starts (144-145) | | 0.00439 (D) to 0.00846 (B); T99 starts A 5, B 4, C 1, D 5, E 4 | yes | |
| Repeat 1: only three crossings after 60 units, all T99 in space B, two by B, one by D (159-161) | attainment wall_fe > 60 | B θ0 90.4, B LHS 4 62.3, D LHS 3 83.3 | yes | Resolves v7 N3 |
| B/A geometric mean 3.1-3.3 (A), 5.1-7.8 (B) (167-168) | Table 6 regenerated | 3.098-3.255; 5.059-7.787 | yes | |
| D/E 2.4-2.8 (A), 3.1-7.2 (B) (168) | | 2.445-2.754; 3.143-7.204 | yes | Table 6 shows 2.45 at two decimals (advisory N3) |
| 45 finite optimizer pairs all favor SLSQP, 1.46-21.5 (169-170) | | 45 (B/A 24, D/E 21); min 1.4647, max 21.537; all > 1 | yes | |
| E/A 0.62-0.79; D/B 0.45-0.56 (171-172) | | 0.620-0.794; 0.445-0.562 | yes | |
| D/A 1.68-1.83 (A), 2.25-4.32 (B); one of 25 pairs a tie (0.99, space-A LHS 2, T90) (173-175) | | 1.675-1.828; 2.253-4.315; 25 pairs; one ≤ 1: 0.9925 (D 7.3 vs A 7.4) | yes | "Tie" covers a 0.75% D advantage, inside the 2.1% median timing noise (advisory N4) |
| E: no credited validation from space-A LHS 4; space-B LHS 1 reaches T50, stalls at 0.28F0, misses T90 and T99 (180-182) | | no-credit confirmed; T50 at 1.12 units; best 0.2789F0 | yes | Resolves v7 N2 |
| Four start-target cells where D reaches a target E misses (182-183) | | (a, T50, lhs4), (a, T90, lhs4), (b, T90, lhs1), (b, T99, lhs1) | yes | |
| A reaches every target any method reaches, from every start (184) | | true for all 25 cells | yes | |
| C attains T99 once in B; T90 from two of five space-A starts (185-186) | | 1; 2 (θ0, LHS 1) | yes | |
| Table 5 (time to target), 25 rows | regenerated from validations | identical at 0.1 | yes | |
| Table 6 (ratios), 25 rows | regenerated | identical at 0.01 | yes | |

## Final refinement (§7.2, Table 7)

| Text claim (line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| 33 designs; six checks; 15 conforming; 213 checks completed (217-223) | refinement.json | 33; 6 × 33 + 15 = 213; all `ok` and `complete` | yes | |
| They do not certify each of the 111 repeat-1 crossings (224-225) | | 111 repeat-1 crossings; 25 selected | yes | |
| 16 moved-mesh designs reproduce F_val to below 2.2e-12 (234-236) | primary F vs study F_val | 16; max 2.113e-12 | yes | Resolves v7 N4 (bound) |
| 15 fresh-mesh designs, at most 1.7e-3 (236-238) | | 15; max 1.685e-3 | yes | |
| Two θ0 designs reproduce F0 (238-239) | | rel −2.2e-16 both | yes | |
| L3: −4.0% to +6.3% over 33 designs; −4.0% to +1.1% over 25 crossings (243-245) | | −4.028% to +6.334%; crossings −4.028% to +1.054% (25 roles on 23 designs) | yes | |
| Largest increase: overall best design, D's in space B, 0.0044F0 → 0.0047F0 (245-246) | | argmax L3 = representative/D/b; 0.004394 → 0.004672; lowest primary F of all candidates | yes | |
| 40-angle and shifted at most 0.11%; tight less than 1.1e-5 (247-249) | | max 0.1065%; max 1.0054e-5 | yes | Resolves v7 N4 (bound) |
| Conforming 1.1-4.4% on space-A designs; 2.0-2.1% at θ0 (249-251) | | 1.064-4.397%; θ0 a 2.070%, b 2.012% | yes | Resolves v7 C3 |
| All 25 crossings satisfy target with allowance at L3; smallest margin 1.2% (251-253) | | 25; min margin 1.205% (crossing/D/b/T99) | yes | |
| Mean floor met in primary, L3, g40, tight, shifted; closest 0.012% (253-255) | | min 0.0121% (representative/E/b, L3) | yes | |
| Held-out ratio 0.65-1.73 (261-262) | | 0.6451 (crossing/E/b/T90) to 1.7275 (representative/D/b) | yes | |
| Five best space-A designs 1.07-1.09 (263-264) | | 1.0713 (C) to 1.0927 (B) | yes | |
| Space B: four best designs 1.23-1.73, B's best 0.89 (264-266) | | A 1.2325, C 1.2900, D 1.7275, E 1.2260; B 0.8922 | yes | Wording "keep less of their reduction, by factors of" (advisory N5) |
| D's best 0.0044F0 at training, 0.0076 of θ0 at held-out (266-267) | | 0.004394; 0.00759 | yes | |
| Table 7, 33 rows × 8 columns | regenerated | 0 diffs | yes | |

## Abstract, introduction, discussion and conclusion (main.tex)

| Text claim (line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| Abstract: 100 budget-matched runs; all 45 paired comparisons where both succeeded; geometric means 2.4-7.8 (19-22) | Table 6 | 45; B/A and D/E cell geometric means 2.445-7.787 | yes | Resolves v7 C1 |
| Abstract: coarser mesh saved only 1.3-2.2 times (22-23) | 1 / (E/A, D/B geometric means) | 1.260-2.245 | yes | |
| Abstract: with SLSQP, missed targets from one start in each design space (23-24) | | E: space-A LHS 4, space-B LHS 1 | yes | |
| Abstract: every selected crossing met its target on a finer mesh; −4.0% to +1.1% (24-25) | | 25/25; −4.028% to +1.054% | yes | |
| Abstract, inherited sentences (26-42) | verify.py v1-v6 blocks | pass | yes | Two scope qualifiers were dropped in the reorder (advisory N2) |
| Intro: 2.4-7.8 times sooner by geometric mean (76-78) | | as abstract | yes | |
| Discussion: adjoint optimizer reaches common targets sooner than COBYQA at matched resolution; D does not close the gap (789-792) | | true wherever both attain; D/A geometric means 1.68-4.32 | yes (paired) | Lacks the "wherever both reach" qualifier used everywhere else (advisory N1) |
| **Discussion: "In space A, every method's best design sits on the active mean-torque floor near 0.043F0" (798-801)** | §7.2 l.137-138; Table 7 "best C" | A, B, D, E 0.0432-0.0434; **C 0.0489** | **no** | **Critical C1**. True that all five are on the floor (within 0.17%); false that all five are near 0.043F0 |
| Conclusion: all 45 paired comparisons; smaller and less reliable saving (832-835) | | 45; 1.26-2.25; E misses 4 cells D reaches | yes | |
| Conclusion: every selected crossing survived refinement, −4.0% to +1.1% (835-836) | | as abstract | yes | |
| Availability: 213 final-check results; 5,453 files (867-869) | | 213; 5,453 | yes | |

## Figures

- **Fig. 8 (exp2-quality)**: re-rendered byte-identically. Curves end at each run's stop. The missing space-A LHS 4 curves for C and E match the four no-credit files.
- **Fig. 9 (exp2-time-to-target)**: now has five panels (space A T50 and T90; space B T50, T90 and T99). Re-rendered byte-identically. Its markers and crosses match Table 5. Resolves v7 N7.
- Every non-Exp-2 figure file and figure source is byte-identical to v7 (`diff -rq` of the two `figures/` trees). Only the two Exp 2 figures, `extract_exp2.py`, `render_exp2.py` and `runs.json` differ, and `provenance/setup/` is new. `runs.json` gains two per-run fields (`in_run_load_max`, `longest_overload_s`). A structural comparison finds 0 changed or removed values against v7, so no study outcome changed (consistent with the changelog). **Stale figures: 0.**
