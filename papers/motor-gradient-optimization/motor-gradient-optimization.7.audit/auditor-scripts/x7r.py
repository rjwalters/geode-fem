import json, math, sys, re, statistics
from pathlib import Path
V=Path(sys.argv[1]); D=V/'figures/src/data/exp2'
X=json.loads((D/'runs.json').read_text()); RF=json.loads((D/'refinement.json').read_text())
A=X['anchor']; F0=A['F0']; floor=A['mean_floor']; T={k:A['target_fractions'][k]*F0 for k in A['target_fractions']}
DS=RF['designs']; byf={r['file']:r for r in X['runs']}
print('designs',len(DS),'checks',sum(len(e['checks']) for e in DS.values()), 'all ok', all(c['ok'] and c['status']=='complete' and c['error'] is None for e in DS.values() for c in e['checks'].values()))
print('zero offsets', sum(e['stator_offsets_zero'] for e in DS.values()), 'with conforming', sum('conforming' in e['checks'] for e in DS.values()), 'zero-offset spaces', sorted(set((e['space'], e['roles'][0].split('/')[0]) for e in DS.values() if e['stator_offsets_zero'])))
# recompute F from per-current std2 / torques for each check (sum sigma^2), verify stored F
def Fcalc(c):
    s=0
    for cur in c['currents']:
        t=cur['torques']; m=sum(t)/len(t); s+=sum((x-m)**2 for x in t)/len(t)
    return s
mx=max(abs(Fcalc(c)/c['F']-1) for e in DS.values() for c in e['checks'].values()); print('F from raw torques (pop var) max rel dev %.2e'%mx)
# design ids
th={e['space']:e for e in DS.values() if any(r.startswith('theta0') for r in e['roles'])}
cands=[e for e in DS.values() if not any(r.startswith('theta0') for r in e['roles'])]
print('theta0 primary F/F0', [th[s]['checks']['primary']['F']/F0-1 for s in 'ab'], 'same design x', th['a']['x']==th['b']['x'][:len(th['a']['x'])])
# selection rule recompute: representative = min F_val over both repeats among feasible credited (mean_ok, admissible, status ok)
def ok(v): return v['status']=='ok' and v['admissible'] and v['mean_full'] is not None and v['mean_full']>=floor and v['wall_fe']<=120
sel={}
for m in 'ABCDE':
  for sp in 'ab':
    best=None
    for r in X['runs']:
      if r['method']!=m or r['space']!=sp: continue
      for v in r['validations']:
        if ok(v) and (best is None or v['F_val']<best[0]): best=(v['F_val'],r['file'],v['index'])
    sel['representative/%s/%s'%(m,sp)]=best
    for tg in (['T50','T90'] if sp=='a' else ['T50','T90','T99']):
      e=None
      for r in X['runs']:
        if r['method']!=m or r['space']!=sp or r['repeat']!=1: continue
        for v in r['validations']:
          if ok(v) and v['F_val']+max(.1*v['F_val'],2.5e-7)<=T[tg]:
            if e is None or v['wall_fe']<e[0]: e=(v['wall_fe'],r['file'],v['index'])
            break
      sel['crossing/%s/%s/%s'%(m,sp,tg)]=e
roles={}
for e in DS.values():
  for ro,src in zip(e['roles'],e['sources']): roles[ro]=(e,src)
bad=[]
for ro,s in sel.items():
  if s is None: 
    if ro in roles: bad.append(('extra',ro))
    continue
  if ro not in roles: bad.append(('missing',ro)); continue
  e,src=roles[ro]
  if src['run']!=s[1] or src['validation']!=s[2]: bad.append((ro,s,src))
print('selection recompute: roles', len([r for r in sel if sel[r]]), 'in cohort', len([r for r in roles if not r.startswith('theta0')]), 'mismatches', bad)
print('crossings selected', sum(1 for r in sel if r.startswith('crossing') and sel[r]))
# reproduction
mv=[];fr=[]
for e in cands:
  rels=[]
  for src in e['sources']:
    v=byf[src['run']]['validations'][src['validation']]; rels.append((abs(e['checks']['primary']['F']/v['F_val']-1), v['mesh_kind']))
  kinds=set(k for _,k in rels)
  (mv if kinds=={'moved'} else fr).append(max(r for r,_ in rels))
  if len(kinds)>1: print('mixed mesh kinds', e['roles'])
print('moved n=%d max %.3e ; fresh n=%d max %.4e'%(len(mv),max(mv),len(fr),max(fr)))
rel=lambda e,ch: e['checks'][ch]['F']/e['checks']['primary']['F']-1
for ch in ['L3','g40','tight','shifted']:
  xs=[rel(e,ch) for e in DS.values()]; print(ch,'min %+.4f%% max %+.4f%% max|.| %.3e'%(100*min(xs),100*max(xs),max(map(abs,xs))))
L3=sorted(DS.values(), key=lambda e: rel(e,'L3')); print('L3 max design', L3[-1]['roles'], 'min', L3[0]['roles'])
db=roles['representative/D/b'][0]; print('D b best primary/F0 %.5f L3/F0 %.5f'%(db['checks']['primary']['F']/F0, db['checks']['L3']['F']/F0))
ov=min(DS.values(), key=lambda e: e['checks']['primary']['F']); print('overall lowest primary', ov['roles'], ov['checks']['primary']['F']/F0)
ovl3=min(DS.values(), key=lambda e: e['checks']['L3']['F']); print('overall lowest L3', ovl3['roles'], ovl3['checks']['L3']['F']/F0)
conf=[(rel(e,'conforming'),e['roles']) for e in cands if 'conforming' in e['checks']]
print('conf cands min %.4f%% max %.4f%%'%(100*min(conf)[0],100*max(conf)[0]), 'theta0 a %.4f%% b %.4f%%'%(100*rel(th['a'],'conforming'),100*rel(th['b'],'conforming')))
# margins at L3
mg=[]
for e in DS.values():
  for ro in e['roles']:
    tg=ro.split('/')[-1]
    if tg in T:
      F3=e['checks']['L3']['F']; mg.append(((T[tg]-F3-max(.1*F3,2.5e-7))/T[tg],ro))
print('L3 margins n=%d min %.4f%% %s'%(len(mg),100*min(mg)[0],min(mg)[1]))
# also margins under other checks
for ch in ['g40','shifted','tight','primary']:
  m2=[]
  for e in DS.values():
    for ro in e['roles']:
      tg=ro.split('/')[-1]
      if tg in T: Fx=e['checks'][ch]['F']; m2.append((T[tg]-Fx-max(.1*Fx,2.5e-7))/T[tg])
  print('  margin under',ch,'min %.4f%%'%(100*min(m2)))
mu=[(e['checks'][ch]['full_current_mean']/floor-1,ch,e['roles'][0]) for e in cands for ch in ('primary','L3','g40','tight','shifted')]
print('mean floor closest %.5f%%'%(100*min(mu)[0]), min(mu)[1:], 'any below', any(x[0]<0 for x in mu))
mu_conf=[(e['checks']['conforming']['full_current_mean']/floor-1) for e in cands if 'conforming' in e['checks']]; print('  conforming mean vs floor min %.4f%%'%(100*min(mu_conf)))
held=[((e['checks']['withheld']['F']/th[e['space']]['checks']['withheld']['F'])/(e['checks']['primary']['F']/F0),e['space'],e['roles']) for e in cands]
print('held ratio min %.4f max %.4f'%(min(held)[0],max(held)[0]), min(held)[2], max(held)[2])
for h in sorted(held): 
  if any(r.startswith('representative') for r in h[2]): print('   rep', h[1], h[2][0], '%.4f'%h[0])
print('D b withheld/theta0 %.5f'%(db['checks']['withheld']['F']/th['b']['checks']['withheld']['F']))
print('peak current', {c['i_peak_A'] for e in DS.values() for c in e['checks']['primary']['currents']})
# Table 6 regeneration
rows=[]
def lab(e):
  return e['roles']
tab=(V/'tables/exp2-refinement.tex').read_text().splitlines()
trows=[l for l in tab if '&' in l and 'Design' not in l and 'change in' not in l]
print('table6 rows', len(trows))
fmt=lambda x: '$%+.1f$'%(100*x) if round(100*x,1)!=0 else '$0.0$'
gen={}
for e in DS.values():
  vals=['%.4f'%(e['checks']['primary']['F']/F0)]+[fmt(rel(e,c)) for c in ['L3','g40','tight','shifted']]+[fmt(rel(e,'conforming')) if 'conforming' in e['checks'] else '---']+['%.4f'%(e['checks']['withheld']['F']/th[e['space']]['checks']['withheld']['F'])]
  gen[(e['space'],tuple(sorted(e['roles'])))]=vals
tv=[ [x.strip().rstrip('\\').strip() for x in l.split('&')[1:]] for l in trows]
genset=sorted(gen.values()); tvs=sorted(tv)
print('table6 value rows equal as multiset:', genset==tvs)
for a,b in zip(genset,tvs):
  if a!=b: print('  T6 diff', a, b)
