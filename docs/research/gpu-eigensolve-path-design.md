# GPU eigensolve path — Jacobi–Davidson + Helmholtz projection vs cuDSS-FFI factorization

**Date:** 2026-07-17
**Status:** design decision (issue #503, [#302 Phase 4]). No code, no dependencies, no
`Cargo.toml` changes — this note decides *which* GPU eigensolve to build, and *whether to
build one at all right now*.
**Parent context:** Epic #476 (transmon benchmark vs Palace), Epic #569 (differentiable EM).
**Scope:** desk design. Every measured claim cites a committed source path; every GPU
performance figure is labelled **external** or **aspirational / hardware-gated** — GEODE has
**no** measured GPU eigensolve win to report.

---

## TL;DR

The question this issue was filed to answer — "Path A (portable preconditioned iterative
eigensolver) vs Path B (cuDSS-FFI direct factorization) for a GPU eigensolve" — has been
**materially reframed** by three findings that landed *after* the issue was filed:

1. The original Path A concretely proposed ("LOBPCG / inexact-shift-invert Arnoldi over the
   matrix-free operator + AMS-preconditioned inner solves") is a **confirmed dead end** at
   the physical operating shift σ ≈ 4.5 GHz. The AMS-preconditioned MINRES inner solve
   plateaus on a coarse-solve-invariant flat residual tail below ~1e-5 and never converges
   ([`sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md),
   #562/#565).
2. The repo has explicitly **retired the matrix-free interior eigensolve as the scale story**
   ([`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) §4;
   [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md), Thread 1). The
   eigensolve is kept **only as a correctness / differentiability anchor**, and the strategy
   doc's own instruction when an eigenvalue is genuinely needed is: *direct-factorize the
   shift-invert like Palace.*
3. The eigensolve now has a real differentiability role independent of GPU wall-clock:
   Hellmann–Feynman eigenvalue sensitivities `∂λ/∂p`
   ([`crates/geode-core/src/eigen/sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs),
   #596/#600) are adjoint-free on a converged eigenpair — they need a *correct* eigenpair,
   not a *fast-on-GPU* one.

**Recommendation (summary): defer building any GPU eigensolve now.** The eigensolve is no
longer the scale story, so there is no urgency to GPU-accelerate it; the driven solve is the
GPU/scale track (#519/#520/#534). *If* a GPU eigensolve cell is nonetheless wanted for the
#476 benchmark before the driven track matures, **Path B (cuDSS-FFI direct factorization) is
the correct minimal expedient** — it is directionally aligned with the repo's own
"direct-factorize like Palace" recommendation, and its f64-native posture is the one genuine
technical edge that sidesteps the burn-cuda f32 ceiling. **Path A is only viable in a
*re-scoped* form** — Jacobi–Davidson + explicit Helmholtz projection (the academic SOTA), not
the falsified AMS-MINRES shift-invert — and only if/when eigensolve work resumes priority.

---

## 1. Why the original A-vs-B framing no longer holds

The issue (2026-07-14) framed two paths as a roughly even strategic coin-flip:

- **Path A** — a portable preconditioned iterative eigensolver: tree-cotree / div-free
  gauging, an AMS-class preconditioner on Burn tensors, then LOBPCG or inexact-shift-invert
  Arnoldi over the matrix-free operator. Pitched as the strategic line that also closes the
  standing Palace-parity preconditioner gap (Epic #475).
- **Path B** — cuDSS FFI: keep shift-invert Lanczos exactly as-is
  ([`crates/geode-core/src/eigen/lanczos.rs`](../../crates/geode-core/src/eigen/lanczos.rs)),
  swap the faer CPU sparse-LU factorization of `A = K − σM` for NVIDIA cuDSS on the `cuda`
  feature. Smallest diff, CUDA-only, f64-native.

Three things changed the calculus.

### 1a. Path A's original inner solve is a proven-dead path (#562/#565)

The AMS-class preconditioner from the original Path A *exists and is tested* — the three-space
Hiptmair–Xu `AmsLitePreconditioner`
([`crates/geode-core/src/eigen/ams.rs`](../../crates/geode-core/src/eigen/ams.rs), #550/#551)
merged after the issue was filed, so that line item is no longer a research gap. But the
characterization run that used it settled the question the wrong way for Path A: at σ = 4.5 GHz
the AMS-preconditioned MINRES inner solve is 2–3 orders of magnitude faster than abs-Jacobi to
~1e-4, then **goes nearly flat** — dropping from 1e-5 (~1000 iters) to 2.6e-6 (~14 300 iters)
took ~13 000 more iterations, and a single inner solve never left outer Lanczos step 0
([`sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)).
The plateau is **coarse-solve-invariant** — even an *exact* coarse solve stalls — so the
limiter is the SPD-proxy preconditioner `K + |σ|M` itself, not the coarse solve. There is **no
single inner tolerance that is both cheap and correct**: 1e-2 completes in ~56 s but returns
the λ ≈ 0 gradient-near-kernel hash (0.42–0.64 GHz, participation ≈ 0) instead of the physical
band; 1e-10 is correct in principle but unreachable. Building LOBPCG or inexact-shift-invert
Arnoldi *on top of* this inner solve inherits the flat tail. The original Path A is falsified.

### 1b. The eigensolve is no longer the scale story

The strategy docs, both merged to `main` after the issue was filed, are explicit. From
[`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) §4 ("Stop
doing"):

> **Stop** investing in the matrix-free interior eigensolve as the *scale story*. It is the
> hardest variant of the problem, its SPD-proxy preconditioner has a proven-dead flat tail at
> the physical deep shift, and even Palace avoids it by factorizing. **Keep the eigensolve
> only as a correctness path** — when an eigenvalue is genuinely needed, direct-factorize the
> shift-invert like Palace (or, if a matrix-free eigensolve is ever revisited, adopt the
> actual SOTA — Jacobi–Davidson + Helmholtz projection … — rather than the SPD-proxy MINRES
> that is confirmed dead).

[`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) Thread 1 adds the
external corroboration: **Palace's own interior-eigenvalue path is SLEPc Krylov–Schur +
shift-invert with a preferred *sparse direct* solver**, and Palace applies AMS only to
*definite* curl-curl, never to the indefinite eigenproblem inner solve. GEODE's matrix-free
AMS-MINRES shift-invert is a harder path than the incumbent even attempts. The academic SOTA
for the singular curl-curl eigenproblem is **preconditioned Jacobi–Davidson (PHJD)** with a
Helmholtz/divergence-free projection solved each iteration, whose convergence factor is proven
mesh-independent (arXiv:2603.29718) — structurally different from SPD-proxy shift-invert.

This reframes the two paths asymmetrically:

- **Path B is now directionally *aligned* with the repo's own recommendation.** "Direct-factorize
  the shift-invert like Palace" *is* Path B, just with the factorization on the GPU. It is no
  longer the "pragmatic but off-strategy" option — it is the on-strategy correctness expedient.
- **Path A survives only if re-scoped to PHJD.** The original AMS-MINRES form is a named dead
  end. Any resurrection must be Jacobi–Davidson + explicit Helmholtz/gradient-kernel projection,
  which is a fresh multi-PR numerical-methods build (flagged as Phase C in the sensitivity
  module's scope fence,
  [`sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs)), not an incremental
  layer on existing code.

### 1c. The eigensolve's job is now correctness + differentiability, not wall-clock

Hellmann–Feynman eigenvalue sensitivities
([`sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs), #596/#600) give
`∂λ/∂ε_k = −λ · xᵀ M_k x` (material) and `∂λ/∂θ` (geometry) **adjoint-free** from a converged,
M-normalized eigenpair — no adjoint solve, no new eigensolver. This makes the eigensolve a
*differentiability anchor*: what it must deliver is a **correct** eigenpair, and correctness is
already at parity with Palace (0.03%). A converged eigenpair from a CPU direct factorization
feeds the sensitivity contraction exactly as well as one from a GPU solve would. GPU wall-clock
on the eigensolve buys nothing on the axis the eigensolve now serves.

---

## 2. Decision matrix

Evaluated against the profile recorded on this issue (M3 Ultra, 133k interior DOF: factorization
33%, back-substitution 29%, reorthogonalization 36% of the 26.7 s eigensolve core) and the
strategic reframe above.

| Dimension | **Path A — PHJD + Helmholtz projection** (re-scoped; portable) | **Path B — cuDSS-FFI direct factorization** (CUDA-only) |
|---|---|---|
| **What it is** | Jacobi–Davidson correction equation + explicit Helmholtz/gradient-kernel projection each iter, over `MatrixFreeNedelecOperator` on Burn tensors. Matrix-free, no factorization. | Keep `SparseShiftInvertLanczos` exactly as-is; swap faer sparse-LU of `A = K − σM` for NVIDIA cuDSS on the `cuda` feature. |
| **Strategic alignment** | The *original* AMS-MINRES form is a **confirmed dead end** (#562/#565). Only the PHJD re-scope is defensible, and only if eigensolve work resumes priority. | **Aligned** with the repo's own "direct-factorize the shift-invert like Palace" recommendation ([`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) §4). |
| **Portability** | Pure Rust/Burn — cuda / wgpu / metal, CPU (ndarray) too. One backend-portable operator. | **CUDA-only.** No wgpu / metal / CPU-GPU story. First native FFI dep in the stack. |
| **f32/f64 posture** | Bound by burn-cuda's **f32-only** ceiling (`cuda = ["burn/cuda"]`, [`crates/geode-core/Cargo.toml`](../../crates/geode-core/Cargo.toml) L75); an interior eigensolve at f32 is accuracy-marginal (cf. f32 COCG stalls at ~1e-3 in the driven sweep). Needs #534. | **f64-native** — cuDSS is f64-native, sidestepping the burn-cuda f32 ceiling entirely. **This is Path B's single strongest technical argument** (see §3). |
| **Dependency footprint** | None new — uses existing Burn/faer stack. | New **native FFI** dependency (cuDSS). Breaks the "pure-Rust, no FFI" posture faer has held (`faer = { version = "0.24", … }`, workspace [`Cargo.toml`](../../Cargo.toml) L47). Operator-policy call required. |
| **Expected win (per this issue's profile)** | Attacks the dominant terms (fewer iterations via gauging; no factorization at all) — but only *after* a from-scratch PHJD build. | Factorization is only **33%** of the solve; even a free cuDSS factor caps the win at ~1.5×, and GPU triangular back-substitution is latency-bound at 133k DOF (the on-issue CPU-rayon result shows single-RHS back-solve resists parallelism). Realistic ~1.2–1.5× over a tuned CPU floor. Strengthens with mesh size (factorization grows ~O(n²) in 3D). |
| **Effort** | Research-grade, multi-PR (Phase C in [`sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs) scope fence). Needs div-free gauging as a hard prerequisite. | Smallest diff to a GPU eigensolve; benchmark-ready quickly, contingent on the FFI-policy decision + GPU hardware. |
| **Verdict** | **Deferred**, and only in re-scoped PHJD form if revived. Do **not** resurrect AMS-MINRES. | **Deferred**, but the **preferred expedient** *if* a GPU eigensolve cell is wanted at all. |

---

## 3. Honest constraints — f32/f64 and the FFI posture

- **burn-cuda is f32-only today.** The `cuda = ["burn/cuda"]` feature
  ([`crates/geode-core/Cargo.toml`](../../crates/geode-core/Cargo.toml) L75) inherits cubecl's
  posture, which disables f64. Any Path-A GPU iterative eigensolver therefore runs f32 on
  device, and an *interior* eigensolve at f32 is accuracy-marginal — the driven GPU sweep
  already shows f32 COCG stalling at a ~1e-3 recurrence floor
  ([`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) §3), and an
  interior eigenproblem near a deep shift is more sensitive, not less. Reaching f64-class
  accuracy on device is exactly what **#534** (blocked on burn/cubecl f64) exists to unblock.
  Until #534 lands, Path A on GPU is accuracy-gated.
- **cuDSS is f64-native — Path B's one genuine edge.** Because cuDSS factorizes in f64, Path B
  **sidesteps the burn-cuda f32 ceiling entirely**: it does not need #534, and it delivers a
  full-accuracy shift-invert on GPU immediately. This is the strongest argument *for* Path B if
  a GPU eigensolve is pursued at all — it is the only one of the two paths that is *not*
  blocked on the f64 gap.
- **Path B breaks the pure-Rust posture.** faer has kept the linear-algebra stack FFI-free
  (`faer = { version = "0.24", default-features = false, … }`, workspace
  [`Cargo.toml`](../../Cargo.toml) L47). cuDSS would be the repo's **first native FFI
  dependency** and is CUDA-only. Adopting it is an **operator-level policy call**; no Path-B
  work should start without an explicit decision on that policy.
- **The profile bounds Path B's payoff.** Per the profile on this issue, factorization is only
  33% of the 133k eigensolve core; back-substitution (29%) and reorthogonalization (36%) stay
  on CPU (or are latency-bound on GPU). A free cuDSS factor caps the win near 1.5×, shrinking
  to ~1.2–1.5× against a tuned CPU floor. The case **strengthens with mesh size** (3D
  factorization fill grows ~O(n²)); at ≥1M DOF the direct faer path OOMs at ~63.9 GB
  ([`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md)), so re-profile before
  holding this verdict if the #476 benchmark mesh grows.

---

## 4. Recommendation

**Defer building a GPU eigensolve now.** The eigensolve is retired as the scale story
([`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) §4) and now
serves as a correctness + differentiability anchor (§1c) that needs a *correct* eigenpair, not
a fast-on-GPU one — a role the existing CPU direct-factorization shift-invert Lanczos already
fills at parity (0.03%). The scale/GPU investment belongs on the **driven** track
(#519/#520/#534), which the strategy doc identifies as the real GPU story. Manufacturing GPU
urgency for a deprioritized axis would be the wrong call.

**If** a GPU eigensolve cell is nonetheless wanted for the #476 benchmark before the driven
track matures, the ordering is:

1. **Path B (cuDSS-FFI), scoped as a correctness/benchmark deliverable, not a scale-story
   investment.** It is directionally aligned with "direct-factorize like Palace," is f64-native
   (the only path not blocked on #534), and is the smallest diff. Gate it on (a) an explicit
   operator FFI-policy decision — cuDSS is the first native FFI dep and CUDA-only — and (b) the
   empirical re-profile below at the benchmark's actual mesh size, since the on-issue profile
   caps the win at ~1.2–1.5× over a tuned CPU floor at 133k DOF.
2. **Path A only in re-scoped PHJD form** — Jacobi–Davidson + explicit Helmholtz/gradient-kernel
   projection (arXiv:2603.29718), *not* the confirmed-dead AMS-MINRES shift-invert — and only
   if/when eigensolve work resumes priority *and* #534 unblocks f64 on device. This is a fresh
   multi-PR spike (Phase C in the
   [`sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs) scope fence), needing
   div-free gauging as a hard prerequisite. Do **not** resurrect the AMS-MINRES approach.

---

## 5. Staged milestones (relative to #519 / #520 / #534)

These milestones **feed off** the driven GPU track rather than competing with it. The empirical
step the original issue demanded ("quantify B's expected win first") is now *enabled* by the
GPU access #519 is bringing online (AWS quota moved 0 → 8 vCPUs, 2026-07-16).

| Milestone | Depends on | Feeds / relationship to #519/#520/#534 | Action |
|---|---|---|---|
| **M0 — this design decision** | nothing | Records "defer; Path B if forced; Path A only as PHJD." | **Done (this doc).** |
| **M1 — re-profile the CPU eigensolve at the #476 benchmark mesh size** | #519 (Palace-GPU rebuild on g6e L40S, now unblocked — the same hardware the profile needs) | Consumes the GPU host #519 stands up; establishes the *honest CPU baseline* any GPU eigensolve cell must beat (mirrors #520's "quote GPU speedups against the tuned CPU, not the untuned one"). | File as a follow-on **only if** a GPU eigensolve cell is actually wanted; otherwise skip. |
| **M2 — Path B cuDSS-FFI prototype** (conditional) | (a) operator FFI-policy decision; (b) M1 confirming factorization re-dominates at the benchmark size; (c) GPU access from #519 | Slots into #520 as an eigensolve counterpart to the driven crossover attempt. **Independent of #534** — cuDSS is f64-native. | File a fresh implementation issue; do not start without the FFI-policy call. |
| **M3 — Path A PHJD spike** (conditional, lower priority) | div-free gauging prerequisite; #534 (f64 on device); eigensolve work regaining priority | Would share the burn-cuda f64 unblock with #534 and the matrix-free operator with the driven track — but only after the driven GPU story (#520) is demonstrated. | Fresh spike issue; **must** be scoped as PHJD + Helmholtz projection, not AMS-MINRES. |

**Sequencing rationale.** #519 is the gating hardware event — it unblocks the M1 re-profile the
original issue asked for. #520 is where a GPU eigensolve cell would live *if* M2 proceeds. #534
gates only Path A (Path B's f64-native posture makes it #534-independent — the design's single
sharpest practical distinction). The honest default across all of them is that the driven track
(#519/#520/#534) is the priority, and the eigensolve GPU milestones (M2/M3) are conditional,
deferred, and filed only on demand.

---

## Cross-links

- [`driven-first-performance-strategy.md`](./driven-first-performance-strategy.md) — the "stop
  doing" reframe: retire the matrix-free interior eigensolve as the scale story; direct-factorize
  like Palace; the f32 GPU convergence floor.
- [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) — Thread 1 (Palace
  factorizes; AMS only on definite curl-curl; PHJD as the SOTA, arXiv:2603.29718).
- [`geode-vs-palace-comparison.md`](./geode-vs-palace-comparison.md) — measured head-to-head;
  direct-LU OOM at ~1M DOF.
- [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)
  — the σ=4.5 AMS-MINRES dead-end (coarse-solve-invariant flat tail; SPD-proxy limiter; #562/#565).
- [`crates/geode-core/src/eigen/ams.rs`](../../crates/geode-core/src/eigen/ams.rs) — the merged
  three-space AMS preconditioner (#550/#551), Path A's existing (but tail-limited) building block.
- [`crates/geode-core/src/eigen/lanczos.rs`](../../crates/geode-core/src/eigen/lanczos.rs) — the
  shift-invert Lanczos (faer CPU sparse-LU) whose factorization Path B would swap for cuDSS.
- [`crates/geode-core/src/eigen/sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs)
  — Hellmann–Feynman eigenvalue sensitivities (#596/#600); the differentiability-anchor rationale
  and the Phase-C PHJD scope fence.
- [`crates/geode-core/src/driven/matrix_free.rs`](../../crates/geode-core/src/driven/matrix_free.rs),
  [`crates/geode-core/src/driven/solve.rs`](../../crates/geode-core/src/driven/solve.rs) — the
  driven matrix-free GPU path the eigensolve milestones sequence against.
- [`crates/geode-core/Cargo.toml`](../../crates/geode-core/Cargo.toml) (`cuda = ["burn/cuda"]`,
  L75) and workspace [`Cargo.toml`](../../Cargo.toml) (`faer` pure-Rust, L47) — the f32-only GPU
  posture and the no-FFI posture Path B would break.
