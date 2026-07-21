---
title: "Differentiable-by-Construction FEM for Freeform Open-Radiator Design: A Curved Conformal Antenna Structured-Grid Inverse Design Cannot Reach"
author: "Robb Walters"
venue: neurips
documentclass: anvil-paper
anonymous: false
web_search: false
---

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
