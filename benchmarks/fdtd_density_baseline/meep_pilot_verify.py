#!/usr/bin/env python3
"""Independently validate AWS RC calibration, probe records and antenna spectra.

Checks remain active under python -O. No spatial-convergence verdict is inferred.
"""
import argparse
import cmath
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def check(condition, message):
    if not condition:
        raise ValueError(message)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_spectrum(run):
    check(run['meep_version'] == '1.34.0', 'Unexpected Meep release')
    check(run['source_sha256'] == digest(ROOT / 'reference/meep/docker/matched_antenna.py'),
          'Source provenance mismatch')
    check(run['architecture'] == 'aarch64', 'Expected native aarch64')
    check(run['mpi_ranks'] > 0 and run['wall_seconds'] > 0 and
          run['sum_rank_peak_rss_bytes'] > 0, 'Missing measured resource data')
    check(not run['spatially_converged'], 'Spatial accuracy has not been established')
    count = len(run['frequency_ghz'])
    check(count == 51 and len(run['snapshots']) >= 2, 'Incomplete spectrum or stopping check')
    for snapshot in run['snapshots']:
        check(len(snapshot['s11']) == len(snapshot['impedance_ohm']) == count,
              'Truncated spectrum')
        for z, gamma in zip(snapshot['impedance_ohm'], snapshot['s11']):
            z, gamma = complex(*z), complex(*gamma)
            check(cmath.isfinite(z) and cmath.isfinite(gamma), 'Nonfinite spectrum')
            check(abs(gamma - (z - 50)/(z + 50)) < 1e-12, 'Incorrect impedance-to-S11 conversion')
    previous, final = run['snapshots'][-2:]
    drift = max(abs(complex(*a) - complex(*b)) for a, b in zip(previous['s11'], final['s11']))
    check(abs(drift - run['max_complex_s11_temporal_change']) < 1e-12, 'Incorrect temporal drift')
    check(run['temporal_tolerance_met'] == (drift <= run['temporal_tolerance']),
          'Incorrect temporal stopping flag')
    return final


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--results', type=Path,
        default=Path(__file__).resolve().parent / 'meep_runs/aws_pilot')
    args = parser.parse_args()
    for resolution in (2, 4, 8):
        impedances = []
        for kind, current in [('volume', 4), ('sheet', 2)]:
            run = json.loads((args.results / f'rc_{kind}_R{resolution}.json').read_text())
            final = validate_spectrum(run)
            check(run['rc_oracle'] and run['rc_sheet'] == (kind == 'sheet'), 'Wrong oracle type')
            check(run['resolution_cells_per_mm'] == resolution and run['mpi_ranks'] == 2,
                  'Wrong calibration resolution/ranks')
            check(run['source_spatial_current'] == [current, 0], 'Wrong spatial source normalization')
            expected = [1j * 376.730313668 / (2 * math.pi * f/299.792458 * 8)
                        for f in run['frequency_ghz']]
            measured = [complex(*z) for z in final['impedance_ohm']]
            error = max(abs(a/b - 1) for a, b in zip(measured, expected))
            check(error < 5e-4, 'RC calibration exceeds tolerance')
            check(abs(error - run['max_rc_impedance_relative_error']) < 1e-12, 'Wrong RC error')
            impedances.append(measured)
        check(max(abs(a/b - 1) for a, b in zip(*impedances)) < 1e-10,
              'Sheet and volume calibration disagree')
    config = json.loads((ROOT / 'reference/meep/docker/matched_initial_2g4.json').read_text())
    for ranks, steps in [(8, 100), (16, 100), (32, 100), (32, 1000)]:
        suffix = '_1000' if steps == 1000 else ''
        run = json.loads((args.results / f'probe_R2_np{ranks}{suffix}.json').read_text())
        check(run['configuration'] == config and run['mpi_ranks'] == ranks, 'Probe configuration drift')
        check(run['meep_version'] == '1.34.0' and run['architecture'] == 'aarch64',
              'Probe environment mismatch')
        check(run['source_sha256'] == digest(ROOT / 'reference/meep/docker/matched_probe.py'),
              'Probe source provenance mismatch')
        check(run['antenna_source_sha256'] == digest(ROOT / 'reference/meep/docker/matched_antenna.py'),
              'Probe antenna provenance mismatch')
        check(run['requested_steps'] == run['actual_steps'] == steps, 'Probe step count mismatch')
        check(run['resolution_cells_per_mm'] == 2 and run['estimated_cells'] == 37103616,
              'Probe resolution/domain mismatch')
        check(all(math.isfinite(run[key]) and run[key] > 0 for key in
                  ['initialization_seconds', 'stepping_seconds', 'sum_rank_peak_rss_bytes']),
              'Invalid probe measurements')
    check((args.results / 'initial_R2.json').is_file(), 'Missing required R2 antenna record')
    for path in sorted(args.results.glob('initial_R*.json')):
        run = json.loads(path.read_text())
        validate_spectrum(run)
        check(run['configuration'] == config and not run['rc_oracle'], 'Antenna configuration drift')
    env = json.loads((args.results / 'environment.json').read_text())
    for key, name in [('source_sha256', 'matched_antenna.py'),
                      ('dockerfile_sha256', 'Dockerfile.matched'),
                      ('conda_lock_sha256', 'matched-conda-linux-aarch64.txt')]:
        check(env[key] == digest(ROOT / 'reference/meep/docker' / name), 'Environment provenance drift')
    def package_urls(path):
        return {line for line in path.read_text().splitlines() if line.startswith('https:')}
    expected_packages = package_urls(ROOT / 'reference/meep/docker/matched-conda-linux-aarch64.txt')
    check(package_urls(args.results / 'conda-explicit.txt') == expected_packages,
          'Installed environment differs from the package lock')
    check(len(expected_packages) == env['explicit_package_urls_replayed'], 'Package count mismatch')
    plot = json.loads((args.results / 'spectra.provenance.json').read_text())
    check(plot['initial_R1.json'] == digest(Path(__file__).resolve().parent / 'meep_runs/initial_R1.json'),
          'Plot historical input drift')
    check(plot['initial_R2.json'] == digest(args.results / 'initial_R2.json'), 'Plot R2 input drift')
    check(plot['generator_sha256'] == digest(Path(__file__).with_name('meep_pilot_plot.py')),
          'Plot generator drift')
    print('PASS: independent RC volume/sheet calibration; full-domain probe provenance; recorded antenna spectra')


if __name__ == '__main__':
    main()
