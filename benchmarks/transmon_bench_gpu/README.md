# benchmarks/transmon_bench_gpu/

Transmon eigenmode GPU head-to-head for issue #519 (Epic #476, PR #925):
Palace built with CUDA + libCEED and run on the GPU, the same Palace image run
on the CPU, and geode CPU f64, all on the identical sha-pinned transmon fixture
(`crates/geode-core/tests/fixtures/transmon_smoke.msh`, 6 eigenmodes at a
4.5 GHz target, Order 1), on one rented host in one session (2026-10-07). It is
the performance companion to
[`../transmon_bench_cpu/`](../transmon_bench_cpu/). It is a performance record,
not a CI gate.

[`results.toml`](results.toml) is the authoritative record: every number and
caveat lives there. This README only explains what is in the directory.

## Hardware

Lambda Cloud `gpu_1x_a100_sxm4`: one NVIDIA A100-SXM4-40GB, AMD EPYC 7J13 with
30 vCPU, 216 GiB RAM. See `[hardware]` and `[software]` in `results.toml` for
the driver, OS, Palace commit, image and build flags.

## Layout

| Path | Contents |
|---|---|
| `results.toml` | The committed result: metadata, the eigenvalue agreement gate, one `[cells.*]` table per cell (n = 3 runs each), and `[notes]` |
| `box_setup.sh` | Run once on a fresh GPU box: builds the `palace:cuda` image from [`reference/palace/docker/Dockerfile.cuda`](../../reference/palace/docker/Dockerfile.cuda) and geode's `transmon_bench` example, converts the mesh to MSH 2.2, and writes `provenance/` |
| `box_bench.sh` | Runs the cells (`CELLS`, `RUNS`, `CPU_RANKS`) under `/usr/bin/time -v` with an `nvidia-smi` memory poll, and writes `raw/` |
| `summarize.py` | Reads `raw/` and a baseline `eig.csv`, prints TOML fragments (per-cell tables and `[eigen_agreement]`) to stdout. For every run it also prints the modes returned, a class per mode and the number of physical modes (#927) |
| `runs/2026-10-07_lambda_a100/` | The evidence tree from the measured session (below) |

Inside `runs/2026-10-07_lambda_a100/`:

| Path | Contents |
|---|---|
| `setup.out`, `bench.out`, `bench_cpu.out`, `bench.start` | Console output of the setup and benchmark invocations, and the benchmark start timestamp |
| `provenance/` | What `box_setup.sh` recorded: host and GPU info, mesh sha256 checks, gmsh and Rust toolchain logs, the geode build log and commit, and the Palace image id, build flags and docker build log |
| `provenance/failed-build-1/` … `failed-build-3/` | Docker build logs and timings of three Palace CUDA builds that exited non-zero; the `build_fixes` list in `results.toml` describes what `Dockerfile.cuda` changed to get a working image |
| `provenance/aborted-build-4/` | The docker build log of a fourth build that was aborted |
| `configs/` | The Palace config generated for each run (`cfg_<cell>_run<i>.json`), plus the debug-smoke config |
| `raw/` | Per-run `.log`, `.time` (`/usr/bin/time -v`), `.gpumem` (Palace cells) and `_eig.csv` (Palace cells), with `runs.csv` (one row per run), `backend.txt` (libCEED backend and device lines from the Palace logs), `gpu_idle.txt`, `missing.txt` and the `BENCH_DONE` marker. This is the input to `summarize.py` |
| `raw/failed_cpu_libcuda/` | The first attempt at the Palace CPU control cells, run without `--gpus all`. The CUDA build's binary links `libcuda.so.1` even for `Device = CPU`, so every run exited 127 before `main` (`runs_failed.csv`). The six "no eig.csv" lines in `raw/missing.txt` are from these runs. The cells were re-run with `--gpus all` |
| `debug_smoke/` | One single-rank Palace GPU run of the fixture config (`Eigenmode.Save = 6`) made before the benchmark: its config, log and CSV outputs. This is the run that showed Palace's ParaView writer dominating the wall clock and motivated the `Save = 0` (`*_nosave`) cells |

## Regenerating

On a GPU box with the NVIDIA driver, docker and nvidia-container-toolkit, copy
the four inputs named at the top of `box_setup.sh` into `$WORK`, then:

```sh
GEODE_REF=<commit> CUDA_ARCH=80 ./box_setup.sh      # CUDA_ARCH = the GPU's sm (80 = A100; the default 89 = L40S)
CELLS="palace_gpu_nosave palace_cpu_nosave geode palace_cpu palace_gpu" RUNS=3 CPU_RANKS=8 ./box_bench.sh
```

Copy the outputs (`$WORK/raw`, `$WORK/provenance` and the generated
`cfg_*.json` files) back into a `runs/<date>_<host>/` directory, then from the
repo root, pointing at that directory's `raw/`:

```sh
python3 -I benchmarks/transmon_bench_gpu/summarize.py \
  benchmarks/transmon_bench_gpu/runs/2026-10-07_lambda_a100/raw \
  reference/fixtures/transmon_palace/results_p1/eig.csv
```

`summarize.py` prints TOML fragments; it does not write `results.toml`. The
numeric fields in `results.toml` are hand-transcribed (rounded) from that
output, and the metadata, caveats and notes are added by hand.

## Caveats to read before quoting a number

These are summarized from `results.toml`; read the file for the full wording.

- **A100, not L40S.** #519 was written for an AWS g6e (NVIDIA L40S). AWS had no
  g6e capacity, so this ran on an A100. The A100 has far more FP64 throughput
  than the L40S and Palace's GPU path is FP64, so `results.toml` says to read
  the Palace-GPU numbers "as optimistic relative to an L40S, not as an L40S
  measurement". The CPU cells are comparable only to each other, not to
  `transmon_bench_cpu` or `gpu_driven_scaling`.
- **The geode and Palace outputs are not like-for-like.** In every geode run
  the six returned pairs are four gradient near-kernel modes, the known
  spurious 3.4528 GHz mode (#514) and 5.152825 GHz, so geode recovers 1 of
  Palace's 6 physical modes while Palace returns all six. In the file's words,
  "'geode is faster' here is a statement about wall clock for different
  outputs, not for an equivalent answer", and this run applies no geode
  eigen-agreement gate. Every cell records `modes_f_ghz`, `modes_class` and
  `n_physical`; the geode cells carry `like_for_like = false` (#927).
- **No cell here supports "geode is faster than Palace at the transmon
  eigenmode".** Re-timing geode in a configuration that returns Palace's six
  modes, beside Palace on this host, is not done (it needs the paid Lambda
  host). A geode-only measurement on a loaded developer machine is in
  [`../transmon_bench_cpu/results_like_for_like_local.toml`](../transmon_bench_cpu/results_like_for_like_local.toml):
  it records which requests return the six modes and what they cost relative
  to the 6-mode request on that same machine. Its times are a different host
  and must not be compared with this directory's. The Palace-GPU vs Palace-CPU
  result does not depend on this. Paper impact: #763, #593.
- **Palace on the GPU is slower than Palace on 8 CPU ranks here** (72.2 s vs
  51.0 s with `Save = 0`). The `HONEST READ` block in `results.toml` breaks
  down where the gap goes.
- **The `Save = 6` (fixture) cells are dominated by Palace's ParaView writer**
  in this CUDA image, not by solver time. Use the `*_nosave` cells for the
  solver comparison.
- **`GEODE_NUM_THREADS=1` is not a single-threaded measurement.** The cap
  applies to faer's sparse LU only; neither geode cell is thread-count-matched
  with Palace.
- **There is no geode-CUDA cell.** geode has no GPU eigensolve path. The
  geode-CUDA vs Palace-GPU comparison that exists is the driven solve in
  [`../gpu_driven_scaling/`](../gpu_driven_scaling/) (#520).
- **The correctness gate is Palace-GPU vs the committed CPU Palace run**
  (`[eigen_agreement]`: PASS, to ~1e-12 relative).
