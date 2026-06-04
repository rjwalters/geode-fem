# `reference/` — cross-backend reference set

Substrate for **Epic #88** (cross-validated L4 lowerings). This directory
holds the canonical fixtures, per-backend reference implementations, and
shared docs that the Rust-side comparison harness
(`crates/geode-validation/`) drives.

> **What this is:** a place where every spine slice has one canonical
> (input, golden output) bundle and a set of independent
> implementations whose agreement *is* the semantic anchor for the L4
> calculus (#88).
>
> **What this is not:** a multi-backend dispatch layer for production
> GEODE-FEM. Burn remains the production runtime; the backends here
> are reference / validation tools.

## Layout

```
reference/
├── README.md                       — this file
├── SCHEMA.md                       — fixture schema (v1)
├── fixtures/                       — canonical (input, golden output) bundles
│   ├── p1_reference_tet/
│   │   └── local_stiffness.json    — Phase A smoke fixture
│   ├── p1_local/                   — 5 per-case canonical fixtures (one fixture pins one identity; #90 / #101)
│   │   ├── canonical_reference_tet.json
│   │   ├── regular_tet.json
│   │   ├── anisotropic_well_shaped.json
│   │   ├── near_degenerate_sliver.json
│   │   └── inverted_tet.json
│   └── cube_cavity/
│       ├── baseline.json           — cube-cavity NumPy baseline (eigenvalues + sub-stages + Q_numpy, #92)
│       ├── baseline.schema.md      — per-fixture schema notes for `baseline.json`
│       ├── jax_baseline.json       — lowest 5 eigenvalues + traces from JAX (#93)
│       ├── julia_baseline.json     — lowest 5 eigenvalues + sub-stages from Julia + Arpack.jl (#115)
│       └── unit_cube.msh           — shared n=10 mesh (MSH 4.1 ASCII via meshio, #92)
├── numpy/                          — NumPy/SciPy reference impls (Python)
│   ├── README.md
│   ├── requirements.txt            — pinned NumPy + scipy + meshio versions (#90, #92)
│   ├── mesh.py                     — shared mesh builders (cube_tet_mesh, cube_interior_mask, load_msh, write_msh) (#103)
│   ├── p1_local_matrices.py        — P1 element-local K and M (#90)
│   ├── gen_p1_local_per_case.py    — regenerates `fixtures/p1_local/<case>.json` (#90 / #101)
│   ├── cube_cavity.py              — cube-cavity end-to-end driver, n=10 + Gmsh-fixture path (#92)
│   ├── cube_cavity_minimal.py      — sibling cube-cavity driver, programmatic n=4 path (#93)
│   └── gen_cube_cavity_baseline.py — regenerates `fixtures/cube_cavity/baseline.json` (#92)
├── jax/                            — JAX reference impls (Python)
│   ├── README.md                   — DX friction notes (per #88 JAX-DX follow-up)
│   ├── cube_cavity.py              — Cube-cavity assembly + autodiff anchor (#93)
│   └── gen_cube_cavity_fixture.py  — regenerates fixtures/cube_cavity/jax_baseline.json (#93)
├── tf_java/                        — TF-Java reference impls (Java + Maven)
│   ├── README.md
│   └── cube_cavity/                — Maven project, static-graph assembly (#93)
│       ├── pom.xml
│       └── src/main/java/dev/geodefem/refcubecavity/
│           ├── CubeMesh.java        — JVM-side mesh
│           ├── AssemblyGraph.java   — TF-Java Ops + Session static graph
│           └── CubeCavityMain.java  — driver + sidecar emitter
├── driver/                         — cross-language seam scripts
│   ├── README.md
│   ├── eigensolve_from_tfjava.py    — SciPy eigensolve from TF-Java sidecar
│   ├── emit_numpy_eigenvalues.py    — fixture-schema JSON shim for NumPy n=4/n=10 in-job
│   └── compare_eigenvalues.py       — cross-IR agreement table (TF-Java / Julia vs JAX / NumPy / Burn)
├── julia/                          — Julia reference impls
│   ├── README.md                   — toolchain bootstrap + Julia friction notes (#115)
│   ├── Project.toml                — pinned Arpack.jl + JSON3.jl deps
│   ├── mesh.jl                     — cube_tet_mesh + inline MSH 4.1 parser + cube_interior_mask
│   ├── cube_cavity.jl              — Cube-cavity assembly + Arpack.jl eigensolve (#115)
│   └── gen_cube_cavity_baseline.jl — regenerates fixtures/cube_cavity/julia_baseline.json
└── onnx/                           — ONNX graph references (deferred)
    └── README.md
```

## Workspace shape — answer to #88 open question #1

> *"Workspace shape: new crate (`geode-validation`) inside the existing
> workspace, or sibling Python/Julia/JAX/ONNX repo cross-linked from
> here?"*

**Resolved as: hybrid — in-tree harness + in-tree reference sources,
with per-backend toolchains owned by sibling tooling rather than the
Rust workspace.**

Concretely:

| Layer | Lives where | Why |
|---|---|---|
| **Comparison harness** | `crates/geode-validation/` (Rust, in this workspace) | The Rust side has to drive the comparison anyway; making it a workspace crate gives us `cargo test` integration and matches the existing pattern (cf. `geode-core`'s `tests/` directory pinning regression fixtures). |
| **Canonical fixtures** | `reference/fixtures/` (in-tree) | One source of truth, reviewed in PRs alongside the code that produces or consumes them. Binary HDF5 fixtures will live here too once the eigenvector slice lands (#92). |
| **Per-backend reference impls** | `reference/<backend>/` (in-tree source files) | Source-of-truth in the same git history as the harness — PR review can show "we changed the NumPy impl *and* the fixture *and* the Rust impl in lockstep." |
| **Per-backend toolchain / venv / `Project.toml`** | Owned by `reference/<backend>/` directly, not by the Cargo workspace | Avoids forcing a polyglot build dependency on every Rust contributor. A Rust dev who never touches the NumPy backend should not need Python to run `cargo test`. |

**Rejected alternatives**:

- **Sibling repo for everything non-Rust** — increases the bookkeeping
  cost (cross-repo PRs every time a fixture changes), and the
  *fixtures* really do belong with the harness that consumes them.
  We may still split out a sibling repo *later* if the per-backend
  source tree grows large enough to warrant its own CI, but Phase A
  doesn't need that.
- **Pure-Rust workspace with backends invoked purely as subprocesses
  reading/writing JSON** — fine in principle but makes the round-trip
  feedback loop (edit NumPy → see new diff artifact) clumsier than
  necessary. Keeping the Python source in-tree gives us natural
  `python reference/numpy/run.py` invocations from the harness.

**Implication for #88's Phase B–F**: each per-backend phase lands a
`reference/<backend>/` directory tree with its own toolchain bootstrap
docs, and the Rust harness gains a thin subprocess-driver per backend.
The fixtures and the comparator stay backend-agnostic.

## Fixture format choice — JSON now, HDF5 reserved

The harness supports a `FixtureFormat::Json` variant today and reserves
`FixtureFormat::Hdf5` for the eigenvector-class outputs that arrive
with the cube-cavity slice (#92).

Rationale:

- The Phase-A smoke fixture is a single 4×4 matrix plus a 4×4 mass
  matrix plus a scalar — JSON is the smallest sufficient format. It
  also reviews cleanly in PRs.
- HDF5 is the format-of-record once we ship large complex eigenvectors
  (#88's framing — see *Implementation notes* in #89). The
  `FixtureFormat::Hdf5` variant is wired into the API now so callers
  can pin format choice without waiting for the implementation; the
  linker dependency on `libhdf5` will be added behind a Cargo feature
  in the same PR that lands the first binary fixture (#92), so
  contributors who never run cube-cavity validation don't pay the
  `libhdf5` install cost.
- See `SCHEMA.md` for the JSON v1 schema.

## How the harness emits a diff artifact

`ComparisonReport::write_diff_artifact(path)` writes a pretty-printed
JSON document with one entry per declared output field:

```json
{
  "fixture_id": "p1_reference_tet/local_stiffness",
  "passed": false,
  "report_schema_version": "1",
  "fields": [
    {
      "field": "k_local",
      "passed": false,
      "status": { "kind": "tolerance_exceeded", "n_violations": 1 },
      "tolerance_abs": 1e-12,
      "golden_shape": [1, 4, 4],
      "actual_len": 16,
      "max_abs_error": 1e-3,
      "worst_offender": {
        "index": 0,
        "golden": 0.5,
        "actual": 0.501,
        "abs_error": 1e-3
      }
    }
  ]
}
```

The status enum names the failure mode (`ok`, `missing_from_actual`,
`shape_mismatch`, `tolerance_exceeded`, `non_finite_in_actual`) so
disagreements stay legible. Per #88's friction-mining loop, this is
the artifact that gets attached to spec-anchoring discussions when
two backends disagree.

See `crates/geode-validation/tests/smoke.rs` for the end-to-end loop.

## What lives where (cheatsheet)

| Question | Answer |
|---|---|
| Where do I add a new fixture? | `reference/fixtures/<slice>/<case>.json` (or `.h5` once #92 lands HDF5) |
| Where do I add a new Rust comparison test? | `crates/geode-validation/tests/<slice>_<case>.rs` |
| Where does the NumPy implementation live? | `reference/numpy/<slice>.py` (e.g. `p1_local_matrices.py` for #90) |
| What schema version are we on? | `1` — see `SCHEMA.md` |
| What tolerance should I use? | Per-field, declared in the fixture's `outputs.<field>.tolerance_abs`. There is no global tolerance. |

## Reference impls in flight

### P1 local matrices (NumPy, #90 / #101)

First concrete reference impl on top of the Phase-A scaffolding.

- **Reference**: `numpy/p1_local_matrices.py` — element-local stiffness
  and mass for the P1 reference tet, f64 throughout.
- **Fixtures**: `fixtures/p1_local/<case>.json` — **five per-case
  canonical-schema-v1 fixtures**, one per tet (`canonical_reference_tet`,
  `regular_tet`, `anisotropic_well_shaped`, `near_degenerate_sliver`,
  `inverted_tet`). Each pins one identity. Per-field tolerances are
  loose absolute (f32-friendly tripwires); the Rust comparator layers a
  tighter backend-aware mixed abs/rel check on top. This shape was
  consolidated from a legacy multi-case `standard.json` bundle in #101
  to land on the canonical `Fixture` / `ComparisonReport` API.
- **Rust comparator**:
  `crates/geode-validation/tests/p1_local_numpy_reference.rs` — uses
  the canonical `Fixture::compare_against` flow and writes one diff
  artifact per case to
  `CARGO_TARGET_TMPDIR/p1_local_<case>_diff.json` on every run.

**Regenerating the fixtures** (deterministic on a pinned NumPy):

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_p1_local_per_case.py
```

Re-runs should produce byte-identical per-case fixtures for the same
pinned NumPy version (see `numpy/requirements.txt`).

**Per-backend dtype honesty.** The NumPy reference is f64 throughout.
The Burn default backend (wgpu) runs f32; the optional `ndarray`
backend runs f64. The #90 comparator applies a backend-aware tolerance:

| Rust backend | Burn dtype | Tolerance vs. NumPy baseline |
|---|---|---|
| `ndarray` (CI / `--features ndarray`) | f64 | `1e-10` relative, `1e-12` absolute |
| `wgpu` / `cuda` (default, GPU) | f32 | `5e-5` relative, `1e-6` absolute |

This is the f32-vs-f64 friction artifact called out in #88; see
PR #73 / PR #86 / [#5 (curator pass 2026-06-02)](https://github.com/rjwalters/geode-fem/issues/5#issuecomment-4606094785).

### Cube cavity end-to-end (NumPy, #92)

Phase B Phase B closure: the first reference impl that exercises the
**full** scalar-Helmholtz pipeline — mesh I/O → P1 local matrices →
global assembly → Dirichlet BC → generalized eigensolve.

- **Reference**: `numpy/cube_cavity.py` — end-to-end scalar Helmholtz
  driver. Reads a `.msh` via `meshio`, assembles global CSR via
  `scipy.sparse.coo_matrix(...).tocsr()`, eigensolves via
  `scipy.sparse.linalg.eigsh(K, k=5, M=M, sigma=0.0, which='LM')`.
- **Fixture**: `fixtures/cube_cavity/baseline.json` + the shared
  `fixtures/cube_cavity/unit_cube.msh` (n=10 mesh, 1331 nodes, 6000
  tets). The fixture stores eigenvalues, K_int / M_int Frobenius
  norms, full diagonals of K_int / M_int, the analytic Dirichlet
  Laplacian targets, AND the NumPy eigenvectors `Q_numpy` as an
  *input* field (so the Rust harness can compute subspace overlap —
  the elementwise comparison is the wrong metric for degenerate
  eigenspaces). See `fixtures/cube_cavity/baseline.schema.md` for the
  per-field tolerance table and the cluster-overlap convention.
- **Rust comparator**: `crates/geode-validation/tests/cube_cavity_numpy_reference.rs`
  — built on `geode-validation`'s `Fixture` + `ComparisonReport` per
  the canonical pattern (the #90 inline shortcut is explicitly *not*
  repeated here). Writes a structured diff artifact to
  `CARGO_TARGET_TMPDIR/cube_cavity_diff.json` on every run.

**Regenerating the fixture**:

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_cube_cavity_baseline.py
```

**Cross-backend mesh sharing**: the same `unit_cube.msh` is the input
for the meshio path of `reference/jax/cube_cavity.py` and the eventual
TF-Java meshio integration. The programmatic n=4 path
(`numpy/cube_cavity_minimal.py`, JAX assembly, TF-Java assembly) skips
mesh I/O entirely so cross-backend disagreements are not contaminated
by mesh-reader friction.

### Cube-cavity Helmholtz (JAX + TF-Java, #93)

Second + third concrete backends for the cube-cavity spine slice
(siblings to the NumPy reference in #92).

- **JAX reference**: `jax/cube_cavity.py` — full pipeline on a
  programmatic n=4 mesh, JAX assembly + SciPy eigensolve boundary,
  with a `jax.grad(tr(K_int))` autodiff anchor
  finite-difference-validated to `1e-5` (actually `~1e-10` per the
  self-check). The programmatic mesh is what lets autodiff propagate
  cleanly through assembly without I/O in the path.
- **TF-Java reference**: `tf_java/cube_cavity/` — Maven project,
  static-graph (`Ops` + `Session`) assembly that emits a JSON sidecar
  for the eigensolve seam in `driver/eigensolve_from_tfjava.py`. The
  baked graph is the differentiable artifact, so a programmatic mesh
  is required here too.
- **NumPy sibling oracle**: `numpy/cube_cavity_minimal.py` — minimal
  NumPy cube-cavity on the **same** programmatic n=4 mesh that JAX
  and TF-Java use. This is the same-tree NumPy oracle for the
  programmatic path; it is a genuine sibling to `cube_cavity.py`
  (n=10 / Gmsh path), not a duplicate.
- **JAX baseline fixture**: `fixtures/cube_cavity/jax_baseline.json`
  — schema v1, lowest 5 eigenvalues + interior-DOF traces. Lives
  beside `baseline.json` (different `n`, different schema, different
  mesh source).
- **Rust comparator**: `crates/geode-validation/tests/cube_cavity_jax_reference.rs`
  loads the JAX baseline via the canonical `Fixture` loader and runs
  the Burn cube-cavity path (`assemble_global_p1` +
  `apply_dirichlet_bc` + `FaerDenseEigensolver`) against it. Migrated
  from `crates/geode-core/tests/` onto `geode-validation` in #101 (was
  Option-A interim placement under #93).
- **TF-Java runtime CI**: deferred to a follow-up CI-config issue;
  the source + Maven project ship here, but JVM/Maven setup in CI is
  out of scope for #93.

## Parent epic

- **#88** — cross-validated L4 lowerings
- **#89** — this scaffolding (Phase A)
- **#90** — NumPy P1 local matrices (Phase B, **merged**)
- **#91** — d² discrete operator (parallel slice, **merged**)
- **#92** — cube-cavity end-to-end NumPy (Phase B, in flight wave 2)
- **#93** — cube-cavity JAX + TF-Java (Phase C+D, this PR)
- **#5** — whiteroom tracker (file friction artifacts here)
