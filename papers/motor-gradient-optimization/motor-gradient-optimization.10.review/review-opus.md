# paper-review: motor-gradient-optimization.10 (origin/main c08ce9e6)

Reviewer: Opus (anvil:paper-review role, read-only). The reviewer acts as an independent, skeptical
referee from computational electromagnetics and electric-machine design optimization. This is a
fresh score of v10. The v7 scores (Opus 37/44, Fable 37/44) do not carry over. Earlier `.review/`
and `.audit/` siblings and the v8, v9 and v10 changelogs were read only to check whether earlier
findings were resolved.

Rubric: `.anvil/skills/paper/rubric.md`, 9 dimensions, **/44, advance threshold ≥35**. Voice:
`.anvil/skills/paper/voice.md`. Declared audience (BRIEF frontmatter): "Computational
electromagnetics, electric-machine optimization, and differentiable scientific computing
researchers". No `venue` in `.anvil.json`, so no venue overlay was scored. The thread has no
litsearch sibling, so D4 is scored on the legacy baseline, with no deduction for the missing
sibling.

Nothing under `papers/` or elsewhere in the repo was modified. All builds and verifier runs used a
`cp -R` copy at `scratchpad/motor-v10-review/opus-work/`. `diff -rq` between the copy and the
original was empty after `verify.py` ran. `git status` showed only the pre-existing untracked
`reports/` and `research_notes/`.

## Verdict

- **Total: 35 / 44**
- **advance: true.** The total meets the threshold exactly and no critical flag is set. The margin
  is zero. Fix R1–R4 before `paper-audit`: R1 and R2 concern what the title and the D/A
  interpretation claim, and an auditor could reasonably escalate either one.
- **Critical flags: none** (see the critical-flag check below).
- **Outstanding dependencies: none.** `pending_marker` found 0 markers.

| # | Dimension | Weight | Score |
|---|---|---|---|
| 1 | Rigor of method / argument | 6 | 5 |
| 2 | Evidence sufficiency | 6 | 4 |
| 3 | Clarity of contribution | 5 | 4 |
| 4 | Related-work positioning | 5 | 4 |
| 5 | Reproducibility | 5 | 4 |
| 6 | Figure & table quality | 4 | 4 |
| 7 | Prose & structural quality | 4 | 3 |
| 8 | Citation hygiene | 5 | 4 |
| 9 | Rhetorical economy | 4 | 3 |
| | **Total** | **44** | **35** |

## Mechanical checks

- **artifact_verify** (`.anvil.json`: `python3 verify.py`, cwd `.10`) **exits 0** in 0.27 s with
  seven PASS lines. The last two are "Exp 2 inventory binding (100 traces), recomputed target
  credit, paired ratios, repeat noise, checkpoint and abstract numbers" and "Exp 2 final refinement
  (33 designs, 213 checks)". `python3 -I -B verify_exp2.py` **exits 0** with two PASS lines.
- **Build**: `papers/build.sh` on the copy converged in 3 pdflatex passes. The final `main.log` has
  0 overfull boxes and 0 undefined citations or references. The PDF is 26 pages. The rebuilt
  `main.bbl` is byte-identical to the committed one.
- **audience_check**: 0 hits (governance 0, private locator 0, unlinked path 0).
- **numeric_consistency**: 1 warning at main.tex L452. "0.27\% higher" was paired with "from 0.01071
  to 0.01220". This is a **false positive**: 0.08036/0.08014 − 1 = 0.27% is correct, and the
  half-current value is 0.03961/0.03947 − 1 = 0.35%, also correct. The
  `% anvil-lint-disable: numeric_consistency` comment sits on L450, two lines above the claim, so it
  does not suppress it. Nit: move the comment to L451.
- **Manual claim-vs-claim pass over §7** found no discrepancy. I checked:
  - 24 + 21 = 45 optimizer pairs, and stop reasons 38 + 46 + 13 + 3 = 100.
  - 111 repeat-1 crossings (23 + 21 + 25 + 23 + 19), 25 refined crossings and 33 designs
    (14 + 19), with 16 + 15 + 2 = 33.
  - Three post-checkpoint crossings (B 90.4 and 62.3, D 83.3) and the D/A near tie
    (7.3/7.4 = 0.99).
  - The held-out ranges: 0.0144/0.0223 = 0.65 and 0.0076/0.0044 = 1.73.
  - The B/A geometric mean for space A at T50, recomputed as 3.25.
  - The abstract's coarse-mesh saving: 1/0.79 to 1/0.45 gives 1.3–2.2.

## Critical-flag check

- **Citation error**: none. All 19 keys resolve. The v10 Dolan–Moré and Moré–Wild attribution fix is
  correct: performance profiles go to Dolan and Moré, data profiles to Moré and Wild.
- **Numerical inconsistency**: none found. Text, Tables 5–7 and both verifiers agree.
- **Missing experiment for a claim**: considered for R1 and R3 but **not set**. The time-to-target
  claim is judged against the common validation model, so the speed result itself does not depend
  on gradient verification. Gradient-access attribution is disclaimed in the body. Both are
  required wording or evidence fixes, not blockers.
- **Close prior work ignored**: none at flag level (see D4 leads).
- **Build / artifact**: clean.

## Required findings (fix before audit)

**R1. The title credits "verified adjoint gradients", but the body says gradient access is not
isolated.** Title: "Verified adjoint gradients reach validated torque-ripple targets sooner in a
nonlinear motor model". optimization.tex L22–24 says: "The optimizer contrast includes differences
in search strategy, so it does not isolate gradient access as the only causal difference." The
voice rule is: "Do not credit a result to the shape derivative or autodiff unless an ablation shows
that this component is what produced the result."

No arm runs SLSQP with finite-difference gradients, and the paper offers no counterfactual for one.
The experiment measured that SLSQP driven by adjoint gradients beats COBYQA at matched resolution.
Two fixes are possible; either is acceptable:
- (a) Retitle along the lines of "Adjoint-driven SLSQP reaches validated torque-ripple targets
  sooner than derivative-free search in a nonlinear motor model". Mirror the abstract's
  "adjoint SLSQP".
- (b) Add an FD-SLSQP counterfactual. Take each A/E run's number of gradient requests from the
  traces and multiply by the measured FD-to-adjoint cost ratio. Table 4 gives about 9× at
  n = 12, and the factor would be roughly 2n/1.2 at n = 8 and n = 18. Report it as an estimate.

Option (b) would also turn "verified gradients pay off" into a measured statement about the adjoint
rather than about SLSQP.

**R2. The charged clock is dominated by validation charges, and the D/A interpretation depends on
that.** Rule: "Changed incumbents are validated as they appear" (optimization.tex L86–87). Each
validation is an L2/order-93 cold forward of about 1 unit. In `runs.json`:
- The median gap between consecutive validations is 1.07–1.43 units per method.
- Repeat-1 runs perform up to 65 (space A, method A) and 127 (space B, method E) validations.
- Crossings occur at validation counts of 2–8 for A against 5–36 for B.

Most of each time to target is therefore the validation charge, not training cost. As a crude
estimate, I subtracted a fixed c units per validation up to the crossing. The same-resolution
contrasts grow:
- The space-A B/A geometric mean rises from 3.25 to about 7.2 at c = 1.
- The space-B D/E geometric mean rises from 3.14–7.20 to about 13–41.

The headline is therefore conservative. The **space-A D/A** contrast is fragile: at
c = 0.8 / 0.9 / 1.0 the geometric means are 1.42 / 1.23 / 0.83 at T50 and 1.35 / 1.18 / 0.82 at
T90. A's residual non-validation time is only about 0.5 units, so the sign depends on the
validation cost. The traces carry per-reply `charges` categories, so the authors can compute this
exactly.

Required:
- Add one sentence or a small table giving, per method and space, the median number of validations
  to each crossing and the share of charged time spent in validation.
- Qualify the space-A D/A sentence (optimization.tex L176–178) and the Discussion clause "and the
  coarse derivative-free competitor D does not close that gap" (main.tex L794–795). Make clear that
  in space A this advantage reflects how many validated incumbents each method needs under the
  validate-every-incumbent rule.
- State that the same rule makes the B/A and D/E ratios conservative.

**R3. The gradients the optimizers actually used are not verified in the paper.** §6 verifies the
12 scalar parameters at 10 angles under the earlier fixed-unscaled-current rule, with ScaleR, LENGTH
and SD1 free. It also says: "The present gradient comparison does not include derivatives of those
additional groups or a new optimized motor." (main.tex L612–614). That sentence is now
contradicted by §7. Space B's A and E runs differentiate ten grouped offsets on 55 control points
through a different mesh-motion map (optimization.tex L56–57). The training model uses 20 angles
and fixed physical current.

The verification exists in the repo but not in the paper:
- `benchmarks/motor_optimization/README.md` §"Finite differences" (`runs/fd_L1_space_{a,b}.json`)
  checks the training model (h45, L1, g20, Newton 1e-12) in both spaces at θ0 and an interior point.
- Directional errors are 1.4e-8 to 2.8e-7, and the offset components CPR1/CPS1 are at about 1e-6.
- CI repeats the check at L0.

Required: add two or three sentences, or a row in §7.1, reporting this check with a pinned link.
Delete or rewrite the stale L612–614 sentence. Without this, "verified" in the title and abstract
covers only space A.

**R4. Several stale sentences predate §7 and now contradict it.**
- main.tex L636–637: "A future optimization comparison must hold this policy fixed across
  methods." §7 has done this (fixed physical current; optimization.tex L30–33). Replace with a
  pointer.
- main.tex L612–614: see R3.
- main.tex L757–759: "Shape-map setup is recorded separately and must be included in a complete
  optimization budget." State that §7 charges it, if it does (evaluator start-up and mesh motion are
  charged per L93–95); otherwise state that it was excluded.

## Advisory findings

- **A1 (D4; §2).** These are leads for a `paper-litsearch` re-run, not citations to add blind, since
  the resolver has not verified them:
  - Lyu, Xu & Martins (2014), "Benchmarking optimization algorithms for wing aerodynamic design
    optimization" (ICCFD8).
  - Zingg, Nemec & Pulliam (2008), "A comparative evaluation of genetic and gradient-based
    algorithms applied to aerodynamic optimization" (Eur. J. Comput. Mech. 17).

  Both report gradient-based adjoint optimization beating gradient-free search with matched budgets
  in shape optimization, with the gap growing with dimension. The headline's direction is therefore
  expected, and the paper should position its contribution as a measured, validated instance in
  nonlinear motor design. The closest nonlinear-magnetostatic motor shape-derivative work, Gangl,
  Langer, Laurain, Meftahi & Sturm (2015), "Shape optimization of an electric motor subject to
  nonlinear magnetostatics" (SIAM J. Sci. Comput. 37(6)), is also absent. Fable's v7 m6 raised the
  first lead; v8 added only Moré–Wild and Dolan–Moré.
- **A2 (D8).** Software is cited inconsistently. SciPy is cited, but GeoPDEs, the independent
  reference solver behind the 12.9% → 0.10% result ("The run uses Octave 11.3.0, GeoPDEs revision
  \texttt{53ae5ca1} and nurbs 1.4.3", L440–441), and Gmsh ("which Gmsh triangulates differently on
  the two platforms", optimization.tex L241–242) are not. Add de Falco, Reali & Vázquez (2011) and/or
  Vázquez (2016) for GeoPDEs and Geuzaine & Remacle (2009) for Gmsh after resolver verification.
  This deduction is the reason D8 is 4.
- **A3 (D5).** "The paper package contains the per-run validation histories..." (L872) gives no
  locator. Link `papers/motor-gradient-optimization/motor-gradient-optimization.10/` at a pinned
  commit, or the Zenodo DOI once deposited. The preregistration is a mutable GitHub issue. A
  timestamped export already exists in `../refs/main-study-methods-protocol-sources.json` (governing
  comments, `updated_at`). Copy it into the package and cite it next to the
  `\href{...issues/1185}`.
- **A4 (D5).** Put the optimizer settings a replicating group needs in a short appendix table:
  SLSQP ftol 1e-15, maxiter, the restoration rule and its 20-restart cap; C's cubic RBF with linear
  tail, Δ0 = 0.1 and subproblem limits. They are currently only in the pinned harness README.
- **A5 (D7; optimization.tex L233–235).** "the capture tool repeatedly refused on host-boot
  attestation details unrelated to the data" is internal-tool language. Say what the registered step
  was ("capturing the outputs on the original host") and that it could not be completed after the
  restart. Also, "they do not certify each of the study's 111 repeat-1 target crossings" (L229–230):
  the voice guide avoids "certify"; use "they do not check".
- **A6 (D9).** The paper is 26 pages with 9 figures and 7 tables; the BRIEF targets 10–15 pages and
  5–6 figures. The headline result first appears on p. 15. Candidates to move to an appendix or
  supplement:
  - The Opt120 size and current-normalization paragraph (main.tex L531–550).
  - The angular-aliasing paragraph (L566–573).
  - The estimator-study paragraph (L427–433).
  - Table 3.

  Also:
  - The abstract's last 60% is forward and derivative detail. "Retaining every order below the trace
    Nyquist limit reproduces conforming torque" is an implementation check that does not need
    abstract space.
  - The Conclusion spends its first 15 lines on Opt120 normalization ("Most of their saved-size
    deficit therefore comes from the size factor") before the matched-study result. Lead with §7.
- **A7 (D2, linking the two halves).** §5.5 conjectures that "optimization against order 45 favors
  cancellation within that truncated model". §7 trains at order 45 and validates at order 93, so it
  can test this directly. Report F_train(h45, L1)/F_val(h93, L2) at the selected designs, especially
  space B's best designs, which reach 0.004–0.009 F0. It costs nothing and connects the paper's two
  halves.
- **A8 (D3).** The introduction opens with "A small error in mean torque can conceal a large change
  in the quantity being optimized." That framing serves §§5–6, not the title. Consider opening with
  the decision problem (when gradients pay off) and placing the interface finding as the reason the
  validation model must differ from training.
- **A9.** Abstract: "A coarser training mesh saved only 1.3–2.2 times" reads awkwardly. Suggest "made
  runs only 1.3–2.2 times faster".
- **A10 (provenance wording, optimization.tex L236–238).** The timestamps prove the inventory existed
  by 2026-10-20 03:50Z, not that it was taken before the restart. The README states this correctly;
  the manuscript should match it in one clause.

## Per-dimension justification (quoted evidence)

**D1 Rigor 5/6.** The design is sound and preregistered:
- A common validation model that differs from training: "Every method is judged on the same
  order-93, L2, 20-angle validation model" (§7.1).
- Censoring is respected: "no finite ratio is derived from a censored cell" (§7.2).
- All costs are charged: "nothing is subtracted as overhead" (§7.1).
- The deterministic repeats are measured, not assumed: "the comparison is a paired case study over
  five starts" (§7.2).
- The post-run deviation is disclosed (L7–10).

One point is deducted for R2. The validate-every-incumbent rule ("Changed incumbents are validated
as they appear, after the initial interpolation set for B, C and D." — §7.1) makes validation count
the dominant cost. The paper neither decomposes it nor discusses how it shapes the cross-resolution
contrast. The confound statement "The optimizer contrast includes differences in search strategy,
so it does not isolate gradient access as the only causal difference." (§7.1) is present and
correct.

**D2 Evidence 4/6.** Strengths:
- 45 of 45 optimizer pairs go one way, with a minimum of 1.46 against ≤12.4% repeat noise: "All 45
  finite optimizer-contrast pairs (by space, target and start) favor SLSQP, with ratios from 1.46
  to 21.5." (§7.2).
- 213 refinement checks were run.
- Both verifiers recompute every reported number.

Deductions:
- (i) R3: the space-B gradients actually used are unverified in the paper, and the paper itself
  says "The present gradient comparison does not include derivatives of those additional groups or
  a new optimized motor." (§6.1).
- (ii) R2: no time decomposition, so the D/A result in space A rests on unreported mechanics.
- (iii) The strongest competitor class in the BRIEF is untested: "No reduced physical model, such as
  a magnetic equivalent circuit or a waveform model, was compared" (§8). This is acknowledged, and
  five starts on one motor bound generality.

**D3 Clarity 4/5.** The contribution is stated with numbers in both the abstract and the
introduction: "the contribution is the measured evidence linking verified gradients to time to a
validated target" (§1). The title is result-based, as the BRIEF asked. One point is deducted
because the title over-credits the component (R1): "Verified adjoint gradients reach validated
torque-ripple targets sooner in a nonlinear motor model" attributes to the gradients what the body
attributes to "adjoint SLSQP" with an acknowledged search-strategy confound. In space B, "verified"
is not yet supported in the paper.

**D4 Related work 4/5.** The electromagnetic positioning is accurate and engages the closest work on
its merits: "Thus torque sensitivity to interface order is already established; our study
quantifies its effect on loaded nonlinear ripple" (§2). Wiesheu, Merkel, Scotzniovsky, Egger,
Bontinck and De Gersem are each placed with an "extends" or "complements" framing. The benchmarking
methodology is now cited (Dolan–Moré, Moré–Wild), and Ion et al. is linked to §7. One point is
deducted (A1): the paper does not engage prior matched gradient-versus-gradient-free shape-
optimization benchmarks, whose findings make the headline direction expected, nor Gangl et al.
(2015) on nonlinear magnetostatic motor shape derivatives. This does not reach flag level, because
the paper claims no novelty for the method: "The numerical methods are established" (§1).

**D5 Reproducibility 4/5.** Much is strong:
- Pinned commits for every evidence tree, including the harness and the refinement source.
- A SHA-bound, RFC 3161 and OpenTimestamps-stamped inventory, and the session and calibration
  records.
- Stated versions (SciPy 1.18.1, NumPy 2.5.4, OpenBLAS, Octave, GeoPDEs).
- Two passing stdlib verifiers.

"The paper package contains the per-run validation histories, the 213 final-check results"
(Code and data) is accurate. One point is deducted because that package has no locator in the paper,
and the preregistration is cited only as a mutable issue (A3). Optimizer settings beyond COBYQA's
npt and radii are not in the paper (A4).

**D6 Figures/tables 4/4.** The v10 changes resolve the v9 vision findings, verified in my render at
110 dpi:
- Fig. 8 now has a log time axis, the full y-range, a budget line, and a distinct dash-dot for C.
- Fig. 9 uses filled and open markers by mesh and spreads the crosses.

The captions are self-contained: "Each curve ends at the run's actual stop (tick). A missing curve
means the run had no credited validation." (Fig. 8). Table 7 is dense (scriptsize) but legible, and
its columns are scaled sensibly. No overflow or clipping in the PDF.

**D7 Prose & structure 3/4.** The voice rules hold: no virtue words, no "load-bearing", 0 em-dashes,
and audience_check finds 0 governance hits. The build is clean with no `??`. One point is deducted
for stale cross-section statements that contradict §7. "A future optimization comparison must hold
this policy fixed across methods." (§6.1), together with the R3/R4 sentences, makes the paper
contradict itself. The internal-tool phrasing "the capture tool repeatedly refused on host-boot
attestation details unrelated to the data" (§7.2) is not addressed to the declared audience.

**D8 Citation hygiene 4/5.** The bibliography is complete: DOIs on all journal and conference
entries, and the Ragonneau thesis identifier was added in v10. The v9 source-level check was applied.
One point is deducted for uncited software on which headline results rest (A2): "The run uses
Octave 11.3.0, GeoPDEs revision \texttt{53ae5ca1} and nurbs 1.4.3" (§5.4) and "which Gmsh
triangulates differently on the two platforms" (§7.2). SciPy, in contrast, is cited.

**D9 Rhetorical economy 3/4.** §7 is tight, and hedging there is proportionate; the COBYQA
mechanism, for example, is labelled: "We conjecture that part of the larger space-B ratios comes from
COBYQA's initial interpolation set" (§7.2). One point is deducted for length and lede placement
(A6). The paper runs 26 pages against a 10–15-page target, and the matched-study result begins on
p. 15. The abstract spends its second half on implementation-level forward checks ("Retaining every
order below the trace Nyquist limit reproduces conforming torque" — Abstract). The Conclusion opens
with Opt120 size normalization ("Most of their saved-size deficit therefore comes from the size
factor" — §9) before the result in the title.

## Resolution of earlier findings (checked, not carried over)

| Earlier finding | Status in v10 |
|---|---|
| v7 Opus R1 / Fable m1: "every credited crossing" scope | Resolved: "every selected target crossing", with −4.0% to +1.1% for the 25 crossings and −4.0% to +6.3% for the 33 designs |
| v7 R2 / Fable M4: "45 paired starts" unit | Resolved: "45 paired comparisons where both succeeded" |
| v7 R3 / Fable M1: deviation misdated | Resolved (optimization.tex L4–10) |
| v7 R4: Jinit control unreported | Resolved: stated as not run, with a pointer to §6 |
| v7 R5 / Fable M2: eligibility, stop reasons, 5° policy, channel wall | Resolved (§7.1) |
| v7 R6: space-A plateau over-attributed | Resolved: mean floor active, "within this budget and these methods"; v8 audit C1 (C at 0.049) fixed in v9 |
| Fable M3: unpinned harness, missing session records | Resolved: `9486a9da`, `3e4ee0c6`, `provenance/setup/` |
| Fable M4: buried headline, no number in introduction, working title | Partly resolved: the numbers are in the abstract and introduction and the title is result-based; the lede still sits behind ~15 pages of §§3–6 and the title over-credits (R1, A6) |
| Fable m6: gradient-vs-gradient-free benchmark literature | Partly resolved: Moré–Wild and Dolan–Moré added; the Lyu/Martins-type shape-optimization comparisons are still absent (A1) |
| v7 Opus D5(iii): `verify_exp2.py` outside artifact_verify | Resolved in effect: `verify.py` now runs the Exp 2 blocks (two PASS lines) |
| v9 vision 1–5, 7, 8, 11 | Resolved in the render; the deferred nits 6, 9 and 10 carry no information loss |

## Procedural notes

- Read `main.tex` (all 912 lines), `optimization.tex` (all), all `tables/*.tex`, `refs.bib`,
  `changelog.md`, `README.md`, `../BRIEF.md`, `../.anvil.json`, the v7 Opus and Fable reviews, and
  the v8/v9 changelogs. Also read `benchmarks/motor_optimization/README.md` (FD section) and
  `harness/README.md` (settings), the L0 smoke gradient records, and `../refs/` listing and protocol
  sources.
- Own analysis on `figures/src/data/exp2/runs.json` (read-only): validation counts, inter-validation
  gaps, and validation-adjusted ratio sensitivity for R2. These are estimates under an assumed
  per-validation charge, not results.
- The L0 space-B smoke gradient records marked `failed` at θ0 are box-bound stencil refusals; the
  boundary variants pass. Not a finding.
- No network requests. No email or identity data sent anywhere. Scratch files are under
  `scratchpad/motor-v10-review/opus-work/` and `png-opus/` only.
