# Changelog — conformal-antenna-diffopt.1 → .2

Reviser pass consuming the single critic sibling
`conformal-antenna-diffopt.1.review/` (verdict 34/44, `advance:false`, 0
critical flags). The central lever: NEW measured evidence landed on `main`
(`benchmarks/fdtd_density_baseline/`) converts the paper's biggest weakness —
evidence off a single fixture, head-to-head deferred to future work — into a real
measured two-axis **Evaluation** section, which in turn substantiates the title's
comparative claim. Every number added traces to a committed JSON; no converged
FDTD-density optimization was run or asserted.

| Source | Note | Resolution |
|--------|------|------------|
| .1.review (generic, major; D2/D7/D9) | Title asserts "Cannot Reach" but body defers the comparison to future work; unsubstantiated comparative claim | **Kept the comparative title** (per task) and made the body deliver it: rewrote §6 "Planned Evaluation (Future Work)" into a real §5 Evaluation (`sec:eval`) presenting the measured two-axis intractability head-to-head. Title↔body tension removed by substantiation, not softening. |
| .1.review (generic, major; D2) | Evidence rests on a single fixture; no ablation/baseline isolating the load-bearing design choice | Added the measured GEODE-vs-FDTD-density head-to-head on two axes (geometric fidelity + compute), from `staircasing_results.json` and `meep_runtime_scaling.json`. Per task, this supersedes the low-DOF-vs-73-DOF / with-vs-without-morph ablation; the residual single-fixture caveat for the *reachability* demo is stated honestly in §6 Limitations. |
| .1.review (venue:neurips, major; `missing_baseline`) | Structured-grid contrast is unmeasured; a NeurIPS reviewer expects a staircased-FDTD or low-DOF baseline | Addressed by the new §5 Evaluation: staircasing (geometric) + measured Meep runtime scaling (compute), both committed. |
| .1.review (generic, major; D8) | `wang2011` bib entry has a garbled/conflated Navier–Stokes + EM title | Replaced with the corrected single-title entry from the root `refs.bib` (DOI 10.2514/1.J050594, "Adjoint-Based Shape Optimization for Electromagnetic Problems Using Discontinuous Galerkin Methods"). |
| .1.review (generic, major; D6) | Both figures are unrendered placeholders | Carried `figures/src/` over verbatim (`plot_s11_band.py`, `setup_schematic.md`) and added `plot_runtime_scaling.py` for the new Evaluation figure. Rendering is the `paper-figures` phase's job (runs next per the convergence loop); captions retain the placeholder note until then. |
| .1.review (generic, minor; D5) | Body has no code/artifact-availability statement (repo/commit only in `.tex` header comment) | Added a body-level `\section*{Artifact and code availability}` (`sec:availability`) naming the repo and the committed artifacts: `benchmarks/patch_antenna_conformal/conformal_results.toml`, `benchmarks/fdtd_density_baseline/` (both JSONs + `RUNTIME_SCALING.md`), `reference/meep/docker/`, and the `crates/geode-core` implementation + runnable example — all now on `main`. |
| .1.review (generic, minor; D8/D5) | "prior single-DOF capstone ... FD rel_err ~1e-9" traces to no cited artifact | Dropped the untraceable `~1e-9` figure; reworded to "a separate, earlier artifact ... through the same FD gate" so every stated number has a traceable home. |
| .1.review (generic, minor; D9) | "narrow, previously-unoccupied combination" + three-differentiators thesis restated in ~5 places | Consolidated to a single canonical statement in §2.4 (`sec:whitespace`); abstract, intro, and conclusion now refer back to it instead of re-listing the three differentiators. |
| .1.review (generic, minor) | Unit reframing vs. artifact `_mm`/`_ohm` field names could confuse a reader comparing paper to artifact | Added a half-sentence in §3.1 noting the solver's stored `bend_radius_mm` / `pml_thick_mm` / `port_resistance_ohm` suffixes are naming scaffolding read as dimensionless natural units. |
| .1.review (generic, nit; venue minor) | Title is long and the "Cannot Reach" flourish sits awkwardly against the hedged body | Declined to retitle — per task the comparative framing is KEPT and is now substantiated by the measured Evaluation; the previously-hedged body now delivers the claim, resolving the tone mismatch. |
| .1.review (generic, minor; D8) | `ghassemi2013` missing volume/number/pages | Declined — the reviser must not invent BibTeX (task constraint); entry left with its verified DOI+year. Flagged for the `paper-litsearch` pass alongside the three named citation gaps. |
| .1.review (findings; D4 substrate) + BRIEF | Three author-declared citation gaps (ceviche/Hughes 2019, Meep-adjoint docs, canonical antenna-topology-opt ref) | Retained as named-but-uncited in §6 Future Work; not cited from unresolved keys. Left for a dedicated `paper-litsearch` pass per BRIEF. |

## Preserved (did not regress)

- **D1 Rigor (5/6), D3 Contribution (5/5), D4 Related-work (5/5)** — Method §3 and
  Related Work §2 carried over essentially verbatim; only the §2.4 white-space
  paragraph gained a label and a one-line "stated once" note. No method prose was
  rewritten for flow.
- **Honesty discipline** — natural units preserved (no GHz); novelty stays narrow
  (explicitly not "first EM shape adjoint"; wang2011/ghassemi2013/ham2020 kept
  distinguished); v1 scope = match+bandwidth (pattern/gain still future work); the
  step-cap reproducibility note kept to a sentence.
- **No fabricated converged-FDTD-density number.** The Evaluation section states
  plainly it did not run a converged FDTD-density optimization; intractability is
  shown by measured scaling + explicitly-labelled projections anchored on a
  measured R=8 ≥421-step / ≥12-min unconverged solve.

## Traceability of new numbers (all from committed JSON on `main`)

- Geometric axis (`benchmarks/fdtd_density_baseline/staircasing_results.json`):
  perimeter rel-err 13.0/15.4/14.2/14.2 % at N=20/40/80/160 (log-log slope
  −0.026, ≈constant ~+14%); area rel-err slope ~1.96; boundary-RMS slope ~0.88;
  conformal reference boundary RMS = 0.0; ~6029 cells across the ~24.1 feature
  (~250 cells/mm) for ~1 µm; ~5.35e4× 3-D cell blow-up over the finest grid.
- Compute axis (`benchmarks/fdtd_density_baseline/meep_runtime_scaling.json`):
  cells = 161280·R³; s/step 0.233/0.742/1.715/3.386/5.761 at R=4/6/8/10/12; peak
  RAM 1.6/4.9/11.3/21.8/37.4 GB; dt 0.125→0.0417 (∝1/R); s/step ∝ R^2.92; RAM ∝
  R^2.89; forward solve ∝ R^3.92 (~R⁴). Measured anchor: R=8 ≥421 steps / ≥12 min
  unconverged → ≥24 min/gradient. Projections (labelled): ≥14 h at R=8, ≥70 h at
  R=12; R=14 ~60 GB, R=16 ~83 GB (R≥14 does not fit the 61 GB box); curve-faithful
  ~250 cells/mm ⇒ ~2.5e15 cells. Host AWS m6i.4xlarge, Meep 1.34.0.
- GEODE comparison numbers unchanged from
  `benchmarks/patch_antenna_conformal/conformal_results.toml` (−12.06 dB worst-of-band,
  6 steps, 1 factorization/frequency).
