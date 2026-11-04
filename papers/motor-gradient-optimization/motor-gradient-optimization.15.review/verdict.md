# motor-gradient-optimization.15 — review verdict

These are fresh, independent re-scores of v15 at `c3263532`; the v14 scores do not carry over. The full reports are archived verbatim beside this index. Both reviewers built and verified in scratch copies and recomputed every new v15 number from the hash-bound extracts.

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **41/44** | true | none |
| Fable | **40/44** | true | none |

**Both reviewers are at or above the operator's 40/44 target.** `[PENDING zenodo-doi]` is an operator dependency: it holds READY and is not scored.

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 5/6 | 5/6 |
| 3 | Clarity of contribution | 5/5 | 5/5 |
| 4 | Related work | 5/5 | 4/5 |
| 5 | Reproducibility | 5/5 | 5/5 |
| 6 | Figures & tables | 4/4 | 4/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 5/5 | 5/5 |
| 9 | Rhetorical economy | 4/4 | 4/4 |

## Required findings (Opus only; Fable has none)

1. **Cost-check residual (R1).** At main.tex L783–789 and optimization.tex L330, the cost-check residual is attributed to A′'s overhead alone. The FD arms' to-crossing costs also deviate from the whole-run model (+6.5/+7% in A, −5/+1.5% in B). Say the model omits per-evaluation overhead for both arms, and replace "accounts for" with "discusses".
2. **Statistic scopes (R2).** The cold space-B factor 4.10 is over 4 starts (the abstract's 4.11 is over 5); say so. The "1.2% median" pools F and A′; F alone is 1.0%.

## Advisories

- The space-B offset −3.11e-3 is missing its unit.
- The SW3 slope's sign is inconsistent between §5 and App. D.
- "That advantage" in the Conclusion has an ambiguous antecedent.
- Space-A SW3 under F_w changes sign at h = 1e-7 (clarify "as for F" as frozen-step only).
- Give the per-space split of zero-update stencils.
- The ten-number sentence in §5 Gradient cost.
- The changelog's page and word counts (38 pages, 241 words).
- BRIEF.md and EXPERIMENTS.md still describe v6.
- The package is only on a branch; merge or deposit it before release.
- Related-work leads: adjoint-FE frameworks; Nikolova/Bakr measured EM adjoint-vs-FD timings.
