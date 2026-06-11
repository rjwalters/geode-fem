"""Generate the analytic Mie efficiency-curve baseline (issue #195).

Writes ``reference/fixtures/mie_efficiencies/baseline.json`` — golden
output fixture in the canonical schema (``reference/SCHEMA.md`` v1)
for the ``Q_ext`` / ``Q_sca`` efficiency curve of the ``n = 1.5``
dielectric sphere, computed by the BHMIE logarithmic-derivative
algorithm (``reference/numpy/mie_efficiencies.py`` — independent of
the Rust direct-formula evaluation in
``geode_core::mie_scattering``).

Two grids:

- ``ka_benchmark`` — the 5-point sweep of the FEM driven-scattering
  benchmark (``examples/mie_driven_scattering.rs``), spanning the
  open-space TE_1,1 (ka ~ 1.26) and TM_1,1 (ka ~ 1.88) resonances.
- ``ka_curve`` — a dense 60-point grid on [0.1, 6.0] pinning the full
  curve shape (Rayleigh tail through the first interference maximum).

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_mie_efficiencies_baseline.py
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/numpy -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "mie_efficiencies"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.mie_efficiencies import (  # noqa: E402
    N_INSIDE,
    R_SPHERE,
    _self_check,
    mie_efficiencies,
)

# The FEM benchmark sweep — mirror examples/mie_driven_scattering.rs.
KA_BENCHMARK = [1.0, 1.5, 1.9, 2.4, 3.0]

# Dense curve grid.
KA_CURVE = np.round(np.linspace(0.1, 6.0, 60), 10).tolist()

# Cross-check contract: the Rust direct-formula evaluation and the
# BHMIE log-derivative algorithm share only the mathematics; both are
# accurate to ~1e-12 relative over this range. Q values are O(1), so
# 1e-10 absolute is the equivalent bound.
Q_TOLERANCE_ABS = 1e-10


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


def _f64_field(description: str, values, tol: float) -> dict:
    arr = np.asarray(values, dtype=np.float64)
    return {
        "shape": [int(arr.shape[0])],
        "dtype": "f64",
        "description": description,
        "tolerance_abs": tol,
        "data": arr.tolist(),
    }


def main() -> None:
    _self_check()

    q_ext_bench, q_sca_bench = zip(
        *(mie_efficiencies(N_INSIDE, x) for x in KA_BENCHMARK)
    )
    q_ext_curve, q_sca_curve = zip(*(mie_efficiencies(N_INSIDE, x) for x in KA_CURVE))

    for label, qe, qs in (
        ("benchmark", q_ext_bench, q_sca_bench),
        ("curve", q_ext_curve, q_sca_curve),
    ):
        worst = max(
            abs(e - s) / e for e, s in zip(qe, qs)
        )
        print(f"{label}: lossless Q_ext vs Q_sca max rel diff = {worst:.3e}")
        if worst > 1e-10:
            raise RuntimeError(
                "lossless-sphere identity Q_ext == Q_sca violated — "
                "BHMIE recurrence is broken, do not write the fixture"
            )

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "mie_efficiencies/n15_qext_qsca",
        "description": (
            "Analytic Mie scattering efficiencies Q_ext / Q_sca for the "
            "n = 1.5 dielectric sphere in vacuum — issue #195 (Epic #193 "
            "Phase 1), the analytic oracle of the driven Mie scattering "
            "benchmark. BHMIE logarithmic-derivative reference "
            "(reference/numpy/mie_efficiencies.py) for "
            "geode_core::mie_scattering::mie_efficiencies: the 5-point "
            "FEM benchmark sweep plus a dense 60-point curve on "
            "[0.1, 6.0]."
        ),
        "units": "ka dimensionless (k in inverse length, a = R_SPHERE); Q dimensionless",
        "inputs": {
            "n_inside": {
                "shape": [1],
                "dtype": "f64",
                "description": "Refractive index of the sphere (real, lossless).",
                "data": [N_INSIDE],
            },
            "r_sphere": {
                "shape": [1],
                "dtype": "f64",
                "description": "Sphere radius a (Burn: mesh::R_SPHERE).",
                "data": [R_SPHERE],
            },
            "ka_benchmark": {
                "shape": [len(KA_BENCHMARK)],
                "dtype": "f64",
                "description": (
                    "FEM driven-benchmark size parameters "
                    "(examples/mie_driven_scattering.rs KA_VALUES)."
                ),
                "data": KA_BENCHMARK,
            },
            "ka_curve": {
                "shape": [len(KA_CURVE)],
                "dtype": "f64",
                "description": "Dense efficiency-curve grid on [0.1, 6.0].",
                "data": KA_CURVE,
            },
        },
        "outputs": {
            "q_ext_benchmark": _f64_field(
                "Q_ext at ka_benchmark (BHMIE log-derivative).",
                q_ext_bench,
                Q_TOLERANCE_ABS,
            ),
            "q_sca_benchmark": _f64_field(
                "Q_sca at ka_benchmark (BHMIE log-derivative).",
                q_sca_bench,
                Q_TOLERANCE_ABS,
            ),
            "q_ext_curve": _f64_field(
                "Q_ext at ka_curve (BHMIE log-derivative).",
                q_ext_curve,
                Q_TOLERANCE_ABS,
            ),
            "q_sca_curve": _f64_field(
                "Q_sca at ka_curve (BHMIE log-derivative).",
                q_sca_curve,
                Q_TOLERANCE_ABS,
            ),
        },
        "provenance": {
            "source": (
                "reference/numpy/mie_efficiencies.py — BHMIE "
                "logarithmic-derivative algorithm (downward D_n "
                "recurrence + scipy.special.spherical_jn/spherical_yn "
                "Riccati ladders), independent of the Rust direct "
                "psi(mx)-formula evaluation."
            ),
            "verified_against": (
                "crates/geode-validation/tests/"
                "mie_efficiencies_numpy_reference.rs — pointwise Q_ext/"
                "Q_sca join at <= 1e-10 absolute on both grids."
            ),
            "issue": f"#195 (Epic #193, Phase 1); generated at git commit {_git_commit()}",
        },
    }

    FIXTURE_PATH.write_text(json.dumps(fixture, indent=1) + "\n")
    print(f"wrote {FIXTURE_PATH.relative_to(REPO_ROOT)}")


if __name__ == "__main__":
    main()
