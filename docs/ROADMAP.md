# GEODE-FEM roadmap

GEODE-FEM is a tensor-native, differentiable-by-construction FEM electromagnetics solver with a `geode` CLI for EDA flows. **Status as of 2026-11-13, `main` @ `a9a2e7fd`.** This is the single source for where the project is going. Each item links to a GitHub issue or epic, which holds the phase plan, goldens and acceptance criteria. When an epic changes scope, update this file in the same PR.

_Changed in this update (previous: 2026-10-26):_ the v0.10.0 table is replaced by a per-epic status, because most phases it listed as "filed" have closed; epics #837, #1253, #475, #476, #547, #569 and #647 are closed and are moved out of the open plan; the list of v0.8.0 contents is dropped in favour of `CHANGELOG.md`, which holds it in full; Now / Next / Later and a blocked list are added.

## Direction

GEODE-FEM complements [AWS Palace](https://github.com/awslabs/palace) rather than racing it on raw scale. The argument is in `docs/research/2026-07-16-strategic-direction.md` and the closed Epic #569. Two tracks carry it: **the `geode` CLI for EDA flows** (spec in, Touchstone/SPICE/JSON out) and **design sensitivities** (adjoint gradients of every reported observable, validated against finite differences).

Rules that every release follows:

- **Never silently wrong.** An unsupported combination fails loudly (`invalid_spec` / `solve_failed`), and the error says what to change.
- **Maximally useful for design work** (operator decision, #804). Don't block reasonable meshes: warn, estimate accuracy per result, and say how far to refine. Fail loudly only when no robust path exists.
- **Validated claims only.** Every feature ships with an analytic, literature or cross-tool golden, met honestly at a stated mesh; nothing is loosened to pass.
- **Additive schema v1.** Spec and report changes are additive, the schemas are regenerated, and the drift test passes.

## Where it stands

| Version | Theme | Status |
|---|---|---|
| v0.4–v0.7 | CLI Phases 1–3 (Epics #673/#680/#702): `check` / `driven` / `eigen` / `extract`, Touchstone, `geode mesh`, C/L + SPICE, lossy eigen, sensitivities, adaptive sweep | Released |
| v0.8.0 | Physics breadth for EDA (Epic #756) and hybrid wave ports (Epic #778) | Released 2026-10-04 |
| v0.9.0 | Library side of #836 (p=2), #841 (differentiable EDA), #835 (AMR), #837 (Floquet), plus `geode-optimize` and CLI N-port sensitivities | Released 2026-10-26 |
| v0.10.0 | The same epics' CLI phases | Not cut. `Cargo.toml` is at 0.9.0; the one CLI phase still open is #1267 |

Merged on `main` since v0.9.0 (per the tracker and `git log v0.9.0..`):

- **#836 p=2:** P4 sensitivities at p=2 (#1264) and P5a CLI `element_order` (#1265) are closed. Every filed phase is done; P3b (hybrid ports at p=2), P5b (default flip) and P6 (curved geometry) were never filed.
- **#841 differentiable EDA:** P2b wall physics and lossy-eigen `f`/`Q` gradients (#1260, #1329, #1330), P6b `geode optimize` (#1261) and the dispersion-model part of P2a (#1259, partial) are merged. P4, P5b and P7 are unfiled.
- **#835 adaptive meshing:** the `adaptive_mesh` spec block, report section and `geode check` bound are merged (`crates/geode-cli/src/adaptive.rs`, `tests/adaptive_mesh_golden.rs`). The goal-oriented DWR code (#1266 steps 1–5, Goldens 1 and 3) is merged.
- **#837 periodic / Floquet: epic closed.** Higher orders and diffraction (#1262), CLI periodic BCs and Bloch bands (#1263), CLI Floquet ports, the fixed-k_t reduced-order sweep (#1521) and `geode mesh --periodic` Floquet groups (#1530).
- **#1252 wide-scan element:** Floquet-port S adjoint (#1256), active reflection and the min-max objective (P2, P3), the element parameterization (#1535), the TCDA-like baseline with its recorded −3.50 dB worst case (#1536, `benchmarks/tcda_baseline/`), and the lumped-feed Γ_active adjoint and min-max driver (#1578).
- **#1251 open-PDK IC flow:** volumetric σ and `∂/∂σ` (#1254); the gds2palace-format stackup reader and `--stackup` (part of #1288).
- **#1253 GEODE-Bench: epic closed.** The dataset exporter is on `main` (#1257, `--dataset`, `docs/dataset-schema.md`). No open issue carries surrogate training.
- **New static and quasi-static solvers:** `geode conduction`, `geode eqs` (#1478, #1494), and library-only heat conduction with a Joule self-heating loop, thermo-elastic stress and electromigration lifetime models (#1556–#1558; plan in `docs/research/self-heating-thermoelastic-electromigration.md`).
- **Code health (2026-11-12/13):** more than a dozen merged consolidation and file-split PRs (#1545–#1604); `main` CI restored to green by #1541.

## Now

Can start today, or is already in review. Each is one PR.

| Item | Done when | Issue / PR | State |
|---|---|---|---|
| Finish CLI `adaptive_mesh` (#835 P6): per-tet η in the field VTU, cookbook spec | PR #1341 is rebased and merged; `examples/driven/adaptive_mesh_box.json` is in `tests/cookbook.rs`; #1267 closes | #1267, PR #1341 | PR conflicts with `main`; needs a conflict-resolution pass, then review |
| Cell-average and point sampling of tet fields | `sample_cell_average` exists in `geode-core::postproc` with its tests | #1363, PR #1368 | In review |
| Design A adjoint min-max run and kill-criterion verdict | `benchmarks/tcda_design/results.toml` and README are on `main`; #1579 closes | #1579, PR #1615 | Approved, awaiting merge |
| Cleanup with no open PR | Each issue's listed duplicates are gone and the test suite is unchanged | #1546, #1610, #1613, #1607 | Ready (`loom:issue`); check #1607 against PR #1603 first, which shares the same box mesher |
| Cleanup and CI gates already in review | PRs merged | #1588, #1595, #1596, #1599, #1602, #1617, #1382, #1391 | PRs #1603–#1618, #1387, #1392 open |
| Bring `CHANGELOG.md` `[Unreleased]` up to date | Every user-visible feature merged since v0.9.0 has an entry; today `[Unreleased]` has none for `geode conduction`, `geode eqs`, heat, thermo-elastic, electromigration, `adaptive_mesh`, `element_order`, Bloch bands or the dataset export | none (seed 1) | Unfiled |
| #841 P4 default: gradients with `sweep.adaptive` (full-order bypass) | The `sweep.adaptive` rejection in `crates/geode-cli/src/problem/sensitivity.rs` becomes a working mode with an FD-checked golden | none (seed 2) | Unfiled; the epic's Phase 4 spec depends only on the merged Phase 1 |
| p=2 Floquet cell | `FloquetCell` builds at `ElementOrder::P2` and a slab golden shows the p=2 error below p=1 on the same mesh | none (seed 3) | Unfiled; `driven/floquet.rs` builds its space at P1 only |
| Can Palace model the Design A cell? | A note in `docs/research/` gives a yes/no with cited Palace documentation, and #1581 is re-scoped to match | none (seed 4) | Unfiled; #1581 records that this was never checked |

## Next

Unblocked once a named item above lands.

| Item | Done when | Issue | Waits on |
|---|---|---|---|
| Cut v0.10.0 | Tag `v0.10.0`, workspace version bumped, `[Unreleased]` renamed | none | #1267 closed and the changelog item above |
| Interior lumped-port sheets in `adapt_driven` | `mesh_layout_reaches_target_met` is un-ignored and reaches `target_met` | #1340 | PR #1341; also needs Gmsh in the test tier |
| Ball/box-averaged peak-field observable | Sharp-corner test: the averaged value converges over ≥ 3 refinements while the pointwise maximum grows | #1480 | #1363 |
| Scalar P1 estimator, DWR and adaptive loop for electrostatics | Estimator effectivity shown on two geometries | #1481 | #1480 (consider splitting, per its curation note) |
| Windowed averages of J and Joule density in `geode conduction` | Report carries the four named averages with location and volume fraction | #1496 | #1480, #1363 |
| Design A: adjoint vs CMA-ES at three parameter counts | Table of solves-to-target per method and parameter count | #1580 | #1579 |
| Design A: p=2 re-centre and Palace cross-check | Final design re-solved at p=2; Palace difference recorded, or the missing capability named | #1581 | #1579, the p=2 Floquet cell and the Palace note |
| Point four log-log slope fits at the shared `fit_slope` | One definition remains | #1616 | PR #1603 |
| Layout schema v2 parent | `geode mesh` meshes the IHP 400 pH inductor example; `geode check` passes | #1255 | #1288 (manual run, below) |

## Later

Direction; each needs its question answered first.

- **#836 P3b, hybrid ports at p=2** (Nédélec-2 transverse + P2-Lagrange E_z). Question: is it still wanted, and under which epic? See the pending decision on #836.
- **#836 P6, curved / isoparametric geometry.** Needed for O(h⁴) eigenvalue rates on curved boundaries. Question: which design needs it first?
- **#841 P4 opt-in ROM gradient, P5b and P7.** P5b (CLI coverage) follows its library phases one sub-item at a time; P7 (a showcase design through `geode optimize` with a Palace cross-check) depends on 2a, 2b, 3b, 5b and 6. Question: which showcase, the coupled-line filter or the microstrip-fed patch?
- **#835 P5 (port-face-driven refinement) and P7 (hp).** Question: does goal-oriented marking pay at all? #1266's Golden 2 measured no consistent DOF advantage.
- **#1251 Phase 2 onward:** forward baseline against Palace on the IHP 400 pH inductor (|ΔL/L| ≤ 2%, |ΔQ/Q| ≤ 5% through 40 GHz), which is the kill gate for everything after it in that epic. Question: answered by the manual run on #1288.
- **#1252 Phase 5, waveguide-simulator measurement.** Operator-gated. Question: does Design A reach its target after #1580 and #1581?
- **CLI exposure of heat conduction, thermo-elastic stress and electromigration.** Library only today. Question: what spec shape, and is there a user for it?
- **Unplanned, no issue yet** (file one when a design needs it): wave-port field export; runtime GPU backend dispatch; hybrid ports in the adaptive sweep (the port mode is non-affine in ω, so research first).

## Blocked and waiting

- **Operator decisions:** close or keep #836 (unfiled P3b/P5b/P6); how #1266 closes after the negative Golden 2; the positions asked for on #1568; paper decisions #1063 and #1231; #955 (TM cutoff from the 3-D model) and with it #1032; removals #1416, #1567, #1565 and #1591.
- **#1259 (#841 P2a remainder):** `ε_xx/yy/zz` in the CLI, `μ_xx/yy/zz` columns and the dispersive filled wave port. Stopped on a design question recorded on the issue: one `eps_r` parameter on an `eps_r_diag` group must become three columns or the user must pick an axis, and the issue forbids a new spec key.
- **#1288:** only a manual run on the real IHP inductor GDS remains, which needs that GDS and Gmsh.
- **Held or parked PRs:** #1376 (build without Burn/Bunsen, #1365) is approved but held for a human merge and now conflicts; #1489 (#1488) has no TeX toolchain to verify the paper build; #1313 (#1299) and #1395 (#1393) are parked; #1076 (`toml` 0.9).
- **Dependencies upstream:** #1237, advisories reached through parent crates (`nom` via `mshio`, `tch`, `candle-core`, `ratatui-core`). Also parked: #891 and #1372.
- **Operator-run or hardware-gated:** #1428, #926, #763, #638–#641 and the conformal-antenna experiments #1083/#1086/#1090.

## Interfaces between the epics (still in force)

- Gradients are written against the order-generic solver of #838; #1264 keeps them working at p=2.
- #842 exposes its adjoint fields, which the goal-oriented estimator reuses at no extra solve.
- Design parameters bind to named mesh groups, so they survive refinement. Optimization runs on a frozen mesh, with adaptation only between stages.
- Periodic pairs expose `paired_face()`, which refinement mirrors. The Bloch adjoint is the solve at −k.
- Phases that edit `crates/geode-cli/src/{main,spec,problem,report}.rs` merge one at a time.

## Research tracks (not scheduled into a release)

- **Scale:** distributed / multi-GPU (#546, #640, #641). The matrix-free epic #547 is closed. July's measurements argue against leading with scale.
- **Papers:** transmon benchmark (#476 closed; stretch phases #638/#639 open), conformal radiator (#647 closed; v5 experiments #1083), and the Palace comparison section (#1488). The v1 Zenodo release is tracked on #1231.
- **Parked from the 2026-10-24 brainstorm** (revisit when a partner appears or a kill test is run): motor rotor built and dyno-tested; die-to-die link co-design; gradient-designed substrate isolation; power-delivery impedance shaping; surface-participation-optimized superconducting resonators; GPU matrix-free adjoint at 10–50M DOF.

## Not planned

- Racing Palace on eigenmode wall-clock, memory or DOF scale. The matrix-free interior eigensolve is a measured dead end (#531, #607). The Palace parity tracker #475 is closed as not planned.
- Hanging-node, anisotropic and coarsening AMR. Mortar coupling for non-matching periodic meshes.
- Differentiating through re-meshing. Optimization morphs the mesh within a stage and re-meshes between stages.
- Plasma-like port fills (#781, closed; waits for a real use case).
- Plasticity, creep and transient response in the thermo-elastic solver (out of scope in its build plan).
