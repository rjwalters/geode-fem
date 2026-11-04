# GEODE-FEM roadmap

_Last updated: 2026-10-26._ This is the single source for where the project is going. Each item links to a GitHub epic, and the epic holds the phase plan, its goldens and its acceptance criteria. When an epic changes scope, update this file in the same PR.

## Direction

GEODE-FEM is a **tensor-native, differentiable-by-construction FEM electromagnetics solver**. It complements [AWS Palace](https://github.com/awslabs/palace) rather than racing it on raw scale. The full argument is in `docs/research/2026-07-16-strategic-direction.md` and Epic #569. Two tracks carry that direction:

1. **The `geode` CLI for EDA flows** (v0.4 → v0.8). Spec in, Touchstone/SPICE/JSON out, with the physics real PCB, package and RF designs need.
2. **Design sensitivities** (the differentiator). Adjoint gradients of every reported observable, validated against finite differences. v0.10 brings this track to the CLI.

Rules that every release follows:

- **Never silently wrong.** An unsupported combination fails loudly (`invalid_spec` / `solve_failed`). The error says what to change.
- **Maximally useful for design work** (operator decision, #804). Don't block reasonable meshes. Warn, estimate accuracy per result, and say how far to refine (refine-to-h). Fail loudly only when no robust path exists.
- **Validated claims only.** Every feature ships with an analytic, literature or cross-tool golden. Its accuracy bar must be met honestly at a stated mesh; nothing is loosened to pass.
- **Additive schema v1.** Spec and report changes are additive, the schemas are regenerated, and the drift test passes.

## Released and pending

| Version | Theme | Status |
|---|---|---|
| v0.4–v0.7 | CLI Phases 1–3 (Epics #673/#680/#702): `check` / `driven` / `eigen` / `extract`, Touchstone, `geode mesh`, C/L + SPICE, lossy eigen, sensitivities, AMS + adaptive sweep, schemas, cookbook, binaries | Released |
| **v0.8.0** | **Physics breadth for EDA (Epic #756)**, plus **hybrid wave ports (Epic #778)** | Released 2026-10-04 |
| **v0.9.0** | **Library side of the four epics below** (#836 p=2, #841 differentiable EDA, #835 AMR, #837 Floquet): the phases marked **Merged** in the next section, plus `geode-optimize` and CLI N-port sensitivities | Released 2026-10-26 |

v0.8.0 contents, as merged:

- **Materials:**
  - Djordjevic–Sarkar, Debye and Drude dispersion (#757, #761);
  - diagonal anisotropic ε/μ (#760);
  - conductor roughness, Hammerstad–Jensen and Huray (#758).
- **Ports:**
  - mixed lumped + wave ports (#759);
  - filled (homogeneous) wave ports (#777);
  - wave ports with Leontovich and rough walls (#776);
  - Touchstone for wave ports via `reference_ohm` (#775);
  - adaptive (PROM) sweep with wave ports (#774);
  - TE-port TM-cutoff guard with a mesh-aware margin over 3 λ_c of the feeding guide (#808, #824, #845).
- **Hybrid wave ports (Epic #778).** Microstrip, stripline, coax and inhomogeneous faces, delivered in phases:
  - P1 (#803): the hybrid mode solver;
  - P2 (#804): driving the 3-D solve with those modes, with complex-pair blocks and per-mode accuracy estimates;
  - P3a and P3b (#805, #817): the quasi-TEM mode, Z_PI/Z_PV/Z_VI, and coupled even/odd lines;
  - P4 (#806): lossy and dispersive substrates;
  - P5 (#807): the CLI. Z_PI is the default Touchstone impedance, and the Z₀ accuracy estimate warns. The microstrip cookbook lands at 49.3 Ω for a 50 Ω design.
- **Correctness fixes:**
  - face-exact PEC masks (#771);
  - weak-guidance fiber modes (#791, #794, #798);
  - faer complex and real QZ replaced by dense shift-invert, fixing a hang and wrong eigenvalues (#796, #800, #813);
  - mesh-unit-invariant thresholds (#814, #826);
  - high-Q `Im k` precision (#830).
- **CI:** 157 of 160 test targets now run in CI, up from 41. A fail-closed coverage guard tracks both integration targets and ignored tiers (#785–#793).

## v0.10.0: differentiable EDA on a higher-order, adaptive foundation

These epics were planned as v0.9.0. v0.9.0 shipped on 2026-10-26 with their library phases (the **Merged** entries below), because the breaking `geode-core` changes they carry needed a minor version. v0.10.0 is cut when the CLI phases land.

| Epic | Theme | First phase | Status (2026-10-24) |
|---|---|---|---|
| #836 | **Higher-order accuracy.** p=2 Nédélec through driven, eigen, lumped/wave/hybrid ports, sensitivities and the CLI `element_order`, then curved geometry | #838: order-generic element space + driven solver | **Merged:** P1a #838, P1b #857, P2 #871, P3a #884. **Filed:** P4 #1264 (sensitivities at p=2), P5a #1265 (CLI `element_order`). **Unfiled:** P3b (hybrid ports at p=2), P5b (default flip, operator decision), P6 (curved geometry, likely v0.11.0). The wave-port gauge fix #888 (PR #892) and its p=2 follow-up #894 are done |
| #841 | **Differentiable EDA.** N-port S, material, roughness, dispersion, port-mode (β, ε_eff, Z₀) and lossy-Q gradients, CLI shape parameters, and `geode optimize` (L-BFGS-B / MMA) | #842: N-port S gradients through wave/filled/mixed/walled specs | **Merged:** P1 #842, P3a #859, P3b #872, P5a #883, P6a #873. **Filed:** P2a #1259 (material physics; its σ item moved to #1254), P2b #1260 (conductor/wall physics, lossy-eigen f and Q), P6b #1261 (`geode optimize`). **Unfiled:** P4 (adaptive PROM), P5b (CLI coverage for P2–P4), P7 (showcase) |
| #835 | **Adaptive meshing.** H(curl) a-posteriori estimator, conforming bisection, the solve→estimate→mark→refine loop, goal-oriented DWR, port-driven refinement, CLI `adaptive_mesh`, and hp | #840: explicit-residual estimator, validated on effectivity | **Merged:** P1 #840, P2 #860, P3 #868, and the P3 boundary residuals #879. **Filed:** P4 #1266 (goal-oriented DWR), P6 #1267 (CLI `adaptive_mesh`). **Unfiled:** P5 (port-face consistency), P7 (hp, after #836 P5a) |
| #837 | **Periodic / Floquet.** Zero-phase periodic BCs, Bloch band structure, Floquet ports (TE + TM, diffraction orders), CLI `geode mesh --periodic`, and infinite-array scan impedance | #839: zero-phase periodic BCs | **Merged:** P1 #839, P2 #858, P3a #870. **Filed:** P3b #1262 (higher Floquet orders and diffraction), P4a #1263 (CLI periodic BCs, Bloch eigen, `geode mesh --periodic`). **Unfiled:** P4b (CLI Floquet ports and scan sweeps). P5 (infinite-array scan impedance) moved to #1252 as its Phase 2 |

**Sequencing.** #838 was the **first solver-core merge** of this cycle (shipped in v0.9.0), because every other epic builds on the order-generic element space. After it:

- #839 (periodic), #840 (estimator) and #842 (S gradients) can run in parallel.
- The AMR epic's minimum deliverable is Phases 1–3 plus 6. Its goal-oriented, port-driven and hp phases can follow in v0.10.x.
- Phases that edit `crates/geode-cli/src/{main,spec,problem,report}.rs` merge one at a time.

**Interfaces between the epics:**

- Gradients are written against #838's order-generic solver, and #836 Phase 4 keeps the existing gradients working at p=2.
- #842 exposes its adjoint fields, which AMR's goal-oriented estimator reuses at no extra solve.
- Design parameters bind to named mesh groups, so they survive refinement. Optimization runs on a frozen mesh, with adaptation only between stages.
- Periodic pairs expose `paired_face()`, which refinement mirrors, plus `alias_node_motion()` and `dphase_dk()` for gradients. The Bloch adjoint is the solve at −k.
- #836 Phase 5a's same-mesh p=1-vs-p=2 difference becomes one component of the AMR estimator, not a second estimator.

**v0.8 hardening:** #824 / #828 / #831 / #834 / #845 shipped in v0.8.0. #850, the bound-mode hole guard for the PML/dielectric solvers, shipped in v0.9 (closed 2026-10-07).

## Design programs (opened 2026-10-24)

Three epics turn the differentiable stack toward results that end in measured hardware or a public dataset. They came out of a 2026-10-24 roadmap brainstorm of ten proposals, five in semiconductors/EDA and five in other EM domains. Each epic carries its phases, acceptance criteria, kill criteria and cost estimates. Every spend or public-release phase is operator-gated.

| Epic | Program | First phase(s) | Depends on |
|---|---|---|---|
| #1251 | **Open-PDK IC flow.** GEODE as a differentiable third backend of the IHP SG13G2 open EM flow: an adjoint-designed freeform mm-wave transformer/balun, checked against Palace, taped out on an MPW and probe-measured; plus `geode calibrate` (stack fitting on the adjoint Jacobian) and `geode driven --corners` | #1254: volumetric σ + `∂/∂σ` (delivers the σ item of #841 P2a). #1255: layout schema v2 for IC stacks | #841 P2a/P2b (calibration), #841 P6b (`geode optimize`), #836 P5a #1265 (p=2 column) |
| #1252 | **Wide-scan phased-array element.** Min-max over scan angle and frequency with a Floquet-port shape adjoint and 50–200 freeform parameters, measured in a waveguide simulator | #1256: Floquet-port S adjoint via the −k_t transpose solve | #837 P3a (merged) for P1; #837 P3b + P4b for P2 (adopts #837 P5) |
| #1253 | **GEODE-Bench.** A gradient-labelled unstructured-mesh EM dataset with Sobolev-trained surrogates and an optimization-transfer metric | #1257: dataset exporter and schema | #837 P4b for the Floquet task only |

**Gates:**
- #1251's tapeout waits on its Phase 2 kill gate: the IHP 400 pH inductor must match Palace (|ΔQ/Q| ≤ 5%).
- #1253's headline waits on its Phase 3 ablation: does gradient supervision improve optimization transfer?

**Parked from the brainstorm** (revisit when a partner appears or a kill test is run):
- motor rotor built and dyno-tested;
- die-to-die (UCIe-class) link co-design;
- gradient-designed substrate isolation;
- power-delivery impedance shaping;
- surface-participation-optimized superconducting resonators;
- GPU matrix-free adjoint at 10–50M DOF.

## v0.11.0 and later

- **Curved / isoparametric geometry** (#836 Phase 6, likely v0.11). It is needed for O(h⁴) eigenvalue rates on curved boundaries.
- **Overflow** from the v0.10 epics.
- **Unplanned, no issue yet** (file one when a user or a design program needs it):
  - **wave-port field export** and **runtime GPU backend dispatch**, both from the old #702/#756 deferred lists;
  - **hybrid ports in the adaptive sweep**. The port mode is non-affine in ω, so this is a research question first, and it sits behind #841 P4.
- **Closed since the last update:** plasma-like port fills #781 (not planned; waits for a real use case); the follow-ups #822, #823, #786 and #793 (done).

## Research tracks (not scheduled into a release)

- **Scale:** the matrix-free solver (#547) and distributed / multi-GPU (#546, #640, #641). July's measurements argue against leading with scale; see #569's non-goals.
- **Benchmarks and papers:**
  - transmon benchmark vs Palace (#476, #639);
  - freeform conformal-radiator inverse design (#647, #651, #652);
  - the GEODE-vs-Palace comparison manuscript (#569).
  - Operator-gated submissions are #593 and #659. The v1 Zenodo release of all three papers is tracked on #1231; close #593 and #659 once it is published.
- **Palace parity tracker:** #475, with the problem types at parity and the infrastructure gaps mapped onto #835 / #836 / #837.

## Non-goals

- Racing Palace on eigenmode wall-clock, memory or DOF scale. The matrix-free interior eigensolve is a measured dead end (#531, #607).
- Hanging-node, anisotropic and coarsening AMR (v0.10 scope). Mortar coupling for non-matching periodic meshes.
- Differentiating through re-meshing. Optimization morphs the mesh within a stage and re-meshes between stages.
