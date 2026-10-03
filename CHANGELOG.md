# Changelog

All notable changes to GEODE-FEM are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

#### `geode` CLI

- Wave ports compose with Leontovich walls, including rough walls: a lossy guide's S-parameters carry the conductor attenuation, validated against Pozar Eq. 3.96 (TE10 α_c) and the roughness factor K(f). A Silver-Müller wall that shares an edge with a wave-port rim is rejected with `invalid_spec` (#776).

#### `geode-core`

- `solve_wave_port_sweep_with_mode` takes a `surfaces` argument (impedance walls folded into the base operator); an empty slice is bit-identical to before. New `analytic::waveguide::te10_conductor_attenuation` oracle (#776).
- New `analytic::port_modes`: a p=1 Whitney + P1 mixed E_t–E_z port-mode solver (`solve_hybrid_port_modes`) for PEC-shielded cross-sections with real per-triangle ε. It returns every propagating mode plus the first K evanescent modes, including fast modes. The exact β² = 0 null space is deflated out of the Krylov space, every pair carries an explicit residual, and modes are normalized by the B-form with `xᵀBx = β²` for propagating modes. A short or uncertified window returns an explicit `Shortfall` error. A complex-conjugate pair inside the window returns a `ComplexPair` error. New `analytic::loaded_guide` closed-form LSE/LSM oracle for the slab-loaded rectangular guide. Measured on the slab guide (ε_r 2.25 and 4): β error ≤ 0.43 % at h = b/16, rate ≈ 2 in β², and the mode set matches the oracle one-to-one with no ceiling pileup. Honest limits: spurious complex LSE/LSM evanescent pairs, with Im β² = O(h); and P1-carried TM modes (uniform TM₂₁ β error 1.8 % at h = b/16). The fiber `mixed_pencil` solver is unchanged; its Arnoldi gains an optional projector (#803, Epic #778 Phase 1).
- **Hybrid wave ports** (#804, Epic #778 Phase 2): a `HybridWavePort` drives the 3-D solve with modes of an inhomogeneous PEC cross-section, re-solved per frequency with the p=1 mixed solver and tracked by continuity (B-pairing overlap, one-to-one; lost identity is an explicit error, never a guess). The modal flux is `f̂ = S_p(E_t − (j/β)∇_tE_z)` normalized by the unconjugated B-form, so the existing rank-N SMW, drive and `√β` power weights carry over unchanged. Additive API: `WavePortSpec` (geometric or hybrid), `solve_wave_port_spec_sweep_with_mode` and `solve_mixed_port_spec_sweep_with_mode` return the S points plus per-port reports (β, E_z energy fraction, tracking overlap, accuracy estimate) and report-level warnings. Every propagating face mode must be a reported channel, else `InvalidPort`; hybrid ports carry TM content, so the TE-only TM-cutoff guard of #808 does not apply to them. Mesh-induced complex evanescent pairs are terminated as a reciprocal 2×2 block with a warning that names the port and modes and gives a predicted split resolution. A per-mode accuracy estimate (an h/2 face re-solve with nested prolongation, `analytic::port_mode_accuracy`) warns above 0.5 % with a refine-to-h hint. `DrivenRom::build_with_wave_port_specs` rejects hybrid ports, because their port operator is not affine in ω. `HybridPortOpts::carry_complex_pairs` (default off, so P1 behaviour is unchanged) returns complex pairs as `HybridComplexPair`. Measured: the homogeneous limit matches the `PortMedium` path to `|ΔS| = 4e-15`; on the slab-loaded section, β is within 0.22 % of the oracle, `|S11|` ≤ 1.0e-2, `|S21 − e^{−jβL}|` ≤ 1.2e-2, and reciprocity and energy hold to round-off; the accuracy estimate is within a factor of 1.11 of the true error over 46 modes.

### Fixed

#### `geode-core`

- `analytic::waveguide::solve_dielectric_modes2` now returns the guided modes of weakly-guiding fibers (for example SMF-28). Every in-window eigenpair of its pencil obeys the exact Rayleigh identity `r = 1 − n_eff²/⟨ε⟩_x`, so its curl ratio is below `(ε_max − ε_min)/ε_max` (`7.8e-3` for SMF-28). The old fixed curl-energy floor (`3e-2`) sat above that bound, so it rejected every guided mode. What it returned as the "fundamental" was an unconverged Lanczos Ritz vector, or nothing, depending on platform rounding and mesh. The floor now scales with the contrast and is capped at the old value, so the Si/SiO₂ floor is unchanged. The solver also rejects in-window pairs that break the Rayleigh identity (unconverged Ritz vectors). The #449 grad–div audit is re-measured on the genuine fundamentals, and its verdict is corrected in `docs/formulation_audit_reduced_vs_full_vector.md` (#791).
- `FaerComplexEigensolver` (and so `self_consistent_k` / `self_consistent_k_vector_tracked`) no longer hangs on complex pencils above about 500 DOF. It called faer 0.24's generalized complex QZ, which turns the matrices into NaN when many shifts come from a degenerate eigenvalue cluster (the Nédélec gradient null space) and then runs its `30·n` sweep cap without reporting an error; a 3300-DOF Mie pencil ran for 14.8 h. The solver now computes the full spectrum by dense shift-invert (LU of `A − σB`, then faer's standard complex Schur QR of `(A − σB)⁻¹B`). The eigenvalues match the old solver's to about `1e-12` relative, and the 3300-DOF pencil takes about 11 s. Pencils larger than the new public `eigen::complex::MAX_DENSE_COMPLEX_DIM` (6000) are refused with the new `EigenError::DenseTooLarge { dim, max }`. Non-finite input or output returns `EigenError::FaerGevd` instead of NaN eigenvalues. `EigenError` is not `#[non_exhaustive]`, so a downstream exhaustive `match` on it needs a new arm (#796).
- `FaerDenseEigensolver` (`smallest_eigenvalues` / `smallest_eigenpairs`) is no longer many times slower from about 590 DOF, and no longer returns inaccurate or spuriously complex eigenvalues on real pencils of a few hundred DOF and up. It called faer 0.24's generalized real QZ, which from 590 DOF spins its aggressive-early-deflation window QZ to an iteration cap (5.7 s at 600 DOF against 0.36 s at 560), and whose blocked path lost accuracy on symmetric-definite FEM pencils: on sub-blocks of the bundled Mie pencil `(Re K, Re M)` its physical eigenvalues were off the symmetric reference by `9e-5` at 250 DOF, `8e-4` at 600 and up to `0.57` at 1200, with spurious complex-conjugate pairs. The solver now uses dense shift-invert (LU of `K + τM`, then faer's standard real Schur QR of `(K + τM)⁻¹M`, with complex-shift fallbacks), which matches the symmetric reference to about `1e-12` relative and takes 0.15 s at 600 DOF and 8 s at 3300 with eigenvectors (46 s before). Pencils larger than the new public `eigen::dense::MAX_DENSE_REAL_DIM` (8000) are refused with `EigenError::DenseTooLarge`, whose message now names the real and the complex sparse solvers; non-finite input returns `EigenError::FaerGevd`. Ordering, the `ComplexEigenvalue` / `SingularPencil` errors and the M-normalized eigenvectors are unchanged (#800).

## [0.7.0] - 2026-10-01

This release completes the `geode` CLI's EDA-flow epic (#702). It adds:
- lossy and open-cavity eigenmodes;
- design sensitivities for every analysis;
- a scalable iterative driven path (an AMS preconditioner) with an adaptive frequency sweep and parallel frequency points;
- published JSON Schemas, a cookbook and prebuilt binaries.

It also fixes several precision and convergence bugs that could silently degrade results.

### Added

#### `geode` CLI

- `geode eigen` solves lossy and open-cavity problems (lossy dielectrics, UPML), reporting complex frequency, f and Q per mode (#706, #737).
- **Design sensitivities:** a new `sensitivity` spec section. Every gradient is computed by an adjoint, and an optional `fd_check` verifies each one against finite differences (#707, #741, #739, #750). Available observables:
  - capacitance: two-terminal C at P2, as `c_farad_p2`;
  - inductance: L_ij;
  - lossless eigen frequencies;
  - driven: |S11|², for one lumped port.
- **Scalable driven solves** (#708):
  - `solver.preconditioner` selects `jacobi` (default), `ilu0` or `ams` for iterative solves (#742, #744, #752).
  - `ams` is a Hiptmair–Xu auxiliary-space preconditioner with an exact nodal coarse solve. On the spiral inductor it converges where Jacobi and ILU(0) stall. At 228k edges it takes 33 s and 1.7 GB, against direct LU's 73 s and 17.8 GB. A spec with floating PEC conductors is rejected up front with `invalid_spec`.
  - `sweep.adaptive {tolerance, max_snapshots}` runs a Galerkin reduced-order sweep. Interpolated rows carry an error indicator. A frequency that misses tolerance, or whose reduced solve is singular, falls back to an exact solve (#745, #747, #753).
  - `--jobs N` solves frequency points in parallel. Each job gets `max(1, threads/N)` threads, so jobs don't oversubscribe the CPU (#745, #753, #755, #762).
  - `--progress` writes JSONL progress events to stderr (#745).
- **Mesh:** `geode mesh --analysis capacitance|inductance` emits starter specs with nets, ground and contacts (#720, #727).
- **SPICE:** `--spice-positive-k` and `--spice-ret-pin` export variants (#723, #733).
- **`geode check`:** reports `anchor_nnz_ratio` and `above_anchor` resource-estimate fields, plus an AMS memory estimate (#713, #735, #752).
- **Touchstone:** the output path is checked before the solve, and the export is round-trip-checked with scikit-rf in CI (#713, #735).

#### Distribution (#709)

- Published JSON Schemas for the spec and the report (`geode schema`, `crates/geode-cli/schemas/`, with a drift test), plus an examples cookbook in `crates/geode-cli/examples/` (#728).
- Prebuilt `geode` binaries for linux x86_64/aarch64 (glibc ≥ 2.35) and macOS arm64, attached to each GitHub Release, plus a Gmsh-equipped container (`docker/geode-cli/`) (#726).

### Changed

- **Iterative driven solves fail honestly:** a solve whose recomputed explicit residual misses `solver.tol` now fails with `solve_failed`, even if the recursive residual converged. The error suggests loosening `tol` (#744, #749).
- **`--threads 1` and `GEODE_NUM_THREADS=1` now mean a serial LU.** Previously faer's LU still used every core. Results at one thread can change at roundoff (#753, #755, #762).
- **`--jobs` bit-identity:** `--jobs N` matches `--jobs 1` bit for bit only at the same threads per factorization. Otherwise the results agree to roundoff, about 1e-14 relative (#753).

### Fixed

- **Nédélec precision:** material and source weights are now uploaded at backend precision (f64). Before this, ε was rounded to f32 (#740, #746).
- **Surface triangles:** a surface triangle that is not a tet face returns `SurfaceNotOnMesh` instead of panicking, with a per-tet-face check and an integration fixture (#725, #729, #732, #751).
- **SPICE `ret` pin docs:** the floating `ret` caveat (it needs a DC path to `0`) is now documented, and the README usage lines are fixed (#736, #738).

### API notes (geode-core, pre-1.0)

- `waveguide_mode_reduce` now returns `Result`.
- `RomError::UnsupportedOperator` is removed.
- The capacitance sensitivity observable is renamed `c_farad` → `c_farad_p2` (#748).
- New: `driven::solve_ams` and `IterativePreconditioner::Ams`, `eigen::parallel::ParallelismGuard::cap` and `with_thread_budget`.

The spec and report schemas stay at v1; every CLI change is additive.

## [0.6.0] - 2026-09-29

This release turns the `geode` CLI into a static parasitic-extraction tool
for EDA flows (Epic #702): layout geometry goes in, and tagged meshes,
Maxwell capacitance and inductance matrices, and SPICE subcircuits come
out, alongside the Touchstone S-parameters added in 0.5.0.

### Added

#### `geode` CLI — EDA flows (Epic #702)

- `geode mesh`: turns a versioned, tool-neutral layout description
  (rectilinear conductor polygons, a dielectric layer stack, lumped gap
  ports, PEC or UPML outer boundary) into an MSH 4.1 mesh with
  automatically named physical groups, plus a starter problem spec, by
  driving an external `gmsh` binary (`--gmsh` / `GEODE_GMSH` / `PATH`,
  `--gmsh-timeout`). The report records the Gmsh version and sha256 of the
  layout, script and mesh. Golden test: the spiral inductor rebuilt from a
  layout, L −4.3 % vs Mohan / −6.3 % vs MoM (#704, #712).
- `geode capacitance`: Maxwell capacitance matrix between named conductor
  surfaces, with one or more grounded surfaces. Golden test: coaxial and
  triaxial lines, ≤ 0.2 % vs analytic on the benchmark mesh (#705, #716).
- `geode capacitance --spice <PATH>`: exports the matrix as a mutual-C
  SPICE `.subckt` (ground branches from row sums, noise-level branches
  dropped and listed, significant sign violations rejected), verified by
  an ngspice AC admittance check (#715, #717).
- `geode inductance`: static inductance matrix for open current paths
  (conductor volume from a source face to a sink face), with a new
  `materials[].mu_r`. Source and sink must sit on the same connected
  piece of the required PEC wall, checked before solving; a non-physical
  L (non-SPD or L_ii ≤ 0) fails with `solve_failed`. Golden test: coax
  and triax, ≤ 0.35 % vs analytic on the benchmark mesh, including a
  μ_r = 4 core (#714, #718).
- `geode inductance --spice <PATH>`: exports the L matrix as an
  `LEXTRACT` `.subckt` (one inductor per path to the PEC return,
  `K` couplings with sign preserved), verified by an ngspice AC check
  (#719, #722).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::assembly::current_path`: general open-path current
  excitation. A P1 conduction solve gives J = −σ∇φ normalised to 1 A,
  discretely divergence-free and verified against the analytic
  wire/loop builders. Grounded-component analysis with a per-path
  current-balance check (`CurrentPathError::UnbalancedGround`) (#718).
- Electrostatic face-flux helpers made public (#718).

### Changed

- `materials[].eps_r` now defaults to vacuum (1) for every analysis, so a
  spec that omits it now parses where it used to fail. A non-unit `mu_r`
  is rejected outside `geode inductance` (#718).
- CI installs Gmsh (4.12.1 on Ubuntu) for the `geode-cli` job and fails,
  rather than skips, the Gmsh tests if it is missing. It also runs the
  inductance golden benchmark tier in release mode (#712, #718).

### Fixed

- `geode mesh` now hollows thick conductors (cut out of the dielectric
  and any sheets they pierce), so Leontovich R/Q match the benchmark:
  spiral R −5.9 % → +0.16 %, Q +6.7 % → +0.76 %. Coplanar-wall merging
  is disabled so no cavity face is left untagged, and new mesh-time and
  solve-time guards reject untagged or dangling surface triangles
  (#721, #724).

## [0.5.0] - 2026-09-28

This release completes the `geode` CLI's second phase: alongside `check`
and `driven`, it now computes cavity eigenmodes (`eigen`), extracts L/R/Q
with a quasi-static f→0 L₀ extrapolation (`extract`), handles open
radiating problems with wave ports and UPML / Silver-Müller boundaries,
exports fields and far-field patterns, and writes Touchstone files for
EDA tools. Every committed benchmark artifact produced through a Burn
backend was audited for float precision. Three had been generated in
f32, and CI now enforces f64 provenance. The audit also uncovered a
singular σ = 0 shift that had silently broken the sparse Mie eigensolve;
it is fixed and now guarded.

### Added

#### `geode` CLI — Phase 2 (Epic #680)

- `geode eigen`: lossless PEC-cavity eigenmodes via sparse shift-invert
  Lanczos, with a residual acceptance gate (`eigen.residual_tol`) so a
  non-converged solve fails with `solve_failed` instead of reporting wrong
  modes. Golden test: dielectric sphere in a PEC cavity vs analytic Mie
  roots (#681, #685).
- `geode extract`: per-frequency L/R/Q, the quasi-static L₀ via
  Richardson extrapolation (optional `l0_rel_tol` gate) and SRF. Golden
  test: SLCFET 3HP, L₀ −2.6 % vs MoM-PEEC (#682, #686).
- Open boundaries and wave ports in the problem spec: box UPML
  (`absorbing_regions`), Silver-Müller walls, and wave ports built from a
  tagged mesh face. Golden test: patch antenna, f_res −6.5 % vs the
  Balanis cavity model (#683, #688).
- `--outdir`: opt-in VTU field export and NTFF far-field results
  (directivity, gain, efficiency, principal-plane cuts). The NTFF box is
  validated before the sweep (#684, #694, #697, #699).
- `--touchstone <PATH>` on `driven` / `extract`: Touchstone 2.0 `.sNp`
  output (S-parameters, RI, per-port `[Reference]` impedances), referenced
  from the report with its sha256; wave-port specs and `eigen` are
  rejected up front (#703, #711).
- `geode check` reports `resources`: nnz(A) and order-of-magnitude
  memory/time estimates for the chosen solver, calibrated on a 1.16M-DOF
  run and documented with measured error bars (#703, #711).
- Spec and report stay schema v1; all changes are additive.

#### Library

- `geode_core::eigen::pec_cavity`: tagged mesh + named PEC group +
  materials → eigenmodes (#685).
- `geode_core::driven::extraction::extrapolate_l0`: public, panic-free
  replacement for two duplicated private helpers (#686).
- Wave-port construction from a tagged planar mesh face
  (`driven::ports::wave_face`) (#688).
- `EigenError::DegenerateShift`: the sparse complex and projected
  shift-invert Lanczos solvers now reject a degenerate σ ≈ 0 solve that
  collapses onto the gradient null space instead of returning spurious
  modes (#696, #701).
- Shared benchmark-provenance helpers in `geode-util` (`BackendInfo`,
  oracle-block carry-through on regeneration) (#689).

### Changed

- CI: a `benchmark_provenance` guard classifies every committed benchmark
  artifact and requires f64 provenance on Burn-backed ones. CI also now
  runs the default tiers of the spiral, SLCFET and patch benchmark tests,
  which never ran there before (#692, #698, #700).

### Fixed

- Benchmark artifacts that were generated on the f32 `wgpu` backend,
  now regenerated in f64:
  - SLCFET 3HP: Q and R had been ~2× off (#687, #689).
  - patch antenna: negligible drift (#690, #695).
- `examples/mie_sphere` used a singular σ = 0 Lanczos shift and on main
  returned only null-space modes. It now uses σ = 1.0, and the committed
  results were regenerated (TM₁,₁ ~5.7 %, TE₁,₁ ~1 %, TM₂,₁ 0.5–2.3 %)
  (#691, #693).
- The same σ = 0 bug in the ignored sparse Mie test and the
  `mie_end_to_end` benchmark; earlier sparse timings from that
  benchmark are invalid (#696, #701).
- A stray tracked `.loom-managed` worktree marker (#679).

## [0.4.0] - 2026-09-27

This release gives GEODE-FEM a headless command-line driver: a `geode`
binary that takes a mesh plus a JSON/TOML problem spec and emits a
versioned JSON report, so design flows and agents can run validated solves
without writing Rust. It also closes two latent hazards found along the
way — a spiral-inductor reference artifact that had silently been generated
in f32, and lint debt outside `geode-core` hidden by a narrowly scoped CI
gate.

### Added

#### `geode` CLI — Phase 1 (#673)

- New `crates/geode-cli` crate producing the `geode` binary, installable
  with `cargo install --locked --git … --rev <sha>` (#676).
- `geode check` validates a spec and mesh without solving (physical-group
  resolution, DOF counts); `geode driven` runs a lumped-port frequency
  sweep with PEC / Leontovich boundaries, direct-LU or iterative COCG, and
  reports Z/Y/S per frequency plus per-port L/R/Q/S11. `eigen` and
  `extract` are reserved and return `not_implemented`.
- Versioned problem-spec v1 (JSON or TOML) and report v1 schemas with
  explicit units, unknown-field rejection, stable error codes, and
  provenance (crate version, git sha, backend, mesh SHA-256); documented in
  `crates/geode-cli/README.md`.
- `geode --version` prints the git sha; the process exits non-zero on
  solver failure or non-convergence; `--backend` confirms the compiled-in
  backend and `--threads N` caps threading.
- Golden tests re-express the spiral-inductor benchmark as CLI input (smoke
  tier in default CI; full-mesh tier `#[ignore]`d) and assert agreement
  with both the committed results and the library (~1e-12).
- `geode_core::mesh::read_tagged_tet_mesh` / `TaggedTetMesh`: a public
  tagged-mesh reader with physical-group name lookup; the sphere, spiral,
  patch, and transmon fixture loaders now share it.

### Changed

- CI clippy gate widened from `geode-core` to the whole workspace
  (`--workspace --tests --features geode-core/arpack`), and a new CI job
  runs `cargo test -p geode-cli`; pre-existing lints fixed across
  `geode-util`, `geode-validation`, and the example crates (#675, #677).
- Vendored `.anvil/` tool install untracked and gitignored — it is a
  copy-installed tool tree, not project source.
- Loom orchestration tooling upgraded to 0.18.0.

### Fixed

- `benchmarks/spiral_inductor/results.toml` regenerated in f64. The
  committed artifact had been produced on the f32 `wgpu` backend, leaving
  R/Q at 1 GHz ~16–18 % off (L unaffected); no physics changed. Regeneration
  now carries the `[oracles.palace]` block through, a default-CI check
  requires `float_dtype = "F64"`, and the CLI golden test checks R/Q against
  the artifact (#674, #678).
- New Rust 1.98 clippy lint (`chunks_exact_to_as_chunks`) fixed in
  `geode-util` and `geode-validation` (#676, #677).
- Stale references to the retired `geode-cli` crate in `geode-app` docs.

## [0.3.0] - 2026-07-29

This release repositions GEODE-FEM as a differentiable-by-construction
complement to Palace: the full 2×2 sensitivity matrix (material + geometry ×
scalar + H(curl) EM) is FD-validated and mutation-tested, and it is exercised
end-to-end by a driven-Maxwell shape-adjoint chain that culminates in
many-DOF freeform inverse design of a curved conformal radiator. Alongside
the adjoint work, a matrix-free eigensolve path with a full Hiptmair–Xu AMS
preconditioner opens the road past the direct-LU memory wall, and a suite of
validated benchmarks lands: slotless-PM Arkkio torque T(θ) at 0.71 %, SMF-28
LP01 modal b at 0.88 %, and transmon capacitance/EPR extraction. The
workspace itself goes standalone (vendored Palace docker, external refs
scrubbed) and gains the `geode-app` harness, standalone example crates, and
the `geode-util` pre-core staging crate.

### Added

#### Differentiable sensitivities — the full 2×2 matrix (Epic #569)

- Discrete-adjoint material sensitivity ∂(observable)/∂ε on the scalar path
  (#570, #573) and geometry/shape gradient ∂(observable)/∂X (#571, #575),
  both FD-validated.
- H(curl) driven-EM material adjoint ∂/∂ε (#579) and geometry/shape adjoint
  ∂(EM observable)/∂(node coords) (#581), completing the matrix; complex-ε
  (loss-tangent) sensitivities on the driven adjoint (#598).
- Hellmann–Feynman eigenvalue sensitivities ∂λ/∂p (#600) and
  inductance-matrix reluctivity sensitivity ∂L/∂ν via the self-adjoint
  energy form (#615).

#### Driven shape-adjoint chain → freeform inverse design

- |S11|² shape-adjoint objective closure and the first driven-Maxwell
  optimization loop (#627); complex-ε lossy shape adjoint (#630);
  pinned-feed (#632) and moving-feed (#644) lumped-port terminations in the
  shape adjoint; box-UPML tensor-material adjoint at fixed Λ (#643).
- Composed open-radiator shape adjoint + full inverse-design capstone
  (#645); high-DOF freeform boundary parametrization with mesh-morph
  regularizer (#654); curved conformal radiator fixture (#653) and many-DOF
  freeform inverse design on it (#655).
- FDTD-density head-to-head baseline plus measured Meep runtime-scaling
  evidence for the intractability axis (#656, #657).

#### Matrix-free scale path and AMS preconditioning (Epic #547)

- Matrix-free Nédélec curl-curl + mass matvecs on Burn (#483), GPU-resident
  COCG over the matrix-free operator (#487), and
  `SolverMode::IterativeMatrixFree` on the driven pencil (#495).
- Matrix-free shift-invert Lanczos to scale past the direct-LU memory wall
  (#525), AMS-lite H(curl) preconditioner (#528), and a matrix-free MINRES
  inner solver for interior/indefinite shifts (#537).
- Full Hiptmair–Xu AMS cycle: three-space AMS as the SPD preconditioner for
  indefinite MINRES (#560), the vector-nodal ΠᵀAΠ block (#553), an
  O(node_dim) few-sweep coarse solve (#554) with a smoothed-aggregation AMG
  alternative (#566), and the `transmon_bench` scale harness (#552).
- Distributed groundwork: geometric k-way edge-DOF partitioner + halo map
  (#642) and a distributed matrix-free Krylov abstraction with a
  single-process mock collective (#646).
- Opt-in `InnerSolver::DirectCustomOrder` — custom fill-reducing LU ordering
  via faer's public deeper API (#543, #544).

#### Second-order elements

- 20-DOF second-order Nédélec tet element (#617) with an opt-in p=2 driven
  forward path + material adjoint (#621), p=2 PEC-cube eigensolve with a
  frequency-convergence gate (#622), and a p=2 shape adjoint via a dual
  element twin (#623).
- P2 Lagrange tets on the scalar magnetostatic path, retiring the 2.44 %
  B-field miss to 0.211 % at O(h²) (#472, #474), and on the electrostatic
  path with the adjoint retained (#608).

#### New solver physics

- 3-D electrostatic solver + Maxwell capacitance-matrix extraction (#481)
  and 3-D vector magnetostatics + inductance-matrix extraction (#512), with
  a 2-D scalar magnetostatic Poisson solver as the oracle rung (#460).
- Transient EM solver — generalized-α time integration with broadband
  S-parameters via DFT (#489); adaptive fast frequency sweep via greedy
  Galerkin PROM (#610).
- London superconductor surface BC (per-tag λ_L) on the driven and eigenmode
  paths, with ∂λ/∂λ_L via Hellmann–Feynman (#609).
- Divergence-free M-orthogonal projection for the eigen path, port-aware so
  the junction LC mode survives (#513, #515), plus a tree-cotree gauge
  module with its spectrum limitation pinned (#508).

#### Validated benchmarks

- Motor torque (Epic #448): PM magnetization sources and air-gap oracle
  (#463), Maxwell-stress + Arkkio torque extractors vs the loop T = m × B
  oracle (#464), and the driven slotless-PM locked-rotor T(θ) benchmark at
  0.71 % (#465), on multi-band annular meshes with ν heterogeneity oracles
  (#462).
- Optical fiber (Epic #339): full-vector mixed E_t–E_z Nédélec–Lagrange
  dielectric modal pencil hitting SMF-28 LP01 b within 0.88 % (≤ 1 % gate),
  with a regression tripwire for the old reduced-pencil artifact (#473,
  #477) and an audit of the ε-coupling term that pencil dropped (#461);
  analytic-cladding DtN fiber solver (#447); chromatic dispersion D(λ) + ZDW
  vs the analytic oracle (#482).
- Transmon (Epics #475/#476 groundwork): fixture ingestion + shared MSH tag
  scanners (#488, #494), Josephson junction as a lumped reactive shunt in
  the eigensolve (#496), EPR quantization + qubit parameters (#511), a
  differentiable capacitance→E_C chain (#586) driving gradient-based
  transmon-parameter optimization (#588) and island-pad shape optimization
  on the real 133k-tet mesh (#590), with harmonic mesh-morphing island
  deformation (#599).

#### New `geode-util` pre-core staging crate (Epic #414)

- Introduced `geode-util`, a pre-core staging layer above `geode-core` that
  collects shared helpers previously scattered across `geode-validation` and
  the example crates (module map `repo` / `convert` / `interop` / `fixture`
  / `viz`): fixture-repository helpers (#417), interop decoders (#418),
  edge-DOF → nodal reconstruction (#419), and the shared fixture
  TOML/pvd/sweep harness (#423). `geode-validation` now consumes these and
  retains only genuine validation-harness code, with JSON fixture
  loader/schema and serde glue migrated in Epic #429 (#434, #435, #437).

### Changed

- The workspace is fully standalone: Palace docker tooling vendored in-repo
  and all external repository references scrubbed (#542).
- Examples restructured into standalone top-level crates (mie, patch
  antenna, spiral inductor, SLCFET, waveguides, fibers; #402–#412) driven by
  the new `geode-app` clap harness with arg groups and a lifecycle seam
  (#400); example crates now depend on `geode-util` instead of
  `geode-validation`.
- Bunsen integration closed out (Epic #355): named shape contracts wired
  across the P1 gather cluster, assembly, and basis/eigensolver paths (#466,
  #467, #469) and the git pin swapped to crates.io bunsen 0.28.0 (#507).
- Assembly and factorization hot paths parallelized: rayon on the Nédélec
  host-side pattern/slot build (#538) and in faer's sparse-LU factorization
  (#521); M·v_j caching drops O(k²) reorthogonalization SpMVs (#510).
- Orchestration/authoring tooling (Loom, Anvil, Repo Skills) kept current
  across the cycle (#540, #597, #625, #662, #667, #670, #671).

### Fixed

- Transmon fixtures resolve mesh groups by name, with the real DeviceLayout
  fixture swapped in (#494).
- `geode-validation` rustdoc lints repaired and the `cargo doc -D warnings`
  gate widened to the whole workspace (#541).
- Dead macos-13 CI leg removed from cube-cavity-tolerance, ending 24 h hangs
  (#443).

### Removed

- Deleted the orphaned `examples/_support` (`geode-examples-support`) crate
  after its last consumer moved to `geode_util::viz` (Epic #414 Phase 3,
  #426, #427).
- Deleted the external-reference MoM baseline crates as part of the
  standalone move (#542).

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
