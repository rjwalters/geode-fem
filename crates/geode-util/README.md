# geode-util

A staging layer that sits above [`geode-core`](../geode-core/README.md). It
holds helpers that several consumers would otherwise each re-implement: the
validation tests, the benchmark binaries in [`examples/`](../../examples/) and
the `geode` CLI. These helpers are not core FEM kernels. The dependency runs one
way only: `geode-util` depends on `geode-core`, `burn` and `faer`, and
`geode-core` uses it only as a dev-dependency. The crate was introduced by
Epic #414 to collect helpers moved out of `geode-validation` and the example
crates.

| Module | Contents |
|---|---|
| [`compare`](src/compare.rs) | Comparison and assertion helpers for the reference-test suite |
| [`convert`](src/convert.rs) | Format conversions |
| [`eigen`](src/eigen.rs) | Dense lowest-eigenpair helpers with degenerate-cluster handling; `k` and `Q` from an eigenvalue `λ` |
| [`fixture`](src/fixture.rs) | Fixture adaptors and the JSON fixture loader and schema |
| [`interop`](src/interop.rs) | Decoders for reference outputs from other languages |
| [`math`](src/math.rs) | Small matrix utilities: Frobenius norm, symmetry residual, sparse nnz count |
| [`repo`](src/repo.rs) | Repo-relative paths and git provenance (`repo_root`, `current_commit`, `fixture_path`) |
| [`units`](src/units.rs) | Unit conversions for the EM stack |
| [`viz`](src/viz.rs) | Visualization glue |

The crate does not choose a Burn backend. The consuming crate selects one
through `geode-core`'s backend features.
