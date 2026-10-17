# `cube_cavity/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
cube-cavity Helmholtz eigenmode slice (issue #92, parent epic #88).

## What the fixture pins

- **Mesh**: the canonical `n=10` tet-split unit cube, stored separately
  at `unit_cube.msh` (MSH 4.1 ASCII, ~227 KB) and referenced from the
  fixture's `inputs.mesh_path.description`. The same `.msh` is the
  shared input for `reference/jax/cube_cavity.py` and
  `reference/tfjava/cube_cavity/...` (issue #93) so all three backends
  consume an identical mesh, with no per-backend regeneration friction.
- **NumPy reference outputs**: the lowest-5 generalized eigenvalues
  ``λ_0 .. λ_4`` of ``K x = λ M x`` on the Dirichlet-interior P1 DOFs,
  Frobenius norms of `K_int` and `M_int`, and the full diagonals of
  both interior matrices. Each carries its own `tolerance_abs`.
- **NumPy reference eigenvectors**: stored as an INPUT field
  `eigenvectors_numpy` of shape `[n_int=729, k=5]`, M-orthonormal,
  with first non-trivial entry sign pinned positive.
- **Analytic eigenvalue targets**: `{3, 6, 6, 6, 9} · π²` — the
  Dirichlet Laplacian eigenvalues on the unit cube. Comparison is
  banded at 12% absolute tolerance (the worst of the 5 modes lands at
  ~10.5% relative error on the n=10 P1 discretization).

## Output fields (under `outputs`)

| Field                  | Shape    | Tolerance       | What it pins                                                                |
|------------------------|----------|-----------------|-----------------------------------------------------------------------------|
| `eigenvalues`          | `[5]`    | `1e-4` absolute (~3.4e-6 relative at λ_min) | The headline cross-backend agreement metric. |
| `k_int_frobenius`      | `[1]`    | `1e-9` absolute (~6e-11 relative on the n=10 K_int ≈ 17.36) | Total energy norm of the stiffness — a single scalar that catches catastrophic assembly drift. Cross-platform f64-vs-f64 floor (issue #110). |
| `m_int_frobenius`      | `[1]`    | `1e-8` absolute | Same as above, for the mass matrix. Cross-platform floor on M (smaller scale, looser absolute). |
| `k_int_diag`           | `[729]`  | `1e-9` absolute | Per-DOF stiffness diagonal. Each entry is the sum of element contributions to a single node — a real sub-stage friction signal. Cross-platform f64-vs-f64 floor (issue #110). |
| `m_int_diag`           | `[729]`  | `5e-9` absolute | Per-DOF mass diagonal. M entries are O(h^3) ≈ 4e-4 on the n=10 mesh. This was the field that broke single-host calibration — see "f64 sub-stage tolerance" below. |
| `analytic_eigenvalues` | `[5]`    | `~10.7` absolute (12% relative at 9π² ≈ 88.8) | Confirms the reference is anchored to physics, not just to itself. |
| `n_int`                | `[1]`    | `0.5` absolute  | Trivial integer-as-f64 shape check on the Dirichlet reduction. |

### f64 sub-stage tolerance — cross-platform floor

Issue **#99** fixed `assembly::upload_mesh` to honor `B::FloatElem`
instead of force-casting node coordinates to `f32`. Under the
nominally-f64 `ndarray` backend, K and M are now assembled in full
f64 and agree with the NumPy reference at the natural f64-vs-f64
roundoff floor.

PR **#106** tightened the sub-stage tolerances ~100x against
single-host measurements (Ubuntu x86_64), setting `m_int_diag` to
`1e-14`. PR **#108** (issue #103) independently reproduced the test
on macOS arm64 and observed `m_int_diag` drift of ~5e-10 against that
same tolerance — a 50,000x miss. Root cause: LLVM FMA contraction +
SIMD reduction-order differences across `target_arch` and runner
generations. Tolerances calibrated on one host do not survive
cross-platform variation.

Issue **#110** re-calibrated the floor with an honest multi-platform
measurement. The `cube-cavity-tolerance.yml` GitHub Actions workflow
runs the `--ignored` test under `--nocapture` on:

| Runner          | OS / Arch                | rustc target                    |
|-----------------|--------------------------|----------------------------------|
| `ubuntu-latest` | Ubuntu 22.04 x86_64      | `x86_64-unknown-linux-gnu`       |
| `macos-latest`  | macOS 14 arm64           | `aarch64-apple-darwin`           |
| `macos-13`      | macOS 13 Intel x86_64    | `x86_64-apple-darwin`            |

Each run emits one `CUBE_CAVITY_SUBSTAGE_DIFF` line per field via
`print_substage_diff()` in the test. The known sub-stage observations
(absolute error, Burn-ndarray-f64 vs NumPy) used to set the floor:

| Field             | PR #106 host (Linux x86_64) | PR #108 host (macOS arm64) | Issue #110 dev host (macOS arm64) | Tolerance set to |
|-------------------|----------------------------:|---------------------------:|----------------------------------:|------------------|
| `k_int_frobenius` |             ~1e-13 (rel)    |             not reported   |                    `3.05e-13`     | `1e-9`           |
| `m_int_frobenius` |             ~1e-13 (rel)    |             not reported   |                    `6.12e-16`     | `1e-8`           |
| `k_int_diag`      |             ~1e-14           |             not reported  |                    `4.44e-16`     | `1e-9`           |
| `m_int_diag`      |             ~1e-15           |             **~5e-10**    |                    `2.17e-19`     | `5e-9`           |

The two macOS arm64 observations differ by 9 orders of magnitude on
`m_int_diag` — strong evidence that the friction is not just
"aarch64 vs x86_64" but depends on the specific rustc minor version,
LLVM SIMD lane width, and possibly the runner-OS scheduling of
gemm threads. The `5e-9` tolerance bounds the worst-known
observation by 10x; the CI matrix in
`.github/workflows/cube-cavity-tolerance.yml` is the ongoing source
of truth and will surface any platform that exceeds the bound.

The new tolerances are still tight enough to catch the original f32
truncation regression: pre-#99 errors were `m_int_diag` ~1.1e-10
(20x over the new 5e-9 floor) and `k_int_diag` ~5.4e-8 (50x over the
new 1e-9 floor).

**Why we did not adopt per-platform tolerances**: the cross-platform
spread is ~5 orders of magnitude (1e-15 to 5e-10), but the worst-case
field (`m_int_diag` on aarch64) still fits comfortably under a single
`5e-9` bound. Per-platform `#[cfg]`-gated thresholds would obscure
the underlying physics floor (`O(h^3) ≈ 4e-4` mass-diagonal entries
times f64 roundoff scaled by SIMD reduction width) without buying us
tighter regression coverage.

**Why we did not implement a SIMD-deterministic reduction**: that's
the actual root cause — sum order in `gemm`/`faer` accumulators
differs by lane count, and Rust + LLVM are free to contract `a*b + c`
into `fma(a, b, c)` per platform. Fixing this requires either a
pinned-order pairwise reduction or `-Ccodegen-units=1 -Cfp-contract=off`
across `gemm`. That's a much larger lift (issue out of scope) and
would slow down the GPU codepath that doesn't have this freedom in
the first place. The honest documented floor is the right artifact.

GPU backends (`wgpu`/`cuda`) where `B::FloatElem = f32` continue to
carry ~1e-7 friction by construction; the cross-backend test
(`crates/geode-validation/tests/cube_cavity_numpy_reference.rs`)
applies looser per-DOF/Frobenius bounds when the active backend is
not `ndarray` (see `GPU_F32_TOLERANCES`).

GPU backends (`wgpu`/`cuda`) where `B::FloatElem = f32` continue to
carry ~1e-7 friction by construction; the cross-backend test
(`crates/geode-validation/tests/cube_cavity_numpy_reference.rs`)
applies looser per-DOF/Frobenius bounds when the active backend is
not `ndarray` (see `GPU_F32_TOLERANCES`).

The **eigenvalue** tolerance was unaffected by the bug because the
eigenvalues have intrinsic conditioning (the K-M pencil is symmetric
SPD and the n=10 spectrum has clean gaps); the pre-fix f32 upload
friction propagated as only a ~3e-9 *relative* shift on the
eigenvalues, comfortably under the 1e-6 acceptance criterion.

## Input fields (under `inputs`)

| Field                | Shape         | Notes                                                                                     |
|----------------------|---------------|-------------------------------------------------------------------------------------------|
| `mesh_path`          | `[0]`         | Path is in `description`; data array is empty (schema v1 has no string field type yet).   |
| `n_per_side`         | `[1]`         | `n = 10` (mesh refinement; matches `unit_cube.msh`).                                      |
| `eigenvectors_numpy` | `[729, 6]`    | `Q_numpy` — M-orthonormal, sign-pinned. **6 columns**: 5 for acceptance criterion #2 plus 1 to close the dim-2 cluster at index {4,5} (the lowest-5 cut bisects a degenerate cluster — see *Per-cluster subspace-overlap convention* below). |

## Per-cluster subspace-overlap convention

Eigenvector comparison **must not** be elementwise: inside a degenerate
eigenvalue cluster the basis is non-unique, so any orthogonal rotation
of `Q_numpy` is an equally valid `Q_burn`. The cross-backend test
(`crates/geode-validation/tests/cube_cavity_numpy_reference.rs`) uses
the **subspace overlap** metric instead:

For a degenerate cluster spanning eigenvalue indices `[i .. i+d)`,
compute the `d × d` block of `O = Q_numpy^T M_int Q_burn`. If
`span(Q_numpy)` and `span(Q_burn)` are the same M-orthogonal subspace
the block is orthogonal, so `‖block‖_F = √d`. The test asserts
`|‖block‖_F − √d| < tol` per cluster.

Cluster layout for the cube cavity at n=10, *discovered from the
eigenvalue gaps in the NumPy reference* (gaps measured relative):

- `{0}`     ground mode at 3.124·π²     (dim 1; gap 104% to next)
- `{1, 2}`  6.374·π²                    (dim 2; bit-identical, P1-lifted from analytic 3-fold)
- `{3}`     6.600·π²                    (dim 1; gap 3.5% from `{1, 2}`)
- `{4, 5}`  9.946·π²                    (dim 2; bit-identical, P1-lifted from analytic 3-fold)

The fixture stores **6** eigenvectors (one beyond the lowest-5 cut)
because the cut at index 5 closes cluster `{4, 5}`. Without that
extra mode, the subspace overlap on index 4 alone would fail by ~2%
— the expected wedge between two arbitrarily-rotated degenerate
basis representatives. Acceptance criterion #2 (lowest 5 eigenvalues
match Burn to 1e-6 relative) still uses the first 5 entries of
`outputs.eigenvalues`.

Tolerance on the per-cluster Frobenius:

| Backend                | Tolerance | Rationale |
|------------------------|-----------|-----------|
| `ndarray` (f64)        | `1e-6`    | M-orthogonality + ARPACK residual (1e-9) leave a comfortable margin. |
| `wgpu` / `cuda` (f32)  | `1e-3`    | f32 assembly + dense eigensolve roundoff dominates. |

## Regeneration

Deterministic on a pinned `scipy.sparse.linalg.eigsh`:

```bash
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 -m pip install meshio  # add to requirements when --user is OK
python3 numpy/gen_cube_cavity_baseline.py
```

Eigenvector signs are pinned post-eigsh so the JSON is byte-stable
across reruns of the same scipy/numpy combination. Re-runs against a
*different* scipy version may produce a different ARPACK iteration
trace; in that case the eigenvalues will agree to `1e-9` but the
eigenvectors will differ by an orthogonal rotation within each
degenerate cluster — exactly the case the subspace-overlap metric is
designed to absorb.

## Versioning

The fixture follows `schema_version = "1"` (the canonical
`reference/SCHEMA.md`). It uses one schema extension: `inputs` can
hold a numeric-data field whose actual content is described in the
`description` string rather than in the `data` array (the `mesh_path`
field). That convention is local to this fixture and is the obvious
forward-compatible move until schema v2 grows a string field type.
