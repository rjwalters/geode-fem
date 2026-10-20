"""Regenerate v6 figures/tables. Uses only pinned records; never runs FEM."""
import runpy
from pathlib import Path
import json,statistics as st
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.ticker import NullLocator
V=Path(__file__).resolve().parents[2];D=V/'figures/src/data';F=V/'figures'
# Preserve and regenerate the original geometry and supplemental plots.
runpy.run_path(str(Path(__file__).with_name('render.py')))
def read(p):return json.loads((D/p).read_text())
def torque(run,key='T_arkkio'):
 return np.array([[r[key] for r in sorted(run['rows'],key=lambda r:r['angle_deg']) if r['current_index']==q] for q in range(3)])
R=np.array(read('reference_data.json')['designs']['initial']['torque_Nm_for_modeled_sector_by_current_then_angle'])
C=np.array([torque(read(f'L{l}_table_default_mesh.json')) for l in range(4)])
H=np.array([torque(read(f'harmonic/runs/L{l}_h45.json')) for l in range(4)])
blue='#704580';orange='#bd4e16';green='#007e80';purple='#62528a'
def save(fig,name):
 fig.savefig(F/f'{name}.pdf',metadata={'CreationDate':None,'ModDate':None});fig.savefig(F/f'{name}.png',dpi=180);plt.close(fig)
x=np.arange(20);fig,ax=plt.subplots(2,3,figsize=(6.5,4.4),sharex=True)
for q,label in enumerate(['Zero current','Half current','Full current']):
 for a,t,c,style,label2 in [(ax[0,q],C[3,q],blue,'-.','Conforming'),(ax[0,q],H[3,q],green,'-','Harmonic (order 45)'),(ax[0,q],R[q],orange,'--','Saved reference')]:
  a.plot(x,t,color=c,ls=style,lw=1.5,label=label2)
 ax[0,q].set_title(label)
 ax[1,q].axhline(0,color='#777',lw=.6)
 ax[1,q].plot(x,1000*(C[3,q]-R[q]),color=blue,lw=1.3,ls='-.')
 ax[1,q].plot(x,1000*(H[3,q]-R[q]),color=green,lw=1.3)
 e=1000*np.maximum(abs(H[3,q]-H[2,q]),abs(H[2,q]-H[1,q]));y=1000*(H[3,q]-R[q])
 ax[1,q].fill_between(x,y-e,y+e,color=green,alpha=.17)
 ax[1,q].set_xlabel('Mechanical angle (deg)')
 for a in ax[:,q]:a.set_xticks([0,5,10,15,19]);a.grid(alpha=.15)
ax[0,0].set_ylabel('Sector torque (N m)');ax[1,0].set_ylabel('L3 − reference (mN m)')
fig.legend(*ax[0,0].get_legend_handles_labels(),ncol=3,loc='upper center',frameon=False,bbox_to_anchor=(.5,1.015));fig.tight_layout(rect=(0,0,1,.95));save(fig,'coupling-waveforms')
fig,ax=plt.subplots(1,2,figsize=(6.5,3.1))
for q,(color,label,marker) in enumerate([(purple,'Zero','s'),(green,'Half','o'),(orange,'Full','^')]):
 for values,style in [(H,'-'),(C,'--')]:
  std=values[:,q].std(axis=1);y=100*(std/R[q].std()-1)
  ax[0].plot(range(4),y,ls=style,color=color,marker=marker,ms=4,label=label if style=='-' else None)
  ax[0].errorbar(3,y[-1],yerr=100*max(abs(std[3]-std[2]),abs(std[2]-std[1]))/R[q].std(),color=color,capsize=3)
 orders=[21,45,93,189,381];yy=[torque(read(f'harmonic/runs/L1_h{n}.json'))[q].std()/C[1,q].std() for n in orders]
 ax[1].semilogx(orders,yy,color=color,marker=marker,ms=4)
ax[0].axhline(0,color='#777',lw=.7);ax[0].set(xlabel='Mesh level',xticks=range(4),ylabel='Ripple deviation from reference (%)',title='(a) Fixed interface model');ax[0].legend(frameon=False,fontsize=8,loc='center right')
ax[1].axhline(1,color='#777',lw=.7,ls='--');ax[1].axvline(45,color='#aaa',lw=.7);ax[1].set(xlabel='Largest transmitted order',ylabel='Ripple / conforming ripple',title='(b) Fixed mesh (L1)',xticks=orders);ax[1].set_xticklabels(orders);ax[1].xaxis.set_minor_locator(NullLocator())
for a in ax:a.grid(alpha=.15)
from matplotlib.lines import Line2D
fig.legend([Line2D([],[],color='#555',ls='-'),Line2D([],[],color='#555',ls='--')],['Harmonic (order 45)','Conforming'],loc='lower center',ncol=2,frameon=False,fontsize=8)
fig.tight_layout(rect=(0,.09,1,1));save(fig,'coupling-convergence')
# Recompute FD error from raw plus/minus evaluations rather than plotting saved errors.
errors=[]
for p in sorted((D/'gradient/runs').glob('study*.json')):
 d=json.loads(p.read_text())
 for cur,c in d['currents'].items():
  if float(cur)==0:continue
  for rule in ['prescribed','jinit']:
   for sweep in c.get('sweep_'+rule,[]):
    if rule=='jinit' and sweep['parameter'] in ['MAG','MH','MW']:
     assert sweep==next(x for x in c['sweep_prescribed'] if x['parameter']==sweep['parameter'])
     continue
    for o,a in enumerate(sweep['adjoint']):
     errors.append(min(abs((r['plus'][o]-r['minus'][o])/(2*r['h'])-a)/abs(a) for r in sweep['rows']))
assert len(errors)==704 and sum(e<=2.8e-7 for e in errors)==688
sample=read('gradient/runs/study_L1_h45_initial.json')['currents']['1'];g=sample['gradient'];sw=next(r for r in sample['sweep_prescribed'] if r['parameter']=='SW1')
fig,ax=plt.subplots(2,2,figsize=(6.5,5.5));a=ax[0,0]
a.semilogy(np.arange(1,705),sorted(errors),color=blue,lw=1.5);a.axhline(2.8e-7,color=orange,ls='--',lw=.8);a.text(25,7e-7,'688 / 704 ≤ 2.8 × 10⁻⁷',fontsize=8)
a.set(xlabel='Case rank',ylabel='Best FD relative error',title='(a) Distinct loaded cases',ylim=(1e-16,1e-4))
a=ax[0,1];hs=np.array([r['h'] for r in sw['rows']]);ii=list(hs).index(1e-5)
for o,(label,color,marker) in enumerate([('Mean',blue,'o'),('Ripple',orange,'s')]):
 vals=np.array([abs(r['plus'][o]-sw['j0'][o]-r['h']*sw['adjoint'][o])/abs(sw['j0'][o]) for r in sw['rows']]);a.loglog(hs,vals,color=color,marker=marker,ms=3,lw=1,label=label)
 a.loglog(hs,vals[ii]*(hs/hs[ii])**2,color=color,ls=':',lw=.8)
a.set(xlabel='SW1 displacement h (m)',ylabel='Remainder / objective',title='(b) First-order Taylor remainder',ylim=(1e-16,1e-1));a.legend(frameon=False,fontsize=8)
a=ax[1,0];idx=np.arange(9)
a.bar(idx-.18,np.array(g['d_mean'][:9]),width=.36,color=blue,label='Fixed current')
a.bar(idx+.18,np.array(g['d_mean_jinit'][:9]),width=.36,color=orange,label='Fixed group-1 density')
a.axhline(0,color='#777',lw=.6);a.set(xticks=idx,xticklabels=['MAG','MH','MW','SW1','SW2','SW3','SW4','SD1','Sr1'],ylabel='Mean-torque derivative (N m/m)',title='(c) Excitation changes sensitivity');a.tick_params(axis='x',labelrotation=50,labelsize=8);a.set_yscale('symlog',linthresh=2);a.set_ylim(-2000,2000);a.set_yticks([-1000,-100,-10,0,10,100,1000]);a.set_yticklabels(['−1000','−100','−10','0','10','100','1000']);a.legend(frameon=False,fontsize=8,loc='upper center',bbox_to_anchor=(.5,-.33),ncol=1);a.annotate('+1.68',(3-.18,g['d_mean'][3]),xytext=(2.6,300),fontsize=8,arrowprops={'arrowstyle':'-','lw':.6});a.annotate('−220.85',(3+.18,g['d_mean_jinit'][3]),xytext=(4,-1400),fontsize=8,arrowprops={'arrowstyle':'-','lw':.6})
a=ax[1,1];cost=[read(f'gradient/runs/cost_L{l}_h45_tol1e-14.json') for l in range(3)]
for key,label,color,mark in [('gradient','Forward + gradient',blue,'o'),('fd24','Base + 24 perturbed forwards',orange,'s')]:
 a.plot(range(3),[c['median'][key]/c['median']['forward']+(1 if key=='fd24' else 0) for c in cost],color=color,marker=mark,label=label)
a.set(xticks=range(3),xlabel='Mesh level',ylabel='Wall time / forward wall time',title='(d) Derivative evaluation cost',ylim=(0,11));a.legend(frameon=False,fontsize=8,loc='center right',bbox_to_anchor=(1,.55))
for a in ax.flat:a.grid(alpha=.15,axis='y')
fig.tight_layout(h_pad=1.9,w_pad=1.6,rect=(0,.05,1,1));save(fig,'design-gradients')
lines=[]
for q,label in enumerate(['0','0.5','1']):
 for key,values in [('Reference',R),('Conforming',C[3]),('Harmonic',H[3])]:
  deviation='n/a' if key=='Reference' else f'{100*(values[q].std()/R[q].std()-1):+.1f}'.replace('-','$-$')
  lines.append(f'{label} & {key} & {(0 if abs(values[q].mean())<.5e-5 else values[q].mean()):.5f} & {values[q].std():.5f} & {deviation} '+r'\\')
(V/'tables/coupling.tex').write_text(r'\begin{tabular}{llrrr}\toprule'+'\n'+r'Current fraction & Model & Mean & Std & Std vs ref. (\%)\\\midrule'+'\n'+'\n'.join(lines)+'\n'+r'\bottomrule\end{tabular}'+'\n')
lines=[]
for l,c in enumerate(cost):
 m=c['median'];lines.append(f'L{l} & {m["forward"]:.3f} & {m["gradient"]:.3f} & {(m["fd24"]+m["forward"]):.3f} & {c["shape_map_setup_seconds"]:.3f} '+r'\\')
(V/'tables/gradient-cost.tex').write_text(r'\begin{tabular}{lrrrr}\toprule'+'\n'+r'Mesh & Forward & Forward + gradient & Base + FD & Setup\\\midrule'+'\n'+'\n'.join(lines)+'\n'+r'\bottomrule\end{tabular}'+'\n')
facts=json.loads((V/'verified-numbers.json').read_text());facts.update({'harmonic_ripple_relative_error_percent':(100*(H[3].std(axis=1)/R.std(axis=1)-1)).tolist(),'harmonic_mean_relative_error_percent':(100*(H[3,1:].mean(axis=1)/R[1:].mean(axis=1)-1)).tolist(),'max_pointwise_harmonic_gap':np.max(abs(H[3]-R),axis=1).tolist(),'loaded_fd_cases':len(errors),'loaded_fd_below_threshold':sum(e<=2.8e-7 for e in errors),'loaded_fd_worst':max(errors),'SW1_mean_fixed_I':g['d_mean'][3],'SW1_mean_fixed_density':g['d_mean_jinit'][3]});(V/'verified-numbers.json').write_text(json.dumps(facts,indent=2)+'\n')
print('PASS: v6 coupling and gradient figures, 704 distinct raw FD cases, comparison/cost tables regenerated.')
# Parameter definitions and design points for readers of the derivative plots.
names=['MAG','MH','MW','SW1','SW2','SW3','SW4','SD1','Sr1','LENGTH','OPERATING_ANGLE','ScaleR']
meanings=['Pole-axis iron depth above magnet','Radial magnet height','Magnet width','Stator tooth width','Slot-opening width','Slot-opening depth','Back-iron depth on slot axis','Stator outer diameter','Slot-bottom fillet radius','Axial length','Electrical current offset','Radial similarity scale']
opt=read('validation/runs/opt120_export_L3_h45_fixed.json')['design']['scalars']
a=read('gradient/runs/study_L1_h45_initial.json')['design_point'];b=read('gradient/runs/study_L1_h45_perturbed.json')['design_point'];lines=[]
for k,(x,y,meaning) in enumerate(zip(a,b,meanings)):
 assert x['name']==y['name']==names[k]
 factor=1000 if k<10 else 1;unit='mm' if k<10 else (r'$^\circ$' if k==10 else '1')
 label=names[k].replace('_',r'\_')
 if k==10:label=r'$\phi_0$'
 lines.append(f'{label} & {meaning} & {unit} & {factor*x["value"]:.4f} & {factor*y["value"]:.4f} & {factor*opt[names[k]]:.4f} '.replace(' -',' $-$')+r'\\')
(V/'tables/design-points.tex').write_text(r'\begin{tabular}{l>{\raggedright\arraybackslash}p{.28\linewidth}lrrr}\toprule'+'\n'+r'Parameter & Meaning & Unit & Initial & Perturbed & Opt120\\\midrule'+'\n'+'\n'.join(lines)+'\n'+r'\bottomrule\end{tabular}'+'\n')

# Model--estimator comparison; widths are read from the pinned reference.
names=['MAG','MH','MW','SW1','SW2','SW3','SW4','SD1','Sr1']
bounds={p['name']:p for p in read('reference_data.json')['optimization_parameter_bounds']}
width=np.array([bounds[n]['maximum']-bounds[n]['minimum'] for n in names])
fig,axes=plt.subplots(2,2,figsize=(6.5,4.8))
comparison={}
for row,design in enumerate(['initial','perturbed']):
 comparison[design]={}
 for model,color,offset,label in [('h45',green,-.18,'Harmonic (order 45) / coupling torque'),('conforming',blue,.18,'Conforming / Arkkio')]:
  g=read(f'gradient/runs/study_L2_{model}_{design}.json')['currents']['1']['gradient']
  comparison[design][model]={}
  for col,key in enumerate(['d_mean','d_std']):
   vals=np.array(g[key][:9])*width*1000
   comparison[design][model][key]=vals.tolist()
   a=axes[row,col];a.plot(vals,np.arange(9)+offset/2,ls='none',marker='o' if model=='h45' else 's',ms=4,color=color,label=label)
 for col,objective in enumerate(['Mean torque','Ripple']):
  a=axes[row,col];a.axvline(0,color='#555',lw=.6);a.grid(axis='x',alpha=.15);a.set_xscale('symlog',linthresh=1)
  for k in range(9):a.plot([comparison[design]['h45'][['d_mean','d_std'][col]][k],comparison[design]['conforming'][['d_mean','d_std'][col]][k]],[k-.09,k+.09],color='#888',lw=.6,zorder=0)
  a.set_ylim(8.7,-.7)
  ticks=[-1000,-10,0,10,1000] if col==0 else [-1,0,1,10,100]
  a.set_xticks(ticks,[str(x).replace('-','−') for x in ticks]);a.tick_params(axis='x',labelsize=8)
  a.set_title(f"({chr(97+2*row+col)}) {design.capitalize()}: {objective}")
  a.set_xlabel('Box-scaled derivative (mN m)',fontsize=8)
  a.set_yticks(range(9),names,fontsize=8)
fig.legend(*axes[0,0].get_legend_handles_labels(),loc='upper center',ncol=1,frameon=False,fontsize=8,bbox_to_anchor=(.5,1.01))
fig.tight_layout(rect=(0,0,1,.91),h_pad=1.5,w_pad=2);save(fig,'model-gradients')
facts=json.loads((V/'verified-numbers.json').read_text());facts['model_gradient_box_scaled_mNm']=comparison;facts['geometric_box_widths_m']=width.tolist()
(V/'verified-numbers.json').write_text(json.dumps(facts,indent=2)+'\n')

# Refinement of the independent reference and the saved optimized geometry.
from refinement_data import facts as refinement_facts
new = refinement_facts()
fig, ax = plt.subplots(2, 2, figsize=(6.5, 5.1))
ns = [3, 6, 12, 24]
a = ax[0, 0]
a.plot(range(4), [1000*new['reference'][str(n)][0]['std'] for n in ns], color=green, marker='o', label='IGA reference')
a.axhline(1000*H[3,0].std(), color=blue, ls='--', label='GEODE L3')
cogging = 1000*np.array([H[l,0].std() for l in (1,2,3)])
band = max(abs(np.diff(cogging)))
a.axhspan(cogging[-1]-band,cogging[-1]+band,color=blue,alpha=.12,label='L3 refinement interval')
a.set(xticks=range(4), xticklabels=['3×3','6×6','12×12','24×24'], xlabel='Subdivisions per IGA patch', ylabel='Cogging std (mN m)', title='(a) Reference refinement')
a.legend(frameon=False, fontsize=7, loc='lower right')
a = ax[0, 1]
for q, color, label in [(1,green,'Half current'),(2,orange,'Full current')]:
    base = new['reference']['24'][q]['std']
    a.plot(range(4), [100*(new['reference'][str(n)][q]['std']/base-1) for n in ns], marker='o', color=color, label=label)
    a.axhline(100*(H[3,q].std()/base-1), color=color, ls='--', lw=1)
a.axhline(0, color='#777', lw=.6)
a.set(xticks=range(4), xticklabels=['3×3','6×6','12×12','24×24'], xlabel='Subdivisions per IGA patch', ylabel='Std / finest reference − 1 (%)', title='(b) Loaded ripple')
a.legend(frameon=False, fontsize=8, loc='lower right')
a = ax[1, 0]
for design, color, label in [('initial_nurbs',blue,'Initial'),('opt120_export',orange,'Opt120')]:
    a.semilogy(range(3), [new['candidate'][m][design]['F'] for m in ['h45','h93','conforming']], marker='o', linestyle='none', color=color, label=label)
a.set(xticks=range(3), xticklabels=['Order 45','Order 93','Conforming'], ylabel='Squared-ripple sum F (N² m²)', title='(c) Interface effect at Opt120')
a.tick_params(axis='x', labelsize=8)
a.annotate('×3.6', xy=(1,new['candidate']['h93']['opt120_export']['F']), xytext=(.6,1.8e-5), ha='center', fontsize=8, arrowprops={'arrowstyle':'->','lw':.6})
a.legend(frameon=False, fontsize=8, loc='center right')
a = ax[1, 1]
vals = [new['current_means'][k] for k in ['fixed','fixed-physical','jinit']]
a.bar(range(3), vals, color=[blue,green,orange], width=.6)
base = new['candidate']['h45']['initial_nurbs']['mean_full']
a.axhline(base, color='#444', ls='--', lw=1)
for i, val in enumerate(vals):a.text(i,val+.018,f'{val:.4f}',ha='center',fontsize=8)
a.text(.02,.96,'Dashed: initial (100 mm)',transform=a.transAxes,fontsize=7,va='top')
a.set(xticks=range(3),xticklabels=['Fixed\nunscaled I','Fixed\nphysical I','Fixed group-1\ndensity'],ylabel='Full-current mean (N m)',title='(d) Opt120 at saved size',ylim=(0,1.75))
a.tick_params(axis='x',labelsize=8)
for a in ax.flat:a.grid(axis='y',alpha=.15)
fig.tight_layout(h_pad=2.0,w_pad=1.5);save(fig,'refined-validation')
facts=json.loads((V/'verified-numbers.json').read_text());facts['refined_validation']=new
(V/'verified-numbers.json').write_text(json.dumps(facts,indent=2)+'\n')

# Supplement: waveforms retain their different means; a second panel isolates ripple.
fig, axes = plt.subplots(1, 2, figsize=(6.5, 2.8))
for model, color, label in [('h45',green,'Order 45'), ('h93',orange,'Order 93'), ('conforming',blue,'Conforming')]:
    run = read(f'validation/runs/opt120_export_L3_{model}_fixed.json')
    rows = sorted(next(c['rows'] for c in run['currents'] if c['fraction']==1), key=lambda r:r['angle_deg'])
    key = 'T_arkkio_outer' if model == 'conforming' else 'T_coupling'
    x = np.array([r['angle_deg'] for r in rows]); y = np.array([r[key] for r in rows])
    axes[0].plot(x,y,color=color,label=label)
    axes[1].plot(x,1000*(y-y.mean()),color=color,label=label)
for a in axes:
    a.set_xlabel('Mechanical angle (deg)');a.grid(alpha=.15)
axes[0].set_ylabel('Sector torque (N m)');axes[1].set_ylabel('Torque − its 40-angle mean (mN m)')
axes[0].set_title('(a) Full-current Opt120');axes[1].set_title('(b) Centered waveforms')
axes[0].legend(frameon=False,fontsize=8)
fig.tight_layout();save(fig,'opt120-waveforms')
