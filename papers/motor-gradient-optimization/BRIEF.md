---
title: "Adjoint Gradients for Torque-Ripple Optimization of a Nonlinear Motor"
slug: motor-gradient-optimization
skill: paper
paper_number: 3
documentclass: anvil-paper
anonymous: false
status_of_inputs: "Main study (#1185, 100 runs) and post-hoc ablations F (#1218, cold stencils) and F_w (#1224, warm stencils) complete and hash-bound; current package motor-gradient-optimization.22 (v21 revised for reading flow: gradient verification before the optimization study, terms defined before use, one model sentence, no number changed; not reviewed; paper DOI 10.5281/zenodo.23580548, raw-trace archive DOI 10.5281/zenodo.23580550); v16 reviews 42/44 and 40/44"
claim: "Test whether exact field gradients reduce time to a validated motor-design target beyond calibrated simple models, and how that advantage depends on useful shape freedom."
audience: "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers"
venue_target: "Select from the validated main-study contribution; use the generic paper class initially"
length_target: "Approximately 10-15 pages with 5-6 main figures; adjust to the evidence and venue"
web_search: true
public_repo_url: "https://github.com/rjwalters/geode-fem"
science_card_id: 3
---

# Third paper: nonlinear motor optimization

The operator designated this work as GEODE's third paper on 2026-10-16,
alongside the transmon and conformal-antenna papers. This brief establishes
the research project. The operator requested the first manuscript on
2026-10-16. **Current state (2026-10-23) — CURRENT:** the package is
`motor-gradient-optimization.22/` (not yet reviewed), v21 revised for reading
flow against the operator's flow review of v21 (geode-fem#1231). V22 changes
order and wording only: the design-gradient verification section now precedes
the optimization study and the ablations (Sections 4, 5, 6), so the adjoints
are verified before the results that rely on them; S8/S18, L0/L1, the three
studies, "stencil" and "arm" are introduced before first use; the two bases of
the S18 gradient factor (2.68/4.10 over the four starts COBYQA reached, as in
Table `tab:decomp`; 2.70/4.11 over all five) are bridged in one sentence of
the ablation section, and the abstract, introduction and conclusion quote the
five-start values; the coauthor's "model" clarification is one positive
sentence in the protocol (both simulations are physics simulations; of what
the optimizers use, only their own search models are fitted, to coarse
values); the A-versus-D validation story is told once; the per-session
coarse-forward medians moved to Appendix D; the ablation protocol is three
paragraphs; the coarse full-current mean is $\bar T_{r,1}$ (was $\mu_{r,1}$,
which collided with $\mu_r=1.05$). Skipped: the Appendix D split and the move
of the provenance text to a final appendix (both judged not clearly helpful
against the byte-level pins on the data statement and Appendix D), and the
review's suggested "below 4.2" and "about a third" paraphrases (new numbers).
No number, result, claim scope, title, author, reference or DOI changed;
figures, tables and the bibliography are byte-identical to v21. A v22 check in
`verify_exp2b.py` pins the numeric multiset of the sources against the v21
sibling (listed duplicates and deletions only, no new value), the preamble,
DOIs, citations and every table, figure and figure source. V21 was v20 with the
coauthor's two requested changes and nothing else:
- terminology: the optimizers' "training model" and the scorer's "validation
  model" are now the *coarse simulation* (L1, or L0 for D and E, interface
  order 45) and the *validation simulation* (L2, order 93, never seen by an
  optimizer), defined once in the protocol. Two sentences there say that an
  unqualified "model" is a formulation of the simulation, that no simulation
  is trained or fitted to torque data, and that of the optimizers' models only
  their own search models (C's surrogate, COBYQA's interpolation model) are
  fitted. Adjectives follow ("coarse forwards", "coarse resolution",
  "optimization mesh", "objective currents"); the appendix's cost model is
  named as one; physics and numerics uses of "model" are kept. Two figure
  legends are relabelled to match, with unchanged data;
- a short future-work paragraph closing the Discussion: second-order and
  gradient-enhanced population or multistart methods and parallel starts, all
  untested and bounded by validation and coarse forwards, and synchronous
  reluctance rotors as a next problem (citing gangl2022multi, already in
  `refs.bib`).

V20 was v19 with three changes:
- the operator's short title, "Adjoint Gradients for Torque-Ripple Optimization
  of a Nonlinear Motor", in `\title` and `pdftitle`. It replaces "Per core,
  adjoint gradients reach validated torque-ripple targets 1.7 and 2.7 times
  sooner than warm-started finite differences in a nonlinear motor model",
  whose factors the abstract states. The operator's choice supersedes this
  brief's instruction to use a result-based title;
- in the data statement, the raw-trace archive is "deposited at" DOI
  10.5281/zenodo.23580550 (it was "for deposit at");
- a sentence giving this paper's own DOI, 10.5281/zenodo.23580548, for its PDF
  and LaTeX source. This is the same sentence as the other two papers'
  Reproducibility footnotes, and it sits where the data statement gives the
  GEODE repository URL.

The `title:` front matter, which held the original working title, now holds
the new title. V19 was the v1 Zenodo release of v18 and changed only release
metadata: each author's own affiliation (Robb Walters, 2AM Logic; Crutcher
Dunnavant, ZSpaceLabs.ai), the paper's DOI 10.5281/zenodo.23580548 under the date, and
the raw-trace archive's reserved DOI 10.5281/zenodo.23580550 in place of
`[PENDING zenodo-doi]`. No pending marker remains. V18 was a reader-clarity
revision of v17 for a reader from tensor compilers and ML systems: design
spaces renamed S8/S18 and the ablation arms FD-cold/FD-warm, annotated
geometry, cost vocabulary, and a gradient/optimizer decomposition of the
headline; no measured result changed. V17 was a targeted revision of v16,
whose reviews scored 42/44 and 40/44 and whose audit was
READY-WITH-AUDIT-FLAGS on the unlinked package path. The package reports
the preregistered main study (#1185) and two post-hoc ablations under
protocols frozen before any run: FD-cold (F in the records), cold-started
forward-difference stencils (#1218), and FD-warm (F_w), warm-started
stencils (#1224). The raw-trace archive (2,790,582,024 bytes, SHA-256
`1dfbf388…a1b98b07`, unchanged since v17; the v16 archive
`a70a0699…203f12` is superseded) is deposited at DOI
10.5281/zenodo.23580550.
All earlier versions are unchanged.

**Current headline.** Per core, with SLSQP held fixed, adjoint gradients
reach the primary target 1.71 and 2.70 times sooner than warm-started finite
differences (8 and 18 parameters) and 2.47 and 4.11 times sooner than
cold-started ones. Of SLSQP's 3.10- and 7.79-fold advantage over COBYQA,
the gradient factor is 1.71--2.47 and 2.68--4.10 and the optimizer factor
1.22--1.76 and 1.91--2.92; each range is the gap between the two
finite-difference baselines. The adjoint cuts the gradient's share of the
time to target from 45--77% to about 5%.

The author line is Robb Walters (2AM Logic) and Crutcher Dunnavant (ZSpaceLabs.ai) (operator ruling 2026-10-22; affiliations since v19). Venue remains open.


## Motivation

A differentiable FEM model is useful for optimization when the information
in its derivatives changes the cost or quality of the design process. A
simple physical model can already guide some design problems effectively.
Increasing the number of parameters on its own does not establish that the
extra derivatives are worth computing.

Motor design offers a concrete test: local rotor and stator geometry can
affect the torque waveform across rotor positions and current levels. The
study will measure whether those effects create useful design freedom that
inexpensive models miss, and whether exact gradients exploit it efficiently.

## Claim

The research hypothesis is that nonlinear, slotted motor shape optimization
has regimes in which exact discrete gradients reach a fixed, independently
validated torque-ripple target substantially faster than calibrated reduced
models and competitive derivative-free methods.

The intended contribution is a reproducible account of **when gradients pay
off**, with measured cost and failure cases. A tenfold improvement is an
ambition to test, not an assumed finding or a criterion for retaining data.

## Strongest claim

**Current measured evidence:** at L3, order-45 truncation shifts standard deviation by 7–9% under load and 59% at zero current relative to conforming coupling. Matching the reference's coupling reduces the loaded ripple discrepancy from 9.9–11.3% to 1.3–1.7%; the original cogging gap falls from 12.9% to 0.10% after independent reference refinement. Opt120 retains a large ripple reduction, but raising order 45 to 93 increases its squared-ripple objective 3.6-fold at fixed unscaled current; at its saved 80 mm length and radial scale 1.048, its mean is below the initial motor under fixed unscaled or physical current and above it under fixed group-1 density. The 0.10% reference agreement does not establish full convergence. Of 704 distinct loaded best-step FD comparisons, 688 agree within 2.8e-7 relatively, worst 1.07e-5 (800 recorded comparisons include 96 repeats). Among 592 eligible distinct Taylor checks, 530 span at least four decades. Ripple gradients differ between h45/coupling-torque and conforming/Arkkio pairs; current policy reverses three L1 initial-design mean sensitivities. Recorded full-gradient cost is 1.13–1.20 forwards versus 8.2–10.5 for warm-started FD including its shared base, under uncontrolled host load. These are derivative timings, not optimization time-to-target results.

**Screening evidence:** the existing linear, slotless motor's stored torque
samples are nearly exactly represented by a three-coefficient sinusoid at
one geometry. The screening script tests held-out angles, not unseen designs
or shape gradients. It identifies a useful negative control.

**Conditional headline:** exact gradients provide a measured advantage over
the fastest credible alternative at the same refined design target, and
the advantage grows as useful shape freedom increases. This statement may
enter the abstract only after the comparison supports it.

**Why this could matter:** it would connect the cost advantage of adjoints
to physically useful design directions, rather than merely to the declared
parameter count. Existing motor adjoints are established prior work.

**What it could inspire:** a practical procedure for choosing between a
simple design model, reduced-order calibration and full-field gradients
before committing to an expensive inverse-design campaign.

**Evidence classes:** the slotless screening result is observed in existing
FEM output; its finite harmonic representation is derived from a fixed
linear operator and sinusoidal sources. The benchmark design is a proposed
synthesis. Any nonlinear-motor speed advantage remains a hypothesis.

**Opening and intellectual scope:** organize the introduction around the
decision to pay for derivatives. Readers should remember the measured
conditions under which that decision helps. A finding that calibrated
simple models remain competitive must be retained and may change the
paper's conclusion or its eventual publication scope.

## Method and initial scope

Start from the published ICEM slotted permanent-magnet motor described in
Wiesheu et al. (2024), with nonlinear iron, phase windings and synchronous
rotor-angle snapshots. Reproduce its initial torque waveform before
modifying the design space. The public MATLAB snapshot and its runtime
requirements are recorded in the pilot protocol.

Use an aggregate squared torque-ripple objective over zero, half and full
reference current. Normalize with a fixed positive reference torque so
zero-current cogging does not introduce division by a near-zero mean.
Constrain loaded mean torque, minimum air gap, bridge thickness, magnet
area and geometric validity. Initially fix the outer envelope, axial length
and control policy to isolate the effect of shape freedom.

Differentiate the converged nonlinear residual with its consistent material
tangent. Include geometry motion, magnetization/current loads and the same
torque functional used in the forward calculation. Adjoints are counted
per operating state and independent scalar output; there is no claim of
one backward solve for all states and constraints.

Thermal qualification, demagnetization, mechanical lifetime, 3D end effects,
full efficiency maps and drive-control co-design are excluded from the
initial experiment. Geometric constraints alone do not establish a
production-ready motor.

## Experiments and milestones

The original research plan is
[pilot_protocol.json](../../benchmarks/motor_optimization_screen/pilot_protocol.json),
an October 16 feasibility-screen artifact. The current execution contract is
[#1185](https://github.com/rjwalters/geode-fem/issues/1185), including its adopted
amendments. Keep this mutable brief aligned with that contract; preserve the
historical screen and report differences in scope explicitly.

| Milestone | Deliverable and acceptance condition | Status |
|---|---|---|
| M0: benchmark screen | Existing motor reducibility checked; relevant prior work and solver gaps recorded. | Complete; analysis of stored output only |
| M1: nonlinear magnetostatics | B-H residual and consistent tangent, linear-limit and manufactured/reference checks, Newton convergence and unit conventions. | Complete; solver and tests merged in #1159 |
| M2: reference motor | Slotted geometry, physical rotation/coupling and torque waveform at each current level; spatial and angular refinement, independent reference agreement. | Paired coupling and reference refinement merged (#1169/#1172/#1191); coarse reference explains cogging gap; finite-resolution and physical-validation limits remain |
| M3: motor shape gradients | Directional finite-difference step sweeps and Taylor checks at initial and perturbed designs; stable gradients as state tolerances tighten. | Scalar and grouped-control derivatives reviewed; scalar evidence in manuscript; validation infrastructure merged through #1194 |
| M4: useful design freedom | Compare engineering-parameter and surface-control spaces from matched starts; establish improvements above numerical uncertainty, with the different motion conventions disclosed. | Complete: 100-run main study and its 213 final checks in the paper |
| M5: measured optimization cost | Same-host, same-target comparisons of SLSQP/COBYQA at L0/L1 and the empirical RBF, with setup/calibration charged and resource use recorded. CMA-ES is an explicitly untested follow-on. | Complete: main study plus ablations F (#1218, 40 runs) and F_w (#1224, 20 runs); CMA-ES untested |
| M6: manuscript and artifacts | Freeze evidence, draft the paper, independently review claims and rendered figures, then complete the Anvil audit. | Package v18 (draft, reader-clarity revision and coauthor); v16 reviewed 42/44 and 40/44, audited READY-WITH-AUDIT-FLAGS (C1 fixed in v17); Zenodo deposit, coauthor affiliation and merge to main pending (operator) |

M2 includes independent reference refinement and M3 includes verified scalar gradients. Opt120 candidate validation is merged. The fixed 8/18-variable optimization protocol is preregistered on #1185; amendments 1–6 and the amendment-6 addendum governed the pilot, and adopted amendments 7/8 govern the main study. The harness #1196 merged at `89e6c4e127a5fbef7e4b2e0f10b90e70bd1193b4`; the 36-run development-host pilot merged in PR #1200 at `43db1c626bdc2367f896905d7807979789ee22b3`. It recorded 13 failures: 11 construction failures and two fresh-mesh minimum-angle failures. These loaded-host observations are feasibility/budget evidence only and do not enter the audited manuscript. Smoke runs are not benchmark evidence. Reviewed safeguards, coarse-model methods and session recovery merged through [PR #1206](https://github.com/rjwalters/geode-fem/pull/1206) at `9486a9da58beeca91968d4d3608f06b998abff45`. The main study ran from that source and is complete (100 runs). Its results, final refinement (#1203/PR #1207) and the post-hoc ablations #1218/#1224 are reported in v18 (see the current-state section above). The dated launch status, prioritized experimental requests and required handoff data are in [EXPERIMENTS.md](EXPERIMENTS.md).

Before M5, freeze numerical tolerances, target/error margins, design bounds,
starts/seeds and compute budgets. The main preregistration uses five starts, two timing repeats and 120 validation-forward wall-time units per run, with a completed-validation checkpoint at 60. The five methods are SLSQP and COBYQA at both L1 and L0, plus an empirical RBF trained on L1: 100 runs across the two spaces. Targets are fixed reductions of 50/90/99% from the refined initial objective, with candidate-based error allowance and a 0.25% full-current mean floor. The empirical RBF baseline does not discharge the separate inexpensive-physics comparison; the adopted L0 physical comparison still needs actual results. Do not infer a finite speedup when a
competitor fails to reach the target; report the censoring and its budget.

**Scope decision (Squad review #18, 2026-10-18).** The original M5 plan also
named CMA-ES, and the initial model proposed mean floors at every loaded current,
physical feature limits, CPU-hours/peak memory and an effective-dimension
diagnostic. These proposals have been reconciled explicitly: CMA-ES and other
population methods remain untested follow-ons; only full-current mean torque is
constrained, with half-current mean reported against theta0 for every final
candidate; no intrinsic-dimension claim is made from the two nested spaces.
CPU-time and RSS instrumentation merged with #1202, with its scope and
units recorded; complete main-study resource evidence remains pending and the
budget stays wall time. The physical-bound proposal is
discharged by the independently verified derivation in
`refs/geometry-bounds-audit.md` and the verbatim review in
`refs/geometry-bounds-review.md`. These continuous-geometry bounds do not mean
fixed magnet volume, qualification of every mesh, or manufacturability. The
paper's performance claim is confined to the tested methods and conditions.
The full mapping and retained limitations are in `EXPERIMENTS.md`.

Count mesh updates, assembly, nonlinear iterations, factorizations, RHS
solves, derivative assembly, constraints, line searches and calibration.
Match core limits and allow valid reuse/parallelization for all methods.
Report elapsed time and total compute. Finite differences are a diagnostic
baseline, not the only competitor.

Validate candidates on finer spatial meshes, denser and shifted rotor-angle
grids, tighter nonlinear tolerances and withheld current levels. Gains
smaller than this error floor do not establish a useful improvement.

## Figures and manuscript structure

Plan six figures, each generated from traceable data or geometry:

1. Motor cross-section, materials, motion interface and nested shape spaces.
2. Initial/reference torque waveforms and spatial/angular convergence.
3. Gradient verification and complete forward/gradient cost breakdown.
4. Attainable ripple versus useful shape freedom under matched constraints.
5. Validated target attainment versus measured elapsed time, with failures
   and variation across starts/seeds shown.
6. Final shape and torque waveforms at training and withheld conditions.

Put the slotless negative control and implementation detail in an appendix
unless they are essential to explaining the main result. No placeholder
plots should be presented as experimental findings.

The paper should proceed from the design decision and prior work to the
physical model, derivative method, validation, design-space comparison,
timing results and limitations. Apply the repository's paper voice guide.
Replace the working title with a result-based title only after the evidence
is fixed.

## Related work and positioning

The starter bibliography identifies
[Spline-Based Rotor and Stator Optimization of a Permanent Magnet Synchronous Motor](https://arxiv.org/abs/2402.19065)
and
[Combined Parameter and Shape Optimization of Electric Machines with Isogeometric Analysis](https://arxiv.org/abs/2311.06046).
These establish that nonlinear motor shape/parameter adjoints already exist.
The literature pass must also cover reduced magnetic models, motor design
surrogates and matched optimizer benchmarks before a novelty claim is made.

The [targeted literature screen](refs/literature-screen.json) adds motor
adjoints with automated derivatives, nonlinear magnetic equivalent circuits,
waveform-targeted surrogates and adaptive reduced-model optimization. These
make the proposed alternatives more demanding: saturation alone does not
establish that full-field gradients are necessary. The screen is not an
exhaustive novelty audit.

Automatic differentiation alone does not improve the asymptotic cost over
an established adjoint. Any distinction must be supported by implementation
evidence or controlled experiments. Separate optimizer comparisons within
one design space from comparisons between design spaces.

## Evidence and project tracking

- Research question and observations: geode-fem Squad Science Card **3**.
- [Screening script](../../benchmarks/motor_optimization_screen/screen_slotless.py).
- [Stored screening result](../../benchmarks/motor_optimization_screen/slotless_screen.json), with input and script hashes.
- [Pilot protocol](../../benchmarks/motor_optimization_screen/pilot_protocol.json), including the source revision audited and reference-code pin.
- [Starter bibliography](refs.bib).
- [Pinned ICEM reference data and conventions](../../benchmarks/motor_reference/README.md),
  including the stored full-precision torque profiles and reconstruction gaps.

The slotless screen is reproducible but is not a nonlinear-motor result.
The artifact-verification command now checks actual forward/gradient records, reference reruns, candidate validation and cost ratios and v5/v6 audit corrections. A passing slotless screen alone cannot satisfy that requirement.

Keep the project brief mutable and manuscript versions immutable under
Anvil. The current manuscript is `motor-gradient-optimization.18/`, with a local LaTeX class, references, data-backed figures, a pinned evidence manifest and four chained verifiers. Earlier versions remain immutable; subsequent results enter a new version. Review and audit state must come from the completed
workflow; project designation does not confer those states.

The operator requested continued paper improvement; revisions continued through v18 (v16 reviews: Opus 42/44, Fable 40/44; v16 audit READY-WITH-AUDIT-FLAGS). This does not confer review approval or authorize publication.
