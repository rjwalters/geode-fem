# `sphere_mie/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
**full-mesh anisotropic-UPML Mie** fixture (issue #171, Epic #88
Phase J.2).

This is the NumPy reference for the exact configuration the Burn-side
Mie acceptance (`crates/geode-core/tests/mie_sphere.rs`) runs: the
bundled refined sphere (774 nodes / 3335 tets) with the diagonal
anisotropic UPML tensor at `σ₀ = 5.0`, `k₀_ref = 2.0`, `n = 1.5`,
anchored to the Phase J.1 analytic Mie-root catalogue
(`reference/fixtures/mie_roots/baseline.json`). The small-mesh sibling
(`sphere_mie_small/`) is the default-CI gate; this fixture's Burn
cross-check is release-gated and `#[ignore]`-marked (faer 0.24 complex
GEVD takes 60+ minutes on the ~3300-DOF interior pencil).

## Mesh

`reference/fixtures/sphere_pml/sphere.msh` (774 nodes / 3335 tets /
4512 edges / 3300 interior DOFs) — byte-identical to the bundled
`crates/geode-core/tests/fixtures/sphere.msh` that `mie_sphere.rs`
loads via `read_sphere_fixture()`.

## What this fixture pins

- **Anisotropic tensor ε** (`epsilon_tensor_diag`, shape `(3335, 3)`,
  dtype `c128`): per-tet diagonal UPML tensor, mirror of
  `geode_core::build_anisotropic_pml_tensor_diag` (see the
  `sphere_mie_small` schema notes for the profile formula; for the
  current `s_r = s_t` profile the diagonal-only kernel is exact).
- **Complex eigenvalue spectrum**: lowest `spurious_dim + 8 = 376`
  eigenvalues of the tensor-ε pencil, `|Re(λ)|` ascending.
- **Physical band** (observed, reproducing the Burn-side documented
  numbers from issues #49/#54):
  - TM_1,1 triplet: λ ≈ 1.51066 + 0.05534j, 1.51147 + 0.05541j,
    1.51191 + 0.05603j → `Re(k)` ≈ 1.2293 at **5.69 / 5.66 / 5.65 %**
    of the analytic TM_1,1 (`k ≈ 1.30343`) — matching the documented
    "observed ≈ 5.7 %" anisotropic-UPML acceptance calibration.
  - Then a gap to the TE_1,1 band at λ ≈ 3.486 + 0.390j (1.03 % /
    0.94 % of the analytic TE_1,1).
- **Strict cross-IR mode window** (`strict_mode_window_len = 3`): the
  closed TM_1,1 triplet. Cluster closure (#160) is extremely clean on
  this refined mesh: triplet spread 0.0013 vs gap-to-next-band 1.974.
- **Q tripwire**: `q_factor_lowest_physical ≈ 27.31`,
  `q_median_tm11_triplet ≈ 27.29` — matching the Burn-side documented
  "median Q ≈ 27" for the anisotropic UPML (vs Q ≈ 5.8 scalar PML),
  far above the `Q_LOWER_BAND_TM11 = 1.5` tripwire.
- No in-fixture σ₀ = 0 anchor: the full mesh's σ₀ = 0 collapse is
  already pinned by the `sphere_pec` / `sphere_pml` fixtures, and an
  in-fixture anchor would double the multi-minute eigensolve cost.

## Sign convention

Physical modes carry **`Im(λ) > 0`** on this refined mesh (the
small-mesh sibling shows `Im(λ) < 0`). The UPML tensor's radial entry
carries `1/s_r` (`Im > 0`) and the transverse entries `s_t`
(`Im < 0`); which contribution wins is mesh-dependent. The sign is a
property of the pencil — not a solver choice — and LAPACK ZGGEV and
faer QZ agree on it. Q is sign-agnostic.

## Output fields (under `outputs`)

| Field                          | Shape   | Dtype  | Tolerance         | What it pins                                                    |
|--------------------------------|---------|--------|-------------------|------------------------------------------------------------------|
| `n_nodes`                      | `[1]`   | `f64`  | `0.5` absolute    | Integer cross-check on mesh I/O (= 774).                        |
| `n_tets`                       | `[1]`   | `f64`  | `0.5` absolute    | Integer cross-check (= 3335).                                   |
| `n_edges`                      | `[1]`   | `f64`  | `0.5` absolute    | Integer cross-check on edge enumeration (= 4512).               |
| `n_interior_edges`             | `[1]`   | `f64`  | `0.5` absolute    | Integer cross-check on PEC mask reduction (= 3300).             |
| `spurious_dim`                 | `[1]`   | `f64`  | `0.5` absolute    | Predicted gradient-kernel dim (= interior nodes = 368).         |
| `n_spurious_observed`          | `[1]`   | `f64`  | `0.5` absolute    | Algebraic d⁰-rank classifier output (= 368).                    |
| `eigenvalues_lowest_complex`   | `[376]` | `c128` | `5e-5` (on `|Δ|`) | Lowest `spurious_dim + 8` eigenvalues, `|Re(λ)|` ascending.     |
| `physical_eigenvalues_complex` | `[5]`   | `c128` | `1e-5` (on `|Δ|`) | Lowest 5 physical modes. `Im(λ) > 0` (see sign note).           |
| `strict_mode_window_len`       | `[1]`   | `f64`  | `0.5` absolute    | Closed TM_1,1 triplet window (= 3, #160 cluster closure).       |
| `analytic_tm11_k`              | `[1]`   | `f64`  | `1e-9` absolute   | J.1 catalogue TM_1,1 root (the 8 % acceptance anchor).          |
| `lowest_physical_re_k`         | `[1]`   | `f64`  | `1e-5` absolute   | `Re(√λ)` of the lowest physical mode (≈ 1.22930).               |
| `tm11_rel_err_lowest`          | `[1]`   | `f64`  | `1e-5` absolute   | Relative error vs analytic TM_1,1 (≈ 0.0569; must stay < 0.08). |
| `q_factor_lowest_physical`     | `[1]`   | `f64`  | `1e-1` absolute   | Q of lowest mode (≈ 27.31). See tolerance note below.           |
| `q_median_tm11_triplet`        | `[1]`   | `f64`  | `1e-1` absolute   | Triplet median Q (≈ 27.29; mirrors the Burn-side Q-band test).  |

## Input fields (under `inputs`)

| Field                 | Shape       | Dtype  | Description                                                     |
|-----------------------|-------------|--------|------------------------------------------------------------------|
| `mesh_path`           | `[0]`       | `f64`  | Relative path: `reference/fixtures/sphere_pml/sphere.msh`.       |
| `sigma_0`             | `[1]`       | `f64`  | UPML absorption strength (`5.0`).                                |
| `k0_ref`              | `[1]`       | `f64`  | Reference wavenumber ω heuristic in `s = 1 − jσ/ω` (`2.0`).      |
| `r_sphere`            | `[1]`       | `f64`  | Inner dielectric sphere radius (`1.0`).                          |
| `r_pml_inner`         | `[1]`       | `f64`  | PML inner radius (`1.5`).                                        |
| `r_buffer`            | `[1]`       | `f64`  | Outer PEC wall radius (`2.0`).                                   |
| `n_index`             | `[1]`       | `f64`  | Refractive index inside the dielectric sphere (`1.5`).           |
| `epsilon_tensor_diag` | `[3335, 3]` | `c128` | Per-tet diagonal UPML tensor, row-major `(tet, axis)` flattened. |

## Tolerance notes

The full-mesh pencil is better conditioned than the small one (same
discussion as `sphere_pml` vs `sphere_pml_small`, #158): the physical
band holds the Phase H.1 `1e-5` floor (measured Burn-vs-NumPy max
`|Δ|` = 8.2e-7 on the 5 physical modes). The **full slice** is held at
`5e-5` instead: the tensor-ε pencil's near-zero spurious cluster
carries slightly larger faer 0.24 QZ vs LAPACK ZGGEV residuals than
the scalar pencil — measured worst offender 1.83e-5 absolute at the
cluster edge (index 367; 11 of 376 modes exceed 1e-5, all inside the
spurious cluster). Q ≈ 27.3 with `Im(λ) ≈ 0.055`
gives `dQ/dIm(λ) ≈ Q/|Im(λ)| ≈ 500`, mapping a `1e-5` λ-residual to
~5e-3 on Q; `1e-1` absolute is the defensible floor. The load-bearing
assertion remains the `Q > 1.5` tripwire.

## On-disk encoding for `c128`

Real–imag interleaved row-major flat arrays per `reference/SCHEMA.md`.
The `(3335, 3)` tensor flattens row-major to 10005 complex = 20010
floats, matching the Burn-side per-tet `[faer::c64; 3]` flat_map order.

## Reproducing the fixture

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_sphere_mie_baseline.py   # ~25 min: dense LAPACK ZGGEV on the 3300-DOF pencil
```
