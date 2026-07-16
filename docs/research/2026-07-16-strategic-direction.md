# GEODE-FEM strategic direction — stop racing Palace, become the differentiable EM design engine

**Date:** 2026-07-16
**Author:** research synthesis (deep-research fan-out: 24 primary sources, 5 search angles, adversarially verified)
**Status:** proposal for discussion — not yet adopted

---

## TL;DR

Every measurement and every external source points the same way:

- **Stop** trying to beat Palace on eigenmode wall-clock / memory / scale. That race is lost on the merits and the axis is table-stakes, not a differentiator.
- **Reposition** GEODE around the one thing its substrate (Rust + Burn **reverse-mode autodiff**, GPU-portable, AI-hardware-colocated) can do that Palace/HFSS/COMSOL **structurally cannot**: provide **solver-derived gradients** of device figures-of-merit with respect to geometry and materials.
- **The differentiator is design *sensitivities*, not any one application.** Superconducting-qubit inverse design is the clearest *documented* unmet need (see below), but it is **one application among several** — RF/microwave, integrated photonics, and general shape/topology optimization all want the same thing: `∂(figure-of-merit)/∂(geometry, materials)`. Do not over-index on qubits; the reusable asset is the differentiable-sensitivity *capability*.

The eigenmode-vs-Palace benchmark stays as a **correctness credential** (already at parity, 0.03%) and the vehicle for **outreach to the Palace authors** — an honest "here is what an independent tensor-native re-implementation measured against your solver, and here is the one capability it adds." It is no longer the headline claim.

> **Honest technical caveat — "differentiable" today = differentiable *assembly*, not a differentiable *solve*.** GEODE's Burn tape reaches the assembled `K`, `M`, `b` and their dependence on **material** ε — but `driven/solve.rs` is explicit that the faer sparse factorization **breaks the tape** (same for the eigensolver). So naïve reverse-mode autodiff does **not** currently yield `∂(eigenfrequency or S-parameter)/∂(anything)`. Realizing that requires an **explicit adjoint / implicit-differentiation layer around the solve** (a custom backward for `Ax=b`: one adjoint solve; for eigenpairs: Hellmann–Feynman / adjoint-eigenpair formulas) — bounded, well-understood work that is *not yet built*. **Geometry** gradients need one more thing: node coordinates flowing through the assembly tape (the P1 path already gathers vertex coords into a tensor; the H(curl) EM path currently takes pre-computed `&[f64]` coefficient arrays), plus a differentiable **design-parameter → mesh** map (mesh-morphing or a density/level-set parameterization, since gmsh is not differentiable). None of this is a fundamental barrier — but the claim "differentiable geometry sensitivities" is a **roadmap item, not a current capability**, and must be pitched that way.

---

## What we measured (the wall that prompted this)

- **Direct solver loses at scale.** At 1.16M DOF: geode-direct 565 s / 92 GB vs Palace 423 s / ~33 GB — worse on *both* axes. The flop+fill crossover is below 1M. No factorization trick (custom AMD ordering, BLR) closes it.
- **The matrix-free interior eigensolve is stuck at a fundamental wall.** At the physical σ = 4.5 GHz deep interior shift the inner solve plateaus at ~1e-5 and never converges. We proved (issues #562/#565) the plateau is **coarse-solve-invariant** — even an *exact* coarse solve plateaus — so the limiter is the SPD-proxy preconditioner `K+|σ|M`, not the coarse solve. Interior eigenvalues near a deep shift are one of the hardest problems in computational EM.

The strategic question this raised: *are we fighting the right fight?*

---

## What the research found (4 threads)

### Thread 1 — Interior eigensolvers: we're on the hardest possible path, and even Palace doesn't fight it matrix-free

- **Palace's own interior-eigenvalue path is SLEPc Krylov-Schur + shift-and-invert with a preferred *sparse direct* solver**, falling back to AMS+GMRES. Crucially, **Palace applies AMS only to *definite*/semi-definite curl-curl** (time-domain, magnetostatics) — **not** to the indefinite eigenproblem inner solve. [awslabs.github.io/palace, SLEPc EPS manual]
  → *Implication:* our matrix-free **AMS-MINRES shift-invert for interior eigenmodes is a harder path than the incumbent even attempts.* Palace "wins" here by simply factorizing. We are hand-rolling the hardest variant of a solved-by-direct-methods problem.
- **The academic SOTA for the singular curl-curl eigenproblem is preconditioned Jacobi–Davidson (PHJD)** — the preconditioner is applied to the JD *correction equation*, with a **Helmholtz/divergence-free projection solved each iteration** to stop the iteration collapsing into the curl operator's near-kernel. Convergence factor is proven mesh-independent. This is *not* shift-invert-with-SPD-proxy. [arXiv:2603.29718 — survived 3/3 adversarial verification]
- Interior eigenvalues are intrinsically hard (peripheral converge first); shift-and-invert is "the most powerful technique" but needs a linear solve each step; harmonic Ritz is a lower-cost alternative. [SLEPc]

**Takeaway:** if we keep the eigenproblem at all, the right move is **Jacobi–Davidson + explicit Helmholtz/gradient-kernel projection** (the deflation direction #531 already identified), *or* just use a direct factorization for the shift-invert like Palace. The matrix-free AMS-MINRES eigensolve is a dead end — confirmed externally.

### Thread 2 — The *driven* (linear-solve) problem is the real GPU/matrix-free win

- **On GPU, matrix-free iterative crushes direct factorization for high-order EM-relevant spaces:** ~**216×** overall for a p=8 H(div) saddle-point system (~1.7M DOF); CG setup ~30,000× faster than Cholesky, solve ~70× faster than triangular solve. [arXiv:2304.12387 — survived]
- Assembled sparse FEM is memory-bandwidth-bound (~2% ALU utilization); **matrix-free is O(1) storage, high utilization, and the advantage *grows* with polynomial order** (`O(p^d)` storage vs `O(1)`). [libCEED / JOSS]
- Low-order-refined (LOR) preconditioning extends matrix-free to **H(curl)/H(div) on GPU** with mesh/degree-independent spectral equivalence. [arXiv:2101.03687]
- libCEED ships a **first-class Rust interface** — GEODE can bind to or align with an established GPU-portable matrix-free stack rather than reinventing it.
- For *multi-source* driven problems, augmented partial factorization computes the whole scattering matrix in one factorization (1000–30M× for ~10M-variable nanophotonics). [arXiv:2210.12253]

**Takeaway:** the frequency-domain **driven** solve — a linear solve, no shift-invert, no interior-eigenvalue pathology — is where matrix-free + GPU genuinely wins, and it is exactly the S-parameter / EPR workhorse device design needs.

### Thread 3 — Differentiable EM: a real, growing field — but FEM + RF/superconducting is *uncontested*

- **Full-autodiff FEM works and is competitive:** JAX-FEM gives ~10× over a commercial FEM code at 7.7M DOF and does gradient-based 3D topology optimization — **but for solid mechanics, not Maxwell.** The differentiable-full-autodiff-FEM-for-EM slot is empty. [JAX-FEM]
- The differentiable-EM niche that *is* filled is **photonics via FDTD or integral equations**, not FEM: FDTDx (JAX differentiable FDTD, billions of cells, photonic nanostructures), TorchGDM (PyTorch, Green's dyadic). Both explicitly **scoped to nanophotonics, not RF/microwave or superconducting-qubit / frequency-domain FEM.** [FDTDx JOSS 2026; TorchGDM]
- **Caution (survived 3/3):** the *favored production approach* is to make existing proven solvers differentiable via **adjoint**, not to build a from-scratch full-autodiff solver. → GEODE's full-autodiff-through-Burn should be framed as *enabling* adjoint-quality gradients cheaply, not as autodiff-for-its-own-sake.
- **Concrete autodiff gap:** SQcircuit (PyTorch) differentiates qubit design at the *lumped-circuit Hamiltonian* level and must hand-roll a custom node because **standard autodiff libraries don't support sparse eigenvalue/eigenvector gradients** — a gap a tensor-native differentiable FEM eigensolver could fill at the *field* level. [SQcircuit]
- **NVIDIA PhysicsNeMo** (the flagship AI-hardware physics-ML platform) covers CFD, structural, chemistry — **EM/Maxwell is absent.** An open slot in the AI-hardware physics ecosystem.

### Thread 4 — The unmet need is the *inverse problem* in superconducting-qubit design

- **The core unsolved problem is the inverse map: target Hamiltonian → physical layout — done today by "guess and check," with no gradient-based optimization.** [arXiv:2508.18027]
- **Because HFSS is not differentiable, the state-of-the-art optimizer (QDesignOptimizer) injects gradients from a *separate analytic model*** rather than the EM solver — an explicit workaround a natively differentiable EM solver would replace. The EM solve remains the cost bottleneck the optimizer can't attack. [arXiv:2408.12704]
- The design loop is expensive: ~280 min for a 10-iteration / 8-pass optimization on a workstation; larger designs take hours-to-days. Groups are "mostly limited to in-lab workstations"; validated designs are hoarded as "secret sauce." Access is a real barrier. [arXiv:2508.18027]
- **ML-based design inversion currently fails from *data sparsity* (overfitting)** — an unmet need for scalable simulation-data generation, i.e. a fast GPU-native solver to produce training fields. [arXiv:2508.18027, 2511.01220]
- **Eigenmode correctness is table-stakes:** Palace matches commercial to 0.02–0.11%; the *real* open gap is sim-to-experiment fidelity (13.25% anharmonicity, 24.53% coupling RMS error) and **differentiability/optimization, which Palace explicitly does not address.** [arXiv:2508.18027, SQDMetal]

---

## The convergent conclusion

Independently, all four threads say the same thing:

1. Eigenmode **accuracy** is solved (parity). Eigenmode **scale/speed** is Palace's (distributed CPU, 24.5M DOF at 99% efficiency). The matrix-free **interior eigensolve** is the hardest path and even Palace factorizes instead. → **These are not where GEODE can win or differentiate.**
2. The **documented, unowned unmet need** in the transmon/qubit world is the **inverse problem** — and it is unsolved *specifically because the EM solvers are not differentiable.* The current workaround literally bolts gradients on from a separate analytic model.
3. GEODE's one true structural advantage — **Burn reverse-mode autodiff through the FEM solve** — is *exactly* the missing capability.
4. The **differentiable-FEM-for-RF/superconducting-frequency-domain** slot is **empty** — photonics is taken (FDTD/integral), EM is absent from PhysicsNeMo, and the driven/GPU/matrix-free path (where we *can* be fast) is the design workhorse.

**GEODE should stop being "a matrix-free FEM solver that wants to be as fast as Palace" and become "a tensor-native, differentiable-by-construction FEM EM solver that complements Palace by providing design *sensitivities* Palace cannot" — with superconducting-qubit / RF / photonic inverse design as applications, not the identity.**

---

## Recommended directions to prototype (evidence-backed)

### ★ Direction 1 (flagship, load-bearing) — The adjoint-through-solve layer + one validated gradient
Build the missing piece that makes every downstream story real: an **explicit adjoint / implicit-differentiation layer around the solve** so a scalar observable's gradient w.r.t. inputs falls out despite the faer tape-break. Prove it on the **driven** (linear) solve first, where the assembly tape already reaches ε: objective `g(x)`, adjoint solve `Aᵀλ = ∂g/∂x`, then `∂g/∂ε = −λᵀ(∂A/∂ε)x` with `(∂A/∂ε)x` from the autodiff-preserving assembly — **validated against a full finite-difference of the pipeline.** This single experiment is the honest test of "is differentiability actually our edge," and it is the prerequisite for *any* application (qubit, RF, photonic). **Material ε first** (tape already reaches it); **geometry/shape second** (needs node-coords-in-tape for H(curl) + a design-param→mesh map — harder, its own issue). Applications are downstream: `∂(f_qubit, anharmonicity, EPR)/∂θ` for qubits (documented unmet need: guess-and-check today, HFSS not differentiable, QDesignOptimizer bolts on external gradients), `∂(S-params)/∂θ` for RF, `∂(mode overlap)/∂θ` for photonics. **Palace structurally cannot do this** — but neither can we *yet*; this issue is what makes the claim true.

### ★ Direction 2 — Pivot the performance story to the driven / frequency-domain GPU solve; retire the matrix-free interior-eigensolve fight
Matrix-free iterative wins **216×** over direct on GPU for high-order EM saddle-point systems; the driven solve is the S-parameter/EPR workhorse and has no interior-eigenvalue pathology. Lead the "tensor-native at scale on AI hardware" thesis with the *driven* problem. For the eigenproblem, either adopt **PHJD (Jacobi–Davidson + Helmholtz projection)** or just factorize like Palace — but stop staking the thesis on matrix-free interior eigenmodes. Consider aligning with **libCEED (Rust interface)** rather than hand-rolling the matrix-free GPU kernels.

### Direction 3 — Surrogate-training data factory
ML design-inversion for qubits currently **fails from data sparsity**. A fast, GPU-native, *differentiable* solver both generates high-volume training fields and provides gradients for physics-informed loss — a concrete, quantified need, and a bridge to the AI-hardware-colocation thesis (train the surrogate on the same box that ran the solves).

### Direction 4 (numerical enabler, lower priority) — If we keep an eigensolver, adopt the actual SOTA
Replace shift-invert-MINRES + SPD-proxy with **Jacobi–Davidson applied to the correction equation + explicit Helmholtz/gradient-near-kernel projection** (mesh-independent convergence, purpose-built for the curl-kernel collapse). This is #531's "deflation" lever, now confirmed as the literature's answer. Only worth doing if Direction 1 needs field-level eigenpair gradients that a direct factorization can't cheaply supply.

---

## What to stop doing

- **Stop** chasing Palace on eigenmode wall-clock, memory, or DOF-scale — lost on merits, and it's table-stakes not a differentiator.
- **Stop** investing in the matrix-free **interior eigensolve** as the scale story — proven-dead preconditioner path, hardest variant, and even Palace avoids it.
- **Stop** framing the pitch as "faster FEM." Frame it as **"a tensor-native, differentiable-by-construction FEM EM solver — an independent cross-check of Palace that adds design sensitivities Palace lacks."**
- **Don't over-claim differentiability.** Today = differentiable assembly (w.r.t. ε), tape broken at the solve. The adjoint-through-solve layer (Direction 1) is what makes gradients real; pitch it as roadmap until that lands.
- **Don't over-index on any one application** (qubits included). The reusable asset is the sensitivity *capability*; qubit/RF/photonic are interchangeable demonstrations.

---

## Sources (24 primary, adversarially verified; selected)

- Palace solver docs — awslabs.github.io/palace/stable/config/solver/ (Krylov-Schur + shift-invert; AMS only for definite curl-curl)
- SLEPc EPS manual — slepc.upv.es (interior eigenvalue methods; shift-invert; harmonic Ritz; JD)
- Preconditioned Helmholtz–Jacobi–Davidson for Maxwell — arXiv:2603.29718 (SOTA curl-curl eigensolver; Helmholtz projection each iter)
- GPU matrix-free vs direct, high-order H(div) — arXiv:2304.12387 (216× speedup)
- LOR preconditioning for H(curl)/H(div) on GPU — arXiv:2101.03687
- libCEED (Rust interface; matrix-free portability) — theoj.org JOSS 02945
- Augmented partial factorization, multi-source driven — arXiv:2210.12253
- JAX-FEM (full-autodiff FEM, 10× at 7.7M DOF, solid mechanics) — nature.com/s43588-022-00370-6
- Adjoint vs full-autodiff in differentiable photonics — arXiv:2309.16731 (favored = adjoint-wrap existing solvers)
- FDTDx (JAX differentiable FDTD, photonics) — github.com/ymahlau/fdtdx; arXiv:2407.10273
- TorchGDM (differentiable Green's-dyadic, photonics) — arXiv:2505.09545
- SQcircuit (PyTorch autodiff qubit design; sparse-eigenvalue-gradient gap) — arXiv:2312.13483
- Superconducting-qubit inverse-design unmet need — arXiv:2508.18027
- QDesignOptimizer (external-gradient workaround for non-differentiable HFSS) — arXiv:2408.12704
- NVIDIA PhysicsNeMo (EM/Maxwell absent) — developer.nvidia.com/blog/physics-ml-platform-physicsnemo
- SQDMetal / Palace parity (eigenmode accuracy 0.02–0.11%) — arXiv:2607.02289

*Full source list and the raw extracted-claim set are archived alongside this memo (see `2026-07-16-strategic-direction.raw-findings.md`).*
