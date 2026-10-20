# paper-review: motor-gradient-optimization.4 (PR #1171, head fea6ba5b)

Reviewer: Opus (anvil:paper-review, read-only). This is a fresh score of the v4 manuscript at head
`fea6ba5b5243f5513b8804bfcdc2117d6cf2ced9`. The v3 scores (38/44 and 37/44) do not carry over. I
modified nothing in `/Volumes/Stripe/geode-fem/review-1171v4`: `git status` was clean before and after.
verify.py and the hash check ran on a `git archive` copy in my scratchpad. No `.4.review/` sidecar was
staged, per dispatch. This file is the full review.

Rubric: the installed `/Users/rwalters/GitHub/geode-fem/.anvil/skills/paper/rubric.md`, 9 dimensions,
**/44, advance threshold ≥35**. House rules: `.anvil/skills/paper/voice.md`. origin/main = `a35591b9`
(it matches `ls-remote`).

## Verdict

- **Total: 38 / 44**
- **advance: false.** The score clears the threshold, but one critical flag blocks.
- **Critical flag (1): `artifact_verify_0`.** The declared artifact check (`.anvil.json`: `python3 verify.py`,
  cwd `.4`) **exits 1 at the PR head**: `FileNotFoundError: figures/src/data/m1_tables.log`. The manifest
  pins this snapshot (source `benchmarks/motor_nonlinear_m1/m1_tables.log` @ 581ee7ad, sha `ccc86e29…`),
  but the file is not committed. `papers/.gitignore:4` has `*.log`, so the v4 copy was never added.
  In v3 the same file was force-added and tracked. The preflight's "PASS: 112 evidence hashes" came from the
  author's working tree, where the file existed untracked. **Fix:** `git add -f
  papers/motor-gradient-optimization/motor-gradient-optimization.4/figures/src/data/m1_tables.log`.
  After I restored that one file from `origin/main` in a scratch copy, `python3 -I -B verify.py` passes
  both PASS blocks. This is the only blocker. It is a packaging defect, not a scientific one.
- **Outstanding dependencies: none.** There are no `[PENDING]` markers. Experiment 2 is described as a
  protocol with no results, which is correct.

| # | Dimension | v4 score |
|---|---|---|
| 1 | Rigor of method / argument | 5 / 6 |
| 2 | Evidence sufficiency | 6 / 6 |
| 3 | Clarity of contribution | 4 / 5 |
| 4 | Related-work positioning | 5 / 5 |
| 5 | Reproducibility | 4 / 5 |
| 6 | Figure & table quality | 3 / 4 |
| 7 | Prose & structural quality | 3 / 4 |
| 8 | Citation hygiene | 5 / 5 |
| 9 | Rhetorical economy | 3 / 4 |
| | **Total** | **38 / 44** |

Top priorities:

1. **Commit `m1_tables.log` with `git add -f`** (critical flag above). Then rerun verify.py from a clean
   `git archive` or a fresh clone, not from the working tree.
2. **Fix the conclusion's Opt120 current-policy sentence** (S1, L731). "fixing physical current also
   removes its mean-torque advantage" is wrong in direction. Opt120 has no mean-torque advantage under
   fixed *unscaled* current either (−8.9%). It is ahead only under the reference's group-1 density
   rule (+3.8%).
3. **Carry the "not fully converged" caveat into the abstract and conclusion** next to "12.9% to 0.10%"
   (S2). The 0.10% is a point agreement inside larger refinement uncertainty. The reference's own
   extrapolated limit is about 0.3% below its 24×24 value, and GEODE's L3 cogging band is ±1.6%.
4. **Disclose that Opt120's L2→L3 increments do not shrink** and that a second fresh-mesh family reaches
   3.2% (S3, L465-467). The abstract's ×3.6 sentence also needs to say that the ripple reduction stays
   above 2,400-fold (S4).
5. **Point the saved-reference displays at the refined reference** (S5: Table 1, Figs. 2-3 captions).
   Also report or plot the L3 values behind Fig. 6. The perturbed h45 MAG ripple component moves from
   0.75 to 0.57 N m/m at L3 (S6).

## Resolution of v3 review and audit items

The changelog was checked against both v3 reviews and the audit, item by item.

| v3 item | v4 status | Evidence |
|---|---|---|
| Opus S1: "three reversals" overgeneralized | **Resolved.** The state is named in the abstract (L32-33: "at the initial L1, order-45, full-current state") and the conclusion (L734-735). | main.tex |
| Opus S2: compare the cross-model gap with refinement | **Resolved.** L628-632 give the L3 ripple gaps 6.58/10.25% against within-model change 0.46-1.19%, and the mean gaps 0.16/0.26% against the conforming model's 0.52-0.54%. I recomputed them independently (below). | Recomputed |
| Opus S3, Fable v2 S3: Opt120 deferral | **Resolved by new evidence.** §5.5 uses the #1182 export meshes with coupling torque and outer-band Arkkio only. It states that both-band Arkkio is invalid on this geometry (L449-454). | Recomputed |
| Opus S4, Fable S1: "All resolved orders" ambiguity | **Resolved** (L21-23: "Retaining every order below the trace Nyquist limit"). | |
| Opus S5, Fable S3: norm hides components | **Resolved** for Opus (L625-627 name the SW2/SW3 6.5%/17% differences). Fable's blanket mean-ratio range is **declined with a sound reason** (the near-zero components and SW3 make it misleading). The cosine is declined; it is not needed for the claim. | Recomputed 6.5% / 16.5% |
| Opus N1: Fig. 1 label on gap band | **Resolved.** The label sits below the axes, and the arrow is clear (p. 3). | PDF |
| Opus N2, audit figure note: Fig. 4(c) overlap | **Resolved.** The legend is below the axes, and +1.68 is in empty space (now Fig. 5(c), p. 12). | PDF |
| Opus N3/N4, Fable S2: signs | **Resolved** in the abstract, §1, §5.2 and §8 ("lowers loaded … raises cogging"). | grep |
| Opus N5: name the Taylor-window failures | **Resolved** (L593-595). | |
| Opus N6, Fable S9: empty author | **Resolved.** `\author{Robb Walters}`, and the PDF Author metadata = "Robb Walters" (pdfinfo). | pdfinfo |
| Fable S4: Taylor normalization and guides | **Resolved** (Fig. 5 caption: "divided by $|F(p)|$, and dotted $h^2$ guides"). | |
| Fable S5: gray connectors | **Resolved** (Fig. 6 caption). | |
| Fable S6: duplicate % | **Resolved** (`tables/coupling.tex` has the unit in the header only). | |
| Fable S7: stacked qualifiers | **Resolved** (L539-540 parenthetical). | |
| Fable S8: "approximately linearly" | **Resolved** (L603: "their increments shrink with refinement"). | |
| Fable S10, Opus D4: harmonic-coupling positioning | **Resolved.** L95-99 state that Egger et al. varied the order from 39 to 129 against an order-200 reference, "Thus torque sensitivity to interface order is already established". I checked this against the arXiv PDF (2112.05572) with an anonymous request (no identity, no mailto): N = 39…129, N_eff 7…22, degree-200 reference, linear setting. It is supported. | Fetched |
| Fable S11: solver-verification paragraph | **Resolved** (condensed to one paragraph plus a pointer, L274-278). | |
| Fable layout nits | **Resolved.** The cost legend sits between the curves. Floats were checked on all 17 pages. | PDF |
| Audit: optional 1.3e-9 → 1.2e-9 | **Correctly declined.** The raw value is 1.2417e-9, so "within 1.3e-9" (L270) is the true upper bound, and verify.py asserts `< 1.3e-9`. The new "within 9.8e-13" (L417; raw 9.7344e-13) follows the same ceiling convention. | Recomputed |
| Audit: ICEM title case | **Resolved** (the refs.bib diff is that single line). | diff |

One changelog inaccuracy: the #1185 row, BRIEF L127 and EXPERIMENTS L9 all say "amendments 1-4".
#1185 now has **amendments 5 and 6** (posted 03:04/03:05 UTC, after the 02:49 revise). The paper text
already complies with both (below), but the package documents should say 1-6 (m).

## Dispatch focus checks

| Check | Finding |
|---|---|
| **Snapshots** | 112 manifest entries: 17 @581ee7ad, 61 @910593ba, 19 @224332c5, 15 @8acd21ec. All four commits are ancestors of origin/main. **111/112 are byte-identical to origin/main.** All 112 are byte-identical to their pinned `source_commit` blob (with `m1_tables.log` taken from the blob, because it is absent from the worktree). The one origin/main difference is `benchmarks/motor_reference/forward_harmonic/README.md`. Commit 6f4d0111 (#1189) appended a 5-line "Update" bullet after the 910593ba pin, and it repeats the paper's own refinement numbers. This is benign provenance drift, not a defect, but the claim "every snapshot is byte-identical to origin/main" is not literally true (m). |
| **verify.py** | **FAIL at head** (exit 1, missing `m1_tables.log`). **PASS** on a clean `git archive` copy with that one file restored from origin/main (sha `ccc86e29…` matches the manifest). Both PASS blocks print, including "v4 reference reruns, 60-sample reproduction, refined-model gaps, candidate objectives/current rules/angular aliasing, invalid moved mesh and L3 gradient comparisons". The verifier is stdlib-only and does not import the renderer. |
| **#1191 reference refinement** | Independently recomputed from `reference_refinement/runs/*.json` and `reference_data.json` (my own script, not `analyze.py`). The 3×3 rerun against the 60 saved torques gives max \|Δ\| **9.7344e-13 N m** over n = 60 (paper: "within 9.8e-13"; README: 9.7e-13). Cogging std is **0.010711, 0.012203, 0.012123, 0.01208** at 3/6/12/24. GEODE L3 h45 Arkkio is 0.01209: **+12.887% vs saved, +0.102% vs 24×24** (paper: 12.9% → 0.10%). The max pointwise gap is **2.11e-3 → 3.41e-5**. Loaded 24×24 std is 0.03947/0.08014, with GEODE +0.349%/+0.269% (paper 0.35/0.27%). GEODE's L3 bands (Eq. 10, h45 L1-L3) contain all five finest-reference statistics: cogging std [0.01190, 0.01228], half std [0.03899, 0.04023], half mean [0.66149, 0.66263], full std [0.07914, 0.08157], full mean [1.44228, 1.44416]. The tolerance control 1e-6 → 1e-14 gives max 4.40e-8 / 4.28e-8 / 4.25e-8 at 3/6/12 (paper "at most 4.4e-8"). DOFs are 4,079 / 12,518 / 43,220 / 159,920. **All match.** |
| **#1182 validation** | Recomputed from `motor_validation/runs/` (initial = `initial_nurbs_*`, Opt120 = `opt120_export_*`, fixed rule, g20; h45/h93 with T_coupling, conforming with T_arkkio_outer). Opt120 F at L3 is **1.0708e-6 (h45), 3.8635e-6 (h93), 3.9627e-6 (conf)**, so h93/h45 = **3.608** (paper ×3.6). Initial F is 8.171e-3 / 9.481e-3 / 9.686e-3, so h93/h45 = **1.160** (paper +16%). The ratios initial/Opt120 are **7,631 / 2,454 / 2,444**. Opt120 full-current std is 0.00066 / 0.00164 / 0.00165. The L2→L3 \|ΔF\|/F3 maxima are **1.36%** (initial, h45) and **2.71%** (Opt120, h45). On g10 vs g40 at L3 export, Opt120 F is **+1.05% / −5.90% / −17.11%**. The current rules at L3 h45 full current give initial mean 1.4433. Opt120 gives **1.3147 (fixed unscaled, −8.91%), 1.2550 (fixed physical, −13.04%), 1.4983 (group-1 density, +3.81%)**, and the saved reference current gives 1.4986. ScaleR is 1.04772. The moved L3 mesh is `invalid_mesh` with **4 inverted** triangles. **All match.** The three current rules do give materially different results: a 19% spread, which spans the initial mean. Two things the paper omits are raised under S3. Opt120's increments do not shrink (h45 L1→L2 +3.05e-8, L2→L3 +2.90e-8; h93 +9.99e-8, +5.85e-8). The fresh *construction* family gives L2→L3 up to **3.22%** (Opt120 h45), which the changelog excludes on purpose ("broader-source summary maxima are not substituted"). |
| **L3 gradient comparison** | Recomputed from `design/runs/{study_L2,gradient_L3}_{h45,conforming}_{initial,perturbed}.json` with box widths from `optimization_parameter_bounds` (9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm). Box-scaled ‖w⊙(g_h45 − g_conf)‖/‖w⊙g_conf‖ at full current: **mean 0.58/0.66% (L2) → 0.16/0.26% (L3); ripple 6.67/10.24% (L2) → 6.58/10.25% (L3).** The within-model L2→L3 ripple change is h45 0.46/1.19% and conforming 0.49/1.13%, so the 0.46-1.19% range matches. The within-model mean change is conforming 0.54/0.52% (and h45 0.05/0.07%). The SW2 mean components are −141.9 vs −133.3 mN m (6.5%), and SW3 0.406 vs 0.487 mN m (16.5%, shown as 17%). The MAG ripple components at L2 are 1.283/1.937 (initial) and 0.753/1.865 (perturbed). **All match.** At L3 MAG is 1.217/1.867 and **0.569**/1.673. The perturbed h45 component changes by −24% between L2 and L3 while the norm changes by about 1% (S6). |
| **Figures (all six)** | All 17 pages were rendered at 70 dpi and inspected. Fig. 4 (p. 10) was also rendered at 150 dpi. The figures are Fig. 1 (p. 3), Fig. 2 (p. 7), Fig. 3 (p. 8), Fig. 4 (p. 10), Fig. 5 (p. 12) and Fig. 6 (p. 13). None has clipping or `??`. The v3 overlaps are fixed. Findings are under D6. |
| **No speedup / no smoke claim** | Confirmed. grep finds "speedup" only at L736 ("A practical speedup still requires optimization…"). There is no "smoke", "pilot", "faster" or "tenfold". L713-714: "There are no matched optimization results in this paper". L667-669: the cost ratio "provides no measurement of optimizer iterations, surrogate calibration cost or time to a validated target". |
| **Experiment 2 / #1185 am. 1-6** | §7 L697-715 matches the amended protocol. It covers the frozen LENGTH/SD1/ScaleR/OPERATING_ANGLE (am. 2 §1), the 8 vs 8+10 spaces, the fixed physical 9.6777 A, h45/L1 training and h93/L2/g20 validation (original + am. 1), and T50/T90/T99 from refined F0 that are not revised after the pilot (am. 2 §2, am. 3 §3). It also covers candidate-evaluated δF = max(0.10F_val, 2.5e-7), the acceptance rule F_val + δF ≤ T with the 0.25% floor, the 60 × validation-forward wall-time budget with everything charged (am. 3 §1, am. 4 §5), and censoring. The methods are SLSQP / COBYQA / "an empirical radial-basis surrogate" (am. 4, **am. 6 §1 label satisfied**), and "an empirical surrogate alone does not establish an advantage over a reduced physical model" (**am. 6 §4 satisfied**). The paper makes no current-generalization claim for C (am. 6 §2-3 satisfied by silence). The L3/40-angle final checks match the original. The paper does not mention that the conforming spot check is restricted to fresh meshes with zero stator offsets (am. 1); this is acceptable at this level of detail. |
| **Method C wording** | Correct: "an empirical radial-basis surrogate". The paper never calls C a reduced or physical model, and L717 keeps "A cheap waveform or reduced physical model is an essential baseline" as a separate requirement. |
| **Cogging caveat "not fully converged"** | **Present in substance, not in those words, and absent from the abstract and conclusion.** §5.4 L434-443 reads: "This agreement still concerns finite discretizations… extending that trend would put a limit near 0.0120 N m, an estimate without a bound. The half-current increments do not halve… Neither cross-code agreement nor refinement proves physical accuracy". The words "converged"/"convergence" never appear about the reference (grep). The README (`.4/README.md`) does say "It does not certify physical accuracy or full convergence", and the #1191 README says "close to converged, but not exactly". The abstract (L23-24) and the conclusion (L727-729, "identifying coarse reference resolution as its cause") carry no caveat. See S2. |
| **"within 1.3e-9"** | Kept (L270), as intended. verify.py guards `< 1.3e-9`. |
| **Author line** | `\author{Robb Walters}` (L8), pdfauthor set, rendered on p. 1. Correct. |

## Scoring with quoted evidence

**D1 Rigor 5/6.** The new reference experiment is designed well. It reproduces before it refines ("The
$3\times3$ rerun first reproduces all 60 saved torques to within $9.8\times10^{-13}$\,N\,m" — §5.4). It
changes only the subdivision, and it includes a Newton-tolerance control. The Opt120 validation handles
the estimator domain correctly ("Its inner air-gap band no longer has the annular area assumed by the
full-band Arkkio functional, so harmonic results here use coupling torque" — §5.5) and also says what
that does not validate. The §6.3 yardstick is now the correct one ("the ripple gap persists beyond the
measured refinement increments" — §6.3). There are three deductions. (i) The headline 0.10% is presented
without the larger refinement uncertainty around it (S2). (ii) "these increments are not error bounds"
(§5.5) leaves out that Opt120's increments are not decreasing, so L3 is not shown to be asymptotic for
the candidate (S3). (iii) The conclusion's current-policy sentence draws a contrast that the data do not
support (S1). The cross-model gradient confound remains, but it is stated correctly.

**D2 Evidence 6/6.** The forward comparison now covers the initial and saved-optimized designs. The cogging
gap was investigated with an independent rerun of the reference code at four resolutions. The
sensitivity gap is shown at L3. Every claim in the abstract and conclusion has an experiment behind it,
and I recomputed each new number from the merged raw records. The missing optimization comparison is
scoped out of the claims explicitly ("There are no matched optimization results in this paper" — §7),
so it is not deducted.

**D3 Contribution 4/5.** The lede is result-first ("A motor optimization target can change with the
numerical interface even when average torque agrees closely" — Abstract). The Opt120 result strengthens
the thesis a great deal: interface order changes a *design improvement*, not just a waveform. There are
two deductions. The abstract's new Opt120 sentence ("increasing interface order from 45 to 93 multiplies
the aggregate squared ripple by 3.6 under fixed unscaled current" — Abstract) gives no denominator, so a
cold reader may read it as the optimum being invalidated. The body says the opposite ("The large ripple
reduction survives these checks" — §5.5) (S4). And the conclusion's "also removes its mean-torque
advantage" misstates the policy result (S1). The title is unchanged and acceptable.

**D4 Related work 5/5.** The v3 gap is closed with a checked, specific statement: "Their linear-material
cogging experiment varies the maximum transmitted order from 39 to 129 and compares against a refined
order-200 reference. Thus torque sensitivity to interface order is already established; our study
quantifies its effect on loaded nonlinear ripple, a saved optimized design, and design sensitivities"
(§2). Merkel is distinguished by objective and validation, and Bontinck by stability. The positioning
uses the labelled-ingredients move the rubric rewards, without novelty inflation.

**D5 Reproducibility 4/5.** In content this is excellent. There are 112 SHA-pinned inputs across four
commits, all ancestors of main. The reference rerun's environment is pinned ("Octave 11.3.0, GeoPDEs
revision \texttt{53ae5ca1} and nurbs 1.4.3; compatibility edits, source hashes and raw records are
retained" — §5.4). There are commit-pinned links for every evidence family. The stdlib verifier
reconstructs the new statistics from raw arrays. The deduction is that the package as committed does
not verify: one manifest snapshot is missing because of `*.log` in `papers/.gitignore`, so a reader
cloning the repo gets a failing `verify.py`. That is the critical flag, and it also costs a point here.

**D6 Figures/tables 3/4.** Fig. 4 is a good addition. Its four panels each carry one claim, the units are
stated (N² m², mN m), panel (c) is on a log axis with a ×3.6 annotation, and the caption names the
estimator for every series ("Harmonic values use coupling torque; conforming values use outer-band
Arkkio" — Fig. 4). There are four deductions. (i) Table 1, Fig. 2 and Fig. 3(a) still show "deviation
from the saved reference" (the cogging line at +13%), with no caption pointer to the reference that §5.4
shows is under-resolved by about 11% (S5). (ii) Fig. 4(a) shows GEODE L3 as a bare dashed line without
its ±1.6% band, so the 0.10% looks more exact than it is (S2/m). (iii) Fig. 6 is L2-only, while the text
now rests on the L3 comparison (S6). (iv) In Fig. 4(d) the "Initial" label sits on the dashed line next
to the 1.3147 bar label (cosmetic, nit).

**D7 Prose 3/4.** The voice rules hold. A grep finds no virtue words, no "load-bearing" or "certified", no
novelty claims, no issue/PR numbers, no Squad/Judge/tracker words, and no `---`. "earlier" appears only
once, at L432 ("the earlier cogging mismatch"), and it refers back to §5.1, not to version history. There
are three deductions. GEODE is never introduced as the authors' implementation (first use: "its cogging
discrepancy with GEODE" — Abstract). "L1" appears in the abstract undefined ("at the initial L1, order-45,
full-current state"). And the S1 sentence is imprecise.

**D8 Citation hygiene 5/5.** There are 13 keys, all resolving, and 13 `\bibitem`s. The bibliography renders
completely (p. 16-17). The only bib change is the ICEM title case. The new positioning claim about
Egger et al. was spot-checked against the primary PDF and is supported. Methods are cited where they are
used.

**D9 Economy 3/4.** The abstract has grown to about 250 words with eight separate findings. The Opt120 and
reference-refinement sentences push the gradient result into the second half. §5.5 packs four
experiments into three paragraphs, and §7's protocol paragraph reads like a preregistration summary. It
is accurate, but it could be a 6-row table. The "X, not Y" closers remain frequent ("these increments are
not error bounds", "These margins are proposed acceptance allowances, not proven numerical error bounds",
"It therefore provides a validation case, not a feasible solution"). Each is correct; together they slow
the reader.

## Critical-flag check

- **artifact_verify_0: SET** (see Verdict). The declared `python3 verify.py` exits 1 in a clean checkout of
  the PR head. The declaration itself is well formed (the cwd exists and the command launches), so this is a
  flag, not a fail-open major.
- Citation error: none. All keys resolve, and the new Egger statement is supported by the primary source.
- Numerical inconsistency: none. Every new v4 number I recomputed matches the raw records (tables above).
  S1 is a wrong *interpretive* contrast, not a wrong number.
- Missing experiment for a claim: none. There is no optimizer claim, no speedup claim and no smoke-run claim.
- Build: preflight reports compile ok, 17 pages, 0 overfull boxes and bib_lint ok. The PDF SHA
  `c7b76293…` matches main.pdf at the head. There is no `??` in the rendered text.
- Close prior work ignored: none.
- Audience: no operator- or team-addressed sentences in the body.

## Blockers

- **B1.** Commit `figures/src/data/m1_tables.log` (`git add -f`). Re-run verify.py from a fresh clone or a
  `git archive` export. (The memory note on gitignored logs describes this exact failure mode.)

## Should-fix (main.tex line refs)

- **S1 (L730-731, conclusion; also BRIEF "Strongest claim").** "fixing physical current also removes its
  mean-torque advantage" implies that Opt120 has an advantage under the fixed unscaled current used for the
  ×3.6 result. It does not: 1.3147 vs 1.4433 N m (−8.9%). Opt120 beats the initial mean only under the
  reference's group-1 density rule (1.4983, +3.8%), and it falls further under fixed physical current
  (−13.0%). Suggested wording: "its mean-torque gain over the initial design appears only under the
  reference's group-1 density rule; at fixed unscaled or physical current it is 8.9–13% lower."
- **S2 (L23-24 abstract; L727-729 conclusion; L425 §5.4; Fig. 4(a)).** Keep the "not fully converged"
  caveat wherever 0.10% appears. Three facts, all from the pinned records, put the 0.10% in context:
  the reference's increments (−0.00008, −0.00004) extrapolate to about 0.01204 N m, about 0.3% below its
  24×24 value; GEODE's own L3 cogging interval is [0.01190, 0.01228] (±1.6%); and at 12×12 the gap was
  −0.26%. The 0.10% is therefore a point agreement inside larger refinement uncertainty, and it is not an
  accuracy. Suggested abstract clause: "…from 12.9% to 0.10%, within both models' remaining refinement
  changes; neither is fully converged." Replace "identifying coarse reference resolution as its cause"
  with "attributing it to coarse reference resolution". Draw the GEODE band in Fig. 4(a).
- **S3 (L465-467, §5.5).** Add that Opt120's F increments do not shrink (h45: +3.05e-8 L1→L2, +2.90e-8
  L2→L3; h93: +9.99e-8, +5.85e-8), so L3 is not shown to be in the asymptotic range for the candidate.
  This is also the stated basis for the 10% δF in §7. Restricting the reported maximum to the export
  family is defensible, but one clause should say that the fresh construction family reaches 3.2%
  (Opt120 h45), so the reader knows a second family exists.
- **S4 (L25-26, abstract).** Give the ×3.6 its context: "…by 3.6 under fixed unscaled current, while the
  initial design's rises 16%. The ripple reduction remains above 2,400-fold." Without this, the abstract
  can be read as the opposite of §5.5's conclusion.
- **S5 (Table 1 caption L324-326; Fig. 2 caption L339-344; Fig. 3 caption L369-374).** These displays measure
  deviation from the saved 3×3 reference. Add one caption clause: "The saved reference under-resolves
  cogging; Section 5.4 and Fig. 4(a) compare with its refined reruns." Otherwise a reader who sees the
  +12.9% cell, or the +13% line in Fig. 3(a), takes it as a GEODE error.
- **S6 (L617-621; Fig. 6).** "These differences persist under mesh refinement" is true for the norm and for
  MAG at the initial design (1.217 vs 1.867 at L3). But the quoted perturbed value 0.75 becomes 0.57 at
  L3 (−24%), while the ripple norm moves only 1.2%. Quote the L3 MAG values (1.22/1.87 and 0.57/1.67),
  or add the L3 markers to Fig. 6. The figure is L2-only, but the argument now rests on L3.

## Minor (m) and nits

- **m1.** The changelog #1185 row, BRIEF L127 and EXPERIMENTS L9 say "amendments 1–4". Amendments 5
  (validation trigger, method C edge cases, hang recording) and 6 (empirical-RBF label, "new-design /
  validation-model checks") exist. The paper already complies; update the package documents.
- **m2.** Snapshot `figures/src/data/harmonic/README.md` (source
  `benchmarks/motor_reference/forward_harmonic/README.md`) matches its 910593ba pin but not origin/main,
  because #1189 appended an update note afterward. This is not a defect. If the provenance statement
  (preflight `source_provenance`) is meant to cover origin/main, scope it to "pinned source commits".
- **m3 (L24, L211).** Introduce GEODE at first use ("our finite-element implementation, GEODE").
- **m4 (L32-33).** "L1" is undefined in the abstract. Use "the second-coarsest mesh" or drop the level.
- **m5 (L697-698).** "outer diameter" is SD1 in Table 4, but the §6.1 parameter list calls SD1 a
  "geometric length" without a gloss. Add "(SD1)" at L697.
- **m6 (L417).** "within 9.8e-13" against raw 9.73e-13 follows the same ceiling convention as 1.3e-9. That
  is fine, but the #1191 README states 9.7e-13, so an auditor will ask about it. "at most 9.8e-13" reads as
  a bound more clearly.
- **m7 (Fig. 4(d)).** Move the "Initial" label off the dashed line, away from the 1.3147 label.
- **m8 (L26).** The source line runs on ("…fixed unscaled current. We differentiate…"). This has no
  render impact.

## Procedural notes

- Read: main.tex (791 lines) and all `tables/*.tex`, changelog.md, README.md, preflight.json,
  _progress.json, citation-notes.md, verify.py (partly), verified-numbers.json (partly), ../BRIEF.md,
  ../EXPERIMENTS.md (grep), the v3 Opus and Fable reviews, issue #1185 with all 8 comments, and the #1191
  and #1182 benchmark READMEs.
- Independent scripts (scratchpad `v4rev/`): `hashes.py` (three-way SHA check), `refine.py` (#1191),
  `valid.py` (#1182), `grad.py` (L2/L3 gradients). The evidence was extracted with `git archive origin/main`.
  None of these scripts imports verify.py, analyze.py or the renderer.
- One literature request was made to arxiv.org, anonymously (generic browser User-Agent, no email, no
  mailto, no identity headers). The download was placed in its own empty directory and read only with
  pdftotext.
- The worktree `/Volumes/Stripe/geode-fem/review-1171v4` is unmodified (`git status` empty, HEAD fea6ba5b).
  No `.4.review/` sidecar was staged.

## Summary

v4 scores **38/44 but does not advance**, because of one packaging critical flag. The manifest pins
`m1_tables.log`, but `papers/.gitignore` kept it out of the commit, so `verify.py` fails in any clean
checkout. A one-line `git add -f` clears it, and with that file restored the verifier passes. The
science is stronger than in v3. I reproduced every new number from the merged raw records with my own
scripts: the 3×3 reproduction (9.73e-13 over 60 torques), the cogging 12.89% → 0.10%, all five refined
statistics inside GEODE's bands, Opt120's ×3.608 with ratios 7,631/2,454/2,444, the three current rules
(−8.9%, −13.0%, +3.8%), the g10 aliasing, and the L3 gradient gaps (6.58/10.25% ripple, 0.16/0.26%
mean). All v3 review comments are addressed or declined with sound reasons. The paper makes no speedup
or smoke-run claim. Experiment 2 is described consistently with #1185 through amendment 6, with method
C labelled as an empirical surrogate. The remaining should-fixes are precision edits: the conclusion's
current-policy contrast (S1), the convergence caveat on the 0.10% headline (S2), the Opt120
non-shrinking increments (S3) and abstract context (S4), and captions that still measure against the
saved reference (S5).
