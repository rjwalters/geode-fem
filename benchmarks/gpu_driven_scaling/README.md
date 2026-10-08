# benchmarks/gpu_driven_scaling/

GPU-vs-CPU wall-clock scaling of the lumped-port driven solve on one physical
fixture (the σ-lossy parallel-plate cube with a single lumped port), refined
via `cube_tet_mesh(n)`. Both files are performance records produced by the
`--ignored` release-tier test
[`crates/geode-core/tests/gpu_driven_scaling.rs`](../../crates/geode-core/tests/gpu_driven_scaling.rs).

There are three separate measurements here, on different hardware. They are not
comparable cell-for-cell and must not be merged.

| File | Issue | Hardware | Sizes |
|---|---|---|---|
| [`results.toml`](results.toml) | #501 | AWS g6e.xlarge: NVIDIA L40S, AMD EPYC 7R13 with 4 vCPU | n ∈ {6, 9, 12, 15}, 1 854 → 25 695 edges |
| [`results_large_a100.toml`](results_large_a100.toml) | #520, PR #929 | Lambda Cloud `gpu_1x_a100_sxm4`: NVIDIA A100-SXM4-40GB, AMD EPYC 7J13 with 30 vCPU | 25 695 → 462 520 edges, plus Palace driven runs on the same meshes |
| [`results_ams_cpu_local.toml`](results_ams_cpu_local.toml) | #930, #946, #945 | A developer Mac (Apple M3 Ultra) under unrelated load. **Not a benchmark host**: iteration counts are the result, times are indicative only | n ∈ {6, 9, 12, 15, 20, 24}, 1 854 → 102 024 edges, CPU f64 only, no Palace |

Each file is the authoritative record of its own numbers and caveats. This
README only explains what is in the directory.

## Layout

| Path | Contents |
|---|---|
| `results.toml` | The #501 L40S record: four solver configurations (Direct faer LU, assembled-CSR COCG + Jacobi, matrix-free on ndarray f64, matrix-free on CUDA f32) |
| `results_large_a100.toml` | The #520 A100 record: (1) GPU f32 vs CPU f64 matrix-free crossover, (2) CPU f64 references at every size, (3) geode vs Palace driven head-to-head |
| `box_sweep.sh` | Runs one leg of the test per call under `/usr/bin/time -v`, with an `nvidia-smi` poll on GPU legs. Writes `<leg>.stdout` (the test output, containing the TOML fragment), `.err`, `.time`, `.meta` and, for GPU legs, `.gpumem` |
| `box_palace_driven.sh` | Runs Palace (the `palace:cuda` image built for #519) on a cube mesh exported by the geode harness, on GPU or CPU, and keeps the log, timing, GPU memory poll, config and port CSVs |
| `palace_driven_cfg.py` | Emits the Palace driven config, mapping geode's natural units to SI. Called by `box_palace_driven.sh` |
| `summarize_520.py` | Merges the leg outputs in a sweep directory into one table on stdout |
| `runs/2026-10-07_lambda_a100/` | The evidence tree from the #520 session (below) |
| `results_ams_cpu_local.toml` | The #930 local record: assembled COCG with the Jacobi preconditioner against the same COCG with geode's AMS preconditioner, with Direct as the accuracy reference. Generated, not hand-edited |
| `ams_cpu_sweep.sh` | Runs the test once per (size, config) in its own process under `/usr/bin/time` (macOS `-l` or GNU `-v`), plus one combined cross-check process per size. Writes `<leg>.stdout`, `.err`, `.time`, `.meta` and `host.txt` |
| `summarize_ams_local.py` | Writes `results_ams_cpu_local.toml` to stdout from an `ams_cpu_sweep.sh` run tree |
| `runs/2026-10-08_local_ams/` | The evidence tree behind `results_ams_cpu_local.toml`: the legs `n<N>_{direct,jacobi,ams,xcheck}` (default threading), `single_thread/` (the Jacobi and AMS legs rerun with `RAYON_NUM_THREADS=1`; every time ratio comes from these), `diagnostics/` (three one-off patched builds, stored as patch files plus captured output and described in its `NOTE.txt`) and `threading_946/` (the before / after measurement of the #946 threading fix, one sweep tree per build and threading mode, plus `eigen_loop/`; see its `NOTE.txt`) |

Inside `runs/2026-10-07_lambda_a100/`:

| Path | Contents |
|---|---|
| `chain520.sh`, `chain520b.sh` | The exact sequences of `box_sweep.sh` and `box_palace_driven.sh` calls that were run, with `chain.out` / `chain2.out` (per-leg exit codes) and the `CHAIN_DONE` / `CHAIN2_DONE` markers |
| `A1_gpu_mf_full.*`, `A2_gpu_mf_cap300.*` | GPU f32 matrix-free legs: full solves at n = 20, 24 and 300-iteration capped runs at n = 30, 40 |
| `C1_cpu_mf_full.*`, `C2_cpu_mf_cap300.*` | The same split for the CPU f64 matrix-free leg (full solves at n = 15, 20, 24) |
| `S1_gpu_mf_cap1.*`, `S2_cpu_mf_cap1.*` | 1-iteration capped runs at n = 30, 40. They isolate the per-ω setup time that is subtracted to get the net per-iteration cost of the capped cells |
| `B1_cpu_direct_iter.*`, `B2_cpu_iter_large.*` | CPU f64 references: Direct LU and assembled COCG at n = 15 to 30, and assembled COCG only at n = 36, 40 |
| `partial/` | The first GPU matrix-free leg (`A_gpu_mf`, sizes 15 to 40 in one process), stopped mid-run before the chain was revised, so its `.stdout` has no end marker and its `.meta` has no exit code. It completed n = 15, and that is the source of the 25.7k-edge GPU cell. `summarize_520.py` reads it and drops any truncated trailing table |
| `tf32_evidence/` | The n = 6 smoke logs behind the TF32 finding: `cuda-smoke.log` is the stock f32 matmul stagnating at relative residual 0.163 after 20 000 iterations; `cuda-notf32-smoke.log` and `cuda-notf32-smoke2.log` are the reworked matvec, whose recursion converges in 782 iterations |
| `ab/`, `ab_new_matvec_n12.*` | The CPU A/B at n = 12 (13.4k edges) showing the matvec change is not a CPU regression: `ab/stock_matmul_n12.*` is the stock matmul, `ab_new_matvec_n12.*` is the new form |
| `palace/` | The Palace driven runs, 18 in total (GPU on 1 rank and CPU on 8 ranks at n = 15, 20, 24, 30, 40; three runs each at n = 20 and 40). Per run: a flat `<tag>.log`, `.time`, `.gpumem`, `_config.json` and `_port-{S,V,I}.csv`, plus a `<tag>/` directory with the config, `time.txt` and the unit mapping (`units.txt`). `runs.txt` has one summary line per run |
| `palace_validate/` | The single 1-rank CPU Palace run at n = 6 (1 854 edges) used to validate the config and unit mapping first; it keeps Palace's `postpro/` CSVs |
| `geode_checkout.txt`, `mesh_sha256.txt`, `nvidia-smi-end.txt` | The box checkout's commit and diff stat, the sha256 of each exported mesh, and the GPU state at the end of the session |

## Regenerating

`results.toml` (#501) comes straight from the test:

```sh
cargo test -p geode-core --release --test gpu_driven_scaling -- --ignored --nocapture                  # CPU leg
cargo test -p geode-core --release --features cuda --test gpu_driven_scaling -- --ignored --nocapture  # GPU leg, on a GPU host
```

`results_large_a100.toml` (#520) is produced on a GPU box. Run the legs with
`box_sweep.sh <leg-name> <cpu|cuda> [VAR=value ...]` and the Palace runs with
`box_palace_driven.sh <n> <GPU|CPU> <ranks> [runs]`; `chain520.sh` and
`chain520b.sh` in the run tree record the exact invocations and
`GEODE_SCALING_*` settings used. Then:

```sh
python3 -I benchmarks/gpu_driven_scaling/summarize_520.py \
  benchmarks/gpu_driven_scaling/runs/2026-10-07_lambda_a100
```

`summarize_520.py` prints a merged table; it does not write
`results_large_a100.toml`. That file is hand-transcribed from the script's
output, and the setup-subtracted per-iteration costs of the capped cells,
`(t_cap300 - t_cap1) / 299`, and the GPU/CPU ratios were computed by hand from
it.

`results_ams_cpu_local.toml` (#930) is generated end to end. On any CPU host:

```sh
benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh <run-dir> 6 9 12 15 20 24
python3 -I benchmarks/gpu_driven_scaling/summarize_ams_local.py <run-dir> > <results-file>
```

For timings, rerun the two iterative legs single-threaded into the
`single_thread/` subdirectory, which the summarizer picks up:

```sh
RAYON_NUM_THREADS=1 SKIP_DIRECT=1 benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh <run-dir>/single_thread 6 9 12 15 20 24
```

A before / after comparison of two builds goes into `threading_946/`, one
tree per build and threading mode, named `<build>_<threading>[_rep<K>]` with
build `main` or `fix` and threading `default` or `rayon1`:

```sh
SKIP_XCHECK=1 benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh <run-dir>/threading_946/fix_default 15 20 24
```

A before / after comparison of the smoother weight goes into `smoother_945/`,
one tree per build and drive frequency, named `<build>_w<omega>[_rayon1]` with
build `main`, `fix` or `w060` (the fix build with the weight forced back to
0.6) and the frequency written as `0p10`:

```sh
GEODE_SCALING_OMEGA=0.20 benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh <run-dir>/smoother_945/fix_w0p20 6 9 12 15 20 24
SKIP_DIRECT=1 GEODE_SCALING_OMEGA=0.20 GEODE_AMS_SMOOTH_WEIGHT=0.6 \
  benchmarks/gpu_driven_scaling/ams_cpu_sweep.sh <run-dir>/smoother_945/w060_w0p20 6 9 12 15 20 24
```

`smoother_945/NOTE.txt` in the committed run tree has the commands for its
`ritz_gap/` and `validation/` subdirectories.

Running the summarizer over the committed `runs/2026-10-08_local_ams/`
reproduces the committed file byte for byte:

```sh
python3 -I benchmarks/gpu_driven_scaling/summarize_ams_local.py \
  benchmarks/gpu_driven_scaling/runs/2026-10-08_local_ams | cmp - benchmarks/gpu_driven_scaling/results_ams_cpu_local.toml
```

The AMS config is opt-in (`GEODE_SCALING_CONFIGS=...,iterative_ams`); the
default config set and its output are unchanged.

## AMS vs Jacobi on this fixture (#930, local subset)

The file is the authority. Everything below is from one fixture at ω = 0.10 on
a loaded developer machine.

> **This section is the record of the AMS as it was before #945**, when its
> smoother weight was fixed at 0.6. That weight is now lowered where it is
> proven unstable, which it is on this fixture. For the current code read
> [The smoother weight, before and after #945](#the-smoother-weight-before-and-after-945):
> 26 to 55 iterations instead of 70 to 2 270. "Shipped" below means the code
> at the time.

**In short: the shipped AMS loses to Jacobi here, and the reason is one
constant.** The AMS edge smoother is a damped-Jacobi sweep with weight 0.6
(`DEFAULT_SMOOTH_WEIGHT` in `crates/geode-core/src/eigen/ams.rs`). On this
fixture that is above the smoother's stability bound, and the iteration count
grows with the mesh. In a one-off diagnostic with a lower weight the count is
nearly flat and AMS is faster than Jacobi. The library constant is not changed
here, and no replacement value has been validated.

| edges | COCG + Jacobi | COCG + AMS, shipped (weight 0.6) | AMS, diagnostic weight 0.45 | AMS, diagnostic weight 0.65 |
|---|---|---|---|---|
| 1 854 | 807 | 70 | 26 | 151 |
| 5 859 | 1 463 | 139 | 28 | 454 |
| 13 428 | 2 167 | 307 | 34 | 1 335 |
| 25 695 | 2 969 | 533 | 39 | 2 037 |
| 59 660 | 4 303 | 1 741 | 48 | 5 646 |
| 102 024 | 5 259 | 2 209 | 58 | 7 764 |

Iteration counts. The two diagnostic columns are single runs of a throwaway
patched build, not shipped code. The shipped column is the default-threading
run; single-threaded, its last three counts were 532, 1 683 and 2 234 (serial
and parallel triangular solves differ in roundoff, and a slowly converging
solve amplifies it). Those counts are from before the #946 threading fix; with
it both modes give 533, 1 693 and 2 270 (next section).

What the shipped AMS does (`[[cell]]`, `[[single_thread_cell]]`, `[finding]`):

- **It converges, and to the right answer.** Every solve met the 1e-8
  tolerance on the explicitly recomputed residual, and its field agrees with
  the Direct solution to 1.3e-11 to 6.0e-11 (relative L2).
- **Its iteration count is not flat.** It grows as edges^0.86 by the endpoint
  fit (smallest and largest size only) and edges^0.91 by least squares over all
  six sizes. Jacobi grows as edges^0.47 by either fit. The advantage in
  iterations shrinks from 11.5× at 1.9k edges to 2.4× at 102k.
- **It was slower than Jacobi at every size.** Single-threaded
  (`RAYON_NUM_THREADS=1`), one AMS iteration cost 18× to 26× a Jacobi
  iteration, and the AMS solve took 1.7× to 11.1× as long as the Jacobi solve.
  These are single samples and the factors are indicative.
- **The default-threading AMS timings are not the cost of AMS.** In the build
  these tables measured, the V-cycle's two small sparse triangular solves per
  iteration fanned out to the rayon pool. On this oversubscribed host that
  became kernel time: the 102k-edge AMS leg used 168 s of user time and 487 s
  of system time for a 274 s solve, and the same solve single-threaded took
  93 s with 0.5 s of system time. The file records user and system seconds per
  cell and takes no time ratio from the default-threading legs. Whether the
  parallel solve helps on an idle host was not measured. This is fixed in #946
  (next section); the `[[cell]]` tables were not rerun.

Why (`[diagnostics]`, from `runs/2026-10-08_local_ams/diagnostics/`):

- **The cause is the smoother weight.** The largest eigenvalue ρ of D⁻¹P (P is
  the real proxy matrix the V-cycle smooths, D its diagonal) was estimated by
  power iteration at 3.39 to 3.42 across the six sizes, with a Gershgorin
  upper bound of 4.00. Damped Jacobi is stable for weights below 2/ρ ≈ 0.585.
  The shipped 0.6 is above that at every size.
- **The fix is not a narrow sweet spot.** Nine weights were run over all six
  sizes. Every weight from 0.2 to 0.55 gave 25 to 58 iterations, growing about
  as edges^0.10 to edges^0.21. At 0.58 the count rises (44 to 88), at 0.6 it is
  the shipped 70 to 2 234, and at 0.65 it is 151 to 7 764, which is more
  iterations than Jacobi at the two largest sizes.
- **With a weight in that range AMS beat Jacobi.** Single-threaded, same
  session, setup plus Krylov, at weight 0.45: 0.40 s against Jacobi's 1.16 s at
  25.7k edges, 1.37 s against 4.51 s at 59.7k, and 2.97 s against 8.53 s at
  102k, about 2.9× to 3.3× faster. Weights 0.3 to 0.55 gave the same within
  10%.
- **Two earlier suspects are not the cause.** The growth persists for every PEC
  layout tried and with an exact LU on both auxiliary spaces. Both of those
  series were run at weight 0.6.
- **Limits.** The weight diagnostic is a throwaway patch (stored as
  `smooth_weight.patch`, not applied), single runs, one fixture, one
  frequency. It identifies the cause on this fixture. It does not validate
  0.45 or any other value.

What this does and does not say:

- **Do not change the default driven preconditioner on this evidence.** It
  stays Jacobi.
- "AMS is slower than Jacobi" and "the spiral's flat band does not carry over"
  are statements about the shipped weight on this fixture only. They are not
  statements about AMS preconditioning.
- The spiral measurement that motivated AMS (#742 / #744) held 102 to 155
  iterations from 14k to 53k edges at the same weight 0.6. The mesh may be the
  difference: this fixture is a structured Kuhn-tet cube and the spiral is an
  unstructured mesh, and ρ depends on the mesh. This is untested. The spiral
  was not rerun and its ρ was not measured.
- Only ω = 0.10 was run, so nothing here says how the count depends on
  frequency.

Recommended follow-ups, in this order. None is done or filed here:

1. A library issue for the AMS edge smoother: take the weight from an estimate
   of the spectral radius, or use an l1-Jacobi smoother, and validate on the
   spiral, the transmon AMS tests and this fixture. Done in #945 (below).
2. The parallel triangular solve inside the V-cycle, which caused the rayon
   contention above. Done in #946 (next section).
3. Only then, the sizes above 102k edges and the same-host Palace comparison,
   on the fixed preconditioner.

Also not done: the decision on the default preconditioner and the matrix-free
/ GPU AMS follow-up. Issue #930 stays open.

## The AMS thread contention, before and after #946

`[threading_fix]` in `results_ams_cpu_local.toml`, from
`runs/2026-10-08_local_ams/threading_946/`. Same fixture, same loaded
developer machine, single samples.

**What was wrong.** Each AMS application solves with a cached sparse LU twice,
through faer's `Lu::solve_in_place`. That call takes its thread count from a
process-global faer setting, which defaults to every core, so two small
triangular solves per COCG iteration went out to the rayon pool. The threads
spent their time contending for work.

**The fix.** The `SolverMode::Iterative` back-solve now holds a
`SequentialSolveScope` for the whole Krylov solve when the preconditioner is
AMS. The preconditioner setup, the other preconditioners and the direct solver
are untouched.

AMS leg, whole process (mesh, assembly, setup and solve). `main` is
`origin/main` at `2803aa96`. The two values in a `fix` cell are two repeats:

| edges | build | threading | iterations | user s | sys s |
|---|---|---|---|---|---|
| 25 695 | main | default | 533 | 7.3 | 13.0 |
| 25 695 | main | `RAYON_NUM_THREADS=1` | 532 | 5.3 | 0.1 |
| 25 695 | fix | default | 533 | 5.4, 5.3 | 0.2, 0.4 |
| 25 695 | fix | `RAYON_NUM_THREADS=1` | 533 | 6.1, 5.5 | 0.2, 0.1 |
| 59 660 | main | default | 1 741 | 67.7 | 187.8 |
| 59 660 | main | `RAYON_NUM_THREADS=1` | 1 683 | 41.3 | 0.4 |
| 59 660 | fix | default | 1 693 | 44.2, 42.8 | 0.5, 0.6 |
| 59 660 | fix | `RAYON_NUM_THREADS=1` | 1 693 | 43.4, 43.6 | 0.4, 0.4 |
| 102 024 | main | default | 2 209 | 173.5 | 646.5 |
| 102 024 | main | `RAYON_NUM_THREADS=1` | 2 234 | 96.8 | 0.6 |
| 102 024 | fix | default | 2 270 | 110.1, 101.8 | 1.1, 1.2 |
| 102 024 | fix | `RAYON_NUM_THREADS=1` | 2 270 | 99.5, 106.0 | 0.7, 0.7 |

- **The kernel time is gone.** At 102k edges it went from 646 s to about 1 s
  with the default threading. CPU time per iteration, whole process: 371 ms
  before, 45 to 49 ms after, against 44 to 47 ms with `RAYON_NUM_THREADS=1`.
- **Default threading now costs the same as one thread.** After the fix the
  two modes run the same sequential code. The spread between them is no
  larger than the spread between repeats of one mode, and at 102k edges in the
  second pair the default-threading leg used less CPU time than the
  single-threaded one.
- **No wall-clock speedup is quoted.** The 1-minute load average ran from 24
  to 197 on 28 CPUs during these runs. Several single-threaded legs got well
  under one CPU (`process_cpu_pct` in the file), so their wall-clock times
  measure the host. The file records them and flags this.
- **Where the contention was.** The scope changes only faer's global
  parallelism during the Krylov solve, and the only faer calls in that solve
  are the two `lu_solve` calls per iteration (`eigen/ams.rs`). Making them
  sequential removed the kernel time, so it was in those calls.
- **The results agree to roundoff, and are not bit-identical to `main`.** After
  the fix the default-threading and single-threaded legs agree in iteration
  count, explicit residual and port voltage to every printed digit at all
  three sizes. Before it they did not (2 209 against 2 234 iterations at 102k
  edges). The fix build reproduces neither `main` count exactly, because
  faer's `Par::Seq` solve differs in roundoff from its rayon solve even on one
  thread, and at the shipped smoother weight a slowly converging solve
  amplifies roundoff into a different count. Every leg met the same 1e-8
  explicit-residual tolerance, and the port voltages of the two builds agree to
  1.1e-11 or better (from the 10 printed digits).
- **The direct solver is unaffected.** Its path is not touched. The Direct legs
  give the same port voltage and residual in both builds, and the same user
  time: 236 s on `main`, 237 s and 239 s on the fix build at 102k edges. The
  direct solver does use the threads: on `main` the same leg took 210 s of
  wall clock with `RAYON_NUM_THREADS=1` against 33 s (neither leg was short of
  CPU: 96% and 886%).

**The same pattern in the eigensolver** (`[threading_fix.eigen_loop]`). The
direct shift-invert Lanczos loop runs one sparse triangular solve per step, on
the same global setting. A throwaway test (24 389 DOFs, 20 modes; stored under
`threading_946/eigen_loop/`) measured the eigensolve alone:

| build | threading | user s | sys s |
|---|---|---|---|
| main | default | 5.3 | 10.7 |
| main | default, Lanczos loop made sequential from the test | 3.6 | 4.4 |
| main | `RAYON_NUM_THREADS=1` | 2.7 | 0.1 |
| fix | default | 3.6 | 2.4 |
| fix | `RAYON_NUM_THREADS=1` | 2.5 | 0.1 |

One retained sample per row. The repeats in `eigen_loop/NOTE.txt` span 10.7 to
15.7 s of system time for `main` with the default threading and 2.3 to 7.2 s
for the fix build. The system time that remains is attributed to the
factorization, which is still parallel and was not measured separately. The
direct eigen backends now hold the same scope for the loop. The 20 eigenvalues
are bit-identical between the two threading modes after the fix (they were not
before), and differ from `main`'s by at most 4.2e-12 relative.

Not measured: an idle host; sizes above 102k edges, where a parallel triangular
solve might pay; the complex shift-invert Lanczos loop and the matrix-free
eigen paths with an LU coarse solve, which have the same structure and are not
changed (#956); and concurrent solves (the CLI's `--jobs`). The scope sets a
process-global faer setting because faer's sparse solve takes no per-call
thread count; rjwalters/faier#45 asks for one.

## The smoother weight, before and after #945

`[smoother_weight]` in `results_ams_cpu_local.toml`, from
`runs/2026-10-08_local_ams/smoother_945/` (`NOTE.txt` there). Same fixture,
developer machine, single samples. `main` is `origin/main` at `61e8571e`.

**What was wrong.** The AMS V-cycle smooths with damped point-Jacobi,
`z = w D⁻¹ r`, which converges only for `w < 2/λ_max(D⁻¹P)` (P is the real
proxy matrix the V-cycle smooths, D its diagonal). The weight was fixed at
0.6. On this fixture `λ_max` is 3.39 to 3.42, so the bound is 0.585 to 0.591
and 0.6 is above it.

**The fix.** Each preconditioner build now takes 30 steps of plain Lanczos on
`D^-1/2 P D^-1/2` and a Gershgorin row-sum bound `g`. With `θ` the largest Ritz
value:

```text
w = 0.6                       if 0.6 θ < 2
w = 1.5 / min(1.1 θ, g)       otherwise
```

COCG + AMS iteration counts, before (weight 0.6) and after, against Jacobi:

| edges | ω = 0.05: Jacobi | before | after | ω = 0.10: Jacobi | before | after | ω = 0.20: Jacobi | before | after |
|---|---|---|---|---|---|---|---|---|---|
| 1 854 | 878 | 68 | 27 | 807 | 70 | 26 | 695 | 77 | 29 |
| 5 859 | 1 653 | 135 | 29 | 1 463 | 139 | 30 | 1 271 | 143 | 32 |
| 13 428 | 2 436 | 299 | 33 | 2 167 | 307 | 34 | 1 894 | 343 | 34 |
| 25 695 | 3 268 | 505 | 37 | 2 969 | 533 | 39 | 2 494 | 556 | 40 |
| 59 660 | 4 840 | 1 896 | 47 | 4 303 | 1 693 | 49 | 3 803 | 1 729 | 52 |
| 102 024 | 6 153 | 2 166 | 55 | 5 259 | 2 270 | 55 | 4 744 | 2 359 | 57 |

"Before" at ω = 0.10 is `main`. `main` has no frequency knob, so at 0.05 and
0.20 it is the fix build with `GEODE_AMS_SMOOTH_WEIGHT=0.6`; at ω = 0.10 both
were run and agree to every printed digit at all six sizes.

The estimate and the weight at ω = 0.10 (the other two frequencies are within
0.3% of these; all 18 are in the file):

| edges | Ritz `θ` (30 steps) | 1000-step reference | Gershgorin `g` | `ρ̂ = min(1.1θ, g)` | weight | bound `2/g` … `2/reference` |
|---|---|---|---|---|---|---|
| 1 854 | 3.3760 | 3.3876 | 3.9994 | 3.7136 | 0.4039 | 0.500 … 0.590 |
| 5 859 | 3.4065 | 3.4075 | 3.9997 | 3.7472 | 0.4003 | 0.500 … 0.587 |
| 13 428 | 3.4062 | 3.4146 | 3.9999 | 3.7468 | 0.4003 | 0.500 … 0.586 |
| 25 695 | 3.4163 | 3.4179 | 3.9999 | 3.7579 | 0.3992 | 0.500 … 0.585 |
| 59 660 | 3.4127 | 3.4205 | 3.9999 | 3.7540 | 0.3996 | 0.500 … 0.585 |
| 102 024 | 3.4133 | 3.4215 | 4.0000 | 3.7546 | 0.3995 | 0.500 … 0.585 |

- **The growth is gone at all three frequencies.** 26 to 57 iterations
  everywhere, growing as edges^0.18 to edges^0.19 by least squares, where the
  old weight grew as edges^0.90 to edges^0.93 and Jacobi grows as edges^0.47 to
  edges^0.48. AMS now needs 24× to 112× fewer iterations than Jacobi.
- **It is not flat by this file's own definition.** The count still rises by
  1.97× to 2.12× from 1.9k to 102k edges, outside the 1.52 band of the spiral
  measurement. The #944 weight sweep showed the same residual rise at every
  stable weight from 0.2 to 0.55, so it is not the smoother weight. Its cause
  was not investigated here.
- **CPU time.** Whole process (mesh, assembly, setup, solve), user + system
  seconds at 102k edges and ω = 0.10: Jacobi 9.8 s, AMS before 100.6 s, AMS
  after 10.7 s with the default threading and 4.4 s against Jacobi's 9.9 s with
  `RAYON_NUM_THREADS=1`. The default-threading AMS figure is half system time
  (5.0 s user, 5.6 s system). The `main` AMS leg has the same 5.5 s and the
  Jacobi leg 0.2 s, and with one thread it is 0.2 s. It is attributed to the
  AMS setup, which still runs at the caller's thread count (#946); that was
  not measured separately. Single-threaded, the AMS process used 1.1× to 2.3×
  less CPU than Jacobi, more at larger sizes. These are single samples; no
  wall-clock speedup is claimed.
- **The answer is unchanged.** Every solve met the 1e-8 explicit-residual
  tolerance. The AMS field agrees with the Direct solution to 1.0e-11 to
  7.9e-10 (relative L2) over the 18 cells.
- **Setup cost of the estimate.** 37 ms at 102k edges, of a 0.51 s AMS setup
  (7%; `main`'s setup took 0.45 s). It is 30 sparse matrix-vector products and
  one pass over the matrix, sequential, and it is done once per build.
- **The estimate does not depend on the thread count.** With
  `RAYON_NUM_THREADS=1` the Ritz value, the weight, the iteration count, the
  residual and the port voltage are identical to the default-threading run to
  every printed digit at all six sizes.

**What is guaranteed and what is not.**

- `θ` is a lower bound on `λ_max` and `g` an upper bound. So `0.6 θ ≥ 2`
  *proves* that 0.6 is at or above the stability bound. The weight is lowered
  only in that case and is exactly 0.6 otherwise.
- A lowered weight is provably below `2/λ_max` when it is below `2/g`. That
  holds in all 18 cells here: 0.40 against `2/g` = 0.50. It does not hold for
  the anisotropic spiral below (0.398 against `2/g` = 0.331), where the
  weight rests on the Ritz value.
- `1.1` is a heuristic allowance for what 30 Lanczos steps have not resolved,
  not a bound. Where `1.1 θ < g` and the weight is not below `2/g`, it is below
  `2/λ_max` only if `λ_max < 1.47 θ`. The measured shortfall of `θ` against the
  1000-step reference is 0.03% to 0.55% on this fixture and at most 0.011% on
  the spiral.
- A weight kept at 0.6 is not thereby shown to be stable. The Ritz value only
  failed to prove it unstable. If a short run reports `θ` below 10/3 for an
  operator whose `λ_max` is above it, the weight stays at 0.6 and the solve
  behaves as it did before #945.

**Outside this fixture one solve changed** (`[[smoother_weight_validation]]`).
Every other AMS test but one keeps the weight at exactly 0.6 and prints the
same iteration counts on `main` and on the fix build:

| test | `λ_max` reference | `0.6 λ_max` | iterations, main | iterations, fix |
|---|---|---|---|---|
| spiral smoke, 14k edges, 1 / 5 / 10 / 20 GHz | 3.093 / 3.012 / 2.962 / 2.929 | 1.86 / 1.81 / 1.78 / 1.76 | 112 / 131 / 145 / 159 | same |
| spiral benchmark, 53k edges, 1 GHz | 3.068 | 1.84 | 114 | same |
| dispersive spirals (DS, Drude), rough spiral | same mesh as the smoke spiral | | 112 / 131 / 145 / 159, 229 / 202 / 232 / 235, 107 / 117 / 128 / 135 | same |
| **anisotropic spiral, 1 / 10 GHz** | **3.430** / 3.227 | **2.06** / 1.94 | **114** / 148 | **118** / 148 |
| PEC cube n = 4, public driven path | 3.186 | 1.91 | passes | passes |
| transmon synthetic n = 6, eigen inner CG | 3.327 | 1.996 | Jacobi 33 481, AMS 6 264 (5.34×); three-space 3 719 | same |

- **The anisotropic spiral at 1 GHz is the exception, and it costs 4
  iterations.** With a uniaxial substrate and an anisotropic μ on the
  dielectric, `λ_max` is 3.430 (the 30-step and 600-step values agree to seven
  digits), so 0.6 is above the bound and the weight becomes 0.398. `main`
  converges there in 114 iterations all the same, and the fix build takes 118.
  On the cube the old weight made the count grow with the mesh; here it did
  not. A likely reason is that only a few modes are above the bound on this
  mesh, but that was not examined. The lowered weight is not below the
  rigorous `2/g` here (`g` = 6.04).

- **This answers the #930 question about the spiral.** Its `λ_max` is 2.93 to
  3.09, so its bound is 0.65 to 0.68 and 0.6 was inside it. The structured
  Kuhn-tet cube is at 3.39 to 3.42. The mesh is the difference.
- **The Debye spiral test still fails on macOS, identically on both builds**
  (#943): 148 iterations, explicit residual 1.018e-10 against a 1e-10
  tolerance. Not changed here.
- **`real_transmon_ams_beats_jacobi` (133k DOFs) fails on both builds before
  its AMS leg runs**: the Jacobi eigensolve it runs first stops at
  ‖r‖/‖b‖ = 0.69 after 266 216 inner iterations (#960). So it gives no AMS
  number on either build. The AMS leg alone, from a throwaway test, was
  stopped by hand after 37 minutes without finishing. Its preconditioner
  build reported `θ` = 2.943 and kept the weight at 0.6, so on that mesh the
  AMS is the one `main` runs.
- **The transmon synthetic case is 0.2% inside the bound** (0.6 × 3.327 =
  1.996). It keeps 0.6 and its 5.34× gate, as on `main`. A slightly finer cube
  would cross 10/3 and take the lowered weight; what that does to this case is
  in the next paragraph.

**Why the weight is not lowered everywhere.** The rule first proposed for #945
was `w = min(0.6, 1.5/ρ̂)` with no proof condition. It was run on a throwaway
build (`smoother_945/unconditional_rule/`, the `unconditional_rule_*` keys of
`[[smoother_weight_validation]]`), single runs on this machine:

| test | main and fix (weight 0.6) | unconditional rule | its weight |
|---|---|---|---|
| spiral smoke | 112 / 131 / 145 / 159 | 116 / 142 / 153 / 170 | 0.441 to 0.466 |
| spiral benchmark | 114 | 126 | 0.444 |
| Drude spiral | 229 / 202 / 232 / 235 | 238 / 222 / 240 / 250 | |
| rough spiral | 107 / 117 / 128 / 135 | 109 / 124 / 130 / 139 | |
| DS spiral | 112 / 131 / 145 / 159, passes | **fails**: explicit residual 1.007e-10 at 10 GHz | |
| Debye spiral | **fails** (#943): 1.018e-10 at 5 GHz | 126 / 163 / 172 / 189, passes | |
| transmon synthetic, gradient-only | 6 264 (5.34× under Jacobi) | 6 771 (4.94×, **fails** the test's 5× gate) | 0.410 |
| transmon synthetic, three-space | 3 719 | 3 113 | 0.410 |

It lowers the spiral's weight although 0.6 is stable there, costs 2 to 20 more
iterations on every spiral solve that could be compared, makes one transmon
count worse and the other better, and moves which spiral solve lands within a
percent of the explicit-residual tolerance (#943; that pattern depends on
roundoff, so it will differ on Linux). The committed rule changes nothing
where 0.6 is not proven unstable.

Not measured: sizes above 102k edges; a benchmark host; Linux; the l1-Jacobi
alternative; and any operator on which 30 Lanczos steps leave `θ` far below
`λ_max`. `GEODE_AMS_SMOOTH_WEIGHT=<w>` overrides the weight (a warning, not an
error, when it is above both `2/ρ̂` and the automatic weight), and
`GEODE_AMS_SMOOTH_REPORT=1` prints the estimate per build.

The Jacobi iteration counts and port voltages at 25.7k, 59.7k and 102k edges
are identical to the Lambda record's, which is the only cross-host statement
this file supports. Do not put its timings beside the A100 or Palace cells.

## Caveats to read before quoting a number

These are summarized from `results_large_a100.toml`; read the file for the
full wording.

- **A100, not L40S.** #520 was written for the L40S of `results.toml`, but ran
  on an A100 with a different host CPU and a different matrix-free local
  apply. The file says it "is NOT comparable cell-for-cell with results.toml",
  and `[notes].not_comparable` says not to merge the cells: "results.toml
  remains the #501 L40S record."
- **The f32 GPU solve never converges to tolerance.** "The f32 GPU solve never
  meets the explicit-residual tolerance (5e-3..1.4e-2 vs the f64 legs'
  ~9e-9), and at 102k edges and above its recursion no longer converges at
  all." Where the recursion met the tolerance but the explicitly recomputed
  residual did not, the harness times the solve as completed and records
  `converged = false, recursion_converged = true`. So the per-iteration
  crossover (GPU cheaper above roughly 100k edges) exists but there is no
  crossover in time-to-solution: "the limiter is f32 convergence, not GPU
  throughput."
- **TF32.** On the A100 the stock autotuned f32 matmul ran as TF32 and the f32
  COCG stagnated already at n = 6. The matrix-free local apply was changed in
  PR #929 to avoid it (evidence in `tf32_evidence/`). Issue #926 tracks the
  remaining f32 matmul sites.
- **Jacobi vs AMS.** The head-to-head against Palace "is geode WITH JACOBI vs
  Palace with AMS", and "is NOT a measurement of geode's best preconditioner":
  the harness hard-codes COCG + Jacobi, the matrix-free / GPU path supports
  Jacobi only, and geode's assembled-path AMS preconditioner was not measured
  in that run. `results_ams_cpu_local.toml` has since measured it up to 102k
  edges on a developer machine (section above): the shipped AMS did not hold a
  flat iteration count on this fixture, and a diagnostic traced that to its
  edge-smoother weight. Issue #930 tracks the smoother follow-up, the at-scale
  measurement and the matrix-free / GPU AMS question.
- **Single-core CPU legs.** Both the CPU matrix-free leg and the assembled
  COCG leg ran single-threaded, so the per-iteration crossover is GPU vs one
  CPU core of the 30 available.
- **Single timed solves.** Most cells are one solve where the warm-up is the
  measurement (`GEODE_SCALING_REPS=0`); Palace timings are n = 3 only at
  59.7k and 463k edges.
- **Palace agreement.** Palace's port voltage agrees with geode's f64 solve to
  2.2e-9 to 2.8e-9 relative at every size, on both Palace devices.
