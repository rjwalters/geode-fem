# paper-review: motor-gradient-optimization.16 (branch claude/motor-paper-v11, 463c7319)

Reviewer: Fable 5.1, in the anvil:paper-review role (read-only). Independent, skeptical referee from computational electromagnetics and electric-machine design optimization. This is a fresh score of v16; the v15 scores (Opus 41/44, Fable 40/44) do not carry over. I read the v15 reviews and v16's `changelog.md` only to check whether each v15 finding was resolved and nothing regressed.

Rubric: `.anvil/skills/paper/rubric.md` (9 dimensions, /44, advance at >= 35, `anvil-pub-v2`). Voice: `.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers". `.anvil.json` declares no `venue`, so no venue overlay was scored. No litsearch sibling exists, so D4 is scored on the legacy baseline. BRIEF sets `web_search: true`, but this session has no web-search tool; D4 is scored from the related-work prose, `refs.bib`, `citation-notes.md` and `citation-metadata-v15.json`, with leads from recall tagged for `paper-litsearch`.

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory was copied with `cp -R` to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v16-review/fable-work/motor-gradient-optimization.16/`, with `.anvil.json` and `BRIEF.md` copied beside it because `verify_exp2c.py` reads `../.anvil.json` and asserts the directory name. Every build, verifier, lint, render and recomputation ran there. The anvil lints ran without `--write-review`, so no sidecar was written anywhere. Network: read-only `gh` queries for issues #1218 and #1224 (bodies, comment timestamps), the branch head, the branch's pull-request list and the `main` contents of `papers/motor-gradient-optimization`. No email address or identity data was sent anywhere. Per the coordinator's constraints this report is written here instead of a `.16.review/` sibling; it carries the content of `verdict.md`, `scoring.md`, `comments.md` and `findings.md` from `paper-review.md`.

## Inputs read

- `main.tex` (1,387 lines) with its resolved `\input` children `optimization.tex` (373 lines) and the 13 `tables/*.tex` (no missing `\input` targets); `refs.bib` (47 entries); the rebuilt `main.pdf` (38 pages; pp. 1, 10-15 rendered and inspected); `README.md`; `changelog.md`; `_progress.json`; `verify.py`, `verify_exp2b.py`, `verify_exp2c.py` (the v16 blocks in full); `figures/src/data/exp2{,b,c}/runs.json`, `exp2b/timeline.json`, `exp2c/timeline.json`, `exp2c/gradient-checks/fd-step-sweep-L1-warm.json`, `exp2b/fd-step-sweep-L1.json`.
- `diff` of v15 against v16 for every changed file (`main.tex`, `optimization.tex`, `verify_exp2b.py`, `verify_exp2c.py`, `README.md`; figures and tables byte-identical).
- `../BRIEF.md`, `../EXPERIMENTS.md` (current-status block), `../.anvil.json`.
- `.15.review/{verdict,review-opus,review-fable}.md`.
- GitHub issues rjwalters/geode-fem#1218 and #1224: bodies, comment `created_at`/`updated_at`.
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.

## Deterministic pre-flight (scratch copy)

| Check | Result |
|---|---|
| `python3 verify.py` (`artifact_verify` command 0, cwd `.16`) | exit 0; 20 PASS blocks |
| `python3 verify_exp2b.py` (command 1) | exit 0; 6 PASS blocks (raw traces not supplied; expected) |
| `python3 verify_exp2c.py` (command 2) | exit 0; 6 PASS blocks, including the v16 assertions (both arms' deviations, F's own 1.0%, per-space zero-update sets, the SW3 sign change, the forbidden v15 phrases) |
| `verify_exp2.py` | exit 0; 4 PASS blocks |
| Build (pdflatex, bibtex, pdflatex x2, from a clean aux state) | exit 0; 0 overfull boxes; 0 undefined references or citations; no bibtex warnings; `pdftotext` shows no `??`; 38 pages (Conclusion on p. 14; Code and data availability and App. A on p. 15; references from p. 34). The changelog's corrected count of 38 is right |
| `\cite` keys vs `refs.bib` | 47 cited, 47 defined, 0 missing, 0 unused; every entry has author, title, year and a venue field |
| `anvil.lib.numeric_consistency` | pass; 885 numbers, 6 claims; 1 info finding at main.tex:1055, suppressed by the correctly placed directive (the known 0.01071/0.01220 false positive) |
| `anvil.lib.pending_marker` | exactly one marker, `[PENDING zenodo-doi]` at main.tex:502; `outstanding_sources = ["zenodo-doi"]`; no suppressed markers |
| `anvil.lib.audience_check` | declared audience resolved from BRIEF; 0 governance, 0 private-locator, 0 unlinked-artifact hits across 13 files |
| `anvil.lib.evidence_drift` (real thread, read-only) | CLEAN (BRIEF and refs mtimes equal the v16 snapshot) |
| `anvil.lib.evidence_check` on my scoring table | 9 dimensions, 0 findings, pass |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `emphatic`, `candid`, `frank`, `uncomfortable`, `strongest`, `novel`, `uncontested`, `ruling`, `Judge`, `Doctor`, `tracker`, `earlier version`, `this version`, `withdrawn`) | 0 hits in the body; "first" occurs only as an ordinal; 0 em-dashes |
| Abstract length | 242 words (the changelog's count) |
| Protocol timing (GitHub API) | #1218: body 2026-10-20 22:34:54Z, freeze comment 23:00:32Z, never edited. #1224: body 2026-10-21 15:18:46Z, freeze 17:54:41Z, never edited. No new comments on either issue since v15. Consistent with the data statement and App. D |
| Default branch | `463c7319` is the branch head; `origin/main` holds the package only up to `.10`; no pull request exists for `claude/motor-paper-v11` |
| Figures and tables | all 12 figure PDFs and all 13 table files byte-identical to v15 |
| v15 -> v16 diff | exactly the changelog's list: §7 and §5 four-start qualifier; Conclusion antecedent; package name `.16`; App. D cost paragraph rewritten; App. D SW3 "magnitude"; App. D zero-update sentence (3.2%, per-space sets); §5 F's 1.0%; §5 Gradient cost split into two sentences and "discusses"; §5 SW3 sign-change clause; §5 offset unit. No other text changed |

## Independent recomputation of the numbers new or relabelled in v16 (from the hash-bound extracts, not via the verifier; script `fable-work/fable_recompute.py`, log `fable-work/fable-recompute.log`)

Every number agrees with the text.

- Headline ratios (repeat 1, GM over five starts): F/A$'$ 2.474 and 4.113; F$_w$/A$'$ 1.713 and 2.704 (text 2.47/4.11 and 1.71/2.70). Ranges 2.45-2.52, 3.84-4.33, 1.68-1.77, 2.62-2.79.
- Whole-run model inputs: F training solve 0.240/0.236, A$'$ 0.232/0.245, adjoint 0.0566/0.0607, F gradient 1.895/4.251; F$_w$ base solve 0.220/0.233 (pooled `base_charges_seconds`), A$'$ 0.220/0.238, adjoint 0.0540/0.0589, F$_w$ gradient 0.969/2.365. All as printed.
- To-crossing shares: F 83.3% (2/3, 2/3, 6/7, 5/6, 7/8) and 76.9%; F$_w$ 83.3% and 71.4%. Two-factor estimates 6.29, 11.48, 3.75, 6.47 (text 6.3, 11.5, 3.8, 6.5); measured 5.58, 10.07, 3.23, 6.10; excess +12.8, +14.0, +16.1, +6.1% (text "6--16%").
- **R1 (Opus v15), now symmetric:** A$'$ to-crossing cost per evaluation 0.326, 0.336, 0.321, 0.326 against the model's 0.289, 0.305, 0.274, 0.297: +12.6, +10.0, +17.1, +10.0% (text "0.32 to 0.34 ... against the model's 0.27 to 0.31, an excess of 10--17%"). The arms: F +6.7% (A), -5.0% (B); F$_w$ +6.9% (A), +1.3% (B) (text "+7% for both F and F$_w$ in space A, and by -5% (F) and +1% (F$_w$) in space B"). The smallest A$'$ deviation (10.0%) exceeds the largest arm deviation (6.9%), so "where the model errs most" holds.
- **R2a:** F/A$'$ over the four space-B starts where B attains (theta0, LHS 1, 2, 4) is 4.100, over five 4.113; B/F 1.908; chain 7.821 against B/A 7.787. Space A: 2.474, 1.221, 3.022 against 3.098. Text "2.47 and 1.2 in space A and, over the four starts where B attains the target, 4.10 and 1.9 in space B" is now correctly scoped in §5 and §7.
- **R2b:** F's repeat agreement at the ten primary crossings: median 1.02%, max 4.42%; A$'$ alone 1.22%; pooled 1.22%. Text "a median 1.0% per primary crossing time" now names F's own value.
- **Zero-update stencils (my v15 A2):** space A 58 of 1,810 (SW4 41, SW1 10, Sr1 7), space B 20 of 648 (SW4 18, Sr1 2), minimum 40 in both; 78/2,458 = 3.17% (text "3.2% in all"; "SW1, SW4 or Sr1 in space A and SW4 or Sr1 in space B").
- **SW3 sign change (my v15 A1):** F$_w$'s forward error per unit step at the space-A interior point is -1.83e3, -1.69e3, +1.30e4 N m/m^2 at h = 1e-5, 1e-6, 1e-7; F's is -1.87e3, -1.88e3, -1.88e3. Text "-1.7e3 against F's -1.9e3; unlike F's, this error changes sign at 1e-7 m" holds, and App. D's "magnitude ... 1.9e3" is consistent with §5's signed value.
- Unit of the space-B offset: the forward error of dF/dp with F in N^2 m^2 and p in m is N^2 m^2/m, as now printed.
- Tables 10 and 11 (`exp2b-ratios.tex`, `exp2c-ratios.tex`): every per-start time and ratio and every GM reproduces from `runs.json`.

No numerical inconsistency between text, tables, figure and data was found. No critical flag of any class.

## Verdict

**Total: 40 / 44**
**Decision: advance: true** (total >= 35 and no blocking critical flag).

Critical flags: none.

Outstanding dependencies: `zenodo-doi` (1 marker, Code and data availability, main.tex L502). This is the operator's deposit of the three-session raw-trace archive, whose bytes, digest and member counts the text, the README and `deposit-record.json` state consistently. It is a disclosure, not a defect: it holds the READY transition and lowers no score. BRIEF declares no `pending_sources`.

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

The same total as my v15 score, for a reason the changelog itself gives: v16 is a text-only revision that adds no claim, run or citation. It resolves the other reviewer's two required findings (the cost-check residual is now accounted for symmetrically and the two statistics carry their sets) and every v15 advisory that touched the text, and I find no regression. My four residual deductions (D1: the warm-stencil offset left unexplained and the warm arm's single repeat; D2: one motor, five starts, no reduced-physics competitor; D4: the positioning gap for the declared audience; D7: the method/space letter collision) are all outside the scope of a text-only pass by design, and I score them the same as before on their merits. The paper is above the operator's 40/44 target and has no finding that must be fixed before the audit.

## Scoring with quoted evidence (checked by `anvil.lib.evidence_check`: 9 dims, 0 findings)

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | Two one-factor ablations with in-session controls and protocols frozen before any run, a pre-declared accuracy gate, and a two-factor cost check whose residual is now accounted for symmetrically ("The model counts only training solves, adjoints and stencil solves, so it omits the per-evaluation overhead of both arms" — App. D); I recompute the A$'$ excess (12.6, 10.0, 17.1, 10.0%) and the arms' deviations (+6.7, +6.9, -5.0, +1.3%) from the timelines, and the printed "errs most for A$'$" holds in all four cells. One point off, as in v15 and untouched by this text-only revision: the warm arm's accuracy loss is characterized but its cause is left open ("whichever component is perturbed; we did not isolate its source" — §5) when one extra base Newton update at the four check points would have settled it, and the warm arm has no in-session timing repeat, its noise bound resting on F's repeats and A$'$'s 4% cross-session agreement. |
| 2 | Evidence sufficiency | 6 | 5 | 100 + 40 + 20 hash-bound runs, both finite-difference configurations a referee would ask for, no censored ablation cell ("Both finite-difference arms and both controls reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts" — §5), L1 accuracy checks and step sweeps for both arms with the component failures and sign reversal reported, L3 refinement of every selected crossing. One point off for scope that stands: one motor, five deterministic starts, the brief's reduced-physics competitor untested ("No reduced physical model, such as a nonlinear magnetic equivalent circuit" — §7), direct and semi-analytic sensitivities named but not measured, and the warm arm measured once. |
| 3 | Clarity of contribution | 5 | 5 | The cold-reader test passes from the title, abstract and introduction alone: the title carries the number, the baseline and the per-core condition; the contribution is one sentence ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1); the attribution is scoped to its baseline and the Conclusion now names the antecedent ("comparable factors to SLSQP's advantage over COBYQA; against the cold one the adjoint contributes more" — §8). Both ablations are labelled post hoc at first mention in every section. No overclaiming, no buried lede, no unsourced novelty claim. |
| 4 | Related-work positioning | 5 | 4 | The motor-adjoint, interface-order, finite-difference-accuracy and benchmark literature is engaged on its merits, with scoped claims ("They do not compare adjoint with finite-difference gradients, which our ablation does" — §2) and the untested cheaper gradients named ("lie between the two in cost; neither was tested" — §2). One point off, unchanged from v15 since v16 adds no citations: for the declared differentiable-scientific-computing audience, §2 never places GEODE's hand-written discrete adjoint against the general adjoint-FE frameworks a reader would otherwise use, and the electromagnetic-device adjoint line it reviews through Nikolova carries no measured adjoint-versus-finite-difference timing precedent. Legacy baseline (no litsearch sibling; no web tool this session); leads in the comments, not a flag. |
| 5 | Reproducibility | 5 | 5 | Every evaluator, harness, check and sweep is pinned by commit; extracts and timelines are hash-bound to timestamped inventories; all four verifiers pass in a copy, and the new v16 assertions bind every changed sentence ("four standard-library verifiers (one entry point that runs the other three) that recompute the reported numbers from those inputs" — Code and data availability); the data statement, README and `deposit-record.json` agree on the archive's bytes, digest and member counts; `_progress.json` is ordered. The one open item is operator-side: the package exists only on `claude/motor-paper-v11`, which has no pull request. The pending DOI is a declared dependency and is not scored. |
| 6 | Figure & table quality | 4 | 4 | All figure files are byte-identical to v15 and were read standalone again: Fig. 5 carries both arms and the main-study B per start, draws the censored cell at the budget and says so, and identifies its two ratio rows ("The two rows above the line give each start's F$_w$/A$'$ (upper) and F/A$'$ (lower) ratio of charged time within its own session" — Fig. 5 caption); Tables 10 and 11 read standalone with their GM columns and I reproduce every cell; Fig. 6 is legible at 0.72 width on p. 14. No instance of a caption that fails standalone reading found. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`, 38 pages), no virtue words, no em-dashes, no version history, audience check clean, and the v15 unit, sign, antecedent and ten-number-sentence nits are fixed. One point off: the method/space letter collision persists in the paper's central paragraphs ("the main-study B/A geometric means are 3.10 and 7.79, and F$_w$/A$'$ over the same starts is 1.71 and 2.68" — §5, where B is a method and A is a space in one clause), and §2's benchmark paragraph still chains "compare ..., and ... survey ..., and ... review". |
| 8 | Citation hygiene | 5 | 5 | All 47 `\cite` keys resolve to 47 `refs.bib` entries with none missing or unused; every entry carries author, title, venue and year, with DOIs where they exist; claims are scoped to what the cited abstracts support ("a comparison that also changes the parametrization and the solver" — §2). No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 4 | The main text ends on p. 14 (Conclusion) after two ablations; the abstract carries three ratio pairs and the factor pair in 242 words ("the adjoint reached the primary target 1.71 and 2.70 times sooner, from every start, than warm-started finite differences" — Abstract); the Discussion opens on what transfers ("Three findings should transfer beyond this motor." — §7); the F$_w$ detail sits in Appendix D. Hedging is proportional to the five-start, one-motor evidence. The 90-second test passes from the abstract. |

## Critical-flag check

- **Citation error**: none. 47 keys resolve; no entry lacks its standard fields.
- **Numerical inconsistency**: none. Every v16 number recomputed above agrees with the text, Tables 10-11 and Fig. 5; the two v15 scope mismatches (4.10 vs 4.11; 1.2% vs 1.0%) are now labelled or corrected, and the verifier asserts the old phrases are absent.
- **Missing experiment for a claim**: not set. The title's ratio is for the measured, frozen configuration; the untested cheaper gradients are named as untested in §2 and §7, not claimed; "where the model errs most" is supported by the recomputed deviations.
- **Close prior work ignored**: not set (D4 leads only; the paper claims no new sensitivity method).
- **Build / artifact**: clean; all three declared `artifact_verify` commands exit 0 in the copy.
- **Post-hoc result in the title**: considered, not set. Both ablations are labelled post hoc at first mention in the abstract, introduction, §5 and Conclusion; both protocols precede their first run by GitHub's timestamps and neither body or freeze comment was edited.
- **Audience fit**: no hits; no flag.
- **Pending marker**: `zenodo-doi` is an outstanding dependency, not a flag.

## Findings

### Required (address before the next review or the audit)

None.

### Advisory

- **A1. Letter collision** (`optimization.tex` §5 "The optimizer factor" and throughout §4-§5). Declined in v12-v16; eight method letters share the alphabet with spaces A and B ("B/A ... in space A"). The author's call; noted because it is the only residual cost in D7 that a reviser could still remove.
- **A2. Print F$_w$'s per-evaluation ratio beside F's** (`optimization.tex` §5 "Gradient cost", "An evaluation with a gradient therefore costs F 7.4 and 14.7 times as much as A$'$"). The "First" factor is stated for both arms but its consequence only for F; the warm values are 4.3 and 8.7 (recomputed: (0.220+0.969)/(0.220+0.054) = 4.34; (0.233+2.365)/(0.238+0.059) = 8.75). Five words keep the two-factor account symmetric, which is what v16's App. D now does for the residual. D7/D9 nit.
- **A3. Space-A to-crossing shares rest on 3-8 evaluations per start** (`optimization.tex` §5 "Gradient cost"; carried from v15 A7). The median 83.3% is 5/6 with per-start values 2/3, 2/3, 6/7, 5/6, 7/8; a parenthetical "(over three to eight evaluations per start)" tells the reader the granularity of the statistic the two-factor check uses. D1 nit.
- **A4. §2 benchmark paragraph** (`main.tex` L147-152; carried from v15 A6): "compare ..., and \citet{bramerdorfer2018modern} survey ..., and \citet{lei2017review} review" — drop the first "and" or make three sentences. The changelog declines it as restructuring; it is a one-word edit. D7 nit.
- **A5. Related-work leads (tag: related-work; verify via `paper-litsearch`, not by the reviser).** Unchanged from v15 A5, since v16 adds no citations: (i) one sentence placing GEODE's hand-written discrete adjoint against the general adjoint-FE frameworks (dolfin-adjoint/pyadjoint, Firedrake, JAX-based FE) for the declared audience; (ii) the electromagnetic-device adjoint line reviewed through Nikolova (2004) includes measured adjoint-versus-finite-difference sensitivity timings for microwave structures (Bakr, Nikolova and co-authors, early 2000s), the nearest EM precedent for the per-gradient ratio. From recall, no live search this session. D4.
- **A6. Package on a branch only** (`main.tex` L463-465; operator-side). `origin/main` holds versions through `.10`; `.11`-`.16` exist only on `claude/motor-paper-v11`, which has no pull request. Until the merge or the Zenodo deposit includes the package, the data statement's repository pointer does not resolve on the default branch. D5, not a reviser action.
- **A7. Thread hygiene, not scored** (`../BRIEF.md`). The frontmatter and the milestone table are current, but the paragraph after the table still reads "The main study is running from that source; completed cohort evidence and M4/M5 comparisons remain pending" and "[PR #1207] ... awaits independent review", and the closing paragraph says "the configured iteration cap is six" while `.anvil.json` sets 10 and `_progress.json` records an operator-directed revision beyond it. Mark those sentences historical as the v6-era sections were.
- **A8. Raw traces** (Code and data availability). Nothing for the reviser until the DOI arrives; replace the marker with the DOI only.

## v15 findings: resolution check

| v15 finding | v16 status |
|---|---|
| Opus R1: cost-check residual attributed to A$'$ alone; "accounts for" | **Resolved.** App. D names the omitted overhead for both arms, gives A$'$'s 10-17% excess and both arms' deviations (+7/+7/-5/+1), drops the one-sided "within 4%"; §5 says "discusses". All five numbers recomputed above; the verifier asserts the old phrases are absent and that A$'$'s smallest deviation exceeds the arms' largest |
| Opus R2a: 4.10 over four starts unlabelled | **Resolved** in §5 and §7 ("over the four starts where B attains the target"); recomputed 4.100 vs 4.113 |
| Opus R2b: "1.2%" pools F and A$'$ | **Resolved.** §5 prints F's own 1.0% (recomputed 1.02%); the verifier computes F alone and keeps the pooled 1.2% as an internal check |
| Opus A1: offset unit; SW3 sign in App. D | **Resolved** ("N^2 m^2/m"; "the magnitude of SW3's forward error per unit step") |
| Opus A4: "that advantage" antecedent | **Resolved** ("SLSQP's advantage over COBYQA") |
| Fable A1: "as for F" read across the sweep | **Resolved** ("unlike F's, this error changes sign at 1e-7 m"; recomputed +1.30e4 at 1e-7) |
| Fable A2: per-space zero-update split and fraction | **Resolved** ("3.2% in all"; "SW1, SW4 or Sr1 in space A and SW4 or Sr1 in space B"; recomputed) |
| Fable A6 / Opus A5 (ten-number sentence) | **Resolved** (split into "First, ... medians of ..." and "An evaluation with a gradient therefore costs ..."). New nit A2 above |
| Fable A6 (§2 "and ..., and") | **Open** (A4) |
| Fable A7: granularity of the shares | **Open** (A3) |
| Opus A3 / Fable A8: changelog counts | **Resolved** (changelog corrected to 38 pages; abstract 242 words by my count this pass) |
| Verdict: BRIEF.md and EXPERIMENTS.md describe v6 | **Mostly resolved.** EXPERIMENTS.md has a current-status block; BRIEF.md's frontmatter, state paragraph and milestone table are current; two stale sentences remain (A7) |
| Opus A2 / Fable A4: package on a branch only | **Open** (A6; operator-side) |
| Opus A5: "in time units" for the cold optimizer factor | **Open**, declined as out of scope; minor |
| Fable A3 / Opus A8: letter collision | **Declined** again (A1) |
| Related-work leads | **Open** by design (no new citations in v16); A5 |
| Length: main text at 14 pages | **Held** (Conclusion on p. 14) |

No regression found: every v15 sentence the changelog lists as changed is changed as described, nothing else changed, every figure and table file is byte-identical, and all verifiers pass.

## Rubric version transition

The prior review sibling (`.15.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers, lints, renders and the recomputation script ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v16-review/fable-work/` (verifier logs `fable-verify*.log`, recomputation `fable_recompute.py` and `fable-recompute.log`, rendered pages `fable-png/`, staged scoring table `fable-stage/scoring.md`). The worktree and git state were not touched; the anvil lints ran without `--write-review`, so no sidecar was written under the worktree.
- `anvil.lib.evidence_check` was run against the scoring table above: `dimensions_checked: 9`, `findings: []`, `pass: true`.
- The staged archive was not re-hashed this pass (unchanged bytes and digest since v14, when both reviewers re-hashed it; the deposit record and the verifier binding were checked instead).
- No web search tool was available in this session (BRIEF `web_search: true`); the D4 leads are from recall and are tagged for `paper-litsearch`.
- No review sibling dir was written: this report is the reviewer's output for the coordinator, who owns the `.review/` sidecar.
