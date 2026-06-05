# `derham/baseline.json` schema notes

Per-fixture extension of the canonical `reference/SCHEMA.md` v1 for the
discrete de Rham complex (`d⁰`, `d¹`, `d²`) on the bundled sphere
fixture (issue #149, parent epic #88, Phase I bridge).

## What the fixture pins

Bit-exact integer cross-check between the Burn-side
`geode_core::derham::{gradient_map, curl_map, divergence_map}` and the
NumPy reference in `reference/numpy/derham.py`. The Rust operators are
materialized as `SparseColMat<usize, f64>` with values exactly
`{-1.0, 0.0, +1.0}`; the NumPy operators are `scipy.sparse.csr_matrix`
with `int64` data in `{-1, +1}`. The matrices are mathematically
*integer* — bit-exactness is the natural cross-check, not a tolerance
question (Hiptmair §3, §4; Arnold–Falk–Winther §1.2).

The fixture also pins:

- **Cell counts**: `n_nodes`, `n_edges`, `n_faces`, `n_tets` — integer
  cross-checks on mesh I/O and the edge/face deduplication order.
- **Euler characteristic**: `euler_chi` = `n_nodes - n_edges + n_faces
  - n_tets`. For the bundled sphere (a 3-ball, contractible) this is
  `1` exactly.
- **Operator shapes + nnz**: `d0_shape`, `d0_nnz`, etc., as direct
  diagnostics of the sparse pattern.
- **Operator CSR data** (the load-bearing payload): each of `d0`, `d1`,
  `d2` ships its `indptr`, `indices`, and `data` arrays after
  `sort_indices()` canonicalization. The Rust harness loads these and
  asserts entry-wise equality of `(indptr, indices, data)` against the
  CSR projection of the Burn operator.
- **Compositional identities**: `d1_d0_nnz` and `d2_d1_nnz` are pinned
  to `0` — the bit-exact `d¹ · d⁰ ≡ 0` and `d² · d¹ ≡ 0` exactness
  identities of the de Rham complex, computed in NumPy via
  `(d1 @ d0).eliminate_zeros()` and checked on the Burn side via the
  same sparse product followed by an entrywise zero assertion.
- **Rank predictions**: `rank_d0`, `rank_d1`, `rank_d2` from
  Euler-characteristic arithmetic on a closed contractible 3-mesh:

      rank(d⁰) = n_nodes - 1            (β_0 = 1)
      rank(d¹) = n_edges - n_nodes + 1  (β_1 = 0)
      rank(d²) = n_faces - n_edges + n_nodes - 1  (β_2 = 0)

  These match measured SVD ranks of the loaded NumPy CSR matrices to
  the bit (rank computed on the integer-valued matrix has no
  floating-point ambiguity at the sphere fixture's scale).

## Output fields (under `outputs`)

All fields are stored as `f64` per schema v1 conventions, but the
underlying values are integers (or integer-comparable counts with
`tolerance_abs = 0.5`).

| Field             | Shape           | Tolerance     | What it pins                                                   |
|-------------------|-----------------|---------------|----------------------------------------------------------------|
| `n_nodes`         | `[1]`           | `0.5`         | Mesh I/O sanity (774 on the bundled fixture).                  |
| `n_edges`         | `[1]`           | `0.5`         | Edge enumeration (`TetMesh::edges` dedup order).               |
| `n_faces`         | `[1]`           | `0.5`         | Face enumeration (`TetMesh::faces` dedup order).               |
| `n_tets`          | `[1]`           | `0.5`         | Mesh I/O sanity (3335 on the bundled fixture).                 |
| `euler_chi`       | `[1]`           | `0.5`         | `χ = n_nodes − n_edges + n_faces − n_tets = 1` for a ball.     |
| `d0_shape`        | `[2]`           | `0.5`         | `[n_edges, n_nodes]` = `[4512, 774]`.                          |
| `d0_nnz`          | `[1]`           | `0.5`         | `2 * n_edges` = `9024` (every row has exactly 2 nonzeros).     |
| `d0_indptr`       | `[n_edges + 1]` | `0.5`         | Row-pointer array of `d⁰` in row-sorted CSR.                   |
| `d0_indices`      | `[d0_nnz]`      | `0.5`         | Column indices, sorted ascending within each row.              |
| `d0_data`         | `[d0_nnz]`      | `0.5`         | Signed integer values in `{-1, +1}`.                           |
| `d1_shape`        | `[2]`           | `0.5`         | `[n_faces, n_edges]` = `[7074, 4512]`.                         |
| `d1_nnz`          | `[1]`           | `0.5`         | `3 * n_faces` = `21222` (every row has exactly 3 nonzeros).    |
| `d1_indptr`       | `[n_faces + 1]` | `0.5`         | Row-pointer array of `d¹` in row-sorted CSR.                   |
| `d1_indices`      | `[d1_nnz]`      | `0.5`         | Column indices, sorted ascending within each row.              |
| `d1_data`         | `[d1_nnz]`      | `0.5`         | Signed integer values in `{-1, +1}`.                           |
| `d2_shape`        | `[2]`           | `0.5`         | `[n_tets, n_faces]` = `[3335, 7074]`.                          |
| `d2_nnz`          | `[1]`           | `0.5`         | `4 * n_tets` = `13340` (every row has exactly 4 nonzeros).     |
| `d2_indptr`       | `[n_tets + 1]`  | `0.5`         | Row-pointer array of `d²` in row-sorted CSR.                   |
| `d2_indices`      | `[d2_nnz]`      | `0.5`         | Column indices, sorted ascending within each row.              |
| `d2_data`         | `[d2_nnz]`      | `0.5`         | Signed integer values in `{-1, +1}`.                           |
| `d1_d0_nnz`       | `[1]`           | `0.5`         | `0` — bit-exact `d¹ · d⁰ ≡ 0`.                                  |
| `d2_d1_nnz`       | `[1]`           | `0.5`         | `0` — bit-exact `d² · d¹ ≡ 0`.                                  |
| `rank_d0`         | `[1]`           | `0.5`         | `n_nodes − 1 = 773` (β_0 = 1 on a connected mesh).             |
| `rank_d1`         | `[1]`           | `0.5`         | `n_edges − n_nodes + 1 = 3739` (β_1 = 0 on a ball).            |
| `rank_d2`         | `[1]`           | `0.5`         | `n_faces − n_edges + n_nodes − 1 = 3335` (β_2 = 0 on a ball).  |

The `tolerance_abs = 0.5` convention is the standard schema-v1 idiom
for "integer cross-check": any non-zero error in an integer-valued
quantity exceeds `0.5`, so the assertion is effectively bit-exact
integer equality.

## Why no eigenvector-class fields

The de Rham operators are integer incidence matrices — there are no
eigenvectors / norms / Frobenius / diagonals to compare. The cross-
check is just "do the integer patterns match?" There is no
floating-point tolerance question, which is precisely the reason
Issue #149 picks this slice as a high-leverage bridge: the Phase G
cross-backend ladder (NumPy → JAX → Julia → TF-Java → ONNX) is overkill
for an algebraic identity. The bit-exact integer agreement between
Burn and NumPy is the spec-anchoring claim.

## Reproduction

```sh
cd reference
python3 -m pip install -r numpy/requirements.txt
python3 numpy/gen_derham_baseline.py
```

## Cross-check harness

The Rust-side cross-check lives at
`crates/geode-validation/tests/derham_numpy_reference.rs`. It loads the
bundled `sphere.msh` via `geode_core::read_sphere_fixture`, builds the
three operators via the public `derham::*` API, projects them into row-
sorted CSR, loads this baseline.json fixture, and asserts:

1. Cell counts agree (`n_nodes`, `n_edges`, `n_faces`, `n_tets`).
2. Each operator's CSR `(indptr, indices, data)` matches the fixture
   entrywise.
3. `(d¹ · d⁰)` and `(d² · d¹)` produce all-zero CSR matrices on the
   Burn side (algebraic exactness; pinned cross-side via NumPy too).
4. Rank predictions match the Euler-formula values.

Bit-exact integer matches throughout; no floating-point tolerance is
applied to the de Rham payload.
