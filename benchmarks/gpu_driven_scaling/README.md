# benchmarks/gpu_driven_scaling/

GPU-vs-CPU wall-clock scaling of the lumped-port driven solve on one physical
fixture (the σ-lossy parallel-plate cube with a single lumped port), refined
via `cube_tet_mesh(n)`. Both files are performance records produced by the
`--ignored` release-tier test
[`crates/geode-core/tests/gpu_driven_scaling.rs`](../../crates/geode-core/tests/gpu_driven_scaling.rs).

There are two separate measurements here, on different hardware. They are not
comparable cell-for-cell and must not be merged.

| File | Issue | Hardware | Sizes |
|---|---|---|---|
| [`results.toml`](results.toml) | #501 | AWS g6e.xlarge: NVIDIA L40S, AMD EPYC 7R13 with 4 vCPU | n ∈ {6, 9, 12, 15}, 1 854 → 25 695 edges |
| [`results_large_a100.toml`](results_large_a100.toml) | #520, PR #929 | Lambda Cloud `gpu_1x_a100_sxm4`: NVIDIA A100-SXM4-40GB, AMD EPYC 7J13 with 30 vCPU | 25 695 → 462 520 edges, plus Palace driven runs on the same meshes |

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
  Jacobi only, and geode's assembled-path AMS preconditioner was not measured.
  Issue #930 tracks the AMS-at-scale measurement and the matrix-free / GPU AMS
  question.
- **Single-core CPU legs.** Both the CPU matrix-free leg and the assembled
  COCG leg ran single-threaded, so the per-iteration crossover is GPU vs one
  CPU core of the 30 available.
- **Single timed solves.** Most cells are one solve where the warm-up is the
  measurement (`GEODE_SCALING_REPS=0`); Palace timings are n = 3 only at
  59.7k and 463k edges.
- **Palace agreement.** Palace's port voltage agrees with geode's f64 solve to
  2.2e-9 to 2.8e-9 relative at every size, on both Palace devices.
