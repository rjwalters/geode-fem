# motor-gradient-optimization.12: review verdict

These are fresh, independent re-scores of v12 at `e7caaa43`; the v11 scores do not carry over. The full reports are archived verbatim beside this index. Both reviewers worked in scratch copies.

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **38/44** | true | none |
| Fable | **37/44** | true | none |

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 5/6 | 5/6 |
| 3 | Clarity of contribution | 5/5 | 5/5 |
| 4 | Related work | 4/5 | 4/5 |
| 5 | Reproducibility | 4/5 | 4/5 |
| 6 | Figures & tables | 4/4 | 3/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 5/5 | 5/5 |
| 9 | Rhetorical economy | 3/4 | 3/4 |

Both reviewers find all seven of the merged v11 required findings resolved. Fable independently reproduces every ablation number from the hash-bound extracts.

## Required findings

1. **Package pointer (both).** main.tex L455–456 names `motor-gradient-optimization.11`. It should name `.12` (or its successor), bound in the verifier.
2. **SW3 "noise floor" reading (Opus R2, Fable A4).** At all four L1 points, each component's FD−adjoint error has a fixed sign and repeats to about two digits, and the error size differs by component; the same pattern appears at L0. That fits a deterministic one-sided-difference (truncation) offset, not noise. Reword, or measure it with a central difference / step sweep at the failing point.
3. **Undefined "step" cost (Opus R3, Fable R2).** The 1.53/3.32 units per "F step" divides by all training evaluations. State the unit, or report cost per gradient. The 10.9 ratio cannot be reproduced from the printed 3.32/0.31.

## Main advisories

- Show the per-start ratio in Fig. 5.
- Related-work leads, to be verified: Lyu/Xu/Martins 2014, Lei et al. 2017, Peter & Dwight 2010, Moré & Wild 2011/2012 (FD step vs computational noise), Kenway et al. 2019.
- Explain F's 8 of 10 budget stops in space A.
- Say "took 2.5 and 4.1 times as long", not "times slower".
- "Essentially the same path" holds only up to each target crossing.
- Reconcile "n-fold compute" with the measured ratios.
- Introduction paragraph 2 restates the abstract.
- Title: consider stating the per-core condition.
- Stale `_progress.json` note; the README reports 123 input hashes where the verifier reports 125.
