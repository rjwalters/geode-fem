# Work Plan

Roadmap generated from the current GitHub label state by the Loom Guide role.
Everything between the markers below is regenerated; do not edit it by hand.

<!-- guide:plan-body:start -->
## Operator Attention: Merge-Risk-Hold Pileup

Judge-approved PRs under a hold (`loom:blocked` or `loom:operator`) — only a human merge decision is missing.

- **#1376**: feat: add default-on tensor feature so geode-core builds without burn/bunsen

## Very Important

`loom:very-important` (one counts per workspace); every stage takes these first.

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer

## Important

`loom:important`; taken before unlabelled work.

_None._

## Ready

Human-approved issues ready for implementation (`loom:issue`).

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer
- **#1299**: mshio: error Display dumps unbounded hex of remaining input
- **#1363**: postproc: sample piecewise-constant E onto cells or points (point location, cell averages)
- **#1382**: Remove dense mixed-pencil assembly (assemble_mixed_pencil, MixedOperators, restrict_mixed): test-only, production uses sparse path
- **#1393**: Consolidate duplicated onnx/tfjava sphere-PEC reference harnesses
- **#1488**: Honest GEODE-vs-Palace comparison (manuscript section)
- **#1626**: Bring CHANGELOG [Unreleased] up to date with features merged since v0.9.0

## In Progress

Issues currently being built (`loom:building`).

- **#1391**: Remove flex_precision_spike example and flex feature: spike #451/#374 closed
- **#1546**: Remove complex_slice_to_vec: trivial to_vec wrapper in geode-util
- **#1623**: Consolidate four copies of the Gauss-Legendre edge interpolant and its hand-typed quadrature table in geode-core tests
- **#1634**: Consolidate the duplicated ICEM design-gradient machinery: 280 of icem_cp_design.rs's 429 lines are icem_design.rs renamed

## PRs Awaiting Review

PRs waiting on Judge (`loom:review-requested`).

- **#1313**: fix: cap hex dump in mshio parse error Display
- **#1341**: feat: export per-tet eta in adaptive field VTU and add adaptive_mesh cookbook spec
- **#1395**: test: share onnx/tfjava sphere-PEC harness body in common module
- **#1489**: docs(papers): add GEODE-vs-Palace measured-performance section (transmon-benchmark.26)
- **#1648**: docs: cover every feature merged since v0.9.0 in CHANGELOG [Unreleased]

## Approved (Awaiting Merge)

PRs that passed review and are queued for Champion auto-merge (`loom:pr`).

- **#1376**: feat: add default-on tensor feature so geode-core builds without burn/bunsen

## Proposed

Issues carrying `loom:curated`.

- **#926**: CUDA f32 matmul runs as TF32 on Ampere+: matrix-free driven COCG stagnates on A100 (audit remaining f32 matmul sites) *(curated)*
- **#955**: p=2 TM guard: compute the guide section's TM cutoff from the 3-D model to recover the coarse-mesh band *(curated)*
- **#1032**: TM guard: extend the acceptance tables to guides longer than the guard's reach (#955 option B) *(curated)*
- **#1083**: [conformal v5] New experiments for conformal-antenna-diffopt v5 (umbrella) *(curated)*
- **#1086**: [conformal v5] E3: Dense S11 sweep plus radiated/dissipated power balance for initial and optimized designs *(curated)*
- **#1090**: [conformal v5] E5b: Meep dielectric-density optimization of the substrate (structured-grid optimizer vs GEODE) *(curated)*
- **#1237**: Resolve remaining archive and parser dependency security advisories *(curated)*
- **#1255**: [#1251 P1b] Layout schema v2 for IC stacks — holes, 45°, via nets, finite-σ shells, SG13G2 stackup import *(curated)*
- **#1259**: [#841 P2a] Material physics gradients — tan δ, dispersive-model (DS/Debye/Drude) and anisotropic ε/μ parameters *(curated)*
- **#1266**: [#835 P4] Goal-oriented DWR for S-parameters and eigenfrequency — GoalFunctional shared with the adjoint gradients *(curated)*
- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer *(curated)*
- **#1288**: layout v2: gds2palace XML stackup importer and scripts/gds2geode-layout.py (IHP 400 pH inductor) *(curated)*
- **#1299**: mshio: error Display dumps unbounded hex of remaining input *(curated)*
- **#1340**: adaptive_mesh: support interior lumped-port sheets in geode-core::adapt (adapt_driven) *(curated)*
- **#1363**: postproc: sample piecewise-constant E onto cells or points (point location, cell averages) *(curated)*
- **#1365**: geode-core: electrostatics-only build without Burn/Bunsen (3 min cold build, MPL-2.0 colored via burn-tensor) *(curated)*
- **#1428**: Meep R2.5/R5 Spot runs on the optimized E4 geometry with harness v3 (split from #1427) *(curated)*
- **#1488**: Honest GEODE-vs-Palace comparison (manuscript section) *(curated)*
- **#1626**: Bring CHANGELOG [Unreleased] up to date with features merged since v0.9.0 *(curated)*
- **#1629**: Research note: can Palace model the Design A Floquet cell for the #1581 cross-check? *(curated)*

## Proposed (Architect / Hermit)

- **#1416**: Prune superseded papers version snapshots (~440 MB tracked) *(hermit)*
- **#1565**: Consolidate duplicated forward-mode Dual number: shape.rs copy is a subset of driven/shape.rs *(hermit)*
- **#1567**: Remove 19 superseded PyTorch Declarations YAMLs from vendored tch (75 MB tracked, read by nothing) *(hermit)*
- **#1591**: Remove ceviche_fdfd_baseline.py: 2D FDFD scaffold with a never-run TODO loop, superseded by the Meep baselines *(hermit)*
- **#1620**: Remove 15 pub fns with no production caller in geode-core/-optimize/-util (~164 lines) *(hermit)*
- **#1621**: Split geode-cli problem/mod.rs (4,064 lines): move the validate_* and resolve_* groups into sibling modules *(hermit)*
- **#1644**: Remove examples/fiber_smf28_solve_timing: one-off AFTER-timing harness for closed #327, no artifact and no caller *(hermit)*
- **#1649**: Consolidate the duplicated mie_roots fixture helpers onto tests/common: the julia harness is a verbatim copy of the numpy one (~118 lines) *(hermit)*
- **#1651**: Consolidate duplicated Burn helpers between nedelec_local and p1_local numpy reference harnesses *(hermit)*

## Epics

- **#546**: Epic: distributed EM solve over NVLink GPU clusters — domain decomposition + distributed sparse Krylov (tensor-compilation scale thesis, Leg 3)
- **#835**: Epic: adaptive meshing — a-posteriori error estimation + AMR loop (v0.9.0)
- **#836**: Epic: higher-order accuracy — p=2 Nédélec through driven/eigen/ports/CLI (v0.9.0)
- **#841**: Epic: differentiable EDA — sensitivities through ports/materials/hybrid modes + geode optimize (v0.9.0)
- **#1251**: Epic: open-PDK IC flow — differentiable mm-wave passives on IHP SG13G2, calibrate + corners, taped out and measured
- **#1252**: Epic: wide-scan phased-array element by Floquet shape adjoint, measured in a waveguide simulator

## Backlog Balance

| Bucket | Count |
|------|-------|
| Operator merge-risk holds | 1 |
| Very important | 1 |
| Important | 0 |
| Ready (`loom:issue`) | 7 |
| In Progress (`loom:building`) | 4 |
| PRs awaiting review | 5 |
| Approved PRs awaiting merge | 1 |
| Curated | 20 |
| Architect / Hermit proposals | 9 |
| Active epics | 6 |
<!-- guide:plan-body:end -->
