# paper-review: motor-gradient-optimization.5 (PR #1171, head 76d78e1f)

Reviewer: Opus (anvil:paper-review, read-only). This is a fresh score of the v5 manuscript at head
`76d78e1f373b20d0c9c7920582b5a0ddd71082c2`. The v4 score (38/44) does not carry over; the v4 review and
the combined v4 list were used only to check the changelog item by item. I modified nothing in
`/Volumes/Stripe/geode-fem/review-1171v4`: `git status` was empty before and after, and HEAD is unchanged.
verify.py and the hash checks ran on a `git archive` copy in my scratchpad. No `.5.review/` sidecar was
staged, per dispatch. This file is the full review.

Rubric: the installed `/Users/rwalters/GitHub/geode-fem/.anvil/skills/paper/rubric.md`, 9 dimensions,
**/44, advance threshold ≥35**. House rules: `.anvil/skills/paper/voice.md`. origin/main = `89e6c4e1`
(it matches `ls-remote`).

## Verdict

- **Total: 42 / 44**
- **advance: true.** The score clears the threshold, and no critical flag is set.
- **Critical flags: none.** The v4 blocker (`artifact_verify_0`, missing `m1_tables.log`) is cleared.
  The log is now tracked through a narrow `!figures/src/data/m1_tables.log` exception in
  `.5/.gitignore`. `python3 -I -B verify.py`, run in the `.5` directory of a clean
  `git archive 76d78e1f papers` extract, **exits 0** and prints all three PASS blocks, including the new
  "v5 size/current normalization, matched-policy reference ratios, candidate refinement increments,
  paired estimators, L3 MAG components, and 3240 candidate solve statuses".
- **Outstanding dependencies: none.** There are no `[PENDING]` markers. Experiment 2 is described as a
  protocol with no results.

| # | Dimension | v5 score |
|---|---|---|
| 1 | Rigor of method / argument | 6 / 6 |
| 2 | Evidence sufficiency | 6 / 6 |
| 3 | Clarity of contribution | 5 / 5 |
| 4 | Related-work positioning | 5 / 5 |
| 5 | Reproducibility | 5 / 5 |
| 6 | Figure & table quality | 4 / 4 |
| 7 | Prose & structural quality | 3 / 4 |
| 8 | Citation hygiene | 5 / 5 |
| 9 | Rhetorical economy | 3 / 4 |
| | **Total** | **42 / 44** |

Top priorities. None of these blocks; they are precision edits for the next pass or the audit.

1. **§6.3 "the vector norm changes by about 1%" (L673-674) is not what the data show** (m1). The h45
   perturbed ripple-vector *norm* changes by only 0.19% from L2 to L3. The ~1% figure (1.19%) is
   ‖g₃ − g₂‖/‖g₂‖, the relative *difference* vector already quoted at L682. Suggested wording: "even
   though the box-scaled ripple vector changes by only 1.2% from L2 to L3". This also states the 24%
   MAG move (0.753 → 0.569) more sharply.
2. **§7 "All costs count toward a budget of 60 measured validation-forward wall times" (L757-758)
   overstates the protocol** (m2). Amendment 4 ruling 1 leaves the θ0 training constants uncharged,
   and amendment 6 §3 puts the post-run held-out-current diagnostic in `post_run_diagnostic`, outside
   the budget. Suggested wording: "All in-run costs, including calibration, remeshing and acceptance
   validations, count toward…".
3. **De Gersem & Weiland verb (L94-95)** (m3). "Harmonic weighting of a sliding interface was developed
   for finite-element machine models by" reads as an origination claim, slightly beyond the title-level
   support that citation-notes.md declares (full text not retrieved). A title-level verb removes the
   question: "\citet{degersem2004harmonic} formulate harmonic weighting functions at the sliding
   interface of a finite-element machine model with angular displacement."
4. **Point to the new Opt120 waveform supplement from the paper** (m4). `figures/opt120-waveforms.pdf`
   is tracked and rendered, but `main.tex` never mentions it (grep finds no "supplement" or
   "opt120-waveforms"). Only the package README describes it. One clause in §5.5 or the Fig. 4 caption
   ("40-angle waveforms are in the accompanying `opt120-waveforms` supplement") would make it reachable.
5. **Abstract "We differentiate the converged nonlinear model" (L29)** (m5). It sits one sentence after
   "neither sequence is fully converged", where "converged" means mesh refinement. Here it means Newton
   convergence, so the reader meets two senses of the word in adjacent sentences. Use "the solved
   nonlinear model" or "the Newton-converged nonlinear model".

## Resolution of v4 review items

The changelog was checked against both v4 reviews and the combined v4 list.

| v4 item | v5 status | Evidence |
|---|---|---|
| Critical B1 / C1: `m1_tables.log` not committed | **Resolved.** Tracked (`git ls-files`), narrow ignore exception, clean-archive verify exits 0. preflight records `artifact_verify_source: "clean Git archive of staged artifact files"`. | Ran |
| Opus S1 / Fable M1: conclusion's "removes its mean-torque advantage" | **Resolved.** Conclusion L778-781: "Its shorter stack and changed radial scale must be separated from current policy… at its saved size it exceeds the initial mean only under the density rule among the three policies tested." This is correct (−8.91% / −13.04% / +3.81%). | Recomputed |
| Fable M1 normalization wording ("per unit length") | **Resolved.** L505-507: "Removing both size factors… gives 1.4971 N m, 3.7% above the initial mean; normalizing only length to 100 mm while retaining the actual radial scale gives 1.6434 N m". "per unit length" does not appear anywhere (grep). Both labelled "algebraic normalizations… not reoptimized designs". | Recomputed |
| Fable M2: saved Opt120 comparison missing | **Resolved.** L512-523 give 1.4986 vs 1.5000, 3.53e-4 vs 1.76e-4 (×2.0), half 5.53e-4 vs 4.37e-4, pointwise ~2.1e-3, and "Reference refinement was run only at the initial design". | Recomputed |
| Fable M2 ratio mixing current policies (7,631 vs 11,982) | **Resolved.** L519-522: "With this matched current policy… 10,170 for GEODE and 11,982 for the saved reference. The 7,631 ratio above instead holds unscaled current fixed." L472-477 label 7,631/2,454/2,444 as fixed-unscaled-current. Each ratio carries its rule. | Recomputed |
| Opus S3 / Fable M3: Opt120 increments do not shrink; second family | **Resolved.** L483-488 state 3.05e-8 / 2.90e-8 ("barely shrink and do not establish an asymptotic rate"), the alternate family's 3.22%, and the ×96 interface effect. | Recomputed |
| Opus S2 / Fable m2: caveat next to 12.9% → 0.10% | **Resolved.** Abstract L25: "although neither sequence is fully converged". Conclusion L774-775: "although neither sequence is fully converged and the remaining error is not bounded". §5.4 L445-448 leave the half-current remainder unestimated, since its increments do not shrink. GEODE band drawn in Fig. 4(a). | PDF p. 11 |
| Opus S4: ×3.6 context in abstract | **Resolved.** Abstract L26-29: "…while the initial objective rises only 16%; the reduction remains above 2,400-fold across the tested models." | |
| Opus S5: captions measure against the coarse saved reference | **Resolved.** Table 1 ("The reference is subsequently refined in Section 5.4"), Fig. 2 ("The saved reference is coarse; Section 5.4 reports its refinement"), Fig. 3 ("see Section 5.4 for its refinement"). | PDF p. 6-8 |
| Opus S6: L3 MAG values | **Resolved** (wording nit m1). L671-672: "1.22 versus 1.87… 0.57 versus 1.67". The Fig. 6 caption points to the text for L3. | Recomputed |
| Fable: paired-estimator check | **Resolved.** L464-467: 1.3e-4 mean, 1.1e-5 std on the same L3 h45 Opt120 field, with a geometry-specific limit (L468-469). | Recomputed |
| Fable: mechanism as conjecture | **Resolved.** L489-492, "This is a conjecture, not an ablation result". | |
| Fable lead: De Gersem & Weiland 2004 | **Added** (verb nit m3). DOI and metadata confirmed by an anonymous Crossref request. | Fetched |
| Fable / Opus: Fig. 4(c) lines, Fig. 4(d) label, Opt120 waveform | **Resolved.** Markers only in 4(c). The "Dashed: initial (100 mm)" label is clear of the bar labels in 4(d). The supplement exists but is unreferenced (m4). | PDF p. 11, supplement |
| Fable / Opus D9: §7 protocol length | **Resolved.** One paragraph (L751-761, about 11 lines). | |
| Opus m1 / Fable: amendments 1-4 → 1-6 | **Resolved.** EXPERIMENTS L9 and the v5 README say "amendments 1–6 and the amendment-6 addendum". BRIEF L127 is truncated in my grep, but the changelog row covers it. | grep |
| Opus m2: "identical to origin/main" | **Resolved.** README and preflight now say "pinned source commits / blobs" and name the harmonic README's later update. | |
| Opus m3/m4/m5: GEODE, L1, SD1 | **Resolved.** "our GEODE implementation" (L18-19), "second-coarsest mesh" (L35), "outer diameter (SD1)" (L751). | |
| Opus m6/m8: 9.8e-13 wording, long source line | **Resolved.** "differences at most 9.8e-13" (L425). The abstract source lines are split. | |
| Fable m9/m10: toolchain, candidate solve statuses | **Resolved.** L809-810 (benchmark READMEs record Gmsh/Rust). L531-532: 3,240 rows / 18 records at tolerance. | Recomputed |
| Fable m8: list of derivative caveats | **Declined with a sound reason** (the three failure modes are distinct). | |

## Dispatch focus checks

| Check | Finding |
|---|---|
| **Clean archive + verify.py** | `git -C …/review-1171v4 archive 76d78e1f papers \| tar -x` into `scratchpad/motor-v5-opus/archive/`, then `python3 -I -B verify.py` in `.5/`: **exit 0**, three PASS lines. |
| **Manifest SHAs** | My own script (`scripts/hashes.py`, no import of verify.py). **115 entries**: 17 @581ee7ad, 61 @910593ba, 15 @8acd21ec, 22 @224332c5. All four are ancestors of origin/main. **115/115** archive files match the manifest SHA, and **115/115** match the blob at their pinned `source_commit`. 114/115 also equal origin/main. The exception is `benchmarks/motor_reference/forward_harmonic/README.md`, the later #1189 note, which the README now discloses. preflight `pdf_sha256` f332a188… = `main.pdf` at head. |
| **M1 normalization** | From `opt120_export_L3_h45_fixed.json` (T_coupling, g20, full current): mean **1.31470**. torque_factor = ScaleR²·LENGTH/0.1 = **0.878175** (it matches the record). 1.3147/0.878175 = **1.4971** (+3.73% vs initial 1.44326). Length only: 1.3147·0.1/0.0800005 = **1.6434**. The paper's wording ("Removing both size factors"; "normalizing only length to 100 mm while retaining the actual radial scale") is correct, and "per unit length" does not appear. |
| **M2 matched-current ratios** | GEODE initial F (h45, L3, g20, T_coupling) = 8.1714e-3. Opt120 under the saved reference current (`opt120_export_L3_h45_reference`, currents 0.5/1, zero current from the fixed record) gives F = 8.0351e-7. The ratio is **10,169.6 → 10,170**. Saved arrays: initial F 7.9282e-3, Opt120 6.6170e-7, ratio **11,981.6 → 11,982**. Fixed unscaled: **7,631.5 / 2,454.0 / 2,444.4**. Labels: 7,631 is tied to "evaluation current held fixed in the unscaled geometry" (L472-477) and to "holds unscaled current fixed" (L521-522); 10,170/11,982 to "this matched current policy" (L519-520). Correct. The other matched-policy numbers: mean 1.4986 vs saved 1.50001; std 3.528e-4 vs 1.759e-4 (×2.01); half std 5.530e-4 vs 4.374e-4; max loaded pointwise 2.058e-3 ("about 2.1e-3"). **All match.** |
| **Alternate family 3.22%, increments** | Opt120 h45 fresh export F at L0-L3: 9.5545e-7, 1.0112e-6, 1.0418e-6, 1.0708e-6. Increments **+5.58e-8, +3.05e-8, +2.90e-8** (L2→L3 2.71% of F₃). Fresh construction family h45: L2→L3 **+3.455e-8 = 3.22%** of F₃. Interface effect 3.8635e-6 − 1.0708e-6 = **2.793e-6**, /2.900e-8 = **96.3** (paper "about 96"). h93/h45 = **3.608**; initial +**16.0%**. Initial export maximum 1.36% matches (the initial *construction* family reaches 1.40%; nit, not quoted). **All match.** |
| **Paired estimators** | Same L3 h45 Opt120 field, T_arkkio_outer vs T_coupling, g20. Full current: |Δmean| **1.32e-4**, |Δstd| **1.09e-5**. Half: 1.01e-4 / 2.2e-8. Zero: 6.1e-5 / 6.6e-7. The paper quotes full current ("about 1.3e-4… 1.1e-5"). **Match.** |
| **L3 MAG values** | From `design/runs/gradient_L3_*` and `study_L2_*`, d_std[0]: initial h45/conf **1.217 / 1.867**, perturbed **0.569 / 1.673** (L2: 1.283/1.937, 0.753/1.865). L3 box-scaled ripple gaps **6.58 / 10.25%**, mean gaps **0.16 / 0.26%**. Within-model ripple change 0.46/0.49% (initial), 1.19/1.13% (perturbed). Within-model conforming mean change 0.54/0.52%. **All match.** Wording issue m1 (the "vector norm" itself changes by 0.19%). |
| **Other spot checks** | 3,240 rows: the 18 records (6 initial L2/L3, 6 Opt120 export L2/L3, export L1 h45, construction L2/L3 h45, and reference / fixed-physical / jinit L3 h45) sum to exactly 3,240 rows, all `tolerance`. No non-tolerance row in any record in `motor_validation/runs`. Current rules: −8.91% / −13.04% / +3.81%. g10 vs g40: +1.05% / −5.90% / −17.11%. Opt120 full std 0.00066 / 0.00164 / 0.00165, cogging 0.00061 → 0.00035. Cogging headline from raw: saved 0.01071, 24×24 0.01208, GEODE 0.01209, giving **+12.89% → +0.102%**. |
| **De Gersem & Weiland 2004** | Anonymous Crossref request (generic User-Agent, no mailto, no identity): title "Harmonic Weighting Functions at the Sliding Interface of a Finite-Element Machine Model Incorporating Angular Displacement", IEEE Trans. Magn. 40(2):545-548, 2004. It matches refs.bib and the rendered bib (p. 17). The single use (L94-95) makes no performance, implementation or "first" claim. Its verb "was developed… by" is mildly stronger than the title supports; see m3. Not a citation error. |
| **Figures** | All 17 pages rendered at 70 dpi and read; p. 11 (Fig. 4) also at 150 dpi; `figures/opt120-waveforms.pdf` at 110 dpi. No clipping, no `??` (pdftotext count 0), no overlapping labels. Findings under D6. |
| **No speedup / smoke** | Confirmed. "speedup" appears only at L786 ("A practical speedup still requires optimization…"). There is no "smoke", "pilot" or "faster". L759: "There are no matched optimization results in this paper". L721-723: the cost ratio "provides no measurement of optimizer iterations, surrogate calibration cost or time to a validated target". |
| **Experiment 2 vs #1185 am. 1-6 + addendum** | Read all 9 issue comments. L751-761 match: frozen SD1/LENGTH/ScaleR/OPERATING_ANGLE and physical peak current (am. 2 §1); 8 vs 8+10 spaces (am. 2 §1); exact-gradient / derivative-free / "empirical radial-basis-surrogate" methods (am. 4 §1, **am. 6 §1 label**); predeclared 50/90/99% targets (am. 2 §2, am. 3 §3); common order-93 / L2 / 20-angle validation with numerical and mean-torque margins (original + am. 2 §2); final L3, 40-angle checks (original); 60× measured validation-forward wall-time budget (am. 3 §1, am. 4 add. 6); censoring (am. 2 §2); "an empirical surrogate alone cannot establish an advantage over a reduced physical model" (**am. 6 §4**). There is no current-generalization claim for C (am. 6 §2-3 and addendum satisfied by silence). C is never called a reduced or physical model, and L763 keeps "A cheap waveform or reduced physical model is an essential baseline" as a separate requirement. One overstatement: "All costs count" (m2). |
| **"within 1.3e-9"** | Kept. L274-275: "within 4.1e-9 N m on L0 and 1.3e-9 N m on L1". verify.py L53 asserts `(1,1.3e-9)`. The abstract says "within a few 10^-9 N m". |
| **Author** | `\author{Robb Walters}` (L8), pdfinfo Author = "Robb Walters", rendered on p. 1. Sole author. |

## Scoring with quoted evidence

**D1 Rigor 6/6.** All three v4 deductions are fixed with the right qualifiers. The 0.10% is now "a
comparison of finite-resolution values, not an accuracy bound; neither refinement sequence is fully
converged" (§5.4). The candidate's refinement state is reported plainly: the increments "barely shrink
and do not establish an asymptotic rate. An alternate freshly constructed mesh family changes by 3.22%
from L2 to L3" (§5.5). The size and current-policy confound is separated: "These policy comparisons use
the same Opt120 geometry, but comparisons with the initial motor also change size… These algebraic
normalizations separate the size factors from the current rule; they are not reoptimized designs"
(§5.5). The matched-current reproduction is kept apart from the fixed-current ratio, and the unexplained
2× ripple discrepancy at Opt120 is left open ("the cause of the Opt120 discrepancy remains unresolved").
The paired-estimator check is reported with its domain limit. The remaining cross-model gradient confound
is stated correctly ("it does not isolate an interface-only derivative error"). Nothing in the method
fails to support the claims made.

**D2 Evidence 6/6.** The v5 additions cover every v4 evidence gap: saved-array comparison under the
matched current, L1 export and alternate-family records, the paired estimator, the L3 MAG components,
and 3,240 solve statuses. I recomputed each new number from raw origin/main blobs with my own scripts.
The missing optimizer comparison is scoped out of the claims ("There are no matched optimization
results in this paper"), so it is not deducted.

**D3 Contribution 5/5.** The lede is result-first ("A motor optimization target can change with the
numerical interface even when average torque agrees closely"). The abstract now gives the ×3.6 its
denominator ("while the initial objective rises only 16%; the reduction remains above 2,400-fold across
the tested models"), so it can no longer be read as invalidating the optimum. The contribution is
stated with its boundary: "The numerical methods are established; the results supply the evidence
needed before comparing optimization costs" (§1). The conclusion's policy sentence is now accurate.
This is neither overclaiming nor a buried contribution.

**D4 Related work 5/5.** The Egger et al. statement remains specific and checked ("varies the maximum
transmitted order from 39 to 129… Thus torque sensitivity to interface order is already established").
The new De Gersem & Weiland sentence credits the harmonic-weighting method. Merkel, Bontinck, Ion and
the reduced-model works are each placed by objective or method, with no novelty inflation.

**D5 Reproducibility 5/5.** The package now verifies from a clean checkout, which v4 did not. It has
115 SHA-pinned inputs across four ancestor commits, byte-identical to their pins, and a stdlib verifier
that reconstructs the new v5 statistics from raw arrays. The rebuild commands pin versions
(`numpy==2.5.4`, `matplotlib==3.11.2`, `meshio==5.3.5`). The reference environment is pinned (Octave
11.3.0, GeoPDEs 53ae5ca1, nurbs 1.4.3), and the GEODE toolchain points to the benchmark READMEs (§Code
and data). The unreferenced supplement (m4) is a reader-navigation issue, not a reproducibility gap,
because the README documents it.

**D6 Figures/tables 4/4.** The v4 deductions are fixed. The Table 1, Fig. 2 and Fig. 3 captions now
point to §5.4. Fig. 4(a) draws GEODE's L3 band, so the 0.10% is visibly a point inside a ±1.6% interval.
Fig. 4(c) uses markers for categorical models. Fig. 4(d) has its label clear of the bars, and its
caption states "at its saved size… the dashed line is the initial mean at the initial size". Table 4
adds the Opt120 column with wrapped descriptions. Captions name the estimator and current rule for
every candidate series. Three nits do not cost a point. (i) Fig. 6 remains L2-only, with an explicit
caption pointer ("L3 MAG values and refinement changes are given in the text"); acceptable. (ii) In
Fig. 3(a), the harmonic zero-current squares sit near +12% among the dashed half- and full-current
conforming lines; legible, but crowded. (iii) The supplement (m4).

**D7 Prose 3/4.** The voice rules hold. A grep finds no virtue words, no "load-bearing", "certified",
"first" (as novelty) or "novel", no issue/PR numbers, no team words and no `---`. "earlier" and
"version" appear only in non-history senses. There are three precision slips in otherwise exact prose.
(i) "even though the vector norm changes by about 1%" (§6.3) misnames the quantity (m1). (ii) "All costs
count toward a budget" (§7) overstates the protocol (m2). (iii) "the converged nonlinear model"
immediately follows "neither sequence is fully converged" (Abstract) (m5). Each is small, but each is a
statement a careful reader can catch as inexact.

**D8 Citation hygiene 5/5.** There are 14 keys, all resolving, with 14 rendered entries complete with
author, title, venue, year and DOI (p. 16-17). The new De Gersem & Weiland entry matches Crossref
exactly (volume 40, issue 2, pages 545-548, 2004), and its author spacing is corrected from Crossref's
"DeGersem". The resolution record and its claim-support limitation are kept in
`citation-resolution-degersem2004.json` and `citation-notes.md`. The verb choice at L94-95 is mildly
stronger than the declared title-level support (m3), but the cited paper is about exactly that method,
so it is not a citation error.

**D9 Economy 3/4.** §7 is now one paragraph, which addresses the v4 complaint. Two things still slow a
busy reader. The abstract is 244 words with eight findings, and the gradient result begins in its second
half. §5.5 has grown to five dense paragraphs: estimator restriction, ×3.6 plus increments plus
conjecture, current policy plus size normalization, matched-current reproduction, and angular sampling.
They hold about 40 numbers. A 3×4 table (rule × {mean, Δ vs initial, size-normalized, F ratio}) would
carry the policy/size/ratio content in less space and make the ratio labelling explicit at a glance.
The "X, not Y" closers remain frequent ("an estimate without a bound", "not an accuracy bound", "not
error bounds", "not reoptimized designs", "not an ablation result"). Each is correct; together they slow
the reader.

## Critical-flag check

- **artifact_verify_0: cleared.** The clean-archive verify exits 0, and all 115 hashes match the archive
  and the pinned blobs.
- Citation error: none. All keys resolve. The new citation's metadata is confirmed; its use is
  title-level in substance (m3 is wording).
- Numerical inconsistency: none. Every v5 number I recomputed matches the raw records. The preflight's
  one numeric-consistency warning (L437 "0.27% higher" paired with the 0.01071 → 0.01220 window) is a
  heuristic mispairing. 0.27% is GEODE full-current std vs the finest reference (0.08036 vs 0.08014),
  which I confirmed in v4 from the same pinned blobs.
- Missing experiment for a claim: none. There is no speedup, smoke-run or optimizer claim.
- Build: preflight reports compile ok, 17 pages, 0 overfull boxes and bib_lint ok. The PDF SHA matches.
  There is no `??`.
- Close prior work ignored: none.
- Audience: the preflight's audience check has 0 hits. I found no operator- or team-addressed sentences
  in the body.

## Blockers

None.

## Should-fix

None at the should-fix level. Every v4 should-fix is resolved.

## Minor (m) and nits

- **m1 (L673-674).** "even though the vector norm changes by about 1%". The h45 perturbed ripple norm
  changes by 0.19% from L2 to L3. The 1.19% is ‖Δg‖/‖g₂‖. Reword as in Top priority 1.
- **m2 (L757-758).** "All costs count toward a budget". Amendment 4 ruling 1 (θ0 training constants
  uncharged) and amendment 6 §3 (post-run held-out-current diagnostic excluded) are exceptions. Use
  "All in-run costs…".
- **m3 (L94-95).** "was developed for finite-element machine models by" is an origination verb. Use a
  title-level verb ("formulate"/"present"), consistent with citation-notes.md ("no stronger priority…
  claim").
- **m4 (§5.5 or Fig. 4 caption).** Reference `figures/opt120-waveforms.pdf` from the manuscript; it is
  currently reachable only through the package README.
- **m5 (L29).** Use "solved" or "Newton-converged" for the differentiated model, to avoid a clash with
  the refinement sense of "converged" one sentence earlier.
- **nit (L480-482).** "reach 1.36% for the initial design" is the export family's maximum. The initial
  *construction* family reaches 1.40%. Not material; the alternate-family clause could say "3.22% for
  Opt120 (1.40% for the initial design)".
- **nit (L464-467).** The paired std difference (1.1e-5) is 1.7% of Opt120's full-current std. Stating
  that relative size would show the reader why the substitution is adequate against a ×3.6 effect.
- **nit (Fig. 3(a)).** The harmonic zero-current trace runs through the dashed conforming half- and
  full-current traces near +12%. Consider a lighter dash or a small vertical offset of the legend.
- **nit (D9).** Consider a compact policy/size/ratio table for §5.5 (see D9).

## Procedural notes

- Read: main.tex (844 lines) and `tables/*.tex`, changelog.md, README.md, `_progress.json`,
  preflight.json, citation-notes.md, `citation-resolution-degersem2004.json`, refs.bib, `.5/.gitignore`,
  ../BRIEF.md (Strongest claim), ../EXPERIMENTS.md (grep), the v4 Opus review and the combined v4
  summary, issue #1185 with all 9 comments, and the `motor_validation` README.
- Independent scripts (scratchpad `motor-v5-opus/scripts/`): `hashes.py` (three-way SHA plus ancestry),
  `opt120.py` (normalization, matched/fixed ratios, increments for both families, current rules,
  paired estimators, aliasing, saved-array comparison), `grad.py` (L2/L3 box-scaled gaps and MAG
  components), and inline status-count and cogging checks. Evidence was extracted with
  `git archive origin/main`. None of these scripts imports verify.py, analyze.py or the renderer.
- One network request, to api.crossref.org for DOI 10.1109/tmag.2004.824616: anonymous, with a generic
  `Mozilla/5.0` User-Agent and no email, mailto or identity headers. The response was saved in its own
  directory and parsed with `python3 -I`.
- The worktree `/Volumes/Stripe/geode-fem/review-1171v4` is unmodified (`git status` empty, HEAD 76d78e1f).
  No `.5.review/` sidecar was staged.

## Summary

v5 scores **42/44 and advances**, with no critical flag. The v4 packaging blocker is fixed: the evidence
log is tracked through a narrow ignore exception, and verify.py passes from a clean `git archive`, with
all 115 manifest SHAs matching both the archive and their pinned source blobs. Every v4 review item is
resolved or declined with a reason. I independently reproduced each new v5 number from raw origin/main
records:

- **Normalization:** 1.4971 N m with both size factors removed, versus 1.6434 with length only; the
  wording is correct, and "per unit length" is gone.
- **Ratios:** the matched reference-current ratio is 10,170 against the saved 11,982, and 7,631 under
  fixed unscaled current. Each is labelled by its current rule.
- **Candidate refinement:** the increments are 3.05e-8 and 2.90e-8; the alternate family reaches
  3.22%; the interface effect is ×96 the last increment.
- **Paired estimator:** 1.3e-4 / 1.1e-5.
- **L3 MAG:** 1.22/1.87 and 0.57/1.67.
- **Solve statuses:** 3,240 rows, all at tolerance.

The cogging caveat now sits beside 12.9% → 0.10% in the abstract and the conclusion. "within 1.3e-9"
is kept. Experiment 2 matches #1185 amendments 1-6 and the addendum, with C labelled an empirical RBF
surrogate. There is no speedup or smoke claim, and Robb Walters is the sole author. The remaining items
are five minor precision edits: the "vector norm" wording, "All costs count", the De Gersem verb, the
unreferenced supplement, and the dual use of "converged".
