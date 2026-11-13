# Work Plan

Roadmap generated from the current GitHub label state by the Loom Guide role.
Everything between the markers below is regenerated; do not edit it by hand.

<!-- guide:plan-body:start -->
## Operator Attention: Merge-Risk-Hold Pileup

Judge-approved PRs under a hold (`loom:blocked` or `loom:operator`) — only a human merge decision is missing.

- **#1376**: feat: add default-on tensor feature so geode-core builds without burn/bunsen

## Very Important

`loom:very-important` (one counts per workspace); every stage takes these first.

_None._

## Important

`loom:important`; taken before unlabelled work.

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer

## Ready

Human-approved issues ready for implementation (`loom:issue`).

- **#1267**: [#835 P6] CLI adaptive_mesh — spec block, per-iteration report, geode check bound, MSH 4.1 writer
- **#1299**: mshio: error Display dumps unbounded hex of remaining input
- **#1363**: postproc: sample piecewise-constant E onto cells or points (point location, cell averages)
- **#1382**: Remove dense mixed-pencil assembly (assemble_mixed_pencil, MixedOperators, restrict_mixed): test-only, production uses sparse path
- **#1393**: Consolidate duplicated onnx/tfjava sphere-PEC reference harnesses
- **#1488**: Honest GEODE-vs-Palace comparison (manuscript section)

## In Progress

Issues currently being built (`loom:building`).

- **#1391**: Remove flex_precision_spike example and flex feature: spike #451/#374 closed

## PRs Awaiting Review

PRs waiting on Judge (`loom:review-requested`).

- **#1313**: fix: cap hex dump in mshio parse error Display
- **#1341**: feat: export per-tet eta in adaptive field VTU and add adaptive_mesh cookbook spec
- **#1368**: feat: add cell-average and point sampling of tet fields onto user grids
- **#1387**: refactor(mixed_pencil): remove dead dense assembler, test sparse path
- **#1392**: chore: remove closed flex_precision_spike example and flex feature
- **#1395**: test: share onnx/tfjava sphere-PEC harness body in common module
- **#1489**: docs(papers): add GEODE-vs-Palace measured-performance section (transmon-benchmark.26)

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
- **#1579**: [#1537 P4c-2] Design A adjoint min-max run and kill-criterion verdict *(curated)*
- **#1593**: Tighten the remaining resolve_* helpers to take &[i32] so GroupTags has no Option columns *(curated)*

## Proposed (Architect / Hermit)

- **#1416**: Prune superseded papers version snapshots (~440 MB tracked) *(hermit)*
- **#1546**: Remove complex_slice_to_vec: trivial to_vec wrapper in geode-util *(hermit)*
- **#1565**: Consolidate duplicated forward-mode Dual number: shape.rs copy is a subset of driven/shape.rs *(hermit)*
- **#1567**: Remove 19 superseded PyTorch Declarations YAMLs from vendored tch (75 MB tracked, read by nothing) *(hermit)*
- **#1586**: Remove four uncalled leftovers: p2_region_mass_action, Fixture::load_json, two test helpers *(hermit)*
- **#1588**: Consolidate eight byte-identical helper functions duplicated across geode-core src modules *(hermit)*
- **#1591**: Remove ceviche_fdfd_baseline.py: 2D FDFD scaffold with a never-run TODO loop, superseded by the Meep baselines *(hermit)*
- **#1592**: Regenerate sphere_pec JAX baseline: committed fixture is a stub copied from NumPy, and remove the --stub path *(hermit)*
- **#1594**: Remove unused onnxscript pin from reference/onnx/requirements.txt *(hermit)*
- **#1595**: Split eigen/ams.rs (4,218 lines): move inline tests to ams/tests.rs and the scalar AMG to eigen/amg.rs *(hermit)*
- **#1596**: Consolidate duplicated port shape-adjoint cores in driven/shape.rs: matched_upml_ports_core is a ~330-line copy of driven_shape_gradient_ports_complex_core *(hermit)*

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
| Very important | 0 |
| Important | 1 |
| Ready (`loom:issue`) | 6 |
| In Progress (`loom:building`) | 1 |
| PRs awaiting review | 7 |
| Approved PRs awaiting merge | 1 |
| Curated | 20 |
| Architect / Hermit proposals | 11 |
| Active epics | 6 |
<!-- guide:plan-body:end -->
