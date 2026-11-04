# paper-review: motor-gradient-optimization.3 (PR #1171, head c5f83e45)

Reviewer: Opus (anvil:paper-review, read-only). This is a fresh score of the v3 manuscript at
head `c5f83e45`. Scores from v2 do not carry over. Nothing in `/Volumes/Stripe/geode-fem/review-1171v3`
or `/Volumes/Stripe/geode-fem/review-main` was modified: the worktree `git status` is clean, and
review-main was only fetched and read through `git show`. No `.3.review/` sidecar was staged, per
dispatch. This file is the full review.

Rubric: the installed `/Users/rwalters/GitHub/geode-fem/.anvil/skills/paper/rubric.md`, 9 dimensions,
**/44, advance threshold ≥35**. House rules: `.anvil/skills/paper/voice.md`.

## Verdict

- **Total: 38 / 44**
- **advance: true.** The score clears the threshold and there are no critical flags.
- **Critical flags: none.** Every number I checked matches the pinned evidence, and I recomputed the
  new cross-model comparison independently from the 910593ba files. `python3 -I -B verify.py`
  passes. All 78 manifest snapshots are byte-identical (SHA-256) to `origin/main`, and 910593ba is
  an ancestor of `origin/main`.
- **Outstanding dependencies: none.** There are no `[PENDING]` markers.
- **Artifact verify** (`.anvil.json` `python3 verify.py`, cwd `.3`): PASS, exit 0.

| # | Dimension | v3 score |
|---|---|---|
| 1 | Rigor of method / argument | 5 / 6 |
| 2 | Evidence sufficiency | 5 / 6 |
| 3 | Clarity of contribution | 4 / 5 |
| 4 | Related-work positioning | 4 / 5 |
| 5 | Reproducibility | 5 / 5 |
| 6 | Figure & table quality | 4 / 4 |
| 7 | Prose & structural quality | 3 / 4 |
| 8 | Citation hygiene | 5 / 5 |
| 9 | Rhetorical economy | 3 / 4 |
| | **Total** | **38 / 44** |

Top priorities (the paper advances, so these are advisory; items 1 and 2 should be fixed before audit):

1. **Scope the "three reversals" claim** in the abstract and conclusion (S1). The count holds only at
   the stated L1/h45/full-current state. Four sensitivities reverse at L2 initial in both models, and
   at L1 conforming. SW1, SW4 and SD1 reverse in every loaded state.
2. **Use the pinned L3 gradients to anchor §6.3** (S2). The ripple gap between the two model pairs
   persists at L3 (6.6% and 10.3%), while each model changes by only 0.5–1.2% from L2 to L3. The
   mean gap at L2 (0.58%) is about the size of the conforming model's own L2→L3 change (0.54%).
   The paper currently measures this gap against finite-difference error, which is the wrong
   comparison.
3. **State the optimized-design (Opt120) deferral in the paper** (S3), and fix the ambiguous
   abstract sentence "All resolved interface orders..." (S4).

## Resolution of v2 review and audit items

| v2 item | v3 status | Evidence |
|---|---|---|
| Audit critical flag: slotless max 6.83e-10 | **Resolved.** L654 reads 6.82e-10. Raw `heldout_max_abs_error_over_analytic_amplitude` = 6.8247e-10 in `origin/main:benchmarks/motor_optimization_screen/slotless_screen.json`, byte-identical to the snapshot. verify.py now derives both mantissas from the raw values and rejects the string 6.83. | Recomputed |
| Opus F1: best-step qualifier missing from abstract | **Resolved** (L27 "best-step central differences"). | |
| Opus F2: 96 duplicate records | **Resolved.** 800 → 704 distinct, 688 ≤ 2.8e-7, worst 1.07e-5, stated at L475-482 and in the abstract and Fig. 4(a). verify.py asserts byte-equality of each duplicated sweep. | verify.py |
| Fable M1/S1: fixed-mesh std result buried | **Resolved.** The abstract (L19-21), introduction (L41-43) and conclusion (L619-621) now lead with the order-45 vs conforming **standard-deviation** shift at L3: 7–9% loaded, 59% at zero current. Recomputed from Table 1: −7.4%, −9.0%, +59.1%. | Table 1 |
| Fable M2/S2, Opus S9: eight-number filter paragraph, version history | **Resolved** (L401-404, one sentence plus a pointer). No version-history words remain. | grep |
| Opus S1, Fable S7: full excitation-policy effect | **Resolved at the stated state** (L454-465: 7 of 10, SW1/SW4/SD1, rotor parameters unchanged, physical mechanism). The abstract generalization is new and is raised as S1. | |
| Opus S2, Fable S5: Fig. 4(c) units and visibility | **Resolved.** N m/m, symlog, +1.68/−220.85 annotations. | Fig. 4 |
| Opus S3/D2: measured model-dependent gradients | **Resolved, with a new section** (§6.3, Fig. 5). Correctly labelled as changing both model and estimator. I recomputed it exactly (below). The yardstick needs strengthening (S2). | Recomputed |
| Opus S4: SW2 non-monotone | **Resolved** (L519-522). | README at 910593ba |
| Opus S5: remesh 2.6e-6 cancellation | **Resolved** (L523-527: the fit is retained only alongside its 6e-4 scatter, and the 3.8% SW1 ripple discrepancy is named). | verify.py |
| Opus S6, Fable S4: fair FD cost, step, abstract | **Resolved.** Timing has been removed from the abstract. h = 1e-6 in stored units (L565-566). Base forward added: 8.2–10.5 (L572-573). | Recomputed |
| Opus S7/S8, Fable S8: estimator labels | **Resolved** (Table 2 caption, Fig. 4 caption, Fig. 5 legend and caption). | |
| Opus S10: legends and colours | **Resolved.** Fig. 2 uses purple dash-dot vs teal. Fig. 3 has its own coupling-style legend. | |
| Fable S6: Taylor-window distribution | **Resolved** (L503-510: 530 of 592 ≥4 decades, 388 ≥5; definition and anchors stated). | Recomputed |
| Fable S9: single-circle estimator unsourced | **Resolved** as an observation (L398-400). Howe–Zhu is cited for the general concern only (L92-93). | |
| Audit citation defects 1–3 | **Resolved** in refs.bib (Arkkio 2012 @inproceedings, Wiesheu 2025 journal DOI, Egger article 6). The rendered bibliography is clean. | PDF p.14-15 |
| Fable S3: add Opt120 reproduction values | **Deferred, and the reason given is sound.** The changelog says so directly. Issue #1183 (both-band Arkkio guard when rotor offsets change the inner gap band) and #1182 are still OPEN on the forge, so using the 910593ba Opt120 numbers now could import an estimator-domain error. **However, the manuscript does not tell the reader** that an optimized-design reproduction was set aside. See S3. | gh issue view |
| Opus S12, Fable S16: empty author | Still `\author{}` (L8). The deferral is explained in the changelog. It must be resolved before deposit (process only, not scored). | |

## Codex focus checks

| Check | Finding |
|---|---|
| **Fixed-mesh std framing** | Correct and consistently worded: "truncation at the reference's harmonic order 45 changes torque standard deviation by 7--9\% under load and 59\% at zero current, relative to conforming coupling" (L19-21). §5.2 (L351-353) and the conclusion match. Direction is not stated: order 45 *lowers* the loaded std and *raises* the cogging std. One clause would add information (nit N3). In the conclusion, "cogging by 59%" (L620) should read "cogging standard deviation" (N4). |
| **Deduplication 800→704/688** | Correct. 704 = 800 − 96. verify.py rebuilds every FD from raw plus/minus values and checks that the 96 fixed-density MAG/MH/MW sweeps are `==` to their fixed-current counterparts. The abstract, L475-482 and the Fig. 4(a) caption and label ("688 / 704 ≤ 2.8 × 10⁻⁷") agree. |
| **530/592 eligible Taylor windows** | Correct. My own pass over the snapshot JSONs gives 592 eligible (704 minus 112 analytic LENGTH/ScaleR), with window widths (rounded to the nearest decade) of {6: 189, 5: 256, 4: 99, 3: 41, 2: 5, 0: 2}. The exact threshold counts, 530 ≥4 and 388 ≥5, come from verify.py's unrounded test and pass. The two zero-width failures are SW1 ripple, **conforming/fixed-density**, at L0 and L1 initial. The paper does not name the pair or rule (N5). The 62 sub-4-decade cases are mostly SD1 (35) and Sr1 (18). Their median relative gradient is 0.15, against 29.8 for the rest, which supports "Smaller gradients reach round-off sooner" (L508). |
| **Fair FD base cost 8.2–10.5** | Correct. From the pinned `design/README.md` cost table, (FD24 + forward)/forward ranges from 7.24+1 = 8.24 (L2 conforming 1e-8) to 9.53+1 = 10.53 (L0 h45 1e-14). The gradient-to-forward ratio is 1.13–1.20. The Table 3 cells match the `cost_L*_h45_tol1e-14.json` medians (verify.py asserts the strings), e.g. L0 6.997 = 6.333 + 0.664. Setup values 0.011/0.041/0.222 s match the README's 0.01–0.23 s. The caveats are complete: M3 Ultra, 28 cores, default threads, load 14–24, threads not pinned, 3 repeats / 1 at L2 (L558-560, Fig. 4 caption). |
| **Box-scaled cross-model comparison** | **Labelled correctly as changing both model and estimator.** The text says "Because the estimator changes with the model, this experiment measures their combined effect; it does not isolate an interface-only derivative error" (L540-543). The Fig. 5 caption says "Coupling and torque estimator both differ between the two series" (L551-552), and the legend reads "Harmonic (order 45) / coupling torque" vs "Conforming / Arkkio". §3.3 (L248-250) and the pinned README "Couplings" bullet confirm h45 = coupling torque and conforming = both-band Arkkio. **Independent recomputation** from `git show 910593ba:benchmarks/motor_shape_gradient/design/runs/study_L2_{h45,conforming}_{initial,perturbed}.json` (SHA-256 e8191219…, a1d7a682…, 87bd6e89…, 5cb77437…, identical to the manifest), with widths from `reference_data.json` `optimization_parameter_bounds` (9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm, matching L553-554), using ‖w⊙(g_h45 − g_conf)‖ / ‖w⊙g_conf‖ at full current: **mean 0.5758% / 0.6646%, ripple 6.6727% / 10.2355%**, which display as 0.58/0.66% and 6.7/10.2%. **MAG ripple 1.2832 / 1.9371 (initial), 0.7534 / 1.8655 (perturbed) N m/m**, which display as 1.28/1.94 and 0.75/1.87. The denominator choice is stated. With the h45 norm as denominator the values are 0.57/0.66% and 7.1/11.1%, so the conclusion does not depend on the choice. No cosine is reported, as the changelog says. Two weaknesses are raised as S2 and S5. |
| **Corrected slotless 6.82e-10** | Correct (see the table above). The RMSE 3.08e-10 also matches the raw 3.0772e-10. |
| **Opt120 deferral** | Accurate in the changelog. In the manuscript it is visible only by implication: L440-444 ("reproducing its 12 scalar values alone does not reproduce that geometry... does not include... a new optimized motor"), L599 ("The strongest forward comparison concerns the initial design") and L668-669. Two Opt120 run snapshots (`parametric/runs/opt120_L3_h45.json`, `opt120scalar_L3_h45.json`) and the parametric README are in the paper's own evidence manifest, but the text never mentions them. The paper is not silent in a misleading way, because it claims nothing about the optimum. Still, "strongest" at L599 implies that weaker forward comparisons are reported, and they are not. S3. |
| **No speedup claim** | Confirmed. "The cost ratio concerns derivative evaluation on one loaded host. It provides no measurement of optimizer iterations, surrogate calibration cost or time to a validated target" (L575-577). "A practical speedup still requires optimization against inexpensive baselines" (L628-629). The abstract ends with "optimization time to a common validated target remains to be measured" (L33-34). A grep for speedup/faster/tenfold finds only L628. |

## Additional numeric verification

- Table 1 vs text: harmonic means −0.0024% / +0.051% (L311, shown as −0.002%), std +1.70/+1.27/+12.89%,
  conforming +9.86/+11.27/−29.09%, means −0.153/−0.156% (verified-numbers.json; verify.py
  recomputes these from raw waveforms). "within 0.16%" (L44) holds.
- Abstract L21-22, body L254-256: hall vs conforming 4.1e-9 (L0) and 1.3e-9 (L1) N m are asserted by
  verify.py, so "a few 1e-9 on the two checked meshes" holds.
- L187 and L568-569: 500 + 1,140 = 1,640 solves, all `tolerance`, per verify.py.
- L456-457: SW1 +1.6837 / −220.852 N m/m, per verified-numbers.json and the raw `study_L1_h45_initial.json`.
- L461-463: 7 changed and reversals at indices SW1/SW4/SD1 at this state, per verify.py. My scan
  across *all* loaded states is under S1.
- L520-522: SW2 conforming −46.5/−41.3/−44.4/−47.2 and harmonic −49.8 → −47.1 match the pinned README.
- Table 4 design points: verify.py asserts every displayed string.
- Fig. 3(a) L3 endpoints (harmonic zero ≈ +13%, conforming zero ≈ −29%, loaded ≈ +1–2% / +10–11%) and
  Fig. 4(d) (≈10.5, 10.3, 10.2; ≈1.13–1.15) are visually consistent with the tables.
- numeric-consistency: the automated detector was not run, because the `.anvil` venv was not used
  against the worktree. A manual claim-vs-claim cross-check over the abstract, introduction, §5–§8
  and the tables found no text/table/figure contradiction.

## Scoring with quoted evidence

**D1 Rigor 5/6.** The interface experiment isolates one change ("Forward comparisons use Arkkio for
both GEODE couplings to isolate the interface change; coupling torque is a separate check" — §3.3).
It carries an identity check and an order ladder, and the refinement interval is correctly tiered
("This interval is a refinement diagnostic, not a proven error bound" — §4). The gradient chain is
thorough: deduplicated best-step FD, an anchored Taylor-window distribution recomputed from raw
remainders, the remesh scatter, and gradient mesh convergence. Deductions: §6.3 measures the
cross-model gap against the wrong reference ("These differences are much larger than the discrete
finite-difference errors" — §6.3), when the relevant comparison is each model's own refinement
change, and the evidence for that is already pinned (S2). The abstract and conclusion also generalize
a single-state reversal count (S1).

**D2 Evidence 5/6.** The paired forward study (1,640 solves, four meshes, ladder, estimator and
free-end controls) supports the headline. §6.3 now supplies measured support for the claim that
sensitivities depend on the model ("the MAG ripple derivative is 1.28\,N\,m/m for harmonic/coupling
torque versus 1.94 for conforming/Arkkio at the initial design" — §6.3). Deductions: the forward
comparison is still at one design ("The strongest forward comparison concerns the initial design" —
§7), the cogging amplitude remains unresolved, and the L3 gradients that would show the ripple gap
is not an L2 artifact are pinned but unused (S2). The missing optimization comparison is scoped out
of the claims and is not deducted.

**D3 Contribution 4/5.** The lede is result-first and quantified ("At the finest mesh, truncation at
the reference's harmonic order 45 changes torque standard deviation by 7--9\% under load and 59\% at
zero current" — Abstract). The two halves now connect through a measured result rather than a
conjecture ("Ripple sensitivities nevertheless differ between the verified model--estimator pairs" —
Abstract). Cold-reader check: the contributions can be extracted in three sentences (interface
truncation changes the ripple objective, verified discrete gradients, model- and policy-dependent
sensitivities). Deductions: the abstract's key control sentence is ambiguous ("All resolved
interface orders reproduce conforming torque within a few $10^{-9}$\,N\,m" — Abstract), which a cold
reader can take as contradicting the 59% figure (S4). The title still names activities rather than
the finding (Fable S1, retained with a reason given).

**D4 Related work 4/5.** The paper positions itself against established motor adjoints, harmonic
coupling and optimizer comparisons, and is clear about what it inherits ("We inherit the initial
geometry, material table and excitation conventions; we replace the field discretization and compare
two rotor--stator couplings" — §2). New engagement: "\citet{merkel2021shape} use isogeometric shape
derivatives and harmonic rotor--stator coupling" (§2) and Howe–Zhu for cogging sensitivity. Remaining
gap: Merkel et al. 2021 is the closest prior work on gradients through a harmonic interface, and it
gets one sentence. The paper does not say whether that work examined the transmitted order's effect
on the optimized objective, which is the paper's central observation. There is no litsearch sibling
(no deduction for that). The sliding/moving-band literature is avoided by describing the conforming
interface as the authors' own implementation, which is acceptable.

**D5 Reproducibility 5/5.** "The evidence used here is fixed at GEODE commit" 910593ba (Code and data
availability), with commit-pinned links. The 78-file SHA-256 manifest is byte-identical to
`origin/main`. verify.py recomputes the FD values, Taylor windows, cost ratios, cross-model norms,
table strings and slotless mantissas from raw records. The FD step, units, host, thread policy and
load are stated. Timing is not reproducible in absolute terms, but it is no longer a headline claim.

**D6 Figures/tables 4/4.** All five figures were inspected at print resolution, and Figs. 1 and 4
again at 200 dpi. The captions are self-contained and name the model/estimator pairs ("All GEODE
curves use Arkkio torque, so the paired difference changes the coupling while holding the estimator
fixed" — Fig. 2). Fig. 4(c)'s symlog axis and annotations make the reversal visible. Fig. 5's
signed symlog dot plot keeps small derivatives visible, and its caption defines $w_k$ and the
linear region. Two cosmetic overlaps do not reach a substantive objection (N1, N2): the "coupling
circle r = 44.5 mm" label sits on the gap band in Fig. 1(b), and the "Fixed group-1 density" legend
text runs into the top of the Sr1 bar in Fig. 4(c). The tables are aligned and have unit-bearing
headers.

**D7 Prose 3/4.** The voice rules hold. A grep finds no virtue words, "load-bearing", "certified",
novelty claims, issue numbers or version history. "--" appears only in ranges and compounds, and the
build has no overfull boxes or `??`. Deductions: the ambiguous abstract sentence (S4); the jargon
compound "The order ladder and all-resolved-order identity checks distinguish this model dependence
from spatial refinement error" (§1); the empty `\author{}`; and §6.2's dense final two paragraphs,
which stack seven caveats.

**D8 Citation hygiene 5/5.** The three audited bib defects are fixed and the rendered bibliography is
complete (authors, venue, year, DOI). Methods are cited where they are used ("We average Maxwell stress over
the air gap using the Arkkio expression \citep{arkkio1987}" — §3.3), and the T_c formula is tied to
\citet{egger2022torque}. The formerly unsourced estimator claim is recast as an observation. No
unsupported-citation instance was found. The two new entries (Howe–Zhu 1992, Merkel 2021) were
checked by the authors against primary sources and should be spot-checked in the audit; no web tool
was used in this pass.

**D9 Economy 3/4.** Tighter than v2, with the filter paragraph cut to a pointer. Remaining cost:
the paper often closes a paragraph with a "this is X, not Y" disclaimer ("This is a best-step
verification statistic, not the accuracy of a finite-difference step chosen in advance" — §6.2;
"These are verification results for the forward model; they do not verify motor shape derivatives"
— §4; "This is an angular interpolation test at one geometry, not a shape-gradient test" —
Appendix A). Each is accurate, but together they slow a reader. §6.2 L503-528 would read faster as
a small caveat table (zero-current, remesh, mesh convergence). The headline 59% / 7–9% appears four
times (Abstract, §1, §5.2, §8), which is acceptable for a headline.

## Critical-flag check

- Citation error: none. All `\cite` keys resolve (verify.py), and there is no `[??]` in the PDF.
- Numerical inconsistency: none between text, tables and figures. The abstract's "three" (S1) is a
  scope over-generalization. Fig. 4(c) and §6.1 match it at the stated state, so it does not
  contradict any reported number. It should still be fixed before audit, because an auditor scanning
  all states would find four.
- Missing experiment for a claim: none. The optimization and speedup claims are explicitly absent.
- Build failure: none. preflight.json reports compile ok, 15 pages, 0 overfull boxes, bib_lint ok,
  and the PDF SHA matches the head's main.pdf (68ad400e…).
- Close prior work ignored: none found.

## Blockers

None.

## Should-fix (main.tex line refs)

- **S1 (L30-31 abstract; L626 conclusion).** "reverses three initial-design mean-torque
  sensitivities" holds at the §6.1 state (L1, h45/coupling torque, full current) and at L0 h45. Across
  the recorded loaded states, SW1, SW4 and SD1 reverse **in every state**. A fourth also reverses at
  L1 conforming (SW2, both designs and currents), at L2 initial in **both** models (SW3) and at L0
  conforming. Suggested wording: "...reverses the SW1, SW4 and SD1 mean-torque sensitivities in every
  loaded state recorded". Alternatively, name the state in the abstract.
- **S2 (L535-543; Fig. 5).** Replace "much larger than the discrete finite-difference errors" with
  the relevant comparison, which is already in the pinned evidence
  (`gradient_L3_{h45,conforming}_{initial,perturbed}.json`, in the manifest). Box-scaled, at L3 the
  cross-model ripple gap is **6.58% / 10.25%**, while each model's own L2→L3 ripple-gradient change
  is **0.46–0.49% (initial) and 1.13–1.19% (perturbed)**. The ripple difference is therefore not a
  discretization artifact at L2. The mean gap falls from 0.58/0.66% (L2) to 0.16/0.26% (L3), and the
  conforming model's own L2→L3 mean change is 0.52–0.54%. The L2 mean "agreement" is therefore at
  the level of refinement error and is not a model statement. One sentence, plus a verify.py
  assertion, closes the gap.
- **S3 (L599, L440-444 or §7).** State the optimized-design deferral for the reader, without
  process framing. For example: "The saved optimized design, which adds free-form surface offsets, is
  not reproduced here; its gap-band torque evaluation on the displaced rotor requires a separate
  check." Change "The strongest forward comparison concerns the initial design" to "The forward
  comparison covers only the initial design." The Opt120 snapshots are in the paper's own manifest, so
  a reader who opens it will otherwise wonder why they are unused.
- **S4 (L21-22 abstract; L45).** "All resolved interface orders reproduce conforming torque within a
  few $10^{-9}$\,N\,m" can be read as "every order in the ladder reproduces conforming", which the
  59% figure contradicts. Use the body's wording: "Retaining every order below the trace Nyquist
  limit reproduces conforming torque within a few $10^{-9}$\,N\,m on the two checked meshes." At L45,
  "all-resolved-order identity checks" → "full-order identity checks".
- **S5 (L537-540).** The norm metric is dominated by MW, MAG and SW2. In the mean at L2 initial, the
  component values are SW2 −141.9 vs −133.3 mN m (6.5%) and SW3 0.41 vs 0.49 mN m (17%). Add "the
  norm is dominated by the three largest components" or name SW2. SW2 is the parameter the paper
  already identifies as exposed to Arkkio mesh motion (L519-522), and linking the two would strengthen
  §6.3.

## Nits

- **N1 (Fig. 1(b), L122-128).** Move the "coupling circle r = 44.5 mm" label off the gap band, e.g.
  below the axis region with the arrow kept.
- **N2 (Fig. 4(c), L487-501).** The legend text overlaps the Sr1 bar. Move the legend to the lower
  left or above the axes.
- **N3 (L19-21, L42-43).** Give the direction: order 45 lowers the loaded std by 7–9% and raises the
  cogging std by 59%.
- **N4 (L620).** "cogging by 59%" → "cogging standard deviation by 59%".
- **N5 (L508-510).** Name the two anchored-window failures as conforming/fixed-density SW1 ripple
  cases at L0 and L1, initial design.
- **N6 (L8).** Resolve `\author{}` before deposit.

## Procedural notes

- All 15 pages were rendered at 80 dpi and inspected. Figs. 1 and 4 were also rendered at 200 dpi.
  All five figures, four tables, the changelog, verify.py, evidence-manifest.json, verified-numbers.json,
  preflight.json and ../BRIEF.md were read.
- verify.py was run with `python3 -I -B` from the version dir: PASS. No files were written to the
  worktree.
- The recomputation scripts and the extracted 910593ba evidence are in this scratchpad's
  `motor-paper-v3/scripts/` and `motor-paper-v3/ev/` (`xmodel.py`, `l3.py`, `windows.py`).
- Prior v2 reviews and the audit (`/Volumes/Stripe/scratch-codex-review/motor-paper-v2-reviews/`)
  were used only to check resolution.
- `web_search: true` is set in the BRIEF, but no web tool was used. The D4/D8 observations about
  Merkel 2021 and Howe–Zhu 1992 should be confirmed in `paper-audit`.

## Summary

v3 scores **38/44 and advances**, with no critical flags. Every item raised in the v2 reviews and
audit is resolved, except the Opt120 values, which were deferred for a sound reason. The new box-scaled
cross-model comparison is labelled correctly as changing both coupling and estimator, and it
recomputes exactly from the pinned 910593ba files (0.58/0.66% mean, 6.7/10.2% ripple, MAG
1.28/1.94 and 0.75/1.87). The fixed-mesh std framing, the 704/688 deduplication, the 530/592 Taylor
windows, the 8.2–10.5 fair FD cost and the corrected 6.82e-10 all check out against merged evidence.
The paper makes no speedup claim. The two highest-leverage fixes need no new computation. First,
scope the abstract's "three reversals" (true at one state; SW1/SW4/SD1 reverse everywhere). Second,
use the pinned L3 gradients to show that the ripple-sensitivity gap survives refinement while the L2
mean gap does not. Separately, state the optimized-design deferral in §7 rather than only in the
changelog.
