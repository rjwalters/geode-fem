"""Generate the analytic Mie root catalogue baseline (issue #170).

Writes ``reference/fixtures/mie_roots/baseline.json`` — golden output
fixture in the canonical schema (``reference/SCHEMA.md`` v1) plus the
Mie-roots-specific output fields described in
``reference/fixtures/mie_roots/baseline.schema.md``.

The catalogue parameters mirror the Burn-side consumers exactly:

- ``n = 1.5`` (``N_INSIDE`` in ``crates/geode-core/examples/mie_sphere.rs``)
- ``R_s = 1.0``, ``R_b = 2.0`` (``R_SPHERE`` / ``R_BUFFER`` in
  ``crates/geode-core/src/mesh/sphere.rs``)
- ``l_max = 4``, ``n_max = 5`` (``L_MAX`` / ``N_MAX`` in
  ``examples/mie_sphere.rs``)

Roots are stored sorted by the canonical key ``(pol, l, n)`` — *not* by
ascending ``k`` — so the cross-check harness can join the two catalogues
on exact integer tags without depending on global ``k``-order ties
(near-degenerate roots from different channels, e.g. TE(1,1) at
k = 1.88943 vs TM(2,1) at k = 1.89074, would otherwise make the
ordering fragile under sub-tolerance perturbations).

Reproduction
============

    cd reference
    python3 -m pip install -r numpy/requirements.txt
    python3 numpy/gen_mie_roots_baseline.py
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent  # reference/numpy -> repo root
FIXTURE_DIR = REPO_ROOT / "reference" / "fixtures" / "mie_roots"
FIXTURE_PATH = FIXTURE_DIR / "baseline.json"

# Repo root on sys.path: `reference.*` resolves as PEP 420 namespace
# packages regardless of cwd (issue #187).
_REPO_ROOT_STR = str(Path(__file__).resolve().parents[2])
if _REPO_ROOT_STR not in sys.path:
    sys.path.insert(0, _REPO_ROOT_STR)
from reference.numpy.mie_roots import (  # noqa: E402
    K_MAX,
    K_MIN,
    N_INSIDE,
    N_SAMPLES,
    R_BUFFER,
    R_SPHERE,
    TE,
    TM,
    resonance_roots,
)

# Catalogue extent — mirror examples/mie_sphere.rs L_MAX / N_MAX.
L_MAX = 4
N_MAX = 5

POL_INDEX = {TE: 0, TM: 1}

# Absolute tolerance on root positions. The cross-check contract is
# <= 1e-10 *relative*; the largest catalogued root is < 20, so 2e-9
# absolute is the equivalent schema-v1 bound (the Rust harness applies
# the relative check itself — this field documents the intent).
K_TOLERANCE_ABS = 2e-9


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


def _int_field(description: str, values) -> dict:
    arr = np.asarray(values, dtype=np.int64)
    return {
        "shape": [int(arr.shape[0])],
        "dtype": "f64",
        "description": description,
        "tolerance_abs": 0.5,
        "data": arr.tolist(),
    }


def _scalar_int_field(description: str, value: int) -> dict:
    return _int_field(description, [int(value)])


def _scalar_f64_field(description: str, value: float, tol: float) -> dict:
    return {
        "shape": [1],
        "dtype": "f64",
        "description": description,
        "tolerance_abs": tol,
        "data": [float(value)],
    }


def main() -> None:
    print(
        f"Mie root catalogue: n = {N_INSIDE}, R_s = {R_SPHERE}, "
        f"R_b = {R_BUFFER}, l_max = {L_MAX}, n_max = {N_MAX}, "
        f"k window ({K_MIN}, {K_MAX}] @ {N_SAMPLES} samples"
    )

    # Per-channel root lists in canonical (pol, l) order.
    roots = []
    count_te: list[int] = []
    count_tm: list[int] = []
    for pol, counts in ((TE, count_te), (TM, count_tm)):
        for l in range(1, L_MAX + 1):
            channel = resonance_roots(pol, N_INSIDE, l, R_SPHERE, R_BUFFER, N_MAX)
            counts.append(len(channel))
            roots.extend(channel)
            print(
                f"  {pol} l={l}: {len(channel)} roots "
                f"{[round(r.k, 6) for r in channel]}"
            )

    # Canonical ordering: (pol_index, l, n). The per-channel lists are
    # already n-ascending; sort defensively anyway.
    roots.sort(key=lambda r: (POL_INDEX[r.pol], r.l, r.n))

    n_roots = len(roots)
    print(f"  total: {n_roots} roots")

    # Structural sanity: every (l, pol) window in (0.1, 20] holds at
    # least n_max roots for this geometry, so the catalogue should be
    # exactly full. Bail loudly if the root finder dropped one.
    expected = 2 * L_MAX * N_MAX
    if n_roots != expected:
        raise RuntimeError(
            f"catalogue has {n_roots} roots, expected {expected}; "
            "a bracket was rejected or a window starved — investigate "
            "before regenerating the fixture."
        )
    for r in roots:
        if not (np.isfinite(r.k) and K_MIN < r.k <= K_MAX):
            raise RuntimeError(f"root out of window: {r}")
        if r.multiplicity != 2 * r.l + 1:
            raise RuntimeError(f"bad multiplicity: {r}")

    FIXTURE_DIR.mkdir(parents=True, exist_ok=True)

    fixture = {
        "schema_version": "1",
        "fixture_id": "mie_roots/n15_pec_cavity_l4_n5",
        "description": (
            "Analytic Mie resonance root catalogue for the dielectric "
            "sphere (n = 1.5, R_s = 1.0) inside a PEC cavity (R_b = 2.0) "
            "— issue #170, parent epic #88, Phase J.1. SciPy reference "
            "(reference/numpy/mie_roots.py) for "
            "geode_core::mie::{resonance_roots, merged_roots, "
            "mie_roots_catalog}: TE/TM roots for l = 1..4, first 5 roots "
            "per channel, stored sorted by the canonical (pol, l, n) key "
            "(pol: 0 = TE, 1 = TM)."
        ),
        "units": "k in inverse length, same units as R_s / R_b (dimensionless geometry)",
        "inputs": {
            "n_inside": {
                "shape": [1],
                "dtype": "f64",
                "description": "Refractive index of the inner sphere.",
                "data": [N_INSIDE],
            },
            "r_sphere": {
                "shape": [1],
                "dtype": "f64",
                "description": "Inner sphere radius R_s (Burn: mesh::R_SPHERE).",
                "data": [R_SPHERE],
            },
            "r_buffer": {
                "shape": [1],
                "dtype": "f64",
                "description": "PEC wall radius R_b (Burn: mesh::R_BUFFER).",
                "data": [R_BUFFER],
            },
            "k_window": {
                "shape": [2],
                "dtype": "f64",
                "description": "Root search window (k_min, k_max].",
                "data": [K_MIN, K_MAX],
            },
        },
        "outputs": {
            "l_max": _scalar_int_field(
                "Maximum angular order in the catalogue (matches "
                "examples/mie_sphere.rs L_MAX).",
                L_MAX,
            ),
            "n_max": _scalar_int_field(
                "Roots per (l, polarisation) channel (matches "
                "examples/mie_sphere.rs N_MAX).",
                N_MAX,
            ),
            "n_roots": _scalar_int_field("Total catalogued roots.", n_roots),
            "n_inside": _scalar_f64_field(
                "Refractive index (replicated as output for harness-side "
                "constant pinning).",
                N_INSIDE,
                1e-15,
            ),
            "r_sphere": _scalar_f64_field(
                "R_s (replicated as output; must equal Burn mesh::R_SPHERE).",
                R_SPHERE,
                1e-15,
            ),
            "r_buffer": _scalar_f64_field(
                "R_b (replicated as output; must equal Burn mesh::R_BUFFER).",
                R_BUFFER,
                1e-15,
            ),
            "root_pol": _int_field(
                "Polarisation tag per root: 0 = TE, 1 = TM.",
                [POL_INDEX[r.pol] for r in roots],
            ),
            "root_l": _int_field(
                "Angular order l per root.", [r.l for r in roots]
            ),
            "root_n": _int_field(
                "Radial order n per root (1 = lowest in window).",
                [r.n for r in roots],
            ),
            "root_multiplicity": _int_field(
                "Degeneracy 2l + 1 per root.",
                [r.multiplicity for r in roots],
            ),
            "root_k": {
                "shape": [n_roots],
                "dtype": "f64",
                "description": (
                    "Resonance positions k, ordered by (pol, l, n) in "
                    "lockstep with root_pol / root_l / root_n. brentq-"
                    "refined to near machine precision; cross-check "
                    "contract is <= 1e-10 relative."
                ),
                "tolerance_abs": K_TOLERANCE_ABS,
                "data": [r.k for r in roots],
            },
            "root_count_te": _int_field(
                "TE root count per l = 1..l_max (after the n_max cap).",
                count_te,
            ),
            "root_count_tm": _int_field(
                "TM root count per l = 1..l_max (after the n_max cap).",
                count_tm,
            ),
        },
        "provenance": {
            "source": (
                "reference/numpy/mie_roots.py — SciPy "
                "(scipy.special.spherical_jn/spherical_yn + "
                "scipy.optimize.brentq) reimplementation of "
                "geode_core::mie with the identical dense-sampling "
                "bracket walk (30000 samples on (0.1, 20.0], 1e8 "
                "pole-rejection, 1e-5 consecutive dedup)."
            ),
            "verified_against": (
                "crates/geode-validation/tests/mie_roots_numpy_reference.rs "
                "— root-for-root (pol, l, n) join at <= 1e-10 relative."
            ),
            "issue": f"#170 (Epic #88, Phase J.1); generated at git commit {_git_commit()}",
        },
    }

    with open(FIXTURE_PATH, "w") as fh:
        json.dump(fixture, fh, indent=1)
        fh.write("\n")
    print(f"Wrote {FIXTURE_PATH}")


if __name__ == "__main__":
    main()
