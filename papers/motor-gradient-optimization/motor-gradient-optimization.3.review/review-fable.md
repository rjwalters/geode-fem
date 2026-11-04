# Review: motor-gradient-optimization.3 (PR #1171, head c5f83e45)

Reviewer: Fable (Anvil `paper-review` role; independent, skeptical referee stance from computational electromagnetics / electrical-machine design optimization). Verdict anchored to head c5f83e45; v2 scores do not carry over.
Reviewed: `/Volumes/Stripe/geode-fem/review-1171v3/papers/motor-gradient-optimization/motor-gradient-optimization.3/main.tex` (678 lines; 15 pp PDF; 5 figures, 4 tables, 13 references), with `tables/*.tex`, `changelog.md`, `verify.py`, `verified-numbers.json`, `README.md`, `preflight.json`, `citation-notes.md`, `figures/src/render_v3.py`, `../BRIEF.md`, `../EXPERIMENTS.md`, and the v2 reviews/audit at `/Volumes/Stripe/scratch-codex-review/motor-paper-v2-reviews/`.
Evidence: `origin/main` of `/Volumes/Stripe/geode-fem/review-main` fetched (00c4d9db). All 78 manifest sources were pulled with `git show origin/main:<path>` and their SHA-256 matched the manifest (78/78, 0 mismatches), so the paper's snapshots are byte-identical to merged main. Every headline number below was recomputed from those blobs with my own script (`scratchpad/motor-paper-v3/scripts/recheck.py`), not read back from `verify.py` or the paper's summaries.
Rubric: installed `.anvil/skills/paper/rubric.md`, `anvil-pub-v2`, **/44, advance threshold 35**, plus `.anvil/skills/paper/voice.md`.
Rendered PDF: all 15 pages rasterized and inspected; figure pages 7, 8, 10, 11 re-rendered at 160 dpi.
Worktree: nothing under `review-1171v3` or `review-main` created or modified (`git status` clean after the review). `verify.py` was run on a scratch copy.

## Verdict

**Total: 37 / 44**
**Decision: advance: true** (above threshold; no critical flag).
**Critical flags: none.**
**Outstanding dependencies: none** (`anvil.lib.pending_marker`: 0 markers).

V3 does what the v2 reviews asked. The model-dependence result is now the headline in all three places a reader looks (abstract lines 19-22, introduction 41-46, conclusion 619-621), stated as a standard-deviation shift relative to conforming coupling on a fixed mesh; the 800-case count is deduplicated to 704 with the 96 repeats explained; the Taylor-window distribution (530 of 592 eligible cases at four or more decades) now accompanies the best-step statistic; the FD cost includes the shared base forward (8.2-10.5); the cross-model gradient comparison is reported with its own figure and labelled as a combined model-plus-estimator change; the audited 6.82e-10 is corrected and guarded; the version-history leak, the unit mismatch, the bib defects and the empty-legend figures are all gone. Every number I recomputed from `origin/main` matches. The deterministic checks pass: `verify.py` exit 0 (78 hashes, 1,640 statuses, 704 distinct cases, 592 windows); `anvil.lib.numeric_consistency` 260 numbers, 0 findings; `anvil.lib.audience_check` 0 hits; `preflight.json` 0 overfull boxes, 0 placeholders. Zero em-dashes, zero virtue words, zero version-history phrases, no "certified", no novelty claims.

What keeps the score at 37 rather than higher: one sentence in the abstract overstates what the identity check tested (S1); the cross-model comparison cannot, with the recorded data, separate estimator from interface and the norm statistic it reports is dominated by two large components (S3); the forward model-dependence evidence is at one geometry, with the optimized-design evidence correctly deferred; cogging remains unarbitrated; timing remains uncontrolled.

| # | Dimension | Score | Weight |
|---|---|---|---|
| 1 | Rigor of method / argument | 5 | 6 |
| 2 | Evidence sufficiency | 5 | 6 |
| 3 | Clarity of contribution | 5 | 5 |
| 4 | Related-work positioning | 4 | 5 |
| 5 | Reproducibility | 4 | 5 |
| 6 | Figure & table quality | 3 | 4 |
| 7 | Prose & structural quality | 3 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 3 | 4 |
| | **Total** | **37** | **44** |

### Top 3 revision priorities (advisory; paper advances)

1. Abstract line 21-22: "All resolved interface orders reproduce conforming torque within a few 1e-9 N m" is not what was measured. The 1e-9 identity holds only when every order below the trace Nyquist limit is transmitted (L0 4.1e-9, L1 1.2e-9); the ladder step at order 189 reaches 1.0e-4 N m. Say "Transmitting every order below the trace Nyquist limit reproduces ...". The body (253-257, 353-354) is already precise; only the abstract is loose.
2. Section 6.3 (536-540): the box-scaled norm differences (0.58/0.66% mean, 6.7/10.2% ripple) are weighted toward SW2 and MW, the two largest components. Give the per-parameter ratio range alongside, which is what an optimizer following the gradient would feel: at L2 full current the ripple ratio h45/conforming is 0.66 (MAG), 0.71 (SW1), 1.25 (SD1), 1.33 (Sr1) at the initial design and 0.40 (MAG), 0.70 (SW1), 1.92 (SD1), 1.40 (Sr1) at the perturbed design, while every mean ratio lies in 0.84-1.11.
3. Fig. 4(b) and Fig. 5: say in the captions what the figure actually draws. Fig. 4(b) plots the remainder divided by the objective value (axis label) while the text (472-473) defines the absolute remainder; Fig. 5 draws a gray segment joining every harmonic/conforming pair without saying so.

## Resolution of v2 findings

### Fable v2 (35/44)

| v2 item | v3 status | Judgment |
|---|---|---|
| M1 / S1: fixed-mesh model-dependence numbers buried in §5.2 | Abstract 19-22, intro 41-46, conclusion 619-621 now lead with "truncation at the reference's harmonic order 45 changes torque standard deviation by 7--9% under load and 59% at zero current, relative to conforming coupling", with the identity check named. Title retained (changelog explains). | **Resolved.** The one loose sentence is S1 below. |
| M2 / S2: version-history leak, reviewer-facing filter paragraph | Paragraph replaced by three sentences (398-404) with no "earlier"; eight numbers removed, pointer to records retained. | **Resolved.** |
| S3: use Opt120 forward runs in `parametric/` | **Deferred, correctly.** The `parametric/README.md` at the evidence commit itself states that full-band Arkkio "is not valid for the full iteration-120 rotor, whose surface is not the 44 mm circle" (the surface moves inward to 42.6 mm); #1183 tracks the estimator-domain guard and #1182 (the validation matrix including Opt120 on three meshes with h45/h93/conforming) is still in review. The numbers I quoted in v2 were therefore not safe to print. The paper makes no Opt120 numerical claim and keeps the scalar/free-form distinction (440-444, 667-669). | **Withdrawn by this reviewer in favour of waiting for #1182.** |
| S4: FD-vs-adjoint cost sentence in abstract | Removed from the abstract; §6.4 keeps it with "The cost ratio concerns derivative evaluation on one loaded host. It provides no measurement of optimizer iterations, surrogate calibration cost or time to a validated target" (575-577). | **Resolved.** |
| S5: Fig. 4c units and invisible SW1 bar | Units N m/m in text and figure; symlog axis, linear within +-2 N m/m; +1.68 and -220.85 annotated (496, Fig. 4c). | **Resolved.** |
| S6: Taylor-window distribution | "530 of 592 loaded comparisons have an anchored quadratic-remainder window spanning at least four decades in step size; 388 span at least five" with the factor-two definition and anchors (503-508); LENGTH/ScaleR excluded as analytic; the two SW1 failures retained. `verify.py` reconstructs every window. Recomputed independently: 592 / 530 / 388, the two zero-width windows are SW1/jinit/conforming/std at L0 and L1. | **Resolved.** |
| S7: why the rule is named by group | "Group-2 density is not held fixed because the ratio of the two winding areas can change" (453-454). | **Resolved.** |
| S8: estimator labels in Fig. 4 caption | Panel (a) "pooling harmonic/coupling-torque and conforming/Arkkio"; panel (b) "harmonic coupling and coupling torque" (489-492). | **Resolved.** |
| S9: uncited single-circle Maxwell claim | Recast as "In the recorded estimator study, single-circle Maxwell-stress estimates vary more with the integration radius than the half-band Arkkio estimates" (398-399); Howe-Zhu added for the general concern (92-93). | **Resolved.** |
| S10: IGA provenance | "as read from the public snapshot" (408). | **Resolved.** |
| S11: arkkio2012proper bib type | `@inproceedings`, SPEEDAM booktitle; renders correctly on p. 14. | **Resolved.** |
| S12: "unbounded" | "not yet bounded" (406); removed from abstract. | **Resolved.** |
| S13: Table 1 header, -0.00000 | "Std vs ref. (%)"; harmonic zero-current mean prints 0.00000; caption "n/a identifies the reference itself". The suggested near-zero-mean note was declined with a correct reason (the column is std, not mean). | **Resolved.** |
| S14: cogging refinement sequence | "0.0101, 0.0119, 0.0121 and 0.0121 N m on L0--L3, compared with 0.0107 N m in the reference. This is consistent with a coarse-model underestimate, but does not bound the reference discretization error" (344-347). | **Resolved**, hedged as requested. |
| S15: novelty disclaimers | Four disclaimers cut; §2 now says what the study adds: "Here the paired interface study, order ladder and excitation-rule comparison test which model and policy the computed design gradients represent" (81-82). | **Resolved.** |
| S16: empty author line | Still `\author{}` (8); rendered PDF has no author. BRIEF says authorship open. | **Carried**; must be resolved before any deposit. |
| S17: Appendix A | Retained; §7 cites it once (613). | **Acceptable.** |
| D4 nit: "sliding interface" uncited | Reworded to "Our conforming interface identifies their potentials" (207), describing the implementation rather than naming a published method. | **Resolved.** |
| D1/D5: cogging arbitration, mesh archives, controlled timing | Not changed; require new experiments (#1189 underway, outside the freeze). Limitations explicit (406-416, 558-560). | **Open by design**; scored as limitations, not defects. |
| Nits: "Detailed logs accompany the code", `120\,2^\ell`, third workload count, page-10 blank | All applied (276, 280, 568-569; Fig. 4 now shares p. 10 with text). | **Resolved.** |

### Opus v2 (36/44) and audit, spot-checked

F1 best-step qualifier in abstract (27): present. F2 704/688 with 96 repeats (26-28, 475-480, Fig. 4a): present and recomputed. S1 Jinit effect (453-465): seven of ten, three sign reversals, rotor parameters unaffected, tooth/slot mechanism: present and recomputed (changed = SW1, SW2, SW3, SW4, SD1, Sr1, ScaleR; flipped = SW1, SW4, SD1). S3 cross-model gradients: new §6.3 and Fig. 5. S4 SW2 non-monotone conforming sequence (519-522): present, recomputed (-46.5, -41.3, -44.4, -47.2; h45 -49.8, -48.1, -47.3, -47.1). S5 remesh scatter and 3.8% SW1 ripple (523-527): present, recomputed (MW mean fit 2.56e-6, scatter 6.34e-4; SW1 std 3.80e-2). S6 FD step and base forward (565-568, 572-573, Table 3): present. S7 Table 2 caption names conforming/Arkkio: present. S10 Fig. 3 coupling legend, Fig. 2 purple dash-dot: present. S11/audit bib: wiesheu2025combined journal version, egger article number 6, arkkio2012proper type: present. Audit critical flag (6.83e-10): corrected to 6.82e-10 (654), recomputed 6.8247e-10, guarded by `verify.py` (`assert r'6.83\times10^{-10}' not in tex`).

## Numbers vs merged artifacts (recomputed from `origin/main` blobs)

| Claim (main.tex line) | Paper | Recomputed | Status |
|---|---|---|---|
| L3 order-45 vs conforming std (19-21, 41-43, 351-353, 619-621) | 7-9% loaded, 59% zero current | -7.4% (half), -9.0% (full), +59.2% (zero) | ok (sign not stated; see S2) |
| Loaded mean vs reference, either model (43-44) | within 0.16% | conforming -0.153/-0.156%; h45 -0.002/+0.051% | ok |
| Identity check (22, 253-256) | 4.1e-9 (L0), 1.3e-9 (L1) | 4.06e-9, 1.24e-9 | ok (abstract wording, S1) |
| Order 189 at L1/L2 (353-354) | within 1.1e-4 N m | 1.02e-4, 6.6e-5 | ok |
| L1 ladder normalized ripple (Fig. 3b) | plotted | 21: 2.41/0.46/0.26; 45: 1.64/0.93/0.91; 93: 1.00/0.99/0.99; 189, 381: 1.00 | ok |
| Table 1 (tables/coupling.tex) | nine rows | all nine recomputed at 5 d.p. | ok |
| Ripple vs reference (23-24, 312-313, 340) | +9.9/+11.3 -> +1.7/+1.3; -29.1 -> +12.9 | +9.86/+11.27 -> +1.70/+1.27; -29.09 -> +12.89 | ok |
| Max full-current pointwise gap (324) | 0.0272 -> 0.0034 | 0.027192 -> 0.003390 | ok |
| Cogging std L0-L3, reference (344-346) | 0.0101/0.0119/0.0121/0.0121; 0.0107 | same | ok |
| 704 distinct, 688 <= 2.8e-7, max 1.07e-5 (26-28, 475-481, Fig. 4a) | | 800 records, 96 duplicates (jinit MAG/MH/MW only, by exact sweep equality), 704 distinct, 688, worst 1.0668e-5 (Sr1 std, L0 h45 initial, half current) | ok |
| Taylor windows (503-505) | 592; 530 >= 4; 388 >= 5 | 592 / 530 / 388 | ok |
| Two SW1 anchored-window failures (508-510) | two loaded SW1 ripple cases | the two zero-decade windows are SW1/jinit/conforming/std at L0 and L1 | ok |
| Zero-current best errors (511-513) | mean 7.0e-4; std 8.9e-7 | 7.04e-4; 8.85e-7 | ok |
| SW1 mean derivative, two rules (456-458) | +1.68 / -220.85 N m/m | 1.68373 / -220.852 | ok |
| Seven of ten changed; SW1/SW4/SD1 reversed; MAG/MH/MW identical (461-464) | | 7 changed; flipped SW1, SW4, SD1; MAG/MH/MW identical | ok |
| SW3 harmonic sign change (517-518) | sign change between coarse levels | -0.784, -0.099, +0.406, +0.642 | ok (see S8 on "approximately linearly") |
| SW2 sequences (519-522) | conf -46.5/-41.3/-44.4/-47.2; h45 -49.8 -> -47.1 | same | ok |
| Remesh L2 (523-527) | MW fit 2.6e-6, scatter ~6e-4; SW1 ripple 3.8% | 2.56e-6, 6.34e-4; 3.80% | ok |
| MAG ripple derivative, two pairs (533-536) | 1.28 vs 1.94; 0.75 vs 1.87 | 1.283 vs 1.937; 0.753 vs 1.865 | ok |
| Box-scaled norm differences (537-540) | 0.58/0.66% mean; 6.7/10.2% ripple | 0.58/0.66; 6.67/10.24 | ok |
| Box widths (Fig. 5 caption) | 9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm | same, from `reference_data.json` bounds | ok |
| Cost ratios (571-573, Fig. 4d) | gradient 1.13-1.20; FD+base 8.2-10.5 | 1.126-1.199; 8.243-10.532 over all 12 cost records | ok (10.53 rounds to 10.5) |
| Table 3 (tables/gradient-cost.tex) | 0.664/0.748/6.997/0.011 ... | h45 tol 1e-14 medians; Base+FD = forward + fd24 | ok |
| Slotless (653-654) | 3.08e-10; 6.82e-10 | 3.0772e-10; 6.8247e-10 | ok (audit flag cleared) |
| 500 + 1,140 solves at tolerance (187-188) | | verify.py asserts; both counts reproduced | ok |

Equations (1)-(11) are unchanged from v2 where they overlap and were re-derived there. No new equations in v3.

## Focus questions from the dispatch

**Is the headline now the model-dependence result?** Yes. Abstract sentence 3 (19-21), introduction sentence 2 (41-43) and conclusion sentence 1 (619-621) all state the fixed-mesh standard-deviation shift relative to conforming coupling, and the introduction names the two checks that distinguish it from spatial error (45-46). The reproduction numbers (9.9-11.3% to 1.3-1.7%) follow as the second statement, which is the right order. The abstract's closing sentence ("The results separate discrete derivative accuracy from model-dependent objectives and sensitivities") ties the two halves of the paper together in one line. The one defect is S1: the identity-check sentence in the abstract claims more than the ladder shows.

**Fixed-mesh std framing.** Correct and consistent: "population standard deviation" is defined (285-290) and every headline percentage is a std ratio with the conforming value as denominator (352-353, Fig. 3b axis "Ripple / conforming ripple"). The paper properly refuses to say which model is closer to the machine (355-358, 542-543).

**800 -> 704/688 dedup.** Correct, disclosed in abstract, body and Fig. 4a caption. My independent dedup by exact sweep equality (not by parameter name) finds exactly 96 duplicates, all jinit MAG/MH/MW, matching the paper's mechanism (rotor parameters leave winding area unchanged, 463-464).

**Taylor-window reporting (530/592).** Correct and now the stronger of the two verification statistics is in the body with its definition (anchor at h = 1e-5 m, factor two on remainder/h^2). The 112 LENGTH/ScaleR exclusions are explained as analytic. The two SW1 failures are retained.

**FD base cost 8.2-10.5.** Correct; Table 3 column "Base + FD" equals forward + fd24 in every row; Fig. 4d plots the same (10.53, 10.34, 10.19); the FD step is stated (566-567). No speedup inferred.

**Box-scaled cross-model comparison.** Fairly labelled: "Coupling and torque estimator both differ between the two series" (Fig. 5 caption) and "Because the estimator changes with the model, this experiment measures their combined effect; it does not isolate an interface-only derivative error" (540-543). I checked whether the recorded data could have done better: the study records carry one objective per model (`d_mean`, `d_std` from coupling torque for h45, Arkkio for conforming; no `d_*_arkkio` on the harmonic side), so the confound cannot be removed without new runs. The combined-effect framing is therefore the only defensible one. Does it support the Discussion claim (590-591, "The paired gradient comparison also measures changes in ripple sensitivities despite small discrete derivative errors")? Yes: ripple vectors differ by 6.7/10.2% in box-scaled norm and by factors 0.40-1.92 per parameter, against discrete errors of at most 1.1e-5. The norm statistic undersells the per-parameter picture (S3).

**6.82e-10.** Corrected (654), recomputed, guarded.

**All five figures.** Inspected at 160 dpi. Fig. 1 geometry: clean, units, legend. Fig. 2 waveforms: conforming purple dash-dot, harmonic teal, reference orange dashed; residual panels have independent scales and say so. Fig. 3: coupling legend added; order axis log-spaced with true tick values. Fig. 4: symlog panel (c) with annotations; panel (d) now includes the base forward; panel (a) 704 cases. Fig. 5: four-panel symlog dot comparison, readable, box widths in caption. Defects are small and listed in S4-S6.

**Opt120 deferral.** Justified; see the resolution table. The evidence README at the pinned commit already disqualifies the full-band Arkkio estimator on the free-form rotor, #1183 is open, and #1182 (the full validation matrix) is in review. The paper neither uses nor hides the records (both snapshots remain in the manifest).

**No speedup claim.** Confirmed: abstract 33-34, §6.4 573-577, §8 627-629 each say time to a validated target is unmeasured.

**Voice rules.** 0 virtue words, 0 "load-bearing", 0 em-dashes (`---` absent; `--` only in ranges), 0 version-history phrases, 0 issue/PR numbers in the body, no "certified", no "first"/"novel". "Not yet bounded" (406), "estimated refinement interval ... not a proven error bound" (294-303), remesh values stated as estimates with scatter (523-527). Methods are credited accurately: the sign reversal is attributed to the winding-area/current term with the FD under the jinit rule as support, not to the adjoint.

## Scoring with quoted evidence

**D1 Rigor 5/6.** The interface experiment is paired and controlled ("Forward comparisons use Arkkio for both GEODE couplings to isolate the interface change" 246-247), carries an identity check and an order ladder, and the gradient chain now reports the Taylor-window distribution, the dedup, mesh convergence of the gradients themselves, remesh scatter rather than a fit, and a cross-model comparison labelled as a combined effect. Deductions: the cogging residual still has no arbitration experiment ("These limitations preclude assigning the residual cogging error to a single cause" 415-416); the cross-model comparison changes two things at once and so cannot attribute; timing is under "concurrent host load averaging 14--24" (559-560).

**D2 Evidence 5/6.** 1,640 forward solves on four meshes, 704 distinct FD cases at two designs, three meshes, two model-estimator pairs and two rules, with estimator and end-pairing controls ("each within 9e-5 N m of full-band Arkkio" 371-372). Deductions: forward model dependence is shown at one geometry (initial design) with the optimized-design evidence deferred for a sound reason; the cross-model gradient comparison is at L2 and full current only (the L2 records contain only full current, so this is a data limit, not an omission); cogging amplitude "remains unresolved" (24-25).

**D3 Clarity of contribution 5/5.** Each contribution is extractable in one sentence from §1 paragraph 3 (56-64) and restated in the abstract and conclusion: fixed-mesh truncation changes the ripple objective 7-9%/59%; discrete gradients verified at two designs; ripple sensitivities differ between model-estimator pairs; excitation policy reverses three mean sensitivities. The v2 buried-lede failure is fixed. The title names activities rather than the result, which the changelog defends; acceptable.

**D4 Related-work positioning 4/5.** Howe-Zhu and Merkel-Gangl-Schöps added and primary-source checked; the section now says what this study adds (81-82) instead of what it does not claim. Deduction: for the central phenomenon the section does not say whether Egger et al. 2022 or Bontinck et al. 2018 report order-truncation sensitivity of ripple, so a reader cannot tell if the 7-9%/59% shift quantifies a known effect or is a new observation (S10); the inexpensive-alternatives paragraph (102-110) remains a list of three citations with no comparison among them.

**D5 Reproducibility 4/5.** Commit-pinned, 78-file manifest byte-identical to main, `verify.py` recomputes FD from plus/minus values, FD step and repeats stated, software versions for figure rendering stated (changelog). Deductions: Table 3 is unreproducible as absolute numbers ("The effective thread count was not pinned" 559-560; L2 single run); meshes are regenerated rather than archived and the mesh/solver versions are not in the paper.

**D6 Figure & table quality 3/4.** Figures are self-contained with units and tell the story ("All GEODE curves use Arkkio torque, so the paired difference changes the coupling while holding the estimator fixed" Fig. 2; "Colors identify current, not coupling" Fig. 3). Deductions: Fig. 4(b) axis "Remainder / objective" is a normalized quantity while the text defines the absolute remainder and the caption does not say which; the dotted guides in Fig. 4(b) are not identified as O(h^2) lines; Fig. 5's gray connector segments are not explained; Table 1 prints "%" in every cell of a column already headed "(%)"; p. 7 has a large blank between Fig. 2 and Table 2.

**D7 Prose & structural quality 3/4.** Flow intact, tense consistent, no AI-tell adjectives, no version history, no `??`, 0 overfull boxes. Deductions: the abstract sentence "All resolved interface orders reproduce conforming torque within a few 1e-9 N m" (21-22) overstates the identity check; "At L1 with harmonic coupling, coupling torque and full current, the mean-torque derivative" (454-456) stacks four qualifiers before the subject; the 7-9% and 59% changes are given without sign anywhere in the prose; the rendered PDF has no author line.

**D8 Citation hygiene 5/5.** All 13 keys resolve; every added entry is primary-source checked in `citation-notes.md` with resolver records archived; the four audit defects are fixed and render correctly (p. 14-15); the two previously uncited claims are now either cited (Howe-Zhu) or recast as observations from the recorded study (398-399); the IGA statement is sourced to the snapshot (408). I found no claim that needs a citation and lacks one.

**D9 Rhetorical economy 3/4.** The filter paragraph, the four novelty disclaimers and the cost sentence in the abstract are gone; the abstract leads with the thesis. Deductions: §6.2's last two paragraphs (503-528) still carry seven distinct caveats in 26 lines; §4's first paragraph (260-270) reports solver verification results that belong in the availability section or an appendix for this audience; Appendix A is cited once and could be two sentences in §7.

## Critical-flag check

- Citation error: none; all keys resolve, added entries verified against the claims they support.
- Numerical inconsistency: none; 27 claims recomputed from `origin/main` blobs (table above); `numeric_consistency` 260 numbers, 0 findings.
- Missing experiment for a claim: none. Each abstract claim has its experiment. The Discussion's sensitivity claim (590-591) is now measured (§6.3) rather than conjectured.
- Close prior work ignored: no; the harmonic-coupling, torque-estimator, cogging-discretization and gradient-vs-stochastic lines are engaged.
- Build: PDF present, 15 pages, 0 overfull, bib clean, `preflight.json` compile_status ok. `artifact_verify` (`python3 verify.py`) exit 0 on a scratch copy.
- Audience: 0 governance/private-locator hits; no issue or PR numbers in the body.
- Pending markers: 0.

## Blockers

None.

## Should-fixes (with main.tex line refs)

S1. Lines 21-22 (abstract): "All resolved interface orders reproduce conforming torque within a few $10^{-9}$ N m on the two checked meshes" reads as if each order in the ladder does so; only the all-orders-below-Nyquist run does (order 189 reaches 1.0e-4 N m at L1). Replace with "Transmitting every order below the trace Nyquist limit reproduces conforming torque within a few $10^{-9}$ N m on the two checked meshes". Line 45 ("all-resolved-order identity checks") is fine.

S2. Lines 19-21, 41-43, 351-353, 619-621: give the sign. Truncation at order 45 lowers loaded standard deviation by 7-9% and raises cogging by 59%. The sign is what an optimizer would see as an objective shift, and Fig. 3b shows it; the prose never does.

S3. Lines 536-540: the Euclidean-norm statistic is dominated by SW2 and MW. Add the per-parameter range: at L2 full current the ripple ratio (harmonic/coupling over conforming/Arkkio) spans 0.66-1.33 at the initial design and 0.40-1.92 at the perturbed design, with MAG, SW1, SD1 and Sr1 outside 0.75-1.25, while every mean ratio lies within 0.84-1.11. A cosine on the box-scaled vectors (all in mN m, so no mixed units) is also legitimate: 0.99999/0.99998 for mean, 0.9997/0.9979 for ripple.

S4. Fig. 4(b) caption (491-494) and line 472-473: the axis is "Remainder / objective" (normalized by |F(p)|) while the text defines the absolute remainder. Say "normalized by the objective value" in one of the two, and identify the dotted lines as $h^2$ guides.

S5. Fig. 5 caption (547-554): "Gray segments join the two estimates for each parameter" (render_v3.py draws one for every parameter; they are visible only where the pair differs).

S6. `tables/coupling.tex`: drop the "%" from the cells or "(%)" from the header.

S7. Lines 454-456: "At L1 (harmonic coupling, coupling torque, full current), the mean-torque derivative with respect to SW1 is ...".

S8. Lines 517-518: "converge approximately linearly" for SW3. The h45 increments are 0.685, 0.505, 0.236 N m/m (ratios 0.74, 0.47), which is not clearly first order on four levels. "Increments shrink with refinement" is what the data show.

S9. Line 8: `\author{}` renders no author. Must be resolved before deposit (BRIEF says authorship open).

S10. Lines 84-91: say whether Egger et al. 2022 or Bontinck et al. 2018 report order-truncation sensitivity of the ripple, and at what level, so the reader can place the 7-9%/59% fixed-mesh shift against them. This is positioning, not a novelty claim.

S11. Lines 260-270: the M1 solver-verification paragraph (2e-14, 1e-9, 1.002/2.007, 3.76e-4) is for the solver's record rather than this paper's argument; two sentences plus the availability link would do.

## Nits

- p. 7: Fig. 2 and Table 2 leave roughly a third of the page blank; `[t]` for Table 2 or moving it to p. 6 would fill it.
- p. 15: two references on an otherwise empty page; harmless.
- Fig. 4(d): legend sits inside the plot between the two curves; fine, but a lower-right placement would avoid crossing the FD line at L1.
- Line 572-573: "8.2--10.5 including the shared base solve" is correct; the L0 h45 tight-tolerance value is 10.53, which the text rounds down. No change needed.
- Line 613: "admits a three-coefficient angular fit at one geometry" is the only use of Appendix A; see S11/D9.
- `README.md` and `changelog.md` mention PR/issue numbers; they are package files, not paper body, so no voice violation.

## Summary

V3 is the paper v2 should have been: the fixed-mesh model-dependence result (order-45 truncation shifts the ripple objective by 7-9% under load and 59% at zero current on identical meshes, while every order below Nyquist reproduces the conforming band to 1e-9 N m and the loaded mean moves 0.1-0.2%) now leads the abstract, introduction and conclusion; the discrete-gradient verification is reported with deduplicated counts and the Taylor-window distribution; the derivative cost is like-for-like and no speedup is claimed; the new cross-model gradient comparison is correctly labelled as a combined model-plus-estimator change and measures what the v2 Discussion only conjectured; the audited number is fixed and guarded; the bibliography is clean. All 27 numbers I recomputed from `origin/main` match, the deterministic pre-flights pass, and the Opt120 deferral is the right call given what the evidence README and #1183 say about the estimator on the free-form rotor.

The paper scores 37/44 and advances with no critical flag. The remaining should-fixes are precision edits that need no new computation: one abstract sentence overstates the identity check (S1), the headline percentages lack a sign (S2), the cross-model norm statistic hides the per-parameter spread that an optimizer would feel (S3), and three caption/axis details (S4-S6). The experiments that would move D1/D2 to 6 (cogging arbitration, optimized-design model dependence, controlled timing) are correctly left outside this evidence freeze and are tracked in #1182 and #1189.

Procedural: review written to this scratchpad file per dispatch; nothing under `/Volumes/Stripe/geode-fem/review-1171v3` or `/Volumes/Stripe/geode-fem/review-main` was created or modified (`git status` clean). `verify.py` was run on a copy at `scratchpad/motor-paper-v3/v3copy/`. Recheck script at `scratchpad/motor-paper-v3/scripts/recheck.py`; extracted `origin/main` blobs at `scratchpad/motor-paper-v3/evidence/`. `web_search: true` is set in the BRIEF but no web tool was available in this session; citation metadata was checked against `citation-notes.md` and `citation-metadata-v3.json` rather than live resolvers.
