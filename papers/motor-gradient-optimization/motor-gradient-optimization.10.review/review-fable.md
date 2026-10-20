# Review: motor-gradient-optimization.10 (fresh score)

Reviewer: Fable (Anvil `paper-review` role, independent skeptical referee from computational electromagnetics / electric-machine design optimization). Date: 2026-10-20. Rubric: `anvil-pub-v2` (/44, advance at >= 35, critical flags short-circuit).

This is a fresh score of v10. The v7 scores (Opus 37/44, Fable 37/44) do not carry over; the v7 review, the v8/v9 audits, the v9 vision pass and the v8-v10 changelogs were read only to check whether earlier findings were actually resolved in the v10 text.

Nothing under `papers/` or in git was modified. The version directory was copied to `scratchpad/motor-v10-review/fable-copy/` and every build/verify/lint step ran there.

## Inputs read

- `motor-gradient-optimization.10/main.tex` (911 lines) with its resolved `\input` children `optimization.tex` (286 lines) and the eight `tables/*.tex`; `refs.bib` (19 entries); `main.pdf` (26 pages, rendered to PNG and inspected); `compile-log.txt`; `README.md`; `changelog.md`; `verified-numbers.json`; `evidence-manifest.json`; `figures/src/data/exp2/{runs,refinement}.json` (structure).
- `../BRIEF.md`, `../EXPERIMENTS.md` (status and scope sections), `../.anvil.json`, `../refs/citation-source-check-v9.md`.
- `.7.review/review-fable.md`, `.9.audit/flags.md`, `.9.vision/_review.json`, the v8/v9/v10 `changelog.md`.
- `.anvil/skills/paper/{SKILL.md, rubric.md, voice.md, commands/paper-review.md}`.

## Deterministic pre-flight (run in the scratch copy)

| Check | Result |
|---|---|
| `python3 -I -B verify.py` (declared `artifact_verify`) | exit 0, six PASS blocks (115 evidence hashes; 704 distinct FD cases; 592 Taylor windows; 3,240 candidate solve statuses; Exp 2 inventory binding of 100 traces; 33 designs / 213 refinement checks) |
| `python3 -I -B verify_exp2.py` | exit 0, two PASS blocks |
| Compile log | 3 pdflatex passes + bibtex; "Label(s) may have changed" only in pass 1; 0 overfull boxes; 0 undefined references/citations; `pdftotext` shows no `??` |
| `anvil.lib.audience_check` | 0 governance-vocabulary, 0 private-locator, 0 unlinked-artifact hits (note: run on the copy, so `public_repo_url` resolved null; the thread `.anvil.json` declares it) |
| `anvil.lib.pending_marker` | 0 markers; no outstanding dependencies |
| `anvil.lib.numeric_consistency` | 1 warning at main.tex:452 ("0.27% higher" vs the window pair 0.01071 -> 0.01220). False positive: the sentence compares 0.08036 with 0.08014. The `% anvil-lint-disable: numeric_consistency` directive at line 450 covers line 451 only; the sentence wraps onto 452. Nit, not a defect in the paper |
| Evidence-drift / venue overlay | No `venue` declared; no `_progress.json` evidence snapshot in the version dir, so drift is NO-SNAPSHOT (treated clean) |

## Independent numeric cross-checks (text vs tables)

All of the following were recomputed by hand from the committed `tables/*.tex`; every one agrees with the text.

- 45 paired optimizer-contrast comparisons: B/A finite pairs 5+5+5+5+4 = 24, D/E 4+4+5+4+4 = 21, total 45. Geometric-mean range 2.45 (D/E, A T90) to 7.79 (B/A, B T99). Per-start extremes 1.46 (D/E, B T90 LHS 3) and 21.54 (B/A, B T90 LHS 4).
- "Coarser mesh saved only 1.3-2.2 times": 1/0.79 = 1.27, 1/0.45 = 2.22.
- E misses exactly one start per space (A LHS 4; B LHS 1 at T90/T99); D attains all four cells E misses; A has no dash anywhere; C reaches space-A T90 from two starts and space-B T99 once.
- Three repeat-1 crossings after 60 units: B T99 at 90.4 and 62.3, D T99 at 83.3.
- D/A near tie 0.99 at space-A LHS 2, T90; D/A ranges 1.68-1.83 (A) and 2.25-4.32 (B).
- Refinement: 14 + 19 = 33 rows; 10 + 15 = 25 crossings; L3 range -4.0% (C T50, A) to +6.3% (best D, B); crossings only -4.0% to +1.1%; shifted max |-10.6| x 1e-4 = 0.106%; held-out ratios 0.646 (E T90, B) to 1.727 (best D, B); space-A best designs 1.072-1.093; space-B best A/C/D/E 1.225-1.727, best B 0.894.
- Interface effect: 0.08036/0.08829 = -8.98%, 0.03961/0.04279 = -7.4%, 0.01209/0.00760 = +59.1%; 0.01209/0.01071 = 1.129.
- Opt120: 3.86/1.07 = 3.61; 7631 x 1.07e-6 vs 2454 x 3.86e-6 gives +16%; 1.3147/1.4433 = -8.9%, 1.2550 = -13.0%, 1.4983 = +3.8%; /0.878175 gives 1.4971, 1.4291, 1.7061; x1.25 gives 1.6434; 1.0477^2 = 1.098; 3.53/1.76 = 2.0.
- Cost: 0.748/0.664 = 1.13, 14.644/12.702 = 1.15; 6.997/0.664 = 10.5, 129.39/12.702 = 10.2.
- Load threshold 1.5 x 15 = 22.5 > 21.1.

No numerical inconsistency found. No critical flag of any class.

## Verdict

**Total: 35 / 44**
**Decision: advance: true** (total >= 35 and no critical flag). This is a threshold pass, not a comfortable one; the required findings below are what separate it from a 38-40 paper.

Critical flags: none.

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

## Scoring with quoted evidence

| # | Dimension | Weight | Score | Justification |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 | The optimization study is preregistered with frozen targets, starts and budget, judged on one common validation model, with censoring handled correctly ("A ratio is formed only when both methods attain the target from that start; no finite ratio is derived from a censored cell" — §7.2) and repeat timing noise measured. The forward/gradient sections use a consistent tangent, full-order identity check, FD step sweeps and Taylor windows. The one structural gap: the paper itself concedes "The optimizer contrast includes differences in search strategy, so it does not isolate gradient access as the only causal difference" (§7.1), yet no SLSQP-with-finite-difference arm was run, so nothing in the design licenses crediting the result to *adjoint* gradients rather than to SQP-versus-DFO; the recorded 8.2-10.5x FD cost (§6.4) suggests that arm would have been decisive either way. Secondary: wall time measured with 15 concurrent processes on one host without pinning, mitigated only by order rotation. |
| 2 | Evidence sufficiency | 6 | 5 | 45/45 paired comparisons in one direction, a four-mesh interface ladder, an independent reference rerun to 0.10%, 704 FD cases and 213 refinement checks are ample for the claims as worded. Two weaknesses: the comparison rests on five starts per space (disclosed: "the comparison is a paired case study over five starts" — §7.2), and the only non-FEM competitor, "C is an empirical cubic radial-basis trust-region surrogate fitted to L1 evaluations" (§7.1), is described in one sentence with no evidence that it was configured competitively, so the paper cannot show it beat the fastest credible cheap alternative; the BRIEF's reduced-physics comparison is explicitly not run. |
| 3 | Clarity of contribution | 5 | 4 | The contribution is extractable: "The numerical methods are established; the contribution is the measured evidence linking verified gradients to time to a validated target" (§1). Deductions: the title and the closing sentence credit "verified adjoint gradients"/"verified adjoints" for an effect the body attributes to the optimizer contrast (see D1); the introduction opens with the interface-truncation result and reaches the optimization headline only at the end of its third paragraph; and the BRIEF's organizing question (whether the advantage grows with useful shape freedom) is answered only implicitly in §7.2 (space-B ratios 5.06-7.79 vs space-A 3.10-3.25) and never stated, or disclaimed, in the abstract or introduction. |
| 4 | Related-work positioning | 5 | 4 | The closest works are engaged on their merits (Wiesheu et al., Scotzniovsky et al., Merkel et al., Egger et al., Ion et al.), and the benchmarking methodology is now correctly attributed. The section "extends that comparison with a coarse finite-element competitor and a common validation target" (§2) but does not engage the nonlinear-magnetostatic motor shape-derivative line (Gangl et al. 2015) or the existing PMSM optimizer-benchmark studies (Duan and Ionel 2013; Bramerdorfer et al. 2018 review), which are the natural comparison points for an optimizer-benchmark paper on a motor. Leads listed under related-work below; not a critical flag because the paper claims no method novelty. |
| 5 | Reproducibility | 5 | 4 | Unusually strong: pinned commits, SHA-256 inventory with RFC 3161/OpenTimestamps stamps, seeds ("Each space uses $\theta_0$ and four Latin-hypercube starts (seeds 1--4), with the scalar coordinates matched across spaces" — §7.1), tolerances, budgets, host, library versions. One gap in the paper itself: method C's surrogate has no stated hyperparameters (initial design size, trust-region radii, acceptance rule, refit schedule), and the SLSQP "restores toward the last admissible point and restarts" rule is not specified; both live only in the pinned harness. |
| 6 | Figure & table quality | 4 | 3 | Captions are self-contained and the v9 vision findings were acted on (Fig. 8 log axis, budget line, larger type, dash-dot C; Fig. 9 open/filled markers). Remaining: Fig. 6(d) is a three-point panel on a 0-10 axis carrying one number already in Table 3; Fig. 2's "Residual panels have independent scales" (caption) invites misreading of the cogging vs loaded residual magnitudes; Table 7 floats to p. 22 leaving the lower third of p. 21 empty; Tables 1 and 8 still typeset negatives with hyphen-minus. |
| 7 | Prose & structural quality | 4 | 3 | LaTeX hygiene is clean (0 overfull, no `??`), no AI-tell adjectives or em-dashes (no instance of virtue-word or structural-importance announcement found). The prose is dense to the point of opacity in places: "Its ratio to Opt120 is consequently 7,631 at order 45, 2,454 at order 93 and 2,444 for conforming coupling" (§5.5) is one of a run of sentences in §5.5 that read as a lab notebook, and the abstract packs about twenty numbers into one paragraph. The capture-tool narrative in §7.2 ("the capture tool repeatedly refused on host-boot attestation details unrelated to the data") is process detail addressed to the project rather than the declared audience; the deviation itself should stay, the tooling anecdote should not. |
| 8 | Citation hygiene | 5 | 4 | Every `\cite` resolves; all 19 entries carry author, title, venue, year and (where they exist) DOI; the Dolan-Moré / Moré-Wild attribution is now correct in both places. Uncited software the results depend on: "The run uses Octave 11.3.0, GeoPDEs revision" (§5.4) and Gmsh (§7.2, "which Gmsh triangulates differently") are named without citations; NumPy/OpenBLAS likewise; Latin-hypercube sampling has no source. |
| 9 | Rhetorical economy | 4 | 3 | The headline is surfaced in the title and the second abstract sentence, and most length is evidence a skeptical reader wants. Two economy failures: the conclusion restates the abstract number for number ("On identical meshes of a published nonlinear motor, order-45 truncation lowers loaded torque standard deviation by 7--9\% and raises cogging standard deviation by 59\% relative to conforming coupling at L3" — §9, repeating the abstract and §5.2), and the §5.5 size/current normalization and matched-reference-current paragraphs (about a page) are a side study that could move to an appendix. At 26 pages against a 10-15 page brief, the same argument would land in roughly 18. |

## Findings

### Required (address before the next review)

- **R1. Title and conclusion credit adjoints for an optimizer-contrast result** (`main.tex:6-7` title; `main.tex:840-842` "verified adjoints shortened the time to a validated target"; abstract line 20 "adjoint SLSQP"). The body states the contrast "does not isolate gradient access as the only causal difference" (`optimization.tex:22-24`). The house voice rule forbids crediting a result to the derivative without an ablation. Either (a) add the SLSQP-with-central-difference-gradients arm (the recorded 8.2-10.5 forward cost per gradient makes the outcome informative in either direction), or (b) retitle to what was measured, e.g. "Gradient-based SQP with verified adjoints reaches validated torque-ripple targets sooner than derivative-free trust-region search in a nonlinear motor model", and reword the closing sentence to "adjoint SLSQP shortened ...". D1/D3.
- **R2. Method C is under-specified and its competitiveness is unexamined** (`optimization.tex:19-21`). Give the surrogate's initial-sample size, trust-region radii and update rule, refit schedule and acceptance test in the Protocol paragraph (one or two sentences, or an appendix table), and state in §8 that C's configuration was not tuned, so its poor attainment bounds this configuration, not RBF surrogates. Without this, "the fastest credible alternative" in the BRIEF's conditional headline is not tested. D2/D5.
- **R3. State the space-A vs space-B dependence in the abstract and introduction, or explicitly decline to** (`main.tex:19-22`, `78-80`). The ranges 2.45-7.79 hide that the larger ratios are all in space B (B/A 5.06-7.79 vs 3.10-3.25). The BRIEF's organizing question is whether the advantage grows with shape freedom; §7.2 has the data and the confound (different mesh-motion maps, COBYQA's 2n+1 initial set). One sentence in the abstract of the form "the ratio is larger in the 18-parameter space (5.06-7.79) than in the 8-parameter space (2.45-3.25), although the two spaces also differ in mesh-motion map" would answer the question the title of the brief asks. D3/D9.
- **R4. Conclusion duplicates the abstract** (`main.tex:815-843`). Cut the restated numbers (7-9%, 59%, 12.9->0.10%, 3.6x, 2,400-fold, -1.0/+3.7%, 45 pairs, -4.0/+1.1%) to the two or three that carry the argument, and use the space for what the reader should do differently (check interface order and excitation policy before trusting a gradient; when a gradient-based SQP pays off and when the parameterization caps it). D9.

### Advisory

- **A1. Related-work leads (tag: related-work; verify via `paper-litsearch`, not by the reviser).** From domain recall, not from a live search (no web tool in this session): Gangl, Langer, Laurain, Meftahi, Sturm, "Shape optimization of an electric motor subject to nonlinear magnetostatics", SIAM J. Sci. Comput. 37(6), 2015 (the canonical nonlinear-magnetostatic motor shape derivative; belongs beside Scotzniovsky and Merkel); Duan and Ionel, "A review of recent developments in electrical machine design optimization methods with a permanent magnet synchronous motor benchmark study", IEEE Trans. Ind. Appl. 49(3), 2013 (an optimizer benchmark on a PMSM; the closest precedent for §7's framing); Bramerdorfer, Tapia, Pyrhönen, Cavagnino, "Modern electrical machine design optimization: techniques, trends, and best practices", IEEE Trans. Ind. Electron. 65(10), 2018 (review of the method landscape the paper positions against). Identifiers to be resolved by the litsearch re-run. D4.
- **A2. Process narrative in §7.2 Final refinement** (`optimization.tex:232-238`). Keep "The refinement instead used a copy of the study outputs whose integrity is established by a SHA-256 inventory ... taken when the session finished, before the host was restarted, and independently timestamped." Drop "the capture tool repeatedly refused on host-boot attestation details unrelated to the data". D7, minor (body).
- **A3. §5.5 normalization and matched-reference-current paragraphs** (`main.tex:531-564`). Move the 1.4971/1.4291/1.7061/1.6434 normalizations and the 10,170 vs 11,982 ratios to an appendix; keep in the body the one sentence that the shorter stack and radial scale, not current policy, explain most of the saved-size deficit. D9.
- **A4. Abstract density** (`main.tex:15-44`). Twenty numbers in one paragraph; the second half (interface, Opt120, 704 FD cases, three reversed sensitivities) could be two sentences. D7/D9.
- **A5. Uncited software**: Gmsh (Geuzaine and Remacle 2009), GeoPDEs (Vázquez 2016 or de Falco, Reali, Vázquez 2011), Octave, NumPy; Latin-hypercube sampling (McKay, Beckman, Conover 1979). D8.
- **A6. Fig. 6(d)** (`main.tex:659-672`): the three-point cost panel duplicates Table 3; replace with the per-level FD-vs-gradient bar chart or drop it and widen (a)-(c). D6.
- **A7. Fig. 2 residual panels** with independent scales: label each residual panel's scale in the panel or use a common scale with an inset for cogging. D6.
- **A8. Table 7 placement** leaves the lower third of p. 21 blank; `[p]` alone, or `\footnotesize`, would let it sit after the Final-refinement text. D6.
- **A9. Hyphen-minus** in Table 1 "Std vs ref." and Table 8 phi0 (`tables/coupling.tex`, `tables/design-points.tex`): emit `$-$`. D6, nit.
- **A10. Lint directive placement** (`main.tex:450-452`): the `% anvil-lint-disable: numeric_consistency` line covers 451 only; the sentence wraps to 452 and still fires. Move the directive or join the lines. Nit.
- **A11. "safeguarded SLSQP"** (`optimization.tex:15`): say in one clause what the safeguard is (restore-and-restart on infeasible candidates, as described later), since "safeguarded" otherwise reads as an algorithm name. Nit.

## Earlier findings: resolution check (v7 Fable review, v8/v9 audits, v9 vision)

| Earlier finding | v10 status |
|---|---|
| Fable v7 M1 (protocol deviation ordering), M2 (missing disclosures), M3 (unpinned links), M4 (headline buried / "all 45 paired starts") | Resolved: opening separates preregistration from the post-run deviation; stop reasons, eligibility filter, 5 degree policy and channel wall disclosed; harness and refinement program pinned; abstract leads with the result and says "45 paired comparisons where both succeeded" |
| v8 audit C1 (space-A "near 0.043F0") | Resolved (§8, "four of the five reach the same $0.043F_0$ (C reaches $0.049F_0$)"); Table 7 confirms 0.0432/0.0432/0.0489/0.0432/0.0434 |
| v9 audit A1 (Fig. 8 "the level") | Resolved: "the A, B, D and E level appears near $0.048$ (C's near $0.054$)" |
| v9 citation check (Dolan-Moré "derivative-free") | Resolved in both §2 and §7.1 |
| v9 vision 1-5, 7, 8, 11 | Resolved in `render_exp2.py` outputs as inspected (log axis, y-range, budget line, 8.5 pt text, dash-dot C, Table 7 float, N^2 m^2, F_val label, spread crosses) |
| v9 vision 6, 9, 10 (hyphen-minus, Fig. 1b arrow, dF/dp spacing) | Deliberately not changed; A9 above repeats the hyphen-minus nit only |

## Rubric version transition

The prior review sibling (`.7.review/`) is a pair of prose reviews without `_meta.json`; both were scored on the same /44 `anvil-pub-v2` rubric, so no transition subsection applies.
