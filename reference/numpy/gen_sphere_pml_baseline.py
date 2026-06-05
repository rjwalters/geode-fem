"""Generate the **scaffolding stub** sphere-PML baseline fixture
(issue #145, Phase H, parent epic #88).

Writes ``reference/fixtures/sphere_pml/baseline.json`` — a
schema-conformant fixture exercising the new ``c128`` on-disk encoding
and the complex comparator path in ``geode-validation``.

This generator is intentionally **stub-quality** on numerics. The role
of this file is to land the cross-backend infrastructure (loader +
schema + comparator) for the Phase H rollout; the per-backend
references (#146 NumPy / #147 Julia / #148 JAX) own the full PML
numerical content and will replace the stubbed fields with real
eigensolver output.

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_sphere_pml_baseline.py

What this stub pins
===================

- **Mesh I/O**: ``n_nodes`` and ``n_tets`` from the bundled
  ``sphere.msh`` (same mesh as ``sphere_pec/``).
- **PML profile parameters**: ``sigma_0 = 5.0`` (matches
  ``crates/geode-core/tests/sphere_pml_eigenmode.rs``), plus the
  ``R_SPHERE / R_PML_INNER / R_BUFFER`` radii.
- **Stub complex permittivity slice**: 4 entries of
  ``epsilon_r_complex`` (full ``[n_tets]`` shape declared in the
  schema doc, populated end-to-end by H.1). The 4 entries cover the
  three regions: dielectric (real 2.25), vacuum gap (real 1.0),
  PML shell (complex 1 - 5j ramp endpoint).
- **Synthetic complex eigenvalues**: 2 entries of
  ``eigenvalues_lowest_complex`` chosen so the smoke comparator can
  round-trip them — one near-zero spurious-cluster sentinel, one
  with the expected ``Im(λ) < 0`` PML signature.
- **Derived q_factor**: ``-Re(λ)/(2·Im(λ))`` for the second
  synthetic eigenvalue. Sanity output for human review.

On-disk c128 encoding
=====================

Real-imag interleaved row-major flat arrays (NOT NumPy's binary
``np.complex128`` layout — JSON is text). See
``reference/SCHEMA.md`` → "Complex encoding (c128)" for the spec.
The serialization helper below uses
``np.asarray(z, dtype=np.complex128).view(np.float64)`` which lays
out exactly the required interleave on a contiguous array.

Out of scope
============

- Full PML eigensolve (owned by H.1, #146).
- ``epsilon_r_complex`` over the full ``n_tets`` mesh (H.1).
- Anisotropic UPML reference impls (deferred sub-phase).
- TF-Java / ONNX Phase H (paired-real-imag deferral, see issue #145).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/numpy -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "sphere_pml"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
MESH_PATH = FIXTURE_DIR / "sphere.msh"

# Mesh constants. Source of truth: crates/geode-core/src/mesh/sphere.rs.
# Copied here (rather than parsed) because the stub doesn't need the
# full mesh I/O path — the per-backend impls will read these from the
# .msh fixture directly.
R_SPHERE = 1.0
R_PML_INNER = 1.5
R_BUFFER = 2.0
N_INDEX = 1.5
SIGMA_0 = 5.0  # matches sphere_pml_eigenmode.rs

# Tolerance: 1e-6 on |Δ| for the synthetic complex eigenvalue stub.
# Real H.1 output will likely settle at 1e-5 (~7e-6 relative at the
# physical band floor) but the stub uses exact round-number values, so
# 1e-6 is a defensible scaffolding tolerance.
COMPLEX_TOL_ABS = 1.0e-6
Q_FACTOR_TOL_ABS = 1.0e-6


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


def _interleave_c128(z: np.ndarray) -> list[float]:
    """Serialize a complex128 array to the canonical real-imag interleaved
    flat list described in ``reference/SCHEMA.md``."""
    z = np.ascontiguousarray(z, dtype=np.complex128)
    # `.view(np.float64)` reinterprets the underlying memory as pairs of
    # f64s laid out [re0, im0, re1, im1, ...]. This is the exact wire
    # format the Rust loader expects.
    return z.view(np.float64).tolist()


def _read_mesh_counts(mesh_path: Path) -> tuple[int, int]:
    """Parse just the node and tet counts from a Gmsh `.msh` file.

    The full reference NumPy mesh loader lives in
    ``reference/numpy/mesh.py``; the stub only needs the two header
    counts, so we do a minimal parse here to avoid pulling in the full
    importer at scaffolding time. H.1 will swap this for the real
    loader.
    """
    n_nodes = 0
    n_elems = 0
    with open(mesh_path, "r") as f:
        text = f.read()
    # Gmsh ASCII v4 format: $Nodes ... numEntityBlocks numNodes ...
    if "$Nodes" in text:
        nodes_block = text.split("$Nodes", 1)[1].split("$EndNodes", 1)[0].strip()
        # First line has 4 integers: numEntityBlocks numNodes minNodeTag maxNodeTag
        first = nodes_block.splitlines()[0].split()
        n_nodes = int(first[1])
    # Count tets in $Elements block. Element type 4 = 4-node tet in Gmsh.
    if "$Elements" in text:
        elems_block = text.split("$Elements", 1)[1].split("$EndElements", 1)[0].strip()
        lines = elems_block.splitlines()
        # First line: numEntityBlocks numElements ...
        idx = 1
        while idx < len(lines):
            header = lines[idx].split()
            if len(header) >= 4:
                ent_dim, _ent_tag, ent_type, num_in_block = (
                    int(header[0]),
                    int(header[1]),
                    int(header[2]),
                    int(header[3]),
                )
                if ent_dim == 3 and ent_type == 4:
                    n_elems += num_in_block
                idx += 1 + num_in_block
            else:
                idx += 1
    return n_nodes, n_elems


def main() -> None:
    print(f"Reading mesh counts from {MESH_PATH} ...")
    if not MESH_PATH.is_file():
        raise FileNotFoundError(
            f"sphere mesh not found at {MESH_PATH}; "
            "expected to be checked in alongside the fixture"
        )
    n_nodes, n_tets = _read_mesh_counts(MESH_PATH)
    print(f"  n_nodes={n_nodes}, n_tets={n_tets}")

    # ---------------- Stub epsilon_r_complex ---------------- #
    #
    # 4-entry slice illustrating the three regions. Full [n_tets] vector
    # lands with H.1 — the schema doc declares the full shape, and the
    # per-backend impls will populate it from the actual centroid radii.
    eps_stub = np.array(
        [
            N_INDEX**2 + 0j,            # dielectric: 2.25 + 0j
            1.0 + 0j,                   # vacuum gap: 1 + 0j
            1.0 - SIGMA_0 * 1j,         # PML ramp endpoint: 1 - 5j
            1.0 - 0.25 * SIGMA_0 * 1j,  # PML mid-shell sample: 1 - 1.25j
        ],
        dtype=np.complex128,
    )

    # ---------------- Synthetic complex eigenvalue stub ---------------- #
    #
    # Two entries — one in the spurious near-zero cluster, one with the
    # expected `Im(λ) < 0` PML absorption signature on a physical band
    # mode (round-number values so the smoke test can assert exact
    # round-trip equality without ARPACK noise).
    eigenvalues_complex = np.array(
        [
            1.0e-13 + 0j,    # spurious cluster sentinel
            1.42 - 0.1j,     # physical-band stub with PML loss
        ],
        dtype=np.complex128,
    )

    # Q-factor sign convention: Q = -Re(λ) / (2·Im(λ)) — positive Q
    # for absorbing modes under exp(+jωt). For (1.42 - 0.1j):
    # Q = -1.42 / (2·(-0.1)) = 7.1
    lam = eigenvalues_complex[1]
    q_factor = float(-lam.real / (2.0 * lam.imag))
    print(f"  synthetic lowest-physical λ = {lam}, Q = {q_factor:.4f}")

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "sphere_pml/n774_pml_eigenmode_stub",
        "description": (
            "Scalar-isotropic PML sphere eigenmode SCAFFOLDING STUB "
            "(issue #145, parent epic #88, Phase H). Exercises the c128 "
            "on-disk encoding and complex comparator end-to-end. Full "
            "numerical content lands with H.1 (#146 NumPy) / H.2 (#147 "
            "Julia) / H.3 (#148 JAX)."
        ),
        "units": (
            "λ = k² (inverse-length squared) with negative-imaginary "
            "convention under exp(+jωt); dimensionless mesh coordinates"
        ),
        "inputs": {
            "mesh_path": {
                "shape": [0],
                "dtype": "f64",
                "description": (
                    "reference/fixtures/sphere_pml/sphere.msh — copy of "
                    "the bundled sphere fixture (same mesh as sphere_pec/)."
                ),
                "data": [],
            },
            "sigma_0": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "PML absorption strength; matches the value used in "
                    "crates/geode-core/tests/sphere_pml_eigenmode.rs."
                ),
                "data": [SIGMA_0],
            },
            "r_sphere": {
                "shape": [1],
                "dtype": "f64",
                "description": "Inner dielectric sphere radius.",
                "data": [R_SPHERE],
            },
            "r_pml_inner": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "PML inner radius — start of the absorbing layer."
                ),
                "data": [R_PML_INNER],
            },
            "r_buffer": {
                "shape": [1],
                "dtype": "f64",
                "description": "Outer PEC wall radius.",
                "data": [R_BUFFER],
            },
            "n_index": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Refractive index inside the dielectric sphere; "
                    "ε_r = n² = 2.25 inside."
                ),
                "data": [N_INDEX],
            },
            "epsilon_r_complex": {
                # STUB shape: 4 illustrative entries (one per region) at
                # scaffolding time. H.1 (#146) replaces this with the
                # full per-tet vector at shape [n_tets]; see the schema
                # doc's "epsilon_r_complex at scaffolding time" note.
                # Declared shape matches the actual data so the
                # `Fixture::input_c128` length check stays meaningful.
                "shape": [int(eps_stub.shape[0])],
                "dtype": "c128",
                "description": (
                    "Per-tet complex relative permittivity. STUB at "
                    "scaffolding time: 4 illustrative entries covering "
                    "the dielectric / vacuum-gap / PML-shell regions. "
                    "Full [n_tets] vector lands with H.1 (#146). "
                    "On-disk: real-imag interleaved flat array per "
                    "reference/SCHEMA.md."
                ),
                "data": _interleave_c128(eps_stub),
            },
        },
        "outputs": {
            "n_nodes": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of mesh nodes. Stored as f64; strict-equality "
                    "semantics (tolerance < 1)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_nodes)],
            },
            "n_tets": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of mesh tets. Stored as f64; strict-equality."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_tets)],
            },
            "eigenvalues_lowest_complex": {
                "shape": [2],
                "dtype": "c128",
                "description": (
                    "STUB: 2 synthetic complex eigenvalues exercising the "
                    "c128 comparator path. Entry 0 is a spurious-cluster "
                    "sentinel (near zero); entry 1 is a physical-band "
                    "stub (1.42 - 0.1j) with the expected Im(λ) < 0 PML "
                    "signature. Full physical spectrum lands with H.1 (#146)."
                ),
                "tolerance_abs": COMPLEX_TOL_ABS,
                "data": _interleave_c128(eigenvalues_complex),
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Derived quality factor Q = -Re(λ)/(2·Im(λ)) for the "
                    "lowest physical mode in eigenvalues_lowest_complex. "
                    "Sign convention: positive Q indicates an absorbing "
                    "mode under exp(+jωt) with Im(λ) < 0."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_factor],
            },
        },
        "provenance": {
            "source": (
                f"reference/numpy/gen_sphere_pml_baseline.py @ commit "
                f"{_git_commit()} ; numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}."
                f"{sys.version_info.micro} ; SCAFFOLDING STUB - full "
                f"numerics deferred to H.1 (#146)"
            ),
            "verified_against": (
                "crates/geode-validation/tests/sphere_pml_schema_smoke.rs "
                "(scaffolding smoke: loader + complex comparator round-trip)"
            ),
            "issue": "#145 (parent epic #88, Phase H scaffolding)",
        },
    }

    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")

    print()
    print(f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes)")


if __name__ == "__main__":
    main()
