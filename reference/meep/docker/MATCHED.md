# Physical bent-patch comparison: calibration and initial pilot

This is a **partial implementation of #1088**, targeting the physical 2.4 GHz
fixture selected in its operator comment. It supersedes the old smoke geometry
for future comparison work. No equal-accuracy solver ranking is established. Run metadata in
`benchmarks/fdtd_density_baseline/meep_runs/environment.json` also records
possible host contention; these are not controlled performance measurements.
The optimized geometry export from #1087, resolution study, and same-host GEODE
measurements are still required.

## Environment and reproduction

The recorded runs use native linux/aarch64 Meep 1.34.0, MPICH, Python 3.11, on an
Apple M3 Ultra with 96 GiB host RAM. Docker Desktop reports 24 CPUs and
67,300,700,160 bytes of memory. Each MPI rank uses one OpenMP/BLAS thread.
The locally built image is `meep-matched:mpi`, image ID
`sha256:d33a3da4e24e6fc0e54ff1e7b93217d0f02fc8408504a80e8054ebff1fc564a0`.
`matched-conda-linux-aarch64.txt` records every resolved package; recreate that
environment with `conda create -n meep --file matched-conda-linux-aarch64.txt`.
The Dockerfile pins the base digest and installs the explicit package export
directly, without solving for newer dependencies. This pins package versions and builds;
it does not promise a byte-identical Docker image filesystem.

Run from the repository root:

```sh
docker build --platform linux/arm64 -t meep-matched:mpi \
  -f reference/meep/docker/Dockerfile.matched reference/meep/docker

# Historical records use the archived v1 source so their provenance remains intact.
for r in 2 4 8; do
  docker run --rm --cpus 2 --memory 2g -v "$PWD:/work" meep-matched:mpi \
    mpirun -np 2 python benchmarks/fdtd_density_baseline/meep_runs/source_v1.py \
    --rc-oracle --resolution "$r" --after-source 100 --check-interval 100 \
    --output "benchmarks/fdtd_density_baseline/meep_runs/rc_R$r.json"
done

docker run --rm --cpus 4 --memory 12g -v "$PWD:/work" meep-matched:mpi \
  mpirun -np 4 python benchmarks/fdtd_density_baseline/meep_runs/source_v1.py \
  --config reference/meep/docker/matched_initial_2g4.json \
  --resolution 1 --after-source 1200 --check-interval 600 --max-checks 5 \
  --output benchmarks/fdtd_density_baseline/meep_runs/initial_R1.json
python3 benchmarks/fdtd_density_baseline/meep_matched_verify.py
docker run --rm --cpus 1 --memory 1g -v "$PWD:/work" meep-matched:mpi \
  python reference/meep/docker/matched_geometry_verify.py
```

Output includes the finite-duration DFT snapshots, complex impedance/S11,
source/current DFTs, configuration, script hash, MPI ranks, elapsed solve time,
and sum of rank peak RSS. Additional windows run until the maximum complex
S11 change is at most 0.001 or the requested check count is exhausted. A small
DFT change is a stopping diagnostic, not a certified error bound. Wall time includes simulation initialization, field
updates, and spectral extraction, but excludes container startup/imports and
geometry-object construction. Rank high-water marks are not simultaneous, so
the RSS sum is a conservative process-memory statistic, not Docker peak usage.

## Geometry and remaining model approximations

`matched_initial_2g4.json` records the relevant values and SHA-256 of
`reference/gmsh/patch_conformal_L0.yaml` at commit
`571feba337e3f8bc77393277d5018a7d5d6a56aa` in the #1084 worktree:
38 × 29 mm patch, 62 × 53 mm finite ground/substrate, 1.6 mm FR-4, bend radius
100 mm, 1.6 mm wide port at `(x,y)=(0,7.5)` mm. The old issue-body feed `(0,2)`
belongs to the superseded smoke fixture. The exact upstream YAML is captured
as `matched_initial_2g4.source.yaml` so provenance remains checkable before that
branch merges; the verifier checks its hash against the JSON. Translation by −h/2 puts the substrate
crown mid-plane at z=0 and the bend axis at z=−100 mm. The rectangular outer
box is 192 × 183 × 131.6 mm, with a 25 mm PML on every side.

The substrate, patch, and finite ground are ring-sector prisms. Patch/ground
are PEC, consistent with the GEODE PEC boundary treatment; the YAML's bulk
copper-conductivity value is not used. Metal shells extend one grid cell
**outward** from the substrate faces. Their inner radii stay at 100.8 and
99.2 mm while thickness decreases with refinement. Each arc has 256 straight
segments (maximum substrate chord sag below 0.08 µm). Dielectric subpixel
averaging is disabled in this first occupancy-grid experiment; a fair final
study should evaluate it and document the resulting metal treatment.

The finite port resistor has thickness one grid cell in x, conductivity
`eta0*h/(50*w*dx)`, and acts on both tangential components Ey and Ez as in the
GEODE surface-port term. It approaches a sheet as the grid refines. The Ez
current source lies on x=0 across the gap. Voltage is the volume-averaged gap
integral. Spatial source normalization is its exact geometric integral divided
by gap height, giving w for the antenna. Integrating interpolated `get_source`
values only over the monitor box loses boundary weights and is unsuitable for
this normalization.

The antenna current is the injected Norton current minus the 50 Ω shunt
current: `I = I_injected - eta0*V/50`, `Z = eta0*V/I`,
`S11 = (Z-50)/(Z+50)` in Meep field units. This extraction replaces the old
reflected-Ez proxy. The uniform RC oracle validates units, signs, and time/source
normalization. **It does not validate the localized port's spatial discretization**;
that needs refinement and cross-checking against GEODE.

Meep uses exp(−iωt); conjugate its S11 before comparing complex values to GEODE's
exp(+iωt). Constant conductivity reproduces FR-4 tanδ=0.02 only at 2.45 GHz;
tanδ varies as 1/f in this pilot, unlike GEODE's constant-loss-tangent model.
A causal dispersion fit or single-frequency material calibration remains
necessary for a strictly matched broadband comparison. PML parameters also
need their own convergence checks; equal physical box extents alone do not
establish equal boundary error.

## Independent port calibration

The periodic 2 × 2 × 2 mm homogeneous oracle has εr=4 and conductivity equivalent
to a 50 Ω shunt. A uniform current excites no curl fields. The exact unloaded
impedance is `Z = i*eta0/(2*pi*f*C)`, with natural capacitance
`C = epsilon*area/gap = 8 mm`. The recorded R2/R4/R8 runs test this independently
of the antenna geometry. `meep_matched_verify.py` recomputes the analytic answer
and checks maximum relative impedance error below 5e−4, decreasing with resolution,
DFT stability, S11 conversion, artifact provenance, and historical arithmetic.

Meep 1.34.0's Gaussian Fourier helper needs the dipole-to-current factor f/fc.
Its forward-difference source additionally contributes
`sinc(f*dt) exp(−i*pi*f*dt)`. Omitting the latter phase produced about 6% impedance
error in the R4 RC oracle; omitting proper spatial normalization was worse.
The five-width Gaussian cutoff leaves a small truncation floor. These are
version-specific corrections, so the harness is tied to the tested release.

Primary references:

- [Meep 1.34 source-time implementation](https://github.com/NanoComp/meep/blob/v1.34.0/src/meep.hpp)
  (`src_time::current`, forward-differenced dipole).
- [Meep 1.34 Gaussian transform and spatial source weights](https://github.com/NanoComp/meep/blob/v1.34.0/src/sources.cpp)
  (`gaussian_src_time::fourier_transform`, `src_vol_chunkloop`).
- [Meep Python interface](https://meep.readthedocs.io/en/latest/Python_User_Interface/)
  for prism extrusion, DFT arrays, and cubature weights. The latest interface
  differs from the pinned release's Fourier helper; do not remove the oracle.

## Resource scope

The physical outer box is about 28.7 times the old 64 × 60 × 42 mm smoke box.
R8 therefore has about **2.37 billion cells**, not 83 million. The script's
400 bytes/cell preflight is a planning estimate for complex fields, not a
measured scaling law; it refuses allocations estimated above 48 GiB by default.
The estimates for this full complex-field box are:

| Cells/mm | Estimated cells | Planning memory (GiB) |
|---:|---:|---:|
| 1 | 4,637,952 | 1.73 |
| 2 | 37,103,616 | 13.82 |
| 3 | 124,908,480 | 46.53 |
| 4 | 296,266,752 | 110.37 |
| 6 | 999,267,840 | 372.26 |
| 8 | 2,367,885,312 | 882.11 |

This formula omits fixed interpreter/MPI overhead and is not a memory bound.
R4/R6/R8 are refused under the default cap; R2/R3 have not been measured.
A real-field or symmetry implementation could reduce storage, but would need
its own source-normalization calibration and convergence measurements.
Container memory limits provide a separate hard boundary. Do not silently
shrink the domain or relabel a coarse run as converged to fill the old table.

The historical JSON retains its measured single-process timing observations,
corrects R250 to 2.52e12 cells and ~5,700× the R14 cell count, and withdraws the
arbitrary 1 µm accuracy equivalence. None of those projections proves an EM
accuracy requirement or the cost of an optimized broadband Meep workflow.

## Recorded pilot outcome

The initial R1 antenna run stopped at t=3833.5 after three 600-unit checks.
Maximum complex-S11 changes between successive windows were 0.011821, 0.00294652, 0.000941977.
The final change 0.000941977 meets the 0.001 stopping criterion.
Elapsed solve/measurement time was 309.89 seconds on four MPI ranks, with summed
rank peak RSS 1.837 GiB. This establishes only the recorded temporal stopping
diagnostic at R1. It does not establish spatial accuracy or a solver ranking.

The R1 minimum |S11| is 0.44149 (−7.10 dB) at the 2.70 GHz upper band edge.
At 2.45 GHz, Z = 4.323 − 12.030j Ω (Meep convention). There is no resolved
resonance minimum inside this sampled window at R1. Temporal stability alone
does not make this a spatially accurate antenna result.

## Follow-up calibration and AWS pilot

The current harness adds `--rc-oracle --rc-sheet`, with a source of size
`(0,2,2)` and integrated current 2 instead of the volume source current 4.
The cell-average Ampere identity gives the same analytic capacitor impedance
even though the sheet excites nonuniform fields. This exercises the delta
normalization used by the antenna. It still does not certify the localized
antenna port discretization. Historical records retain their exact generating
source in `meep_runs/source_v1.py`; their source hashes have not been rewritten.

See [AWS_PILOT.md](AWS_PILOT.md) for the follow-up calibration, measured
resource probes, production settings, and interpretation of the R2 pilot.
[AWS_R4.md](AWS_R4.md) records the R4 run, the R1/R2/R4 trend (no spatial
convergence: successive-grid differences did not shrink), the comparison with
GEODE's bent-fixture dip (GEODE ≥ 2.3566 GHz, inside the window; Meep's only deep
feature is at or below the 2.20 GHz edge; the two refine in opposite directions) and the
R8 requirement measured from R4 actuals. [DIAGNOSIS.md](DIAGNOSIS.md) traces that
disagreement to the harness's per-component voxelization of its one-cell metal shells
on the curved board, and recommends against R8 with the current harness.
