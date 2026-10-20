# Numerical audit: motor-gradient-optimization.7

## Method

`python3 -I -B verify.py` passes (exit 0): all v6 blocks, plus the two v7 Exp 2 blocks via `runpy`. `verify_exp2.py` alone also passes.

Independently of verify_exp2.py, two auditor scripts (`auditor-scripts/x7.py`, `x7r.py`; outputs `x7.out`, `x7r.out`) recompute every Section 7 number and every changed abstract, introduction, discussion and conclusion number from `figures/src/data/exp2/runs.json` and `refinement.json`. They share no code with the verifier or the renderer. Where possible they derive values from lower-level fields than the verifier uses:

- **Credit** is recomputed from `status`, `admissible`, `mean_full >= mean_floor` (recomputed rather than read from `mean_ok`), `wall_fe <= 120` and `F_val + max(0.1 F_val, 2.5e-7) <= T`. It does not use the harness `credited` flag. All 1,000 (run, target, repeat) attainment cells agree with the harness attainment block (0 disagreements). `mean_ok` agrees with the recomputed floor test on every validation.
- **F** in refinement.json is recomputed from the raw per-angle torques (population variance, summed over currents): max relative deviation 4.4e-16 across all 213 checks.
- **Inventory binding**: the inventory sha256 is `0650f4ee…6e16` with 5,453 entries; 100 of 100 run files match.
- **Selection rule**: re-derived from the validation records (lowest feasible F over both repeats; earliest repeat-1 crossing per method, space and target). It reproduces all 35 cohort roles (10 representatives + 25 crossings) with the same run and validation index, and 0 mismatches.
- **Repeat paths**: 45 of 50 repeat pairs have identical validation sequences. The other 5 are prefixes of each other, cut by the budget. "The two repeats follow the same path" is therefore confirmed.
- **Tables 5, 6 and 7** (`exp2-time-to-target`, `exp2-ratios`, `exp2-refinement`) were regenerated independently. 25/25, 25/25 and 33/33 rows match at displayed precision.
- **Re-render**: `render_exp2.py` (numpy 2.5.4, matplotlib 3.11.2) reproduces both new figures (PDF and PNG) and all three new tables byte-identically. No figure is stale. The inherited v6 figures and tables are byte-identical to v6.

Convention, as in the v5/v6 audits: "within", "at most" and "less than" are read as true upper bounds; every other number is read as a rounded display value.

## Anchor and protocol (§7.1)

| Text claim (optimization.tex line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| Peak current 9.6777 A (21) | refinement.json `i_peak_A` | 9.677657 | yes | |
| F0 = 9.6034e-3 (N m)², μ0 = 1.44084 N m (57-58) | runs.json anchor | 9.603404e-3; 1.4408390 | yes | |
| Floor 1.0025 μ0 (48) | anchor `mean_floor` / μ0 | 1.0025000 | yes | The optimizer constraint is relative to the *training* θ0 mean (driver.py l.20); credit uses the validation μ0 (N9) |
| δF = max{0.10F, 2.5e-7} (62) | anchor | 0.1; 2.5e-7 | yes | |
| 100 runs (81-82); 5 methods × 2 spaces × 5 starts × 2 repeats | runs.json | 100 unique keys | yes | |
| Unit 70.94 s (93) | `unit_seconds` (all runs) | 70.935572 | yes | |
| 15 concurrent processes; 1.5N = 22.5 (89, 96) | EXPERIMENTS.md "N = 15" | 15; 22.5 | yes | |
| "median of 45 concurrent cold validations"; load peak 21.1; c7a.4xlarge, OpenBLAS; host anchor agreement within 1e-9 and bit for bit across spaces (88-96) | not in the package | n/a | unverified | Session records are not in the paper package. The refinement-host θ0 primary F is bit-identical across spaces (consistent) |
| No recovery, deferral or contamination (94) | `host_contaminated` | 0/100 | yes | |
| In-run cost 109 CPU-hours (96-97) | Σ `in_run_wall_seconds` | 109.09 h | yes | Single-threaded processes, so wall-hours equal CPU-hours |
| Space B: 8 + 10 variables (42) | refinement.json `x` length (space B) | 18 | yes | 55 control points per harness/README.md l.262 |

## Results (§7.2)

| Text claim (line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| Space A: no run below F/F0 = 0.043 (103) | min credited F over all space-A runs | 0.043196 | yes | |
| Best of A, B, D, E reach 0.043F0; C 0.049F0 (104-105) | per-method min over both repeats | 0.0432 (A, B, D), 0.0433-0.0434 (E); C 0.0489 | yes | |
| LHS 4: A, B ~0.09F0; D 0.044F0; C and E no credited validation in either repeat (108-110) | repeat-1 best | 0.0906, 0.0904, 0.0445 (rounds to 0.044); no-credit files = E and C, a, lhs4, r1 and r2 | yes | |
| Space B best 0.004-0.009F0 (110) | representatives | 0.0044 (D) to 0.0085 (B) | yes | |
| Every method except C reaches T99 from ≥4 of 5 starts (111-112) | repeat 1 | A 5, B 4, D 5, E 4, C 1 | yes | |
| 111 crossings attained in both repeats (127) | | 111 in r1, 111 in r2, 111 paired | yes | |
| Median repeat difference 2.1% (128) | | \|t2/t1−1\| median 2.13%; \|ln\| median 2.11% | yes | |
| **Largest repeat difference 13.2% (128)** | | \|ln(t2/t1)\| max = 0.1319. As a percentage: \|t2/t1−1\| = **12.36%**; relative to the faster run, **14.10%** (D, space B, LHS 3, T99: 83.30 vs 73.00 units) | **no** | Critical C2. The verifier computes `abs(log(...))` and the text reports it as a percentage |
| Six crossings after the 60-unit checkpoint, all T99, space B, B or D (128-130) | checkpoint_60 vs attainment | 6 across both repeats (3 in repeat 1: B θ0 90.4, B LHS4 62.3, D LHS3 83.3; 3 in repeat 2: B θ0 90.9, B LHS1 60.1, D LHS3 73.0) | yes (both repeats) | The paragraph has just said "Repeat-1 times are reported" (N3) |
| B/A geometric mean 3.1-3.3 (A), 5.1-7.8 (B) (136-137) | Table 6 regenerated | 3.098-3.255; 5.058-7.787 | yes | |
| D/E 2.4-2.8 (A), 3.1-7.2 (B) (137-138) | | 2.445-2.754; 3.143-7.204 | yes | |
| 45 finite optimizer-contrast pairs, all favor SLSQP, 1.46-21.5 (138-139) | | 45 (B/A 24, D/E 21); min 1.4647, max 21.537; all > 1 | yes | |
| E/A 0.62-0.79; D/B 0.45-0.56 (140-141) | | 0.620-0.794; 0.445-0.562 | yes | |
| D/A 1.68-1.83 (A), 2.25-4.32 (B); 25 pairs, one tie at 0.99 (space-A LHS 2, T90) (141-144) | | 1.675-1.828; 2.253-4.315; 25 pairs; one ≤1: 0.9925 at a/T90/lhs2 | yes | |
| "In both spaces and at every attained target, SLSQP reaches the target sooner than COBYQA at the same resolution" (134-136) | Table 5 | true for every paired cell. In 4 repeat-1 cells COBYQA-L0 (D) attains and SLSQP-L0 (E) does not: a/T50/LHS4, a/T90/LHS4, b/T90/LHS1, b/T99/LHS1 | partial | Correct only when read as "whenever both attain" (N1). Disclosed at l.146-149 |
| E: no credited validation from space-A LHS 4; space-B LHS 1 stalls at 0.28F0, missing T90 and T99 (147-148) | | best 0.2789; T50 attained at 1.12 units | yes | |
| A attains every target that any method reaches, from every start (149) | | 0 misses | yes | |
| C: T99 once in space B; T90 from 2 of 5 space-A starts (150) | | 1; 2 | yes | |

## Final refinement (§7.2, Table 7)

| Text claim (line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| 33 designs; six checks; 15 conforming; 213 checks, all complete (179-185) | refinement.json | 33; 6 each + 15 = 213; all ok, all complete, error null | yes | The 15 zero-offset designs are the 13 space-A candidates plus both θ0 |
| 16 moved-mesh designs reproduce within 2.1e-12 relative (188-190) | primary F vs study F_val | n = 16, max **2.113e-12** | display yes; bound no | Exceeds "within 2.1e-12" at the third digit (N4) |
| 15 fresh-mesh designs differ by at most 1.7e-3 (190-192) | | n = 15, max 1.6852e-3 | yes | |
| L3 changes F by −4.0% to +6.3%; largest is D's best in space B, 0.0044 → 0.0047 F0 (196-198) | | −4.028% (C T50, a) to +6.334% (best D, b); 0.004394 → 0.004672 | yes | D's best is also the overall lowest-F design at L2 and at L3 |
| 40-angle and shifted grids ≤ 0.11% (199-200) | | max \|·\| 0.1065% (shifted), 0.0534% (g40) | yes | |
| Tight Newton tolerance ≤ 1e-5 (200) | | max \|·\| **1.005e-5** | display yes; bound no | N4 |
| Conforming 1.1-4.4% more on space-A designs (201-202) | | 1.064% to 4.397% (13 candidates) | yes | |
| **"and 2.0% more at θ0" (202)** | Table 7: θ0 space A **+2.1**, space B +2.0 | 2.070% (a), 2.012% (b) | **no** | Critical C3. The space-A θ0 row of Table 7 says +2.1 |
| All 25 crossings satisfy their target at L3; smallest margin 1.2% of target (203-204) | | 25; min 1.205% (D T99, b); all > 0 | yes | Margins under g40, shifted and tight are all ≥ 1.63% |
| Every candidate meets the mean floor in all five training-current checks, closest by 0.012% (205-206) | | min 0.01208% (best E, b, at L3); none below | yes | The conforming check is not one of the five; its minimum is 0.055% |
| Held-out ratio 0.65-1.73 (211-213) | (F_w/F_w,θ0)/(F/F0) | 0.6451 (E T90, b) to 1.7275 (best D, b) | yes | Wording inverted: >1 means a *weaker* reduction (N5) |
| Space A within 10% for all five best designs (213) | | 1.071-1.093 | yes | |
| "In space B, the best designs lose more" (213-214) | | best A 1.23, C 1.29, D 1.73, E 1.23; best B **0.89** | partial | 4 of 5 (N5) |
| D's best: 0.0044F0 at training, 0.0076 at held-out (214-216) | | 0.004394; 0.00759 | yes | |

## Abstract, introduction, §6.4, discussion and conclusion (changed text)

| Text claim (main.tex line) | Source | Source value | Match | Notes |
|---|---|---|---|---|
| 100 runs, common order-93 validation (39-40) | | 100 | yes | |
| **"in all 45 paired starts" (41)** | Table 6 | 45 paired (space, target, start) comparisons, drawn from 5 starts per space (20 start-space-contrast pairings) | **no** | Critical C1. The body (optimization.tex l.85-86, main.tex l.791, 828) says "five starts"; the conclusion (825) correctly says "45 paired cases" |
| Geometric means 2.4-7.8 (42) | | 2.445-7.787 | yes | |
| Coarser mesh saving 1.3-2.2 times (42-43) | 1/GM(E/A, D/B) | 1.260-2.245 | yes | |
| "with SLSQP, lost one start in each design space" (43) | | space A LHS 4: no credit; space B LHS 1: attains T50 at 1.1 units, misses T90/T99 | partial | Overstated for space B (N2) |
| Every crossing met its target on a finer mesh; −4.0% to +6.3% (44-45) | | see above | yes | |
| §6.4: "Section 7 measures optimizer iterations, surrogate fitting and time to a validated target directly" (755-756) | §7 | iteration counts are charged but never reported | partial | N6 |
| Discussion: plateau near 0.043F0 (793) | | 0.0432 | yes | Interpretation overreaches (N8) |
| Conclusion: all 45 paired cases (825) | | 45 | yes | |
| Conclusion: refinement changed the objective "by at most 6.3%" (828) | | 6.334% | display yes; bound no | N4 |
| Availability: 213 checks, 5,453 files (856-858) | | 213; 5,453 | yes | |

## Figures (informational)

- `exp2-quality.pdf`: the renderer plots repeat 1, the cumulative best credited `F_upper/F0`, and a tick at `min(in_run/unit, 120)`. This matches the caption. Missing curves are the four no-credit runs, of which two (C and E, a, LHS 4) are in repeat 1.
- `exp2-time-to-target.pdf`: 4 panels (a T50, a T90, b T90, b T99). **Space-B T50 is omitted**, but the text (l.125-126) says Table 5 and the figure give the time "to each target". The caption names neither the panels nor the omission (N7). Unattained cells are plotted as crosses at x = 130, beyond the 120-unit line, as captioned.
- Stale figures: none. Byte-identical re-render; the inherited assets are byte-identical to v6.
