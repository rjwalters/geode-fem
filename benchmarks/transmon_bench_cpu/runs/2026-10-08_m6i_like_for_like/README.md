# 2026-10-08 m6i.4xlarge like-for-like run tree

Raw evidence for
[`../../results_like_for_like_m6i.toml`](../../results_like_for_like_m6i.toml)
(issues #927 and #763): geode's transmon eigensolve in five requests and Palace
in seven, on one AWS m6i.4xlarge in one session. The results file is generated
from this directory by
[`../../summarize_like_for_like_m6i.py`](../../summarize_like_for_like_m6i.py);
read it for the numbers and caveats.

## Layout

| Path | Contents |
|---|---|
| `launch.txt` | Hand-written: instance type, region, AMI name, launch and termination times |
| `provenance/host.txt`, `lscpu.txt`, `lscpu-e.txt`, `free-g.txt` | Host facts recorded on the box by [`m6i_box_setup.sh`](../../m6i_box_setup.sh) |
| `provenance/geode-version.txt` | geode commit, `rustc` and `cargo` versions, build command |
| `provenance/palace-image-provenance.txt`, `palace-image-id.txt` | Palace commit, compiler wrapper line, CMake flags, MPICH and OpenBLAS versions, and the id of the image that ran |
| `provenance/palace-image-id-1-broken.txt`, `palace-build-time.txt`, `palace-rebuild-time.txt` | The first image and the rebuild (see below) |
| `provenance/mesh-sha.txt`, `mesh-sha-check.txt`, `gmsh-version.txt` | Fixture hash check and the MSH 2.2 conversion Palace reads |
| `cells.tsv` | One row per cell: solver, gauge, shift, mode count, width, pinning, runs |
| `pinning.txt` | The CPU sets, and the affinity masks `taskset` and `mpirun -bind-to core` gave |
| `configs/` | The Palace config generated for each Palace run |
| `raw/<cell>_run<i>.log` | Solver output: the mode list and phase times |
| `raw/<cell>_run<i>.time` | `/usr/bin/time -v`: wall, user, sys, peak RSS |
| `raw/<cell>_run<i>.meta` | Environment, pinning, start and end times, load average, and the affinity mask and task count observed on the live process |
| `raw/<cell>_run<i>_eig.csv` | Palace cells: the eigenvalue table Palace wrote |
| `setup.out`, `bench.out`, `bench2.out` | Console output of the setup and of the two driver invocations |

## What happened in the session

1. `m6i_box_setup.sh` built geode at `61e8571e` and the Palace image from
   [`reference/palace/docker/Dockerfile`](../../../../reference/palace/docker/Dockerfile).
2. The first Palace image exited 127: its runtime stage lacked
   `libopenblas.so.0`. The Dockerfile gained `openblas-serial`, and the image was
   rebuilt from the cached builder stage (52 s). No timed run used the first
   image; its id is kept in `palace-image-id-1-broken.txt`.
3. `m6i_box_bench.sh` ran the sweep (`bench.out`). Before it, three geode cells
   and one Palace cell were run once into a separate directory to check the
   driver; those runs overlapped the Palace build and are not kept.
4. The driver was run a second time on the same output directory to add the two
   `*_nosave` Palace cells (`bench2.out`). It skips runs that already finished,
   so no earlier run was repeated or replaced.

## Edits made to the files after the run

- `/usr/bin/time` and the driver print the box's home directory; it is replaced
  by `<home>` in `raw/*.time`, `bench.out` and `bench2.out`. Nothing else in
  those files is changed.
- Not committed: the docker, cargo, apt and rustup build logs (about 3 MB of
  compiler output). The build outcome is in the `provenance/` files above.
- `launch.txt` is written by hand from the launch record. Instance and network
  identifiers are deliberately not recorded anywhere in this tree.
