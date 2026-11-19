# Work Plan

Roadmap generated from the current GitHub label state by the Loom Guide role.
Everything between the markers below is regenerated; do not edit it by hand.

<!-- guide:plan-body:start -->
_Generated: 2026-11-19T21:06:03Z (snapshot; compare against live labels)_
## Operator Attention: Merge-Risk-Hold Pileup

Judge-approved PRs under a hold (`loom:blocked` or `loom:operator`) — only a human merge decision is missing.

_None._

## Very Important

`loom:very-important` (one counts per workspace); every stage takes these first.

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer

## Important

`loom:important`; taken before unlabelled work.

_None._

## Ready

Human-approved issues (`loom:issue`) ready for implementation — excluding held (`loom:blocked`/`loom:operator`/`loom:frozen`), `loom:building`, and issues closed by an open `loom:pr` PR (listed under Approved).

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer
- **#1299**: mshio: error Display dumps unbounded hex of remaining input
- **#1393**: Consolidate duplicated onnx/tfjava sphere-PEC reference harnesses
- **#1488**: Honest GEODE-vs-Palace comparison (manuscript section)
- **#1626**: Bring CHANGELOG [Unreleased] up to date with features merged since v0.9.0
- **#1729**: CI: integration a-m default-tier step overran its 30-min step bound (#1713) uncut; job cancelled at 65 min and log discarded again

## In Progress

Issues currently being built (`loom:building`).

- **#1391**: Remove flex_precision_spike example and flex feature: spike #451/#374 closed
- **#1767**: Drive scalar refinement by peak-field error and measure effectivity on two geometries
- **#1775**: Make electrostatic P1 residual eta_t2 accumulation order-independent

## PRs Awaiting Review

PRs waiting on Judge (`loom:review-requested`).

- **#1313**: fix: cap hex dump in mshio parse error Display *(held)*
- **#1341**: feat: export per-tet eta in adaptive field VTU and add adaptive_mesh cookbook spec *(held)*
- **#1395**: test: share onnx/tfjava sphere-PEC harness body in common module *(held)*
- **#1489**: docs(papers): add GEODE-vs-Palace measured-performance section (transmon-benchmark.26) *(held)*
- **#1625**: docs: roadmap as of 2026-11-13
- **#1648**: docs: cover every feature merged since v0.9.0 in CHANGELOG [Unreleased] *(held)*
- **#1777**: fix: accumulate electrostatic P1 eta_t2 in sorted face order

## Approved (Awaiting Merge)

PRs that passed review and are queued for Champion auto-merge (`loom:pr`).

_None._

## Proposed

Issues carrying `loom:curated`.

- **#926**: CUDA f32 matmul runs as TF32 on Ampere+: matrix-free driven COCG stagnates on A100 (audit remaining f32 matmul sites) *(curated)* *(held)*
- **#955**: p=2 TM guard: compute the guide section's TM cutoff from the 3-D model to recover the coarse-mesh band *(curated)* *(held)*
- **#1032**: TM guard: extend the acceptance tables to guides longer than the guard's reach (#955 option B) *(curated)* *(held)*
- **#1083**: [conformal v5] New experiments for conformal-antenna-diffopt v5 (umbrella) *(curated)*
- **#1086**: [conformal v5] E3: Dense S11 sweep plus radiated/dissipated power balance for initial and optimized designs *(curated)* *(held)*
- **#1090**: [conformal v5] E5b: Meep dielectric-density optimization of the substrate (structured-grid optimizer vs GEODE) *(curated)* *(held)*
- **#1237**: Resolve remaining archive and parser dependency security advisories *(curated)* *(held)*
- **#1255**: [#1251 P1b] Layout schema v2 for IC stacks — holes, 45°, via nets, finite-σ shells, SG13G2 stackup import *(curated)* *(held)*
- **#1259**: [#841 P2a] Material physics gradients — tan δ, dispersive-model (DS/Debye/Drude) and anisotropic ε/μ parameters *(curated)* *(held)*
- **#1266**: [#835 P4] Goal-oriented DWR for S-parameters and eigenfrequency — GoalFunctional shared with the adjoint gradients *(curated)* *(held)*
- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer *(curated)*
- **#1288**: layout v2: gds2palace XML stackup importer and scripts/gds2geode-layout.py (IHP 400 pH inductor) *(curated)* *(held)*
- **#1299**: mshio: error Display dumps unbounded hex of remaining input *(curated)*
- **#1340**: adaptive_mesh: support interior lumped-port sheets in geode-core::adapt (adapt_driven) *(curated)* *(held)*
- **#1428**: Meep R2.5/R5 Spot runs on the optimized E4 geometry with harness v3 (split from #1427) *(curated)* *(held)*
- **#1488**: Honest GEODE-vs-Palace comparison (manuscript section) *(curated)*
- **#1626**: Bring CHANGELOG [Unreleased] up to date with features merged since v0.9.0 *(curated)*
- **#1629**: Research note: can Palace model the Design A Floquet cell for the #1581 cross-check? *(curated)* *(held)*
- **#1729**: CI: integration a-m default-tier step overran its 30-min step bound (#1713) uncut; job cancelled at 65 min and log discarded again *(curated)*
- **#1767**: Drive scalar refinement by peak-field error and measure effectivity on two geometries *(curated)*
- **#1775**: Make electrostatic P1 residual eta_t2 accumulation order-independent *(curated)*

## Proposed (Architect / Hermit)

- **#1416**: Prune superseded papers version snapshots (~440 MB tracked) *(hermit)* *(held)*
- **#1621**: Split geode-cli problem/mod.rs (4,064 lines): move the validate_* and resolve_* groups into sibling modules *(hermit)* *(held)*
- **#1649**: Consolidate the duplicated mie_roots fixture helpers onto tests/common: the julia harness is a verbatim copy of the numpy one (~118 lines) *(hermit)* *(held)*
- **#1651**: Consolidate duplicated Burn helpers between nedelec_local and p1_local numpy reference harnesses *(hermit)* *(held)*

## Epics

- **#546**: Epic: distributed EM solve over NVLink GPU clusters — domain decomposition + distributed sparse Krylov (tensor-compilation scale thesis, Leg 3) *(held)*
- **#835**: Epic: adaptive meshing — a-posteriori error estimation + AMR loop (v0.9.0)
- **#836**: Epic: higher-order accuracy — p=2 Nédélec through driven/eigen/ports/CLI (v0.9.0) *(held)*
- **#841**: Epic: differentiable EDA — sensitivities through ports/materials/hybrid modes + geode optimize (v0.9.0)
- **#1251**: Epic: open-PDK IC flow — differentiable mm-wave passives on IHP SG13G2, calibrate + corners, taped out and measured
- **#1252**: Epic: wide-scan phased-array element by Floquet shape adjoint, measured in a waveguide simulator

## Backlog Balance

| Bucket | Count |
|------|-------|
| Operator merge-risk holds | 0 |
| Very important | 1 |
| Important | 0 |
| Ready (`loom:issue`, net of holds/building/merge-pending) | 6 |
| In Progress (`loom:building`) | 3 |
| PRs awaiting review | 7 |
| Approved PRs awaiting merge | 0 |
| Curated | 21 |
| Architect / Hermit proposals | 4 |
| Active epics | 6 |
<!-- guide:plan-body:end -->
