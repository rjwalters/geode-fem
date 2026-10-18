"""Closed capture -> scientific validation audit -> frozen final-check cohort.

Only `load` establishes the complete input chain. No solver is launched, no
refinement outcome influences selection, and all original attempt rows remain.
"""
import copy
import importlib
import argparse
import hashlib
import json
import sys
from pathlib import Path

if __name__=='__main__':sys.path.insert(0,str(Path(__file__).resolve().parent))

import capture_archive
import check_plan
import producer_source
import setup_evidence
from design_evidence import bits, _bound_json
from selection import select, _exact
from trace_archive import _queue
from validation_evidence import adapt_validation, number, same


def equal(a,b,label):
    if _exact(a)!=_exact(b):raise ValueError('changed '+label)


def _ledger(trace, session, model, ledger_type):
    """Replay the reviewed ledger, including validation-time prefixes."""
    raw=trace['ledger']
    unit=session['unit_seconds'];budget=session['budget_fe']
    ledger=ledger_type(unit,budget,weights=model['weights'],clock=lambda:0.)
    prefixes={0.:{0.}}
    for i,row in enumerate(raw['entries']):
        if type(row.get('seq')) is not int or row['seq']!=i:raise ValueError('reordered ledger')
        for k in ('forward_solves','adjoint_solves'):
            if type(row.get(k)) is not int or row[k]<0:raise ValueError('invalid ledger solve count')
        ledger.charge(row['category'],number(row['seconds'],'ledger charge',minimum=0),
                      row['forward_solves'],row['adjoint_solves'],row['note'])
        prefixes.setdefault(ledger.charged_seconds(),set()).add(ledger.solve_fe())
    elapsed=number(raw['elapsed_seconds'],'elapsed time',minimum=0)
    ledger.clock=lambda:elapsed
    equal(raw,ledger.to_json(),'run ledger arithmetic/weights')
    same(trace['in_run_wall_seconds'],raw['total_seconds'],'in-run charged seconds')
    previous=0.
    for v in trace.get('validations',[]):
        seconds=number(v['seconds'],'validation time',minimum=0)
        if seconds<previous or seconds not in prefixes or number(v.get('solve_fe'),'validation solve FE',minimum=0) not in prefixes[seconds]:
            raise ValueError('validation stamp is not an ordered ledger prefix')
        previous=seconds
    return {'total_seconds':raw['total_seconds'],'total_fe':raw['total_fe'],
            'solve_fe':raw['solve_fe'],'validation_prefixes':len(trace.get('validations',[]))}


def _mesh_bytes(reader, generation, attempt, record, reference):
    """Bind every produced mesh to bytes in this attempt's validation directory."""
    inventory=reader.manifest['inventory']['files'];wanted=[]
    if record['status']=='ok':
        mesh=record['mesh']
        if mesh['kind']!='fresh':
            # Frozen inputs were authenticated by capture_archive.load.
            return [{'sha256':record['msh_sha256'],'file':reference,'origin':'reference'}]
        wanted.append(record['msh_sha256']);attempts=mesh['attempts']
    else:attempts=record['attempts']
    wanted.extend(a['msh_sha256'] for a in attempts if a.get('msh_sha256'))
    original=Path(generation['original_session_directory'])/Path(attempt['trace_file']).with_suffix('.meshes')/'validation'
    prefix=reader.mapped(str(original)).relative_to(reader.root).as_posix()+'/'
    rows=[]
    for digest in sorted(set(wanted)):
        paths=[p for p,entry in inventory.items() if p.startswith(prefix) and p.endswith('.msh') and entry['sha256']==digest]
        if not paths:raise ValueError('missing generated validation mesh bytes in originating attempt')
        for p in paths:reader.path(p)
        rows.append({'sha256':digest,'files':sorted(paths),'origin':'generated'})
    return rows


def _anchor(trace, setup, frozen):
    actual=trace['anchor'];a=setup['effective_anchor']
    for k,v in a.items():equal(actual.get(k),v,'effective trace anchor '+k)
    equal(actual.get('targets'),{n:r*a['F0'] for n,r in (('T50',.5),('T90',.1),('T99',.01))},'trace targets')
    equal(actual.get('mean_floor'),1.0025*a['mean0_full_current'],'trace mean floor')
    binding=frozen['inputs']['anchor']
    equal(actual.get('source'),binding['path'],'trace anchor path')
    equal(actual.get('file_sha256'),binding['sha256'],'trace anchor file hash')
    original=actual.get('file_values') or {}
    for key,k in (('F0','F0'),('mean0','mean0_full_current')):
        equal(original.get(key),setup['canonical_anchor'][k],'original anchor file values')


def _assemble(capture, setup, reader, *, ledger_type, starts):
    """Internal assembly; caller must supply verified capture/setup, never flags alone."""
    chain=capture['chain'];session=capture['frozen']['session']
    jobs=chain['generations'][0]['jobs'];_queue(jobs)
    policy=chain['policy']['jobs']
    if (chain['all_registered_jobs_resolved'] is not True or [r['job'] for r in policy]!=jobs
            or any(r['selected_completion'] is None for r in policy)):
        raise ValueError('scientific selection requires the complete resolved study')
    selected={r['selected_completion']['key']:r['selected_completion'] for r in policy}
    observed=set();audit=[];records=[];designs={}
    for ref in setup['theta0'].values():designs[ref['design'].key]=ref
    for generation in chain['generations']:
        equal(generation['session'],session,'generation session')
        for attempt in generation['attempts']:
            job=attempt['job'];key=generation['original_session_directory']+'#'+str(attempt['attempt_id'])
            if key in observed:raise ValueError('duplicate attempt identity')
            observed.add(key);choice=selected.get(key);trace=attempt['trace']
            row={'run_id':key,'job':copy.deepcopy(job),'selected_completion':choice is not None,
                 'terminal_bound':attempt['terminal_bound'],'host_contaminated':attempt['host_contaminated'],
                 'trace_file':attempt.get('trace_file'),'trace_sha256':attempt['trace_sha256'],
                 'status':'missing_trace' if trace is None else trace['stop_reason'],'validations':[]}
            audit.append(row)
            if choice is not None:
                if (choice['trace_sha256']!=attempt['trace_sha256'] or choice['trace']!=attempt.get('trace_file')
                        or not attempt['terminal_bound'] or attempt['host_contaminated'] is not False
                        or trace is None or trace.get('optimizer_completed') is not True):
                    raise ValueError('selected completion disagrees with terminal attempt')
            if trace is None:continue
            model=session['models']['L0' if job['method_letter'] in 'DE' else 'L1']
            equal(trace['training_model'],model,'trace training model')
            row['ledger']=_ledger(trace,session,model,ledger_type)
            row['validation_unavailable_events']=[copy.deepcopy(e) for e in trace.get('events',[]) if e.get('kind')=='validation_unavailable']
            validations=trace.get('validations',[])
            if not validations:continue
            _anchor(trace,setup,capture['frozen'])
            equal(bits(trace.get('x_start',[])),bits(starts['starts'][job['start']][job['space']]),'registered run start')
            ready=trace['evaluator_ready'];train=ready['session']['training'];level=model['train_level']
            expected={'degrees':list(range(20)),'fractions':[0.,.5,1.],'i_peak0_A':9.677657374172274,
                'level':level,'mesh_repair':True,'mode':'h45',
                'newton_max_iterations':60,'newton_rel_tol':1e-8,'policy_min_angle_deg':5.,
                'torque':'h45/coupling','warm':'Cold'}
            equal(train,expected,'registered training evaluator settings')
            if ready['session'].get('registered') is not True:raise ValueError('unregistered evaluator session')
            for record in validations:
                adapted=setup['definitions'].adapt(trace,record,provenance=setup['provenance'])
                if adapted['theta0'].key!=setup['theta0'][job['space']]['design'].key:
                    raise ValueError('validation changed matched theta0 settings')
                designs[adapted['design'].key]=adapted
                context={'run_id':key,'source_sha256':attempt['trace_sha256'],'method':job['method_letter'],
                         'space':job['space'],'start_index':job['start_index'],'repeat':job['repeat'],
                         'eligible_run':choice is not None,'host_contaminated':attempt['host_contaminated'],
                         'stop_reason':trace['stop_reason']}
                checked=adapt_validation(record,context=context,design=adapted['design'],
                    anchor=setup['effective_anchor'],unit_seconds=setup['unit_seconds'],budget_fe=setup['budget_fe'],
                    **setup['geometry_definitions'][job['space']])
                meshes=_mesh_bytes(reader,generation,attempt,record,capture['input_paths']['validation_mesh'])
                records.append(checked)
                row['validations'].append({'record_id':checked.record_id,'design_key':checked.design.key,'meshes':meshes})
    if not set(selected)<=observed:raise ValueError('selected completion absent from generations')
    a=setup['effective_anchor']
    selection=select(records,f0=a['F0'],mean0=a['mean0_full_current'],theta0=setup['theta0']['a']['design'],
                     theta0_references=[setup['theta0']['b']['design']])
    plan=check_plan.make_plan(selection,include_conforming=True)
    selected_designs={j['design_key']:copy.deepcopy(designs[j['design_key']]) for j in selection['jobs']}
    # Store JSON-compatible complete design identity for later native requests.
    for value in selected_designs.values():
        for k in ('design','theta0'):value[k]=value[k].canonical
    reader.stable()
    return {'schema':1,'capture_manifest_sha256':capture['manifest_sha256'],
            'source_commit':setup['provenance']['source_commit'],'selection':selection,'plan':plan,
            'designs':selected_designs,'attempts':audit,'geometry_definitions':setup['geometry_definitions'],
            'can_select':True,'can_launch':False,
            'scope':'Frozen cohort from verified closed study and recorded scientific evidence; no final refinement has run. Performance and publication claims require the full study analysis and completed refinements.'}


def load(root, *, manifest_sha256, git_repo, reviewed_commit, capture_tool_commit):
    capture=capture_archive.load(root,manifest_sha256=manifest_sha256,git_repo=git_repo,
                                 reviewed_commit=reviewed_commit,capture_tool_commit=capture_tool_commit)
    setup=setup_evidence.verify(capture,git_repo=git_repo)
    reader=capture_archive.Reader(root,manifest_sha256)
    starts=_bound_json(capture['input_paths']['starts'],capture['frozen']['inputs']['starts']['sha256'])
    with producer_source.committed_modules(git_repo,reviewed_commit,('ledger',)):
        result=_assemble(capture,setup,reader,ledger_type=importlib.import_module('ledger').Ledger,starts=starts)
    reader.stable()
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--capture',required=True,type=Path)
    parser.add_argument('--manifest-sha256',required=True)
    parser.add_argument('--git-repo',required=True,type=Path)
    parser.add_argument('--reviewed-commit',required=True)
    parser.add_argument('--capture-tool-commit',required=True)
    parser.add_argument('--out',required=True,type=Path)
    args=parser.parse_args()
    root=args.capture.resolve(strict=True);out=args.out.resolve()
    if out==root or root in out.parents or out.exists():
        parser.error('output must be a new file outside the captured archive')
    result=load(root,manifest_sha256=args.manifest_sha256,git_repo=args.git_repo,
                reviewed_commit=args.reviewed_commit,capture_tool_commit=args.capture_tool_commit)
    raw=(json.dumps(result,sort_keys=True,indent=2,allow_nan=False)+'\n').encode()
    with out.open('xb') as stream:stream.write(raw)
    print(json.dumps({'file':str(out),'sha256':hashlib.sha256(raw).hexdigest(),
                      'designs':len(result['designs']),'checks':len(result['plan']['jobs']),
                      'can_launch':False}))


if __name__=='__main__':main()
