# GEODE-FEM roadmap

_Last updated: 2026-10-03._ This is the single source for where the project is going. Each item links to a GitHub epic, and the epic holds the phase plan, its goldens and its acceptance criteria. When an epic changes scope, update this file in the same PR.

## Direction

GEODE-FEM is a **tensor-native, differentiable-by-construction FEM electromagnetics solver**. It complements [AWS Palace](https://github.com/awslabs/palace) rather than racing it on raw scale. The full argument is in `docs/research/2026-07-16-strategic-direction.md` and Epic #569. Two tracks carry that direction:

1. **The `geode` CLI for EDA flows** (v0.4 → v0.8). Spec in, Touchstone/SPICE/JSON out, with the physics real PCB, package and RF designs need.
2. **Design sensitivities** (the differentiator). Adjoint gradients of every reported observable, validated against finite differences. v0.9 brings this track to the CLI.

Rules that every release follows:

- **Never silently wrong.** An unsupported combination fails loudly (`invalid_spec` / `solve_failed`). The error says what to change.
- **Maximally useful for design work** (operator decision, #804). Don't block reasonable meshes. Warn, estimate accuracy per result, and say how far to refine (refine-to-h). Fail loudly only when no robust path exists.
- **Validated claims only.** Every feature ships with an analytic, literature or cross-tool golden. Its accuracy bar must be met honestly at a stated mesh; nothing is loosened to pass.
- **Additive schema v1.** Spec and report changes are additive, the schemas are regenerated, and the drift test passes.

## Released and pending

| Version | Theme | Status |
|---|---|---|
| v0.4–v0.7 | CLI Phases 1–3 (Epics #673/#680/#702): `check` / `driven` / `eigen` / `extract`, Touchstone, `geode mesh`, C/L + SPICE, lossy eigen, sensitivities, AMS + adaptive sweep, schemas, cookbook, binaries | Released |
| **v0.8.0** | **Physics breadth for EDA (Epic #756)**, plus **hybrid wave ports (Epic #778)** | Merged on `main`; not yet tagged |

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
  - TE-port TM-cutoff guard (#808); its mesh-aware margin (#824, PR #827) is still in review.
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

## v0.9.0: differentiable EDA on a higher-order, adaptive foundation

| Epic | Theme | First phase |
|---|---|---|
| #836 | **Higher-order accuracy.** p=2 Nédélec through driven, eigen, lumped/wave/hybrid ports, sensitivities and the CLI `element_order`, then curved geometry | #838: order-generic element space + driven solver |
| #841 | **Differentiable EDA.** N-port S, material, roughness, dispersion, port-mode (β, ε_eff, Z₀) and lossy-Q gradients, CLI shape parameters, and `geode optimize` (L-BFGS-B / MMA) | #842: N-port S gradients through wave/filled/mixed/walled specs |
| #835 | **Adaptive meshing.** H(curl) a-posteriori estimator, conforming bisection, the solve→estimate→mark→refine loop, goal-oriented DWR, port-driven refinement, CLI `adaptive_mesh`, and hp | #840: explicit-residual estimator, validated on effectivity |
| #837 | **Periodic / Floquet.** Zero-phase periodic BCs, Bloch band structure, Floquet ports (TE + TM, diffraction orders), CLI `geode mesh --periodic`, and infinite-array scan impedance | #839: zero-phase periodic BCs |

**Sequencing.** #838 is the **first solver-core merge** of v0.9, because every other epic builds on the order-generic element space. After it:

- #839 (periodic), #840 (estimator) and #842 (S gradients) can run in parallel.
- The AMR epic's minimum deliverable is Phases 1–3 plus 6. Its goal-oriented, port-driven and hp phases can follow in v0.9.x.
- Phases that edit `crates/geode-cli/src/{main,spec,problem,report}.rs` merge one at a time.

**Interfaces between the epics:**

- Gradients are written against #838's order-generic solver, and #836 Phase 4 keeps the existing gradients working at p=2.
- #842 exposes its adjoint fields, which AMR's goal-oriented estimator reuses at no extra solve.
- Design parameters bind to named mesh groups, so they survive refinement. Optimization runs on a frozen mesh, with adaptation only between stages.
- Periodic pairs expose `paired_face()`, which refinement mirrors, plus `alias_node_motion()` and `dphase_dk()` for gradients. The Bloch adjoint is the solve at −k.
- #836 Phase 5a's same-mesh p=1-vs-p=2 difference becomes one component of the AMR estimator, not a second estimator.

**v0.8 hardening carried into early v0.9:** #824 / #828 / #831 / #834 (the mesh-aware TM-cutoff margin, the Lanczos convergence floor, the stable √, and the complex Lanczos residual check).

## v0.10.0 and later

- **Curved / isoparametric geometry** (#836 Phase 6, likely v0.10). It is needed for O(h⁴) eigenvalue rates on curved boundaries.
- **Overflow** from v0.9 epics.
- **Wave-port field export** and **runtime GPU backend dispatch**. Both are on the old #702/#756 deferred lists and not yet planned.
- **Hybrid ports in the adaptive sweep.** The port mode is non-affine in ω, so this is a research question first.
- **Plasma-like (non-propagating) port fills** (#781). This waits for a real use case.
- **Smaller follow-ups:** #822, #823; and #786 (the three remaining slow CI targets), #793's allowlist.

## Research tracks (not scheduled into a release)

- **Scale:** the matrix-free solver (#547) and distributed / multi-GPU (#546, #640, #641). July's measurements argue against leading with scale; see #569's non-goals.
- **Benchmarks and papers:**
  - transmon benchmark vs Palace (#476, #639);
  - freeform conformal-radiator inverse design (#647, #651, #652);
  - the GEODE-vs-Palace comparison manuscript (#569).
  - Operator-gated submissions are #593 and #659.
- **Palace parity tracker:** #475, with the problem types at parity and the infrastructure gaps mapped onto #835 / #836 / #837.

## Non-goals

- Racing Palace on eigenmode wall-clock, memory or DOF scale. The matrix-free interior eigensolve is a measured dead end (#531, #607).
- Hanging-node, anisotropic and coarsening AMR (v0.9 scope). Mortar coupling for non-matching periodic meshes.
- Differentiating through re-meshing. Optimization morphs the mesh within a stage and re-meshes between stages.
