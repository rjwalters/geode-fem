# `sphere_pec/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
vector-Nédélec sphere-PEC eigenmode slice (issue #118, parent epic #88,
sibling of #117). Analog of `reference/fixtures/cube_cavity/baseline.schema.md`.

## What the fixture pins

- **Mesh**: the bundled `sphere.msh` (copy of
  `crates/geode-core/tests/fixtures/sphere.msh`, ~140 KB), referenced
  from the fixture's `inputs.mesh_path.description`. The `.msh` lives
  alongside `baseline.json` so the NumPy reference is runnable from a
  fresh checkout (`reference/` does not reach into `crates/`).
- **Mesh constants**: `n_index = 1.5`, `r_sphere = 1.0`,
  `r_buffer = 2.0` — pinned as input fields for documentation. The
  Burn-side constants `R_SPHERE`, `R_BUFFER`, `PHYS_*` in
  `crates/geode-core/src/mesh/sphere.rs` are the source of truth.
- **ε_r assignment**: full per-tet relative permittivity vector
  (`epsilon_r`, length `n_tets = 3335`). `ε = n² = 2.25` inside the
  dielectric, `1.0` in the vacuum buffer (both `PHYS_VACUUM_GAP` and
  `PHYS_PML_SHELL` — this phase is PEC without PML).
- **Edge enumeration**: full `[n_tets, 6]` `tet_edge_idx` and
  `tet_edge_sign` arrays. Per the open-question 1 resolution, stored as
  two separate i64 arrays (not a flattened interleaved single array) for
  legibility. Total ~40 KB per array on the 3335-tet fixture.
- **PEC mask**: full `[n_edges]` `interior_mask` boolean array plus
  the integer summary `n_interior_edges` and the predicted
  gradient-kernel dimension `spurious_dim` (= number of nodes strictly
  inside the outer PEC wall).
- **K_int / M_int diagnostics**: Frobenius norms, full per-DOF diagonals
  (length `n_interior_edges`), per-row nnz histograms, and symmetry
  residuals. The histograms + diagonals + symmetry residual triple is
  the sparsity-pattern fingerprint per open-question 2 resolution
  (avoiding full CSR `indptr`/`indices` parity, which is brittle to
  scipy's COO->CSR sort order).
- **Spectrum**: full `eigenvalues_lowest` slice of length
  `spurious_dim + 8 = 376` from `scipy.sparse.linalg.eigsh(K_int, k,
  M=M_int, sigma=0, which='LM')`. The comparator runs the spurious-
  mode filter on Burn's spectrum and asserts agreement with the
  NumPy-observed `n_spurious_observed` (371 on this fixture; see the
  "Spurious-mode filter discrepancy" note below).
- **Physical eigenvalues**: lowest 5 physical eigenvalues after
  spurious filtering. On the bundled fixture: λ ≈ {3.272, 3.277, 3.280,
  3.285, 3.293} (so `k = √λ` ≈ {1.81, 1.81, 1.81, 1.81, 1.81} — a
  near-degenerate cluster).

## Output fields (under `outputs`)

| Field                      | Shape                  | Tolerance              | What it pins                                                                       |
|----------------------------|------------------------|------------------------|------------------------------------------------------------------------------------|
| `n_nodes`                  | `[1]`                  | `0.5` absolute         | Integer cross-check on mesh I/O.                                                   |
| `n_tets`                   | `[1]`                  | `0.5` absolute         | Integer cross-check on mesh I/O.                                                   |
| `epsilon_r`                | `[n_tets]`             | `1e-14` absolute       | Per-tet ε_r vector. f64 ULP × max value cross-check.                              |
| `n_edges`                  | `[1]`                  | `0.5` absolute         | Integer cross-check on edge enumeration.                                           |
| `tet_edge_idx`             | `[n_tets, 6]`          | `0.5` absolute         | Per-tet global edge indices in canonical `TET_LOCAL_EDGES` order. Bit-exact.       |
| `tet_edge_sign`            | `[n_tets, 6]`          | `0.5` absolute         | Per-tet edge orientation sign in `{-1, +1}`. Bit-exact integer cross-check.        |
| `n_interior_edges`         | `[1]`                  | `0.5` absolute         | Integer cross-check on PEC reduction.                                              |
| `interior_mask`            | `[n_edges]`            | `0.5` absolute         | Full boolean edge mask. Bit-exact cross-check on the PEC boundary classifier.      |
| `spurious_dim`             | `[1]`                  | `0.5` absolute         | Predicted gradient-kernel dimension = interior node count.                          |
| `k_int_frobenius`          | `[1]`                  | `1e-8` absolute        | Frobenius norm of K_int (assembly sub-stage diagnostic).                            |
| `m_int_frobenius`          | `[1]`                  | `1e-9` absolute        | Frobenius norm of M_int (ε-scaled mass sub-stage diagnostic).                       |
| `k_int_diag`               | `[n_interior_edges]`   | `1e-9` absolute        | Per-DOF stiffness diagonal. Catches per-row assembly drift.                         |
| `m_int_diag`               | `[n_interior_edges]`   | `1e-10` absolute       | Per-DOF mass diagonal. Catches per-tet ε broadcast regression.                      |
| `k_int_nnz_histogram`      | `[max_nnz_per_row+1]`  | `0.5` absolute         | Per-row nnz histogram of K_int. Sparsity-pattern fingerprint.                       |
| `m_int_nnz_histogram`      | `[max_nnz_per_row+1]`  | `0.5` absolute         | Per-row nnz histogram of M_int. Should equal `k_int_nnz_histogram` by construction. |
| `k_int_symmetry_residual`  | `[1]`                  | `1e-10` absolute       | `max(|K - K^T|)` — exact zero modulo COO->CSR float roundoff.                       |
| `m_int_symmetry_residual`  | `[1]`                  | `1e-12` absolute       | `max(|M - M^T|)` — same as K.                                                       |
| `eigenvalues_lowest`       | `[spurious_dim + 8]`   | `1e-6` absolute        | Full lowest-spectrum slice (spurious cluster + physical band).                      |
| `n_spurious_observed`      | `[1]`                  | `0.5` absolute         | Largest-gap heuristic output. Bit-exact integer cross-check on edge sign + masking. |
| `best_gap`                 | `[1]`                  | `1e-6` absolute        | Ratio at the largest gap (filter heuristic diagnostic).                             |
| `physical_eigenvalues`     | `[5]`                  | `1e-5` absolute        | Lowest 5 physical eigenvalues (post-spurious filter). 1e-5 abs ≈ 7e-6 rel at λ≈1.4. |

## Spurious-mode filter discrepancy

On the bundled 774-node sphere fixture, the largest-relative-gap
heuristic (verbatim port from `sphere_pec_eigenmode.rs:194-215`) gives
`n_spurious = 371`, *not* the predicted `spurious_dim = 368`. **This
disagreement is reproduced bit-exactly by both Burn and NumPy** — it is
not a port bug; both backends compute the same eigenvalues and run the
same heuristic. The mechanism:

- Eigenvalues 0..367 cluster near zero (gradient kernel, f64 roundoff
  scaled by the shift-invert residual).
- Eigenvalues 368, 369, 370 sit at λ ≈ 1.42 (a true 3-fold-degenerate
  physical mode at `2l + 1 = 3`).
- Eigenvalue 371 jumps to λ ≈ 3.27 (next physical band).

The heuristic's gap calculation treats the spurious-cluster→1.42
transition as an **absolute** jump (`a < 1e-9` branch, `gap = b ≈ 1.42`)
and the 1.42→3.27 transition as a **relative** jump (`gap = 3.27 / 1.42
= 2.30`). 2.30 > 1.42, so the heuristic picks the latter, classifying
the 1.42 cluster as spurious.

The Burn-side test `sphere_pec_eigenmode_spectrum` asserts `n_spurious
== spurious_dim` (= 368) and `best_gap > 100` — both of which fail on
this fixture for the same reason. The Burn test was calibrated against
an earlier, smaller sphere fixture ("313 nodes / ~1226 tets" per the
parent issue body) where the heuristic worked. This is tracked as a
follow-up Burn-side calibration issue and is **out of scope for this
PR** — the cross-backend agreement on the *observed* heuristic output
is what this fixture and comparator establish.

## Open-question resolutions (from issue #118)

1. **Edge-table comparison strategy** — two separate i64 arrays
   (`tet_edge_idx [n_tets, 6]`, `tet_edge_sign [n_tets, 6]`), full
   bit-exact comparison. Pre-flattened interleaved storage would
   compress the JSON ~negligibly while losing readability.
2. **Sparsity-pattern comparison** — per-row nnz histogram + diagonal
   match + symmetry residual `(|K - K^T|.max())`. Avoids binding to
   scipy's COO->CSR sort order (a re-spin against a different scipy
   could permute equal-key triplets within a row).
3. **Subspace-overlap tolerance under mesh-asymmetry split** — deferred
   to a follow-up. The current comparator does not compare
   eigenvectors; the spurious-count + lowest-5-physical-eigenvalue
   integer/scalar match is the load-bearing cross-check for this PR.
   Eigenvector subspace overlap on the sphere fixture also needs the
   "measure cluster dim from gaps, not from 2l+1" logic that the cube
   cavity test uses — straightforward port but adds ~150 LOC; tracked
   as a follow-up sub-issue.
4. **Port `mie::merged_roots` to NumPy** — NO for this PR. The Riccati-
   Bessel root-finder is ~250 lines of Rust (`crates/geode-core/src/mie.rs`)
   and porting it adds a third independent root-finding implementation
   without buying additional cross-check value. The Mie pairing
   anchors *both* backends to physics; a NumPy port would anchor NumPy
   to Mie a second time. Tracked as a follow-up sub-issue.
5. **CI gate for `--ignored` release-mode eigensolve** — deferred.
   Mirrors the cube-cavity convention: non-eigensolve sub-stages run
   under default `cargo test`; eigensolve gated to release mode via
   `#[ignore]`. Adding the matrix CI step is a separate sub-issue.

## Regeneration

The eigensolve uses `scipy.sparse.linalg.eigsh` with shift-and-invert
at `sigma=0`, deterministic on a pinned scipy version (we do not ship
eigenvectors so the ARPACK sign-pinning step is unnecessary).

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_sphere_pec_baseline.py
```

Re-runs against a different scipy version may produce slightly
different ARPACK convergence noise in the spurious near-zero cluster
(eigenvalues 0..367 here are O(1e-13)) but the physical band agrees to
well under `1e-6` relative.

## Cross-platform tolerance floor

The bundled fixture is larger than the cube cavity (3300 vs. 729 DOFs;
≈4.5x more floating-point traffic in Frobenius accumulators). The
absolute tolerances in the table above are loose-but-real:

- `k_int_diag` at `1e-9` and `m_int_diag` at `1e-10` give ~100x
  headroom over the natural f64-vs-f64 roundoff floor on `ndarray`.
- The `eigenvalues_lowest` field at `1e-6` absolute is loose at the
  spurious-cluster end (any number under `1e-6` is "near zero" for the
  cluster comparison) but tight at the physical-band end (λ ≈ 1.4
  ⇒ rel ≈ 7e-7).
- Physical eigenvalues at `1e-5` absolute → ~7e-6 relative at the
  physical-band floor, well inside the `1e-6` acceptance criterion
  with headroom.

If a new platform reproduces this test outside these bounds, the
`SPHERE_PEC_SUBSTAGE_DIFF` lines emitted by the `--ignored` eigensolve
test (`print_substage_diff` in
`sphere_pec_numpy_reference.rs`) feed the cross-platform table the same
way `CUBE_CAVITY_SUBSTAGE_DIFF` does for the cube cavity. A CI matrix
job mirroring `.github/workflows/cube-cavity-tolerance.yml` is the
natural follow-up (open question 5 above).

## Versioning

The fixture follows `schema_version = "1"` (the canonical
`reference/SCHEMA.md`). No schema extensions beyond the conventions
already established by `cube_cavity/baseline.json`.
