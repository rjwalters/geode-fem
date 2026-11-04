"""Independent auditor recomputation of v7 Section 7 numbers (does not import verify_exp2)."""
import json, math, sys, hashlib, statistics
from pathlib import Path
V = Path(sys.argv[1]); D = V/'figures/src/data/exp2'
X = json.loads((D/'runs.json').read_text()); RF = json.loads((D/'refinement.json').read_text())
A = X['anchor']; F0 = A['F0']; floor = A['mean_floor']
print('F0', F0, 'mu0', A['mean0_full_current'], 'floor', floor, floor/A['mean0_full_current'])
T = {k: A['target_fractions'][k]*F0 for k in A['target_fractions']}
runs = X['runs']
# inventory binding
inv = {}
for l in (D/'provenance/exp2-main-inventory-0650f4ee.sha256.txt').read_text().splitlines():
    h, p = l.split('  ', 1); inv[p] = h
print('inventory sha', hashlib.sha256((D/'provenance/exp2-main-inventory-0650f4ee.sha256.txt').read_bytes()).hexdigest(), 'entries', len(inv))
print('bound', sum(inv.get('exp2-main/main/'+r['file']) == r['sha256'] for r in runs), '/', len(runs))
# my own credit rule
flags = {'mean_ok_mismatch': 0, 'credited_field_false_but_ok': 0, 'over_budget_hits': 0}
def mycredit(v):
    if v['status'] != 'ok' or v['F_val'] is None: return False
    mok = v['mean_full'] is not None and v['mean_full'] >= floor
    if mok != bool(v['mean_ok']): flags['mean_ok_mismatch'] += 1
    return bool(v['admissible']) and mok and v['wall_fe'] <= 120.0
def mytime(r, tg):
    for v in r['validations']:
        if mycredit(v) and v['F_val'] + max(0.1*v['F_val'], 2.5e-7) <= T[tg]:
            return v['wall_fe']
    return None
for r in runs:
    for v in r['validations']:
        if v['status']=='ok' and v['admissible'] and v['mean_ok'] and not v['credited']: flags['credited_field_false_but_ok'] += 1
        if v['wall_fe'] > 120: flags['over_budget_hits'] += 1
print('flags', flags)
R = {(r['method'], r['space'], r['start'], r['repeat']): r for r in runs}
S = ['theta0','lhs1','lhs2','lhs3','lhs4']
tt = {(m,sp,s,tg,rep): mytime(R[(m,sp,s,rep)], tg) for m in 'ABCDE' for sp in 'ab' for s in S for tg in T for rep in (1,2)}
# harness agreement
dis = [(k) for k,v in tt.items() if (v is None) != (not R[k[:3]+(k[4],)]['attainment'][k[3]]['attained']) or (v is not None and v != R[k[:3]+(k[4],)]['attainment'][k[3]]['wall_fe'])]
print('disagreements with harness attainment', len(dis), dis[:5])
# table 3
tab = (V/'tables/exp2-time-to-target.tex').read_text()
rows=[]
for sp,tg in [('a','T50'),('a','T90'),('b','T50'),('b','T90'),('b','T99')]:
    for m in 'ABEDC':
        rows.append(' & '.join(['%.1f'%tt[(m,sp,s,tg,1)] if tt[(m,sp,s,tg,1)] else '---' for s in S]))
import re
trows = [ ' & '.join(x.strip() for x in l.split('&')[2:]).replace('\\\\','').strip() for l in tab.splitlines() if re.match(r'^(A|B|\s)', l) and '&' in l and 'Space' not in l]
print('table3 rows match', sum(a==b for a,b in zip(rows,trows)), '/', len(rows), len(trows))
for a,b in zip(rows,trows):
    if a!=b: print('  T3 MISMATCH', a, '|', b)
print('space A T99 any', any(tt[(m,'a',s,'T99',rep)] for m in 'ABCDE' for s in S for rep in (1,2)))
cnt1 = sum(1 for k,v in tt.items() if v and k[4]==1); cnt2 = sum(1 for k,v in tt.items() if v and k[4]==2)
both = [(k, tt[k], tt[k[:4]+(2,)]) for k in tt if k[4]==1 and tt[k] and tt[k[:4]+(2,)]]
print('crossings r1', cnt1, 'r2', cnt2, 'both', len(both))
only1 = [k for k in tt if k[4]==1 and tt[k] and not tt[k[:4]+(2,)]]; only2=[k for k in tt if k[4]==2 and tt[k] and not tt[k[:4]+(1,)]]
print('only r1', only1, 'only r2', only2)
pct = sorted(abs(b/a-1) for k,a,b in both); lg = sorted(abs(math.log(b/a)) for k,a,b in both)
sym = sorted(abs(b-a)/min(a,b) for k,a,b in both); sym2 = sorted(abs(b-a)/max(a,b) for k,a,b in both)
print('repeat diff: median |t2/t1-1| %.4f max %.4f ; |ln| median %.4f max %.4f; rel-to-min max %.4f rel-to-max max %.4f'%(statistics.median(pct), max(pct), statistics.median(lg), max(lg), max(sym), max(sym2)))
kmax = max(both, key=lambda z: abs(math.log(z[2]/z[1]))); print('  max pair', kmax)
# late crossings (checkpoint)
late=[]
for r in runs:
    for tg in T:
        t = mytime(r, tg)
        if t is not None and t > 60: late.append((r['file'], tg, round(t,1)))
print('late (>60 by my time):', len(late), late)
late_r1=[x for x in late if x[0].endswith('_r1.json')]; print('late repeat1', len(late_r1))
latecp=[(r['file'],tg) for r in runs for tg in T if r['attainment'][tg]['attained'] and not r['checkpoint_60'][tg].get('attained')]
print('late per checkpoint_60 block', len(latecp), latecp)
# ratios
def rat(n,d,sp,tg): return [ (tt[(n,sp,s,tg,1)]/tt[(d,sp,s,tg,1)] if tt[(n,sp,s,tg,1)] and tt[(d,sp,s,tg,1)] else None) for s in S]
gm = lambda xs: math.exp(sum(math.log(x) for x in xs)/len(xs))
CELLS=[('a','T50'),('a','T90'),('b','T50'),('b','T90'),('b','T99')]
rt = (V/'tables/exp2-ratios.tex').read_text()
mine=[]
G={}
for sp,tg in CELLS:
    for n,d in [('B','A'),('D','E'),('E','A'),('D','B'),('D','A')]:
        rr=rat(n,d,sp,tg); fin=[x for x in rr if x]; G[(n,d,sp,tg)]=gm(fin)
        mine.append('%d & %.2f & %s'%(len(fin), gm(fin), ', '.join('%.2f'%x if x else '---' for x in rr)))
trs=[ ' & '.join(x.strip() for x in l.split('&')[2:]).replace('\\\\','').strip() for l in rt.splitlines() if '&' in l and 'Space' not in l]
print('table4 rows match', sum(a==b for a,b in zip(mine,trs)), '/', len(mine))
for a,b in zip(mine,trs):
    if a!=b: print('  T4 MISMATCH', a,'|',b)
def rng(n,d,sps): v=[G[(n,d,sp,tg)] for sp,tg in CELLS if sp in sps]; return min(v),max(v)
for n,d in [('B','A'),('D','E'),('E','A'),('D','B'),('D','A')]:
    print(n+'/'+d, 'A: %.3f-%.3f'%rng(n,d,'a'), 'B: %.3f-%.3f'%rng(n,d,'b'), 'both: %.3f-%.3f'%rng(n,d,'ab'))
opt=[x for sp,tg in CELLS for n,d in [('B','A'),('D','E')] for x in rat(n,d,sp,tg) if x]
print('opt pairs', len(opt), 'min %.4f max %.4f all>1'%(min(opt),max(opt)), all(x>1 for x in opt))
res=[1/G[(n,d,sp,tg)] for n,d in [('E','A'),('D','B')] for sp,tg in CELLS]; print('resolution saving 1/gm: %.3f-%.3f'%(min(res),max(res)))
da=[(sp,tg,s,x) for sp,tg in CELLS for s,x in zip(S,rat('D','A',sp,tg)) if x]; print('D/A pairs', len(da), 'le1', [z for z in da if z[3]<=1.0], 'lt1.0 exact', [z for z in da if z[3]<1])
# censored cells where COBYQA attains but SLSQP at same resolution does not
cens=[(sp,tg,s,n,d) for sp,tg in CELLS for n,d in [('B','A'),('D','E')] for s in S if tt[(n,sp,s,tg,1)] and not tt[(d,sp,s,tg,1)]]
print('COBYQA attains, SLSQP same-res not (repeat1):', cens)
cens2=[(sp,tg,s,n,d) for sp,tg in CELLS for n,d in [('B','A'),('D','E')] for s in S if tt[(d,sp,s,tg,1)] and not tt[(n,sp,s,tg,1)]]
print('SLSQP attains, COBYQA same-res not:', cens2)
# A attains every target that any method reaches
print('A misses where another reaches:', [(sp,tg,s) for sp,tg in CELLS for s in S if not tt[('A',sp,s,tg,1)] and any(tt[(m,sp,s,tg,1)] for m in 'BCDE')])
print('C T99 b', sum(bool(tt[('C','b',s,'T99',1)]) for s in S), 'C T90 a', sum(bool(tt[('C','a',s,'T90',1)]) for s in S))
print('T99 b per method', {m: sum(bool(tt[(m,'b',s,'T99',1)]) for s in S) for m in 'ABCDE'})
# best credited (no target cond)
def best(r): 
    xs=[v['F_val'] for v in r['validations'] if mycredit(v)]; return min(xs) if xs else None
for sp in 'ab':
    for m in 'ABCDE':
        bs=[best(R[(m,sp,s,1)]) for s in S]; b2=[best(R[(m,sp,s,2)]) for s in S]
        print(sp, m, 'r1', ['%.4f'%(x/F0) if x else '-' for x in bs], 'r2 same?', all((a==b) for a,b in zip(bs,b2)))
print('E b lhs1 best', best(R[('E','b','lhs1',1)])/F0, 'E b lhs1 T50', tt[('E','b','lhs1','T50',1)])
print('min space A (all methods, both repeats)', min(best(r) for r in runs if r['space']=='a' and best(r))/F0)
print('nocredit', sorted(r['file'] for r in runs if best(r) is None))
print('in-run total h', sum(r['in_run_wall_seconds'] for r in runs)/3600, 'unit', set(round(r['unit_seconds'],6) for r in runs))
print('stop reasons', statistics.multimode([r['stop_reason'] for r in runs]), sorted(set(r['stop_reason'] for r in runs)))
print('contaminated', sum(r['host_contaminated'] for r in runs))
json.dump({'tt': {'|'.join(map(str,k)):v for k,v in tt.items()}}, open(sys.argv[2],'w'))
