# `reference/palace/docker/` — reproducible Palace oracle builds

In-repo build recipes so the Palace reference solver can be rebuilt **from
geode-fem alone** — a prerequisite for the honest Palace head-to-head in epic #476.

| File | Backend | Used for |
|------|---------|----------|
| `Dockerfile`      | CPU (`/cpu/self/opt/blocked` as built: `PALACE_WITH_LIBXSMM=OFF`) | Rebuilding the CPU reference; the m6i like-for-like timing (#927). The committed baseline in `reference/fixtures/transmon_palace/results_p1/` came from a source build of the same Palace commit on an EC2 box |
| `Dockerfile.cuda` | CUDA (`/gpu/cuda/*`)           | Palace-on-GPU head-to-head (issue #519) |

Both compile Palace from source (MFEM + hypre + SLEPc/PETSc + libCEED + SuperLU +
STRUMPACK + MUMPS, plus MAGMA in the CUDA build). First build is long
(~15 min on 48 vCPU, ~1–2 h on 4 vCPU).

## CPU build

```sh
cd reference/palace/docker
docker build -t palace:cpu -f Dockerfile .
docker run --rm -v "$PWD/../../..:/work" palace:cpu \
  /work/reference/fixtures/transmon_palace/palace_config.json
```

The CPU recipe pins Palace with `--build-arg PALACE_REF=<commit>` (default
`fba6a5b`, the changeset of the committed baseline and of `Dockerfile.cuda`),
writes the commit, compiler-wrapper line and CMake flags to
`/opt/palace/palace-build-info.txt` inside the image, and installs `time` so a
run can be measured from inside the container
(`--entrypoint /usr/bin/time`). It was first built and run end to end for the
2026-10-08 m6i re-timing (issue #927), which found the runtime stage missing
`libopenblas.so.0`; `openblas-serial` is now installed there. That build took
34 min on 16 vCPU. The MPICH compiler wrappers of Rocky 9 add
`-O2 -flto=auto -march=x86-64-v2` to every target, so the image is not tuned
for the build host. Run record:
[`benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like/`](../../../benchmarks/transmon_bench_cpu/runs/2026-10-08_m6i_like_for_like/README.md).

## CUDA (GPU) build — issue #519

Requires an NVIDIA GPU host with the `nvidia-container-toolkit` (the AWS Deep
Learning Base OSS Nvidia Driver AMI ships it). Target GPU for #519 is the **L40S**
(Ada, `sm_89`) on a `g6e` instance.

```sh
cd reference/palace/docker
docker build -t palace:cuda -f Dockerfile.cuda .          # CUDA_ARCH defaults to 89 (L40S)
# For a different GPU: --build-arg CUDA_ARCH=90  (H100), etc.

# Sanity-check the GPU is visible inside a container first:
docker run --rm --gpus all nvidia/cuda:12.6.3-base-rockylinux9 nvidia-smi
```

### Running the transmon eigenmode fixture on GPU

The full operator workflow (mesh SHA, MSH-2.2 conversion, ingest) lives in
`reference/fixtures/transmon_palace/palace_config.provenance.txt`. GPU-specific
steps layered on top:

1. Convert the fixture to MSH 2.2 (MFEM rejects the MSH 4.1 multi-block file):
   ```sh
   gmsh transmon_smoke.msh -0 -save -format msh2 -o transmon_smoke_v22.msh
   ```
2. Point the config's `Model.Mesh` at the converted mesh and set the solver to
   GPU. Palace selects the device from the config:
   ```json
   "Solver": { "Device": "GPU", "Order": 1 }
   ```
   The libCEED backend can also be forced via env, e.g. `CEED_BACKEND=/gpu/cuda/gen`
   (or `/gpu/cuda/magma`). **Record whichever backend string the run log prints**
   into the fixture provenance (#519 AC).
3. Run with the GPU exposed and capture wall / RSS / GPU-mem (n≥3):
   ```sh
   docker run --rm --gpus all -v "$PWD:/work" palace:cuda /work/palace_config.json
   ```
4. Palace writes `postpro/transmon_p1/eig.csv`. Diff the mode frequencies against
   the committed CPU baseline (`results_p1/eig.csv`) — they must agree
   (correctness preserved across CPU→GPU); the win we are measuring is wall-clock,
   not eigenvalues.

## Provenance to capture for #519

- Exact CUDA base tag + driver version (`nvidia-smi`), Palace git SHA (the
  `--depth 1` clone HEAD), and the full cmake flag line (already in the Dockerfile).
- The libCEED backend string from the run log (`/gpu/cuda/...`).
- Wall / peak-RSS / peak-GPU-mem (n≥3) committed to a results artifact under
  [`benchmarks/transmon_bench_gpu/`](../../../benchmarks/transmon_bench_gpu/)
  (`results.toml`, with the raw logs and build provenance under `runs/`).

## Note on the `g6e` / geode-CUDA f32 caveat

geode's CUDA path is **f32-only** (burn-cuda 0.21 disables f64 — see #534). When
the `palace_gpu` benchmark cell compares against geode-CUDA, keep that caveat
visible in the table; Palace GPU here runs f64.
