# paper-audit: motor-gradient-optimization.2 (PR #1171 head 30664b3a)

Auditor: anvil:paper-audit (Claude Opus 5.5), 2026-10-17.
Version dir (read-only, untouched; `git status` clean before and after):
`/Volumes/Stripe/geode-fem/review-1171v2/papers/motor-gradient-optimization/motor-gradient-optimization.2/`
Evidence: `origin/main` = b487ad45 (fetched in `review-main`); evidence commit 910593ba is an ancestor of origin/main.

Deviation from the command contract, at the operator's request: no `<thread>.2.audit/` sidecar was staged in the worktree. This single file replaces citation-audit.md, numerical-audit.md and flags.md. The compile log is next to it at `compile-log.txt`, built from a copy in `build/`.

## Verdict

**FAIL (READY-WITH-AUDIT-FLAGS).** One critical numerical flag, which is a one-character fix. There are also four bib-metadata defects, of which only the arkkio2012proper type/booktitle error shows in the rendered PDF.

- Critical flags: 1 (numerical inconsistency, Appendix A).
- Unresolved cites: 0. Claim-support failures: 0.
- Build: OK. Converged at pass 4 of pdflatex (bibtex 0 warnings; pass-5 `.aux` byte-identical to pass 4). No undefined citations or references, no `??`, 0 overfull boxes, 14 pages.
- `verify.py`: PASS (exit 0; 78 evidence hashes, 500 + 1140 solve statuses, all 800 loaded FD sweeps recomputed).
- Pending-marker gate: pass (0 markers). Audience/link-hygiene scan: pass (0 hits; public_repo_url declared via .anvil.json, every artifact path is `\href`ed).
- Figures: render scripts and PDFs have identical mtimes (checkout), so none is stale.

## Critical flags

1. **Numerical inconsistency** (Appendix A, main.tex:606). The text says "maximum error $6.83\times10^{-10}$". The recorded `slotless_screen.json` `through_harmonic_1.heldout_max_abs_error_over_analytic_amplitude` is 6.8247e-10, which rounds to **6.82e-10**. The snapshot hash matches the manifest, so the source is not in doubt. The RMSE 3.08e-10 (3.0772e-10) is correct. Fix: change 6.83 to 6.82. Suggest adding an `assert_table`-style check of this value to verify.py, since it is not currently covered.

## Citation audit

All 11 keys resolve. Every DOI was checked against the Crossref API, and abstracts came from OpenAlex, Semantic Scholar or arXiv where available. `refs/` holds only literature-screen metadata, not PDFs, so claim support is judged from the abstracts.

| Key | Metadata (Crossref) | Surrounding claim | Verdict | Notes / defect |
|---|---|---|---|---|
| wiesheu2024spline | OK: ICEM 2024, pp. 1-6, doi 10.1109/ICEM60801.2024.10700065; 6 authors match (Crossref drops the umlaut in "Schops"; the bib is correct) | spline rotor+stator shape optimization with engineering parameters, nonlinear PMSM | supports | Abstract: "optimized for both geometric parameters and surface shapes ... 2D nonlinear". |
| wiesheu2025combined | **Defect.** Cited as arXiv 2311.06046v2, but a version of record exists: *Optimization and Engineering* 26(2):1011-1038, 2025 (online 2024-09-21), doi 10.1007/s11081-024-09925-0. There is also a published Correction, doi 10.1007/s11081-024-09947-8, pp. 1039-1040. arXiv v2 metadata (updated 2025-03-17) matches the bib note. | "compares parameter and shape optimization with IGA" | partial | The paper *combines* parameter and shape optimization and compares the result with separately optimized designs. Suggest "combine parameter and shape optimization ... and compare against each alone". Fix: `@article`, journal = {Optimization and Engineering}, volume 26, number 2, pages 1011--1038, year 2025, doi 10.1007/s11081-024-09925-0. Optionally keep the eprint field. |
| scotzniovsky2024 | OK: Optim. Eng. 26(1):121-158, doi 10.1007/s11081-024-09892-6; 7 authors match. Print 2025-03, online 2024-05-22. The v1 year/volume issue is resolved (year 2025, vol 26). | adjoint gives many sensitivities without a solve per parameter; combines geometric deformation, mesh motion and EM analysis with adjoint derivatives | unverified detail, title consistent | No abstract retrievable (Springer and S2 returned none). The title confirms adjoint-based geometric shape optimization of motors. The mesh-motion detail is unverified. Cosmetic: key says 2024, year is 2025; acceptable. |
| bontinck2018isogeometric | OK: CMAME 334:40-55, 2018, doi 10.1016/j.cma.2018.01.047; 4 authors match | separate IGA subdomains with harmonic interface constraints; exact circular geometry avoids polygonal air-gap error | supports | Abstract: arcs parametrized exactly, "avoiding any geometrical error ... air gap"; rotor/stator multipatch coupled with harmonic basis functions (saddle point). |
| egger2022torque | OK: J. Math. Ind. 12(1), 2022, doi 10.1186/s13362-022-00121-2; 5 authors match. **Minor defect:** article number 6 is missing. | torque from the energy of a harmonic mortar formulation, used for ripple evaluation; coupling-torque Eq. (8) "consistent with" it | supports | Abstract: "explicit formulas for the torque computation are derived by energetic considerations ... evaluation of torque ripples". Fix: add `pages = {6}` (or `eid = {6}`). |
| arkkio1987 | OK: Helsinki Univ. Tech. doctoral thesis, 1987. The handle URL resolves (HTTP 200 → items/1bcfc3de...); citation-notes metadata matches (date 1987-12-18, ISBN 951-22-6076-X). | gap-averaged Maxwell-stress torque (Eq. 5) | supports (standard attribution) | Full text not inspected. Optional: the record's ISSN 0001-6845 is *Acta Polytechnica Scandinavica*; adding the series field (check the number on the record) helps retrieval. |
| arkkio2012proper | **Defect.** Typed `@article` with journal = "International Symposium on Power Electronics Power Electronics, Electrical Drives, Automation and Motion". The duplicated phrase comes from Crossref's own container-title, and it renders that way on p. 13. Authors, title, pp. 1456-1461, 2012-06 and doi 10.1109/SPEEDAM.2012.6264430 are correct. | mesh selection for torque computation is an established concern | supports | Abstract: torque accuracy "is sensitive to the quality of the finite-element mesh in the air-gap region". Fix: `@inproceedings`, booktitle = {2012 International Symposium on Power Electronics, Electrical Drives, Automation and Motion (SPEEDAM)}. |
| ion2018robust | OK: Electr. Eng. 100(4):2635-2647, 2018, doi 10.1007/s00202-018-0716-6; 8 authors match | gradient-based vs stochastic optimization of electric devices incl. a PMSM; parametrizations avoid remeshing | supports | Abstract: affine parametrization / design elements "avoid remeshing"; PMSM example; advantage over PSO. |
| kemmetmueller2015 | OK: Mechatronics 32:22-33, 2015, doi 10.1016/j.mechatronics.2015.10.007; 3 authors match | nonlinear MEC models for torque control, including saturation and nonsinusoidal behavior | partial (title-level) | No abstract retrievable. The title confirms MEC-based optimal torque control of PMSMs. The "saturation and nonsinusoidal" detail is unverified; the author should confirm from the paper. |
| roomi2022 | OK: IET EPA 16(11):1286-1299, 2022, doi 10.1049/elp2.12226; 3 authors match | waveform-targeting surrogate for machine design | supports | Abstract: Waveform-Targeted Surrogate Modeling on an IPMSM. |
| wen2023 | OK: JCP 484:112082, 2023, doi 10.1016/j.jcp.2023.112082; authors Wen & Zahr match | reduced models adapted along a shape-optimization trajectory, fluid examples | supports | arXiv 2206.09942 abstract: on-the-fly hyperreduced ROM in a trust region; "two fluid shape optimization problems". |

### Citation defects and fixes (summary)

1. `arkkio2012proper`: change to `@inproceedings` with booktitle `2012 International Symposium on Power Electronics, Electrical Drives, Automation and Motion (SPEEDAM)`. This removes the duplicated "Power Electronics" visible in the PDF.
2. `wiesheu2025combined`: cite the version of record, Optim. Eng. 26(2):1011-1038 (2025), doi 10.1007/s11081-024-09925-0, instead of arXiv v2. Note the Correction (10.1007/s11081-024-09947-8). Soften "compares" to "combines ... and compares".
3. `egger2022torque`: add article number 6.
4. Optional: `arkkio1987` series field; `scotzniovsky2024` key/year mismatch is cosmetic.

Claim-support notes that are not flagged: the Scotzniovsky mesh-motion detail and the Kemmetmüller saturation/nonsinusoidal detail can only be checked at title level from what I could retrieve. Both reviewers also note that two claims are uncited: the single-circle Maxwell-stress sensitivity (main.tex:392-393) and the conforming sliding interface (205-206).

### Reviewer-raised leads

| Lead | Exists? | Verified metadata | Fit |
|---|---|---|---|
| Howe & Zhu 1992, cogging | **Yes** | D. Howe, Z. Q. Zhu, "The influence of finite element discretisation on the prediction of cogging torque in permanent magnet excited motors", IEEE Trans. Magn. 28(2):1080-1083, Mar. 1992, doi 10.1109/20.123869. Crossref lists Howe as first author. | **Strong.** The abstract says torque accuracy is "determined largely by the level of discretization, as well as the evaluation method" and compares the energy method with Maxwell-stress integration for cogging. That matches the "mean agrees, cogging does not" phenomenon (Sec. 5.1/5.3), and it can cite the currently uncited single-circle Maxwell-stress sentence (392-393). A companion, Zhu & Howe, "Analytical prediction of the cogging torque in radial-field PM brushless motors", same issue pp. 1371-1374, doi 10.1109/20.123947, is less relevant. |
| Merkel, Gangl, Schöps 2021 | **Yes** | M. Merkel, P. Gangl, S. Schöps, "Shape Optimization of Rotating Electric Machines Using Isogeometric Analysis", IEEE Trans. Energy Convers. 36(4):2683-2690, Dec. 2021, doi 10.1109/TEC.2021.3061271 | **Strong.** It covers a 6-pole IPMSM, multipatch IGA with harmonic stator-rotor coupling, and shape-derivative freeform optimization of EMF THD. This is the reference group's own IGA shape-optimization line and it fits both the "differentiable motor design is established" and "harmonic coupling is established" paragraphs. Bonus lead found in the same search: Merkel, Kapidani, Schöps, Vázquez, "Torque Computation With the Isogeometric Mortar Method for the Simulation of Electric Machines", IEEE Trans. Magn. 58(9), 2022, doi 10.1109/TMAG.2022.3186247 (metadata only). It fits the torque-estimator paragraph. |

## Numerical audit

`verify.py` passes. It covers Table 1 (all coupling.tex cells), harmonic ripple/mean percentages, max harmonic pointwise gaps, 500/1140 solve statuses, Table 2 sensitivities, estimator/free-end/full-order/ladder bounds, the 800/782/worst FD statistics, zero-current FD bounds, SW1 ±, cost ratios and Table 3, Table 4 design points, cite keys and figure assets.

Numbers it does **not** cover, which I checked against the hashed snapshot data (identical to the source at the evidence commit) and against `origin/main`:

| Text claim (line) | Source | Source value | Match |
|---|---|---|---|
| radii 16/44/45/67.5 mm, L = 0.1 m (115-117) | reference_data.json geometry defaults (RD2 32, RD1 88, SD2 90, SD1 135 mm diameters; LENGTH 0.1) | same | yes |
| winding areas 36.29 / 31.22 mm² (136); ~16% density difference (149) | origin/main geometry/README.md | 36.291215073 / 31.215149045; ratio 1.163 | yes |
| I* = 9.677657374 A (144) | reference_data.json | 9.677657374172274 | yes |
| 20 angles; even 10-angle optimization grid (149-152) | reference_data.json | 0..19; 0,2,..,18 | yes |
| 47 B-H points, 0-2.30 T (189) | reference_data.json material | 47, 0.0-2.3 | yes |
| H/B non-monotone up to ~0.75 T (191) | snapshot README | ν' < 0 from 0 to 0.735 T | yes |
| agree to round-off 0.05-2.25 T; 6.66%; last segment 2.17e-4 (196-201) | pchip_compare.json | 4.2e-16; 0.06659; 2.1706e-4 | yes |
| linear-limit agreement ≈2e-14 (260-261) | m1 README / m1_tables.log | 1.3e-14, 1.7e-14 | yes (loose rounding) |
| tangent FD < 1e-9 (262-263) | m1 README | 6.55e-10, 8.26e-10 | yes |
| rates 1.002 / 2.007; annulus 3.76e-4 (264-268) | m1_tables.log / README | same | yes |
| nodes 3,153 / 9,937 / 36,083 / 139,268 (278-279) | harmonic summary cost | same | yes |
| 120·2^ℓ coupling segments (279) | origin/main geometry/README.md | N = 120·2^level | yes |
| 60 solves per level; 500 = 240 + sensitivity (186, 281) | run row counts | 4×60 + 4×60 + 20 = 500 | yes |
| conforming means −0.15/−0.16%; ripple +9.9/+11.3%, cogging −29.1% (308-312, 339) | recomputed from summary + reference | −0.153/−0.156; +9.86/+11.27; −29.09 | yes |
| both reference means inside harmonic band; full-current std inside; half-current ~5e-5 below (310-311, 324-325) | harmonic summary convergence_h45 (Arkkio) | inside/inside; inside; outside by 4.61e-5 | yes |
| conforming max full-current gap 0.0272 (23, 323) | harmonic summary pointwise | 0.027192 | yes |
| cogging ≈1.13× reference (340) | least-squares scale of L3 h45 vs reference | 1.1286 | yes |
| order 45 vs conforming at L3: ~59% cogging, 7-9% loaded (348-349) | harmonic summary main_h45 | +59.2%, −7.4%, −9.0% | yes |
| order 189 within 1.1e-4 at L1, L2 (350) | ladder | 1.02e-4, 6.6e-5 | yes |
| full-order identity 4.1e-9 (L0), 1.3e-9 (L1) (254-255) | hall_vs_conforming | 4.06e-9, 1.24e-9 | yes |
| filtering diagnostic 0.0075→0.0184; 0.04280→0.04460; 0.08832→0.08775; cuts 0.0077/0.0300/0.0184/0.0077/0.0075 (399-403) | origin/main forward/tables.md + snapshot harmonic_torque | 0.00752→0.01841; 0.04280→0.04460; 0.08832→0.08775; 0.00774/0.03004/0.01841/0.00771/0.00755 (0.007549) | yes |
| zero-current mean best FD up to 7.0e-4; cogging std ≤ 8.9e-7 (498-500) | recomputed from study sweeps | 7.04e-4; 8.85e-7 | yes |
| two SW1 ripple anchored-window failures, widest window 2 decades (495-497) | gradient README | SW1 std Jinit conforming L0/L1; 2.0 decades | yes |
| SW3 mean sign change, ~linear convergence (504-505) | gradient README | −0.099 (L1) → +0.406 (L2); "roughly O(h)" | yes |
| MW remesh gap 5.4e-3 (L0) → 2.6e-6 fitted (L2) (509-511) | gradient README | same | yes |
| M3 Ultra 28 cores, load 14-24; L0/L1 median of 3, L2 single (490, 516-517) | gradient README; cost JSON `repeats` | same; 3/3/1 | yes |
| ratios 1.13-1.20 and 7.2-9.5 (29-30, 526-527) | cost JSON | 1.126-1.199; 7.24-9.53 | yes |
| slotless 24 angles; RMSE 3.08e-10 (602-606) | slotless_screen.json | 24; 3.077e-10 | yes |
| **slotless max error 6.83e-10 (606)** | slotless_screen.json | **6.8247e-10 → 6.82e-10** | **NO (critical flag 1)** |

Non-critical numeric notes:
- Fig. 4(c) plots SW1 sensitivities in N m/mm, while the text (455-456) quotes N m/m. The values are consistent (−0.22 N m/mm = −220.85 N m/m), but the units should be unified. Both reviewers raise this.
- The merged gradient README prose (L116-117) quotes the L0 SW1 values (+1.73 / −221.7) without labelling them. The paper correctly uses and labels the L1 values. This is an evidence-README nit, not a paper defect.

## Files

- Audit: `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-paper-v2/audit.md`
- Compile log (all passes): `/private/tmp/claude-501/-Users-rwalters-GitHub-geode-fem/499ca39e-eae0-4400-9dc2-215b57f2d28d/scratchpad/motor-paper-v2/compile-log.txt`
- Check scripts: `.../scratchpad/motor-paper-v2/scripts/x1.py`, `x2.py`
