# paper-review: motor-gradient-optimization.13 (branch claude/motor-paper-v11, 6e4544fc)

Reviewer: Fable 5.1, in the anvil:paper-review role (read-only). Independent, skeptical referee from computational electromagnetics and electric-machine design optimization. This is a fresh score of v13; the v12 scores (Opus 38/44, Fable 37/44) do not carry over. I read the v12 reviews and v13's `changelog.md` only to check whether each v12 finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md` (9 dimensions, /44, advance at >= 35, `anvil-pub-v2`). Voice: `.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers". `.anvil.json` declares no `venue`, so no venue overlay was scored. No litsearch sibling exists, so D4 is scored on the legacy baseline. BRIEF sets `web_search: true`, but this session has no web tool; the D4 lead below is from domain recall and is tagged for `paper-litsearch`.

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory, `papers/build.sh` and `papers/_shared/` were copied to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v13-review/fable-work/` and every build, verifier, lint and recomputation ran there. The raw-trace archive was read from its staged location and unpacked (ablation subtree only) into the same scratch directory; the refinement bundle at `/Volumes/Stripe/scratch-codex-review/exp2-fd-refine-direct-20261021/` was read, not written. Harness source was read from the main checkout. Network: read-only `gh` queries for issue #1218 and PRs #1219, #1222, #1223. No email address or identity data was sent anywhere. Per the coordinator's constraints this report is written here instead of a `.13.review/` sibling; it follows the `verdict.md` / `scoring.md` / `comments.md` / `findings.md` content of `paper-review.md`.

## Inputs read

- `main.tex` (1,201 lines) with its resolved `\input` children `optimization.tex` (371 lines) and the 12 `tables/*.tex`; `refs.bib` (40 entries); `main.pdf` (36 pages; pp. 1, 11, 12, 15, 22, 23 rendered and inspected); `build.log`; `README.md`; `changelog.md`; `citation-notes.md`; `evidence-manifest.json` (126 entries); `_progress.json`; `verify.py`, `verify_exp2.py`, `verify_exp2b.py`; `figures/src/extract_exp2b.py`, `render_exp2b_figure.py`; `figures/src/data/exp2b/{runs,fd-gradient-runtime-checks,fd-gradient-runtime-checks-L1,fd-step-sweep-L1}.json`, `provenance/setup/*.json`, `provenance/protocol-1218-export.json`; `figures/src/data/exp2/runs.json`; `figures/src/data/deposit/deposit-record.json`.
- `../BRIEF.md`, `../EXPERIMENTS.md` (status sections), `../.anvil.json`.
- `.12.review/{verdict,review-opus,review-fable}.md`.
- GitHub issue rjwalters/geode-fem#1218 (body, freeze comment, run-complete comment and its correction); PRs #1219, #1222, #1223 (bodies, file lists, merge commits); `benchmarks/motor_optimization/harness/driver.py` (`fd_gradient`, `FD_STEP`); `crates/geode-core/tests/common/icem_harness.rs` (training `warm` field).
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.
- The staged raw-trace archive `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/motor-paper-raw-traces.tar.zst` and the two refinement bundles on the same volume.

## Deterministic pre-flight (scratch copy)

| Check | Result |
|---|---|
| `python3 -I -B verify.py` (`artifact_verify` command 1, cwd `.13`) | exit 0; 14 PASS blocks (126 evidence hashes; the new step-sweep block included) |
| `python3 -I -B verify_exp2b.py` (command 2) | exit 0; 6 PASS blocks in the default package run |
| `verify_exp2b.py` with `EXP2B_SESSION` (unpacked from the archive), `EXP2B_REFINE` (the on-disk refinement bundle) and `MOTOR_RAW_ARCHIVE` | exit 0; **8 PASS blocks**, including "Exp 2b extracts re-derived from the 40 raw traces and the 90 refinement results (exact match)" and "raw-trace archive re-hashed against the deposit record" |
| Build (`papers/build.sh`) | exit 0; 3 pdflatex passes; 0 overfull boxes; 0 undefined references or citations; `pdftotext` shows no `??`; rebuilt `main.bbl` byte-identical to the committed one; 36 pages (main text pp. 1-14, Code and data p. 15, appendices A-J pp. 15-31, references from p. 32) |
| `\cite` keys vs `refs.bib` | 40 cited, 40 defined, 0 missing, 0 unused; every entry has author, title, venue and year; DOIs on all but the thesis, the tech report and the software manual |
| `anvil.lib.numeric_consistency` | pass; 1 info finding at main.tex:870, suppressed by the correctly placed directive (the known 0.08036/0.08014 false positive) |
| `anvil.lib.pending_marker` | exactly one marker, `[PENDING zenodo-doi]` at main.tex:471; `outstanding_sources = ["zenodo-doi"]`; no suppressed markers |
| `anvil.lib.audience_check` | 0 governance, 0 private-locator, 0 unlinked-artifact hits across 14 files |
| `anvil.lib.evidence_drift` | CLEAN (BRIEF and refs mtimes equal the snapshot) |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `emphatic`, `ruling`, `Judge`, `Doctor`, `tracker`, `earlier version`, `this version`, `withdrawn`, `gradient access`, `dilutes`, `logarithm of`, `noise floor`, `times slower`, `essentially the same path`, `first` as novelty, em-dashes) | 0 hits in the body; the only `---` strings are table dashes |
| Abstract length | 238 words |
| Commit pins | `fd-step-sweep-L1.json` SHA-256 `b04b2e7d...` equals `git show f94acad3:benchmarks/motor_optimization/harness/fd-step-sweep-L1.json`; `fd-gradient-runtime-checks-L1.json` `5ecb87d3...` equals `git show 74843242:...`; `f94acad3`, `74843242` and `5cc86ad8` are ancestors of `origin/main` (currently at `f94acad3`) |
| Archive | 2,179,756,244 bytes; SHA-256 `2de2132c...40a2a2` recomputed here and equal to the text, the README and `deposit-record.json`; listing 7,582 members (5,453 + 2,118 + 10 + README); unpacked ablation subtree verified 2118/2118 against its inventory, whose own digest is `c2bc65ff...` as the text states |
| Protocol timing | export shows the issue body created 2026-10-20T22:34:54Z with `graphql_lastEditedAt = None`, freeze comment 23:00:32Z; the earliest `started_utc` in the 40 raw traces is 2026-10-21T02:07:07Z (the five space-B repeat-1 A' runs), the latest 04:05:20Z; last run ended 06:18:39Z. The paper's "posted at 22:34 UTC ... frozen by a comment at 23:00 UTC, before the first ablation run began at 02:07 UTC" is correct |

## Independent cross-checks of the ablation claims (recomputed from the hash-bound extracts and the raw records, not from the verifier)

Every number below agrees with the text, Fig. 5 and Tables 8-10.

- F/A' at T90 in space A: 2.446, 2.450, 2.503, 2.524, 2.450 (GM 2.474); at T99 in space B: 4.329, 3.840, 4.144, 4.169, 4.101 (GM 4.113); repeat 2: 2.493 and 4.045. Every cell attained for T50/T90 (space A) and all three targets (space B) in both repeats; none for T99 in space A. The validation index at every attained crossing is equal between F and A' in all 50 start-target-repeat cells.
- Per-gradient cost: median `cost_fe` over F's 372 space-A gradients 1.895 units and over its 72 space-B gradients 4.251 units, i.e. 0.237 and 0.236 units per stencil solve. Adjoint per gradient (A' adjoint seconds / gradient train events / unit): medians 0.0566 and 0.0607. Non-stencil training cost per evaluation ((forward + line search minus stencil)/train events): A' 0.2325 / 0.2448, F 0.2373 / 0.2378. The text's "1.90", "4.25", "about 0.24", "0.057 and 0.061" and "0.23--0.24 units in both arms" all follow.
- Share of F's training evaluations that request a gradient (gradients / train events): medians 0.681 and 0.769 (text: 68% and 77%). Per-gradient cost ratios are 33.5 and 70.0; per gradient-requesting evaluation including the shared training solve they are 7.4 and 14.7; with the 68%/77% shares 5.6 and 12.0, against the measured no-validation ratios 5.581 and 10.071. See R2 on how the text attributes this.
- Excluding validation charges up to the crossing: F/A' = 5.581 and 10.071. Validation share of time to crossing, medians over all attained targets: A' 0.653 / 0.609, F 0.271 / 0.161 (text 61-65% and 16-27%; Table 9).
- Stop reasons: space A, F budget 8 / converged 2; A' converged 4 / solver_stopped 4 / budget 2. Space B: all 20 stop at T99. Best credited validated objective (status ok, credited, admissible, mean floor met): F within -0.32% to +0.08% of A' from every space-A start in both repeats (text: within 0.4%).
- Space-B path: validation counts equal at all five starts; training evaluations equal at four (LHS 2: 9 vs 10); maximum paired validated-objective difference 0.4%, 0.3%, 28.8%, 0.8%, 17.5% (text: within 1% at three starts, up to 29% at two).
- Backward steps: 359 of 4,281 evaluated stencil components (8.4%); 0 refusals; 0 base re-solves. Two of the 446 F gradients (both space-A theta0 runs, cut by the budget) carry `cost_fe = null` and are excluded from the medians; neither is a refusal.
- A'/A at the primary targets, same start and repeat, each in its own unit: 0.960-1.043 (space A), 0.944-1.024 (space B); the text's 0.94-1.04 holds.
- B/F per start, repeat 1: 1.24, 1.45, 0.70, 0.95, 2.27 (GM 1.22; in seconds 1.30) and 3.84, 2.46, 1.26, 1.12 (GM 1.91; in seconds 2.03); main-study B/A 3.10 and 7.79; F/A' over the matched starts 2.474 and 4.100; chain 3.02 and 7.82.
- Session: unit 66.66 s in all 40 runs, 0 host-contaminated, maximum one-minute load 19.7; every training block in the raw traces records `"warm": "Cold"`.
- L1 check record: norm-wise max-norm relative errors for F 5.40e-4, 5.22e-4, 1.93e-3, 1.83e-3 and for the mean 1.03e-4, 1.09e-4, 7.17e-4, 6.30e-4; exactly one component fails the 0.1 limit (SW3 of the mean gradient, space-A interior, relative 0.243). For each of the eight scalar components the FD-minus-adjoint error of the mean gradient has one sign at all four points, at L1 and at L0; SW3's is -1.81 to -1.89e-3 at L1 and -3.04 to -3.13e-3 at L0. (The objective-gradient error of SW1 changes sign; the paper claims the pattern only for the mean gradient, so no finding.)
- Step sweep (recomputed from the raw stencil and base values): forward error divided by step is +559.9/+564.3/+564.8 (SW3, F; spread 0.86%), -1865.8/-1880.1/-1881.2 (SW3, mean; 0.82%), -36.2/-36.3/-37.3 (MH, F; 3.00%), -2780/-2783/-2784 (MH, mean; 0.12%), +6918/+6912/+6908 (SW2, F; 0.15%), -9692/-9693/-9694 (SW2, mean; 0.01%). SW3 mean: forward 1e-6 relative 0.243, central 1e-6 relative 2.21e-4; central error falls 93.9-fold from 1e-5 to 1e-6. Central errors at 1e-6 for MH and SW2: 9.1e-8, 2.9e-7, 3.1e-7, 3.4e-7 absolute. Adjoint SW3 mean slope -7.740e-3. The repeated base solve is bit-identical; the forward 1e-6 values equal the L1 check's (differences 0.0). Every sentence of Appendix D's step-sweep paragraph is reproduced.
- Fig. 5 (`render_exp2b_figure.py`): the number printed above each pair is `rf/ra` of charged time per start, and the printed values 2.45, 2.45, 2.50, 2.52, 2.45 and 4.33, 3.84, 4.14, 4.17, 4.10 match Table 8; x-axis labelled "start".

No numerical inconsistency between text, tables, figure and data was found. No critical flag of any class.

## Verdict

**Total: 40 / 44**
**Decision: advance: true** (total >= 35 and no blocking critical flag).

Critical flags: none.

Outstanding dependencies: `zenodo-doi` (1 marker, Code and data availability). This is the operator's deposit of the raw-trace archive, whose bytes and digest the text already states and which I verified. It is a disclosure, not a defect: it holds the READY transition and lowers no score.

Evidence drift: none.

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 4 |
| | **Total** | **44** | **40** |

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The ablation changes one variable with an in-session control, a frozen estimand, no censored cell and the crossing index equal in every cell; the v12 open question is closed by measurement ("A step sweep at that point shows the excess to be forward-difference truncation error: a central difference with the same step reduces it from 0.24 to $2.2\times10^{-4}$ relative" — §5 Gradient accuracy), and I reproduce every sweep number from the raw stencil values. One point off for a condition the paper does not state: every stencil solve is a cold Newton solve ("Each stencil solve is an ordinary training solve on the base point's mesh generation" — §5 Protocol; the raw traces record `warm: Cold`, and the per-solve cost 0.236 equals the training solve's 0.237), whereas the paper's own derivative timings warm-start their stencil at 0.33–0.42 forwards per sweep (App. H), so the title's ratio is for a cold-start stencil and a warm-started F would be cheaper (R1). The shortfall of the no-validation ratios below the per-gradient ratio is attributed to one cause only ("less than the per-gradient cost ratio because only a median 68\% and 77\% of F's training evaluations request a gradient" — §5 Results) when the shared training solve is the larger one (R2). Five deterministic starts of one motor and 15 unpinned concurrent processes remain disclosed, secondary limits. |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 hash-bound runs, every ablation cell attained ("Both F and A$'$ reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts, in both repeats" — §5 Results), repeats within noise, L3 refinement of the selected designs, the L1 accuracy check with its single failing component reported, a step sweep that identifies the failure's mechanism, stop reasons and the best-objective gap reported, and a figure carrying the estimand. One point off: one motor, five starts and one finite-difference configuration (cold, forward, fixed step), and the brief's strongest competitor class is untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7). |
| 3 | Clarity of contribution | 5 | 5 | Cold-reader test passes on title, abstract and introduction alone: the title carries the measured number and its per-core condition, the abstract labels the ablation post hoc at its first mention, the contribution is one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1) and the decision result is in the introduction ("in the smaller space, gradient-based search without an adjoint is not reliably faster than the derivative-free method" — §1). No overclaiming and no buried lede. |
| 4 | Related-work positioning | 5 | 4 | The derivative-method and nonlinear-magnetostatic adjoint literature is positioned on its merits ("These works establish why an adjoint gradient is cheaper than a finite-difference one" — §2), the step-choice literature now frames the SW3 result, and no method novelty is claimed. One point off, legacy baseline (no litsearch sibling): two of the five v13 additions are cited for their titles only and do little positioning work ("\citet{peter2010numerical} and \citet{kenway2019effective} survey sensitivity-analysis approaches and adjoint practice for aerodynamic optimization" — §2), and the section still engages no prior measured adjoint-versus-finite-difference optimization benchmark (lead in A1). Not a flag. |
| 5 | Reproducibility | 5 | 5 | The package pointer now names this version and the verifier binds it ("The paper package is the directory \texttt{papers/motor-gradient-optimization/motor-gradient-optimization.13}" — Code and data availability); evaluator, harness, L1-check and step-sweep commits are pinned and verified as ancestors of `origin/main`; settings table, starts, unit, host and thread settings are given; three stdlib verifiers pass, and the optional raw path ("the package's third verifier re-derives the ablation extracts from the raw traces" — Code and data availability) reproduced the extracts exactly here from the staged archive, whose size and digest match the text. The DOI is an outstanding dependency, not scored. |
| 6 | Figure & table quality | 4 | 4 | Fig. 5 now carries the title's number per start with a labelled x-axis and a self-contained caption ("The number above each pair is that start's F/A$'$ ratio of charged time, the estimand of Table~\ref{tab:exp2b-ratios}" — Fig. 5 caption), the printed values match Table 8, Table 9's caption defines its columns, and the earlier white-space and tick defects are gone. No instance of a caption that fails standalone reading found. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no virtue words or structural-importance announcements, no em-dashes, no version history, no internal team words, audience check clean, and the gradient-accuracy check now has its own paragraph. One point off: the method/space letter collision persists as a decoding cost in the paper's central sentences ("the geometric-mean F/A$'$ ratio is 2.47 in space A (range 2.45--2.52)" — §5 Results), declined twice, and the Code and data availability statement is one 60-line paragraph. |
| 8 | Citation hygiene | 5 | 5 | All 40 `\cite` keys resolve with no unused entries; every entry carries author, title, venue and year, with DOIs where they exist; the five v13 additions are verified in `citation-notes.md` with no identifier sent, the two without retrievable abstracts are cited only for what their titles state, and the one claim they support is scoped accordingly ("which \citet{more2011estimating} show how to estimate and \citet{more2012estimating} use to choose a near-optimal step" — §2). No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 4 | Main text 14 pages, abstract 238 words, the headline stated once each in title, abstract, introduction, §5 and conclusion, the introduction no longer restates the abstract's ranges, the Discussion carries no numbers, and the central mechanism is stated in one sentence ("Up to each target crossing the two take the same path, crossing after the same number of validations from every start, so the gap is the cost of the gradient" — §5 Results). The appendices are evidence a skeptical reader wants; no instance of a paragraph that restates an earlier one found. |

## Critical-flag check

- **Citation error**: none. 40 keys resolve; the five new entries are complete with DOIs and scoped in `citation-notes.md`.
- **Numerical inconsistency**: none. Every ablation, L1-check and step-sweep number recomputed above agrees with the text, Fig. 5 and Tables 8-10; the archive digest and size match.
- **Missing experiment for a claim**: not set. The v12 open question (noise floor versus truncation) is settled by the pinned step sweep.
- **Close prior work ignored**: not set (D4 lead only).
- **Build / artifact**: clean; both declared `artifact_verify` commands exit 0; the raw re-derivation path also exits 0.
- **Post-hoc result in the title**: considered, not set. The label appears at the ablation's first mention in the abstract, introduction, §5 and conclusion; the freeze precedes the first run by three hours by GitHub's and the traces' own timestamps, and the protocol text has no edits.
- **Audience fit**: no hits; no flag.
- **Pending marker**: `zenodo-doi` is an outstanding dependency, not a flag.

## Findings

### Required (address before the next review or the audit)

- **R1. State that F's stencil solves start cold, and what a warm start would change** (`optimization.tex` L278-279, §5 Protocol: "Each stencil solve is an ordinary training solve on the base point's mesh generation"; §7 limits). Every training solve in the session, including the stencil solves, starts Newton from zero at the first angle (`"warm": "Cold"` in all 40 raw traces), which is why a stencil solve costs 0.24 units, the same as a training solve. The paper's own derivative timings (App. H) warm-start the stencil from the base states and get 24 sweeps plus the base for 8.2–10.5 forwards, about 0.33–0.42 forwards per sweep. A finite-difference implementation that warm-starts its stencil is the stronger competitor, and against it F's per-gradient cost, and the title's 2.5 and 4.1, would be smaller. The measurement is correct for the F that was frozen; the condition should be stated where the stencil is defined and the consequence named once in §7 beside the serial-stencil remark. D1 (and D2 scope).
- **R2. Name the larger cause of the gap between the per-gradient and the no-validation ratios** (`optimization.tex` L308-311: "Excluding the validation charges up to each crossing, F/A$'$ becomes 5.58 and 10.07, less than the per-gradient cost ratio because only a median 68\% and 77\% of F's training evaluations request a gradient"). The per-gradient ratios are 33 and 70; the training solve both arms pay on every evaluation brings them to 7.4 and 14.7, and the gradient-free evaluations then bring them to about 5.6 and 12. The sentence names only the second cause. Add "the training solve that both arms pay on every evaluation and" before "only a median", and make the same addition in §7 ("because shared validation charges and line-search evaluations without a gradient are included"). D1/D7, small.

### Advisory

- **A1. Related-work lead (tag: related-work; verify via `paper-litsearch`, not by the reviser).** From recall, no live search this session. The closest experimental precedent for §5 remains a matched benchmark of one gradient-based optimizer with adjoint and with finite-difference gradients against gradient-free methods: Lyu, Xu and Martins, "Benchmarking optimization algorithms for wing aerodynamic design optimization", ICCFD8, Chengdu, 2014 (paper ICCFD8-2014-0203, no DOI), and its journal successor Yu, Lyu, Xu and Martins, "On the influence of optimization algorithm and initial design on wing aerodynamic shape optimization", Aerospace Science and Technology 75:183–199, 2018, DOI 10.1016/j.ast.2018.01.016. `citation-notes.md` records that neither Crossref nor OpenAlex returned the 2014 record and that the 2018 abstract could not be retrieved; a litsearch pass with the publisher page would settle whether the 2018 paper includes a finite-difference arm. If it does, one sentence in §2 would position the ablation against the only prior measurement of the same contrast. D4.
- **A2. Letter collision** (throughout §4-§5: "B/A in space A", "F/A$'$ ... in space A"). Declined in v12 and v13 as high-risk; noted only. The lightest fix is one sentence at the top of §4 ("the spaces are written S8 and S18 below") and a substitution in `optimization.tex` and the three ablation tables. D7 nit.
- **A3. Split the Code and data availability statement** (`main.tex` L413-474). One paragraph now carries the forward evidence, the main study, the ablation, the protocol timestamps and the archive. Three paragraphs (forward evidence; main study and package; ablation and archive) would let a reader find the archive line. D7 nit.
- **A4. Fig. 5 label placement** (`figures/src/render_exp2b_figure.py` L45). In panel (a) at LHS 4 the "2.45" sits between F's square and B's triangle; placing the label at a fixed offset above F's marker rather than above the pair's maximum would remove the ambiguity. D6 nit.
- **A5. Two budget-truncated gradients** (`figures/src/data/exp2b/runs.json`, `slsqp_fd_a_theta0_r1/r2`). The last gradient of each space-A theta0 F run was cut by the budget (7 and 4 of 8 components started, `cost_fe` null) and is rightly excluded from the medians; the text's "no gradient was refused" is correct. One clause in Appendix D ("two gradients cut by the budget are excluded from the cost medians") would pre-empt an auditor's question. D5 nit.
- **A6. Raw traces** (Code and data availability). The deposit is the operator's; the archive exists, its digest and member counts match, and the verifier reproduces the extracts from it. Nothing for the reviser to do until the DOI arrives; replace the marker with the DOI only.

## v12 findings: resolution check

| v12 finding (merged list and per-reviewer) | v13 status |
|---|---|
| Merged R1 (both): package pointer names `.11` | **Resolved.** Names `.13`; `verify_exp2b.py` L364-365 asserts the pointer equals its own directory and that `.11`/`.12` are absent |
| Merged R2 (Opus R2, Fable A4): SW3 "noise floor" reading | **Resolved by measurement.** The phrase is gone (verifier asserts); Appendix D states the fixed-sign per-component offsets (reproduced here) and the pinned step sweep (`f94acad3`) shows truncation; §5 carries one sentence |
| Merged R3 (Opus R3, Fable R2): undefined "step" cost; 10.9 not reproducible | **Resolved.** Costs are per gradient in a defined unit (1.90/4.25, 0.057/0.061, 0.24 per solve, 0.23-0.24 per training solve), all reproduced; the 5.3/10.9 ratios are gone. R2 above is a residue of the new wording |
| Fable A1 / Opus A10: Fig. 5 estimand and x-axis label | **Resolved** (per-start ratio printed, "start" label) |
| Fable A3: explain F's 8/10 budget stops | **Resolved** (stops reported, best objective within 0.4%, `ftol` named, effect on time to target excluded; the cause left as untested) |
| Opus A3: L1 check timing | **Resolved** (Appendix D: run after the ablation, not part of its protocol) |
| Opus A5: "times slower" | **Resolved** ("took 2.5 and 4.1 times as long"; verifier asserts) |
| Opus A4: "essentially the same path" | **Resolved** ("Up to each target crossing the two take the same path"; post-crossing divergence reported) |
| Opus A6: "n-fold compute" | **Resolved** (§7 reconciled; see R2 for the remaining attribution) |
| Fable A5: introduction restates the abstract | **Resolved** (main-study ranges removed from §1) |
| Opus A1: title per-core condition | **Resolved** (title begins "Per core") |
| Fable A8 / Opus A7: stale `_progress.json`, README 123 vs 125 | **Resolved** (`_progress.json` note names the dependency; README says 126, matching the verifier) |
| Fable A9: verifier count | **Resolved** ("three standard-library verifiers" once) |
| Opus A2 / Fable A2: related-work leads | **Partly.** Lei, Peter & Dwight, Moré & Wild (two), Kenway added and verified; Lyu/Xu/Martins not added for lack of a record (A1) |
| Opus A7 / Fable A10: raw traces private | **Partly, by design.** Deposit-ready archive built, named with size and digest, verified here; upload is the operator's (`[PENDING zenodo-doi]`) |
| Opus A9 / Fable A7: letter collision | **Declined** again (A2) |
| Fable D1 secondary: 15 unpinned concurrent processes | Unchanged, disclosed |

## Rubric version transition

The prior review sibling (`.12.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers, lints, renders and the recomputation scripts ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v13-review/fable-work/` (rendered pages in `fable-png/`, the archive listing in `archive-listing.txt`, the unpacked ablation subtree in `raw/`). The worktree, the staged archive, the refinement bundle and git state were not touched.
- `anvil.lib.evidence_check` was run against the scoring table above (extracted to `fable-stage/scoring.md` in the review scratch dir); the result is recorded at the end of this file.
- No web search tool was available in this session; the D4 lead is from recall and is tagged for `paper-litsearch`.
- No review sibling dir was written: this report is the reviewer's output for the coordinator, who owns the `.review/` sidecar.

## Evidence-check result

`uv run --project .anvil python -m anvil.lib.evidence_check <scratch .13> --scoring fable-stage/scoring.md`: `dimensions_checked: 9`, `findings: []`, `pass: true`.
