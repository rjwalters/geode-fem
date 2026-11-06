# GEODE dataset sample schema (version 1)

`geode driven --dataset DIR` and `geode eigen --dataset DIR` (issue #1257,
Epic #1253 Phase 1) write **one gradient-labelled sample** of an EM solve
to `DIR`. The spec must carry a `sensitivity` section: the sample's labels
are the run's own gradients, so a spec without one is rejected
(`invalid_spec`) before solving. Wave-port and adaptive-sweep driven specs
are rejected too (no full-order field per row). Without the flag nothing
extra is written.

## Format

A [Zarr v2](https://zarr-specs.readthedocs.io/en/latest/v2/v2.0.html)
directory store with no compressor and **one chunk per array** (chunk =
whole array, key `0.0…0`): raw little-endian `<f8` / `<u4` / `<i4` bytes in C
order, described by each array's `.zarray`. Groups (`mesh`, `params`,
`field`, `obs`, `grad`) carry `.zgroup`; each array's `.zattrs` names its
dimensions (`_ARRAY_DIMENSIONS`); the root `.zattrs` holds the metadata
below. Zarr was chosen over HDF5 because it needs no C library (the FFI
policy gates new C dependencies on operator sign-off) and is written here
with `serde_json` alone. An existing `DIR` is reused: same-named files are
overwritten and nothing else is touched.

## Arrays

`kind` says which sample kinds carry the array. `reim` is a trailing
dimension of 2 holding `[re, im]`. A *sample* is a driven frequency row (in
the report's `results` order) or an eigenmode (in `modes` order).

| array | dtype | dims | kind |
|---|---|---|---|
| `mesh/nodes` | `<f8` | `node, xyz` | both |
| `mesh/tets` | `<u4` | `tet, vertex` | both |
| `mesh/tet_physical_tag` | `<i4` | `tet` | both |
| `params/values` | `<f8` | `param` | both |
| `params/bounds` | `<f8` | `param, bound` | both |
| `field/e_edges` | `<f8` | `sample, edge, reim` | both |
| `field/e_cell_centre` | `<f8` | `sample, tet, xyz, reim` | both |
| `obs/frequency_hz` | `<f8` | `sample` | both |
| `obs/s` | `<f8` | `sample, port, port, reim` | driven |
| `obs/q` | `<f8` | `sample` | eigen |
| `grad/entry_parameter` | `<u4` | `entry` | both |
| `grad/entry_sample` | `<u4` | `entry` | both |
| `grad/entry_observable` | `<i4` | `entry` | both |
| `grad/value` | `<f8` | `entry` | both |
| `grad/gradient` | `<f8` | `entry` | both |

### Semantics

* **Mesh.** `mesh/nodes` are in mesh length units (`length_unit_m` metres
  each); `mesh/tets` are 0-based node indices; `mesh/tet_physical_tag` is
  the per-tet volume physical tag (`0` = untagged), named by the root
  attribute `physical_groups`.
* **Parameters.** `params/values[i]` is the point the gradient was taken
  at and `params/bounds[i] = [lower, upper]` the parameter kind's
  physical domain (`±inf` = unbounded; the spec carries no bounds of its
  own). Names and kinds are in the root attribute `parameters`, in the same
  order.
* **Field.** `field/e_edges` are the edge DOFs in `mesh.edges()` order
  (each edge oriented low → high node index): driven, every port driven at
  its incident voltage, the same solve `--outdir` exports; eigen, the
  eigenvector scattered to all edges (PEC edges 0, **arbitrary sign**).
  `field/e_cell_centre` is the Whitney interpolant of those DOFs evaluated
  at each tet centroid (not the vertex average used for `.vtu` plots).
* **Observables.** `obs/frequency_hz` per sample; driven also
  `obs/s[sample, i, j]` (report channel order), eigen also `obs/q`
  (`NaN` where the report has `null`, e.g. a lossless mode).
* **Gradients.** One row per `sensitivities.entries[]` of the same run's
  report, copied unchanged (so they equal the `-o` report exactly):
  `grad/entry_parameter` indexes `parameters`; `grad/entry_sample` is the
  entry's first `index` (frequency row or mode); `grad/entry_observable`
  indexes the root attribute `observables` for the N-port driven path and
  is `-1` otherwise; `grad/value` and `grad/gradient` are `value` and
  `∂value/∂parameter`.

## Root attributes (`.zattrs`)

`format` = `"geode-dataset"`, `schema_version` (this document: `1`), `kind`
(`"driven"` / `"eigen"`), `provenance` (`geode_version`, `git_sha`,
`backend`, `spec_sha256`, `mesh_sha256`), `length_unit_m`,
`physical_groups` (`[{dim, tag, name}]`), `parameters` (the report's
`sensitivities.parameters`), `observable`, `observable_unit`,
`gradient_method`, `observables`.

The table above is mirrored by `ARRAYS` in
`crates/geode-cli/src/dataset.rs`; `tests/dataset_roundtrip.rs` and a unit
test fail when the writer and this table diverge. Any breaking change
bumps `schema_version`.

## Python loader

Documentation only (no Python dependency enters CI):

```python
import zarr  # pip install zarr (v2 store; zarr>=2.16, or zarr>=3 with zarr_format=2)
import numpy as np

s = zarr.open_group("sample", mode="r")
meta = dict(s.attrs)                       # format, kind, provenance, parameters, ...
nodes, tets = s["mesh/nodes"][:], s["mesh/tets"][:]
e_edges = s["field/e_edges"][..., 0] + 1j * s["field/e_edges"][..., 1]
e_cell = s["field/e_cell_centre"][..., 0] + 1j * s["field/e_cell_centre"][..., 1]
grad = s["grad/gradient"][:]               # one row per report sensitivity entry
param = s["grad/entry_parameter"][:]
```

Without `zarr`, each array is its single chunk file (`0`, `0.0`, `0.0.0`, … by rank), e.g. `np.fromfile("field/e_edges/0.0.0", dtype="<f8")`,
reshaped to its `.zarray` `shape`.
