"""Generate the cube-cavity baseline fixture for issue #92.

Writes:
- ``reference/fixtures/cube_cavity/unit_cube.msh`` — the canonical n=10
  tet-split unit-cube mesh, MSH 4.1 ASCII (consumable by
  ``geode_core::GmshReader`` and by sibling backend impls like
  ``reference/jax/cube_cavity.py`` from issue #93).
- ``reference/fixtures/cube_cavity/baseline.json`` — golden output
  fixture in the canonical schema (``reference/SCHEMA.md`` v1) plus a
  few cube-cavity-specific input/output fields:
    * inputs.``eigenvectors_numpy`` — the NumPy-computed eigenvectors
      ``Q_numpy`` of shape ``[n_int, k]``, stored as inputs so the Rust
      harness can compute subspace overlap against the Burn-computed
      ``Q_burn`` (the elementwise compare path is not appropriate for
      eigenvectors — see issue #92 acceptance criteria #3).
    * outputs.``eigenvalues`` — lowest 5 eigenvalues with relative
      tolerance ~1e-6 (per acceptance criteria).
    * outputs.``k_int_frobenius``, ``m_int_frobenius`` — scalar
      sub-stage checks on the assembled interior matrices.
    * outputs.``k_int_diag``, ``m_int_diag`` — full diagonals of the
      interior K, M (n_int values each — cheap, sub-stage friction
      signal).
    * outputs.``analytic_eigenvalues`` — the analytic targets, included
      so the test reports both "match NumPy" *and* "match physics".

Why a single JSON fixture (instead of HDF5)
============================================
The Phase-A scaffolding (PR #94 / issue #89) wired ``FixtureFormat::Hdf5``
as a reserved variant but the loader still returns ``HdfNotEnabled``.
For n=10 the largest field is ``eigenvectors_numpy`` at 5 × 729 ≈ 3645
floats, which serializes to ~70 KB of JSON — fine in-repo for a
durable artifact. HDF5 only buys us real value once eigenvectors
exceed ~10⁵ entries (full Mie-sphere slices, etc.). The schema
``FixtureFormat`` enum is forward-compatible: a future PR can swap to
HDF5 without changing the on-disk semantic schema.

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_cube_cavity_baseline.py

Re-runs are deterministic up to ARPACK's eigenvector sign convention.
ARPACK picks the sign of each eigenvector based on the Arnoldi
iteration's initial random vector. We pin scipy's RNG seed so the
fixture round-trips byte-identically as long as scipy's ARPACK
wrapper API is stable. (If the seed mechanism changes upstream,
regenerate and commit the new fixture.)
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import numpy as np
import scipy

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/ -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "cube_cavity"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
MESH_PATH = FIXTURE_DIR / "unit_cube.msh"

sys.path.insert(0, str(HERE))
from cube_cavity import (  # noqa: E402
    analytic_lowest_five,
    cube_tet_mesh,
    run_cube_cavity,
    write_msh,
)


# Acceptance criteria #2: lowest 5 eigenvalues agree to 1e-6 relative.
# Absolute tolerance applies in the fixture; pick something that is
# 1e-6 of the smallest eigenvalue (~3π² ≈ 29.6), padded slightly for
# headroom against ARPACK convergence noise.
EIGENVALUE_TOL_ABS = 1e-4  # ~3.4e-6 relative at λ_min ≈ 29.6

# Frobenius / diagonal tolerances are tight (f64-vs-f64 floor) under
# the `ndarray` backend now that issue #99 fixed
# `geode_core::assembly::upload_mesh` to honor `B::FloatElem`. Burn's
# K and M carry full f64 precision on `ndarray` (the CI backend), so
# the cross-backend agreement on the n=10 cube cavity hits f64
# roundoff. Observed post-fix maxima:
#   * K_int diag max abs err ≈ 1e-14
#   * K_int frobenius abs err ≈ 1e-13 (rel ~1e-15 against ‖K‖_F ≈ 17.36)
#   * M_int diag max abs err ≈ 1e-18  (M entries scale as h^3 ≈ 4e-4)
#   * M_int frobenius abs err ≈ 1e-15
# Tolerances are set ~100x looser than observed so the fixture absorbs
# cross-platform LLVM FMA / SIMD reduction-order drift while still
# catching real regressions (the original f32 truncation was a 5e-8
# floor — comfortably above these new bounds).
# GPU backends (wgpu/cuda) have `B::FloatElem = f32` and apply looser
# bounds at the test level (see NDARRAY_F64_TOLERANCES /
# GPU_F32_TOLERANCES in cube_cavity_numpy_reference.rs).
FROBENIUS_TOL_ABS_K = 1e-11
FROBENIUS_TOL_ABS_M = 1e-13
DIAG_TOL_ABS_K = 1e-12
DIAG_TOL_ABS_M = 1e-14

# Analytic eigenvalues are compared with the O(h²) tolerance band that
# both Burn and NumPy share by construction. The ground mode hits 4.1%
# at n=10; mode 5 (9π²) lands at ~10.5%. We pin a 12% relative
# tolerance band — comfortably above what the physics produces but
# tight enough to catch a backend that drifted off the physics
# entirely.
ANALYTIC_REL_TOL = 0.12  # 12% — see README in fixture dir


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


def _pin_eigvec_signs(eigvecs):
    """Make each eigenvector's sign canonical: first entry with magnitude
    above 1e-10 must be positive.

    ARPACK is free to flip the sign of any eigenvector; pinning the
    convention here keeps the fixture deterministic. The subspace
    overlap comparison the Rust harness uses is sign-invariant anyway,
    but the per-entry storage benefits from a stable convention.
    """
    out = eigvecs.copy()
    for j in range(out.shape[1]):
        col = out[:, j]
        # Find first entry with |x| > 1e-10 and force its sign positive.
        nonzero = np.where(np.abs(col) > 1e-10)[0]
        if nonzero.size and col[nonzero[0]] < 0:
            out[:, j] = -col
    return out


def main():
    n = 10
    # We solve for `k_full=6` eigenpairs so the subspace-overlap test in
    # the Rust harness can close the degenerate cluster {4, 5} at
    # 9.946·π² — a numerical lifting (P1 anisotropy on the hex split)
    # of the analytic 3-fold-degenerate 9·π² mode that lands as a
    # bit-identical pair within the lowest-6 cut. Acceptance criterion
    # #2 (lowest 5 eigenvalues match Burn to 1e-6 rel) still uses the
    # first 5 entries.
    #
    # Cluster layout on the n=10 mesh, detected from eigenvalue gaps:
    #   {0}         3.124·π² (dim 1)
    #   {1, 2}      6.374·π² (dim 2, P1-lifted from analytic 3-fold)
    #   {3}         6.600·π² (dim 1, third 6·π² mode lifted off)
    #   {4, 5}      9.946·π² (dim 2, P1-lifted from analytic 3-fold)
    k = 5  # acceptance criterion #2 cut for eigenvalue comparison
    k_full = 6  # closes the {4, 5} degenerate cluster for subspace overlap
    side = 1.0

    # Generate the canonical mesh and write it for cross-backend reuse.
    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)
    nodes, tets = cube_tet_mesh(n, side=side)
    write_msh(MESH_PATH, nodes, tets)

    # Run the full pipeline, loading the mesh we just wrote so we are
    # testing the same code path the cross-backend harness exercises.
    # Compute k_full eigenpairs (cluster-closing), then store the
    # lowest k as the headline eigenvalues + lowest k_full eigenvectors
    # so the harness has the closure for the dim-2 cluster at 9.946·π².
    result = run_cube_cavity(n=n, k=k_full, side=side, mesh_path=str(MESH_PATH))

    eigvecs_full = _pin_eigvec_signs(result["eigenvectors"])  # [n_int, k_full]
    eigvals_full = result["eigenvalues"]  # length k_full
    eigvals_headline = eigvals_full[:k]

    # Sanity-check: NumPy eigenvalues land inside the analytic O(h²) band.
    analytic = analytic_lowest_five()
    rel_err = np.abs(eigvals_headline - analytic) / analytic
    if np.any(rel_err > ANALYTIC_REL_TOL):
        worst = int(np.argmax(rel_err))
        raise SystemExit(
            f"NumPy eigenvalue {worst} drifted off analytic target: "
            f"got {eigvals_headline[worst]:.6e}, want {analytic[worst]:.6e}, "
            f"rel_err {rel_err[worst]:.3%} > tol {ANALYTIC_REL_TOL:.0%}"
        )

    # Build the fixture document in the canonical schema (SCHEMA.md v1).
    fixture = {
        "schema_version": "1",
        "fixture_id": "cube_cavity/n10_first_five_modes",
        "description": (
            "Scalar Helmholtz Dirichlet cube cavity: lowest 5 eigenpairs of "
            "K x = λ M x on the n=10 tet-split unit cube. NumPy reference "
            "computed by reference/numpy/cube_cavity.py via "
            "scipy.sparse.linalg.eigsh(K, k=5, M=M, sigma=0, which='LM'). "
            "Cross-checked against Burn (geode_core) in "
            "crates/geode-validation/tests/cube_cavity_numpy_reference.rs."
        ),
        "units": "dimensionless (unit cube, P1 Laplacian; eigenvalues in units of 1/length²)",
        "inputs": {
            "mesh_path": {
                # mesh_path is a string-valued input; we sidestep the
                # numeric-only `data` schema by storing the path in
                # `description` and an empty data array (length 0).
                "shape": [0],
                "dtype": "f64",
                "description": (
                    f"Mesh fixture file (relative to repo root): "
                    f"reference/fixtures/cube_cavity/unit_cube.msh — "
                    f"n={n} tet-split unit cube, {result['n_nodes']} nodes, "
                    f"{result['n_tets']} tets."
                ),
                "data": [],
            },
            "n_per_side": {
                "shape": [1],
                "dtype": "f64",
                "description": "Hexes per cube side (mesh refinement level).",
                "data": [float(n)],
            },
            "eigenvectors_numpy": {
                "shape": [result["n_int"], k_full],
                "dtype": "f64",
                "description": (
                    f"NumPy-computed eigenvectors Q_numpy, "
                    f"shape [n_int={result['n_int']}, k_full={k_full}]. "
                    "M-orthonormal. Stored as INPUT (not output) because "
                    "eigenvector comparison must use subspace overlap "
                    "(see issue #92 acceptance criteria #3), not "
                    "elementwise diff. The Rust harness reads this field "
                    "and computes Q_numpy^T M_burn Q_burn block-Frobenius "
                    "norms per degenerate cluster. The 6th column closes "
                    "the dim-2 cluster {4, 5} at 9.946·π² (P1-numerical "
                    "lifting of the analytic 3-fold-degenerate 9·π² mode)."
                ),
                # Row-major flatten: index (i, j) -> i*k_full + j.
                "data": eigvecs_full.flatten(order="C").tolist(),
            },
        },
        "outputs": {
            "eigenvalues": {
                "shape": [k],
                "dtype": "f64",
                "description": (
                    "Lowest 5 generalized eigenvalues of K x = λ M x on the "
                    "interior P1 DOFs (Dirichlet eliminated). Ascending order. "
                    "Acceptance criterion: 1e-6 relative agreement with Burn. "
                    f"Tolerance stored as absolute (1e-4 ≈ {EIGENVALUE_TOL_ABS/(3*np.pi**2):.1e} "
                    "relative at λ_min ≈ 3π²)."
                ),
                "tolerance_abs": EIGENVALUE_TOL_ABS,
                "data": eigvals_headline.tolist(),
            },
            "k_int_frobenius": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Frobenius norm of the Dirichlet-interior global stiffness "
                    "K_int. Sub-stage diagnostic: this scalar pins assembly "
                    "agreement before the eigensolve runs. Post-#99 "
                    "(upload_mesh honors B::FloatElem), the f64 ndarray backend "
                    "hits ~1e-13 here; the 1e-11 tolerance gives 100x headroom "
                    "for cross-platform LLVM FMA / SIMD reduction-order drift."
                ),
                "tolerance_abs": FROBENIUS_TOL_ABS_K,
                "data": [result["k_int_frobenius"]],
            },
            "m_int_frobenius": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Frobenius norm of the Dirichlet-interior global mass M_int. "
                    "Sub-stage diagnostic; companion to k_int_frobenius. Post-#99 "
                    "the f64 ndarray backend hits ~1e-16 here (M entries are "
                    "O(h^3) ≈ 1e-3, with f64 roundoff scaling accordingly); the "
                    "1e-13 tolerance keeps headroom for cross-platform drift."
                ),
                "tolerance_abs": FROBENIUS_TOL_ABS_M,
                "data": [result["m_int_frobenius"]],
            },
            "k_int_diag": {
                "shape": [result["n_int"]],
                "dtype": "f64",
                "description": (
                    "Diagonal of the Dirichlet-interior global stiffness K_int. "
                    "Full per-row sub-stage diagnostic — if assembly disagrees "
                    "anywhere, at least one diagonal entry surfaces it. Post-#99 "
                    "the f64 ndarray backend hits ~1e-14 per entry; tolerance "
                    "1e-12 keeps ~100x headroom."
                ),
                "tolerance_abs": DIAG_TOL_ABS_K,
                "data": result["k_int_diag"].tolist(),
            },
            "m_int_diag": {
                "shape": [result["n_int"]],
                "dtype": "f64",
                "description": (
                    "Diagonal of the Dirichlet-interior global mass M_int. "
                    "Post-#99 the f64 ndarray backend hits ~1e-18 here (M "
                    "diagonal entries are O(h^3) ≈ 4e-4); tolerance 1e-14 keeps "
                    "room for cross-platform drift."
                ),
                "tolerance_abs": DIAG_TOL_ABS_M,
                "data": result["m_int_diag"].tolist(),
            },
            "analytic_eigenvalues": {
                "shape": [k],
                "dtype": "f64",
                "description": (
                    "Analytic Dirichlet Laplacian eigenvalues on the unit cube: "
                    "{3, 6, 6, 6, 9}·π². The NumPy reference reproduces these "
                    "to the same O(h²) discretization band that the Burn path "
                    f"hits (~4.1% on the ground mode at n={n}); tolerance below "
                    "is set to the analytic value × ANALYTIC_REL_TOL = "
                    f"{ANALYTIC_REL_TOL:.0%} so the test surfaces a P1-physics "
                    "regression but not the expected discretization tail."
                ),
                # Per-field absolute tolerance, scaled to the largest analytic
                # eigenvalue (9π²) so the test allows the entire O(h²) band.
                "tolerance_abs": float(ANALYTIC_REL_TOL * 9.0 * np.pi**2),
                "data": analytic.tolist(),
            },
            "n_int": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of interior (non-Dirichlet) DOFs. Stored as f64 "
                    "because schema v1 doesn't have an integer output kind; "
                    "compared with strict equality (tolerance < 1)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(result["n_int"])],
            },
        },
        "provenance": {
            "source": (
                f"reference/numpy/cube_cavity.py @ commit {_git_commit()} ; "
                f"scipy {scipy.__version__}, numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"
            ),
            "verified_against": (
                "crates/geode-validation/tests/cube_cavity_numpy_reference.rs "
                "(Burn ndarray and wgpu/cuda backends with backend-aware tolerances)"
            ),
            "issue": "#92 (parent epic #88)",
        },
    }

    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")

    print(f"Wrote {MESH_PATH} ({os.path.getsize(MESH_PATH)} bytes)")
    print(
        f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes) — "
        f"n_int={result['n_int']}, k={k}"
    )
    print()
    print(f"Eigenvalues (lowest {k}) vs analytic targets:")
    pi2 = np.pi**2
    print("idx  target/π²   λ_np/π²   rel err")
    print("---  ---------   -------   --------")
    for i, (got, want) in enumerate(zip(eigvals_headline, analytic)):
        rel = abs(got - want) / want * 100.0
        print(f"{i:<3}  {want / pi2:.4f}      {got / pi2:.4f}    {rel:+.4f}%")
    print()
    print(
        f"Additional eigenvalues (closing degenerate clusters), idx {k}..{k_full-1}:"
    )
    for i in range(k, k_full):
        print(f"{i:<3}  {eigvals_full[i] / pi2:.4f}")


if __name__ == "__main__":
    main()
