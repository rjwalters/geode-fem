# F from raw torques in refinement.json; selection-rule re-derivation from runs.json
import json, sys, math
from pathlib import Path
V=Path(sys.argv[1]); D=V/'figures/src/data/exp2'
X=json.loads((D/'runs.json').read_text()); RF=json.loads((D/'refinement.json').read_text())
A=X['anchor']; F0=A['F0']; floor=A['mean_floor']; T={k:v*F0 for k,v in A['target_fractions'].items()}
mx=0; n=0
for e in RF['designs'].values():
    for ch,q in e['checks'].items():
        F=0
        for c in q['currents']:
            t=c['torques']; m=sum(t)/len(t); F+=sum((x-m)**2 for x in t)/len(t)
        mx=max(mx,abs(F/q['F']-1)); n+=1
print('F from raw torques: checks',n,'max rel dev',mx)
ok=lambda v: v['status']=='ok' and v['admissible'] and v['mean_full'] is not None and v['mean_full']>=floor and v['wall_fe']<=120
want={}
for m in 'ABCDE':
    for sp in 'ab':
        cand=[(v['F_val'],{'theta0':0,'lhs1':1,'lhs2':2,'lhs3':3,'lhs4':4}[r['start']],r['repeat'],v['index'],r['file']) for r in X['runs'] if r['method']==m and r['space']==sp for v in r['validations'] if ok(v)]
        b=min(cand); want[f'representative/{m}/{sp}']=(b[4],b[3])
        for tg,Tv in T.items():
            hits=[(v['wall_fe'],r['file'],v['index']) for r in X['runs'] if r['method']==m and r['space']==sp and r['repeat']==1 for v in r['validations'] if ok(v) and v['F_val']+max(.1*v['F_val'],2.5e-7)<=Tv]
            if hits: h=min(hits); want[f'crossing/{m}/{sp}/{tg}']=(h[1],h[2])
got={}
for e in RF['designs'].values():
    for role,src in zip(e['roles'],e['sources']):
        if src: got[role]=(src['run'],src['validation'])
print('roles derived',len(want),'in refinement.json',len(got),'mismatches',[k for k in set(want)|set(got) if want.get(k)!=got.get(k)])
