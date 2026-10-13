# Matched Meep AWS pilot

This is a resource and discretization pilot for #1088, not a solver cost-to-accuracy
comparison. It requests the full 192 × 183 × 131.6 mm physical-fixture domain.
Meep rounds cell extents to its lattice: the R2 log reports 192 × 183 × 131.5 mm
(384 × 366 × 263 cells). The JSON `estimated_cells` uses a ceiling for memory
planning; it is not the actual allocated grid count. This small boundary shift
is another discretization effect to include in future convergence checks.

The first pilot uses a dedicated r8g.8xlarge (32 Graviton4 cores, 256 GiB nominal
RAM), one OpenMP/BLAS thread per MPI rank, native arm64 Docker and the committed
explicit conda package list. Its 60-minute allocation cap includes bootstrap and
image construction. An OS shutdown is scheduled inside that deadline, in addition
to the worker tool's idle alarm. Containers have independent memory limits and
commands have wall-clock limits. Save results locally before stopping the worker.

## Reproduction

Use `scripts/aws/geode-worker.sh` as described in `scripts/aws/README.md`, with
profile `geode-fem-bench`, region `us-east-1`, and a dedicated worker. On the worker,
from the repository root (set `OUT` to an absolute writable results directory):

```sh
docker build -t meep-matched:locked \
  -f reference/meep/docker/Dockerfile.matched reference/meep/docker

docker run --rm --cpus 1 --memory 2g -v "$PWD:/work:ro" meep-matched:locked \
  python reference/meep/docker/matched_geometry_verify.py

for kind in volume sheet; do
  for r in 2 4 8; do
    extra=()
    if [ "$kind" = sheet ]; then extra+=(--rc-sheet); fi
    timeout 120 docker run --rm --cpus 2 --memory 2g \
      -v "$PWD:/work:ro" -v "$OUT:/results" meep-matched:locked \
      mpirun -np 2 python reference/meep/docker/matched_antenna.py \
      --rc-oracle "${extra[@]}" --resolution "$r" \
      --after-source 100 --check-interval 100 \
      --output "/results/rc_${kind}_R${r}.json"
  done
done

for n in 8 16 32; do
  timeout 240 docker run --rm --cpus "$n" --memory 200g --shm-size 2g \
    -v "$PWD:/work:ro" -v "$OUT:/results" meep-matched:locked \
    mpirun -np "$n" python reference/meep/docker/matched_probe.py \
    --config reference/meep/docker/matched_initial_2g4.json \
    --resolution 2 --steps 100 --output "/results/probe_R2_np${n}.json"
done
```

The calibration loop uses Bash arrays. The probe includes the same geometry,
complex fields, source, and 51-frequency DFT monitor as the production harness.
It measures initialization separately from 100 field-update steps. Such a short
startup sample does not establish steady-state speed; use it only to budget a
bounded production run. Rank peak RSS is summed and is not a simultaneous peak.
A second 32-rank probe uses `--steps 1000` and output
`probe_R2_np32_1000.json` to reduce the influence of first-step overhead.
No warmup samples are discarded. Geometry construction and Python/container
startup are outside the timed region, as in the production harness.

`timeout` bounds the Docker client; if a timeout fires, explicitly remove its
container before continuing or stopping the worker. The scheduled OS shutdown
is the independent upper bound on compute duration. Never leave an unattended
container to run until the idle alarm (a busy solver is not idle).

Verify locally with Python's standard library:

```sh
python3 benchmarks/fdtd_density_baseline/meep_pilot_verify.py
python3 -O benchmarks/fdtd_density_baseline/meep_pilot_verify.py
python3 benchmarks/fdtd_density_baseline/meep_matched_verify.py
```

The archived initial pilot and its RC records are immutable; `source_v1.py`
checks their original provenance. New spectra use the current harness hash.
The physical fixture is unchanged. Material-loss matching, localized port
refinement, boundary convergence, optimized geometry, and a same-host GEODE
reference remain separate requirements before any accuracy or cost ranking.

## Calibration and resource measurements (2026-10-13)

All 179 explicit package URLs were reproduced. The sheet-source and volume-source
RC spectra agree within numerical precision. Maximum relative impedance errors
at R2/R4/R8 are 4.1218e−4, 1.1847e−4, and 5.4871e−5; the volume spectra reproduce
the earlier Mac Docker records exactly. Geometry occupancy checks pass at
R1/R2/R4/R8. These tests isolate environment, source normalization and geometry
construction; they do not validate the antenna's localized spatial port model.

| MPI ranks | Initialization (s) | First 100 steps (s) | Sum of rank peak RSS (GiB) |
|---:|---:|---:|---:|
| 8 | 17.902 | 44.584 | 10.295 |
| 16 | 20.200 | 47.729 | 11.374 |
| 32 | 16.757 | 61.636 | 13.905 |

The separate 32-rank 1,000-step probe takes 16.717 s to initialize and 150.621 s
for stepping, with 13.934 GiB summed rank peak RSS. Much of the short sample is
first-step setup: the raw log reports about 52 s on the first step, then about
0.10 s/step. Differencing the two 32-rank samples gives
`(150.621 − 61.636) / 900 = 0.09887 s/step`. Including the fitted startup overhead
and initialization predicts about 1,822 s for 17,734 steps (t=4433.5).
This is a two-sample planning projection, not a runtime guarantee.

The production pilot uses 32 ranks, R2, `--after-source 1200`,
`--check-interval 600`, `--max-checks 4`, and tolerance 0.001. It can stop early
on the measured complex-S11 drift. The independent process timeout is 2,100 s:

```sh
timeout 2100 docker run --rm --name meep-production \
  --cpus 32 --memory 200g --shm-size 2g \
  -v "$PWD:/work:ro" -v "$OUT:/results" meep-matched:locked \
  mpirun -np 32 python reference/meep/docker/matched_antenna.py \
  --config reference/meep/docker/matched_initial_2g4.json \
  --resolution 2 --after-source 1200 --check-interval 600 --max-checks 4 \
  --output /results/initial_R2.json
```
