"""Read-only check of the matched-optimization claims (v7 onward); no FEM or optimizer run.

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
assert [round(x, 2) for x in (min(g('B', 'A', 'a')), max(g('B', 'A', 'a')))] == [3.10, 3.25]
assert [round(x, 2) for x in (min(g('B', 'A', 'b')), max(g('B', 'A', 'b')))] == [5.06, 7.79]
assert [round(x, 2) for x in (min(g('D', 'E', 'a')), max(g('D', 'E', 'a')))] == [2.45, 2.75]
assert [round(x, 2) for x in (min(g('D', 'E', 'b')), max(g('D', 'E', 'b')))] == [3.14, 7.20]
assert [round(x, 2) for x in (min(g('E', 'A', 'ab')), max(g('E', 'A', 'ab')))] == [.62, .79]
assert [round(x, 2) for x in (min(g('D', 'B', 'ab')), max(g('D', 'B', 'ab')))] == [.45, .56]
assert [round(x, 2) for x in (min(g('D', 'A', 'a')), max(g('D', 'A', 'a')))] == [1.68, 1.83]
assert [round(x, 2) for x in (min(g('D', 'A', 'b')), max(g('D', 'A', 'b')))] == [2.25, 4.32]
optg = g('B', 'A', 'ab') + g('D', 'E', 'ab'); assert round(min(optg), 2) == 2.45 and round(max(optg), 2) == 7.79
res = [1 / x for x in g('E', 'A', 'ab') + g('D', 'B', 'ab')]; assert round(min(res), 1) == 1.3 and round(max(res), 1) == 2.2
da = [x for sp, tg in CELLS for x in ratios('D', 'A', sp, tg)]
assert len(da) == 25 and sum(x <= 1 for x in da) == 1 and round(min(da), 2) == .99 and round(100 * (1 - min(da)), 2) == .75
# A attains every target from every start that any method reaches it from.
for sp, tg in CELLS:
    assert all(t('A', sp, s, tg) for s in S)
assert sum(bool(t('C', 'b', s, 'T99')) for s in S) == 1 and sum(bool(t('C', 'a', s, 'T90')) for s in S) == 2
assert t('E', 'b', 'lhs1', 'T90') is None and t('E', 'b', 'lhs1', 'T99') is None
best = lambda r: min([v['F_val'] for v in r['validations'] if credited(v)], default=None)
assert round(best(R[('E', 'b', 'lhs1', 1)]) / F0, 2) == .28
nocredit = sorted(r['file'] for r in runs if best(r) is None)
assert nocredit == ['slsqp_l0_a_lhs4_r1.json', 'slsqp_l0_a_lhs4_r2.json', 'surrogate_a_lhs4_r1.json', 'surrogate_a_lhs4_r2.json']
# Timing repeats: same path up to the shorter run; time differences relative to repeat 1.
for m in 'ABCDE':
    for sp in 'ab':
        for s in S:
            a1 = [v['F_val'] for v in R[(m, sp, s, 1)]['validations']]; a2 = [v['F_val'] for v in R[(m, sp, s, 2)]['validations']]
            n = min(len(a1), len(a2)); assert a1[:n] == a2[:n], (m, sp, s)
            if len(a1) != len(a2):
                assert {R[(m, sp, s, k)]['stop_reason'] for k in (1, 2)} <= {'budget', 'solver_stopped'}
d = [abs(t(m, sp, s, tg, 2) / t(m, sp, s, tg, 1) - 1) for m in 'ABCDE' for sp in 'ab' for s in S for tg in A['targets']
     if t(m, sp, s, tg, 1) and t(m, sp, s, tg, 2)]
assert len(d) == 111 and round(100 * st.median(d), 1) == 2.1 and round(100 * max(d), 1) == 12.4
assert sum(r['attainment'][tg]['attained'] for r in runs if r['repeat'] == 1 for tg in A['targets']) == 111
late = [(r['method'], r['space'], tg) for r in runs if r['repeat'] == 1 for tg in A['targets']
        if r['attainment'][tg]['attained'] and not r['checkpoint_60'][tg]['attained']]
assert sorted(late) == [('B', 'b', 'T99'), ('B', 'b', 'T99'), ('D', 'b', 'T99')]
import collections
assert collections.Counter(r['stop_reason'] for r in runs) == {'T99': 38, 'converged': 46, 'budget': 13, 'solver_stopped': 3}
ss = sorted((r['method'], r['space'], r['start'], r['repeat'], round(r['in_run_wall_seconds'] / r['unit_seconds'])) for r in runs if r['stop_reason'] == 'solver_stopped')
assert ss == [('A', 'a', 'lhs2', 1, 43), ('A', 'a', 'lhs2', 2, 43), ('A', 'a', 'lhs3', 1, 119)], ss
assert max(r['in_run_load_max'] for r in runs) < 22.5 and round(max(r['in_run_load_max'] for r in runs), 1) == 21.1
assert all(r['longest_overload_s'] == 0 for r in runs)
assert sum(1 for sp, tg in CELLS for s in S if t('D', sp, s, tg) and not t('E', sp, s, tg)) == 4
assert t('E', 'b', 'lhs1', 'T50')
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
print('PASS: Exp 2 inventory binding (100 traces), recomputed target credit, paired ratios, repeat noise, checkpoint and abstract numbers.')

# Final refinement (Section 7.2, Table 7).
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
assert len(moved) == 16 and max(moved) < 2.2e-12
assert len(fresh) == 15 and round(max(fresh), 4) == 1.7e-3
rel = lambda ch: [e['checks'][ch]['F'] / e['checks']['primary']['F'] - 1 for e in DS.values() if ch in e['checks']]
assert round(100 * min(rel('L3')), 1) == -4.0 and round(100 * max(rel('L3')), 1) == 6.3
dbest = next(e for e in DS.values() if 'representative/D/b' in e['roles'])
assert max(rel('L3')) == dbest['checks']['L3']['F'] / dbest['checks']['primary']['F'] - 1
assert round(dbest['checks']['primary']['F'] / F0, 4) == .0044 and round(dbest['checks']['L3']['F'] / F0, 4) == .0047
assert max(abs(x) for x in rel('g40') + rel('shifted')) < .0011 and round(100 * max(abs(x) for x in rel('shifted')), 2) == .11
assert max(abs(x) for x in rel('tight')) < 1.1e-5
conf_a = [x for e in cands if e['stator_offsets_zero'] for x in [e['checks']['conforming']['F'] / e['checks']['primary']['F'] - 1]]
assert all(e['space'] == 'a' for e in cands if e['stator_offsets_zero'])
assert round(100 * min(conf_a), 1) == 1.1 and round(100 * max(conf_a), 1) == 4.4
assert [round(100 * (theta[sp]['checks']['conforming']['F'] / F0 - 1), 1) for sp in 'ab'] == [2.1, 2.0]
cross = [e['checks']['L3']['F'] / e['checks']['primary']['F'] - 1 for e in DS.values() if any(r.startswith('crossing') for r in e['roles'])]
assert round(100 * min(cross), 1) == -4.0 and round(100 * max(cross), 1) == 1.1
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
reps = {e['roles'][0]: held(e) for e in cands if e['roles'][0].startswith('representative')}
assert round(min(reps[f'representative/{m}/a'] for m in 'ABCDE'), 2) == 1.07 and round(max(reps[f'representative/{m}/a'] for m in 'ABCDE'), 2) == 1.09
assert round(reps['representative/B/b'], 2) == .89 and round(min(reps[f'representative/{m}/b'] for m in 'ACDE'), 2) == 1.23 and round(max(reps[f'representative/{m}/b'] for m in 'ACDE'), 2) == 1.73
# Mean constraint active in space A: every method's best design within 0.17% of the floor.
for m in 'ABCDE':
    b = min((v for r in runs if r['method'] == m and r['space'] == 'a' for v in r['validations'] if credited(v)), key=lambda v: v['F_val'])
    assert 1 < b['mean_full'] / A['mean_floor'] < 1.0017
# Session setup records, bound to the inventory.
SU = D / 'provenance/setup'
for name in ('session.json', 'calibration_primary.json', 'anchor_preflight.json'):
    assert hashlib.sha256((SU / name).read_bytes()).hexdigest() == inventory['exp2-main/main/' + name], name
sess = json.loads((SU / 'session.json').read_text()); cal = json.loads((SU / 'calibration_primary.json').read_text())['calibration']
ap = json.loads((SU / 'anchor_preflight.json').read_text())
assert sess['procs'] == 15 and math.isclose(sess['unit_seconds'], 70.935572288) and len(cal['validation_seconds']) == 45
assert st.median(cal['validation_seconds']) == cal['unit_seconds'] == sess['unit_seconds']
assert ap['status'] == 'PASS' and ap['spaces_agree_bitwise'] is True and all(s['comparison']['status'] == 'PASS' for s in ap['sessions'])
assert max(s['comparison']['F_relative_error'] for s in ap['sessions']) < 2.5e-13
assert round(sum(r['in_run_wall_seconds'] for r in runs) / 3600) == 109
# Text-side bindings for the revised wording.
txt = (V / 'optimization.tex').read_text() + (V / 'main.tex').read_text()
for phrase in ['in all 45 paired comparisons', 'median of 2.1\\% and at most 12.4\\%', 'only three crossings occur',
               '$2.0$--$2.1\\%$ more at $\\theta_0$', 'by $-4.0\\%$ to $+1.1\\%$', 'below\n$2.2\\times10^{-12}$',
               'less\nthan $1.1\\times10^{-5}$', 'values above one mean a smaller', 'design it is 0.89', 'within 0.17\\% above',
               '38 stopped on reaching $T_{99}$, 46 on optimizer convergence', 'It was not run in this study',
               'by geometric means of 2.45--7.79', 'gets there 2.45--7.79 times sooner', 'C reaches $0.049F_0$',
               'is 1.23--1.73 times that at the training currents', 'D is 0.75\\% faster', 'the A, B, D and E level appears near $0.048$ (C\'s near $0.054$)',
               'wherever both reach a common validated target']:
    assert phrase in txt, phrase
plot = lambda ms: min(v['F_upper'] for r in runs if r['space'] == 'a' and r['repeat'] == 1 and r['method'] in ms for v in r['validations'] if credited(v)) / F0
assert round(plot('ABDE'), 3) == .048 and round(plot('C'), 3) == .054
assert 'near $0.043F_0$, so' not in txt and '45 paired starts' not in txt and '13.2' not in txt and 'tree/main' not in txt
print('PASS: Exp 2 final refinement (33 designs, 213 checks): reproduction, L3/g40/tight/shifted/conforming changes, target margins, mean floor, held-out currents.')
