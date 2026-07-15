# Nédélec H(curl) stiffness sparsity pattern — a COLAMD-poor test matrix

`nedelec_hcurl_102k.mtx.gz` — gzipped [MatrixMarket](https://math.nist.gov/MatrixMarket/formats.html)
coordinate pattern.

## What it is

The sparsity pattern of the lowest-order Nédélec (edge-element) H(curl) curl–curl
stiffness matrix `K` from geode-fem, on a structured cube tetrahedral mesh.

- `n = 102,024` (edge DOFs), `nnz = 1,615,752`, **structurally symmetric**.
- Values are all `1` — this is a *pattern* matrix; the connectivity is what
  fill-reducing orderings act on.

## Provenance / regeneration

Generated from geode-fem's public `assembly::nedelec::sparsity_pattern_from_tet_edges`
applied to `mesh::cube_tet_mesh(24, 1.0)` (82,944 tets), then written CSC → MatrixMarket.
geode-fem is a [Burn](https://burn.dev)-native, tensor-compiled FEM electromagnetics
solver (see the repository root).

## Why it's interesting (ordering behavior)

Fill under different column orderings, measured on this pattern:

| ordering | nnz(L+U), symbolic (pivoting off) | vs COLAMD |
|---|---:|---|
| COLAMD (faer / SuperLU default) | 340.2M | — |
| AMD / minimum-degree | 148.8M | 2.3× less |
| METIS nested dissection | 68.5M | **5.0× less** |

So COLAMD is a poor default for this symmetric-pattern 3D FEM matrix.

**But there's a twist.** On the *real* unsymmetric LU with partial pivoting (a
refined ~1.16M-DOF version of the same problem), COLAMD is the **robust** winner:
the symbolically-1.7×-lighter AMD ordering's fill blew up under pivoting and OOM'd
(>128 GB) where COLAMD completed (~92 GB). This is a matrix where the
**symbolic-fill-optimal ordering is *not* the real-LU-optimal ordering** — the
pivoting interaction dominates.

## Context

- Shared with the faer maintainer for ordering-algorithm research:
  <https://codeberg.org/sarah-quinones/faer/issues/307>.
- geode-fem custom fill-reducing LU ordering work: issues #543 / #544.
