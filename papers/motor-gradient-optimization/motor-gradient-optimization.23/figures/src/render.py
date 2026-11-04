"""Regenerate paper figures and tables from immutable, hash-checked evidence.

Run: uv run --with numpy==2.5.4 --with matplotlib==3.11.2 --with meshio==5.3.5 python figures/src/render.py
No FEM solves are performed. Numerical checks compare raw runs with the
independently reviewed summary; the manuscript tables are derived from raw runs.
"""
from pathlib import Path
import hashlib
import json
import statistics
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import PolyCollection
from matplotlib.patches import Patch
import meshio

V = Path(__file__).resolve().parents[2]
D = V / 'figures/src/data'
F = V / 'figures'
for item in json.loads((V / 'evidence-manifest.json').read_text())['files']:
    assert hashlib.sha256((V / item['snapshot']).read_bytes()).hexdigest() == item['sha256'], item
ref = json.loads((D / 'reference_data.json').read_text())['designs']['initial']
R = np.array(ref['torque_Nm_for_modeled_sector_by_current_then_angle'])
S = json.loads((D / 'summary.json').read_text())
runs = [json.loads((D / f'L{lv}_table_default_mesh.json').read_text()) for lv in range(4)]
T = np.zeros((4, 3, 20))
for lv, run in enumerate(runs):
    assert len(run['rows']) == 60
    for q in range(3):
        rows = sorted((r for r in run['rows'] if r['current_index'] == q), key=lambda r:r['angle_deg'])
        assert [r['angle_deg'] for r in rows] == list(range(20))
        assert all(r['status'] == 'tolerance' for r in rows)
        T[lv, q] = [r['T_arkkio'] for r in rows]
        s = S['comparison_20_angles'][f'I{q}_L{lv}']
        assert abs(statistics.mean(T[lv,q]) - s['mean']) < 1e-13
        assert abs(statistics.pstdev(T[lv,q]) - s['std']) < 1e-13
        assert abs(statistics.pstdev(R[q]) - s['ref_std']) < 1e-13

plt.rcParams.update({'font.size':9, 'axes.titlesize':10, 'axes.labelsize':9,
                     'axes.spines.top':False, 'axes.spines.right':False,
                     'pdf.fonttype':42, 'savefig.bbox':'tight'})
col = ['#b8d4df','#78a8bf','#377fa2','#00557f']
orange = '#bd4e16'
def save(fig,name):
    fig.savefig(F/f'{name}.pdf', metadata={'CreationDate':None,'ModDate':None})
    fig.savefig(F/f'{name}.png',dpi=160)
    plt.close(fig)

# v18: the annotated geometry figure (parts, design parameters, control points) has its own script.
import runpy; runpy.run_path(str(Path(__file__).with_name('render_geometry.py')))

fig,axs=plt.subplots(2,3,figsize=(6.5,4.5),sharex=True)
x=np.arange(20)
for q,label in enumerate(['Zero current','Half current','Full current']):
    for lv in range(4):axs[0,q].plot(x,T[lv,q],color=col[lv],lw=1 if lv<3 else 1.7,label=f'L{lv}')
    axs[0,q].plot(x,R[q],color=orange,ls='--',lw=1.4,label='Reference')
    axs[0,q].set_title(label)
    axs[1,q].axhline(0,color='#888',lw=.6)
    axs[1,q].plot(x,1000*(T[3,q]-R[q]),color=col[3],lw=1.5)
    band=np.maximum(abs(T[2,q]-T[1,q]),abs(T[3,q]-T[2,q]))*1000
    axs[1,q].fill_between(x,1000*(T[3,q]-R[q])-band,1000*(T[3,q]-R[q])+band,color=col[1],alpha=.4)
    axs[1,q].set_xlabel('Mechanical angle (deg)')
    for a in axs[:,q]:a.grid(alpha=.2);a.set_xticks([0,5,10,15,19])
axs[0,0].set_ylabel('Sector torque (N m)');axs[1,0].set_ylabel('L3 − reference (mN m)')
handles,labels=axs[0,0].get_legend_handles_labels()
fig.legend(handles,labels,ncol=5,loc='upper center',frameon=False,bbox_to_anchor=(.5,1.015))
fig.tight_layout(rect=(0,0,1,.96));save(fig,'waveforms')

fig,axs=plt.subplots(1,2,figsize=(6.5,2.8))
for q,(label,c,mark) in enumerate(zip(['Zero','Half','Full'],['#62528a','#007e80','#bd4e16'],['s','o','^'])):
    vals=T[:,q].std(axis=1); r=R[q].std()
    axs[0].plot(range(4),(vals/r-1)*100,marker=mark,color=c,label=label)
    band=max(abs(vals[2]-vals[1]),abs(vals[3]-vals[2]))/r*100
    axs[0].errorbar(3,(vals[3]/r-1)*100,yerr=band,color=c,capsize=4)
    inc=np.max(abs(np.diff(T[:,q],axis=0)),axis=1)
    axs[1].semilogy([1,2,3],inc,marker=mark,color=c)
axs[0].axhline(0,color='#555',ls='--',lw=.8)
axs[0].set(xlabel='Mesh level',ylabel='Ripple deviation from reference (%)',xticks=range(4),title='(a) Population standard deviation')
axs[1].set(xlabel='Finer level in successive pair',ylabel='Max. torque increment (N m)',xticks=[1,2,3],title='(b) Spatial refinement')
axs[0].legend(frameon=False,fontsize=8);axs[1].grid(alpha=.2);fig.tight_layout();save(fig,'convergence')

lines=[]
for q,label in enumerate(['0','0.5','1']):
    a,b=T[3,q].mean(),R[q].mean(); c,d=T[3,q].std(),R[q].std()
    dm='n/a' if q==0 else f'{100*(a/b-1):+.2f}'
    lines.append(f'{label} & {a:.5f} & {b:.5f} & {dm} & {c:.5f} & {d:.5f} & {100*(c/d-1):+.1f} '+r'\\')
(V/'tables/torque.tex').write_text(r'\begin{tabular}{rrrrrrr}\toprule'+'\n'+r'$a$ & $\bar T_{\rm L3}$ & $\bar T_{\rm ref}$ & $\Delta\bar T$ (\%) & $\sigma_{\rm L3}$ & $\sigma_{\rm ref}$ & $\Delta\sigma$ (\%)\\\midrule'+'\n'+'\n'.join(lines)+'\n'+r'\bottomrule\end{tabular}'+'\n')
lines=[]
for lv,run in enumerate(runs):
    rows=run['rows'];med=statistics.median(r['wall_s'] for r in rows)
    lines.append(f'L{lv} & {run["nodes"]:,} & {med:.2f} & {sum(r["wall_s"] for r in rows):.1f} '+r'\\')
(V/'tables/cost.tex').write_text(r'\begin{tabular}{lrrr}\toprule'+'\n'+r'Mesh & Nodes & Median (s) & Sum, 60 solves (s)\\\midrule'+'\n'+'\n'.join(lines)+'\n'+r'\bottomrule\end{tabular}'+'\n')
facts={'mean_relative_error_percent':[100*(T[3,q].mean()/R[q].mean()-1) for q in [1,2]],
       'ripple_relative_error_percent':[100*(T[3,q].std()/R[q].std()-1) for q in range(3)],
       'baseline_solves':sum(len(r['rows']) for r in runs)}
(V/'verified-numbers.json').write_text(json.dumps(facts,indent=2)+'\n')
print('PASS: all snapshot hashes, 12 mean/std rows, reference statistics, angle grids and solver statuses; three figures and two tables generated.')
