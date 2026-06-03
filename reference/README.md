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
│   └── p1_local/
│       ├── standard.json           — 5-tet P1 fixture (inputs + NumPy baseline outputs, #90)
│       └── standard.schema.md      — per-fixture schema notes for `standard.json`
├── numpy/                          — NumPy/SciPy reference impls (Python)
│   ├── README.md
│   ├── requirements.txt            — pinned NumPy version (#90)
│   ├── p1_local_matrices.py        — P1 element-local K and M (#90)
│   └── gen_p1_local_standard.py    — regenerates `fixtures/p1_local/standard.json` (#90)
├── jax/                            — JAX reference impls (deferred)
│   └── README.md
├── julia/                          — Julia reference impls (deferred)
│   └── README.md
├── onnx/                           — ONNX graph references (deferred)
│   └── README.md
└── tf_java/                        — TF-Java reference impls (deferred)
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

### P1 local matrices (NumPy, #90)

First concrete reference impl on top of the Phase-A scaffolding.

- **Reference**: `numpy/p1_local_matrices.py` — element-local stiffness
  and mass for the P1 reference tet, f64 throughout.
- **Fixture**: `fixtures/p1_local/standard.json` — a 5-tet cluster with
  inputs (vertex coordinates) plus pre-computed NumPy baseline outputs
  under `reference.numpy`. Per-fixture schema notes live next to it in
  `standard.schema.md`; the canonical schema is still `SCHEMA.md` at
  the root.
- **Rust comparator (interim location)**: lives at
  `crates/geode-core/tests/p1_local_numpy_reference.rs` rather than the
  `geode-validation` slot above. This is **Option A** for #90 — the PR
  inlined a minimal load-and-compare path against the standard fixture
  to unblock the cross-check without taking a dependency on the broader
  harness API surface. Migrating this test onto `geode-validation`'s
  `Fixture::compare_against` flow is tracked as a follow-up; the
  fixture itself is already canonical-shape, so the move is mechanical.

**Regenerating the fixture** (deterministic on a pinned NumPy):

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_p1_local_standard.py
```

Re-runs should produce a byte-identical `standard.json` for the same
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

## Parent epic

- **#88** — cross-validated L4 lowerings
- **#89** — this scaffolding (Phase A)
- **#90** — NumPy P1 local matrices (Phase B, in flight)
- **#91** — d² discrete operator (parallel slice)
- **#92** — cube-cavity end-to-end (Phase B continued)
- **#5** — whiteroom tracker (file friction artifacts here)
