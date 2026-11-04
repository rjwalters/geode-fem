# paper-review: motor-gradient-optimization.15 (branch claude/motor-paper-v11, c3263532)

Reviewer: Fable 5.1, in the anvil:paper-review role (read-only). Independent, skeptical referee from computational electromagnetics and electric-machine design optimization. This is a fresh score of v15; the v14 scores (Opus 39/44, Fable 38/44) do not carry over. I read the v14 reviews and v15's `changelog.md` only to check whether each v14 finding was resolved.

Rubric: `.anvil/skills/paper/rubric.md` (9 dimensions, /44, advance at >= 35, `anvil-pub-v2`). Voice: `.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers". `.anvil.json` declares no `venue`, so no venue overlay was scored. No litsearch sibling exists, so D4 is scored on the legacy baseline. BRIEF sets `web_search: true`, but this session has no web-search tool; D4 is scored from the related-work prose, `refs.bib`, `citation-notes.md` and `citation-metadata-v15.json`, and the leads below are from recall, tagged for `paper-litsearch`.

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory, `.anvil.json`, `BRIEF.md` and `refs/` were copied with `cp -R` to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v15-review/fable-copy-parent/` (the version directory keeps its name, which `verify_exp2b.py` asserts), and every build, verifier, lint, render and recomputation ran there. The anvil lints ran without `--write-review`, so no sidecar was written anywhere. Network: read-only `gh` queries for issues #1224 and #1218 (bodies, comments, edit timestamps) and PR #1225, plus a `git fetch origin`. No email address or identity data was sent anywhere. Per the coordinator's constraints this report is written here instead of a `.15.review/` sibling; it carries the content of `verdict.md`, `scoring.md`, `comments.md` and `findings.md` from `paper-review.md`.

## Inputs read

- `main.tex` (1,380 lines) with its resolved `\input` children `optimization.tex` (370 lines) and the 13 `tables/*.tex` (no missing `\input` targets); `refs.bib` (47 entries); the rebuilt `main.pdf` (38 pages; pp. 1, 10-14 rendered and inspected); `build.log`; `README.md`; `changelog.md`; `citation-notes.md`; `citation-metadata-v15.json`; `evidence-manifest.json` (129 files); `_progress.json`; `verify.py`, `verify_exp2.py`, `verify_exp2b.py`, `verify_exp2c.py` (the v15 blocks in full); `figures/src/data/exp2{,b,c}/runs.json`, `exp2b/timeline.json`, `exp2c/timeline.json`, `exp2c/gradient-checks/*.json`, `exp2b/fd-step-sweep-L1.json`, `deposit/deposit-record.json`.
- `../BRIEF.md`, `../EXPERIMENTS.md` (grep for the warm arm), `../.anvil.json`.
- `.14.review/{verdict,review-opus,review-fable}.md`.
- GitHub issue rjwalters/geode-fem#1224 (body, freeze comment, run-complete comment and correction, with `lastEditedAt`), issue #1218 (same), PR #1225 (merge time and commit); `benchmarks/motor_optimization/harness/{fd-gradient-runtime-checks-L1-warm,fd-step-sweep-L1-warm,fd-step-sweep-L1-warm-b}.json` at `669029c5` via `git show`.
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.

## Deterministic pre-flight (scratch copy)

| Check | Result |
|---|---|
| `python3 -I verify.py` (`artifact_verify` command 0, cwd `.15`) | exit 0; 20 PASS blocks (129 evidence hashes; the two new v15 blocks: timelines/to-crossing shares/zero-update stencils, and package text) |
| `python3 -I verify_exp2b.py` (command 1) | exit 0; 6 PASS blocks |
| `python3 -I verify_exp2c.py` (command 2) | exit 0; 6 PASS blocks |
| `verify_exp2.py` | exit 0; 4 PASS blocks |
| Build (pdflatex, bibtex, pdflatex x3, from a clean aux state) | exit 0; 0 overfull boxes; 0 undefined references or citations; `pdftotext` shows no `??`; 38 pages (main text pp. 1-14 with the Conclusion on p. 14, Code and data p. 15, appendices pp. 15-33, references from p. 34). The changelog says 39 pages; the committed and the rebuilt PDF both have 38 |
| `\cite` keys vs `refs.bib` | 47 cited, 47 defined, 0 missing, 0 unused; every entry has author, title, year and a venue; the six v15 entries carry journal/booktitle, volume, pages (where paginated) and DOI |
| `anvil.lib.numeric_consistency` | pass; 880 numbers, 7 claims; 1 info finding at main.tex:1049, suppressed by the correctly placed directive (the known 0.01071/0.01220 false positive) |
| `anvil.lib.pending_marker` | exactly one marker, `[PENDING zenodo-doi]` at main.tex:501; `outstanding_sources = ["zenodo-doi"]`; no suppressed markers |
| `anvil.lib.audience_check` | 0 governance, 0 private-locator, 0 unlinked-artifact hits across 15 files |
| `anvil.lib.evidence_drift` | CLEAN (BRIEF and refs mtimes equal the snapshot) |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `emphatic`, `candid`, `frank`, `uncomfortable`, `strongest`, `novel`, `uncontested`, `ruling`, `Judge`, `Doctor`, `tracker`, `earlier version`, `this version`, `withdrawn`) | 0 hits in the body; "first" occurs only as an ordinal; 0 em-dashes; the only issue links are the three protocol/preregistration `\href`s |
| Abstract length | 241 words by a macro-stripped count (changelog says 242) |
| Pinned records | the three F_w gradient-check records in the package are byte-identical to `git show 669029c5:benchmarks/motor_optimization/harness/<file>`; `669029c5` is the merge commit of PR #1225 (merged 2026-10-21T20:15:57Z) |
| Protocol timing (GitHub API) | #1224 body created 15:18:46Z, `lastEditedAt` null; freeze comment 17:54:41Z, never edited, names the warm-start source and the gate result; PR #1225 merged 20:15:57Z; the paper's "posted at 15:18 UTC ... frozen by a comment at 17:54 UTC, before the first F$_w$ run began at 20:30 UTC" and the new freeze-comment clause are consistent. #1218: body 22:34:54Z, freeze 23:00:32Z, neither edited, as the data statement says |
| Package files named in the README | all present (`render_*.py`, `extract_timeline.py`, `build_deposit.py`, `deposit-record.json`, both protocol exports, `main.bbl`, `papers/build.sh`); `deposit-record.json` gives 2,609,375,627 bytes, `a70a0699...203f12`, `uploaded: false` |
| Figures | all 12 figure PDFs unchanged in content; `exp2b-per-start.pdf` is byte-identical to v14's |
| Default branch | `c3263532` is not an ancestor of `origin/main`; `origin/main` holds the package only up to `.10`; no pull request exists for `claude/motor-paper-v11` |

## Independent recomputation of the numbers that are new in v15 (from the hash-bound extracts, not via the verifier)

Every number agrees with the text unless marked.

- Timelines: 40 + 20 records whose SHA-256 and file names match `runs.json`; `train_wall_fe` and `fd_gradient_before_wall_fe` are sorted and their lengths equal `train_events` and the gradient count.
- To-crossing gradient shares (evaluations and gradients strictly before the primary crossing time, repeat 1, median over the five starts): F 0.833 (space A; per start 2/3, 6/7, 5/6, 7/8, 2/3) and 0.769 (space B; 5/6, 7/9, 10/14, 10/13, 4/6); F$_w$ 0.833 (the same counts as F in space A) and 0.714 (5/6, 7/10, 10/14, 10/13, 4/6). Text: 83.3%, 76.9%, 71.4%. Whole-run medians are 0.684/0.769 (F) and 0.271/0.714 (F$_w$), so the whole-run share that v14 used is replaced by the to-crossing share, as v14 advisory A2 asked.
- Two-factor estimates from the printed costs and shares: (0.240 + 0.833 x 1.90)/(0.232 + 0.057) = 6.31, 11.46, 3.75, 6.46 (text 6.3, 11.5, 3.8, 6.5); excess over the measured 5.58, 10.07, 3.23, 6.10: +13.1%, +13.8%, +16.2%, +5.9% (text "6--16%"). The per-evaluation-with-gradient ratios are 7.40 and 14.66 for F (printed 7.4, 14.7) and 4.34 and 8.73 for F$_w$ (not printed; the changelog explains why).
- No-validation ratios recomputed by subtracting each run's validation charges up to and including the crossing: F/A' 5.581 and 10.071; F$_w$/A' 3.229 and 6.103.
- A' evaluation cost before the crossing (non-validation time to the crossing over evaluations before it): medians 0.326/0.336 (F session) and 0.321/0.326 (F$_w$ session), ranges 0.306-0.352, against whole-run 0.283-0.336. Text "0.32 to 0.34 units each ... against 0.27 to 0.31 over whole runs" (the whole-run figure in the text is the sum of the printed training-solve and adjoint medians, 0.274-0.306; mine includes line-search and remesh charges, so the ranges differ by construction and I do not count this as a discrepancy).
- F's repeat agreement at the primary crossings: median |t2/t1 - 1| = 1.02% over F's ten cells (the verifier's 1.2% pools F and A'); "about 1%" either way.
- F$_w$: 227 + 36 = 263 gradients; 262 warm; one cold fallback at space-A LHS 2, gradient 40 of 41 in a run that ran to the budget, `base_resolved: false`, after both of that run's crossings. Median Newton iterations per warm stencil evaluation 60; median gradient cost 0.9688 and 2.3649 units. The two budget-truncated last gradients (space-A LHS 1 and LHS 4, 5 of 6 components) have no stencil below 60 iterations.
- Zero-update stencils: 58 of 1,810 warm stencil evaluations in space A and 20 of 648 in space B recorded fewer than 60 Newton iterations, minimum 40; the perturbed components are SW1, SW4 and Sr1 in space A and SW4 and Sr1 in space B. The text's union statement ("all of them perturb SW1, SW4 or Sr1") is correct.
- Cold split over the starts where B attains the primary target: F/A' 2.474 and B/F 1.221 in space A (chain 3.022 against B/A 3.098); F/A' 4.100 and B/F 1.908 in space B (chain 7.821 against 7.787). Text "2.47 and 1.2 in space A and 4.10 and 1.9 in space B". Warm chain: F$_w$/A' 1.713 x B/F$_w$ 1.764 = 3.022 and 2.683 x 2.917 = 7.825 (text 3.02 and 7.82).
- SW3 of the mean gradient at the space-A interior point, forward error per unit step: F$_w$ -1.83e3, -1.69e3, +1.30e4 N m/m^2 at h = 1e-5, 1e-6, 1e-7; F -1.87e3, -1.88e3, -1.88e3. Text "-1.7 x 10^3 ... against F's -1.9 x 10^3" at the frozen step is right. Note the sign change of F$_w$'s error at 1e-7 (advisory A1).
- Space-B interior point, forward objective-gradient errors of Sr1, SW4, SW1: -3.08e-4/-3.16e-4/-3.83e-4 at 1e-5; -3.111e-3/-3.112e-3/-3.119e-3 at 1e-6 (spread 0.24%, mean -3.11e-3, implied objective offset -3.11e-9 N^2 m^2); -3.01e-2/-3.03e-2/-3.11e-2 at 1e-7; central differences at 1e-6 are 3.1e-9, 1.1e-7 and -2.2e-8. Text: "-3.11 x 10^-3 ... to within 0.3%", "about -3 x 10^-9", "grows about tenfold per decade", "a central difference removes" all hold.
- README against the deposit record: bytes, digest and the three member counts match; the stale strings the verifier forbids are absent; the Interpretation paragraph states the warm headline, the cold secondary, the scoped factor attribution and the cold split exactly as the paper does.

No numerical inconsistency between text, tables, figure and data was found. No critical flag of any class.

## Verdict

**Total: 40 / 44**
**Decision: advance: true** (total >= 35 and no blocking critical flag).

Critical flags: none.

Outstanding dependencies: `zenodo-doi` (1 marker, Code and data availability, main.tex L501). This is the operator's deposit of the three-session raw-trace archive, whose bytes, digest and member counts the text, the README and `deposit-record.json` state consistently. It is a disclosure, not a defect: it holds the READY transition and lowers no score. BRIEF declares no `pending_sources`.

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

Two points above my v14 score. D5 recovers the point lost to the stale package text, which is now rewritten and bound by a verifier block that I mutation-tested in reading (the forbidden strings, the digest, the verifier count); D9 recovers the point lost to the abstract, which now names its baseline and carries three ratio pairs instead of five plus two ranges. D1 and D7 stay where they were for different reasons than in v14: the to-crossing share and the single-repeat reason are now in the text and recomputed here, and the 1/h referent is fixed; what remains is the unexplained warm-stencil offset and the method/space letter collision.

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | Two one-factor ablations with in-session controls and protocols frozen before any run ("Both were added after the main outcomes were known and are not part of the preregistered study; each public protocol was frozen before any of its runs" — §5), a pre-declared accuracy gate, and a two-factor cost check that now uses each arm's own to-crossing gradient share, which I recompute from the timelines (6.31/11.46/3.75/6.46 against 5.58/10.07/3.23/6.10) together with the pre-crossing correction. The factor attribution is scoped to the finite-difference variant and the cold split is printed and reproduces (2.474/1.221 and 4.100/1.908). One point off: the warm arm's accuracy loss is characterized (a common offset of about -3e-9 N^2 m^2, tenfold per decade) but its cause is left open ("whichever component is perturbed; we did not isolate its source" — §5), and the one cheap test that would settle it (one extra Newton update on the base, or a tighter base tolerance at the four check points) was not run; the warm arm also has no in-session timing repeat of its own, its noise bound resting on F's 1.2% repeats and A''s 4% cross-session agreement. |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 + 20 hash-bound runs; both finite-difference configurations a referee would ask for; every ablation cell attained ("Both finite-difference arms and both controls reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts" — §5); L1 accuracy checks and step sweeps for both arms with the component failures, the sign reversal and the zero-update stencils reported; L3 refinement of every selected crossing; the rebuilt-binary equivalence disclosed. One point off for scope that stands: one motor, five deterministic starts, the brief's reduced-physics competitor still untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7), direct and semi-analytic sensitivities named but not measured, and the warm arm measured once. |
| 3 | Clarity of contribution | 5 | 5 | The cold-reader test passes from the title, abstract and introduction alone: the title carries the number, the baseline and the per-core condition; the contribution is one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1); the attribution is stated with its dependence on the baseline ("the adjoint and the gradient-based optimizer contribute comparable factors to SLSQP's advantage; the split depends on the finite-difference variant" — Abstract). Both ablations are labelled post hoc at first mention in every section. No overclaiming, no buried lede, no unsourced novelty claim. |
| 4 | Related-work positioning | 5 | 4 | The adjoint, interface-order, FD-accuracy and benchmark literature is engaged on its merits, and the six v15 additions are placed for what their abstracts support, including the untested cheaper gradients ("lie between the two in cost; neither was tested" — §2) and the warm arm's stance toward the iterative-solve FD literature ("whereas our warm-started arm keeps the frozen tolerance and reports the resulting error" — §2); records in `citation-metadata-v15.json`. One point off, legacy baseline (no litsearch sibling; no web tool this session): for the declared differentiable-scientific-computing audience, §2 never places GEODE's discrete adjoint against the general adjoint-FE frameworks (dolfin-adjoint/pyadjoint, Firedrake, JAX-style FE) a reader would otherwise use for a motor adjoint, and it cites no measured adjoint-versus-finite-difference sensitivity timing from the electromagnetic-device line it already reviews through Nikolova. Leads in A5; not a flag. |
| 5 | Reproducibility | 5 | 5 | Every evaluator, harness, check and sweep is pinned by commit and the three F$_w$ records are byte-identical to the pinned tree; extracts and the new timelines are hash-bound to timestamped inventories; four stdlib verifiers pass in a copy ("four standard-library verifiers (one entry point that runs the other three) that recompute the reported numbers from those inputs" — Code and data availability); the README now states the three-session archive, the warm headline and the chained verifiers, and a verifier block asserts that it does; `_progress.json` is consistent; the archive digest, size and member counts agree across text, README and deposit record. The one open item is operator-side: the package exists only on `claude/motor-paper-v11` (no pull request), so the data statement's repository pointer does not yet resolve on the default branch (A4). |
| 6 | Figure & table quality | 4 | 4 | Fig. 5 carries both arms and the main-study B per start, draws the censored cell at the budget and says so, and identifies its two ratio rows in the caption ("The two rows above the line give each start's F$_w$/A$'$ (upper) and F/A$'$ (lower) ratio of charged time within its own session" — Fig. 5 caption); Table 11 reads standalone with its GM column; Fig. 6 sits with the Conclusion on p. 14 and its four panels are legible at the new 0.72 width. No instance of a caption that fails standalone reading found. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no virtue words, no em-dashes, no version history, audience check clean, and the v14 referent error is fixed with the 1/h growth attributed to the space-B point. One point off: the method/space letter collision persists in the paper's central paragraphs ("the main-study B/A geometric means are 3.10 and 7.79, and F$_w$/A$'$ over the same starts is 1.71 and 2.68" — §5, where B is a method and A is a space in the same clause), the §5 "Gradient cost" paragraph packs twelve numbers into one sentence, and §2's benchmark paragraph chains three clauses with "and ..., and ..." (A3, A6). |
| 8 | Citation hygiene | 5 | 5 | All 47 `\cite` keys resolve with no unused entries; every entry carries author, title, venue and year, with DOIs where they exist; the six v15 additions are complete and cited for scoped claims, for example Gangl et al. with its confound named ("a comparison that also changes the parametrization and the solver" — §2). No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 4 | The main text holds at 14 pages after a second arm; the abstract now names its baseline and carries three ratio pairs plus the factor pair ("the adjoint reached the primary target 1.71 and 2.70 times sooner, from every start, than warm-started finite differences" — Abstract), the no-validation parenthetical and the ranges having moved to §5; the Discussion opens on what transfers ("Three findings should transfer beyond this motor." — §7); the F$_w$ detail is in Appendix D and the §7 duplicates of §5 are gone. Hedging is proportional to the five-start, one-motor evidence. The 90-second test passes from the abstract. |

## Critical-flag check

- **Citation error**: none. 47 keys resolve; the new entries are complete and scoped.
- **Numerical inconsistency**: none. Every v15 number recomputed above agrees with the text, Table 11 and Fig. 5; the archive digest, size and member counts match in all three places.
- **Missing experiment for a claim**: not set. The title's ratio is for the measured, frozen configuration; the untested cheaper gradients are named as untested in §2 and §7, not claimed.
- **Close prior work ignored**: not set (D4 leads only; the paper claims no new sensitivity method).
- **Build / artifact**: clean; all three declared `artifact_verify` commands exit 0 in the copy; the pinned records match the harness tree.
- **Post-hoc result in the title**: considered, not set. Both ablations are labelled post hoc at first mention in the abstract, introduction, §5 and Conclusion; both protocols precede their first run by GitHub's timestamps and neither body was edited.
- **Audience fit**: no hits; no flag.
- **Pending marker**: `zenodo-doi` is an outstanding dependency, not a flag.

## Findings

### Required (address before the next review or the audit)

None.

### Advisory

- **A1. SW3 at the space-A interior point: say that the warm offset is present there too** (`optimization.tex` §5 Gradient accuracy, "where truncation dominates at the frozen step as for F"). True at h = 1e-6 (F$_w$ -1.69e3 against F -1.88e3 N m/m^2 per unit step). But F$_w$'s forward error of this component changes sign at 1e-7 (+1.30e-3 against F's -1.88e-4, which keeps its sign and size at all three steps), so the same tolerance offset that dominates the three space-B components is visible on SW3 below the frozen step. One clause ("and, unlike F's, changes sign at 10^-7 m") keeps "as for F" from being read as "across the sweep". D1/D7 nit.
- **A2. Zero-update stencils: give the per-space components** (`main.tex` App. D, "all of them perturb SW1, SW4 or Sr1"). The union is correct, but in space B the 20 low-count evaluations perturb only SW4 and Sr1, while SW1 also fails the space-B L1 check; a reader who takes the sentence as "each failing component has zero-update stencils" is misled by one component. "SW1, SW4 or Sr1 in space A and SW4 or Sr1 in space B" costs six words. Also 78 of 2,458 stencil evaluations is 3.2%; stating the fraction would show the reader how rare the event is. D2 nit.
- **A3. Letter collision** (throughout §4-§5). Declined in v12-v15; eight method letters share the alphabet with spaces A and B. The author's call; noted because it is now the main residual cost in D7.
- **A4. Package on a branch only** (`main.tex` L462-464). `origin/main` holds versions through `.10`; `.11`-`.15` exist only on `claude/motor-paper-v11`, which has no pull request. Until the merge or the Zenodo deposit includes the package, a reader of the default branch cannot find `motor-gradient-optimization.15`. Operator-side, not a reviser action. D5.
- **A5. Related-work leads (tag: related-work; verify via `paper-litsearch`, not by the reviser).** (i) For the declared audience, one sentence placing GEODE's hand-written discrete adjoint against the general adjoint-FE frameworks (dolfin-adjoint/pyadjoint, Firedrake, JAX-based FE) would pre-empt the question "why not a framework adjoint"; the contribution does not depend on it. (ii) The electromagnetic-device adjoint line the paper reviews through Nikolova (2004) includes measured adjoint-versus-finite-difference sensitivity timings for microwave structures (Bakr, Nikolova and co-authors, early 2000s); if a litsearch confirms one, it is the nearest EM precedent for the per-gradient ratio and belongs in the "These works establish why" sentence. From recall, no live search this session. D4.
- **A6. Two prose nits** (D7). §2 benchmark paragraph: "compare ..., and \citet{bramerdorfer2018modern} survey ..., and \citet{lei2017review} review" — drop the first "and" or make three sentences. §5 "Gradient cost": the sentence from "Every training evaluation costs" to "14.7 times as much as A$'$" carries ten numbers; split after the F$_w$ medians.
- **A7. Space-A to-crossing shares rest on 3-8 evaluations per start** (`optimization.tex` §5 Gradient cost). The median 83.3% is 5/6 with per-start values 2/3, 6/7, 5/6, 7/8, 2/3; a parenthetical "(over three to eight evaluations per start)" tells the reader the granularity of the statistic that the two-factor check uses. D1 nit.
- **A8. Changelog counts** (`changelog.md`, not the paper). It records 39 pages and 242 abstract words; the committed and rebuilt PDF have 38 pages and the abstract 241 words by a macro-stripped count. Metadata only.
- **A9. Raw traces** (Code and data availability). Nothing for the reviser until the DOI arrives; replace the marker with the DOI only.

## v14 findings: resolution check

| v14 finding (merged list and per-reviewer) | v15 status |
|---|---|
| Merged R1 (Opus R3, Fable R1): README stale (two-session archive, two chained verifiers, cold-F headline, "strongest"); "three" verifiers; `_progress.json` order | **Resolved.** README rewritten (verified against the deposit record and the text above); main.tex says "four standard-library verifiers (one entry point that runs the other three)"; `_progress.json` has started 00:07:18Z before completed 00:14:11Z; a verifier block asserts all of it |
| Merged R2 (Opus R1): scope "comparable factors" to the FD variant | **Resolved.** Abstract, introduction, §5 "The optimizer factor", §7 and Conclusion all carry the dependence and the cold split (2.47/1.2 and 4.10/1.9), which I recompute |
| Merged R3 (Opus R2): 1/h sentence misattributed to the space-A SW3 point | **Resolved.** SW3 is now truncation "as for F" with both slopes printed; the tenfold growth is attributed to "the space-B interior point"; the verifier asserts the old sentence is absent. Residual nuance in A1 |
| Opus A1: qualify warm accuracy; common signed error; no unmeasured mechanism | **Resolved.** "at the frozen Newton tolerance" in the abstract and §5; the common -3.11e-3 offset stated with its spread; the mechanism clause removed and its absence asserted |
| Opus A2: count the zero-update warm stencils | **Resolved** (58 of 1,810 and 20 of 648, minimum 40, components named; recomputed). Residual in A2 |
| Fable A1 / Opus A6: abstract names the baseline; fewer ratio pairs | **Resolved** ("warm-started" in the headline sentence; three pairs; the parenthetical and ranges moved to §5) |
| Fable A2 / Opus A4: to-crossing shares; run the check for F$_w$ | **Resolved** (shares from the timelines for both arms; estimates 6.3/11.5 and 3.8/6.5; the 6-16% excess accounted for in App. D; all recomputed) |
| Fable A3: why F$_w$ has one repeat | **Resolved** (protocol reason stated in §5; F's 1.2% median repeat difference printed and recomputed) |
| Opus A5: "with L2 validation as in the sessions" | **Resolved** (deleted; absence asserted) |
| Opus A10: freeze-comment nuance | **Resolved** ("The freeze comment recorded the result of the L1 accuracy gate and fixed the warm-start source"; checked against the comment) |
| Opus A11: bare AssertionError on a renamed package dir | **Resolved** (message added) |
| Opus A3 / Fable A5: related-work leads | **Resolved** (six references, each verified on Crossref/OpenAlex with no identity sent; placed with scoped claims). New leads in A5 |
| Opus A8 / Fable A6: package on a branch only | **Open** (A4; operator-side) |
| Opus A7: label Fig. 5 ratio rows in the figure | **Declined** (caption names them; figure byte-identical). Accepted |
| Opus A12 / Fable A7: letter collision | **Declined** again (A3) |
| Length: main text at 14 pages | **Held** (Conclusion on p. 14; F$_w$ detail moved to App. D; §7 duplicates removed) |

## Rubric version transition

The prior review sibling (`.14.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers, lints, renders and recomputation scripts ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v15-review/fable-copy-parent/motor-gradient-optimization.15/` (the verifier and build log in `../../fable-verify.log`, rendered pages in `../../fable-png/`). The worktree and git state were not touched; the anvil lints ran without `--write-review`, so no sidecar was written under the worktree. The staged archive was not re-hashed this pass (unchanged bytes and digest since v14, where both reviewers re-hashed it; the deposit record and the verifier binding were checked instead).
- `anvil.lib.evidence_check` was run against the scoring table above (extracted to `fable-stage/scoring.md` in the review scratch dir); the result is recorded at the end of this file.
- No web search tool was available in this session (BRIEF `web_search: true`); the D4 leads are from recall and are tagged for `paper-litsearch`.
- No review sibling dir was written: this report is the reviewer's output for the coordinator, who owns the `.review/` sidecar.

## Evidence-check result

`uv run --project .anvil python -m anvil.lib.evidence_check <scratch .15> --scoring fable-stage/scoring.md`: `dimensions_checked: 9`, `findings: []`, `pass: true`.
