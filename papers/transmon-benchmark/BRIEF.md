---
title: "Cross-validating an open-source Rust FEM solver against Palace on a superconducting transmon geometry"
slug: transmon-benchmark
skill: pub
venue_target: "arXiv preprint (physics.comp-ph, cross-list quant-ph); possible later submission to a computational-physics journal"
audience: "Computational electromagnetics practitioners and superconducting-qubit design engineers; secondary: open-source scientific-software community"
length_target: "8-12 pages two-column (or ~15 single-column), 4-6 figures"
authors: "Robb Walters and Crutcher Dunnavant (in that order), equal contributors — standard equal-contribution footnote on both names; affiliations still TODO(operator); + AI-orchestrated development acknowledgment"
status_of_inputs: "Agreement + CPU cell + GPU correctness FINAL; GPU performance = declared future work"
web_search: true
claim_area: "cross-solver validation of open-source finite-element electromagnetics solvers on superconducting-qubit geometries"
closest_prior_work: "Palace (unpublished software); DeviceLayout.jl transmon workflow (blog/talk only); TEAM benchmark-problem tradition in computational electromagnetics; EPR/BBQ quantization papers for the physics framing"
---

# Brief: the transmon cross-validation benchmark paper

## v7 preparation — 2026-10-10 (#1039)

#1039 requests one new version beyond the v6 cap: fold the committed post-v6
measurements into a draft while the real-device multi-parameter anchor
experiment (#1034) is still in progress. The cap is raised to 7 for this pass,
pending operator confirmation (requested on PR #1040); v6 and its review/audit
remain immutable.
This does not confer a new review score or publication readiness.

- Harmonic motion (#594): 8.90× safe-budget extension, but a 128.792998 fF
  endpoint. Replace the initial-slope 3.54× headline with the conditional
  endpoint-slope estimate of at least 6.3×, explicitly stating its assumption.
- CPU timing (#964/#927): replace the old unmatched-work efficiency claim and
  plot with all three mode-recovering routes, OS pinning, measured CPU time,
  and Palace field-output/error-estimator caveats. Include the subsequent
  tuned-build control without pooling sessions.
- Port-subspace update (#1002): seven requested modes recover six physical
  modes, with the 3.45 GHz mode still unresolved under #1003. Its local timing
  is not a replacement for the EC2 timing cell.
- AMS at scale (#967): assembled CPU-f64 driven measurements, separate from
  the transmon eigenmode and GPU experiments.
- Keep a visible `[PENDING issue-1034]` anchor section and every existing
  `TODO(operator)` marker. The real-device optimizer result, independent
  review/audit, and operator publication inputs remain outstanding.

The new draft's `provenance.md` maps these changes to committed evidence;
its `validation.md` records checks without claiming an independent audit.

## ⭐ REFRAME 2026-07-16 (operator direction — SUPERSEDES the 2026-07-14 framing below where they conflict)

**The paper's contribution is now DIFFERENTIABLE TRANSMON DESIGN — gradient-based
optimization of the electrostatic (LOM) Hamiltonian parameters — with the
cross-validation benchmark as the correctness credential, not the headline.**
The full spine, PROVEN-vs-ROADMAP capability ledger, and section plan live at
`docs/research/transmon-paper-reframe.md` (committed, reviewed via PR #587) —
the reviser MUST read it and treat it as the section-level outline for v5.
Supporting strategy docs: `docs/research/2026-07-16-strategic-direction.md`
(the research-backed pivot), `docs/research/geode-vs-palace-comparison.md`
(honest perf comparison), `docs/research/driven-first-performance-strategy.md`.

### What resolved since v4
- **The v4 FRAMING DECISION GATE below is RESOLVED — Branch A (measured), and
  superseded**: `benchmarks/gpu_driven_scaling/results.toml` (#501) landed and
  the GPU-f32 cell LOSES at the measured sizes (~44× slower than assembled-CSR
  CPU at n=15; f32 residual floor 5.4e-3). Do NOT take Branch B. The honest
  GPU story: correctness proven, performance aspirational/gated (#519/#520/#534).
- **The scale story is now measured and honest** (already folded into v4's
  main.tex by PR #557): at 1.16M DOF geode-direct completes (565.5 s / 92.2 GB)
  but loses to Palace (423 s / ~33 GB) on both axes — a flop-and-fill
  crossover below 1M, not merely a memory wall. Log committed at
  `benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`.
- **The matrix-free interior eigensolve is retired as the scale story**
  (`docs/research/driven-first-performance-strategy.md`): the σ=4.5 GHz
  deep-shift inner solve plateaus coarse-solve-invariantly
  (`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`,
  issues #562/#565) — the SPD-proxy preconditioner is the limiter; even an
  exact coarse solve stalls. Keep as honest roadmap, not a result.

### The NEW load-bearing content for v5 (all PROVEN, merged, committed)
1. **The differentiable-sensitivity capability (the contribution).** GEODE's
   Burn tape reaches assembly but the faer factorization breaks it; the
   discrete-adjoint layer closes that gap. The full 2×2 matrix is merged and
   FD-validated (each judge-verified with mutation tests):
   - material ∂/∂ε, scalar electrostatic: `crate::adjoint` (#570/PR#573, ~3e-8)
   - geometry ∂/∂X, scalar electrostatic: `crate::shape` (#571/PR#575, ~1e-9;
     exact ∂K/∂node via forward-mode Dual through the P1 kernel)
   - material ∂/∂ε, complex driven H(curl) Nédélec: `crate::driven::adjoint`
     (#576/PR#579, 2.3e-5; complex-symmetric transpose-solve reuses the LU)
   - geometry ∂/∂X, complex driven H(curl): `crate::driven::shape`
     (#577/PR#581, 2.2e-9; the RHS b(X) geometry term is load-bearing —
     dropping it fails FD at 0.58)
2. **The capacitance→E_C chain** (#583/PR#586): `shape::capacitance_shape_gradient`
   — ∂(C, E_C, anharmonicity)/∂θ. Elegant result worth a paragraph: C=φᵀKφ is
   variationally stationary → the adjoint vanishes and the shape derivative is
   a pure explicit-geometry (Hellmann–Feynman-like) term. Validated vs analytic
   parallel-plate (∂C/∂d=−ε₀ε_r, ∂C/∂A=+2ε₀ε_r, ~1e-10) AND central FD (~1e-9).
3. **The centerpiece figure — gradient-descent to a target E_C** (#584/PR#588):
   `benchmarks/transmon_diffopt/results.toml`. Newton hits the 0.2156 GHz
   target; an INDEPENDENT fresh forward solve confirms at rel-err 1.4e-15.
   HONESTY RULE: the parallel-plate parametrization is affine, so 1-step Newton
   is expected — frame as proving the loop end-to-end, NOT a hard optimization;
   the damped-Newton 13-step curve is the descent illustration.
4. **The real-device demo + honest negative** (#589/PR#590):
   `benchmarks/transmon_diffopt/pad_results.toml`. ∂C_Σ/∂θ FD-validated ON THE
   REAL 133k DeviceLayout mesh (rel-err 1.15e-4, clean O(h²) sweep); a genuine
   non-affine 2-step Newton convergence to a within-budget target (fresh-solve
   confirmed, 5.6e-6); AND the honest negative: the 89.9 fF anchor needs
   θ≈−0.241 but fixed-topology pad scaling caps at θ=−0.0073 (33× short —
   junction-attachment nodes crush ~0.7 μm tets). Closing the anchor gap is a
   mesh-morphing problem, not a scale problem. This finding is publishable
   content — present it as the mature-engineering result it is.
5. **The motivation from the literature** (from the 2026-07-16 deep-research
   pass). ⚠️ CITATION-HYGIENE WARNING (v4 reviewer, verified against the live
   arXiv API): three of the originally-noted IDs are MISALIGNED — 2408.12704
   is the Safavi-Naeini gradient-optimization/qubit-discovery paper, NOT
   QDesignOptimizer; 2312.13483 is SQuADDS (already cited as
   `shanto2024squadds`), NOT SQcircuit; 2407.10273 is a photonics
   quantized-inverse-design paper, NOT FDTDx (already cited). The litsearch
   re-run must resolver-verify every entry; the reviser cites ONLY verified
   keys. The claims to support (paper names, not IDs): superconducting-qubit
   design is guess-and-check because the EM solvers are not differentiable
   (arXiv:2508.18027 — confirmed); the QDesignOptimizer workflow bolts
   analytic-model gradients onto non-differentiable HFSS (find the correct
   ID); SQcircuit differentiates at the lumped-circuit level via PyTorch and
   notes sparse-eigenpair gradients as a gap (find the correct ID);
   differentiable-EM is owned by photonics FDTD/integral methods (FDTDx —
   already in refs; TorchGDM arXiv:2505.09545 — confirmed), leaving
   frequency-domain FEM for RF/superconducting uncontested; JAX-FEM proves
   full-autodiff FEM at scale in solid mechanics (Nature Comp. Sci. 2023,
   resolvable by DOI). Also verify arXiv:2603.29718 actually carries the
   Jacobi–Davidson + Helmholtz-projection method before anchoring the
   eigenmode roadmap on it. Palace/HFSS structurally cannot produce
   solver-derived design gradients — that claim rests on their documented
   architecture, not on any single citation.

### Scope discipline for v5 (from the reframe spine — do not violate)
- **LOM now, eigenmode as roadmap**: the differentiable contribution is the
  electrostatic/LOM branch (E_C, α≈−E_C, coupling C). Eigenmode/EPR
  differentiation is ROADMAP, blocked on the σ=4.5 eigensolve wall — named
  path: Jacobi–Davidson + Helmholtz projection (arXiv:2603.29718) +
  Hellmann–Feynman adjoint-eigenpair formulas. NO overclaim.
- **Do not over-index on qubits**: the sensitivity capability is general
  (RF/photonic/shape optimization); the transmon is the demonstration vehicle.
- The E_C anchor gap (C_Σ 136.7 fF vs ~90 fF anchor) stays an honest negative.
- Everything measured in v4 (agreement table, CPU cell, GPU correctness,
  honest physics notes) remains valid supporting material — the reframe
  changes WHY the paper exists and what leads; it does not invalidate data.
- Title class: "Differentiable transmon design via a tensor-native FEM
  electromagnetics solver: gradient-based optimization of charging energy,
  cross-validated against Palace" (final wording operator-approved; keep the
  tensor-compiler substrate as the enabling architecture, one section).

## Thesis (one sentence) — REFRAMED 2026-07-14 (operator direction)

Machine-learning tensor-compiler stacks are a viable foundation for
production-grade computational electromagnetics: a full-wave H(curl) FEM
solver built on one (GEODE-FEM, on Burn/cubecl — batched element-local
tensor kernels, one codebase retargeting CPU/CUDA/WebGPU) reproduces the
reference MFEM-based solver (Palace) to 0.032% worst-case across all six
eigenmodes of a real superconducting transmon geometry on identical meshes,
matches it on serial efficiency (~4× per-core), and gains a portable GPU
execution path essentially for free — with the cross-validation benchmark
serving as the evidence standard the claim is held to.

> **OPEN CAVEAT on the efficiency clause (issue #927, 2026-10-08; see also
> #763 and #593).** The "matches it on serial efficiency (~4× per-core)"
> clause, and the later "one core 28.7 s beats eight ranks 44.5 s, ~12× fewer
> core-seconds" form of it, compare wall clocks for the same *request*
> ("6 eigenmodes near 4.5 GHz"), not for the same *returned modes*. Palace
> returns six physical modes. geode's ungauged shift-invert returns the six
> eigenvalues nearest the shift: in every run of that request that has a mode
> log (Lambda A100, `benchmarks/transmon_bench_gpu/results.toml`; a developer
> machine, `benchmarks/transmon_bench_cpu/results_like_for_like_local.toml`)
> those are 4 gradient near-kernel modes, the spurious 3.45 GHz port mode and
> **one** physical mode. The m6i runs behind the CPU table have no committed
> mode log, so their mode content is unverified. The eigenvalue-agreement
> result (0.032%, six modes) is unaffected: that gate uses a 20 GHz shift and
> 12 modes.
>
> **RE-TIMED 2026-10-08 (same box as Palace, mode logs committed, one-thread
> runs pinned by the OS):**
> `benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml`. The request
> behind the clause returns one of the six modes on the m6i too. Three geode
> requests return all six; they are reported side by side and no winner is
> chosen. **Neither "~4× per-core" nor "one core beats eight ranks, ~12× fewer
> core-seconds" is reproduced, and neither may be carried into a new draft or
> a publication.** What the measurement supports, and the exact lines of
> `transmon-benchmark.6/main.tex` it confirms, weakens or refutes, are listed
> under "CPU cell: 2026-10-08 re-measurement and the claims it changes" below.
> Which of the three requests the paper presents is an operator decision
> (#927, #593); `main.tex` is not edited here.

### FRAMING DECISION GATE (operator direction, 2026-07-14) — resolves when
### benchmarks/gpu_driven_scaling/results.toml (issue #501) lands

Two framings, selected by the GPU scaling result:

**Branch B (BOLD — take it if the GPU cell shows a significant win, e.g.
the CUDA-f32 matrix-free solve beats the best same-host CPU config by ≥2×
at the largest sizes with acceptable f32 accuracy):**
- The PAPER IS ABOUT GEODE-FEM AND THE APPROACH. Title class: "GEODE-FEM:
  full-wave finite-element electromagnetics on a general-purpose tensor
  compiler". Abstract leads with the architecture bet and the performance
  evidence.
- The transmon cross-validation becomes the FLAGSHIP CASE STUDY (one major
  section), not the paper's identity.
- Add a "validation portfolio" section (~1 page + one summary table)
  drawing on the repo's committed benchmark artifacts as breadth evidence:
  Mie sphere (driven Q_ext/Q_sca vs analytic series), spiral inductor
  (L within 5% of Mohan/PEEC), patch antenna (S11/pattern), rectangular
  waveguide modes (0.01-0.22%), motor torque (Arkkio T(θ) 0.71% vs exact),
  SMF-28 fiber (LP01 b=0.88% vs exact oracle), transmon (0.032% vs Palace).
  Each row: problem, oracle type, headline number, artifact path. NO new
  measurements — committed TOMLs only; where a benchmark has an honest
  caveat (e.g. fiber's oracle-fidelity floor), carry it in the table notes.
- The GPU scaling table is the performance centerpiece; the CPU cell and
  agreement table support the flagship study.
- Same honest-negative spine; the concurrent-work (TensorGalerkin) and
  libCEED positioning from the .2.litsearch applies unchanged.

**Branch A (MEASURED — take it if the GPU result is a wash or mixed):**
- Keep the current reframe: tensor-compiler viability demonstrated through
  the transmon cross-validation, GPU correctness + scaling reported
  honestly, performance promise framed as architecture trajectory rather
  than demonstrated advantage.

Either branch: the wedge sentence keeps its three qualifiers (H(curl)
full-wave / general-purpose ML tensor stack / cross-validated at production
accuracy); wen2026learning cited as concurrent work; brown2021libceed as
the domain-specific-JIT foil.

### Framing consequences (the reviser/drafter must apply)
- TITLE shifts toward the architecture claim, e.g. "Tensor-compiler-based
  finite-element electromagnetics: cross-validating a Burn-native H(curl)
  solver against Palace on a superconducting transmon" (final wording
  operator-approved).
- INTRO leads with the tensor-compiler thesis (why ML-stack infrastructure —
  batched kernels, JIT, backend portability, f32/mixed-precision reality —
  maps onto FEM assembly and matrix-free operators); the transmon benchmark
  is introduced as the validation vehicle.
- The architecture section grows: batched [n_elem,6,6] element assembly as
  tensor ops, the matrix-free gather→batched-matmul→scatter-add apply, the
  on-device Krylov loop with O(1)-scalar sync budget, and the honest
  constraints (burn-cuda f32-only today; eigensolve factorization-bound on
  CPU — the tensor-compiler story currently covers assembly + driven
  solves, NOT sparse direct factorization; cite issues #502/#503 chain).
- RELATED WORK gains a dedicated axis (see litsearch re-run): form
  compilers and code-generation FEM (FEniCS FFC/FFCx, Firedrake TSFC,
  libCEED — NOTE the irony/positioning: Palace itself runs on libCEED, a
  domain-specific element-kernel JIT; our claim is about GENERAL-PURPOSE
  ML tensor stacks), differentiable/ML-framework EM (Ceviche, PyTorch-FDTD,
  JAX-FEM class), GPU-EM solvers generally. The wedge: to our knowledge no
  full-wave 3D H(curl) FEM solver has been built on a general-purpose ML
  tensor-compiler stack and validated at production accuracy against the
  reference solver.
- The honest-negative culture stays the paper's spine — unchanged.
- All existing numbers/sections remain valid; this is a reframe of WHY the
  paper exists, not of what was measured.

## Why this paper (positioning)

- Palace (awslabs/palace, MFEM-based) is the reference open solver for
  superconducting-qubit EM design, but was never published as a paper (repo +
  AWS blog + MFEM-workshop talk only). The DeviceLayout.jl transmon workflow
  (AWS blog, Peairs & Carson 2025; JuliaCon 2025 talk) likewise has no
  journal artifact. Independent, quantitative, third-party validation of
  this stack does not exist in the literature. This paper supplies it — and
  exceeds the original's formality (preprint > blog).
- The comparison is *same-mesh, same-physics, same-junction-model*: geometry
  generated by DeviceLayout.jl's own SingleTransmon example, meshed once,
  consumed by both solvers. Agreement claims are therefore about
  formulation/solver correctness, not meshing luck.
- Secondary contribution: the methodology itself — oracle-first benchmark
  culture (exact analytic tripwires; inverse tests that must fail), a fully
  scripted, reproducible pipeline (fixture generation → both solvers → CSV
  comparison), and honest-negative reporting (spurious mode disclosed, absent
  qubit mode explained, per-cell caveats).

## The hard numbers (all FINAL unless marked TBD)

### Agreement (correctness cell) — FINAL
Same mesh (22,684 nodes / 133,314 tets / 156,863 Nédélec DOFs, 133,108
interior after PEC), Order 1 both solvers, junction as lumped reactive shunt
(L=14.860 nH, C=5.5 fF on the 4-triangle lumped_element group):

SOURCE OF TRUTH: benchmarks/transmon_eigen/results.toml (committed) — use these
verbatim; the headline is "all six modes agree to ≤0.033% (worst 0.032%)":

| Mode | geode-fem (GHz) | Palace (GHz) | rel_err_pct |
|---|---|---|---|
| resonator | 5.153 | 5.151335830348 | 0.032 |
| mode_2 | 15.465 | 15.46052107794 | 0.029 |
| junction LC | 17.490 | 17.49010903536 | 0.001 |
| mode_4 | 18.693 | 18.69165792915 | 0.007 |
| mode_5 | 20.703 | 20.69755679425 | 0.026 |
| mode_6 | 26.088 | 26.08089940472 | 0.027 |

Precision note (state in the paper's reproducibility section, and a repo
follow-up): the toml stores geode frequencies at 3 decimals while Palace
carries full precision, so rel_err_pct is computed against rounded geode
values — the agreement at full precision is slightly better (~0.029% worst);
we quote the committed-artifact numbers and disclose the rounding.

- Junction-participation mode ID: junction mode p=1.000, others ≤0.0005.
- L-doubling tripwire: junction mode 17.49 → 12.37 GHz, ratio 0.7071 = 1/√2
  exactly (Josephson √L scaling).
- Analytic anchor: f_LC = 1/(2π√(LC)) = 17.60 GHz.
- Palace rerun reproduces its committed eig.csv bit-for-bit (deterministic).

### Performance, CPU cell — FINAL
m6i.4xlarge (8 physical cores / 16 vCPU, 64 GB), us-west-2; both = full
pipeline (mesh load + assembly + solve + output), /usr/bin/time, n=3 where ±:

| Solver | Parallelism | Wall (s) | Peak RSS |
|---|---|---|---|
| geode-fem @3174015 | 1 process (serial faer sparse-LU shift-invert Lanczos) | 51.2 ± 0.4 | 3.1 GB |
| Palace @fba6a5b | 4 MPI ranks | 50.8 | ~0.7 GB/rank |
| Palace @fba6a5b | 8 MPI ranks | 30.6 ± 0.1 | ~0.5 GB/rank |

Honest read (verbatim intent): per-core, geode's serial direct-factorization
eigensolve is ~4× more efficient than Palace's distributed Krylov-Schur+AMS
on this 133k-DOF problem; at the whole-box level Palace's MPI parallelism
wins 1.7×. geode has no intra-solve parallelism today. Palace -np 16
(hyperthreads) refused by MPI binding — excluded, not a data point.

**Not like-for-like output (issue #927).** Read the table and the "honest
read" above as wall clocks for the same request, not for an equivalent answer:
see the OPEN CAVEAT under the thesis. The same applies to the `[matched.*]`
tables of `benchmarks/transmon_bench_cpu/results.toml` that later drafts cite
(28.7 s vs 44.5 s, ~12× core-seconds, and the off-target 12-modes-at-20-GHz
gap). What geode must be asked for to return Palace's six modes on this mesh,
and its cost relative to the 6-mode request on one developer machine, is in
`benchmarks/transmon_bench_cpu/results_like_for_like_local.toml` (geode only;
not a benchmark host; its times must not be set beside the Palace cells).

### CPU cell: 2026-10-08 re-measurement and the claims it changes

Source: `benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml`
(generated from `runs/2026-10-08_m6i_like_for_like/`; issues #927, #763).
One m6i.4xlarge (8 cores / 16 vCPU, us-east-1), otherwise idle, one session.
geode-fem @61e8571e (release, direct LU, f64); Palace @fba6a5b built from
`reference/palace/docker/Dockerfile` (Rocky 9 MPICH wrapper flags,
`-march=x86-64-v2`, no libxsmm; not tuned for the host, so its times are an
upper bound for a tuned Palace), `OMP_NUM_THREADS=1`. One thread
= one CPU enforced with `taskset`; 8 = one hardware thread on each core.
Wall = min / median / max of n = 3 unless marked; core-s = user + sys, median.

| Request | Solver (width) | Of Palace's 6 | Other modes | Wall (s) | Core-s | Peak RSS |
|---|---|---|---|---|---|---|
| 6 @ 4.5 GHz (historical) | geode-fem (1) | 1 | 5 | 32.6 / 32.8 / 33.7 | 32.8 | 3.16 GB |
| | geode-fem (8) | 1 | 5 | 26.9 / 27.1 / 27.2 | 38.6 | 3.24 GB |
| 30 @ 4.5 GHz ("more modes") | geode-fem (1) | 6 | 24 | 34.0 / 34.5 / 35.2 | 34.4 | 3.16 GB |
| | geode-fem (8) | 6 | 24 | 28.5 / 29.1 / 29.3 | 40.7 | 3.24 GB |
| 6 @ 20 GHz ("shift") | geode-fem (1) | 6 | 0 | 32.6 / 33.2 / 33.6 | 33.2 | 3.16 GB |
| | geode-fem (8) | 6 | 0 | 26.6 / 27.3 / 28.1 | 38.9 | 3.23 GB |
| 8 @ 4.5 GHz, port-aware | geode-fem (1) | 6 | 2 | 66.9 / 67.3 / 67.7 | 67.2 | 3.23 GB |
| | geode-fem (8) | 6 | 2 | 50.1 / 51.1 / 51.2 | 91.7 | 3.29 GB |
| 6 @ 4.5 GHz, fixture (Save = 6) | Palace (1) | 6 | 0 | 186.7 / 186.8 / 188.0 | 186.8 | 2.20 GB |
| | Palace (8) | 6 | 0 | 33.1 / 33.4 / 33.5 | 266.7 | 0.54 GB/rank, 3.8 GB total |
| 6 @ 4.5 GHz, Save = 0 | Palace (1) | 6 | 0 | 113.0 (n = 1) | 113.0 | 2.24 GB |
| | Palace (8) | 6 | 0 | 23.6 / 23.6 / 23.8 | 188.9 | 0.54 GB/rank, 3.8 GB total |
| 12 @ 20 GHz (historical off-target) | geode-fem (1) | 6 | 6 | 33.2 / 33.9 / 33.9 | 33.9 | 3.16 GB |
| | geode-fem (8) | 6 | 6 | 27.5 / 27.8 / 28.0 | 39.4 | 3.24 GB |
| | Palace (1) | 2 | 10 | 285.9 (n = 1) | 285.9 | 2.26 GB |
| | Palace (8) | 2 | 10 | 49.0 (n = 1) | 391.5 | 0.54 GB/rank |

Caveats that must travel with any of these numbers:

- "30 modes" was found by trial against the Palace eigenvalues. It is not a
  recipe.
- The 20 GHz shift was chosen knowing where the modes are, and it is a
  different request from Palace's.
- Port-aware is the only request not tuned against the oracle. It still
  returns two non-physical modes, which are dropped by matching Palace's
  frequencies (#950), and it costs about twice the others.
- Palace's fixture config writes six ParaView fields; geode writes none. The
  Save = 0 rows are the ones without field output. Save = 0 matches field
  output only: by Palace's own phase timers those runs still spend 5.0 s of
  23.6 s at 8 ranks and 40.2 s of 112.9 s at 1 rank in its error estimator,
  which geode has no counterpart for. Whether the estimator can be switched
  off at this Palace commit was not checked. It counts against Palace at both
  widths, the one-wide "33 to 67 s against 113 to 187 s" included.
- Palace is untuned (vendored recipe: no libxsmm, `-march=x86-64-v2`), so its
  times are an upper bound for a tuned build. Any statement that has geode
  ahead (one-wide wall clock, core-seconds) is about this Palace build.
- Palace's times differ from the 2026-07-14 session in opposite directions
  (1 rank: 186.8 s now, 130.9 s then; 8 ranks: 33.4 s now, 44.5 s then). The
  cause is not established and was not investigated; the builds differ and
  the July build flags were not recorded.
- Palace's core-seconds are close to ranks × wall because MPICH ranks
  busy-wait.
- Palace's `Target` is a lower bound (modes above it); geode's shift is a
  centre (modes nearest it). At 20 GHz the two return different mode sets.
- One mesh, one host, one session, Palace's fixture solver settings, neither
  solver built with `-march=native`, Palace built without libxsmm (the Palace
  of the committed baseline log had it).

**Lines of `transmon-benchmark.6/main.tex` and what the measurement does to
them** (for the operator's revision under #593; the file is not edited):

| Lines | Claim as written | Status | Replacement numbers |
|---|---|---|---|
| 229-231 (contributions) | "A matched CPU cell shows a per-core, small-to-medium-scale win for the direct eigensolve" | **Weakened.** A core-second advantage is measured for requests that return the six modes (against an untuned Palace whose ranks busy-wait; caveats above); a wall-clock win at 8 ranks is not | Core-s, geode 1 thread vs Palace 8 ranks: 7.7× (more modes), 8.0× (shift), 4.0× (port-aware) with Save = 6; 5.5×, 5.7×, 2.8× with Save = 0 |
| 1024-1025 (table caption) | geode-fem on one core "beats Palace on eight ranks in absolute wall clock" | **Refuted** | geode 1 thread 33.2 to 34.5 s (67.3 s port-aware) vs Palace 8 ranks 33.4 s (Save = 6), 23.6 s (Save = 0) |
| 1042 (Table `tab:cpu`) | geode-fem (1): 28.7 s, 3.1 GB | **Refuted as a one-core, like-for-like time.** Not pinned, no mode log; the request returns 1 of 6 modes | Same request, pinned: 32.8 s, 3.16 GB, 1 of 6. Six-mode requests: 34.5 s / 33.2 s / 67.3 s |
| 1043 | geode-fem (8): 29.0 s | **Replaced** | Same request: 27.1 s, 1 of 6. Six-mode requests: 29.1 s / 27.3 s / 51.1 s |
| 1044 | Palace (1): 130.9 s, 0.5 GB/rank | **Not reproduced**; cause not established (not investigated): the builds differ and the July build flags were not recorded | 186.8 s (Save = 6), 113.0 s (Save = 0, n = 1); 2.2 GB |
| 1045 | Palace (8): 44.5 s, 0.5 GB/rank | **Not reproduced**, and in the opposite direction from the 1-rank row; cause not established (not investigated): the builds differ and the July build flags were not recorded | 33.4 s (Save = 6), 23.6 s (Save = 0); 0.54 GB/rank, 3.8 GB over all ranks |
| 1048-1052 (off-target rows: 36.8 / 26.6 / 248.0 / 64.7 s) | Same workload for both solvers | **Refuted as like-for-like.** The two solvers return different mode sets: 2 modes in common | geode 33.9 s (1) / 27.8 s (8): six physical + six non-physical, 0.0003 to 26.09 GHz. Palace 285.9 s (1) / 49.0 s (8): twelve modes, 20.70 to 51.54 GHz |
| 1087-1089 (Fig. `fig:cpu` caption) and `figures/src/fig4_cpu_wallclock.py` | "geode-fem on one core consumes 28.7 core-s against Palace's 356.0 at 8 ranks (~12×)" | **Refuted** | 32.8 vs 266.7 core-s = 8.1× for the 1-of-6 request; see row 229-231 for the six-mode requests. 356.0 was 44.5 s × 8, never a measured CPU time |
| 1094-1098 | "serial direct factorization on one core (28.7 s) beats Palace on eight ranks (44.5 s) in absolute wall clock at ~12× fewer core-seconds (28.7 versus 356.0)" | **Refuted** (wall clock) and **weakened** (core-seconds) | As rows 1024-1025 and 1087-1089 |
| 1098-1100 | "geode-fem's own 8-thread run gives essentially no speedup (29.0 s)" | **Weakened.** With one thread enforced there is a modest speedup | 32.8 s → 27.1 s: 17% lower wall for 1.17× the core-seconds (142% CPU). The old "no speedup" is what two multithreaded runs would show (#763) |
| 1101-1104 | "the direct shift-invert is target-insensitive while Palace's iterative solve degrades off-target, so the gap widens at 20 GHz: 6.7× serial (248.0/36.8), 2.4× at 8-wide (64.7/26.6)" | **Refuted as a comparison.** geode's own cost is flat across shifts (confirmed: 32.8 s at 4.5 GHz, 33.9 s at 20 GHz); the Palace side timed twelve higher modes, not the same answer | Same-request ratios, different answers: 8.4× (285.9/33.9), 1.8× (49.0/27.8) |
| 1117-1119 | "geode-fem wins small-to-medium on speed, per-core efficiency, and target robustness" | **Speed: refuted at 8 wide; at 1 wide geode measured ahead against an untuned Palace whose time includes its error estimator. Per-core efficiency: measured ahead in core-seconds, smaller than stated, with the busy-wait and untuned-Palace caveats above. Target robustness: not established** | As above |
| 1247-1249 | "Derived ratios quoted in the text (core-second and speedup multipliers) are computed from the displayed table values" | **Weakened**: core-seconds were wall × ranks, not measured CPU time | Measured user + sys is in the results file for every cell |
| 1347 | "(iv) The CPU per-core win is small-to-medium-scale only" | **Weakened** in the same way as 229-231 | As row 229-231 |
| Table `tab:cpu` memory column (caption 1023-1024 already says per-rank), and any "~6× less memory" reading of it | Palace 0.5 GB/rank against geode 3.1 GB | **Per-rank figure confirmed; the 6× reading is weakened**, since it sets one rank against a whole process | geode 3.2 GB; Palace 3.8 GB over 8 ranks (its own estimate), 2.1 to 2.2 GB at 1 rank |

**Confirmed and unchanged:** the eigenvalue agreement (the six modes agree to
0.029% in every six-mode cell of this run; Palace reproduces its committed
`eig.csv` to about 1e-12); geode's peak RSS of about 3.1 to 3.3 GB at 133k DOFs;
the large-scale finding (not re-run).

**#763 answer.** The old "1 thread" row cannot be shown to have been
multithreaded, because it recorded no CPU time. Its 28.7 s is 13% below the
time measured with one CPU enforced (32.8 s) and 6% above the 8-thread time
(27.1 s), and it equals its own 8-thread row to 1%, which is what a
multithreaded LU in both old rows would give. At the current commit
`GEODE_NUM_THREADS=1` alone is serial (102% CPU, 32.7 s, unpinned).

### GPU cell — CORRECTNESS RESULTS FINAL; performance = future work
g6e.xlarge (1× NVIDIA L40S 46GB, driver 595.71.05, CUDA 13.2), 2026-07-14:
- CUDA-f32 correctness smokes PASS on physical hardware: matrix-free
  Nédélec matvec (matrix_free_cuda_f32_smoke, 15.0 s incl. GPU init/JIT)
  and on-device COCG (cocg_burn_cuda_f32_smoke, 3.3 s). First execution of
  geode-fem code on a physical GPU.
- The driven IterativeMatrixFree path is CUDA-compilable but shipped no
  runtime smoke (disclosed gap; repo follow-up issue filed).
- NO GPU performance numbers exist: large-fixture GPU timing and the
  Palace-GPU (libCEED/CUDA) cell are explicitly FUTURE WORK. The paper
  reports the correctness result + the architecture (on-device Krylov with
  O(1)-scalar sync budget) and defers the performance cell honestly.
Note honestly: geode's GPU path accelerates the DRIVEN solve (matrix-free
matvec + on-device COCG), NOT the eigensolve used in the headline
comparison; burn-cuda 0.21 is f32-only (cubecl disables f64) so the GPU
path is mixed-precision-qualified.

## Methodology content the paper must include

1. **Geometry/mesh provenance**: DeviceLayout.jl v1.15.0 SingleTransmon
   example (sapphire substrate with rotated anisotropic ε = R·diag(9.3, 9.3,
   11.5)·Rᵀ, ~36.87° in-plane; 7 named physical groups), gmsh mesh, MSH 4.1
   fixture, sha256-pinned. One documented gotcha: MFEM requires an MSH 2.2
   conversion (gmsh -save -format msh2) — physics-neutral (bit-for-bit
   eigenvalue reproduction).
2. **The junction model**: lumped reactive shunt on a surface group —
   K_port = (ℓ/(w·L̃))·S_Γ added to stiffness (frequency-independent),
   M_port = (C̃·ℓ/w)·S_Γ added to mass; real symmetric pencil preserved;
   identical treatment in Palace config (LumpedPort, reactive only, readout
   ports left open/lossless in v1 — no R so the pencil stays real).
3. **Solver architectures compared** (table): geode = sparse full-tensor
   Nédélec assembly (Rust/Burn f64) → real faer sparse-LU shift-invert
   Lanczos, single process; Palace = MFEM H(curl), SLEPc-class Krylov with
   divergence-free projection + AMS preconditioning, MPI-distributed.
4. **Honest physics notes** (both load-bearing for credibility):
   a. NO ~4 GHz qubit mode exists in this v1 junction model *by
      construction* — the physical transmon qubit needs L against the
      ~80-100 fF pad/shunt capacitance, not the junction's own 5.5 fF; both
      solvers agree on its absence (Palace's projected eigensolve hunted at
      σ=4.5 GHz and found nothing below 5.15). The blog's [4.14, 5.591] GHz
      spectrum is NOT reproduced by this model and we say so plainly; EPR
      post-processing over field modes (Phase C, future work) is the route
      to qubit quantities.
   b. geode's un-projected Lanczos leaks ONE spurious junction-localized
      mode (~3.45 GHz, participation 0.994) that Palace's divergence-free
      projection suppresses; disclosed, filtered by documented criteria,
      tree-cotree projection flagged as the fix (future work).
5. **Infrastructure reproducibility**: everything scripted (fixture
   generator committed; Palace config + provenance committed; benchmark
   commands recorded); EC2 instance types + hardware disclosed; costs
   optional footnote (~$0.77/hr CPU box).

## Figures (planned; figure scripts can consume benchmarks/*.toml + eig.csv)

1. Geometry/mesh render (transmon + resonator, physical groups color-coded).
2. Mode-frequency agreement: geode vs Palace scatter with Δ% annotations.
3. Junction-mode L-scaling tripwire (f vs L on log-log, 1/√2 line).
4. CPU-cell wall-clock bar chart (geode 1-proc vs Palace 4/8 ranks) +
   per-core-efficiency inset. (Must be redrawn from
   `results_like_for_like_m6i.toml`: the current bars are the 2026-07-14
   request, which returns 1 of 6 physical modes on the m6i as well, and the
   inset's 28.7 vs 356.0 core-s is not reproduced. Which geode request the
   bars show is the operator's choice, #927.)
5. (TBD-GPU) GPU cell results.
6. Spurious-mode illustration: participation spectrum geode vs Palace
   (the honest-physics figure — reviewers will love or demand it).

## Related work the litsearch/draft should cover

- Palace announcement + docs; MFEM (Anderson et al.); DeviceLayout.jl blog +
  JuliaCon talk (cite as software/URL/talk — no paper exists, note this).
- EPR quantization (Minev et al. 2021 npj QI); BBQ (Nigg et al. 2012 PRL);
  transmon (Koch et al. 2007 PRA) — for the qubit-mode discussion.
- Cross-solver FEM validation precedents (e.g., FEM code benchmarking
  literature, TEAM problems tradition in computational EM).
- Whitney/Nédélec elements (standard refs), shift-invert Lanczos, AMS.

## Submission logistics (researched 2026-07-15 — operator actions flagged)

**Category:** primary **physics.comp-ph** (Computational Physics — solver/method/
software papers incl. GPU-accelerated solvers live there); cross-list
**quant-ph** (the transmon audience that most needs the validation) and, under
the Branch-B project/approach framing, **cs.MS** (Mathematical Software — the
systems-paper community; JAX-FEM-class related work appears there). math.NA is
the alternate third slot if cs.MS feels off at submission time.

**Endorsement: WILL BE REQUIRED.** arXiv requires first-time submitters to be
endorsed per category-domain; auto-endorsement needs prior claimed arXiv
papers + institutional email. A search (2026-07-15) found NO arXiv publication
history for either author, so plan on a manual endorsement for physics.comp-ph
(one positive endorsement per domain; endorsers need several comp-ph-domain
arXiv papers dated 3 months–5 years back).

**Operator checklist (start EARLY — endorsement takes days):**
1. Create the arXiv account for the submitting author (Walters is the natural
   submitter as first author; use the most institutional-looking email).
2. Start a submission stub in physics.comp-ph — arXiv immediately says whether
   endorsement is needed and issues the six-character endorsement code.
3. Endorser candidates, in order of fit: (a) personal physics/CEM network;
   (b) authors active in exactly this space — the SQDMetal (Sommers et al.,
   arXiv:2511.01220) and Palace-workflow (Ye et al., arXiv:2511.09041) author
   groups publish in comp-ph/quant-ph and are GUARANTEED endorsement-
   qualified (Nov-2025 papers, inside the 3mo-5yr window);
   (c) the Palace/DeviceLayout authors — OPERATOR DECISION 2026-07-15: the
   endorsement ask goes to them bundled with the draft share (never cold).
   Public contact (Palace maintainers, via GitHub; no emails recorded here):
     Hugh Carson   (GitHub: @hughcars)     (Palace lead committer)
     Greg Peairs   (GitHub: @gpeairs)      (DeviceLayout.jl lead, blog author)
     Simon Lapointe (GitHub: @simlapointe) (Palace #2, backup)
   CAVEAT: verify they are arXiv-qualified endorsers at request time (needs
   their own comp-ph-domain arXiv papers within 3mo-5yr; Carson has no
   Palace paper; Peairs's record may predate the window) — the endorsement
   page checks a named person instantly. Hedge: share the draft with one of
   the (b) groups too; they are the guaranteed endorsement path.
4. Cross-list endorsements: get the primary (comp-ph) endorsement first;
   request quant-ph/cs.MS cross-lists at submission (moderators can adjust).

**No fees; moderation (not review) follows submission — typically 1-2 business
days to announcement once endorsed.**

## Coauthor note (2026-07-14)
Crutcher Dunnavant is the expected coauthor, and the two authors are
EQUAL CONTRIBUTORS (operator-confirmed): the author block carries the
standard "These authors contributed equally" footnote on both names.
arXiv note: equal contribution is expressed in the PDF byline footnote (no
special metadata field); name order RESOLVED (operator, 2026-07-14): Walters, Dunnavant.
Remaining author-block TODO(operator): affiliations only. Relevant intellectual lineage
for the paper: the whiteroom L1-L4 operator specification (Crutcher's) is
the architectural substrate GEODE-FEM's solver surfaces were mapped against
(tracker #5). OPERATOR QUESTION carried as a TODO: is the whiteroom spec
public/citable (repo/DOI), or acknowledged-not-cited? Coauthor review of the
draft happens via the operator's channel — the reviser should leave the
author block as \author{Robb Walters \and Crutcher Dunnavant} with the
affiliation TODO(operator) marker.

## Voice and framing rules

- Honest-science register: every claim traces to a committed artifact
  (results.toml / eig.csv / provenance file); misses and caveats are stated
  in the abstract, not buried.
- No marketing language about either solver. Palace is treated with respect
  as the reference implementation; the per-core result is presented as an
  architecture trade-off (direct vs iterative, serial vs distributed), not
  a victory lap.
- "TBD-GPU" placeholders must be impossible to mistake for results.
- Numbers in text must match the tables exactly (the pub audit will check).

## Starter references

refs.bib beside this brief seeds: Palace repo, MFEM paper, gmsh paper, Koch
2007, Minev 2021, Nigg 2012, DeviceLayout.jl blog, JuliaCon talk, faer,
Burn, ARPACK/Lehoucq (shift-invert Lanczos), Hiptmair AMS or Kolev-Vassilevski
(auxiliary-space preconditioning).
