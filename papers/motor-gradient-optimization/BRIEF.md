---
title: "When do field gradients pay off? A controlled study of nonlinear motor optimization"
slug: motor-gradient-optimization
skill: paper
paper_number: 3
documentclass: anvil-paper
anonymous: false
status_of_inputs: "Research setup; slotless negative-control screen complete; nonlinear motor experiments pending"
claim: "Test whether exact field gradients reduce time to a validated motor-design target beyond calibrated simple models, and how that advantage depends on useful shape freedom."
audience: "Computational electromagnetics, electric-machine optimization, and differentiable scientific computing researchers"
venue_target: "Select after the pilot establishes the contribution; use the generic paper class initially"
length_target: "Approximately 10-15 pages with 5-6 main figures; adjust to the evidence and venue"
web_search: true
public_repo_url: "https://github.com/rjwalters/geode-fem"
science_card_id: 3
---

# Third paper: nonlinear motor optimization

The operator designated this work as GEODE's third paper on 2026-10-16,
alongside the transmon and conformal-antenna papers. This brief establishes
the research project. Its Anvil state is **EMPTY (brief only)**: there is no
manuscript, reviewed result, or publication-ready speedup claim.

The working title may change with the findings. Authorship, venue and final
title remain open until manuscript preparation; no author invitation or
submission is implied by this setup.

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

**Current evidence:** the existing linear, slotless motor's stored torque
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

The detailed execution contract is
[pilot_protocol.json](../../benchmarks/motor_optimization_screen/pilot_protocol.json).
Keep it and this brief consistent when scope changes.

| Milestone | Deliverable and acceptance condition | Status |
|---|---|---|
| M0: benchmark screen | Existing motor reducibility checked; relevant prior work and solver gaps recorded. | Complete; analysis of stored output only |
| M1: nonlinear magnetostatics | B-H residual and consistent tangent, linear-limit and manufactured/reference checks, Newton convergence and unit conventions. | Claude coordinating implementation in #1157 |
| M2: reference motor | Slotted geometry, physical rotation/coupling and torque waveform at each current level; spatial and angular refinement, independent reference agreement. | Reference data prepared by Codex; forward reproduction depends on M1 |
| M3: motor shape gradients | Directional finite-difference step sweeps and Taylor checks at initial and perturbed designs; stable gradients as state tolerances tighten. | Depends on M2 |
| M4: useful design freedom | Compare nested engineering-parameter and surface-control spaces from matched starts; establish improvements above numerical uncertainty. | Depends on M3 |
| M5: measured optimization cost | Same-host, same-target comparisons against calibrated simple/reduced models, trust-region derivative-free search and CMA-ES, with all setup and calibration charged. | Depends on M4 |
| M6: manuscript and artifacts | Freeze evidence, draft the paper, independently review claims and rendered figures, then complete the Anvil audit. | Depends on M5 |

M1 is the immediate implementation task. Existing GEODE components include
linear P1/P2 magnetostatics, PM sources and P1 torque extraction; the audited
source did not contain the required nonlinear B-H solver, physical
rotation/coupling or motor shape adjoint. Record runnable evidence as these
capabilities are implemented rather than inferring readiness from related
electrostatic or antenna gradients.

Before M5, freeze numerical tolerances, target/error margins, design bounds,
starts/seeds and compute budgets. The preliminary M4 cap of three starts and
20 iterations per space is a screening budget, not a proof about global
optima or dimensional necessity. Do not infer a finite speedup when a
competitor fails to reach the target; report the censoring and its budget.

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
Before manuscript review, add external-artifact verification commands that
check the actual motor evidence and every claimed cost ratio. A passing
slotless screen cannot satisfy that requirement.

Keep the project brief mutable and manuscript versions immutable under
Anvil. Once results support a paper, create `motor-gradient-optimization.1/`
with the repository's shared LaTeX class, references, data-backed figures
and provenance. Review and audit state must come from the completed
workflow; project designation does not confer those states.
