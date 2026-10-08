# GEODE-FEM vs Palace — an honest, measured comparison

**Date:** 2026-07-16 · **updated 2026-07-18** (§3/§4 now reflect the complete, FD-validated
2×2 sensitivity matrix + eigenmode/inductance extensions shipped on `main`; §1–§2 unchanged)
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

### 2a. Small–medium (133 108 interior DOF)

Two measurements exist. The second one supersedes the first for anything that is
quoted.

#### 2a-i. The 2026-10-08 same-box re-timing (the numbers to use)

Source: [`benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml`](../../benchmarks/transmon_bench_cpu/results_like_for_like_m6i.toml),
generated from the raw logs under
[`runs/2026-10-08_m6i_like_for_like/`](../../benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like/README.md)
(issues [#927](https://github.com/rjwalters/geode-fem/issues/927) and
[#763](https://github.com/rjwalters/geode-fem/issues/763)). One m6i.4xlarge (8 cores /
16 vCPU, us-east-1), otherwise idle, one session; GEODE `61e8571e`, Palace `fba6a5b`
built from [`reference/palace/docker/Dockerfile`](../../reference/palace/docker/Dockerfile).
Every run's mode list is committed. "1 thread" is one CPU enforced with `taskset`;
"8" is one hardware thread on each of the eight cores. Wall is min / median / max over
three runs unless marked; core-seconds are user + sys of the whole process tree (median).

| request | solver, width | of Palace's 6 modes | other modes returned | wall (s) | core-s | peak RSS |
|---|---|---:|---:|---:|---:|---:|
| 6 modes @ 4.5 GHz (the historical request) | GEODE, 1 thread | **1** | 5 | 32.6 / 32.8 / 33.7 | 32.8 | 3.16 GB |
| | GEODE, 8 threads | **1** | 5 | 26.9 / 27.1 / 27.2 | 38.6 | 3.24 GB |
| 30 modes @ 4.5 GHz ("more modes") | GEODE, 1 thread | 6 | 24 | 34.0 / 34.5 / 35.2 | 34.4 | 3.16 GB |
| | GEODE, 8 threads | 6 | 24 | 28.5 / 29.1 / 29.3 | 40.7 | 3.24 GB |
| 6 modes @ 20 GHz ("shift") | GEODE, 1 thread | 6 | 0 | 32.6 / 33.2 / 33.6 | 33.2 | 3.16 GB |
| | GEODE, 8 threads | 6 | 0 | 26.6 / 27.3 / 28.1 | 38.9 | 3.23 GB |
| 8 modes @ 4.5 GHz, port-aware projection | GEODE, 1 thread | 6 | 2 | 66.9 / 67.3 / 67.7 | 67.2 | 3.23 GB |
| | GEODE, 8 threads | 6 | 2 | 50.1 / 51.1 / 51.2 | 91.7 | 3.29 GB |
| 6 modes @ 4.5 GHz, fixture config (`Save = 6`) | Palace, 1 rank | 6 | 0 | 186.7 / 186.8 / 188.0 | 186.8 | 2.20 GB |
| | Palace, 8 ranks | 6 | 0 | 33.1 / 33.4 / 33.5 | 266.7 | 0.54 GB/rank (3.8 GB all ranks) |
| same, no field output (`Save = 0`) | Palace, 1 rank | 6 | 0 | 113.0 (n = 1) | 113.0 | 2.24 GB |
| | Palace, 8 ranks | 6 | 0 | 23.6 / 23.6 / 23.8 | 188.9 | 0.54 GB/rank (3.8 GB all ranks) |

The three GEODE requests that return Palace's six modes agree with Palace to 0.029% on
every mode. They are reported side by side; **none is declared the one to compare
against Palace.** Each has a caveat:

- **More modes (30 @ 4.5 GHz).** The 30 was found by trial against the Palace
  eigenvalues. It is a property of this mesh, Krylov size and start vector, not a
  recipe, and the caller still needs the oracle to pick the six out of the thirty.
- **Shift (6 @ 20 GHz).** A different request from Palace's, with the shift chosen
  knowing where the modes are. It returns exactly the six.
- **Port-aware (8 @ 4.5 GHz).** The only request not tuned against the oracle. It keeps
  Palace's target and still returns two non-physical modes (a near-zero survivor and
  the 3.45 GHz port mode), so eight are requested to get six, at about twice the cost
  ([#950](https://github.com/rjwalters/geode-fem/issues/950)).

What the table shows, on this mesh and host:

- **The historical request is not like-for-like on the m6i either:** it returns one
  of the six modes, as it did on the Lambda host and the developer machine.
- **Wall clock at eight wide.** GEODE's two oracle-tuned requests take 27.3 s and
  29.1 s, and the port-aware one 51.1 s. Palace takes 33.4 s with its fixture config
  and 23.6 s when it writes no ParaView fields. GEODE's benchmark binary writes no
  fields, so the `Save = 0` row is the one without field output on either side.
  Against it, Palace has the lower wall clock than every GEODE cell.
- **Wall clock at one wide.** GEODE takes 33 to 35 s (67 s port-aware); Palace 186.8 s
  with field output (`Save = 6`) and 113.0 s without (`Save = 0`, one run). This is
  measured on this run against an untuned Palace: read it with the two caveats below.
- **Core-seconds.** GEODE uses fewer than Palace in every cell: 33 to 41 for the
  oracle-tuned requests and 67 to 92 for port-aware, against 113 to 267 for Palace.
  Palace's MPICH ranks busy-wait, so its core-seconds are close to ranks × wall by
  construction; this is CPU time occupied, not work done. The same two caveats apply.
- **`Save = 0` removes field output only; it does not match the two programs' work.**
  By Palace's own phase timers the `Save = 0` runs still spend 5.0 s of 23.6 s at
  8 ranks and 40.2 s of 112.9 s at 1 rank in its error estimator, which GEODE's
  benchmark has no counterpart for. Whether the estimator can be switched off at this
  Palace commit was not checked. It counts against Palace at both widths, so the
  one-wide "GEODE 33 to 67 s, Palace 113 to 187 s" in particular sets GEODE against a
  Palace time that includes work GEODE does not do.
- **Palace is not tuned.** Palace here is the vendored recipe's build (no libxsmm, `-march=x86-64-v2`), so its times are an upper bound for a tuned build.
  Every statement above that has GEODE ahead (one-wide wall clock, core-seconds) is a
  statement about this build of Palace.
- **Parallel scaling.** GEODE gains 17% in wall clock from 1 to 8 threads (142% CPU),
  for 1.17× the core-seconds. Palace gains 4.8× to 5.6× from 1 to 8 ranks.
- **Memory.** GEODE peaks at 3.2 GB in one process. Palace's largest rank peaks at
  0.54 GB at 8 ranks, and Palace's own estimate over all ranks is 3.8 GB (2.1 GB at
  1 rank). The earlier "about 6× less memory" compared one rank with the whole GEODE
  process; in total the two are about equal at eight ranks on this mesh.

What it does not show:

- Anything about another mesh size. §2b records that the direct path loses on both
  axes at 1.16M DOF.
- A tuned Palace. Palace was built as the vendored recipe builds it (distribution
  compiler-wrapper flags, `-march=x86-64-v2`, no libxsmm), and neither solver was built
  with `-march=native`. Only Palace's fixture solver settings were run. Read the Palace
  times as an upper bound on what a tuned Palace needs.
- A GEODE answer a user could trust without the oracle. In two of the three routes the
  request was tuned against Palace's answer, and in two of the three the six modes are
  identified by matching Palace's frequencies.
- Target robustness. Palace's `Target` is a lower bound (it returns the modes above
  it); GEODE's shift returns the modes nearest it on either side. For "12 modes @
  20 GHz" Palace returns twelve modes from 20.70 to 51.54 GHz and GEODE returns the six
  physical modes from 5.15 to 26.09 GHz plus six non-physical values; two modes are
  common to both lists (`[off_target_request]`). GEODE's own cost is nearly the same
  at both shifts (32.8 s and 33.9 s on one thread). The Palace half of the earlier
  "GEODE is target-insensitive, Palace degrades off-target" reading timed a different
  set of modes, so that reading is not a like-for-like comparison.

#### 2a-ii. The 2026-07-14 session (historical; do not quote)

Source: [`benchmarks/transmon_bench_cpu/results.toml`](../../benchmarks/transmon_bench_cpu/results.toml)
(`[matched.physical_target]`, m6i.4xlarge, us-west-2, GEODE `3174015`).

| solver / config | wall (s) | peak RSS |
|-----------------|---------:|---------:|
| GEODE, "1 thread" | 28.7 | 3.1 GB |
| GEODE, 8 threads | 29.0 | 3.1 GB |
| Palace, 1 rank | 130.9 | 0.5 GB/rank |
| Palace, 8 ranks | 44.5 | 0.5 GB/rank |

Off-target (12 modes @ 20 GHz, `[matched.off_target]`): GEODE 36.8 s / 26.6 s vs Palace
248.0 s (np1) / 64.7 s (np8).

The Palace rows do not match the 2a-i re-timing of the same request either, and they
miss in opposite directions: 186.8 s now against 130.9 s then at 1 rank, 33.4 s now
against 44.5 s then at 8 ranks (both `Save = 6`). **That difference is unexplained and
was not investigated.** The two Palace builds differ and the July build flags were not
recorded, which is a possible cause, not an established one. Neither pair is a
regression measurement.

Two things are wrong with reading these as a comparison, and both are now measured:

- **Not like-for-like output ([#927](https://github.com/rjwalters/geode-fem/issues/927)).**
  No mode log of that session was kept. The same request, re-run on the same instance
  type with logs (2a-i), returns one of Palace's six modes.
- **"1 thread" was not enforced ([#763](https://github.com/rjwalters/geode-fem/issues/763)).**
  The row set `GEODE_NUM_THREADS=1` with no pinning, at a commit where that did not
  make the LU serial, and recorded no CPU time. With one CPU enforced the same request
  takes 32.8 s; with eight threads, 27.1 s. The old 28.7 s is 13% below the first and
  6% above the second, and equals its own 8-thread row to 1%. That is consistent with
  the old "1 thread" row having run a multithreaded LU. It is not proof: the commit,
  region and session differ.

So "28.7 s on one core beats 44.5 s on eight ranks, about 12× fewer core-seconds" is
not supported. Measured, a one-core GEODE run of that request (32.8 s, one of six
modes) ties Palace on eight ranks with field output (33.4 s) and is behind Palace
without it (23.6 s); the core-second ratio is 8.1× and 5.8× respectively, and 4.0× to
8.0× (2.8× to 5.7×) for the GEODE requests that do return the six modes. Those ratios
are against an untuned Palace whose MPICH ranks busy-wait (core-seconds close to
8 × wall by construction) and whose runs include its error estimator (§2a-i).

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

| axis | reading |
|------|---------|
| small–medium wall clock, 8 wide | **Palace** without field output (23.6 s) vs GEODE 27.3–51.1 s for the requests that return the same six modes; GEODE's two oracle-tuned requests are ahead of Palace only when Palace also writes ParaView fields (33.4 s), and that Palace is untuned (§2a-i) |
| small–medium wall clock, 1 wide | no winner picked. Measured ahead on this run, Palace untuned: GEODE 33–67 s for the six-mode requests vs Palace 113.0 s (`Save = 0`, one run) to 186.8 s (`Save = 6`). Palace's build has no libxsmm and `-march=x86-64-v2`, so its times are an upper bound for a tuned build, and they include its error estimator (40.2 s of the 112.9 s `Save = 0` run), which GEODE does not run (§2a-i) |
| small–medium core-seconds | no winner picked. Measured ahead on this run, Palace untuned: GEODE 33–92 vs Palace 113–267. Palace's MPICH ranks busy-wait, so its core-seconds are close to ranks × wall by construction (CPU occupied, not work done); its build has no libxsmm and `-march=x86-64-v2`, so they are an upper bound for a tuned build; and they include its error estimator (§2a-i) |
| memory at 133k DOF | about equal in total at 8 ranks (GEODE 3.2 GB; Palace 3.8 GB over all ranks, 0.54 GB per rank) |
| memory at scale | **Palace** |
| large-scale (≥1M DOF) wall clock **and** memory | **Palace** |
| interior eigensolve at the physical deep shift | **Palace** (direct factorization) |
| distributed scale (24.5M DOF, 99% efficiency) | **Palace** |
| target-insensitivity (off-target shifts) | not established: the off-target cells compare different mode sets (§2a-i) |

The two small–medium rows where GEODE measured ahead are deliberately not entered as
GEODE wins, and the column is headed "reading" for that reason. What GEODE measured
ahead on (§2a-i) is core-seconds and one-core wall clock, against a Palace built from
the vendored recipe without libxsmm and with `-march=x86-64-v2` (an upper bound for a
tuned build), whose ranks busy-wait and whose runs include an error estimator GEODE does
not run. At eight wide Palace without field output is ahead. Two of GEODE's
three six-mode requests were tuned against Palace's answer, the margin is bounded by a hard
memory wall, and 8 threads buy 17% — so it does not amount
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
- **Differentiable-by-construction — the full sensitivity matrix, FD-validated.** This is
  the intended differentiator, and as of **2026-07-18 it is a demonstrated capability, not
  a roadmap item.** The faer sparse factorization breaks the naïve autodiff tape, so
  reverse-mode alone yields *no* gradient of a solved observable; the fix is an explicit
  **discrete-adjoint layer** — one forward + one adjoint solve reusing the same LU factors,
  supplying the custom backward for `Ax=b`. The complete
  **{material ε, geometry} × {scalar SPD, H(curl) Maxwell} 2×2 matrix** is now on `main`,
  every quadrant carrying a committed test that asserts the adjoint gradient matches a
  central finite-difference of the *whole* pipeline:

  | | material ε | geometry / shape |
  |---|---|---|
  | **scalar** (electrostatic, SPD) | [`adjoint.rs`](../../crates/geode-core/src/adjoint.rs) — `adjoint_gradient_matches_central_finite_difference`, rel-err **< 1e-4** ([#570](https://github.com/rjwalters/geode-fem/issues/570)/[#573](https://github.com/rjwalters/geode-fem/pull/573)) | [`shape.rs`](../../crates/geode-core/src/shape.rs) — `shape_gradient_matches_central_finite_difference`, rel-err **< 1e-4** (observed ~7e-12 hi-face / ~1e-9 control-node) ([#571](https://github.com/rjwalters/geode-fem/issues/571)/[#575](https://github.com/rjwalters/geode-fem/pull/575)) |
  | **H(curl)** (driven Maxwell, complex) | [`driven/adjoint.rs`](../../crates/geode-core/src/driven/adjoint.rs) — `driven_adjoint_gradient_matches_central_finite_difference`, rel-err **< 1e-3** ([#576](https://github.com/rjwalters/geode-fem/issues/576)/[#579](https://github.com/rjwalters/geode-fem/pull/579)) | [`driven/shape.rs`](../../crates/geode-core/src/driven/shape.rs) — `driven_shape_gradient_matches_central_finite_difference`, rel-err **< 1e-3** ([#577](https://github.com/rjwalters/geode-fem/issues/577)/[#581](https://github.com/rjwalters/geode-fem/pull/581)) |

  Both H(curl) quadrants additionally ship a `conjugation_error_is_detected_by_fd` tripwire
  that reproduces the complex-adjoint conjugation bug inline and asserts the FD catches it —
  a standing regression guard. The scalar ε-adjoint runs end-to-end on the **real, SPD
  electrostatic** solve (itself validated to <1%, O(h²), in
  [`benchmarks/electrostatic/results.toml`](../../benchmarks/electrostatic/results.toml)),
  returning `∂g/∂ε_k` for every material region from one forward + one adjoint solve.

  Two eigen / magnetostatic extensions land on top of the driven matrix, same FD discipline:
  - **Eigenmode sensitivities** via Hellmann–Feynman, in
    [`eigen/sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs) and validated in
    [`tests/transmon_eigen_sensitivity.rs`](../../crates/geode-core/tests/transmon_eigen_sensitivity.rs)
    ([#596](https://github.com/rjwalters/geode-fem/issues/596)/[#600](https://github.com/rjwalters/geode-fem/pull/600)):
    **material** `∂λ/∂ε` (`deigenvalue_deps`) — central-FD **< 1e-4** *and* a closed form **< 1e-6**
    (`material_sensitivity_matches_fd_and_closed_form`); **geometry** `∂λ/∂θ` node-motion
    (`deigenvalue_dtheta`) — central-FD **< 5e-3** (`geometry_sensitivity_matches_fd`); and a
    London-superconductor surface BC `∂λ/∂λ_L` (`deigenvalue_dlambda_l`) — central-FD **< 1e-4**
    (`london_lambda_l_sensitivity_matches_fd`, [#609](https://github.com/rjwalters/geode-fem/pull/609)).
  - **Inductance reluctivity sensitivity** `∂L_ij/∂ν_k` via the self-adjoint energy form —
    [`adjoint.rs::inductance_adjoint_sensitivity`](../../crates/geode-core/src/adjoint.rs),
    coax-fixture central-FD-validated in
    [`tests/magnetostatic_inductance.rs`](../../crates/geode-core/tests/magnetostatic_inductance.rs)
    ([#615](https://github.com/rjwalters/geode-fem/pull/615)).

  And two **applied** demonstrations, both from committed benchmark data, show the gradient
  driving real optimization:
  - **Gradient-based transmon design** — a damped-Newton optimizer drives a capacitor
    geometry parameter θ to a target charging energy `E_C` using the analytic `dE_C/dθ`
    from the electrostatic-energy adjoint (the capacitance→E_C chain,
    [#583](https://github.com/rjwalters/geode-fem/issues/583)/[#586](https://github.com/rjwalters/geode-fem/pull/586));
    one forward + one adjoint solve per step, the converged θ confirmed by an independent
    forward capacitance solve
    ([`benchmarks/transmon_diffopt/results.toml`](../../benchmarks/transmon_diffopt/results.toml),
    [#584](https://github.com/rjwalters/geode-fem/issues/584)/[#588](https://github.com/rjwalters/geode-fem/pull/588)).
    Pitched **honestly as a capability / step-count argument** — the derivative-free
    alternative (Qiskit Metal + HFSS/Palace parameter sweeps) needs `N_params` extra forward
    solves per step — **no wall-clock speedup is claimed**.
  - **DeviceLayout island-pad shape gradient on the real 133k-tet mesh** — the analytic
    `∂C_Σ/∂θ` for the actual SingleTransmon island conductor, FD-validated on the production
    mesh to **1.154e-4**. **Honest outcome:** the 89.9 fF anchor is *not* reachable under the
    fixed-topology node-motion map (the island's junction-attachment nodes sit ~225 µm from
    its centroid, so the prescribed scale inverts ~0.7 µm junction-region tets at
    θ ≈ −0.0097 while the anchor needs θ ≈ −0.24); the bounded Newton run stalls honestly at
    the distortion boundary, and a clearly-labeled in-budget target (`C_Σ → 137.0 fF`)
    converges, confirmed by an independent multi-conductor extraction
    ([`benchmarks/transmon_diffopt/pad_results.toml`](../../benchmarks/transmon_diffopt/pad_results.toml),
    [#589](https://github.com/rjwalters/geode-fem/issues/589)/[#590](https://github.com/rjwalters/geode-fem/pull/590)).

  The adjoint layer now also extends to the **p=2 (20-DOF) Nédélec** driven path — material
  and geometry shape adjoints on second-order elements
  ([#621](https://github.com/rjwalters/geode-fem/pull/621)/[#623](https://github.com/rjwalters/geode-fem/pull/623)) —
  so differentiability is not tied to lowest-order accuracy.

The differentiator is design **sensitivities**, now anchored by a *complete, FD-validated*
matrix spanning material/geometry × scalar/H(curl)/eigenmode/magnetostatic — not a single
result or a roadmap promise, and still not a claim of raw-speed superiority.

---

## 4. The honest bottom line for the Palace authors

- **Correctness:** GEODE independently reproduces Palace's transmon spectrum to 0.03% on
  the identical mesh. Treat this as a cross-check credential.
- **Performance:** Palace wins where it counts — scale, memory, the interior eigensolve,
  and raw wall clock at production sizes. On one 133k-DOF mesh GEODE measured ahead in
  core-seconds and one-core wall clock when it is asked in a way that returns Palace's
  six modes (§2a-i; two of the three such requests are oracle-tuned). That was against
  an untuned Palace (vendored recipe, no libxsmm, `-march=x86-64-v2`: an upper bound
  for a tuned build) whose MPICH ranks busy-wait and whose time includes an error
  estimator GEODE does not run; it does not hold at eight ranks without field output
  and does not survive scaling. **No clearly-preferred GEODE raw-perf
  corner exists.**
- **Complement:** GEODE's tensor-native, single-binary, differentiable-by-construction
  substrate adds **solver-derived design sensitivities** across the full
  {material, geometry} × {scalar, H(curl), eigenmode, magnetostatic} space — every quadrant
  FD-validated and test-asserted (rel-err ≤ 1e-4 for scalar and material/London eigenmode
  quadrants, ≤ 1e-3 H(curl), ≤ 5e-3 eigenmode geometry) — and the
  gradient already drives Newton-based transmon design on the real DeviceLayout mesh (§3).
  A factorization-based solver cannot provide these. That is the intended relationship: an
  independent cross-check that *adds* a capability, not a faster replacement.

---

### Sources cited

- [`benchmarks/transmon_eigen/results.toml`](../../benchmarks/transmon_eigen/results.toml) — eigenmode agreement (0.032% worst-case)
- [`benchmarks/transmon_bench_cpu/results.toml`](../../benchmarks/transmon_bench_cpu/results.toml) — matched CPU head-to-head (133k + 1.16M)
- [`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`](../../benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log) — 1.16M-DOF run (565.5 s / 92.2 GB; AMD OOM at 128.5 GB)
- [`benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md`](../../benchmarks/transmon_bench_cpu/sigma4p5_deepshift_characterization.md) — σ=4.5 interior-eigensolve plateau
- [`benchmarks/electrostatic/results.toml`](../../benchmarks/electrostatic/results.toml) — electrostatic solver validation (scalar-ε adjoint demo problem)
- [`benchmarks/transmon_diffopt/results.toml`](../../benchmarks/transmon_diffopt/results.toml) — gradient-based transmon E_C optimization (Newton via analytic dE_C/dθ)
- [`benchmarks/transmon_diffopt/pad_results.toml`](../../benchmarks/transmon_diffopt/pad_results.toml) — DeviceLayout island-pad ∂C_Σ/∂θ on the real 133k-tet mesh (FD rel-err 1.154e-4; honest anchor outcome)
- Adjoint/shape modules (each with a committed FD-validation test): [`adjoint.rs`](../../crates/geode-core/src/adjoint.rs) (scalar-ε + inductance ∂L/∂ν), [`shape.rs`](../../crates/geode-core/src/shape.rs) (geometry + capacitance→E_C chain), [`driven/adjoint.rs`](../../crates/geode-core/src/driven/adjoint.rs) (H(curl) material), [`driven/shape.rs`](../../crates/geode-core/src/driven/shape.rs) (H(curl) geometry), [`eigen/sensitivity.rs`](../../crates/geode-core/src/eigen/sensitivity.rs) (eigenmode ∂λ/∂ε, ∂λ/∂θ, ∂λ/∂λ_L, Hellmann–Feynman)
- [`reference/CONFORMANCE.md`](../../reference/CONFORMANCE.md) — cross-backend / independent-solver agreement ledger
- [`docs/research/2026-07-16-strategic-direction.md`](./2026-07-16-strategic-direction.md) — strategic framing
- Sensitivity Issues/PRs: 2×2 matrix — [#570](https://github.com/rjwalters/geode-fem/issues/570)/[#573](https://github.com/rjwalters/geode-fem/pull/573) (scalar ε), [#571](https://github.com/rjwalters/geode-fem/issues/571)/[#575](https://github.com/rjwalters/geode-fem/pull/575) (geometry), [#576](https://github.com/rjwalters/geode-fem/issues/576)/[#579](https://github.com/rjwalters/geode-fem/pull/579) (H(curl) ε), [#577](https://github.com/rjwalters/geode-fem/issues/577)/[#581](https://github.com/rjwalters/geode-fem/pull/581) (H(curl) geometry); extensions — [#596](https://github.com/rjwalters/geode-fem/issues/596)/[#600](https://github.com/rjwalters/geode-fem/pull/600) (eigenmode ∂λ/∂x), [#609](https://github.com/rjwalters/geode-fem/pull/609) (London ∂λ/∂λ_L), [#615](https://github.com/rjwalters/geode-fem/pull/615) (inductance ∂L/∂ν); applied — [#583](https://github.com/rjwalters/geode-fem/issues/583)/[#586](https://github.com/rjwalters/geode-fem/pull/586) (C→E_C chain), [#584](https://github.com/rjwalters/geode-fem/issues/584)/[#588](https://github.com/rjwalters/geode-fem/pull/588) (E_C opt), [#589](https://github.com/rjwalters/geode-fem/issues/589)/[#590](https://github.com/rjwalters/geode-fem/pull/590) (island-pad on real mesh); p=2 adjoint — [#621](https://github.com/rjwalters/geode-fem/pull/621)/[#623](https://github.com/rjwalters/geode-fem/pull/623); [#562](https://github.com/rjwalters/geode-fem/issues/562), [#565](https://github.com/rjwalters/geode-fem/issues/565) (plateau); [#518](https://github.com/rjwalters/geode-fem/issues/518) (no 8-thread speedup)
