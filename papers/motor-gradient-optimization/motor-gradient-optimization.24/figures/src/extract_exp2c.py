"""Extract the compact record of the post-hoc warm-stencil arm F_w and its control A' (#1224).

Inputs: the verified copy of the session (20 per-run traces) and the direct
final-refinement directory. Outputs: figures/src/data/exp2c/runs.json and
refinement.json, plus selection-check.json when the independent selection is given.
Every source file's SHA-256 is recorded so verify_exp2c.py can bind the extract to
the session inventory; `extract(session, refine)` lets the verifier re-derive the
extract from the raw traces when they are available.

Usage: python3 -I extract_exp2c.py <copy>/exp2-fd-warm/session <refine-dir> <out-dir> [<selection.json>]
"""
import collections, hashlib, json, re, sys
from pathlib import Path

PAT = re.compile(r'^(slsqp_fd_warm|slsqp)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')
LETTER = {'slsqp': 'Ap', 'slsqp_fd_warm': 'W'}
VKEYS = ('index', 'wall_fe', 'status', 'F_val', 'F_upper', 'mean_full', 'credited', 'admissible', 'mean_ok', 'x')


def run_record(f):
    raw = f.read_bytes(); d = json.loads(raw); m = PAT.match(f.name)
    led = d['ledger']; E = led['entries']; ev = d['events']
    stencil = [e['seconds'] for e in E if e['category'] == 'forward' and (e['note'] or '').startswith('slsqp_fd')]
    base = [e['seconds'] for e in E if e['category'] in ('forward', 'line_search') and not (e['note'] or '').startswith('slsqp_fd')]
    return {'file': f.name, 'sha256': hashlib.sha256(raw).hexdigest(), 'method': LETTER[m[1]],
            'method_spec': d['method_spec'], 'space': m[2], 'start': m[3], 'repeat': int(m[4]),
            'stop_reason': d['stop_reason'], 'host_contaminated': d['host_contaminated'],
            'training_warm': d['evaluator_ready']['session']['training']['warm'],
            'in_run_load_max': (d.get('in_run_load') or {}).get('max'), 'in_run_wall_seconds': d['in_run_wall_seconds'],
            'unit_seconds': d['unit_seconds'], 'budget_fe': d['budget_fe'], 'fd_step': d.get('fd_step'),
            'attainment': {t: {k: a.get(k) for k in ('attained', 'wall_fe', 'validation')} for t, a in d['attainment'].items()},
            'validations': [{k: (i if k == 'index' else v.get(k)) for k in VKEYS} for i, v in enumerate(d['validations'])],
            'category_seconds': {c: t['seconds'] for c, t in led['totals'].items()},
            'validation_charges_seconds': [e['seconds'] for e in E if e['category'] == 'validation'],
            'stencil_charges_seconds': stencil, 'base_charges_seconds': base,
            'train_events': sum(e.get('kind') == 'train' for e in ev),
            'gradient_train_events': sum(e.get('kind') == 'train' and e.get('gradient') is True for e in ev),
            'fd_train_events': sum(e.get('kind') == 'fd_train' for e in ev),
            'train_newton': [e.get('newton_iterations') for e in ev if e.get('kind') == 'train'],
            'fd_gradients': [{'components': len(g['components']), 'evaluated': sum('F' in c for c in g['components']),
                              'backward': sum(c.get('side') == 'backward' for c in g['components']),
                              'base_resolved': g.get('base_resolved'), 'refused': g.get('refused', False),
                              'cost_fe': g.get('cost_fe'), 'stencil_start': g.get('stencil_start'),
                              'starts': dict(collections.Counter(c.get('start') for c in g['components'] if 'F' in c)),
                              'newton': [c.get('newton_iterations') for c in g['components'] if 'F' in c]}
                             for g in (d.get('fd_gradients') or [])]}


def extract(session, refine):
    runs = [run_record(f) for f in sorted(Path(session).iterdir()) if PAT.match(f.name)]
    assert len(runs) == 20
    cohort = json.loads((Path(refine) / 'bundle/cohort.json').read_text())
    res = {}
    for key, e in cohort['designs'].items():
        row = {'space': e['space'], 'roles': e['roles'], 'sources': e['sources'], 'x_bits': e['x_bits'],
               'stator_offsets_zero': e['stator_offsets_zero'], 'checks': {}}
        for p in sorted((Path(refine) / 'results' / key[:12]).glob('*/result.json')):
            raw = p.read_bytes(); r = json.loads(raw); v = r.get('validation') or {}
            row['checks'][p.parent.name] = {'ok': r.get('ok'), 'status': r.get('status'), 'F': v.get('F'),
                                            'full_current_mean': v.get('full_current_mean'),
                                            'result_sha256': hashlib.sha256(raw).hexdigest()}
        res[key] = row
    return {'runs': runs}, {'cohort_sha256': hashlib.sha256((Path(refine) / 'bundle/cohort.json').read_bytes()).hexdigest(),
                            'designs': res}


if __name__ == '__main__':
    session, refine, out = map(Path, sys.argv[1:4])
    runs, ref = extract(session, refine)
    (out / 'runs.json').write_text(json.dumps(runs, indent=0) + '\n')
    (out / 'refinement.json').write_text(json.dumps(ref, indent=0) + '\n')
    if len(sys.argv) > 4:
        sp = Path(sys.argv[4]); sel = json.loads(sp.read_text())
        (out / 'selection-check.json').write_text(json.dumps({
            'selection_sha256': hashlib.sha256(sp.read_bytes()).hexdigest(), 'source_commit': sel['source_commit'],
            'designs': sorted([{'x_bits': d['x_bits']} for d in sel['designs'].values()], key=lambda d: d['x_bits']),
            'cells': [{k: c.get(k) for k in ('arm', 'kind', 'space', 'status', 'reason')} for c in sel['selection']['cells']]}, indent=0) + '\n')
    print(len(runs['runs']), 'runs;', len(ref['designs']), 'designs;', sum(len(d['checks']) for d in ref['designs'].values()), 'checks')
