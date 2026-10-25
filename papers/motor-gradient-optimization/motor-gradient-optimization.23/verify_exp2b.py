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
From v17, MOTOR_RAW_ROOT=<unpacked>/motor-paper-raw-traces (the unpacked deposit
archive) sets all three from the archive layout.
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
if os.environ.get('MOTOR_RAW_ROOT'):  # v17: the unpacked deposit archive supplies every raw input
    _R = Path(os.environ['MOTOR_RAW_ROOT'])
    os.environ.setdefault('EXP2B_SESSION', str(_R / 'exp2-fd-ablation/session'))
    os.environ.setdefault('EXP2B_REFINE', str(_R / 'refinement/exp2-fd-ablation'))
    os.environ.setdefault('EXP2B_SELECTION', str(_R / 'refinement/exp2-fd-ablation/selection.json'))
raw = os.environ.get('EXP2B_SESSION'), os.environ.get('EXP2B_REFINE')
if all(raw):
    sys.path.insert(0, str(V / 'figures/src')); import extract_exp2b
    runs2, ref2 = extract_exp2b.extract(*raw)
    assert runs2 == json.loads((D / 'runs.json').read_text()) and ref2 == RF, 'extract differs from raw re-derivation'
    for r in X:
        assert sha(Path(raw[0]) / r['file']) == r['sha256']
    if os.environ.get('EXP2B_SELECTION'):
        sp = Path(os.environ['EXP2B_SELECTION']); assert sha(sp) == json.loads((D / 'selection-check.json').read_text())['selection_sha256']
        import subprocess, tempfile  # v17: the whole extractor, selection check included, reproduces the package files byte for byte
        with tempfile.TemporaryDirectory() as td:
            subprocess.run([sys.executable, '-I', '-B', str(V / 'figures/src/extract_exp2b.py'), raw[0], raw[1], td, str(sp)], check=True, capture_output=True)
            for n in ('runs.json', 'refinement.json', 'selection-check.json'):
                assert (Path(td) / n).read_bytes() == (D / n).read_bytes(), n
        print('PASS: Exp 2b selection check re-derived from the independent selection output (runs, refinement and selection-check files byte-identical).')
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
_bam = [gm([MA[('B', 'a', s_, 1)]['attainment'][tg]['wall_fe'] / MA[('A', 'a', s_, 1)]['attainment'][tg]['wall_fe'] for s_ in S]) for tg in ('T50', 'T90')]
assert (round(min(_bam), 2), round(max(_bam), 2)) == (3.10, 3.25)  # the eight-parameter ratios set beside Yu et al. 2018
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
# v13: cost per gradient in session units (unit = median cold validation solve, 66.66 s).
gcost = {sp: st.median(g['cost_fe'] for r in X if r['method'] == 'F' and r['space'] == sp for g in r['fd_gradients']
                       if g['evaluated'] == g['components'] == (8 if sp == 'a' else 18)) for sp in 'ab'}
assert (round(gcost['a'], 2), round(gcost['b'], 2)) == (1.90, 4.25) and (round(gcost['a'] / 8, 2), round(gcost['b'] / 18, 2)) == (.24, .24)
adjg = {sp: st.median(r['category_seconds']['adjoint'] / r['unit_seconds'] / r['gradient_train_events'] for r in X if r['method'] == 'Ap' and r['space'] == sp) for sp in 'ab'}
assert (round(adjg['a'], 3), round(adjg['b'], 3)) == (.057, .061)
tsolve = [st.median((r['category_seconds']['forward'] + r['category_seconds']['line_search']) / r['unit_seconds'] / (r['train_events'] + r['fd_train_events'])
                    for r in X if r['method'] == m and r['space'] == sp) for m in ('F', 'Ap') for sp in 'ab']
assert (round(min(tsolve), 2), round(max(tsolve), 2)) == (.23, .24)
gfrac = {sp: st.median(len(r['fd_gradients']) / r['train_events'] for r in X if r['method'] == 'F' and r['space'] == sp) for sp in 'ab'}
assert (round(100 * gfrac['a']), round(100 * gfrac['b'])) == (68, 77)
assert round(U, 2) == 66.66
# v14: every training solve, stencils included, starts Newton-cold (all 40 traces).
assert {r['training_warm'] for r in X} == {'Cold'} and len(X) == 40
# v14: the per-gradient ratio chain, from the data and from the printed inputs.
tsm = {(m, sp): st.median((r['category_seconds']['forward'] + r['category_seconds']['line_search']) / r['unit_seconds'] / (r['train_events'] + r['fd_train_events'])
                          for r in X if r['method'] == m and r['space'] == sp) for m in ('F', 'Ap') for sp in 'ab'}
assert [round(tsm[k], 3) for k in (('F', 'a'), ('F', 'b'), ('Ap', 'a'), ('Ap', 'b'))] == [.240, .236, .232, .245]
assert all(r['gradient_train_events'] == r['train_events'] for r in X if r['method'] == 'Ap')
P = {'g': (1.90, 4.25), 'adj': (.057, .061), 'tF': (.240, .236), 'tA': (.232, .245), 'fr': (.68, .77)}
pg = [round(P['g'][i] / P['adj'][i]) for i in (0, 1)]; assert pg == [33, 70]
ge = [round((P['tF'][i] + P['g'][i]) / (P['tA'][i] + P['adj'][i]), 1) for i in (0, 1)]; assert ge == [7.4, 14.7]
ex = [round((P['tF'][i] + P['fr'][i] * P['g'][i]) / (P['tA'][i] + P['adj'][i]), 1) for i in (0, 1)]; assert ex == [5.3, 11.5]  # whole-run shares; v15 prints the to-crossing check instead (verify_exp2c.py)
assert 'expected ratio is 5.3 and 11.5' not in (V / 'optimization.tex').read_text()
for i, sp in enumerate('ab'):  # the printed inputs are the data's own medians
    assert round(gcost[sp], 2) == P['g'][i] and round(adjg[sp], 3) == P['adj'][i] and round(gfrac[sp], 2) == P['fr'][i]
# v14: Appendix H's warm-started FD sweeps cost 0.30-0.40 of a cold forward.
cr = []
for cp in (V / 'figures/src/data/gradient/runs').glob('cost*.json'):
    m_ = json.loads(cp.read_text())['median']; cr.append(((m_['fd24'] + m_['forward']) / m_['forward'] - 1) / 24)
assert (round(min(cr), 2), round(max(cr), 2)) == (.30, .40)
# v14: "same path" replaced by the claim the data support, tested directly.
agree = {}
for sp, tg in (('a', 'T90'), ('b', 'T99')):
    for s_ in S:
        f_, a_ = R[('F', sp, s_, 1)], R[('Ap', sp, s_, 1)]; k = a_['attainment'][tg]['validation']
        assert f_['attainment'][tg]['validation'] == k
        d_ = [abs(x['F_val'] / y['F_val'] - 1) for x, y in zip(f_['validations'][:k + 1], a_['validations'][:k + 1])]
        agree[(sp, s_)] = (max(d_), max(range(len(d_)), key=d_.__getitem__), k)
assert max(agree[('a', s_)][0] for s_ in S) < .004
assert sorted(s_ for s_ in S if agree[('b', s_)][0] < .009) == ['lhs1', 'lhs3', 'theta0']
assert round(agree[('b', 'lhs2')][0], 2) == .29 and agree[('b', 'lhs2')][1] == agree[('b', 'lhs2')][2]  # at the crossing validation
assert round(agree[('b', 'lhs4')][0], 2) == .18 and agree[('b', 'lhs4')][1] == agree[('b', 'lhs4')][2] - 1  # one validation before it
assert (R[('F', 'b', 'lhs2', 1)]['train_events'], R[('Ap', 'b', 'lhs2', 1)]['train_events']) == (9, 10)
assert all(R[(m, 'b', s_, 1)]['stop_reason'] == 'T99' for m in ('F', 'Ap') for s_ in S)
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
# v13: per-component offsets of the mean gradient have fixed sign at all four points, at L1 and L0; SW3 nearly constant.
SCAL = ['MAG', 'MH', 'MW', 'SW1', 'SW2', 'SW3', 'SW4', 'Sr1']
for rec in (L1, fdc):
    errs = {n: [] for n in SCAL}
    for r in rec['records']:
        for p in r['points']:
            for c in p['components']:
                if c['name'] in errs:
                    errs[c['name']].append(c['fd_dmean_full'] - c['adjoint_dmean_full'])
    assert all(len(v) == 4 and (all(x > 0 for x in v) or all(x < 0 for x in v)) for v in errs.values())
    sw = errs['SW3']; rng = (round(-max(sw), 4), round(-min(sw), 4))
    assert rng == ((.0018, .0019) if rec is L1 else (.003, .0031)), rng
assert [p['x'] for p in L1['records'][0]['points'] if p['point'] == 'theta0'][0] == [p['x'] for p in L1['records'][1]['points'] if p['point'] == 'theta0'][0][:8]
# v13: step sweep at the failing L1 point (PR #1223, merge f94acad3), recomputed from the raw stencil values.
e2 = man['figures/src/data/exp2b/fd-step-sweep-L1.json']
assert e2['source_commit'] == 'f94acad3f312c7d417830ff82b66a78a1ea89fb2' and sha(V / e2['snapshot']) == e2['sha256']
SW = json.loads((V / e2['snapshot']).read_text()); rr = SW['record']; base = rr['base']
assert SW['level'] == 1 and SW['protocol_step'] == 1e-6 and SW['forward_steps'] == [1e-5, 1e-6, 1e-7] and SW['central_steps'] == [1e-5, 1e-6]
assert rr['space'] == 'a' and rr['point'] == 'interior' and rr['session']['training']['level'] == 1 and rr['session']['training']['newton_rel_tol'] == 1e-8
assert rr['base_repeat']['dF_abs'] == 0.0 and rr['base_repeat']['dmean_full_abs'] == 0.0 and rr['base_repeat']['F'] == base['F']
pint = [p for r_ in L1['records'] if r_['space'] == 'a' for p in r_['points'] if p['point'] == 'interior'][0]
assert pint['x'] == rr['x'] and pint['F'] == base['F']
fe, ce, perh = {}, {}, {}
for c in rr['components']:
    stn = {(q['step'], q['side']): q for q in c['stencil']}
    l1c = [x for x in pint['components'] if x['name'] == c['name']][0]
    for key, bk in (('dF', 'F'), ('dmean_full', 'mean_full')):
        adj = l1c['adjoint_' + key]
        assert all(sc[key]['adjoint'] == adj for sc in c['schemes'] if sc['usable'])
        for h in SW['forward_steps']:
            q = stn[(h, '+')]; fd = (q[bk] - base[bk]) / q['dx']; fe[(c['name'], key, h)] = fd - adj
            if h == 1e-6:
                assert fd == l1c['fd_' + key]  # bit-identical to the L1 check
        for h in SW['central_steps']:
            qp, qm = stn[(h, '+')], stn[(h, '-')]
            ce[(c['name'], key, h)] = (qp[bk] - qm[bk]) / (qp['dx'] - qm['dx']) - adj
        for sc in c['schemes']:
            if sc['usable']:
                ref = fe if sc['scheme'] == 'forward' else ce
                assert math.isclose(ref[(c['name'], key, sc['step'])], sc[key]['signed_err'], rel_tol=1e-9, abs_tol=1e-15)
        v = [fe[(c['name'], key, h)] / h for h in SW['forward_steps']]
        assert all(x > 0 for x in v) or all(x < 0 for x in v)
        perh[(c['name'], key)] = (v, max(map(abs, v)) / min(map(abs, v)) - 1, adj)
spread = {k: x[1] for k, x in perh.items()}
assert round(100 * max(x for k, x in spread.items() if k != ('MH', 'dF')), 1) == .9 and round(100 * spread[('MH', 'dF')], 1) == 3.0
adj3 = perh[('SW3', 'dmean_full')][2]
assert round(abs(fe[('SW3', 'dmean_full', 1e-6)] / adj3), 2) == .24 and f"{abs(ce[('SW3', 'dmean_full', 1e-6)] / adj3):.1e}" == '2.2e-04'
assert round(ce[('SW3', 'dmean_full', 1e-5)] / ce[('SW3', 'dmean_full', 1e-6)]) == 94
fl = [abs(ce[(n, k, 1e-6)]) for n in ('MH', 'SW2') for k in ('dF', 'dmean_full')]
assert (f'{min(fl):.0e}', f'{max(fl):.1e}') == ('9e-08', '3.4e-07')
ph = {n: abs(perh[(n, 'dmean_full')][0][1]) for n in ('SW3', 'MH', 'SW2')}
assert (f"{ph['SW3']:.1e}", f"{ph['MH']:.1e}", f"{ph['SW2']:.1e}") == ('1.9e+03', '2.8e+03', '9.7e+03') and f'{adj3:.1e}' == '-7.7e-03'
print('PASS: Exp 2b L1 step sweep (3 components x 2 outputs; forward/central errors recomputed from raw stencil values; truncation, bit-identical to the L1 check).')
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
for m, name in (('Ap', "A$'$"), ('F', 'FD-cold')):
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
import re as _re0
assert 'PENDING' not in tex  # v19: no pending marker remains
# v19 (v1 Zenodo release): each author's own affiliation and the paper's own DOI under the date.
assert ('\\author{Robb Walters\\\\\n  {\\normalsize 2AM Logic}\n  \\and Crutcher Dunnavant\\\\\n  {\\normalsize ZSpaceLabs.ai}}\n'
        '\\date{October 2026\\\\[0.5ex]\n  {\\normalsize DOI: \\href{https://doi.org/10.5281/zenodo.23580548}{10.5281/zenodo.23580548}}}\n') in tex
assert 'Independent Researcher' not in tex and tex.count('zenodo.23580548') == 4  # v20: title page and data statement
TITLE = 'Adjoint Gradients for Torque-Ripple Optimization of a Nonlinear Motor'  # v20: the operator's short title (#1231)
TITLE_SRC = '\\title{Adjoint Gradients for Torque-Ripple Optimization\\\\of a Nonlinear Motor}'  # one line break, no orphan word
assert TITLE_SRC.replace('\\\\', ' ') == '\\title{' + TITLE + '}'
assert tex.count('\\title{') == 1 and (TITLE_SRC + '\n') in tex and ('pdftitle={' + TITLE + '},') in tex
assert 'Per core, adjoint gradients reach' not in tex and 'in a nonlinear motor model' not in tex
# Package pointer (v13; v17 links it). Inside the repository the directory must carry the name the data
# statement links; a package downloaded on its own may sit in a directory of any other name.
PKG = 'motor-gradient-optimization.22'
assert V.name == PKG or not V.name.startswith('motor-gradient-optimization.'), f'the package must sit in a directory named {PKG} (the data statement links it)'
assert ('\\href{https://github.com/rjwalters/geode-fem/tree/main/papers/motor-gradient-optimization/' + PKG + '}{\\texttt{papers/}\\allowbreak\\texttt{motor-gradient-optimization/}\\allowbreak\\texttt{' + PKG + '}}') in tex
assert not any(f'motor-gradient-optimization.{k}}}' in tex for k in range(1, 22))
assert '\\href{https://github.com/rjwalters/geode-fem/commit/63b3e3d8bbc30504f845eebb5b78e8be6a2ee12f}{\\texttt{63b3e3d8}}' in tex  # v17: the rebuilt binary's commit is linked
assert '/crates/geode-core/examples/icem_final_refinement.rs}{\\texttt{63b3e3d8}},\nwhich differs from the version at' in tex and 'only by one unused import' in txt
for bad in ('gradient access', 'ruling', 'dilutes every', 'logarithm of', 'noise floor', 'times slower', 'essentially the same path', 'take the same path', 'upper bound on the noise'):
    assert bad not in txt, bad
import re as _re
ab = tex[tex.index('\\begin{abstract}') + 16:tex.index('\\end{abstract}')]
assert len(_re.sub(r'\$[^$]*\$', 'X', ab).replace('--', ' ').split()) <= 250
for phrase in [
    'and 2.47 and 4.11 times sooner than cold-started ones',
    'so the $n$ stencil solves run one after another and the time ratio is also a ratio of compute',
    'with a step of $10^{-6}$ in each coordinate\'s physical unit (metres for the lengths and offsets)',
    'as 8\\% of components did; no gradient was refused',
    'Against cold-started stencils it is 2.47 (2.45--2.52) and 4.11 (3.84--4.33), with repeat 2 giving 2.49 and 4.04',
    "Costs are in each session's unit, the median cold validation forward (66.66 and 69.09\\,s). One adjoint gradient costs A$'$ 0.057 and 0.061 units with eight and 18 parameters",
    'from every start its best validated objective is within 0.4\\% of A$\'$\'s', 'was not tested; it does not affect the time to target, since every crossing happens at the same validation count',
    "FD-cold's gradients agree with the adjoint within a max-norm relative error of $5.2\\times10^{-4}$ to $1.9\\times10^{-3}$ for $F$ and at most $7.2\\times10^{-4}$ for the full-current mean",
    'but one small component exceeds the check\'s per-component limit of $10^{-1}$',
    'The L1 check was run after the ablation, with the frozen step and the frozen limits; it was not part of the ablation\'s protocol',
    'For every one of the eight scalar components, the finite-difference-minus-adjoint error of the mean gradient has the same sign at all four points, at L1 and at L0',
    '$-1.8$ to $-1.9\\times10^{-3}$ at L1 and $-3.0$ to $-3.1\\times10^{-3}$ at L0',
    'was 1.71 and 2.70 against warm-started stencils and 2.47 and 4.11 against cold ones',
    'the counterpart of our eight-parameter SLSQP-against-COBYQA ratios of 3.10--3.25. They do not compare adjoint with finite-difference gradients, which our ablation does',
    "Up to the primary crossing the validated objectives of FD-cold and A$'$ agree within 0.4\\% in S8 and within 0.9\\% at three S18 starts; at S18 LHS~2 and LHS~4 the iterates drift apart before the $T_{99}$ crossing, by 29\\% at the crossing validation and 18\\% one validation before it, respectively, and at LHS~2 FD-cold uses one coarse evaluation fewer",
    'One finite-difference gradient costs FD-cold 1.90 and 4.25 units, about 0.24 units per cold stencil solve, 33 and 70 times the adjoint',
    # v22: the per-session medians moved to Appendix D's cost-estimate paragraph; Section 6 keeps the cost-table range and the ratios.
    "every coarse evaluation costs a coarse forward that both arms pay, 0.216--0.245 units (Table~\\ref{tab:costs}; Appendix~\\ref{sec:exp2-times} gives each session's medians), so an evaluation with a gradient costs FD-cold 7.4 and 14.7 times as much as A$'$",
    "The coarse forward that every coarse evaluation costs is taken per session: as a median over runs of coarse-simulation time per coarse forward, it is 0.240 and 0.236 units for FD-cold against 0.232 and 0.245 for its A$'$",
    'the gradient accounts for a factor of 1.71--2.47 and 2.68--4.10, and the optimizer for 1.22--1.76 and 1.91--2.92',
    'The lower gradient factor and the higher optimizer factor belong to FD-warm, the cheaper finite-difference baseline',
    'finite-difference SLSQP contributes the remaining 1.2 to 1.8 with 8 parameters and 1.9 to 2.9 with 18',
    'like every coarse forward in the preregistered coarse configuration it starts Newton-cold (all 40 traces record a cold start)',
    "FD-cold's SLSQP met its stopping test in two of ten runs and ran to the budget in the other eight, whereas A$'$ converged in four, stopped in the solver in four and reached the budget in two",
    'The last gradient of each S8 $\\theta_0$ run of FD-cold was cut off by the budget and is excluded from the cost medians',
    'it is drawn at B\'s 120-unit budget',
    'exceeds the check\'s per-component limit of $10^{-1}$ through forward-difference truncation, which a central difference with the same step reduces from 0.24 to $2.2\\times10^{-4}$ relative',
    'keeps its sign and its size to within 0.9\\% (3.0\\% for the MH component of the objective gradient), as truncation error does and noise would not',
    'its central error falls 94-fold from $10^{-5}$ to $10^{-6}$, close to the 100-fold that $h^2$ predicts',
    'of the MH and SW2 components are $9\\times10^{-8}$ to $3.4\\times10^{-7}$ in absolute terms',
    'the magnitude of SW3\'s forward error per unit step, $1.9\\times10^{3}$\\,N\\,m/m$^2$, is smaller than MH\'s ($2.8\\times10^{3}$\\,N\\,m/m$^2$) and SW2\'s ($9.7\\times10^{3}$\\,N\\,m/m$^2$), so SW3 fails because its mean-gradient slope at this design is small ($-7.7\\times10^{-3}$\\,N\\,m/m)',
    'the frozen step lies where truncation, not computational noise, dominates the forward-difference error',
    'a repeated base solve agrees bit for bit, and its forward values at $10^{-6}$ equal those of the L1 check',
    "FD-cold's forward differences carry a truncation error and FD-warm's warm stencils a Newton-tolerance offset",
    'because they cross after the same validated designs',
    "The two labelled rows above the line give each start's FD-warm/A$'$ and FD-cold/A$'$ ratio of charged time, each formed within its own session",
    "Validation is a median 61--65\\% of A$'$'s time to a target and 16--27\\% of FD-cold's",
        'the SW3 component of the mean gradient is $-9.62\\times10^{-3}$ by finite differences against $-7.74\\times10^{-3}$ by the adjoint, a relative error of 0.24, on a component $8.7\\times10^{-5}$ times the largest',
    'against FD-cold it matched or beat SLSQP from two of five S8 starts (B/FD-cold 0.70 and 0.95)',
    'keep every target crossing within its target at L3, with at least 15\\% margin',
    'max-norm relative error of $5.1\\times10^{-4}$ to $1.9\\times10^{-3}$ for $F$ and $1.0\\times10^{-4}$ to $6.7\\times10^{-4}$',
    'up to $5.7\\times10^{-2}$ for SW3 at the S18 interior point',
    'calibrated unit of $66.66\\,\\mathrm{s}$, against $70.94\\,\\mathrm{s}$', 'agreed within $2.4\\times10^{-13}$', 'load peaked at 19.7',
    'inventory of the 2,118 session files',
    'was posted at 22:34~UTC on 2026-10-20 and frozen by a comment at 23:00~UTC, before the first FD-cold run began at 02:07~UTC on 2026-10-21',
    'All 90 checks of the 14 designs completed', 'within $6.3\\times10^{-4}$ relative',
    'L3 changes $F$ by $-2.9\\%$ to $+2.6\\%$ ($-2.9\\%$ to $+1.1\\%$ at the crossings)', 'by less than $0.03\\%$',
    'gives $1.1\\%$ to $4.4\\%$ more ripple on the S8 designs', 'with at least 15\\% margin', 'the closest by 0.044\\%',
    'chose the same 14 designs, bit for bit']:
    assert phrase in txt, phrase
# v13: deposit-ready raw-trace archive named in the data statement; v19: its reserved DOI replaces the marker; v20: "deposited at".
DR = json.loads((V / 'figures/src/data/deposit/deposit-record.json').read_text())
inv_main = (D2 / 'provenance/exp2-main-inventory-0650f4ee.sha256.txt').read_text().splitlines()
assert DR['archive'] == 'motor-paper-raw-traces.tar.zst' and DR['uploaded'] is False and len(DR['sha256']) == 64
inv_warm = (V / 'figures/src/data/exp2c/provenance/exp2-fd-warm-inventory-df509322.sha256.txt').read_text().splitlines()
assert {k: DR['files'][k] for k in ('exp2-main', 'exp2-fd-ablation', 'exp2-fd-warm')} == {'exp2-main': len(inv_main), 'exp2-fd-ablation': len(inventory), 'exp2-fd-warm': len(inv_warm)} == {'exp2-main': 5453, 'exp2-fd-ablation': 2118, 'exp2-fd-warm': 1180}
assert f"{DR['bytes']:,}" in tex and DR['sha256'][:32] in tex and DR['sha256'][32:] in tex
assert ('deposited at\nDOI \\href{https://doi.org/10.5281/zenodo.23580550}{10.5281/zenodo.23580550}. Every file') in tex and tex.count('zenodo.23580550') == 2
assert 'for deposit at' not in txt
# v20: the paper's own DOI in the data statement, where it first gives the GEODE repository URL.
DOI_V20 = ("only by one unused import.\nThis paper's PDF and \\LaTeX{} source are archived at\n"
           "\\href{https://doi.org/10.5281/zenodo.23580548}{doi:10.5281/zenodo.23580548}.\nThe paper package is the directory\n")
assert tex.count(DOI_V20) == 1
# v21 (#1231, coauthor review): the optimizers' "training model" and the scorer's "validation model" are now the
# coarse simulation and the validation simulation (adjectives likewise; the appendix's cost model is named as one), and a
# future-work paragraph closes the Discussion. Every v21 edit, as (file, v20 text, v21 text, count):
V21_EDITS = [
    ('main.tex', 'The optimizers query a cheap\ntraining model; each new best design is rescored on a finer validation\nmodel that no optimizer sees', 'The optimizers query a coarse\nsimulation; each new best design is rescored on a finer validation\nsimulation that no optimizer sees', 1),
    ('main.tex', 'at the same training resolution in all 45', 'at the same coarse resolution in all 45', 1),
    ('main.tex', 'so validation and\ntraining forwards bound the end-to-end gain', 'so validation and\ncoarse forwards bound the end-to-end gain', 1),
    ('main.tex', 'on one validation model finer than any training model, and charges', 'on one validation simulation, finer than any coarse simulation an optimizer\nqueries, and charges', 1),
    ('main.tex', 'bounded by the time all arms spend on validation and training forwards', 'bounded by the time all arms spend on validation and coarse forwards', 1),
    ('main.tex', 'the\noptimization study trains on order-45 coupling torque and validates on\norder 93.', "the\noptimization study's coarse simulation uses order-45 coupling torque and\nits validation simulation order 93.", 1),
    ('main.tex', '\\subsection{Interface order sets the validation model}', '\\subsection{Interface order sets the validation simulation}', 1),
    ('main.tex', 'validation and training forwards bounds the factor', 'validation and coarse forwards bounds the factor', 1),
    ('main.tex', 'D is a coarse finite-element model, not such a baseline', 'D searches a coarse finite-element simulation, not such a baseline', 1),
    ('main.tex', 'On a common validation model, SLSQP', 'On a common validation simulation, SLSQP', 1),
    ('main.tex', 'so validation and training forwards now bound the', 'so validation and coarse forwards now bound the', 1),
    ('main.tex', '{training-gradient finite-difference records}', '{coarse-gradient finite-difference records}', 1),
    ('main.tex', 'copies of the\ntraining-gradient checks', 'copies of the\ncoarse-gradient checks', 1),
    ('main.tex', 'The optimization study trains on order-45 coupling torque and validates on\norder 93.', "The optimization study's coarse simulation uses order-45 coupling torque and\nits validation simulation order 93.", 1),
    ('main.tex', 'minimize $F_{\\mathrm{train}}/F_{\\mathrm{train}}(\\theta_0)$ and impose the\nmean floor as $\\mu_{\\mathrm{train}}/\\mu_{\\mathrm{train}}(\\theta_0)-1.0025\\geq0$.', 'minimize $F_r/F_r(\\theta_0)$ on their coarse simulation $r$ and impose the\nmean floor as $\\mu_{r,1}/\\mu_{r,1}(\\theta_0)-1.0025\\geq0$.', 1),
    ('main.tex', 'the three training-current standard deviations, each over $\\sqrt{F_{\\mathrm{train}}(\\theta_0)}$', 'the three objective-current standard deviations, each over $\\sqrt{F_r(\\theta_0)}$', 1),
    ('main.tex', '\\paragraph{Training gradients.}', '\\paragraph{Coarse gradients.}', 1),
    ('main.tex', 'the L1\ntraining objective $F$', 'the L1\ncoarse objective $F$', 1),
    ('main.tex', "gradient replies at the study's training tolerance of $10^{-8}$ agree at", "gradient replies at the coarse simulation's Newton tolerance of $10^{-8}$ agree at", 1),
    ('main.tex', '$2n+1=37$ training evaluations', '$2n+1=37$ coarse evaluations', 1),
    ('main.tex', 'outside the three training\ncurrents.', 'outside the three objective\ncurrents.', 1),
    ('main.tex', 'at the training currents', 'at the objective currents', 3),
    ('main.tex', 'times the training-current ratio', 'times the objective-current ratio', 1),
    ('main.tex', 'specific to the three\ntraining currents.', 'specific to the three\nobjective currents.', 1),
    ('main.tex', 'On the L1 training model with the frozen step', 'On the L1 coarse simulation with the frozen step', 1),
    ('main.tex', 'for a cold training evaluation', 'for a cold coarse evaluation', 1),
    ('main.tex', "for the session's base training forwards", "for the session's base coarse forwards", 1),
    ('main.tex', 'the measured ratios. The model counts only training forwards, adjoints and', 'the measured ratios. Their cost model counts only coarse forwards, adjoints and', 1),
    ('main.tex', "against the model's 0.27 to 0.31", "against the cost model's 0.27 to 0.31", 1),
    ('main.tex', 'differ from their model by', 'differ from their cost model by', 1),
    ('main.tex', 'is where the model errs most.', 'is where the cost model errs most.', 1),
    ('main.tex', '\\paragraph{Training and validation objectives.}\nThe training model also reads lower than validation.', '\\paragraph{Coarse and validation objectives.}\nThe coarse simulation also reads lower than validation.', 1),
    ('main.tex', 'order-45 training objective is', 'order-45 coarse objective is', 1),
    ('main.tex', 'the ratio of training to validation objective', 'the ratio of coarse to validation objective', 1),
    ('main.tex', 'come with a\ntraining objective that underestimates', 'come with a\ncoarse objective that underestimates', 1),
    ('main.tex', 'On the L0 model, FD-cold', 'On the L0 coarse simulation, FD-cold', 1),
    ('main.tex', 'FD-cold uses one training evaluation fewer', 'FD-cold uses one coarse evaluation fewer', 1),
    ('main.tex', 'this coarse-model\nexplanation', 'this coarse-reference\nexplanation', 1),
    ('main.tex', 'the corresponding training-to-validation', 'the corresponding coarse-to-validation', 1),
    ('main.tex', 'with measurements on a physical motor.\n\n\\section{Conclusion}', 'with measurements on a physical motor.\n\nCheap, verified gradients also make methods affordable that this study did\nnot test. Second-order information is one: Hessian-vector products through a\nsecond-order adjoint, or quasi-Newton methods driven by exact gradients beyond\nthe BFGS update inside SLSQP. Gradient-enhanced population and multistart\nsearches are another, such as particle swarms or CMA-style searches seeded or\nsteered by adjoint gradients, with runs in parallel across starts; every run\nhere used one single-threaded core. Each still pays for validation and coarse\nforwards, which already bound the end-to-end gain (Fig.~\\ref{fig:decomp}), and\ncan gain beyond that only by needing fewer of them or, in wall time but not in\ncompute, by running them in parallel. A natural next problem for the same\npipeline is the rotor flux-barrier design of synchronous reluctance machines,\na classic shape-optimization target because their torque ripple is high;\ngradient-based free-form shape optimization has already been applied to such a\nmachine \\citep{gangl2022multi}.\n\n\\section{Conclusion}', 1),
    ('main.tex', 'motor-gradient-optimization.20}', 'motor-gradient-optimization.21}', 2),
    ('optimization.tex', "\\paragraph{Training and validation models.}\nThe optimizers never query the model that scores them. Each queries a\ncheaper \\emph{training model}, the L1 (or, for two methods, L0) mesh with\ninterface order 45, and its search is driven only by training values. Every\ndesign that becomes the method's new best, its incumbent, is rescored on one\nfiner \\emph{validation model}, the L2 mesh with interface order 93, and only\nvalidated scores count toward a target. Training here means querying the\ncheap model, not fitting: apart from the surrogate of method C, nothing is\nfitted to data. Validation is charged", "\\paragraph{Coarse and validation simulations.}\nThe optimizers never query the simulation that scores them. Each queries a\n\\emph{coarse simulation}, the L1 mesh (the coarser L0 for methods D and E)\nwith interface order 45, and its search is driven only by coarse values.\nEvery design that becomes the method's new best, its incumbent, is rescored\non one \\emph{validation simulation}, the finer L2 mesh with interface order\n93, which no optimizer sees; only validated scores count toward a target.\nUnqualified, \\emph{model} means a formulation of the simulation, such as the\nfield or interface model; no simulation is trained or fitted to torque data.\nOf the models the optimizers use, only their own search models are fitted,\nto coarse values: C's radial-basis surrogate and COBYQA's interpolation model.\nValidation is charged", 1),
    ('optimization.tex', 'of the L1 training outputs', 'of the L1 coarse outputs', 1),
    ('optimization.tex', 'at fixed training resolution', 'at fixed coarse resolution', 1),
    ('optimization.tex', 'Table~\\ref{tab:methods}. Training uses order-45 coupling torque', 'Table~\\ref{tab:methods}. The coarse simulation uses order-45 coupling torque', 1),
    ('optimization.tex', '\\label{tab:methods}\n\\begin{tabular}{llll}\\toprule\nMethod & Optimizer or search model & Gradient & Training mesh\\\\', '\\label{tab:methods}\\setlength{\\tabcolsep}{5pt}\n\\begin{tabular}{llll}\\toprule\nMethod & Optimizer or search model & Gradient & Optimization mesh\\\\', 1),
    ('optimization.tex', 'Each training model $r\\in\\{\\mathrm{L0},\\mathrm{L1}\\}$ minimizes\n$F_r=\\sum_c\\sigma_{r,c}^2$ over the three training currents,', 'Each coarse simulation $r\\in\\{\\mathrm{L0},\\mathrm{L1}\\}$ minimizes\n$F_r=\\sum_c\\sigma_{r,c}^2$ over the three objective currents $c$,', 1),
    ('optimization.tex', 'L2, 20-angle validation model.', 'L2, 20-angle validation simulation.', 1),
    ('optimization.tex', 'a training\nforward at a perturbed point', 'a coarse\nforward at a perturbed point', 1),
    ('optimization.tex', 'repair, training forwards, adjoints', 'repair, coarse forwards, adjoints', 1),
    ('optimization.tex', 'Operation & Model & Cost (units)', 'Operation & Simulation & Cost (units)', 1),
    ('optimization.tex', 'Training forward & L1, order 45 &', 'Coarse forward & L1, order 45 &', 1),
    ('optimization.tex', 'COBYQA at the same\ntraining resolution.', 'COBYQA at the same\ncoarse resolution.', 1),
    ('optimization.tex', 'Filled markers: L1 training; open markers: L0 training;', 'Filled markers: L1 coarse simulation; open markers: L0;', 1),
    ('optimization.tex', 'costs $n$ extra training forwards', 'costs $n$ extra coarse forwards', 1),
    ('optimization.tex', "Each stencil solve is an ordinary training\nforward on the base point's mesh generation, and like every training\nforward in the preregistered training configuration", "Each stencil solve is an ordinary coarse\nforward on the base point's mesh generation, and like every coarse\nforward in the preregistered coarse configuration", 1),
    ('optimization.tex', 'on the L1 training model at 120 units', 'on the L1 coarse simulation at 120 units', 1),
    ('optimization.tex', 'and the rest (training forwards, mesh motion', 'and the rest (coarse forwards, mesh motion', 1),
    ('optimization.tex', 'bounded by the validation and training forwards', 'bounded by the validation and coarse forwards', 1),
    ('optimization.tex', 'charged about half a base training forward', 'charged about half a base coarse forward', 1),
    ('optimization.tex', 'First, every training\nevaluation costs a training forward that both arms pay. As a median over runs\nof training time per training forward,', 'First, every coarse\nevaluation costs a coarse forward that both arms pay. As a median over runs\nof coarse-simulation time per coarse forward,', 1),
    ('optimization.tex', "FD-warm's training time also contains", "FD-warm's coarse-simulation time also contains", 1),
    ('optimization.tex', 'median charge of one base training forward', 'median charge of one base coarse forward', 1),
    ('optimization.tex', 'and training forwards do not grow with $n$', 'and coarse forwards do not grow with $n$', 1),
]
assert not _re0.search(r'(?i)\btrain', tex), 'training vocabulary remains'  # v22: no "train" token at all (v21 still had "no simulation is trained")
for bad in ('training model', 'validation model', 'cheap model', 'Training mesh', 'coarse-model', 'Optimization mesh', 'Unqualified', 'physical competitor'):
    assert bad not in txt, bad
assert txt.count('coarse simulation') >= 10 and txt.count('validation simulation') >= 5 and 'cost model' in txt
# v22: the three "not fitted" statements of v21 (protocol paragraph, methods paragraph, Table tab:methods caption) are one positive
# sentence. It is scoped so that it stays literally true: the cost model, the slotless fit and the derivative-check fits are not
# "what the optimizers use", and the tabulated B-H law is a material input, not torque data.
MODEL_V22 = ('Both are physics simulations, the nonlinear finite-element model of Section~\\ref{sec:model} at coarser and finer mesh levels and interface orders, '
             'neither fitted to torque data nor corrected against validation; of what the optimizers use, only their own search models, '
             "C's radial-basis surrogate and COBYQA's interpolation model, are fitted, to coarse values.")
assert MODEL_V22 in txt and txt.count('fitted to torque data') == 1 and 'is trained' not in txt
for gone in ('neither is fitted or corrected against validation', 'neither fitted magnetic circuits nor reduced-basis models', 'Of the models the optimizers use'):
    assert gone not in txt, gone
assert tex.count('\\citep{gangl2022multi}') == 1 and tex.count('gangl2022multi') == 2  # related work and future work; no new reference
# v22 (reading-flow revision, #1231): Section "Design-gradient verification" precedes the optimization study, which precedes the
# ablations and the Discussion; the ablation protocol is three paragraphs; the "model" statement is the one sentence above.
assert tex.index('\\section{Motor model}') < tex.index('\\section{Design-gradient verification}') < tex.index('\\input{optimization}') < tex.index('\\section{What transfers, and limits}') < tex.index('\\section{Conclusion}')
for phrase in ['\\paragraph{FD-cold.}', '\\paragraph{FD-warm.}', '\\paragraph{Controls and estimand.}', 'We call the methods of an ablation its \\emph{arms}.',
               'Both finite-difference arms and both A$\'$ controls reach', 'the finite-difference stencil (one perturbed coarse solve per parameter) solved serially',
               'our finite-difference baselines instead use one step fixed in advance', 'whereas our warm-started baseline keeps the frozen tolerance',  # Judge: "arm" is not used before Section 6 defines it
               'the eight scalar design parameters that form the smaller design space, S8, and the 55 control points whose grouped offsets the larger space, S18, adds',
               'The 18-parameter space S18 (Section~\\ref{sec:optimization}) adds ten offsets', 'on the two coarsest meshes, L0 and L1 (Section~\\ref{sec:meshes})',
               'The forward comparisons of Section~\\ref{sec:coupling-results} use Arkkio for both couplings', 'the derivative study of Section~\\ref{sec:gradients} pairs',
               'the optimization study of Section~\\ref{sec:optimization} runs its optimizers on order-45 coupling torque (its \\emph{coarse simulation}) and scores them at order 93 (its \\emph{validation simulation})',
               'are 0.15\\% and 0.16\\% below the saved reference at half and full current',
               'this sets the validation simulation: its optimizers search at order 45, and a design is credited only after it is rescored at order 93 on a finer mesh',
               'is described in the \\emph{Final refinement} paragraph of Section~\\ref{sec:exp2-results}',
               'This is the one deviation from the preregistration noted at the start of Section~\\ref{sec:optimization}: the registered pipeline began by capturing the closed study on its original host, which had been restarted, so the refinement used a hash-verified copy',
               'every paired ratio below is written in this order (B/A, D/E, E/A, D/B), so a value above one favors the second method',
               'Against D, the derivative-free competitor on the coarser mesh', "FD-warm/A$'$ uses the plotted A$'$, and FD-cold/A$'$ uses the FD-cold session's A$'$, which is not plotted and agrees with the plotted one within 4\\%",
               '60-solve, single-core forward charged in the optimization study (Section~\\ref{sec:optimization}); the two studies\' cost ratios are not interchangeable',
               'Method & Optimizer or search model & Gradient & Coarse simulation', '$\\bar T_{r,1}\\geq1.0025\\,\\bar T_{r,1}(\\theta_0)$', '$\\bar T_{r,1}/\\bar T_{r,1}(\\theta_0)-1.0025\\geq0$']:
    assert phrase in txt, phrase
assert '\\mu_{r,1}' not in tex and tex.count('$\\mu_r=1.05$') == 1  # the coarse mean is \bar T_{r,1}; \mu_r stays the relative permeability
_main, _opt = (V / 'main.tex').read_text(), (V / 'optimization.tex').read_text()
assert not _re0.search(r'\barms?\b', _main[:_main.index('\\input{optimization}')] + _opt[:_opt.index('\\section{Post-hoc ablations')]), '"arm" used before Section 6 defines it'
assert tex.count("Against D, the") == 1 and txt.count('1.15--1.16') == 1 and txt.count('1.92--3.19') == 1  # the A-D validation story is told once, in "Validation charges"
# Inside the repository, v22 is pinned against the v21 sibling: the same title block, authors, date and DOIs; the same bibliography,
# class, tables, figures and figure sources, byte for byte; and the same numbers. The multiset of numeric tokens of main.tex +
# optimization.tex differs from v21's only by the listed duplicates (a bridging sentence, the cost-table range, the package pointer,
# cross-references) and deletions (a Discussion recap and an Appendix D paragraph now stated once in the main text).
V21 = V.parent / 'motor-gradient-optimization.21'
V22_NUM_ADDED = {'0.216': 1, '0.245': 1, '2': 3, '2.70': 1, '22': 2, '4': 1, '4.11': 1}
V22_NUM_REMOVED = {'1.15': 1, '1.16': 1, '1.22': 1, '1.71': 1, '1.76': 1, '1.91': 1, '1.92': 1, '18': 1, '2.47': 1, '2.92': 1, '21': 2, '3.19': 1}
if V.name == PKG and (V21 / 'main.tex').exists():
    import collections as _co
    assert sha(V21 / 'main.tex') == 'fb3b266dad3a4d00374a871acab2c7557a978d09c3ae1abca30f7ce9b8c38b4c' and sha(V21 / 'optimization.tex') == 'b1c6970571a917cc2b4bfbcd66a77cc2ef98eb2300ccfdec0790c7d48c120716', 'v21 sources changed'
    tex21 = (V21 / 'main.tex').read_text() + (V21 / 'optimization.tex').read_text()
    n21, n22 = _co.Counter(_re0.findall(r'\d+(?:\.\d+)?', tex21)), _co.Counter(_re0.findall(r'\d+(?:\.\d+)?', tex))
    assert set(n22) <= set(n21), sorted(set(n22) - set(n21))  # every number of v22 is a number of v21
    assert dict(n22 - n21) == V22_NUM_ADDED and dict(n21 - n22) == V22_NUM_REMOVED, (dict(n22 - n21), dict(n21 - n22))
    head = lambda t: t[:t.index('\\begin{document}')]
    assert head(tex) == head(tex21)  # preamble: title, authors, affiliations, date, DOI, pdftitle, pdfauthor
    assert _re0.findall(r'zenodo\.\d+', tex) == _re0.findall(r'zenodo\.\d+', tex21) and _re0.findall(r'\\cite[pt]?\{[^}]+\}', tex) and set(_re0.findall(r'\\cite[pt]?\{([^}]+)\}', tex)) == set(_re0.findall(r'\\cite[pt]?\{([^}]+)\}', tex21))
    for rel in ['refs.bib', 'main.bbl', 'anvil-paper.cls', 'evidence-manifest.json', 'verified-numbers.json'] + sorted(str(p.relative_to(V21)) for p in list((V21 / 'tables').glob('*.tex')) + list((V21 / 'figures').glob('*.p*')) + list((V21 / 'figures/src').glob('*.py'))):
        assert sha(V / rel) == sha(V21 / rel), rel
    print(f'PASS: v22 against the v21 sibling: numbers (multiset differs only by the {sum(V22_NUM_ADDED.values())} listed duplicates and {sum(V22_NUM_REMOVED.values())} deletions, no new value), preamble, DOIs, citations, bibliography, class, tables, figures and figure sources.')
else:
    print('NOTE: no motor-gradient-optimization.21 beside this package; the v22-against-v21 comparison is not checked.')
# Inside the repository: undoing the v21 edits on the v21 sibling must give back v20's main.tex and optimization.tex exactly, and
# undoing the v20 edits must then give back v19's main.tex. A package downloaded on its own has no siblings and skips this.
V20 = V.parent / 'motor-gradient-optimization.20'; V19 = V.parent / 'motor-gradient-optimization.19'
if V.name == PKG and all((d / 'main.tex').exists() for d in (V21, V20, V19)):
    src = {f: (V21 / f).read_text() for f in ('main.tex', 'optimization.tex')}
    for f, a, b, n in reversed(V21_EDITS):
        assert src[f].count(b) == n, (f, b[:60])
        src[f] = src[f].replace(b, a)
    assert src['main.tex'] == (V20 / 'main.tex').read_text() and src['optimization.tex'] == (V20 / 'optimization.tex').read_text(), 'v21 differs from v20 outside the v21 edits'
    assert hashlib.sha256(src['main.tex'].encode()).hexdigest() == '71aa4ae329fa40960119e22963cae13a2447e861531b1da8cc16bd3e90ddaa58', 'v20 main.tex changed'
    assert sha(V20 / 'optimization.tex') == 'bfb904416b705ef634cd8966ea5fd50a68bf9dc5270713f92ec6202977b88124'
    print(f'PASS: v21 sources (the sibling package) equal v20 apart from the {len(V21_EDITS)} listed v21 edits (terminology, future work, package pointer).')
# v20: v20's main.tex equals v19's apart from the title, the pdftitle, the archive's "deposited at", the DOI sentence
# and the package pointer; optimization.tex is v19's bytes. It is checked on the v20 text recovered above.
if V.name == PKG and all((d / 'main.tex').exists() for d in (V21, V20, V19)):
    v19 = (V19 / 'main.tex').read_text(); new = src['main.tex']
    assert hashlib.sha256(v19.encode()).hexdigest() == '76208679f1523b973145f157305eedcdc6df38f86cd16b9c21c68781d7315064', 'v19 main.tex changed'
    assert src['optimization.tex'] == (V19 / 'optimization.tex').read_text() and sha(V19 / 'optimization.tex') == 'bfb904416b705ef634cd8966ea5fd50a68bf9dc5270713f92ec6202977b88124'
    T19 = ('Per core, adjoint gradients reach validated torque-ripple targets\\\\\n'
           '1.7 and 2.7 times sooner than warm-started finite differences\\\\in a nonlinear motor model')
    P19 = 'Per core, adjoint gradients reach validated torque-ripple targets 1.7 and 2.7 times sooner than warm-started finite differences in a nonlinear motor model'
    back = [(TITLE_SRC, '\\title{' + T19 + '}'), ('pdftitle={' + TITLE + '}', 'pdftitle={' + P19 + '}'),
            (DOI_V20, 'only by one unused import.\nThe paper package is the directory\n'),
            ('), deposited at\nDOI', '), for deposit at\nDOI'), ('motor-gradient-optimization.20}', 'motor-gradient-optimization.19}')]
    for a, b in back:
        assert new.count(a) == (2 if a.startswith('motor-') else 1) and v19.count(b) == new.count(a), a
        new = new.replace(a, b)
    assert new == v19, 'main.tex differs from v19 outside the v20 changes'
    print('PASS: v20 main.tex (recovered from v21) equals v19 apart from the title, pdftitle, "deposited at", the paper-DOI sentence and the package pointer; optimization.tex unchanged.')
else:
    print('NOTE: no motor-gradient-optimization.21, .20 and .19 beside this package; the v21, v20 and v19 comparisons are not checked.')
if os.environ.get('MOTOR_RAW_ARCHIVE'):
    h = hashlib.sha256()
    with open(os.environ['MOTOR_RAW_ARCHIVE'], 'rb') as fh:
        for chunk in iter(lambda: fh.read(1 << 24), b''):
            h.update(chunk)
    assert h.hexdigest() == DR['sha256'] and Path(os.environ['MOTOR_RAW_ARCHIVE']).stat().st_size == DR['bytes']
    print('PASS: raw-trace archive re-hashed against the deposit record.')
print('PASS: Exp 2b tables (per-start times and ratios, validation charges, final checks) and every manuscript number; title (v20); v21 terminology and future work; v22 section order, one-sentence model statement and moved text; package pointer; author affiliations and the paper and archive DOIs, no pending marker.')
