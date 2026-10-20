"""Read-only check of the v7 matched-optimization claims; no FEM or optimizer run.

Binds the 100 per-run extracts to the study inventory taken before any host
reboot, recomputes target credit from the validation records, and checks the
numbers stated in optimization.tex and the abstract against the extract.
"""
from pathlib import Path
import hashlib, json, math, statistics as st

V = Path(__file__).resolve().parent; D = V / 'figures/src/data/exp2'
INV = D / 'provenance/exp2-main-inventory-0650f4ee.sha256.txt'
assert hashlib.sha256(INV.read_bytes()).hexdigest() == '0650f4ee779b64c8efc32020571657f3009f3fe46a52c8adc9f8ffdf28ea6e16'
inventory = dict(reversed(l.split('  ', 1)) for l in INV.read_text().splitlines())
assert len(inventory) == 5453
X = json.loads((D / 'runs.json').read_text()); RF = json.loads((D / 'refinement.json').read_text())
A = X['anchor']; F0 = A['F0']
assert math.isclose(F0, 9.603404147653405e-3, rel_tol=1e-15) and math.isclose(A['mean_floor'], 1.4444411337396714, rel_tol=1e-15)
runs = X['runs']; assert len(runs) == 100
for r in runs:
    assert inventory['exp2-main/main/' + r['file']] == r['sha256'], r['file']
    assert r['host_contaminated'] is False and r['budget_fe'] == 120.0
R = {(r['method'], r['space'], r['start'], r['repeat']): r for r in runs}
assert len(R) == 100
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']


def credited(v):
    return v['status'] == 'ok' and v['credited'] and v['admissible'] and v['mean_ok']


# Recompute attainment independently of the harness's attainment block.
for r in runs:
    for tg, T in A['targets'].items():
        hits = [v for v in r['validations'] if credited(v) and v['F_val'] + max(.1 * v['F_val'], 2.5e-7) <= T]
        a = r['attainment'][tg]
        assert bool(hits) == bool(a['attained']), (r['file'], tg)
        if hits:
            assert hits[0]['index'] == a['validation'] and hits[0]['wall_fe'] == a['wall_fe']
            assert math.isclose(hits[0]['F_upper'], hits[0]['F_val'] + max(.1 * hits[0]['F_val'], 2.5e-7), rel_tol=1e-12)


def t(m, sp, s, tg, rep=1):
    a = R[(m, sp, s, rep)]['attainment'][tg]
    return a['wall_fe'] if a['attained'] else None


def gm(xs):
    return math.exp(sum(map(math.log, xs)) / len(xs))


def ratios(n, d, sp, tg):
    return [t(n, sp, s, tg) / t(d, sp, s, tg) for s in S if t(n, sp, s, tg) and t(d, sp, s, tg)]


CELLS = [('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')]
assert not any(t(m, 'a', s, 'T99', rep) for m in 'ABCDE' for s in S for rep in (1, 2))
opt = [x for sp, tg in CELLS for n, d in (('B', 'A'), ('D', 'E')) for x in ratios(n, d, sp, tg)]
assert len(opt) == 45 and min(opt) > 1 and round(min(opt), 2) == 1.46 and round(max(opt), 1) == 21.5
g = lambda n, d, sps: [gm(ratios(n, d, sp, tg)) for sp, tg in CELLS if sp in sps]
assert [round(x, 1) for x in (min(g('B', 'A', 'a')), max(g('B', 'A', 'a')))] == [3.1, 3.3]
assert [round(x, 1) for x in (min(g('B', 'A', 'b')), max(g('B', 'A', 'b')))] == [5.1, 7.8]
assert [round(x, 1) for x in (min(g('D', 'E', 'a')), max(g('D', 'E', 'a')))] == [2.4, 2.8]
assert [round(x, 1) for x in (min(g('D', 'E', 'b')), max(g('D', 'E', 'b')))] == [3.1, 7.2]
assert [round(x, 2) for x in (min(g('E', 'A', 'ab')), max(g('E', 'A', 'ab')))] == [.62, .79]
assert [round(x, 2) for x in (min(g('D', 'B', 'ab')), max(g('D', 'B', 'ab')))] == [.45, .56]
assert [round(x, 2) for x in (min(g('D', 'A', 'a')), max(g('D', 'A', 'a')))] == [1.68, 1.83]
assert [round(x, 2) for x in (min(g('D', 'A', 'b')), max(g('D', 'A', 'b')))] == [2.25, 4.32]
optg = g('B', 'A', 'ab') + g('D', 'E', 'ab'); assert round(min(optg), 1) == 2.4 and round(max(optg), 1) == 7.8
res = [1 / x for x in g('E', 'A', 'ab') + g('D', 'B', 'ab')]; assert round(min(res), 1) == 1.3 and round(max(res), 1) == 2.2
da = [x for sp, tg in CELLS for x in ratios('D', 'A', sp, tg)]
assert len(da) == 25 and sum(x <= 1 for x in da) == 1 and round(min(da), 2) == .99
# A attains every target from every start that any method reaches it from.
for sp, tg in CELLS:
    assert all(t('A', sp, s, tg) for s in S)
assert sum(bool(t('C', 'b', s, 'T99')) for s in S) == 1 and sum(bool(t('C', 'a', s, 'T90')) for s in S) == 2
assert t('E', 'b', 'lhs1', 'T90') is None and t('E', 'b', 'lhs1', 'T99') is None
best = lambda r: min([v['F_val'] for v in r['validations'] if credited(v)], default=None)
assert round(best(R[('E', 'b', 'lhs1', 1)]) / F0, 2) == .28
nocredit = sorted(r['file'] for r in runs if best(r) is None)
assert nocredit == ['slsqp_l0_a_lhs4_r1.json', 'slsqp_l0_a_lhs4_r2.json', 'surrogate_a_lhs4_r1.json', 'surrogate_a_lhs4_r2.json']
# Timing repeats.
d = [abs(math.log(t(m, sp, s, tg, 2) / t(m, sp, s, tg, 1))) for m in 'ABCDE' for sp in 'ab' for s in S for tg in A['targets']
     if t(m, sp, s, tg, 1) and t(m, sp, s, tg, 2)]
assert len(d) == 111 and round(100 * st.median(d), 1) == 2.1 and round(100 * max(d), 1) == 13.2
late = [(r['file'], tg) for r in runs for tg in A['targets'] if r['attainment'][tg]['attained'] and not r['checkpoint_60'][tg]['attained']]
assert len(late) == 6 and all(tg == 'T99' and r.split('_')[0] == 'cobyqa' and '_b_' in r for r, tg in late)
assert round(sum(r['in_run_wall_seconds'] for r in runs) / 3600) == 109
assert all(math.isclose(r['unit_seconds'], 70.935572288) for r in runs)
# Space-A plateau and space-B range of best designs.
for m in 'ABCDE':
    for s in S:
        b = best(R[(m, 'a', s, 1)])
        assert b is None or b / F0 > .043
assert round(min(best(R[(m, 'a', s, 1)]) / F0 for m in 'ABDE' for s in S if best(R[(m, 'a', s, 1)])), 3) == .043
bb = [best(R[(m, 'b', s, 1)]) / F0 for m in 'ABDE' for s in S]
assert round(min(bb), 3) == .004
rep = lambda m, sp: min(best(r) for r in runs if r['method'] == m and r['space'] == sp and best(r)) / F0
assert all(round(rep(m, 'a'), 3) == .043 for m in 'ABDE') and round(rep('C', 'a'), 3) == .049
assert all(.004 <= round(rep(m, 'b'), 3) <= .009 for m in 'ABCDE')
assert sum(bool(t(m, 'b', s, 'T99')) for s in S for m in 'A') == 5 and all(sum(bool(t(m, 'b', s, 'T99')) for s in S) >= 4 for m in 'ABDE')
assert round(best(R[('A', 'a', 'lhs4', 1)]) / F0, 2) == .09 and round(best(R[('B', 'a', 'lhs4', 1)]) / F0, 2) == .09 and round(best(R[('D', 'a', 'lhs4', 1)]) / F0, 3) == .044
print('PASS: v7 Exp 2 inventory binding (100 traces), recomputed target credit, paired ratios, repeat noise, checkpoint and abstract numbers.')

# Final refinement (Section 8.3 and Table 6).
DS = RF['designs']; assert len(DS) == 33
assert sum(len(e['checks']) for e in DS.values()) == 213 and all(q['ok'] and q['status'] == 'complete' for e in DS.values() for q in e['checks'].values())
assert sum(e['stator_offsets_zero'] for e in DS.values()) == 15 and all(('conforming' in e['checks']) == e['stator_offsets_zero'] for e in DS.values())
for e in DS.values():
    assert set(e['checks']) - {'conforming'} == {'primary', 'L3', 'g40', 'tight', 'shifted', 'withheld'}
theta = {e['space']: e for e in DS.values() if e['roles'][0].startswith('theta0')}
for sp, e in theta.items():
    assert math.isclose(e['checks']['primary']['F'], F0, rel_tol=1e-12)
cands = [e for e in DS.values() if not e['roles'][0].startswith('theta0')]
assert len(cands) == 31
byfile = {r['file']: r for r in runs}
moved, fresh = [], []
for e in cands:
    src = next(s for s in e['sources'] if s); v = byfile[src['run']]['validations'][src['validation']]
    (moved if v['mesh_kind'] == 'moved' else fresh).append(abs(e['checks']['primary']['F'] / v['F_val'] - 1))
assert len(moved) == 16 and max(moved) < 2.2e-12 and round(max(moved), 13) == 2.1e-12
assert len(fresh) == 15 and round(max(fresh), 4) == 1.7e-3
rel = lambda ch: [e['checks'][ch]['F'] / e['checks']['primary']['F'] - 1 for e in DS.values() if ch in e['checks']]
assert round(100 * min(rel('L3')), 1) == -4.0 and round(100 * max(rel('L3')), 1) == 6.3
dbest = next(e for e in DS.values() if 'representative/D/b' in e['roles'])
assert max(rel('L3')) == dbest['checks']['L3']['F'] / dbest['checks']['primary']['F'] - 1
assert round(dbest['checks']['primary']['F'] / F0, 4) == .0044 and round(dbest['checks']['L3']['F'] / F0, 4) == .0047
assert max(abs(x) for x in rel('g40') + rel('shifted')) < .0011 and round(100 * max(abs(x) for x in rel('shifted')), 2) == .11
assert max(abs(x) for x in rel('tight')) < 1.05e-5
conf_a = [x for e in cands if e['stator_offsets_zero'] for x in [e['checks']['conforming']['F'] / e['checks']['primary']['F'] - 1]]
assert all(e['space'] == 'a' for e in cands if e['stator_offsets_zero'])
assert round(100 * min(conf_a), 1) == 1.1 and round(100 * max(conf_a), 1) == 4.4
assert [round(100 * (theta[sp]['checks']['conforming']['F'] / F0 - 1), 1) for sp in 'ab'] == [2.1, 2.0]
margins = []
for e in DS.values():
    for role in e['roles']:
        tg = role.split('/')[-1]
        if tg in A['targets']:
            F3 = e['checks']['L3']['F']; margins.append((A['targets'][tg] - F3 - max(.1 * F3, 2.5e-7)) / A['targets'][tg])
assert len(margins) == 25 and min(margins) > 0 and round(100 * min(margins), 1) == 1.2
mu = min(e['checks'][ch]['full_current_mean'] for e in cands for ch in ('primary', 'L3', 'g40', 'tight', 'shifted')) / A['mean_floor']
assert mu > 1 and round(100 * (mu - 1), 3) == .012
held = lambda e: (e['checks']['withheld']['F'] / theta[e['space']]['checks']['withheld']['F']) / (e['checks']['primary']['F'] / F0)
assert round(min(map(held, cands)), 2) == .65 and round(max(map(held, cands)), 2) == 1.73
assert all(held(e) <= 1.10 for e in cands if e['space'] == 'a' and e['roles'][0].startswith('representative'))
assert round(dbest['checks']['withheld']['F'] / theta['b']['checks']['withheld']['F'], 4) == .0076
print('PASS: v7 final refinement (33 designs, 213 checks): reproduction, L3/g40/tight/shifted/conforming changes, target margins, mean floor, held-out currents.')
