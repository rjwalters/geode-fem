#!/usr/bin/env python3
"""Validate the #1088 Meep/GEODE disagreement diagnosis records.

Standard library only; checks stay active under python -O. `--write` regenerates
`meep_runs/diag/diagnosis.json` from the committed records; the default mode
recomputes it and requires an exact match.

What is checked:
* environment: the us-east-2 image replays the 179-URL package lock, and the R2
  RC oracles reproduce the committed us-east-1 spectra exactly;
* every diagnostic run: provenance (matched_antenna.py unchanged, diag_antenna.py
  one of the archived versions), finite spectra, S11 = (Z-50)/(Z+50), recomputed
  temporal drift and stopping flag;
* the grid dumps: per-component metal/FR-4/air at Meep's own Yee samples, the
  discrete gap and port, and the E/H gap model below;
* the comparison with GEODE's flat and bent ladders.

E/H gap model. Meep zeroes each E component whose own sample lies in mp.metal.
Under a patch, h_E is the height over which Ez samples are free and h_H the
separation of the tangential-E (Ex, Ey) metal planes. A layer with Ez zeroed but
tangential E free stores magnetic, not electric, energy, so a TEM-like cavity
mode along y scales as sqrt(<1/h_H> / <1/h_E>). The averages run over the x columns
of the y = 0 slice on the half patch, x in [0, 19] mm, by symmetry. Nothing is
fitted, but the model is anchored: every predicted dip is the measured flat_R2 dip
times the grid factor. Its precision is about 1-2 % (25-45 MHz). It explains the
~250 MHz harness artifact, but it cannot resolve GEODE-sized (~1 MHz) bend shifts;
the corrected bent board's agreement with GEODE is a measurement, not a prediction.
Each test has its own tolerance, bracketing the error DIAGNOSIS.md states.
"""
import argparse
import cmath
import json
import math
import sys
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE))
from meep_pilot_verify import check, digest  # noqa: E402

DIAG = HERE / 'meep_runs/diag'
PILOT = HERE / 'meep_runs/aws_pilot'
CONFIG = ROOT / 'reference/meep/docker/matched_initial_2g4.json'
ETA0 = 376.730313668

# name -> (geometry, resolution, shell mode, bend radius or None)
RUNS = {
    'flat_R2': ('flat', 2, 'outward', None),
    'bent_R2': ('bent', 2, 'outward', 100.0),
    'ctrl_R2': ('bent', 2, 'outward', 10000.0),
    'inset_flat_R2': ('flat', 2, 'inset', None),
    'cells_bent_R2': ('bent', 2, 'cells', 100.0),
    'map_inset_flat_R2': ('flat', 2, 'inset', None),
    'map_flat_R2': ('flat', 2, 'outward', None),
}
GRIDS = {
    'flat_R2': ('flat', 2, 'outward', None),
    'bent_R2': ('bent', 2, 'outward', 100.0),
    'flat_R4': ('flat', 4, 'outward', None),
    'bent_R4': ('bent', 4, 'outward', 100.0),
    'ctrl_R2': ('bent', 2, 'outward', 10000.0),
    'inset_flat_R2': ('flat', 2, 'inset', None),
    'cells_bent_R2': ('bent', 2, 'cells', 100.0),
    'face_flat_R2p5': ('flat', 2.5, 'face', None),
    'face_bent_R2p5': ('bent', 2.5, 'face', 100.0),
}
# Model tests: run -> grid of the same geometry; the reference is flat_R2.
MODEL_TESTS = {'bent_R2': 'bent_R2', 'ctrl_R2': 'ctrl_R2', 'cells_bent_R2': 'cells_bent_R2',
               'inset_flat_R2': 'inset_flat_R2'}
# Per-test bounds on |measured/predicted - 1|, bracketing the errors DIAGNOSIS.md
# states (bent +0.05 %, ctrl +0.01 %, cells +1.0 %, inset +1.8 %).
MODEL_TOLERANCE = {'bent_R2': 0.002, 'ctrl_R2': 0.002, 'cells_bent_R2': 0.015,
                   'inset_flat_R2': 0.02}


def load(path):
    return json.loads(path.read_text())


def archived_sources():
    return {digest(p): p.name for p in sorted((DIAG / 'sources').glob('diag_antenna.*.py'))}


def spec_of(run):
    """(geometry, resolution, shell mode, bend radius); early records predate the keys."""
    geometry = run['geometry']
    bend = run.get('bend_radius_mm', 100.0 if geometry == 'bent' else None)
    return (geometry, run['resolution_cells_per_mm'], run.get('shell_mode', 'outward'), bend)


def check_provenance(run, sources):
    check(run['meep_version'] == '1.34.0' and run['architecture'] == 'aarch64', 'Environment')
    check(run['configuration'] == load(CONFIG), 'Configuration drift')
    check(run['source_sha256']['matched_antenna.py']
          == digest(ROOT / 'reference/meep/docker/matched_antenna.py'), 'Harness changed')
    check(run['source_sha256']['diag_antenna.py'] in sources, 'diag_antenna.py version not archived')
    check(run['mpi_ranks'] == 32 and run['wall_seconds'] > 0, 'Ranks / wall time')


def check_spectrum(run):
    freqs = run['frequency_ghz']
    check(len(run['snapshots']) >= 2, 'No stopping check')
    for snap in run['snapshots']:
        check(len(snap['s11']) == len(snap['impedance_ohm']) == len(freqs), 'Truncated spectrum')
        for z, g in zip(snap['impedance_ohm'], snap['s11']):
            z, g = complex(*z), complex(*g)
            check(cmath.isfinite(z) and cmath.isfinite(g), 'Nonfinite spectrum')
            check(abs(g - (z - 50) / (z + 50)) < 1e-12, 'Impedance-to-S11 conversion')
    prev, last = run['snapshots'][-2:]
    drift = max(abs(complex(*a) - complex(*b)) for a, b in zip(prev['s11'], last['s11']))
    check(abs(drift - run['max_complex_s11_temporal_change']) < 1e-12, 'Temporal drift')
    check(run['temporal_tolerance_met'] == (drift <= run['temporal_tolerance']), 'Stopping flag')
    check(not run['spatially_converged'], 'No spatial convergence is claimed')


def parabola_vertex(xs, ys):
    (x0, x1, x2), (y0, y1, y2) = xs, ys
    den = (x0 - x1) * (x0 - x2) * (x1 - x2)
    a = (x2 * (y1 - y0) + x1 * (y0 - y2) + x0 * (y2 - y1)) / den
    b = (x2 * x2 * (y0 - y1) + x1 * x1 * (y2 - y0) + x0 * x0 * (y1 - y2)) / den
    c = (x1 * x2 * (x1 - x2) * y0 + x2 * x0 * (x2 - x0) * y1 + x0 * x1 * (x0 - x1) * y2) / den
    xv = -b / (2 * a)
    return xv, a * xv * xv + b * xv + c


def summarize(run):
    freqs = run['frequency_ghz']
    snap = run['snapshots'][-1]
    gamma = [complex(*g) for g in snap['s11']]
    z = [complex(*v) for v in snap['impedance_ohm']]
    volts = [complex(*v) for v in run['voltage_dft']]
    inj = [complex(*v) for v in run['injected_current_dft']]
    db = [20 * math.log10(abs(g)) for g in gamma]
    i = min(range(len(db)), key=db.__getitem__)
    interior = 0 < i < len(db) - 1
    dip_f, dip_db = (parabola_vertex(freqs[i - 1:i + 2], db[i - 1:i + 2]) if interior
                     else (freqs[i], db[i]))
    k = max(range(len(z)), key=lambda j: z[j].real)
    # Norton total impedance eta0 V / I_injected: the antenna in parallel with
    # Meep's own discretized resistor, independent of the 50-ohm bookkeeping.
    zt = [abs(ETA0 * v / c) for v, c in zip(volts, inj)]
    m = max(range(len(zt)), key=zt.__getitem__)
    at = {}
    for target in (2.36, 2.40, 2.45):
        j = min(range(len(freqs)), key=lambda q: abs(freqs[q] - target))
        check(abs(freqs[j] - target) < 1e-9, 'Target frequency not sampled')
        at[f'{target:.2f}'] = {'s11_db': db[j], 'z_ohm': [z[j].real, z[j].imag]}
    minima = []
    for j in range(1, len(db) - 1):
        if db[j] < db[j - 1] and db[j] <= db[j + 1] and db[j] < -6:
            fv, dv = parabola_vertex(freqs[j - 1:j + 2], db[j - 1:j + 2])
            minima.append({'f_ghz': fv, 's11_db': dv})
    return {
        'final_time': snap['time'], 'checks_run': len(run['snapshots']) - 1,
        'local_minima_below_6db': minima,
        'max_complex_s11_temporal_change': run['max_complex_s11_temporal_change'],
        'temporal_tolerance_met': run['temporal_tolerance_met'],
        'wall_seconds': run['wall_seconds'],
        'window_ghz': [freqs[0], freqs[-1]], 'df_ghz': round(freqs[1] - freqs[0], 9),
        'min_s11_sample_ghz': freqs[i], 'min_s11_sample_db': db[i],
        'min_is_interior': interior,
        'dip_parabolic_ghz': dip_f, 'dip_parabolic_db': dip_db,
        'max_re_z_ohm': z[k].real, 'max_re_z_ghz': freqs[k],
        'max_norton_abs_z_ohm': zt[m], 'max_norton_abs_z_ghz': freqs[m],
        'at': at,
    }


def map_points(run, k, half_w=19.0, half_l=14.5, port=(0.0, 7.5), keep_out=3.0):
    """(x, y, Ez) on the patch area of map k, excluding a disc around the port."""
    m = run['ez_map_z0']
    xs, ys, mp_ = m['x_mm'], m['y_mm'], m['maps'][k]
    out = {}
    for i, x in enumerate(xs):
        for j, y in enumerate(ys):
            if abs(x) <= half_w and abs(y) <= half_l and math.hypot(x - port[0], y - port[1]) > keep_out:
                out[(round(x, 6), round(y, 6))] = complex(mp_['re'][i][j], mp_['im'][i][j])
    return out


def mode_shape(run, reference):
    """Correlation with the reference TM010 map and y-oddness, per mapped frequency."""
    ref = map_points(reference, 0)
    out = []
    for k, entry in enumerate(run['ez_map_z0']['maps']):
        pts = map_points(run, k)
        common = [p for p in pts if p in ref]
        check(len(common) > 100, 'Map grids do not overlap')
        dot = sum(pts[p] * ref[p].conjugate() for p in common)
        corr = abs(dot) / math.sqrt(sum(abs(pts[p]) ** 2 for p in common)
                                    * sum(abs(ref[p]) ** 2 for p in common))
        pairs = [(p, (p[0], -p[1])) for p in pts if (p[0], -p[1]) in pts and p[1] > 0]
        odd = math.sqrt(sum(abs(pts[a] + pts[b]) ** 2 for a, b in pairs)
                        / sum(abs(pts[a]) ** 2 + abs(pts[b]) ** 2 for a, b in pairs))
        out.append({'f_ghz': entry['f_ghz'], 'correlation_with_flat_tm010': corr,
                    'y_even_part_fraction': odd})
    return out


def column_gaps(grid, comp, half_width):
    """Free samples between the patch metal and the ground metal, per x column."""
    s = grid[f'slice_y0_{comp}']
    dx = s['z'][1] - s['z'][0]
    out = []
    for j in range(len(s['rows'][0])):
        x = s['x0'] + j * dx
        if not (-1e-9 <= x <= half_width + 1e-9):
            continue
        col = [row[j] for row in s['rows']]
        metal = [i for i, c in enumerate(col) if c == 'M']
        runs = []
        for i in metal:
            if runs and i == runs[-1][-1] + 1:
                runs[-1].append(i)
            else:
                runs.append([i])
        check(len(runs) == 2, f'{comp} column at x={x}: expected patch and ground metal runs')
        between = col[runs[0][-1] + 1:runs[1][0]]
        check(set(between) <= {'F'}, f'{comp} column at x={x}: non-FR-4 sample in the gap')
        out.append((round(x, 9), len(between)))
    return out, dx


def gap_model(grid, half_width):
    ex, dx = column_gaps(grid, 'Ex', half_width)
    ez, _ = column_gaps(grid, 'Ez', half_width)
    h_h = [(n + 1) * dx for _, n in ex]  # tangential-E metal plane separation
    h_e = [n * dx for _, n in ez]        # height of free Ez samples
    inv_h = sum(1 / h for h in h_h) / len(h_h)
    inv_e = sum(1 / h for h in h_e) / len(h_e)
    hist = lambda hs: {f'{h:.3f}': hs.count(h) for h in sorted(set(hs))}  # noqa: E731
    return {'h_H_mm_histogram': hist(h_h), 'h_E_mm_histogram': hist(h_e),
            'mean_inverse_h_H': inv_h, 'mean_inverse_h_E': inv_e,
            'predicted_frequency_factor': math.sqrt(inv_h / inv_e)}


def connected_conductors(grid):
    """Connected components of metal Yee edges in the y = 0 slice (patch, ground)."""
    ex, ez = grid['slice_y0_Ex'], grid['slice_y0_Ez']
    dx = ex['z'][1] - ex['z'][0]
    parent = {}

    def find(a):
        while parent.setdefault(a, a) != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    key = lambda x, z: (round(2 * x / dx), round(2 * z / dx))  # noqa: E731
    for z, row in zip(ex['z'], ex['rows']):
        for j, c in enumerate(row):
            if c == 'M':
                x = ex['x0'] + j * dx
                parent[find(key(x - dx / 2, z))] = find(key(x + dx / 2, z))
    for z, row in zip(ez['z'], ez['rows']):
        for j, c in enumerate(row):
            if c == 'M':
                x = ez['x0'] + j * dx
                parent[find(key(x, z - dx / 2))] = find(key(x, z + dx / 2))
    roots = {find(n) for n in list(parent)}
    return len(roots)


def grid_summary(name, grid, spec):
    config = load(CONFIG)
    p = config['patch']
    geometry, res, mode, bend = spec
    check(grid['grid']['offset_search'] and 'error' not in grid['grid'], f'{name}: offsets')
    check(spec_of(grid) == spec, f'{name}: spec')
    g = grid['grid']
    for comp, offs in g['sample_offsets_mm'].items():
        check(all(o is not None for o in offs), f'{name}: {comp} offset undetermined')
    port = g['port_ez_samples']
    dx = 1 / res
    width = len(port['y_in_resistor']) * dx
    free_ez = [z for z, c in port['z_material'] if c == 'fr4' and z in port['z_in_resistor']]
    length = len(free_ez) * dx
    nominal_sigma = p['h'] / (50 * p['probe_w'] * dx)  # in units of 1/eta0, per harness
    # Discrete resistor: free Ez cells in series (length) and in parallel (width).
    discrete_r = length / (nominal_sigma * width * dx) if free_ez else None
    tangential_metal = [z for z, c in g['port_line']['Ex']['samples'] if c == 'metal']
    upper = min(z for z in tangential_metal if z > 0)
    lower = max(z for z in tangential_metal if z < 0)
    out = {
        'grid_cells': g['grid_cells'], 'sample_offsets_mm': g['sample_offsets_mm'],
        'port_tangential_metal_planes_mm': [lower, upper],
        'port_tangential_gap_mm': upper - lower,
        'port_free_ez_height_mm': length,
        'port_resistor_width_mm': width,
        'port_resistor_discrete_ohm': discrete_r,
        'source_share_on_metal_samples': g['source_share_on_metal_samples'],
        'patch_ey_metal_y_mm': [g['patch_length_Ey']['first_metal_y'], g['patch_length_Ey']['last_metal_y']],
        'slice_conductor_components': connected_conductors(g),
    }
    out.update(gap_model(g, p['patch_w'] / 2))
    return out


def geode_reference():
    conv = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/convergence.toml').read_text())
    v6 = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/v6/v6_summary.toml').read_text())
    out = {'source': 'benchmarks/patch_antenna_conformal/convergence.toml [s11.flat], [s11.bent], '
                     '[detune]; v6/v6_summary.toml [x2]'}
    for geom in ('flat', 'bent'):
        out[geom] = {f"{lv['level']}_{lv['element_order']}": lv['dip_f_ghz'] for lv in conv['s11'][geom]}
        out[f'{geom}_lower_bound_dip_ghz'] = conv['gate_g1'][geom]['lower_bound_dip_f_ghz']
    out['detune_bent_minus_flat_mhz'] = {k: v['dip_shift_mhz'] for k, v in conv['detune'].items()}
    out['x2_optimized_design_p1_dip_shift_bent_minus_flat_mhz'] = v6['x2']['p1_dip_shift_bent_minus_flat_mhz']
    return out


def harness_equivalence(bent):
    """Diag bent R2 vs the committed harness R2 at the shared frequencies and times."""
    harness = load(PILOT / 'initial_R2.json')
    idx = bent['harness_window_indices']
    check(len(idx) == 51, 'Harness window not contained in the diag window')
    check([round(bent['frequency_ghz'][i], 6) for i in idx]
          == [round(f, 6) for f in harness['frequency_ghz']], 'Shared frequency grid')
    diffs = []
    for a, b in zip(bent['snapshots'], harness['snapshots']):
        check(a['time'] == b['time'], 'Snapshot times differ')
        diffs.append(max(abs(complex(*a['s11'][i]) - complex(*g)) for i, g in zip(idx, b['s11'])))
    return {'shared_snapshots': len(diffs), 'max_complex_s11_difference_per_snapshot': diffs}


def window_equivalence():
    """R1: harness vs diag (harness window) vs diag (wide window), same host and ranks."""
    harness = load(DIAG / 'r1eq_harness.json')
    committed = load(HERE / 'meep_runs/initial_R1.json')
    narrow, wide = load(DIAG / 'r1eq_narrow.json'), load(DIAG / 'r1eq_wide.json')
    check(harness['source_sha256'] == digest(ROOT / 'reference/meep/docker/matched_antenna.py'),
          'R1 harness provenance')
    for run in (narrow, wide):
        check(run['source_sha256']['diag_antenna.py'] in archived_sources(), 'R1 diag provenance')
        check(run['mpi_ranks'] == harness['mpi_ranks'] == 8, 'R1 ranks')

    def diff(a, b, idx=None):
        out = []
        for sa, sb in zip(a['snapshots'], b['snapshots']):
            check(sa['time'] == sb['time'], 'Snapshot times differ')
            ga = [sa['s11'][i] for i in idx] if idx else sa['s11']
            out.append(max(abs(complex(*x) - complex(*y)) for x, y in zip(ga, sb['s11'])))
        return out
    return {
        'harness_worker_8_ranks_vs_committed_R1_4_ranks': diff(harness, committed),
        'diag_harness_window_vs_harness': diff(narrow, harness),
        'diag_wide_window_vs_harness': diff(wide, harness, wide['harness_window_indices']),
    }


def build():
    sources = archived_sources()
    runs, grids, raw = {}, {}, {}
    for name, spec in RUNS.items():
        run = load(DIAG / f'{name}.json')
        check_provenance(run, sources)
        check_spectrum(run)
        check(spec_of(run) == spec, f'{name}: spec')
        runs[name], raw[name] = summarize(run), run
    for name, spec in GRIDS.items():
        grid = load(DIAG / f'grid_{name}.json')
        check_provenance(grid, sources)
        grids[name] = grid_summary(name, grid, spec)
    # Identify the TM010-like mode of the inset board from the field maps.
    shapes = {'map_inset_flat_R2': mode_shape(raw['map_inset_flat_R2'], raw['map_flat_R2']),
              'map_flat_R2': mode_shape(raw['map_flat_R2'], raw['map_flat_R2'])}
    tm010 = max(shapes['map_inset_flat_R2'], key=lambda e: e['correlation_with_flat_tm010'])['f_ghz']
    a, b = (runs[n]['local_minima_below_6db'] for n in ('map_inset_flat_R2', 'inset_flat_R2'))
    check(len(a) == len(b) and all(abs(x['f_ghz'] - y['f_ghz']) < 1e-4 for x, y in zip(a, b)),
          'Mapped inset run does not reproduce the inset spectrum')
    flat = runs['flat_R2']['dip_parabolic_ghz']
    tests = {}
    check(MODEL_TOLERANCE.keys() == MODEL_TESTS.keys(), 'Every model test needs its own tolerance')
    for name, grid_name in MODEL_TESTS.items():
        factor = grids[grid_name]['predicted_frequency_factor'] / grids['flat_R2']['predicted_frequency_factor']
        if name == 'inset_flat_R2':
            dip = min(runs[name]['local_minima_below_6db'], key=lambda m: abs(m['f_ghz'] - tm010))['f_ghz']
        else:
            dip = runs[name]['dip_parabolic_ghz']
        tests[name] = {'predicted_factor_vs_flat_R2': factor, 'measured_factor_vs_flat_R2': dip / flat,
                       'predicted_dip_ghz': flat * factor, 'measured_dip_ghz': dip,
                       'relative_error': dip / (flat * factor) - 1}
    tests['inset_flat_R2']['tm010_identified_by_map_at_ghz'] = tm010
    predictions = {name: grids[name]['predicted_frequency_factor']
                   for name in ('flat_R4', 'bent_R4', 'face_flat_R2p5', 'face_bent_R2p5')}
    geode = geode_reference()
    return {
        'schema': 'geode-meep-diagnosis/1',
        'note': ('Diagnosis of the Meep/GEODE disagreement on patch_conformal_L0 (#1088). '
                 'No spatial convergence is claimed for any Meep run.'),
        'runs': runs, 'grids': grids, 'mode_shapes': shapes,
        'harness_equivalence_bent_R2': harness_equivalence(raw['bent_R2']),
        'dft_window_equivalence_R1': window_equivalence(),
        'eh_gap_model_tests': tests,
        'eh_gap_model_unmeasured_factors': predictions,
        'flat_R2_vs_geode_flat': {
            'meep_flat_R2_dip_ghz': flat,
            'geode_flat_dips_ghz': geode['flat'],
            'meep_minus_geode_P1_L3_mhz': 1e3 * (flat - geode['flat']['L3_p1']),
            'meep_minus_geode_P2_L1_mhz': 1e3 * (flat - geode['flat']['L1_p2'])},
        'meep_bend_shift_R2_mhz': {
            'harness_shells': 1e3 * (runs['bent_R2']['dip_parabolic_ghz'] - flat),
            'cell_staircase': 1e3 * (runs['cells_bent_R2']['dip_parabolic_ghz'] - flat)},
        'geode': geode,
    }


def close(a, b, path='diagnosis'):
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


def check_environment():
    env = load(DIAG / 'environment.json')
    for key, name in [('dockerfile_sha256', 'Dockerfile.matched'),
                      ('conda_lock_sha256', 'matched-conda-linux-aarch64.txt'),
                      ('harness_sha256', 'matched_antenna.py')]:
        check(env[key] == digest(ROOT / 'reference/meep/docker' / name), f'{key} drift')
    urls = lambda p: {l for l in p.read_text().splitlines() if l.startswith('https:')}  # noqa: E731
    lock = urls(ROOT / 'reference/meep/docker/matched-conda-linux-aarch64.txt')
    check(urls(DIAG / 'conda-explicit.txt') == lock and len(lock) == env['explicit_package_urls_replayed'],
          'Installed environment differs from the package lock')
    for kind in ('volume', 'sheet'):
        new, old = load(DIAG / f'rc_{kind}_R2.json'), load(PILOT / f'rc_{kind}_R2.json')
        check(new['snapshots'] == old['snapshots'], f'RC {kind} R2 not reproduced')
    check(env['region'] == 'us-east-2' and env['instance_type'] == 'r8g.8xlarge', 'Host record')
    check(env['worker_final_state'] == 'terminated' and env['retained_volumes'] == 0, 'Worker not torn down')
    hours = env['launch_to_terminate_seconds'] / 3600
    check(abs(hours * env['cost_estimate_rate_usd_per_hour'] - env['compute_cost_upper_elapsed_estimate_usd']) < 1e-4,
          'Cost arithmetic')
    check(env['compute_cost_upper_elapsed_estimate_usd'] <= 15, 'Over the diagnosis budget')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='regenerate diagnosis.json')
    args = parser.parse_args()
    result = build()
    out = DIAG / 'diagnosis.json'
    if args.write:
        out.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
        print(f'wrote {out}')
        return
    check_environment()
    close(result, load(out))
    for line in (DIAG / 'SHA256SUMS').read_text().splitlines():
        want, name = line.split(None, 1)
        check(digest(DIAG / name) == want, f'SHA256SUMS mismatch: {name}')
    # The findings the record states, re-asserted from the data.
    runs, grids, tests = result['runs'], result['grids'], result['eh_gap_model_tests']
    check(runs['flat_R2']['min_is_interior'] and runs['bent_R2']['min_is_interior'], 'Dips not resolved')
    check(abs(result['flat_R2_vs_geode_flat']['meep_minus_geode_P1_L3_mhz']) < 20, 'Flat R2 vs GEODE')
    check(result['meep_bend_shift_R2_mhz']['harness_shells'] < -200, 'Meep bend shift (harness)')
    check(abs(result['meep_bend_shift_R2_mhz']['cell_staircase']) < 10, 'Meep bend shift (cells)')
    eq = result['dft_window_equivalence_R1']
    check(max(eq['harness_worker_8_ranks_vs_committed_R1_4_ranks']) == 0, 'R1 not reproduced')
    check(max(eq['diag_harness_window_vs_harness']) < 1e-12, 'diag_antenna.py differs from harness')
    check(all(abs(v) < 2 for v in result['geode']['detune_bent_minus_flat_mhz'].values()), 'GEODE bend shift')
    check(grids['flat_R2']['predicted_frequency_factor'] == 1.0, 'Flat R2 grid should be E/H consistent')
    for name, test in tests.items():
        check(abs(test['relative_error']) < MODEL_TOLERANCE[name], f'E/H gap model misses {name}')
    for name in GRIDS:
        check(grids[name]['slice_conductor_components'] == 2, f'{name}: shell not two closed conductors')
    print('PASS: diagnosis environment, run provenance and spectra, grid dumps, E/H gap model, GEODE comparison')


if __name__ == '__main__':
    main()
