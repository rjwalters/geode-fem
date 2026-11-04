# paper-review: motor-gradient-optimization.12 (branch claude/motor-paper-v11, e7caaa43)

Reviewer: Fable 5.1, in the anvil:paper-review role (read-only). Independent, skeptical referee from computational electromagnetics and electric-machine design optimization. This is a fresh score of v12; the v11 scores (Opus 35/44, Fable 36/44) do not carry over. I read the v11 reviews and v12's `changelog.md` only to check whether each v11 finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md` (9 dimensions, /44, advance at >= 35, `anvil-pub-v2`). Voice: `.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers". `.anvil.json` declares no `venue`, so no venue overlay was scored. No litsearch sibling exists, so D4 is scored on the legacy baseline. BRIEF sets `web_search: true`, but this session has no web tool; the D4 leads below are from domain recall and are tagged for `paper-litsearch`.

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory and `papers/build.sh` were copied to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v12-review/fable-work/` and every build, verifier and lint ran there. The harness source was read from the main checkout (read-only). Network: read-only `gh` queries for issue #1218 only. No email address or identity data was sent anywhere. Per the coordinator's constraints this report is written here instead of a `.12.review/` sibling; it follows the `verdict.md` / `scoring.md` / `comments.md` / `findings.md` content of `paper-review.md`.

## Inputs read

- `main.tex` (1,166 lines) with its resolved `\input` children `optimization.tex` (363 lines) and the 13 `tables/*.tex`; `refs.bib` (35 entries); `main.pdf` (34 pages, rendered to PNG; pp. 1, 2, 9, 10, 11, 12, 13, 14, 15, 22, 23 inspected); `build.log`; `README.md`; `changelog.md`; `citation-notes.md`; `evidence-manifest.json`; `_progress.json`; `verify.py`, `verify_exp2.py`, `verify_exp2b.py`; `figures/src/data/exp2b/{runs,fd-gradient-runtime-checks,fd-gradient-runtime-checks-L1}.json`; `figures/src/data/exp2/{runs,supplement}.json`; `figures/src/render_exp2b_figure.py`.
- `../BRIEF.md`, `../EXPERIMENTS.md` (status sections), `../.anvil.json`.
- `.11.review/{verdict,review-opus,review-fable}.md`.
- GitHub issue rjwalters/geode-fem#1218 (body, freeze comment, run-complete comment and its correction) and its GraphQL edit history; `benchmarks/motor_optimization/harness/driver.py` (`fd_gradient`, `FD_STEP`); commit `74843242` (PR #1222).
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.

## Deterministic pre-flight (scratch copy)

| Check | Result |
|---|---|
| `python3 -I -B verify.py` (`artifact_verify` command 1, cwd `.12`) | exit 0; 13 PASS blocks, including the chained Exp 2b blocks and the new L1 block |
| `python3 -I -B verify_exp2b.py` (command 2) | exit 0; 5 PASS blocks; raw-trace re-derivation not run (`EXP2B_SESSION` unset, as expected in the package) |
| Build (`papers/build.sh`) | exit 0; 3 pdflatex passes; 0 overfull boxes; 0 undefined references or citations; `pdftotext` shows no `??`; 34 pages (main text pp. 1-14, Code and data p. 15, appendices A-J pp. 15-30, references from p. 31) |
| `\cite` keys vs `refs.bib` | 35 cited, 35 defined, 0 missing, 0 unused |
| `anvil.lib.numeric_consistency` | pass; 1 info finding at main.tex:849, suppressed by the correctly placed directive (the known 0.08036/0.08014 false positive) |
| `anvil.lib.pending_marker` | 0 markers; no outstanding dependency |
| `anvil.lib.audience_check` | 0 governance, 0 private-locator, 0 unlinked-artifact hits across 12 files |
| `anvil.lib.evidence_drift` | CLEAN (BRIEF and refs mtimes equal the snapshot) |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `emphatic`, `ruling`, `Judge`, `Doctor`, `tracker`, `earlier version`, `gradient access`, `dilutes every`, `logarithm of`, `first` as novelty, em-dashes) | 0 hits in the body |
| Abstract length | 238 words (v11: about 450) |
| L1 record pin | `fd-gradient-runtime-checks-L1.json` SHA-256 `5ecb87d3...be94` equals `git show 74843242:benchmarks/motor_optimization/harness/fd-gradient-runtime-checks-L1.json`; `74843242` is an ancestor of `origin/main` |
| Protocol timing (GraphQL) | issue #1218 body created 2026-10-20T22:34:54Z, `userContentEdits.totalCount = 0`, `lastEditedAt = null`; freeze comment 23:00:32Z, 0 edits. The paper's "the protocol text was never edited" is correct. (The 48-times-edited comment is Loom's lease record, not protocol text.) |

## Independent cross-checks of the ablation claims (recomputed from the hash-bound extracts, not from the verifier)

Every number below agrees with the text and Tables 8-10.

- F/A' at T90 in space A: 2.446, 2.450, 2.503, 2.524, 2.450, geometric mean 2.474; at T99 in space B: 4.329, 3.840, 4.144, 4.169, 4.101, mean 4.113; repeat 2: 2.493 and 4.045. F crosses the primary target at the same validation index as A' from every start in both spaces (1/1, 1/1, 5/5, 4/4, 6/6; 3/3, 4/4, 6/6, 9/9, 9/9).
- Matched-start sets: B attains the primary target from all five space-A starts and from four space-B starts (LHS 3 censored). F/A' over the matched space-B starts is 4.100. B/F per start in session units: 1.245, 1.445, 0.703, 0.948, 2.267 (gm 1.221) and 3.837, 2.460, 1.256, 1.117 (gm 1.908); in seconds (x 70.936/66.659 = 1.064): 1.300 and 2.030. Main-study B/A: 3.098 and 7.787. Chain (F/A')(B/F): 3.022 and 7.821.
- A'/A at the primary targets: 1.010-1.043 in space A, 0.944-1.024 in space B; the paper's 0.94-1.04 holds.
- Excluding validation charges up to the crossing: F/A' = 5.581 and 10.071.
- Step costs, median (forward + line search + adjoint)/train_events: F 1.533 and 3.323 units; A' 0.289 and 0.3055. Ratios 5.30 and 10.88. Median (forward + line search) per solve, stencil solves included: 0.233-0.245 for both arms. Adjoint per A' step: 0.057 and 0.061 units.
- Validation share of time to crossing, medians over all attained targets: A' 0.653 and 0.609; F 0.271 and 0.161. The text's "61-65%" and "16-27%" now cite the same statistic as Table 9.
- Stop reasons: space A, F budget 8 / converged 2; A' converged 4 / solver_stopped 4 / budget 2. Space B: all 20 at T99.
- Best credited validated objective (credit = status ok, credited, admissible and mean floor met, the verifier's definition and the paper's Section 4 rule): F within -0.32% to +0.08% of A' from every space-A start. (Dropping the mean-floor condition would give +1.34% at LHS 1; the paper's definition is the stated one, so no finding.)
- Space-B path: validation counts equal at all five starts; training evaluations equal at four (LHS 2: 10 vs 9); paired validated objectives differ by at most 0.4%, 0.3%, 28.8%, 0.8%, 17.5%.
- Backward steps: 359 of 4,283 stencil components (8.38%) across 446 F gradients; 0 base re-solves; 0 refusals.
- L1 FD record (pinned `74843242`): norm-wise max-norm relative errors for F 5.40e-4, 5.22e-4 (space A), 1.93e-3, 1.83e-3 (space B); for the mean 1.03e-4, 1.10e-4, 7.17e-4, 6.30e-4. Exactly one component fails the 0.1 limit: SW3 of the mean gradient at the space-A interior point, FD -9.620e-3 vs adjoint -7.740e-3, relative 0.243, |adjoint| 8.74e-5 of the largest component, absolute error 1.88e-3; SW3's absolute errors at the three passing points are 1.81e-3, 1.85e-3, 1.89e-3. The text's reading (a fixed-step noise floor on a small component) is consistent with these values and is labelled as untested by a step sweep.
- Driver: `FD_STEP = 1e-6` is applied to the physical coordinate vector `P.x_of(u)`; stencil points are solved one after another in the run's process; the backward step is tried when the forward step is unusable.
- Fig. 5 (`render_exp2b_figure.py`): minutes = `wall_fe * unit_seconds / 60` per run, so B's main-study times use the 70.94 s unit and the ablation's the 66.66 s unit, as the caption says.

No numerical inconsistency between text, tables and data was found. No critical flag of any class.

## Verdict

**Total: 37 / 44**
**Decision: advance: true** (total >= 35 and no critical flag).

Critical flags: none.

Outstanding dependencies: none (0 pending markers).

Evidence drift: none.

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **37** |

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The ablation is the right experiment, run with one variable changed, an in-session control, a frozen estimand and the censoring rule kept; v11's interpretation defects are gone: the per-core condition is stated where the ratio is defined ("Every run, F and A$'$ alike, is one single-threaded process, so F's $n$ stencil solves run one after another and the time ratio is also a ratio of compute" — §5 Protocol), the factor labels are right, the per-start optimizer factor is reported, and F's gradients are now checked on the model it trains on, with the one failing component reported rather than hidden. One point off for a specific gap the paper itself names: the F arm is a single fixed step, and its noise floor is inferred, not measured ("an interpretation that no step sweep has tested" — §5 Results). A step sweep at the failing point (two extra steps, 16 L1 solves) would close the only open question in "the gap is the cost of the gradient". Secondary, disclosed and unchanged: five deterministic starts of one motor; the chain across sessions closes only approximately; 15 unpinned concurrent processes. |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 hash-bound runs, every ablation cell attained ("Both F and A$'$ reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts, in both repeats" — §5 Results), repeats within noise, selected designs refined at L3, the L1 gradient check present, stop reasons reported, the per-start factor given with its sign changes, and a figure for the title's contrast. One point off: the brief's strongest competitor class is still untested ("No reduced physical model, such as a magnetic equivalent circuit or a waveform model, was compared" — §7), and the title's result rests on one motor, five starts and one finite-difference configuration. |
| 3 | Clarity of contribution | 5 | 5 | Cold-reader test passes on the title, abstract and introduction alone: one measured claim with its conditions (one core per run, post hoc, frozen protocol), the contribution stated in one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1), and the decision result that v11 lacked now in the introduction ("in the smaller space, gradient-based search without an adjoint is not reliably faster than the derivative-free method" — §1). The abstract's first results sentence for the ablation opens with its label ("A post-hoc ablation, not preregistered but run under a protocol frozen beforehand" — Abstract). No overclaiming and no buried lede: the direction is called expected and the size is the contribution. |
| 4 | Related-work positioning | 5 | 4 | The derivative-method and nonlinear-magnetostatic adjoint literature the title's contrast needed is now in and positioned on its merits ("These works establish why an adjoint gradient is cheaper than a finite-difference one; Section~\ref{sec:exp2-fd} measures what that saving is worth in time to a validated target" — §2), and no method novelty is claimed. One point off, legacy baseline (no litsearch sibling): the section still engages no prior measured finite-difference-versus-adjoint optimization comparison (the only cost benchmark cited is genetic-algorithm-versus-adjoint), and the SW3 noise-floor reading in §5 is made without reference to the computational-noise and FD-step literature that gives it a method (leads in A2). Not a flag. |
| 5 | Reproducibility | 5 | 4 | Evaluator, harness, refinement and L1-check commits pinned; settings table; protocol export with GitHub timestamps verified here; three stdlib verifiers with an optional raw re-derivation; F's step and stencil solves now specified ("a step of $10^{-6}$ in each coordinate's physical unit (metres for the lengths and offsets), reusing the base value" — §5 Protocol). One point off for a concrete defect: the package pointer names the previous version ("papers/motor-gradient-optimization/motor-gradient-optimization.11" — Code and data availability); a reader following it finds a package whose README describes v11, without the L1 record or the v12 verifier. Secondary and disclosed: the raw trees are "retained in two verified copies each" (Code and data availability), not deposited, so the default verifier run checks extracts only. |
| 6 | Figure & table quality | 4 | 3 | The title's evidence now has a figure ("Charged wall time, in minutes, to the primary target from each start" — Fig. 5 caption) with a log axis, units, the censored B cell marked and a self-contained caption; Table 9's caption now defines its columns; the Table 10 white space is gone; Fig. 4's ticks read 1/10/100. One specific weakness: Fig. 5 plots absolute minutes over two decades, so the estimand, the per-start ratio F/A', is read as a small vertical offset and cannot be checked against Table 8 without a calculator; a ratio panel or the per-start ratio printed above each pair would carry the title's number. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no virtue words, no structural-importance announcements, no em-dashes, no version history, no internal team words, audience check clean. One point off: the §5 Results paragraph runs the headline, the no-validation ratios, the step-cost decomposition ("A stencil solve costs a median of 0.23 units, like any training solve, while the adjoint adds 0.06 units per step" — §5 Results), the L1 accuracy check with its failing component and the A'-versus-A control together in one 20-line paragraph; the gradient-accuracy check wants its own paragraph. The method/space letter collision ("F/A$'$ ... in space A") remains a decoding cost. |
| 8 | Citation hygiene | 5 | 5 | All 35 `\cite` keys resolve; every entry carries author, title, venue and year, with DOIs where they exist; the six v12 additions are verified in `citation-notes.md` with no identifier sent, and the two without retrievable abstracts are cited only for what their titles state. No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 3 | Main text 14 pages (v11: 18), abstract 238 words, the headline stated once each in the title, abstract, introduction, §5 and conclusion, the Discussion without numbers, hedging proportional. One point off: the introduction's second paragraph restates the abstract's results nearly sentence for sentence ("SLSQP with adjoint gradients reaches every target that both reach sooner than COBYQA trained on the same mesh" — §1, after the same claim with the same ranges in the Abstract), and the document is 34 pages against the brief's 10-15; the appendices are evidence a skeptical reader wants, so this is noted, not pressed. |

## Critical-flag check

- **Citation error**: none. 35 keys resolve; the six new entries are complete with DOIs.
- **Numerical inconsistency**: none. Every ablation number recomputed above agrees with the text and tables; the L1 record's values and its single FAIL are reproduced from the raw FD and adjoint components. One presentation nit (R2): the printed step costs 3.32 and 0.31 give 10.7, while the text's 10.9 comes from unrounded medians (3.323/0.3055 = 10.88).
- **Missing experiment for a claim**: not set. The v11 gap (F's accuracy unmeasured on the model it trains on) is closed by the pinned L1 record.
- **Close prior work ignored**: not set (D4 leads only).
- **Build / artifact**: clean; both declared `artifact_verify` commands exit 0.
- **Post-hoc result in the title**: considered, not set. The label appears at the ablation's first mention in the abstract, introduction, §5 and conclusion; the freeze precedes the first run by three hours by GitHub's own timestamps and the protocol text has no edits.
- **Audience fit**: no hits; no flag.

## Findings

### Required (address before the next review or the audit)

- **R1. Package pointer names the previous version** (`main.tex` L456, Code and data availability: "The paper package is the directory `papers/motor-gradient-optimization/motor-gradient-optimization.11`"). V12 is the package that holds `fd-gradient-runtime-checks-L1.json`, the five-block `verify_exp2b.py` and the rewritten README; `.11`'s README still describes v11. Change to `.12` (or to whatever the deposited directory is called) and consider binding the path in `verify_exp2b.py` so it cannot go stale again. D5/D7.
- **R2. Step-cost ratio does not follow from the printed inputs** (`optimization.tex` L303-305: "close to the ratios of median training-step cost, 5.3 and 10.9: one F step costs 1.53 and 3.32 units, an A$'$ step 0.29 and 0.31"). From the printed numbers a reader gets 3.32/0.31 = 10.7; the 10.9 is 3.323/0.3055. Either print 0.305 (or 0.31 and 10.7) or say the ratios use unrounded medians. An auditor's claim-vs-claim check will stop here otherwise. D7, small.

### Advisory

- **A1. Fig. 5: show the estimand** (`optimization.tex` L311-320; `figures/src/render_exp2b_figure.py`). Print the per-start F/A' ratio above each A'/F pair, or add a narrow third panel of ratios on a linear axis with the 2.47 and 4.11 means as lines, so the title's numbers are visible in a figure and checkable against Table 8. D6.
- **A2. Related-work leads (tag: related-work; verify via `paper-litsearch`, not by the reviser).** From recall, no live search this session. (i) Peter and Dwight, "Numerical sensitivity analysis for aerodynamic optimization: A survey of approaches", Computers & Fluids 39(3), 2010: cost and accuracy comparison of finite differences, complex step and adjoints in a design-optimization setting, the measured counterpart of Martins and Hwang's unification. (ii) More and Wild, "Estimating computational noise", SIAM J. Sci. Comput. 33(3), 2011, and "Estimating derivatives of noisy simulations", ACM Trans. Math. Softw. 38(3), 2012: the standard method for choosing a finite-difference step from a simulation's noise level; citing it would turn §5's noise-floor reading of the SW3 failure from an interpretation into a testable one. (iii) Kenway, Mader, He and Martins, "Effective adjoint approaches for computational fluid dynamics", Prog. Aerosp. Sci. 110, 2019: a review that reports measured adjoint-versus-FD costs in optimization. D4.
- **A3. Interpret F's budget stops** (`optimization.tex` L326-328: "In space A, F stops at the budget in eight of ten runs and converges in two, whereas A$'$ converges in four"). SLSQP's `ftol` of $10^{-15}$ (Table 4) is unreachable with forward-difference gradients whose per-component noise is about $2\times10^{-3}$, so F keeps iterating after the crossing until the budget ends it. One clause saying so would tell the reader that "with the optimizer fixed" means fixed settings, that the post-crossing behavior differs for a known reason, and that it does not touch the time-to-target comparison (crossing indices match at every start). D1.
- **A4. Step sweep at the failing point** (§5 Results; App. D "Ablation session and finite-difference accuracy"). The paper says its noise-floor reading is untested. Two extra steps ($10^{-5}$ and $10^{-7}$ m) for the space-A interior point cost 16 L1 solves and would either confirm the floor or show truncation error. Also state in App. D whether the L1 check applies the step to the same physical coordinates as the session (it does, per the record's `step` field). D1.
- **A5. Introduction restates the abstract** (`main.tex` L55-71). The second paragraph repeats the abstract's main-study ranges, the post-hoc sentence and the 2.47/4.11 numbers. Keep the two-step structure, drop the duplicated ranges, and let §5 carry the numbers. D9.
- **A6. Split the §5 Results paragraph** (`optimization.tex` L293-343). Headline and path (to "pulls the measured ratio toward one"), then a "Gradient accuracy" paragraph for the L1 check and its failing component, then the control check. D7.
- **A7. Letter collision** (throughout §5: "F/A$'$ ... in space A"). Declined in v12 as high-risk; noted only. A one-line "Spaces are written S8 and S18 below" at the top of §4 and a global substitution in `optimization.tex` and three table files would do it, but the risk assessment is the author's. D7 nit.
- **A8. Package metadata is stale** (`motor-gradient-optimization.12/_progress.json`, `metadata.note`: "L1 FD-accuracy measurement (coordinator item 2) not run: requires harness code change; reported back"). The second v12 commit did run and report it (changelog addendum, README). Not in the paper; fix at the next revision so the package's own record agrees with its contents. D5 nit.
- **A9. Verifier count** (Code and data availability: "two standard-library verifiers that recompute the reported numbers", then "a third verifier"). Say three once, or name them. D7 nit.
- **A10. Raw traces** (Code and data availability). Unchanged from v11 (Opus A5): the raw trees are private; a Zenodo deposit with a DOI, or at least a pinned commit for the package, is the remaining reproducibility step and is the operator's call. D5.

## v11 findings: resolution check

| v11 finding (merged list and per-reviewer) | v12 status |
|---|---|
| Merged R1 (Opus R4, Fable R1): serial-FD framing, per-core ratio, no-validation ratios | **Resolved.** Abstract ("With every run on one single-threaded core, so that the finite-difference stencil is solved serially"), §5 Protocol (ratio of compute; parallel stencil shortens wall time, not compute), §7 first finding; 5.58/10.07 beside 2.47/4.11 in the abstract and §5 |
| Merged R2 (Opus R1): "gradient access" mislabel | **Resolved.** The phrase is gone (verifier asserts absence); the ablation "changes only how SLSQP obtains its gradient, and therefore what the gradient costs"; B/F is "COBYQA against SLSQP driven by finite differences" |
| Merged R3 (Opus R2, Fable R4/A6): per-start B/F, drop log share, matched starts | **Resolved.** §5 "The optimizer factor": matched start sets stated, per-start B/F in units and seconds, geometric means 1.22/1.91 and 1.30/2.03, chain closure 3.02/7.82 vs 3.10/7.79, F/A' over the matched four = 4.10, the 2-of-5 sign change named; log share removed. All recomputed here |
| Merged R4 (Opus R3): "dilutes every ratio" | **Resolved.** Abstract no longer says it; §7 states the correct direction (compresses same-resolution ratios, inflates A's lead over D) |
| Merged R5 (Fable R2): title evidence only in an appendix | **Resolved.** Fig. 5 in §5 with A', F and B per start; Table 8 stays in App. D |
| Merged R6 (Fable R3): L1 accuracy and stop reasons | **Resolved.** L1 record pinned at `74843242`, norm-wise pass and the single component FAIL reported with values; stop reasons for both arms in both spaces; "measured at L0 only" statements gone |
| Merged R7 (Fable R4, Opus A9): abstract and length, headline repetition | **Resolved.** Abstract 238 words; main text 14 pages; headline once per section; Discussion without numbers; conclusion says "post-hoc" |
| Opus A1: title carries the predictable contrast | **Addressed.** Title kept with exact numbers; the decision result (adjoint, more than the optimizer; FD-SLSQP not reliably faster than COBYQA in 8-D) is in the abstract, introduction, §7 and conclusion |
| Opus A2 / Fable A2: step unit, warm start | **Resolved.** "metres for the lengths and offsets"; "Each stencil solve is an ordinary training solve on the base point's mesh generation"; per-solve cost 0.23 given |
| Opus A3: no-validation vs step-cost consistency; adjoint cost | **Resolved** (5.58/10.07 vs 5.3/10.9; adjoint 0.06 units per step). R2 above is a presentation residue of this fix |
| Opus A4 / Fable A5: derivative-method references | **Resolved.** Martins and Hwang, Giles and Pierce, Park 1992, Nikolova 2004, Kuci 2016/2017 added, verified, positioned in §2 and §1 |
| Opus A5: raw traces, unpinned package | **Partly.** Retention stated; deposit left to the operator (A10). The package path is now wrong (R1) |
| Opus A6 / Fable A3, A4: Table 9 caption, text-vs-table statistic, Table 10 white space, Fig. 3 ticks | **Resolved** (caption self-contained; text cites all-target medians 61-65% / 16-27%; p. 23 full; Fig. 4 ticks 1/10/100) |
| Opus A7: method/space letter collision | **Declined** by the author; noted as A7 |
| Opus A8: forwarding cross-reference | **Resolved** (points to App. D) |
| Opus D7 / voice: "ruling" | **Resolved** (absent; verifier asserts) |
| Fable A1: stale package README | **Resolved** |
| Fable A7: provenance language in the body | **Resolved** (moved to Code and data) |
| Fable A8: repetition of the headline | **Largely resolved** (A5 above is what remains) |
| Fable A9: title rounding | **Resolved** ("2.5 and 4.1 times") |
| Fable A10: abstract framing of the post-hoc arm | **Resolved** |
| Fable A11: length | **Partly** (34 pages; main text 14) |
| Fable D1 secondary: 15 unpinned concurrent processes | Unchanged, disclosed |

## Rubric version transition

The prior review sibling (`.11.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers, lints and the recomputation scripts ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v12-review/fable-work/`; rendered pages are in `fable-png/` there. The worktree and git state were not touched.
- `anvil.lib.evidence_check` was run against the scoring table above (extracted to `fable-stage/scoring.md` in the review scratch dir): 9 dimensions checked, 0 findings, pass.
- No web search tool was available in this session; the D4 leads are from recall and are tagged for `paper-litsearch`.
- No review sibling dir was written: this report is the reviewer's output for the coordinator, who owns the `.review/` sidecar.
