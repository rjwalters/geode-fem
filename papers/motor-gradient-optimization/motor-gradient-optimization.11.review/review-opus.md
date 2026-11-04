# paper-review: motor-gradient-optimization.11 (branch claude/motor-paper-v11, c7d465f6)

Reviewer: Opus, in the anvil:paper-review role (read-only). I reviewed as an independent, skeptical
referee from computational electromagnetics and electric-machine design optimization. This is a
fresh score of v11. The v10 scores (Opus 35/44, Fable 35/44) do not carry over. I read the v10
reviews and the v11 changelog only to check whether each earlier finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md`, 9 dimensions, **/44, advance threshold ≥35**. Voice:
`.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational
electromagnetics, electric-machine optimization, and differentiable scientific computing
researchers". `.anvil.json` declares no `venue`, so I scored no venue overlay. The thread has no
litsearch sibling, so D4 is scored on the legacy baseline. The BRIEF sets `web_search: true`, but I
had no web tool in this session, so the D4 leads below come from domain knowledge. They are leads
for a `paper-litsearch` re-run, not verified citations.

I modified nothing in the worktree or the repo, and I did not touch git state. All builds and
verifier runs used a `cp -R` copy at `scratchpad/motor-v11-review/opus-work/`.

## Verdict

- **Total: 35 / 44**
- **advance: true.** The total meets the threshold and no critical flag is set. The margin is zero.
  R1–R4 below concern how the new ablation is interpreted (what it varies, how the cross-session
  split is presented, which ratios validation dilutes, and the hardware condition behind the
  headline number). All four are wording or reporting fixes on data the package already holds. An
  auditor could escalate R1 or R3 if they are left as they are.
- **Critical flags: none** (see the critical-flag check below).
- **Outstanding dependencies: none.** `pending_marker` found 0 markers. The nine
  `[PENDING exp2b-fd-slsqp]` markers from the first v11 commit are filled.

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 4 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **35** |

## Mechanical checks

- **artifact_verify** (`python3 verify.py`, then `python3 verify_exp2b.py`, cwd `.11`): **both exit
  0**. `verify.py` printed 13 PASS lines, including its chained Exp 2b blocks, in 0.35 s.
  `verify_exp2b.py` printed "NOTE: raw ablation traces not supplied (EXP2B_SESSION/EXP2B_REFINE);
  checking the hash-bound extracts only." So the default run checks the hash-bound extracts. The
  changelog's raw-trace re-derivation needs the private session copy (see A5).
- **Build**: `papers/build.sh` on the copy exited 0. The log has 0 overfull boxes and 0
  undefined references or citations, and the PDF has no `??`. The PDF is 34 pages, and the rebuilt
  `main.bbl` is byte-identical to the committed one. Page layout:
  - Abstract and introduction on p. 1, the optimization study from p. 5 and the ablation on p. 10.
  - Discussion and Conclusion on p. 18, Code and data on p. 19.
  - Appendices pp. 19–30, references from p. 31.
- **numeric_consistency**: pass. Its only finding, at main.tex L354, is suppressed, and the
  suppression is now correct: 0.08036/0.08014 − 1 = 0.27%.
- **audience_check**: 0 hits across main.tex, optimization.tex and 10 table files.
- **Issue #1218 timing** (read-only GraphQL query): the body was created 2026-10-20 22:34:54Z with
  `userContentEdits.totalCount = 0` and `lastEditedAt = null`. The freeze comment was created at
  23:00:32Z and never edited. The paper's "the body was never edited" is therefore correct.
- **Manual claim-vs-claim pass over the new ablation numbers.** All of the following agree with
  Table 8 and the verifier:
  - F/A′ at T90 in space A: 2.45, 2.45, 2.50, 2.52, 2.45, geometric mean 2.474.
  - F/A′ at T99 in space B: 4.33, 3.84, 4.14, 4.17, 4.10, geometric mean 4.113.
  - Shares: log 2.474 / log 3.10 = 0.80, and log 4.113 / log 7.79 = 0.69.
  - B/F from the same starts: 1.215 in space A (five starts) and 1.91 in space B (four starts,
    since B is censored at LHS 3). Multiplied by 70.94/66.66 these give 1.29 and 2.03 in seconds.
  - A′/A: 0.95–1.04 at repeat 1.
- **One cross-check the paper does not make**, which supports its reading: excluding validation,
  F/A′ is 5.58 and 10.07. The per-step cost ratios are 1.53/0.29 = 5.3 and 3.32/0.31 = 10.7. With
  paths matched, the non-validation ratio is essentially the per-step gradient-cost ratio.

## Critical-flag check

- **Citation error**: none. All 29 cited keys resolve, and the 10 new entries carry DOIs where one
  exists.
- **Numerical inconsistency**: none at flag level. The abstract's "dilutes every ratio" is a wrong
  qualitative statement about D/A, not a number that conflicts with a table (R3).
- **Missing experiment for a claim**: not set. The v10 gap (gradient credit without an ablation) is
  closed by a measured, within-session F/A′ arm.
- **Close prior work ignored**: not set (D4 leads only).
- **Build / artifact**: clean.
- **Post-hoc result in the title**: considered and not set. Every place the ablation is introduced
  labels it post hoc. GitHub's edit history shows the protocol frozen before the first run, and
  the result's direction is mechanically predictable, which limits forking-path risk (see A1).

## Required findings (fix before audit)

**R1. The ablation does not vary "gradient access", and the decomposition labels its two factors
wrongly.** Method F has gradient access: it runs SLSQP on forward-difference gradients. What the
ablation varies is how the gradient is obtained and what it costs. The wrong label appears in five
places:
- optimization.tex L320–321: "This ablation varies gradient access alone."
- optimization.tex L28–30: "does not isolate gradient access as the only causal difference; the
  ablation of Section~\ref{sec:exp2-fd} does."
- Abstract, main.tex L34: "gradient access accounts for roughly 80\% and 70\%".
- Introduction, L82: "Gradient access is therefore the larger part of the SLSQP-versus-COBYQA
  advantage".
- Discussion, L579: "separates gradient access from search strategy".

The decomposition B/A = (B/F)(F/A′) has two factors:
- F/A′ is **adjoint versus finite-difference gradients**, with the optimizer fixed.
- B/F is **COBYQA versus SLSQP driven by finite-difference gradients**. That factor changes the
  search strategy *and* whether gradient information is used at all. It is not "the optimizer".

Required: replace "gradient access" with "the gradient source" or "the adjoint, versus finite
differences" throughout. State the shares as "the adjoint's cheaper gradients account for roughly
80% and 70% of log(B/A), and SLSQP-with-finite-differences over COBYQA for the rest". The title
already uses the correct contrast ("than finite differences"), so this is a body and abstract fix
only.

**R2. Report the per-start optimizer factor B/F. In space A it favors COBYQA at two of five
starts.** From Table 5 (main study B, session-1 units) and Table 8 (ablation F, session-2 units),
B/F at T90 in space A is:

| Start | B/F (units) | B/F (seconds) |
|---|---|---|
| θ0 | 1.23 | 1.31 |
| LHS 1 | 1.43 | 1.52 |
| LHS 2 | 0.70 | 0.75 |
| LHS 3 | 0.94 | 1.01 |
| LHS 4 | 2.26 | 2.41 |

At LHS 2 the derivative-free COBYQA reaches T90 sooner than finite-difference SLSQP, and at LHS 3
the two tie. In space B at T99, B/F is 3.83, 2.46, 1.26 and 1.12. F/A′, by contrast, lies in
2.45–2.52 and 3.84–4.33.

So "the optimizer contributes the rest" (Introduction L83; Conclusion L631: "the rest from the
optimizer") presents an aggregate as if it held at every start. Behind the aggregate, the optimizer
factor varies from 0.70 to 3.83 while the adjoint factor is nearly constant.

This also matters for the paper's own question. In the eight-parameter space, gradient-based search
*without an adjoint* is no reliably faster than COBYQA. That is a sharper answer to "when do field
gradients pay off" than the aggregate share, and the paper's data already contain it.

Required:
- Add the per-start B/F row, in both units, to Table 8 or to a sentence in §4.3.
- Qualify "the rest" in the Introduction and Conclusion.
- State that the space-B share pairs a five-start F/A′ (4.11) with a four-start B/A (7.79). On the
  four matched starts, F/A′ is 4.10, so the effect is negligible, but the reader cannot see that.
- State that the chain does not close exactly: 2.47 × 1.22 = 3.01 against 3.10 in units, and
  2.47 × 1.29 = 3.19 in seconds. "Roughly 80%" is better given as about 77–82% in space A and
  66–69% in space B.

**R3. The abstract's "dilutes every ratio" is false for the D/A contrast.** Abstract L35–37:
"Validating each new incumbent took 53\% of all charged time in the main study and dilutes every
ratio." In §4.2, removing validation lowers D/A from 1.68–1.83 to 1.15–1.16 in space A and from
2.25–4.32 to 1.92–3.19 in space B. Validation therefore *inflates* the cross-resolution A-versus-D
contrast; it does not compress it. The paragraph says this itself: "The space-A contrast between A
and D does not survive this decomposition" (optimization.tex L225–226).

Required: say "compresses the same-resolution and finite-difference ratios toward one and inflates
A's lead over the coarse competitor D". Alternatively, drop "every".

**R4. The title's 2.5–4× holds only at one core per run, and the paper never says so.** Each run
is a single-threaded process. F's n extra forwards (8 or 18) are charged serially. The stencil
solves are independent, so with n workers per run F's wall time per gradient would approach A′'s,
at n times the compute. The BRIEF asks to "allow valid reuse/parallelization for all methods" and
match core limits. One core per run does match core limits, so the comparison is fair, but it is
a compute-matched comparison, and "sooner" in the title depends on that.

The headline number also depends on the validate-every-incumbent rule. Without validation, F/A′ is
5.58 and 10.07, which equals the per-step cost ratio.

Required: one sentence in the §4.3 Protocol paragraph and one clause in the abstract stating that
every run uses one single-threaded process, so the finite-difference stencil is evaluated serially
and the ratio is equivalent to a compute ratio. Also add the non-validation ratios (5.6 and 10.1)
next to 2.47 and 4.11 in the abstract, or in §4.3's first results sentence, so a reader can tell
the protocol-diluted figure from the per-step gradient-cost figure.

## Advisory findings

- **A1 (D3/D9; title).** The title now headlines the post-hoc contrast, and that contrast is the
  most predictable result in the paper. F follows A′'s validation sequence at every start, so F/A′
  is fixed by the per-gradient cost ratio and the validation share. The preregistered SLSQP-vs-COBYQA
  study and the "adjoint is what makes gradient search pay" reading (R2) carry more information for
  the BRIEF's question.

  The title is defensible: it is scoped to "a nonlinear motor model", the ablation is labelled in
  the abstract, and the protocol was frozen before the runs. Still, a referee in this area will read
  "adjoint beats finite differences by 2.5–4×" as textbook, unless the paper makes explicit what the
  experiment adds:
  - FD noise at h = 1e-6 does not change the optimization path.
  - Validation dilutes the gain.
  - Without the adjoint, SLSQP's advantage over COBYQA largely vanishes in 8-D.

  Consider a title that carries the decision result, for example "...: the adjoint, not the
  gradient-based optimizer, accounts for most of SLSQP's lead". Also make the Conclusion say "post
  hoc" once. It is the only summary section that does not.
- **A2 (D2/D1; §4.3, App. C).** F's gradient accuracy is reported only at L0: "On the L0 model, F's
  gradients agree with the adjoint within...". F trains on L1, at Newton tolerance 1e-8. Report the
  L1 accuracy, or say why the L0 wiring check stands in for it. The matched validation counts are
  indirect evidence that it does. Also, "a step of 10^{-6} in each component's unit" is ambiguous
  between physical units (m, degrees) and the optimizer's unit box. Appendix F's "stored unit (m,
  degrees or dimensionless scale)" suggests the former; say so in §4.3.
- **A3 (D2; §4.3).** Add the consistency check from the Mechanical checks above. The non-validation
  F/A′ (5.58, 10.07) matches the per-step cost ratio (1.53/0.29 = 5.3, 3.32/0.31 = 10.7). That one
  sentence shows the gap is gradient cost with paths matched.

  Separately, an A′ training step costs 0.29 to 0.31 units, against about 0.17 per L1 forward
  implied by F's step: (1.53 − 0.29)/8 ≈ 0.155, and 3.32/19 ≈ 0.175. That is about 1.7 forwards per
  adjoint step, not the 1.13–1.20 of §6.4. Most likely the training gradient covers more outputs and
  angles than the 10-angle timing study, but say so, because §6.4 is the paper's only stated adjoint
  cost.
- **A4 (D4; §1–2).** These are leads for `paper-litsearch`, not verified citations. The title's
  contrast is the derivative-computation question, and none of the 29 references addresses it
  directly. Candidates:
  - Martins & Hwang (2013), "Review and unification of methods for computing derivatives of
    multidisciplinary computational models", AIAA J. 51(11).
  - Giles & Pierce (2000), "An introduction to the adjoint approach to design", Flow Turbul.
    Combust. 65.
  - For nonlinear magnetostatic devices: Park, Lee & Hahn (1992), design sensitivity analysis for
    nonlinear magnetostatic problems by the adjoint variable method (IEEE Trans. Magn. 28(2)). This
    is the classic CEM adjoint shape-sensitivity reference, older than Gangl et al. 2015.

  Lyu, Xu & Martins (2014) remains a lead; the changelog notes that no DOI record was found.
- **A5 (D5; Code and data).** The raw traces of both studies are not public. They are held in "two
  verified copies and an EBS snapshot", and the package has extracts bound to them by hash. Without
  the private session copy, `verify_exp2b.py` checks extracts only. The package itself is named by
  path at the repository root, not at a pinned commit. Deposit the two inventories' raw trees (5,453
  and 2,118 files) with the package on Zenodo, or pin the package path to a commit, so the
  re-derivation path is open to readers.
- **A6 (D6; Tables 2, 8, 9, 10).**
  - Table 9's caption, "Validation charges in the ablation, repeat~1, as in Table~\ref{...}", does
    not stand alone. Repeat the column definitions or give a one-line gloss.
  - The text's "66--69\% of A$'$'s time to the primary target" is a primary-target median (0.69 and
    0.66 in `verify_exp2b.py` L112). The table reports all-target medians (0.65, 0.61), so a reader
    cannot find 66–69% in it. Add a primary-target column, or cite the all-target medians.
  - Fig. 3's x-tick labels collide across panels ("120" against the next panel's "1").
- **A7 (D7).** Methods A–F and spaces A/B share letters. "B/A in space A" or "space-B LHS 1 ... B"
  makes the reader stop and decode. Consider S8/S18 for the spaces, or set method names in a
  distinct font.
- **A8 (D7; main.tex L423).** "Section~\ref{sec:exp2-refinement} reports the corresponding
  training-to-validation ratios" points to §4.2, which now only forwards to Appendix C. Point
  directly to Appendix C.
- **A9 (D9).** The paper is 34 pages; main text plus data statement runs pp. 1–19. The BRIEF
  targets 10–15 pages. The abstract is about 330 words and still closes with the interface-order
  finding. Its last sentence ("Validation uses a higher interface order than training because...")
  could move to the introduction, which already makes that point. The Conclusion repeats the 3.6×
  and 96× figures from §5.4.

## Per-dimension justification (quoted evidence)

**D1 Rigor 5/6.** The ablation is well designed:
- One arm changes only the gradient source: "Only the gradients of the objective and of the mean
  constraint change" (§4.3).
- A same-session control, A′, shares host, unit and load.
- The protocol was frozen before the runs, as GitHub's edit history confirms.
- A primary estimand is fixed in advance, and censoring is handled the same way as in the main
  study.

The main study's v10 gap is closed by the exact validation decomposition, "Excluding the validation
charges up to each crossing" (§4.2). One point is deducted for the interpretation layer added in
v11:
- The ablation is described as varying "gradient access" when F has gradients (R1).
- The cross-session split hides per-start sign changes (R2), though its approximation is
  acknowledged: "These shares are approximate: the comparison spans two sessions with different
  time units" (§4.3).
- The abstract's validation claim is wrong for D/A (R3).

**D2 Evidence 5/6.** The evidence covers:
- 100 + 40 hash-bound runs, 213 + 90 final checks, and no censored ablation cell.
- Repeat agreement: 2.49 and 4.04 against 2.47 and 4.11.
- Training gradients now verified in both spaces: "the gradients that A and E use were checked
  separately" (§4.1).

One point is deducted. The study is one motor with five deterministic starts. The BRIEF's strongest
competitor class is untested: "No reduced physical model, such as a magnetic equivalent circuit or a
waveform model, was compared" (§7). F's accuracy is shown only on the model it does not train on:
"On the L0 model, F's gradients agree with the adjoint within" (§4.3; A2).

**D3 Clarity 4/5.** The contribution can be extracted from the abstract and introduction: "the
contribution is the measured time-to-target evidence for adjoint gradients, separated from the
choice of optimizer" (§1). The title is result-based and correctly attributes the result to the
adjoint over finite differences: "Adjoint gradients reach validated torque-ripple targets" (title).
One point is deducted for two reasons:
- The headline number depends on serial stencil evaluation and per-incumbent validation, and the
  paper does not state either condition (R4).
- The decision-relevant finding, that FD-driven SLSQP is not reliably faster than COBYQA in 8-D, is
  absent. In its place the introduction gives an aggregate attribution under a mislabelled factor:
  "This ablation varies gradient access alone." (§4.3; R1, R2, A1).

**D4 Related work 4/5.** The v10 leads were added and positioned. Gangl et al. 2015, Zingg et al.
2008, Duan & Ionel and Bramerdorfer et al. are now in the paper. The paper also frames its headline
direction correctly as expected: "so the direction of an advantage is expected" (§1). One point is
deducted (A4). The title's contrast, adjoint versus finite-difference gradient cost, is not placed
against the derivative-computation literature (Martins & Hwang; Giles & Pierce). The classic
adjoint-variable sensitivity work for nonlinear magnetostatic devices, which predates Gangl, is also
absent. Not at flag level: the paper claims no novelty for the method.

**D5 Reproducibility 4/5.** The paper provides:
- Pinned commits for both evaluators (`9486a9da`, `5cc86ad8`) and for the refinement.
- An appendix table of optimizer settings and the exported preregistration and protocol.
- RFC 3161-timestamped inventories and three passing stdlib verifiers.

One point is deducted. The raw traces behind both studies are not public. The package carries
extracts: "For the post-hoc ablation, the package holds the 40 run extracts" (Code and data). The
default verifier run checks only those extracts, and the package is named by path, not by pinned
commit or DOI (A5).

**D6 Figures/tables 3/4.**
- The v10 figure fixes hold in my 70-dpi render: Fig. 2's log axis and budget lines, and Fig. 7's
  scale note.
- The new ablation tables are clean, with geometric means and repeat 2 in one column.

One point is deducted for one weakness that repeats across the new tables. Captions defer to other
tables instead of standing alone: "Validation charges in the ablation" ... "as in" Table 2 (App. C).
The text's 66–69% primary-target shares cannot be found in Table 9. Fig. 3's tick labels collide
(A6).

**D7 Prose & structure 3/4.** The prose is otherwise clean:
- The v10 stale sentences are gone. The build is clean.
- No virtue words, no "load-bearing", and no em-dashes in the body.
- audience_check reports 0 hits.

One point is deducted:
- An internal team word survives that voice.md lists by name: "frozen by a ruling comment" (§4.3).
- The "gradient access" mislabel recurs across five sections (R1).
- Method and space letters collide (A7).
- A cross-reference forwards twice (A8).

**D8 Citation hygiene 4/5.** All 29 entries have author, title and year, and DOIs are present
except for the thesis, technical report and manual entries. The v10 software gap (Gmsh, GeoPDEs,
Octave, NumPy, LHS) is closed, and `citation-notes.md` records how each new entry was checked. One
point is deducted. The paper's central cost premise is cited to a motor-application paper, not to a
derivative-method source: "An adjoint gives the sensitivities of a few outputs to many design
parameters at the cost of one linear solve per output and operating state" (§1). The adjoint-vs-FD
contrast in the title has no methodological citation (A4).

**D9 Rhetorical economy 3/4.** The lede placement is much better: the matched study starts on p. 5
(v10: p. 15), the introduction opens with the decision, and material moved to appendices
intact. One point is deducted:
- The paper is still 34 pages against a 10–15-page target.
- The abstract runs about 330 words and still ends on a forward-model detail: "Validation uses a
  higher interface order than training because the ripple objective depends on it" (Abstract).
- One of its summary clauses is wrong: "and dilutes every ratio" (Abstract; R3).

## Resolution of v10 findings (checked, not carried over)

| v10 finding | Status in v11 |
|---|---|
| Both R1: title and conclusion credit gradients without an ablation | **Resolved by experiment.** Post-hoc FD-SLSQP arm F with a same-session control; the title now names the measured contrast. New interpretation issues follow from it (R1–R4 above) |
| Opus R2: validation dominates; D/A fragile | **Resolved.** Table 2 plus the exact decomposition. Space-A D/A is qualified ("does not survive this decomposition"); the Discussion no longer says "D does not close that gap". The abstract's "every ratio" overreaches (R3) |
| Opus R3: space-B gradients unverified in the paper | **Resolved.** §4.1 "Training gradients" with pinned records; the stale L612–614 sentence now points to §4.1 |
| Opus R4: stale sentences (L636–637, L757–759) | **Resolved.** §6.1 states fixed physical current for every method; App. F says setup is charged |
| Fable R2: method C underspecified | **Resolved.** §4.1 and Table 4; the Discussion bounds C's result to this configuration |
| Fable R3: space-B dependence buried | **Resolved.** Per-space ranges and the mesh-motion confound are in the abstract and introduction |
| Fable R4: conclusion restates the abstract | **Largely resolved.** Rewritten around what to check; it still repeats 3.6× and 96× (A9) |
| A1 (litsearch leads) | **Resolved** for Gangl, Zingg, Duan, Bramerdorfer. New derivative-method leads in A4 |
| A2 (uncited software) | **Resolved** |
| A3 (package locator, mutable preregistration) | **Partly resolved.** The preregistration export is in the package; the package path is unpinned and the raw traces are private (A5) |
| A4 (optimizer settings) | **Resolved** (Table 4) |
| A5 (capture-tool wording, "certify") | **Resolved.** A new internal word, "ruling", was introduced in §4.3 (D7) |
| A6 (length, lede) | **Partly resolved.** Lede moved to p. 5; still 34 pages (A9) |
| A7 (F_train/F_val at selected designs) | **Resolved** (App. C paragraph) |
| A8 (introduction opening) | **Resolved** |
| A9, A10 (wording) | **Resolved** |
| numeric_consistency suppression | **Resolved** (suppressed correctly) |

## Procedural notes

- Read:
  - all of main.tex (1111 lines), optimization.tex (393 lines), all `tables/*.tex` and refs.bib;
  - `changelog.md`, `citation-notes.md`, `README.md`, and the `verify_exp2b.py` share and
    decomposition assertions;
  - `../BRIEF.md`, `../.anvil.json`, the v10 `verdict.md` and `review-opus.md`;
  - issue #1218, with body, comments and GraphQL edit history.
- Ran the following on the scratch copy:
  - `verify.py` and `verify_exp2b.py`;
  - `papers/build.sh`, plus a 70-dpi render of every page, of which I inspected pp. 1, 7, 9, 10, 25
    and 27;
  - `anvil.lib.numeric_consistency`, `pending_marker` and `audience_check`, without
    `--write-review`.
- The B/F per-start values in R2 are my arithmetic on the published Tables 5 and 8. They cross two
  sessions and are approximate in the same sense the paper states.
- No email or identity data was sent anywhere. The only network calls were read-only `gh`
  queries for issue #1218. No review sibling dir was written: this report is the reviewer's
  output for the coordinator, who owns the `.review/` sidecar.
