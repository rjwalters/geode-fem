# motor-gradient-optimization.16: review verdict

Fresh, independent re-scores of v16 (`463c7319`, a text-only revision of v15). The full reports are archived verbatim beside this index. Both reviewers built and verified in scratch copies, recomputed every new v16 number from the hash-bound extracts, and confirmed that the v15→v16 diff is exactly the changelog's list, with no regressions.

| Reviewer | Total | Advance | Critical flags | Required findings |
|---|---|---|---|---|
| Opus | **42/44** | true | none | none |
| Fable | **40/44** | true | none | none |

`[PENDING zenodo-doi]` is an operator dependency: it holds READY and is not scored.

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 6/6 | 5/6 |
| 2 | Evidence | 5/6 | 5/6 |
| 3 | Clarity of contribution | 5/5 | 5/5 |
| 4 | Related work | 5/5 | 4/5 |
| 5 | Reproducibility | 5/5 | 5/5 |
| 6 | Figures & tables | 4/4 | 4/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 5/5 | 5/5 |
| 9 | Rhetorical economy | 4/4 | 4/4 |

The remaining deductions are scope, not defects:
- one motor and five starts;
- a single F_w repeat;
- no reduced-physics competitor;
- the source of the warm-stencil offset is unisolated;
- the method/space letter collision, which the author declined to change.

Advisories:
- the cost-check per-cell residual;
- SW3's eightfold change at h = 1e-7 under F_w;
- F_w's per-evaluation ratio printed beside F's;
- related-work leads (adjoint-FE frameworks; Bakr/Nikolova EM timings);
- stale "within 4%" and "v15" strings in `verify_exp2c.py`;
- two stale BRIEF sentences (fixed at the thread root in the archiving commit).
