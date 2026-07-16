# Lead the performance story with the driven solve — retire the matrix-free interior eigensolve as the scale story

**Date:** 2026-07-16
**Status:** design decision (codifies **Direction 2** of the strategic memo)
**Parent:** Epic #569
**Scope:** positioning / design note. No new solves, no GPU access required. Every
measured claim below cites a committed source path; every GPU-performance claim is
explicitly labelled **aspirational / hardware-gated** — GEODE has **no** measured GPU
*win* to report, only a measured GPU *baseline that currently loses* (see §3).

> **One-line thesis.** GEODE's "tensor-native at scale on AI hardware" story should be
> led by the frequency-domain **driven** (linear-solve) problem `A(ω) x = b` — the
> S-parameter / EPR workhorse device design actually needs — and should **stop** being
> staked on the matrix-free **interior eigensolve**, which is the hardest variant of a
> problem the incumbent (Palace) solves by simply factorizing. This is the positioning
> artifact for Direction 2 of
> [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md); read that
> memo for the full four-thread framing, and
> [`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) for the honest
> measured head-to-head this rests on.

---

## 1. Why driven, not eigen

The two problems GEODE solves are structurally different, and that difference is the
whole argument.

**The eigenproblem is an interior-eigenvalue problem** `(K − k²M)x = 0` sought near a
deep physical shift σ ≈ 4.5 GHz. Reaching interior modes requires **shift-and-invert**,
i.e. an inner linear solve against the *indefinite* operator `A = K − σ²M` at every outer
Lanczos step. That inner solve is where GEODE's matrix-free scale path hits a wall:

- At the default inner tolerance (`1e-10`), a **single** inner AMS-preconditioned MINRES
  solve — outer Lanczos step 0 — never converges. It plateaus on a nearly-flat residual
  tail below ~`1e-5` and was still running at **14 300 inner iterations** (‖r‖ ≈ 2.57e-6)
  when killed. Dropping from `1e-5` (~1000 iters) to `2.6e-6` (~14 300 iters) took ~13 000
  more iterations — the tail has no steep asymptote.
- The plateau is **coarse-solve-invariant**: even an *exact* coarse solve stalls, so the
  limiter is the SPD-proxy preconditioner `K + |σ|M`, not the coarse solve (#562/#565).
- There is **no single inner tolerance that is both cheap and correct**: `1e-2` completes
  all 96 outer steps in 55.3 s but returns non-physical modes (the λ ≈ 0 gradient
  near-kernel hash at 0.42–0.64 GHz, not the σ=4.5 physical band); `1e-4` is the only
  candidate and it is both slow and unconfirmed.

Source: [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md).

This is not a tuning miss — it is intrinsic. Interior eigenvalues near a deep shift are
one of the hardest problems in computational EM, and **even Palace does not fight it
matrix-free**: Palace's interior-eigenvalue path is SLEPc Krylov–Schur + shift-invert with
a preferred *sparse direct* solver, and it applies AMS only to *definite* curl-curl, never
to the indefinite eigenproblem inner solve
([`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md), Thread 1).
GEODE's matrix-free AMS-MINRES shift-invert is therefore a **harder path than the incumbent
even attempts** ([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2c).

**The driven problem is a plain linear solve** `A(ω) x = b` at a prescribed real ω, with
```text
A(ω) = K + iωC(σ) − ω²M(ε),
```
all three matrices ω-independent and complex-**symmetric** so `A(ω)ᵀ = A(ω)`
([`crates/geode-core/src/driven/solve.rs`](../../crates/geode-core/src/driven/solve.rs)).
There is **no shift-invert, no interior-eigenvalue pathology, no SPD-proxy tail** — the
σ=4.5 wall simply does not exist here. And it is the *right* problem for device design:
S-parameters, port impedances, and EPR are all read off the driven solution, not off a
bare eigenmode. The eigenmode is a correctness credential; the driven solve is the
workhorse.

Externally, the driven problem is also where matrix-free + GPU is documented to win big:
matrix-free iterative crushes direct factorization by **~216×** overall on GPU for a
high-order (p=8) H(div) saddle-point system (~1.7M DOF), with the advantage *growing* with
polynomial order (arXiv:2304.12387, cited in the memo, Thread 2). **That 216× is an
external result, not a GEODE measurement** — see §3 for what GEODE has actually measured on
GPU (which is the opposite, at the small sizes tested).

---

## 2. What GEODE already has (CPU-proven)

### 2a. A working driven matrix-free COCG path

The driven solver exposes three back-solve modes behind one seam
([`crates/geode-core/src/driven/mod.rs`](../../crates/geode-core/src/driven/mod.rs),
[`crates/geode-core/src/driven/matrix_free.rs`](../../crates/geode-core/src/driven/matrix_free.rs)):

- `SolverMode::Direct` — faer sparse LU (the accuracy reference).
- `SolverMode::Iterative` — COCG + Jacobi against the assembled sparse `A(ω)`.
- `SolverMode::IterativeMatrixFree` — GPU-resident matrix-free COCG (`BurnCocg` over a
  `ComplexMatrixFreeOperator`): the volume terms `K − ω²M + iωC` apply element-locally on
  Burn tensors with **O(1)** global storage (never assembling `A`), plus a small on-device
  COO correction for the lumped-port / Leontovich surface terms. The per-iteration
  host-sync budget is O(1) scalars; vectors cross the host boundary exactly twice per
  back-solve.

Crucially, the matrix-free operator is the **same source path** on CPU (ndarray backend)
and GPU (CUDA backend) — only the Burn backend differs. That is the tensor-native lever the
whole thesis rests on: one differentiable, backend-portable operator, not two hand-written
kernels.

### 2b. Measured CPU driven scaling — the iterative path already beats direct

The committed driven-scaling benchmark
([`benchmarks/gpu_driven_scaling/results.toml`](../../benchmarks/gpu_driven_scaling/results.toml),
issue #501) sweeps one physical fixture (σ-lossy parallel-plate cube, single lumped port)
across mesh refinements `n ∈ {6, 9, 12, 15}` (1 854 → 25 695 edges), same host, at ω = 0.1,
solve-only median of 3 reps:

| n | edges | Direct LU (s) | assembled-CSR COCG (s) | matrix-free COCG, CPU f64 (s) |
|--:|------:|--------------:|-----------------------:|------------------------------:|
| 6  | 1 854  | 0.024 | 0.032 |  1.65 |
| 9  | 5 859  | 0.203 | 0.198 |  8.88 |
| 12 | 13 428 | 1.540 | 0.709 | 30.23 |
| 15 | 25 695 | 6.036 | 1.865 | 82.06 |

Two CPU-proven facts fall out (all from the same committed file):

- **The assembled iterative COCG scales better than direct.** It matches direct LU at
  n=9 and is **3.2× faster** than LU at n=15 (1.865 s vs 6.036 s) — direct's factorization
  cost grows super-linearly while the iterative solve tracks nnz. This is the same
  fill-growth wall that makes direct lose at 1.16M DOF
  ([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2b: 565.5 s / 92.2 GB
  vs Palace 423 s / ~33 GB).
- **Both f64 iterative paths are accurate**: full-field rel-L2 vs the direct-f64 reference
  sits at 6e-9…2e-8 — a documented equivalence class, not a degradation.

The matrix-free CPU column is *slow in absolute terms* at these tiny sizes (per-element
tensor apply on ndarray has high constant overhead), and this note does **not** claim the
matrix-free path is CPU-competitive yet. The load-bearing CPU claim is narrower and solid:
**the driven problem admits an iterative solve that already out-scales direct factorization
on CPU** — exactly the regime where GPU matrix-free is documented to extend the win, and
exactly the regime the eigensolve cannot enter because of §1's shift-invert wall.

### 2c. Differentiable assembly + a validated adjoint that composes with the driven solve

GEODE's one structural advantage is that its FEM operators are built on Burn tensors, so
assembly is differentiable and the tape reaches `K`, `M`, `b` and their dependence on
material ε. The faer factorization breaks that tape at the solve — so the missing piece was
an explicit **adjoint-through-solve** layer, and it has **landed**:

- [`crates/geode-core/src/adjoint.rs`](../../crates/geode-core/src/adjoint.rs) (Epic #569,
  issue #570, merged PR #573) implements the discrete adjoint `Aᵀλ = ∂g/∂x`, then
  `dg/dε_k = −λᵀ(∂A/∂ε_k)x` — the full material-ε gradient from **one forward + one adjoint
  solve** (the adjoint reuses the forward LU factors via `solve_transpose_in_place`, never
  a refactorization). Its committed test asserts the adjoint gradient matches a full central
  finite-difference of the whole pipeline to relative error ≤ 1e-4 (~3e-8 observed). The
  geometry / shape sensitivity extension landed next
  ([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §3; issue #571,
  merged PR #575).

The adjoint identity is written for a **generic** linear system `A x = b` — it is agnostic
to whether `A` is the SPD electrostatic operator (the validated proof-of-concept) or the
complex-symmetric driven pencil `A(ω)`. Because `A(ω)ᵀ = A(ω)`
([`driven/solve.rs`](../../crates/geode-core/src/driven/solve.rs)), the driven adjoint is
the *natural* next composition: one adjoint back-solve against the same operator yields
`∂(S-parameter or EPR)/∂ε`. This is why "lead with the driven solve" and "lead with
differentiability" are the **same** recommendation — the driven linear solve is precisely
the seam the adjoint layer plugs into, and it is a capability Palace / HFSS / COMSOL
structurally cannot provide.

---

## 3. Honest gaps and roadmap (GPU is aspirational / gated)

**GEODE has measured GPU numbers for the driven solve, and today they *lose*.** The same
committed benchmark ([`benchmarks/gpu_driven_scaling/results.toml`](../../benchmarks/gpu_driven_scaling/results.toml))
ran config 4 — the identical matrix-free COCG source path on a CUDA backend (NVIDIA L40S,
f32) — against the CPU configs on the *same* rented host:

- **GPU-f32 matrix-free loses to every CPU configuration at every measured size.** At n=15
  the assembled CSR COCG is ~44× faster (1.86 s vs 81.8 s); even direct LU is ~13× faster.
  The GPU per-iteration cost is kernel-launch dominated (~28 ms/iter at n=15) — 25.7k edges
  is far too small to fill an L40S.
- The **only** crossover in range is GPU-f32 vs the *same* matrix-free algorithm on CPU
  (ndarray f64): the ratio falls from 2.7× slower (n=6) to parity at n=15, i.e. the GPU
  catches the CPU matrix-free path at ~26k edges and would pass it beyond — **but that
  crossover is moot in f32**, because n=15 already sits at the f32 convergence ceiling
  (true recomputed residual floors at 5.4e-3, sweep DNF, nondeterministic stagnation).

The honest reading is therefore:

- **CPU-proven (real, committed):** the driven problem admits an iterative solve that
  out-scales direct on CPU (§2b); differentiable assembly + a validated ε-adjoint compose
  with it (§2c).
- **GPU-aspirational (gated, NOT yet demonstrated for GEODE):** the ~216× matrix-free-vs-
  direct GPU win is an **external** result (arXiv:2304.12387), realized at high polynomial
  order and problem sizes that actually fill a GPU. GEODE's own GPU crossover requires (a)
  problems large enough to amortize kernel-launch overhead and (b) **f64 or
  mixed-precision / iterative-refinement** on device, since f32 COCG stalls at a ~1e-3
  recurrence floor (the L40S has 1/64-rate f64, so naïve f64-on-L40S trades the convergence
  ceiling for raw-rate loss). The at-scale GPU execution is hardware-gated: #519/#520/#534
  are blocked on GPU quota (see the AWS-quota notes). **No GEODE GPU driven *win* exists to
  cite, and none is claimed here.**
- **Alignment candidate, not yet adopted:** libCEED ships matrix-free, GPU-portable
  operator application with a **first-class Rust interface** and low-order-refined (LOR)
  preconditioning for H(curl)/H(div) on GPU (memo Thread 2). Aligning with or binding to
  libCEED is a candidate to avoid hand-rolling GPU kernels — a roadmap direction, not a
  committed decision.

---

## 4. Recommendation and the "stop doing" list

**Recommendation.** Lead GEODE's performance / GPU / tensor-hardware story with the
frequency-domain **driven** solve:

1. Frame the scale thesis around `A(ω) x = b` — a linear solve with no interior-eigenvalue
   pathology, whose iterative form already out-scales direct on CPU (§2b) and is the
   documented GPU matrix-free winner externally (§1).
2. Pitch it as the **differentiable design-sensitivity workhorse**: the driven solve is the
   seam the validated adjoint layer plugs into (§2c), giving `∂(S-param, EPR)/∂(ε, geometry)`
   — the capability Palace structurally lacks.
3. State the GPU crossover as **roadmap, hardware-gated**, and be explicit that the only
   GEODE GPU numbers to date *lose* at the sizes measured (§3). Consider libCEED (Rust
   interface) as the alignment path rather than hand-rolling kernels.

**Stop doing.**

- **Stop** investing in the matrix-free **interior eigensolve** as the *scale story*. It is
  the hardest variant of the problem, its SPD-proxy preconditioner has a proven-dead flat
  tail at the physical deep shift (§1), and even Palace avoids it by factorizing. **Keep the
  eigensolve only as a correctness path** — when an eigenvalue is genuinely needed, direct-
  factorize the shift-invert like Palace (or, if a matrix-free eigensolve is ever revisited,
  adopt the actual SOTA — Jacobi–Davidson + Helmholtz projection, memo Direction 4 / #531 —
  rather than the SPD-proxy MINRES that is confirmed dead).
- **Stop** staking the "tensor-native at scale" thesis on beating Palace's interior-
  eigensolve wall-clock. That race is lost on the merits
  ([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) §2c/§2d) and is
  table-stakes, not a differentiator.
- **Don't fabricate a GPU driven win.** Until #519/#520/#534 are unblocked and a large,
  f64/mixed-precision GPU run is measured, the GPU claim stays labelled aspirational and the
  216× stays attributed to arXiv:2304.12387, not to GEODE.

---

## Cross-links

- [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) — the strategic
  memo (Direction 2; the 216× GPU result; libCEED Rust interface; the four-thread framing).
- [`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) — the honest measured
  head-to-head (correctness parity 0.03%; no clearly-preferred GEODE raw-perf corner;
  the ε-adjoint complement).
- [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)
  — the σ=4.5 deep-shift wall (coarse-solve-invariant plateau; SPD-proxy limiter; #562/#565).
- [`benchmarks/gpu_driven_scaling/results.toml`](../../benchmarks/gpu_driven_scaling/results.toml)
  — the measured driven CPU-vs-GPU scaling (iterative out-scales direct on CPU; GPU-f32
  currently loses; #501).
- [`crates/geode-core/src/driven/matrix_free.rs`](../../crates/geode-core/src/driven/matrix_free.rs),
  [`crates/geode-core/src/driven/solve.rs`](../../crates/geode-core/src/driven/solve.rs) —
  the driven matrix-free COCG path and the complex-symmetric pencil.
- [`crates/geode-core/src/adjoint.rs`](../../crates/geode-core/src/adjoint.rs) — the
  validated discrete-adjoint layer (#570 / PR #573; shape extension #571 / PR #575) that
  composes with the driven solve.
