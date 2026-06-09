# `sphere_mie_small/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
**small-mesh anisotropic-UPML Mie** fixture (issue #171, Epic #88
Phase J.2 — small-mesh sibling per the #158 pattern).

This fixture carries the anisotropic-UPML port that Phase H deferred:
`sphere_pml_small/` pins the **scalar isotropic** PML; this fixture
pins the **diagonal anisotropic UPML tensor** at the exact
`crates/geode-core/tests/mie_sphere.rs` acceptance parameters
(`σ₀ = 5.0`, `k₀_ref = 2.0`, `n = 1.5`), cross-anchored to the Phase
J.1 analytic Mie-root catalogue
(`reference/fixtures/mie_roots/baseline.json`).

## Mesh

**Shared with `sphere_pml_small/`** — the fixture's `mesh_path` input
points at `reference/fixtures/sphere_pml_small/sphere.msh` (48 nodes /
197 tets / 259 edges / 214 interior DOFs, generated from
`mesh_scripts/sphere_small.geo`). No mesh duplication.

## What this fixture pins

- **Anisotropic tensor ε** (`epsilon_tensor_diag`, shape `(197, 3)`,
  dtype `c128`): per-tet diagonal UPML tensor in the global Cartesian
  basis, mirror of `geode_core::build_anisotropic_pml_tensor_diag`:
  real isotropic `n²` in the dielectric, `1` in the vacuum gap; in the
  shell `ε_α = (1/s) r̂_α² + s (1 − r̂_α²)` with
  `s = 1 − jσ(r_c)/ω`, `σ(r) = σ₀ clamp((r−R_PML_INNER)/(R_BUFFER−R_PML_INNER), 0, 1)²`,
  `ω = k₀_ref`. For this profile `s_r = s_t`, so the full Sacks tensor's
  off-diagonals are identically zero and the diagonal-only kernel is
  **exact**.
- **Complex eigenvalue spectrum**: lowest `spurious_dim + 8 = 39`
  eigenvalues of the tensor-ε pencil, `|Re(λ)|` ascending.
- **Physical band**: lowest 5 modes past the d⁰-rank spurious split.
  Ground band: λ ≈ 1.930 − 0.0073j, 1.997 − 0.0069j, 2.060 − 0.0046j
  (the mesh-split TM_1,1 triplet), then a gap to λ ≈ 3.31 (TE_1,1 /
  TM_2,1 band).
- **Strict cross-IR mode window** (`strict_mode_window_len = 3`): the
  closed TM_1,1 triplet (multiplicity 2l+1 = 3). Per the #160
  cluster-closure convention the tight-tolerance window must end at a
  spectral gap — never bisect a degenerate multiplet. Taking all 5
  stored physical modes would cut into the next band; positions [3, 4]
  are still compared (dense-vs-dense sees the whole spectrum) but are
  outside the closed-cluster claim.
- **J.1 analytic anchor**: `analytic_tm11_k ≈ 1.3034341302750476`
  re-exported from the Phase J.1 catalogue. The lowest physical mode's
  `Re(k) ≈ 1.38929` classifies as TM_1,1 at **6.59 %** relative error
  — inside the documented 8 % coarse-mesh acceptance band of
  `mie_sphere.rs`.
- **Q tripwire**: `q_factor_lowest_physical ≈ 264.6` and
  `q_median_tm11_triplet ≈ 288.2`, both far above the
  `Q_LOWER_BAND_TM11 = 1.5` PML-misconfiguration tripwire (σ₀ drift /
  mask break / vacuum-gap removal detection).
- **σ₀ = 0 PEC anchor** (`sigma_zero_lowest_physical_re ≈ 1.926392`):
  at σ₀ = 0 the tensor collapses to the real isotropic scalar, so the
  anchor coincides numerically with the `sphere_pml_small` one; kept
  in-fixture for self-containment.

## Sign convention (differs from `sphere_pml_small`)

Physical modes carry **`Im(λ) < 0`** on this small mesh's anisotropic
tensor pencil (vs the scalar-PML `Im(λ) > 0` of PR #155). The radial
tensor entry carries `1/s_r` (`Im > 0`) while the transverse entries
carry `s_t` (`Im < 0`); which contribution wins is **mesh-dependent**
— the refined full-mesh sibling (`sphere_mie/`) shows `Im(λ) > 0`.
In both cases the sign is a property of the pencil, **not** a solver
choice — eigenvalues of a fixed complex-symmetric pencil are uniquely
determined (only eigenvector phase is ambiguous), and LAPACK ZGGEV and
faer QZ agree on it. Q stays sign-agnostic
(`Q = Re(k) / (2 |Im(k)|)`).

## Output fields (under `outputs`)

| Field                           | Shape  | Dtype  | Tolerance         | What it pins                                                        |
|---------------------------------|--------|--------|-------------------|---------------------------------------------------------------------|
| `n_nodes`                       | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on mesh I/O (= 48).                             |
| `n_tets`                        | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check (= 197).                                        |
| `n_edges`                       | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on edge enumeration (= 259).                    |
| `n_interior_edges`              | `[1]`  | `f64`  | `0.5` absolute    | Integer cross-check on PEC mask reduction (= 214).                  |
| `spurious_dim`                  | `[1]`  | `f64`  | `0.5` absolute    | Predicted gradient-kernel dim (= interior nodes = 31).              |
| `n_spurious_observed`           | `[1]`  | `f64`  | `0.5` absolute    | Algebraic d⁰-rank classifier output (= 31).                         |
| `eigenvalues_lowest_complex`    | `[39]` | `c128` | `5e-4` (on `|Δ|`) | Lowest `spurious_dim + 8` eigenvalues, `|Re(λ)|` ascending.         |
| `physical_eigenvalues_complex`  | `[5]`  | `c128` | `1e-4` (on `|Δ|`) | Lowest 5 physical modes. `Im(λ) < 0` (see sign note).               |
| `strict_mode_window_len`        | `[1]`  | `f64`  | `0.5` absolute    | Closed TM_1,1 triplet window (= 3, #160 cluster closure).           |
| `analytic_tm11_k`               | `[1]`  | `f64`  | `1e-9` absolute   | J.1 catalogue TM_1,1 root (the 8 % acceptance anchor).              |
| `lowest_physical_re_k`          | `[1]`  | `f64`  | `1e-4` absolute   | `Re(√λ)` of the lowest physical mode (≈ 1.38929).                   |
| `tm11_rel_err_lowest`           | `[1]`  | `f64`  | `1e-4` absolute   | Relative error vs analytic TM_1,1 (≈ 0.0659; must stay < 0.08).     |
| `q_factor_lowest_physical`      | `[1]`  | `f64`  | `5.0` absolute    | Q of lowest mode (≈ 264.6). See tolerance note below.               |
| `q_median_tm11_triplet`         | `[1]`  | `f64`  | `5.0` absolute    | Triplet median Q (≈ 288.2; mirrors the Burn-side Q-band test).      |
| `sigma_zero_lowest_physical_re` | `[1]`  | `f64`  | `5e-5` absolute   | Lowest physical Re(λ) at σ₀ = 0 (PEC collapse anchor).              |

## Input fields (under `inputs`)

| Field                 | Shape      | Dtype  | Description                                                       |
|-----------------------|------------|--------|--------------------------------------------------------------------|
| `mesh_path`           | `[0]`      | `f64`  | Relative path: `reference/fixtures/sphere_pml_small/sphere.msh`.   |
| `sigma_0`             | `[1]`      | `f64`  | UPML absorption strength (`5.0`).                                  |
| `k0_ref`              | `[1]`      | `f64`  | Reference wavenumber ω heuristic in `s = 1 − jσ/ω` (`2.0`).        |
| `r_sphere`            | `[1]`      | `f64`  | Inner dielectric sphere radius (`1.0`).                            |
| `r_pml_inner`         | `[1]`      | `f64`  | PML inner radius (`1.5`).                                          |
| `r_buffer`            | `[1]`      | `f64`  | Outer PEC wall radius (`2.0`).                                     |
| `n_index`             | `[1]`      | `f64`  | Refractive index inside the dielectric sphere (`1.5`).             |
| `epsilon_tensor_diag` | `[197, 3]` | `c128` | Per-tet diagonal UPML tensor, row-major `(tet, axis)` flattened.   |

## Q-factor tolerance note

Q ≈ 265 on the lowest mode with `Im(λ) ≈ −7.3e-3`, so
`dQ/dIm(λ) ≈ Q/|Im(λ)| ≈ 3.6e4` — a `1e-4` eigenvalue residual maps
to O(1) on Q. The `5.0` absolute floor (~2 % relative) is the
regression bound; the **load-bearing** assertion is the
`Q > Q_LOWER_BAND_TM11 = 1.5` tripwire, which a real PML
misconfiguration would trip by orders of magnitude.

## On-disk encoding for `c128`

Real–imag interleaved row-major flat arrays. See `reference/SCHEMA.md`
→ "Complex encoding (`c128`)". The `(197, 3)` tensor flattens row-major
to 591 complex = 1182 floats, matching the Burn-side per-tet
`[faer::c64; 3]` flat_map order.

## Reproducing the fixture

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_sphere_mie_small_baseline.py
```

The dense `scipy.linalg.eigvals` on the 214-DOF interior pencil takes
< 1 s; the generator also re-runs the σ₀ = 0 PEC collapse, so the whole
script is a few seconds.
