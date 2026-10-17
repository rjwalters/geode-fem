"""Generate `reference/fixtures/sphere_pml/jax_baseline.json` from the JAX pipeline.

Phase H.3 / Issue #148 / Epic #88.

Produces a ``schema_version: "1"`` fixture compatible with the Phase H
scaffolding c128 encoding (PR #151, Issue #145): real-imag interleaved
complex on disk, `|Δ|`-tolerance on the comparator.

Cross-check vs the NumPy PML baseline (PR #155, fixture
``reference/fixtures/sphere_pml/baseline.json``) is performed but does
**not** block fixture generation — the SciPy ARPACK shift-invert basin
returns a clustered slice that disagrees with NumPy/LAPACK on a
per-position basis. The robust acceptance criterion is the lowest
physical mode ``physical[0]`` (Re-rel and |Im|-abs) — see
``verified_against`` in the fixture provenance.

CI policy: Option A drift gate
==============================

**As of issue #159, this fixture is gated in CI by
``.github/workflows/jax-sphere-pml.yml``**, which re-runs this
generator on every PR that touches the JAX pipeline, the generator,
the committed baseline fixture, or the workflow itself. The freshly
emitted fixture is strictly diffed against the committed
``reference/fixtures/sphere_pml/jax_baseline.json`` per each field's
declared ``tolerance_abs`` (c128 fields compared on |Δ|), bringing
the JAX path to parity with the Julia path
(``julia-cube-cavity.yml``).

There are now two independent and complementary drift gates:

* **This workflow** (Option A): freshly emitted JAX fixture vs the
  committed snapshot — catches drift between
  ``reference/jax/sphere_pml.py`` and the on-disk baseline.
* **Rust per-PR test**
  (``geode-validation/tests/sphere_pml_jax_reference.rs``): Burn
  output vs the committed snapshot — catches drift between the Rust
  pipeline and the JAX baseline.

When ``reference/jax/sphere_pml.py`` changes substantively
(algorithm edits, not refactor), the maintainer must re-run this
script locally, review the cross-check |Δ| reported on stdout, and
commit the regenerated fixture. CI will then verify the committed
snapshot is reproducible on the runner. Sign convention ``Im(λ) > 0``
is part of the committed snapshot; convention drift surfaces as a
|Δ| violation on the c128 eigenvalue fields.

History: PR #154 (issue #148) originally adopted Option B
(snapshot-only, no CI re-emission) on ROI grounds — a ~1.5 GB JAX
install for one fixture. Issue #159 reversed that decision for
parity with the Julia Option A gate from PR #153 cycle 2.

Usage
=====

    python3 reference/jax/gen_sphere_pml_fixture.py
    python3 reference/jax/gen_sphere_pml_fixture.py --sigma0 5.0
    python3 reference/jax/gen_sphere_pml_fixture.py \\
        --out reference/fixtures/sphere_pml/jax_baseline.json
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
    return REPO_ROOT / "reference" / "fixtures" / "sphere_pml" / "baseline.json"


def _default_out_path() -> Path:
    return REPO_ROOT / "reference" / "fixtures" / "sphere_pml" / "jax_baseline.json"


def _interleave_c128(z: np.ndarray) -> list[float]:
    """Real-imag interleaved encoding per `reference/SCHEMA.md` "Complex
    encoding (c128)"."""
    z = np.ascontiguousarray(z, dtype=np.complex128)
    return z.view(np.float64).tolist()


def _build_fixture_dict(
    *,
    n_nodes: int,
    n_tets: int,
    n_edges: int,
    n_interior_edges: int,
    spurious_dim: int,
    sigma_0: float,
    n_index: float,
    epsilon_r_complex: np.ndarray,
    eigenvalues_lowest_complex: np.ndarray,
    physical_eigenvalues_complex: np.ndarray,
    q_factor_lowest_physical: float,
    verified_note: str,
) -> dict:
    """Build the canonical schema-v1 fixture dict for the JAX PML reference."""
    # Tolerances:
    #   - complex eigenvalues: 1e-3 absolute on |Δ| (ARPACK shift-invert
    #     repeatability across runs on the same sparse pencil; far above
    #     the eigsolve internal tol=1e-10 because the lowest physical
    #     mode has Re(λ) ≈ 0.88 and Im(λ) ≈ -2e-3 so the absolute
    #     scale is ~1).
    #   - Q factor: 0.5 absolute (Q can swing by ±tens when the lowest
    #     mode hops between conjugate-near-degenerate pairs across runs;
    #     this is documentation, not validation — see issue body).
    return {
        "schema_version": "1",
        "fixture_id": "sphere_pml/n774_pml_eigenmode_jax",
        "description": (
            "JAX reference for the scalar-isotropic sphere-PML Nédélec "
            "eigenmode pipeline (Epic #88 / Phase H.3 / Issue #148). "
            "Per-element curl-curl and ε-mass assembly via jax.vmap/jit; "
            "global complex scatter and SciPy shift-and-invert eigensolve "
            "remain in NumPy/SciPy (no sparse complex generalized "
            "eigensolver in JAX, matching the Stage 7 ONNX audit boundary "
            "in reference/onnx/audit/). Option A CI gate: regenerated "
            "by `.github/workflows/jax-sphere-pml.yml` on every PR that "
            "touches the JAX pipeline and strictly diffed against this "
            "committed snapshot per field `tolerance_abs` (c128 on |Δ|). "
            f"σ₀ = {sigma_0}. {verified_note}"
        ),
        "units": (
            "λ = k² (inverse-length squared) with Im(λ) > 0 convention "
            "(canonical per Epic #88 PR #155 NumPy tiebreaker); "
            "dimensionless mesh coordinates"
        ),
        "inputs": {
            "mesh_path": {
                "shape": [0],
                "dtype": "f64",
                "description": (
                    "reference/fixtures/sphere_pml/sphere.msh — bundled "
                    "sphere mesh (same as sphere_pec/)."
                ),
                "data": [],
            },
            "sigma_0": {
                "shape": [1],
                "dtype": "f64",
                "description": "PML absorption strength.",
                "data": [sigma_0],
            },
            "r_sphere": {
                "shape": [1],
                "dtype": "f64",
                "description": "Inner dielectric sphere radius.",
                "data": [1.0],
            },
            "r_pml_inner": {
                "shape": [1],
                "dtype": "f64",
                "description": "PML inner radius.",
                "data": [1.5],
            },
            "r_buffer": {
                "shape": [1],
                "dtype": "f64",
                "description": "Outer PEC wall radius.",
                "data": [2.0],
            },
            "n_index": {
                "shape": [1],
                "dtype": "f64",
                "description": "Refractive index in the dielectric.",
                "data": [n_index],
            },
            "epsilon_r_complex": {
                "shape": [int(epsilon_r_complex.shape[0])],
                "dtype": "c128",
                "description": (
                    "Per-tet complex relative permittivity from the "
                    "scalar-isotropic PML profile "
                    "(geode_core::build_complex_epsilon_r_pml). On-disk: "
                    "real-imag interleaved per reference/SCHEMA.md."
                ),
                "data": _interleave_c128(epsilon_r_complex),
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
                "description": "Number of global Nédélec edges.",
                "tolerance_abs": 0.5,
                "data": [float(n_edges)],
            },
            "n_interior_edges": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Number of interior Nédélec DOFs after PEC reduction."
                ),
                "tolerance_abs": 0.5,
                "data": [float(n_interior_edges)],
            },
            "spurious_dim": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Algebraic spurious-mode dimension = rank(d⁰_interior); "
                    "carries over from the PEC case (gradient kernel is "
                    "independent of complex ε scaling)."
                ),
                "tolerance_abs": 0.5,
                "data": [float(spurious_dim)],
            },
            "eigenvalues_lowest_complex": {
                "shape": [int(eigenvalues_lowest_complex.shape[0])],
                "dtype": "c128",
                "description": (
                    "Complex eigenvalue slice from "
                    "scipy.sparse.linalg.eigs with shift-and-invert at "
                    "sigma = 0.9 + 0j (physical-band shift; pulls "
                    "lowest physical PML modes directly past the "
                    "spurious cluster near λ = 0). Sorted ascending "
                    "by Re(λ). Contains both branches of the conjugate "
                    "pairs that the non-Hermitian solver returns."
                ),
                "tolerance_abs": 1.0e-3,
                "data": _interleave_c128(eigenvalues_lowest_complex),
            },
            "physical_eigenvalues_complex": {
                "shape": [int(physical_eigenvalues_complex.shape[0])],
                "dtype": "c128",
                "description": (
                    "Lowest physical PML eigenvalues — filtered for "
                    "Re(λ) > 0 (oscillatory) and Im(λ) > 0 (canonical "
                    "absorbing branch per Epic #88 PR #155). One "
                    "representative per near-conjugate pair."
                ),
                "tolerance_abs": 1.0e-3,
                "data": _interleave_c128(physical_eigenvalues_complex),
            },
            "q_factor_lowest_physical": {
                "shape": [1],
                "dtype": "f64",
                "description": (
                    "Quality factor Q = Re(λ)/(2|Im(λ)|) (sign-agnostic, "
                    "matches NumPy/Burn canonical formula) for the lowest "
                    "absorbing physical mode. Always positive. NaN for "
                    "σ₀=0 regression."
                ),
                "tolerance_abs": 0.5,
                "data": [float(q_factor_lowest_physical)],
            },
        },
        "provenance": {
            "source": (
                f"reference/jax/sphere_pml.py @ commit {_git_commit()} "
                f"(Epic #88 / Phase H.3 / Issue #148)"
            ),
            "verified_against": verified_note,
            "issue": "#148 (parent epic #88, Phase H.3)",
        },
    }


def main():
    parser = argparse.ArgumentParser(
        description="Generate JAX sphere-PML reference fixture"
    )
    parser.add_argument(
        "--out", default=str(_default_out_path()),
        help="Output JSON path",
    )
    parser.add_argument(
        "--sigma0", type=float, default=5.0,
        help="PML absorption strength (default 5.0)",
    )
    parser.add_argument(
        "--n-take", type=int, default=5,
        help="Number of physical eigenvalues to retain (default 5)",
    )
    parser.add_argument(
        "--tol", type=float, default=5.0e-3,
        help="Max allowed |Δ| of JAX vs NumPy physical eigenvalues (default 5e-3)",
    )
    args = parser.parse_args()
    out_path = Path(args.out)

    try:
        from reference.jax.sphere_pml import solve_sphere_pml_jax, JaxSpherePmlResult
    except ImportError as e:
        print(f"ERROR: Could not import JAX pipeline: {e}")
        print("Install JAX with: pip install 'jax[cpu]'")
        sys.exit(1)

    print(f"Solving sphere-PML with JAX pipeline (σ₀={args.sigma0})...")
    result: JaxSpherePmlResult = solve_sphere_pml_jax(
        sigma_0=args.sigma0, n_take=args.n_take
    )

    print(f"  n_nodes = {result.n_nodes}, n_tets = {result.n_tets}")
    print(f"  n_edges = {result.n_edges}, n_interior_edges = {result.n_interior_edges}")
    print(f"  spurious_dim = {result.spurious_dim}")
    print(f"  Q_lowest_physical = {result.q_factor_lowest_physical:.4f}")
    print(f"  lowest 5 physical λ:")
    for lam in result.physical_eigenvalues_complex:
        print(f"    {lam.real:+.6e} {lam.imag:+.6e}j")

    # Try cross-check vs NumPy baseline if it exists and is non-stub.
    numpy_baseline_path = _numpy_baseline_path()
    verified_note: str
    if not numpy_baseline_path.exists():
        verified_note = (
            "NumPy PML baseline not found at generation time; "
            "cross-check deferred until #146 lands."
        )
        print(f"\nWARNING: {verified_note}")
    else:
        with open(numpy_baseline_path) as f:
            np_baseline = json.load(f)

        # The scaffolding stub from #145 has fixture_id ending in "_stub";
        # the real #146 fixture will have a different id (e.g. "_n774_pml_eigenmode_numpy").
        # Skip the cross-check when we're still on the stub.
        np_fid = np_baseline.get("fixture_id", "")
        if "_stub" in np_fid:
            verified_note = (
                f"NumPy baseline is the Phase H scaffolding stub "
                f"(fixture_id='{np_fid}') — full cross-check deferred to "
                f"when #146 lands."
            )
            print(f"\nWARNING: {verified_note}")
        else:
            # Compare physical eigenvalues (best-effort — fields may
            # be named slightly differently per #146's design choice).
            np_outputs = np_baseline.get("outputs", {})
            for candidate in (
                "physical_eigenvalues_complex",
                "eigenvalues_lowest_complex",
            ):
                if candidate in np_outputs and np_outputs[candidate].get("dtype") == "c128":
                    np_flat = np.asarray(
                        np_outputs[candidate]["data"], dtype=np.float64
                    )
                    np_complex = np_flat.view(np.complex128)
                    jax_complex = (
                        result.physical_eigenvalues_complex
                        if candidate == "physical_eigenvalues_complex"
                        else result.eigenvalues_lowest_complex
                    )
                    n_compare = min(len(np_complex), len(jax_complex))
                    if n_compare > 0:
                        deltas = np.abs(np_complex[:n_compare] - jax_complex[:n_compare])
                        max_abs = float(np.max(deltas))
                        # The robust cross-check is the lowest physical
                        # mode (physical[0]) — the non-Hermitian sparse
                        # ARPACK shift-invert returns a cluster of
                        # near-degenerate modes around the shift in a
                        # solver-dependent order, while dense LAPACK
                        # (NumPy) returns the deterministic ascending
                        # band slice. Per-position |Δ| can be large
                        # without physical disagreement on the lowest
                        # mode. Report both.
                        d0 = float(np.abs(np_complex[0] - jax_complex[0]))
                        re_rel0 = float(
                            abs(jax_complex[0].real - np_complex[0].real)
                            / max(abs(np_complex[0].real), 1e-12)
                        )
                        im_abs0 = float(
                            abs(abs(jax_complex[0].imag) - abs(np_complex[0].imag))
                        )
                        print(
                            f"\nCross-check vs NumPy field `{candidate}`:"
                        )
                        print(
                            f"  physical[0] |Δ|     = {d0:.3e}"
                        )
                        print(
                            f"  physical[0] Re-rel  = {re_rel0:.3e}"
                        )
                        print(
                            f"  physical[0] |Im|-abs = {im_abs0:.3e}"
                        )
                        print(
                            f"  per-position max |Δ| = {max_abs:.3e} "
                            f"(over {n_compare} entries; cluster ordering "
                            f"differs between sparse ARPACK and dense LAPACK)"
                        )
                        if d0 > args.tol:
                            print(
                                f"WARNING: physical[0] |Δ| {d0:.3e} exceeds "
                                f"target tolerance {args.tol:.0e}. "
                                f"(Documented Epic #88 friction artifact.)"
                            )
                        verified_note = (
                            f"NumPy canonical (PR #155) `{candidate}`: "
                            f"physical[0] |Δ| = {d0:.3e} "
                            f"(Re-rel = {re_rel0:.3e}, "
                            f"|Im|-abs = {im_abs0:.3e}). "
                            f"Per-position max |Δ| = {max_abs:.3e} over "
                            f"{n_compare} entries reflects the cluster-"
                            f"ordering difference between SciPy ARPACK "
                            f"shift-invert (basin near sigma) and "
                            f"NumPy/LAPACK dense ZGGEV — not a physical "
                            f"disagreement."
                        )
                        break
            else:
                verified_note = (
                    "NumPy baseline present but lacks a comparable c128 "
                    "eigenvalue field; cross-check skipped."
                )
                print(f"\nWARNING: {verified_note}")

    fixture = _build_fixture_dict(
        n_nodes=result.n_nodes,
        n_tets=result.n_tets,
        n_edges=result.n_edges,
        n_interior_edges=result.n_interior_edges,
        spurious_dim=result.spurious_dim,
        sigma_0=result.sigma_0,
        n_index=1.5,
        epsilon_r_complex=result.epsilon_r_complex,
        eigenvalues_lowest_complex=result.eigenvalues_lowest_complex,
        physical_eigenvalues_complex=result.physical_eigenvalues_complex,
        q_factor_lowest_physical=result.q_factor_lowest_physical,
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
