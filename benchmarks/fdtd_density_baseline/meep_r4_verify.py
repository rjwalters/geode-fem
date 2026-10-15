#!/usr/bin/env python3
"""Validate the #1088 R4 matched bent-patch record, the R1/R2/R4 trend and the R8 projection.

Standard library only; checks stay active under python -O. `--write` regenerates
`meep_runs/aws_r4/resolution_trend.json` from the committed records; the default
mode recomputes it and requires an exact match. Nothing here certifies spatial
convergence: the trend reports differences between successive grids and says
whether they shrink, and the R8 block is a planning projection from R4 actuals.
"""
import argparse
import cmath
import gzip
import hashlib
import json
import math
import re
import statistics
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
from meep_pilot_verify import check, digest, validate_spectrum  # noqa: E402

R4 = HERE / 'meep_runs/aws_r4'
RUNS = [('R1', HERE / 'meep_runs/initial_R1.json'),
        ('R2', HERE / 'meep_runs/aws_pilot/initial_R2.json'),
        ('R4', R4 / 'initial_R4.json')]
# Price table of scripts/aws/geode-worker.sh (PRICE_DATE 2026-10-12, us-east-1
# on-demand); not a billing receipt. r8g.16xlarge/24xlarge are not in that table;
# they are derived from its exactly linear r8g ladder (0.94256 USD/h per 16 vCPU,
# which reproduces the 4x/8x/48x entries) and must be re-checked before quoting.
R8G_USD_PER_16_VCPU = 0.94256
INSTANCES = {'r8g.8xlarge': (32, 256), 'r8g.16xlarge': (64, 512),
             'r8g.24xlarge': (96, 768), 'r8g.48xlarge': (192, 1536)}
PRICES = {name: R8G_USD_PER_16_VCPU * vcpus / 16 for name, (vcpus, _) in INSTANCES.items()}
GIB = 2**30


def final_spectrum(run):
    snap = run['snapshots'][-1]
    return ([complex(*g) for g in snap['s11']], [complex(*z) for z in snap['impedance_ohm']])


def summarize(name, run):
    freqs = run['frequency_ghz']
    gamma, z = final_spectrum(run)
    mags = [abs(g) for g in gamma]
    i = min(range(len(mags)), key=mags.__getitem__)
    k = min(range(len(freqs)), key=lambda j: abs(freqs[j] - 2.45))
    band = [j for j, f in enumerate(freqs) if 2.4 - 1e-9 <= f <= 2.4835 + 1e-9]
    return {
        'resolution_cells_per_mm': run['resolution_cells_per_mm'],
        'mpi_ranks': run['mpi_ranks'],
        'wall_seconds': run['wall_seconds'],
        'sum_rank_peak_rss_gib': run['sum_rank_peak_rss_bytes'] / GIB,
        'final_time': run['snapshots'][-1]['time'],
        'time_step': run['time_step'],
        'checks_run': len(run['snapshots']) - 1,
        'temporal_drift_sequence': [
            max(abs(complex(*a) - complex(*b)) for a, b in zip(p['s11'], q['s11']))
            for p, q in zip(run['snapshots'], run['snapshots'][1:])],
        'temporal_tolerance_met': run['temporal_tolerance_met'],
        'min_s11_db': 20 * math.log10(mags[i]),
        'min_s11_frequency_ghz': freqs[i],
        'min_is_interior': 0 < i < len(freqs) - 1,
        'worst_in_band_s11_db': max(20 * math.log10(mags[j]) for j in band),
        'impedance_2g45_ohm': [z[k].real, z[k].imag],
        'name': name,
    }


def max_diff(a, b):
    return max(abs(x - y) for x, y in zip(final_spectrum(a)[0], final_spectrum(b)[0]))


def log_step_rates(path):
    text = gzip.decompress(path.read_bytes()).decode()
    rates = [(int(s), float(r)) for s, r in
             re.findall(r'on time step (\d+) \(time=[^)]*\), ([0-9.e+-]+) s/step', text)]
    return rates


def cgroup_peak_bytes(path):
    text = gzip.decompress(path.read_bytes()).decode()
    peaks = [int(p) for p in re.findall(r'peak=(\d+)', text)]
    check(peaks, 'No cgroup memory samples')
    return max(peaks)


def geode_reference(runs):
    """GEODE bent-fixture dip (same YAML geometry) next to Meep at the same frequencies."""
    conv = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/convergence.toml').read_text())
    gate = conv['gate_g1']['bent']
    levels = conv['s11']['bent']
    lower, (lo, hi) = gate['lower_bound_dip_f_ghz'], gate['extrapolation_range_ghz']
    check(not gate['converged'] and lo > lower, 'GEODE gate record changed')
    dips = [lv['dip_f_ghz'] for lv in levels]
    window = (runs['R4']['frequency_ghz'][0], runs['R4']['frequency_ghz'][-1])
    meep = {}
    for name, run in runs.items():
        freqs = run['frequency_ghz']
        gamma, z = final_spectrum(run)
        at = {}
        for target in (2.36, 2.40):
            k = min(range(len(freqs)), key=lambda j: abs(freqs[j] - target))
            check(abs(freqs[k] - target) < 1e-9, 'Target not on the Meep frequency grid')
            at[f'{target:.2f}'] = {'s11_db': 20 * math.log10(abs(gamma[k])), 'z_re_ohm': z[k].real}
        meep[name] = at
    return {
        'source': 'benchmarks/patch_antenna_conformal/convergence.toml [s11.bent], [gate_g1.bent]',
        'geode_levels': [f"{lv['level']}_{lv['element_order']}" for lv in levels],
        'geode_dip_f_ghz': dips,
        'geode_dip_db': [lv['dip_db'] for lv in levels],
        'geode_dip_z_re_ohm': [lv['dip_z_re_ohm'] for lv in levels],
        'geode_dip_rises_with_refinement': all(a < b for a, b in zip(dips, dips[1:])),
        'geode_converged': gate['converged'],
        'geode_lower_bound_dip_f_ghz': lower,
        'geode_extrapolation_range_ghz': [lo, hi],
        'geode_lower_bound_inside_meep_window': window[0] < lower < window[1],
        'meep_at_geode_frequencies': meep,
        'meep_min_at_window_edge': {
            name: not summarize(name, run)['min_is_interior'] for name, run in runs.items()},
        'meep_min_frequency_ghz': {
            name: summarize(name, run)['min_s11_frequency_ghz'] for name, run in runs.items()},
        'note': ('Opposite refinement directions recorded, not attributed to one cause. '
                 'Meep exp(-i wt) vs GEODE exp(+i wt) does not affect |S11| or Re Z.'),
    }


def build_trend():
    runs = {name: json.loads(path.read_text()) for name, path in RUNS}
    r4 = runs['R4']
    summaries = [summarize(name, runs[name]) for name, _ in RUNS]
    d12, d24 = max_diff(runs['R1'], runs['R2']), max_diff(runs['R2'], runs['R4'])
    rates = log_step_rates(R4 / 'logs/initial_R4.log.gz')
    steady = [r for step, r in rates if step > 1]
    check(len(steady) >= 10, 'Too few logged step rates')
    env = json.loads((R4 / 'environment.json').read_text())
    peak = cgroup_peak_bytes(R4 / 'logs/cgroup_memory.log.gz')
    r4_steps = round(r4['snapshots'][-1]['time'] / r4['time_step'])
    median_rate = statistics.median(steady)
    # R8: cells x8 (grid doubles per axis), steps x2 (dt halves, same final time),
    # memory x8, per-step work x8 spread over the target's ranks.
    grid4 = env['logged_R4_grid_shape']
    cells4, cells8 = math.prod(grid4), math.prod(2 * n for n in grid4)
    mem8 = peak * cells8 / cells4
    rss8 = r4['sum_rank_peak_rss_bytes'] * cells8 / cells4
    steps8 = 2 * r4_steps
    rank_seconds_per_cell_step = median_rate * r4['mpi_ranks'] / cells4
    candidates = {}
    for itype, (vcpus, gib) in INSTANCES.items():
        fits = gib * GIB >= 1.15 * max(mem8, rss8)
        entry = {'vcpus': vcpus, 'nominal_memory_gib': gib, 'usd_per_hour': PRICES[itype],
                 'fits_memory_with_15pct_headroom': fits}
        if fits:
            ideal = rank_seconds_per_cell_step * cells8 / vcpus
            entry['ideal_s_per_step'] = ideal
            for eff in (1.0, 0.7):
                hours = (steps8 * ideal / eff) / 3600
                entry[f'stepping_hours_at_{int(eff*100)}pct_efficiency'] = hours
                entry[f'stepping_usd_at_{int(eff*100)}pct_efficiency'] = hours * PRICES[itype]
        candidates[itype] = entry
    return {
        'schema': 'geode-meep-resolution-trend/1',
        'note': ('Successive-grid differences and planning projection only. '
                 'No spatial convergence or cost-to-accuracy verdict.'),
        'runs': summaries,
        'max_complex_s11_change_R1_R2': d12,
        'max_complex_s11_change_R2_R4': d24,
        'successive_change_ratio_R1R2_over_R2R4': d12 / d24,
        'geode_comparison': geode_reference(runs),
        'r4_measured': {
            'logged_grid_shape': grid4,
            'cells': cells4,
            'steps_to_final_time': r4_steps,
            'median_logged_s_per_step': median_rate,
            'mean_wall_s_per_step': r4['wall_seconds'] / r4_steps,
            'cgroup_memory_peak_gib': peak / GIB,
            'sum_rank_peak_rss_gib': r4['sum_rank_peak_rss_bytes'] / GIB,
            'rank_seconds_per_cell_step': rank_seconds_per_cell_step,
        },
        'r8_projection': {
            'assumptions': [
                'Grid doubles on each axis (8x cells); dt halves, so 2x steps to the same final time.',
                'Same stopping outcome as R4 (final time unchanged); more checks would add time.',
                'Memory scales with cells from the R4 cgroup peak and from the R4 rank-RSS sum.',
                'Per-step cost from R4 median rank-seconds per cell-step; 70% efficiency is an assumed band, not measured.',
                'Prices from the scripts/aws/geode-worker.sh table (2026-10-12, us-east-1 on-demand); 16x/24x derived from its linear r8g ladder; not a bill.',
                'Excludes initialization, environment rebuild and idle time.'],
            'cells': cells8,
            'steps': steps8,
            'memory_from_cgroup_peak_gib': mem8 / GIB,
            'memory_from_rank_rss_sum_gib': rss8 / GIB,
            'instances': candidates,
        },
    }


def close(a, b, path='trend'):
    if isinstance(a, dict):
        check(isinstance(b, dict) and a.keys() == b.keys(), f'{path}: keys differ')
        for key in a:
            close(a[key], b[key], f'{path}.{key}')
    elif isinstance(a, list):
        check(isinstance(b, list) and len(a) == len(b), f'{path}: length differs')
        for i, (x, y) in enumerate(zip(a, b)):
            close(x, y, f'{path}[{i}]')
    elif isinstance(a, float) and not isinstance(b, bool):
        check(isinstance(b, (int, float)) and math.isclose(a, b, rel_tol=1e-12, abs_tol=1e-12),
              f'{path}: {a} != {b}')
    else:
        check(a == b, f'{path}: {a!r} != {b!r}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='regenerate resolution_trend.json')
    args = parser.parse_args()
    config = json.loads((ROOT / 'reference/meep/docker/matched_initial_2g4.json').read_text())
    r4 = json.loads((R4 / 'initial_R4.json').read_text())
    validate_spectrum(r4)
    check(r4['configuration'] == config and not r4['rc_oracle'], 'R4 configuration drift')
    check(r4['resolution_cells_per_mm'] == 4 and r4['mpi_ranks'] == 32, 'R4 resolution/ranks')
    check((r4['after_source'], r4['check_interval'], r4['max_checks'], r4['temporal_tolerance'])
          == (1200, 600, 4, 0.001), 'R4 stopping checks differ from R2')
    r2 = json.loads(RUNS[1][1].read_text())
    check((r2['after_source'], r2['check_interval'], r2['max_checks'], r2['temporal_tolerance'])
          == (1200, 600, 4, 0.001), 'R2 stopping checks changed')
    # The us-east-2 replay must reproduce the committed us-east-1 RC spectra exactly.
    for kind in ('volume', 'sheet'):
        new = json.loads((R4 / f'rc_{kind}_R4.json').read_text())
        old = json.loads((HERE / f'meep_runs/aws_pilot/rc_{kind}_R4.json').read_text())
        validate_spectrum(new)
        check(new['snapshots'] == old['snapshots'], f'RC {kind} R4 not reproduced')
        expected = [1j * 376.730313668 / (2 * math.pi * f / 299.792458 * 8) for f in new['frequency_ghz']]
        measured = [complex(*z) for z in new['snapshots'][-1]['impedance_ohm']]
        check(max(abs(a / b - 1) for a, b in zip(measured, expected)) < 5e-4, 'RC tolerance')
    env = json.loads((R4 / 'environment.json').read_text())
    for key, name in [('source_sha256', 'matched_antenna.py'),
                      ('dockerfile_sha256', 'Dockerfile.matched'),
                      ('conda_lock_sha256', 'matched-conda-linux-aarch64.txt')]:
        check(env[key] == digest(ROOT / 'reference/meep/docker' / name), f'{key} drift')
    check(env['region'] == 'us-east-2' and env['instance_type'] == 'r8g.8xlarge', 'Host record')
    check(env['worker_final_state'] == 'terminated' and env['retained_volumes'] == 0, 'Worker not torn down')
    table = dict(re.findall(r'^\s*(r8g\.\w+)\)\s+echo ([0-9.]+) ;;',
                            (ROOT / 'scripts/aws/geode-worker.sh').read_text(), re.M))
    for name, price in table.items():
        vcpus = int(name.split('.')[1].removesuffix('xlarge')) * 4
        check(math.isclose(R8G_USD_PER_16_VCPU * vcpus / 16, float(price), rel_tol=1e-12),
              f'Price ladder mismatch: {name}')
    check(math.isclose(env['cost_estimate_rate_usd_per_hour'], PRICES['r8g.8xlarge'], rel_tol=1e-12),
          'R4 rate differs from the worker price table')
    check(abs(env['launch_to_terminate_seconds'] / 3600 * env['cost_estimate_rate_usd_per_hour']
              - env['compute_cost_upper_elapsed_estimate_usd']) < 1e-4, 'R4 cost arithmetic')
    urls = lambda p: {l for l in p.read_text().splitlines() if l.startswith('https:')}  # noqa: E731
    lock = urls(ROOT / 'reference/meep/docker/matched-conda-linux-aarch64.txt')
    check(urls(R4 / 'conda-explicit.txt') == lock and len(lock) == env['explicit_package_urls_replayed'],
          'Installed environment differs from the package lock')
    plot = json.loads((R4 / 'spectra.provenance.json').read_text())
    for name, path in RUNS:
        check(plot[path.name] == digest(path), f'Plot input drift: {name}')
    check(plot['generator_sha256'] == digest(HERE / 'meep_r4_plot.py'), 'Plot generator drift')
    trend = build_trend()
    for summary in trend['runs']:
        check(cmath.isfinite(complex(*summary['impedance_2g45_ohm'])), 'Nonfinite impedance')
    out = R4 / 'resolution_trend.json'
    if args.write:
        out.write_text(json.dumps(trend, indent=2, allow_nan=False) + '\n')
        print(f'wrote {out}')
        return
    close(trend, json.loads(out.read_text()))
    ref = trend['geode_comparison']
    check(ref['geode_dip_rises_with_refinement'] and ref['geode_lower_bound_inside_meep_window'],
          'GEODE reference no longer inside the window / rising')
    check(all(ref['meep_min_at_window_edge'].values()), 'A Meep minimum moved inside the window')
    for line in (R4 / 'SHA256SUMS').read_text().splitlines():
        want, name = line.split(None, 1)
        check(digest(R4 / name) == want, f'SHA256SUMS mismatch: {name}')
    print('PASS: R4 record, us-east-2 environment replay, R1/R2/R4 trend and R8 projection arithmetic')


if __name__ == '__main__':
    main()
