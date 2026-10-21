"""Read-only check of pinned evidence, figures and reported numbers; no FEM run."""
from pathlib import Path
import hashlib,json,math,re,statistics as st
V=Path(__file__).resolve().parent;D=V/'figures/src/data'
def load(p):return json.loads((D/p).read_text())
def close(a,b):assert math.isclose(a,b,rel_tol=1e-11,abs_tol=2e-14),(a,b)
def waveform(run,q,key='T_arkkio'):
 rows=sorted((r for r in run['rows'] if r['current_index']==q),key=lambda r:r['angle_deg'])
 assert [r['angle_deg'] for r in rows]==list(range(20))
 return [r[key] for r in rows]
def diff(a,b):return max(abs(x-y) for x,y in zip(a,b))
def displayed_mean(x):return f'{0 if abs(x)<.5e-5 else x:.5f}'
def assert_table(path,*values):
 text=(V/path).read_text().replace('$-$','-')
 for value in values:assert value in text,(path,value)
manifest=json.loads((V/'evidence-manifest.json').read_text())
for item in manifest['files']:
 assert len(item['source_commit'])==40
 assert hashlib.sha256((V/item['snapshot']).read_bytes()).hexdigest()==item['sha256'],item
R=load('reference_data.json')['designs']['initial']['torque_Nm_for_modeled_sector_by_current_then_angle'];S=load('summary.json');HS=load('harmonic/summary.json');facts=json.loads((V/'verified-numbers.json').read_text())
C=[load(f'L{l}_table_default_mesh.json') for l in range(4)];H=[load(f'harmonic/runs/L{l}_h45.json') for l in range(4)]
for l in range(4):
 for q in range(3):
  c=waveform(C[l],q);h=waveform(H[l],q)
  for x,y in [(st.mean(c),S['comparison_20_angles'][f'I{q}_L{l}']['mean']),(st.pstdev(c),S['comparison_20_angles'][f'I{q}_L{l}']['std'])]:close(x,y)
  row=next(r for r in HS['main_h45'] if r['level']==l and r['current']==q/2)
  close(st.mean(h),row['h45_arkkio'][0]);close(st.pstdev(h),row['h45_arkkio'][1]);close(st.pstdev(R[q]),row['reference'][1])
  if l==3:
   close(100*(st.pstdev(h)/st.pstdev(R[q])-1),facts['harmonic_ripple_relative_error_percent'][q]);close(diff(h,R[q]),facts['max_pointwise_harmonic_gap'][q])
   assert_table('tables/coupling.tex',displayed_mean(st.mean(h)),f'{st.pstdev(h):.5f}',displayed_mean(st.mean(c)),f'{st.pstdev(c):.5f}',displayed_mean(st.mean(R[q])),f'{st.pstdev(R[q]):.5f}')
   if q:close(100*(st.mean(h)/st.mean(R[q])-1),facts['harmonic_mean_relative_error_percent'][q-1])
 # Legacy forward cost table remains reproducible as supplementary output.
 assert_table('tables/cost.tex',f'{st.median(r["wall_s"] for r in C[l]["rows"]):.2f}',f'{sum(r["wall_s"] for r in C[l]["rows"]):.1f}')
oldruns=[json.loads(p.read_text()) for p in D.glob('L*.json')];newruns=[json.loads(p.read_text()) for p in (D/'harmonic/runs').glob('*.json')]
assert sum(len(r['rows']) for r in oldruns)==500
assert sum(len(r['rows']) for r in newruns)==1140
assert all(row['status']=='tolerance' for run in oldruns+newruns for row in run['rows'])
# Recompute sensitivity table from paired raw rows.
sens={}
for kind,limit in [('clamp426',2.1e-6),('tight',7.3e-8),('exact',3.9e-4)]:
 val=0
 for run in oldruns:
  name=next((p.name for p in D.glob('L*.json') if json.loads(p.read_text())==run), '')
  if kind not in name:continue
  base={(r['current_index'],r['angle_deg']):r['T_arkkio'] for r in C[run['level']]['rows']}
  val=max(val,max(abs(r['T_arkkio']-base[(r['current_index'],r['angle_deg'])]) for r in run['rows']))
 assert val<=limit and val>limit*.9,(kind,val,limit)
 sens[kind]=val
half=max(abs(r[k]-r['T_arkkio']) for r in C[3]['rows'] for k in ['T_arkkio_inner','T_arkkio_outer']);assert 7.25e-5<half<7.35e-5
# All harmonic estimators, free-end and full-order comparisons claimed in text.
assert max(abs(r[k]-r['T_arkkio']) for r in H[3]['rows'] for k in ['T_coupling','T_arkkio_inner','T_arkkio_outer'])<9e-5
free=load('harmonic/runs/L3_h45f.json');assert max(diff(waveform(free,q),waveform(H[3],q)) for q in range(3))<5.4e-5
for l,lim in [(0,4.1e-9),(1,1.3e-9)]:
 hall=load(f'harmonic/runs/L{l}_hall.json');assert max(diff(waveform(hall,q),waveform(C[l],q)) for q in range(3))<lim
for l in [1,2]:
 ladder=load(f'harmonic/runs/L{l}_h189.json');assert max(diff(waveform(ladder,q),waveform(C[l],q)) for q in range(3))<1.1e-4
# Check all FD sweeps from plus/minus objective values, not the saved summary.
errors=[];zero_errors=[[],[]];duplicates=0;windows=[]
for p in sorted((D/'gradient/runs').glob('study*.json')):
 study=json.loads(p.read_text())
 for cur,c in study['currents'].items():
  for rule in ['prescribed','jinit']:
   for sweep in c.get('sweep_'+rule,[]):
    duplicate=rule=='jinit' and sweep['parameter'] in ['MAG','MH','MW']
    if duplicate:assert sweep==next(x for x in c['sweep_prescribed'] if x['parameter']==sweep['parameter'])
    if duplicate and float(cur)>0:duplicates+=2
    for o,a in enumerate(sweep['adjoint']):
     err=[]
     for r in sweep['rows']:
      fd=(r['plus'][o]-r['minus'][o])/(2*r['h']);close(fd,r['fd'][o])
      rem=abs(r['plus'][o]-sweep['j0'][o]-r['h']*a);close(rem,r['remainder'][o])
      if a:err.append(abs(fd-a)/abs(a))
     if not err:continue
     best=min(err);close(best,sweep['best'][o]['rel_err'])
     if float(cur)==0:zero_errors[o].append(best)
     elif not duplicate:errors.append(best)
     if float(cur)>0 and not duplicate and sweep['parameter'] not in ['LENGTH','ScaleR']:
      rows=sweep['rows'];hs=[r['h'] for r in rows]
      href=study['h_ref']*(1000 if sweep['parameter']=='OPERATING_ANGLE' else 1)
      i=min(range(len(hs)),key=lambda j:abs(math.log(hs[j]/href)))
      scaled=[abs(r['plus'][o]-sweep['j0'][o]-r['h']*a)/r['h']**2 for r in rows]
      assert scaled[i]>0
      lo=hi=i
      while lo>0 and .5<=scaled[lo-1]/scaled[i]<=2:lo-=1
      while hi+1<len(rows) and .5<=scaled[hi+1]/scaled[i]<=2:hi+=1
      decades=math.log10(max(hs[lo],hs[hi])/min(hs[lo],hs[hi]))
      close(decades,sweep['taylor_window'][o]['decades']);windows.append(decades)
assert len(errors)==facts['loaded_fd_cases']==704
assert sum(e<=2.8e-7 for e in errors)==facts['loaded_fd_below_threshold']==688
assert duplicates==96 and len(windows)==592
assert sum(x>=4 for x in windows)==530 and sum(x>=5 for x in windows)==388
close(max(errors),facts['loaded_fd_worst']);assert max(errors)<1.07e-5
assert max(zero_errors[0])<7.1e-4 and max(zero_errors[1])<8.9e-7
sample=load('gradient/runs/study_L1_h45_initial.json')['currents']['1']['gradient']
close(sample['d_mean'][3],facts['SW1_mean_fixed_I']);close(sample['d_mean_jinit'][3],facts['SW1_mean_fixed_density'])
ratios=[]
for p in (D/'gradient/runs').glob('cost*.json'):
 run=json.loads(p.read_text());m=run['median']
 for k,source in [('forward','forward_seconds'),('gradient','gradient_seconds'),('fd24','fd24_seconds')]:close(st.median(run[source]),m[k])
 ratios.append((m['gradient']/m['forward'],(m['fd24']+m['forward'])/m['forward']))
 if run['mode']=='h45' and run['rel_tol']==1e-14:
  assert_table('tables/gradient-cost.tex',*[f'{m[k]:.3f}' for k in ['forward','gradient']],f'{m["forward"]+m["fd24"]:.3f}',f'{run["shape_map_setup_seconds"]:.3f}')
assert 1.12<min(r[0] for r in ratios) and max(r[0] for r in ratios)<1.20
assert 8.2<min(r[1] for r in ratios) and max(r[1] for r in ratios)<10.54
for design in ['initial','perturbed']:
 points=load(f'gradient/runs/study_L1_h45_{design}.json')['design_point']
 for k,point in enumerate(points):assert_table('tables/design-points.tex',f'{point["value"]*(1000 if k<10 else 1):.4f}')
# Independent reconstruction of the remesh fit and its single-step scatter.
remesh=load('gradient/runs/remesh_L2_h45.json')['currents']['1']['parameters']
gaps={}
for item in remesh:
 rows=[r for r in item['rows'] if r['moved_fd']]
 for o in range(2):
  a=abs(item['adjoint_fresh'][o])
  gap=sum((r['remeshed_fd'][o]-r['moved_fd'][o])*r['h']**2 for r in rows)/sum(r['h']**2 for r in rows)/a
  gaps[(item['parameter'],o)]=gap
  if item['parameter']=='MW' and o==0:
   scatter=max(abs(r['remeshed_fd'][o]-r['moved_fd'][o])/a for r in rows if r['h']>=1e-4)
   assert 6e-4<scatter<6.4e-4 and 2.55e-6<gap<2.65e-6
assert max(gaps,key=lambda k:abs(gaps[k]))==('SW1',1)
assert round(100*abs(gaps[('SW1',1)]),1)==3.8
tex=(V/'main.tex').read_text()+(V/'optimization.tex').read_text();bib=(V/'refs.bib').read_text();keys=set(re.findall(r'@\w+\{([^,]+),',bib))
# Regression guards for the audited appendix number and new manuscript claims.
fit=load('slotless_screen.json')['harmonic_fits']['through_harmonic_1']
for field in ['heldout_rmse_over_analytic_amplitude','heldout_max_abs_error_over_analytic_amplitude']:
 mantissa,exponent=f'{fit[field]:.2e}'.split('e')
 assert mantissa+r'\times10^{'+str(int(exponent))+'}' in tex
assert r'6.83\times10^{-10}' not in tex
for claim in ['704','688','530 of 592','388 span','8.2--10.5']:assert claim in tex,claim
assert '-0.00000' not in (V/'tables/coupling.tex').read_text()
bounds={p['name']:p for p in load('reference_data.json')['optimization_parameter_bounds']}
parameters=load('gradient/runs/study_L2_h45_initial.json')['design_point'][:9]
widths=[bounds[p['name']]['maximum']-bounds[p['name']]['minimum'] for p in parameters]
for x,y in zip(widths,facts['geometric_box_widths_m']):close(x,y)
for design in ['initial','perturbed']:
 for model in ['h45','conforming']:
  g=load(f'gradient/runs/study_L2_{model}_{design}.json')['currents']['1']['gradient']
  for key in ['d_mean','d_std']:
   expected=[1000*w*x for w,x in zip(widths,g[key])]
   for a,b in zip(expected,facts['model_gradient_box_scaled_mNm'][design][model][key]):close(a,b)
  precision=2
  assert f"{g['d_std'][0]:.{precision}f}" in tex
for design,expected in [('initial',[.58,6.7]),('perturbed',[.66,10.2])]:
 models=facts['model_gradient_box_scaled_mNm'][design]
 for key,value,places in zip(['d_mean','d_std'],expected,[2,1]):
  a=models['h45'][key];b=models['conforming'][key]
  norm=100*math.sqrt(sum((x-y)**2 for x,y in zip(a,b))/sum(y*y for y in b))
  assert round(norm,places)==value
  assert f'{value:.{places}f}'+r'\%' in tex
changed=[k for k in list(range(9))+[11] if sample['d_mean'][k]!=sample['d_mean_jinit'][k]]
assert len(changed)==7
assert [k for k in changed if sample['d_mean'][k]*sample['d_mean_jinit'][k]<0]==[3,6,7]
for group in re.findall(r'\\cite(?:p|t|yearpar)?\{([^}]+)\}',tex):assert set(group.split(','))<=keys,group
for asset in re.findall(r'\\includegraphics(?:\[[^]]*\])?\{([^}]+)\}',tex):assert (V/asset).is_file(),asset
for asset in ['main.pdf','main.bbl']:assert (V/asset).is_file()
print(f'PASS: {len(manifest["files"])} evidence hashes; 500 + 1140 forward solve statuses; raw means/ripple, coupling/estimator controls, sensitivity and cost tables; 704 distinct loaded FD cases (96 duplicate records identified), 592 recomputed Taylor windows, cross-model sensitivities, slotless rounding and zero-current sweeps; citations/figure assets.')
print('Recorded-artifact verification only; not a new FEM run or independent scientific review.')

# Independently reconstruct reference and candidate claims from raw torques (not the renderer helpers).
new=facts['refined_validation']; rr={}
for p in (D/'reference_refinement/runs').glob('*.json'):
 match=re.fullmatch(r's(\d+)_c\d+',p.stem)
 if not match:continue
 run=json.loads(p.read_text());indices=run['current_indices'];values=run['torque_Nm']
 if isinstance(indices,int):indices=[indices];values=[values]
 for q,vals in zip(indices,values):
  rr[int(match[1]),q-1]=vals
  expect=new['reference'][match[1]][q-1]
  close(st.mean(vals),expect['mean']);close(st.pstdev(vals),expect['std'])
reproduction=max(diff(rr[3,q],R[q]) for q in range(3));close(reproduction,new['saved_reproduction_max_Nm']);assert reproduction<9.8e-13
for n,rounded in [(3,'0.01071'),(6,'0.01220'),(12,'0.01212'),(24,'0.01208')]:
 assert f'{st.pstdev(rr[n,0]):.5f}'==rounded and rounded in tex
for q in range(3):
 h=waveform(H[3],q);gap=100*(st.pstdev(h)/st.pstdev(rr[24,q])-1);close(gap,new['refined_gap_percent'][q])
 assert f'{gap:.2f}'+r'\%' in tex
 for fun in ([st.mean,st.pstdev] if q else [st.pstdev]):
  vals=[fun(waveform(H[l],q)) for l in [1,2,3]];band=max(abs(vals[1]-vals[0]),abs(vals[2]-vals[1]));assert abs(fun(rr[24,q])-vals[2])<=band
close(diff(waveform(H[3],0),rr[24,0]),new['refined_cogging_max_gap_Nm']);assert round(new['refined_cogging_max_gap_Nm']/1e-5,1)==3.4
for n in [3,6,12]:
 tight=load(f'reference_refinement/runs/s{n}_c1_t14.json')['torque_Nm']
 assert diff(tight,rr[n,0])<4.4e-8

def vstats(design,model,l=3,rule='fixed',step=1):
 run=load(f'validation/runs/{design}_L{l}_{model}_{rule}.json');key='T_coupling' if model.startswith('h') else 'T_arkkio_outer';out={}
 assert run['status']=='ok' and run['solves_failed']==0
 for cur in run['currents']:
  rows={r['angle_deg']:r for r in cur['rows']};selected=[rows[i*step] for i in range(round(20/step))]
  assert all(r['status'] in ['tolerance','roundoff_floor'] for r in selected)
  vals=[r[key] for r in selected];out[cur['fraction']]=(st.mean(vals),st.pstdev(vals))
 return out

def vf(stats):return sum(stats[q][1]**2 for q in [0,.5,1])
for model,ratio in [('h45',7631),('h93',2454),('conforming',2444)]:
 for design in ['initial_nurbs','opt120_export']:
  a=vstats(design,model);expected=new['candidate'][model][design]
  close(vf(a),expected['F']);close(a[1][0],expected['mean_full']);close(a[1][1],expected['std_full'])
  close(100*(vf(a)-vf(vstats(design,model,l=2)))/vf(a),expected['refinement_percent'])
 assert round(vf(vstats('initial_nurbs',model))/vf(vstats('opt120_export',model)))==ratio
 angular=100*(vf(vstats('opt120_export',model,step=2))/vf(vstats('opt120_export',model,step=.5))-1)
 close(angular,new['candidate'][model]['g10_vs_g40_percent'])
 assert f'{abs(angular):.{1 if model=="conforming" else 2}f}'+r'\%' in tex
assert round(vf(vstats('opt120_export','h93'))/vf(vstats('opt120_export','h45')),1)==3.6
assert round(100*(vf(vstats('initial_nurbs','h93'))/vf(vstats('initial_nurbs','h45'))-1))==16
for rule in ['fixed','fixed-physical','jinit']:
 mean=vstats('opt120_export','h45',rule=rule)[1][0];close(mean,new['current_means'][rule]);assert f'{mean:.4f}' in tex
assert load('validation/runs/opt120_moved_L3_h45_fixed.json')['mesh_quality']['inverted']==4
for design in ['initial','perturbed']:
 def vector(l,m,key):
  name=f'{"study" if l==2 else "gradient"}_L{l}_{m}_{design}.json'
  g=load('gradient/runs/'+name)['currents']['1']['gradient'][key]
  return [w*x for w,x in zip(widths,g)]
 def normgap(a,b):return 100*math.sqrt(sum((x-y)**2 for x,y in zip(a,b))/sum(y*y for y in b))
 for key in ['d_mean','d_std']:
  gap=normgap(vector(3,'h45',key),vector(3,'conforming',key));assert f'{gap:.2f}'+r'\%' in tex
  for m in ['h45','conforming']:
   change=normgap(vector(2,m,key),vector(3,m,key))
   if key=='d_std':assert .46<=change<=1.20
   elif m=='conforming':assert .51<=change<=.55
print('PASS: reference reruns, 60-sample reproduction, refined-model gaps, candidate objectives/current rules/angular aliasing, invalid moved mesh and L3 gradient comparisons independently reconstructed.')

# V5 review corrections, independently reconstructed from the pinned records.
opt=load('validation/runs/opt120_export_L3_h45_fixed.json');scalars=opt['design']['scalars']
close(opt['torque_factor'],scalars['ScaleR']**2*scalars['LENGTH']/.1)
mean=vstats('opt120_export','h45')[1][0]
assert f"{mean/opt['torque_factor']:.4f}" in tex
assert f"{mean*.1/scalars['LENGTH']:.4f}" in tex
for k,p in enumerate(load('gradient/runs/study_L1_h45_initial.json')['design_point']):
 assert_table('tables/design-points.tex',f"{scalars[p['name']]*(1000 if k<10 else 1):.4f}")
expected_increments=[3.05e-8,2.90e-8]
objectives=[vf(vstats('opt120_export','h45',l=l)) for l in (1,2,3)]
for inc,expected in zip([b-a for a,b in zip(objectives,objectives[1:])],expected_increments):
 assert math.isclose(inc,expected,rel_tol=.002)
interface=vf(vstats('opt120_export','h93'))-objectives[-1]
assert round(interface/(objectives[-1]-objectives[-2]))==96
assert math.isclose(interface,2.79e-6,rel_tol=.002)
construction=[vf(vstats('opt120_construction','h45',l=l)) for l in (2,3)]
assert round(100*(construction[1]-construction[0])/construction[1],2)==3.22
saved=load('reference_data.json')['designs']['optimized']['torque_Nm_for_modeled_sector_by_current_then_angle']
matched=vstats('opt120_export','h45',rule='reference');matched[0]=vstats('opt120_export','h45')[0]
saved_f=sum(st.pvariance(a) for a in saved)
assert round(sum(st.pvariance(a) for a in R)/saved_f)==11982
assert round(vf(vstats('initial_nurbs','h45'))/vf(matched))==10170
assert round(matched[1][1]/st.pstdev(saved[2]),1)==2.0
assert round(matched[1][0],4)==1.4986 and round(st.mean(saved[2]),4)==1.5000
reference_run=load('validation/runs/opt120_export_L3_h45_reference.json')
max_gap=0
for c in reference_run['currents']:
 if c['fraction'] not in (.5,1):continue
 rows=sorted((r for r in c['rows'] if r['angle_deg']%1==0),key=lambda r:r['angle_deg'])
 max_gap=max(max_gap,diff([r['T_coupling'] for r in rows],saved[round(c['fraction']*2)]))
assert math.isclose(max_gap,2.1e-3,rel_tol=.03)
rows=[r for c in opt['currents'] if c['fraction']==1 for r in c['rows'] if r['angle_deg']%1==0]
paired=[abs(fun([r['T_coupling'] for r in rows])-fun([r['T_arkkio_outer'] for r in rows])) for fun in (st.mean,st.pstdev)]
assert math.isclose(paired[0],1.3e-4,rel_tol=.02) and math.isclose(paired[1],1.1e-5,rel_tol=.02)
for design in ('initial','perturbed'):
 for model in ('h45','conforming'):
  g=load(f'gradient/runs/gradient_L3_{model}_{design}.json')['currents']['1']['gradient']
  assert f"{g['d_std'][0]:.2f}" in tex
h93zero=vstats('opt120_export','h93')[0][1]
assert f'{h93zero:.5f}' in tex
# Every successful candidate record used in the expanded v5 comparison.
files=[p for p in (D/'validation/runs').glob('*.json') if 'moved' not in p.name]
assert len(files)==18
rows=[r for p in files for c in json.loads(p.read_text())['currents'] for r in c['rows']]
assert len(rows)==3240 and all(r['status']=='tolerance' for r in rows)
print('PASS: v5 size/current normalization, matched-policy reference ratios, candidate refinement increments, paired estimators, L3 MAG components, and 3240 candidate solve statuses.')


# V6: raw-record regression coverage for both v5 audit criticals.
h45=load('gradient/runs/study_L2_h45_initial.json')['currents']['1']['gradient']['d_mean']
conf=load('gradient/runs/study_L2_conforming_initial.json')['currents']['1']['gradient']['d_mean']
component_gaps=[100*abs(h45[i]-conf[i])/abs(conf[i]) for i in (4,5)]
assert [round(x,1) for x in component_gaps]==[6.5,16.5]
assert 'by '+r'6.5\% and 16.5\% relative to conforming values.' in tex
for level in (0,1):
 sweep=next(s for s in load(f'gradient/runs/study_L{level}_conforming_initial.json')['currents']['1']['sweep_jinit'] if s['parameter']=='SW1')
 signed={r['h']:(r['plus'][1]-sweep['j0'][1]-r['h']*sweep['adjoint'][1])/r['h']**2 for r in sweep['rows']}
 anchor=signed[1e-5]
 assert sweep['taylor_window'][1]['decades']==0
 if level==0:assert anchor<0<signed[3e-6]
 else:
  assert all(signed[h]<0 for h in (3e-5,1e-5,3e-6,1e-6))
  assert signed[3e-5]/anchor>2 and signed[3e-6]/anchor<.5
  assert round(signed[3e-5]/anchor,2)==2.64 and round(signed[3e-6]/anchor,3)==.459
  assert all(v<0 for h,v in signed.items() if 3e-8<=h<=1e-5)
 assert 'at L0 the\nremainder changes sign near the anchor, whereas at L1 remainder/$h^2$ is\nstill drifting there.' in tex
norm=lambda a:math.sqrt(sum(x*x for x in a))
a=[w*x for w,x in zip(widths,load('gradient/runs/study_L2_h45_perturbed.json')['currents']['1']['gradient']['d_std'])]
b=[w*x for w,x in zip(widths,load('gradient/runs/gradient_L3_h45_perturbed.json')['currents']['1']['gradient']['d_std'])]
assert round(100*norm([x-y for x,y in zip(a,b)])/norm(a),2)==1.19
assert round(100*(norm(b)/norm(a)-1),2)==-.19
assert r'box-scaled vector changes by only 1.19\% of its L2 norm' in tex
assert r'norm itself decreases by 0.19\%' in tex
initial_mean=vstats('initial_nurbs','h45')[1][0]
for rule,expected,percent in [('fixed',1.4971,3.7),('fixed-physical',1.4291,-1.0),('jinit',1.7061,18.2)]:
 normalized=vstats('opt120_export','h45',rule=rule)[1][0]/opt['torque_factor']
 assert round(normalized,4)==expected and round(100*(normalized/initial_mean-1),1)==percent
 assert f'{expected:.4f}'+r'\,N\,m' in tex
length_only=mean*.1/scalars['LENGTH']
assert round(100*(length_only/initial_mean-1),1)==13.9
assert round(scalars['ScaleR']**2,3)==1.098
assert abs(100*(matched[1][0]/st.mean(saved[2])-1))<.1
assert 'figures/opt120-waveforms.pdf' in tex
print('PASS: v6 SW2/SW3 component rounding, distinct L0/L1 Taylor failure mechanisms, vector versus norm changes, all three size-normalized current policies, and matched full-current abstract claim.')
import runpy
runpy.run_path(str(V/'verify_exp2.py'),run_name='__main__')
runpy.run_path(str(V/'verify_exp2b.py'),run_name='__main__')
runpy.run_path(str(V/'verify_exp2c.py'),run_name='__main__')
