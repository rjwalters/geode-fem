# Independent regeneration of Table 7 cells from refinement.json, compared to tables/exp2-refinement.tex
import json, sys, re
from pathlib import Path
V=Path(sys.argv[1]); RF=json.loads((V/'figures/src/data/exp2/refinement.json').read_text())
F0=json.loads((V/'figures/src/data/exp2/runs.json').read_text())['anchor']['F0']
tex=(V/'tables/exp2-refinement.tex').read_text()
rows={}
for l in tex.splitlines():
    if '&' in l and not l.startswith('Design') and not l.strip().startswith('&'):
        c=[x.strip().rstrip('\\').strip() for x in l.split('&')]; rows.setdefault(c[0],[]).append(c)
DS=list(RF['designs'].values()); th={e['space']:e for e in DS if e['roles'][0].startswith('theta0')}
def label(e):
    out=[]
    for r in e['roles']:
        p=r.split('/')
        if p[0]=='theta0': out.append('$\\theta_0$')
        elif p[0]=='representative': out.append(f'best {p[1]}')
        else: out.append(f'{p[1]} $T_{{{p[3][1:]}}}$')
    return ', '.join(out)
def f(x,d,sign=True):
    s=f'{x:+.{d}f}' if sign else f'{x:.{d}f}'
    if s in ('+0.0','-0.0'): s='0.0'
    return s
bad=0; seen=0
for sp in 'ab':
    for e in DS:
        if e['space']!=sp: continue
        lab=label(e); c=e['checks']; p=c['primary']['F']
        mine=[lab, f(p/F0,4,False), f'${f(100*(c["L3"]["F"]/p-1),1)}$', f'${f(1e4*(c["g40"]["F"]/p-1),1)}$', f'${f(1e4*(c["tight"]["F"]/p-1),1)}$', f'${f(1e4*(c["shifted"]["F"]/p-1),1)}$',
              (f'${f(100*(c["conforming"]["F"]/p-1),1)}$' if 'conforming' in c else '---'), f(c['withheld']['F']/th[sp]['checks']['withheld']['F'],4,False)]
        cand=[r for r in rows.get(lab,[])]
        # disambiguate theta0 rows by order
        if lab=='$\\theta_0$': cand=[rows[lab][0 if sp=='a' else 1]]
        elif len(cand)>1: cand=[r for r in cand if (sp=='a')==(tex.index(' & '.join(r))<tex.index('Space B'))]
        seen+=1
        if not cand or cand[0]!=mine:
            bad+=1; print('DIFF',sp,mine,cand)
print('rows checked',seen,'diffs',bad)
