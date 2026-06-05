"""Generate the de Rham baseline fixture for issue #149.

Writes ``reference/fixtures/derham/baseline.json`` — golden output
fixture in the canonical schema (``reference/SCHEMA.md`` v1) plus the
de Rham-specific output fields described in
``reference/fixtures/derham/baseline.schema.md``.

The mesh fixture re-used here is
``reference/fixtures/sphere_pec/sphere.msh`` (Phase G's bundled
sphere-in-vacuum mesh, copy of
``crates/geode-core/tests/fixtures/sphere.msh``). Issue #149 deliberately
does not duplicate the .msh into the ``derham/`` directory because the
``baseline.json`` shipped here pins matrix-level identities that are
backend-agnostic — only the mesh + the operator definitions matter.

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_derham_baseline.py

What this fixture pins
======================

- **Cell counts** (n_nodes, n_edges, n_faces, n_tets) and the
  Euler characteristic.
- **d⁰, d¹, d² CSR payloads** — full (indptr, indices, data) triples
  after :meth:`scipy.sparse.csr_matrix.sort_indices` canonicalization.
  Bit-exact integer cross-check (no tolerance).
- **Compositional identities** d¹·d⁰ ≡ 0 and d²·d¹ ≡ 0 as integer
  ``nnz = 0`` after elimination.
- **Rank predictions** from Euler-characteristic arithmetic on a
  contractible 3-mesh (the bundled sphere fixture is a 3-ball).
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/numpy -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "derham"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
MESH_PATH = REPO_ROOT / "reference" / "fixtures" / "sphere_pec" / "sphere.msh"

sys.path.insert(0, str(HERE))
from derham import (  # noqa: E402
    _read_msh_tets,
    build_edges,
    build_faces,
    curl_map,
    divergence_map,
    euler_ranks,
    gradient_map,
)


def _git_commit() -> str:
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=REPO_ROOT,
            stderr=subprocess.DEVNULL,
        )
        return out.decode().strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def _csr_field(description: str, arr, *, dtype: str = "f64") -> dict:
    """Build a schema-v1 output field dict for a flat 1D integer array.

    Stored as ``f64`` (per schema v1's lack of an ``i32`` loader path on
    the Rust side); the values themselves are integers so the
    ``tolerance_abs = 0.5`` strict-integer convention applies.
    """
    arr = np.asarray(arr).astype(np.int64)
    return {
        "shape": [int(arr.shape[0])],
        "dtype": dtype,
        "description": description,
        "tolerance_abs": 0.5,
        "data": arr.tolist(),
    }


def _scalar_field(name: str, value: int) -> dict:
    return {
        "shape": [1],
        "dtype": "f64",
        "description": name,
        "tolerance_abs": 0.5,
        "data": [int(value)],
    }


def _shape_field(name: str, shape: tuple[int, int]) -> dict:
    return {
        "shape": [2],
        "dtype": "f64",
        "description": name,
        "tolerance_abs": 0.5,
        "data": [int(shape[0]), int(shape[1])],
    }


def main():
    print(f"Reading bundled sphere fixture: {MESH_PATH}")
    nodes, tets = _read_msh_tets(MESH_PATH)
    n_nodes = int(nodes.shape[0])
    n_tets = int(tets.shape[0])

    edges = build_edges(tets)
    faces = build_faces(tets)
    n_edges = int(edges.shape[0])
    n_faces = int(faces.shape[0])

    d0 = gradient_map(n_nodes, edges)
    d1 = curl_map(edges, faces)
    d2 = divergence_map(tets, faces)

    # Canonicalize: ensure indices are sorted within each row.
    d0.sort_indices()
    d1.sort_indices()
    d2.sort_indices()

    # Compositional identities — sparse products, eliminate zeros.
    d1_d0 = (d1 @ d0).tocsr()
    d1_d0.eliminate_zeros()
    d2_d1 = (d2 @ d1).tocsr()
    d2_d1.eliminate_zeros()

    print(f"  n_nodes={n_nodes}, n_edges={n_edges}, n_faces={n_faces}, n_tets={n_tets}")
    print(f"  d0 shape={d0.shape}, nnz={d0.nnz}")
    print(f"  d1 shape={d1.shape}, nnz={d1.nnz}")
    print(f"  d2 shape={d2.shape}, nnz={d2.nnz}")
    print(f"  d1@d0 nnz={d1_d0.nnz} (should be 0)")
    print(f"  d2@d1 nnz={d2_d1.nnz} (should be 0)")

    if d1_d0.nnz != 0:
        raise RuntimeError(
            f"d¹ · d⁰ is not exactly zero (nnz = {d1_d0.nnz}); "
            "the NumPy de Rham reference is broken — sign convention "
            "drift somewhere in build_edges / build_faces / curl_map."
        )
    if d2_d1.nnz != 0:
        raise RuntimeError(
            f"d² · d¹ is not exactly zero (nnz = {d2_d1.nnz}); "
            "the NumPy de Rham reference is broken — sign convention "
            "drift somewhere in build_tet_faces / divergence_map."
        )

    ranks = euler_ranks(n_nodes, n_edges, n_faces, n_tets)
    print(
        f"  Euler χ = {ranks['euler_chi']}; ranks: d0={ranks['rank_d0']}, "
        f"d1={ranks['rank_d1']}, d2={ranks['rank_d2']}"
    )

    # Sanity check: measured ranks (dense SVD on integer matrices)
    # should match Euler predictions exactly. We don't ship these as
    # output fields (the Rust harness can verify against the same
    # NumPy SVD if it wants), but bail loudly here if the bundled
    # mesh isn't actually a closed contractible domain.
    r0 = int(np.linalg.matrix_rank(d0.toarray().astype(np.float64)))
    r1 = int(np.linalg.matrix_rank(d1.toarray().astype(np.float64)))
    r2 = int(np.linalg.matrix_rank(d2.toarray().astype(np.float64)))
    if (r0, r1, r2) != (ranks["rank_d0"], ranks["rank_d1"], ranks["rank_d2"]):
        raise RuntimeError(
            f"measured ranks ({r0}, {r1}, {r2}) disagree with Euler "
            f"predictions ({ranks['rank_d0']}, {ranks['rank_d1']}, "
            f"{ranks['rank_d2']}); the bundled sphere fixture may not "
            "be a closed contractible 3-mesh as assumed."
        )

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "derham/sphere_n774_d0_d1_d2",
        "description": (
            "Discrete de Rham complex (d⁰, d¹, d²) on the bundled sphere "
            "fixture — issue #149, parent epic #88, Phase I bridge. Bit-"
            "exact integer cross-check of geode_core::derham::{gradient_map, "
            "curl_map, divergence_map} against the NumPy reference in "
            "reference/numpy/derham.py."
        ),
        "units": "dimensionless integer incidence matrices",
        "inputs": {
            "mesh_path": {
                "shape": [0],
                "dtype": "f64",
                "description": (
                    "Mesh fixture (relative to repo root): "
                    "reference/fixtures/sphere_pec/sphere.msh — bundled "
                    f"sphere-in-vacuum mesh, {n_nodes} nodes, {n_tets} tets. "
                    "Same .msh consumed by Phase G's sphere_pec.py."
                ),
                "data": [],
            },
        },
        "outputs": {
            # Cell counts + Euler characteristic.
            "n_nodes": _scalar_field("Number of mesh nodes.", n_nodes),
            "n_edges": _scalar_field(
                "Number of mesh edges (TetMesh::edges dedup order).", n_edges
            ),
            "n_faces": _scalar_field(
                "Number of mesh faces (TetMesh::faces dedup order).", n_faces
            ),
            "n_tets": _scalar_field("Number of mesh tets.", n_tets),
            "euler_chi": _scalar_field(
                "Euler characteristic χ = n_nodes − n_edges + n_faces − n_tets. "
                "Equals 1 for a contractible 3-ball.",
                ranks["euler_chi"],
            ),
            # d⁰ — discrete gradient.
            "d0_shape": _shape_field("d⁰ shape [n_edges, n_nodes].", d0.shape),
            "d0_nnz": _scalar_field("d⁰ nnz (= 2 · n_edges).", d0.nnz),
            "d0_indptr": _csr_field("d⁰ CSR indptr (row pointers).", d0.indptr),
            "d0_indices": _csr_field(
                "d⁰ CSR column indices, sorted ascending within each row.",
                d0.indices,
            ),
            "d0_data": _csr_field(
                "d⁰ CSR data — signed integers in {-1, +1}.", d0.data
            ),
            # d¹ — discrete curl.
            "d1_shape": _shape_field("d¹ shape [n_faces, n_edges].", d1.shape),
            "d1_nnz": _scalar_field("d¹ nnz (= 3 · n_faces).", d1.nnz),
            "d1_indptr": _csr_field("d¹ CSR indptr (row pointers).", d1.indptr),
            "d1_indices": _csr_field(
                "d¹ CSR column indices, sorted ascending within each row.",
                d1.indices,
            ),
            "d1_data": _csr_field(
                "d¹ CSR data — signed integers in {-1, +1}.", d1.data
            ),
            # d² — discrete divergence.
            "d2_shape": _shape_field("d² shape [n_tets, n_faces].", d2.shape),
            "d2_nnz": _scalar_field("d² nnz (= 4 · n_tets).", d2.nnz),
            "d2_indptr": _csr_field("d² CSR indptr (row pointers).", d2.indptr),
            "d2_indices": _csr_field(
                "d² CSR column indices, sorted ascending within each row.",
                d2.indices,
            ),
            "d2_data": _csr_field(
                "d² CSR data — signed integers in {-1, +1}.", d2.data
            ),
            # Compositional identities — d¹·d⁰ ≡ 0, d²·d¹ ≡ 0.
            "d1_d0_nnz": _scalar_field(
                "Bit-exact d¹ · d⁰ ≡ 0 (nnz after eliminate_zeros).", 0
            ),
            "d2_d1_nnz": _scalar_field(
                "Bit-exact d² · d¹ ≡ 0 (nnz after eliminate_zeros).", 0
            ),
            # Rank predictions (Euler-arithmetic on contractible 3-ball).
            "rank_d0": _scalar_field(
                "rank(d⁰) = n_nodes − 1 (β_0 = 1 on a connected mesh).",
                ranks["rank_d0"],
            ),
            "rank_d1": _scalar_field(
                "rank(d¹) = n_edges − n_nodes + 1 (β_1 = 0 on a ball).",
                ranks["rank_d1"],
            ),
            "rank_d2": _scalar_field(
                "rank(d²) = n_faces − n_edges + n_nodes − 1 (β_2 = 0 on a ball).",
                ranks["rank_d2"],
            ),
        },
        "provenance": {
            "source": (
                "reference/numpy/derham.py — NumPy reimplementation of "
                "geode_core::derham::{gradient_map, curl_map, divergence_map} "
                "with matched sign conventions (TET_LOCAL_EDGES, "
                "TET_LOCAL_FACES, simplicial boundary alternation)."
            ),
            "verified_against": (
                "crates/geode-validation/tests/derham_numpy_reference.rs — "
                "bit-exact integer CSR equality between Burn and NumPy."
            ),
            "issue": (
                f"#149 (Epic #88, Phase I bridge); generated at git "
                f"commit {_git_commit()}"
            ),
        },
    }

    with FIXTURE_PATH.open("w") as fh:
        json.dump(fixture, fh, indent=2)
        fh.write("\n")
    print(f"Wrote {FIXTURE_PATH}")
    print(f"  {FIXTURE_PATH.stat().st_size / 1024:.1f} KB")


if __name__ == "__main__":
    main()
