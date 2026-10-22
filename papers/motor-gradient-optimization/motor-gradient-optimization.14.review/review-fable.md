# paper-review: motor-gradient-optimization.14 (branch claude/motor-paper-v11, 41b628cf)

Reviewer: Fable 5.1, in the anvil:paper-review role (read-only). Independent, skeptical referee from computational electromagnetics and electric-machine design optimization. This is a fresh score of v14; the v13 scores (Opus 39/44, Fable 40/44) do not carry over. I read the v13 reviews and v14's `changelog.md` only to check whether each v13 finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md` (9 dimensions, /44, advance at >= 35, `anvil-pub-v2`). Voice: `.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers". `.anvil.json` declares no `venue`, so no venue overlay was scored. No litsearch sibling exists, so D4 is scored on the legacy baseline. BRIEF sets `web_search: true`, but this session has no web tool; D4 is scored from the related-work prose, `refs.bib`, `citation-notes.md` and `../refs/yu2018-fulltext-check.md`.

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory was copied with `cp -R` to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v14-review/fable-copy/` and every build, verifier, lint, render and recomputation ran there. The staged raw-trace archive was read (hashed and listed), not written. Harness source at the pinned commit was read with `git show` from the main checkout. Network: read-only `gh` queries for issue #1224, issue #1218 and PR #1225, plus a `git fetch` of `origin`. No email address or identity data was sent anywhere. Per the coordinator's constraints this report is written here instead of a `.14.review/` sibling; it follows the `verdict.md` / `scoring.md` / `comments.md` / `findings.md` content of `paper-review.md`.

## Inputs read

- `main.tex` (1,318 lines) with its resolved `\input` children `optimization.tex` (377 lines) and the 13 `tables/*.tex` (no missing `\input` targets); `refs.bib` (41 entries); `main.pdf` (38 pages; pp. 1, 10-13, 24-25 rendered and inspected); `build.log`; `README.md`; `changelog.md`; `citation-notes.md`; `evidence-manifest.json` (129 files, 10 evidence commits); `_progress.json`; `verify.py`, `verify_exp2.py`, `verify_exp2b.py`, `verify_exp2c.py`; `figures/src/render_exp2b_figure.py`; `figures/src/data/exp2c/{runs,refinement,selection-check}.json`, `exp2c/gradient-checks/*.json`, `exp2c/provenance/protocol-1224-export.json`; `figures/src/data/exp2b/{runs,fd-gradient-runtime-checks-L1}.json`; `figures/src/data/exp2/runs.json`; `figures/src/data/deposit/deposit-record.json`.
- `../BRIEF.md`, `../EXPERIMENTS.md` (status sections), `../.anvil.json`, `../refs/yu2018-fulltext-check.md`.
- `.13.review/{verdict,review-opus,review-fable}.md`.
- GitHub issue rjwalters/geode-fem#1224 (body, freeze comment, run-complete comment and its correction), issue #1218 (state), PR #1225 (body, file list, merge commit `669029c5`); `benchmarks/motor_optimization/harness/driver.py` at `669029c5` (`METHOD_SPECS["slsqp_fd_warm"]`, `fd_solve`, `fd_gradient`).
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.
- The staged archive `/Volumes/Stripe/scratch-codex-review/motor-paper-raw-deposit/motor-paper-raw-traces.tar.zst`.

## Deterministic pre-flight (scratch copy)

| Check | Result |
|---|---|
| `python3 -I verify.py` (`artifact_verify` command 0, cwd `.14`) | exit 0; 18 PASS blocks (129 evidence hashes; the four new Exp 2c blocks chained) |
| `python3 -I verify_exp2b.py` (command 1) | exit 0; 6 PASS blocks |
| `python3 -I verify_exp2c.py` (command 2) | exit 0; 4 PASS blocks (inventory binding of 20 traces, refinement and rebuilt binary, L1 warm accuracy and space-B sweep, protocol timing and Table 11) |
| `verify_exp2.py` | exit 0; 4 PASS blocks |
| `verify_exp2b.py` with `MOTOR_RAW_ARCHIVE` = the staged three-session archive | exit 0; 7 PASS blocks, including "raw-trace archive re-hashed against the deposit record" |
| Build (pdflatex, bibtex, pdflatex x3, from a clean aux state) | exit 0; 0 overfull boxes; 0 undefined references or citations; `pdftotext` shows no `??`; 38 pages (main text pp. 1-14, Code and data p. 15, appendices pp. 15-33, references from p. 34) |
| `\cite` keys vs `refs.bib` | 41 cited, 41 defined, 0 missing, 0 unused; every entry has author, title, venue and year; the new `yu2018influence` entry carries journal, volume, pages and DOI |
| `anvil.lib.numeric_consistency` | pass; 824 numbers, 6 claims; 1 info finding at main.tex:987, suppressed by the correctly placed directive (the known 0.01071/0.01220 false positive) |
| `anvil.lib.pending_marker` | exactly one marker, `[PENDING zenodo-doi]` at main.tex:484; `outstanding_sources = ["zenodo-doi"]`; no suppressed markers |
| `anvil.lib.audience_check` | 0 governance, 0 private-locator, 0 unlinked-artifact hits across 13 files |
| `anvil.lib.evidence_drift` | CLEAN (BRIEF and refs mtimes equal the snapshot) |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `emphatic`, `ruling`, `Judge`, `Doctor`, `tracker`, `earlier version`, `this version`, `withdrawn`, `candid`, `frank`, `uncomfortable`, `novel`, `first` as novelty) | 0 hits in the body (all `candidate` hits are the word "candidate"); 0 em-dashes outside table dashes; the only issue links are the three protocol/preregistration `\href`s |
| Abstract length | 241 words by a crude macro-stripped count (changelog says 244) |
| Pinned records | `fd-gradient-runtime-checks-L1-warm.json`, `fd-step-sweep-L1-warm.json`, `fd-step-sweep-L1-warm-b.json` in the package are byte-identical to `git show 669029c5:benchmarks/motor_optimization/harness/<file>`; `669029c5` is the merge commit of PR #1225 (merged 2026-10-21T20:15:57Z) and an ancestor of `origin/main` |
| Archive | 2,609,375,627 bytes; SHA-256 `a70a0699...0203f12` recomputed here with `shasum` and equal to the text and `deposit-record.json`; member listing 5,453 + 2,118 + 1,180 session files, 15 inventory/timestamp files and the README, as the text states |
| Protocol timing (`protocol-1224-export.json`, GitHub API) | issue body created 2026-10-21T15:18:46Z with `graphql_lastEditedAt = None`; freeze comment 17:54:41Z (never edited); PR #1225 merged 20:15:57Z; the paper's "posted at 15:18 UTC ... frozen by a comment at 17:54 UTC, before the first F_w run began at 20:30 UTC" is consistent (the 20:30 start is asserted by `verify_exp2c.py`'s protocol-timing block; the compact extract carries no start times, so I did not recompute it) |

## Independent cross-checks of the F_w claims (recomputed from the hash-bound extracts, not from the verifier)

Every number below agrees with the text, Fig. 5 and Table 11 unless marked.

- Session: 20 traces, methods `Ap` and `W`, unit 69.09 s in every run, every `training_warm` field `Cold`, 0 host-contaminated, maximum one-minute load 17.37 (text 17.4).
- F_w/A' per start, repeat 1: space A T90 1.68, 1.68, 1.75, 1.77, 1.69 (GM 1.713); space B T99 2.69, 2.73, 2.69, 2.79, 2.62 (GM 2.704). Secondary targets: A T50 GM 1.688; B T50 2.50, 2.41, 2.42, 6.92, 2.50 (GM 3.024); B T90 GM 2.586. Every cell attained for T50/T90 in space A and all three targets in space B; none for T99 in space A; no censoring.
- Crossing validation index: equal between F (exp2b, repeat 1), F_w, the F session's A' and the F_w session's A' in all 50 space-target-start cells except space-B LHS 3 at T50, where F_w crosses at validation 8 against 2 for the other three. Its third validated objective is 0.46 F0 and misses the allowance, and it crosses six validations later, as Appendix D says.
- Stop reasons: space A, F_w converged 1 / budget 4, A' converged 2 / solver_stopped 2 / budget 1; space B all 10 stop at T99. As stated.
- F_w/F in seconds, cross-session: GM 0.718 (space A) and 0.679 (space B); text 0.72 and 0.68.
- A' cross-session (F_w session over F session, primary targets, each in its own unit): 0.979-1.017 and 0.960-1.032, so within 4.04%; text "within 4%".
- B/F_w per start (B in main-study units, F_w in its own): 1.83, 2.07, 1.01, 1.38, 3.23 (GM 1.764) and 6.11, 3.35, 1.94, 1.82 (GM 2.917; B censored at LHS 3); in seconds GM 1.811 and 2.995; main-study B/A over the same starts 3.098 and 7.787; F_w/A' over the same starts 1.713 and 2.683; chain 3.022 and 7.825. All as printed (1.76/2.92, 1.81/"about 3.0", 3.10/7.79, 1.71/2.68, 3.02/7.82).
- Gradient cost: 227 + 36 = 263 F_w gradients; median `cost_fe` 0.9688 and 2.3649 units (text 0.97 and 2.36); A' adjoint per gradient (adjoint seconds / gradient train events / unit) medians 0.0540 and 0.0589 (text 0.054 and 0.059); per-gradient ratios 17.9 and 40.2 (text 18 and 40). Stencil-solve charge medians 0.1173 and 0.1259 units over 1,810 and 648 stencil solves, against base training-solve medians 0.2199 and 0.2328 (text 0.117/0.126 and 0.220/0.233; ratio 0.53-0.54, "about half"). Newton iterations per stencil evaluation median 60 against 170 and 167 per cold training evaluation (as stated).
- Cold fallback: exactly one gradient, space-A LHS 2, gradient 40 of 41 in a run that ran to the budget (crossings at 4.6 and 13.1 units), all eight stencils cold at 169 Newton iterations, `base_resolved: false`. The text's "at 110 units" is asserted by the verifier; the extract has no per-gradient time, so I confirm the run, the position and the cause but not the unit value.
- L1 warm accuracy (`fd-gradient-runtime-checks-L1-warm.json` against exp2b's F record, norms recomputed from the component values): max-norm relative errors F_w/F = 1.04/1.00 (A interior), 1.08/0.97 (A theta0), 0.92/0.99 (B interior), 1.01/0.98 (B theta0) for (dF, dmean), so 0.92-1.08 as stated and inside the frozen 10x gate. Component failures above 0.1: SW3 dmean at A interior (0.219; F 0.243); Sr1 dF at A theta0 (0.141) and B theta0 (0.155); SW1 (0.226), SW4 (0.532) and Sr1 (1.581, sign reversed) dF at B interior. The failing dF components are 9.0e-5 to 6.3e-4 of the largest. All as Appendix D states.
- Space-B warm step sweep: forward dF signed errors at 1e-5/1e-6/1e-7 m are Sr1 -3.08e-4/-3.11e-3/-3.01e-2, SW4 -3.16e-4/-3.11e-3/-3.03e-2, SW1 -3.83e-4/-3.12e-3/-3.11e-2 (tenfold per decade, the 1/h signature); central at 1e-6: 3.1e-9, 1.1e-7, -2.2e-8, all below 1.2e-7. The repeated base solve is bit-identical. As stated.
- Check-host cost (the record's cold reference): base 186/186/186/176 Newton iterations against 60 per warm stencil evaluation, i.e. 0.32-0.34 of the iterations at the four points (text 0.32-0.37 quotes the PR's per-point ratios, which include the 60-120 range at the space-B points); wall-time ratio 0.48-0.53 is asserted by the verifier.
- Fig. 5 (`render_exp2b_figure.py`): four series (A' from the F_w session, F_w, F, B) in minutes; the censored B cell is drawn at `budget_fe * unit_seconds / 60` (B's own 120-unit budget), as the caption now says; the two ratio rows are F_w/A' (green, upper) and F/A' (teal, lower), each within its own session; the printed values match Tables 10 and 11. Rendered page 11 is legible at print size.
- No-validation ratios: subtracting each run's validation charges up to and including the crossing, F_w/A' per start is 2.94, 2.94, 3.46, 3.48, 3.37 (GM 3.23) and 6.01, 6.42, 6.08, 6.05, 5.96 (GM 6.10); validation is a median 0.70/0.67 of A''s time to the primary target and 0.40/0.24 of F_w's. The abstract's 3.23 and 6.10 follow.
- Table 11 values match the extract to the printed precision; GM column 1.69, 1.71, 3.02, 2.59, 2.70.

No numerical inconsistency between text, tables, figure and data was found. No critical flag of any class.

## Verdict

**Total: 38 / 44**
**Decision: advance: true** (total >= 35 and no blocking critical flag).

Critical flags: none.

Outstanding dependencies: `zenodo-doi` (1 marker, Code and data availability, main.tex L484). This is the operator's deposit of the three-session raw-trace archive, whose bytes, digest and member counts the text states and which I verified. It is a disclosure, not a defect: it holds the READY transition and lowers no score.

Evidence drift: none.

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

The total is two below v13's. That is not a regression in the science: v14 answers v13's required finding 1 by measurement rather than disclosure, withdraws the attribution sentence the new data contradict, and rebases the title on the stronger baseline. The points come off the package documentation (D5) and the abstract (D9), both of which grew stale or dense in the same revision that added the F_w arm.

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The second ablation changes one thing again (the stencil's Newton start), with an in-session control, a protocol frozen before any run (issue body 15:18 UTC, freeze 17:54 UTC, PR merged 20:16 UTC, first run 20:30 UTC) and a pre-declared accuracy gate; the paper discloses both the cold-start condition ("like every training solve in the preregistered training configuration it starts Newton-cold" — §5 Protocols) and the warm arm's accuracy cost, and withdraws the v13 attribution the data no longer support. I reproduce every F_w ratio, crossing index, cost median and L1 error from the extracts. One point off for two residual gaps. (a) The cost-decomposition check is run for one arm only, with whole-run medians explaining a to-crossing ratio ("With both factors F's expected ratio is 5.3 and 11.5, against the measured 5.58 and 10.07 once validation is excluded" — §5 Gradient cost); the whole-run gradient share of F_w is 0.27 in space A against F's 0.68, so the share that explains a to-crossing ratio is the to-crossing share, which the paper never defines (A2). (b) The warm arm has one repeat ("F$_w$ with repeat~1 only (20 runs)" — §5 Protocols) and the paper gives no reason; the timing noise of that session rests on the 4% cross-session agreement of A' rather than on a repeat (A3). |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 + 20 hash-bound runs; both finite-difference configurations a referee would ask for; every ablation cell attained ("Both finite-difference arms and both controls reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts" — §5 Results); L1 accuracy checks and step sweeps for both arms with the component failures and the sign reversal reported; L3 refinement of every selected crossing; the rebuilt-binary equivalence disclosed. One point off: one motor, five deterministic starts, the brief's strongest competitor class still untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7), and the warm arm measured once. |
| 3 | Clarity of contribution | 5 | 5 | Cold-reader test passes on title, abstract and introduction alone: the title carries the number, the baseline and the per-core condition; the contribution is one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1); the changed conclusion is stated where the old one was ("so the adjoint and the gradient-based optimizer contribute comparable factors to SLSQP's advantage" — Abstract). The ablations are labelled post hoc at their first mention in every section. No overclaiming, no buried lede. |
| 4 | Related-work positioning | 5 | 4 | The adjoint, interface-order and benchmark literature is positioned on its merits, and the one new entry is cited for exactly what its full text supports and no more ("They do not compare adjoint with finite-difference gradients, which our ablation does, but state its premise" — §2), with the full-text check filed in `refs/`. One point off, legacy baseline (no litsearch sibling): the section still engages no prior measured adjoint-versus-finite-difference optimization comparison and nothing on warm-started or continuation finite differences in nonlinear FE, which is now the paper's strongest baseline (A5). Not a flag. |
| 5 | Reproducibility | 5 | 4 | Every evaluator, harness, check and sweep is pinned by commit and the three new records are byte-identical to the pinned tree; extracts are hash-bound to timestamped inventories; four stdlib verifiers pass and the archive path re-hashes; the package pointer names this version ("The paper package is the directory" — Code and data availability). One point off for the package's own documentation: `README.md` still describes the two-session archive (2,179,756,244 bytes, `2de2132c...`) and "the raw traces of both studies", says `verify.py` chains only `verify_exp2.py` and `verify_exp2b.py`, and its Interpretation paragraph still states the cold-F headline and the cold B/F factor, all contradicting the data statement and `deposit-record.json` (R1); the text also counts "three standard-library verifiers that recompute the reported numbers from those inputs" (Code and data availability) when the package ships four (A4); and the `.14` directory is still on a branch with no pull request (A6). |
| 6 | Figure & table quality | 4 | 4 | Fig. 5 carries both estimands per start with a self-contained caption ("The two rows above the line give each start's F$_w$/A$'$ (upper) and F/A$'$ (lower) ratio of charged time within its own session" — Fig. 5 caption), the censored cell is drawn at the budget and the caption says so, the four series are distinguishable in print, and Table 11 reads standalone with its GM column. No instance of a caption that fails standalone reading found. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no virtue words, no em-dashes, no version history, audience check clean. One point off: the abstract's headline sentence makes the reader infer the baseline from a relative clause ("than the stronger variant, whose stencil solves start from the base state" — Abstract) without the word the title uses, "warm-started", and the method/space letter collision persists and has grown (A, A', B, F, F$_w$ against spaces A and B) in the paper's central paragraphs (A1, A7). |
| 8 | Citation hygiene | 5 | 5 | All 41 `\cite` keys resolve with no unused entries; every entry carries author, title, venue and year, with DOIs where they exist; the one v14 addition is complete and its claim is scoped to the verified full text ("On an eight-variable wing twist problem, \citet{yu2018influence} found gradient-free optimizers 4 to over 100 times as expensive as adjoint-driven ones at the same optimum" — §2). No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 3 | Main text 14 pages, the Discussion still opens on what transfers ("Three findings should transfer beyond this motor." — §7), the F_w detail is in Appendix D, and the F path detail moved out of §5. One point off: the abstract now carries five ratio pairs plus the interface-order percentages, including a parenthetical a busy reader does not need there ("(3.23 and 6.10 times without the shared validation charges)" — Abstract); the 90-second test passes from the title, not from the abstract (A1). |

## Critical-flag check

- **Citation error**: none. 41 keys resolve; the new entry is complete and scoped.
- **Numerical inconsistency**: none. Every F_w ratio, cost, accuracy and chain number recomputed above agrees with the text, Fig. 5 and Table 11; the archive digest, size and member counts match.
- **Missing experiment for a claim**: not set. v13's open question (what a warm-started stencil changes) is now measured; the title's ratio is for the measured, frozen configuration.
- **Close prior work ignored**: not set (D4 lead only).
- **Build / artifact**: clean; all three declared `artifact_verify` commands exit 0; the archive re-hash path also exits 0.
- **Post-hoc result in the title**: considered, not set. Both ablations are labelled post hoc at first mention in the abstract, introduction, §5 and conclusion; the F_w freeze precedes the first run by GitHub's timestamps, the protocol body has no edits, and the gate was frozen before the run.
- **Audience fit**: no hits; no flag.
- **Pending marker**: `zenodo-doi` is an outstanding dependency, not a flag.

## Findings

### Required (address before the next review or the audit)

- **R1. Bring `README.md` to the v14 facts** (package file, D5). Three sections are stale and now contradict the paper and `figures/src/data/deposit/deposit-record.json`: (i) "Raw-trace archive" (L33) names the two-session archive, "the raw traces of both studies", 2,179,756,244 bytes and SHA-256 `2de2132c...`, whereas the paper and the deposit record name the three-session archive of 2,609,375,627 bytes and `a70a0699...`; (ii) "Rebuild" (L27) says "`verify.py` runs `verify_exp2.py` and `verify_exp2b.py` at its end", but it also chains `verify_exp2c.py` (18 PASS blocks); (iii) "Interpretation" (L53) still gives the cold-F headline ("2.47 ... and 4.11 times sooner than forward differences; 5.58 and 10.07 without validation charges") and the cold B/F factor ("0.70–2.27 in space A, 1.12–3.84 in space B") as the paper's reading, which v14 has rebased on F_w. A depositor or replicator who follows the README would hash the wrong archive and read a conclusion the paper has withdrawn. Regenerate these three sections from the data statement; the Interpretation paragraph should state the F_w headline, the cold-F secondary, and the "comparable factors" conclusion.

### Advisory

- **A1. Name the baseline in the abstract's headline sentence** (`main.tex` L28-32). "than the stronger variant, whose stencil solves start from the base state (3.23 and 6.10 times without the shared validation charges), and 2.47 and 4.11 times sooner than cold-started stencils" asks the reader to work out that "start from the base state" is the title's "warm-started". Suggested: "than the stronger, warm-started variant, and 2.47 and 4.11 times sooner than cold-started stencils", and drop the no-validation parenthetical from the abstract (it is in §5 and §7). The "2.45--3.25 and 3.14--7.79" ranges could likewise become one pair each. D7/D9.
- **A2. Define the gradient share the two-factor check uses, and run it for F_w too** (`optimization.tex` L329-336, §5 Gradient cost). The 68% and 77% shares are whole-run medians of gradients over training evaluations. For F_w the whole-run shares are 0.27 (space A: 0.38, 0.24, 0.29, 0.24, 0.27) and 0.71 (space B), so the same arithmetic gives about 1.8 and 6.6 against the measured 3.23 and 6.10; with F's shares, which equal F_w's up to the crossing because the iterates coincide, it gives about 3.3 and 7.0. The check therefore works when the share is the to-crossing share, and the paper should say so (or print the to-crossing shares for both arms from the ledger). One sentence; it strengthens the paragraph. D1.
- **A3. Say why F_w has one repeat** (`optimization.tex` L283-284). The protocol's reason (F's two repeats agreed to about 1%, and the estimand uses repeat 1) is a clause; without it a referee reads 20 runs against 40 as an unexplained asymmetry. The 4% cross-session agreement of A' is the only in-text bound on that session's timing noise. D1/D2.
- **A4. "three standard-library verifiers"** (`main.tex` L451). The package ships `verify.py`, `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py`; `.anvil.json` declares three commands because `verify.py` chains the rest. Either "four" or "a chained set of standard-library verifiers". D5 nit.
- **A5. Related-work lead (tag: related-work; verify via `paper-litsearch`, not by the reviser).** Now that the headline baseline is a warm-started finite-difference stencil, §2 could position it in one sentence against the usual practice of reusing the converged state for perturbed nonlinear solves (continuation or warm-started finite differences), and against the tolerance-floor argument the step sweep makes, which is the computational-noise half of the Moré and Wild framing the paper already cites. From recall, no live search this session; a litsearch pass should settle whether there is a citable precedent for warm-started FD gradients of nonlinear magnetostatic FE. D4.
- **A6. Package pointer still on a branch** (`main.tex` L446-448). `origin/main` holds versions through `.10`; `.11`-`.14` exist only on `claude/motor-paper-v11`, which has no pull request. The changelog defers the pin to the operator; until the merge or the Zenodo deposit includes the package, a reader of the default branch cannot find `motor-gradient-optimization.14`. Operator-side, not a reviser action. D5.
- **A7. Letter collision** (throughout §4-§5). Declined in v12-v14; now eight method letters (A, A', B, C, D, E, F, F$_w$) share the alphabet with spaces A and B in sentences such as "the main-study B/A geometric means are 3.10 and 7.79, and F$_w$/A$'$ over the same starts is 1.71 and 2.68". The author's call; noted only. D7 nit.
- **A8. `_progress.json`** records `revise.completed` (15:26:30Z) before `revise.started` (15:30:00Z). Metadata only. D5 nit.
- **A9. Raw traces** (Code and data availability). Nothing for the reviser until the DOI arrives; replace the marker with the DOI only. The two-session archive under `v1-two-sessions/` is superseded and need not be deposited.

## v13 findings: resolution check

| v13 finding (merged list and per-reviewer) | v14 status |
|---|---|
| Merged R1 (Opus R2, Fable R1): cold-start stencils undisclosed; measure a warm-started F | **Resolved by measurement.** §5 Protocols states the cold start with the trace evidence; a second arm F_w (20 runs, protocol #1224 frozen before any run) measures it: 1.71 and 2.70 against 2.47 and 4.11; the title and abstract are rebased on the stronger baseline; the accuracy trade-off (component failures, Sr1 sign reversal, 1/h offset) is reported, and the "adjoint, more than the optimizer" sentence is withdrawn for "comparable factors". All recomputed here |
| Merged R2 (Opus R1): "same path" wrong at space-B LHS 2/4 | **Resolved.** Replaced by the validation-count statement with the LHS 2/4 drift quantified in Appendix D ("by 29% at the crossing validation and 18% one validation before it"); the one F_w exception at a secondary target is disclosed and recomputed here |
| Merged R3 (Opus R3, Fable R2): per-gradient ratio unprinted; both factors unnamed | **Resolved** for F (33 and 70 printed; shared training solve and gradient share both named; 7.4/14.7 and 5.3/11.5 chain). Residual A2: the share is whole-run and the check is not run for F_w |
| Opus A1: "upper bound on the noise" | **Resolved** ("of that order or smaller, unless it cancels against the remaining truncation term") |
| Opus A2: units on App. D slope and curvature | **Resolved** (N m/m and N m/m^2, "for the mean gradient") |
| Opus A3 / Fable A4: Fig. 5 censored marker; label placement | **Resolved** (drawn at the budget, caption says so; ratio rows fixed at the top of each panel) |
| Fable A3: split the data statement | **Resolved** (three paragraphs) |
| Opus A5 / Fable A1: Yu et al. 2018 / Lyu 2014 | **Resolved.** Yu 2018 positioned from the full text with an explicit statement that it has no FD arm; Lyu 2014 dropped with the reason recorded |
| Fable A5: two budget-truncated gradients | **Resolved** (Appendix D) |
| Opus A8: abstract ending | **Resolved** (ends on the decision result) |
| Opus A7: Fig. 6 placement | **Resolved** (within §6's pages) |
| Opus A4: package pointer not on the default branch | **Open** (A6; operator-side) |
| Opus A6 / Fable A2: letter collision | **Declined** again (A7) |

## Rubric version transition

The prior review sibling (`.13.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers, lints, renders and recomputation scripts ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v14-review/fable-copy/` (rendered pages in `../fable-png/`, the archive log in `fable-copy/fable-archive.log`, the verifier log in `fable-copy/fable-verify.log`). The worktree, the staged archive and git state were not touched; the anvil lints ran without `--write-review`, so no sidecar was written under the worktree.
- `anvil.lib.evidence_check` was run against the scoring table above (extracted to `fable-stage/scoring.md` in the review scratch dir); the result is recorded at the end of this file.
- No web search tool was available in this session; the D4 lead is from recall and is tagged for `paper-litsearch`.
- No review sibling dir was written: this report is the reviewer's output for the coordinator, who owns the `.review/` sidecar.

## Evidence-check result

`uv run --project .anvil python -m anvil.lib.evidence_check <scratch .14> --scoring fable-stage/scoring.md`: `dimensions_checked: 9`, `findings: []`, `pass: true`.
