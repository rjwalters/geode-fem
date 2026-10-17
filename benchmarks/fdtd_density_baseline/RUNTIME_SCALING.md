# Historical Meep timing observations

These measurements describe the **old unmatched smoke scaffold**, measured on
2026-07-21 with Meep 1.34.0 on AWS `m6i.4xlarge` (16 vCPU, 61 GB).
The model has a flat FR-4 slab, infinite flat PEC ground, rectangular patch
proxy and Ez readout. It is not the physical bent patch or a validated S11
comparison. Machine-readable observations and labelled projections are in
`meep_runtime_scaling.json`; the historical probe is `meep_runtime_bench.py`.

## Measured per-step cost

Single-tenant seconds/timestep over 61 steps after warmup:

| R (cells/mm) | Yee cells | s/step | peak RAM (GiB) |
|---|---:|---:|---:|
| 4 | 10.3 M | 0.233 | 1.573 |
| 6 | 34.8 M | 0.742 | 4.918 |
| 8 | 82.6 M | 1.715 | 11.338 |
| 10 | 161.3 M | 3.386 | 21.836 |
| 12 | 278.7 M | 5.761 | 37.417 |

The historical JSON reports fitted powers of about R^2.92 for per-step time
and R^2.89 for RAM. Cells are exactly `161280*R^3` for this 64×60×42 mm box.
The measured time step decreases as 1/R. If a fixed physical duration is
required, this suggests roughly R^4 total runtime for this implementation;
it does not determine the resolution or duration required for an S11 tolerance.
A separate historical R8 run did not reach field decay in its 15-minute window.
That incomplete run is not a converged broadband optimization benchmark.

## Arithmetic and limits of extrapolation

The earlier R250 cell count was wrong. The corrected illustrative count is
`64*60*42*250^3 = 2.52e12`, about `(250/14)^3 = 5694` times the R14 cell count.
The historical R14/R16 memory estimates (~60/~83 GB in the JSON) are
extrapolations, not measured points. No required EM accuracy is assigned to
R250. The arbitrary 1 µm geometric target has been withdrawn.

The earlier optimization-hour estimates multiplied separate solves by the
frequency count without measuring a broadband/adjoint workflow. They have been
withdrawn from the JSON. Geometric perimeter bias and per-step timing alone do
not establish whether FDTD can solve or optimize this antenna at useful accuracy.

## Physical-antenna comparison

The current [geometry diagnostic](staircasing_notes.md) uses R=100 mm, h=1.6 mm
and the actual L0/L1 surfaces. Its displacement proxy and facet errors do not
supply a conversion to S11 accuracy. Linear FEM facets also have finite error.
The old 73-DOF optimization used a different model and is not an equal-accuracy
cost reference for these historical Meep timings.

The [matched Meep experiment](../../reference/meep/docker/MATCHED.md) records a
calibrated impedance extraction and a physical R1 pilot. Its 192×183×131.6 mm
box is about 28.7 times larger than the historical box. Spatial, material, port
and boundary convergence, plus controlled same-host timing, remain required
under #1085/#1088 before the paper can claim a solver cost advantage.
