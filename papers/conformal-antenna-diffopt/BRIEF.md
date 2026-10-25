---
title: "Adjoint Shape Optimization of a Conformal Patch Antenna"
author: "Robb Walters, 2AM Logic; Crutcher Dunnavant, ZSpaceLabs.ai"
venue: arxiv
documentclass: anvil-paper
anonymous: false
web_search: true
---

# v13 — published on Zenodo (v1), 2026-10-25 — CURRENT

**Published.** `conformal-antenna-diffopt.13/` is the paper of record for Zenodo v1, 23 pages.

- **DOI (this version):** 10.5281/zenodo.21480892, https://zenodo.org/records/21480892
- **Concept DOI (all versions; cite and link this one):** 10.5281/zenodo.21480891
- **Files:** `walters-dunnavant-2026-conformal-antenna-v1.pdf` and `walters-dunnavant-2026-conformal-antenna-v1-source.zip` (builds with pdflatex alone), md5-verified against the upload.
- **v13 vs v12:** a corresponding-author footnote on Robb Walters (robb@2amlogic.com) is the only change (operator request before publishing, PR #1268). Coauthor Crutcher Dunnavant approved the v1 release.
- **Revisions from here** go up as new versions of the same Zenodo record (v2, ...); the concept DOI stays the same. Open v2 items are in geode-fem#1231.

# v12 — reading flow, 2026-10-23

**Current state: `conformal-antenna-diffopt.12/` (draft, not reviewed), 23 pages.**
v12 = v11 reordered and glossed for reading flow, from a flow review for an interested
non-specialist reader (geode-fem#1231). No number, result, claim scope, title, author, DOI or
reference changed and no computation was run; v1–v11 stay immutable. Where this section
conflicts with the sections below, it wins.

- **Body order:** Introduction (with Table 1 of names and Figures 1 and 2 as a primer block,
  and "Position of this work" as its last paragraph), Method (opening with the fixture),
  The Adjoint Against Gradient-Free Search, Forward-Model Accuracy, Design Procedure,
  Results, Related Work, Limitations, Conclusion, Acknowledgments, How geode-fem Was
  Designed and Built, Reproducibility. The body now follows the Introduction's three-result
  order and designs A and C are not discussed before Design Procedure introduces them.
- **Added:** one table of wall-clock costs (solves, host, time) built from numbers already in
  v11; glosses for fixture, open-radiator, feed band, cold start, σ₀, UPML, λ and dual
  arithmetic; the point counts behind the two "worst" quantities; the signpost that the
  space-mapping shift is re-fitted at each descent; "unknowns" as the one noun for the count.
- **Trimmed:** the Conclusion no longer repeats the abstract; the Results "Cost" paragraph is
  the cost table; the set-aside 53.6 MHz run is a footnote.
- **Kept:** the title, the DOI 10.5281/zenodo.21480892, the author block, `refs.bib`,
  `figures/` and `main.bbl`, all byte-identical to v11's. `check_v12()` pins the multiset of
  numbers in `main.tex` against v11's with its exact delta. There is no pending marker.
- `changelog.md` maps every proposal of the review to its resolution; none was skipped.

# v11 — short title, 2026-10-22

**State at v11: `conformal-antenna-diffopt.11/` (draft, not reviewed), 22 pages.**
v11 = v10 with the operator's short title (geode-fem#1231). No other wording or number
changed and no computation was run; v1–v10 stay immutable. Where this section conflicts with
the sections below, it wins.

- **Title:** "Adjoint Shape Optimization of a Conformal Patch Antenna" (operator's choice and
  capitalisation). It replaces "Adjoint Shape Optimization of a Conformal 2.4 GHz Patch
  Antenna: EM Solves to Target Against Gradient-Free Search at Several Settings", whose
  subtitle stated a result the abstract already gives. The `title:` front matter, which still
  held an earlier working title, now holds the new title.
- **Kept:** the DOI 10.5281/zenodo.21480892 and the author block. There is no pending marker.

# v10 — v1 Zenodo release, 2026-10-22

**State at v10: `conformal-antenna-diffopt.10/` (draft, not reviewed), 22 pages.**
v10 = v9 with release metadata only (operator, geode-fem#1231). No other
wording or number changed and no computation was run; v1–v9 stay immutable.
Where this section conflicts with the sections below, it wins.

- **Authors:** Robb Walters, "2AM Logic"; Crutcher Dunnavant, "ZSpaceLabs.ai"
  (second author since the operator ruling of 2026-10-22, which superseded
  the v5 "solo author" line). Each author has their own `\normalsize`
  affiliation line. The `[PENDING coauthor-affiliation]` marker is gone.
- **DOI:** the paper's reserved Zenodo DOI, 10.5281/zenodo.21480892, is under
  the date on the title page. v10 is the v1 Zenodo release version.

# Amendment for v5 (operator rulings, 2026-10-13/14)

This section supersedes the original brief below wherever they conflict.

- **Evidence base.** The paper is about the physical 2.4 GHz probe-fed patch bent on R = 100 mm (`benchmarks/patch_antenna_conformal/`): fixture, refinement study, copper-only design and thickness ablation. Units are mm and GHz. The natural-unit smoke design (`conformal_results.toml`) is no longer used.
- **Manufacturability.** The board is a fixed, uniform 1.6 mm FR-4 shell; only the copper (outline, corners, feed inset and width) is designed. Radial / thickness designs are an idealized ablation only.
- **Headline.** `rbD_L1_copper_38_sm45`: −10.78 dB worst in band and a 94.9 MHz −10 dB band covering the ISM band with p = 2 on L1; L3 not met (−8.80 dB, 89 %). State plainly that on the converged estimate (12.7–48.4 MHz further upshift vs a 6.2 MHz margin) the design likely misses at 2.400 GHz. The feed is an idealized 4.3 mm port strip. The L2-chain design is a related design, not a verified L2 result.
- **Method.** Coarse-model optimization plus a fitted frequency shift (frequency space mapping).
- **Comparison.** No FEM-vs-FDTD cost-to-accuracy claim. The Meep harness is unconverged; the comparative framing is a stated hypothesis and future work. The "Cannot Reach" title is dropped.
- **Provenance.** Solo author; a "How geode-fem Was Designed and Built" section and acknowledgment as in transmon-benchmark v11.
- **Publication.** Not to be published; any Zenodo deposit stays an unpublished draft.

# Original brief

# Brief — freeform conformal-radiator differentiable inverse design

## Audience & framing

Target community: **differentiable simulation / ML-for-science** (NeurIPS-style sci-ML, differentiable-physics). The headline is a **killer design result**; the differentiable open-radiator FEM is the *substrate* that makes it reachable, and a moving-boundary shape-adjoint geometry class is the *payload*. Keep the tone rigorous and honest — the contribution is a **narrow, well-scoped implementation-combination claim**, explicitly NOT "first shape adjoint in EM."

## Thesis (one sentence)

A tensor-compiled, differentiable-by-construction 3-D Nédélec H(curl) FEM exposes an **exact, single-factorization discrete shape (moving-boundary / node-motion) adjoint** through the **full open-radiator forward** — box-UPML tensor-material absorbers + lumped ports + lossy complex-ε + a passive |S11| — on **unstructured tetrahedral meshes**, and we use it to gradient-design a **freeform curved-metal conformal antenna**, a geometry class that structured-grid density-based differentiable EM represents poorly and that low-DOF parametric antenna optimization cannot reach.

## Contributions (claim exactly these; do not overclaim)

1. A differentiable-by-construction open-radiator FEM shape adjoint: forward-mode AD "Dual-twin" element kernels give exact ∂K/∂X, ∂M/∂X, ∂b/∂X through a **radiating** complex-symmetric pencil `A(ω) = K(ν) − ω²M(ε) + (jω/Z_s)S_p`, so the transpose-adjoint reuses the **single** forward factorization (`n_factorizations == 1`), with an enforced passivity |S11| ≤ 1 and a non-inversion / mesh-distortion guard.
2. A **high-DOF freeform boundary parametrization** (harmonic / Laplacian mesh-morph, 73 active boundary DOFs) that generalizes single-parameter tuning to freeform shape while keeping the volumetric mesh valid, FD-validated to rel_err ~1e-6.
3. **Headline result:** end-to-end gradient design of a curved-metal conformal radiator that drives the **entire band below −10 dB return loss** (worst-of-band −5.51 → −12.06 dB), on a geometry a Yee-grid density method staircases.
4. Honest positioning of the narrow white space relative to prior EM shape-adjoint and differentiable-FEM work.

## Key result (all numbers trace to committed artifacts — do NOT fabricate)

Artifact of record: `benchmarks/patch_antenna_conformal/conformal_results.toml` (committed on `main`). Implementation: `crates/geode-core/src/driven/shape.rs`, `crates/geode-core/src/shape.rs`, `crates/geode-core/src/mesh/patch.rs`, and the runnable example `crates/geode-core/examples/patch_conformal_diffopt.rs`.

- 73 active freeform boundary DOFs on a curved `bent_conformal` FR-4 + PEC patch/ground radiator inside a box-UPML.
- Multi-frequency match objective over ω ∈ {0.30, 0.35, 0.40} (**natural units, as recorded — keep them dimensionless; do not convert to GHz**).
- Final per-frequency |S11|: ω=0.30 → −12.06 dB, ω=0.35 → −23.92 dB, ω=0.40 → −14.42 dB. Worst-of-band improved −5.51 → −12.06 dB (band objective ×5.56).
- Terminal condition `target_reached` (6 of 600 steps — a genuine target hit, NOT a step cap); non-inversion guard never bound (worst per-tet volume ratio 0.572 vs 0.25 budget); passivity |S11| ≤ 1 held at every evaluation; deterministic (bit-identical re-run).
- Gradient correctness: directional finite-difference gate through the public `driven_solve_with_ports(MatchedUpml, port)` pipeline on the curved mesh at rel_err 1.17e-10; one factorization per band frequency.

Methodological foundation (already committed): the driven SHAPE-adjoint stack — lossy complex-ε adjoint, pinned/moving-feed lumped-port adjoint, box-UPML tensor-material adjoint — validated by a prior single-DOF capstone that retuned a flat patch to −28 dB (FD rel_err ~1e-9). The conformal result extends that from 1 DOF to 73 freeform DOFs on curved geometry.

## Related work & honest positioning (from a 104-agent adversarially-verified prior-art scan)

The paper MUST cite and clearly distinguish (starter entries in `refs.bib`):

- **Structured-grid density-based differentiable EM** (the contrast class): Meep-adjoint, ceviche / ceviche-challenges, SPINS-B (`su_spinsb`), FDTDX (`fdtdx`), invrs-gym (`invrsgym`). These optimize a permittivity/density field (topology optimization, later binarized) on a Yee/Cartesian grid; canonical "killer" demos — Piggott WDM demultiplexer (`piggott2015`), high-NA achromatic metalens (`chung2020`) — are guided/periodic photonics, **not** open radiators with PML.
- **AD shape optimization still on a rectilinear grid**: Hooten AutoDiffGeo (`hooten2025`) maps shape parameters to a permittivity distribution painted on a fixed grid — reinforces, not closes, the white space.
- **True EM shape (node-motion) adjoints on unstructured meshes exist** — so DO NOT claim a first: Wang & Anderson (`wang2011`) is a discrete Maxwell shape adjoint on unstructured meshes, but 2-D, time-domain, lossless, PEC-walled, **no PML / no ports / no S-parameters**; Ghassemi et al. (`ghassemi2013`) evolve a microstrip **antenna** geometry via FEM adjoint sensitivity but with a handful of **low-DOF control vertices** and a hand-derived adjoint; Ham/Mitusch/Schmidt (`ham2020`) provide **automatic** FEM shape derivatives (Firedrake pyadjoint) for PDEs generally.
- **Defensible white space (state it precisely):** a 3-D frequency-domain Nédélec H(curl) FEM shape adjoint through the **full open-radiator forward** (box-UPML tensor material + lumped ports + lossy complex-ε + passive |S11|), differentiable-by-construction with single-factorization reuse, applied to a **freeform, high-DOF curved-metal conformal** design. No found work occupies this exact combination. The three stacked differentiators vs the prior art: (1) freeform / high-DOF vs a few control points; (2) full open-radiator physics vs lossless PEC; (3) differentiable-by-construction & composable vs hand-derived adjoints.

## Scope & honesty constraints (LOAD-BEARING — the audit will enforce these)

- **The 3-way head-to-head is NOT done yet.** The comparative baselines — an FDTD-density optimizer (Meep-adjoint / ceviche) staircasing the curved geometry, and a low-DOF parametric sweep — are planned (project epic #647 Phase 4). Write the comparison as an **explicit "planned evaluation" / future-work subsection with NO fabricated baseline numbers**. The paper's honest claim today is: GEODE *reaches* a freeform curved-conformal −10 dB design; the *quantified* superiority over FDTD-density is the announced next step.
- **v1 scope = impedance match + bandwidth only.** Radiation pattern / gain (a near-to-far-field adjoint) is explicit **future work** (Phase 5), not claimed here.
- **Units:** frequencies are dimensionless natural units as recorded; do not invent physical GHz values.
- **Every quantitative claim must trace to `conformal_results.toml` or the cited code.** Flag anything not yet substantiated rather than asserting it.
- A brief methods-honesty note is welcome: the first optimization run mis-recorded a premature (step-cap) stop as a physics limit; a corrected line search reached the true `target_reached` terminal. This is a reproducibility/rigor strength, not something to hide — but keep it to a sentence.

## Suggested structure

1. Introduction — the differentiable-simulation lens; why open radiators + moving boundaries are the underserved corner.
2. Related work — the honest positioning above.
3. Method — the differentiable open-radiator FEM shape adjoint (pencil, Dual-twin kernels, single-factorization adjoint, passivity + non-inversion guards) and the high-DOF harmonic morph.
4. Result — the curved conformal −10 dB freeform design; FD validation; determinism.
5. Planned evaluation (future work) — the 3-way head-to-head; pattern/gain NTFF adjoint.
6. Conclusion.

## Gaps for litsearch (author-known, BibTeX not yet supplied)

- The ceviche method paper (Hughes et al., forward-mode differentiation of Maxwell's equations, ACS Photonics 2019) — supply a resolvable DOI/arXiv id.
- Meep-adjoint documentation / the Meep adjoint-solver reference.
- A canonical antenna shape/topology-optimization reference beyond Ghassemi, if one strengthens the parametric-baseline framing.
