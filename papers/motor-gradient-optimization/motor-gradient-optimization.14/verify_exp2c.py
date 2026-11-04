"""Read-only check of the post-hoc warm-stencil arm F_w and its control A' (#1224), v14 onward.

No FEM or optimizer run. Binds the 20 per-run extracts and the session setup records
to the F_w session inventory, recomputes target credit, the per-start F_w/A' ratios,
hit fractions, crossing validation counts, costs, the cross-session comparisons with
F and the main study, the final checks, the L1 warm accuracy records, the rebuilt
refinement binary's equivalence and the protocol timing, and checks every stated
number against the manuscript text.

With EXP2C_SESSION=<copy>/exp2-fd-warm/session and EXP2C_REFINE=<refine-dir> (and
optionally EXP2C_SELECTION=<selection.json>) the extracts are re-derived from the raw
traces and results and must match exactly.
"""
from pathlib import Path
import hashlib, json, math, os, re, statistics as st, sys

V = Path(__file__).resolve().parent; D = V / 'figures/src/data/exp2c'
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
INV = D / 'provenance/exp2-fd-warm-inventory-df509322.sha256.txt'
assert sha(INV) == 'df5093225b52b74f622d4bfbd193c58ea24d6116eadb2f0a03b9e9830e8a2559'
inventory = dict(reversed(l.split('  ', 1)) for l in INV.read_text().splitlines()); assert len(inventory) == 1180
X = json.loads((D / 'runs.json').read_text())['runs']; RF = json.loads((D / 'refinement.json').read_text())
XB = json.loads((V / 'figures/src/data/exp2b/runs.json').read_text())['runs']
M = json.loads((V / 'figures/src/data/exp2/runs.json').read_text()); TG = M['anchor']['targets']; F0 = M['anchor']['F0']; FLOOR = M['anchor']['mean_floor']

raw = os.environ.get('EXP2C_SESSION'), os.environ.get('EXP2C_REFINE')
if all(raw):
    sys.path.insert(0, str(V / 'figures/src')); import extract_exp2c
    runs2, ref2 = extract_exp2c.extract(*raw)
    assert runs2 == json.loads((D / 'runs.json').read_text()) and ref2 == RF, 'extract differs from raw re-derivation'
    if os.environ.get('EXP2C_SELECTION'):
        assert sha(os.environ['EXP2C_SELECTION']) == json.loads((D / 'selection-check.json').read_text())['selection_sha256']
    print('PASS: Exp 2c extracts re-derived from the 20 raw traces and the 84 refinement results (exact match).')
else:
    print('NOTE: raw F_w traces not supplied (EXP2C_SESSION/EXP2C_REFINE); checking the hash-bound extracts only.')

# Inventory binding, setup records, session facts.
assert len(X) == 20
for r in X:
    assert inventory['exp2-fd-warm/session/' + r['file']] == r['sha256'], r['file']
SU = D / 'provenance/setup'
for n in ('session.json', 'calibration_primary.json', 'anchor_preflight.json'):
    assert sha(SU / n) == inventory['exp2-fd-warm/session/' + n], n
sess = json.loads((SU / 'session.json').read_text()); cal = json.loads((SU / 'calibration_primary.json').read_text())['calibration']
ap = json.loads((SU / 'anchor_preflight.json').read_text())
U = 69.088454188
assert sess['session_id'] == 'fd_warm_ablation-20261021T203040Z' and sess['procs'] == 15 and math.isclose(sess['unit_seconds'], U)
assert len(cal['validation_seconds']) == 45 and st.median(cal['validation_seconds']) == cal['unit_seconds'] == sess['unit_seconds']
assert ap['status'] == 'PASS' and max(s['comparison']['F_relative_error'] for s in ap['sessions']) < 2.5e-13
assert all(math.isclose(r['unit_seconds'], U) and r['budget_fe'] == 120.0 and r['host_contaminated'] is False and r['training_warm'] == 'Cold' for r in X)
assert round(max(r['in_run_load_max'] for r in X), 1) == 17.4
R = {(r['method'], r['space'], r['start']): r for r in X}
S = ['theta0', 'lhs1', 'lhs2', 'lhs3', 'lhs4']
assert set(R) == {(m, sp, s) for m in ('Ap', 'W') for sp in 'ab' for s in S} and all(r['repeat'] == 1 for r in X)
for r in X:
    if r['method'] == 'W':
        assert r['method_spec']['letter'] == 'W' and r['method_spec']['fd_stencil_start'] == 'base' and r['fd_step'] == 1e-6
    else:
        assert r['method_spec']['letter'] == 'A' and r['fd_gradients'] == []


def credited(v):
    return v['status'] == 'ok' and v['credited'] and v['admissible'] and v['mean_ok']


for r in X:
    for tg, T in TG.items():
        hits = [v for v in r['validations'] if credited(v) and v['F_val'] + max(.1 * v['F_val'], 2.5e-7) <= T]
        a = r['attainment'][tg]; assert bool(hits) == bool(a['attained']), (r['file'], tg)
        if hits:
            assert hits[0]['index'] == a['validation'] and hits[0]['wall_fe'] == a['wall_fe']
t = lambda m, sp, s, tg: R[(m, sp, s)]['attainment'][tg]['wall_fe'] if R[(m, sp, s)]['attainment'][tg]['attained'] else None
CELLS = (('a', 'T50'), ('a', 'T90'), ('b', 'T50'), ('b', 'T90'), ('b', 'T99'))
for m in ('Ap', 'W'):
    assert all(t(m, sp, s, tg) for sp, tg in CELLS for s in S) and not any(t(m, 'a', s, 'T99') for s in S)
wa = {c: [t('W', *c[:1], s, c[1]) / t('Ap', *c[:1], s, c[1]) for s in S] for c in CELLS}
fmt = lambda xs: (round(gm(xs), 2), round(min(xs), 2), round(max(xs), 2))
assert fmt(wa[('a', 'T90')]) == (1.71, 1.68, 1.77) and fmt(wa[('b', 'T99')]) == (2.70, 2.62, 2.79)
assert round(gm(wa[('a', 'T50')]), 2) == 1.69 and round(gm(wa[('b', 'T90')]), 2) == 2.59 and round(gm(wa[('b', 'T50')]), 2) == 3.02 and round(max(wa[('b', 'T50')]), 2) == 6.92
import collections
sr = collections.Counter((r['method'], r['space'], r['stop_reason']) for r in X)
assert (sr[('W', 'a', 'budget')], sr[('W', 'a', 'converged')]) == (4, 1) and (sr[('Ap', 'a', 'converged')], sr[('Ap', 'a', 'solver_stopped')], sr[('Ap', 'a', 'budget')]) == (2, 2, 1)
assert sr[('W', 'b', 'T99')] == sr[('Ap', 'b', 'T99')] == 5
# All arms cross each target after the same number of validations (F_w, its A', F and the F session's A').
RB = {(r['method'], r['space'], r['start']): r for r in XB if r['repeat'] == 1}
exc = []
for sp, tg in CELLS:
    for s in S:
        k = {R[(m, sp, s)]['attainment'][tg]['validation'] for m in ('Ap', 'W')} | {RB[(m, sp, s)]['attainment'][tg]['validation'] for m in ('Ap', 'F')}
        if len(k) > 1:
            exc.append((sp, tg, s))
assert exc == [('b', 'T50', 'lhs3')]  # primary targets all equal; one secondary-target exception
wl = R[('W', 'b', 'lhs3')]; kw = wl['attainment']['T50']['validation']; ka = R[('Ap', 'b', 'lhs3')]['attainment']['T50']['validation']
assert (ka, kw - ka) == (2, 6) and round(wl['validations'][2]['F_val'] / F0, 2) == .46 and wl['validations'][2]['F_upper'] > TG['T50']
assert all(not wl['validations'][i]['mean_ok'] for i in range(3, 8)) and wl['validations'][8]['mean_ok']


def cross(m, sp, s, tg):
    r = R[(m, sp, s)]; a = r['attainment'][tg]; c = r['validation_charges_seconds']
    assert math.isclose(sum(c), r['category_seconds']['validation'], rel_tol=1e-9, abs_tol=1e-9)
    v = sum(c[:a['validation'] + 1]) / r['unit_seconds']; return a['wall_fe'] - v


nv = {sp: gm([cross('W', sp, s, tg) / cross('Ap', sp, s, tg) for s in S]) for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert (round(nv['a'], 2), round(nv['b'], 2)) == (3.23, 6.10)
# Cross-session: F_w against F (seconds), this A' against the F session's A' (units).
uB = XB[0]['unit_seconds']; uM = M['runs'][0]['unit_seconds']
tb = lambda m, sp, s, tg: RB[(m, sp, s)]['attainment'][tg]['wall_fe']
wf = {sp: gm([t('W', sp, s, tg) * U / (tb('F', sp, s, tg) * uB) for s in S]) for sp, tg in (('a', 'T90'), ('b', 'T99'))}
assert (round(wf['a'], 2), round(wf['b'], 2)) == (.72, .68)
aa = [t('Ap', sp, s, tg) / tb('Ap', sp, s, tg) for sp, tg in (('a', 'T90'), ('b', 'T99')) for s in S]
assert round(max(abs(x - 1) for x in aa), 2) == .04  # 'within 4%'
assert round(U, 2) == 69.09 and round(uB, 2) == 66.66 and round(uM / U - 1, 2) == .03
# Costs.
gcost = {sp: st.median(g['cost_fe'] for r in X if r['method'] == 'W' and r['space'] == sp for g in r['fd_gradients']
                       if g['evaluated'] == g['components'] == (8 if sp == 'a' else 18) and g['cost_fe'] is not None) for sp in 'ab'}
adj = {sp: st.median(r['category_seconds']['adjoint'] / r['unit_seconds'] / r['gradient_train_events'] for r in X if r['method'] == 'Ap' and r['space'] == sp) for sp in 'ab'}
assert (round(gcost['a'], 2), round(gcost['b'], 2)) == (.97, 2.36) and (round(adj['a'], 3), round(adj['b'], 3)) == (.054, .059)
assert (round(.97 / .054), round(2.36 / .059)) == (18, 40) and (round(gcost['a'] / adj['a']), round(gcost['b'] / adj['b'])) == (18, 40)
newt = {sp: st.median(x for r in X if r['method'] == 'W' and r['space'] == sp for g in r['fd_gradients'] for x in g['newton']) for sp in 'ab'}
coldn = {sp: st.median(x for r in X if r['method'] == 'W' and r['space'] == sp for x in r['train_newton']) for sp in 'ab'}
assert newt == {'a': 60, 'b': 60} and (coldn['a'], coldn['b']) == (170, 167)
sc = {sp: st.median(x / r['unit_seconds'] for r in X if r['method'] == 'W' and r['space'] == sp for x in r['stencil_charges_seconds']) for sp in 'ab'}
bc = {sp: st.median(x / r['unit_seconds'] for r in X if r['method'] == 'W' and r['space'] == sp for x in r['base_charges_seconds']) for sp in 'ab'}
assert (round(sc['a'], 3), round(sc['b'], 3), round(bc['a'], 3), round(bc['b'], 3)) == (.117, .126, .220, .233)
assert all(.5 <= sc[sp] / bc[sp] <= .56 for sp in 'ab')
G = [(r, g) for r in X if r['method'] == 'W' for g in r['fd_gradients']]
assert len(G) == 263 and all(g['stencil_start'] == 'base' and not g['refused'] and not g['base_resolved'] for _, g in G)
cold = [(r, g) for r, g in G if 'cold' in g['starts']]
assert sum(set(g['starts']) == {'same_mesh'} for _, g in G) == 262 and len(cold) == 1
assert cold[0][0]['space'] == 'a' and cold[0][0]['start'] == 'lhs2' and set(cold[0][1]['starts']) == {'cold'}
# Decomposition against the main study: B/A = (B/F_w)(F_w/A'), matched starts.
MA = {(r['method'], r['space'], r['start']): r for r in M['runs'] if r['repeat'] == 1}
dec = {}
for sp, tg in (('a', 'T90'), ('b', 'T99')):
    ss = [s for s in S if MA[('B', sp, s)]['attainment'][tg]['attained']]
    bw = [MA[('B', sp, s)]['attainment'][tg]['wall_fe'] / t('W', sp, s, tg) for s in ss]
    ba = gm([MA[('B', sp, s)]['attainment'][tg]['wall_fe'] / MA[('A', sp, s)]['attainment'][tg]['wall_fe'] for s in ss])
    wam = gm([t('W', sp, s, tg) / t('Ap', sp, s, tg) for s in ss]); dec[sp] = (bw, ba, wam)
assert [round(x, 2) for x in dec['a'][0]] == [1.83, 2.07, 1.01, 1.38, 3.23] and [round(x, 2) for x in dec['b'][0]] == [6.11, 3.35, 1.94, 1.82]
assert (round(gm(dec['a'][0]), 2), round(gm(dec['b'][0]), 2)) == (1.76, 2.92) and (round(gm(dec['a'][0]), 1), round(gm(dec['b'][0]), 1)) == (1.8, 2.9)
assert round(gm(dec['a'][0]) * uM / U, 2) == 1.81 and round(gm(dec['b'][0]) * uM / U, 1) == 3.0
assert (round(dec['a'][2], 2), round(dec['b'][2], 2)) == (1.71, 2.68) and (round(dec['a'][1], 2), round(dec['b'][1], 2)) == (3.10, 7.79)
assert (round(dec['a'][2] * gm(dec['a'][0]), 2), round(dec['b'][2] * gm(dec['b'][0]), 2)) == (3.02, 7.82)
assert min(dec['a'][0] + dec['b'][0]) > 1 and round(min(dec['a'][0]), 2) == 1.01
print("PASS: Exp 2c inventory binding (20 traces, setup records), target credit, F_w/A' ratios, hit fractions, crossings, costs, cold fallback, cross-session comparisons and the B/F_w decomposition.")

# Final checks, selection identity, A' design keys against the F session.
DS = RF['designs']; assert len(DS) == 13 and sum(len(d['checks']) for d in DS.values()) == 84
assert all(c['ok'] and c['status'] == 'complete' for d in DS.values() for c in d['checks'].values())
assert sha(D / 'refinement-tools/cohort.json') == RF['cohort_sha256']
sel = json.loads((D / 'selection-check.json').read_text())
assert sorted(d['x_bits'] for d in sel['designs']) == sorted(d['x_bits'] for d in DS.values()) and sel['source_commit'].startswith('669029c5')
byf = {r['file']: r for r in X}
cands = [d for d in DS.values() if not d['roles'][0].startswith('theta0')]; crossd = [d for d in cands if any(r.startswith('crossing') for r in d['roles'])]
rel = lambda d, c: d['checks'][c]['F'] / d['checks']['primary']['F'] - 1
assert f"{max(abs(d['checks']['primary']['F'] / byf[d['sources'][0]['run']]['validations'][d['sources'][0]['validation']]['F_val'] - 1) for d in cands):.1e}" == '4.1e-04'
assert (round(100 * min(rel(d, 'L3') for d in cands), 1), round(100 * max(rel(d, 'L3') for d in cands), 1)) == (-2.9, 2.6)
assert (round(100 * min(rel(d, 'L3') for d in crossd), 1), round(100 * max(rel(d, 'L3') for d in crossd), 1)) == (-2.9, 1.1)
assert round(100 * max(abs(rel(d, c)) for d in cands for c in ('g40', 'tight', 'shifted')), 2) == .03
conf = [rel(d, 'conforming') for d in cands if 'conforming' in d['checks']]; assert (round(100 * min(conf), 1), round(100 * max(conf), 1)) == (.9, 4.4)
marg = [(TG[r.split('/')[-1]] - d['checks']['L3']['F'] - max(.1 * d['checks']['L3']['F'], 2.5e-7)) / TG[r.split('/')[-1]] for d in crossd for r in d['roles'] if r.startswith('crossing')]
assert len(marg) == 10 and len(crossd) == 7 and round(100 * min(marg)) == 15
mu = min(d['checks'][c]['full_current_mean'] for d in cands for c in ('primary', 'L3', 'g40', 'tight', 'shifted')) / FLOOR
assert mu > 1 and round(100 * (mu - 1), 3) == .044
RBF = json.loads((V / 'figures/src/data/exp2b/refinement.json').read_text())['designs']
ra = {r: k for k, d in DS.items() for r in d['roles'] if 'A_prime' in r}; rb = {r: k for k, d in RBF.items() for r in d['roles'] if 'A_prime' in r}
assert set(ra) == set(rb) and len(ra) == 7 and [r for r in ra if ra[r] != rb[r]] == ['crossing/A_prime/b/T50']
t50 = sorted(t('Ap', 'b', s, 'T50') for s in ('theta0', 'lhs1', 'lhs4')); assert (round(t50[0], 2), round(t50[-1], 2)) == (1.56, 1.60)
# Rebuilt refinement binary: the 12 rerun checks reproduce every non-timing field of the original results.
BE = D / 'binary-equivalence'
bins = dict(reversed(l.split('  ', 1)) for l in (BE / 'binary.sha256').read_text().splitlines())
assert any(v.startswith('679a40c8') for v in bins.values()) and any(v.startswith('c75fe847') for v in bins.values())


def flat(o, p=''):
    if isinstance(o, dict):
        if not o:
            yield p, o
        for k, v in o.items():
            yield from flat(v, p + '/' + k)
    elif isinstance(o, list):
        if not o:
            yield p, o
        for i, v in enumerate(o):
            yield from flat(v, p + '[%d]' % i)
    else:
        yield p, o


def timing(p):  # wall/CPU times, resource usage, process ids and output paths
    leaf = p.split('/')[-1].split('[')[0]
    return 'resources' in p or leaf.endswith('_s') or 'seconds' in leaf or leaf in ('pid', 'maxrss_bytes', 'maxrss_raw', 'file', 'self', 'reaped_children')


nfield = ndiff = 0; types = set(); nck = 0
for f in sorted((BE / 'new').glob('*/*.json')):
    k, ch = f.parent.name, f.stem; old = BE / 'old' / k / f.name
    key = next(kk for kk in RBF if kk.startswith(k)); assert sha(old) == RBF[key]['checks'][ch]['result_sha256']
    a = dict(flat(json.loads(f.read_bytes()))); b = dict(flat(json.loads(old.read_bytes())))
    ks = [p for p in set(a) | set(b) if not timing(p)]; nfield += len(ks); ndiff += sum(a.get(p) != b.get(p) for p in ks); types.add(ch); nck += 1
assert (nck, len(types), nfield, ndiff) == (12, 7, 4181, 0)
print('PASS: Exp 2c final refinement (13 designs, 84 checks), selection identity, A\' design keys, and the rebuilt binary (12 checks, 7 types, 4,181 non-timing fields identical).')

# L1 warm accuracy and step sweeps (PR #1225, merge 669029c5).
man = {f['snapshot']: f for f in json.loads((V / 'evidence-manifest.json').read_text())['files']}
rec = {}
for n in ('fd-gradient-runtime-checks-L1-warm', 'fd-step-sweep-L1-warm', 'fd-step-sweep-L1-warm-b'):
    e = man[f'figures/src/data/exp2c/gradient-checks/{n}.json']
    assert e['source_commit'] == '669029c5653cb5fe4724ec0c7525ba60c770975e' and sha(V / e['snapshot']) == e['sha256']
    rec[n] = json.loads((V / e['snapshot']).read_text())
LW = rec['fd-gradient-runtime-checks-L1-warm']; L1 = json.loads((V / 'figures/src/data/exp2b/fd-gradient-runtime-checks-L1.json').read_text())
cold1 = {(r['space'], p['point']): p for r in L1['records'] for p in r['points']}
assert LW['status'] == 'FAIL' and LW['gate'] == 'proceed' and LW['gate_factor'] == 10.0 and LW['stencil_start'] == 'base'
ratios, fails, mac_n, mac_s = [], [], [], []
for r in LW['records']:
    assert r['level'] == 1 and r['step'] == 1e-6 and r['session']['training']['newton_rel_tol'] == 1e-8
    for p in r['points']:
        comp = p['components']
        for key in ('dF', 'dmean_full'):
            f = [c['fd_' + key] for c in comp]; a = [c['adjoint_' + key] for c in comp]; big = max(abs(y) for y in a)
            nrm = max(abs(x - y) for x, y in zip(f, a)) / big; assert math.isclose(nrm, p['normwise_rel_' + key], rel_tol=1e-9)
            ratios.append(nrm / cold1[(r['space'], p['point'])]['normwise_rel_' + key])
            for c in comp:
                y = c['adjoint_' + key]
                if y != 0 and abs(y) >= LW['negligible'] * big and abs(c['fd_' + key] - y) / abs(y) > .1:
                    fails.append((r['space'], p['point'], c['name'], key, abs(c['fd_' + key] - y) / abs(y), abs(y) / big, c['fd_' + key] * y < 0))
        cs = p['cost']; mac_n.append(cs['warm_over_cold_newton_iterations']); mac_s.append(cs['warm_over_cold_solve_s'])
        assert math.isclose(cs['warm_over_cold_newton_iterations'], sum(cs['warm_stencil']['newton_iterations']) / sum(cs['cold_stencil']['newton_iterations']), rel_tol=1e-9)
assert (round(min(ratios), 2), round(max(ratios), 2)) == (.92, 1.08) and max(ratios) < 10
assert {(f[0], f[1]) for f in fails} == {('a', 'theta0'), ('a', 'interior'), ('b', 'theta0'), ('b', 'interior')}
objf = [f for f in fails if f[3] == 'dF']; assert {f[2] for f in objf} == {'SW1', 'SW4', 'Sr1'} and (f'{min(f[5] for f in objf):.0e}', f'{max(f[5] for f in objf):.1e}') == ('9e-05', '6.3e-04')
assert [f[2:4] for f in fails if f[:2] == ('a', 'interior')] == [('SW3', 'dmean_full')]
assert sorted(f[2] for f in fails if f[:2] == ('b', 'interior')) == ['SW1', 'SW4', 'Sr1'] and all(f[2] == 'Sr1' for f in fails if f[1] == 'theta0')
flip = [f for f in fails if f[6]]; assert len(flip) == 1 and flip[0][:3] == ('b', 'interior', 'Sr1') and round(flip[0][4], 2) == 1.58
assert (round(min(mac_n), 2), round(max(mac_n), 2), round(min(mac_s), 2), round(max(mac_s), 2)) == (.32, .37, .48, .53)
SWb = rec['fd-step-sweep-L1-warm-b']; rr = SWb['record']; base = rr['base']; assert rr['space'] == 'b' and rr['point'] == 'interior'
fe = {}; ce = {}
for c in rr['components']:
    stn = {(q['step'], q['side']): q for q in c['stencil']}; adjv = c['schemes'][0]['dF']['adjoint']
    for h in SWb['forward_steps']:
        q = stn[(h, '+')]; fe[(c['name'], h)] = (q['F'] - base['F']) / q['dx'] - adjv
    for h in SWb['central_steps']:
        qp, qm = stn[(h, '+')], stn[(h, '-')]; ce[(c['name'], h)] = (qp['F'] - qm['F']) / (qp['dx'] - qm['dx']) - adjv
names = ('Sr1', 'SW4', 'SW1')
assert all(fe[(n, 1e-5)] < 0 and fe[(n, 1e-6)] < 0 and fe[(n, 1e-7)] < 0 for n in names)
assert (f"{max(fe[(n, 1e-5)] for n in names):.1e}", f"{min(fe[(n, 1e-5)] for n in names):.1e}") == ('-3.1e-04', '-3.8e-04')
assert {f'{fe[(n, 1e-6)]:.1e}' for n in names} == {'-3.1e-03'} and {f'{fe[(n, 1e-7)]:.1e}' for n in names} <= {'-3.0e-02', '-3.1e-02'}
assert max(abs(ce[(n, 1e-6)]) for n in names) < 1.2e-7 and f"{-fe[('Sr1', 1e-6)] * 1e-6:.0e}" == '3e-09'
assert SWb['reproduction']['components'][0]['dF_diff'] == 0.0
print('PASS: Exp 2c L1 warm accuracy (4 points; norm ratios to F, component failures, sign reversal, Mac cost) and the space-B warm step sweep (1/h growth, central removal).')

# Protocol timing (#1224).
pe = json.loads((D / 'provenance/protocol-1224-export.json').read_text())
assert hashlib.sha256(pe['issue']['body'].encode()).hexdigest() == pe['issue']['body_sha256'] and pe['issue']['graphql_userContentEdits_totalCount'] == 0
assert pe['issue']['created_at'] == '2026-10-21T15:18:46Z' and pe['freeze_comment']['created_at'] == pe['freeze_comment']['updated_at'] == '2026-10-21T17:54:41Z'
assert pe['freeze_comment']['created_at'] < '2026-10-21T20:30:40Z'


class _Norm(str):
    def __contains__(self, phrase):
        return ' '.join(phrase.split()) in ' '.join(self.split())


tex = (V / 'main.tex').read_text() + (V / 'optimization.tex').read_text(); txt = _Norm(tex)
assert r'\title{Per core, adjoint gradients reach validated torque-ripple targets\\' in tex and '1.7 and 2.7 times sooner than warm-started finite differences' in txt
assert 'exp2c-fd-warm' not in tex and 'more than the gradient-based optimizer' not in txt
for phrase in [
    'the adjoint reached the primary target 1.71 and 2.70 times sooner, from every start, than the stronger variant, whose stencil solves start from the base state (3.23 and 6.10 times without the shared validation charges), and 2.47 and 4.11 times sooner than cold-started stencils',
    'Warm stencils halve the cost of a stencil solve but are less accurate on small gradient components',
    'The remaining factor, COBYQA against warm finite-difference SLSQP, is about 1.8 and 2.9, approximate across sessions, so the adjoint and the gradient-based optimizer contribute comparable factors',
    'the adjoint reaches the target 1.71 and 2.70 times sooner than the stronger, warm-started variant and 2.47 and 4.11 times sooner than cold-started stencils',
    'COBYQA against warm finite-difference SLSQP, is about 1.8 with eight parameters and 2.9 with 18',
    'took 1.7 and 2.7 times as long with 8 and 18 parameters on one core per run when the stencils were warm-started, and 2.5 and 4.1 times as long when they started cold',
    'the geometric-mean F$_w$/A$\'$ ratio is 1.71 in space A (range 1.68--1.77) and 2.70 in space B (2.62--2.79)',
    'F$_w$ takes about 0.72 and 0.68 of F\'s time in seconds', 'F$_w$/A$\'$ becomes 3.23 and 6.10',
    'All three arms cross each primary target after the same number of validations from every start, with one exception at a secondary target',
    'its third validated objective, $0.46F_0$, misses $T_{50}$ once the allowance is added, and its next five fall below the mean floor, so it crosses six validations later',
    'it costs F$_w$ 0.97 and 2.36 units, against 0.054 and 0.059 for the adjoint in F$_w$\'s session, 18 and 40 times as much',
    'a median of 60 Newton iterations over its 60 field solves, against 170 and 167 for a cold training evaluation',
    'charged a median of 0.117 and 0.126 units against 0.220 and 0.233 for the session\'s base training solves',
    'One of F$_w$\'s 263 gradients fell back to cold stencils, at 110 units, long after that run\'s crossings',
    'F$_w$\'s max-norm errors are 0.92 to 1.08 times F\'s', '(SW1, SW4 and Sr1, $9\\times10^{-5}$ to $6.3\\times10^{-4}$ of the largest)',
    'with a sign reversal on Sr1 at the space-B interior point (relative error 1.58), and at the fourth on SW3 of the mean gradient',
    '(about $3\\times10^{-9}\\,\\mathrm{N^2\\,m^2}$)',
    'F$_w$/A$\'$ over the same starts is 1.71 and 2.68', 'is 1.83, 2.07, 1.01, 1.38 and 3.23 in space A (geometric mean 1.76) and 6.11, 3.35, 1.94 and 1.82 in space B (2.92)',
    'the means are 1.81 and about 3.0', '(F$_w$/A$\'$)(B/F$_w$) is 3.02 and 7.82 in units against 3.10 and 7.79',
    'COBYQA was slower from every start, by as little as 1.01 at space-A LHS~2',
    'calibrated unit of $69.09\\,\\mathrm{s}$', 'the one-minute load peaked at 17.4', 'inventory of its 1,180 files',
    'posted at 15:18~UTC on 2026-10-21 and frozen by a comment at 17:54~UTC, before the first F$_w$ run began at 20:30~UTC',
    'F$_w$ ran to the budget in four of five runs and converged in one, and its A$\'$ converged in two, stopped in the solver in two and reached the budget in one',
    'The other targets give F$_w$/A$\'$ of 1.69 in space A and 2.59 ($T_{90}$) and 3.02 ($T_{50}$, with one start at 6.92) in space B',
    'Of F$_w$\'s 263 gradients, 262 warm-started every stencil from its base', 'within 4\\% in each session\'s units, and the selected A$\'$ designs coincide for six of seven roles',
    'three starts cross within 1.56--1.60 units of each other',
    '$-3.1\\times10^{-4}$ to $-3.8\\times10^{-4}$, $-3.1\\times10^{-3}$ and $-3.0\\times10^{-2}$ to $-3.1\\times10^{-2}$', 'below $1.2\\times10^{-7}$',
    'a warm stencil takes 0.32--0.37 of a cold stencil\'s Newton iterations and 0.48--0.53 of its wall time',
    'completed all 84 checks of 13 designs', 'within $4.1\\times10^{-4}$ relative', 'gives $0.9\\%$ to $4.4\\%$ more ripple on the space-A designs',
    'chose the same 13 designs, bit for bit', 'non-timing result field (4,181 fields) exactly', '\\texttt{679a40c8}', '\\texttt{63b3e3d8}', '\\texttt{c75fe847}']:
    assert phrase in txt, phrase
# Table exp2c-ratios recomputed independently of render_exp2c.py.
tt = (V / 'tables/exp2c-ratios.tex').read_text()
for c in CELLS:
    assert (' & '.join(f'{x:.2f}' for x in wa[c]) + f' & {gm(wa[c]):.2f}') in tt
    for m in ('Ap', 'W'):
        assert ' & '.join(f'{t(m, c[0], s, c[1]):.1f}' for s in S) in tt
print('PASS: Exp 2c protocol timing, Table exp2c-ratios and every manuscript number; title; no exp2c pending marker.')
