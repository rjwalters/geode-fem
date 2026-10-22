"""Read-only check of the post-hoc FD-SLSQP ablation claims (method F and its control A'), v11 onward.

No FEM or optimizer run. Binds the 40 per-run extracts and the session setup
records to the ablation inventory (taken on the host before it stopped), recomputes
target credit, the paired F/A' ratios, hit fractions, path comparison, validation
charges, the cross-session decomposition against the main study, the final
refinement checks, the L0 finite-difference accuracy record and the protocol
timing, and checks the stated numbers against the manuscript text.

When the raw ablation session and refinement directory are available, set
EXP2B_SESSION=<copy>/exp2-fd-ablation/session and EXP2B_REFINE=<refine-dir> (and
optionally EXP2B_SELECTION=<selection.json>): the extracts are then re-derived from
the raw traces and results with extract_exp2b.extract() and must match exactly.
"""
from pathlib import Path
import hashlib, json, math, os, statistics as st, sys

V = Path(__file__).resolve().parent; D = V / 'figures/src/data/exp2b'; D2 = V / 'figures/src/data/exp2'
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
INV = D / 'provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt'
assert sha(INV) == 'c2bc65ff61fb8485d14a1e2b07dadb3f9c9fa4b040ce98aa1360f46f64788a83'
inventory = dict(reversed(l.split('  ', 1)) for l in INV.read_text().splitlines()); assert len(inventory) == 2118
X = json.loads((D / 'runs.json').read_text())['runs']; RF = json.loads((D / 'refinement.json').read_text())
M = json.loads((D2 / 'runs.json').read_text()); F0 = M['anchor']['F0']; TG = M['anchor']['targets']; FLOOR = M['anchor']['mean_floor']

# Optional re-derivation from the raw traces and refinement results.
raw = os.environ.get('EXP2B_SESSION'), os.environ.get('EXP2B_REFINE')
if all(raw):
    sys.path.insert(0, str(V / 'figures/src')); import extract_exp2b
    runs2, ref2 = extract_exp2b.extract(*raw)
    assert runs2 == json.loads((D / 'runs.json').read_text()) and ref2 == RF, 'extract differs from raw re-derivation'
    for r in X:
        assert sha(Path(raw[0]) / r['file']) == r['sha256']
    if os.environ.get('EXP2B_SELECTION'):
        sp = Path(os.environ['EXP2B_SELECTION']); assert sha(sp) == json.loads((D / 'selection-check.json').read_text())['selection_sha256']
    print('PASS: Exp 2b extracts re-derived from the 40 raw traces and the 90 refinement results (exact match).')
else:
    print('NOTE: raw ablation traces not supplied (EXP2B_SESSION/EXP2B_REFINE); checking the hash-bound extracts only.')

# Inventory binding: traces and setup records.
assert len(X) == 40
for r in X:
    assert inventory['exp2-fd-ablation/session/' + r['file']] == r['sha256'], r['file']
SU = D / 'provenance/setup'
for n in ('session.json', 'calibration_primary.json', 'anchor_preflight.json'):
    assert sha(SU / n) == inventory['exp2-fd-ablation/session/' + n], n
sess = json.loads((SU / 'session.json').read_text()); cal = json.loads((SU / 'calibration_primary.json').read_text())['calibration']
ap = json.loads((SU / 'anchor_preflight.json').read_text())
U = 66.658590752
assert sess['label'] == 'fd_ablation' and sess['procs'] == 15 and math.isclose(sess['unit_seconds'], U) and sess['budget_fe'] == 120.0
assert len(cal['validation_seconds']) == 45 and st.median(cal['validation_seconds']) == cal['unit_seconds'] == sess['unit_seconds']
assert ap['status'] == 'PASS' and ap['spaces_agree_bitwise'] is True and max(s['comparison']['F_relative_error'] for s in ap['sessions']) < 2.5e-13
assert all(math.isclose(r['unit_seconds'], U) and r['budget_fe'] == 120.0 and r['host_contaminated'] is False for r in X)
assert round(max(r['in_run_load_max'] for r in X), 1) == 19.7 and max(r['in_run_load_max'] for r in X) < 22.5
R = {(r['method'], r['space'], r['start'], r['repeat']): r for r in X}; assert len(R) == 40
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']
assert {(m, sp, s, k) for m in ('Ap', 'F') for sp in 'ab' for s in S for k in (1, 2)} == set(R)
for r in X:
    spec = r['method_spec']
    if r['method'] == 'F':
        assert spec['letter'] == 'F' and spec['gradient'] == 'forward_difference' and r['fd_step'] == 1e-6
    else:
        assert spec['letter'] == 'A' and r['fd_step'] is None and r['fd_gradients'] == []


def credited(v):
    return v['status'] == 'ok' and v['credited'] and v['admissible'] and v['mean_ok']


# Target credit recomputed from the validation records (same rule as the main study).
for r in X:
    for tg, T in TG.items():
        hits = [v for v in r['validations'] if credited(v) and v['F_val'] + max(.1 * v['F_val'], 2.5e-7) <= T]
        a = r['attainment'][tg]; assert bool(hits) == bool(a['attained']), (r['file'], tg)
        if hits:
            assert hits[0]['index'] == a['validation'] and hits[0]['wall_fe'] == a['wall_fe']
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
t = lambda m, sp, s, tg, k=1: R[(m, sp, s, k)]['attainment'][tg]['wall_fe'] if R[(m, sp, s, k)]['attainment'][tg]['attained'] else None


def fa(sp, tg, k=1):
    return [t('F', sp, s, tg, k) / t('Ap', sp, s, tg, k) for s in S if t('F', sp, s, tg, k) and t('Ap', sp, s, tg, k)]


# Hit fractions: both arms attain every target from every start, except T99 in space A, which neither reaches.
for m in ('Ap', 'F'):
    for k in (1, 2):
        assert all(t(m, sp, s, tg, k) for sp, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')) for s in S)
        assert not any(t(m, 'a', s, 'T99', k) for s in S)
fmt = lambda xs: (round(gm(xs), 2), round(min(xs), 2), round(max(xs), 2))
assert len(fa('a', 'T90')) == len(fa('b', 'T99')) == 5
assert fmt(fa('a', 'T90')) == (2.47, 2.45, 2.52) and fmt(fa('b', 'T99')) == (4.11, 3.84, 4.33)
assert fmt(fa('a', 'T90', 2)) == (2.49, 2.39, 2.58) and fmt(fa("b", "T99", 2)) == (4.04, 3.75, 4.37)
assert min(fa(sp, tg, k) [i] for sp, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')) for k in (1, 2) for i in range(5)) > 2.3
assert [round(gm(fa(*c)), 2) for c in (('a', 'T50'), ('b', 'T50'), ('b', 'T90'))] == [2.43, 3.67, 3.86]


# Validation charges: crossings at the same validation count, non-validation ratios and shares.
def cross(m, sp, s, tg, k=1):
    r = R[(m, sp, s, k)]; a = r['attainment'][tg]; c = r['validation_charges_seconds']
    assert math.isclose(sum(c), r['category_seconds']['validation'], rel_tol=1e-9, abs_tol=1e-9)
    assert math.isclose(sum(r['category_seconds'].values()), r['in_run_wall_seconds'], rel_tol=1e-9)
    v = sum(c[:a['validation'] + 1]) / r['unit_seconds']
    return {'n': a['validation'] + 1, 't': a['wall_fe'], 'v': v, 'rest': a['wall_fe'] - v}


for sp, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')):
    assert all(cross('F', sp, s, tg)['n'] == cross('Ap', sp, s, tg)['n'] for s in S)
nv = lambda sp, tg: [cross('F', sp, s, tg)['rest'] / cross('Ap', sp, s, tg)['rest'] for s in S]
assert fmt(nv('a', 'T90')) == (5.58, 5.02, 6.17) and fmt(nv('b', 'T99')) == (10.07, 9.35, 10.55)
share = lambda m, sp, tg: round(st.median(cross(m, sp, s, tg)['v'] / cross(m, sp, s, tg)['t'] for s in S), 2)
assert (share('F', 'a', 'T90'), share('Ap', 'a', 'T90'), share('F', 'b', 'T99'), share('Ap', 'b', 'T99')) == (.27, .69, .16, .66)
step = lambda m, sp: round(st.median(sum(r['category_seconds'][c] for c in ('forward', 'line_search', 'adjoint')) / r['unit_seconds'] / r['train_events']
                                      for r in X if r['method'] == m and r['space'] == sp), 2)
assert (step('F', 'a'), step('F', 'b'), step('Ap', 'a'), step('Ap', 'b')) == (1.53, 3.32, .29, .31)
# Finite-difference gradients: n differences each, no stale-base re-solve, no refusal.
for r in X:
    if r['method'] == 'F':
        n = 8 if r['space'] == 'a' else 18; G = r['fd_gradients']
        assert not any(g['base_resolved'] or g['refused'] for g in G)
        assert r['fd_train_events'] == sum(g['evaluated'] for g in G)
        done = [g for g in G if g['evaluated'] == g['components'] == n]
        assert len(done) >= len(G) - 1 and (len(done) == len(G) or (r['stop_reason'] == 'budget' and G[-1] not in done))
fdx = [g for r in X if r['method'] == 'F' for g in r['fd_gradients']]
complete = [g for g in fdx if g['evaluated'] == g['components'] and g['components'] in (8, 18)]
assert (len(fdx), len(complete)) == (446, 444)
back = sum(g['backward'] for g in complete) / sum(g['components'] for g in complete); assert round(100 * back) == 8
# Space-B path: same validation counts at every start; training-evaluation counts at four of five.
pathb = [(len(R[('F', 'b', s, 1)]['validations']) == len(R[('Ap', 'b', s, 1)]['validations']),
          R[('F', 'b', s, 1)]['train_events'] == R[('Ap', 'b', s, 1)]['train_events']) for s in S]
assert all(a for a, _ in pathb) and sum(b for _, b in pathb) == 4
dv = [max(abs(x['F_val'] / y['F_val'] - 1) for x, y in zip(R[('F', 'b', s, 1)]['validations'], R[('Ap', 'b', s, 1)]['validations'])) for s in S]
assert sum(x < .01 for x in dv) == 3 and round(max(dv), 2) == .29
# A' against the main-study A at the same start and repeat (each session's own unit).
MA = {(r['method'], r['space'], r['start'], r['repeat']): r for r in M['runs']}
ma = lambda sp, s, tg, k: MA[('A', sp, s, k)]['attainment'][tg]['wall_fe']
apa = [t('Ap', sp, s, tg) / ma(sp, s, tg, 1) for sp, tg in (('a', 'T90'), ('b', 'T99')) for s in S]
assert (round(min(apa), 2), round(max(apa), 2)) == (.94, 1.04)
assert round(M['runs'][0]['unit_seconds'], 2) == 70.94 and round(U, 2) == 66.66
# Cross-session decomposition against the main-study B/A (approximate).
mb = lambda sp, tg: [MA[('B', sp, s, 1)]['attainment'][tg]['wall_fe'] / ma(sp, s, tg, 1) for s in S if MA[('B', sp, s, 1)]['attainment'][tg]['attained']]
ba = {sp: gm(mb(sp, tg)) for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert (round(ba['a'], 2), round(ba['b'], 2)) == (3.10, 7.79)
bf = {sp: gm([MA[('B', sp, s, 1)]['attainment'][tg]['wall_fe'] / t('F', sp, s, tg) for s in S if MA[('B', sp, s, 1)]['attainment'][tg]['attained']])
      for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert (round(bf['a'], 2), round(bf['b'], 2)) == (1.22, 1.91)
u0 = M['runs'][0]['unit_seconds']
assert (round(bf['a'] * u0 / U, 1), round(bf['b'] * u0 / U, 1)) == (1.3, 2.0)
# v12: per-start optimizer factor B/F on matched starts, in units and seconds; chain closure.
mstart = {sp: [s_ for s_ in S if MA[('B', sp, s_, 1)]['attainment'][tg]['attained']] for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert mstart['a'] == S and mstart['b'] == ['theta0', 'lhs1', 'lhs2', 'lhs4']
bfs = {sp: [MA[('B', sp, s_, 1)]['attainment'][tg]['wall_fe'] / t('F', sp, s_, tg) for s_ in mstart[sp]] for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert [round(x, 2) for x in bfs['a']] == [1.24, 1.45, .70, .95, 2.27] and [round(x, 2) for x in bfs['b']] == [3.84, 2.46, 1.26, 1.12]
assert [round(x * u0 / U, 2) for x in bfs['a'] if x < 1] == [.75, 1.01] or [round(x * u0 / U, 2) for x in sorted(bfs['a'])[:2]] == [.75, 1.01]
assert round(u0 / U - 1, 2) == .06
fam = {sp: gm([t('F', sp, s_, tg) / t('Ap', sp, s_, tg) for s_ in mstart[sp]]) for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert (round(fam['a'], 2), round(fam['b'], 2)) == (2.47, 4.10)
assert (round(fam['a'] * bf['a'], 2), round(fam['b'] * bf['b'], 2)) == (3.02, 7.82)
assert round(min(bfs['a']), 2) == .70 and round(max(bfs['a']), 2) == 2.27 and round(bf['a'], 1) == 1.2 and round(bf['b'], 1) == 1.9
assert all(x > 1 for x in bfs['b']) and (round(min(bfs['b']), 1), round(max(bfs['b']), 1)) == (1.1, 3.8)
# v12: stop reasons, best validated objectives, per-step and per-solve costs, all-target validation shares.
import collections as _c
sr = _c.Counter((r['method'], r['space'], r['stop_reason']) for r in X if r['repeat'] == 1)
sr2 = _c.Counter((r['method'], r['space'], r['stop_reason']) for r in X)
assert sr2[('F', 'a', 'budget')] == 8 and sr2[('F', 'a', 'converged')] == 2
assert (sr2[('Ap', 'a', 'converged')], sr2[('Ap', 'a', 'solver_stopped')], sr2[('Ap', 'a', 'budget')]) == (4, 4, 2)
assert sr2[('F', 'b', 'T99')] == sr2[('Ap', 'b', 'T99')] == 10
bestv = lambda r: min(v['F_val'] for v in r['validations'] if credited(v))
assert max(abs(bestv(R[('F', 'a', s_, 1)]) / bestv(R[('Ap', 'a', s_, 1)]) - 1) for s_ in S) < .004
stepm = lambda m, sp: st.median(sum(r['category_seconds'][c] for c in ('forward', 'line_search', 'adjoint')) / r['unit_seconds'] / r['train_events']
                               for r in X if r['method'] == m and r['space'] == sp)
assert (round(stepm('F', 'a') / stepm('Ap', 'a'), 1), round(stepm('F', 'b') / stepm('Ap', 'b'), 1)) == (5.3, 10.9)
solve = [r['category_seconds']['forward'] / r['unit_seconds'] / (r['train_events'] + r['fd_train_events']) for r in X if r['method'] == 'F']
adjs = [r['category_seconds']['adjoint'] / r['unit_seconds'] / r['train_events'] for r in X if r['method'] == 'Ap']
assert round(st.median(solve), 2) == .23 and round(st.median(adjs), 2) == .06
alls = lambda m, sp: round(st.median(cross(m, sp, s_, tg)['v'] / cross(m, sp, s_, tg)['t'] for s_ in S for q, tg in (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99')) if q == sp), 2)
assert (alls('Ap', 'a'), alls('Ap', 'b'), alls('F', 'a'), alls('F', 'b')) == (.65, .61, .27, .16)
# v12: Fig. exp2b-per-start exists and its plotted minutes follow from the extracts.
assert (V / 'figures/exp2b-per-start.pdf').is_file() and 'figures/exp2b-per-start.pdf' in (V / 'optimization.tex').read_text()
assert MA[('B', 'b', 'lhs3', 1)]['attainment']['T99']['attained'] is False
print('PASS: Exp 2b inventory binding (40 traces, setup records), target credit, F/A\' ratios and repeats, hit fractions, validation dilution, path, A\'-vs-A and cross-session decomposition.')

# Final refinement of the ablation's selected designs.
DS = RF['designs']; assert len(DS) == 14 and sum(len(d['checks']) for d in DS.values()) == 90
assert all(c['ok'] and c['status'] == 'complete' for d in DS.values() for c in d['checks'].values())
assert sha(D / 'refinement-tools/cohort.json') == RF['cohort_sha256']
coh = json.loads((D / 'refinement-tools/cohort.json').read_text())
assert {k[:12] for k in coh['designs']} == {k[:12] for k in DS} and all(DS[k]['x_bits'] == coh['designs'][k]['x_bits'] for k in DS)
sel = json.loads((D / 'selection-check.json').read_text())
assert sorted(d['x_bits'] for d in sel['designs']) == sorted(d['x_bits'] for d in DS.values()) and sel['source_commit'].startswith('5cc86ad8')
cands = [d for d in DS.values() if not d['roles'][0].startswith('theta0')]; assert len(cands) == 12
byf = {r['file']: r for r in X}
rep = [abs(d['checks']['primary']['F'] / byf[s['run']]['validations'][s['validation']]['F_val'] - 1) for d in cands for s in d['sources'][:1]]
crossd = [d for d in cands if any(r.startswith('crossing') for r in d['roles'])]
assert len(crossd) == 8 and sum(r.startswith('crossing') for d in crossd for r in d['roles']) == 10
assert f'{max(rep):.1e}' == '6.3e-04' and max(abs(d['checks']['primary']['F'] / byf[d['sources'][0]['run']]['validations'][d['sources'][0]['validation']]['F_val'] - 1) for d in crossd) < 5e-6
rel = lambda d, c: d['checks'][c]['F'] / d['checks']['primary']['F'] - 1
assert (round(100 * min(rel(d, 'L3') for d in cands), 1), round(100 * max(rel(d, 'L3') for d in cands), 1)) == (-2.9, 2.6)
assert (round(100 * min(rel(d, 'L3') for d in crossd), 1), round(100 * max(rel(d, 'L3') for d in crossd), 1)) == (-2.9, 1.1)
assert max(abs(rel(d, c)) for d in cands for c in ('g40', 'tight', 'shifted')) < 3.0e-4
conf = [rel(d, 'conforming') for d in cands if 'conforming' in d['checks']]
assert all(d['space'] == 'a' for d in cands if 'conforming' in d['checks']) and (round(100 * min(conf), 1), round(100 * max(conf), 1)) == (1.1, 4.4)
marg = [(TG[r.split('/')[-1]] - d['checks']['L3']['F'] - max(.1 * d['checks']['L3']['F'], 2.5e-7)) / TG[r.split('/')[-1]] for d in crossd for r in d['roles']]
assert len(marg) == 10 and round(100 * min(marg)) == 15
mu = min(d['checks'][c]['full_current_mean'] for d in cands for c in ('primary', 'L3', 'g40', 'tight', 'shifted')) / FLOOR
assert mu > 1 and round(100 * (mu - 1), 3) == .044
print('PASS: Exp 2b final refinement (14 designs, 90 checks), reproduction, L3/g40/tight/shifted/conforming changes, target margins, mean floor, selection identity.')

# L0 finite-difference accuracy record of method F (pinned on main).
man = {f['snapshot']: f for f in json.loads((V / 'evidence-manifest.json').read_text())['files']}
e = man['figures/src/data/exp2b/fd-gradient-runtime-checks.json']
assert e['source_commit'] == '2d4fbcfe8479c0df30e22323d987d885a671c1fb' and sha(V / e['snapshot']) == e['sha256']
fdc = json.loads((V / e['snapshot']).read_text()); assert fdc['status'] == 'PASS' and fdc['norm_tol'] == 5e-3 and fdc['component_tol'] == .1
pts = [(r, p) for r in fdc['records'] for p in r['points']]
assert len(pts) == 4 and all(r['level'] == 0 and r['step'] == 1e-6 for r, _ in pts)
for r, p in pts:
    comp = p['components']; f = [c['fd_dF'] for c in comp]; a = [c['adjoint_dF'] for c in comp]
    nrm = max(abs(x - y) for x, y in zip(f, a)) / max(abs(y) for y in a)  # max-norm relative error, as in fd_gradient_check.py
    assert math.isclose(nrm, p['normwise_rel_dF'], rel_tol=1e-9) and nrm <= 5e-3 and max(c['rel_dF'] for c in comp) <= .1
nd = [p['normwise_rel_dF'] for _, p in pts]; nm = [p['normwise_rel_dmean_full'] for _, p in pts]
assert (f'{min(nd):.1e}', f'{max(nd):.1e}', f'{min(nm):.1e}', f'{max(nm):.1e}') == ('5.1e-04', '1.9e-03', '1.0e-04', '6.7e-04')
worst = max(((c['rel_dF'], c['name'], r['space'], p['point']) for r, p in pts for c in p['components']))
assert (round(worst[0], 3), worst[1], worst[2], worst[3]) == (.057, 'SW3', 'b', 'interior')
# Protocol timing: the frozen protocol and its freeze comment predate the first ablation run.
pe = json.loads((D / 'provenance/protocol-1218-export.json').read_text())
assert hashlib.sha256(pe['issue']['body'].encode()).hexdigest() == pe['issue']['body_sha256'] and pe['issue']['graphql_userContentEdits_totalCount'] == 0
assert hashlib.sha256(pe['freeze_comment']['body'].encode()).hexdigest() == pe['freeze_comment']['body_sha256']
assert pe['freeze_comment']['created_at'] == pe['freeze_comment']['updated_at'] == '2026-10-20T23:00:32Z' and pe['issue']['created_at'] == '2026-10-20T22:34:54Z'
assert sess['session_id'] == 'fd_ablation-20261021T020705Z' and pe['freeze_comment']['created_at'] < '2026-10-21T02:07:05Z'
print('PASS: Exp 2b L0 finite-difference accuracy (4 points, recomputed norms) and protocol timing.')
# L1 finite-difference accuracy record of method F (PR merge 74843242), recomputed from its raw FD/adjoint values.
e1 = man['figures/src/data/exp2b/fd-gradient-runtime-checks-L1.json']
assert e1['source_commit'] == '74843242db03e8d57edf971420171064bf19f107' and sha(V / e1['snapshot']) == e1['sha256']
L1 = json.loads((V / e1['snapshot']).read_text()); assert L1['status'] == 'FAIL' and L1['component_tol'] == .1 and L1['norm_tol'] == 5e-3
neg = L1['negligible']; nf, nm, fails, sw3 = {}, {}, [], {}
for r in L1['records']:
    tr, va = r['session']['training'], r['session']['validation']
    assert tr['level'] == 1 and va['level'] == 2 and tr['newton_rel_tol'] == 1e-8 and r['step'] == 1e-6
    for p in r['points']:
        comp = p['components']
        for key, out in (('dF', nf), ('dmean_full', nm)):
            f = [c['fd_' + key] for c in comp]; a = [c['adjoint_' + key] for c in comp]; big = max(abs(y) for y in a)
            out[(r['space'], p['point'])] = max(abs(x - y) for x, y in zip(f, a)) / big
            assert math.isclose(out[(r['space'], p['point'])], p['normwise_rel_' + key], rel_tol=1e-9)
            for c in comp:
                y = c['adjoint_' + key]; rel = abs(c['fd_' + key] - y) / abs(y) if y != 0 and abs(y) >= neg * big else None
                assert (rel is None and c['rel_' + key] is None) or math.isclose(rel, c['rel_' + key], rel_tol=1e-9)
                if rel is not None and rel > .1:
                    fails.append((r['space'], p['point'], c['name'], key, rel, c['fd_' + key], y, abs(y) / big))
                if c['name'] == 'SW3' and key == 'dmean_full':
                    sw3[(r['space'], p['point'])] = abs(c['fd_' + key] - y)
        assert p['pass'] == (nf[(r['space'], p['point'])] <= 5e-3 and nm[(r['space'], p['point'])] <= 5e-3 and not any(x[:2] == (r['space'], p['point']) for x in fails))
assert max(nf.values()) <= 5e-3 and max(nm.values()) <= 5e-3
assert (f'{min(nf.values()):.1e}', f'{max(nf.values()):.1e}', f'{max(nm.values()):.1e}') == ('5.2e-04', '1.9e-03', '7.2e-04')
assert [round(nf[k], 5) for k in (('a', 'theta0'), ('a', 'interior'), ('b', 'theta0'), ('b', 'interior'))] == [.00054, .00052, .00193, .00183]
assert len(fails) == 1 and fails[0][:4] == ('a', 'interior', 'SW3', 'dmean_full')
_, _, _, _, rel, fdv, adv, frac = fails[0]
assert (f'{fdv:.2e}', f'{adv:.2e}', round(rel, 2), f'{frac:.1e}') == ('-9.62e-03', '-7.74e-03', .24, '8.7e-05')
others = [v_ for k, v_ in sw3.items() if k != ('a', 'interior')]
assert f"{sw3[('a', 'interior')]:.1e}" == '1.9e-03' and (f'{min(others):.1e}', f'{max(others):.1e}') == ('1.8e-03', '1.9e-03')
print('PASS: Exp 2b L1 finite-difference accuracy (4 points; norm-wise and component errors recomputed from raw FD/adjoint values; one component-wise FAIL, SW3 mean at the space-A interior).')


# Tables exp2b-*, recomputed independently of render_exp2b.py.
CELLS = (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99'))
rt = (V / 'tables/exp2b-ratios.tex').read_text()
for sp, tg in CELLS:
    r1 = fa(sp, tg); r2 = fa(sp, tg, 2)
    assert (' & '.join(f'{x:.2f}' for x in r1) + f' & {gm(r1):.2f} ({gm(r2):.2f})') in rt, (sp, tg)
    for m in ('Ap', 'F'):
        assert ' & '.join(f'{t(m, sp, s, tg):.1f}' for s in S) in rt, (m, sp, tg)
vt = (V / 'tables/exp2b-validation.tex').read_text()
for m, name in (('Ap', "A$'$"), ('F', 'F')):
    cells = []
    for sp in 'ab':
        cs = [cross(m, sp, s, tg) for s in S for q, tg in CELLS if q == sp]; n = [c['n'] for c in cs]; sh = [c['v'] / c['t'] for c in cs]
        ru = [R[(m, sp, s, 1)]['category_seconds']['validation'] / sum(R[(m, sp, s, 1)]['category_seconds'].values()) for s in S]
        cells.append(f'{st.median(n):g} ({min(n)}--{max(n)}) & {st.median(sh):.2f} ({min(sh):.2f}--{max(sh):.2f}) & {min(ru):.2f}--{max(ru):.2f}')
    assert f'{name} & ' + ' & '.join(cells) + '\\\\' in vt, m
ft = (V / 'tables/exp2b-refinement.tex').read_text()
for d in DS.values():
    c = d['checks']; p = c['primary']['F']; g = max(abs(c[k]['F'] / p - 1) for k in ('g40', 'tight', 'shifted'))
    l3 = 100 * (c['L3']['F'] / p - 1); cf = 100 * (c['conforming']['F'] / p - 1) if 'conforming' in c else None
    row = f"& {p / F0:.4f} & ${l3:+.1f}$ & {1e4 * g:.1f} & " + (f'${cf:+.1f}$' if cf is not None else '---')
    assert row in ft, (d['roles'], row)
assert len([l for l in ft.splitlines() if l.endswith(' \\\\')]) == len(DS) == 14


# Manuscript bindings (line breaks and spacing ignored).
class _Norm(str):
    def __contains__(self, phrase):
        return ' '.join(phrase.split()) in ' '.join(self.split())


tex = (V / 'main.tex').read_text() + (V / 'optimization.tex').read_text(); txt = _Norm(tex)
assert '[PENDING' not in tex and 'PENDING exp2b' not in tex
assert r'\title{Adjoint gradients reach validated torque-ripple targets\\' in tex and '2.5 and 4.1 times sooner than finite differences' in txt
for bad in ('gradient access', 'ruling', 'dilutes every', 'logarithm of'):
    assert bad not in txt, bad
import re as _re
ab = tex[tex.index('\\begin{abstract}') + 16:tex.index('\\end{abstract}')]
assert len(_re.sub(r'\$[^$]*\$', 'X', ab).replace('--', ' ').split()) <= 250
for phrase in [
    'With every run on one single-threaded core, so that the finite-difference stencil is solved serially, the adjoint reached the primary target 2.47 and 4.11 times sooner, from every start, and 5.58 and 10.07 times sooner once the shared validation charges are excluded',
    'COBYQA against finite-difference SLSQP, is 1.2 in the eight-parameter space, where it ranges from 0.70 to 2.27 across starts, and 1.9 in the 18-parameter space',
    'On one core per run the adjoint reaches the target 2.47 and 4.11 times sooner',
    'was 2.5 and 4.1 times slower with 8 and 18 parameters on one core per run',
    'so F\'s $n$ stencil solves run one after another and the time ratio is also a ratio of compute',
    'with a step of $10^{-6}$ in each coordinate\'s physical unit (metres for the lengths and offsets)',
    'as 8\\% of components did; no gradient was refused',
    'the geometric-mean F/A$\'$ ratio is 2.47 in space A (range 2.45--2.52) and 4.11 in space B (3.84--4.33), and repeat 2 gives 2.49 and 4.04',
    'F crosses each target after the same number of validations as A$\'$ from every start',
    'Excluding the validation charges up to each crossing, F/A$\'$ becomes 5.58 and 10.07, close to the ratios of median training-step cost, 5.3 and 10.9: one F step costs 1.53 and 3.32 units, an A$\'$ step 0.29 and 0.31',
    'A stencil solve costs a median of 0.23 units, like any training solve, while the adjoint adds 0.06 units per step',
    'Validation is a median 61--65\\% of A$\'$\'s time to a target and 16--27\\% of F\'s',
    'In space B all 20 ablation runs stop at $T_{99}$',
    'its validated objectives agree with A$\'$\'s within 1\\% at three starts, and at the other two individual validated objectives differ by up to 29\\%',
    'In space A, F stops at the budget in eight of ten runs and converges in two, whereas A$\'$ converges in four, stops in the solver in four and reaches the budget in two; from every start, F\'s best validated objective is within 0.4\\% of A$\'$\'s',
    'the max-norm relative error is $5.2\\times10^{-4}$ to $1.9\\times10^{-3}$ for $F$ and at most $7.2\\times10^{-4}$ for the full-current mean',
    'the SW3 component of the mean gradient is $-9.62\\times10^{-3}$ by finite differences against $-7.74\\times10^{-3}$ by the adjoint, a relative error of 0.24, on a component $8.7\\times10^{-5}$ times the largest',
    'Its absolute error, $1.9\\times10^{-3}$, matches SW3\'s at the three passing points ($1.8$--$1.9\\times10^{-3}$); we read this as the noise floor of the fixed step on a small component, an interpretation that no step sweep has tested',
    'F\'s L1 gradient check fails its per-component limit at one small component', 'the record\'s status is FAIL because of one small component',
    'A$\'$\'s primary crossing times are 0.94--1.04 times those of the main-study A',
    'the main-study B/A geometric means are 3.10 and 7.79, and F/A$\'$ over the same starts is 2.47 and 4.10',
    'is 1.24, 1.45, 0.70, 0.95 and 2.27 in space A (geometric mean 1.22) and 3.84, 2.46, 1.26 and 1.12 in space B (1.91)',
    'the means are 1.30 and 2.03', 'removes the 6\\% difference in time unit',
    '(F/A$\'$)(B/F) is 3.02 and 7.82 in units against 3.10 and 7.79',
    'with eight parameters COBYQA matched or beat finite-difference SLSQP from two of five starts (B/F 0.70 and 0.95 in units, 0.75 and 1.01 in seconds), and with 18 parameters finite-difference SLSQP was faster from every start',
    'keep all ten target crossings within their targets at L3, with at least 15\\% margin',
    'max-norm relative error of $5.1\\times10^{-4}$ to $1.9\\times10^{-3}$ for $F$ and $1.0\\times10^{-4}$ to $6.7\\times10^{-4}$',
    'up to $5.7\\times10^{-2}$ for SW3 at the space-B interior point',
    'calibrated unit of $66.66\\,\\mathrm{s}$, against $70.94\\,\\mathrm{s}$', 'agreed within $2.4\\times10^{-13}$', 'load peaked at 19.7',
    'inventory of the 2,118 session files',
    'was posted at 22:34~UTC on 2026-10-20 and frozen by a comment at 23:00~UTC, before the first ablation run began at 02:07~UTC on 2026-10-21',
    'All 90 checks of the 14 designs completed', 'within $6.3\\times10^{-4}$ relative',
    'L3 changes $F$ by $-2.9\\%$ to $+2.6\\%$ ($-2.9\\%$ to $+1.1\\%$ at the crossings)', 'by less than $0.03\\%$',
    'gives $1.1\\%$ to $4.4\\%$ more ripple on the space-A designs', 'with at least 15\\% margin', 'the closest by 0.044\\%',
    'chose the same 14 designs, bit for bit']:
    assert phrase in txt, phrase
print('PASS: Exp 2b tables (per-start times and ratios, validation charges, final checks) and every manuscript number; title; no pending markers.')
