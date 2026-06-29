# Changelog

All notable changes to GEODE-FEM are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### New `geode-util` pre-core staging crate (Epic #414)

- Introduced `geode-util`, a pre-core staging layer that sits above
  `geode-core` and collects shared, reusable helpers previously scattered
  across `geode-validation` and the standalone example crates. Its module
  map is `repo` / `convert` / `interop` / `fixture` / `viz`.
- Migrated the fixture-repository helpers (#417), interop decoders (#418),
  edge-DOF → nodal field reconstruction (`viz::edge_field_to_nodes`, #419),
  conversion glue, and the shared fixture TOML/pvd/sweep harness (#423) out
  of `geode-validation` and the examples and into `geode-util`.
  `geode-validation` now re-exports/consumes these helpers and retains only
  genuine validation-harness code.

### Changed

- Broke the standalone example crates' dependency on `geode-validation`;
  examples now depend on `geode-util` (and `geode-core` / `geode-app`) for
  shared utility code.

### Removed

- Deleted the orphaned `examples/_support` (`geode-examples-support`) crate.
  After #419 it was a thin re-export of `geode_util::viz::edge_field_to_nodes`
  with no remaining consumers; its `[workspace.dependencies]` entry has been
  removed (Epic #414 Phase 3, #426).

## [0.2.0] - 2026-06-25

### Changed

#### geode-core public API reorganized into a hierarchical module tree (Epic #377 — BREAKING)

- The crate's public surface, previously a flat set of root re-exports
  (`geode_core::<item>`), is now organized into directory-backed module
  groups: `backend`, `traits`, `mesh`, `elements`, `derham`, `assembly`,
  `solver`, `eigen`, `driven`, `analytic`, `postproc`, `interop`, and
  `prelude`. Every public item now lives at its canonical path
  `geode_core::<module>::<item>` (children #378–#386).
- **All deprecated flat-root re-export shims have been removed.** Code that
  imported items via `geode_core::<item>` must migrate to the canonical
  module path or `use geode_core::prelude::*;`. The only re-exports that
  remain at the crate root are the core traits
  `geode_core::{Element, Mesh, Operator}` (also available via
  `geode_core::traits::*` and the prelude).
- `silvermuller_self_consistent` has moved to `eigen::self_consistent`
  (canonical path `geode_core::eigen::self_consistent::*`), with no compat
  shim — it is a quasimode-`k` eigenpencil finder and now lives alongside
  the other eigensolvers.
- `geode_core::prelude` is finalized as the recommended ergonomic surface:
  glob-import it (`use geode_core::prelude::*;`) to pull in the high-traffic
  entry points (mesh constructors/readers, assembly/eigen/driven/analytic
  types, core traits) from their canonical paths.

This is a breaking change for downstream callers; the workspace minor
version is bumped accordingly. See epic #377 and children #378–#387.

## [0.1.0] - 2026-06-15

### Summary

Initial public release. GEODE-FEM is a Burn-based Rust FEM/DG electromagnetic
solver. The 0.1 milestone closes four foundational epics (#88, #193, #226,
#234) and lands a Krylov + iterative-solver sweep on top, bringing the project
to the point where the driven solver hits Palace 3D parity on a spiral
inductor benchmark and the wave-port path validates against analytic
mode-matching cross-checks.

### Added

#### Solver core (Epic #88 — Burn bring-up)

- Workspace skeleton with three crates: `geode-core` (solver primitives),
  `geode-cli` (`geode` binary), and `geode-validation` (cross-backend
  comparison harness).
- Burn-tensor assembly layer with `wgpu` default backend and opt-in `ndarray`
  / `cuda` / `autodiff` backends; `unsafe_code = "deny"` at the crate
  boundary.
- Whitney / Nédélec / P1 element kernels including the shared
  `whitney_face` surface-mass module (#221).
- Sparse `[nnz]` pattern-slot Nédélec assembly for the driven path, lifting
  the 46k-edge dense-scatter cap (#220).
- De Rham `d⁰` rank classifier replacing the older spurious-mode heuristic
  (#124).
- ARPACK FFI eigensolver (vendored `dsaupd_c` / `dseupd_c` bindings, no
  bindgen required) behind the `arpack` Cargo feature.
- Pure-Rust sparse shift-invert Lanczos path (faer sparse LU) as the
  default eigensolver.
- Phase I/J cross-backend Mie reference suite: NumPy (#179), Julia (#181),
  JAX (#180), TF-Java (#183), and ONNX expressibility audits (#178, #182).

#### Driven solver (Epic #193)

- Deterministic driven solve `A(ω)x = b` with volumetric current source
  (#194).
- Conductivity term σ via ω-independent damping matrix C (#196).
- Matched (full Sacks) UPML lifted into the Burn assembly layer and into
  `driven_solve` (#205).
- Palace-style uniform lumped port for `driven_solve` with R termination
  and V/I bookkeeping (#206).
- Leontovich surface-impedance BC for thick conductors (#207).
- Driven Mie scattering benchmark Q_ext / Q_sca vs ka with matched UPML
  against the analytic series (#195).
- Z(ω) → L/R/Q/S₁₁ extraction and assembly-reusing frequency sweep over
  port-driven solves (#209).
- Layered-stack spiral inductor mesh generation (gmsh) with tag adapter to
  port / Leontovich / UPML inputs (#217).
- N-port S-matrix extraction over factor-once / multi-RHS port-driven
  solves (#219).
- Spiral inductor L/Q benchmark — FEM sweep vs Mohan analytic and MoM PEEC
  baselines (#211).
- SLCFET 3HP spiral capstone hitting the 5 % bar on quasi-static L₀
  comparison (#230).

#### Patch antenna (Epic #226)

- Probe-fed FR-4 patch-antenna gmsh fixture with box-UPML open-radiator
  adapter (#231).
- Patch-antenna S11 / resonance / bandwidth / efficiency benchmark vs the
  cavity-model oracle (#232).
- Love-equivalence near-to-far-field transform → patch radiation pattern,
  directivity, gain (#229).
- Impedance-matched patch feed delivering a real −10 dB return loss and
  bandwidth (#237).
- NTFF pattern artifact for the impedance-matched patch fixture
  (G = D·η_matched) (#252).

#### Wave-port BC (Epic #234)

- 2D transverse modal eigensolver for waveguide port cross-sections
  (#240).
- Wave-port boundary condition and wave-port S-parameters (#234 Phase 2)
  (#245).
- 2D waveguide modal pencil moved onto the sparse Lanczos path; drops the
  faer-QZ debug-overflow workaround (#253).
- True mesh height-step waveguide fixture and single-mode S-parameter
  validation (#248).
- Multi-mode waveguide modal eigensolve with outgoing-β branch and
  wrapper unification (#254).
- Rank-N SMW wave-port BC, multi-mode `waveguide_mode_reduce`, and block
  S-matrix (#255).
- Bi-modal straight-section wave-port validation (#256).
- Bi-modal height-step with analytic mode-matching cross-check (#257).
- Deterministic eigenvector sign pin in `solve_rect_waveguide_modes`
  (#262).
- General-cross-section 2D modal eigensolver (#265).

#### Iterative solvers and oracle parity (post-epic sweep)

- Krylov iterative solver path (COCG + Jacobi) for the driven
  complex-symmetric system (#243).
- Krylov iterative solver wired through sweep pipelines (#264).
- ILU(0) preconditioner for the COCG Krylov path (#267).
- Palace 3D oracle scaffolding: config generator, result ingester, and
  patch-benchmark wiring (#239).
- Palace 3D oracle parity for the spiral inductor benchmark (#266).
- Matched (full Sacks) UPML on the eigenmode path with quasi-mode Q vs
  `mie_open` complex roots (#223).
- Fine Mie sphere fixture — on-resonance driven Q_ext / Q_sca below 5 %
  (#224).

### Changed

- Build profile keeps dense linear algebra (faer) and tensor backends
  (Burn / wgpu) optimized in debug and test builds; project crates remain
  no-opt for fast iteration.
- README refreshed for the driven + multi-mode wave-port era (#274).

### Fixed

- ARPACK iterations are now deterministic (fixed-seed v₀ + rng) for all
  reference eigensolves (#191).
- `upload_mesh` honors `B::FloatElem` instead of forcing f32 (#99).
- Backend cfg robust to feature unification via precedence selection
  (#76).

### Removed

- A1's deprecated wave-port shims dropped following the multi-mode
  migration (#268).
