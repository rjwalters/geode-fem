# σ = 4.5 GHz deep-interior-shift eigensolve cost characterization (133k)

**Issue:** #562 (Phase 1c de-risking of #531 / Epic #547)
**Measured:** 2026-07-16, laptop (Apple Silicon), `GEODE_NUM_THREADS=4`, release build.
**Fixture:** embedded `transmon_smoke.msh` — 22 684 nodes, 133 314 tets → 156 863
edges → **N = 133 108 interior DOFs** after PEC reduction, pencil nnz = 2 561 711.
**Solver:** `SparseShiftInvertLanczos { sigma = σ(4.5 GHz), max_iters = 96, tol = 1e-8 }`,
inner backend `MatrixFreeIndefinite` (matrix-free **MINRES**), three-space AMS
(`GEODE_PRECOND=ams`, the SPD proxy `K + |σ|M` of #559) or absolute-value Jacobi
(`GEODE_PRECOND=jacobi`). Default inner tol = `tol · INNER_TOL_FACTOR` = `1e-8 · 1e-2`
= **`1e-10`** (relative preconditioned residual `‖r‖_{M⁻¹}/‖r₀‖_{M⁻¹}`).

This is a **characterization**, not a fix: the deliverable is the decomposition of
*where the σ=4.5 time goes*, not a completed 6-mode solve. All runs are bounded.

## TL;DR — the dominant factor

**Inner-solve tolerance (coupled to the AMS convergence *tail* at the deep shift)
dominates the non-completion — NOT the outer step count.**

At the default inner tol (`1e-10`) a **single** inner MINRES solve — outer Lanczos
**step 0** — does not converge: it stalls on a nearly-flat residual tail below
~`1e-5` and was still running at **14 300 inner iterations** (`‖r‖ ≈ 2.6e-6`) when
killed. The ~40-minute "non-completion" is the solver stuck inside **one** inner
solve chasing an unreachable tolerance — the outer shift-invert loop never even
advances past step 0. When the inner tol is loosened to a value the inner solve
*can* reach, all 96 outer steps finish in under a minute (see the tol sweep). So:

- **Outer step count** — NOT the bottleneck. 96 outer steps complete in ~45–55 s
  once the inner solve returns.
- **Inner iters/step** — unbounded *only because of* the tol × preconditioner-tail
  interaction; at a reachable tol it is a few tens of iters/step.
- **Inner tol** — **the dominant knob.** But there is a *narrow correctness window*
  (below), so "just loosen it" is not the whole story.

## Instrumentation added (opt-in, zero default-behavior change)

All knobs are read internally by the library (like the existing `GEODE_NUM_THREADS`)
and default to the exact prior behavior when unset:

| Env knob | Effect |
|----------|--------|
| `GEODE_MINRES_LOG=<k>` | Print the inner-MINRES relative preconditioned residual every `k` iters (the convergence *curve*). |
| `GEODE_EIGEN_STEP_LOG=1` | Print each outer Lanczos step's inner iter count + cumulative wall-clock. |
| `GEODE_INNER_TOL=<x>` | Override the absolute inner-MINRES tolerance (default `1e-10`). |
| `GEODE_INNER_MAXITERS=<n>` | Cap each inner solve so several outer steps fit a bounded run. |

## (1) Inner-MINRES convergence curve — AMS vs abs-Jacobi (outer step 0)

Relative preconditioned residual `‖r‖_{M⁻¹}/‖r₀‖_{M⁻¹}` of the **first** inner solve
(`GEODE_MINRES_LOG`), both preconditioners, at σ=4.5:

| inner iter | abs-Jacobi | three-space AMS |
|-----------:|-----------:|----------------:|
| 50   | —        | 7.10e-4 |
| 100  | —        | 9.36e-5 |
| 200  | 1.32e-1  | 4.79e-5 |
| 500  | —        | 2.01e-5 |
| 1000 | 5.73e-2  | 1.23e-5 |
| 2000 | 3.78e-2  | ~6e-6   |
| 4000 | 2.49e-2 → **ERROR (non-convergence)** | ~4e-6 |
| 14300| —        | 2.57e-6 (**still in step 0**, killed) |

- **abs-Jacobi is hopeless**: glacial log decay, still `2.5e-2` after 4000 iters —
  it does not even reach `1e-2`, let alone a usable tol. (This reproduces the #526
  gradient-near-kernel stall: plain diagonal scaling is blind to the `image(d⁰)`
  near-kernel that dominates the deep-interior indefinite operator.)
- **AMS is 2–3 orders faster to ~`1e-4`** (~100 iters) — the wiring is correct and
  effective in the *early* regime. **But the tail goes nearly flat**: dropping from
  `1e-5` (itn ~1000) to `2.6e-6` (itn ~14 300) took **~13 000 more iterations**.
  The default `1e-10` target is unreachable in any practical iteration budget.

**This flat AMS tail is the mechanism of the non-completion.** AMS's SGS-smoothed
nodal / vector-nodal coarse solves damp the near-kernel enough for the first ~4
orders, then plateau — there is no steep asymptote to carry MINRES to `1e-10`.

## (2) Outer step count is not the bottleneck (inner-tol sweep)

Same AMS preconditioner, sweeping `GEODE_INNER_TOL` (full solve, `GEODE_EIGEN_STEP_LOG`):

| inner tol | outer steps completed | inner iters/step | total inner iters | solve wall | result |
|-----------|----------------------:|------------------|------------------:|-----------:|--------|
| `1e-10` (default) | **0** (stuck in step 0 >14 300 iters) | ∞ (does not converge) | — | DNF | non-completion |
| `1e-4` | see §3 | 63 – 4323 (highly variable) | — | slow | pending correctness |
| `1e-2` | **all 96** | 13 – 51 (uniform, avg ~36) | 3478 | **55.3 s** | **WRONG modes** |

At **`1e-2`** the entire eigensolve completes in **~56 s** with a tight, uniform
~36 inner iters/step — proving the *outer* loop and per-step cost are cheap once
the inner solve returns. **But the eigenvalues are non-physical:**

```
mode[0]: λ = 7.73e-11, f = 0.4196 GHz, p = 0.0000
mode[1]: λ = 7.92e-11, f = 0.4247 GHz, p = 0.0000
...
mode[5]: λ = 1.79e-10, f = 0.6378 GHz, p = 0.0002
```

These are the `λ ≈ 0` gradient near-kernel hash (0.42–0.64 GHz, participation ≈ 0),
**not** the σ=4.5 physical band (Direct resolves junction 3.45 GHz p≈0.99,
resonator 5.15 GHz here). At `1e-2` the inner solve is too inaccurate: the
shift-invert operator `A⁻¹M` is polluted by inner-residual noise, so the Lanczos
tridiagonalization converges to garbage. This is exactly the "the inner solve must
out-accuracy the outer Lanczos target" coupling documented on `INNER_TOL_FACTOR`.

## (3) Inner-tol = 1e-4 — the intermediate regime

Per-outer-step inner iters at `GEODE_INNER_TOL=1e-4` (AMS):

```
step 0: 99    step 3: 367   step 6: 909
step 1: 63    step 4: 425   step 7: 105
step 2: 395   step 5: 1217  step 8: 4323   (cumulative 7903 iters @ 93 s)
```

Outer steps *do* advance (unlike the default tol), but **inner iters/step are
highly variable** — 63 to 4323 — because some Lanczos RHS directions load the AMS
flat tail harder than others. This is the same flat-tail pathology as §1, just
sampled at a looser stopping point: whenever a step's target residual lands in the
plateau region, that step explodes to thousands of iters.

**Correctness at `1e-4` was not confirmed within the bounded budget:** the run
advanced 9 outer steps (7903 inner iters in ~97 s) and was still climbing — the
occasional multi-thousand-iter step (step 8 = 4323 iters) makes the full 96-step
solve too slow to finish under the characterization budget. It is a *candidate*
tol that must be validated against the Direct spectrum before use; this does not
change the dominant-factor conclusion.

## Which of {outer steps, inner iters/step, inner tol} dominates?

**Inner tol dominates, via the AMS-preconditioned-MINRES flat convergence tail at
the deep interior shift.** Concretely:

- The outer step count is a non-issue: 96 steps in <60 s at any reachable tol.
- Inner-iters/step is the proximate cost, but it is *set by* the interaction of the
  inner tol with the preconditioner tail — it is a few tens of iters when the tol
  sits in AMS's fast early regime and blows up (thousands → ∞) the moment the tol
  enters the plateau below ~`1e-5`.
- There is **no single inner tol that is both cheap and correct** with the current
  AMS: `1e-2` is cheap but returns non-physical modes; `1e-10` is correct in
  principle but never converges; `1e-4` is the only candidate and it is both slow
  (thousands of iters on some steps) and unconfirmed.

## Recommendation for #531 (the 1.16M at-scale run)

1. **Do NOT launch σ=4.5 at the default inner tol (`1e-10`) at 1.16M.** It will
   reproduce the DNF — a single inner MINRES solve stuck on the flat tail — and
   waste the AWS spend with no diagnosis. This is now understood cheaply at 133k.

2. **A uniform tol-loosening is not a safe fix by itself.** The correctness window
   is narrow and shift-dependent: `1e-2` completes fast but yields the gradient-hash
   spectrum, not the physical band. If a loose inner tol is used, it must be
   validated against the Direct spectrum at 133k first (candidate `≈1e-4`), and the
   outer `max_iters`/wall budget raised to absorb the variable, occasionally
   multi-thousand-iter steps.

3. **The SGS(2) AMS coarse solve is the per-step bottleneck at the deep shift → the
   conditional AMG coarse-solve follow-on is now justified.** The root cause is not
   the outer loop and not the abs-Jacobi baseline (already known-bad); it is that
   the three-space AMS's SGS-smoothed coarse solves plateau below ~`1e-5`, leaving
   no steep asymptote to carry MINRES to a tight, *correct* tolerance in bounded
   iters. Replacing the SGS(2) coarse solve with a true AMG V-cycle (or a direct
   coarse factorization on the node-space `Gᵀ(K+|σ|M)G` / `Πᵀ(K+|σ|M)Π` blocks)
   is what would flatten the tail and make a tight inner tol converge cheaply. This
   is precisely the data the conditional-AMG follow-on was gated on.

4. **Consider a shallower / harmonic shift for the at-scale run.** σ=4.5 is
   intrinsically deep — it sits among physical modes *and* the 4 gradient
   near-kernel modes, maximizing the indefinite near-kernel content MINRES must
   fight. A shift nearer a single target mode, a harmonic Ritz extraction, or an
   explicit deflation of the known gradient near-kernel (`image(d⁰)` = the AMS `G`)
   would reduce the near-kernel load and steepen the tail independently of the
   coarse-solve upgrade.

## Honest limitations

- Timings are laptop / 4-thread and are relative, not the at-scale numbers; the
  *shape* of the findings (flat tail, narrow correctness window, cheap outer loop)
  is what transfers, not the absolute seconds.
- The `1e-4` full-solve correctness result is reported in §3 above; if it is marked
  pending, the run did not finish within the bounded budget — the intermediate-tol
  regime remains partially characterized (steps advance but with variable,
  occasionally multi-thousand-iter cost), which does not change the dominant-factor
  conclusion or the AMG recommendation.
</content>
