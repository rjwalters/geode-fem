"""Generate `reference/fixtures/sphere_mie_small/jax_baseline.json` from the
JAX anisotropic-UPML Mie pipeline.

Phase J.4 / Issue #173 / Epic #88.

Granularity: **small mesh** (197-tet, shared with `sphere_pml_small/`,
no duplication) — the granularity that default CI can actually check
(#158 / #164 / #160 precedent). The full-mesh dense LAPACK ZGGEV on the
~3300-DOF tensor pencil is multi-minute-to-half-hour and its Burn
cross-check is `#[ignore]`-gated anyway, so a full-mesh JAX snapshot
would be CI-dead weight; the JAX-vs-NumPy delta lives entirely in the
assembly kernels, which the small mesh exercises completely.

Cross-checks performed at generation time (hard gates)
======================================================

1. **NumPy J.2 baseline** (`sphere_mie_small/baseline.json`, PR #179):
   the JAX spectrum must agree per the NumPy fixture's own committed
   `tolerance_abs` (5e-4 full slice, 1e-4 physical band). Dense LAPACK
   on the same pencil — both sides see the whole spectrum, so this is
   a strict per-position diff (no ARPACK cluster-ordering caveat, in
   contrast to the H.3 sphere-PML generator).
2. **J.1 analytic catalogue** (`mie_roots/baseline.json`, PR #177):
   the lowest mode classifies as TM_1,1 inside the documented 8 %
   coarse-mesh band; Q tripwire (> 1.5) on the lowest mode and the
   TM_1,1-triplet median; #160 cluster-closure gap assertion.
3. **σ₀ = 0 collapse**: tensor degenerates to the real isotropic
   scalar; spectrum real to f64 precision; anchor recorded in-fixture.

Autodiff probe (recorded in the fixture)
========================================

`probe_autodiff_tensor_assembly` results — `jit_ok`, `grad_ok`,
`grad_finite` (0/1 flags), `loss_value`, `||grad_re||_∞`,
`||grad_im||_∞` — are recorded as output fields so the Option A drift
gate re-runs the probe in CI on every relevant PR. A `grad_ok = 0`
regression (e.g. a JAX upgrade breaking c128 tensor-kernel VJPs) fails
the gate loudly instead of rotting silently.

CI policy: Option A drift gate
==============================

`.github/workflows/jax-sphere-mie.yml` re-runs this generator on every
PR that touches the JAX Mie pipeline, its kernel dependency, the NumPy
algorithmic source, the committed fixtures, or the workflow itself,
and strictly diffs the fresh fixture against the committed snapshot
per each field's declared `tolerance_abs` (c128 on |Δ|) — same gate as
`jax-sphere-pml.yml` (#159 / #165).

Usage
=====

    python3 reference/jax/gen_sphere_mie_fixture.py
    python3 reference/jax/gen_sphere_mie_fixture.py \\
        --out reference/fixtures/sphere_mie_small/jax_baseline.json
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
REPO_ROOT = HERE.parent.parent
# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187). Package-qualified imports
# disambiguate the same-named NumPy and JAX modules.
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)

MESH_PATH = (
    REPO_ROOT / "reference" / "fixtures" / "sphere_pml_small" / "sphere.msh"
)
NUMPY_BASELINE_PATH = (
    REPO_ROOT / "reference" / "fixtures" / "sphere_mie_small" / "baseline.json"
)


def _default_out_path() -> Path:
    return (
        REPO_ROOT
        / "reference"
        / "fixtures"
        / "sphere_mie_small"
        / "jax_baseline.json"
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


def _interleave_c128(z: np.ndarray) -> list[float]:
    """Real-imag interleaved (row-major flattened) per reference/SCHEMA.md."""
    z = np.ascontiguousarray(z, dtype=np.complex128).reshape(-1)
    return z.view(np.float64).tolist()


def _load_jax_mie():
    """Import reference.jax.sphere_mie lazily (defers the JAX import
    until the solve is actually requested)."""
    from reference.jax import sphere_mie as jax_sphere_mie

    return jax_sphere_mie


# --------------------------------------------------------------------------- #
# Tolerances — kept identical to the NumPy small-mesh fixture (#171) so
# the Burn-side comparator treats both snapshots symmetrically.
# --------------------------------------------------------------------------- #

EPS_C128_TOL_ABS = 1.0e-14
EIG_C128_TOL_ABS = 5.0e-4
PHYSICAL_C128_TOL_ABS = 1.0e-4
Q_FACTOR_TOL_ABS = 5.0
RE_K_TOL_ABS = 1.0e-4
ANALYTIC_K_TOL_ABS = 1.0e-9
SIGMA_ZERO_RE_TOL_ABS = 5.0e-5

# Burn-side acceptance constants mirrored from mie_sphere.rs.
TM11_REL_TOL = 0.08
Q_LOWER_BAND_TM11 = 1.5
STRICT_MODE_WINDOW_LEN = 3

N_INDEX = 1.5


def _probe_value_tol(value: float) -> float:
    """Tolerance for the recorded autodiff probe scalars: 1e-6 relative
    (floored at 1e-9). Expected cross-runner reproducibility is ~1e-12
    relative (f64 XLA on a fixed graph); 1e-6 absorbs BLAS/XLA-version
    summation-order jitter with three orders of margin."""
    return max(1.0e-9, 1.0e-6 * abs(float(value)))


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Generate JAX anisotropic-UPML sphere-Mie reference fixture"
    )
    parser.add_argument("--out", default=str(_default_out_path()))
    parser.add_argument("--sigma0", type=float, default=5.0)
    parser.add_argument("--n-take", type=int, default=5)
    args = parser.parse_args()
    out_path = Path(args.out)

    jax_mie = _load_jax_mie()
    from reference.numpy.sphere_mie import (
        K0_REF,
        classify_modes_against_catalogue,
        load_mie_roots_catalogue,
        q_factor_from_lambda,
    )
    from reference.numpy.sphere_pec import R_BUFFER, R_PML_INNER, R_SPHERE

    print(f"Running anisotropic-UPML Mie JAX pipeline on {MESH_PATH} ...")
    print(f"  sigma_0 = {args.sigma0}, n_index = {N_INDEX}, k0_ref = {K0_REF}")

    result = jax_mie.run_sphere_mie_jax(
        MESH_PATH,
        sigma_0=args.sigma0,
        n_index=N_INDEX,
        k0_ref=K0_REF,
        n_take=args.n_take,
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
    print(
        f"  M complex-symmetry residual = "
        f"{result['m_int_complex_symmetry_residual']:.3e}"
    )

    # ------------------------------------------------------------------ #
    # Gate 1: J.1 analytic anchor + Q tripwire + cluster closure.
    # ------------------------------------------------------------------ #
    roots = load_mie_roots_catalogue()
    tm11 = next(r for r in roots if r["pol"] == "TM" and r["l"] == 1 and r["n"] == 1)
    analytic_tm11_k = tm11["k"]
    table = classify_modes_against_catalogue(physical_ks, roots)

    print("  lowest physical modes:")
    for i, (lam, k, row) in enumerate(zip(physical, physical_ks, table)):
        print(
            f"    [{i}] λ = {lam.real:+.6e} {lam.imag:+.6e}j  "
            f"k = {k.real:.5f} {k.imag:+.5f}j  ->  "
            f"{row['pol']}_{row['l']},{row['n']} "
            f"(rel err = {row['rel_err'] * 100:.2f}%)"
        )

    lowest_re_k = float(physical_ks[0].real)
    rel_err_tm11 = abs(lowest_re_k - analytic_tm11_k) / analytic_tm11_k
    print(
        f"  lowest mode vs analytic TM_1,1 (k = {analytic_tm11_k:.6f}): "
        f"rel err = {rel_err_tm11 * 100:.2f}%"
    )
    assert table[0]["pol"] == "TM" and table[0]["l"] == 1 and table[0]["n"] == 1, (
        f"lowest physical mode classified as "
        f"{table[0]['pol']}_{table[0]['l']},{table[0]['n']} — expected TM_1,1"
    )
    assert rel_err_tm11 < TM11_REL_TOL, (
        f"lowest mode rel err {rel_err_tm11 * 100:.2f}% exceeds the "
        f"{TM11_REL_TOL * 100:.0f}% acceptance band"
    )

    triplet_qs = sorted(
        q_factor_from_lambda(lam) for lam in physical[:STRICT_MODE_WINDOW_LEN]
    )
    q_median_triplet = float(triplet_qs[1])
    print(
        f"  Q lowest = {q_factor:.4f}, TM_1,1 triplet median Q = "
        f"{q_median_triplet:.4f} (band > {Q_LOWER_BAND_TM11})"
    )
    assert q_factor > Q_LOWER_BAND_TM11
    assert q_median_triplet > Q_LOWER_BAND_TM11

    triplet_spread = float(
        physical[STRICT_MODE_WINDOW_LEN - 1].real - physical[0].real
    )
    gap_to_next = float(
        physical[STRICT_MODE_WINDOW_LEN].real
        - physical[STRICT_MODE_WINDOW_LEN - 1].real
    )
    print(
        f"  TM_1,1 triplet spread = {triplet_spread:.4f}, "
        f"gap to next band = {gap_to_next:.4f}"
    )
    assert gap_to_next > 2.0 * triplet_spread, (
        "strict mode window does not end at a spectral gap — "
        "cluster-closure convention (#160) violated"
    )

    # ------------------------------------------------------------------ #
    # Gate 2: cross-check vs the committed NumPy J.2 small baseline.
    # Dense LAPACK on both sides — strict per-position diff against the
    # NumPy fixture's own committed tolerances.
    # ------------------------------------------------------------------ #
    verified_note: str
    if not NUMPY_BASELINE_PATH.exists():
        verified_note = (
            "NumPy sphere_mie_small baseline not found at generation time; "
            "cross-check skipped."
        )
        print(f"\nWARNING: {verified_note}")
    else:
        with open(NUMPY_BASELINE_PATH) as f:
            np_baseline = json.load(f)
        np_out = np_baseline["outputs"]

        def _c128_field(name: str) -> np.ndarray:
            flat = np.asarray(np_out[name]["data"], dtype=np.float64)
            return flat.view(np.complex128)

        np_eig_all = _c128_field("eigenvalues_lowest_complex")
        np_physical = _c128_field("physical_eigenvalues_complex")
        np_eps = _c128_field("epsilon_tensor_diag") if (
            "epsilon_tensor_diag" in np_out
        ) else np.asarray(
            np_baseline["inputs"]["epsilon_tensor_diag"]["data"],
            dtype=np.float64,
        ).view(np.complex128)

        d_eps = float(np.max(np.abs(eps_tensor.reshape(-1) - np_eps)))
        assert len(np_eig_all) == len(eigvals_all), (
            f"slice length mismatch: JAX {len(eigvals_all)} vs "
            f"NumPy {len(np_eig_all)}"
        )
        d_all = float(np.max(np.abs(eigvals_all - np_eig_all)))
        d_phys = float(np.max(np.abs(physical - np_physical[: len(physical)])))
        print("\nCross-check vs NumPy sphere_mie_small/baseline.json:")
        print(f"  epsilon_tensor_diag        max |Δ| = {d_eps:.3e} "
              f"(tol {EPS_C128_TOL_ABS:.0e})")
        print(f"  eigenvalues_lowest_complex max |Δ| = {d_all:.3e} "
              f"(tol {EIG_C128_TOL_ABS:.0e})")
        print(f"  physical_eigenvalues       max |Δ| = {d_phys:.3e} "
              f"(tol {PHYSICAL_C128_TOL_ABS:.0e})")
        assert d_eps < EPS_C128_TOL_ABS, (
            f"tensor profile drifted from NumPy: |Δ| = {d_eps:.3e}"
        )
        assert d_all < EIG_C128_TOL_ABS, (
            f"full-slice spectrum disagrees with NumPy: |Δ| = {d_all:.3e}"
        )
        assert d_phys < PHYSICAL_C128_TOL_ABS, (
            f"physical band disagrees with NumPy: |Δ| = {d_phys:.3e}"
        )
        verified_note = (
            f"NumPy J.2 small baseline (PR #179): epsilon_tensor_diag "
            f"max |Δ| = {d_eps:.3e}; eigenvalues_lowest_complex "
            f"max |Δ| = {d_all:.3e}; physical band max |Δ| = "
            f"{d_phys:.3e} (dense LAPACK ZGGEV both sides — strict "
            f"per-position diff, no ARPACK ordering caveat)."
        )

    # ------------------------------------------------------------------ #
    # Gate 3: σ₀ = 0 collapse anchor.
    # ------------------------------------------------------------------ #
    result_pec = jax_mie.run_sphere_mie_jax(
        MESH_PATH,
        sigma_0=0.0,
        n_index=N_INDEX,
        k0_ref=K0_REF,
        n_take=args.n_take,
        r_outer=R_BUFFER,
    )
    pec_lowest_re = float(result_pec["physical_eigenvalues"][0].real)
    pec_max_imag_rel = float(result_pec["max_imag_eigval_rel"])
    print(
        f"\nσ₀=0 PEC anchor: lowest physical Re(λ) = {pec_lowest_re:.6e}, "
        f"max|Im(λ)|/max(|Re(λ)|,1) = {pec_max_imag_rel:.3e}"
    )
    assert pec_max_imag_rel < 1e-10

    # ------------------------------------------------------------------ #
    # Autodiff probe — the Phase J.4 spec payoff, recorded in-fixture.
    # ------------------------------------------------------------------ #
    print("\nAutodiff probe: jax.grad through the tensor-ε assembly path ...")
    probe = jax_mie.probe_autodiff_tensor_assembly(
        mesh_path=MESH_PATH, sigma_0=args.sigma0, k0_ref=K0_REF
    )
    print(f"  jit_ok = {probe['jit_ok']}, grad_ok = {probe['grad_ok']}, "
          f"grad_finite = {probe['grad_finite']}")
    print(f"  loss_value = {probe['loss_value']}")
    print(f"  ||grad_re||_∞ = {probe['grad_max_abs_re']}, "
          f"||grad_im||_∞ = {probe['grad_max_abs_im']}")
    if probe["errors"]:
        print(f"  errors = {probe['errors']}")
    assert probe["jit_ok"] and probe["grad_ok"] and probe["grad_finite"], (
        "autodiff probe failed on the tensor-ε path — this partially "
        "reverses the Phase H.3 finding and must be filed on #88 + #5 "
        f"(errors: {probe['errors']})"
    )

    # ------------------------------------------------------------------ #
    # Emit the fixture.
    # ------------------------------------------------------------------ #
    fixture = {
        "schema_version": "1",
        "fixture_id": "sphere_mie_small/n48_aniso_upml_mie_jax",
        "description": (
            "JAX reference for the small-mesh anisotropic-UPML "
            "dielectric-sphere Mie eigenmode pipeline (issue #173, Epic "
            "#88 Phase J.4). Port of the J.2 NumPy reference "
            "(sphere_mie_small/baseline.json, PR #179): per-element "
            "curl-curl via the Phase G.3 JAX kernel, per-element mass "
            "via the tensor-valued complex kernel (per-axis cofactor "
            "gram contracted with the diagonal UPML tensor, jax.vmap/"
            "jit, c128 under jax_enable_x64), global scatter through "
            "jax.experimental.sparse BCOO[complex128] + sum_duplicates, "
            "eigensolve out-of-graph on host LAPACK ZGGEV (dense, "
            "canonical-tiebreaker path — same sidecar boundary as the "
            "NumPy reference). Mesh: 197-tet sphere shared with "
            "sphere_pml_small (#158), σ₀ = 5.0, k₀_ref = 2.0. Physical "
            "Im(λ) < 0 on this small mesh's tensor pencil (mesh-"
            "dependent sign, see the J.2 fixture description). Includes "
            "the Phase J.4 autodiff probe verdict: jax.grad traces the "
            "TENSOR-valued complex-ε assembly path with zero custom "
            "VJPs (closing the H.3 scalar-only caveat on #88). Option A "
            "CI gate: regenerated by .github/workflows/jax-sphere-mie."
            "yml and strictly diffed against this snapshot per field "
            "tolerance_abs (c128 on |Δ|)."
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
                "description": "UPML absorption strength at r=R_BUFFER.",
                "data": [float(args.sigma0)],
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
                    "tensor (ε_x, ε_y, ε_z), global Cartesian basis — "
                    "mirror of geode_core::build_anisotropic_pml_tensor_"
                    "diag (see the J.2 fixture for the profile formula). "
                    "On-disk: row-major (tet, axis) flattened, real-imag "
                    "interleaved per reference/SCHEMA.md."
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
                    "eigenvalues of the tensor-ε pencil K x = λ M x, "
                    "sorted by |Re(λ)| ascending (Burn "
                    "FaerComplexEigensolver order). On-disk: real-imag "
                    "interleaved per reference/SCHEMA.md."
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
                    "Im(λ) < 0 on this tensor pencil (mesh-dependent "
                    "sign). Strict cross-IR window = first "
                    "`strict_mode_window_len` entries (the closed TM_1,1 "
                    "triplet, #160 cluster-closure convention)."
                ),
                "tolerance_abs": PHYSICAL_C128_TOL_ABS,
                "data": _interleave_c128(physical),
            },
            "strict_mode_window_len": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Length of the strict cross-IR mode window: the "
                    "mesh-split TM_1,1 triplet (multiplicity 2l+1 = 3), "
                    "closed at a spectral gap per #160."
                ),
                "tolerance_abs": 0.5,
                "data": [float(STRICT_MODE_WINDOW_LEN)],
            },
            "analytic_tm11_k": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Analytic TM_1,1 root from the Phase J.1 catalogue "
                    "(reference/fixtures/mie_roots/baseline.json) — the "
                    "8% coarse-mesh acceptance anchor."
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
                    "analytic TM_1,1 root (must stay below the 8% band)."
                ),
                "tolerance_abs": RE_K_TOL_ABS,
                "data": [rel_err_tm11],
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Q = Re(k)/(2|Im(k)|) of the lowest physical mode "
                    "(sign-agnostic). Loose tolerance — dQ/dIm(λ) ≈ "
                    "Q/|Im(λ)| amplification; the load-bearing assertion "
                    "is the Q > 1.5 tripwire."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_factor],
            },
            "q_median_tm11_triplet": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Median Q over the TM_1,1 triplet (strict window) — "
                    "mirrors the Burn-side Q_LOWER_BAND_TM11 = 1.5 tripwire."
                ),
                "tolerance_abs": Q_FACTOR_TOL_ABS,
                "data": [q_median_triplet],
            },
            "sigma_zero_lowest_physical_re": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Lowest physical Re(λ) at σ₀ = 0 (PEC limit; the "
                    "tensor collapses to the real isotropic scalar)."
                ),
                "tolerance_abs": SIGMA_ZERO_RE_TOL_ABS,
                "data": [pec_lowest_re],
            },
            "autodiff_jit_ok": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Phase J.4 autodiff probe: 1.0 iff jax.jit lowers the "
                    "tensor-ε complex assembly loss without errors."
                ),
                "tolerance_abs": 0.5,
                "data": [1.0 if probe["jit_ok"] else 0.0],
            },
            "autodiff_grad_ok": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Phase J.4 autodiff probe: 1.0 iff jax.grad produces "
                    "a gradient through the tensor-valued complex-ε "
                    "kernel with ZERO custom VJPs. A 0.0 here partially "
                    "reverses the Phase H.3 scalar-isotropic finding and "
                    "must be filed on #88 + #5."
                ),
                "tolerance_abs": 0.5,
                "data": [1.0 if probe["grad_ok"] else 0.0],
            },
            "autodiff_grad_finite": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Phase J.4 autodiff probe: 1.0 iff both gradient "
                    "halves are finite (no NaN/inf)."
                ),
                "tolerance_abs": 0.5,
                "data": [1.0 if probe["grad_finite"] else 0.0],
            },
            "autodiff_loss_value": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Probe loss tr(K_int) + |Tr(M_int)|² at the UPML "
                    "tensor evaluation point (H.3-comparable functional). "
                    "Documentation, not validation; tolerance is 1e-6 "
                    "relative to absorb cross-runner XLA/BLAS jitter."
                ),
                "tolerance_abs": _probe_value_tol(probe["loss_value"]),
                "data": [float(probe["loss_value"])],
            },
            "autodiff_grad_max_abs_re": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "||∂loss/∂Re(ε_diag)||_∞ over the (n_tets, 3) tensor "
                    "parameter. Documentation, not validation."
                ),
                "tolerance_abs": _probe_value_tol(probe["grad_max_abs_re"]),
                "data": [float(probe["grad_max_abs_re"])],
            },
            "autodiff_grad_max_abs_im": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "||∂loss/∂Im(ε_diag)||_∞ over the (n_tets, 3) tensor "
                    "parameter — the absorption-axis gradient. "
                    "Documentation, not validation."
                ),
                "tolerance_abs": _probe_value_tol(probe["grad_max_abs_im"]),
                "data": [float(probe["grad_max_abs_im"])],
            },
        },
        "provenance": {
            "source": (
                f"reference/jax/sphere_mie.py @ commit {_git_commit()} "
                f"(Epic #88 / Phase J.4 / Issue #173); "
                f"jax {__import__('jax').__version__}, "
                f"numpy {np.__version__}, "
                f"python {sys.version_info.major}.{sys.version_info.minor}"
                f".{sys.version_info.micro}; assembly via jax.vmap/jit "
                "c128 kernels + BCOO[complex128] scatter; eigensolve "
                "out-of-graph via dense LAPACK ZGGEV (scipy.linalg.eigvals)"
            ),
            "verified_against": verified_note,
            "issue": "#173 (Epic #88 Phase J.4; J.2 source #171, anchor #170)",
        },
    }

    out_path.parent.mkdir(parents=True, exist_ok=True)
    with open(out_path, "w") as f:
        json.dump(fixture, f, indent=2)
        f.write("\n")
    print(f"\nWrote {out_path} ({os.path.getsize(out_path)} bytes)")
    print(f"  generator_commit = {_git_commit()}")


if __name__ == "__main__":
    main()
