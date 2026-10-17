# Fixture schema v1

Canonical (input, golden output) bundle format for the GEODE-FEM
reference set. Loaded by `geode-validation::Fixture::load`. Versioned
via the top-level `schema_version` string; this document describes
v1.

## Top-level keys

| Key | Type | Required? | Meaning |
|---|---|---|---|
| `schema_version` | string | yes | Currently `"1"`. |
| `fixture_id` | string | yes | Stable id, e.g. `"p1_reference_tet/local_stiffness"`. Used as the key inside `ComparisonReport`. |
| `description` | string | yes | One-liner: what does this fixture pin down? |
| `units` | string | yes | Free-form units convention. |
| `inputs` | map\<string, Field\> | optional | Inputs the implementation should consume. May be empty for fixtures that pin a pure-output identity (e.g. "the reference tet's local matrices are *exactly* this"). |
| `outputs` | map\<string, OutputField\> | yes | Golden outputs to compare against. |
| `provenance` | Provenance | yes | Where the golden values came from. |

## `Field` (inputs)

| Key | Type | Required? | Meaning |
|---|---|---|---|
| `shape` | array<int> | yes | Row-major shape. |
| `dtype` | string | yes | `"f64"` is exercised today. `"i64"`, `"c128"`, etc. reserved for future slices. |
| `description` | string | optional | Per-field comment. |
| `data` | array (nested or flat) of numbers | yes | Values. Both nested arrays matching `shape` and pre-flattened row-major arrays are accepted; the loader normalizes to flat. |

## `OutputField` (outputs)

Same as `Field`, plus:

| Key | Type | Required? | Meaning |
|---|---|---|---|
| `tolerance_abs` | f64 | yes | Per-field absolute tolerance used when comparing actual against golden. There is no global tolerance — different output kinds (eigenvalues vs. matrix entries vs. eigenvector residuals) call for different tolerances. |

## `Provenance`

| Key | Type | Required? | Meaning |
|---|---|---|---|
| `source` | string | yes | E.g. `"hand-computed exact rationals"`, or `"reference/numpy/p1_local_matrices.py SHA <sha>"`. |
| `verified_against` | string | optional | Cross-reference to a Rust-side check (test path / function). |
| `issue` | string | optional | Issue / PR that introduced the fixture. |

## Conventions

- **Row-major** flattening throughout. A `[1, 4, 4]` tensor flattens to
  a length-16 vector with `(b, i, j) → b*16 + i*4 + j`.
- **dtype names** match NumPy: `f64`, `f32`, `i64`, `i32`, `c128`,
  `c64`. The harness consumes the loader path for `f64` and `c128`
  today (Phase H scaffolding, issue #145); other dtypes remain reserved
  for forward use.

### Complex encoding (`c128`)

`c128` arrays are encoded as **real–imag interleaved** flat numeric
arrays of length `2 · prod(shape)`. The element at logical row-major
index `k` occupies disk positions `2k` (real part) and `2k+1`
(imaginary part), both as plain JSON numbers.

> **Note**: this is *not* NumPy's `np.complex128` byte layout — JSON
> is text, so the layout has to be explicit. Use
> `np.asarray(z).view(np.float64).tolist()` (on a contiguous c128
> array) to serialize, or equivalently
> `np.column_stack([z.real, z.imag]).flatten().tolist()`. On the load
> side, `geode_validation::Fixture::output_c128` consumes the
> interleaved flat list into `Vec<num_complex::Complex<f64>>`.

For `c128` **output fields**, `tolerance_abs` is applied to the
**complex modulus** `|Δ| = |actual − golden|`. Per-component
(separate `(Re, Im)`) tolerances are not part of v1 — if a fixture
needs them, document the convention in the fixture's per-field
description and split into two real fields until v2 lands a typed
`tolerance` shape.
- **Per-field tolerances** are absolute. Relative tolerances are
  deliberately omitted at v1 — they would require deciding whether to
  divide by the golden, the actual, or their mean, and Phase A doesn't
  need that decision yet. Re-evaluate once we have a fixture where the
  natural tolerance is relative (likely with the eigenvalue slice).
- **Inputs may be embedded mesh data** for the early phases. Mesh
  canonicalization (#88 open question #2) is intentionally deferred —
  start by embedding mesh data in the fixture file directly, revisit
  when the fixture set grows.

## Adding a new fixture

1. Pick a slice / case slug, e.g. `cube_cavity/n8_first_five_modes`.
2. Create `reference/fixtures/<slug>.json` (or `.h5` once HDF5 lands).
3. Document the field-naming convention in the fixture's `description`.
4. Add an integration test under `crates/geode-validation/tests/`
   that loads the fixture and compares an actual implementation
   against it. Use `1e-12` for hand-computed exact rationals; pick a
   defensible looser tolerance for numerically-derived references and
   document *why* in the fixture's `tolerance_abs` field.

## Versioning

When the schema changes incompatibly:

- Bump `schema_version` (`"1"` → `"2"`).
- Add the new value to `SUPPORTED_SCHEMA_VERSIONS` in
  `crates/geode-validation/src/fixture.rs`.
- Either continue supporting v1 loads (preferred — fixtures are
  durable artifacts) or write a one-shot migration script.
