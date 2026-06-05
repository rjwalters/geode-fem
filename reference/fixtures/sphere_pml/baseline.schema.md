# `sphere_pml/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
scalar-isotropic-PML sphere eigenmode slice (issue #145, parent epic
#88, Phase H scaffolding — sibling of `sphere_pec/`). Analog of
`reference/fixtures/sphere_pec/baseline.schema.md` extended to the
**complex-permittivity / complex-eigenvalue** PML case.

This fixture is the cross-backend infrastructure substrate for the
per-backend Phase H reference impls:

- **H.1** `#146` — NumPy sphere PML reference (fills out the numerical
  fields this scaffold stubs out).
- **H.2** `#147` — Julia sphere PML reference.
- **H.3** `#148` — JAX sphere PML reference.

The scaffolding here is intentionally **stub-quality** on the numerics
side: the loader/schema/comparator have to land before the per-backend
references can branch from a stable `c128` API. Full PML numerical
content is owned by H.1.

## What this fixture pins (scaffolding scope)

- **Mesh**: the same bundled `sphere.msh` used by `sphere_pec/` (the
  three nested regions — dielectric / vacuum gap / PML shell — are
  defined in `crates/geode-core/src/mesh/sphere.rs`).
- **PML profile parameters**: `sigma_0 = 5.0` (the value used in
  `crates/geode-core/tests/sphere_pml_eigenmode.rs`) and the
  `R_PML_INNER / R_BUFFER` radii (pinned via the same mesh constants
  the Burn side keys off of).
- **Complex permittivity assignment**: full per-tet `epsilon_r_complex`
  (length `n_tets`, dtype `c128`). The scalar-isotropic PML profile is
  documented in `geode_core::build_complex_epsilon_r_pml`:
  ```text
  ε_r(r) = 1 − j σ₀ ((r − R_PML_INNER) / (R_BUFFER − R_PML_INNER))²
  ```
  in the PML shell; `n² = 2.25` inside the dielectric; real `1.0` in
  the vacuum gap. Using the `exp(+jωt)` convention.
- **Complex eigenvalue smoke output**: `eigenvalues_lowest_complex`
  (dtype `c128`). At scaffolding time this is **two synthetic
  entries** (one in the spurious near-zero cluster, one with the
  expected `Im(λ) < 0` PML signature) so the complex comparator gets
  exercised end-to-end without depending on the eigensolve numerical
  output. The full physical spectrum lands with H.1 (`#146`).
- **Q-factor diagnostic**: `q_factor_lowest_physical` (scalar f64,
  derived metric `Re(λ) / (2·Im(λ))` for the lowest non-spurious
  mode). Sanity output for human review; sign convention is "Q > 0
  means absorbing" (PML loss as positive Q with our negative-`Im`
  convention is `-Re/(2·Im)`).

## Output fields (under `outputs`)

| Field                          | Shape          | Dtype | Tolerance          | What it pins                                                                                  |
|--------------------------------|----------------|-------|--------------------|-----------------------------------------------------------------------------------------------|
| `n_nodes`                      | `[1]`          | `f64` | `0.5` absolute     | Integer cross-check on mesh I/O.                                                              |
| `n_tets`                       | `[1]`          | `f64` | `0.5` absolute     | Integer cross-check on mesh I/O.                                                              |
| `eigenvalues_lowest_complex`   | `[2]` (stub)   | `c128`| `1e-6` (on `|Δ|`)  | Lowest complex eigenvalues. **Stub**: 2 synthetic entries at scaffolding; expanded under H.1. |
| `q_factor_lowest_physical`     | `[1]`          | `f64` | `1e-6` absolute    | Derived metric `-Re(λ)/(2·Im(λ))` for the lowest non-spurious mode. Sanity output.            |

## Input fields (under `inputs`)

| Field                  | Shape       | Dtype | Description                                                                            |
|------------------------|-------------|-------|----------------------------------------------------------------------------------------|
| `mesh_path`            | `[0]`       | `f64` | Relative path to the bundled mesh (`reference/fixtures/sphere_pml/sphere.msh`).        |
| `sigma_0`              | `[1]`       | `f64` | PML absorption strength. Matches the Burn integration test value (`5.0`).              |
| `r_sphere`             | `[1]`       | `f64` | Inner dielectric sphere radius (`R_SPHERE` = 1.0).                                     |
| `r_pml_inner`          | `[1]`       | `f64` | PML inner radius (`R_PML_INNER` = 1.5).                                                |
| `r_buffer`             | `[1]`       | `f64` | Outer PEC wall radius (`R_BUFFER` = 2.0).                                              |
| `n_index`              | `[1]`       | `f64` | Refractive index inside the dielectric sphere (`1.5`).                                 |
| `epsilon_r_complex`    | `[n_tets]`  | `c128`| Per-tet **complex** relative permittivity. Stub-populated at scaffolding (small slice; see note). |

> **Note on `epsilon_r_complex` at scaffolding time**: the long-term
> shape is `[n_tets]`, but the scaffolding stub
> (`gen_sphere_pml_baseline.py`) populates **4 illustrative entries**
> covering the three regions (dielectric / vacuum-gap / PML-shell) and
> declares the shape as `[4]` in the stub fixture so the length-check
> in `Fixture::input_c128` stays meaningful. H.1 (#146) will swap to
> the full `[n_tets]` vector when the NumPy reference lands.

## On-disk encoding for `c128`

Real–imag interleaved row-major flat arrays. See
`reference/SCHEMA.md` → "Complex encoding (`c128`)" for the
load-side contract. Generators serialize via
`np.asarray(z).view(np.float64).tolist()` (on a contiguous c128
array); the Rust loader (`Fixture::output_c128`) reads it back into
`Vec<num_complex::Complex<f64>>`.

## Q-factor sign convention

We use the `exp(+jωt)` convention throughout the Burn side. The PML
profile produces `Im(ε_r) < 0` in the absorbing shell
(`build_complex_epsilon_r_pml`), which propagates to eigenvalues with
`Im(λ) < 0` for outgoing physical modes. The "Q > 0 means absorbing"
display convention then requires the minus sign:

```text
Q = -Re(λ) / (2 · Im(λ))    # outputs positive Q for absorbing modes
```

The fixture stores `q_factor_lowest_physical` under this sign
convention. If a backend uses the `exp(-jωt)` convention internally,
the comparator step is responsible for matching sign before comparing.

## Tolerance budget

The scaffolding fixture's `c128` tolerance (`1e-6`) is loose-but-real:
the synthetic 2-entry stub uses round-number values where exact
equality holds, so the tolerance is reserved for when H.1 lands real
eigensolver output. Real eigenvalue tolerances at the physical-band
floor (λ ≈ 1.4) will likely settle at `1e-5` absolute (~7e-6
relative), mirroring the `sphere_pec/` fixture's
`physical_eigenvalues` field — H.1 will revisit this when the NumPy
output lands.

## Spurious filter / cluster detection

Inherited from `sphere_pec/` — the algebraic `d⁰`-rank classifier
(`spurious_dim_from_derham`) survives the lossy-ε scaling because
gradients of `H¹_0` sit in the kernel of curl-curl independent of
`ε(x)` scaling on the mass. H.1 will populate `n_spurious_observed`
and the spurious-filtered `physical_eigenvalues` once the eigensolve
is wired in.

## Regeneration

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_sphere_pml_baseline.py
```

The scaffolding generator is **stub-quality**: it produces a
schema-conformant `baseline.json` with synthetic complex values
sufficient to exercise the loader and comparator. Full PML numerics
land with H.1 (`#146`).

## Versioning

The fixture follows `schema_version = "1"` (the canonical
`reference/SCHEMA.md`, extended with the `c128` interleaved encoding
in issue #145).
