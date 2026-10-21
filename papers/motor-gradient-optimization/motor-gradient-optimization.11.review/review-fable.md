# Review: motor-gradient-optimization.11 (fresh score)

Reviewer: Fable (Anvil `paper-review` role, independent skeptical referee from computational electromagnetics / electric-machine design optimization). Date: 2026-10-21. Target: `/Volumes/Stripe/geode-fem/motor-paper-v11/papers/motor-gradient-optimization/motor-gradient-optimization.11/` at commit `c7d465f6` (branch `claude/motor-paper-v11`). Rubric: `anvil-pub-v2` (/44, advance at >= 35, critical flags short-circuit). No `venue` in `../.anvil.json`, so no venue overlay. No litsearch sibling, so D4 is scored on the legacy baseline.

This is a fresh score of v11. The v10 scores (Opus 35/44, Fable 35/44) do not carry over; the v10 reviews and the v11 `changelog.md` were read only to check whether each v10 finding was resolved in the v11 text (table at the end).

Nothing under `/Volumes/Stripe/geode-fem/motor-paper-v11/` or in git was modified. The version directory was copied to `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v11-review/fable-copy/` and every build, verifier and lint step ran there. The harness source was read from the main checkout at `/Users/rwalters/GitHub/geode-fem/benchmarks/motor_optimization/harness/driver.py` (read-only). No network request other than `gh issue view 1218`; no email address or identity data sent anywhere.

Per the operator's constraints this report is written here instead of a `motor-gradient-optimization.11.review/` sibling; the content follows the `verdict.md` / `scoring.md` / `comments.md` shape of `paper-review.md`.

## Inputs read

- `main.tex` (1,100 lines) with its resolved `\input` children `optimization.tex` (370 lines) and the eleven `tables/*.tex`; `refs.bib` (29 entries); `main.pdf` (34 pages, rendered to PNG and inspected); `compile-log.txt`; `README.md`; `changelog.md`; `evidence-manifest.json`; `verify.py`, `verify_exp2.py`, `verify_exp2b.py`; `figures/src/data/exp2b/{runs,refinement,fd-gradient-runtime-checks,selection-check}.json`, `provenance/protocol-1218-export.json`, `provenance/setup/session.json`.
- `../BRIEF.md`, `../EXPERIMENTS.md` (status sections), `../.anvil.json`.
- `.10.review/{verdict,review-opus,review-fable}.md`.
- GitHub issue rjwalters/geode-fem#1218 (body, freeze comment, run-complete comment and its correction).
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.

## Deterministic pre-flight (run in the scratch copy)

| Check | Result |
|---|---|
| `python3 -I -B verify.py` (declared `artifact_verify` command 1) | exit 0 in 0.37 s; twelve PASS blocks, including the four Exp 2b blocks chained at the end (inventory binding of 40 traces, ratios/repeats/hit fractions/dilution/path/A'-vs-A/decomposition; 14 designs / 90 checks; L0 FD accuracy and protocol timing; tables and every manuscript number) |
| `python3 -I -B verify_exp2b.py` (declared command 2) | exit 0; four PASS blocks; raw-trace re-derivation not run (EXP2B_SESSION unset, as expected in the package) |
| Build (pdflatex, bibtex, pdflatex x2) | exit 0; 0 overfull boxes; 0 undefined references or citations; `pdftotext` shows no `??`; rebuilt `main.bbl` byte-identical to the committed one; 34 pages |
| `\cite` keys vs `refs.bib` | 29 cited, 29 defined, 0 missing, 0 unused |
| `anvil.lib.audience_check` | 0 governance, 0 private-locator, 0 unlinked-artifact hits |
| `anvil.lib.pending_marker` | 0 markers; no outstanding dependency |
| `anvil.lib.numeric_consistency` | 1 info finding at main.tex:354, now suppressed by the correctly placed directive (the known false positive: 0.08036/0.08014) |
| Voice scan (`honest`, `genuine`, `transparent`, `load-bearing`, `certif`, `first` as novelty, version-history phrases, em-dashes) | 0 hits in the body; 0 `---` or Unicode em-dashes in prose |

## Independent cross-checks of the ablation claims (from the hash-bound extracts, not from the verifier)

- Geometric means from `tables/exp2b-ratios.tex` and `runs.json`: F/A' at T90 in space A = 2.474 (2.45, 2.45, 2.50, 2.52, 2.45); at T99 in space B = 4.113 (4.33, 3.84, 4.14, 4.17, 4.10). Repeat 2: 2.49 and 4.04. Other targets: 2.43 (A, T50), 3.67 (B, T50), 3.86 (B, T90). Every one of the 50 per-start ratios across both repeats exceeds 2.3. Text, table and title agree.
- Hit fractions: both arms attain T50/T90 in space A and all three targets in space B from all five starts in both repeats; neither reaches T99 in space A. No censoring.
- Cross-session decomposition: main B/A = 3.10 (A, T90) and 7.79 (B, T99); B_main/F_ablation = 1.22 and 1.91 in each session's unit; x 70.94/66.66 gives 1.30 and 2.03 in seconds; the implied A/A' is 0.97 and 1.01, inside the stated 0.94-1.04. log(2.47)/log(3.10) = 0.799 and log(4.11)/log(7.79) = 0.689, so "roughly 80% and 70%" is correct; the alternative normalisation log(F/A')/(log(F/A') + log(B/F)) gives 0.82 and 0.69.
- Step costs: median (forward + line_search + adjoint)/train_events = 1.53 and 3.32 units for F, 0.29 and 0.31 for A'; ratios 5.3 and 10.9. A per-solve check (forward + line_search over all training solves including stencil solves) gives 0.23-0.25 units per solve for both arms, so a stencil solve costs about the same as any other training forward.
- Path comparison in space B (repeat 1): validation counts equal at all five starts (4, 5, 7, 10, 10); training-evaluation counts equal at four (LHS 2: 9 vs 10); max relative difference of paired validated objectives 0.4%, 0.3%, 28.8%, 0.8%, 17.5%. Matches the text.
- Space A, which the paper does not describe: F and A' cross every target at the same validation index, but afterwards diverge: validations per run 35/37/38/36/39 for F against 52/46/34/64/65 for A'; F stops at the budget in 8 of 10 runs (A': 2 of 10, with 4 converged and 4 solver-stopped); F's best credited F_val matches A''s within 0.1% at every start (Table 10: 0.0432 vs 0.0432).
- L0 FD accuracy record (`fd-gradient-runtime-checks.json`, pinned `2d4fbcfe`): max-norm relative errors 5.2e-4, 5.1e-4 (space A), 1.87e-3, 1.77e-3 (space B); worst component SW3 at the space-B interior point, 5.7e-2 on an adjoint component of -1.24e-2. Newton tolerance in that check is 1e-8, the training tolerance. Text agrees.
- Driver (`driver.py`, `fd_gradient`): the step `FD_STEP = 1e-6` is applied to the physical coordinate vector `x = P.x_of(u)`, not the unit box; stencil solves are issued one after another in the run's single process (`for k, name in enumerate(P.names): ... run.fd_solve(...)`); each stencil solve is a `train` value request charged to `forward`; a backward step is tried when the forward step is unusable; refusal path matches the text.
- Protocol timing: issue body created 2026-10-20T22:34:54Z with 0 content edits (GraphQL), freeze comment 23:00:32Z, session id `fd_ablation-20261021T020705Z`. The run-complete comment's "follows A''s iterate path exactly" and "within about 3%" were corrected in a later comment, and the paper uses the corrected statements.
- Two of 446 F gradients are incomplete (7/6 and 4/3 components); both are the last gradient of budget-stopped runs, as the text says.

No numerical inconsistency between text, tables and data was found. No critical flag of any class.

## Verdict

**Total: 36 / 44**
**Decision: advance: true** (total >= 35 and no critical flag). A one-point improvement over v10 that reflects the ablation: the title is now supported by a measured arm, and the four v10 required findings are resolved. The required findings below are scope and presentation fixes, not new experiments.

Critical flags: none.

Outstanding dependencies: none (0 pending markers).

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 5 |
| 3 | Clarity of contribution | 5 | 4 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 3 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **36** |

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The ablation is the right experiment and is run correctly: optimizer, bounds, restoration, validation policy and targets held fixed, the control rerun in-session ("A is rerun unchanged in the same session as A$'$, so the comparison shares host, time unit and load" — §4.3), estimand fixed before the runs, censoring rule retained, validation dilution decomposed. Two gaps keep it from full weight. (i) The headline is a wall-time ratio in which the n stencil solves run one after another in a single-threaded process; the paper never says so, and a design-optimization referee will immediately object that forward differences are embarrassingly parallel, so "sooner" is a per-core statement that becomes an n-fold core-occupancy statement under parallel evaluation. (ii) F's gradient accuracy is checked only on the coarse model ("On the L0 model, F's gradients agree with the adjoint within" — §4.3) while F ran on L1 at the same 1e-8 tolerance with a 1e-6 physical step, so the paper cannot separate cost from gradient quality by measurement; the path agreement in space B is the only evidence, and it is indirect. Secondary, as in v10: wall time under 15 unpinned concurrent processes. |
| 2 | Evidence sufficiency | 6 | 5 | The claim now has its ablation: 40 runs, every cell attained ("Both F and A$'$ reach $T_{50}$ and $T_{90}$ in space A and all three targets in space B from all five starts, in both repeats" — §4.3), all 50 per-start ratios above 2.3, repeat noise small, F's selected designs refined. Deductions: the title's result rests on five starts of one motor and one FD configuration (forward, fixed step, no sweep in the running model); the per-start data behind the title live only in an appendix table with no figure; and the ablation's stop reasons are not reported, although 8 of F's 10 space-A runs ran to the budget against 2 of A''s, which a reader needs to interpret "with the optimizer fixed". The reduced-physics competitor named in the brief remains untested, as the Discussion states. |
| 3 | Clarity of contribution | 5 | 4 | Cold-reader test passes: title, abstract and introduction state one extractable claim and the gradient-credit statement is now licensed by the ablation ("the contribution is the measured time-to-target evidence for adjoint gradients, separated from the choice of optimizer" — §1); the post-hoc status is stated in the abstract, introduction, §4 opener and §4.3. The brief's strongest claim (advantage grows with shape freedom) is stated with its confound. One point off: the abstract runs to about 450 words with 29 numbers and asks the reader to hold a preregistered result, a post-hoc result, a log-share decomposition, validation shares, coarse-mesh results, refinement and the interface finding in one paragraph; the organizing idea is present but not easy to extract in 90 seconds. |
| 4 | Related-work positioning | 5 | 4 | The v10 leads are in and placed on their merits (Gangl, Duan and Ionel, Bramerdorfer, Zingg: "found that a genetic algorithm needed 5 to 200 times as many function evaluations as an adjoint gradient-based method in aerodynamic shape optimization" — §2), and the paper claims no method novelty. With the title now an adjoint-versus-finite-difference result, the section does not engage the adjoint-versus-FD sensitivity literature in computational electromagnetics or machine design that makes the direction of the result expected (leads under A5); the only support for the cost argument is one motor-adjoint paper in the introduction. Not a flag: the paper's contribution is the measured instance, not the mechanism. |
| 5 | Reproducibility | 5 | 4 | Strong: harness and evaluator commits pinned, the 2,118-file inventory with RFC 3161 and OpenTimestamps records, protocol export with GitHub timestamps, settings table, three stdlib verifiers with an optional raw re-derivation path ("its protocol and a third verifier; the ablation's evaluator source is" — Code and data). One specific weakness: method F is under-specified in the paper on three replication-relevant points that only the harness pins: the step unit for the grouped offsets ("with a step of $10^{-6}$ in each component's unit" — §4.3; the driver applies it to physical coordinates), whether stencil solves are warm-started, and that they are evaluated sequentially. The package README the paper points to also contradicts the paper (A1). |
| 6 | Figure & table quality | 4 | 3 | Table 8 (per-start A', F and ratios with repeat-2 means) is clear and self-contained. Deductions: the result in the title has no figure anywhere, and Figs. 2 and 3 still show only A-E; Table 9's caption delegates its definitions ("Validation charges in the ablation, repeat~1, as in" — App. C) so it is not self-contained; Table 10 occupies the top third of p. 26 and leaves the rest blank; Table 9's medians (0.65, 0.61, over all targets) sit beside a text statistic for the primary targets only (66-69%), which reads as a mismatch until the reader notices the different denominators. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no AI-tell adjectives, no em-dashes, no version history (no instance of a virtue word or structural-importance announcement found). Deductions: the abstract and the §4.3 results paragraph are dense to the point of being hard to parse, and the decomposition sentence ("Compared approximately across the two sessions, gradient access accounts for roughly 80\% and 70\% of the logarithm of the SLSQP-versus-COBYQA ratios" — Abstract) puts an unusual statistic where a factor would do; "(GitHub timestamps; the body was never edited)" (§4.3) is provenance process language in the body that belongs in Code and data; the 2.47/4.11 result and the optimizer-factor sentence are repeated in five places. |
| 8 | Citation hygiene | 5 | 5 | All 29 `\cite` keys resolve; every entry carries author, title, venue and year, with DOIs where they exist; SLSQP, COBYQA, SciPy, NumPy, Gmsh, Octave, GeoPDEs and Latin-hypercube sampling are now cited; the benchmarking-methodology attribution is correct. No instance of an unresolved citation or an uncited non-trivial claim found. |
| 9 | Rhetorical economy | 4 | 3 | The lede is now on p. 1 and the study's results begin on p. 7, and the Conclusion is one paragraph that says what to check and what was measured ("so most of the advantage over derivative-free search came from the gradients and the rest from the optimizer" — §8). Deductions: the abstract is roughly twice a venue abstract; the decomposition is stated three times in nearly the same words ("Gradient access therefore accounts for roughly 80\% of the logarithm of the space-A ratio and 70\% of the space-B ratio, and the optimizer for the rest." — §4.3; again in the Discussion and the Conclusion); the paper is 34 pages (18 main) against the brief's 10-15. |

## Scrutiny of the new ablation claims

**Title ("2.5 to 4 times sooner than finite differences").** The two numbers are point estimates for two spaces (2.47, 4.11), not a range over conditions; "2.5 to 4" rounds 4.11 down. The comparison is SLSQP-with-adjoint against SLSQP-with-forward-differences at one fixed step, with sequential stencil solves, judged on the validated target. All of that is in §4.3 except the sequential evaluation (R1). The title is defensible as a description of what was measured; "2.5 and 4.1 times" would be more exact (A9).

**2.47 / 4.11 headline.** Recomputed from the hash-bound per-run extracts; every per-start ratio, both repeats, and the secondary targets agree with the text and Table 8. The ratio is heavily diluted by validation (A' spends 66-69% of its time to target in validation, F 16-27%), which the paper reports with the undiluted 5.58 / 10.07. The claim that the ratio grows with n is two points with a disclosed confound; the paper says so.

**"Roughly 80% and 70% of the log ratio".** Arithmetically right (0.799, 0.689) and robust to the choice of normalisation (0.82, 0.69). It is approximate in three ways the paper states (two sessions, two units, approximate separability) and one it does not: in space B the B/A geometric mean is over the four starts where B attained T99 while F/A' is over five; over the same four starts F/A' is 4.10, so the share is unchanged (A6). The residual "optimizer" factor B/F (1.22, 1.91) is COBYQA against FD-SLSQP at equal information, which is the right reading of "optimizer".

**Post-hoc labelling.** Stated in the abstract, introduction, §4 opener and §4.3, with the freeze timing and the public protocol; the Discussion repeats it. The only place that cannot carry the label is the title, and the abstract's first sentence frames the whole paper as "a preregistered, budget-matched study" before the post-hoc sentence arrives. A reader who stops at the title and first sentence could take the headline as preregistered. A short fix is to make the abstract's results sentence for the ablation open with "A post-hoc ablation, not preregistered, ..." (A10). The primary targets (T90 in A, T99 in B) were chosen knowing A's main-study attainment; this is reasonable (the highest target A attains in each space) and the other targets are reported, so there is no selection concern.

**What the paper does not say about F.** F's space-A runs mostly ran to the budget (8 of 10) while A' converged or solver-stopped; F's gradient error at the running model is unmeasured; stencil solves are sequential. None of these changes the time-to-target result, but the first two are what a referee asks when told "with the optimizer fixed" (R3), and the third governs how the headline transfers to a parallel design workflow (R1).

## Findings

### Required (address before the next review)

- **R1. State that F's stencil solves run sequentially in one single-threaded process, and what that means for the headline** (`optimization.tex` §4.3 Protocol paragraph, after "Every difference solve is charged to the run"; one clause in the abstract or introduction where "sooner" first appears). The driver evaluates the n stencil points one after another in the run's own process, so the F/A' wall-time ratio is also the per-core compute ratio. Say so, and add that parallel stencil evaluation would convert the n extra solves into n-fold core occupancy rather than remove them, while the adjoint's per-state solves admit the same parallelism. Without this, the first objection from a design-optimization referee is that finite differences are embarrassingly parallel and the wall-time claim is a scheduling artefact. D1.
- **R2. Put the title's evidence in the body** (`main.tex` App. C Table 8 `tab:exp2b-ratios`; `optimization.tex` §4.3 Results). The per-start times and ratios behind the title appear only in Appendix C, and no figure shows F against A'. Either move Table 8 into §4.3 or add A' and F to Fig. 3 (open/filled markers already distinguish mesh; a third marker shape would do), and keep the per-start table in the appendix. D6/D2.
- **R3. Report F's gradient quality at the running model, and the ablation's stop reasons** (`optimization.tex` §4.3, "On the L0 model, F's gradients agree with the adjoint within"; `main.tex` App. C "Ablation session and finite-difference accuracy"). The only accuracy check is on L0; F ran on L1 at Newton tolerance 1e-8 with a 1e-6 physical step. Either report the same max-norm comparison on L1 at theta0 and an interior point per space (two to four gradient evaluations, as §4.1 already does for the adjoint), or state in one sentence that F's L1 gradient error was not measured and that the equal validation counts at crossing are the evidence that its gradients were adequate for the path. Add the stop reasons as for the main study: in space A, F stopped at the budget in 8 of 10 runs and A' in 2 (4 converged, 4 solver-stopped), with identical best credited designs; in space B all 20 stopped at T99. D2.
- **R4. Cut the abstract** (`main.tex` L14-45). About 450 words and 29 numbers. Keep: the question, the preregistered contrast with its per-space ranges, the post-hoc ablation with 2.47/4.11 and "every primary target from every start", the factor left for the optimizer (1.2-1.3, 1.9-2.0, in place of the log shares), the validation-dilution sentence, and one sentence on the interface finding. Drop the coarse-mesh and refinement numbers (they are in §4) and the 7-9%/59% numbers (they are in §5). Target 220-250 words. D9/D7/D3.

### Advisory

- **A1. Package README contradicts the paper** (`README.md`, "Interpretation": "its results are not yet available, and the title is to be revisited once they are"; the second paragraph still says v11 "adds a skeleton for the post-hoc ablation F"). Delete the stale paragraph and the duplicated state paragraph. D5.
- **A2. Specify F's finite-difference step and stencil solves** (`optimization.tex` §4.3 Protocol). "each component's unit" should say the step is 1e-6 in the parameter's stored physical unit (metres for the eight lengths and for the grouped offsets, if that is what the harness uses), and say whether a stencil solve starts cold or from the base state. The per-solve cost in the ledger (0.23-0.25 units, the same as any training forward) suggests no warm-start discount; one clause settles it. D5.
- **A3. Text/table statistic mismatch for validation shares** (`optimization.tex` §4.3, "validation is 66--69\% of A$'$'s time to the primary target"; Table 9). The table's medians are over all targets (0.65, 0.61); the text's are the primary targets only. Add "(Table 9 gives medians over all targets)" or add a primary-target column. D7.
- **A4. Table 9 caption and Table 10 placement** (`main.tex` L902-914). Table 9's caption is "as in Table 2"; restate the three column definitions in one line. Table 10 leaves two-thirds of p. 26 blank because Appendix D opens with an `[H]` figure; `[tbp]` on Table 10 or on Fig. 7 would close the gap. D6.
- **A5. Related-work leads (tag: related-work; verify via `paper-litsearch`, not by the reviser).** From domain recall, no live search in this session. (i) Nikolova, Bandler and Bakr, "Adjoint techniques for sensitivity analysis in high-frequency structure CAD", IEEE Trans. Microwave Theory Tech. 52(1), 2004: the standard adjoint-versus-finite-difference sensitivity cost argument in computational electromagnetics. (ii) Kuci, Henrotte, Duysinx and Geuzaine, "Design sensitivity analysis for shape optimization based on the Lie derivative", Comput. Methods Appl. Mech. Engrg. 317, 2017: electric-machine shape sensitivities in a Gmsh-based pipeline, validated against finite differences. Either would anchor the introduction's "one linear solve per output" sentence and position the title's adjoint-versus-FD contrast against the sensitivity literature rather than only against optimizer benchmarks. D4.
- **A6. Decomposition start sets** (`optimization.tex` §4.3, "Dividing the main-study B times by the ablation's F times from the same starts"). In space B the B/A and B/F means are over four starts (B misses T99 from LHS 3) and F/A' over five; say so, or quote F/A' over the same four (4.10). The share is unchanged. D1 nit.
- **A7. Provenance process language in the body** (`optimization.tex` §4.3, "(GitHub timestamps; the body was never edited)"). Move to Code and data, which already describes the protocol export. D7, minor.
- **A8. Repetition of the headline and the optimizer factor** (abstract, §1 L80-83, §4.3 decomposition paragraph, §7 L578-584, §8 L628-631). Five statements of the same two numbers and the same split. The Discussion can refer to §4.3 and the Conclusion can keep the words without the numbers. D9.
- **A9. Title rounding** ("2.5 to 4 times"). These are two point estimates; "2.5 and 4.1 times" is exact and avoids reading as a range over conditions. D3 nit.
- **A10. Abstract framing of the post-hoc arm** (`main.tex` L24-27). The abstract's first results sentence belongs to "a preregistered, budget-matched study"; the post-hoc sentence follows. Open that sentence with the label ("A post-hoc ablation, not preregistered, ...") so the title's number cannot be read as preregistered by a reader who stops early. D3.
- **A11. Length** (34 pages, 18 main, against a 10-15 page brief). Already acknowledged in v11's restructuring; the appendices are evidence a skeptical reader wants, so this is noted, not pressed. D9.

## v10 findings: resolution check

| v10 finding | v11 status |
|---|---|
| Both R1: title and conclusion credit adjoints without an ablation | Resolved by the ablation; title retargeted to the measured F/A' result; the old "verified adjoints shortened" clause is gone (verifier asserts absence) |
| Opus R2: validation charges dominate; qualify space-A D/A | Resolved: Table 2 and the "Validation charges" paragraph (53% of charged time, non-validation ratios, space-A D/A 1.15-1.16 with two pairs favoring D); Discussion reworded |
| Opus R3: space-B training gradients unverified in the paper | Resolved: §4.1 "Training gradients" paragraph with the L1 FD checks in both spaces, records pinned at `9486a9da` and in the manifest |
| Opus R4: stale pre-§7 sentences | Resolved (§6.1 and App. F reworded) |
| Fable R2: method C under-specified | Resolved: §4.1 description plus Appendix B Table 4; Discussion bounds the result to this configuration |
| Fable R3: space-B dependence buried | Resolved: per-space ranges in abstract and introduction with the mesh-motion confound |
| Fable R4: conclusion restates the abstract | Resolved in form (one paragraph, what to check, scope); still carries 3.6, 96, 2.5 and 4.1 (A8) |
| Opus A1 / Fable A1: related-work leads | Resolved: Gangl 2015, Duan and Ionel 2013, Bramerdorfer 2018, Zingg 2008 added and positioned; Lyu/Martins not added (no DOI record) |
| Opus A2 / Fable A5: uncited software | Resolved: Gmsh, GeoPDEs (two entries), Octave manual, NumPy, McKay 1979 |
| Opus A3: package locator and preregistration export | Resolved: package directory named; preregistration export pinned and hash-checked; DOI left for deposit |
| Opus A4: optimizer settings table | Resolved: Appendix B Table 4 |
| Opus A5 / Fable A2: capture-tool narrative, "certify" | Resolved |
| Opus A6 / Fable A3, A4: length, lede placement, abstract density | Partly resolved: restructured, lede on p. 1, results on p. 7; abstract is still ~450 words (R4) |
| Opus A7: training/validation ratios at selected designs | Resolved (App. C paragraph) |
| Opus A8: introduction opening | Resolved |
| Opus A9, A10: wording nits | Resolved |
| Fable A6 (Fig. 6d), A7 (Fig. 2 residual scales), A8 (Table 7 placement), A9 (hyphen-minus), A10 (lint directive), A11 (safeguard defined) | Resolved or addressed by caption/move as the changelog states; hyphen-minus now `$-$`; lint directive suppresses the false positive |
| Fable D1 secondary: 15 unpinned concurrent processes | Unchanged, disclosed |

## Rubric version transition

The prior review sibling (`.10.review/`) is a pair of prose reviews scored on the same /44 `anvil-pub-v2` rubric; no transition subsection applies.

## Procedural notes

- Build, verifiers and lints ran only in `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/4368e3f5-05b1-468d-b8fa-5ae5a7c9f146/scratchpad/motor-v11-review/fable-copy/`; rendered pages are in `fable-png/` there. The worktree and git state were not touched.
- `anvil.lib.evidence_check` was run against the scoring table above (staged as `fable-stage/scoring.md` in the review scratch dir): 9 dimensions checked, 0 findings, pass.
- No web search tool was available in this session; the D4 leads are from recall and are tagged for `paper-litsearch`.
