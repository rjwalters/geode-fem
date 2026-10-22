import json, math, statistics as st, sys
from pathlib import Path
from collections import Counter
D = Path(sys.argv[1]) / 'figures/src/data'
med=st.median
M=json.loads((D/'exp2/runs.json').read_text()); X=M['runs']; byf={r['file']:r for r in X}
SUP=json.loads((D/'exp2/supplement.json').read_text())['runs']
u=X[0]['unit_seconds']
ch=[]; 
for s in SUP:
    r=byf[s['file']]
    for c,v,n in zip(s['validation_charges_seconds'], r['validations']+[None]*5, s['validation_charge_notes']):
        if v is not None and v['status']=='ok': ch.append(c/u)
print('main: completed validation charges n', len(ch), 'median', round(med(ch),3), 'range', round(min(ch),2), round(max(ch),2))
tot=sum(s['total_seconds'] for s in SUP); val=sum(s['category_seconds']['validation'] for s in SUP)
print('validation share of all charged', round(val/tot,3), 'per run', round(min(s['category_seconds']['validation']/s['total_seconds'] for s in SUP),3), round(max(s['category_seconds']['validation']/s['total_seconds'] for s in SUP),3))
print('notes', Counter(n for s in SUP for n in s['validation_charge_notes'] if n))
# ablation costs
for d,arm in (('exp2b','F'),('exp2c','W')):
    XB=json.loads((D/d/'runs.json').read_text())['runs']
    for sp in 'ab':
        ap=[r for r in XB if r['method']=='Ap' and r['space']==sp]
        fa=[r for r in XB if r['method']==arm and r['space']==sp]
        uu=XB[0]['unit_seconds']
        adj=med(r['category_seconds']['adjoint']/r['gradient_train_events']/uu for r in ap)
        fdc=[g['cost_fe'] for r in fa for g in r['fd_gradients'] if g.get('cost_fe') is not None]
        print(f' {d} {sp}: adjoint/grad (median over runs) {adj:.4f}; {arm} FD grad median {med(fdc):.3f} (n={len(fdc)}); ratio {med(fdc)/adj:.1f}; per stencil {med(fdc)/(8 if sp=="a" else 18):.3f}')
        if d=='exp2c':
            st_=[c/uu for r in fa for c in r['stencil_charges_seconds']]; bs=[c/uu for r in fa for c in r['base_charges_seconds']]; bsa=[c/uu for r in ap for c in r['base_charges_seconds']]
            print(f'    stencil charge median {med(st_):.3f}; Fw base {med(bs):.3f}; Ap base {med(bsa):.3f}; n stencils {len(st_)}')
            tn=[n for r in fa for n in r.get('stencil_newton',[])] if 'stencil_newton' in fa[0] else None
            print('    keys', [k for k in fa[0] if 'newton' in k or 'stencil' in k])
        gtot=sum(len(r['fd_gradients']) for r in fa); 
        if d=='exp2c': print('    Fw gradients', gtot, 'base_resolved', Counter(g.get('base_resolved') for r in fa for g in r['fd_gradients']))
        bk=sum(g['backward'] for r in fa for g in r['fd_gradients']); cm=sum(g['components'] for r in fa for g in r['fd_gradients'])
        print(f'    backward components {bk}/{cm}', 'refused', sum(g['refused'] for r in fa for g in r['fd_gradients']))
