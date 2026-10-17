"""Generate `reference/fixtures/sphere_pec/jax_baseline.json` from the JAX pipeline.

Produces a ``schema_version: "1"`` fixture (compatible with
``crates/geode-validation/src/fixture.rs::Fixture``) containing the
sphere-PEC Nédélec pipeline outputs from the JAX reference implementation.
The fixture is cross-checked against the NumPy baseline at generation time.

Usage
=====

    python3 reference/jax/gen_sphere_pec_fixture.py
    python3 reference/jax/gen_sphere_pec_fixture.py \\
        --out reference/fixtures/sphere_pec/jax_baseline.json

JAX is required. Install with: pip install "jax[cpu]"

If JAX is not available, use ``--stub`` to write a placeholder fixture:

    python3 reference/jax/gen_sphere_pec_fixture.py --stub

Epic #88 / Phase G.3 / Issue #128.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/ -> repo root
# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)


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


def _numpy_baseline_path() -> Path:
    return REPO_ROOT / "reference" / "fixtures" / "sphere_pec" / "baseline.json"


def _default_out_path() -> Path:
    return REPO_ROOT / "reference" / "fixtures" / "sphere_pec" / "jax_baseline.json"


def _load_numpy_baseline():
    """Load the NumPy baseline from the pre-generated fixture JSON."""
    path = _numpy_baseline_path()
    if not path.exists():
        return None
    with open(path) as f:
        return json.load(f)


def write_stub_fixture(out_path: Path):
    """Write a placeholder fixture when JAX is not available.

    The fixture has the correct schema shape but all numeric values are
    copied from the NumPy baseline (or set to zero if baseline is missing).
    A ``TODO`` comment in the description flags it for regeneration.
    """
    print("Writing stub fixture (JAX not available — must be regenerated with JAX)")

    numpy_baseline = _load_numpy_baseline()

    if numpy_baseline is not None:
        # Use NumPy baseline values as placeholders
        np_out = numpy_baseline["outputs"]
        n_nodes = int(np_out["n_nodes"]["data"][0])
        n_tets = int(np_out["n_tets"]["data"][0])
        n_edges = int(np_out["n_edges"]["data"][0])
        n_interior_edges = int(np_out["n_interior_edges"]["data"][0])
        spurious_dim = int(np_out["spurious_dim"]["data"][0])
        k_int_frobenius = float(np_out["k_int_frobenius"]["data"][0])
        m_int_frobenius = float(np_out["m_int_frobenius"]["data"][0])
        k_int_diag = list(np_out["k_int_diag"]["data"])
        m_int_diag = list(np_out["m_int_diag"]["data"])
        eigenvalues_lowest = list(np_out["eigenvalues_lowest"]["data"])
        physical_eigenvalues = list(np_out["physical_eigenvalues"]["data"])
        note = (
            "STUB: JAX was not available at generation time. Values are copied "
            "from the NumPy baseline. Regenerate with JAX installed: "
            "python3 reference/jax/gen_sphere_pec_fixture.py"
        )
    else:
        # No baseline available — write zeros with correct shapes
        n_nodes, n_tets, n_edges = 774, 3335, 4512
        n_interior_edges, spurious_dim = 3300, 368
        k_int_frobenius, m_int_frobenius = 0.0, 0.0
        k_int_diag = [0.0] * n_interior_edges
        m_int_diag = [0.0] * n_interior_edges
        eigenvalues_lowest = [0.0] * (spurious_dim + 8)
        physical_eigenvalues = [0.0] * 5
        note = (
            "STUB: JAX was not available and no NumPy baseline was found. "
            "All numeric values are zero placeholders. "
            "Regenerate with JAX installed: "
            "python3 reference/jax/gen_sphere_pec_fixture.py"
        )

    fixture = _build_fixture_dict(
        n_nodes=n_nodes,
        n_tets=n_tets,
        n_edges=n_edges,
        n_interior_edges=n_interior_edges,
        spurious_dim=spurious_dim,
        k_int_frobenius=k_int_frobenius,
        m_int_frobenius=m_int_frobenius,
        k_int_diag=k_int_diag,
        m_int_diag=m_int_diag,
        eigenvalues_lowest=eigenvalues_lowest,
        physical_eigenvalues=physical_eigenvalues,
        verified_note=note,
    )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"Wrote stub fixture to {out_path} ({os.path.getsize(out_path)} bytes)")


def _build_fixture_dict(
    n_nodes: int,
    n_tets: int,
    n_edges: int,
    n_interior_edges: int,
    spurious_dim: int,
    k_int_frobenius: float,
    m_int_frobenius: float,
    k_int_diag: list,
    m_int_diag: list,
    eigenvalues_lowest: list,
    physical_eigenvalues: list,
    verified_note: str,
) -> dict:
    """Build the canonical schema-v1 fixture dict."""
    return {
        "schema_version": "1",
        "fixture_id": "sphere_pec/n774_pec_eigenmode_jax",
        "description": (
            "JAX reference for the sphere-PEC vector-Nédélec eigenmode pipeline "
            "(Epic #88 / Phase G.3 / Issue #128). Per-element curl-curl and "
            "ε-mass assembly runs under jax.vmap/jit; global COO scatter and "
            "SciPy shift-and-invert eigensolve remain in NumPy (dynamic shapes, "
            "no sparse generalized eigensolver in JAX). "
            f"{verified_note}"
        ),
        "units": "dimensionless (k^2, with sphere geometry in arbitrary length units)",
        "inputs": {
            "mesh_path": {
                "shape": [1],
                "dtype": "str",
                "description": (
                    "Path to the bundled Gmsh .msh fixture "
                    "(reference/fixtures/sphere_pec/sphere.msh)."
                ),
                "data": ["reference/fixtures/sphere_pec/sphere.msh"],
            },
            "n_index": {
                "shape": [1],
                "dtype": "f64",
                "description": "Refractive index inside the dielectric sphere.",
                "data": [1.5],
            },
            "r_sphere": {
                "shape": [1],
                "dtype": "f64",
                "description": "Inner dielectric sphere radius.",
                "data": [1.0],
            },
            "r_buffer": {
                "shape": [1],
                "dtype": "f64",
                "description": "Outer vacuum buffer radius (PEC wall location).",
                "data": [2.0],
            },
        },
        "outputs": {
            "n_nodes": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of mesh nodes (integer cross-check).",
                "tolerance_abs": 0.5,
                "data": [float(n_nodes)],
            },
            "n_tets": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of tetrahedra (integer cross-check).",
                "tolerance_abs": 0.5,
                "data": [float(n_tets)],
            },
            "n_edges": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of global Nédélec edges (integer cross-check).",
                "tolerance_abs": 0.5,
                "data": [float(n_edges)],
            },
            "n_interior_edges": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of interior Nédélec DOFs after PEC reduction "
                    "(integer cross-check)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_interior_edges)],
            },
            "spurious_dim": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Algebraic spurious-mode dimension = rank(d0_interior) "
                    "(integer cross-check; Issue #124)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(spurious_dim)],
            },
            "k_int_frobenius": {
                "shape": [1],
                "dtype": "f64",
                "description": "Frobenius norm of K_int (curl-curl, post-Dirichlet).",
                "tolerance_abs": 1.0e-8,
                "data": [float(k_int_frobenius)],
            },
            "m_int_frobenius": {
                "shape": [1],
                "dtype": "f64",
                "description": "Frobenius norm of M_int (epsilon-mass, post-Dirichlet).",
                "tolerance_abs": 1.0e-8,
                "data": [float(m_int_frobenius)],
            },
            "k_int_diag": {
                "shape": [n_interior_edges],
                "dtype": "f64",
                "description": (
                    f"Per-DOF stiffness diagonal, shape [{n_interior_edges}]. "
                    "Catches per-row assembly drift."
                ),
                "tolerance_abs": 1.0e-9,
                "data": k_int_diag,
            },
            "m_int_diag": {
                "shape": [n_interior_edges],
                "dtype": "f64",
                "description": (
                    f"Per-DOF mass diagonal, shape [{n_interior_edges}]. "
                    "Catches per-tet epsilon broadcast regression."
                ),
                "tolerance_abs": 1.0e-10,
                "data": m_int_diag,
            },
            "eigenvalues_lowest": {
                "shape": [spurious_dim + 8],
                "dtype": "f64",
                "description": (
                    f"Full lowest-spectrum slice (spurious cluster + physical band), "
                    f"shape [{spurious_dim + 8}]. From scipy.sparse.linalg.eigsh "
                    f"with shift-and-invert at sigma=0."
                ),
                "tolerance_abs": 1.0e-8,
                "data": eigenvalues_lowest,
            },
            "eigenvalues_physical": {
                "shape": [5],
                "dtype": "f64",
                "description": (
                    "Lowest 5 physical eigenvalues (post-spurious filter). "
                    "Lambda = k^2; expected ~{1.420, 1.420, 1.421, 3.272, 3.277}."
                ),
                "tolerance_abs": 1.0e-6,
                "data": physical_eigenvalues,
            },
        },
        "provenance": {
            "source": "reference/jax/sphere_pec.py (Epic #88 / Phase G.3 / Issue #128)",
            "generator_commit": _git_commit(),
            "verified_against": verified_note,
        },
    }


def main():
    parser = argparse.ArgumentParser(
        description="Generate JAX sphere-PEC Nédélec baseline fixture"
    )
    parser.add_argument(
        "--out",
        default=str(_default_out_path()),
        help="Output JSON path (default: reference/fixtures/sphere_pec/jax_baseline.json)",
    )
    parser.add_argument(
        "--stub",
        action="store_true",
        help="Write a placeholder fixture without running JAX (use when JAX is not installed)",
    )
    parser.add_argument(
        "--tol",
        type=float,
        default=1e-6,
        help="Max allowed absolute deviation of JAX vs NumPy eigenvalues (default 1e-6)",
    )
    args = parser.parse_args()
    out_path = Path(args.out)

    if args.stub:
        write_stub_fixture(out_path)
        return

    # Try to import JAX
    try:
        from reference.jax.sphere_pec import solve_sphere_pec_jax, JaxSpherePecResult
    except ImportError as e:
        print(f"ERROR: Could not import JAX pipeline: {e}")
        print("Install JAX with: pip install 'jax[cpu]'")
        print("Or use --stub to write a placeholder fixture.")
        sys.exit(1)

    print("Solving sphere-PEC with JAX pipeline (Phase G.3)...")
    result: JaxSpherePecResult = solve_sphere_pec_jax()

    print(f"  n_nodes = {result.n_nodes}, n_tets = {result.n_tets}")
    print(f"  n_edges = {result.n_edges}, n_interior_edges = {result.n_interior_edges}")
    print(f"  spurious_dim = {result.spurious_dim}")
    print(f"  K_int Frobenius = {result.k_int_frobenius:.8e}")
    print(f"  M_int Frobenius = {result.m_int_frobenius:.8e}")
    print(f"  eigenvalues_lowest[0..5] = {result.eigenvalues_lowest[:5]}")
    print(f"  eigenvalues_physical = {list(result.eigenvalues_physical)}")

    # Cross-check against NumPy baseline
    numpy_baseline = _load_numpy_baseline()
    if numpy_baseline is None:
        print("\nWARNING: NumPy baseline not found; skipping cross-check.")
        verified_note = (
            "NumPy baseline not found at generation time; "
            "cross-check skipped."
        )
    else:
        np_phys = np.array(numpy_baseline["outputs"]["physical_eigenvalues"]["data"])
        jax_phys = np.array(result.eigenvalues_physical)
        phys_abs = np.abs(jax_phys - np_phys)
        phys_rel = phys_abs / np.maximum(np.abs(np_phys), 1.0)

        np_eig_all = np.array(numpy_baseline["outputs"]["eigenvalues_lowest"]["data"])
        jax_eig_all = np.array(result.eigenvalues_lowest)
        eig_abs = np.abs(jax_eig_all - np_eig_all)

        print("\nCross-check vs NumPy baseline:")
        print(f"  max |JAX - NumPy| physical eigenvalues = {phys_abs.max():.3e}")
        print(f"  max rel(JAX, NumPy) physical eigenvalues = {phys_rel.max():.3e}")
        print(f"  max |JAX - NumPy| lowest spectrum (all) = {eig_abs.max():.3e}")

        if phys_abs.max() > args.tol:
            print(
                f"\nERROR: JAX-NumPy physical eigenvalue disagreement {phys_abs.max():.3e} "
                f"exceeds tolerance {args.tol:.0e}."
            )
            print("Investigate before publishing baseline.")
            sys.exit(2)

        verified_note = (
            f"reference/numpy/sphere_pec.py baseline — physical eigenvalues agree to "
            f"{phys_rel.max():.1e} relative, full spectrum to "
            f"{eig_abs.max():.1e} absolute at fixture-gen time."
        )
        print(f"\nCross-check PASSED (JAX-NumPy max physical rel = {phys_rel.max():.1e})")

    fixture = _build_fixture_dict(
        n_nodes=result.n_nodes,
        n_tets=result.n_tets,
        n_edges=result.n_edges,
        n_interior_edges=result.n_interior_edges,
        spurious_dim=result.spurious_dim,
        k_int_frobenius=result.k_int_frobenius,
        m_int_frobenius=result.m_int_frobenius,
        k_int_diag=result.k_int_diag.tolist(),
        m_int_diag=result.m_int_diag.tolist(),
        eigenvalues_lowest=result.eigenvalues_lowest.tolist(),
        physical_eigenvalues=result.eigenvalues_physical.tolist(),
        verified_note=verified_note,
    )

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"\nWrote {out_path} ({os.path.getsize(out_path)} bytes)")
    print(f"  generator_commit = {_git_commit()}")


if __name__ == "__main__":
    main()
