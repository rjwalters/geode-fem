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
   per-core-efficiency inset.
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
   Public contact (from their repos' commit metadata):
     Hugh Carson <hughcars@amazon.com>   (Palace lead committer)
     Greg Peairs <gpeairs@amazon.com>    (DeviceLayout.jl lead, blog author)
     Simon Lapointe <simlap@amazon.com>  (Palace #2, backup)
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
