# paper-review: motor-gradient-optimization.12 (branch claude/motor-paper-v11, e7caaa43)

Reviewer: Opus, in the anvil:paper-review role (read-only). I reviewed as an independent, skeptical
referee from computational electromagnetics and electric-machine design optimization. This is a
fresh score of v12. The v11 scores (Opus 35/44, Fable 36/44) do not carry over. I read the v11
reviews and the v12 changelog only to check whether each v11 finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md`, 9 dimensions, **/44, advance threshold ≥35**. Voice:
`.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational
electromagnetics, electric-machine optimization, and differentiable scientific computing
researchers". `.anvil.json` declares no `venue`, so I scored no venue overlay. The thread has no
litsearch sibling, so D4 is scored on the legacy baseline. The BRIEF sets `web_search: true`, but I
had no web tool in this session, so the D4 leads come from domain knowledge. They are leads for a
`paper-litsearch` re-run, not verified citations.

I modified nothing in the worktree or the repo and did not touch git state. All builds and verifier
runs used a `cp -R` copy at `scratchpad/motor-v12-review/opus-work/`. The scoring table was
self-checked with `anvil.lib.evidence_check` from `opus-stage/opus-scoring.md`: 9 dimensions, 0
findings, pass.

## Verdict

- **Total: 38 / 44**
- **advance: true.** The total clears the threshold by three points and no critical flag is set.
- **Critical flags: none** (see the check below).
- **Outstanding dependencies: none.** `pending_marker` found 0 markers.

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **38** |

v12 resolves all seven merged v11 required findings. The three required findings below are small
text fixes. One of them (R2) corrects an interpretation that the paper's own L1 record contradicts.

## Mechanical checks

- **artifact_verify** (`python3 verify.py`, then `python3 verify_exp2b.py`, cwd `.12`): **both
  exit 0**.
  - `verify.py` printed 13 PASS lines in 0.34 s, including "125 evidence hashes" and the new block
    "Exp 2b L1 finite-difference accuracy (4 points; ...; one component-wise FAIL, SW3 mean at the
    space-A interior)".
  - `verify_exp2b.py` printed 5 PASS lines. As before, the default run checks only the hash-bound
    extracts ("raw ablation traces not supplied").
- **Build**: `papers/build.sh` on the copy exited 0 after 3 pdflatex passes. The log has 0 overfull
  boxes and 0 undefined references or citations. `pdftotext` finds no `??`. The rebuilt `main.bbl`
  is byte-identical to the committed one. The PDF is 34 pages:
  - main text pp. 1–14, with the ablation §5 on pp. 10–12;
  - Code and data on p. 15;
  - appendices pp. 16–31 and references from p. 31.
- **numeric_consistency**: pass. Its one finding, at main.tex L849, is suppressed, and the
  suppression is still correct (0.08036/0.08014 − 1 = 0.27%).
- **pending_marker**: 0 markers. **audience_check**: 0 hits across main.tex, optimization.tex and 9
  table files.
- **Voice scan** (honest/genuine/transparent/load-bearing/certif/novelty "first"/version history/
  "ruling"/"gradient access"/"dilutes"/"logarithm of"/em-dashes): 0 hits in prose. The only `---`
  strings are censored cells in `tables/exp2-ratios.tex`.
- **Citations**: 35 cited keys and 35 bib entries, with 0 missing and 0 unused.
- **Abstract**: 238 words.
- **Issue #1218** (read-only GraphQL): the body was created 2026-10-20T22:34:54Z with
  `userContentEdits.totalCount = 0`. The freeze comment came at 23:00:32Z. The paper's "the protocol
  text was never edited" holds. The frozen protocol also matches the paper's description of F:
  1e-6 per component unit, base value reused, backward step when the forward step is unusable, and
  the L0 limits of 5e-3 and 1e-1.

### Independent checks on the new v12 material

- **Matched-start B/F.** The per-start values (1.24, 1.45, 0.70, 0.95, 2.27 and 3.84, 2.46, 1.26,
  1.12) match my v11 arithmetic to rounding. F/A′ over the four matched space-B starts is 4.10.
  Chain closure is 3.02/7.82 against 3.10/7.79. Text, verifier and Fig. 5 agree.
- **Fig. 5** (p. 11, rendered at 80 dpi) shows what the caption says:
  - A′ sits below F from every start.
  - B is above F at θ0, LHS 1 and LHS 4 in space A, and close to F at LHS 2 and LHS 3.
  - The censored B cell at space-B LHS 3 is marked.
- **Step-cost numbers**, recomputed from `exp2b/runs.json`. "Step" in the verifier means
  (forward + line_search + adjoint) per **training event**. Only a median 0.68 (space A) and 0.77
  (space B) of F's training events carry a finite-difference gradient: F's stencil solves per
  training event are 5.45 and 13.85, not 8 and 18. The numbers are internally consistent, but the
  text does not define "step" (R3).
- **L1 finite-difference record** (`fd-gradient-runtime-checks-L1.json`). I printed the signed
  error FD − adjoint of the mean gradient for every component at all four points:

  | Point | MAG | MH | MW | SW1 | SW2 | SW3 | SW4 |
  |---|---|---|---|---|---|---|---|
  | A, θ0 | +2.93e-3 | −2.98e-3 | −7.6e-4 | −1.48e-3 | −9.78e-3 | −1.81e-3 | −2.6e-4 |
  | A, interior | +1.09e-3 | −2.78e-3 | −9.9e-4 | −1.07e-3 | −9.69e-3 | −1.88e-3 | −1.2e-4 |
  | B, θ0 | +2.92e-3 | −2.98e-3 | −7.5e-4 | −1.48e-3 | −9.80e-3 | −1.85e-3 | −2.6e-4 |
  | B, interior | +1.87e-3 | −2.96e-3 | −9.7e-4 | −3.7e-4 | −8.45e-3 | −1.89e-3 | −6.6e-5 |

  Each component's error has a fixed sign and nearly the same size at four designs, including two
  quite different interior points. Components share no common floor: SW2 is about 1e-2, SW4 1e-4 and
  Sr1 2e-5. The F-objective errors for SW3 show the same pattern (+4.3 to +5.7e-4 at all four
  points). The L0 record repeats it (SW3 −3.0 to −3.1e-3 at all four points). That is the
  signature of a deterministic, component-specific offset, most plausibly one-sided truncation
  error h/2·f″ (for SW3 this implies f″ ≈ 3.7e3 N m/m²). It is not a noise floor of the step (R2).

## Critical-flag check

- **Citation error**: none. The six new entries are positioned for what their verified abstracts or
  titles state. `citation-notes.md` records the basis for each.
- **Numerical inconsistency**: none at flag level. The step-cost wording (R3) looks inconsistent,
  but the numbers are correct under the verifier's definition.
- **Missing experiment for a claim**: not set. The v11 gap (F's accuracy at the running model) is
  closed by measurement.
- **Close prior work ignored**: not set. Lyu, Xu & Martins (2014) is a lead (A2), and it is in a
  different domain.
- **Build / artifact**: clean.
- **Post-hoc result in the title**: considered again and not set. Every introduction of the ablation
  labels it post hoc, the abstract's sentence opens with the label, and the protocol was frozen
  before the runs (verified).

## Required findings (fix before audit)

**R1. The data statement points readers to the v11 package.** main.tex L455–456: "The paper package
is the directory \texttt{papers/motor-gradient-optimization/motor-gradient-optimization.11}". The
v12 package is `.12`. The v11 package lacks the L1 record, Fig. 5, the matched-start and stop-reason
checks and the v12 phrase bindings, so a reader who follows the pointer verifies a different
paper. Change it to `.12`. Better, pin it to a commit or a deposit DOI so the pointer cannot go
stale again. D5.

**R2. The paper's own L1 record contradicts its reading of the failing component.** optimization.tex
L338–341: "Its absolute error, $1.9\times10^{-3}$, matches SW3's at the three passing points
($1.8$--$1.9\times10^{-3}$); we read this as the noise floor of the fixed step on a small component,
an interpretation that no step sweep has tested." A noise floor would be roughly common to all
components and would vary in sign and size between designs. The record shows the opposite (table
above):
- Every component carries its own fixed-sign error, reproduced to two digits at four designs.
- SW3's error is the same at all four points because it is a deterministic offset, most plausibly
  forward-difference truncation.

The conclusion is unaffected. F's gradient is what F is, and the paths match. But the stated
mechanism is wrong, and the hedge "no step sweep has tested" does not repair a reading the data
already contradict.

Required: replace the sentence. For example: "Its absolute error, 1.9e-3, is the same at all four
points and has the same sign, as is each other component's; this pattern indicates a deterministic
one-sided-difference offset, which is small for every component except where the derivative itself
is near zero." Optionally add one central-difference evaluation of SW3 at the space-A interior point
(two solves). That would separate truncation from an adjoint bias. Otherwise state that the two are
not separated. D1. Make the matching change in Appendix D ("Ablation session and finite-difference
accuracy") and in §7's limits sentence if it is reworded.

**R3. Define the training "step" in the step-cost sentence.** optimization.tex L303–307: "one F step
costs 1.53 and 3.32 units, an A$'$ step 0.29 and 0.31. A stencil solve costs a median of 0.23 units".
A reader takes a "step" to be one gradient, base plus n stencil solves, and computes
9 × 0.23 = 2.07 and 19 × 0.23 = 4.37. Neither is 1.53 or 3.32. The verifier divides by all training
events, and only 68% and 77% of F's events request a gradient (5.45 and 13.85 stencil solves per
event). Say "per training evaluation (line-search evaluations included, of which about a quarter
request no gradient)". Alternatively, report the cost per gradient. Without this, an auditor reading
the paragraph cold will log a numerical inconsistency. D7.

## Advisory findings

- **A1 (D3; title).** Carried from v11 A1, partly addressed. The decision result is now in the
  abstract, introduction, §7 and conclusion:
  - the adjoint, more than the optimizer, accounts for the advantage;
  - FD-SLSQP is not reliably faster than COBYQA with eight parameters.

  The title still headlines the post-hoc and most predictable contrast, and its "sooner" holds per
  core. The abstract qualifies both at once, so this is defensible. A title that carries the
  decision result, or adds "per core", would pre-empt the first referee objection.
- **A2 (D4; related-work leads, tag related-work, for `paper-litsearch`).**
  - Lyu, Xu & Martins, "Benchmarking optimization algorithms for wing aerodynamic design
    optimization", ICCFD8, 2014. It is the closest experimental precedent for §5: the same
    gradient optimizer run with adjoint and with finite-difference gradients, against gradient-free
    methods, as the variable count grows. It has no DOI, but a conference citation is acceptable.
    The v11 changelog dropped it for lack of a DOI record.
  - A machine-design optimization-methods review, e.g. Lei et al., "A review of design optimization
    methods for electrical machines", Energies 10(12), 2017, to sit beside Bramerdorfer et al.
- **A3 (D2; §5).** State in one clause that F's L1 accuracy check was run after the ablation and
  was not part of its frozen protocol. Only the L0 wiring check was frozen (issue #1218, ruling 4).
  The applied limits were frozen, but the check's timing was not.
- **A4 (D7; Introduction L66–67, §5 L300–302).** "both variants take essentially the same path"
  holds up to each crossing (equal validation counts). After that the paths diverge:
  - in space A, F hits the budget in 8 of 10 runs against 2 of 10 for A′;
  - in space B, validated objectives differ by up to 29% at two starts.

  Write "cross each target after the same number of validations".
- **A5 (D7; Conclusion L423).** "was 2.5 and 4.1 times slower" should read "took 2.5 and 4.1 times
  as long". "Times slower" is ambiguous.
- **A6 (D7; §7 L381–382).** The sentence "solving its $n$ points in parallel would shorten the wall
  time per gradient but not the $n$-fold compute, which is the quantity the ratio measures" needs a
  fix. The measured 2.47/4.11 is not an n-fold ratio: it is diluted by shared validation, and the
  per-step ratios are 5.3/10.9. Say "which is what the ratio compares, diluted by the shared
  validation charges".
- **A7 (D5; Code and data, README).**
  - The raw traces of both studies are still not public ("retained in two verified copies each").
    A Zenodo deposit of the two raw trees would open the re-derivation path to readers.
  - The package README says the verifiers "check 123 input hashes", but `verify.py` reports 125.
- **A8 (D9; §5 Results).** Move the SW3 detail (L334–341) to Appendix D. Leave one sentence in the
  body: norm-wise errors within limits, and one small component outside the per-component limit,
  explained in App. D.
- **A9 (D7).** Methods A–F and spaces A/B still share letters (declined in v12 as high-risk). "B/A
  in space A" still makes the reader decode. A lighter fix than renaming everything is to call the
  spaces S8 and S18 in prose only.
- **A10 (D6; Fig. 5).** The x-axis has no label ("start"). This is a nit.

## Per-dimension justification (quoted evidence; verified by evidence_check)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The ablation varies one thing and now says how the stencil runs ("so F's $n$ stencil solves run one after another and the time ratio is also a ratio of compute" — §5 Protocol); the decomposition uses matched starts and the stop reasons are given. One point off: the reading of the failing L1 component ("we read this as the noise floor of the fixed step on a small component" — §5 Results) is contradicted by the record, where every component's signed FD-minus-adjoint error repeats across all four points (SW3 −1.81 to −1.89e-3, MH −2.78 to −2.98e-3), the signature of a deterministic one-sided-difference offset, not noise (R2). Wall time under 15 concurrent processes remains a disclosed secondary weakness. |
| 2 | Evidence sufficiency | 6 | 5 | The title's result is measured from every start with no censored cell, a same-session control, repeats, refinement of the selected designs, and now an L1 accuracy check of F's own gradients ("F's gradients were checked against the adjoint on the L1 training model" — §5). One point off for scope: five deterministic starts of one motor, one FD configuration with no step sweep, and the BRIEF's reduced-physics competitor untested ("No reduced physical model, such as a magnetic equivalent circuit or a waveform model, was compared" — §7). |
| 3 | Clarity of contribution | 5 | 5 | Abstract and introduction state the contribution in one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1) and now carry the decision result as well ("in the smaller space, gradient-based search without an adjoint is not reliably faster than the derivative-free method" — §1). The post-hoc status and the one-core condition are in the abstract. |
| 4 | Related-work positioning | 5 | 4 | The derivative-method literature the title's contrast needs is now in and positioned ("These works establish why an adjoint gradient is cheaper than a finite-difference one" — §2), with Park 1992 and Kuci 2016/2017 for nonlinear magnetostatic and machine sensitivities. One point off: the closest experimental precedent for the ablation itself, a matched benchmark of the same gradient optimizer with adjoint versus finite-difference gradients against gradient-free methods as the variable count grows (Lyu, Xu and Martins 2014, wing design), is absent, as is a machine-design optimization-methods review beyond Bramerdorfer (leads, A2). |
| 5 | Reproducibility | 5 | 4 | Pinned commits for every evaluator, the L1 check and the refinement program; hash-bound extracts; timestamped inventories; three stdlib verifiers that pass. One point off: the data statement names the wrong package ("papers/motor-gradient-optimization/motor-gradient-optimization.11" — Code and data), whose verifiers do not cover the L1 check or the v12 numbers (R1), and the raw traces of both studies are still not public. |
| 6 | Figure & table quality | 4 | 4 | The new Fig. 5 puts the title's evidence in the body with a self-contained caption ("Minutes avoid the 6\% difference between the two sessions' time units" — Fig. 5); Table 9's caption now stands alone and its medians match the text; Fig. 4's ticks no longer collide. No instance of a caption that fails standalone reading among the new or changed exhibits found. |
| 7 | Prose & structural quality | 4 | 3 | Clean build (0 overfull, no ??), no voice-rule hits, audience_check 0. One point off: an undefined unit makes two adjacent numbers look inconsistent ("one F step costs 1.53 and 3.32 units, an A$'$ step 0.29 and 0.31" next to a 0.23-unit stencil solve, while a full eight-component gradient would cost 9 x 0.23 = 2.07; the "step" is a training evaluation, of which only 68–77% request a gradient, R3); the method/space letter collision persists; and the conclusion's "used forward differences was 2.5 and 4.1 times slower" inverts the ratio's natural reading. |
| 8 | Citation hygiene | 5 | 5 | All 35 cited keys resolve with no unused entries; every entry carries author, title, venue and year with DOIs where they exist; the cost premise now has methodological sources ("whereas a finite-difference gradient needs one extra solve per parameter" — §1), and citation-notes.md records what each new source was checked for. No instance of an unresolved citation found. |
| 9 | Rhetorical economy | 4 | 3 | The v11 bloat is gone: 238-word abstract, main text pp. 1–14, headline numbers once each in abstract, introduction and conclusion. One point off: the §5 Results paragraphs carry most of the paper's new numbers in two dense blocks, including an eight-line digression on one component 8.7e-5 times the largest ("Its absolute error, $1.9\times10^{-3}$, matches SW3's at the three passing points" — §5) that belongs in Appendix D with one sentence left in the body; the abstract still closes on a forward-model detail rather than the decision result. |

## Resolution of v11 findings (checked against the v12 text, not carried over)

| v11 finding | Status in v12 |
|---|---|
| Merged R1 (Opus R4, Fable R1): serial stencil, per-core ratio, no-validation ratios | **Resolved.** Abstract ("With every run on one single-threaded core, so that the finite-difference stencil is solved serially"), §5 Protocol and §7. 5.58/10.07 are in the abstract and §5. A6 is a residual wording nit |
| Merged R2 (Opus R1): "gradient access" mislabel | **Resolved.** 0 occurrences. §5 "changes only how SLSQP obtains its gradient, and therefore what the gradient costs" |
| Merged R3: per-start B/F, matched starts, log share | **Resolved.** New "The optimizer factor" paragraph with per-start values, matched start sets, unit and seconds means, and chain closure. The log share is gone |
| Merged R4 (Opus R3): "dilutes every ratio" | **Resolved.** §7: "compresses same-resolution ratios toward one, and it inflates the lead of SLSQP over the coarse competitor D" |
| Merged R5 (Fable R2): title evidence only in appendix | **Resolved.** Fig. 5 on p. 11 |
| Merged R6 (Fable R3): FD accuracy at L1; stop reasons | **Resolved by measurement.** The L1 check is reported, including the component FAIL. Stop reasons are given. The new interpretation issue is R2 |
| Merged R7: abstract and length | **Resolved.** 238 words; main text 14 pages; no headline restated in the Discussion |
| Opus A1: title carries the predictable contrast | **Partly.** The decision result is now in the abstract, introduction, §7 and conclusion, and the conclusion says "post-hoc". The title is unchanged (A1) |
| Opus A2 / Fable A2: step unit, stencil solves | **Resolved** ("metres for the lengths and offsets"; "an ordinary training solve on the base point's mesh generation") |
| Opus A3: consistency of non-validation and step-cost ratios | **Resolved**, and the v11 review's 1.7-forward inference is corrected. The new wording needs "step" defined (R3) |
| Opus A4 / Fable A5: derivative-method references | **Resolved** (Martins & Hwang, Giles & Pierce, Park, Nikolova, Kuci ×2). New lead A2 |
| Opus A5: raw traces, unpinned package | **Partly.** Retention is stated, but the traces are still private, and the package pointer is now wrong (R1) |
| Opus A6 / Fable A3, A4: Table 9 caption, statistic mismatch, ticks | **Resolved** |
| Opus A7: letter collision | **Declined** (A9) |
| Opus A8: forwarding cross-reference | **Resolved** |
| Opus A9 / Fable A8: repetition in conclusion | **Resolved** |
| Opus D7: "ruling" | **Resolved** (0 occurrences) |
| Fable A1: stale README | **Resolved**, except the hash count (A7) |
| Fable A7: provenance language in body | **Resolved** (moved to Code and data) |
| Fable A9: title rounding | **Resolved** ("2.5 and 4.1 times") |
| Fable A10: post-hoc label in abstract | **Resolved** ("A post-hoc ablation, not preregistered but run under a protocol frozen beforehand") |

## Procedural notes

- **Read:**
  - all of main.tex (1166 lines) and optimization.tex (363 lines), the ablation tables, and refs.bib;
  - `changelog.md`, `citation-notes.md`, `README.md`, and the `verify_exp2b.py` step-cost,
    decomposition and stop-reason assertions;
  - `../BRIEF.md`, `../.anvil.json`, and the v11 `verdict.md`, `review-opus.md` and
    `review-fable.md`;
  - issue #1218, with body, comments and GraphQL edit history.
- **Ran on the scratch copy:**
  - `verify.py` and `verify_exp2b.py`;
  - `papers/build.sh`, plus 80-dpi renders of pp. 1, 9, 11, 22 and 23;
  - `anvil.lib.numeric_consistency`, `pending_marker` and `audience_check`, without
    `--write-review`;
  - `anvil.lib.evidence_check` on `opus-stage/opus-scoring.md`;
  - two read-only Python passes over `exp2b/runs.json` and the two FD records.
- The signed-error table in the mechanical checks is my computation from
  `fd-gradient-runtime-checks-L1.json` (fields `fd_dmean_full` and `adjoint_dmean_full`).
- No email or identity data was sent anywhere. The only network calls were read-only `gh` queries
  for issue #1218.
- No review sibling dir was written. This report is the reviewer's output for the coordinator, who
  owns the `.review/` sidecar.
