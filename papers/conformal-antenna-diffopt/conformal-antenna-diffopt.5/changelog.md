# Changelog: conformal-antenna-diffopt.4 → .5

v5 is a rewrite, not an incremental revision. The operator replaced the v4
evidence base (the natural-unit smoke design and the Meep staircasing / runtime
axes) with the physical 2.4 GHz conformal-patch programme merged on `main` at
`219a089c`: fixture (PR #1097), smooth basis and outline DOFs (#1104),
refinement study (#1107), thickness ablation (#1122) and the fixed-1.6 mm
copper-only design (#1124). Every number in v5 is new and traces to those
artifacts (`provenance.md`). v1–v4 and their critic siblings are unchanged.

## Operator inputs (2026-10-13/14)

| Source | Input | Resolution |
|---|---|---|
| operator | Headline = copper-only design on a fixed, uniform 1.6 mm FR-4 board (#1124); thickness/radial designs only as an idealized ablation | Title, abstract, §3.3, §5, §6 rebuilt on `rbD_L1_copper_38_sm45`; thickness designs in §7 "Varying the board thickness", labelled not manufacturable |
| operator | State plainly that the converged design likely misses at 2.400 GHz (6.2 MHz margin vs 12.7–48.4 MHz extrapolated upshift) | Abstract, §6 "Where the converged design probably falls", Figure 3a arrow, Conclusion |
| operator | L3 not met (−8.80 dB, 89 %) | Abstract, §6, Table 3 |
| operator | Feed is an idealized 4.3 mm port strip | Abstract, §8 "Feed" |
| operator | L2-chain design is a related design, not a verified L2 result | §6 "A related design from an L2 chain", Table 3 rows with −0.47 mm |
| operator | Method = band shift / frequency space mapping (47.85 MHz) | §5, citing `bandler1994space`, `bandler2004space` (Crossref-checked) |
| operator ruling | No FEM-vs-FDTD cost-to-accuracy claim; Meep unconverged; comparison only as hypothesis / future work | All v4 §5 (staircasing, R^3/R^4 runtime, RAM wall, "cannot reach") removed; §8 "Comparison with structured-grid methods" states the hypothesis and the unconverged Meep pilot with no numbers; title drops "Cannot Reach" |
| operator | Solo author; provenance section modelled on transmon v11 §9; same acknowledgment pattern | §9 "How geode-fem Was Designed and Built" with counts from `count_provenance.py` at `219a089c`; Acknowledgments as in transmon v11 (with the operator TODO) |
| operator | Literature context: 1.6 mm FR-4 patches ~60–95 MHz | Declined as worded: the range is not in `.4.litsearch` or any bib entry. Replaced by the Jackson–Alexopoulos (1991) closed form, evaluated for this substrate (61 / 69 MHz), with the FEM's larger bandwidth stated as unresolved (§6 "Bandwidth in context") |
| operator | Not to be published anywhere | Nothing deposited; Zenodo TODO comment kept |

## Critic siblings at v4

| Source | Note | Resolution |
|---|---|---|
| codex v4 figure review M1 | Fig 1 does not match its caption | New `fig1_geometry.py`, drawn from the committed geometry JSON and fixture metadata: to-scale section with UPML box, feed section, top view of initial and final copper |
| codex M2 | Labels print at 6–7 pt | Shared `figures/src/figure_style.py`: figures drawn at final width (≤ 6.3 in), save refuses text < 8 pt, off-canvas text and overlaps; all three pass at 8.09 pt printed |
| codex M3 | Code paths overflow margins | `xurl` + `\path` in body text and the artifacts table; `\texttt` in captions; 0 overfull boxes in the build log |
| codex S1 | Fig 3 caption mentions absent shading | Runtime figure removed with the Meep axis; new captions describe only drawn encodings |
| codex S2 | Frequency plot should show sampled frequencies on a numeric axis, (a)/(b) panels | Figure 3 plots dense sweeps against frequency in GHz with markers, ISM band shaded, −10 dB line; panels (a)/(b) |
| codex S3 | Grayscale contrast | Dark outlines, dashed vs solid vs hatched encodings in Figure 1 |
| codex S4 | Unnumbered result tables | All four tables are captioned floats |
| codex nice-to-have | Three-figure set (geometry, response, cost) | Adopted geometry and response; the third figure is the refinement study (cost has no committed comparison) |
| .4.review (generic, major) | `fdtdx` preprint author field conflicts with JOSS | `fdtdx` dropped; only `mahlau2026fdtdx` cited |
| .4.review | Overfull hboxes; inline tabulars | 0 overfull; tables are floats |
| .4.litsearch claim changes 1–10 | Title "Cannot Reach", staircasing/perimeter, intractability, missing FEM S-parameter lineage, Hassan, conformal FDTD | Title changed; staircasing/runtime claims removed; Related Work rebuilt around Webb/Akel, Nair, Nikolova, Choi, HFSS, COMSOL, Firedrake/NGSolve, Hassan, Nikolova FDTD, conformal FDTD (Dey–Mittra, Mittra, Zagorodnov, Benkler), Häggblad–Runborg, Cangellaris |
| .4.litsearch item 11 | `ghassemi2013` → `ghassemi2013antenna`; `hammond2025ssp` → `hammond2025unifying` | Done |
| .4.audit / .4.vision / .4.numeric | v4-specific numeric and render findings | Moot: every v4 number and figure was replaced |
| voice.md | No virtue words, no version history, sparing em-dashes, certified vs estimated, credit methods by ablation, no unsourced novelty | No em-dashes in body; "certified" unused (no bounds here); extrapolations called estimates; §7 credits the frequency correction via the uncorrected run and does not credit the shape gradient or the 38-DOF basis ("What the data do not separate"); "We do not claim a first EM shape adjoint" |

## Removed from v4

The 73-DOF natural-unit smoke design (`conformal_results.toml`), the
staircasing table, the Meep runtime-scaling table and figure, the "head-to-head"
section, PNGF scoping paragraph's intractability framing, and the v4
`setup_schematic`, `s11_band`, `runtime_scaling` figures and scripts.
`oskooi2010meep`, `sun2025pngf`, `tidy3dautograd`, `meepadjointdocs` are kept as
related work only.

## Process

- `max_iterations` (4) is exceeded by design: operator-requested rewrite, and
  the operator has ruled that paper revisions have no iteration cap.
  `_progress.json` records `operator_override: true`.
- `BRIEF.md` amended (v5 section) to match the operator's rulings; the original
  brief text is kept below the amendment.
- Build: `papers/build.sh`, 0 undefined citations or references, 0 overfull
  boxes, 15 pages.

## Judge review of PR #1125 (head `de89da46`)

| Note | Resolution |
|---|---|
| Required 1: Jackson–Alexopoulos p factor misremembered, surface-wave Q missing | `jackson_bandwidth.py` (new, committed) implements the CAD formulas as restated in Jackson's ECE 6345 Notes 15–16 (p with the (a2/10), (a2² + 2a4)(3/560), (c2/5) and (a2 c2/70) terms; Q_sp, Q_sw from e_r^hed = 0.899, Q_d; Q_c omitted for the PEC model). §6 now says about 63 and 70 MHz with about 10 % surface-wave loss and cites `jackson_ece6345` alongside `jackson1991simple`; provenance row rewritten |
| 2: "sits 14.4 MHz lower" | "sits about 14 MHz lower" (13.8 / 14.1 / 14.0 MHz at the low edge / high edge / dip against the P2/L1 band; provenance row) |
| 3: `count_provenance.py` attributes untrailered commits to Codex | Docstring sentence removed; keys renamed `untrailered_commits`, `untrailered_added_lines`, `untrailered_line_share_pct` |
| 4: 45–48 MHz read as general | Abstract: "a design optimized on the L1 mesh"; Conclusion: "an L1 optimum lands 45–48 MHz off at P2/L1" |
| 5: Fig 1(a) label runs into the air-box edge | Label shortened to "R = 100 mm"; "patch" and "UPML, 25 mm" labels moved off the box edges |
| 6: Codex review sourced to an uncommitted file | `codex-figure-review.md` committed in this directory (host paths scrubbed); provenance row points to it |
