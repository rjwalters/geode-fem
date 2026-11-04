# paper-audit: motor-gradient-optimization.5 (PR #1171 head 76d78e1f)

Auditor: anvil:paper-audit (Claude Opus 5.5), 2026-10-17.
Audited tree: a clean `git archive 76d78e1f papers`, extracted to
`/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-v5-audit/tree/`.
Worktree `/Volumes/Stripe/geode-fem/review-1171v4` was read only. HEAD is `76d78e1f373b20d0c9c7920582b5a0ddd71082c2` before and after, and `git status --short --ignored` is empty before and after.
origin/main = `89e6c4e1`. All four evidence commits (`581ee7ad`, `910593ba`, `224332c5`, `8acd21ec`) are ancestors of origin/main.

The verdict applies to this head only. It is not carried over from the v3 audit; every v5 number was rechecked from scratch.

Deviation from the command contract, at the operator's request: no `<thread>.5.audit/` sidecar was staged in the repo. This single file replaces citation-audit.md, numerical-audit.md and flags.md. The compile log is in the scratchpad (see Files).

## Verdict

**FAIL. READY-WITH-AUDIT-FLAGS; not eligible for AUDITED.**

- Critical flags: **2** (both numerical/evidence inconsistencies; both are one-line text fixes).
- Unresolved cites: 0. Bib-metadata defects: 0. Claim-support `does-not-support`: 0. Claim-support `partial`: 1 (De Gersem & Weiland 2004, "developed by"; see below). This is non-critical by contract but should be fixed.
- Build: OK. Sequence: pdflatex, bibtex, pdflatex ×4; every pass exits 0. `.aux` from passes 4 and 5 is byte-identical. The final log has no undefined citations or references, no `Label(s) may have changed`, 0 overfull boxes and 0 underfull boxes. No `??` in the PDF. 17 pages. bibtex: 0 warnings. The regenerated `main.bbl` is identical to the committed one.
- `verify.py` (`python3 -I -B`, run on the archive): PASS, exit 0, all three PASS blocks.
- Evidence hashes: all 115 manifest snapshots are byte-identical (sha256) to `git show <source_commit>:<source>`. Against origin/main, 114 of 115 match. The exception is `benchmarks/motor_reference/forward_harmonic/README.md`, a later update note that the v5 README discloses ("Snapshots match their pinned source commits, not necessarily a later main branch").
- Regeneration: `render_v5.py` with the pinned NumPy 2.5.4, Matplotlib 3.11.2 and meshio 5.3.5 reproduces **byte-identically** all 9 figure PDFs, all 9 PNGs, all 5 tables and `verified-numbers.json`. None is stale. Note: the renderer imports a sibling module (`refinement_data`), so it fails under `python -I`. It runs as written in the README.
- Pending-marker gate (`anvil.lib.pending_marker`): pass, 0 markers.
- Audience/link-hygiene (`anvil.lib.audience_check`): pass. `public_repo_url` is declared (`anvil_json`), with 0 unlinked artifact paths, 0 governance-vocabulary hits and 0 private-locator hits. All 8 `\href` targets return HTTP 200.

## Critical flags (block AUDITED)

### C1. Numerical inconsistency: SW3 component difference "17%" (main.tex:679-680)

Text: "the initial SW2 and SW3 mean components differ by 6.5\% and 17\% relative to conforming values."
Context: L2, full fixed current, initial design (the paragraph's comparison state; the L3 figures follow at l.681).
Source: `gradient/runs/study_L2_h45_initial.json` and `study_L2_conforming_initial.json`, `currents['1'].gradient.d_mean`:
- SW2: h45 −47.29714, conforming −44.42616, |Δ|/|conf| = **6.462%**, which displays as 6.5%. Matches.
- SW3: h45 +0.406420, conforming +0.486690, |Δ|/|conf| = **16.493%**, which **rounds to 16%, not 17%**.

The other plausible readings do not give 17% either: |Δ|/|h45| = 19.75%, and L3 gives 13.5%. Box scaling cancels in the ratio. The sentence first appeared in v4 (l.627), which was never audited. `verify.py` does not guard it.
Fix: "6.5\% and 16\%", or "6.5\% and 16.5\%" if three significant figures are preferred.

### C2. Text contradicts the record: Taylor-window failure mechanism at L1 (main.tex:643-645)

Text: "An anchored-window diagnostic fails for the initial-design, full-current conforming/fixed-density SW1 ripple cases at L0 and L1 because the remainder changes sign near the anchor; their widest quadratic windows span two decades."

Recomputed from `study_L{0,1}_conforming_initial.json`, `currents['1'].sweep_jinit[SW1]`, objective 1 (std), with remainder = J(p+h) − J(p) − h·adjoint:
- **L0: correct.** remainder/h² is −0.379 at h = 1e-5 (the anchor) and +4.46 at h = 3e-6, so the sign changes right at the anchor.
- **L1: not as stated.** remainder/h² is −25.5 at 3e-5, **−9.65 at 1e-5 (anchor)**, −4.43 at 3e-6, −2.61 at 1e-6, and stays negative down to 3e-8. The first sign change is at h = 1e-8, three decades below the anchor, in the round-off regime. The anchored window has zero width because the neighbour ratios are 2.64 and 0.459, both outside the factor-of-two band. In other words, the remainder is pre-asymptotic at the anchor (remainder/h² still drifting), not changing sign.
- The counts are correct: exactly these two cases fail, 530/592 and 388/592 hold, and the widest window anywhere is 2.0 decades for both. Only the stated cause is wrong for L1.

The sentence has been carried since v2 (l.496) and was not caught by the v3 audit, which checked only the count.
Fix (wording only), e.g.: "...at L0 and L1: at L0 the remainder changes sign at the anchor, and at L1 remainder/$h^2$ is still drifting there; their widest quadratic windows span two decades."

## Citation audit

There are 14 bib entries; 14 cited keys resolve and 14 entries render. All 13 DOIs were fetched from `api.crossref.org/works/<doi>` **anonymously**: User-Agent `paper-audit/1.0`, no `mailto`, no identity in any header, URL or payload. All 13 resolve. Arkkio 1987 has no DOI; its Aalto handle resolves (301 to the item record cited in citation-notes.md). `<thread>/refs/` holds screening metadata only, so claim support was judged from abstracts and arXiv full texts retrieved during this audit.

| Key | Metadata vs Crossref | Surrounding claim (line) | Verdict | Notes |
|---|---|---|---|---|
| degersem2004harmonic (**new**) | OK. IEEE Trans. Magn. 40(2):545-548, 2004-03, doi 10.1109/TMAG.2004.824616; authors De Gersem, Weiland. Crossref has "DeGersem"; the bib spacing "De Gersem" is correct, and the PDF renders "H. De Gersem and T. Weiland". | "Harmonic weighting of a sliding interface was **developed** for finite-element machine models by De Gersem and Weiland (2004)" (94-95) | **partial** | See "De Gersem wording" below. The topic is supported; the origination verb is not. |
| wiesheu2024spline | OK. ICEM 2024 pp. 1-6; 6 authors in order (Crossref ASCII-folds "Schops") | reproduction target (46, 75-78) | supports | unchanged from v3 |
| wiesheu2025combined | OK. Optim. Eng. 26(2):1011-1038, 2025-06 (online 2024-09-21); 6 authors | combines parameter + shape optimization with IGA, compares each alone (78-80) | supports | unchanged |
| scotzniovsky2025 | OK. Optim. Eng. 26(1):121-158, 2025-03 (online 2024-05-22); 7 authors | adjoint gives many sensitivities (56-58); FFD + differentiable mesh warping + EM analysis with adjoints (85-86) | supports (title-level, plus the author-verified publisher abstract) | unchanged; no abstract is retrievable anonymously |
| merkel2021shape | OK. IEEE TEC 36(4):2683-2690, 2021-12; Merkel, Gangl, Schöps | IGA shape derivatives + harmonic coupling to reduce EMF THD (87-88); **new:** "validation compares initial and optimized designs with a separate field solver" (89-90) | supports | arXiv 1908.06009 §VII "Validation": "The results of the optimization in Section VI is validated by using JMAG as reference solution", with initial (iter 0) and optimized (iter 59) designs and linear and nonlinear laws. |
| bontinck2018isogeometric | OK. CMAME 334:40-55, 2018-06; 4 authors | separate IGA subdomains, harmonic constraints, exact circles (96-98); **new:** stability as multiplier space grows relative to field space (105-107) | supports | arXiv 1709.05301 §3.4 and Fig. 7: "when the spatial discretization is not fine enough (or, roughly said, the number of degrees of freedom is too low compared to the number of coupling harmonics), the saddle-point problem becomes unstable". |
| egger2022torque | OK. J. Math. Ind. 12(1), article 6, 2022 (online 2022-01-31); 5 authors | energetic torque from harmonic mortar (98-100); **new:** linear-material cogging, orders 39 to 129 vs refined order-200 reference (100-102); Eq. 8 consistent with (265) | supports | arXiv 2112.05572 §5: iron μr = 500 (linear); cogging; N = 39/69/99/129; "reference solution is computed on a refined mesh with p = 2 ... and with trigonometric degree N = 200". |
| arkkio1987 | No DOI; the Aalto handle resolves; Helsinki Univ. of Technology, 1987 | gap-averaged stress torque (108, 234) | supports (standard attribution) | unchanged |
| howe1992cogging | OK. IEEE Trans. Magn. 28(2):1080-1083, 1992-03; Howe, Zhu (Crossref order) | cogging prediction sensitive to discretization/evaluation method (111-112) | supports | unchanged |
| arkkio2012proper | OK. SPEEDAM 2012 pp. 1456-1461; Arkkio, Hannukainen. The bib booktitle drops Crossref's duplicated "Power Electronics", which is correct. | mesh selection for torque (113) | supports | unchanged |
| ion2018robust | OK. Electr. Eng. 100(4):2635-2647, 2018; 8 authors in order | gradient vs stochastic, PMSM, no remeshing (115-117) | supports | unchanged |
| kemmetmueller2015 | OK. Mechatronics 32:22-33, 2015; 3 authors | MEC torque control incl. saturation/nonsinusoidal (121-122) | supports | unchanged (ACIN postprint) |
| roomi2022 | OK. IET EPA 16(11):1286-1299, 2022; 3 authors | waveform-targeting surrogate (125-126) | supports | unchanged |
| wen2023 | OK. JCP 484:112082, 2023; Wen & Zahr | on-the-fly reduced models along shape optimization, fluid examples (126-127) | supports | unchanged |

### De Gersem wording (flagged as requested)

**Still worded "was developed ... by" (main.tex:94-95).** The v5 package declares title-level support only (`citation-notes.md`: "Full text of the 2004 paper was not retrieved; no stronger priority, implementation or performance claim is made"). "Developed for finite-element machine models by X" is a priority/origination claim.

During this audit I retrieved the abstract anonymously from OpenAlex. It moves support to abstract level, but it still does not support origination:

> "The formulation presented in this paper couples the magnetic vector potential and the tangential component of the magnetic field strength at a sliding interface in the air gap of a two-dimensional finite-element machine model using harmonic weighting functions which enable the use of fast Fourier transforms in combination with a simple, diagonal operator to account for the angular displacement between stator and rotor. The method applies to models where the stator and rotor meshes are nonequidistant and nonmatching at the interface. ..."

The abstract describes the paper's own formulation. It makes no claim to be the first, and it does not survey prior sliding-interface methods.

Verdict: `partial` (topic supported, verb overreaches). Abstract-supported replacement:

> "\citet{degersem2004harmonic} couple stator and rotor at a sliding air-gap interface of a two-dimensional finite-element machine model with harmonic weighting functions, allowing nonmatching interface meshes."

Opus's "formulate harmonic weighting functions at the sliding interface" (review m3) is also acceptable. Either way, drop "was developed".

## Numerical audit

`verify.py` passes. In addition, every number in main.tex and the three `\input` tables was recomputed by my own scripts (`scripts/x1.py`, `scripts/x2.py`). They read the hash-verified snapshots and share no code with verify.py or the renderer.

Rounding convention: values introduced by "within", "at most" or "less than" are read as true upper bounds. On that reading, the following are correct as written and need no action: 1.3e-9 (raw 1.2417e-9, the operator's fixed point), 9.8e-13 (raw 9.734e-13), 8.9e-7 (raw 8.848e-7), 1.1e-4 (raw 1.018e-4), 5.4e-5 (raw 5.33e-5) and 9e-5 (raw 8.84e-5). Every other number was checked as a rounded display value.

### Forward reproduction (sections 3-5.3)

| Text claim (line) | Recomputed | Match |
|---|---|---|
| Order 45 vs conforming, L3: loaded std −7–9%, cogging std +59% (20-22, 46-48, 365-367, 770-772) | −7.42%, −8.99%; +59.19% | yes |
| Full-order identity: within 4.1e-9 (L0), 1.3e-9 (L1); "a few 1e-9" (23-24, 273-275) | 4.0568e-9 (60 orders), 1.2417e-9 (120 orders) | yes (1.3e-9 is a bound) |
| Loaded mean of either model within 0.16% of reference (48-49) | conforming −0.153/−0.156%; harmonic −0.002/+0.051% | yes |
| Radii 16/44/45/67.5 mm, L = 0.1 m (135-137) | reference RD2/2, RD1/2, SD2/2, SD1/2 = 16, 44, 45, 67.5 mm | yes |
| Winding areas ≈36.29, 31.22 mm²; 16% (156, 169) | exact 36.2912, 31.2151; ratio 1.162 | yes |
| I* = 9.677657374 A (164) | 9.677657374172274 | yes |
| Hc = Br/(μ0 μr) (188-189) | 1.4/(μ0·1.05) = 1.061033e6 = record `magnet_hc_A_per_m` | yes |
| 500 + 1,140 solves, all at tolerance (206-207) | 500, 1140; statuses {tolerance} | yes |
| 47 B-H points 0-2.30 T; H/B non-monotone to 0.75 T; round-off 0.05-2.25 T; 6.66%; 2.17e-4 (209-221) | 47, 0.0-2.3; last decrease at 0.75; interior max 4.2e-16; 0.06659; 2.1706e-4 | yes |
| 16 constraints, orders 3..45 (249-251) | 8 orders [3, 9, ..., 45] | yes |
| Nodes 3,153 / 9,937 / 36,083 / 139,268; arc 120·2^ℓ (292-293) | same; n_coupling 120/240/480/960 | yes |
| L3 means −0.15/−0.16%; harmonic −0.002/+0.051%; ripple +9.9/+11.3 → +1.7/+1.3 (322-326); Table 1 cells | −0.1532/−0.1563; −0.0024/+0.0511; 9.858/11.266 → 1.703/1.267; all 27 cells match | yes |
| Reference means inside harmonic L3 intervals (324-325) | half [0.661494, 0.662632] ∋ 0.662079; full [1.44228, 1.44416] ∋ 1.44248 | yes |
| Max full-current gap 0.0272 → 0.0034 (338) | 0.027192 → 0.003390 | yes |
| Full ripple interval contains ref; half ref ≈5e-5 below its interval (339-340) | full [0.079140, 0.081571] ∋ 0.079350; half 4.61e-5 below | yes |
| Cogging −29.1% → +12.9%; 1.13-fold scaling; ref outside interval (355-358) | −29.088 → +12.887; LS scale 1.1287; interval [0.011905, 0.012278] vs 0.010711 | yes |
| Harmonic cogging std L0-L3 0.0101/0.0119/0.0121/0.0121; ref 0.0107 (358-360) | 0.01014, 0.01187, 0.01206, 0.01209; 0.01071 | yes |
| Order 189 within 1.1e-4 at L1, L2 (367-368) | 1.018e-4, 6.63e-5 | yes (bound) |
| Coupling and half-band Arkkio within 9e-5 of full band; free ends < 5.4e-5 (386-389) | 8.53e-5, 8.84e-5, 8.84e-5; 5.33e-5 | yes |
| Table 2: 2.1e-6 (L2, L3), 7.3e-8 (L2; L3 full current), 3.9e-4 (L2), 7.3e-5 (L3) | 2.062e-6; 7.269e-8 (L3 record = current index 2 only); 3.865e-4; 7.304e-5 | yes |

### Independent reference refinement (section 5.4)

| Text claim (line) | Recomputed | Match |
|---|---|---|
| 3×3 reproduces all 60 saved torques, at most 9.8e-13 (424-425) | 9.734e-13 | yes (bound) |
| DOFs 4,079 / 12,518 / 43,220 / 159,920 (428-429) | rotor + stator ndof: same | yes |
| Cogging std 0.01071 → 0.01220, 0.01212, 0.01208; GEODE 0.01209, 0.10% (431-433; abstract 25; conclusion 774) | 0.010711, 0.012203, 0.012123, 0.012079; 0.012091; +0.1019% | yes |
| Max pointwise cogging gap 2.1e-3 → 3.4e-5 (434-435) | 2.114e-3 → 3.409e-5 | yes |
| Finest ref std 0.03947 / 0.08014; GEODE +0.35% / +0.27% (435-437) | 0.039474 / 0.080140; +0.3488% / +0.2687% | yes |
| All five finest-ref statistics inside GEODE L3 intervals (437-439) | 5/5 True | yes |
| Last cogging increments ≈ halve; limit near 0.0120 (442-444) | −7.99e-5, −4.43e-5 (ratio 0.55); geometric extrapolation ≈0.01203 | yes |
| Last two half-current std increments both ≈ −0.3% (444-445) | −0.334%, −0.302% | yes |
| Newton 1e-6 → 1e-14 changes cogging by at most 4.4e-8 (449-451) | 4.398e-8, 4.283e-8, 4.255e-8 (s3/s6/s12) | yes |

### Opt120 validation (section 5.5); new or changed in v5

| Text claim (line) | Recomputed | Match |
|---|---|---|
| 12 scalars, ten grouped offsets; moved mesh has four inverted triangles (457-460) | 12 scalars; `offsets_m` has 10 groups; `mesh_quality.inverted` = 4, status `invalid_mesh` | yes |
| Paired outer-band vs coupling, L3 h45 Opt120 full current: ≈1.3e-4 mean, 1.1e-5 std (464-466) | 1.317e-4, 1.087e-5 | yes |
| F at h45 → h93: ×3.6, 1.07e-6 → 3.86e-6 (473-475; abstract 26-27; 739, 777) | 1.07075e-6 → 3.86348e-6; ×3.608 | yes |
| Initial F +16% (475-476; abstract 28) | +16.03% | yes |
| Ratios 7,631 / 2,454 / 2,444; ">2,400-fold" (476-477; abstract 29; conclusion 777-778) | 7631.49, 2454.03, 2444.39 | yes |
| Opt120 full std 0.00066 → 0.00164 (477-478) | 6.595e-4 → 1.636e-3 | yes |
| L2→L3 F change max 1.36% initial, 2.71% Opt120, across three models (480-482) | max |·|: 1.356% (h45), 2.708% (h45) | yes |
| Opt120 h45 increments 3.05e-8, 2.90e-8 (483-485) | 3.0525e-8, 2.9001e-8 | yes |
| Alternate construction family L2→L3 3.22% (485-486) | 3.217% | yes |
| Interface effect 2.79e-6, ≈96× last increment (487-488) | 2.7927e-6; 96.30 | yes |
| Opt120 cogging std 0.00061 → 0.00035 (492) | 6.110e-4 → 3.467e-4 | yes |
| Initial mean 1.4433; Opt120 1.3147 / 1.2550 / 1.4983; −8.9 / −13.0 / +3.8% (494-499) | 1.44326; 1.31470 / 1.25502 / 1.49829; −8.908 / −13.043 / +3.813% | yes |
| 100 → 80 mm; s = 1.0477; SD1 130.293 mm; −11.646° (500-503) | LENGTH 80.0005 mm; 1.0477172; 130.2931; −11.64627 | yes |
| Size factor s²L/0.1 = 0.878175 (503-504) | record `torque_factor` 0.878175 = s²L/0.1 | yes |
| Both factors removed 1.4971 N m, +3.7%; length only 1.6434 (505-507) | 1.49708, +3.729%; 1.64336 | yes |
| Matched reference current: mean 1.4986 vs 1.5000; std 3.53e-4 vs 1.76e-4, ×2.0; half 5.53e-4 vs 4.37e-4 (512-517) | 1.49857 vs 1.50001; 3.5276e-4 vs 1.7586e-4, ×2.006; 5.530e-4 vs 4.374e-4 | yes |
| Max loaded pointwise gap ≈2.1e-3 (518-519) | 2.058e-3 | yes |
| Matched-policy ratios 10,170 (GEODE), 11,982 (saved) (519-521) | 10169.6, 11981.6 (zero current from the fixed record, which is rule-independent) | yes |
| Ten-angle vs 40-angle: +1.05% (h45), −5.90% (h93), −17.1% (conforming) (525-528) | +1.050%, −5.896%, −17.107% | yes |
| Withheld currents 0.25, 0.75; shifted 20-angle grids (528-529) | fractions {0, 0.25, 0.5, 0.75, 1}; 40-angle records at 0.5° step | yes |
| 3,240 rows in 18 successful records, all at tolerance (531-532) | 18 files (moved excluded), 3240 rows, {tolerance} | yes |
| Table 4 (design points incl. new Opt120 column) | all 36 cells match the record values at 4 dp | yes |

### Design gradients (section 6)

| Text claim (line) | Recomputed | Match |
|---|---|---|
| SW1 mean derivative +1.68 (fixed I) / −220.85 (fixed density), L1 h45 initial full (590-592) | +1.68373 / −220.85203 | yes |
| Policy changes 7 of 10 geometric/scale derivatives; reverses SW1, SW4, SD1; MAG/MH/MW identical (595-598; abstract 34-36) | changed: SW1, SW2, SW3, SW4, SD1, Sr1, ScaleR (7); reversed: SW1, SW4, SD1; rotor identical | yes |
| 800 records, 96 duplicates, 704 distinct; 688 ≤ 2.8e-7; max 1.07e-5, Sr1 ripple (611-617; abstract 30-32: 1.1e-5) | 800 / 96 / 704; 688; 1.0668e-5 (`study_L0_h45_initial`, a = 0.5, Sr1 std) | yes |
| 530 / 592 ≥ 4 decades, 388 ≥ 5; anchor 1e-5 m / 1e-2° (638-643) | 592 / 530 / 388; h_ref = 1e-5 (×1000 for degrees) | yes |
| Two anchored failures L0/L1 conforming fixed-density SW1 ripple; widest window 2 decades (643-645) | exactly these two; widest 2.0 / 2.0 | counts yes; **mechanism wrong at L1 (C2)** |
| Zero-current mean best errors reach 7.0e-4; cogging std errors at most 8.9e-7 (645-648) | 7.040e-4; 8.848e-7 | yes |
| Harmonic SW3 sign change, increments shrink (651-653) | −0.784, −0.099, +0.406, +0.642 (increments 0.685, 0.505, 0.236) | yes |
| Conforming SW2 −46.5, −41.3, −44.4, −47.2; harmonic −49.8 → −47.1 (654-657) | −46.499, −41.279, −44.426, −47.246; −49.781, −48.057, −47.297, −47.110 (monotone) | yes |
| Remesh MW fit gap 2.6e-6; scatter ≈6e-4 at h ≥ 1e-4; SW1 ripple 3.8% (658-662) | 2.60e-6; 6.34e-4; 3.80% (largest) | yes |
| L2 MAG ripple derivative 1.28 vs 1.94 (initial), 0.75 vs 1.87 (perturbed) (667-670) | 1.2832 / 1.9371; 0.7534 / 1.8655 | yes |
| L3 MAG 1.22 vs 1.87; 0.57 vs 1.67 (671-672) | 1.2173 / 1.8671; 0.5688 / 1.6735 | yes |
| "the latter harmonic component changes appreciably even though the vector norm changes by about 1%" (673-674) | relative norm of the L2→L3 *change* (perturbed h45 ripple) = 1.19%; the norm itself changes −0.19% | see non-critical note N2 |
| Box-scaled mean gaps 0.58 / 0.66%, ripple 6.7 / 10.2% (675-678) | 0.576 / 0.665; 6.673 / 10.235 | yes |
| SW2 and SW3 components differ by 6.5% and 17% (679-680) | 6.462% and **16.493%** | **SW2 yes; SW3 NO (C1)** |
| L3 ripple gaps 6.58 / 10.25%; per-model change 0.46-1.19% (681-682) | 6.583 / 10.247; 0.465, 0.492, 1.190, 1.128 | yes |
| L3 mean gaps 0.16 / 0.26%; conforming changes 0.52-0.54% (682-683) | 0.163 / 0.261; 0.540, 0.517 | yes |
| Box widths 9, 10, 15, 4, 3, 1, 18, 70, 1.5 mm (699-700) | same (from `optimization_parameter_bounds`) | yes |
| M3 Ultra, 28 cores, load 14-24; 24 warm-started FD sweeps at h = 1e-6 (704-712) | design README l.429-439 | yes |
| Gradient 1.13-1.20 forwards; FD 8.2-10.5 incl. base; L0/L1 median of 3, L2 single (717-719, 634) | 1.1256-1.1990; 8.2427-10.5324; repeats 3/3/1 | yes |
| Table 3 cells | match the cost JSON medians (also display-asserted by verify.py) | yes |
| 1,640 forward validation solves (714-715) | 500 + 1140 | yes |

### Protocol and appendix

| Text claim (line) | Source | Match |
|---|---|---|
| Fixed SD1/LENGTH/ScaleR/angle/physical current; 8 scalars vs 8 + 10 offsets; T50/T90/T99; order-93 L2 g20 validation; final L3 g40 checks; 60-unit wall-time budget; censoring (751-759) | `../EXPERIMENTS.md` l.9 and preregistration #1185 with amendments (h93/L2/g20 validator, L3/g40 spot-checks, `FeRule::WallTime` 60 × unit) | yes |
| Slotless: 24 angles, 12 held out, RMSE 3.08e-10, max 6.82e-10 (814-818) | 24 / 12; 3.0772e-10, 6.8247e-10 | yes |

## Figures

- All figures regenerate byte-identically; none is stale.
- Fig. 4 (refined-validation, the only figure changed in v5) agrees with the text in every panel: (a) GEODE band [11.90, 12.28] mN m equals the recomputed interval; (b) GEODE lines at +0.35% / +0.27%; (c) ×3.6; (d) bar labels 1.3147 / 1.2550 / 1.4983. The labels do not overlap.
- `figures/opt120-waveforms.pdf` is in the package but is not referenced by main.tex (Fable M1). That is a presentation item, not an audit defect.

## Flags summary

### Critical flags (block AUDITED)

1. **C1, numerical inconsistency** (main.tex:680): SW3 component difference "17%"; the record gives 16.49%, which rounds to 16%.
2. **C2, text vs record** (main.tex:643-645): "because the remainder changes sign near the anchor" is true at L0 but false at L1. At L1 the remainder stays negative until h = 1e-8, and the window fails because remainder/h² is still drifting at the anchor.

### Non-critical notes

1. **De Gersem & Weiland 2004 "was developed ... by"** (main.tex:94-95): claim support is `partial`. The abstract, now retrieved, supports the method description but not origination. Replace the verb (wording above). The operator asked for this to be flagged; it should be fixed together with C1/C2.
2. **N2, "vector norm changes by about 1%"** (main.tex:673-674): taken literally this is wrong. The perturbed harmonic ripple-vector norm changes by −0.19%; what is about 1% (1.19%) is the norm of the L2→L3 change divided by the norm (the metric of l.681-682). Suggest: "even though the relative change of the vector is only about 1%". Opus raised the same point as a nit.
3. **verify.py coverage gaps**: it does not guard the SW2/SW3 component percentages (C1), the Taylor-failure mechanism (C2) or the l.673-674 norm statement. Optional: add display-string assertions next to the existing v5 block.
4. **Snapshot vs origin/main**: one of 115 snapshots (the harmonic README) differs from origin/main because of a later update note. This is disclosed in the v5 README and the text cites pinned commits, so no action is needed.
5. **Renderer under `python -I`**: `render_v5.py` imports the sibling `refinement_data.py`, so isolated mode fails. The README invocation works. Informational only.

### Outstanding dependencies

None (0 pending markers).

### Audience-fit notes

None (0 governance-vocabulary hits, 0 private-locator hits). The author line reads "Robb Walters" (sole author), as intended.

## Process disclosure (network)

Every request was anonymous, with User-Agent `paper-audit/1.0`, no `mailto`, and no email, name or identity in any header, URL or payload:
- Crossref `api.crossref.org/works/<doi>` (13)
- OpenAlex `api.openalex.org/works/doi:...` (1)
- arXiv PDFs 2112.05572, 1709.05301, 1908.06009 (3)
- aaltodoc handle (1)
- GitHub tree URLs for the 8 `\href` targets (8)

The single exception: `gh issue view 1185 -R rjwalters/geode-fem` used the existing gh CLI session. That is the project's own forge, not a citation or metadata service, and nothing was sent beyond the standard authenticated read.

## Files

- Audit (this file): `/Volumes/Stripe/scratch-codex-review/motor-paper-v5-reviews/audit.md`
- Scratch root: `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-v5-audit/`
  - `tree/`: clean `git archive 76d78e1f papers`
  - `build/`: compile copy; `compile-log.txt` and `final-pass.log` alongside
  - `render/`: re-render copy (byte-identical outputs)
  - `scripts/x1.py`, `scripts/x2.py` (independent recomputation), `hashes.py` (230 hash checks), `cr.py` (Crossref summary)
  - `x1.out`, `x2.out`: recomputation output
  - `crossref/`, `abstracts/`, `arxiv/`: raw resolver responses and source texts
