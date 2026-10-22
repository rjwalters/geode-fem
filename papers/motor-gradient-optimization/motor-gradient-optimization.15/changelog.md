# Revision 14 → 15

Inputs: `motor-gradient-optimization.14.review/` (`verdict.md`, `review-opus.md` 39/44, `review-fable.md` 38/44; both advance, no critical flags) and the coordinator's v15 instructions of 2026-10-21. V14 and earlier versions are unchanged.

**No earlier number changed.** Every new number is recomputed by `verify_exp2c.py` from hash-bound data. The only new data are two evaluation timelines, `figures/src/data/exp2b/timeline.json` and `exp2c/timeline.json`. They were written by the new `figures/src/extract_timeline.py` from the raw session copies. Each record carries its trace's SHA-256, which the verifier matches to `runs.json` and through it to the session inventories. With `EXP2B_SESSION`/`EXP2C_SESSION` set, both files are re-derived exactly from the raw traces (checked). The 129-entry evidence manifest is unchanged.

**Outstanding dependency (holds READY):** `[PENDING zenodo-doi]`, the operator's deposit of the three-session archive. Resolution: pending; the marker is carried forward and is the only one.

**Results:**
- Build from a clean aux state: 0 overfull boxes, 0 undefined references or citations, no `??`.
- 39 pages. The main text is pp. 1–14 (Conclusion on p. 14); Code and data availability starts on p. 15.
- Abstract: 242 words by the verifier's count.
- Verifiers:
  - `verify.py`: exit 0, 20 PASS blocks (was 18).
  - `verify_exp2.py`: 4.
  - `verify_exp2b.py`: 6 (7 with `MOTOR_RAW_ARCHIVE`, which re-hashes the staged archive).
  - `verify_exp2c.py`: 6 (8 with both raw session copies).
- Lints:
  - `numeric_consistency`: pass (the one known suppressed info finding).
  - `audience_check`: pass, 0 hits.
  - `pending_marker`: `zenodo-doi` only.
- Citations: 47 cited = 47 defined.

| Source | Finding | Resolution |
|---|---|---|
| v14.review required 1 (Opus R3, Fable R1) | README stale: two-session archive, `verify.py` chaining two verifiers, cold-F headline and v13 factors, "strongest" | README rewritten from the data statement and `deposit-record.json`. It now gives: the three-session archive (2,609,375,627 bytes, `a70a0699…203f12`, 5,453/2,118/1,180 files); `verify.py` chaining `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py` (four verifiers, one entry point); the warm-FD headline (1.71/2.70; 3.23/6.10 without validation) with cold F as the weaker baseline; COBYQA's about 1.8/2.9 against warm FD; and the cold split. "Strongest" removed. The state line names v15 |
| v14.review required 1 | main.tex says "three" verifiers | Now "four standard-library verifiers (one entry point that runs the other three)" |
| v14.review required 1 | `_progress.json` completed before started | v15 records `started` when the phase began and `completed` at the end. The verifier asserts started ≤ completed and the iteration numbers |
| v14.review required 1 (Opus R3.4) | Bind the package text in a verifier | New `verify_exp2c.py` block. It asserts: README size, digest and member counts equal `deposit-record.json` and the paper; no stale digest, size, "both studies", "strongest" or two-verifier sentence; the README names the four verifiers and `verify.py` actually chains the three; the README carries the paper's headline, cold numbers, warm factors, split and the pending marker; `.anvil.json` `cwd` equals this directory; `_progress.json` is consistent. Mutation-tested: a stale digest, an altered headline or "three standard-library verifiers" each fail the verifier |
| v14.review required 2 (Opus R1) | "Comparable factors" holds only against warm FD | Scoped in all four places plus §5. Abstract: "against that baseline … comparable factors …; the split depends on the finite-difference variant". Introduction: "against cold stencils it is 2.47 for the gradient and 1.2 for the optimizer with eight parameters". §7: "Against cold stencils the split is 2.47 and 1.2 in space A and 4.10 and 1.9 in space B", and a cheaper (direct or semi-analytic) gradient would move more of it to the optimizer. Conclusion: comparable against the warm baseline; against the cold one the adjoint contributes more. §5 "The optimizer factor" gives the cold-chain factors. These come from the existing measured cold chain (F/A′ 2.47/4.10 and B/F 1.22/1.91 on matched starts), already asserted in `verify_exp2b.py`; the new sentences are bound there |
| v14.review required 3 (Opus R2) | 1/h sentence misattributed to the space-A SW3 point | Rewritten. At the space-A interior point, SW3 of the mean gradient is truncation as for F: forward error per unit step −1.7×10³ against F's −1.9×10³ N m/m², recomputed from both sweeps. The tenfold-per-decade growth and central-difference removal are attributed explicitly to "the space-B interior point". The verifier asserts "There the forward error" is absent |
| Opus A1 / coordinator | Qualify warm accuracy; common signed error; no unmeasured mechanism | Abstract and §5: "at the frozen Newton tolerance, … less accurate". §5 states the observation: the three failing space-B components share one signed forward error, −3.11×10⁻³ at 10⁻⁶ m to within 0.3%, so the stencil objectives differ from the base value by a common offset of about −3×10⁻⁹ N² m², "whichever component is perturbed; we did not isolate its source". The v14 mechanism clause ("consistent with a warm stencil stopping after one Newton update") is removed, and the verifier asserts its absence |
| Opus A2 | Count the zero-update warm stencils | App. D: 58 of space A's 1,810 and 20 of space B's 648 warm stencil evaluations recorded fewer than 60 Newton iterations (as few as 40), so at least one field solve took no update. All of them perturb SW1, SW4 or Sr1, the three objective-gradient components that fail the L1 check (stated as an observation). The two budget-truncated last gradients have no such solves; the verifier checks this |
| Fable A1, Opus A6 | Abstract: "warm-started" in the headline; fewer ratio pairs | Headline sentence: "… 1.71 and 2.70 times sooner, from every start, than warm-started finite differences, and 2.47 and 4.11 times sooner than cold-started ones". Dropped from the abstract: the no-validation parenthetical (in §5 and README), the 2.45–3.25/3.14–7.79 ranges (replaced by the one primary-target B/A pair 3.10/7.79, which the chain uses) and "approximate across sessions". The abstract goes from 244 to 242 words while carrying three ratio pairs plus the factor pair, against five pairs and two ranges before |
| Fable A2, Opus A4 | Two-factor check used whole-run shares; F_w unchecked | Up to the primary crossing (from the timelines), F and F_w request a gradient at a median 83.3% of evaluations in space A and 76.9% and 71.4% in space B. The whole-run share of F_w in space A, 27%, is not used. Expected no-validation ratios: 6.3/11.5 for F and 3.8/6.5 for F_w, against the measured 5.58/10.07 and 3.23/6.10. F_w's session medians are printed (0.220/0.233 base against A′ 0.220/0.238). App. D accounts for the 6–16% excess. Before the crossings, A′'s evaluations cost 0.32–0.34 units, against 0.27–0.31 whole-run; with those costs the estimates come within 4%. The verifier recomputes shares, estimates, residual and correction, and checks that the printed inputs reproduce the printed estimates. v14's whole-run "5.3 and 11.5" is removed (its absence is asserted). F_w's per-evaluation ratio (4.34/8.75) is not printed, because printed-input rounding gives 8.7 against the data's 8.8 |
| Fable A3 / coordinator | Why F_w has one repeat | §5 Protocols: "as its protocol fixed in advance because F's repeats had agreed to about 1% (a median 1.2% per primary crossing time) and the estimand uses repeat 1". The verifier checks the protocol body's sentence and recomputes the 1.2% from F's repeats |
| Opus A5 | Puzzling "with L2 validation as in the sessions" | Deleted (absence asserted) |
| Opus A10 | Freeze-comment nuance | App. D: "The freeze comment recorded the result of the L1 accuracy gate and fixed the warm-start source" (checked against the export) |
| Opus A11 | Bare AssertionError when the package dir is renamed | The assertion now carries a message naming the required directory |
| Opus A3, Fable A5, coordinator task 3 | Related-work leads | All verified on Crossref and OpenAlex (User-Agent `citation-check/1.0`; no email, `mailto` or identifier; records in `citation-metadata-v15.json`; notes in `citation-notes.md`). Six added, each positioned from its abstract (details below the table) |
| Opus A8 / Fable A6 | Package on a branch only | Pointer and verifier moved to `.15`. Merge or deposit left to the operator |
| Opus A7 | Label Fig. 5 ratio rows | Declined: the caption names the rows and the row order is fixed. Not changed, to keep the figure byte-identical |
| Opus A12 / Fable A7 | Letter collision | Declined again (the author's call) |
| length | Main text ≤ 14 pages | Moved to App. D, not deleted (bindings intact): F's L1 accuracy detail, F_w's stencil Newton and charge medians, the per-start B/F_w list, the A–D validation decomposition and the COBYQA interpolation-set conjecture (a one-line pointer stays in §4). Removed as duplicates: §7's "the measured ratios are smaller…" sentence, §7's parallel-stencil sentence (both in §5), §7's space-A level sentence (in §4), and the §5 cold-fallback sentence (in App. D). Figures 1–5 at 0.92 and Fig. 6 at 0.72 of the line width. No figure was re-rendered |

The six added references (related-work leads, verified as above):
- **El Bechari et al. 2022** (doi:10.3390/math10060885): a discrete adjoint for linear and nonlinear magnetostatics, verified against finite differences. Placed in the adjoint paragraph.
- **Gangl et al. 2022** (doi:10.1108/compel-02-2021-0063): gradient-based free-form shape optimization of a synchronous reluctance machine takes minutes, against hours for stochastic parametric optimization in JMAG. Placed in the benchmark paragraph, noting that the comparison also changes the parametrization and the solver.
- **Adelman & Haftka 1986** (AIAA J 24(5), doi:10.2514/3.48671): the standard survey of sensitivity analysis, cited for the direct method.
- **Barthelemy & Haftka 1990** (doi:10.1080/08905459008915677): the accuracy problems of the semi-analytic method for shape variables.
- **Haftka 1985** (IJNME 21(8), doi:10.1002/nme.1620210813): a modified finite-difference procedure for iteratively solved problems.
- **Eyi 2011** (AIAA paper, doi:10.2514/6.2011-3071): chooses the finite-difference step from the convergence error.

Haftka 1985 and Eyi 2011 together position the warm-started arm: it keeps the frozen tolerance and reports the error rather than correcting it. Direct and semi-analytic sensitivities are named as untested.
