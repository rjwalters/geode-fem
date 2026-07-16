# GEODE-FEM vs Palace — an honest, measured comparison

**Date:** 2026-07-16
**Purpose:** outreach to the Palace authors. This is a *complement* document, not a
competitor pitch. It states plainly where Palace wins, where the two solvers match,
and the one capability GEODE is being built to add. Every performance number below
cites the committed file it came from.

> **One-line thesis.** GEODE reproduces Palace's eigenmode spectrum to ~0.03% (table
> stakes), does **not** clearly beat Palace on raw solve performance in *any* corner we
> have measured, and is repositioning around the one thing its Rust + Burn (reverse-mode
> autodiff) substrate can do that Palace structurally cannot: **solver-derived design
> sensitivities**. See [`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md)
> for the full strategic framing — this document is the measured evidence behind it.

---

## 1. Correctness parity — table stakes, met

GEODE and Palace were run on the **identical** sha-pinned transmon mesh
(`transmon_smoke.msh`, 22 684 nodes / 133 314 tets / 133 108 interior DOF) at matched
first-order Nédélec / Palace Order 1, junction modeled as a reactive `LumpedPort`
(L = 14.860 nH ∥ C = 5.5 fF), PEC on metal + exterior, readout ports open. Palace commit
`fba6a5b`, 8 MPI ranks.

Source: [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml)

| mode | GEODE (GHz) | Palace (GHz) | rel-err |
|------|------------:|-------------:|--------:|
| resonator | 5.153 | 5.151335830348 | 0.032% |
| mode 2 | 15.465 | 15.46052107794 | 0.029% |
| junction LC (p ≈ 1) | 17.490 | 17.49010903536 | 0.001% |
| mode 4 | 18.693 | 18.69165792915 | 0.007% |
| mode 5 | 20.703 | 20.69755679425 | 0.026% |
| mode 6 | 26.088 | 26.08089940472 | 0.027% |

**Worst-case per-mode agreement: 0.032%** — the ≤1% same-mesh bar is met with ~25×
margin (`comparison.worst_case_rel_err_pct = 0.032`). Cross-solver mode identification
rests on **frequency** agreement, not participation: GEODE's stiffness-participation `p`
and Palace's field port-EPR are complementary, differently-normalized diagnostics that
rank modes differently (documented in the `[oracles.palace]` note).

Two honesty caveats, both already documented in the benchmark:
- GEODE's real Lanczos path admits a **spurious 3.4528 GHz** junction-surface mode with
  no Palace counterpart (a `K_port`-driven surface-operator LC resonance); it is filtered
  by frequency-matching against the committed Palace oracle. Removing it in-solver is a
  divergence-free / port-aware projection follow-on.
- The cross-backend conformance ledger records that the *independent* solver pair (LAPACK
  ZGGEV vs Burn's faer QZ) agrees to 8.2e-7 on the physical band, while the ~1e-13
  figures are shared-LAPACK-lineage assembly-agreement comparisons, not independent-solver
  agreement. Source: [`reference/CONFORMANCE.md`](../../reference/CONFORMANCE.md).

**Verdict:** eigenmode correctness is at parity. This is a *credential*, not a
differentiator — it is the price of entry, and Palace already has it against commercial
tools.

---

## 2. Honest performance — where each solver wins

All CPU numbers below are `/usr/bin/time` wall clock over the full pipeline (mesh load +
assembly + solve + output) and peak resident-set memory. GEODE uses a direct faer
sparse-LU shift-invert Lanczos; Palace uses distributed MPI Krylov + AMS (iterative).

### 2a. Small–medium (133 108 interior DOF), matched workload

Source: [`benchmarks/transmon_bench_cpu/results.toml`](../../benchmarks/transmon_bench_cpu/results.toml)
(`[matched.physical_target]`, m6i.4xlarge, 8 physical cores)

| solver / config | wall (s) | peak RSS |
|-----------------|---------:|---------:|
| GEODE, 1 thread | 28.7 | 3.1 GB |
| GEODE, 8 threads | 29.0 | 3.1 GB |
| Palace, 1 rank | 130.9 | 0.5 GB/rank |
| Palace, 8 ranks | 44.5 | 0.5 GB/rank |

Off-target (12 modes @ 20 GHz, `[matched.off_target]`): GEODE 36.8 s (1 t) / 26.6 s
(8 t) vs Palace 248.0 s (np1) / 64.7 s (np8) — GEODE's direct factorization is
**target-insensitive** while Palace's iterative Krylov+AMS degrades far from a
well-chosen shift.

Read honestly, this corner is **not** a clear GEODE win:
- **GEODE wins wall clock** here — 28.7 s on one core beats Palace's 44.5 s on eight
  ranks (~12× fewer core-seconds, `notes.per_core_efficiency`). But this is a
  per-core-efficiency / direct-vs-iterative reading, **not** a parallelization claim:
  GEODE's 8-thread run gives essentially **no speedup** over 1 thread at the physical
  target (28.7 → 29.0 s; issue [#518](https://github.com/rjwalters/geode-fem/issues/518)).
- **Palace wins memory** decisively — 0.5 GB/rank vs GEODE's 3.1 GB (~6×), and
- the wall-clock advantage is **scale-bounded** and inverts before 1M DOF (§2b).

### 2b. Large scale (1 157 564 interior DOF) — Palace wins on both axes

The identical fixture, uniformly refined (~1.16M interior DOF, pencil nnz ≈ 20.5M), run
on a memory-abundant box so the direct path could *complete* rather than OOM.

Source: [`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`](../../benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log)
(GEODE), Palace figure from [`benchmarks/transmon_bench_cpu/results.toml`](../../benchmarks/transmon_bench_cpu/results.toml)
`[matched.large_scale]` (4.1 GB/rank × 8 ≈ 33 GB aggregate).

| solver / config | wall (s) | peak RSS | outcome |
|-----------------|---------:|---------:|---------|
| GEODE-direct (COLAMD) | 565.5 | 92.2 GB | completed (`TOTAL_S = 565.531`, max RSS 92 166 884 kB) |
| GEODE-direct (custom AMD order) | — | 128.5 GB | **OOM-killed** (SIGKILL; max RSS 128 565 428 kB) |
| Palace, 8 ranks | 423 | ~33 GB | completed |

**GEODE loses on both axes at 1.16M DOF:** slower (565.5 s vs 423 s) *and* far heavier
(92.2 GB vs ~33 GB). The custom-AMD fill-reducing ordering — which won on *symbolic*
fill — does **not** predict real supernodal LU and OOM'd at 128.5 GB. The direct
factorization's fill-in grows super-linearly; the flop+fill crossover is below 1M DOF and
no factorization trick we tried closes it.

### 2c. Interior eigensolve at the physical deep shift — Palace's approach wins

The matrix-free scale path (to escape the direct memory wall) hits a **fundamental
preconditioner wall** at the physical σ = 4.5 GHz deep interior shift.

Source: [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md)

- At the default inner tolerance (`1e-10`), a **single** inner AMS-preconditioned MINRES
  solve — outer Lanczos step 0 — never converges: it plateaus on a nearly-flat residual
  tail below ~`1e-5` and was still running at 14 300 inner iterations (‖r‖ ≈ 2.57e-6)
  when killed. Dropping from `1e-5` (~1000 iters) to `2.6e-6` (~14 300 iters) took ~13 000
  more iterations — the tail has no steep asymptote.
- Loosening the inner tol to `1e-2` completes all 96 outer steps in **55.3 s**, but
  returns **non-physical** modes (the λ ≈ 0 gradient near-kernel hash at 0.42–0.64 GHz).
  There is **no single inner tol that is both cheap and correct** with the current AMS.
- The plateau is **coarse-solve-invariant** — even an exact coarse solve stalls — so the
  limiter is the SPD-proxy preconditioner `K + |σ|M`, not the coarse solve (issues
  [#562](https://github.com/rjwalters/geode-fem/issues/562) /
  [#565](https://github.com/rjwalters/geode-fem/issues/565)).

Palace **wins here by simply factorizing** the shift-invert operator (its preferred path
is a sparse direct solver; it applies AMS only to *definite* curl-curl, not the indefinite
eigenproblem inner solve). GEODE's matrix-free AMS-MINRES interior eigensolve is a harder
path than the incumbent even attempts.

### 2d. Summary — no clearly-preferred raw-performance corner

**There is no corner where GEODE is *clearly preferred* on raw solve performance.**

| axis | winner |
|------|--------|
| small–medium wall clock | GEODE (28.7 s vs 44.5 s) — but ~6× the memory, no parallel speedup |
| memory (all scales) | **Palace** |
| large-scale (≥1M DOF) wall clock **and** memory | **Palace** |
| interior eigensolve at the physical deep shift | **Palace** (direct factorization) |
| distributed scale (24.5M DOF, 99% efficiency) | **Palace** |
| target-insensitivity (off-target shifts) | GEODE — but bounded by the same memory wall |

GEODE's small–medium per-core wall-clock edge is real, but it is bounded by a hard memory
wall, buys no parallel scaling, and carries a ~6× memory penalty — so it does not amount
to a corner where GEODE is the clear choice. Palace wins scale, memory, the interior
eigensolve, and raw wall clock at the sizes that matter for production devices.

---

## 3. The architectural complement — what GEODE adds

GEODE is not trying to out-run Palace. Its value is a different substrate that opens a
capability Palace structurally lacks. Full framing in
[`2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md); the load-bearing
points:

- **Tensor-native assembly.** The FEM operators are built on Burn tensors, so assembly is
  a differentiable, GPU-portable computation rather than hand-written CPU kernels.
- **AI-hardware portability.** The same tensor program targets GPU / accelerator backends;
  the driven (frequency-domain linear-solve) problem — no interior-eigenvalue pathology —
  is where matrix-free + GPU genuinely wins and is the S-parameter / EPR workhorse.
- **Single-binary Rust.** No MPI cluster, no `mpirun -np`, no external solver stack to
  provision — one static binary. (The trade-off is the direct-solver memory wall of §2b.)
- **Differentiable-by-construction — with an honest boundary.** This is the intended
  differentiator, and it must be pitched precisely:

  - **Today (done):** the Burn tape reaches the assembled `K`, `M`, `b` and their
    dependence on **material ε** — assembly is differentiable.
  - **Just landed (done):** the missing **adjoint-through-solve** layer. The faer sparse
    factorization breaks the autodiff tape, so naïve reverse-mode yields *no* gradient of
    a solved observable. Issue [#570](https://github.com/rjwalters/geode-fem/issues/570) /
    PR [#573](https://github.com/rjwalters/geode-fem/pull/573) adds an explicit
    discrete-adjoint layer and proves it end-to-end on the **real, SPD electrostatic**
    solve (itself validated to <1%, O(h²), in
    [`benchmarks/electrostatic/results.toml`](../../benchmarks/electrostatic/results.toml)):
    one forward + one adjoint solve (reusing the same LU factors) returns `∂g/∂ε_k` for
    every material region, validated against a full central finite-difference of the whole
    pipeline at **worst-case relative error 2.9e-8**. This is the first validated
    solver-derived gradient in GEODE — a `∂observable/∂ε` that **Palace structurally
    cannot produce**.
  - **Roadmap (not done):** **geometry / shape sensitivities** — `∂observable/∂(geometry
    param)` via the adjoint plus a differentiable node-motion / design-param → mesh map —
    are issue [#571](https://github.com/rjwalters/geode-fem/issues/571), *in progress*.
    They are **not** claimed here. The material-ε adjoint is the proof of concept; shape
    sensitivities are the forthcoming extension.

The differentiator is design **sensitivities**, framed as a roadmap capability anchored by
one already-validated result (the ε-adjoint), not a claim of raw-speed superiority.

---

## 4. The honest bottom line for the Palace authors

- **Correctness:** GEODE independently reproduces Palace's transmon spectrum to 0.03% on
  the identical mesh. Treat this as a cross-check credential.
- **Performance:** Palace wins where it counts — scale, memory, the interior eigensolve,
  and raw wall clock at production sizes. GEODE has a narrow small–medium per-core
  efficiency edge that does not survive scaling. **No clearly-preferred GEODE raw-perf
  corner exists.**
- **Complement:** GEODE's tensor-native, single-binary, differentiable-by-construction
  substrate adds **solver-derived design sensitivities** (material-ε adjoint validated at
  2.9e-8; geometry sensitivities forthcoming) that a factorization-based solver cannot
  provide. That is the intended relationship: an independent cross-check that *adds* a
  capability, not a faster replacement.

---

### Sources cited

- [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml) — eigenmode agreement (0.032% worst-case)
- [`benchmarks/transmon_bench_cpu/results.toml`](../../benchmarks/transmon_bench_cpu/results.toml) — matched CPU head-to-head (133k + 1.16M)
- [`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`](../../benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log) — 1.16M-DOF run (565.5 s / 92.2 GB; AMD OOM at 128.5 GB)
- [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md) — σ=4.5 interior-eigensolve plateau
- [`benchmarks/electrostatic/results.toml`](../../benchmarks/electrostatic/results.toml) — electrostatic solver validation (adjoint demo problem)
- [`reference/CONFORMANCE.md`](../../reference/CONFORMANCE.md) — cross-backend / independent-solver agreement ledger
- [`docs/research/2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) — strategic framing
- Issues/PRs: [#570](https://github.com/rjwalters/geode-fem/issues/570), [#573](https://github.com/rjwalters/geode-fem/pull/573) (ε-adjoint, done); [#571](https://github.com/rjwalters/geode-fem/issues/571) (shape sensitivities, in progress); [#562](https://github.com/rjwalters/geode-fem/issues/562), [#565](https://github.com/rjwalters/geode-fem/issues/565) (plateau); [#518](https://github.com/rjwalters/geode-fem/issues/518) (no 8-thread speedup)
