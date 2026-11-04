#!/usr/bin/env python3
"""Check recorded Meep RC calibration and pilot provenance (stdlib only).

The RC impedance is derived independently from Maxwell's uniform-field ODE.
This does not certify antenna spatial convergence or GEODE/Meep equivalence.
"""
import cmath
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent


def main():
    # Historical records retain their original executable source, not a rewritten hash.
    source = HERE / 'meep_runs/source_v1.py'
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    errors = []
    for resolution in (2, 4, 8):
        path = HERE / f'meep_runs/rc_R{resolution}.json'
        run = json.loads(path.read_text())
        assert run['meep_version'] == '1.34.0'
        assert run['architecture'] == 'aarch64' and run['mpi_ranks'] == 2
        assert run['source_sha256'] == source_hash, f'{path}: stale source provenance'
        # epsilon=4, plate area=4 mm², gap=2 mm => C_natural=8 mm.
        expected = [1j * 376.730313668 / (2 * cmath.pi * (f / 299.792458) * 8)
                    for f in run['frequency_ghz']]
        measured = [complex(*z) for z in run['snapshots'][-1]['impedance_ohm']]
        error = max(abs(a / b - 1) for a, b in zip(measured, expected))
        assert error < 5e-4, (resolution, error)
        assert abs(error - run['max_rc_impedance_relative_error']) < 1e-12
        assert run['max_complex_s11_temporal_change'] < 1e-8
        errors.append(error)
        for z, gamma in zip(measured, run['snapshots'][-1]['s11']):
            assert abs(complex(*gamma) - (z-50)/(z+50)) < 1e-12
    assert errors[0] > errors[1] > errors[2], errors

    pilot = json.loads((HERE / 'meep_runs/initial_R1.json').read_text())
    config = json.loads((ROOT / 'reference/meep/docker/matched_initial_2g4.json').read_text())
    assert pilot['source_sha256'] == source_hash
    assert pilot['configuration'] == config
    snapshot = ROOT / 'reference/meep/docker/matched_initial_2g4.source.yaml'
    assert hashlib.sha256(snapshot.read_bytes()).hexdigest() == config['source']['sha256']
    assert pilot['mpi_ranks'] == 4 and pilot['architecture'] == 'aarch64'
    assert not pilot['spatially_converged']
    assert pilot['wall_seconds'] > 0 and pilot['sum_rank_peak_rss_bytes'] > 0
    snapshots = [[complex(*z) for z in s['s11']] for s in pilot['snapshots']]
    drift = max(abs(a-b) for a, b in zip(snapshots[-2],snapshots[-1]))
    assert abs(drift-pilot['max_complex_s11_temporal_change']) < 1e-12
    assert pilot['temporal_tolerance_met'] == (drift <= pilot['temporal_tolerance'])
    assert all(cmath.isfinite(z) for s in snapshots for z in s)
    # No tolerance asserted for this intentionally coarse antenna pilot.

    historical = json.loads((HERE / 'meep_runtime_scaling.json').read_text())
    assert historical['projection']['cells_at_R250'] == 64*60*42*250**3 == 2520000000000
    assert abs(historical['projection']['cell_ratio_R250_over_R14'] - (250/14)**3) < 1e-9
    print(f'PASS: independent RC oracle errors {errors}; pilot provenance/DFT; R250 arithmetic')


if __name__ == '__main__':
    main()
