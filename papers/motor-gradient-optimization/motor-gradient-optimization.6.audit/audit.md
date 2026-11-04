# paper-audit: motor-gradient-optimization.6 (PR #1171 head 1dd8cdef)

Auditor: anvil:paper-audit (Claude Opus 5.5), 2026-10-17.
Audited tree: a clean `git archive 1dd8cdef papers`, extracted to
`/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-v6-audit/tree/`.
Worktree `/Volumes/Stripe/geode-fem/review-1171v4` was read only. HEAD is `1dd8cdefba886001ea70c43ca4c2b60523d5c53f` before and after, and `git status --short --ignored` is empty before and after.
origin/main = `89e6c4e1`. All four evidence commits (`581ee7ad`, `910593ba`, `224332c5`, `8acd21ec`) are ancestors of origin/main.

The verdict applies to this head only. It is not carried over from the v5 audit. Every v6 number was rechecked, and every number new or changed in v6 was recomputed from raw arrays by a new auditor script (`x3.py`).

Deviation from the command contract, at the operator's request: no `<thread>.6.audit/` sidecar was staged in the repo, and the pending-marker gate was run without `--write-review`. This single file replaces citation-audit.md, numerical-audit.md and flags.md. The compile log is in the scratchpad (see Files).

## Verdict

**PASS. Eligible for AUDITED.**

- Critical flags: **0**. Both v5 criticals (C1 SW3 rounding, C2 L1 Taylor mechanism) are fixed and correct against the records.
- Unresolved cites: 0. Bib-metadata defects: 0. Claim-support `does-not-support`: 0. `partial`: 0. The v5 `partial` (De Gersem & Weiland, "was developed by") is resolved.
- Non-critical notes: 1 recommended edit (N1, Fig. 5 caption "as well as the mean"); the rest are informational.
- Build: OK. Sequence: pdflatex, bibtex, pdflatex ×4; every pass exits 0. `.aux` from passes 4 and 5 is byte-identical. The final log has no undefined citations or references, no `Label(s) may have changed`, 0 overfull boxes and 0 underfull boxes. No `??` in the PDF. **18 pages** (v5: 17; the new Fig. 5 adds one). bibtex: 0 warnings. The regenerated `main.bbl` is identical to the committed one. The `pdftotext` of the rebuilt PDF is identical to that of the committed `main.pdf`.
- `verify.py` (`python3 -I -B`, run on the archive): **PASS**, exit 0, all four PASS blocks, including the new "v6 SW2/SW3 component rounding, distinct L0/L1 Taylor failure mechanisms, vector versus norm changes, all three size-normalized current policies, and matched full-current abstract claim".
- Evidence hashes: `evidence-manifest.json` is byte-identical to v5. All 115 snapshots are byte-identical (sha256) to `git show <source_commit>:<source>`. Against origin/main, 114 of 115 match. The exception is the same as in v5: `benchmarks/motor_reference/forward_harmonic/README.md`, a later update note that the README discloses.
- Regeneration: `render_v6.py` with the pinned NumPy 2.5.4, Matplotlib 3.11.2 and meshio 5.3.5 (run as the README states) reproduces **byte-identically** all 24 assets: 9 figure PDFs, 9 PNGs, 5 tables and `verified-numbers.json` (24 files compared, 0 differ). A full `diff -r` of the re-render copy against the archive is empty. None is stale.
- Pending-marker gate (`anvil.lib.pending_marker`): pass, 0 markers.
- Audience/link-hygiene (`anvil.lib.audience_check`): pass. `public_repo_url` is declared (`anvil_json`), with 0 unlinked artifact paths, 0 governance-vocabulary hits and 0 private-locator hits. All 8 `\href` targets return HTTP 200.
- Preflight: the one `numeric_consistency` finding (l.439, "0.27%") is the same heuristic false positive adjudicated in v5. 0.27% is the full-current GEODE vs finest-reference std (+0.2687%), not the cogging refinement pair.

## Critical flags (block AUDITED)

None.

## v5 findings: resolution check

| v5 finding | v6 text (line) | Recomputed from raw records | Status |
|---|---|---|---|
| C1: SW3 "17%" | "differ by 6.5\% and 16.5\% relative to conforming values" (705) | `study_L2_{h45,conforming}_initial.json`, `currents['1'].gradient.d_mean`: SW2 −47.297139 vs −44.426156 → **6.4624%**; SW3 +0.406420 vs +0.486690 → **16.4931%** | **fixed**; 6.5 and 16.5 are correct roundings |
| C2: L1 mechanism | "at L0 the remainder changes sign near the anchor, whereas at L1 remainder/$h^2$ is still drifting there. Their widest quadratic windows span two decades." (667-669) | `study_L{0,1}_conforming_initial.json`, `sweep_jinit` SW1, objective 1, remainder = J(p+h) − J(p) − h·adjoint. **L0:** remainder/h² −16.28 (3e-5), **−0.3794 (anchor 1e-5)**, +4.462 (3e-6): the sign changes at the anchor. **L1:** −25.47 (3e-5), **−9.647 (anchor)**, −4.432 (3e-6), −2.611, −2.065, −1.677, −3.184 (3e-8); first sign change at 1e-8. Neighbour ratios 2.640 / 0.4594, both outside the ×2 band; stored window 0 decades at both levels | **fixed**; matches the operator's (−25.5, −9.65, −4.43) |
| N2: "vector norm changes by about 1%" | "the box-scaled vector changes by only 1.19\% of its L2 norm; the norm itself decreases by 0.19\%" (698-699) | Perturbed h45 ripple sensitivities (`d_std`, scaled by the 9 box widths), L2 `study_L2_h45_perturbed` vs L3 `gradient_L3_h45_perturbed`: ‖g₃−g₂‖/‖g₂‖ = **1.188%** → 1.19%; ‖g₃‖/‖g₂‖ − 1 = **−0.190%** | **fixed**. The vector change is relative to the L2 norm, as stated. The existing l.707 range "0.46-1.19%" uses the L3 norm (1.190%); both round to 1.19, so they are consistent |
| De Gersem "was developed ... by" | "\citet{degersem2004harmonic} formulate harmonic weighting functions at the sliding interface of a finite-element machine model." (96-97) | The title is "Harmonic Weighting Functions at the Sliding Interface of a Finite-Element Machine Model Incorporating Angular Displacement". The abstract (OpenAlex, re-fetched anonymously) says "couples ... at a sliding interface in the air gap of a two-dimensional finite-element machine model using harmonic weighting functions" | **fixed**; `supports`. There is no origination claim. The wording is title-level and abstract-supported |

## Citation audit

There are 14 bib entries; 14 cited keys resolve and 14 entries render. `refs.bib` and `main.bbl` are byte-identical to v5. Every DOI was re-resolved today anyway. All 13 were fetched from `api.crossref.org/works/<doi>` **anonymously**: User-Agent `paper-audit/1.0`, no `mailto`, no identity in any header, URL or payload. All 13 return 200, and the parsed metadata (type, title, container, volume, issue, pages/article, dates, authors) is **identical** to the v5-audit Crossref responses. Arkkio 1987 has no DOI; its Aalto handle returns 200. Apart from the De Gersem sentence, the text around every citation is unchanged from v5. The v5 claim-support verdicts therefore stand, and they were re-read against the v6 lines below.

| Key | Metadata vs Crossref | Surrounding claim (v6 line) | Verdict | Notes |
|---|---|---|---|---|
| degersem2004harmonic | OK. IEEE Trans. Magn. 40(2):545-548, 2004-03, doi 10.1109/TMAG.2004.824616; De Gersem, Weiland (Crossref "DeGersem"; the bib spacing is correct) | "formulate harmonic weighting functions at the sliding interface of a finite-element machine model" (96-97) | **supports** | v5 `partial` resolved; see the table above. `citation-notes.md` gains a "V6 audit correction" section that accurately describes the change and claims no new retrieval |
| wiesheu2024spline | OK. ICEM 2024 pp. 1-6; 6 authors | reproduction target (48, 77-80) | supports | unchanged |
| wiesheu2025combined | OK. Optim. Eng. 26(2):1011-1038, 2025 | parameter + shape optimization with IGA (80-82) | supports | unchanged |
| scotzniovsky2025 | OK. Optim. Eng. 26(1):121-158, 2025 | adjoint sensitivities (60); FFD + mesh warping + adjoint EM (87-88) | supports | unchanged |
| merkel2021shape | OK. IEEE TEC 36(4):2683-2690, 2021 | IGA shape derivatives + harmonic coupling; validation with a separate solver (89-92) | supports | unchanged |
| bontinck2018isogeometric | OK. CMAME 334:40-55, 2018 | IGA subdomains, harmonic constraints, exact circles; stability vs multiplier space (98-109) | supports | unchanged |
| egger2022torque | OK. J. Math. Ind. 12, art. 6, 2022 | energetic torque; linear cogging, orders 39-129 vs order-200 reference (100-104, 267) | supports | unchanged |
| arkkio1987 | No DOI; Aalto handle 200 | gap-averaged stress torque (110, 236) | supports | unchanged |
| howe1992cogging | OK. IEEE Trans. Magn. 28(2):1080-1083, 1992 | cogging sensitive to discretization/evaluation (113-114) | supports | unchanged |
| arkkio2012proper | OK. SPEEDAM 2012 pp. 1456-1461 | mesh selection for torque (115) | supports | unchanged |
| ion2018robust | OK. Electr. Eng. 100(4):2635-2647, 2018 | gradient vs stochastic PMSM (117-119) | supports | unchanged |
| kemmetmueller2015 | OK. Mechatronics 32:22-33, 2015 | MEC torque control (123-124) | supports | unchanged |
| roomi2022 | OK. IET EPA 16(11):1286-1299, 2022 | waveform-targeting surrogate (127) | supports | unchanged |
| wen2023 | OK. JCP 484:112082, 2023 | on-the-fly reduced models along shape optimization (128-129) | supports | unchanged |

## Numerical audit

`verify.py` passes. In addition, every number in main.tex and the three `\input` tables was recomputed by my own scripts. They read the hash-verified snapshots and share no code with verify.py or the renderer:
- `x1.py` and `x2.py` are the v5 auditor scripts, rerun on the v6 data tree. Their outputs are **byte-identical** to the v5 outputs, as expected: the evidence is byte-identical, and every v5 number not touched by the diff appears unchanged in v6. All rows of the v5 audit's numerical tables therefore hold for v6. Only the line numbers shift: +2 from l.96, +20 after the moved Fig. 4 at l.444, and about +25 from §6.
- `x3.py` is new for v6 and covers every number added or changed in v6, plus the Fig. 5 data.

Rounding convention (unchanged): values introduced by "within", "at most" or "less than" are read as true upper bounds. On that reading, "within 1.3e-9" (l.277; raw 1.2417e-9) is correct and intentional (operator fixed point), as are 9.8e-13, 8.9e-7, 1.1e-4, 5.4e-5 and 9e-5. "Within 0.1%" in the new abstract clause is read the same way (raw −0.096%). Every other number was checked as a rounded display value.

### Numbers new or changed in v6

Initial full-current mean (L3, order 45, coupling torque, 20 angles) = 1.4432576. Opt120 size factor `torque_factor` = 0.878175028 = s²L/0.1 from the record scalars (exact match).

| Text claim (line) | Recomputed (raw precision) | Match |
|---|---|---|
| Fixed unscaled, both size factors removed: 1.4971 N m, 3.7% above (529-530; conclusion 803) | 1.3146990 / 0.878175 = **1.4970808**; +3.7293% | yes |
| **Fixed physical, normalized: 1.4291 N m, 1.0% below** (530-531; conclusion "−1.0%" 803) | 1.2550197 / 0.878175 = **1.4291225**; −0.9794% | yes |
| **Density rule, normalized: 1.7061 N m, 18.2% above** (532) | 1.4982949 / 0.878175 = **1.7061461**; +18.2149% | yes; see the next row |
| (reviewer's 1.7062) | From 4-dp inputs, 1.4983 / 0.878175 = 1.7061520 → 1.7062. At raw precision the value is 1.7061461 → **1.7061**. The changelog's claim that v6 "corrects the review's 1.7062 rounded-input calculation" is **correct**. The margin is thin (4.6e-6 below the rounding boundary), but 1.7061 is the right display value | yes |
| **Length-only, fixed unscaled: 1.6434 N m (+13.9%), retaining s² = 1.098** (532-534) | 1.3146990 × 0.1 / 0.08000054 = **1.6433626**; +13.8648%; ScaleR² = 1.047717² = **1.0977114** → 1.098. Consistency check: 1.6433626 / 1.0977114 = 1.4970808, the both-factors value, exactly | yes |
| Saved-size means 8.9% and 13.0% below, density 3.8% above (520-522; unchanged) | −8.9075%, −13.0426%, +3.8134% | yes |
| Conclusion: "removing both explicit size factors puts the two fixed-current means within −1.0% to +3.7% of the initial mean. Most of their saved-size deficit therefore comes from the size factor, although the normalized fixed-physical-current value remains lower." (801-805) | Fixed unscaled −8.91% → +3.73%; fixed physical −13.04% → −0.98%. In log terms the size factor (ln 0.878175 = −0.130) is 93% of the fixed-physical deficit (ln 0.86957 = −0.140) and more than all of the fixed-unscaled one. "Most" is correct; "remains lower" is correct (−0.98%) | yes |
| **Abstract: "At matched reference current, the saved design's full-current mean agrees within 0.1%, but its ripple is 2.0 times the saved reference value."** (29-31) | `opt120_export_L3_h45_reference.json` vs `reference_data.json` optimized arrays, full current: mean 1.4985684 vs 1.5000117 → **−0.0962%** (within 0.1%); std 3.52755e-4 vs 1.75860e-4 → **×2.0059** (population or sample std give the same ratio). Half current: std ratio 1.264, mean −0.154%. The clause is correctly scoped to full current; it would be false at half current | yes |
| Zero current "rule-independent and taken from the fixed-current record" (547-548) | Only `opt120_export_L3_h45_fixed.json` carries fraction 0 (fractions 0, .25, .5, .75, 1); the fixed-physical, jinit and reference records carry only 0.5 and 1.0. All three rule definitions scale the current by f, so f = 0 is identical under every rule | yes |
| "changes in F, normalized by L3, reach 1.36% for the initial design and 2.71% for Opt120 ... on the exported mesh family" (504-505) | 1.356% (initial, h45), 2.708% (Opt120, h45) | yes; see N3 for the family label |
| "An alternate freshly constructed mesh family changes by 3.22% for Opt120 from L2 to L3" (508-509) | `opt120_construction_L{2,3}_h45_fixed`: 3.217% | yes |
| SW3 16.5%, L1 drift, 1.19%/−0.19% | see the resolution table above | yes |

### Fig. 5 (new in the manuscript; `figures/opt120-waveforms.pdf`)

The figure is byte-identical to the v5 supplement and regenerates byte-identically. Data recomputed from `opt120_export_L3_{h45,h93,conforming}_fixed.json`, fraction 1:

| Model (estimator) | n, grid | mean (N m) | min / max (N m) | peak-to-peak (mN m) | std (N m) | statuses |
|---|---|---|---|---|---|---|
| order 45 (`T_coupling`) | 40, 0:0.5:19.5 | 1.3146990 | 1.31312 / 1.31545 | 2.32 | 6.597e-4 | all tolerance |
| order 93 (`T_coupling`) | 40, 0:0.5:19.5 | 1.3146643 | 1.31085 / 1.31710 | 6.25 | 1.6363e-3 | all tolerance |
| conforming (`T_arkkio_outer`) | 40, 0:0.5:19.5 | 1.3146397 | 1.31091 / 1.31729 | 6.38 | 1.6447e-3 | all tolerance |

Caption check (566-571):
- "Opt120 full-current torque on fresh L3 meshes at 40 angles, with fixed unscaled current": correct (fresh-export family, `current_rule: fixed`, 40 angles at 0.5°).
- "Harmonic orders 45 and 93 use coupling torque; conforming coupling uses outer-band Arkkio": matches the renderer and the records.
- "Left: full torque, retaining each model's mean. Right: each curve minus its own 40-angle mean": matches the renderer (`y` and `1000*(y-y.mean())`, mN m).
- "The interface change alters the resolved ripple as well as the mean": the ripple part is correct (std ×2.48, p2p 2.3 → 6.3 mN m). The **mean part is overstated**; see N1.

The figure numbering is sequential: Fig. 4 (refined-validation, p.9), Fig. 5 (opt-waveforms, p.11), then design gradients as Figs. 6-7. There are no hard-coded figure numbers; the one hard-coded "Section 5.1" became `Section~\ref{sec:coupling-results}`, which resolves to 5.1.

## Scope: v5 → v6 diff

Compared: `motor-gradient-optimization.5/` vs `.6/` in the archive, plus `git diff 76d78e1f 1dd8cdef` for the whole tree.

**Unchanged byte-for-byte:** all figures (PDF and PNG), all tables, `refs.bib`, `main.bbl`, `evidence-manifest.json`, `verified-numbers.json`, `anvil-paper.cls` and all evidence snapshots. `render_v5.py` was renamed to `render_v6.py`; the only changes are the docstring and the PASS message.

**main.tex changes, each mapped to its source:**

| Change (v6 line) | Source |
|---|---|
| "neither spatial-refinement sequence" (25, 796) and "Newton-converged" (31) | Opus m5. The changelog states both halves of the disambiguation |
| Abstract matched-current clause (29-31) | Fable m4 |
| De Gersem sentence (96-97) | v5 audit non-critical 1 / Opus m3 |
| `\label{sec:coupling-results}` and `\ref` (322, 442) | Fable m2 |
| Fig. 4 moved after §5.4's first citing paragraph, `[!t]` → `[!htb]`; caption text unchanged | Fable m1 |
| Fig. 5 added, with its new caption, and cited in §5.5 (500-501, 564-573) | Fable M1 / Opus m4 |
| §5.5 paragraph split in two places | Fable m3 |
| "on the exported mesh family" (505) | Opus nit (L480-482) |
| "for Opt120" added to the alternate-family 3.22% (509) | Opus nit (same item); clarification only |
| Normalized fixed-physical 1.4291 / density 1.7061; length-only +13.9%, s² = 1.098 (530-534) | Fable M2, m6 |
| Zero-current source parenthetical (547-548) | Fable m5 |
| Taylor mechanism (667-669) | v5 audit C2 |
| Vector change vs norm change (698-699) | v5 audit N2 / Opus m1 |
| 17% → 16.5% (705) | v5 audit C1 |
| Discussion opening: three-line restatement removed (old 762-764) | Fable m8 |
| "All in-run costs" (779) | Opus m2 |
| Conclusion size/current sentence rewritten (801-805) | Fable M2 (conclusion part). It is more conservative than Fable's suggested wording ("most", and "remains lower") |

**Verdict: no main.tex or figure change goes beyond the v5 audit corrections and the advisories in review-opus.md / review-fable.md. No re-review is required on scope grounds.** The changelog's declined items (Fable m7, m9, m10; Opus paired-estimator nit, Fig. 3 nit, compact-table suggestion) produce no text change. Two pieces of new prose were not seen by either reviewer: the Fig. 5 caption and the conclusion's "Most of ... although ..." sentence. Both implement requested advisories and were audited here; the caption has one overstatement (N1).

**Outside the manuscript** (informational, not manuscript scope): `BRIEF.md` and `EXPERIMENTS.md` gain project-status updates. These state that #1196 merged at `89e6c4e1`, and that the #1198 development-host pilot is running, with 12 of 36 runs complete and 6 failed per Squad message 525. Both files label this as non-paper evidence, and none of it appears in main.tex. `.anvil.json` `max_iterations` goes from 5 to 6 (disclosed in the changelog), with `artifact_verify.cwd` moved to `.6`. `papers/README.md` points to `.6`. The archived `.5.review/review-{opus,fable}.md` and `.5.audit/audit.md` are byte-identical to the originals in `/Volumes/Stripe/scratch-codex-review/motor-paper-v5-reviews/`.

## Flags summary

### Critical flags (block AUDITED)

None.

### Non-critical notes

1. **N1, Fig. 5 caption overstates the mean effect** (main.tex:571). "The interface change alters the resolved ripple as well as the mean." Across the three models the full-current 40-angle means differ by at most 5.9e-5 N m (order 93: −3.5e-5, −0.0026%; conforming: −5.9e-5, −0.0045%, relative to order 45). That is about 1/100 of the ripple peak-to-peak, and no offset is visible in panel (a); panels (a) and (b) look the same apart from the y-offset. The conforming difference is also smaller than the 1.3e-4 N m outer-band vs coupling estimator difference reported at l.484-485, so it is not resolved as an interface effect. The statement is literally true for order 45 vs 93, which use the same estimator, so this is not a critical flag. Recommended replacement: "The interface change alters the resolved ripple; the three means agree within $6\times10^{-5}$\,N\,m." Alternatively, drop "as well as the mean". `verify.py` does not guard this sentence.
2. **N2, 1.7061 margin** (532): the raw value 1.7061461 is 4.6e-6 below the 1.70615 rounding boundary. The display is correct, and `verify.py` asserts it from the raw arrays. Informational.
3. **N3, "exported mesh family" for the initial design** (505): the initial-design records are `mesh_family: nurbs-multipatch`, built from the reference's NURBS exports (l.152, "the initial NURBS exports"), while Opt120's are `fresh-export`. The label is consistent with l.152 and with Opus's nit, so no action is needed. "On the exported meshes" would be marginally clearer.
4. **Snapshot vs origin/main**: one of 115 snapshots differs from origin/main, unchanged from v5 and disclosed. No action.
5. **Renderer under `python -I`**: `render_v6.py` imports the sibling `refinement_data.py`. It works as the README invokes it. Informational, unchanged from v5.

### Outstanding dependencies

None (0 pending markers).

### Audience-fit notes

None (0 governance-vocabulary hits, 0 private-locator hits). The author line reads "Robb Walters" (sole author), as intended.

## Process disclosure (network)

Every request was anonymous, with User-Agent `paper-audit/1.0`, no `mailto`, and no email, name or identity in any header, URL or payload:
- Crossref `api.crossref.org/works/<doi>` (13)
- OpenAlex `api.openalex.org/works/doi:10.1109/tmag.2004.824616` (1)
- aaltodoc handle (1)
- GitHub tree URLs for the 8 `\href` targets (8)

No gh CLI call was made in this audit. One `git fetch -q origin main` was run against the worktree's repository to confirm origin/main. It updates only the shared remote-tracking ref, not the worktree's files, HEAD or status; origin/main stayed `89e6c4e1`.

## Files

- Audit (this file): `/Volumes/Stripe/scratch-codex-review/motor-paper-v6-reviews/audit.md`
- Scratch root: `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-v6-audit/`
  - `tree/`: clean `git archive 1dd8cdef papers`
  - `build/`: compile copy; `compile-log.txt` and `final-pass.log` alongside; `main.txt` (rebuilt PDF text), `committed.txt`
  - `render/`: re-render copy (24 byte-identical outputs); `render.out`
  - `scripts/x1.py`, `x2.py` (v5 recomputation, rerun), `x3.py` (v6 recomputation), `hashes.py`, `cr.py`, `build.sh`, `render.sh`, `fetch.sh`
  - `x1.out`, `x2.out` (byte-identical to v5), `x3.out`
  - `crossref/`, `abstracts/`: raw resolver responses; `cr-v5.out` / `cr-v6.out` (identical)
