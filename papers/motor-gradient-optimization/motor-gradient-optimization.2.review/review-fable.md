# Review: motor-gradient-optimization.2 (PR #1171, head 30664b3a)

Reviewer: Fable (Anvil `paper-review` role; independent, skeptical referee stance from computational electromagnetics / electrical-machine design optimization).
Reviewed: `/Volumes/Stripe/geode-fem/review-1171v2/papers/motor-gradient-optimization/motor-gradient-optimization.2/main.tex` (630 lines; 14 pp PDF; 4 figures, 4 tables, 11 references), with `tables/*.tex`, `changelog.md`, `verify.py`, `README.md`, `preflight.json`, `../BRIEF.md`, `../EXPERIMENTS.md`, and the v1 reviews (`review-opus.md`, `review-fable.md`).
Evidence cross-checked against `origin/main` (b487ad45) of `/Volumes/Stripe/geode-fem/review-main` via `git show`: `benchmarks/motor_reference/{forward,forward_harmonic,parametric}/` and `benchmarks/motor_shape_gradient/{,design/}` (README, tables.md, summary.json, raw `runs/*.json`). The paper pins 910593ba; the listed directories are unchanged between 910593ba and b487ad45 except for the #1179/#1180 additions the paper correctly excludes.
Rubric: installed `.anvil/skills/paper/rubric.md`, `anvil-pub-v2`, **/44, advance threshold 35**, plus `.anvil/skills/paper/voice.md`.
Rendered PDF: all 14 pages rasterized and inspected.

## Verdict

**Total: 35 / 44**
**Decision: advance: true** (total at threshold; no critical flag).
**Critical flags: none.**
**Outstanding dependencies: none** (no `[PENDING]` markers).

The paper now has a result. The v1 title question ("when do field gradients pay off?") is gone; the manuscript reports a paired, same-mesh interface experiment with an order ladder and a Nyquist-limit identity check, plus an 800-case discrete-gradient verification with FD, Taylor, tolerance and remesh controls. Every number I checked against the merged artifacts matches (table below). The deterministic pre-flights pass: `python3 -I verify.py` exits 0 (78 hashes, 1,640 solve statuses, every FD sweep recomputed from plus/minus objective values); `anvil.lib.numeric_consistency` 226 numbers, 0 findings; `anvil.lib.audience_check` 0 hits; `anvil.lib.pending_marker` 0 markers; `preflight.json` reports 0 overfull boxes, 0 placeholders. Zero em-dashes, zero virtue words, zero `certified`.

The score sits exactly at the threshold because the framing has not fully caught up with the evidence: the abstract, introduction and conclusion state the result as *reproduction agreement* (GEODE conforming is 10% high; matching the reference's orders brings it to 1.3-1.7%) rather than as the *model-dependence* result the paper actually proves on a fixed mesh (truncating at order 45 shifts the loaded ripple objective by 7-9% and cogging by 59% relative to the converged-in-modes value, while the mean moves 0.1-0.2%). That sharper statement is in Section 5.2 and nowhere else. Several smaller defects (a version-history leak, a unit mismatch between Fig. 4c and the text, a bib entry typed wrongly, an unused experiment at the optimized design that bears directly on the thesis) are listed as should-fixes.

| # | Dimension | Score | Weight | v1 (Fable) |
|---|---|---|---|---|
| 1 | Rigor of method / argument | 5 | 6 | 5 |
| 2 | Evidence sufficiency | 5 | 6 | 4 |
| 3 | Clarity of contribution | 4 | 5 | 3 |
| 4 | Related-work positioning | 4 | 5 | 2 |
| 5 | Reproducibility | 4 | 5 | 4 |
| 6 | Figure & table quality | 3 | 4 | 3 |
| 7 | Prose & structural quality | 3 | 4 | 3 |
| 8 | Citation hygiene | 4 | 5 | 3 |
| 9 | Rhetorical economy | 3 | 4 | 3 |
| | **Total** | **35** | **44** | **30** |

### Top 3 revision priorities (advisory; paper advances)

1. Put the fixed-mesh model-dependence numbers (59% cogging, 7-9% loaded, order ladder flat above 189, Nyquist-limit identity to 1e-9 N m) into the abstract, the first paragraph of the introduction and the conclusion, so the headline reads as "the published ripple objective is a property of the order-45 truncation" and not only "we reproduced it".
2. Remove the v1-facing material: "the earlier order-45 post-processing diagnostic" (line 398) is version history the reader cannot see; the whole paragraph (397-405) can go or shrink to one sentence.
3. Use the optimized-design forward runs already in the pinned evidence (`motor_reference/parametric/`: full Opt120 export under order-45 coupling at L0-L3 gives mean torque -0.10% but ripple 0.00035 vs saved 0.00018 N m at full current). One row or sentence would show that the *optimized* target, not only the initial one, is model/discretization-dependent, which is the paper's thesis.

## Resolution of v1 blockers

| v1 item | v2 status | Judgment |
|---|---|---|
| Fable B1 / Opus S1: title asks an unanswered optimization question; prospective §6 | Retitled; §6 replaced by measured M3 results; protocol compressed to two paragraphs (557-564) | **Resolved.** Title matches content. Residual: the title phrase "verified design gradients" undersells; see D3. |
| Fable B2: unresolved cause with no arbitration | The harmonic-truncated solve was run (the decisive experiment named in v1). Loaded ripple gap closed to 1.3-1.7%; cogging still 12.9% off, disclosed as unresolved (407-417, 575-576) | **Resolved for the loaded objective; cogging stays open by the paper's own account.** P2 / third-code / reference-refinement arbitration not done; EXPERIMENTS.md §3 scopes it. Acceptable as a limitation. |
| Fable B3 / Opus B3, B4: harmonic-coupling, Arkkio, Ion 2018 missing | Bontinck 2018, Egger 2022, Ion 2018, Arkkio 1987, Arkkio & Hannukainen 2012 added and engaged (80-98) | **Resolved.** Howe & Zhu-type cogging/discretization literature still absent (minor). |
| Opus B1: selective filter diagnostic (cogging only) | All three currents and all five cuts reported, labelled non-monotone and non-evidentiary (397-405) | **Resolved**, but the paragraph is now a response to reviewers rather than to readers (D9, S2). |
| Opus B2: "two explanations" not exhaustive | Torque-method (coupling vs Arkkio vs half-band) and free-end controls measured (365-370); straight-sided geometry and reference discretization named as unbounded (407-410) | **Resolved.** |
| Fable S1 / Opus: material-endpoint overbreadth | Restricted to low field; high-field segment explicitly untested, bridge fields exceed table (413-415) | **Resolved.** |
| Coupling-arc segment wording | "The 60° coupling arc has 120 2^l segments" (278-279) | **Resolved.** |
| Author placeholder / pdfsubject | `\author{}`; pdfsubject now descriptive | **Resolved** for a draft; the rendered PDF has no author line at all, which must be settled before deposit. |
| Single-circle Maxwell range | Removed; replaced by paired estimator evidence | **Resolved.** |
| 500-solve mislabel; CPU/threads | "500 baseline and sensitivity solves, and all 1,140 additional" (186-187); M3 Ultra, 28 cores, load 14-24, threads unpinned (516-518) | **Resolved** as disclosure; timing remains non-reproducible (D5). |
| verify.py coverage | Now covers cost medians, sensitivity rows, coupling/order/estimator claims, all FD sweeps | **Resolved**; it is a good artifact. |
| n/a vs dash; appendix definitions; license; Scotzniovsky year | All applied | **Resolved.** |

## Numbers vs merged artifacts

Every headline number was recomputed from `forward_harmonic/runs/*.json`, `motor_shape_gradient/design/runs/*.json`, or read from the corresponding `tables.md` / README at `origin/main`. All match.

| Claim (main.tex line) | Paper | Artifact | Status |
|---|---|---|---|
| Loaded mean error, conforming L3 (19, 308) | 0.15 / 0.16 % low | -0.1532 / -0.1563 % | ok |
| Loaded ripple, conforming L3 (20, 311) | +9.9 / +11.3 % | +9.858 / +11.266 % | ok |
| Harmonic ripple L3 (22, 312) | +1.7 / +1.3 % | +1.703 / +1.267 % (recomputed from L3_h45.json) | ok |
| Harmonic mean deviation (310) | -0.002 / +0.051 % | -0.00245 / +0.0511 % (recomputed) | ok |
| Max full-current pointwise gap (22-23, 323) | 0.0272 -> 0.0034 N m | 0.02719 -> 0.00339 | ok |
| Half-current std below band (325) | ~5e-5 N m | ref - edge = -0.00005 (0.1 %) | ok |
| Cogging std (23, 339) | -29.1 % -> +12.9 % | -29.088 -> +12.887 % | ok |
| 1.13-fold cogging scaling (340) | 1.13 | 0.01209/0.01071 = 1.129 | ok |
| Table 1 (tables/coupling.tex) | all nine rows | forward_harmonic/tables.md L3 rows, 5 d.p. | ok (verify.py asserts) |
| Order 45 vs conforming at L3 (347-349) | ~59 % cogging, 7-9 % loaded | 0.01209/0.00760 = +59.1 %; 0.03961/0.04279 = -7.4 %; 0.08036/0.08829 = -9.0 % | ok |
| Order 189 within 1.1e-4 N m at L1, L2 (349-350) | 1.1e-4 | 0.00010 (L1), 0.00007 (L2); changelog records max 1.0175e-4 | ok |
| Nyquist-limit identity (253-255) | 4.1e-9 (L0), 1.3e-9 (L1) | 4.06e-9, 1.24e-9 | ok |
| Estimator controls at L3 (365-367) | within 9e-5 N m | inner/outer 0.00009, coupling 0.00009 at full current | ok |
| Free ends at L3 (368-369) | < 5.4e-5 | 0.00005 (5.33e-5 per changelog) | ok |
| Post-filter diagnostic (399-403) | 0.0075->0.0184; 0.04280->0.04460; 0.08832->0.08775; cuts 0.0077/0.0300/0.0184/0.0077/0.0075 | forward/tables.md (v1 check) | ok |
| Sensitivity table (385-388) | 2.1e-6, 7.3e-8, 3.9e-4, 7.3e-5 | 2.06e-6, 7.27e-8, 3.86e-4, 7.30e-5 | ok |
| 1,140 harmonic solves, all at tolerance (186-187) | 1,140 | 19 runs x 60; all `tolerance` | ok |
| Node counts (278) | 3,153 / 9,937 / 36,083 / 139,268 | cost tables | ok |
| 800 / 782 / 1.07e-5 (26-27, 471-473) | 800, 782 <= 2.8e-7, max 1.07e-5 (Sr1 std) | design/README: 782 of 800, 1.1e-5, |dJ| <= 0.054 entries; verify.py recomputes from plus/minus values | ok |
| SW1 mean derivative, L1 h45 full (455-457) | +1.68 / -220.85 N m/m | study_L1_h45_initial.json d_mean[3] = 1.68373, d_mean_jinit[3] = -220.852 | ok |
| Two anchored-window failures (495-497) | two loaded SW1 ripple cases, widest window two decades | raw: 40 zero-decade anchored windows = 38 LENGTH (linear objective, documented as not an O(h^2) test) + 2 SW1/Jinit/conforming/std; README widest window 2.0 decades | ok (see S6 for the LENGTH caveat) |
| Zero-current mean best FD 7.0e-4; cogging std <= 8.9e-7 (498-500) | 7.0e-4; 8.9e-7 | README 7.0e-4; <= 8.8e-7 (verify asserts < 8.9e-7) | ok |
| SW3 sign change, ~O(h) (504-505) | yes | -0.784, -0.099, +0.406, +0.642 N m/m L0-L3 | ok |
| MW remesh gap 5.4e-3 -> 2.6e-6 (509-511) | yes | rm-mv slope +5.4e-3 (L0), +2.6e-6 (L2) | ok |
| Cost ratios (29-31, 525-527) | gradient 1.13-1.20; FD 7.2-9.5 | 1.13-1.20; 7.24-9.53 (9.53 rounds to 9.5) | ok |
| Table 3 (tables/gradient-cost.tex) | 0.664/0.748/6.333/0.011 etc. | cost_L*_h45_tol1e-14.json medians; L0/L1 `repeats: 3`, L2 `repeats: 1` | ok |
| Host load 14-24 (517) | 14-24 | design/README "load average at 14 to 24 (logs/cost.log)" | ok (not in the JSON; from the log) |
| M1 verification (259-268) | 2e-14, <1e-9, 1.002/2.007, 3.76e-4 | unchanged from v1 (m1_tables.log) | ok |
| Design points (Table 4) | 12 rows | study_L1_h45_{initial,perturbed}.json design_point; verify.py asserts | ok |

Equations (1)-(11) are unchanged from v1 where they overlap and were re-derived there; the new ones are also correct: Eq. (6) n = 3:6:45 gives eight orders and 16 sine/cosine constraints; Eq. (7) is the standard saddle tangent; Eq. (8) T_c = -L eta^T d(C^T u)/d beta is the envelope-theorem torque of the constrained energy (matches Egger et al. and the README derivation); Eq. (11) is the correct discrete adjoint with the multiplier block in z.

## Focus questions from the dispatch

### Is the headline now a crisp result?

Mostly, with one framing gap. The thesis sentence is clear ("A motor optimization target can change with the numerical interface even when average torque agrees closely", abstract) and the title matches the content. But the quantitative support offered in the abstract, introduction and conclusion is all *reproduction-framed*: conforming is 9.9-11.3% above the saved reference, order-45 brings it to 1.3-1.7%. A referee can read that as "GEODE's conforming model was wrong and matching the reference fixed it". The paper's own stronger and cleaner evidence is in §5.2 and Fig. 3b: on a fixed mesh, every order below Nyquist reproduces the conforming band to 1e-9 N m, the ladder is flat above order 189, and the cut at 45 shifts cogging std by 59% and loaded std by 7-9% while the mean moves 0.1-0.2%. That is the statement "the published ripple objective is model-dependent at ~10% because of a truncation choice, not a discretization detail", and it does not appear in the abstract, the introduction or the conclusion. The discussion (541-546) gestures at it ("interface truncation changes the objective even as spatial error decreases") without the number. This is the single highest-leverage edit (S1).

The paper is properly careful not to say which model is closer to the physical motor (352-354). That is correct: the harmonic truncation is a modelling choice the reference made (a mortar with eight modes), and GEODE's conforming band on a polygonal gap with P1 is not a ground truth either.

### Model / estimator labelling

Adequate and explicit at the method level: "Forward comparisons use Arkkio for both GEODE couplings to isolate the interface change; coupling torque is a separate check. The derivative study uses coupling torque for the harmonic model and Arkkio for the conforming model" (245-249). Table 1 and Fig. 2 state Arkkio. Fig. 4 caption says "harmonic coupling, fixed current" for panel (b) without naming the estimator (coupling torque); Fig. 4a pools both model-estimator pairs without saying so. Minor (S8). The "800 comparisons" count is clearly scoped as recorded cases, not a factorial (477-479).

### Best-step FD interpretation

Correctly labelled: "This is a best-step verification statistic, not the accuracy of a single finite-difference step chosen in advance" (474-475). As a referee I would still note that a best-of-13-steps error is selection-biased downward and is the weaker of the two tests; the Taylor-remainder O(h^2) window is the stronger one, and the paper reports it only for one explicit sweep plus two failures. The raw records contain the anchored window for all 800 cases: 515 of 800 span >= 5 decades, 685 >= 4 decades, 38 are LENGTH cases where the objective is linear (no O(h^2) test by construction), 2 are the SW1 cases named. One sentence with that distribution would convert the best-step statistic from "we picked the best step" into "the remainder is quadratic over 4-6 decades in 86% of cases" (S6).

### Fixed-current vs fixed-group-1-density

Framed well. The rule is defined precisely (447-453: I* proportional to |Omega_1(X)| s, constant fixed at the initial design, unscaled geometry evaluated and torque multiplied by s^2), the agreement at the initial point and disagreement of derivatives is stated, and the sign change is attributed to the winding-area/current term with the FD validation under the Jinit rule backing "not an adjoint discrepancy" (design README: Jinit SW1 best FD 6.1e-11). The paper draws the right conclusion for a future comparison (459-460). One omission worth a clause: under the group-1 rule the *group-2* density is not fixed (areas differ by 16%, line 149), so "fixed group-1 density" is the accurate name and the paper should say that this is why the reference's rule is named by group (S9). Also a nit: the figure is in N m/mm and the text in N m/m (S5).

### Timing caveats and the speedup question

No optimization speedup is claimed anywhere; the paper says so three times (33, 529-531, 579-581). The cost disclosure is complete (CPU, cores, default threading, load 14-24, threads unpinned, 3 repeats at L0/L1, 1 at L2, warm-started FD). Two referee reactions remain. First, the abstract devotes a sentence to "1.13-1.20 forward evaluations, compared with 7.2-9.5 for warm-started central differences": that is the textbook adjoint-vs-FD ratio for 12 parameters and 10 operating points and is not a finding of this paper; placing it in the abstract invites the reader to treat it as a result and to multiply. Either drop it from the abstract or state it as the expected adjoint scaling (S4). Second, "concurrent host load averaging 14-24" on 28 cores makes Table 3 unreproducible as absolute numbers; ratios are more robust but not immune, because the forward Newton loop and the adjoint phase may have different parallel efficiency under contention. The paper says as much; D5 takes a point for it.

### Related work (harmonic coupling methods, Arkkio, Ion et al. 2018)

The v1 gap is closed. Bontinck et al. 2018 (harmonic stator-rotor coupling with IGA) and Egger et al. 2022 (torque from harmonic mortar energy) are cited for exactly the formulation the reference uses and the torque estimator the paper re-derives (80-86, 244-245). Arkkio 1987 is cited at the estimator (213-214); Arkkio & Hannukainen 2012 at mesh selection for torque (89-90). Ion et al. 2018 is cited for the gradient-vs-stochastic PMSM comparison (92-98), and the paper draws the correct consequence that such a comparison alone is not new. The paragraph on inexpensive alternatives (100-109) is unchanged and adequate. Still missing, none critical: the classical FE-discretization-vs-cogging literature (Howe & Zhu 1992 or equivalent) for the "mean agrees, ripple does not" phenomenon; a reference for the sliding-band / moving-band conforming coupling the paper calls "the sliding interface" (205-206); and a source for the single-circle Maxwell-stress sensitivity claim (392-393). The related-work section reads as a chain of "X is established, so Y would not be novel" (73-78, 80, 94-96, 561-562), which is positioning by disclaimer rather than by contrast; it is accurate but costs D9.

## Scoring with quoted evidence

**D1 Rigor 5/6.** The interface experiment is designed so the only change is the coupling ("Table 1 compares the same geometry, material and excitation with two interface models" -- §5.1), and it carries the two checks a referee would ask for: an identity check ("Increasing the transmitted orders to every mode below the trace Nyquist limit reproduces the conforming Arkkio torques within 4.1e-9 N m on L0" -- §3.3) and an order ladder on a fixed mesh (§5.2). The gradient verification chain (central FD step sweeps, first-order remainders, tolerance study, fresh-remesh cross-check, mesh convergence of the gradient itself) is more complete than most published adjoint papers. Deductions: the cogging residual is left without arbitration (no P2, no third code, no reference refinement), and the best-step FD statistic is the headline verification number while the stronger Taylor-window distribution is reported only anecdotally.

**D2 Evidence 5/6.** 1,640 forward solves over four meshes, 800 recorded FD cases at two designs, two couplings and three meshes, and the controls needed to exclude estimator and end-pairing effects ("coupling torque and the inner- and outer-half Arkkio estimates are each within 9e-5 N m of full-band Arkkio" -- §5.2). Deduction: model dependence is demonstrated at one geometry (the initial design) while the pinned evidence already contains the optimized-design forward runs (`motor_reference/parametric/`) that would show the optimized target is also model/discretization-dependent (ripple 0.00035 vs saved 0.00018 N m at full current with matched coupling); and the cogging amplitude remains "12.9% high, with reference and geometry errors still unbounded" (abstract).

**D3 Clarity of contribution 4/5.** The contribution is extractable in one sentence per item from §1 paragraph 3 ("We report a four-mesh comparison of conforming and truncated harmonic coupling ... We also verify derivatives of mean torque and ripple with respect to 12 design parameters ... and expose the effect of holding current rather than current density fixed" -- §1), and the title matches. Deduction (buried lede, major): the organizing result, that order-45 truncation shifts the objective by 59% / 7-9% at fixed mesh while every order below Nyquist reproduces the conforming band to 1e-9 N m, is stated only in §5.2 ("At L3, order 45 and the conforming model differ in ripple by about 59% at zero current and 7-9% under load" -- §5.2); the abstract and conclusion carry only the reproduction numbers, and "The contribution is a reproducible account of model and derivative errors in this published benchmark" (§1) undersells it.

**D4 Related-work positioning 4/5.** Substrate-backed by `refs/literature-screen.json` and the new identifier-verified entries ("Bontinck et al. use separate isogeometric subdomains and harmonic interface constraints; their exact circular geometry avoids polygonal air-gap error. Egger et al. derive torque from the energy of a harmonic mortar formulation" -- §2). Ion 2018 restored with the correct consequence drawn. Deductions: no cogging/discretization-sensitivity reference for the central phenomenon; no reference for the conforming sliding interface; the section positions by novelty disclaimer four times rather than by what this study adds to each line.

**D5 Reproducibility 4/5.** Commit-pinned ("The evidence used here is fixed at GEODE commit 910593ba" -- Code and data availability), per-file SHA-256 manifest, a verify.py that recomputes the FD errors from plus/minus objective values rather than trusting summaries, CPU and run counts named. Deductions: "concurrent host load averaging 14-24. The effective thread count was not pinned" (§6.3) makes Table 3 unreproducible; mesh and solver commands are "separately documented" rather than given; gmsh/Rust versions are not stated in the paper; L1-L3 meshes are regenerated, not archived.

**D6 Figure & table quality 3/4.** Figures are self-contained with units and tell the story ("All GEODE curves use Arkkio torque, so the paired difference changes the coupling while holding the estimator fixed" -- Fig. 2 caption; "Colors identify current, not coupling" -- Fig. 3 caption). Deductions: Fig. 4c is in N m/mm while the text quotes N m/m, and the SW1 fixed-current bar (+0.0017 N m/mm) is invisible, so the sign reversal the text emphasizes cannot be read from the panel; Table 1's last column header "Std deviation" is ambiguous (deviation of the std) and the harmonic zero-current mean prints "-0.00000"; Fig. 4 occupies a page that is 30% blank; Fig. 3b uses an evenly spaced categorical axis for orders 21-381.

**D7 Prose & structural quality 3/4.** Flow is intact, tense consistent, no AI-tell adjectives (grep for honest/genuine/transparent/load-bearing/emphatically: 0), no `Section ??`, no overfull boxes (preflight). Deductions: a version-history leak ("the earlier order-45 post-processing diagnostic" -- §5.3; there is no earlier diagnostic in this paper); "with reference and geometry errors still unbounded" (abstract) reads as "infinite" to a cold reader (mean "not yet bounded"); the Table 1 caption sentence "Relative zero-current mean error is not used" is telegraphic; the rendered PDF has no author line; "Detailed logs accompany the code" (§4) is a pointer, not a sentence for this audience.

**D8 Citation hygiene 4/5.** Every `\cite` resolves (verify.py), all added DOIs are resolver-verified per `citation-notes.md`, and the estimator and coupling claims now carry sources ("We average Maxwell stress over the air gap using the Arkkio expression" -- §3.3, cited). Deductions: `arkkio2012proper` is typed `@article` with `journal = {International Symposium on Power Electronics Power Electronics, Electrical Drives, Automation and Motion}` (a conference, with a duplicated phrase; renders that way on p. 13); "the reference's degree-two isogeometric discretization with 3x3 subdivisions per patch" (§5.3) is still stated without saying it was read from the snapshot; "Single-circle Maxwell-stress estimates are more sensitive to the piecewise-constant field than the gap average" (§5.3) is uncited.

**D9 Rhetorical economy 3/4.** Far tighter than v1; the abstract leads with the thesis and the protocol is down to two paragraphs. Deductions: the post-processing-diagnostic paragraph ("For completeness, the earlier order-45 post-processing diagnostic changed the cogging standard deviation from about 0.0075 to 0.0184 N m" -- §5.3) is addressed to the previous reviewers, not the reader; the novelty disclaimers repeat ("Differentiable motor design is established prior work" -- §2; "Harmonic rotor--stator coupling is also established" -- §2; "would not establish a new optimization result" -- §2; "their combination alone is not a new contribution" -- §7); the FD-vs-adjoint cost sentence in the abstract spends headline space on an expected number; Appendix A remains tangential to the paper's result.

## Critical-flag check

- Citation error: none found; all keys resolve, added entries inspected against the claims they support.
- Numerical inconsistency: none; 30 claims recomputed or read from raw artifacts (table above); automated detector 0 findings.
- Missing experiment for a claim: none. Each claim in the abstract has its experiment; the cost sentence is explicitly scoped to "recorded gradient evaluations ... under uncontrolled host load".
- Close prior work ignored: no; the harmonic-coupling and gradient-vs-stochastic lines are now engaged.
- Build: PDF present, 14 pages, 0 overfull, bib clean.
- Audience: 0 governance/private-locator hits; no issue or PR numbers in main.tex (the README mentions PR #1180, which is outside the paper body).

## Blockers

None at the critical-flag level. The paper advances on score (35/44). The two items below are **major** and should be fixed before any deposit:

**M1. Headline framing (lines 15-23, 37-43, 573-576).** State the fixed-mesh model-dependence result where the reader looks for the result. Suggested abstract sentence after line 23: "On a fixed mesh, transmitting every order below the trace Nyquist limit reproduces the conforming torque to 1e-9 N m, while the order-45 cut shifts the loaded ripple objective by 7-9% and cogging by 59%; the published ripple target is therefore a property of the truncation, not of spatial resolution." Mirror it in §1 paragraph 1 and §8.

**M2. Version-history leak (lines 397-405).** "the earlier order-45 post-processing diagnostic" refers to v1. Per the voice guide the reader has only this paper. Replace with "A post-processing filter of the solved conforming field at order 45 ..." or, better, delete the paragraph down to its last two sentences (which carry the point).

## Should-fixes (with main.tex line refs)

S1. Lines 15-33 (abstract), 37-43, 573-581: see M1. Also reconsider the title: "Interface truncation and verified design gradients" names two activities; a result-based title ("Harmonic-interface truncation sets the torque-ripple objective of a published motor benchmark at the 10% level") would carry the finding.

S2. Lines 397-405: see M2; the paragraph is reviewer-facing.

S3. Lines 436-445 and Table 1: use the optimized-design evidence in `motor_reference/parametric/` (pinned at 910593ba). The full Opt120 export under order-45 coupling at L3 gives mean torque -0.10% (half current -0.15%) but ripple 0.00035 N m against the saved 0.00018 N m at full current and 0.00055 vs 0.00044 at half current, with the reference outside the L3 band. That is the paper's thesis evaluated at the published optimum: the optimized ripple target is reproduced to within a factor of two only, while the mean is reproduced to 0.1%. One row in Table 1 (or a sentence in §5.1 and §7) makes the "optimization target" claim concrete at the point where it matters. Note the caveats the README records: the 9 scalar parameters alone miss the free-form rotor shaping by 1.4 mm.

S4. Lines 29-31 (abstract) and 525-531: either drop the 1.13-1.20 vs 7.2-9.5 sentence from the abstract or add "as expected for an adjoint over 12 parameters and 10 operating points". Keep it in §6.3 with the existing caveats. A referee will otherwise read the abstract as a derivative-cost speedup claim against the weakest baseline.

S5. Lines 455-457 and Fig. 4c: unify units (text N m/m, figure N m/mm). Make the SW1 sign reversal visible: annotate the +0.0017 N m/mm bar with its value, or use a symmetric-log axis, or add a small inset for SW1.

S6. Lines 470-479, 494-497: report the Taylor-window distribution from the recorded cases alongside the best-step statistic. From the raw study JSON: of the 800 loaded cases, 515 have an anchored O(h^2) window >= 5 decades and 685 >= 4 decades; 38 of the 40 zero-width windows are LENGTH, where the objective is linear and the remainder is round-off (not an O(h^2) test), and the remaining two are the SW1 cases already named. State that LENGTH and ScaleR are excluded from the window statistic by construction. This also pre-empts the "best-of-13-steps selection bias" objection.

S7. Lines 447-453: add one clause explaining why the rule is named by group: group-2 density changes under it because the two winding areas differ by 16% (line 149).

S8. Fig. 4 caption (483-491): name the estimator in panel (b) ("coupling torque") and say that panel (a) pools harmonic/coupling-torque and conforming/Arkkio cases.

S9. Lines 392-395: cite a source for single-circle Maxwell-stress sensitivity on P1 fields, or rephrase as an observation from the recorded estimator study.

S10. Lines 407-409: "as read from the public snapshot" for the degree-2 / 3x3 IGA statement.

S11. `refs.bib` `arkkio2012proper`: change to `@inproceedings` with `booktitle = {International Symposium on Power Electronics, Electrical Drives, Automation and Motion (SPEEDAM)}`; the current field duplicates "Power Electronics" and renders that way on p. 13.

S12. Line 23 (abstract) and 407: "still unbounded" -> "not yet bounded".

S13. Table 1 caption (315-317) and header: rename the last column "Std vs reference (%)" and drop "Relative zero-current mean error is not used" in favour of "n/a: the zero-current mean is ~1e-4 N m, so a relative error is not meaningful". Print the harmonic zero-current mean as 0.00000, not -0.00000.

S14. Lines 338-343 and 407-417: add the one hedged observation the artifacts support about the cogging residual: GEODE's own order-45 cogging std rises with refinement (0.0101, 0.0119, 0.0121, 0.0121 from L0 to L3) and the coarse-IGA reference sits at 0.0107, so a coarse discretization of this model sits below the refined value. This points toward reference under-resolution without claiming to bound it, exactly as the forward_harmonic README states.

S15. Lines 62-109: reduce the four novelty disclaimers to one, and position each line by what this study adds (same-mesh paired coupling comparison; order ladder with a Nyquist identity check; discrete design-gradient verification at two designs with an excitation-rule ablation).

S16. Line 8 / rendered title block: the PDF has no author. Fine for a draft, must be resolved before deposit (BRIEF says authorship open).

S17. Lines 601-613 (Appendix A): keep only if §7 continues to cite it; otherwise two sentences in §7 suffice.

## Nits

- Line 186: "All 500 baseline and sensitivity solves, and all 1,140 additional harmonic-coupling solves" is fine; Table 3's "ten-angle sweeps" solves are a third workload and are not counted anywhere in the body; a half-sentence in §6.3 would close the count.
- Line 276: "Detailed logs accompany the code." Delete or fold into the availability section.
- Line 279: "$120\,2^\ell$" renders as "120 2^l"; "$120\cdot 2^{\ell}$" is clearer.
- Fig. 3b x-axis: consider log spacing or tick labels at the true orders; the even spacing suggests a linear ladder.
- Page 10 is a figure-only page with a large blank; `[t]` placement or moving Table 3 up would fill it.
- numeric-consistency: automated detector ran (`uv run python -m anvil.lib.numeric_consistency`, 226 numbers, 1 claim checked, 0 findings); manual claim-vs-claim pass performed in addition (table above).

## Summary

v2 answers the v1 reviews' main demand: the decisive experiment (the harmonic-truncated solve on identical meshes) was run, and it turns "two codes disagree on ripple, cause unknown" into a result: with the reference's eight orders the loaded ripple gap drops from 10-11% to 1.3-1.7% with mean torque unchanged, the order ladder is flat above 189, and every order below Nyquist reproduces the conforming band to 1e-9 N m, so the published ripple objective is set by a modelling cut at order 45 at the ~10% level (59% for cogging). The discrete design gradients are verified more thoroughly than is usual, the excitation-rule sign reversal is a useful and correctly attributed observation, no speedup is claimed, and the timing caveats are complete. All 30 numbers I checked match the merged artifacts; the pre-flights are clean; no critical flag.

The paper scores 35/44 and advances. The gap between the evidence and the score is almost entirely framing: the abstract, introduction and conclusion present the reproduction numbers and leave the fixed-mesh model-dependence numbers in §5.2, one paragraph still speaks to the v1 reviewers, the FD-vs-adjoint cost sentence sits in the abstract as if it were a finding, and the optimized-design runs already in the pinned evidence go unused. Fixing M1, M2 and S3-S6 would move the paper comfortably above threshold without new computation.

Procedural: review written to this scratchpad file per dispatch; nothing under `/Volumes/Stripe/geode-fem/review-1171v2` or `/Volumes/Stripe/geode-fem/review-main` was created or modified. `web_search: true` is set in the BRIEF but no web tool was available in this session; the D4 leads (Howe & Zhu 1992, sliding-band references) are from reviewer memory and should go through `paper-litsearch` before entering `refs.bib`.
