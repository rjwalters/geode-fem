# Cross-backend conformance matrix — Epic #88 close-out

Status roll-up for [Epic #88](https://github.com/rjwalters/geode-fem/issues/88)
(cross-validated L4 lowerings). All ten phases (A–J) are merged as of
2026-06-09; this document is the epic's win-condition artifact
([#184](https://github.com/rjwalters/geode-fem/issues/184)).

> **Win condition (from #88):** a documented L4 surface where every operator
> on the spine has an equivalent-semantics reference implementation in at
> least 3 of {NumPy, JAX, TF-Java, Julia, ONNX}, with each cross-backend
> disagreement explicitly catalogued, and Burn's realization re-anchored
> against the reference set.

**Verdict: MET, WITH CAVEATS.** All eleven spine-operator rows reach the
≥3-backend bar (per-row tally below). The caveats, stated precisely:

1. **De Rham (row 8) and the analytic Mie catalogue (row 10) sit exactly at
   three backends**, with ONNX audit-verdict coverage as the third — the de
   Rham verdict covers *application only* (construction is host-side by the
   input-boundary rule), and the Mie root-finding probe was executed at
   single-channel extent (TE l=1) rather than the full 40-root catalogue.
2. **The eigensolve row (row 9) is met as an opaque-node contract, not a
   traced one.** No backend traces the generalized eigensolve — all six exit
   to a host solver at the same boundary. Among the five reference backends,
   the SciPy/LAPACK lineage is shared by NumPy, JAX, and TF-Java; the
   genuinely independent solver pairs are LAPACK ZGGEV vs Burn's faer QZ
   (agreeing at ~1e-13 on the small-mesh tensor pencil) and SciPy `eigsh`
   vs Arpack.jl vs Burn's Lanczos/ARPACK-FFI on the real slices.
3. **TF-Java is sidecar-bounded at the eigensolve on every slice** — the
   typed static graph assembles `(K_int, M_int)` but delegates the
   eigensolve to SciPy through the sidecar seam
   ([`driver/eigensolve_from_sidecar.py`](driver/eigensolve_from_sidecar.py)).
4. **ONNX coverage on the PML and Mie slices is audit-verdict** (executed,
   numerically validated probes under pinned `onnx==1.21.0` /
   `onnxruntime==1.26.0` / opset 18), not committed end-to-end baselines.
   The cube-cavity and sphere-PEC slices *do* have executed ONNX assembly
   graphs with committed fixtures and Rust cross-checks.

## Cell legend

| Token | Meaning |
|---|---|
| **F** | Full reference — independent end-to-end implementation, committed fixture, Rust cross-check |
| **F\*** | Full assembly reference, **sidecar-bounded** eigensolve (SciPy seam) |
| **G** | Audit-verdict **graph-pure** — executed ONNX probe/graph, numerically validated |
| **H** | Audit-verdict **host-side** — documented as outside the graph (input-boundary rule / out-of-graph eigensolve) |
| **S** | Shared host implementation — reuses the NumPy module at the host boundary (not independent) |
| **A** | Burn production realization, **anchored to the reference set** (see [Burn re-anchoring](#burn-re-anchoring-statement)) |
| — | No implementation |

## The matrix

Rows are spine operators; columns are backends. Burn is the anchored
production target and does not count toward the ≥3 bar (the bar is over
{NumPy, JAX, TF-Java, Julia, ONNX}).

| # | Spine operator | NumPy | JAX | TF-Java | Julia | ONNX | Burn | ≥3 bar |
|---|---|---|---|---|---|---|---|---|
| 1 | Mesh I/O (Gmsh `.msh` → node/element tables) | F | S | F | F | H | A | **3** ✓ |
| 2 | P1 local kernels (stiffness + mass) | F | F | F | F | G | A | **5** ✓ |
| 3 | Nédélec local curl-curl + mass | F | F | F | F | G | A | **5** ✓ |
| 4 | Scalar (isotropic) PML construction (complex-ε ramp) | F | F | F\* | F | G | A | **5** ✓ |
| 5 | Tensor-ε (anisotropic UPML) construction | F | F | F\* | F | G | A | **5** ✓ |
| 6 | Global assembly / gather-scatter | F | F | F | F | G | A | **5** ✓ |
| 7 | Dirichlet / PEC reduction | F | F | F | F | G | A | **5** ✓ |
| 8 | Discrete de Rham d⁰ / d¹ / d² | F | — | — | F | G/H | A | **3** ✓ (exact) |
| 9 | Generalized eigensolve boundary | F | S | F\* | F | H | A | **3** ✓ (opaque-node) |
| 10 | Analytic Mie root catalogue | F | — | — | F | G | A | **3** ✓ (exact) |
| 11 | Mode classification / selection | F | F | S | F | H | A | **3** ✓ |

Per-row evidence (implementing file, fixture, Rust cross-check, source PR)
follows. All agreement figures are quoted from the merged PR descriptions
and committed baselines; none are new measurements.

### Row 1 — Mesh I/O

- **NumPy** — [`numpy/mesh.py`](numpy/mesh.py) (meshio-based `.msh` load +
  programmatic cube builders; [PR #98](https://github.com/rjwalters/geode-fem/pull/98),
  [PR #108](https://github.com/rjwalters/geode-fem/pull/108)).
- **JAX** — shared: reuses the NumPy mesh module at the host boundary (mesh
  I/O is outside the traced graph by design; see e.g.
  [`jax/sphere_pec.py`](jax/sphere_pec.py)).
- **TF-Java** — [`tf_java/sphere_pec/src/main/java/dev/geodefem/refspherepec/SphereMesh.java`](tf_java/sphere_pec/src/main/java/dev/geodefem/refspherepec/SphereMesh.java)
  (independent JVM MSH4 parser; [PR #137](https://github.com/rjwalters/geode-fem/pull/137)) +
  [`tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/CubeMesh.java`](tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/CubeMesh.java).
- **Julia** — [`julia/mesh.jl`](julia/mesh.jl) (inline MSH 4.1 parser;
  [PR #121](https://github.com/rjwalters/geode-fem/pull/121)).
- **ONNX** — host-side by the **input-boundary rule**: mesh and topology
  tables are graph inputs
  ([`onnx/audit/sphere_pec/nedelec_operator_audit.md`](onnx/audit/sphere_pec/nedelec_operator_audit.md),
  [`onnx/audit/derham/derham_operator_audit.md`](onnx/audit/derham/derham_operator_audit.md)).
- **Burn** — `crates/geode-core/src/mesh/` (mshio;
  [PR #15](https://github.com/rjwalters/geode-fem/pull/15)).
- Shared fixtures: [`fixtures/cube_cavity/unit_cube.msh`](fixtures/cube_cavity/unit_cube.msh),
  [`fixtures/sphere_pec/sphere.msh`](fixtures/sphere_pec/sphere.msh),
  [`fixtures/sphere_pml_small/sphere.msh`](fixtures/sphere_pml_small/sphere.msh).

### Row 2 — P1 local kernels

- **NumPy** — [`numpy/p1_local_matrices.py`](numpy/p1_local_matrices.py);
  fixtures [`fixtures/p1_local/`](fixtures/p1_local/) (5 per-case);
  test `crates/geode-validation/tests/p1_local_numpy_reference.rs`
  ([PR #96](https://github.com/rjwalters/geode-fem/pull/96)).
- **JAX** — [`jax/cube_cavity.py`](jax/cube_cavity.py) (`vmap`-batched local
  kernels); fixture [`fixtures/cube_cavity/jax_baseline.json`](fixtures/cube_cavity/jax_baseline.json);
  test `cube_cavity_jax_reference.rs`
  ([PR #97](https://github.com/rjwalters/geode-fem/pull/97)).
- **TF-Java** — [`tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/AssemblyGraph.java`](tf_java/cube_cavity/src/main/java/dev/geodefem/refcubecavity/AssemblyGraph.java);
  gated by the three-way CI compare in
  [`.github/workflows/tfjava-cube-cavity.yml`](../.github/workflows/tfjava-cube-cavity.yml)
  ([PR #97](https://github.com/rjwalters/geode-fem/pull/97),
  [PR #107](https://github.com/rjwalters/geode-fem/pull/107)).
- **Julia** — [`julia/cube_cavity.jl`](julia/cube_cavity.jl); fixture
  [`fixtures/cube_cavity/julia_baseline.json`](fixtures/cube_cavity/julia_baseline.json);
  test `cube_cavity_julia_reference.rs`
  ([PR #121](https://github.com/rjwalters/geode-fem/pull/121)).
- **ONNX** — graph-pure: probe [`onnx/audit/probe_p1_local.py`](onnx/audit/probe_p1_local.py)
  plus the executed F.2 assembly graph
  [`onnx/cube_cavity/assembly_graph.py`](onnx/cube_cavity/assembly_graph.py);
  fixture [`fixtures/cube_cavity/onnx_baseline.json`](fixtures/cube_cavity/onnx_baseline.json);
  test `cube_cavity_onnx_reference.rs`
  ([PR #119](https://github.com/rjwalters/geode-fem/pull/119),
  [PR #125](https://github.com/rjwalters/geode-fem/pull/125)).
- **Burn** — `crates/geode-core/src/p1.rs`.
- Acceptance anchor (all backends): lowest 5 cube-cavity modes vs the
  analytic {3, 6, 6, 6, 9}·π² spectrum, with the cluster-closure window
  (friction artifact 2).

### Row 3 — Nédélec local curl-curl + mass

- **NumPy** — [`numpy/nedelec_local_matrices.py`](numpy/nedelec_local_matrices.py);
  fixtures [`fixtures/nedelec_local/`](fixtures/nedelec_local/);
  test `nedelec_local_numpy_reference.rs`
  ([PR #120](https://github.com/rjwalters/geode-fem/pull/120)).
- **JAX** — [`jax/sphere_pec.py`](jax/sphere_pec.py); fixture
  [`fixtures/sphere_pec/jax_baseline.json`](fixtures/sphere_pec/jax_baseline.json);
  test `sphere_pec_jax_reference.rs`
  ([PR #131](https://github.com/rjwalters/geode-fem/pull/131)).
- **TF-Java** — [`tf_java/sphere_pec/src/main/java/dev/geodefem/refspherepec/NedelecAssemblyGraph.java`](tf_java/sphere_pec/src/main/java/dev/geodefem/refspherepec/NedelecAssemblyGraph.java);
  fixture [`fixtures/sphere_pec/tfjava_sidecar.json`](fixtures/sphere_pec/tfjava_sidecar.json);
  test `sphere_pec_tfjava_reference.rs`
  ([PR #137](https://github.com/rjwalters/geode-fem/pull/137)).
- **Julia** — [`julia/sphere_pec.jl`](julia/sphere_pec.jl); fixture
  [`fixtures/sphere_pec/julia_baseline.json`](fixtures/sphere_pec/julia_baseline.json);
  test `sphere_pec_julia_reference.rs`
  ([PR #132](https://github.com/rjwalters/geode-fem/pull/132)).
- **ONNX** — graph-pure: probe
  [`onnx/audit/sphere_pec/probe_nedelec_local.py`](onnx/audit/sphere_pec/probe_nedelec_local.py)
  plus the executed G.7 partial assembly graph
  [`onnx/sphere_pec/assembly_graph.py`](onnx/sphere_pec/assembly_graph.py)
  (host-computed topology inputs); fixture
  [`fixtures/sphere_pec/onnx_sidecar.json`](fixtures/sphere_pec/onnx_sidecar.json);
  test `sphere_pec_onnx_reference.rs`
  ([PR #138](https://github.com/rjwalters/geode-fem/pull/138),
  [PR #142](https://github.com/rjwalters/geode-fem/pull/142)).
- **Burn** — `crates/geode-core/src/nedelec.rs`.

### Row 4 — Scalar (isotropic) PML construction

- **NumPy** — [`numpy/sphere_pml.py`](numpy/sphere_pml.py); fixtures
  [`fixtures/sphere_pml/baseline.json`](fixtures/sphere_pml/baseline.json) +
  small-mesh sibling [`fixtures/sphere_pml_small/baseline.json`](fixtures/sphere_pml_small/baseline.json);
  tests `sphere_pml_numpy_reference.rs`
  ([PR #155](https://github.com/rjwalters/geode-fem/pull/155),
  [PR #164](https://github.com/rjwalters/geode-fem/pull/164)).
- **JAX** — [`jax/sphere_pml.py`](jax/sphere_pml.py) (c128 autodiff through
  complex assembly, **zero custom VJPs** — friction artifact 7); fixture
  [`fixtures/sphere_pml/jax_baseline.json`](fixtures/sphere_pml/jax_baseline.json);
  test `sphere_pml_jax_reference.rs`; drift gate
  [`.github/workflows/jax-sphere-pml.yml`](../.github/workflows/jax-sphere-pml.yml)
  ([PR #154](https://github.com/rjwalters/geode-fem/pull/154),
  [PR #165](https://github.com/rjwalters/geode-fem/pull/165)).
- **TF-Java** — [`tf_java/sphere_pml/src/main/java/dev/geodefem/refspherepml/ComplexNedelecAssemblyGraph.java`](tf_java/sphere_pml/src/main/java/dev/geodefem/refspherepml/ComplexNedelecAssemblyGraph.java)
  (paired-real f64 graph, sidecar eigensolve); fixture
  [`fixtures/sphere_pml/tfjava_baseline.json`](fixtures/sphere_pml/tfjava_baseline.json);
  test `sphere_pml_tfjava_reference.rs`
  ([PR #163](https://github.com/rjwalters/geode-fem/pull/163)).
- **Julia** — [`julia/sphere_pml.jl`](julia/sphere_pml.jl) +
  [`julia/sphere_pml_small.jl`](julia/sphere_pml_small.jl) (native ComplexF64;
  dense-LAPACK small-mesh tiebreaker after the Arpack windowed-selection
  divergence, friction artifact 6); fixtures
  [`fixtures/sphere_pml/julia_baseline.json`](fixtures/sphere_pml/julia_baseline.json),
  [`fixtures/sphere_pml/julia_small_baseline.json`](fixtures/sphere_pml/julia_small_baseline.json);
  tests `sphere_pml_julia_reference.rs`, `sphere_pml_julia_small_reference.rs`
  ([PR #153](https://github.com/rjwalters/geode-fem/pull/153),
  [PR #167](https://github.com/rjwalters/geode-fem/pull/167)).
- **ONNX** — audit-verdict graph-pure via the sanctioned **paired-real
  lowering** (c128 is vestigial in opset 18 — friction artifact 8):
  [`onnx/audit/sphere_pml/probe_complex_eps_ramp.py`](onnx/audit/sphere_pml/probe_complex_eps_ramp.py),
  [`onnx/audit/sphere_pml/probe_complex_local_scatter.py`](onnx/audit/sphere_pml/probe_complex_local_scatter.py),
  audit [`onnx/audit/sphere_pml/nedelec_pml_operator_audit.md`](onnx/audit/sphere_pml/nedelec_pml_operator_audit.md)
  ([PR #162](https://github.com/rjwalters/geode-fem/pull/162)).
- **Burn** — complex assembly path in `crates/geode-core/src/nedelec_assembly.rs`
  (f64-pair split kernels) + `complex_eigen.rs` / `complex_lanczos.rs`.

### Row 5 — Tensor-ε (anisotropic UPML) construction

- **NumPy** — [`numpy/sphere_mie.py`](numpy/sphere_mie.py) (line-for-line
  mirror of `build_anisotropic_pml_tensor_diag` + per-axis cofactor-gram
  mass); fixtures [`fixtures/sphere_mie/baseline.json`](fixtures/sphere_mie/baseline.json)
  (full mesh) + [`fixtures/sphere_mie_small/baseline.json`](fixtures/sphere_mie_small/baseline.json);
  test `sphere_mie_numpy_reference.rs`. Full-mesh Burn-vs-NumPy physical band
  max |Δλ| = 8.2e-7; TM₁,₁ at 5.69 % of the analytic anchor (full) / 6.59 %
  (small) vs k = 1.30343
  ([PR #179](https://github.com/rjwalters/geode-fem/pull/179)).
- **JAX** — [`jax/sphere_mie.py`](jax/sphere_mie.py) (BCOO[complex128]
  scatter — friction artifact 12; tensor-ε autodiff probe clean, zero custom
  VJPs); fixture [`fixtures/sphere_mie_small/jax_baseline.json`](fixtures/sphere_mie_small/jax_baseline.json);
  test `sphere_mie_jax_reference.rs`; drift gate
  [`.github/workflows/jax-sphere-mie.yml`](../.github/workflows/jax-sphere-mie.yml).
  JAX-vs-NumPy: tensor bit-exact (|Δ| = 0), eigenvalues max |Δ| = 1.9e-13
  ([PR #180](https://github.com/rjwalters/geode-fem/pull/180)).
- **TF-Java** — [`tf_java/sphere_mie/src/main/java/dev/geodefem/refspheremie/ComplexNedelecAssemblyGraph.java`](tf_java/sphere_mie/src/main/java/dev/geodefem/refspheremie/ComplexNedelecAssemblyGraph.java)
  (per-axis tensor kernel inside the typed graph — friction artifact 13;
  sidecar eigensolve); fixture
  [`fixtures/sphere_mie_small/tfjava_baseline.json`](fixtures/sphere_mie_small/tfjava_baseline.json);
  test `sphere_mie_tfjava_reference.rs`; live JVM path CI-gated at 1e-4 in
  [`.github/workflows/tfjava-cube-cavity.yml`](../.github/workflows/tfjava-cube-cavity.yml)
  ([PR #183](https://github.com/rjwalters/geode-fem/pull/183)).
- **Julia** — [`julia/sphere_mie_small.jl`](julia/sphere_mie_small.jl);
  fixture [`fixtures/sphere_mie_small/julia_baseline.json`](fixtures/sphere_mie_small/julia_baseline.json);
  test `sphere_mie_julia_small_reference.rs`. Julia-vs-NumPy eigensolve
  max |Δλ| = 1.0e-13 ([PR #181](https://github.com/rjwalters/geode-fem/pull/181)).
- **ONNX** — audit-verdict graph-pure (paired-real, composing cleanly with
  the diagonal-tensor structure; ramp / local blocks / assembled global all
  bit-exact): [`onnx/audit/sphere_mie/probe_tensor_eps_ramp.py`](onnx/audit/sphere_mie/probe_tensor_eps_ramp.py),
  audit [`onnx/audit/sphere_mie/mie_operator_audit.md`](onnx/audit/sphere_mie/mie_operator_audit.md)
  ([PR #182](https://github.com/rjwalters/geode-fem/pull/182)).
- **Burn** — `build_anisotropic_pml_tensor_diag` +
  `batched_nedelec_local_mass_anisotropic_diag` in
  `crates/geode-core/src/nedelec_assembly.rs`
  ([PR #60](https://github.com/rjwalters/geode-fem/pull/60)).

### Row 6 — Global assembly / gather-scatter

Exercised end-to-end by every slice above; per-backend lowering:

- **NumPy** — COO triplets → `scipy.sparse` CSR ([`numpy/cube_cavity.py`](numpy/cube_cavity.py)).
- **JAX** — `jax.experimental.sparse` BCOO + `sum_duplicates` (c128-clean
  per [PR #180](https://github.com/rjwalters/geode-fem/pull/180)) / segment-sum.
- **TF-Java** — `scatterNd` in the typed static graph (drift vs COO→CSR
  bounded at ~1.2e-5 relative by the Phase G.5 measurement,
  [PR #137](https://github.com/rjwalters/geode-fem/pull/137)).
- **Julia** — direct 0-based row-sorted CSR construction
  ([`julia/derham.jl`](julia/derham.jl)) / `SparseArrays`.
- **ONNX** — static-shape `ScatterND(reduction="add")`, executed in the F.2
  and G.7 graphs ([`onnx/cube_cavity/assembly_graph.py`](onnx/cube_cavity/assembly_graph.py),
  [`onnx/sphere_pec/assembly_graph.py`](onnx/sphere_pec/assembly_graph.py),
  probe [`onnx/audit/probe_assembly_scatter.py`](onnx/audit/probe_assembly_scatter.py)).
- **Burn** — `crates/geode-core/src/assembly.rs`, `nedelec_assembly.rs`, and
  the named L4 operator surface `fe_assemble.rs`
  ([PR #139](https://github.com/rjwalters/geode-fem/pull/139),
  [PR #143](https://github.com/rjwalters/geode-fem/pull/143)).

### Row 7 — Dirichlet / PEC reduction

- **NumPy / JAX / Julia** — boundary masks + interior submatrix extraction
  in each slice driver (e.g. [`numpy/sphere_pec.py`](numpy/sphere_pec.py),
  [`jax/sphere_pec.py`](jax/sphere_pec.py), [`julia/sphere_pec.jl`](julia/sphere_pec.jl)).
- **TF-Java** — interior reduction inside the typed graph (sidecar emits
  `K_int`, `M_int`).
- **ONNX** — graph-pure `Gather`-based interior extraction:
  [`onnx/audit/probe_dirichlet_mask.py`](onnx/audit/probe_dirichlet_mask.py),
  [`onnx/audit/sphere_pec/probe_pec_mask.py`](onnx/audit/sphere_pec/probe_pec_mask.py),
  executed in the F.2/G.7 graphs.
- **Burn** — `apply_dirichlet_bc` in `crates/geode-core/src/assembly.rs`.

### Row 8 — Discrete de Rham d⁰ / d¹ / d² (exactly at the bar)

- **NumPy** — [`numpy/derham.py`](numpy/derham.py); fixture
  [`fixtures/derham/baseline.json`](fixtures/derham/baseline.json);
  test `derham_numpy_reference.rs`
  ([PR #152](https://github.com/rjwalters/geode-fem/pull/152)).
- **Julia** — [`julia/derham.jl`](julia/derham.jl); fixture
  [`fixtures/derham/julia_baseline.json`](fixtures/derham/julia_baseline.json);
  test `derham_julia_reference.rs`. **Bit-exact** integer CSR equality vs
  NumPy and Burn on the bundled sphere (774 nodes / 4512 edges / 7074 faces /
  3335 tets; ranks 773 / 3739 / 3335)
  ([PR #176](https://github.com/rjwalters/geode-fem/pull/176)).
- **ONNX** — split verdict ([PR #178](https://github.com/rjwalters/geode-fem/pull/178)):
  *construction* host-side (all four topology constructors blocked by
  data-dependent-shape dedup + hash-map inverse lookup — the input-boundary
  rule generalized); *application* graph-pure and **bit-exact** vs scipy in
  int64 and f64, with `d¹·(d⁰·φ) ≡ 0` asserted **in-graph** on the integer
  channel (f64 control residual 8.9e-16 — roundoff, not ONNX):
  [`onnx/audit/derham/probe_d0_apply.py`](onnx/audit/derham/probe_d0_apply.py),
  [`onnx/audit/derham/probe_d1_apply.py`](onnx/audit/derham/probe_d1_apply.py),
  [`onnx/audit/derham/probe_exactness_in_graph.py`](onnx/audit/derham/probe_exactness_in_graph.py).
- **JAX / TF-Java** — no implementation (the row meets the bar without them).
- **Burn** — `crates/geode-core/src/derham.rs` (Epic #57).

### Row 9 — Generalized eigensolve boundary (opaque-node contract)

The empirical Phase A–J finding: **the eigensolve is uniformly out-of-graph
in all six backends** — every implementation exits to a host solver at the
same boundary. This row is therefore met as an opaque operator node
(`solve_family`-shaped), not a traced body.

- **NumPy** — SciPy `eigsh` shift-invert (real slices) + dense LAPACK ZGGEV
  (complex slices).
- **JAX** — same SciPy/LAPACK seam at the documented host boundary (shared
  lineage, not independent).
- **TF-Java** — sidecar-bounded: no JVM sparse complex generalized
  eigensolver exists; `(K_int, Re(M), Im(M))` cross the sidecar seam to
  SciPy ([`driver/eigensolve_from_sidecar.py`](driver/eigensolve_from_sidecar.py)).
- **Julia** — Arpack.jl shift-invert (real; calling-convention divergence vs
  SciPy = friction artifact 4) + dense `LinearAlgebra.eigen` (LAPACK ZGGEV)
  on lossy complex pencils (Arpack windowed selection unreliable there —
  friction artifact 6).
- **ONNX** — host-side; no eigensolver in the graph (every audit).
- **Burn** — faer dense QZ (`eigen.rs`), shift-invert Lanczos (`lanczos.rs`,
  `complex_lanczos.rs`), ARPACK FFI (`arpack.rs`,
  [PR #73](https://github.com/rjwalters/geode-fem/pull/73)).

Independent solver lineages cross-checked: SciPy/LAPACK vs Arpack.jl vs
Burn's faer QZ + Lanczos/ARPACK-FFI; LAPACK-vs-faer agreement on the
small-mesh tensor pencil at ~1e-13–5.6e-5 depending on band (see
[`fixtures/sphere_mie_small/baseline.schema.md`](fixtures/sphere_mie_small/baseline.schema.md)).

### Row 10 — Analytic Mie root catalogue (exactly at the bar)

- **NumPy** — [`numpy/mie_roots.py`](numpy/mie_roots.py)
  (`scipy.special.spherical_jn/yn` + `brentq`); fixture
  [`fixtures/mie_roots/baseline.json`](fixtures/mie_roots/baseline.json);
  test `mie_roots_numpy_reference.rs`. All 40 roots (l = 1..4, n = 1..5,
  TE+TM) agree with Burn at worst-case **3.4e-13 relative**
  ([PR #177](https://github.com/rjwalters/geode-fem/pull/177)).
- **Julia** — [`julia/mie_roots.jl`](julia/mie_roots.jl)
  (SpecialFunctions.jl half-order Bessel — a third, independent Bessel
  lineage vs scipy and Burn's hand-rolled ladder); fixture
  [`fixtures/mie_roots/julia_baseline.json`](fixtures/mie_roots/julia_baseline.json);
  test `mie_roots_julia_reference.rs`. All 40 roots agree with the SciPy
  catalogue at worst **1.527e-15 relative**
  ([PR #181](https://github.com/rjwalters/geode-fem/pull/181)).
- **ONNX** — audit-verdict graph-pure at **single-channel extent (TE l=1)**:
  the full pipeline (Bessel evaluation, 30000-interval grid scan, 60-step
  bisection as both unrolled and `Loop` forms — bit-identical) reproduces
  the catalogue TE l=1 roots to **1.2e-12 abs**; the 1e-5 consecutive dedup
  and compaction are host-side
  ([`onnx/audit/sphere_mie/probe_root_finding_loop.py`](onnx/audit/sphere_mie/probe_root_finding_loop.py),
  [PR #182](https://github.com/rjwalters/geode-fem/pull/182)).
- **JAX / TF-Java** — no implementation; both consume the J.1 catalogue
  fixture for classification.
- **Burn** — `crates/geode-core/src/mie.rs` (`mie_roots_catalog`,
  `merged_roots`).

### Row 11 — Mode classification / selection

- **NumPy** — d⁰-rank spurious-mode classifier + nearest-analytic-root
  labeling + cluster-closure window
  ([`numpy/sphere_pml.py`](numpy/sphere_pml.py),
  [`numpy/sphere_mie.py`](numpy/sphere_mie.py)).
- **JAX** — same classifier chain implemented in
  [`jax/sphere_pml.py`](jax/sphere_pml.py) / [`jax/sphere_mie.py`](jax/sphere_mie.py).
- **TF-Java** — shared: classification happens host-side in the common
  sidecar driver (NumPy lineage), not in the typed graph.
- **Julia** — d⁰ classifier + classification in
  [`julia/sphere_pml_small.jl`](julia/sphere_pml_small.jl) /
  [`julia/sphere_mie_small.jl`](julia/sphere_mie_small.jl).
- **ONNX** — host-side (sits past the out-of-graph eigensolve; not probed).
- **Burn** — de-Rham d⁰-rank classifier
  ([PR #126](https://github.com/rjwalters/geode-fem/pull/126)) +
  catalogue-driven classification in `crates/geode-core/examples/mie_sphere.rs`.
- The selection *contract* itself is the headline upstream spec gap —
  friction artifacts 2, 6, and 14 (see catalogue below). All backends meet
  it operationally via cluster-closure windows pinned in the fixtures
  (`strict_mode_window_len`).

## Disagreement catalogue

Every cross-backend disagreement found during the epic, with resolution
status. Artifacts 1–8 were catalogued in the
[pass-1 re-evaluation on #5](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4662851339)
(2026-06-09); artifacts 9–14 are the Phase I/J sweep findings catalogued in
the pass-2 re-evaluation (also on
[#5](https://github.com/rjwalters/geode-fem/issues/5)).

| # | Disagreement / finding | Source | Resolution status |
|---|---|---|---|
| 1 | `upload_mesh` silently downcast node coordinates to f32, relaxing K/M sub-stage tolerances ~5 orders of magnitude (NumPy vs Burn) | [comment](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4608345978), PRs [#73](https://github.com/rjwalters/geode-fem/pull/73)/[#86](https://github.com/rjwalters/geode-fem/pull/86)/[#106](https://github.com/rjwalters/geode-fem/pull/106) | **RESOLVED** — Burn honors `B::FloatElem`; a backend-honesty bug, not Maxwell-forced |
| 2 | "Lowest 5 modes" bisects the P1-lifted 9·π² degenerate cluster (NumPy vs Burn) | [comment](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4608346412) | **RESOLVED downstream** (k+1 eigenpairs + cluster-closure windows in all fixtures); **OPEN upstream** — mode-selection contract recommendation |
| 3 | ONNX cube-cavity expressibility: dedup/eigensolve not graph-expressible | [F.1 audit](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4624436015), PR [#119](https://github.com/rjwalters/geode-fem/pull/119) | **RESOLVED** — F.2 reduced graph ([PR #125](https://github.com/rjwalters/geode-fem/pull/125)) + input-boundary rule |
| 4 | Arpack.jl 0.5 vs SciPy calling-convention divergence on generalized eigenproblems | [comment](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4625959292) | **RESOLVED** — Julia uses shift-invert / dense fallback ([#133](https://github.com/rjwalters/geode-fem/issues/133), [PR #132](https://github.com/rjwalters/geode-fem/pull/132)); boundary divergence documented |
| 5 | `build_edges` secretly imperative under the graph-only constraint (G.6) | [comment](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4628822985), [input-boundary observation](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4636930206) | **RESOLVED** — topology at the L4-input boundary; G.7 host-computed-topology graph ([PR #142](https://github.com/rjwalters/geode-fem/pull/142)) |
| 6 | Arpack windowed shift-invert vs LAPACK dense global mode selection diverge on the lossy PML l=2 quintuplet (Julia vs NumPy/Burn) | [#160](https://github.com/rjwalters/geode-fem/issues/160), [PR #167](https://github.com/rjwalters/geode-fem/pull/167) | **RESOLVED downstream** (small-mesh dense-LAPACK tiebreaker); feeds the upstream mode-selection contract |
| 7 | JAX c128 autodiff traces complex assembly cleanly, zero custom VJPs (positive) | [PR #154](https://github.com/rjwalters/geode-fem/pull/154) | **CLOSED & EXTENDED** — J.4 extends to tensor-ε UPML with no reversal ([PR #180](https://github.com/rjwalters/geode-fem/pull/180)) |
| 8 | ONNX c128 vestigial in opset 18 — no arithmetic kernel honors it | [PR #162](https://github.com/rjwalters/geode-fem/pull/162) | **RESOLVED** — paired-real (f64-pair) lowering sanctioned as a derivation, not a semantic fork |
| 9 | onnxruntime 1.26.0 lacks f64 `Cos`/`Tan`/`Atan` kernels (`Sin` exists) — schema acceptance ≠ kernel coverage | [PR #182](https://github.com/rjwalters/geode-fem/pull/182) | **RESOLVED** — `cos(x) = Sin(x + π/2)` lowering preserves the f64 root contract; **upstream**: conformance statements must pin the runtime, not just the IR version |
| 10 | int64 is first-class end-to-end in opset 18 (incl. `ScatterND(add)`) — integer-exact contracts transfer in-graph (positive) | [PR #178](https://github.com/rjwalters/geode-fem/pull/178) | **CLOSED** — mirror image of artifact 8; exactness asserts ride the integer channel |
| 11 | ONNX `Loop` is a working `iterate-while-with-prev` given loop-invariant shapes + scalar condition + no data-dependent output counts; unroll-vs-Loop is a lowering choice (bit-identical) | [PR #182](https://github.com/rjwalters/geode-fem/pull/182), [comment](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4664817238) | **CLOSED** — resolves the pass-1 open iteration candidate; three contract restrictions recommended upstream |
| 12 | BCOO[complex128] friction-free in JAX 0.10.1 for non-differentiated paths — H.3's BCOO avoidance no longer warranted | [PR #180](https://github.com/rjwalters/geode-fem/pull/180) | **CLOSED** — positive finding; supersedes the H.3 workaround |
| 13 | TF-Java scalar late-scaling shortcut unavailable under tensor ε — per-axis coupling must happen inside the typed graph (element-wise `mul` + `reduceSum`; no batched einsum in TF-Java 1.0.0) | [PR #183](https://github.com/rjwalters/geode-fem/pull/183) | **RESOLVED** — per-axis kernel re-derived in-graph; op-vocabulary friction documented |
| 14 | Im(λ) sign on the tensor UPML pencil is mesh-dependent (Im < 0 on the 197-tet small mesh, Im > 0 on the refined 774-node mesh) — a pencil property, distinct from the scalar-PML sign convention | [PR #179](https://github.com/rjwalters/geode-fem/pull/179) | **RESOLVED** — sign assertions scoped per fixture; LAPACK and faer agree (not a solver convention); feeds the upstream mode-selection contract |

No unresolved cross-backend numerical disagreement remains. The two items
that stay open are *upstream spec recommendations* (mode-selection contract;
runtime-pinned conformance), tracked through
[#5](https://github.com/rjwalters/geode-fem/issues/5) for the maintainer
channel — not through this repo's issue flow.

## Burn re-anchoring statement

Per Epic #88's stated principle: **Burn's realization is anchored to the
reference set — when Burn disagrees with the reference, the reference is
right by definition.** Burn is the production runtime, not the semantic
anchor; the agreement of the independent reference implementations (NumPy as
default tiebreaker) defines what each L4 operator means.

This anchoring is enforced mechanically, per slice, in CI:

- **Per-slice cross-IR tests** — the `crates/geode-validation/tests/*_reference.rs`
  suite (24 cross-IR test files at close-out) compares Burn's output against
  every committed backend baseline under `cargo test -p geode-validation`:
  `p1_local_*`, `nedelec_local_*`, `cube_cavity_{numpy,jax,julia,onnx}_*`,
  `sphere_pec_{numpy,jax,julia,onnx,tfjava}_*`,
  `sphere_pml_{numpy,jax,julia,julia_small,tfjava}_*`,
  `derham_{numpy,julia}_*`, `mie_roots_{numpy,julia}_*`, and
  `sphere_mie_{numpy,jax,julia_small,tfjava}_*`.
- **Drift-gate workflows** — generator re-runs that strictly diff fresh
  backend output against the committed snapshots:
  [`jax-sphere-mie.yml`](../.github/workflows/jax-sphere-mie.yml),
  [`jax-sphere-pml.yml`](../.github/workflows/jax-sphere-pml.yml),
  [`julia-cube-cavity.yml`](../.github/workflows/julia-cube-cavity.yml),
  [`onnx-cube-cavity.yml`](../.github/workflows/onnx-cube-cavity.yml),
  [`tfjava-cube-cavity.yml`](../.github/workflows/tfjava-cube-cavity.yml)
  (three-way TF-Java/NumPy/Burn compare + the live-JVM sphere-mie lane), and
  [`cube-cavity-tolerance.yml`](../.github/workflows/cube-cavity-tolerance.yml).

A Burn-side change that drifts from any reference baseline fails these gates;
the documented escalation is to fix Burn (or, if the reference itself is
shown wrong by *cross-backend* disagreement, to file a friction artifact on
[#5](https://github.com/rjwalters/geode-fem/issues/5) and resolve it there
first). The retroactive re-anchoring promised in #88 is complete: every
operator Burn realizes on the spine now has at least one (usually several)
independent reference implementations gating it.
