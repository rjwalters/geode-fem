#!/usr/bin/env python3
"""Screen the committed slotless motor fixture for cheap harmonic structure.

Run from any directory with Python >=3.11:
    python3 benchmarks/motor_optimization_screen/screen_slotless.py

This reanalyses stored FEM results. It neither runs FEM nor measures optimizer
speed. Training uses even angles; odd angles are held out. The duplicated
period endpoint is excluded. Output goes to stdout, leaving fixtures untouched.
"""

import hashlib
import json
import math
from pathlib import Path
import tomllib


def fit_harmonics(angles, values, order):
    """DFT coefficients on a complete, uniformly spaced periodic grid."""
    n = len(angles)
    if n <= 2 * order:
        raise ValueError("Insufficient samples to identify the requested harmonics")
    for i, angle in enumerate(angles):
        expected = angles[0] + math.tau * i / n
        if abs(angle - expected) > 1e-7:
            raise ValueError("DFT fit requires a uniform full-period grid")
    coefficients = [sum(values) / n]
    for k in range(1, order + 1):
        coefficients.extend([
            2 * sum(y * math.cos(k * a) for a, y in zip(angles, values)) / n,
            2 * sum(y * math.sin(k * a) for a, y in zip(angles, values)) / n,
        ])
    return coefficients


def predict(angle, coefficients):
    value = coefficients[0]
    for k in range(1, (len(coefficients) + 1) // 2):
        value += coefficients[2 * k - 1] * math.cos(k * angle)
        value += coefficients[2 * k] * math.sin(k * angle)
    return value


def main():
    root = Path(__file__).resolve().parents[2]
    fixture = root / "benchmarks/motor/results.toml"
    raw = fixture.read_bytes()
    # Pin the provenance of stored data rather than stamping the current HEAD;
    # moving this screening script to a new commit must not change its result.
    fixture_sha = "f3e457da6207913582904e5a3e5c5eb9ab05fefe877532c994ebbb0ee98ee0d4"
    if hashlib.sha256(raw).hexdigest() != fixture_sha:
        raise ValueError("Motor fixture changed; review and update the source pin")
    data = tomllib.loads(raw.decode())
    rows = [data[key] for key in sorted(
        (key for key in data if key.startswith("point_")),
        key=lambda key: int(key.split("_")[1]),
    )]
    p = data["meta"]["pole_pairs"]
    if not math.isclose(p * (rows[-1]["theta_r"] - rows[0]["theta_r"]),
                        math.tau, abs_tol=1e-7):
        raise ValueError("Expected one full electrical period including endpoint")
    if not math.isclose(rows[0]["T_arkkio"], rows[-1]["T_arkkio"], abs_tol=1e-7):
        raise ValueError("Repeated endpoint does not match")
    rows = rows[:-1]
    if len(rows) % 2:
        raise ValueError("Even/odd split requires an even number of unique angles")
    train, test = rows[::2], rows[1::2]
    amplitude = max(abs(row["T_analytic"]) for row in rows)
    analyses = {}
    for order in (1, 2):
        coefficients = fit_harmonics(
            [p * row["theta_r"] for row in train],
            [row["T_arkkio"] for row in train], order,
        )
        errors = [predict(p * row["theta_r"], coefficients) - row["T_arkkio"]
                  for row in test]
        analyses[f"through_harmonic_{order}"] = {
            "coefficient_order": "constant, cos(phi), sin(phi), cos(2phi), sin(2phi), ...",
            "coefficients_in_fixture_torque_units": coefficients,
            "heldout_rmse_over_analytic_amplitude":
                math.sqrt(sum(e * e for e in errors) / len(errors)) / amplitude,
            "heldout_max_abs_error_over_analytic_amplitude":
                max(abs(e) for e in errors) / amplitude,
        }
    fem_l2 = math.sqrt(
        sum((r["T_arkkio"] - r["T_analytic"]) ** 2 for r in rows)
        / sum(r["T_analytic"] ** 2 for r in rows)
    )
    result = {
        "schema_version": 1,
        "kind": "stored_fixture_screen_not_new_fem_or_optimization",
        "fixture_source_commit": "9b3a9a4d02457a6be807275422397428f900eb8f",
        "fixture": str(fixture.relative_to(root)),
        "fixture_sha256": hashlib.sha256(raw).hexdigest(),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "unique_angles": len(rows),
        "training_angles": len(train),
        "heldout_angles": len(test),
        "fem_l2_relative_to_closed_form": fem_l2,
        "harmonic_fits": analyses,
        "interpretation": [
            "At fixed geometry, angle-independent linear K and sinusoidal magnetization give u(phi)=u_J+cos(phi)*u_c+sin(phi)*u_s.",
            "Quadratic Arkkio torque therefore contains only harmonics 0, 1 and 2 in phi=p*theta_r, even with discrete symmetry error.",
            "One matrix factorization and three RHS solves suffice to construct that field basis; this is a derivation, not a measured implementation speedup.",
            "The fit checks held-out angles of one geometry, not unseen designs or shape gradients.",
            "Slotless locked-rotor torque-versus-angle is not synchronous operating torque ripple: the current phase must track rotor angle at fixed load angle for that measurement.",
            "The harmonic reduction need not hold when material response is nonlinear or physical rotor motion changes the operator.",
            "Retain this fixture as a correctness control, not evidence that exact gradients accelerate realistic motor optimization.",
        ],
    }
    print(json.dumps(result, indent=2, allow_nan=False))


if __name__ == "__main__":
    main()
