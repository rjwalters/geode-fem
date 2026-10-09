# 2026-10-08 m6i.4xlarge tuned-Palace run tree

Raw evidence for
[`../../results_tuned_palace_m6i.toml`](../../results_tuned_palace_m6i.toml)
(issue #927). Palace was built from
[`reference/palace/docker/Dockerfile.tuned`](../../../../reference/palace/docker/Dockerfile.tuned)
(libxsmm, `-march=icelake-server`, OpenBLAS 0.3.29 built for the host with
threading off, Release). It was timed on one AWS m6i.4xlarge together with geode
at the `main` commit of the day. The cells are the fixture-request cells of the
untuned PR #964 run
([`../2026-10-08_m6i_like_for_like/`](../2026-10-08_m6i_like_for_like/README.md)).
To separate the effect of the build from the effect of the instance, the
unchanged untuned recipe
[`reference/palace/docker/Dockerfile`](../../../../reference/palace/docker/Dockerfile)
was then built and timed on the same box in the same session (`untuned_control/`).
The results file is generated from this directory and the PR #964 directory by
[`../../summarize_tuned_palace_m6i.py`](../../summarize_tuned_palace_m6i.py).
Read the results file for the numbers and caveats.

## Layout

| Path | Contents |
|---|---|
| `launch.txt` | Hand-written: instance type, region, AMI name, launch and termination times |
| `provenance/host.txt`, `lscpu.txt`, `lscpu-e.txt`, `free-g.txt` | Host facts recorded on the box by [`m6i_tuned_box_setup.sh`](../../m6i_tuned_box_setup.sh) |
| `provenance/geode-version.txt` | geode commit, `rustc` and `cargo` versions, build command |
| `provenance/palace-image-provenance.txt`, `palace-image-id.txt`, `palace-build-time.txt` | Palace commit, `-march`, compiler flags, OpenBLAS build, CMake options, MPICH version, image id, build wall time |
| `provenance/palace-image-libs.txt` | The BLAS, LAPACK, libCEED and libxsmm libraries that `ldd` resolves for the installed Palace binary and libraries. Also the OpenBLAS build config and zmm instruction counts. |
| `provenance/palace-image-isa.txt` | Per library, the count of instructions that use `ymm` registers (AVX) and the count that use EVEX-only registers or opmask registers (AVX-512). Written by `isa_check.sh` (below). |
| `provenance/mesh-sha.txt`, `mesh-sha-check.txt`, `gmsh-version.txt` | Fixture hash check and the MSH 2.2 conversion that Palace reads |
| `isa_check.sh` | The script behind `palace-image-isa.txt`, run in the image after the build (not part of the setup script) |
| `cells.tsv` | One row per cell: solver, gauge, shift, mode count, width, pinning, runs, Palace `Linear.Type` |
| `pinning.txt` | The CPU sets, and the affinity masks that `taskset` and `mpirun -bind-to core` gave |
| `configs/` | The Palace config generated for each Palace run |
| `raw/<cell>_run<i>.log` | Solver output: the mode list and phase times |
| `raw/<cell>_run<i>.time` | `/usr/bin/time -v`: wall, user, sys, peak RSS |
| `raw/<cell>_run<i>.meta` | Environment, pinning, image, `Linear.Type`, start and end times, load average, and the affinity mask and task count observed on the live process |
| `raw/<cell>_run<i>_eig.csv` | Palace cells: the eigenvalue table Palace wrote |
| `setup.out`, `bench.out` | Console output of the setup and of the driver |
| `untuned_control/` | The same-box untuned control, laid out like this directory: `cells.tsv`, `pinning.txt`, `configs/`, `raw/`, `bench.out`, and `provenance/` (image provenance, build time and `palace-image-isa.txt` of the untuned image) |
| `untuned_control/untuned_control_build.sh`, `untuned_control_chain.sh` | The two scripts that built the untuned image on the box and then ran the driver with `IMAGE=palace:cpu GEODE_RUNS=0` and a `CELL_FILTER` for the four fixture-request Palace cells. They were written on the box during the session, after the tuned sweep |

## What happened in the session

1. `m6i_tuned_box_setup.sh` built geode at `8367ee7b` (`main` HEAD when the run
   started) and the `palace:tuned` image, in parallel. The image built on the
   first attempt.
2. `isa_check.sh` was run once against the image to record its AVX and AVX-512
   instruction use. At `-march=icelake-server`, GCC prefers 256-bit vectors, so
   a zmm count alone would read as no AVX-512.
3. `m6i_tuned_box_bench.sh` ran the sweep once (`bench.out`), round-robin, with
   no runs before it and no re-runs.
4. Tuned Palace came out 5 to 11% faster than the untuned PR #964 numbers. In
   the same comparison geode, which did not change build, came out 6 to 9%
   faster than in PR #964. So the instance difference alone could explain the
   Palace difference. To separate the two, the untuned recipe was built on the
   same box (`untuned_control_build.sh`, 32 min) and its four fixture-request
   cells were timed with the same driver and pinning (`untuned_control_chain.sh`
   -> `untuned_control/bench.out`). This step was not in the plan; it was added
   when the cross-session numbers came out this close. The control image's
   `isa_check.sh` run reports `find: '/opt/openblas/lib': No such file or
   directory`, because that image uses the distribution OpenBLAS under
   `/usr/lib64`. Its count is not in the table.

## Edits made to the files after the run

- `/usr/bin/time` and the driver print the box's home directory. It is replaced
  by `<home>` in `raw/*.time`, `bench.out` and the same files under
  `untuned_control/`. Nothing else in those files is
  changed.
- Not committed: the docker, cargo, apt and rustup build logs (several MB of
  compiler output). The build outcome is in the `provenance/` files above.
- `launch.txt` is written by hand from the launch record. Instance and network
  identifiers are deliberately not recorded anywhere in this tree.
