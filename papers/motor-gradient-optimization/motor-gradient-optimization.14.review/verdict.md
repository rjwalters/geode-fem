# motor-gradient-optimization.14 — review verdict

These are fresh, independent re-scores of v14 at `41b628cf`; the v13 scores do not carry over. The full reports are archived verbatim beside this index. Both reviewers worked in scratch copies and re-hashed the staged three-session archive (`a70a0699…`).

| Reviewer | Total | Advance | Critical flags |
|---|---|---|---|
| Opus | **39/44** | true | none |
| Fable | **38/44** | true | none |

`[PENDING zenodo-doi]` is an operator dependency. It holds READY and is not scored.

| # | Dimension | Opus | Fable |
|---|---|---|---|
| 1 | Rigor | 5/6 | 5/6 |
| 2 | Evidence | 5/6 | 5/6 |
| 3 | Clarity of contribution | 5/5 | 5/5 |
| 4 | Related work | 4/5 | 4/5 |
| 5 | Reproducibility | 4/5 | 4/5 |
| 6 | Figures & tables | 4/4 | 4/4 |
| 7 | Prose & structure | 3/4 | 3/4 |
| 8 | Citation hygiene | 5/5 | 5/5 |
| 9 | Rhetorical economy | 4/4 | 3/4 |

Both reviewers find all three v13 required findings resolved. Fable independently reproduces every F_w number.

## Required findings

1. **Stale package text (Opus R3, Fable R1).**
   - The README names the two-session archive (`2de2132c…`) and the cold-F headline and factors, and says `verify.py` chains only exp2/exp2b.
   - main.tex says "three" verifiers; there are four.
   - `_progress.json` records the phase as completed before it started.
2. **Scope of "comparable factors" (Opus R1).** The split depends on which FD variant sits in the middle: against cold stencils it is 2.47 × ~1.25. Scope the sentence to warm FD and say the split depends on the variant.
3. **Misattributed 1/h sentence (Opus R2).** At optimization.tex L353–358, "grows about tenfold per decade" grammatically refers to the space-A SW3 point, where the error is truncation. The 1/h growth is at the space-B interior point.

## Main advisories

- **Warm-stencil error (Opus A1).** The same signed error (−3.11e-3) appears on three components, which suggests an uncancelled base-solve error rather than an inherent cost of warm starting. Qualify "less accurate" as "at the frozen tolerance".
- **Zero-update stencils (Opus A2).** Some warm stencils take zero Newton updates (e.g. 41 of 225 space-A SW4 gradients). Report the count.
- **Abstract headline (Fable A1).** Say "warm-started" in the headline sentence and drop the no-validation parenthetical. It currently carries five ratio pairs.
- **Two-factor cost check (Fable A2).** It uses whole-run gradient shares to explain a to-crossing ratio. With F's shares it reproduces F_w's 3.23/6.10 as 3.3/7.0.
- **Repeats (Fable A3).** State why F_w has one repeat.
- **Related-work leads.** Semi-analytic/direct sensitivity methods (doi:10.1108/compel-02-2021-0063, doi:10.3390/math10060885); warm-started/continuation FD.
- **Branch (both).** v11–v14 live only on a branch, with no PR to main.
