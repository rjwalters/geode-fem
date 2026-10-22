"""Extract the compact record of the post-hoc FD-SLSQP ablation (method F and its control A') (v11 onward).

Inputs: the verified copy of the ablation session (40 per-run traces) and the
direct final-refinement directory. Outputs: figures/src/data/exp2b/runs.json and
refinement.json. Every source file's SHA-256 is recorded so verify_exp2b.py can
bind the extract to the ablation inventory. `extract(session, refine)` is
imported by verify_exp2b.py to re-derive the extract from the raw traces when
they are available.

Usage: python3 -I extract_exp2b.py <copy>/exp2-fd-ablation/session <refine-dir> <out-dir> [<selection.json>]

With the optional selection.json (written by final_refinement/ablation_selection.py),
also writes selection-check.json: that file's SHA-256 and source commit and the
x_bits and roles of each design it selected.
"""
import hashlib, json, re, sys
from pathlib import Path

PAT = re.compile(r'^(slsqp_fd|slsqp)_(a|b)_(theta0|lhs\d)_r(\d)\.json$')
LETTER = {'slsqp': 'Ap', 'slsqp_fd': 'F'}
VKEYS = ('index', 'wall_fe', 'status', 'F_val', 'F_upper', 'mean_full', 'credited', 'admissible', 'mean_ok', 'x')


def run_record(f):
    raw = f.read_bytes(); d = json.loads(raw); m = PAT.match(f.name)
    led = d['ledger']; ent = [e for e in led['entries'] if e['category'] == 'validation']
    ev = d['events']
    fdg = d.get('fd_gradients') or []
    return {'file': f.name, 'sha256': hashlib.sha256(raw).hexdigest(), 'method': LETTER[m[1]],
            'method_spec': d['method_spec'], 'space': m[2], 'start': m[3], 'repeat': int(m[4]),
            'stop_reason': d['stop_reason'], 'host_contaminated': d['host_contaminated'],
            'training_warm': d['evaluator_ready']['session']['training']['warm'],
            'in_run_load_max': (d.get('in_run_load') or {}).get('max'), 'in_run_wall_seconds': d['in_run_wall_seconds'],
            'unit_seconds': d['unit_seconds'], 'budget_fe': d['budget_fe'], 'fd_step': d.get('fd_step'),
            'attainment': {t: {k: a.get(k) for k in ('attained', 'wall_fe', 'validation')} for t, a in d['attainment'].items()},
            'validations': [{k: (i if k == 'index' else v.get(k)) for k in VKEYS} for i, v in enumerate(d['validations'])],
            'category_seconds': {c: t['seconds'] for c, t in led['totals'].items()},
            'validation_charges_seconds': [e['seconds'] for e in ent],
            'train_events': sum(e.get('kind') == 'train' for e in ev),
            'gradient_train_events': sum(e.get('kind') == 'train' and e.get('gradient') is True for e in ev),
            'fd_train_events': sum(e.get('kind') == 'fd_train' for e in ev),
            'fd_gradients': [{'components': len(g['components']), 'evaluated': sum('F' in c for c in g['components']),
                              'backward': sum(c.get('side') == 'backward' for c in g['components']),
                              'base_resolved': g.get('base_resolved'), 'refused': g.get('refused', False),
                              'cost_fe': g.get('cost_fe')} for g in fdg]}


def extract(session, refine):
    runs = [run_record(f) for f in sorted(Path(session).iterdir()) if PAT.match(f.name)]
    assert len(runs) == 40
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
            'designs': sorted([{'x_bits': d['x_bits'], 'space': d.get('space')} for d in sel['designs'].values()], key=lambda d: d['x_bits']),
            'cells': [{k: c.get(k) for k in ('arm', 'kind', 'space', 'status', 'reason')} for c in sel['selection']['cells']]}, indent=0) + '\n')
    print(len(runs['runs']), 'runs;', len(ref['designs']), 'designs;', sum(len(d['checks']) for d in ref['designs'].values()), 'checks')
