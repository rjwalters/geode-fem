# paper-review: motor-gradient-optimization.13 (branch claude/motor-paper-v11, 6e4544fc)

**Reviewer:** Opus, acting in the anvil:paper-review role. The review is read-only and written as an independent, skeptical referee from computational electromagnetics and electric-machine design optimization.

This is a fresh score of v13; the v12 scores (Opus 38/44, Fable 37/44) do not carry over. I read the v12 reviews and v13's `changelog.md` only to check whether each finding was resolved.

**Rubric and voice:**
- Rubric: `.anvil/skills/paper/rubric.md`: 9 dimensions, /44, advance at ≥35, `anvil-pub-v2`.
- Voice: `.anvil/skills/paper/voice.md`.
- Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers".

**Scope limits:**
- `.anvil.json` declares no `venue`, so I scored no venue overlay.
- The thread has no litsearch sibling, so D4 is scored on the legacy baseline.
- The BRIEF sets `web_search: true`, but this session had no web tool. The D4 lead therefore comes from domain knowledge and is a lead for `paper-litsearch`, not a verified citation.

**Isolation:** I modified nothing in the worktree or the repo and did not touch git state.
- All builds, verifier runs and lints used a `cp -R` copy at `scratchpad/motor-v13-review/opus-copy/`.
- The only network calls were read-only `gh` queries (issue #1218, PRs #1219/#1222/#1223, the branch's PR list) and a read-only `git ls-tree origin/main`.
- No email address or identity data was sent anywhere.

## Verdict

- **Total: 39 / 44**
- **Decision: advance: true.** The total is ≥ 35 and no blocking critical flag is set.
- **Critical flags:** none (see the check below).
- **Outstanding dependency (not a critical flag, not scored):** `[PENDING zenodo-doi]`, main.tex L471, which is the operator's deposit of the raw-trace archive. `pending_marker` reports exactly this one source. It holds the READY transition, so the thread stays REVISED until the DOI is filled with the real value. The archive itself exists, and I re-hashed it to the printed SHA-256.
- **Evidence drift:** none (`evidence_drift`: CLEAN).

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 4 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **39** |

v13 resolves the three v12 required findings:
- the package pointer;
- the SW3 reading, settled by measurement;
- the undefined "step".

v13 also resolves most v12 advisories. I raise three new required findings:
- R1 is a sentence the data contradict.
- R2 is a baseline-configuration question no earlier review raised.
- R3 is a comparator the text uses but never prints.

None of the three changes the direction of the result, and none meets the critical-flag standard.

## Mechanical checks (scratch copy)

| Check | Result |
|---|---|
| `artifact_verify` cmd 0: `python3 verify.py` (cwd `.13`) | exit 0, 13 PASS blocks, "126 evidence hashes" |
| `artifact_verify` cmd 1: `python3 verify_exp2b.py` | exit 0, 7 PASS blocks, including the new "Exp 2b L1 step sweep" block. Raw ablation traces were not supplied, as expected |
| `verify_exp2.py` | exit 0, 4 PASS blocks |
| `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` = the staged archive | adds "PASS: raw-trace archive re-hashed against the deposit record". Independent `openssl dgst -sha256`: `2de2132c…0b2a2`, matching the text |
| Build (pdflatex → bibtex → pdflatex ×3) | exit 0, 0 overfull boxes, 0 undefined references or citations, no `??` in `pdftotext`. 36 pages: main text pp. 1–14, Code and data p. 15, appendices pp. 15–31, references from p. 32 |
| `numeric_consistency` | pass. One info finding at main.tex L870 is suppressed, and the suppression is correct (0.27% = GEODE vs the finest reference, not the 0.01071→0.01220 pair) |
| `pending_marker` | 1 marker, `zenodo-doi` (L471) |
| `audience_check` | 0 hits across main.tex, optimization.tex and 10 table files |
| `evidence_drift` | CLEAN |
| `evidence_check` on my scoring table | 9 dimensions, 0 findings, pass |
| Citations | 40 cited keys, 40 bib entries, 0 missing, 0 unused. The five v13 entries are complete with DOIs |
| Abstract | 238 words |
| Voice scan (honest/genuine/transparent/load-bearing/certif/emphatic/ruling/version history/em-dashes) | 0 hits in prose. One tier slip: "an upper bound on the noise contribution" (A1) |
| PR #1223 (`f94acad3`) | MERGED 2026-10-21T09:30:57Z. The record's adjoint values and forward-1e-6 values equal the L1 check's bit for bit (`reproduction.*_diff = 0.0`) |
| Issue #1218 | body created 22:34:54Z, freeze comment 23:00:32Z, first run 02:07Z. The paper's timeline is correct |

## Independent recomputations (from the hash-bound extracts, not via the verifier)

**Step sweep (`fd-step-sweep-L1.json`, recomputed from the raw stencil values and base).**

Forward error ÷ h at h = 1e-5, 1e-6 and 1e-7 m:

| Component | Objective gradient | Mean gradient |
|---|---|---|
| SW3 | 559.9, 564.3, 564.8 | −1865.8, −1880.1, −1881.2 |
| MH | −36.17, −36.29, −37.26 (3.0% spread) | −2780, −2783, −2784 |
| SW2 | 6918, 6912, 6908 | −9692, −9693, −9694 |

- **Spread:** within 0.9% except MH's objective component at 3.0%. This matches the text.
- **SW3 mean, central differences:** 1.6e-4 → 1.7e-6, a 94-fold drop; 2.2e-4 relative at 1e-6.
- **MH and SW2 central residuals at 1e-6:** 9.1e-8, 2.9e-7, 3.1e-7 and 3.4e-7, i.e. 9e-8 to 3.4e-7, matching the text.
- **Reading:** the SW3 miss is truncation, and the central difference converges on the adjoint value, so the adjoint is not biased. This is the right experiment and it settles v12 R2.

**Sign pattern (L0 and L1 records).** For all eight scalar components the signed FD − adjoint error of the mean gradient has one sign at all four points at both levels. SW3 is −1.81…−1.89e-3 at L1 and −3.04…−3.13e-3 at L0. The App. D statements are correct.

**Per-gradient costs (`runs.json`).**
- Median F gradient cost: 1.895 and 4.251 units.
- Median gradient fraction: 0.681 and 0.769.
- Median A′ adjoint: 0.0566 and 0.0607 units.
- All printed values are correct. On the comparator, see R3.

**Path up to the crossing (validated iterates, repeat 1).**
- Space A: every validated objective up to the T90 crossing agrees within 0.36% (max |Δx| 1.3e-5 m).
- Space B at θ0, LHS 1 and LHS 3: within 0.84%.
- Space B at LHS 2 and LHS 4: see the table.

| Space-B start | Validation (crossing) | A′ F/F₀ | F F/F₀ | Difference |
|---|---|---|---|---|
| LHS 2 | 4 | 0.02410 | 0.02281 | −5.4% |
| LHS 2 | 5 | 0.01186 | 0.01114 | −6.1% |
| LHS 2 | 6 (the T99 crossing) | 0.00781 | 0.00556 | −28.8% |
| LHS 4 | 8 | 0.02826 | 0.02331 | −17.5% |
| LHS 4 | 9 (the T99 crossing) | | | −3.2% |

Training evaluations at LHS 2 are 10 (A′) against 9 (F). Space-B runs stop at T99, so these differences all lie at or before the primary crossing (R1).

**Warm start of F's stencil (pinned harness, `git show 5cc86ad8`, read-only).**
- `driver.fd_solve` issues an ordinary `train` request.
- The study's training model is `EvaluatorConfig::training`, which sets `warm: WarmStart::Cold`; the harness README also says "cold start".
- The ablation data agree: a stencil solve costs 0.24 units, the same as a training solve (0.23–0.24).
- The paper's own App. H timing warm-starts its FD sweeps from the base states. There, (8.2−1)/24 to (10.5−1)/24 gives 0.30–0.40 of a cold forward per perturbed sweep (R2).

**Fig. 5 script.** The censored B cell is drawn at a fixed y = 150 min (`render_exp2b_figure.py` L38). B's budget is 120 × 70.94 s = 141.9 min (A3).

**Package on the default branch.** `origin/main` holds versions up to `.10`. `.11`–`.13` exist only on `claude/motor-paper-v11`, which has no PR (A4).

## Critical-flag check

- **Numerical inconsistency:** considered for R1 and not set.
  - Every printed number is correct. The defect is a sentence that misplaces where two numbers lie relative to the crossing.
  - The inference it supports still holds: validation counts match at every start, and the two affected per-start ratios (4.14 and 4.10) lie inside the 3.84–4.33 range.
  - It is a required text fix, not a reason to stop reading.
- **Missing experiment for a claim:** considered for R2 and not set.
  - The title's ratio is measured for a stated FD configuration (forward differences, fixed step, one core).
  - Missing is a disclosure, and ideally a measurement, of how much that configuration's cold stencil solves contribute.
- **Citation error / close prior work ignored:** none. Lyu et al. 2014 is a lead in a neighbouring domain.
- **Build / artifact:** clean. Both declared `artifact_verify` commands exit 0.
- **Audience fit:** 0 hits.
- **Pending marker:** a `pending_dependency`, not a blocking flag. No dimension is scored down for it.

## Required findings (fix before audit)

**R1. §5 says F and A′ take the same path up to each crossing; in space B the data say otherwise at two starts.**

The text:
- optimization.tex L300–302: "Up to each target crossing the two take the same path, crossing after the same number of validations from every start, so the gap is the cost of the gradient."
- L327–330: "After the crossings the paths can diverge. In space B all 20 runs stop at $T_{99}$, F's validated objectives agree with A$'$'s within 1\% at three starts, while at the other two individual validated objectives differ by up to 29\%."

The data:
- Space-B runs stop at T99, the primary target, so there is no "after" in space B.
- The 29% difference is the LHS 2 crossing validation itself (F 0.00556 F₀ against A′ 0.00781 F₀).
- The 17.5% difference is LHS 4's validation 8 of 9.
- At LHS 2, F uses 9 training evaluations to A′'s 10.

The claim that holds, and that the introduction already states correctly, is that both cross each target after the same number of validations. Suggested wording: "Both cross each target after the same number of validations from every start. In space A, and in space B at three starts, their validated objectives agree within 1% up to the crossing; at space-B LHS 2 and LHS 4 the iterates drift apart before the T99 crossing (validated objectives up to 29% and 18% apart, and at LHS 2 F uses one fewer training evaluation), yet the crossing comes at the same validation. The gap is therefore mainly the cost of the gradient."

Then limit the "After the crossings" paragraph to space A. `verify_exp2b.py` L379 binds the current sentence as a phrase, so update the bound phrase and add an assertion on where the divergence occurs.

Note on provenance: my own v12 advisory A4 placed this divergence after the crossings ("After that the paths diverge: … in space B, validated objectives differ by up to 29% at two starts"). The v13 wording follows that imprecise note. The error is mine as much as the reviser's. D1.

**R2. F's stencil solves start cold, and the paper does not say so; the paper's own FD timing warm-starts them.**

The text:
- §5 Protocol: "Each stencil solve is an ordinary training solve on the base point's mesh generation." In the pinned harness that means `WarmStart::Cold`.
- App. H: "Central differences use 24 forward sweeps, each warm-started from the base states". There a perturbed sweep costs about 0.30–0.40 of a cold forward. In the ablation a stencil solve costs one full training solve (0.24 against 0.23–0.24 units).

A CEM referee will ask first how much of the 2.47 and 4.11 is the cold start. The introduction says the size of the advantage "is the question here".

A rough estimate, labelled as such: under the paper's own per-evaluation cost model, a stencil solve at 0.35 of a training solve would bring the no-validation ratio from about 5.2 to about 2.3 in space A and from about 11.9 to about 4.7 in space B. That ignores any effect on A′'s and F's base solves.

There is a plausible reason for cold stencils that the paper could state. At the 1e-8 Newton tolerance, a cold base and cold stencil follow nearly the same iteration path, so their solver errors cancel in the difference. A warm stencil can stop after one Newton step with an error that does not cancel, which would add noise at h = 1e-6.

Required, at minimum:
- state in §5 Protocol that stencil solves start cold, as every training solve does under the preregistered training configuration, and why;
- in §7, scope the per-gradient argument to that configuration.

Better: measure the cost and accuracy of warm-started stencils at the four L1 check points, which costs a few dozen L1 solves. Then report an estimated F/A′ for a warm-started F, labelled as an estimate, beside the measured one.

If the title keeps "than finite differences", the paper owes the reader this condition, as it already gives the per-core one. D2.

**R3. Print the "per-gradient cost ratio" that §5 and §7 compare against.**

The text:
- optimization.tex L308–311: "F/A$'$ becomes 5.58 and 10.07, less than the per-gradient cost ratio because only a median 68\% and 77\% of F's training evaluations request a gradient".
- main.tex L366–369: "smaller than the per-gradient cost ratio, because shared validation charges and line-search evaluations without a gradient are included".

Read with the printed numbers:
- The per-gradient ratio is 1.90/0.057 = 33 and 4.25/0.061 = 70.
- The 68%/77% fraction takes these only to 23 and 54.
- The larger reduction comes from the 0.23–0.24-unit training solve that every evaluation pays in both arms. That fact is in the same sentence, but the "because" does not credit it.

If the intended comparator is the cost of a gradient-bearing evaluation, (0.24 + 1.90)/(0.24 + 0.057) ≈ 7.3 and (0.24 + 4.25)/(0.24 + 0.061) ≈ 15, the explanation works: the fraction gives ≈ 5.2 and ≈ 11.9 against the measured 5.58 and 10.07.

Print that ratio and name both factors, the shared training solve and the gradient fraction. Make the same change in §7. This is the same failure class as v12 R3, and the v13 changelog's statement that "No printed ratio needs unprinted inputs" does not yet hold. D7.

## Advisory findings

- **A1 (D7, voice: "bounded, not estimated").** App. D, main.tex L756–757: "an upper bound on the noise contribution at that step". A single central-difference residual at one h includes the O(h²) truncation term and can partly cancel noise, so it is not a proven bound. Write "which puts the noise contribution at that step at or below about 3e-7".
- **A2 (D7).** App. D, L758–760: give units for "1.9e3, 2.8e3, 9.7e3" (N m/m²) and for the slope "−7.7e-3" (N m/m). Say "of the mean gradient", since SW3's objective-gradient truncation (564 per unit step) is larger than MH's (36).
- **A3 (D6).** Fig. 5: the censored B cell at space-B LHS 3 is drawn at a fixed 150 min, above B's 141.9-min budget. Add "plotted above the budget" to the caption, or draw it at the budget with an arrow.
- **A4 (D5).** The data statement names `papers/motor-gradient-optimization/motor-gradient-optimization.13` "of the GEODE repository", linked to the repo root. That directory is not on the default branch yet (`origin/main` stops at `.10`). Either pin the pointer to the merge commit, or include the package directory in the Zenodo deposit, so the pointer survives later branch changes.
- **A5 (D4, tag related-work, for `paper-litsearch`).** Lead: Lyu, Xu & Martins, "Benchmarking optimization algorithms for wing aerodynamic design optimization", ICCFD8, 2014. As I recall, it runs the same gradient optimizer with adjoint and with FD gradients against gradient-free methods as the variable count grows, which is §5's design in aerodynamics. The changelog dropped it because neither Crossref nor OpenAlex has a record. A conference citation with the proceedings URL is acceptable once litsearch confirms it.
- **A6 (D7).** Methods A–F and spaces A/B still share letters (declined twice). It is a decoding cost, and the author's call.
- **A7 (D6/D7).** Fig. 6 (`[!htb]`, §6) floats past §7 to p. 14, just before the Conclusion. Consider `[!t]` placement inside §6 or a `\FloatBarrier` before §7.
- **A8 (D9, optional).** The abstract still ends on the interface-order fact. Ending on the decision result ("the adjoint, more than the gradient-based optimizer, accounts for that advantage") would leave the reader with the paper's point.
- **Procedural (operator-side, not a paper finding):** the archive currently sits in one directory, `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/`. Keep a second durable copy until the deposit lands.

## Per-dimension justification (quoted evidence; checked by `anvil.lib.evidence_check`: 9 dims, 0 findings)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The ablation varies one thing, runs an in-session control, freezes its estimand before any run, and now separates truncation from noise and from adjoint bias by measurement: a central difference brings the failing SW3 component to 2.2e-4 relative, so the adjoint is right and F's forward step is truncation-limited. One point off: the mechanism sentence for the title's ratio overstates the data ("Up to each target crossing the two take the same path, crossing after the same number of validations from every start, so the gap is the cost of the gradient" — §5 Results). At space-B LHS 2 and LHS 4 the validated objectives differ by 29% and 17.5% at or before the T99 crossing (validation 6 of 6 and 8 of 9), and at LHS 2 F uses 9 training evaluations against A′'s 10. The next paragraph then places those differences after the crossings ("After the crossings the paths can diverge." — §5 Results), but space-B runs stop at T99 (R1). The conclusion survives: validation counts match and the per-start ratios (4.14, 4.10) sit in the 3.84–4.33 range. |
| 2 | Evidence sufficiency | 6 | 4 | 100 + 40 hash-bound runs. Every ablation cell is attained, repeats are within noise, selected designs are refined at L3, F's gradients are checked at L1, and the SW3 miss is now explained by a step sweep. Two points off. (a) The FD baseline's cost configuration is untested and undisclosed: F's stencil solves start cold ("Each stencil solve is an ordinary training solve on the base point's mesh generation" — §5 Protocol), while the paper's own derivative timing warm-starts them ("Central differences use 24 forward sweeps, each warm-started from the base states" — App. H) at about 0.30–0.40 forwards each against about 1.0 in the ablation. The paper names the size of the gap as its question, so the size rests on one FD configuration (R2). (b) The standing scope limits: one motor, five deterministic starts, and the BRIEF's reduced-physics competitor untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7). |
| 3 | Clarity of contribution | 5 | 5 | The title now carries the per-core condition ("Per core, adjoint gradients reach validated torque-ripple" — title). The abstract labels the ablation post hoc in its opening clause, and the introduction states the contribution in one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1) along with the decision result. Cold-reader test passed: no buried lede and no overclaim. |
| 4 | Related-work positioning | 5 | 4 | v13 positions the cost and step-selection literature on its merits ("our finite-difference arm instead uses one step fixed in advance" — §2) and adds Lei et al., Peter & Dwight and Kenway et al. One point off, legacy baseline (no litsearch sibling): the closest experimental precedent for the ablation, the same gradient optimizer run with adjoint and with finite-difference gradients against gradient-free methods as the variable count grows (Lyu, Xu & Martins, ICCFD8 2014), is still not engaged. A conference citation needs no DOI. Not a flag. |
| 5 | Reproducibility | 5 | 5 | Every evaluator, harness, check and sweep is pinned by commit. Extracts are hash-bound, and the inventories are RFC 3161- and OpenTimestamps-stamped. The package pointer is correct and bound in the verifier, and the package ships "three standard-library verifiers that recompute the reported numbers from those inputs" (Code and data availability). The raw-trace archive is named by size and SHA-256, and I re-hashed it to the printed digest. The DOI marker is a declared operator dependency and is not scored. Advisory only: the named directory is not yet on the repository's default branch (A4). |
| 6 | Figure & table quality | 4 | 4 | Fig. 5 now shows the estimand: each start's F/A′ ratio is printed above its pair, and the x-axis is labelled. The caption reads standalone ("The cross marks the start from which" B missed T99 — Fig. 5). Captions of Tables 8–10 define their columns, and Fig. 6 (c) labels are legible at print size. One nit, not deducted: the censored cross is drawn at a fixed 150 min with no caption note (A3). |
| 7 | Prose & structural quality | 4 | 3 | Clean build (0 overfull boxes, no ??). The voice scan and audience check are clean. One point off: v13's cost paragraph explains a measured ratio against a comparator it never prints ("less than the per-gradient cost ratio because only a median 68\% and 77\% of F's training evaluations request a gradient" — §5 Results). Read with the printed numbers, the per-gradient ratio is 1.90/0.057 = 33 and 4.25/0.061 = 70. The gradient fraction takes those only to 23 and 54. The larger factor, the 0.23–0.24-unit training solve that every evaluation pays in both arms, is in the preceding clause but not in the "because" (R3). Minor: an unproven "bound" ("an upper bound on the noise contribution at that step" — App. D), unit-less curvature and slope values in App. D, and the method/space letter collision. |
| 8 | Citation hygiene | 5 | 5 | All 40 \cite keys resolve, with 0 missing and 0 unused. Every entry carries author, title, venue and year, with DOIs where they exist. The five v13 additions are cited for what their abstracts or titles state ("survey sensitivity-analysis approaches and adjoint practice for aerodynamic optimization" — §2). No instance of an unresolved citation found. |
| 9 | Rhetorical economy | 4 | 4 | The abstract is 238 words and the main text is 14 pages. The SW3 detail has moved to App. D, leaving one sentence in §5. The introduction no longer restates the abstract's ranges, and §7 opens with what transfers ("Three findings should transfer beyond this motor." — §7). Hedging is proportional. The appendices are long but are evidence a skeptical reader asks for. |

## Resolution of v12 findings (checked against the v13 text, not carried over)

| v12 finding | Status in v13 |
|---|---|
| R1 (both): package pointer names `.11` | **Resolved.** Names `.13`; `verify_exp2b.py` asserts the path equals its own directory. Residual A4 (not yet on the default branch) |
| R2 (Opus R2, Fable A4): SW3 "noise floor" reading | **Resolved by measurement.** "Noise floor" is gone (verifier asserts absence); App. D states the fixed-sign pattern, and the step sweep at the failing point shows truncation, with a central difference converging on the adjoint. Recomputed above. Residual A1/A2 wording |
| R3 (Opus R3, Fable R2): undefined "step"; 10.9 not reproducible | **Resolved** for the unit and the step (per-gradient costs in a defined unit; 5.3/10.9 removed). The new comparator, "the per-gradient cost ratio", is unprinted (R3) |
| Fig. 5 estimand (Fable A1) | **Resolved.** Per-start ratios printed; x-axis labelled |
| F's budget stops (Fable A3) | **Resolved.** Stated, and stated as untested why |
| "times slower" (Opus A5) | **Resolved** ("took 2.5 and 4.1 times as long") |
| "essentially the same path" (Opus A4) | **Changed, now overstated.** "Take the same path" up to each crossing is false at space-B LHS 2/4 (R1). My v12 note mislocated the divergence |
| "n-fold compute" (Opus A6) | **Partly.** §7 now says the ratios are smaller than the per-gradient ratio; the stated reasons omit the shared training solve (R3) |
| Intro restates abstract (Fable A5) | **Resolved** |
| Title per-core (Opus A1) | **Resolved** |
| Stale `_progress.json`, README 123 vs 125 (Fable A8, Opus A7) | **Resolved** (README 126 = manifest 126 = verifier) |
| "three verifiers" once (Fable A9) | **Resolved** |
| Related-work leads (Opus A2, Fable A2) | **Mostly resolved.** Lei 2017, Peter & Dwight 2010, Moré & Wild 2011/2012 and Kenway 2019 added and positioned; Lyu et al. 2014 not added (A5) |
| L1 check timing (Opus A3) | **Resolved** ("it was not part of the ablation's protocol") |
| SW3 detail out of §5 (Opus A8, Fable A6) | **Resolved.** New "Gradient accuracy" paragraph in §5; detail in App. D |
| Raw traces private (Opus A7, Fable A10) | **Resolved pending deposit.** Archive built and digest printed; `[PENDING zenodo-doi]` is the operator's dependency |
| Letter collision (Opus A9, Fable A7) | **Declined** (A6) |
| Fig. 5 x-axis label (Opus A10) | **Resolved** |

## Rubric version transition

The prior review (`.12.review/`) was scored on the same `anvil-pub-v2` /44 rubric, so no transition subsection applies.

## Procedural notes

**Read:**
- main.tex (1,202 lines) and optimization.tex (372 lines), with the tables and refs.bib;
- `changelog.md`, `README.md`, `_progress.json`, `evidence-manifest.json`, and the `verify_exp2b.py` cost, path and sweep blocks;
- `../BRIEF.md` and `../.anvil.json`;
- `.12.review/{verdict,review-opus,review-fable}.md`;
- issue #1218 (body and comments) and the bodies of PRs #1219, #1222 and #1223;
- `driver.py` (`fd_gradient`, `fd_solve`), the harness README and `icem_remesh.rs`/`icem_harness.rs` at `5cc86ad8`, via `git show`.

**Ran on the copy:**
- the three verifiers, one with `MOTOR_RAW_ARCHIVE` (read-only re-hash of the staged archive);
- a pdflatex/bibtex build;
- renders of pp. 1, 11, 12, 14 and 22;
- `numeric_consistency`, `pending_marker`, `audience_check` and `evidence_drift`, without `--write-review` (no sidecars written under the worktree);
- `evidence_check` against `scratchpad/motor-v13-review/opus-stage/opus-scoring.md`;
- read-only Python passes over `exp2b/runs.json`, `fd-step-sweep-L1.json` and both FD-check records.

**Outputs:** no `.13.review/` sibling was written. Per the coordinator's constraints, this report is the reviewer output, and the coordinator owns the sidecar. It follows the content of `paper-review.md`'s `verdict.md`, `scoring.md`, `comments.md` and `findings.md`.
