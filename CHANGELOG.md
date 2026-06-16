# Changelog

All notable changes to GEODE-FEM are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
