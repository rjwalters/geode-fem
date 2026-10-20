# Numerical audit: motor-gradient-optimization.9

Scope: every numerical value added or changed between v8 and v9, plus the values in the v8 C1 sentence. Every other number is identical to v8. `refs.bib`, `figures/`, `tables/` and `figures/src/data/exp2/` are byte-identical, and the v8 audit re-derived all Section 7 numbers from the raw data. That recomputation was repeated here. `auditor-scripts/x9.py` is the v8 auditor script `x8.py`, unmodified apart from its header comment. It does not import `verify_exp2.py`. Run against v9, its output `x9.out` matches v8's `x8.out` line for line. `x9fig8.py` adds the repeat-1 per-method Fig. 8 minima.

| Text claim (location) | Source | Source value (raw) | Match | Notes |
|---|---|---|---|---|
| "by geometric means of 2.45--7.79" (abstract, main.tex:21) | Table 6; runs.json | optimizer gm min 2.4454 (D/E A T90), max 7.7866 (B/A B T99) | yes | N3 fix; verifier-bound |
| "gets there 2.45--7.79 times sooner" (intro, main.tex:79) | same | same | yes | |
| B/A "3.10--3.25" space A (optimization.tex:171) | Table 6 | 3.0984, 3.2545 | yes | |
| B/A "5.06--7.79" space B | Table 6 | 5.0585, 7.7866 | yes | |
| D/E "2.45--2.75" space A | Table 6 | 2.4454, 2.7538 | yes | |
| D/E "3.14--7.20" space B | Table 6 | 3.1433, 7.2036 | yes | |
| "near tie (0.99 ... D is 0.75% faster" (optimization.tex:177-178) | Table 6 D/A A T90 LHS2 | 0.9925 → 0.75% | yes | Only D/A ratio ≤ 1 of 25 |
| "within the timing noise between repeats" | §7.2 repeat noise | median 2.131%, max 12.360% | yes | 0.75% < median |
| "four of the five reach the same 0.043F0 (C reaches 0.049F0)" (Discussion, main.tex:802-803) | Table 7; runs.json | A .04321, B .04320, D .04321, E .04332; C .04893 | yes | v8 C1 resolved |
| "every method's best design sits on the active mean-torque floor" | runs.json | mean/floor − 1 = 0.12-0.16% for all five | yes | "within 0.17%" elsewhere |
| "in Fig. 8 ... the level appears near 0.048" (optimization.tex:138-139) | Fig. 8 data (F+δF)/F0, repeat 1 | A .04753, B .04752, D .04753, E .04765; C .05382 | yes | See flags A1 (antecedent) |
| "remaining objective fraction ... 1.23--1.73 times" (optimization.tex:269-270) | refinement.json, representative/{A,C,D,E}/b | 1.2325, 1.2900, 1.7275, 1.2260 | yes | N5 fix |
| "for B's best design it is 0.89" | representative/B/b | 0.8922 | yes | |
| "is 1.07--1.09" space A (unchanged) | representative/*/a | 1.0713-1.0927 | yes | Verifier now asserts min == 1.07 exactly |
| "multiplies ... by 3.6 under fixed unscaled current" (abstract, main.tex:35) | main.tex:504-507 | 1.07e-6 → 3.86e-6 (×3.6) | yes | Qualifier now matches body |
| "reverses three mean-torque sensitivities at the initial design on the second-coarsest mesh, at order 45 and full current" (abstract, main.tex:42-44) | main.tex:624-633; L0-L3 at 306 | SW1, SW4, SD1 at L1 | yes | L1 is the second-coarsest mesh |
| Table 6 two-decimal gms vs text | tables/exp2-ratios.tex | 3.25/3.10, 2.75/2.45, 5.06/7.79, 3.14/7.20 | yes | Text and table now use the same precision |

## Verifier

`python3 verify.py` (runs verify_exp2.py): all seven PASS blocks. The v9 text-binding list adds `by geometric means of 2.45--7.79`, `gets there 2.45--7.79 times sooner`, `C reaches $0.049F_0$`, `is 1.23--1.73 times that at the training currents`, `D is 0.75\% faster`, `the level appears near $0.048$` and `wherever both reach a common validated target`. It forbids `near $0.043F_0$, so`. The no-op `replace` is gone, so every phrase check is strict. The Fig. 8 assertion takes its minimum over both repeats, not repeat 1 alone, but both round to 0.048 (flags A3).

## Figure source-of-truth

`figures/` is byte-identical to v8, and no figure script changed. No stale figures.

## Mismatches

None.
