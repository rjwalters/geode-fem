# geode-validation

The cross-backend reference comparison harness (Epic #88). The library loads a
fixture, which is one canonical (input, golden output) pair, and compares it
field by field against GEODE-FEM's output, with a per-field absolute
tolerance. A failed comparison writes a structured JSON diff artifact that
names which field disagreed and by how much. The integration tests run the
solver on small problems and check the results against golden fixtures
produced by independent implementations in NumPy, JAX, Julia, ONNX and
TF-Java. Those reference implementations and their fixtures live under
[`reference/`](../../reference/README.md).

| Path | Contents |
|---|---|
| [`src/lib.rs`](src/lib.rs) | `Fixture`, `FixtureFormat`, `compare_against`, `ComparisonReport`; re-exports the repo-path helpers from `geode-util` |
| [`src/diff.rs`](src/diff.rs) | Per-field comparison and the diff artifact |
| [`src/fixture.rs`](src/fixture.rs) | Fixture loading (JSON) |
| [`tests/`](tests/) | Reference tests: P1 and Nédélec local matrices, cube cavity, de Rham operators, sphere PEC / PML / Mie, Mie roots and efficiencies, each against one or more reference languages |

Run the suite, or one target, with the `ndarray` CPU backend as CI does:

```sh
cargo test --release -p geode-validation
cargo test --release -p geode-validation --test cube_cavity_numpy_reference -- --nocapture
```
