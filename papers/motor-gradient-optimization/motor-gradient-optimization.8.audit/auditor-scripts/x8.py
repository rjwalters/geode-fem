# Independent v8 Exp-2 recomputation (auditor-written; does not import verify_exp2.py)
import json, math, sys, statistics as st, hashlib, collections, re
from pathlib import Path
V = Path(sys.argv[1]); D = V/'figures/src/data/exp2'
X = json.loads((D/'runs.json').read_text()); RF = json.loads((D/'refinement.json').read_text())
A = X['anchor']; F0 = A['F0']; floor = A['mean_floor']; mu0 = A['mean0_full_current']
print('F0', F0, 'mu0', mu0, 'floor/mu0', floor/mu0)
T = {k: v*F0 for k, v in A['target_fractions'].items()}
runs = X['runs']
# own credit rule: completed ok, admissible, validated mean >= floor (own comparison), within budget
def ok(v):
    return v['status'] == 'ok' and v['admissible'] and v['mean_full'] is not None and v['mean_full'] >= floor and v['wall_fe'] <= 120.0
# cross-check against harness flags
mism = [(r['file'], v['index']) for r in runs for v in r['validations'] if v['status']=='ok' and ok(v) != (v['credited'] and v['admissible'] and v['mean_ok'])]
print('credit-rule mismatches vs harness flags:', len(mism), mism[:5])
R = {(r['method'], r['space'], r['start'], r['repeat']): r for r in runs}
S = ['theta0','lhs1','lhs2','lhs3','lhs4']
def tt(m, sp, s, tg, rep=1):
    for v in R[(m,sp,s,rep)]['validations']:
        if ok(v) and v['F_val'] + max(0.1*v['F_val'], 2.5e-7) <= T[tg]:
            return v['wall_fe']
    return None
# Table 5
lines=[]
for sp, tgs in (('a',['T50','T90']),('b',['T50','T90','T99'])):
    for tg in tgs:
        for m in 'ABEDC':
            lines.append(f"{sp} {tg} {m}: " + ', '.join('---' if tt(m,sp,s,tg) is None else f"{tt(m,sp,s,tg):.1f}" for s in S))
print('\n'.join(lines))
print('any space-A T99 (both reps):', any(tt(m,'a',s,'T99',r) for m in 'ABCDE' for s in S for r in (1,2)))
# Table 6
def rat(n,d,sp,tg):
    out=[]
    for s in S:
        a,b=tt(n,sp,s,tg),tt(d,sp,s,tg)
        out.append(a/b if a and b else None)
    return out
gm=lambda xs: math.exp(sum(math.log(x) for x in xs)/len(xs))
cells=[('a','T50'),('a','T90'),('b','T50'),('b','T90'),('b','T99')]
G=collections.defaultdict(dict)
for sp,tg in cells:
    for n,d in (('B','A'),('D','E'),('E','A'),('D','B'),('D','A')):
        r=rat(n,d,sp,tg); f=[x for x in r if x]
        G[(n,d)][(sp,tg)]=gm(f)
        print(sp,tg,f"{n}/{d}",len(f),f"{gm(f):.4f}",', '.join('---' if x is None else f"{x:.2f}" for x in r))
opt=[x for sp,tg in cells for n,d in (('B','A'),('D','E')) for x in rat(n,d,sp,tg) if x]
print('optimizer pairs',len(opt),'min',min(opt),'max',max(opt),'all>1',all(x>1 for x in opt))
for key in [('B','A'),('D','E'),('E','A'),('D','B'),('D','A')]:
    for spc in 'ab':
        vals=[v for (sp,tg),v in G[key].items() if sp==spc]
        print(key,spc,'gm range',round(min(vals),4),round(max(vals),4))
og=[v for k in [('B','A'),('D','E')] for v in G[k].values()]; print('opt gm overall',min(og),max(og))
res=[1/v for k in [('E','A'),('D','B')] for v in G[k].values()]; print('resolution saving',min(res),max(res))
da=[x for sp,tg in cells for x in rat('D','A',sp,tg) if x]; print('D/A pairs',len(da),'<=1:',[round(x,4) for x in da if x<=1])
# E failures; D-attains-E-misses
print('D attains, E misses:',[(sp,tg,s) for sp,tg in cells for s in S if tt('D',sp,s,tg) and not tt('E',sp,s,tg)])
print('A attains all where any:',all(tt('A',sp,s,tg) for sp,tg in cells for s in S if any(tt(m,sp,s,tg) for m in 'ABCDE')))
print('C T99 in b:',sum(bool(tt('C','b',s,'T99')) for s in S),'C T90 in a:',sum(bool(tt('C','a',s,'T90')) for s in S))
best=lambda r: min([v['F_val'] for v in r['validations'] if ok(v)], default=None)
for rep in (1,2):
    print('rep',rep,'no credited validation:',sorted(r['file'] for r in runs if r['repeat']==rep and best(r) is None))
print('E b lhs1 best/F0',best(R[('E','b','lhs1',1)])/F0, 'reaches T50 at', tt('E','b','lhs1','T50'))
# space A plateau / best per method
for sp in 'ab':
    for m in 'ABCDE':
        rr=[r for r in runs if r['method']==m and r['space']==sp]
        b=min((v for r in rr for v in r['validations'] if ok(v)), key=lambda v:v['F_val'])
        print('best',sp,m,round(b['F_val']/F0,5),'mean/floor-1 %',round(100*(b['mean_full']/floor-1),4))
print('min over all space-A runs F/F0',min(best(r)/F0 for r in runs if r['space']=='a' and best(r)))
print('min over space-A F_upper/F0',min((v['F_val']+max(.1*v['F_val'],2.5e-7))/F0 for r in runs if r['space']=='a' for v in r['validations'] if ok(v)))
for m in 'ABCDE':
    print('LHS4 space a',m,[None if best(R[(m,'a','lhs4',k)]) is None else round(best(R[(m,'a','lhs4',k)])/F0,4) for k in (1,2)])
for m in 'ABCDE':
    print('space b T99 starts',m,sum(bool(tt(m,'b',s,'T99')) for s in S))
# repeats
same=0; difflen=[]
for m in 'ABCDE':
    for sp in 'ab':
        for s in S:
            a1=[v['F_val'] for v in R[(m,sp,s,1)]['validations']]; a2=[v['F_val'] for v in R[(m,sp,s,2)]['validations']]
            n=min(len(a1),len(a2)); same+= a1[:n]==a2[:n]
            if len(a1)!=len(a2): difflen.append((m,sp,s,len(a1),len(a2),R[(m,sp,s,1)]['stop_reason'],R[(m,sp,s,2)]['stop_reason']))
print('pairs same prefix',same,'/50; length differs:',difflen)
d=[(tt(m,sp,s,tg,2)/tt(m,sp,s,tg,1)-1, m,sp,s,tg) for m in 'ABCDE' for sp in 'ab' for s in S for tg in T if tt(m,sp,s,tg,1) and tt(m,sp,s,tg,2)]
ad=[abs(x[0]) for x in d]
print('both-repeat crossings',len(d),'median %',100*st.median(ad),'max %',100*max(ad), max(d,key=lambda x:abs(x[0])))
print('repeat-1 crossings',sum(1 for r in runs if r['repeat']==1 for tg in T if tt(r['method'],r['space'],r['start'],tg)))
print('repeat-2 crossings',sum(1 for r in runs if r['repeat']==2 for tg in T if tt(r['method'],r['space'],r['start'],tg,2)))
late=[(r['method'],r['space'],r['start'],tg,round(tt(r['method'],r['space'],r['start'],tg,1),1)) for r in runs if r['repeat']==1 for tg in T if tt(r['method'],r['space'],r['start'],tg) and tt(r['method'],r['space'],r['start'],tg)>60]
print('repeat-1 after 60:',late)
# stops / session
print(collections.Counter(r['stop_reason'] for r in runs))
print([(r['method'],r['space'],r['start'],r['repeat'],round(r['in_run_wall_seconds']/r['unit_seconds'],1)) for r in runs if r['stop_reason']=='solver_stopped'])
print('load max',max(r['in_run_load_max'] for r in runs),'contaminated',sum(r['host_contaminated'] for r in runs),'overload',max(r['longest_overload_s'] for r in runs))
print('in-run hours',sum(r['in_run_wall_seconds'] for r in runs)/3600)
SU=D/'provenance/setup'; cal=json.loads((SU/'calibration_primary.json').read_text())['calibration']
print('cal n',len(cal['validation_seconds']),'median',st.median(cal['validation_seconds']),'procs',cal['processes'])
ap=json.loads((SU/'anchor_preflight.json').read_text()); print('anchor rel err',[s['comparison']['F_relative_error'] for s in ap['sessions']],'bitwise',ap['spaces_agree_bitwise'],'tol',ap['relative_tolerance'])
sess=json.loads((SU/'session.json').read_text()); print('sw',{k:sess['linear_algebra'][k] for k in ('machine','numpy','scipy','numpy_blas')},'procs',sess['procs'])
# inventory binding, own hash check
INV=D/'provenance/exp2-main-inventory-0650f4ee.sha256.txt'
inv={}
for l in INV.read_text().splitlines():
    h,p=l.split('  ',1); inv[p]=h
print('inventory sha',hashlib.sha256(INV.read_bytes()).hexdigest()[:16],'entries',len(inv))
bad=[r['file'] for r in runs if inv.get('exp2-main/main/'+r['file'])!=r['sha256']]
print('runs bound',100-len(bad),bad[:3])
print('setup bound',[inv.get('exp2-main/main/'+n)==hashlib.sha256((SU/n).read_bytes()).hexdigest() for n in ('session.json','calibration_primary.json','anchor_preflight.json')])
# ---- Refinement
DS=RF['designs']; print('designs',len(DS),'checks',sum(len(e['checks']) for e in DS.values()),'all ok',all(q['ok'] and q['status']=='complete' for e in DS.values() for q in e['checks'].values()))
th={e['space']:e for e in DS.values() if any(r.startswith('theta0') for r in e['roles'])}
for sp,e in th.items(): print('theta0',sp,'primary/F0-1',e['checks']['primary']['F']/F0-1)
cands=[e for e in DS.values() if not any(r.startswith('theta0') for r in e['roles'])]
byf={r['file']:r for r in runs}
mv,fr=[],[]
for e in cands:
    src=[s for s in e['sources'] if s]
    devs=[]
    for s0 in src:
        v=byf[s0['run']]['validations'][s0['validation']]
        devs.append((v['mesh_kind'],abs(e['checks']['primary']['F']/v['F_val']-1)))
    kinds={k for k,_ in devs}
    (mv if kinds=={'moved'} else fr).append(max(x for _,x in devs))
    if len(kinds)>1: print('mixed kinds',e['roles'])
print('moved',len(mv),max(mv),'fresh',len(fr),max(fr))
rel=lambda e,ch: e['checks'][ch]['F']/e['checks']['primary']['F']-1
L3=[rel(e,'L3') for e in DS.values()]; print('L3 all',min(L3),max(L3))
cr=[e for e in DS.values() if any(r.startswith('crossing') for r in e['roles'])]
ncross=sum(1 for e in DS.values() for r in e['roles'] if r.startswith('crossing'))
print('crossing designs',len(cr),'crossing roles',ncross,'L3 range',min(rel(e,'L3') for e in cr),max(rel(e,'L3') for e in cr))
bestall=min(cands,key=lambda e:e['checks']['primary']['F']); print('overall best',bestall['roles'],bestall['checks']['primary']['F']/F0,bestall['checks']['L3']['F']/F0, rel(bestall,'L3'))
print('argmax L3',max(DS.values(),key=lambda e:rel(e,'L3'))['roles'])
print('g40/shifted max |rel|',max(abs(rel(e,c)) for e in DS.values() for c in ('g40','shifted')),'tight max',max(abs(rel(e,'tight')) for e in DS.values()))
conf=[(e['space'],rel(e,'conforming'),e['roles'][0]) for e in DS.values() if 'conforming' in e['checks']]
print('conforming n',len(conf),'space-A cands',min(x for s,x,r in conf if s=='a' and not r.startswith('theta0')),max(x for s,x,r in conf if s=='a' and not r.startswith('theta0')),'theta0',[ (s,x) for s,x,r in conf if r.startswith('theta0')], 'non-a cands', [r for s,x,r in conf if s!='a' and not r.startswith('theta0')])
m=[]
for e in DS.values():
    for role in e['roles']:
        if role.startswith('crossing'):
            tg=role.split('/')[-1]; F3=e['checks']['L3']['F']; m.append(((T[tg]-F3-max(.1*F3,2.5e-7))/T[tg],role))
print('margins n',len(m),'min',min(m))
mus=[(e['checks'][ch]['full_current_mean']/floor-1,ch,e['roles'][0]) for e in cands for ch in ('primary','L3','g40','tight','shifted')]
print('mean floor min',min(mus))
print('conforming mean/floor-1 min', min(e['checks']['conforming']['full_current_mean']/floor-1 for e in cands if 'conforming' in e['checks']))
held=lambda e:(e['checks']['withheld']['F']/th[e['space']]['checks']['withheld']['F'])/(e['checks']['primary']['F']/F0)
hv=sorted((held(e),e['roles']) for e in cands); print('held min',hv[0],'max',hv[-1])
for e in cands:
    if any(r.startswith('representative') for r in e['roles']): print('rep',e['roles'],round(held(e),4), 'trainF/F0',round(e['checks']['primary']['F']/F0,5),'heldF/F0w',round(e['checks']['withheld']['F']/th[e['space']]['checks']['withheld']['F'],5))
# role list
for e in DS.values(): print(e['space'],e['roles'],e['stator_offsets_zero'])
