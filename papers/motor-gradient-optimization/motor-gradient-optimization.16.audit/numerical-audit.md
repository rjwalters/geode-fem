# Numerical audit: motor-gradient-optimization.16

## Method

Every check ran in a scratch copy named `motor-gradient-optimization.16`. The verifiers bind to that directory name and to the thread-root `.anvil.json`. The version directory itself was not written. Five layers:

1. **Package verifiers, extract mode.**
   - `verify.py`: 20 PASS, including the three chained verifiers.
   - `verify_exp2.py`: 4 PASS.
   - `verify_exp2b.py`: 6 PASS.
   - `verify_exp2c.py`: 6 PASS.
   - Outputs: `auditor-scripts/verify*.out`.
2. **Raw-trace mode.** The staged archive `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/motor-paper-raw-traces.tar.zst` was used:
   - Its SHA-256 is `a70a0699…203f12` and it is 2,609,375,627 bytes, equal to the paper (main:500-501), the BRIEF and `deposit-record.json`.
   - `MOTOR_RAW_ARCHIVE` mode passes: 7 PASS (`verify_exp2b.archive.out`).
   - Unpacked, all 8,751 files match their three inventories (5,453 + 2,118 + 1,180, 0 mismatches). The three inventory digests equal the package copies (`0650f4ee…`, `c2bc65ff…`, `df509322…`).
   - With `EXP2B_SESSION`/`EXP2C_SESSION` set, both evaluation timelines re-derive exactly: 8 PASS (`verify_exp2c.raw-sessions.out`).
   - The 40 F-session and 20 F_w-session run extracts re-derive exactly from the raw traces through the package's own `run_record` (`rederive.out`).
   - The main study's `supplement.json` re-derives byte-identically from `exp2-main/main` with `extract_exp2_supplement.py`.
   - All 100 main-run trace hashes match `runs.json`.
3. **Independent recomputation.**
   - `auditor-scripts/x16.py` recomputes target credit from the raw validation rows: `F_val + max(0.1F_val, 2.5e-7) ≤ T`, admissible, and mean ≥ floor. It does not read the extracts' stored attainment flags. 0 disagreements over 100×3 main-study cells.
   - From that credit it rebuilds every time-to-target cell, ratio and geometric mean, the repeat noise, the stop reasons, and the ablation and F_w tables and decompositions.
   - `x16cost.py` recomputes the validation charges and gradient costs.
4. **Table and figure regeneration.**
   - `render_exp2_validation.py`, `render_exp2b.py` and `render_exp2c.py` (stdlib), plus `render_exp2.py` and `render_exp2b_figure.py` (pinned numpy 2.5.4 and matplotlib 3.11.2), were rerun in the scratch copy.
   - Every table in `tables/` is byte-identical to the committed copy, and so are `exp2-quality.pdf`, `exp2-time-to-target.pdf` and `exp2b-per-start.{pdf,png}`.
   - Every other figure, and `cost/gradient-cost/torque/exp2-ratios/exp2-refinement/exp2-time-to-target.tex`, is byte-identical to the PASS-audited v9. `coupling.tex` and `design-points.tex` differ from v9 only in `-` → `$-$` typography.
5. **Coverage scan** (`cover.py`).
   - Every numeric token in the body that no verifier contains literally was compared with v9.
   - Only five such tokens are new since the v9 PASS audit: Gmsh 4.15.2, the archive size, and 0.887/0.894. Each was checked here: Gmsh against the pinned README; the size against the archive; 0.887 and 0.894 by recomputation (L1 0.8867, L0 0.8939 from `supplement.json`).
   - The forward-model numbers carried from v9 rely on the v7–v9 audits and on `verify.py`'s 20 recomputation blocks.

## Results

| Text claim | Source (Tab/Fig/data) | Source value (recomputed) | Match | Notes |
|---|---|---|---|---|
| Title/abstract/intro/§5/§7: F_w/A′ 1.71 and 2.70 "from every start" | Tab. exp2c-ratios; exp2c runs | GM 1.71 (1.68–1.77), 2.70 (2.62–2.79); every per-start ratio > 1 | yes | |
| Abstract/§5: F/A′ 2.47 and 4.11; repeat 2 2.49 and 4.04 | Tab. exp2b-ratios | 2.47 (2.45–2.52), 4.11 (3.84–4.33); rep. 2 2.49, 4.04 | yes | |
| Abstract: B/A 3.10 and 7.79 at the primary targets | Tab. exp2-ratios | 3.10 (A T90), 7.79 (B T99, 4 finite pairs) | yes | |
| All 45 optimizer-contrast pairs favor SLSQP, ratios 1.46–21.5 | Tab. exp2-ratios | 45 finite pairs, all > 1, 1.46–21.54 | yes | |
| B/A 3.10–3.25 / 5.06–7.79; D/E 2.45–2.75 / 3.14–7.20; E/A 0.62–0.79; D/B 0.45–0.56; D/A 1.68–1.83 / 2.25–4.32 | Tab. exp2-ratios | identical, every cell | yes | Every per-start entry of Tabs. exp2-time-to-target and exp2-ratios recomputed. |
| D/A near tie 0.99 (A LHS 2, T90) | Tab. exp2-ratios | 0.99 | yes | |
| Repeats: 111 crossings in both repeats, median 2.1%, max 12.4% | main runs | 111; 2.13%; 12.36% | yes | |
| Three repeat-1 crossings after 60 units (two by B, one by D, all B T99) | main runs | B lhs4 62.3, B θ0 90.4, D lhs3 83.3 | yes | |
| One validation costs a median 0.87 units over 3,350 (0.55–1.10); validation 53% of charged time, 2–90% per run | supplement | 3,350; 0.873; 0.55–1.10; 52.6%; 2.3–89.9% | yes | |
| Stops: 38 at T99, 46 converged, 13 budget, 3 solver; load peak 21.1; 109 CPU-hours | main runs | 38/46/13/3; 21.09; 109.09 h | yes | |
| Space-A best designs: A, B, D, E at 0.043F₀, C at 0.049F₀ | main runs; Tab. exp2-refinement | 0.0432/0.0432/0.0432/0.0433 (L2 recheck 0.0434)/0.0489 | yes | E rounds to 0.043 either way. |
| Space-B best 0.004–0.009F₀; every method except C reaches T99 from ≥4 of 5 starts | Tab. exp2-time-to-target | 0.0044–0.0085; A 5, B 4, D 5, E 4, C 1 | yes | |
| D reaches a target E misses in four cells; A reaches every target any method reaches | Tab. exp2-time-to-target | A-T50/T90 LHS 4, B-T90/T99 LHS 1; yes | yes | |
| Training/validation at θ₀: 0.887 (L1), 0.894 (L0) | supplement `theta0_train_F` / F₀ | 0.8867, 0.8939 | yes | New since v9. |
| Ablation: no censored cell; both FD arms cross each primary at the same validation as A′ | exp2b/exp2c runs | 0 censored; identical indices except F_w B-T50 LHS 3 (2 → 8) | yes | Exception stated in text (main:927-931). |
| F_w B LHS 3: third validated objective 0.46F₀ misses T50 with the allowance; next five below the mean floor; crosses six later; ratio 6.92 | exp2c runs | 0.459 (upper 0.505); 5 × mean below floor; index 8 vs 2; 6.92 | yes | |
| F repeats agreed to a median 1.0% per primary crossing | exp2b runs | 1.02% (0.2–4.4%) | yes | |
| B/F_w per start 1.83, 2.07, 1.01, 1.38, 3.23 (GM 1.76) and 6.11, 3.35, 1.94, 1.82 (2.92); seconds 1.81, about 3.0 | cross-session | identical; seconds 1.81, 2.99 | yes | |
| Chain (F_w/A′)(B/F_w) 3.02 and 7.82 against 3.10 and 7.79; F_w/A′ over B's starts 1.71 and 2.68 | cross-session | 3.02, 7.82; 1.71, 2.68 | yes | |
| Cold chain: 2.47 and 1.2 (A); 4.10 and 1.9 (B, four starts) | cross-session | F/A′ 2.4745, direct B/F 1.22; 4.0996, 1.91 | yes | The quotient B/A ÷ F/A′ is 1.25 in space A. "1.2" is the directly measured B/F, consistent with how B/F_w is reported. |
| B/F 0.70 and 0.95 at two space-A starts; B/F_w ≥ 1.01 | cross-session | 0.70 (LHS 2), 0.95 (LHS 3); min 1.01 | yes | |
| F_w takes 0.72 and 0.68 of F's time in seconds | cross-session | 0.718, 0.679 | yes | |
| Adjoint gradient 0.057/0.061 (F session), 0.054/0.059 (F_w session); F 1.90/4.25 (~0.24 per stencil; 33×, 70×); F_w 0.97/2.36 (18×, 40×) | exp2b/exp2c runs | 0.0566/0.0607, 0.0540/0.0589; 1.895/4.251 (0.237); 33.5, 70.0; 0.969/2.365; 17.9, 40.2 | yes | |
| Warm stencil 0.117/0.126 units against base 0.220/0.233; 263 gradients; 1,810 + 648 stencils | exp2c runs | 0.117/0.126; 0.220/0.233; 227 + 36 = 263; 1,810, 648 | yes | |
| Training solve "medians of 0.240 and 0.236 units for F against 0.232 and 0.245 for its A′, and 0.220 and 0.233 for F_w against 0.220 and 0.238" (optimization:321-323) | exp2b/exp2c runs | F 0.240/0.236 and both A′ arms are run-medians of (forward + line search)/solves. F_w 0.220/0.233 are per-solve base-charge medians. On F_w's definition its A′ gives 0.216/0.233. | values match their definitions | **Mixed statistics in one comparison**: non-critical note N3. No conclusion changes. |
| 8% of F components took the backward step; none refused | exp2b runs | 359/4,283 = 8.4%; 0 refused | yes | |
| A′ primary crossings 0.94–1.04 of main-study A ("Control", main:811-814) | cross-session | F-session A′: 0.944–1.043; F_w-session A′: 0.973–1.052 | yes for the F session | Session unnamed: non-critical note N4. |
| F_w session's A′ within 4% of F session's A′ | cross-session | 0.960–1.032 | yes | |
| Paths: within 0.4% (space A), within 0.9% (three space-B starts); "29% at the crossing validation and 18% one validation before it" | exp2b runs | 0.36%; 0.37/0.26/0.83%; 29% is LHS 2 at its crossing, 18% is LHS 4 one validation before its crossing | yes | Each figure belongs to a different start: non-critical note N5. |
| F's best validated objective within 0.4% of A′'s from every space-A start | exp2b runs | max 0.32% | yes | |
| Run outcomes: F space A 2 converged, 8 budget; A′ 4/4/2; F_w space A 4 budget, 1 converged; its A′ 2/2/1; space B all T99 | exp2b/exp2c runs | identical | yes | |
| Units 66.66 / 69.09 / 70.94 s; loads 19.7 / 17.4 | runs | 66.659, 69.088, 70.936; 19.73, 17.37 | yes | Fig. exp2b caption "3–6%": 2.7–6.4%, rounds to 3–6. |
| Ablation final checks: 90 checks / 14 designs; L3 −2.9% to +2.6% (crossings −2.9% to +1.1%); 40-angle/tight/shifted < 0.03%; conforming +1.1% to +4.4% | Tab. exp2b-refinement; refinement.json | identical; max 2.997e-4 | yes | |
| F_w final checks: 84 / 13; ≤ 0.03% | exp2c refinement.json | max 3.02e-4 (rounds to 0.03%) | yes | |
| Main final checks: 213 checks / 33 designs; L3 −4.0% to +6.3% (crossings −4.0% to +1.1%); angular ≤ 0.11%; tight < 1.1e-5 | Tab. exp2-refinement | identical (max shifted 10.6e-4) | yes | Unchanged since v9. |
| Archive: 2,609,375,627 bytes; SHA-256 a70a0699…203f12; 5,453 / 2,118 / 1,180 files | staged archive; deposit-record.json | identical; 8,751 files verified | yes | |

## Figure source-of-truth check

- Every `\includegraphics` target exists. No render script is newer than its rendered figure.
- The Exp 2 figures and every table re-render byte-identically (see Method §4).
- Visual checks:
  - Fig. exp2b: the ratio rows equal Tabs. exp2b-ratios and exp2c-ratios, and B's LHS 3 cross sits at 120 units ≈ 142 min.
  - Fig. exp2-quality: the 60/120-unit lines and the missing E/C curves at space-A LHS 4 match the text.
  - Fig. exp2-time-to-target: the crosses match the dashes in Tab. exp2-time-to-target.
- Stale figures: **0**.

## Numerical inconsistencies

**0.** Every checked number matches its source. Notes N3–N5 in `flags.md` concern presentation: a mixed statistic, an unnamed session and an unassigned pair of values. They are not mismatches.
