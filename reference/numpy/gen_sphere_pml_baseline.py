"""Generate the sphere-PML baseline fixture for issue #146 (Phase H.1).

Promotes the Phase H scaffolding stub (#145, PR #151) to a full
NumPy-computed baseline:

- ``epsilon_r_complex`` becomes the full ``[n_tets]`` per-tet complex
  permittivity from :func:`sphere_pml.build_complex_epsilon_r_pml` (no
  longer a 4-entry illustrative slice).
- ``eigenvalues_lowest_complex`` becomes the lowest
  ``spurious_dim + 8`` complex eigenvalues from the full
  scipy-LAPACK dense generalized eigensolve at ``σ₀ = 5.0`` (no longer
  the 2-entry synthetic stub).
- ``physical_eigenvalues_complex`` adds the lowest 5 physical complex
  eigenvalues after spurious filtering (new field, not present in the
  stub schema).
- ``n_spurious_observed`` adds the algebraic d⁰-rank spurious count
  (new field).
- ``q_factor_lowest_physical`` is recomputed from the real lowest
  physical mode in the sign-agnostic k-space form
  ``Re(k) / (2 |Im(k)|)`` (matches the Burn-side print convention).

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_sphere_pml_baseline.py

The dense scipy.linalg.eigvals on the 3300-DOF interior pencil takes
~28 minutes single-threaded on a typical laptop. The eigenvector-less
path keeps memory under control (no n×n complex eigenvector matrix).

What this fixture pins
======================

- **Mesh shape**: ``n_nodes`` and ``n_tets`` (parsed by the NumPy
  ``meshio``-backed loader, must agree with the Burn ``GmshReader``).
- **PML profile parameters**: ``sigma_0 = 5.0``, ``R_SPHERE / R_PML_INNER
  / R_BUFFER`` (mirror of the Burn integration test).
- **Complex permittivity**: full per-tet ``epsilon_r_complex`` vector
  matching ``geode_core::build_complex_epsilon_r_pml`` bit-for-bit.
- **Complex eigenvalue spectrum**: ``eigenvalues_lowest_complex`` =
  lowest ``spurious_dim + 8`` complex eigenvalues, sorted by
  ``|Re(λ)|`` ascending. Includes the spurious near-zero cluster +
  lowest physical modes.
- **Physical eigenvalues**: lowest 5 complex eigenvalues past the
  d⁰-rank-derived spurious split.
- **Q-factor**: of the lowest physical mode (sanity check, sign-
  agnostic ``Re(k) / (2 |Im(k)|)`` form).

Tolerance budget
================

- ``epsilon_r_complex``: ``1e-14`` absolute (bit-exact ``c128`` round-trip).
- ``eigenvalues_lowest_complex``: ``1e-5`` absolute (eigenvalue
  agreement budget — the dense QZ on Burn vs LAPACK on NumPy differ at
  ~``1e-7`` absolute on the physical band per the Phase G.2 PEC
  precedent; ``1e-5`` adds 100× headroom for the lossy ε path which
  amplifies any per-element scatter drift).
- ``q_factor_lowest_physical``: ``1e-3`` absolute (depends on
  eigenvector phase + small mode-tracking ambiguity on the degenerate
  triplet).
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
REPO_ROOT = HERE.parent.parent  # reference/numpy -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "sphere_pml"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
MESH_PATH = FIXTURE_DIR / "sphere.msh"

sys.path.insert(0, str(HERE))
from sphere_pml import (  # noqa: E402
    R_BUFFER,
    R_PML_INNER,
    R_SPHERE,
    run_sphere_pml,
)

# --------------------------------------------------------------------------- #
# Tolerances applied to the on-disk baseline.
# --------------------------------------------------------------------------- #

# c128 tolerances apply to |Δ| = |actual - golden| (the complex modulus
# of the residual). See reference/SCHEMA.md → "Complex encoding (c128)".

EPS_C128_TOL_ABS = 1.0e-14
"""``epsilon_r_complex`` ε floor — bit-exact c128 round-trip. The Burn-
side build emits ε via a deterministic ``c64::new(re, im)`` constructor
that matches the NumPy double-precision result exactly modulo the f32
GPU backend; the f64 ndarray backend should be within f64 ULP, while
the GPU f32 backend may need a looser per-test override. 1e-14 is the
defensible f64 floor for the bundled fixture (largest |ε| ≈ 5)."""

EIG_C128_TOL_ABS = 1.0e-5
"""``eigenvalues_lowest_complex`` ε floor — generalized eigensolver
agreement. Burn uses faer's dense QZ; NumPy uses LAPACK ZGGEV. Phase
G.2's PEC eigenvalue tolerance settled at ``1e-6`` absolute (~7e-6
relative at λ ≈ 1.4); the PML mass adds complex roundoff per scatter
entry, so 1e-5 gives 10× headroom over the PEC floor. Set per the
issue body: "Likely tolerance is ~1e-5; tighten if you have headroom"."""

PHYSICAL_C128_TOL_ABS = 1.0e-5
"""Lowest 5 physical complex eigenvalues — same budget as
``eigenvalues_lowest_complex``. Stored as a separate field so the
spurious-filtered slice is unambiguous in the diff artifact."""

Q_FACTOR_TOL_ABS = 1.0e-3
"""``q_factor_lowest_physical`` ε floor. The Q-factor derives from k =
sqrt(λ), so a 1e-5 eigenvalue residual translates to roughly 1e-5/sqrt
of the real part times the Im(k) inverse — typically 1e-4 on the
bundled fixture's ~1.18 + 0.21j ground mode (Q ≈ 1.18). 1e-3 is a
defensible round-number floor."""


# --------------------------------------------------------------------------- #
# PML problem parameters — mirror of the Burn integration test.
# --------------------------------------------------------------------------- #

N_INDEX = 1.5
SIGMA_0 = 5.0
"""Matches ``crates/geode-core/tests/sphere_pml_eigenmode.rs:196``."""


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
    flat list described in ``reference/SCHEMA.md``.

    Identical to the helper in the scaffolding stub generator
    (``gen_sphere_pml_baseline.py`` at #145). Promoted here verbatim so
    the file stays self-contained.
    """
    z = np.ascontiguousarray(z, dtype=np.complex128)
    # `.view(np.float64)` reinterprets the underlying memory as pairs
    # of f64s laid out [re0, im0, re1, im1, ...]. This is the exact
    # wire format the Rust loader expects (Fixture::output_c128 /
    # input_c128).
    return z.view(np.float64).tolist()


def main() -> None:
    print(f"Running sphere-PML NumPy pipeline on {MESH_PATH} ...")
    print(f"  sigma_0 = {SIGMA_0}, n_index = {N_INDEX}")
    print("  (dense scipy.linalg.eigvals on a 3300-DOF complex pencil; ~30 min)")

    result = run_sphere_pml(
        MESH_PATH,
        sigma_0=SIGMA_0,
        n_index=N_INDEX,
        n_take=5,
        r_outer=R_BUFFER,
    )

    n_nodes = result["n_nodes"]
    n_tets = result["n_tets"]
    n_edges = result["n_edges"]
    n_interior_edges = result["n_interior_edges"]
    spurious_dim = result["spurious_dim"]
    n_spurious = result["n_spurious"]
    eps_complex = result["epsilon_r_complex"]
    eigvals_all = result["eigenvalues_all"]
    physical = result["physical_eigenvalues"]
    q_factor = result["q_factor_lowest_physical"]
    max_imag_rel = result["max_imag_eigval_rel"]
    n_request = len(eigvals_all)

    print(f"  n_nodes={n_nodes}, n_tets={n_tets}, n_edges={n_edges}")
    print(f"  n_interior_edges={n_interior_edges}, spurious_dim={spurious_dim}")
    print(f"  n_spurious (d⁰-rank) = {n_spurious}")
    print(f"  max|Im(λ)|/max(|Re(λ)|, 1) over full slice = {max_imag_rel:.3e}")
    print("  lowest 5 physical complex eigenvalues:")
    for i, lam in enumerate(physical):
        print(f"    physical[{i}]: λ = {lam.real:+.6e} {lam.imag:+.6e}j")
    print(f"  Q-factor of lowest physical mode (Re(k)/(2|Im(k)|)) = {q_factor:.4f}")

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "sphere_pml/n774_pml_eigenmode",
        "description": (
            "Scalar-isotropic PML sphere eigenmode (issue #146, parent epic "
            "#88, Phase H.1). End-to-end NumPy reference for the dielectric "
            "sphere (n=1.5) in a vacuum gap surrounded by a quadratic-ramp "
            f"PML shell (sigma_0={SIGMA_0}) with a PEC outer wall. Promotes "
            "the Phase H scaffolding stub (#145, PR #151) to a full "
            "numerical baseline. Cross-checked against Burn (geode_core) "
            "in crates/geode-validation/tests/sphere_pml_numpy_reference.rs."
        ),
        "units": (
            "λ = k² (inverse-length squared) with the eigensolver-determined "
            "sign of Im(λ) under exp(+jωt); dimensionless mesh coordinates"
        ),
        "inputs": {
            "mesh_path": {
                "shape": [0],
                "dtype": "f64",
                "description": (
                    "Mesh fixture file (relative to repo root): "
                    "reference/fixtures/sphere_pml/sphere.msh — same as "
                    "the sphere_pec/ bundled mesh "
                    f"({n_nodes} nodes, {n_tets} tets)."
                ),
                "data": [],
            },
            "sigma_0": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "PML absorption strength at r=R_BUFFER. Matches the "
                    "value used in "
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
            # ----- Promoted from stub: full [n_tets] complex ε ----- #
            "epsilon_r_complex": {
                "shape": [int(n_tets)],
                "dtype": "c128",
                "description": (
                    "Per-tet complex relative permittivity. Promoted from "
                    "the 4-entry stub at scaffolding time (PR #151) to "
                    "the full [n_tets] vector. Profile: ε = n² + 0j in "
                    "the dielectric, ε = 1 + 0j in the vacuum gap, "
                    "ε = 1 - j σ_0 ((r - R_PML_INNER)/(R_BUFFER - "
                    "R_PML_INNER))² in the PML shell. Mirror of "
                    "geode_core::build_complex_epsilon_r_pml. On-disk: "
                    "real-imag interleaved flat array per "
                    "reference/SCHEMA.md."
                ),
                "data": _interleave_c128(eps_complex),
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
                    "Number of mesh tets. Stored as f64; strict equality."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_tets)],
            },
            "n_edges": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of globally-deduplicated edges.",
                "tolerance_abs": 0.5,
                "data": [float(n_edges)],
            },
            "n_interior_edges": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of edges that survive PEC reduction (at least "
                    "one endpoint strictly inside the outer wall)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_interior_edges)],
            },
            "spurious_dim": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Predicted gradient-kernel dimension = number of mesh "
                    "nodes strictly inside the outer PEC wall. Equals the "
                    "expected number of spurious near-zero eigenvalues. "
                    "Same value as in sphere_pec/baseline.json (the mesh "
                    "is shared)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(spurious_dim)],
            },
            "n_spurious_observed": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Algebraic spurious-mode dimension = "
                    "rank(d⁰_interior) (`spurious_dim_from_derham`). The "
                    "discrete H¹_0 → H(curl) gradient image dimension is "
                    "**independent** of the complex ε scaling on the "
                    "mass — gradients of H¹_0 sit in the kernel of "
                    "curl-curl regardless of how the mass is scaled "
                    "(Epic #57 risk note). On the bundled 774-node "
                    "fixture this is 368, identical to the PEC baseline. "
                    "Cross-checked bit-exactly between Burn and NumPy."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_spurious)],
            },
            "eigenvalues_lowest_complex": {
                "shape": [n_request],
                "dtype": "c128",
                "description": (
                    f"Lowest {n_request} = spurious_dim + 8 complex "
                    "eigenvalues of the complex generalized pencil "
                    "K x = λ M x (K real, M complex-symmetric). Sorted by "
                    "|Re(λ)| ascending — same order as Burn's "
                    "FaerComplexEigensolver. Includes the entire "
                    "spurious near-zero cluster (~368 entries) plus the "
                    "lowest few physical modes. The sign of Im(λ) is "
                    "**not constrained** by the complex-symmetric pencil "
                    "and may differ between scipy LAPACK and faer QZ; "
                    "the `1e-5` |Δ|-tolerance accommodates this. "
                    "On-disk: real-imag interleaved flat array per "
                    "reference/SCHEMA.md."
                ),
                "tolerance_abs": EIG_C128_TOL_ABS,
                "data": _interleave_c128(eigvals_all),
            },
            "physical_eigenvalues_complex": {
                "shape": [int(len(physical))],
                "dtype": "c128",
                "description": (
                    "Lowest 5 physical complex eigenvalues past the "
                    "d⁰-rank spurious split. Sorted by |Re(λ)| ascending. "
                    "On the bundled fixture these are the absorbing "
                    f"counterparts of the PEC λ ≈ 1.42 triplet — at "
                    f"σ₀={SIGMA_0} they sit at λ ≈ 1.18 + 0.21j "
                    "(triplet) plus the higher-l absorbing modes. The "
                    "Re part drops vs the PEC reference because PML "
                    "boundary loss + finite cavity Q shift the cavity "
                    "resonance down; this is the expected physical "
                    "behavior. Acceptance criterion: 1e-5 absolute on "
                    "|Δ| (eigenvalue comparator)."
                ),
                "tolerance_abs": PHYSICAL_C128_TOL_ABS,
                "data": _interleave_c128(physical),
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Quality factor Q = Re(k) / (2 |Im(k)|) for "
                    "k = sqrt(λ) of the lowest physical complex "
                    "eigenvalue (sign-agnostic k-space form; matches "
                    "the Burn-side print convention in "
                    "tests/sphere_pml_eigenmode.rs::sphere_pml_eigenmode_spectrum). "
                    "Sanity diagnostic; the σ₀=0 limit gives Q=inf and "
                    "is exercised by the in-process PEC regression test "
                    "(sphere_pml::run_sphere_pml with sigma_0=0.0)."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_factor],
            },
        },
        "provenance": {
            "source": (
                f"reference/numpy/sphere_pml.py @ commit {_git_commit()} ; "
                f"scipy {scipy.__version__}, numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}"
                f".{sys.version_info.micro} ; dense LAPACK ZGGEV "
                "(scipy.linalg.eigvals) on the full interior pencil"
            ),
            "verified_against": (
                "crates/geode-validation/tests/sphere_pml_numpy_reference.rs "
                "(Burn ndarray backend, release mode)"
            ),
            "issue": "#146 (parent epic #88, Phase H.1)",
        },
    }

    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")

    print()
    print(f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes)")


if __name__ == "__main__":
    main()
