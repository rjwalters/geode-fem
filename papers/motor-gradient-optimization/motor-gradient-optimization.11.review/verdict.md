# motor-gradient-optimization.11: review verdict

Fresh, independent re-scores of v11 at `c7d465f6`. The v10 scores do not carry over. Full reports are archived verbatim beside this index. Reviewers built and verified in scratch copies.

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **35/44** | true | none |
| Fable | **36/44** | true | none |

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 5/6 | 5/6 |
| 3 | Clarity of contribution | 4/5 | 4/5 |
| 4 | Related work | 4/5 | 4/5 |
| 5 | Reproducibility | 4/5 | 4/5 |
| 6 | Figures & tables | 3/4 | 3/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 4/5 | 5/5 |
| 9 | Rhetorical economy | 3/4 | 3/4 |

Both reviewers find all seven v10 required findings resolved. Both recompute the ablation headline independently: 2.474 (8 parameters, T90) and 4.113 (18 parameters, T99).

## Required findings (both reviewers, merged)

1. **Serial-FD framing (Opus R4, Fable R1).** F's n stencil solves run one after another in a single-threaded process, so "2.5–4× sooner" is a per-core compute ratio. State this. Give the no-validation ratios (5.58 and 10.07), which match the per-gradient cost ratios (5.3 and 10.7).
2. **Gradient-source labelling (Opus R1).** F has gradients, from finite differences. The ablation varies how the gradient is obtained and what it costs, not "gradient access". In B/A = (B/F)(F/A′), the B/F factor is COBYQA against finite-difference SLSQP.
3. **Per-start optimizer factor (Opus R2, Fable R4).** Report B/F per start: in space A, COBYQA matches or beats finite-difference SLSQP at 2 of 5 starts. Replace the log-share statistic with the factors themselves (1.2–1.3 in A, 1.9–2.0 in B). Fix the 4-start vs 5-start mix in space B.
4. **"Dilutes every ratio" (Opus R3)** is false for A vs D.
5. **Evidence placement (Fable R2).** Move the per-start F/A′ table into §4.3, or add A′/F to a figure.
6. **FD accuracy at L1 (Fable R3).** F's gradient accuracy is checked only at L0, while F ran on L1. Measure it at L1 or say it wasn't measured. Report the ablation stop reasons: 8 of 10 space-A F runs hit the budget, against 2 of 10 for A′.
7. **Abstract and length (Fable R4, both).** The abstract is ~450 words with 29 numbers; target ~250. The paper is 34 pages against a 10–15 page target, and the headline is restated five times.
