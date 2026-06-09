# NumPy reference implementations

Canonical reference backend per **Epic #88**. NumPy goes first because
it has the largest training cohort, the thinnest abstraction between
math and code, and the most mature sparse eigensolvers
(`scipy.sparse.linalg.eigsh`). When backends disagree, NumPy is the
default tiebreaker.

## Status

Stub — first concrete impl lands with **#90** (NumPy P1 local
matrices) and **#92** (cube cavity end-to-end). Until then this
directory is intentionally empty.

## Planned layout

```
reference/numpy/
├── README.md                       — this file
├── pyproject.toml                  — pinned NumPy/SciPy versions (lands with #90)
├── p1_local_matrices.py            — element-local K and M for the P1 reference tet (#90)
├── cube_cavity.py                  — end-to-end cube-cavity eigenmode driver (#92)
├── sphere_pec.py                   — end-to-end PEC sphere eigenmode driver (#118, Phase G.2)
├── sphere_pml.py                   — end-to-end scalar-PML sphere eigenmode driver (#146, Phase H.1)
├── derham.py                       — discrete de Rham operators d⁰, d¹, d² (#149, Epic #88 Phase I bridge)
├── gen_derham_baseline.py          — fixture generator for reference/fixtures/derham/baseline.json (#149)
├── gen_sphere_pec_baseline.py      — fixture generator for sphere_pec/baseline.json
├── gen_sphere_pml_baseline.py      — fixture generator for sphere_pml/baseline.json (#146)
├── mie_roots.py                    — analytic Mie root catalogue, SciPy port of geode_core::mie (#170, Phase J.1)
├── gen_mie_roots_baseline.py       — fixture generator for mie_roots/baseline.json (#170)
├── sphere_mie.py                   — end-to-end anisotropic-UPML Mie driver (#171, Phase J.2)
├── gen_sphere_mie_baseline.py      — fixture generator for sphere_mie/baseline.json (#171)
├── gen_sphere_mie_small_baseline.py— fixture generator for sphere_mie_small/baseline.json (#171)
└── _harness.py                     — fixture I/O helper shared across slices
```

### Scalar-PML sphere (`sphere_pml.py`, Phase H.1)

Issue #146 promotes the Phase H scaffolding stub (#145 / PR #151) to a
full NumPy reference for the scalar-isotropic PML sphere eigenmode.
It mirrors `sphere_pec.py` line-for-line, replacing the real per-tet
ε with the complex profile from `geode_core::build_complex_epsilon_r_pml`
and the real generalized eigensolve with a dense complex one
(`scipy.linalg.eigvals`).

```bash
# Quick smoke print of the lowest 5 physical modes + Q-factor.
python3 reference/numpy/sphere_pml.py --fixture reference/fixtures/sphere_pml/sphere.msh --sigma0 5.0

# σ₀ = 0 PEC regression — should match `sphere_pec.py` at the physical band.
python3 reference/numpy/sphere_pml.py --sigma0 0.0

# Regenerate the on-disk baseline.json (~30 min, dense LAPACK ZGGEV).
python3 reference/numpy/gen_sphere_pml_baseline.py
```

The eigensolver is the dense `scipy.linalg.eigvals` (LAPACK ZGGEV), not
`scipy.sparse.linalg.eigs(... sigma=0)`. ARPACK shift-and-invert at
σ=0 produces numerical garbage on this pencil because the curl-curl
operator K is singular (gradient kernel of dimension ~368) so
`K - 0·M` cannot be LU-factored cleanly. Shifting σ into the
physical band biases the selection. The dense path sees the entire
spectrum and slices it deterministically; it is the canonical-
tiebreaker reference for the H sub-epic regardless of cost.

### Anisotropic-UPML Mie sphere (`sphere_mie.py`, Phase J.2)

Issue #171 carries the anisotropic-UPML port that Phase H deferred:
the end-to-end Mie pipeline of `crates/geode-core/tests/mie_sphere.rs`
(dielectric sphere, diagonal UPML tensor at σ₀ = 5.0, k₀_ref = 2.0),
anchored to the Phase J.1 analytic root catalogue. The constitutive
piece mirrors `geode_core::build_anisotropic_pml_tensor_diag` +
`batched_nedelec_local_mass_anisotropic_diag`; everything else is
shared with `sphere_pec.py` / `sphere_pml.py`.

```bash
# Quick smoke print on the small mesh (modes + J.1 classification + Q).
python3 reference/numpy/sphere_mie.py

# σ₀ = 0 collapse — the tensor degenerates to the real isotropic scalar.
python3 reference/numpy/sphere_mie.py --sigma0 0.0

# Regenerate the small baseline (seconds) / full baseline (~1 h dense ZGGEV).
python3 reference/numpy/gen_sphere_mie_small_baseline.py
python3 reference/numpy/gen_sphere_mie_baseline.py
```

The `derham.py` module is the formal NumPy reference for the discrete
de Rham complex (`gradient_map`, `curl_map`, `divergence_map`) plus the
interior-restricted ``rank(d⁰)`` spurious-mode classifier consumed by
``sphere_pec.py``. It cross-checks against
``geode_core::derham::{gradient_map, curl_map, divergence_map}`` at the
bit-exact integer-CSR level (no floating-point tolerance).

## Invocation convention

Reference impls are invoked by the Rust harness as subprocesses:

```bash
python reference/numpy/<slice>.py <fixture-path> <output-path>
```

The script reads inputs from the fixture (JSON v1, see
`reference/SCHEMA.md`), produces a results file in the same schema,
and exits 0 on success / nonzero on internal error. The Rust harness
diffs the output against the fixture's golden values per the standard
`Fixture::compare_against` flow.

## Toolchain bootstrap (forward-looking, defined in #90)

Recommended pattern when the first impl lands:

```bash
cd reference/numpy
python3.12 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt   # or `pip install -e .` once pyproject.toml lands
```

The Rust harness should *not* assume a particular venv path — every
backend script's interpreter is configurable via an env var like
`GEODE_VALIDATION_NUMPY=python3` (default `python3`). See #90 for
the exact wiring.
