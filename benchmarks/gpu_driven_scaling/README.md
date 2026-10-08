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
| [`results_ams_cpu_local.toml`](results_ams_cpu_local.toml) | #930 | A developer Mac (Apple M3 Ultra) under heavy unrelated load. **Not a benchmark host**: iteration counts are the result, times are indicative only | n ∈ {6, 9, 12, 15, 20, 24}, 1 854 → 102 024 edges, CPU f64 only, no Palace |

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

Running the summarizer over the committed `runs/2026-10-08_local_ams/`
reproduces the committed file byte for byte. The AMS config is opt-in
(`GEODE_SCALING_CONFIGS=...,iterative_ams`); the default config set and its
output are unchanged.

## AMS vs Jacobi on this fixture (#930, local subset)

The file is the authority. Everything below is from one fixture at ω = 0.10 on
a loaded developer machine.

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
   spiral, the transmon AMS tests and this fixture.
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
