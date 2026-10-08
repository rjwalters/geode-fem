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
| `runs/2026-10-08_local_ams/` | The evidence tree behind `results_ams_cpu_local.toml`: the legs `n<N>_{direct,jacobi,ams,xcheck}` and `diagnostics/` (two one-off patched builds, described in its `NOTE.txt`) |

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

Running the summarizer over the committed `runs/2026-10-08_local_ams/`
reproduces the committed file byte for byte. The AMS config is opt-in
(`GEODE_SCALING_CONFIGS=...,iterative_ams`); the default config set and its
output are unchanged.

## AMS vs Jacobi on this fixture (#930, local subset)

The file is the authority. In short, on this fixture at ω = 0.10:

| edges | COCG + Jacobi iterations | COCG + AMS iterations |
|---|---|---|
| 1 854 | 807 | 70 |
| 5 859 | 1 463 | 139 |
| 13 428 | 2 167 | 307 |
| 25 695 | 2 969 | 533 |
| 59 660 | 4 303 | 1 741 |
| 102 024 | 5 259 | 2 209 |

- **AMS converges, and to the right answer.** Every AMS solve met the 1e-8
  tolerance on the explicitly recomputed residual, and its field agrees with
  the Direct solution to 1.3e-11 to 6.0e-11 (relative L2).
- **Its iteration count is not flat.** It grows about as edges^0.86, faster
  than Jacobi's edges^0.47, so the advantage in iterations shrinks from 11.5×
  at 1.9k edges to 2.4× at 102k. This is unlike the spiral measurement that
  motivated AMS (102 to 155 iterations from 14k to 53k edges).
- **It was slower than Jacobi at every size here.** One AMS iteration cost 17×
  to 78× a Jacobi iteration. These are single samples from a loaded developer
  machine and are indicative only. At the three largest sizes the AMS solve
  took 5× to 33× as long as the Jacobi solve, well outside the load noise; at
  the three smallest it took 1.7× to 2.8× as long, which is not clearly
  outside it.
- **The cause is not established.** Two one-off diagnostics
  (`runs/2026-10-08_local_ams/diagnostics/`) show the growth does not come from
  the fixture's two disconnected PEC planes, and does not come only from the
  inexact vector-nodal coarse solve.
- **Not done:** the sizes above 102k edges, the same-host Palace comparison,
  the decision on the default preconditioner (still Jacobi), and the
  matrix-free / GPU AMS follow-up. Issue #930 stays open for these.

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
  edges on a developer machine (section above): AMS did not hold a flat
  iteration count on this fixture. Issue #930 tracks the at-scale measurement
  and the matrix-free / GPU AMS question.
- **Single-core CPU legs.** Both the CPU matrix-free leg and the assembled
  COCG leg ran single-threaded, so the per-iteration crossover is GPU vs one
  CPU core of the 30 available.
- **Single timed solves.** Most cells are one solve where the warm-up is the
  measurement (`GEODE_SCALING_REPS=0`); Palace timings are n = 3 only at
  59.7k and 463k edges.
- **Palace agreement.** Palace's port voltage agrees with geode's f64 solve to
  2.2e-9 to 2.8e-9 relative at every size, on both Palace devices.
