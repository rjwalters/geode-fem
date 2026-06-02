# `p1_local/standard.json` schema (v1)

This document describes the on-disk format of the P1 local-matrices
reference fixture used by the cross-backend comparison harness
(issue #90, parent epic #88).

## Top-level structure

```json
{
  "meta": { ... },
  "cases": [ { ... }, ... ]
}
```

## `meta`

Generator metadata. Useful for diagnostics when a fixture regresses
across NumPy versions or whitespace-mungers.

| Field | Type | Description |
|---|---|---|
| `slice` | string | Always `"p1_local"` for this file. |
| `schema_version` | integer | Bumped when the case schema below changes shape. Currently `1`. |
| `generator` | string | Repo-relative path to the script that produced this file. |
| `generator_commit` | string | Short git SHA of the commit the generator ran on. |
| `numpy_version` | string | Output of `np.__version__` at generation time. |
| `python_version` | string | `major.minor.patch`. |
| `issue` | integer | The issue this fixture was first introduced under. |
| `epic` | integer | The parent epic. |
| `note` | string | Free-text format notes. |

## `cases`

Array of independent test cases. Each case is one tetrahedron.

| Field | Type | Description |
|---|---|---|
| `name` | string | Short snake_case identifier, used in test failure messages. |
| `description` | string | Human-readable case description. |
| `input.vertices` | `[[float; 3]; 4]` | The four tet vertex coordinates in (x, y, z) order, f64. Vertex 0 is the "base" used to form edges `e_k = v_k - v_0`. |
| `reference.numpy.k_local` | `[[float; 4]; 4]` | NumPy-baseline local stiffness `K_{ij}`, f64, row-major. |
| `reference.numpy.m_local` | `[[float; 4]; 4]` | NumPy-baseline local mass `M_{ij}`, f64, row-major. |
| `reference.numpy.signed_volume` | `float` | NumPy-baseline signed element volume `det(J) / 6`, f64. |

Extension points (forward compatible):

- More backends can be added under `reference.<backend_name>` with the
  same field shape. Comparisons remain backend-pair-specific.
- More fields can be added to a case (e.g. `reference.numpy.condition_number`)
  without bumping `schema_version` provided existing fields keep their
  shapes.

## Float precision

All floats are decimal-serialized via Python's default `float` repr,
which has been documented to be round-trip-faithful for IEEE 754 binary64
since Python 3.1. Rust's `serde_json` parses such strings back into
identical `f64` bit patterns, so the on-disk reference values match
NumPy's in-memory values exactly (no f64 → decimal → f64 slop).

When a tighter audit trail is needed, switch to hexfloat (`float.hex()`
in Python; `f64::from_str_radix`-style hex parsing on the Rust side).
Currently overkill for this slice.

## How baseline values were generated

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_p1_local_standard.py
```

The generator imports `numpy/p1_local_matrices.py` (the reference impl)
and runs it on each of the five canonical test cases. The output is
deterministic — re-running the generator on the same NumPy version
produces a byte-identical file modulo `generator_commit`.

## Backend-aware tolerance (Rust-side)

The Rust harness applies different tolerances depending on the active
Burn backend, since Burn's `wgpu` / `cuda` backends are f32 but the
NumPy baseline is f64:

| Backend | dtype | Relative | Absolute (near-zero) |
|---|---|---|---|
| `ndarray` | f64 | `1e-10` | `1e-12` |
| `wgpu`, `cuda` | f32 | `5e-5` | `1e-6` |

The f64 tolerances are the ones called out in issue #90's acceptance
criteria. The f32 tolerances are documented here as the f32-vs-f64
backend-dtype-honesty friction artifact (see Epic #88).
