"""Read-only check of the post-hoc warm-stencil arm F_w and its control A' (#1224), v14 onward;
from v15 also the to-crossing cost check of both ablation arms and the package text (README, deposit record).

No FEM or optimizer run. Binds the 20 per-run extracts and the session setup records
to the F_w session inventory, recomputes target credit, the per-start F_w/A' ratios,
hit fractions, crossing validation counts, costs, the cross-session comparisons with
F and the main study, the final checks, the L1 warm accuracy records, the rebuilt
refinement binary's equivalence and the protocol timing, and checks every stated
number against the manuscript text.

With EXP2C_SESSION=<copy>/exp2-fd-warm/session and EXP2C_REFINE=<refine-dir> (and
optionally EXP2C_SELECTION=<selection.json>) the extracts are re-derived from the raw
traces and results and must match exactly. From v17, MOTOR_RAW_ROOT=<unpacked>/motor-paper-raw-traces
(the unpacked deposit archive) sets these, and EXP2B_SESSION for the timelines, from the archive
layout, and additionally checks every archived file against the archive manifest and the
binary-equivalence files against the archived refinement results and the rebuilt binary.
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

RAWROOT = Path(os.environ['MOTOR_RAW_ROOT']) if os.environ.get('MOTOR_RAW_ROOT') else None
if RAWROOT:  # v17: the unpacked deposit archive supplies every raw input
    os.environ.setdefault('EXP2C_SESSION', str(RAWROOT / 'exp2-fd-warm/session'))
    os.environ.setdefault('EXP2C_REFINE', str(RAWROOT / 'refinement/exp2-fd-warm'))
    os.environ.setdefault('EXP2C_SELECTION', str(RAWROOT / 'refinement/exp2-fd-warm/selection.json'))
    os.environ.setdefault('EXP2B_SESSION', str(RAWROOT / 'exp2-fd-ablation/session'))
raw = os.environ.get('EXP2C_SESSION'), os.environ.get('EXP2C_REFINE')
if all(raw):
    sys.path.insert(0, str(V / 'figures/src')); import extract_exp2c
    runs2, ref2 = extract_exp2c.extract(*raw)
    assert runs2 == json.loads((D / 'runs.json').read_text()) and ref2 == RF, 'extract differs from raw re-derivation'
    if os.environ.get('EXP2C_SELECTION'):
        assert sha(os.environ['EXP2C_SELECTION']) == json.loads((D / 'selection-check.json').read_text())['selection_sha256']
        import subprocess, tempfile  # v17: the whole extractor, selection check included, reproduces the package files byte for byte
        with tempfile.TemporaryDirectory() as td:
            subprocess.run([sys.executable, '-I', '-B', str(V / 'figures/src/extract_exp2c.py'), raw[0], raw[1], td, os.environ['EXP2C_SELECTION']], check=True, capture_output=True)
            for n in ('runs.json', 'refinement.json', 'selection-check.json'):
                assert (Path(td) / n).read_bytes() == (D / n).read_bytes(), n
        print('PASS: Exp 2c selection check re-derived from the independent selection output (runs, refinement and selection-check files byte-identical).')
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
MA1 = {(r['method'], r['space'], r['start']): r for r in M['runs'] if r['repeat'] == 1}  # v17: this session's A' against the main-study A
apm = [t('Ap', sp, s, tg) / MA1[('A', sp, s)]['attainment'][tg]['wall_fe'] for sp, tg in (('a', 'T90'), ('b', 'T99')) for s in S]
assert (round(min(apm), 2), round(max(apm), 2)) == (.97, 1.05)
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
bcA = {sp: st.median(x / r['unit_seconds'] for r in X if r['method'] == 'Ap' and r['space'] == sp for x in r['base_charges_seconds']) for sp in 'ab'}
assert (round(bcA['a'], 3), round(bcA['b'], 3)) == (.216, .233)  # v17: A' on F_w's measure (median base-solve charge)
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
if RAWROOT:  # v17: the package's comparison files are the archived results, and the archived binary is the rebuilt one
    BR = RAWROOT / 'refinement/binary-equivalence'
    assert sha(BR / 'icem_final_refinement.c75fe847') == bins['icem_final_refinement (rebuilt at 63b3e3d8)']
    for f in sorted((BE / 'new').glob('*/*.json')):
        k, ch = f.parent.name, f.stem
        assert f.read_bytes() == (BR / 'results' / k / ch / 'result.json').read_bytes(), (k, ch)
        assert (BE / 'old' / k / f.name).read_bytes() == (RAWROOT / 'refinement/exp2-fd-ablation/results' / k / ch / 'result.json').read_bytes(), (k, ch)
    assert all((BE / n).read_bytes() == (BR / n).read_bytes() for n in ('equivalence.txt', 'one.sh', 'sample.txt'))
    print('PASS: binary-equivalence files re-derived from the archive (12 rebuilt and 12 original results byte-identical; rebuilt binary c75fe847 re-hashed).')
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
assert '\\title{Adjoint Gradients for Torque-Ripple Optimization\\\\of a Nonlinear Motor}\n' in tex  # v20: the operator's short title; the F_w factors are in the abstract
assert '1.71 and 2.70 times sooner, from every start, than forward differences with warm-started solves' in txt  # v22: the abstract defines the stencil in words
assert 'exp2c-fd-warm' not in tex and 'more than the gradient-based optimizer' not in txt
# v15: the 1/h sentence names its point; no mechanism is claimed for the warm offset; the abstract carries no no-validation pair.
for bad in ('There the forward error', 'consistent with a warm stencil stopping', 'the stronger variant, whose stencil solves', 'without the shared validation charges', 'with L2 validation as in the sessions'):
    assert bad not in txt, bad
for phrase in [
    'The adjoint reached the primary target 1.71 and 2.70 times sooner, from every start, than forward differences with warm-started solves, and 2.47 and 4.11 times sooner than cold-started ones',
    'warm-starting it halved the cost of a stencil solve and cut the gradient factor from 2.47 and 4.11 to 1.71 and 2.70, at some cost in accuracy on small components',
    'COBYQA against finite-difference SLSQP contributes the remaining 1.2 to 1.8 with 8 parameters and 1.9 to 2.9 with 18',
    "The adjoint cuts the gradient's share of the time to target from 45--77\\% to about 5\\%, so validation and coarse-simulation evaluations bound the end-to-end gain",
    # v22: the introduction quotes Table decomp's four-start S18 gradient factor with the table's optimizer factors; Section 6 bridges it to the five-start values of the abstract and conclusion; the Discussion recap carries no numbers.
    'Table~\\ref{tab:decomp} gives the main result. On one core per run, with the finite-difference stencil (one perturbed coarse solve per parameter) solved serially, the gradient factor is 1.71--2.47 with eight parameters and 2.68--4.10 with 18, and the optimizer factor is 1.22--1.76 and 1.91--2.92',
    "Over those four starts the FD-warm/A$'$ and FD-cold/A$'$ means are 2.68 and 4.10, against 2.70 and 4.11 over all five starts; the abstract and conclusion quote the five-start values",
    'optimizer factor, and both matter (Table~\\ref{tab:decomp}). Second, with the optimization path held fixed, the gradient factor is a ratio of end-to-end times, not of gradient costs',
    'the optimizer factor, COBYQA against finite-difference SLSQP, made up the rest',
    'per-gradient cost ratios of 18--70 became end-to-end ratios of 1.71--4.11',
    "Of SLSQP's 3.10-fold (S8) and 7.79-fold (S18) advantage over COBYQA",
    'validation and coarse forwards now bound the remaining gain',
    "the geometric-mean FD-warm/A$'$ ratio is 1.71 in S8 (range 1.68--1.77) and 2.70 in S18 (2.62--2.79)",
    "FD-warm takes about 0.72 and 0.68 of FD-cold's time in seconds", "FD-warm/A$'$ becomes 3.23 and 6.10",
    'All three arms cross each primary target after the same number of validations from every start, with one exception at a secondary target',
    'its third validated objective, $0.46F_0$, misses $T_{50}$ once the allowance is added, and its next five fall below the mean floor, so it crosses six validations later',
    "it costs FD-warm 0.97 and 2.36 units, against 0.054 and 0.059 for the adjoint in FD-warm's session, 18 and 40 times as much",
    'a median of 60 Newton iterations over its 60 field solves, against 170 and 167 for a cold coarse evaluation',
    "charged a median of 0.117 and 0.126 units against 0.220 and 0.233 for the session's base coarse forwards",
    "one, at 110 units in S8 LHS~2, long after that run's crossings, fell back to cold stencils",
    "FD-warm's max-norm errors are 0.92 to 1.08 times FD-cold's", '(SW1, SW4 and Sr1, $9\\times10^{-5}$ to $6.3\\times10^{-4}$ of the largest)',
    'with a sign reversal on Sr1 at the S18 interior point (relative error 1.58), and the fourth is at the S8 interior point on SW3 of the mean gradient, where truncation dominates at the frozen step as for FD-cold',
    'At the frozen Newton tolerance, warm stencils are less accurate',
    'a common offset of about $-3\\times10^{-9}\\,\\mathrm{N^2\\,m^2}$, whichever component is perturbed; we did not isolate its source',
    'gives both splits over the starts where B attains the primary target', 'is 1.83, 2.07, 1.01, 1.38 and 3.23 in S8 (geometric mean 1.76) and 6.11, 3.35, 1.94 and 1.82 in S18 (2.92)',
    'the means are 1.81 and about 3.0', "The products close the main study's B/A only approximately",
    'COBYQA was slower than FD-warm from every start, by as little as 1.01 at S8 LHS~2',
    'calibrated unit of $69.09\\,\\mathrm{s}$', 'the one-minute load peaked at 17.4', 'inventory of its 1,180 files',
    'posted at 15:18~UTC on 2026-10-21 and frozen by a comment at 17:54~UTC, before the first FD-warm run began at 20:30~UTC',
    "FD-warm ran to the budget in four of five runs and converged in one, and its A$'$ converged in two, stopped in the solver in two and reached the budget in one",
    "The other targets give FD-warm/A$'$ of 1.69 in S8 and 2.59 ($T_{90}$) and 3.02 ($T_{50}$, with one start at 6.92) in S18",
    "Of FD-warm's 263 gradients, 262 warm-started every stencil from its base", 'within 4\\% in each session\'s units, and the selected A$\'$ designs coincide for six of seven roles',
    'three starts cross within 1.56--1.60 units of each other',
    '$-3.1\\times10^{-4}$ to $-3.8\\times10^{-4}$, $-3.1\\times10^{-3}$ and $-3.0\\times10^{-2}$ to $-3.1\\times10^{-2}$', 'below $1.2\\times10^{-7}$',
    'a warm stencil takes 0.32--0.37 of a cold stencil\'s Newton iterations and 0.48--0.53 of its wall time',
    'completed all 84 checks of 13 designs', 'within $4.1\\times10^{-4}$ relative', 'gives $0.9\\%$ to $4.4\\%$ more ripple on the S8 designs',
    'chose the same 13 designs, bit for bit', 'non-timing result field (4,181 fields) exactly', '\\texttt{679a40c8}', '\\texttt{63b3e3d8}', '\\texttt{c75fe847}']:
    assert phrase in txt, phrase
# Table exp2c-ratios recomputed independently of render_exp2c.py.
tt = (V / 'tables/exp2c-ratios.tex').read_text()
for c in CELLS:
    assert (' & '.join(f'{x:.2f}' for x in wa[c]) + f' & {gm(wa[c]):.2f}') in tt
    for m in ('Ap', 'W'):
        assert ' & '.join(f'{t(m, c[0], s, c[1]):.1f}' for s in S) in tt
print('PASS: Exp 2c protocol timing, Table exp2c-ratios and every manuscript number; title; no exp2c pending marker.')

# v15: evaluation timelines of both ablation sessions, bound to the run extracts (and so to the inventories).
TL = {k: json.loads((V / f'figures/src/data/{k}/timeline.json').read_text())['runs'] for k in ('exp2b', 'exp2c')}
for k, env, runs_ in (('exp2b', 'EXP2B_SESSION', XB), ('exp2c', 'EXP2C_SESSION', X)):
    byf_ = {r['file']: r for r in runs_}; assert len(TL[k]) == len(runs_) and {q['file'] for q in TL[k]} == set(byf_)
    for q in TL[k]:
        r = byf_[q['file']]
        assert q['sha256'] == r['sha256'] and len(q['train_wall_fe']) == r['train_events'] and len(q['fd_gradient_before_wall_fe']) == len(r['fd_gradients'])
        assert q['train_wall_fe'] == sorted(q['train_wall_fe']) and q['fd_gradient_before_wall_fe'] == sorted(q['fd_gradient_before_wall_fe'])
        own = [max(x for x in q['train_wall_fe'] if x <= g_) for g_ in q['fd_gradient_before_wall_fe']]  # the evaluation each gradient follows
        assert len(set(own)) == len(own)  # at most one gradient per training evaluation (it may start after that evaluation's validation)
    if os.environ.get(env):
        sys.path.insert(0, str(V / 'figures/src')); import extract_timeline
        assert extract_timeline.extract(os.environ[env]) == json.loads((V / f'figures/src/data/{k}/timeline.json').read_text()), k
        print(f'PASS: {k} timeline re-derived from the raw traces (exact match).')
TLB = {q['file']: q for q in TL['exp2b']}; TLC = {q['file']: q for q in TL['exp2c']}
TLR = {('b', r['method'], r['space'], r['start']): TLB[r['file']] for r in XB if r['repeat'] == 1}
TLR.update({('c', r['method'], r['space'], r['start']): TLC[r['file']] for r in X})
RUN = {('b', r['method'], r['space'], r['start']): r for r in XB if r['repeat'] == 1}; RUN.update({('c', m, sp, s): R[(m, sp, s)] for (m, sp, s) in R})
PT = (('a', 'T90'), ('b', 'T99'))
# The cold-fallback gradient: at 110 units, after both of that run's crossings.
cr_run = cold[0][0]; cr_i = cr_run['fd_gradients'].index(cold[0][1]); cr_t = TLR[('c', 'W', 'a', 'lhs2')]['fd_gradient_before_wall_fe'][cr_i]
assert cr_run['file'] == TLR[('c', 'W', 'a', 'lhs2')]['file'] and round(cr_t, -1) == 110 and cr_t > max(cr_run['attainment'][tg]['wall_fe'] for tg in ('T50', 'T90'))


def upto(ses, m, sp, s, tg):  # training evaluations and gradient requests before the crossing; non-validation time to it
    r, q = RUN[(ses, m, sp, s)], TLR[(ses, m, sp, s)]; a = r['attainment'][tg]
    n = sum(x < a['wall_fe'] for x in q['train_wall_fe']); g_ = sum(x < a['wall_fe'] for x in q['fd_gradient_before_wall_fe'])
    rest = a['wall_fe'] - sum(r['validation_charges_seconds'][:a['validation'] + 1]) / r['unit_seconds']
    return n, g_, rest


share = {(ses, m, sp): st.median(upto(ses, m, sp, s, tg)[1] / upto(ses, m, sp, s, tg)[0] for s in S) for ses, m in (('b', 'F'), ('c', 'W')) for sp, tg in PT}
assert [round(100 * share[k], 1) for k in (('b', 'F', 'a'), ('b', 'F', 'b'), ('c', 'W', 'a'), ('c', 'W', 'b'))] == [83.3, 76.9, 83.3, 71.4]
assert all(upto(ses, 'Ap', sp, s, tg)[1] == 0 for ses in 'bc' for sp, tg in PT for s in S)
nvm = {(ses, m, sp): gm([upto(ses, m, sp, s, tg)[2] / upto(ses, 'Ap', sp, s, tg)[2] for s in S]) for ses, m in (('b', 'F'), ('c', 'W')) for sp, tg in PT}
assert [round(nvm[k], 2) for k in (('b', 'F', 'a'), ('b', 'F', 'b'), ('c', 'W', 'a'), ('c', 'W', 'b'))] == [5.58, 10.07, 3.23, 6.10]
tsolve = lambda runs_, m, sp: st.median((r['category_seconds']['forward'] + r['category_seconds']['line_search']) / r['unit_seconds'] / (r['train_events'] + r['fd_train_events'])
                                        for r in runs_ if r['method'] == m and r['space'] == sp)
adjB = {sp: st.median(r['category_seconds']['adjoint'] / r['unit_seconds'] / r['gradient_train_events'] for r in XB if r['method'] == 'Ap' and r['space'] == sp) for sp in 'ab'}
gcB = {sp: st.median(g['cost_fe'] for r in XB if r['method'] == 'F' and r['space'] == sp for g in r['fd_gradients'] if g['evaluated'] == g['components'] == (8 if sp == 'a' else 18)) for sp in 'ab'}
C = {('b', sp): (tsolve(XB, 'F', sp), gcB[sp], tsolve(XB, 'Ap', sp), adjB[sp]) for sp in 'ab'}
C.update({('c', sp): (bc[sp], gcost[sp], tsolve(X, 'Ap', sp), adj[sp]) for sp in 'ab'})
assert [round(C[('c', sp)][2], 3) for sp in 'ab'] == [.220, .238] and [round(C[('c', sp)][0], 3) for sp in 'ab'] == [.220, .233]
pe_ = {k: (c_[0] + c_[1]) / (c_[2] + c_[3]) for k, c_ in C.items()}
assert [round(pe_[k], 1) for k in (('b', 'a'), ('b', 'b'))] == [7.4, 14.7]  # F_w's per-evaluation ratio (4.34, 8.75) is not printed
mm = {'b': 'F', 'c': 'W'}
exp_ = {k: (c_[0] + share[(k[0], mm[k[0]], k[1])] * c_[1]) / (c_[2] + c_[3]) for k, c_ in C.items()}
assert [round(exp_[k], 1) for k in (('b', 'a'), ('b', 'b'), ('c', 'a'), ('c', 'b'))] == [6.3, 11.5, 3.8, 6.5]
gap = [exp_[k] / nvm[(k[0], mm[k[0]], k[1])] - 1 for k in C]; assert (round(100 * min(gap)), round(100 * max(gap))) == (6, 16)
# From the printed inputs (shares to 0.1%, costs as printed), the same estimates to within rounding.
Pp = {('b', 'a'): (.240, 1.90, .232, .057, .833), ('b', 'b'): (.236, 4.25, .245, .061, .769), ('c', 'a'): (.220, .97, .220, .054, .833), ('c', 'b'): (.233, 2.36, .238, .059, .714)}
assert all(round((x[0] + x[4] * x[1]) / (x[2] + x[3]), 1) == round(exp_[k], 1) for k, x in Pp.items())
assert all(round((x[0] + x[1]) / (x[2] + x[3]), 1) == round(pe_[k], 1) for k, x in Pp.items() if k[0] == 'b')
# A''s evaluations before the crossings cost more than its whole-run medians (the model omits per-evaluation overhead).
ape = {(ses, sp): st.median(upto(ses, 'Ap', sp, s, tg)[2] / upto(ses, 'Ap', sp, s, tg)[0] for s in S) for ses in 'bc' for sp, tg in PT}
assert (round(min(ape.values()), 2), round(max(ape.values()), 2)) == (.32, .34)
wr = [c_[2] + c_[3] for c_ in C.values()]; assert (round(min(wr), 2), round(max(wr), 2)) == (.27, .31)
# v16: both arms' to-crossing cost per evaluation against the model (no one-sided closure).
adev = [ape[k] / (c_[2] + c_[3]) - 1 for k, c_ in C.items()]; assert (round(100 * min(adev)), round(100 * max(adev))) == (10, 17)
fdev = {k: st.median(upto(k[0], mm[k[0]], k[1], s, dict(PT)[k[1]])[2] / upto(k[0], mm[k[0]], k[1], s, dict(PT)[k[1]])[0] for s in S) / (c_[0] + share[(k[0], mm[k[0]], k[1])] * c_[1]) - 1 for k, c_ in C.items()}
assert [round(100 * fdev[k]) for k in (('b', 'a'), ('c', 'a'), ('b', 'b'), ('c', 'b'))] == [7, 7, -5, 1]
assert min(adev) > max(fdev.values())  # the model errs most for A'
assert 'come within 4\\% of the measurements' not in txt and 'accounts for the 6--16' not in txt
# F_w's single repeat: fixed by its protocol because F's repeats had agreed to about 1%.
assert "#1218's repeats agreed to about 1%, and the estimand uses repeat 1" in pe['issue']['body']
RB2 = {(r['method'], r['space'], r['start'], r['repeat']): r for r in XB}
rep = {m: [RB2[(m, sp, s, 2)]['attainment'][tg]['wall_fe'] / RB2[(m, sp, s, 1)]['attainment'][tg]['wall_fe'] - 1 for sp, tg in PT for s in S] for m in ('F', 'Ap')}
assert round(100 * st.median(abs(x) for x in rep['F']), 1) == 1.0  # v16: F's own repeats (pooled with A' it would be 1.2)
assert round(100 * st.median(abs(x) for x in rep['F'] + rep['Ap']), 1) == 1.2 and '1.2\\% per primary crossing' not in tex
# Warm stencil evaluations with fewer Newton iterations than field solves (some solves took no update).
NM = ('MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'Sr1')
zu = {}
for sp in 'ab':
    GG = [g for r in X if r['method'] == 'W' and r['space'] == sp for g in r['fd_gradients']]
    assert all(min(g['newton']) >= 60 for g in GG if g['evaluated'] != g['components'])  # the two budget-truncated last gradients
    low = [(i, x) for g in GG if g['evaluated'] == g['components'] for i, x in enumerate(g['newton']) if x < 60]
    zu[sp] = (len(low), sum(len(g['newton']) for g in GG), min(x for _, x in low), {NM[i] for i, _ in low})
assert zu['a'][:3] == (58, 1810, 40) and zu['b'][:3] == (20, 648, 40) and zu['a'][3] | zu['b'][3] == {'SW1', 'SW4', 'Sr1'} == {f[2] for f in objf}
assert zu['a'][3] == {'SW1', 'SW4', 'Sr1'} and zu['b'][3] == {'SW4', 'Sr1'} and round(100 * (zu['a'][0] + zu['b'][0]) / (zu['a'][1] + zu['b'][1]), 1) == 3.2
# Space-A interior SW3 (mean gradient): truncation dominates at the frozen step for F_w as for F.
SWa, SWf = rec['fd-step-sweep-L1-warm'], json.loads((V / 'figures/src/data/exp2b/fd-step-sweep-L1.json').read_text())


def slope(sw, h):
    rr_ = sw['record']; assert rr_['space'] == 'a' and rr_['point'] == 'interior'
    c = next(c for c in rr_['components'] if c['name'] == 'SW3'); q = {(x['step'], x['side']): x for x in c['stencil']}[(h, '+')]
    return ((q['mean_full'] - rr_['base']['mean_full']) / q['dx'] - c['schemes'][0]['dmean_full']['adjoint']) / h


assert f'{slope(SWa, 1e-6):.1e}' == '-1.7e+03' and f'{slope(SWf, 1e-6):.1e}' == '-1.9e+03' and f'{slope(SWa, 1e-5):.1e}' == '-1.8e+03'
assert slope(SWa, 1e-7) > 0 and all(slope(SWf, h) < 0 for h in (1e-5, 1e-6, 1e-7))  # v16: F_w's SW3 error changes sign at 1e-7, F's does not
assert 'mean gradient, SW3\'s forward error per unit step, $1.9' not in tex  # v16: App. D compares magnitudes
# Space-B interior: the three failing components share one signed forward error at the frozen step.
e6 = [fe[(n, 1e-6)] for n in names]; assert f'{st.mean(e6):.2e}' == '-3.11e-03' and (max(e6) - min(e6)) / abs(st.mean(e6)) < .003
for phrase in [
    "as its protocol fixed in advance because FD-cold's repeats had agreed to about 1\\% (a median 1.0\\% per primary crossing time) and the estimand uses repeat~1",
    "The coarse forward that every coarse evaluation costs is taken per session: as a median over runs of coarse-simulation time per coarse forward, it is 0.240 and 0.236 units for FD-cold against 0.232 and 0.245 for its A$'$, and 0.220 and 0.238 for FD-warm's A$'$",  # v22: in Appendix D
    "so for FD-warm we take the median charge of one base coarse forward, 0.220 and 0.233 units (0.216 and 0.233 for its A$'$ on the same measure)",
    "A$'$'s primary crossing times in the FD-cold session are 0.94--1.04 times those of the main-study A from the same start and repeat (0.97--1.05 in the FD-warm session)",
    '4 times (NOMAD) to over 100 times (NSGA2, which converged to a slightly worse design) as expensive as adjoint-driven ones',
    'up to the primary crossing FD-cold and FD-warm request one at a median 83.3\\% of their evaluations in S8 and at 76.9\\% and 71.4\\% in S18',
    'the expected ratios are 6.3 and 11.5 for FD-cold and 3.8 and 6.5 for FD-warm, against the measured 5.58 and 10.07 and 3.23 and 6.10 once validation is excluded',
    'discusses the 6--16\\% excess', 'The two-factor estimates of Section~\\ref{sec:exp2-fd} run 6--16\\% above the measured ratios. Their cost model counts only coarse forwards, adjoints and stencil solves, so it omits the per-evaluation overhead of both arms',
    'an A$\'$ evaluation costs a median 0.32 to 0.34 units, overhead included, against the cost model\'s 0.27 to 0.31, an excess of 10--17\\%',
    'differ from their cost model by +7\\% for both FD-cold and FD-warm in S8, and by $-5$\\% (FD-cold) and +1\\% (FD-warm) in S18',
    "(a forward error per unit step of $-1.7\\times10^{3}$\\,N\\,m/m$^2$, against FD-cold's $-1.9\\times10^{3}$)",
    'the three failing components share one signed forward error, $-3.11\\times10^{-3}\\,\\mathrm{N^2\\,m^2/m}$ at a step of $10^{-6}$\\,m to within 0.3\\%, which grows about tenfold per decade of decreasing step',
    "58 of S8's 1,810 warm stencil evaluations and 20 of S18's 648, 3.2\\% in all, recorded fewer than 60 Newton iterations, as few as 40",
    'They perturb SW1, SW4 or Sr1 in S8 and SW4 or Sr1 in S18, all among the objective-gradient components that fail the L1 check',
    "unlike FD-cold's, this error changes sign at $10^{-7}$\\,m", 'In detail, three of the four failures are on small objective-gradient components',
    'The freeze comment recorded the result of the L1 accuracy gate and fixed the warm-start source']:
    assert phrase in txt, phrase
fz = pe['freeze_comment']['body']; assert 'Warm-start source' in fz and 'gate is **PASSED**' in fz
print('PASS: timelines (60 traces) bound to the extracts; to-crossing gradient shares, both arms\' two-factor cost estimates and their residual; F_w\'s single repeat; zero-update warm stencils; SW3 truncation and the common space-B offset.')

# v18: the gradient/optimizer decomposition (Table decomp), the time split (Fig. decomp), the cost table and the
# reader-facing additions, recomputed here from the extracts and timelines without importing render_decomp.py.
MR = {(r['method'], r['space'], r['start']): r for r in M['runs'] if r['repeat'] == 1}
MSUP = {u['file']: u for u in json.loads((V / 'figures/src/data/exp2/supplement.json').read_text())['runs']}
tA = lambda ses, m, sp, s, tg: (MR[(m, sp, s)] if ses == 'm' else RUN[(ses, m, sp, s)])['attainment'][tg]['wall_fe']
BS = {sp: [s for s in S if MR[('B', sp, s)]['attainment'][tg]['attained']] for sp, tg in PT}
assert BS == {'a': S, 'b': [s for s in S if s != 'lhs3']}
FAC = {}
for sp, tg in PT:
    ss = BS[sp]; q = lambda n, d: gm([tA(*n, sp, s, tg) / tA(*d, sp, s, tg) for s in ss])
    FAC[sp] = {'BA': q(('m', 'B'), ('m', 'A')), 'WA': q(('c', 'W'), ('c', 'Ap')), 'BW': q(('m', 'B'), ('c', 'W')),
               'FA': q(('b', 'F'), ('b', 'Ap')), 'BF': q(('m', 'B'), ('b', 'F'))}
r2 = lambda x: f'{x:.2f}'
assert [r2(FAC[sp][k]) for sp in 'ab' for k in ('BA', 'WA', 'BW', 'FA', 'BF')] == ['3.10', '1.71', '1.76', '2.47', '1.22', '7.79', '2.68', '2.92', '4.10', '1.91']
assert [r2(FAC[sp]['WA'] * FAC[sp]['BW']) for sp in 'ab'] == [r2(FAC[sp]['FA'] * FAC[sp]['BF']) for sp in 'ab'] == ['3.02', '7.82']
dt = (V / 'tables/decomp.tex').read_text()
for lab, k in (('(B/A, main study)', 'BA'), ("(FD-warm/A$'$)", 'WA'), ('(B/FD-warm)', 'BW'), ("(FD-cold/A$'$)", 'FA'), ('(B/FD-cold)', 'BF')):
    assert f"{lab} & {r2(FAC['a'][k])} & {r2(FAC['b'][k])}\\\\" in dt, lab
assert dt.count('Product & 3.02 & 7.82\\\\') == 2
lo_g = [min(FAC[sp]['WA'], FAC[sp]['FA']) for sp in 'ab']; hi_g = [max(FAC[sp]['WA'], FAC[sp]['FA']) for sp in 'ab']
lo_o = [min(FAC[sp]['BW'], FAC[sp]['BF']) for sp in 'ab']; hi_o = [max(FAC[sp]['BW'], FAC[sp]['BF']) for sp in 'ab']
assert list(map(r2, lo_g + hi_g)) == ['1.71', '2.68', '2.47', '4.10'] and list(map(r2, lo_o + hi_o)) == ['1.22', '1.91', '1.76', '2.92']
assert [f'{x:.1f}' for x in lo_o + hi_o] == ['1.2', '1.9', '1.8', '2.9']  # the abstract's 1.2 to 1.8 and 1.9 to 2.9


def tsplit(ses, m, sp, s, tg):  # validation, gradient, rest of the time to one crossing, in the session's unit
    r = MR[(m, sp, s)] if ses == 'm' else RUN[(ses, m, sp, s)]; a = r['attainment'][tg]; u = r['unit_seconds']
    vc = MSUP[r['file']]['validation_charges_seconds'] if ses == 'm' else r['validation_charges_seconds']
    val = sum(vc[:a['validation'] + 1]) / u; grad = 0.0
    if m != 'B':
        qq = TLR[(ses, m, sp, s)]; n = sum(x < a['wall_fe'] for x in qq['train_wall_fe']); g_ = sum(x < a['wall_fe'] for x in qq['fd_gradient_before_wall_fe'])
        grad = n * r['category_seconds']['adjoint'] / r['gradient_train_events'] / u if m == 'Ap' else sum(x['cost_fe'] for x in r['fd_gradients'][:g_])
    return a['wall_fe'], val, grad


MS = {}
for sp, tg in PT:
    for ses, m in (('c', 'Ap'), ('b', 'Ap'), ('c', 'W'), ('b', 'F'), ('m', 'B')):
        xs = [tsplit(ses, m, sp, s, tg) for s in BS[sp]]; MS[(ses, m, sp)] = tuple(sum(x[i] for x in xs) / len(xs) for i in range(3))
sh = lambda k: MS[k][2] / MS[k][0]
assert [f'{MS[(ses, m, sp)][2]:.2f}' for sp in 'ab' for ses, m in (('c', 'Ap'), ('c', 'W'), ('b', 'F'))] == ['0.30', '4.37', '8.72', '0.51', '14.91', '27.47']
assert [round(100 * sh((ses, m, sp))) for sp in 'ab' for ses, m in (('c', 'W'), ('b', 'F'))] == [45, 62, 65, 77]
assert [round(100 * sh(('c', 'Ap', sp))) for sp in 'ab'] == [5, 6] and all(round(100 * sh(('b', 'Ap', sp))) in (5, 6) for sp in 'ab')
nong = {sp: [MS[k][0] - MS[k][2] for k in (('c', 'Ap', sp), ('c', 'W', sp), ('b', 'F', sp))] for sp in 'ab'}  # validation + rest
assert [f'{min(nong[sp]):.1f}--{max(nong[sp]):.1f}' for sp in 'ab'] == ['5.3--5.4', '8.0--8.1']
assert all(abs(MS[('b', 'Ap', sp)][0] / MS[('c', 'Ap', sp)][0] - 1) < .01 for sp in 'ab')  # FD-cold's own A' within 1%
# Per-gradient and end-to-end scaling from 8 to 18 parameters; the common target T90.
pgw = {sp: gcost[sp] / adj[sp] for sp in 'ab'}; pgc = {sp: gcB[sp] / adjB[sp] for sp in 'ab'}
assert (round(pgw['a']), round(pgw['b']), round(pgc['a']), round(pgc['b'])) == (18, 40, 33, 70)
assert (f"{pgw['b'] / pgw['a']:.1f}", f"{pgc['b'] / pgc['a']:.1f}") == ('2.2', '2.1')
e2e = {(ses, m, sp): gm([tA(ses, m, sp, s, tg) / tA(ses, 'Ap', sp, s, tg) for s in S]) for ses, m in (('c', 'W'), ('b', 'F')) for sp, tg in PT}
assert [r2(e2e[k]) for k in (('c', 'W', 'a'), ('c', 'W', 'b'), ('b', 'F', 'a'), ('b', 'F', 'b'))] == ['1.71', '2.70', '2.47', '4.11']
assert (f"{e2e[('c', 'W', 'b')] / e2e[('c', 'W', 'a')]:.1f}", f"{e2e[('b', 'F', 'b')] / e2e[('b', 'F', 'a')]:.1f}") == ('1.6', '1.7')
t90 = {(ses, m): gm([tA(ses, m, 'b', s, 'T90') / tA(ses, 'Ap', 'b', s, 'T90') for s in S]) for ses, m in (('c', 'W'), ('b', 'F'))}
assert (r2(t90[('c', 'W')]), r2(t90[('b', 'F')])) == ('2.59', '3.86')
assert (min(pgw.values()) >= 17.5 and max(pgc.values()) < 70.5) and r2(min(e2e.values())) == '1.71' and r2(max(e2e.values())) == '4.11'
# Units in minutes (Fig. 5) and the cost table.
us = sorted({r['unit_seconds'] for r in M['runs']} | {r['unit_seconds'] for r in XB} | {r['unit_seconds'] for r in X})
assert [f'{u:.2f}' for u in us] == ['66.66', '69.09', '70.94'] and (f'{us[0] / 60:.2f}', f'{us[-1] / 60:.2f}') == ('1.11', '1.18')
trf = [C[k][0] for k in C] + [C[k][2] for k in C] + [bcA['a'], bcA['b']]
assert (f'{min(trf):.3f}', f'{max(trf):.3f}') == ('0.216', '0.245')
ad = [adj['a'], adj['b'], adjB['a'], adjB['b']]; assert (f'{min(ad):.3f}', f'{max(ad):.3f}') == ('0.054', '0.061')
for row in ('Validation forward & L2, order 93 & 0.87 (range 0.55--1.10)\\\\', 'Coarse forward & L1, order 45 & 0.216--0.245\\\\',
            'all $n$ components & L1, order 45 & 0.054--0.061\\\\', f"cold stencil solves & L1, order 45 & {gcB['a']:.2f} and {gcB['b']:.2f}\\\\",
            f"warm stencil solves & L1, order 45 & {gcost['a']:.2f} and {gcost['b']:.2f}\\\\"):
    assert row in tex, row
# Final-check crossings named in the abstract, the 50 optimizer-contrast cells, and the geometry inputs.
ncr = {k: sum(ro.startswith('crossing/') for d in json.loads((V / f'figures/src/data/{k}/refinement.json').read_text())['designs'].values() for ro in d['roles']) for k in ('exp2', 'exp2b', 'exp2c')}
assert ncr == {'exp2': 25, 'exp2b': 10, 'exp2c': 10}
cells = [(sp, tg, s) for sp in 'ab' for tg in ('T50', 'T90', 'T99') for s in S if any(r['attainment'][tg]['attained'] for r in M['runs'] if r['space'] == sp)]
assert 2 * len(cells) == 50
CP = V / 'figures/src/data/cp_table.json'
assert sha(CP) == '03a986f7b26de4f16c84182e5dc57a1380ef1ba1bf9293e638b5522ef775ab77'
EM = json.loads((V / 'evidence-manifest.json').read_text()); EMf = EM['files'] if isinstance(EM, dict) and 'files' in EM else EM
assert any(isinstance(e, dict) and e.get('sha256') == sha(CP) for e in EMf)
for phrase in [
    'in all 45 of the 50 paired comparisons where both succeeded, by geometric means of 3.10 and 7.79 at the primary targets, 90\\% and 99\\% reductions of the ripple objective with 8 and 18 parameters',
    'Every target crossing selected for final checks, 25 of the main study\'s 111 and 10 in each ablation, kept its target on a finer mesh',
    'Of SLSQP\'s 3.10-fold (S8) and 7.79-fold (S18) advantage over COBYQA, the gradient accounts for a factor of 1.71--2.47 and 2.68--4.10, and the optimizer for 1.22--1.76 and 1.91--2.92',
    'together 5.3--5.4 units in S8 and 8.0--8.1 in S18', "A$'$'s adjoints take about 5\\% of its time to the target, 0.30 and 0.51 units, against 4.37 and 14.91 units (45\\% and 65\\%) for FD-warm's stencils and 8.72 and 27.47 units (62\\% and 77\\%) for FD-cold's",
    "A free gradient would therefore shorten A$'$'s time to the target by only 5--6\\%", 'the per-gradient cost ratios of 18--70',
    'the per-gradient cost ratio grows 2.2-fold against FD-warm and 2.1-fold against FD-cold', '1.6-fold (1.71 to 2.70) and 1.7-fold (2.47 to 4.11)',
    "FD-warm/A$'$ is 1.71 in S8 and 2.59 in S18, and FD-cold/A$'$ 2.47 and 3.86", '(1 unit is 1.11 to 1.18 minutes)',
    'median cold validation forward measured on the same host at the same concurrency (66.66--70.94\\,s in the three sessions)',
    'which move 55 control points of the rotor surface and the stator bore',
    'C.D. wrote the four-layer operator specification that GEODE realizes \\citep{dunnavant2026whiteroom}', 'R.W. ran the work',
    '\\author{Robb Walters\\\\ {\\normalsize 2AM Logic} \\and Crutcher Dunnavant\\\\ {\\normalsize ZSpaceLabs.ai}}']:  # v19: one affiliation per author
    assert phrase in txt, phrase
assert 'github.com/crutcher/palace_whiteroom' in (V / 'refs.bib').read_text()
for bad in ('comparable factors', 'space A', 'space B', 'space-A', 'space-B', 'F$_w$'):
    assert bad not in txt, bad
print('PASS: v18 decomposition (gradient and optimizer factors, Table decomp), time split and gradient shares, scaling, cost table, units in minutes, final-check crossings, geometry input and authorship recomputed.')

# Package text agrees with the paper, the deposit record and the verifier set (v15); v17 adds the archive manifest.
PKG = 'motor-gradient-optimization.22'
DR = json.loads((V / 'figures/src/data/deposit/deposit-record.json').read_text()); README = _Norm((V / 'README.md').read_text())
assert f"{DR['bytes']:,}" in README and DR['sha256'] in README and f"{DR['bytes']:,}" in tex and DR['sha256'][:32] in tex and DR['sha256'][32:] in tex
assert DR['sha256'] == '1dfbf3883a9f1f3d8ddc44f67ce594b88291ec235a154337d24efa07a1b98b07' and DR['bytes'] == 2790582024 and DR['uploaded'] is False
assert DR['files'] == {'exp2-main': 5453, 'exp2-fd-ablation': 2118, 'exp2-fd-warm': 1180, 'refinement/exp2-main': 1075,
                       'refinement/exp2-fd-ablation': 451, 'refinement/exp2-fd-warm': 427, 'refinement/binary-equivalence': 58}
for stale in ('2de2132c', '2,179,756,244', 'raw traces of both studies', 'strongest', 'runs `verify_exp2.py` and `verify_exp2b.py`',
              'a70a0699261e40573187a0e4007e8d4b50f608be707a31a59aaf87c650203f12', '2,609,375,627'):
    assert stale not in README and stale not in tex, stale
assert all(f"{DR['files'][k]:,}" in README for k in ('exp2-main', 'exp2-fd-ablation', 'exp2-fd-warm'))
# The archive manifest: a copy in the package, bound to the deposit record; every inventory row and every
# archived refinement, selection and binary-comparison file the extracts name is listed under its digest.
MF = V / 'figures/src/data/deposit/MANIFEST.sha256'; assert sha(MF) == DR['manifest_sha256']
man = dict(reversed(l.split('  ', 1)) for l in MF.read_text().splitlines()); assert len(man) == DR['members'] - 1
for invp in (V / 'figures/src/data/exp2/provenance/exp2-main-inventory-0650f4ee.sha256.txt',
             V / 'figures/src/data/exp2b/provenance/exp2-fd-ablation-inventory-c2bc65ff.sha256.txt', INV):
    rows = [l.split('  ', 1) for l in invp.read_text().splitlines()]
    assert all(man.get(rel) == d for d, rel in rows) and man[f'inventories/{rows[0][1].split("/")[0]}/{invp.name}'] == sha(invp)
nres = 0
for k, fo in (('exp2', 'exp2-main'), ('exp2b', 'exp2-fd-ablation'), ('exp2c', 'exp2-fd-warm')):
    rf = json.loads((V / f'figures/src/data/{k}/refinement.json').read_text())
    if 'cohort_sha256' in rf:
        assert man[f'refinement/{fo}/bundle/cohort.json'] == rf['cohort_sha256']
    for key, d in rf['designs'].items():
        for ch, c in d['checks'].items():
            assert man[f'refinement/{fo}/results/{key[:12]}/{ch}/result.json'] == c['result_sha256']; nres += 1
    if k != 'exp2':
        assert man[f'refinement/{fo}/selection.json'] == json.loads((V / f'figures/src/data/{k}/selection-check.json').read_text())['selection_sha256']
assert nres == 213 + 90 + 84
for f in sorted((BE / 'new').glob('*/*.json')):
    assert man[f'refinement/binary-equivalence/results/{f.parent.name}/{f.stem}/result.json'] == sha(f)
assert man['refinement/binary-equivalence/icem_final_refinement.c75fe847'] == bins['icem_final_refinement (rebuilt at 63b3e3d8)']
if RAWROOT:  # every file of the unpacked archive, and nothing else, matches the manifest
    assert (RAWROOT / 'MANIFEST.sha256').read_bytes() == MF.read_bytes()
    got = {q.relative_to(RAWROOT).as_posix(): q for q in RAWROOT.rglob('*') if q.is_file() and q.name != 'MANIFEST.sha256'}
    assert set(got) == set(man) and not any(q.is_symlink() for q in RAWROOT.rglob('*'))
    for rel, q in got.items():
        h = hashlib.sha256()
        with open(q, 'rb') as fh:
            for chunk in iter(lambda: fh.read(1 << 24), b''):
                h.update(chunk)
        assert h.hexdigest() == man[rel], rel
    print(f'PASS: all {len(got):,} files of the unpacked archive match its manifest (the package copy, bound to the deposit record).')
vs = sorted(p.name for p in V.glob('verify*.py')); assert vs == ['verify.py', 'verify_exp2.py', 'verify_exp2b.py', 'verify_exp2c.py']
vtxt = (V / 'verify.py').read_text(); assert all(f"runpy.run_path(str(V/'{n}')" in vtxt for n in vs[1:])
assert 'four standard-library verifiers (one entry point that runs the other three)' in txt and 'three standard-library verifiers' not in txt
assert '`verify.py` runs `verify_exp2.py`, `verify_exp2b.py` and `verify_exp2c.py`' in README and 'four standard-library verifiers' in README
for phrase in ['1.71 and 2.70 times sooner than warm-started finite differences', '2.47 and 4.11', '1.71-2.47 (S8) and 2.68-4.10 (S18)', '1.22-1.76 and 1.91-2.92', 'from 45-77% to about 5%', 'Robb Walters (2AM Logic) and Crutcher Dunnavant (ZSpaceLabs.ai)',
               'depends on the finite-difference variant', '10.5281/zenodo.23580550', '10.5281/zenodo.23580548', PKG, 'MOTOR_RAW_ROOT']:
    assert phrase in README, phrase
assert 'PENDING' not in README and 'Independent Researcher' not in README  # v19: no pending marker remains
assert 'reserved DOI' not in README and 'deposited at DOI [10.5281/zenodo.23580550]' in README  # v20: as the data statement
assert 'ones at the same optimum' not in txt  # v17: the NSGA2 end of Yu et al.'s range did not reach the common optimum
assert 'given the unpacked sessions' not in txt and 'Given the unpacked archive, the package\'s verifiers re-derive the run, timeline and final-check extracts of all three sessions' in txt
# Repository layout: .anvil.json sits at the thread root; a package downloaded on its own has none (v17: optional).
AJp = V.parent / '.anvil.json'
if AJp.exists() and V.name == PKG:
    AJ = json.loads(AJp.read_text())['artifact_verify']
    assert AJ['cwd'] == V.name and all(c.split()[-1] in vs for c in AJ['commands'])
    lay = '.anvil.json and '
else:
    print('NOTE: no thread-root .anvil.json beside this package (downloaded on its own); the artifact_verify binding is not checked.'); lay = ''
PG = json.loads((V / '_progress.json').read_text()); rv = PG['phases']['revise']
assert (rv['state'] == 'in_progress' and 'completed' not in rv) or (rv['state'] == 'done' and rv['started'] <= rv['completed'])
assert PG['metadata']['iteration'] == int(PKG.split('.')[-1]) == PG['metadata']['revised_from'] + 1
print(f'PASS: package text: README archive digest, size and member counts match the deposit record and the paper; archive manifest bound to the deposit record, the inventories and every archived result the extracts name; four verifiers named and chained; README headline and factors match the paper; {lay}_progress.json consistent.')
