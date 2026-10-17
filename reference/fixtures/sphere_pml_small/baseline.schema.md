# `sphere_pml_small/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
**small-mesh sibling** of `sphere_pml/baseline.json` (issue #158,
parent epic #88).

The sphere_pml fixture's Burn faer 0.24 complex generalized
eigensolve on the full 3300×3300 interior pencil takes 60+ minutes,
forcing its cross-check to be release-gated and `#[ignore]`-marked.
This sibling shrinks the mesh to ~200 tets so the canonical Burn vs
NumPy PML spectrum check fits in default `cargo test -p
geode-validation` (target <30 s for Burn complex GEVD on a developer
machine; measured ~3.5 s).

## What this fixture pins

Same physical-group convention and PML profile as the full
`sphere_pml/` fixture (3 shells, `sigma_0 = 5.0`, `n_index = 1.5`,
same radii), at the smaller mesh:

- **Mesh**: 48 nodes / 197 tets / 259 edges / 214 interior DOFs.
  Generated from `mesh_scripts/sphere_small.geo` (lc-floored by the
  3-shell BooleanFragments topology — see geo header for the
  ~200-tet practical lower bound and why the issue's "<100 tets"
  target isn't reachable with this physical-group layout).
- **PML profile parameters**: `sigma_0 = 5.0`, `R_SPHERE = 1.0`,
  `R_PML_INNER = 1.5`, `R_BUFFER = 2.0`, `n_index = 1.5` — same as
  the full fixture so cross-mesh comparisons are well-defined.
- **Complex permittivity**: full per-tet `epsilon_r_complex` (length
  197, dtype `c128`). Same profile as the full fixture.
- **Complex eigenvalue spectrum**: `eigenvalues_lowest_complex` =
  lowest `spurious_dim + 8 = 39` complex eigenvalues, sorted by
  `|Re(λ)|` ascending. Sign convention: `Im(λ) > 0` per PR #155
  Judge's binding decision (Wave-2 canonical).
- **Physical eigenvalues**: lowest 5 complex eigenvalues past the
  d⁰-rank spurious split. On this mesh the ground physical mode sits
  at λ ≈ 1.92 + 0.055j (vs the full fixture's λ ≈ 1.18 + 0.21j —
  coarser discretization pushes the physical band higher and reduces
  the per-mode loss).
- **Q-factor**: of the lowest physical mode (~34.8; much higher Q
  than the full fixture's ~1.2 because the small mesh's ground mode
  has a smaller Im(λ)).
- **σ₀=0 PEC anchor**: `sigma_zero_lowest_physical_re` — the lowest
  physical Re(λ) at σ₀=0 (the PEC limit). Used by the σ₀=0 collapse
  test since the small mesh has no separate PEC baseline to defer to
  (the full fixture cross-references `sphere_pec/baseline.json`).

## Output fields (under `outputs`)

| Field                          | Shape  | Dtype  | Tolerance         | What it pins                                                                                          |
|--------------------------------|--------|--------|-------------------|-------------------------------------------------------------------------------------------------------|
| `n_nodes`                      | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on mesh I/O (= 48).                                                               |
| `n_tets`                       | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check (= 197).                                                                          |
| `n_edges`                      | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on edge enumeration (= 259).                                                      |
| `n_interior_edges`             | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on PEC mask reduction (= 214).                                                    |
| `spurious_dim`                 | `[1]`  | `f64`  | `0.5` absolute    | Predicted d⁰-rank spurious dim (= number of interior nodes = 31).                                     |
| `n_spurious_observed`          | `[1]`  | `f64`  | `0.5` absolute    | Algebraic d⁰-rank classifier output (= 31).                                                           |
| `eigenvalues_lowest_complex`   | `[39]` | `c128` | `5e-4` (on `|Δ|`) | Lowest `spurious_dim + 8` complex eigenvalues, sorted by `|Re(λ)|`. Spurious cluster + physical.       |
| `physical_eigenvalues_complex` | `[5]`  | `c128` | `1e-4` (on `|Δ|`) | Lowest 5 physical complex eigenvalues past the d⁰-rank split. Sign convention: `Im(λ) > 0`.            |
| `q_factor_lowest_physical`     | `[1]`  | `f64`  | `1e-2` absolute   | Sign-agnostic `Re(k) / (2 |Im(k)|)` for the lowest physical mode. Wider tol than full fixture — high-Q is sensitive to Im(λ) residuals. |
| `sigma_zero_lowest_physical_re`| `[1]`  | `f64`  | `5e-5` absolute   | Lowest physical Re(λ) at σ₀=0 (PEC anchor; small-mesh PEC collapse target).                            |

## Input fields (under `inputs`)

| Field                  | Shape       | Dtype  | Description                                                                                  |
|------------------------|-------------|--------|----------------------------------------------------------------------------------------------|
| `mesh_path`            | `[0]`       | `f64`  | Relative path: `reference/fixtures/sphere_pml_small/sphere.msh`.                              |
| `sigma_0`              | `[1]`       | `f64`  | PML absorption strength (`5.0`).                                                              |
| `r_sphere`             | `[1]`       | `f64`  | Inner dielectric sphere radius (`R_SPHERE` = 1.0).                                            |
| `r_pml_inner`          | `[1]`       | `f64`  | PML inner radius (`R_PML_INNER` = 1.5).                                                       |
| `r_buffer`             | `[1]`       | `f64`  | Outer PEC wall radius (`R_BUFFER` = 2.0).                                                     |
| `n_index`              | `[1]`       | `f64`  | Refractive index inside dielectric sphere (`1.5`).                                            |
| `epsilon_r_complex`    | `[197]`     | `c128` | Per-tet complex relative permittivity (full vector).                                          |

## Why these tolerances are looser than the full fixture

The full `sphere_pml/baseline.json` uses `1e-5` on the complex
eigenvalue fields and `1e-3` on Q-factor. On the small mesh those
budgets aren't reachable because:

1. **Smaller spectral gap**: the 214-DOF pencil's spurious-vs-physical
   condition-number gap is smaller than the full 3300-DOF pencil's,
   inflating faer 0.24 QZ vs LAPACK ZGGEV per-eigenvalue residuals
   from ~1e-6 to ~1.2e-4 absolute (on the spurious cluster) and
   ~6e-5 (on the physical band).
2. **High-Q ground mode**: the small mesh's coarser discretization
   pushes the ground physical mode to a higher Q (~34.8 vs ~1.2),
   amplifying dQ/dIm(λ) by ~30× and inflating the Q-factor residual
   from ~6e-5 (full) to ~9e-3 (small).
3. **PEC anchor**: even on the real-symmetric σ₀=0 collapse, the
   smaller pencil's faer-vs-LAPACK residual is ~2e-5 absolute on Re(λ)
   (vs ~1e-7 on the full fixture).

These looser bounds still pin the cross-backend agreement
**qualitatively** — the canonical `Im(λ) > 0` sign convention is
bit-exact (no sign flips), the spurious-vs-physical classifier
matches at 0 violations, and the physical eigenvalues agree to
4 decimal digits. The full fixture's tighter tolerances remain the
canonical numerical pin via the release-gated path.

## On-disk encoding for `c128`

Real–imag interleaved row-major flat arrays. See
`reference/SCHEMA.md` → "Complex encoding (`c128`)" for the
load-side contract. Same as the full sphere_pml fixture.

## Reproducing the fixture

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_sphere_pml_small_baseline.py
```

The dense scipy.linalg.eigvals on the ~214-DOF interior pencil takes
<1 s — small enough that regenerating the baseline is essentially
free.

## Regenerating the mesh

```bash
gmsh -3 -format msh4 -o reference/fixtures/sphere_pml_small/sphere.msh \
    mesh_scripts/sphere_small.geo
```

The geo file's header documents the topology floor (`Mesh.Character
isticLengthFactor = 4.0` is the smallest workable value before PLC
recovery fails on the inner shell).
