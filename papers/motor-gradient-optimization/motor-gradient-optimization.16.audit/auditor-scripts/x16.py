"""Independent recomputation for the v16 audit (stdlib only). Recomputes target credit from raw validation rows."""
import json, math, statistics as st, sys
from pathlib import Path
D = Path(sys.argv[1]) / 'figures/src/data'
gm = lambda xs: math.exp(sum(map(math.log, xs)) / len(xs))
M = json.loads((D/'exp2/runs.json').read_text()); A = M['anchor']; F0=A['F0']; FLOOR=A['mean_floor']
TG = A['targets']
def credit(v, T):
    if v.get('status')!='ok' or v.get('F_val') is None: return False
    dF = max(0.1*v['F_val'], 2.5e-7)
    return v['F_val']+dF <= T*(1+1e-15) and v.get('admissible') and v.get('mean_full') is not None and v['mean_full'] >= FLOOR
def first(r, t):
    for v in r['validations']:
        if credit(v, TG[t]) and v['wall_fe'] <= r['budget_fe']: return v['wall_fe'], v['index']
    return None, None
STARTS=['theta0','lhs1','lhs2','lhs3','lhs4']
def table(runs, rep=1):
    T={}
    for r in runs:
        if r['repeat']!=rep: continue
        for t in TG:
            w,i=first(r,t); T[(r['method'],r['space'],r['start'],t)]=(w,i)
    return T
X=M['runs']; T1=table(X); T2=table(X,2)
# consistency with stored attainment
mis=0
for r in X:
    for t in TG:
        w,i=first(r,t); a=r['attainment'][t]
        if (w is None)!=(not a['attained']) or (w is not None and abs(w-a['wall_fe'])>1e-12): mis+=1
print('main: credit mismatches vs stored attainment =', mis)
print('Table exp2-time (repeat1):')
for sp,ts in (('a',['T50','T90']),('b',['T50','T90','T99'])):
    for t in ts:
        for m in 'ABEDC':
            print(' ',sp,t,m,' '.join('---' if T1[(m,sp,s,t)][0] is None else f'{T1[(m,sp,s,t)][0]:.1f}' for s in STARTS))
print('T99 attained in space A by any run:', any(first(r,'T99')[0] is not None for r in X if r['space']=='a'))
def ratios(num,den,sp,t,T=T1):
    out=[]
    for s in STARTS:
        a=T[(num,sp,s,t)][0]; b=T[(den,sp,s,t)][0]
        out.append(None if a is None or b is None else a/b)
    return out
allopt=[]; 
for sp,ts in (('a',['T50','T90']),('b',['T50','T90','T99'])):
    for t in ts:
        for n,d in (('B','A'),('D','E'),('E','A'),('D','B'),('D','A')):
            rr=ratios(n,d,sp,t); f=[x for x in rr if x]
            print(f'  {sp} {t} {n}/{d} n={len(f)} gm={gm(f):.2f} per-start=',', '.join('---' if x is None else f'{x:.2f}' for x in rr))
            if (n,d) in (('B','A'),('D','E')): allopt+=f
print('optimizer-contrast finite pairs', len(allopt), 'all>1', all(x>1 for x in allopt), f'range {min(allopt):.2f}-{max(allopt):.2f}')
# repeats: crossings attained in both repeats
diffs=[]
for k,(w1,_) in T1.items():
    w2=T2[k][0]
    if w1 is not None and w2 is not None: diffs.append(abs(w2-w1)/w1)
print('crossings in both repeats', len(diffs), f'median {100*st.median(diffs):.2f}% max {100*max(diffs):.2f}%')
# checkpoint
late=[(k,w) for k,(w,_) in T1.items() if w is not None and w>60]
print('repeat-1 crossings after 60 units:', late)
# validation stats
vals=[v for r in X for v in r['validations'] if v['status']=='ok' and v.get('solve_fe') is not None]
print('completed validations', len(vals), 'median solve_fe', round(st.median(v['solve_fe'] for v in vals),3), 'range', round(min(v['solve_fe'] for v in vals),2), round(max(v['solve_fe'] for v in vals),2))
from collections import Counter
print('stop reasons', Counter(r['stop_reason'] for r in X))
print('max in-run load', max(r['in_run_load_max'] for r in X), 'unit', X[0]['unit_seconds'])
print('in-run CPU-hours', sum(r['in_run_wall_seconds'] for r in X)/3600)
# best designs space A
for sp in 'ab':
    for m in 'ABCDE':
        best=min((v['F_val'] for r in X if r['space']==sp and r['method']==m for v in r['validations'] if v['status']=='ok' and v.get('F_val') is not None and v.get('admissible') and v['mean_full']>=FLOOR), default=None)
        print('  best feasible',sp,m, None if best is None else round(best/F0,4))
# ---------------- ablations
for d,arm in (('exp2b','F'),('exp2c','W')):
    XB=json.loads((D/d/'runs.json').read_text())['runs']
    print('==',d, len(XB),'runs; methods', Counter(r['method'] for r in XB))
    TB=table(XB); TB2=table(XB,2)
    for sp,ts in (('a',['T50','T90']),('b',['T50','T90','T99'])):
        for t in ts:
            ap=[TB[('Ap',sp,s,t)][0] for s in STARTS]; f=[TB[(arm,sp,s,t)][0] for s in STARTS]
            rr=[b/a for a,b in zip(ap,f)]
            line=f'  {sp} {t} Ap '+' '.join(f'{x:.1f}' for x in ap)+f' | {arm} '+' '.join(f'{x:.1f}' for x in f)+' | ratio '+', '.join(f'{x:.2f}' for x in rr)+f' gm {gm(rr):.2f} range {min(rr):.2f}-{max(rr):.2f}'
            if any(r['repeat']==2 for r in XB):
                rr2=[TB2[(arm,sp,s,t)][0]/TB2[('Ap',sp,s,t)][0] for s in STARTS]; line+=f' rep2 gm {gm(rr2):.2f}'
            print(line)
    # same validation index at crossings
    same=[(sp,s,t,TB[('Ap',sp,s,t)][1],TB[(arm,sp,s,t)][1]) for sp,ts in (('a',['T50','T90']),('b',['T50','T90','T99'])) for t in ts for s in STARTS if TB[('Ap',sp,s,t)][1]!=TB[(arm,sp,s,t)][1]]
    print('  crossings at different validation index:', same)
    globals()['T_'+arm]=TB; globals()['X_'+arm]=XB
# B/F_w and chain
for sp,t in (('a','T90'),('b','T99')):
    bw=[]; ch=[]; ba=[]; wa=[]; bf=[]; sec=[]
    for s in STARTS:
        b=T1[('B',sp,s,t)][0]; w=T_W[('W',sp,s,t)][0]; ap=T_W[('Ap',sp,s,t)][0]; a=T1[('A',sp,s,t)][0]; f=T_F[('F',sp,s,t)][0]
        if b is None: continue
        bw.append(b/w); ba.append(b/a); wa.append(w/ap); bf.append(b/f)
        uB=X[0]['unit_seconds']; uW=X_W[0]['unit_seconds']
        sec.append(b*uB/(w*uW))
    print(f' {sp} B/Fw per start', ', '.join(f'{x:.2f}' for x in bw), f'gm {gm(bw):.2f}; seconds gm {gm(sec):.2f}; B/A gm {gm(ba):.2f}; Fw/Ap gm over same starts {gm(wa):.2f}; product {gm(wa)*gm(bw):.2f}; B/F per start', ', '.join(f'{x:.2f}' for x in bf), f'gm {gm(bf):.2f}')
    fa=[T_F[('F',sp,s,t)][0]/T_F[('Ap',sp,s,t)][0] for s in STARTS if T1[('B',sp,s,t)][0] is not None]
    print(f'    F/Ap over B-attaining starts gm {gm(fa):.2f}; B/A / that = {gm(ba)/gm(fa):.2f}')
# cross-session seconds Fw vs F
for sp,t in (('a','T90'),('b','T99')):
    r=[T_W[('W',sp,s,t)][0]*X_W[0]['unit_seconds']/(T_F[('F',sp,s,t)][0]*X_F[0]['unit_seconds']) for s in STARTS]
    print(f' Fw/F seconds {sp}: gm {gm(r):.3f}')
# Ap vs main A control
for nm,TB,XB in (('F',T_F,X_F),('W',T_W,X_W)):
    r=[TB[('Ap',sp,s,t)][0]/T1[('A',sp,s,t)][0] for sp,t in (('a','T90'),('b','T99')) for s in STARTS]
    print(f' {nm}-session Ap / main A at primary: {min(r):.3f}-{max(r):.3f}')
r=[T_W[('Ap',sp,s,t)][0]/T_F[('Ap',sp,s,t)][0] for sp,t in (('a','T90'),('b','T99')) for s in STARTS]
print(f' Ap(Fw session)/Ap(F session) primary, in units: {min(r):.3f}-{max(r):.3f}')
print(' units', X_F[0]['unit_seconds'], X_W[0]['unit_seconds'], 'loads', max(r['in_run_load_max'] for r in X_F), max(r['in_run_load_max'] for r in X_W))
print(' Fw stop reasons', Counter((r['space'],r['method'],r['stop_reason']) for r in X_W))
print(' F stop reasons (space a, rep1+2)', Counter((r['method'],r['stop_reason']) for r in X_F if r['space']=='a'))
print(' F repeat noise primary:', [round(abs(T_F[(m,sp,s,t)][0]-table(X_F,2)[(m,sp,s,t)][0])/T_F[(m,sp,s,t)][0]*100,2) for m in ('F',) for sp,t in (('a','T90'),('b','T99')) for s in STARTS])
