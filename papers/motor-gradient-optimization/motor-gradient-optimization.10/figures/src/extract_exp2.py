"""Extract the compact Exp 2 record used by the v7 figures and verifier.

Input: the verified copy of the main study (100 per-run traces) and the direct
final-refinement results. Output: figures/src/data/exp2/{runs,refinement}.json.
Every source file's SHA-256 is recorded so the extract can be checked against
the retained study inventory.
"""
import hashlib, json, re, sys
from pathlib import Path

main, refine, out = map(Path, sys.argv[1:4])
pat = re.compile(r'^(slsqp_l0|cobyqa_l0|slsqp|cobyqa|surrogate)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')
LETTER = {'slsqp': 'A', 'cobyqa': 'B', 'surrogate': 'C', 'cobyqa_l0': 'D', 'slsqp_l0': 'E'}
runs = []
for f in sorted(main.iterdir()):
    m = pat.match(f.name)
    if not m:
        continue
    raw = f.read_bytes(); d = json.loads(raw)
    vals = [{'index': i, 'wall_fe': v['wall_fe'], 'solve_fe': v.get('solve_fe'), 'status': v['status'],
             'F_val': v.get('F_val'), 'F_upper': v.get('F_upper'), 'mean_full': v.get('mean_full'),
             'credited': v.get('credited'), 'admissible': v.get('admissible'), 'mean_ok': v.get('mean_ok'),
             'mesh_kind': (v.get('mesh') or {}).get('kind')}
            for i, v in enumerate(d['validations'])]
    runs.append({'file': f.name, 'sha256': hashlib.sha256(raw).hexdigest(), 'method': LETTER[m[1]],
                 'space': m[2], 'start': m[3], 'repeat': int(m[4]), 'stop_reason': d['stop_reason'],
                 'host_contaminated': d['host_contaminated'], 'in_run_load_max': (d.get('in_run_load') or {}).get('max'), 'longest_overload_s': d.get('longest_overload_s'), 'in_run_wall_seconds': d['in_run_wall_seconds'],
                 'unit_seconds': d['unit_seconds'], 'budget_fe': d['budget_fe'],
                 'attainment': {t: {k: a.get(k) for k in ('attained', 'censored', 'wall_fe', 'validation')}
                                for t, a in d['attainment'].items()},
                 'checkpoint_60': d['checkpoint_60']['targets'], 'validations': vals})
assert len(runs) == 100
anchor = json.loads((main / 'slsqp_a_theta0_r1.json').read_text())['evaluator_ready']['anchor']
(out / 'runs.json').write_text(json.dumps({'anchor': anchor, 'runs': runs}, indent=0) + '\n')
cohort = json.loads((refine / 'bundle/cohort.json').read_text())
res = {}
for key, e in cohort['designs'].items():
    row = {'space': e['space'], 'roles': e['roles'], 'sources': e['sources'], 'x': e['x'],
           'stator_offsets_zero': e['stator_offsets_zero'], 'checks': {}}
    for p in sorted((refine / 'results' / key[:12]).glob('*/result.json')):
        raw = p.read_bytes(); r = json.loads(raw); v = r.get('validation') or {}
        row['checks'][p.parent.name] = {'ok': r.get('ok'), 'status': r.get('status'), 'error': r.get('error'),
            'F': v.get('F'), 'full_current_mean': v.get('full_current_mean'),
            'currents': v.get('currents'), 'wall_seconds': r.get('wall_seconds'),
            'result_sha256': hashlib.sha256(raw).hexdigest()}
    res[key] = row
(out / 'refinement.json').write_text(json.dumps({'meshes': cohort['meshes'], 'designs': res}, indent=0) + '\n')
print(len(runs), 'runs;', len(res), 'designs;', sum(len(r['checks']) for r in res.values()), 'checks')
