"""Summarize direct final-refinement results against the study's common validation."""
import json, sys
from pathlib import Path

R = Path(sys.argv[1]); main = Path(sys.argv[2])
c = json.loads((R / 'bundle/cohort.json').read_text())
T = {'T50': 0.004801702073826702, 'T90': 0.0009603404147653405, 'T99': 9.603404147653405e-05}
FLOOR = 1.4444411337396714
def res(key, check):
    p = R / 'results' / key[:12] / check / 'result.json'
    if not p.exists(): return None
    return json.loads(p.read_text())
rows = []
for key, e in sorted(c['designs'].items(), key=lambda kv: kv[1]['roles']):
    pr = res(key, 'primary')
    src = next((s for s in e['sources'] if s), None)
    orig = None
    if src:
        orig = json.loads((main / src['run']).read_text())['validations'][src['validation']]
    line = {'key': key[:12], 'space': e['space'], 'roles': e['roles']}
    if pr and pr.get('ok'):
        F2 = pr['validation']['F']; m2 = pr['validation']['full_current_mean']
        line['F_primary'] = F2; line['mean_primary'] = m2
        if orig: line['repro_rel'] = abs(F2 - orig['F_val']) / orig['F_val']
        for ch in ('L3', 'g40', 'tight', 'shifted', 'conforming'):
            r = res(key, ch)
            if r is None: continue
            if not r.get('ok'): line[ch] = {'status': r.get('status'), 'error': (r.get('error') or '')[:80]}; continue
            v = r['validation']; line[ch] = {'F': v['F'], 'rel': v['F'] / F2 - 1, 'mean': v['full_current_mean'],
                                            'mean_ok': v['full_current_mean'] >= FLOOR}
        w = res(key, 'withheld')
        if w is not None: line['withheld'] = w.get('validation', {}).get('F') if w.get('ok') else w.get('status')
    else:
        line['primary'] = None if pr is None else pr.get('status')
    rows.append(line)
(R / 'report.json').write_text(json.dumps(rows, indent=1) + '\n')
f = lambda x: '     -' if x is None else f'{x:+6.1%}'
print(f'{"design":12s} sp {"F_primary/F0":>12s} {"repro":>8s} {"L3":>7s} {"g40":>7s} {"tight":>7s} {"shift":>7s} {"conf":>7s}  roles / target status under L3')
for l in rows:
    if 'F_primary' not in l:
        print(l['key'], l['space'], 'primary:', l.get('primary')); continue
    g = lambda ch: l[ch]['rel'] if isinstance(l.get(ch), dict) and 'rel' in l[ch] else None
    tags = []
    for role in l['roles']:
        tg = role.split('/')[-1]
        if tg in T and isinstance(l.get('L3'), dict) and 'F' in l['L3']:
            F3 = l['L3']['F']; tags.append(f'{tg}:L3 {"keeps" if F3 + max(.1*F3, 2.5e-7) <= T[tg] else "LOSES"}')
    bad_mean = [ch for ch in ('L3', 'g40', 'tight', 'shifted') if isinstance(l.get(ch), dict) and l[ch].get('mean_ok') is False]
    print(f'{l["key"]} {l["space"]}  {l["F_primary"]/9.603404147653405e-3:12.5f} {l.get("repro_rel", 0):8.1e} '
          + ' '.join(f(g(ch)).rjust(7) for ch in ('L3', 'g40', 'tight', 'shifted', 'conforming'))
          + '  ' + ','.join(l['roles']) + ' ' + ' '.join(tags) + (f' mean<floor:{bad_mean}' if bad_mean else ''))
