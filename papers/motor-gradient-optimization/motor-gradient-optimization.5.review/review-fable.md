# Review: motor-gradient-optimization.5 (PR #1171, head 76d78e1f)

Reviewer: Fable (Anvil `paper-review` role; independent, skeptical referee from computational electromagnetics / electric-machine design). Fresh score of v5 at head `76d78e1f373b20d0c9c7920582b5a0ddd71082c2`; the v4 scores (Opus 38/44, Fable 36/44) do not carry over.
Reviewed: `papers/motor-gradient-optimization/motor-gradient-optimization.5/main.tex` (844 lines; 17 pp PDF; 6 figures, 4 tables, 14 references) with `tables/*.tex`, `refs.bib`, `changelog.md`, `README.md`, `preflight.json`, `evidence-manifest.json`, `verified-numbers.json`, `verify.py`, `citation-notes.md`, `citation-resolution-degersem2004.json`, `figures/opt120-waveforms.pdf`, `../BRIEF.md`, `../EXPERIMENTS.md`, `../.anvil.json`, and the v4 reviews at `/Volumes/Stripe/scratch-codex-review/motor-paper-v4-reviews/` (used only to check that each v4 item, in particular my M1–M3, was fixed rather than reworded).
Clean-archive check: `git archive 76d78e1f papers | tar -x` into `scratchpad/motor-v5-fable/archive/`, then `python3 -I -B verify.py` in the extracted `.5/` directory. Exit 0, three PASS lines (115 hashes; reference reruns and candidate comparisons; v5 size/current normalization, matched-policy ratios, refinement increments, paired estimators, L3 MAG components, 3,240 candidate solve statuses). The tracked file list at `76d78e1f` equals the on-disk file list; `figures/src/data/m1_tables.log` is tracked and the package `.gitignore` carries the `!` exception. `preflight.json` now records `artifact_verify_source: clean Git archive`.
Evidence: all 115 manifest snapshots were hashed against `git show <source_commit>:<source>` for their pinned commits; 115 match. All four evidence commits (`581ee7ad`, `910593ba`, `224332c5`, `8acd21ec`) are ancestors of `origin/main`. Every number below was recomputed from the raw run records in the clean archive with my own script (`scratchpad/motor-v5-fable/scripts/recheck_v5.py`), not read back from `verify.py`, `verified-numbers.json` or the benchmark summaries.
Rendered PDF: all 17 pages read at 110 dpi; pages 8, 12 and 16 re-rasterized at 170 dpi; `figures/opt120-waveforms.png` read.
Deterministic checks (`.anvil` venv via `uv`, run on the archive copy, no sidecar written): `numeric_consistency` 419 numbers, 1 finding (the known false positive at line 437, confirmed below); `audience_check` 0 hits; `pending_marker` grep 0; `preflight.json` compile ok, 0 overfull boxes, 0 placeholders, bib lint clean.
Network: none this pass. The De Gersem resolution record in the package was produced by the author with a generic User-Agent and no identity; I did not repeat it.
Worktree: nothing under `/Volumes/Stripe/geode-fem/review-1171v4` created or modified (`git status --short` empty before and after); no `.review` sidecar staged.

## Verdict

**Total: 39 / 44**
**Decision: advance: true** (total clears the 35 threshold; no critical flag).
**Critical flags: 0.**
**Outstanding dependencies: none** (0 pending markers).

The v4 blocker is gone and was fixed the right way: the log is tracked with an explicit ignore exception, the verifier runs from a `git archive` of the commit, and the preflight records that it did. The three majors I raised on v4 are fixed in substance, not reworded. The size scalars are stated (80 mm, s = 1.0477, SD1 130.293 mm, angle −11.646°), the size factor 0.878175 is named, Table 4 has an Opt120 column, and the conclusion no longer says fixing physical current "removes" the mean-torque advantage. The saved-vs-GEODE Opt120 comparison is in §5.5 with mean, both loaded ripples, the pointwise gap and the like-for-like ratios. The non-shrinking increments, the alternate mesh family and the 96× separation are stated. The paired-estimator check, the conjectured mechanism, De Gersem 2004, the categorical markers in Fig. 4(c), the band in Fig. 4(a), the finest-IGA-zero caption, the "neither sequence is fully converged" caveat in abstract and conclusion, the compressed protocol paragraph, the floating Table 2 and the pinned-commit wording are all present and correct.

On the two places where Codex says it corrected my v4 numbers rather than copying them: Codex was right both times, and I recomputed both from the raw records (details under "Codex's corrections").

What keeps the score at 39 rather than higher is presentational and one incompleteness in the separation of effects:

- **M1 (D6, D5).** The Opt120 waveform figure, the one picture that makes "×3.6" a visible mechanism, exists in the package (`figures/opt120-waveforms.pdf`) but is neither included in nor referenced from `main.tex`. A reader of the PDF cannot know it exists. Either include it as a figure or cite it as supplementary material with a public link in the availability section.
- **M2 (D1, D3).** The size normalization is applied to one of the three current rules. Under the rule the prospective problem actually uses (fixed physical current), the same algebra gives 1.2550 / 0.878175 = 1.4291 N m, 1.0% below the initial mean, not 13% below. The paper leaves the reader with "8.9% and 13.0% below" at saved size and "+3.7%" normalized at fixed unscaled current, and never states the normalized fixed-physical value. The conclusion's sentence is correct but stops one step short of the number a designer would want.

| # | Dimension | Score | Weight |
|---|---|---|---|
| 1 | Rigor of method / argument | 5 | 6 |
| 2 | Evidence sufficiency | 5 | 6 |
| 3 | Clarity of contribution | 4 | 5 |
| 4 | Related-work positioning | 5 | 5 |
| 5 | Reproducibility | 5 | 5 |
| 6 | Figure & table quality | 3 | 4 |
| 7 | Prose & structural quality | 4 | 4 |
| 8 | Citation hygiene | 5 | 5 |
| 9 | Rhetorical economy | 3 | 4 |
| | **Total** | **39** | **44** |

### Top 5 revision priorities

1. **M1.** Put the Opt120 waveform (order 45 / order 93 / conforming, full current, 40 angles) into the manuscript, or reference it as a supplement with a public link. The changelog says it was added; the paper as read does not contain or cite it.
2. **M2.** Complete the separation of effects with the normalized value under fixed physical current (1.4291 N m, −1.0%), and say in the conclusion what the normalization shows: at the unscaled cross-section and 100 mm, Opt120's full-current mean is within ±4% of the initial under every rule tested (−1.0%, +3.7%); the saved-size deficits of 9% and 13% are size, not current rule.
3. **m6.** The length-only figure 1.6434 N m is given with no percentage and no reminder that it still carries s² = 1.098; add "(+13.9%, retaining the factor s² = 1.098)" or drop it, since the both-factors value is the comparable one.
4. **m4.** The matched-current result (GEODE's Opt120 loaded ripple 2.0× the saved arrays while the mean agrees within 0.1%) is the paper's first sentence in its purest form and is not in the abstract or introduction. One clause.
5. **m3 / m1.** Split the §5.5 paragraph at lines 471–492 (nine claims in one paragraph) at "Between L2 and L3" and at "A possible explanation"; and let Fig. 4 float nearer its first reference (it is first cited on p.8 and lands on p.11).

## Codex's corrections to my v4 numbers

Both recomputed from `validation/runs/opt120_export_L3_h45_fixed.json`, `..._reference.json`, `initial_nurbs_L3_h45_fixed.json` and `reference_data.json` in the clean archive, 20-angle grid, coupling torque.

**Normalization.** The record's `torque_factor` is 0.878175028 = s²·L/0.1 with s = 1.04771722 and L = 0.08000054 m, so s² = 1.09771. Dividing the fixed-unscaled-current mean 1.314699 by 0.878175 gives **1.4971 N m, +3.73%** against the initial 1.443258. That removes both the length factor and the radial-scale factor. Normalizing length alone (× 0.1/0.0800005) gives **1.6434 N m, +13.86%**, still carrying s². My v4 text called 1.497 "per unit length"; the arithmetic was right and the label was wrong. Codex's v5 wording ("removing both size factors … 1.4971; normalizing only length … 1.6434") is correct.

**Ratio.** The 7,631 ratio uses GEODE's fixed unscaled current at Opt120 (I* = 9.6777 A, the initial design's current), F_init = 8.1714e-3, F_opt = 1.07075e-6, ratio 7,631.5. The saved reference arrays evaluate Opt120 at the reference's saved current (11.0485 A unscaled, 11.5757 A physical), F_saved,init = 7.9282e-3, F_saved,opt = 6.6170e-7, ratio 11,981.6. Those are different current policies, so my v4 "7,631 vs 11,982" juxtaposition was not like for like. Under the matched saved current GEODE gives full std 3.528e-4, half std 5.530e-4, with zero-current std 6.110e-4 (rule-independent, taken from the fixed-rule record): F_opt,ref = 8.0351e-7, ratio **10,169.6 → 10,170**. Codex was right; the v5 sentence "10,170 for GEODE and 11,982 for the saved reference; the 7,631 ratio above instead holds unscaled current fixed" is the correct statement. The accompanying numbers also match: GEODE mean 1.4986 vs saved 1.5000 (−0.10%), full-std ratio 2.01, half 5.53e-4 vs 4.37e-4, max pointwise gap over the two loaded currents 2.058e-3 N m.

## Separation of effects (sizing vs current policy)

Correct as far as it goes, and now complete on the sizing side: Table 4's Opt120 column carries MAG … Sr1, LENGTH 80.0005 mm, φ₀ −11.6463°, ScaleR 1.0477 (all match the record scalars to the displayed precision); §5.5 states the factor 0.878175 and that every Opt120 torque carries it; the Fig. 4(d) caption says "at its saved size" and "initial mean at the initial size"; the conclusion says the stack and radial scale "must be separated from current policy" and that at saved size Opt120 exceeds the initial mean only under the density rule. All three rule values recompute: 1.3147 (−8.91%), 1.2550 (−13.04%), 1.4983 (+3.81%).

Incomplete on the current-policy side (M2): the normalization is shown only for fixed unscaled current. The same division for fixed physical current gives 1.4291 N m (−0.98%); for the density rule 1.4983/0.878175 = 1.7062 N m (+18.2%). The first of these is the one that matters for the prospective fixed-physical-current problem and is absent. With it, the reader sees that the 9% and 13% saved-size deficits are almost entirely the 20% shorter stack, which is what the v4 reviews asked the paper to show.

## Numbers recomputed from the clean-archive blobs

All from raw per-angle `T_coupling` / `T_arkkio_outer` rows, gradient vectors and the saved reference arrays; none from summaries.

| Claim (main.tex line) | Paper | Recomputed | Status |
|---|---|---|---|
| Size scalars (501–503) | 80 mm; s = 1.0477; 130.293 mm; −11.646° | 0.0800005 m; 1.0477172; 0.1302931 m; −11.64627° | ok |
| Size factor (504) | 0.878175 | 0.878175028 (= s²L/0.1) | ok |
| Both factors removed (505–506) | 1.4971; +3.7% | 1.4971; +3.73% | ok |
| Length only (506–507) | 1.6434 | 1.6434 (+13.86%) | ok |
| Three rules (495–498) | 1.4433; 1.3147 / 1.2550 / 1.4983; −8.9 / −13.0 / +3.8% | 1.44326; same; −8.91 / −13.04 / +3.81% | ok |
| Matched-current Opt120 (513–517) | 1.4986 vs 1.5000; 3.53e-4 vs 1.76e-4 (2.0×); 5.53e-4 vs 4.37e-4 | 1.4986 vs 1.5000 (−0.10%); 3.528e-4 vs 1.759e-4 (2.01×); 5.530e-4 vs 4.374e-4 | ok |
| Max pointwise gap, loaded (518–519) | ≈ 2.1e-3 | 2.058e-3 | ok |
| Matched ratios (519–521) | 10,170 GEODE; 11,982 saved; 7,631 fixed | 10,169.6; 11,981.6; 7,631.5 | ok |
| Export-mesh F increments (483–484) | 3.05e-8; 2.90e-8 | 3.052e-8 (L1→L2); 2.900e-8 (L2→L3) | ok (ratio 0.95) |
| Alternate (construction) family (485–486) | 3.22% L2→L3 | 3.22% (of L3; 3.32% of L2); construction vs export at L3 0.29% | ok |
| Interface effect / last increment (487–488) | 2.79e-6; ≈ 96× | 2.793e-6; 96.3× | ok |
| ×3.6; initial +16%; ratios (473–477) | 3.6; 16%; 7,631 / 2,454 / 2,444 | 3.608; +16.03%; 7,631 / 2,454 / 2,444 | ok |
| L2→L3 F change over L3, max (480–482) | 1.36% initial; 2.71% Opt120 | −1.36 / −1.29 / −1.14%; +2.71 / +1.51 / +0.76% | ok |
| Opt120 full std 45→93; cogging std (478, 492) | 0.00066 → 0.00164; 0.00061 → 0.00035 | 0.000660 → 0.001636; 0.000611 → 0.000350 | ok |
| Paired estimator, Opt120 L3 h45 full current (464–466) | ≈ 1.3e-4 mean; 1.1e-5 std | 1.32e-4; 1.09e-5 (half: 1.01e-4 / 2e-8; zero: 6.1e-5 / 6.6e-7) | ok |
| 3,240 rows, 18 records at tolerance (531–532) | 3,240; 18 | 3,240 rows; 18 `ok` records; 0 non-tolerance rows; 1 `invalid_mesh` record | ok |
| g10 vs g40 at L3 (526–528) | +1.05% / −5.90% / −17.1% | +1.05 / −5.90 / −17.11% | ok |
| L3 MAG ripple pairs (671–672) | 1.22 vs 1.87; 0.57 vs 1.67 | 1.217 vs 1.867; 0.569 vs 1.673 | ok |
| "norm changes by about 1%" for perturbed harmonic (673–674) | ≈ 1% | box-scaled ripple norm change L2→L3: h45 0.46 / 1.19%, conforming 0.49 / 1.13% (initial / perturbed); MAG component −24% | ok |
| Abstract: reduction above 2,400-fold (28–29) | > 2,400 | min 2,444 (conforming) | ok |
| Reference refinement, unchanged from v4 (431–437) | 0.01071 / 0.01220 / 0.01212 / 0.01208; 0.10%; 0.35 / 0.27%; 3.4e-5 | same (verified-numbers and my v4 script agree; blobs unchanged, hashes match) | ok |
| Full-order identity (274–275) | 4.1e-9; 1.3e-9 | 4.057e-9; 1.2417e-9 | ok (1.3e-9 is the agreed fixed point) |

`numeric_consistency` flags one claim: "0.27% higher" (line 437) paired by the ±1-paragraph window with "from 0.01071 to 0.01220". The claim compares GEODE's full-current std with the finest reference (recomputed +0.269%); the pairing is wrong. Confirmed false positive, already adjudicated in `preflight.json`.

## Resolution of v4 comments

| v4 item | v5 status |
|---|---|
| S1 / Opus B1 (critical): `m1_tables.log` untracked, `verify.py` fails from checkout | Resolved: tracked at `76d78e1f`, `.gitignore` exception, `verify.py` exits 0 from `git archive`, preflight records the archive source. |
| Fable M1 / Opus S1: sizing omitted from the mean-torque feasibility argument | Resolved in substance (scalars, factor, Table 4 column, caption, conclusion rewritten). Residual: normalization shown for one rule only (M2 below). |
| Fable M1 wording "per unit length 1.497" | Codex's correction is right; v5 text is correct. |
| Fable M2: saved-vs-GEODE Opt120 comparison absent | Resolved: §5.5 second paragraph; reference refinement at Opt120 stated as not run. |
| Fable M2 ratio "7,631 vs 11,982" | Codex's correction is right; v5 gives 10,170 matched and keeps 7,631 labeled. |
| Fable M3 / Opus S3: non-shrinking increments | Resolved: 3.05e-8 / 2.90e-8, alternate family 3.22%, 96× separation, no asymptotic claim. |
| Fable m1: paired estimator | Resolved (464–467), with the geometry-specific limit retained. |
| Fable m2 / Opus S2: half-current 0.3%; "0.10%" looks like accuracy | Resolved: "both about −0.3% and do not shrink; we therefore leave its remaining error unestimated"; caveat in abstract and conclusion. |
| Fable m3: mechanism | Resolved as a labeled conjecture with the loaded/cogging contrast. |
| Fable m4: De Gersem–Weiland 2004 | Resolved: cited (94–95), resolved anonymously via Crossref, metadata archived. |
| Fable m5 / Opus D6: Fig. 4(c) lines; no waveform; IGA zero | Fig. 4(c) now markers only; caption explains the zero by construction; waveform exists as a supplement but is not in or cited by the manuscript (M1). |
| Fable m6 / Opus m7: layout | Resolved: Table 2 floats on p.8, p.7 is full, no orphaned page; Fig. 4(d) label moved clear of the bar. Fig. 4 now lands on p.11, three pages after first citation (m1). |
| Fable m7 / Opus D9: prospective protocol | Resolved: one paragraph (751–761). |
| Fable m8: caveat list | Declined with a reason; acceptable. |
| Fable m9: GEODE toolchain versions | Resolved by pointer to benchmark READMEs (809–810). |
| Fable m10: validation solve statuses | Resolved: 3,240 rows / 18 records, recomputed. |
| Fable m11 / Opus m2: snapshots "match main" | Resolved: README says snapshots match pinned commits, names the harmonic README exception. |
| Opus S4: ×3.6 context | Resolved in abstract and conclusion (+16%, > 2,400-fold). |
| Opus S5: saved-reference captions | Resolved: Table 1, Fig. 2, Fig. 3 captions point to §5.4. |
| Opus S6: L2-only component picture | Resolved: L3 MAG pairs in text; Fig. 6 caption points to the text. |
| Opus m1, m3–m6, m8 | Resolved (amendments 1–6 in BRIEF/EXPERIMENTS; GEODE, L1 and SD1 glossed; "differences at most"; source line split; `\bibsep`). |

Every v4 comment is accounted for in `changelog.md`; none is silently dropped. My v4 review is archived byte-identically in `.4.review/review-fable.md`.

## Figures

All 17 pages read. Fig. 1–3, 5, 6 are byte-identical to v4 (SHA match) and were already acceptable. Fig. 4 is the only changed figure: (a) now shows the GEODE L3 band around the dashed line and the IGA sequence 3×3 → 24×24 entering it; (b) circles at the four subdivisions with the finest at zero, horizontal lines for GEODE L3, caption says why the last point is zero; (c) markers without connecting lines for the three categorical models, the ×3.6 annotation on the Opt120 order-93 point, log axis spanning 1e-6 to 1e-2; (d) three bars with values, the "Dashed: initial (100 mm)" label in clear space at the upper left, no overlap. Table 4 renders with wrapped ragged-right descriptions and the Opt120 column aligned. No overfull boxes; no blank partial pages; references end cleanly on p.17.

The Opt120 waveform supplement (`figures/opt120-waveforms.pdf`) shows exactly what §5.5 argues: at order 45 the full-current torque is a smooth 1.3131–1.3154 N m curve, while order 93 and conforming coincide with each other and carry a visible ≈ 5° ripple of ±2 mN m peak-to-peak about 6 mN m. It is a good figure. It is not in the paper (M1).

## Scoring with quoted evidence

**D1 Rigor 5/6.** The method chain is sound and now complete on the items v4 lacked: "Every Opt120 torque carries the explicit size factor $s^2L/(0.1\,\mathrm{m})=0.878175$" (§5.5); "the successive L1–L2 and L2–L3 increases in $F$ are $3.05\times10^{-8}$ and $2.90\times10^{-8}$ … so they barely shrink and do not establish an asymptotic rate" (§5.5); "the outer-band and coupling estimates differ by about $1.3\times10^{-4}$ N m in mean and $1.1\times10^{-5}$ N m in standard deviation" (§5.5); the matched-policy comparison is kept separate from the fixed-current one and labeled. Deductions: M2 (the normalization that separates size from rule is applied to one rule, so the fixed-physical-current comparison the prospective problem needs is left at saved size); the cross-model gradient comparison still changes estimator and interface together (acknowledged at §6.3, "this experiment measures their combined effect"); the conforming Opt120 value rests on outer-band Arkkio checked against coupling torque on the harmonic field, not on the conforming field (the paper says so).

**D2 Evidence 5/6.** 1,640 original forward solves, 3,240 candidate rows over 18 records (all at tolerance, recomputed), 300 reference reruns across four subdivisions, 704 distinct FD cases with 592 Taylor windows, pinned L3 gradients, and now the L1 export record and the alternate L2/L3 construction family. The evidence for each claim is present and the one unresolved item is declared: "Reference refinement was run only at the initial design, so the cause of the Opt120 discrepancy remains unresolved" (§5.5). Deductions: that discrepancy (GEODE's Opt120 ripple 2.0× the saved arrays) is the strongest instance of the paper's thesis and has no refinement evidence on either side at Opt120; the reference refinement remains at one design; the waveform evidence for the ×3.6 mechanism is outside the manuscript (M1).

**D3 Clarity of contribution 4/5.** The organizing idea opens the abstract and each contribution is extractable from §1 paragraph 3. The conjecture is now labeled as one: "This is a conjecture, not an ablation result" (§5.5). Deductions: the abstract and introduction do not mention the matched-current Opt120 result (0.1% mean agreement with 2.0× ripple), which is the paper's first sentence made concrete (m4); the conclusion's size/current sentence stops one number short (M2), so a reader still cannot say from the conclusion whether Opt120's cross-section gains or loses torque under the prospective rule.

**D4 Related-work positioning 5/5.** The origin of the method is now credited: "Harmonic weighting of a sliding interface was developed for finite-element machine models by De Gersem and Weiland (2004)" (§2), with the claim limited to what the resolved title supports (`citation-notes.md`). Egger's order ladder and Bontinck's stability result are engaged on their merits and the paper states what it adds: "torque sensitivity to interface order is already established; our study quantifies its effect on loaded nonlinear ripple, a saved optimized design, and design sensitivities." No litsearch sibling; no deduction for its absence. No close prior work ignored.

**D5 Reproducibility 5/5.** Declared artifact check passes from a clean `git archive` of the reviewed commit; 115 snapshots hash-match their pinned source blobs; all evidence commits are on `origin/main`; Octave/GeoPDEs/nurbs versions in text, GEODE-side Gmsh and Rust versions pointed to ("the benchmark READMEs record the GEODE-side Gmsh and Rust toolchain versions", §Code and data); `render_v5.py` and `papers/build.sh` exist; `README.md` states that snapshots match pinned commits rather than main. Timing is unreproducible in absolute terms and the paper says so. The one reproducibility-adjacent gap is that the waveform supplement is not linked (counted under D6).

**D6 Figure & table quality 3/4.** Fig. 4 is now correct in every panel (markers for categorical models, band in (a), zero explained in (b), label clear in (d), "at its saved size" in the caption). Table 4 has the Opt120 column with wrapped descriptions. Layout defects of v4 are gone. Deductions: the Opt120 waveform figure is absent from the manuscript although it is the natural carrier of the ×3.6 claim (M1); Fig. 4 is first cited on p.8 and appears on p.11 (m1); Fig. 6 remains an L2 picture with the L3 movement in text only (acceptable, caption pointer present).

**D7 Prose & structural quality 4/4.** Flow intact, tense consistent, 0 em-dashes, no virtue words, no "load-bearing", no "certified", no version-history language, no governance vocabulary (audience check 0 hits), author present, 0 overfull boxes, no `??`. Nits that did not cost a point: hard-coded "Section 5.1" at line 440 instead of `\ref` (m2); the §5.5 paragraph at 471–492 carries nine claims and reads as a list (m3).

**D8 Citation hygiene 5/5.** All 14 keys resolve; every entry has author, title, venue, year and DOI (Arkkio 1987 has the institutional URL); the new De Gersem entry was resolved by DOI with the original Crossref record archived and the author spacing corroborated from a second source. Every claim about prior work is specific and matches `citation-notes.md`.

**D9 Rhetorical economy 3/4.** The prospective protocol is now one paragraph; the opening is the thesis; the headline numbers appear where they should. Deductions: §5.5 has grown to about 60 lines of numbers in which nine claims share one paragraph (m3); the 3.6-fold and size/current statements are restated in the abstract, §5.5, §7 and §8 with little compression; Appendix A is still cited once and would fit in two sentences of §7.

## Critical-flag check

- **Build / compile, including external artifact (`artifact_verify`):** PDF present, 17 pages, compile ok, 0 overfull, bib clean. **`python3 -I -B verify.py` exits 0 from a clean `git archive` of `76d78e1f`** with three PASS lines. No flag.
- Citation error: none.
- Numerical inconsistency: none; 22 v5-new or v5-changed claims recomputed from raw blobs match; the one detector finding is a confirmed false positive.
- Missing experiment for a claim: none; no speedup claim; the prospective protocol is labeled as unmeasured.
- Close prior work ignored: none.
- Audience: 0 hits.
- Pending markers: 0.

## Findings by severity

S = blocking (critical flag), M = major, m = minor.

**(No S findings.)**

**M1 (D6, D5; §5.5, Fig. 4, §Code and data).** `figures/opt120-waveforms.pdf` (order 45 / 93 / conforming at full current, 40 angles, with a mean-centered panel) is in the package and described in `README.md` but is not included in `main.tex` and not referenced from it; `grep opt120-waveforms main.tex` returns nothing. The paper's reader has no way to see the ≈ 5° content that reappears above order 45. Fix: either add it as Fig. 5 after Fig. 4 (one third-page figure) or add a sentence in §5.5 and a public link in the availability section ("A supplementary figure, `opt120-waveforms.pdf` in the paper package at …, shows the three waveforms").

**M2 (D1, D3; §5.5 lines 494–510, §8 lines 778–781).** The normalization is applied to one rule. Add the other two, or at least the one the prospective problem uses: fixed physical current 1.2550 / 0.878175 = 1.4291 N m (−1.0% vs the initial 1.4433); density rule 1.4983 / 0.878175 = 1.7062 N m (+18.2%). Then the conclusion can say what the separation shows: at its saved size Opt120 is 9–13% below the initial mean under the two fixed-current rules; with both size factors removed it is within −1.0% to +3.7% of the initial under those rules, so the saved-size deficit is the 20% shorter stack and not the current rule. Keep the sentence that these are algebraic normalizations, not reoptimized designs.

**m1 (D6; layout).** Fig. 4 is first cited at §5.4 (p.8, "Fig. 4a") and lands on p.11. `[!t]` plus the preceding floats push it past §5.5 entirely. Consider `[!htb]` or moving the figure environment to just after the §5.4 paragraph that first cites it.

**m2 (D7; line 440).** "the cogging mismatch in Section 5.1" is hard-coded; use `Section~\ref{...}` (a label on §5.1 is missing).

**m3 (D9, D7; §5.5 lines 471–492).** One paragraph carries: the definition of F, the ×3.6 result, the initial +16%, the three ratios, the full-current std rise, the L2→L3 relative changes, the two absolute increments, the alternate family, the 96× separation, and the conjecture. Split after "depends strongly on the interface model" and before "A possible explanation".

**m4 (D3; abstract, §1).** Add one clause for the matched-current result: "at the saved design, GEODE's loaded ripple is twice the saved reference's while the mean agrees within 0.1%". It is the most direct instance of the paper's opening sentence and currently appears only in §5.5.

**m5 (D1; §5.5 line 519).** "With this matched current policy and zero-current values included" does not say that the zero-current std comes from the fixed-rule record (the reference-rule record has only 0.5 and 1.0). It is rule-independent by construction, so one parenthesis suffices: "(zero current is rule-independent and is taken from the fixed-current record)".

**m6 (D3; §5.5 lines 506–507).** "normalizing only length to 100 mm while retaining the actual radial scale gives 1.6434 N m" is given with no percentage and no reminder that it still carries s² = 1.098. Either add "(+13.9%, retaining the factor s² = 1.098)" or drop the length-only figure, since the both-factors value is the comparable one and the length-only one invites a wrong comparison with 1.4433.

**m7 (D2; §5.5 lines 522–523).** "the cause of the Opt120 discrepancy remains unresolved" could name the candidate: the 3×3 IGA reference under-resolved cogging by 13% at the initial design (§5.4), so under-resolution of the saved Opt120 arrays is the obvious conjecture; label it as such. Not required.

**m8 (D9; §7–§8).** The 3.6-fold statement and the size/current statement each appear four times (abstract, §5.5, §7, §8). §7's first paragraph could drop its restatement.

**m9 (procedural; `preflight.json`, line 437).** The numeric-consistency false positive recurs every pass. A suppression comment on the line above (`% anvil-lint-disable: numeric_consistency`) would record the adjudication in the source rather than in the preflight.

**m10 (D9; Appendix A).** Still cited once from §7; two sentences in §7 would carry it.

## Procedural notes

- Review written to this file per dispatch; no `.review` sidecar staged; nothing under the worktree created or modified.
- Scratch: clean archive at `scratchpad/motor-v5-fable/archive/`; recheck script `scratchpad/motor-v5-fable/scripts/recheck_v5.py`; page rasters `scratchpad/motor-v5-fable/pages/`.
- Deterministic detectors were run from the `.anvil` project venv via `uv` on the archive copy without `--write-review`.
- No network requests were made this pass.
