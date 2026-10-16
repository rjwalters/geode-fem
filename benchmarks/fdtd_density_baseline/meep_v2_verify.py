#!/usr/bin/env python3
"""Validate the #1146 harness-v2 Meep records (corrected matched-geometry ladder).

Standard library only; checks stay active under python -O. `--write` regenerates
`meep_runs/v2/ladder.json` from the committed records; the default mode
recomputes it and requires an exact match, then re-asserts the findings that
HARNESS_V2.md states, each against its own tolerance.

What is checked:
* environment: the us-east-1 image replays the 179-URL package lock; worker
  torn down; cost arithmetic and the ~$40 budget;
* v1 is intact: matched_antenna.py and diag_antenna.py keep the hashes pinned
  by the R1/R2/R4/diagnosis records, and on this host v1 reproduces the
  committed R1 antenna run and the R2 RC oracles bit for bit;
* v2 provenance: every record was written by an archived matched_antenna_v2.py;
* the conventions v2 relies on (probe): Yee offsets for even and odd cell counts,
  point-current deposition on one sample, point-monitor readout;
* the point-source/point-monitor RC oracles and the sheet-equivalence oracle
  against v1, and the FR-4 material oracle against the designed eps(f);
* the grid audits: whole-cell gap in every patch column for Ex and Ez, 50-ohm
  whole-cell port, patch extent, recomputed from the recorded slices;
* every production spectrum (S11 = (Z-50)/(Z+50), drift, stopping flag), its
  dip, and the comparison with GEODE and Palace.
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
from meep_diag_verify import parabola_vertex  # noqa: E402

V2 = HERE / 'meep_runs/v2'
PILOT = HERE / 'meep_runs/aws_pilot'
DOCKER = ROOT / 'reference/meep/docker'
CONFIG = DOCKER / 'matched_initial_2g4.json'
ETA0 = 376.730313668
# v1 hashes pinned by the committed R2/R4/diagnosis records (diag/environment.json and
# every diag record); v2 must leave them untouched.
V1_HARNESS_SHA256 = '66d3285f058b42a62bd26671bb9c2955fd66572de99f4c0a1d96f35f7d9d6d05'

# name -> (geometry, resolution, loss model)
RUNS = {'c_flat_R2.5': ('flat', 2.5, 'conductivity'),
        'c_bent_R2.5': ('bent', 2.5, 'conductivity'),
        'c_bent_R5': ('bent', 5, 'conductivity')}
# Initialization-only audits (4 ranks), run before production.
AUDITS = {'flat_R2.5': ('flat', 2.5), 'bent_R2.5': ('bent', 2.5)}
TIMING = {}
# Per-case tolerances (filled from HARNESS_V2.md once the data are in).
RC_TOL = {'2': 5e-4, '2.5': 3e-4, '5': 1e-4}
MATERIAL_EPS_ABS_TOL = 1e-4
MATERIAL_TAND_REL_TOL = 5e-4


def load(path):
    return json.loads(path.read_text())


def archived_sources():
    return {digest(p): p.name for p in sorted((V2 / 'sources').glob('matched_antenna_v2.*.py'))}


def check_provenance(run, sources, ranks=None):
    check(run['meep_version'] == '1.34.0' and run['architecture'] == 'aarch64', 'Environment')
    check(run['harness_version'] == 'matched-antenna/2', 'Harness version')
    check(run['source_sha256'] in sources, 'matched_antenna_v2.py version not archived')
    if ranks is not None:
        check(run['mpi_ranks'] == ranks, 'Ranks')
    check(run['wall_seconds'] > 0 and run['sum_rank_peak_rss_bytes'] > 0, 'Resource data')


def fr4_epsilon(f, m):
    chi = m['sigma_l'] * m['f_n'] ** 2 / (m['f_n'] ** 2 - f * f - 1j * f * m['gamma'])
    return (1 + 1j * m['sigma_d'] / (2 * math.pi * f)) * (m['eps_inf'] + chi)


def check_fr4(run):
    """Recompute the realized FR-4 table from the recorded model (pure stdlib)."""
    m = run['fr4_model']
    for g, er, td in run['fr4_realized']:
        e = fr4_epsilon(g / 299.792458, m)
        check(abs(e.real - er) < 1e-12 and abs(e.imag / e.real - td) < 1e-12, 'FR-4 table')
    for g, mr, mi, fr, fi in run['fr4_meep_medium_epsilon_vs_formula']:
        check(abs(mr - fr) < 1e-12 and abs(mi - fi) < 1e-12, "Meep's Medium.epsilon differs from the formula")
    anchor = fr4_epsilon(m['anchor_ghz'] / 299.792458, m)
    check(abs(anchor.real - m['eps_r']) < 1e-9, 'eps at the anchor')
    check(abs(anchor.imag / anchor.real - m['tan_delta']) < 1e-9, 'tan d at the anchor')
    tds = [td for _, _, td in run['fr4_realized']]
    return {'model': m.get('model', 'lorentz'), 'anchor_ghz': m['anchor_ghz'],
            'tan_delta_min': min(tds), 'tan_delta_max': max(tds),
            'tan_delta_band_2p30_2p42': [min(td for g, _, td in run['fr4_realized'] if 2.30 - 1e-9 <= g <= 2.42 + 1e-9),
                                         max(td for g, _, td in run['fr4_realized'] if 2.30 - 1e-9 <= g <= 2.42 + 1e-9)],
            'eps_real_range': [min(e for _, e, _ in run['fr4_realized']), max(e for _, e, _ in run['fr4_realized'])]}


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
    check(abs(drift - run['max_temporal_change']) < 1e-12, 'Temporal drift')
    check(run['temporal_tolerance_met'] == (drift <= run['temporal_tolerance']), 'Stopping flag')
    check(not run['spatially_converged'], 'No spatial convergence is claimed by a single run')


def summarize(run):
    freqs = run['frequency_ghz']
    snap = run['snapshots'][-1]
    gamma = [complex(*g) for g in snap['s11']]
    z = [complex(*v) for v in snap['impedance_ohm']]
    db = [20 * math.log10(abs(g)) for g in gamma]
    i = min(range(len(db)), key=db.__getitem__)
    interior = 0 < i < len(db) - 1
    dip_f, dip_db = (parabola_vertex(freqs[i - 1:i + 2], db[i - 1:i + 2]) if interior else (freqs[i], db[i]))
    # Re Z at the dip: linear interpolation of the sampled Z to the parabolic dip frequency.
    j = max(0, min(len(freqs) - 2, int((dip_f - freqs[0]) // (freqs[1] - freqs[0]))))
    t = (dip_f - freqs[j]) / (freqs[j + 1] - freqs[j])
    z_dip = z[j] * (1 - t) + z[j + 1] * t
    minima = []
    for k in range(1, len(db) - 1):
        if db[k] < db[k - 1] and db[k] <= db[k + 1] and db[k] < -6:
            fv, dv = parabola_vertex(freqs[k - 1:k + 2], db[k - 1:k + 2])
            minima.append({'f_ghz': fv, 's11_db': dv})
    at = {}
    for target in (2.36, 2.40, 2.45):
        k = min(range(len(freqs)), key=lambda q: abs(freqs[q] - target))
        check(abs(freqs[k] - target) < 1e-9, 'Target frequency not sampled')
        at[f'{target:.2f}'] = {'s11_db': db[k], 'z_ohm': [z[k].real, z[k].imag]}
    return {
        'final_time': snap['time'], 'checks_run': len(run['snapshots']) - 1, 'steps': run['steps'],
        'max_temporal_change': run['max_temporal_change'],
        'temporal_tolerance_met': run['temporal_tolerance_met'],
        'window_ghz': [freqs[0], freqs[-1]], 'min_is_interior': interior,
        'dip_parabolic_ghz': dip_f, 'dip_parabolic_db': dip_db,
        'z_at_dip_ohm': [z_dip.real, z_dip.imag],
        'local_minima_below_6db': minima, 'at': at,
        'wall_seconds': run['wall_seconds'], 'init_seconds': run['init_seconds'],
        'stepping_seconds': run['stepping_seconds'],
        'seconds_per_step': run['stepping_seconds'] / run['steps'],
        'mpi_ranks': run['mpi_ranks'], 'grid_cells_estimated': run['estimated_cells'],
        'sum_rank_peak_rss_gib': run['sum_rank_peak_rss_bytes'] / 2**30,
    }


def cgroup_peak_gib(name):
    peak = 0
    for line in (V2 / 'logs' / f'{name}.cgroup_memory.log').read_text().splitlines():
        for tok in line.split():
            if tok.startswith('peak=') and tok[5:].isdigit():
                peak = max(peak, int(tok[5:]))
    check(peak > 0, f'{name}: no cgroup memory samples')
    return peak / 2**30


def gap_from_slices(audit, layout):
    """Recount the gap per column from the recorded y-row slices (half board)."""
    n = layout['n_gap']
    pc = dict((i, k) for i, k in layout['patch_columns'])
    ex, ez = audit['slice_Ex'], audit['slice_Ez']
    i0, k0 = ex['i0'], ex['k0']
    cols = [(c, ''.join(row[c - i0] for row in ex['rows'])) for c in sorted(pc) if c >= i0]
    h_h = []
    for c, col in cols:
        runs = [r for r in col.replace('A', ' ').replace('I', ' ').split()]
        check(len(runs) == 1 and runs[0] == 'M' + 'F' * (n - 1) + 'M', f'Ex column {c} gap')
        h_h.append(n)
        top = k0 + col.rindex('M')
        check(top == pc[c] + n // 2, f'Ex column {c} patch plane')
    h_e, steps = [], 0
    for c in sorted(pc):
        if c - 1 not in pc or c < i0:
            continue
        col = ''.join(row[c - i0] for row in ez['rows'])
        free = max(len(r) for r in col.replace('M', ' ').replace('A', ' ').replace('I', ' ').split())
        step = pc[c - 1] != pc[c]
        steps += step
        check(free == (n - 1 if step else n), f'Ez line {c} free run')
        h_e.append(free)
    inv_h = sum(1 / h for h in h_h) / len(h_h)
    inv_e = sum(1 / h for h in h_e) / len(h_e)
    return {'n_gap_cells': n, 'ex_columns_half_patch': len(h_h), 'ez_lines_half_patch': len(h_e),
            'ez_step_lines_half_patch': steps,
            'diag_gap_model_factor': math.sqrt(inv_h / inv_e)}


def audit_summary(run):
    a, L = run['audit'], run['layout']
    check(a['errors'] == [], 'Grid audit errors recorded')
    check(abs(run['discrete_port_ohm'] - 50) < 1e-9 and abs(a['port_discrete_ohm'] - 50) < 1e-9, 'Port ohm')
    n, nw = L['n_gap'], L['n_port_rows']
    check(abs(n * L['dx'] - 1.6) < 1e-9 and abs(nw * L['dx'] - 1.6) < 1e-9, 'Gap/port not 1.6 mm')
    check(len(a['port']['ez']) == n * nw, 'Port sample count')
    check(run['n_source_samples'] == run['n_monitor_samples'] == n * nw, 'Source/monitor samples')
    check(a['patch_ex_metal_rows'] == L['n_patch_l'] + 1 and a['patch_ey_metal_rows'] == L['n_patch_l'],
          'Patch rows')
    for comp, conv in a['lattice_convention'].items():
        if 'assumed_pure' in conv:
            check(conv['assumed_pure'] and conv['shifted_has_mixed'], f'{comp} lattice convention')
    out = {k: L[k] for k in ('board_centre_mm', 'patch_length_mm', 'patch_width_mm', 'port_centre_mm',
                             'probe_inset_from_patch_edge_mm', 'port_width_mm', 'gap_mm', 'n_gap')}
    out.update({'patch_columns': a['patch_columns'], 'ez_step_lines': a['ez_step_lines'],
                'ez_lines': a['ez_lines'], 'ez_free_run_histogram': a['ez_free_run_histogram'],
                'port_discrete_ohm': a['port_discrete_ohm'], 'time_step': run['time_step']})
    out.update(gap_from_slices(a, L))
    return out


def probes():
    out = {}
    for r in ('2.5', '5'):
        p = load(V2 / f'probe_R{r}.json')['probe']
        dx = 1 / float(r)
        want = {'Ex': [[0.5], [0.0], [0.0]], 'Ey': [[0.0], [0.5], [0.0]], 'Ez': [[0.0], [0.0], [0.5]]}
        for key in ('yee_offsets_cells_n24', 'yee_offsets_cells_n25'):
            check(p[key] == want, f'R{r} {key}: Yee convention')
        dep = {k: complex(*v) for k, v in p['point_deposition_ez'].items()}
        check(abs(dep['0,0,0'] + p['dt'] / dx) < 1e-12, f'R{r}: point deposition = -dt J/eps')
        check(all(v == 0 for k, v in dep.items() if k != '0,0,0'), f'R{r}: point current spilled')
        sheet = {k: complex(*v).real for k, v in p['sheet_deposition_ez'].items()}
        total = sum(sheet.values())
        inside = sum(v for k, v in sheet.items()
                     if int(k.split(',')[0]) in (0, 1) and int(k.split(',')[1]) in (-1, 0))
        check(abs(complex(*p['point_dft_at_neighbour'])) == 0, f'R{r}: monitor reads the neighbour')
        check(abs(complex(*p['point_dft_at_sample'])) > 0, f'R{r}: monitor reads nothing')
        out[f'R{r}'] = {'dt': p['dt'], 'v1_sheet_share_outside_its_samples': 1 - inside / total}
    return out


def oracles():
    out = {}
    for r in ('2', '2.5', '5'):
        o = load(V2 / f'rc_point_R{r}.json')
        check(o['mode'] == 'rc_oracle' and not o['rc_sheet'], 'RC oracle mode')
        check(o['max_rc_impedance_relative_error'] < RC_TOL[r], f'RC point oracle R{r}')
        out[f'rc_point_R{r}'] = o['max_rc_impedance_relative_error']
    v1 = load(PILOT / 'rc_volume_R2.json')
    out['rc_point_R2_vs_v1_volume_error'] = abs(out['rc_point_R2'] - v1['max_rc_impedance_relative_error'])
    check(out['rc_point_R2_vs_v1_volume_error'] < 1e-9, 'Point RC R2 differs from v1 volume RC')
    sp, v1s = load(V2 / 'rc_sheetpoint_R2.json'), load(PILOT / 'rc_sheet_R2.json')
    check(sp['rc_sheet'] and [s['time'] for s in sp['snapshots']] == [s['time'] for s in v1s['snapshots']],
          'Sheet-equivalence oracle settings')
    d = max(abs(complex(*a) - complex(*b)) for a, b in zip(sp['snapshots'][-1]['s11'], v1s['snapshots'][-1]['s11']))
    check(d < 1e-12, 'Sheet-equivalent point currents do not reproduce v1 rc_sheet_R2')
    out['sheet_equivalence_max_s11_difference_vs_v1'] = d
    for r in ('2.5', '5', 'conductivity_R5'):
        m = load(V2 / (f'material_{r}.json' if r.startswith('c') else f'material_R{r}.json'))
        check(m['temporal_tolerance_met'], f'Material oracle R{r} stopping')
        de = max(abs(a[1] - b[1]) for a, b in zip(m['fr4_realized'], m['measured_fr4']))
        dt = max(abs(b[2] / a[2] - 1) for a, b in zip(m['fr4_realized'], m['measured_fr4']))
        check(de < MATERIAL_EPS_ABS_TOL and dt < MATERIAL_TAND_REL_TOL, f'Material oracle R{r}')
        if r.startswith('c'):
            check(m['fr4_model']['model'] == 'conductivity', 'conductivity material oracle model')
        out[f'material_{r}' if r.startswith('c') else f'material_R{r}'] = {'max_eps_real_abs_error': de, 'max_tan_delta_rel_error': dt,
                                 'designed': check_fr4(m)}
    return out


def v1_intact():
    check(digest(DOCKER / 'matched_antenna.py') == V1_HARNESS_SHA256, 'v1 harness changed')
    diag_env = load(HERE / 'meep_runs/diag/environment.json')
    check(diag_env['harness_sha256'] == V1_HARNESS_SHA256, 'diag pin')
    diag_hashes = {load(p)['source_sha256']['diag_antenna.py'] for p in (HERE / 'meep_runs/diag').glob('*_R2.json')
                   if 'source_sha256' in load(p) and isinstance(load(p)['source_sha256'], dict)}
    archived = {digest(p) for p in (HERE / 'meep_runs/diag/sources').glob('diag_antenna.*.py')}
    check(digest(DOCKER / 'diag_antenna.py') in archived and diag_hashes <= archived, 'diag_antenna.py changed')
    r1, c1 = load(V2 / 'v1_initial_R1.json'), load(HERE / 'meep_runs/initial_R1.json')
    check(r1['source_sha256'] == V1_HARNESS_SHA256, 'v1 R1 replay provenance')
    check(r1['snapshots'] == c1['snapshots'], 'v1 harness no longer reproduces the committed R1 run')
    for kind in ('volume', 'sheet'):
        new, old = load(V2 / f'v1_rc_{kind}_R2.json'), load(PILOT / f'rc_{kind}_R2.json')
        check(new['snapshots'] == old['snapshots'], f'v1 RC {kind} R2 not reproduced')
    geo = (V2 / 'logs/v1_geometry.log').read_text()
    check('PASS' in geo and 'geometry_exit=0' in geo, 'v1 geometry check')
    return {'v1_r1_ranks_here_vs_committed': [r1['mpi_ranks'], c1['mpi_ranks']],
            'v1_r1_snapshots_identical': True, 'v1_rc_r2_identical': True}


def geode_reference():
    conv = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/convergence.toml').read_text())
    v6 = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/v6/v6_summary.toml').read_text())
    out = {'source': 'convergence.toml [s11.*], [gate_g1.*], [detune]; v6/v6_summary.toml [x1]'}
    for geom in ('flat', 'bent'):
        out[geom] = {f"{lv['level']}_{lv['element_order']}": {
            'dip_f_ghz': lv['dip_f_ghz'], 'dip_db': lv['dip_db'], 'dip_z_re_ohm': lv['dip_z_re_ohm']}
            for lv in conv['s11'][geom]}
        g = conv['gate_g1'][geom]
        out[f'{geom}_lower_bound_dip_ghz'] = g['lower_bound_dip_f_ghz']
        out[f'{geom}_p2_extrapolated_dip_ghz'] = g['p2_extrapolated_dip_f_ghz']
    out['detune_bent_minus_flat_mhz'] = {k: v['dip_shift_mhz'] for k, v in conv['detune'].items()}
    out['palace_initial_L1'] = {p: {'dip_f_ghz': v6['x1']['palace'][f'init_L1_{p}']['dip_f_ghz'],
                                    'dip_db': v6['x1']['palace'][f'init_L1_{p}']['dip_db']}
                                for p in ('p1', 'p2', 'p3')}
    out['geode_p1_x1comb_L1_dip_ghz'] = [g['dip_f_ghz'] for g in v6['x1']['geode_p1']
                                         if g['fixture'] == 'x1comb_L1'][0]
    out['geode_rim_fit_f_inf_ghz'] = v6['x1']['rim_fit']['f_inf_ghz']
    out['geode_rim_richardson_order1_ghz'] = v6['x1']['rim_fit']['richardson_order1_rim_pair_ghz']
    out['textbook_delta_l_mm'] = v6['x1']['textbook_tl_delta_l_mm']
    x1 = tomllib.loads((ROOT / 'benchmarks/patch_antenna_conformal/v6/x1/dip_x1comb_L1.toml').read_text())
    out['geode_x1comb_L1_cost'] = {'host': x1['meta']['host'], 'n_dofs': x1['meta']['n_dofs'],
                                   'n_solves': x1['cost']['n_solves'], 'total_s': x1['cost']['total_s'],
                                   'peak_rss_gib': x1['cost']['peak_rss_gib'],
                                   'dip_z_re_ohm': x1['dip']['bent']['sampled_z_re_ohm']}
    return out


def comparison(runs, grids, geode):
    """Meep v2 against GEODE/Palace, the bend shift, and the R2.5 -> R5 trend."""
    flat, b25, b5 = (runs[n]['dip_parabolic_ghz'] for n in ('c_flat_R2.5', 'c_bent_R2.5', 'c_bent_R5'))
    p3 = geode['palace_initial_L1']['p3']['dip_f_ghz']
    dl = geode['textbook_delta_l_mm']
    l25, l5 = grids['c_bent_R2.5']['patch_length_mm'], grids['c_bent_R5']['patch_length_mm']
    return {
        'flat_R2.5_minus_geode_flat_mhz': {k: 1e3 * (flat - v['dip_f_ghz']) for k, v in geode['flat'].items()},
        'bend_shift_R2.5_mhz': 1e3 * (b25 - flat),
        'geode_bend_shift_mhz': geode['detune_bent_minus_flat_mhz'],
        'bent_minus_palace_p3_mhz': {'R2.5': 1e3 * (b25 - p3), 'R5': 1e3 * (b5 - p3)},
        'bent_minus_geode_x1comb_p1_mhz': {'R2.5': 1e3 * (b25 - geode['geode_p1_x1comb_L1_dip_ghz']),
                                          'R5': 1e3 * (b5 - geode['geode_p1_x1comb_L1_dip_ghz'])},
        'bent_minus_geode_lower_bound_mhz': {'R2.5': 1e3 * (b25 - geode['bent_lower_bound_dip_ghz']),
                                            'R5': 1e3 * (b5 - geode['bent_lower_bound_dip_ghz'])},
        'bent_trend_R2.5_to_R5_mhz': 1e3 * (b5 - b25),
        'bent_depth_trend_R2.5_to_R5_db': runs['c_bent_R5']['dip_parabolic_db'] - runs['c_bent_R2.5']['dip_parabolic_db'],
        # Estimate only (textbook open-end extension, v6 [x1]): the R2.5 patch is 28.8 mm,
        # R5's 29.0 mm; f ~ 1/(L + 2 dL) predicts this part of the R2.5 -> R5 change.
        'textbook_patch_length_shift_R2.5_to_R5_mhz': 1e3 * b25 * ((l25 + 2 * dl) / (l5 + 2 * dl) - 1),
        'bent_resolutions': 2,
        'spatial_convergence_established': False,
    }


def build():
    sources = archived_sources()
    runs, audits = {}, {}
    for name, (geom, res, model) in RUNS.items():
        run = load(V2 / f'{name}.json')
        check_provenance(run, sources)
        check(run['mode'] == 'antenna' and run['geometry'] == geom and
              run['resolution_cells_per_mm'] == res and run['fr4_model'].get('model', 'lorentz') == model,
              f'{name}: spec')
        check(run['configuration'] == load(CONFIG), 'Configuration drift')
        check_spectrum(run)
        s = summarize(run)
        s['cgroup_peak_gib'] = cgroup_peak_gib(name)
        s['fr4'] = check_fr4(run)
        runs[name] = s
        audits[name] = audit_summary(run)
    for name, (geom, res) in AUDITS.items():
        run = load(V2 / f'audit_{name}.json')
        check_provenance(run, sources, 4)
        check(run['geometry'] == geom and run['resolution_cells_per_mm'] == res, f'audit {name}: spec')
        audits[f'audit_{name}'] = audit_summary(run)
    timing = {}
    for name, (model, res, ranks) in TIMING.items():
        run = load(V2 / f'{name}.json')
        check_provenance(run, sources, ranks)
        check(run['mode'] == 'timing_probe' and run['fr4_model'].get('model', 'lorentz') == model, 'timing spec')
        timing[name] = {'model': model, 'resolution': res, 'ranks': ranks,
                        'seconds_per_step': run['seconds_per_step'], 'steps': run['steps']}
    geode = geode_reference()
    return {'schema': 'geode-meep-v2-ladder/1',
            'note': ('Harness v2 (#1146): grid-aligned metal, whole-cell gap and port, flat-loss FR-4. '
                     'Two bent resolutions do not establish spatial convergence.'),
            'probes': probes(), 'oracles': oracles(), 'v1': v1_intact(),
            'runs': runs, 'grids': audits, 'timing': timing,
            'comparison': comparison(runs, audits, geode) if 'c_bent_R5' in runs else None,
            'geode': geode}


def close(a, b, path='ladder'):
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
    env = load(V2 / 'environment.json')
    for key, name in [('dockerfile_sha256', 'Dockerfile.matched'),
                      ('conda_lock_sha256', 'matched-conda-linux-aarch64.txt'),
                      ('v1_harness_sha256', 'matched_antenna.py')]:
        check(env[key] == digest(DOCKER / name), f'{key} drift')
    urls = lambda p: {l for l in p.read_text().splitlines() if l.startswith('https:')}  # noqa: E731
    lock = urls(DOCKER / 'matched-conda-linux-aarch64.txt')
    check(urls(V2 / 'conda-explicit.txt') == lock and len(lock) == env['explicit_package_urls_replayed'] == 179,
          'Installed environment differs from the package lock')
    check(env['region'] == 'us-east-1' and env['instance_type'] == 'r8g.8xlarge', 'Host record')
    check(env['worker_final_state'] == 'terminated' and env['retained_volumes'] == 0, 'Worker not torn down')
    hours = env['launch_to_terminate_seconds'] / 3600
    check(abs(hours * env['cost_estimate_rate_usd_per_hour'] - env['compute_cost_upper_elapsed_estimate_usd']) < 1e-4,
          'Cost arithmetic')
    check(env['compute_cost_upper_elapsed_estimate_usd'] <= 40, 'Over the #1146 budget')
    return env


# Per-finding brackets (MHz unless stated), each bracketing the value HARNESS_V2.md states.
FIND = {
    'flat_R2.5_minus_geode_P2_L1': (5.0, 15.0),     # stated +10.3
    'bend_shift_R2.5': (-3.0, 0.0),                 # stated -1.6 (GEODE -0.97 to -1.58)
    'bent_R2.5_minus_palace_p3': (-2.0, 3.0),       # stated +0.8
    'bent_R5_minus_palace_p3': (-7.0, -3.0),       # stated -4.9
    'bent_R2.5_minus_geode_lower_bound': (5.0, 15.0),   # stated +10.1
    'bent_R5_minus_geode_lower_bound': (2.0, 7.0),  # stated +4.5
    'trend_R2.5_to_R5': (-8.0, -3.0),              # stated -5.6
    're_z_at_dip_ohm': (40.0, 50.0),
}


def findings(result):
    """The statements HARNESS_V2.md makes, re-asserted from the data, each against its own bracket."""
    c, runs = result['comparison'], result['runs']

    def within(name, value):
        lo, hi = FIND[name]
        check(lo <= value <= hi, f'Finding {name}: {value:.3f} outside [{lo}, {hi}]')
    within('flat_R2.5_minus_geode_P2_L1', c['flat_R2.5_minus_geode_flat_mhz']['L1_p2'])
    within('bend_shift_R2.5', c['bend_shift_R2.5_mhz'])
    within('bent_R2.5_minus_palace_p3', c['bent_minus_palace_p3_mhz']['R2.5'])
    within('bent_R5_minus_palace_p3', c['bent_minus_palace_p3_mhz']['R5'])
    within('bent_R2.5_minus_geode_lower_bound', c['bent_minus_geode_lower_bound_mhz']['R2.5'])
    within('bent_R5_minus_geode_lower_bound', c['bent_minus_geode_lower_bound_mhz']['R5'])
    within('trend_R2.5_to_R5', c['bent_trend_R2.5_to_R5_mhz'])
    for name, r in runs.items():
        check(r['min_is_interior'] and len(r['local_minima_below_6db']) == 1,
              f'{name}: expected one resonance below -6 dB in 1.6-3.0 GHz')
        within('re_z_at_dip_ohm', r['z_at_dip_ohm'][0])
    for name in ('c_flat_R2.5', 'c_bent_R2.5'):
        check(runs[name]['temporal_tolerance_met'], f'{name}: stopping tolerance not met')
    check(not c['spatial_convergence_established'] and c['bent_resolutions'] == 2,
          'Two bent resolutions cannot establish convergence')
    check(runs['c_bent_R5']['temporal_tolerance_met'], 'c_bent_R5: stopping tolerance not met')
    for name in ('c_bent_R2.5', 'c_bent_R5'):
        check(result['grids'][name]['ez_step_lines'] > 0, f'{name}: bent board without staircase steps')
    check(result['grids']['c_flat_R2.5']['diag_gap_model_factor'] == 1.0, 'Flat grid E/H consistent')


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--write', action='store_true', help='regenerate ladder.json')
    args = ap.parse_args()
    result = build()
    out = V2 / 'ladder.json'
    if args.write:
        out.write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')
        print(f'wrote {out}')
        return
    check_environment()
    close(result, load(out))
    for line in (V2 / 'SHA256SUMS').read_text().splitlines():
        want, name = line.split(None, 1)
        check(digest(V2 / name) == want, f'SHA256SUMS mismatch: {name}')
    findings(result)
    print('PASS: v2 environment, v1 intact, probes, oracles, grid audits, spectra, GEODE/Palace comparison')


if __name__ == '__main__':
    main()
