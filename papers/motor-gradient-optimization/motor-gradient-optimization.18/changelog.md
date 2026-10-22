# Revision 17 → 18

Inputs:
- The operator's v18 instructions: a reader-clarity revision for Crutcher Dunnavant, a tensor-compiler and ML-systems engineer who is not a motor or EM-FEM specialist, plus the coauthor ruling of 2026-10-22.
- A pre-read of v17 through his eyes. It lists 20 items: 1–7 MUST, 8–17 SHOULD, 18–20 NIT. Each claim was checked against the paper and the records before acting on it.
- His feedback on the sister antenna paper.

No solver or optimizer was rerun. V17 and earlier versions are unchanged.

**No measured result changed.** Every new number comes from the committed extracts and timelines:
- the decomposition factors;
- the time split and gradient shares;
- the scaling ratios;
- the cost-table ranges;
- the unit in minutes;
- the crossing counts.

`verify_exp2c.py` recomputes all of them in a new v18 block. That block does not import the renderer.

**Outstanding dependencies:**
- `[PENDING zenodo-doi]`: carried forward. The operator's deposit of the raw-trace archive is unchanged, 2,790,582,024 bytes, SHA-256 `1dfbf388…a1b98b07`.
- `[PENDING coauthor-affiliation]`: new, by the coauthor ruling.

**Results (measured on the rebuilt PDF):**
- **Build:** 42 pages (v17: 38), 0 overfull boxes, 0 undefined references or citations. The main text is pp. 1–18 (v17: pp. 1–14):
  - Conclusion pp. 17–18;
  - Author contributions and Code and data availability start on p. 18.
  - The growth comes from three new tables (methods, parameter sets, costs), the decomposition table and figure, and the larger Fig. 1.
- **Abstract:** 249 words by the verifier's count (limit 250).
- **Verifiers (extract mode):**
  - `verify.py`: 21 PASS, exit 0; it chains the other three.
  - `verify_exp2.py`: 4 PASS.
  - `verify_exp2b.py`: 6 PASS.
  - `verify_exp2c.py`: 7 PASS (adds "v18 decomposition …").
- **Lints:**
  - `numeric_consistency`: pass. The only finding is the known suppressed 0.01071/0.01220 pair.
  - `audience_check`: pass, 0 hits.
  - `pending_marker`: lists `coauthor-affiliation` and `zenodo-doi`, both expected.

| Item | Finding | Resolution |
|---|---|---|
| 1 MUST | Space A/B collides with method A/B | The spaces are now **S8** and **S18** everywhere: text, figures and tables, including table headers and figure row labels ("S8 (8 parameters)"). Method letters are kept for methods only. `verify_exp2c.py` asserts that no "space A/B" remains |
| 2 MUST | Objective F collides with method F/F_w | The arms are now **FD-cold** and **FD-warm** everywhere, including figure legends, Fig. 5 row labels and table rows. *F* now means only the objective. The records keep the names `F`/`F_w`, and the data statement and README say so. **Declined:** renaming *T* (target vs torque) and θ0. Targets are always written T50/T90/T99 and torque is always T̄ or σ_T. θ0 is now defined at first use as the reference's published initial design |
| 3 MUST | No diagram for the parameters and offsets | **Fig. 1 rebuilt** (`figures/src/render_geometry.py`, from the committed mesh and the newly copied `cp_table.json`, hash-bound in the evidence manifest). Panels:<br>(a) the sector with labelled parts and MAG/MH/MW arrows, with winding group 1 hatched;<br>(b) one slot turned upright, with SW1–SW4 and Sr1;<br>(c) the 55 control points by polar angle, coloured by their 10 groups, with each group's direction and limit.<br>The caption defines every parameter. The text now also defines the NURBS patch and the winding groups (two coil sides per slot). Table 3 shows the three parameter sets (item 12). "Slot-opening channel wall" is now just "S18's slot-opening channel wall", next to the figure that shows the opening |
| 4 MUST | Solve, forward, evaluation and unit are undefined | New paragraph "Budget, timing and cost vocabulary" in §4.1. It defines:<br>- *forward*: 60 nonlinear field solves, 3 currents × 20 angles;<br>- *cold* and *warm* forwards;<br>- *stencil solve*;<br>- the unit: the median cold validation forward, 66.66–70.94 s.<br>New **Table 4** gives each operation's cost in units:<br>- validation 0.87 (0.55–1.10);<br>- training forward 0.216–0.245;<br>- adjoint 0.054–0.061;<br>- FD-cold gradient 1.90 and 4.25;<br>- FD-warm gradient 0.97 and 2.36.<br>"Training solve" and "validation solve" became "training forward" and "validation forward". §6 notes that its derivative-cost forwards are a different, ten-angle multithreaded sweep |
| 5 MUST | 18–70× per-gradient ratio vs 1.7–4.1× end to end | New **Fig. 6** and the paragraph "Where the time goes". The mean time to the primary target is split into validation, gradient and the rest for A′, FD-warm, FD-cold and B (`render_decomp.py`).<br>- A′'s adjoints take about 5% of its time.<br>- FD-warm's stencils take 45%/65%; FD-cold's take 62%/77%.<br>- All SLSQP arms spend 5.3–5.4 (S8) and 8.0–8.1 (S18) units on validation and the rest.<br>- A free gradient would shorten A′ by only 5–6%.<br>The abstract carries the share sentence. A′'s adjoint slice is labelled as an estimate (evaluations × the run's mean adjoint charge per evaluation), because adjoint time is recorded per run |
| 6 MUST | The "comparable factors" split claims and then denies | The decomposition is now the finding:<br>- **Table 5** gives B/A and both splits (FD-warm and FD-cold) over the starts where B attains the target. Gradient factor 1.71–2.47 (S8) and 2.68–4.10 (S18); optimizer factor 1.22–1.76 and 1.91–2.92.<br>- The abstract, introduction (paragraph 2), §5, Discussion and Conclusion all state this one claim. Each range is the gap between the two finite-difference baselines.<br>- "Comparable factors" is gone from the paper and the README; the verifier asserts its absence.<br>- The old §5 "optimizer factor" paragraph and its display were folded into "Gradient factor and optimizer factor" |
| 7 MUST | Figs. 3 and 4 lack target labels and start identity | **Fig. 3** has:<br>- target labels at the right edge;<br>- "checkpoint" and "budget" on the verticals;<br>- a dot at each curve's first crossing of each target;<br>- row labels "S8 (8 parameters)" / "S18 (18 parameters)".<br>The caption now says the plotted quantity includes δF, and the Attained-quality text quotes both F/F0 and the plotted level. **Fig. 4** gives one marker shape per start, with a legend (θ0, LHS 1–4, open = L0, cross = not attained), and panel titles "S8, T50", etc. |
| 8 SHOULD | Interface order never explained plainly | §3.2 now explains:<br>- the separately meshed rotor and stator meet on a circle and the rotor mesh turns;<br>- the harmonic model matches only Fourier orders up to N_h, "like a low-pass filter";<br>- a higher order passes more, and the conforming interface passes all that the mesh resolves.<br>The abstract now says "a finer validation model" instead of the order-93 clause |
| 9 SHOULD | "Training" and "validation" read as ML terms | New first Protocol paragraph, "Training and validation models":<br>- the optimizers query a cheaper training model;<br>- each incumbent is rescored on one validation model;<br>- training means querying, not fitting; apart from C's surrogate, nothing is fitted.<br>It also explains why validation is charged to the run, and points to §4.2 for its share |
| 10 SHOULD | "Primary target" used before it is defined; targets confounded | The abstract defines the primary targets as 90% and 99% reductions with 8 and 18 parameters. §4.1 defines T_x as an x% reduction and names the primary targets. §5 gives the common-target comparison at T90: FD-warm/A′ is 1.71 vs 2.59 and FD-cold/A′ is 2.47 vs 3.86. It also says the spaces differ in mesh-motion map |
| 11 SHOULD | Scaling rate with n | §5: n grows 2.25-fold and the per-gradient ratio 2.2-fold (warm) and 2.1-fold (cold). End to end it grows 1.6-fold (1.71 → 2.70) and 1.7-fold (2.47 → 4.11). "Two spaces do not determine a rate, and we do not extrapolate." The Discussion repeats this |
| 12 SHOULD | Three parameter sets; θ0 and Opt120 undefined | New **Table 3**: the 12-parameter gradient-check set vs S8 vs S18, marked free/fixed/—. θ0 is defined in §4.1. Opt120 is defined in §3 as the reference's saved optimized design |
| 13 SHOULD | One table for all methods | **Table 2** extended with a Gradient column and rows for FD-cold, FD-warm and A′. The caption explains sessions and repeats. The Fig. 5 caption now states that the FD-cold ratio row divides by the FD-cold session's A′, not the plotted A′ marker |
| 14 SHOULD | Fig. 5 ratio rows unlabelled; mixed units | Left-edge row labels "FD-warm/A′" and "FD-cold/A′", coloured like their markers. The caption gives "1 unit is 1.11 to 1.18 minutes" |
| 15 SHOULD | Scope of "every selected crossing" | Abstract: "Every target crossing selected for final checks, 25 of the main study's 111 and 10 in each ablation, kept its target on a finer mesh." The counts are recomputed from the three `refinement.json` role lists |
| 16 SHOULD | "A beats D", then withdrawn | §4.2 now leads with the mechanism. D needs more validated incumbents (a median of six against two), and each costs a full validation. As charged the ratio is 1.68–1.83; without validation it is 1.15–1.16. The S18 figures follow (2.25–4.32; 1.92–3.19). The duplicate sentence in "Validation charges" was removed |
| 17 SHOULD | Why ripple, and why the mean floor | §3.3: ripple is a source of vibration and acoustic noise, and the reference minimizes it. A design could lower ripple by giving up torque, hence the floor. §3.1 gives what the current fractions mean (a = 0 magnets only, i.e. cogging; 0.5 and 1 loaded) |
| 18 NIT | Torque estimator pairing | One-clause glosses of Arkkio (Maxwell stress averaged over the air-gap annulus) and coupling torque (energy derivative through the interface constraint) added in §3.2. **Declined:** the suggested 3-row table. The existing sentence already lists the three pairings, and a table would add a float for three facts |
| 19 NIT | Dense gradient-accuracy paragraph | The §5 paragraph is cut to the conclusion: max-norm parity, component-wise failures on small components, one sign flip, a shared offset that a central difference removes, and the final-check margin. The per-component numbers (SW1/SW4/Sr1, the 1.58 sign reversal, the SW3 per-unit-step error, the −3.11×10⁻³ common error, the −3×10⁻⁹ offset) moved verbatim to App. D, "Warm-stencil arm" |
| 20 NIT | Small undefined terms | - "Target crossing" is defined in §4.1.<br>- "L0 (coarsest) to L3 (finest)" appears at first use.<br>- LHS is spelled out in §4.1 and in the Fig. 3 caption.<br>- The abstract now reads "all 45 of the 50 paired comparisons".<br>- On "per core", the introduction says "on one core per run, with the stencil solved serially", and §5 explains that a parallel stencil would shorten wall time but not compute. **Declined:** the abstract clause on parallel stencils (word limit; "on one single-threaded core per run" stays). **Not changed:** "refinement interval", which is defined in App. E and referenced where used |
| Coauthor (operator ruling 2026-10-22) | Add Crutcher Dunnavant | Authors are Robb Walters, then Crutcher Dunnavant. His affiliation is `[PENDING coauthor-affiliation]`, with no email. New "Author contributions" section: "C.D. wrote the four-layer operator specification that GEODE realizes [Dunnavant 2026, github.com/crutcher/palace_whiteroom], whose layers are unrelated to the mesh levels L0–L3, and gave design input and review. R.W. ran the work and the software agents that carried it out and is responsible for the results." The paper writes "four-layer" instead of "L1–L4" because L0–L3 already name mesh levels here. The bib entry is copied from `conformal-antenna-diffopt.8/refs.bib`. v17 had no third-person crediting section to convert. `verify_exp2b.py` now expects exactly the pending markers {zenodo-doi, coauthor-affiliation} |
| Package | Package link and verifiers | `\href` and verifier constants changed from `.17` to `.18`. README rewritten for v18 (state, authors, interpretation, rebuild steps with `render_decomp.py`, the new inputs). The evidence manifest gains `cp_table.json` (130 files). The `verify_exp2c.py` README checks now assert the decomposition phrases instead of "comparable factors" |
