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
def assert_table(path,*values):
 text=(V/path).read_text()
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
   assert_table('tables/coupling.tex',f'{st.mean(h):.5f}',f'{st.pstdev(h):.5f}',f'{st.mean(c):.5f}',f'{st.pstdev(c):.5f}',f'{st.mean(R[q]):.5f}',f'{st.pstdev(R[q]):.5f}')
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
errors=[];zero_errors=[[],[]]
for p in sorted((D/'gradient/runs').glob('study*.json')):
 study=json.loads(p.read_text())
 for cur,c in study['currents'].items():
  for rule in ['prescribed','jinit']:
   for sweep in c.get('sweep_'+rule,[]):
    for o,a in enumerate(sweep['adjoint']):
     err=[]
     for r in sweep['rows']:
      fd=(r['plus'][o]-r['minus'][o])/(2*r['h']);close(fd,r['fd'][o])
      rem=abs(r['plus'][o]-sweep['j0'][o]-r['h']*a);close(rem,r['remainder'][o])
      if a:err.append(abs(fd-a)/abs(a))
     if not err:continue
     best=min(err);close(best,sweep['best'][o]['rel_err'])
     (errors if float(cur)>0 else zero_errors[o]).append(best)
assert len(errors)==facts['loaded_fd_cases']==800
assert sum(e<=2.8e-7 for e in errors)==facts['loaded_fd_below_threshold']==782
close(max(errors),facts['loaded_fd_worst']);assert max(errors)<1.07e-5
assert max(zero_errors[0])<7.1e-4 and max(zero_errors[1])<8.9e-7
sample=load('gradient/runs/study_L1_h45_initial.json')['currents']['1']['gradient']
close(sample['d_mean'][3],facts['SW1_mean_fixed_I']);close(sample['d_mean_jinit'][3],facts['SW1_mean_fixed_density'])
ratios=[]
for p in (D/'gradient/runs').glob('cost*.json'):
 run=json.loads(p.read_text());m=run['median']
 for k,source in [('forward','forward_seconds'),('gradient','gradient_seconds'),('fd24','fd24_seconds')]:close(st.median(run[source]),m[k])
 ratios.append((m['gradient']/m['forward'],m['fd24']/m['forward']))
 if run['mode']=='h45' and run['rel_tol']==1e-14:
  assert_table('tables/gradient-cost.tex',*[f'{m[k]:.3f}' for k in ['forward','gradient','fd24']],f'{run["shape_map_setup_seconds"]:.3f}')
assert 1.12<min(r[0] for r in ratios) and max(r[0] for r in ratios)<1.20
assert 7.2<min(r[1] for r in ratios) and max(r[1] for r in ratios)<9.54
for design in ['initial','perturbed']:
 points=load(f'gradient/runs/study_L1_h45_{design}.json')['design_point']
 for k,point in enumerate(points):assert_table('tables/design-points.tex',f'{point["value"]*(1000 if k<10 else 1):.4f}')
tex=(V/'main.tex').read_text();bib=(V/'refs.bib').read_text();keys=set(re.findall(r'@\w+\{([^,]+),',bib))
for group in re.findall(r'\\cite(?:p|t|yearpar)?\{([^}]+)\}',tex):assert set(group.split(','))<=keys,group
for asset in re.findall(r'\\includegraphics(?:\[[^]]*\])?\{([^}]+)\}',tex):assert (V/asset).is_file(),asset
for asset in ['main.pdf','main.bbl']:assert (V/asset).is_file()
print(f'PASS: {len(manifest["files"])} evidence hashes; 500 + 1140 forward solve statuses; raw means/ripple, coupling/estimator controls, sensitivity and cost tables; all 800 loaded FD cases and zero-current sweeps; citations/figure assets.')
print('Recorded-artifact verification only; not a new FEM run or independent scientific review.')
