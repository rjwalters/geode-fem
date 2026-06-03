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
| `k_int_frobenius`      | `[1]`    | `1e-6` absolute (~6e-8 relative on the n=10 K_int) | Total energy norm of the stiffness — a single scalar that catches catastrophic assembly drift. Tolerance set to absorb Burn's f32-upload friction (see note below). |
| `m_int_frobenius`      | `[1]`    | `1e-6` absolute | Same as above, for the mass matrix. |
| `k_int_diag`           | `[729]`  | `1e-7` absolute | Per-DOF stiffness diagonal. Each entry is the sum of element contributions to a single node — a real sub-stage friction signal. Tolerance absorbs the f32 upload (see below). |
| `m_int_diag`           | `[729]`  | `1e-7` absolute | Per-DOF mass diagonal. |
| `analytic_eigenvalues` | `[5]`    | `~10.7` absolute (12% relative at 9π² ≈ 88.8) | Confirms the reference is anchored to physics, not just to itself. |
| `n_int`                | `[1]`    | `0.5` absolute  | Trivial integer-as-f64 shape check on the Dirichlet reduction. |

### Why K_int / M_int sub-stage tolerances are loose

Burn's `assembly::upload_mesh` (`crates/geode-core/src/assembly.rs:83`)
truncates node coordinates to **f32** at the tensor-upload boundary
regardless of the active backend's `FloatElem`. The downstream
assembly tensors therefore carry f32 precision (~1e-7) even under the
nominally-f64 `ndarray` backend; faer upcasts to f64 too late to
recover precision. Observed maxima on the n=10 cube cavity:

| Quantity              | Burn-vs-NumPy max abs err | Tolerance set to |
|-----------------------|---------------------------|------------------|
| K_int diag            | ~5.4e-8                   | 1e-7             |
| K_int Frobenius       | ~2.0e-7                   | 1e-6             |
| M_int diag            | ~1.1e-10                  | 1e-7             |
| M_int Frobenius       | ~5.2e-10                  | 1e-6             |

This friction is tracked on issue #5 (whiteroom). Once `upload_mesh`
is fixed to honor `B::FloatElem`, the K/M sub-stage tolerances should
tighten to ~1e-12 on the ndarray backend (the natural f64-vs-f64
roundoff at this matrix size).

The **eigenvalue** tolerance is unaffected because the eigenvalues
have intrinsic conditioning (the K-M pencil is symmetric SPD and the
n=10 spectrum has clean gaps); the f32 upload friction propagates as
a ~3e-9 *relative* shift on the eigenvalues, comfortably under the
1e-6 acceptance criterion.

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
