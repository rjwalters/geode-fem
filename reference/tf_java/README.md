# TF-Java reference implementations

L4-shaped reference backend per **Epic #88**. Adds static
typechecking + IDE tooling against an L4-shaped object graph
(no other backend exposes the graph as a first-class typed value).

## Status

- **`cube_cavity/`** — Maven project for the scalar-Helmholtz
  cube-cavity assembly (Epic #88 / #93). Uses TF-Java's static-graph
  API (`Graph` + `Session` + `Ops`); explicitly *not* eager mode per
  the #88 framing comment ("the point of including TF-Java is the
  explicit symbolic-graph surface").
- Assembly closes inside the graph; the eigensolve falls out to SciPy
  via the JSON sidecar produced by `CubeCavityMain` (see
  `reference/driver/eigensolve_from_tfjava.py`). This is the
  documented "TF-Java cannot natively close the spine" L4 friction
  artifact (TF-Java has no native sparse generalized eigensolver).

## Build + run

Requires JDK 17+ and Maven. On macOS Apple Silicon:

```sh
cd reference/tf_java/cube_cavity
mvn -Pmacos-arm64 package

# Run the assembly + sidecar dump
mvn -Pmacos-arm64 exec:java \
    -Dexec.mainClass=dev.geodefem.refcubecavity.CubeCavityMain \
    -Dexec.args="--n 4 --out target/reduced_kM.json"

# Close the eigenproblem at the SciPy boundary
python3 ../../driver/eigensolve_from_tfjava.py \
    target/reduced_kM.json \
    --out target/eigenresult.json
```

On Linux x86_64 CI, replace `-Pmacos-arm64` with `-Plinux-x86_64`.

## What the static-graph build actually exercises

The `AssemblyGraph` class constructs a TF-Java `Graph` with:

- a `Placeholder<TFloat64>` for node coordinates (shape `[nNodes, 3]`),
- a baked-in `Constant<TInt32>` for tet connectivity (shape `[nElem, 4]`),
- per-element edge-vector subtractions, hand-rolled per-row cross
  products (TF-Java has no `cross` op, so this is implemented as
  six `mul` + three `sub`s + a `stack`),
- per-element local matrices `K_local`, `M_local` (shape `[nElem, 4, 4]`),
- a `ScatterNd` into a zero buffer of shape `[nNodes, nNodes]` for
  both K and M, using a `[nElem * 16, 2]` index table.

This is the L4 lowering on the TF-Java side. Comparing it to the
JAX `cube_cavity.py` lowering validates the XLA-shaped IR target on
both sides (per Epic #88 framing: JAX + TF-Java together validate
the L4 → XLA lowering; disagreement isolates DX-surface from
compiler-semantics).

## Cross-XLA agreement table

Once both JAX and TF-Java run on the same problem, the comparison
table goes here. The harness path is:

1. `reference/numpy/cube_cavity_minimal.py` → NumPy baseline.
2. `reference/jax/gen_cube_cavity_fixture.py` →
   `reference/fixtures/cube_cavity/jax_baseline.json` (JAX baseline).
3. `mvn exec:java` + `python3 .../eigensolve_from_tfjava.py` →
   TF-Java path eigenvalues.
4. `crates/geode-core/tests/cube_cavity_jax_reference.rs` → Burn
   path eigenvalues vs JAX baseline.

A side-by-side table goes in this README after the first CI run of
the TF-Java path lands. For now, the *expected* table is:

| Backend  | dtype     | trace(K_int) | λ[0]        | tol (rel)  |
|----------|-----------|--------------|-------------|------------|
| NumPy    | f64       | 40.5         | 37.4992105  | 0 (anchor) |
| JAX      | f64 (CPU) | 40.5         | 37.4992105  | <1e-13     |
| TF-Java  | f64 (CPU) | 40.5         | (expected)  | <1e-10     |
| Burn     | f32       | 40.499994    | 37.4991835  | <1e-5      |

(JAX vs NumPy was measured at `1e-13` relative on `n=4` during
fixture generation; see `gen_cube_cavity_fixture.py` output. Burn
vs JAX is measured by
`crates/geode-core/tests/cube_cavity_jax_reference.rs` at `~7e-7`
relative for `n=4`. TF-Java number pending first build.)

## CI gating

TF-Java pulls ~200 MB of native libs and requires JDK 17+, so it is
gated behind a slow, optional CI job rather than the default
`cargo test` run. The fast path (JAX vs Burn) is exercised by
`cargo test -p geode-core --release --test cube_cavity_jax_reference`
and does not require any JVM infrastructure.
