# Changelog: conformal-antenna-diffopt.8 → .9

Input: outside-reader feedback on v8 from Crutcher Dunnavant (a tensor-compiler and ML-systems
engineer, not an antenna specialist), forwarded by the operator. The aim of this revision is that
such a reader can tell what every named object is. Text and figures only: no solver, mesher or
`cargo` run, and every new number is read from committed records. v1–v8 and their critic
directories are unchanged (`verify_evidence.py` diffs them against the merge base with
`origin/main`).

## Reader feedback

| Crutcher's point | Resolution |
|---|---|
| "Solve count" is confusing; maybe "optimization step cost"? | The unit is now an **EM solve**, defined at first use in a new §1 paragraph "The cost measure": one frequency of the finite-element model, one sparse LU factorization of the 150,561-unknown coarse system and its back-substitutions; one objective evaluation is five solves, 72.1 s forward and 95.1 s with the adjoint gradient on one host. The compared quantity is **solves to target** (EM solves spent when the best design so far first reaches a target level). Title: "... EM Solves to Target Against Gradient-Free Search at Several Settings"; "solve count" removed throughout (the verifier forbids it). A per-step cost would not compare optimizers whose steps differ in size, so the cumulative count was kept and named. |
| "The relative convergence rate improves with more parameters": at what rate? | §1, abstract, §5 and §11 now give the numbers: to −9.5 dB the ratio is 2.4×, 2.9× and 11× at 4, 14 and 38 parameters (−9 dB: 1.1×, 3.3×, 7.7×); the adjoint's count falls 85 → 40 → 35 while the best gradient-free count is 205, 115, 385. §5 says what the three points show and do not: from 14 to 38 parameters the gradient-free count grows 3.3-fold for 2.7× the parameters, the 4 → 14 step goes the other way, and no law is fitted to three points from one problem. Figure 4(b) labels the ratio at each n. |
| The coverage result both claims and denies the ISM band | One statement, in the same structure everywhere (abstract, §1 bullet 3, a new §7 paragraph "What is claimed", §11): **measured** (model-dependent) coverage of 2.400–2.4835 GHz at −10 dB on the three most refined models, with 6.5, 6.6 and 1.9 MHz to spare; **estimated** mesh-limit miss of 1 to 13 MHz at the lower edge; therefore ISM coverage is claimed for those discrete models, not for the mesh-limit model or a built antenna. The §1 bullet headline now reads "covers the ISM band on the most refined models computed, and is estimated to miss it in the mesh limit". The phrase "we do not claim that it meets the ISM band" is gone (the verifier forbids it). |
| Where do the §3.4 geometry terms (tetrahedron, plateau) come from? | §3.3 opens with a "Mesh morphing" paragraph: the mesh conforms to the copper, a design moves nodes instead of remeshing, the air follows, and a squeezed tetrahedron inverts. §3.4 has a new paragraph "The volume guard and how a descent ends", written from the generator source (`patch_conformal_nested_dof.rs`): a trial step is rejected without a solve, and halved, if any tetrahedron's signed volume would fall below 0.25 of its unmorphed value; a descent ends on the guard when 25 halvings find no step that passes the guard and the Armijo test while some trial failed the guard, or at a plateau with the worst element within 5% of the guard; a plateau is four accepted steps in a row each lowering G by less than 5×10⁻⁴ relative. The one plateau run is identified (`copper/rbB_L1_copper_38_warm.toml`). What "ends on the guard" does and does not mean is stated. |
| A diagram for slab, patch, rim and patch edge | Figure 1 rebuilt as six panels from the same records: (a) board, air box, 40 mm air margin and UPML to scale; (b) a new section across the bend, z ×3, labelling ground, substrate, slab (dotted), patch, the ±s edges, the 1.8 mm sagitta and the 12 mm margin, with the rim marked; (c) the section through the feed labelling the ±y edges, probe and inset; (d) a new to-scale close-up of the rim (within 0.5 mm of the patch edge); (e) a new top view of what the 38 parameters move (one arrow per B-spline coefficient, the corner bumps, the probe's inset and width); (f) initial and final copper. The script asserts the basis counts and descriptions against the geometry record. |
| Named curve features ("the dip") without charts; ordering of charts; point labels | New Figure 2 in §3.4, before any of these words are used in results: the initial patch and design C annotated with the dip, the −10 dB target, the ISM band, the −10 dB band and its edges (crossings), both margins, the in-band points and the worst-of-band value; the script asserts each against the summary record. "Dip" is defined at first use in §1 and §4. Figure 5 (v8 Figure 4) marks the dips, labels the −10 dB line and the margins (6.5 and 1.9 MHz). A new Table 1 lists every named object (patch, ground, board/slab, ±y/±s edges, rim, probe/feed, air margin/UPML, sagitta, chart, mesh morphing/guard, dip, band edges/margins, worst-of-band/level, EM solve, L1/P2/L1/refinement levels, space mapping δ, designs A–C) with the figure or table that shows it. "Rung" is replaced by "refinement level" (Table 2); the space-mapping chain and re-basing are defined where first used (§6); Design A's first mention in §4 points to Table 4. |
| The solves-to-level chart needs the levels (second legend?) | Figure 4(a): labelled horizontal lines at the target levels of Table 3 (−9, −9.5, −10 dB), with open circles and labels where the adjoint (35), the fastest gradient-free run (385, separable CMA-ES seed 2) and the finite-difference control (1,365) first reach −9.5 dB; the caption says how to read solves to target off the chart. Figure 4(b): a second legend for the line styles (target −9.5 dB solid, −9 dB dashed) and the ratio labelled at each n. Axis labels say "EM solves". |
| Refinement probes of new designs at different grid sizes | **Deferred.** This is new experiment work (scatter/refinement probes of new designs across mesh sizes), out of scope for a text-and-figures revision; not run. |

## Other changes

- Abstract: the radiation-efficiency sentence moved out (it stays in §7) to keep the abstract
  under the 240-word gate after the cost-unit definition was added; the "perfectly conducting
  (PEC) conductors" wording moved to §7 with it.
- Figures renumbered: v8 Figures 2–4 are v9 Figures 3–5 (scripts and PDFs renamed to match);
  tables shift by one after the new Table 1. `provenance.md` notes the shift and adds (v9) rows.
- `verify_evidence.py`: new `check_v9_reader()` (cost-unit numbers, guard constants read from
  the Rust source, the plateau run, Figure 2 caption numbers, Table 1 entries, figure files and
  labels, no "rung"); title, abstract, §1, §7 and §11 checks rewritten for the new claim
  structure; v1–v8 now checked unchanged.

## Build and checks

- `papers/build.sh papers/conformal-antenna-diffopt/conformal-antenna-diffopt.9`: 22 pages (v8:
  19), 0 overfull boxes, no float-too-large warning, no undefined references or citations.
- `python3.14 -I verify_evidence.py`: PASS. Ten mutations of the new text, each killed: 2.9× →
  3.0× (abstract ratio); 3.3-fold → 3.4-fold; 6.6 → 6.7 MHz (claim margins); 0.25 → 0.20 (guard);
  25 → 20 halvings; 89 → 88 MHz (Figure 2 caption); "We do not claim ISM coverage" → "We claim";
  seed 2 → seed 3 (Figure 4 caption); 150,561 → 150,541 unknowns; the old "Solve Counts" title.
- Figures: all five scripts run under `python3.14 -I` and pass the layout checks of
  `figure_style.py` (smallest print font 8.09 pt); each page was rendered and inspected.
