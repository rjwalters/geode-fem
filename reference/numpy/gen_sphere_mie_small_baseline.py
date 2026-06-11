"""Generate the small-mesh anisotropic-UPML Mie baseline fixture (issue #171).

Phase J.2 (Epic #88) small-mesh sibling, following the #158 pattern:
the full-mesh ``sphere_mie`` fixture's Burn faer 0.24 complex GEVD on
the ~3300-DOF interior pencil takes 60+ minutes, so its cross-check is
``#[ignore]``-gated and release-only. This small-mesh sibling reuses
the 197-tet mesh from ``reference/fixtures/sphere_pml_small/sphere.msh``
(no mesh duplication — the fixture's ``mesh_path`` input points there)
so the canonical Burn vs NumPy anisotropic-UPML spectrum check fits in
the default ``cargo test -p geode-validation`` budget.

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_sphere_mie_small_baseline.py

What this fixture pins
======================

- **Anisotropic tensor ε**: full per-tet diagonal UPML tensor
  ``epsilon_tensor_diag`` (shape ``(n_tets, 3)``, row-major flattened,
  c128-interleaved) matching
  ``geode_core::build_anisotropic_pml_tensor_diag`` bit-for-bit at
  σ₀ = 5.0, k₀_ref = 2.0 (the `mie_sphere.rs` acceptance parameters).
- **Complex eigenvalue spectrum**: lowest ``spurious_dim + 8`` complex
  eigenvalues of the tensor-ε pencil, sorted by ``|Re(λ)|`` ascending.
- **Physical eigenvalues**: lowest 5 past the d⁰-rank spurious split.
- **Strict cross-IR mode window** (#160 cluster-closure convention):
  the first **3** physical modes — the mesh-split TM_1,1 triplet
  (multiplicity 2l+1 = 3) — form the tight-tolerance window. Taking 5
  would bisect the next multiplet (TE_1,1 / TM_2,1 band starting at
  λ ≈ 3.3); the remaining positions [3, 4] are still compared, but the
  window field documents where the closed cluster ends.
- **J.1 analytic anchor**: ``analytic_tm11_k`` re-exported from
  ``reference/fixtures/mie_roots/baseline.json`` (TM_1,1, k ≈ 1.30343)
  plus the observed lowest-mode relative error (≈ 6.6 % on this coarse
  mesh — inside the documented 8 % Burn-side acceptance band).
- **Q tripwire**: Q of the lowest mode and the TM_1,1-triplet median Q
  (the `Q_LOWER_BAND_TM11 = 1.5` PML-misconfiguration tripwire from
  `mie_sphere.rs`).
- **σ₀ = 0 PEC anchor**: in-fixture lowest physical Re(λ) at σ₀ = 0
  (the tensor collapses to real isotropic scalar ε, identical to the
  sphere_pml_small anchor — kept in-fixture for self-containment).

Sign convention note
====================

Unlike the scalar-isotropic PML (PR #155: ``Im(λ) > 0``), the
anisotropic UPML pencil's physical eigenvalues come out with
``Im(λ) < 0`` on this mesh: the radial tensor entry carries ``1/s_r``
(``Im > 0``) while the transverse entries carry ``s_t`` (``Im < 0``),
and the net sign is a property of the pencil — not a solver choice.
Both LAPACK ZGGEV and faer QZ agree on it (eigenvalues of a fixed
complex-symmetric pencil are uniquely determined; only eigenvector
phase is ambiguous). The Q-factor stays sign-agnostic.
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
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "sphere_mie_small"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"
# Reuse the #158 small mesh — no duplication.
MESH_PATH = (
    REPO_ROOT / "reference" / "fixtures" / "sphere_pml_small" / "sphere.msh"
)

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.sphere_mie import (  # noqa: E402
    K0_REF,
    SIGMA_0_DEFAULT,
    classify_modes_against_catalogue,
    load_mie_roots_catalogue,
    q_factor_from_lambda,
    run_sphere_mie,
)
from reference.numpy.sphere_pec import R_BUFFER, R_PML_INNER, R_SPHERE  # noqa: E402

# --------------------------------------------------------------------------- #
# Tolerances applied to the on-disk baseline.
# --------------------------------------------------------------------------- #

# Bit-exact c128 round-trip of the tensor profile (pure f64 host math on
# both sides).
EPS_C128_TOL_ABS = 1.0e-14
# Same floors as the sphere_pml_small fixture (#158): the small-mesh
# pencil's near-zero spurious cluster inflates faer 0.24 QZ vs LAPACK
# ZGGEV residuals to ~1e-4 absolute; the physical band is tighter.
EIG_C128_TOL_ABS = 5.0e-4
PHYSICAL_C128_TOL_ABS = 1.0e-4
# Q of the lowest mode is ~265 on this mesh: with Im(λ) ≈ -7.3e-3 the
# sensitivity dQ/dIm(λ) ≈ Q/|Im(λ)| ≈ 3.6e4, so a 1e-4 eigenvalue
# residual translates to O(1) on Q. 5.0 absolute is the defensible
# floor (≈ 2 % relative); the tripwire band check (Q > 1.5) is the
# load-bearing assertion, not this regression floor.
Q_FACTOR_TOL_ABS = 5.0
# Re(k) of the lowest mode: |dk/dλ| = 1/(2k) ≈ 0.36, so a 1e-4 λ
# residual is ~4e-5 on k. 1e-4 absolute.
RE_K_TOL_ABS = 1.0e-4
# Analytic TM_1,1 anchor from the J.1 catalogue: brentq at machine
# precision on both sides; 1e-9 absolute.
ANALYTIC_K_TOL_ABS = 1.0e-9
# σ₀ = 0 PEC anchor — same floor as the sphere_pml_small fixture.
SIGMA_ZERO_RE_TOL_ABS = 5.0e-5

N_INDEX = 1.5
SIGMA_0 = SIGMA_0_DEFAULT  # 5.0

# Burn-side acceptance constants mirrored from
# `crates/geode-core/tests/mie_sphere.rs`.
TM11_REL_TOL = 0.08
Q_LOWER_BAND_TM11 = 1.5

# Strict cross-IR window (#160 cluster closure): the TM_1,1 triplet.
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
    """Serialize a complex128 array (row-major flattened) to the
    canonical real-imag interleaved flat list per ``reference/SCHEMA.md``."""
    z = np.ascontiguousarray(z, dtype=np.complex128).reshape(-1)
    return z.view(np.float64).tolist()


def main() -> None:
    print(f"Running anisotropic-UPML Mie NumPy pipeline on {MESH_PATH} ...")
    print(f"  sigma_0 = {SIGMA_0}, n_index = {N_INDEX}, k0_ref = {K0_REF}")

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
    print(f"  M complex-symmetry residual = "
          f"{result['m_int_complex_symmetry_residual']:.3e}")

    # J.1 analytic catalogue anchor.
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

    # Acceptance gate 1: the lowest mode classifies as TM_1,1 and lands
    # inside the documented 8 % coarse-mesh band.
    lowest_re_k = float(physical_ks[0].real)
    rel_err_tm11 = abs(lowest_re_k - analytic_tm11_k) / analytic_tm11_k
    print(f"  lowest mode vs analytic TM_1,1 (k = {analytic_tm11_k:.6f}): "
          f"rel err = {rel_err_tm11 * 100:.2f}%")
    assert table[0]["pol"] == "TM" and table[0]["l"] == 1 and table[0]["n"] == 1, (
        f"lowest physical mode classified as "
        f"{table[0]['pol']}_{table[0]['l']},{table[0]['n']} — expected TM_1,1"
    )
    assert rel_err_tm11 < TM11_REL_TOL, (
        f"lowest mode rel err {rel_err_tm11 * 100:.2f}% exceeds the "
        f"documented {TM11_REL_TOL * 100:.0f}% Burn-side acceptance band"
    )

    # Acceptance gate 2: Q tripwire — lowest mode and TM_1,1-triplet
    # median both above the Burn-side lower band.
    triplet_qs = sorted(
        q_factor_from_lambda(lam) for lam in physical[:STRICT_MODE_WINDOW_LEN]
    )
    q_median_triplet = float(triplet_qs[1])
    print(f"  Q lowest = {q_factor:.4f}, TM_1,1 triplet median Q = "
          f"{q_median_triplet:.4f} (band > {Q_LOWER_BAND_TM11})")
    assert q_factor > Q_LOWER_BAND_TM11
    assert q_median_triplet > Q_LOWER_BAND_TM11

    # Cluster-closure sanity (#160): the strict window must end at a
    # spectral gap — the TM_1,1 triplet (λ ≈ 1.9-2.1) is separated from
    # the next band (λ ≈ 3.3) by a gap much larger than the intra-
    # triplet mesh splitting.
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

    # σ₀ = 0 PEC anchor — tensor collapses to real isotropic scalar.
    result_pec = run_sphere_mie(
        MESH_PATH,
        sigma_0=0.0,
        n_index=N_INDEX,
        k0_ref=K0_REF,
        n_take=5,
        r_outer=R_BUFFER,
    )
    pec_lowest_re = float(result_pec["physical_eigenvalues"][0].real)
    pec_max_imag_rel = float(result_pec["max_imag_eigval_rel"])
    print(f"  σ₀=0 PEC anchor: lowest physical Re(λ) = {pec_lowest_re:.6e}, "
          f"max|Im(λ)|/max(|Re(λ)|,1) = {pec_max_imag_rel:.3e}")
    assert pec_max_imag_rel < 1e-10

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "sphere_mie_small/n48_aniso_upml_mie",
        "description": (
            "Small-mesh anisotropic-UPML dielectric-sphere Mie eigenmode "
            "(issue #171, Epic #88 Phase J.2). Reuses the 197-tet mesh "
            "from reference/fixtures/sphere_pml_small/sphere.msh (#158) "
            "with the diagonal UPML tensor "
            "(geode_core::build_anisotropic_pml_tensor_diag, σ₀ = 5.0, "
            "k₀_ref = 2.0 — the mie_sphere.rs acceptance parameters). "
            "Anchored to the Phase J.1 analytic Mie-root catalogue "
            "(reference/fixtures/mie_roots/baseline.json): the lowest "
            "physical mode is the mesh-split TM_1,1 triplet's leading "
            "member at ~6.6% of the analytic k ≈ 1.30343. Strict "
            "cross-IR window = first 3 physical modes (the closed "
            "TM_1,1 triplet, #160 cluster-closure convention). Sign "
            "note: physical Im(λ) < 0 on this small mesh's tensor "
            "pencil (unlike the scalar-PML Im(λ) > 0 of PR #155); the "
            "sign is mesh-dependent — the refined full-mesh sibling "
            "(sphere_mie/) shows Im(λ) > 0 — and stems from the "
            "mixed-sign 1/s_r vs s_t tensor entries. It is a property "
            "of the pencil, agreed on by both LAPACK ZGGEV and faer QZ."
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
                    "reference/fixtures/sphere_pml_small/sphere.msh "
                    f"({n_nodes} nodes, {n_tets} tets) — shared with the "
                    "#158 sphere_pml_small fixture, not duplicated."
                ),
                "data": [],
            },
            "sigma_0": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "UPML absorption strength at r=R_BUFFER (mie_sphere.rs "
                    "acceptance value)."
                ),
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
                    "tensor (ε_x, ε_y, ε_z) in the global Cartesian "
                    "basis. Profile: real isotropic n² in the dielectric, "
                    "1 in the vacuum gap; in the PML shell ε_α = "
                    "(1/s) r̂_α² + s (1 - r̂_α²) with s = 1 - jσ(r_c)/ω, "
                    "σ(r) = σ₀ clamp((r-R_PML_INNER)/(R_BUFFER-"
                    "R_PML_INNER),0,1)². Mirror of geode_core::"
                    "build_anisotropic_pml_tensor_diag. On-disk: row-major "
                    "(tet, axis) flattened, real-imag interleaved per "
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
                    "nodes strictly inside the outer PEC wall."
                ),
                "tolerance_abs": 0.5,
                "data": [float(spurious_dim)],
            },
            "n_spurious_observed": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Algebraic spurious-mode dimension = rank(d⁰_interior) "
                    "(`spurious_dim_from_derham`). Invariant under the "
                    "tensor-ε scaling on the mass."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_spurious)],
            },
            "eigenvalues_lowest_complex": {
                "shape": [n_request],
                "dtype": "c128",
                "description": (
                    f"Lowest {n_request} = spurious_dim + 8 complex "
                    "eigenvalues of the tensor-ε pencil K x = λ M x "
                    "(K real, M complex-symmetric), sorted by |Re(λ)| "
                    "ascending (Burn FaerComplexEigensolver order). "
                    "On-disk: real-imag interleaved per reference/SCHEMA.md."
                ),
                "tolerance_abs": EIG_C128_TOL_ABS,
                "data": _interleave_c128(eigvals_all),
            },
            "physical_eigenvalues_complex": {
                "shape": [int(len(physical))],
                "dtype": "c128",
                "description": (
                    "Lowest 5 physical complex eigenvalues past the "
                    "d⁰-rank spurious split, |Re(λ)| ascending. Physical "
                    "Im(λ) < 0 on this tensor pencil (see fixture "
                    "description). The strict cross-IR window is the "
                    "first `strict_mode_window_len` entries (the closed "
                    "TM_1,1 triplet); positions [3, 4] belong to the "
                    "next band and are compared at the same tolerance "
                    "but excluded from the cluster-closure claim."
                ),
                "tolerance_abs": PHYSICAL_C128_TOL_ABS,
                "data": _interleave_c128(physical),
            },
            "strict_mode_window_len": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Length of the strict cross-IR mode window: the "
                    "mesh-split TM_1,1 triplet (multiplicity 2l+1 = 3). "
                    "Chosen at a spectral gap per the #160 cluster-"
                    "closure convention — never bisect a degenerate "
                    "multiplet (taking 5 would cut into the TE_1,1 / "
                    "TM_2,1 band at λ ≈ 3.3)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(STRICT_MODE_WINDOW_LEN)],
            },
            "analytic_tm11_k": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Analytic TM_1,1 PEC-cavity root from the Phase J.1 "
                    "catalogue (reference/fixtures/mie_roots/baseline.json"
                    ", pol=TM, l=1, n=1). The 8% coarse-mesh acceptance "
                    "anchor."
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
                    "analytic TM_1,1 root. Must stay below the documented "
                    "8% coarse-mesh band (mie_sphere.rs)."
                ),
                "tolerance_abs": RE_K_TOL_ABS,
                "data": [rel_err_tm11],
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Q = Re(k)/(2|Im(k)|) of the lowest physical mode "
                    "(sign-agnostic). Tolerance is loose (5.0 absolute on "
                    "Q ≈ 265) because dQ/dIm(λ) ≈ Q/|Im(λ)| ≈ 3.6e4 "
                    "amplifies eigenvalue residuals; the load-bearing "
                    "assertion is the Q > 1.5 tripwire "
                    "(Q_LOWER_BAND_TM11), not this regression floor."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_factor],
            },
            "q_median_tm11_triplet": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Median Q over the TM_1,1 triplet (strict window) — "
                    "mirrors the mie_sphere_tm11_triplet_q_above_band "
                    "Burn-side tripwire (Q_LOWER_BAND_TM11 = 1.5)."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_median_triplet],
            },
            "sigma_zero_lowest_physical_re": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Lowest physical Re(λ) at σ₀ = 0 (PEC limit; the "
                    "tensor collapses to real isotropic scalar ε). "
                    "In-fixture anchor for the σ₀ = 0 collapse test; "
                    "numerically identical to the sphere_pml_small anchor."
                ),
                "tolerance_abs": SIGMA_ZERO_RE_TOL_ABS,
                "data": [pec_lowest_re],
            },
        },
        "provenance": {
            "source": (
                f"reference/numpy/sphere_mie.py @ commit {_git_commit()} ; "
                f"scipy {scipy.__version__}, numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}"
                f".{sys.version_info.micro} ; dense LAPACK ZGGEV "
                "(scipy.linalg.eigvals) on the small-mesh interior pencil"
            ),
            "verified_against": (
                "crates/geode-validation/tests/sphere_mie_numpy_reference.rs "
                "(Burn ndarray backend, default `cargo test` — no release flag)"
            ),
            "issue": "#171 (Epic #88 Phase J.2; mesh from #158, anchor from #170)",
        },
    }

    with open(FIXTURE_PATH, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")

    print()
    print(f"Wrote {FIXTURE_PATH} ({os.path.getsize(FIXTURE_PATH)} bytes)")


if __name__ == "__main__":
    main()
