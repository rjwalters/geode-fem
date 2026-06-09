"""Generate the full-mesh anisotropic-UPML Mie baseline fixture (issue #171).

Phase J.2 (Epic #88) full-mesh fixture: the bundled refined sphere
(774 nodes / 3335 tets, ``reference/fixtures/sphere_pml/sphere.msh``)
at the exact ``crates/geode-core/tests/mie_sphere.rs`` acceptance
parameters — ``n = 1.5``, ``σ₀ = 5.0``, ``k₀_ref = 2.0``, anisotropic
diagonal UPML tensor.

The dense LAPACK ZGGEV on the ~3300-DOF interior pencil takes tens of
minutes single-threaded; the Burn-side cross-check
(``sphere_mie_spectrum_agrees_with_numpy``) is correspondingly
``#[ignore]``-gated and release-only. The default-CI gate is the
small-mesh sibling (``gen_sphere_mie_small_baseline.py``).

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_sphere_mie_baseline.py     # long: dense ZGGEV

What this fixture pins — see ``gen_sphere_mie_small_baseline.py``; the
field set is identical except that the σ₀ = 0 PEC anchor is omitted:
the full mesh's σ₀ = 0 collapse is already pinned by the
``sphere_pml`` / ``sphere_pec`` fixtures (Phase G.2 / H.1), and the
in-fixture anchor would double the multi-minute eigensolve cost.

Tolerance budget
================

The full-mesh pencil is better conditioned than the small one (see the
#158 discussion): the Phase H.1 full fixture holds 1e-5 absolute on
the complex spectrum, and the tensor-ε pencil inherits that floor.
Q of the lowest mode is O(30) on this mesh with |Im(λ)| ≈ 0.06, so the
Q sensitivity dQ/dIm(λ) ≈ Q/|Im(λ)| ≈ 500 maps a 1e-5 λ-residual to
~5e-3 on Q; 1e-1 absolute is the defensible floor.
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
REPO_ROOT = HERE.parent.parent
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "sphere_mie"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
MESH_PATH = REPO_ROOT / "reference" / "fixtures" / "sphere_pml" / "sphere.msh"

sys.path.insert(0, str(HERE))
from sphere_mie import (  # noqa: E402
    K0_REF,
    SIGMA_0_DEFAULT,
    classify_modes_against_catalogue,
    load_mie_roots_catalogue,
    q_factor_from_lambda,
    run_sphere_mie,
)
from sphere_pec import R_BUFFER, R_PML_INNER, R_SPHERE  # noqa: E402

# --------------------------------------------------------------------------- #
# Tolerances — full-mesh floors (see module docstring).
# --------------------------------------------------------------------------- #

EPS_C128_TOL_ABS = 1.0e-14
# Full-slice (spurious + physical) floor. The scalar Phase H.1 fixture
# holds 1e-5 on this mesh, but the tensor-ε pencil's near-zero spurious
# cluster carries slightly larger faer 0.24 QZ vs LAPACK ZGGEV
# residuals: measured Burn-vs-NumPy worst offender 1.83e-5 absolute at
# the cluster edge (11 of 376 modes exceed 1e-5, all within the
# spurious cluster; the physical band holds < 1e-6). 5e-5 gives ~3×
# headroom on the cluster while the physical-band field keeps the
# tight 1e-5 floor.
EIG_C128_TOL_ABS = 5.0e-5
PHYSICAL_C128_TOL_ABS = 1.0e-5
Q_FACTOR_TOL_ABS = 1.0e-1
RE_K_TOL_ABS = 1.0e-5
ANALYTIC_K_TOL_ABS = 1.0e-9

N_INDEX = 1.5
SIGMA_0 = SIGMA_0_DEFAULT  # 5.0

TM11_REL_TOL = 0.08
Q_LOWER_BAND_TM11 = 1.5
STRICT_MODE_WINDOW_LEN = 3


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
    z = np.ascontiguousarray(z, dtype=np.complex128).reshape(-1)
    return z.view(np.float64).tolist()


def main() -> None:
    print(f"Running anisotropic-UPML Mie NumPy pipeline on {MESH_PATH} ...")
    print(f"  sigma_0 = {SIGMA_0}, n_index = {N_INDEX}, k0_ref = {K0_REF}")
    print("  (dense ZGGEV on the ~3300-DOF interior pencil — this takes a while)")

    result = run_sphere_mie(
        MESH_PATH,
        sigma_0=SIGMA_0,
        n_index=N_INDEX,
        k0_ref=K0_REF,
        n_take=5,
        r_outer=R_BUFFER,
    )

    n_nodes = result["n_nodes"]
    n_tets = result["n_tets"]
    n_edges = result["n_edges"]
    n_interior_edges = result["n_interior_edges"]
    spurious_dim = result["spurious_dim"]
    n_spurious = result["n_spurious"]
    eps_tensor = result["epsilon_tensor_diag"]
    eigvals_all = result["eigenvalues_all"]
    physical = result["physical_eigenvalues"]
    physical_ks = result["physical_ks"]
    q_factor = result["q_factor_lowest_physical"]
    n_request = len(eigvals_all)

    print(f"  n_nodes={n_nodes}, n_tets={n_tets}, n_edges={n_edges}")
    print(f"  n_interior_edges={n_interior_edges}, spurious_dim={spurious_dim}")
    print(f"  n_spurious (d⁰-rank) = {n_spurious}")

    roots = load_mie_roots_catalogue()
    tm11 = next(
        r for r in roots if r["pol"] == "TM" and r["l"] == 1 and r["n"] == 1
    )
    analytic_tm11_k = tm11["k"]
    table = classify_modes_against_catalogue(physical_ks, roots)

    print("  lowest 5 physical modes:")
    for i, (lam, k, row) in enumerate(zip(physical, physical_ks, table)):
        print(
            f"    [{i}] λ = {lam.real:+.6e} {lam.imag:+.6e}j  "
            f"k = {k.real:.5f} {k.imag:+.5f}j  ->  "
            f"{row['pol']}_{row['l']},{row['n']} "
            f"(rel err = {row['rel_err'] * 100:.2f}%)"
        )

    lowest_re_k = float(physical_ks[0].real)
    rel_err_tm11 = abs(lowest_re_k - analytic_tm11_k) / analytic_tm11_k
    print(f"  lowest mode vs analytic TM_1,1 (k = {analytic_tm11_k:.6f}): "
          f"rel err = {rel_err_tm11 * 100:.2f}%")
    assert table[0]["pol"] == "TM" and table[0]["l"] == 1 and table[0]["n"] == 1
    assert rel_err_tm11 < TM11_REL_TOL, (
        f"lowest mode rel err {rel_err_tm11 * 100:.2f}% exceeds the "
        f"documented {TM11_REL_TOL * 100:.0f}% band"
    )

    triplet_qs = sorted(
        q_factor_from_lambda(lam) for lam in physical[:STRICT_MODE_WINDOW_LEN]
    )
    q_median_triplet = float(triplet_qs[1])
    print(f"  Q lowest = {q_factor:.4f}, TM_1,1 triplet median Q = "
          f"{q_median_triplet:.4f} (band > {Q_LOWER_BAND_TM11})")
    assert q_factor > Q_LOWER_BAND_TM11
    assert q_median_triplet > Q_LOWER_BAND_TM11

    triplet_spread = float(
        physical[STRICT_MODE_WINDOW_LEN - 1].real - physical[0].real
    )
    gap_to_next = float(
        physical[STRICT_MODE_WINDOW_LEN].real
        - physical[STRICT_MODE_WINDOW_LEN - 1].real
    )
    print(f"  TM_1,1 triplet spread = {triplet_spread:.4f}, "
          f"gap to next band = {gap_to_next:.4f}")
    assert gap_to_next > 2.0 * triplet_spread, (
        "strict mode window does not end at a spectral gap — "
        "cluster-closure convention (#160) violated"
    )

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "sphere_mie/n774_aniso_upml_mie",
        "description": (
            "Full-mesh anisotropic-UPML dielectric-sphere Mie eigenmode "
            "(issue #171, Epic #88 Phase J.2). The bundled refined "
            "sphere (774 nodes / 3335 tets, "
            "reference/fixtures/sphere_pml/sphere.msh) with the diagonal "
            "UPML tensor (geode_core::build_anisotropic_pml_tensor_diag, "
            "σ₀ = 5.0, k₀_ref = 2.0) — the exact mie_sphere.rs "
            "acceptance configuration. Anchored to the Phase J.1 "
            "analytic catalogue (TM_1,1 k ≈ 1.30343). Strict cross-IR "
            "window = first 3 physical modes (closed TM_1,1 triplet, "
            "#160 cluster-closure convention). Sign note: physical "
            "Im(λ) > 0 on this refined mesh's tensor pencil — the sign "
            "is mesh-dependent (the small-mesh sibling shows Im(λ) < 0) "
            "because the UPML tensor carries mixed-sign 1/s_r vs s_t "
            "entries; it is a property of the pencil, not a solver "
            "choice. Burn cross-check is "
            "release-gated (#[ignore]) — faer 0.24 complex GEVD takes "
            "60+ min on the ~3300-DOF pencil; the default-CI gate is "
            "the sphere_mie_small sibling."
        ),
        "units": (
            "λ = k² (inverse-length squared) under exp(+jωt); "
            "dimensionless mesh coordinates"
        ),
        "inputs": {
            "mesh_path": {
                "shape": [0],
                "dtype": "f64",
                "description": (
                    "Mesh fixture (relative to repo root): "
                    "reference/fixtures/sphere_pml/sphere.msh "
                    f"({n_nodes} nodes, {n_tets} tets) — the bundled "
                    "refined sphere, identical to "
                    "crates/geode-core/tests/fixtures/sphere.msh."
                ),
                "data": [],
            },
            "sigma_0": {
                "shape": [1],
                "dtype": "f64",
                "description": "UPML absorption strength at r=R_BUFFER.",
                "data": [SIGMA_0],
            },
            "k0_ref": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Reference wavenumber ω heuristic in the UPML stretch "
                    "s = 1 - jσ(r)/ω (K0_REF in mie_sphere.rs)."
                ),
                "data": [K0_REF],
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
                "description": "PML inner radius — start of the absorbing layer.",
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
            "epsilon_tensor_diag": {
                "shape": [int(n_tets), 3],
                "dtype": "c128",
                "description": (
                    "Per-tet diagonal anisotropic complex permittivity "
                    "tensor (ε_x, ε_y, ε_z), global Cartesian basis. "
                    "Mirror of geode_core::build_anisotropic_pml_tensor_"
                    "diag — see the sphere_mie_small fixture for the "
                    "profile formula. On-disk: row-major (tet, axis) "
                    "flattened, real-imag interleaved per "
                    "reference/SCHEMA.md."
                ),
                "data": _interleave_c128(eps_tensor),
            },
        },
        "outputs": {
            "n_nodes": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of mesh nodes.",
                "tolerance_abs": 0.5,
                "data": [float(n_nodes)],
            },
            "n_tets": {
                "shape": [1],
                "dtype": "f64",
                "description": "Number of mesh tets.",
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
                    "Number of edges that survive PEC reduction."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_interior_edges)],
            },
            "spurious_dim": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Predicted gradient-kernel dimension = number of mesh "
                    "nodes strictly inside the outer PEC wall."
                ),
                "tolerance_abs": 0.5,
                "data": [float(spurious_dim)],
            },
            "n_spurious_observed": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Algebraic spurious-mode dimension = rank(d⁰_interior)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_spurious)],
            },
            "eigenvalues_lowest_complex": {
                "shape": [n_request],
                "dtype": "c128",
                "description": (
                    f"Lowest {n_request} = spurious_dim + 8 complex "
                    "eigenvalues of the tensor-ε pencil, |Re(λ)| "
                    "ascending. On-disk: real-imag interleaved."
                ),
                "tolerance_abs": EIG_C128_TOL_ABS,
                "data": _interleave_c128(eigvals_all),
            },
            "physical_eigenvalues_complex": {
                "shape": [int(len(physical))],
                "dtype": "c128",
                "description": (
                    "Lowest 5 physical complex eigenvalues past the "
                    "d⁰-rank spurious split. Strict cross-IR window = "
                    "first `strict_mode_window_len` entries (closed "
                    "TM_1,1 triplet)."
                ),
                "tolerance_abs": PHYSICAL_C128_TOL_ABS,
                "data": _interleave_c128(physical),
            },
            "strict_mode_window_len": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Strict cross-IR mode window length: the mesh-split "
                    "TM_1,1 triplet (2l+1 = 3), closed at a spectral gap "
                    "per the #160 cluster-closure convention."
                ),
                "tolerance_abs": 0.5,
                "data": [float(STRICT_MODE_WINDOW_LEN)],
            },
            "analytic_tm11_k": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Analytic TM_1,1 PEC-cavity root from the Phase J.1 "
                    "catalogue (reference/fixtures/mie_roots/baseline.json)."
                ),
                "tolerance_abs": ANALYTIC_K_TOL_ABS,
                "data": [analytic_tm11_k],
            },
            "lowest_physical_re_k": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Re(k) = Re(√λ) (principal branch) of the lowest "
                    "physical mode."
                ),
                "tolerance_abs": RE_K_TOL_ABS,
                "data": [lowest_re_k],
            },
            "tm11_rel_err_lowest": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Relative error of the lowest physical Re(k) vs the "
                    "analytic TM_1,1. Must stay below the documented 8% "
                    "band (mie_sphere.rs; observed ≈ 5.7% Burn-side)."
                ),
                "tolerance_abs": RE_K_TOL_ABS,
                "data": [rel_err_tm11],
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Q = Re(k)/(2|Im(k)|) of the lowest physical mode "
                    "(sign-agnostic). Load-bearing assertion is the "
                    "Q > 1.5 tripwire (Q_LOWER_BAND_TM11)."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_factor],
            },
            "q_median_tm11_triplet": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Median Q over the TM_1,1 triplet — mirrors "
                    "mie_sphere_tm11_triplet_q_above_band "
                    "(Q_LOWER_BAND_TM11 = 1.5)."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_median_triplet],
            },
        },
        "provenance": {
            "source": (
                f"reference/numpy/sphere_mie.py @ commit {_git_commit()} ; "
                f"scipy {scipy.__version__}, numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}"
                f".{sys.version_info.micro} ; dense LAPACK ZGGEV "
                "(scipy.linalg.eigvals) on the full-mesh interior pencil"
            ),
            "verified_against": (
                "crates/geode-validation/tests/sphere_mie_numpy_reference.rs "
                "(release-gated #[ignore] full-mesh test; default-CI gate "
                "is the sphere_mie_small sibling)"
            ),
            "issue": "#171 (Epic #88 Phase J.2; anchor from #170)",
        },
    }

    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")

    print()
    print(f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes)")


if __name__ == "__main__":
    main()
