# paper-audit: motor-gradient-optimization.3 (PR #1171 head c5f83e45)

Auditor: anvil:paper-audit (Claude Opus 5.5), 2026-10-17.
Version dir (read-only, untouched; `git status --short --ignored` on the worktree is clean before and after):
`/Volumes/Stripe/geode-fem/review-1171v3/papers/motor-gradient-optimization/motor-gradient-optimization.3/`
Worktree HEAD: `c5f83e45fc9effc776f38b671efaad63099775e3`.
Evidence: `origin/main` = `224332c5` (fetched in `review-main`). Evidence commit `910593ba` is an ancestor of origin/main. All 78 manifest snapshots are byte-identical (sha256) to their sources, both at the recorded `source_commit` and at `origin/main`: 156/156 hash checks pass.

The verdict is anchored to this head only. The v2 audit (FAIL on 6.83e-10) is not carried over. Its fixes were re-checked from scratch.

Deviation from the command contract, at the operator's request: no `<thread>.3.audit/` sidecar was staged in the worktree. This single file replaces citation-audit.md, numerical-audit.md and flags.md. The compile log is next to it at `compile-log.txt`, built from a copy in `build/`.

## Verdict

**PASS. Eligible for AUDITED.** No critical flags, pending gate passes, link-hygiene gate passes.

- Critical flags: **0**.
- Unresolved cites: 0. Claim-support failures (`does-not-support`): 0. Bib-metadata defects: 0.
- Build: OK. Sequence was pdflatex, bibtex, then pdflatex ×4; every pass exited 0. `.aux` files from passes 3, 4 and 5 are byte-identical, so the build converged. The final-pass log has no undefined citations or references, no `Rerun` request, 0 overfull boxes and 0 underfull boxes. The PDF contains no `??`. 15 pages. bibtex: 0 warnings. The regenerated `main.bbl` matches the committed one.
- `verify.py`: PASS (exit 0). It covers 78 hashes, 500 + 1140 solve statuses, 704 distinct FD cases, 592 Taylor windows, the cross-model sensitivities and slotless rounding.
- Figure/table regeneration: in a scratch copy, `render_v3.py` with the pinned NumPy 2.5.4, Matplotlib 3.11.2 and meshio 5.3.5 reproduces all five figure PDFs, all three `\input` tables and `verified-numbers.json` **byte-identically**. None is stale.
- Pending-marker gate (`anvil.lib.pending_marker`): pass, 0 markers.
- Audience/link-hygiene scan (`anvil.lib.audience_check`): pass. `public_repo_url` is declared via `.anvil.json`. There are 0 unlinked artifact paths, 0 governance-vocabulary hits and 0 private-locator hits.

## Critical flags

None.

## v2 audit fixes: confirmation

| v2 item | v3 state | Confirmed |
|---|---|---|
| Critical 1: slotless max error 6.83e-10 | main.tex:654 now reads `6.82\times10^{-10}`. The recorded value is 6.8247e-10. `verify.py` now derives both slotless strings from raw `slotless_screen.json` and asserts that `6.83` is absent. | yes |
| arkkio2012proper type and booktitle | `@inproceedings`, booktitle "2012 International Symposium on Power Electronics, Electrical Drives, Automation and Motion (SPEEDAM)". The PDF (p. 14) renders "In 2012 International Symposium … (SPEEDAM), pages 1456–1461", and the duplicated "Power Electronics" is gone. Crossref: proceedings-article, pp. 1456-1461, 2012-06, authors Arkkio and Hannukainen. | yes |
| wiesheu2025combined version of record | `@article`, Optimization and Engineering 26(2):1011–1038, 2025, doi 10.1007/s11081-024-09925-0. Crossref matches on all fields, including the six authors in order (print 2025-06, online 2024-09-21). The Correction 10.1007/s11081-024-09947-8 is a separate record (pp. 1039–1040) and is not cited. That is acceptable: the author's note says it is licensing-only. I did not independently verify the correction's content, and it does not affect the citation. | yes |
| "compares" → "combines … compares" wording | main.tex:70-72: "Their related study combines parameter and shape optimization with isogeometric analysis and compares against each alone". The OpenAlex abstract says it combines parameter and shape optimization and is "compared to designs, which were optimized using parameter and shape optimization separately". | yes, supports |
| egger2022torque article number | `pages = {6}`; renders as "12(1):6". Crossref has `article-number: 6`, volume 12, issue 1. | yes |
| Fig. 4(c) unit mismatch (non-critical) | Axis is now in N m/m with +1.68 / −220.85 annotations, matching the text at line 457. | yes |
| Optional: scotzniovsky key/year | Key renamed `scotzniovsky2025`; year 2025, 26(1):121–158 | yes |

## Citation audit

There are 13 bib entries and all 13 cited keys resolve; `verify.py` also asserts this. All 12 DOIs were checked against the Crossref API (`api.crossref.org/works/<doi>`), and every one resolves. Abstracts came from OpenAlex, arXiv, or the authors' institutional postprint. `<thread>/refs/` holds only screening metadata, not PDFs, so claim support is judged from abstracts.

| Key | Metadata vs Crossref | Surrounding claim (line) | Verdict | Notes |
|---|---|---|---|---|
| wiesheu2024spline | OK. ICEM 2024, pp. 1-6, doi 10.1109/ICEM60801.2024.10700065; 6 authors in order. Crossref drops the umlaut ("Schops"); the bib is correct. | reproduction target; spline rotor+stator shape optimization with engineering parameters (41, 67-69) | supports | Unchanged from v2. |
| wiesheu2025combined | OK (see above) | combines parameter and shape optimization with IGA, compares against each alone (70-72) | supports | |
| scotzniovsky2025 | OK. Optim. Eng. 26(1):121-158; print 2025-03, online 2024-05-22; 7 authors in order | adjoint gives many sensitivities without a solve per parameter (49-50); FFD + differentiable mesh warping + EM analysis with adjoints (77-78) | supports at title level; detail author-verified | No abstract was retrievable by the auditor. Springer serves a bot-protection page, and OpenAlex and Crossref carry no abstract. `citation-notes.md` records an author-side check of the publisher abstract naming FFD, PDE-based mesh warping with exact derivatives and FE analysis. The title is consistent. Non-critical. |
| merkel2021shape (new) | OK. IEEE Trans. Energy Convers. 36(4):2683-2690, 2021-12, doi 10.1109/TEC.2021.3061271; authors Merkel, Gangl, Schöps in Crossref order. Crossref ASCII-folds "Schops"; the bib umlaut is correct. | isogeometric shape derivatives and harmonic rotor–stator coupling to reduce EMF harmonic distortion (79-80) | supports | The OpenAlex journal abstract mentions a 6-pole IPMSM, multipatch IGA, stator and rotor coupled with harmonic basis functions, a shape derivative of EMF THD, and freeform optimization (75% reduction). The arXiv 1908.06009 abstract is consistent. |
| bontinck2018isogeometric | OK. CMAME 334:40-55, 2018; 4 authors | separate IGA subdomains, harmonic constraints, exact circles avoid polygonal air-gap error (84-86) | supports | Unchanged; v2 abstract check stands. |
| egger2022torque | OK, article 6 | energetic torque from harmonic mortar, ripple accuracy (86-88); Eq. 8 "consistent with" (246) | supports | Unchanged; v2 abstract check stands. |
| arkkio1987 | No DOI. The Aalto handle record matches citation-notes (1987-12-18, Helsinki Univ. of Technology). | gap-averaged Maxwell-stress torque (89, 215) | supports (standard attribution) | Unchanged. |
| howe1992cogging (new) | OK. IEEE Trans. Magn. 28(2):1080-1083, 1992-03, doi 10.1109/20.123869. Crossref authors are "Howe, D.; Zhu, Z.Q." in that order, and the bib follows Crossref. | sensitivity of cogging-torque prediction to FE discretization and evaluation method (92-93) | supports | OpenAlex abstract: accuracy "determined largely by the level of discretization, as well as the evaluation method"; energy method vs Maxwell-stress integration for cogging. The paper correctly avoids attributing its own single-circle observation (398-399) to this source. |
| arkkio2012proper | OK (see above) | mesh selection for torque computation (94) | supports | v2 abstract check stands: accuracy "sensitive to the quality of the finite-element mesh in the air-gap region". |
| ion2018robust | OK. Electr. Eng. 100(4):2635-2647, 2018; 8 authors in order | gradient-based vs stochastic optimization including a PMSM; parametrizations avoid remeshing (96-98) | supports | OpenAlex abstract: affine parametrization and design elements "avoid remeshing"; PMSM example; advantage over PSO. |
| kemmetmueller2015 | OK. Mechatronics 32:22-33, 2015; 3 authors | nonlinear MEC torque control including saturation and nonsinusoidal behavior (102-103) | supports | **Upgraded from v2's "partial (title-level)".** I retrieved the authors' ACIN postprint myself (it states it is identical in content to the published paper). The abstract says PSM designs are "characterized by a pronounced magnetic saturation and non-sinusoidal properties … MEC models allow to systematically account for these effects". |
| roomi2022 | OK. IET EPA 16(11):1286-1299, 2022; 3 authors | waveform-targeting surrogate (106-107) | supports | Unchanged. |
| wen2023 | OK. JCP 484:112082, 2023; Wen & Zahr | reduced models adapted along a shape-optimization trajectory, fluid examples (107-108) | supports | Unchanged. |

Reviewer and brief leads:
- **Howe & Zhu 1992**: added; correct.
- **Merkel et al. 2021**: added; correct.
- **Merkel, Kapidani, Schöps, Vázquez 2022** (IEEE Trans. Magn. 58(9), doi 10.1109/TMAG.2022.3186247), the torque-computation lead raised in the v2 audit, was **not added**. That is optional, not a defect. It would fit the torque-estimator paragraph (86-94) if the author wants an isogeometric-mortar torque reference besides Egger 2022.

Previously uncited claims, now handled:
- Single-circle estimator (398-399) is recast as the paper's own recorded observation. Merged `benchmarks/motor_reference/forward/README.md` (origin/main, sensitivity table and l.180) records single-circle vs Arkkio at L3 differing by 0.0023–0.025 N m, versus 7e-5 for the half-bands. The claim is supported by the evidence.
- The conforming interface (205-210) is now described as the authors' own nodal identification and reduction algebra. No citation is needed.

## Numerical audit

`verify.py` passes and covers these items:
- Table 1 cells and the harmonic ripple and mean percentages
- max pointwise harmonic gaps and the 500/1140 statuses
- Table 2 sensitivities, the estimator, free-end, full-order and ladder bounds
- 704 distinct / 688 / worst FD, the 96 duplicates, 592/530/388 Taylor windows and zero-current FD bounds
- SW1 +1.68/−220.85; the seven changed derivatives and three sign reversals (SW1, SW4, SD1)
- cost ratios and Table 3, Table 4 design points, the remesh MW fit/scatter and SW1 3.8%
- slotless RMSE and max, box widths, MAG ripple derivatives and the four relative norms
- cite keys and figure assets

Numbers **not** covered by verify.py were cross-checked independently. My script reads the evidence directly from `origin/main` via `git show`, mapping each snapshot to its source through the manifest. Where a sentence is unchanged from v2 and its source data is byte-identical, v2's check carries over and is marked "carried".

### New or changed in v3

| Text claim (line) | Source (origin/main) | Recomputed | Match |
|---|---|---|---|
| Order 45 vs conforming at L3: std +59% at zero current, 7–9% under load, relative to conforming (19-21, 41-43, 351-353, 619-621) | forward_harmonic summary `main_h45` L3 | +59.19%, −7.42%, −8.99% | yes |
| "All resolved interface orders reproduce conforming torque within a few 1e-9 N m on the two checked meshes" (21-22) | `hall_vs_conforming` | 4.057e-9 (L0, 60 orders), 1.242e-9 (L1, 120 orders) | yes |
| Loaded mean of either model within 0.16% of reference (43-44) | `main_h45` L3 | conforming −0.153/−0.156%; harmonic −0.002/+0.051% | yes |
| Harmonic cogging std L0–L3 0.0101, 0.0119, 0.0121, 0.0121; reference 0.0107 (344-346) | `main_h45` a=0, `h45_arkkio` | 0.01014, 0.01187, 0.01206, 0.01209; ref 0.01071 | yes |
| Conforming/Arkkio SW2 mean sensitivity L0–L3 −46.5, −41.3, −44.4, −47.2 N m/m (519-521) | design `study_L{0,1,2}_conforming_initial` + `gradient_L3_conforming_initial`, full current, d_mean[SW2] | −46.50, −41.28, −44.43, −47.246 | yes |
| Harmonic SW2 approaches −47.1 from −49.8 (521-522) | same, h45 | −49.78, −48.06, −47.30, −47.110 (monotone) | yes |
| SW3 harmonic sign change, roughly linear convergence (517-518) | same, d_mean[SW3] h45 | −0.784, −0.099, +0.406, +0.642 | yes (sign change L1→L2) |
| **Cross-model (§6.3).** MAG ripple derivative 1.28 vs 1.94 N m/m (initial), 0.75 vs 1.87 (perturbed), L2, full fixed current (532-535) | `study_L2_{h45,conforming}_{initial,perturbed}.json` `currents['1'].gradient.d_std[0]` | 1.2832 / 1.9371; 0.7534 / 1.8655 | yes |
| Relative norm of box-scaled difference, conforming denominator: mean 0.58% / 0.66%, ripple 6.7% / 10.2% (537-540) | same files; widths from `reference_data.json` bounds | 0.576 / 0.6646; 6.673 / 10.235 | yes |
| Fig. 5 widths 9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm in display order (553-554) | `optimization_parameter_bounds`, order MAG…Sr1 | 9, 10, 15, 4, 3, 1, 18, 70, 1.5 | yes |
| Both models evaluated at the same design point (implicit in §6.3) | design_point values, h45 vs conforming | identical lists | yes |
| Model differences "much larger than the discrete FD errors" (535-536) | best FD rel. err. of MAG std at those states | 4e-9 to 7e-8 vs ≈34–60% model difference | yes |
| Max FD error 1.07e-5 "for a small Sr1 ripple derivative" (480-481); abstract 1.1e-5 (28) | all loaded non-duplicate sweeps | 1.0668e-5, `study_L0_h45_initial`, a=0.5, Sr1 std, adjoint 7.99e-5 | yes |
| 800 records = 704 + 96 duplicates (475-479) | sweeps | 704 + 96 | yes |
| Taylor window anchor h = 1e-5 m; 1e-2 deg for operating angle (493-494, 506-508) | study `h_ref` = 1e-5 | 1e-5 (×1000 for degrees) | yes |
| Two loaded SW1 ripple anchored-window failures (508-510) | `taylor_window` decades = 0.0 | conforming, fixed-density, full current, initial, L0 and L1 SW1 std (exactly two) | yes; "widest window 2 decades" carried from v2 (README) |
| Cost: gradient 1.13–1.20 forwards; FD 8.2–10.5 including base (571-573) | 12 `cost_*.json` medians | 1.1256–1.1990; 8.2427–10.5324 | yes |
| FD step h = 1e-6 in stored units; 24 warm-started forwards (564-566) | design README l.439 | "24 forwards at h = 1e-6, each warm-started from the base" | yes |
| M3 Ultra, 28 cores, load 14–24; L0/L1 median of 3, L2 single (499, 558-559) | design README l.429-431; cost JSON `repeats` | same; 3/3/1 | yes |
| 1,640 forward validation solves (568-569) | 500 + 1140 | 1640 | yes |
| Fig. 4(c) linear within ±2 N m/m; Fig. 5 linear within ±1 mN m (496, 553) | `render_v3.py` `symlog linthresh=2` / `linthresh=1` (mN m units) | same | yes |
| Fig. 4(d) harmonic, tol 1e-14 (496-499) | render data / Table 3 | h45 tol1e-14 ratios 1.126/1.152/1.153 and 10.53/10.34/10.19 match plotted points | yes |

### Unchanged from v2 (carried; source data byte-identical, sentences unchanged)

These all reproduced in the v2 audit:
- geometry radii and length
- winding areas and the 16% figure, I*, angle grids
- 47 B-H points, the 0.75 T non-monotonicity, PCHIP 6.66% and 2.17e-4
- the 2e-14, 1e-9, 1.002/2.007 and 3.76e-4 verification numbers
- node counts and 120·2^ℓ
- 60 solves per level
- conforming −0.15/−0.16%, +9.9/+11.3%, −29.1%
- 0.0272 → 0.0034 max gap, the ~5e-5 half-current offset, the 1.13-fold cogging scale
- order 189 within 1.1e-4
- 9e-5 and 5.4e-5 controls
- zero-current 7.0e-4 and 8.9e-7
- MW remesh 2.6e-6, slotless RMSE 3.08e-10 and 24 angles

The 0.0034 figure is also covered by verify.py's `max_pointwise_harmonic_gap` (0.00339).

### Non-critical numeric notes

1. main.tex:256 states the L1 full-order identity as "$1.3\times10^{-9}$". The recorded value is 1.2417e-9, which rounds to 1.2e-9. As written it is a true upper bound: the sentence reads "within", and verify.py asserts `< 1.3e-9`. It is not a mismatch under the bound reading, and v2 accepted it. If the authors want displayed values to be rounded values, change it to 1.2e-9, which also stays consistent with the "a few 1e-9" abstract.
2. Some verify.py guards are range checks rather than display-string checks: cost ratios `1.12 < min`, `max < 10.54`; worst FD `< 1.07e-5`; L1 identity `< 1.3e-9`. A value that still passes could display differently (e.g. a min of 1.121 displays as 1.12). The current values round correctly. The new v3 claims that verify.py does not guard are all reproduced above: the 59% / 7–9% shifts, the cogging L0–L3 sequence, the SW2 sequences, and the 0.16% intro bound. Optional: add display-string assertions for these, as was done for the slotless metrics.

## Figures

- All five `\includegraphics` targets regenerate byte-identically from `render_v3.py` (pinned deps), so none is stale. Mtimes are checkout-uniform and carry no information.
- **Non-critical legibility note (Fig. 4c).** At 300 dpi, the "+1.68" annotation is drawn over the MW fixed-density bar, and the "Fixed group-1 density" legend entry overlaps the tops of the SD1/Sr1 fixed-density bars, which reach about 85 N m/m. Both labels stay readable. However, `preflight.json` `visual_check` reports "No clipping or overlapping labels found", which is not accurate for this panel. Fix: move the legend to `loc='lower left'`, or place it outside the axes, and set `xytext` for +1.68 to about (2.6, 300) or into empty space above SW1/SW2.
- Fig. 5 (model-gradients) has no clipping or overlaps. The series labels match the text (harmonic/coupling vs conforming/Arkkio).

## Flags summary

### Critical flags (block AUDITED)

None.

### Non-critical notes

1. Fig. 4(c) label and legend overlap; preflight's visual-check statement is inaccurate for this panel (see Figures).
2. L1 full-order identity shown as 1.3e-9 against 1.24e-9 recorded. True as a bound; optionally 1.2e-9.
3. scotzniovsky2025 FFD/mesh-warping detail: author-verified per `citation-notes.md`; the auditor could only verify at title level (publisher bot wall). Not a defect.
4. Optional citation: Merkel–Kapidani–Schöps–Vázquez 2022 (TMAG, isogeometric mortar torque) was not added; Merkel 2021 was.
5. Cosmetic bib style: `wiesheu2024spline` is double-braced and renders in Title Case; every other title renders in sentence case under plainnat. Optional: single braces for consistency.
6. Optional verify.py hardening: display-string assertions for range-checked values and for the new unguarded v3 numbers.

### Outstanding dependencies

None (0 pending markers).

### Audience-fit notes

None (0 governance-vocabulary hits, 0 private-locator hits). The author line is empty (`\author{}`), and `changelog.md` defers attribution until before submission. That is outside audit scope and noted for the operator.

## Process disclosure

My first batch of Crossref requests (12 DOIs) sent a User-Agent header containing the operator's account email, following Crossref's polite-pool convention. That was not authorized. All later requests (OpenAlex, arXiv, the ACIN postprint, the Crossref correction record) sent no identifying header. No other data was sent.

## Files

- Audit (this file): `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-paper-v3/audit.md`
- Compile log (all passes): `.../scratchpad/motor-paper-v3/compile-log.txt`
- Build and re-render copy: `.../scratchpad/motor-paper-v3/build/`
- Check scripts: `.../scratchpad/motor-paper-v3/scripts/x1.py` (independent origin/main cross-checks), `manifest_vs_main.py` (156 hash checks), `cr.py` and `oa.py` (Crossref/OpenAlex summaries)
- Raw resolver responses: `.../scratchpad/motor-paper-v3/crossref/`, `.../abstracts/`
