# motor-gradient-optimization.13: review verdict

Fresh, independent re-scores of v13 at `6e4544fc`; the v12 scores do not carry over. The full reports are archived verbatim beside this index. Both reviewers worked in scratch copies. Fable also unpacked the staged raw-trace archive and verified 2118/2118 ablation files against the inventory.

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **39/44** | true | none |
| Fable | **40/44** | true | none |

One `[PENDING zenodo-doi]` marker remains. It is an operator dependency and holds READY, but is not scored.

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 4/6 | 5/6 |
| 3 | Clarity of contribution | 5/5 | 5/5 |
| 4 | Related work | 4/5 | 4/5 |
| 5 | Reproducibility | 5/5 | 5/5 |
| 6 | Figures & tables | 4/4 | 4/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 5/5 | 5/5 |
| 9 | Rhetorical economy | 4/4 | 4/4 |

Both reviewers find all three v12 required findings resolved.

## Required findings

1. **Cold-start finite-difference stencils (Opus R2, Fable R1).** All 40 traces record `warm: Cold`. A stencil solve costs about the same as a training solve (0.236 vs 0.237 units). Appendix H's own timings put warm-started stencils at about 0.33–0.42 of a cold forward. So F is not the strongest finite-difference competitor, and the 2.5/4.1 ratios would fall against a warm-started one. The minimum fix is to disclose this. The stronger fix is to measure it.
2. **"Same path" (Opus R1).** optimization.tex L300–302 is wrong at space-B LHS 2 and LHS 4: the validated objectives differ by 29% and 17.5% at or just before T99, and LHS 2 uses 9 vs 10 training evaluations. What holds is that the crossing comes after the same number of validations.
3. **Per-gradient cost ratio (Opus R3, Fable R2).** The per-gradient ratio is never printed (33 and 70 from the printed numbers). The main reason the no-validation ratios (5.58 and 10.07) sit below it is the training solve both arms pay on every evaluation (33 → 7.4 → 5.6), not the gradient-request share. Name both factors.

## Advisories

- Lyu/Xu/Martins 2014 and Yu et al. 2018 (DOI 10.1016/j.ast.2018.01.016) as related-work leads.
- "Upper bound on the noise contribution" is unproven.
- The Appendix D slope and curvature values lack units.
- The censored marker in Fig. 5 needs a caption note, and the LHS 4 label is misplaced.
- Split the data statement.
- Pin `.13` to a commit or to the Zenodo deposit.
