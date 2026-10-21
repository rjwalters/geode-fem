"""Raw-record statistics for the v4 validation figure (no field solves)."""
import json
import re
import statistics as st
from pathlib import Path

D = Path(__file__).resolve().parent / 'data'


def read(path):
    return json.loads((D / path).read_text())


def reference_runs():
    out = {}
    for p in (D / 'reference_refinement/runs').glob('*.json'):
        match = re.fullmatch(r's(\d+)_c\d+', p.stem)
        if not match:
            continue
        run = json.loads(p.read_text())
        indices, torques = run['current_indices'], run['torque_Nm']
        if isinstance(indices, int):
            indices, torques = [indices], [torques]
        for index, values in zip(indices, torques):
            assert run['angles_deg'] == list(range(20))
            out[int(match[1]), index - 1] = values
    return out


def candidate_stats(design, model, level=3, rule='fixed', step=1):
    run = read(f'validation/runs/{design}_L{level}_{model}_{rule}.json')
    assert run['status'] == 'ok' and run['solves_failed'] == 0
    key = 'T_arkkio_outer' if model == 'conforming' else 'T_coupling'
    if model == 'conforming':
        assert abs(run['constraints']['gap_outer_band_area_ratio'] - 1) < .001
        assert run['mesh_source'] in ('fresh', 'nurbs')
    result = {}
    for current in run['currents']:
        rows = sorted(current['rows'], key=lambda row: row['angle_deg'])
        rows = [row for row in rows if abs(row['angle_deg']/step - round(row['angle_deg']/step)) < 1e-8]
        assert [row['angle_deg'] for row in rows] == [i*step for i in range(round(20/step))]
        assert all(row['status'] in ('tolerance', 'roundoff_floor') for row in rows)
        values = [row[key] for row in rows]
        result[current['fraction']] = {'mean': st.mean(values), 'std': st.pstdev(values)}
    return result


def objective(stats):
    return sum(stats[q]['std']**2 for q in (0, .5, 1))


def facts():
    raw = reference_runs()
    ref = {str(n): [{'mean': st.mean(raw[n,q]), 'std': st.pstdev(raw[n,q])} for q in range(3)] for n in (3,6,12,24)}
    saved = read('reference_data.json')['designs']['initial']['torque_Nm_for_modeled_sector_by_current_then_angle']
    geode = read('harmonic/runs/L3_h45.json')
    h = [[row['T_arkkio'] for row in sorted(geode['rows'], key=lambda r:r['angle_deg']) if row['current_index']==q] for q in range(3)]
    result = {'reference': ref, 'saved_reproduction_max_Nm': max(abs(a-b) for q in range(3) for a,b in zip(raw[3,q],saved[q])),
              'refined_gap_percent': [100*(st.pstdev(h[q])/st.pstdev(raw[24,q])-1) for q in range(3)],
              'refined_cogging_max_gap_Nm': max(abs(a-b) for a,b in zip(h[0],raw[24,0])),
              'candidate': {}, 'current_means': {}}
    for model in ('h45','h93','conforming'):
        result['candidate'][model] = {}
        for design in ('initial_nurbs','opt120_export'):
            stats = candidate_stats(design, model)
            result['candidate'][model][design] = {'F': objective(stats), 'std_full': stats[1]['std'], 'mean_full': stats[1]['mean'],
                'refinement_percent': 100*(objective(stats)-objective(candidate_stats(design,model,level=2)))/objective(stats)}
        result['candidate'][model]['g10_vs_g40_percent'] = 100*(objective(candidate_stats('opt120_export',model,step=2))/objective(candidate_stats('opt120_export',model,step=.5))-1)
    for rule in ('fixed','fixed-physical','jinit'):
        result['current_means'][rule] = candidate_stats('opt120_export','h45',rule=rule)[1]['mean']
    return result
