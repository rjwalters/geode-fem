# p=2 memory-anchor measurement attempt (issue #1265)

Date: 2026-11-05, host loom-worker-2 (16 vCPU shared, 61 GB RAM, ~52 GB
available at start, load ~16). Binary: `geode` release build, `ndarray`
backend, `geode eigen` on `sphere_pec_golden.json` with `element_order = 2`
and the mesh swapped. Measured with GNU `/usr/bin/time -v`.

| mesh | interior DOF | nnz(A) | model peak (p=1 anchor x nnz) | measured peak RSS | model wall | measured wall |
|---|---|---|---|---|---|---|
| `sphere.msh` | 19 132 | 946 176 | 4.36 GB | 0.798 GB (798 192 kB) | 26.1 s | 12.3 s |
| `sphere_fine.msh` | 184 948 | 8 613 980 | 39.7 GB | **no completed run** | 238 s | - |

Result: the one completed p=2 run has the model 5.5x HIGH on memory and 2.1x
HIGH on wall time, i.e. outside the 2x needed to upgrade
`peak_memory_confidence`. It is far below anchor scale (nnz ratio 0.046), where
the p=1 model is already documented to over-estimate 1.4-5x, so it neither
confirms nor refutes the p=2 slope.

`sphere_fine.msh` (nnz ratio 0.42) was started under `ulimit -v 36 GiB`
because the model predicts ~40 GB on a host shared by up to 12 sweeps. It
failed with `sparse LU: Generic(OutOfMemory)` after reaching 26.1 GB RSS
(26 094 904 kB) at 78.8 s. That is only a lower bound on the p=2 peak, not a
measurement, so **no p=2 anchor constant is recorded**: the p=1 anchor model
stays in force and flagged `order_of_magnitude`. A real anchor needs a
completed run on a box with headroom (e.g. the 128 GB cloud box used for the
p=1 anchor).
